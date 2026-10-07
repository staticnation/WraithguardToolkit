/* =====================================================================================
   Wraithguard: path grid editing in the render window (the Editor's "Path grid" mode).

   The Construction Set's path grid mode, over the loaded cells: each cell's PGRD points
   drawn with their links, and edited where they stand.

     Click a point            select it (Shift+click: add it to the selection)
     Drag a point             move it (Z held: up and down; Alt: onto the surface under
                              the pointer; the grid snap applies)
     Shift+click the ground   a new point there, linked to the one selected
     Ctrl+click a point       link it to the selected point, or unlink the two
     J / U                    link the selected points in a chain / unlink them
     Delete                   delete the selected points and their links
     Ctrl+Z / Ctrl+Y          undo / redo (path grid changes only, while the mode is on)
     (right-click the button) points from the navmesh: one per walkable polygon of the
                              cell on screen, linked as the mesh joins them
     Esc                      let the selection go; again, leave the mode

   Every change queues the cell's whole path grid in Wraithguard's patch pool
   (`editPathgridSet`; `EditorSession.set_pathgrid`), a cell without one getting the
   patch's own. Points are kept in world units here; an exterior's go to the record
   relative to its cell's south-west corner, as the record holds them. A link joins two
   points of one cell's grid. With the navmesh overlay on (Tools), the navigator's mesh
   is under the grid to place it by.
   ===================================================================================== */
const WgPath={
  on:false,
  grids:new Map(),     // spec -> {spec, grid, base:[x,y], pts:[[x,y,z] world], edges:[[a,b]], queued, plugins}
  sel:[],              // [{spec, i}]
  undoStack:[], redoStack:[],
  PICK_PX:12,

  E(){ return WgEditor; },
  origin(){ const s=App._scene; return (s && s.origin)? s.origin : [0,0]; },
  specs(){ return (typeof WgTools==='object')? WgTools.specs() : []; },

  async toggle(){ return this.on? this.stop() : this.start(); },

  async start(){
    const E=this.E();
    if(!E.canEdit()) return toast('Path grid editing needs Wraithguard: open the viewer from Wraithguard (Cell Preview)','warn',5000);
    if(!App.R) return;
    this.on=true; this.sel=[]; this.undoStack=[]; this.redoStack=[];
    this.hook();
    this.button();
    await this.load();
    toast('Path grid: click a point to select, drag to move, Shift+click the ground to add, Ctrl+click to link (Keys lists the rest)','ok',5000);
  },

  stop(){
    this.on=false; this.sel=[];
    if(App.R) App.R.setOverlay('pgedit',null);
    this.button();
  },

  /** Clicks in the render window go to the mode while it is on. */
  hook(){
    const R=App.R; if(!R || R._wgPathHooked===R.onPick) return;
    const prev=R.onPick;
    R.onPick=(hit,e)=>{ if(this.on && e) return this.click(e); if(prev) prev(hit,e); };
    R._wgPathHooked=R.onPick;
  },

  button(){
    const b=document.getElementById('edPathBtn');
    if(b) b.classList.toggle('pri', this.on);
  },

  /** Every loaded cell's grid, as the patch would write it. */
  async load(){
    const E=this.E(), specs=this.specs();
    // A newer load (the cells changed again while this one waited) wins: its grids
    // are the loaded cells', this one's are not.
    const turn=this._loadN=(this._loadN||0)+1;
    this.grids.clear(); this.sel=[];
    this.draw();
    const got=new Map();
    for(const spec of specs){
      try{ got.set(spec, await E.ask('editPathgrid', {cell:spec})); }
      catch(e){ if(turn===this._loadN) toast(String(e.message||e),'err',4000); }
      if(turn!==this._loadN) return;
    }
    this.grids.clear();
    for(const [spec, v] of got) this.take(spec, v);
    this.draw();
  },

  /** A view from Wraithguard (`pathgrid_view`) kept in world units. */
  take(spec, v){
    const base= v.grid? [v.grid[0]*CELL, v.grid[1]*CELL] : [0,0];
    this.grids.set(spec, {spec, grid:v.grid||null, base, queued:!!v.queued, plugins:v.plugins||[],
      pts:(v.points||[]).map(p=>[p[0]+base[0], p[1]+base[1], p[2]]), edges:(v.edges||[]).map(e=>[e[0],e[1]])});
  },

  /** The loaded cell a world position falls in (an interior: the one interior). */
  cellAt(p){
    for(const g of this.grids.values()){
      if(!g.grid) return g;
      if(p[0]>=g.base[0] && p[0]<g.base[0]+CELL && p[1]>=g.base[1] && p[1]<g.base[1]+CELL) return g;
    }
    return null;
  },

  /** The point nearest the pointer on screen, within PICK_PX: `{spec, i}` or null. */
  pointAt(x, y){
    const proj=this.E().projector(); if(!proj) return null;
    const o=this.origin();
    let best=null, bd=this.PICK_PX;
    for(const g of this.grids.values()){
      g.pts.forEach((p,i)=>{
        const s=proj([p[0]-o[0], p[1]-o[1], p[2]]); if(!s) return;
        const d=Math.hypot(s[0]-x, s[1]-y);
        if(d<bd){ bd=d; best={spec:g.spec, i}; }
      });
    }
    return best;
  },

  isSel(spec, i){ return this.sel.some(s=>s.spec===spec && s.i===i); },

  draw(){
    const R=App.R; if(!R) return;
    if(!this.on){ R.setOverlay('pgedit',null); return; }
    const o=this.origin(), L=new OvlLines();
    const at=p=>[p[0]-o[0], p[1]-o[1], p[2]+10];
    for(const g of this.grids.values()){
      const has=new Set(g.edges.map(e=>e[0]+':'+e[1]));
      for(const [a,b] of g.edges){
        if(!g.pts[a] || !g.pts[b]) continue;
        const both=has.has(b+':'+a);
        if(both && a>b) continue;              // a two-way link once
        L.seg(at(g.pts[a]), at(g.pts[b]), both? (g.queued? [0.45,1,0.6] : [0.3,0.9,1]) : [1,0.55,0.2]);
      }
      g.pts.forEach((p,i)=>{
        const sel=this.isSel(g.spec,i);
        L.cross(at(p), sel? 22 : 14, sel? [1,1,1] : [1,0.9,0.3]);
        if(sel) L.circle(at(p), 26, [1,1,1], 'z', 16);
      });
    }
    R.setOverlay('pgedit', L, {xray:true});
    R.dirty=true;
  },

  snapshot(spec){
    const g=this.grids.get(spec);
    return {spec, pts:g.pts.map(p=>p.slice()), edges:g.edges.map(e=>e.slice())};
  },
  /** Before a change: what undo puts back. */
  remember(spec){ this.undoStack.push(this.snapshot(spec)); if(this.undoStack.length>200) this.undoStack.shift(); this.redoStack=[]; },

  async restore(from, to){
    const s=from.pop(); if(!s) return toast('Nothing to '+(from===this.undoStack? 'undo' : 'redo'),'ok',1500);
    to.push(this.snapshot(s.spec));
    const g=this.grids.get(s.spec); if(!g) return;
    g.pts=s.pts; g.edges=s.edges; this.sel=[];
    await this.send(s.spec);
  },

  /** The cell's grid to the pool; Wraithguard's answer is what is drawn after. */
  async send(spec){
    const g=this.grids.get(spec); if(!g) return;
    const body={cell:spec, points:g.pts.map(p=>[Math.round(p[0]-g.base[0]), Math.round(p[1]-g.base[1]), Math.round(p[2])]), edges:g.edges};
    this.draw();
    try{
      this.take(spec, await this.E().ask('editPathgridSet', body));
      this.E().refreshPending();
    }catch(e){ toast(String(e.message||e),'err',5000); }
    this.draw();
  },

  link(g, a, b, on){
    g.edges=g.edges.filter(e=>!((e[0]===a && e[1]===b) || (e[0]===b && e[1]===a)));
    if(on){ g.edges.push([a,b]); g.edges.push([b,a]); }
  },
  linked(g, a, b){ return g.edges.some(e=>(e[0]===a && e[1]===b) || (e[0]===b && e[1]===a)); },

  /** A still click in the render window. */
  async click(e){
    const hit=this.pointAt(e.clientX, e.clientY);
    if(hit && (e.ctrlKey||e.metaKey)){
      const last=this.sel[this.sel.length-1];
      if(!last) { this.sel=[hit]; return this.draw(); }
      if(last.spec!==hit.spec) return toast('A link joins two points of one cell\'s path grid','warn',3000);
      if(last.i===hit.i) return;
      const g=this.grids.get(hit.spec);
      this.remember(hit.spec);
      this.link(g, last.i, hit.i, !this.linked(g, last.i, hit.i));
      this.sel=[hit];
      return this.send(hit.spec);
    }
    if(hit){
      if(e.shiftKey){ if(this.isSel(hit.spec,hit.i)) this.sel=this.sel.filter(s=>!(s.spec===hit.spec && s.i===hit.i)); else this.sel.push(hit); }
      else this.sel=[hit];
      return this.draw();
    }
    if(e.shiftKey) return this.add(e.clientX, e.clientY);
    this.sel=[]; this.draw();
  },

  /** A new point on the surface under the pointer, linked to the one point selected. */
  async add(x, y){
    const E=this.E(), at=E.placePoint(x, y); if(!at) return;
    const o=this.origin();
    let w=at.world.slice();
    if(E.gridOn){ const s=E.snapPos([at.local[0], at.local[1], at.local[2]], [o[0],o[1],0], [true,true,false]); w=[s[0]+o[0], s[1]+o[1], s[2]]; }
    const g=this.cellAt(w);
    if(!g) return toast('That is outside the loaded cells','warn',2500);
    this.remember(g.spec);
    g.pts.push(w);
    const i=g.pts.length-1;
    const one=this.sel.length===1 && this.sel[0].spec===g.spec? this.sel[0].i : null;
    if(one!=null) this.link(g, one, i, true);
    this.sel=[{spec:g.spec, i}];
    await this.send(g.spec);
  },

  async remove(){
    if(!this.sel.length) return;
    const by=new Map();
    for(const s of this.sel){ if(!by.has(s.spec)) by.set(s.spec, new Set()); by.get(s.spec).add(s.i); }
    this.sel=[];
    for(const [spec, gone] of by){
      const g=this.grids.get(spec); if(!g) continue;
      this.remember(spec);
      const map=[]; let n=0;
      g.pts.forEach((_,i)=>{ map[i]= gone.has(i)? -1 : n++; });
      g.pts=g.pts.filter((_,i)=>!gone.has(i));
      g.edges=g.edges.filter(e=>map[e[0]]>=0 && map[e[1]]>=0).map(e=>[map[e[0]], map[e[1]]]);
      await this.send(spec);
    }
  },

  /** J: the selected points (one cell's) joined in the order picked; U: all their links
   *  to each other taken away. */
  async chain(on){
    const specs=new Set(this.sel.map(s=>s.spec));
    if(this.sel.length<2) return toast('Select two or more points first (Shift+click)','warn',2500);
    if(specs.size>1) return toast('A link joins two points of one cell\'s path grid','warn',3000);
    const spec=this.sel[0].spec, g=this.grids.get(spec);
    this.remember(spec);
    if(on){ for(let k=1;k<this.sel.length;k++) this.link(g, this.sel[k-1].i, this.sel[k].i, true); }
    else{ const ids=new Set(this.sel.map(s=>s.i)); g.edges=g.edges.filter(e=>!(ids.has(e[0]) && ids.has(e[1]))); }
    await this.send(spec);
  },

  /** The renderer's `onGrab` while the mode is on: a press on a point drags it. */
  grab(e){
    const R=App.R, E=this.E(); if(!R) return null;
    if(e.ctrlKey || e.metaKey || e.shiftKey) return null;
    const hit=this.pointAt(e.clientX, e.clientY); if(!hit) return null;
    const g=this.grids.get(hit.spec), o=this.origin();
    const p0=g.pts[hit.i].slice();
    const local=[p0[0]-o[0], p0[1]-o[1], p0[2]], off=[o[0], o[1], 0];
    const st={cur:local.slice(), moved:false, x0:e.clientX, y0:e.clientY, start:E.onPlane(R.rayAt(e.clientX,e.clientY), local[2])};
    if(!this.isSel(hit.spec, hit.i)) this.sel=[hit];
    return {
      move:ev=>{
        if(ev.altKey){
          const at=E.surfaceUnder(ev.clientX, ev.clientY, null); if(!at) return;
          st.cur=E.snapPos(at.p.slice(), off, [true,true,false]);
        }else if(E.held.has('z')){
          const ray=R.rayAt(ev.clientX,ev.clientY);
          const dist=Math.hypot(ray.eye[0]-local[0], ray.eye[1]-local[1], ray.eye[2]-local[2]);
          const H=(R.cv && R.cv.clientHeight)||800;
          const perPx=2*Math.tan((typeof FOV==='number'? FOV : 1)/2)*dist/H;
          st.cur=[local[0], local[1], local[2]-(ev.clientY-st.y0)*perPx];
        }else{
          const p=E.onPlane(R.rayAt(ev.clientX,ev.clientY), local[2]);
          if(!p || !st.start) return;
          st.cur=E.snapPos([local[0]+p[0]-st.start[0], local[1]+p[1]-st.start[1], local[2]], off, [true,true,false]);
        }
        st.moved=true;
        g.pts[hit.i]=[st.cur[0]+o[0], st.cur[1]+o[1], st.cur[2]];
        this.draw();
      },
      end:()=>{
        if(!st.moved) return;
        g.pts[hit.i]=p0; this.remember(hit.spec);
        g.pts[hit.i]=[st.cur[0]+o[0], st.cur[1]+o[1], st.cur[2]];
        this.send(hit.spec);
      },
      cancel:()=>{ g.pts[hit.i]=p0; this.draw(); },
    };
  },

  keys(e){
    if(!this.on) return false;
    const typing=/^(INPUT|TEXTAREA|SELECT)$/.test((document.activeElement||{}).tagName||'');
    if(typing || e.altKey) return false;
    const k=String(e.key||'').toLowerCase(), mod=e.ctrlKey||e.metaKey;
    // Wraithguard's history covers the path grid too (one Ctrl+Z for everything);
    // this mode's own steps are for a Wraithguard without it.
    const pool=this.E().links && this.E().links().editUndo;
    if(mod && k==='z'){ if(pool){ if(e.shiftKey) this.E().redo(); else this.E().undo(); } else if(e.shiftKey) this.restore(this.redoStack, this.undoStack); else this.restore(this.undoStack, this.redoStack); return true; }
    if(mod && k==='y'){ if(pool) this.E().redo(); else this.restore(this.redoStack, this.undoStack); return true; }
    if(mod) return false;
    if(e.key==='Delete' || e.key==='Backspace'){ this.remove(); return true; }
    if(k==='j'){ this.chain(true); return true; }
    if(k==='u'){ this.chain(false); return true; }
    if(e.key==='Escape'){ if(this.sel.length){ this.sel=[]; this.draw(); } else this.stop(); return true; }
    return false;
  },

  /** The cell's path grid made from OpenMW's navmesh, one for one: a point for each
   *  walkable polygon of the mesh (Tools' Navmesh overlay, its chosen actor size) whose
   *  middle is in the cell on screen, at that middle; and a link, both ways, for each
   *  pair of polygons the mesh joins - an edge they share inside a tile, or an
   *  overlapping stretch of a tile's border where the navigator joins neighbouring tiles.
   *  Nothing is sampled or thinned. Replaces the cell's grid - Ctrl+Z puts it back. */
  async fromNavmesh(){
    const T=(typeof WgTools==='object')? WgTools : null;
    if(!T) return;
    if(!this.on) await this.start();
    let d=T.nav && T.nav.data;
    if(!d){
      try{ d=T.nav.data=T.readNav(await Engine.bytes('navmesh', {cells:T.specs(), db:T.navDb()})); }
      catch(e){ return toast(String(e.message||e),'err',6000); }
    }
    const cur=this.E().currentCell();
    const spec=!cur? null : /^\(/.test(cur.key)? cur.key.replace(/[() ]/g,'') : 'int:'+cur.key;
    const g=spec && this.grids.get(spec);
    if(!g) return toast('No cell on screen to fill','warn',3000);
    const out=this.navGraph(d, (T.nav && T.nav.agent)||0, g.grid? [g.base[0], g.base[1], g.base[0]+CELL, g.base[1]+CELL] : null);
    if(!out.pts.length) return toast('The navmesh has nothing walkable in this cell (is the overlay\'s actor size the one you want?)','warn',4000);
    if(out.pts.length>65535) return toast('That is '+out.pts.length+' polygons: more than a path grid holds','warn',5000);
    this.remember(spec);
    g.pts=out.pts; g.edges=out.edges; this.sel=[];
    await this.send(spec);
    toast(out.pts.length+' points (one per navmesh polygon) and '+(out.edges.length/2)+' links in '+cur.label+' (Ctrl+Z puts the old grid back)','ok',6000);
  },

  /** The navmesh's polygon graph: `{pts, edges}` - a point per walkable polygon of the
   *  agent with its middle inside `box` ([x0, y0, x1, y1], world; null: all), edges
   *  `[a, b]` both ways between polygons the mesh joins. Inside a tile, Recast's own
   *  neighbour record says which (exact); across tiles, a portal edge joins the portal
   *  edges of the tile next to it that overlap it along their shared border, at about
   *  the same height - as the navigator connects tiles. */
  navGraph(d, agent, box){
    const P=d.polys.filter(p=>p.a===agent && (p.flags&1));
    const mids=P.map(p=>{ let x=0,y=0,z=0; const n=p.v.length/3; for(let i=0;i<p.v.length;i+=3){ x+=p.v[i]; y+=p.v[i+1]; z+=p.v[i+2]; } return [x/n, y/n, z/n]; });
    const keep=mids.map(m=>!box || (m[0]>=box[0] && m[0]<box[2] && m[1]>=box[1] && m[1]<box[3]));
    const ix=new Map(), pts=[];
    mids.forEach((m,i)=>{ if(keep[i]){ ix.set(i, pts.length); pts.push([Math.round(m[0]), Math.round(m[1]), Math.round(m[2])]); } });
    const links=new Set();
    const join=(a,b)=>{ if(a===b || !keep[a] || !keep[b]) return; const A=ix.get(a), B=ix.get(b); links.add(A<B? A+','+B : B+','+A); };
    const exact=P.length && P[0].nei;
    const byLocal=new Map(); P.forEach((p,i)=>{ if(exact) byLocal.set(p.tile+':'+p.index, i); });
    const portals=new Map();       // border line -> edges on it
    const W=d.tileW||0;
    P.forEach((p,pi)=>{
      const n=p.v.length/3;
      for(let e=0;e<n;e++){
        const f=(e+1)%n;
        let portal=false;
        if(exact){
          const nb=p.nei[e];
          if(nb===0xffff) continue;
          if(!(nb&0x8000)){ const q=byLocal.get(p.tile+':'+nb); if(q!=null) join(pi, q); continue; }
          portal=true;
        }
        // A border edge (from the record, or - without it - lying on a tile line).
        const x0=p.v[e*3], y0=p.v[e*3+1], z0=p.v[e*3+2], x1=p.v[f*3], y1=p.v[f*3+1], z1=p.v[f*3+2];
        const onLine=a=>W>0 && Math.abs(a/W-Math.round(a/W))*W<0.6;
        let line=null, u0, u1;
        if(Math.abs(x0-x1)<0.6 && (portal || onLine(x0))){ line='x'+Math.round(x0/(W||1)); u0=y0; u1=y1; }
        else if(Math.abs(y0-y1)<0.6 && (portal || onLine(y0))){ line='y'+Math.round(y0/(W||1)); u0=x0; u1=x1; }
        if(!line) continue;
        const seg= u0<=u1? {pi, tile:p.tile, lo:u0, hi:u1, zlo:z0, zhi:z1} : {pi, tile:p.tile, lo:u1, hi:u0, zlo:z1, zhi:z0};
        if(!portals.has(line)) portals.set(line, []); portals.get(line).push(seg);
      }
    });
    // No neighbour record (an older engine's answer): edges shared corner for corner.
    if(!exact){
      const key=(v,i)=>Math.round(v[i*3]*2)+','+Math.round(v[i*3+1]*2)+','+Math.round(v[i*3+2]*2);
      const byEdge=new Map();
      P.forEach((p,pi)=>{ const n=p.v.length/3; for(let i=0;i<n;i++){ const a=key(p.v,i), b=key(p.v,(i+1)%n), k=a<b? a+'|'+b : b+'|'+a; if(!byEdge.has(k)) byEdge.set(k, []); byEdge.get(k).push(pi); } });
      for(const list of byEdge.values()) for(let a=0;a<list.length;a++) for(let b=a+1;b<list.length;b++) join(list[a], list[b]);
    }
    // Height of a border edge at a point along it.
    const zAt=(s,u)=> s.hi-s.lo<1e-6? (s.zlo+s.zhi)/2 : s.zlo+(s.zhi-s.zlo)*(u-s.lo)/(s.hi-s.lo);
    for(const list of portals.values()){
      list.sort((a,b)=>a.lo-b.lo);
      for(let a=0;a<list.length;a++) for(let b=a+1;b<list.length && list[b].lo<list[a].hi;b++){
        const A=list[a], B=list[b];
        if(exact && A.tile===B.tile) continue;          // the same tile's own border pieces
        const lo=Math.max(A.lo,B.lo), hi=Math.min(A.hi,B.hi);
        if(hi-lo<=0.01) continue;
        if(Math.abs(zAt(A,lo)-zAt(B,lo))<=32 || Math.abs(zAt(A,hi)-zAt(B,hi))<=32) join(A.pi, B.pi);
      }
    }
    const edges=[];
    for(const k of links){ const [a,b]=k.split(',').map(Number); edges.push([a,b],[b,a]); }
    return {pts, edges};
  },

  /** Drops the queued grid of the cells in view (Wraithguard's copy comes back). */
  async revertAll(){
    for(const spec of this.grids.keys()){
      try{ this.take(spec, await this.E().ask('editPathgridSet', {cell:spec, revert:true})); }
      catch(e){ toast(String(e.message||e),'err',4000); }
    }
    this.sel=[]; this.undoStack=[]; this.redoStack=[];
    this.E().refreshPending();
    this.draw();
  },
};
