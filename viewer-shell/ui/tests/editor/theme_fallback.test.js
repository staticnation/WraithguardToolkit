/* The old settings' theme (19_settings.js `Themes.fallback`; Static, 2026-11-17): Wraithguard's
   own theme from its settings file, as `settings_seed` gives it, worn where nothing names one -
   and never over a theme already chosen, nor a code the page has no block for. */
'use strict';
const { page, EDITOR, source } = require('./harness');

module.exports = async (t) => {
  const p = page({ modules: EDITOR, engine: () => ({}), answer: () => ({}) });
  const { w } = p;
  w.eval(source('src/19_settings.js'));
  w.eval(source('src/19_wg_themes.js'));
  const T = w.Themes;
  const tick = () => new Promise(r => setTimeout(r, 0));
  let seed = { theme: 'wg-dark-default' };
  w.WgSettings = { seed: () => Promise.resolve(seed) };

  w.App.theme = ''; T.fallback(); await tick(); await tick();
  t.ok(w.App.theme === 'wg-dark-default' && w.document.body.dataset.theme === 'wg-dark-default',
    'nothing named: the old settings\' theme is worn (' + w.App.theme + ')');

  w.App.theme = 'telvanni'; T.fallback(); await tick(); await tick();
  t.ok(w.App.theme === 'telvanni', 'a theme already chosen stands');

  w.App.theme = ''; seed = { theme: 'wg-no-such-theme' }; T.fallback(); await tick(); await tick();
  t.ok(w.App.theme === '', 'a code the page has no block for is not worn');

  seed = JSON.stringify({ theme: 'wg-dark-default' }); T.fallback(); await tick(); await tick();
  t.ok(w.App.theme === 'wg-dark-default', 'the seed as text, as the engine may answer it');

  w.App.theme = ''; seed = { theme: null }; T.fallback(); await tick(); await tick();
  t.ok(w.App.theme === '', 'old settings with no theme: the one it ships with');
};
