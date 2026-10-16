/* The toolbar's home: across the top of the window (in the page's own flow, not over the
   render window) until the user moves it - and a position the user chose is kept. */
'use strict';
const { page, EDITOR } = require('./harness');

module.exports = async (t) => {
  const p = page({
    modules: EDITOR,
    html: '<div id="app"><div id="main"><div id="center"><div id="vpwrap"></div></div></div></div>',
    links: { editRecord: 'r' },
    engine: (c) => c === 'editor_tags' ? [['STAT', 1]] : {},
    answer: () => ({}),
  });
  const { w, d } = p;
  const UI = w.WgUI;
  UI.toolbar();
  UI.applyToolbar();
  const bar = () => d.getElementById('edTbWin');
  t.ok(UI.tb.classList.contains('wintop'), 'the toolbar starts across the top of the window: ' + UI.tb.className);
  t.ok(bar() && UI.tb.parentNode === bar() && bar().nextSibling === d.getElementById('main'), 'in its own bar above the main area, not over the render window');
  t.ok(!d.getElementById('vpwrap').contains(UI.tb), 'the render window does not carry it');

  // A position the user chose is theirs.
  UI.dockToolbar('bottom');
  t.ok(UI.tb.classList.contains('bottom') && d.getElementById('vpwrap').contains(UI.tb), 'a chosen position is kept');
  // The grip's double-click puts it back at the home position.
  UI.dockToolbar(UI.TB_HOME);
  t.ok(UI.tb.classList.contains('wintop') && UI.tb.parentNode === bar(), 'and back to the top of the window');
};
