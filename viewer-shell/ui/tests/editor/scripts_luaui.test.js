/* The windows OpenMW Lua scripts make (75_wg_luaui.js, `ui.create`): a tree as the engine
   sends it (templates resolved) drawn over the viewport - placed, sized and anchored by
   OpenMW's props, text with its size and color, a flat-color image, a Flex row - and a
   click on a node with mouseClick going back with the next frame; destroy takes it away. */
'use strict';
const { JSDOM } = require('jsdom');
const fs = require('fs');
const path = require('path');

module.exports = async (t) => {
  const dom = new JSDOM('<!doctype html><body><div id="vpwrap"></div></body>', { runScripts: 'outside-only', pretendToBeVisual: true });
  const w = dom.window, d = w.document;
  w.eval('var $=s=>document.querySelector(s); var Engine={async bytes(){ throw new Error("no textures"); }}; var readTextureBytes=()=>null; var rgbaToDataURL=()=>"";');
  w.eval(fs.readFileSync(path.join(__dirname, '..', '..', 'src', '75_wg_luaui.js'), 'utf8').replace(/^const WgLuaUI=/m, 'var WgLuaUI='));
  const U = w.WgLuaUI;

  // A box (a black flat image) in the middle of the screen, a slot inside, a title and a
  // row of two buttons.
  const tree = {
    type: 'Image', path: 'r', events: [], props: { resource: { path: 'white' }, color: { r: 0, g: 0, b: 0, a: 1 }, relativePosition: { x: 0.5, y: 0.5 }, anchor: { x: 0.5, y: 0.5 }, size: { x: 300, y: 120 } },
    content: [{
      type: 'Widget', path: 'r.t1', name: 'slot', events: [], props: { position: { x: 4, y: 4 } }, content: [
        { type: 'Text', path: 'r.s1', name: 'title', events: [], props: { text: 'Hello', textSize: 18, textColor: { r: 1, g: 0, b: 0, a: 1 } }, content: [] },
        { type: 'Flex', path: 'r.s2', events: [], props: { horizontal: true, position: { x: 0, y: 30 } }, content: [
          { type: 'Text', path: 'r.s2.c1', name: 'ok', events: ['mouseClick'], props: { text: 'OK' }, content: [] },
          { type: 'Text', path: 'r.s2.c2', name: 'no', events: [], props: { text: 'Cancel' }, content: [] },
        ] },
      ],
    }],
  };
  U.draw('e1', 'Windows', tree);
  const box = d.querySelector('[data-lua-el="e1"]');
  t.ok(box && box.parentNode.id === 'wgLuaUI', 'drawn over the viewport');
  t.ok(box.style.left === 'calc(50% + 0px)' && box.style.width === '300px' && /translate\(-50%,\s*-50%\)/.test(box.style.transform), 'placed in the middle, sized and anchored: ' + [box.style.left, box.style.width, box.style.transform]);
  t.ok(/rgba?\(0,\s*0,\s*0(,\s*1)?\)/.test(box.style.background) && box.style.zIndex === '2', 'a black box on the Windows layer: ' + box.style.background);
  const title = [...box.querySelectorAll('div')].find(x => x.textContent === 'Hello');
  t.ok(title && title.style.fontSize === '18px' && /rgba?\(255,\s*0,\s*0/.test(title.style.color), 'the title, at its size and color');
  const row = [...box.querySelectorAll('div')].find(x => x.style.display === 'flex');
  t.ok(row && row.style.flexDirection === 'row' && row.children.length === 2 && row.children[0].style.position === 'relative', 'a Flex row of two');

  // A click on OK goes back for the next frame; a click on Cancel (no event) does not.
  const ok = row.children[0], no = row.children[1];
  t.ok(ok.style.pointerEvents === 'auto' && no.style.pointerEvents !== 'auto', 'only what answers clicks takes them');
  ok.dispatchEvent(new w.MouseEvent('click', { bubbles: true, clientX: 10, clientY: 5 }));
  const ev = U.take();
  t.ok(ev.length === 1 && ev[0].id === 'e1' && ev[0].path === 'r.s2.c1' && ev[0].event === 'mouseClick', 'the click, for the engine: ' + JSON.stringify(ev));
  t.ok(U.take().length === 0, 'once');

  // A move (at most ten a second), the pointer in and out, keys once clicked; a tint.
  const tree2 = { type: 'Widget', path: 'r', events: ['mouseMove', 'focusGain', 'focusLoss', 'keyPress'], props: { size: { x: 50, y: 50 } }, content: [
    { type: 'Image', path: 'r.t1', events: [], props: { resource: { path: 'icons/x.dds' }, color: { r: 1, g: 0, b: 0, a: 1 } }, content: [] }] };
  w.eval('Engine.bytes=async()=>new ArrayBuffer(8); readTextureBytes=()=>({kind:"rgba", levels:[{w:1,h:1,data:new Uint8Array(4)}]}); rgbaToDataURL=()=>"data:x";');
  U.imgs.clear();
  U.draw('e2', 'HUD', tree2);
  const box2 = d.querySelector('[data-lua-el="e2"]');
  box2.dispatchEvent(new w.Event('pointerenter'));
  box2.dispatchEvent(new w.MouseEvent('pointermove', { clientX: 5, clientY: 6 }));
  box2.dispatchEvent(new w.MouseEvent('pointermove', { clientX: 7, clientY: 6 }));
  box2.dispatchEvent(new w.KeyboardEvent('keydown', { code: 'KeyE', key: 'e' }));
  box2.dispatchEvent(new w.Event('pointerleave'));
  const ev2 = U.take().map(e => e.event + (e.event === 'keyPress' ? ':' + e.x + e.text : ''));
  t.ok(ev2.join() === 'focusGain,mouseMove,keyPress:8e,focusLoss', 'in, one move, a key (E is 8), out: ' + ev2.join());
  await new Promise(r => setTimeout(r, 10));
  const img = box2.firstChild;
  t.ok(/multiply/.test(img.style.backgroundBlendMode) && /rgba?\(255,\s*0,\s*0/.test(img.style.backgroundColor), 'a red tint over the texture: ' + img.style.backgroundBlendMode + ' ' + img.style.backgroundColor);
  U.remove('e2');

  // Drawn again (an update) and destroyed.
  tree.content[0].content[0].props.text = 'Bye';
  U.draw('e1', 'Windows', tree);
  t.ok(d.querySelectorAll('[data-lua-el="e1"]').length === 1 && /Bye/.test(d.querySelector('[data-lua-el="e1"]').textContent), 'an update replaces it');
  U.remove('e1');
  t.ok(!d.querySelector('[data-lua-el="e1"]'), 'destroy takes it away');
};
