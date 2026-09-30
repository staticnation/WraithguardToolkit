
/* =====================================================================================
   Wraithguard: the Preview's review, landscape, overlay and link tools.

   Review a mod    - the loaded cells with a mod's part marked (what it added, changed),
                     or with the mod left out of the load order (what it changed, back as
                     it was; what it deleted or moved away, back where it stood) - the
                     engine replays the order (`cell_data` / `interior_data` `review`,
                     `without`; viewcore review.rs). And checks over what is on screen:
                     objects floating over or buried in the ground, duplicates, references
                     the load order moved, and files the setup is missing.
   Landscape       - the ground as the load order has it, or as Merged Lands would build
                     it under each strategy (`cell_data` `land`; viewcore mland.rs), with
                     the vertices two plugins both moved marked.
   Overlays        - path grids, light radii, collision, door links, NPC reach (activation
                     distance, and the greeting distance from the NPC's AI Hello), and a
                     measuring tape (47_wg_overlay.js draws them).
   Links & export  - the spot as an OpenMW console line, a report bundle (a screenshot and
                     what is known about the place), where a mesh, texture or record is
                     used, and a plugin's cells on the map.

   The Preview's groups are accordions (`.sect.sub.pvacc` in 02_body.html); which are open
   is remembered here.
   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
const WgTools={
  review:'', without:false, land:'',
  checks:[], checkKind:'', ovl:{pathgrid:false, lights:false, collision:false, doors:false, reach:false, markers:false},
  measure:{on:false, a:null, b:null}, _gs:null, _gsSig:null,

  COL:{added:[0.35,0.95,0.45], changed:[1,0.82,0.2], reverted:[0.4,0.72,1], restored:[1,0.35,0.3]},

  /** What `CellData.loadCell` sends with a cell, and the cache key's part for it. */
  cellArgs(){
    const sig=[this.review? 'r:'+this.review.toLowerCase()+(this.without? ':w' : '') : '', this.land? 'l:'+this.land : '']
      .filter(Boolean).join('|');
    return {review:this.review, without:this.without, land:this.land, sig};
  },

  sc(){ return App._scene||null; },
  origin(){ const s=this.sc(); return (s && s.origin)? s.origin : [0,0]; },
  picks(){ return (App.R && App.R.pickables)||[]; },
  centre(p){ const a=p.aabb; return a? [(a.x0+a.x1)/2,(a.y0+a.y1)/2,(a.z0+a.z1)/2] : [p.m[3],p.m[7],p.m[11]]; },
  isActor(p){ return typeof CellData==='object' && CellData.actors && CellData.actors.has(String(p.id||'').toLowerCase()); },

  /** Rebuilds the scene with the cells asked for again (the cache keeps each way apart). */
  reload(){
    App._scene=null;
    if(typeof schedulePreview==='function') schedulePreview();
  },

  /* ---- start-up ------------------------------------------------------------------ */
  bind(){
    this.accordions();
    this.buildPanes();
    const R=App.R; if(!R) return;
    // Every new scene: the review's marks, the checks and the overlays follow it.
    const set=R.setPickables.bind(R);
    R.setPickables=list=>{ set(list); try{ this.afterScene(); }catch(e){ console.error(e); } };
    // The measuring tape takes the click while it is on.
    const pick=R.onPick;
    R.onPick=(hit,e)=>{ if(this.measure.on && e) return this.measureClick(e); if(pick) pick(hit,e); };
    document.addEventListener('keydown',e=>{ if(e.key==='Escape' && this.measure.on) this.setMeasure(false); });
    // A tape button beside the viewport's own.
    const tools=$('#vptools');
    if(tools && !$('#tbMeasure')){
      const b=document.createElement('button');
      b.className='vpbtn'; b.id='tbMeasure'; b.textContent='📏 Measure';
      b.title='Measure: click two points in the view for the distance between them (Esc to stop)';
      b.onclick=()=>this.setMeasure(!this.measure.on);
      const map=$('#tbMap'); tools.insertBefore(b, map? map.nextSibling : null);
    }
  },

  /** The Preview's groups, open or closed as last left. */
  accordions(){
    let st={};
    try{ st=JSON.parse(localStorage.getItem('wg.pvAcc')||'{}')||{}; }catch(_){ st={}; }
    $$('#secCell .sect.pvacc').forEach(sec=>{
      if(st[sec.id]!=null) sec.classList.toggle('closed', !!st[sec.id]);
      const h=sec.querySelector(':scope > .sh');
      if(h) h.addEventListener('click',()=>{
        st[sec.id]=sec.classList.contains('closed');
        try{ localStorage.setItem('wg.pvAcc', JSON.stringify(st)); }catch(_){ }
      });
    });
  },

  row(label, ctl, title){
    return '<div class="row"><label'+(title? ' title="'+escHtml(title)+'"' : '')+'>'+label+'</label><div class="ctl">'+ctl+'</div></div>';
  },
  sw(id, title){ return '<label class="sw"'+(title? ' title="'+escHtml(title)+'"' : '')+'><input type="checkbox" id="'+id+'"><span class="tr"></span></label>'; },

  buildPanes(){
    const rv=$('#wgReviewPane');
    if(rv){
      rv.innerHTML=
        this.row('Review mod','<select class="fld" id="wgRvMod"><option value="">None</option></select>',
          'Mark what this plugin does to the loaded cells: what it placed (green) and what it changed (yellow). "Without it" shows the cells with the plugin left out of the load order: its changes undone (blue) and what it deleted or moved away back where it stood (red).')+
        '<div class="wgBtns"><button class="btn sm pri" id="wgRvWith">With it</button><button class="btn sm" id="wgRvWithout">Without it</button></div>'+
        '<div class="wgStats" id="wgRvStats"></div>'+
        '<div class="wgBtns" title="Checks over the loaded cells">'+
          '<button class="btn sm" data-chk="float" title="Objects standing clear of the ground or sunk into it - most often a landscape mod that moved the ground under another mod\'s objects">Floating / buried</button>'+
          '<button class="btn sm" data-chk="dupes" title="The same object placed twice in the same spot">Duplicates</button>'+
          '<button class="btn sm" data-chk="moved" title="References a later plugin moved from where their first plugin put them">Moved</button>'+
          '<button class="btn sm" data-chk="missing" title="Meshes and textures the loaded cells ask for that the setup does not have">Missing files</button>'+
          '<button class="btn sm" data-chk="clear" title="Clear the check">✕</button></div>'+
        '<div class="wgStats" id="wgChkStats"></div><div class="wgList" id="wgChkList" hidden></div>';
      $('#wgRvMod').onchange=e=>{ this.review=e.target.value; this.without=false; this.syncReviewBtns(); this.reload(); };
      $('#wgRvWith').onclick=()=>{ if(!this.without) return; this.without=false; this.syncReviewBtns(); this.reload(); };
      $('#wgRvWithout').onclick=()=>{ if(this.without || !this.review) return; this.without=true; this.syncReviewBtns(); this.reload(); };
      rv.querySelectorAll('[data-chk]').forEach(b=>b.onclick=()=>this.runCheck(b.dataset.chk));
    }
    const ld=$('#wgLandPane');
    if(ld){
      ld.innerHTML=
        this.row('Ground','<select class="fld" id="wgLand">'+
          '<option value="">Load order (last plugin wins)</option>'+
          '<option value="overwrite">Merged Lands: Overwrite</option>'+
          '<option value="resolve">Merged Lands: Resolve</option>'+
          '<option value="ignore">Merged Lands: Ignore</option>'+
          '<option value="curvature">Merged Lands: Curvature</option></select>',
          'Where two or more plugins edit a cell\'s ground, show it as Merged Lands would build it: each plugin\'s edit kept where only it moved the ground, and a vertex two plugins both moved settled by the strategy - Overwrite (the later plugin), Ignore (the earlier), Resolve (the two blended towards the larger), Curvature (weighted by the shape each adds). Textures are never blended. Seam repair across cells is not previewed.')+
        this.row('Mark contested',this.sw('wgLandMarks','Mark the vertices two or more plugins both moved (red where the blend sits far from an edit)'))+
        '<div class="wgStats" id="wgLandStats"></div>';
      $('#wgLand').onchange=e=>{ this.land=e.target.value; this.reload(); };
      $('#wgLandMarks').onchange=()=>this.landMarks();
    }
    const ov=$('#wgOverlayPane');
    if(ov){
      ov.innerHTML=
        this.row('Path grids',this.sw('wgOvPathgrid','The cells\' path grids (PGRD): the points NPCs and creatures walk between, and the links between them'))+
        this.row('Light radii',this.sw('wgOvLights','Each lit lamp\'s radius, in its colour'))+
        this.row('Collision',this.sw('wgOvCollision','The collision shape of every object in the loaded cells (its hull, or the drawn mesh where it has none)'))+
        this.row('Door links',this.sw('wgOvDoors','A line from each door to where it leads, when that is in view; a post over doors that lead elsewhere'))+
        this.row('Editor markers',this.sw('wgOvMarkers','The Construction Set\'s markers the game does not draw - door and travel landing spots, north markers and the rest - as outlines (door and travel markers in R-Zero\'s replacements)'))+
        this.row('NPC reach',this.sw('wgOvReach','Round each NPC and creature: how close you must be to talk to it or activate it (iMaxActivateDist), and - larger - where it greets you (its AI Hello × iGreetDistanceMultiplier)'))+
        '<div class="wgBtns"><button class="btn sm" id="wgOvMeasure">📏 Measure</button></div>'+
        '<div class="hint" style="margin-top:4px">Overlays show through walls, fainter.</div>';
      for(const k of Object.keys(this.ovl)){
        const id='wgOv'+k[0].toUpperCase()+k.slice(1);
        const el=$('#'+id); if(el) el.onchange=()=>{ this.ovl[k]=el.checked; this.overlay(k); };
      }
      $('#wgOvMeasure').onclick=()=>this.setMeasure(!this.measure.on);
    }
    const lk=$('#wgLinksPane');
    if(lk){
      lk.innerHTML=
        '<div class="wgBtns">'+
          '<button class="btn sm" id="wgCopySpot" title="Copy OpenMW console lines that put you here, facing the way the camera faces - paste them in the game\'s console (the ` key)">Copy spot for OpenMW</button>'+
          '<button class="btn sm" id="wgReport" title="Save a report for a mod author: a screenshot, and the cell, coordinates, the object you inspected and every plugin that touches the place">Save report…</button></div>'+
        this.row('Where used','<div style="display:flex;gap:4px;width:100%"><select class="fld" id="wgUseKind" style="width:auto"><option value="mesh">Mesh</option><option value="texture">Texture</option><option value="id">Record id</option></select>'+
          '<input class="fld" id="wgUseName" placeholder="x\\ex_rock_01.nif" style="flex:1;min-width:0"></div>','Every cell and room where a mesh, a texture (on objects and on the ground) or a base record is used')+
        '<div class="wgBtns"><button class="btn sm" id="wgUseGo">Find</button></div>'+
        '<div class="wgStats" id="wgUseStats"></div><div class="wgList" id="wgUseList" hidden></div>'+
        this.row('Plugin\'s cells','<select class="fld" id="wgPlugCells"></select>','Show on the cell map every cell this plugin touches')+
        '<div class="wgBtns"><button class="btn sm" id="wgPlugGo">Show on map</button></div>';
      $('#wgCopySpot').onclick=()=>this.copySpot();
      $('#wgReport').onclick=()=>this.saveReport();
      $('#wgUseGo').onclick=()=>this.whereUsed($('#wgUseKind').value, $('#wgUseName').value);
      $('#wgUseName').onkeydown=e=>{ if(e.key==='Enter') this.whereUsed($('#wgUseKind').value, $('#wgUseName').value); };
      $('#wgPlugGo').onclick=()=>{ const v=$('#wgPlugCells').value; if(v && typeof WgCoverage==='object') WgCoverage.showPlugin(v); };
    }
  },

  syncReviewBtns(){
    const w=$('#wgRvWith'), wo=$('#wgRvWithout');
    if(w) w.classList.toggle('pri',!this.without);
    if(wo){ wo.classList.toggle('pri',this.without); wo.disabled=!this.review; }
  },

  /* ---- after every scene ------------------------------------------------------------ */
  afterScene(){
    this.fillPluginLists();
    this.reviewMarks();
    this.landInfo();
    this.landMarks();
    for(const k of Object.keys(this.ovl)) if(this.ovl[k]) this.overlay(k);
    if(this.checkKind && this.checkKind!=='clear') this.runCheck(this.checkKind, true);
    if(this.measure.a) this.measureDraw();
  },

  /** The plugins touching the loaded cells, in load order - for Review mod - and every
      plugin, for the map's plugin list. */
  fillPluginLists(){
    const s=this.sc();
    const names=new Set();
    if(s && typeof WgCoverage==='object'){
      for(const c of (s.cells||[])){
        const m=c.kind==='int'? WgCoverage.mods({kind:'int',name:c.name}) : WgCoverage.mods({kind:'ext',x:c.gx,y:c.gy});
        m.forEach(n=>names.add(n));
        (c.landEditors||[]).forEach(n=>names.add(n));
      }
    }
    for(const p of this.picks()) if(p.plugin) names.add(p.plugin);
    const order=(typeof GameData==='object' && GameData.plugins)? GameData.plugins.map(p=>String(p.name||p)) : [];
    const ix=n=>{ const i=order.findIndex(o=>o.toLowerCase()===n.toLowerCase()); return i<0? 1e9 : i; };
    const list=[...names].sort((a,b)=>ix(a)-ix(b));
    const sel=$('#wgRvMod');
    if(sel){
      if(this.review && !list.some(n=>n.toLowerCase()===this.review.toLowerCase())) list.push(this.review);
      sel.innerHTML='<option value="">None</option>'+list.map(n=>'<option>'+escHtml(n)+'</option>').join('');
      sel.value=this.review;
    }
    const pc=$('#wgPlugCells');
    if(pc){
      const all=(typeof WgCoverage==='object' && WgCoverage.plugins.length)? WgCoverage.plugins : order;
      if(pc.options.length!==all.length){ const was=pc.value; pc.innerHTML=all.map(n=>'<option>'+escHtml(n)+'</option>').join(''); if(was) pc.value=was; }
    }
    this.syncReviewBtns();
  },

  /* ---- review a mod ------------------------------------------------------------------- */
  reviewMarks(){
    const R=App.R; if(!R) return;
    const st=$('#wgRvStats');
    if(!this.review){
      R.setOverlay('review',null);
      if(st) st.textContent='';
      if(this._hl){ this._hl=false; R.setStaticHighlight(null); if(typeof WgModHl==='object' && WgModHl.active()) WgModHl.apply(); }
      return;
    }
    const n={added:0, changed:0, reverted:0, restored:0};
    const L=new OvlLines(), marked=[];
    for(const p of this.picks()){
      if(!p.mark || !this.COL[p.mark]) continue;
      n[p.mark]++; marked.push(p);
      if(p.aabb) L.box(p.aabb, this.COL[p.mark]);
    }
    R.setOverlay('review', L, {xray:true});
    if(marked.length){
      if(typeof WgModHl==='object' && WgModHl.active()) WgModHl.choose('');
      R.hlMode='outline'; R.setStaticHighlight(marked); this._hl=true;
    }
    const key=(k,t)=>'<span class="wgKey" style="background:rgb('+this.COL[k].map(v=>Math.round(v*255)).join(',')+')"></span>'+t;
    if(st){
      st.innerHTML=this.without
        ? escHtml(this.review)+' left out: '+key('reverted',n.reverted+' changed back')+' · '+key('restored',n.restored+' restored')
        : escHtml(this.review)+': '+key('added',n.added+' placed')+' · '+key('changed',n.changed+' changed')+
          '<br><span style="color:var(--tx3)">Without it shows what it deleted or moved away.</span>';
    }
  },

  /* ---- checks ----------------------------------------------------------------------- */
  runCheck(kind, quiet){
    const R=App.R; if(!R) return;
    this.checkKind=kind;
    const list=$('#wgChkList'), st=$('#wgChkStats');
    if(kind==='clear'){ this.checks=[]; R.setOverlay('checks',null); if(list){ list.hidden=true; list.innerHTML=''; } if(st) st.textContent=''; this.checkKind=''; return; }
    let rows=[];
    if(kind==='float') rows=this.floating();
    else if(kind==='dupes') rows=this.duplicates();
    else if(kind==='moved') rows=this.movedRefs();
    else if(kind==='missing') rows=this.missing();
    this.checks=rows;
    const L=new OvlLines();
    for(const r of rows) if(r.p && r.p.aabb) L.box(r.p.aabb, r.col||[1,0.5,0.1]);
    R.setOverlay('checks', L, {xray:true});
    const what={float:'objects floating or buried', dupes:'duplicated objects', moved:'moved references', missing:'missing files'}[kind];
    if(st) st.textContent=rows.length? rows.length+' '+what+(rows.length>300? ' (first 300 listed)' : '') : 'No '+what+' in the loaded cells.';
    if(list){
      list.hidden=!rows.length;
      list.innerHTML=rows.slice(0,300).map((r,i)=>'<div class="it" data-i="'+i+'" title="'+escHtml(r.title||'')+'"><span class="k">'+escHtml(r.label)+'</span><span class="n">'+escHtml(r.note||'')+'</span></div>').join('');
      list.querySelectorAll('.it').forEach(el=>el.onclick=()=>{ const r=rows[+el.dataset.i]; if(r && r.p) this.focus(r.p, true); else if(r && r.act) r.act(); });
    }
    if(!quiet && !rows.length) toast(st? st.textContent : '','ok',2500);
  },

  /** Objects standing clear of the ground under them, or sunk wholly into it. */
  floating(){
    if(typeof sceneGroundZ!=='function') return [];
    const out=[];
    const s=this.sc();
    const landBy=new Map();
    for(const c of ((s&&s.cells)||[])) if(c.kind!=='int') landBy.set(c.gx+','+c.gy, c.landEditors||[]);
    const order=(typeof GameData==='object' && GameData.plugins)? GameData.plugins.map(p=>String(p.name||p).toLowerCase()) : [];
    for(const p of this.picks()){
      const a=p.aabb; if(!a || this.isActor(p) || p.light) continue;
      const w=a.x1-a.x0, d=a.y1-a.y0;
      if(w>3000 || d>3000 || (a.z1-a.z0)<4) continue;
      const pts=[[a.x0,a.y0],[a.x1,a.y0],[a.x0,a.y1],[a.x1,a.y1],[(a.x0+a.x1)/2,(a.y0+a.y1)/2]];
      const gs=pts.map(([x,y])=>sceneGroundZ(x,y));
      if(gs.some(g=>g==null)) continue;
      const lo=Math.min(...gs), hi=Math.max(...gs);
      let kind=null, gap=0;
      if(a.z0-hi>24){ kind='floats'; gap=a.z0-hi; }
      else if(a.z1<lo-8){ kind='buried'; gap=lo-a.z1; }
      if(!kind) continue;
      // Whose ground: a plugin later than the object's own that edits it is the likely cause.
      const ed=landBy.get(p.cellKey)||[];
      const last=ed.length? ed[ed.length-1] : '';
      const later=last && order.indexOf(last.toLowerCase())>order.indexOf(String(p.plugin||'').toLowerCase());
      out.push({p, label:p.id+'  ('+(p.plugin||'?')+')', note:kind+' '+Math.round(gap)+'u'+(later? ' · ground: '+last : ''),
                title:kind==='floats'? 'Stands '+Math.round(gap)+' units above the ground' : 'Sunk '+Math.round(gap)+' units below the ground',
                col: kind==='floats'? [1,0.55,0.1] : [0.75,0.35,1], score:(later? 1e6 : 0)+gap});
    }
    return out.sort((a,b)=>b.score-a.score);
  },

  /** The same object placed twice in one spot. */
  duplicates(){
    const by=new Map();
    for(const p of this.picks()){
      if(!p.wpos) continue;
      const k=String(p.id||'').toLowerCase()+'@'+p.wpos.map(v=>Math.round(v/2)).join(',')+'@'+(+p.scale||1).toFixed(2);
      let g=by.get(k); if(!g){ g=[]; by.set(k,g); } g.push(p);
    }
    const out=[];
    for(const g of by.values()){
      if(g.length<2) continue;
      const plugins=[...new Set(g.map(p=>p.plugin||'?'))];
      for(const p of g) out.push({p, label:p.id+'  ×'+g.length, note:plugins.join(', '), col:[1,0.2,0.6],
                                   title:'Placed '+g.length+' times here, by '+plugins.join(', ')});
    }
    return out;
  },

  /** References a later plugin moved from where their first plugin put them. */
  movedRefs(){
    const out=[];
    for(const p of this.picks()){
      if(!(p.moved>1)) continue;
      const names=(p.hist||[]).map(i=>pluginName(i)||'?');
      out.push({p, label:p.id, note:Math.round(p.moved)+'u · '+(names[names.length-1]||''), col:[0.3,0.85,1],
                title:'Moved '+Math.round(p.moved)+' units. Versions from: '+names.join(' → '), score:p.moved});
    }
    return out.sort((a,b)=>b.score-a.score);
  },

  /** What the loaded cells ask for that the setup does not have. */
  missing(){
    return (App._sceneMissing||[]).map(m=>({label:(m.kind==='mesh'? 'Mesh ' : 'Texture ')+m.name,
      note:m.count+'×', title:(m.mesh? 'On '+m.mesh+'. ' : '')+(m.err||''),
      act:()=>{ const k=$('#wgUseKind'), n=$('#wgUseName'); if(k&&n){ k.value=m.kind==='mesh'? 'mesh' : 'texture'; n.value=m.name; this.whereUsed(k.value,n.value); } }}));
  },

  /** Frames the camera on a pickable, and inspects it. */
  focus(p, inspect){
    const R=App.R; if(!R||!p) return;
    const c=this.centre(p), a=p.aabb;
    const size=a? Math.max(a.x1-a.x0,a.y1-a.y0,a.z1-a.z0) : 200;
    R.cam.tx=c[0]; R.cam.ty=c[1]; R.cam.tz=c[2];
    R.cam.dist=Math.max(400, size*2.5); R.cam.vs=1; R.dirty=true;
    if(inspect && typeof Ori==='object') Ori.show(p);
  },

  /* ---- landscape ---------------------------------------------------------------------- */
  landInfo(){
    const st=$('#wgLandStats'); if(!st) return;
    const s=this.sc(); if(!s){ st.textContent=''; return; }
    const centre=(s.cells||[])[0];
    if(!centre || centre.kind==='int'){ st.textContent='A room has no landscape.'; return; }
    const ed=centre.landEditors||[];
    let h='<b>'+escHtml(centre.name||'')+' ('+centre.gx+', '+centre.gy+')</b> ground: '+
      (ed.length? ed.map(escHtml).join(' → ') : 'no landscape');
    if(this.land){
      let merged=0, contested=0, major=0;
      for(const c of (s.cells||[])){ const m=c.landMerged; if(m){ merged++; contested+=m.contested.length; major+=m.major.length; } }
      h+='<br>'+merged+' of the loaded cells have two or more editors'+(merged? ': '+contested+' vertices contested'+(major? ', '+major+' far from an edit' : '') : '')+'.';
    } else if(ed.length>1){
      h+='<br><span style="color:var(--tx3)">The load order keeps '+escHtml(ed[ed.length-1])+'\'s ground whole.</span>';
    }
    st.innerHTML=h;
  },
  landMarks(){
    const R=App.R; if(!R) return;
    const on=$('#wgLandMarks') && $('#wgLandMarks').checked;
    const s=this.sc();
    if(!on || !this.land || !s){ R.setOverlay('land',null); return; }
    const L=new OvlLines(), o=this.origin(), N=65, step=8192/64;
    for(const c of (s.cells||[])){
      const m=c.landMerged; if(!m || !c.heights) continue;
      const major=new Set(m.major);
      for(const v of m.contested){
        const vx=v%N, vy=(v/N)|0;
        const p=[c.gx*8192+vx*step-o[0], c.gy*8192+vy*step-o[1], c.heights[v]+6];
        L.cross(p, 24, major.has(v)? [1,0.2,0.15] : [1,0.85,0.2]);
      }
    }
    R.setOverlay('land', L, {xray:true});
  },

  /* ---- overlays ------------------------------------------------------------------------ */
  async gameSettings(){
    const sig=(typeof GameData==='object')? GameData.sig : null;
    if(this._gs && this._gsSig===sig) return this._gs;
    try{ this._gs=await Engine.call('game_settings'); }catch(_){ this._gs={activate:192, greetMult:6}; }
    this._gsSig=sig;
    return this._gs;
  },
  /** Adds a mesh's triangle edges to `L`, placed by `M` (a 3x4, translation at 3, 7, 11):
      a built-in marker (`__wg/…`, viewcore markers.rs) or a load-order mesh. */
  async markerLines(L, path, M, col){
    let info=null;
    try{ info=await loadMesh(path); }catch(_){ info=null; }
    if(!info || info.err) return false;
    const X=(x,y,z)=>[M[0]*x+M[1]*y+M[2]*z+M[3], M[4]*x+M[5]*y+M[6]*z+M[7], M[8]*x+M[9]*y+M[10]*z+M[11]];
    for(const p of (info.parts||[])){
      const P=p.pos, I=p.idx; if(!P||!I) continue;
      const v=k=>X(P[k*3],P[k*3+1],P[k*3+2]);
      for(let i=0;i+2<I.length;i+=3){ const a=v(I[i]), b=v(I[i+1]), c=v(I[i+2]); L.seg(a,b,col); L.seg(b,c,col); L.seg(c,a,col); }
    }
    return true;
  },
  /** Which file draws an editor marker: R-Zero's for door and travel markers, the load
      order's own for the rest. */
  markerPath(model){
    const base=String(model||'').split(/[\\/]/).pop().toLowerCase();
    if(base==='marker_arrow.nif' || base==='marker_travel.nif' || base==='marker_creature.nif') return '__wg/'+base;
    return /^meshes[\\/]/i.test(model)? model : 'Meshes\\'+model;
  },
  /** A matrix standing at `a` with its +Y towards `b`, level. */
  facing(a,b){
    const dx=b[0]-a[0], dy=b[1]-a[1], h=Math.hypot(dx,dy)||1, ux=dx/h, uy=dy/h;
    return [uy,ux,0,a[0], -ux,uy,0,a[1], 0,0,1,a[2]];
  },
  specs(){
    const s=this.sc();
    return ((s&&s.cells)||[]).map(c=>c.kind==='int'? 'int:'+c.name : c.gx+','+c.gy);
  },
  async overlay(k){
    const R=App.R; if(!R) return;
    if(!this.ovl[k]){ R.setOverlay(k,null); return; }
    const o=this.origin();
    const L=new OvlLines();
    if(k==='pathgrid'){
      let r=[];
      try{ r=await Engine.call('pathgrid',{cells:this.specs()}); }catch(e){ toast(String(e.message||e),'err',4000); }
      for(const g of r){
        const P=g.points.map(p=>[p[0]-o[0], p[1]-o[1], p[2]+12]);
        for(const [a,b] of g.edges) if(P[a]&&P[b]) L.seg(P[a],P[b],[0.3,0.9,1]);
        for(const p of P) L.cross(p,14,[1,0.9,0.3]);
      }
      if(!r.length) toast('No path grid in the loaded cells','ok',2500);
      R.setOverlay(k,L,{xray:true});
    }else if(k==='lights'){
      for(const li of (R.lights||[])){
        const c=li.c||[1,1,1], mx=Math.max(c[0],c[1],c[2],0.001);
        const col=[c[0]/mx, c[1]/mx, c[2]/mx, 0.9];
        L.circle(li.p, li.r, col, 'z', 64);
        L.circle(li.p, li.r, col, 'x', 48);
        L.cross(li.p, 12, col);
      }
      R.setOverlay(k,L,{xray:false});
    }else if(k==='doors'){
      const s=this.sc(), here=s&&s.cells&&s.cells[0];
      const room=here && here.kind==='int'? String(here.name||'').toLowerCase() : null;
      for(const p of this.picks()){
        const d=p.door; if(!d || !d.pos) continue;
        const from=this.centre(p);
        const inView = room==null? !d.cell : String(d.cell||'').toLowerCase()===room;
        if(inView){
          const to=[d.pos[0]-o[0], d.pos[1]-o[1], d.pos[2]+40];
          L.seg(from,to,[1,0.35,0.95]);
          // Where you arrive, and facing which way: the door marker (R-Zero's).
          const land=[d.pos[0]-o[0], d.pos[1]-o[1], d.pos[2]];
          if(!await this.markerLines(L,'__wg/marker_arrow.nif', refMatrix(land, d.rot||[0,0,0], 1), [1,0.35,0.95]))
            { L.circle(to,48,[1,0.35,0.95],'z',32); L.cross(to,20,[1,0.35,0.95]); }
        }else{
          const top=[from[0],from[1],from[2]+256];
          L.seg(from,top,[1,0.6,0.2]); L.circle(top,32,[1,0.6,0.2],'z',24);
        }
      }
      R.setOverlay(k,L,{xray:true});
    }else if(k==='reach'){
      const gs=await this.gameSettings();
      for(const p of this.picks()){
        const a=typeof CellData==='object'? CellData.actors.get(String(p.id||'').toLowerCase()) : null;
        if(!a || a.kind==='lev') continue;
        const c=[p.m[3],p.m[7],p.m[11]+8];
        L.circle(c, +gs.activate||192, [0.95,0.95,0.95,0.85], 'z', 48);
        const h=+a.hello||0;
        if(h>0) L.circle(c, h*(+gs.greetMult||6), [0.35,1,0.45,0.85], 'z', 64);
      }
      R.setOverlay(k,L,{xray:true});
    }else if(k==='markers'){
      for(const m of (App._sceneMarkers||[])){
        const base=String(m.model).split(/[\\/]/).pop().toLowerCase();
        const col= base==='marker_arrow.nif'? [1,0.35,0.95] : base==='marker_travel.nif'? [0.3,0.9,1] : [0.85,0.85,0.85];
        await this.markerLines(L, this.markerPath(m.model), refMatrix(m.local, m.rot||[0,0,0], m.scale||1), col);
      }
      if(!(App._sceneMarkers||[]).length) toast('No editor markers in the loaded cells','ok',2500);
      R.setOverlay(k,L,{xray:true});
    }else if(k==='collision'){
      await this.collision(L);
      R.setOverlay(k,L,{xray:false});
    }
  },
  /** Every object's collision, fetched in one bundle and placed by its instance matrix. */
  async collision(L){
    const picks=this.picks().filter(p=>p.model && !String(p.model).startsWith('__npc/') && p.model!==(typeof CORPSE_SLAB==='string'? CORPSE_SLAB : ''));
    const models=[...new Set(picks.map(p=>p.model))];
    if(!models.length) return;
    let buf;
    try{ buf=await Engine.bytes('collision_bundle',{meshes:models.map(m=>/^meshes[\\/]/i.test(m)? m : 'meshes\\'+m)}); }
    catch(e){ toast(String(e.message||e),'err',4000); return; }
    const dv=new DataView(buf), dec=new TextDecoder();
    const tris=new Map();
    let o=8; const n=dv.getUint32(4,true);
    for(let i=0;i<n;i++){
      const pl=dv.getUint16(o,true); o+=2; o+=pl;
      const bl=dv.getUint32(o,true); o+=4;
      const at=o; o+=bl;
      if(bl<9) { tris.set(models[i],null); continue; }
      const shape=dv.getUint8(at+4), nt=dv.getUint32(at+5,true);
      tris.set(models[i], {shape, f:new Float32Array(buf.slice(at+9, at+9+nt*36))});
    }
    // Nearest the view first, and a ceiling, so a 49-cell load stays drawable.
    const c=App.R.cam, MAX=900000;
    picks.sort((a,b)=>{ const ca=this.centre(a), cb=this.centre(b);
      return Math.hypot(ca[0]-c.tx,ca[1]-c.ty)-Math.hypot(cb[0]-c.tx,cb[1]-c.ty); });
    let segs=0, capped=false;
    for(const p of picks){
      const t=tris.get(p.model); if(!t || !t.f.length) continue;
      const m=p.m, col=t.shape===0? [0.2,1,0.4] : [1,0.8,0.2];
      const X=(x,y,z)=>[m[0]*x+m[1]*y+m[2]*z+m[3], m[4]*x+m[5]*y+m[6]*z+m[7], m[8]*x+m[9]*y+m[10]*z+m[11]];
      const f=t.f;
      if(segs+f.length/3>MAX){ capped=true; break; }
      for(let i=0;i+8<f.length;i+=9){
        const a=X(f[i],f[i+1],f[i+2]), b=X(f[i+3],f[i+4],f[i+5]), d=X(f[i+6],f[i+7],f[i+8]);
        L.seg(a,b,col); L.seg(b,d,col); L.seg(d,a,col);
      }
      segs+=f.length/3;
    }
    if(capped) toast('Collision shown for the objects nearest the view (too many to draw them all)','ok',4000);
  },

  /* ---- measure ------------------------------------------------------------------------- */
  setMeasure(on){
    this.measure.on=!!on;
    const b=$('#tbMeasure'); if(b) b.classList.toggle('on',this.measure.on);
    const b2=$('#wgOvMeasure'); if(b2) b2.classList.toggle('pri',this.measure.on);
    if(!on){ this.measure.a=this.measure.b=null; if(App.R) App.R.setOverlay('measure',null); this.badge(''); }
    else this.badge('Click the first point');
  },
  measureClick(e){
    const R=App.R; if(!R) return;
    const s=R.surfaceAt? R.surfaceAt(e.clientX,e.clientY) : null;
    const p=(s&&s.p) || (R.groundAt? R.groundAt(e.clientX,e.clientY) : null);
    if(!p){ this.badge('Nothing there to measure to'); return; }
    const M=this.measure;
    if(!M.a || M.b){ M.a=p; M.b=null; } else M.b=p;
    this.measureDraw();
  },
  measureDraw(){
    const R=App.R, M=this.measure; if(!R||!M.a) return;
    const L=new OvlLines(), col=[1,1,0.3];
    L.cross(M.a,16,col);
    if(M.b){
      L.cross(M.b,16,col); L.seg(M.a,M.b,col);
      /* R-Zero's character gauge at the first point, facing the second - a person's
         height, and the activation sphere round them - and his ruler laid towards the
         second, 700 units long (10 m, a division 10 cm). */
      const F=this.facing(M.a,M.b);
      this.markerLines(L,'__wg/marker_character.nif',F,[0.4,1,0.5,0.8])
        .then(()=>this.markerLines(L,'__wg/marker_ruler.nif',F,[1,1,1,0.9]))
        .then(()=>{ if(this.measure.b===M.b) R.setOverlay('measure',L,{xray:true}); });
      L.seg(M.a,[M.b[0],M.b[1],M.a[2]],[1,1,0.3,0.4]);
      const dx=M.b[0]-M.a[0], dy=M.b[1]-M.a[1], dz=M.b[2]-M.a[2];
      const d=Math.hypot(dx,dy,dz), h=Math.hypot(dx,dy);
      // A game unit is about 1.43 cm (64 units to the yard).
      const m=u=>(u*0.0142875).toFixed(u*0.0142875<10? 2 : 1)+' m';
      this.badge('Distance '+Math.round(d)+' u ('+m(d)+') · across '+Math.round(h)+' u · rise '+Math.round(dz)+' u');
    }else this.badge('Click the second point');
    R.setOverlay('measure',L,{xray:true});
  },
  badge(t){
    let b=$('#wgMeasure');
    if(!t){ if(b) b.hidden=true; return; }
    if(!b){ b=document.createElement('div'); b.id='wgMeasure'; ($('#vpwrap')||document.body).appendChild(b); }
    b.hidden=false; b.textContent=t;
  },

  /* ---- links & export ---------------------------------------------------------------- */
  /** Where the view is, in the world: the orbit pivot, the camera's heading (degrees,
      0 north, 90 east), and the cell. */
  spot(){
    const R=App.R, s=this.sc(); if(!R||!s) return null;
    const o=this.origin(), c=R.cam;
    const fx=-Math.cos(c.az), fy=-Math.sin(c.az);
    let head=Math.atan2(fx,fy)*180/Math.PI; if(head<0) head+=360;
    const here=(s.cells||[])[0]||{};
    return {x:c.tx+o[0], y:c.ty+o[1], z:c.tz, head, cell:here};
  },
  consoleLines(pt, head, cell){
    const f=v=>Math.round(v);
    if(cell && cell.kind==='int')
      return 'player->positioncell '+f(pt[0])+' '+f(pt[1])+' '+f(pt[2])+' '+f(head)+' "'+cell.name+'"';
    const gx=Math.floor(pt[0]/8192), gy=Math.floor(pt[1]/8192);
    return 'coe '+gx+' '+gy+'\nplayer->position '+f(pt[0])+' '+f(pt[1])+' '+f(pt[2])+' '+f(head);
  },
  copySpot(){
    const sp=this.spot(); if(!sp){ toast('Load a cell first','warn',2500); return; }
    let t=this.consoleLines([sp.x,sp.y,sp.z+16], sp.head, sp.cell);
    const hit=(typeof Ori==='object')? Ori._hit : null;
    if(hit && hit.wpos && Ori.el && !Ori.el.hidden)
      t+='\n; beside '+hit.id+' ('+(hit.plugin||'?')+'):\n'+this.consoleLines([hit.wpos[0],hit.wpos[1],hit.wpos[2]+64], sp.head, sp.cell);
    if(navigator.clipboard) navigator.clipboard.writeText(t).then(()=>toast('Copied: paste into the OpenMW console','ok',3000)).catch(()=>toast(t,'ok',8000));
    else toast(t,'ok',8000);
    return t;
  },
  /** The report's text: the place, the view, the inspected object, the plugins. */
  reportText(){
    const sp=this.spot(); if(!sp) return '';
    const c=sp.cell||{};
    const L=[];
    L.push('# Wraithguard Cell Preview report', '');
    L.push('Cell: '+(c.kind==='int'? c.name : ((c.name? c.name+' ' : '')+'('+c.gx+', '+c.gy+')'))+(c.region? ' - '+c.region : ''));
    L.push('View: '+Math.round(sp.x)+', '+Math.round(sp.y)+', '+Math.round(sp.z)+' facing '+Math.round(sp.head)+'°');
    L.push('', 'OpenMW console:', '```', this.consoleLines([sp.x,sp.y,sp.z+16],sp.head,c), '```');
    const hit=(typeof Ori==='object')? Ori._hit : null;
    if(hit && Ori.el && !Ori.el.hidden){
      L.push('', '## Inspected object', '');
      const body=$('#oriBody'); L.push((body? body.innerText||body.textContent : '').replace(/\n{3,}/g,'\n\n').trim());
    }
    if(typeof WgCoverage==='object'){
      const m=c.kind==='int'? WgCoverage.mods({kind:'int',name:c.name}) : WgCoverage.mods({kind:'ext',x:c.gx,y:c.gy});
      if(m.length){ L.push('', '## Plugins touching this cell', ''); m.forEach(n=>L.push('- '+n)); }
    }
    if(c.landEditors && c.landEditors.length) L.push('', 'Ground edited by: '+c.landEditors.join(' → '));
    if(this.review) L.push('', 'Reviewing: '+this.review+(this.without? ' (left out)' : ''));
    if(this.checks.length){
      L.push('', '## Check results', '');
      this.checks.slice(0,100).forEach(r=>L.push('- '+r.label+(r.note? ' - '+r.note : '')));
    }
    L.push('', 'Load order: '+((typeof GameData==='object' && GameData.plugins)? GameData.plugins.length : '?')+' plugins');
    return L.join('\n')+'\n';
  },
  async saveReport(){
    const R=App.R; if(!R || !this.sc()){ toast('Load a cell first','warn',2500); return; }
    const T=window.__TAURI__;
    if(!T || !T.dialog){ toast('Saving needs the Wraithguard viewer','warn',3000); return; }
    let out;
    try{ out=await T.dialog.save({defaultPath:'cell-report.md', filters:[{name:'Markdown', extensions:['md']}]}); }
    catch(e){ toast(String(e),'err',4000); return; }
    if(!out) return;
    const png=String(out).replace(/\.md$/i,'')+'.png';
    let shot='';
    try{ R.draw(); shot=R.cv.toDataURL('image/png'); }catch(_){ shot=''; }
    const text=this.reportText().replace('# Wraithguard Cell Preview report\n','# Wraithguard Cell Preview report\n\n![view]('+png.split(/[\\/]/).pop()+')\n');
    try{
      await Engine.call('write_text',{path:String(out), text});
      if(shot && shot.length>30) await Engine.call('write_b64',{path:png, data:shot});
      toast('Report saved','ok',3000);
    }catch(e){ toast(String(e.message||e),'err',5000); }
  },

  /** Every cell and room where something is used, listed; a click opens it. */
  async whereUsed(kind, name){
    name=String(name||'').trim(); if(!name) return;
    const st=$('#wgUseStats'), list=$('#wgUseList');
    if(st) st.textContent='Looking…';
    let r;
    try{ r=await Engine.call('where_used',{kind, name}); }
    catch(e){ if(st) st.textContent=String(e.message||e); return; }
    const places=r.places||[];
    const total=places.reduce((a,p)=>a+p.count,0);
    if(st) st.textContent=places.length
      ? total+' use'+(total===1? '' : 's')+' in '+places.length+' place'+(places.length===1? '' : 's')+
        (kind==='texture'? ' ('+(r.meshes||[]).length+' meshes name it)' : kind==='mesh'? ' ('+(r.ids||[]).length+' records draw with it)' : '')
      : 'Not used in any cell.';
    if(list){
      list.hidden=!places.length;
      list.innerHTML=places.slice(0,400).map((p,i)=>'<div class="it" data-i="'+i+'"><span class="k">'+escHtml(p.label)+'</span><span class="n">'+(p.ground? 'ground ' : '')+p.count+'×</span></div>').join('');
      list.querySelectorAll('.it').forEach(el=>el.onclick=()=>{
        const p=places[+el.dataset.i]; if(!p) return;
        if(typeof WgNav==='object') WgNav.go(p.spec).then(()=>{ App._doorAim={pos:p.pos, rot:p.rot||0, key:p.spec.startsWith('int:')? 'i:'+p.spec.slice(4) : p.spec}; });
      });
    }
    const acc=$('#acc_links'); if(acc) acc.classList.remove('closed');
  },

  /* ---- ORI: every provider of an asset --------------------------------------------- */
  /** Adds to the inspector's Assets section: each file's providers on request, a
      side-by-side compare of the loose ones, and Where used. */
  decorateOri(body, r){
    if(!body || !r) return;
    const rows=[...body.querySelectorAll('.orirow')].filter(el=>{ const k=el.querySelector('.k'); return k && (k.textContent==='Mesh' || k.textContent==='Texture'); });
    for(const el of rows){
      const isMesh=el.querySelector('.k').textContent==='Mesh';
      const code=el.querySelector('code'); if(!code) continue;
      const path=code.textContent;
      if(!path || path==='—') continue;
      const v=el.querySelector('.v');
      const bar=document.createElement('div'); bar.className='wgBtns';
      bar.innerHTML='<button class="btn sm" data-a="prov" title="Every mod and archive that supplies this file, the winner first">Providers</button>'+
                    '<button class="btn sm" data-a="used" title="Every cell where this is used">Where used</button>';
      v.appendChild(bar);
      const out=document.createElement('div'); out.className='from'; v.appendChild(out);
      bar.querySelector('[data-a=used]').onclick=()=>{ const k=$('#wgUseKind'), n=$('#wgUseName'); if(k&&n){ k.value=isMesh? 'mesh' : 'texture'; n.value=path; } this.whereUsed(isMesh? 'mesh' : 'texture', path); };
      bar.querySelector('[data-a=prov]').onclick=async()=>{
        let list=[];
        try{ list=await Engine.call('asset_providers',{path, kind:isMesh? 'mesh' : 'texture'}); }catch(e){ out.textContent=String(e.message||e); return; }
        out.innerHTML=list.length? list.map(p=>(p.winner? '<b>✓ ' : '&nbsp;&nbsp;')+escHtml(p.label)+(p.winner? '</b>' : '')).join('<br>') : 'not supplied by anything';
        const loose=list.filter(p=>p.disk);
        if(isMesh && loose.length>1 && typeof WgMeshView==='object'){
          const b=document.createElement('button'); b.className='btn sm'; b.textContent='Compare '+loose.length+' side by side';
          b.onclick=()=>WgMeshView.openFrom(loose.map(p=>({path:p.disk, label:p.label+(p.winner? ' (wins)' : '')})), path+' - versions');
          out.appendChild(document.createElement('br')); out.appendChild(b);
        }
      };
    }
  },
};
