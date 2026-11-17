/* The Land view's texture mask (67_wg_land.js): the height and color brushes held to the
   ground painted with the textures named ("Only on these textures"), or kept off it
   ("Everywhere but these textures"). A texture cell is four vertices wide: a vertex inside one
   has its texture, and one on the line between two takes half the brush. */
'use strict';
const { page, EDITOR } = require('./harness');

module.exports = async (t) => {
  const p = page({
    modules: EDITOR,
    links: { editRecord: 'r', editPending: 'pend' },
    engine: (c) => c === 'editor_tags' ? [] : c === 'editor_records' ? { plugins: [], rows: [] }
      : c === 'cells' ? { grid: [], count: 0, interiors: [], interiorCount: 0 } : {},
    answer: (url) => url === 'pend' ? [] : {},
  });
  const { w, d } = p;
  w.eval(`var BrushSize={pos:()=>50, radius:()=>1200}; var DEFAULT_LTEX='_land_default'; var DEFAULT_LAND_HEIGHT=-2048; var GameData={};`);
  w.eval(require('./harness').source('src/72_wg_landbrush.js'));
  w.eval(require('./harness').source('src/67_wg_land.js'));
  const L = w.WgLand;
  w.WgEditor.ask = async () => ({});
  w.WgEditor.refreshPending = async () => {};
  w.WgEditor.canEdit = () => true;
  L.rebuild = () => {};
  L.renormal = () => {};

  // One cell: road on its west half (texture columns 0-7), grass on the east. The line between
  // them is vertex column 32.
  const N = 65, TS = 16;
  const ids = []; for (let j = 0; j < TS; j++) for (let i = 0; i < TS; i++) ids.push(i < 8 ? 'road' : 'grass');
  const cell = () => ({ kind: 'ext', gx: 0, gy: 0, heights: new w.Float32Array(N * N), normals: new w.Float32Array(N * N * 3), colours: null, layers: [], _landIds: ids.slice() });
  const fresh = () => { w.App._scene = { origin: [0, 0], cells: [cell()], chunks: [] }; L._edited.clear(); L.dirty.clear(); L._before = null; L._stroke = null; };
  const h = (vx, vy) => w.App._scene.cells[0].heights[vy * N + vx];
  L.preview = true; L.mode = 'height'; L.tool = 'raise'; L.strength = 40; L.feather = 0;
  L.b.falloff = 'flat'; L.b.mask = 'off'; L.b.maskTex = [];
  fresh();
  L.panel();
  const stroke = () => { L._stroke = null; L.paint([4096, 4096, 0]); };   // on the line, mid cell

  stroke();
  t.ok(h(30, 32) > 0 && h(34, 32) > 0 && h(30, 32) === h(34, 32), 'no mask: both sides rise alike (' + h(30, 32) + ', ' + h(34, 32) + ')');
  const full = h(30, 32);

  fresh(); L.b.mask = 'only'; L.b.maskTex = ['road'];
  stroke();
  t.ok(h(30, 32) === full, 'only on road: the road rises whole (' + h(30, 32) + ')');
  t.ok(h(34, 32) === 0, 'only on road: the grass does not move');
  t.ok(Math.abs(h(32, 32) - full / 2) < 1e-3, 'the vertex on the line takes half (' + h(32, 32) + ' of ' + full + ')');
  t.ok(h(32, 30) > 0 && h(33, 30) === 0, 'along the line: on it some, past it none');

  // As drawn: the road fades into the grass between the two texture cells' centres (vertex
  // columns 30 and 34, the road's last column of cells and the grass's first), not at the line.
  fresh(); L.b.maskEdge = 'drawn';
  stroke();
  t.ok(h(30, 32) === full && h(34, 32) === 0, 'as drawn: whole at the road cell\'s centre, none at the grass cell\'s (' + h(30, 32) + ', ' + h(34, 32) + ')');
  t.ok(Math.abs(h(31, 32) - full * 0.75) < 1e-3 && Math.abs(h(32, 32) - full / 2) < 1e-3 && Math.abs(h(33, 32) - full / 4) < 1e-3,
    'and in quarters between (' + [h(31, 32), h(32, 32), h(33, 32)].join(', ') + ')');
  L.b.maskEdge = 'cell';

  // As drawn where three textures meet: the renderer lays each texture over the ones below it,
  // the most painted the opaque base (engine order). Grass is the base; road has three texture
  // cells and rock two, so rock is drawn over road. Halfway between the centers of road (4,4)
  // and rock (5,4): rock shows half, road half of what rock leaves, grass the rest.
  {
    const three = []; for (let j = 0; j < TS; j++) for (let i = 0; i < TS; i++) three.push('grass');
    for (const [i, j] of [[4, 4], [4, 3], [3, 3]]) three[j * TS + i] = 'road';
    for (const [i, j] of [[5, 4], [6, 3]]) three[j * TS + i] = 'rock';
    const c3 = Object.assign(cell(), { _landIds: three });
    const cells = new w.Map([['0,0', c3]]);
    const at = (ids) => L.drawnShare(cells, 20, 18, ids);
    t.ok(Math.abs(at(['rock']) - 0.5) < 1e-6 && Math.abs(at(['road']) - 0.25) < 1e-6 && Math.abs(at(['grass']) - 0.25) < 1e-6,
      'three meet: rock 0.5, road 0.25 under it, grass 0.25 (' + [at(['rock']), at(['road']), at(['grass'])].join(', ') + ')');
    t.ok(Math.abs(at(['road', 'rock']) - 0.75) < 1e-6, 'two named: what they show together (' + at(['road', 'rock']) + ')');
    // A texture painted there: the layers kept on the cell are made again.
    L.texture = 'rock';
    L.dabTexture(cells, 4.5 * 512, 4.5 * 512, new w.Set());
    t.ok(at(['rock']) === 1, 'painted over with rock, the vertex is all rock (' + at(['rock']) + ')');
  }

  fresh(); L.b.mask = 'not';
  stroke();
  t.ok(h(30, 32) === 0 && h(34, 32) === full, 'everywhere but road: the grass rises, the road stays');

  fresh(); L.b.mask = 'only'; L.b.maskTex = [];
  stroke();
  t.ok(h(30, 32) === full && h(34, 32) === full, 'a mask with no texture named is no mask');

  // Flatten to a height: only the masked ground is levelled.
  fresh(); L.b.mask = 'only'; L.b.maskTex = ['grass']; L.tool = 'flatten'; L.strength = 100; L.b.flatTarget = 'fixed'; L.b.flatHeight = 500;
  stroke();
  t.ok(h(34, 32) === 500 && h(30, 32) === 0, 'flatten only on grass: grass at 500, road untouched (' + h(34, 32) + ', ' + h(30, 32) + ')');
  L.tool = 'raise'; L.strength = 40;

  // The color brush goes by the mask too.
  fresh(); L.mode = 'color'; L.color = '#000000'; L.strength = 100; L.b.mask = 'only'; L.b.maskTex = ['road'];
  stroke();
  const col = (vx, vy) => { const c = w.App._scene.cells[0].colours; return c ? c[(vy * N + vx) * 3] : 255; };
  t.ok(col(30, 32) === 0 && col(34, 32) === 255, 'color only on road: road tinted, grass white (' + col(30, 32) + ', ' + col(34, 32) + ')');
  L.mode = 'height';

  // Pick adds the texture under the click to the mask, and the page shows it.
  fresh(); L.b.mask = 'only'; L.b.maskTex = [];
  d.getElementById('wgLandMaskPick').click();
  t.ok(L._picking === 'mask', 'Pick waits for a click on the ground');
  L.paint([6000, 4096, 0]);
  t.ok(L.b.maskTex.join() === 'grass' && L._picking === false, 'the texture under the click is in the mask: ' + L.b.maskTex.join());
  t.ok(h(40, 32) === 0, 'the picking click sculpts nothing');
  t.ok(d.getElementById('wgLandMaskList').textContent.includes('grass'), 'and it is listed');
  d.querySelector('#wgLandMaskList [data-mid]').click();
  t.ok(L.b.maskTex.length === 0, 'a click on a listed texture takes it out');
  L.b.mask = 'off';

  // The painted mask: the Mask mode paints it (nothing of it is land, or waits to be written),
  // Alt rubs it out, and the height brush can be held inside it or kept out of it.
  fresh(); L.mode = 'mask'; L.strength = 100; L.feather = 0;
  const m = () => L.pmaskCells.get('0,0');
  L._stroke = null; L.paint([2048, 4096, 0]);   // radius 1200 round vertex (16, 32)
  t.ok(m() && m()[32 * N + 16] === 255 && m()[32 * N + 40] === 0, 'the Mask mode paints the mask under the brush');
  t.ok(h(16, 32) === 0 && !L.waiting() && L.dirty.size === 0, 'painting the mask moves no ground and leaves nothing to write');
  L.mode = 'height'; L.tool = 'raise'; L.strength = 40; L.b.pmask = 'only';
  L._stroke = null; L.paint([2048 + 1024, 4096, 0]);   // round vertex (24, 32): half in the mask
  t.ok(h(20, 32) === full && h(30, 32) === 0, 'only inside the mask: inside rises, outside stays (' + h(20, 32) + ', ' + h(30, 32) + ')');
  fresh(); L.b.pmask = 'not';
  L._stroke = null; L.paint([2048 + 1024, 4096, 0]);
  t.ok(h(20, 32) === 0 && h(30, 32) === full, 'everywhere but the mask: the other way round');
  // With the texture mask as well, both must let the brush through.
  fresh(); L.b.pmask = 'only'; L.b.mask = 'only'; L.b.maskTex = ['grass'];
  L._stroke = null; L.paint([2048 + 1024, 4096, 0]);
  t.ok(h(20, 32) === 0, 'inside the painted mask but on road, with only grass allowed: nothing moves');
  L.b.mask = 'off';
  // The overlay is on the mesh alone.
  const mesh = { col: new w.Uint8Array(N * N * 3).fill(255) };
  const offMesh = { col: new w.Uint8Array(N * N * 3).fill(255) };
  L.tintMask('0,0', offMesh);
  t.ok(offMesh.col.every(v => v === 255), 'outside the Land view the mask is not drawn');
  L.on = true;
  L.tintMask('0,0', mesh);
  L.on = false;
  t.ok(mesh.col[(32 * N + 16) * 3 + 1] < 255 && mesh.col[(32 * N + 40) * 3 + 1] === 255, 'the mask is drawn as a tint where it is painted');
  t.ok(!w.App._scene.cells[0].colours, 'and the cell\'s own colors are not touched');
  // Kept between sessions: saved a moment after a change, and read back.
  await p.wait(700);
  const keptMask = w.localStorage.getItem('wgLandPMask');
  t.ok(keptMask && JSON.parse(keptMask)['0,0'], 'the mask is kept in the page\'s storage');
  const was = m().slice();
  L.pmaskCells.clear(); L.loadMask();
  t.ok(m() && m().every((v, i) => v === was[i]), 'and read back as it was');
  // Alt rubs it out; Invert turns it round; Clear empties it.
  L.mode = 'mask'; L.strength = 100; L._mods.alt = true;
  L._stroke = null; L.paint([2048, 4096, 0]);
  L._mods.alt = false;
  t.ok(m()[32 * N + 16] === 0, 'Alt rubs the mask out');
  d.getElementById('wgLandPMaskInv').click();
  t.ok(m()[32 * N + 16] === 255 && m()[32 * N + 40] === 255, 'Invert: what was clear is masked');
  d.getElementById('wgLandPMaskClear').click();
  t.ok(L.pmaskCells.size === 0, 'Clear empties it');
  await p.wait(700);
  t.ok(!w.localStorage.getItem('wgLandPMask'), 'and the kept copy with it');
  L.mode = 'height'; L.strength = 40; L.b.pmask = 'only';
  fresh(); L._stroke = null; L.paint([4096, 4096, 0]);
  t.ok(h(30, 32) === full, 'with nothing painted, "only inside the mask" is no mask');
  L.b.pmask = 'off';

  // Static, 2026-11-16: a masked flatten left a cliff steeper than a record can store round the
  // corner four cells share, and the write clamped it into tears and spikes. What is written now
  // is on the eight-unit grid, every stored step within -1,024 and +1,016, and every border the
  // cells share agrees.
  {
    const asked = [];
    w.WgEditor.ask = async (name, body) => { asked.push([name, body]); return {}; };
    const mk = (gx, gy) => ({ kind: 'ext', gx, gy, heights: new w.Float32Array(N * N).fill(704), normals: new w.Float32Array(N * N * 3), colours: null, layers: [], _landIds: ids.slice() });
    const quad = [mk(-6, -2), mk(-5, -2), mk(-6, -1), mk(-5, -1)];
    w.App._scene = { origin: [0, 0], cells: quad, chunks: [] };
    L._edited.clear(); L.dirty.clear(); L._before = null; L.preview = false;
    // A plateau of 2,203.4 units (off the grid, as a brush leaves it) in the corner, sheer to 704.
    const world = (c, lx, ly) => [c.gx * 64 + lx, c.gy * 64 + ly];
    for (const c of quad) {
      for (let ly = 0; ly < N; ly++) for (let lx = 0; lx < N; lx++) {
        const [vx, vy] = world(c, lx, ly);
        if (Math.abs(vx - (-5 * 64)) <= 3 && Math.abs(vy - (-1 * 64)) <= 3) c.heights[ly * N + lx] = 2203.4;
      }
      L._edited.set(c.gx + ',' + c.gy, c); L.mark(c.gx + ',' + c.gy, 'h');
    }
    await L.commit();
    const wrote = new Map(asked.filter(a => a[0] === 'editLandSet').map(a => [a[1].cell, a[1].heights]));
    t.ok(wrote.size === 4, 'the four cells are written: ' + [...wrote.keys()].join(' '));
    let off = 0, steep = 0;
    for (const hs of wrote.values()) {
      for (let y = 0; y < N; y++) for (let x = 0; x < N; x++) {
        const v = hs[y * N + x];
        if (v % 8 !== 0) off++;
        if (x > 0) { const d = v - hs[y * N + x - 1]; if (d > 1016 || d < -1024) steep++; }
        if (x === 0 && y > 0) { const d = v - hs[(y - 1) * N]; if (d > 1016 || d < -1024) steep++; }
      }
    }
    t.ok(off === 0, 'every height on the eight-unit grid (' + off + ' off it)');
    t.ok(steep === 0, 'every stored step within what a record holds (' + steep + ' beyond)');
    const H = k => wrote.get(k);
    const sw = H('(-6, -2)'), se = H('(-5, -2)'), nw = H('(-6, -1)'), ne = H('(-5, -1)');
    let open = 0;
    for (let i = 0; i < N; i++) {
      if (sw[i * N + 64] !== se[i * N]) open++;
      if (nw[i * N + 64] !== ne[i * N]) open++;
      if (sw[64 * N + i] !== nw[i]) open++;
      if (se[64 * N + i] !== ne[i]) open++;
    }
    t.ok(open === 0, 'every shared border agrees (' + open + ' vertices apart)');
    t.ok(ne[0] === se[64 * N] && ne[0] === nw[64] && ne[0] === sw[64 * N + 64], 'the corner is one height in all four: ' + [sw[64 * N + 64], se[64 * N], nw[64], ne[0]]);
    t.ok(w.App._scene.cells[3].heights[0] === ne[0], 'and the page draws what is written');
  }
};
