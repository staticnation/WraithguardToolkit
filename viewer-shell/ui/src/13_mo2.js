

/* Round 18ab. The connect is a window, and nothing a read finds inside it may be filed
   under either world.

   Standing the old install down bumps the page's world signature, but the engine only
   changes hands when `open_install` returns, several round trips later. Anything the
   page read in between asked the *outgoing* install and was answered about it: for a
   mesh the two installs do not share, "not found in the load order". The asset cache's
   guard compared world signatures, saw its own unchanged — it had been bumped before
   the read started — and filed that failure as the new world's answer. That is why MO2
   → Data Files → MO2 came back with the grass gone, and why only restarting the program
   brought it back.

   `App.swapEpoch` ticks at both ends of the window and `App.swapping` marks its inside;
   between them the asset caches refuse anything read across the window or within it.
   The second drop, once the world has actually changed hands, throws away whatever a
   path that does not go through those guards may still have left behind. */
/** Opens an install and fills the page from the world the engine loaded.
    Returns the engine's summary, or null when it could not be opened. */
async function openInstall(kind, path, profile, plugins){
  /* The load order is Wraithguard's (its sort lists, written as the cfg this opens), so
     there is nothing to choose here: `plugins` is the whole of it - or, for the mesh
     viewer, an empty list: the data folders and archives, and no plugin parsed at all. */
  if(!(Array.isArray(plugins) && plugins.length===0)) plugins=null;
  App.swapEpoch=(App.swapEpoch|0)+1; App.swapping=true;
  try{ return await openInstallRun(kind, path, profile, plugins); }
  catch(e){
    /* Round 18bd (F2): a throw part-way through the connect.
       `openInstallRun` sets `VFS.connected=true` and shows the busy card, then awaits
       five commands and fills the page from their answers. If any of them rejected — or
       any of the synchronous fill after them threw — the throw escaped: the `Busy.hide()`
       at the end was never reached, the `finally` below restored the swap epoch and
       nothing else, and the rejection landed in an unawaited `onclick` as an unhandled
       rejection. So the overlay stayed, the reading-the-install card stayed with it, and
       because `unloadForSwitch()` had already emptied the page the program claimed an
       install was open with nothing in it. Only a restart cleared it.
       Stood back down instead: not connected, no install marked, and the reason said out
       loud — which is the same answer the `open_install` failure a few lines down has
       always given. */
    console.error(e);
    VFS.connected=false;
    App.installKind=null;
    try{ if(typeof markInstall==='function') markInstall(null); }catch(_){ }
    toast(T('connect.failed_partway',{err:(e&&e.message)||String(e)}),'err',12000);
    return null;
  }
  finally{ endSwap(); Busy.free(); }
}
/** Closes the window, once. Called where the connect finishes properly — before the page
    is refreshed, so the rebuild that refresh asks for is the first thing to run *outside*
    the window and is kept — and again from the `finally` above, which is what covers every
    early return and every throw on the way. */
function endSwap(){
  if(!App.swapping) return;
  App.swapping=false;
  App.swapEpoch=(App.swapEpoch|0)+1;
}
/** Which install is open, as a string that says so: kind, path and MO2 profile.
 *
 *  Round 18bd (E9). Robin: clear the undo history "Only when the install is actually
 *  changed" — reconnecting the same folder to pick up a mod you just enabled is not the
 *  act the history's header is about ("connecting a different install […] replaces
 *  everything on screen with a different piece of work"), and losing an afternoon's undo
 *  to it would be its own annoyance. */
function installId(kind, path, profile){
  return [kind||'', String(path||''), String(profile||'')].join('\u0000');
}

async function openInstallRun(kind, path, profile, plugins){
  const wasId=App.installId||null;
  /* Round 18bo: the card is claimed and on screen *before* anything is taken down.
   *
   * It used to go up after `unloadForSwitch`, which drops every mesh and texture, disposes
   * the renderer's statics and terrain and re-renders the whole rule list. On Robin's
   * install — 916 mods, 143 rules — that is seconds of work with the scene already gone
   * and nothing on screen to say so, and anything that yields inside it paints exactly what
   * he photographed: a checkerboard patch, a window you can still click, and no card. And
   * a connect is long enough that the page's own debounced work wakes up inside it and
   * takes the card down again (see `Busy.claim`).
   *
   * So: claim, show, let it paint, and only then stand the old install down. The unload is
   * still after the pick — which is what the note below is about, and still true: until the
   * person pressed the button they could have gone back to what they were doing. */
  const card=Busy.claim();
  card.show(T('busy.reading_install'),T('busy.resolving_overlay'),0.04);
  await Busy.tick();
  /* Standing the old install down is what makes the program empty, and everything below
     replaces what this takes away. */
  if(typeof unloadForSwitch==='function') unloadForSwitch();
  card.show(T('busy.reading_install'),T('busy.resolving_overlay'),0.08);
  await Busy.tick();
  /* The world changes hands *here*, not when the page finishes reading the answer.
   *
   * `open_install` replaces the engine's world, so from the moment it is called every
   * rebuild still in flight is asking about a world that is going away. Those rebuilds
   * stop themselves by watching `GameData.sig`, and the signature used to be bumped
   * further down, with the rest of the page's state — which left a window a few round
   * trips wide where a rebuild believed it was current and asked the new world for a
   * cell only the old one had. In simplified mode that is the preview patch, and the
   * engine's answer is "no landscape for that cell": an error in the console for a run
   * whose results were going to be thrown away regardless.
   *
   * Bumping it first costs nothing — the page's copy of the world is about to be
   * cleared anyway — and closes the window completely. */
  GameData.newWorld();
  let r;
  // Round 18cr: the page's own clock on each step of the connect, for the log (see the
  // `connect.log_page` line below).
  const ct=App.connectTimes=Object.assign({}, App.connectTimes||{}, {tOpen:performance.now()});
  try{ r=await Engine.call('open_install',{kind, path, profile:profile||null,
                                          ...(plugins? {plugins} : {})}); }
  catch(e){ card.done(); toast(Text.word(String(e.message||e)),'err',12000); return null; }
  ct.open=performance.now()-ct.tOpen;
  if(!r.overlayReused){ ct.layout=r.msLayout; ct.index=r.msIndex; ct.archives=r.msArchives; ct.plugins=r.msPlugins; ct.files=r.files; }

  /* The world has changed hands *now*. Drop again: everything read between the first
     drop and this line came out of the install being left. */
  if(typeof dropMeshes==='function') dropMeshes();
  dropTextures();

  VFS.connected=true;
  App.installKind=kind;
  /* Round 18bd (E9): the history goes only when this is *another* install. The header of
     `22_history.js` has always said a connect clears it; only the rule-set switch actually
     did, so after a connect you could step back into a state taken against the install you
     left, paint revision and all. */
  const nowId=installId(kind, path, profile);
  App.installId=nowId;
  /* Round 18bi: what the plugin chooser needs to reopen against the install that is
     actually open. `installId` is a joined string built for comparing, not for taking
     apart again, so the three parts are kept as themselves. */
  App.installAt={kind, path, profile:profile||null};

  card.show(T('busy.reading_install'),T('busy.indexing'),0.55);
  await Busy.tick();
  const tList=performance.now();
  const [tex,sc,cl,use_,as_]=await Promise.all([
    Engine.call('textures'), Engine.call('scopes'), Engine.call('cells'),
    Engine.call('texture_usage'), Engine.call('assets'),
  ]);
  ct.list=performance.now()-tList;

  GameData.clear();
  /* A cell chosen in the install you just left does not exist in this one — not at
     the same grid position, and often not at all. Keeping the selection means the
     first thing a new install does is ask the engine for a landscape it has never
     heard of. The startup path restores the remembered cell *after* this, out of the
     profile, so nothing is lost by dropping it here. */
  if(typeof CellData!=='undefined' && CellData.clear) CellData.clear();
  App.cellSel=null; App.texOn={};
  /* And the built scene with it. It can never be reused — its key carries the world it
     was built for — so leaving it behind only pins a cell's worth of terrain, meshes
     and instance buffers for a world nobody can get back to. */
  App._scene=null;
  if(typeof syncCellButton==='function') syncCellButton();
  /* `file` is what actually gets drawn: a config names an LTEX id, and the LTEX
     record's own DATA field names the image. They are rarely the same string. */
  tex.forEach(t=>GameData.ltex.set(t.id.toLowerCase(),
    {id:t.id, index:null, file:t.file||'', from:'native'}));
  sc.regions.forEach(n=>GameData._addScope(n,'region','native'));
  sc.cells.forEach(n=>GameData._addScope(n,'cell','native'));

  // grid drives the cell picker; the named ones also drive the scope list.
  cl.grid.forEach(([x,y,name,region])=>{
    GameData.grid.set(x+','+y,{name,region});
    if(name) GameData.cells.set(name.toLowerCase(),{name,x,y,region,interior:false});
  });
  /* And the rooms, by name, kept apart from the grid. Nothing scatters in one yet — the
     picker can offer them, and picking one says so. */
  GameData.interiors=(cl.interiors||[]).map(([name,refs])=>({name, refs:refs|0}));
  // usage narrows the scope picker to places a texture is actually painted.
  for(const [t,u] of Object.entries(use_))
    GameData.usage.set(t,{cells:new Set(u.cells), regions:new Set(u.regions)});

  GameData.landScanned=true;
  GameData.scanned=true;
  GameData._names=r.order;
  // A non-empty plugin list is what tells the page the load order resolved; leave it
  // empty and it warns that texture checking is off.
  GameData.plugins=r.order.map(name=>({name, ltex:0, regn:0, cells:0, records:0}));
  GameData.missing=r.missing;
  GameData.orderSource=r.orderFile;
  /* Each opened plugin's masters (round 14 — the report had called a method that went
     with the first engine, and threw before it opened). The engine lists them for the
     plugins it opened, in order; a listed-but-missing plugin has none. */
  const opened=r.order.filter(n=>!r.missing.includes(n));
  GameData.pluginMasters=r.order.map(n=>{ const k=opened.indexOf(n); return k>=0 && r.masters? (r.masters[k]||[]) : []; });
  const lc=r.order.map(n=>n.toLowerCase());
  GameData.masterIx=GameData.pluginMasters.map(ms=>ms.map(m=>lc.indexOf(String(m).toLowerCase())));

  Assets.meshes=as_.meshes;
  Assets.textures=as_.textures;
  Assets.indexing=false;
  Assets.done=true;
  if(typeof buildGameDatalists==='function') buildGameDatalists();

  /* The sky's sources - the ini's weather ramp, Weather Adjuster, MGE.ini - read with
     the install (round 13). Best effort: a page with no sky data draws the vanilla ramp. */
  if(typeof Sky==='object') await Sky.load();
  const msOf=(v)=> (v==null? "?" : String(Math.round(v)));
  App.connectLog=[
    {k:'ok', t:T('connect.log_plugins',{plugins:r.plugins, size:mb(r.bytes), records:r.records})},
    {k:'ok', t:T('connect.log_times',{read:r.msRead, parse:r.msParse, merge:r.msMerge})},
    /* Round 18cr: the overlay's own steps, and the page's. Robin: start-up "feels a bit
       slow", and the line above was the only clock on it. `ct.scene` is filled in when
       the first scene stands (below), so the line is built then. */
    {k:'ok', t:T('connect.log_overlay_times',{layout:msOf(ct.layout), index:msOf(ct.index), files:(ct.files||0).toLocaleString(),
                                             roots:(r.roots||[]).length, archives:msOf(ct.archives), plugins:msOf(ct.plugins), headers:msOf(ct.headers)})},
    {k:'ok', t:T('connect.log_cells',{cells:r.cells, lands:r.lands})},
    {k:'ok', t:T('connect.log_indexed',{meshes:Assets.meshes.length, textures:Assets.textures.length})},
    /* The archives (round 11 item 10): which, and how much they hold. A load order with
       none is worth a word too, since a vanilla basket drawing white is what that looks
       like from the viewport. */
    {k:r.archives? 'ok':'warn',
     t:r.archives? T('connect.log_archives',{n:r.archives, names:(r.archiveNames||[]).join(', '), packed:r.packed})
                 : T('connect.log_no_archives')},
    {k:'ok', t:T('connect.log_order',{file:r.orderFile})},
    /* Both halves of "what did it actually read", because with a thousand mods installed
       neither is obvious and Robin asked the question out loud: the manager's own count of
       what is switched on, and the plugins that came from it. Only the active ones are in
       the overlay — a disabled mod, an unmanaged one and a separator line are all skipped
       where the overlay is built. */
    ...(r.mods? [{k:'ok', t:T('connect.log_mods',{n:r.mods})+
        (r.modsOff? ', '+T('connect.log_mods_off',{n:r.modsOff}) : '')}] : []),
    /* The overlay itself, trimmed — see `overlayLogLine`. */
    {k:'ok', t:T('connect.log_overlay',{roots:overlayLogLine(r.roots)})},
    {k:r.refsOverridden? 'warn':'ok',
     t:T('connect.log_refs',{kept:r.refsKept, overridden:r.refsOverridden, deleted:r.refsDeleted, moved:r.refsMoved})},
    /* Round 18al: the Code Patch, when the game folder carries its record, and the one
       feature that bears on the scene. Robin: "There are some banners that move in
       Gardenfell that doesn't in my load order … What makes them not move in the game?"
       Feature 132 does: a script's flagless PlayGroup waits behind the object's Idle, and
       a banner's Idle never lets it through. Said here so that the difference between the
       game and the preview is traceable to the line that explains it - and, on Robin's
       word, the preview keeps them waving outdoors regardless: "for banners we make an
       exception: Make them wave as they did before. Only outdoors though". */
    ...(r.mcpFound? [{k:'ok', t:T('connect.log_mcp',{n:r.mcpFeatures})+
        (r.mcpStillBanners? ' — '+T('connect.log_mcp_banners') : '')}] : []),
  ];
  (r.warnings||[]).forEach(w=>App.loadLog.push({k:'warn', t:Text.word(w)}));
  if(r.missing.length)
    App.loadLog.push({k:'err', t:T('connect.log_missing',{list:r.missing.join(', ')})});

  /* The mesh viewer's connect (an empty plugin list): the files are what it needs, and
     they are indexed now. No cell scene to draw and no map to read. */
  if(Array.isArray(plugins) && plugins.length===0){
    markInstall(kind, r.label, r.orderFile);
    endSwap();
    card.done();
    return r;
  }
  const ms=r.msRead+r.msParse+r.msMerge;
  toast((r.mods? T('connect.toast_mods',{n:r.mods})+', ' : '')+
        T('connect.toast',{plugins:r.plugins, cells:r.cells, ms}),'ok',5000);
  (r.warnings||[]).forEach(w=>toast(Text.word(w),'warn',9000));
  markInstall(kind, r.label, r.orderFile);
  endSwap();
  refreshAll();
  /* Round 18bl. The card used to come down here — with the world read and the scene not
     yet drawn — and Robin met the gap the moment the Plugins button made re-reading a
     load order an ordinary thing to do: "I am just left in simplified preview and a
     checkerboard texture on the ground […] It does eventually load". The world arriving is
     not the job finishing; the viewport showing it is. So the card stays up, over a
     darkened viewport, until the scene stands.
     Best effort, and after `endSwap`: a rebuild that throws must not leave the card on
     screen for ever, and the `finally` in `openInstall` hides it either way. */
  card.show(T('busy.reading_install'), T('busy.drawing_scene'), 0.9);
  await Busy.tick();
  const tScene=performance.now();
  try{ if(typeof previewNow==='function') await previewNow(); }
  catch(e){ console.error(e); }
  /* Round 18cr: the page's clock on the whole connect, one line. `probe` is the wait for
     the load order and its headers (the chooser's), `open` the wait for the world,
     `listing` the five answers the page fills itself from, `scene` the first picture.
     Round 18cs: `scene` is measured from the connect's start to the first cell scene
     standing, wherever that happens (18_cellpreview.js rewrites the line then); what the
     wait just above took stands in until it does. */
  if(!ct.sceneDone) ct.scene=performance.now()-tScene;
  /* Round 18cx: and the cell picker's map, read here so the picker opens with it. Robin:
     "I want to load in the map and cache it on opening the install (including changing
     install, or changing which .esp files are ticked), so it's zero load time for the map
     when the user opens the map cell picker" and "Keep the loading bar until the map is
     readied too". After the scene, so the picture the person is waiting for is not made
     to share the engine's cores with the map; the card counts the cells as they land.
     Best effort, like the scene: a map that could not be read is a line in the log and
     the picker reads it again when opened. */
  card.show(T('busy.reading_install'), T('busy.reading_map'), 0.92);
  await Busy.tick();
  const tMap=performance.now();
  let mapLine=null;
  try{
    if(typeof CellMap==='object' && CellMap.load){
      await CellMap.load((done,total)=>card.show(T('busy.reading_install'), T('busy.reading_map_n',{done, total}), 0.92+0.08*(total? done/total : 1)));
      const w=CellMap.world;
      mapLine={k:'ok', t:T('connect.log_map',{cells:(w&&w.total)||0, ms:Math.round(performance.now()-tMap)})};
    }
  }catch(e){ console.error(e); mapLine={k:'warn', t:T('connect.log_map_failed',{err:Text.word(String((e&&e.message)||e))})}; }
  ct.map=performance.now()-tMap;
  ct.line={k:'ok', t:T('connect.log_page',{probe:msOf(ct.probe), open:msOf(ct.open), listing:msOf(ct.list), scene:msOf(ct.scene)})};
  App.connectLog.push(ct.line);
  if(mapLine) App.connectLog.push(mapLine);
  card.done();
  return r;
}


/** The MO2 profile chooser. Profiles enable different mods and different plugin
    orders, so which one is meant cannot be guessed — but when there is only one
    there is nothing to ask. */
/** The overlay, trimmed to a log line: the six that win, read the way MO2 reads.
 *
 *  Nine hundred roots on one line is not a log line, it is a wall, so six of them are
 *  shown — and it has to be the six that *win*, because "which mod's copy of this file is
 *  the one being used" is the only question anybody brings to this line.
 *
 *  **Round 18cb: base-first, so the last name is the winner.** The engine hands its roots
 *  back winner-first, because that is the order `Vfs::resolve` walks them in, and this line
 *  used to print them in that order under the words "highest priority first". Robin, twice:
 *  *"in MO2, the next to last mod read is Distant Land, and the very last is overwrite, and
 *  later always win and overwrite files from earlier"*, and then *"I still don't like that
 *  the MO2 reading feels inverted"*. He is right that it reads backwards against the tool he
 *  compares it with, and a diagnostic you have to mentally flip is one you stop trusting.
 *
 *  So the order is reversed **here**, at the point of display, and the count of everything
 *  below the six goes in *front* of them rather than after — which is what keeps the
 *  winners on the line while reading in MO2's direction. Nothing reverses the array itself:
 *  `slice` copies first, and `r.roots` is still the engine's own order for anything else
 *  that reads it.
 *
 *  Kept as a function rather than inline in the log's array literal so that a test can ask
 *  it the one thing that matters — that the last name is the one whose files win. */
function overlayLogLine(roots){
  const all=roots||[];
  const top=all.slice(0, 6).reverse();
  const rest=all.length-top.length;
  return (rest>0? T('connect.log_overlay_more',{n:rest})+' → ' : '')+top.join(' → ');
}



/** What the shell knows before anything is on screen: the install to reopen, and
    where openmw.cfg lives on this machine. */
let Startup={install:null, openmwCfg:null, openmwCfgExists:false};


