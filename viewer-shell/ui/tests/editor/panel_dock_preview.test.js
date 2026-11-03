/* A Preview panel docked on the right outside the Editor (51_wg_editor_ui.js): Static,
   2026-11-03, "scripts panel disappeared and didn't return when trying to dock on right by
   drag". The rail only showed in the Editor, so the Scripts panel docked into nothing and
   stayed there. Now the rail shows outside the Editor while a panel of the Preview's own
   (`data-anywhere`) is open in it; an Editor panel docked there stays hidden outside it. */
'use strict';
const fs = require('fs');
const path = require('path');
const { page, EDITOR } = require('./harness');

module.exports = async (t) => {
  const p = page({ modules: EDITOR, engine: () => ({}), answer: () => ({}) });
  const { w, d } = p;
  // The page's own styles, for what is shown.
  const html = fs.readFileSync(path.join(__dirname, '..', '..', 'viewer_only.html'), 'utf8');
  for (const m of html.matchAll(/<style[^>]*>([\s\S]*?)<\/style>/g)) { const st = d.createElement('style'); st.textContent = m[1]; d.head.appendChild(st); }
  if (!d.getElementById('main')) { const m = d.createElement('div'); m.id = 'main'; d.body.appendChild(m); }
  d.body.classList.remove('wgEditMode');
  const U = w.WgUI;
  const shown = el => w.getComputedStyle(el).display !== 'none';

  const scripts = d.createElement('div');
  scripts.id = 'wgScrPanel'; scripts.className = 'ori'; scripts.dataset.anywhere = '';
  scripts.innerHTML = '<div class="orihead"><b>Scripts</b></div><div class="oribody">x</div>';
  U.mount(scripts);
  U.place(scripts, 'right');
  await new Promise(r => setTimeout(r, 0));
  const rail = d.getElementById('edRail');
  t.ok(rail && scripts.parentNode === rail, 'the Scripts panel is in the rail');
  t.ok(shown(rail) && shown(scripts), 'and shown outside the Editor');

  // An Editor panel docked there too: hidden outside the Editor, the rail still shown.
  const pend = d.createElement('div');
  pend.id = 'edPendingPanel'; pend.className = 'ori';
  pend.innerHTML = '<div class="orihead"><b>Pending</b></div><div class="oribody">y</div>';
  U.mount(pend);
  U.place(pend, 'right');
  await new Promise(r => setTimeout(r, 0));
  t.ok(!shown(pend) && shown(scripts), 'an Editor panel stays hidden outside the Editor');

  // Scripts closed: nothing of the Preview's open, the rail goes.
  scripts.hidden = true; U.syncRail();
  t.ok(!shown(rail), 'with the Scripts panel closed the rail is gone');
  // And opened again: back where it was docked.
  scripts.hidden = false; U.syncRail();
  t.ok(shown(rail) && shown(scripts), 'opened again, it is there');
  // In the Editor, both.
  d.body.classList.add('wgEditMode');
  t.ok(shown(rail) && shown(pend) && shown(scripts), 'in the Editor both are shown');
};
