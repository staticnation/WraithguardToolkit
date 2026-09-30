
/* =====================================================================================
   Application state + UI
   ===================================================================================== */
/* The brush size slider is a *position*, and the radius grows exponentially along it.
 *
 * Robin: "Make the Brush size slider change according to an exponential curve, so there
 * is less change in size per step on the lower end of the slider, and more change per
 * step in the higher end." Which is how a brush is actually used: the difference between
 * 64 and 96 units matters when you are picking around a doorway, and the difference
 * between 3800 and 4000 does not.
 *
 * Sixty-four to four thousand and ninety-six, a factor of sixty-four, so the middle of
 * the slider is 512 — where the linear one had its default — and every step is the same
 * *proportion* of the size rather than the same number of units.
 *
 * The profile still stores the radius, not the position (`brush_size`, unchanged): the
 * file is meant to be readable, and a number that means "somewhere along a slider" is
 * only meaningful next to the slider it came from. */
const BrushSize={
  /* 16 units at the bottom, not 64: the brush paints on objects now, and the smallest
     thing worth painting on is smaller than a ground texel. The profile stores the radius
     in units rather than the slider's position, so widening the range moves nobody's
     saved brush. */
  MIN:16, MAX:4096,
  /** Units, from a 0..100 position. */
  radius(pos){
    const t=Math.max(0,Math.min(100,+pos||0))/100;
    return Math.round(this.MIN*Math.pow(this.MAX/this.MIN,t));
  },
  /** And back, for a profile that stores the radius. */
  pos(radius){
    const r=Math.max(this.MIN,Math.min(this.MAX,+radius||this.MIN));
    return Math.round(100*Math.log(r/this.MIN)/Math.log(this.MAX/this.MIN));
  },
};

const App={
  cfg:newConfig(),
  active:0,
  R:null,
  seed:1337,
  meshCache:new Map(),      // lc mesh path -> {parts,err,tris,thumb}
  texGL:new Map(),          // lc tex path  -> {gl,img,err}
  /* A connect, as the asset caches see it: the counter ticks at both ends of one and
     the flag marks its inside. A read that spans or sits inside a connect belongs to
     neither install and is never cached — see `worldToken`. */
  swapEpoch:0,
  swapping:false,
  loadLog:[],
  /* The connect report's own lines - what was read, which archives, how many meshes
     and textures indexed. Kept apart from `loadLog` (round 14): that one is cleared
     whenever the grass rules change, and it was taking the connect lines with it, so
     the report said "everything loaded cleanly" and nothing else. Cleared only when
     the install is switched. */
  connectLog:[],
  pending:0,
  _seq:0,
  lastToml:'',
  mode:'cell',          // the viewer shows real cells only
  cellName:'',
  /* `showStatics` used to live here. Objects are always drawn: a cell without its rocks
     and buildings is not what anybody loads a cell to see, and the switch's other effect
     was to suggest that hiding a thing stopped grass avoiding it. */
  showActors:true,      // Wraithguard: NPCs, creatures and leveled spawn points (#p_actors)
  showCorpses:true,     // actors that spawn dead: they sit on the ground like statics
  avoidExclude:[],      // glob patterns for objects grass may sit inside anyway
  exportScope:'world', exportPrefix:'', exportAuthor:'Gardenfell', exportDesc:'',
  cellSel:null,         // nothing loaded until a cell is chosen
  showAdjacent:0,           // rings of neighbours loaded with the cell: 0..3 (round 15; was a bool)
  texOn:{},             // per-land-texture grass switches in cell mode
  /* Per-rule switches, keyed by `selKey` — the texture and the scope, which is what
     tells one rule from another to a person. Several rules can govern one texture, and
     the texture switch cannot separate them; this can. Viewport state, like `texOn`:
     nothing here reaches the .esp. */
  ruleOff:{},
  /* Whether the ground-texture list is folded away. Viewport state: which parts of the
     window you have open is about the window, not about the grass. */
  selFolded:false,
  /* And the same question for each of the two groups inside it. A config that is mostly
     paint rules is one where the texture rules are in the way, and the other way round;
     folding the whole list answers neither. Keyed rather than two flags so the profile
     writes one thing and a third group later needs no new key. */
  selGroupShut:{},
  /* Round 18q: a grass card's sections start folded, and stay wherever you leave them
     (`fold_card_*` in the profile's viewport block). 18ac added the third. */
  cardShut:{overrides:true, conditions:true, drift:true},
  /* Whether the asset picker shows tiles rather than lines. Shared by the mesh and the
     texture picker: they are the same dialogue looking at two lists, and remembering two
     answers to one question is how they end up disagreeing about which one you asked. */
  browseGrid:false,
  /* Rebuilds queued or running. Zero means the scene on screen is the scene that was
     asked for — see `rebuildPreview`. */
  previewRuns:0,
  /* The ground textures the loaded scene actually carries, newest first by coverage.
     Filled by the cell list; read by the bans section, which offers them. */
  cellTex:[],
  /* True while the engine's copy of the config is the authoritative one and the page is
     about to take it. Nothing may push the page's copy back during that window — see
     `syncConfig`. */
  adopting:false,
  avoidStatics:false,   // skip grass that would sit inside a placed object
  avoidPad:0,
  avoidMinSize:8,       // colliders smaller than this radius are stepped over
  avoidHeight:96,
  hiTex:true,           // hovering a ground texture lights up the ground it covers
  hiSlot:true,          // hovering a grass slot lights up the instances it placed
  hiObj:true,           // pointing at an object, or at a never-avoid pattern, lights it
  /* Painting ground. The mask itself is the engine's — see `paint.rs` — and these two
     are only what the window is doing: whether the right button paints instead of
     picking, and which of the three tools the brush is holding.
     `paintTool` is 'bare' (no grass), 'grow' (grass whatever else says) or 'erase', and
     it is the name the engine reads — one field rather than a flag per colour, because
     exactly one tool can be held and two flags can say otherwise. */
  paintMode:false,
  paintTool:'bare',
  /* Which rule colour the brush is holding, when it is holding one. Zero is none, which
     is also what the engine reads for every tool that is not a rule colour. */
  paintSlot:0,
  /* The paint layers, as the engine last reported them: `[{id,name,visible,swatches}]`,
     and which one the tools are working on. Rule paint lives on layers that sit beside
     each other — where two of them cover the same ground, both rules grow there — so
     there is no order to them and nothing to reorder. */
  /* Whether the cell picker offers interiors. Off to begin with: an install has several
     times more rooms than named exteriors, and a list you have to wade through is a worse
     list. Nothing scatters in one yet — see `GameData.cellList`. */
  showInteriors:false,
  /* A mesh being looked at on its own, or null. Not a preview mode — see
     `rebuildMeshPreview`. */
  inspect:null,
  paintLayers:[],
  paintLayer:0,
  /* The palette, as the engine last reported it: `[{slot, rule, colour, layer}]`. Held for
     drawing the swatches and the layers, and never edited here — adding, recolouring and
     removing all go through the engine, which is where the ground painted with them is. */
  paintPalette:[],
  /* The revision the engine's mask is at. A snapshot in `History` carries this number
     rather than the mask, which is the whole reason three hundred steps of undo can
     include painting at all. */
  paintRev:0,
  /* A stroke has changed the ground and the grass has not caught up yet. It waited for
     the mode to end for one round, back when a rescatter held the thread the window is
     drawn on; it rescatters as you paint now (§43, §46, §47), so this is only ever set
     between a stroke landing and the rebuild it schedules. */
  paintPending:false,
};

/* ---------------- hover highlights ----------------
   Both are viewport state only: nothing is rebuilt and nothing is rescattered, so
   running the pointer down a list stays as cheap as moving it over empty space. */
function hoverTexture(id){
  if(!App.R) return;
  App.R.setTexHighlight(App.hiTex? (id||null) : null);
}
function hoverSlot(selIx,slotIx){
  if(!App.R) return;
  App.R.setGrassHighlight(App.hiSlot && selIx>=0 && slotIx>=0 ? selIx+':'+slotIx : null);
  /* Round 18q: and the cull mask narrows to the card under the pointer, so what it
     refuses that the rule does not is visible where it happens. The rule's own mask comes
     back when the pointer leaves. */
  const sel=App.cfg.selectors[selIx];
  const slot=sel && sel.slots? sel.slots[slotIx] : null;
  syncCullMask(slot||null);
}
/** Hands the renderer what the mask is asking, for the active rule and this card. */
function syncCullMask(slot){
  const R=App.R; if(!R) return;
  const c=cullFor(activeSel(), slot||null);
  const was=JSON.stringify(R.opts.cull||null), now=JSON.stringify(c);
  if(was===now) return;
  R.opts.cull=c;
  R.dirty=true;
  // The field is the engine's, and only worth a round trip when the mask needs one.
  if(typeof refreshCullField==='function')
    Promise.resolve().then(refreshCullField).catch(e=>console.error(e));
}
/** Attaches enter/leave to one element. Leave always clears, whatever the toggle. */
function onHover(el,enter){
  if(!el) return;
  el.addEventListener('pointerenter',enter);
  el.addEventListener('pointerleave',()=>{
    hoverTexture(null); hoverSlot(-1,-1);
    if(App.R && App.R.setStaticHighlight) App.R.setStaticHighlight(null);
  });
}

function activeSel(){ return App.cfg.selectors[App.active]||null; }

/* =====================================================================================
   Editing several rules at once.

   Robin: "I want to be able to select several Rules to bulk edit their settings. When
   selecting more than one rule, the right details panel should clearly indicate if there
   are several values that are different from each other by rendering an em-dash in the
   field, rather than a number. [...] Whenever a setting is changed, no matter if the
   different rules had the same value there before or not, that value is set to the same
   for both the Rules."

   **The panel edits one object, and that object is not a rule.** `Bulk.view()` builds a
   merged rule: every field the selected rules agree on carries their value, every field
   they disagree on carries `MIXED`, and the three input helpers know to draw `MIXED` as
   an em-dash rather than as a number. The panel then edits it exactly as it edits a real
   rule — nothing in `renderRight` has to know how many rules are selected.

   **Nothing is copied until you change it.** `Bulk.base` is what the merged view looked
   like when it was built; `Bulk.mirror()` compares the two and writes across only the
   fields that actually moved. So selecting two rules and dragging an unrelated slider
   does not quietly flatten every difference between them — which is what mirroring the
   whole view would do, and is the one way this feature could destroy work.

   **One undo step, without doing anything about undo.** History is snapshots on a
   debounce (§27), so a change that lands on five rules inside one debounce window is one
   step by construction, and stepping back puts all five where they were.

   `mirror()` is called from `schedulePreview`, which is documented as the one place that
   reliably sees an edit — every control that changes a rule asks for a redraw. Putting it
   there rather than wrapping each of the forty setters is what makes this a hundred lines
   instead of a rewrite of the panel.
   ===================================================================================== */
const MIXED='\u0000mixed';





/* ---------------- the viewport legend ----------------
   A key to the colours on the ground, lower right, and only the colours that are on it.
   The list comes from the renderer — see `Renderer.legendItems`, and §19a 275d for why a
   panel that worked out for itself what the viewport was drawing would be the wrong shape
   even when it happened to be right. */
/** Where the legend stands: bottom-right of the viewport. */
function placeLegend(){
  const box=$('#legend');
  if(box) box.style.bottom='';
}
function renderLegend(items){
  const box=$('#legend');
  if(!box) return;
  placeLegend();
  if(!items || !items.length){ box.hidden=true; box.innerHTML=''; return; }
  box.innerHTML='';
  for(const it of items){
    const r=document.createElement('div'); r.className='lgrow';
    const sw=document.createElement('span'); sw.className='lgsw';
    sw.style.background=it.col;
    const lb=document.createElement('span'); lb.textContent=it.label;
    r.appendChild(sw); r.appendChild(lb);
    if(it.note){
      const n=document.createElement('span'); n.className='lgnote'; n.textContent='— '+it.note;
      r.appendChild(n);
    }
    box.appendChild(r);
  }
  box.hidden=false;
}

/** Shows a mode's badge on its section heading, when the section is folded away.
 *
 *  A section that is folded is a mode you cannot see the controls for, and two of them
 *  change what the window does rather than what it looks like: painting takes the right
 *  mouse button, and inspecting a mesh takes the whole viewport. Folding the section away
 *  does neither of those things — correctly, folding is about the column — but it did take
 *  away the only way to say so and the only way back.
 *
 *  Robin, for both: "add a clickable button next to the PAINTING TOOLS headline (same
 *  line) that indicates painting mode is on, and clicking it turns it off (and removes the
 *  button)", and the same for Statics setup.
 *
 *  Only while the section is *closed*: with it open the control itself is right there, and
 *  two ways to stop painting six inches apart is one too many. */
function syncSectionBadges(){
  const set=(sect,badge,on)=>{
    const s=document.getElementById(sect), b=document.getElementById(badge);
    if(!s||!b) return;
    b.hidden = !(on && s.classList.contains('closed'));
  };
  set('secPaint','paintOnBadge',!!App.paintMode);
  /* The statics badge is gone (round 4 item 13): the viewport's own exit is the one
     way back, and a second door on a folded heading was one too many. */
}















/* Round 18ed, Robin: "if the pill with a grass rules name becomes too big so it clips
   (if the name is long) make it shorter by keeping the start and end of the name [...] so
   that the size of the grass rules name doesn't matter for the UI."

   CSS can only take an ellipsis off the end, and the end of a name is often the half that
   tells two sets apart ("Bitter Coast, working" against "Bitter Coast, shipped"), so the
   middle is what gives way. Measured rather than counted: a name's width is its glyphs'
   widths, and a counted cap is either too tight for `illill` or too loose for `WWWWWW`.

   The element decides what fits - it is the one wearing the font, the padding and the
   max-width - so this asks it, by setting a candidate and comparing what it can show with
   what it would need. Binary search on how many characters survive, so a fifty-character
   name costs six measurements rather than fifty. An element that is not on screen measures
   zero both ways and keeps its full text: it is measured again the next time it is
   written, which is every refresh. */
function fitMiddle(el, text){
  if(!el) return;
  const full=String(text==null? '' : text);
  el.textContent=full;
  const fits=()=>el.scrollWidth<=el.clientWidth+1;
  if(el.clientWidth<=0 || fits()) return;
  let lo=0, hi=full.length-1, best='…';
  while(lo<=hi){
    const keep=(lo+hi)>>1;                 // characters of the name kept, head and tail
    const head=Math.ceil(keep/2), tail=keep-head;
    el.textContent=full.slice(0,head)+'…'+(tail? full.slice(full.length-tail) : '');
    if(fits()){ best=el.textContent; lo=keep+1; } else hi=keep-1;
  }
  el.textContent=best;
}




















/** Asks for a name before making the thing. Resolves with the trimmed text, or null.
 *
 *  Robin: "I also want to be able to name my Paint rule on creation (no matter if it's
 *  from which +Paint rule button I click), so make that into it's own dialogue, where I
 *  can choose what name I want for it before it's created."
 *
 *  A paint rule's name is not an id read off the landscape — it is the one thing about it
 *  you choose, it is what the list shows, and it is what a colour is labelled with. Making
 *  the rule first and calling it "Paint rule 4" meant every one of them was named twice:
 *  once by the tool and once by you, in a field you had to go and find.
 *
 *  `o.taken` is the names already in use, checked as you type rather than on submit. Two
 *  paint rules may legitimately share a name — they are told apart by their identity, not
 *  by what they are called (§71) — so this warns rather than refuses.
 */
function askName(o){
  o=o||{};
  return new Promise(resolve=>{
    let m=document.getElementById('mAskName');
    if(!m){
      m=document.createElement('div'); m.className='modal'; m.id='mAskName';
      m.innerHTML='<div class="mbox" style="max-width:440px">'+
        '<div class="mhead"><b id="anTitle"></b><span style="flex:1"></span>'+
        '<button class="btn dim ic" id="anX">✕</button></div>'+
        '<div class="mbody"><div class="hint" id="anBody" style="margin:0 0 10px"></div>'+
        '<input class="fld" id="anField" style="width:100%">'+
        '<div class="hint" id="anWarn" style="margin:8px 0 0;min-height:16px"></div></div>'+
        '<div class="mfoot"><button class="btn" id="anCancel">'+T('dlg.cancel')+'</button>'+
        '<span style="flex:1"></span>'+
        '<button class="btn pri" id="anGo"></button></div></div>';
      document.body.appendChild(m);
    }
    const f=document.getElementById('anField');
    const warn=document.getElementById('anWarn');
    document.getElementById('anTitle').textContent=o.title||T('name.default_title');
    document.getElementById('anBody').innerHTML=o.body||'';
    document.getElementById('anGo').textContent=o.go||T('name.create');
    f.value=o.value||'';
    const taken=(o.taken||[]).map(x=>String(x).toLowerCase());
    const check=()=>{
      const v=f.value.trim();
      document.getElementById('anGo').disabled=!v;
      // `o.takenText` says what "taken" means here when it is not a rule's name (round 18as).
      warn.textContent = v && taken.includes(v.toLowerCase())
        ? (o.takenText||T('name.taken'))
        : '';
    };
    const close=v=>{
      document.removeEventListener('keydown',onKey,true);
      f.oninput=null; m.hidden=true; resolve(v);
    };
    const go=()=>{ const v=f.value.trim(); if(v) close(v); };
    const onKey=e=>{
      if(e.key==='Escape'){ e.preventDefault(); close(null); }
      if(e.key==='Enter' && document.activeElement===f){ e.preventDefault(); go(); }
    };
    f.oninput=check;
    document.getElementById('anGo').onclick=go;
    document.getElementById('anX').onclick=()=>close(null);
    document.getElementById('anCancel').onclick=()=>close(null);
    document.addEventListener('keydown',onKey,true);
    m.hidden=false;
    check();
    // Selected rather than merely focused: the suggested name is a starting point, and
    // typing over it should not mean clearing it first.
    f.focus(); f.select();
  });
}




/** Stops the webview offering back what you typed into this field last time.
 *
 *  Robin: "Remove the functionality that the webviewer has that saves input in fields
 *  and tries to autocomplete them." It is the browser's saved-form-data dropdown, and
 *  in a tool like this one it is worse than useless: the suggestions are texture ids and
 *  mesh paths from whatever config you had open an hour ago, they cover the asset
 *  browser's own list of what the install actually has, and picking one by accident
 *  writes a name no mod supplies.
 *
 *  `autocomplete="off"` is the switch; the rest stop the mobile-style helpers a webview
 *  also brings along. `data-lpignore` and `data-1p-ignore` ask the two common password
 *  managers to leave the field alone as well, since they inject on any text input they
 *  find and this one is never a login. */
function noAutofill(i){
  if(!i || !i.setAttribute) return i;
  /* Round 17L: `off` is a request the webview is allowed to ignore - Chromium's own
     heuristics override it on fields it thinks it recognises - and a value it does not
     know is not. So both: the standard switch, and a token that matches no autofill
     category, which is the trick that actually silences it. */
  i.setAttribute('autocomplete','off');
  i.setAttribute('autocomplete','gardenfell-none');
  i.setAttribute('autocapitalize','off');
  i.setAttribute('autocorrect','off');
  i.setAttribute('data-lpignore','true');
  i.setAttribute('data-1p-ignore','true');
  i.spellcheck=false;
  return i;
}







/* Round 18k: every number field scrubs.
 *
 * Robin: "For any number field, I want to be able to press and hold and move mouse to
 * left and right to change the number value, like they do in Unreal engine and similar.
 * With this change, we can remove all the arrow up and arrow down on number fields. A
 * click should still select the field so I can put in a manual input."
 *
 * So a press on a field that is not being edited does nothing until the mouse has moved
 * a few pixels; then it is a drag, and every four pixels is one step of the field's own
 * `step` - Shift for ten at a time, Ctrl for a tenth - with the value landing in the
 * field and its `change` fired as it goes, so the preview follows the hand. Let go
 * without having moved and the field takes the caret with everything selected, which is
 * what a click on a number field is for. Escape mid-drag puts the value back. A field
 * that already has the caret is left to the browser: dragging in it selects text, as it
 * should.
 *
 * One listener on the document rather than one per field, because the fields are
 * rebuilt on every redraw of the panel and a hundred listeners that come and go are a
 * hundred chances to miss one. The spinner arrows are gone by CSS; they were the size of
 * a grain of rice and this is what they were for. */
const Scrub=(()=>{
  const PX=4;     // pixels of drag per step
  const DEAD=3;   // pixels before a press becomes a drag
  let live=null;
  const decimals=v=>{ const s=String(v); if(/e/i.test(s)) return 6; const i=s.indexOf('.'); return i<0? 0 : Math.min(6,s.length-i-1); };
  const round=(v,d)=>{ const m=Math.pow(10,d); return Math.round(v*m)/m; };
  const wants=el=> el instanceof HTMLInputElement && el.type==='number' && el.classList.contains('num')
                   && !el.disabled && !el.readOnly;
  function base(l){
    if(l.start!=='' && isFinite(parseFloat(l.start))) return parseFloat(l.start);
    /* A blank field starts from what blank *means*. Round 18q: on a grass card that is
       the word "inherit" rather than the rule's number - Robin asked for the word, since
       a number in grey reads as a value somebody typed - so the number it stands for
       rides on the field itself and is looked at first. */
    const inh=parseFloat(l.i.dataset.inherit);
    if(isFinite(inh)) return inh;
    const ph=parseFloat(l.i.placeholder);
    return isFinite(ph)? ph : 0;
  }
  function begin(e){
    if(e.button!==0 || live) return;
    const i=e.target;
    if(!wants(i) || document.activeElement===i) return;
    e.preventDefault();   // neither the caret nor a text selection, until we know it is a click
    const step=parseFloat(i.step);
    live={i, x:e.clientX, y:e.clientY, start:i.value, step:(isFinite(step)&&step>0)? step : 1,
          dragging:false, acc:0, last:null};
    window.addEventListener('mousemove',move,true);
    window.addEventListener('mouseup',end,true);
    window.addEventListener('keydown',key,true);
  }
  function move(e){
    const l=live; if(!l) return;
    if(!l.dragging){
      if(Math.abs(e.clientX-l.x)<DEAD && Math.abs(e.clientY-l.y)<DEAD) return;
      l.dragging=true; l.x=e.clientX;
      l.i.classList.add('scrub'); document.body.classList.add('scrubbing');
      return;
    }
    const mult = e.shiftKey? 10 : (e.ctrlKey||e.altKey)? 0.1 : 1;
    l.acc += (e.clientX-l.x)/PX*l.step*mult;
    l.x=e.clientX;
    const d=Math.max(decimals(l.step*mult), decimals(l.start||0));
    let v=round(base(l)+l.acc, d);
    const mn=parseFloat(l.i.min), mx=parseFloat(l.i.max);
    if(isFinite(mn)) v=Math.max(mn,v);
    if(isFinite(mx)) v=Math.min(mx,v);
    const txt=String(v);
    if(txt===l.last) return;
    l.last=txt; l.i.value=txt;
    l.i.classList.remove('mixed');
    l.i.dispatchEvent(new Event('input',{bubbles:true}));
    l.i.dispatchEvent(new Event('change',{bubbles:true}));
  }
  function key(e){
    const l=live; if(!l || !l.dragging || e.key!=='Escape') return;
    e.preventDefault(); e.stopPropagation();
    l.i.value=l.start;
    l.i.dispatchEvent(new Event('change',{bubbles:true}));
    finish();
  }
  function end(){
    const l=live; if(!l) return;
    finish();
    if(!l.dragging){ l.i.focus(); try{ l.i.select(); }catch(_){ /* not every field can */ } }
  }
  function finish(){
    const l=live; live=null;
    window.removeEventListener('mousemove',move,true);
    window.removeEventListener('mouseup',end,true);
    window.removeEventListener('keydown',key,true);
    if(l){ l.i.classList.remove('scrub'); document.body.classList.remove('scrubbing'); }
  }
  document.addEventListener('mousedown',begin,true);
  return {PX, DEAD, dragging:()=>!!(live&&live.dragging)};
})();







/** A hint line; the optional second string becomes its hover tooltip — where the
 *  distilled sentence's detail lives. */
function hint(html,tip){ const d=document.createElement('div'); d.className='hint';
  d.innerHTML=html; if(tip) d.title=tip; return d; }























/* =====================================================================================
   Round 18q: what the cull mask is asking.

   Robin: "I want all rules that cull grass to be represented in the cull mask on terrain
   and on meshes. Default is to show the selected Rules culling mask (as we do currently,
   but with the extra rules like facing, curvature, near objects and shoreline also taken
   into account). When hovering a grasscard, the culling mask on terrain and on meshes
   should update to reflect any override settings that might change the culling area
   (usually making it smaller)."

   So: the rule's own gates, narrowed by the hovered card's. Bands intersect by taking the
   tighter end of each - which is what the engine does, asking the rule first and then the
   card - and the two facing arcs are both handed over rather than merged, since two arcs
   have no single arc for an intersection. What comes back is what `_cullUniforms` reads.

   A rule that states nothing gates nothing: every band comes back wide open, and the mask
   shades nothing at all.
   ===================================================================================== */
function cullFor(sel,slot){
  const NO=1e9;
  if(!sel) return null;
  const B=sel.b||{}, T=sel.terrain||TERRAIN_DEF;
  const num=v=>(v==null || v===MIXED || !isFinite(+v))? null : +v;
  const band=(b,lo,hi)=>{
    const l=num(lo), h=num(hi);
    if(l!=null) b[0]=Math.max(b[0],l);
    if(h!=null) b[1]=Math.min(b[1],h);
    return b;
  };
  const c={slope:[0,90], h:[-NO,NO], shore:[-NO,NO], curve:[-NO,NO], obs:[-NO,NO],
           face:null, face2:null, canopy:null};
  band(c.slope, num(T.slopeMin), num(B.fMaximumAngle));
  band(c.h, num(B.fMinHeight), num(B.fMaxHeight));
  const rb=b=>(b && b!==MIXED)? b : null;
  const sh=rb(T.shore), cv=rb(T.curve), ob=rb(T.obstacle);
  if(sh) band(c.shore, sh.lo, sh.hi);
  if(cv) band(c.curve, cv.lo, cv.hi);
  if(ob) band(c.obs, ob.lo, ob.hi);
  const rf=rb(T.facing);
  if(rf) c.face=[+rf.heading||0, rf.width==null? 90 : +rf.width];
  if(slot && !slot.empty){
    band(c.slope, slot.slopeMin, slot.maxAngle);
    band(c.h, slot.minH, slot.maxH);
    if(slot.shoreOn || slot.shoreLo!=null || slot.shoreHi!=null) band(c.shore, slot.shoreLo, slot.shoreHi);
    if(slot.curveOn || slot.curveLo!=null || slot.curveHi!=null) band(c.curve, slot.curveLo, slot.curveHi);
    band(c.obs, slot.obsLo, slot.obsHi);
    if(slot.facingAt!=null || slot.facingWidth!=null)
      c.face2=[+slot.facingAt||0, slot.facingWidth==null? 90 : +slot.facingWidth];
    /* The canopies, as bits of the field's blue channel: the set's first eight, in the
       order the left column lists them, which is the order the engine packs them in. */
    const ids=(App.cfg.canopies||[]).map(k=>k.id).filter(Boolean).slice(0,8);
    let under=0, clear=0, underAny=false, clearAny=false;
    for(const r of (slot.canopies||[])){
      if(!r || !r.canopy) continue;
      if(r.canopy==='*'){ if(r.under!==false) underAny=true; else clearAny=true; continue; }
      const b=ids.indexOf(r.canopy);
      if(b<0) continue;                       // a canopy past the eighth, or one since gone
      if(r.under!==false) under|=1<<b; else clear|=1<<b;
    }
    if(under||clear||underAny||clearAny) c.canopy={under, clear, underAny, clearAny};
  }
  return c;
}
/** Whether the mask needs the engine's field - which is what makes asking for one worth
 *  the round trip. Curvature, the distance to objects and the canopies are the three the
 *  viewport cannot answer from a fragment alone. */
function cullWantsField(c){
  const NO=1e8;
  return !!c && (c.curve[0]>-NO || c.curve[1]<NO || c.obs[0]>-NO || c.obs[1]<NO || !!c.canopy);
}





