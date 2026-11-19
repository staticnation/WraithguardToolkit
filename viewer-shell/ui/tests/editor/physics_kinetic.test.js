/* The Editor's Physics, its kinetic events (76_wg_physics.js; Static, 2026-11-18): a blast at a
   clicked point, a throw the way the camera faces, a scatter of copies placed on Keep, and
   "Struck objects move too" - the objects around going to the engine as `near`, with the
   blacklist, and those that moved kept with the rest. */
'use strict';
const fs = require('fs');
const path = require('path');
const { page, EDITOR } = require('./harness');

module.exports = async (t) => {
  let asked = null, answer = null;
  const p = page({
    modules: [...EDITOR, 'src/76_wg_physics.js'],
    links: { editRecord: 'r', editPending: 'pend', editRefsSet: 'refs', editRefSet: 'ref1', editPlaceMany: 'many' },
    engine: (c, a) => {
      if (c === 'editor_ref_names') return { names: a.keys.map((k, i) => [1, 'Balmora', 'M.esm', 10 + i]), plugins: [] };
      if (c === 'editor_record_tag') return { tag: 'MISC', plugins: ['M.esm'] };
      if (c !== 'phys_drop') return c === 'editor_tags' ? [] : {};
      const b = a.body, tb = p.w.unpackTables(b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength));
      const k = tb.kinetic[0];
      asked = { bodies: tb.bodies.map(([key, model]) => ({ key, model })), statics: tb.statics.map(([model]) => model),
        near: tb.near.map(([key, id, model]) => ({ key, id, model })), blast: k[0], at: [k[1], k[2], k[3]], radius: k[4], strength: k[5],
        vel: [k[6], k[7], k[8]], blacklist: tb.blacklist.map(([kind, v]) => kind + ':' + v) };
      return answer(asked);
    },
    answer: (url, b) => url === 'pend' ? [] : url === 'refs' ? { changed: b.changes.length } : url === 'many' ? { placed: b.items.map((x, i) => ({ uid: 'u' + i })) } : {},
  });
  const { w, d } = p;
  const src = fs.readFileSync(path.join(__dirname, '..', '..', 'src', '18_cellpreview.js'), 'utf8');
  w.eval(src.match(/^function refMatrix[\s\S]*?^}/m)[0]);
  w.localStorage.clear();
  const E = w.WgEditor, P = w.WgPhysics;
  await E.enter();
  await p.wait(5);

  const hit = (key, id, model, pos) => ({ refKey: key, id, model, wpos: [pos[0] + 4096, pos[1], pos[2]], rot: [0, 0, 0], scale: 1, m: [1, 0, 0, pos[0], 0, 1, 0, pos[1], 0, 0, 1, pos[2]] });
  const crate = hit('k1', 'crate_01', 'm\\crate.nif', [0, 0, 20]);
  const bucket = hit('k2', 'bucket_01', 'm\\bucket.nif', [100, 0, 10]);
  const house = hit('k3', 'ex_house', 'x\\ex_house.nif', [400, 0, 0]);
  const far = hit('k4', 'rock', 'r\\rock.nif', [9000, 0, 0]);
  w.App.R.pickables = [crate, bucket, house, far];
  w.App._scene = { origin: [4096, 0], centre: { kind: 'ext', x: 0, y: 0 }, cells: [{ kind: 'ext', gx: 0, gy: 0, heights: null }] };
  w.App.R.cv = d.createElement('canvas'); d.body.appendChild(w.App.R.cv);
  let ghosts = [];
  w.App.R.setStaticHighlight = (l) => { ghosts = l; };
  const r1 = { cell: 'Balmora', origin: 'M.esm', refr: 1 };
  E._group = []; E._sel = { hit: crate, ref: r1 };
  E.selected = () => E._sel;
  E.placePoint = (x, y) => ({ local: [50, 0, 0], world: [4146, 0, 0] });
  E.currentCell = () => ({ key: '(0, 0)', label: '(0, 0)' });

  P.toggle();
  const panel = d.getElementById('wgPhysPanel');
  t.ok(P.st.mode === 'drop' && P.st.all === false, 'a drop, only the selection moving, by default');
  t.ok(panel.querySelectorAll('button[data-mode]').length === 4, 'four modes');

  // Throw: the camera faces east; the crate leaves east at the speed, a little upward.
  w.App.R.cameraLook = () => [1, 0, 0];
  panel.querySelector('button[data-mode="throw"]').click();
  answer = q => ({ frames: [[[4096, 0, 20, 0, 0, 0]], [[4596, 0, 10, 0, 0, 0]]], seconds: 2, settled: true, skipped: [], keys: ['k1'] });
  await P.start();
  t.ok(asked.vel[0] > 600 && asked.vel[2] > 100 && Math.abs(asked.vel[1]) < 1e-6 && asked.near.length === 0, 'a throw: east and up, nothing else moving: ' + JSON.stringify(asked.vel));
  t.ok(asked.statics.includes('m\\bucket.nif') && !asked.statics.includes('r\\rock.nif'), 'what is near is ground; what is far is not');
  P.discard(true);

  // Blast, with struck objects moving: Run, then a click on the render window.
  panel.querySelector('button[data-mode="blast"]').click();
  panel.querySelector('#wgPhysAll').click();
  t.ok(P.st.all === true, 'struck objects move too, on');
  answer = q => ({ frames: [q.near.map((n, i) => [4096 + i, 0, 0, 0, 0, 0]), q.near.map((n, i) => [4096 + i + 200, 0, 0, 0, 0, 0])], seconds: 1, settled: true, skipped: [],
    keys: q.near.map(n => n.key), fixed: [] });
  E._sel = null;
  await P.start();
  t.ok(/Click where the blast goes/.test(panel.textContent), 'asks for the point');
  w.App.R.cv.dispatchEvent(new w.MouseEvent('pointerdown', { button: 0, bubbles: true, clientX: 10, clientY: 10 }));
  await p.wait(30);
  t.ok(asked && asked.blast === 1 && asked.at[0] === 4146 && asked.radius === P.st.radius && asked.strength === P.st.strength, 'the blast at the clicked point, in the world: ' + JSON.stringify([asked.at, asked.radius]));
  t.ok(asked.near.map(n => n.key).sort().join() === 'k1,k2,k3' && asked.near.find(n => n.key === 'k3').id === 'ex_house', 'the objects in reach go as near, with their ids (the engine applies the blacklist): ' + JSON.stringify(asked.near));
  t.ok(asked.blacklist.includes('pattern:x/ex_*') && asked.blacklist.includes('tag:DOOR') && asked.blacklist.includes('size:512'), 'the blacklist goes with it: ' + asked.blacklist.join(' '));
  await p.wait(60);
  t.ok(ghosts.length === 3, 'the struck objects play as ghosts');
  await P.commit();
  const step = p.posts.filter(x => x[0] === 'refs');
  t.ok(step.length === 1 && step[0][1].changes.length === 6, 'the three that moved, kept as one step (their references asked for): ' + JSON.stringify(step.map(x => x[1].changes.length)));

  // Scatter: three copies of the crate around the point, placed on Keep as one step.
  panel.querySelector('button[data-mode="scatter"]').click();
  panel.querySelector('#wgPhysAll').click();
  P.st.count = 3;
  E._sel = { hit: crate, ref: r1 };
  answer = q => ({ frames: [q.bodies.map((b, i) => [4100 + i, 0, 150, 0, 0, 0]), q.bodies.map((b, i) => [4100 + i, 0, 5, 0, 0, i])], seconds: 1, settled: true, skipped: [], keys: q.bodies.map(b => b.key) });
  await P.start();
  w.App.R.cv.dispatchEvent(new w.MouseEvent('pointerdown', { button: 0, bubbles: true, clientX: 10, clientY: 10 }));
  await p.wait(30);
  t.ok(asked.bodies.length === 3 && asked.bodies.every(b => b.model === 'm\\crate.nif' && /^scatter:/.test(b.key)) && asked.near.length === 0, 'three copies fall, nothing else moving: ' + JSON.stringify(asked.bodies));
  const before = p.posts.length;
  P.discard();
  t.ok(p.posts.length === before, 'Drop places nothing');
  await P.start();
  w.App.R.cv.dispatchEvent(new w.MouseEvent('pointerdown', { button: 0, bubbles: true, clientX: 10, clientY: 10 }));
  await p.wait(30);
  await P.commit();
  const many = p.posts.filter(x => x[0] === 'many');
  t.ok(many.length === 1 && many[0][1].items.length === 3 && many[0][1].cell === '(0, 0)', 'Keep places the three as one step: ' + JSON.stringify(many.map(x => x[1].cell)));
  const it = many[0] && many[0][1].items[2];
  t.ok(it && it.tag === 'MISC' && it.id === 'crate_01' && JSON.stringify(it.translation) === '[4102,0,5]' && it.rotation[2] === 2, 'each where it came to rest, a copy of its record: ' + JSON.stringify(it));
};
