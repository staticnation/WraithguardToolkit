/* The Object Window's category columns follow the queue: an edit shows in its column
   (not the plugin's original value), and a columns answer that was asked for before
   a later edit never replaces the newer one. */
'use strict';
const { page, EDITOR } = require('./harness');

module.exports = async (t) => {
  let level = 2, rev = 1;
  const gate = [];                       // answers held back, to be let go later
  let hold = false;
  const cols = (n) => ({
    columns: [
      { label: 'Name', kind: 'text', title: '', path: 'name' },
      { label: 'Level', kind: 'num', title: '', path: 'data.level' },
      { label: 'Persists', kind: 'bool', title: '', path: null },
      { label: 'Blocked', kind: 'bool', title: '', path: null },
    ],
    rows: { fargoth: ['Fargoth', n, 'no', 'no'] },
  });
  const p = page({
    modules: EDITOR,
    links: { editRecord: 'r', editPending: 'pend', editColumns: 'cols', editRevision: 'rev' },
    engine: (c) => c === 'editor_tags' ? [['NPC_', 1]]
      : c === 'editor_records' ? { plugins: ['M.esm'], rows: [['fargoth', 'Fargoth', '', [0], 0]] } : {},
    answer: (url) => {
      if (url === 'pend') return level === 2 ? [] : [{ tag: 'NPC_', id: 'fargoth', changes: [{ path: 'data.level', value: level }] }];
      if (url === 'rev') return { rev, undo: 0, redo: 0 };
      if (url === 'cols') {
        const n = level;                 // what the backend knows when it is asked
        if (!hold) return cols(n);
        return new Promise(res => gate.push(() => res(cols(n))));
      }
      return {};
    },
  });
  const { w, d } = p;
  const E = w.WgEditor;
  const levelCell = () => {
    const th = [...d.querySelectorAll('#edTable thead th')].findIndex(x => /^Level/.test(x.textContent));
    const tr = d.querySelector('#edTable tr[data-id="fargoth"]');
    return th < 0 || !tr ? null : tr.children[th].textContent;
  };
  await E.enter();
  await p.wait(30);
  t.ok(levelCell() === '2', 'the plugin\'s level to begin with: ' + levelCell());
  const asked = () => p.posts.filter(x => x[0] === 'cols').length;
  const first = asked();

  // An edit lands: the queue's revision moves, and the column is read again.
  level = 7; rev = 2;
  await E.refreshPending();
  await p.wait(30);
  t.ok(asked() === first + 1, 'the columns were asked for again: ' + (asked() - first));
  t.ok(levelCell() === '7', 'the column shows the queued value: ' + levelCell());

  // The same revision again (the poll): nothing is read again.
  await E.refreshPending();
  await p.wait(30);
  t.ok(asked() === first + 1, 'an unchanged queue does not re-read the columns');

  // Two edits close together, the answers coming back in the wrong order: the newer wins.
  hold = true;
  level = 8; rev = 3; await E.refreshPending(); await p.wait(10);   // asked, held (says 8)
  level = 9; rev = 4; await E.refreshPending(); await p.wait(10);   // asked, held (says 9)
  t.ok(gate.length === 2, 'two answers are held: ' + gate.length);
  gate[1](); await p.wait(20);
  gate[0](); await p.wait(20);
  t.ok(levelCell() === '9', 'the older answer did not replace the newer: ' + levelCell());
  hold = false;

  // A world change drops the columns (a connect): they are asked for again with the rows.
  const before = asked();
  await E.worldChanged();
  await p.wait(30);
  t.ok(asked() > before && levelCell() === '9', 'a new world asks again: ' + (asked() - before));

  // Clicking down a row of tabs: only the tab stopped on has its columns asked for, and the rows
  // of every tab are on screen at once (not waiting for columns that may be slow or time out).
  w.WgEditor.tags = [['NPC_', 1], ['WEAP', 1], ['LIGH', 1], ['MISC', 1]];
  w.WgEditor._cols = {};
  const before2 = asked();
  const tabsEl = [...d.querySelectorAll('#edTabs [data-tag]')];
  t.ok(tabsEl.length >= 1, 'tabs are drawn');
  for (const tag of ['WEAP', 'LIGH', 'MISC']) {
    w.WgEditor.tag = tag; w.WgEditor._fromTab = true;
    await w.WgEditor.loadTag(tag);          // returns without waiting for the columns
  }
  await p.wait(500);
  t.ok(asked() - before2 === 1, 'one columns request for the tab stopped on, not one per click: ' + (asked() - before2));
};
