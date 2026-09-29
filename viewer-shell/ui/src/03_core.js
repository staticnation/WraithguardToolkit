<script>
'use strict';
/* =====================================================================================
   Gardenfell — Morrowind Groundcover Generator config studio
   Single-file tool: ini editing + NIF/DDS preview + ini generation.
   ===================================================================================== */

const $  = (s,r)=> (r||document).querySelector(s);
const $$ = (s,r)=> Array.from((r||document).querySelectorAll(s));
const clamp=(v,a,b)=> v<a?a:(v>b?b:v);
/* =====================================================================================
   The engine.

   Gardenfell is a Rust engine with a window on it. Everything that decides where a
   blade goes, what a cell contains, what a mesh looks like or what a config means
   happens there; this page renders the answers and collects the questions.

   There is no fallback. There used to be — a second implementation of the scatter, the
   NIF reader, the plugin writer and the config format, kept so the page could run in a
   browser with no backend — and keeping the two in step is what most of this project's
   bugs were. `Engine.has()` is false only before the shell has finished starting, or in
   a page opened by hand, and the honest answer then is to say so rather than to guess.
   ===================================================================================== */
const Engine={
  has(){ return !!(window.__TAURI__ && window.__TAURI__.core); },

  /** A command returning JSON. Throws with the engine's own message on failure. */
  async call(cmd,args){
    if(!this.has()) throw new Error('the engine is not running');
    const r=await window.__TAURI__.core.invoke(cmd,args||{});
    if(typeof r!=='string') return r;
    try{ return JSON.parse(r); }
    catch(e){ return r; }          // commands that hand back a file's text
  },

  /** A command returning raw bytes — meshes and textures, which never become JSON. */
  async bytes(cmd,args){
    if(!this.has()) throw new Error('the engine is not running');
    const b=await window.__TAURI__.core.invoke(cmd,args||{});
    const ab = b instanceof ArrayBuffer ? b : new Uint8Array(b).buffer;
    // Round 16b: what crossed the boundary, for the report's load timings.
    this.stats.calls++; this.stats.bytes+=ab.byteLength;
    return ab;
  },
  /** Binary calls made and bytes received since start-up. */
  stats:{calls:0, bytes:0},

  /** Asks the OS for a path. Returns null when the person backs out. */
  async pick(kind,opts){
    const d=window.__TAURI__&&window.__TAURI__.dialog;
    if(!d) return null;
    return kind==='save' ? d.save(opts||{}) : d.open(opts||{});
  },

  /** Writes text to a path the person chooses. Returns the path, or null if they
      backed out.

      **Every export goes through here.** They used to build a Blob and click an
      invented `<a download>`, which in a packaged app means the file lands in the
      downloads folder under a name nobody picked and with no dialogue at all — the
      browser's way of saving, left behind from when this ran in a browser. Exporting is
      the moment you decide where something goes, so it asks. */
  async saveAs(defaultPath, text, filters){
    const path=await this.pick('save',{defaultPath, filters:filters||[]});
    if(!path) return null;
    await this.call('write_text',{path, text});
    return path;
  },
};

/** Morrowind's exterior cell, in game units. Everything spatial is measured in
    these: a landscape record covers one, and the coverage maps divide it into 16. */
const CELL=8192;


/** The delete mark, drawn rather than typed.
 *
 *  It was the character `×`, centred with flexbox — and it kept coming out sitting low in
 *  its square on Robin's machine while looking perfectly centred here. That is not a bug
 *  in the centring: a flex box centres the *line box*, and where a glyph's ink sits inside
 *  its line box is a fact about the font. Measured — Segoe UI and this sandbox's fallback
 *  font disagree about `×` by a couple of pixels at 17px, and no amount of `line-height`
 *  fixes a difference that lives in the font file.
 *
 *  Two lines in a 20-unit box instead. Centred by construction, on every machine, and a
 *  symbol rather than a letter — which is what it was always meant to read as.
 *  `currentColor`, so it still takes the red from the stylesheet and lights on hover. */
const DEL_MARK='<svg class="delsvg" viewBox="0 0 20 20" width="20" height="20" '+
  'aria-hidden="true" focusable="false">'+
  '<path d="M6 6 L14 14 M14 6 L6 14" fill="none" stroke="currentColor" '+
  'stroke-width="2" stroke-linecap="round"/></svg>';

/* The two marks on a layer's row, for the same reason and by the same means as the cross
 * above. Robin: "The small glyphs in the Layer boxes (the arrows showing if it's replacing
 * or adding, and the paint on or off on the ground) are both a bit off center the same way
 * as the cross was. Can we make them into svg files too?"
 *
 * `⇥`, `⇉`, `◉` and `○` are all arrow-and-geometry characters, and where their ink sits in
 * the line box is a fact about whichever font the machine fell back to — which is a
 * different fact for each of the four, so the two buttons went off centre by *different*
 * amounts on the same row. Drawn in a 20-unit box they are centred by construction.
 *
 * The pairs are deliberately the same drawing in two states rather than two drawings, so a
 * row does not change shape or weight as either is toggled:
 *   replace  an arrow running into a wall it cannot pass
 *   add      the same arrow with a second one beside it, both running on
 *   shown    a filled eye
 *   hidden   the same eye, hollow, with a stroke through it
 */
const svgMark=(cls,body)=>'<svg class="'+cls+'" viewBox="0 0 20 20" width="20" height="20" '+
  'aria-hidden="true" focusable="false">'+body+'</svg>';
const STROKE='fill="none" stroke="currentColor" stroke-width="1.8" '+
  'stroke-linecap="round" stroke-linejoin="round"';
/** Replacing: the arrow stops at the wall — what was there does not go past it. */
const OVER_MARK=svgMark('glyphsvg',
  '<path d="M4 10 H12 M9 7 L12 10 L9 13 M15.5 5 V15" '+STROKE+'/>');







/** Bytes, for a log line. */
const mb = b => T('unit.mb',{n:(b/1048576).toFixed(1)});
/** Shortens a string to `max` characters by keeping the start and the end, with an
 *  ellipsis in the middle. Robin's rule for strings the interface has no room for:
 *  a long cell name's two ends identify it — "Vivec, St. Olms Canal…orks Underworks"
 *  says more than "Vivec, St. Olms Cana…" ever could. The full string belongs in a
 *  tooltip beside every use of this. */
function midTrim(s,max){
  s=String(s==null?'':s); max=max|0;
  if(max<5 || s.length<=max) return s;
  const head=Math.ceil((max-1)/2), tail=(max-1)-head;
  return s.slice(0,head)+'\u2026'+s.slice(s.length-tail);
}
/** Runs `fn` over every item, `n` of them in flight at once, results in order.
 *
 *  Loading a cell used to await one mesh at a time and, inside each, one texture at a
 *  time — a hundred round trips end to end for a busy cell, each one the page sitting
 *  idle while the engine answered. Since §43 and §46 the engine answers off the main
 *  thread and in parallel, so the page can ask for the next one before the last has come
 *  back, and the wait becomes about the slowest few rather than the sum of all of them.
 *
 *  Bounded rather than `Promise.all` over everything: a cell with four hundred distinct
 *  meshes would otherwise open four hundred requests at once, and the engine's pool is
 *  not that wide. Errors travel with their item — one mesh that will not load must not
 *  take the other ninety-nine down. */
async function inFlight(items, n, fn){
  const list=Array.from(items);
  const out=new Array(list.length);
  let next=0;
  const worker=async()=>{
    for(;;){
      const i=next++;
      if(i>=list.length) return;
      try{ out[i]=await fn(list[i], i); }
      catch(e){ out[i]=null; }
    }
  };
  await Promise.all(Array.from({length:Math.max(1,Math.min(n,list.length))}, worker));
  return out;
}

const nf = n => (n>=1e6? (n/1e6).toFixed(2)+'M' : n>=1000? (n/1000).toFixed(1)+'k' : String(n));
/** A duration, in the unit a person would say it in.
 *
 *  Two decimals under a second because that is where the difference between 40 ms and
 *  400 ms is worth seeing, one above it, and minutes past sixty seconds — "184.2 s" is a
 *  number you have to do arithmetic on before it means anything. */
const secs = ms => {
  const s = Math.max(0, +ms || 0) / 1000;
  if (s < 1)  return T('unit.s',{n:s.toFixed(2)});
  if (s < 60) return T('unit.s',{n:s.toFixed(1)});
  const m = Math.floor(s/60);
  return T('unit.min_s',{m, s:String(Math.round(s-m*60)).padStart(2,'0')});
};

/** A message in the corner.
 *
 *  Round 9 item 4. `opts.go` makes the message a door: it is drawn with a cue line, and
 *  clicking it runs `go` and takes the message away. `opts.cue` is what that line says —
 *  it should name the place, not the act ("Show it in the mesh editor"), because the act
 *  is the sentence above it.
 *
 *  A message you can act on also waits while the pointer is on it. Reading a sentence
 *  and reaching for it takes longer than reading a word, and a door that closes as you
 *  arrive at it is worse than no door.
 */
function toast(msg,kind,ms,opts){
  opts=opts||{};
  const el=document.createElement('div');
  el.className='toast'+(kind?' '+kind:'');
  const body=document.createElement('div');
  body.className='tmsg'; body.textContent=msg;
  el.appendChild(body);
  let timer=0, dead=false;
  const drop=()=>{
    if(dead) return;
    dead=true;
    el.style.transition='opacity .25s'; el.style.opacity='0';
    setTimeout(()=>el.remove(),260);
  };
  const wait=ms||5200;
  const arm=n=>{ clearTimeout(timer); timer=setTimeout(drop, n); };
  if(typeof opts.go==='function'){
    el.classList.add('go');
    const cue=document.createElement('div');
    cue.className='tgo';
    cue.textContent='\u2192  '+(opts.cue||T('toast.show_it'));
    el.appendChild(cue);
    if(opts.title) el.title=opts.title;
    el.onclick=()=>{ clearTimeout(timer); drop(); try{ opts.go(); }catch(e){ } };
    // Held open while the pointer is on it, and given the full stay again on leaving.
    el.onpointerenter=()=>clearTimeout(timer);
    el.onpointerleave=()=>arm(Math.min(wait,2600));
  }
  $('#toasts').appendChild(el);
  arm(wait);
  return el;
}


function escHtml(s){ return String(s==null?'':s)
  .replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;'); }



/* =====================================================================================
   Virtual file system — an ordered overlay of roots, highest priority first.
   A plain setup has one root (Data Files). An MO2 setup has:
       overwrite/  >  each enabled mod by priority  >  the game's real Data Files
   Reads walk the stack top-down and take the first hit, which is exactly what
   MO2's usvfs presents to a process launched through it.
   ===================================================================================== */
/* =====================================================================================
   Reading the install.

   The engine has already resolved the overlay — which mod's copy of a file wins, in
   what order — while loading the world. This is a thin reader over that: give it a
   virtual path, get the bytes the game would get.

   It used to be a File System Access crawler, walking folders the browser had been
   granted. That is how the page found assets when there was no backend, and it could
   not see an MO2 stack the way the engine does, so the two disagreed about which mod
   won. Now there is one overlay and this asks it.
   ===================================================================================== */
const VFS = {
  /** True once an install is open. Nothing can be read before that. */
  connected:false,
  ok(){ return this.connected; },

  norm(p){
    return String(p||'').trim().replace(/^["']|["']$/g,'')
      .replace(/\\/g,'/').replace(/\/+/g,'/').replace(/^\.\//,'').replace(/^\//,'');
  },

  /** Whether a mesh path is a grass mesh - a file under `Meshes\grass\`. Round 18aw,
   *  Robin: "The only object that should have the grass wind shader effect are grass
   *  meshes from the grass folder." The wind leans a batch only when this says so: a
   *  rock or a shrine placed from the load order stands still, and so does a flora mesh
   *  from another folder used as a grass card. */
  isGrass(p){
    const n=this.norm(p).toLowerCase();
    return n.startsWith('grass/') || n.startsWith('meshes/grass/');
  },

  /** The bytes at a virtual path, or null. A miss is normal — a config naming a mesh
      the install does not have is the case this tool exists to report. */
  async read(relPath){
    const p=this.norm(relPath);
    if(!p) return null;
    try{ return await Engine.bytes('read_asset',{path:p}); }
    catch(e){ return null; }
  },

  async file(relPath){
    const buf=await this.read(relPath);
    return buf? new Blob([buf]) : null;
  },
};

/* ---------- Morrowind asset path resolution ----------
   Meshes: ini gives e.g.  Grass\azbc03.nif   -> Data Files/Meshes/Grass/azbc03.nif
   Textures: NIF gives e.g. textures\azbc01.tga OR azbc01.tga, and Morrowind famously
             ships .dds files under .tga names.

   The engine answers that one now. Walking the family here — one `read_asset` per
   candidate extension — could only ever be extension-major: it asked the whole load
   order for `.tga` before it asked anyone for `.dds`, so vanilla's loose copy at the
   bottom of the order beat every retexture above it, which is why Robin's landscape
   looked vanilla under a stack of them. `Vfs::resolve_family` asks root by root
   instead. The list below is only the fallback for a host with no engine behind it. */

/* VTEX index 0 is the engine's built-in land texture rather than an absence of one.
   It exists in no plugin, so the tool gives it a reserved id and a known filename. */
/** Morrowind's editor markers, which the game never draws.
 *
 *  `marker_*` in any path segment, not just the file name, so a mod that keeps them in
 *  a `marker_stuff` folder is caught too. Character for character the rule the engine
 *  uses (`world::is_marker_mesh`), because the two disagreeing is exactly the bug this
 *  fixes: the engine has always known these are not obstacles, while the viewport drew
 *  whatever had geometry in it. Most markers have none, so Door and Travel were invisible
 *  by luck rather than by rule — and Temple and Prison, which do have a little model,
 *  stood in the scene as objects the game will never show.
 */
function isMarkerMesh(model){
  return String(model||'').split(/[\\/]/)
    .some(seg=>seg.length>=7 && seg.slice(0,7).toLowerCase()==='marker_');
}

const DEFAULT_LTEX='_land_default';
const DEFAULT_LTEX_FILE='_land_default.dds';
/* If that file is not loose in the overlay (vanilla keeps it inside Morrowind.bsa,
   which this tool does not read) the layer still has to look like ground rather than
   a missing-texture checker. */
const DEFAULT_LTEX_COLOR=[0.42,0.37,0.30];


/* `resolveMesh` was here, and is gone — round 18bf (§I25).
 *
 * A walk over `Meshes/` and the bare path, one `read_asset` per candidate, called by
 * nothing: `mesh_data` does the resolve and the parse in the engine, the way
 * `resolveTexture` was folded into `texture_data` below it. And for the same reason —
 * §37: walking candidates from the page can only ever get the answer wrong, because the
 * overlay's priority is the engine's to know. */

/* `resolveTexture` used to live here: a walk over `Textures/` and the extension family,
   one `read_asset` per candidate. `texture_data` does the whole thing in the engine now —
   the prefix, the family, the read and the decode — in one call. See §37 for why walking
   candidates from here could only ever get the answer wrong.  */
