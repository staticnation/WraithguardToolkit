/* The Preview's batch of 2026-11-24 (Static: "lets do all this"): camera bookmarks kept per
   cell and the screenshot's clean frame (84_wg_views.js); the occlusion culling's read-back
   per view, so the reflection tests against its own (29_wg_occlusion.js); and the report's
   "no visible geometry" left for meshes that are neither particle emitters nor editor markers
   (10_preview.js `isMarkerMesh`; "the mist and smoke are particle emitters, the other 3 are
   editor markers"). */
'use strict';
const { JSDOM } = require('jsdom');
const fs = require('fs');
const path = require('path');

module.exports = async (t) => {
  const src = f => fs.readFileSync(path.join(__dirname, '..', '..', 'src', f), 'utf8');
  const dom = new JSDOM('<!doctype html><body><div id="wgViews"></div></body>', { runScripts: 'outside-only', pretendToBeVisual: true, url: 'http://tauri.localhost/' });
  const w = dom.window, d = w.document;
  w.eval('var toast=()=>{}; var $=s=>document.querySelector(s); var App={};');
  w.eval(src('38_wg_cellkey.js').replace(/^const CellKey=/m, 'var CellKey='));
  w.eval(src('84_wg_views.js').replace(/^const WgViews=/m, 'var WgViews='));
  const V = w.WgViews, App = w.App;

  // Views: kept per cell, in world units, and put back.
  const R = { cam: { az: 1, el: 0.2, dist: 900, tx: 10, ty: 20, tz: 300 }, nav: 'orbit', walk: false, dirty: false, opts: {},
    setNav(m) { this.nav = m === 'walk' ? 'wasd' : m; this.walk = m === 'walk'; } };
  App.R = R; App.cellSel = { kind: 'ext', gx: -2, gy: -9 }; App._scene = { origin: [1000, 2000] };
  t.ok(V.list().length === 0, 'no views yet');
  V.save('Dock');
  t.ok(V.list().length === 1 && V.list()[0].x === 1010 && V.list()[0].y === 2020, 'saved in world units: ' + JSON.stringify(V.list()[0]));
  t.ok(d.querySelectorAll('#wgViews button').length >= 3 && /Dock/.test(d.getElementById('wgViews').textContent), 'listed on the panel');
  App.cellSel = { kind: 'int', name: 'Arrille\'s Tradehouse' };
  t.ok(V.list().length === 0, 'another cell has its own (none)');
  App.cellSel = { kind: 'ext', gx: -2, gy: -9 };
  App._scene.origin = [0, 0]; R.cam.tx = 0; R.cam.ty = 0; R.cam.az = 0;
  V.go(0);
  t.ok(R.cam.tx === 1010 && R.cam.ty === 2020 && R.cam.az === 1 && R.dirty, 'put back where it stood, whatever the origin is now');
  R.nav = 'wasd'; R.walk = true; V.save('Walk');
  R.nav = 'orbit'; R.walk = false; V.go(1);
  t.ok(R.nav === 'wasd' && R.walk, 'and walking, if it was walking');
  V.remove(0);
  t.ok(V.list().length === 1 && V.list()[0].name === 'Walk', 'forgotten');

  // The clean screenshot leaves out the highlight and the arrows for its frame, then puts them back.
  let seen = null;
  R.hlStatics = ['x']; R.opts.arrows = true;
  R.draw = () => { seen = { hl: R.hlStatics, arrows: R.opts.arrows }; };
  R.cv = { toBlob: cb => cb({ arrayBuffer: async () => new ArrayBuffer(40) }) };
  const png = await V.capture(true);
  t.ok(seen.hl === null && seen.arrows === false && png.length === 40, 'clean: drawn without them');
  t.ok(R.hlStatics[0] === 'x' && R.opts.arrows === true, 'and they are back after');

  // The occlusion's read-back per view: the reflection's is its own.
  const ctx = { Math, Float32Array, Float64Array, Uint8Array, Infinity, isFinite, Object, Array, Renderer: class {} };
  require('vm').createContext(ctx);
  require('vm').runInContext(src('29_wg_occlusion.js').replace(/^const (\w+)=/mg, 'var $1='), ctx);
  const r = Object.create(ctx.Renderer.prototype);
  Object.assign(r, { gl: {}, _groups: {}, _occEdits: 0, _occN: 5, nearFar: () => [16, 400000] });
  const VP = new Float32Array([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, -1, -1, 0, 0, -32, 0]);
  const main = r._occView('main'), refl = r._occView('refl');
  main.src = { w: new Float32Array(1), W: 8, H: 8, VP: Float64Array.from(VP), p10: -1, p14: -32, G: r._groups, edits: 0, f: 4 };
  t.ok(r._occFor(VP, 'main') && !r._occFor(VP, 'refl'), 'the main view\'s read-back does not answer for the reflection');
  refl.src = Object.assign({}, main.src);
  t.ok(r._occFor(VP, 'refl') && r._occFor(VP, 'refl') !== r._occFor(VP, 'main'), 'each view tests against its own');
  r._occEdits = 1;
  t.ok(!r._occFor(VP, 'main'), 'a big thing moved: the read-back is dropped');

  // No visible geometry: not for particle emitters or editor markers.
  const m = src('10_preview.js').match(/function isMarkerMesh[\s\S]*?\n}/)[0];
  const isMarker = new Function(m + '; return isMarkerMesh;')();
  t.ok(['Meshes\\editormarker.nif', 'Meshes\\tr\\tr_editormarker_npc.nif', 'Meshes\\EditorMarker_box_01.NIF', 'meshes/marker_north.nif'].every(isMarker), 'editor markers are known');
  t.ok(!['Meshes\\mist\\xmist.nif', 'Meshes\\chimney_smoke02.NIF', 'Meshes\\x\\ex_wall.nif'].some(isMarker), 'other meshes are not markers');
  t.ok(/!\(rec\.particles && rec\.particles\.length\) && !isMarkerMesh\(rec\.path\)/.test(src('10_preview.js')), 'a mesh with particles, or a marker, is not reported');
};
