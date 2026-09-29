
/* =====================================================================================
   Wraithguard's cell map, inside the cell picker: which mods touch which cells.

   The engine answers from the world it already loaded (`cell_coverage`: for each exterior
   grid and each interior, the plugins with a CELL record for it). This draws it the way
   Wraithguard's modmapper-style map does - the same banded heat colours
   (wraithguard/viz/palette.py: coverage_bands / coverage_heat), laid over the map's own
   picture as translucent masks (yellow to red here, easier to see on the map picture) - adds the mods to the map's
   tooltip, and a mods badge to the picker's list rows, interiors included. Cells touched
   by the user's own mods (Wraithguard's subset, `__WG_VIEW__.extra.subset`) are ringed in
   cyan, which stands apart from the heat.
   Copyright (c) 2026 StaticNation.
   ===================================================================================== */
const WgCoverage={
  /** Heat overlay on or off (the picker's Mods button; kept in the viewer profile). */
  on:true,
  plugins:[], ext:new Map(), int:new Map(), worst:0, ours:new Set(), _sig:null, _loading:null,

  // palette.py's bands; the ramp is the picker's own - yellow to red, which reads over
  // the map picture's greens, browns and blues where palette.py's blues did not.
  // Same seven stops and positions as palette.py, same translucency; only the colours
  // run pale yellow to deep red.
  NEUTRAL:'#2c313a', MINE:'#ff9d5c', RING:'#3fd8ff', SINGLE_MAX:5, GROUP:5, MAX_BANDS:16, ALPHA:0.42,
  STOPS:[[0.0,1.0,0.96,0.55],[0.17,1.0,0.9,0.35],[0.34,1.0,0.78,0.2],[0.5,1.0,0.62,0.12],
         [0.67,0.96,0.45,0.1],[0.84,0.88,0.27,0.08],[1.0,0.72,0.08,0.06]],

  /** Reads the coverage for the world that is loaded, once per world. */
  load(){
    if(!Engine.has() || typeof GameData!=='object') return Promise.resolve();
    if(this._sig===GameData.sig && !this._loading) return Promise.resolve();
    if(this._loading) return this._loading;
    const sig=GameData.sig;
    this._loading=Engine.call('cell_coverage').then(r=>{
      this.plugins=r.plugins||[];
      this.ext=new Map((r.ext||[]).map(([x,y,ix])=>[x+','+y, ix]));
      this.int=new Map((r.int||[]).map(([n,ix])=>[String(n).toLowerCase(), ix]));
      this.worst=0;
      for(const v of this.ext.values()) if(v.length>this.worst) this.worst=v.length;
      const sub=((window.__WG_VIEW__||{}).extra||{}).subset||[];
      this.ours=new Set(sub.map(s=>String(s).toLowerCase()));
      this._sig=sig;
    }).catch(e=>console.warn('cell coverage:',e)).finally(()=>{ this._loading=null; if(typeof CellMap==='object') CellMap.redraw(); });
    return this._loading;
  },

  /** The plugin names touching a cell: `{kind:'ext',x,y}` or `{kind:'int',name}`. */
  mods(c){
    if(!c) return [];
    const ix=c.kind==='int'? this.int.get(String(c.name||'').toLowerCase()) : this.ext.get(c.x+','+c.y);
    return (ix||[]).map(i=>this.plugins[i]).filter(Boolean);
  },
  isOurs(name){ return this.ours.has(String(name).toLowerCase()); },

  /* ---- palette.py's bands and ramp ---- */
  bands(worst){
    if(worst<1) return [];
    const out=[];
    for(let n=1;n<=Math.min(worst,this.SINGLE_MAX);n++) out.push([n,n]);
    let low=this.SINGLE_MAX+1;
    while(low<=worst){
      const high=low+this.GROUP-1;
      if(out.length+1>=this.MAX_BANDS && high<worst){ out.push([low,null]); return out; }
      out.push([low,Math.min(high,worst)]); low=high+1;
    }
    return out;
  },
  bandIndex(count,worst){
    const b=this.bands(worst); if(!b.length) return 0;
    for(let i=0;i<b.length;i++){ const [lo,hi]=b[i]; if(count>=lo && (hi==null || count<=hi)) return i; }
    return count<b[0][0]? 0 : b.length-1;
  },
  ramp(t){
    const S=this.STOPS;
    for(let i=0;i<S.length-1;i++){
      const [p0,r0,g0,b0]=S[i], [p1,r1,g1,b1]=S[i+1];
      if(t<=p1){ const u=(t-p0)/((p1-p0)||1); return [r0+(r1-r0)*u, g0+(g1-g0)*u, b0+(b1-b0)*u]; }
    }
    return S[S.length-1].slice(1);
  },
  /** The heat colour of a count, `[r,g,b]` 0..255. */
  heat(count){
    const b=this.bands(this.worst);
    const c= b.length<=1? this.STOPS[0].slice(1) : this.ramp(Math.min(1,Math.max(0,this.bandIndex(count,this.worst)/(b.length-1))));
    return c.map(v=>Math.round(v*255));
  },

  /** The heat masks over the map picture, and the "yours" rings. Called from CellMap.draw. */
  drawOverlay(ctx, map){
    if(!this.on || !this.ext.size || !map.view) return;
    const W=map.W, H=map.H;
    const ours=[];
    for(const [k,ix] of this.ext){
      const i=k.indexOf(','), x=+k.slice(0,i), y=+k.slice(i+1);
      const [rx,ry,rw,rh]=map.rect(x,y);
      if(rx+rw<0||ry+rh<0||rx>W||ry>H) continue;
      const [r,g,b]=this.heat(ix.length);
      ctx.fillStyle='rgba('+r+','+g+','+b+','+this.ALPHA+')';
      ctx.fillRect(rx,ry,rw,rh);
      if(this.ours.size && ix.some(p=>this.isOurs(this.plugins[p]))) ours.push([rx,ry,rw,rh]);
    }
    if(ours.length){
      // Cyan, not the toolkit's orange: that would vanish into the yellow-to-red heat.
      ctx.strokeStyle=this.RING; ctx.lineWidth=1.5; ctx.beginPath();
      for(const [rx,ry,rw,rh] of ours) ctx.rect(rx+1,ry+1,rw-2,rh-2);
      ctx.stroke();
    }
  },

  /** The tooltip's mods section: how many, and which (yours marked), up to a dozen. */
  tip(c){
    const m=this.mods(c); if(!m.length) return '';
    const shown=m.slice(0,12).map(n=>'  '+(this.isOurs(n)? '★ ' : '')+n);
    if(m.length>12) shown.push('  … '+(m.length-12)+' more');
    return '\n'+m.length+(m.length===1? ' mod' : ' mods')+' touch this cell:\n'+shown.join('\n');
  },

  /** A badge for a picker list row: the count, the mods in its title. Null when none. */
  badge(c){
    const m=this.mods(c); if(!m.length) return null;
    const b=document.createElement('span');
    const [r,g,bl]=this.heat(m.length);
    b.className='wgcov';
    b.style.cssText='flex:0 0 auto;font-size:10px;padding:0 6px;border-radius:8px;'+
      'background:rgba('+r+','+g+','+bl+',0.55);color:var(--tx)'+
      (m.some(n=>this.isOurs(n))? ';box-shadow:0 0 0 1px '+this.RING : '');
    b.textContent=m.length+(m.length===1? ' mod' : ' mods');
    b.title=m.map(n=>(this.isOurs(n)? '★ ' : '')+n).join('\n');
    return b;
  },
};
