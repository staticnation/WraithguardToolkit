/* =====================================================================================
   Wraithguard: the Editor's Physics. Four ways to set things moving, each played in the
   render window as gold ghosts, then kept (Keep: one step, one Ctrl+Z) or thrown away (Drop):

   - Drop: the selected objects fall, tumble and come to rest on what is around them.
   - Blast: click a point; what is within its reach is thrown outward, the push falling off
     to nothing at the edge, lighter things further.
   - Throw: the selected objects fly the way the camera faces, at a speed.
   - Scatter: click a point; copies of the selected objects (one picked at random each) fall
     from above it, spread over a circle and turned at random, and settle as things dropped
     do - a scatter brush for clutter, rocks, debris. Keep places them (one step); Drop
     places nothing.

   "Struck objects move too" (off by default: only the selected objects, or the copies,
   move): the objects around join in, so what is hit scatters. The blacklist says which never
   move but still collide: mesh path patterns (`x/ex_*`), record types (`DOOR`) and a size.

   OpenMW does not simulate loose objects (they stay where the plugin puts them), so this
   is a placing tool, not the game: the surfaces are the game's collision, the motion is
   Rapier's (viewer-shell/physcore, the engine's `phys_drop`). Copyright (c) 2026
   StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
const WgPhysics={
  /** The request (`request`) as `phys_drop` reads it (`PhysRequest`, commands.rs): WGTB
   *  tables - `settings` [gravity, friction, bounce, seconds, drop only, hollow, exterior,
   *  origin x, origin y], `bodies` [key, model, matrix, drawn triangles], `statics` [model,
   *  matrix], `land` [gx, gy, has ground, 65 x 65 heights], `kinetic` [blast 0/1, its point x,
   *  y, z (world), radius, strength, the bodies' starting velocity x, y, z], `near` [key, id,
   *  model, matrix] (the objects around that move when struck) and `blacklist` [kind, value]
   *  (`pattern`, `tag` or `size`). */
  pack(q){
    const st=q.settings||{}, o=q.origin||[0,0], k=q.kinetic||{}, at=k.at||[0,0,0], v=k.throw||[0,0,0];
    const bl=q.blacklist||{};
    return [
      ['settings','dddduuudd',[[+st.gravity||0, +st.friction||0, +st.bounce||0, +st.seconds||0, st.dropOnly? 1 : 0, st.hollow===false? 0 : 1, q.exterior? 1 : 0, +o[0]||0, +o[1]||0]]],
      ['bodies','ssFF',(q.bodies||[]).map(b=>[b.key, b.model, b.m, b.tris||[]])],
      ['statics','sF',(q.statics||[]).map(s=>[s.model, s.m])],
      ['land','iiuF',(q.land||[]).map(l=>[l.gx, l.gy, l.h? 1 : 0, l.h||[]])],
      ['kinetic','udddddddd',[[k.blast? 1 : 0, +at[0]||0, +at[1]||0, +at[2]||0, +k.radius||0, +k.strength||0, +v[0]||0, +v[1]||0, +v[2]||0]]],
      ['near','sssF',(q.near||[]).map(n=>[n.key, n.id, n.model, n.m])],
      ['blacklist','ss',[...(bl.patterns||[]).map(p=>['pattern', p]), ...(bl.tags||[]).map(t=>['tag', t]), ['size', String(+bl.size||0)]]],
    ];
  },
  /** `phys_drop`'s tables (WGTB, commands.rs): {frames:[[[x, y, z, rx, ry, rz] per body] per
   *  frame], seconds, settled, skipped, keys (the bodies in the poses' order), fixed
   *  [{key, why}] (the objects around the blacklist held)}. */
  readDrop(tb){
    const [seconds,settled,count]=(tb.head||[])[0]||[0,0,0];
    const frames=Array.from({length:count}, ()=>[]);
    for(const [f,...pose] of tb.poses||[]) frames[f].push(pose);
    return {frames, seconds, settled:!!settled, skipped:(tb.skipped||[]).map(r=>r[0]),
      keys:(tb.keys||[]).map(r=>r[0]), fixed:(tb.fixed||[]).map(([key, why])=>({key, why}))};
  },
  MODES:[['drop','Drop','The selected objects fall and come to rest'],
    ['blast','Blast','Click a point: what is within reach is thrown outward'],
    ['throw','Throw','The selected objects fly the way the camera faces'],
    ['scatter','Scatter','Click a point: copies of the selected objects fall around it and settle']],
  /** The blacklist as it ships: physcore's `Blacklist::morrowind`. */
  BLACKLIST:{patterns:['x/ex_*','i/in_*','*_wall*','*_bridge*','*_dock*','*terrain_*','*_rock_*','f/flora_tree*','*_tree_*'], tags:['DOOR','NPC_','CREA','LEVC'], size:512},
  st:{gravity:1, friction:0.7, bounce:0.05, seconds:6, dropOnly:false, hollow:true,
      mode:'drop', all:false, radius:300, strength:900, speed:700, count:8, spread:200, height:150, blacklist:null},
  run:null,               // {bodies:[{hit, ref, off, scale, copy}], frames, raf}
  REACH:1024,             // how far around the bodies other objects count as ground (units)
  MOVERS:300,             // the most objects around that join in (the nearest)

  load(){
    try{ const s=JSON.parse(localStorage.getItem('wgPhysics')||'null'); if(s) Object.assign(this.st, s); }catch(_){ }
    if(!this.st.blacklist) this.st.blacklist=JSON.parse(JSON.stringify(this.BLACKLIST));
  },
  keep(){ try{ localStorage.setItem('wgPhysics', JSON.stringify(this.st)); }catch(_){ } },

  toggle(){
    if(this._el && !this._el.hidden){ this.close(); return; }
    this.panel(); this._el.hidden=false;
  },
  close(){ this.disarm(); this.discard(true); if(this._el) this._el.hidden=true; },

  panel(){
    if(this._el) return this.draw();
    this.load();
    const d=document.createElement('div');
    d.id='wgPhysPanel'; d.className='ori'; d.hidden=true;
    d.innerHTML='<div class="orihead"><b>Physics</b><span style="flex:1"></span>'+
      '<button class="btn dim ic" id="wgPhysX" title="Close">&#x2715;</button></div><div class="oribody" id="wgPhysBody"></div>';
    if(typeof WgUI==='object' && WgUI.mount) WgUI.mount(d); else ($('#vpwrap')||document.body).appendChild(d);
    d.querySelector('#wgPhysX').onclick=()=>this.close();
    this._el=d;
    this.draw();
  },

  /** The modes, the sliders, the buttons, the blacklist and what the last run did. */
  draw(){
    const box=this._el && this._el.querySelector('#wgPhysBody'); if(!box) return;
    const s=this.st, r=this.run, mode=s.mode;
    const rng=(k, label, min, max, step, tip)=>'<div class="row"><label title="'+escHtml(tip)+'">'+label+'</label><div class="ctl"><div class="rng">'+
      '<input type="range" min="'+min+'" max="'+max+'" step="'+step+'" value="'+s[k]+'" data-k="'+k+'"><span class="val" data-v="'+k+'">'+this.shown(k)+'</span></div></div></div>';
    let h='<div class="lrow" style="display:flex;gap:4px;flex-wrap:wrap;margin:0 0 6px">'+
      this.MODES.map(([k, l, tip])=>'<button class="btn sm'+(mode===k? ' pri' : '')+'" data-mode="'+k+'" title="'+escHtml(tip)+'">'+l+'</button>').join('')+'</div>';
    if(mode==='blast') h+=rng('radius', 'Reach', 50, 2000, 10, 'How far the blast reaches, in units (about 70 to the meter)')+
      rng('strength', 'Strength', 100, 5000, 50, 'How fast it throws a body at its middle, units a second; it falls off to nothing at the edge, and lighter things go further');
    if(mode==='throw') h+=rng('speed', 'Speed', 100, 3000, 50, 'How fast the selected objects leave, units a second, the way the camera faces (a little upward)');
    if(mode==='scatter') h+=rng('count', 'Copies', 1, 60, 1, 'How many copies fall')+
      rng('spread', 'Spread', 16, 2000, 8, 'The circle they fall over, its radius in units')+
      rng('height', 'Height', 0, 1000, 10, 'How high above the point they start');
    h+=rng('gravity', 'Gravity', 0.1, 3, 0.05, 'How hard they fall, as a part of the world\'s (1 is 9.81 m/s², about 690 units a second each second)')+
      rng('friction', 'Friction', 0, 1.5, 0.05, 'How much they grip what they land on: low slides, high sticks')+
      rng('bounce', 'Bounce', 0, 0.9, 0.05, 'How much of a fall comes back as a bounce')+
      rng('seconds', 'Time limit', 1, 20, 1, 'The most seconds to let them move; they stop sooner when they come to rest');
    h+='<div class="lrow" style="display:flex;gap:6px;flex-wrap:wrap;margin:6px 0">'+
      '<button class="btn sm'+(s.all? ' pri' : '')+'" id="wgPhysAll" title="Off: only the selected objects (or the copies) move. On: the objects around move too when struck, unless the blacklist holds them">Struck objects move too</button>'+
      '<button class="btn sm'+(s.dropOnly? ' pri' : '')+'" id="wgPhysOnly" title="Only fall: they keep their turn and do not tip over">Drop only</button>'+
      '<button class="btn sm'+(s.hollow? ' pri' : '')+'" id="wgPhysHollow" title="Keep their hollows: a bowl or a cart is split into convex parts, so things can land inside it. Off, each is one hull round all of it (faster)">Hollow shapes</button></div>';
    const bl=s.blacklist||this.BLACKLIST;
    h+='<details id="wgPhysBl"'+(this._blOpen? ' open' : '')+'><summary title="What never moves, though it still collides">Never moves</summary>'+
      '<div class="row"><label title="Mesh paths from meshes/ on, one a line: * any run, ? one character, case and slash direction ignored">Meshes</label><div class="ctl"><textarea class="fld" id="wgPhysBlPat" rows="4" style="width:100%">'+escHtml((bl.patterns||[]).join('\n'))+'</textarea></div></div>'+
      '<div class="row"><label title="Record types, comma separated: STAT, DOOR, NPC_, CREA, CONT, MISC...">Record types</label><div class="ctl"><input class="fld" id="wgPhysBlTag" value="'+escHtml((bl.tags||[]).join(', '))+'"></div></div>'+
      '<div class="row"><label title="Anything wider than this many units never moves (0: no limit). 512 is about 7 m: a house, a big rock">Wider than</label><div class="ctl"><input class="fld" type="number" min="0" step="16" id="wgPhysBlSize" value="'+(+bl.size||0)+'" style="width:90px"></div></div>'+
      '<button class="btn sm" id="wgPhysBlReset" title="The blacklist as it ships: Morrowind\'s buildings, walls, docks, bridges, rocks and trees; doors and actors; wider than 512">Defaults</button></details>';
    const verb=mode==='blast' || mode==='scatter'? 'Run, then click' : 'Run';
    h+='<div class="lrow" style="display:flex;gap:6px;flex-wrap:wrap;margin:6px 0">'+
      '<button class="btn sm pri" id="wgPhysRun" title="'+escHtml(this.MODES.find(m=>m[0]===mode)[2])+'"'+(r && r.busy? ' disabled' : '')+'>'+verb+'</button>'+
      '<button class="btn sm" id="wgPhysKeep" title="Put them where they came to rest: one step, one Ctrl+Z"'+(r && r.frames? '' : ' disabled')+'>Keep</button>'+
      '<button class="btn sm" id="wgPhysDrop" title="Throw the run away; everything stays as it was"'+(r? '' : ' disabled')+'>Drop</button></div>';
    h+='<div class="hint" id="wgPhysSay">'+escHtml(this.say||this.hintFor(mode))+'</div>';
    box.innerHTML=h;
    box.querySelectorAll('input[data-k]').forEach(el=>el.oninput=()=>{
      s[el.dataset.k]=+el.value; this.keep();
      const v=box.querySelector('[data-v="'+el.dataset.k+'"]'); if(v) v.textContent=this.shown(el.dataset.k);
    });
    box.querySelectorAll('button[data-mode]').forEach(b=>b.onclick=()=>{ s.mode=b.dataset.mode; this.say=''; this.disarm(); this.keep(); this.draw(); });
    box.querySelector('#wgPhysAll').onclick=()=>{ s.all=!s.all; this.keep(); this.draw(); };
    box.querySelector('#wgPhysOnly').onclick=()=>{ s.dropOnly=!s.dropOnly; this.keep(); this.draw(); };
    box.querySelector('#wgPhysHollow').onclick=()=>{ s.hollow=!s.hollow; this.keep(); this.draw(); };
    const det=box.querySelector('#wgPhysBl'); det.ontoggle=()=>{ this._blOpen=det.open; };
    const blSave=()=>{
      s.blacklist={patterns:box.querySelector('#wgPhysBlPat').value.split(/\r?\n/).map(x=>x.trim()).filter(Boolean),
        tags:box.querySelector('#wgPhysBlTag').value.split(/[,\s]+/).map(x=>x.trim().toUpperCase()).filter(Boolean),
        size:Math.max(0, +box.querySelector('#wgPhysBlSize').value||0)};
      this.keep();
    };
    for(const id of ['#wgPhysBlPat','#wgPhysBlTag','#wgPhysBlSize']) box.querySelector(id).onchange=blSave;
    box.querySelector('#wgPhysBlReset').onclick=()=>{ s.blacklist=JSON.parse(JSON.stringify(this.BLACKLIST)); this.keep(); this.draw(); };
    box.querySelector('#wgPhysRun').onclick=()=>this.start();
    box.querySelector('#wgPhysKeep').onclick=()=>this.commit();
    box.querySelector('#wgPhysDrop').onclick=()=>this.discard();
  },
  hintFor(mode){
    return mode==='blast'? 'Run, then click where the blast goes. With "Struck objects move too" off, only the selected objects are thrown.'
      : mode==='throw'? 'Select objects (Ctrl+click for more), face where they should go, then Run.'
      : mode==='scatter'? 'Select the objects to copy (Ctrl+click for more), Run, then click where they fall. Keep places the copies.'
      : 'Select objects in the render window (Ctrl+click for more than one), then Run. They fall onto the other objects\' collision and, outside, the land.';
  },
  shown(k){
    const v=this.st[k];
    return k==='gravity'? '×'+(+v).toFixed(2) : k==='seconds'? v+' s' : k==='count'? String(v)
      : k==='radius' || k==='spread' || k==='height'? v+' u' : k==='strength' || k==='speed'? v+' u/s' : (+v).toFixed(2);
  },
  tell(t){ this.say=t; const el=this._el && this._el.querySelector('#wgPhysSay'); if(el) el.textContent=t; },

  /** The selected objects: the Ctrl+click group, or the one selected object. */
  bodies(){
    const E=WgEditor, g=E.group();
    if(g.length>1) return g;
    const sel=E.selected();
    return sel && sel.ref && sel.hit && sel.hit.m? [{hit:sel.hit, ref:sel.ref}] : [];
  },

  /** The next click in the render window, as a point (`WgEditor.placePoint`), for a blast or a
   *  scatter; Escape gives up. */
  arm(then){
    const cv=App.R && App.R.cv; if(!cv) return this.tell('The render window is not ready: open a cell first.');
    this.disarm();
    const down=e=>{
      if(e.button!==0) return;
      e.preventDefault(); e.stopPropagation();
      const p=WgEditor.placePoint(e.clientX, e.clientY);
      this.disarm();
      if(p) then(p);
    };
    const key=e=>{ if(e.key==='Escape'){ this.disarm(); this.tell('Given up.'); } };
    cv.addEventListener('pointerdown', down, true);
    document.addEventListener('keydown', key, true);
    cv.style.cursor='crosshair';
    this._armed=()=>{ cv.removeEventListener('pointerdown', down, true); document.removeEventListener('keydown', key, true); cv.style.cursor=''; };
  },
  disarm(){ if(this._armed){ this._armed(); this._armed=null; } },

  /** The objects around a region (`lo`, `hi`, the scene's frame) as ground, and in `all` mode
   *  those `moves(o)` takes as the ones that join in: `{statics, near}`. */
  around(skip, lo, hi, moves){
    const R=App.R, r=this.REACH, statics=[], near=[];
    const inBox=o=>{
      const a=o.aabb, x=o.m[3], y=o.m[7];
      if(a) return a.x1>=lo[0]-r && a.x0<=hi[0]+r && a.y1>=lo[1]-r && a.y0<=hi[1]+r;
      return x>=lo[0]-r && x<=hi[0]+r && y>=lo[1]-r && y<=hi[1]+r;
    };
    const cands=[];
    for(const o of (R && R.pickables)||[]){
      if(!o || !o.model || !o.m || o.hidden || skip.has(String(o.refKey||'').toLowerCase())) continue;
      if(!inBox(o)) continue;
      const d=this.st.all && o.refKey && moves? moves(o) : Infinity;
      if(d<Infinity) cands.push([d, o]); else statics.push({model:o.model, m:Array.from(o.m).slice(0, 12)});
    }
    // The nearest join in; past the most, the rest are ground.
    cands.sort((a, b)=>a[0]-b[0]);
    cands.forEach(([, o], i)=>{
      if(i<this.MOVERS) near.push({key:String(o.refKey), id:String(o.id||''), model:o.model, m:Array.from(o.m).slice(0, 12), hit:o});
      else statics.push({model:o.model, m:Array.from(o.m).slice(0, 12)});
    });
    return {statics, near};
  },

  /** The request for the engine: the bodies (`{key, model, m, tris}`), what is around them,
   *  the land, the kinetic event (`kin`: {blast, at, radius, strength, throw}) and, in `all`
   *  mode, the objects around that `moves(o)` (a distance, or Infinity) takes. */
  request(bodies, kin, region, moves){
    const sc=App._scene||{};
    const skip=new Set(bodies.map(b=>String(b.key||'').toLowerCase()));
    const {statics, near}=this.around(skip, region.lo, region.hi, moves);
    const exterior=!(sc.centre && sc.centre.kind==='int');
    return {
      bodies:bodies.map(b=>({key:b.key, model:b.model, m:b.m, tris:b.tris})),
      statics, near, origin:sc.origin||[0,0], exterior, land:exterior? this.land(region.lo, region.hi) : [],
      settings:{gravity:9.81*70.3*this.st.gravity, friction:this.st.friction, bounce:this.st.bounce, seconds:this.st.seconds, dropOnly:this.st.dropOnly, hollow:this.st.hollow},
      kinetic:kin||{}, blacklist:this.st.blacklist||this.BLACKLIST,
    };
  },
  /** An object's drawn triangles in its own units (nine numbers each), for a mesh with no
   *  collision; at most 6000 of them. */
  drawn(o){
    const out=[], MAX=6000*9;
    for(const part of o.parts||[]){
      const V=part.pos, I=part.idx; if(!V || !I) continue;
      for(let t=0; t+2<I.length && out.length<MAX; t+=3)
        for(let k=0;k<3;k++){ const q=I[t+k]*3; out.push(Math.round(V[q]*10)/10, Math.round(V[q+1]*10)/10, Math.round(V[q+2]*10)/10); }
    }
    return out;
  },
  /** The ground as the render window has it, squares near the region (the Land view's
   *  strokes not yet kept are in it): 65 x 65 heights, null for a square with no land. */
  land(lo, hi){
    const sc=App._scene||{}, o=sc.origin||[0,0], out=[], C=8192;
    for(const c of sc.cells||[]){
      if(c.kind==='int' || c.gx==null) continue;
      const x0=c.gx*C-o[0], y0=c.gy*C-o[1];
      if(x0>hi[0]+C || x0+C<lo[0]-C || y0>hi[1]+C || y0+C<lo[1]-C) continue;
      out.push({gx:c.gx, gy:c.gy, h:c.heights? Array.from(c.heights, v=>Math.round(v)) : null});
    }
    return out;
  },
  /** A box round points in the scene's frame, `pad` wider each way. */
  box(points, pad){
    const lo=[Infinity, Infinity], hi=[-Infinity, -Infinity];
    for(const p of points) for(let k=0;k<2;k++){ lo[k]=Math.min(lo[k], p[k]-pad); hi[k]=Math.max(hi[k], p[k]+pad); }
    return {lo, hi};
  },
  /** One body of the selection: its pickable and reference, as the request and the ghosts
   *  want it. */
  bodyOf(g){
    const m=g.hit.m;
    return {key:String(g.hit.refKey||''), model:g.hit.model, m:Array.from(m).slice(0, 12), tris:this.drawn(g.hit),
      hit:g.hit, ref:g.ref, scale:g.hit.scale||1, off:[g.hit.wpos[0]-m[3], g.hit.wpos[1]-m[7], g.hit.wpos[2]-m[11]]};
  },

  async start(){
    if(!(typeof WgEditor==='object' && WgEditor.canEdit && WgEditor.canEdit())) return this.tell('Open the Editor on a plugin to move objects.');
    const s=this.st, mode=s.mode;
    const sel=this.bodies().filter(g=>g.hit.model);
    this.discard(true);
    if(mode==='drop' || mode==='throw'){
      if(!sel.length) return this.tell('Select an object in the render window first (Ctrl+click for more).');
      const bodies=sel.map(g=>this.bodyOf(g));
      const pos=bodies.map(b=>[b.m[3], b.m[7]]);
      if(mode==='drop') return this.go(bodies, {}, this.box(pos, 0), o=>{
        const d=Math.min(...pos.map(p=>Math.hypot(o.m[3]-p[0], o.m[7]-p[1]))); return d<=this.REACH? d : Infinity;
      });
      // A throw: the way the camera faces, a little upward, at the speed.
      const L=App.R.cameraLook? App.R.cameraLook() : [1,0,0];
      const n=Math.hypot(L[0], L[1], L[2]+0.25)||1, dir=[L[0]/n, L[1]/n, (L[2]+0.25)/n];
      const v=dir.map(x=>x*s.speed), far=s.speed*1.5;
      const ends=pos.map(p=>[p[0]+dir[0]*far, p[1]+dir[1]*far]);
      // Along the flight: the objects within half the reach of the path, nearest the start first.
      const onPath=o=>{
        let best=Infinity;
        pos.forEach((p, i)=>{
          const e=ends[i], ex=e[0]-p[0], ey=e[1]-p[1], L2=ex*ex+ey*ey||1;
          const t=Math.max(0, Math.min(1, ((o.m[3]-p[0])*ex+(o.m[7]-p[1])*ey)/L2));
          const dx=o.m[3]-(p[0]+ex*t), dy=o.m[7]-(p[1]+ey*t);
          if(Math.hypot(dx, dy)<=this.REACH/2) best=Math.min(best, t*far);
        });
        return best;
      };
      return this.go(bodies, {throw:v}, this.box(pos.concat(ends), 0), onPath);
    }
    if(mode==='scatter' && !sel.length) return this.tell('Select the objects to copy first (Ctrl+click for more).');
    if(mode==='blast' && !sel.length && !s.all) return this.tell('Select the objects the blast throws, or turn on "Struck objects move too".');
    this.tell(mode==='blast'? 'Click where the blast goes (Escape gives up).' : 'Click where they fall (Escape gives up).');
    this.arm(p=>mode==='blast'? this.blast(sel, p) : this.scatter(sel, p));
  },

  blast(sel, p){
    const s=this.st, at=p.local;
    const bodies=sel.map(g=>this.bodyOf(g));
    const reach=s.radius*1.5;
    return this.go(bodies, {blast:true, at:p.world, radius:s.radius, strength:s.strength}, this.box([at], reach), o=>{
      const d=Math.hypot(o.m[3]-at[0], o.m[7]-at[1], o.m[11]-at[2]); return d<=reach? d : Infinity;
    });
  },

  /** Copies of the selected objects over a circle at `p`, above it, turned at random. */
  async scatter(sel, p){
    const s=this.st, at=p.local, copies=[];
    // The records they are copies of (a placed copy needs its type).
    const srcs=[];
    for(const g of sel){
      let rec=WgEditor.selectedRecord({hit:g.hit, ref:g.ref});
      if(!rec.tag){
        try{ const t=await Engine.call('editor_record_tag', {id:g.hit.id}); const o=typeof t==='string'? JSON.parse(t) : t; if(o && o.tag) rec=Object.assign({}, rec, {tag:o.tag}); }catch(_){ }
      }
      if(rec.tag) srcs.push({g, rec});
    }
    if(!srcs.length) return this.tell('The selected objects\' records are not known yet: open one in the reference dialog, then Run.');
    const off=[p.world[0]-at[0], p.world[1]-at[1], 0];
    for(let i=0;i<s.count;i++){
      const {g, rec}=srcs[Math.floor(Math.random()*srcs.length)];
      const a=Math.random()*Math.PI*2, d=Math.sqrt(Math.random())*s.spread;
      const rot=[(Math.random()-0.5)*0.6, (Math.random()-0.5)*0.6, Math.random()*Math.PI*2];
      const scale=g.hit.scale||1;
      const local=[at[0]+Math.cos(a)*d, at[1]+Math.sin(a)*d, at[2]+s.height+(i%4)*24];
      const m=refMatrix(local, rot, scale);
      copies.push({key:'scatter:'+i, model:g.hit.model, m, tris:this.drawn(g.hit), hit:g.hit, ref:null, scale, off, copy:{tag:rec.tag, id:rec.id}});
    }
    return this.go(copies, {}, this.box([at], s.spread), o=>{
      const d=Math.hypot(o.m[3]-at[0], o.m[7]-at[1]); return d<=s.spread+256? d : Infinity;
    });
  },

  /** Runs the request and plays it: `bodies` (each with its pickable, for the ghosts), the
   *  kinetic event, the region and which objects around join in. */
  async go(bodies, kin, region, moves){
    const run=this.run={busy:true, bodies:[]};
    this.draw();
    this.tell('Moving…');
    const q=this.request(bodies, kin, region, moves);
    let got;
    try{ got=this.readDrop(unpackTables(await Engine.send('phys_drop', packTables(this.pack(q))))); }
    catch(e){ this.run=null; this.draw(); return this.tell(String(e.message||e)); }
    if(this.run!==run) return;
    run.busy=false;
    // The bodies in the order of the poses: the ones asked for, then the objects around that joined in.
    const byKey=new Map(bodies.map(b=>[String(b.key).toLowerCase(), b]));
    for(const n of q.near){
      const o=n.hit, m=o.m;
      byKey.set(String(n.key).toLowerCase(), {key:n.key, hit:o, ref:null, scale:o.scale||1, off:[o.wpos[0]-m[3], o.wpos[1]-m[7], o.wpos[2]-m[11]], joined:true});
    }
    run.bodies=(got.keys.length? got.keys : bodies.map(b=>b.key)).map(k=>byKey.get(String(k).toLowerCase())).filter(Boolean);
    run.frames=(got.frames||[]).filter(f=>f && f.length===run.bodies.length);
    if(!run.frames.length){ this.run=null; this.draw(); return this.tell('Nothing moved: none of them has a collision shape.'); }
    const skipped=(got.skipped||[]).length, joined=run.bodies.filter(b=>b.joined).length, held=got.fixed.length;
    this.tell((got.settled? 'At rest after ' : 'Still moving when the time ran out, at ')+(+got.seconds||0).toFixed(1)+' s'+
      (joined? '; '+joined+' struck object(s) moved too' : '')+(held? '; '+held+' held by the blacklist or with no collision' : '')+
      (skipped? '; '+skipped+' without a collision shape stayed put' : '')+'. Keep puts them there; Drop leaves everything as it was.');
    this.draw();
    this.play(run);
  },

  /** The motion, as ghosts, at the engine's 30 frames a second; the last frame stays. */
  play(run){
    const R=App.R; if(!R) return;
    const t0=performance.now();
    const step=()=>{
      if(this.run!==run) return;
      const i=Math.min(run.frames.length-1, Math.floor((performance.now()-t0)/1000*30));
      this.ghost(run, run.frames[i]);
      if(i<run.frames.length-1) run.raf=requestAnimationFrame(step);
    };
    step();
  },
  ghost(run, frame){
    const R=App.R; if(!R) return;
    R.setStaticHighlight(run.bodies.map((b, k)=>{
      const f=frame[k];
      return Object.assign({}, b.hit, {m:refMatrix([f[0]-b.off[0], f[1]-b.off[1], f[2]-b.off[2]], [f[3], f[4], f[5]], b.scale)});
    }), {alone:true});
    R.dirty=true;
  },

  /** Keep: the copies placed where they came to rest (one step) and every object that moved
   *  put where it came to rest (one step). */
  async commit(){
    const run=this.run; if(!run || !run.frames) return;
    if(run.raf) cancelAnimationFrame(run.raf);
    const E=WgEditor, last=run.frames[run.frames.length-1];
    const copies=[], moves=[];
    run.bodies.forEach((b, k)=>{ (b.copy? copies : moves).push([b, last[k]]); });
    this.run=null;
    let said=[];
    if(copies.length){
      const c=E.currentCell(), f0=copies[0][1];
      const cell=c && (c.key.startsWith('(')? '('+Math.floor(f0[0]/8192)+', '+Math.floor(f0[1]/8192)+')' : c.key);
      const items=copies.map(([b, f])=>({tag:b.copy.tag, id:b.copy.id, translation:[f[0], f[1], f[2]], rotation:[f[3], f[4], f[5]], scale:b.scale}));
      try{ const r=await E.ask('editPlaceMany', {cell, items}); said.push((r.placed||items).length+' placed'); }
      catch(e){ said.push('the copies could not be placed: '+String(e.message||e).replace(/^Wraithguard answered 400:\s*/,'')); }
    }
    if(moves.length){
      // The objects around that joined in have no reference yet: asked for, all at once.
      const need=moves.filter(([b])=>!b.ref);
      if(need.length){
        const refs=await E.refsOfHits(need.map(([b])=>b.hit));
        need.forEach(([b], i)=>{ b.ref=refs[i]; });
      }
      const list=moves.filter(([b])=>b.ref).map(([b, f])=>{
        const was=b.hit.wpos, rot=b.hit.rot||[0,0,0];
        const moved=Math.hypot(f[0]-was[0], f[1]-was[1], f[2]-was[2])>0.01 || [0,1,2].some(i=>Math.abs(f[3+i]-rot[i])>1e-4);
        return [b.ref, moved? [['translation', [f[0], f[1], f[2]]], ['rotation', [f[3], f[4], f[5]]]] : []];
      });
      const ok=await E.sendRefs(list);
      const n=list.filter(([, c])=>c.length).length, lost=moves.length-list.length;
      said.push(ok===false? 'not all of them could be moved' : n+' moved'+(lost? ' ('+lost+' could not be edited)' : ''));
    }
    if(copies.length && typeof E.refreshPending==='function') await E.refreshPending();
    this.clearGhost();
    this.tell(said.join('; ')+'. Ctrl+Z takes '+(copies.length && moves.length? 'each step' : 'it')+' back.');
    this.draw();
  },

  /** Drop: the run thrown away, everything as it was (no copy placed). */
  discard(quiet){
    const run=this.run; if(!run) return;
    if(run.raf) cancelAnimationFrame(run.raf);
    this.run=null;
    this.clearGhost();
    if(!quiet){ this.tell('Dropped: everything stays as it was.'); this.draw(); }
  },
  clearGhost(){
    const R=App.R; if(!R) return;
    R.setStaticHighlight(null);
    if(typeof WgEditor==='object' && WgEditor.syncSel) WgEditor.syncSel();
    R.dirty=true;
  },
};
