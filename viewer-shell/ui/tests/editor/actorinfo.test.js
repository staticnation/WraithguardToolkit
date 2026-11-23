/* What the actor overlay says over an actor (79_wg_actorinfo.js, Static 2026-11-23): its AI
   package, its animation and whether it blends or loads, whether it waits for its walk mesh,
   and how often it is posed. */
'use strict';
const fs = require('fs');
const path = require('path');
const vm = require('vm');

module.exports = async (t) => {
  const live = new Map([['n1', { spec: 'WalkForward@0*1.2', batches: [{ part: { _blend: {} } }], want: 'a', model: 'a' }]]);
  const ctx = {
    Math, String, performance: { now: () => 0 },
    App: { _liveRefs: live, R: { poseInterval: d2 => (d2 < 9e6 ? null : d2 < 3.6e7 ? 1 / 30 : 0) } },
    WgScripts: { warming: k => k === 'n2', anims: new Map(), animSpec: () => null },
    $: () => null, localStorage: { getItem() { return null; }, setItem() {} },
  };
  vm.createContext(ctx);
  vm.runInContext(fs.readFileSync(path.join(__dirname, '..', '..', 'src', '79_wg_actorinfo.js'), 'utf8').replace(/^const WgActorInfo=/m, 'var WgActorInfo='), ctx);
  const I = ctx.WgActorInfo;
  const a = I.lines({ key: 'n1', id: 'fargoth', ai: { kind: 'wander', range: 512 }, packages: [1, 2], pkg: 0, _moved: true }, 100);
  t.ok(JSON.stringify(a) === JSON.stringify(['fargoth', 'AI: wander 512 (1/2) · walking', 'anim: WalkForward · blending', 'posed every frame']), 'a walker near the camera ' + JSON.stringify(a));
  const b = I.lines({ key: 'n2', id: 'guard', ai: null, combat: 'x', _fleeing: true }, 2e7);
  t.ok(JSON.stringify(b) === JSON.stringify(['guard', 'AI: combat, fleeing', 'anim: idle', 'waiting for its walk mesh · not drawn live · posed 30/s']), 'one waiting, farther off ' + JSON.stringify(b));
};
