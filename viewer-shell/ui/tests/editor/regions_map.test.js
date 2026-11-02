/* The region map (70_wg_regions.js): a region painted over cells goes to the patch as one
   `editCellRegions`; a box over the sea makes each empty square's land and cell
   (`editNewLand`) and gives them the region; the map draws what the patch now has. */
'use strict';
const { page, EDITOR } = require('./harness');

module.exports = async (t) => {
  const asked = [];
  const p = page({
    modules: EDITOR.concat(['src/70_wg_regions.js']),
    links: { editRecord: 'r', editPending: 'pend', editCellRegions: 'cr', editNewLand: 'nl' },
    engine: (c) => c === 'editor_tags' ? [['CELL', 1]] : c === 'editor_records' ? { plugins: [], rows: [] }
      : c === 'cells' ? { grid: [], count: 0, interiors: [], interiorCount: 0 }
      : c === 'regions_list' ? [{ id: 'Bitter Coast Region', name: 'Bitter Coast', color: [30, 80, 40] }, { id: 'Dark Region', name: 'Dark', color: [0, 0, 0] }] : {},
    answer: (url, body) => {
      const B = typeof body === 'string' ? JSON.parse(body || '{}') : (body || {}); asked.push([url, B]);
      if (url === 'pend') return [];
      if (url === 'cr') return { set: B.cells.length, made: 0, skipped: 0 };
      if (url === 'nl') return { cell: B.cell, land: true, madeCell: true };
      return {};
    },
  });
  const { w } = p;
  const R = w.WgRegions;
  t.ok(R, 'WgRegions is there');
  await R.load();
  t.ok(R.list.length === 2, 'the regions read: ' + R.list.length);
  // A black map color gets one of its own (it would vanish into the sea).
  t.ok(R.color('Dark Region').join() !== '0,0,0', 'a black region gets a color of its own');
  t.ok(R.color('bitter coast region').join() === '30,80,40', 'a region keeps its own color');

  // A stand-in for the cell map: ten pixels a cell, two cells known.
  const fills = [];
  const map = {
    W: 500, H: 500, view: { z: 10 },
    cellsByKey: new Map([['1,1', { kind: 'ext', x: 1, y: 1, region: 'Dark Region' }], ['2,1', { kind: 'ext', x: 2, y: 1, region: '' }]]),
    known(x, y) { return this.cellsByKey.has(x + ',' + y); },
    rect(x, y) { return [x * 10, y * 10, 10, 10]; },
    local(e) { return [e.px, e.py]; },
    cellAt(px, py) { return { x: Math.floor(px / 10), y: Math.floor(py / 10) }; },
    canvas: { setPointerCapture() {}, releasePointerCapture() {} },
    redraw() {},
  };
  const ev = (px, py) => ({ button: 0, px, py, pointerId: 1, preventDefault() {} });
  w.App.cellPick = 'map';
  R.on = true; R.tool = 'paint'; R.region = 'Bitter Coast Region'; R.makeCells = false;
  t.ok(R.down(map, ev(15, 15)) && R.move(map, ev(25, 15)), 'a paint stroke takes the pointer');
  R.up(map, ev(25, 15));
  await new Promise(r => setTimeout(r, 30));
  const cr = asked.find(a => a[0] === 'cr');
  t.ok(cr && cr[1].cells.join('|') === '(1, 1)|(2, 1)' && cr[1].region === 'Bitter Coast Region', 'one editCellRegions for the cells painted: ' + JSON.stringify(cr && cr[1]));
  t.ok(R.regionAt(map, '1,1') === 'Bitter Coast Region', 'the map has the patch\'s region');

  // A box over the sea: two empty squares, made and given the region.
  R.tool = 'box';
  R.down(map, ev(55, 55)); R.move(map, ev(65, 55)); R.up(map, ev(65, 55));
  await new Promise(r => setTimeout(r, 30));
  const nl = asked.filter(a => a[0] === 'nl').map(a => a[1].cell);
  t.ok(nl.join('|') === '(5, 5)|(6, 5)', 'each empty square made: ' + nl.join('|'));
  t.ok(map.known(5, 5) && map.cellsByKey.get('6,5').region === 'Bitter Coast Region', 'the new cells are on the map, in the region');

  // Drawn: the region colors, dashed where the patch changed or made the cell.
  const ctx = { fillRect: (...a) => fills.push(a), set fillStyle(v) { this._f = v; }, get fillStyle() { return this._f; },
    save() {}, restore() {}, setLineDash() {}, beginPath() {}, rect() {}, stroke() {}, strokeRect() {} };
  R.drawOverlay(ctx, map);
  t.ok(fills.length >= 4, 'every cell with a region is filled: ' + fills.length);
};
