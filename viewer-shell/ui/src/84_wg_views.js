/* =====================================================================================
   Wraithguard: camera bookmarks and screenshots (Static 2026-11-24, "lets do all this":
   "camera bookmarks - save a spot and view per cell and jump back to it" and "screenshot
   to file, with an option to hide the UI and overlays").

   A view is the camera as it stands - the pivot, its angles and distance, and whether it is
   orbiting, flying or walking - kept per cell (`CellKey.fold` of the loaded cell) on this
   machine (`localStorage`, `wgViews`), in world units so it lands in the same place whatever
   the scene's origin is the next time. A screenshot is the 3D view as a PNG where you choose
   (`write_bytes`); the panels are never in it (they are not the canvas), and "clean" also
   leaves out the selection's highlight and the cell-stepping arrows for that one frame.
   Copyright (c) 2026 StaticNation.
   ===================================================================================== */
const WgViews={
  KEY:'wgViews',
  all(){ try{ return JSON.parse(localStorage.getItem(this.KEY)||'{}')||{}; }catch(_){ return {}; } },
  keep(all){ try{ localStorage.setItem(this.KEY, JSON.stringify(all)); }catch(_){ } },
  /** The loaded cell's key, or null with none loaded. */
  cell(){
    const c=App.cellSel; if(!c || typeof CellKey!=='object') return null;
    return CellKey.fold(c.kind==='int'? {kind:'int', name:c.name} : {kind:'ext', x:c.gx!=null? c.gx : c.x, y:c.gy!=null? c.gy : c.y});
  },
  origin(){ return (App._scene && App._scene.origin)||[0,0]; },
  /** This cell's views, oldest first. */
  list(){ const k=this.cell(); return k? (this.all()[k]||[]) : []; },
  /** The camera now, as a view (world units). */
  now(){
    const R=App.R; if(!R || !R.cam) return null;
    const c=R.cam, o=this.origin();
    return {az:c.az, el:c.el, dist:c.dist, x:c.tx+o[0], y:c.ty+o[1], z:c.tz, nav:R.nav||'orbit', walk:!!R.walk};
  },
  save(name){
    const k=this.cell(), v=this.now();
    if(!k || !v){ toast('Load a cell first', 'warn', 2500); return null; }
    const all=this.all(), list=all[k]||(all[k]=[]);
    v.name=String(name||'').trim() || 'View '+(list.length+1);
    list.push(v); this.keep(all); this.draw();
    toast('Saved "'+v.name+'"', 'ok', 2000);
    return v;
  },
  remove(i){ const k=this.cell(); if(!k) return; const all=this.all(); (all[k]||[]).splice(i, 1); if(all[k] && !all[k].length) delete all[k]; this.keep(all); this.draw(); },
  /** Put the camera where view `i` (or a view object) stood. */
  go(i){
    const R=App.R, v=typeof i==='object'? i : this.list()[i];
    if(!R || !v) return false;
    if(R.setNav && v.nav && v.nav!==R.nav) R.setNav(v.nav==='wasd' && v.walk? 'walk' : v.nav);
    const o=this.origin(), c=R.cam;
    c.az=v.az; c.el=v.el; c.dist=v.dist; c.tx=v.x-o[0]; c.ty=v.y-o[1]; c.tz=v.z;
    R.dirty=true;
    return true;
  },
  /** The 3D view as PNG bytes. `clean`: the selection's highlight and the stepping arrows
   *  left out of this one frame. */
  async capture(clean){
    const R=App.R; if(!R || !R.cv) return null;
    const keep={hl:R.hlStatics, arrows:R.opts.arrows};
    if(clean){ R.hlStatics=null; R.opts.arrows=false; }
    try{
      R.draw();
      const blob=await new Promise(res=>R.cv.toBlob(res, 'image/png'));
      return blob? new Uint8Array(await blob.arrayBuffer()) : null;
    }finally{
      if(clean){ R.hlStatics=keep.hl; R.opts.arrows=keep.arrows; R.dirty=true; }
    }
  },
  async shot(clean){
    if(!App.R){ toast('Nothing to capture yet', 'warn', 2500); return; }
    const stamp=new Date().toISOString().slice(0, 19).replace(/[:T]/g, '-');
    let out;
    try{ out=await Engine.pick('save', {defaultPath:'wraithguard-'+stamp+'.png', filters:[{name:'PNG', extensions:['png']}]}); }
    catch(e){ toast(String(e.message||e), 'err', 4000); return; }
    if(!out) return;
    const png=await this.capture(clean);
    if(!png || png.length<30){ toast('The view could not be captured', 'err', 4000); return; }
    try{ await Engine.send('write_bytes', png, {'x-path':String(out)}); toast('Saved '+String(out).split(/[\\/]/).pop(), 'ok', 3000); }
    catch(e){ toast(String(e.message||e), 'err', 5000); }
  },
  /** The panel's part (`#wgViews`), built with the DOM. */
  draw(){
    const box=$('#wgViews'); if(!box) return;
    box.textContent='';
    const mk=(tag, cls, text)=>{ const e=document.createElement(tag); if(cls) e.className=cls; if(text!=null) e.textContent=text; return e; };
    const btn=(text, title, fn)=>{ const b=mk('button', 'btn sm', text); b.title=title; b.onclick=fn; return b; };
    const row=mk('div', 'wgBtns');
    const name=mk('input', 'fld'); name.placeholder='View name'; name.style.cssText='flex:1;min-width:0';
    row.append(name, btn('Save view', 'Keep where the camera stands now, for this cell', ()=>{ this.save(name.value); name.value=''; }));
    box.append(row);
    const list=this.list();
    if(!list.length){ box.append(mk('div', 'hint', this.cell()? 'No views saved for this cell.' : 'Load a cell to save views of it.')); }
    list.forEach((v, i)=>{
      const r=mk('div', 'wgBtns');
      const go=btn(v.name, 'Go to it ('+(v.nav==='wasd'? (v.walk? 'walking' : 'flying') : 'orbit')+')', ()=>this.go(i));
      go.style.flex='1';
      r.append(go, btn('×', 'Forget this view', ()=>this.remove(i)));
      box.append(r);
    });
    const s=mk('div', 'wgBtns');
    const clean=mk('label', 'hint'); const cb=mk('input'); cb.type='checkbox'; cb.checked=true; clean.append(cb, ' without highlights and arrows');
    s.append(btn('Screenshot…', 'Save the 3D view as a PNG (the panels are never in it)', ()=>this.shot(cb.checked)), clean);
    box.append(s);
  }
};
