/* MWScript at scale (73_wg_scripts.js; Static 2026-11-23, the Preview's report: "MWScript 163.8
   ms" a frame over a Balmora with 615 plugins). `Return` is handed back up the blocks rather than
   thrown; `ref->`, a reference by id or by key and the interior holding one are looked up in
   indexes made again when the scene changes, not by walking every object for each ask. */
'use strict';
const vm = require('vm');
const fs = require('fs');
const path = require('path');

module.exports = async (t) => {
  const ctx = { console, Math, JSON, Map, Set, Object, Array, String, Number, Infinity, isFinite, Error,
    performance: { now: () => 0 }, localStorage: { getItem: () => null, setItem() {} }, $: () => null, setTimeout: () => 0,
    App: { _scene: null, R: { cam: { tx: 0, ty: 0, tz: 0, dist: 1, az: 0, el: 0 } } } };
  vm.createContext(ctx);
  vm.runInContext(fs.readFileSync(path.join(__dirname, '..', '..', 'src', '73_wg_scripts.js'), 'utf8').replace(/^const (\w+)=/mg, 'var $1='), ctx);
  const S = ctx.WgScripts;
  S.say = () => {};

  // Return, from deep in ifs and a while: nothing after it runs, and it is not an error.
  S.parsed.set('r', S.parse(`begin r
short a
short b
set a to 1
while ( a < 5 )
  set a to a + 1
  if ( a == 3 )
    if ( 1 == 1 )
      return
    endif
  endif
endwhile
set b to 1
end`));
  const it = { key: 'k1', id: 'thing', script: 'r', locals: null };
  S.runOne(it, it);
  t.ok(it.locals.get('a').v === 3 && it.locals.get('b').v === 0, 'Return stops the script where it stands');
  t.ok(!(S.mwErr.get('r') > 0), 'and is not counted as an error');

  // ref-> by id: the first object of that id, and the index made again when objects come.
  S.inst.set('a1', { key: 'a1', id: 'lamp', locals: null });
  S.inst.set('a2', { key: 'a2', id: 'lamp', locals: null });
  t.ok(S.firstOf('lamp').key === 'a1', 'the first of an id');
  t.ok(S.firstOf('chest') === null, 'none of an id');
  S.inst.set('a3', { key: 'a3', id: 'chest', locals: null });
  t.ok(S.firstOf('chest') && S.firstOf('chest').key === 'a3', 'an object bound later is found');

  // References by key and id, and the interior holding one, follow the scene's cells.
  const cells = [{ kind: 'int', name: 'Balmora, Guild', refs: [{ key: 'r1', id: 'Fargoth', pos: [1, 2, 3] }] }];
  ctx.App._scene = { cells };
  t.ok(S.refById('fargoth').key === 'r1' && S.refByKey('r1').id === 'Fargoth', 'by id and by key');
  t.ok(S.cellOf({ key: 'r1' }) === 'balmora, guild', 'the interior holding it');
  cells.push({ kind: 'ext', gx: 0, gy: 0, refs: [{ key: 'r2', id: 'Mudcrab', pos: [0, 0, 0] }] });
  S._frameN++;
  t.ok(S.refById('mudcrab') && S.refById('mudcrab').key === 'r2', 'a cell loaded into the same scene is indexed on the next frame');
  t.ok(S.refOf({ key: 'r2' }).id === 'Mudcrab' && S.refOf({ key: 'gone', _ref: { id: 'x' } }).id === 'x', 'refOf: the scene\'s, else the one it was bound with');
};
