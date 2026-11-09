/* The markup's wording table (04_tx_markup.js) is what the markup says, and the page's text
   module answers from it rather than from the page (CodeQL js/xss-through-dom). */
'use strict';
const fs = require('fs');
const path = require('path');
const gen = require('../gen_tx_markup');

module.exports = async (t) => {
  const want = gen.source(gen.table());
  const have = fs.readFileSync(gen.OUT, 'utf8').replace(/\r\n/g, '\n');
  t.ok(have === want, 'ui/src/04_tx_markup.js is out of date: run node ui/tests/gen_tx_markup.js');
  const order = fs.readFileSync(path.join(__dirname, '..', '..', 'ORDER'), 'utf8').split(/\r?\n/).map(l => l.trim());
  t.ok(order.indexOf('04_tx_markup.js') >= 0 && order.indexOf('04_tx_markup.js') < order.indexOf('05_text.js'), 'ORDER loads the table before 05_text.js');
  const n = Object.keys(gen.table()).length;
  t.ok(n > 100, 'the table has the markup\'s keys: ' + n);
  const text = fs.readFileSync(path.join(__dirname, '..', '..', 'src', '05_text.js'), 'utf8');
  t.ok(!/querySelector\('\[data-tx|innerHTML\.replace\(/.test(text), '05_text.js reads no wording back out of the page');
};
