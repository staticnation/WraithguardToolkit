/* A cell the patch makes itself: it is in the Cell View, shows the references the patch
   places in it, and takes placement - none of which the engine can answer for (it has only
   the loaded world's cells). */
'use strict';
const { page, EDITOR } = require('./harness');

module.exports = async (t) => {
  let pending = [{ tag: 'CELL', id: 'New room', made: { name: 'New room', mesh: '' } }];
  const engineCalls = [];
  const p = page({
    modules: EDITOR,
    links: { editRecord: 'r', editPending: 'pend', editPlace: 'place', editNewCell: 'nc', editNewLand: 'nl' },
    engine: (c, args) => {
      engineCalls.push([c, args]);
      if (c === 'editor_tags') return [['STAT', 1]];
      if (c === 'cells') return { grid: [], interiors: [['Old room', 3, false]] };
      if (c === 'editor_cell_refs') throw new Error(String(args.cell).includes(',') ? 'no exterior cell ' + args.cell : 'No interior called ' + String(args.cell));
      return {};
    },
    answer: (url, body) => {
      if (url === 'pend') return pending;
      if (url === 'nl') return { cell: body.cell, land: true, madeCell: true };
      if (url === 'place') return { cell: 'new room', uid: 'u9', tag: body.tag, id: body.id, new: true, plugins: [], fields: [] };
      return {};
    },
  });
  const { w, d } = p;
  const E = w.WgEditor;
  w.App = w.App || {};
  await E.enter();
  await p.wait(40);
  const names = () => [...d.querySelectorAll('#edCellList tr[data-cell]')].map(tr => tr.dataset.cell);
  t.ok(names().includes('int:New room'), 'the patch\'s new interior is in the Cell View: ' + names());
  t.ok(names().includes('int:Old room'), 'and the loaded world\'s cells still are');
  const row = d.querySelector('#edCellList tr[data-cell="int:New room"]');
  t.ok(row && /edited/.test(row.className), 'it is marked as the patch\'s');

  // Its references are the patch's own: the engine is not asked for them.
  pending = [...pending,
    { tag: '', id: null, new: { cell: 'New room', uid: 'u1', tag: 'STAT' }, changes: [{ path: 'id', value: 'rock' }, { path: 'translation', value: [1, 2, 3] }] }];
  await E.refreshPending();
  const before = engineCalls.filter(c => c[0] === 'editor_cell_refs').length;
  await E.loadRefs('int:New room');
  t.ok(engineCalls.filter(c => c[0] === 'editor_cell_refs').length === before, 'no engine request for a cell the engine has not got');
  t.ok(E.refs.length === 1 && E.refs[0][1] === 'rock' && E.refs[0][0] === 'new:u1', 'its references are the patch\'s: ' + JSON.stringify(E.refs));
  t.ok(d.querySelector('#edCellName').textContent.includes('New room'), 'its name is on the list');
  t.ok(d.querySelector('#edCellList tr[data-cell="int:New room"] td.num').textContent === '1', 'the count is the patch\'s references');
  // A cell of the load order is still the engine's.
  await E.loadRefs('int:Old room');
  t.ok(engineCalls.filter(c => c[0] === 'editor_cell_refs').length === before + 1, 'a cell of the load order is asked of the engine');

  // The render window's record for it: an empty room, then the patch's reference is overlaid.
  const rec = E.emptyCell({ kind: 'int', name: 'New room' });
  t.ok(rec.refs.length === 0 && !rec.hasLand && rec.name === 'New room', 'an empty room');
  t.ok(E.pendingCell('int:new room') && E.pendingCell({ kind: 'int', name: 'New room' }) && !E.pendingCell('int:Old room'), 'which cells are the patch\'s');

  // Placement: into the new cell, the engine not asked, the cell key as the patch has it.
  w.App.cellSel = { kind: 'int', name: 'New room', label: 'New room' };
  w.App._scene = { origin: [0, 0] };
  w.App.R = { cam: { tx: 0, ty: 0, tz: 0 } };
  const asked = engineCalls.filter(c => c[0] === 'editor_cell_refs').length;
  const v = await E.placeAt({ tag: 'STAT', id: 'rock', model: 'x\\rock.nif' }, null, null, [5, 6, 7]);
  t.ok(v && v.uid === 'u9', 'the placement went through');
  const post = p.posts.filter(x => x[0] === 'place').pop();
  t.ok(post && post[1].cell === 'New room' && post[1].plugins === null, 'editPlace names the patch\'s cell: ' + JSON.stringify(post && post[1]));
  t.ok(engineCalls.filter(c => c[0] === 'editor_cell_refs').length === asked, 'placing did not ask the engine for the cell');

  // A new exterior square, by its grid.
  pending = [{ tag: 'CELL', id: '(40, -12)', made: { name: '', mesh: '' } }];
  await E.refreshPending();
  t.ok(names().includes('40,-12'), 'a new exterior square is listed: ' + names());
  t.ok(E.pendingCell('40,-12') && E.pendingKey(E.pendingCell('40,-12')) === '(40, -12)', 'and keyed as the patch keys it');

  // Flat land: asked for as a square, the patch's land and cell then listed; its tab is not a record type's.
  const WF = w.WgFlow;
  const r = await WF.newLand('(7, -3)');
  t.ok(r && r.land && p.posts.some(x => x[0] === 'nl' && x[1].cell === '(7, -3)'), 'newLand asks the patch for the square: ' + JSON.stringify(p.posts.filter(x => x[0] === 'nl')));
  const body = p.posts.filter(x => x[0] === 'nl').pop()[1];
  t.ok(body.noLand === true && body.hasCell === false, 'the viewer says no land and no cell, so Python need not scan: ' + JSON.stringify(body));
  await WF.newLand('7,-3');
  t.ok(p.posts.filter(x => x[0] === 'nl').pop()[1].cell === '(7, -3)', 'a Cell View key is a square too');
  pending = [{ tag: 'CELL', id: '(7, -3)', made: {} }, { tag: 'LAND', id: '(7, -3)', made: {} }];
  await E.refreshPending();
  const tabs = [...d.querySelectorAll('#edTabs [data-tag]')].map(b => b.dataset.tag);
  t.ok(!tabs.includes('CELL') && !tabs.includes('LAND'), 'the patch\'s cells and land are not Object Window tabs: ' + tabs);

  // Placing on a square with land but no cell record (the open sea): the patch makes the cell, then places.
  pending = [];
  w.App.cellSel = { kind: 'ext', x: 3, y: 38 };
  const nPost = p.posts.length;
  const sea = await E.placeAt({ tag: 'MISC', id: 'gold_100', model: 'm\\gold.nif' }, null, null, [4 * 8192 + 10, 38 * 8192 + 10, 0]);
  const sent = p.posts.slice(nPost).map(x => x[0] + ':' + (x[1].cell || ''));
  t.ok(sent[0] === 'nc:(4, 38)' && sent.includes('place:(4, 38)'), 'the cell is made, then the object placed in it: ' + sent.join());
  t.ok(sea && sea.uid === 'u9', 'and the placement went through');

  // New interior from the Cell View: the viewer's own list settles whether the name is free.
  const say = (name) => { w.WgUI.askText = async () => name; };
  say('Brand new room');
  await WF.newCell(false);
  const nc = p.posts.filter(x => x[0] === 'nc').pop();
  t.ok(nc && nc[1].cell === 'Brand new room' && nc[1].absent === true, 'a free name is sent with absent, so Python does not scan every plugin: ' + JSON.stringify(nc && nc[1]));
  const before2 = p.posts.filter(x => x[0] === 'nc').length;
  say('old ROOM');
  await WF.newCell(false);
  t.ok(p.posts.filter(x => x[0] === 'nc').length === before2, 'a name the load order has is refused without a request');
};
