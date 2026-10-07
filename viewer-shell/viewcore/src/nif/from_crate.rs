//! The mesh's records read by greatness7's `tes3::nif`, laid out as the engine's
//! `Records` tables.
//!
//! The crate parses the file and validates it. This module only copies what the viewer
//! uses into the per-record tables that the draw, skin, particle and animation walks
//! read. It keeps the same numbers the engine's own reader keeps: colours are clamped
//! the same way, only a curve's time and value are kept, and strips become triangles
//! with the same winding. A file the crate refuses returns `None`, and the caller then
//! uses the engine's own reader (`own_records`), which tolerates damaged files.

use std::collections::HashMap;

use tes3::nif::glam::{Mat3, Quat, Vec3};
use tes3::nif::*;

use super::{
    ExtraString, KfCtrl, KfData, MorphCtrl, MorphData, Node, ParticleCtrl, ParticleData,
    ParticleMod, Records, ShapeData, SkinBone, SkinData, SkinInst, UvAnim,
};

/// `buf` as the crate reads it: Morrowind's handful of 4.0.0.0 files carry the same
/// layout under an older header, so the header is rewritten to 4.0.0.2 first.
pub(super) fn as_4002(bytes: &[u8]) -> std::borrow::Cow<'_, [u8]> {
    const V4000: &[u8; 40] = b"NetImmerse File Format, Version 4.0.0.0\n";
    let text_ok = bytes.len() >= 44 && (&bytes[..40] == V4000 || bytes[..40] == NiStream::HEADER);
    if text_ok && bytes[40..44] == 0x0400_0000u32.to_le_bytes() {
        let mut b = bytes.to_vec();
        b[..40].copy_from_slice(&NiStream::HEADER);
        b[40..44].copy_from_slice(&NiStream::VERSION.to_le_bytes());
        return std::borrow::Cow::Owned(b);
    }
    std::borrow::Cow::Borrowed(bytes)
}

/// Parses `buf` with the crate. `None` when the crate refuses the file or panics on it
/// (it unwraps on some malformed data).
pub(super) fn crate_records(buf: &[u8]) -> Option<Records> {
    if buf.len() < 48 || !buf.starts_with(b"NetImmerse File Format") {
        return None;
    }
    let bytes = as_4002(buf);
    let stream = std::panic::catch_unwind(|| NiStream::from_bytes(&bytes).ok()).ok().flatten()?;
    let n = stream.objects.len();
    if n == 0 {
        return None;
    }
    let index: HashMap<NiKey, i32> = stream.objects.keys().enumerate().map(|(i, k)| (k, i as i32)).collect();
    let ix = |key: NiKey| -> i32 { index.get(&key).copied().unwrap_or(-1) };
    let mut rec = Records::empty(n);
    for (i, o) in stream.objects.values().enumerate() {
        rec.types[i] = String::from_utf8_lossy(o.type_name()).into_owned();
        fill(&mut rec, i, o, &ix);
    }
    Some(rec)
}

/// Clamps a colour to 0..1, as the fixed pipeline does (see `skip_or_read`).
fn c3(v: Vec3) -> [f32; 3] {
    [v.x.clamp(0.0, 1.0), v.y.clamp(0.0, 1.0), v.z.clamp(0.0, 1.0)]
}

fn v3(v: Vec3) -> [f32; 3] {
    [v.x, v.y, v.z]
}

/// A rotation and translation as the engine packs them: the 3x3 in file order, then
/// the translation. The crate loads the file's nine floats into glam's column storage,
/// so the column array is the file order.
fn trafo(rotation: Mat3, translation: Vec3) -> [f32; 12] {
    let mut m = [0f32; 12];
    m[..9].copy_from_slice(&rotation.to_cols_array());
    m[9..].copy_from_slice(&v3(translation));
    m
}

/// A float curve's (time, value) pairs; tangents and TCB parameters are dropped.
fn float_keys(k: &NiFloatKey) -> Vec<(f32, f32)> {
    match k {
        NiFloatKey::LinKey(v) => v.iter().map(|k| (k.time, k.value)).collect(),
        NiFloatKey::BezKey(v) => v.iter().map(|k| (k.time, k.value)).collect(),
        NiFloatKey::TCBKey(v) => v.iter().map(|k| (k.time, k.value)).collect(),
    }
}

fn node_of(kind: &str, av: &NiAVObject, ix: &impl Fn(NiKey) -> i32) -> Node {
    Node {
        kind: kind.to_string(),
        name: av.name.clone(),
        flags: av.flags,
        trafo: trafo(av.rotation, av.translation),
        scale: av.scale,
        children: Vec::new(),
        extra: ix(av.extra_data.key),
        data_ref: -1,
        active_index: 0,
        props: av.properties.iter().map(|l| ix(l.key)).collect(),
        skin_ref: -1,
        skinned: false,
        ctrl: ix(av.controller.key),
        effects: Vec::new(),
    }
}

fn geom_data(g: &NiGeometryData) -> ShapeData {
    let nverts = g.vertices.len();
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    let colours: Vec<[u8; 3]> = g.vertex_colors.iter().map(|c| [q(c.x), q(c.y), q(c.z)]).collect();
    let mut alphas: Vec<u8> = g.vertex_colors.iter().map(|c| q(c.w)).collect();
    if alphas.iter().all(|&a| a == 255) {
        alphas.clear();
    }
    let mut sets = if nverts > 0 { g.uv_sets.chunks(nverts) } else { [].chunks(1) };
    let uvs: Vec<[f32; 2]> = sets.next().map(|s| s.iter().map(|v| [v.x, v.y]).collect()).unwrap_or_default();
    let uv_more = sets.take(7).map(|s| s.iter().map(|v| [v.x, v.y]).collect()).collect();
    ShapeData {
        verts: g.vertices.iter().map(|v| v3(*v)).collect(),
        tris: Vec::new(),
        normals: g.normals.iter().map(|v| v3(*v)).collect(),
        uvs,
        uv_more,
        colours,
        alphas,
    }
}

/// Strips to triangles, as `read_strips_data`: odd triangles swapped back, degenerate
/// ones (how strips are joined) dropped.
fn strip_tris(d: &NiTriStripsData) -> Vec<[u16; 3]> {
    let mut out = Vec::new();
    let mut at = 0usize;
    for &len in &d.strip_lengths {
        let len = len as usize;
        let Some(strip) = d.strips.get(at..at + len) else { break };
        at += len;
        for i in 0..strip.len().saturating_sub(2) {
            let (a, b, c) = (strip[i], strip[i + 1], strip[i + 2]);
            if a == b || b == c || a == c {
                continue;
            }
            out.push(if i % 2 == 0 { [a, b, c] } else { [b, a, c] });
        }
    }
    out
}

fn keyframes(d: &NiKeyframeData) -> KfData {
    let mut k = KfData::default();
    let quat = |q: Quat| [q.w, q.x, q.y, q.z];
    match &d.rotations.keys {
        NiRotKey::LinKey(v) => k.rot = v.iter().map(|r| (r.time, quat(r.value))).collect(),
        NiRotKey::BezKey(v) => k.rot = v.iter().map(|r| (r.time, quat(r.value))).collect(),
        NiRotKey::TCBKey(v) => k.rot = v.iter().map(|r| (r.time, quat(r.value))).collect(),
        NiRotKey::EulerKey(e) => {
            k.rot_xyz = Some([float_keys(&e.axes[0].keys), float_keys(&e.axes[1].keys), float_keys(&e.axes[2].keys)]);
        }
    }
    k.trans = match &d.translations.keys {
        NiPosKey::LinKey(v) => v.iter().map(|p| (p.time, v3(p.value))).collect(),
        NiPosKey::BezKey(v) => v.iter().map(|p| (p.time, v3(p.value))).collect(),
        NiPosKey::TCBKey(v) => v.iter().map(|p| (p.time, v3(p.value))).collect(),
    };
    k.scales = float_keys(&d.scales.keys);
    k
}

/// The controller's active-particle count. The file has it straight after the particle
/// count, before the particle array (nif.xml, and OpenMW's `NiParticleSystemController`);
/// the crate reads it after the array, so with any particles saved the crate's array
/// starts two bytes early and the count it reports is the last particle's index. Taken
/// from where it is: the first two bytes the crate read as the first particle's velocity.
fn active_count(c: &NiParticleSystemController) -> u16 {
    match c.particles.first() {
        Some(p) => p.velocity.x.to_bits() as u16,
        None => c.num_active_particles,
    }
}

/// The clock fields every controller shares: next, flags, frequency, phase, start, stop,
/// target.
fn clock(c: &NiTimeController, ix: &impl Fn(NiKey) -> i32) -> (i32, u16, f32, f32, f32, f32, i32) {
    (ix(c.next.key), c.flags, c.frequency, c.phase, c.start_time, c.stop_time, ix(c.target.key))
}

fn fill(rec: &mut Records, i: usize, o: &NiType, ix: &impl Fn(NiKey) -> i32) {
    let kind = rec.types[i].clone();
    match o {
        /* Nodes with children. The same list as the engine's reader, and any other
           node type the crate knows (a BSP or mirrored node) reads as a plain node. */
        NiType::NiNode(_)
        | NiType::RootCollisionNode(_)
        | NiType::AvoidNode(_)
        | NiType::NiBSParticleNode(_)
        | NiType::NiBSAnimationNode(_)
        | NiType::NiSwitchNode(_)
        | NiType::NiBillboardNode(_)
        | NiType::NiCollisionSwitch(_)
        | NiType::NiSortAdjustNode(_)
        | NiType::NiLODNode(_)
        | NiType::NiBSPNode(_)
        | NiType::NiFltAnimationNode(_)
        | NiType::BSMirroredNode(_) => {
            let Ok(node) = <&NiNode>::try_from(o) else { return };
            let mut n = node_of(&kind, node, ix);
            n.children = node.children.iter().map(|l| ix(l.key)).collect();
            n.effects = node.effects.iter().map(|l| ix(l.key)).collect();
            // A LOD node's level 0 is the nearest, the one a preview wants: kept at 0.
            if let NiType::NiSwitchNode(s) = o {
                n.active_index = s.active_index;
            }
            rec.nodes[i] = Some(n);
        }
        NiType::NiTriShape(_)
        | NiType::NiTriStrips(_)
        | NiType::NiAutoNormalParticles(_)
        | NiType::NiRotatingParticles(_)
        | NiType::NiParticles(_) => {
            let Ok(g) = <&NiGeometry>::try_from(o) else { return };
            let mut n = node_of(&kind, g, ix);
            n.data_ref = ix(g.geometry_data.key);
            n.skin_ref = ix(g.skin_instance.key);
            rec.nodes[i] = Some(n);
        }
        NiType::NiPointLight(_) | NiType::NiSpotLight(_) | NiType::NiDirectionalLight(_) | NiType::NiAmbientLight(_) => {
            let Ok(l) = <&NiLight>::try_from(o) else { return };
            let d = l.diffuse_color;
            rec.lights[i] = Some(([d.x, d.y, d.z], l.dimmer));
            rec.nodes[i] = Some(node_of(&kind, l, ix));
        }
        NiType::NiTextureEffect(t) => {
            if t.texture_type as i32 == 2 && t.coordinate_generation_type as i32 == 2 {
                rec.texeffects[i] = Some((ix(t.source_texture.key), (t.texture_clamp as i32).clamp(0, 3) as u32));
            }
            rec.nodes[i] = Some(node_of(&kind, t, ix));
        }
        NiType::NiTriShapeData(d) => {
            let mut s = geom_data(d);
            s.tris = d.triangles.clone();
            rec.datas[i] = Some(s);
        }
        NiType::NiTriStripsData(d) => {
            let mut s = geom_data(d);
            s.tris = strip_tris(d);
            rec.datas[i] = Some(s);
        }
        NiType::NiAutoNormalParticlesData(_) | NiType::NiParticlesData(_) | NiType::NiRotatingParticlesData(_) => {
            let Ok(p) = <&NiParticlesData>::try_from(o) else { return };
            rec.pdatas[i] = Some(ParticleData {
                nverts: p.vertices.len(),
                radius: p.particle_radius,
                sizes: p.sizes.clone(),
                colours: p.vertex_colors.iter().map(|c| [c.x, c.y, c.z, c.w]).collect(),
                rotating: matches!(o, NiType::NiRotatingParticlesData(_)),
            });
        }
        NiType::NiParticleSystemController(_) | NiType::NiBSPArrayController(_) => {
            let Ok(c) = <&NiParticleSystemController>::try_from(o) else { return };
            let ic = c.initial_color;
            rec.pctrls[i] = Some(ParticleCtrl {
                flags: c.flags,
                frequency: c.frequency,
                start: c.start_time,
                stop: c.stop_time,
                target: ix(c.target.key),
                velocity: c.speed,
                velocity_random: c.speed_variation,
                vertical_dir: c.declination_angle,
                vertical_angle: c.declination_variation,
                horizontal_dir: c.planar_angle,
                horizontal_angle: c.planar_angle_variation,
                colour: [ic.x, ic.y, ic.z, ic.w],
                size: c.initial_size,
                emit_start: c.emit_start_time,
                emit_stop: c.emit_stop_time,
                emit_rate: c.birth_rate,
                lifetime: c.lifespan,
                lifetime_random: c.lifespan_variation,
                // Two bytes in the file, read as one little-endian short by the engine.
                emit_flags: c.use_birth_rate as u16 | (c.spawn_on_death as u16) << 8,
                offset_random: [c.emitter_width, c.emitter_height, c.emitter_depth],
                emitter: ix(c.emitter.key),
                num_particles: c.particles.len() as u16,
                num_valid: active_count(c),
                modifier: ix(c.particle_modifier.key),
                collider: ix(c.particle_collider.key),
            });
        }
        NiType::NiUVController(c) => {
            let (next, _flags, frequency, phase, start, stop, target) = clock(c, ix);
            rec.uvctrls[i] = Some((next, target, ix(c.data.key), frequency, phase, start, stop, c.texture_set as u32));
        }
        NiType::NiUVData(d) => {
            rec.uvdatas[i] = Some(UvAnim {
                u_trans: float_keys(&d.u_offset_data.keys),
                v_trans: float_keys(&d.v_offset_data.keys),
                u_scale: float_keys(&d.u_tiling_data.keys),
                v_scale: float_keys(&d.v_tiling_data.keys),
                ..UvAnim::default()
            });
        }
        NiType::NiKeyframeController(c) => {
            let (next, flags, frequency, phase, start, stop, target) = clock(c, ix);
            rec.kfctrls[i] = Some(KfCtrl { next, flags, frequency, phase, start, stop, target, data: ix(c.data.key) });
        }
        NiType::NiKeyframeData(d) => rec.kfdatas[i] = Some(keyframes(d)),
        NiType::NiVisController(c) => {
            let (next, flags, frequency, phase, start, stop, target) = clock(c, ix);
            rec.visctrls[i] = Some(KfCtrl { next, flags, frequency, phase, start, stop, target, data: ix(c.data.key) });
        }
        NiType::NiVisData(d) => {
            rec.visdatas[i] = Some(d.keys.iter().map(|k| (k.time, if k.value != 0 { 1.0 } else { 0.0 })).collect());
        }
        NiType::NiGeomMorpherController(c) => {
            let (next, flags, frequency, phase, start, stop, target) = clock(c, ix);
            rec.morphctrls[i] = Some(MorphCtrl { next, flags, frequency, phase, start, stop, target, data: ix(c.data.key) });
        }
        NiType::NiMorphData(d) => {
            rec.morphdatas[i] = Some(MorphData {
                relative: d.relative_targets,
                morphs: d.targets.iter().map(|t| (float_keys(&t.keys), t.vertices.iter().map(|v| v3(*v)).collect())).collect(),
            });
        }
        NiType::NiAlphaController(c) => {
            let (next, flags, frequency, phase, start, stop, target) = clock(c, ix);
            rec.alphactrls[i] = Some((next, flags, frequency, phase, start, stop, target, ix(c.data.key)));
        }
        NiType::NiFloatData(d) => rec.floatdatas[i] = Some(float_keys(&d.keys)),
        NiType::NiGravity(g) => {
            rec.pmods[i] = Some(ParticleMod::Gravity {
                next: ix(g.next.key),
                decay: g.decay,
                force: g.strength,
                kind: g.force_type as i32 as u32,
                position: v3(g.position),
                direction: v3(g.direction),
            });
        }
        NiType::NiParticleGrowFade(g) => {
            rec.pmods[i] = Some(ParticleMod::GrowFade { next: ix(g.next.key), grow: g.grow_time, fade: g.fade_time });
        }
        NiType::NiParticleColorModifier(m) => {
            rec.pmods[i] = Some(ParticleMod::Colour { next: ix(m.next.key), data: ix(m.color_data.key) });
        }
        NiType::NiParticleRotation(m) => {
            rec.pmods[i] = Some(ParticleMod::Rotation {
                next: ix(m.next.key),
                random_axis: m.random_initial_axis,
                axis: v3(m.initial_axis),
                speed: m.rotation_speed,
            });
        }
        NiType::NiParticleBomb(m) => rec.pmods[i] = Some(ParticleMod::Other { next: ix(m.next.key) }),
        NiType::NiColorData(d) => {
            let NiColorKey::LinKey(keys) = &d.keys;
            rec.colour_data[i] = Some(keys.iter().map(|k| [k.time, k.value.x, k.value.y, k.value.z, k.value.w]).collect());
        }
        NiType::NiStringExtraData(e) => {
            rec.extras[i] = Some(ExtraString { value: e.value.clone(), next: ix(e.next.key) });
        }
        NiType::NiTextKeyExtraData(e) => {
            rec.textkeys[i] = Some(e.keys.iter().map(|k| (k.time, k.value.clone())).collect());
            rec.extras[i] = Some(ExtraString { value: String::new(), next: ix(e.next.key) });
        }
        NiType::NiSequenceStreamHelper(h) => {
            rec.seq = Some((ix(h.extra_data.key), ix(h.controller.key)));
        }
        NiType::NiSkinInstance(s) => {
            rec.skins[i] = Some(SkinInst {
                data: ix(s.data.key),
                root: ix(s.root.key),
                bones: s.bones.iter().map(|l| ix(l.key)).collect(),
            });
        }
        NiType::NiSkinData(s) => {
            rec.skin_datas[i] = Some(SkinData {
                trafo: (trafo(s.rotation, s.translation), s.scale),
                bones: s
                    .bone_data
                    .iter()
                    .map(|b| SkinBone { trafo: (trafo(b.rotation, b.translation), b.scale), weights: b.vertex_weights.clone() })
                    .collect(),
            });
        }
        NiType::NiTexturingProperty(t) => {
            let m = &mut rec.mats;
            for (slot, map) in t.texture_maps.iter().enumerate() {
                let Some(map) = map else { continue };
                let base = match map {
                    TextureMap::Map(b) => b,
                    TextureMap::BumpMap(b) => &b.base,
                };
                let (src, uv_set, clamp) = (ix(base.texture.key), base.texture_index as u32, (base.clamp_mode as i32).clamp(0, 3) as u32);
                match slot {
                    0 => {
                        m.tex_source[i] = Some(src);
                        m.base_clamp[i] = Some(clamp);
                        m.base_uv_set[i] = Some(uv_set);
                    }
                    1 => m.dark_source[i] = Some((src, uv_set)),
                    2 => m.detail_source[i] = Some((src, uv_set)),
                    3 => m.gloss_source[i] = Some((src, uv_set)),
                    4 => m.glow_source[i] = Some((src, uv_set)),
                    5 => {
                        if let TextureMap::BumpMap(b) = map {
                            let d = b.displacement.to_cols_array();
                            m.bump_source[i] = Some((src, uv_set, b.luma_scale, b.luma_offset, d));
                        }
                    }
                    6 => m.decal_source[i] = Some((src, uv_set)),
                    _ => {}
                }
            }
        }
        NiType::NiSourceTexture(t) => match &t.source {
            TextureSource::External(f) => rec.mats.tex_file[i] = Some(f.clone()),
            // Wraithguard: a texture inside the file (NiPixelData), fetched by its index.
            TextureSource::Internal(px) => {
                let at = ix(px.key);
                if at >= 0 {
                    rec.mats.tex_file[i] = Some(format!("{}{at}", super::EMBEDDED));
                }
            }
        },
        NiType::NiMaterialProperty(p) => {
            let alpha = p.alpha.clamp(0.0, 1.0);
            rec.mats.material[i] = Some((c3(p.diffuse_color), alpha, c3(p.emissive_color), c3(p.ambient_color)));
            rec.mats.material_ctrl[i] = ix(p.controller.key);
        }
        NiType::NiAlphaProperty(p) => rec.mats.alpha[i] = Some((p.flags, p.test_ref)),
        NiType::NiStencilProperty(p) => rec.mats.stencil[i] = Some(p.draw_mode as i32 as u32),
        NiType::NiZBufferProperty(p) => rec.mats.zbuffer[i] = Some(p.flags),
        NiType::NiVertexColorProperty(p) => {
            rec.mats.vcol_prop[i] = Some((p.source_vertex_mode as i32 as u32, p.lighting_mode as i32 as u32));
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::super::own_records;
    use super::crate_records;

    const ANIM: &[u8] = include_bytes!("../../../../tests/fixtures/gardenfell_anim/anim.nif");

    #[test]
    fn the_crate_reads_what_the_engine_reader_reads() {
        let a = crate_records(ANIM).expect("the crate reads the fixture");
        let b = own_records(ANIM).expect("the engine reads the fixture");
        assert_eq!(a.types, b.types);
        for (x, y) in a.nodes.iter().zip(&b.nodes) {
            let key = |n: &Option<super::Node>| {
                n.as_ref().map(|n| (n.kind.clone(), n.name.clone(), n.flags, n.trafo, n.scale, n.children.clone(), n.props.clone(), n.data_ref, n.ctrl))
            };
            assert_eq!(key(x), key(y));
        }
        for (x, y) in a.datas.iter().zip(&b.datas) {
            let key = |d: &Option<super::ShapeData>| d.as_ref().map(|d| (d.verts.clone(), d.tris.clone(), d.uvs.clone(), d.normals.clone()));
            assert_eq!(key(x), key(y));
        }
        assert_eq!(a.mats.tex_file, b.mats.tex_file);
        assert_eq!(a.kfctrls.iter().flatten().count(), b.kfctrls.iter().flatten().count());
    }

    #[test]
    fn garbage_and_panics_are_none() {
        assert!(crate_records(b"not a nif at all, not even close to one, no no no").is_none());
        let mut cut = ANIM.to_vec();
        cut.truncate(ANIM.len() / 2);
        assert!(crate_records(&cut).is_none());
    }
}
