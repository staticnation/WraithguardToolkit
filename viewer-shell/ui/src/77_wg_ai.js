/* =====================================================================================
   Wraithguard: the Preview's actors' AI, the routes and the motion (Static, 2026-11-18:
   "AI ... I feel the scripts will want that too").

   The packages themselves are the scripts' layer's (73_wg_scripts.js `ai`, `actorsFrame`):
   MWScript's AiWander, AiTravel, AiFollow, AiEscort and AiActivate, StartCombat, and OpenMW
   Lua's I.AI through 74_wg_lua.js. This is what carries them out the way the game does:

   - routes over the loaded cells' pathgrids (the `pathgrid` command, the points the game's
     own AI walks): A* from the grid point nearest the actor to the one nearest where it is
     going, the exterior cells' grids joined where their points meet across a border. With no
     grid, a straight line, as before;
   - wander: a grid point within the package's distance of where it began, walked to, then a
     wait with an idle played by the package's idle chances (Idle2 ... Idle9);
   - the walk: `WalkForward` (`RunForward` when it runs) on the actor's mesh while it moves,
     its idle when it stops (73_wg_scripts.js `playAnim`, `by: 'AI'`). A group a script plays
     on it wins: the AI's walk stands aside until it ends.

   The actors still step rather than glide (the scene is rebuilt a few times a second; TODO,
   The viewer). Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
const WgAI={
  grid:null,            // {pts:[[x,y,z]], adj:[[i...]]} over the loaded cells, or null
  _for:'', _busy:null, _busyFor:'',
  /** What an actor's model has no group for (`model|group`), so the walk is not asked again. */
  lacks:new Set(),
  /** Two exterior grids' points this close across a border are one path. */
  JOIN:384,

  /** The loaded cells' pathgrids as one graph, read when the cells change. */
  async load(){
    const cells=(App._scene && App._scene.cells)||[];
    const specs=[...new Set(cells.map(c=>CellKey.spec(c)).filter(Boolean))];
    const key=specs.join('|');
    if(key===this._for) return this.grid;
    if(this._busy && this._busyFor===key) return this._busy;
    this._busyFor=key;
    this._busy=(async()=>{
      let grids=[];
      try{
        /* WGTB (`pathgrid`, commands.rs): `grids` [spec, from], `points` [grid, x, y, z] in
           world units, `edges` [grid, a, b]. */
        const tb=unpackTables(await Engine.bytes('pathgrid',{cells:specs}));
        grids=(tb.grids||[]).map(([spec])=>({spec, points:[], edges:[]}));
        for(const [g,x,y,z] of tb.points||[]) if(grids[g]) grids[g].points.push([+x, +y, +z]);
        for(const [g,a,b] of tb.edges||[]) if(grids[g]) grids[g].edges.push([a|0, b|0]);
      }catch(_){ grids=[]; }
      this.grid=this.build(grids);
      this._for=key; this._busy=null;
      return this.grid;
    })();
    return this._busy;
  },
  /** One graph from the grids: each grid's points and edges (both ways), and the exterior
   *  grids' points that meet across a border joined. */
  build(grids){
    const pts=[], adj=[], ext=[];
    for(const g of grids){
      const base=pts.length, isExt=!!CellKey.grid(g.spec);
      for(const p of g.points){ pts.push(p); adj.push([]); ext.push(isExt? base : -1); }
      for(const [a,b] of g.edges){
        const i=base+a, j=base+b;
        if(i===j || i>=pts.length || j>=pts.length) continue;
        if(!adj[i].includes(j)) adj[i].push(j);
        if(!adj[j].includes(i)) adj[j].push(i);
      }
    }
    if(!pts.length) return null;
    // Across borders: a bucket of the exterior points near each cell's edge.
    const near=p=>{ const fx=((p[0]%8192)+8192)%8192, fy=((p[1]%8192)+8192)%8192; return fx<this.JOIN || fx>8192-this.JOIN || fy<this.JOIN || fy>8192-this.JOIN; };
    const B=new Map(), cs=this.JOIN, bk=(x,y)=>Math.floor(x/cs)+','+Math.floor(y/cs);
    pts.forEach((p,i)=>{ if(ext[i]<0 || !near(p)) return; const k=bk(p[0],p[1]); if(!B.has(k)) B.set(k,[]); B.get(k).push(i); });
    for(const [k, list] of B){
      const [bx,by]=k.split(',').map(Number);
      for(const i of list) for(let dx=-1;dx<=1;dx++) for(let dy=-1;dy<=1;dy++){
        for(const j of B.get((bx+dx)+','+(by+dy))||[]){
          if(j<=i || ext[i]===ext[j]) continue;
          const a=pts[i], b=pts[j];
          if(Math.hypot(a[0]-b[0], a[1]-b[1])<=this.JOIN && Math.abs(a[2]-b[2])<256){ adj[i].push(j); adj[j].push(i); }
        }
      }
    }
    return {pts, adj};
  },
  /** The grid point nearest `p`, a height difference counting double (the floor above is
   *  not the one to walk from). -1 with no grid. */
  nearest(p){
    const G=this.grid; if(!G) return -1;
    let best=-1, bd=Infinity;
    for(let i=0;i<G.pts.length;i++){
      const q=G.pts[i], d=Math.hypot(q[0]-p[0], q[1]-p[1], 2*(q[2]-p[2]));
      if(d<bd){ bd=d; best=i; }
    }
    return best;
  },
  /** The points to walk from `from` to `to`: grid points on the way, then `to` itself. A
   *  straight line when there is no grid, the two are close, or the grid does not join them. */
  route(from, to){
    const G=this.grid;
    if(!G || !from || !to) return [to];
    if(Math.hypot(to[0]-from[0], to[1]-from[1])<256 && Math.abs(to[2]-from[2])<128) return [to];
    const a=this.nearest(from), b=this.nearest(to);
    if(a<0 || b<0 || a===b) return [to];
    const P=G.pts, h=i=>Math.hypot(P[i][0]-P[b][0], P[i][1]-P[b][1], P[i][2]-P[b][2]);
    // A* with a small binary heap.
    const g=new Map([[a,0]]), came=new Map(), open=[[h(a), a]], shut=new Set();
    const push=(f,i)=>{ open.push([f,i]); let k=open.length-1; while(k>0){ const u=(k-1)>>1; if(open[u][0]<=open[k][0]) break; [open[u],open[k]]=[open[k],open[u]]; k=u; } };
    const pop=()=>{ const top=open[0], last=open.pop(); if(open.length){ open[0]=last; let k=0; for(;;){ const l=2*k+1, r=l+1; let m=k; if(l<open.length && open[l][0]<open[m][0]) m=l; if(r<open.length && open[r][0]<open[m][0]) m=r; if(m===k) break; [open[m],open[k]]=[open[k],open[m]]; k=m; } } return top; };
    let found=false;
    while(open.length){
      const [, i]=pop();
      if(i===b){ found=true; break; }
      if(shut.has(i)) continue;
      shut.add(i);
      for(const j of G.adj[i]){
        if(shut.has(j)) continue;
        const c=g.get(i)+Math.hypot(P[i][0]-P[j][0], P[i][1]-P[j][1], P[i][2]-P[j][2]);
        if(c<(g.has(j)? g.get(j) : Infinity)){ g.set(j, c); came.set(j, i); push(c+h(j), j); }
      }
    }
    if(!found) return [to];
    const path=[];
    for(let i=b; i!==undefined; i=came.get(i)) path.unshift(P[i].slice());
    // The first point is skipped when the second is nearer from here (it is behind), and
    // when the actor already stands on it.
    if(path.length>1){ const d=(p,q)=>Math.hypot(p[0]-q[0], p[1]-q[1]); if(d(from, path[0])<48 || d(from, path[1])<d(path[0], path[1])) path.shift(); }
    path.push(to.slice());
    return path;
  },
  /** Where a wanderer goes next: a grid point within `range` of `home` (and near its
   *  height), as the game picks; with none, a spot in that circle. */
  wanderPoint(home, range){
    const G=this.grid;
    if(G){
      const c=[];
      for(const q of G.pts) if(Math.hypot(q[0]-home[0], q[1]-home[1])<=range && Math.abs(q[2]-home[2])<512) c.push(q);
      if(c.length) return c[Math.floor(Math.random()*c.length)].slice();
    }
    const r=Math.random()*range, th=Math.random()*Math.PI*2;
    return [home[0]+r*Math.cos(th), home[1]+r*Math.sin(th), home[2]];
  },
  /** The idle a wanderer plays while it waits, by the package's chances (Idle2 ... Idle9,
   *  each 0-100): one of them, weighted by its chance, when a roll of 100 falls inside their
   *  sum; else none and it just stands. */
  pickIdle(chances, roll){
    const c=(chances||[]).map(x=>Math.max(0, +x||0));
    const sum=c.reduce((a,b)=>a+b, 0);
    if(!sum) return null;
    const r=(roll==null? Math.random() : roll)*Math.max(100, sum);
    if(r>=sum) return null;
    let acc=0;
    for(let i=0;i<c.length;i++){ acc+=c[i]; if(r<acc) return 'Idle'+(i+2); }
    return null;
  },

  /* ---- the walk ------------------------------------------------------------------------- */

  /** Whether a script (not the AI) is playing a group on `key`. */
  scripted(key){
    const st=WgScripts.anims.get(key);
    return !!st && st.slots.some(p=>p && p.by!=='AI');
  },
  /** The AI's own play on `key`, if any. */
  own(key){
    const st=WgScripts.anims.get(key);
    return st? st.slots.find(p=>p && p.by==='AI') || null : null;
  },
  /** `group` looped on the actor by the AI (the walk, the run), or nothing when a script is
   *  playing something or its model lacks the group. */
  loop(key, group){
    const S=WgScripts;
    if(this.scripted(key)) return;
    const cur=this.own(key);
    if(cur && cur.group.toLowerCase()===group.toLowerCase()) return;
    const model=S.modelOfKey(key);
    if(this.lacks.has(model+'|'+group.toLowerCase())) return;
    if(cur) S.stopAnim(key, cur.group);
    S.playAnim(key, group, Infinity, 1, 'AI', {quiet:true, onLack:()=>this.lacks.add(model+'|'+group.toLowerCase())});
  },
  /** The AI's looping walk off the actor (back to its idle); an idle it chose is left to end. */
  still(key){
    const cur=this.own(key);
    if(cur && cur.loops===Infinity) WgScripts.stopAnim(key, cur.group);
  },
  /** An idle played once on the actor by the AI (a wanderer's wait). */
  idle(key, group){
    if(this.scripted(key) || !group) return;
    const model=WgScripts.modelOfKey(key);
    if(this.lacks.has(model+'|'+group.toLowerCase())) return;
    const cur=this.own(key); if(cur) WgScripts.stopAnim(key, cur.group);
    WgScripts.playAnim(key, group, 0, 1, 'AI', {quiet:true, onLack:()=>this.lacks.add(model+'|'+group.toLowerCase())});
  },
};
