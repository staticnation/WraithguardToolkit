/* Actors among the scene's objects while the Scripts run (73_wg_scripts.js `stepTo`, `go`,
   `keepLive`, `animSig`; 71_wg_walk.js `aiBlocked`; 77_wg_ai.js `room`). Static: "the
   movement rubberbands, and the actors dont have collision like 1st person camera".
   - A wall in the collision world stops an actor walking at it, as it stops the walking
     camera; held a moment, it gives the leg up rather than pushing forever.
   - The walking player keeps the actors out of its room.
   - The scene drawn again puts each live actor straight back where the scripts have it, not
     where it stood when the drawing began.
   - An object not drawn live playing the same group again does not have the scene drawn
     again (a mist's `PlayGroup idle2 1`, every frame). */
'use strict';
const { addTables, sceneTables } = require('./harness');
const vm = require('vm');
const fs = require('fs');
const path = require('path');

const UI = path.join(__dirname, '..', '..');
const src = f => fs.readFileSync(path.join(UI, 'src', f), 'utf8').replace(/^const (\w+)=/mg, 'var $1=');

module.exports = async (t) => {
  const ref = (id, key, pos) => ({ id, key, pos, rot: [0, 0, 0], scale: 1 });
  const cells = [{ kind: 'ext', gx: 0, gy: 0, name: '', refs: [ref('walker', 'n1', [0, 0, 0]), ref('mist', 'm1', [3000, 3000, 0])] }];
  const moves = [], rebuilds = [];
  let now = 0;
  const picks = [];
  const ctx = {
    console, Math, JSON, Map, Set, Promise, Object, Array, String, Number, Error, Infinity, isFinite, Float32Array,
    performance: { now: () => now }, requestAnimationFrame: () => 0, cancelAnimationFrame() {}, setTimeout: (f) => f(), clearTimeout() {},
    localStorage: { getItem: () => null, setItem() {} },
    document: { activeElement: null, getElementById: () => null },
    $: () => null, escHtml: s => String(s), toast() {},
    schedulePreview() { rebuilds.push(now); },
    previewMoveRef(key, pos) { moves.push([key, pos.slice()]); return true; },
    App: { _scene: { origin: [0, 0], cells }, _liveRefs: new Map([['n1', {}]]),
      R: { groundZ: () => 0, pickables: picks, cam: { tx: 0, ty: 0, tz: 0, dist: 1, az: 0, el: 0 }, walk: false } },
    WgSound: { place: { on: false } },
    // 06_gl.js's body, as the page has it.
    WALK: { eye: 116, walk: 150, run: 300, step: 34, radius: 29 },
    Engine: {
      async bytes(c, a) { if (c === 'scene_scripts') return sceneTables(await this.call(c, a)); throw new Error('no ' + c); },
      async call(c) {
        if (c === 'scene_scripts') return { objects: {}, scripts: {}, start: [], globals: [], actors: { walker: { tag: 'NPC_', items: [], stats: [100, 50, 80] } } };
        if (c === 'anim_groups') return '{}';
        return {};
      },
    },
  };
  vm.createContext(ctx);
  addTables(ctx);
  vm.runInContext(src('38_wg_cellkey.js') + '\n' + src('71_wg_walk.js') + '\n' + src('73_wg_scripts.js') + '\n' + src('77_wg_ai.js'), ctx);
  const S = ctx.WgScripts, Wk = ctx.WgWalk;
  S.on = true;
  await S.gather();

  // The actors' collision world: a wall across x = 500 (two triangles, 0-300 high, y -1000..1000).
  const tris = new Float32Array([500, -1000, 0, 500, 1000, 0, 500, 1000, 300, 500, -1000, 0, 500, 1000, 300, 500, -1000, 300]);
  const grid = new Map();
  for (let gx = 1; gx <= 2; gx++) for (let gy = -4; gy <= 4; gy++) grid.set(gx + ',' + gy, [0, 1]);
  Object.assign(Wk._ai, { _world: { tris, grid, actors: [], count: 2 }, _picksRef: picks, _builtAt: [0, 0], _building: false });

  // Walking at the wall: it stops short of it.
  const walker = { key: 'n1', id: 'walker' };
  const A = S.actorOf(walker, true);
  S.ai(walker, 'travel', ['2000', '0', '0'], { self: walker }, true);
  for (let i = 0; i < 100; i++) { now += 50; S.frame(0.05); }
  const x = S.posOf(walker)[0];
  t.ok(x > 300 && x < 500, 'the wall stops it short of it: x ' + Math.round(x));
  t.ok(A._route === null || A._blockedT < 0.8, 'held a moment, the leg is given up rather than pushed at forever');

  // No wall in the way: it walks on (the same world, going the other way) - up onto a raised
  // floor 20 high from x -300 to -200 (a step: climbed), and down off it again.
  const floor = [-300, -1000, 20, -200, -1000, 20, -200, 1000, 20, -300, -1000, 20, -200, 1000, 20, -300, 1000, 20];
  const all = new Float32Array(tris.length + floor.length); all.set(tris); all.set(floor, tris.length);
  Wk._ai._world.tris = all;
  for (let gx = -2; gx <= -1; gx++) for (let gy = -4; gy <= 4; gy++) grid.set(gx + ',' + gy, [2, 3]);
  S.ai(walker, 'travel', ['-600', '0', '0'], { self: walker }, true);
  const heights = [];
  for (let i = 0; i < 200; i++) { now += 50; S.frame(0.05); const q = S.posOf(walker); heights.push([Math.round(q[0]), Math.round(q[2])]); }
  t.ok(S.posOf(walker)[0] < -500, 'away from the wall it walks freely: x ' + Math.round(S.posOf(walker)[0]));
  t.ok(heights.some(([x, z]) => x < -200 && x > -300 && z === 20), 'up onto the raised floor: ' + JSON.stringify(heights.filter(([x]) => x < -150 && x > -350)));
  t.ok(heights.every(([x, z]) => (x < -300 || x > -200) ? z === 0 : true), 'and down off it, on the land again');

  // The walking player: an actor stepping where the player stands is kept out of its room.
  ctx.App.R.walk = true;
  ctx.App.R.cam = { tx: -800, ty: 0, tz: 116, dist: 0.0001, az: 0, el: 0 };
  const p = [-800, 0, 0];
  ctx.WgAI.room('n1', p);
  t.ok(Math.hypot(p[0] + 800, p[1]) >= ctx.WgAI.ROOM - 1e-6, 'kept out of the walking player\'s room: ' + JSON.stringify(p.map(Math.round)));
  ctx.App.R.walk = false;

  // The scene drawn again: every live actor straight back where the scripts have it.
  moves.length = 0;
  ctx.App._liveRefs = new Map([['n1', {}]]);
  S.frame(0.001);
  const last = moves.filter(m => m[0] === 'n1').pop();
  t.ok(last && Math.abs(last[1][0] - S.posOf(walker)[0]) < 1, 'put where the scripts have it at once: ' + JSON.stringify(last));

  // A mist playing idle2 again and again: drawn once, not again for the same group.
  const mist = { key: 'm1', id: 'mist' };
  rebuilds.length = 0;
  S.playAnim('m1', 'idle2', 1, 1, 'script');
  for (let i = 0; i < 3; i++) { now += 200; S.frame(0.05); }
  const first = rebuilds.length;
  for (let i = 0; i < 20; i++) { S.playAnim('m1', 'idle2', 1, 1, 'script'); now += 400; S.frame(0.05); }
  t.ok(first === 1, 'its group drawn once: ' + first);
  t.ok(rebuilds.length === first, 'the same group again is not drawn again: ' + rebuilds.length + ' drawings');
};
