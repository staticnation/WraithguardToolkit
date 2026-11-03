/* The Land view's brush engine (72_wg_landbrush.js) and its overlays (67_wg_land.js): the
   falloffs and shapes, the stroke (a click, the spacing along a drag, the hold, the fade,
   the cap), the height tools on a small field, and the hillshade's lights and the
   contours' round interval. */
'use strict';
const vm = require('vm');
const fs = require('fs');
const path = require('path');
const src = f => fs.readFileSync(path.join(__dirname, '..', '..', 'src', f), 'utf8').replace(/^const (\w+)=/mg, 'var $1=');

module.exports = async (t) => {
  const ctx = { Math, Map, Set, Object, isFinite };
  vm.createContext(ctx);
  vm.runInContext(src('72_wg_landbrush.js'), ctx);
  const B = ctx.WgLandBrush;
  const near = (a, b, e) => Math.abs(a - b) < (e || 1e-6);

  // Falloffs: 1 inside the feather, 0 at the edge, each its own shape between.
  for (const [k] of B.FALLOFFS) {
    t.ok(near(B.weight(k, 0, 100, 50), 1) && B.weight(k, 100.0001, 100, 50) === 0, k + ' is 1 inside and 0 past the edge');
  }
  t.ok(B.weight('linear', 75, 100, 50) > B.weight('sharp', 75, 100, 50), 'linear holds more than sharp halfway out');
  t.ok(near(B.weight('flat', 99, 100, 50), 1), 'flat is a hard edge');
  // Shapes: a square reaches its corner, a circle does not; a blade is narrow across its angle.
  t.ok(B.dist(90, 90, { shape: 'square' }) <= 100 && B.dist(90, 90, { shape: 'circle' }) > 100, 'a square brush reaches its corners');
  t.ok(B.dist(0, 50, { shape: 'circle', aspect: 0.25, angle: 0 }) > 100 && B.dist(80, 0, { shape: 'circle', aspect: 0.25, angle: 0 }) <= 100, 'a narrow blade along its angle');

  // The stroke: the click, then a dab each spacing along the path.
  const S = B.begin(0, { spacing: 25, click: 50, drag: 100 });
  let d = B.along(S, 0, 0, 100, 0);
  t.ok(d.length === 1 && near(d[0][2], 0.5), 'a click is one dab at the click intensity');
  d = B.along(S, 100, 0, 100, 16);
  t.ok(d.length === 4 && near(d[0][0], 25) && near(d[3][0], 100), 'a drag lays a dab every 25 units: ' + d.map(x => Math.round(x[0])));
  t.ok(B.hold(S, 100, 0.016) === 0, 'no hold while moving');
  S.s.rate = 10;
  t.ok(near(B.hold(S, 1000, 0.1), 1), 'held still: a second\'s rate, a frame at a time');
  // Decay halves at its half-life; the fade runs out over its length.
  const D = B.begin(0, { decay: 2 });
  t.ok(near(B.envelope(D, 2000), 0.5), 'the decay\'s half-life');
  const F = B.begin(0, { spacing: 10, fadeLen: 1000 });
  B.along(F, 0, 0, 100, 0); const f1 = B.along(F, 500, 0, 100, 10);
  t.ok(near(f1[f1.length - 1][2], 0.5, 0.02), 'halfway along the fade length, half the strength: ' + f1[f1.length - 1][2]);
  F.s.fadeRepeat = 'tri'; F.dist = 1500;
  t.ok(near(B.fade(F), 0.5), 'a triangle fade comes back up');
  // The cap: no vertex further than it in one stroke.
  const C = B.begin(0, { cap: 10 });
  t.ok(B.capped(C, 'a', 0, 7) === 7 && B.capped(C, 'a', 7, 30) === 10 && B.capped(C, 'a', 10, -50) === -10, 'the stroke cap holds a vertex within it');

  // The tools on a 9 x 9 field: a spike, a slope, a plain.
  const field = (fn) => { const h = new Map(); for (let y = -4; y <= 4; y++) for (let x = -4; x <= 4; x++) h.set(x + ',' + y, fn(x, y)); return { h, get: (x, y) => h.has(x + ',' + y) ? h.get(x + ',' + y) : null }; };
  const one = (x, y) => [x, y, 1];
  const apply = (F, verts, p) => { const out = B.dab(F, verts, Object.assign({ step: 128, k: 1, amount: 10 }, p)); for (let i = 0; i < out.length; i += 3) F.h.set(out[i] + ',' + out[i + 1], out[i + 2]); return F; };
  let F2 = field((x, y) => (x || y) ? 0 : 100);
  apply(F2, one(0, 0), { tool: 'smooth', smooth: 'average' });
  t.ok(near(F2.get(0, 0), 0), 'smoothing a spike levels it with its neighbours');
  F2 = field((x, y) => (x || y) ? 0 : 100);
  apply(F2, one(0, 0), { tool: 'sharpen', k: 0.5 });
  t.ok(F2.get(0, 0) > 100, 'sharpening a spike makes it stand out more');
  F2 = field(() => 50);
  apply(F2, [...one(0, 0), ...one(1, 0)], { tool: 'flatten', target: 100, only: 'down' });
  t.ok(F2.get(0, 0) === 50, 'flatten only down leaves what is under the target');
  apply(F2, one(0, 0), { tool: 'flatten', target: 100, only: 'up' });
  t.ok(F2.get(0, 0) === 100, 'flatten only up fills to it');
  F2 = field(() => 300);
  apply(F2, one(0, 0), { tool: 'terrace', terrace: 128, edge: 0 });
  t.ok(F2.get(0, 0) === 256, 'a terrace drops to its tread');
  F2 = field(x => x * 400);
  const before = F2.get(0, 0) - F2.get(-1, 0);
  apply(F2, one(0, 0), { tool: 'erode', talus: 30 });
  t.ok(F2.get(0, 0) - F2.get(-1, 0) < before, 'erosion takes some of a steep step away');
  F2 = field(() => 0);
  apply(F2, [...one(0, 0), ...one(2, 0)], { tool: 'ramp', ramp: { x0: 0, y0: 0, z0: 0, x1: 512, y1: 0, z1: 400 }, radius: 256, feather: 0, falloff: 'smooth' });
  t.ok(near(F2.get(2, 0), 200), 'halfway up the ramp: ' + F2.get(2, 0));
  const n1 = B.fbm(1000, 2000, { scale: 512, octaves: 4, seed: 3 }), n2 = B.fbm(1000, 2000, { scale: 512, octaves: 4, seed: 3 });
  t.ok(n1 === n2 && n1 >= -1 && n1 <= 1 && n1 !== B.fbm(1000, 2000, { scale: 512, octaves: 4, seed: 4 }), 'noise is the same for a seed and another for another');

  // The overlays: the lights' weights sum to one, the contours a round dozen.
  const W = {};
  const pctx = { Math, Map, Set, Object, isFinite, JSON, WgLandBrush: B, BrushSize: { pos: () => 50, radius: () => 400 }, App: { R: { opts: {}, dirty: false } },
    localStorage: { getItem: () => null, setItem() {} }, $: () => null, toast() {}, navigator: { userAgent: 'test' } };
  vm.createContext(pctx);
  vm.runInContext(src('67_wg_land.js'), pctx);
  const L = pctx.WgLand;
  L.on = true;
  L.cells = () => new Map([['0,0', { heights: Float32Array.from({ length: 65 * 65 }, (_, i) => (i % 65) * 20) }]]);
  Object.assign(L.ov, { hill: 0.6, lights: 6, az: 225, alt: 45, tint: 'rainbow', tintA: 55, contours: true, interval: 0 });
  L.applyOverlays();
  const o = pctx.App.R.opts;
  const ws = [0, 1, 2, 3, 4, 5].map(i => o.hillLights[i * 4 + 3]);
  t.ok(near(ws.reduce((a, b) => a + b, 0), 1) && ws[0] === Math.max(...ws), 'six lights, the azimuth\'s strongest, weights summing to one');
  t.ok(near(Math.hypot(o.hillLights[0], o.hillLights[1], o.hillLights[2]), 1) && o.hillLights[0] < 0 && o.hillLights[1] < 0, 'the light from the south-west, a unit vector');
  t.ok(o.elev === 2 && near(o.elevA, 0.55) && o.contour === 100, 'rainbow at 55 %, a line every 100 over 1280 units: ' + o.contour);
  // The ring: the renderer hands the ground shader the brush's shape, aspect and angle.
  const rctx = { console, Math, window: { addEventListener() {} }, document: {}, performance: { now: () => 0 } };
  vm.createContext(rctx);
  vm.runInContext(src('06_mge.js').replace(/^class (\w+)/mg, 'var $1=class $1') + '\n' + src('06_gl.js').replace(/^class (\w+)/mg, 'var $1=class $1'), rctx);
  const got = [];
  const fake = { brush: { r: 400, inner: 200, shape: 'square', aspect: 0.5, angle: 90 }, progGround: { u: { uBrushA: 1 } }, gl: { uniform2f: (u, a, b) => got.push(a, b) } };
  const bs = rctx.Renderer.prototype._brushS.call(fake);
  t.ok(bs.join() === '2,0.5,400,200' && near(got[0], 0) && near(got[1], 1), 'a square ring, half as wide, turned 90 degrees: ' + bs + ' ' + got);
  fake.brush = { r: 100, inner: 50 };
  t.ok(rctx.Renderer.prototype._brushS.call(fake)[0] === 0, 'a brush without a shape keeps the sphere');
  L.applyOverlays(true);
  t.ok(o.hill === 0 && o.elev === 0 && o.contour === 0, 'off outside the Land view');
};
