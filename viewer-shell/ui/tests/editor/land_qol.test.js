/* The Land view's batch of 2026-11-24 (Static: "lets do all this"; 87_wg_land_qol.js): the
   strokes listed and taken back, the mirror, the road, painting by rules, and the heightmap
   PNG out and back in. Two cells of land, (0, 0) and (1, 0), in the page's own WgLand. */
'use strict';
const { JSDOM } = require('jsdom');
const fs = require('fs');
const path = require('path');

module.exports = async (t) => {
  const src = f => fs.readFileSync(path.join(__dirname, '..', '..', 'src', f), 'utf8');
  const dom = new JSDOM('<!doctype html><body><div id="main"></div></body>', { runScripts: 'outside-only', pretendToBeVisual: true, url: 'http://tauri.localhost/' });
  const w = dom.window;
  for (const k of ['CompressionStream', 'DecompressionStream', 'Response', 'Blob', 'TextEncoder', 'TextDecoder']) w[k] = globalThis[k];
  w.eval('var toast=()=>{}; var $=s=>document.querySelector(s); var escHtml=s=>String(s); var DEFAULT_LTEX="_land_default"; var DEFAULT_LAND_HEIGHT=-2048;' +
    'var BrushSize={pos:()=>50, radius:()=>300}; var GameData={ltex:new Map()}; var App={};');
  w.eval(src('72_wg_landbrush.js').replace(/^const WgLandBrush=/m, 'var WgLandBrush='));
  w.eval(src('67_wg_land.js').replace(/^const WgLand=/m, 'var WgLand='));
  w.eval(src('87_wg_land_qol.js').replace(/^const WgLandQol=/m, 'var WgLandQol='));
  const W = w.WgLand, Q = w.WgLandQol, App = w.App, N = 65;
  W.rebuild = () => {}; W.applyOverlays = () => {}; W.preview = true;
  // Ground rising 8 units a vertex eastward from -200: the west under the sea.
  const cell = gx => { const h = new w.Float32Array(N * N); for (let y = 0; y < N; y++) for (let x = 0; x < N; x++) h[y * N + x] = -200 + (gx * 64 + x) * 8; return { kind: 'ext', gx, gy: 0, heights: h, colours: null, layers: null }; };
  App._scene = { origin: [0, 0], cells: [cell(0), cell(1)] };
  App.cellSel = { kind: 'ext', gx: 0, gy: 0 };
  const cells = W.cells(), C = k => cells.get(k);

  // Reading the ground.
  t.ok(Math.abs(Q.heightAt(cells, 192, 50) - (-188)) < 1e-6, 'between vertices, bilinear: ' + Q.heightAt(cells, 192, 50));
  t.ok(Math.abs(Q.slopeAt(cells, 4000, 4000) - Math.atan(8 / 128) * 180 / Math.PI) < 1e-6, 'the slope in degrees');

  // The mirror.
  W.b.mirror = 'xy';
  const m = Q.mirrored([[4096 + 100, 4096 + 50, 1]]);
  t.ok(m.length === 4 && m.some(d => d[0] === 3996 && d[1] === 4046), 'four ways round the cell\'s middle');
  W.mode = 'height'; W.tool = 'ramp';
  t.ok(Q.mirrored([[1, 2, 1]]).length === 1, 'not the ramp');
  W.b.mirror = 'off';

  // A stroke, kept in the list, and taken back.
  W.tool = 'raise'; W.strength = 50;
  const was = C('0,0').heights[32 * N + 32];
  W.beginStroke(); W.applyDabs([[4096, 4096, 1]]); W.endStroke();
  t.ok(Q.hist.length === 1 && Q.hist[0].label === 'Raise' && C('0,0').heights[32 * N + 32] > was, 'a stroke listed: ' + (Q.hist[0] && Q.hist[0].label));
  t.ok([...Q.hist[0].cells.keys()].join() === '0,0', 'with only the cell it changed');
  W.beginStroke(); W.endStroke();
  t.ok(Q.hist.length === 1, 'a stroke that changed nothing is not listed');

  // The road: level along its line, no steeper than asked, and painted.
  Object.assign(Q.road, { pts: [[1000, 4096], [12000, 4096]], width: 384, shoulder: 256, slope: 0.5, curve: false, level: true, paint: true, tex: 'Road' });
  const e = Q.buildRoad();
  t.ok(e && Q.hist.length === 2 && e.label === 'Road', 'built, one entry');
  const zs = [2000, 6000, 10000].map(x => Q.heightAt(cells, x, 4096));
  const grade = Math.atan(Math.abs(zs[2] - zs[0]) / 8000) * 180 / Math.PI;
  t.ok(grade <= 0.5 + 1e-6, 'held to the steepest slope: ' + grade.toFixed(3) + '°');
  t.ok(W.ids(C('0,0'))[8 * 16 + 4] === 'Road' && W.ids(C('1,0'))[8 * 16 + 4] === 'Road', 'the texture along it, in both cells');
  t.ok(Q.road.pts.length === 0, 'the points used');

  // Back to before the first stroke: both taken back.
  Q.back(0);
  t.ok(Q.hist.length === 0 && C('0,0').heights[32 * N + 32] === was && W.ids(C('0,0'))[8 * 16 + 4] === '_land_default', 'the ground as it was, the texture too');

  // Painting by rules: sand by the water, rock where steep, grass the rest.
  C('1,0').heights[10 * N + 12] += 600; // a spike: steep round it
  W.b.rules = [{ tex: 'Sand', hmin: null, hmax: 64, water: 'near', wdist: 1024 }, { tex: 'Rock', smin: 35 }, { tex: 'Grass' }];
  W.b.rulesWhere = 'all';
  Q.paintRules();
  const id = (k, i, j) => W.ids(C(k))[j * 16 + i];
  t.ok(id('0,0', 0, 0) === 'Sand' && id('0,0', 15, 8) === 'Grass' && id('1,0', 15, 8) === 'Grass', 'sand at the water, grass inland');
  t.ok(id('1,0', 2, 2) === 'Rock', 'rock on the spike');
  Q.back(0);

  // The heightmap: out as a 16-bit PNG, read back, put on.
  const ex = await Q.exportPng();
  const img = await Q.decodePng(ex.png);
  t.ok(img.w === 129 && img.h === 65 && JSON.parse(img.text.Wraithguard).cols === 2, 'two cells, 129 x 65, with its range: ' + ex.name);
  const before = Float32Array.from(C('1,0').heights);
  for (const c of cells.values()) c.heights.fill(0);
  Q.applyImport(img, ex.min, ex.max, 1);
  let worst = 0; for (let i = 0; i < before.length; i++) worst = Math.max(worst, Math.abs(before[i] - C('1,0').heights[i]));
  t.ok(worst < 0.2, 'the heights back within a fraction of a unit: ' + worst.toFixed(3));
  // An 8-bit RGB PNG is read as its grey.
  const enc = Q.encodePng.bind(Q);
  const rgbRaw = new Uint8Array([0, 255, 0, 0, 0, 0, 255]);
  const z = await Q.zlib(rgbRaw, false);
  const chunk = (type, data) => { const b = new Uint8Array(12 + data.length), dv = new DataView(b.buffer); dv.setUint32(0, data.length); b.set(new TextEncoder().encode(type), 4); b.set(data, 8); dv.setUint32(8 + data.length, Q.crc(b.subarray(4, 8 + data.length))); return b; };
  const ih = new Uint8Array(13); new DataView(ih.buffer).setUint32(0, 2); new DataView(ih.buffer).setUint32(4, 1); ih[8] = 8; ih[9] = 2;
  const parts = [new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10]), chunk('IHDR', ih), chunk('IDAT', z), chunk('IEND', new Uint8Array(0))];
  const rgb = new Uint8Array(parts.reduce((s, p) => s + p.length, 0)); let o = 0; for (const p of parts) { rgb.set(p, o); o += p.length; }
  const g = await Q.decodePng(rgb);
  t.ok(Math.abs(g.v[0] - 0.2126) < 1e-3 && Math.abs(g.v[1] - 0.0722) < 1e-3, 'color read as luminance: ' + Array.from(g.v).map(v => v.toFixed(3)));
  void enc;

  // The panel: ours in the dock, each for its modes.
  w.eval('var WgEditor={ask:async()=>[]};');
  W.mode = 'height'; W.panel();
  const d = w.document, shown = id => d.getElementById(id) && d.getElementById(id).style.display !== 'none';
  t.ok(['wgLandMirror', 'wgLandRoad', 'wgLandHmap', 'wgLandHist'].every(shown) && !shown('wgLandRules'), 'height: mirror, road, heightmap, strokes');
  t.ok(d.getElementById('wgLandHist').nextElementSibling.id === 'wgLandPatchS', 'above In the patch');
  d.querySelector('#wgLandModes [data-m="texture"]').click();
  await new Promise(r => setTimeout(r, 5));
  t.ok(shown('wgLandRules') && shown('wgLandRoad') && !shown('wgLandHmap'), 'texture: the rules and the road');
  t.ok(d.querySelectorAll('#wgLandRuleList > div').length === 3, 'the three rules drawn');
  d.querySelector('#wgLandModes [data-m="color"]').click();
  d.getElementById('wgLandColPick').click();
  t.ok(Q._pick === 'color', 'the color pick armed');
  t.ok(Q.click(4096, 4096) && /^#[0-9a-f]{6}$/.test(W.color) && Q._pick === null, 'and taken: ' + W.color);
};
