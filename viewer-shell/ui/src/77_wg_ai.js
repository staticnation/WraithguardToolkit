/* =====================================================================================
   Wraithguard: the Preview's actors' AI, the routes and the motion (Static, 2026-11-18:
   "AI ... I feel the scripts will want that too").

   The packages themselves are the scripts' layer's (73_wg_scripts.js `ai`, `actorsFrame`):
   MWScript's AiWander, AiTravel, AiFollow, AiEscort and AiActivate, StartCombat, and OpenMW
   Lua's I.AI through 74_wg_lua.js. This is what carries them out the way the game does:

   - routes over OpenMW's navmesh when one has been chosen (the viewer's navmesh overlay,
     `wg.navDb`): A* over its polygons, through the middles of the edges they share - OpenMW's
     own actors walk it; else over the loaded cells' pathgrids (the `pathgrid` command, the
     points Morrowind's AI walks): A* from the grid point nearest the actor to the one nearest
     where it is going, the exterior cells' grids joined where their points meet across a
     border. With neither, a straight line;
   - the record's own pace: walk and run from its Speed and Athletics by the game's settings
     (fMinWalkSpeed ... fBaseRunMultiplier; a creature's fMinWalkSpeedCreature ...);
   - greetings: an actor with Hello stops and turns to the player within Hello x
     iGreetDistanceMultiplier units, as the game's do, once each time the player comes near;
   - fleeing: in a fight, one whose Flee and lost health come to 100 runs from its foe, and
     stops once it is clear;
   - room: a moving actor keeps out of another's;
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
  /** The navmesh as a graph, when one was chosen and read: {polys:[{v, c}], adj:[[[j, mid]...]]}. */
  nav:null, _navFor:'',
  /** The game's settings the walk and the greeting are worked out by (`game_settings`). */
  gs:{greetMult:6, move:{fMinWalkSpeed:100, fMaxWalkSpeed:200, fMinWalkSpeedCreature:5, fMaxWalkSpeedCreature:300, fBaseRunMultiplier:1.75, fAthleticsRunBonus:1}},
  _gsRead:false,
  /** How far apart two actors keep. */
  ROOM:56,
  /** How far a fleeing actor runs from its foe before it stops. */
  FLEE:2048,

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
      await this.loadNav(specs, key);
      if(!this._gsRead){
        this._gsRead=true;
        try{ const g=await Engine.call('game_settings'); if(g){ if(g.greetMult!=null) this.gs.greetMult=+g.greetMult; if(g.greetReset!=null) this.gs.greetReset=+g.greetReset; if(g.greetDuration!=null) this.gs.greetDuration=+g.greetDuration; Object.assign(this.gs.move, g.move||{}); } }catch(_){ }
      }
      this._for=key; this._busy=null;
      return this.grid;
    })();
    return this._busy;
  },
  /** OpenMW's navmesh for the loaded cells, when one has been chosen (the navmesh overlay's
   *  `wg.navDb`), as a graph of polygons for the agent nearest an actor's size. */
  async loadNav(specs, key){
    let db=''; try{ db=localStorage.getItem('wg.navDb')||''; }catch(_){ }
    if(!db || typeof WgTools!=='object' || !WgTools.readNav){ this.nav=null; return; }
    if(this._navFor===db+'|'+key) return;
    try{
      const d=WgTools.readNav(await Engine.bytes('navmesh',{cells:specs, db}));
      this.nav=this.buildNav(d.polys, this.agentOf(d.agents));
    }catch(_){ this.nav=null; }
    this._navFor=db+'|'+key;
  },
  /** The agent an actor walks as: the one nearest OpenMW's default half extents. */
  agentOf(agents){
    let best=0, bd=Infinity;
    (agents||[]).forEach((a, i)=>{ const h=a.h||[0,0,0], d=Math.abs(h[0]-29.27)+Math.abs(h[1]-28.48)+Math.abs(h[2]-66.5); if(d<bd){ bd=d; best=i; } });
    return best;
  },
  /** The agent's polygons, each with its middle, joined where two share an edge (the edge's
   *  middle the point to walk through). Null with none. */
  buildNav(polys, agent){
    const P=[], adj=[], edges=new Map();
    const vk=(v, i)=>Math.round(v[i*3])+','+Math.round(v[i*3+1])+','+Math.round(v[i*3+2]);
    for(const p of polys||[]){
      if(p.a!==agent) continue;
      const v=p.v, n=v.length/3|0; if(n<3) continue;
      const c=[0,0,0]; for(let i=0;i<n;i++){ c[0]+=v[i*3]; c[1]+=v[i*3+1]; c[2]+=v[i*3+2]; }
      c[0]/=n; c[1]/=n; c[2]/=n;
      const id=P.length; P.push({v, c}); adj.push([]);
      for(let i=0;i<n;i++){
        const j=(i+1)%n, a=vk(v, i), b=vk(v, j), k=a<b? a+'|'+b : b+'|'+a;
        const e0=[v[i*3], v[i*3+1], v[i*3+2]], e1=[v[j*3], v[j*3+1], v[j*3+2]];
        const mid=[(e0[0]+e1[0])/2, (e0[1]+e1[1])/2, (e0[2]+e1[2])/2];
        const was=edges.get(k);
        if(was==null) edges.set(k, {id, mid});
        else if(was.id!==id){ adj[id].push([was.id, mid, e0, e1]); adj[was.id].push([id, mid, e0, e1]); }
      }
    }
    return P.length? {polys:P, adj} : null;
  },
  /** The navmesh polygon a point stands on (the nearest one below or above it among those
   *  whose outline holds it), else the one whose middle is nearest. */
  navAt(p){
    const N=this.nav; if(!N) return -1;
    let best=-1, bz=Infinity;
    N.polys.forEach((q, i)=>{
      const v=q.v, n=v.length/3|0;
      let inside=false;
      for(let a=0, b=n-1; a<n; b=a++){
        const xa=v[a*3], ya=v[a*3+1], xb=v[b*3], yb=v[b*3+1];
        if(((ya>p[1])!==(yb>p[1])) && (p[0]<(xb-xa)*(p[1]-ya)/(yb-ya)+xa)) inside=!inside;
      }
      if(inside){ const dz=Math.abs(q.c[2]-p[2]); if(dz<bz && dz<256){ bz=dz; best=i; } }
    });
    if(best>=0) return best;
    let bd=Infinity;
    N.polys.forEach((q, i)=>{ const d=Math.hypot(q.c[0]-p[0], q.c[1]-p[1], 2*(q.c[2]-p[2])); if(d<bd){ bd=d; best=i; } });
    return best;
  },
  /** A route over the navmesh: the middles of the edges crossed, then `to`. Null when the
   *  navmesh does not join the two (the pathgrid is tried then). */
  routeNav(from, to){
    const N=this.nav; if(!N) return null;
    const a=this.navAt(from), b=this.navAt(to);
    if(a<0 || b<0) return null;
    if(a===b) return [to.slice()];
    const C=i=>N.polys[i].c, h=i=>Math.hypot(C(i)[0]-C(b)[0], C(i)[1]-C(b)[1], C(i)[2]-C(b)[2]);
    const g=new Map([[a,0]]), came=new Map(), shut=new Set(), open=[[h(a), a]];
    while(open.length){
      let m=0; for(let i=1;i<open.length;i++) if(open[i][0]<open[m][0]) m=i;
      const [, i]=open.splice(m, 1)[0];
      if(i===b) break;
      if(shut.has(i)) continue;
      shut.add(i);
      for(const [j, mid, e0, e1] of N.adj[i]){
        if(shut.has(j)) continue;
        const c=g.get(i)+Math.hypot(C(i)[0]-mid[0], C(i)[1]-mid[1])+Math.hypot(mid[0]-C(j)[0], mid[1]-C(j)[1]);
        if(c<(g.has(j)? g.get(j) : Infinity)){ g.set(j, c); came.set(j, [i, mid, e0, e1]); open.push([c+h(j), j]); }
      }
    }
    if(!came.has(b)) return null;
    // The edges crossed, each as [left, right] seen walking from the one polygon into the next.
    const portals=[];
    for(let i=b; came.has(i); i=came.get(i)[0]){
      const [from_, , e0, e1]=came.get(i), dir=[C(i)[0]-C(from_)[0], C(i)[1]-C(from_)[1]];
      const side=dir[0]*(e0[1]-C(from_)[1])-dir[1]*(e0[0]-C(from_)[0]);
      portals.unshift(side>0? [e0, e1] : [e1, e0]);
    }
    return this.funnel(from, portals, to);
  },
  /** The shortest way through a row of portals ([left, right] each): the simple stupid
   *  funnel - the route turns only at a portal's corner, so it hugs the inside of a bend
   *  rather than zig-zagging through the edges' middles. Ends with `to`. */
  funnel(from, portals, to){
    const P=[[from, from], ...portals, [to, to]];
    const cross=(a, b, c)=>(b[0]-a[0])*(c[1]-a[1])-(b[1]-a[1])*(c[0]-a[0]);   // > 0: c left of a->b
    const same=(a, b)=>Math.abs(a[0]-b[0])<1e-6 && Math.abs(a[1]-b[1])<1e-6;
    const out=[];
    let apex=from, left=P[0][0], right=P[0][1], ai=0, li=0, ri=0;
    for(let i=1;i<P.length;i++){
      const [l, r]=P[i];
      // The right side: narrower, or past the left (a turn at the left corner).
      if(cross(apex, right, r)>=0){
        if(same(apex, right) || cross(apex, left, r)<0){ right=r; ri=i; }
        else{ out.push(left.slice()); apex=left; ai=li; left=right=apex; li=ri=ai; i=ai; continue; }
      }
      // The left side, the same the other way.
      if(cross(apex, left, l)<=0){
        if(same(apex, left) || cross(apex, right, l)>0){ left=l; li=i; }
        else{ out.push(right.slice()); apex=right; ai=ri; left=right=apex; li=ri=ai; i=ai; continue; }
      }
    }
    out.push(to.slice());
    // A corner reached twice (the end portal is a point) is one corner.
    return out.filter((p, i)=>i===0 || !same(p, out[i-1]));
  },

  /* ---- pace, greetings, fleeing, room --------------------------------------------------- */

  /** An actor's walk and run in units a second, by OpenMW's formulas over the game's settings:
   *  walk = min + Speed% of (max - min) (a creature's own min and max), run = walk x (Athletics%
   *  x fAthleticsRunBonus + fBaseRunMultiplier) (a creature: x fBaseRunMultiplier). */
  speeds(A){
    const g=this.gs.move, m=(A && A.move)||{speed:50, athletics:30, creature:false};
    const sp=Math.max(0, +m.speed||0);
    const walk=m.creature? g.fMinWalkSpeedCreature+0.01*sp*(g.fMaxWalkSpeedCreature-g.fMinWalkSpeedCreature)
                         : g.fMinWalkSpeed+0.01*sp*(g.fMaxWalkSpeed-g.fMinWalkSpeed);
    const run=m.creature? walk*g.fBaseRunMultiplier : walk*(0.01*Math.max(0, +m.athletics||0)*g.fAthleticsRunBonus+g.fBaseRunMultiplier);
    return {walk:Math.max(1, walk), run:Math.max(1, run)};
  },
  /** Whether a fighter runs: its Flee and the share of its health lost come to 100. */
  flees(A){
    const flee=(A.rates && A.rates[2])||0, max=(A.max && A.max[0])||100;
    if(!flee || !(max>0)) return false;
    return flee+(1-Math.max(0, A.hp)/max)*100>=100;
  },
  /** Where a fleeing actor runs to: straight away from its foe, the flee distance off. */
  fleePoint(p, foe){
    let dx=p[0]-foe[0], dy=p[1]-foe[1], d=Math.hypot(dx, dy);
    if(d<1e-3){ dx=1; dy=0; d=1; }
    return [p[0]+dx/d*this.FLEE, p[1]+dy/d*this.FLEE, p[2]];
  },
  /** The greeting, as OpenMW's `Actors::updateGreetingState` has it: an actor with Hello and
   *  no fight on, the player within Hello x iGreetDistanceMultiplier for two of its quarter-second
   *  updates, greets - turns to the player and plays Idle2 for iGreetDuration seconds - and is
   *  done: it goes back to what it was doing though the player is still there, and greets again
   *  only once the player has been past fGreetDistanceReset. True while it greets (a wanderer
   *  stands for it). */
  greet(A, dt){
    const hello=(A.rates && A.rates[0])||0, S=WgScripts;
    if(!hello || A.key==='player') return false;
    const g=A._greet||(A._greet={state:'none', timer:0, t:0});
    const p=S.posOf(A.t), pl=S.posOf({player:true}), d=Math.hypot(pl[0]-p[0], pl[1]-p[1], pl[2]-p[2]);
    const reset=+(this.gs.greetReset||512), duration=+(this.gs.greetDuration||4);
    if(g.state!=='none' && d>=reset){ g.state='none'; g.timer=0; }
    if(g.state==='none'){
      g.t+=dt||0;
      if(g.t>=0.25){ g.t=0; if(d<=hello*this.gs.greetMult) g.timer++; }
      if(g.timer>=2){ g.state='in'; g.timer=0; g.left=duration; S.say(A.id, 'do', A.id+' greets the player'); this.idle(A.key, 'Idle2'); }
    }
    if(g.state==='in'){
      S.face(A.t, pl);
      g.left-=dt||0;
      if(g.left<=0) g.state='done';
      return g.state==='in';
    }
    return false;
  },
  /** Room: `p`, where a moving actor is about to stand, pushed out of every other actor's. */
  room(key, p){
    const S=WgScripts;
    // The player too, while the camera walks as the player's body (its feet under the eye).
    const R=App.R;
    if(R && R.walk){
      const q=S.posOf({player:true}), dx=p[0]-q[0], dy=p[1]-q[1], d=Math.hypot(dx, dy), need=this.ROOM;
      if(d<need && Math.abs(p[2]-q[2])<128){ if(d<1e-3) p[0]+=need; else { p[0]+=dx/d*(need-d); p[1]+=dy/d*(need-d); } }
    }
    for(const B of S.actors.values()){
      if(B.key===key || B.key==='player' || B.dead || S.disabled.has(B.key)) continue;
      const q=S.posOf(B.t), dx=p[0]-q[0], dy=p[1]-q[1], d=Math.hypot(dx, dy);
      if(d<this.ROOM && Math.abs(p[2]-q[2])<128){
        if(d<1e-3){ p[0]+=this.ROOM; continue; }
        p[0]+=dx/d*(this.ROOM-d); p[1]+=dy/d*(this.ROOM-d);
      }
    }
    return p;
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
    if(!from || !to) return [to];
    // OpenMW's navmesh first, when one was chosen and it joins the two.
    const nv=this.nav && this.routeNav(from, to);
    if(nv) return nv;
    const G=this.grid;
    if(!G) return [to];
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

  /** How near the camera an actor's walk and idles are played. */
  NEAR:4096,
  near(A){
    const S=WgScripts, p=S.posOf(A.t), c=S.posOf({player:true});
    return !(App.R && App.R.cam) || Math.hypot(p[0]-c[0], p[1]-c[1])<=this.NEAR;
  },
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
  loop(key, group, pace){
    const S=WgScripts;
    if(this.scripted(key)) return;
    const cur=this.own(key);
    if(cur && cur.group.toLowerCase()===group.toLowerCase()) return;
    const model=S.modelOfKey(key);
    if(this.lacks.has(model+'|'+group.toLowerCase())) return;
    if(cur) S.stopAnim(key, cur.group);
    S.playAnim(key, group, Infinity, this.playRate(model, group, pace), 'AI', {quiet:true, at:0, onLack:()=>this.lacks.add(model+'|'+group.toLowerCase())});
  },
  /** How fast a walk or run is played for an actor moving at `pace` (units a second): its
   *  pace over how fast the group carries the body across the ground (`anim_groups`' fourth
   *  number), as OpenMW's character controller sets it - the feet keep the ground. 1 until
   *  the groups are known, or for a group that does not travel. Two decimals, so an actor's
   *  walk is one mesh whenever it starts. */
  playRate(model, group, pace){
    const list=WgScripts.groupsKnown && WgScripts.groupsKnown(model);
    const g=list && list.find(x=>String(x[0]).toLowerCase()===String(group).toLowerCase());
    const vel=g && +g[3];
    if(!(vel>1) || !(pace>0)) return 1;
    return Math.round(Math.min(4, Math.max(0.25, pace/vel))*100)/100;
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
