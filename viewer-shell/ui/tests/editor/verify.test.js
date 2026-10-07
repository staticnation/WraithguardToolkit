/* The Verify panel (editVerify): the findings in a table, counted, filtered by check; a
   row opens its record; the toolbar counts the errors. */
'use strict';
const { page, EDITOR } = require('./harness');

const FINDINGS = {
  checked: 3,
  findings: [
    { level: 'error', check: 'Missing ID', tag: 'NPC_', id: 'fargoth', path: 'script', message: 'script names NoSuch, which no plugin defines (SCPT)' },
    { level: 'warning', check: 'Leveled list', tag: 'LEVI', id: 'l_gold', path: 'items', message: 'the list is empty' },
  ],
};

module.exports = async (t) => {
  const p = page({
    modules: EDITOR,
    links: { editRecord: 'r', editPending: 'pend', editVerify: 'ver' },
    engine: (c) => c === 'editor_tags' ? [['NPC_', 1]] : c === 'editor_records' ? { plugins: [], rows: [] } : {},
    answer: (url) => url === 'pend' ? [] : url === 'ver' ? FINDINGS : {},
  });
  const { w, d } = p;
  const E = w.WgEditor;
  await E.enter();
  E.keys(new w.KeyboardEvent('keydown', { key: 'V', ctrlKey: true, shiftKey: true }));
  await p.wait(10);
  const body = d.getElementById('edVerBody');
  t.ok(body && body.querySelectorAll('tbody tr').length === 2, 'Ctrl+Shift+V lists the findings');
  t.ok(/3 records checked: 1 error, 1 warning/.test(body.textContent), 'counted: ' + body.textContent.slice(0, 60));
  const btn = d.getElementById('edVerifyBtn');
  t.ok(btn && btn.dataset.n === '1', 'the toolbar counts the errors');
  const sel = d.getElementById('edVerCheck');
  sel.value = 'Leveled list'; sel.onchange();
  t.ok(body.querySelectorAll('tbody tr').length === 1, 'filtered by check');
  let opened = null;
  E.showEntry = async (tag, id) => { opened = tag + ' ' + id; };
  sel.value = ''; sel.onchange();
  body.querySelector('tbody tr').ondblclick();
  t.ok(opened === 'NPC_ fargoth', 'a row opens its record: ' + opened);
  E.leave();
};
