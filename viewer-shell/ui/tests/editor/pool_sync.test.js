/* The pool kept in step (editRevision): the page's own change notes the revision it
   read; a change from elsewhere (the Patch Builder) reads the lists and the open record
   again; a field being typed in is not taken from under the cursor. */
'use strict';
const { page, EDITOR } = require('./harness');

module.exports = async (t) => {
  let rev = 1, pending = [];
  const p = page({
    modules: EDITOR,
    links: { editRecord: 'r', editPending: 'pend', editRevision: 'rev', editUndo: 'undo' },
    engine: (c) => c === 'editor_tags' ? [['WEAP', 1]] : c === 'editor_records' ? { plugins: [], rows: [] } : {},
    answer: (url) => url === 'pend' ? pending : url === 'rev' ? { rev, undo: 2, redo: 0 }
      : url === 'r' ? { tag: 'WEAP', id: 'iron sword', type: 'Weapon', winner: 'Morrowind.esm', plugins: ['Morrowind.esm'], fields: [], whole: false, new: false } : {},
  });
  const { w, d } = p;
  const E = w.WgEditor;
  await E.enter();
  E.stopPoll();                                   // driven by hand below
  t.ok(E._rev === 1 && E._hist && E._hist.undo === 2, 'the revision read with the list: ' + E._rev);

  await E.openRecord('WEAP', 'iron sword', null);
  const reads = () => p.posts.filter(x => x[0] === 'r').length;
  const pend = () => p.posts.filter(x => x[0] === 'pend').length;
  let r0 = reads(), p0 = pend();
  await E.pollPool();
  t.ok(reads() === r0 && pend() === p0, 'nothing read again while the pool is the same');

  // The Patch Builder queues something: the lists and the record are read again.
  rev = 2; pending = [{ tag: 'WEAP', id: 'iron sword' }];
  await E.pollPool();
  t.ok(pend() === p0 + 1 && reads() === r0 + 1, 'a change elsewhere reads the pool and the record again');
  t.ok(E.edited.has('WEAP:iron sword') && E._rev === 2, 'the list follows');

  // While a field has the focus, the dialog only says it changed.
  const inp = d.createElement('input'); d.querySelector('#edDlgBody').appendChild(inp); inp.focus();
  r0 = reads(); rev = 3;
  await E.pollPool();
  t.ok(reads() === r0 && d.querySelector('#edDlgTitle .edStale'), 'not under the cursor: marked out of date');
  E.leave();
  t.ok(!E._poll, 'the poll stops on leaving');
};
