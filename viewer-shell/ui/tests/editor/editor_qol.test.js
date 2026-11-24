/* The Editor's batch of 2026-11-24 (Static: "lets do all this"): the command palette
   (85_wg_palette.js), back and forward, the changes field by field, Script Edit's completion
   and a response's placeholders (86_wg_editor_qol.js), and the Object Window's amounts
   (50_wg_editor_objects.js `byAmount`). */
'use strict';
const { JSDOM } = require('jsdom');
const fs = require('fs');
const path = require('path');

module.exports = async (t) => {
  const src = f => fs.readFileSync(path.join(__dirname, '..', '..', 'src', f), 'utf8');
  const dom = new JSDOM('<!doctype html><body></body>', { runScripts: 'outside-only', pretendToBeVisual: true, url: 'http://tauri.localhost/' });
  const w = dom.window, d = w.document;
  w.eval('var toast=()=>{}; var escHtml=s=>String(s); var WgUI={mount:el=>document.body.append(el)};' +
    'var calls=[]; var Engine={call:async(c,a)=>{ calls.push([c,a]); if(c==="record_search") return [{id:"fargoth",tag:"NPC_"},{id:"fargoth_ring",tag:"MISC"}]; if(c==="editor_record_tag") return JSON.stringify(a.id==="TimesFargoth"? {tag:"GLOB"} : {}); return []; }};' +
    'var opened=[]; var WgEditor={NAMES:{NPC_:"NPC"}, record:null, dlg:null,' +
    ' dialog(){ if(!this.dlg){ const d=document.createElement("div"); d.innerHTML=\'<div class="orihead"><b id="edDlgTitle">R</b></div><div id="edDlgBody"></div>\'; document.body.append(d); this.dlg=d; } return this.dlg; },' +
    ' async openRecord(tag,id){ opened.push(tag+":"+id); this.record={tag,id}; this.dialog(); },' +
    ' async openRef(ref){ opened.push("ref:"+ref.refr); this.dialog(); },' +
    ' drawRecord(v){ this.dialog().querySelector("#edDlgBody").innerHTML=\'<textarea data-path="text">\'+v.text+\'</textarea>\'; return true; },' +
    ' drawPending(){}, async openScript(){} };');
  w.eval(src('50_wg_editor_objects.js'));
  w.eval(src('86_wg_editor_qol.js').replace(/^const (WgEdHistory|WgEdDiff|WgScriptHelp|WgTextCheck)=/mg, 'var $1='));
  const E = w.WgEditor, H = w.WgEdHistory;

  // Amounts.
  const A = x => JSON.stringify(E.byAmount(x));
  t.ok(A('+10') === '{"op":"add","by":10}' && A('-2.5') === '{"op":"add","by":-2.5}', 'add: ' + A('+10'));
  t.ok(A('*1.25') === '{"op":"mul","by":1.25}' && A('x2') === '{"op":"mul","by":2}', 'multiply');
  t.ok(Math.abs(E.byAmount('+20%').by - 1.2) < 1e-9 && Math.abs(E.byAmount('-20%').by - 0.8) < 1e-9, 'a percentage multiplies');
  t.ok(E.byAmount('10') === null && E.byAmount('=+10') === null && E.byAmount('fargoth') === null, 'a plain value sets');

  // Back and forward.
  await E.openRecord('NPC_', 'fargoth');
  await E.openRecord('MISC', 'ring');
  await E.openRef({ refr: 7, origin: 'Morrowind.esm' });
  t.ok(H.back.length === 2 && !H.fwd.length, 'three opened, two behind');
  await H.go(-1);
  t.ok(w.opened[w.opened.length - 1] === 'MISC:ring' && H.back.length === 1 && H.fwd.length === 1, 'back opens the one before, and is not itself a new step');
  await H.go(-1); await H.go(1);
  t.ok(w.opened[w.opened.length - 1] === 'MISC:ring', 'forward again');
  await E.openRecord('MISC', 'ring');
  t.ok(H.back.length === 1, 'opening the same record again is not a step');
  await E.openRecord('BOOK', 'bk_a');
  t.ok(!H.fwd.length, 'a new record drops what was ahead');
  const bts = d.querySelectorAll('#edHist button');
  t.ok(bts.length === 2 && !bts[0].disabled && bts[1].disabled, 'the ◀ ▶ in the dialog\'s title');

  // The changes field by field.
  const box = d.createElement('div');
  w.WgEdDiff.fill(box, 'NPC_', 'fargoth', { fields: [{ path: 'name', value: 'Fargoth', queued: 'Fargoth the Brave' }, { path: 'level', value: 2 }] });
  t.ok(box.querySelectorAll('tr').length === 2 && /Fargoth the Brave/.test(box.textContent) && box.querySelector('button').textContent === '↶', 'one row per changed field, with what it was');

  // Script Edit: the script's own variables, and the word at the caret.
  t.ok(JSON.stringify(w.WgScriptHelp.locals('begin x\nshort doOnce\n  float Timer ; c\nlong n\nend')) === '["doOnce","Timer","n"]', 'the script\'s variables');
  const ta = d.createElement('textarea'); ta.value = 'if ( GetItemCount "farg'; ta.selectionStart = ta.selectionEnd = ta.value.length;
  t.ok(w.WgScriptHelp.wordAt(ta)[2] === '"farg', 'the word at the caret');
  d.body.append(ta);
  await w.WgScriptHelp.complete(ta);
  t.ok(w.WgScriptHelp._pop && w.WgScriptHelp._pop.items.length === 2, 'completions from the load order');
  w.WgScriptHelp.accept(ta);
  t.ok(ta.value === 'if ( GetItemCount "fargoth"', 'the quote kept: ' + ta.value);

  // A response's placeholders.
  const s = w.WgTextCheck.scan('Hello %PCName, the %pcrank of %Faction. %TimesFargoth %Bogus @Balmora#  @Vivec');
  t.ok(JSON.stringify(s.unknown) === '["TimesFargoth","Bogus"]', 'the defines pass: ' + JSON.stringify(s.unknown));
  t.ok(s.links.length === 1 && s.links[0] === 'Vivec', 'an @ without its #');
  E.record = { tag: 'INFO', id: '1' };
  E.drawRecord({ text: 'Hi %Bogus and %TimesFargoth' });
  await new Promise(r => setTimeout(r, 20));
  const note = d.querySelector('textarea[data-path="text"]').nextElementSibling;
  t.ok(/%Bogus/.test(note.textContent) && !/TimesFargoth/.test(note.textContent), 'a global variable passes, an unknown name is shown: ' + note.textContent);

  // The palette's matching.
  w.eval('var WgKeys={is:()=>false, ACTIONS:[]};');
  w.eval(src('85_wg_palette.js').replace(/^const WgPalette=/m, 'var WgPalette='));
  const P = w.WgPalette;
  t.ok(P.score('Load order', 'load') >= 0 && P.score('Load order', 'ord lo') >= 0 && P.score('Load order', 'conf') < 0, 'every word, any order');
  t.ok(P.score('Load order', 'load') < P.score('Unload all', 'load'), 'a match at the start ranks first');
};
