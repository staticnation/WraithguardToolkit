
/* =====================================================================================
   Wiring
   ===================================================================================== */
function refreshAll(){
  schedulePreview();
}

/* ===============================================================================
   THE FILLED PART OF A RAIL — round 18co.

   `01_head.html` gives every slider's track a themed trough (see the long note there for
   what was measured and why there was no smaller change available), and a track with a
   background of its own is a track the browser has stopped drawing the fill on. So the fill
   is a gradient whose stop is `--p`, and `--p` has to say where the thumb's *middle* is —
   `calc(6px + (100% - 12px) * f)`, because a 12px thumb's centre travels six pixels in from
   each end, which was measured rather than guessed: at half value the browser puts it at
   exactly 50% of the input's width.

   **18cg shipped this and 18cj spent a round chasing it**, because three places write a
   slider's value by plain assignment — `syncNav`, `showSpeed`, `PrevSettings.applyNative` —
   and an assignment fires no event, so a delegated `input` listener never hears it and the
   rail keeps describing the value the markup shipped with. Robin saw that as "some sliders
   show too much color pre-filled in". Those three were fixed one at a time, and the note in
   `claude/memory.md` says the honest part out loud: there was a fourth path nobody had found.

   So this time the *class* is closed rather than the cases. Three things, in order of how
   much they cover:

     1. **The `value` setter itself.** Patched once, here, so every assignment anywhere in
        the program — past, present, and the one somebody writes next year — lights its own
        rail on the way through. A prototype patch is a real cost and is worth it exactly
        once: this is a single-page application that owns its whole document, there is no
        framework to surprise, and the alternative is a list of call sites that is wrong the
        moment it is written. The original descriptor is called first, so the assignment
        behaves exactly as it did.
     2. **A delegated capture-phase `input` listener**, for the ordinary case of a hand on a
        slider, including sliders built long after start-up.
     3. **One sweep** at start-up, for the twenty-one sliders written into `02_body.html`
        with a `value` attribute: an attribute sets the default without going through the
        setter, so those would otherwise open with an unlit rail. Nothing else needs it —
        every slider built in script (`09_ui.js` ×2, `18_cellpreview.js`) assigns `.value`
        and so lights itself on the way in, and no dialogue builds a range input from a
        markup string, which is the one case that would want a mutation observer. `--p` is a
        length rather than a colour, so a theme switch does not invalidate it; `App.litTracks`
        is exposed for a future panel that needs the sweep for a region of its own.

   `--p` is a percentage the gradient reads; it is set on the input, not on the track, because
   a pseudo-element cannot carry a custom property of its own. */
function litOne(e){
  if(!e || e.type!=='range') return;
  const min=+e.min||0, max=(e.max===''||e.max==null)? 100 : +e.max;
  const span=max-min;
  const f=span>0? Math.min(1,Math.max(0,((+e.value||0)-min)/span)) : 0;
  /* Vertical sliders (`#vpSpeedBar`) measure `--p` down the height instead, and the gradient
     there runs `to top`; the fraction is the same number either way, so one formula does
     both and only the axis the gradient names differs. */
  e.style.setProperty('--p','calc(6px + (100% - 12px) * '+f.toFixed(4)+')');
}
function litTracks(root){
  const box=root||document;
  box.querySelectorAll && box.querySelectorAll('input[type=range]').forEach(litOne);
}
App.litTracks=litTracks; App.litOne=litOne;
function bindLitTracks(){
  document.addEventListener('input',e=>{ if(e.target && e.target.type==='range') litOne(e.target); },true);
  const d=Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value');
  if(d && d.set && !d.set.__lit){
    const set=function(v){ d.set.call(this,v); if(this.type==='range') litOne(this); };
    set.__lit=true;
    Object.defineProperty(HTMLInputElement.prototype,'value',
      {configurable:true, enumerable:d.enumerable, get:d.get, set});
  }
  litTracks();
}

function boot(){
  bindLitTracks();
  // region autocomplete
  const dl=document.createElement('datalist'); dl.id='regionList';
  MW_REGIONS.forEach(r=>{ const o=document.createElement('option'); o.value=r; dl.appendChild(o); });
  document.body.appendChild(dl);

  /* Every field in the window, and every field that ever gets focus after this.
     `noAutofill` is applied where fields are built, but the static markup is not built
     there and neither is anything a future panel adds, so the sweep catches the first
     kind and the capture-phase listener catches the rest — before the field is focused,
     which is the moment the webview would decide to offer its history. */
  document.querySelectorAll('input,textarea,select').forEach(noAutofill);
  document.addEventListener('focusin',e=>{
    const t=e.target;
    if(t && (t.tagName==='INPUT'||t.tagName==='TEXTAREA'||t.tagName==='SELECT')) noAutofill(t);
  },true);
  /* Round 17L, Robin: "make it so that the tool never saves cookies etc. so we don't get
     any autocomplete on any text fields". The window is built `incognito` (tauri.conf.json)
     so the webview keeps nothing between runs; this is the other half - anything an
     earlier, non-incognito build left behind is cleared on the way in, and the page
     itself writes none of it. */
  try{
    for(const c of String(document.cookie||'').split(';')){
      const k=c.split('=')[0].trim();
      if(k) document.cookie=k+'=; expires=Thu, 01 Jan 1970 00:00:00 GMT; path=/';
    }
    if(window.localStorage) localStorage.clear();
    if(window.sessionStorage) sessionStorage.clear();
  }catch(_){ }
  /* And nothing is reachable by Tab. Robin: "Make it so that no element is selectable by
     using Tab so we never get the white outline on stuff." Swallowed here rather than by
     putting `tabindex="-1"` on a hundred controls, so it holds for every panel built
     later too. The viewport's own Tab - which swaps the navigation scheme (round 17j) -
     has already run by the time this does: it listens on the canvas, this on the
     document, and the bubble reaches them in that order.

     Round 18cw: and the pointer being over the viewport is as good as the viewport being
     focused. Robin: "make it so that the TAB button works to switch between navigation
     modes as long as my mouse hovers the viewport. Currently I have to click the viewport
     once to make that work ... Hovering the viewport however must not make me lose focus
     from a text field if I am in it and editing. It's OK to lose focus like that only if I
     hover the viewport AND press tab." So hovering changes nothing by itself - the field
     keeps the keys - and the Tab is what hands them over: the field lets go, the viewport
     takes the focus, the scheme swaps. A Tab the viewport already answered (it had the
     focus) is not answered twice, and a dialogue over the page keeps its own Tab. */
  document.addEventListener('keydown',e=>{
    if(e.key!=='Tab') return;
    e.preventDefault();
    const R=App.R;
    if(!R || !R.hovered || !R.cv || e.target===R.cv || e.altKey || e.ctrlKey || e.metaKey) return;
    if(document.querySelector('.modal:not([hidden])')) return;
    const a=document.activeElement;
    try{ if(a && a!==document.body && a!==R.cv) a.blur(); }catch(_){ }
    try{ R.cv.focus({preventScroll:true}); }catch(_){ }
    R.setNav(R.nav==='wasd'? 'orbit' : 'wasd');
  });
  /* Round 18ba: and the webview's own menu never opens. Robin: "When I right click on
     something that doesn't consume my right click for use in the editor, I get a webview
     menu that has some options like Back, and Update etc. I never want this menu to
     appear." The viewport, the swatches and the cover buttons already swallow theirs; this
     is the same swallow for the rest of the page, on the document so it holds for every
     panel built later. The one exception is a text field: its menu is the edit menu -
     cut, copy, paste - not the page's, and it has no Back or Reload in it. */
  document.addEventListener('contextmenu',e=>{
    const t=e.target;
    if(t && t.nodeType===1 && (t.isContentEditable || /^(INPUT|TEXTAREA)$/.test(t.tagName))) return;
    e.preventDefault();
  });
  /* Round 18cz: the mouse's own back and forward buttons walk the cell visit history.
     Robin: "if I press mouse buttons for 'back' and 'forward' (usually buttons for use in
     a browser) while my cursor is hovering the viewport, or any of the buttons or windows
     within it, and I am in a real cell preview (any cell that shows the back and forward
     buttons), I want to go forward or backwards the same way as pressing the arrow
     buttons. Only when the cursor is in the viewport."

     Buttons 3 and 4, acted on at the release the way the arrows' own clicks are, anywhere
     inside `#vpwrap` - the picture, the statistics box, the toggles - in Real cell mode
     with no dialogue over the page and the mesh editor closed (`CellHistory.go` is what the
     arrows call, and it refuses on its own while a step is still loading). And nowhere in
     the window do the two buttons reach the webview: its own default is to walk *its*
     history, and a page that has none is one accidental press from a blank window.
     Swallowed on the mouse events, not the pointer events - cancelling `pointerdown`
     would also cancel the mouse events that follow it, and the release is what acts. */
  {
    const histDir=e=> e.button===3? -1 : (e.button===4? 1 : 0);
    const swallow=e=>{ if(histDir(e)) e.preventDefault(); };
    document.addEventListener('mousedown',swallow,true);
    document.addEventListener('mouseup',swallow,true);
    document.addEventListener('auxclick',swallow,true);
    document.addEventListener('mouseup',e=>{
      const dir=histDir(e); if(!dir) return;
      const w=$('#vpwrap'), t=e.target;
      if(!w || !t || t.nodeType!==1 || !w.contains(t)) return;
      if(App.mode!=='cell' || App.inspect) return;
      if(document.querySelector('.modal:not([hidden])')) return;
      if(typeof CellHistory==='object') CellHistory.go(dir);
    });
  }

  try{
    App.R=new Renderer($('#gl'));
    App.R.frame(2048);
    /* The renderer names its own colours (`legendItems`); this draws them. A rule swatch
       carries the rule's *tag* rather than a label, and turning a tag into the name you
       gave the rule is the page's job, so the renderer is handed a function for it rather
       than a copy of the config. */
    App.R.ruleName=tag=>{
      const list=(App.cfg&&App.cfg.selectors)||[];
      for(let i=0;i<list.length;i++) if(ruleTag(App.cfg,i)===tag) return selKey(list[i]);
      return tag;
    };
    App.R.onLegend=renderLegend;
    /* Round 18ay: the compass rose turns with the camera - both roses, the cell stack's
       and the mesh editor's. Round 18az: an SVG rotate on the rose's own group rather
       than a CSS transform on the element, so the rose is drawn from its paths at every
       angle instead of a 42 px bitmap being turned (which looked "a bit pixly").

       Round 18ba: eased. The CSS transition that went with the transform had softened
       every turn, and without it a Recentre - one frame from one azimuth to another -
       snapped the rose round; Robin: "make it rotate smoothly again". So the angle
       drawn chases the camera's: each frame it closes the gap by a fraction set from the
       frame's length, a time constant of about a tenth of a second - tight enough to
       track an orbit without visible lag, soft enough that a jump reads as a turn. The
       angle the renderer hands over is unwrapped (`_tellNorth`), so the chase never
       goes the long way round at ±180. */
    {
      const rose={target:0, shown:null, raf:0, last:0};
      const TAU=100;   // ms
      const write=deg=>{
        const t='rotate('+deg.toFixed(2)+' 22 22)';
        document.querySelectorAll('.compass .rose').forEach(g=>g.setAttribute('transform',t));
      };
      const step=now=>{
        rose.raf=0;
        const dt=rose.last? Math.min(100,now-rose.last) : 16;
        rose.last=now;
        const gap=rose.target-rose.shown;
        if(Math.abs(gap)<0.05){ rose.shown=rose.target; write(rose.shown); rose.last=0; return; }
        rose.shown+=gap*(1-Math.exp(-dt/TAU));
        write(rose.shown);
        rose.raf=requestAnimationFrame(step);
      };
      App.R.onNorth=deg=>{
        rose.target=deg;
        if(rose.shown==null){ rose.shown=deg; write(deg); return; }   // the first frame: no turn to ease
        if(!rose.raf) rose.raf=requestAnimationFrame(step);
      };
    }
  }catch(e){
    $('#noctx').hidden=false;
    $('#noctx').innerHTML='<div class="big">'+T('vp.noctx_title')+'</div><div class="sm">'+
      e.message+'<br><br>'+T('vp.noctx_body')+'</div>';
  }
  new ResizeObserver(()=>App.R&&App.R.resize()).observe($('#vpwrap'));




  bindCellMode();

  /* collapsible sections in the left column (built as static markup, so they
     need the handler sectionBox() gives the dynamic ones) */
  $$('#left .sect > .sh').forEach(h=>{
    h.onclick=()=>{ h.parentElement.classList.toggle('closed'); syncSectionBadges(); };
  });


  $('#p_fog').onchange=()=>{ if(App.R){ App.R.opts.fog=$('#p_fog').checked; App.R.dirty=true; } };
  /* Round 15 item 4: the fog density and the sunshafts only mean anything under a sky,
     so they fold away with the switch. */
  const skyPane=()=>{ const e=$('#skyPane'); if(e) e.hidden=!$('#p_sky').checked; };
  const applySky=()=>{ if(App.R){ App.R.opts.scatter=$('#p_sky').checked; App.R.dirty=true; } skyPane(); };
  $('#p_sky').onchange=applySky;
  skyPane();
  /* Round 17y: a room that is not a quasi-exterior has no sky, so the switch is put out of
     reach - shown off, dimmed, with the reason on hover - and your own setting is put aside
     on the way in and given back on the way out, the way "Paint on statics too" is forced
     the other way (`syncStaticsSwitch`). `App._skySwitchSaved` is what the profile saves
     meanwhile. Robin: "toggle Atmosphere off by default in such interiors and grey it out
     [...] save the toggle's value and restore it when exiting". */
  const syncAtmosphereSwitch=(indoors)=>{
    const el=$('#p_sky'); if(!el) return;
    const row=el.closest('.row');
    if(indoors){
      if(App._skySwitchSaved==null) App._skySwitchSaved=!!el.checked;
      el.checked=false; el.disabled=true;
      if(row){ row.classList.add('forced'); row.title=T('preview.sky_room'); }
    }else{
      if(App._skySwitchSaved!=null){ el.checked=App._skySwitchSaved; App._skySwitchSaved=null; }
      el.disabled=false;
      if(row){ row.classList.remove('forced'); row.title=''; }
    }
    applySky();
  };
  App.syncAtmosphereSwitch=syncAtmosphereSwitch;
  $('#p_mge').onchange=()=>{ if(App.R){ App.R.opts.mgeGrass=$('#p_mge').checked; App.R.dirty=true; } };
  /* Round 18dy: and the underwater row follows it. The effects are that water seen from
     the other side, so with MGE water off there is nothing for them to be - the row hides
     and the renderer stands them down (`underwaterAt`), whatever the switch remembers. */
  const syncUnderRow=()=>{ const r=$('#rowUnder'), w=$('#p_mgew'); if(r && w) r.hidden = !w.checked;
    for(const s of document.querySelectorAll('.row.wtint')) if(w) s.hidden = !w.checked; };
  App.syncUnderRow=syncUnderRow;
  $('#p_mgew').onchange=()=>{ if(App.R){ App.R.opts.mgeWater=$('#p_mgew').checked; App.R.dirty=true; } syncUnderRow(); };
  syncUnderRow();
  // Round 18a: the ambient occlusion (29_ssao.js), off unless asked for.
  { const e=$('#p_ssao'); if(e) e.onchange=()=>{ if(App.R){ App.R.opts.ssao=e.checked; App.R.dirty=true; } }; }
  // Round 18do: FXAA (33_fxaa.js), off unless asked for.
  { const e=$('#p_fxaa'); if(e) e.onchange=()=>{ if(App.R){ App.R.opts.fxaa=e.checked; App.R.dirty=true; } }; }
  /* Round 18dy: the depth of field (36_dof.js). The switch is the eye's own blur; the
     distance blur in fog runs under the water whatever it says, which is what Robin
     asked for and what the shader does on its own - its fog term is nought wherever the
     fog starts at a positive distance. */
  { const e=$('#p_dof'); if(e){ const apply=()=>{ if(App.R){ App.R.opts.dof=e.checked; App.R.dirty=true; } }; e.onchange=apply; apply(); } }
  { const e=$('#p_bloom'); if(e){ const apply=()=>{ if(App.R){ App.R.opts.bloom=e.checked; App.R.dirty=true; } }; e.onchange=apply; apply(); } }   // Wraithguard: 42_wg_bloom.js
/* Round 18ee: the sun's shadows. Applied at wiring time as well, because the switch may
   already carry an install's answer by the time the renderer exists. */
{ const e=$('#p_shadows'); if(e){ const apply=()=>{ if(App.R){ App.R.opts.shadows=e.checked; App.R.dirty=true; } }; e.onchange=apply; apply(); } }
{ const e=$('#p_shadowres'); if(e){ const apply=()=>{ if(App.R){ App.R.opts.shadowRes=+e.value||0; App.R.dirty=true; } }; e.onchange=apply; e.oninput=apply; apply(); } }
  // Round 18dt: the underwater effects, on unless switched off.
  { const e=$('#p_under'); if(e){ const apply=()=>{ if(App.R){ App.R.opts.underwater=e.checked; App.R.dirty=true; } }; e.onchange=apply; apply(); } }
  // Round 18f: the Unlit switch.
  { const e=$('#p_unlit'); if(e) e.onchange=()=>{ if(App.R){ App.R.opts.unlit=e.checked; App.R.dirty=true; } if(typeof PrevSettings==='object' && PrevSettings.touch) PrevSettings.touch(); }; }
  // Round 16: the sunshafts, inside the atmosphere pane - they need its sky.
  $('#p_shafts').onchange=()=>{ if(App.R){ App.R.opts.sunshafts=$('#p_shafts').checked; App.R.dirty=true; } };
  // Wraithguard: NPCs, creatures and spawn points - a change of what the scene holds.
  { const e=$('#p_actors'); if(e) e.onchange=()=>{ App.showActors=e.checked; if(typeof schedulePreview==='function') schedulePreview(); }; }
  { const e=$('#p_npcdrawn'); if(e) e.onchange=()=>{ App.npcDrawn=e.checked; if(typeof schedulePreview==='function') schedulePreview(); }; }
  // Round 17w: the meshes' particle systems, on by default.
  { const e=$('#p_particles');
    /* Round 18q: and the nocturnal moths, which are an effect on a lamp rather than a
       switch of their own - Robin: "Have them toggle on and off together with the
       Particles toggle instead of its own toggle." The weather rides along here too,
       since the moths are the only thing that reads it and the sky owns the word. */
    if(e){ const apply=()=>{ if(App.R){ App.R.opts.particles=e.checked;
                                        App.R.opts.weather=(typeof Sky==='object' && Sky.weather)||'Clear';
                                        App.R.dirty=true; } };
           e.onchange=apply; apply(); } }

  /* Round 18dr: the window's own buttons, and the bar as its title bar. Only inside the
     shell, where `window.__TAURI__.window` is there; a browser keeps the bar as it was.
     The inert parts of the bar - the separators and the spacer, which the markup could
     not mark - take the drag attribute here, since the shell starts a drag only from the
     element under the pointer. Double-clicking a draggable spot maximises (the shell's
     own rule); the maximise glyph follows the window's state. */
  (()=>{
    const W=window.__TAURI__ && window.__TAURI__.window;
    const bar=$('#topbar'), ctl=$('#winctl');
    if(!W || !W.getCurrentWindow || !bar || !ctl) return;
    const win=W.getCurrentWindow();
    for(const el of bar.querySelectorAll('.tbsep, :scope > div[style]')) el.setAttribute('data-tauri-drag-region','');
    ctl.hidden=false;
    const sep=$('#wcSep'); if(sep) sep.hidden=false;   // 18ds: the divider before them (a .tbsep, so it drags too)
    $('#wcMin').onclick=()=>{ win.minimize().catch(()=>{}); };
    $('#wcMax').onclick=()=>{ win.toggleMaximize().catch(()=>{}); };
    $('#wcClose').onclick=()=>{ win.close().catch(()=>{}); };
    const syncMax=()=>{ win.isMaximized().then(m=>bar.classList.toggle('max',!!m)).catch(()=>{}); };
    syncMax();
    if(win.onResized) win.onResized(syncMax).catch(()=>{});
  })();

  /* viewport toolbar */
  const tog=(id,key,fn)=>{
    $(id).onclick=()=>{
      const on=!$(id).classList.contains('on');
      $(id).classList.toggle('on',on);
      if(App.R){ if(key) App.R.opts[key]=on; App.R.dirty=true; }
      fn&&fn(on);
    };
  };
  /* Lighting: it changes nothing about the output, only how easy the output is to
     look at, so it lives under Preview.

     There used to be two Sun height sliders — one here, one in the export section —
     kept in step by each writing into the other. Two controls for one value is one
     control too many; `#p_sun` is the survivor because it is the id the profile and
     the tests already know. */
  const lightSlider=(id,valId,apply)=>{
    const e=$(id); if(!e) return;
    const paint=()=>{ $(valId).textContent=e.value; apply(+e.value); if(App.R) App.R.dirty=true; };
    e.oninput=paint; paint();
  };
  lightSlider('#pvBright','#pvBrightV',v=>{ if(App.R) App.R.opts.bright=v/100; });
  lightSlider('#p_tint','#p_tintV',v=>{ if(App.R) App.R.opts.tint=v/100; });
  lightSlider('#p_wind','#p_windV',v=>{ if(App.R) App.R.opts.wind=v/100; });
  /* Round 17v: grass distance, said in cells and kept in world units — a cell is 8192,
     and "three cells" is a thing to picture where "24576" is not. Zero is off, and the
     value says so rather than showing a nought that looks like a setting of nothing. */
  {
    const e=$('#p_grassfar'), v=$('#p_grassfarV');
    if(e&&v){
      const paint=()=>{
        const n=+e.value||0;
        v.textContent = n>0? T('preview.grassfar_cells',{n}) : T('preview.grassfar_off');
        if(App.R){ App.R.opts.grassFar=n*8192; App.R.dirty=true; }
      };
      e.oninput=paint; paint();
    }
  }
  lightSlider('#p_fogd','#p_fogdV',v=>{ if(App.R) App.R.opts.fogDensity=v/100; });   // round 15 item 3
  // Wraithguard: the colour over the water (26_water.js) - its hue, and 0..1 of it.
  lightSlider('#p_whue','#p_whueV',v=>{ if(App.R) App.R.opts.waterHue=v; });
  lightSlider('#p_wtint','#p_wtintV',v=>{ if(App.R) App.R.opts.waterTint=v/100; });
  { const e=$('#p_sewers'); if(e) e.onchange=()=>{ if(App.R){ App.R.opts.sewerWaves=e.checked; App.R.dirty=true; } }; }  // Wraithguard: MGE XE's water settings, driven here (26_water.js, 35_underwater.js).
  { const e=$('#p_waves'); if(e){ const f=()=>{ if(App.R){ App.R.opts.waves=e.checked; App.R.dirty=true; } }; e.onchange=f; f(); } }
  lightSlider('#p_wheight','#p_wheightV',v=>{ if(App.R) App.R.opts.waveHeight=v; });
  lightSlider('#p_caust','#p_caustV',v=>{ if(App.R) App.R.opts.caustics=v; });
  { const e=$('#p_weather'); if(e){ const f=()=>{ if(typeof Sky==='object'){ Sky.weather=e.value||'Clear'; } if(App.R){ App.R.opts.weather=e.value||'Clear'; App.R.dirty=true; } if(typeof Sky==='object' && Sky.showSources) Sky.showSources(); }; e.onchange=f; e.oninput=f; f(); } }   // Wraithguard: the weathers
  { const e=$('#p_wdepth'); if(e){ const f=()=>{ if(App.R){ App.R.opts.waterDepth=e.checked; App.R.dirty=true; } }; e.onchange=f; f(); } }   // Wonders of Water
  { const e=$('#p_wblur'); if(e){ const f=()=>{ if(App.R){ App.R.opts.reflBlur=e.checked; App.R.dirty=true; } }; e.onchange=f; f(); } }
  /* Round 17h, Robin: "Add a reset fog to install density button by the fog density
     slider, that sets the fog to what the install says." The slider is a multiplier on
     the distances the install gives, so the install's own answer is 100; the button puts
     it back there and says, on the sources line, which file those distances came from. */
  { const b=$('#p_fogdReset');
    if(b) b.onclick=()=>{
      const f=(typeof Sky==='object')? Sky.installFog() : {density:100};
      const sl=$('#p_fogd'); if(sl){ sl.value=String(f.density); sl.dispatchEvent(new Event('input')); }
      const el=$('#p_skySrc');
      if(el && typeof Sky==='object'){
        const cells=f.cells? f.cells.map(v=>v.toFixed(1)).join('–') : '';
        el.textContent=Sky.sources()+(f.source? ' · '+T('preview.fogd_from')+' '+f.source+
          ' ('+f.start+'–'+f.end+' cells × '+f.ratio+(f.offset? ', offset '+f.offset : '')+
          (cells? ' → '+cells+' cells' : '')+')' : '');
        /* Round 17m: hovering says *why* that file — which d3d8.dll was asked, and where
           it was found. A detector nobody can question is one nobody can correct. */
        el.title=(f.how? T('preview.sky_src_mge_how',{how:f.how})+(f.path? '\n'+f.path : '')+'\n' : '')+Sky.rendererNote();
      }
    };
  }
  /* The clock (round 13): the label reads as a time, and the sun and the sky follow.
   *
   * Round 17s: the *hour* is `App.hour`, not the slider's value. The slider is a control
   * with a step on it, and a step rounds — which is why "Time passes" did nothing at all
   * when it first shipped: the loop added twenty game-seconds to the hour, wrote it back
   * to the slider, the slider snapped it to the nearest step, and the next frame read the
   * same number it had started with. The clock now advances a float and the slider is
   * shown it; a control that quantises can display a value it must not also own. */
  { const e=$('#p_hour');
    App.hour=+e.value;
    const apply=()=>{ $('#p_hourV').textContent=Sky.clock(App.hour);
                      if(App.R){ App.R.opts.hour=App.hour; App.R.dirty=true; }
                      /* Wraithguard: night falls or breaks - the NPCs take out or put away
                         their torches, which is a different NPC to the engine. */
                      const night=typeof npcNightHour==='function' && npcNightHour();
                      if(App._npcNight!=null && night!==App._npcNight && App.showActors!==false && typeof schedulePreview==='function') schedulePreview();
                      App._npcNight=night; };
    /* Rounded to the five minutes the slider steps in. `step="0.0833333"` is an inexact
       twelfth, so the browser snaps 17:30 to 17.499993 and a profile would remember that
       instead of half past five. The step is what the thumb moves in; this is what the
       hour *is*. Round 17s. */
    e.oninput=()=>{ App.hour=Math.round(+e.value*12)/12; apply(); };
    App.setHour=h=>{ App.hour=((h%24)+24)%24; e.value=String(App.hour); apply(); };
    apply();
  }
  /* Round 17s: the clock, running. Robin asked for a switch that "moves the time forward
     smoothly in the world, so that we can see the change in real time", with a speed
     beside it.

     Off by default. The loop stops itself when the switch goes off rather than running a
     no-op every frame, and it asks for a redraw each tick because nothing else would —
     the viewport only draws when something says it is dirty.

     And it holds while the slider is held. Robin: "make it pause as long as I hold the
     slider for time of day, and resume again when I release it." Dragging a slider whose
     value is being written underneath you is a fight; the pointer capture is the whole
     rule. */
  { const sw=$('#p_flow'), k=$('#p_flowk'), row=$('#flowRow'), kv=$('#p_flowkV'),
          hour=$('#p_hour');
    let last=0, raf=0, held=false;
    const step=now=>{
      if(!sw.checked){ raf=0; return; }
      /* A frame gap over a quarter second is a stall, not elapsed time: coming back to a
         backgrounded tab should not fast-forward a day. */
      const dt=(held||!last)? 0 : Math.min(0.25,(now-last)/1000);
      last=now;
      // The slider beside it is game *minutes* a real second.
      if(dt>0) App.setHour(App.hour + dt*(+k.value)/60);
      raf=requestAnimationFrame(step);
    };
    /* Round 17y item 10: the clouds read the scale too — twenty is the game's pace, and a
       faster clock hurries them — whether or not the clock is running, so the viewport is
       told the number on its own rather than only through the ticking hour. */
    const pace=()=>{ if(App.R){ App.R.opts.timeScale=+k.value||20; App.R.dirty=true; } };
    App.syncTimeScale=pace;
    const sync=()=>{
      if(row) row.hidden=!sw.checked;
      if(kv) kv.textContent=k.value;
      pace();
      if(sw.checked && !raf){ last=0; raf=requestAnimationFrame(step); }
      if(!sw.checked && raf){ cancelAnimationFrame(raf); raf=0; }
    };
    sw.onchange=sync; k.oninput=()=>{ if(kv) kv.textContent=k.value; pace(); };
    /* Held by pointer or by keyboard — a slider takes arrow keys too, and holding one
       down is the same act. `last=0` on release so the pause is not paid back as a jump. */
    const grab=()=>{ held=true; };
    const drop=()=>{ held=false; last=0; };
    hour.addEventListener('pointerdown',grab);
    hour.addEventListener('pointerup',drop);
    hour.addEventListener('pointercancel',drop);
    hour.addEventListener('keydown',grab);
    hour.addEventListener('keyup',drop);
    hour.addEventListener('blur',drop);
    window.addEventListener('pointerup',drop);
    sync();
  }
  /* Round 17s: whether the cell's lamps burn at all. The hours are Robin's own rule and
     stay the default; the other two are "whatever the clock says, they are lit" and
     "there are no lamps". `lightsLit` in 06_gl.js is where it is read. */
  { const e=$('#p_lights');
    const paint=()=>{ if(App.R){ App.R.opts.lights=e.value; App.R.dirty=true; } };
    /* Both events: a person changes a picker with `change`, and `setEl` (19_settings.js)
       restores one by dispatching `input`. Binding one of them means the profile that
       remembers this setting does not apply it. */
    e.onchange=e.oninput=paint; paint(); }

  tog('#tbGrid','grid'); tog('#tbEmpty','markers');
  /* The red wash over ground the slope and height limits are rejecting. It has always
     been drawn and there has never been a way to stop it, which is a problem when what
     you want to look at is the ground itself — the overlay is the loudest thing in the
     picture on a hillside. `showSlope` was already the uniform; this is the switch it
     never had. */
  /* Three states now, not two. Robin: "sometimes it's distracting when I don't care
     about statics" — so the shade on the rocks is the third click, not part of the
     second. The label says which state you are in; off is the plain name. */
  const cullState=()=>{ const o=(App.R&&App.R.opts)||{};
                        return !o.showSlope? 0 : (o.showSlopeStatics? 2 : 1); };
  App.syncCullButton=()=>{
    const b=$('#tbCull'); if(!b) return;
    const st=cullState();
    b.classList.toggle('on',st>0);
    b.textContent= st===2? T('vp.cull_both') : st===1? T('vp.cull_terrain') : T('vp.cull_off');
  };
  $('#tbCull').onclick=()=>{
    if(!App.R) return;
    const next=(cullState()+1)%3;
    App.R.opts.showSlope=next>0;
    App.R.opts.showSlopeStatics=next===2;
    App.R.dirty=true;
    App.syncCullButton();
    if(typeof PrevSettings==='object') PrevSettings.touch();
  };
  App.syncCullButton();
  /* The stepping arrows. A viewport switch like the rest: they are drawn on top of
     everything and there are moments — a screenshot, a close look at a corner — when
     they are in the way. Off also stops them being pickable, or the cursor changes over
     a control nobody can see. */
  tog('#tbArrows','arrows',on=>{ if(!on && App.R) App.R.setArrowHover(null); });
  // The highlights are off-by-choice rather than off-by-default: they cost nothing
  // until the pointer is over something, and clearing them on toggle-off matters
  // more than the toggle itself, or a stale glow outlives the hover.
  tog('#tbHiTex',null,on=>{ App.hiTex=on; if(!on&&App.R) App.R.setTexHighlight(null); });
  tog('#tbHiSlot',null,on=>{ App.hiSlot=on; if(!on&&App.R) App.R.setGrassHighlight(null); });
  /* One switch for both ways of pointing at an object: the pointer in the viewport, and
     a never-avoid pattern in the left column. They are the same question — which objects
     is this about — so they are the same answer and the same button. */
  /* Round 18dq: three states, like Highlight culling - off, the gold fill, the outline
     (34_outline.js). Robin: "add it to the toggle for 'Highlight objects' in the
     viewport upper right corner (similar to how we toggle through Highlight culling)". */
  App.hiObjMode=App.hiObjMode||'fill';
  App.syncHiObjButton=()=>{
    const b=$('#tbHiObj'); if(!b) return;
    const on=!!App.hiObj;
    b.classList.toggle('on',on);
    b.textContent= on && App.hiObjMode==='outline'? T('vp.hiobj_outline') : T('vp.hiobj');
    if(App.R) App.R.hlMode=App.hiObjMode;
  };
  $('#tbHiObj').onclick=()=>{
    const st=!App.hiObj? 0 : App.hiObjMode==='outline'? 2 : 1;
    const next=(st+1)%3;
    App.hiObj=next>0; App.hiObjMode= next===2? 'outline' : 'fill';
    if(App.R){ App.R.hlMode=App.hiObjMode; if(!App.hiObj) App.R.setStaticHighlight(null); App.R.dirty=true; }
    App.syncHiObjButton();
    if(typeof PrevSettings==='object') PrevSettings.touch();
  };
  App.syncHiObjButton();
  $('#tbRecentre').onclick=()=>recentreView(true);

  // Clicking an object opens ORI, the object inspector (24_ori.js); Shift+click on a door
  // goes through to the cell it leads to.
  if(App.R) App.R.onPick=(hit,e)=>Ori.pick(hit,e);
  /* Round 18r: the shell moves the mouse pointer for the WASD look, and the renderer
     asks through this rather than reaching for `Engine` itself - the same shape as
     `onPick` and `onFlyState` below. A browser has no shell to ask and leaves it unset,
     which is exactly what `_park` treats as "nothing to do". */
  if(App.R && Engine.has()) App.R.onCursor=(show,x,y)=>
    Engine.call('cursor_park',{show, x, y}).catch(()=>{});
  /* The same pick, one button earlier. Right-clicking an object is easier when you can
     see which one you are about to get — that is the whole reason this exists. */
  if(App.R) App.R.onHoverPick=hit=>{
    /* Never while painting — a general rule, not a toggle to keep resetting. The right
       button is a brush there, so a highlight promising a pick would be a lie, and
       Robin found himself switching it off by hand every session. */
    if(App.paintMode) return;
    /* And never in the mesh editor: the whole mesh is the only thing under the pointer,
       and lighting all of it on every hover would shout over the texture panel's own
       per-face highlight. */
    if(App.inspect) return;
    // Wraithguard: a highlighted mod owns the highlight while it is on (40_wg_modhl.js).
    if(typeof WgModHl==='object' && WgModHl.active()) return;
    if(!App.hiObj) return;
    App.R.setStaticHighlight(hit? [hit] : null);
  };

  /** Reads the visible geometry of whatever is standing in the cells on screen.
   *
   *  The chicken and the egg: a mesh carries paint only after you have painted it, and
   *  the brush cannot land on one whose triangles have not been read. So turning the
   *  switch on — and moving to another cell while it is on — is what asks for them.
   *
   *  Says how long it took only when it took long enough to notice. A dense cell is a
   *  couple of hundred meshes and the first one costs a moment; the second costs nothing,
   *  because nothing is read twice. */
  /** The interior on screen, when the scene is a room rather than cells. */
  const sceneRoom=()=>{
    const cells=(App._scene&&App._scene.cells)||[];
    return cells.find(c=>c&&c.kind==='int')||null;
  };
  App.sceneRoom=sceneRoom;





  /* The mouse line, worked out from the state rather than stashed.
   *
   * Three wordings, three situations, and the right button is what tells them apart:
   * it is a brush while painting, it picks a placed object in a Real cell, and it picks
   * nothing at all in the mesh editor or on the Simplified patch — the editor holds one
   * mesh rather than a placement, and the patch is invented ground with nothing standing
   * on it. Robin: "when in the Mesh editor mode, or simplified preview mode, remove the
   * 'Right click object' message".
   *
   * This used to swap two strings and keep the one it replaced in `App._vphintRest`, so
   * the line said whatever it had last been rather than what is true now — a stashed
   * string is a second copy of the state, and it went stale the moment a third situation
   * existed. Nothing is remembered here: every wording is read from its own catalogued
   * template each time. */
  const syncVpHint=()=>{
    const vh=$('#vphint'); if(!vh) return;
    // Round 17: the WASD scheme has its own three, and a fourth while the button is held.
    const wasd=App.R && App.R.nav==='wasd';
    const src = wasd
      ? (App._flying ? $('#vphintWasdFly')
         : App.paintMode ? $('#vphintWasdPaint')
         : (App.inspect || App.mode!=='cell') ? $('#vphintWasdPlain')
         : $('#vphintWasd'))
      : (App.paintMode ? $('#vphintPaint')
         : (App.inspect || App.mode!=='cell') ? $('#vphintPlain')
         : $('#vphintPick'));
    if(src) vh.textContent=src.textContent;
  };
  App.syncVpHint=syncVpHint;
  /* Round 17: the navigation picker and the speed bar. The bar's position is a log
     scale over 50..50000 units a second, so a notch of the wheel is the same step
     anywhere along it. */
  const SP_MIN=50, SP_MAX=50000;
  // Round 18ag: the sensitivity bar is the same shape over 0.25..4, with 1 in the middle.
  const SE_MIN=0.25, SE_MAX=4;
  const sensPos=v=>100*Math.log(Math.max(SE_MIN,Math.min(SE_MAX,v))/SE_MIN)/Math.log(SE_MAX/SE_MIN);
  const sensOf=pos=>SE_MIN*Math.pow(SE_MAX/SE_MIN,Math.max(0,Math.min(100,+pos||0))/100);
  const fmtSens=v=>Math.round(v*100)+'%';
  App.sensOf=sensOf; App.sensPos=sensPos;
  const speedPos=v=>100*Math.log(Math.max(SP_MIN,Math.min(SP_MAX,v))/SP_MIN)/Math.log(SP_MAX/SP_MIN);
  const speedOf=pos=>SP_MIN*Math.pow(SP_MAX/SP_MIN,Math.max(0,Math.min(100,+pos||0))/100);
  const fmtSpeed=v=>v>=10000? Math.round(v/100)*100 : v>=1000? Math.round(v/10)*10 : Math.round(v);
  let popTimer=null;
  const showSpeed=(v,announce)=>{
    const val=$('#vpSpeedV'), bar=$('#vpSpeedBar'), pop=$('#vpSpeedPop');
    if(val) val.textContent=String(fmtSpeed(v));
    if(bar && document.activeElement!==bar) bar.value=String(Math.round(speedPos(v)));
    if(announce && pop){
      pop.textContent=fmtSpeed(v)+' u/s'; pop.hidden=false;
      clearTimeout(popTimer); popTimer=setTimeout(()=>{ pop.hidden=true; },900);
    }
  };
  const syncNav=()=>{
    const wasd=App.R && App.R.nav==='wasd';
    /* Round 17h: the button at the top of the viewport's toggles says which scheme is
       on, and is lit like every other "this is on" toggle. */
    { const nb=$('#tbNav');
      if(nb){ nb.textContent = wasd? T('vp.nav_wasd_text') : T('vp.nav_orbit');
              nb.classList.toggle('orbit',!wasd); nb.classList.toggle('wasd',wasd); nb.classList.remove('on'); }
    }
    const sp=$('#vpSpeed'); if(sp) sp.hidden=!wasd;
    const pick=$('#setNav'); if(pick) pick.value=wasd? 'wasd' : 'orbit';
    /* Round 18ag: the sensitivity, shown as a percentage of the built-in rate, with the
       bar's middle at 100%. The bar is a log scale over a quarter to four times, so the
       left half and the right half are the same size in feel - halving at 25, doubling
       at 75 - which a straight 0.25..4 would not give (the middle would sit at 212%). */
    const se=$('#setSens'); if(se && App.R){ se.value=String(Math.round(sensPos(App.R.lookSensitivity()))); const ev=$('#setSensV'); if(ev) ev.textContent=fmtSens(App.R.lookSensitivity()); }
    const seRow=$('#setSensRow'); if(seRow) seRow.hidden=!wasd;
    // Round 17c: the smoothing is the WASD look's alone; its row only shows for that scheme.
    const sm=$('#setSmooth'); if(sm && App.R){ const v=Math.round(Math.max(1,Math.min(100,+App.R.lookSmooth||35))); sm.value=String(v); const sv=$('#setSmoothV'); if(sv) sv.textContent=String(v); }
    const smRow=$('#setSmoothRow'); if(smRow) smRow.hidden=!wasd;
    const mm=$('#setMoveSmooth'); if(mm && App.R){ const v=Math.round(Math.max(1,Math.min(100,+App.R.moveSmooth||12))); mm.value=String(v); const mv=$('#setMoveSmoothV'); if(mv) mv.textContent=String(v); }
    const mmRow=$('#setMoveSmoothRow'); if(mmRow) mmRow.hidden=!wasd;
    /* Round 17f: the orbit camera's own position smoothing, shown for that scheme only -
       so exactly one "Camera position smoothing" row is up, whichever scheme is chosen. */
    const om=$('#setOrbitSmooth'); if(om && App.R){ const v=Math.round(Math.max(1,Math.min(100,+App.R.orbitSmooth||40))); om.value=String(v); const ov=$('#setOrbitSmoothV'); if(ov) ov.textContent=String(v); }
    const omRow=$('#setOrbitSmoothRow'); if(omRow) omRow.hidden=wasd;
    // Round 17g: and its rotation smoothing, the same way.
    const orr=$('#setOrbitSmoothRot'); if(orr && App.R){ const v=Math.round(Math.max(1,Math.min(100,+App.R.orbitLookSmooth||30))); orr.value=String(v); const rv=$('#setOrbitSmoothRotV'); if(rv) rv.textContent=String(v); }
    const orrRow=$('#setOrbitSmoothRotRow'); if(orrRow) orrRow.hidden=wasd;
    if(App.R) showSpeed(App.R.fly.speed,false);
    syncVpHint();
  };
  App.syncNav=syncNav;

  /* Round 18aq: after a language pack goes on or comes off (`Text.apply`), the things
     drawn from script that are cheap to draw again. Not everything — a dialogue that is
     open keeps its words until it is opened again, the statistics box until the next
     rebuild — and the picker's note says so. Each guarded: a renderer that is not there
     yet (this runs from `applyNative` at start-up too) is simply skipped. */
  App.reword=()=>{
    const run=f=>{ try{ if(typeof f==='function') f(); }catch(e){ console.warn('reword:',e); } };
    run(typeof syncCellButton==='function'? syncCellButton : null);
    run(syncVpHint); run(syncNav);
    run(typeof Profiles==='object' && Profiles.showName? ()=>Profiles.showName() : null);
    run(typeof Settings==='object' && Settings.show? ()=>Settings.show() : null);
    run(typeof Sky==='object' && Sky.showSources? ()=>Sky.showSources() : null);
    run(typeof Lang==='object' && Lang.fill? ()=>Lang.fill() : null);
    // Round 18cg: the theme names are catalogued strings, and the picker holds five of them.
    run(typeof Themes==='object' && Themes.fill? ()=>Themes.fill() : null);
    run(App.syncCullButton);
    run(()=>{ if(App.installKind) markInstall(App.installKind); });
    run(()=>{ const f=$('#folderState'); if(f && f.title) f.title=T('top.folder_report_title'); });
    if(App.R && typeof App.R._tellLegend==='function'){ App.R._legendSig=null; run(()=>App.R._tellLegend()); }
  };
  if(App.R){
    /* Round 18at: the scheme and the fly speed are profile settings too, and two of the
       ways they change are not a control in the dialogue — the viewport's own toggle
       button and the wheel while flying — so the save is asked for here, where every
       change arrives. `touch` is debounced and muted while a profile is being adopted. */
    App.R.onNavChange=()=>{ syncNav(); if(typeof PrevSettings==='object') PrevSettings.touch(); };
    App.R.onSpeedChange=(v,announce)=>{ showSpeed(v,announce); if(announce && typeof PrevSettings==='object') PrevSettings.touch(); };
    App.R.onFlyState=on=>{ App._flying=on;
      // Round 17h: the green border while the right button is held in the WASD scheme.
      const w=$('#vpwrap'); if(w) w.classList.toggle('flying',!!on);
      syncVpHint(); };
  }
  // Round 17b: the scheme and the smoothing live in the Settings dialogue.
  const navPick=$('#setNav');
  if(navPick) navPick.onchange=()=>{ if(App.R) App.R.setNav(navPick.value); };
  { const nb=$('#tbNav');
    if(nb) nb.onclick=()=>{ if(App.R) App.R.setNav(App.R.nav==='wasd'? 'orbit' : 'wasd'); }; }
  const sens=$('#setSens');
  if(sens) sens.oninput=()=>{
    /* Snapped to the middle within a step either side: the bar's whole point is that
       the middle is "as it was", and a thumb that lands at 49 reads 98% and is not. */
    const pos=+sens.value; const v= Math.abs(pos-50)<=1 ? 1 : sensOf(pos);
    const ev=$('#setSensV'); if(ev) ev.textContent=fmtSens(v);
    if(App.R) App.R.lookSens=v;
  };
  const smooth=$('#setSmooth');
  if(smooth) smooth.oninput=()=>{ const v=Math.max(1,Math.min(100,+smooth.value||35)); const sv=$('#setSmoothV'); if(sv) sv.textContent=String(v); if(App.R) App.R.lookSmooth=v; };
  const moveSm=$('#setMoveSmooth');
  if(moveSm) moveSm.oninput=()=>{ const v=Math.max(1,Math.min(100,+moveSm.value||12)); const mv=$('#setMoveSmoothV'); if(mv) mv.textContent=String(v); if(App.R) App.R.moveSmooth=v; };
  const orbSm=$('#setOrbitSmooth');
  if(orbSm) orbSm.oninput=()=>{ const v=Math.max(1,Math.min(100,+orbSm.value||40)); const ov=$('#setOrbitSmoothV'); if(ov) ov.textContent=String(v); if(App.R) App.R.orbitSmooth=v; };
  const orbRot=$('#setOrbitSmoothRot');
  if(orbRot) orbRot.oninput=()=>{ const v=Math.max(1,Math.min(100,+orbRot.value||30)); const rv=$('#setOrbitSmoothRotV'); if(rv) rv.textContent=String(v); if(App.R) App.R.orbitLookSmooth=v; };
  const spBar=$('#vpSpeedBar');
  if(spBar) spBar.oninput=()=>{ if(App.R) App.R.setFlySpeed(speedOf(spBar.value),false); };
  syncNav();






























// 18eb: gone from the bar

  $('#mLogX').onclick=$('#mLogX2').onclick=()=>$('#mLog').hidden=true;
  // Round 17v: the whole report, read back off the dialogue at the moment it is asked for.
  { const cp=$('#mLogCopy');
    if(cp) cp.onclick=()=>logCopyWhenReady($('#logBody'),()=>logAllText($('#logBody')),cp); }
  /* Round 18cy: the statistics box no longer opens the report on a click - the Report
     button beside it does, and the box carries the visit history's arrows and the star
     now (Robin: "I don't want clicking it to bring up the report"). */
  $('#btnReport').onclick=openLog;
  $('#folderState').onclick=openLog;
  $('#folderState').style.cursor='pointer';
  $('#folderState').title=T('top.folder_report_title');

  document.addEventListener('keydown',e=>{
    if(e.key==='Escape'){ $('#mLog').hidden=true;
                          const mc=$('#mCell'); if(mc) mc.hidden=true; }
  });
  /* Round 18cv: the cell picker on the § key - `Backquote`, the key under Escape on a
     Nordic layout and a US one alike, so it is bound by its place on the keyboard rather
     than by what it prints. Robin: "Open the preview Cell picker with § key [...] If I
     choose a cell, change to cell preview and load that scene. If I cancel out again from
     the picker without choosing a cell, just take me back to wherever I was before."
     Pressed again while the picker is up, it closes it - that is the cancel. Not while
     typing in a field, and not with a modifier held.
     Round 18dj: "a field" means a field that takes text. 18cv stood aside for every
     input, and a slider, a checkbox, a select or a number box keeps the focus after a
     click - so the key went dead for as long as the last thing touched was one of those.
     Robin: "No matter what element in the program I have focused except for text fields
     (where one might want to write a §), I want clicking the § to open the cell picker."
     `takesText` is the test: a text-like input, a textarea, or anything editable. */
  const takesText=(t)=>{
    if(!t || t.nodeType!==1) return false;
    if(t.isContentEditable || t.tagName==='TEXTAREA') return true;
    if(t.tagName!=='INPUT') return false;
    const ty=(t.type||'text').toLowerCase();
    return !/^(range|checkbox|radio|button|submit|reset|color|file|image)$/.test(ty);
  };
  document.addEventListener('keydown',e=>{
    if(e.code!=='Backquote' || e.ctrlKey || e.altKey || e.metaKey) return;
    const t=e.target;
    if(takesText(t)){
      // The picker's own filter field is the one exception: § there closes the picker.
      if(t.id!=='cellFilter') return;
    }
    const mc=$('#mCell');
    if(mc && !mc.hidden){ e.preventDefault(); mc.hidden=true; return; }
    if(document.querySelector('.modal:not([hidden])')) return;   // another dialogue is up; leave it be
    e.preventDefault();
    if(typeof openCellPicker==='function') openCellPicker();
  });

  refreshAll();
}

/* =====================================================================================
   Install connection — one at a time, highlighted, confirmed before switching
   ===================================================================================== */
const INSTALLS={
  /* `tx` is where the idle label comes from — the wording catalogue, which is the one
     place that knows what this button says in whatever language the file is written in.
     It used to be cached off the button at start-up (`gateInstallButtons`), and a cache
     taken before `Text.apply` runs is a cache of the *English* label: connecting anything
     then wrote that stale copy back over every other button, quietly undoing a
     translation. Round 18z found it while giving these buttons their glyphs. */
  vanilla:{btn:'#btnFolder', tx:'top.connect_folder', short:'Data Files'},
  mo2    :{btn:'#btnMO2',    tx:'top.connect_mo2',    short:'MO2'},
  openmw :{btn:'#btnOMW',    tx:'top.connect_omw',    short:'OpenMW'}
};

/* Paint the top bar so it is obvious which install is live. */
function markInstall(kind,label,title){
  App.installKind=kind||null;
  for(const k in INSTALLS){
    const b=$(INSTALLS[k].btn); if(!b) continue;
    const live=(k===kind);
    b.classList.toggle('live',live);
    /* Round 18z: the words only. The button holds a drawn mark beside its label now, and
       writing `textContent` over the button would take the mark with it — which is exactly
       what happened the first time these glyphs went in. The label span is what changes;
       the glyph is not the label's to touch. */
    const lbl=b.querySelector('.lbl')||b;
    /* The label the button ships with, not the one it is wearing: `T` answers from the
       snapshot taken at start-up, so a button already shortened to "MO2" still knows it
       says "Connect MO2…" when the install changes. */
    const idle=(typeof T==='function' && T(INSTALLS[k].tx)) || lbl.textContent;
    if(live){
      /* "Connect Data Files…" becomes "Data Files" once it is connected: the verb has
         happened, and the button now says what is live. The short name, not the label
         with its verb cut off — the same three words in every language (round 18aq),
         since a translated verb is not "Connect" any more. */
      lbl.textContent=INSTALLS[k].short;
      b.title=T('top.connected_title',{kind:INSTALLS[k].short});
    }else{
      lbl.textContent=idle;
      b.title='';
    }
  }
  const fs=$('#folderState');
  if(fs && label!=null){
    fs.classList.add('ok');
    const rb=$('#btnReport'); if(rb) rb.hidden=false;
    // Wraithguard: the Editor mode's switch, beside it (50_wg_editor.js).
    if(typeof WgEditor==='object') WgEditor.button();
    fs.querySelector('.p').textContent=label;
    if(title) fs.querySelector('.p').title=title;
  }
}

/* Throw away everything tied to the install that is being replaced.
 *
 * The grass rules are **not** tied to it and stay exactly where they are. This used to
 * reset the config as well, from back when opening a config and connecting an install
 * were one gesture. They are not: rules describe grass, an install is where you look at
 * it, and somebody swapping from their MO2 setup to a plain Data Files folder to check
 * something has not asked to throw their work away. The rules are a saved set now, so
 * there is nothing to lose by keeping them and everything to lose by not. */
/* Round 18cn: one slider, two homes. Robin: "Move the Grass distance slider to sit beneath
   the Patch size in the simplified preview, and beneath the texture detail in the Full cell
   preview." Two rows was the other way to read that and it is the wrong one - there is one
   range input, one id, one value and one `oninput`, and a second copy would be a second
   thing to keep in step in exchange for nothing. So the row itself moves: `appendChild` of
   a node that already has a parent *moves* it rather than copying it, the input keeps its
   value across the move because it is the same input, and nothing is rebound.

   Called on the same lines that hide and show the panes, so the row cannot end up parked in
   a pane nobody can see. Idempotent - it checks where the row already is - because those
   lines run on every mode change and on every install switch. */
function placeGrassFar(cell){
  const row=$('#grassfarRow');
  const slot=$(cell? '#grassfarSlotCell' : '#grassfarSlotSimple');
  if(row && slot && row.parentNode!==slot) slot.appendChild(row);
}
App.placeGrassFar=placeGrassFar;

function unloadForSwitch(){
  dropMeshes(); dropTextures(); App.loadLog=[]; App.connectLog=[]; App.lastToml='';
  App.gitd=undefined;   // round 17x: the window mod's settings belong to the install
  if(App.R && App.R.dropWaterVolume) App.R.dropWaterVolume();
  App.texOn={}; App.cellSel=null;
  const ms=$('#pvMode');                     // a cell preview cannot survive the switch
  if(ms){ ms.value='cell'; App.mode='cell';
          const cp=$('#cellPane'); if(cp) cp.hidden=true;
          const cr=$('#cellRow'); if(cr) cr.hidden=true;
          }
  if(typeof GameData!=='undefined'){ GameData.newWorld(); GameData.clear(); }
  if(typeof CellData!=='undefined' && CellData.clear) CellData.clear();
  // Round 18cx: the picker's map is the install's; the connect that follows reads the next one.
  if(typeof CellMap==='object' && CellMap.drop) CellMap.drop();
  // Round 18cy: and so are the cells visited - another install's cells are another world's.
  if(typeof CellHistory==='object' && CellHistory.reset) CellHistory.reset();
  if(typeof setCellStatus==='function') setCellStatus(T('cell.status_pick'));
  // Through `syncCellButton` rather than by writing the text, so there is one place that
  // decides what the button says.
  if(typeof syncCellButton==='function') syncCellButton();
  const ctl=$('#cellTexList'); if(ctl) ctl.innerHTML='';
  if(App.R){ App.R.disposeStatics&&App.R.disposeStatics(); App.R.setStatics&&App.R.setStatics([]);
             App.R.setCellTerrain&&App.R.setCellTerrain([]);
             App.R.setWater&&App.R.setWater(null);
             /* The paint goes with the world it was painted on. It is keyed by grid
                position, and grid 30,30 in one install is not grid 30,30 in another. */
             App.R.clearCellPaint&&App.R.clearCellPaint();
             App.R.setArrows&&App.R.setArrows(null); }
  // Whatever clears the renderer clears the cached scene with it.
  App._scene=null;
  refreshAll();
}






























/* =====================================================================================
   Getting the report out of the window (round 17v).

   Robin: "add a copy to clipboard button to the report page that copies the entire
   report, plus one copy to clipboard button next to each individual section that only
   copies that section." Every performance round so far has ended with him selecting this
   dialogue by hand and pasting it into a message, and the sections are long enough that
   selecting one of them is fiddly.

   This is deliberately a pass over the finished report rather than a rebuild of it.
   `openLog` appends plain elements in order and knows nothing about sections; teaching it
   to would mean touching all five, and every one of them would then have two ways to be
   wrong. Reading the DOM back has one: a header is a header because it is styled like
   one, which is exactly what the eye uses too.

   The text is rebuilt from the elements rather than taken from `innerText`, because the
   dialogue is `hidden` when this runs and a hidden subtree has no layout — `innerText`
   falls back to `textContent` and the load-order rows come out as "#1Morrowind.esm3 ltex".
   ===================================================================================== */

/** A report element as one line of text. Flex rows (the load-order list) are their
    columns, spaced; everything else is its own words. */
function logRowText(el){
  // Round 18cu: the copy buttons sit inside the headings now that the text is read at
  // click time, and a button's label is not part of the report.
  const kids=Array.from(el.childNodes).filter(n=>!(n.nodeType===1 && n.tagName==='BUTTON'));
  const els=kids.filter(n=>n.nodeType===1);
  if(els.length>1)
    return els.map(c=>(c.textContent||'').trim()).filter(Boolean).join('  ');
  return kids.map(n=>n.textContent||'').join('').trim();
}

/** True for the section headings — the small upper-case labels. */
function logIsHead(el){
  return !!(el.style && el.style.textTransform==='uppercase');
}

/** The whole report as text, headings and all. */
function logAllText(b){
  return Array.from(b.children).map(logRowText).filter(t=>t).join('\n');
}

/** Write to the clipboard and say so on the button. `navigator.clipboard` needs a secure
    context and a focused document, and the webview gives neither reliably, so the old
    hidden-textarea route stands behind it. */
function logCopy(text,btn){
  const said=btn.textContent;
  const done=(ok)=>{
    btn.textContent = ok? T('report.copied') : T('report.copyfail');
    btn.disabled=true;
    setTimeout(()=>{ btn.textContent=said; btn.disabled=false; }, ok? 1100 : 2600);
  };
  const byHand=()=>{
    try{
      const ta=document.createElement('textarea');
      ta.value=text;
      ta.style.cssText='position:fixed;left:-9999px;top:0;opacity:0';
      document.body.appendChild(ta); ta.focus(); ta.select();
      const ok=document.execCommand('copy');
      ta.remove();
      return ok;
    }catch(_){ return false; }
  };
  if(navigator.clipboard && navigator.clipboard.writeText)
    navigator.clipboard.writeText(text).then(()=>done(true),()=>done(byHand()));
  else done(byHand());
}

/** One section's text - its heading and every row down to the next heading - read off
    the dialogue at the moment it is asked for (round 18cu). The buttons used to carry
    the text as it stood when the report was built, and the GPU lines fill in a second or
    two later, so a click copied "GPU by pass: …" however long the answer had been on
    screen. Robin: "the copy button does not seem to copy exactly what is shown". */
function logSectionText(h){
  const lines=[logRowText(h)];
  for(let el=h.nextElementSibling; el && !logIsHead(el); el=el.nextElementSibling){
    const t=logRowText(el); if(t) lines.push(t);
  }
  return lines.join('\n');
}

/** Whether any line of `b` is still waiting on the driver (a GPU line ending in "…"). */
function logPending(b){
  return Array.from(b.children).some(el=>/…$/.test(logRowText(el)));
}

/** Copies what `text()` gives, once nothing in `b` is still measuring - up to five
    seconds of waiting, then whatever is there. The button says it is waiting. */
function logCopyWhenReady(b,text,btn){
  if(!logPending(b)){ logCopy(text(),btn); return; }
  const said=btn.textContent;
  btn.textContent=T('report.copywait'); btn.disabled=true;
  const t0=performance.now();
  const tick=()=>{
    if(logPending(b) && performance.now()-t0<5000){ setTimeout(tick,100); return; }
    btn.disabled=false; btn.textContent=said;
    logCopy(text(),btn);
  };
  setTimeout(tick,100);
}

/** Put a copy button on every section heading. */
function logCopyButtons(b){
  for(const h of Array.from(b.children)){
    if(!logIsHead(h)) continue;
    const btn=document.createElement('button');
    btn.className='btn sm';
    btn.style.cssText='padding:1px 7px;font-size:10px;letter-spacing:.2px;text-transform:none;'+
      'font-weight:600;margin-left:auto';
    btn.textContent=T('report.copysec');
    btn.title=T('report.copysec_title');
    btn.onclick=()=>logCopyWhenReady(b,()=>logSectionText(h),btn);
    h.style.display='flex';
    h.style.alignItems='center';
    h.style.gap='8px';
    h.appendChild(btn);
  }
}

/** Round 18cr: every WebGL call `fn` makes, counted - for the report's "WebGL calls last
 *  frame" line. The context's methods are wrapped for the duration and put back after,
 *  the multi-draw extension's with them; nothing else about the frame changes. A draw is
 *  a draw call as the page issued it (a multi-draw counts once here; the sub-draws are
 *  in `R._drawn`). */
function countGlCalls(gl,fn){
  const P=WebGL2RenderingContext.prototype;
  const saved=[];
  const c={total:0, draws:0, multi:0, uniforms:0, binds:0};
  const wrap=(obj,k,kind)=>{
    const d=Object.getOwnPropertyDescriptor(obj,k);
    if(!d || typeof d.value!=='function') return;
    const f=d.value;
    saved.push([obj,k,d]);
    Object.defineProperty(obj,k,{configurable:true, writable:true, value:function(...a){
      c.total++; if(kind) c[kind]++; return f.apply(this,a); }});
  };
  try{
    for(const k of Object.getOwnPropertyNames(P)){
      if(k==='constructor') continue;
      const kind = /^draw(Arrays|Elements)/.test(k)? 'draws' : /^uniform/.test(k)? 'uniforms' :
                   /^bind/.test(k)? 'binds' : null;
      wrap(P,k,kind);
    }
    const ext=(App.R && App.R._mdraw)||null;
    if(ext){ const EP=Object.getPrototypeOf(ext); for(const k of Object.getOwnPropertyNames(EP)) if(/^multiDraw/.test(k)) wrap(EP,k,'multi'); }
    fn();
  }catch(_){ }
  finally{ for(const [obj,k,d] of saved) Object.defineProperty(obj,k,d); }
  return c;
}

function openLog(){
  const b=$('#logBody'); b.innerHTML='';
  /* Round 18bv: one dialogue, two reports. The title says which — it carries `data-tx`
     as well, so switching language while it is open re-applies the load-order name; that
     is a stale label on an open dialogue rather than anything wrong, and the alternative
     is a second modal to keep in step with this one.
     Round 18dj: written as markup, the way `Text.apply` writes every `data-tx` element -
     the wording of a `data-tx` key is the element's own innerHTML, and this one holds an
     ampersand as `&amp;`. Written as text it read "Load order &amp; asset report" (Robin). */
  { const ttl=$('#mLogTitle'); if(ttl) ttl.innerHTML=T('report.title'); }

  // --- plugin load order, so it can be checked against MO2's right pane ---
  if(GameData.scanned || GameData.plugins.length || GameData.missing.length){
    const h=document.createElement('div');
    h.style.cssText='font-size:11px;font-weight:700;letter-spacing:.6px;text-transform:uppercase;'+
      'color:var(--tx2);margin:2px 0 8px';
    h.textContent=T('report.load_order');
    b.appendChild(h);
    const src=document.createElement('div');
    src.className = GameData.plugins.length? 'okbox':'warnbox';
    src.innerHTML = GameData.plugins.length
      ? T('report.order_read',{source:'<b>'+GameData.orderSource+'</b>', n:GameData.plugins.length})
      : T('report.order_none');
    b.appendChild(src);
    GameData.plugins.forEach((pg,i)=>{
      const r=document.createElement('div');
      r.style.cssText='display:flex;gap:9px;align-items:baseline;padding:3px 9px;'+
        'border-bottom:1px solid var(--line);font-family:var(--mono);font-size:11px';
      const n=document.createElement('span');
      n.style.cssText='color:var(--tx3);flex:0 0 34px;text-align:right'; n.textContent='#'+(i+1);
      const t=document.createElement('span');
      t.style.cssText='flex:1;overflow:hidden;text-overflow:ellipsis;white-space:nowrap';
      t.textContent=pg.name;
      const c=document.createElement('span');
      c.style.cssText='flex:0 0 auto;font-size:10px;color:var(--tx3)';
      // The second engine does not count per plugin; a row of zeros said nothing.
      c.textContent=(pg.ltex||pg.regn||pg.cells)? [pg.ltex+' ltex',pg.regn+' regn',pg.cells+' cells'].join(' · ') : '';
      r.appendChild(n); r.appendChild(t); r.appendChild(c);
      b.appendChild(r);
      // Masters decide how this plugin's reference numbers are read: a reference tagged
      // with master n replaces the one already in that plugin instead of adding to it.
      const mn=GameData.masterNames(i), mi=GameData.masterIx[i]||[];
      if(mn.length){
        const m=document.createElement('div');
        m.style.cssText='padding:1px 9px 4px 43px;border-bottom:1px solid var(--line);'+
          'font-family:var(--mono);font-size:10px;color:var(--tx3)';
        m.textContent=T('report.masters',{list:mn.map((x,k)=>x+(mi[k]>=0?'':T('report.not_loaded'))).join(' · ')});
        if(mi.some(v=>v<0)) m.style.color='var(--warn)';
        b.appendChild(m);
      }
    });
    if(CellData.loadWarnings && CellData.loadWarnings.length){
      const w=document.createElement('div'); w.className='warnbox'; w.style.marginTop='10px';
      w.textContent=T('report.index_warnings',{list:CellData.loadWarnings.join('; ')});
      b.appendChild(w);
    }
    if(GameData.missing.length){
      const w=document.createElement('div'); w.className='warnbox'; w.style.marginTop='10px';
      w.innerHTML=T('report.missing_plugins',{n:GameData.missing.length, list:GameData.missing.slice(0,8).join(', ')})+
        (GameData.missing.length>8?' …':'');
      b.appendChild(w);
    }
    const h2=document.createElement('div');
    h2.style.cssText='font-size:11px;font-weight:700;letter-spacing:.6px;text-transform:uppercase;'+
      'color:var(--tx2);margin:16px 0 8px';
    h2.textContent=T('report.assets');
    b.appendChild(h2);
  }

  const line=(l)=>{ const d=document.createElement('div'); d.className='logline '+(l.k||'');
    d.textContent=(l.k==='e'?'✕ ' : (l.k==='ok'?'✓ ':'⚠ '))+l.t; b.appendChild(d); return d; };
  for(const l of (App.connectLog||[])) line(l);
  if(!App.loadLog.length){
    const ok=document.createElement('div'); ok.className='okbox';
    ok.textContent=T('report.assets_ok');
    b.appendChild(ok);
  } else {
    const seen=new Set();
    for(const l of App.loadLog){
      if(seen.has(l.t)) continue; seen.add(l.t);
      line(l);
    }
  }

  /* wording: developer diagnostics begin. Everything from here to the matching end is
     the numbers Robin pastes into a bug report — load times, frame cost, the ground map's
     bake — and stays English by design (wording-plan.md, "What is in scope"); the wording
     guard skips it. The headings above them are catalogued like any other. */
  /* Round 16b: where the last loads' time went. Robin: "the placing objects step was
     heavy" - and the sandbox cannot say what his machine does, so the report says it. */
  if(App.loadTimes && App.loadTimes.length){
    const h4=document.createElement('div');
    h4.style.cssText='font-size:11px;font-weight:700;letter-spacing:.6px;text-transform:uppercase;'+
      'color:var(--tx2);margin:16px 0 8px';
    h4.textContent=T('report.loading');
    b.appendChild(h4);
    const sec=ms=>(ms/1000).toFixed(ms<10000? 1 : 0)+' s';
    const mb=n=>(n/1e6).toFixed(n<10e6? 1 : 0)+' MB';
    for(const t of App.loadTimes.slice().reverse()){
      const total=t.cells+t.terrain+t.objects;
      // Round 17y: and what a step to the next cell kept - chunks and cells' grass.
      const kept=t.pan? ' · kept '+(t.chunksKept|0)+' chunks'+(t.scatterKept!=null? ', '+t.scatterKept+' cells’ grass' : '') : '';
      line({k:'ok', t:t.label+': '+sec(total)+' — cells '+sec(t.cells)+' · terrain '+sec(t.terrain)+
                     ' · objects '+sec(t.objects)+' ('+t.meshes+' new meshes, '+t.textures+' new textures'+
                     (t.calls? ', '+t.calls+' engine calls, '+mb(t.bytes) : '')+')'+kept});
      /* Round 18cr: the objects phase apart. Robin: "placing statics when loading a scene"
         feels slow, and the sandbox cannot say which of its four parts is slow on his
         machine - the engine reading and decoding (on its own threads; `engine` is what
         the engine itself clocked, `waited` the page's wait for the answers, summed over
         the bundles in flight at once), the page decoding and uploading what came back
         (main thread, so these sums are real time), or building the scene from it. */
      const pf=t.prof;
      if(pf && (pf.bundles||pf.texN||pf.meshN)){
        line({k:'ok', t:'   objects apart: engine '+sec(pf.engine)+' ('+pf.bundles+' bundles, '+
          (pf.engineKnown? 'its own clock' : 'not clocked by this engine')+') · waited '+sec(pf.wait)+
          ' summed over bundles in flight · page: textures '+sec(pf.tex)+' ('+pf.texN+', '+mb(pf.texBytes)+
          ' to the GPU) · meshes '+sec(pf.mesh)+' ('+pf.meshN+') · scene '+sec(pf.build)+
          (pf.groupsMs!=null? ' (groups '+Math.round(pf.groupsMs)+' ms)' : '')});
      }
    }
  }

  /* Round 17s: what a frame costs, on *this* machine. Robin: "Add a performance report
     (if it doesn't already exist) so I can share real numbers from my machine with you."
     The sandbox these are developed in renders in software, so every frame time measured
     there is between ten and a hundred times out; the counts transfer and the milliseconds
     do not. This is the other half — the same numbers `probe_perf.js` collects, taken
     where they mean something.

     Twenty frames, timed, with `finish()` at each end so the number is the GPU's and not
     the queue's. Drawn at whatever the viewport is showing: that is the point. */
  if(App.R && App.R.gl){
    const h5=document.createElement('div');
    h5.style.cssText='font-size:11px;font-weight:700;letter-spacing:.6px;text-transform:uppercase;'+
      'color:var(--tx2);margin:16px 0 8px';
    h5.textContent=T('report.performance');
    b.appendChild(h5);
    const R=App.R, gl=R.gl;
    let card='unknown';
    try{ const dbg=gl.getExtension('WEBGL_debug_renderer_info');
         if(dbg) card=gl.getParameter(dbg.UNMASKED_RENDERER_WEBGL)||'unknown'; }catch(_){ }
    /* WebKit (the viewer's engine on Linux and macOS) hides the real GPU from pages and
       answers "Apple GPU" everywhere - a Steam Deck included. Said as such off a Mac. */
    if(/^Apple GPU$/i.test(card) && !/Mac/i.test(navigator.platform||''))
      card='hidden by WebKit (it reports every GPU as "Apple GPU")';
    /* Round 18cr: three numbers where there was one. Robin: "I think the performance
       report doesn't show correct amount of FPS, as I can feel the scene stutter a bit."
       The twenty frames are timed twice over - the page's own submit (`draw()` returning,
       nothing waited for) and the whole with `finish()` - and the GPU's share on its own
       clock when the timer-query extension is there, filled in a moment after the report
       opens because the answer comes back asynchronously. Then one more frame with every
       WebGL call counted, on its own so the counting does not weigh on the timing. And
       the cadence the screen actually showed, from the loop's own ring (`frameStats`). */
    /* Round 18cu: the whole is measured by reading a pixel back after each frame, which
       cannot return before the GPU has drawn it. It was measured with `finish()`, which
       Chromium implements as a flush on purpose - so the "whole" was the page's submit
       time said twice, on every report since 18cr. Read one frame at a time, this is a
       frame's latency, submit to last pixel; the GPU's own clock below is its busy time
       over the same twenty. */
    let ms=null, cpu=null, calls=null, tq=null, q=null;
    const px=new Uint8Array(4);
    const wait=()=>{ try{ gl.bindFramebuffer(gl.FRAMEBUFFER,null); gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,px); }catch(_){ } };
    try{
      R.dirty=true; R.draw(); wait();               // warm, and not counted
      try{ tq=gl.getExtension('EXT_disjoint_timer_query_webgl2')||null; }catch(_){ tq=null; }
      if(tq){ try{ q=gl.createQuery(); gl.beginQuery(tq.TIME_ELAPSED_EXT,q); }catch(_){ q=null; } }
      const t0=performance.now(); let sub=0;
      for(let i=0;i<20;i++){ const a=performance.now(); R.dirty=true; R.draw(); sub+=performance.now()-a; wait(); }
      if(q){ try{ gl.endQuery(tq.TIME_ELAPSED_EXT); }catch(_){ q=null; } }
      ms=(performance.now()-t0)/20; cpu=sub/20;
      calls=countGlCalls(gl,()=>{ R.dirty=true; R.draw(); });
      /* Round 18ct: one more frame, the GPU's clock started afresh at every pass, so the
         report can say *where* the GPU's time goes (`gpuTiming`, `gpuTimes` in 06_gl.js). */
      if(tq && R.gpuTiming){ R.gpuTiming(tq); R.dirty=true; R.draw(); gl.flush(); }
    }catch(_){ }
    const tris=(R.statics||[]).reduce((a,x)=>a+(x.count/3)*x.n,0);
    const gtris=(R.batches||[]).reduce((a,x)=>a+(x.count/3)*x.n,0);
    const n=x=>x.toLocaleString();
    line({k:'ok', t:'GPU: '+card});
    /* Wraithguard: what the GL said about MGE's water on its first frame (26_water.js,
       `T.diag`) - for the black band Linux draws over the water and Windows does not. */
    { const d=R._wt && R._wt.diag;
      if(d){
        const e=x=>x? '0x'+x.toString(16) : 'ok';
        line({k:(d.errCol||d.errDep||d.errSurface)? 'w' : 'ok',
              t:'water: resolve colour '+e(d.errCol)+', depth '+e(d.errDep)+', surface '+e(d.errSurface)+
                ' · '+d.W+'×'+d.H+' ×'+d.samples+' samples, depth '+d.depBits+' bits'+
                ' · near '+(+d.near).toFixed(2)+' far '+Math.round(d.far)+' · highp '+d.hp+' bits'+
                ' · reflection '+(d.refl? (d.rect? 'band '+d.rect.map(v=>v.toFixed(2)).join(',') : 'whole') : 'skipped')+
                ' '+d.RW+'×'+d.RH+' · waves '+(d.waves? 'on' : 'off')+', volume '+(d.vol? 'yes' : 'no')});
      } }
    const vpLine=line({k:'ok', t:'viewport '+gl.drawingBufferWidth+'×'+gl.drawingBufferHeight+
                    (ms!=null? ' — '+ms.toFixed(1)+' ms a frame from submit to last pixel ('+(1000/ms).toFixed(0)+' fps), of which the page submitting '+
                               cpu.toFixed(1)+' ms' : '')+
                    (q? ' — GPU …' : ' — no GPU timer on this machine')});
    if(q){
      /* The GPU's own time for the twenty frames, read when the driver has it. A
         disjoint answer (the clock was reset mid-measurement) is said rather than shown. */
      const base=vpLine.textContent.replace(/ — GPU …$/,'');
      let tries=0;
      const poll=()=>{
        try{
          if(++tries>1200){ gl.deleteQuery(q); vpLine.textContent=base+' — GPU timer never answered'; return; }
          const avail=gl.getQueryParameter(q,gl.QUERY_RESULT_AVAILABLE);
          const disj=gl.getParameter(tq.GPU_DISJOINT_EXT);
          if(!avail && !disj){ setTimeout(poll,50); return; }
          if(avail && !disj){
            const ns=gl.getQueryParameter(q,gl.QUERY_RESULT);
            vpLine.textContent=base+' — GPU '+(ns/1e6/20).toFixed(1)+' ms a frame';
          } else vpLine.textContent=base+' — GPU timer disjoint, open the report again';
          gl.deleteQuery(q);
        }catch(_){ vpLine.textContent=base+' — GPU timer failed'; }
      };
      setTimeout(poll,50);
    }
    if(tq && R.gpuTimes && R._gpuT){
      /* The passes' times, when the driver has them all. Under a tenth of a millisecond
         is left out; the sum is what the frame line's GPU number should agree with. */
      const byLine=line({k:'ok', t:'GPU by pass: …'});
      let tries=0;
      const pollBy=()=>{
        try{
          const r=R.gpuTimes();
          if(r===null){ if(++tries>600){ byLine.textContent='GPU by pass: the driver never answered'; return; } setTimeout(pollBy,50); return; }
          if(r===false){ byLine.textContent='GPU by pass: the GPU timer was disjoint, open the report again'; return; }
          const sum=r.reduce((a,x)=>a+x.ms,0);
          // The passes' names as `draw` marks them, said in full (the sunshafts' stretch ends with the present).
          const PASS={sky:'sky', ground:'ground', grass:'grass', objects:'objects', translucent:'see-through objects',
                      particles:'particles', water:'water', occlusion:'ambient occlusion', sunshafts:'sunshafts and present', present:'present'};
          const label=n=>n.startsWith('reflection:')? 'reflection: '+(PASS[n.slice(11)]||n.slice(11)) : (PASS[n]||n);
          const parts=r.filter(x=>x.ms>=0.05).map(x=>label(x.name)+' '+x.ms.toFixed(1));
          byLine.textContent='GPU by pass ('+sum.toFixed(1)+' ms in all): '+(parts.length? parts.join(' · ') : 'nothing measurable');
        }catch(_){ byLine.textContent='GPU by pass: the timer failed'; }
      };
      setTimeout(pollBy,50);
    }
    const fs=R.frameStats? R.frameStats() : null;
    line({k:'ok', t: fs? 'on screen: the last '+fs.n+' frames — median '+fs.median.toFixed(1)+' ms, 95th '+
                        fs.p95.toFixed(1)+' ms, worst '+fs.worst.toFixed(1)+' ms, '+Math.round(fs.over*100)+
                        '% over one 60 Hz refresh ('+(1000/fs.mean).toFixed(0)+' fps as shown)'
                      : 'on screen: no run of frames measured yet — move the camera for a second, then open the report again'});
    if(calls) line({k:'ok', t:'WebGL calls last frame: '+n(calls.total)+' ('+n(calls.draws)+' draws'+
                      (calls.multi? ', '+n(calls.multi)+' multi-draw' : '')+', '+n(calls.uniforms)+' uniforms, '+
                      n(calls.binds)+' binds)'});
    line({k:'ok', t:'objects: '+n((R.statics||[]).length)+' batches, '+n(Math.round(tris))+
                    ' triangles in the scene'});
    // Round 18cr: how the still objects are drawn - see `_buildGroups` in 06_gl.js.
    { const G=R._groups;
      if(G) line({k:'ok', t:'material groups: '+n(G.all.length)+' groups over '+n(G.batches)+' batches ('+
                    n(G.runs)+' runs, '+n(G.inst)+' instances, '+(G.bytes/1e6).toFixed(1)+' MB), '+
                    n((R._legacyStatics||[]).length)+' batches on their own — multi-draw '+(G.multi? 'on' : 'off')});
      else line({k:'ok', t:'material groups: none (nothing grouped in this scene)'});
      // Round 18cs: the bake - how many whole batches are copies now, and what they cost.
      if(G && G.baked) line({k:'ok', t:'baked: '+n(G.baked)+' whole batches as '+n(G.bakedInst)+' copies ('+
                    n(G.bakedVerts)+' vertices) — one sub-draw a group, no uniform between'});
      else if(G) line({k:'ok', t:'baked: nothing — no whole batch in a solid group'}); }
    line({k:'ok', t:'submitted last frame: '+n(R._drawn||0)+' draw calls ('+n(R._drawnBaked||0)+' of them baked ranges), '+
                    n(R._drawnInst||0)+' instances — the rest was culled'});
    /* Round 18cs: the reflection pass, drawn for the part of the screen the water can
       show on, or not at all when it shows nowhere. */
    if(R.water && R.opts.mgeWater!==false){
      const rc=R.reflRect;
      line({k:'ok', t:'reflection: '+(R.reflDrawn===false? 'skipped — the water is under ground everywhere in this load'
                       : rc? 'drawn for '+Math.round((rc[2]-rc[0])*(rc[3]-rc[1])/4*100)+'% of the screen, where the water can show'
                       : 'drawn for the whole screen')});
    }
    // Round 17v: a batch is one slot across every cell, with a run per cell inside it.
    const gruns=(R.batches||[]).reduce((a,x)=>a+((x.groups&&x.groups.length)||1),0);
    line({k:'ok', t:'grass: '+n((R.batches||[]).length)+' batches ('+n(gruns)+' cell runs), '+
                    n(Math.round(gtris))+' triangles'});
    line({k:'ok', t:'ground: '+n((R.cellChunks||[]).length)+' cell chunks'});
    // Round 17w: the particle systems — how many placed, how many near enough to run.
    if(R._parts) line({k:'ok', t:'particles: '+n(R._parts.runs||0)+' systems placed, '+
                        n(R._parts.live||0)+' particles alive, '+n(R._parts.drawn||0)+' drawn last frame'});
    const lg=R._lgrid||null;
    line({k:'ok', t:'lamps: '+n((R.lights||[]).length)+' in the scene, '+
                    n((lg&&lg.n)||0)+' sent'+(lg&&lg.gw? ' — a '+lg.gw+'×'+lg.gw+' grid of '+
                    Math.round(lg.bin)+'-unit columns, '+n(lg.total||0)+' entries' : '')+
                    ' (budget '+n(R.maxLights||0)+')'});
    line({k:'ok', t:'time of day '+Sky.clock(R.opts.hour||0)+' · lights '+(R.opts.lights||'night')+
                    ' · mode '+(App.mode==='cell'? 'real cell' : 'simplified')});
    /* The ground map, and whether it covers what is on screen. Round 17u: three rounds of
       "the grass goes white at the foot" have each had a different cause, and every one of
       them showed up here — a map that is stale, short, or holding a layer the install
       could not supply. The grass reads its tint from this and nothing else, so this is
       the line that answers it without another round of guessing.
       Its own section since round 17v — Robin: "consider moving the grass tint to a place
       where it fits better, or create a new header" — so it can be copied on its own. */
    {
      const h6=document.createElement('div');
      h6.style.cssText='font-size:11px;font-weight:700;letter-spacing:.6px;text-transform:uppercase;'+
        'color:var(--tx2);margin:16px 0 8px';
      h6.textContent=T('report.tint');
      b.appendChild(h6);
      line({k:'ok', t:T('report.tint_slider',{pct:Math.round((R.opts.tint||0)*100)})+
        (R.opts.tint>0? '' : T('report.tint_off'))});
      const gm=R.groundMap;
      let x0=Infinity,y0=Infinity,x1=-Infinity,y1=-Infinity;
      for(const ch of (R.cellChunks||[])) if(ch.box){
        x0=Math.min(x0,ch.box[0]); y0=Math.min(y0,ch.box[1]);
        x1=Math.max(x1,ch.box[2]); y1=Math.max(y1,ch.box[3]); }
      if(R.ground && R.ground.box){ const b2=R.ground.box;
        x0=Math.min(x0,b2[0]); y0=Math.min(y0,b2[1]); x1=Math.max(x1,b2[2]); y1=Math.max(y1,b2[3]); }
      const has=x1>x0;
      if(!gm || !gm.xf) line({k: R.opts.tint>0? 'w':'ok',
        t:'ground map: none baked'+(R.opts.tint>0? ' — the tint is on and has nothing to read' : '')});
      else{
        const w=1/gm.xf[2], h=1/gm.xf[3];
        const covers = has && gm.xf[0]<=x0+1 && gm.xf[1]<=y0+1 &&
                       gm.xf[0]+w>=x1-1 && gm.xf[1]+h>=y1-1;
        line({k:covers||!has? 'ok':'w',
          t:'ground map: '+Math.round(w)+'×'+Math.round(h)+' units at ('+Math.round(gm.xf[0])+', '+
            Math.round(gm.xf[1])+')'+(has? ' — ground is '+Math.round(x1-x0)+'×'+Math.round(y1-y0)+
            ' at ('+Math.round(x0)+', '+Math.round(y0)+'), '+(covers? 'covered' :
            'NOT COVERED: blades outside it take no tint') : '')});
      }
      // Layers the install could not supply: those bake as nothing and tint nothing.
      let bad=0, layers=0;
      for(const ch of (R.cellChunks||[])) for(const L of (ch.layers||[])){
        layers++; if(!L.tex && !L.flat) bad++; }
      if(layers) line({k:bad? 'w':'ok', t:'ground layers: '+layers+
        (bad? ', '+bad+' with no texture the install could supply — those patches tint nothing' : ', all supplied')});

      /* Round 17v: what the last bake did, and what is on the map now.
         Robin: the first full cell after start-up tints correctly at any ring count, a
         round trip through Simplified and back to that same cell still does, and loading
         any other cell or changing the ring count turns every blade white until the
         program is restarted. His report already said the map covers the ground and every
         layer was supplied, and the sandbox bakes the map correctly cell after cell — so
         the two things left are whether the bake ran at all and what it put there. Both
         are said here rather than guessed at again. */
      const bk=R._gmLast||null;
      line({k:'ok', t:'ground map bakes: '+(R._gmBakes||0)+' this session'+
        (bk? ' — the last was #'+bk.n+', '+bk.chunks+' chunk(s)'+(bk.patch? ' + the invented patch':'')+
             ', '+bk.drew+' draw(s)'+(bk.skipped? ', '+bk.skipped+' layer(s) skipped':'')+
             (bk.why? ' — '+bk.why : '') : ''),
        });
      /* Round 17v, second pass: what the driver said about those draws, and whether the
         first thing drawn actually took paint. Robin's report showed 436 draws and a map
         nothing had landed on; this is the line that says why. */
      if(bk && bk.drew){
        const gl2=R.gl;
        const names={[gl2.NO_ERROR]:'no error',[gl2.INVALID_ENUM]:'INVALID_ENUM',
                     [gl2.INVALID_VALUE]:'INVALID_VALUE',[gl2.INVALID_OPERATION]:'INVALID_OPERATION',
                     [gl2.INVALID_FRAMEBUFFER_OPERATION]:'INVALID_FRAMEBUFFER_OPERATION',
                     [gl2.OUT_OF_MEMORY]:'OUT_OF_MEMORY',[gl2.CONTEXT_LOST_WEBGL]:'CONTEXT_LOST'};
        const bad=(bk.err!=null && bk.err!==gl2.NO_ERROR) || bk.fbo===false || bk.landed===false;
        line({k:bad? 'e':'ok', t:'the last bake: GL '+(bk.err==null? 'not read' : (names[bk.err]||('error '+bk.err)))+
          ', framebuffer '+(bk.fbo===false? 'INCOMPLETE' : 'complete')+
          (bk.landed==null? '' : ', first chunk\'s centre '+(bk.landed? 'took paint' : 'took NOTHING')+
             (bk.sample? ' (rgba '+bk.sample.join(', ')+')' : ''))});
      }
      if(gm && gm.xf && R.readGroundMap){
        /* Sampled where a blade actually stands, not only averaged: an average can be
           right while the corner the grass is standing in is grey. */
        let at=null;
        for(const bt of (R.batches||[])) if(bt.n && bt.box){
          const bx=(bt.groups && bt.groups.length)? bt.groups[0].b : bt.box;   // one cell's run, not the whole span
          at=[(bx[0]+bx[3])/2,(bx[1]+bx[4])/2]; break; }
        let rd=null; try{ rd=R.readGroundMap(at); }catch(_){ }
        if(rd){
          const grey=rd.clear>50, pale=rd.white>50;
          line({k:(grey||pale)? 'w':'ok',
            t:'ground map contents: mean rgb '+rd.mean.join(', ')+' — '+rd.clear.toFixed(1)+
              '% never drawn on, '+rd.white.toFixed(1)+'% white'+
              (grey? ' — the map covers ground the terrain does not fill; blades standing there take no tint'
                   : pale? ' — the bake drew white' : '')});
          if(rd.at) line({k:'ok', t:'under the first grass batch: rgb '+rd.at.join(', ')+
              ' at ('+rd.uv[0].toFixed(3)+', '+rd.uv[1].toFixed(3)+') on the map'});
          else if(rd.uv) line({k:'w', t:'the first grass batch sits off the map, at ('+
              rd.uv[0].toFixed(3)+', '+rd.uv[1].toFixed(3)+') — every blade in it takes the edge texel'});
        }
        R.dirty=true;   // the readback borrowed the context; the window is redrawn anyway
      }
    }
  }

  /* Textures, one line each (round 14). Robin's vanilla pot and basket draw white on
     his machine and textured here, and the log had nothing to say about it: a texture
     that reads but never reaches the GPU is white with no error. So every texture the
     page has touched is listed with how it ended - its size and format, or why not. */
  if(App.texGL && App.texGL.size){
    const h3=document.createElement('div');
    h3.style.cssText='font-size:11px;font-weight:700;letter-spacing:.6px;text-transform:uppercase;'+
      'color:var(--tx2);margin:16px 0 8px';
    h3.textContent=T('report.textures');
    b.appendChild(h3);
    let good=0;
    const rows=[];
    for(const [key,rec] of App.texGL){
      if(rec.err) rows.push({k:'e', t:key+': '+rec.err});
      else if(!rec.gl) rows.push({k:'w', t:key+': read ('+rec.w+'×'+rec.h+') but not on the GPU - drawn white'+(rec.from? ' — '+rec.from : '')});
      else { good++; if(rows.length<400) rows.push({k:'ok', t:key+': '+rec.w+'×'+rec.h+(rec.kind? ' '+rec.kind : '')+(rec.from? ' — '+rec.from : '')}); }
    }
    const sum=document.createElement('div'); sum.className=(good===App.texGL.size)? 'okbox':'warnbox';
    let gpuBytes=0, biggest=0; for(const rec of App.texGL.values()){ gpuBytes+=rec.bytes||0; biggest=Math.max(biggest,rec.w||0,rec.h||0); }
    sum.textContent=good+' of '+App.texGL.size+' textures on the GPU'+
      (gpuBytes? ' — '+(gpuBytes/1e6).toFixed(gpuBytes<10e6? 1:0)+' MB, the largest '+biggest+' px (Texture detail: '+
                 ((App.textureDetail==null? 2048 : +App.textureDetail||0)||'full')+')' : '');
    b.appendChild(sum);
    rows.sort((x,y)=> (x.k==='ok')-(y.k==='ok'));
    for(const l of rows) line(l);
  }
  /* wording: developer diagnostics end. */
  logCopyButtons(b);
  $('#mLog').hidden=false;
}





/* =====================================================================================
   Starting up.

   The shell knows two things before anything is on screen: which install to reopen, and
   where `openmw.cfg` lives on this machine. Both are fetched first, because the Connect
   buttons are wired from them.

   Order matters. The install is reopened before the profile is adopted, because the
   profile remembers a cell and naming that cell needs the world loaded — the other way
   round, the viewport comes up on "(12, 9)" instead of "Balmora".

   A remembered path that no longer resolves is not worth stopping for: an external
   drive that is not plugged in, a folder that moved. The engine only offers a path that
   still exists, and anything that fails beyond that is said once, leaving the app
   simply unconnected — which is where a fresh install starts anyway.
   ===================================================================================== */
/** Opens Wraithguard's setup: the openmw.cfg it wrote from its sort lists for this launch
    (`window.__WG_VIEW__.cfg`), then the requested cell, or the picker when none was named.
    Set by the shell's initialization script; absent in a page opened any other way. */
async function openWraithguardSetup(){
  const want=window.__WG_VIEW__||{};
  if(!want.cfg) return;
  // Wraithguard's mesh viewer: the meshes, not a cell - so the install is opened for its
  // files only (no plugins read, no world merged), and nothing of the cell side starts.
  const meshes=(want.extra||{}).meshes;
  const meshView=want.meshView && Array.isArray(meshes) && meshes.length && typeof WgMeshView==='object';
  if(meshView) WgMeshView.prepare();
  const r=await openInstall('openmw', want.cfg, null, meshView? [] : null);
  if(!r && !VFS.connected) return;
  if(meshView){ WgMeshView.open(meshes); return; }
  // The cell map's coverage, read ahead so the picker opens with it.
  if(typeof WgCoverage==='object') WgCoverage.load();
  // Real-cell mode is the only one; running its handler shows the cell controls.
  const ms=$('#pvMode');
  if(ms){ ms.value='cell'; if(ms.onchange) ms.onchange(); }
  // Wraithguard: a record from its conflict viewer, found where it stands (45_wg_nav.js).
  if(want.cell && want.cell.kind==='find' && typeof WgNav==='object') WgNav.go('find:'+want.cell.tag+':'+want.cell.id);
  else if(want.cell && want.cell.kind==='plugin' && typeof WgNav==='object') WgNav.go('plugin:'+want.cell.name);
  else if(want.cell){ App.cellSel=want.cell; syncCellButton(); schedulePreview(); }
  else openCellPicker();
  // And whatever else it asks this window to show while it stays open.
  if(typeof WgNav==='object') WgNav.poll();
  // Launched on `--editor`: straight into the Editor mode (50_wg_editor.js).
  if(want.editor && typeof WgEditor==='object'){ WgEditor.button(); WgEditor.enter(); }
}

async function start(){
  boot();
  if(!Engine.has()){
    // Opened as a file rather than launched. Everything that matters needs the engine,
    // so say so plainly rather than degrading into something that looks like it works.
    const n=$('#noctx');
    if(n){
      n.hidden=false;
      n.innerHTML='<div class="big">'+T('vp.noengine_title')+'</div>'+
        '<div class="sm">'+T('vp.noengine_body')+'</div>';
    }
    return;
  }

  try{ Startup=await Engine.call('startup_install'); }catch(e){ /* defaults */ }
  /* Round 18cj: the colour theme, first of everything. It is in the profile as well, and
     the profile is what decides — but the profile is read a dozen awaits further down,
     after the install has been reopened, and until then the window would be showing the
     palette it ships with and then correcting itself in front of the person. The state
     file is read before anything is on screen, which is the whole reason Robin asked for
     the theme to be in it: "so it can be set directly on program start". */
  // Wraithguard: its current palette first, so the default theme is the toolkit's own.
  if(typeof Themes==='object') Themes.wraithguard((window.__WG_VIEW__||{}).theme);
  if(typeof Themes==='object'){
    if(Startup && Startup.theme!=null) App.theme=String(Startup.theme||'').trim();
    Themes.apply();
  }
  /* Round 18cp: and how the cell picker was last shown — list or map — from the same
     file, for the same reason. Robin: "Remember List/Map between runs." */
  if(Startup && Startup.cellPicker!=null) App.cellPick = String(Startup.cellPicker).trim()==='map'? 'map' : 'list';
  // Round 9 item 5: the cog, and the line under it saying where the files are kept.
  if(typeof wireSettings==='function') wireSettings();
  if(typeof WgViewport==='object') WgViewport.bind();
  if(typeof WgModHl==='object') WgModHl.bind();
  if(typeof WgTools==='object') WgTools.bind();   // the review, landscape, overlay and link tools

  Engine.call('cpu_threads').then(n=>{
    const el=$('#folderState .p');
    if(el) el.title=T('top.engine_title',{n});
  }).catch(()=>{});

  // The viewer profile first (23_viewer_profile.js), so the preview settings are in
  // place before the first cell draws.
  await Profiles.start().catch(e=>{
    toast(T('profiles.start_failed',{err:e}),'warn',9000);
  });
  /* Starred entries, read once so the first picker to open is already in the right
     order. Failure here is silent by design — `Favourites.load` falls back to an empty
     set, and a picker in its plain order is not worth a warning. */
  await Favourites.load(true);
  await openWraithguardSetup();
}

/** One way in for a harness: open an install the way a person does, rather than by
    reaching into the page's data structures and pretending. */
/* Round 18bi: `null` for the plugin chooser — the suites connect to fixtures whose whole
   load order is the point, and a dialogue in front of every one of them would be a
   dialogue in front of every suite. `t_plugins.js` drives the chooser directly instead. */
window.__gfConnect=(path,kind,profile)=>openInstall(kind||'vanilla',path,profile||null,null);

if(document.readyState==='loading') document.addEventListener('DOMContentLoaded',start);
else start();
</script>
</body>
</html>
