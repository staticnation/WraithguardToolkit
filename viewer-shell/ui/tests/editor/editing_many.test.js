/* Editing more at once: rows picked with Ctrl/Shift+click and a field set on all of
   them (editSetMany); a table cell edited in place; the transform panel's maths (turn and
   scale about the middle) sent as one step (editRefsSet); selection sets, isolate, hide;
   saved searches; a record dropped on a field. */
'use strict';
const fs = require('fs');
const path = require('path');
const { page, EDITOR } = require('./harness');

const ROWS = [['a_one', 'One', '', [0], 0], ['b_two', 'Two', '', [0], 0], ['c_three', 'Three', '', [0], 0]];
const COLS = { columns: [{ label: 'Name', kind: 'text', title: '', path: 'name' }, { label: 'Persists', kind: 'bool', title: '', path: null }, { label: 'Blocked', kind: 'bool', title: '', path: null }],
  rows: { a_one: ['One', 'no', 'no'], b_two: ['Two', 'no', 'no'], c_three: ['Three', 'no', 'no'] } };

module.exports = async (t) => {
  const p = page({
    modules: EDITOR,
    links: { editRecord: 'r', editPending: 'pend', editSetMany: 'many', editColumns: 'cols', editRefsSet: 'refs', editRefSet: 'ref1' },
    engine: (c, a) => c === 'editor_tags' ? [['MISC', 3], ['SCPT', 1]] : c === 'editor_records' ? (a.tag === 'SCPT' ? { plugins: ['M.esm'], rows: [['MyScript', '', '', [0], 0]] } : { plugins: ['M.esm'], rows: ROWS }) : {},
    answer: (url, b) => url === 'pend' ? [] : url === 'cols' ? COLS : url === 'many' ? { changed: b.ids.length, failed: [], missing: [] }
      : url === 'refs' ? { changed: b.changes.length } : {},
  });
  const { w, d } = p;
  // The real rotation maths (the harness stands in a flat one).
  const src = fs.readFileSync(path.join(__dirname, '..', '..', 'src', '18_cellpreview.js'), 'utf8');
  w.eval(src.match(/^function refMatrix[\s\S]*?^}/m)[0]);
  const E = w.WgEditor;
  w.localStorage.clear();
  await E.enter();
  await p.wait(5);
  const rows = () => [...d.querySelectorAll('#edTable tr[data-id]')];
  const click = (tr, more) => tr.onclick(Object.assign({ shiftKey: false, ctrlKey: false }, more || {}));

  // Ctrl+click and Shift+click pick several rows.
  click(rows()[0]);
  click(rows()[2], { shiftKey: true });
  t.ok(E.selectedRows().length === 3, 'Shift+click: the run: ' + E.selectedRows());
  click(rows()[1], { ctrlKey: true });
  t.ok(JSON.stringify(E.selectedRows()) === '["a_one","c_three"]', 'Ctrl+click takes one away: ' + E.selectedRows());

  // A field set on all of them.
  await E.setMany(E.selectedRows(), 'data.value', '5');
  const sent = p.posts.find(x => x[0] === 'many');
  t.ok(sent && JSON.stringify(sent[1].ids) === '["a_one","c_three"]' && sent[1].path === 'data.value', 'editSetMany: ' + JSON.stringify(sent && sent[1]));

  // A cell edited in place: on every selected row.
  const tr = rows().find(r => r.dataset.id === 'a_one');
  const nameTd = tr.children[2];                       // ID, Count, Name
  tr.ondblclick({ target: nameTd });
  const inp = nameTd.querySelector('input');
  t.ok(inp && inp.value === 'One', 'the cell becomes a field');
  inp.value = 'Renamed';
  inp.onkeydown({ key: 'Enter', stopPropagation() {} });
  await p.wait(5);
  const cellSent = p.posts.filter(x => x[0] === 'many').pop();
  t.ok(cellSent[1].path === 'name' && cellSent[1].value === 'Renamed' && cellSent[1].ids.length === 2, 'in place, on the selection: ' + JSON.stringify(cellSent[1]));

  // The transform: two objects turned 90 degrees about their middle, as one step.
  const hit = (key, pos) => ({ refKey: key, wpos: pos, rot: [0, 0, 0], scale: 1, m: [1, 0, 0, pos[0], 0, 1, 0, pos[1], 0, 0, 1, pos[2]] });
  const r1 = { cell: 'Vault', origin: 'M.esm', refr: 1 }, r2 = { cell: 'Vault', origin: 'M.esm', refr: 2 };
  const k1 = E.refKeyOf(r1), k2 = E.refKeyOf(r2);
  const group = [{ hit: hit(k1, [0, 0, 0]), ref: r1 }, { hit: hit(k2, [100, 0, 0]), ref: r2 }];
  const plan = E.transformPlan(group, [0, 0, 0], [0, 0, 90], 1);
  const at = plan.map(([, c]) => c.find(x => x[0] === 'translation')[1].map(v => Math.round(v)));
  // The game's turn: a positive Z turn is clockwise seen from above, the positions turned
  // by the same matrix as each object's own rotation (the group stays as it was built).
  t.ok(JSON.stringify(at) === '[[50,50,0],[50,-50,0]]', 'turned about the middle: ' + JSON.stringify(at));
  t.ok(plan.every(([, c]) => c.some(x => x[0] === 'rotation' && Math.abs(Math.abs(x[1][2]) - Math.PI / 2) < 1e-6)), 'each turned too');
  const scaled = E.transformPlan(group, [0, 0, 10], [0, 0, 0], 2);
  t.ok(JSON.stringify(scaled[1][1]) === '[["translation",[150,0,10]],["scale",2]]', 'scaled about the middle, moved: ' + JSON.stringify(scaled[1][1]));
  await E.sendRefs(plan);
  const refs = p.posts.find(x => x[0] === 'refs');
  t.ok(refs && refs[1].changes.length === 4 && !p.posts.some(x => x[0] === 'ref1'), 'one step for all of them');

  // Selection sets, isolate, hide.
  w.App.R.pickables = group.map(g => g.hit);
  w.App.R.cam = { tx: 0, ty: 0, tz: 0 };
  E._group = group.slice();
  E.keepSet(3);
  E._group = []; E._sel = null;
  E.recallSet(3);
  t.ok(E._group.length === 2 && w.App.R.cam.tx === 50, 'set 3 again, framed on its middle');
  E._group = [group[0]];
  E._sel = group[0];
  E.isolateSel();
  t.ok(E.hiddenKeys().has(k2.toLowerCase()) && !E.hiddenKeys().has(k1.toLowerCase()), 'only the selection shown');
  E.isolateSel();
  t.ok(!E.hiddenKeys().size, 'everything back');

  // Saved searches.
  E.filter = 'heavy'; E.keepSearches({ Heavy: { tag: 'MISC', query: 'data.weight>10', all: false } });
  await E.runSaved(E.savedSearches().Heavy);
  t.ok(d.querySelector('#edFilter').value === 'data.weight>10' && E.filter === 'data.weight>10', 'a saved search runs');

  // A record dropped on a field naming its type fills it; one of another type does not.
  const fld = d.createElement('input'); fld.type = 'text'; fld.dataset.path = 'script'; fld.dataset.ids = 'SCPT';
  let changed = 0; fld.onchange = () => { changed++; };
  E.dropOnField(fld, { tag: 'SCPT', id: 'MyScript' });
  t.ok(fld.value === 'MyScript' && changed === 1, 'dropped: filled and sent');
  E.dropOnField(fld, { tag: 'MISC', id: 'a_one' });
  t.ok(fld.value === 'MyScript' && /names Script/.test(p.toasts[p.toasts.length - 1] || ''), 'another type refused: ' + p.toasts[p.toasts.length - 1]);
  const ids = await E.idsOf('SCPT');
  t.ok(ids.includes('MyScript'), 'a type\'s ids, for the field to offer');
  E.leave();
};
