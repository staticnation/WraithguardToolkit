// escHtml (03_core.js) escapes all five characters that matter in HTML text and in a quoted
// attribute, the single quote included (security audit 2026-11-20, F6): a value in a single-quoted
// attribute must not be able to end it.
const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const path = require('node:path');

const src = fs.readFileSync(path.join(__dirname, '..', 'src', '03_core.js'), 'utf8');
const m = src.match(/function escHtml\(s\)\{[\s\S]*?\n?[^\n]*\); \}/);
assert.ok(m, 'escHtml is in 03_core.js');
const escHtml = new Function(`${m[0]}; return escHtml;`)();

test('all five characters are escaped, and null is empty', () => {
  assert.strictEqual(escHtml(`<a href="x" title='y'>&</a>`), '&lt;a href=&quot;x&quot; title=&#39;y&#39;&gt;&amp;&lt;/a&gt;');
  assert.strictEqual(escHtml(null), '');
  assert.strictEqual(escHtml(undefined), '');
  assert.strictEqual(escHtml(12), '12');
});

test('a value cannot end a single-quoted attribute', () => {
  const html = `<span title='${escHtml("x' onmouseover='alert(1)")}'>t</span>`;
  assert.ok(!/title='[^']*'[^>]*onmouseover/.test(html), html);
});
