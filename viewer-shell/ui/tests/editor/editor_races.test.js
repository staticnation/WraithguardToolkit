/* Delayed replies retain the identity of the dialog, tab and bulk edit. */
'use strict';
const { page, EDITOR } = require('./harness');
const deferred = () => { let resolve; const promise = new Promise(r => { resolve = r; }); return { promise, resolve }; };
const ref = n => ({ cell: 'Vault', origin: 'M.esm', refr: n });
const view = n => ({ ...ref(n), id: 'object' + n, winner: 'M.esm', plugins: ['M.esm'], fields: [] });
module.exports = async t => {
  {
    const a = deferred(), b = deferred();
    const p = page({ modules: EDITOR, links: { editRecord: 'r', editRef: 'ref', editRefSet: 'set', editPending: 'pend' },
      answer: (u, body) => u === 'ref' ? (body.refr === 1 ? a.promise : b.promise) : u === 'set' ? view(body.refr) : [] });
    const E = p.w.WgEditor, pa = E.openRef(ref(1)), pb = E.openRef(ref(2));
    b.resolve(view(2)); await pb; a.resolve(view(1)); await pa;
    t.ok(E.refReq().refr === 2 && p.d.querySelector('#edDlgTitle').textContent.includes('object2'), 'late reference reply cannot replace newer dialog');
    await E.sendRef(ref(1), [['scale', 1.5]], { noUndo: true });
    t.ok(E.refReq().refr === 2, 'transform keeps other dialog identity'); p.w.close();
  }
  {
    const a = deferred(), b = deferred();
    const v = (tag, id) => ({ tag, id, type: tag, winner: 'M.esm', plugins: ['M.esm'], fields: [], whole: false, new: false });
    const p = page({ modules: EDITOR, links: { editRecord: 'r' }, answer: (_u, body) => body.tag === 'STAT' ? a.promise : b.promise });
    const E = p.w.WgEditor, pa = E.openRecord('STAT', 'rock', null), pb = E.openRecord('WEAP', 'sword', null);
    b.resolve(v('WEAP', 'sword')); await pb; a.resolve(v('STAT', 'rock')); await pa;
    t.ok(E.record.tag === 'WEAP' && E.record.id === 'sword' && p.d.querySelector('#edDlgTitle').textContent.includes('sword'), 'late record reply cannot mix tag and ID'); p.w.close();
  }
  {
    const d = deferred();
    const p = page({ modules: EDITOR, links: { editRecord: 'r', editRef: 'ref' }, answer: u => u === 'r' ? d.promise : view(2) });
    const E = p.w.WgEditor, old = E.openRecord('STAT', 'rock', null);
    await E.openRef(ref(2));
    d.resolve({ tag: 'STAT', id: 'rock', type: 'Static', winner: 'M.esm', plugins: ['M.esm'], fields: [], whole: false, new: false }); await old;
    t.ok(E.record === null && p.d.querySelector('#edDlgTitle').textContent.includes('object2'), 'opening a reference invalidates an old record request'); p.w.close();
  }
  {
    const a = deferred(), b = deferred();
    const p = page({ modules: EDITOR, engine: (_c, args) => args.tag === 'STAT' ? a.promise : b.promise });
    const E = p.w.WgEditor; E.tag = 'STAT'; const pa = E.loadTag('STAT'); E.tag = 'WEAP'; const pb = E.loadTag('WEAP');
    b.resolve({ plugins: ['M.esm'], rows: [['sword', '', '', [], 0]] }); await pb;
    a.resolve({ plugins: ['Other.esm'], rows: [['rock', '', '', [], 0]] }); await pa;
    t.ok(E.rows[0][0] === 'sword' && E.plugins[0] === 'M.esm', 'late tab reply cannot overwrite rows/plugins'); p.w.close();
  }
  {
    const a = deferred(), b = deferred();
    const p = page({ modules: EDITOR, html: '<div id="edRefList"></div><span id="edCellName"></span>', engine: (_c, args) => args.cell === 'int:A' ? a.promise : b.promise });
    const E = p.w.WgEditor, pa = E.loadRefs('int:A'), pb = E.loadRefs('int:B');
    const v = name => ({ name, refCell: name, plugins: ['M.esm'], rows: [['M.esm:1', name, 'STAT', [0,0,0], 'M.esm', '']] });
    b.resolve(v('B')); await pb; a.resolve(v('A')); await pa;
    t.ok(p.d.querySelector('#edCellName').textContent.startsWith('B -') && E.refs[0][1] === 'B', 'late cell reply cannot replace newer reference list'); p.w.close();
  }
  {
    const d = deferred();
    const p = page({ modules: EDITOR, links: { editRecord: 'r', editSetMany: 'many', editPending: 'pend' }, answer: u => u === 'r' ? d.promise : u === 'many' ? { changed: 2, missing: [], failed: [] } : [] });
    const E = p.w.WgEditor; E.tag = 'MISC'; p.w.WgUI.askText = async title => title.startsWith('Set which') ? 'name' : 'New name';
    const job = E.setManyAsk(['one', 'two']); E.tag = 'WEAP';
    d.resolve({ tag: 'MISC', id: 'one', type: 'MiscItem', winner: 'M.esm', plugins: ['M.esm'], whole: false, new: false, fields: [{ path: 'name', value: 'Old', editable: true }] }); await job;
    t.ok(p.posts.find(x => x[0] === 'many')[1].tag === 'MISC' && E._fieldsOf.MISC && !E._fieldsOf.WEAP, 'bulk request and field cache retain original type'); p.w.close();
  }
  {
    let rev = 1, pending = [], reads = 0;
    const p = page({ modules: EDITOR, links: { editRecord: 'r', editPending: 'pend', editRevision: 'rev' }, answer: u => u === 'rev' ? { rev, undo: 1, redo: 0 } : u === 'pend' ? (() => { reads++; const list = pending; pending = [{ tag: 'CELL', id: 'Vault', changes: [] }, { tag: 'PGRD', id: 'Other', changes: [] }]; rev = 2; return list; })() : {} });
    const E = p.w.WgEditor; E.on = true; await E.refreshPending(); await E.pollPool();
    t.ok(reads === 2 && E.edited.has('CELL:vault') && E.editedCells.has('vault') && E.editedCells.has('other'), 'next poll fetches intervening edit and marks CELL'); p.w.close();
  }
  {
    const p = page({ modules: EDITOR, links: { editRecord: 'r', editRefsSet: 'batch', editPending: 'pend' }, answer: u => { if (u === 'batch') throw new Error('invalid scale'); return []; } });
    const E = p.w.WgEditor; E.transformTargets = () => [1, 2].map(n => ({ ref: ref(n), hit: { wpos: [n, 0, 0], rot: [0, 0, 0], scale: 1 } }));
    await E.transformBy([1, 0, 0], [0, 0, 0], 1);
    t.ok(p.toasts.includes('invalid scale') && !p.toasts.includes('2 changed together'), 'rejected group has no success toast');
    E.sendRef = async () => false; await E.groupAction('rot0', E.transformTargets());
    t.ok(!p.toasts.includes('2 changed'), 'rejected context-menu group change has no success toast'); p.w.close();
  }
};
