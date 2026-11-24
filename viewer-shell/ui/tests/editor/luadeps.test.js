/* The Lua mods check's missing dependencies (89_wg_luadeps.js, Static 2026-11-24): what the
   findings need gathered once each, looked for (`lua_find`), turned on through the Load order
   page as a drop, and what is not installed offered as a Nexus search. */
'use strict';
const { JSDOM } = require('jsdom');
const fs = require('fs');
const path = require('path');

module.exports = async (t) => {
  const src = f => fs.readFileSync(path.join(__dirname, '..', '..', 'src', f), 'utf8');
  const dom = new JSDOM('<!doctype html><body></body>', { runScripts: 'outside-only', pretendToBeVisual: true, url: 'http://tauri.localhost/' });
  const w = dom.window, d = w.document;
  w.eval('var toast=()=>{}; var $=s=>document.querySelector(s); var asked=[]; var dropped=[]; var opened=[];' +
    'var Engine={call:async(c,a)=>{ asked.push([c,a]);' +
    '  if(c==="lua_find") return a.needs.map(([k,n])=>({kind:k, name:n, found: n==="Perks"? [{content:"Perks.omwscripts", path:"D:/data/Perks.omwscripts", data:null, via:"a script it registers provides I.Perks"}]' +
    '    : n==="scripts.ern.util"? [{content:null, path:"D:/mods/Ern", data:"D:/mods/Ern", via:"the folder has D:/mods/Ern/scripts/ern/util.lua"}] : []}));' +
    '  if(c==="open_web"){ opened.push(a.url); return null; } return null; }};' +
    'var WgLoadOrder={on:false, plan:null, enter(){ this.on=true; setTimeout(()=>{ this.plan={}; }, 5); }, saved(){ return {}; }, val(){ return ""; },' +
    '  async dropDo(paths, where){ dropped.push([paths[0], where.kind]); }};');
  w.eval(src('80_wg_luacheck.js').replace(/^const WgLuaCheck=/m, 'var WgLuaCheck='));
  w.eval(src('89_wg_luadeps.js').replace(/^const WgLuaDeps=/m, 'var WgLuaDeps='));
  const C = w.WgLuaCheck, D = w.WgLuaDeps;
  const data = { mods: [
    { name: 'A.omwscripts', status: 'dependency', scripts: ['x'], found: [
      { class: 'dependency', about: 'interface Perks', text: 'attempt to index a nil value (field \'Perks\')' },
      { class: 'dependency', about: 'module ', text: 'module \'scripts.ern.util\' not found' }] },
    { name: 'B.omwscripts', status: 'plugin', scripts: ['y'], found: [
      { class: 'dependency', about: 'interface Perks', text: '...' },
      { class: 'plugin', about: 'global ern_ready', text: 'no global variable ern_ready' },
      { class: 'runtime', about: '', text: 'something else' }] }], gaps: [], lint: [] };

  // What is needed, once each, with who needs it.
  const needs = D.needs(data);
  t.ok(needs.length === 3, 'three needs: ' + JSON.stringify(needs));
  t.ok(needs[0].kind === 'interface' && needs[0].name === 'Perks' && needs[0].mods.join() === 'A.omwscripts,B.omwscripts', 'an interface two mods need, once');
  t.ok(needs[1].kind === 'module' && needs[1].name === 'scripts.ern.util', 'a module named only in the message text');
  t.ok(needs[2].kind === 'global' && needs[2].name === 'ern_ready', 'a plugin\'s global');

  // Drawn at the top of the findings once looked for.
  const host = d.createElement('div'); host.append(C.pane()); d.body.append(host);
  C.data = data; C.draw();
  await new Promise(r => setTimeout(r, 20));
  const box = d.getElementById('tlLuaDeps');
  t.ok(box && d.getElementById('tlLuaBody').firstChild === box, 'the list is at the top');
  t.ok(/Turn on all 2 found/.test(box.textContent) && box.querySelectorAll('.warn').length === 3, 'three needs, two found');
  t.ok(/Perks\.omwscripts is installed and not turned on/.test(box.textContent) && /not in data= yet/.test(box.textContent), 'what each found is');
  const lf = w.asked.find(x => x[0] === 'lua_find');
  t.ok(lf && lf[1].needs.length === 3, 'looked for in one call');

  // Turn on: through the Load order page, as a drop.
  const all = [...box.querySelectorAll('button')].find(b => /Turn on all/.test(b.textContent));
  all.click();
  await new Promise(r => setTimeout(r, 400));
  t.ok(w.WgLoadOrder.on && JSON.stringify(w.dropped) === JSON.stringify([['D:/data/Perks.omwscripts', 'content'], ['D:/mods/Ern', 'data']]), 'the plugin into content=, the folder into data=: ' + JSON.stringify(w.dropped));

  // Not installed: a Nexus search, by a word from its name.
  const srch = [...box.querySelectorAll('button')].find(b => b.textContent === 'Search Nexus');
  srch.click();
  await new Promise(r => setTimeout(r, 5));
  t.ok(w.opened[0] === 'https://www.nexusmods.com/games/morrowind/mods?keyword=ern%20ready', 'the search: ' + w.opened[0]);
  t.ok(D.word({ kind: 'module', name: 'scripts.SkillFramework.api' }) === 'api' && D.word({ kind: 'interface', name: 'SkillFramework' }) === 'Skill Framework', 'the word searched for');

  // The same run drawn again is not looked for again.
  const n = w.asked.filter(x => x[0] === 'lua_find').length;
  C.draw(); await new Promise(r => setTimeout(r, 10));
  t.ok(w.asked.filter(x => x[0] === 'lua_find').length === n && d.getElementById('tlLuaDeps'), 'drawn again from what was found');
};
