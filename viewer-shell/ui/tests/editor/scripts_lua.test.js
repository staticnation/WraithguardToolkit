/* OpenMW Lua in the scene, the page's side (74_wg_lua.js): the engine (viewcore::luarun,
   stubbed here; its Lua is tested in Rust) gets the scene - every object with its record
   type, then only what changed - and the page does what the scripts did: a lamp disabled
   and lit again, a global set, a barrel created and placed, an NPC sent on a Travel
   package, the weather and the music changed, a message shown; an activation reaches the
   engine; one frame is in flight at a time. */
'use strict';
const vm = require('vm');
const fs = require('fs');
const path = require('path');

const UI = path.join(__dirname, '..', '..');
const src = f => fs.readFileSync(path.join(UI, 'src', f), 'utf8').replace(/^const (\w+)=/mg, 'var $1=');

module.exports = async (t) => {
  const ref = (id, key, pos) => ({ id, key, pos, rot: [0, 0, 0], scale: 1 });
  const cell = { kind: 'ext', gx: 0, gy: 0, name: 'Balmora', refs: [ref('light_a', 'l1', [0, 0, 100]), ref('guard_b', 'n1', [0, 0, 0]), ref('ex_wall', 's1', [0, 900, 0])] };
  const weather = { selectedIndex: 0, options: ['Clear', 'Cloudy', 'Foggy', 'Overcast', 'Rain'].map(text => ({ text })), dispatchEvent() {} };
  const music = [], toasts = [], frames = [];
  let answer = null, hold = null;
  const ctx = {
    console, Math, JSON, Map, Set, Promise, Object, Array, String, Number, Error, Event: function (n) { this.type = n; },
    performance: { now: () => Date.now() }, requestAnimationFrame: () => 0, cancelAnimationFrame() {}, setTimeout,
    document: { activeElement: null },
    $: s => s === '#p_weather' ? weather : null, escHtml: s => String(s), schedulePreview() {}, toast: m => toasts.push(m),
    App: { _scene: { origin: [0, 0], cells: [cell] }, R: { cam: { tx: 0, ty: 0, tz: 116, dist: 0.0001, az: 0, el: 0 }, groundZ: () => 0 } },
    WgSound: { place: { on: true }, audio: () => ({}), placeGraph() {}, musicStop() {}, buffer: async st => { music.push(st.path); throw new Error('no audio'); } },
    Engine: {
      async call(c, a) {
        if (c === 'scene_scripts') return { objects: {}, scripts: {}, start: [], globals: [{ name: 'TownLit', kind: 's', value: 0 }], actors: { guard_b: { tag: 'NPC_', items: [], stats: [100, 50, 80] } } };
        if (c === 'lua_scene') { const T = { light_a: 'LIGH', guard_b: 'NPC_', ex_wall: 'STAT' }; const types = {}; for (const id of a.ids) if (T[id]) types[id] = T[id]; return { types }; }
        if (c === 'lua_run_start') return { backend: 'LuaJIT (E:/OpenMW/lua51.dll)', resources: 'E:/OpenMW/resources', note: '', answer: { effects: [], log: [], status: { global: 5, player: 2, local: 4, failing: [], timers: 0 } } };
        if (c === 'lua_run_step') {
          frames.push(JSON.parse(a.frame));
          if (hold) await hold;
          const r = answer || { effects: [], log: [], status: { global: 5, player: 2, local: 4, failing: [], timers: 0 } };
          answer = null;
          return r;
        }
        if (c === 'lua_run_stop') return {};
        return {};
      },
    },
  };
  vm.createContext(ctx);
  vm.runInContext(src('73_wg_scripts.js') + '\n' + src('74_wg_lua.js'), ctx);
  const S = ctx.WgScripts, L = ctx.WgLua;
  S.on = true;
  await S.gather();
  await L.start();
  t.ok(L.running && ctx.WgSound.place.scriptMusic === true, 'running, OpenMW\'s playlists choosing the music');

  // The first frame: the whole scene, each object with its record type; globals; the player.
  await L.frame(0.05);
  const f1 = frames[0];
  t.ok(f1 && f1.objects && f1.objects.l1.tag === 'LIGH' && f1.objects.n1.tag === 'NPC_' && f1.objects.s1.tag === 'STAT', 'every object, typed: ' + JSON.stringify(f1 && f1.objects));
  t.ok(f1.objects.l1.cell.name === 'Balmora' && f1.objects.l1.cell.ext === true && f1.objects.l1.pos[2] === 100, 'with its cell and place');
  t.ok(f1.globals && f1.globals.townlit && f1.globals.townlit.v === 0 && f1.player && f1.player.pos, 'the globals and the player');

  // What the scripts did, done.
  answer = { log: [{ kind: 'print', path: 'scripts/town/lamps.lua', text: 'lamps out' }], status: { global: 5, player: 2, local: 4, failing: [], timers: 1 }, effects: [
    { k: 'enable', key: 'l1', on: false },
    { k: 'global', name: 'townlit', v: 2 },
    { k: 'create', key: 'lua:1', id: 'barrel_01', cell: '', pos: [100, 200, 0], rz: 0 },
    { k: 'ai', key: 'n1', kind: 'travel', opt: { dest: [600, 0, 0] } },
    { k: 'weather', name: 'Rain' },
    { k: 'music', path: 'music/explore/a.mp3' },
    { k: 'msg', text: 'Welcome to Balmora' },
  ] };
  await L.frame(0.05);
  t.ok(S.disabled.has('l1'), 'the lamp is out');
  t.ok(S.state.globals.get('townlit').v === 2, 'TownLit is 2');
  t.ok(S.placed.some(p => p.key === 'lua:1' && p.pos[1] === 200), 'the barrel is placed');
  const G = S.actors.get('n1');
  t.ok(G && G.ai && G.ai.kind === 'travel', 'the guard travels');
  t.ok(weather.selectedIndex === 4 && toasts.includes('Welcome to Balmora'), 'rain, and the message');
  await new Promise(r => setTimeout(r, 5));
  t.ok(music.some(m => /explore\/a\.mp3$/.test(m)), 'the music: ' + music);
  t.ok(S.log.some(l => /lamps out/.test(l.text) && /town\/lamps\.lua/.test(l.script)), 'the print in the log');

  // The next frame sends only what changed: the lamp switched off, the barrel, the guard's
  // package; not the wall, and not the globals the engine already has.
  S.frame(0.05);   // the guard walks (73's actors), then Lua's frame goes out
  await new Promise(r => setTimeout(r, 5));
  const f3 = frames[frames.length - 1];
  t.ok(f3.objects && f3.objects.l1 && f3.objects.l1.enabled === false && f3.objects['lua:1'] && !f3.objects.s1, 'only the changed objects: ' + Object.keys(f3.objects || {}));
  t.ok(!f3.globals, 'the global the engine set is not sent back');
  t.ok(f3.actors && f3.actors.n1 && f3.actors.n1.ai && f3.actors.n1.ai.kind === 'travel', 'the guard\'s package is');

  // An activation goes to the engine with the next frame.
  L.activate('l1');
  await L.frame(0.05);
  t.ok(frames[frames.length - 1].activations && frames[frames.length - 1].activations[0].key === 'l1', 'the activation is sent');

  // One frame in flight: while the engine works, the time adds up for the next.
  let release; hold = new Promise(r => { release = r; });
  const n = frames.length;
  const p = L.frame(0.05);
  L.frame(0.05); L.frame(0.05);
  t.ok(frames.length === n + 1, 'one frame in flight');
  release(); await p; hold = null;
  await L.frame(0);
  t.ok(Math.abs(frames[frames.length - 1].dt - 0.1) < 1e-9, 'the next carries the time between: ' + frames[frames.length - 1].dt);

  // Reset stops it and gives the music back.
  S.reset(false);
  await new Promise(r => setTimeout(r, 5));
  t.ok(!L.running && ctx.WgSound.place.scriptMusic === false && !S.disabled.size, 'Reset stops it all');
};
