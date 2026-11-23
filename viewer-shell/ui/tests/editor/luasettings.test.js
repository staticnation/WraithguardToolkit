/* Lua settings (81_wg_luasettings.js, Static 2026-11-23): a storage file's sections drawn, a
   section opened, a value changed and saved - after the confirmation, the one value sent - and
   a value the page cannot edit kept as it is. */
'use strict';
const { page, EDITOR, source } = require('./harness');

module.exports = async (t) => {
  const sent = [];
  const data = { player: { path: 'C:/OpenMW/player_storage.bin', backup: false, sections: {
    Horizontal_Compass_Settings: { stylePath: '1', compassScale: 100, showCompass: true, pos: { $v2: [0.5, 0.05] }, list: ['a'], obj: { $raw: '9261' } },
    SettingsOther: { x: 1 } } }, global: { path: 'C:/OpenMW/global_storage.bin', sections: null } };
  const p = page({ modules: EDITOR, engine: (c, a) => { if (c === 'lua_settings_read') return data; if (c === 'lua_settings_write') { sent.push(a); return { path: 'x' }; } return {}; }, answer: () => ({}) });
  const { w } = p;
  const d = w.document;
  let asked = '';
  w.wgConfirm = async m => { asked = m; return true; };
  w.toast = () => {};
  w.eval(source('src/81_wg_luasettings.js').replace(/^const WgLuaSettings=/m, 'var WgLuaSettings='));
  const S = w.WgLuaSettings;
  d.body.appendChild(S.pane()); S.wire();
  await S.load();
  const body = d.getElementById('tlLsBody');
  t.ok(/Horizontal_Compass_Settings/.test(body.textContent) && /SettingsOther/.test(body.textContent), 'the sections are listed');
  S.open.add('Horizontal_Compass_Settings'); S.draw();
  const rows = [...body.querySelectorAll('div')].filter(r => r.firstChild && r.firstChild.textContent === 'compassScale');
  const num = rows[0].querySelector('input[type=number]'), save = rows[0].querySelector('button');
  t.ok(num && save && save.disabled, 'a number has its editor; Save waits for a change');
  num.value = '120'; num.oninput();
  await save.onclick();
  t.ok(/Close OpenMW first/.test(asked) && /wg-backup/.test(asked), 'the confirmation says to close OpenMW and where the original goes');
  t.ok(sent.length === 1 && sent[0].section === 'Horizontal_Compass_Settings' && sent[0].key === 'compassScale' && sent[0].value === 120 && sent[0].which === 'player', 'the one value sent ' + JSON.stringify(sent));
  const raw = [...body.querySelectorAll('div')].filter(r => r.firstChild && r.firstChild.textContent === 'obj')[0];
  t.ok(raw && !raw.querySelector('button') && /kept as it is/.test(raw.textContent), 'a value the page cannot edit is shown, not offered for change');
  d.getElementById('tlLsWhich').value = 'global'; d.getElementById('tlLsWhich').onchange();
  t.ok(/no global_storage.bin/.test(body.textContent), 'a file not there yet is said so');
};
