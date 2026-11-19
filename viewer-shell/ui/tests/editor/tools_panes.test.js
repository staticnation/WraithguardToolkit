/* The Tools page's panes (61_wg_tools.js `build`): one a tool, each a child of the page itself,
   so picking a tool shows its pane. Static, 2026-11-18: an unclosed `<div>` put Check meshes'
   pane inside the Mesh optimizer's, hidden with it, and the page showed nothing. */
'use strict';
const { page, EDITOR, source } = require('./harness');

module.exports = async (t) => {
  const p = page({ modules: EDITOR, engine: () => ({}), answer: () => ({}) });
  const { w } = p;
  // The Load order page's escaper, which the Tools page borrows.
  w.eval("var loEsc=s=>String(s).replace(/[&<>\"']/g,c=>'&#'+c.charCodeAt(0)+';');");
  w.eval(source('src/61_wg_tools.js'));
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
};
