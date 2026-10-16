/* The render window's read of a cell the patch makes itself (17_cell.js `loadCell`): the
   engine has no such cell, so it is an empty room (or a square with no land), and the
   patch's own references are overlaid on it. */
'use strict';
const { page, EDITOR } = require('./harness');

module.exports = async (t) => {
  const asked = [];
  const p = page({
    modules: ['src/17_cell.js', ...EDITOR],
    links: { editPending: 'pend', editRecord: 'r', editRevision: 'rev' },
    engine: (c, a) => {
      asked.push(c);
      if (c === 'editor_tags') return [['STAT', 1]];
      if (c === 'interior_data' || c === 'cell_data') throw new Error('the engine has no such cell');
      return {};
    },
    answer: (u) => u === 'pend' ? [
      { tag: 'CELL', id: 'New room', made: {} },
      { tag: 'CELL', id: '(40, -12)', made: {} },
      { new: { cell: 'New room', uid: 'u1', tag: 'STAT' }, changes: [{ path: 'id', value: 'rock' }, { path: 'translation', value: [1, 2, 3] }] },
    ] : {},
  });
  const w = p.w;
  w.GameData = { pluginLtexIx: [] }; w.worldToken = () => 1;
  await w.WgEditor.refreshPending();

  const room = await w.CellData.loadCell({ kind: 'int', name: 'New room' });
  t.ok(room.name === 'New room' && room.refs.length === 1, 'an empty room with the patch\'s reference: ' + room.refs.length);
  t.ok(room.refs[0].id === 'rock' && room.refs[0].pos.join() === '1,2,3', 'the reference is where the patch put it');
  t.ok(!asked.includes('interior_data'), 'the engine was not asked for the room');

  const sq = await w.CellData.loadCell({ kind: 'ext', x: 40, y: -12 });
  t.ok(sq.heights === null && sq.refs.length === 0 && !asked.includes('cell_data'), 'a new exterior square: no land, the engine not asked');

  let failed = null;
  try { await w.CellData.loadCell({ kind: 'int', name: 'Elsewhere' }); } catch (e) { failed = e; }
  t.ok(failed && asked.includes('interior_data'), 'a cell the patch did not make is still the engine\'s');

  // An empty square opened from the Cell map: the engine has nothing, so it is drawn empty - in the Editor only.
  w.WgEditor.cells = [{ kind: 'ext', x: 1, y: 1, name: '', label: 'x', refs: 0, path: false }];
  asked.length = 0;
  const failing = w.Engine.call;
  w.Engine.call = async (n, ...r) => { if (n === 'cell_data') { asked.push(n); throw new Error('no such cell'); } return failing.call(w.Engine, n, ...r); };
  const open = await w.CellData.loadCell({ kind: 'ext', x: 77, y: 5 });
  t.ok(open.refs.length === 0 && open.heights === null, 'an empty square opens empty in the editor');
  let known = null;
  try { await w.CellData.loadCell({ kind: 'ext', x: 1, y: 1 }); } catch (e) { known = e; }
  t.ok(known, 'a square the engine lists that fails is still an error');
};
