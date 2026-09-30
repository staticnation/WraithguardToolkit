//! Meshes in the shape the viewport draws them: the drawable parts, the collision
//! geometry, attach points and particle systems of a mesh, packed as the bytes the page
//! reads (`mesh_bytes*`), and its animation as JSON.


/* ---- mesh delivery ------------------------------------------------------------
   The viewport draws; the engine reads. Shipping parsed geometry rather than the
   raw NIF is not extra traffic — it is less, because everything the renderer does
   not use is dropped on the way (2.7 MB of buffers against a 3.6 MB file, for the
   largest Bitter Coast tree).

   Little-endian throughout, and laid out so the page can take typed-array views
   straight onto the buffer instead of copying:

     "GDN2"                       magic
     u8   collision shape         0 hull, 1 visible, 2 bounds, 3 none
     f32  collision radius        already scaled by nothing; the caller applies scale
     f32 x6  collision bounds     min xyz, max xyz
     u32  part count
     per part:
       u16 name length, name bytes
       u16 texture length, texture bytes
       u16 alpha flags
       i16 alpha threshold        -1 when alpha testing is off
       f32 x3 diffuse
       f32 material alpha
       u32 vertex count, u32 triangle count
       f32 x3n positions, f32 x3n normals, f32 x2n uvs, u16 x3t indices
       u8 has vertex colours, then u8 x3n RGB when it is 1     (GDN2, round 11)
*/

fn put_u16(o: &mut Vec<u8>, v: u16) {
    o.extend_from_slice(&v.to_le_bytes());
}
fn put_u32(o: &mut Vec<u8>, v: u32) {
    o.extend_from_slice(&v.to_le_bytes());
}
fn put_f32(o: &mut Vec<u8>, v: f32) {
    o.extend_from_slice(&v.to_le_bytes());
}
fn put_str(o: &mut Vec<u8>, s: &str) {
    let b = s.as_bytes();
    let n = b.len().min(u16::MAX as usize);
    put_u16(o, n as u16);
    o.extend_from_slice(&b[..n]);
}

/// `geom` is the collision answer for the same mesh, if it had one. It travels with
/// the geometry because the page reports it — the right-click verdict names the
/// shape, and the statistics count them — but it never needs the triangles, which
/// are the expensive half and which only the engine tests against.
pub fn mesh_bytes(parts: &[crate::nif::DrawPart], geom: Option<&crate::nif::MeshGeom>) -> Vec<u8> {
    mesh_bytes_lit(parts, geom, None)
}

/// The same payload, with the mesh's `AttachLight` offset when it has one (round 17m).
pub fn mesh_bytes_lit(
    parts: &[crate::nif::DrawPart],
    geom: Option<&crate::nif::MeshGeom>,
    attach: Option<[f32; 3]>,
) -> Vec<u8> {
    mesh_bytes_full(parts, geom, attach, "")
}

/// The payload with everything (round 17w): the parts, the light's hanging point, and
/// the mesh's particle systems as the JSON `nif::particles_json` writes — an empty
/// string when it has none, which is nearly every mesh.
pub fn mesh_bytes_full(
    parts: &[crate::nif::DrawPart],
    geom: Option<&crate::nif::MeshGeom>,
    attach: Option<[f32; 3]>,
    particles: &str,
) -> Vec<u8> {
    mesh_bytes_all(parts, geom, attach, particles, None)
}

/// Round 17y: and the light hanging under AttachLight, when there is one — its colour and
/// radius, after everything else so no earlier offset moves.
pub fn mesh_bytes_all(
    parts: &[crate::nif::DrawPart],
    geom: Option<&crate::nif::MeshGeom>,
    attach: Option<[f32; 3]>,
    particles: &str,
    light: Option<([f32; 3], f32)>,
) -> Vec<u8> {
    use crate::nif::ColShape;
    let mut o = Vec::with_capacity(4096);
    o.extend_from_slice(b"GDN2");
    let (shape, radius, mn, mx) = match geom {
        Some(g) => (
            match g.shape {
                ColShape::Hull => 0u8,
                ColShape::Visible => 1,
                ColShape::Bounds => 2,
            },
            g.radius,
            g.aabb_min,
            g.aabb_max,
        ),
        None => (3u8, 0.0, [0.0; 3], [0.0; 3]),
    };
    o.push(shape);
    put_f32(&mut o, radius);
    for v in mn.iter().chain(mx.iter()) {
        put_f32(&mut o, *v);
    }
    /* Round 17m: where a light hangs on this mesh. A flag and three floats, before the
       parts, because it is a fact about the mesh rather than about any one shape. */
    match attach {
        Some(a) => {
            o.push(1);
            for v in a {
                put_f32(&mut o, v);
            }
        }
        None => o.push(0),
    }
    put_u32(&mut o, parts.len() as u32);
    for p in parts {
        put_str(&mut o, &p.name);
        put_str(&mut o, &p.tex);
        put_u16(&mut o, p.alpha_flags);
        let t: i16 = p.alpha_threshold.map(|v| v as i16).unwrap_or(-1);
        o.extend_from_slice(&t.to_le_bytes());
        for v in p.diffuse {
            put_f32(&mut o, v);
        }
        put_f32(&mut o, p.mat_alpha);
        let nv = (p.pos.len() / 3) as u32;
        let nt = (p.idx.len() / 3) as u32;
        put_u32(&mut o, nv);
        put_u32(&mut o, nt);
        for v in p.pos.iter().chain(p.nrm.iter()).chain(p.uv.iter()) {
            put_f32(&mut o, *v);
        }
        for v in &p.idx {
            put_u16(&mut o, *v);
        }
        let coloured = p.col.len() == nv as usize * 3;
        o.push(coloured as u8);
        if coloured {
            o.extend_from_slice(&p.col);
        }
        /* Round 17h: the detail map and the UVs it reads, when the shape has one. A
           name and a flag; the page loads the texture like any other. */
        let detailed = !p.detail_tex.is_empty() && p.uv2.len() == nv as usize * 2;
        o.push(detailed as u8);
        if detailed {
            put_str(&mut o, &p.detail_tex);
            for v in &p.uv2 {
                put_f32(&mut o, *v);
            }
        }
        /* Round 17p: the glow map — a flag, then the name and the UV set it asked for.
           The page loads the texture like any other and adds it to the final colour. */
        o.push(!p.glow_tex.is_empty() as u8);
        if !p.glow_tex.is_empty() {
            put_str(&mut o, &p.glow_tex);
            o.push(p.glow_uv_set.min(255) as u8);
        }
        /* Round 17m: the emissive colour, always — three floats is cheaper than a flag
           and a branch, and it is black on nearly everything. */
        for v in p.emissive {
            put_f32(&mut o, v);
        }
        /* And the UV animation, when the shape has one: the controller's clock, then
           the four key curves as (count, then count × time+value). The page runs it. */
        match &p.uv_anim {
            Some(a) => {
                o.push(1);
                put_uv_anim(&mut o, a);
            }
            None => o.push(0),
        }
        /* Round 17w: how the shape is drawn. Bit 0: unlit (NiVertexColorProperty lighting
           mode 0 — texture times vertex colour, no sun); bits 1-2: the vertex colour mode
           (0 ignored, 1 emissive, 2 multiplied); bits 3-4: the NiStencilProperty draw
           mode (0 default, 1 CCW, 2 CW, 3 both sides); bits 5-6 (17x): the day/night
           switch branch. One byte at the end of the part, so every offset before it stands. */
        // Bits 5-6 (round 17x): the branch of a NightDaySwitch — 0 none, 1 off, 2 on, 3 int-day.
        // Bit 7 (round 18dg): the shape does not write depth (NiZBufferProperty, bit 1 clear).
        let flags: u8 = (p.unlit as u8) | ((p.vcol_mode & 3) << 1) | ((p.draw_mode & 3) << 3) | ((p.day_night & 3) << 5) | ((p.no_zwrite as u8) << 7);
        o.push(flags);
    }
    /* Round 17w: the particle systems, as JSON, after the parts. The reader was kept when
       the weather went (17h) for exactly this; the page runs them. A length-prefixed
       string like every other, and two bytes of nothing on a mesh without any. */
    let pj = if particles == "[]" { "" } else { particles };
    put_u32(&mut o, pj.len() as u32);   // four bytes of length: a file of systems is long
    o.extend_from_slice(pj.as_bytes());
    /* Round 17y: what moves, as JSON after the particles — the animated nodes above a
       shape and the fade on its material, by part index. Nothing on nearly every mesh:
       four bytes of zero. After the particles rather than inside the parts so every
       offset before it, and every reader of the old tail, stands. */
    let aj = anims_json(parts);
    put_u32(&mut o, aj.len() as u32);
    o.extend_from_slice(aj.as_bytes());
    /* Round 17y: the AttachLight's own light — a flag, three floats of colour and the
       radius. Glow in the Dahrk's windows carry one for the light they throw into a room. */
    match light {
        Some((c, r)) => {
            o.push(1);
            for v in c {
                put_f32(&mut o, v);
            }
            put_f32(&mut o, r);
        }
        None => o.push(0),
    }
    /* Round 17y: the dark maps, by part - a count, then per entry the part's index, the
       texture's name and its UVs (two floats a vertex). Nearly always a count of nought;
       the lit branch of a Glow in the Dahrk window is what has one. */
    let dark: Vec<(usize, &crate::nif::DrawPart)> = parts
        .iter()
        .enumerate()
        .filter(|(_, p)| !p.dark_tex.is_empty() && p.uv_dark.len() == (p.pos.len() / 3) * 2)
        .collect();
    put_u32(&mut o, dark.len() as u32);
    for (i, p) in dark {
        put_u32(&mut o, i as u32);
        put_str(&mut o, &p.dark_tex);
        for v in &p.uv_dark {
            put_f32(&mut o, *v);
        }
    }
    /* Round 18ag: the skinned shapes under an animated skeleton, by part - a count,
       then per entry the part's index, the bones as JSON (each bone's chain of moving
       nodes in the form `anims_json` writes a part's, its still tail and its skin-to-bone
       transform), and the vertices the page skins from: skin-space positions and
       normals, four bone slots a vertex and four weights. A count of nought on every
       mesh that has no `.kf` beside it, which is nearly all of them; the silt strider's
       feelers are what this exists for. */
    let skins: Vec<(usize, &crate::nif::SkinAnim)> = parts
        .iter()
        .enumerate()
        .filter_map(|(i, p)| p.skin.as_ref().map(|s| (i, s)))
        .filter(|(_, s)| s.pos.len() % 3 == 0 && s.idx.len() == (s.pos.len() / 3) * 4 && s.w.len() == s.idx.len())
        .collect();
    put_u32(&mut o, skins.len() as u32);
    for (i, sk) in skins {
        put_u32(&mut o, i as u32);
        let bj = bones_json(&sk.bones);
        put_u32(&mut o, bj.len() as u32);
        o.extend_from_slice(bj.as_bytes());
        let nv = (sk.pos.len() / 3) as u32;
        put_u32(&mut o, nv);
        for v in &sk.pos {
            put_f32(&mut o, *v);
        }
        let has_n = sk.nrm.len() == sk.pos.len();
        o.push(has_n as u8);
        if has_n {
            for v in &sk.nrm {
                put_f32(&mut o, *v);
            }
        }
        o.extend_from_slice(&sk.idx);
        for v in &sk.w {
            put_f32(&mut o, *v);
        }
    }
    /* Round 18au: the morph-animated shapes - the moths' wingbeat - by part: a count, then
       per entry the part's index, the controller's flags and clock, the target count and
       the vertex count, and per target its weight keys (count, then time+weight pairs)
       and its offsets (three floats a vertex, already in the part's space). The page adds
       the weighted offsets to the positions each frame. Nought on nearly every mesh. */
    let morphs: Vec<(usize, &crate::nif::MorphAnim)> = parts
        .iter()
        .enumerate()
        .filter_map(|(i, p)| p.morph.as_ref().map(|m| (i, m)))
        .filter(|(i, m)| !m.targets.is_empty() && m.targets.iter().all(|t| t.offsets.len() == parts[*i].pos.len()))
        .collect();
    put_u32(&mut o, morphs.len() as u32);
    for (i, m) in morphs {
        put_u32(&mut o, i as u32);
        o.extend_from_slice(&m.flags.to_le_bytes());
        put_f32(&mut o, m.frequency);
        put_f32(&mut o, m.phase);
        put_f32(&mut o, m.start);
        put_f32(&mut o, m.stop);
        put_u32(&mut o, m.targets.len() as u32);
        put_u32(&mut o, (parts[i].pos.len() / 3) as u32);
        for t in &m.targets {
            put_u32(&mut o, t.keys.len() as u32);
            for (time, w) in &t.keys {
                put_f32(&mut o, *time);
                put_f32(&mut o, *w);
            }
            for v in &t.offsets {
                put_f32(&mut o, *v);
            }
        }
    }
    /* Round 18de: the material ambient, by part - a count, then per entry the part's
       index and three floats. Only the parts whose ambient differs from their diffuse
       are listed, which is about one static shape in twelve; the page takes the diffuse
       for the rest, as the shader always did. After the morphs so every reader of the
       older tail stands. */
    let ambients: Vec<(usize, &crate::nif::DrawPart)> = parts.iter().enumerate().filter(|(_, p)| p.ambient != p.diffuse).collect();
    put_u32(&mut o, ambients.len() as u32);
    for (i, p) in ambients {
        put_u32(&mut o, i as u32);
        for v in p.ambient {
            put_f32(&mut o, v);
        }
    }
    /* Round 18df: the vertex alpha, by part - a count, then per entry the part's index,
       its vertex count and that many bytes. Only the parts with a vertex below 255 are
       listed (no vanilla shape; OAAB's root pillars and inflow sheets), so the payload of
       everything else is what it was. After the ambients. */
    let alphas: Vec<(usize, &crate::nif::DrawPart)> = parts.iter().enumerate().filter(|(_, p)| !p.col_alpha.is_empty()).collect();
    put_u32(&mut o, alphas.len() as u32);
    for (i, p) in alphas {
        put_u32(&mut o, i as u32);
        put_u32(&mut o, p.col_alpha.len() as u32);
        o.extend_from_slice(&p.col_alpha);
    }
    /* Round 18df: the decal maps, by part, laid out exactly as the dark maps are - a
       count, then per entry the part's index, the texture's name and its UVs. Nothing in
       vanilla; OAAB's and Tamriel Rebuilt's signs and posters. */
    let decal: Vec<(usize, &crate::nif::DrawPart)> = parts
        .iter()
        .enumerate()
        .filter(|(_, p)| !p.decal_tex.is_empty() && p.uv_decal.len() == (p.pos.len() / 3) * 2)
        .collect();
    put_u32(&mut o, decal.len() as u32);
    for (i, p) in decal {
        put_u32(&mut o, i as u32);
        put_str(&mut o, &p.decal_tex);
        for v in &p.uv_decal {
            put_f32(&mut o, *v);
        }
    }
    /* Round 18df: the clamped base maps, by part - a count, then per entry the part's
       index and one byte: bit 0 clamps S, bit 1 clamps T. Only the parts that clamp at
       all are listed (447 vanilla static shapes); the rest wrap, as every texture did. */
    let clamped: Vec<(usize, &crate::nif::DrawPart)> = parts.iter().enumerate().filter(|(_, p)| p.clamp_st != 0).collect();
    put_u32(&mut o, clamped.len() as u32);
    for (i, p) in clamped {
        put_u32(&mut o, i as u32);
        o.push(p.clamp_st & 3);
    }
    /* Round 18dj: the UV animations of the other maps, by part - a count, then per entry
       the part's index, one byte of the maps the controller moves (1 the dark map, 2 the
       detail, 4 the glow, 8 the decal) and the controller's clock and curves laid out as
       the base map's are. A shape lists one entry per controller, so two maps on one UV
       set share an entry. Nothing on any vanilla shape; the Ghostgate dome's dark map,
       OAAB's lava decal and Tamriel Rebuilt's mushroom glow. After the clamps. */
    let mut others: Vec<(usize, u8, &crate::nif::UvAnim)> = Vec::new();
    for (i, p) in parts.iter().enumerate() {
        for (bits, a) in &p.uv_anims_other {
            others.push((i, *bits, a));
        }
    }
    put_u32(&mut o, others.len() as u32);
    for (i, bits, a) in others {
        put_u32(&mut o, i as u32);
        o.push(bits);
        put_uv_anim(&mut o, a);
    }
    /* Round 18dl: the environment maps, by part - a count, then per entry the part's
       index, the texture's name, one byte of clamp mode (0 clamp both, 1 clamp S, 2
       clamp T, 3 wrap), and one byte saying whether a bump map follows: its name, the
       luma scale and offset, and the four floats of its matrix. Nineteen vanilla statics
       (the Telvanni crystals and portals) and the fake bump-mapping mods' every shape.
       After the other maps' animations. */
    let envs: Vec<(usize, &crate::nif::DrawPart)> = parts.iter().enumerate().filter(|(_, p)| !p.env_tex.is_empty()).collect();
    put_u32(&mut o, envs.len() as u32);
    for (i, p) in envs {
        put_u32(&mut o, i as u32);
        put_str(&mut o, &p.env_tex);
        o.push((p.env_clamp & 3) as u8);
        if p.bump_tex.is_empty() {
            o.push(0);
        } else {
            o.push(1);
            put_str(&mut o, &p.bump_tex);
            put_f32(&mut o, p.bump_luma[0]);
            put_f32(&mut o, p.bump_luma[1]);
            for v in p.bump_mat {
                put_f32(&mut o, v);
            }
        }
    }
    /* Wraithguard: the gloss maps (slot 3), by part - a count, then per entry the part's
       index and the texture's name. After the environment maps. */
    let gloss: Vec<(usize, &crate::nif::DrawPart)> = parts.iter().enumerate().filter(|(_, p)| !p.gloss_tex.is_empty()).collect();
    put_u32(&mut o, gloss.len() as u32);
    for (i, p) in gloss {
        put_u32(&mut o, i as u32);
        put_str(&mut o, &p.gloss_tex);
    }
    o
}

/// One UV animation as the payload carries it (round 17m): the controller's clock, then
/// the four key curves as (count, then count × time+value).
fn put_uv_anim(o: &mut Vec<u8>, a: &crate::nif::UvAnim) {
    put_f32(o, a.frequency);
    put_f32(o, a.phase);
    put_f32(o, a.start);
    put_f32(o, a.stop);
    for keys in [&a.u_trans, &a.v_trans, &a.u_scale, &a.v_scale] {
        put_u32(o, keys.len() as u32);
        for (t, v) in keys {
            put_f32(o, *t);
            put_f32(o, *v);
        }
    }
}

/// Round 18ag: one animated node's link as JSON - what `anims_json` writes for each of a
/// part's nodes, shared with the bones of a skinned shape.
fn link_json(l: &crate::nif::AnimLink) -> String {
    use crate::toml::num;
    let m12 = |m: &[f32; 12]| {
        let v: Vec<String> = m.iter().map(|x| num(*x)).collect();
        format!("[{}]", v.join(","))
    };
    let keys2 = |k: &[(f32, f32)]| {
        let v: Vec<String> = k.iter().map(|(t, x)| format!("[{},{}]", num(*t), num(*x))).collect();
        format!("[{}]", v.join(","))
    };
    let rot: Vec<String> = l
        .data
        .rot
        .iter()
        .map(|(t, q)| format!("[{},{},{},{},{}]", num(*t), num(q[0]), num(q[1]), num(q[2]), num(q[3])))
        .collect();
    let trans: Vec<String> = l
        .data
        .trans
        .iter()
        .map(|(t, v)| format!("[{},{},{},{}]", num(*t), num(v[0]), num(v[1]), num(v[2])))
        .collect();
    let xyz = match &l.data.rot_xyz {
        Some(ch) => format!("[{},{},{}]", keys2(&ch[0]), keys2(&ch[1]), keys2(&ch[2])),
        None => "null".to_string(),
    };
    format!(
        "{{\"pre\":{},\"preS\":{},\"local\":{},\"localS\":{},\"flags\":{},\"freq\":{},\"phase\":{},\"start\":{},\"stop\":{},\"rot\":[{}],\"xyz\":{},\"trans\":[{}],\"scale\":{}}}",
        m12(&l.pre.0),
        num(l.pre.1),
        m12(&l.local.0),
        num(l.local.1),
        l.ctrl.flags,
        num(l.ctrl.frequency),
        num(l.ctrl.phase),
        num(l.ctrl.start),
        num(l.ctrl.stop),
        rot.join(","),
        xyz,
        trans.join(","),
        keys2(&l.data.scales)
    )
}

/// Round 18ag: a skinned shape's bones as JSON - per bone its chain of links, the still
/// transform after the last of them, and the skin-to-bone transform.
pub fn bones_json(bones: &[crate::nif::BoneAnim]) -> String {
    use crate::toml::num;
    let m12 = |m: &[f32; 12]| {
        let v: Vec<String> = m.iter().map(|x| num(*x)).collect();
        format!("[{}]", v.join(","))
    };
    let mut out = String::from("[");
    for (i, b) in bones.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let links: Vec<String> = b.chain.iter().map(link_json).collect();
        out.push_str(&format!(
            "{{\"chain\":[{}],\"post\":{},\"postS\":{},\"trafo\":{},\"trafoS\":{}}}",
            links.join(","),
            m12(&b.post.0),
            num(b.post.1),
            m12(&b.trafo.0),
            num(b.trafo.1)
        ));
    }
    out.push(']');
    out
}

/// The parts' animations (round 17y) as one JSON list — `[]` when nothing moves, which
/// the payload writes as no bytes at all.
pub fn anims_json(parts: &[crate::nif::DrawPart]) -> String {
    use crate::toml::num;
    let keys2 = |k: &[(f32, f32)]| {
        let v: Vec<String> = k.iter().map(|(t, x)| format!("[{},{}]", num(*t), num(*x))).collect();
        format!("[{}]", v.join(","))
    };
    let mut out = String::from("[");
    let mut any = false;
    for (i, p) in parts.iter().enumerate() {
        let Some(a) = &p.anim else { continue };
        if any {
            out.push(',');
        }
        any = true;
        out.push_str(&format!("{{\"part\":{i},\"nodes\":["));
        for (k, l) in a.nodes.iter().enumerate() {
            if k > 0 {
                out.push(',');
            }
            out.push_str(&link_json(l));
        }
        out.push_str("],\"alpha\":");
        match &a.alpha {
            Some(al) => out.push_str(&format!(
                "{{\"flags\":{},\"freq\":{},\"phase\":{},\"start\":{},\"stop\":{},\"keys\":{}}}",
                al.flags,
                num(al.frequency),
                num(al.phase),
                num(al.start),
                num(al.stop),
                keys2(&al.keys)
            )),
            None => out.push_str("null"),
        }
        out.push_str(",\"vis\":[");
        for (k, v) in a.vis.iter().enumerate() {
            if k > 0 {
                out.push(',');
            }
            out.push_str(&format!(
                "{{\"flags\":{},\"freq\":{},\"phase\":{},\"start\":{},\"stop\":{},\"keys\":{}}}",
                v.flags,
                num(v.frequency),
                num(v.phase),
                num(v.start),
                num(v.stop),
                keys2(&v.keys)
            ));
        }
        out.push(']');
        out.push('}');
    }
    out.push(']');
    if any { out } else { String::new() }
}
