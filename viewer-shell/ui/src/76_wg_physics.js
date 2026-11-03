/* =====================================================================================
   Wraithguard: the Editor's Physics drop. The selected objects fall, tumble and come to
   rest on what is around them - the other objects' collision and, outside, the land - and
   the fall plays in the render window as gold ghosts. Keep writes where they came to
   rest as one move (one Ctrl+Z takes it back); Drop throws it away.

   OpenMW does not simulate loose objects (they stay where the plugin puts them), so this
   is a placing tool, not the game: the surfaces are the game's collision, the settling is
   Rapier's (viewer-shell/physcore, the engine's `phys_drop`). Copyright (c) 2026
   StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
const WgPhysics={
  st:{gravity:1, friction:0.7, bounce:0.05, seconds:6, dropOnly:false, hollow:true},
  run:null,               // {bodies:[{hit, ref, off, scale}], frames, i, raf}
  REACH:1024,             // how far around the bodies other objects count as ground (units)

  load(){
    try{ const s=JSON.parse(localStorage.getItem('wgPhysics')||'null'); if(s) Object.assign(this.st, s); }catch(_){ }
  },
  keep(){ try{ localStorage.setItem('wgPhysics', JSON.stringify(this.st)); }catch(_){ } },

  toggle(){
    if(this._el && !this._el.hidden){ this.close(); return; }
    this.panel(); this._el.hidden=false;
  },
  close(){ this.discard(true); if(this._el) this._el.hidden=true; },

  panel(){
    if(this._el) return this.draw();
    this.load();
    const d=document.createElement('div');
    d.id='wgPhysPanel'; d.className='ori'; d.hidden=true;
    d.innerHTML='<div class="orihead"><b>Physics drop</b><span style="flex:1"></span>'+
      '<button class="btn dim ic" id="wgPhysX" title="Close">&#x2715;</button></div><div class="oribody" id="wgPhysBody"></div>';
    if(typeof WgUI==='object' && WgUI.mount) WgUI.mount(d); else ($('#vpwrap')||document.body).appendChild(d);
    d.querySelector('#wgPhysX').onclick=()=>this.close();
    this._el=d;
    this.draw();
  },

  /** The sliders, the buttons and what the last run did. */
  draw(){
    const box=this._el && this._el.querySelector('#wgPhysBody'); if(!box) return;
    const s=this.st, r=this.run;
    const rng=(k, label, min, max, step, tip)=>'<div class="row"><label title="'+escHtml(tip)+'">'+label+'</label><div class="ctl"><div class="rng">'+
      '<input type="range" min="'+min+'" max="'+max+'" step="'+step+'" value="'+s[k]+'" data-k="'+k+'"><span class="val" data-v="'+k+'">'+this.shown(k)+'</span></div></div></div>';
    let h=rng('gravity', 'Gravity', 0.1, 3, 0.05, 'How hard they fall, as a part of the world\'s (1 is 9.81 m/s², about 690 units a second each second)')+
      rng('friction', 'Friction', 0, 1.5, 0.05, 'How much they grip what they land on: low slides, high sticks')+
      rng('bounce', 'Bounce', 0, 0.9, 0.05, 'How much of the fall comes back as a bounce')+
      rng('seconds', 'Time limit', 1, 20, 1, 'The most seconds to let them fall and roll; they stop sooner when they come to rest');
    h+='<div class="lrow" style="display:flex;gap:6px;flex-wrap:wrap;margin:6px 0">'+
      '<button class="btn sm'+(s.dropOnly? ' pri' : '')+'" id="wgPhysOnly" title="Only fall: they keep their turn and do not tip over">Drop only</button>'+
      '<button class="btn sm'+(s.hollow? ' pri' : '')+'" id="wgPhysHollow" title="Keep their hollows: a bowl or a cart is split into convex parts, so things can land inside it. Off, each is one hull round all of it (faster)">Hollow shapes</button></div>';
    h+='<div class="lrow" style="display:flex;gap:6px;flex-wrap:wrap;margin:6px 0">'+
      '<button class="btn sm pri" id="wgPhysRun" title="Let the selected objects fall (the selection, or every object Ctrl+clicked)"'+(r && r.busy? ' disabled' : '')+'>Run</button>'+
      '<button class="btn sm" id="wgPhysKeep" title="Put them where they came to rest: one move, one Ctrl+Z"'+(r && r.frames? '' : ' disabled')+'>Keep</button>'+
      '<button class="btn sm" id="wgPhysDrop" title="Throw the run away; they stay where they were"'+(r? '' : ' disabled')+'>Drop</button></div>';
    h+='<div class="hint" id="wgPhysSay">'+escHtml(this.say||'Select objects in the render window (Ctrl+click for more than one), then Run. They fall onto the other objects\' collision and, outside, the land.')+'</div>';
    box.innerHTML=h;
    box.querySelectorAll('input[data-k]').forEach(el=>el.oninput=()=>{
      s[el.dataset.k]=+el.value; this.keep();
      const v=box.querySelector('[data-v="'+el.dataset.k+'"]'); if(v) v.textContent=this.shown(el.dataset.k);
    });
    box.querySelector('#wgPhysOnly').onclick=()=>{ s.dropOnly=!s.dropOnly; this.keep(); this.draw(); };
    box.querySelector('#wgPhysHollow').onclick=()=>{ s.hollow=!s.hollow; this.keep(); this.draw(); };
    box.querySelector('#wgPhysRun').onclick=()=>this.start();
    box.querySelector('#wgPhysKeep').onclick=()=>this.commit();
    box.querySelector('#wgPhysDrop').onclick=()=>this.discard();
  },
  shown(k){
    const v=this.st[k];
    return k==='gravity'? '×'+(+v).toFixed(2) : k==='seconds'? v+' s' : (+v).toFixed(2);
  },
  tell(t){ this.say=t; const el=this._el && this._el.querySelector('#wgPhysSay'); if(el) el.textContent=t; },

  /** What falls: the Ctrl+click group, or the one selected object. */
  bodies(){
    const E=WgEditor, g=E.group();
    if(g.length>1) return g;
    const sel=E.selected();
    return sel && sel.ref && sel.hit && sel.hit.m? [{hit:sel.hit, ref:sel.ref}] : [];
  },

  /** The request for the engine: the bodies, what is near them, the land. */
  request(list){
    const R=App.R, sc=App._scene||{};
    const keys=new Set(list.map(g=>String(g.hit.refKey||'').toLowerCase()));
    const lo=[Infinity, Infinity], hi=[-Infinity, -Infinity];
    for(const g of list) for(let k=0;k<2;k++){ const v=g.hit.m[k*4+3]; lo[k]=Math.min(lo[k], v); hi[k]=Math.max(hi[k], v); }
    const near=o=>{
      const a=o.aabb, x=o.m[3], y=o.m[7], r=this.REACH;
      if(a) return a.x1>=lo[0]-r && a.x0<=hi[0]+r && a.y1>=lo[1]-r && a.y0<=hi[1]+r;
      return x>=lo[0]-r && x<=hi[0]+r && y>=lo[1]-r && y<=hi[1]+r;
    };
    const statics=[];
    for(const o of (R && R.pickables)||[]){
      if(!o || !o.model || !o.m || o.hidden || keys.has(String(o.refKey||'').toLowerCase())) continue;
      if(near(o)) statics.push({model:o.model, m:Array.from(o.m).slice(0, 12)});
    }
    const exterior=!(sc.centre && sc.centre.kind==='int');
    return {
      bodies:list.map(g=>({key:String(g.hit.refKey||''), model:g.hit.model, m:Array.from(g.hit.m).slice(0, 12), tris:this.drawn(g.hit)})),
      statics, origin:sc.origin||[0,0], exterior, land:exterior? this.land(lo, hi) : [],
      settings:{gravity:9.81*70.3*this.st.gravity, friction:this.st.friction, bounce:this.st.bounce, seconds:this.st.seconds, dropOnly:this.st.dropOnly, hollow:this.st.hollow},
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
  /** The ground as the render window has it, squares near the bodies (the Land view's
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

  async start(){
    if(!(typeof WgEditor==='object' && WgEditor.canEdit && WgEditor.canEdit())) return this.tell('Open the Editor on a plugin to move objects.');
    const list=this.bodies().filter(g=>g.hit.model);
    if(!list.length) return this.tell('Select an object in the render window first (Ctrl+click for more).');
    this.discard(true);
    const run=this.run={busy:true, bodies:list.map(g=>{
      const m=g.hit.m;
      return {hit:g.hit, ref:g.ref, scale:g.hit.scale||1, off:[g.hit.wpos[0]-m[3], g.hit.wpos[1]-m[7], g.hit.wpos[2]-m[11]]};
    })};
    this.draw();
    this.tell('Falling…');
    let got;
    try{ got=await Engine.call('phys_drop', {request:JSON.stringify(this.request(list))}); }
    catch(e){ this.run=null; this.draw(); return this.tell(String(e.message||e)); }
    if(this.run!==run) return;
    run.busy=false;
    run.frames=(got.frames||[]).filter(f=>f && f.length===list.length);
    if(!run.frames.length){ this.run=null; this.draw(); return this.tell('Nothing fell: none of them has a collision shape.'); }
    const skipped=(got.skipped||[]).length;
    this.tell((got.settled? 'At rest after ' : 'Still moving when the time ran out, at ')+(+got.seconds||0).toFixed(1)+' s'+
      (skipped? '; '+skipped+' without a collision shape stayed put' : '')+'. Keep puts them there; Drop leaves them where they were.');
    this.draw();
    this.play(run);
  },

  /** The fall, as ghosts, at the engine's 30 frames a second; the last frame stays. */
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

  /** Keep: each where it came to rest, sent as one step. */
  async commit(){
    const run=this.run; if(!run || !run.frames) return;
    if(run.raf) cancelAnimationFrame(run.raf);
    const last=run.frames[run.frames.length-1];
    const list=run.bodies.map((b, k)=>{
      const f=last[k], was=b.hit.wpos, rot=b.hit.rot||[0,0,0];
      const moved=Math.hypot(f[0]-was[0], f[1]-was[1], f[2]-was[2])>0.01 || [0,1,2].some(i=>Math.abs(f[3+i]-rot[i])>1e-4);
      return [b.ref, moved? [['translation', [f[0], f[1], f[2]]], ['rotation', [f[3], f[4], f[5]]]] : []];
    });
    this.run=null;
    const ok=await WgEditor.sendRefs(list);
    this.clearGhost();
    this.tell(ok===false? 'Not all of them could be moved.' : list.filter(([, c])=>c.length).length+' moved. Ctrl+Z takes it back.');
    this.draw();
  },

  /** Drop: the run thrown away, the objects where they were. */
  discard(quiet){
    const run=this.run; if(!run) return;
    if(run.raf) cancelAnimationFrame(run.raf);
    this.run=null;
    this.clearGhost();
    if(!quiet){ this.tell('Dropped: they stay where they were.'); this.draw(); }
  },
  clearGhost(){
    const R=App.R; if(!R) return;
    R.setStaticHighlight(null);
    if(typeof WgEditor==='object' && WgEditor.syncSel) WgEditor.syncSel();
    R.dirty=true;
  },
};
