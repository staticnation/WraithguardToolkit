/* The Editor's Physics drop (76_wg_physics.js): Static, 2026-11-03, "optional physics when
   placing objects". The selected objects and what is near them go to the engine
   (`phys_drop`, Rapier in viewer-shell/physcore); the fall it answers plays as ghosts; Keep
   sends where they came to rest as one step (editRefsSet); Drop sends nothing. */
'use strict';
const fs = require('fs');
const path = require('path');
const { page, EDITOR } = require('./harness');

module.exports = async (t) => {
  let asked = null;
  const p = page({
    modules: [...EDITOR, 'src/76_wg_physics.js'],
    links: { editRecord: 'r', editPending: 'pend', editRefsSet: 'refs', editRefSet: 'ref1' },
    engine: (c, a) => {
      if (c !== 'phys_drop') return c === 'editor_tags' ? [] : {};
      asked = JSON.parse(a.request);
      // Two frames: the crate falls 100 units and turns a quarter round about Z.
      return { frames: [[[4096 + 10, 20, 300, 0, 0, 0], [4096 + 500, 0, 0, 0, 0, 0]], [[4096 + 10, 20, 200, 0, 0, Math.PI / 2], [4096 + 500, 0, 0, 0, 0, 0]]], seconds: 1.5, settled: true, skipped: ['k2'] };
    },
    answer: (url, b) => url === 'pend' ? [] : url === 'refs' ? { changed: b.changes.length } : {},
  });
  const { w, d } = p;
  const src = fs.readFileSync(path.join(__dirname, '..', '..', 'src', '18_cellpreview.js'), 'utf8');
  w.eval(src.match(/^function refMatrix[\s\S]*?^}/m)[0]);
  w.localStorage.clear();
  const E = w.WgEditor, P = w.WgPhysics;
  await E.enter();
  await p.wait(5);

  // A crate (the one that falls), a page with no shape, a table near them and a rock far off.
  const hit = (key, model, pos) => ({ refKey: key, model, wpos: [pos[0] + 4096, pos[1], pos[2]], rot: [0, 0, 0], scale: 1, m: [1, 0, 0, pos[0], 0, 1, 0, pos[1], 0, 0, 1, pos[2]] });
  const r1 = { cell: 'Balmora', origin: 'M.esm', refr: 1 }, r2 = { cell: 'Balmora', origin: 'M.esm', refr: 2 };
  const crate = hit('k1', 'm\\crate.nif', [10, 20, 300]), leaf = hit('k2', 'm\\page.nif', [500, 0, 0]);
  const table = hit('k3', 'f\\table.nif', [0, 0, 0]), rock = hit('k4', 'r\\rock.nif', [90000, 0, 0]);
  w.App.R.pickables = [crate, leaf, table, rock];
  // The ground: one square with land (a stroke not yet kept raised a vertex), one with none,
  // one far off.
  const N = 65, hs = new w.Float32Array(N * N).fill(-100); hs[32 * N + 32] = 250.4;
  w.App._scene = { origin: [4096, 0], centre: { kind: 'ext', x: 0, y: 0 },
    cells: [{ kind: 'ext', gx: 0, gy: 0, heights: hs }, { kind: 'ext', gx: 1, gy: 0, heights: null }, { kind: 'ext', gx: 30, gy: 0, heights: hs }] };
  // The crate's drawn triangles (one), for when its mesh has no collision.
  crate.parts = [{ pos: new w.Float32Array([0, 0, 0, 10, 0, 0, 0, 10, 0]), idx: new w.Uint16Array([0, 1, 2]) }];
  let ghosts = [];
  w.App.R.setStaticHighlight = (l) => { ghosts = l; };
  E._group = [{ hit: crate, ref: r1 }, { hit: leaf, ref: r2 }];

  P.toggle();
  const panel = d.getElementById('wgPhysPanel');
  t.ok(panel && !panel.hidden && panel.querySelector('input[type=range][data-k="friction"]'), 'the panel, with its sliders');
  t.ok(panel.querySelector('#wgPhysKeep').disabled, 'Keep waits for a run');
  panel.querySelector('#wgPhysOnly').click();
  t.ok(P.st.dropOnly && panel.querySelector('#wgPhysOnly').classList.contains('pri'), 'Drop only, on');

  await P.start();
  t.ok(asked && asked.bodies.length === 2 && asked.bodies[0].key === 'k1' && asked.bodies[0].model === 'm\\crate.nif', 'the bodies go to the engine: ' + JSON.stringify(asked && asked.bodies[0]));
  t.ok(asked.statics.length === 1 && asked.statics[0].model === 'f\\table.nif', 'what is near them is the ground, not what is far off: ' + JSON.stringify(asked.statics.map(s => s.model)));
  t.ok(JSON.stringify(asked.bodies[0].tris) === '[0,0,0,10,0,0,0,10,0]' && asked.bodies[1].tris.length === 0, 'the drawn triangles go too: ' + JSON.stringify(asked.bodies[0].tris));
  t.ok(asked.land.length === 2 && asked.land[0].h[32 * N + 32] === 250 && asked.land[1].h === null, 'the ground as the render window has it, near them: ' + JSON.stringify(asked.land.map(l => [l.gx, l.h && l.h[32 * N + 32]])));
  t.ok(asked.settings.hollow === true, 'hollow shapes by default');
  t.ok(asked.exterior === true && asked.origin[0] === 4096 && asked.settings.dropOnly === true && Math.abs(asked.settings.gravity - 9.81 * 70.3) < 1e-6, 'outside, with the land; the settings: ' + JSON.stringify(asked.settings));
  await p.wait(60);
  t.ok(ghosts.length === 2 && Math.abs(ghosts[0].m[11] - 200) < 1e-6, 'the fall plays as ghosts, in the render window\'s frame: ' + (ghosts[0] && ghosts[0].m[11]));
  t.ok(/At rest after 1\.5 s; 1 without a collision shape stayed put/.test(panel.textContent), 'what happened: ' + panel.querySelector('#wgPhysSay').textContent);

  // Keep: the crate only (the page did not move) - one object, so the ordinary move
  // (editRefSet), with its Ctrl+Z.
  await P.commit();
  const sent = p.posts.filter(x => x[0] === 'ref1').map(x => x[1]);
  t.ok(sent.length === 2 && !p.posts.some(x => x[0] === 'refs') && E._undo.length === 1, 'one move, one undo: ' + JSON.stringify(sent));
  const tr = sent.find(c => c.path === 'translation'), rot = sent.find(c => c.path === 'rotation');
  t.ok(JSON.stringify(tr.value) === '[4106,20,200]' && Math.abs(rot.value[2] - Math.PI / 2) < 1e-9, 'where it came to rest, in the world: ' + JSON.stringify([tr.value, rot.value]));
  t.ok(!P.run && /1 moved/.test(panel.textContent), 'kept');

  // Run and Drop: nothing sent.
  await P.start();
  P.discard();
  t.ok(p.posts.filter(x => x[0] === 'ref1').length === 2 && !P.run && ghosts === null, 'Drop sends nothing and clears the ghosts');

  // Both moving: one step for both (editRefsSet).
  const both = [[[4106, 20, 300, 0, 0, 0], [4596, 0, 0, 0, 0, 0]], [[4106, 20, 100, 0, 0, 0], [4596, 0, -5, 0, 0, 0]]];
  P.run = { bodies: E._group.map(g => ({ hit: g.hit, ref: g.ref, scale: 1, off: [4096, 0, 0] })), frames: both };
  await P.commit();
  const step = p.posts.filter(x => x[0] === 'refs');
  t.ok(step.length === 1 && step[0][1].changes.length === 4, 'two moved, one step: ' + JSON.stringify(step.map(x => x[1].changes.length)));

  // Nothing selected.
  E._group = []; E._sel = null;
  await P.start();
  t.ok(/Select an object/.test(panel.textContent), 'asks for a selection');
};
