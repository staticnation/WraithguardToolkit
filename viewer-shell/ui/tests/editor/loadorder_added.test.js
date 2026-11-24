/* The Load order page (59_wg_loadorder.js): a plugin dropped in after the sort is sent with
   Export as the person's (`addedPlugins`), so the customizations TOML gets its insert and the
   next Sort keeps it (Static 2026-11-23: "drag in a mod, export, get the save message, then hit
   sort and the mod ... is now gone"). */
'use strict';
const { page } = require('./harness');

module.exports = async (t) => {
  let sent = null;
  const p = page({ modules: ['src/13_mo2.js', 'src/59_wg_loadorder.js'], engine: (c, a) => { if (c === 'loadorder_export') sent = [c, JSON.parse(a.args)]; return {}; } });
  const L = p.w.WgLoadOrder;
  L.build();
  p.d.querySelector('#lo_cfg').value = 'C:/Docs/openmw.cfg';
  p.d.querySelector('#lo_customizations').value = 'C:/Docs/customizations.toml';
  L.plan = { finalOrder: ['Morrowind.esm', 'A.esp'], dataOrder: [], subset: ['A.esp'] };
  L.content = [{ name: 'Morrowind.esm', on: true }, { name: 'Dropped.esp', on: true, isNew: true }, { name: 'Off.esp', on: false, isNew: true }, { name: 'A.esp', on: true, mine: true }];
  L.data = [];
  const toasts = []; p.w.toast = (m) => toasts.push(m);
  p.w.eval("var unpackTables=x=>x;");
  p.w.eval("Engine.bytes=async (c,a)=>{ window.__loSent=[c,a]; throw new Error('stop here'); };");
  await L.exportPlan();
  if (p.w.__loSent) sent = [p.w.__loSent[0], JSON.parse(p.w.__loSent[1].args)];
  t.ok(sent && sent[0] === 'loadorder_export', 'Export asks for the export ' + JSON.stringify(toasts));
  t.ok(JSON.stringify(sent[1].addedPlugins) === '["Dropped.esp"]', 'the plugin dropped in (and ticked on) is sent as the person\'s: ' + JSON.stringify(sent[1].addedPlugins));
  t.ok(sent[1].finalOrder.includes('Dropped.esp') && !sent[1].finalOrder.includes('Off.esp'), 'and is in the order written');
};
