/* The Scripts panel's game state (73_wg_scripts.js `drawState`): a script that changes things
   every frame no longer has it rebuilt every frame (Static, 2026-11-19: "part above journal
   below globals wont scroll" - a list rebuilt under the wheel never scrolls). Redraws are
   gathered up, at most every 400 ms, held while the pointer is over the box, and done once it
   leaves. */
'use strict';
const vm = require('vm');
const fs = require('fs');
const path = require('path');

module.exports = async (t) => {
  const UI = path.join(__dirname, '..', '..');
  let now = 0, hover = false;
  const timers = [];
  const box = { matches: q => q === ':hover' && hover };
  const ctx = {
    console, Math, JSON, Map, Set, Promise, Object, Array, String, Number, Error,
    performance: { now: () => now }, setTimeout: (f, ms) => { timers.push({ f, at: now + ms }); return timers.length; },
    App: {}, document: { activeElement: null }, $: () => null, escHtml: s => String(s),
  };
  vm.createContext(ctx);
  vm.runInContext(fs.readFileSync(path.join(UI, 'src', '73_wg_scripts.js'), 'utf8').replace(/^const (\w+)=/mg, 'var $1='), ctx);
  const S = ctx.WgScripts;
  S._el = { querySelector: q => q === '#wgScrState' ? box : null };
  let drawn = 0;
  S.drawStateNow = () => { drawn++; };
  const run = () => { for (const tm of timers.splice(0)) { if (tm.at <= now) tm.f(); else timers.push(tm); } };
  now = 1000;
  S.drawState();
  t.ok(drawn === 1, 'the first change is drawn at once');
  for (let i = 0; i < 30; i++){ now += 16; S.drawState(); run(); }
  t.ok(drawn === 2, 'a change every frame for half a second: drawn ' + drawn + ' times, not 30');
  hover = true;
  S.drawState();
  for (let i = 0; i < 100; i++){ now += 50; run(); }
  t.ok(drawn === 2, 'held while the pointer is over it (the wheel scrolls the list)');
  hover = false;
  for (let i = 0; i < 20; i++){ now += 50; run(); }
  t.ok(drawn === 3, 'and drawn once it leaves');
};
