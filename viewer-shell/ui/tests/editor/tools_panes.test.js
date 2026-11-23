/* The Tools page's panes (61_wg_tools.js `build`): one a tool, each a child of the page itself,
   so picking a tool shows its pane. Static, 2026-11-18: an unclosed `<div>` put Check meshes'
   pane inside the Mesh optimizer's, hidden with it, and the page showed nothing. And Merge
   objects' file (Static, 2026-11-19: "let me select where to save these files"): the default
   folder shown, Save as setting the folder and the name, the name sent, the path confirmed. */
'use strict';
const { page, EDITOR, source, wgtb } = require('./harness');

module.exports = async (t) => {
  const p = page({ modules: EDITOR, engine: (c) => c === 'land_merge_home' ? { home: 'E:\\OpenMW\\Data Files', ignore: [] } : ({}), answer: () => ({}) });
  const { w } = p;
  // The Load order page's escaper, which the Tools page borrows.
  w.eval("var loEsc=s=>String(s).replace(/[&<>\"']/g,c=>'&#'+c.charCodeAt(0)+';');");
  w.eval(source('src/61_wg_tools.js'));
  w.eval(source('src/80_wg_luacheck.js').replace(/^const WgLuaCheck=/m, 'var WgLuaCheck='));
  w.eval(source('src/81_wg_luasettings.js').replace(/^const WgLuaSettings=/m, 'var WgLuaSettings='));
  const T = w.WgToolsPage;
  T.build();
  const root = w.document.getElementById('wgTL');
  t.ok(!!root, 'the page is built');
  const panes = [...root.querySelectorAll('.pane')];
  const strays = panes.filter(el => el.parentElement !== root).map(el => el.id + ' in ' + (el.parentElement.closest('.pane') || el.parentElement).id);
  t.ok(strays.length === 0, 'every pane is the page\'s own child' + (strays.length ? ' (not: ' + strays.join(', ') + ')' : ''));
  for (const [k] of T.TOOLS) {
    try { T.show(k); } catch (_) { /* a tool's own start may want the engine; the panes are set first */ }
    const shown = panes.filter(el => !el.hidden && el.offsetParent !== undefined && !el.closest('.pane[hidden]'));
    t.ok(shown.length === 1, k + ': one pane shows (' + shown.map(el => el.id).join(', ') + ')');
  }

  // Merge objects: where the file goes, and its name.
  const $ = id => w.document.getElementById(id);
  const tick = () => new Promise(r => setTimeout(r, 0));
  T.show('mergeobjects'); await tick(); await tick();
  t.ok($('tlObFolder').placeholder === 'default: E:\\OpenMW\\Data Files', 'the default folder is shown, not described: ' + $('tlObFolder').placeholder);
  t.ok($('tlObRun').textContent === 'Build Merged Objects.esp', 'the button names the file');
  let asked = null;
  w.Engine.pick = async (kind, o) => { asked = [kind, o]; return 'D:\\Merges\\My Lists'; };
  $('tlObLists').checked = true; $('tlObLists').onchange();
  await $('tlObSaveAs').onclick();
  t.ok(asked && asked[0] === 'save' && asked[1].defaultPath === 'E:\\OpenMW\\Data Files\\multipatch.esp', 'Save as opens on where it would go ' + JSON.stringify(asked));
  t.ok($('tlObFolder').value === 'D:\\Merges' && $('tlObName').value === 'My Lists.esp' && $('tlObRun').textContent === 'Build My Lists.esp', 'and sets the folder and the name (.esp added)');
  let sent = null, said = '';
  w.wgConfirm = async m => { said = m; return true; };
  w.Engine.bytes = async (c, a) => { sent = [c, a]; return wgtb([['head', 'ssu', [['Wrote 3 merged records', 'D:\\Merges\\My Lists.esp', 3]]], ['lines', 's', []], ['types', 'su', [['LEVI', 3]]]]); };
  w.toast = () => {};
  await T.obRun();
  t.ok(said.includes('D:\\Merges\\My Lists.esp'), 'the confirm names the whole path: ' + JSON.stringify(said));
  t.ok(sent && sent[0] === 'tools_merge_objects' && sent[1].name === 'My Lists.esp' && sent[1].folder === 'D:\\Merges' && sent[1].listsOnly === true, 'the name and folder are sent ' + JSON.stringify(sent && sent[1]));
  $('tlObName').value = 'bad.txt'; sent = null;
  await T.obRun();
  t.ok(sent === null, 'a name that is not a plugin\'s is refused before anything runs');
};
