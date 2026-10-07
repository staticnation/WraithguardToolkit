/* The Dialogue window's drag to reorder (editMoveResponse), and the Build patch list's
   selection and Delete. */
'use strict';
const { page, ev, EDITOR } = require('./harness');

module.exports = async (t) => {
  let topic = {
    id: 'Rumors', type: 'Topic',
    responses: ['First.', 'Second.', 'Third.'].map((text, i) => ({ id: String(i + 1), text, speaker: '', plugins: ['M.esm'], winner: 'M.esm' })),
  };
  let pool = [
    { tag: 'WEAP', type: 'Weapon', id: 'iron sword', whole: 'A.esp', changes: [] },
    { tag: 'NPC_', type: 'Npc', id: 'fargoth', changes: [{ path: 'name' }] },
    { type: 'Reference', id: 'lamp', ref: { cell: '(1, 2)', origin: 'M.esm', refr: 7 }, changes: [{}] },
    { type: 'Reference', id: 'crate', new: { cell: '(1, 2)', uid: 'u1', id: 'crate' }, changes: [] },
  ];
  const p = page({
    modules: EDITOR,
    links: { editRecord: 'r', editPending: 'pend', editTopic: 'topic', editMoveResponse: 'move', editBuild: 'b',
      editBuildInfo: 'info', editBuildCheck: 'check', editRevert: 'rev', editRefRevert: 'refrev', editNewRemove: 'newrm' },
    engine: (c) => c === 'editor_tags' ? [['WEAP', 1]] : c === 'editor_records' ? { plugins: [], rows: [] } : {},
    answer: (url, b) => {
      if (url === 'pend') return pool;
      if (url === 'topic') return topic;
      if (url === 'move') {
        const ids = topic.responses.map(r => r.id).filter(i => i !== b.id);
        ids.splice(b.after ? ids.indexOf(b.after) + 1 : 0, 0, b.id);
        topic = Object.assign({}, topic, { responses: ids.map(i => topic.responses.find(r => r.id === i)) });
        return topic;
      }
      if (url === 'info') return { summary: { records: pool.length, whole: 1 }, folders: [], defaultName: 'p.esp', suggested: '', last: null, order: [] };
      if (url === 'check') return { exists: false };
      if (url === 'rev') pool = pool.filter(x => !(x.tag === b.tag && x.id === b.id));
      if (url === 'refrev') pool = pool.filter(x => !(x.ref && x.ref.refr === b.refr));
      if (url === 'newrm') pool = pool.filter(x => !(x.new && x.new.uid === b.uid));
      return {};
    },
  });
  const { w, d } = p;
  const E = w.WgEditor;
  await E.enter();

  // Dialogue: drag the third response above the first.
  E.dial = d.createElement('div'); E.dial.innerHTML = '<div id="edDialResp"></div>'; d.body.appendChild(E.dial);
  await E.openTopic('Rumors');
  const rows = [...E.dial.querySelectorAll('tr[data-r]')];
  rows.forEach((r, i) => { r.getBoundingClientRect = () => ({ top: 100 + i * 20, height: 20, left: 0, right: 500, bottom: 120 + i * 20, width: 500 }); });
  d.elementFromPoint = (x, y) => rows[Math.floor((y - 100) / 20)] || null;
  rows[2].dispatchEvent(ev(w, 'pointerdown', 10, 150));
  w.dispatchEvent(ev(w, 'pointermove', 10, 103));
  w.dispatchEvent(ev(w, 'pointerup', 10, 103));
  await p.wait(30);
  const mp = p.posts.find(x => x[0] === 'move');
  t.ok(mp && mp[1].id === '3' && mp[1].after === '', 'moved to the top: ' + JSON.stringify(mp));
  const order = [...E.dial.querySelectorAll('tr[data-r]')].map(tr => tr.title.split('\n')[0]);
  t.ok(order[0] === 'Third.', 'redrawn in the new order: ' + order);

  // Build patch: select with Shift and Ctrl, Delete takes the selected out.
  await E.showBuild();
  const brows = () => [...d.querySelectorAll('#edBuildBody tr[data-i]')];
  t.ok(brows().length === 4, 'four entries');
  t.ok(!!brows()[0].querySelector('[data-bdrop]'), 'a whole record has its own delete');
  const click = (tr, o) => tr.dispatchEvent(new w.MouseEvent('click', Object.assign({ bubbles: true }, o || {})));
  click(brows()[0]); click(brows()[2], { shiftKey: true });
  t.ok(d.querySelectorAll('#edBuildBody tr.sel').length === 3, 'Shift selects a run');
  click(brows()[1], { ctrlKey: true });
  d.querySelector('.edBuildList').focus();
  d.dispatchEvent(new w.KeyboardEvent('keydown', { key: 'Delete', bubbles: true }));
  for (let i = 0; i < 50 && brows().length !== 2; i++) await p.wait(10);
  t.ok(brows().length === 2 && /fargoth/.test(brows()[0].textContent), 'Delete took out the selected: ' + brows().map(r => r.textContent).join('|'));
  E.leave();
};
