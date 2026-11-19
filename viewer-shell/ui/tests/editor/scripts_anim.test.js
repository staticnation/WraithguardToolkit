/* Animation groups and which scripts run (73_wg_scripts.js, 74_wg_lua.js, 18_cellpreview.js):
   PlayGroup and openmw.animation play a group on the object's mesh - asked of the engine
   by `__grp/<group>/<model>` (an NPC: `group=` among its flags), started at its first
   frame, back to the idle when its loops are done; a group the `.kf` lacks is reported and
   nothing changes. A Lua script or MWScript turned off is not run, and the choice is kept;
   a Lua change starts the Lua again without it. */
'use strict';
const { wgtb, addTables, sceneTables } = require('./harness');
const vm = require('vm');
const fs = require('fs');
const path = require('path');

const UI = path.join(__dirname, '..', '..');
const src = f => fs.readFileSync(path.join(UI, 'src', f), 'utf8').replace(/^const (\w+)=/mg, 'var $1=');
/** One top-level function (or const) of a file, by name, for the parts of the renderer's
 *  file that need no GL. */
const pick = (text, name) => {
  const i = text.search(new RegExp('^(function ' + name + '\\b|const ' + name + '=)', 'm'));
  if (i < 0) throw new Error('no ' + name);
  const j = text.slice(i + 1).search(/^(function |const |\/\*\*|\/\* )/m);
  return text.slice(i, j < 0 ? undefined : i + 1 + j).replace(/^const (\w+)=/mg, 'var $1=');
};

module.exports = async (t) => {
  const ref = (id, key, pos) => ({ id, key, pos, rot: [0, 0, 0], scale: 1 });
  const cell = { kind: 'ext', gx: 0, gy: 0, name: 'Balmora', refs: [ref('strider', 'c1', [0, 0, 0]), ref('guard_b', 'n1', [100, 0, 0]), ref('banner', 'a1', [200, 0, 0])] };
  const store = new Map();
  const calls = [], starts = [];
  let now = 1000;
  const ctx = {
    console, Math, JSON, Map, Set, Promise, Object, Array, String, Number, Error, Infinity, isFinite,
    performance: { now: () => now }, requestAnimationFrame: () => 0, cancelAnimationFrame() {}, setTimeout: (f) => f(), clearTimeout() {},
    localStorage: { getItem: k => store.has(k) ? store.get(k) : null, setItem: (k, v) => store.set(k, String(v)) },
    document: { activeElement: null },
    $: () => null, escHtml: s => String(s), schedulePreview() {}, toast() {},
    App: { _scene: { origin: [0, 0], cells: [cell] }, R: { _uvSecs: 40, cam: { az: 0 }, pickables: [
      { refKey: 'c1', model: 'r/xsiltstrider.nif' }, { refKey: 'n1', model: '__npc/guard_b?night' }, { refKey: 'a1', model: 'f/furn_banner.nif' }] } },
    WgSound: { place: { on: false } },
    Engine: {
      async bytes(c, a) {
        if (c === 'scene_scripts') return sceneTables(await this.call(c, a));
        if (c === 'lua_scene') {
          const T = { strider: 'CREA', guard_b: 'NPC_', banner: 'ACTI' };
          return wgtb([['head', 's', [['']]], ['scripts', 's', [['scripts/mod/a.lua'], ['scripts/mod/b.lua'], ['scripts/mod/menu.lua']]],
            ['script_flags', 'us', [[0, 'GLOBAL'], [1, 'NPC'], [2, 'MENU']]], ['types', 'ss', (a.ids || []).filter(id => T[id]).map(id => [id, T[id]])]]);
        }
        throw new Error('no ' + c);
      },
      async call(c, a) {
        calls.push([c, a]);
        if (c === 'scene_scripts') return { objects: { banner: 'bannerscript' }, scripts: { bannerscript: { id: 'BannerScript', text: 'begin BannerScript\nset TownLit to TownLit + 1\nend' } }, start: [], globals: [{ name: 'TownLit', kind: 's', value: 0 }], actors: { strider: { tag: 'CREA', items: [], stats: [100, 0, 100] } } };
        if (c === 'anim_groups') return JSON.stringify(Object.fromEntries(a.models.map(m => [m, m.startsWith('__npc') ? [['Idle', 0, 2], ['Idle3', 10, 14]] : m.includes('strider') ? [['Idle', 0, 26.667], ['Idle2', 26.667, 30.667]] : []])));
        if (c === 'lua_run_start') { starts.push(a); return { backend: 'Lua 5.4', resources: '', note: '', answer: { effects: [], log: [], status: null } }; }
        if (c === 'lua_run_step') return { effects: [], log: [], status: null };
        return {};
      },
    },
  };
  vm.createContext(ctx);
  addTables(ctx);
  const cp = fs.readFileSync(path.join(UI, 'src', '18_cellpreview.js'), 'utf8');
  vm.runInContext(src('73_wg_scripts.js') + '\n' + src('74_wg_lua.js') + '\n' +
    ['NPC_MESH_PREFIX', 'CORPSE_SLAB', 'frac2', 'PHASE_BINS', 'SPEED_LO', 'animModel', 'animSpan', 'animStart', 'phaseBins'].map(n => pick(cp, n)).join('\n'), ctx);
  const S = ctx.WgScripts, L = ctx.WgLua;
  S.on = true;
  await S.gather();
  const tick = async () => { await new Promise(r => setImmediate(r)); };

  // PlayGroup on the strider: the group's length from its `.kf`, once, then back to the idle.
  const strider = { key: 'c1', id: 'strider' };
  S.playAnim('c1', 'idle2', 0, 1, 'PlayGroup'); await tick();
  const a = S.anims.get('c1');
  t.ok(a && a.group === 'Idle2' && Math.abs(a.len - 4) < 1e-3 && a.ends === 1000 + 4000 && a.at === 40, 'played: the file\'s spelling, its length, when it ends ' + JSON.stringify(a));
  t.ok(S.actorOf(strider).anim && S.actorOf(strider).anim.group === 'idle2', 'the Actors list says what it plays');
  const over = S.overlay(cell).refs.find(r => r.key === 'c1');
  t.ok(over.anim === 'Idle2' && over.animAt === 40 && over.animSpeed === 1, 'the reference carries it to the renderer ' + JSON.stringify(over));
  t.ok(ctx.animModel('r/xsiltstrider.nif', 'Idle2') === '__grp/Idle2/r/xsiltstrider.nif', 'drawn as the group\'s mesh');
  t.ok(ctx.animModel('__npc/guard_b?night', 'Idle3') === '__npc/guard_b?night&group=Idle3' && ctx.animModel('__npc/x', 'Idle3') === '__npc/x?group=Idle3', 'an NPC asks by its flags');
  t.ok(ctx.animModel('__corpse_slab', 'Idle3') === '__corpse_slab' && ctx.animModel('x.nif', 'a/b?c') === '__grp/abc/x.nif', 'the stand-ins are left alone, and a group cannot break the path');
  // From its first frame: the shift puts the clip's start at the clock's reading when it began.
  const info = { animated: true, parts: [{ anim: { nodes: [{ start: 26.667, stop: 30.667 }] } }] };
  const bins = ctx.phaseBins([{ ref: over }], info);
  t.ok(bins.length === 1 && bins[0].speed === 1 && Math.abs(40 * bins[0].speed + bins[0].shift - 26.667) < 1e-6, 'starts at the group\'s first frame ' + JSON.stringify(bins));
  now = 5001; S.animsFrame();
  t.ok(!S.anims.has('c1') && !S.actorOf(strider).anim && !S.overlay(cell).refs.find(r => r.key === 'c1').anim, 'back to the idle when it ends');

  // LoopGroup twice more: three lengths. A group the file lacks: reported, nothing changes.
  now = 0; S.playAnim('c1', 'Idle2', 2, 2, 'LoopGroup'); await tick();
  t.ok(S.anims.get('c1').ends === 4 * 3 / 2 * 1000, 'loops at its speed: ' + S.anims.get('c1').ends);
  S.playAnim('c1', 'Swim', 0, 1, 'PlayGroup'); await tick();
  t.ok(!S.anims.has('c1') && S.log.some(l => /has no group Swim \(it has Idle, Idle2\)/.test(l.text)), 'a group it lacks is named, with what it has');
  S.playAnim('a1', 'Idle2', 0, 1, 'PlayGroup'); await tick();
  t.ok(!S.anims.has('a1') && S.log.some(l => /furn_banner\.nif has no group Idle2 \(no \.kf beside it\)/.test(l.text)), 'a mesh with no .kf is named');
  t.ok(calls.filter(c => c[0] === 'anim_groups' && c[1].models[0] === 'r/xsiltstrider.nif').length === 1, 'a model\'s groups are asked once');

  // openmw.animation through the page's effects; an NPC's groups come by its plain path.
  await L.start();
  L.apply({ effects: [{ k: 'anim', key: 'n1', group: 'idle3', loops: -1, speed: 1 }] }); await tick();
  t.ok(S.anims.get('n1') && S.anims.get('n1').group === 'Idle3' && S.anims.get('n1').ends === null, 'forever until something else: ' + JSON.stringify(S.anims.get('n1')));
  t.ok(calls.some(c => c[0] === 'anim_groups' && c[1].models[0] === '__npc/guard_b'), 'the NPC\'s groups asked without its flags');
  const f = L.delta(0.05);
  t.ok(f.anims && f.anims.n1 === 'Idle3', 'the engine is told what plays ' + JSON.stringify(f.anims));
  t.ok(!L.delta(0.05).anims, 'and only when it changes');
  L.apply({ effects: [{ k: 'animStop', key: 'n1', group: 'IDLE3' }] });
  t.ok(!S.anims.has('n1') && JSON.stringify(L.delta(0.05).anims) === '{}', 'cancel: stopped, and said');

  // Which scripts run.
  const list = await L.scriptList();
  t.ok(list.length === 2 && list[1].path === 'scripts/mod/b.lua' && list[1].flags[0] === 'NPC', 'the registered scripts, not the menu\'s ' + JSON.stringify(list));
  const before = starts.length;
  L.setOff(['Scripts\\Mod\\B.lua', '@builtins'], true);
  t.ok(L.isOff('scripts/mod/b.lua') && JSON.parse(store.get('wgLuaOff')).includes('scripts/mod/b.lua'), 'kept, in any spelling');
  await tick();
  t.ok(starts.length === before + 1 && starts[starts.length - 1].off.includes('scripts/mod/b.lua') && starts[starts.length - 1].off.includes('@builtins'), 'the Lua starts again without them ' + JSON.stringify(starts[starts.length - 1].off));
  L.setOff('scripts/mod/b.lua', false);
  t.ok(!L.isOff('scripts/mod/b.lua') && L.isOff('@builtins'), 'turned back on');

  // An MWScript turned off is not run.
  const g = () => S.state.globals.get('townlit').v;
  S.frame(0.05); const was = g();
  t.ok(was >= 1, 'the banner\'s script runs: ' + was);
  S.mwOff.add('bannerscript'); S.frame(0.05);
  t.ok(g() === was, 'and not once turned off');
  S.mwOff.clear(); S.frame(0.05);
  t.ok(g() === was + 1, 'and again once on');
};
