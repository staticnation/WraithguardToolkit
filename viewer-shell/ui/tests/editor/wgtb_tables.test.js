/* The Editor's other lists as bytes (WGTB): the page's unpackTables reads each of the
   fixture's hex strings (what pack_tags, pack_cells and pack_cell_refs in commands.rs
   make - their Rust test checks that side) back to its tables, and listTags, listCells and
   listCellRefs turn them into the shape the JSON commands answer with. A cut-short answer
   is an error; the engine refusing is said once, not asked again; an engine without the
   command falls back to the JSON one, and says so in Messages. */
'use strict';
const fs = require('fs');
const path = require('path');
const { page, EDITOR } = require('./harness');

module.exports = async (t) => {
  const fx = JSON.parse(fs.readFileSync(path.join(__dirname, 'wgtb_tables.json'), 'utf8'));
  const p = page({ modules: EDITOR, engine: (c) => c === 'editor_tags' ? [] : {} });
  const w = p.w;
  const bytesOf = (name) => Uint8Array.from(Buffer.from(fx[name + 'Hex'], 'hex')).buffer;
  const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

  for (const name of ['tags', 'cells', 'cellRefs']) {
    const got = w.unpackTables(bytesOf(name));
    t.ok(same(got, fx[name].tables), name + ': the tables read back: ' + JSON.stringify(got));
  }

  // The helpers, on the engine's bytes: the JSON command's shape, and no JSON command asked.
  const asked = [];
  w.Engine.call = async (c) => { asked.push(c); throw new Error('the JSON command was asked: ' + c); };
  w.Engine.bytes = async (c) => ({ editor_tags_bytes: bytesOf('tags'), cells_bytes: bytesOf('cells'),
    editor_cell_refs_bytes: bytesOf('cellRefs') })[c];
  t.ok(same(await w.listTags(), fx.tags.json), 'listTags: the JSON shape');
  t.ok(same(await w.listCells(), fx.cells.json), 'listCells: the JSON shape');
  t.ok(same(await w.listCellRefs('-2,-9'), fx.cellRefs.json), 'listCellRefs: the JSON shape');
  t.ok(asked.length === 0, 'the bytes were enough: ' + JSON.stringify(asked));

  // Malformed bytes are an error, never a short list.
  const whole = new Uint8Array(bytesOf('cells'));
  for (const [what, buf] of [['cut short', whole.slice(0, whole.length - 3)],
    ['with bytes after the last table', Uint8Array.from([...whole, 0])]]) {
    let threw = null;
    try { w.unpackTables(buf.buffer); } catch (e) { threw = e; }
    t.ok(threw && /ended early|after its last table/.test(threw.message), what + ' is an error: ' + (threw && threw.message));
  }

  // The engine refusing is its answer: said once, the JSON command not asked as well.
  w.Engine.bytes = async () => { throw new Error('no exterior cell 7,-3'); };
  let refused = null;
  try { await w.listCellRefs('7,-3'); } catch (e) { refused = e; }
  t.ok(refused && refused.message === 'no exterior cell 7,-3' && asked.length === 0, 'a refusal is said as it is, and not asked again: ' + (refused && refused.message));

  // An engine without the command (an older build): the JSON one, said in Messages.
  const said = [];
  w.WgLog = { add: (m, k) => said.push([k, m]) };
  w.Engine.bytes = async (c) => { throw new Error('command ' + c + ' not found'); };
  w.Engine.call = async (c) => { asked.push(c); return fx.tags.json; };
  t.ok(same(await w.listTags(), fx.tags.json), 'an older engine: the JSON list');
  t.ok(same(asked, ['editor_tags']) && said.length === 1 && said[0][0] === 'warn' && /second way/.test(said[0][1]),
    'and Messages says it came the second way: ' + JSON.stringify(said));

  // Both ways failing is an error naming both.
  w.Engine.call = async () => 'not a list';
  let both = null;
  try { await w.listTags(); } catch (e) { both = e; }
  t.ok(both && /not found; then the engine's answer was not the record types/.test(both.message), 'both ways failing says both: ' + (both && both.message));
};
