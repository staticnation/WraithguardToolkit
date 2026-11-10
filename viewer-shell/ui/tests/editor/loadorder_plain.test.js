/* The Load order page without MOMW: an MO2 or Morrowind.ini install, or an openmw.cfg with
   "Plain mlox". What each mode shows, and what Sort and Export send. */
'use strict';
const { page } = require('./harness');

module.exports = async (t) => {
  const calls = [];
  const sorted = (kind) => ({ kind, was: ['Morrowind.esm', 'A.esp', 'B.esp'], order: ['Morrowind.esm', 'B.esp', 'A.esp'],
    moved: [{ name: 'B.esp', from: 3, to: 2 }, { name: 'A.esp', from: 2, to: 3 }], conflicts: [], notes: [],
    data: kind === 'openmw' ? ['/g/base', '/g/b', '/g/a'] : [], dataMoved: kind === 'openmw' ? [{ name: '/g/b', from: 3, to: 2 }] : [] });
  const p = page({
    modules: ['src/13_mo2.js', 'src/59_wg_loadorder.js'],
    engine: (c, a) => {
      calls.push([c, a]);
      if (c === 'loadorder_plain_sort') return sorted(String(a.cfg).endsWith('.cfg') ? 'openmw' : String(a.cfg).startsWith('mo2:') ? 'mo2' : 'vanilla');
      if (c === 'loadorder_plain_write') return { lines: ['written'] };
      if (c === 'loadorder_rule_ages') return [];
      return {};
    },
  });
  const { d } = p;
  const L = p.w.WgLoadOrder;
  L.build();
  const $ = (s) => d.querySelector(s);
  const shown = (k) => !d.querySelector(`#wgLO [data-f="${k}"]`).hidden;
  const set = (v) => { $('#lo_cfg').value = v; $('#wgLO').dispatchEvent(new p.w.Event('input')); };

  set('C:/Games/Morrowind/Morrowind.ini');
  t.ok(L.mode() === 'install' && !shown('customizations') && !shown('groundcover') && !shown('plainMlox') && shown('rules') && shown('dryRun'), 'a game install: the OpenMW and MOMW fields hidden');
  t.ok(!$('#loRestore').hidden && $('#loUpYml').hidden && !$('#loMode').hidden, 'Restore dates shown, the MOMW downloads hidden, the mode said');
  $('#lo_rules').value = 'mlox_base.txt';
  await L.sort();
  t.ok(L.content.map(r => r.name).join() === 'Morrowind.esm,B.esp,A.esp', 'sorted: ' + L.content.map(r => r.name).join());
  t.ok(/game reads its one Data Files/.test($('#loData').textContent), 'no data= lines for the game');
  L.content[2].on = false;
  await L.exportPlan();
  const w = calls.filter(c => c[0] === 'loadorder_plain_write').pop()[1];
  t.ok(w.off.join() === 'A.esp' && w.order.length === 3 && !('tomlTo' in w), 'Export sends the order and the plugin ticked off: ' + JSON.stringify(w));

  set('mo2:C:/MO2|Main');
  t.ok(L.mode() === 'install' && $('#loRestore').hidden, 'MO2: no Restore dates (MO2 orders by loadorder.txt)');

  set('C:/Users/me/Documents/My Games/OpenMW/openmw.cfg');
  t.ok(L.mode() === 'momw' && shown('pluginOrderYml') && shown('groundcover') && $('#loMode').hidden, 'an openmw.cfg: every field, as before');
  $('#lo_plainMlox').checked = true; $('#wgLO').dispatchEvent(new p.w.Event('change'));
  t.ok(L.mode() === 'plaincfg' && shown('customizations') && shown('sortDataPaths') && !shown('pluginOrderYml') && !shown('groundcover'), 'Plain mlox: the cfg, the TOML, Sort data paths; not the list');
  $('#lo_sortDataPaths').checked = true;
  await L.sort();
  const s = calls.filter(c => c[0] === 'loadorder_plain_sort').pop()[1];
  t.ok(s.sortData === true, 'Sort data paths asked');
  t.ok(L.data.map(r => r.line).join() === '/g/base,/g/b,/g/a', 'the data= folders as sorted');
  $('#lo_customizations').value = 'C:/momw-customizations.toml';
  $('#lo_writeCfg').checked = false;
  await L.exportPlan();
  const w2 = calls.filter(c => c[0] === 'loadorder_plain_write').pop()[1];
  t.ok(w2.writeCfg === false && w2.tomlTo === 'C:/momw-customizations.toml' && w2.tomlFrom === w2.tomlTo && w2.data.length === 3, 'Export: the TOML back into customizations.toml, the cfg not written: ' + JSON.stringify(w2));
};
