/* =====================================================================================
   Wraithguard Editor (50_wg_editor.js): the Q menu, its settings and QuickStart, layers, and the selection of several (Ctrl+click).

   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
Object.assign(WgEditor, {
  /* ---- the Q menu's settings and QuickStart ---------------------------------------------- */

  /** The Q menu's choices, kept in this viewer. */
  loadPrefs(){
    if(this._prefsRead) return;
    this._prefsRead=true;
    try{
      const p=JSON.parse(localStorage.getItem('wgEditorPrefs')||'null');
      if(p && typeof p==='object'){
        if(typeof p.snapAxis==='string' && ['+z','-z','+x','-x','+y','-y',''].includes(p.snapAxis)) this.snapAxis=p.snapAxis;
        if(typeof p.rotWorld==='boolean') this.rotWorld=p.rotWorld;
        if(typeof p.gridOn==='boolean') this.gridOn=p.gridOn;
        if(typeof p.angleOn==='boolean') this.angleOn=p.angleOn;
        if(Number.isFinite(p.grid) && p.grid>0) this.grid=p.grid;
        if(Number.isFinite(p.angle) && p.angle>0) this.angle=p.angle;
        if(typeof p.showOri==='boolean') this.showOri=p.showOri;
        if(typeof p.showTfh==='boolean') this.showTfh=p.showTfh;
      }
    }catch(_){ }
  },
  savePrefs(){
    try{ localStorage.setItem('wgEditorPrefs', JSON.stringify({snapAxis:this.snapAxis, rotWorld:this.rotWorld,
      gridOn:this.gridOn, grid:this.grid, angleOn:this.angleOn, angle:this.angle,
      showOri:this.showOri!==false, showTfh:this.showTfh!==false})); }catch(_){ }
  },

  /** The inspector (ORI) and the full help (TFH) shown or not while editing - picking
   *  still selects; only the panels stay out of the way. */
  setPanels(which, on){
    if(which==='ori') this.showOri=on; else if(which==='tfh') this.showTfh=on;
    this.applyPanels(); this.savePrefs();
    if(typeof WgUI==='object') WgUI.refresh();
    toast((which==='ori'? 'Inspector (ORI)' : 'Full help (TFH)')+(on? ' shown' : ' hidden')+' in the Editor','ok',1500);
  },
  applyPanels(){
    const b=document.body;
    b.classList.toggle('edNoOri', this.on && this.showOri===false);
    b.classList.toggle('edNoTfh', this.on && this.showTfh===false);
  },

  /** CSSE's QuickStart: the cell and the view the viewer opens on when it is launched
   *  without one. `{cell, cam}` or null. */
  quickStart(){
    try{
      const q=JSON.parse(localStorage.getItem('wgEditorQuickStart')||'null');
      return (q && q.cell && typeof q.cell==='object')? q : null;
    }catch(_){ return null; }
  },
  setQuickStart(){
    const sel=App.cellSel, R=App.R;
    if(!sel) return toast('Open a cell first','warn',3000);
    const q={cell:{kind:sel.kind, x:sel.x, y:sel.y, name:sel.name, label:sel.label}, cam:R && R.cam? Object.assign({}, R.cam) : null};
    try{ localStorage.setItem('wgEditorQuickStart', JSON.stringify(q)); }catch(_){ return toast('This viewer cannot keep it','err',4000); }
    toast('QuickStart: '+(sel.label||sel.name||(sel.x+','+sel.y))+' opens next time','ok',3000);
  },

  /* ---- layers and the Q menu ----------------------------------------------------------- */

  /** The layers: [{name, visible, keys:[refKey]}], kept in this viewer (localStorage). */
  layers(){
    if(this._layers) return this._layers;
    let L=null;
    try{ L=JSON.parse(localStorage.getItem('wgEditorLayers')||'null'); }catch(_){ }
    this._layers=Array.isArray(L)? L.filter(l=>l && typeof l.name==='string' && Array.isArray(l.keys)) : [];
    return this._layers;
  },
  saveLayers(){
    try{ localStorage.setItem('wgEditorLayers', JSON.stringify(this._layers||[])); }catch(_){ }
  },
  /** The references the layers hide, by key (lower case). */
  hiddenKeys(){
    const out=new Set();
    for(const l of this.layers()) if(!l.visible) for(const k of l.keys) out.add(String(k).toLowerCase());
    // Isolated: everything else of the loaded cells, as it was when isolated.
    if(this._isolated) for(const k of this._isolated.hide) out.add(k);
    return out;
  },
  layerSig(){ return this.layers().filter(l=>!l.visible).map(l=>l.name+':'+l.keys.length).join(',')+(this._isolated? '|iso'+this._isolated.hide.size : ''); },

  /** The layers changed: the panel and the render window follow, the camera kept. */
  layersChanged(){
    this.saveLayers();
    this.applyTints();
    if(this.lay && !this.lay.hidden) this.drawLayers();
    if(App.R && App.R.cam) App._camAfter=Object.assign({}, App.R.cam);
    if(typeof schedulePreview==='function' && App.cellSel) schedulePreview();
  },

  /** CSSE's layer tint: a layer's references drawn again in its colour (the renderer's
   *  `setTintGroups`), re-applied whenever the loaded objects change. */
  hookTints(){
    const R=App.R;
    if(!R || R._wgTints || typeof R.setTintGroups!=='function') return;
    R._wgTints=true;
    const set=R.setPickables.bind(R);
    R.setPickables=list=>{ set(list); if(this.on) this.applyTints(); };
  },
  applyTints(){
    const R=App.R; if(!R || typeof R.setTintGroups!=='function') return;
    const hex=h=>{ const m=/^#?([0-9a-f]{6})$/i.exec(String(h||'')); if(!m) return null; const n=parseInt(m[1],16); return [(n>>16&255)/255, (n>>8&255)/255, (n&255)/255]; };
    const byKey=new Map((R.pickables||[]).map(p=>[String(p.refKey||'').toLowerCase(), p]));
    const groups=[];
    for(const l of this.layers()){
      const col=l.visible && l.tint? hex(l.tint) : null;
      if(!col) continue;
      const items=l.keys.map(k=>byKey.get(String(k).toLowerCase())).filter(Boolean);
      if(items.length) groups.push({col, items});
    }
    R.setTintGroups(this.on? groups : null);
  },

  /** Puts a reference on a layer (made when new), off every other. */
  toLayer(key, name){
    key=String(key).toLowerCase();
    const L=this.layers();
    for(const l of L) l.keys=l.keys.filter(k=>String(k).toLowerCase()!==key);
    let l=L.find(x=>x.name===name);
    if(!l){ l={name, visible:true, keys:[]}; L.push(l); }
    l.keys.push(key);
    this.layersChanged();
    return l;
  },

  showLayers(){
    if(!this.lay){
      const d=document.createElement('div');
      d.id='edLayers'; d.className='ori'; d.hidden=true;
      d.innerHTML='<div class="orihead"><b>Layers</b><span style="flex:1"></span>'+
        '<button class="btn dim ic" id="edLayersX" title="Close">&#x2715;</button></div><div class="oribody" id="edLayersBody"></div>';
      WgUI.mount(d);
      d.querySelector('#edLayersX').onclick=()=>{ d.hidden=true; };
      this.lay=d;
    }
    this.lay.hidden=false;
    this.drawLayers();
  },

  drawLayers(){
    const body=this.lay.querySelector('#edLayersBody'), L=this.layers();
    let h='<div class="hint">Groups of references, shown or hidden in the render window. Q on an object puts it on one; '+
      'Ctrl+Shift+1 to 9 shows or hides the layer of that number; the colour box tints its objects. A view only: nothing here is written to the patch.</div>';
    if(!L.length) h+='<div class="hint">No layers yet.</div>';
    h+='<table class="edT"><tbody>'+L.map((l,i)=>
      '<tr><td class="num" title="Ctrl+Shift+'+(i+1)+'">'+(i<9? i+1 : '')+'</td><td><input type="checkbox" data-lvis="'+i+'"'+(l.visible?' checked':'')+' title="Shown"></td>'+
      '<td><input class="fld" data-lname="'+i+'" value="'+escHtml(l.name)+'" spellcheck="false"></td>'+
      '<td class="num">'+l.keys.length+'</td>'+
      '<td><input type="checkbox" data-ltint="'+i+'"'+(l.tint?' checked':'')+' title="Tint its objects"> <input type="color" data-lcol="'+i+'" value="'+escHtml(l.tint||l.lastTint||'#3fa7ff')+'" title="The tint"></td>'+
      '<td><button class="btn dim ic" data-ldrop="'+i+'" title="Remove the layer (its references are shown again)">&#x2715;</button></td></tr>').join('')+'</tbody></table>'+
      '<div class="orirow"><span class="v"><button class="btn sm" id="edLayerNew">New layer</button> '+
      '<button class="btn sm" id="edLayerAll" title="Every layer shown">Show all</button></span></div>';
    body.innerHTML=h;
    body.querySelectorAll('[data-lvis]').forEach(c=>c.onchange=()=>{ L[+c.dataset.lvis].visible=c.checked; this.layersChanged(); });
    body.querySelectorAll('[data-ltint]').forEach(c=>c.onchange=()=>{
      const l=L[+c.dataset.ltint], col=body.querySelector('[data-lcol="'+c.dataset.ltint+'"]');
      l.tint=c.checked? (col? col.value : '#3fa7ff') : null;
      if(l.tint) l.lastTint=l.tint;
      this.saveLayers(); this.applyTints();
    });
    body.querySelectorAll('[data-lcol]').forEach(c=>c.oninput=()=>{
      const l=L[+c.dataset.lcol]; l.lastTint=c.value;
      if(l.tint){ l.tint=c.value; this.saveLayers(); this.applyTints(); }
    });
    body.querySelectorAll('[data-lname]').forEach(c=>{
      c.onchange=()=>{ const v=c.value.trim(); if(v && !L.some((l,j)=>j!==+c.dataset.lname && l.name===v)){ L[+c.dataset.lname].name=v; this.saveLayers(); } else this.drawLayers(); };
      c.onkeydown=e=>{ if(e.key==='Enter') c.blur(); e.stopPropagation(); };
    });
    body.querySelectorAll('[data-ldrop]').forEach(b=>b.onclick=()=>{ L.splice(+b.dataset.ldrop, 1); this.layersChanged(); });
    body.querySelector('#edLayerNew').onclick=()=>{
      let n=1; while(L.some(l=>l.name==='Layer '+n)) n++;
      L.push({name:'Layer '+n, visible:true, keys:[]}); this.layersChanged();
    };
    body.querySelector('#edLayerAll').onclick=()=>{ L.forEach(l=>{ l.visible=true; }); this.layersChanged(); };
  },

  /** The record a selected reference places: {tag, id, model}. */
  selectedRecord(sel){
    const hit=sel.hit;
    let tag='';
    if(sel.ref.uid){ for(const list of this.liveNew.values()) for(const a of list) if(a.uid===sel.ref.uid) tag=a.tag; }
    else if(this._oriRecord && String(this._oriRecord.id).toLowerCase()===String(hit.id).toLowerCase()) tag=this._oriRecord.tag;
    return {tag, id:hit.id, model:hit.model||CellData.models.get(String(hit.id).toLowerCase())||''};
  },

  /* ---- the selection of several (Ctrl+click), as CSSE's Q menu works on it ------------- */

  /** The Ctrl+click selection still in view, in the order picked. */
  group(){
    const R=App.R; if(!R || !this.on) return [];
    // A redrawn scene has new objects for the same references: follow them by key.
    const byKey=new Map((R.pickables||[]).map(p=>[String(p.refKey||'').toLowerCase(), p]));
    let moved=false;
    for(const g of this._group){
      const now=byKey.get(String(g.hit.refKey||'').toLowerCase());
      if(now && now!==g.hit){ g.hit=now; moved=true; }
    }
    if(moved) this.syncSel();
    return this._group.filter(g=>byKey.has(String(g.hit.refKey||'').toLowerCase()) && g.hit.m && g.hit.wpos);
  },
  clearGroup(){
    this._group=[];
    this.syncSel();
    if(App.R) App.R.setStaticHighlight(this._sel && this._sel.hit? [this._sel.hit] : null);
  },
  /** Ctrl+click: the object in or out of the selection (the one selected before starting it). */
  async toggleGroup(hit){
    if(!this.on || !this.canEdit() || !hit || !hit.refKey) return false;
    const key=String(hit.refKey).toLowerCase();
    if(!this._group.length && this._sel && this._sel.hit && String(this._sel.hit.refKey).toLowerCase()!==key)
      this._group.push({hit:this._sel.hit, ref:this._sel.ref});
    const at=this._group.findIndex(g=>String(g.hit.refKey).toLowerCase()===key);
    if(at>=0) this._group.splice(at, 1);
    else{
      const ref=await this.refOfHit(hit);
      if(!ref){ toast('That object cannot be edited','warn',2500); return true; }
      this._group.push({hit, ref});
    }
    this.syncSel();
    if(App.R) App.R.setStaticHighlight(null);
    toast(this._group.length+' selected - Q for what to do with them, Esc to clear','ok',2000);
    return true;
  },

  /** Q with several selected: CSSE's - align to the last picked, randomize, reset. */
  groupMenu(x, y, group){
    if(!this.qm){
      const m=document.createElement('div');
      m.id='edQ'; m.className='ori'; m.hidden=true;
      document.body.appendChild(m);
      this.qm=m;
    }
    const m=this.qm, last=group[group.length-1];
    const items=[
      ['pos', 'Align position to '+(last.hit.id||'the last picked')],
      ['rot', 'Align rotation to the last picked'],
      ['scl', 'Align scale to the last picked'],
      ['-'],
      ['rrot', 'Randomize rotation (about Z)'],
      ['rscl', 'Randomize scale (±10%)'],
      ['rpos', 'Randomize position (±32 units)'],
      ['-'],
      ['rot0', 'Reset rotation'],
      ['scl1', 'Reset scale'],
      ['drop', 'Drop each to the ground'],
      ['hide', 'Hide (layer "Hidden")'],
      ['clear', 'Clear the selection ('+group.length+')'],
      ['-'],
      ['transform', 'Transform… (move, turn, scale by numbers)', 'Ctrl+T'],
      ['prefab', 'Keep as a prefab…', 'Ctrl+Shift+P'],
    ];
    return this.showMenu(m, x, y, items, a=>this.groupAction(a, group));
  },

  async groupAction(a, group){
    const last=group[group.length-1], rest=group.slice(0, -1);
    const rnd=(lo, hi)=>lo+Math.random()*(hi-lo);
    const each=async(fn)=>{ for(const g of a==='pos'||a==='rot'||a==='scl'? rest : group){ const c=fn(g); if(c && c.length) await this.sendRef(g.ref, c); } };
    if(a==='clear') return this.clearGroup();
    if(a==='transform') return this.showTransform();
    if(a==='prefab') return WgFlow.keepPrefab();
    if(a==='hide'){
      for(const g of group) this.toLayer(this.refKeyOf(g.ref), 'Hidden').visible=false;
      this._group=[]; this._sel=null; this.syncSel(); return this.layersChanged();
    }
    if(a==='drop'){
      for(const g of group){ this._sel={hit:g.hit, ref:g.ref}; this.drop(); }
      return;
    }
    await each(g=>{
      const h=g.hit;
      if(a==='pos') return [['translation', last.hit.wpos.slice()]];
      if(a==='rot') return [['rotation', (last.hit.rot||[0,0,0]).slice()]];
      if(a==='scl') return [['scale', last.hit.scale||1]];
      if(a==='rrot'){ const r=(h.rot||[0,0,0]).slice(); r[2]=rnd(0, Math.PI*2); return [['rotation', r]]; }
      if(a==='rscl') return [['scale', Math.max(0.5, Math.min(2, (h.scale||1)*rnd(0.9, 1.1)))]];
      if(a==='rpos') return [['translation', [h.wpos[0]+rnd(-32,32), h.wpos[1]+rnd(-32,32), h.wpos[2]]]];
      if(a==='rot0') return [['rotation', [0,0,0]]];
      if(a==='scl1') return [['scale', 1]];
      return null;
    });
    toast(group.length+' changed','ok',2000);
  },

  /* ---- selection sets, isolate, hide, frame ------------------------------------------ */

  /** Ctrl+1..9: the selection kept as set N (this session's; by reference, so it
   *  survives the cells being drawn again). */
  keepSet(n){
    const items=this.transformTargets();
    if(!items.length) return toast('Nothing is selected to keep','warn',2000);
    this._sets=this._sets||{};
    this._sets[n]=items.map(g=>({key:String(g.hit.refKey||'').toLowerCase(), ref:g.ref}));
    toast('Kept as set '+n+' ('+items.length+') - '+n+' selects it again','ok',2000);
    return true;
  },
  /** 1..9: set N selected again (those of it the loaded cells have), and framed. */
  recallSet(n){
    const set=(this._sets||{})[n];
    if(!set) return toast('No set '+n+' - Ctrl+'+n+' keeps the selection as it','warn',2000);
    const byKey=new Map(((App.R||{}).pickables||[]).map(p=>[String(p.refKey||'').toLowerCase(), p]));
    const found=set.map(s=>({hit:byKey.get(s.key), ref:s.ref})).filter(g=>g.hit);
    if(!found.length) return toast('Set '+n+' is not in the loaded cells','warn',2500);
    if(found.length===1){ this._group=[]; this._sel=found[0]; }
    else{ this._group=found; this._sel=found[found.length-1]; }
    this.syncSel();
    this.frameSel();
    toast('Set '+n+': '+found.length+(found.length<set.length? ' of '+set.length+' (the rest are not loaded)' : ''),'ok',1800);
    return true;
  },
  /** C: the camera's pivot on the selection - one object, or the middle of several. */
  frameSel(){
    const items=this.transformTargets(), R=App.R;
    if(!items.length || !R || !R.cam) return false;
    const m=[0,0,0];
    for(const g of items){ m[0]+=g.hit.m[3]/items.length; m[1]+=g.hit.m[7]/items.length; m[2]+=g.hit.m[11]/items.length; }
    R.cam.tx=m[0]; R.cam.ty=m[1]; R.cam.tz=m[2];
    R.dirty=true;
    return true;
  },
  /** H: the selection hidden (layer "Hidden", shown again from the Layers panel). */
  hideSel(){
    const items=this.transformTargets();
    if(!items.length) return false;
    for(const g of items) this.toLayer(this.refKeyOf(g.ref), 'Hidden').visible=false;
    this._group=[]; this._sel=null; this.syncSel();
    this.layersChanged();
    toast(items.length+' hidden - the Layers panel shows them again','ok',2000);
    return true;
  },
  /** Shift+H: only the selection shown (of the loaded cells); again, everything back. */
  isolateSel(){
    if(this._isolated){ this._isolated=null; this.layersChanged(); toast('Everything shown again','ok',1500); return true; }
    const items=this.transformTargets();
    if(!items.length) return false;
    const keep=new Set(items.map(g=>String(g.hit.refKey||'').toLowerCase()));
    const hide=new Set(((App.R||{}).pickables||[]).map(p=>String(p.refKey||'').toLowerCase()).filter(k=>k && !keep.has(k)));
    this._isolated={keep, hide};
    this.layersChanged();
    toast('Only the selection shown - Shift+H shows everything again','ok',2500);
    return true;
  },

  /* ---- the transform panel: numbers, for one object or many ------------------------- */

  /** What the transform panel works on: the Ctrl+click selection, or the one selected -
   *  `[{hit, ref}]`. */
  transformTargets(){
    const g=this.group();
    if(g.length>1) return g;
    const sel=this.selected();
    return sel && sel.ref? [{hit:sel.hit, ref:sel.ref}] : [];
  },

  /** Changes to many references, sent as one step (`editRefsSet`) when Wraithguard can:
   *  `[[ref, [[path, value]...]]]`. */
  async sendRefs(list){
    list=list.filter(([, c])=>c && c.length);
    if(!list.length) return;
    if(!this.links().editRefsSet || list.length===1){
      for(const [ref, c] of list) await this.sendRef(ref, c);
      return;
    }
    const changes=[];
    for(const [ref, c] of list) for(const [path, value] of c) changes.push(Object.assign(this.refBody(ref), {path, value}));
    try{ await this.ask('editRefsSet', {changes}); }
    catch(e){ toast(String(e.message||e).replace(/^Wraithguard answered 400:\s*/,''),'err',6000); }
    await this.refreshPending();
  },

  /** The middle of a group (the mean of their positions). */
  middleOf(group){
    const m=[0,0,0];
    for(const g of group) for(let k=0;k<3;k++) m[k]+=g.hit.wpos[k]/group.length;
    return m;
  },

  /** Each one's changes for a move by `d`, a turn by `deg` (world X, Y, Z, in degrees)
   *  about the group's middle, and a scale by `f` about it - what `transformBy` sends. */
  transformPlan(group, d, deg, f){
    const mid=this.middleOf(group);
    const turn=[0,0,0].map((_, k)=>(deg[k]||0)*Math.PI/180);
    const T=EdMath.mul(EdMath.mul(EdMath.mat([0,0,turn[2]]), EdMath.mat([0,turn[1],0])), EdMath.mat([turn[0],0,0]));
    return group.map(g=>{
      const h=g.hit, p=h.wpos;
      const rel=[0,1,2].map(k=>(p[k]-mid[k])*f);
      const moved=[0,1,2].map(k=>mid[k]+T[k][0]*rel[0]+T[k][1]*rel[1]+T[k][2]*rel[2]+(d[k]||0));
      let rot=(h.rot||[0,0,0]).slice();
      for(const k of [0,1,2]) if(turn[k]) rot=EdMath.turnWorld(rot, k, turn[k]);
      const changes=[];
      if(moved.some((v,k)=>Math.abs(v-p[k])>1e-4)) changes.push(['translation', moved]);
      if(turn.some(Boolean)) changes.push(['rotation', rot]);
      if(f!==1) changes.push(['scale', Math.max(0.5, Math.min(2, (h.scale||1)*f))]);
      return [g.ref, changes];
    });
  },
  async transformBy(d, deg, f){
    const group=this.transformTargets();
    if(!group.length) return toast('Select an object (Ctrl+click for several) first','warn',2500);
    await this.sendRefs(this.transformPlan(group, d, deg, f));
    toast(group.length===1? 'Changed' : group.length+' changed together','ok',1800);
  },

  /** The transform panel (Q menu, or Ctrl+T): move, turn about the middle, scale about
   *  it, by numbers; snap to the last one Ctrl+clicked. */
  showTransform(){
    if(!this.trEl){
      const d=document.createElement('div');
      d.id='edTransform'; d.className='ori'; d.hidden=true;
      const row=(k, label, title, step)=>'<div class="orirow" title="'+title+'"><span class="k">'+label+'</span><span class="v">'+
        ['X','Y','Z'].map(a=>'<input class="fld edTrN" data-'+k+'="'+a+'" type="number" step="'+step+'" placeholder="'+a+'" value="0">').join(' ')+'</span></div>';
      d.innerHTML='<div class="orihead"><b>Transform</b><span style="flex:1"></span><span class="from" id="edTrWhat"></span> '+
        '<button class="btn dim ic" id="edTrX" title="Close">&#x2715;</button></div><div class="oribody">'+
        row('mv', 'Move by', 'Units along the world\'s axes', 1)+
        row('tn', 'Turn by', 'Degrees about the world\'s axes, about their middle (one object: about itself)', 1)+
        '<div class="orirow" title="Scale about their middle: the positions spread, each one\'s scale times this (kept within 0.5 - 2)"><span class="k">Scale by</span><span class="v"><input class="fld edTrN" id="edTrS" type="number" step="0.05" min="0.1" value="1"></span></div>'+
        '<div class="orirow"><span class="v"><button class="btn sm pri" id="edTrGo">Apply</button> <button class="btn sm dim" id="edTrZero">Reset</button></span></div>'+
        '<div class="orirow" title="Ctrl+click the objects, the one to snap to last"><span class="k">Snap to the last picked</span><span class="v">'+
          '<button class="btn sm" data-snap="pos">Position</button> <button class="btn sm" data-snap="rot">Rotation</button> <button class="btn sm" data-snap="scl">Scale</button></span></div>'+
        '</div>';
      WgUI.mount(d);
      d.querySelector('#edTrX').onclick=()=>{ d.hidden=true; };
      const nums=k=>['X','Y','Z'].map(a=>+(d.querySelector('[data-'+k+'="'+a+'"]').value)||0);
      d.querySelector('#edTrGo').onclick=()=>this.transformBy(nums('mv'), nums('tn'), +(d.querySelector('#edTrS').value)||1).then(()=>this.trWhat());
      d.querySelector('#edTrZero').onclick=()=>{ d.querySelectorAll('.edTrN').forEach(i=>{ i.value=i.id==='edTrS'? '1' : '0'; }); };
      d.querySelectorAll('[data-snap]').forEach(b=>b.onclick=()=>{
        const g=this.group();
        if(g.length<2) return toast('Ctrl+click the objects to snap, then last the one to snap to','warn',3500);
        this.groupAction(b.dataset.snap, g);
      });
      d.querySelectorAll('input').forEach(i=>i.addEventListener('keydown', e=>{ e.stopPropagation(); if(e.key==='Enter') d.querySelector('#edTrGo').click(); }));
      this.trEl=d;
    }
    this.trEl.hidden=false;
    this.trWhat();
    return this.trEl;
  },
  trWhat(){
    if(!this.trEl) return;
    const n=this.transformTargets().length;
    this.trEl.querySelector('#edTrWhat').textContent=n? (n===1? 'the selected object' : n+' objects') : 'nothing selected';
  },

  /** Q: what can be done to the selected object, at the pointer. */
  quickMenu(x, y){
    const group=this.group();
    if(group.length>1) return this.groupMenu(x, y, group);
    const sel=this.selected(); if(!sel) return false;
    if(!this.qm){
      const m=document.createElement('div');
      m.id='edQ'; m.className='ori'; m.hidden=true;
      document.body.appendChild(m);
      this.qm=m;
    }
    const m=this.qm, rec=this.selectedRecord(sel), L=this.layers();
    // [action, label, key] - or ['>', label, [items]] for a submenu, ['-'] a rule.
    const items=[
      ['ref', 'Edit reference', 'F3'],
      ['transform', 'Transform… (move, turn, scale by numbers)', 'Ctrl+T'],
      ['prefab', 'Keep as a prefab…', 'Ctrl+Shift+P'],
      rec.tag? ['record', 'Edit record', 'F2'] : null,
      rec.tag? ['uses', 'Use Report'] : null,
      ['-'],
      ['drop', 'Drop to ground', 'F'],
      rec.tag? ['dup', 'Duplicate'] : null,
      ['rot0', 'Reset rotation'],
      ['scale1', 'Reset scale'],
      ['del', sel.ref.uid? 'Remove from the patch' : 'Delete reference'],
      ['-'],
      ['hide', 'Hide (layer "Hidden")'],
      ['>', 'Layers', [
        L.some(l=>l.name==='Hidden' && l.keys.length)? ['unhide', 'Restore hidden references'] : null,
        ...L.filter(l=>l.name!=='Hidden').map(l=>['layer:'+l.name, 'Move to layer: '+l.name]),
        ['newlayer', 'Move to a new layer'],
      ].filter(Boolean)],
      ['>', 'Render window', [
        ['snap', 'Alt-drag snap axis: '+(this.snapAxis||'keep the turn')],
        ['rotmode', 'Shift-drag turns on the '+(this.rotWorld? 'world' : 'object\'s own')+' axes'],
        ['land', (App.R && App.R.opts && App.R.opts.ground===false)? 'Show landscape' : 'Hide landscape'],
        ['qs', 'QuickStart: open this cell and view at start'],
        this.quickStart()? ['qsclear', 'QuickStart: clear'] : null,
      ].filter(Boolean)],
    ].filter(Boolean);
    return this.showMenu(m, x, y, items, a=>this.quickAction(a, sel, rec));
  },

  /** The Q menu's body, with its submenus (they open to the side on hover or a click). */
  showMenu(m, x, y, items, act){
    const render=list=>list.map(it=>it[0]==='-'? '<hr class="edQsep">'
      : it[0]==='>'? '<div class="it sub"><span class="lb">'+escHtml(it[1])+'</span><span class="ar">▸</span><div class="edFly edFlySub ori">'+render(it[2])+'</div></div>'
      : '<div class="it" data-q="'+escHtml(it[0])+'"><span class="lb">'+escHtml(it[1])+'</span>'+(it[2]? '<span class="kb">'+escHtml(it[2])+'</span>' : '')+'</div>').join('');
    m.classList.add('edFly');
    m.innerHTML='<div class="oribody">'+render(items)+'</div>';
    m.hidden=false;
    const W=window.innerWidth||1200, H=window.innerHeight||800;
    const h=m.offsetHeight||items.length*24+20, w=m.offsetWidth||240;
    m.style.left=Math.min(Math.max(0,(x==null? W/2 : x)), W-w-4)+'px';
    m.style.top=Math.min(Math.max(0,(y==null? H/2 : y)), H-h-4)+'px';
    m.querySelectorAll('.it.sub').forEach(el=>{
      const sub=el.querySelector('.edFlySub');
      el.onmouseenter=()=>WgUI.fitSub(el, sub);
      el.onclick=e=>{ if(e.target.closest('[data-q]')) return; e.stopPropagation(); el.classList.toggle('open'); WgUI.fitSub(el, sub); };
    });
    m.querySelectorAll('[data-q]').forEach(el=>el.onclick=e=>{ if(e) e.stopPropagation(); m.hidden=true; act(el.dataset.q); });
    return true;
  },

  async quickAction(a, sel, rec){
    const key=this.refKeyOf(sel.ref);
    if(a==='ref') return this.openRef(sel.ref);
    if(a==='transform') return this.showTransform();
    if(a==='prefab') return WgFlow.keepPrefab();
    if(a==='record') return this.openRecord(rec.tag, rec.id, this._oriRecord? this._oriRecord.plugins : null);
    if(a==='uses') return this.showUses(rec.tag, rec.id, null);
    if(a==='drop') return this.drop();
    if(a==='del'){
      if(sel.ref.uid){ this.refRec=Object.assign({}, sel.ref); return this.refChange('editRefRevert', {}); }
      return this.sendRef(sel.ref, [['deleted', true]]);
    }
    if(a==='dup'){
      // Beside it, turned the same way: a new reference of the same record.
      const p=sel.hit.wpos;
      return this.placeAt({tag:rec.tag, id:rec.id, model:rec.model}, null, null, [p[0]+64, p[1], p[2]], (sel.hit.rot||[0,0,0]).slice());
    }
    if(a==='rot0') return this.sendRef(sel.ref, [['rotation', [0,0,0]]]);
    if(a==='scale1') return this.sendRef(sel.ref, [['scale', 1]]);
    if(a==='unhide'){
      const h=this.layers().find(l=>l.name==='Hidden');
      if(h){ h.visible=true; h.keys=[]; }
      return this.layersChanged();
    }
    if(a==='snap'){
      const order=['+z','-z','+x','-x','+y','-y',''];
      this.snapAxis=order[(order.indexOf(this.snapAxis)+1)%order.length];
      this.savePrefs();
      return toast('Alt-drag: '+(this.snapAxis? 'the object\'s '+this.snapAxis+' axis turned out of the surface' : 'onto the surface, the turn kept'),'ok',3000);
    }
    if(a==='rotmode'){
      this.rotWorld=!this.rotWorld; this.savePrefs();
      return toast('Shift-drag turns about the '+(this.rotWorld? 'world\'s' : 'object\'s own')+' axes','ok',3000);
    }
    if(a==='land'){
      const R=App.R; if(!R || !R.opts) return;
      R.opts.ground=R.opts.ground===false; R.dirty=true;
      return;
    }
    if(a==='qs') return this.setQuickStart();
    if(a==='qsclear'){ try{ localStorage.removeItem('wgEditorQuickStart'); }catch(_){ } return toast('QuickStart cleared','ok',2500); }
    if(a==='hide'){ this.toLayer(key, 'Hidden').visible=false; this._sel=null; this.syncSel(); return this.layersChanged(); }
    if(a.startsWith('layer:')) return this.toLayer(key, a.slice(6));
    if(a==='newlayer'){
      let n=1; while(this.layers().some(l=>l.name==='Layer '+n)) n++;
      this.toLayer(key, 'Layer '+n);
      return this.showLayers();
    }
  },
});
