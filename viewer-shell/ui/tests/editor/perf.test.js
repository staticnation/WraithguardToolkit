/* Where a frame's CPU time goes (05_wg_perf.js, Static 2026-11-23): each part's time and
   counts kept a frame, the last frames summed up - mean, worst, the gaps between frames and
   Lua's round trips. */
'use strict';
const fs = require('fs');
const path = require('path');
const vm = require('vm');

module.exports = async (t) => {
  let clock = 0;
  const ctx = { performance: { now: () => clock }, Math, Object, Set, Array };
  vm.createContext(ctx);
  vm.runInContext(fs.readFileSync(path.join(__dirname, '..', '..', 'src', '05_wg_perf.js'), 'utf8').replace(/^const WgPerf=/m, 'var WgPerf='), ctx);
  const P = ctx.WgPerf;
  for (let i = 0; i < 10; i++) {
    P.add('pose', 2); P.count('posed', 30);
    if (i === 7) P.add('pose', 20);
    P.time('ai', () => { clock += 1; });
    clock += 15; P.frame();
  }
  P.trip(12); P.trip(30);
  const s = P.summary();
  const pose = s.parts.find(x => x.name === 'pose');
  t.ok(s.frames === 10 && s.parts[0].name === 'pose' && Math.abs(pose.mean - 4) < 1e-9 && pose.worst === 22, 'a part\'s mean and worst a frame ' + JSON.stringify(pose));
  t.ok(s.parts.find(x => x.name === 'ai').mean === 1 && s.counts.posed === 30, 'timed by `time`, counted by `count`');
  t.ok(s.gap.median === 16 && s.trip.worst === 30 && s.trip.n === 2, 'the gaps between frames and the round trips ' + JSON.stringify([s.gap, s.trip]));
  for (let i = 0; i < 200; i++) P.frame();
  t.ok(P.ring.length === 120, 'the last 120 frames kept');
};
