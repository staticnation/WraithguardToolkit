/* One undo history, Wraithguard's (editUndo / editRedo): Ctrl+Z asks it, the open
   record is read again, the toolbar greys what there is none of, a toast says what. */
'use strict';
const { page, EDITOR } = require('./harness');

module.exports = async (t) => {
  let hist = { undo: 1, redo: 0 };
  const p = page({
    modules: EDITOR,
    links: { editRecord: 'r', editPending: 'pend', editUndo: 'undo', editRedo: 'redo' },
    engine: (c) => c === 'editor_tags' ? [['WEAP', 1]] : c === 'editor_records' ? { plugins: [], rows: [] } : {},
    answer: (url) => {
      if (url === 'pend') return [];
      if (url === 'r') return { tag: 'WEAP', id: 'iron sword', type: 'Weapon', winner: 'Morrowind.esm', plugins: ['Morrowind.esm'], fields: [], whole: false, new: false };
      if (url === 'undo' || url === 'redo') {
        const take = url === 'undo' ? 'undo' : 'redo', give = url === 'undo' ? 'redo' : 'undo';
        if (!hist[take]) return { done: false, what: [], ...hist };
        hist = { [take]: hist[take] - 1, [give]: hist[give] + 1 };
        return { done: true, what: ['Weapon iron sword'], ...hist };
      }
      return {};
    },
  });
  const { w, d } = p;
  const E = w.WgEditor;
  await E.enter();
  await E.openRecord('WEAP', 'iron sword', null);
  const reads = () => p.posts.filter(x => x[0] === 'r').length;
  const before = reads();

  await E.undo();
  t.ok(p.posts.some(x => x[0] === 'undo'), 'Ctrl+Z asks Wraithguard');
  t.ok(reads() === before + 1, 'the open record is read again');
  t.ok(p.toasts.some(m => /^Undone: Weapon iron sword/.test(m)), 'a toast says what: ' + JSON.stringify(p.toasts));
  const tb = k => d.querySelector('[data-tb="' + k + '"]');
  t.ok(tb('undo') && tb('undo').classList.contains('off'), 'Undo greyed: nothing more');
  t.ok(tb('redo') && !tb('redo').classList.contains('off'), 'Redo on');

  await E.undo();
  t.ok(p.toasts.includes('Nothing to undo'), 'nothing more to undo');
  await E.redo();
  t.ok(p.posts.some(x => x[0] === 'redo') && /^Redone/.test(p.toasts[p.toasts.length - 1]), 'Ctrl+Y redoes');
  t.ok(!tb('undo').classList.contains('off') && tb('redo').classList.contains('off'), 'the toolbar follows');
  E.leave();
};
