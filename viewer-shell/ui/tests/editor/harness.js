/* The Editor's page tests: one shared stand-in for the engine and for Wraithguard.

   `boot.js` runs the whole page against the headless engine (wg-view-serve). These run
   the Editor's own modules (`src/5*_wg_*.js` and the few they lean on) in jsdom, alone,
   with the engine's commands and Wraithguard's links answered by the test - so a
   window's behaviour (a drag, a search, a menu) is checked without a load order.

   page({modules, html, engine, links, answer}) -> {w, d, posts, toasts, run}
     modules  the src/ files to load, in order (paths relative to viewer-shell/ui)
     html     the body to start with
     engine   (cmd, args) -> answer, for Engine.call / Engine.bytes
     links    link name -> url, the extra file's `links`
     answer   (url, body) -> answer (an object is sent back as JSON), for wg_post
   `posts` holds every [url, body] the page sent Wraithguard; `toasts` every toast. */
'use strict';
const fs = require('fs');
const path = require('path');
// WG_JSDOM: jsdom from elsewhere (a copy outside a slow network or VM-shared folder).
const { JSDOM } = require(process.env.WG_JSDOM || 'jsdom');

const UI = path.resolve(__dirname, '..', '..');

/* The page / Wraithguard contract (contract.json; tests/test_editor_contract.py keeps
   it to Wraithguard's handlers). Every request a page sends to a link must carry the
   keys the link needs and no key it does not read; every answer a test stands in with
   (when it is more than `{}`) must have the keys Wraithguard's answer has. What breaks
   it is kept in `violations`, which run.js fails the test on. */
const CONTRACT = JSON.parse(fs.readFileSync(path.join(__dirname, 'contract.json'), 'utf8')).links;
const violations = [];
function checkRequest(link, body) {
  const c = CONTRACT[link]; if (!c) return;
  const keys = Object.keys(body || {});
  for (const k of c.req) if (!keys.includes(k)) violations.push(link + ': the request has no ' + k + ' (' + JSON.stringify(body) + ')');
  for (const k of keys) if (!c.req.includes(k) && !c.opt.includes(k)) violations.push(link + ': the request sends ' + k + ', which Wraithguard does not read');
}
function checkAnswer(link, ans) {
  const c = CONTRACT[link]; if (!c || !Array.isArray(c.ans)) return;
  if (!ans || typeof ans !== 'object' || Array.isArray(ans) || !Object.keys(ans).length) return;
  const miss = c.ans.filter(k => !(k in ans));
  if (miss.length) violations.push(link + ': a stand-in answer without ' + miss.join(', ') + ' (Wraithguard answers ' + c.ans.join(', ') + ')');
}

/** The source of a module, its top-level `const X=` / `class X` made globals of the
 *  window (as one concatenated page would have them). */
function source(rel) {
  return fs.readFileSync(path.join(UI, rel), 'utf8')
    .replace(/^<script>/, '')
    .replace(/^const (\w+)=/mg, 'var $1=')
    .replace(/^class (\w+)/mg, 'var $1=class $1');
}

function page(opts = {}) {
  const dom = new JSDOM('<!doctype html><body>' + (opts.html || '<div id="main"><div id="center"><div id="vpwrap"></div></div></div>') + '</body>',
    { runScripts: 'outside-only', pretendToBeVisual: true, url: 'http://wg.test/' });
  const w = dom.window, d = w.document;
  w.TextDecoder = TextDecoder;
  w.TextEncoder = TextEncoder;
  const posts = [], toasts = [];
  w.__posts = posts; w.__toasts = toasts;
  w.__engine = opts.engine || (() => ({}));
  const answer = opts.answer || (() => '{}');
  const linkOf = {};
  for (const [name, url] of Object.entries(opts.links || {})) linkOf[url] = name;
  w.__answer = async (url, body) => {
    const link = linkOf[url];
    if (link) checkRequest(link, body);
    const r = await answer(url, body);
    if (link) checkAnswer(link, typeof r === 'string' ? (() => { try { return JSON.parse(r); } catch (_) { return null; } })() : r);
    return r;
  };
  w.eval(`
    var $=(s,r)=>(r||document).querySelector(s); var $$=(s,r)=>[...(r||document).querySelectorAll(s)];
    function escHtml(s){ return String(s==null? '' : s).replace(/[&<>"']/g,c=>'&#'+c.charCodeAt(0)+';'); }
    function toast(m){ __toasts.push(String(m)); }
    function T(k){ return k; }
    var CELL=8192; function Renderer(){} function refMatrix(){ return [1,0,0,0, 0,1,0,0, 0,0,1,0]; }
    function schedulePreview(){}
    var CellData={models:new Map(), actors:new Map()};
    var App={R:{opts:{}, pickables:[], setOverlay(k,L){ (this.ov=this.ov||{})[k]=L; }, setStaticHighlight(){}, setSelection(){}}, _scene:null};
    var Engine={
      has(){ return true; },
      async call(c,a){
        if(c==='wg_post'){
          const b=JSON.parse(a.body||'{}'); __posts.push([a.url,b]);
          const r=await __answer(a.url,b);
          return typeof r==='string'? r : JSON.stringify(r==null? {} : r);
        }
        return __engine(c,a||{});
      },
      async bytes(c,a){ return __engine(c,a||{}); },
      async pick(){ return null; },
    };`);
  w.__WG_VIEW__ = { extra: { links: opts.links || {} } };
  for (const m of opts.modules || []) w.eval(source(m));
  return { w, d, posts, toasts, wait: ms => new Promise(r => setTimeout(r, ms || 0)) };
}

/** A pointer-ish event for jsdom (it has no PointerEvent). */
function ev(w, type, x, y, more) {
  return new w.MouseEvent(type, Object.assign({ clientX: x, clientY: y, button: 0, bubbles: true, cancelable: true }, more || {}));
}

/** The editor's modules and what they lean on, in page order. */
/** The Editor's parts, in ORDER's order. */
const EDITOR_PARTS = ['objects', 'cells', 'record', 'ref', 'move', 'qmenu', 'place', 'dialogue', 'script', 'lua', 'uses', 'build', 'keys'];
const EDITOR = ['src/24_ori.js', 'src/50_wg_editor.js', ...EDITOR_PARTS.map(p => 'src/50_wg_editor_' + p + '.js'), 'src/51_wg_editor_ui.js', 'src/53_wg_record_forms.js', 'src/56_wg_workflow.js'];

module.exports = { page, ev, EDITOR, source, CONTRACT, violations };
