/* Smoother actors (73_wg_scripts.js `stepTo`, `turned`, `go`; Static 2026-11-23: "the
   animations and movement are way better but could still use optimization to be more fluid
   and smooth"): an actor turns toward where it goes at a set rate rather than at once, walks
   the way it faces (slowed while it still faces away, so it never slides sideways), eases its
   pace up from still, and eases over the floor's small bumps while taking a real step at once. */
'use strict';
const vm = require('vm');
const fs = require('fs');
const path = require('path');

module.exports = async (t) => {
  let ground = () => 0;
  const ctx = { console, Math, JSON, Map, Set, Object, Array, String, Number, Infinity, isFinite,
    performance: { now: () => 0 }, localStorage: { getItem: () => null, setItem() {} },
    App: { _scene: { origin: [0, 0], cells: [] }, R: { groundZ: (x, y) => ground(x, y) } } };
  vm.createContext(ctx);
  vm.runInContext(fs.readFileSync(path.join(__dirname, '..', '..', 'src', '73_wg_scripts.js'), 'utf8').replace(/^const (\w+)=/mg, 'var $1='), ctx);
  const S = ctx.WgScripts;
  const T = { key: 'a', base: { pos: [0, 0, 0], rot: [0, 0, 0] } };
  S.moved = new Map(); S.refOf = () => null;
  const rotOf = () => S.rotOf(T)[2], posOf = () => S.posOf(T);

  // A goal straight behind it (it faces +y; the goal is at -y): it turns first, in place.
  S._dt = 0.05;
  S.stepTo(T, [0, -1000, 0], 7.5, 32);
  t.ok(Math.abs(Math.abs(rotOf()) - S.TURN * 0.05) < 1e-9, 'one frame turns it TURN·dt, not all the way: ' + rotOf());
  t.ok(Math.hypot(posOf()[0], posOf()[1]) < 1e-6, 'facing away, it does not move (no walking backwards or sideways)');
  for (let i = 0; i < 20; i++) S.stepTo(T, [0, -1000, 0], 7.5, 32);
  t.ok(Math.abs(Math.abs(rotOf()) - Math.PI) < 0.1 && posOf()[1] < -50, 'turned round (its line bent a little as it turned), it walks to it: ' + JSON.stringify(posOf().map(Math.round)));
  const helper = S.turned(0, 3, 1);
  t.ok(helper[0] === 3 && helper[1] === 0, 'a turn that fits in the frame lands exactly on the way to go');
  t.ok(Math.abs(S.angleTo(3, -3) - (2 * Math.PI - 6)) < 1e-9, 'the short way round across ±π');

  // Its pace eases up from still over a fifth of a second.
  const A = { t: { key: 'b', base: { pos: [0, 0, 0], rot: [0, 0, 0] } }, _route: null };
  S.go(A, [0, 1000, 0], 150, 32, 0.05, false);
  t.ok(Math.abs(A._pace - 37.5) < 1e-9, 'the first frame at a quarter of its walk: ' + A._pace);
  for (let i = 0; i < 4; i++) S.go(A, [0, 1000, 0], 150, 32, 0.05, false);
  t.ok(A._pace === 150, 'a fifth of a second in, its full walk');

  // The floor: a small bump eased over, a step taken at once.
  const C = { key: 'c', base: { pos: [0, 0, 0], rot: [0, 0, 0] } };
  ground = () => 4;
  S.stepTo(C, [0, 1000, 0], 7.5, 32);
  const z1 = S.posOf(C)[2];
  t.ok(z1 > 0 && z1 < 4, 'a bump of 4 is eased over: ' + z1);
  ground = () => 30;
  S.stepTo(C, [0, 1000, 0], 7.5, 32);
  t.ok(S.posOf(C)[2] === 30, 'a step of more than STEP_Z is taken at once');

  /* Stairs (Static 2026-11-24: "npcs seem to get stuck on stairs? one step and then they
     stop"), against the walk's own collision (71_wg_walk.js `aiFloor`, `aiBlocked`) with its
     ray casts answered by an exact profile along y: a ramp of 45 degrees, stairs of 16 by 16,
     and a wall. Two faults held them after a step: the eased height trailed a rising floor by
     up to STEP_Z, and the knee ray reached a whole half-width ahead, where 45-degree stairs are
     already above the knee. Up the stairs, walking and running, at 60 and 20 frames a second;
     and still stopped short of a wall. */
  ctx.WALK = { eye: 116, walk: 150, run: 300, gravity: 686, jump: 400, step: 34, radius: 29, swim: 115 };
  vm.runInContext(fs.readFileSync(path.join(__dirname, '..', '..', 'src', '71_wg_walk.js'), 'utf8').replace(/^const (\w+)=/mg, 'var $1='), ctx);
  const WW = ctx.WgWalk;
  let floor = () => 0;
  WW.st.collide = true; WW.watch = () => {}; WW._ai = { _world: {} };
  WW.rayIn = (W, o, d, max) => {
    if (!d[0] && !d[1]) { const f = floor(o[1]); return f <= o[2] ? { t: o[2] - f } : null; }   // down
    for (let t = 0; t <= max; t += 0.25) if (floor(o[1] + d[1] * t) > o[2]) return { t };      // level, along y
    return null;
  };
  ctx.App.R.cam = { tx: 0, ty: 0 };
  ground = () => null;
  const walkUp = (prof, speed, fps, secs) => {
    floor = prof;
    const B = { t: { key: 'st' + Math.random(), base: { pos: [0, -100, 0], rot: [0, 0, 0] } }, _route: null };
    S._dt = 1 / fps;
    let lag = 0, held = 0;
    for (let i = 0; i < fps * secs; i++) {
      const before = S.posOf(B.t)[1];
      S.go(B, [0, 2000, 0], speed, 32, 1 / fps, false);
      const q = S.posOf(B.t);
      if (q[1] === before && i > 2) held++;
      lag = Math.max(lag, prof(q[1]) - q[2]);
    }
    return { y: S.posOf(B.t)[1], z: S.posOf(B.t)[2], lag, held };
  };
  const ramp = y => y < 0 ? 0 : y, steps = y => y < 0 ? 0 : Math.floor(y / 16) * 16;
  for (const [name, prof] of [['ramp', ramp], ['steps of 16', steps]]) for (const speed of [150, 300]) for (const fps of [60, 20]) {
    const r = walkUp(prof, speed, fps, 3);
    t.ok(r.y > speed * 2 && r.held === 0, name + ' at ' + speed + ', ' + fps + ' fps: climbs (to y ' + Math.round(r.y) + ', z ' + Math.round(r.z) + ', ' + r.held + ' frames held)');
    t.ok(r.lag <= S.CLIMB_LAG + 0.01, name + ' at ' + speed + ', ' + fps + ' fps: the feet trail the floor by ' + S.CLIMB_LAG + ' at most going up: ' + r.lag.toFixed(2));
  }
  { const r = walkUp(y => y >= 500 ? 1000 : 0, 300, 60, 3);
    t.ok(r.y < 500 - 14 && r.y > 400, 'a wall still stops it, short of it: y ' + Math.round(r.y)); }
  delete ctx.WgWalk; delete ctx.WALK;
};
