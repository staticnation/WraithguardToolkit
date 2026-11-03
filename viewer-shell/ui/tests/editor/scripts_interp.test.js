/* The scene's MWScript, run (73_wg_scripts.js): a lamp disabled by the hour, a static
   swapped for another, a music script, a light that turns, a door moved with SetPos, the
   weather changed, a global set, a script started, and what the cell then holds. */
'use strict';
const vm = require('vm');
const fs = require('fs');
const path = require('path');

const UI = path.join(__dirname, '..', '..');
const src = f => fs.readFileSync(path.join(UI, 'src', f), 'utf8')
  .replace(/^const (\w+)=/mg, 'var $1=');

const SCRIPTS = {
  lampbyhour: `Begin LampByHour
; lamps out by day
if ( GameHour > 6 )
  if ( GameHour < 20 )
    if ( GetDisabled == 0 )
      Disable
    endif
    return
  endif
endif
if ( GetDisabled )
  Enable
endif
End`,
  swapstatic: `Begin SwapStatic
short done
if ( done == 1 )
  return
endif
if ( TownBurned == 1 )
  "ex_house_good"->Disable
  ex_house_burnt->Enable
  set done to 1
endif
End`,
  musicat: `Begin MusicAt
short played
if ( played == 0 )
  if ( GetDistance Player < 1000 )
    StreamMusic "Special/MW_Triumph.mp3"
    set played to 1
  endif
endif
End`,
  spinlight: `begin spinlight
float t
set t to t + GetSecondsPassed
Rotate, Z, 90
if ( t > 2.0 )
  SetPos z, 500
  ChangeWeather "Bitter Coast Region" 4
endif
end`,
  startup: `Begin Startup
short n
short m
set n to n + 1
if ( n == 3 )
  set TownBurned to 1
  StartScript Later
endif
set m to 0
while ( m < 10 )
  set m to m + 1
endwhile
End`,
  later: `Begin Later
Player->PlaceAtMe "ex_rock_01" 2 256 0
StopScript Later
End`,
};

module.exports = async (t) => {
  const hour = { value: '12', dispatchEvent() {} };
  const weather = { selectedIndex: 0, options: [0, 1, 2, 3, 4, 5], changed: 0, dispatchEvent() { this.changed++; } };
  const music = [];
  let rebuilds = 0;
  const ref = (id, key, pos) => ({ id, key, pos, rot: [0, 0, 0], scale: 1 });
  const cell = {
    kind: 'ext', gx: 0, gy: 0, name: 'Seyda Neen',
    refs: [ref('lamp_a', 'k1', [100, 100, 0]), ref('ex_house_good', 'k2', [500, 500, 0]), ref('ex_house_burnt', 'k3', [500, 500, 0]),
      ref('music_marker', 'k4', [0, 0, 0]), ref('spin_light', 'k5', [300, 300, 100]), ref('swapper', 'k6', [0, 0, 0])],
  };
  const ctx = {
    console, Math, JSON, Map, Set, Promise, Object, Array, String, Number, Error, Event: function (n) { this.type = n; },
    performance: { now: () => Date.now() + 1e6 }, requestAnimationFrame: () => 0, cancelAnimationFrame() {},
    document: { activeElement: null },
    $: s => s === '#p_hour' ? hour : s === '#p_weather' ? weather : null, escHtml: s => String(s),
    schedulePreview: () => { rebuilds++; },
    App: { _scene: { origin: [0, 0], cells: [cell] }, R: { cam: { tx: 0, ty: 0, tz: 116, dist: 0.0001, az: 0, el: 0 } } },
    WgSound: { place: { on: true }, audio: () => ({}), placeGraph() {}, musicStop() {}, buffer: async st => { music.push(st.path); throw new Error('no audio here'); } },
    Engine: {
      async call(c, a) {
        if (c === 'scene_scripts') {
          const objects = { lamp_a: 'lampbyhour', ex_house_burnt: '', music_marker: 'musicat', spin_light: 'spinlight', swapper: 'swapstatic' };
          const o = {}; for (const id of a.ids) if (objects[id]) o[id] = objects[id];
          const scripts = {}; for (const k of [...Object.values(o), 'startup']) scripts[k] = { id: k, text: SCRIPTS[k] };
          return { objects: o, scripts, start: ['Startup'], globals: [{ name: 'TownBurned', kind: 's', value: 0 }] };
        }
        if (c === 'script_texts') { const o = {}; for (const n of a.names) if (SCRIPTS[n]) o[n] = { id: n, text: SCRIPTS[n] }; return o; }
        return {};
      },
    },
  };
  vm.createContext(ctx);
  vm.runInContext(src('73_wg_scripts.js'), ctx);
  const S = ctx.WgScripts;
  t.ok(S, 'WgScripts is there');

  // The parser: a function's arguments without parentheses, `ref->`, `ref.var`, comments.
  const p = S.parse('begin x\nshort a\nset a to GetDistance Player + 2 * 3 ; a comment\nset b.c to -a\nfoo->SetPos x, (a + 1)\nend');
  t.ok(p.name === 'x' && p.locals.length === 1 && p.body.length === 3, 'parsed: ' + JSON.stringify(p).slice(0, 200));
  t.ok(p.body[0].expr.k === 'bin' && p.body[0].expr.a.k === 'fn' && p.body[0].expr.a.args[0].v === 'Player', 'GetDistance takes Player, then + 2 * 3');
  t.ok(p.body[1].target.ref === 'b' && p.body[1].target.name === 'c', 'set b.c');
  t.ok(p.body[2].ref === 'foo' && p.body[2].fn === 'setpos' && p.body[2].args.length === 2, 'foo->SetPos x, (a + 1)');

  S.on = true;
  await S.gather();
  await new Promise(r => setTimeout(r, 5));
  t.ok(S.inst.size === 4 && S.globalRun.has('startup'), 'four scripted objects and the start script: ' + [...S.inst.keys()] + ' / ' + [...S.globalRun.keys()]);

  // Noon: the lamp goes out.
  S.frame(0.05);
  t.ok(S.disabled.has('k1'), 'the lamp is disabled by day');
  const shown = S.overlay(cell);
  t.ok(!shown.refs.some(r => r.key === 'k1') && shown.refs.length === 5, 'and is out of the cell the scene draws');
  // Night: back on.
  hour.value = '22';
  S.frame(0.05);
  t.ok(!S.disabled.has('k1'), 'the lamp is enabled at night');

  // The start script: on its third frame it sets the global and starts another.
  S.frame(0.05);
  t.ok(S.state.globals.get('townburned').v === 1, 'Startup set TownBurned');
  await new Promise(r => setTimeout(r, 5));
  S.frame(0.05);
  t.ok(S.disabled.has('k2') && !S.disabled.has('k3'), 'the good house swapped for the burnt one');
  t.ok(S.placed.length === 2 && S.placed[0].id === 'ex_rock_01' && Math.abs(S.placed[0].pos[1] - 256) < 1e-6, 'Later placed two rocks 256 in front: ' + JSON.stringify(S.placed[0] && S.placed[0].pos));
  t.ok(!S.globalRun.has('later'), 'and stopped itself');
  t.ok(S.overlay(cell).refs.filter(r => r.id === 'ex_rock_01').length === 2, 'the rocks are in the cell');

  // The music marker is at the player: the track streams once.
  await new Promise(r => setTimeout(r, 5));
  t.ok(music.length === 1 && music[0] === 'Music/Special/MW_Triumph.mp3', 'StreamMusic: ' + music);

  // The light turns 90 degrees a second; past two seconds it is moved up and the weather changes.
  const before = S.rotOf(S.inst.get('k5'))[2];
  for (let i = 0; i < 40; i++) S.frame(0.05);
  const turned = (S.rotOf(S.inst.get('k5'))[2] - before) * 180 / Math.PI;
  t.ok(Math.abs(turned - 180) < 1, 'the light turned 180 degrees in two seconds: ' + turned);
  S.frame(0.05);
  t.ok(S.posOf(S.inst.get('k5'))[2] === 500, 'SetPos z 500');
  t.ok(weather.selectedIndex === 4 && weather.changed === 1, 'ChangeWeather to rain');
  const lit = S.overlay(cell).refs.find(r => r.key === 'k5');
  t.ok(lit.pos[2] === 500 && cell.refs[4].pos[2] === 100, 'the scene shows it moved; the load order\'s reference is untouched');
  t.ok(rebuilds > 0, 'the scene is built again');

  // A loop that never ends is stopped, and said.
  S.learn('forever', { id: 'forever', text: 'begin forever\nshort n\nwhile ( 1 )\nset n to n + 1\nendwhile\nend' });
  S.globalRun.set('forever', { script: 'forever', locals: null });
  S.frame(0.05);
  t.ok(S.log.some(l => l.script === 'forever' && l.kind === 'error'), 'the endless loop is stopped and logged');

  // Reset: everything the scripts did is undone.
  S.reset(false);
  t.ok(!S.disabled.size && !S.moved.size && !S.placed.length && S.overlay(cell) === cell, 'Reset puts the scene back');
};
