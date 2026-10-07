/* The Object Window: the plain filter, "All fields" search (editSearch), and the
   record drag into the render window (pointer events, no scrolling, no picture left). */
'use strict';
const { page, ev, EDITOR } = require('./harness');

module.exports = async (t) => {
  const p = page({
    modules: EDITOR,
    links: { editRecord: 'r', editPending: 'pend', editSearch: 'srch' },
    engine: (c) => c === 'editor_tags' ? [['WEAP', 2]]
      : c === 'editor_records' ? { plugins: ['M.esm'], rows: [['iron sword', 'Iron Sword', 'w\\i.nif', [0], 1], ['steel axe', 'Steel Axe', 'w\\a.nif', [0], 2]] }
      : {},
    answer: (url) => url === 'pend' ? [] : url === 'srch' ? { hits: { 'steel axe': 'data.weight: 30' }, count: 1, capped: false } : {},
  });
  const { w, d } = p;
  const E = w.WgEditor;
  await E.enter();
  await p.wait(30);
  const rows = () => [...d.querySelectorAll('#edTable tr[data-id]')].map(tr => tr.dataset.id);
  t.ok(rows().length === 2, 'both records listed: ' + rows());

  // The plain filter: id, name, model.
  const fi = d.getElementById('edFilter');
  fi.value = 'iron'; fi.oninput({ target: fi });
  t.ok(JSON.stringify(rows()) === '["iron sword"]', 'plain filter: ' + rows());

  // "All fields": Wraithguard searches; the rows are its hits, with what matched.
  const all = d.getElementById('edFilterAll');
  all.checked = true; all.onchange({ target: all });
  fi.value = 'data.weight>25'; fi.oninput({ target: fi });
  await p.wait(450);
  t.ok(JSON.stringify(rows()) === '["steel axe"]', 'search rows: ' + rows());
  t.ok(/Matched/.test(d.querySelector('#edTable thead').textContent), 'a Matched column');
  const sp = p.posts.find(x => x[0] === 'srch');
  t.ok(sp && sp[1].tag === 'WEAP' && sp[1].query === 'data.weight>25', 'search asked: ' + JSON.stringify(sp));

  // Dragging a record: pointer events; the list must not scroll; the picture goes.
  const U = w.WgUI;
  U.hideHover = () => {}; U._thumbs = new Map(); U.modelPath = () => '';
  const placed = [];
  E.placeAt = (rec, x, y) => placed.push([rec.id, x, y]);
  E.placePoint = (x, y) => ({ world: [x, y, 0] });
  const cv = d.createElement('canvas'); d.getElementById('vpwrap').appendChild(cv);
  w.App.R.cv = cv;
  const row = d.createElement('div'); row.id = 'dragsrc'; d.body.appendChild(row);
  const side = d.createElement('div'); side.appendChild(row.parentNode.removeChild(row)); d.body.appendChild(side);
  Object.defineProperty(side, 'scrollWidth', { value: 900 }); Object.defineProperty(side, 'clientWidth', { value: 300 });
  cv.getBoundingClientRect = () => ({ left: 0, top: 0, right: 800, bottom: 600, width: 800, height: 600 });
  d.getElementById('vpwrap').getBoundingClientRect = cv.getBoundingClientRect;
  d.elementFromPoint = (x) => x < 800 ? cv : row;
  U.armDrag(row, () => ({ tag: 'WEAP', id: 'iron sword', model: 'w\\i.nif' }));
  side.scrollLeft = 0;
  const down = ev(w, 'pointerdown', 900, 50);
  row.dispatchEvent(down);
  t.ok(down.defaultPrevented, 'the press starts no text selection');
  w.dispatchEvent(ev(w, 'pointermove', 903, 50));
  t.ok(!d.body.classList.contains('edDragging'), 'no drag under 6 px');
  side.scrollLeft = 120;
  w.dispatchEvent(ev(w, 'pointermove', 700, 300));
  t.ok(d.body.classList.contains('edDragging'), 'the drag starts');
  t.ok(side.scrollLeft === 0, 'the list does not slide: ' + side.scrollLeft);
  w.dispatchEvent(ev(w, 'pointerup', 400, 300));
  t.ok(JSON.stringify(placed) === '[["iron sword",400,300]]', 'placed where let go: ' + JSON.stringify(placed));
  t.ok(!d.getElementById('edDragImg'), 'the picture goes after the drop');
  // Let go elsewhere, or Esc: nothing placed.
  row.dispatchEvent(ev(w, 'pointerdown', 900, 50)); w.dispatchEvent(ev(w, 'pointermove', 950, 80)); w.dispatchEvent(ev(w, 'pointerup', 950, 80));
  row.dispatchEvent(ev(w, 'pointerdown', 900, 50)); w.dispatchEvent(ev(w, 'pointermove', 500, 80));
  w.dispatchEvent(new w.KeyboardEvent('keydown', { key: 'Escape' })); w.dispatchEvent(ev(w, 'pointerup', 500, 80));
  t.ok(placed.length === 1, 'nothing placed off the canvas or after Esc');
  E.leave();
};
