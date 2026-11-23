/* Animation blending (78_wg_animblend.js, 10_preview.js `blendTS`, 18_cellpreview.js
   `previewSwapActor`), as OpenMW's "Smooth animation transitions": the rules read as OpenMW
   reads them (its own animation-config.yaml; the bottom-most rule that fits; `*` at either
   end; `$`; a model's own after the global ones; JSON too), OpenMW's easings, and the blend -
   a new animation's node starting from the pose the old one was in and easing into its own
   over the rule's duration, scale at once; a skinned part's bones likewise; mid-blend, a change
   again starting from where the blend had it. */
'use strict';
const vm = require('vm');
const fs = require('fs');
const path = require('path');

const SRC = path.join(__dirname, '..', '..', 'src');
const cp = fs.readFileSync(path.join(SRC, '10_preview.js'), 'utf8');
const pick = name => { const i = cp.search(new RegExp('^(async )?function ' + name + '\\b', 'm')); if (i < 0) throw new Error('no ' + name); const j = cp.slice(i + 1).search(/^(async function |function |const |\/\*\*|\/\* )/m); return cp.slice(i, i + 1 + j); };

// OpenMW's own files/data/animations/animation-config.yaml, as it ships (the comments kept).
const OPENMW_YAML = `# This is the default OpenMW animation blending config file (global config) , will affect NPCs and creatures but not animated objects.
blending_rules:
  # General blending rule, any transition that will not be caught by the rules below - will use this rule
  - from: "*"
    to: "*"
    easing: "sineOut"
    duration: 0.25
  # From anything to sneaking
  - from: "*"
    to: "idlesneak*"
    easing: "springOutWeak"
    duration: 0.4
  - from: "*:*start"
    to: "*:*attack"
    easing: "sineOut"
    duration: 0.05
  - from: "*"
    to: "*:*follow start"
    easing: "linear"
    duration: 0
  - from: "jump:start"
    to: "*"
    easing: "sineOut"
    duration: 0.1
  # Transitions from a no-state are always instant
  - from: ""
    to: "*"
    easing: "linear"
    duration: 0
`;

module.exports = async (t) => {
  const ctx = { console, Math, JSON, Map, Set, Promise, Object, Array, String, Number, Float32Array, Engine: { async call() { return { global: OPENMW_YAML, files: { 'r/guar.nif': '{"blending_rules":[{"from":"*","to":"runforward","easing":"cubicOut","duration":0.5}]}' } }; } } };
  vm.createContext(ctx);
  vm.runInContext(['ctrlTime', 'keyAt', 'quatAt', 'quatMat3', 'mat3Quat', 'slerpQ', 'blendTS', 'animLinksAt', 'mul3x3', 'mulTS', 'animNodeAt', 'animChainTS', 'boneFullTS', 'sameChain'].map(pick).join('\n')
    + '\n' + (() => { const i = cp.indexOf('function skinBoneMats('), j = cp.indexOf('/** Wraithguard: a bone\'s chain and still tail', i); return cp.slice(i, j); })()
    + '\n' + fs.readFileSync(path.join(SRC, '78_wg_animblend.js'), 'utf8').replace(/^const WgAnimBlend=/m, 'var WgAnimBlend='), ctx);
  const B = ctx.WgAnimBlend;

  // The rules: OpenMW's defaults until read; then the file's, bottom-most first.
  t.ok(B.find('idle:start', 'walkforward:start').easing === 'sineOut' && B.find('idle:start', 'walkforward:start').duration === 0.25, 'the general rule');
  await B.load(['r/guar.nif']);
  const g = B._global;
  t.ok(g && g.length === 6 && g[1].tg === 'idlesneak*' && g[2].fk === '*start', 'OpenMW\'s file read: ' + JSON.stringify(g && g.map(r => [r.fg, r.fk, r.tg, r.tk, r.easing, r.duration])));
  t.ok(B.find('walkforward:start', 'idlesneak:start').easing === 'springOutWeak', 'to sneaking: springOutWeak');
  t.ok(B.find('weapononehanded:chop start', 'weapononehanded:chop max attack').duration === 0.05, 'start to attack, by keys with wildcards');
  t.ok(B.find('jump:start', 'idle:start').duration === 0.1, 'out of a jump');
  t.ok(B.find(':', 'idle:start').duration === 0 || B.find('', 'idle:start').duration === 0, 'from no state: instant');
  t.ok(B.find('idle:start', 'runforward:start', 'r/guar.nif').easing === 'cubicOut' && B.find('idle:start', 'runforward:start').easing === 'sineOut', 'a model\'s own JSON rules after the global ones, for it alone');
  B._global = B._global.concat([B.rule('walk*', '$', 'linear', 0.7)]);
  t.ok(B.find('walkforward:start', 'walkforward:loop').duration === 0.7 && B.find('walkforward:start', 'runforward:start').duration !== 0.7, '`$`: the same group');

  // The easings, as OpenMW's.
  const E = B.EASE;
  t.ok(Math.abs(E.sineOut(0.5) - Math.sin(Math.PI / 4)) < 1e-9 && E.cubicInOut(0.25) === 0.0625 && Math.abs(E.springOutWeak(1) - (1 - Math.exp(-4) * Math.cos(1.5 * Math.PI))) < 1e-9, 'easings');

  // A rigid part: one node, turned 90° about z by its old animation, 0° by its new; moved x 0 -> 10; scale 2 -> 1.
  const node = (deg, x, s) => ({ pre: [1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0], local: [Math.cos(deg * Math.PI / 180), -Math.sin(deg * Math.PI / 180), 0, Math.sin(deg * Math.PI / 180), Math.cos(deg * Math.PI / 180), 0, 0, 0, 1, x, 0, 0], localS: s, start: 0, stop: 0, freq: 1, phase: 0 });
  const oldB = [{ part: { anim: { nodes: [node(90, 0, 2)] } } }], newB = [{ part: { anim: { nodes: [node(0, 10, 1)] } } }];
  B._global = [B.rule('*', '*', 'linear', 1)];
  t.ok(B.begin(oldB, newB, 'Idle2@3', 'WalkForward@0', null, 100) === true && newB[0]._blend, 'a blend begun');
  const at = secs => { const f = B.factor(newB[0]._blend.B, secs); const n = ctx.blendTS(newB[0]._blend.nodes[0], ctx.animNodeAt(newB[0].part.anim.nodes[0], secs), f == null ? 1 : f); return { deg: Math.round(Math.atan2(n.m[3], n.m[0]) * 180 / Math.PI), x: n.m[9], s: n.s }; };
  const a0 = at(100), a5 = at(100.5), a1 = at(101.2);
  t.ok(a0.deg === 90 && a0.x === 0, 'at the change: the old pose ' + JSON.stringify(a0));
  t.ok(a5.deg === 45 && Math.abs(a5.x - 5) < 1e-6 && a5.s === 1, 'half way (linear): half turned, half moved, the new scale at once ' + JSON.stringify(a5));
  t.ok(a1.deg === 0 && a1.x === 10 && newB[0]._blend.B.done, 'over: the new animation\'s own pose ' + JSON.stringify(a1));

  // Changed again mid-blend: from where the blend had it.
  const midB = [{ part: { anim: { nodes: [node(0, 10, 1)] } } }];
  const firstB = [{ part: { anim: { nodes: [node(90, 0, 1)] } } }], secondB = [{ part: { anim: { nodes: [node(0, 10, 1)] } } }];
  B.begin(firstB, secondB, 'idle', 'walkforward@0', null, 200);
  B.begin(secondB, midB, 'walkforward@0', 'idle2@1', null, 200.5);
  const from = midB[0]._blend.nodes[0];
  t.ok(Math.round(Math.atan2(from.m[3], from.m[0]) * 180 / Math.PI) === 45 && Math.abs(from.m[9] - 5) < 1e-6, 'a change mid-blend starts where the blend had it');

  // A skinned part: each bone's chain.
  const bone = deg => ({ chain: [node(deg, 0, 1)], post: [1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0], trafo: [1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0] });
  const oS = [{ part: { skin: { bones: [bone(60), bone(0)] } } }], nS = [{ part: { skin: { bones: [bone(0), bone(0)] } } }];
  B.begin(oS, nS, null, 'walkforward@0', null, 300);
  const p = nS[0].part, f = B.factor(p._blend.B, 300.5);
  const mats = ctx.skinBoneMats(p.skin, 300.5, p._blend.skin, f);
  t.ok(Math.round(Math.atan2(mats[0].m[3], mats[0].m[0]) * 180 / Math.PI) === 30 && Math.round(Math.atan2(mats[1].m[3], mats[1].m[0]) * 180 / Math.PI) === 0, 'a skinned part\'s bones blend each from its own pose');

  /* A bone whose chain is other nodes in the new animation, as long (an idle moves the spine,
     the walk the thigh): blended as the bone's whole transform, never one node's pose put on
     another node. The old chain: a spine node 50 up, turned 90°; the new: a thigh node with its
     own 50 up as its still part. The bone starts where it was, and stays near it. */
  const at50 = [1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 50];
  const spine = Object.assign(node(90, 0, 1), { pre: at50 });
  const thigh = node(0, 0, 1);
  const oX = [{ part: { skin: { bones: [{ chain: [spine], post: [1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0], trafo: [1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0] }] } } }];
  const nX = [{ part: { skin: { bones: [{ chain: [thigh], post: at50, trafo: [1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0] }] } } }];
  t.ok(!ctx.sameChain([spine], [thigh]), 'chains of other nodes are told apart');
  B.begin(oX, nX, 'idle', 'walkforward@0', null, 400);
  const px = nX[0].part, mx = s => ctx.skinBoneMats(px.skin, s, px._blend.skin, B.factor(px._blend.B, s) ?? 1)[0];
  const s0 = mx(400), sH = mx(400.5);
  t.ok(px._blend.skin[0].full && Math.round(Math.atan2(s0.m[3], s0.m[0]) * 180 / Math.PI) === 90 && Math.abs(s0.m[11] - 50) < 1e-6, 'the bone starts where the old animation had it ' + JSON.stringify(s0));
  t.ok(Math.round(Math.atan2(sH.m[3], sH.m[0]) * 180 / Math.PI) === 45 && Math.abs(sH.m[11] - 50) < 1e-6, 'and turns toward the new pose in place, not flung ' + JSON.stringify(sH));

  // No rule, a nought duration, or a different skeleton: no blend.
  B._global = [B.rule('*', '*', 'linear', 0)];
  const n2 = [{ part: { anim: { nodes: [node(0, 0, 1)] } } }];
  t.ok(B.begin(oldB, n2, 'idle', 'walkforward', null, 0) === false && !n2[0]._blend, 'a nought duration: none');
  B._global = [B.rule('*', '*', 'linear', 1)];
  const n3 = [{ part: { anim: { nodes: [node(0, 0, 1), node(0, 0, 1)] } } }];
  t.ok(B.begin(oldB, n3, 'idle', 'walkforward', null, 0) === false, 'another skeleton: none');
};
