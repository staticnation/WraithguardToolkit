/* The Messages panel: the log (WgLog, from 03_core.js) kept, filtered, copied, cleared;
   the toolbar's count of errors not yet seen. */
'use strict';
const fs = require('fs');
const path = require('path');
const { page, EDITOR } = require('./harness');

module.exports = async (t) => {
  const p = page({
    modules: EDITOR,
    links: { editRecord: 'r', editPending: 'pend' },
    engine: (c) => c === 'editor_tags' ? [['WEAP', 1]] : c === 'editor_records' ? { plugins: [], rows: [] } : {},
    answer: (url) => url === 'pend' ? [] : {},
  });
  const { w, d } = p;
  // WgLog as 03_core.js has it; the harness's toast() then keeps to it, as the real one does.
  const core = fs.readFileSync(path.join(__dirname, '..', '..', 'src', '03_core.js'), 'utf8');
  const log = core.match(/^const WgLog=\{[\s\S]*?^\};/m);
  t.ok(log, 'WgLog found in 03_core.js');
  w.eval(log[0].replace(/^const /, 'var '));
  w.eval('var __t=toast; toast=function(m,k){ WgLog.add(m,k); return __t(m,k); };');
  w.eval(require('./harness').source('src/55_wg_messages.js'));
  let copied = null;
  Object.defineProperty(w.navigator, 'clipboard', { value: { writeText: async s => { copied = s; } } });

  const E = w.WgEditor;
  await E.enter();
  const btn = d.getElementById('edMsgBtn');
  t.ok(btn, 'the toolbar has a Messages button');

  w.toast('Saved iron sword', 'ok');
  w.toast('Wraithguard answered 400: no such record', 'err');
  w.toast('Nothing to undo', 'warn');
  t.ok(btn.classList.contains('badge') && btn.dataset.n === '1', 'one error not yet seen: ' + btn.dataset.n);

  E.keys(new w.KeyboardEvent('keydown', { key: 'M', ctrlKey: true, shiftKey: true }));
  const panel = d.getElementById('edMsgPanel');
  t.ok(panel && !panel.hidden, 'Ctrl+Shift+M opens it');
  t.ok(!btn.classList.contains('badge'), 'opening it sees the errors');
  t.ok(panel.querySelectorAll('.edMsg').length === 3, 'every message listed');

  panel.querySelector('[data-k="ok"]').checked = false;
  panel.querySelector('[data-k="ok"]').onchange();
  const f = panel.querySelector('#edMsgFind'); f.value = 'record'; f.oninput();
  const rows = panel.querySelectorAll('.edMsg');
  t.ok(rows.length === 1 && rows[0].classList.contains('err'), 'filtered by kind and text');
  await panel.querySelector('#edMsgCopy').onclick();
  t.ok(copied && /err\s+Wraithguard answered 400/.test(copied) && !/Saved/.test(copied), 'copies what is shown: ' + copied);

  panel.querySelector('#edMsgClear').click();
  t.ok(w.WgLog.items.length <= 1 && panel.querySelectorAll('.edMsg').length === 0, 'cleared');
  E.leave();
};
