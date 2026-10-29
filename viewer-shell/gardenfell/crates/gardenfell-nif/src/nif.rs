//! The engine's views of a NetImmerse 4.0.0.2 mesh: the collision hull, the drawable
//! parts, the particle systems, the lights and the animation, each built from the typed
//! model in [`crate::netimmerse`].
//!
//! Round 18gh. Until this round the file had a byte reader of its own (`struct Reader`,
//! `skip_or_read`, `resync`) that walked the records and filled [`Records`], and the typed
//! model was a second reader of the same bytes that the engine consulted only to measure
//! a record it could not. A byte of a mesh is now read in exactly one place:
//! `netimmerse::stream` reads the blocks, `records_from_stream` turns them into the
//! `Records` the views have always read, and a file the model refuses is unreadable, with
//! the model's error naming the byte. There is no resync and no record scan.

use crate::netimmerse::stream::{Link, Stream};
use crate::netimmerse::types as ni;
use glam::Vec3;

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
    /// Round 18bz: per-triangle bounds and the blocks above them, so an obstacle query
    /// skips most of the soup instead of walking all of it. `None` for a mesh too small to
    /// be worth one (and for a `MeshGeom` built by hand, which is how the tests keep the
    /// flat walk covered) — every caller falls back to walking every triangle, which is
    /// what they all did before this existed.
    ///
    /// On the mesh rather than beside it: `geom` and an accelerator in a second map would
    /// be two sources for one fact, and this codebase has paid for that shape three times.
    pub accel: Option<std::sync::Arc<crate::triaccel::TriAccel>>,
}

/// The children a switch or level-of-detail node shows. A switch node shows the one
/// child its `active_index` names. A level-of-detail node shows every child whose
/// distance range holds 0: the preview stands next to what it draws, and the game at
/// that distance shows the near level and not the far one (round 18gi, the Improved
/// Lights sconces: a fire sprite for 2500 to 4000 units away was drawn up close). A
/// child with no range of its own, which a file with more children than levels has, is
/// shown, as it always was.
fn live_children(n: &Node) -> Vec<i32> {
    if n.kind == "NiLODNode" {
        return n
            .children
            .iter()
            .enumerate()
            .filter(|(k, _)| n.lod_levels.get(*k).map_or(true, |r| r[0] <= 0.0 && 0.0 < r[1]))
            .map(|(_, &c)| c)
            .collect();
    }
    n.children.get(n.active_index).copied().into_iter().collect()
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
    /// NiLODNode only: each child's distance range `[near, far)`, by child index. The
    /// preview stands next to what it draws, so the level whose range holds distance 0
    /// is the one it shows (`live_children`, round 18gi).
    lod_levels: Vec<[f32; 2]>,
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
        if b > a + 1e-4 {
            Some((a, b))
        } else {
            None
        }
    }

    /// The whole span the tracks key, for a file with no usable text keys.
    pub fn span(&self) -> Option<(f32, f32)> {
        let mut lo = f32::MAX;
        let mut hi = f32::MIN;
        for t in &self.tracks {
            for (k, _) in &t.data.rot {
                lo = lo.min(*k);
                hi = hi.max(*k);
            }
            for (k, _) in &t.data.trans {
                lo = lo.min(*k);
                hi = hi.max(*k);
            }
            for (k, _) in &t.data.scales {
                lo = lo.min(*k);
                hi = hi.max(*k);
            }
            if let Some(ch) = &t.data.rot_xyz {
                for c in ch {
                    for (k, _) in c {
                        lo = lo.min(*k);
                        hi = hi.max(*k);
                    }
                }
            }
        }
        if hi > lo + 1e-4 {
            Some((lo, hi))
        } else {
            None
        }
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
        if self.text_keys.is_empty() {
            self.span()
        } else {
            None
        }
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
    /// NiTexturingProperty slot 3, the gloss map: a luminance mask on the specular
    /// highlight. Neither Morrowind nor OpenMW draws it; a mesh viewer's studio view can.
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

/// Round 18ey: a NiFlipController and the texture files it flips between.
///
/// Robin: *"These meshes: TR\\x\\TR_water_Circle2048_DG.NIF, CAP\\nb_ex_vivec_p_water_01.nif
/// (there are several of this type), have animated textures in the game, but not in the
/// tool."* The mechanism is a flipbook: the controller hangs off the shape's
/// NiTexturingProperty and cycles that slot's texture through a list of NiSourceTextures,
/// one every `delta` seconds. Tamriel Rebuilt's water circle carries thirty-two frames of
/// `tr_water00.dds` through `tr_water31.dds`; the reader measured the record so the file
/// would not desync (round 18df) and then threw it away, so the tool drew frame nothing and
/// the water sat still.
///
/// The clock is the time controller's own - `frequency`, `phase`, `start`, `stop` - and the
/// frame is `floor((t - tex_start) / delta)` wrapped by the number of frames, which is what
/// OpenMW's `FlipController::apply` does.
#[derive(Clone, Default, Debug, PartialEq)]
pub struct FlipAnim {
    /// The NiTexturingProperty slot it flips: 0 base, 1 dark, 2 detail, 4 glow, 6 decal.
    pub slot: u32,
    pub frequency: f32,
    pub phase: f32,
    pub start: f32,
    pub stop: f32,
    /// The time the first frame is shown at, and the seconds a frame lasts.
    pub tex_start: f32,
    pub delta: f32,
    /// The frames, in order, as the file names they resolve to.
    pub frames: Vec<String>,
}

impl FlipAnim {
    /// Whether it would show more than one picture. A one-frame flip is a still texture
    /// with a controller on it, and carrying it to the page would cost a draw-time branch
    /// for nothing.
    pub fn moves(&self) -> bool {
        self.frames.len() > 1 && self.delta.is_finite() && self.delta > 0.0
    }
}

/// Round 18ez: a shape under a NiBillboardNode - drawn turned to the eye.
///
/// Robin: *"Fix the billboards to turn towards the eye."* `glowbugjar01`'s ten glowbugs
/// are ten of these and were left out entirely ("a still preview cannot turn them"). At
/// 4.0.0.2 the mode is not a field of its own: OpenMW's `NiBillboardNode::read` takes
/// `(flags >> 5) & 3` below version 10.1.0.0 - 0 always face the camera, 1 rotate about
/// the up axis only, 2 rigid face camera, 3 face the centre - and hangs an `AutoTransform`
/// on the node that keeps the node's translation and scale and replaces its rotation
/// every frame. The vertices of a shape under one are kept in the node's own frame (its
/// rotation and scale baked, its translation not), and the page turns them per instance
/// in the vertex shader, because which way "towards the eye" is depends on where each
/// instance stands.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Billboard {
    /// 0 always face camera, 1 rotate about up, 2 rigid face camera, 3 face centre.
    pub mode: u8,
    /// The node's origin in mesh space - the point it turns about.
    pub origin: [f32; 3],
    /// The scale accumulated above the node, which its vertices do not carry.
    pub scale: f32,
}

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

/// The name a texture held inside a NIF goes by (`NiSourceTexture` with no file, its pixels
/// in an `NiPixelData` record): this prefix and the pixel record's index. A front end that
/// knows which mesh it came from can name it further (Wraithguard appends `@` and the
/// mesh's path, and decodes the record when the page asks for that texture).
pub const EMBEDDED: &str = "__nifpx/";

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
    /// Round 18gf (backend-plan N6): the axis order of the three Euler channels, as the
    /// file states it - 0 XYZ, 1 XZY, 2 YZX, 3 YXZ, 4 ZXY, 5 ZYX, 6 XYX, 7 YZY, 8 ZXZ
    /// (`tes3` nirotkey.rs:98-101). Channel `i` turns about the `i`th axis of the order,
    /// first channel applied first. It was read and dropped before, and every file
    /// composed as XYZ; vanilla states nothing else, a mod's export may.
    pub rot_order: i32,
    pub trans: Vec<(f32, [f32; 3])>,
    pub scales: Vec<(f32, f32)>,
}

impl KfData {
    pub fn is_empty(&self) -> bool {
        self.rot.is_empty() && self.rot_xyz.is_none() && self.trans.is_empty() && self.scales.is_empty()
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
    /// every one of them says visible. Their keys are (time, 0 or 1), held from key to key
    /// rather than blended. The clock fields are the controller's; `keys` the NiVisData's.
    pub vis: Vec<AlphaAnim>,
}

impl PartAnim {
    /// Where this part's geometry stands at the clip's **first frame** — the pose a still
    /// picture shows and the pose the collision hull is built from.
    ///
    /// Round 18bf, while retiring §I26. A part under a moving node is handed out in that
    /// node's own space, not the mesh's: the walk resets the composed transform at every
    /// animated node and leaves the rest to whoever draws it, because the page poses the
    /// thing per frame. So comparing a drawn part's raw vertices against the hull compares
    /// two different coordinate systems, which is what made the two passes *look* like
    /// they disagreed. Composed the way the page composes it — `pre₀ × node₀ × pre₁ ×
    /// node₁ × …`, `10_preview.js:animMatrixAt` — they agree, and this is the function
    /// that lets a test say so rather than a comment claiming it.
    pub fn at_start(&self) -> ([f32; 12], f32) {
        let mut m = [1., 0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 0.];
        let mut s = 1.0f32;
        for l in &self.nodes {
            let (pm, ps) = mul(&m, s, &l.pre.0, l.pre.1);
            let (lm, ls) = keyed_pose_at(&l.local.0, l.local.1, &l.ctrl, &l.data, l.ctrl.start);
            let (nm, ns) = mul(&pm, ps, &lm, ls);
            m = nm;
            s = ns;
        }
        (m, s)
    }
}

/// Round 18ag: one bone of a skinned shape under an animated skeleton — the animated
/// nodes from the root down to it (`chain`, as a part's own), the still transform from
/// the last of those down to the bone itself (`post`; the identity when the bone is the
/// last link), and the skin-to-bone transform from NiSkinData. The bone's matrix at time
/// `t` is `chain(t) × post × trafo`, and a vertex is the weighted sum of its bones'.
#[derive(Clone, Debug)]
pub struct BoneAnim {
    /// The bone node's name, so a part's skin can be bound to another file's skeleton by
    /// name.
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

/// The record table, built once from the typed stream (`records_from_stream`); every
/// walk reads from this. Each `Vec<Option<X>>` has one entry per block of the file,
/// indexed by block index, so a link in one record is an index into every table.
struct Records {
    /// Round 18gh: the block the file's first root names, where every walk starts and
    /// whose own transform is never applied. Block 0 in every file the game's exporters
    /// write; the last block in a file Blender wrote, which used to be read through a
    /// rewritten copy with the root moved to 0 (`stream::root_first`).
    root: i32,
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
    /// NiVisController by record index (its clock, target and data), and NiVisData's keys
    /// as (time, 0 or 1).
    visctrls: Vec<Option<KfCtrl>>,
    visdatas: Vec<Option<Vec<(f32, f32)>>>,
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
    /// Round 18ey: NiFlipController by record index - the target it names (a
    /// NiTexturingProperty) and everything needed to run it, with the frames still as
    /// record references; `flips_of` resolves them to file names.
    flipctrls: Vec<Option<(i32, FlipAnim, Vec<i32>)>>,
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

/// NiParticleSystemController (and NiBSPArrayController, which adds nothing) as the
/// system runs it. The byte reader named these fields for what they do; the typed model
/// names them as `tes3` does, so this is where the two vocabularies meet: `velocity` is
/// `speed`, `vertical_dir` the declination, `horizontal_dir` the planar angle,
/// `offset_random` the emitter's width, height and depth, and `emit_flags` the two bytes
/// (`use_birth_rate`, `spawn_on_death`) the byte reader read as one short, low byte first.
fn particle_ctrl(c: &ni::NiParticleSystemController) -> ParticleCtrl {
    let t = &c.base;
    ParticleCtrl {
        flags: t.flags,
        frequency: t.frequency,
        start: t.start_time,
        stop: t.stop_time,
        target: t.target.index,
        velocity: c.speed,
        velocity_random: c.speed_variation,
        vertical_dir: c.declination_angle,
        vertical_angle: c.declination_variation,
        horizontal_dir: c.planar_angle,
        horizontal_angle: c.planar_angle_variation,
        colour: c.initial_color,
        size: c.initial_size,
        emit_start: c.emit_start_time,
        emit_stop: c.emit_stop_time,
        emit_rate: c.birth_rate,
        lifetime: c.lifespan,
        lifetime_random: c.lifespan_variation,
        emit_flags: (c.use_birth_rate as u16) | ((c.spawn_on_death as u16) << 8),
        offset_random: [c.emitter_width, c.emitter_height, c.emitter_depth],
        emitter: c.emitter.index,
        num_particles: c.particles.len() as u16,
        num_valid: c.num_active_particles,
        modifier: c.particle_modifier.index,
        collider: c.particle_collider.index,
    }
}

/// NiAutoNormalParticlesData, NiParticlesData or NiRotatingParticlesData: what the
/// vertices carry. The vertex count is the vertex list's length, the colours the file's
/// RGBA floats as they are.
fn particle_data(d: &ni::NiParticlesData, rotating: bool) -> ParticleData {
    ParticleData {
        nverts: d.base.vertices.len(),
        radius: d.particle_radius,
        sizes: d.sizes.clone(),
        colours: d.base.vertex_colors.clone(),
        rotating,
    }
}

/// A NIF transform as the skin records spell it, rotation then translation then scale,
/// packed the way `Node.trafo` is: the 3x3 row by row in `m[0..9]`, the translation in
/// `m[9..12]`, the scale beside it.
fn skin_trafo(rotation: &[[f32; 3]; 3], translation: &[f32; 3], scale: f32) -> ([f32; 12], f32) {
    (pack_trafo(rotation, translation), scale)
}

/// The engine's packed transform from the model's 3x3 and translation.
fn pack_trafo(rotation: &[[f32; 3]; 3], translation: &[f32; 3]) -> [f32; 12] {
    [
        rotation[0][0],
        rotation[0][1],
        rotation[0][2],
        rotation[1][0],
        rotation[1][1],
        rotation[1][2],
        rotation[2][0],
        rotation[2][1],
        rotation[2][2],
        translation[0],
        translation[1],
        translation[2],
    ]
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
        1.0 - 2.0 * (y * y + z * z),
        2.0 * (x * y - w * z),
        2.0 * (x * z + w * y),
        2.0 * (x * y + w * z),
        1.0 - 2.0 * (x * x + z * z),
        2.0 * (y * z - w * x),
        2.0 * (x * z - w * y),
        2.0 * (y * z + w * x),
        1.0 - 2.0 * (x * x + y * y),
    ]
}

/// Three Euler channels (radians about x, y, z, applied z·y·x as the format has it).
/// The three Euler channels composed in the file's own axis order (round 18gf): channel
/// `i` turns about axis `order[i]`, the first channel applied first, so the matrix is
/// `R(ax2) · R(ax1) · R(ax0)`. Order 0 (XYZ) is `R(z) · R(y) · R(x)`, which is what every file was read as before.
pub fn euler_mat3_ordered(e: [f32; 3], order: i32) -> [f32; 9] {
    const AXES: [[usize; 3]; 9] =
        [[0, 1, 2], [0, 2, 1], [1, 2, 0], [1, 0, 2], [2, 0, 1], [2, 1, 0], [0, 1, 0], [1, 2, 1], [2, 0, 2]];
    let ax = AXES.get(order.max(0) as usize).copied().unwrap_or([0, 1, 2]);
    let about = |axis: usize, a: f32| -> [f32; 9] {
        let (c, s) = (a.cos(), a.sin());
        match axis {
            0 => [1., 0., 0., 0., c, -s, 0., s, c],
            1 => [c, 0., s, 0., 1., 0., -s, 0., c],
            _ => [c, -s, 0., s, c, 0., 0., 0., 1.],
        }
    };
    let m = |a: &[f32; 9], b: &[f32; 9]| -> [f32; 9] {
        let mut o = [0f32; 9];
        for r in 0..3 {
            for c in 0..3 {
                o[r * 3 + c] = a[r * 3] * b[c] + a[r * 3 + 1] * b[3 + c] + a[r * 3 + 2] * b[6 + c];
            }
        }
        o
    };
    m(&m(&about(ax[2], e[2]), &about(ax[1], e[1])), &about(ax[0], e[0]))
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
        m[..9].copy_from_slice(&euler_mat3_ordered([e[0], e[1], e[2]], data.rot_order));
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
            let (nm, ns) = match if i as i32 != rec.root { kf_for(rec, i as i32) } else { None } {
                Some((ctrl, data)) => {
                    let (lm, ls) = keyed_pose(&nd.trafo, nd.scale, &ctrl, &data);
                    mul(&m, s, &lm, ls)
                }
                None => node_trafo(rec.root, i as i32, &m, s, nd),
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
            let Some((wm, ws)) = world_of(bref) else {
                ok = false;
                break;
            };
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

/// The records, with a `.kf`'s tracks bound to the nodes when one is given (round 18ag).
fn read_records_kf(buf: &[u8], kf: Option<&KfSequence>) -> Option<Records> {
    let s = read_stream(buf)?;
    Some(records_from_stream(&s, kf))
}

/// The file as the typed model reads it, or `None` for one the engine does not read.
///
/// Round 18gh: the typed model reads the file and nothing else reads a byte of it. A file
/// the model refuses (a header line that is not a NIF's, a version word that is not
/// 4.0.0.2, a block type the format does not have, a count the bytes left cannot hold,
/// bytes after the roots) is `None`, and under `GDN_NIF_DEBUG` the model's error is said on
/// stderr with the byte it stopped at, so a failure names itself the way the byte reader's
/// did. The game's engine is NetImmerse 4.0.0.2 and reads nothing else (18gf, backend-plan
/// N8), so a 4.1 or 10.x file is refused here as it is there.
/// What reads a mesh's bytes into the typed model: [`Stream::parse`] unless another
/// reader is set.
pub type StreamReader = fn(&[u8]) -> Result<Stream, String>;

static STREAM_READER: std::sync::OnceLock<StreamReader> = std::sync::OnceLock::new();

/// Read meshes with another reader than this crate's own (a program embedding the engine
/// with a reader of its own, whose blocks it converts to these). Set once, before the
/// first mesh is read; later calls are refused.
pub fn set_stream_reader(reader: StreamReader) -> Result<(), StreamReader> {
    STREAM_READER.set(reader)
}

fn read_stream(buf: &[u8]) -> Option<Stream> {
    // One look at the environment for the whole file, not one per record.
    let dbg = std::env::var_os("GDN_NIF_DEBUG").is_some();
    let parsed = match STREAM_READER.get() {
        Some(r) => r(buf),
        None => Stream::parse(buf).map_err(|e| e.to_string()),
    };
    let s = match parsed {
        Ok(s) => s,
        Err(e) => {
            if dbg {
                eprintln!("nif: {e}");
            }
            return None;
        }
    };
    /* Round 18gf (backend-plan N8): the header line is checked as well as the version
    word. The model accepts any "NetImmerse File Format" line with a 4.0.0.2 version
    word and keeps the text as read, since a tool's variation on it writes back as it
    came; the game's engine takes its version from the line, so a line that names
    another version is a file it will not load, and neither does this. */
    if !s.header.starts_with("NetImmerse File Format, Version 4.0.0.2") {
        if dbg {
            eprintln!("nif: header is not NetImmerse 4.0.0.2: {:?}", s.header.trim_end());
        }
        return None;
    }
    // A file with no blocks has no mesh in it, and reads as none, as it always has.
    if s.blocks.is_empty() {
        return None;
    }
    if dbg {
        for (i, b) in s.blocks.iter().enumerate() {
            eprintln!("nif: {i} {}", b.name());
        }
    }
    Some(s)
}

/// The block indices a list of links names, `-1` for none, as the tables here hold them.
fn indices<T>(links: &[Link<T>]) -> Vec<i32> {
    links.iter().map(|l| l.index).collect()
}

/// A `Node` from the NiAVObject every scene block carries: the name, the flags, the
/// transform as the engine packs it, the property list and the heads of the extra-data
/// and controller chains. The extra-data chain carries NC ("nothing collides with this"),
/// MRK and RCN, so the link is kept rather than stepped over; the properties are kept
/// because a shape's texture, colour and alpha all hang off this list and a shape inherits
/// whatever its parents carry; the controller because the particle systems hang their
/// NiParticleSystemController off it (round 17).
fn node_of(kind: &str, av: &ni::NiAVObject) -> Node {
    Node {
        kind: kind.to_string(),
        name: av.base.name.clone(),
        flags: av.flags,
        trafo: pack_trafo(&av.rotation, &av.translation),
        scale: av.scale,
        children: Vec::new(),
        extra: av.base.extra_data.index,
        data_ref: -1,
        active_index: 0,
        lod_levels: Vec::new(),
        props: indices(&av.properties),
        skin_ref: -1,
        skinned: false,
        ctrl: av.base.controller.index,
        effects: Vec::new(),
    }
}

/// An NiNode or one of its kinds: the AV object, its children and (round 18dl) its
/// dynamic effects, the NiTextureEffect that puts an environment map on everything below.
fn group_node(kind: &str, n: &ni::NiNode) -> Node {
    let mut out = node_of(kind, &n.base);
    out.children = indices(&n.children);
    out.effects = indices(&n.effects);
    out
}

/// A shape or a particle system: the AV object, its geometry data and its skin.
fn shape_node(kind: &str, g: &ni::NiGeometry) -> Node {
    let mut out = node_of(kind, &g.base);
    out.data_ref = g.geometry_data.index;
    out.skin_ref = g.skin_instance.index;
    out
}

/// A float key track as the engine keeps it: the time and the value of every key,
/// whatever the interpolation. Morrowind's UV animations and fades are linear ramps, and
/// reading a curve as a line is a small error where, in the byte reader's day, reading it
/// as the wrong length was a broken file. The model has already read the tangents and the
/// tension, bias and continuity; they are dropped here.
fn float_keys(k: &ni::NiFloatKey) -> Vec<(f32, f32)> {
    match k {
        ni::NiFloatKey::Lin(v) => v.iter().map(|k| (k.time, k.value)).collect(),
        ni::NiFloatKey::Bez(v) => v.iter().map(|k| (k.time, k.value)).collect(),
        ni::NiFloatKey::Tcb(v) => v.iter().map(|k| (k.time, k.value)).collect(),
    }
}

/// NiKeyframeData as the engine keeps it (round 17y): quaternion keys `(time, [w, x, y,
/// z])`, or the three Euler channels with the axis order the file states (round 18gf,
/// backend-plan N6: the model reads the order word, and this takes it), translations
/// `(time, [x, y, z])` and scales `(time, s)`. Only the time and the value of a key are
/// kept, as `float_keys` keeps them.
fn kf_data(d: &ni::NiKeyframeData) -> KfData {
    let mut out = KfData::default();
    match &d.rotations.keys {
        ni::NiRotKey::Lin(v) => out.rot = v.iter().map(|k| (k.time, k.value)).collect(),
        ni::NiRotKey::Bez(v) => out.rot = v.iter().map(|k| (k.time, k.value)).collect(),
        ni::NiRotKey::Tcb(v) => out.rot = v.iter().map(|k| (k.time, k.value)).collect(),
        ni::NiRotKey::Euler(e) => {
            out.rot_order = e.axis_order;
            out.rot_xyz =
                Some([float_keys(&e.axes[0].keys), float_keys(&e.axes[1].keys), float_keys(&e.axes[2].keys)]);
        }
    }
    out.trans = match &d.translations.keys {
        ni::NiPosKey::Lin(v) => v.iter().map(|k| (k.time, k.value)).collect(),
        ni::NiPosKey::Bez(v) => v.iter().map(|k| (k.time, k.value)).collect(),
        ni::NiPosKey::Tcb(v) => v.iter().map(|k| (k.time, k.value)).collect(),
    };
    out.scales = float_keys(&d.scales.keys);
    out
}

/// A time controller's clock as `KfCtrl` and `MorphCtrl` spell it.
fn kf_ctrl(t: &ni::NiTimeController, data: i32) -> KfCtrl {
    KfCtrl {
        next: t.next.index,
        flags: t.flags,
        frequency: t.frequency,
        phase: t.phase,
        start: t.start_time,
        stop: t.stop_time,
        target: t.target.index,
        data,
    }
}

/// Everything a `NiTriBasedGeomData` holds before its faces: the vertices, the normals,
/// the vertex colours and the UV sets.
///
/// Round 18bg (§G2): `NiTriShapeData` and `NiTriStripsData` share all of it and differ
/// only in how they spell their faces, so one function makes the shared part. The model
/// has every UV set as one flat list; the first is `uvs` and the rest `uv_more`, up to the
/// eight the engine keeps (round 17y: every set is kept, since Glow in the Dahrk's windows
/// read a third). A bounding sphere the model read is not wanted here.
fn geom_data(g: &ni::NiGeometryData) -> ShapeData {
    let nverts = g.vertices.len();
    let mut colours = Vec::new();
    let mut alphas = Vec::new();
    if !g.vertex_colors.is_empty() {
        // Vertex colours: four floats each. Kept as RGB bytes, and the alpha apart.
        colours.reserve(nverts);
        alphas.reserve(nverts);
        for c in &g.vertex_colors {
            /* The fourth is the vertex alpha. Round 17n read and dropped it - a flat 255
            on every shape of `Ex_Vivec_waterfall_03` and on the Ghostfence, and on
            every other vanilla shape, the audit found. Round 18df keeps it: OAAB's
            root pillars, inflow sheets and lava falls fade through it (nif-audit.md
            §1), and a veteran saw them drawn solid. Kept beside the colours, not in
            them, so the payload only grows for a shape that uses it. */
            let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            colours.push([q(c[0]), q(c[1]), q(c[2])]);
            alphas.push(q(c[3]));
        }
        if alphas.iter().all(|&a| a == 255) {
            alphas.clear();
        }
    }
    let nuv = g.num_uv_sets();
    let uvs = if nuv > 0 { g.uv_set(0).map(|s| s.to_vec()).unwrap_or_default() } else { Vec::new() };
    let mut uv_more: Vec<Vec<[f32; 2]>> = Vec::new();
    for k in 1..nuv.min(8) {
        if let Some(set) = g.uv_set(k) {
            uv_more.push(set.to_vec());
        }
    }
    ShapeData {
        verts: g.vertices.clone(),
        tris: Vec::new(),
        normals: g.normals.clone(),
        uvs,
        uv_more,
        colours,
        alphas,
    }
}

/// `NiTriShapeData`: the shared geometry, then the faces written out one by one.
fn shape_data(d: &ni::NiTriShapeData) -> ShapeData {
    let mut out = geom_data(&d.base.base);
    out.tris = d.triangles.clone();
    out
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
/// **The winding**, which is the part worth getting right. A strip's triangles alternate
/// handedness, so every other one has to be swapped back or the mesh comes out with half
/// its faces inside out — and a face read inside out is a normal pointing into the mesh,
/// which is §H4's failure arriving by another road: blades growing into a rock rather than
/// out of it, and a closed hull that reports the outside as its inside. Even triangles are
/// taken in order, odd ones with their first two points swapped, and a degenerate triangle
/// — two of its points the same, which is how a file joins two strips into one — is
/// dropped rather than kept as a zero-area face. That is what OpenMW does with the same
/// record, deliberately: two readers of one format that disagree is a bug whichever one is
/// wrong. The strips land in the same `datas` slot as a shape's faces, already turned into
/// triangles, so nothing downstream has to know a shape was stripified.
fn strips_data(d: &ni::NiTriStripsData) -> ShapeData {
    let mut out = geom_data(&d.base.base);
    let nstrips = d.strip_lengths.len();
    // At most one triangle per point after the first two of each strip.
    out.tris = Vec::with_capacity(d.strips.len().saturating_sub(2 * nstrips));
    let mut at = 0usize;
    for &len in &d.strip_lengths {
        let len = len as usize;
        let Some(strip) = d.strips.get(at..at + len) else { break };
        at += len;
        for (i, w) in strip.windows(3).enumerate() {
            let &[a, b, c] = w else { continue };
            // A joined strip carries repeated points to get from one run to the next.
            if a == b || b == c || a == c {
                continue;
            }
            out.tris.push(if i % 2 == 0 { [a, b, c] } else { [b, a, c] });
        }
    }
    out
}

/// The maps of a NiTexturingProperty into `Materials`, by the slot each sits in.
///
/// Slot 0 is the base map, slot 2 the detail map (round 17h) and slot 4 the glow map
/// (round 17p) — OpenMW's `NiTexturingProperty::TextureType` is BaseTexture 0,
/// DarkTexture 1, DetailTexture 2, GlossTexture 3, GlowTexture 4, BumpTexture 5,
/// DecalTexture 6. Slot 1, the dark map, since round 17y; slot 6, the decal, since 18df;
/// slot 5, the bump map with its luma scale and offset and its 2x2 matrix, since 18dl.
/// Gloss is not drawn (Morrowind does not draw it). Each map's clamp mode is the file's
/// word held to 0..3 (round 18df: 0 clamp S and T, 1 clamp S wrap T, 2 wrap S clamp T, 3
/// wrap both), and the UV set it reads is kept beside it (round 17h: a detail map usually
/// has its own; round 18dj: the base map's set names its controller).
fn texturing(m: &mut Materials, at: usize, p: &ni::NiTexturingProperty) {
    for (i, slot) in p.texture_maps.iter().enumerate() {
        let Some(map) = slot else { continue };
        let (base, bump) = match map {
            ni::TextureMap::Map(x) => (x, None),
            ni::TextureMap::BumpMap(b) => (&b.base, Some(b)),
        };
        let src = base.texture.index;
        let uv_set = base.texture_index;
        let clamp = (base.clamp_mode as u32).min(3);
        match i {
            0 => {
                m.tex_source[at] = Some(src);
                m.base_clamp[at] = Some(clamp);
                m.base_uv_set[at] = Some(uv_set);
            }
            1 => m.dark_source[at] = Some((src, uv_set)),
            2 => m.detail_source[at] = Some((src, uv_set)),
            3 => m.gloss_source[at] = Some((src, uv_set)),
            4 => m.glow_source[at] = Some((src, uv_set)),
            5 => {
                if let Some(b) = bump {
                    let d = &b.displacement;
                    m.bump_source[at] = Some((
                        src,
                        uv_set,
                        b.luma_scale,
                        b.luma_offset,
                        [d[0][0], d[0][1], d[1][0], d[1][1]],
                    ));
                }
            }
            6 => m.decal_source[at] = Some((src, uv_set)),
            _ => {}
        }
    }
}

/// A colour held to 0..1 the way the game's fixed pipeline holds it.
fn clamp3(c: [f32; 3]) -> [f32; 3] {
    [c[0].clamp(0.0, 1.0), c[1].clamp(0.0, 1.0), c[2].clamp(0.0, 1.0)]
}

/// The record table from the typed stream (round 18gh), block by block, by block index.
///
/// Every table has one slot per block, and a block lands in the slot its kind belongs to:
/// the scene blocks in `nodes`, the geometry in `datas`, the properties in `mats`, the
/// controllers and their data each in their own, with exactly the values the byte reader
/// produced from the same bytes. Where the engine derives something, the packed
/// transform, a key group as `(time, value)` pairs, the texture file out of an
/// NiSourceTexture, the Euler channels and their order, the vertex colours as bytes, it is
/// derived here from the typed fields. A link out of range stays as the index it is and is
/// treated as none wherever a table is read, which is what the tables' `get` has always
/// done. Then the `.kf` is bound (round 18ag) and the skinned shapes posed, as before.
fn records_from_stream(s: &Stream, kf: Option<&KfSequence>) -> Records {
    records_from_stream_opts(s, kf, false)
}

/// `records_from_stream`, and with `keep_skin` every skinned shape kept as one that moves
/// (its skin, bones and weights), as a part to be hung on another file's skeleton needs.
fn records_from_stream_opts(s: &Stream, kf: Option<&KfSequence>, keep_skin: bool) -> Records {
    let n = s.blocks.len();
    // Where the walks start: the file's first root, or block 0 when the footer names
    // nothing the file has (the byte reader read such a footer as "root at 0" too).
    let root = s.roots.first().and_then(|l| l.at()).filter(|&r| r < n).map(|r| r as i32).unwrap_or(0);
    let mut nodes: Vec<Option<Node>> = vec![None; n];
    let mut datas: Vec<Option<ShapeData>> = vec![None; n];
    let mut extras: Vec<Option<ExtraString>> = vec![None; n];
    let mut types: Vec<String> = Vec::with_capacity(n);
    let mut mats = Materials {
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
    };
    let mut skins: Vec<Option<SkinInst>> = vec![None; n];
    let mut skin_datas: Vec<Option<SkinData>> = vec![None; n];
    let mut pctrls: Vec<Option<ParticleCtrl>> = vec![None; n];
    let mut pdatas: Vec<Option<ParticleData>> = vec![None; n];
    let mut pmods: Vec<Option<ParticleMod>> = vec![None; n];
    let mut colour_data: Vec<Option<Vec<[f32; 5]>>> = vec![None; n];
    let mut uvctrls: Vec<Option<(i32, i32, i32, f32, f32, f32, f32, u32)>> = vec![None; n];
    let mut uvdatas: Vec<Option<UvAnim>> = vec![None; n];
    let mut flipctrls: Vec<Option<(i32, FlipAnim, Vec<i32>)>> = vec![None; n];
    let mut kfctrls: Vec<Option<KfCtrl>> = vec![None; n];
    let mut kfdatas: Vec<Option<KfData>> = vec![None; n];
    let mut morphctrls: Vec<Option<MorphCtrl>> = vec![None; n];
    let mut morphdatas: Vec<Option<MorphData>> = vec![None; n];
    let mut alphactrls: Vec<Option<(i32, u16, f32, f32, f32, f32, i32, i32)>> = vec![None; n];
    let mut floatdatas: Vec<Option<Vec<(f32, f32)>>> = vec![None; n];
    let mut visctrls: Vec<Option<KfCtrl>> = vec![None; n];
    let mut visdatas: Vec<Option<Vec<(f32, f32)>>> = vec![None; n];
    let mut lights: Vec<Option<([f32; 3], f32)>> = vec![None; n];
    // Round 18dl: NiTextureEffect by record index - (source texture, clamp mode).
    let mut texeffects: Vec<Option<(i32, u32)>> = vec![None; n];
    let mut seq: Option<(i32, i32)> = None;
    let mut textkeys: Vec<Option<Vec<(f32, String)>>> = vec![None; n];

    for (i, b) in s.blocks.iter().enumerate() {
        let name = b.name();
        types.push(name.to_string());
        match b {
            ni::NiType::NiNode(x) => nodes[i] = Some(group_node(name, x)),
            ni::NiType::RootCollisionNode(x) => nodes[i] = Some(group_node(name, &x.base)),
            ni::NiType::AvoidNode(x) => nodes[i] = Some(group_node(name, &x.base)),
            ni::NiType::NiBSParticleNode(x) => nodes[i] = Some(group_node(name, &x.base)),
            ni::NiType::NiBSAnimationNode(x) => nodes[i] = Some(group_node(name, &x.base)),
            ni::NiType::NiBillboardNode(x) => nodes[i] = Some(group_node(name, &x.base)),
            ni::NiType::NiCollisionSwitch(x) => nodes[i] = Some(group_node(name, &x.base)),
            ni::NiType::NiSortAdjustNode(x) => nodes[i] = Some(group_node(name, &x.base)),
            // A switch node draws whichever child is the live one.
            ni::NiType::NiSwitchNode(x) => {
                let mut nd = group_node(name, &x.base);
                nd.active_index = x.active_index as usize;
                nodes[i] = Some(nd);
            }
            /* A level-of-detail node: the same subtree at several detail levels, one
            chosen by how far away the camera is. Its children are the levels and
            `lod_levels` their distance ranges, by index. Round 18gi: "level 0 is the
            nearest" was an assumption, and Improved Lights for All Shaders' sconces
            break it - `light_sconce10.nif` hangs a billboarded fire sprite under a
            LOD node whose one level is 2500 to 4000 units, a few lit pixels for the
            distant land, and the walk drew it at every distance (Robin: "a large
            billboarded particle that is static in front of the lamp"). The ranges
            are kept and `live_children` picks the level the preview's distance, 0,
            falls in; a far-only level is drawn by nothing. (Round 18gh: the byte
            reader read the file's index word as part of the LOD centre and so never
            saw it; `active_index` is left alone here on purpose.) */
            ni::NiType::NiLODNode(x) => {
                let mut nd = group_node(name, &x.base.base);
                nd.lod_levels = x.lod_levels.clone();
                nodes[i] = Some(nd);
            }
            ni::NiType::NiTriShape(x) => nodes[i] = Some(shape_node(name, &x.base.base)),
            ni::NiType::NiTriStrips(x) => nodes[i] = Some(shape_node(name, &x.base.base)),
            ni::NiType::NiAutoNormalParticles(x) => nodes[i] = Some(shape_node(name, &x.base.base)),
            ni::NiType::NiRotatingParticles(x) => nodes[i] = Some(shape_node(name, &x.base.base)),
            ni::NiType::NiParticles(x) => nodes[i] = Some(shape_node(name, &x.base)),
            /* Round 17y: the lights a mesh carries. Glow in the Dahrk hangs a NiPointLight
            under a window's AttachLight and reads its colour and, as the node's *scale*,
            its radius (main.lua: "the radius is stored as scale"); the page puts that
            light in the room while the sunlit branch shows. The node goes in `nodes`
            under the same index as its colour and dimmer in `lights`. */
            ni::NiType::NiPointLight(x) => {
                nodes[i] = Some(node_of(name, &x.base.base.base));
                lights[i] = Some((x.base.diffuse_color, x.base.dimmer));
            }
            ni::NiType::NiSpotLight(x) => {
                nodes[i] = Some(node_of(name, &x.base.base.base.base));
                lights[i] = Some((x.base.base.diffuse_color, x.base.base.dimmer));
            }
            ni::NiType::NiDirectionalLight(x) => {
                nodes[i] = Some(node_of(name, &x.base.base.base));
                lights[i] = Some((x.base.diffuse_color, x.base.dimmer));
            }
            ni::NiType::NiAmbientLight(x) => {
                nodes[i] = Some(node_of(name, &x.base.base.base));
                lights[i] = Some((x.base.diffuse_color, x.base.dimmer));
            }
            /* Round 18dl: the environment map. A NiTextureEffect hung on a node's effect
            list puts a sphere-mapped texture over everything below it, added after the
            lighting - the Telvanni crystals and portals in vanilla, and the fake bump
            mapping mods use (Telvanni Bump Maps: a bump map on NiTexturingProperty slot
            5 perturbs where the environment map is read). Only an environment map (type
            2) by sphere mapping (coordinate generation 2) is kept, as OpenMW keeps only
            that (`handleTextureEffect`). */
            ni::NiType::NiTextureEffect(x) => {
                nodes[i] = Some(node_of(name, &x.base.base));
                if x.texture_type == 2 && x.coordinate_generation_type == 2 {
                    texeffects[i] = Some((x.source_texture.index, (x.texture_clamp as u32).min(3)));
                }
            }
            ni::NiType::NiAutoNormalParticlesData(x) => pdatas[i] = Some(particle_data(&x.base, false)),
            ni::NiType::NiParticlesData(x) => pdatas[i] = Some(particle_data(x, false)),
            ni::NiType::NiRotatingParticlesData(x) => pdatas[i] = Some(particle_data(&x.base, true)),
            ni::NiType::NiParticleSystemController(x) => pctrls[i] = Some(particle_ctrl(x)),
            ni::NiType::NiBSPArrayController(x) => pctrls[i] = Some(particle_ctrl(&x.base)),
            /* Round 17m: the UV animation. The controller says when and how fast; the data
            says what the four channels do. Round 18dj: and the texture set, which of
            the shape's maps the controller moves: those whose NiTexturingProperty slot
            reads this UV set (OpenMW's `handleTextureControllers`: "UVController should
            work only for textures which use a given UV Set, usually 0"). Every vanilla
            controller says 0, the base map's set; Memento Mori's Ghostgate dome has a
            second one on set 1 for its dark map, OAAB's lava one for its decal, Tamriel
            Rebuilt's bulb mushrooms one for their glow map alone. */
            ni::NiType::NiUVController(x) => {
                let t = &x.base;
                uvctrls[i] = Some((
                    t.next.index,
                    t.target.index,
                    x.data.index,
                    t.frequency,
                    t.phase,
                    t.start_time,
                    t.stop_time,
                    x.texture_set as u32,
                ));
            }
            /* Round 18ey: the flipbook, kept whole: the texture slot, the time the first
            frame shows at, the seconds a frame lasts, and the NiSourceTexture
            references, which `flip_of` resolves to file names. */
            ni::NiType::NiFlipController(x) => {
                let t = &x.base;
                let a = FlipAnim {
                    slot: x.affected_map,
                    frequency: t.frequency,
                    phase: t.phase,
                    start: t.start_time,
                    stop: t.stop_time,
                    tex_start: x.flip_start_time,
                    delta: x.secs_per_frame,
                    frames: Vec::new(),
                };
                flipctrls[i] = Some((t.target.index, a, indices(&x.textures)));
            }
            ni::NiType::NiUVData(x) => {
                uvdatas[i] = Some(UvAnim {
                    u_trans: float_keys(&x.u_offset_data.keys),
                    v_trans: float_keys(&x.v_offset_data.keys),
                    u_scale: float_keys(&x.u_tiling_data.keys),
                    v_scale: float_keys(&x.v_tiling_data.keys),
                    ..UvAnim::default()
                });
            }
            /* Round 17y: the node animations. Robin's `f\active_blight_large.nif` is a
            stack of translucent sheets that turn slowly upwards, and a still frame of it
            is "a still cut out thing in the world". The controller says when and how
            fast; the data says where the node goes. */
            ni::NiType::NiKeyframeController(x) => kfctrls[i] = Some(kf_ctrl(&x.base, x.data.index)),
            /* The visibility animation: a shape switched on and off by its keys, held from
            key to key - a lightning bolt's frames, one shape shown at a time. */
            ni::NiType::NiVisController(x) => visctrls[i] = Some(kf_ctrl(&x.base, x.data.index)),
            ni::NiType::NiVisData(x) => {
                visdatas[i] = Some(x.keys.iter().map(|k| (k.time, if k.value != 0 { 1.0 } else { 0.0 })).collect())
            }
            ni::NiType::NiKeyframeData(x) => kfdatas[i] = Some(kf_data(x)),
            /* Round 18au: the morph animation, the moths' wingbeat. The controller's
            always-update byte (round 18gf, backend-plan N3) is the model's to read and
            is not wanted here; the data is a `relativeTargets` flag and, per morph, its
            weight keys and one vector per vertex. Only the time and the weight of a key
            are kept, as `float_keys` keeps them. */
            ni::NiType::NiGeomMorpherController(x) => {
                let c = kf_ctrl(&x.base.base, x.base.data.index);
                morphctrls[i] = Some(MorphCtrl {
                    next: c.next,
                    flags: c.flags,
                    frequency: c.frequency,
                    phase: c.phase,
                    start: c.start,
                    stop: c.stop,
                    target: c.target,
                    data: c.data,
                });
            }
            ni::NiType::NiMorphData(x) => {
                morphdatas[i] = Some(MorphData {
                    relative: x.relative_targets,
                    morphs: x.targets.iter().map(|t| (float_keys(&t.keys), t.vertices.clone())).collect(),
                });
            }
            /* And the material's fade: a NiAlphaController on the NiMaterialProperty,
            its keys in a NiFloatData. The blight's sheets are born clear, thicken and
            thin again as they rise. */
            ni::NiType::NiAlphaController(x) => {
                let t = &x.base.base;
                alphactrls[i] = Some((
                    t.next.index,
                    t.flags,
                    t.frequency,
                    t.phase,
                    t.start_time,
                    t.stop_time,
                    t.target.index,
                    x.base.data.index,
                ));
            }
            ni::NiType::NiFloatData(x) => floatdatas[i] = Some(float_keys(&x.keys)),
            ni::NiType::NiGravity(x) => {
                pmods[i] = Some(ParticleMod::Gravity {
                    next: x.base.next.index,
                    decay: x.decay,
                    force: x.strength,
                    kind: x.force_type as u32,
                    position: x.position,
                    direction: x.direction,
                });
            }
            ni::NiType::NiParticleGrowFade(x) => {
                pmods[i] = Some(ParticleMod::GrowFade {
                    next: x.base.next.index,
                    grow: x.grow_time,
                    fade: x.fade_time,
                });
            }
            ni::NiType::NiParticleColorModifier(x) => {
                pmods[i] = Some(ParticleMod::Colour { next: x.base.next.index, data: x.color_data.index });
            }
            ni::NiType::NiParticleRotation(x) => {
                pmods[i] = Some(ParticleMod::Rotation {
                    next: x.base.next.index,
                    random_axis: x.random_initial_axis,
                    axis: x.initial_axis,
                    speed: x.rotation_speed,
                });
            }
            ni::NiType::NiParticleBomb(x) => pmods[i] = Some(ParticleMod::Other { next: x.base.next.index }),
            /* Round 18bg (§G6): a colour track, `(time, r, g, b, a)` a key. An emptily
            keyed `NiColorData` is not exotic: it is what a particle system saves when
            somebody clears a colour curve and leaves the modifier in place; the model
            reads it as a bare count, as the format writes it. */
            ni::NiType::NiColorData(x) => {
                // Only the time and the colour are kept, whatever the key's shape: a curve
                // read as a line is a small error, and the model has measured the tangents.
                let five = |t: f32, v: &[f32; 4]| [t, v[0], v[1], v[2], v[3]];
                colour_data[i] = Some(match &x.keys {
                    ni::NiColorKey::Lin(keys) => keys.iter().map(|k| five(k.time, &k.value)).collect(),
                    ni::NiColorKey::Bez(keys) => keys.iter().map(|k| five(k.time, &k.value)).collect(),
                    ni::NiColorKey::Tcb(keys) => keys.iter().map(|k| five(k.time, &k.value)).collect(),
                });
            }
            ni::NiType::NiTriShapeData(x) => datas[i] = Some(shape_data(x)),
            /* Round 18bg (§G2): the strips land in the same `datas` slot, already turned
            into triangles, so nothing downstream — the draw walk, the collision walk,
            the skinner, the mesh scatter — has to know a shape was stripified. */
            ni::NiType::NiTriStripsData(x) => datas[i] = Some(strips_data(x)),
            // The markers: a string value and the next link in the chain.
            ni::NiType::NiStringExtraData(x) => {
                extras[i] = Some(ExtraString { value: x.value.clone(), next: x.base.next.index });
            }
            /* Round 18ag: a `.kf` file's root - an NiObjectNET and nothing more: a name,
            the extra-data chain (the text keys, then one string per track naming the
            node it moves) and the controller chain (the tracks, in the same order). */
            ni::NiType::NiSequenceStreamHelper(x) => {
                seq = Some((x.base.extra_data.index, x.base.controller.index));
            }
            /* And the text keys: `Idle: Start`, `Idle: Stop` and their kin are how a clip
            is found inside the file's one timeline. */
            ni::NiType::NiTextKeyExtraData(x) => {
                textkeys[i] = Some(x.keys.iter().map(|k| (k.time, k.value.clone())).collect());
                // The chain runs on through it: kept as an extra with no marker in it.
                extras[i] = Some(ExtraString { value: String::new(), next: x.base.next.index });
            }
            /* The skin of a skinned shape (round 12 item 3): which data, which skeleton
            root, which bones. At 4.0.0.2 there is no skin partition. */
            ni::NiType::NiSkinInstance(x) => {
                skins[i] =
                    Some(SkinInst { data: x.data.index, root: x.root.index, bones: indices(&x.bones) });
            }
            // NiSkinData: the skin's own transform and, per bone, the transform from skin
            // space into that bone's space plus the vertices it moves and by how much.
            ni::NiType::NiSkinData(x) => {
                let bones = x
                    .bone_data
                    .iter()
                    .map(|b| SkinBone {
                        trafo: skin_trafo(&b.rotation, &b.translation, b.scale),
                        weights: b.vertex_weights.clone(),
                    })
                    .collect();
                skin_datas[i] =
                    Some(SkinData { trafo: skin_trafo(&x.rotation, &x.translation, x.scale), bones });
            }
            // The properties a shape is drawn with, by record index, for the walks to
            // resolve nearest-first.
            ni::NiType::NiTexturingProperty(x) => texturing(&mut mats, i, x),
            /* The texture file an NiSourceTexture names, when it names one. Round 18gf
            (backend-plan N2): a texture with an internal pixel record, or with neither
            source, names no file; the model reads both bytes and the link, and only an
            external source lands here. */
            ni::NiType::NiSourceTexture(x) => match &x.source {
                ni::TextureSource::External(f) => mats.tex_file[i] = Some(f.clone()),
                /* A texture held in the file (its pixels in an NiPixelData record): named
                by that record, so a front end can fetch it like a file (`EMBEDDED`). */
                ni::TextureSource::Internal(px) => {
                    if let Some(at) = px.at() {
                        mats.tex_file[i] = Some(format!("{EMBEDDED}{at}"));
                    }
                }
            },
            /* Clamped to 0..1 the way the game's fixed pipeline clamps them. RR Better
            Meshes' pot and basket carry 255.0 in every channel — a 0..255 export in a
            0..1 field — and drew pure white here while the game shows them fine
            (Robin's round 12 item 2, found in round 14). Round 17m: the emissive
            colour, which is how a lit lantern is lit (OpenMW's objects.frag: `lighting
            = diffuseColor*diffuseLight + ambientColor*ambientLight +
            emissionColor*emissiveMult`). Round 18de: the ambient colour, which is
            different from the diffuse on 1,784 of Morrowind's own static shapes - the
            shipwrecks, the colony piers, the Daedric walls sit at 0.1 ambient under a
            white diffuse, which is what keeps them dark in shade. Round 17y: the head of
            the material's controller chain, where a NiAlphaController hangs. */
            ni::NiType::NiMaterialProperty(x) => {
                mats.material[i] = Some((
                    clamp3(x.diffuse_color),
                    x.alpha.clamp(0.0, 1.0),
                    clamp3(x.emissive_color),
                    clamp3(x.ambient_color),
                ));
                mats.material_ctrl[i] = x.base.base.controller.index;
            }
            ni::NiType::NiAlphaProperty(x) => mats.alpha[i] = Some((x.base.flags, x.test_ref)),
            ni::NiType::NiStencilProperty(x) => mats.stencil[i] = Some(x.draw_mode as u32),
            /* Round 18dg: the flags are kept - bit 0 tests the depth, bit 1 writes it.
            Absent, a shape does both; 331 vanilla blended shapes (the kwama eggs, the
            magic targets) and the mods' pools and fountains carry flags 1, test only,
            which is what lets what is behind them draw after them. */
            ni::NiType::NiZBufferProperty(x) => mats.zbuffer[i] = Some(x.base.flags),
            // Vertex mode 0 ignore, 1 emissive, 2 ambient+diffuse; lighting mode 0
            // emissive (unlit), 1 emissive+ambient+diffuse (lit).
            ni::NiType::NiVertexColorProperty(x) => {
                mats.vcol_prop[i] = Some((x.source_vertex_mode as u32, x.lighting_mode as u32));
            }
            // Every other block the format has is read by the model and wanted by no
            // view: the shade, wireframe, dither, specular and fog properties, the
            // cameras, the visibility and look-at controllers, the pixel data, and the
            // rest. Their slots stay empty.
            _ => {}
        }
    }

    let skin_src = vec![None; n];
    let mut rec = Records {
        root,
        kfctrls,
        kfdatas,
        morphctrls,
        morphdatas,
        seq,
        textkeys,
        skel_animated: false,
        skin_src,
        alphactrls,
        floatdatas,
        visctrls,
        visdatas,
        lights,
        texeffects,
        nodes,
        datas,
        extras,
        types,
        mats,
        skins,
        skin_datas,
        pctrls,
        pdatas,
        pmods,
        colour_data,
        uvctrls,
        uvdatas,
        flipctrls,
    };
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
                && rec
                    .kfdatas
                    .get(c.data.max(0) as usize)
                    .map(|d| d.as_ref().map(|d| !d.is_empty()).unwrap_or(false))
                    .unwrap_or(false)
        });
    }
    skin_records(&mut rec);
    rec
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
    if out.tracks.is_empty() {
        None
    } else {
        Some(out)
    }
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
        exact.or_else(|| {
            rec.nodes
                .iter()
                .position(|n| n.as_ref().map(|n| n.name.eq_ignore_ascii_case(name)).unwrap_or(false))
        })
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
            if idx as i32 == rec.root {
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
        if idx as i32 == rec.root {
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
        // Only the selected clip plays; keep its boundary keys for interpolation.
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

/// The type name of every block in file order, the quickest census of what a mesh carries.
/// Round 18gh: this used to come with a count of records found by scanning forward when
/// the reader could not measure one; the typed model measures every block or refuses the
/// file, so there is nothing to count and the pair is gone.
///
/// `None` when the file is not a NIF the model reads.
pub fn record_types(buf: &[u8]) -> Option<Vec<String>> {
    let s = read_stream(buf)?;
    Some(s.blocks.iter().map(|b| b.name().to_string()).collect())
}

/// Parses a NIF and returns the collision geometry plus bounds.
pub fn parse(buf: &[u8]) -> Option<MeshGeom> {
    parse_with_kf(buf, None)
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
    let rec = read_records_kf(buf, kf)?;
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
    let (root, nodes, datas, extras, types) = (rec.root, rec.nodes, rec.datas, rec.extras, rec.types);
    build(root, &nodes, &datas, &extras, &types, &poses)
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
    /// The gloss map (NiTexturingProperty slot 3), on the base UVs; empty when the shape
    /// has none.
    pub gloss_tex: String,
    /// Which UV set the glow map names.
    ///
    /// Round 17p assumed this was always 0 - "every glow map in Morrowind and in OAAB
    /// names 0 (`tests/glow.rs` asserts it); a shape asking for another set is drawn with
    /// set 0 rather than dropped, and the count is what would say the assumption had
    /// broken". Round 18ev is the shape that broke it: Tamriel Data's
    /// `tr\f\tr_flora_ow_blbshrm_02.nif` has two UV sets and names **1** for its glow, and
    /// drawn with set 0 - whose u runs -0.46 to 1.91 against set 1's 0.33 to 0.79 - the
    /// mask was tiled across the whole mushroom and then scrolled by its own animation.
    /// Robin: "black and white animated scrolling textures rather than what is supposed to
    /// be there. It looks like a mask is scrolling instead of the texture."
    pub glow_uv_set: u32,
    /// Round 18ev: the glow map's own UVs, one pair per vertex - empty unless `glow_tex`
    /// is set and the shape carries the set it names. The detail, dark and decal maps have
    /// had theirs since 17h, 17y and 18df; the glow is the one that never got it.
    pub uv_glow: Vec<f32>,
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
    /// Round 18ey: the flipbook on one of this shape's maps, when its texturing property
    /// carries a NiFlipController - Tamriel Rebuilt's and OAAB's animated water, and the
    /// same shape of thing on fires and lava. `None` on nearly every shape.
    pub flip: Option<FlipAnim>,
    /// Round 18ez: under a NiBillboardNode, and how it turns. `None` on nearly all shapes.
    pub billboard: Option<Billboard>,
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
        // Wraithguard (0019): the face normal with `glam::Vec3` (the same operations in the same
        // order as the lines it replaced; `tests/wraithguard_glam_face.rs` pins it bit for bit).
        let (a, b, c) = (Vec3::from(t[0]), Vec3::from(t[1]), Vec3::from(t[2]));
        let n = (b - a).cross(c - a);
        let len = n.length().max(1e-6);
        for p in [a, b, c] {
            pos.extend_from_slice(&p.to_array());
            nrm.extend_from_slice(&(n / len).to_array());
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
        uv_glow: Vec::new(),
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
        flip: None,
        billboard: None,
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
fn node_trafo(root: i32, idx: i32, trafo: &[f32; 12], scale: f32, n: &Node) -> ([f32; 12], f32) {
    if idx == root {
        (*trafo, scale)
    } else {
        mul(trafo, scale, &n.trafo, n.scale)
    }
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
        match if i != rec.root { kf_of(i) } else { None } {
            Some((ctrl, data)) => {
                chain.push(AnimLink { pre: (m, s), local: (nd.trafo, nd.scale), ctrl, data });
                m = ident;
                s = 1.0;
            }
            None => {
                let (nm, ns) = node_trafo(rec.root, i, &m, s, nd);
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

/// The visibility animation over a shape: every NiVisController whose target is the shape
/// or one of its ancestors, switched on and keyed.
fn vis_of(rec: &Records, parent: &[i32], shape: i32) -> Vec<AlphaAnim> {
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
                out.push(AlphaAnim {
                    flags: c.flags,
                    frequency: c.frequency,
                    phase: c.phase,
                    start: c.start,
                    stop: c.stop,
                    keys: keys.clone(),
                });
            }
        }
    }
    out
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

    /* Round 18ey: the flipbook a texturing property carries, with its frames resolved to
    the file names they name. By the controller's own `target`, like the UV animations
    above - a NiFlipController names the NiTexturingProperty it flips, so the shape asks
    for the property it is already reading its maps out of rather than guessing from the
    node tree. A controller whose frames do not resolve, or that has only one, is
    dropped here: `FlipAnim::moves`. */
    let flip_of = |pi: i32| -> Option<FlipAnim> {
        for (target, a, refs) in rec.flipctrls.iter().flatten() {
            if *target != pi {
                continue;
            }
            let mut a = a.clone();
            a.frames = refs
                .iter()
                .filter_map(|&x| rec.mats.tex_file.get(x.max(0) as usize).cloned().flatten())
                .collect();
            if a.frames.len() == refs.len() && a.moves() {
                return Some(a);
            }
        }
        None
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
            if let Some(Some((next, flags, frequency, phase, start, stop, _target, data))) =
                rec.alphactrls.get(ci as usize)
            {
                if let Some(Some(keys)) = rec.floatdatas.get((*data).max(0) as usize) {
                    if !keys.is_empty() {
                        return Some(AlphaAnim {
                            flags: *flags,
                            frequency: *frequency,
                            phase: *phase,
                            start: *start,
                            stop: *stop,
                            keys: keys.clone(),
                        });
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

    let mut out: Vec<DrawPart> = Vec::new();
    // Each entry carries the properties inherited from above it, the UV animation, which
    // branch of a day/night switch it is under (round 17x; 0 for none), and (round 17y)
    // the animated nodes passed on the way down — the composed transform restarts at
    // each of them, so a shape's geometry is baked relative to the nearest one.
    #[allow(clippy::type_complexity)]
    let mut stack: Vec<(
        i32,
        [f32; 12],
        f32,
        u32,
        Vec<i32>,
        Vec<(u32, UvAnim)>,
        u8,
        Vec<AnimLink>,
        Option<(i32, u32)>,
        Option<Billboard>,
    )> = vec![(rec.root, ident, 1.0, 0, Vec::new(), Vec::new(), 0, Vec::new(), None, None)];
    let mut guard = 0u32;

    while let Some((idx, trafo, scale, depth, inherited, in_uv, dn, mut chain, in_env, in_bb)) = stack.pop() {
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
        if ty == "AvoidNode" || ty == "RootCollisionNode" || ty == "NiBSParticleNode" || is_dynamic_effect(ty)
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
        /* Wraithguard (0020): a shape OpenMW's `distortion` shader effect owns is not a
        surface - the game bends what is behind it by its texture and draws no colour of its
        own. TR's and OAAB's animated water carries one over the water (`Tri Water
        Distortion2`, 90 frames of `omw_distortion\water\NN.dds`); painted as a texture it was
        a sheet of normal-map rainbow on top of the water. */
        if omw_shader(&rec.extras, n.extra).distortion {
            continue;
        }
        /* Round 17y: a node that moves. Its resting transform is *not* composed in — the
        page puts the animated one there — so everything below it is baked in its own
        frame, and the transform gathered so far (root to its parent) travels with the
        link. The root is never a link: the game ignores the root's transform, and its
        animation with it. */
        /* Round 18ez: a billboard node (not the root - a billboard *root* is the whole
        mesh, walked as a plain node since 17c). Its subtree is kept in the node's own
        frame: rotation and scale baked into the vertices, the translation not, so the
        page can turn the whole subtree about the node's origin per instance. Innermost
        wins where one sits under another. */
        let bb = if ty == "NiBillboardNode" && idx != rec.root {
            let (whole, _) = mul(&trafo, scale, &n.trafo, n.scale);
            Some(Billboard {
                mode: ((n.flags >> 5) & 3) as u8,
                origin: [whole[9], whole[10], whole[11]],
                scale,
            })
        } else {
            in_bb
        };
        let (m, s) = match if idx != rec.root { kf_of(idx) } else { None } {
            Some((ctrl, data)) => {
                chain.push(AnimLink { pre: (trafo, scale), local: (n.trafo, n.scale), ctrl, data });
                (ident, 1.0)
            }
            None if ty == "NiBillboardNode" && idx != rec.root => {
                // The node's own rotation and scale, from the origin: what the shader turns.
                let mut r = n.trafo;
                r[9] = 0.0;
                r[10] = 0.0;
                r[11] = 0.0;
                (r, n.scale)
            }
            None => node_trafo(rec.root, idx, &trafo, scale, n),
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
            if own.is_empty() {
                in_uv
            } else {
                own
            }
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
                    let name = rec
                        .nodes
                        .get(c.max(0) as usize)
                        .and_then(|x| x.as_ref())
                        .map(|x| x.name.to_ascii_lowercase())
                        .unwrap_or_default();
                    let tag: u8 = match name.as_str() {
                        "off" => 1,
                        "on" => 2,
                        "int-day" => 3,
                        _ => match k {
                            0 => 1,
                            1 => 2,
                            2 => 3,
                            _ => 0,
                        },
                    };
                    if tag != 0 {
                        stack.push((
                            c,
                            m,
                            s,
                            depth + 1,
                            props.clone(),
                            uv.clone(),
                            tag,
                            chain.clone(),
                            env,
                            bb,
                        ));
                    }
                }
                continue;
            }
            for c in live_children(n) {
                stack.push((c, m, s, depth + 1, props.clone(), uv.clone(), dn, chain.clone(), env, bb));
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
                            /* Round 18ey: and the flipbook on it, from the same property
                            the base map came from - so a shape that inherits its
                            texture from an ancestor inherits that ancestor's flip with
                            it, and never a nearer property's. */
                            if part.flip.is_none() {
                                part.flip = flip_of(pi as i32);
                            }
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
                if part.glow_tex.is_empty() {
                    if let Some(Some((src, uv_set))) = rec.mats.glow_source.get(pi) {
                        if let Some(Some(f)) = rec.mats.tex_file.get(*src as usize) {
                            part.glow_tex = f.clone();
                            part.glow_uv_set = *uv_set;
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
            /* Round 18ev: and the glow's, which it should have had all along. Only written
            when the set it names is one the shape actually carries, so the commonest
            case by far - a glow on set 0 - stays exactly what it was: an empty vector,
            and the page draws the glow with the base UVs as before. */
            let gset = part.glow_uv_set;
            if !part.glow_tex.is_empty() && gset >= 1 {
                if let Some(sv) = d.uv_more.get(gset as usize - 1).filter(|s| s.len() == nv) {
                    part.uv_glow.reserve(nv * 2);
                    for v in 0..nv {
                        let uv = sv.get(v).copied().unwrap_or([0.0, 0.0]);
                        part.uv_glow.push(uv[0]);
                        part.uv_glow.push(uv[1]);
                    }
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
            let anim_for =
                |set: u32| -> Option<UvAnim> { uv.iter().find(|(t, _)| *t == set).map(|(_, a)| a.clone()) };
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
            match env.and_then(|(src, clamp)| {
                rec.mats.tex_file.get(src.max(0) as usize).cloned().flatten().map(|f| (f, clamp))
            }) {
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
            part.billboard = bb;
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
            let vis = vis_of(rec, &parent, idx);
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
            /* Round 18ho: the triangles in the order the GPU's vertex cache likes, the
            ones likely in front first. The same triangles and the same vertices, so
            nothing above (skins, morphs, UV sets, the part's bounds) and nothing after
            (the hull, the thumbnail, the payload) sees a difference; see `triorder`.
            Only when the vertex cache does not come out worse: a part the file has in good
            order is left as it is. */
            crate::triorder::reorder_triangles_if_not_worse(&mut part.idx, &part.pos);
            out.push(part);
            continue;
        }
        for &c in n.children.iter().rev() {
            stack.push((c, m, s, depth + 1, props.clone(), uv.clone(), dn, chain.clone(), env, bb));
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
    Some(MorphAnim {
        flags: c.flags,
        frequency: c.frequency,
        phase: c.phase,
        start: c.start,
        stop: c.stop,
        targets,
    })
}

/// Everything the viewport needs to draw a mesh.
///
/// The collision walk and this one answer different questions about the same file
/// and share nothing but the record table — which is the point: what the engine is
/// solid against and what it draws are genuinely different sets of nodes.
/// A fingerprint of a mesh's drawn geometry, **textures excluded** — a retexture (the
/// same shapes under different texture names) hashes identically, which is the whole
/// meaning of "the exact same mesh" in the twins question (Robin's choice: identical
/// geometry, however the file is named). Positions and indices, part by part in file
/// order, hashed as their little-endian bytes: a retexture is made by copying the .nif
/// and editing the texture strings, so the geometry bytes are byte-for-byte the same.
pub fn geometry_hash(parts: &[DrawPart]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    let eat = |h: &mut u64, bytes: &[u8]| {
        for b in bytes {
            *h ^= *b as u64;
            *h = h.wrapping_mul(0x100000001b3);
        }
    };
    for p in parts {
        eat(&mut h, &(p.pos.len() as u32).to_le_bytes());
        for v in &p.pos {
            eat(&mut h, &v.to_le_bytes());
        }
        eat(&mut h, &(p.idx.len() as u32).to_le_bytes());
        for i in &p.idx {
            eat(&mut h, &i.to_le_bytes());
        }
    }
    h
}

/// The distinct textures a mesh names, normalized, in part order — the same table
/// [`crate::mesh_scatter::Surface`] interns, said once here for callers that have
/// parts but no surface. Position in this list is what a twin copy maps assignments
/// through: the retexture's texture standing in the same place takes the rule.
pub fn part_textures(parts: &[DrawPart]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for p in parts {
        if p.tex.is_empty() {
            continue;
        }
        let t = crate::paths::norm_tex(&p.tex);
        if !out.contains(&t) {
            out.push(t);
        }
    }
    out
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
    /// Round 18hm: the material's ambient colour, used when the vertex colour property says
    /// the material's colours stand in for the particles' own (`vcol_mode` 0).
    pub ambient: [f32; 3],
    /// Round 18hm: how the nearest NiVertexColorProperty routes the particle colour, the
    /// file's own numbering as `DrawPart` keeps it: 0 SOURCE_IGNORE (the material's
    /// diffuse and ambient are used, the particle colour is not), 1 SOURCE_EMISSIVE (the
    /// particle colour is the emissive, the material gives diffuse and ambient), 2
    /// SOURCE_AMB_DIFF (the particle colour is diffuse and ambient). No property means 2,
    /// which is what the game and OpenMW do for a shape that carries vertex colours, and a
    /// particle system always carries them.
    pub vcol_mode: u8,
    /// Round 18hm: lighting mode 0 on that property. The particles are drawn unlit, the
    /// texture times the particle colour and nothing else.
    pub unlit: bool,
    /// Round 18hn: how the game lights this system, read against the engine's own class
    /// notes and measured by Robin in a cell with no light at all (the Caverns of
    /// Karstaag, 2026-10-09). 0: a NiRotatingParticles. Its data carries no normals and
    /// the engine generates none, so the fixed pipeline has no diffuse term for it: the
    /// ambient and the emissive, nothing else (the waterfall mist, black in that cell;
    /// round 18d's canalworks numbers). 1: a NiAutoNormalParticles, "normals [...]
    /// automatically generated to face the camera when rendered": a lit sheet, sun and
    /// lamps and ambient (the Dwemer steam and the furn mist, black in that cell). OpenMW
    /// lights both as 1 with one fixed normal, which is where round 18hm's model came from.
    /// What makes OAAB's crystal flakes glow in that cell is not a kind at all but the
    /// glow map below: a rule keyed on their NiParticleRotation modifier was tried and
    /// disproved by swapping the modifier between the flakes and the furn mist, which
    /// changed nothing.
    pub lighting: u8,
    /// Round 18hn: the glow map, the NiTexturingProperty's slot 4, added on top of the lit
    /// colour unlit as the statics' is. OAAB's crystal flakes carry their own base texture
    /// in it again, which is why the game draws them at the texture's full brightness in a
    /// lightless cell, the same from every angle, with the lamps' colours as a cast on top.
    /// Empty when there is none, which is every vanilla particle system.
    pub glow: String,
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
    let mut stack: Vec<(i32, [f32; 12], f32, u32, Vec<i32>)> = vec![(rec.root, ident, 1.0, 0, Vec::new())];
    let mut props_of: Vec<Vec<i32>> = vec![Vec::new(); n];
    let mut guard = 0u32;
    while let Some((idx, trafo, scale, depth, inherited)) = stack.pop() {
        guard += 1;
        if guard > 400_000 || depth > 64 || idx < 0 || idx as usize >= n {
            continue;
        }
        let Some(nd) = rec.nodes[idx as usize].as_ref() else { continue };
        let (m, s) = node_trafo(rec.root, idx, &trafo, scale, nd);
        world[idx as usize] = Some((m, s));
        let mut props = nd.props.clone();
        props.extend_from_slice(&inherited);
        props_of[idx as usize] = props.clone();
        /* Round 17y: one branch of a switch, as the draw walk takes one. Ashfall's kettles
        keep their steam under `SWITCH_KETTLE_STEAM` → `ON`, which the mod's script
        turns on while the water boils; a kettle placed in a cell sits on `OFF`, and so
        did the game's, steamless, while the preview boiled every one of them. The
        day/night switch is the exception here as it is there: every branch is walked. */
        if (nd.kind == "NiSwitchNode" && !nd.name.eq_ignore_ascii_case("NightDaySwitch")) || nd.kind == "NiLODNode" {
            for c in live_children(nd) {
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
        if !(nd.kind == "NiAutoNormalParticles"
            || nd.kind == "NiRotatingParticles"
            || nd.kind == "NiParticles")
        {
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
        other controller types this file knows are found by their target rather than
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
            ambient: [1.0, 1.0, 1.0],
            vcol_mode: 2,
            unlit: false,
            lighting: 1,
            glow: String::new(),
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
        let mut vcol_seen = false;
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
            // Round 18hn: the glow slot, from the nearest texturing property that has one.
            if sys.glow.is_empty() {
                if let Some(Some((src, _uv_set))) = rec.mats.glow_source.get(pi) {
                    if let Some(Some(f)) = rec.mats.tex_file.get(*src as usize) {
                        sys.glow = f.clone();
                    }
                }
            }
            if let Some(Some((diffuse, a, emissive, ambient))) = rec.mats.material.get(pi) {
                // Round 18be (G4): "have I been told yet" — see `draw_parts`.
                if !mat_seen {
                    mat_seen = true;
                    sys.diffuse = *diffuse;
                    sys.mat_alpha = *a;
                    sys.emissive = *emissive;
                    sys.ambient = *ambient;
                }
            }
            /* Round 18hm: the nearest NiVertexColorProperty, the shape's own list before
            what it inherited, as `draw_parts` reads it for a shape. OpenMW collects the
            same chain for a particle system (nifloader, `collectDrawableProperties`), and
            the game sets its material sources from it like for any shape: a root-level
            SOURCE_IGNORE over a white 'dust' material (OAAB's lougian_crystals) means the
            flakes are a lit white material, whatever colour the controller gives them. */
            if !vcol_seen {
                if let Some(Some((vmode, lmode))) = rec.mats.vcol_prop.get(pi) {
                    vcol_seen = true;
                    sys.vcol_mode = (*vmode).min(2) as u8;
                    sys.unlit = *lmode == 0;
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
        // Round 18hn: the lighting kind, see the field. The shape type is the data's.
        let rotating_shape = sys.data.as_ref().map(|d| d.rotating).unwrap_or(false);
        sys.lighting = if rotating_shape { 0 } else { 1 };
        out.push(sys);
    }
    out
}

/// `parse_particles` as JSON for the page.
pub fn particles_json(buf: &[u8]) -> String {
    particles_json_of(&parse_particles(buf))
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
        o.push_str(&format!("\"name\":\"{}\",\"tex\":\"{}\",\"glow\":\"{}\"", escape(&s.name), escape(&s.tex), escape(&s.glow)));
        o.push_str(&format!(
            ",\"alphaFlags\":{},\"matAlpha\":{},\"diffuse\":{},\"emissive\":{},\"ambient\":{},\"vcolMode\":{},\"unlit\":{},\"lighting\":{}",
            s.alpha_flags,
            num(s.mat_alpha),
            v3(&s.diffuse),
            v3(&s.emissive),
            v3(&s.ambient),
            s.vcol_mode,
            s.unlit,
            s.lighting
        ));
        o.push_str(&format!(
            ",\"node\":{},\"nodeScale\":{},\"emitter\":{},\"emitterScale\":{}",
            m12(&s.node.0),
            num(s.node.1),
            m12(&s.emitter.0),
            num(s.emitter.1)
        ));
        o.push_str(&format!(
            ",\"velocity\":{},\"velocityRandom\":{},\"verticalDir\":{},\"verticalAngle\":{},\"horizontalDir\":{},\"horizontalAngle\":{}",
            num(c.velocity), num(c.velocity_random), num(c.vertical_dir), num(c.vertical_angle), num(c.horizontal_dir), num(c.horizontal_angle)
        ));
        o.push_str(&format!(
            ",\"color\":[{},{},{},{}],\"size\":{},\"emitStart\":{},\"emitStop\":{},\"emitRate\":{},\"lifetime\":{},\"lifetimeRandom\":{},\"emitFlags\":{},\"offsetRandom\":{}",
            num(c.colour[0]), num(c.colour[1]), num(c.colour[2]), num(c.colour[3]), num(c.size), num(c.emit_start), num(c.emit_stop),
            num(c.emit_rate), num(c.lifetime), num(c.lifetime_random), c.emit_flags, v3(&c.offset_random)
        ));
        o.push_str(&format!(
            ",\"ctrlFlags\":{},\"frequency\":{},\"start\":{},\"stop\":{},\"numParticles\":{},\"numValid\":{}",
            c.flags,
            num(c.frequency),
            num(c.start),
            num(c.stop),
            c.num_particles,
            c.num_valid
        ));
        match &s.data {
            Some(d) => {
                let mean =
                    if d.sizes.is_empty() { 1.0 } else { d.sizes.iter().sum::<f32>() / d.sizes.len() as f32 };
                let mean_alpha = if d.colours.is_empty() {
                    1.0
                } else {
                    d.colours.iter().map(|c| c[3]).sum::<f32>() / d.colours.len() as f32
                };
                o.push_str(&format!(",\"data\":{{\"nverts\":{},\"radius\":{},\"meanSize\":{},\"meanAlpha\":{},\"rotating\":{}}}", d.nverts, num(d.radius), num(mean), num(mean_alpha), d.rotating));
            }
            None => o.push_str(",\"data\":null"),
        }
        o.push_str(",\"gravity\":[");
        for (gi, g) in s.gravity.iter().enumerate() {
            if gi > 0 {
                o.push(',');
            }
            o.push_str(&format!(
                "{{\"direction\":{},\"position\":{},\"force\":{},\"kind\":{},\"decay\":{}}}",
                v3(&g.0),
                v3(&g.1),
                num(g.2),
                g.3,
                num(g.4)
            ));
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

pub fn parse_draw(buf: &[u8]) -> Option<Vec<DrawPart>> {
    let rec = read_records(buf)?;
    Some(draw_parts(&rec))
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

pub fn parse_draw_lit_kf(buf: &[u8], kf: Option<&KfSequence>) -> Option<(Vec<DrawPart>, Option<[f32; 3]>)> {
    let rec = read_records_kf(buf, kf)?;
    let at = attach_light(&rec);
    Some((draw_parts(&rec), at))
}

pub fn parse_draw_lit(buf: &[u8]) -> Option<(Vec<DrawPart>, Option<[f32; 3]>)> {
    let rec = read_records(buf)?;
    let at = attach_light(&rec);
    Some((draw_parts(&rec), at))
}

/// Round 17y: the light hanging under `AttachLight`, when the mesh carries one — its
/// diffuse colour and its radius, which Glow in the Dahrk keeps as the light node's
/// scale. `None` for a mesh with no such node, or with nothing under it.
pub fn attached_light(buf: &[u8]) -> Option<([f32; 3], f32)> {
    let rec = read_records(buf)?;
    attached_light_of(&rec)
}

fn attached_light_of(rec: &Records) -> Option<([f32; 3], f32)> {
    let attach = rec.nodes.iter().flatten().find(|n| n.name.eq_ignore_ascii_case("AttachLight"))?;
    for &c in &attach.children {
        if c < 0 {
            continue;
        }
        if let (Some(Some(n)), Some(Some((diffuse, _dimmer)))) =
            (rec.nodes.get(c as usize), rec.lights.get(c as usize))
        {
            return Some((*diffuse, n.scale));
        }
    }
    None
}

/// Where the node called `AttachLight` sits, in the mesh's own space.
/// The node chain down to AttachLight, one line a node — a diagnostic for round 17q's
/// "is the flame inside the lamp" question, and the only way to see whether a surprising
/// answer is the file's doing or the walk's.
pub fn attach_chain(buf: &[u8]) -> Vec<String> {
    let Some(rec) = read_records(buf) else { return vec!["unreadable".into()] };
    let ident = [1., 0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 0.];
    let mut out = Vec::new();
    let mut stack: Vec<(i32, [f32; 12], f32, u32, String)> = vec![(rec.root, ident, 1.0, 0, String::new())];
    while let Some((idx, trafo, scale, depth, path)) = stack.pop() {
        if depth > 64 || idx < 0 {
            continue;
        }
        let Some(n) = rec.nodes.get(idx as usize).and_then(|n| n.as_ref()) else { continue };
        let (m, s) = node_trafo(rec.root, idx, &trafo, scale, n);
        let here = format!("{path}/{}", n.name);
        if n.name.eq_ignore_ascii_case("AttachLight") || depth < 3 {
            out.push(format!(
                "{:<40} local [{:.1},{:.1},{:.1}] scale {:.2}  ->  world [{:.1},{:.1},{:.1}]",
                here, n.trafo[9], n.trafo[10], n.trafo[11], n.scale, m[9], m[10], m[11]
            ));
        }
        for &c in &n.children {
            stack.push((c, m, s, depth + 1, here.clone()));
        }
    }
    out
}

/// Round 17y: every node of a file, indented, with its kind, flags, scale and the
/// controller chain hanging off it — the record types by index, since a node only
/// keeps the head. A debugging tool for meshes that animate: which node turns, which
/// scrolls, which is scaled.
pub fn node_tree(buf: &[u8]) -> Vec<String> {
    let Some(rec) = read_records(buf) else { return vec!["unreadable".into()] };
    let ident = [1., 0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 0.];
    let mut out = Vec::new();
    let mut stack: Vec<(i32, [f32; 12], f32, u32)> = vec![(rec.root, ident, 1.0, 0)];
    let mut guard = 0u32;
    while let Some((idx, trafo, scale, depth)) = stack.pop() {
        guard += 1;
        if guard > 100_000 || depth > 64 || idx < 0 {
            continue;
        }
        let Some(n) = rec.nodes.get(idx as usize).and_then(|n| n.as_ref()) else { continue };
        let (m, s) = node_trafo(rec.root, idx, &trafo, scale, n);
        // The controller chain: the head's type and, for the ones read, what is read.
        let mut ctrls = String::new();
        let mut ci = n.ctrl;
        for _ in 0..16 {
            if ci < 0 || ci as usize >= rec.types.len() {
                break;
            }
            let ty = rec.types[ci as usize].as_str();
            ctrls.push(' ');
            ctrls.push_str(ty);
            let next = if let Some(Some(c)) = rec.pctrls.get(ci as usize) {
                ctrls.push_str(&format!("(emitter {} size {} vel {})", c.emitter, c.size, c.velocity));
                -1
            } else if let Some(Some(u)) = rec.uvctrls.get(ci as usize) {
                ctrls.push_str(&format!("(freq {} data {})", u.3, u.2));
                u.0
            } else if let Some(Some(k)) = rec.kfctrls.get(ci as usize) {
                let kd = rec.kfdatas.get(k.data.max(0) as usize).cloned().flatten().unwrap_or_default();
                ctrls.push_str(&format!(
                    "(flags {:#06x} freq {} phase {} {}..{} data {} rot {} xyz {} trans {} scale {})",
                    k.flags,
                    k.frequency,
                    k.phase,
                    k.start,
                    k.stop,
                    k.data,
                    kd.rot.len(),
                    kd.rot_xyz.as_ref().map(|c| c[0].len() + c[1].len() + c[2].len()).unwrap_or(0),
                    kd.trans.len(),
                    kd.scales.len()
                ));
                k.next
            } else {
                -1
            };
            ci = next;
        }
        let props: Vec<String> =
            n.props.iter().filter_map(|&p| rec.types.get(p.max(0) as usize).cloned()).collect();
        /* Round 18ag: the node's own rotation, as the axis its local +z ends up along -
        enough to tell a bone that stands up from one that lies flat, which is the
        difference between a banner and a rug. Left out when it is the identity. */
        let up = [n.trafo[2], n.trafo[5], n.trafo[8]];
        let rot = if (up[0].abs() < 1e-4) && (up[1].abs() < 1e-4) && (up[2] - 1.0).abs() < 1e-4 {
            String::new()
        } else {
            format!(" z→[{:.2},{:.2},{:.2}]", up[0], up[1], up[2])
        };
        out.push(format!(
            "{}{} [{}] {:<24} flags {:#06x} scale {:.3} world {:.3} at [{:.1},{:.1},{:.1}]{}{}{}",
            "  ".repeat(depth as usize),
            idx,
            n.kind,
            n.name,
            n.flags,
            n.scale,
            s,
            m[9],
            m[10],
            m[11],
            rot,
            if ctrls.is_empty() { String::new() } else { format!("  ctrl:{ctrls}") },
            if props.is_empty() { String::new() } else { format!("  props: {}", props.join(",")) },
        ));
        for &c in n.children.iter().rev() {
            stack.push((c, m, s, depth + 1));
        }
    }
    out
}

fn attach_light(rec: &Records) -> Option<[f32; 3]> {
    let ident = [1., 0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 0.];
    let mut stack: Vec<(i32, [f32; 12], f32, u32)> = vec![(rec.root, ident, 1.0, 0)];
    let mut guard = 0u32;
    while let Some((idx, trafo, scale, depth)) = stack.pop() {
        guard += 1;
        if guard > 100_000 || depth > 64 || idx < 0 {
            continue;
        }
        let Some(n) = rec.nodes.get(idx as usize).and_then(|n| n.as_ref()) else { continue };
        let (m, s) = node_trafo(rec.root, idx, &trafo, scale, n);
        if n.name.eq_ignore_ascii_case("AttachLight") {
            return Some([m[9], m[10], m[11]]);
        }
        for &c in &n.children {
            stack.push((c, m, s, depth + 1));
        }
    }
    None
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
    l.starts_with("editormarker") || l.starts_with("tri editor marker") || l.starts_with("tri editormarker")
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

/// Wraithguard (0020): what a node's OpenMW shader extra data (`omw:data`, a NiStringExtraData
/// holding YAML/JSON) asks of the renderer, as far as a still preview cares.
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
struct OmwShader {
    /// `shader: distortion:` - not a surface at all: OpenMW draws nothing of the shape's own
    /// colour and uses its texture (a normal map, `omw_distortion\water\NN.dds`) to bend the
    /// scene behind it. Drawn as a texture it is the rainbow of a normal map.
    distortion: bool,
}

fn omw_shader(extras: &[Option<ExtraString>], head: i32) -> OmwShader {
    let mut out = OmwShader::default();
    let mut i = head;
    for _ in 0..64 {
        if i < 0 || i as usize >= extras.len() {
            break;
        }
        let Some(e) = extras[i as usize].as_ref() else { break };
        if e.value.trim_start().to_ascii_lowercase().starts_with("omw:data") {
            // Quotes, spaces and line breaks do not matter to the reader (it is YAML), so
            // look at the text without them.
            let flat: String = e.value.chars().filter(|c| !matches!(c, '"' | '\'' | ' ' | '\t' | '\r' | '\n')).collect::<String>().to_ascii_lowercase();
            if flat.contains("shader:") && flat.contains("distortion:") {
                out.distortion = true;
            }
        }
        i = e.next;
    }
    out
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
    // Round 18gh: the block the file's first root names. Every walk below starts here,
    // and this is the one node whose transform is never applied.
    root: i32,
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
        if idx != root && idx >= 0 {
            if let Some(Some((lm, ls))) = poses.get(idx as usize) {
                return mul(trafo, scale, lm, *ls);
            }
        }
        node_trafo(root, idx, trafo, scale, n)
    };

    // The engine reads its markers off the root, so that is where they are looked for.
    let root_node = nodes.get(root.max(0) as usize).and_then(|n| n.as_ref());
    let marks = root_node.map(|n| root_markers(extras, n.extra)).unwrap_or_default();

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
    if let Some(root_node) = root_node {
        if marks.rcn {
            // Depth-first, carrying the transform down so a nested hull lands in the
            // right place rather than at the mesh origin.
            let mut stack = vec![(root, ident, 1.0f32, 0u32)];
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
            for &c in &root_node.children {
                if kind_of(c) == "RootCollisionNode" {
                    if nodes.get(c as usize).and_then(|n| n.as_ref()).is_some() {
                        col_root = c;
                        // Round 18be (G1): the root's transform is never applied
                        // (`node_trafo` returns the parent's for the root), so the hull's
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
        if col_root >= 0 { (col_root, col_trafo, col_scale) } else { (root, ident, 1.0) };

    // One walk, two questions. Collision asks what the engine is solid against;
    // visible asks what it draws, and is only needed as a last resort — to measure
    // something that has no collision at all.
    let gather =
        |start: i32, start_trafo: [f32; 12], start_scale: f32, skip_culled: bool, collision: bool| {
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
                    // A switch node has one live branch, a level-of-detail node the levels
                    // at distance 0. The same rule the drawing walk uses, for the same
                    // reason — the alternative is counting the same shape several times.
                    for c in live_children(n) {
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
    let mut shape = if col_root >= 0 { ColShape::Hull } else { ColShape::Visible };
    if marks.nc || tris.is_empty() {
        let vis = if tris.is_empty() { gather(root, ident, 1.0, true, false) } else { tris };
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
    let radius = (0..3).map(|k| (aabb_max[k] - aabb_min[k]) * 0.5).fold(0.0f32, |a, v| a + v * v).sqrt();
    let accel = crate::triaccel::TriAccel::build(&tris);
    Some(MeshGeom { tris: std::sync::Arc::new(tris), aabb_min, aabb_max, shape, radius, accel })
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
    let quads = [[0, 3, 2, 1], [4, 5, 6, 7], [0, 1, 5, 4], [1, 2, 6, 5], [2, 3, 7, 6], [3, 0, 4, 7]];
    let mut tris = Vec::with_capacity(36);
    for q in quads {
        for k in [q[0], q[1], q[2], q[0], q[2], q[3]] {
            tris.push(p[k]);
        }
    }
    tris
}

/// Parts hung on another file's skeleton (an NPC's body parts and what it wears).
pub mod skeleton;
