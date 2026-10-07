/* The workflow (56_wg_workflow.js): prefabs kept and placed as one change, new cells,
   the condition editor and "who can say this", a leveled list rolled, Script Edit's
   completion, go-to and uses, and search everything with replace. */
'use strict';
const { page, EDITOR } = require('./harness');

module.exports = async (t) => {
  const p = page({
    modules: EDITOR,
    links: { editRecord: 'r', editPending: 'pend', editPlaceMany: 'pm', editNewCell: 'nc', editFilterChoices: 'fc',
      editWhoCanSay: 'who', editLeveledRoll: 'roll', editScriptWords: 'sw', editScriptRefs: 'sr', editScript: 'scr',
      editSearchAll: 'sa', editReplaceAll: 'ra', editSet: 'set' },
    engine: (c, a) => c === 'editor_tags' ? [['MISC', 1], ['SCPT', 1]]
      : c === 'editor_records' ? { plugins: ['M.esm'], rows: a.tag === 'SCPT' ? [['OtherScript', '', '', [0], 0]] : [['gold_001', 'Gold', '', [0], 0]] }
      : c === 'editor_record_tag' ? (a.id === 'gold_001' ? { tag: 'MISC', plugins: ['M.esm'] } : {}) : {},
    answer: (url, b) => {
      if (url === 'pend') return [];
      if (url === 'pm') return { placed: b.items.map((_, i) => ({ cell: b.cell, uid: 'new-' + i })) };
      if (url === 'nc') return { cell: b.cell, interior: !b.cell.startsWith('(') };
      if (url === 'fc') return { types: ['None', 'Function', 'NotRace'], functions: ['PcLevel', 'Reputation'], comparisons: ['Equal', 'Greater'] };
      if (url === 'who') return { npcs: [{ id: 'fargoth', name: 'Fargoth' }], count: 1, capped: false, unchecked: ['Journal MS_Q'] };
      if (url === 'roll') return { level: b.level, outcomes: [{ id: 'gold_001', chance: 0.75 }, { id: '', chance: 0.25 }], nested: [] };
      if (url === 'sw') return { functions: ['AddItem', 'AddSpell'], keywords: ['begin', 'end'], globals: ['Day'] };
      if (url === 'sr') return { word: b.word, hits: [{ script: 'OtherScript', line: 3, text: 'set myVar to 1' }], scripts: 1, capped: false };
      if (url === 'scr') return { id: b.id, text: 'begin x\nend\n', findings: [], listing: '', compiled: false, compile: { ok: true, bytes: 0, messages: [] } };
      if (url === 'sa') return { hits: [{ tag: 'MISC', id: 'gold_001', path: 'name', value: 'Gold' }], count: 1, capped: false };
      if (url === 'ra') return { changed: b.hits.length, failed: [] };
      if (url === 'r') return { tag: b.tag, id: b.id, type: 'x', winner: 'M.esm', plugins: ['M.esm'], whole: false, new: false,
        fields: [{ path: 'filters', value: [], editable: true, kind: 'list', options: [] }] };
      return {};
    },
  });
  const { w, d } = p;
  const E = w.WgEditor, F = w.WgFlow;
  w.localStorage.clear();
  await E.enter();
  const answerAsk = async (value) => { await p.wait(5); const i = d.querySelector('.edAskBg input'); i.value = value; d.querySelector('.edAskBg [data-a="ok"]').click(); };

  // A prefab: kept from two objects, placed (their middle at the point) as one change.
  const r1 = { cell: 'Vault', origin: 'M.esm', refr: 1 }, r2 = { cell: 'Vault', origin: 'M.esm', refr: 2 };
  const hit = (ref, pos) => ({ refKey: E.refKeyOf(ref), id: 'gold_001', wpos: pos, rot: [0, 0, 1], scale: 1, m: [1, 0, 0, pos[0], 0, 1, 0, pos[1], 0, 0, 1, pos[2]] });
  const group = [{ hit: hit(r1, [0, 0, 0]), ref: r1 }, { hit: hit(r2, [100, 0, 0]), ref: r2 }];
  w.App.R.pickables = group.map(g => g.hit);
  E._group = group.slice();
  E.selectedRecord = () => ({ tag: 'MISC', id: 'gold_001' });
  const kept = F.keepPrefab(); await answerAsk('Stall'); await kept;
  t.ok(F.prefabs().Stall && F.prefabs().Stall.length === 2 && F.prefabs().Stall[1].offset[0] === 50, 'kept relative to the middle: ' + JSON.stringify(F.prefabs().Stall));
  await F.placePrefab('Stall', [1000, 2000, 10], 'My Room');
  const pm = p.posts.find(x => x[0] === 'pm');
  t.ok(pm && pm[1].cell === 'My Room' && JSON.stringify(pm[1].items.map(i => i.translation)) === '[[950,2000,10],[1050,2000,10]]', 'placed as one: ' + JSON.stringify(pm && pm[1]));

  // New cells.
  let made = F.newCell(true); await answerAsk('40,-12'); await made;
  t.ok(p.posts.some(x => x[0] === 'nc' && x[1].cell === '(40, -12)'), 'a new exterior square, its key as Wraithguard keys it');
  made = F.newCell(false); await answerAsk('My Room'); await made;
  t.ok(p.posts.some(x => x[0] === 'nc' && x[1].cell === 'My Room'), 'a new interior');

  // The condition editor, on a response.
  E.record = { tag: 'INFO', id: '123', plugins: null };
  E.dialog().hidden = false;
  E.drawRecord({ tag: 'INFO', id: '123', type: 'DialogueInfo', winner: 'M.esm', plugins: ['M.esm'], whole: false, new: false,
    fields: [{ path: 'filters', value: [{ index: 0, filter_type: 'NotRace', function: 'PcLevel', comparison: 'Equal', id: 'Khajiit', value: { type: 'Integer', data: 0 } }], editable: true, kind: 'list', options: [] }] });
  await p.wait(10);
  const conds = d.querySelector('.edConds');
  t.ok(conds && conds.querySelectorAll('.edCond').length === 1, 'the conditions as rows');
  conds.querySelector('#edCondAdd').click();
  const rows = conds.querySelectorAll('.edCond');
  t.ok(rows.length === 2 && rows[1].querySelector('[data-k="function"]'), 'a function condition added, its functions to choose from');
  const v = rows[1].querySelector('[data-k="value"]'); v.value = '5'; v.onchange();
  conds.querySelector('#edCondSave').click();
  await p.wait(5);
  const set = p.posts.find(x => x[0] === 'set' && x[1].path === 'filters');
  t.ok(set && set[1].value.length === 2 && set[1].value[1].value.data === 5 && set[1].value[1].index === 1, 'saved as the record holds them: ' + JSON.stringify(set && set[1].value));
  await F.whoCanSay(conds.querySelector('#edWhoOut'), '123');
  t.ok(/1 NPC/.test(conds.textContent) && /Journal MS_Q/.test(conds.textContent), 'who can say it, and what is not checked');

  // A leveled list rolled.
  E.drawRecord({ tag: 'LEVI', id: 'l_gold', type: 'LeveledItem', winner: 'M.esm', plugins: ['M.esm'], whole: false, new: false, fields: [] });
  const lv = d.querySelector('#edRollLv'); lv.value = '7';
  d.querySelector('#edRollGo').onclick();
  await p.wait(5);
  t.ok(p.posts.some(x => x[0] === 'roll' && x[1].level === 7) && /75\.0%/.test(d.querySelector('#edRollOut').textContent), 'rolled at level 7');

  // Script Edit: completion, uses.
  await E.openScript('SCPT', 'x', null);
  const ta = d.querySelector('#edScriptText');
  ta.value = 'begin x\nAddI'; ta.selectionStart = ta.selectionEnd = ta.value.length;
  ta.dispatchEvent(new w.KeyboardEvent('keydown', { key: ' ', ctrlKey: true, bubbles: true }));
  await p.wait(20);
  const pop = d.querySelector('.edComplete');
  t.ok(pop && !pop.hidden && pop.textContent.includes('AddItem'), 'Ctrl+Space completes: ' + (pop && pop.textContent));
  ta.dispatchEvent(new w.KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
  t.ok(ta.value.endsWith('AddItem'), 'Enter takes it: ' + JSON.stringify(ta.value));
  await F.scriptRefs(E.scr, 'myVar');
  t.ok(/OtherScript/.test(d.querySelector('#edScriptFind').textContent), 'scripts naming the word');

  // Search everything, replace in the checked.
  F.showSearch();
  d.querySelector('#edSaText').value = 'Gold';
  await F.runSearch();
  d.querySelector('#edSaBy').value = 'Coin';
  await F.runReplace();
  const ra = p.posts.find(x => x[0] === 'ra');
  t.ok(ra && ra[1].text === 'Gold' && ra[1].by === 'Coin' && ra[1].hits.length === 1, 'replaced in the checked: ' + JSON.stringify(ra && ra[1]));
  E.leave();
};
