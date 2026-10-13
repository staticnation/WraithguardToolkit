/* The Object Window's rows as bytes: the page's unpackRows reads the fixture's `hex`
   (what pack_rows in commands.rs makes - its Rust test checks that side) back to the
   fixture's plugins and rows; and a cut-short answer is an error, not an empty list. */
'use strict';
const fs = require('fs');
const path = require('path');
const { page, EDITOR } = require('./harness');

module.exports = async (t) => {
  const fx = JSON.parse(fs.readFileSync(path.join(__dirname, 'wgor_rows.json'), 'utf8'));
  const p = page({ modules: EDITOR, engine: (c) => c === 'editor_tags' ? [] : {} });
  const E = p.w.WgEditor;
  const bytes = Uint8Array.from(Buffer.from(fx.hex, 'hex'));
  const got = E.unpackRows(bytes.buffer);
  t.ok(JSON.stringify(got.plugins) === JSON.stringify(fx.plugins), 'the plugins read back: ' + JSON.stringify(got.plugins));
  t.ok(JSON.stringify(got.rows) === JSON.stringify(fx.rows), 'the rows read back: ' + JSON.stringify(got.rows));
  let threw = null;
  try { E.unpackRows(bytes.slice(0, bytes.length - 3).buffer); } catch (e) { threw = e; }
  t.ok(threw && /ended early/.test(threw.message), 'a cut-short answer is an error: ' + (threw && threw.message));
};
