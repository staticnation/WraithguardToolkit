/* The Load order and Conflicts pages' batch of 2026-11-24 (Static: "lets do all this";
   88_wg_lo_qol.js): Why here?, snapshots compared and restored, and the reviewed marks on the
   Conflicts lists, on the pages themselves (59_wg_loadorder.js, 60_wg_conflicts.js). */
'use strict';
const { JSDOM } = require('jsdom');
const fs = require('fs');
const path = require('path');

module.exports = async (t) => {
  const src = f => fs.readFileSync(path.join(__dirname, '..', '..', 'src', f), 'utf8');
  const dom = new JSDOM('<!doctype html><body></body>', { runScripts: 'outside-only', pretendToBeVisual: true, url: 'http://tauri.localhost/' });
  const w = dom.window, d = w.document;
  w.eval('var toasts=[]; var toast=(m)=>toasts.push(m); var $=s=>document.querySelector(s); var wgConfirm=async()=>true;' +
    'var asked=[]; var Engine={call:async(c,a)=>{ asked.push([c,a]); if(c==="loadorder_why") return JSON.stringify({name:a.name, at:2, sorted:true, mine:true, curated:false, origin:"customizations.toml",' +
    ' anchor:{how:"after", other:"A.esp"}, masters:[{name:"Morrowind.esm", at:0},{name:"C.esp", at:3}], refused:["mlox wanted C before B"],' +
    ' rules:[{file:"mlox_base.txt", line:12, kind:"order", names:["A.esp","B.esp","C.esp"], before:["A.esp"], after:["C.esp"], broken:[]},' +
    ' {file:"my_rules.txt", line:3, kind:"order", names:["C.esp","B.esp"], before:["C.esp"], after:[], broken:["C.esp"]}]}); return null; }};');
  w.eval(src('59_wg_loadorder.js').replace(/^const WgLoadOrder=/m, 'var WgLoadOrder=').replace(/^const WgSettings=/m, 'var WgSettings='));
  w.eval(src('60_wg_conflicts.js').replace(/^const WgConflicts=/m, 'var WgConflicts='));
  w.eval(src('88_wg_lo_qol.js').replace(/^const WgLoQol=/m, 'var WgLoQol='));
  const L = w.WgLoadOrder, C = w.WgConflicts, Q = w.WgLoQol;

  // Why here?: the button over content=, and the answer drawn.
  L.build();
  t.ok(d.getElementById('loWhy') && d.getElementById('loSnaps'), 'the two buttons over content=');
  d.getElementById('lo_cfg').value = 'C:/omw/openmw.cfg';
  d.getElementById('lo_rules').value = 'mlox_base.txt\nmy_rules.txt';
  L.content = ['Morrowind.esm', 'A.esp', 'B.esp', 'C.esp', 'D.esp'].map(n => ({ name: n, on: true }));
  L.data = [];
  L.drawOrders();
  d.querySelector('#loContent li[data-i="2"] span').dispatchEvent(new w.MouseEvent('dblclick', { bubbles: true }));
  await new Promise(r => setTimeout(r, 10));
  const why = w.asked.find(x => x[0] === 'loadorder_why');
  t.ok(why && why[1].name === 'B.esp' && why[1].order.length === 5 && why[1].rules.length === 2 && why[1].cfg === 'C:/omw/openmw.cfg', 'asked with the page\'s order and rules');
  const box = d.getElementById('loWhyBox'), text = box.textContent;
  t.ok(/loads 3 of 5, after A\.esp and before C\.esp/.test(text), 'where it loads: ' + text.slice(0, 80));
  t.ok(/right after A\.esp/.test(text) && /customizations\.toml/.test(text), 'the anchor and where it came from');
  t.ok(/mlox_base\.txt, line 12/.test(text) && /Not followed for C\.esp/.test(text), 'the rules, one not followed');
  t.ok(/C\.esp: loads after it/.test(text), 'a master after it named');
  t.ok(box.querySelectorAll('a').length >= 6, 'the plugins as links');

  // Snapshots: kept, compared, restored.
  const s = Q.snap('Before', false);
  t.ok(Q.snaps().length === 1 && s.content.length === 5, 'kept for this cfg');
  L.content = ['Morrowind.esm', 'C.esp', 'A.esp', 'B.esp', 'E.esp'].map(n => ({ name: n, on: n !== 'B.esp' }));
  const df = Q.diff(s);
  t.ok(JSON.stringify(df.added) === '["E.esp"]' && JSON.stringify(df.removed) === '["D.esp"]' && JSON.stringify(df.off) === '["B.esp"]', 'added, taken out, ticked off: ' + JSON.stringify(df));
  t.ok(df.moved.length === 1 && df.moved[0].name === 'C.esp' && df.moved[0].from === 4 && df.moved[0].to === 2, 'one moved, not three');
  Q.restore(s);
  t.ok(L.content.map(r => r.name + (r.on ? '' : '-')).join() === 'Morrowind.esm,A.esp,B.esp,C.esp,D.esp,E.esp-', 'put back, the new one at the end and off: ' + L.content.map(r => r.name).join());
  for (let i = 0; i < 12; i++) Q.snap('auto ' + i, true);
  t.ok(Q.snaps().filter(x => x.auto).length === 10 && Q.snaps().some(x => x.name === 'Before'), 'the automatic ones kept to ten, the named one kept');
  d.getElementById('lo_cfg').value = 'other.cfg';
  t.ok(Q.snaps().length === 0, 'another cfg has its own');

  // Conflicts: reviewed marks.
  C.build();
  C.list = [{ type: 'NPC_', id: 'fargoth', plugins: ['Morrowind.esm', 'A.esp'], winner: 'A.esp', involvesSubset: true, deletedBy: [] },
    { type: 'CELL', id: 'Balmora', plugins: ['Morrowind.esm', 'B.esp'], winner: 'B.esp', involvesSubset: false, deletedBy: [] }];
  C.files = [{ path: 'meshes/x.nif', providers: ['D1', 'D2'], winner: 'D2', involvesSubset: false, identical: false }];
  C.draw();
  t.ok(d.getElementById('cfRev') && d.querySelectorAll('#cfList .cfRevMk').length === 2 && d.querySelectorAll('#cfFList .cfRevMk').length === 1, 'a mark on each row, and the filter');
  d.querySelector('#cfList .cfRevMk').click();
  t.ok(Q.stateRec(Q.rev(), C.list[0]) === 'yes' && d.querySelector('#cfList .cfRevMk').textContent === '✓', 'marked');
  d.getElementById('cfRev').value = 'no'; C.draw();
  t.ok(d.querySelectorAll('#cfList div.r').length === 1 && /1 of 2/.test(d.getElementById('cfCount').textContent), 'not reviewed: one of two');
  C.list[0].plugins.push('Z.esp');
  d.getElementById('cfRev').value = 'changed'; C.draw();
  t.ok(d.querySelectorAll('#cfList div.r').length === 1 && d.querySelector('#cfList .cfRevMk').textContent === '↻', 'a new plugin touches it: changed since');
  d.getElementById('cfRev').value = 'all'; C.tab = 'files'; C.draw(); Q.markShown();
  t.ok(Q.stateFile(Q.rev(), C.files[0]) === 'yes', 'the files shown, marked');
  t.ok(C.list.length === 2 && C.files.length === 1, 'the page\'s lists themselves untouched');
};
