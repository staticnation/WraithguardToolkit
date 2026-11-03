/* The first-person body (71_wg_walk.js, and 06_gl.js's `_walkStep`): OpenMW's speeds from
   the stats, the collision shapes (a mesh with none is walked through), swimming in deep
   water (the head stays at the surface), levitating, and the steps it hears. */
'use strict';
const vm = require('vm');
const fs = require('fs');
const path = require('path');

const UI = path.join(__dirname, '..', '..');
const src = f => fs.readFileSync(path.join(UI, 'src', f), 'utf8')
  .replace(/^class (\w+)/mg, 'var $1=class $1').replace(/^const (\w+)=/mg, 'var $1=');

module.exports = async (t) => {
  const played = [];
  const ctx = {
    console, Math, JSON, Map, Set, Float32Array, DataView, ArrayBuffer, Uint8Array, TextDecoder, Promise, setTimeout, clearTimeout,
    window: { addEventListener() {} }, document: {}, performance: { now: () => 0 },
    localStorage: { getItem: () => null, setItem() {} },
    $: () => null, escHtml: s => String(s), toast() {},
    App: { R: null },
    GameData: { sig: 1 },
    WgSound: { place: { on: true }, oneShot: d => played.push(d) },
    Engine: {
      async call(c, a) {
        if (c === 'game_settings') return { move: {} };
        if (c === 'sound_records') return [{ id: a.ids[0], file: a.ids[0] + '.wav', gain: 1 }];
        return {};
      },
      async bytes(c, a) {
        // floor.nif: a 2000-unit square at z 100 (a collision shape); grass.nif: no collision.
        const parts = a.meshes.map(m => {
          const tris = /floor/.test(m) ? [[-1000, -1000, 100, 1000, -1000, 100, 1000, 1000, 100], [-1000, -1000, 100, 1000, 1000, 100, -1000, 1000, 100]] : [];
          const shape = /floor/.test(m) ? 0 : 2;
          const b = Buffer.alloc(9 + tris.length * 36);
          b.write('GDCL', 0); b.writeUInt8(shape, 4); b.writeUInt32LE(tris.length, 5);
          tris.flat().forEach((v, i) => b.writeFloatLE(v, 9 + i * 4));
          const p = Buffer.from(m);
          const head = Buffer.alloc(2); head.writeUInt16LE(p.length);
          const len = Buffer.alloc(4); len.writeUInt32LE(b.length);
          return Buffer.concat([head, p, len, b]);
        });
        const n = Buffer.alloc(4); n.writeUInt32LE(parts.length);
        const all = Buffer.concat([Buffer.from('GDCB'), n, ...parts]);
        return all.buffer.slice(all.byteOffset, all.byteOffset + all.length);
      },
      async pick() { return null; },
    },
  };
  vm.createContext(ctx);
  vm.runInContext(src('06_mge.js') + '\n' + src('06_gl.js') + '\n' + src('71_wg_walk.js'), ctx);
  const W = ctx.WgWalk, R = ctx.Renderer;
  t.ok(W && R, 'the body and the renderer are there');

  // Speed 50, Athletics 30, Acrobatics 30, rested, unburdened: Morrowind's settings.
  Object.assign(W.st, { speed: 50, athletics: 30, acrobatics: 30, encumbrance: 0, fatigue: 100, levitate: 10, jump: 0, swiftSwim: 0, footVol: 100 });
  W.gm = Object.assign({}, W.GM);
  const sp = W.speeds();
  const near = (a, b) => Math.abs(a - b) < 0.01;
  t.ok(near(sp.walk, 150) && near(sp.run, 307.5) && near(sp.sneak, 112.5), 'walk, run, sneak: ' + [sp.walk, sp.run, sp.sneak]);
  t.ok(near(sp.swimRun, 307.5 * 0.53) && near(sp.fly, 32), 'swim and levitate: ' + [sp.swimRun, sp.fly]);
  t.ok(near(sp.jump, (128 + 16) * 1.5 * 1.25 / 3 + W.GRAVITY / 3), 'the jump: ' + sp.jump);
  W.st.encumbrance = 110;
  t.ok(W.speeds().run === 0, 'over-encumbered, the body cannot move');
  W.st.encumbrance = 0;

  // The collision world: the floor's shape stops a ray, the grass (no collision) does not.
  const id = (x, y, z) => [1, 0, 0, x, 0, 1, 0, y, 0, 0, 1, z];
  const picks = [{ model: 'floor.nif', m: id(0, 0, 0) }, { model: 'grass.nif', m: id(0, 0, 300) }];
  ctx.App.R = { pickables: picks };
  await W.build(picks, [0, 0]);
  const h = W.ray([0, 0, 500], [0, 0, -1]);
  t.ok(h && near(h.t, 400), 'down onto the floor shape: ' + (h && h.t));
  t.ok(W.ray([5000, 0, 500], [0, 0, -1]) === null, 'nothing where nothing collides');

  // A body on that floor, in deep water, then levitating.
  const body = Object.create(R.prototype);
  Object.assign(body, {
    cam: { tx: 0, ty: 0, tz: 100 + 116, dist: 0.0001, az: Math.PI, el: 0 },
    fly: { keys: {}, vel: [0, 0, 0], last: 0, pad: null }, walkSt: { vz: 0, ground: true },
    groundZ: () => -5000, pickRay: () => null, water: null, walkBody: W, walk: true,
  });
  ctx.App.R = Object.assign(body, { pickables: picks });
  W._picksRef = picks; W._builtAt = [0, 0];
  let now = 1000;
  const run = (n, keys) => { body.fly.keys = keys; for (let i = 0; i < n; i++) { now += 16; body._walkStep(now); } };
  run(1, {});
  run(62, { W: true });   // az = pi: forward is +x
  const e1 = body._eye();
  t.ok(e1[0] > 200 && e1[0] < 320 && near(e1[2], 216), 'about a second of running along the floor: ' + e1.map(Math.round));
  await new Promise(r => setTimeout(r, 5));
  t.ok(played.some(d => /FootBare/.test(d.id || d.file)), 'bare steps heard');

  // Deep water over the floor: swimming; E rises, but the head stays at the surface.
  body.water = { z: 400 };
  played.length = 0;
  run(120, { E: true });
  const e2 = body._eye();
  t.ok(near(e2[2] - 116, 400 - W.g('fSwimHeightScale') * W.BODY_H + 0.5) || e2[2] - 116 <= 400 - 115 + 0.6, 'swimming up stops at the surface: feet ' + Math.round(e2[2] - 116));
  run(100, { W: true });
  await new Promise(r => setTimeout(r, 5));
  t.ok(played.some(d => /Swim/.test(d.id || d.file)), 'swim strokes heard');

  // Levitating out of the water: up past the surface.
  body.water = null;
  W.st.levOn = true;
  run(120, { E: true });
  t.ok(body._eye()[2] > e2[2] + 10, 'levitating rises: ' + Math.round(body._eye()[2]));
  W.st.levOn = false;
  run(400, {});
  t.ok(near(body._eye()[2], 216) && body.walkSt.ground, 'down again onto the floor: ' + Math.round(body._eye()[2]));
};
