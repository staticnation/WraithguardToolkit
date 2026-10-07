/* The Keys sheet (columns, filter), the ORI / full help switches, the cell on screen,
   and "Make a copy as" for an interior (editDuplicateCell). */
'use strict';
const { page, EDITOR } = require('./harness');

module.exports = async (t) => {
  const p = page({
    modules: EDITOR,
    links: { editRecord: 'r', editPending: 'pend', editDuplicateCell: 'dup' },
    engine: (c) => c === 'editor_tags' ? [['WEAP', 1]] : c === 'editor_records' ? { plugins: [], rows: [] } : {},
    answer: (url) => url === 'pend' ? [] : url === 'dup' ? { cell: 'Vault 2', refs: 3, pathgrid: true } : {},
  });
  const { w, d } = p;
  const E = w.WgEditor;
  await E.enter();

  // The Keys sheet: sections, a filter that also matches a section's heading, closes.
  const sheet = w.WgUI.keysSheet(null);
  const secs = sheet.querySelectorAll('.edKeysSec');
  t.ok(secs.length >= 4, 'sections: ' + secs.length);
  const f = sheet.querySelector('input'); f.value = 'path'; f.oninput();
  const shown = [...secs].filter(s => !s.hidden).length;
  t.ok(shown > 0 && shown < secs.length, 'the filter narrows it: ' + shown);
  d.body.dispatchEvent(new w.Event('pointerdown', { bubbles: true }));
  t.ok(!d.querySelector('.edKeys'), 'a click elsewhere closes it');

  // ORI / full help off in the Editor, by the setting and by Shift+I.
  E.setPanels('ori', false);
  t.ok(d.body.classList.contains('edNoOri'), 'ORI hidden');
  E.keys(new w.KeyboardEvent('keydown', { key: 'I', shiftKey: true }));
  t.ok(d.body.classList.contains('edNoTfh'), 'full help hidden by Shift+I');

  // The cell on screen.
  w.App._scene = { cells: [{ kind: 'int', name: 'Vault' }], origin: [0, 0] };
  t.ok(E.currentCell().key === 'Vault', 'an interior by name');
  w.App._scene = { cells: [{ kind: 'ext', gx: -3, gy: -2, name: 'Balmora' }, { kind: 'ext', gx: -2, gy: -2 }], origin: [-24576, -16384] };
  w.App.R.cam = { tx: 9000, ty: 100 };
  t.ok(E.currentCell().key === '(-2, -2)', 'the exterior under the pivot: ' + JSON.stringify(E.currentCell()));

  // "Make a copy as": the name asked, the copy queued.
  const done = E.copyCell('Vault');
  await p.wait(5);
  const inp = d.querySelector('.edAskBg input');
  t.ok(inp && inp.value === 'Vault copy', 'the name is asked');
  inp.value = 'Vault 2'; d.querySelector('.edAskBg [data-a="ok"]').click();
  await done;
  const dp = p.posts.find(x => x[0] === 'dup');
  t.ok(dp && dp[1].cell === 'Vault' && dp[1].newName === 'Vault 2', 'copy asked: ' + JSON.stringify(dp));
  t.ok(!d.querySelector('.edAskBg'), 'the question closes');
  E.leave();
  t.ok(!d.body.classList.contains('edNoOri'), 'the switches apply in the Editor only');
};
