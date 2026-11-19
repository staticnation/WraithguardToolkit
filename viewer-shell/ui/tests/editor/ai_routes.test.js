/* The actors' AI (77_wg_ai.js, 73_wg_scripts.js `actorsFrame`, `go`): routes over the
   loaded cells' pathgrids (A*, around what the grid goes around; two exterior grids joined
   where their points meet across a border; a straight line without a grid), a wanderer's
   next point within its range and the idle it plays by its chances, and the walk on the
   actor's mesh while it moves - its idle when it stops, a script's own group first, and a
   mesh with no walk not asked again. And the glide: an actor drawn live is moved where it
   stands (18_cellpreview.js `previewMoveRef`), its matrix, cull run and pickable with it,
   without the scene drawn again. */
'use strict';
const { wgtb, addTables, sceneTables } = require('./harness');
const vm = require('vm');
const fs = require('fs');
const path = require('path');

const UI = path.join(__dirname, '..', '..');
const src = f => fs.readFileSync(path.join(UI, 'src', f), 'utf8').replace(/^const (\w+)=/mg, 'var $1=');

module.exports = async (t) => {
  const ref = (id, key, pos) => ({ id, key, pos, rot: [0, 0, 0], scale: 1 });
  // Two exterior cells side by side; the guard stands in the first.
  const cells = [{ kind: 'ext', gx: 0, gy: 0, name: '', refs: [ref('guard_b', 'n1', [100, 100, 0]), ref('rat', 'c1', [500, 500, 0])] }, { kind: 'ext', gx: 1, gy: 0, name: '', refs: [] }];
  /* Cell 0,0: a U around a wall from (1000, 0) to (1000, 2000): 0 (100,100) - 1 (100,3000) -
     2 (2000,3000) - 3 (2000,100), and 4 (8100,100) at the east border. Cell 1,0: 0 (8250,100)
     just across it, 1 (9000,100). The two grids share no edge. */
  const grids = [['0,0', 'Morrowind.esm'], ['1,0', 'Morrowind.esm']];
  const points = [[0, 100, 100, 0], [0, 100, 3000, 0], [0, 2000, 3000, 0], [0, 2000, 100, 0], [0, 8100, 100, 0], [1, 8250, 100, 0], [1, 9000, 100, 0]];
  const edges = [[0, 0, 1], [0, 1, 2], [0, 2, 3], [0, 3, 4], [1, 0, 1]];
  const calls = [];
  let now = 0;
  const ctx = {
    console, Math, JSON, Map, Set, Promise, Object, Array, String, Number, Error, Infinity, isFinite,
    performance: { now: () => now }, requestAnimationFrame: () => 0, cancelAnimationFrame() {}, setTimeout: (f) => f(), clearTimeout() {},
    localStorage: { getItem: () => null, setItem() {} },
    document: { activeElement: null },
    $: () => null, escHtml: s => String(s), schedulePreview() {}, toast() {},
    App: { _scene: { origin: [0, 0], cells }, R: { _uvSecs: 5, groundZ: () => 0, pickables: [{ refKey: 'n1', model: '__npc/guard_b' }, { refKey: 'c1', model: 'r/rat.nif' }] } },
    WgSound: { place: { on: false } },
    Engine: {
      async bytes(c, a) {
        calls.push([c, a]);
        if (c === 'scene_scripts') return sceneTables(await this.call(c, a));
        if (c === 'pathgrid') return wgtb([['grids', 'ss', grids], ['points', 'uddd', points], ['edges', 'uuu', edges]]);
        throw new Error('no ' + c);
      },
      async call(c, a) {
        calls.push([c, a]);
        if (c === 'scene_scripts') return { objects: {}, scripts: {}, start: [], globals: [], actors: { guard_b: { tag: 'NPC_', items: [], stats: [100, 50, 80] }, rat: { tag: 'CREA', items: [], stats: [10, 0, 10] } } };
        if (c === 'anim_groups') return JSON.stringify(Object.fromEntries(a.models.map(m => [m, m.startsWith('__npc') ? [['Idle', 0, 2], ['Idle2', 2, 4], ['WalkForward', 10, 11], ['RunForward', 12, 12.5]] : [['Idle', 0, 2]]])));
        return {};
      },
    },
  };
  vm.createContext(ctx);
  addTables(ctx);
  vm.runInContext(src('38_wg_cellkey.js') + '\n' + src('73_wg_scripts.js') + '\n' + src('77_wg_ai.js'), ctx);
  const S = ctx.WgScripts, W = ctx.WgAI;
  const tick = async () => { for (let i = 0; i < 3; i++) await new Promise(r => setImmediate(r)); };
  S.on = true;
  await S.gather(); await W.load(); await tick();

  // The graph: the two grids, joined across the border.
  t.ok(W.grid && W.grid.pts.length === 7, 'every point of both cells');
  t.ok(W.grid.adj[4].includes(5) && W.grid.adj[5].includes(4), 'joined where their points meet across the border');
  t.ok(calls.some(c => c[0] === 'pathgrid' && JSON.stringify(c[1].cells) === '["0,0","1,0"]'), 'asked for the loaded cells');

  // A route around the wall, by the grid; a short hop straight; no grid, straight.
  const r = W.route([100, 100, 0], [2000, 150, 0]);
  t.ok(JSON.stringify(r.map(p => p.slice(0, 2))) === '[[100,3000],[2000,3000],[2000,100],[2000,150]]', 'around the wall, not through it: ' + JSON.stringify(r));
  t.ok(W.route([100, 100, 0], [200, 200, 0]).length === 1, 'a short hop is straight');
  const across = W.route([2000, 100, 0], [9000, 120, 0]);
  t.ok(across.some(p => p[0] === 8250) && across.some(p => p[0] === 8100), 'across the border by the join ' + JSON.stringify(across));

  // Wander: a grid point within range of home; the idle by the chances.
  for (let i = 0; i < 20; i++) { const p = W.wanderPoint([100, 100, 0], 3000); t.ok(Math.hypot(p[0] - 100, p[1] - 100) <= 3000 && [100, 2000].includes(p[0]), 'a grid point in range'); if (i > 2) break; }
  t.ok(W.pickIdle([0, 0, 0, 0, 0, 0, 0, 0]) === null, 'no chances, no idle');
  t.ok(W.pickIdle([60, 0, 20, 0, 0, 0, 0, 0], 0.5) === 'Idle2' && W.pickIdle([60, 0, 20, 0, 0, 0, 0, 0], 0.7) === 'Idle4' && W.pickIdle([60, 0, 20, 0, 0, 0, 0, 0], 0.9) === null, 'weighted by chance, a roll past their sum stands');

  // Travel by the grid: the guard walks the U, with its walk playing; at the end, its idle.
  const guard = { key: 'n1', id: 'guard_b' };
  S.ai(guard, 'travel', [2000, 150, 0], {});
  S.actorsFrame(0.1); await tick();
  const A = S.actorOf(guard);
  const p1 = S.posOf(guard);
  t.ok(Math.abs(p1[0] - 100) < 1 && p1[1] > 100, 'heads north first, along the grid: ' + JSON.stringify(p1));
  t.ok(W.own('n1') && W.own('n1').group === 'WalkForward' && W.own('n1').ends === null, 'walking ' + JSON.stringify(S.animSlots('n1')));
  for (let i = 0; i < 1000 && !A.ai.done; i++) S.actorsFrame(0.1);
  await tick();
  const p2 = S.posOf(guard);
  t.ok(A.ai.done && Math.hypot(p2[0] - 2000, p2[1] - 150) <= 64, 'there: ' + JSON.stringify(p2));
  S.actorsFrame(0.1);
  t.ok(!W.own('n1'), 'and standing in its idle');

  // Running to a fight; a script's group wins over the walk.
  S.ai(guard, 'travel', [100, 100, 0], {});
  S.playAnim('n1', 'Idle2', Infinity, 1, 'PlayGroup'); await tick();
  S.actorsFrame(0.1); await tick();
  t.ok(S.animSlots('n1')[0] === 'Idle2' && !W.own('n1'), 'the script\'s group stays while it walks');
  S.stopAnim('n1');
  A.combat = { key: 'c1', id: 'rat' }; S.actorOf(A.combat);
  S.actorsFrame(0.1); await tick();
  t.ok(W.own('n1') && W.own('n1').group === 'RunForward', 'runs to a fight ' + JSON.stringify(S.animSlots('n1')));
  A.combat = null; A.ai.done = true;
  S.actorsFrame(0.1);

  // A mesh without a walk: asked once, then it slides as before, and nothing is logged.
  const rat = { key: 'c1', id: 'rat' };
  S.ai(rat, 'travel', [3000, 500, 0], {});
  S.actorsFrame(0.1); await tick();
  S.actorsFrame(0.1); await tick();
  S.actorsFrame(0.1); await tick();
  const asks = calls.filter(c => c[0] === 'anim_groups' && c[1].models[0] === 'r/rat.nif').length;
  t.ok(asks === 1 && W.lacks.has('r/rat.nif|walkforward') && !S.anims.has('c1'), 'a mesh without a walk is not asked again (' + asks + ')');
  t.ok(!S.log.some(l => /has no group WalkForward/.test(l.text)), 'and nothing is logged for it');

  // Wander: walks to a point, then waits with the idle its chances give.
  S.stopAnim('n1');
  S.ai(guard, 'wander', [3000, 0, 0, 100, 0, 0, 0, 0, 0, 0, 0], {});
  const ai = A.ai;
  t.ok(ai.idle.length === 8 && ai.idle[0] === 100, 'AiWander\'s idle chances kept');
  let waited = false;
  for (let i = 0; i < 3000 && !waited; i++){ S.actorsFrame(0.1); if (ai.wait > 0) waited = true; }
  await tick();
  t.ok(waited && W.own('n1') && W.own('n1').group === 'Idle2' && W.own('n1').ends !== null, 'arrived, and plays Idle2 once while it waits ' + JSON.stringify(S.animSlots('n1')));

  // The glide: a live actor moved in place, the scene not drawn again.
  const cp = fs.readFileSync(path.join(UI, 'src', '18_cellpreview.js'), 'utf8');
  const pick = name => { const i = cp.search(new RegExp('^function ' + name + '\\b', 'm')); const j = cp.slice(i + 1).search(/^(function |const |\/\*\*|\/\* )/m); return cp.slice(i, i + 1 + j); };
  vm.runInContext(['refMatrix', 'worldAabb', 'previewMoveRef'].map(pick).join('\n'), ctx);
  const moved = [];
  ctx.App.R.moveInstance = (b, k, m) => { moved.push([b.id, k, m[3], m[7]]); return !!b.live; };
  const arr = new Float32Array(24), runs = [{ at: 0, n: 2, b: [0, 0, 0, 10, 10, 10] }], pk = { m: null };
  ctx.App._liveRefs = new Map([['n1', { arr, i: 1, batches: [{ id: 'a', live: true }, { id: 'b', live: true }], runs, vis: { x0: -10, y0: -10, z0: 0, x1: 10, y1: 10, z1: 100 }, full: arr, k: 1, pick: pk }]]);
  S.actorOf(rat).ai = null;   // the rat (not drawn live here) stands
  S.ai(guard, 'travel', [5000, 100, 0], {}); A.combat = null;
  S.actorsFrame(0.1); await tick();   // the walk starts: that is a new mesh, drawn again
  moved.length = 0; S._dirtyScene = false;
  S.actorsFrame(0.1);
  const at = S.posOf(guard);
  t.ok(moved.length === 2 && moved.every(x => x[1] === 1 && Math.abs(x[2] - at[0]) < 1e-3 && Math.abs(x[3] - at[1]) < 1e-3), 'its matrix written into both batches ' + JSON.stringify(moved));
  t.ok(Math.abs(arr[12 + 3] - at[0]) < 1e-3 && runs[0].b[3] >= at[0] + 10 - 1e-3 && pk.m && Math.abs(pk.m[3] - at[0]) < 1e-3, 'the instance array, its cull run and its pickable with it');
  t.ok(S._dirtyScene === false, 'and the scene is not drawn again for it');
  ctx.App._liveRefs = new Map();
  S._dirtyScene = false;
  S.actorsFrame(0.1);
  t.ok(S._dirtyScene === true, 'one not drawn live waits for the scene to be drawn again');

  // The record's own pace, by the game's formulas.
  const sp = W.speeds({ move: { speed: 50, athletics: 40, creature: false } });
  t.ok(Math.abs(sp.walk - 150) < 1e-6 && Math.abs(sp.run - 150 * (0.4 + 1.75)) < 1e-6, 'an NPC walks and runs by its Speed and Athletics ' + JSON.stringify(sp));
  const cr = W.speeds({ move: { speed: 20, athletics: -1, creature: true } });
  t.ok(Math.abs(cr.walk - (5 + 0.2 * 295)) < 1e-6 && Math.abs(cr.run - cr.walk * 1.75) < 1e-6, 'a creature by its own settings ' + JSON.stringify(cr));

  // Fleeing: Flee 40 runs once it has lost 60 of every 100 health, and stops once clear.
  ctx.App._liveRefs = new Map();
  const R2 = S.actorOf(rat);
  R2.rates = [0, 80, 40, 0]; R2.max = [100, 0, 0]; R2.hp = 50; R2.ai = null;
  R2.combat = guard; S.place(rat, { pos: [500, 500, 0] }); S.place(guard, { pos: [400, 500, 0] }); A.ai = null; A.combat = null;
  t.ok(!W.flees(R2), 'half its health: it fights on');
  R2.hp = 39;
  t.ok(W.flees(R2), 'past sixty lost: it runs');
  S.actorsFrame(0.1);
  const rp = S.posOf(rat);
  t.ok(R2._fleeing && R2._fleeTo[0] > 2000 && S.log.some(l => /rat flees from guard_b/.test(l.text)), 'it runs for a point away from its foe (by the grid) ' + JSON.stringify(R2._fleeTo));
  for (let i = 0; i < 2000 && R2.combat; i++) S.actorsFrame(0.1);
  t.ok(!R2.combat && !R2._fleeing && S.log.some(l => /rat stops fleeing/.test(l.text)) && S.dist(rat, guard) >= W.FLEE - 1, 'and stops once clear ' + S.dist(rat, guard));

  // A greeting: Hello 30 within 30 x 6 of the player turns and stands, said once.
  ctx.App.R.cam = { tx: 1000, ty: 1000, tz: 116, dist: 0.0001, az: 0, el: 0 };
  A.rates = [30, 30, 0, 0]; A.combat = null;
  S.place(guard, { pos: [1100, 1000, 0], rot: [0, 0, 0] });
  S.ai(guard, 'wander', [2000, 0, 0], {});
  const before = S.posOf(guard);
  S.actorsFrame(0.1); S.actorsFrame(0.1);
  const after = S.posOf(guard);
  t.ok(Math.abs(after[0] - before[0]) < 1e-6 && Math.abs(after[1] - before[1]) < 1e-6, 'it stands while it greets');
  t.ok(Math.abs(S.rotOf(guard)[2] - Math.atan2(-100, 0)) < 1e-6, 'facing the player');
  t.ok(S.log.filter(l => /guard_b greets the player/.test(l.text)).length === 1, 'and says so once');
  ctx.App.R.cam.tx = 5000;
  S.actorsFrame(0.1);
  t.ok(!A._greeted, 'the player gone, it goes back to its wander');

  // Room: a moving actor keeps out of another's.
  A.ai = null; S.place(guard, { pos: [3000, 3000, 0] }); S.place(rat, { pos: [3000, 3100, 0] }); R2.combat = null; R2.ai = null;
  S.ai(guard, 'travel', [3000, 3400, 0], {});
  for (let i = 0; i < 20; i++) S.actorsFrame(0.1);
  let closest = Infinity;
  for (let i = 0; i < 40; i++){ S.actorsFrame(0.05); closest = Math.min(closest, S.dist(guard, rat)); }
  t.ok(closest >= W.ROOM - 1, 'never closer than the room it keeps: ' + closest.toFixed(1));

  // The navmesh, when one is read: two squares joined by a shared edge, a third apart.
  const sq = (x, y) => ({ a: 0, v: new Float32Array([x, y, 0, x + 100, y, 0, x + 100, y + 100, 0, x, y + 100, 0]) });
  W.nav = W.buildNav([sq(0, 0), sq(100, 0), sq(500, 500), { a: 1, v: new Float32Array([0, 0, 0, 1, 0, 0, 1, 1, 0]) }], 0);
  t.ok(W.nav.polys.length === 3 && W.nav.adj[0].length === 1 && W.nav.adj[0][0][0] === 1, 'joined by the shared edge, the other agent left out');
  const nr = W.routeNav([50, 50, 0], [150, 50, 0]);
  t.ok(JSON.stringify(nr) === '[[100,50,0],[150,50,0]]', 'through the shared edge\'s middle ' + JSON.stringify(nr));
  t.ok(W.routeNav([50, 50, 0], [550, 550, 0]) === null, 'not joined: the pathgrid is tried');
  t.ok(JSON.stringify(W.route([50, 50, 0], [150, 50, 0])) === '[[100,50,0],[150,50,0]]', 'route takes the navmesh first');
  t.ok(W.agentOf([{ h: [10, 10, 10] }, { h: [29, 28, 66] }]) === 1, 'the agent of a person\'s size');
  W.nav = null;
};
