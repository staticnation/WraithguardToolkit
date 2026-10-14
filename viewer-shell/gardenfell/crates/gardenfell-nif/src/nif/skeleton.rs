//! Parts hung on another file's skeleton: an NPC's body parts and what it wears, bound by
//! bone name to the skeleton the race uses (`base_anim.nif`), with an idle playing. From
//! Wraithguard (Static), whose viewer assembles NPCs this way.

use super::*;

impl KfData {
/// Only the keys a clip from `start` to `stop` plays - those inside it,
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
            rot_order: self.rot_order,
            trans: cut(&self.trans, start, stop),
            scales: cut(&self.scales, start, stop),
        }
    }
}


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
    let s = read_stream(buf)?;
    let rec = records_from_stream_opts(&s, None, true);
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

/// A point in a carried mesh's own space, where the skeleton's `bone` holds
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
    use super::*;

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

    #[test]
    fn skeleton_binding_keeps_only_the_active_clips_keys() {
        let mut stream = Stream::new();
        let mut root = ni::NiNode::default();
        root.base.base.name = "root".into();
        root.children = vec![Link::new(1)];
        stream.blocks.push(ni::NiType::NiNode(root));
        let mut bone = ni::NiNode::default();
        bone.base.base.name = "Bone".into();
        stream.blocks.push(ni::NiType::NiNode(bone));
        stream.roots = vec![Link::new(0)];
        let sequence = KfSequence {
            text_keys: vec![(10.5, "Idle: Start".into()), (20.5, "Idle: Stop".into())],
            tracks: vec![KfTrack {
                node: "Bone".into(),
                ctrl: KfCtrl::default(),
                data: KfData {
                    scales: (0..1000).map(|i| (i as f32, i as f32)).collect(),
                    trans: (0..1000).map(|i| (i as f32, [i as f32, 0.0, 0.0])).collect(),
                    rot: (0..1000).map(|i| (i as f32, [1.0, 0.0, 0.0, 0.0])).collect(),
                    rot_xyz: Some(std::array::from_fn(|_| (0..1000).map(|i| (i as f32, i as f32)).collect())),
                    ..Default::default()
                },
            }],
        };
        let bytes = stream.write().expect("synthetic skeleton");
        let skel = Skeleton::read(&bytes, Some(&sequence)).expect("skeleton reads");
        let bone = skel.bone("Bone", (IDENT, 1.0)).expect("bone binds");
        assert_eq!(bone.chain.len(), 1);
        let link = &bone.chain[0];
        assert_eq!((link.ctrl.start, link.ctrl.stop), (10.5, 20.5));
        let data = &link.data;
        assert_eq!(data.scales.len(), 12, "unplayed clips must not be copied into bone chains");
        assert_eq!(data.trans.len(), 12);
        assert_eq!(data.rot.len(), 12);
        for channel in data.rot_xyz.as_ref().unwrap() {
            assert_eq!(channel.len(), 12);
        }
        assert_eq!(data.scales.first().unwrap().0, 10.0);
        assert_eq!(data.scales.last().unwrap().0, 21.0);
        let original = &sequence.tracks[0].data;
        assert_eq!(original.scales.len(), 1000, "the source sequence stays intact");
        for time in [10.5, 11.0, 15.25, 20.5] {
            assert_eq!(key1_at(&data.scales, time), key1_at(&original.scales, time));
            assert_eq!(key_at(&data.trans, time), key_at(&original.trans, time));
            assert_eq!(key_at(&data.rot, time), key_at(&original.rot, time));
        }
    }

}
