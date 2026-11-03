/* The Dialogue window's In game tab (52_wg_dialogue_views.js): the cell where the speaker
   is placed (or the Preview's), the player and the game sent with every line asked, the
   player, globals and quests from a save, and the globals shared with the Scripts panel. */
'use strict';
const { JSDOM } = require('jsdom');
const fs = require('fs');
const path = require('path');

module.exports = async (t) => {
  const dom = new JSDOM('<!doctype html><body></body>', { runScripts: 'outside-only', pretendToBeVisual: true });
  const w = dom.window, d = w.document;
  const toasts = [];
  w.__toast = m => toasts.push(m);
  w.eval('var escHtml=s=>String(s==null?"":s).replace(/[&<>"\']/g,c=>"&#"+c.charCodeAt(0)+";"); var toast=m=>__toast(m);');
  w.eval('var App={_scene:{centre:{kind:"int", name:"Balmora, Guild of Mages"}, cells:[]}};');
  w.eval(`var WgScripts={state:{globals:new Map(), journal:new Map()}, actors:new Map([['player',{items:new Map([['gold_001',40]])}]]),
    async loadGlobals(){ if(!this.state.globals.size) this.state.globals.set('pcrace',{name:'PCRace',kind:'s',v:0}); }, panel(){}, drawState(){}, _el:{}};`);
  w.eval(`var Engine={ async pick(){ return 'C:/saves/Mage.omwsave'; }, async call(c){ if(c==='dialogue_state_from_save') return {name:'Barelos Mothran', sex:'Male', race:'Dark Elf', class:'Mage', level:3,
    factions:[['Mages Guild', 1]], globals:[{name:'PCRace', kind:'s', value:5}, {name:'GameHour', kind:'f', value:9.5}], journal:[['CS_BasicChargen', 190]]}; return {}; } };`);
  w.eval('var WgEditor={recordRows: async tag=>({rows: tag==="RACE"? [["Dark Elf"],["Nord"]] : [["Mages Guild"],["Fighters Guild"]]})};');
  w.eval(fs.readFileSync(path.join(__dirname, '..', '..', 'src', '52_wg_dialogue_views.js'), 'utf8').replace(/^const WgDial=/m, 'var WgDial='));
  const D = w.WgDial;

  const asked = [];
  const E = {
    dial: d.createElement('div'), cells: [{ kind: 'ext', x: -2, y: -9, name: 'Seyda Neen' }, { kind: 'int', name: 'Balmora, Guild of Mages' }],
    ask: async (k, b) => {
      asked.push([k, b]);
      if (k === 'editUses') return { uses: [{ type: 'Cell', key: '(-2, -9)', wins: true }, { type: 'NPC', key: 'x', wins: true }] };
      if (k === 'editDialoguePlay') return { speaker: { id: 'fargoth', name: 'Fargoth' }, topics: ['latest rumors'], line: null };
      return {};
    },
    err: e => String(e), openRecord() {},
  };
  E.dial.innerHTML = '<div class="orihead"><span style="flex:1"></span></div><div class="oribody"></div>';
  d.body.appendChild(E.dial);
  D.attach(E);
  await D.show('play');
  await new Promise(r => setTimeout(r, 5));
  const $ = s => d.querySelector(s);
  t.ok($('#edPCellFrom') && $('#edPSex') && $('#edPNew').checked, 'the cell choice and the player\'s boxes, a new game by default');
  t.ok([...d.querySelectorAll('#edPCells option')].map(o => o.value).join() === 'Balmora, Guild of Mages,Seyda Neen', 'the cells to pick from');
  t.ok(/1 globals/.test($('#edPGameSum').textContent), 'the load order\'s globals, loaded for the conditions: ' + $('#edPGameSum').textContent);

  // Where they are: Fargoth is placed in (-2, -9), Seyda Neen.
  $('#edPSpeaker').value = 'fargoth';
  $('#edPCellFrom').value = 'npc';
  await $('#edPCellFrom').onchange();
  t.ok($('#edPCell').value === 'Seyda Neen', 'the cell where they are: ' + $('#edPCell').value);
  $('#edPSex').value = 'Female'; $('#edPLevel').value = '7'; $('#edPSex').onchange();
  await $('#edPGreet').onclick();
  await new Promise(r => setTimeout(r, 5));
  const play = asked.filter(a => a[0] === 'editDialoguePlay').pop();
  t.ok(play && play[1].cell === 'Seyda Neen' && play[1].state.pcSex === 'Female' && play[1].state.pcLevel === 7 && play[1].state.newGame === true, 'the line asked with the cell and the player: ' + JSON.stringify(play && play[1]));
  t.ok(play[1].state.globals.pcrace === 0 && play[1].state.items.gold_001 === 40, 'and the globals and what the player carries');

  // The Preview's cell.
  $('#edPCellFrom').value = 'current';
  await $('#edPCellFrom').onchange();
  t.ok($('#edPCell').value === 'Balmora, Guild of Mages', 'the Preview\'s cell');

  // From a save: the player into the boxes, globals and quests into the shared state.
  await $('#edPSave').onclick();
  t.ok($('#edPSex').value === 'Male' && $('#edPRace').value === 'Dark Elf' && $('#edPFaction').value === 'Mages Guild' && $('#edPRank').value === '1' && $('#edPLevel').value === '3', 'the player from the save');
  t.ok(!$('#edPNew').checked, 'a save is not a new game');
  const S = w.WgScripts.state;
  t.ok(S.globals.get('pcrace').v === 5 && S.globals.get('gamehour').v === 9.5 && S.journal.get('cs_basicchargen') === 190, 'globals and the journal shared with the Scripts panel');
  t.ok(toasts.some(m => /Barelos Mothran/.test(m)), 'and said');
};
