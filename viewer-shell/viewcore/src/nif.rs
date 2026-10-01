//! NetImmerse 4.0.0.2 meshes: what the viewer draws, animates and collides with.
//!
//! Files are read by greatness7's `tes3::nif` ([`from_crate`]) into the per-record tables
//! (`Records`) that the draw, skin, particle and animation walks below use. A file the
//! crate refuses is read by the engine's own reader (`own_records`), which scans past
//! records it cannot decode, so a damaged mesh still shows what it can.

use crate::esp::{cstring, f32le, u32le};

mod from_crate;


/// Where an obstacle's shape came from, which is also how much to trust it.
#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
pub enum ColShape {
    /// A RootCollisionNode: what the engine itself collides with. Closed, so it is
    /// meaningful to ask whether a point is inside it.
    #[default]
    Hull,
    /// No hull, so the visible geometry stands in for one. Open: a single leaf plane
    /// overhead is one crossing, and asking "inside?" of that is meaningless.
    Visible,
    /// No collision at all — the mesh is marked NC, or nothing in it collides. A box
    /// half the size of its bounds stands where the object is, so that grass does
    /// not grow straight through it. Closed, being a box.
    Bounds,
}

#[derive(Default, Clone)]
pub struct MeshGeom {
    /// World-space-agnostic triangle soup of the collision shape, in object space.
    ///
    /// Shared rather than owned. One cell's obstacle index is rebuilt on every rescatter
    /// and every reference in it used to take a *copy* of its mesh's triangles: eight
    /// megabytes of memcpy for a cell of three hundred rocks, nine times over for a
    /// nine-cell view, all of it identical to the copy made the frame before. Behind an
    /// `Arc` the index costs a pointer per obstacle. Nothing writes to these after the
    /// reader has built them, which is what makes sharing them safe rather than merely
    /// cheap.
    pub tris: std::sync::Arc<Vec<[f32; 3]>>,
    pub aabb_min: [f32; 3],
    pub aabb_max: [f32; 3],
    /// Which of the three shapes `tris` is.
    pub shape: ColShape,
    /// Half the diagonal of the collision bounds. The size threshold is applied by
    /// the caller, not here, so that it can be a setting rather than a constant.
    pub radius: f32,
    /// Records the reader could not decode outright and had to find by scanning.
    /// Anything above zero means the record indices after that point are a guess,
    /// which is worth knowing when a mesh turns out to have no collision hull.
    pub scanned: usize,
}

struct Reader<'a> {
    b: &'a [u8],
    p: usize,
    /// The controller link of the last NiObjectNET read — the particle systems hang
    /// their NiParticleSystemController off it (round 17).
    last_ctrl: i32,
}

impl<'a> Reader<'a> {
    #[inline]
    fn u32(&mut self) -> u32 {
        let v = if self.p + 4 <= self.b.len() { u32le(self.b, self.p) } else { 0 };
        self.p += 4;
        v
    }
    #[inline]
    fn i32(&mut self) -> i32 {
        self.u32() as i32
    }
    #[inline]
    fn u16(&mut self) -> u16 {
        let v = if self.p + 2 <= self.b.len() {
            u16::from_le_bytes([self.b[self.p], self.b[self.p + 1]])
        } else {
            0
        };
        self.p += 2;
        v
    }
    #[inline]
    fn f32(&mut self) -> f32 {
        let v = if self.p + 4 <= self.b.len() { f32le(self.b, self.p) } else { 0.0 };
        self.p += 4;
        v
    }
    fn str_sized(&mut self) -> String {
        let n = self.u32() as usize;
        if self.p + n > self.b.len() {
            self.p = self.b.len();
            return String::new();
        }
        let s = cstring(&self.b[self.p..self.p + n]);
        self.p += n;
        s
    }
    #[inline]
    fn skip(&mut self, n: usize) {
        self.p += n;
    }
    #[inline]
    fn ok(&self) -> bool {
        self.p <= self.b.len()
    }
}

#[derive(Clone)]
struct Node {
    kind: String,
    name: String,
    flags: u16,
    trafo: [f32; 12], // 3x3 rotation (row major) + translation + scale packed
    scale: f32,
    children: Vec<i32>,
    /// Head of the extra-data chain, where Morrowind keeps its per-mesh markers.
    extra: i32,
    // NiTriShape payload
    data_ref: i32,
    /// NiSwitchNode only: which child is the live one.
    active_index: usize,
    /// Texturing, material and alpha properties. A shape inherits whatever its
    /// parents carry, so these have to be kept per node, not per shape.
    props: Vec<i32>,
    /// NiTriShape only: its NiSkinInstance, or -1. A skinned shape's vertices are in
    /// skin space and are put where the bones hold them by `skin_records` — see there.
    skin_ref: i32,
    /// Set by `skin_records` once the shape's data has been rewritten into the mesh's
    /// own space: the walks then apply no transform of their own to it.
    skinned: bool,
    /// The controller chain's head: a particle node's NiParticleSystemController.
    ctrl: i32,
    /// Round 18dl: the node's dynamic effects - the NiTextureEffect that puts an
    /// environment map on everything below (the Telvanni crystals, the bump-mapped pods).
    effects: Vec<i32>,
}

/// NiSkinInstance: the skin data, the skeleton root, and the bones in the data's order.
#[derive(Clone)]
struct SkinInst {
    data: i32,
    /// The skeleton root. Not needed for the bind pose — the bones' own transforms
    /// are accumulated from the file's root — but kept because the record has it.
    #[allow(dead_code)]
    root: i32,
    bones: Vec<i32>,
}

/// NiSkinData: the skin's own transform and, per bone, the transform from skin space
/// into that bone's space plus the vertices it moves and by how much.
#[derive(Clone)]
struct SkinData {
    /// The skin's own overall transform. Read so the record's layout stays right, and
    /// then ignored — see `skin_records`, and OpenMW's `RigGeometry` (round 17h).
    #[allow(dead_code)]
    trafo: ([f32; 12], f32),
    bones: Vec<SkinBone>,
}
#[derive(Debug, Clone)]
struct SkinBone {
    trafo: ([f32; 12], f32),
    weights: Vec<(u16, f32)>,
}

/// Round 18ag: what a skinned shape needs to be moved per frame rather than baked once —
/// the vertices and normals as the file stores them (skin space) and, per bone, the
/// vertices it claims and by how much. `skin_records` bakes the bind pose out of the same
/// numbers; this is the copy the page skins from every frame.
#[derive(Clone, Debug)]
struct SkinSrc {
    verts: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    /// The NiSkinInstance's bone nodes, in the skin data's order.
    bone_nodes: Vec<i32>,
    /// Per bone: its skin-to-bone transform and (vertex, weight) pairs.
    bones: Vec<SkinBone>,
}

/// Round 18ag: one track of a `.kf` file — the node it moves, by name, and its keys.
#[derive(Clone, Debug)]
pub struct KfTrack {
    pub node: String,
    pub ctrl: KfCtrl,
    pub data: KfData,
}

/// Round 18ag: a `.kf` file — Morrowind keeps an actor's animation beside its mesh, in a
/// file of the same name, and binds each track to a node of the mesh by *name*. The text
/// keys mark the clips ("Idle: Start" … "Idle: Stop"); the tracks are the bones' keys.
#[derive(Clone, Debug, Default)]
pub struct KfSequence {
    pub text_keys: Vec<(f32, String)>,
    pub tracks: Vec<KfTrack>,
}

impl KfSequence {
    /// The window of the clip called `name` — its `Start` and `Stop` text keys, or its
    /// `Loop Start`/`Loop Stop` when the clip has a loop inside it (a creature's idle
    /// often has a settle before the part that repeats). Case-insensitive, as the game
    /// reads them. `None` when the file has no such clip.
    pub fn clip(&self, name: &str) -> Option<(f32, f32)> {
        /* One key can carry several markers, one a line: the strider's key at 26.667 s
           reads `Idle: Stop\r\nIdle2: Start\r\nIdle2: Loop Start`. The game reads each
           line as a marker of its own; so does this. */
        let want = |tail: &str| -> Option<f32> {
            let full = format!("{}: {}", name, tail).to_ascii_lowercase();
            self.text_keys
                .iter()
                .find(|(_, k)| k.lines().any(|l| l.trim().to_ascii_lowercase() == full))
                .map(|(t, _)| *t)
        };
        let (start, stop) = (want("start")?, want("stop")?);
        let (ls, le) = (want("loop start"), want("loop stop"));
        let (a, b) = match (ls, le) {
            (Some(a), Some(b)) if b > a + 1e-4 => (a, b),
            _ => (start, stop),
        };
        if b > a + 1e-4 { Some((a, b)) } else { None }
    }

    /// The whole span the tracks key, for a file with no usable text keys.
    pub fn span(&self) -> Option<(f32, f32)> {
        let mut lo = f32::MAX;
        let mut hi = f32::MIN;
        for t in &self.tracks {
            for (k, _) in &t.data.rot { lo = lo.min(*k); hi = hi.max(*k); }
            for (k, _) in &t.data.trans { lo = lo.min(*k); hi = hi.max(*k); }
            for (k, _) in &t.data.scales { lo = lo.min(*k); hi = hi.max(*k); }
            if let Some(ch) = &t.data.rot_xyz {
                for c in ch { for (k, _) in c { lo = lo.min(*k); hi = hi.max(*k); } }
            }
        }
        if hi > lo + 1e-4 { Some((lo, hi)) } else { None }
    }

    /// How far a clip moves: the largest angle any track turns away from where it stands
    /// at the clip's start, in degrees. A banner's sway is 16°, its gale 39°; the
    /// strider's idle 57°, its second idle 94°.
    pub fn motion(&self, clip: (f32, f32)) -> f32 {
        let angle = |a: [f32; 4], b: [f32; 4]| -> f32 {
            let d = (a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3]).abs().min(1.0);
            2.0 * d.acos().to_degrees()
        };
        let mut worst = 0f32;
        for t in &self.tracks {
            let mut first: Option<[f32; 4]> = None;
            for (k, q) in &t.data.rot {
                if *k < clip.0 - 1e-4 || *k > clip.1 + 1e-4 {
                    continue;
                }
                match first {
                    None => first = Some(*q),
                    Some(f) => worst = worst.max(angle(f, *q)),
                }
            }
        }
        worst
    }

    /// The clip a parked actor plays: `Idle`, or failing that whatever the file keys.
    ///
    /// A creature's `Idle` is the standing-about it does between anything else — the
    /// strider's twenty-six seconds of swaying and feeling the air. A banner's `Idle` is
    /// *empty* (`Idle: Start` and `Idle: Stop` on the same key at 0): the still pose,
    /// hanging straight down. Its `Idle2` is the gentle sway — the root bone tilting
    /// sixteen degrees and back over two seconds — and its `Idle3` a gale, the cloth blown
    /// out to seventy-five degrees and whipping in a loop. The game shows the sway nearly
    /// all the time and the gale in a storm, so when `Idle` has no length the numbered
    /// idles are weighed by `motion` and **the one that moves least** is the one — every
    /// banner and flag in the archive puts that at `Idle2`, and a mod that numbers them
    /// the other way round still gets its calm one. Robin: "Is the motion on the banners
    /// the least amount there is among their animations? If it isn't go to an animation
    /// with lower amount." A file with no usable text keys plays end to end.
    pub fn idle(&self) -> Option<(f32, f32)> {
        if let Some(c) = self.clip("Idle") {
            return Some(c);
        }
        let mut best: Option<((f32, f32), f32)> = None;
        for n in 2..=9 {
            let Some(c) = self.clip(&format!("Idle{n}")) else { continue };
            let m = self.motion(c);
            // Strictly less: the first of two that move alike keeps its place.
            if best.map(|(_, bm)| m < bm).unwrap_or(true) {
                best = Some((c, m));
            }
        }
        if best.is_some() {
            return best.map(|(c, _)| c);
        }
        /* Round 18ak: markers but nothing to loop - Akulakhan's file is an empty `Idle`
           and a `Death1` - is a thing that stands still until something asks. Playing the
           whole span here had the dwemer god collapsing in a loop. Only a file with no
           markers at all plays end to end. */
        if self.text_keys.is_empty() { self.span() } else { None }
    }

    /// Round 18ak: when the still pose is - the `Idle: Start` marker's time, or the
    /// file's first instant. Where a thing that does not loop is posed.
    pub fn idle_time(&self) -> f32 {
        self.text_keys
            .iter()
            .find(|(_, k)| k.lines().any(|l| l.trim().eq_ignore_ascii_case("idle: start")))
            .map(|(t, _)| *t)
            .unwrap_or(0.0)
    }
}

/// One NiStringExtraData: the marker itself, and the next link in the chain.
#[derive(Clone)]
struct ExtraString {
    value: String,
    next: i32,
}

#[derive(Clone, Default)]
struct ShapeData {
    verts: Vec<[f32; 3]>,
    tris: Vec<[u16; 3]>,
    /// Kept for drawing only; the collision walk never looks at them.
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    /// The second UV set, when the shape has one — what a detail map reads (round 17h).
    /// The UV sets after the first, in order: set 1 at index 0 (round 17y - a Glow in the
    /// Dahrk window reads three: base, dark map, detail).
    uv_more: Vec<Vec<[f32; 2]>>,
    /// Per-vertex colours as bytes, RGB, when the shape has them; empty otherwise.
    /// Drawing only. Round 11 item 11: the game multiplies the texture by these, and a
    /// mesh drawn without them shows every baked shadow and tint missing.
    colours: Vec<[u8; 3]>,
    /// Round 18df: the vertex alpha, one byte a vertex, when any vertex has one below
    /// 255; empty otherwise (every vanilla shape). With the vertex colours as the colour
    /// source the fixed pipeline's fragment alpha is the texture's times *this*, and
    /// OAAB's `t_root_pillar_xl` fades both ends of the Telvanni root pillar through it.
    alphas: Vec<u8>,
}

/* ---- shared record fragments -------------------------------------------------
   Morrowind writes 4.0.0.2, so the version-dependent branches other readers need
   collapse to one shape. These mirror the page's reader field for field: two
   readers of the same format that disagree is a bug whichever one is wrong, and
   the only way to keep them honest is to write them the same way. */

impl<'a> Reader<'a> {
    /// NiObjectNET: name, extra-data ref, controller ref.
    fn object_net(&mut self) -> (String, i32) {
        let name = self.str_sized();
        // The extra-data chain carries NC ("nothing collides with this"), MRK and
        // RCN, so the link has to be kept rather than stepped over.
        let extra = self.i32();
        self.last_ctrl = self.i32(); // controller
        (name, extra)
    }
    /// NiAVObject, up to and including the bounding volume.
    fn av_object(&mut self) -> (String, u16, [f32; 12], f32, i32, Vec<i32>) {
        let (name, extra) = self.object_net();
        let flags = self.u16();
        // translation, then the 3x3, then scale — in that order
        let mut m = [0f32; 12];
        m[9] = self.f32();
        m[10] = self.f32();
        m[11] = self.f32();
        for v in m.iter_mut().take(9) {
            *v = self.f32();
        }
        let scale = self.f32();
        self.skip(12); // velocity, present up to 4.2.2.0
        let nprops = self.u32() as usize;
        if nprops > 4096 {
            self.p = self.b.len() + 1; // force the caller to give up on this record
            return (name, flags, m, scale, extra, Vec::new());
        }
        // Kept, not skipped: a shape's texture, colour and alpha all hang off this
        // list, and a shape inherits whatever its parents carry.
        let mut props = Vec::with_capacity(nprops);
        for _ in 0..nprops {
            props.push(self.i32());
        }
        if self.u32() != 0 {
            // bounding volume: type, centre, 3x3, extents
            self.skip(4 * 16);
        }
        (name, flags, m, scale, extra, props)
    }
    fn ref_list(&mut self) -> Vec<i32> {
        let n = self.u32() as usize;
        if n > 65536 {
            self.p = self.b.len() + 1;
            return Vec::new();
        }
        (0..n).map(|_| self.i32()).collect()
    }
    /// NiTimeController: next, flags, frequency, phase, start, stop, target.
    fn time_controller(&mut self) {
        let _ = self.time_controller_read();
    }
    /// The same fields, kept rather than stepped over (round 17m: the UV controller
    /// that scrolls a waterfall needs its clock).
    fn time_controller_read(&mut self) -> (i32, u16, f32, f32, f32, f32, i32) {
        let next = self.i32();
        let flags = self.u16();
        let frequency = self.f32();
        let phase = self.f32();
        let start = self.f32();
        let stop = self.f32();
        let target = self.i32();
        (next, flags, frequency, phase, start, stop, target)
    }
    /* The head of a key group: how many keys there are, and how many extra floats each
       key carries after its time and its value.

       Two rules, and being wrong about either desyncs the rest of the file rather than
       losing one animation:

         * **The interpolation kind is written only when there is at least one key.** An
           empty group is a bare count and nothing else. Reading the kind anyway takes four
           bytes out of the next record — round 18bg, bug hunt §G6.
         * **What a key carries depends on the kind and on how wide the value is.** LINEAR
           (1) carries nothing extra. QUADRATIC (2) adds a forward and a backward tangent,
           each as wide as the value: two floats for a float key, eight for a colour.
           TBC (3) adds tension, bias and continuity — three floats, whatever the width.

       `width` is the value's width in floats. Answers `None` only for a count so large the
       file cannot be holding it, having marked the reader spent.
       (Round 17m; round 18bg made it shared.) */
    fn key_group_head(&mut self, width: usize) -> Option<(usize, usize)> {
        let n = self.u32() as usize;
        if n > 65536 {
            self.p = self.b.len() + 1;
            return None;
        }
        if n == 0 {
            return Some((0, 0));
        }
        let extra = match self.u32() {
            2 => 2 * width,
            3 => 3,
            _ => 0,
        };
        Some((n, extra))
    }

    /* A NiFloatData-shaped key group. Only the time and the value are kept — Morrowind's
       UV animations are linear ramps, and reading a curve as a line is a small error where
       reading it as the wrong length is a broken file. */
    fn key_group_f32(&mut self) -> Vec<(f32, f32)> {
        let Some((n, extra)) = self.key_group_head(1) else { return Vec::new() };
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            let t = self.f32();
            let v = self.f32();
            self.skip(extra * 4);
            out.push((t, v));
        }
        out
    }
    /// A texture descriptor inside NiTexturingProperty. The source link, which UV set it
    /// reads (round 17h: a detail map usually has its own) and, since round 18df, its
    /// clamp mode: 0 clamp S and T, 1 clamp S wrap T, 2 wrap S clamp T, 3 wrap both - the
    /// default, and what 447 vanilla static shapes do not have: a road sign or a banner
    /// that reads its texture past the edge wraps a strip of the far side round unless it
    /// clamps.
    fn tex_desc(&mut self) -> (i32, u32, u32) {
        let source = self.i32();
        let clamp = self.u32();
        self.skip(4); // filter
        let uv_set = self.u32();
        self.skip(4); // PS2 L and K
        self.skip(2); // unknown, present up to 4.1.0.12
        (source, uv_set, clamp.min(3))
    }
}

/// What a shape needs to be drawn, beyond its geometry.
#[derive(Clone, Default)]
struct Materials {
    /// NiTexturingProperty -> the base texture's NiSourceTexture record.
    tex_source: Vec<Option<i32>>,
    /// NiTexturingProperty -> its *detail* map's NiSourceTexture and UV set (round 17h).
    /// The game's second texture stage multiplies the base by this and doubles it;
    /// meshes like `pc_flora_scumalpha_03.nif` keep all of their colour in it.
    detail_source: Vec<Option<(i32, u32)>>,
    /// NiTexturingProperty slot 4, the glow map, and the UV set it names (round 17p).
    glow_source: Vec<Option<(i32, u32)>>,
    /// Wraithguard: slot 3, the gloss map - a luminance mask on the specular highlight.
    /// Neither Morrowind nor OpenMW draws it; the mesh viewer's studio views do, as the
    /// old three.js viewer did.
    gloss_source: Vec<Option<(i32, u32)>>,
    /// NiTexturingProperty slot 1, the *dark* map, and its UV set (round 17y). A second
    /// stage that multiplies the base — Glow in the Dahrk's lit windows keep their colour
    /// in it: `tex02_2.dds` is the warm of a Nord window, `tex03_2.dds` the green of a
    /// Hlaalu one, over a grey gradient base. Drawn without it they were grey-white.
    dark_source: Vec<Option<(i32, u32)>>,
    /// NiTexturingProperty slot 6, the decal map, and its UV set (round 18df). A stage
    /// laid over the base by its own alpha - OpenMW's objects.frag: `mix(base.rgb,
    /// decal.rgb, decal.a)`. Thirteen OAAB and Tamriel Rebuilt shapes in the audit's
    /// sample carry one; none of vanilla does.
    decal_source: Vec<Option<(i32, u32)>>,
    /// NiTexturingProperty -> the base map's clamp mode (round 18df): 0 clamp both, 1
    /// clamp S/wrap T, 2 wrap S/clamp T, 3 wrap both.
    base_clamp: Vec<Option<u32>>,
    /// NiTexturingProperty -> the bump map on slot 5 (round 18dl): its source, UV set,
    /// luma scale and offset, and the 2x2 matrix that turns its texel into an offset on
    /// the environment map's coordinates. Nothing in vanilla but one Vurt tree; the fake
    /// bump-mapping mods (Telvanni Bump Maps) put one on every shape under a
    /// NiTextureEffect.
    bump_source: Vec<Option<(i32, u32, f32, f32, [f32; 4])>>,
    /// NiTexturingProperty -> the base map's UV set (round 18dj): which UV controller
    /// moves it. 0 on every shape surveyed; read rather than assumed.
    base_uv_set: Vec<Option<u32>>,
    /// NiSourceTexture -> the file it names.
    tex_file: Vec<Option<String>>,
    /// NiMaterialProperty -> diffuse colour, alpha, emissive colour and (round 18de) the
    /// ambient colour.
    material: Vec<Option<([f32; 3], f32, [f32; 3], [f32; 3])>>,
    /// NiMaterialProperty -> the head of its controller chain, or -1 (round 17y: where a
    /// NiAlphaController hangs).
    material_ctrl: Vec<i32>,
    /// NiAlphaProperty -> flags, and the test threshold when testing is on.
    alpha: Vec<Option<(u16, u8)>>,
    /// NiVertexColorProperty -> (vertex mode, lighting mode). Round 17w: lighting mode 0
    /// is "emissive" — the shape is drawn *unlit*, its colour the texture times the vertex
    /// colours and nothing else. The waterfalls are this, which is why the game draws them
    /// smooth and bright while a lit copy shows every facet of their normals.
    vcol_prop: Vec<Option<(u32, u32)>>,
    /// NiStencilProperty -> its draw mode: 0 default (counter-clockwise front, back faces
    /// culled), 1 counter-clockwise, 2 clockwise, 3 both sides. Round 17w: the game culls
    /// back faces on every shape unless one of these says otherwise, and so does OpenMW;
    /// a translucent sheet drawn from both sides is drawn twice where it folds.
    stencil: Vec<Option<u32>>,
    /// NiZBufferProperty -> its flags (round 18dg): bit 0 test, bit 1 write.
    zbuffer: Vec<Option<u16>>,
}

/// One record, either stepped over or read into `store` when it carries material.
/// Returns false when the type is not one we know how to walk, in which case the
/// caller falls back to scanning for the next record name.
///
/// Both jobs walk exactly the same bytes, so they are the same function: a reader
/// that skips and a reader that keeps, kept apart, drift.
///
/// Guessing where the next record begins works most of the time, but "most" is not
/// good enough: a 4.0.0.2 file carries no per-record length, so one bad guess
/// desynchronises every record after it — and since the collision hull is usually
/// written last, what gets lost is exactly the thing obstacle avoidance depends on.
fn skip_or_read(r: &mut Reader, ty: &str, mut store: Option<(&mut Materials, usize)>) -> bool {
    match ty {
        "NiTexturingProperty" => {
            let _ = r.object_net();
            r.u16(); // flags
            r.u32(); // apply mode
            let count = r.u32() as usize;
            if count > 32 {
                return false;
            }
            for i in 0..count {
                if r.u32() == 0 {
                    continue; // not in use
                }
                let (src, uv_set, clamp) = r.tex_desc();
                /* Slot 0 is the base map, slot 2 the detail map (round 17h) and slot 4
                   the glow map (round 17p) — OpenMW's `NiTexturingProperty::TextureType`
                   is BaseTexture 0, DarkTexture 1, DetailTexture 2, GlossTexture 3,
                   GlowTexture 4, BumpTexture 5, DecalTexture 6. Slot 1, the dark map, since
                   round 17y; slot 6, the decal, since 18df. Gloss and bump are not drawn
                   (Morrowind draws neither). */
                if i == 0 {
                    if let Some((m, at)) = store.as_mut() {
                        m.tex_source[*at] = Some(src);
                        m.base_clamp[*at] = Some(clamp);
                        m.base_uv_set[*at] = Some(uv_set);
                    }
                }
                if i == 1 {
                    if let Some((m, at)) = store.as_mut() {
                        m.dark_source[*at] = Some((src, uv_set));
                    }
                }
                if i == 2 {
                    if let Some((m, at)) = store.as_mut() {
                        m.detail_source[*at] = Some((src, uv_set));
                    }
                }
                if i == 3 {
                    if let Some((m, at)) = store.as_mut() {
                        m.gloss_source[*at] = Some((src, uv_set));
                    }
                }
                if i == 4 {
                    if let Some((m, at)) = store.as_mut() {
                        m.glow_source[*at] = Some((src, uv_set));
                    }
                }
                if i == 5 {
                    // Round 18dl: the bump map's luma scale and offset, and its 2x2 matrix.
                    let luma_scale = r.f32();
                    let luma_offset = r.f32();
                    let m = [r.f32(), r.f32(), r.f32(), r.f32()];
                    if let Some((m_, at)) = store.as_mut() {
                        m_.bump_source[*at] = Some((src, uv_set, luma_scale, luma_offset, m));
                    }
                }
                if i == 6 {
                    if let Some((m, at)) = store.as_mut() {
                        m.decal_source[*at] = Some((src, uv_set));
                    }
                }
            }
        }
        "NiSourceTexture" => {
            let _ = r.object_net();
            let external = r.b.get(r.p).copied().unwrap_or(0);
            r.p += 1;
            if external == 1 {
                let f = r.str_sized();
                if let Some((m, at)) = store.as_mut() {
                    m.tex_file[*at] = Some(f);
                }
            } else {
                r.p += 1; // unknown byte
                r.i32(); // NiPixelData ref
            }
            r.skip(12); // pixel layout, mipmaps, alpha format
            r.p += 1; // is static
        }
        "NiMaterialProperty" => {
            let _ = r.object_net();
            r.u16();
            /* Round 18de: the ambient colour, kept. The fixed pipeline lights a shape as
               `ambient * ambientLight + diffuse * diffuseLight + emissive`, and the two
               are different on 1,784 of Morrowind's own static shapes - the shipwrecks,
               the colony piers, the Daedric walls sit at 0.1 ambient under a white
               diffuse, which is what keeps them dark in shadow. It was stepped over, so
               the shader lit the ambient by the diffuse and every one of them was too
               bright in shade. Clamped like the rest. */
            let ambient = [r.f32().clamp(0.0, 1.0), r.f32().clamp(0.0, 1.0), r.f32().clamp(0.0, 1.0)];
            /* Clamped to 0..1 the way the game's fixed pipeline clamps them. RR Better
               Meshes' pot and basket carry 255.0 in every channel — a 0..255 export in a
               0..1 field — and drew pure white here while the game shows them fine
               (Robin's round 12 item 2, found in round 14). */
            let diffuse = [r.f32().clamp(0.0, 1.0), r.f32().clamp(0.0, 1.0), r.f32().clamp(0.0, 1.0)];
            r.skip(12); // specular
            /* Round 17m: the emissive colour, which is how a lit lantern is lit. The
               game adds it to the lighting outright (OpenMW's objects.frag:
               `lighting = diffuseColor*diffuseLight + ambientColor*ambientLight +
               emissionColor*emissiveMult`), so a shape with a bright emissive glows at
               midnight and one with a black emissive does not. Clamped like the
               diffuse, and for the same reason. */
            let emissive = [r.f32().clamp(0.0, 1.0), r.f32().clamp(0.0, 1.0), r.f32().clamp(0.0, 1.0)];
            r.f32(); // glossiness
            let alpha = r.f32().clamp(0.0, 1.0);
            if let Some((m, at)) = store.as_mut() {
                m.material[*at] = Some((diffuse, alpha, emissive, ambient));
                m.material_ctrl[*at] = r.last_ctrl;
            }
        }
        "NiAlphaProperty" => {
            let _ = r.object_net();
            let flags = r.u16();
            let threshold = r.b.get(r.p).copied().unwrap_or(0);
            r.p += 1;
            if let Some((m, at)) = store.as_mut() {
                m.alpha[*at] = Some((flags, threshold));
            }
        }
        "NiStencilProperty" => {
            let _ = r.object_net();
            r.u16();
            r.p += 1; // stencil enabled
            r.skip(24); // function, reference, mask, fail, z-fail, z-pass
            let draw_mode = r.u32();
            if let Some((m, at)) = store.as_mut() {
                m.stencil[*at] = Some(draw_mode);
            }
        }
        "NiZBufferProperty" => {
            /* Round 18dg: the flags are kept - bit 0 tests the depth, bit 1 writes it.
               Absent, a shape does both; 331 vanilla blended shapes (the kwama eggs, the
               magic targets) and the mods' pools and fountains carry flags 1, test only,
               which is what lets what is behind them draw after them. At 4.0.0.2 the
               record is the flags alone; the compare function comes in at 4.1. */
            let _ = r.object_net();
            let flags = r.u16();
            if let Some((m, at)) = store.as_mut() {
                m.zbuffer[*at] = Some(flags);
            }
        }
        "NiShadeProperty" | "NiWireframeProperty" | "NiDitherProperty" | "NiSpecularProperty" => {
            let _ = r.object_net();
            r.u16();
        }
        "NiVertexColorProperty" => {
            let _ = r.object_net();
            r.u16();
            let vmode = r.u32(); // 0 ignore, 1 emissive, 2 ambient+diffuse
            let lmode = r.u32(); // 0 emissive (unlit), 1 emissive+ambient+diffuse (lit)
            if let Some((m, at)) = store.as_mut() {
                m.vcol_prop[*at] = Some((vmode, lmode));
            }
        }
        "NiFogProperty" => {
            let _ = r.object_net();
            r.u16();
            r.skip(16); // depth, then the colour
        }
        "NiStringExtraData" => {
            // Decoded in the main loop instead, because its value is a marker we act on.
            return false;
        }
        "NiTextKeyExtraData" => {
            r.i32();
            r.u32();
            let n = r.u32() as usize;
            if n > 65536 {
                return false;
            }
            for _ in 0..n {
                r.f32();
                r.str_sized();
            }
        }
        "NiVertWeightsExtraData" => {
            r.i32();
            r.u32();
            let n = r.u16() as usize;
            r.skip(n * 4);
        }
        "NiUVController" => {
            r.time_controller();
            r.u16();
            r.i32();
        }
        /* A morph controller and its data — `NiGeomMorpherController` and `NiMorphData` —
         * are read whole in `read_records_kf` since round 18au (the moths' wingbeat).
         * Round 18r measured them here first, out of `MWSE - Nocturnal Moths`'
         * `moths_lntrn.NIF`: the controller is **31** bytes, not 30 - the time controller's
         * 26, the data reference, and a trailing byte (nif.xml's `Always Update`, dated
         * later than 4.0.0.2 by the notes and carried by this 4.0.0.2 file anyway), and
         * the data is a morph count, a vertex count, a `relativeTargets` byte, then per
         * morph a key count, an interpolation kind, that many keys and one vector per
         * vertex. Read short by one byte, every record after the controller was lost and
         * the mesh came back with no geometry at all. */
        "NiFlipController" => {
            /* Round 18df: slot, *start time*, delta, count, refs - the start time was
             * missing, so the delta (0.065 on OAAB's `water_sqflow256.nif`) was read as
             * the count, refused, and the file resynced by scanning for the next name.
             * Verified against that file: after the time controller's 26 bytes, 0, 0.0,
             * 0.065, 32, then 32 references. */
            r.time_controller();
            r.u32(); // texture slot
            r.f32(); // start time
            r.f32(); // delta
            let n = r.u32() as usize;
            if n > 65536 {
                return false;
            }
            r.skip(n * 4);
        }
        _ => return false,
    }
    r.ok()
}

/// NiKeyframeData at 4.0.0.2. Rotation keys first: a count, then (when there are any) a
/// type — 1 linear, 2 quadratic, 3 TBC, 4 XYZ. Quaternion keys are a time and four
/// floats, plus tension/bias/continuity for TBC; an XYZ rotation is one float (the
/// order, at this version) and then three float key groups. Then the translations, a
/// key group of vectors — quadratic ones carry two tangent vectors — and the scales, a
/// float key group. `None` when a count is absurd, so the caller resyncs.
fn read_keyframe_data(r: &mut Reader) -> Option<KfData> {
    let mut d = KfData::default();
    let nrot = r.u32() as usize;
    if nrot > 65536 {
        return None;
    }
    if nrot > 0 {
        let rt = r.u32();
        if rt == 4 {
            r.f32(); // the order
            let mut ch: [Vec<(f32, f32)>; 3] = Default::default();
            for c in ch.iter_mut() {
                *c = r.key_group_f32();
            }
            d.rot_xyz = Some(ch);
        } else {
            d.rot.reserve(nrot);
            for _ in 0..nrot {
                let t = r.f32();
                let q = [r.f32(), r.f32(), r.f32(), r.f32()];
                if rt == 3 {
                    r.skip(12);
                }
                d.rot.push((t, q));
            }
        }
    }
    let ntr = r.u32() as usize;
    if ntr > 65536 {
        return None;
    }
    if ntr > 0 {
        let interp = r.u32();
        d.trans.reserve(ntr);
        for _ in 0..ntr {
            let t = r.f32();
            let v = [r.f32(), r.f32(), r.f32()];
            match interp {
                2 => r.skip(24),
                3 => r.skip(12),
                _ => {}
            }
            d.trans.push((t, v));
        }
    }
    d.scales = r.key_group_f32();
    if !r.ok() {
        return None;
    }
    Some(d)
}

/// Finds where the next record starts, for stepping over one we cannot decode.
///
/// A 4.0.0.2 file is a bare sequence of records, each introduced by its class
/// name as a length-prefixed string, so an unknown record can be skipped by
/// looking for the next name rather than by knowing its length. Insisting on the
/// `Ni` prefix — plus the two odd names Morrowind uses — stops the scan locking
/// onto stray bytes that happen to look like a string inside vertex data.
/// Whether a record's type name — a length and that many plain characters — starts at `i`.
///
/// One reading, used both by `resync` and by the one record that has to *ask* whether it
/// has read enough (`NiGeomMorpherController`, below).
fn block_starts_at(b: &[u8], i: usize) -> bool {
    if i + 8 > b.len() {
        return false;
    }
    let n = u32le(b, i) as usize;
    if !(3..=40).contains(&n) || i + 4 + n > b.len() {
        return false;
    }
    let s = &b[i + 4..i + 4 + n];
    let named = s.starts_with(b"Ni") || s == b"RootCollisionNode" || s == b"AvoidNode";
    named && s.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'_')
}

/// Round 18be (G3): the scan starts one byte *past* the record we gave up on, never at
/// it.
///
/// The type name has already been consumed by the time this is called, so `r.p` is the
/// unknown record's own body and the next record's name can only begin after it. It used
/// to start at `r.p`, which mattered because most records this reader cannot measure are
/// NiObjectNET-derived and so begin with a length-prefixed *name*: a NiTextureEffect an
/// exporter left named `NiTextureEffect` matched `block_starts_at` on the spot, the scan
/// returned where it started, the reader read the record's name as the next record's
/// type — and since each turn of the loop spends one record slot, every index after it
/// was off by one. Children and shape data are referenced by index, so the mesh came
/// back with pieces missing; measured, `parse_draw` yielded 0 parts.
///
/// No valid start is lost by the step: a record with an empty body does not exist at
/// 4.0.0.2 — NiObjectNET alone is a name, an extra-data link and a controller link.
fn resync(r: &mut Reader) -> bool {
    let b = r.b;
    let mut i = r.p + 1;
    while i + 8 <= b.len() {
        if block_starts_at(b, i) {
            r.p = i;
            return true;
        }
        i += 1;
    }
    false
}

/// Parses a NIF and returns the collision geometry plus bounds.
///
/// Only the records that carry geometry are decoded. Everything else — the
/// texturing, material and alpha properties that make up most of a real mesh —
/// is stepped over by `resync`, because decoding them properly would mean
/// implementing most of the format to learn nothing we need.
/// The record table, decoded once. Both walks read from this.
/// A NiUVController and its NiUVData, kept whole so the renderer can run the clock
/// (round 17m).
///
/// Robin: "Ex_Vivec_waterfall_03 [...] All have different kinds of translucency and
/// movement in the meshes. Is that something we can get in our renderer too? All
/// information should be in the .nif files." The movement is this: a texture matrix
/// animated over time, which OpenMW builds as `scale(uScale,vScale) then translate
/// (uTrans,vTrans)` and hands to the texture unit (`components/nifosg/controller.cpp`,
/// `UVController::apply`). Every one of the meshes he named turned out to be a
/// NiTriShape under a NiBSAnimationNode with exactly this on it.
#[derive(Clone, Debug, Default)]
pub struct UvAnim {
    pub frequency: f32,
    pub phase: f32,
    pub start: f32,
    pub stop: f32,
    /// (time, value) pairs. Empty means the curve is absent, and the renderer leaves
    /// that channel alone — 0 for the offsets, 1 for the tilings.
    pub u_trans: Vec<(f32, f32)>,
    pub v_trans: Vec<(f32, f32)>,
    pub u_scale: Vec<(f32, f32)>,
    pub v_scale: Vec<(f32, f32)>,
}

impl UvAnim {
    /// Is there anything here worth animating? A controller whose four curves are all
    /// empty is a controller the renderer can forget about.
    pub fn moves(&self) -> bool {
        !(self.u_trans.is_empty()
            && self.v_trans.is_empty()
            && self.u_scale.is_empty()
            && self.v_scale.is_empty())
    }
}

/// Round 17y: a NiKeyframeController — the clock of a node that turns, slides or
/// grows on its own. `data` is the NiKeyframeData with the keys.
#[derive(Clone, Debug, Default)]
pub struct KfCtrl {
    pub next: i32,
    pub flags: u16,
    pub frequency: f32,
    pub phase: f32,
    pub start: f32,
    pub stop: f32,
    pub target: i32,
    pub data: i32,
}

/// Round 17y: NiKeyframeData — the keys a node's transform runs through. Rotations are
/// quaternion keys `(time, [w, x, y, z])`, or three Euler channels when the file says
/// XYZ; translations `(time, [x, y, z])`; scales `(time, s)`. Only the time and the
/// value are kept, whatever the interpolation — the effect sheets that use these are
/// slow steady turns, and a curve read as a line is a small error next to a record read
/// as the wrong length.
#[derive(Clone, Debug, Default)]
pub struct KfData {
    pub rot: Vec<(f32, [f32; 4])>,
    pub rot_xyz: Option<[Vec<(f32, f32)>; 3]>,
    pub trans: Vec<(f32, [f32; 3])>,
    pub scales: Vec<(f32, f32)>,
}

impl KfData {
    pub fn is_empty(&self) -> bool {
        self.rot.is_empty() && self.rot_xyz.is_none() && self.trans.is_empty() && self.scales.is_empty()
    }

    /// Wraithguard: only the keys a clip from `start` to `stop` plays - those inside it,
    /// and the last key before and the first after, so sampling at either end reads what
    /// it read with every key. Times are kept as they were.
    ///
    /// A `.kf` controller spans every clip in the file end to end, and `bind_kf` plays one
    /// of them. xbase_anim.kf is minutes of keys for every bone; carried whole, an NPC's
    /// payload repeated them for each bone of each skinned part, and a town's NPCs came to
    /// hundreds of megabytes for a few seconds of idle.
    pub fn window(&self, start: f32, stop: f32) -> KfData {
        fn cut<V: Clone>(keys: &[(f32, V)], start: f32, stop: f32) -> Vec<(f32, V)> {
            if keys.is_empty() {
                return Vec::new();
            }
            let first = keys.iter().rposition(|(t, _)| *t <= start).unwrap_or(0);
            let last = keys.iter().position(|(t, _)| *t >= stop).unwrap_or(keys.len() - 1);
            keys[first..=last.max(first)].to_vec()
        }
        KfData {
            rot: cut(&self.rot, start, stop),
            rot_xyz: self
                .rot_xyz
                .as_ref()
                .map(|ch| [cut(&ch[0], start, stop), cut(&ch[1], start, stop), cut(&ch[2], start, stop)]),
            trans: cut(&self.trans, start, stop),
            scales: cut(&self.scales, start, stop),
        }
    }
}

/// Round 17y: one animated node above a shape — the still transform from the previous
/// link (or the mesh's root) down to this node's parent, the node's own resting
/// transform (what a channel without keys keeps), and the controller with its keys.
/// The page composes `pre × animated(t)` link by link, then draws the shape's baked
/// geometry under the product.
#[derive(Clone, Debug)]
pub struct AnimLink {
    pub pre: ([f32; 12], f32),
    pub local: ([f32; 12], f32),
    pub ctrl: KfCtrl,
    pub data: KfData,
}

/// Round 17y: a NiAlphaController on the shape's material — the alpha fades the effect
/// sheets in and out as they rise.
#[derive(Clone, Debug, Default)]
pub struct AlphaAnim {
    pub flags: u16,
    pub frequency: f32,
    pub phase: f32,
    pub start: f32,
    pub stop: f32,
    pub keys: Vec<(f32, f32)>,
}

/// Round 17y: what moves about a shape — the chain of animated nodes above it and the
/// fading of its material. `None` on the vast majority of shapes.
#[derive(Clone, Debug, Default)]
pub struct PartAnim {
    pub nodes: Vec<AnimLink>,
    pub alpha: Option<AlphaAnim>,
    /// NiVisControllers on the shape or on any node above it: the part shows only while
    /// every one of them says visible. Their keys are (time, 0 or 1), held from key to
    /// key rather than blended - a lightning bolt's frames, one shape switched on at a
    /// time. The clock fields are the controller's; `keys` the NiVisData's.
    pub vis: Vec<AlphaAnim>,
}

impl PartAnim {
}

/// Round 18ag: one bone of a skinned shape under an animated skeleton — the animated
/// nodes from the root down to it (`chain`, as a part's own), the still transform from
/// the last of those down to the bone itself (`post`; the identity when the bone is the
/// last link), and the skin-to-bone transform from NiSkinData. The bone's matrix at time
/// `t` is `chain(t) × post × trafo`, and a vertex is the weighted sum of its bones'.
#[derive(Clone, Debug)]
pub struct BoneAnim {
    /// Wraithguard: the bone node's name, so a part's skin can be bound to another
    /// file's skeleton by name (`bind_to_skeleton`).
    pub name: String,
    pub chain: Vec<AnimLink>,
    pub post: ([f32; 12], f32),
    pub trafo: ([f32; 12], f32),
}

/// Round 18ag: a skinned shape as the page moves it every frame — the vertices and
/// normals in skin space, four bone slots and four weights a vertex (the rest of a
/// vertex's bones, on the rare vertex with more, folded into those by weight), and the
/// bones. `None` on every shape that is not skinned to an animated skeleton.
#[derive(Clone, Debug, Default)]
pub struct SkinAnim {
    pub pos: Vec<f32>,
    pub nrm: Vec<f32>,
    /// Four per vertex; 255 is an empty slot.
    pub idx: Vec<u8>,
    /// Four per vertex, summing to one where any bone claims the vertex.
    pub w: Vec<f32>,
    pub bones: Vec<BoneAnim>,
}

/// Round 18au: a NiGeomMorpherController — the clock of a shape whose vertices move
/// between morph targets: the moths' wingbeat. `data` is the NiMorphData with the keys.
#[derive(Clone, Debug, Default)]
pub struct MorphCtrl {
    pub next: i32,
    pub flags: u16,
    pub frequency: f32,
    pub phase: f32,
    pub start: f32,
    pub stop: f32,
    pub target: i32,
    pub data: i32,
}

/// Round 18au: NiMorphData as the file stores it — per morph its weight keys `(time,
/// weight)` and one vector per vertex. Morph 0 is the base pose; the rest are offsets
/// from it when `relative` is set (Morrowind's own files always are) and whole positions
/// otherwise.
#[derive(Clone, Debug, Default)]
pub struct MorphData {
    pub relative: bool,
    pub morphs: Vec<(Vec<(f32, f32)>, Vec<[f32; 3]>)>,
}

/// Round 18au: a morph-animated shape as the page moves it — the controller's clock and,
/// per target, its weight keys and its offsets already baked into the part's space (three
/// floats a vertex, rotated and scaled as the positions were, never translated). The
/// page's vertex at time `t` is `pos + Σ weightᵢ(t) · offsetsᵢ`, which is OpenMW's
/// `MorphGeometry`. `None` on every shape that has no NiGeomMorpherController.
#[derive(Clone, Debug, Default)]
pub struct MorphAnim {
    pub flags: u16,
    pub frequency: f32,
    pub phase: f32,
    pub start: f32,
    pub stop: f32,
    pub targets: Vec<MorphTarget>,
}

#[derive(Clone, Debug, Default)]
pub struct MorphTarget {
    pub keys: Vec<(f32, f32)>,
    pub offsets: Vec<f32>,
}

struct Records {
    /// Round 17y: NiKeyframeController and NiKeyframeData by record index.
    kfctrls: Vec<Option<KfCtrl>>,
    kfdatas: Vec<Option<KfData>>,
    /// Round 18au: NiGeomMorpherController and NiMorphData by record index.
    morphctrls: Vec<Option<MorphCtrl>>,
    morphdatas: Vec<Option<MorphData>>,
    /// Round 18ag: a `.kf` file's root — NiSequenceStreamHelper's extra-data chain and
    /// controller chain — and the NiTextKeyExtraData records' keys, by record index.
    seq: Option<(i32, i32)>,
    textkeys: Vec<Option<Vec<(f32, String)>>>,
    /// Round 18ag: set once a `.kf`'s tracks have been bound to this file's nodes, so
    /// `skin_records` knows to keep what a skinned shape needs to move.
    skel_animated: bool,
    /// Round 18ag: a skinned shape's data as the file stores it — skin space, with each
    /// bone's weights — by data index, kept only under an animated skeleton.
    skin_src: Vec<Option<SkinSrc>>,
    /// Round 17y: NiAlphaController by record index — (next, flags, frequency, phase,
    /// start, stop, target, data) — and NiFloatData's keys.
    alphactrls: Vec<Option<(i32, u16, f32, f32, f32, f32, i32, i32)>>,
    floatdatas: Vec<Option<Vec<(f32, f32)>>>,
    /// Round 17y: the lights a mesh carries, by record index — diffuse colour and dimmer.
    /// The node itself (position, scale) is in `nodes` under the same index.
    lights: Vec<Option<([f32; 3], f32)>>,
    /// Round 18dl: NiTextureEffect by record index - the environment map's source
    /// texture and its clamp mode, for the sphere-mapped ones.
    texeffects: Vec<Option<(i32, u32)>>,
    nodes: Vec<Option<Node>>,
    datas: Vec<Option<ShapeData>>,
    extras: Vec<Option<ExtraString>>,
    types: Vec<String>,
    mats: Materials,
    skins: Vec<Option<SkinInst>>,
    skin_datas: Vec<Option<SkinData>>,
    scanned: usize,
    /// Round 17: the particle systems' records, by index.
    pctrls: Vec<Option<ParticleCtrl>>,
    pdatas: Vec<Option<ParticleData>>,
    pmods: Vec<Option<ParticleMod>>,
    colour_data: Vec<Option<Vec<[f32; 5]>>>,
    /// Round 17m: NiUVController by record index — (next controller, target, data ref,
    /// frequency, phase, start, stop) — and the NiUVData it points at.
    /// Round 18dj: and the texture set the controller names, last - which slots it
    /// drives (see `uv_anims_of`).
    uvctrls: Vec<Option<(i32, i32, i32, f32, f32, f32, f32, u32)>>,
    uvdatas: Vec<Option<UvAnim>>,
    /// NiVisController by record index (the keyframe controller's shape: clock, target,
    /// data) and NiVisData's keys as (time, 0 or 1).
    visctrls: Vec<Option<KfCtrl>>,
    visdatas: Vec<Option<Vec<(f32, f32)>>>,
}

/* ---- particles (round 17) --------------------------------------------------------
   The weather's clouds of ash, blight and snow are NiAutoNormalParticles under
   NiBSParticleNodes, each with a NiParticleSystemController and a chain of modifiers
   (NiGravity, NiParticleGrowFade, NiParticleColorModifier). Read here as the numbers
   they are; the viewport runs them (27_weather.js). The layouts are 4.0.0.2's, checked
   against `meshes\ashcloud.nif` and `blightcloud.nif`. */

/// NiParticleSystemController, the fields that drive the system.
#[derive(Clone, Debug)]
pub struct ParticleCtrl {
    pub flags: u16,
    pub frequency: f32,
    pub start: f32,
    pub stop: f32,
    pub target: i32,
    pub velocity: f32,
    pub velocity_random: f32,
    pub vertical_dir: f32,
    pub vertical_angle: f32,
    pub horizontal_dir: f32,
    pub horizontal_angle: f32,
    pub colour: [f32; 4],
    pub size: f32,
    pub emit_start: f32,
    pub emit_stop: f32,
    pub emit_rate: f32,
    pub lifetime: f32,
    pub lifetime_random: f32,
    pub emit_flags: u16,
    pub offset_random: [f32; 3],
    pub emitter: i32,
    pub num_particles: u16,
    pub num_valid: u16,
    pub modifier: i32,
    pub collider: i32,
}

/// NiAutoNormalParticlesData / NiRotatingParticlesData: what the vertices carry.
#[derive(Clone, Debug)]
pub struct ParticleData {
    pub nverts: usize,
    pub radius: f32,
    pub sizes: Vec<f32>,
    pub colours: Vec<[f32; 4]>,
    pub rotating: bool,
}

/// One link of a controller's modifier chain.
#[derive(Clone, Debug)]
pub enum ParticleMod {
    Gravity { next: i32, decay: f32, force: f32, kind: u32, position: [f32; 3], direction: [f32; 3] },
    GrowFade { next: i32, grow: f32, fade: f32 },
    Colour { next: i32, data: i32 },
    Rotation { next: i32, random_axis: bool, axis: [f32; 3], speed: f32 },
    Other { next: i32 },
}

fn read_particle_ctrl(r: &mut Reader) -> Option<ParticleCtrl> {
    r.i32(); // next controller
    let flags = r.u16();
    let frequency = r.f32();
    r.f32(); // phase
    let start = r.f32();
    let stop = r.f32();
    let target = r.i32();
    let velocity = r.f32();
    let velocity_random = r.f32();
    let vertical_dir = r.f32();
    let vertical_angle = r.f32();
    let horizontal_dir = r.f32();
    let horizontal_angle = r.f32();
    r.skip(12); // normal
    let colour = [r.f32(), r.f32(), r.f32(), r.f32()];
    let size = r.f32();
    let emit_start = r.f32();
    let emit_stop = r.f32();
    r.p += 1; // unknown byte
    let emit_rate = r.f32();
    let lifetime = r.f32();
    let lifetime_random = r.f32();
    let emit_flags = r.u16();
    let offset_random = [r.f32(), r.f32(), r.f32()];
    let emitter = r.i32();
    r.u16(); // unknown short
    r.f32(); // unknown float
    r.u32(); // unknown int
    r.u32(); // unknown int
    r.u16(); // unknown short
    let num_particles = r.u16();
    let num_valid = r.u16();
    if num_particles > 20000 {
        return None;
    }
    // Each particle: velocity, an unknown vector, lifetime, lifespan, timestamp, an
    // unknown short, the vertex id — 40 bytes.
    r.skip(num_particles as usize * 40);
    r.i32(); // unknown link
    let modifier = r.i32();
    let collider = r.i32();
    r.p += 1; // trailer
    if !r.ok() {
        return None;
    }
    Some(ParticleCtrl {
        flags, frequency, start, stop, target, velocity, velocity_random, vertical_dir, vertical_angle,
        horizontal_dir, horizontal_angle, colour, size, emit_start, emit_stop, emit_rate, lifetime,
        lifetime_random, emit_flags, offset_random, emitter, num_particles, num_valid, modifier, collider,
    })
}

fn read_particle_data(r: &mut Reader, rotating: bool) -> Option<ParticleData> {
    let nverts = r.u16() as usize;
    if nverts > 200_000 {
        return None;
    }
    if r.u32() != 0 {
        r.skip(nverts * 12);
    }
    if r.u32() != 0 {
        r.skip(nverts * 12);
    }
    r.skip(16); // bounding sphere
    let mut colours = Vec::new();
    if r.u32() != 0 {
        colours.reserve(nverts);
        for _ in 0..nverts {
            colours.push([r.f32(), r.f32(), r.f32(), r.f32()]);
        }
    }
    let nuv = (r.u16() as usize) & 0x3f;
    let has_uv = r.u32();
    if has_uv != 0 {
        r.skip(nverts * 8 * nuv);
    }
    let _num_particles = r.u16();
    let radius = r.f32();
    // Measured out of ashcloud.nif: after the radius comes the active count (a
    // ushort, 8 where the controller says 8 valid) and only then the has-sizes flag.
    r.u16();
    let mut sizes = Vec::new();
    if r.u32() != 0 {
        sizes.reserve(nverts);
        for _ in 0..nverts {
            sizes.push(r.f32());
        }
    }
    if rotating && r.u32() != 0 {
        r.skip(nverts * 16); // quaternions
    }
    if !r.ok() {
        return None;
    }
    Some(ParticleData { nverts, radius, sizes, colours, rotating })
}

/// A NIF transform as the skin records spell it: rotation first, then translation,
/// then scale — the other order from NiAVObject's, which is translation first.
fn read_skin_trafo(r: &mut Reader) -> ([f32; 12], f32) {
    let mut m = [0f32; 12];
    for v in m.iter_mut().take(9) {
        *v = r.f32();
    }
    m[9] = r.f32();
    m[10] = r.f32();
    m[11] = r.f32();
    let s = r.f32();
    (m, s)
}

/// Applies `(m, s)` to a point.
fn apply(m: &[f32; 12], s: f32, v: [f32; 3]) -> [f32; 3] {
    [
        (m[0] * v[0] + m[1] * v[1] + m[2] * v[2]) * s + m[9],
        (m[3] * v[0] + m[4] * v[1] + m[5] * v[2]) * s + m[10],
        (m[6] * v[0] + m[7] * v[1] + m[8] * v[2]) * s + m[11],
    ]
}
/// Rotates a direction by `m` (no translation, no scale).
fn rotate(m: &[f32; 12], v: [f32; 3]) -> [f32; 3] {
    [
        m[0] * v[0] + m[1] * v[1] + m[2] * v[2],
        m[3] * v[0] + m[4] * v[1] + m[5] * v[2],
        m[6] * v[0] + m[7] * v[1] + m[8] * v[2],
    ]
}

/// Puts every skinned shape where its bones hold it — round 12 item 3.
///
/// Robin: "Most flags are rotated wrong. They should be vertical but are horizontal."
/// Every vanilla banner is a *skinned* mesh: its NiTriShape carries a NiSkinInstance,
/// and its vertices are stored in the skin's own space rather than the node's, to be
/// placed by the bones at run time (the `x…` skeleton files animate them; the base
/// file holds the rest pose). Drawn through the node transform alone, the cloth lay
/// on its side. This rewrites the shape's data in the mesh's own space, once, at the
/// bind pose:
///
/// ```text
///   v' = Σ_i w_i · World(bone_i) · BoneTrafo_i · v
/// ```
///
/// where `World(bone)` is the bone node's transform accumulated from the file's root
/// (the same product the walks build) and `BoneTrafo_i` is NiSkinData's per-bone
/// transform, which takes a skin-space vertex into that bone's space. The skin's own
/// transform in NiSkinData is applied first when the file states one that is not the
/// identity. A shape marked `skinned` is then drawn and collided with as it is, with no
/// node transform on top — both walks check the flag.
/// Round 18ag: the switched-on NiKeyframeController that moves node `idx`, with its
/// keys — a bound `.kf` track (`bind_kf`) or the file's own — or `None`. What
/// `draw_parts` hangs a link on and what `skin_records` poses a bone by.
fn kf_for(rec: &Records, idx: i32) -> Option<(KfCtrl, KfData)> {
    for c in rec.kfctrls.iter().flatten() {
        if c.target != idx || c.flags & 0x0008 == 0 {
            continue;
        }
        if let Some(Some(d)) = rec.kfdatas.get(c.data.max(0) as usize) {
            if !d.is_empty() {
                return Some((c.clone(), d.clone()));
            }
        }
    }
    None
}

/// The value of a key list at `t`, blended between the keys either side and held at
/// the ends - the page's `keyAt`, for the one frame the engine needs.
fn key_at<const N: usize>(keys: &[(f32, [f32; N])], t: f32) -> Option<[f32; N]> {
    let first = keys.first()?;
    let last = keys.last()?;
    if t <= first.0 {
        return Some(first.1);
    }
    if t >= last.0 {
        return Some(last.1);
    }
    for w in keys.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        if t <= b.0 {
            let span = b.0 - a.0;
            let f = if span > 1e-9 { (t - a.0) / span } else { 0.0 };
            let mut out = a.1;
            for k in 0..N {
                out[k] = a.1[k] + (b.1[k] - a.1[k]) * f;
            }
            return Some(out);
        }
    }
    Some(last.1)
}

/// The same for a list of scalar keys.
fn key1_at(keys: &[(f32, f32)], t: f32) -> Option<f32> {
    let wrapped: Vec<(f32, [f32; 1])> = keys.iter().map(|(k, v)| (*k, [*v])).collect();
    key_at(&wrapped, t).map(|v| v[0])
}

/// A unit quaternion `[w, x, y, z]` as the row-major 3x3 the transforms here use.
fn quat_mat3(q: [f32; 4]) -> [f32; 9] {
    let l = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt().max(1e-9);
    let (w, x, y, z) = (q[0] / l, q[1] / l, q[2] / l, q[3] / l);
    [
        1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y - w * z), 2.0 * (x * z + w * y),
        2.0 * (x * y + w * z), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - w * x),
        2.0 * (x * z - w * y), 2.0 * (y * z + w * x), 1.0 - 2.0 * (x * x + y * y),
    ]
}

/// Three Euler channels (radians about x, y, z, applied z·y·x as the format has it).
fn euler_mat3(rx: f32, ry: f32, rz: f32) -> [f32; 9] {
    let (cx, sx, cy, sy, cz, sz) = (rx.cos(), rx.sin(), ry.cos(), ry.sin(), rz.cos(), rz.sin());
    let x = [1., 0., 0., 0., cx, -sx, 0., sx, cx];
    let y = [cy, 0., sy, 0., 1., 0., -sy, 0., cy];
    let z = [cz, -sz, 0., sz, cz, 0., 0., 0., 1.];
    let m = |a: &[f32; 9], b: &[f32; 9]| -> [f32; 9] {
        let mut o = [0f32; 9];
        for r in 0..3 {
            for c in 0..3 {
                o[r * 3 + c] = a[r * 3] * b[c] + a[r * 3 + 1] * b[3 + c] + a[r * 3 + 2] * b[6 + c];
            }
        }
        o
    };
    m(&m(&z, &y), &x)
}

/// Round 18ag: a node's transform at the first instant of its clip - its resting one
/// with each keyed channel replaced by the keys' value at `ctrl.start`. The page's
/// `animNodeAt` at t = start, and the frame the still picture is baked from.
///
/// Robin: "teamod\\TMT_Banner_TeaShop.nif is rendered horizontally, but it should be
/// vertical. Other flags also did this earlier." That banner's skeleton is the vanilla
/// one turned 90° about x at rest - an export's y-up - and its bones carry the keys that
/// stand it up: a key *replaces* the node's rotation, so the game never shows the rest
/// pose at all. Baking the bind pose from the rest transform showed exactly what the
/// game never does.
fn keyed_pose(local: &[f32; 12], scale: f32, ctrl: &KfCtrl, data: &KfData) -> ([f32; 12], f32) {
    keyed_pose_at(local, scale, ctrl, data, ctrl.start)
}

/// [`keyed_pose`] at an arbitrary time, which is what composing a chain needs.
fn keyed_pose_at(local: &[f32; 12], scale: f32, ctrl: &KfCtrl, data: &KfData, t: f32) -> ([f32; 12], f32) {
    let _ = ctrl;
    let mut m = *local;
    let mut s = scale;
    if let Some(q) = key_at(&data.rot, t) {
        m[..9].copy_from_slice(&quat_mat3(q));
    } else if let Some(ch) = &data.rot_xyz {
        let e: Vec<f32> = ch.iter().map(|c| key1_at(c, t).unwrap_or(0.0)).collect();
        m[..9].copy_from_slice(&euler_mat3(e[0], e[1], e[2]));
    }
    if let Some(v) = key_at(&data.trans, t) {
        m[9..12].copy_from_slice(&v);
    }
    if let Some(v) = key1_at(&data.scales, t) {
        s = v;
    }
    (m, s)
}

fn skin_records(rec: &mut Records) {
    let ident = [1., 0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 0.];
    // The parent of every node, so a bone's world transform can be built upward.
    let n = rec.nodes.len();
    let mut parent: Vec<i32> = vec![-1; n];
    for (i, node) in rec.nodes.iter().enumerate() {
        if let Some(nd) = node {
            for &c in &nd.children {
                if c >= 0 && (c as usize) < n && parent[c as usize] < 0 {
                    parent[c as usize] = i as i32;
                }
            }
        }
    }
    let world_of = |idx: i32| -> Option<([f32; 12], f32)> {
        let mut chain = Vec::new();
        let mut at = idx;
        let mut guard = 0;
        while at >= 0 && (at as usize) < n && guard < 128 {
            chain.push(at as usize);
            at = parent[at as usize];
            guard += 1;
        }
        let mut m = ident;
        let mut s = 1.0;
        for &i in chain.iter().rev() {
            let nd = rec.nodes[i].as_ref()?;
            /* Round 18ag: a node with keys stands where its keys put it at the clip's
               start, not where its resting transform does - see `keyed_pose`. The root
               is never posed, as its transform is never applied. */
            let (nm, ns) = match if i != 0 { kf_for(rec, i as i32) } else { None } {
                Some((ctrl, data)) => {
                    let (lm, ls) = keyed_pose(&nd.trafo, nd.scale, &ctrl, &data);
                    mul(&m, s, &lm, ls)
                }
                None => node_trafo(i as i32, &m, s, nd),
            };
            m = nm;
            s = ns;
        }
        Some((m, s))
    };

    let mut jobs: Vec<(usize, usize, Vec<([f32; 12], f32)>, SkinData, Vec<i32>)> = Vec::new();
    for (i, node) in rec.nodes.iter().enumerate() {
        let Some(nd) = node else { continue };
        if !is_shape(&nd.kind) || nd.skin_ref < 0 || nd.data_ref < 0 {
            continue;
        }
        let Some(Some(inst)) = rec.skins.get(nd.skin_ref as usize) else { continue };
        if inst.data < 0 {
            continue;
        }
        let Some(Some(sd)) = rec.skin_datas.get(inst.data as usize) else { continue };
        if sd.bones.len() != inst.bones.len() || sd.bones.is_empty() {
            continue;
        }
        let mut bone_ms = Vec::with_capacity(sd.bones.len());
        let mut ok = true;
        for (b, &bref) in inst.bones.iter().enumerate() {
            let Some((wm, ws)) = world_of(bref) else { ok = false; break };
            let (m, s) = mul(&wm, ws, &sd.bones[b].trafo.0, sd.bones[b].trafo.1);
            bone_ms.push((m, s));
        }
        if !ok {
            continue;
        }
        jobs.push((i, nd.data_ref as usize, bone_ms, sd.clone(), inst.bones.clone()));
    }

    let animated = rec.skel_animated;
    for (node_i, data_i, bone_ms, sd, bone_nodes) in jobs {
        let Some(Some(d)) = rec.datas.get(data_i) else { continue };
        let nv = d.verts.len();
        /* Round 18ag: under an animated skeleton the bind pose below is only the still
           picture; the page moves the shape itself, from the skin-space vertices and the
           bones' weights, so those are kept beside it. */
        if animated {
            rec.skin_src[data_i] = Some(SkinSrc {
                verts: d.verts.clone(),
                normals: if d.normals.len() == nv { d.normals.clone() } else { Vec::new() },
                bone_nodes,
                bones: sd.bones.clone(),
            });
        }
        let mut pos = vec![[0f32; 3]; nv];
        let mut nrm = vec![[0f32; 3]; nv];
        let mut wsum = vec![0f32; nv];
        let has_n = d.normals.len() == nv;
        /* NiSkinData's *overall* transform is deliberately not applied (round 17h).
         *
         * Robin: "x\Ex_V_ban_vivec_01.NIF always placed a bit too far down compared to
         * their position in the game." His install has Project Atlas, whose version of
         * that banner states one — (32, 0, -128) — while the vanilla file's is identity.
         * Applying it dropped the Atlas banner 128 units below the vanilla one on the
         * same reference, which is the offset he was seeing. OpenMW's `RigGeometry`
         * builds its bone matrices from `boneWorld * bones[i].trafo` alone and never
         * touches `NiSkinData::trafo`, so the game and OpenMW draw the two versions of
         * the banner in the same place and this now does too. */
        let (st, ss) = (ident, 1.0f32);
        let skin_ident = true;
        for (b, bone) in sd.bones.iter().enumerate() {
            let (m, s) = bone_ms[b];
            for &(vi, w) in &bone.weights {
                let vi = vi as usize;
                if vi >= nv || w == 0.0 {
                    continue;
                }
                let v0 = if skin_ident { d.verts[vi] } else { apply(&st, ss, d.verts[vi]) };
                let p = apply(&m, s, v0);
                for k in 0..3 {
                    pos[vi][k] += w * p[k];
                }
                if has_n {
                    let n0 = if skin_ident { d.normals[vi] } else { rotate(&st, d.normals[vi]) };
                    let q = rotate(&m, n0);
                    for k in 0..3 {
                        nrm[vi][k] += w * q[k];
                    }
                }
                wsum[vi] += w;
            }
        }
        // A vertex no bone claims stays where it was; the walks would otherwise move
        // it and the rest not, so it is left in place rather than dropped to the origin.
        let mut out = d.clone();
        for vi in 0..nv {
            if wsum[vi] > 0.0 {
                let inv = 1.0 / wsum[vi];
                out.verts[vi] = [pos[vi][0] * inv, pos[vi][1] * inv, pos[vi][2] * inv];
                if has_n {
                    let q = nrm[vi];
                    let l = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2]).sqrt();
                    if l > 1e-6 {
                        out.normals[vi] = [q[0] / l, q[1] / l, q[2] / l];
                    }
                }
            }
        }
        rec.datas[data_i] = Some(out);
        if let Some(Some(nd)) = rec.nodes.get_mut(node_i) {
            nd.skinned = true;
        }
    }
}

fn read_records(buf: &[u8]) -> Option<Records> {
    read_records_kf(buf, None)
}

/// Reads every mesh with the engine's own reader, skipping the crate: for comparing the
/// two readers' output (see `set_own_reader_only`).
static OWN_READER_ONLY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Test hook: read meshes with the engine's own reader only (`true`) or with the crate
/// first (`false`, the default).
#[doc(hidden)]
pub fn set_own_reader_only(own: bool) {
    OWN_READER_ONLY.store(own, std::sync::atomic::Ordering::Relaxed);
}

/// The records, with a `.kf`'s tracks bound to the nodes when one is given (round 18ag).
///
/// Read by greatness7's `tes3::nif` (`from_crate`); a file the crate refuses - an
/// unknown record, a truncated or damaged one - is read by the engine's own reader
/// (`own_records`), which scans past what it cannot decode.
fn read_records_kf(buf: &[u8], kf: Option<&KfSequence>) -> Option<Records> {
    read_records_opts(buf, kf, false)
}

/// `read_records_kf`; with `keep_skin`, every skinned shape keeps its skin-space data and
/// bones as under an animated skeleton, whether or not anything here moves them.
fn read_records_opts(buf: &[u8], kf: Option<&KfSequence>, keep_skin: bool) -> Option<Records> {
    let crate_read = if OWN_READER_ONLY.load(std::sync::atomic::Ordering::Relaxed) { None } else { from_crate::crate_records(buf) };
    let mut rec = crate_read.or_else(|| own_records(buf))?;
    if let Some(kf) = kf {
        bind_kf(&mut rec, kf);
    }
    /* Round 18ag: a skeleton is animated whether its keys came from the `.kf` beside
       the mesh or were saved into the file itself - the tea shop's plain banner carries
       the vanilla controllers on its bones. Either way a skinned shape under it is moved
       by the page and its still picture is baked from the keys' first frame. */
    if keep_skin {
        rec.skel_animated = true;
    }
    if !rec.skel_animated {
        rec.skel_animated = rec.kfctrls.iter().flatten().any(|c| {
            c.flags & 0x0008 != 0
                && c.target >= 0
                && rec.kfdatas.get(c.data.max(0) as usize).map(|d| d.as_ref().map(|d| !d.is_empty()).unwrap_or(false)).unwrap_or(false)
        });
    }
    skin_records(&mut rec);
    Some(rec)
}

impl Records {
    /// Empty tables for `n` records.
    fn empty(n: usize) -> Records {
        Records {
            kfctrls: vec![None; n],
            kfdatas: vec![None; n],
            morphctrls: vec![None; n],
            morphdatas: vec![None; n],
            seq: None,
            textkeys: vec![None; n],
            skel_animated: false,
            skin_src: vec![None; n],
            alphactrls: vec![None; n],
            floatdatas: vec![None; n],
            lights: vec![None; n],
            texeffects: vec![None; n],
            nodes: vec![None; n],
            datas: vec![None; n],
            extras: vec![None; n],
            types: vec![String::new(); n],
            mats: Materials {
                tex_source: vec![None; n],
                detail_source: vec![None; n],
                glow_source: vec![None; n],
                gloss_source: vec![None; n],
                dark_source: vec![None; n],
                decal_source: vec![None; n],
                base_clamp: vec![None; n],
                bump_source: vec![None; n],
                base_uv_set: vec![None; n],
                tex_file: vec![None; n],
                material: vec![None; n],
                material_ctrl: vec![-1; n],
                alpha: vec![None; n],
                vcol_prop: vec![None; n],
                stencil: vec![None; n],
                zbuffer: vec![None; n],
            },
            skins: vec![None; n],
            skin_datas: vec![None; n],
            scanned: 0,
            pctrls: vec![None; n],
            pdatas: vec![None; n],
            pmods: vec![None; n],
            colour_data: vec![None; n],
            uvctrls: vec![None; n],
            uvdatas: vec![None; n],
            visctrls: vec![None; n],
            visdatas: vec![None; n],
        }
    }
}

/// The engine's own reader: tolerant of damaged files, which it reads as far as it can.
fn own_records(buf: &[u8]) -> Option<Records> {
    if buf.len() < 64 {
        return None;
    }
    // Header: "NetImmerse File Format, Version 4.0.0.2\n" then version u32,
    // then record count u32, then a type-string table.
    let nl = buf.iter().position(|&c| c == b'\n')?;
    let mut r = Reader { b: buf, p: nl + 1, last_ctrl: -1 };
    let _version = r.u32();
    let num_records = r.u32() as usize;
    if num_records == 0 || num_records > 200_000 {
        return None;
    }

    // How many records had to be found by scanning rather than decoded outright.
    // Non-zero means the file used something we do not know the length of, and any
    // index after that point is a guess.
    let mut scanned = 0usize;
    // One look at the environment for the whole file, not one per record.
    let dbg = std::env::var_os("GDN_NIF_DEBUG").is_some();
    let mut nodes: Vec<Option<Node>> = vec![None; num_records];
    let mut datas: Vec<Option<ShapeData>> = vec![None; num_records];
    let mut extras: Vec<Option<ExtraString>> = vec![None; num_records];
    let mut types: Vec<String> = vec![String::new(); num_records];
    let mut mats = Materials {
        tex_source: vec![None; num_records],
        detail_source: vec![None; num_records],
        glow_source: vec![None; num_records],
        gloss_source: vec![None; num_records],
        dark_source: vec![None; num_records],
        decal_source: vec![None; num_records],
        base_clamp: vec![None; num_records],
        bump_source: vec![None; num_records],
        base_uv_set: vec![None; num_records],
        tex_file: vec![None; num_records],
        material: vec![None; num_records],
        material_ctrl: vec![-1; num_records],
        alpha: vec![None; num_records],
        vcol_prop: vec![None; num_records],
        stencil: vec![None; num_records],
        zbuffer: vec![None; num_records],
    };
    let mut skins: Vec<Option<SkinInst>> = vec![None; num_records];
    let mut skin_datas: Vec<Option<SkinData>> = vec![None; num_records];
    let mut pctrls: Vec<Option<ParticleCtrl>> = vec![None; num_records];
    let mut pdatas: Vec<Option<ParticleData>> = vec![None; num_records];
    let mut pmods: Vec<Option<ParticleMod>> = vec![None; num_records];
    let mut colour_data: Vec<Option<Vec<[f32; 5]>>> = vec![None; num_records];
    let mut uvctrls: Vec<Option<(i32, i32, i32, f32, f32, f32, f32, u32)>> = vec![None; num_records];
    let mut uvdatas: Vec<Option<UvAnim>> = vec![None; num_records];
    let mut visctrls: Vec<Option<KfCtrl>> = vec![None; num_records];
    let mut visdatas: Vec<Option<Vec<(f32, f32)>>> = vec![None; num_records];
    let mut kfctrls: Vec<Option<KfCtrl>> = vec![None; num_records];
    let mut kfdatas: Vec<Option<KfData>> = vec![None; num_records];
    let mut morphctrls: Vec<Option<MorphCtrl>> = vec![None; num_records];
    let mut morphdatas: Vec<Option<MorphData>> = vec![None; num_records];
    let mut alphactrls: Vec<Option<(i32, u16, f32, f32, f32, f32, i32, i32)>> = vec![None; num_records];
    let mut floatdatas: Vec<Option<Vec<(f32, f32)>>> = vec![None; num_records];
    let mut lights: Vec<Option<([f32; 3], f32)>> = vec![None; num_records];
    // Round 18dl: NiTextureEffect by record index - (source texture, clamp mode).
    let mut texeffects: Vec<Option<(i32, u32)>> = vec![None; num_records];
    let mut seq: Option<(i32, i32)> = None;
    let mut textkeys: Vec<Option<Vec<(f32, String)>>> = vec![None; num_records];

    for i in 0..num_records {
        if !r.ok() || r.p >= buf.len() {
            break;
        }
        let ty = r.str_sized();
        if ty.is_empty() {
            break;
        }
        if dbg {
            eprintln!("nif: {i} {ty} @{}", r.p);
        }
        types[i] = ty.clone();
        match ty.as_str() {
            "NiNode" | "RootCollisionNode" | "AvoidNode" | "NiBSParticleNode" | "NiBSAnimationNode"
            | "NiSwitchNode" | "NiBillboardNode" | "NiCollisionSwitch" | "NiSortAdjustNode"
            | "NiLODNode" => {
                if let Some(n) = read_node(&mut r, &ty, true) {
                    nodes[i] = Some(n);
                } else {
                    break;
                }
            }
            "NiTriShape" | "NiTriStrips" | "NiAutoNormalParticles" | "NiRotatingParticles" | "NiParticles" => {
                if let Some(n) = read_node(&mut r, &ty, false) {
                    nodes[i] = Some(n);
                } else {
                    break;
                }
            }
            /* Round 17y: the lights a mesh carries, read rather than stepped over. Glow in
               the Dahrk hangs a NiPointLight under a window's AttachLight and reads its
               colour and, as the node's *scale*, its radius (main.lua: "the radius is stored
               as scale"); the page puts that light in the room while the sunlit branch
               shows. An NiAVObject, then NiDynamicEffect's affected-node list, then the
               light's dimmer and three colours, then what each kind adds. */
            "NiPointLight" | "NiSpotLight" | "NiDirectionalLight" | "NiAmbientLight" => {
                let (name, flags, trafo, scale, extra, props) = r.av_object();
                let ctrl = r.last_ctrl;
                let affected = r.u32() as usize;
                if affected > 4096 {
                    break;
                }
                r.skip(affected * 4);
                let dimmer = r.f32();
                r.skip(12); // ambient
                let diffuse = [r.f32(), r.f32(), r.f32()];
                r.skip(12); // specular
                match ty.as_str() {
                    "NiPointLight" => r.skip(12),
                    "NiSpotLight" => r.skip(20),
                    _ => {}
                }
                if !r.ok() {
                    break;
                }
                lights[i] = Some((diffuse, dimmer));
                nodes[i] = Some(Node {
                    kind: ty.clone(),
                    name,
                    flags,
                    trafo,
                    scale,
                    children: Vec::new(),
                    extra,
                    data_ref: -1,
                    active_index: 0,
                    props,
                    skin_ref: -1,
                    skinned: false,
                    ctrl,
                    effects: Vec::new(),
                });
            }
            /* Round 18dl: the environment map. A NiTextureEffect hung on a node's effect
               list puts a sphere-mapped texture over everything below it, added after the
               lighting - the Telvanni crystals and portals in vanilla, and the fake bump
               mapping mods use (Telvanni Bump Maps: a bump map on NiTexturingProperty slot
               5 perturbs where the environment map is read). An NiAVObject, then
               NiDynamicEffect's affected-node list, then the model projection (a matrix
               and a vector), filtering, clamping, the texture type (2 environment), the
               coordinate generation (2 sphere map), the source texture, a clipping-plane
               byte and its plane, PS2 L/K and a short - 91 bytes after the list. Measured
               against `ex_t_housepod_02.nif`; the audit's OAAB waters scanned past this
               record before. Only an environment map by sphere mapping is kept, as OpenMW
               keeps only that (`handleTextureEffect`). */
            "NiTextureEffect" => {
                let (name, flags, trafo, scale, extra, props) = r.av_object();
                let ctrl = r.last_ctrl;
                let affected = r.u32() as usize;
                if affected > 4096 {
                    break;
                }
                r.skip(affected * 4);
                r.skip(36 + 12); // model projection matrix and translation
                r.skip(4); // texture filtering
                let clamp = r.u32();
                let tex_type = r.u32();
                let coord_gen = r.u32();
                let source = r.i32();
                r.skip(1 + 16 + 4 + 2); // clipping plane flag and plane, PS2 L/K, a short
                if !r.ok() {
                    break;
                }
                if tex_type == 2 && coord_gen == 2 {
                    texeffects[i] = Some((source, clamp.min(3)));
                }
                nodes[i] = Some(Node {
                    kind: ty.clone(),
                    name,
                    flags,
                    trafo,
                    scale,
                    children: Vec::new(),
                    extra,
                    data_ref: -1,
                    active_index: 0,
                    props,
                    skin_ref: -1,
                    skinned: false,
                    ctrl,
                    effects: Vec::new(),
                });
            }
            "NiAutoNormalParticlesData" | "NiParticlesData" | "NiRotatingParticlesData" => {
                if let Some(d) = read_particle_data(&mut r, ty == "NiRotatingParticlesData") {
                    pdatas[i] = Some(d);
                } else {
                    break;
                }
            }
            "NiParticleSystemController" | "NiBSPArrayController" => {
                if let Some(c) = read_particle_ctrl(&mut r) {
                    pctrls[i] = Some(c);
                } else {
                    break;
                }
            }
            /* Round 17m: the UV animation, kept rather than stepped over. The
               controller says when and how fast; the data says what the four channels
               do. */
            "NiUVController" => {
                let (next, _flags, frequency, phase, start, stop, target) = r.time_controller_read();
                /* Round 18dj: the texture set. Which of the shape's maps the controller
                   moves: those whose NiTexturingProperty slot reads this UV set (OpenMW's
                   `handleTextureControllers`: "UVController should work only for textures
                   which use a given UV Set, usually 0"). Every vanilla controller says 0,
                   the base map's set; Memento Mori's Ghostgate dome has a second one on
                   set 1 for its dark map, OAAB's lava one for its decal, Tamriel Rebuilt's
                   bulb mushrooms one for their glow map alone. Read as the slot it drives
                   rather than skipped, which had put every controller on the base map. */
                let texture_set = r.u16() as u32;
                let data = r.i32();
                if !r.ok() {
                    break;
                }
                uvctrls[i] = Some((next, target, data, frequency, phase, start, stop, texture_set));
            }
            "NiUVData" => {
                // The four key groups in the file's order (a struct literal evaluates its
                // fields in the order written).
                let a = UvAnim {
                    u_trans: r.key_group_f32(),
                    v_trans: r.key_group_f32(),
                    u_scale: r.key_group_f32(),
                    v_scale: r.key_group_f32(),
                    ..Default::default()
                };
                if !r.ok() {
                    break;
                }
                uvdatas[i] = Some(a);
            }
            "NiVisController" => {
                let (next, flags, frequency, phase, start, stop, target) = r.time_controller_read();
                let data = r.i32();
                if !r.ok() {
                    break;
                }
                visctrls[i] = Some(KfCtrl { next, flags, frequency, phase, start, stop, target, data });
            }
            /* NiVisData: a key count, then per key its time and a visible byte. */
            "NiVisData" => {
                let n = r.u32() as usize;
                if n > 65536 {
                    break;
                }
                let mut keys = Vec::with_capacity(n);
                for _ in 0..n {
                    let t = r.f32();
                    let v = r.b.get(r.p).copied().unwrap_or(0);
                    r.p += 1;
                    keys.push((t, if v != 0 { 1.0 } else { 0.0 }));
                }
                if !r.ok() {
                    break;
                }
                visdatas[i] = Some(keys);
            }
            /* Round 17y: the node animations, kept rather than stepped over. Robin's
               `f\active_blight_large.nif` is a stack of translucent sheets that turn
               slowly upwards, and a still frame of it is "a still cut out thing in the
               world". The controller says when and how fast; the data says where the node
               goes. */
            "NiKeyframeController" => {
                let (next, flags, frequency, phase, start, stop, target) = r.time_controller_read();
                let data = r.i32();
                if !r.ok() {
                    break;
                }
                kfctrls[i] = Some(KfCtrl { next, flags, frequency, phase, start, stop, target, data });
            }
            "NiKeyframeData" => {
                let Some(d) = read_keyframe_data(&mut r) else { break };
                kfdatas[i] = Some(d);
            }
            /* Round 18au: the morph animation, kept rather than stepped over — 18r measured
               these two records so the moths' geometry survived them; now their contents
               are what makes the wings beat. The controller is the time controller's 26
               bytes, the data reference, and (see `skip_or_read`'s note, moved here) the
               one trailing byte that 4.0.0.2 files carry although the format notes date it
               later — taken only when the next type name is not already there. */
            "NiGeomMorpherController" => {
                let (next, flags, frequency, phase, start, stop, target) = r.time_controller_read();
                let data = r.i32();
                if !block_starts_at(r.b, r.p) && block_starts_at(r.b, r.p + 1) {
                    r.skip(1); // always-update
                }
                if !r.ok() {
                    break;
                }
                morphctrls[i] = Some(MorphCtrl { next, flags, frequency, phase, start, stop, target, data });
            }
            /* NiMorphData: a morph count, a vertex count, a `relativeTargets` byte, then per
               morph a key count, an interpolation kind, that many keys and one vector per
               vertex. Only the time and the weight of a key are kept — a quadratic key's
               tangents go the way every other curve's do here (`key_group_f32`). */
            "NiMorphData" => {
                let morphs = r.u32() as usize;
                let verts = r.u32() as usize;
                if morphs > 4096 || verts > 1 << 20 {
                    break;
                }
                let relative = r.b.get(r.p).copied().unwrap_or(1) != 0;
                r.p += 1;
                let mut d = MorphData { relative, morphs: Vec::with_capacity(morphs) };
                for _ in 0..morphs {
                    let nk = r.u32() as usize;
                    let interp = r.u32();
                    if nk > 1 << 20 {
                        break;
                    }
                    let extra = match interp {
                        2 => 2,
                        3 => 3,
                        _ => 0,
                    };
                    let mut keys = Vec::with_capacity(nk);
                    for _ in 0..nk {
                        let t = r.f32();
                        let w = r.f32();
                        r.skip(extra * 4);
                        keys.push((t, w));
                    }
                    let mut vs = Vec::with_capacity(verts);
                    for _ in 0..verts {
                        vs.push([r.f32(), r.f32(), r.f32()]);
                    }
                    if !r.ok() {
                        break;
                    }
                    d.morphs.push((keys, vs));
                }
                if !r.ok() {
                    break;
                }
                morphdatas[i] = Some(d);
            }
            /* And the material's fade: a NiAlphaController on the NiMaterialProperty,
               its keys in a NiFloatData. The blight's sheets are born clear, thicken and
               thin again as they rise. */
            "NiAlphaController" => {
                let (next, flags, frequency, phase, start, stop, target) = r.time_controller_read();
                let data = r.i32();
                if !r.ok() {
                    break;
                }
                alphactrls[i] = Some((next, flags, frequency, phase, start, stop, target, data));
            }
            "NiFloatData" => {
                let keys = r.key_group_f32();
                if !r.ok() {
                    break;
                }
                floatdatas[i] = Some(keys);
            }
            "NiGravity" => {
                let next = r.i32();
                r.i32(); // controller
                let decay = r.f32();
                let force = r.f32();
                let kind = r.u32();
                let position = [r.f32(), r.f32(), r.f32()];
                let direction = [r.f32(), r.f32(), r.f32()];
                if !r.ok() {
                    break;
                }
                pmods[i] = Some(ParticleMod::Gravity { next, decay, force, kind, position, direction });
            }
            "NiParticleGrowFade" => {
                let next = r.i32();
                r.i32();
                let grow = r.f32();
                let fade = r.f32();
                if !r.ok() {
                    break;
                }
                pmods[i] = Some(ParticleMod::GrowFade { next, grow, fade });
            }
            "NiParticleColorModifier" => {
                let next = r.i32();
                r.i32();
                let data = r.i32();
                if !r.ok() {
                    break;
                }
                pmods[i] = Some(ParticleMod::Colour { next, data });
            }
            "NiParticleRotation" => {
                let next = r.i32();
                r.i32();
                let random_axis = r.b.get(r.p).copied().unwrap_or(0) != 0;
                r.p += 1;
                let axis = [r.f32(), r.f32(), r.f32()];
                let speed = r.f32();
                if !r.ok() {
                    break;
                }
                pmods[i] = Some(ParticleMod::Rotation { next, random_axis, axis, speed });
            }
            "NiParticleBomb" => {
                let next = r.i32();
                r.i32();
                r.skip(4 * 4); // decay, duration, delta v, start
                r.skip(8); // decay type, symmetry
                r.skip(24); // position, direction
                if !r.ok() {
                    break;
                }
                pmods[i] = Some(ParticleMod::Other { next });
            }
            /* Round 18bg (§G6): through `key_group_head`, like every other key group.
               Written out by hand, this record read the interpolation kind even when it
               had no keys — four bytes out of the *next* record, and from there the file
               is gibberish: every record after it reads the wrong bytes, and `resync`
               then salvages what it can, which is a mesh with pieces missing. It also
               knew only about quadratic keys, so a TBC-keyed colour track lost twelve
               bytes a key on top. An emptily-keyed `NiColorData` is not exotic: it is
               what a particle system saves when somebody clears a colour curve and leaves
               the modifier in place. */
            "NiColorData" => {
                let Some((n, extra)) = r.key_group_head(4) else { break };
                let mut keys = Vec::with_capacity(n);
                for _ in 0..n {
                    let t = r.f32();
                    let v = [r.f32(), r.f32(), r.f32(), r.f32()];
                    r.skip(extra * 4);
                    keys.push([t, v[0], v[1], v[2], v[3]]);
                }
                if !r.ok() {
                    break;
                }
                colour_data[i] = Some(keys);
            }
            "NiTriShapeData" => {
                if let Some(d) = read_shape_data(&mut r) {
                    datas[i] = Some(d);
                } else {
                    break;
                }
            }
            /* Round 18bg (§G2): the strips land in the same `datas` slot, already turned
               into triangles, so nothing downstream — the draw walk, the collision walk,
               the skinner, the mesh scatter — has to know a shape was stripified. */
            "NiTriStripsData" => {
                if let Some(d) = read_strips_data(&mut r) {
                    datas[i] = Some(d);
                } else {
                    break;
                }
            }
            "NiStringExtraData" => {
                let next = r.i32();
                r.u32(); // bytes remaining
                let value = r.str_sized();
                if !r.ok() {
                    break;
                }
                extras[i] = Some(ExtraString { value, next });
            }
            /* Round 18ag: a `.kf` file's root - an NiObjectNET and nothing more: a name,
               the extra-data chain (the text keys, then one string per track naming the
               node it moves) and the controller chain (the tracks, in the same order). */
            "NiSequenceStreamHelper" => {
                let (_name, extra) = r.object_net();
                if !r.ok() {
                    break;
                }
                seq = Some((extra, r.last_ctrl));
            }
            /* And the text keys, kept rather than stepped over: `Idle: Start`, `Idle:
               Stop` and their kin are how a clip is found inside the file's one timeline. */
            "NiTextKeyExtraData" => {
                let next = r.i32();
                r.u32(); // bytes remaining
                let n = r.u32() as usize;
                if n > 65536 {
                    break;
                }
                let mut keys = Vec::with_capacity(n);
                for _ in 0..n {
                    let t = r.f32();
                    let k = r.str_sized();
                    keys.push((t, k));
                }
                if !r.ok() {
                    break;
                }
                textkeys[i] = Some(keys);
                // The chain runs on through it: kept as an extra with no marker in it.
                extras[i] = Some(ExtraString { value: String::new(), next });
            }
            /* The skin of a skinned shape (round 12 item 3): which data, which skeleton
               root, which bones. At 4.0.0.2 there is no skin partition. */
            "NiSkinInstance" => {
                let data = r.i32();
                let root = r.i32();
                let bones = r.ref_list();
                if !r.ok() {
                    break;
                }
                skins[i] = Some(SkinInst { data, root, bones });
            }
            "NiSkinData" => {
                let trafo = read_skin_trafo(&mut r);
                let nb = r.u32() as usize;
                if nb > 4096 {
                    break;
                }
                /* A skin partition link, -1 in every vanilla file. The format notes put
                   this field at 4.2.1.0 and later; `furn_banner_hlaalu_01.nif` (4.0.0.2)
                   has it, measured: after the bone count comes 0xFFFFFFFF and only then
                   the first bone's rotation. Reading the bones four bytes early made
                   every rotation start with NaN. */
                r.i32();
                let mut bones = Vec::with_capacity(nb);
                for _ in 0..nb {
                    let bt = read_skin_trafo(&mut r);
                    r.skip(16); // bounding sphere: centre, radius
                    let nw = r.u16() as usize;
                    let mut weights = Vec::with_capacity(nw);
                    for _ in 0..nw {
                        let vi = r.u16();
                        let w = r.f32();
                        weights.push((vi, w));
                    }
                    bones.push(SkinBone { trafo: bt, weights });
                }
                if !r.ok() {
                    break;
                }
                skin_datas[i] = Some(SkinData { trafo, bones });
            }
            _ => {
                // Records we do not need the contents of, but do need the length of:
                // the indices that follow have to stay aligned, because children and
                // shape data are referenced by index.
                let at = r.p;
                if !skip_or_read(&mut r, &ty, Some((&mut mats, i))) {
                    r.p = at;
                    /* Round 18r: said out loud, behind `GDN_NIF_DEBUG`. A record the
                       reader cannot measure is the one failure in this file that is both
                       silent and total - it does not throw, it does not return an error,
                       it hands back a mesh with pieces missing or with none at all - and
                       the two that hid the moths took an afternoon to find because
                       nothing anywhere said "I do not know how long this is". */
                    if dbg {
                        eprintln!("nif: {i} {ty} UNKNOWN at {at} - scanning forward");
                    }
                    if !resync(&mut r) {
                        break;
                    }
                    scanned += 1;
                }
            }
        }
    }

    let skin_src = vec![None; num_records];
    let rec = Records { kfctrls, kfdatas, morphctrls, morphdatas, seq, textkeys, skel_animated: false, skin_src, alphactrls, floatdatas, lights, texeffects, nodes, datas, extras, types, mats, skins, skin_datas, scanned, pctrls, pdatas, pmods, colour_data, uvctrls, uvdatas, visctrls, visdatas };
    Some(rec)
}

/// Round 18ag: a `.kf` file's tracks and text keys. `None` for a file that is not one.
///
/// The layout is the one OpenMW's `loadKf` reads: the root is a NiSequenceStreamHelper;
/// its extra-data chain is a NiTextKeyExtraData and then one NiStringExtraData per track,
/// each naming the node the track moves; its controller chain is the tracks themselves,
/// one NiKeyframeController per string, in the same order. A track that is switched off
/// or keys nothing is dropped, as the game drops it.
pub fn parse_kf(buf: &[u8]) -> Option<KfSequence> {
    let rec = read_records_kf(buf, None)?;
    let (extra, ctrl) = rec.seq?;
    let mut out = KfSequence::default();
    // The text keys: the first link of the chain, by the layout - but found wherever
    // they are, since the chain's order is the only thing that ties strings to tracks.
    let mut e = extra;
    let mut guard = 0;
    let mut names: Vec<String> = Vec::new();
    while e >= 0 && (e as usize) < rec.types.len() && guard < 4096 {
        guard += 1;
        if let Some(Some(keys)) = rec.textkeys.get(e as usize) {
            out.text_keys.extend(keys.iter().cloned());
        } else if let Some(Some(x)) = rec.extras.get(e as usize) {
            names.push(x.value.clone());
        }
        e = match rec.extras.get(e as usize) {
            Some(Some(x)) => x.next,
            _ => -1,
        };
    }
    let mut c = ctrl;
    let mut k = 0usize;
    guard = 0;
    while c >= 0 && (c as usize) < rec.types.len() && k < names.len() && guard < 4096 {
        guard += 1;
        let Some(Some(kc)) = rec.kfctrls.get(c as usize) else { break };
        if kc.flags & 0x0008 != 0 {
            if let Some(Some(d)) = rec.kfdatas.get(kc.data.max(0) as usize) {
                if !d.is_empty() {
                    out.tracks.push(KfTrack { node: names[k].clone(), ctrl: kc.clone(), data: d.clone() });
                }
            }
        }
        k += 1;
        c = kc.next;
    }
    if out.tracks.is_empty() { None } else { Some(out) }
}

/// Round 18ag: hangs a `.kf`'s tracks on this file's nodes, by name, as if the file
/// carried the controllers itself - after which `draw_parts` finds them exactly as it
/// finds a NiBSAnimationNode's own. Every track is set to the same clip, the actor's
/// idle, looping: a parked silt strider sways and feels the air; a banner ripples.
///
/// The controller's own window is *replaced* by the clip's rather than intersected with
/// it: a `.kf` controller spans the whole file, every clip in it end to end, and playing
/// that would walk the strider through its death throes on the quay.
fn bind_kf(rec: &mut Records, kf: &KfSequence) {
    let find = |rec: &Records, name: &str| -> Option<usize> {
        let exact = rec.nodes.iter().position(|n| n.as_ref().map(|n| n.name == name).unwrap_or(false));
        exact.or_else(|| rec.nodes.iter().position(|n| n.as_ref().map(|n| n.name.eq_ignore_ascii_case(name)).unwrap_or(false)))
    };
    let Some((start, stop)) = kf.idle() else {
        /* Round 18ak: nothing loops, so nothing is bound - the nodes are *posed* where
           the keys put them at the still instant and left there, and the skeleton is not
           an animated one: no per-frame skinning, no frames kept coming for a banner
           that hangs still. The tea shop's flat-resting skeleton stands up this way too
           when its twin is a still one. */
        let at = kf.idle_time();
        for t in &kf.tracks {
            let Some(idx) = find(rec, &t.node) else { continue };
            if idx == 0 {
                continue;
            }
            let ctrl = KfCtrl { start: at, stop: at, ..t.ctrl.clone() };
            if let Some(Some(nd)) = rec.nodes.get_mut(idx) {
                let (m, s) = keyed_pose(&nd.trafo, nd.scale, &ctrl, &t.data);
                nd.trafo = m;
                nd.scale = s;
            }
        }
        return;
    };
    let mut bound = 0usize;
    for t in &kf.tracks {
        let Some(idx) = find(rec, &t.node) else { continue };
        if idx == 0 {
            continue; // the root's transform is never applied, and neither is its animation
        }
        // The file's own controller on that node, if any, gives way to the sequence's.
        for c in rec.kfctrls.iter_mut() {
            if let Some(kc) = c {
                if kc.target == idx as i32 {
                    *c = None;
                }
            }
        }
        let data_i = rec.kfdatas.len();
        // Only the clip's keys (`KfData::window`): the rest never play.
        rec.kfdatas.push(Some(t.data.window(start, stop)));
        rec.kfctrls.push(Some(KfCtrl {
            next: -1,
            flags: 0x0008, // on, and looping (cycle bits 1-2 clear)
            frequency: 1.0,
            phase: 0.0,
            start,
            stop,
            target: idx as i32,
            data: data_i as i32,
        }));
        bound += 1;
    }
    rec.skel_animated = bound > 0;
}



/// The same, with the animation beside the mesh bound first — round 18be (G7).
///
/// A skinned shape's vertices are baked at the clip's first frame (`skin_records`), so
/// the pose the keys put the skeleton in is the pose the hull is built from. The draw
/// pass has always done this (`parse_draw_lit_kf`, which is how the tea-shop banner is
/// stood up); collision never did, because it went through `nif::parse` and nothing
/// there knew about the `.kf`. For an `x`-twin whose rest pose differs from its idle
/// pose that left a flat hull under a standing object — the opposite of the "what you
/// see is what blocks grass" rule `mesh_payload` states. Both paths bind the same
/// sequence now, so the two agree by construction rather than by coincidence (which is
/// why no test caught it: every fixture keeps its keys in the `.nif` itself, where both
/// paths already saw them).
pub fn parse_with_kf(buf: &[u8], kf: Option<&KfSequence>) -> Option<MeshGeom> {
    geom_of(read_records_kf(buf, kf)?)
}

/// The collision geometry from records already read (see `parse_with_kf`).
fn geom_of(rec: Records) -> Option<MeshGeom> {
    /* Round 18be (G7): each node's *local* transform as its keys put it at the clip's
       start, or `None` for a node with none — the same question `skin_records` asks
       through `keyed_pose`, asked once here so the collision walk can pose a rigid
       shape the way the draw pass does. A skinned shape's vertices are baked by
       `skin_records` and need no transform at all; a rigid piece hanging under a moving
       node (the siltstrider's sixty-five of them, a banner's pole) is put where its keys
       put it by `draw_parts` and used to be put where its rest pose put it here. */
    let poses: Vec<Option<([f32; 12], f32)>> = (0..rec.nodes.len())
        .map(|i| {
            let n = rec.nodes[i].as_ref()?;
            let (ctrl, data) = kf_for(&rec, i as i32)?;
            Some(keyed_pose(&n.trafo, n.scale, &ctrl, &data))
        })
        .collect();
    let (nodes, datas, extras, types, scanned) =
        (rec.nodes, rec.datas, rec.extras, rec.types, rec.scanned);
    let mut g = build(&nodes, &datas, &extras, &types, &poses)?;
    g.scanned = scanned;
    Some(g)
}

/// One drawable piece of a mesh: the geometry, and what to paint it with.
#[derive(Default, Clone)]
pub struct DrawPart {
    pub name: String,
    /// World-space-agnostic, in the mesh's own space, already transformed by the
    /// node chain above it.
    pub pos: Vec<f32>,
    pub nrm: Vec<f32>,
    pub uv: Vec<f32>,
    pub idx: Vec<u16>,
    /// The texture file this shape names, as written in the NIF.
    pub tex: String,
    /// Round 17h: the *detail* map, if the shape's NiTexturingProperty has one, and the
    /// second UV set it reads. The game multiplies the base by it and doubles the
    /// result; a mesh like `pc_flora_scumalpha_03.nif` is white without it.
    pub detail_tex: String,
    /// NiTexturingProperty's glow map (slot 4), the light a shape *makes* rather than
    /// the light it takes — a lamp's glass, a lit sign, a rune. OpenMW adds it to the
    /// final colour outright (objects.frag: `gl_FragData[0].xyz += texture2D(emissiveMap,
    /// emissiveMapUV).xyz`), after the lighting and after the fog, which is why a lamp
    /// reads as lit at midnight and stays visible through fog at a distance. Round 17p.
    pub glow_tex: String,
    /// Wraithguard: the gloss map (NiTexturingProperty slot 3), on the base UVs.
    pub gloss_tex: String,
    /// Which UV set the glow map names. Every glow map in Morrowind and in OAAB names 0
    /// (`tests/glow.rs` asserts it); a shape asking for another set is drawn with set 0
    /// rather than dropped, and the count is what would say the assumption had broken.
    pub glow_uv_set: u32,
    /// The detail map's UVs, one pair per vertex — empty unless `detail_tex` is set and
    /// the shape carries a second set.
    pub uv2: Vec<f32>,
    /// Round 17y: NiTexturingProperty's dark map (slot 1) and the UVs it reads — a second
    /// stage multiplied over the base, no doubling (OpenMW's objects.frag: `gl_FragData[0]
    /// *= texture2D(darkMap, darkMapUV)`). Glow in the Dahrk's lit windows keep their
    /// colour here. Empty on nearly everything.
    pub dark_tex: String,
    pub uv_dark: Vec<f32>,
    /// Round 18df: NiTexturingProperty's decal map (slot 6) and the UVs it reads - laid
    /// over the base by its own alpha before the lighting (OpenMW's objects.frag:
    /// `mix(base.rgb, decal.rgb, decal.a)`). OAAB's and Tamriel Rebuilt's road signs,
    /// posters and stains; nothing in vanilla. Empty otherwise.
    pub decal_tex: String,
    pub uv_decal: Vec<f32>,
    /// Round 18df: which edges of the base map are clamped - bit 0 for S, bit 1 for T;
    /// 0 wraps both, the default and the format's mode 3. (The NIF's own numbering is 0
    /// clamp both, 1 clamp S/wrap T, 2 wrap S/clamp T, 3 wrap both; turned round here so
    /// that the commonest case is the zero `Default` gives.) 447 vanilla static shapes
    /// clamp: drawn wrapping, a texture read past its edge shows a strip of its far side.
    pub clamp_st: u8,
    /// Alpha test threshold, 0-255, when alpha testing is on. `None` leaves the
    /// renderer on its own default.
    pub alpha_threshold: Option<u8>,
    pub alpha_flags: u16,
    pub diffuse: [f32; 3],
    /// Round 18de: the NiMaterialProperty's ambient colour - what the scene's ambient
    /// light is multiplied by, where the diffuse scales the sun and the lamps. Equal to
    /// the diffuse on most shapes; 0.1 under a white diffuse on the ships, the piers and
    /// the Daedric walls, which is how they stay dark in shade. Neither is consulted when
    /// the vertex colours are the colour source (vertex mode 2, the default) - the fixed
    /// pipeline takes both from the vertex then - which is the other half of the fix.
    pub ambient: [f32; 3],
    pub mat_alpha: f32,
    /// Vertex colours, RGB bytes, three per vertex — or empty when the shape has none,
    /// which the renderer draws as white. Round 11 item 11.
    pub col: Vec<u8>,
    /// Round 18df: the vertex alpha, one byte a vertex - or empty when every vertex is
    /// at 255 (all of vanilla) or the shape has no colours. With the vertex colours as
    /// the colour source the fragment's alpha is the texture's times this, and the
    /// material's alpha is not consulted (the fixed pipeline's D3DMCS_COLOR1; OpenMW's
    /// getDiffuseColor()); OAAB's `t_root_pillar_xl` fades its ends through it.
    pub col_alpha: Vec<u8>,
    /// Round 17m: the NiMaterialProperty's emissive colour. Light the shape gives off
    /// rather than receives — a lantern's paper shade, a candle flame, a glowing rune.
    /// Black on almost everything.
    pub emissive: [f32; 3],
    /// Round 17m: the UV animation this shape inherits, if any. The waterfalls, the
    /// stream ripples and the planter water all move by scrolling their texture.
    pub uv_anim: Option<UvAnim>,
    /// Round 18dj: the UV animations of the other maps, each the controller of the UV
    /// set those maps read (`uv_anims_of`), with the maps it moves as bits - 1 the dark
    /// map, 2 the detail, 4 the glow, 8 the decal. Empty on every vanilla shape; the
    /// Ghostgate dome scrolls its dark map, OAAB's lava its decal, Tamriel Rebuilt's
    /// bulb mushrooms their glow map.
    pub uv_anims_other: Vec<(u8, UvAnim)>,
    /// Round 18dl: the environment map a NiTextureEffect above the shape puts on it,
    /// sphere-mapped and added after the lighting, and its clamp mode (0 clamp both,
    /// 1 clamp S, 2 clamp T, 3 wrap). Empty on all but 19 vanilla statics (the Telvanni
    /// crystals and portals) and the bump-mapped mod meshes.
    pub env_tex: String,
    pub env_clamp: u32,
    /// Round 18dl: the bump map on slot 5, read only under an environment map (it does
    /// nothing without one): the texture, the luma scale and offset (the blue channel
    /// scales the reflection), and the 2x2 matrix from its red and green to an offset
    /// on the environment map's coordinates.
    pub bump_tex: String,
    pub bump_luma: [f32; 2],
    pub bump_mat: [f32; 4],
    /// Round 17w: how the vertex colours are used — 0 ignored, 1 as the emissive term,
    /// 2 multiplied into the lit colour (the default, and what a shape with no
    /// NiVertexColorProperty gets).
    pub vcol_mode: u8,
    /// Round 17w: true when the NiVertexColorProperty says lighting mode 0 — the shape is
    /// drawn as its texture times its vertex colours, with no sun and no ambient at all.
    /// The waterfalls, the ripples, most effect sheets.
    pub unlit: bool,
    /// Round 17w: the NiStencilProperty's draw mode — 0 none/default, 1 counter-clockwise
    /// front, 2 clockwise front, 3 both sides. Anything but 3 is drawn with its back faces
    /// culled, as the game draws it.
    pub draw_mode: u8,
    /// Round 17x: which branch of a Glow in the Dahrk NightDaySwitch this shape is under —
    /// 0 none (always drawn), 1 OFF (the window by day), 2 ON (lit, at night), 3 INT-DAY
    /// (an interior's sunlit window). The page shows one branch at a time by the clock.
    pub day_night: u8,
    /// Round 18dg: true when the nearest NiZBufferProperty says the shape does not write
    /// the depth buffer (bit 1 clear). False - writes - for a shape without one, which is
    /// every tree; the game draws its blended foliage with depth writes on and the shapes
    /// sorted far to near, and the viewport does the same now. The kwama eggs, the magic
    /// targets and the mods' pools say test-only, and are drawn over without writing.
    pub no_zwrite: bool,
    /// Round 17y: what moves about this shape — the animated nodes above it, whose
    /// transforms the geometry here is baked *under* rather than through, and the
    /// material's alpha keys. `None` on nearly every shape; the effect sheets
    /// (`f\active_blight_large.nif`) are the reason it exists.
    pub anim: Option<PartAnim>,
    /// Round 18ag: the shape skinned to an animated skeleton, when it is one.
    pub skin: Option<SkinAnim>,
    /// Round 18au: the shape's morph animation, when it has one — the moths' wingbeat.
    pub morph: Option<MorphAnim>,
}

/// Draw geometry for a shape the engine invented rather than read — the corpse slab
/// being the only one.
///
/// Flat-shaded from the triangle soup: there are no smoothing groups to honour and no
/// texture to map, and a stand-in that looked more finished than it is would invite
/// the reader to trust it as a model. The point of it is the ground it covers.
pub fn draw_from_geom(g: &MeshGeom, name: &str) -> Vec<DrawPart> {
    let mut pos = Vec::with_capacity(g.tris.len() * 3);
    let mut nrm = Vec::with_capacity(g.tris.len() * 3);
    let mut uv = Vec::with_capacity(g.tris.len() * 2);
    let mut idx: Vec<u16> = Vec::with_capacity(g.tris.len());
    for (i, t) in g.tris.chunks_exact(3).enumerate() {
        let (a, b, c) = (t[0], t[1], t[2]);
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-6);
        for p in [a, b, c] {
            pos.extend_from_slice(&p);
            nrm.extend_from_slice(&[n[0] / len, n[1] / len, n[2] / len]);
            uv.extend_from_slice(&[0.0, 0.0]);
        }
        let base = (i * 3) as u16;
        idx.extend_from_slice(&[base, base + 1, base + 2]);
    }
    vec![DrawPart {
        name: name.to_string(),
        pos,
        nrm,
        uv,
        idx,
        tex: String::new(),
        detail_tex: String::new(),
        glow_tex: String::new(),
        gloss_tex: String::new(),
        glow_uv_set: 0,
        uv2: Vec::new(),
        dark_tex: String::new(),
        uv_dark: Vec::new(),
        decal_tex: String::new(),
        uv_decal: Vec::new(),
        clamp_st: 0,
        alpha_threshold: None,
        alpha_flags: 0,
        // A muted, slightly warm grey: visible against grass without reading as an
        // object anybody placed.
        diffuse: [0.62, 0.45, 0.38],
        ambient: [0.62, 0.45, 0.38],
        mat_alpha: 1.0,
        col: Vec::new(),
        col_alpha: Vec::new(),
        emissive: [0.0; 3],
        uv_anim: None,
        uv_anims_other: Vec::new(),
        env_tex: String::new(),
        env_clamp: 3,
        bump_tex: String::new(),
        bump_luma: [1.0, 0.0],
        bump_mat: [1.0, 0.0, 0.0, 1.0],
        skin: None,
        morph: None,
        vcol_mode: 2,
        unlit: false,
        draw_mode: 0,
        day_night: 0,
        no_zwrite: false,
        anim: None,
    }]
}

/// Collects the drawable geometry, resolving each shape's inherited properties.
///
/// The rules match the visible walk in `build`: nothing culled, nothing under a
/// collision node, no billboards, particles or effects, editor markers only when the
/// root says MRK, and one live child per switch node.
/* The root node's own transform is not applied — the game does not apply it, and
   neither does OpenMW. Found in round 15 with `ex_stronghold_wall00.nif`, whose root
   NiNode carries a 90° turn: with it the wall runs along x, and every stronghold in
   Morrowind.esm places the piece with rotations that only line the walls up if it runs
   along y. Robin: "looks rotated 90 degrees wrong". The rule holds for every walk that
   composes transforms - drawing, collision, the RCN search, and the bones a skin
   accumulates from the root. */
fn node_trafo(idx: i32, trafo: &[f32; 12], scale: f32, n: &Node) -> ([f32; 12], f32) {
    if idx == 0 { (*trafo, scale) } else { mul(trafo, scale, &n.trafo, n.scale) }
}

/// Round 18ag: each node's parent (-1 at the root and for anything unparented) — the
/// first node that lists it as a child, which is the tree's own answer.
fn parents_of(rec: &Records) -> Vec<i32> {
    let n = rec.nodes.len();
    let mut parent: Vec<i32> = vec![-1; n];
    for (i, node) in rec.nodes.iter().enumerate() {
        if let Some(nd) = node {
            for &c in &nd.children {
                if c >= 0 && (c as usize) < n && parent[c as usize] < 0 {
                    parent[c as usize] = i as i32;
                }
            }
        }
    }
    parent
}

/// Round 18ag: a bone's animation - the chain of moving nodes from the root down to
/// it, built the way `draw_parts` builds a shape's: the still transform gathered since
/// the last link rides on each link as `pre`, and what is left over after the last one
/// (down to the bone itself) is `post`. A bone under no moving node has an empty chain
/// and its whole world transform in `post`, which is the bind pose the old way.
fn bone_anim(
    rec: &Records,
    parent: &[i32],
    kf_of: &dyn Fn(i32) -> Option<(KfCtrl, KfData)>,
    bone: i32,
    trafo: ([f32; 12], f32),
) -> Option<BoneAnim> {
    let ident = [1., 0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 0.];
    let n = rec.nodes.len();
    let mut path = Vec::new();
    let mut at = bone;
    let mut guard = 0;
    while at >= 0 && (at as usize) < n && guard < 128 {
        path.push(at);
        at = parent[at as usize];
        guard += 1;
    }
    let mut chain = Vec::new();
    let mut m = ident;
    let mut s = 1.0f32;
    for &i in path.iter().rev() {
        let nd = rec.nodes[i as usize].as_ref()?;
        match if i != 0 { kf_of(i) } else { None } {
            Some((ctrl, data)) => {
                chain.push(AnimLink { pre: (m, s), local: (nd.trafo, nd.scale), ctrl, data });
                m = ident;
                s = 1.0;
            }
            None => {
                let (nm, ns) = node_trafo(i, &m, s, nd);
                m = nm;
                s = ns;
            }
        }
    }
    let name = rec.nodes.get(bone as usize).and_then(|n| n.as_ref()).map(|n| n.name.clone()).unwrap_or_default();
    Some(BoneAnim { name, chain, post: (m, s), trafo })
}

/// Round 18ag: a skinned shape's per-frame form - the bones with their chains, and the
/// vertices with four bone slots each. A vertex the file gives more than four bones
/// keeps the four heaviest, renormalised; nothing in Morrowind's own files has more.
fn skin_anim(
    rec: &Records,
    parent: &[i32],
    kf_of: &dyn Fn(i32) -> Option<(KfCtrl, KfData)>,
    src: &SkinSrc,
) -> Option<SkinAnim> {
    if src.bones.len() != src.bone_nodes.len() || src.bones.is_empty() || src.bones.len() > 255 {
        return None;
    }
    let mut bones = Vec::with_capacity(src.bones.len());
    for (b, &node) in src.bone_nodes.iter().enumerate() {
        bones.push(bone_anim(rec, parent, kf_of, node, src.bones[b].trafo)?);
    }
    let nv = src.verts.len();
    let mut per: Vec<Vec<(u8, f32)>> = vec![Vec::new(); nv];
    for (b, bone) in src.bones.iter().enumerate() {
        for &(vi, w) in &bone.weights {
            if (vi as usize) < nv && w > 0.0 {
                per[vi as usize].push((b as u8, w));
            }
        }
    }
    let mut idx = vec![255u8; nv * 4];
    let mut w = vec![0f32; nv * 4];
    for (vi, list) in per.iter_mut().enumerate() {
        list.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        list.truncate(4);
        let sum: f32 = list.iter().map(|x| x.1).sum();
        if sum <= 0.0 {
            continue;
        }
        for (k, (b, wt)) in list.iter().enumerate() {
            idx[vi * 4 + k] = *b;
            w[vi * 4 + k] = wt / sum;
        }
    }
    let mut pos = Vec::with_capacity(nv * 3);
    for v in &src.verts {
        pos.extend_from_slice(v);
    }
    let mut nrm = Vec::with_capacity(nv * 3);
    for v in &src.normals {
        nrm.extend_from_slice(v);
    }
    Some(SkinAnim { pos, nrm, idx, w, bones })
}

fn draw_parts(rec: &Records) -> Vec<DrawPart> {
    let ident = [1., 0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 0.];
    // Round 18ag: the parent of every node, so a bone's chain can be built upward.
    let parent = parents_of(rec);

    // The first NiSourceTexture in the file, used when a shape names no texture of
    // its own — which is how a good many grass meshes are written.
    let fallback_tex = rec.mats.tex_file.iter().flatten().next().cloned().unwrap_or_default();

    let kind_of = |i: i32| -> &str {
        if i < 0 {
            return "";
        }
        rec.types.get(i as usize).map(|s| s.as_str()).unwrap_or("")
    };

    /* Round 17m: the UV animations a node carries, found by walking its controller
       chain. A NiUVController usually hangs off the NiBSAnimationNode above the shape
       rather than off the shape itself, so it is inherited the way the properties are:
       the nearest node with any down the tree wins. Round 18dj: all of them, each with
       the texture set it names - a node may carry one per set (the Ghostgate dome's
       tube has one on set 0 for its base map and another on set 1 for its dark map, on
       different curves), and the shape below matches each of its maps to the
       controller of the set that map reads. */
    let uv_anims_of = |i: i32| -> Vec<(u32, UvAnim)> {
        /* By the controller's own `target` rather than by walking the node's controller
           chain: a NiTimeController names what it animates, and reading the answer off
           the controller needs no guess about the length of whatever other controllers
           share the chain with it. */
        let mut out: Vec<(u32, UvAnim)> = Vec::new();
        for c in rec.uvctrls.iter().flatten() {
            let (_next, target, data, freq, phase, start, stop, texture_set) = *c;
            if target != i {
                continue;
            }
            if let Some(Some(a)) = rec.uvdatas.get(data as usize) {
                if a.moves() && !out.iter().any(|(t, _)| *t == texture_set) {
                    let mut a = a.clone();
                    a.frequency = freq;
                    a.phase = phase;
                    a.start = start;
                    a.stop = stop;
                    out.push((texture_set, a));
                }
            }
        }
        out
    };

    /* Round 18au: the NiGeomMorpherController a shape carries, by the controller's
       `target` the way the UV animation is found, with its NiMorphData. */
    let morph_of = |i: i32| -> Option<(&MorphCtrl, &MorphData)> {
        for c in rec.morphctrls.iter().flatten() {
            if c.target != i {
                continue;
            }
            if let Some(Some(d)) = rec.morphdatas.get(c.data.max(0) as usize) {
                return Some((c, d));
            }
        }
        None
    };

    /* Round 17y: the NiKeyframeController a node carries, by the controller's `target`
       the way the UV animation is found — and only one that is switched on and has keys.
       A controller that is off, or that keys nothing, leaves the node where its resting
       transform puts it, which is what the walk does anyway. */
    let kf_of = |i: i32| -> Option<(KfCtrl, KfData)> { kf_for(rec, i) };
    /* And a material's fade — the NiAlphaController on its controller chain, with the
       NiFloatData it reads. */
    let alpha_anim_of = |mat: usize| -> Option<AlphaAnim> {
        let mut ci = *rec.mats.material_ctrl.get(mat)?;
        for _ in 0..16 {
            if ci < 0 || ci as usize >= rec.types.len() {
                return None;
            }
            if let Some(Some((next, flags, frequency, phase, start, stop, _target, data))) = rec.alphactrls.get(ci as usize) {
                if let Some(Some(keys)) = rec.floatdatas.get((*data).max(0) as usize) {
                    if !keys.is_empty() {
                        return Some(AlphaAnim { flags: *flags, frequency: *frequency, phase: *phase, start: *start, stop: *stop, keys: keys.clone() });
                    }
                }
                ci = *next;
                continue;
            }
            // Some other controller on the chain: its `next` is the first field, read raw.
            if let Some(Some(k)) = rec.kfctrls.get(ci as usize) {
                ci = k.next;
                continue;
            }
            if let Some(Some(u)) = rec.uvctrls.get(ci as usize) {
                ci = u.0;
                continue;
            }
            return None;
        }
        None
    };

    /* The visibility animation over a shape: every NiVisController whose target is the
       shape or one of its ancestors, switched on and keyed. Found by the controller's
       own `target`, as the UV animation is. */
    let vis_of = |shape: i32| -> Vec<AlphaAnim> {
        let mut up = Vec::new();
        let mut at = shape;
        for _ in 0..64 {
            if at < 0 {
                break;
            }
            up.push(at);
            at = parent.get(at as usize).copied().unwrap_or(-1);
        }
        let mut out = Vec::new();
        for c in rec.visctrls.iter().flatten() {
            if c.flags & 8 == 0 || !up.contains(&c.target) {
                continue;
            }
            if let Some(Some(keys)) = rec.visdatas.get(c.data.max(0) as usize) {
                if !keys.is_empty() {
                    out.push(AlphaAnim { flags: c.flags, frequency: c.frequency, phase: c.phase, start: c.start, stop: c.stop, keys: keys.clone() });
                }
            }
        }
        out
    };

    let mut out: Vec<DrawPart> = Vec::new();
    // Each entry carries the properties inherited from above it, the UV animation, which
    // branch of a day/night switch it is under (round 17x; 0 for none), and (round 17y)
    // the animated nodes passed on the way down — the composed transform restarts at
    // each of them, so a shape's geometry is baked relative to the nearest one.
    let mut stack: Vec<(i32, [f32; 12], f32, u32, Vec<i32>, Vec<(u32, UvAnim)>, u8, Vec<AnimLink>, Option<(i32, u32)>)> =
        vec![(0, ident, 1.0, 0, Vec::new(), Vec::new(), 0, Vec::new(), None)];
    let mut guard = 0u32;

    while let Some((idx, trafo, scale, depth, inherited, in_uv, dn, mut chain, in_env)) = stack.pop() {
        guard += 1;
        if guard > 400_000 || depth > 64 || idx < 0 {
            continue;
        }
        let Some(n) = rec.nodes.get(idx as usize).and_then(|n| n.as_ref()) else { continue };
        let ty = kind_of(idx);
        /* A billboard node turns its subtree to face the camera; a still preview cannot,
           so glows and flares hanging off a static are left out. A billboard *root* is
           the whole mesh (round 17c: `BM_Snow_01.nif`, the game's snowflake, is one) and
           is walked as a plain node - whoever draws it turns it. */
        if ty == "AvoidNode"
            || ty == "RootCollisionNode"
            || (ty == "NiBillboardNode" && idx != 0)
            || ty == "NiBSParticleNode"
            || is_dynamic_effect(ty)
        {
            continue;
        }
        if n.flags & CULLED_FLAG != 0 {
            continue;
        }
        /* Marker geometry is skipped whether or not the file sets `MRK`.
         *
         * `MRK` is the root flag that tells the game to hide it, and honouring only that
         * is the letter of the format. It is not what a groundcover tool is for: an
         * effect mesh built on a copy of the marker — mist, a light glow, a sound
         * trigger's box — is not something grass grows around and not something anybody
         * wants filling the preview. The shape's *name* is the modder saying what it is,
         * and it survives the copy that the `MRK` flag does not. */
        if is_editor_marker(&n.name) {
            continue;
        }
        /* Round 17y: a node that moves. Its resting transform is *not* composed in — the
           page puts the animated one there — so everything below it is baked in its own
           frame, and the transform gathered so far (root to its parent) travels with the
           link. The root is never a link: the game ignores the root's transform, and its
           animation with it. */
        let (m, s) = match if idx != 0 { kf_of(idx) } else { None } {
            Some((ctrl, data)) => {
                chain.push(AnimLink { pre: (trafo, scale), local: (n.trafo, n.scale), ctrl, data });
                (ident, 1.0)
            }
            None => node_trafo(idx, &trafo, scale, n),
        };
        // Nearest wins: a shape's own property overrides one further up the tree.
        let mut props = n.props.clone();
        props.extend_from_slice(&inherited);

        // One child, not all of them. A switch node draws whichever branch is active;
        // a level-of-detail node draws one level. Walking every child would draw the
        // same tree three times, once per detail level, stacked on itself.
        // Nearest wins here too: a shape's own controller beats the one above it.
        let uv = {
            let own = uv_anims_of(idx);
            if own.is_empty() { in_uv } else { own }
        };
        /* Round 18dl: the environment map. A NiTextureEffect in a node's effect list
           covers everything below the node (OpenMW merges it into each child's state,
           which the subtree inherits); the nearest one down the tree wins. */
        let env = n
            .effects
            .iter()
            .filter_map(|&e| rec.texeffects.get(e.max(0) as usize).copied().flatten())
            .next()
            .or(in_env);

        if ty == "NiSwitchNode" || ty == "NiLODNode" {
            /* Round 17x: Glow in the Dahrk's switch. A NiSwitchNode called NightDaySwitch
               holds the window as it looks by day ("OFF"), lit at night ("ON") and, for an
               interior, with the sun coming through ("INT-DAY"); the game's MWSE add-on and
               OpenMW both pick the branch by the clock. Every branch is walked here, each
               part tagged with which it belongs to, and the page shows one at a time. The
               names are the mod's own convention, matched the way its Lua does — by name,
               falling back to the first three positions. */
            if ty == "NiSwitchNode" && n.name.eq_ignore_ascii_case("NightDaySwitch") {
                for (k, &c) in n.children.iter().enumerate().rev() {
                    let name = rec.nodes.get(c.max(0) as usize).and_then(|x| x.as_ref()).map(|x| x.name.to_ascii_lowercase()).unwrap_or_default();
                    let tag: u8 = match name.as_str() {
                        "off" => 1,
                        "on" => 2,
                        "int-day" => 3,
                        _ => match k { 0 => 1, 1 => 2, 2 => 3, _ => 0 },
                    };
                    if tag != 0 {
                        stack.push((c, m, s, depth + 1, props.clone(), uv.clone(), tag, chain.clone(), env));
                    }
                }
                continue;
            }
            if let Some(&c) = n.children.get(n.active_index) {
                stack.push((c, m, s, depth + 1, props, uv, dn, chain, env));
            }
            continue;
        }

        if is_shape(&n.kind) && n.data_ref >= 0 {
            let Some(d) = rec.datas.get(n.data_ref as usize).and_then(|o| o.as_ref()) else {
                continue;
            };
            if d.verts.is_empty() || d.tris.is_empty() {
                continue;
            }
            // A skinned shape's data is already in the mesh's own space (`skin_records`).
            let (m, s) = if n.skinned { (ident, 1.0) } else { (m, s) };
            let mut part = DrawPart {
                name: n.name.clone(),
                diffuse: [1.0, 1.0, 1.0],
                ambient: [1.0, 1.0, 1.0],
                mat_alpha: 1.0,
                vcol_mode: 2,
                ..Default::default()
            };
            let mut base_uv_set = 0u32;
            let mut bump_seen = false;
            let mut detail_uv_set = 0u32;
            let mut dark_uv_set = 0u32;
            let mut decal_uv_set = 0u32;
            let mut vcol_seen = false;
            /* Round 18be (G4, G5): "have I been told yet", the way `vcol_seen` already
               did it. Every one of these used to ask the *value* whether it had been
               set, and every one of them had a sentinel a real record can write:
               `[1,1,1]` with alpha 1 is the commonest material in Morrowind, alpha
               flags 0 is how an exporter switches blending off under a blended parent,
               and stencil mode 0 is "draw both sides, test nothing". So a shape's own
               material lost to its parent's, while the alpha *controller* came from the
               first material seen — one material's colour with another's fade. */
            let mut mat_seen = false;
            let mut alpha_seen = false;
            let mut stencil_seen = false;
            let mut zbuf_seen = false;
            let mut alpha_anim: Option<AlphaAnim> = None;
            for p in &props {
                let pi = *p;
                if pi < 0 {
                    continue;
                }
                let pi = pi as usize;
                if part.tex.is_empty() {
                    if let Some(Some(src)) = rec.mats.tex_source.get(pi) {
                        if let Some(Some(f)) = rec.mats.tex_file.get(*src as usize) {
                            part.tex = f.clone();
                            // Round 18dj: the UV set it reads, for its controller.
                            if let Some(Some(u)) = rec.mats.base_uv_set.get(pi) {
                                base_uv_set = *u;
                            }
                            // Round 18df: and which of its edges are clamped.
                            if let Some(Some(c)) = rec.mats.base_clamp.get(pi) {
                                part.clamp_st = match *c {
                                    0 => 3,
                                    1 => 1,
                                    2 => 2,
                                    _ => 0,
                                };
                            }
                        }
                    }
                }
                /* Round 18dl: the bump map, from the nearest texturing property that
                   has one. Read whether or not an environment map is in force here;
                   whether it is kept is decided below, since it only ever perturbs one. */
                if !bump_seen {
                    if let Some(Some((src, _uv_set, ls, lo, m))) = rec.mats.bump_source.get(pi) {
                        if let Some(Some(f)) = rec.mats.tex_file.get(*src as usize) {
                            bump_seen = true;
                            part.bump_tex = f.clone();
                            part.bump_luma = [*ls, *lo];
                            part.bump_mat = *m;
                        }
                    }
                }
                if part.decal_tex.is_empty() {
                    if let Some(Some((src, uv_set))) = rec.mats.decal_source.get(pi) {
                        if let Some(Some(f)) = rec.mats.tex_file.get(*src as usize) {
                            part.decal_tex = f.clone();
                            decal_uv_set = *uv_set;
                        }
                    }
                }
                if part.detail_tex.is_empty() {
                    if let Some(Some((src, uv_set))) = rec.mats.detail_source.get(pi) {
                        if let Some(Some(f)) = rec.mats.tex_file.get(*src as usize) {
                            part.detail_tex = f.clone();
                            detail_uv_set = *uv_set;
                        }
                    }
                }
                if part.gloss_tex.is_empty() {
                    if let Some(Some((src, _))) = rec.mats.gloss_source.get(pi) {
                        if let Some(Some(f)) = rec.mats.tex_file.get(*src as usize) {
                            part.gloss_tex = f.clone();
                        }
                    }
                }
                if part.glow_tex.is_empty() {
                    if let Some(Some((src, uv_set))) = rec.mats.glow_source.get(pi) {
                        if let Some(Some(f)) = rec.mats.tex_file.get(*src as usize) {
                            part.glow_tex = f.clone();
                            part.glow_uv_set = *uv_set;
                        }
                    }
                }
                if part.dark_tex.is_empty() {
                    if let Some(Some((src, uv_set))) = rec.mats.dark_source.get(pi) {
                        if let Some(Some(f)) = rec.mats.tex_file.get(*src as usize) {
                            part.dark_tex = f.clone();
                            dark_uv_set = *uv_set;
                        }
                    }
                }
                if let Some(Some((diffuse, a, emissive, ambient))) = rec.mats.material.get(pi) {
                    if !mat_seen {
                        mat_seen = true;
                        part.diffuse = *diffuse;
                        part.ambient = *ambient;
                        part.mat_alpha = *a;
                        part.emissive = *emissive;
                        /* Round 17y: and whether that alpha is animated — from this
                           material, the one whose colour we just took, rather than from
                           whichever one happened to be first. */
                        alpha_anim = alpha_anim_of(pi);
                    }
                }
                /* Round 17w: the nearest NiVertexColorProperty decides whether the shape
                   is lit at all. Nearest first, like every other property here — the
                   shape's own list comes before what it inherited. */
                if !vcol_seen {
                    if let Some(Some((vmode, lmode))) = rec.mats.vcol_prop.get(pi) {
                        vcol_seen = true;
                        part.vcol_mode = (*vmode).min(2) as u8;
                        part.unlit = *lmode == 0;
                    }
                }
                if !stencil_seen {
                    if let Some(Some(dm)) = rec.mats.stencil.get(pi) {
                        stencil_seen = true;
                        part.draw_mode = (*dm).min(3) as u8;
                    }
                }
                if !zbuf_seen {
                    if let Some(Some(zf)) = rec.mats.zbuffer.get(pi) {
                        zbuf_seen = true;
                        part.no_zwrite = zf & 2 == 0;
                    }
                }
                if let Some(Some((flags, threshold))) = rec.mats.alpha.get(pi) {
                    if !alpha_seen {
                        alpha_seen = true;
                        part.alpha_flags = *flags;
                        // Bit 9 is "alpha testing enabled"; without it the threshold
                        // in the record is not the one the engine uses.
                        if flags & 0x200 != 0 {
                            part.alpha_threshold = Some(*threshold);
                        }
                    }
                }
            }
            if part.tex.is_empty() {
                part.tex = fallback_tex.clone();
            }

            let nv = d.verts.len();
            part.pos.reserve(nv * 3);
            part.nrm.reserve(nv * 3);
            part.uv.reserve(nv * 2);
            for (v, vert) in d.verts.iter().enumerate() {
                part.pos.push((m[0] * vert[0] + m[1] * vert[1] + m[2] * vert[2]) * s + m[9]);
                part.pos.push((m[3] * vert[0] + m[4] * vert[1] + m[5] * vert[2]) * s + m[10]);
                part.pos.push((m[6] * vert[0] + m[7] * vert[1] + m[8] * vert[2]) * s + m[11]);
                // Normals rotate but do not translate or scale.
                let nrm = d.normals.get(v).copied().unwrap_or([0.0, 0.0, 1.0]);
                part.nrm.push(m[0] * nrm[0] + m[1] * nrm[1] + m[2] * nrm[2]);
                part.nrm.push(m[3] * nrm[0] + m[4] * nrm[1] + m[5] * nrm[2]);
                part.nrm.push(m[6] * nrm[0] + m[7] * nrm[1] + m[8] * nrm[2]);
                let uv = d.uvs.get(v).copied().unwrap_or([0.0, 0.0]);
                part.uv.push(uv[0]);
                part.uv.push(uv[1]);
            }
            /* The detail map's own UVs. A detail map that names UV set 1 on a shape that
               only has one set reads the set it has, which is what the game does. */
            let uv_set_of = |set: u32| -> &Vec<[f32; 2]> {
                if set >= 1 {
                    if let Some(s) = d.uv_more.get(set as usize - 1) {
                        if s.len() == nv {
                            return s;
                        }
                    }
                }
                &d.uvs
            };
            if !part.detail_tex.is_empty() {
                let src = uv_set_of(detail_uv_set);
                part.uv2.reserve(nv * 2);
                for v in 0..nv {
                    let uv = src.get(v).copied().unwrap_or([0.0, 0.0]);
                    part.uv2.push(uv[0]);
                    part.uv2.push(uv[1]);
                }
            }
            // Round 17y: the dark map's own UVs, the same way.
            if !part.dark_tex.is_empty() {
                let src = uv_set_of(dark_uv_set);
                part.uv_dark.reserve(nv * 2);
                for v in 0..nv {
                    let uv = src.get(v).copied().unwrap_or([0.0, 0.0]);
                    part.uv_dark.push(uv[0]);
                    part.uv_dark.push(uv[1]);
                }
            }
            // Round 18df: the decal's own UVs, the same way.
            if !part.decal_tex.is_empty() {
                let src = uv_set_of(decal_uv_set);
                part.uv_decal.reserve(nv * 2);
                for v in 0..nv {
                    let uv = src.get(v).copied().unwrap_or([0.0, 0.0]);
                    part.uv_decal.push(uv[0]);
                    part.uv_decal.push(uv[1]);
                }
            }
            if d.colours.len() == nv {
                part.col.reserve(nv * 3);
                for c in &d.colours {
                    part.col.extend_from_slice(c);
                }
                if d.alphas.len() == nv {
                    part.col_alpha = d.alphas.clone();
                }
            }
            part.idx.reserve(d.tris.len() * 3);
            for t in &d.tris {
                part.idx.extend_from_slice(t);
            }
            /* Round 18dj: each map takes the controller of the UV set it reads - the
               base map's nearly always set 0 and the only one there is; the dark, detail,
               glow and decal maps their own, when a controller names it. */
            let anim_for = |set: u32| -> Option<UvAnim> { uv.iter().find(|(t, _)| *t == set).map(|(_, a)| a.clone()) };
            part.uv_anim = anim_for(base_uv_set);
            let mut others: Vec<(u32, u8, UvAnim)> = Vec::new();
            for (has, set, bit) in [
                (!part.dark_tex.is_empty(), dark_uv_set, 1u8),
                (!part.detail_tex.is_empty(), detail_uv_set, 2),
                (!part.glow_tex.is_empty(), part.glow_uv_set, 4),
                (!part.decal_tex.is_empty(), decal_uv_set, 8),
            ] {
                if !has {
                    continue;
                }
                if let Some(e) = others.iter_mut().find(|(t, _, _)| *t == set) {
                    e.1 |= bit;
                } else if let Some(a) = anim_for(set) {
                    others.push((set, bit, a));
                }
            }
            part.uv_anims_other = others.into_iter().map(|(_, bits, a)| (bits, a)).collect();
            /* Round 18dl: the environment map, when a NiTextureEffect above names one the
               file has; the bump map rides only with it - alone it perturbs nothing (D3D's
               bump stage feeds the stage after it, and OpenMW reads it inside the
               environment map's branch). */
            match env.and_then(|(src, clamp)| rec.mats.tex_file.get(src.max(0) as usize).cloned().flatten().map(|f| (f, clamp))) {
                Some((f, clamp)) => {
                    part.env_tex = f;
                    part.env_clamp = clamp;
                }
                None => {
                    part.bump_tex.clear();
                    part.bump_luma = [1.0, 0.0];
                    part.bump_mat = [1.0, 0.0, 0.0, 1.0];
                }
            }
            part.day_night = dn;
            /* Round 18au: the shape's morph targets, baked into the part's space the way
               the positions were — rotated and scaled, never translated, since an offset
               is a difference of positions. A target with no keys never weighs anything
               and is left out; morph 0 is the base pose and is never a target. */
            if let Some((c, md)) = morph_of(idx) {
                if c.flags & 8 != 0 {
                    part.morph = morph_anim(c, md, nv, &m, s);
                }
            }
            // A skinned shape is already in the mesh's space (`skin_records`), so no
            // node above it moves it here — its bones are a different question.
            let chain = if n.skinned { Vec::new() } else { chain };
            let vis = vis_of(idx);
            if !chain.is_empty() || alpha_anim.is_some() || !vis.is_empty() {
                part.anim = Some(PartAnim { nodes: chain, alpha: alpha_anim, vis });
            }
            /* Round 18ag: and here is that question. Under an animated skeleton the
               shape keeps its skin-space data and its bones, each bone with the
               animated nodes above it, and the page moves it every frame the way the
               bind pose was made once - `skin_records`' sum, with `World(bone)` read off
               the clock. */
            if n.skinned {
                if let Some(Some(src)) = rec.skin_src.get(n.data_ref as usize) {
                    part.skin = skin_anim(rec, &parent, &kf_of, src);
                }
            }
            out.push(part);
            continue;
        }
        for &c in n.children.iter().rev() {
            stack.push((c, m, s, depth + 1, props.clone(), uv.clone(), dn, chain.clone(), env));
        }
    }
    out
}

/// Round 18au: a shape's morph animation as the page moves it — see [`MorphAnim`].
fn morph_anim(c: &MorphCtrl, md: &MorphData, nv: usize, m: &[f32; 12], s: f32) -> Option<MorphAnim> {
    if md.morphs.len() < 2 {
        return None;
    }
    let base = &md.morphs[0].1;
    let mut targets = Vec::new();
    for (keys, vs) in md.morphs.iter().skip(1) {
        if keys.is_empty() || vs.len() != nv || (!md.relative && base.len() != nv) {
            continue;
        }
        let mut offsets = Vec::with_capacity(nv * 3);
        for (v, p) in vs.iter().enumerate() {
            let o = if md.relative { *p } else { [p[0] - base[v][0], p[1] - base[v][1], p[2] - base[v][2]] };
            offsets.push((m[0] * o[0] + m[1] * o[1] + m[2] * o[2]) * s);
            offsets.push((m[3] * o[0] + m[4] * o[1] + m[5] * o[2]) * s);
            offsets.push((m[6] * o[0] + m[7] * o[1] + m[8] * o[2]) * s);
        }
        targets.push(MorphTarget { keys: keys.clone(), offsets });
    }
    if targets.is_empty() {
        return None;
    }
    Some(MorphAnim { flags: c.flags, frequency: c.frequency, phase: c.phase, start: c.start, stop: c.stop, targets })
}



/// One particle system of a NIF, as the viewport runs it (round 17).
#[derive(Clone, Debug)]
pub struct ParticleSystem {
    pub name: String,
    pub tex: String,
    pub alpha_flags: u16,
    pub mat_alpha: f32,
    pub diffuse: [f32; 3],
    /// Round 17x: the material's emissive colour. A flame's is white — which is why a torch
    /// burns at full brightness at midnight while the smoke beside it takes the ambient.
    pub emissive: [f32; 3],
    pub ctrl: ParticleCtrl,
    /// The particle node's transform in the mesh's space, and the emitter node's.
    pub node: ([f32; 12], f32),
    pub emitter: ([f32; 12], f32),
    pub data: Option<ParticleData>,
    pub gravity: Vec<([f32; 3], [f32; 3], f32, u32, f32)>, // direction, position, force, kind, decay
    pub grow_fade: Option<(f32, f32)>,
    pub colour_keys: Vec<[f32; 5]>,
    pub rotation: Option<f32>,
}

/// Every particle system in the file, with the transforms of their nodes and emitters
/// in the mesh's own space. The walk goes *through* NiBSParticleNodes here — the draw
/// walk stops at them, because they hold nothing a still preview draws.
pub fn parse_particles(buf: &[u8]) -> Vec<ParticleSystem> {
    let Some(rec) = read_records(buf) else { return Vec::new() };
    particles_of(&rec)
}

/// The same, from records already read (round 18cr: `mesh_payload` reads a file once).
fn particles_of(rec: &Records) -> Vec<ParticleSystem> {
    let n = rec.nodes.len();
    // World transforms of every node, by index.
    let mut world: Vec<Option<([f32; 12], f32)>> = vec![None; n];
    let ident: [f32; 12] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0];
    let mut stack: Vec<(i32, [f32; 12], f32, u32, Vec<i32>)> = vec![(0, ident, 1.0, 0, Vec::new())];
    let mut props_of: Vec<Vec<i32>> = vec![Vec::new(); n];
    let mut guard = 0u32;
    while let Some((idx, trafo, scale, depth, inherited)) = stack.pop() {
        guard += 1;
        if guard > 400_000 || depth > 64 || idx < 0 || idx as usize >= n {
            continue;
        }
        let Some(nd) = rec.nodes[idx as usize].as_ref() else { continue };
        let (m, s) = node_trafo(idx, &trafo, scale, nd);
        world[idx as usize] = Some((m, s));
        let mut props = nd.props.clone();
        props.extend_from_slice(&inherited);
        props_of[idx as usize] = props.clone();
        /* Round 17y: one branch of a switch, as the draw walk takes one. Ashfall's kettles
           keep their steam under `SWITCH_KETTLE_STEAM` → `ON`, which the mod's script
           turns on while the water boils; a kettle placed in a cell sits on `OFF`, and so
           did the game's, steamless, while the preview boiled every one of them. The
           day/night switch is the exception here as it is there: every branch is walked. */
        if nd.kind == "NiSwitchNode" && !nd.name.eq_ignore_ascii_case("NightDaySwitch") {
            if let Some(&c) = nd.children.get(nd.active_index) {
                stack.push((c, m, s, depth + 1, props.clone()));
            }
            continue;
        }
        for &c in &nd.children {
            stack.push((c, m, s, depth + 1, props.clone()));
        }
    }
    let mut out = Vec::new();
    for (i, nd) in rec.nodes.iter().enumerate() {
        let Some(nd) = nd else { continue };
        if !(nd.kind == "NiAutoNormalParticles" || nd.kind == "NiRotatingParticles" || nd.kind == "NiParticles") {
            continue;
        }
        // Not reached by the walk: under a switch's inactive branch (round 17y).
        if world[i].is_none() {
            continue;
        }
        /* The node's first controller, when it is a particle controller. This was
           written as a walk down the controller chain that never took a second step -
           the "read the other type's `next` raw" arm was a `break` - and clippy's
           `never_loop` said so. Said plainly now: the chain is not followed. No vanilla
           particle node has another controller ahead of its particle controller, and the
           other controller types this reader knows are found by their target rather than
           by link (`draw_parts`), so a second link here would be a new case to add, not a
           loop to unroll. */
        let ci = nd.ctrl;
        let ctrl = if ci >= 0 && (ci as usize) < n { rec.pctrls[ci as usize].clone() } else { None };
        let Some(ctrl) = ctrl else { continue };
        let mut sys = ParticleSystem {
            name: nd.name.clone(),
            tex: String::new(),
            alpha_flags: 0,
            mat_alpha: 1.0,
            emissive: [0.0; 3],
            diffuse: [1.0, 1.0, 1.0],
            node: world[i].unwrap_or((ident, 1.0)),
            emitter: (ident, 1.0),
            data: nd.data_ref.try_into().ok().and_then(|d: usize| rec.pdatas.get(d).cloned().flatten()),
            ctrl: ctrl.clone(),
            gravity: Vec::new(),
            grow_fade: None,
            colour_keys: Vec::new(),
            rotation: None,
        };
        if ctrl.emitter >= 0 && (ctrl.emitter as usize) < n {
            sys.emitter = world[ctrl.emitter as usize].unwrap_or((ident, 1.0));
        }
        let mut mat_seen = false;
        let mut alpha_seen = false;
        for p in &props_of[i] {
            if *p < 0 {
                continue;
            }
            let pi = *p as usize;
            if sys.tex.is_empty() {
                if let Some(Some(src)) = rec.mats.tex_source.get(pi) {
                    if let Some(Some(f)) = rec.mats.tex_file.get(*src as usize) {
                        sys.tex = f.clone();
                    }
                }
            }
            if let Some(Some((diffuse, a, emissive, _ambient))) = rec.mats.material.get(pi) {
                // Round 18be (G4): "have I been told yet" — see `draw_parts`.
                if !mat_seen {
                    mat_seen = true;
                    sys.diffuse = *diffuse;
                    sys.mat_alpha = *a;
                    sys.emissive = *emissive;
                }
            }
            if let Some(Some((flags, _))) = rec.mats.alpha.get(pi) {
                if !alpha_seen {
                    alpha_seen = true;
                    sys.alpha_flags = *flags;
                }
            }
        }
        // The modifier chain.
        let mut mi = ctrl.modifier;
        for _ in 0..32 {
            if mi < 0 || mi as usize >= n {
                break;
            }
            let Some(m) = rec.pmods[mi as usize].as_ref() else { break };
            mi = match m {
                ParticleMod::Gravity { next, decay, force, kind, position, direction } => {
                    sys.gravity.push((*direction, *position, *force, *kind, *decay));
                    *next
                }
                ParticleMod::GrowFade { next, grow, fade } => {
                    sys.grow_fade = Some((*grow, *fade));
                    *next
                }
                ParticleMod::Colour { next, data } => {
                    if *data >= 0 && (*data as usize) < n {
                        if let Some(keys) = rec.colour_data[*data as usize].as_ref() {
                            sys.colour_keys = keys.clone();
                        }
                    }
                    *next
                }
                ParticleMod::Rotation { next, speed, .. } => {
                    sys.rotation = Some(*speed);
                    *next
                }
                ParticleMod::Other { next } => *next,
            };
        }
        out.push(sys);
    }
    out
}


/// The JSON for systems already parsed (round 18cr).
pub fn particles_json_of(systems: &[ParticleSystem]) -> String {
    use crate::toml::num;
    let v3 = |v: &[f32; 3]| format!("[{},{},{}]", num(v[0]), num(v[1]), num(v[2]));
    let m12 = |m: &[f32; 12]| {
        let parts: Vec<String> = m.iter().map(|x| num(*x)).collect();
        format!("[{}]", parts.join(","))
    };
    let mut out = String::from("[");
    for (k, s) in systems.iter().enumerate() {
        if k > 0 {
            out.push(',');
        }
        let c = &s.ctrl;
        let mut o = String::from("{");
        o.push_str(&format!("\"name\":\"{}\",\"tex\":\"{}\"", escape(&s.name), escape(&s.tex)));
        o.push_str(&format!(",\"alphaFlags\":{},\"matAlpha\":{},\"diffuse\":{},\"emissive\":{}", s.alpha_flags, num(s.mat_alpha), v3(&s.diffuse), v3(&s.emissive)));
        o.push_str(&format!(",\"node\":{},\"nodeScale\":{},\"emitter\":{},\"emitterScale\":{}", m12(&s.node.0), num(s.node.1), m12(&s.emitter.0), num(s.emitter.1)));
        o.push_str(&format!(
            ",\"velocity\":{},\"velocityRandom\":{},\"verticalDir\":{},\"verticalAngle\":{},\"horizontalDir\":{},\"horizontalAngle\":{}",
            num(c.velocity), num(c.velocity_random), num(c.vertical_dir), num(c.vertical_angle), num(c.horizontal_dir), num(c.horizontal_angle)
        ));
        o.push_str(&format!(
            ",\"color\":[{},{},{},{}],\"size\":{},\"emitStart\":{},\"emitStop\":{},\"emitRate\":{},\"lifetime\":{},\"lifetimeRandom\":{},\"emitFlags\":{},\"offsetRandom\":{}",
            num(c.colour[0]), num(c.colour[1]), num(c.colour[2]), num(c.colour[3]), num(c.size), num(c.emit_start), num(c.emit_stop),
            num(c.emit_rate), num(c.lifetime), num(c.lifetime_random), c.emit_flags, v3(&c.offset_random)
        ));
        o.push_str(&format!(",\"ctrlFlags\":{},\"frequency\":{},\"start\":{},\"stop\":{},\"numParticles\":{},\"numValid\":{}", c.flags, num(c.frequency), num(c.start), num(c.stop), c.num_particles, c.num_valid));
        match &s.data {
            Some(d) => {
                let mean = if d.sizes.is_empty() { 1.0 } else { d.sizes.iter().sum::<f32>() / d.sizes.len() as f32 };
                let mean_alpha = if d.colours.is_empty() { 1.0 } else { d.colours.iter().map(|c| c[3]).sum::<f32>() / d.colours.len() as f32 };
                o.push_str(&format!(",\"data\":{{\"nverts\":{},\"radius\":{},\"meanSize\":{},\"meanAlpha\":{},\"rotating\":{}}}", d.nverts, num(d.radius), num(mean), num(mean_alpha), d.rotating));
            }
            None => o.push_str(",\"data\":null"),
        }
        o.push_str(",\"gravity\":[");
        for (gi, g) in s.gravity.iter().enumerate() {
            if gi > 0 {
                o.push(',');
            }
            o.push_str(&format!("{{\"direction\":{},\"position\":{},\"force\":{},\"kind\":{},\"decay\":{}}}", v3(&g.0), v3(&g.1), num(g.2), g.3, num(g.4)));
        }
        o.push(']');
        match s.grow_fade {
            Some((g, f)) => o.push_str(&format!(",\"grow\":{},\"fade\":{}", num(g), num(f))),
            None => o.push_str(",\"grow\":null,\"fade\":null"),
        }
        o.push_str(",\"colorKeys\":[");
        for (ki, k) in s.colour_keys.iter().enumerate() {
            if ki > 0 {
                o.push(',');
            }
            o.push_str(&format!("[{},{},{},{},{}]", num(k[0]), num(k[1]), num(k[2]), num(k[3]), num(k[4])));
        }
        o.push(']');
        match s.rotation {
            Some(r) => o.push_str(&format!(",\"rotation\":{}", num(r))),
            None => o.push_str(",\"rotation\":null"),
        }
        o.push('}');
        out.push_str(&o);
    }
    out.push(']');
    out
}

fn escape(s: &str) -> String {
    let mut o = String::new();
    for ch in s.chars() {
        match ch {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o
}

/// The drawable parts, and where a light attached to this mesh belongs (round 17m).
///
/// Morrowind hangs a LIGH record's light on a node named `AttachLight` when the mesh has
/// one, and on the mesh's origin when it does not — OpenMW's `SceneUtil::addLight`:
/// `FindByNameVisitor visitor("AttachLight"); ... attachTo = visitor.mFoundNode ?
/// visitor.mFoundNode : node;`. 88 of the 131 `light_*.nif` in Morrowind.bsa carry one,
/// and it is what puts a street lamp's glow at the flame rather than at the foot of the
/// post. Returned as a translation in the mesh's own space, with the chain above it
/// applied.
/// Round 18ag: `parse_draw_lit` with the mesh's `.kf` bound to its nodes, so a parked
/// creature's parts carry their idle and its skinned shapes their bones.
/// Round 18cr: everything the viewport's mesh bundle wants from one file, read once.
///
/// `mesh_payload` read the same bytes up to four times - the draw parts, the particle
/// systems, the systems again for their JSON, and the light under AttachLight - and a
/// forty-nine cell load asks for five hundred meshes. One read now, when there is no
/// `.kf` to bind; with one, the particle and light walks read the file *unposed* as they
/// always have, so an animated mesh's answer is the answer it was.
pub struct MeshRead {
    pub parts: Vec<DrawPart>,
    pub attach: Option<[f32; 3]>,
    pub systems: Vec<ParticleSystem>,
    pub light: Option<([f32; 3], f32)>,
}

pub fn read_for_draw(buf: &[u8], kf: Option<&KfSequence>) -> Option<MeshRead> {
    let rec = read_records_kf(buf, kf)?;
    let attach = attach_light(&rec);
    let parts = draw_parts(&rec);
    let (systems, light) = if kf.is_some() {
        (parse_particles(buf), if attach.is_some() { attached_light(buf) } else { None })
    } else {
        (particles_of(&rec), if attach.is_some() { attached_light_of(&rec) } else { None })
    };
    Some(MeshRead { parts, attach, systems, light })
}



/// Wraithguard: everything the viewer wants from one mesh file - what `read_for_draw`
/// gives and the collision geometry `parse_with_kf` builds - from one read of the
/// records. The viewer used to read each drawn mesh twice (once to draw it, once for its
/// collision); the records are read once here and handed to both builders.
pub fn read_for_draw_and_geom(buf: &[u8], kf: Option<&KfSequence>) -> Option<(MeshRead, Option<MeshGeom>)> {
    let rec = read_records_kf(buf, kf)?;
    let attach = attach_light(&rec);
    let parts = draw_parts(&rec);
    let (systems, light) = if kf.is_some() {
        (parse_particles(buf), if attach.is_some() { attached_light(buf) } else { None })
    } else {
        (particles_of(&rec), if attach.is_some() { attached_light_of(&rec) } else { None })
    };
    let geom = geom_of(rec);
    Some((MeshRead { parts, attach, systems, light }, geom))
}

/// Round 17y: the light hanging under `AttachLight`, when the mesh carries one — its
/// diffuse colour and its radius, which Glow in the Dahrk keeps as the light node's
/// scale. `None` for a mesh with no such node, or with nothing under it.
pub fn attached_light(buf: &[u8]) -> Option<([f32; 3], f32)> {
    let rec = read_records(buf)?;
    attached_light_of(&rec)
}

fn attached_light_of(rec: &Records) -> Option<([f32; 3], f32)> {
    let attach = rec
        .nodes
        .iter()
        .flatten()
        .find(|n| n.name.eq_ignore_ascii_case("AttachLight"))?;
    for &c in &attach.children {
        if c < 0 {
            continue;
        }
        if let (Some(Some(n)), Some(Some((diffuse, _dimmer)))) = (rec.nodes.get(c as usize), rec.lights.get(c as usize)) {
            return Some((*diffuse, n.scale));
        }
    }
    None
}



fn attach_light(rec: &Records) -> Option<[f32; 3]> {
    let ident = [1., 0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 0.];
    let mut stack: Vec<(i32, [f32; 12], f32, u32)> = vec![(0, ident, 1.0, 0)];
    let mut guard = 0u32;
    while let Some((idx, trafo, scale, depth)) = stack.pop() {
        guard += 1;
        if guard > 100_000 || depth > 64 || idx < 0 {
            continue;
        }
        let Some(n) = rec.nodes.get(idx as usize).and_then(|n| n.as_ref()) else { continue };
        let (m, s) = node_trafo(idx, &trafo, scale, n);
        if n.name.eq_ignore_ascii_case("AttachLight") {
            return Some([m[9], m[10], m[11]]);
        }
        for &c in &n.children {
            stack.push((c, m, s, depth + 1));
        }
    }
    None
}

fn read_node(r: &mut Reader, ty: &str, has_children: bool) -> Option<Node> {
    let (name, flags, trafo, scale, extra, props) = r.av_object();
    let ctrl = r.last_ctrl;
    let mut children = Vec::new();
    let mut effects = Vec::new();
    let mut data_ref = -1;
    let mut active_index = 0usize;
    if has_children {
        children = r.ref_list();
        effects = r.ref_list();
        // Two node types carry fields past NiNode's own. Records here have no length
        // in front of them, so under-reading one by four bytes does not spoil that
        // record — it spoils every record after it, because the next type name is
        // then read out of the middle of this one.
        match ty {
            "NiSwitchNode" => {
                active_index = r.u32() as usize;
            }
            "NiSortAdjustNode" => {
                r.u32(); // sorting mode
                r.i32(); // sub-sorter
            }
            /* A level-of-detail node: the same subtree at several detail levels, one
               chosen by how far away the camera is. Its children are the levels, in
               order, and level 0 is the nearest — so `active_index` stays 0 and the
               traversal takes the highest-detail one, which is what a preview wants.

               It was missing from this list entirely, which is how `flora_tree_wg_01`
               came to draw nothing at all. Unrecognised records fall through to
               `skip_or_read`, which does not know this one's length either, so `resync`
               scanned forward to the next plausible type name — past the node, and past
               its children with it. Every visible shape in that tree hangs off the LOD
               node, so the whole tree vanished, silently, while its collision geometry
               (which is a sibling of the LOD node, not a child) went on working. */
            "NiLODNode" => {
                /* Sixteen bytes, then the level count, then a near and a far extent per
                   level. Measured out of `flora_tree_wg_02.nif` rather than taken from a
                   format note, because the notes disagree about this record at 4.0.0.2
                   and a record with no length in front of it cannot be re-found if the
                   arithmetic is wrong — it takes every block after it with it.

                   The measurement: after the NiNode part came four zeroed words, then 2
                   (the node has two children), then (0, 9000) and (9000, 100000) — near
                   and far extents that make sense as LOD ranges — and then the value 6,
                   which is the length prefix of the next block's type name, "NiNode".
                   Sixteen plus four plus sixteen lands exactly there. */
                for _ in 0..4 {
                    r.u32(); // LOD centre, and one word beside it, zero in every file seen
                }
                let levels = r.u32();
                // Bounded: a miscount here would otherwise spin through the whole file
                // reading nothing, and a mesh with more than this many levels does not
                // exist.
                for _ in 0..levels.min(64) {
                    r.f32(); // near extent
                    r.f32(); // far extent
                }
            }
            _ => {}
        }
    }
    let mut skin_ref = -1;
    if !has_children {
        data_ref = r.i32();
        skin_ref = r.i32();
    }
    if !r.ok() {
        return None;
    }
    Some(Node {
        kind: ty.to_string(),
        name,
        flags,
        trafo,
        scale,
        children,
        extra,
        data_ref,
        active_index,
        props,
        skin_ref,
        skinned: false,
        ctrl,
        effects,
    })
}

/// Everything a `NiTriBasedGeomData` holds before its faces: the vertices, the normals,
/// the bounding sphere, the vertex colours and the UV sets.
///
/// Round 18bg (§G2): split out, because `NiTriShapeData` and `NiTriStripsData` share all
/// of it and differ only in how they spell their faces. One reader for the shared part is
/// the only way the two cannot drift — and this part is where every length in the record
/// comes from, so a disagreement here would desync the file rather than lose a shape.
fn read_geom_data(r: &mut Reader) -> Option<ShapeData> {
    let nverts = r.u16() as usize;
    if nverts > 2_000_000 {
        return None;
    }
    let has_verts = r.u32();
    let mut verts = Vec::new();
    if has_verts != 0 {
        verts.reserve(nverts);
        for _ in 0..nverts {
            verts.push([r.f32(), r.f32(), r.f32()]);
        }
    }
    let mut normals = Vec::new();
    if r.u32() != 0 {
        normals.reserve(nverts);
        for _ in 0..nverts {
            normals.push([r.f32(), r.f32(), r.f32()]);
        }
    }
    r.skip(16); // bounding sphere: centre + radius
    let mut colours = Vec::new();
    let mut alphas = Vec::new();
    if r.u32() != 0 {
        // Vertex colours: four floats each. Kept as RGB bytes, and the alpha apart.
        colours.reserve(nverts);
        alphas.reserve(nverts);
        for _ in 0..nverts {
            let (cr, cg, cb) = (r.f32(), r.f32(), r.f32());
            /* The fourth is the vertex alpha. Round 17n read and dropped it - a flat 255
               on every shape of `Ex_Vivec_waterfall_03` and on the Ghostfence, and on
               every other vanilla shape, the audit found. Round 18df keeps it: OAAB's
               root pillars, inflow sheets and lava falls fade through it (nif-audit.md
               §1), and a veteran saw them drawn solid. Kept beside the colours, not in
               them, so the payload only grows for a shape that uses it. */
            let ca = r.f32();
            let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            colours.push([q(cr), q(cg), q(cb)]);
            alphas.push(q(ca));
        }
        if alphas.iter().all(|&a| a == 255) {
            alphas.clear();
        }
    }
    let nuv = (r.u16() as usize) & 0x3f;
    let has_uv = r.u32();
    let mut uvs = Vec::new();
    let mut uv_more: Vec<Vec<[f32; 2]>> = Vec::new();
    if has_uv != 0 && nuv > 0 {
        /* Every set is kept (round 17y). Two were, and the rest stepped over - a detail
           map reads the second (round 17h) - until Glow in the Dahrk's windows turned up
           reading a third: base on set 0, the dark map on 1, the window pane on 2. */
        uvs.reserve(nverts);
        for _ in 0..nverts {
            uvs.push([r.f32(), r.f32()]);
        }
        for _ in 1..nuv.min(8) {
            let mut set = Vec::with_capacity(nverts);
            for _ in 0..nverts {
                set.push([r.f32(), r.f32()]);
            }
            uv_more.push(set);
        }
        if nuv > 8 {
            r.skip(nverts * 8 * (nuv - 8));
        }
    }
    if !r.ok() {
        return None;
    }
    Some(ShapeData { verts, tris: Vec::new(), normals, uvs, uv_more, colours, alphas })
}

/// `NiTriShapeData`: the shared geometry, then the faces written out one by one.
fn read_shape_data(r: &mut Reader) -> Option<ShapeData> {
    let mut d = read_geom_data(r)?;
    let _ntris = r.u16();
    // Triangle *points*, i.e. three per triangle. There is no has-triangles
    // flag at this version.
    let npoints = r.u32() as usize;
    if npoints > 6_000_000 || !npoints.is_multiple_of(3) {
        return None;
    }
    d.tris = Vec::with_capacity(npoints / 3);
    for _ in 0..npoints / 3 {
        d.tris.push([r.u16(), r.u16(), r.u16()]);
    }
    let groups = r.u16() as usize;
    for _ in 0..groups {
        let n = r.u16() as usize;
        r.skip(n * 2);
    }
    if !r.ok() {
        return None;
    }
    Some(d)
}

/// `NiTriStripsData`: the shared geometry, then the faces written as triangle strips —
/// round 18bg, bug hunt §G2.
///
/// **What it was.** `NiTriStrips` was in the node dispatch but `NiTriStripsData` was not,
/// so the shape's data reference pointed at a record the reader had resynced past: the
/// shape came back with no vertices and no faces and was dropped in silence. No grass was
/// kept out of it, it grew no grass of its own, and it had no hull — the mesh was simply
/// not there, with nothing said. Measured across 48 meshes of Robin's load order (Remiros'
/// Groundcover, Aesthesia, Graht Swamp Trees, Vurt's Animated Solstheim Trees) not one is
/// stripified, which is why this went unnoticed: Morrowind's own exporters write
/// `NiTriShape` at 4.0.0.2 and the mods follow them. It is the *silence* that made it
/// worth fixing.
///
/// **The layout.** At 4.0.0.2 the faces are a strip count, one length per strip, then that
/// many point indices per strip. There is no `hasPoints` flag (that arrives at 10.0.1.3)
/// and there are no match groups — those belong to `NiTriShapeData` alone.
///
/// **The winding**, which is the part worth getting right. A strip's triangles alternate
/// handedness, so every other one has to be swapped back or the mesh comes out with half
/// its faces inside out — and a face read inside out is a normal pointing into the mesh,
/// which is §H4's failure arriving by another road: blades growing into a rock rather than
/// out of it, and a closed hull that reports the outside as its inside. Even triangles are
/// taken in order, odd ones with their first two points swapped, and a degenerate triangle
/// — two of its points the same, which is how a file joins two strips into one — is
/// dropped rather than kept as a zero-area face. That is what OpenMW does with the same
/// record, deliberately: two readers of one format that disagree is a bug whichever one is
/// wrong.
fn read_strips_data(r: &mut Reader) -> Option<ShapeData> {
    let mut d = read_geom_data(r)?;
    let _ntris = r.u16();
    let nstrips = r.u16() as usize;
    if nstrips > 65_536 {
        return None;
    }
    let mut lengths = Vec::with_capacity(nstrips);
    let mut total = 0usize;
    for _ in 0..nstrips {
        let n = r.u16() as usize;
        total += n;
        lengths.push(n);
    }
    if total > 6_000_000 || !r.ok() {
        return None;
    }
    // At most one triangle per point after the first two of each strip.
    d.tris = Vec::with_capacity(total.saturating_sub(2 * nstrips));
    let mut strip: Vec<u16> = Vec::new();
    for len in lengths {
        strip.clear();
        strip.reserve(len);
        for _ in 0..len {
            strip.push(r.u16());
        }
        if !r.ok() {
            return None;
        }
        for i in 0..strip.len().saturating_sub(2) {
            let (a, b, c) = (strip[i], strip[i + 1], strip[i + 2]);
            // A joined strip carries repeated points to get from one run to the next.
            if a == b || b == c || a == c {
                continue;
            }
            d.tris.push(if i % 2 == 0 { [a, b, c] } else { [b, a, c] });
        }
    }
    if !r.ok() {
        return None;
    }
    Some(d)
}

/// Whether a node is drawable geometry — round 18bg (§G2): `NiTriStrips` is a shape like
/// `NiTriShape`, and by the time anything asks, its strips are already triangles
/// (`read_strips_data`). One predicate rather than three spellings of the same test, so a
/// third kind could never again be read as a node and then quietly ignored as a shape.
fn is_shape(kind: &str) -> bool {
    kind == "NiTriShape" || kind == "NiTriStrips"
}

/// Whether a shape is the Construction Set's editor-marker geometry.
///
/// `Tri EditorMarker` — no space — is the third spelling and the one that matters most:
/// mods build visual effects by editing a copy of the marker mesh and leaving the
/// shape's name alone. `Mistify`'s `mist.nif` is one, a 900-vertex fog volume whose only
/// shape carries that name and which reports a 453-unit box as its collision. Drawn, it
/// is a translucent slab across half the view; avoided, it carves a 453-unit hole in the
/// grass.
fn is_editor_marker(name: &str) -> bool {
    let l = name.to_ascii_lowercase();
    l.starts_with("editormarker")
        || l.starts_with("tri editor marker")
        || l.starts_with("tri editormarker")
}

fn mul(parent: &[f32; 12], ps: f32, child: &[f32; 12], cs: f32) -> ([f32; 12], f32) {
    let mut out = [0f32; 12];
    for row in 0..3 {
        for col in 0..3 {
            let mut acc = 0.0;
            for k in 0..3 {
                acc += parent[row * 3 + k] * child[k * 3 + col];
            }
            out[row * 3 + col] = acc;
        }
    }
    for row in 0..3 {
        let mut acc = 0.0;
        for k in 0..3 {
            acc += parent[row * 3 + k] * child[9 + k];
        }
        out[9 + row] = parent[9 + row] + acc * ps;
    }
    (out, ps * cs)
}

/// Flag bits on a node that decide whether it takes part in collision.
///
/// These are not conventions — they are the bits the engine itself reads, and two
/// of them mean the opposite of what the node's name suggests. A NiCollisionSwitch
/// is not "a node that switches collision off": it collides normally unless its
/// propagate bit is clear. A NiBSParticleNode collides unless it is set to follow.
/// Reading either as a blanket "walk through this" turns solid objects into thin
/// air, and grass grows straight through them.
const CULLED_FLAG: u16 = 0x0001;
const PROPAGATE_FLAG: u16 = 0x0020;
const FOLLOW_FLAG: u16 = 0x0080;

/// A light or a texture effect. Never geometry, never collision.
fn is_dynamic_effect(ty: &str) -> bool {
    matches!(
        ty,
        "NiDynamicEffect"
            | "NiLight"
            | "NiAmbientLight"
            | "NiDirectionalLight"
            | "NiPointLight"
            | "NiSpotLight"
            | "NiTextureEffect"
    )
}

/// Whether this node stops the collision walk, given its own flags.
fn blocks_collision_walk(kind: &str, flags: u16) -> bool {
    if is_dynamic_effect(kind) || kind == "NiBillboardNode" {
        return true;
    }
    if kind == "NiBSParticleNode" {
        return flags & FOLLOW_FLAG != 0;
    }
    if kind == "NiCollisionSwitch" {
        return flags & PROPAGATE_FLAG == 0;
    }
    false
}

/// Walks the extra-data chain hanging off a node and reports the markers on it.
///
/// NC is the one that matters most: it means nothing collides with this mesh at
/// all — the player walks straight through it. Ground flora carries it, and a
/// region carpeted in flora is exactly where treating every plant as solid leaves
/// no room for grass.
#[derive(Default, Clone, Copy)]
struct RootMarkers {
    /// Nothing collides with this mesh at all — the player walks straight through
    /// it. Ground flora carries it, and a region carpeted in flora is exactly where
    /// treating every plant as solid leaves no room for grass.
    nc: bool,
    /// The mesh contains editor markers, which the construction set draws and the
    /// game does not.
    mrk: bool,
    /// The collision node is nested deeper than the root's own children.
    rcn: bool,
}

fn root_markers(extras: &[Option<ExtraString>], head: i32) -> RootMarkers {
    let mut out = RootMarkers::default();
    let mut i = head;
    for _ in 0..64 {
        if i < 0 || i as usize >= extras.len() {
            break;
        }
        let Some(e) = extras[i as usize].as_ref() else { break };
        let v = e.value.trim().to_ascii_lowercase();
        if v.starts_with("nc") {
            out.nc = true;
        } else if v.starts_with("mrk") {
            out.mrk = true;
        } else if v.starts_with("rcn") {
            out.rcn = true;
        }
        i = e.next;
    }
    out
}

fn build(
    nodes: &[Option<Node>],
    datas: &[Option<ShapeData>],
    extras: &[Option<ExtraString>],
    types: &[String],
    // Round 18be (G7): per node, its local transform as its keys put it at the clip's
    // start — see `parse_with_kf`. Shorter than `nodes`, or empty, means "no keys
    // anywhere", which is what every still mesh hands in.
    poses: &[Option<([f32; 12], f32)>],
) -> Option<MeshGeom> {
    let ident = [1., 0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 0.];
    /* A node's world transform: its parent's, then its own — posed by its keys where it
       has any, exactly as `skin_records`' `world_of` does it. The root is never posed,
       because its transform is never applied. */
    let step = |idx: i32, trafo: &[f32; 12], scale: f32, n: &Node| -> ([f32; 12], f32) {
        if idx > 0 {
            if let Some(Some((lm, ls))) = poses.get(idx as usize) {
                return mul(trafo, scale, lm, *ls);
            }
        }
        node_trafo(idx, trafo, scale, n)
    };

    // The engine reads its markers off the root, so that is where they are looked for.
    let marks = nodes
        .first()
        .and_then(|n| n.as_ref())
        .map(|n| root_markers(extras, n.extra))
        .unwrap_or_default();

    let kind_of = |i: i32| -> &str {
        if i < 0 {
            return "";
        }
        types.get(i as usize).map(|s| s.as_str()).unwrap_or("")
    };

    // A collision node, if the mesh has one, replaces the root outright: the engine
    // collides with the hull and ignores everything the hull does not cover. Normally
    // it is one of the root's own children; a mesh marked RCN buries it deeper.
    let mut col_root = -1i32;
    let mut col_trafo = ident;
    let mut col_scale = 1.0f32;
    if let Some(root) = nodes.first().and_then(|n| n.as_ref()) {
        if marks.rcn {
            // Depth-first, carrying the transform down so a nested hull lands in the
            // right place rather than at the mesh origin.
            let mut stack = vec![(0i32, ident, 1.0f32, 0u32)];
            let mut guard = 0u32;
            while let Some((idx, trafo, scale, depth)) = stack.pop() {
                guard += 1;
                if guard > 100_000 || depth > 64 || idx < 0 {
                    continue;
                }
                let Some(n) = nodes.get(idx as usize).and_then(|n| n.as_ref()) else { continue };
                let (m, sc) = step(idx, &trafo, scale, n);
                if kind_of(idx) == "RootCollisionNode" {
                    col_root = idx;
                    /* Round 18be (G1): the hull's *parent* chain, not its own combined
                       transform. `gather` is seeded with this node and applies
                       `node_trafo` to it like any other, so handing it `m` — which
                       already has the node's own local in it — applied that local
                       twice. Measured before the fix: an RCN translated (100,0,0) over
                       a 0..10 box gave a hull at x = 200..210, and RCN scale 2 gave
                       0..40 instead of 0..20. `trafo`/`scale` here are what the parent
                       pushed, which is exactly what the walk needs to arrive at this
                       node's world transform once. */
                    col_trafo = trafo;
                    col_scale = scale;
                    break;
                }
                for &c in n.children.iter().rev() {
                    stack.push((c, m, sc, depth + 1));
                }
            }
        } else {
            for &c in &root.children {
                if kind_of(c) == "RootCollisionNode" {
                    if nodes.get(c as usize).and_then(|n| n.as_ref()).is_some() {
                        col_root = c;
                        // Round 18be (G1): the root's transform is never applied
                        // (`node_trafo` returns the parent's for index 0), so the hull's
                        // parent chain is the identity — and `gather` puts the hull's own
                        // local on top of it. It used to be handed the local as well.
                        col_trafo = ident;
                        col_scale = 1.0;
                    }
                    break;
                }
            }
        }
    }

    // Hulls are routinely authored with the hidden flag set, because they are not
    // meant to be drawn. Honouring that flag inside the hull throws the hull away
    // and quietly falls back to the visible mesh, which is a different shape.
    let skip_culled = col_root < 0;
    let (start, start_trafo, start_scale) =
        if col_root >= 0 { (col_root, col_trafo, col_scale) } else { (0, ident, 1.0) };

    // One walk, two questions. Collision asks what the engine is solid against;
    // visible asks what it draws, and is only needed as a last resort — to measure
    // something that has no collision at all.
    let gather = |start: i32, start_trafo: [f32; 12], start_scale: f32, skip_culled: bool, collision: bool| {
        let mut tris: Vec<[f32; 3]> = Vec::new();
        let mut stack = vec![(start, start_trafo, start_scale, 0u32)];
        let mut guard = 0u32;
        while let Some((idx, trafo, scale, depth)) = stack.pop() {
            guard += 1;
            if guard > 400_000 || depth > 64 || idx < 0 {
                continue;
            }
            let i = idx as usize;
            let Some(n) = nodes.get(i).and_then(|n| n.as_ref()) else { continue };
            let ty = kind_of(idx);
            if ty == "AvoidNode" {
                continue;
            }
            if collision {
                if blocks_collision_walk(ty, n.flags) {
                    continue;
                }
            } else if ty == "RootCollisionNode"
                || ty == "NiBillboardNode"
                || ty == "NiBSParticleNode"
                || is_dynamic_effect(ty)
            {
                continue;
            }
            if (skip_culled || !collision) && n.flags & CULLED_FLAG != 0 {
                continue;
            }
            // Same rule as the draw pass: the shape's name outlives the MRK flag.
            if is_editor_marker(&n.name) {
                continue;
            }
            let (m, s) = step(idx, &trafo, scale, n);

            if ty == "NiSwitchNode" || ty == "NiLODNode" {
                // One child: a switch node has one live branch, a level-of-detail node
                // one live level. The same rule the drawing walk uses, for the same
                // reason — the alternative is counting the same shape several times.
                if let Some(&c) = n.children.get(n.active_index) {
                    stack.push((c, m, s, depth + 1));
                }
                continue;
            }

            if is_shape(&n.kind) && n.data_ref >= 0 {
                if let Some(d) = datas.get(n.data_ref as usize).and_then(|o| o.as_ref()) {
                    // Skinned data is already in the mesh's own space (`skin_records`).
                    let (m, s) = if n.skinned { (ident, 1.0) } else { (m, s) };
                    for t in &d.tris {
                        // Round 18bc (B7): a whole face, or none of it — see `whole_face`.
                        let Some(face) = whole_face(&d.verts, t) else { continue };
                        for v in face {
                            tris.push([
                                (m[0] * v[0] + m[1] * v[1] + m[2] * v[2]) * s + m[9],
                                (m[3] * v[0] + m[4] * v[1] + m[5] * v[2]) * s + m[10],
                                (m[6] * v[0] + m[7] * v[1] + m[8] * v[2]) * s + m[11],
                            ]);
                        }
                    }
                }
                continue;
            }
            for &c in n.children.iter().rev() {
                stack.push((c, m, s, depth + 1));
            }
        }
        tris
    };

    let mut tris = gather(start, start_trafo, start_scale, skip_culled, true);

    // Nothing to collide with — either the mesh says so outright, or the collision
    // walk came back empty. Rather than let grass grow straight through it, stand a
    // box where the object is. Picking flora is the case that matters: it has no
    // collision in game, and burying it in grass makes it unfindable.
    let mut shape =
        if col_root >= 0 { ColShape::Hull } else { ColShape::Visible };
    if marks.nc || tris.is_empty() {
        let vis = if tris.is_empty() { gather(0, ident, 1.0, true, false) } else { tris };
        tris = half_box(&bounds_of(&vis)?);
        shape = ColShape::Bounds;
    }

    if tris.is_empty() {
        return None;
    }

    // Bounds describe whatever is actually tested against. Taking them from the
    // visible mesh while testing the hull would clear grass out to a tree's whole
    // canopy before the triangle test ever ran.
    let (aabb_min, aabb_max) = bounds_of(&tris)?;
    // Clutter too small to matter is not worth keeping grass away from, and a floor
    // carpeted in it would otherwise have no grass left at all. The caller decides
    // where that line falls; `radius` is reported so it can.
    let radius = (0..3)
        .map(|k| (aabb_max[k] - aabb_min[k]) * 0.5)
        .fold(0.0f32, |a, v| a + v * v)
        .sqrt();
    Some(MeshGeom { tris: std::sync::Arc::new(tris), aabb_min, aabb_max, shape, radius, scanned: 0 })
}

fn bounds_of(tris: &[[f32; 3]]) -> Option<([f32; 3], [f32; 3])> {
    if tris.is_empty() {
        return None;
    }
    let mut mn = [f32::INFINITY; 3];
    let mut mx = [f32::NEG_INFINITY; 3];
    for p in tris {
        for k in 0..3 {
            if p[k] < mn[k] {
                mn[k] = p[k];
            }
            if p[k] > mx[k] {
                mx[k] = p[k];
            }
        }
    }
    Some((mn, mx))
}

/// A box half the size of the bounds, about their centre.
///
/// Half, because bounds are generous: a plant's are drawn around leaves that reach
/// out much further than the stem anyone would try to click on, and blocking the
/// whole envelope would leave a bare ring around every fern. Centred rather than
/// stood on the ground, because that is also the safeguard: a tall thing's half-box
/// floats in the middle of its own height, above where grass grows, while a low
/// thing's sits right in it. A fern gets a box the grass runs into; a canopy that
/// somehow reached here gets one the grass passes under.
/// One triangle's three corners, or **none of them** — round 18bc (B7).
///
/// The collision soup is a flat list read in threes by everything downstream. A triangle
/// naming a vertex past the end of the list — a malformed mod mesh — used to skip that
/// single *point*, which left the soup no longer a multiple of three: every triangle
/// after the bad one was then built out of its neighbours' corners. Measured, 38 entries
/// for 13 triangles, and the obstacle became random slivers — grass blocked in odd places
/// and not blocked where the object actually stands. `Surface::from_parts` in
/// `mesh_scatter.rs` has always dropped the whole face; this is the same rule, said here.
pub fn whole_face<'a>(verts: &'a [[f32; 3]], t: &[u16; 3]) -> Option<[&'a [f32; 3]; 3]> {
    Some([verts.get(t[0] as usize)?, verts.get(t[1] as usize)?, verts.get(t[2] as usize)?])
}

fn half_box((mn, mx): &([f32; 3], [f32; 3])) -> Vec<[f32; 3]> {
    let c = [(mn[0] + mx[0]) * 0.5, (mn[1] + mx[1]) * 0.5, (mn[2] + mx[2]) * 0.5];
    let h = [(mx[0] - mn[0]) * 0.25, (mx[1] - mn[1]) * 0.25, (mx[2] - mn[2]) * 0.25];
    let v = |sx: f32, sy: f32, sz: f32| [c[0] + h[0] * sx, c[1] + h[1] * sy, c[2] + h[2] * sz];
    let p = [
        v(-1., -1., -1.),
        v(1., -1., -1.),
        v(1., 1., -1.),
        v(-1., 1., -1.),
        v(-1., -1., 1.),
        v(1., -1., 1.),
        v(1., 1., 1.),
        v(-1., 1., 1.),
    ];
    let quads = [
        [0, 3, 2, 1],
        [4, 5, 6, 7],
        [0, 1, 5, 4],
        [1, 2, 6, 5],
        [2, 3, 7, 6],
        [3, 0, 4, 7],
    ];
    let mut tris = Vec::with_capacity(36);
    for q in quads {
        for k in [q[0], q[1], q[2], q[0], q[2], q[3]] {
            tris.push(p[k]);
        }
    }
    tris
}


/* ---- Wraithguard: parts hung on another file's skeleton (npc.rs) --------------------- */

/// A skeleton read once, with an idle bound when there is one, for hanging body parts on.
pub struct Skeleton {
    rec: Records,
    parent: Vec<i32>,
}

impl Skeleton {
    pub fn read(buf: &[u8], kf: Option<&KfSequence>) -> Option<Skeleton> {
        let rec = read_records_kf(buf, kf)?;
        let parent = parents_of(&rec);
        Some(Skeleton { rec, parent })
    }

    fn find(&self, name: &str) -> Option<i32> {
        let by = |exact: bool| {
            self.rec.nodes.iter().position(|n| {
                n.as_ref().is_some_and(|n| if exact { n.name == name } else { n.name.eq_ignore_ascii_case(name) })
            })
        };
        by(true).or_else(|| by(false)).map(|i| i as i32)
    }

    /// The bone called `name` as a skinned shape needs it: its chain of moving nodes, the
    /// still tail and `trafo` as its skin-to-bone transform.
    pub fn bone(&self, name: &str, trafo: ([f32; 12], f32)) -> Option<BoneAnim> {
        let i = self.find(name)?;
        let kf_of = |j: i32| kf_for(&self.rec, j);
        bone_anim(&self.rec, &self.parent, &kf_of, i, trafo)
    }
}

/// A mesh's draw parts with every skinned shape keeping its skin (`DrawPart::skin`), so
/// it can be bound to a skeleton elsewhere.
pub fn read_parts_skinned(buf: &[u8]) -> Option<Vec<DrawPart>> {
    let rec = read_records_opts(buf, None, true)?;
    Some(draw_parts(&rec))
}

const IDENT: [f32; 12] = [1., 0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 0.];

/// A bone's skinning matrix at `t` (`None`: each link's clip start) - the page's
/// `skinBoneMats`: the chain, the still tail, the skin-to-bone transform.
fn bone_at(b: &BoneAnim, t: Option<f32>) -> ([f32; 12], f32) {
    let (mut m, mut s) = (IDENT, 1.0f32);
    for l in &b.chain {
        let (pm, ps) = mul(&m, s, &l.pre.0, l.pre.1);
        let at = t.unwrap_or(l.ctrl.start);
        let (lm, ls) = keyed_pose_at(&l.local.0, l.local.1, &l.ctrl, &l.data, at);
        (m, s) = mul(&pm, ps, &lm, ls);
    }
    let (m, s) = mul(&m, s, &b.post.0, b.post.1);
    mul(&m, s, &b.trafo.0, b.trafo.1)
}

/// Puts a skinned part's vertices where its bones hold them at the clip's start - the
/// still picture, and what the page's bounds are taken from.
fn rebake_skin(p: &mut DrawPart) {
    let Some(sk) = &p.skin else { return };
    let nv = sk.pos.len() / 3;
    if p.pos.len() != nv * 3 {
        return;
    }
    let mats: Vec<([f32; 12], f32)> = sk.bones.iter().map(|b| bone_at(b, None)).collect();
    let has_n = sk.nrm.len() == nv * 3 && p.nrm.len() == nv * 3;
    for v in 0..nv {
        let (mut acc, mut nacc, mut ws) = ([0f32; 3], [0f32; 3], 0f32);
        let src = [sk.pos[v * 3], sk.pos[v * 3 + 1], sk.pos[v * 3 + 2]];
        for k in 0..4 {
            let bi = sk.idx[v * 4 + k];
            let w = sk.w[v * 4 + k];
            if bi == 255 || w <= 0.0 {
                continue;
            }
            let Some((m, s)) = mats.get(bi as usize) else { continue };
            let q = apply(m, *s, src);
            for c in 0..3 {
                acc[c] += w * q[c];
            }
            if has_n {
                let n = rotate(m, [sk.nrm[v * 3], sk.nrm[v * 3 + 1], sk.nrm[v * 3 + 2]]);
                for c in 0..3 {
                    nacc[c] += w * n[c];
                }
            }
            ws += w;
        }
        if ws <= 0.0 {
            continue;
        }
        for c in 0..3 {
            p.pos[v * 3 + c] = acc[c] / ws;
        }
        if has_n {
            let l = (nacc[0] * nacc[0] + nacc[1] * nacc[1] + nacc[2] * nacc[2]).sqrt();
            if l > 1e-6 {
                for c in 0..3 {
                    p.nrm[v * 3 + c] = nacc[c] / l;
                }
            }
        }
    }
}

/// Binds a skinned part to `skel`'s bones by name, as OpenMW binds a body part to the
/// actor's skeleton: each bone's chain becomes the skeleton's, so the part moves with it.
/// A bone the skeleton lacks keeps the part file's own. When nothing moves, the skin is
/// baked into the part and dropped.
pub fn bind_to_skeleton(p: &mut DrawPart, skel: &Skeleton) {
    let Some(sk) = p.skin.as_mut() else { return };
    for b in sk.bones.iter_mut() {
        if let Some(nb) = skel.bone(&b.name, b.trafo) {
            *b = nb;
        }
    }
    let moves = sk.bones.iter().any(|b| !b.chain.is_empty());
    rebake_skin(p);
    p.anim = None;
    if !moves {
        p.skin = None;
    }
}

/// Hangs a rigid part on `skel`'s bone `bone` (mirrored across X when `mirror`, as OpenMW
/// mirrors the left-side parts): the bone's moving nodes go in front of the part's own,
/// and the still transform down to the bone is applied to it. False when the skeleton has
/// no such bone.
pub fn attach_to_bone(p: &mut DrawPart, skel: &Skeleton, bone: &str, mirror: bool) -> bool {
    let Some(b) = skel.bone(bone, (IDENT, 1.0)) else { return false };
    let mut m = b.post.0;
    if mirror {
        // post x diag(-1, 1, 1): the first column of the rotation flips.
        m[0] = -m[0];
        m[3] = -m[3];
        m[6] = -m[6];
    }
    prepend_transform(p, &m, b.post.1);
    if mirror {
        for t in p.idx.chunks_exact_mut(3) {
            t.swap(1, 2);
        }
    }
    if !b.chain.is_empty() {
        let a = p.anim.get_or_insert_with(PartAnim::default);
        let mut nodes = b.chain;
        nodes.append(&mut a.nodes);
        a.nodes = nodes;
    }
    true
}

/// Puts `(m, s)` above everything in the part: into its first moving node's `pre` when
/// it has any, into each bone's first transform when it is skinned, and into its
/// vertices when it is still. `m`'s 3x3 may scale or mirror.
pub fn prepend_transform(p: &mut DrawPart, m: &[f32; 12], s: f32) {
    if let Some(sk) = p.skin.as_mut() {
        for b in sk.bones.iter_mut() {
            match b.chain.first_mut() {
                Some(l) => l.pre = mul(m, s, &l.pre.0, l.pre.1),
                None => b.post = mul(m, s, &b.post.0, b.post.1),
            }
        }
        rebake_skin(p);
        return;
    }
    if let Some(a) = p.anim.as_mut() {
        if let Some(l) = a.nodes.first_mut() {
            l.pre = mul(m, s, &l.pre.0, l.pre.1);
            return;
        }
    }
    for v in p.pos.chunks_exact_mut(3) {
        let q = apply(m, s, [v[0], v[1], v[2]]);
        v.copy_from_slice(&q);
    }
    for v in p.nrm.chunks_exact_mut(3) {
        let q = rotate(m, [v[0], v[1], v[2]]);
        let l = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2]).sqrt();
        if l > 1e-6 {
            v.copy_from_slice(&[q[0] / l, q[1] / l, q[2] / l]);
        }
    }
}

/// The identity transform, as `Skeleton::bone` takes a skin-to-bone one.
pub fn identity() -> ([f32; 12], f32) {
    (IDENT, 1.0)
}

/// Hangs particle systems read from a carried mesh on `skel`'s bone `bone`, where the bone
/// stands at the idle's start (the systems do not follow the idle): a torch's flame.
pub fn attach_systems(sys: &mut [ParticleSystem], skel: &Skeleton, bone: &str) -> bool {
    let Some(b) = skel.bone(bone, (IDENT, 1.0)) else { return false };
    let (m, s) = bone_at(&b, None);
    prepend_systems(sys, &m, s);
    true
}

/// Wraithguard: a point in a carried mesh's own space, where the skeleton's `bone` holds
/// it at the clip's start - a torch's AttachLight in the NPC's space.
pub fn bone_point(skel: &Skeleton, bone: &str, p: [f32; 3]) -> Option<[f32; 3]> {
    let b = skel.bone(bone, (IDENT, 1.0))?;
    let (m, s) = bone_at(&b, None);
    Some(apply(&m, s, p))
}

/// Puts `(m, s)` above particle systems' nodes and emitters.
pub fn prepend_systems(sys: &mut [ParticleSystem], m: &[f32; 12], s: f32) {
    for p in sys.iter_mut() {
        p.node = mul(m, s, &p.node.0, p.node.1);
        p.emitter = mul(m, s, &p.emitter.0, p.emitter.1);
    }
}

#[cfg(test)]
mod kf_window_tests {
    use super::KfData;

    #[test]
    fn a_clip_keeps_its_keys_and_one_either_side() {
        let d = KfData {
            scales: (0..100).map(|i| (i as f32, i as f32)).collect(),
            trans: vec![(50.0, [1.0, 2.0, 3.0])],
            ..Default::default()
        };
        let w = d.window(10.5, 20.5);
        let t: Vec<f32> = w.scales.iter().map(|k| k.0).collect();
        assert_eq!(t.first(), Some(&10.0));
        assert_eq!(t.last(), Some(&21.0));
        assert_eq!(t.len(), 12);
        // A channel with its one key outside the clip keeps it (the value it holds).
        assert_eq!(w.trans.len(), 1);
        assert!(KfData::default().window(0.0, 1.0).is_empty());
    }
}
