/* The Use Report (50_wg_editor_uses.js) says when the id is also another object type
   (`twins`, the Construction Set's Matching IDs), and says nothing when it is not. */
'use strict';
const { page, EDITOR } = require('./harness');

module.exports = async (t) => {
  let twins = [{ tag: 'STAT', plugin: 'Morrowind.esm' }];
  const p = page({
    modules: EDITOR,
    links: { editRecord: 'r', editPending: 'pend', editUses: 'u' },
    engine: (c) => c === 'editor_tags' ? [['NPC_', 1]] : c === 'editor_records' ? { plugins: [], rows: [] }
      : c === 'cells' ? { grid: [], count: 0, interiors: [], interiorCount: 0 } : {},
    answer: (url) => url === 'pend' ? [] : url === 'u'
      ? Object.assign({ tag: 'NPC_', id: 'fargoth', uses: [], live: 0, cells: 0 }, twins ? { twins } : {}) : {},
  });
  const { w, d } = p;
  const E = w.WgEditor;
  await E.enter();
  await E.showUses('NPC_', 'fargoth');
  const warn = d.querySelector('#edUsesBody .warn');
  t.ok(warn && /also a .*Morrowind\.esm/.test(warn.textContent), 'the other type is said: ' + (warn && warn.textContent));
  twins = null;
  await E.showUses('NPC_', 'fargoth');
  t.ok(!d.querySelector('#edUsesBody .warn'), 'nothing said when the id is one type only');
};
