/* Gardenfell, 09_ui.js. Copyright (c) 2026 Robin Hjelte. MIT License; see page-source/LICENSE. */

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
  showAdjacent:0,           // rings of neighbours loaded with the cell: 0..3 (round 15; was a bool). Wraithguard keeps 0; Gardenfell 18hj defaults to 1
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
  cardShutBy:{},   // round 18er: and the cards that have been told otherwise
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
  avoidPad:80,          // the clearance outdoors (18ha: 64 by default, was 0; 18hb: 80)
  avoidPadIn:32,        // and indoors, where rooms are smaller (18ha)
  avoidMinSize:8,       // colliders smaller than this radius are stepped over
  avoidHeight:96,
  hiTex:true,           // hovering a ground texture lights up the ground it covers - always (18fv: the switch is gone)
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
  el._hoverEnter=enter;
  el.addEventListener('pointerenter',()=>{ Hover.el=el; enter(); });
  el.addEventListener('pointerleave',()=>{ if(Hover.el===el) Hover.el=null; Hover.clear(); });
}
/* Round 18fs: the element under the pointer, so a rebuild can tell when it has just
   removed it. Robin: "When hovering a rule element in the left panel, so that the ground
   is highlighted, and clicking the enable / disable button, and then immediately after
   moving the cursor away from the list [...] the highlight stays on the ground forever."
   A row taken out of the document while the pointer is on it never gets its
   `pointerleave`, so the highlight it switched on has nobody to switch it off. Every
   list rebuild ends in `Hover.check()`: if the element that lit the highlight is gone,
   the highlight goes with it. Should the pointer still be over the list, the browser's
   own synthetic move enters the new row under it and lights that one instead. */
const Hover={
  el:null,
  // Where the pointer last was, so a rebuild can ask what is under it now.
  x:-1, y:-1,
  clear(){
    hoverTexture(null); hoverSlot(-1,-1);
    if(App.R && App.R.setStaticHighlight) App.R.setStaticHighlight(null);
  },
  check(){
    if(!Hover.el || Hover.el.isConnected) return;
    Hover.el=null; Hover.clear();
    /* The pointer is still where it was; what is under it is the question. Chrome does
       re-enter the new element by a synthetic move of its own, but on its own clock -
       under a busy page it came late or not at all (t_panel, 18fv) - so the element under
       the pointer is looked up here and entered directly: whichever ancestor carries a
       hover of its own is the one lit. */
    if(Hover.x<0) return;
    let n=document.elementFromPoint(Hover.x, Hover.y);
    while(n && !n._hoverEnter) n=n.parentElement;
    if(n){ Hover.el=n; n._hoverEnter(); }
  },
};
document.addEventListener('pointermove',e=>{
  Hover.x=e.clientX; Hover.y=e.clientY;
  /* And the leave the browser cannot give: an element entered by `check` above was never
     hovered as far as the browser knows, so its `pointerleave` never comes. The move
     itself says where the pointer is; off the element, the highlight goes out. */
  if(Hover.el && e.target instanceof Node && !Hover.el.contains(e.target)){ Hover.el=null; Hover.clear(); }
},{passive:true});

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
/** `suffix` (18fe) is written whole after the name and never shortened - the rules pill's
 *  " · 128 rules" - so what gives way is always the name's middle. Robin: "Always write
 *  out the full ' · 128 rules', so when the string is too long, we shorten the grass
 *  rules name the way we did before." */
function fitMiddle(el, text, suffix){
  if(!el) return;
  const full=String(text==null? '' : text), tail0=String(suffix==null? '' : suffix);
  el.textContent=full+tail0;
  const fits=()=>el.scrollWidth<=el.clientWidth+1;
  if(el.clientWidth<=0 || fits()) return;
  let lo=0, hi=full.length-1, best='…'+tail0;
  while(lo<=hi){
    const keep=(lo+hi)>>1;                 // characters of the name kept, head and tail
    const head=Math.ceil(keep/2), tail=keep-head;
    el.textContent=full.slice(0,head)+'…'+(tail? full.slice(full.length-tail) : '')+tail0;
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

/* Gardenfell's section fold (round 18er), for the Preview's section headings
   (11_events.js); the rest of 09_ui.js's 18er-18hb changes are the grass rule panel. */
/* Round 18er: a fold that moves. Robin: "Lets see if we can animate collapsing and
   revealing collapsable sections."

   **The class goes on at once and only the picture takes its time.** Everything that asks
   whether a section is folded - `syncSectionBadges`, the profile's `collect`, every test
   that clicks a heading and reads `closed` a moment later - gets the same answer in the
   same tick it always did. What `folding` does is keep the body drawn, and give it a
   height transition, for the few frames its height is running down to nothing; against
   the `display:none` the closed class already carries (01_head.html).

   **The tidy-up is a timer, and that is the whole lesson of this change.** The first cut
   used `Element.animate` and removed `folding` in its `onfinish`. In the test harness
   that never fires: the animation sits at `playState:"running"` with `currentTime` stuck
   at 0 and `startTime` null, because nothing is driving the document's animation clock
   there - ten of them piled up in one run. An end state that waits on a clock is an end
   state that can never arrive, and a section stuck half-folded is a great deal worse than
   one that folds instantly. So the picture is a CSS transition (which is free to not run,
   and simply snaps where the clock is frozen) and the cleanup is `setTimeout`, which
   fires wherever the code is.

   A person who has asked for less motion gets the fold they had before this round. */
const FOLD_MS=170;
/** The class and the picture. `cls` is the class that says "shut" - `closed` for a
 *  section, `folded` for the whole rules list - and it lands in this tick whatever the
 *  height does afterwards. */
function foldBox(box, body, shut, cls, after){
  if(!box) return null;
  box.classList.toggle(cls||'closed', !!shut);
  return foldHeight(body, shut, after);
}
/** The picture alone, for a body whose shut state is not a class on anything - the rules
 *  list's groups, where a folded group has no rows in the document at all and the fold is
 *  the last thing that sees them (round 18eu).
 *
 *  `after` runs exactly once, whichever way the fold ends: on the transition, on the
 *  timer, when there is nothing to animate, or when another fold takes this body over -
 *  because for the groups it is the call that drops the rows, and a fold that swallowed it
 *  would leave the list showing rows it has already decided are not there. */
function foldHeight(body, shut, after){
  const fire=()=>{ if(after){ const f=after; after=null; f(); } };
  if(!body){ fire(); return null; }
  /* Whatever the last fold left running, stopped - the timer and the listener both, or a
     fold that is overtaken half way tidies up after the one that replaced it - and its
     own `after` settled before this one starts. */
  if(body._foldT){ clearTimeout(body._foldT); body._foldT=0; }
  if(body._foldEnd){ body.removeEventListener('transitionend', body._foldEnd); body._foldEnd=null; }
  if(body._foldAfter){ const f=body._foldAfter; body._foldAfter=null; f(); }
  body._foldAfter=after? fire : null;
  const clear=()=>{ body.style.height=''; body.style.paddingTop=''; body.style.paddingBottom='';
                    body.classList.remove('folding'); };
  let reduce=false;
  try{ reduce=!!(window.matchMedia && matchMedia('(prefers-reduced-motion: reduce)').matches); }catch(_){ }
  if(reduce){ clear(); fire(); return null; }
  /* Where it stands now, so a second click part way through carries on from there
     rather than jumping to one end and running back. */
  const cur=body.classList.contains('folding')? body.getBoundingClientRect().height : null;
  body.classList.add('folding');
  /* Cleared before anything is read, or every number below is the last fold's target
     rather than the body's own. */
  body.style.height=''; body.style.paddingTop=''; body.style.paddingBottom='';
  const cs=getComputedStyle(body);
  const pt=cs.paddingTop, pb=cs.paddingBottom;
  const h=body.scrollHeight;          // content *and* padding, measured drawn
  const from=(cur!=null)? cur : (shut? h : 0);
  body.style.height=from+'px';
  /* Round 18et: the padding goes with it. `box-sizing:border-box` means an element told
     `height:0` is still as tall as its own padding, so without this the fold ran down to
     21px and then jumped the rest of the way when `display:none` arrived - the "small lag
     at the very end" Robin reported. Only on a fold that starts from rest: one that
     overtakes another picks up the paddings where that one left them. */
  if(cur==null && !shut){ body.style.paddingTop='0px'; body.style.paddingBottom='0px'; }
  void body.offsetHeight;             // a state to move *from*, or there is no transition
  body.style.height=(shut? 0 : h)+'px';
  body.style.paddingTop=shut? '0px' : pt;
  body.style.paddingBottom=shut? '0px' : pb;
  /* **The end state is made true by two things, and only one of them can be trusted.**
     The height's own `transitionend` is what makes the fold end when it looks like it has
     ended - waiting on the timer alone left a tail of dead frames after the motion had
     stopped. But a transition that never runs never ends: the test harness's animation
     clock does not advance at all (18er), and a fold whose height does not actually change
     fires nothing either. So the timer stays, as the thing that always fires. */
  const done=()=>{
    if(body._foldT){ clearTimeout(body._foldT); body._foldT=0; }
    if(body._foldEnd){ body.removeEventListener('transitionend', body._foldEnd); body._foldEnd=null; }
    body._foldAfter=null;
    clear();
    fire();
  };
  body._foldEnd=(e)=>{ if(e.target===body && e.propertyName==='height') done(); };
  body.addEventListener('transitionend', body._foldEnd);
  body._foldT=setTimeout(done, FOLD_MS+60);
  return body._foldT;
}

/* Gardenfell 18hb and 18hf, brought in 2026-10-12 (Static): the left column's width by
   dragging its edge, and the page's own find bar on Ctrl+F. */
/* Round 18hb: the side columns, dragged to size on their inner edges.
   Robin: "I would like us to make the right and left side panels to be resizable, by
   clicking and dragging them. Keep the current size as the default value, and save down
   the current value to the profile. In the Theme section of the Settings menu, add a
   button to 'Reset panel sizes' that sets them to the default values." The grip is the
   column's own edge (`.colgrip`, astride the border line, no thickness of its own), the
   widths are CSS variables on the root, and the profile carries them as `panel_left` and
   `panel_right` under `[viewport]` (19_settings.js). */
const Panels={
  DEF:{left:310, right:396},
  MIN:{left:220, right:280},
  w:{left:310, right:396},
  /* Neither column may push the viewport under 360 pixels. Wraithguard's page has no
     right column (Gardenfell's was the grass rules); a column that is not there takes no
     room. */
  max(side){
    const other=side==='left'? 'right' : 'left';
    const there=!!document.getElementById(other);
    return Math.max(this.MIN[side], window.innerWidth-360-(there? this.w[other] : 0));
  },
  clamp(side,v){ return Math.round(Math.min(this.max(side), Math.max(this.MIN[side], +v||this.DEF[side]))); },
  apply(){
    const r=document.documentElement.style;
    r.setProperty('--left-w', this.w.left+'px');
    r.setProperty('--right-w', this.w.right+'px');
    if(typeof App==='object' && App.R && App.R.resize) App.R.resize();
  },
  set(side,v,save){
    const w=this.clamp(side,v);
    if(w===this.w[side]) return;
    this.w[side]=w; this.apply();
    if(save && typeof PrevSettings==='object' && PrevSettings.touch) PrevSettings.touch();
  },
  reset(save){
    this.w={...this.DEF}; this.apply();
    if(save!==false && typeof PrevSettings==='object' && PrevSettings.touch) PrevSettings.touch();
  },
  init(){
    for(const side of ['left','right']){
      const col=document.getElementById(side); if(!col || col.querySelector('.colgrip')) continue;
      const g=document.createElement('div'); g.className='colgrip'; g.dataset.side=side;
      col.appendChild(g);
      g.addEventListener('pointerdown',e=>{
        if(e.button!==0) return;
        e.preventDefault();
        const x0=e.clientX, w0=this.w[side];
        try{ g.setPointerCapture(e.pointerId); }catch(_){ }
        document.body.classList.add('colresizing');
        const move=ev=>{ const d=ev.clientX-x0; this.set(side, side==='left'? w0+d : w0-d, false); };
        const up=ev=>{
          g.removeEventListener('pointermove',move); g.removeEventListener('pointerup',up); g.removeEventListener('pointercancel',up);
          try{ g.releasePointerCapture(ev.pointerId); }catch(_){ }
          document.body.classList.remove('colresizing');
          if(this.w[side]!==w0 && typeof PrevSettings==='object' && PrevSettings.touch) PrevSettings.touch();
        };
        g.addEventListener('pointermove',move); g.addEventListener('pointerup',up); g.addEventListener('pointercancel',up);
      });
    }
    const b=document.getElementById('setPanelsReset');
    if(b) b.onclick=()=>this.reset(true);
    this.apply();
  },
};

/* Round 18hf: the find bar. Robin: "When I click 'CTRL+F' I want the search bar that
   opens up to spawn centered just under the brightness meter in the viewport, unless i
   have a dialogue open, then I want it centered at the very top of the window. Use the
   same spacing from the top as between the viewport and the brightness slider window."
   The bar that opened before was the webview's own, which goes where the webview puts
   it; this one is the page's, so it goes where Robin says. It searches the text on screen
   (every visible text node outside itself; a folded section or a hidden dialogue is not
   on screen), paints the matches through the CSS highlight API with the current one in
   the warning colour, and scrolls the current one into view. */
const Finder={
  GAP:12,                 // the pills' own distance from the viewport's top edge
  q:'', ranges:[], at:-1, _timer:0, _mo:null,
  el(){ return document.getElementById('findBar'); },
  input(){ return document.getElementById('findText'); },
  isOpen(){ const b=this.el(); return !!b && !b.hidden; },
  /** Where the bar stands: under the row of pills in the viewport (18hi: the brightness,
      the grass distance and, in a room, the view distance, centred as one), or at the top
      of the window while a dialogue is open, with the pills' own gap in both. */
  place(){
    const b=this.el(); if(!b || b.hidden) return;
    const modal=this.topModal();
    const pill=document.getElementById('vpTop');
    let x, y;
    if(!modal && pill && pill.offsetParent!==null){
      const r=pill.getBoundingClientRect();
      x=r.left+r.width/2; y=r.bottom+this.GAP;
    }else{
      x=window.innerWidth/2; y=this.GAP;
    }
    b.style.left=Math.round(x)+'px'; b.style.top=Math.round(y)+'px';
    b.dataset.over=modal? 'dialogue' : 'viewport';
  },
  open(){
    const b=this.el(), i=this.input(); if(!b || !i) return;
    b.hidden=false; this.place();
    i.focus({preventScroll:true}); i.select();
    if(this.q) this.search(this.q, true);
    // Dialogues open and close while the bar is up; it follows them.
    if(!this._mo && typeof MutationObserver==='function'){
      let due=false;
      this._mo=new MutationObserver(()=>{ if(due) return; due=true; setTimeout(()=>{
        due=false; this.place();
        // A dialogue came or went: what can be read changed, so the matches are asked again.
        if(this.q.trim() && this.topModal()!==this._root) this.search(this.q, true);
      },0); });
      // Attributes and children both: a dialogue is shown by its `hidden` going, or made new.
      this._mo.observe(document.body,{attributes:true, attributeFilter:['hidden','class','style'], childList:true, subtree:true});
    }
  },
  close(){
    const b=this.el(); if(!b || b.hidden) return;
    b.hidden=true; this.clearMarks();
    if(this._mo){ this._mo.disconnect(); this._mo=null; }
    const i=this.input(); if(i && document.activeElement===i) i.blur();
  },
  toggle(){ if(this.isOpen()) this.close(); else this.open(); },
  /** The dialogue on top, when one is open: the open modal with the highest z-index,
      the last in the document among equals. Round 18hh, Robin: "When a dialogue window is
      open, only search in the top most open dialogue [...] Make sure it's always the top
      most dialogue that is searched when I have several dialogues open." */
  topModal(){
    let best=null, bz=-Infinity;
    for(const m of document.querySelectorAll('.modal:not([hidden])')){
      const z=parseInt(getComputedStyle(m).zIndex,10)||0;
      if(z>=bz){ bz=z; best=m; }
    }
    return best;
  },
  /** Every visible text node in what can be read right now, in document order: the
      dialogue on top when one is open, the whole window otherwise, the bar itself apart. */
  textNodes(){
    const out=[], bar=this.el();
    const root=this.topModal()||document.body;
    this._root=this.topModal();
    const skip=new Set(['SCRIPT','STYLE','TEMPLATE','NOSCRIPT','CANVAS','SVG','TITLE']);
    const walker=document.createTreeWalker(root, NodeFilter.SHOW_ELEMENT|NodeFilter.SHOW_TEXT, {
      acceptNode:n=>{
        if(n.nodeType===1){
          if(n===bar || skip.has(n.tagName) || n.hidden) return NodeFilter.FILTER_REJECT;
          const cs=getComputedStyle(n);
          if(cs.display==='none' || cs.visibility==='hidden') return NodeFilter.FILTER_REJECT;
          return NodeFilter.FILTER_SKIP;
        }
        return /\S/.test(n.nodeValue)? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_SKIP;
      }});
    let n; while((n=walker.nextNode())) out.push(n);
    return out;
  },
  search(q, keep){
    this.q=q;
    const needle=q.trim().toLowerCase();
    const prev=this.ranges[this.at];
    this.ranges=[];
    if(needle){
      for(const node of this.textNodes()){
        const hay=node.nodeValue.toLowerCase();
        let i=hay.indexOf(needle);
        while(i>=0){
          const r=document.createRange(); r.setStart(node,i); r.setEnd(node,i+needle.length);
          this.ranges.push(r);
          i=hay.indexOf(needle,i+needle.length);
        }
      }
    }
    // The same match stays current across a retype when it still exists; else the first.
    let at=-1;
    if(keep && prev && this.ranges.length){
      at=this.ranges.findIndex(r=>r.startContainer===prev.startContainer && r.startOffset===prev.startOffset);
    }
    this.at=this.ranges.length? (at>=0? at : 0) : -1;
    this.paint(true);
  },
  step(d){
    if(!this.ranges.length){ this.paint(false); return; }
    this.at=(this.at+d+this.ranges.length)%this.ranges.length;
    this.paint(true);
  },
  paint(scroll){
    const count=document.getElementById('findCount');
    if(count){
      if(!this.q.trim()){ count.textContent=''; count.classList.remove('none'); }
      else if(!this.ranges.length){ count.textContent=T('find.none'); count.classList.add('none'); }
      else { count.textContent=T('find.count',{i:this.at+1, n:this.ranges.length}); count.classList.remove('none'); }
    }
    if(typeof CSS!=='undefined' && CSS.highlights && typeof Highlight==='function'){
      CSS.highlights.set('gf-find', new Highlight(...this.ranges));
      CSS.highlights.set('gf-find-current', new Highlight(...(this.at>=0? [this.ranges[this.at]] : [])));
    }else if(this.at>=0){
      // No highlight API: the current match is the selection, which is at least visible.
      const sel=window.getSelection(); sel.removeAllRanges(); sel.addRange(this.ranges[this.at]);
    }
    if(scroll && this.at>=0){
      const r=this.ranges[this.at];
      const el=r.startContainer.parentElement;
      if(el && el.scrollIntoView) el.scrollIntoView({block:'center', inline:'nearest'});
      this.place();
    }
  },
  clearMarks(){
    this.ranges=[]; this.at=-1;
    if(typeof CSS!=='undefined' && CSS.highlights){ CSS.highlights.delete('gf-find'); CSS.highlights.delete('gf-find-current'); }
    const count=document.getElementById('findCount'); if(count){ count.textContent=''; count.classList.remove('none'); }
  },
  init(){
    const b=this.el(), i=this.input(); if(!b || !i) return;
    /* On the document, capturing, and registered at start-up: a dialogue's own key
       handler is registered when it opens and so runs after this one, which is what
       keeps Enter and Escape typed into the bar from pressing the dialogue's buttons. */
    document.addEventListener('keydown',e=>{
      /* Wraithguard: not Ctrl+Shift+F (the Editor's search of every record), and not while
         the Editor is open - there Ctrl+F is the Object Window's filter (50_wg_editor_keys). */
      if((e.ctrlKey||e.metaKey) && !e.altKey && !e.shiftKey && (e.key==='f'||e.key==='F')
         && !document.body.classList.contains('wgEditMode')){
        e.preventDefault(); e.stopImmediatePropagation();     // the webview's own bar stays shut
        this.open();
        return;
      }
      if(document.activeElement!==i || i.closest('#findBar').hidden) return;
      if(e.key==='Enter'){
        e.preventDefault(); e.stopImmediatePropagation();
        if(i.value!==this.q) this.search(i.value, true); else this.step(e.shiftKey? -1 : 1);
      }else if(e.key==='Escape'){
        e.preventDefault(); e.stopImmediatePropagation();
        this.close();
      }
    },true);
    i.addEventListener('input',()=>{ clearTimeout(this._timer); this._timer=setTimeout(()=>this.search(i.value, true),90); });
    const on=(id,fn)=>{ const el=document.getElementById(id); if(el) el.onclick=fn; };
    on('findNext',()=>this.step(1)); on('findPrev',()=>this.step(-1)); on('findClose',()=>this.close());
    window.addEventListener('resize',()=>this.place());
  },
};
