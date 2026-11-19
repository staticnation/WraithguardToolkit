/* Animation groups and which scripts run (73_wg_scripts.js, 74_wg_lua.js, 18_cellpreview.js):
   PlayGroup and openmw.animation play a group on the object's mesh, on the whole body or on
   some of its bone groups - asked of the engine by `__grp/<spec>/<model>` (an NPC: `group=`
   among its flags), the spec naming each bone group's group and when it began; back to the
   idle when its loops are done, and what was queued behind it plays then. PlayGroup's flag
   queues (0), starts at once (1) or loops (2). A group the `.kf` lacks is reported and
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
        if (c === 'scene_scripts') return { objects: { banner: 'bannerscript', guard_b: 'guardscript' }, scripts: { bannerscript: { id: 'BannerScript', text: 'begin BannerScript\nset TownLit to TownLit + 1\nend' },
          guardscript: { id: 'GuardScript', text: 'begin GuardScript\nshort done\nif ( done == 0 )\n  PlayGroup Idle 1\n  PlayGroup Idle3\n  set done to 1\nelseif ( done == 1 )\n  LoopGroup Idle3 2 2\n  set done to 2\nendif\nend' } }, start: [], globals: [{ name: 'TownLit', kind: 's', value: 0 }], actors: { strider: { tag: 'CREA', items: [], stats: [100, 0, 100] } } };
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
    ['NPC_MESH_PREFIX', 'CORPSE_SLAB', 'frac2', 'PHASE_BINS', 'SPEED_LO', 'animModel', 'animSpan', 'phaseBins'].map(n => pick(cp, n)).join('\n'), ctx);
  const S = ctx.WgScripts, L = ctx.WgLua;
  S.on = true;
  await S.gather();
  const tick = async () => { await new Promise(r => setImmediate(r)); };

  // PlayGroup on the strider: the group's length from its `.kf`, once, then back to the idle.
  const strider = { key: 'c1', id: 'strider' };
  const slot = (k, i) => S.anims.get(k) && S.anims.get(k).slots[i];
  S.playAnim('c1', 'idle2', 0, 1, 'PlayGroup'); await tick();
  const a = slot('c1', 0);
  t.ok(a && a.group === 'Idle2' && Math.abs(a.len - 4) < 1e-3 && a.ends === 1000 + 4000 && a.at === 40 && [0, 1, 2, 3].every(i => slot('c1', i) === a), 'played on the whole body: the file\'s spelling, its length, when it ends ' + JSON.stringify(a));
  t.ok(S.actorOf(strider).anim && S.actorOf(strider).anim.group === 'Idle2', 'the Actors list says what it plays');
  const over = S.overlay(cell).refs.find(r => r.key === 'c1');
  t.ok(over.anim === 'Idle2@40', 'the reference carries the request: one slot, when it began ' + JSON.stringify(over.anim));
  t.ok(ctx.animModel('r/xsiltstrider.nif', over.anim) === '__grp/Idle2@40/r/xsiltstrider.nif', 'drawn as the group\'s mesh');
  t.ok(ctx.animModel('__npc/guard_b?night', 'Idle3@1') === '__npc/guard_b?night&group=Idle3@1' && ctx.animModel('__npc/x', 'Idle3@1') === '__npc/x?group=Idle3@1', 'an NPC asks by its flags');
  t.ok(ctx.animModel('__corpse_slab', 'Idle3') === '__corpse_slab' && ctx.animModel('x.nif', 'a/b?c') === '__grp/abc/x.nif', 'the stand-ins are left alone, and a group cannot break the path');
  // The engine times it, so the renderer keeps the file's clock for it.
  const info = { animated: true, parts: [{ anim: { nodes: [{ start: 1000, stop: 1004 }] } }] };
  t.ok(JSON.stringify(ctx.phaseBins([{ ref: over }], info)) === '[{"all":true}]', 'unshifted');
  now = 5001; S.animsFrame();
  t.ok(!S.anims.has('c1') && !S.actorOf(strider).anim && !S.overlay(cell).refs.find(r => r.key === 'c1').anim, 'back to the idle when it ends');

  // LoopGroup twice more: three lengths, at its speed. A group the file lacks: reported, nothing changes.
  now = 0; S.playAnim('c1', 'Idle2', 2, 2, 'LoopGroup'); await tick();
  t.ok(slot('c1', 0).ends === 4 * 3 / 2 * 1000 && S.animSpec('c1') === 'Idle2@40*2', 'loops at its speed: ' + slot('c1', 0).ends + ' ' + S.animSpec('c1'));
  S.stopAnim('c1');
  S.playAnim('c1', 'Swim', 0, 1, 'PlayGroup'); await tick();
  t.ok(!S.anims.has('c1') && S.log.some(l => /has no group Swim \(it has Idle, Idle2\)/.test(l.text)), 'a group it lacks is named, with what it has');
  S.playAnim('a1', 'Idle2', 0, 1, 'PlayGroup'); await tick();
  t.ok(!S.anims.has('a1') && S.log.some(l => /furn_banner\.nif has no group Idle2 \(no \.kf beside it\)/.test(l.text)), 'a mesh with no .kf is named');
  t.ok(calls.filter(c => c[0] === 'anim_groups' && c[1].models[0] === 'r/xsiltstrider.nif').length === 1, 'a model\'s groups are asked once');

  // Bone groups and the queue, on the NPC: legs walking (Idle), arms in Idle3, a queued Idle for the legs.
  now = 0;
  S.playAnim('n1', 'Idle', 0, 1, 'test', { mask: 1 }); await tick();
  S.playAnim('n1', 'Idle3', 0, 1, 'test', { mask: 14 }); await tick();
  t.ok(S.animSpec('n1') === 'Idle@40,Idle3@40,Idle3@40,Idle3@40' && JSON.stringify(S.animSlots('n1')) === '["Idle","Idle3","Idle3","Idle3"]', 'four slots: ' + S.animSpec('n1'));
  t.ok(S.playAnim('n1', 'Idle3', 1, 1, 'test', { mask: 1, queued: true }) === null && S.anims.get('n1').queue.length === 1, 'queued behind the legs\' play');
  ctx.App.R._uvSecs = 42; now = 2001; S.animsFrame(); await tick();
  t.ok(slot('n1', 0).group === 'Idle3' && slot('n1', 0).at === 42 && slot('n1', 0).ends === 2001 + 8000 && slot('n1', 1).group === 'Idle3' && slot('n1', 1) !== slot('n1', 0), 'the legs\' play ended, the queued one took them, the arms played on ' + S.animSpec('n1'));
  now = 4001; S.animsFrame();
  t.ok(slot('n1', 1) === null && slot('n1', 0) && S.animSpec('n1') === 'Idle3@42,,,', 'the arms back to the idle, the legs still busy: ' + S.animSpec('n1'));
  S.stopAnim('n1');
  t.ok(!S.anims.has('n1'), 'all stopped');

  // openmw.animation through the page's effects; an NPC's groups come by its plain path.
  await L.start();
  L.apply({ effects: [{ k: 'anim', key: 'n1', group: 'idle3', loops: -1, speed: 1, mask: 15 }] }); await tick();
  t.ok(slot('n1', 0) && slot('n1', 0).group === 'Idle3' && slot('n1', 0).ends === null, 'forever until something else: ' + JSON.stringify(slot('n1', 0)));
  t.ok(calls.some(c => c[0] === 'anim_groups' && c[1].models[0] === '__npc/guard_b'), 'the NPC\'s groups asked without its flags');
  L.apply({ effects: [{ k: 'anim', key: 'n1', group: 'idle', loops: 0, speed: 1, mask: 1, queued: true }] }); await tick();
  t.ok(S.anims.get('n1').queue.length === 1, 'playQueued waits');
  const f = L.delta(0.05);
  t.ok(f.anims && JSON.stringify(f.anims.n1) === '["Idle3","Idle3","Idle3","Idle3"]', 'the engine is told what each bone group plays ' + JSON.stringify(f.anims));
  t.ok(!L.delta(0.05).anims, 'and only when it changes');
  L.apply({ effects: [{ k: 'animClear', key: 'n1' }] });
  t.ok(S.anims.get('n1').queue.length === 0, 'clearAnimationQueue');
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

  // MWScript's flags (the guard's script ran in those frames): PlayGroup's 1 at once, its
  // default 0 queued behind it; then LoopGroup's 2 at once and looping until something else.
  S.anims.delete('n1'); S.inst.get('n1').locals = null;
  S.frame(0.05); await tick();
  t.ok(slot('n1', 0) && slot('n1', 0).group === 'Idle' && S.anims.get('n1').queue.length === 1 && S.anims.get('n1').queue[0].group.toLowerCase() === 'idle3', 'PlayGroup 1 at once, PlayGroup queued ' + JSON.stringify(S.animSlots('n1')));
  S.frame(0.05); await tick();
  t.ok(slot('n1', 0).group === 'Idle3' && slot('n1', 0).loops === Infinity && slot('n1', 0).ends === null, 'LoopGroup with flag 2 cuts in and loops ' + JSON.stringify(slot('n1', 0)));

  // A group with no length (a banner's Idle, its still pose) is the mesh's own idle: nothing
  // played, nothing said. The same group asked for again while it plays: not started over.
  ctx.Engine._call = ctx.Engine.call;
  ctx.Engine.call = async (c, a) => c === 'anim_groups' ? JSON.stringify({ 'f/furn_banner.nif': [['Idle', 0, 0], ['Idle2', 0, 2]] }) : ctx.Engine._call(c, a);
  S._groups.clear(); S.anims.delete('a1');
  const logged = S.log.length;
  S.playAnim('a1', 'Idle', 0, 1, 'PlayGroup'); await tick();
  t.ok(!S.anims.has('a1') && S.log.length === logged, 'PlayGroup Idle on a banner: its still pose, quietly');
  const first = S.playAnim('a1', 'Idle2', 0, 1, 'PlayGroup'); await tick();
  const again = S.playAnim('a1', 'Idle2', 0, 1, 'PlayGroup');
  t.ok(first && again === first, 'asked again while it plays: the same play, not started over');
  ctx.Engine.call = ctx.Engine._call;
};
