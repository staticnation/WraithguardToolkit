/* The Dialogue window's Map (52_wg_dialogue_views.js): a busy ring is drawn as wide as its
   topics need, so labels do not sit on each other; the wheel zooms about the pointer, a
   drag moves the map (and is not a click on the topic under it), Fit puts it back, and
   zoomed in the labels are written in full. */
'use strict';
const { JSDOM } = require('jsdom');
const fs = require('fs');
const path = require('path');

module.exports = async (t) => {
  const dom = new JSDOM('<!doctype html><body></body>', { runScripts: 'outside-only', pretendToBeVisual: true });
  const w = dom.window, d = w.document;
  w.eval('var escHtml=s=>String(s==null?"":s).replace(/[&<>"\']/g,c=>"&#"+c.charCodeAt(0)+";"); var toast=()=>{};');
  w.eval(fs.readFileSync(path.join(__dirname, '..', '..', 'src', '52_wg_dialogue_views.js'), 'utf8').replace(/^const WgDial=/m, 'var WgDial='));
  const D = w.WgDial;

  // Forty topics around "Rumors", eight that lead to it.
  const names = Array.from({ length: 40 }, (_, i) => 'A rather long topic name ' + i);
  const map = {
    centre: 't:rumors', capped: false,
    nodes: [{ id: 't:rumors', label: 'Rumors', depth: 0, kind: 'topic' }].concat(names.map((n, i) => ({ id: 't:' + n, label: n, depth: 1, kind: 'topic' }))),
    edges: names.map((n, i) => i < 8 ? { from: 't:' + n, to: 't:rumors', kind: 'addtopic' } : { from: 't:rumors', to: 't:' + n, kind: 'link' }),
  };
  const asked = [];
  const E = { dial: d.createElement('div'), ask: async (k, b) => { asked.push([k, b]); return map; }, err: e => String(e) };
  E.dial.innerHTML = '<div class="orihead"><span style="flex:1"></span></div><div class="oribody"></div>';
  d.body.appendChild(E.dial);
  D.attach(E);
  await D.show('map', 'Rumors');
  const svg = d.querySelector('svg.edDMap');
  t.ok(svg && d.querySelector('.edDMapWrap'), 'the map, in its own resizable area');

  // No two topics' boxes overlap.
  const boxes = [...svg.querySelectorAll('g[data-nid]')].map(g => {
    const m = /translate\(([-\d.]+),([-\d.]+)\)/.exec(g.getAttribute('transform'));
    const rc = g.querySelector('rect');
    return { x: +m[1], y: +m[2], w: +rc.getAttribute('width'), h: 24 };
  });
  let overlaps = 0;
  for (let i = 0; i < boxes.length; i++) for (let j = i + 1; j < boxes.length; j++) {
    const a = boxes[i], b = boxes[j];
    if (Math.abs(a.x - b.x) < (a.w + b.w) / 2 && Math.abs(a.y - b.y) < (a.h + b.h) / 2) overlaps++;
  }
  t.ok(overlaps === 0, 'no labels on top of each other: ' + overlaps + ' overlaps');

  // The drawing at 1000 x 600 on screen.
  const vb = svg.viewBox.baseVal;
  svg.getBoundingClientRect = () => ({ left: 0, top: 0, width: 1000, height: 600, right: 1000, bottom: 600 });
  const g = svg.querySelector('.edDMapG');
  const scale = () => +/scale\(([\d.]+)\)/.exec(g.getAttribute('transform'))[1];
  const ev = (type, x, y, extra) => Object.assign(new w.MouseEvent(type, { clientX: x, clientY: y, bubbles: true, cancelable: true, button: 0 }), extra || {});
  const wheel = new w.WheelEvent('wheel', { clientX: 500, clientY: 300, deltaY: -400, bubbles: true, cancelable: true });
  svg.dispatchEvent(wheel);
  t.ok(scale() > 1.5 && wheel.defaultPrevented, 'the wheel zooms in: ' + scale());
  t.ok(svg.querySelector('text[data-full]').textContent === svg.querySelector('text[data-full]').dataset.full, 'zoomed in, the labels are whole');

  // A drag moves the map, and the click it ends with is not a click on a topic.
  const before = g.getAttribute('transform');
  const node = svg.querySelectorAll('g[data-nid]')[3];
  for (const [type, x] of [['pointerdown', 400], ['pointermove', 460], ['pointermove', 520]]) node.dispatchEvent(ev(type, x, 300, { pointerId: 1 }));
  node.dispatchEvent(ev('pointerup', 520, 300, { pointerId: 1 }));
  const n0 = asked.length;
  node.dispatchEvent(ev('click', 520, 300));
  t.ok(g.getAttribute('transform') !== before, 'the drag moved it');
  t.ok(asked.length === n0, 'and opened nothing');

  d.querySelector('#edDFit').onclick();
  t.ok(scale() === 1 && /translate\(0\.0,0\.0\)/.test(g.getAttribute('transform')), 'Fit shows the whole map again');
  t.ok(svg.querySelector('text[data-full]').textContent.length <= 22, 'with the short labels');
  t.ok(vb.width > 900, 'the drawing grew for the busy ring: ' + vb.width + ' x ' + vb.height);
};
