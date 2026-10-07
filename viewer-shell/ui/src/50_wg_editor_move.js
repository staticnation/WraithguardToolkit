/* =====================================================================================
   Wraithguard Editor (50_wg_editor.js): moving the selected object in the render window, box selection, undo and redo (the pool's), copy and paste, dropping onto what is under it.

   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
Object.assign(WgEditor, {
  /* ---- moving the selected object in the render window -------------------------------- */

  /** The selected object, when it is a reference the editor can change: the inspector's
   *  pick and its {cell, origin, refr, plugins}. */
  selected(){
    const s=this._sel, R=App.R;
    if(!this.on || !this.canEdit() || !s || !s.hit || !R) return null;
    if(!(R.pickables||[]).includes(s.hit) || !s.hit.m || !s.hit.wpos) return null;
    if(String(s.hit.refKey).toLowerCase()!==this.refKeyOf(s.ref).toLowerCase()) return null;
    return s;
  },

  /** Where a ray meets the horizontal plane at height `z`, or null when it runs along it. */
  onPlane(ray, z){
    if(Math.abs(ray.dir[2])<1e-4) return null;
    const t=(z-ray.eye[2])/ray.dir[2];
    if(!(t>0)) return null;
    return [ray.eye[0]+ray.dir[0]*t, ray.eye[1]+ray.dir[1]*t, z];
  },

  /** The renderer's `onGrab`: a press on the selected object - the drag that moves or
   *  turns it, or null to leave the press to the viewport. Positions here are the scene's
   *  (the renderer's origin); `off` takes them back to the world's. */
  grab(e){
    if(typeof WgPath==='object' && WgPath.on) return WgPath.grab(e);
    const sel=this.selected(), R=App.R;
    if(!R) return null;
    // A press on one of several selected moves them all; anywhere but on the selected
    // object draws a selection box.
    const under=R.pickAt(e.clientX,e.clientY);
    const group=this.group();
    if(group.length>1 && !e.ctrlKey && !e.metaKey && !e.shiftKey && !e.altKey && group.some(g=>g.hit===under))
      return this.grabGroup(e, group);
    const onSel=sel && !e.ctrlKey && !e.metaKey && under===sel.hit;
    if(!onSel) return (this.on && this.canEdit() && !e.altKey)? this.boxSelect(e) : null;
    const hit=sel.hit, m=hit.m;
    const local=[m[3], m[7], m[11]];
    const off=[hit.wpos[0]-local[0], hit.wpos[1]-local[1], hit.wpos[2]-local[2]];
    const st={sel, off, cur:local.slice(), rot:(hit.rot||[0,0,0]).slice(), scale:hit.scale||1,
              mode:null, base:null, rotBase:null, x0:0, y0:0, p0:null, moved:false, turned:false};
    const modeOf=ev=> ev.altKey? 'surface'
      : ev.shiftKey? 'turn'+(this.held.has('x')? 0 : this.held.has('y')? 1 : 2)
      : this.held.has('z')? 'lift' : this.held.has('x')? 'x' : this.held.has('y')? 'y' : 'slide';
    const rebase=(ev, mode)=>{
      st.mode=mode; st.base=st.cur.slice(); st.rotBase=st.rot.slice();
      st.x0=ev.clientX; st.y0=ev.clientY;
      st.p0=this.onPlane(R.rayAt(ev.clientX,ev.clientY), st.base[2]);
    };
    const ghost=()=>R.setStaticHighlight([Object.assign({}, hit, {m:refMatrix(st.cur, st.rot, st.scale)})], {alone:true});
    rebase(e, modeOf(e));       // from the press, so the first few pixels count
    return {
      move:ev=>{
        const mode=modeOf(ev);
        if(mode!==st.mode) rebase(ev, mode);
        if(mode==='surface'){
          // CSSE's Alt-drag: onto the surface under the pointer, the chosen axis turned
          // to face out of it.
          const at=this.surfaceUnder(ev.clientX, ev.clientY, hit);
          if(!at) return;
          st.cur=at.p.slice(); st.moved=true;
          st.cur=this.snapPos(st.cur, off, [true, true, false]);
          if(this.snapAxis){
            st.rot=EdMath.alignAxis(st.rotBase, this.snapAxis, at.n);
            st.turned=true;
          }
        } else if(mode.startsWith('turn')){
          const k=+mode.slice(4);
          let a=(ev.clientX-st.x0)*this.TURN;
          const step=this.angleOn? this.angle*Math.PI/180 : 0;
          if(this.rotWorld){ if(step) a=Math.round(a/step)*step; st.rot=EdMath.turnWorld(st.rotBase, k, a); }
          else{ st.rot[k]=st.rotBase[k]+a; if(step) st.rot[k]=Math.round(st.rot[k]/step)*step; }
          st.turned=true;
        } else if(mode==='lift'){
          const ray=R.rayAt(ev.clientX,ev.clientY);
          const dist=Math.hypot(ray.eye[0]-st.base[0], ray.eye[1]-st.base[1], ray.eye[2]-st.base[2]);
          const H=(R.cv && R.cv.clientHeight)||800;
          const perPx=2*Math.tan((typeof FOV==='number'? FOV : 1)/2)*dist/H;
          st.cur=this.snapPos([st.base[0], st.base[1], st.base[2]-(ev.clientY-st.y0)*perPx], off, [false, false, true]);
          st.moved=true;
        } else {
          const p=this.onPlane(R.rayAt(ev.clientX,ev.clientY), st.base[2]);
          if(!p || !st.p0) return;
          let dx=p[0]-st.p0[0], dy=p[1]-st.p0[1];
          if(mode==='x') dy=0; else if(mode==='y') dx=0;
          st.cur=this.snapPos([st.base[0]+dx, st.base[1]+dy, st.base[2]], off, [mode!=='y', mode!=='x', false]);
          st.moved=true;
        }
        ghost();
      },
      end:()=>{
        R.setStaticHighlight([hit]);
        const world=st.cur.map((v,i)=>v+off[i]);
        const sends=[];
        if(st.moved) sends.push(['translation', world]);
        if(st.turned) sends.push(['rotation', st.rot.slice()]);
        this.sendRef(sel.ref, sends);
      },
      cancel:()=>R.setStaticHighlight([hit]),
      state:st,
    };
  },

  /** Several selected objects dragged together: across the ground plane (Z held: up and
   *  down, X or Y: along that axis), the grid kept, each moved by the same amount; one
   *  undo step each. */
  grabGroup(e, group){
    const R=App.R;
    const items=group.map(g=>({g, m:g.hit.m, local:[g.hit.m[3], g.hit.m[7], g.hit.m[11]],
      off:[g.hit.wpos[0]-g.hit.m[3], g.hit.wpos[1]-g.hit.m[7], g.hit.wpos[2]-g.hit.m[11]]}));
    const lead=items[0];
    const base=lead.local.slice();
    const p0=this.onPlane(R.rayAt(e.clientX, e.clientY), base[2]);
    const y0=e.clientY;
    let d=[0,0,0], moved=false;
    const ghost=()=>R.setStaticHighlight(items.map(it=>Object.assign({}, it.g.hit,
      {m:refMatrix([it.local[0]+d[0], it.local[1]+d[1], it.local[2]+d[2]], it.g.hit.rot||[0,0,0], it.g.hit.scale||1)})), {alone:true});
    return {
      move:ev=>{
        if(this.held.has('z')){
          const ray=R.rayAt(ev.clientX, ev.clientY);
          const dist=Math.hypot(ray.eye[0]-base[0], ray.eye[1]-base[1], ray.eye[2]-base[2]);
          const H=(R.cv && R.cv.clientHeight)||800;
          const perPx=2*Math.tan((typeof FOV==='number'? FOV : 1)/2)*dist/H;
          d=[0, 0, -(ev.clientY-y0)*perPx];
        }else{
          const p=this.onPlane(R.rayAt(ev.clientX, ev.clientY), base[2]);
          if(!p || !p0) return;
          d=[p[0]-p0[0], p[1]-p0[1], 0];
          if(this.held.has('x')) d[1]=0; else if(this.held.has('y')) d[0]=0;
        }
        // On the grid: the lead object lands on it, the rest keep their spacing.
        const snapped=this.snapPos([base[0]+d[0], base[1]+d[1], base[2]+d[2]], lead.off, [true, true, this.held.has('z')]);
        d=[snapped[0]-base[0], snapped[1]-base[1], snapped[2]-base[2]];
        moved=true;
        ghost();
      },
      end:async()=>{
        R.setStaticHighlight(items.map(it=>it.g.hit));
        if(!moved) return;
        for(const it of items){
          const w=it.g.hit.wpos;
          await this.sendRef(it.g.ref, [['translation', [w[0]+d[0], w[1]+d[1], w[2]+d[2]]]]);
        }
        toast(items.length+' moved','ok',1500);
      },
      cancel:()=>R.setStaticHighlight(items.map(it=>it.g.hit)),
      state:{group:true},
    };
  },

  /* ---- box selection ------------------------------------------------------------------ */

  /** Where a point of the scene is on screen: `[x, y]` in the page's pixels, or null
   *  behind the camera. The projection the frame was drawn with. */
  projector(){
    const R=App.R; if(!R || !R.cam || !R.cv || typeof M4!=='object') return null;
    const rect=R.cv.getBoundingClientRect(), W=rect.width||1, H=rect.height||1, c=R.cam;
    const ce=Math.cos(c.el), se=Math.sin(c.el);
    const eye=[c.tx+c.dist*ce*Math.cos(c.az), c.ty+c.dist*ce*Math.sin(c.az), c.tz+c.dist*se];
    const [n, f]=R.nearFar? R.nearFar() : [1, 1e6];
    const VP=M4.mul(M4.persp(typeof FOV==='number'? FOV : 0.84, W/H, n, f), M4.lookAt(eye, [c.tx,c.ty,c.tz], [0,0,1]));
    return p=>{
      const x=VP[0]*p[0]+VP[4]*p[1]+VP[8]*p[2]+VP[12];
      const y=VP[1]*p[0]+VP[5]*p[1]+VP[9]*p[2]+VP[13];
      const w=VP[3]*p[0]+VP[7]*p[1]+VP[11]*p[2]+VP[15];
      if(!(w>0)) return null;
      return [rect.left+((x/w)*0.5+0.5)*W, rect.top+(1-((y/w)*0.5+0.5))*H];
    };
  },

  /** The placed objects whose middle is inside a screen rectangle (`{l, t, r, b}`, the
   *  page's pixels): the bounding box's centre, or where the object stands. */
  pickInRect(rect){
    const R=App.R, proj=this.projector();
    if(!R || !proj) return [];
    const out=[];
    for(const h of R.pickables||[]){
      if(!h || !h.refKey || !h.m) continue;
      const a=h.aabb;
      const p= a && a.x0!=null? [(a.x0+a.x1)/2, (a.y0+a.y1)/2, (a.z0+a.z1)/2] : [h.m[3], h.m[7], h.m[11]];
      const s=proj(p);
      if(s && s[0]>=rect.l && s[0]<=rect.r && s[1]>=rect.t && s[1]<=rect.b) out.push(h);
    }
    return out;
  },

  /** A left drag that starts off the selected object: a box, as the Construction Set's.
   *  Let go and what is inside is selected (Ctrl or Shift held at the press: added to
   *  what is selected) - Q acts on them all, Esc lets them go. A still click picks, as
   *  ever. */
  boxSelect(e){
    const R=App.R, host=$('#vpwrap');
    if(!R || !host) return null;
    const add=e.ctrlKey || e.metaKey || e.shiftKey;
    const x0=e.clientX, y0=e.clientY;
    const before=add? this.group().map(g=>g.hit) : [];
    let box=null, t=0, rect=null;
    const rectOf=ev=>({l:Math.min(x0, ev.clientX), t:Math.min(y0, ev.clientY), r:Math.max(x0, ev.clientX), b:Math.max(y0, ev.clientY)});
    const done=()=>{ if(box) box.remove(); box=null; };
    return {
      move:ev=>{
        rect=rectOf(ev);
        if(!box){ box=document.createElement('div'); box.id='edBox'; host.appendChild(box); }
        const hr=host.getBoundingClientRect();
        Object.assign(box.style, {left:(rect.l-hr.left)+'px', top:(rect.t-hr.top)+'px', width:(rect.r-rect.l)+'px', height:(rect.b-rect.t)+'px'});
        const now=performance.now();
        if(now-t>90){
          t=now;
          const inside=this.pickInRect(rect);
          box.dataset.n=inside.length;
          R.setStaticHighlight(before.concat(inside.filter(h=>!before.includes(h))), {alone:true});
        }
      },
      end:ev=>{
        done();
        const r=rect||rectOf(ev);
        // A hand that shook a few pixels on a click: a click, not an empty box.
        if(r.r-r.l<10 && r.b-r.t<10){
          R.setStaticHighlight(null);
          const hit=R.pickAt(x0, y0);
          if(typeof Ori==='object') return Ori.pick(hit, {ctrlKey:e.ctrlKey, metaKey:e.metaKey, shiftKey:e.shiftKey});
          return null;
        }
        R.setStaticHighlight(null);
        return this.selectHits(this.pickInRect(r), add);
      },
      cancel:()=>{ done(); R.setStaticHighlight(null); this.syncSel(); },
      state:{box:true},
    };
  },

  /** What names a placed object to Wraithguard, asked of the engine (`ori`), or null. */
  async refOfHit(hit){
    const key=String(hit.refKey||'').toLowerCase();
    if(key.startsWith('new:')){
      const uid=key.slice(4);
      for(const [c, list] of this.liveNew) if(list.some(a=>a.uid===uid)) return {cell:c, uid};
      return null;
    }
    try{
      const r=await Engine.call('ori',{key:hit.refKey, model:hit.model||''});
      if(r && r.refCell && r.createdBy && r.refIndex!=null)
        return {cell:r.refCell, origin:r.createdBy, refr:r.refIndex|0, plugins:r.cellPlugins||null};
    }catch(_){ }
    return null;
  },

  /** Selects several objects at once (a box): in place of the selection, or added to it. */
  async selectHits(hits, add){
    const R=App.R;
    const CAP=500;
    if(!add){ this._group=[]; this._sel=null; if(typeof Ori==='object' && Ori.el) Ori.el.hidden=true; }
    else if(!this._group.length && this._sel && this._sel.hit) this._group.push({hit:this._sel.hit, ref:this._sel.ref});
    const have=new Set(this._group.map(g=>String(g.hit.refKey).toLowerCase()));
    const want=hits.filter(h=>!have.has(String(h.refKey).toLowerCase())).slice(0, CAP);
    if(hits.length>CAP) toast('Only the first '+CAP+' objects in the box are selected','warn',3000);
    // Asked of the engine eight at a time: a box over a town is hundreds of objects.
    let skipped=0;
    for(let i=0;i<want.length;i+=8){
      const refs=await Promise.all(want.slice(i, i+8).map(h=>this.refOfHit(h)));
      refs.forEach((ref, j)=>{ if(ref) this._group.push({hit:want[i+j], ref}); else skipped++; });
    }
    this.syncSel();
    if(R) R.setStaticHighlight(null);
    const n=this._group.length;
    toast(n? n+' selected'+(skipped? ' ('+skipped+' that cannot be edited left out)' : '')+' - Q for what to do with them, Esc to clear'
           : 'Nothing in the box','ok',2500);
    return n;
  },

  /** Queue changes to a reference (`[[path, value], ...]`), one after another; the dialog,
   *  when it shows that reference, and the render window follow. */
  async sendRef(ref, changes, opts){
    if(!changes.length) return;
    if(!(opts && opts.noUndo)){
      // What it was, from the object as the render window has it, so Ctrl+Z puts it back.
      const prior=this.priorOf(ref, changes);
      if(prior){ this._undo.push({ref:Object.assign({}, ref), changes:changes.map(c=>c.slice()), prior}); this._redo=[]; }
      if(this._undo.length>200) this._undo.shift();
    }
    let v=null;
    try{
      for(const [path, value] of changes)
        v=await this.ask(ref.uid? 'editNewSet' : 'editRefSet', Object.assign(this.refBody(ref), {path, value}));
    }catch(e){
      toast(String(e.message||e).replace(/^Wraithguard answered 400:\s*/,''),'err',6000);
    }
    // A new reference dragged into the next exterior cell now belongs to that one.
    if(v && ref.uid && v.cell) ref.cell=v.cell;
    const same=this.refRec && this.dlg && !this.dlg.hidden && this.refKeyOf(this.refRec)===this.refKeyOf(ref);
    if(v && same) this.drawRef(v);
    if(!this.refRec || !same) this.refRec=Object.assign({}, ref);
    await this.refreshPending();
  },

  /** A position snapped to the grid (the world's coordinates, not the scene's: `off` is
   *  the scene's offset), on the axes `which` says - unchanged with Snap to Grid off. */
  snapPos(p, off, which){
    if(!this.gridOn || !(this.grid>0)) return p;
    const g=this.grid;
    return p.map((v,i)=> which[i]? Math.round((v+off[i])/g)*g-off[i] : v);
  },

  /** The values a change to a reference replaces, read off the object the render window
   *  drew (its selection, or one of Ctrl+click's) - or null when it is not on screen. */
  priorOf(ref, changes){
    const key=this.refKeyOf(ref).toLowerCase();
    const picks=[this._sel, ...(this._group||[])].filter(Boolean);
    const g=picks.find(x=>x.ref && this.refKeyOf(x.ref).toLowerCase()===key);
    const h=g && g.hit;
    if(!h) return null;
    const was={translation:h.wpos? h.wpos.slice() : null, rotation:(h.rot||[0,0,0]).slice(), scale:h.scale||1, deleted:false};
    const out=[];
    for(const [path] of changes){
      if(!(path in was) || was[path]==null) return null;
      out.push([path, was[path]]);
    }
    return out;
  },

  /** Ctrl+Z: the last change to a reference made here, taken back. */
  async undo(){
    if(this.links().editUndo) return this.poolStep('editUndo');
    const u=this._undo.pop();
    if(!u) return toast('Nothing to undo','warn',1500);
    this._redo.push(u);
    await this.sendRef(u.ref, u.prior, {noUndo:true});
    toast('Undone: '+u.changes.map(c=>this.REF_LABELS[c[0]]||c[0]).join(', ').toLowerCase(),'ok',1800);
  },
  /** Ctrl+Y: what Ctrl+Z took back, made again. */
  async redo(){
    if(this.links().editRedo) return this.poolStep('editRedo');
    const u=this._redo.pop();
    if(!u) return toast('Nothing to redo','warn',1500);
    this._undo.push(u);
    await this.sendRef(u.ref, u.changes, {noUndo:true});
    toast('Redone','ok',1500);
  },

  /** Ctrl+Z / Ctrl+Y as one history for everything the Editor changes: the patch pool
   *  steps back or forward (Wraithguard keeps the history, `editUndo` / `editRedo`), and
   *  what is open here is read again - the render window's objects, the dialog, the path
   *  grid, the topic. */
  async poolStep(link){
    let r;
    try{ r=await this.ask(link, {}); }
    catch(e){ return toast(String(e.message||e).replace(/^Wraithguard answered 400:\s*/,''),'err',5000); }
    const hist={undo:r.undo|0, redo:r.redo|0};
    this._hist=hist;
    if(!r.done){ toast(link==='editUndo'? 'Nothing to undo' : 'Nothing to redo','warn',1500); if(typeof WgUI==='object') WgUI.refresh(); return r; }
    await this.refreshPending();
    await this.rereadViews(true);
    const what=(r.what||[]);
    toast((link==='editUndo'? 'Undone' : 'Redone')+(what.length? ': '+what.slice(0,3).join(', ')+(what.length>3? ' and '+(what.length-3)+' more' : '') : ''),'ok',2500);
    this._hist=hist;            // refreshPending forgets it: a change elsewhere is undoable
    if(typeof WgUI==='object') WgUI.refresh();
    return r;
  },

  /** The open views read again from the pool: the record or reference dialog, the path
   *  grid, the topic. `force`: the dialog too while something in it has the focus (an
   *  undo asked for it; a change from elsewhere does not take a field from under the
   *  cursor - the dialog says it is out of date instead). */
  async rereadViews(force){
    const dlgOpen=this.dlg && !this.dlg.hidden;
    const typing=dlgOpen && document.activeElement && this.dlg.contains(document.activeElement) && /^(INPUT|TEXTAREA|SELECT)$/.test(document.activeElement.tagName);
    if(dlgOpen && typing && !force){
      const t=this.dlg.querySelector('#edDlgTitle');
      if(t && !t.querySelector('.edStale')) t.insertAdjacentHTML('beforeend', ' <span class="edStale from" title="The pool changed elsewhere (the Patch Builder, an undo): open it again to see it">changed elsewhere</span>');
    }else if(dlgOpen && this.refRec) await this.openRef(this.refRec);
    else if(dlgOpen && this.record) await this.openRecord(this.record.tag, this.record.id, this.record.plugins);
    if(typeof WgPath==='object' && WgPath.on) await WgPath.load();
    if(this.dial && !this.dial.hidden && this._dialSel) await this.openTopic(this._dialSel);
  },

  /** Ctrl+C: the selected object, to place copies of (Ctrl+V). */
  copySel(){
    const sel=this.selected(); if(!sel) return false;
    const rec=this.selectedRecord(sel);
    if(!rec.tag) return toast('The selected object\'s record is not known yet','warn',2500), true;
    this._clip={tag:rec.tag, id:rec.id, model:rec.model, rotation:(sel.hit.rot||[0,0,0]).slice(), scale:sel.hit.scale||1};
    toast('Copied '+rec.id+' - Ctrl+V places one under the pointer','ok',2500);
    return true;
  },
  /** Ctrl+V: a new reference of the copied object, under the pointer (on the grid when
   *  Snap to Grid is on), turned as the original was. */
  async paste(){
    const c=this._clip; if(!c) return toast('Nothing copied (select an object, Ctrl+C)','warn',2500);
    const pt=this._pointer, R=App.R;
    const over=pt && R && R.cv && R.cv.getBoundingClientRect && (r=>pt[0]>=r.left && pt[0]<=r.right && pt[1]>=r.top && pt[1]<=r.bottom)(R.cv.getBoundingClientRect());
    const v=await this.placeAt(c, over? pt[0] : null, over? pt[1] : null, null, c.rotation);
    if(v && c.scale!==1) await this.sendRef({cell:v.cell, uid:v.uid}, [['scale', c.scale]], {noUndo:true});
    return v;
  },

  /** C: the view's pivot onto the selected object, so the camera turns about it. */
  centreOnSel(){
    const sel=this.selected(), R=App.R;
    if(!sel || !R || !R.cam) return false;
    const m=sel.hit.m;
    R.cam.tx=m[3]; R.cam.ty=m[7]; R.cam.tz=m[11];
    R.dirty=true;
    return true;
  },

  /** The surface under the pointer, leaving out `self`: `{p, n}` in the scene's
   *  coordinates, its normal from the hits a few pixels either side (an object's faces
   *  carry none the picker returns), or the ground's from its slope. */
  surfaceUnder(x, y, self){
    const R=App.R; if(!R) return null;
    const all=R.pickables;
    const at=(cx, cy)=>{
      let s=null;
      try{ R.pickables=(all||[]).filter(o=>o!==self); s=R.surfaceAt? R.surfaceAt(cx, cy) : null; }
      finally{ R.pickables=all; }
      const g=R.groundAt? R.groundAt(cx, cy) : null;
      const gp=g? [g[0], g[1], (g.length>2 && g[2]!=null)? g[2] : (R.groundZ? R.groundZ(g[0], g[1]) : 0)] : null;
      if(s && s.p){
        if(!gp) return s.p;
        const eye=R.cameraEye? R.cameraEye() : [0,0,0];
        const d=q=>Math.hypot(q[0]-eye[0], q[1]-eye[1], q[2]-eye[2]);
        return d(s.p)<=d(gp)? s.p : gp;
      }
      return gp;
    };
    const p=at(x, y); if(!p) return null;
    const a=at(x+3, y), b=at(x, y+3);
    let n=[0,0,1];
    if(a && b){
      const u=[a[0]-p[0], a[1]-p[1], a[2]-p[2]], v=[b[0]-p[0], b[1]-p[1], b[2]-p[2]];
      const c=[u[1]*v[2]-u[2]*v[1], u[2]*v[0]-u[0]*v[2], u[0]*v[1]-u[1]*v[0]];
      const L=Math.hypot(c[0], c[1], c[2]);
      if(L>1e-6){ n=c.map(q=>q/L); if(n[2]<0) n=n.map(q=>-q); }
    }
    return {p, n};
  },

  /** F: the selected object dropped onto the ground or the object under it, its lowest
   *  point on the surface (the Construction Set's "drop to ground"). */
  drop(){
    const sel=this.selected(), R=App.R;
    if(!sel || !R) return false;
    const hit=sel.hit, m=hit.m, a=hit.aabb;
    const local=[m[3], m[7], m[11]];
    const bottom=a? a.z0 : local[2];
    let surface=null;
    if(R.groundZ){ const g=R.groundZ(local[0], local[1]); if(g!=null && g<=bottom+1) surface=g; }
    const all=R.pickables;
    try{
      R.pickables=(all||[]).filter(o=>o!==hit);
      const under=R.pickRay([local[0], local[1], bottom-0.5], [0,0,-1]);
      if(under && (surface==null || under.p[2]>surface)) surface=under.p[2];
    }finally{ R.pickables=all; }
    if(surface==null){ toast('Nothing under it to drop onto','warn',3000); return true; }
    const z=local[2]+(surface-bottom);
    const off=hit.wpos[2]-local[2];
    this.sendRef(sel.ref, [['translation', [hit.wpos[0], hit.wpos[1], z+off]]]);
    return true;
  },
});
