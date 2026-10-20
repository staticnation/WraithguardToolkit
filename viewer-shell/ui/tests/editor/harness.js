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
     editor   the launch's `extra.editor` ({plugins, journal}): the viewer's own Editor
              opened (`editor_open`, answered by `engine`) and asked its links
   `posts` holds every [url, body] the page sent Wraithguard; `native` every [link, body]
   it asked the viewer's own Editor (`editor_link`); `toasts` every toast. */
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
  if (miss.length && Array.isArray(c.alt) && c.alt.every(k => k in ans)) return;  // its other shape
  if (miss.length)violations.push(link + ': a stand-in answer without ' + miss.join(', ') + ' (Wraithguard answers ' + c.ans.join(', ') + ')');
}

/** The source of a module, its top-level `const X=` / `class X` made globals of the
 *  window (as one concatenated page would have them). */
function source(rel) {
  return fs.readFileSync(path.join(UI, rel), 'utf8')
    .replace(/^<script>/, '')
    .replace(/^const (\w+)=/mg, 'var $1=')
    .replace(/^class (\w+)/mg, 'var $1=class $1');
}

const TAINT = '<b data-taint="1">!</b>';
const tainted = [];
/** Where a tainted element landed: the nearest ancestor with an id or class, and its text. */
function where(el) {
  let p = el.parentElement;
  while (p && !p.id && !p.className) p = p.parentElement;
  const tag = p ? (p.id ? '#' + p.id : '.' + String(p.className).split(/\s+/)[0]) : '?';
  return tag + ': ' + String((el.parentElement || el).textContent || '').replace(/\s+/g, ' ').slice(0, 80);
}

function page(opts = {}) {
  const dom = new JSDOM('<!doctype html><body>' + (opts.html || '<div id="main"><div id="center"><div id="vpwrap"></div></div></div>') + '</body>',
    { runScripts: 'outside-only', pretendToBeVisual: true, url: 'http://wg.test/' });
  const w = dom.window, d = w.document;
  w.TextDecoder = TextDecoder;
  w.TextEncoder = TextEncoder;
  const posts = [], toasts = [];
  w.__posts = posts; w.__toasts = toasts;
  /* WG_TAINT=1: every string the engine or Wraithguard answers ends in markup, as a mod's
     record name could. Plugin text is untrusted: if any of it reaches innerHTML unescaped,
     an element with data-taint appears in the page, and run.js lists where. */
  const engine0 = opts.engine || (() => ({}));
  const answer0 = opts.answer || (() => '{}');
  const tainting = !!process.env.WG_TAINT;
  const taint = v => typeof v === 'string' ? v + TAINT
    : Array.isArray(v) ? v.map(taint)
    : v && typeof v === 'object' ? Object.fromEntries(Object.entries(v).map(([k, x]) => [k, taint(x)])) : v;
  w.__engine = tainting ? async (c, a) => taint(await engine0(c, a)) : engine0;
  const answer = tainting ? async (u, b) => { const r = await answer0(u, b);
    return typeof r === 'string' ? (() => { try { return JSON.stringify(taint(JSON.parse(r))); } catch (_) { return r; } })() : taint(r); } : answer0;
  if (tainting) new w.MutationObserver(ms => { for (const m of ms) for (const n of m.addedNodes) {
    const hits = n.nodeType === 1 ? (n.matches('[data-taint]') ? [n] : [...n.querySelectorAll('[data-taint]')]) : [];
    for (const h of hits) tainted.push(where(h)); } }).observe(d, { childList: true, subtree: true });
  const linkOf = {};
  for (const [name, url] of Object.entries(opts.links || {})) linkOf[url] = name;
  const native = [];
  w.__native = native;
  w.__checkRequest = checkRequest;
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
        if(c==='editor_link'){
          // A link the viewer's own Editor answers: the same contract as Wraithguard's.
          const b=JSON.parse(a.body||'{}'); __native.push([a.name,b]); __checkRequest(a.name,b);
          const r=await __engine(c,a||{});
          return typeof r==='string'? r : JSON.stringify(r==null? {} : r);
        }
        return __engine(c,a||{});
      },
      async bytes(c,a){
        // The Object Window's rows as the engine packs them (pack_rows in commands.rs),
        // from the test's own editor_records answer.
        if(c==='editor_rows'){
          const r=await __engine('editor_records',a||{}), te=new TextEncoder(), out=[];
          const u=v=>{ const b=new Uint8Array(4); new DataView(b.buffer).setUint32(0,v,true); out.push(b); };
          const s=t=>{ const b=te.encode(String(t)); u(b.length); out.push(b); };
          out.push(te.encode('WGOR')); u(1);
          const pl=(r&&r.plugins)||[], rows=(r&&r.rows)||[];
          u(pl.length); pl.forEach(s);
          u(rows.length);
          for(const x of rows){ s(x[0]); s(x[1]); s(x[2]); u(x[3].length); x[3].forEach(u); u(x[4]|0); }
          const all=new Uint8Array(out.reduce((n,b)=>n+b.length,0)); let at=0;
          for(const b of out){ all.set(b,at); at+=b.length; }
          return all.buffer;
        }
        /* And the other lists as WGTB (Tables in commands.rs), from the test's own answer to
           the JSON command of the same list - so a test's stand-in answers once, under the
           name it always had, and the page reads it the way it reads the engine's bytes. An
           answer that is not the list makes the bytes fail too, as the engine's would. */
        const lists={
          editor_tags_bytes:['editor_tags', r=>Array.isArray(r) && [['tags','su',r.map(x=>[x[0], x[1]|0])]]],
          cells_bytes:['cells', r=>r && Array.isArray(r.grid) && [
            ['grid','iissuu',r.grid.map(x=>[x[0]|0, x[1]|0, x[2]||'', x[3]||'', x[4]|0, x[5]|0])],
            ['interiors','suu',(r.interiors||[]).map(x=>[x[0]||'', x[1]|0, x[2]|0])]]],
          editor_cell_refs_bytes:['editor_cell_refs', r=>r && Array.isArray(r.rows) && [
            ['meta','ss',[[r.name||'', r.refCell||'']]],
            ['plugins','s',(r.plugins||[]).map(p=>[p])],
            ['rows','sssdddss',r.rows.map(x=>{ const p=x[3]||[0,0,0]; return [x[0]||'', x[1]||'', x[2]||'', +p[0]||0, +p[1]||0, +p[2]||0, x[4]||'', x[5]||'']; })]]],
        };
        if(lists[c]){
          const tables=lists[c][1](await __engine(lists[c][0],a||{}));
          if(!tables) throw new Error('not '+lists[c][0]);
          const te=new TextEncoder(), out=[];
          const put=(n,set)=>{ const b=new Uint8Array(n); set(new DataView(b.buffer)); out.push(b); };
          const u=v=>put(4,dv=>dv.setUint32(0,v,true)), i=v=>put(4,dv=>dv.setInt32(0,v,true)), dd=v=>put(8,dv=>dv.setFloat64(0,v,true));
          const s=t=>{ const b=te.encode(String(t)); u(b.length); out.push(b); };
          out.push(te.encode('WGTB')); u(1); u(tables.length);
          for(const [name, cols, rows] of tables){
            s(name); s(cols); u(rows.length);
            for(const row of rows) [...cols].forEach((k,j)=>({s, u, i, d:dd})[k](row[j]));
          }
          const all=new Uint8Array(out.reduce((n,b)=>n+b.length,0)); let at=0;
          for(const b of out){ all.set(b,at); at+=b.length; }
          return all.buffer;
        }
        return __engine(c,a||{});
      },
      async pick(){ return null; },
    };`);
  w.__WG_VIEW__ = { extra: Object.assign({ links: opts.links || {} }, opts.editor ? { editor: opts.editor } : {}) };
  for (const m of opts.modules || []) w.eval(source(m));
  return { w, d, posts, toasts, native, wait: ms => new Promise(r => setTimeout(r, ms || 0)) };
}

/** A pointer-ish event for jsdom (it has no PointerEvent). */
function ev(w, type, x, y, more) {
  return new w.MouseEvent(type, Object.assign({ clientX: x, clientY: y, button: 0, bubbles: true, cancelable: true }, more || {}));
}

/** The editor's modules and what they lean on, in page order. */
/** The Editor's parts, in ORDER's order. */
const EDITOR_PARTS = ['objects', 'cells', 'record', 'ref', 'move', 'qmenu', 'place', 'dialogue', 'script', 'lua', 'uses', 'build', 'keys'];
const EDITOR = ['src/38_wg_cellkey.js', 'src/24_ori.js', 'src/50_wg_editor.js', ...EDITOR_PARTS.map(p => 'src/50_wg_editor_' + p + '.js'), 'src/51_wg_editor_ui.js', 'src/53_wg_record_forms.js', 'src/56_wg_workflow.js'];

module.exports = { tainted, page, ev, EDITOR, source, CONTRACT, violations };
