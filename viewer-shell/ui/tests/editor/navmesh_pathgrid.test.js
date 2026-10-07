/* The Navmesh overlay (its answer read, tiles out of date found) and the Path grid
   mode (editing, and points from the navmesh one per polygon), on a small navmesh made
   here in the engine's own answer format (viewcore::navmesh::encode, version 3). */
'use strict';
const { page, ev } = require('./harness');

/** A `navmesh` answer: `tiles` [{x, y, polys:[{v:[[x,y,z]...], flags, nei:[...]}], objs:[[shape, [x,y,z]]],
 *  heights:[{cx,cy,w,rows,mx,my,orig,v:[...]}], water:[], flat:[]}]. */
function answer(tileW, shapes, tiles) {
  const out = [];
  const u8 = v => out.push(v & 255);
  const u16 = v => { u8(v); u8(v >> 8); };
  const u32 = v => { u16(v & 0xffff); u16((v >>> 16) & 0xffff); };
  const f32 = v => { const b = Buffer.alloc(4); b.writeFloatLE(v); out.push(...b); };
  const str16 = s => { const b = Buffer.from(s); u16(b.length); out.push(...b); };
  out.push(...Buffer.from('GDNM')); u8(3); str16('navmesh.db');
  u32(tiles.length); u32(0); u8(0);
  u8(1); u8(2); f32(29); f32(28); f32(66);        // one agent
  const polys = []; tiles.forEach((t, ti) => t.polys.forEach((p, pi) => polys.push([ti, pi, p])));
  u32(polys.length);
  for (const [ti, pi, p] of polys) {
    u8(0); u8(0); u16(p.flags); u8(p.v.length);
    for (const v of p.v) for (const c of v) f32(c);
    u32(ti); u16(pi); for (const n of p.nei) u16(n);
  }
  f32(tileW);
  u32(shapes.length); shapes.forEach(str16);
  u32(tiles.length);
  for (const t of tiles) {
    u32(t.x); u32(t.y); u8(0); u32(t.objs.length);
    for (const [s, pos] of t.objs) { u32(s); pos.forEach(f32); }
    u32(0);                                          // water
    u32(t.heights.length);
    for (const h of t.heights) { u32(h.cx); u32(h.cy); u32(8192); u16(h.w); u16(h.rows); u16(h.mx); u16(h.my); u16(h.orig); h.v.forEach(f32); }
    u32(0);                                          // flat
  }
  const b = Buffer.from(out);
  return b.buffer.slice(b.byteOffset, b.byteOffset + b.length);
}

const sq = (x0, x1) => [[x0, 0, 0], [x1, 0, 0], [x1, 1000, 0], [x0, 1000, 0]];
const NM = answer(1000, ['meshes/x/rock.nif'], [
  { x: 0, y: 0, objs: [[0, [100, 100, 0]], [0, [200, 200, 0]], [0, [300, 300, 0]]], heights: [{ cx: 0, cy: 0, w: 2, rows: 2, mx: 0, my: 0, orig: 65, v: [0, 0, 0, 0] }],
    polys: [{ v: sq(0, 500), flags: 1, nei: [0xffff, 1, 0xffff, 0xffff] },          // A, joined to B inside the tile
            { v: sq(500, 1000), flags: 1, nei: [0xffff, 0x8000, 0xffff, 0] }] },    // B, a portal on x=1000
  { x: 1, y: 0, objs: [], heights: [],
    polys: [{ v: sq(1000, 2000), flags: 1, nei: [0xffff, 0xffff, 0xffff, 0x8004] }] }, // C, the portal back
]);

module.exports = async (t) => {
  const p = page({
    modules: ['src/47_wg_overlay.js', 'src/48_wg_tools.js', 'src/54_wg_pathgrid_edit.js'],
    html: '<div id="wgOverlayPane"><div id="wgNavRow"><select id="wgNavAgent"></select><div id="wgNavNote"></div></div></div><div id="vpwrap"></div>',
    engine: (c) => c === 'navmesh' ? NM : [],
    answer: (url, b) => url === 'get' ? { grid: [0, 0], points: [[100, 100, 500], [300, 100, 500]], edges: [[0, 1], [1, 0]] }
      : { grid: [0, 0], points: b.points, edges: b.edges, queued: true },
  });
  const { w, d } = p;
  w.App._scene = { cells: [{ kind: 'ext', gx: 0, gy: 0, heights: new Float32Array(65 * 65) }], origin: [0, 0] };
  w.App.R.cam = { tx: 500, ty: 500, tz: 0 };
  const T = w.WgTools;
  T.sc = () => w.App._scene;
  w.WgEditor = {
    gridOn: false, held: new Set(), canEdit: () => true, refreshPending() {}, links: () => ({}),
    projector: () => (pt => [pt[0], pt[1]]),
    onPlane: (ray, z) => [ray.eye[0], ray.eye[1], z], snapPos: v => v,
    placePoint: (x, y) => ({ local: [x, y, 480], world: [x, y, 480] }),
    currentCell: () => ({ key: '(0, 0)', label: '(0, 0)' }),
    async ask(link, body) { return w.__answer(link === 'editPathgrid' ? 'get' : 'set', body); },
  };
  w.App.R.rayAt = (x, y) => ({ eye: [x, y, 1000], dir: [0, 0, -1] });

  // The overlay reads the answer: polygons, the agent, the tiles' build input.
  T.ovl.navmesh = true; await T.overlay('navmesh');
  const nm = T.nav.data;
  t.ok(nm.polys.length === 3 && nm.built.length === 2 && nm.tileW === 1000, 'answer read');
  t.ok(nm.polys[1].nei[1] === 0x8000, 'neighbour record read');
  t.ok(w.App.R.ov.navmesh && w.App.R.ov.navmesh.p.length > 0, 'drawn');

  // Out of date: nothing when the scene matches; an object moved; a height changed.
  // Three rocks: when one moves, two of three still match, so the tile can be compared.
  w.App.R.pickables = [100, 200, 300].map(c => ({ model: 'x\\rock.nif', m: [1, 0, 0, c, 0, 1, 0, c, 0, 0, 1, 0], wpos: [c, c, 0] }));
  t.ok(T.staleTiles(nm, 0).length === 0, 'up to date');
  w.App.R.pickables[0].wpos = [150, 100, 0];
  let st = T.staleTiles(nm, 0);
  t.ok(st.length === 1 && /moved or gone/.test(st[0].why), 'a moved object: ' + JSON.stringify(st));
  w.App.R.pickables[0].wpos = [100, 100, 0];
  w.App._scene.cells[0].heights[1] = 40;
  st = T.staleTiles(nm, 0);
  t.ok(st.length === 1 && /land changed/.test(st[0].why), 'a land edit: ' + JSON.stringify(st));

  // Points from the navmesh, one per polygon; inside the tile from the record, across
  // the border where the portals overlap.
  const g = w.WgPath.navGraph(nm, 0, null);
  t.ok(g.pts.length === 3, 'one point per polygon: ' + g.pts.length);
  t.ok(g.edges.length === 4, 'A-B inside, B-C across: ' + JSON.stringify(g.edges));

  // Path grid mode: select, add (linked), link, drag, delete, undo.
  const P = w.WgPath;
  await P.start();
  let last = null;
  const ask = w.WgEditor.ask;
  w.WgEditor.ask = async (link, body) => { if (link === 'editPathgridSet') last = body; return ask(link, body); };
  await P.click({ clientX: 101, clientY: 99 });
  t.ok(P.sel.length === 1 && P.sel[0].i === 0, 'a point selected');
  await P.click({ clientX: 2000, clientY: 3000, shiftKey: true });
  t.ok(last && last.points.length === 3 && last.edges.some(e => e[0] === 0 && e[1] === 2), 'Shift+click adds, linked');
  await P.click({ clientX: 300, clientY: 100, ctrlKey: true });
  t.ok(last.edges.some(e => e[0] === 2 && e[1] === 1), 'Ctrl+click links');
  const drag = P.grab({ clientX: 300, clientY: 100 });
  drag.move({ clientX: 350, clientY: 120 }); drag.end();
  await p.wait(5);
  t.ok(JSON.stringify(last.points[1]) === '[350,120,500]', 'dragged: ' + JSON.stringify(last.points[1]));
  P.sel = [{ spec: '0,0', i: 0 }]; await P.remove();
  t.ok(last.points.length === 2 && !last.edges.some(e => e[0] > 1 || e[1] > 1), 'deleted, links renumbered');
  await P.restore(P.undoStack, P.redoStack);
  t.ok(last.points.length === 3, 'undone');
  P.keys({ key: 'Escape' }); P.keys({ key: 'Escape' });
  t.ok(!P.on, 'Esc twice leaves the mode');
  void ev; void d;
};
