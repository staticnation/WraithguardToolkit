/* The flyout menus by keyboard: the page's generic flyout (WgUI.menu) and the Q menu
   (WgEditor.showMenu) are one behaviour - menu items with roles that take the focus,
   arrows / Home / End to move, Right and Left for a submenu, Enter or Space to do an
   item, Esc to close and give the focus back. */
'use strict';
const { page, EDITOR } = require('./harness');

module.exports = async (t) => {
  const p = page({
    modules: EDITOR,
    links: { editPending: 'pend' },
    engine: (c) => c === 'editor_tags' ? [['WEAP', 1]] : {},
    answer: (url) => url === 'pend' ? [] : {},
  });
  const { w, d } = p;
  const UI = w.WgUI, E = w.WgEditor;
  const key = (k, el) => {
    const e = new w.KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true });
    (el || d.activeElement || d.body).dispatchEvent(e);
    return e;
  };
  const label = () => {
    const a = d.activeElement, lb = a && a.querySelector && a.querySelector(':scope > .lb');
    return ((lb || a || {}).textContent || '').trim();
  };

  // The generic flyout, opened from a button that has the focus.
  const btn = d.createElement('button'); btn.textContent = 'opener'; d.body.appendChild(btn); btn.focus();
  const did = [];
  UI.menu({ x: 10, y: 10 }, [
    { head: 'Things' },
    { label: 'First', act: () => did.push('first') },
    { label: 'Greyed', disabled: true, act: () => did.push('greyed') },
    { label: 'Second', checked: true, act: () => did.push('second') },
    { sep: true },
    { label: 'More', sub: [{ label: 'Inner one', act: () => did.push('inner1') }, { label: 'Inner two', act: () => did.push('inner2') }] },
    { label: 'Last', act: () => did.push('last') },
  ]);
  const menu = d.querySelector('.edFlyRoot');
  t.ok(menu && menu.getAttribute('role') === 'menu', 'the flyout is a menu');
  t.ok(label() === 'First', 'the first item has the focus: ' + label());
  const items = [...menu.querySelectorAll(':scope > .it')];
  t.ok(items.every(i => i.getAttribute('role') && i.tabIndex === -1), 'every item is a focusable menu item');
  t.ok(items[1].getAttribute('aria-disabled') === 'true', 'the greyed item says so');
  t.ok(items[2].getAttribute('role') === 'menuitemcheckbox' && items[2].getAttribute('aria-checked') === 'true', 'a checked item is a checkbox item');
  t.ok(items[3].getAttribute('aria-haspopup') === 'menu' && items[3].getAttribute('aria-expanded') === 'false', 'a submenu item says it opens one');

  let e = key('ArrowDown');
  t.ok(e.defaultPrevented, 'the key is the menu\'s, not the page\'s');
  t.ok(label() === 'Second', 'Down skips the greyed item: ' + label());
  key('ArrowDown');
  t.ok(label() === 'More', 'Down reaches the submenu item: ' + label());
  key('ArrowDown'); key('ArrowDown');
  t.ok(label() === 'First', 'Down wraps round: ' + label());
  key('ArrowUp');
  t.ok(label() === 'Last', 'Up wraps the other way: ' + label());
  key('Home'); t.ok(label() === 'First', 'Home');
  key('End'); t.ok(label() === 'Last', 'End');

  // A submenu: Right opens it into its first item; Left (or Esc) comes back and closes only it.
  key('ArrowUp');
  const more = d.activeElement;
  t.ok(more.classList.contains('sub'), 'on the submenu item');
  key('ArrowRight');
  t.ok(more.classList.contains('open') && more.getAttribute('aria-expanded') === 'true', 'Right opens the submenu');
  t.ok(label() === 'Inner one', 'the submenu\'s first item has the focus: ' + label());
  key('ArrowDown'); t.ok(label() === 'Inner two', 'Down inside the submenu');
  key('Escape');
  t.ok(!more.classList.contains('open') && d.activeElement === more, 'Esc leaves the submenu only');
  t.ok(d.querySelector('.edFlyRoot'), 'the menu is still open');
  key('Enter');
  t.ok(more.classList.contains('open') && label() === 'Inner one', 'Enter on a submenu item opens it');
  key('ArrowLeft');
  t.ok(!more.classList.contains('open') && d.activeElement === more, 'Left leaves the submenu');

  // Enter does the item, closes the menu, and the focus goes back to the opener.
  key('Home'); key('Enter');
  t.ok(did.join() === 'first', 'Enter did the item: ' + did.join());
  t.ok(!d.querySelector('.edFlyRoot'), 'and closed the menu');
  t.ok(d.activeElement === btn, 'the focus went back to the opener');

  // Space does an item too; Esc closes with nothing done and gives the focus back.
  UI.menu({ x: 10, y: 10 }, [{ label: 'One', act: () => did.push('one') }, { label: 'Two', act: () => did.push('two') }]);
  key('ArrowDown'); key(' ');
  t.ok(did[did.length - 1] === 'two', 'Space did the second: ' + did.join());
  btn.focus();
  UI.menu({ x: 10, y: 10 }, [{ label: 'One', act: () => did.push('nope') }]);
  key('Escape');
  t.ok(!d.querySelector('.edFlyRoot') && d.activeElement === btn && !did.includes('nope'), 'Esc closes it and does nothing');

  // A disabled item cannot be done from the keyboard (it cannot even be reached).
  UI.menu({ x: 10, y: 10 }, [{ label: 'Off', disabled: true, act: () => did.push('off') }, { label: 'On', act: () => did.push('on') }]);
  t.ok(label() === 'On', 'the focus starts on the first item that can be done: ' + label());
  key('Enter');
  t.ok(did[did.length - 1] === 'on' && !did.includes('off'), 'only the enabled item ran');

  // The Q menu (tuple items) behaves the same.
  const q = d.createElement('div'); q.id = 'edQ'; q.className = 'ori'; d.body.appendChild(q);
  btn.focus();
  const acts = [];
  E.showMenu(q, 20, 20, [
    ['ref', 'Edit reference', 'F3'],
    ['-'],
    ['drop', 'Drop to ground', 'F'],
    ['>', 'Layers', [['layer:A', 'Move to layer: A'], ['newlayer', 'Move to a new layer']]],
    ['del', 'Delete reference'],
  ], a => acts.push(a));
  t.ok(q.getAttribute('role') === 'menu', 'the Q menu is a menu');
  t.ok(label() === 'Edit reference', 'the Q menu\'s first item has the focus: ' + label());
  key('ArrowDown'); t.ok(label() === 'Drop to ground', 'Down in the Q menu');
  key('ArrowDown'); key('ArrowRight');
  t.ok(label() === 'Move to layer: A', 'Right opens the Q menu\'s submenu: ' + label());
  key('ArrowLeft');
  key('ArrowDown');
  t.ok(label() === 'Delete reference', 'Down leaves the submenu item: ' + label());
  key('Enter');
  t.ok(acts.join() === 'del' && q.hidden, 'Enter did the Q menu item and closed it: ' + acts.join());
  t.ok(d.activeElement === btn, 'the focus went back after the Q menu');

  btn.focus();
  E.showMenu(q, 20, 20, [['ref', 'Edit reference'], ['del', 'Delete reference']], a => acts.push(a));
  key('Escape');
  t.ok(q.hidden && d.activeElement === btn && !acts.includes('ref'), 'Esc closes the Q menu with nothing done');

  // Typing in a field inside a flyout is not the menu's (the Keys sheet's filter).
  UI.keysSheet(null);
  await p.wait(5);
  const f = d.querySelector('.edKeys input');
  t.ok(d.activeElement === f, 'the Keys sheet\'s filter has the focus');
  e = key('ArrowDown', f);
  t.ok(!e.defaultPrevented, 'arrow keys in the filter are left to it');
  key('Escape', f);
  t.ok(!d.querySelector('.edKeys'), 'Esc still closes the sheet');
};
