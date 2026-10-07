/* Test in OpenMW's launch setups (editTestSetups): one made in the panel, kept with its
   content files and script, and chosen as the one tests use. */
'use strict';
const { page, EDITOR } = require('./harness');

module.exports = async (t) => {
  let kept = { setups: {}, use: '' };
  const p = page({
    modules: EDITOR,
    links: { editRecord: 'r', editPending: 'pend', editTestRun: 'test', editTestSetups: 'setups' },
    engine: (c) => c === 'editor_tags' ? [['WEAP', 1]] : c === 'editor_records' ? { plugins: [], rows: [] } : {},
    answer: (url, b) => {
      if (url === 'pend') return [];
      if (url === 'setups') {
        if (b.setups) kept.setups = b.setups;
        if ('use' in b) kept.use = b.use;
        return kept;
      }
      return {};
    },
  });
  const { d } = p;
  const E = p.w.WgEditor;
  await E.enter();
  await E.showSetups();
  const body = d.getElementById('edSetBody');
  t.ok(body && body.querySelector('#edSetPick').value === '', 'none yet: a new one');
  body.querySelector('#edSetName').value = 'Mine';
  body.querySelector('#edSetContent').value = 'A.esp\n\n B.esp ';
  body.querySelector('#edSetScript').value = 'tgm';
  body.querySelector('#edSetUse').checked = true;
  await body.querySelector('#edSetSave').onclick();
  t.ok(JSON.stringify(kept) === JSON.stringify({ setups: { Mine: { content: ['A.esp', 'B.esp'], script: 'tgm' } }, use: 'Mine' }), 'kept: ' + JSON.stringify(kept));
  t.ok(body.querySelector('#edSetPick').value === 'Mine' && body.querySelector('#edSetDel'), 'shown as saved');
  await body.querySelector('#edSetDel').onclick();
  t.ok(!Object.keys(kept.setups).length && kept.use === '', 'deleted, and no longer used');
  E.leave();
};
