/* A live actor's batches swapped in place (06_gl.js `swapLive`, Static 2026-11-23: "the
   animation/fps is still very low and everything is choppy"): the same statics array, the new
   batches where the old ones were, the sun's still-shadow cache kept (its revision not bumped)
   and only its movers listed again; and a live actor posed by where it stands (`_skinClock`),
   every frame near the camera. */
'use strict';
const fs = require('fs');
const path = require('path');

const gl = fs.readFileSync(path.join(__dirname, '..', '..', 'src', '06_gl.js'), 'utf8');
// One method of the renderer's class, as a function of `this`.
const method = name => {
  const i = gl.indexOf('\n  ' + name + '(');
  if (i < 0) throw new Error('no ' + name);
  const open = gl.indexOf('{', i), args = gl.slice(gl.indexOf('(', i) + 1, gl.indexOf(')', i));
  let depth = 0, j = open;
  for (; j < gl.length; j++) { if (gl[j] === '{') depth++; else if (gl[j] === '}' && --depth === 0) break; }
  return new Function(args, gl.slice(open + 1, j));
};

module.exports = async (t) => {
  const swapLive = method('swapLive'), skinClock = method('_skinClock'), poseInterval = method('poseInterval');
  const a = { id: 'a' }, b = { id: 'b' }, oldW = { id: 'old' }, c = { id: 'c' }, newW = { id: 'new', skin: {} };
  const statics = [a, oldW, b, c];
  const groups = { src: statics };
  const R = { statics, _groups: groups, _legacyStatics: null, _casterRev: 7, _shMov: { list: [] }, dropped: [], _dropVao(x) { this.dropped.push(x.id); } };
  swapLive.call(R, [oldW], [newW]);
  t.ok(R.statics === statics && statics.map(x => x.id).join() === 'a,new,b,c', 'the same array, the new batch in the old one\'s place: ' + statics.map(x => x.id).join());
  t.ok(R._groups.src === R.statics, 'the material groups still drawn: built from this same list');
  t.ok(R._casterRev === 7 && R._shMov === null && R.dirty === true && R.dropped.join() === 'old', 'the still-shadow cache kept, the movers listed again, the old batch freed');
  const two = [{ id: 'n1' }, { id: 'n2' }];
  swapLive.call(R, [newW], two);
  t.ok(statics.map(x => x.id).join() === 'a,n1,b,c,n2', 'more batches than before: the rest at the end ' + statics.map(x => x.id).join());
  swapLive.call(R, two, [{ id: 'm' }]);
  t.ok(statics.map(x => x.id).join() === 'a,m,b,c', 'fewer: the extra ones taken out ' + statics.map(x => x.id).join());

  // A live actor near the camera is posed every frame, wherever its run's box has grown to.
  const mats = new Float32Array([1, 0, 0, 100, 0, 1, 0, 200, 0, 0, 1, 0]);
  const live = { live: true, mats, groups: [{ b: [-50000, -50000, 0, 50000, 50000, 100] }] };
  const C = { _uvSecs: 12.3456, _eyeNow: [120, 220, 100], poseInterval };
  t.ok(skinClock.call(C, live) === 12.3456, 'near: every frame');
  mats[3] = 7000;
  t.ok(Math.abs(skinClock.call(C, live) - 12.3) < 1e-9, 'farther: ten a second, by where it stands ' + skinClock.call(C, live));
};
