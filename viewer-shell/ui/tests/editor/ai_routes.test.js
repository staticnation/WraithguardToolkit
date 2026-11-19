/* The actors' AI (77_wg_ai.js, 73_wg_scripts.js `actorsFrame`, `go`): routes over the
   loaded cells' pathgrids (A*, around what the grid goes around; two exterior grids joined
   where their points meet across a border; a straight line without a grid), a wanderer's
   next point within its range and the idle it plays by its chances, and the walk on the
   actor's mesh while it moves - its idle when it stops, a script's own group first, and a
   mesh with no walk not asked again. */
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
};
