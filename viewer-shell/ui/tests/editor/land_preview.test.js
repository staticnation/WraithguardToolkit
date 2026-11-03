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
};
