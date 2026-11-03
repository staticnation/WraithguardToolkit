/* The scene's scripts and its actors (73_wg_scripts.js): what a container carries (the load
   order's, then AddItem), a guard's AiTravel through the scene, a fight to the death with
   OnDeath, a MessageBox's buttons answered, GetSoundPlaying, a line of sight blocked by a
   wall, and PlaceAtMe in a room landing in that room only. */
'use strict';
const vm = require('vm');
const fs = require('fs');
const path = require('path');

const UI = path.join(__dirname, '..', '..');
const src = f => fs.readFileSync(path.join(UI, 'src', f), 'utf8').replace(/^const (\w+)=/mg, 'var $1=');

const SCRIPTS = {
  chestcount: `Begin ChestCount
short told
if ( told == 0 )
  if ( GetItemCount "gold_001" == 25 )
    AddItem "gold_001" 5
    set told to 1
  endif
endif
End`,
  guardwalk: `Begin GuardWalk
short state
if ( state == 0 )
  AiTravel 1000 0 0
  set state to 1
elseif ( state == 1 )
  if ( GetAIPackageDone )
    StartCombat "rat_a"
    set state to 2
  endif
endif
End`,
  ratdies: `Begin RatDies
if ( OnDeath )
  set RatsDead to RatsDead + 1
  PlaceAtMe "ingred_rat_meat_01" 1 0 0
endif
End`,
  asker: `Begin Asker
short asked
short button
if ( asked == 0 )
  MessageBox "Light the brazier? Gold: %g" GoldLeft "Yes" "No"
  set asked to 1
elseif ( asked == 1 )
  set button to GetButtonPressed
  if ( button == 0 )
    Enable
    PlaySound3D "Fire"
    set asked to 2
  elseif ( button == 1 )
    set asked to 3
  endif
endif
End`,
  roomdrop: `Begin RoomDrop
short done
if ( done == 0 )
  PlaceAtMe "misc_com_bucket_01" 1 64 0
  set done to 1
endif
End`,
};

module.exports = async (t) => {
  const ref = (id, key, pos) => ({ id, key, pos, rot: [0, 0, 0], scale: 1 });
  const out = {
    kind: 'ext', gx: 0, gy: 0, name: 'Pelagiad',
    refs: [ref('chest_a', 'c1', [0, 0, 0]), ref('guard_a', 'g1', [0, 0, 0]), ref('rat_a', 'r1', [1100, 0, 0]),
      ref('brazier_a', 'b1', [200, 200, 0]), ref('wall_a', 'w1', [0, 500, 0])],
  };
  const room = { kind: 'int', name: 'Pelagiad, Halfway Tavern', refs: [ref('dropper', 'd1', [10, 10, 0])] };
  const room2 = { kind: 'int', name: 'Pelagiad, Ahnia\'s House', refs: [] };
  const plays = [];
  const ctx = {
    console, Math, JSON, Map, Set, Promise, Object, Array, String, Number, Error, Event: function (n) { this.type = n; },
    performance: { now: () => Date.now() }, requestAnimationFrame: () => 0, cancelAnimationFrame() {}, setTimeout,
    document: { activeElement: null },
    $: () => null, escHtml: s => String(s), schedulePreview() {},
    App: {
      _scene: { origin: [0, 0], cells: [out, room, room2] },
      R: {
        cam: { tx: 0, ty: 0, tz: 116, dist: 0.0001, az: 0, el: 0 },
        groundZ: () => 0,
        // A wall across y = 500: anything looking past it is blocked.
        pickRay: (eye, d) => {
          if (Math.abs(d[1]) < 1e-6) return null;
          const tt = (500 - eye[1]) / d[1];
          return tt > 0 ? { obj: { refKey: 'w1' }, t: tt } : null;
        },
      },
    },
    WgSound: {
      place: { on: true, fx: {} }, placeGraph() {},
      audio: () => ({ createBufferSource: () => ({ connect: x => x, start() {} }), createGain: () => ({ gain: {}, connect: x => x }) }),
      playable: async d => { plays.push(d.id || d.file); return { duration: 2 }; },
    },
    Engine: {
      async call(c, a) {
        if (c === 'scene_scripts') {
          const objects = { chest_a: 'chestcount', guard_a: 'guardwalk', rat_a: 'ratdies', brazier_a: 'asker', dropper: 'roomdrop' };
          const o = {}; for (const id of a.ids) if (objects[id]) o[id] = objects[id];
          const scripts = {}; for (const k of Object.values(o)) scripts[k] = { id: k, text: SCRIPTS[k] };
          return {
            objects: o, scripts, start: [],
            globals: [{ name: 'RatsDead', kind: 's', value: 0 }, { name: 'GoldLeft', kind: 'f', value: 12.5 }],
            actors: {
              chest_a: { tag: 'CONT', items: [['gold_001', 25]], stats: null },
              guard_a: { tag: 'NPC_', items: [], stats: [200, 0, 300] },
              rat_a: { tag: 'CREA', items: [], stats: [20, 0, 50] },
            },
          };
        }
        if (c === 'sound_records') return [{ id: a.ids[0], file: a.ids[0] + '.wav', gain: 1 }];
        return {};
      },
    },
  };
  vm.createContext(ctx);
  vm.runInContext(src('73_wg_scripts.js'), ctx);
  const S = ctx.WgScripts;
  S.on = true;
  await S.gather();
  const tick = (n, dt = 0.05) => { for (let i = 0; i < n; i++) S.frame(dt); };

  // The chest: 25 gold from the load order, then 5 more.
  tick(1);
  t.ok(S.actors.get('c1').items.get('gold_001') === 30, 'the chest held 25 and was given 5: ' + S.actors.get('c1').items.get('gold_001'));

  // The guard walks 1000 units at walk pace (150 a second): about 6 seconds, facing +x.
  tick(60, 0.05);
  const gx = S.posOf(S.inst.get('g1'))[0];
  t.ok(gx > 400 && gx < 500, 'three seconds in, the guard is partway: ' + Math.round(gx));
  t.ok(Math.abs(S.rotOf(S.inst.get('g1'))[2] - Math.PI / 2) < 1e-6, 'and faces where it goes');
  tick(80, 0.05);
  const G = S.actors.get('g1');
  t.ok(G.ai.done && G.combat && G.combat.key === 'r1', 'arrived, the guard attacks the rat');

  // The fight: the rat (20 health) falls; its OnDeath counts it and drops meat at it.
  tick(200, 0.05);
  const Rt = S.actors.get('r1');
  t.ok(Rt.dead && Rt.hp === 0, 'the rat is dead');
  t.ok(S.state.globals.get('ratsdead').v === 1, 'OnDeath ran once: ' + S.state.globals.get('ratsdead').v);
  t.ok(S.placed.some(p => p.id === 'ingred_rat_meat_01'), 'and placed the meat');
  t.ok(Math.abs(S.rotOf(S.inst.get('r1'))[0] + Math.PI / 2) < 1e-6, 'the rat lies down');
  t.ok(!G.combat && !G.dead, 'the guard stops fighting, alive');

  // The brazier's question: put to the person; "Yes" lights it and plays the fire.
  S.disabled.add('b1');
  t.ok(S.msg && S.msg.text === 'Light the brazier? Gold: 12.5' && S.msg.buttons.join() === 'Yes,No', 'the MessageBox and its buttons: ' + JSON.stringify(S.msg && { text: S.msg.text, b: S.msg.buttons }));
  tick(3);
  t.ok(S.disabled.has('b1'), 'nothing happens until a button is chosen');
  S.choose(0);
  tick(1);
  t.ok(!S.disabled.has('b1'), 'Yes: the brazier is enabled');
  await new Promise(r => setTimeout(r, 10));
  t.ok(plays.includes('Fire') && S.soundOn(S.inst.get('b1'), 'Fire'), 'the fire plays at it, and GetSoundPlaying says so');

  // Line of sight: the wall at y 500 hides what is beyond it.
  const P = { player: true };
  t.ok(S.los(P, S.inst.get('r1')), 'the rat at y 0 is seen');
  t.ok(!S.los(P, { key: 'x', pos: [0, 900, 0], base: { pos: [0, 900, 0], rot: [0, 0, 0] } }), 'behind the wall is not');

  // PlaceAtMe in the tavern: the bucket is in the tavern, not the other room or outside.
  t.ok(S.overlay(room).refs.some(r => r.id === 'misc_com_bucket_01'), 'the bucket is in the tavern');
  t.ok(!S.overlay(room2).refs.some(r => r.id === 'misc_com_bucket_01') && !S.overlay(out).refs.some(r => r.id === 'misc_com_bucket_01'), 'and nowhere else');
};
