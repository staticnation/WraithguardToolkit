
/* =====================================================================================
   Wraithguard viewport settings: zoom to cursor, a frame-rate cap, and pausing the
   renderer while the view cannot be seen.

   The renderer (06_gl.js) reads `opts.zoomToCursor` and `opts.fpsCap`, and asks
   `R.covered()` each frame; this binds the Settings controls, keeps them in the viewer
   profile (PrevSettings' `[viewport]`: zoom_cursor, fps_cap), and answers `covered`.
   Copyright (c) 2026 StaticNation.
   ===================================================================================== */
const WgViewport={
  /** How much of the viewport an open dialogue must hide before drawing stops. */
  COVER:0.6,
  _at:0, _was:false,

  /** Whether an open dialogue covers most of the viewport. Asked every frame by the
      renderer, so it answers from a cache and measures at most four times a second - and
      not at all while no dialogue is open. */
  covered(){
    const open=document.querySelector('.modal:not([hidden])');
    if(!open) return (this._was=false);
    const now=performance.now();
    if(now-this._at<250) return this._was;
    this._at=now;
    const vp=$('#vpwrap'); if(!vp) return (this._was=false);
    const v=vp.getBoundingClientRect(), area=v.width*v.height;
    if(!(area>0)) return (this._was=false);
    let hidden=0;
    for(const m of document.querySelectorAll('.modal:not([hidden])')){
      const box=m.querySelector('.mbox')||m;
      const r=box.getBoundingClientRect();
      const w=Math.max(0,Math.min(r.right,v.right)-Math.max(r.left,v.left));
      const h=Math.max(0,Math.min(r.bottom,v.bottom)-Math.max(r.top,v.top));
      hidden=Math.max(hidden,w*h);
    }
    return (this._was=hidden/area>=this.COVER);
  },

  /** The Settings controls, and the renderer's hook. Called once from `boot`. */
  bind(){
    if(App.R) App.R.covered=()=>this.covered();
    // Zoom to cursor is on unless the viewer profile says otherwise.
    if(App.R && App.R.opts.zoomToCursor==null) App.R.opts.zoomToCursor=true;
    { const zc=$('#setZoomCursor'); if(zc && App.R) zc.checked=!!App.R.opts.zoomToCursor; }
    const zc=$('#setZoomCursor');
    if(zc) zc.onchange=()=>{ if(App.R) App.R.opts.zoomToCursor=zc.checked; this.touch(); };
    // Normal and specular maps in cells: read with the meshes from now on, and the meshes
    // already read are read again, so the scene has them at once.
    const nm=$('#setNrmMaps');
    if(nm) nm.onchange=()=>{ App._nrmChosen=true; this.setCellNormalMaps(nm.checked); this.touch(); };
    const pd=$('#setPad');
    if(pd) pd.onchange=()=>{ if(typeof WgPad==='object') WgPad.setEnabled(pd.checked); this.touch(); };
    const fc=$('#setFpsCap');
    if(fc) fc.onchange=()=>{ if(App.R){ App.R.opts.fpsCap=+fc.value||0; App.R.dirty=true; } this.touch(); };
    // Ambient occlusion's mode: SSAO or SSGI, one or the other (29_ssao.js).
    const am=$('#p_aomode');
    if(am) am.onchange=()=>{ if(App.R){ App.R.opts.ssgi=am.value==='ssgi'; App.R.dirty=true; } this.touch(); };
    // The cell map, one click away from inside a cell: the picker, opened on its map.
    const mb=$('#tbMap');
    if(mb) mb.onclick=()=>{ App.cellPick='map'; if(typeof openCellPicker==='function') openCellPicker(); };
    // Back from minimized or hidden: the frame that was owed is drawn now.
    document.addEventListener('visibilitychange',()=>{ if(!document.hidden && App.R) App.R.dirty=true; });
  },

  setCellNormalMaps(on){
    App.cellNormalMaps=!!on;
    if(typeof WgMeshView==='object' && WgMeshView.active()) return;   // the mesh viewer has its own
    App.loadNormalMaps=!!on;
    if(App.R){ App.R.opts.normalMaps=!!on; App.R.dirty=true; }
    if(on && App.meshCache && App.meshCache.size){
      // Cached meshes were read without them: dropped (with their GPU objects) so the next
      // build reads them again.
      if(typeof dropMeshes==='function') dropMeshes(); else App.meshCache.clear();
      App._scene=null;
      if(typeof schedulePreview==='function') schedulePreview();
    }
  },

  touch(){ if(typeof PrevSettings==='object') PrevSettings.touch(); },

  /** Wraithguard: cells load without the normal and specular maps unless the switch is
      turned on. Following the setup's settings.cfg turned them on for most installs, and
      a big cell (Balmora, ~600 meshes) then decoded two or three extra textures per mesh
      and ran the page out of memory. The mesh viewer keeps them; the profile remembers
      the switch once touched. */
  installMaps(){
    if(App._nrmChosen) return;
    if(App.cellNormalMaps) this.setCellNormalMaps(false);
    const nm=$('#setNrmMaps'); if(nm) nm.checked=!!App.cellNormalMaps;
  },

  /** The two values, for the viewer profile. */
  save(){
    const o=(App.R&&App.R.opts)||{};
    // `zoom_cursor`, not the first build's `zoom_to_cursor`: that one saved `false` into
    // profiles before the option defaulted on, and is ignored.
    return [['zoom_cursor',o.zoomToCursor!==false], ['fps_cap',Math.max(0,+o.fpsCap||0)],
            ['ao_mode',o.ssgi? 'ssgi' : 'ssao'],
            ['gamepad', typeof WgPad==='object'? WgPad.enabled!==false : true],
            ['cov_heat', typeof WgCoverage==='object'? !!WgCoverage.on : true],
            ['cov_mode', typeof WgCoverage==='object'? String(WgCoverage.mode==='plugin'? 'mods' : WgCoverage.mode) : 'mods'],
            // `cell_maps`, not `normal_maps_cells`: that one saved `true` into profiles
            // while cells followed the setup, and is ignored now that they default off.
            ...(App._nrmChosen? [['cell_maps', !!App.cellNormalMaps]] : [])];
  },

  /** Back from the viewer profile. */
  load(V){
    if(!V) return;
    if(V.zoom_cursor!=null && App.R) App.R.opts.zoomToCursor=!!V.zoom_cursor;
    if(V.fps_cap!=null && App.R) App.R.opts.fpsCap=Math.max(0,+V.fps_cap||0);
    if(V.ao_mode!=null && App.R) App.R.opts.ssgi=String(V.ao_mode)==='ssgi';
    if(V.gamepad!=null && typeof WgPad==='object') WgPad.setEnabled(!!V.gamepad);
    { const pd=$('#setPad'); if(pd && typeof WgPad==='object') pd.checked=WgPad.enabled!==false; }
    if(V.cov_heat!=null && typeof WgCoverage==='object') WgCoverage.on=!!V.cov_heat;
    if(V.cov_mode!=null && typeof WgCoverage==='object' && ['mods','conflicts','land'].includes(String(V.cov_mode))) WgCoverage.mode=String(V.cov_mode);
    if(V.cell_maps!=null){ App._nrmChosen=true; this.setCellNormalMaps(!!V.cell_maps); }
    { const nm=$('#setNrmMaps'); if(nm) nm.checked=!!App.cellNormalMaps; }
    const o=(App.R&&App.R.opts)||{};
    const am=$('#p_aomode'); if(am) am.value=o.ssgi? 'ssgi' : 'ssao';
    const zc=$('#setZoomCursor'); if(zc) zc.checked=!!o.zoomToCursor;
    const fc=$('#setFpsCap'); if(fc) fc.value=String(o.fpsCap||0);
  },
};
