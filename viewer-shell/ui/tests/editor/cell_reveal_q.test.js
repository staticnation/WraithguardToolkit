/* A cell the patch has just made is shown (filter off, list long enough, row selected), and Q
   opens the menu for an object picked by a box or Ctrl+click - a selection of one. */
'use strict';
const { page, EDITOR } = require('./harness');

module.exports = async (t) => {
  const interiors = [];
  for (let i = 0; i < 600; i++) interiors.push(['Room ' + String(i).padStart(3, '0'), 1, false]);
  const pending = [{ tag: 'CELL', id: 'Zzz new room', made: { name: 'Zzz new room', mesh: '' } }];
  const made = (id, grid) => ({ tag: 'CELL', id, made: { name: grid ? '' : id, mesh: '' } });
  const p = page({
    modules: EDITOR,
    links: { editRecord: 'r', editPending: 'pend', editNewCell: 'nc', editDuplicateCell: 'dup', editNewLand: 'nl' },
    engine: (c) => {
      if (c === 'editor_tags') return [['STAT', 1]];
      if (c === 'cells') return { grid: [], interiors };
      return {};
    },
    answer: (url, body) => {
      if (url === 'pend') return pending;
      if (url === 'dup') { pending.push(made(body.newName)); return { cell: body.newName, refs: 2, pathgrid: false }; }
      if (url === 'nl') { pending.push(made('(90, 90)', true)); return { cell: body.cell, land: true, madeCell: true }; }
      return {};
    },
  });
  const { w, d } = p;
  const E = w.WgEditor;
  w.App = w.App || {};
  await E.enter();
  await p.wait(40);
  E.cellSort = { col: 0, dir: 1 };
  E.cellFilter = 'room';
  E.drawCells();
  t.ok(!d.querySelector('#edCellList tr[data-cell="int:Zzz new room"]'), 'the new cell sorts past the rows shown');
  E.revealCell('int:Zzz new room');
  await p.wait(20);
  const row = d.querySelector('#edCellList tr[data-cell="int:Zzz new room"]');
  t.ok(row, 'revealed: its row is on the list');
  t.ok(row && /sel/.test(row.className), 'and selected');
  t.ok(E.cellFilter === '', 'the filter that would hide it is cleared');

  // The other routes that make a cell show it too.
  w.WgUI.askText = async () => 'Zzz copy of an old room';
  E.cellSel = null;
  await E.copyCell('Room 001');
  await p.wait(20);
  t.ok(E.cellSel === 'int:Zzz copy of an old room', 'a copied interior is selected: ' + E.cellSel);
  t.ok(d.querySelector('#edCellList tr.sel[data-cell="int:Zzz copy of an old room"]'), 'and its row is on the list');
  E.cellFilter = 'room';
  await w.WgFlow.newLand('90,90');
  await p.wait(20);
  t.ok(E.cellSel === '90,90' && E.cellFilter === '', 'a square made with land is revealed: ' + E.cellSel);

  // Q with one object in a box selection.
  const hit = { refKey: 'new:u1', id: 'rock', model: 'x.nif', m: {}, wpos: [0, 0, 0] };
  w.App.R = { pickables: [hit], setSelection() {}, setStaticHighlight() {} };
  E._group = [{ hit, ref: { cell: 'Zzz new room', uid: 'u1' } }];
  E._sel = null;
  t.ok(E.group().length === 1 && !E.selected(), 'one in the group, nothing "selected" the click way');
  const shown = E.quickMenu(20, 20);
  const q = d.getElementById('edQ');
  t.ok(shown !== false && q && !q.hidden, 'Q opens a menu for a selection of one');
  t.ok(q && /Edit reference/.test(q.textContent), 'the single-object menu');
  // And through the key itself, which is where a selection of one used to be turned away.
  q.hidden = true;
  const handled = E.keys({ key: 'q', ctrlKey: false, metaKey: false, altKey: false, preventDefault() {} });
  t.ok(handled === true && !q.hidden, 'the Q key opens it');
};
