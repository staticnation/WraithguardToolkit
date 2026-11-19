/* A big load order's Scripts start (73_wg_scripts.js `gather`): the engine's answer for the
   scene's objects comes back after the scene was built again (turning the Scripts on draws
   the actors on their own, a new scene). The answer is kept and bound to the newer scene:
   the object's script runs and the actor's own AiWander starts. Dropping it left every
   object and actor of the scene unknown - nothing moved, and the log said nothing. */
'use strict';
const { addTables, sceneTables } = require('./harness');
const vm = require('vm');
const fs = require('fs');
const path = require('path');

const UI = path.join(__dirname, '..', '..');
const src = f => fs.readFileSync(path.join(UI, 'src', f), 'utf8').replace(/^const (\w+)=/mg, 'var $1=');

module.exports = async (t) => {
  const ref = (id, key, pos) => ({ id, key, pos, rot: [0, 0, 0], scale: 1 });
  const cell = () => ({ kind: 'ext', gx: -2, gy: -9, refs: [ref('fargoth', 'f1', [0, 0, 0]), ref('lamp_a', 'l1', [100, 0, 0])] });
  // Answers held until released: the first (the scene's ids) comes back last.
  const held = [];
  const answer = a => ({
    objects: a.ids.includes('lamp_a') ? { lamp_a: 'lampon' } : {},
    scripts: a.ids.includes('lamp_a') ? { lampon: { id: 'LampOn', text: 'Begin LampOn\nshort s\nset s to 1\nEnd' } } : {},
    start: [], globals: [{ name: 'Day', kind: 's', value: 1 }],
    actors: a.ids.includes('fargoth') ? { fargoth: { tag: 'NPC_', items: [], stats: [45, 80, 120], rates: [30, 30, 30, 0], packages: [['wander', '128|5|0|60|20|10|0|0|0|0|0|1']] } } : {},
  });
  const ctx = {
    console, Math, JSON, Map, Set, Promise, Object, Array, String, Number, Error,
    performance: { now: () => Date.now() }, requestAnimationFrame: () => 0, cancelAnimationFrame() {}, setTimeout,
    document: { activeElement: null }, localStorage: { getItem: () => null, setItem() {} },
    $: () => null, escHtml: s => String(s), schedulePreview() {},
    App: { _scene: { origin: [0, 0], cells: [cell()] }, R: { cam: { tx: 0, ty: 0, tz: 116, dist: 0.0001, az: 0, el: 0 }, groundZ: () => 0 } },
    Engine: {
      bytes(c, a) {
        if (c !== 'scene_scripts') return Promise.reject(new Error('no ' + c));
        return new Promise(res => held.push(() => res(sceneTables(answer(a)))));
      },
      async call() { return {}; },
    },
  };
  vm.createContext(ctx);
  addTables(ctx);
  vm.runInContext(src('73_wg_scripts.js'), ctx);
  const S = ctx.WgScripts;
  S.on = true;

  // Scripts on: the scene's ids asked about...
  const first = S.gather();
  // ...and meanwhile the scene is built again (same references, a new scene), gathered again.
  ctx.App._scene = { origin: [0, 0], cells: [cell()] };
  const second = S.gather();
  t.ok(held.length === 2, 'two gatherings asked: ' + held.length);
  // The newer (asking nothing new) answers first; then the first, with every object.
  held[1](); await second;
  held[0](); await first;

  const A = S.actors.get('f1');
  t.ok(A && A.ai && A.ai.kind === 'wander' && A.ai.range === 128, 'Fargoth wanders on his own package: ' + JSON.stringify(A && A.ai && { k: A.ai.kind, r: A.ai.range }));
  t.ok(S.inst.has('l1') && S.inst.get('l1').script === 'lampon', 'the lamp\'s script runs on the newer scene');
  t.ok(S.log.some(l => /AI: 1 actor on their own packages/.test(l.text || l.msg || l[2] || JSON.stringify(l))), 'and the log says so: ' + JSON.stringify(S.log.slice(-3)));
};
