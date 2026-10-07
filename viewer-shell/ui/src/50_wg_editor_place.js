/* =====================================================================================
   Wraithguard Editor (50_wg_editor.js): placing new references, and the render window drawn as the pending changes leave it.

   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
Object.assign(WgEditor, {
  /* ---- placing new references -------------------------------------------------------- */

  /** The render window takes records dropped on it from the Object Window (once). */
  hookDrop(){
    // The drag is WgUI.armDrag's (pointer events); this takes only a native drop that
    // still gets through, from a page without it.
    const cv=App.R && App.R.cv;
    if(!cv || cv._wgDrop) return;
    cv._wgDrop=true;
    const ours=e=>this.on && e.dataTransfer && [...(e.dataTransfer.types||[])].includes('application/x-wg-record');
    cv.addEventListener('dragover', e=>{ if(ours(e)){ e.preventDefault(); e.dataTransfer.dropEffect='copy'; WgUI.dragOver(e); } });
    cv.addEventListener('dragleave', ()=>WgUI.dropMarker(null));
    cv.addEventListener('drop', e=>{
      if(!ours(e)) return;
      e.preventDefault();
      WgUI.dragEnd();
      let rec=null; try{ rec=JSON.parse(e.dataTransfer.getData('application/x-wg-record')); }catch(_){ }
      if(rec) this.placeAt(rec, e.clientX, e.clientY);
    });
  },

  /** Where a new reference goes: the surface under the pointer (an object, or the
   *  ground), or the view's pivot when there is no pointer or nothing under it - the
   *  scene's coordinates and the world's. */
  placePoint(clientX, clientY){
    const R=App.R, sc=App._scene;
    if(!R || !R.cam || !sc || !sc.origin) return null;
    let p=null;
    if(clientX!=null){
      const s=R.surfaceAt? R.surfaceAt(clientX, clientY) : null;
      const g=R.groundAt? R.groundAt(clientX, clientY) : null;
      const eye=R.cameraEye? R.cameraEye() : [0,0,0];
      const d=q=>q? Math.hypot(q[0]-eye[0], q[1]-eye[1], q[2]-eye[2]) : Infinity;
      p= (s && s.p && d(s.p)<=d(g))? s.p : g;
    }
    if(!p) p=[R.cam.tx, R.cam.ty, R.cam.tz];
    return {local:p.slice(), world:[p[0]+sc.origin[0], p[1]+sc.origin[1], p[2]]};
  },

  /** Places a record (`{tag, id, model, defined}`) as a new reference: in the cell on
   *  screen, an exterior's by where it lands. Opens it in the reference dialog. */
  async placeAt(rec, clientX, clientY, world, rotation){
    if(!this.canEdit()) return toast('Placing needs Wraithguard: open this viewer from Wraithguard (Cell Preview)','warn',5000);
    const at=world? {world:world.slice()} : this.placePoint(clientX, clientY), sel=App.cellSel;
    if(!at || !sel) return toast('Open a cell first','warn',3000);
    if(!world && this.gridOn && this.grid>0) at.world=at.world.map((v,i)=> i<2? Math.round(v/this.grid)*this.grid : v);
    const cellArg= sel.kind==='int'? 'int:'+sel.name : Math.floor(at.world[0]/CELL)+','+Math.floor(at.world[1]/CELL);
    let cell;
    try{ cell=await Engine.call('editor_cell_refs',{cell:cellArg}); }
    catch(e){ return toast('No cell there to place into: '+String(e.message||e),'warn',5000); }
    if(rec.model) CellData.models.set(String(rec.id).toLowerCase(), rec.model);
    await this.actorsFor([rec.id]);
    let v;
    try{
      v=await this.ask('editPlace', {cell:cell.refCell, tag:rec.tag, id:rec.id, translation:at.world, rotation:rotation||[0,0,0],
                                     plugins:cell.plugins||null, defined:rec.defined||null});
    }catch(e){ return toast(String(e.message||e).replace(/^Wraithguard answered 400:\s*/,''),'err',6000); }
    this.refRec={cell:v.cell, uid:v.uid};
    this.dialog().hidden=false;
    this.drawRef(v);
    await this.refreshPending();
    return v;
  },

  /** What draws an NPC, a creature or a spawn point the render window has not seen placed
   *  (the cell's `actors` map is per cell): asked of the engine for the ids it lacks. */
  async actorsFor(ids){
    const want=[...new Set(ids.map(i=>String(i).toLowerCase()))].filter(k=>!CellData.actors.has(k));
    if(!want.length) return false;
    try{
      const r=await Engine.call('editor_actors',{ids:want});
      let got=false;
      for(const [k,a] of Object.entries(r.actors||{})){ CellData.actors.set(k,a); got=true; }
      for(const [k,m] of Object.entries(r.models||{})) if(!CellData.models.has(k)) CellData.models.set(k,m);
      return got;
    }catch(_){ return false; }
  },

  /** The models of every record of a tag, for objects the render window has not drawn
   *  yet (a reference that now places another): asked of the engine once per tag. */
  async loadModels(tag){
    if(!tag) return;
    this._modelTags=this._modelTags||new Set();
    if(this._modelTags.has(tag)) return;
    this._modelTags.add(tag);
    try{
      const r=await Engine.call('editor_records',{tag});
      for(const row of r.rows||[]) if(row[2] && !CellData.models.has(row[0].toLowerCase())) CellData.models.set(row[0].toLowerCase(), row[2]);
    }catch(_){ }
  },

  /** The models of new references' objects the page has not seen placed (a journal
   *  brought back after a restart): asked of the engine by tag, once each. */
  async modelsFor(liveNew){
    const ids=[];
    for(const list of liveNew.values()) for(const a of list) if(a.fields.id) ids.push(a.fields.id);
    const actors=ids.length? await this.actorsFor(ids) : false;
    const want=new Set();
    for(const list of liveNew.values()) for(const a of list)
      if(a.tag && a.fields.id && !CellData.models.has(String(a.fields.id).toLowerCase())) want.add(a.tag);
    let got=actors;
    if(!want.size && !got) return;
    this._modelTags=this._modelTags||new Set();
    for(const tag of want){
      if(this._modelTags.has(tag)) continue;
      this._modelTags.add(tag);
      try{
        const r=await Engine.call('editor_records',{tag});
        for(const row of r.rows||[]) if(row[2] && !CellData.models.has(row[0].toLowerCase())){ CellData.models.set(row[0].toLowerCase(), row[2]); got=true; }
      }catch(_){ }
    }
    if(got){ this._liveSig+=' '; if(typeof schedulePreview==='function' && App.cellSel) schedulePreview(); }
  },

  /** The inspector's pick of a new reference (24_ori.js asks first): it has no record in
   *  the world yet, so the reference dialog is its inspector. */
  showNew(hit){
    if(!this.on || !hit || !String(hit.refKey||'').startsWith('new:')) return false;
    const uid=String(hit.refKey).slice(4);
    let cell=null;
    for(const [k, list] of this.liveNew) if(list.some(a=>a.uid===uid)) cell=k;
    if(cell==null) return false;
    const ref={cell, uid};
    this._sel={hit, ref};
    this.syncSel();
    if(typeof Ori==='object'){ Ori._hit=hit; if(Ori.el) Ori.el.hidden=true; }
    if(App.R) App.R.setStaticHighlight([hit]);
    this.openRef(ref);
    return true;
  },

  /* ---- the render window, as the pending changes leave it ------------------------------ */

  /** A loaded cell with the pending reference changes applied (CellData.live). The same
   *  record when none touches it; else a copy - moved, turned, scaled, deleted ones gone. */
  overlay(rec){
    if(!rec.refs) return rec;
    const cellKey=rec.kind==='int'? String(rec.name||'').toLowerCase() : '('+rec.gx+', '+rec.gy+')';
    const added=this.liveNew.get(cellKey)||[];
    const hide=this.hiddenKeys();
    const touched=r=>{ const k=String(r.key).toLowerCase(); return this.live.has(k) || hide.has(k); };
    if(!added.length && !rec.refs.some(touched)) return rec;
    const refs=[];
    for(const r of rec.refs){
      if(hide.has(String(r.key).toLowerCase())) continue;
      const c=this.live.get(String(r.key).toLowerCase());
      if(!c){ refs.push(r); continue; }
      if(c.deleted) continue;
      const n=Object.assign({}, r);
      if(c.id) n.id=c.id;
      if(Array.isArray(c.translation)) n.pos=c.translation.slice();
      if(Array.isArray(c.rotation)) n.rot=c.rotation.slice();
      if('scale' in c) n.scale= c.scale==null? 1 : c.scale;
      if('destination' in c){
        const t=c.destination;
        n.door= t? {pos:t.translation.slice(), rot:t.rotation.slice(), cell:t.cell||''} : null;
      }
      n.edited=true;
      refs.push(n);
    }
    for(const a of added){
      const f=a.fields;
      if(!f.translation || f.deleted || hide.has('new:'+a.uid)) continue;
      refs.push({id:f.id, pos:f.translation.slice(), rot:(f.rotation||[0,0,0]).slice(), scale:f.scale==null? 1 : f.scale,
                 key:'new:'+a.uid, from:-1, door:f.destination? {pos:f.destination.translation.slice(), rot:f.destination.rotation.slice(), cell:f.destination.cell||''} : null,
                 gc:false, mark:'', hist:null, moved:0, edited:true, isNew:true});
    }
    return Object.assign({}, rec, {refs});
  },

  /** The pending reference changes, from the pool's list; redraws the render window
   *  when they changed, the camera kept and the edited object still selected. */
  setLive(list){
    const live=new Map(), liveNew=new Map();
    for(const p of list){
      if(!p.ref && !p.new) continue;
      const c={}; for(const ch of p.changes) if('value' in ch) c[ch.path]=ch.value;
      if(p.ref) live.set((p.ref.origin+':'+p.ref.refr).toLowerCase(), c);
      else{
        const k=String(p.new.cell).toLowerCase();
        if(!liveNew.has(k)) liveNew.set(k, []);
        liveNew.get(k).push({uid:p.new.uid, tag:p.new.tag||'', fields:c});
      }
    }
    const byKey=(a,b)=>a[0]<b[0]?-1:a[0]>b[0]?1:0;
    const sig=JSON.stringify([[...live].sort(byKey), [...liveNew].sort(byKey)]);
    if(sig===this._liveSig) return false;
    this.live=live; this.liveNew=liveNew; this._liveSig=sig;
    this.modelsFor(liveNew);
    if(App.R && App.R.cam) App._camAfter=Object.assign({}, App.R.cam);
    if(this.refRec) App._findOri=this.refKeyOf(this.refRec);
    if(typeof schedulePreview==='function' && App.cellSel) schedulePreview();
    return true;
  },
});
