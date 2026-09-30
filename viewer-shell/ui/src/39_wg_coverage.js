
/* =====================================================================================
   Wraithguard's cell map, inside the cell picker: which mods touch which cells.

   The engine answers from the world it already loaded (`cell_coverage`: for each exterior
   grid and each interior, the plugins with a CELL record for it; and per exterior, how
   many of its references are contested and how many plugins edit its ground). This lays
   a heat over the map's own picture as translucent masks, adds the details to the map's
   tooltip, and a badge to the picker's list rows, interiors included. Cells touched by
   the user's own mods (Wraithguard's subset, `__WG_VIEW__.extra.subset`) are ringed in
   cyan, which stands apart from the heat.

   The heat runs pale yellow through orange to deep red, spread by rank: a cell's colour
   is how many cells have less than it. Counting straight (1..worst) put nearly every
   cell in the first band - most cells have one or two mods and a few have forty - so the
   map was all yellow; by rank the common counts sit at the yellow end and the busiest
   cells at the red end, whatever the load order's numbers are.

   What the heat shows (`mode`, the picker's Heat list, kept in the viewer profile):
   `mods` touching each cell; reference `conflicts` (references two or more plugins
   supplied, and ones a plugin deleted or moved away); `land` edits (plugins with a LAND
   for the cell); or one `plugin`'s cells, solid.
   Copyright (c) 2026 StaticNation.
   ===================================================================================== */
const WgCoverage={
  /** Heat overlay on or off, and what it shows. */
  on:true, mode:'mods', focus:'',
  plugins:[], ext:new Map(), int:new Map(), extc:new Map(), worst:0, ours:new Set(), _sig:null, _loading:null,
  _ranks:{},

  NEUTRAL:'#2c313a', MINE:'#ff9d5c', RING:'#3fd8ff', ALPHA:0.5,
  // Pale yellow, yellow, orange, red-orange, red, deep red.
  STOPS:[[0.0,1.0,0.95,0.45],[0.2,1.0,0.84,0.12],[0.42,1.0,0.58,0.05],[0.62,0.96,0.34,0.04],
         [0.82,0.84,0.1,0.05],[1.0,0.55,0.0,0.08]],

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
      this.extc=new Map((r.extc||[]).map(([x,y,c,l])=>[x+','+y, {conf:c|0, land:l|0}]));
      this.worst=0;
      for(const v of this.ext.values()) if(v.length>this.worst) this.worst=v.length;
      const sub=((window.__WG_VIEW__||{}).extra||{}).subset||[];
      this.ours=new Set(sub.map(s=>String(s).toLowerCase()));
      this._ranks={};
      this._sig=sig;
    }).catch(e=>console.warn('cell coverage:',e)).finally(()=>{ this._loading=null; if(typeof CellMap==='object') CellMap.redraw(); });
    return this._loading;
  },

  /** The picker's Heat list and its plugin list, wired to this. */
  bindPicker(){
    const sel=$('#cellHeat'), ps=$('#cellHeatPlugin');
    if(!sel) return;
    sel.value=this.on? this.mode : 'off';
    const fillPlugins=()=>{
      if(!ps) return;
      ps.hidden=sel.value!=='plugin';
      if(ps.options.length!==this.plugins.length){
        ps.innerHTML=this.plugins.map(p=>'<option>'+escHtml(p)+'</option>').join('');
      }
      if(this.focus) ps.value=this.focus;
      if(!this.focus && ps.options.length) this.focus=ps.value;
    };
    fillPlugins();
    sel.onchange=()=>{
      const v=sel.value;
      this.on=v!=='off';
      if(this.on) this.mode=v;
      fillPlugins();
      if(typeof CellMap==='object') CellMap.redraw();
      if(typeof PrevSettings==='object') PrevSettings.touch();
    };
    if(ps) ps.onchange=()=>{ this.focus=ps.value; if(typeof CellMap==='object') CellMap.redraw(); };
  },

  /** Shows `plugin`'s cells on the map (the Links panel, and Wraithguard's plugin list). */
  showPlugin(plugin){
    this.on=true; this.mode='plugin'; this.focus=String(plugin||'');
    App.cellPick='map';
    if(typeof openCellPicker==='function') openCellPicker();
    this.bindPicker();
    if(typeof CellMap==='object') CellMap.redraw();
  },

  /** The plugin names touching a cell: `{kind:'ext',x,y}` or `{kind:'int',name}`. */
  mods(c){
    if(!c) return [];
    const ix=c.kind==='int'? this.int.get(String(c.name||'').toLowerCase()) : this.ext.get(c.x+','+c.y);
    return (ix||[]).map(i=>this.plugins[i]).filter(Boolean);
  },
  isOurs(name){ return this.ours.has(String(name).toLowerCase()); },

  /** A cell's number in the current mode. */
  value(key){
    if(this.mode==='conflicts') return (this.extc.get(key)||{}).conf||0;
    if(this.mode==='land'){ const l=(this.extc.get(key)||{}).land||0; return l>1? l : 0; }
    return (this.ext.get(key)||[]).length;
  },
  /** Every cell's number in `mode`, sorted - what a colour's rank is taken against. */
  ranks(){
    if(this._ranks[this.mode]) return this._ranks[this.mode];
    const keys=new Set([...this.ext.keys(), ...this.extc.keys()]);
    const v=[];
    for(const k of keys){ const n=this.value(k); if(n>0) v.push(n); }
    v.sort((a,b)=>a-b);
    return (this._ranks[this.mode]=v);
  },
  ramp(t){
    const S=this.STOPS;
    for(let i=0;i<S.length-1;i++){
      const [p0,r0,g0,b0]=S[i], [p1,r1,g1,b1]=S[i+1];
      if(t<=p1){ const u=(t-p0)/((p1-p0)||1); return [r0+(r1-r0)*u, g0+(g1-g0)*u, b0+(b1-b0)*u]; }
    }
    return S[S.length-1].slice(1);
  },
  /** Where `n` stands among the cells, 0..1: the share of cells with less than it, and
      the top value at 1. */
  rankOf(n){
    const v=this.ranks(); if(v.length<2) return 0;
    if(n>=v[v.length-1]) return 1;
    if(n<=v[0]) return 0;
    const first=x=>{ let lo=0, hi=v.length; while(lo<hi){ const mid=(lo+hi)>>1; if(v[mid]<x) lo=mid+1; else hi=mid; } return lo; };
    // Among many equal counts, sit halfway up their run rather than at its foot.
    const lo=first(n), hi=first(n+1e-9);
    return Math.max(0, Math.min(1, ((lo+hi-1)/2)/(v.length-1)));
  },
  /** The heat colour of a count, `[r,g,b]` 0..255. */
  heat(count){
    return this.ramp(this.rankOf(count)).map(v=>Math.round(v*255));
  },

  /** The heat masks over the map picture, and the "yours" rings. Called from CellMap.draw. */
  drawOverlay(ctx, map){
    if(!this.on || !map.view) return;
    const W=map.W, H=map.H;
    const ours=[];
    const want=this.mode==='plugin'? this.plugins.findIndex(p=>p.toLowerCase()===this.focus.toLowerCase()) : -1;
    const keys=this.mode==='conflicts'||this.mode==='land'? this.extc.keys() : this.ext.keys();
    for(const k of keys){
      const i=k.indexOf(','), x=+k.slice(0,i), y=+k.slice(i+1);
      const [rx,ry,rw,rh]=map.rect(x,y);
      if(rx+rw<0||ry+rh<0||rx>W||ry>H) continue;
      const ix=this.ext.get(k)||[];
      if(this.mode==='plugin'){
        if(want<0 || !ix.includes(want)) continue;
        ctx.fillStyle='rgba(235,60,20,0.6)';
        ctx.fillRect(rx,ry,rw,rh);
        continue;
      }
      const n=this.value(k); if(!n) continue;
      const [r,g,b]=this.heat(n);
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

  /** The tooltip's mods section: how many, and which (yours marked), up to a dozen; and
      the cell's contested references and ground edits. */
  tip(c){
    const m=this.mods(c);
    let s='';
    if(m.length){
      const shown=m.slice(0,12).map(n=>'  '+(this.isOurs(n)? '★ ' : '')+n);
      if(m.length>12) shown.push('  … '+(m.length-12)+' more');
      s+='\n'+m.length+(m.length===1? ' mod' : ' mods')+' touch this cell:\n'+shown.join('\n');
    }
    if(c && c.kind!=='int'){
      const e=this.extc.get(c.x+','+c.y);
      if(e && e.conf) s+='\n'+e.conf+' contested reference'+(e.conf===1? '' : 's');
      if(e && e.land>1) s+='\n'+e.land+' plugins edit the ground';
    }
    return s;
  },

  /** A badge for a picker list row: the count, the mods in its title. Null when none. */
  badge(c){
    const m=this.mods(c); if(!m.length) return null;
    const b=document.createElement('span');
    const [r,g,bl]=this.ramp(this.rankOfMods(m.length)).map(v=>Math.round(v*255));
    b.className='wgcov';
    b.style.cssText='flex:0 0 auto;font-size:10px;padding:0 6px;border-radius:8px;'+
      'background:rgba('+r+','+g+','+bl+',0.55);color:var(--tx)'+
      (m.some(n=>this.isOurs(n))? ';box-shadow:0 0 0 1px '+this.RING : '');
    b.textContent=m.length+(m.length===1? ' mod' : ' mods');
    b.title=m.map(n=>(this.isOurs(n)? '★ ' : '')+n).join('\n');
    return b;
  },
  /** The list's badges always rank by mods, whatever the map shows. */
  rankOfMods(n){ const was=this.mode; this.mode='mods'; const t=this.rankOf(n); this.mode=was; return t; },
};
