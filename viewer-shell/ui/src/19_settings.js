
/* =====================================================================================
   Preview settings.

   These used to live in a separate .ini, because the groundcover file had to stay
   exactly what Groundcover Generator expected and none of this belonged in it. With
   the native format that constraint is gone: one file holds the rules, the placement
   settings and the state of the viewport, so a config is a single artefact rather
   than a pair that can be separated and then quietly disagree.

   `collect` gathers the raw values off the controls; `native` splits them into the
   half the engine reads and the half only the viewport cares about, and
   `applyNative` puts them back.
   ===================================================================================== */
const PrevSettings={
  keys:[
    ['relief','#p_relief','num'], ['base','#p_base','num'], ['patch','#p_patch','val'],
    ['full','#p_full','bool'], ['road','#p_road','bool'],
    ['hour','#p_hour','num'], ['fog','#p_fog','bool'], ['bright','#pvBright','num'],
    ['tint','#p_tint','num'], ['sky','#p_sky','bool'],
    /* Round 17s: the lamps' switch, and the clock's. `lights` is a word rather than a
       number, so it travels as `val`; an older profile has none and `lightsLit` falls
       back to the hour rule. */
    ['lights','#p_lights','val'], ['timeFlow','#p_flow','bool'], ['timeScale','#p_flowk','num'],
    ['wind','#p_wind','num'], ['grassFar','#p_grassfar','num'], ['mge','#p_mge','bool'], ['mgew','#p_mgew','bool'], ['waterHue','#p_whue','num'], ['waterTint','#p_wtint','num'], ['sewers','#p_sewers','bool'], ['waves','#p_waves','bool'], ['waveHeight','#p_wheight','num'], ['caustics','#p_caust','num'], ['reflBlur','#p_wblur','bool'], ['waterDepth','#p_wdepth','bool'], ['weather','#p_weather','val'], ['fogd','#p_fogd','num'], ['shafts','#p_shafts','bool'], ['particles','#p_particles','bool'], ['actors','#p_actors','bool'], ['npcDrawn','#p_npcdrawn','bool'],
    ['ssao','#p_ssao','bool'],   // round 18a
    ['fxaa','#p_fxaa','bool'],   // round 18do
    ['underwater','#p_under','bool'],   // round 18dt
    ['dof','#p_dof','bool'],            // round 18dy
    ['bloom','#p_bloom','bool'],        // Wraithguard: 42_wg_bloom.js
    ['shadows','#p_shadows','bool'],       // round 18ee
    ['shadowRes','#p_shadowres','val'],    // Wraithguard: 0 = the install's
    ['unlit','#p_unlit','bool'], // round 18f
    ['brushFeather','#paintFeather','num'],
    ['brushOpacity','#paintOpacity','num'],
  ],
  collect(){
    const o={};
    for(const [k,sel,kind] of this.keys){
      const el=$(sel); if(!el) continue;
      o[k] = kind==='bool'? (el.checked?1:0) : el.value;
    }
    /* The other control whose position is not its value. `step="0.0833333"` is an inexact
       twelfth, so the browser snaps half past nine to 21.499992 and the file remembered
       that; the page has kept the rounded hour in `App.hour` since round 17s for exactly
       this reason ("a control that quantises can display a value it must not also own"),
       and the profile should carry the hour rather than the thumb's position. Round 18r. */
    if(typeof App==='object' && isFinite(+App.hour)) o.hour=+App.hour;
    /* Not `mode` and not `cell`. Which preview you had open and which cell was in it are
       where you *were*, not how you work, and restoring them meant a saved profile could
       open the tool straight into a full cell load — every plugin's landscape, every
       placed object — before anyone had asked for one. Simplified is where a session
       starts and Real cell starts empty. */
    o.showCorpses=App.showCorpses?1:0;
    o.showAdjacent=adjacentRings();
    o.avoidStatics=App.avoidStatics?1:0;
    o.avoidPad=App.avoidPad;
    o.avoidMinSize=App.avoidMinSize;
    o.avoidHeight=App.avoidHeight;
    o.seed=App.seed;
    o.exportScope=App.exportScope||'world';
    o.exportPrefix=App.exportPrefix||'';
    o.exportAuthor=App.exportAuthor||'Gardenfell';
    o.exportDesc=App.exportDesc||'';
    o.filler=App.cfg.filler||DEFAULT_FILLER;
    o.showEmpty=(App.R&&App.R.opts.markers!==false)?1:0;
    return o;
  },
  /** The halves of the native file this module owns: what the engine reads, and
      what only the viewport cares about. */
  /** The profile, in the shape the engine reads.

      Viewport settings travel as key/value pairs rather than as fields: the engine
      writes them into the file but never looks inside them, and giving it a struct to
      keep in step with the page's controls would be handing it an opinion about camera
      angles it has no business having. */
  native(){
    const o=this.collect();
    const n=v=>+v||0;
    const vp=[
      ['relief',n(o.relief)], ['base',n(o.base)],
      /* Round 18bf (§I9): the simplified preview's patch extent. It has always been in
         `keys` — so `collect` gathered it and every caller of `collect` saw it — and it
         was the one member of `keys` with no entry here, which meant it was gathered and
         then dropped on the floor. The same omission the note at the foot of this list
         describes for the five controls round 18q added, arriving by the same road: a
         control added to the reading side and never to the writing one. Robin: "All
         settings under preview [...] should be saved to profile, and restored on program
         launch." */
      ['patch',n(o.patch)],
      /* `clump_tile` used to be written here. It was one number for the whole config,
         held in `[viewport]` — the block the engine explicitly carries and never reads —
         and nothing consumed it. It is a per-rule field in the rules file now, because
         it changes the .esp. Nothing migrates: the old value never did anything, so
         there is nothing to carry forward. */
      ['full',!!+o.full], ['road',!!+o.road],
      ['hour',n(o.hour)], ['fog',!!+o.fog], ['brightness',n(o.bright)],
      // Round 11 item 11: the viewport's own two, carried like the rest of the lighting.
      /* `vertex_colours` and `blend` were written here until round 15; both are always
         on now and an old profile's value for either is ignored. */
      ['ground_tint',n(o.tint)],
      /* Round 12 item 4. Round 17y: while a room has the switch forced off, the profile
         keeps saving *your* setting — the one put aside on the way in. */
      ['atmosphere', App._skySwitchSaved!=null? !!App._skySwitchSaved : !!+o.sky],
      ['wind',n(o.wind)],        // round 12 item 6
      /* Round 18q: five controls the *reading* side has always known about and the
         writing side never wrote, so they were restored from a value nothing ever put in
         the file - which is to say they were not restored at all. Robin: "All settings
         under preview [...] should be saved to profile, and restored on program launch."
         The Lights mode, the clock and its speed, how far the grass reaches, and the
         weather particles. `lights` is a word, like `navigation`; the rest are numbers
         and switches. */
      ['lights',String(o.lights||'night')],
      ['timeFlow',!!+o.timeFlow], ['timeScale',n(o.timeScale)],
      ['grass_distance',n(o.grassFar)],
      ['particles',!!+o.particles],
      ['mge_grass',!!+o.mge],    // round 14
      ['mge_water',!!+o.mgew],   // round 15
      ['water_hue',n(o.waterHue)], ['water_tint',n(o.waterTint)], ['sewer_waves',!!+o.sewers],   // Wraithguard: 26_water.js
      ['waves',!!+o.waves], ['wave_height',n(o.waveHeight)], ['caustics',n(o.caustics)], ['blur_reflections',!!+o.reflBlur], ['water_depth',!!+o.waterDepth], ['weather',String(o.weather||'Clear')],   // Wraithguard: MGE XE's water settings
      ['sunshafts',!!+o.shafts], // round 16
      ['ssao',!!+o.ssao],        // round 18a
      ['fxaa',!!+o.fxaa],        // round 18do
      ['underwater',!!+o.underwater],   // round 18dt
      ['dof',!!+o.dof],                 // round 18dy
      ['bloom',!!+o.bloom],             // Wraithguard: 42_wg_bloom.js
      ['shadows',!!+o.shadows],         // round 18ee
      ['shadow_detail',String(o.shadowRes||'0')],
      ['unlit',!!+o.unlit],      // round 18f
      ['fog_density',n(o.fogd)], // round 15 item 3
      /* `mode` and `cell` used to be written here too. They are the only two things the
         viewport ever saved that were not a setting: one said which preview was open and
         the other which cell was in it. Restoring them meant opening a profile could
         start a full cell load — every landscape in the order, every placed object —
         before anybody had asked to see a cell. A profile is a setup, not a bookmark. */
      ['show_corpses',!!+o.showCorpses],
      // Which parts of the window you have folded away is about the window.
      ['fold_textures',!!App.selFolded],
      /* And which of the list's two groups are folded. Written as two flags rather than
         as the object the page holds, because a `[viewport]` value is a key and a scalar —
         the engine carries these and never looks inside them. */
      ['fold_ground',!!(App.selGroupShut||{}).ground],
      ['fold_paint',!!(App.selGroupShut||{}).paint],
      /* Round 18q: and the grass card's two sections, which start folded (Robin: "make it
         default closed") and stay wherever you leave them. */
      ['fold_card_overrides', (App.cardShut||{}).overrides!==false],
      ['fold_card_conditions',(App.cardShut||{}).conditions!==false],
      // Round 18ac: the third section, Drifts.
      ['fold_card_drift',     (App.cardShut||{}).drift!==false],
      ['arrows',App.R? App.R.opts.arrows!==false : true],
      ['show_cull',App.R? App.R.opts.showSlope!==false : true],
      ['show_cull_statics',App.R? !!App.R.opts.showSlopeStatics : false],
      ['show_cull_mesh',App.R? !!App.R.opts.showSlopeMesh : false],   // round 18l: the editor's
      ['highlight_objects', !App.hiObj? 'off' : (App.hiObjMode||'fill')],   // round 18dq: off, fill, outline
      /* The brush, not the paint. What you painted is the engine's and goes in the
         profile's own `[paint]` block; these three are the size of the tool in your hand,
         which is window state like everything else here. The mode itself is deliberately
         not saved — opening a profile should not put you in a mode where the right mouse
         button does something other than what it usually does. */
      ['brush_size',n(o.brushSize)], ['brush_feather',n(o.brushFeather)],
      ['brush_opacity',n(o.brushOpacity)],
      ['show_adjacent',+o.showAdjacent||0], ['show_empty',!!+o.showEmpty],   // rings, 0..3 (round 15)
      ['texture_detail', (App.textureDetail==null)? 2048 : (+App.textureDetail||0)],   // longest side, 0 = full (round 16)
      ['navigation', (App.R && App.R.nav)||'orbit'],                    // round 17: orbit | wasd
      ['fly_speed', App.R? Math.round(App.R.fly.speed) : 1500],
      ['look_smoothing', App.R? Math.round(Math.max(1,Math.min(100,+App.R.lookSmooth||35))) : 35],   // round 17d: the catch-up rate, 1..100
      /* Round 18ag: the WASD head's turn per pixel as a multiple of the built-in rate -
         the number a person would say ("twice as fast"), not the bar's position, and to
         three places so a thumb set by hand comes back where it was left. */
      ['look_sensitivity', App.R? +App.R.lookSensitivity().toFixed(3) : 1],
      ['move_smoothing', App.R? Math.round(Math.max(1,Math.min(100,+App.R.moveSmooth||12))) : 12],
      ['orbit_smoothing', App.R? Math.round(Math.max(1,Math.min(100,+App.R.orbitSmooth||40))) : 40],
      ['orbit_look_smoothing', App.R? Math.round(Math.max(1,Math.min(100,+App.R.orbitLookSmooth||30))) : 30],
      /* Whether the cell picker offers interiors. It is a switch inside a dialogue that
         opens fresh every time, so without this it would go back to off between one
         opening and the next — which for somebody working on rooms is the setting they
         would set most often and keep least. */
      ['show_interiors',!!App.showInteriors],
      // Tiles or lines in the asset picker, for the same reason: a dialogue that opens
      // fresh every time would forget it between one opening and the next.
      ['browse_grid',!!App.browseGrid],
      // Round 18bg: and the object picker's, which is the same switch on the same kind of
      // dialogue. Kept apart from the asset picker's because they are looked at for
      // different reasons — a mesh by its shape, an object by its id as often as not.
      ['object_grid',!!App.objGrid],
      // Wraithguard: zoom to cursor and the frame-rate cap (38_wg_viewport.js).
      ...(typeof WgViewport==='object'? WgViewport.save() : []),
    ];
    return {
      language:App.language||'',      // round 18aq: the language pack, by code; '' is English
      theme:App.theme||'',            // round 18cg: the colour theme, by code; '' is the one it ships with
      viewport:vp.map(([key,value])=>({key,value})),
    };
  },

  /* ---- persistence -------------------------------------------------------------
     The profile is a person's setup, so it should not have to be saved. It lives as a
     named `.profile.toml` beside the executable and this rewrites the active one as
     things change; the engine spells the TOML, because the page has no business
     holding a second serialiser.

     Debounced, and skipped when the text has not actually changed — dragging a
     slider fires a hundred times, and a hundred writes of an identical file is
     both pointless and the kind of thing that eventually corrupts one. */
  _last:null,
  _timer:0,
  _muted:false,

  /** What "nothing has changed since the last write" means.
   *
   *  The settings, **and where the engine's painted mask is**. The second half is not
   *  decoration: painting changes nothing in `native()` — the paint is the engine's and
   *  never crosses into the page — so a stroke left this key identical, `touch` returned
   *  at the guard below, and the profile was never rewritten. Every stroke of every kind
   *  was lost on restart.
   *
   *  It survived a long time because something else was tripping the write by accident:
   *  the `[viewport]` block used to carry which cell was open, the preview picked one on
   *  its way past, and that difference saved the paint as a side effect. When the cell
   *  came out of the profile — a profile is a setup, not a bookmark — the accident went
   *  with it, and nothing was left to notice. Naming the paint in the key is the write
   *  said out loud, the same fix `take` above got for the same reason.
   *
   *  A revision rather than the mask itself: the engine hands one back from every call
   *  that changes the paint, and it is already the number `History` puts in a snapshot. */
  _key(){
    return JSON.stringify(this.native())+'|'+(App.paintRev|0);
  },

  /** Writes the active profile, debounced and skipped when nothing changed.

      Dragging a slider fires a hundred times, and a hundred writes of an identical
      file is both pointless and the kind of thing that eventually corrupts one. */
  touch(){
    if(this._muted || !Engine.has()) return;
    clearTimeout(this._timer);
    this._timer=setTimeout(()=>this._save(),600);
  },

  /** The write the debounce ends in — see `Rules._save` for why it is its own function. */
  async _save(){
    /* A throw here means the settings could not even be turned into a payload, which
       is a different failure from the write being refused — and returning silently
       meant the profile stopped saving with nothing said anywhere. It goes in the log
       beside the write failures, for the same reason: a toast per keystroke is worse. */
    let payload;
    try{ payload=this._key(); }
    catch(e){
      App.loadLog&&App.loadLog.push(
        {k:'w',t:T('profiles.not_saved_read',{err:e&&e.message||e})});
      return;
    }
    if(payload===this._last) return;
    if(typeof Profiles!=='object' || !Profiles.active) return;
    try{
      await Profiles.write(JSON.stringify(this.native(), null, 1));
      /* **After the write, not before it.** `_last` means "what is on disk", and it is
         the only thing standing between a change and being written again — so recording
         a payload that never reached the file makes the next attempt at the same
         settings look like a duplicate and skips it. The change is then lost for good,
         silently, with the log line from the first failure the only trace. That is the
         shape of every bug in this section: something else did the saving, and when it
         stopped nobody noticed. */
      this._last=payload;
    }catch(e){
      // One line in the log rather than a toast on every keystroke: a read-only
      // folder would otherwise leave the user wondering why nothing persists.
      App.loadLog&&App.loadLog.push({k:'w',t:T('profiles.not_saved',{err:e&&e.message||e})});
    }
  },

  /** Writes now what the debounce was holding — before the store moves (round 18aq),
   *  for the reason `Rules.flush` gives. */
  async flush(){
    if(!this._timer) return;
    clearTimeout(this._timer); this._timer=0;
    if(this._muted || !Engine.has()) return;
    await this._save();
  },

  /** Applies settings that arrived from somewhere other than the active profile, and
      writes them into it.
   *
   *  `adopt` below is for a profile being opened: the file on disk already says what was
   *  applied, so writing it straight back would be a no-op. A *rules* file that carries
   *  settings — anything written before the two were split — is the other case. Those
   *  settings are new to the active profile and are gone the moment you switch profile
   *  and come back, unless something writes them.
   *
   *  Something did, by accident, for a long time: the saved viewport block carried which
   *  cell was open, the preview picked one on its way past, and the difference that made
   *  is what tripped the save. The cell is not saved any more — a profile is a setup, not
   *  a bookmark — and the accident went with it. This is the write said out loud. */
  take(preview){
    this.applyNative(preview, viewportMap(preview));
    this._last=null;              // what is in memory now has not been written anywhere
    this.touch();
  },

  /** Applies a profile without triggering a save of what was just loaded. */
  adopt(preview){
    this._muted=true;
    try{ this.applyNative(preview, viewportMap(preview)); }
    finally{ this._muted=false; }
    // Remembered as written, so restoring at startup does not immediately rewrite an
    // identical file.
    try{ this._last=this._key(); }catch(e){ this._last=null; }
  },

  /** Applies a profile the engine read back. Everything is optional: a file written
      before `[viewport]` existed has none, and must still open cleanly. */
  applyNative(P,V){
    P=P||{}; V=V||{};
    const setEl=(sel,v,kind)=>{ const el=$(sel); if(!el||v==null) return;
      if(kind==='bool') el.checked=!!v; else el.value=v;
      el.dispatchEvent(new Event(kind==='bool'?'change':'input')); };
    // Whether dead actors are drawn (the profile keeps it under its old avoidance name).
    if(P.avoidCorpses!=null) App.showCorpses=!!P.avoidCorpses;
    /* Only two scopes exist now. A profile written before the split names 'cell' or
       'all'; both meant "generate from the loaded rules", and 'cell' additionally read a
       viewport toggle. Whole world is the honest reading of either. */
    /* Round 18aq: the language the profile asks for. Applied after the rest, so the
       renderers it re-runs draw the settings that have just landed; a pack the machine
       has not got is said once, here, and the profile keeps asking for it (it is a shared
       profile's fact about its author, not a mistake to correct). */
    App.language=String(P.language||'').trim();
    if(typeof Lang==='object') Lang.sync();
    /* Round 18cg: and the colour theme, which needs nothing fetched — the blocks are in the
       stylesheet, so this is one attribute on `body` and the picker redrawn. A code this
       build has no block for is kept and worn anyway: nothing matches it, the window shows
       the palette it ships with, and the picker says what the profile asked for. */
    App.theme=String(P.theme||'').trim();
    if(typeof Themes==='object'){
      Themes.apply();
      /* Round 18cj: and this machine remembers what the profile asked for, so the next
         start-up wears it before it has read any profile. Switching profile is exactly
         the case Robin named — "as well as when I switch profiles later on in a live
         session" — and it is this line that makes the two agree. */
      Themes.remember();
    }
    // Absent in a profile written before the Advanced scope existed, which reads as
    // nothing ticked — the state that scope opens in anyway.

    for(const [k,sel,kind] of [['relief','#p_relief','num'],['base','#p_base','num'],
                               ['patch','#p_patch','val'],   // round 18bf (§I9)
                               ['full','#p_full','bool'], ['road','#p_road','bool'],
                               ['hour','#p_hour','num'],['fog','#p_fog','bool'],
                               ['lights','#p_lights','val'],
                               ['timeFlow','#p_flow','bool'],['timeScale','#p_flowk','num'],
                               ['brightness','#pvBright','num'],
                               ['ground_tint','#p_tint','num'],
                               ['wind','#p_wind','num'],
                               ['grass_distance','#p_grassfar','num'],
                               ['mge_grass','#p_mge','bool'],['mge_water','#p_mgew','bool'],['water_hue','#p_whue','num'],['water_tint','#p_wtint','num'],['sewer_waves','#p_sewers','bool'],['waves','#p_waves','bool'],['wave_height','#p_wheight','num'],['caustics','#p_caust','num'],['blur_reflections','#p_wblur','bool'],['water_depth','#p_wdepth','bool'],['weather','#p_weather','val'],['fog_density','#p_fogd','num'],['sunshafts','#p_shafts','bool'],['particles','#p_particles','bool'],['npcs_and_creatures','#p_actors','bool'],['weapons_drawn','#p_npcdrawn','bool'],
                               ['ssao','#p_ssao','bool'],
                               ['fxaa','#p_fxaa','bool'],
                               ['underwater','#p_under','bool'],
                               ['dof','#p_dof','bool'],
                               ['bloom','#p_bloom','bool'],
                               ['shadows','#p_shadows','bool'],
                               ['shadow_detail','#p_shadowres','val'],
                               ['unlit','#p_unlit','bool'],
                               ['brush_feather','#paintFeather','num'],
                               ['brush_opacity','#paintOpacity','num']])
      if(V[k]!=null) setEl(sel,V[k],kind);
    /* Round 18i: a profile that never stored the two post-shader switches takes them from
       the install's own chain - SSAO on when the chain names an SSAO, sunshafts likewise
       (`Sky.installPost`). A stored value is the person's and wins; this is only the
       starting point of a profile that has none, the way the fog density starts at the
       install's. The chain arrives with the atmosphere reply, so Sky fills these in when
       it lands if it has not yet.

       Round 18aw: only for a payload that carries a `[viewport]` block at all - a profile
       from disk. An undo step deliberately has no such block (`History.snap` strips it so
       stepping back never reaches over and resizes the brush), and reading its absence as
       "unset, take the chain's answer" switched the occlusion on with every undo. Robin:
       "The ambient occlusion sometimes gets enabled without me clicking it. I think it is
       sometimes when I paint, or when I undo painting. [...] I want it to only toggle or
       set state either based on my input, or when it loads on program start, never
       otherwise." A block that is there and says nothing about them is still the old
       profile 18i was written for. */
    if(Array.isArray(P.viewport)){
      App._postUnset={ssao:V.ssao==null, sunshafts:V.sunshafts==null, fxaa:V.fxaa==null, dof:V.dof==null, bloom:V.bloom==null,
                      shadows:V.shadows==null};   // 18do: and FXAA. 18dy: and the depth of field. 18ee: and the sun's shadows
      if(typeof Sky==='object') Sky.applyInstallPost();
    }
    // Units in the file, a position on the slider — see `collect`.
    if(V.brush_size!=null) setEl('#paintSize',BrushSize.pos(V.brush_size),'num');
    if(V.show_adjacent!=null){
      // A bool from before round 15 is one ring; a number is the ring count.
      App.showAdjacent = V.show_adjacent===true? 1 : (V.show_adjacent===false? 0 : Math.max(0,Math.min(3,+V.show_adjacent||0)));
      const e=$('#pvAdjacent'); if(e) e.value=String(App.showAdjacent); }
    if(V.show_interiors!=null) App.showInteriors=!!V.show_interiors;
    if(V.navigation!=null && App.R) App.R.setNav(String(V.navigation));
    if(V.fly_speed!=null && App.R) App.R.setFlySpeed(+V.fly_speed||1500,false);
    if(V.look_smoothing!=null && App.R){ App.R.lookSmooth=Math.max(1,Math.min(100,+V.look_smoothing||35)); if(App.syncNav) App.syncNav(); }
    if(V.look_sensitivity!=null && App.R){ App.R.lookSens=Math.max(0.25,Math.min(4,+V.look_sensitivity||1)); if(App.syncNav) App.syncNav(); }
    if(V.move_smoothing!=null && App.R){ App.R.moveSmooth=Math.max(1,Math.min(100,+V.move_smoothing||12)); if(App.syncNav) App.syncNav(); }
    if(V.orbit_smoothing!=null && App.R){ App.R.orbitSmooth=Math.max(1,Math.min(100,+V.orbit_smoothing||40)); if(App.syncNav) App.syncNav(); }
    if(V.orbit_look_smoothing!=null && App.R){ App.R.orbitLookSmooth=Math.max(1,Math.min(100,+V.orbit_look_smoothing||30)); if(App.syncNav) App.syncNav(); }
    if(V.texture_detail!=null){
      const want=Math.max(0,+V.texture_detail||0);
      const had=(App.textureDetail==null)? 2048 : (+App.textureDetail||0);
      App.textureDetail=want;
      const e=$('#pvTexDetail'); if(e) e.value=String([0,4096,2048,1024,512].includes(want)? want : 2048);
      // A different cap than the loaded textures were read at: they are read again.
      if(want!==had && App.texGL && App.texGL.size){ dropMeshes(); dropTextures(); App._scene=null; }
    }
    /* Round 17y: the Atmosphere switch - a room holds it off, and a profile opened in
       that room sets the value the room will give back. */
    if(V.atmosphere!=null){
      if(App._skySwitchSaved!=null) App._skySwitchSaved=!!V.atmosphere;
      else setEl('#p_sky',V.atmosphere,'bool');
    }
    if(V.browse_grid!=null) App.browseGrid=!!V.browse_grid;
    if(V.object_grid!=null) App.objGrid=!!V.object_grid;   // round 18bg
    if(V.show_empty!=null && App.R){ App.R.opts.markers=!!V.show_empty;
      const e=$('#tbEmpty'); if(e) e.classList.toggle('on',App.R.opts.markers); }
    if(V.arrows!=null && App.R){ App.R.opts.arrows=!!V.arrows;
      const e=$('#tbArrows'); if(e) e.classList.toggle('on',App.R.opts.arrows); }
    if(V.show_cull!=null && App.R){ App.R.opts.showSlope=!!V.show_cull; }
    if(V.show_cull_statics!=null && App.R){
      App.R.opts.showSlopeStatics=!!V.show_cull_statics; }
    if(V.show_cull_mesh!=null && App.R){ App.R.opts.showSlopeMesh=!!V.show_cull_mesh; }
    if(typeof WgViewport==='object') WgViewport.load(V);
    if(V.highlight_objects!=null){
      const v=String(V.highlight_objects);
      App.hiObj = v!=='off' && v!=='false';
      App.hiObjMode = v==='outline'? 'outline' : 'fill';
      if(App.R){ App.R.hlMode=App.hiObjMode; if(!App.hiObj) App.R.setStaticHighlight(null); }
      if(App.syncHiObjButton) App.syncHiObjButton();
    }
    if(App.syncCullButton) App.syncCullButton();
    schedulePreview();
  }
};



/* The last thing actually pushed, so the same config is not pushed twice.
 *
 * `CellScatter.cell` calls this before every cell, and a nine-cell rebuild therefore
 * serialised the whole config nine times and had the engine parse it nine times — on
 * the main thread, where the window is drawn. The config cannot have changed between
 * two cells of one rebuild, so eight of the nine were work done for nothing.
 *
 * Safe to skip because a skip means the engine already holds exactly this. The one
 * shape that would break it — the engine's copy changing without the page's — does not
 * arise: an import or a profile load is followed by the page *adopting* what the engine
 * holds, which changes the page's copy too and so changes what serialises here. Painted
 * ground is not in this payload at all (`set_config` carries it across), so nothing
 * about the brush can go stale here either. */
let _syncedCfg=null, _syncedPrev=null;

function syncConfig(){
  /* The grass rules this used to push to the engine are gone with grass generation; the
     viewer's settings are saved by the page itself (Profiles.write). Kept as a resolved
     promise for its callers. */
  return Promise.resolve();
}

/** `[viewport]` arrives as key/value pairs, because the engine carries it without
    reading it. The page does read it, so it wants an object. */
function viewportMap(P){
  const out={};
  for(const kv of (P&&P.viewport)||[]) out[kv.key]=kv.value;
  return out;
}

/* =====================================================================================
   Settings — the cog in the bottom-left corner.

   Round 9 item 5. One setting so far, and it is the one that matters most to somebody
   with a working install: where Gardenfell keeps the `.toml` files it writes.

   The default is beside the program, which is where they have always been. Choosing a
   folder moves what is there into it — Robin: "When I choose where to save the files and
   I already have existing files they are moved to that folder by Gardenfell" — and a
   name already taken in the destination is reported rather than overwritten, because
   the file already there is somebody's work too.

   The one file that never moves is `gardenfell.profiles.toml`: it is what says where
   the rest are, and a pointer that lives at the end of itself is no pointer at all. Two
   caches stay with it — the mesh index and the wording file — because neither is work
   anybody keeps.
   ===================================================================================== */
const Settings={
  show(){
    const w=$('#btnSettings');
    if(w) w.title=T('settings.btn_title');
  },

  async refresh(){ this.show(); },

  async open(){
    if(App.syncNav) App.syncNav();     // round 17c: the scheme's rows as they are now
    $('#mSettings').hidden=false;
  },

  async openFolder(what){
    if(!Engine.has()){ toast(T('profiles.no_engine'),'err',5000); return null; }
    try{ return await Engine.call('open_folder',{what:what||'store'}); }
    catch(e){ toast(T('settings.open_failed',{err:e.message||e}),'err',8000); return null; }
  },
};

/* Round 18r: every control the profile records saves the profile when it changes.
 *
 * Robin: "Changing the Time of Day in simplified preview does not save it between
 * sessions" and "Same with Ambient Occlusion, starts on sometimes even if I turned it
 * off." Both had the same cause and it was not the reading side: `#p_hour` and `#p_ssao`
 * are in `keys`, are collected, are written and are restored - but neither control called
 * `touch`, so moving them never asked for a write. They were saved only when some *other*
 * control's handler tripped a save and swept them up with it, which is why the ambient
 * occlusion came back "sometimes": with no `ssao` key in the file at all, `_postUnset`
 * hands the switch to the install's post chain, which turns it on.
 *
 * Wiring each control to `touch` is what had been going wrong one control at a time, so
 * this is the general answer instead: one delegated listener, driven by `keys` itself. A
 * control added to `keys` tomorrow saves without anyone remembering to say so, and a
 * control that is not in `keys` is not in the profile and does not save. `touch` is
 * debounced and skips a payload it has already written, so an extra call costs nothing.
 *
 * On the document, in the capture phase, so it does not depend on any handler letting the
 * event through or on the order the panels are wired in. */
(function bindProfileSaves(){
  const ids=new Set();
  for(const [,sel] of PrevSettings.keys) if(/^#[\w-]+$/.test(sel)) ids.add(sel.slice(1));
  // Its slider position is not its value (see `collect`), but it is still a saved setting.
  ids.add('paintSize');
  /* Round 18at: the Settings dialogue's navigation controls. They are not in `keys` —
     their values live on the renderer (`App.R.lookSens`, `.lookSmooth`, `.nav`…) and
     `native()` reads them from there — so the listener above never saw them, and their
     handlers never called `touch` either. Robin: "Mouse sensitivity in the Settings
     dialogue doesn't seem to save to the profile, but resets to 100% every time the
     program loads." Exactly 18r's shape, five controls further down the dialogue. */
  for(const id of ['setNav','setSens','setSmooth','setMoveSmooth','setOrbitSmooth','setOrbitSmoothRot','vpSpeedBar']) ids.add(id);
  const on=e=>{
    const t=e.target;
    if(!t || !t.id || !ids.has(t.id)) return;
    if(typeof PrevSettings==='object' && PrevSettings.touch) PrevSettings.touch();
  };
  document.addEventListener('input',on,true);
  document.addEventListener('change',on,true);
  PrevSettings._savedIds=ids;    // so a test can ask what is wired without guessing
})();

/* =====================================================================================
   Language packs (round 18aq, wording-plan.md phase 2).

   The engine owns the files — `lang_list` finds and checks every `gardenfell.<code>.toml`
   in `languages\` under where the files are kept (round 18as: beside the program, or the
   folder Settings chose, so packs move with the rest), `lang_get` hands one over whole,
   `lang_create` starts a new pack from the template, `lang_update` refreshes a pack
   against this version's catalogue, `open_folder` shows a folder in the file browser.
   The page owns the wording: `Text.apply` puts a pack on the markup and behind `T()`,
   and `App.reword` re-runs the renderers that are cheap to run again.

   The choice is the profile's (`App.language`, decision 5): `PrevSettings.native` writes
   it and `applyNative` reads it, so switching profile can switch language, and a profile
   that names a pack this machine has not got falls back to English with a warning.
   ===================================================================================== */
const Lang={
  /** What `lang_list` last said: {dir, packs}. */
  known:null,

  async list(){
    if(!Engine.has()) return null;
    try{ this.known=await Engine.call('lang_list',{}); }
    catch(e){ this.known={dir:'', packs:[], error:String(e.message||e)}; }
    this.fill();
    return this.known;
  },

  /** The picker: English first, then every pack by the name it gives itself, with its
   *  coverage — a pack that will not read is listed too, greyed, so a person who dropped
   *  a broken file in sees that it was found rather than wondering whether it was. */
  fill(){
    const sel=$('#setLang'); if(!sel) return;
    const k=this.known||{dir:'',packs:[]};
    sel.innerHTML='';
    const opt=(value,label,disabled)=>{ const o=document.createElement('option');
      o.value=value; o.textContent=label; if(disabled) o.disabled=true; sel.appendChild(o); };
    opt('', T('settings.lang_english'));
    for(const p of k.packs){
      if(p.error) opt(p.code, T('settings.lang_pack_broken',{name:p.name||p.code}), true);
      else opt(p.code, T('settings.lang_pack_option',{name:p.name, translated:p.translated, total:p.total}));
    }
    const want=App.language||'';
    if(want && !Array.from(sel.options).some(o=>o.value===want))
      opt(want, T('settings.lang_pack_missing_option',{code:want}));
    sel.value=want;
    /* Round 18at: the headline in the pack's own word, with the English word after it in
       parentheses — "Språk (language)" — so that somebody who picked a language they
       cannot read can still find the menu to pick another. The English is the catalogue's
       own, never the pack's; nothing to append when the pack shows the English anyway. */
    const head=$('#setLangHead');
    if(head){
      const shown=T('settings.language'), en=Text.said['settings.language']||shown;
      head.textContent = (Text.lang==='en' || shown===en)? shown : shown+' ('+en.toLowerCase()+')';
    }
    this.note();
  },

  /** The line under the picker: where packs go, and what is known about the chosen one. */
  note(){
    const n=$('#setLangNote'); if(!n) return;
    const k=this.known||{dir:'',packs:[]};
    const dir=k.dir||'';
    const lines=[T('settings.lang_note',{dir})];
    const code=App.language||'';
    const p=code? (k.packs||[]).find(x=>x.code===code) : null;
    if(code && !p) lines.push(T('settings.lang_missing_note',{file:'gardenfell.'+code+'.toml'}));
    else if(p && p.error) lines.push(T('settings.lang_broken_note',{file:p.file, err:p.error}));
    else if(p){
      lines.push(T('settings.lang_pack_note',{name:p.name, author:p.author||'—', madeFor:p.madeFor||'—',
                                              translated:p.translated, total:p.total}));
      if(p.orphans) lines.push(T('settings.lang_orphans_note',{n:p.orphans}));
      if(p.problems && p.problems.length){
        lines.push(T('settings.lang_problems_note',{n:p.problems.length}));
        for(const q of p.problems.slice(0,4)) lines.push(T('settings.lang_problem_line',{line:q.line, key:q.key, what:q.what}));
      }
    }
    n.innerHTML=lines.map(escHtml).join('<br>');
    const u=$('#setLangUpdate'); if(u) u.disabled=!(p && !p.error);
  },

  /** Puts the profile's language on the page, if it is not there already. The start-up
   *  and profile-switch path: quiet when it works, one warning when the pack is not
   *  there, English either way until it is. */
  async sync(force){
    const want=App.language||'';
    if(!force && want===(Text.lang==='en'? '' : Text.lang)) { this.note(); return; }
    if(!want){ Text.apply(null); this.fill(); return; }
    if(!Engine.has()) return;
    try{
      const pack=await Engine.call('lang_get',{code:want});
      Text.apply(pack);
      this.fill();
      if(pack.problems && pack.problems.length)
        toast(T('settings.lang_problems_toast',{name:pack.name, n:pack.problems.length}),'warn',8000);
    }catch(e){
      /* The warning Robin asked for (decision 5): the profile names a pack this machine
         has not got. English, and the profile keeps asking, so the next start says so
         again until the pack arrives. */
      Text.apply(null);
      this.fill();
      const dir=(this.known||{}).dir||'';
      toast(T('settings.lang_missing_toast',{file:'gardenfell.'+want+'.toml', dir}),'warn',12000);
    }
  },

  /** The picker's answer: the choice goes into the profile and onto the page. */
  async choose(code){
    App.language=String(code||'');
    await this.sync();
    if(typeof PrevSettings==='object' && PrevSettings.touch) PrevSettings.touch();
    if(App.language) toast(T('settings.lang_switched',{name:(Text.meta&&Text.meta.name)||App.language}),'ok',6000);
    else toast(T('settings.lang_switched_english'),'ok',4000);
  },

  /** Starts a translation: asks for the code and the language's own name, and has the
   *  engine write gardenfell.<code>.toml — every string listed, its English above, an
   *  empty value — into the languages folder. Round 18as, Robin: "Create new language
   *  file". Two questions rather than one form: the code decides the file's name and
   *  the name is what the picker shows, and neither is the other. */
  async create(){
    if(!Engine.has()){ toast(T('profiles.no_engine'),'err',5000); return; }
    const taken=((this.known||{}).packs||[]).map(p=>p.code);
    const code=await askName({title:T('settings.lang_new_head'), body:escHtml(T('settings.lang_new_code_body')),
                              go:T('settings.lang_new_next'), value:'', taken, takenText:T('settings.lang_new_taken')});
    if(!code) return;
    const name=await askName({title:T('settings.lang_new_head'), body:escHtml(T('settings.lang_new_name_body',{file:'gardenfell.'+code+'.toml'})),
                              go:T('name.create'), value:code});
    if(!name) return;
    try{
      const r=await Engine.call('lang_create',{code, name});
      toast(T('settings.lang_created',{file:r.file}),'ok',10000);
      await this.list();
    }catch(e){ toast(T('settings.lang_create_failed',{err:e.message||e}),'err',8000); }
  },

  /** The languages folder, in the file browser. */
  open(){ return Settings.openFolder('languages'); },

  async update(){
    const code=App.language||''; if(!code || !Engine.has()) return;
    try{
      const r=await Engine.call('lang_update',{code});
      toast(T('settings.lang_updated',{file:r.file, added:r.added, orphans:(r.orphans||[]).length, backup:r.backup}),'ok',10000);
      await this.list();
      await this.sync(true);       // the refreshed file is what is on the page now
    }catch(e){ toast(T('settings.lang_update_failed',{err:e.message||e}),'err',8000); }
  },
};

/* =====================================================================================
   The colour theme (round 18cg, theming-plan.md).

   Robin: "add a section in the settings menu that is called Theme, where we have a drop
   down list where all these themes are listed and can be picked. Their own entries in the
   drop down list should be like a small preview of the theme itself."

   **The whole of a theme is a block of custom properties** in `01_head.html` under
   `[data-theme="<code>"]`, so this object does almost nothing: it puts a code on `body`,
   saves it in the profile, and draws a list. It reads no colour and holds none. Two
   consequences worth knowing:

   1. **The rows are the themes.** A row carries `data-theme` itself, so the cascade paints
      it — its ground, its text, its accent bar, its swatches and its miniature are that
      theme's own, not a picture of them. Which is also why the first row wears `gardenfell`
      rather than nothing: while the window is in Telvanni, a row with no theme on it would
      inherit Telvanni and the entry for "the one it ships with" would be a lie. `:root`
      answers to `[data-theme="gardenfell"]` for that one row's sake.
   2. **A code this build has never heard of survives.** A profile written by some later
      version names a theme that is not here; the engine keeps the string
      (`config.rs`, `Preview::theme`), the window falls back to the palette it ships with —
      nothing matches, so nothing overrides `:root` — and the picker lists the code so that
      choosing something else is a decision rather than an accident. Nothing is rewritten
      on the way through.

   **The paint colours are not part of this.** `--hot` and `--grow` are declared in `:root`
   and in no theme block; the two tools' swatches and the ground the renderer tints are the
   same in every theme. Robin: "I don't want to ever change the painted colors for Grass or
   No Grass no matter what color theme is chosen." `t_theme.js` reads them under all five
   and fails if one moves.
   ===================================================================================== */
const Themes={
  /** The five, in the order the study reads them (`gardenfell-five-palettes.html`): the
   *  one the program ships with, the three from the deck, then the new one.
   *
   *  `code` is what the profile stores and what goes on `body` — '' for the default, which
   *  is the engine omitting the key rather than writing a name for "no name". `wear` is
   *  what the *row* carries, which differs only for the default (see the note above). */
  all:[
    // Wraithguard: the default is Wraithguard's own palette (see `wraithguard`, below);
    // Gardenfell's green is one choice among the rest.
    {code:'',          wear:'wraithguard', name:'settings.theme_wraithguard'},
    {code:'gardenfell', wear:'gardenfell', name:'settings.theme_default'},
    {code:'velothi',  wear:'velothi',  name:'settings.theme_velothi'},
    {code:'redoran',  wear:'redoran',  name:'settings.theme_redoran'},
    {code:'telvanni', wear:'telvanni', name:'settings.theme_telvanni'},
    {code:'hlaalu',   wear:'hlaalu',   name:'settings.theme_hlaalu'},
    // Round 18db: the quiet one - starlight on a moonless sky.
    {code:'firmament', wear:'firmament', name:'settings.theme_firmament'},
  ],

  /** The chosen code, trimmed, '' for the one it ships with. */
  code(){ return String(App.theme||'').trim(); },

  /** Wraithguard's palette, as the toolkit is wearing it now (`__WG_VIEW__.theme`, its
      `DARK` dict): the base colours of the `wraithguard` theme block in viewer_only.html,
      which derives everything else from them. Absent keys keep the stock values. */
  wraithguard(p){
    if(!p || typeof p!=='object') return;
    const hex=v=>/^#[0-9a-f]{3,8}$/i.test(String(v||''))? String(v) : null;
    const map={bg:'--wg-bg', bg2:'--wg-bg2', field_bg:'--wg-field', border:'--wg-border',
               fg:'--wg-fg', fg_dim:'--wg-fg-dim', select:'--wg-select', btn_bg:'--wg-btn',
               btn_bg_active:'--wg-btn-active', accent:'--wg-accent', log_bg:'--wg-log'};
    const lines=[];
    for(const k in map){ const v=hex(p[k]); if(v) lines.push(map[k]+':'+v+';'); }
    // The text that sits on the accent: dark on a light accent, white on a dark one.
    const ac=hex(p.accent);
    if(ac){
      const n=ac.length===4? ac.slice(1).split('').map(c=>parseInt(c+c,16)) : [1,3,5].map(i=>parseInt(ac.slice(i,i+2),16));
      const lin=c=>{ c/=255; return c<=0.03928? c/12.92 : Math.pow((c+0.055)/1.055,2.4); };
      const L=0.2126*lin(n[0])+0.7152*lin(n[1])+0.0722*lin(n[2]);
      lines.push('--wg-on-accent:'+(L>0.179? '#1b1b1b' : '#ffffff')+';');
    }
    let el=document.getElementById('wg-theme-live');
    if(!el){ el=document.createElement('style'); el.id='wg-theme-live'; document.head.appendChild(el); }
    el.textContent='[data-theme="wraithguard"]{'+lines.join('')+'}';
  },

  /** Puts the chosen theme on the window, and redraws the picker to agree with it.
   *
   *  The start-up and profile-switch path, and the only thing that ever writes
   *  `body[data-theme]`. The default writes no attribute at all rather than
   *  `data-theme="gardenfell"`: "omitted means the one it ships with" is what the
   *  profile file says, and the window saying the same thing costs nothing. */
  apply(){
    const code=this.code();
    document.body.dataset.theme=code||'wraithguard';
    /* Round 18cj: the one thing in the window the stylesheet does not draw. The orbit
       pivot — its ball, rings, centre and the ring it stamps on the ground — is painted by
       the renderer in the theme's `--orbit`, which it reads from the stylesheet once and
       caches, because it is asked for inside the draw. This is the only moment that answer
       can change, so this is where the cache is dropped and the viewport is asked again. */
    if(typeof orbitCol==='function') orbitCol.forget();
    // Round 18dd: and the exterior's backdrop with the atmosphere off (`--vp-back`), read
    // and cached the same way, for the same reason.
    if(typeof backCol==='function') backCol.forget();
    // Wraithguard: and the cell picker's map, which caches the theme's colours the same way.
    if(typeof CellMap==='object' && CellMap.forgetTheme) CellMap.forgetTheme();
    if(App.R) App.R.dirty=true;
    this.fill();
  },

  /** The button's face and the list's rows. Called by `apply`, and again by `App.reword`
   *  when a language pack goes on or comes off — the names are catalogued strings. */
  fill(){
    const btn=$('#setThemeBtn'), list=$('#setThemeList');
    if(!btn||!list) return;
    const code=this.code();
    const known=this.all.find(t=>t.code===code);
    const nm=btn.querySelector('.thn');
    if(nm) nm.textContent = known? T(known.name) : code;
    list.innerHTML='';
    /* Every theme the build has, and — last — the one the profile named that it has not.
       An unknown row wears nothing, so it shows the palette the window actually falls back
       to, which is the honest preview of choosing it. */
    const rows=this.all.map(t=>({code:t.code, wear:t.wear, label:T(t.name), tag:''}));
    if(code && !known) rows.push({code, wear:'', label:T('settings.theme_unknown'), tag:code});
    for(const r of rows){
      const el=document.createElement('div');
      el.className='thopt'+(r.code===code? ' on':'');
      el.setAttribute('role','option');
      el.setAttribute('aria-selected', r.code===code? 'true':'false');
      el.dataset.code=r.code;
      if(r.wear) el.dataset.theme=r.wear;
      el.innerHTML='<span class="thmini"></span>'+
                   '<span class="thn">'+escHtml(r.label)+'</span>'+
                   (r.tag? '<span class="thtag">'+escHtml(r.tag)+'</span>' : '')+
                   '<span class="thsw"><i></i><i></i><i></i></span>';
      el.onclick=()=>{ this.close(); this.choose(r.code); };
      list.appendChild(el);
    }
  },

  /** Open, closed, and which way up. */
  open(){
    const btn=$('#setThemeBtn'), list=$('#setThemeList');
    if(!btn||!list||!list.hidden) return;
    this.fill();
    list.hidden=false;
    btn.setAttribute('aria-expanded','true');
    this.place();
    /* Both on the document, in the capture phase, for the reason the rule picker's own
       keys are: nothing else has to agree to let the event through, and Escape reaches
       this before the dialogue's own close. Removed on the way out — a listener left
       behind would swallow Escape for the rest of the session. */
    this._keys=e=>this.onKey(e);
    this._away=e=>{ if(!$('#setTheme').contains(e.target)) this.close(); };
    document.addEventListener('keydown',this._keys,true);
    document.addEventListener('mousedown',this._away,true);
  },

  close(){
    const btn=$('#setThemeBtn'), list=$('#setThemeList');
    if(list){ list.hidden=true; list.classList.remove('up'); }
    if(btn) btn.setAttribute('aria-expanded','false');
    if(this._keys) document.removeEventListener('keydown',this._keys,true);
    if(this._away) document.removeEventListener('mousedown',this._away,true);
    this._keys=this._away=null;
    this._at=-1;
  },

  toggle(){ const l=$('#setThemeList'); if(l && l.hidden) this.open(); else this.close(); },

  /** Drops downward, which is what a drop-down does — except when it cannot. The Settings
   *  body scrolls (`.mbody{overflow:auto}`), so a list that reaches past its foot is
   *  clipped rather than overlaid. Scrolling the dialogue is the first answer and the
   *  better one (the list stays under the button it belongs to); opening upward is for the
   *  case where there is nothing below to scroll into view. */
  place(){
    const wrap=$('#setTheme'), list=$('#setThemeList');
    if(!wrap||!list||list.hidden) return;
    const sc=wrap.closest('.mbody'); if(!sc) return;
    const over=list.getBoundingClientRect().bottom - sc.getBoundingClientRect().bottom + 8;
    if(over<=0) return;
    const room=Math.max(0, sc.scrollHeight - sc.clientHeight - sc.scrollTop);
    const by=Math.min(over,room);
    if(by>0) sc.scrollTop+=by;
    if(over-by>0) list.classList.add('up');
  },

  /** The keyboard a listbox owes, while the list is open. Arrows move a highlight, Enter
   *  takes it, Escape leaves with the theme unchanged. Nothing is focusable in this program
   *  (Tab is swallowed in `11_events.js`), so the highlight is ours to draw and `_at` is
   *  where it is — not `document.activeElement`. */
  onKey(e){
    const list=$('#setThemeList'); if(!list||list.hidden) return;
    const rows=Array.from(list.querySelectorAll('.thopt'));
    if(!rows.length) return;
    const k=e.key;
    if(k==='Escape'){ e.preventDefault(); e.stopPropagation(); this.close(); return; }
    if(k==='Enter'||k===' '){
      e.preventDefault(); e.stopPropagation();
      const at=this._at>=0? this._at : rows.findIndex(r=>r.classList.contains('on'));
      const r=rows[at>=0? at : 0];
      this.close(); this.choose(r.dataset.code||'');
      return;
    }
    if(k!=='ArrowDown'&&k!=='ArrowUp'&&k!=='Home'&&k!=='End') return;
    e.preventDefault(); e.stopPropagation();
    let at=this._at;
    if(at<0) at=rows.findIndex(r=>r.classList.contains('on'));
    if(k==='Home') at=0;
    else if(k==='End') at=rows.length-1;
    else at=Math.max(0,Math.min(rows.length-1,(at<0?0:at)+(k==='ArrowDown'?1:-1)));
    this._at=at;
    rows.forEach((r,i)=>r.classList.toggle('at',i===at));
    rows[at].scrollIntoView({block:'nearest'});
  },

  /** Writes the choice into the machine's own bookkeeping (`gardenfell.profiles.toml`),
   *  which is what start-up reads before any profile has been opened. Round 18cj, Robin:
   *  "I want the chosen theme to be saved both to the profile, and the profiles.toml file,
   *  so it can be set directly on program start, as well as when I switch profiles later
   *  on in a live session."
   *
   *  The profile is still where the choice *lives* — a shared profile brings its author's
   *  theme with it, and opening one mirrors it here. Best-effort: this is bookkeeping, and
   *  a window that cannot write it comes up in the default next time rather than failing
   *  to come up. The engine skips the write when the file already says this. */
  remember(){
    if(!Engine.has()) return;
    Engine.call('theme_set',{code:this.code()}).catch(()=>{});
  },

  /** The picker's answer: the window wears it and the profile keeps it.
   *
   *  No toast. A language switch says so because most of the window is unchanged by it and
   *  the one thing that proves it worked is a word you may not read; a theme switch repaints
   *  every surface on screen, which is the same message delivered by the thing itself. */
  choose(code){
    const want=String(code||'');
    if(want===this.code()) { this.fill(); return; }
    App.theme=want;
    this.apply();
    this.remember();
    if(typeof PrevSettings==='object' && PrevSettings.touch) PrevSettings.touch();
  },
};

/** Wires the cog. Called from the same place every other panel's wiring is. */
function wireSettings(){
  const b=$('#btnSettings');
  if(b) b.onclick=()=>Settings.open();
  const x=()=>{ $('#mSettings').hidden=true; };
  if($('#mSettingsX')) $('#mSettingsX').onclick=x;
  if($('#mSettingsClose')) $('#mSettingsClose').onclick=x;
  Settings.refresh();
  /* Round 18cg: the theme row. The button opens the list; every row's own click is wired
     as the list is drawn (`Themes.fill`), because a row is a theme rather than a position. */
  const tb=$('#setThemeBtn');
  if(tb) tb.onclick=()=>Themes.toggle();
  /* Drawn now rather than at the first opening: the button carries the theme's name, and a
     dialogue that opens saying "—" until something else happens has already said the wrong
     thing. `apply` would do it too, but that runs when a profile arrives, and a first run
     has none. */
  Themes.fill();
  // Round 18aq: the language row.
  const ls=$('#setLang');
  if(ls) ls.onchange=()=>Lang.choose(ls.value);
  const lr=$('#setLangReload'); if(lr) lr.onclick=()=>Lang.list();
  const lu=$('#setLangUpdate'); if(lu) lu.onclick=()=>Lang.update();
  const lc=$('#setLangCreate'); if(lc) lc.onclick=()=>Lang.create();
  const lo=$('#setLangOpen'); if(lo) lo.onclick=()=>Lang.open();
  // Round 18as: the store folder, in the file browser.
  Lang.list();
}
