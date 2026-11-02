/* The key binding page (69_wg_keys.js): a shortcut changed there is the one the Editor
   answers, the old key no longer does, the page draws every action, and Reset puts the
   defaults back. The controller's functions read through its button table. */
'use strict';
const { page, EDITOR } = require('./harness');

module.exports = async (t) => {
  const p = page({
    modules: EDITOR,
    links: { editRecord: 'r', editPending: 'pend' },
    engine: (c) => c === 'editor_tags' ? [['WEAP', 1]] : c === 'editor_records' ? { plugins: [], rows: [] }
      : c === 'cells' ? { grid: [], count: 0, interiors: [], interiorCount: 0 } : {},
    answer: (url) => url === 'pend' ? [] : {},
  });
  const { w, d } = p;
  const E = w.WgEditor, K = w.WgKeys;
  t.ok(K, 'WgKeys is there');
  await E.enter();

  // Snap to grid moved from G to Alt+S.
  K.map().grid = ['Alt+S']; K.save();
  const was = E.gridOn;
  t.ok(!E.keys(new w.KeyboardEvent('keydown', { key: 'g', code: 'KeyG' })), 'G no longer snaps');
  t.ok(E.gridOn === was, 'G left the snap alone');
  t.ok(E.keys(new w.KeyboardEvent('keydown', { key: 's', code: 'KeyS', altKey: true })), 'Alt+S is answered');
  t.ok(E.gridOn === !was, 'Alt+S switched the snap');
  t.ok(K.label('grid') === 'Alt+S', 'the help shows the new key: ' + K.label('grid'));

  // Kept: a fresh read of the storage has it.
  K._map = null;
  t.ok(K.map().grid.join() === 'Alt+S', 'kept in storage');

  // The page lists every action and marks a key two of them share.
  K.map().hide = ['Alt+S']; K.save();
  K.page();
  const body = d.getElementById('wgKeysBody');
  t.ok(body && body.querySelectorAll('[data-a]').length >= K.ACTIONS.length, 'every action has its key on the page');
  t.ok(body.querySelector('button.warn'), 'a shared key is marked');

  // Reset all.
  d.getElementById('wgKeysReset').click();
  t.ok(K.label('grid') === 'G' && K.label('hide') === 'H', 'Reset puts the defaults back');

  // The pad: function 0 moved to the B button (1) is read from button 1.
  K.pad()[0] = 1; K.savePad();
  t.ok(K.padButton(0) === 1, 'the pad function reads its chosen button');
};
