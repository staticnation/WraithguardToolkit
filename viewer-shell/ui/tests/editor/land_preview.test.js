/* The Land view's preview (67_wg_land.js, "Preview first"): a stroke is drawn but not
   written; Drop puts the ground back and writes nothing, Keep writes it, Ctrl+Z drops it,
   and a stroke waiting on a cell the scene builds again (a cell arrow) is carried onto the
   new cell, so Keep writes the stroke and not the load order's ground. */
'use strict';
const { page, EDITOR } = require('./harness');

module.exports = async (t) => {
  const p = page({
    modules: EDITOR,
    links: { editRecord: 'r', editPending: 'pend' },
    engine: (c) => c === 'editor_tags' ? [] : c === 'editor_records' ? { plugins: [], rows: [] }
      : c === 'cells' ? { grid: [], count: 0, interiors: [], interiorCount: 0 } : {},
    answer: (url) => url === 'pend' ? [] : {},
  });
  const { w, d } = p;
  w.eval(`var BrushSize={pos:()=>50, radius:()=>600}; var DEFAULT_LTEX='_land_default'; var GameData={};`);
  w.eval(require('./harness').source('src/72_wg_landbrush.js'));
  w.eval(require('./harness').source('src/67_wg_land.js'));
  const L = w.WgLand;
  t.ok(L, 'WgLand is there');

  const asked = [];
  w.WgEditor.ask = async (name, body) => { asked.push([name, body]); return /Pending$/.test(name) ? [] : {}; };
  w.WgEditor.refreshPending = async () => {};
  w.WgEditor.canEdit = () => true;
  const rebuilt = [];
  L.rebuild = keys => rebuilt.push([...keys]);
  // Static, 2026-11-02: new land beside a square with none threw "Cannot read properties of
  // null (reading '63')" here. That square's ground is the flat default under the sea.
  {
    w.eval('var DEFAULT_LAND_HEIGHT=-2048;');
    const N0 = 65;
    const lone = { kind: 'ext', gx: 5, gy: 5, heights: new w.Float32Array(N0 * N0).fill(-2048) };
    const empty = { kind: 'ext', gx: 6, gy: 5, heights: null };
    let threw = null;
    try { L.renormal(lone, new w.Map([['5,5', lone], ['6,5', empty]])); } catch (e) { threw = e; }
    t.ok(!threw && lone.normals && Math.abs(lone.normals[(32 * N0 + 64) * 3 + 2] - 1) < 1e-6, 'land beside an empty square: lit flat, no error (' + (threw && threw.message) + ')');
    L.newLand({ kind: 'ext', gx: 7, gy: 5, heights: null, layers: [] });
    t.ok(L._edited.get('7,5').heights.every(h => h === -2048), 'new land starts at the empty square\'s own ground, -2048');
    L._edited.delete('7,5'); L.dirty && L.dirty.delete && L.dirty.delete('7,5');
  }
  L.renormal = () => {};
  const N = 65;
  const cell = () => ({ kind: 'ext', gx: 0, gy: 0, heights: new w.Float32Array(N * N), normals: new w.Float32Array(N * N * 3), colours: null, layers: [], _landIds: [] });
  w.App._scene = { origin: [0, 0], cells: [cell()], chunks: [] };

  L.preview = true; L.mode = 'height'; L.tool = 'raise'; L.strength = 40; L.feather = 0;
  L.panel();
  const stroke = () => L.paint([4096, 4096, 0]);
  const at = () => w.App._scene.cells[0].heights[32 * N + 32];
  const wrote = () => asked.filter(a => a[0] === 'editLandSet').length;

  stroke();
  t.ok(at() > 0, 'the stroke is drawn: ' + at());
  t.ok(L.waiting() && d.getElementById('wgLandWait').style.display === '', 'Keep and Drop are shown');
  L.scheduleCommit();
  await p.wait(800);
  t.ok(wrote() === 0, 'nothing is written while it waits');

  d.getElementById('wgLandDrop').click();
  t.ok(at() === 0, 'Drop puts the ground back');
  t.ok(!L.waiting() && d.getElementById('wgLandWait').style.display === 'none', 'nothing waits after Drop');
  t.ok(wrote() === 0, 'Drop writes nothing');

  stroke();
  await L.step('undo');
  t.ok(at() === 0 && !L.waiting(), 'Ctrl+Z with a stroke waiting drops it');

  stroke();
  const h = at();
  // The scene built again round the same cell: a new cell object with the load order's land.
  const fresh = cell();
  w.App._scene = { origin: [0, 0], cells: [fresh], chunks: [] };
  t.ok(L.apply(fresh, [fresh]) === true && fresh.heights[32 * N + 32] === h, 'the waiting stroke is carried onto the new cell');
  d.getElementById('wgLandKeep').click();
  await p.wait(10);
  const set = asked.find(a => a[0] === 'editLandSet');
  t.ok(set && set[1].cell === '(0, 0)' && set[1].heights[32 * N + 32] === Math.round(h), 'Keep writes the stroke');
  t.ok(!L.waiting(), 'nothing waits after Keep');

  // Static, 2026-11-03: "trying to edit land in the ocean did nothing with the brush". A
  // scene of open sea (no square with land): the stroke makes land there and raises it.
  const sea = { kind: 'ext', gx: 3, gy: 3, heights: null, layers: [] };
  w.App._scene = { origin: [0, 0], cells: [sea], chunks: [] };
  L.paint([3 * 8192 + 4096, 3 * 8192 + 4096, 0]);
  t.ok(sea.heights && sea.heights[32 * N + 32] > -2048 && sea.heights[0] === -2048, 'open sea: new land, raised under the brush: ' + (sea.heights && sea.heights[32 * N + 32]));
  t.ok(L.waiting(), 'and waiting for Keep');

  // Kept: the square's New land first, at the sea floor, then its heights. A square a stroke
  // reached without the mark (written while its New land was) gets it on the engine's word.
  const made = new Set();
  w.WgEditor.ask = async (name, body) => {
    asked.push([name, body]);
    if (name === 'editNewLand') { made.add(body.cell); return { cell: body.cell, land: true }; }
    if (name === 'editLandSet' && !made.has(body.cell)) throw new Error(body.cell + ' has no land to shape: make land there first (New land)');
    return /Pending$/.test(name) ? [] : {};
  };
  d.getElementById('wgLandKeep').click();
  await p.wait(20);
  const nl = asked.filter(a => a[0] === 'editNewLand').pop();
  t.ok(nl && nl[1].cell === '(3, 3)' && nl[1].height === -2048, 'New land at the sea floor: ' + JSON.stringify(nl && nl[1]));
  t.ok(asked.some(a => a[0] === 'editLandSet' && a[1].cell === '(3, 3)') && !L.waiting(), 'then its heights');
  const sea2 = { kind: 'ext', gx: 4, gy: 3, heights: new w.Float32Array(N * N).fill(-2048), layers: [] };
  w.App._scene = { origin: [0, 0], cells: [sea2], chunks: [] };
  L.paint([4 * 8192 + 4096, 3 * 8192 + 4096, 0]);
  d.getElementById('wgLandKeep').click();
  await p.wait(20);
  t.ok(made.has('(4, 3)') && asked.filter(a => a[0] === 'editLandSet' && a[1].cell === '(4, 3)').length === 2, 'no land in the patch yet: made, then shaped');

  // A texture painted over open sea: the square's land made, then the texture written.
  const sea3 = { kind: 'ext', gx: 5, gy: 3, heights: null, layers: [] };
  w.App._scene = { origin: [0, 0], cells: [sea3], chunks: [] };
  L.relayer = () => {};
  L.mode = 'texture'; L.texture = 'Grass';
  w.WgEditor.ask = async (name, body) => {
    asked.push([name, body]);
    if (name === 'editNewLand') { made.add(body.cell); return { cell: body.cell, land: true }; }
    if (name === 'editLandTextures' && !made.has(body.cell)) throw new Error(body.cell + ' has no land to shape: make land there first (New land)');
    return name === 'editLandTextures' ? { textures: 1 } : /Pending$/.test(name) ? [] : {};
  };
  L.paint([5 * 8192 + 4096, 3 * 8192 + 4096, 0]);
  d.getElementById('wgLandKeep').click();
  await p.wait(20);
  const tex = asked.filter(a => a[0] === 'editLandTextures' && a[1].cell === '(5, 3)').pop();
  t.ok(made.has('(5, 3)') && tex && tex[1].ids.includes('Grass') && tex[1].ids.filter(x => x === 'Grass').length < 256, 'texture over the sea: land made, the brush\'s texture cells written: ' + (tex && tex[1].ids.filter(x => x === 'Grass').length));

  // Flatten to the sea floor: the ground under the brush goes down toward -2048.
  const hill = { kind: 'ext', gx: 6, gy: 3, heights: new w.Float32Array(N * N).fill(500), normals: new w.Float32Array(N * N * 3), layers: [], _landIds: [] };
  w.App._scene = { origin: [0, 0], cells: [hill], chunks: [] };
  L.mode = 'height'; L.tool = 'flatten'; L.b.flatTarget = 'floor'; L.strength = 100; L._stroke = null;
  L.paint([6 * 8192 + 4096, 3 * 8192 + 4096, 0]);
  const mid = hill.heights[32 * N + 32];
  t.ok(mid < 500 && mid >= -2048, 'Level to Sea floor flattens toward -2048: ' + mid);
  t.ok(d.querySelector('[data-s="flatTarget"] option[value="floor"]'), 'and it is a choice in Level to');

  // Static, 2026-11-03: hovering a land texture shows it - its swatch tiled, its file and size.
  w.eval(`GameData.ltex=new Map([['gl_grass_03',{id:'GL_Grass_03',file:'gl_grass_03.dds'}]]);
    GameData.textureFileFor=id=>id==='GL_Grass_03'? 'gl_grass_03.dds' : null;
    var loadTexture=async f=>({thumb:'data:image/png;base64,AAAA', w:256, h:128});`);
  const f = d.getElementById('wgLandTexF');
  f.value = 'grass_03'; f.dispatchEvent(new w.Event('input'));
  const row = d.querySelector('#wgLandTexList [data-id="GL_Grass_03"]');
  t.ok(row, 'the texture is listed');
  row.dispatchEvent(new w.Event('mouseenter'));
  await p.wait(5);
  const tip = d.getElementById('wgLandTexTip');
  t.ok(tip && !tip.hidden && /AAAA/.test(tip.querySelector('.sw').style.backgroundImage), 'the hover card shows the swatch: ' + (tip && tip.querySelector('.sw').style.backgroundImage));
  t.ok(/GL_Grass_03/.test(tip.textContent) && /gl_grass_03\.dds\s+256 x 128/.test(tip.textContent), 'its name, file and size: ' + tip.textContent);
  row.dispatchEvent(new w.Event('mouseleave'));
  t.ok(tip.hidden, 'and leaves with the pointer');
};
