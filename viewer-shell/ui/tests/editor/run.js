/* Runs every `*.test.js` beside it (each exports `async (t) => {...}` and calls
   `t.fail(message)` for what is wrong). Exit status 1 when any fails.

     node editor/run.js            all of them
     node editor/run.js search     the ones whose file name has "search" in it */
'use strict';
const fs = require('fs');
const path = require('path');

// WG_TAINT runs answer with text the flows do not expect: a page error then is the flow
// breaking, not the check failing, so it is noted and the run goes on.
if (process.env.WG_TAINT) process.on('unhandledRejection', e => console.log('   (page error under taint: ' + String(e && e.message || e).slice(0, 100) + ')'));
(async () => {
  const want = process.argv[2] || '';
  const files = fs.readdirSync(__dirname).filter(f => f.endsWith('.test.js') && f.includes(want)).sort();
  let failed = 0;
  for (const f of files) {
    const fails = [];
    const t = { fail: m => fails.push(m), ok: (c, m) => { if (!c) fails.push(m); } };
    const started = Date.now();
    try {
      await require(path.join(__dirname, f))(t);
    } catch (e) {
      fails.push('threw: ' + (e && e.stack || e));
    }
    // What the page sent or was answered that the contract does not allow (harness.js).
    const broke = require('./harness').violations;
    for (const v of [...new Set(broke.splice(0))]) fails.push('contract: ' + v);
    // WG_TAINT=1 (harness.js): answer text that reached the page as markup.
    for (const v of [...new Set(require('./harness').tainted.splice(0))]) fails.push('unescaped: ' + v);
    const ms = Date.now() - started;
    if (fails.length) {
      failed++;
      console.log('FAIL ' + f + ' (' + ms + ' ms)');
      for (const m of fails) console.log('   - ' + m);
    } else {
      console.log('ok   ' + f + ' (' + ms + ' ms)');
    }
  }
  console.log(files.length + ' test file(s), ' + failed + ' failed');
  process.exit(failed ? 1 : 0);
})();
