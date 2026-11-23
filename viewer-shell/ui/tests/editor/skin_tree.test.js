/* A skin's bones posed node by node (10_preview.js `skinBoneMats`, Static 2026-11-23: "fps drop
   when actors are moving"): the chains made one tree, each node worked out once a frame and
   shared by the parts of an actor at the same moment - the same matrices as each bone's own
   chain from the root, and far less work. */
'use strict';
const fs = require('fs');
const path = require('path');
const vm = require('vm');

const cp = fs.readFileSync(path.join(__dirname, '..', '..', 'src', '10_preview.js'), 'utf8');
const pick = name => { const i = cp.search(new RegExp('^function ' + name + '\\b', 'm')); if (i < 0) throw new Error('no ' + name); const j = cp.slice(i + 1).search(/^(function |const |\/\*\*|\/\* )/m); return cp.slice(i, i + 1 + j); };
const span = (a, b) => { const i = cp.indexOf(a), j = cp.indexOf(b, i); if (i < 0 || j < 0) throw new Error('no ' + a); return cp.slice(i, j); };

module.exports = async (t) => {
  const ctx = { Math, JSON, Map, Array, Object, Number, Float32Array };
  vm.createContext(ctx);
  vm.runInContext(['ctrlTime', 'keyAt', 'quatAt', 'quatMat3', 'mat3Quat', 'slerpQ', 'blendTS', 'mul3x3', 'mulTS', 'animNodeAt', 'animChainTS', 'boneFullTS'].map(pick).join('\n')
    + '\n' + span('function skinBoneMats(', '/** Wraithguard: a bone\'s chain and still tail'), ctx);
  // A skeleton: a root, a spine of four, two arms of three off the spine's top, each node turning on its own keys.
  let seed = 7; const rnd = () => (seed = (seed * 16807) % 2147483647) / 2147483647;
  const node = () => {
    const keys = []; for (let i = 0; i <= 10; i++) { const a = rnd() * 6.28; keys.push([i * 0.1, Math.cos(a / 2), 0, 0, Math.sin(a / 2)]); }
    return { pre: [1, 0, 0, 0, 1, 0, 0, 0, 1, rnd() * 10, rnd() * 10, rnd() * 10], preS: 1, local: [1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0], localS: 1,
      flags: 8, freq: 1, phase: 0, start: 0, stop: 1, rot: keys, xyz: null, trans: [], scale: null };
  };
  // Each bone's chain its own copy (as the engine sends them: no node shared by reference).
  const copy = x => JSON.parse(JSON.stringify(x));
  const spine = [node(), node(), node(), node()], armL = [node(), node(), node()], armR = [node(), node(), node()];
  const I = [1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0];
  const bones = [];
  for (let i = 1; i <= spine.length; i++) bones.push({ chain: copy(spine.slice(0, i)), post: I, trafo: I });
  for (const arm of [armL, armR]) for (let i = 1; i <= arm.length; i++) bones.push({ chain: copy(spine.concat(arm.slice(0, i))), post: I, trafo: [1, 0, 0, 0, 1, 0, 0, 0, 1, 1, 2, 3] });
  const skin = { bones }, skin2 = { bones: copy(bones) };
  const old = (sk, s) => sk.bones.map(b => { const p = ctx.boneFullTS(b, s, null, 1); return ctx.mulTS(p.m, p.s, b.trafo, 1); });
  let worst = 0;
  for (const s of [0, 0.13, 0.5, 0.77, 3.21]) {
    const a = ctx.skinBoneMats(skin, s, null, 1), b = old(skin, s), c = ctx.skinBoneMats(skin2, s, null, 1);
    for (let i = 0; i < a.length; i++) for (let k = 0; k < 12; k++) worst = Math.max(worst, Math.abs(a[i].m[k] - b[i].m[k]), Math.abs(c[i].m[k] - b[i].m[k]));
  }
  t.ok(worst < 1e-9, 'each bone as its own chain from the root has it (off by ' + worst + ')');
  t.ok(skin._tree.nodes.length === 10 && bones.reduce((n, b) => n + b.chain.length, 0) === 46, 'the chains\' 46 links are 10 nodes: ' + skin._tree.nodes.length);
  // Two parts of one actor at the same moment share the nodes: the second part works none out.
  let calls = 0; const at = ctx.animNodeAt; ctx.animNodeAt = (L, s) => { calls++; return at(L, s); };
  vm.runInContext('animNodeAt = this.animNodeAt', ctx);
  ctx.skinBoneMats(skin, 9.5, null, 1); const first = calls; ctx.skinBoneMats(skin2, 9.5, null, 1);
  t.ok(first === 10 && calls === 10, 'a node once a frame, for both parts: ' + first + ', then ' + (calls - first));
  vm.runInContext('animNodeAt = ' + pick('animNodeAt').replace(/^function animNodeAt/, 'function'), ctx);
  const t0 = Date.now(); for (let i = 0; i < 2000; i++) old(skin, i * 0.01); const tOld = Date.now() - t0;
  const t1 = Date.now(); for (let i = 0; i < 2000; i++) ctx.skinBoneMats(skin, i * 0.01, null, 1); const tNew = Date.now() - t1;
  t.ok(tNew < tOld, 'quicker: ' + tNew + ' ms against ' + tOld + ' ms for 2000 poses of one part');
};
