
/* =====================================================================================
   The mesh viewer: one mesh, or each mod's copy of it side by side, in the cell renderer.

   Replaces Wraithguard's old three.js mesh viewer. Wraithguard launches the shell with
   `--mesh-view` and the meshes in its extra file (`__WG_VIEW__.extra.meshes`:
   `[{path, label}]`, a path being a load-order path or one mod's own file on disk), on
   the same setup the cell preview uses, so textures resolve as the game would draw them.
   The meshes are drawn down the same path a cell's objects are (`buildStaticBatch`), at
   the origin or in a row, with the sky, lighting and effects of the Preview section.
   While it is on it owns the viewport: the preview rebuild draws the meshes, not a cell.
   Beside it, the shown mesh's block tree (`inspect`, from Wraithguard's NIF reader) and
   the selected block's fields; with `edit` ({url, filename}) the editable ones take
   input, and Save sends the edits to Wraithguard, which applies them to the original
   and answers with the edited file, written where the user picks (wg_save_edited).
   Comparing versions (the old viewer's tools): one at a time, side by side, or overlaid
   at the origin with a checkbox each and the camera framed over all of them, so it never
   moves as they are switched; lit, flat colour or the surface normals; normal maps; and
   each shape with what it is made of, click to isolate.
   Copyright (c) 2026 StaticNation.
   ===================================================================================== */
const WgMeshView={
  list:[], idx:0, layout:'one', on:false, el:null, _gen:0, sel:null, pending:{},
  light:{angle:0.6, elevation:0.5, key:1.4, ambient:0.25},
  shown:null, solo:null, shapes:[], viewMode:0, nrmOn:true, mapsOff:{}, alphaMode:'file', showCol:false, _col:new Map(),

  active(){ return this.on && this.list.length>0; },

  /** Before the connect: the window says what it is from the first frame, and the cell
      side's startup (the intro card, the picker) stays out of the way. */
  prepare(){
    document.title='Wraithguard - Mesh Viewer';
    const i=document.querySelector('#brand i'); if(i) i.textContent='Mesh Viewer';
    document.body.classList.add('wgMeshMode');
    App.loadNormalMaps=true;
    if(App.R){ App.R.opts.normalMaps=true; }
  },

  /** Opens the viewer on `meshes` ([{path,label}]). */
  open(meshes){
    this.list=(meshes||[]).filter(m=>m && m.path).map(m=>({path:String(m.path), label:String(m.label||m.path),
      inspect:m.inspect||null, edit:(m.edit && m.edit.url)? m.edit : null}));
    this.sel=null; this.pending={};
    if(!this.list.length) return;
    this.on=true; this.idx=0; this.layout='one'; this.shown=this.list.map(()=>true); this.solo=null;
    App.loadNormalMaps=true;
    if(App.R){ App.R.opts.normalMaps=this.nrmOn; App.R.opts.viewMode=this.viewMode; App.R.opts.mapsOff=this.mapsOff; }
    document.title='Wraithguard - Mesh Viewer';
    const i=document.querySelector('#brand i'); if(i) i.textContent='Mesh Viewer';
    this.panel();
    schedulePreview();
  },

  /** Wraithguard: opens the viewer from a cell - a container's contents (46_wg_tfh.js),
      side by side - remembering the cell and the camera, so "Back to cell" returns to
      them rather than to the picker. */
  openFrom(meshes, label){
    this.back={cellSel:App.cellSel, cam:App.R? Object.assign({}, App.R.cam) : null, label:label||''};
    this.open(meshes);
    if(this.list.length>1){ this.layout='side'; this.panel(); schedulePreview(); }
  },

  /** Back to cells: the picker, and the scene is the cell's again. */
  close(){
    this.on=false; if(this.el) this.el.hidden=true;
    document.body.classList.remove('wgMeshMode');
    if(App.R){ App.R.opts.viewMode=0; App.R.opts.normalMaps=!!App.cellNormalMaps; App.R.opts.mapsOff=null; }
    // And stop reading the extra maps: left on, every mesh of the next cell loaded its
    // normal, specular and gloss maps too, whatever the cell's own switch said.
    App.loadNormalMaps=!!App.cellNormalMaps;
    document.title='Wraithguard - Cell Preview';
    const i=document.querySelector('#brand i'); if(i) i.textContent='Cell Preview';
    App._scene=null; App._framedKey=null;
    // Wraithguard: opened from a cell - back to it, the camera where it was.
    const back=this.back; this.back=null;
    if(back && back.cellSel){
      App.mode='cell'; App.cellSel=back.cellSel; App._camAfter=back.cam;
      if(typeof syncCellButton==='function') syncCellButton();
      schedulePreview();
      return;
    }
    if(typeof openCellPicker==='function') openCellPicker();
    schedulePreview();
  },

  /** The panel: the versions, side by side, and the shown mesh's textures. */
  panel(){
    let d=this.el;
    if(!d){
      d=document.createElement('div'); d.id='meshViewPanel'; d.className='ori';
      d.innerHTML='<div class="orihead"><b>Mesh viewer</b><span style="flex:1"></span>'+
        '<button class="btn sm" id="mvBack" title="Leave the mesh viewer for the cell map">Cell map</button></div>'+
        '<div class="oribody" id="mvBody"></div>';
      ($('#vpwrap')||document.body).appendChild(d);
      d.querySelector('#mvBack').onclick=()=>this.close();
      this.el=d;
    }
    d.hidden=false;
    // Opened from a cell: the way back says so.
    const bk=d.querySelector('#mvBack');
    if(bk){ bk.textContent=this.back? 'Back to cell' : 'Cell map'; bk.title=this.back? 'Back to the cell and the view you left' : 'Leave the mesh viewer for the cell map'; }
    const hd=d.querySelector('.orihead b'); if(hd) hd.textContent=(this.back&&this.back.label)||'Mesh viewer';
    const b=d.querySelector('#mvBody');
    let h='';
    if(this.list.length>1){
      const L=this.layout;
      h+='<div class="orisec">Versions</div>'+
         '<div class="orirow mvSeg">'+[['one','One'],['side','Side by side'],['overlay','Overlaid']].map(([k,t])=>
           '<button class="btn sm'+(L===k?' on':'')+'" data-layout="'+k+'">'+t+'</button>').join('')+'</div>';
      this.list.forEach((m,k)=>{
        const pick = L==='overlay'
          ? '<input type="checkbox" data-shown="'+k+'"'+(this.shown[k]?' checked':'')+'> '
          : '';
        h+='<div class="orirow"><span class="v">'+pick+'<a class="orimod" data-k="'+k+'"'+
           (L!=='side' && k===this.idx? ' style="font-weight:600"' : '')+'>'+escHtml(m.label)+'</a>'+
           '<br><span class="from">'+escHtml(m.path)+'</span></span></div>';
      });
    } else {
      h+='<div class="orirow"><span class="k">Mesh</span><span class="v"><code>'+escHtml(this.list[0].path)+'</code></span></div>';
    }
    h+='<div class="orisec">View</div><div class="orirow mvSeg">'+
       [[0,'Lit'],[1,'Flat colour'],[2,'Relief']].map(([k,t])=>
         '<button class="btn sm'+(this.viewMode===k?' on':'')+'" data-view="'+k+'" title="'+
         (k===1?'Clay grey under a light that moves with the camera: the shape, and the relief the normal map puts in it':
          k===2?'The textures under a light that moves with the camera, raking across them: the normal map bends it, the specular map shines in it':'As the game lights it')+'">'+t+'</button>').join('')+'</div>'+

       /* The light the studio views use (Flat colour, Relief), as the old texture
          viewer's lit material had it: where it comes from, round the view and above it,
          and how strong it and the ambient are. */
       '<div class="orisec">Light (Flat colour, Relief)</div>'+
       [['angle','Angle',-3.14,3.14,0.05],['elevation','Elevation',-1.5,1.5,0.05],['key','Light',0,4,0.05],['ambient','Ambient',0,2,0.05]].map(([k,t,lo,hi,st])=>
         '<div class="orirow"><span class="k">'+t+'</span><span class="v"><input type="range" style="width:100%" data-light="'+k+'" min="'+lo+'" max="'+hi+'" step="'+st+'" value="'+this.light[k]+'"></span></div>').join('')+
       '<div class="orisec">Maps</div><div id="mvMaps" class="from">…</div>'+
       '<div class="orisec">Alpha</div><div class="orirow mvSeg">'+
       [['file','As the file says'],['cutout','Cutout'],['opaque','Opaque']].map(([k,t])=>
         '<button class="btn sm'+(this.alphaMode===k?' on':'')+'" data-alpha="'+k+'" title="'+
         (k==='cutout'?'Every shape cut at half alpha, no blending: what a cutout texture keeps':
          k==='opaque'?'No alpha at all: the whole of every quad':'Each shape as its NiAlphaProperty asks')+'">'+t+'</button>').join('')+'</div>'+
       '<div class="orirow"><label class="sw sm"><input type="checkbox" id="mvCol"'+(this.showCol?' checked':'')+
       '><span class="tr"></span></label><span class="v">Collision shape <span class="from" id="mvColN"></span></span></div>'+
       '<div id="mvStats" class="from"></div>';
    h+='<div class="orisec">Shapes</div><div id="mvShapes" class="from">…</div>';
    h+='<div class="orisec">Textures</div><div id="mvTex" class="from">…</div>';
    const cur=this.list[this.idx];
    if(cur && cur.inspect && cur.inspect.tree && cur.inspect.tree.length){
      h+='<div class="orisec">Blocks'+(this.layout!=='one' && this.list.length>1? ' · '+escHtml(cur.label) : '')+'</div>'+
         '<div id="mvTree" class="nifTree">'+this.treeHtml(cur.inspect.tree)+'</div>'+
         '<div id="mvFields"></div>';
      if(cur.edit) h+='<div class="orirow"><button class="btn sm" id="mvSave">Save edited .nif…</button>'+
                     '<span class="from" id="mvSaveSt" style="margin-left:8px"></span></div>';
      else if(cur.inspect.complete===false) h+='<div class="from">This mesh does not parse completely; read-only.</div>';
    }
    b.innerHTML=h;
    const tr=b.querySelector('#mvTree');
    if(tr) tr.onclick=e=>{ const li=e.target.closest('[data-bi]'); if(li) this.select(+li.dataset.bi); };
    const sv=b.querySelector('#mvSave'); if(sv) sv.onclick=()=>this.save();
    if(this.sel!=null) this.select(this.sel);
    this.status();
    this.shapeList();
    b.querySelectorAll('a[data-k]').forEach(a=>a.onclick=()=>{
      if(+a.dataset.k!==this.idx){ this.sel=null; this.pending={}; }
      this.idx=+a.dataset.k; if(this.layout==='side') this.layout='one';
      this.solo=null; this.panel(); schedulePreview(); });
    b.querySelectorAll('[data-layout]').forEach(x=>x.onclick=()=>{ this.layout=x.dataset.layout; this.solo=null; App._framedKey=null; this.panel(); schedulePreview(); });
    b.querySelectorAll('[data-shown]').forEach(x=>x.onchange=()=>{ this.shown[+x.dataset.shown]=x.checked; this.solo=null; schedulePreview(); });
    // The studio light's sliders.
    if(App.R) App.R.opts.keyLight=this.light;
    b.querySelectorAll('[data-light]').forEach(x=>x.oninput=()=>{
      this.light[x.dataset.light]=+x.value;
      if(App.R){ App.R.opts.keyLight=this.light; App.R.dirty=true; }
    });
    b.querySelectorAll('[data-view]').forEach(x=>x.onclick=()=>{
      this.viewMode=+x.dataset.view; if(App.R){ App.R.opts.viewMode=this.viewMode; App.R.dirty=true; }
      b.querySelectorAll('[data-view]').forEach(y=>y.classList.toggle('on', y===x));
    });
    b.querySelectorAll('[data-alpha]').forEach(x=>x.onclick=()=>{
      this.alphaMode=x.dataset.alpha;
      b.querySelectorAll('[data-alpha]').forEach(y=>y.classList.toggle('on', y===x));
      schedulePreview();
    });
    const cb=b.querySelector('#mvCol'); if(cb) cb.onchange=()=>{ this.showCol=cb.checked; schedulePreview(); };
  },

  /** The block tree as nested lists: index, type, name, note. */
  treeHtml(nodes){
    return '<ul>'+nodes.map(n=>'<li><div class="nb" data-bi="'+n.index+'"><span class="ix">'+n.index+'</span> '+
      '<span class="ty">'+escHtml(n.type)+'</span>'+(n.name? ' <span class="nm">'+escHtml(n.name)+'</span>' : '')+
      (n.note? ' <span class="no">'+escHtml(n.note)+'</span>' : '')+'</div>'+
      (n.children && n.children.length? this.treeHtml(n.children) : '')+'</li>').join('')+'</ul>';
  },

  /** Selects block `bi`: its fields below the tree, editable ones as inputs when editing. */
  select(bi){
    const cur=this.list[this.idx], box=$('#mvFields'); if(!cur || !cur.inspect || !box) return;
    this.sel=bi;
    document.querySelectorAll('#mvTree .nb.sel').forEach(x=>x.classList.remove('sel'));
    const row=document.querySelector('#mvTree .nb[data-bi="'+bi+'"]'); if(row) row.classList.add('sel');
    const blk=cur.inspect.blocks[String(bi)];
    if(!blk){ box.innerHTML='<div class="from">No fields for this block.</div>'; return; }
    const edit=!!cur.edit;
    const val=f=>{ const k=bi+':'+f.name; return k in this.pending? this.pending[k].value : f.value; };
    const widget=f=>{
      if(!edit || !f.editable) return '<span class="v">'+escHtml(f.value==null? '—' : String(f.value))+'</span>';
      if(f.kind==='bool32') return '<input type="checkbox" data-f="'+escHtml(f.name)+'" data-kind="bool32"'+(val(f)===true?' checked':'')+'>';
      return '<input class="nf" type="text" data-f="'+escHtml(f.name)+'" data-kind="'+escHtml(f.kind)+'" value="'+escHtml(val(f)==null? '' : String(val(f)))+'">';
    };
    const rows=list=>list.map(f=>'<div class="orirow'+((bi+':'+f.name) in this.pending? ' dirty' : '')+'"><span class="k" title="'+
      escHtml(f.kind)+'">'+escHtml(f.name)+'</span>'+widget(f)+'</div>').join('');
    const ed=blk.fields.filter(f=>f.editable), ro=blk.fields.filter(f=>!f.editable);
    box.innerHTML='<div class="orisec">'+escHtml(blk.type)+' · block '+bi+'</div>'+
      (ed.length? rows(ed) : '')+(ro.length? '<details'+(ed.length? '' : ' open')+'><summary class="from">structure ('+ro.length+')</summary>'+rows(ro)+'</details>' : '')+
      (!blk.fields.length? '<div class="from">No fields.</div>' : '');
    box.querySelectorAll('[data-f]').forEach(el=>el.onchange=()=>{
      const k=el.dataset.kind, name=el.dataset.f;
      let v=el.value;
      if(k==='bool32') v=el.checked;
      else if(k==='f32'){ v=parseFloat(v); if(!isFinite(v)){ el.classList.add('bad'); return; } }
      else if(/^(u8|u16|u32|i32|link)$/.test(k)){ v=parseInt(v,10); if(!Number.isFinite(v)){ el.classList.add('bad'); return; } }
      el.classList.remove('bad');
      this.pending[bi+':'+name]={op:'set_field', block:bi, name:name, value:v};
      const r=el.closest('.orirow'); if(r) r.classList.add('dirty');
      this.status();
    });
  },

  status(msg){
    const st=$('#mvSaveSt'); if(!st) return;
    const n=Object.keys(this.pending||{}).length;
    st.textContent=msg || (n? n+' edit'+(n>1?'s':'')+' pending' : '');
  },

  /** Save: pick where, then Wraithguard applies the edits and the result is written there. */
  async save(){
    const cur=this.list[this.idx]; if(!cur || !cur.edit) return;
    const edits=Object.values(this.pending||{});
    if(!edits.length){ this.status('no edits to save'); return; }
    const T=window.__TAURI__;
    if(!T || !T.dialog || !T.core){ this.status('saving needs the Wraithguard viewer'); return; }
    let out;
    try{ out=await T.dialog.save({defaultPath:cur.edit.filename||'edited.nif', filters:[{name:'NIF mesh', extensions:['nif']}]}); }
    catch(e){ this.status('error: '+e); return; }
    if(!out) return;
    this.status('saving…');
    try{
      await T.core.invoke('wg_save_edited', {url:cur.edit.url, body:JSON.stringify({edits}), out:String(out)});
      this.pending={};
      document.querySelectorAll('#mvFields .orirow.dirty').forEach(x=>x.classList.remove('dirty'));
      this.status('saved ✓ '+out);
    }catch(e){ this.status('error: '+e); }
  },

  /** A shape as the Alpha buttons have it: the file's own (unchanged), every one cut at
      half alpha with no blending, or no alpha at all. */
  alphaOverride(p){
    if(this.alphaMode==='cutout') return Object.assign({}, p, {alphaFlags:0x200|(4<<10), alphaThreshold:128});
    if(this.alphaMode==='opaque') return Object.assign({}, p, {alphaFlags:0, alphaThreshold:null});
    return p;
  },

  /** A mesh's collision shape as drawable parts, read once per path (mesh_collision). */
  async collision(path){
    if(this._col.has(path)) return this._col.get(path);
    let out={shape:2, tris:0, parts:[]};
    try{
      const ab=await Engine.bytes('mesh_collision',{path});
      const dv=new DataView(ab);
      if(dv.byteLength>=9 && dv.getUint32(0,true)===0x4c434447){   // "GDCL"
        const shape=dv.getUint8(4), n=dv.getUint32(5,true);
        const all=new Float32Array(ab.slice(9, 9+n*36));
        out={shape, tris:n, parts:[]};
        const PER=21845;   // triangles a 16-bit index reaches
        for(let t0=0;t0<n;t0+=PER){
          const tn=Math.min(PER,n-t0), pos=all.slice(t0*9,(t0+tn)*9), nrm=new Float32Array(pos.length);
          for(let t=0;t<tn;t++){
            const o=t*9, ax=pos[o+3]-pos[o], ay=pos[o+4]-pos[o+1], az=pos[o+5]-pos[o+2],
                  bx=pos[o+6]-pos[o], by=pos[o+7]-pos[o+1], bz=pos[o+8]-pos[o+2];
            let nx=ay*bz-az*by, ny=az*bx-ax*bz, nz=ax*by-ay*bx; const l=Math.hypot(nx,ny,nz)||1;
            nx/=l; ny/=l; nz/=l;
            for(let v=0;v<3;v++){ nrm[o+v*3]=nx; nrm[o+v*3+1]=ny; nrm[o+v*3+2]=nz; }
          }
          const idx=new Uint16Array(tn*3); for(let i=0;i<idx.length;i++) idx[i]=i;
          const col=new Uint8Array(tn*9); for(let i=0;i<tn*3;i++){ col[i*3]=60; col[i*3+1]=230; col[i*3+2]=110; }
          out.parts.push({name:'collision', pos, nrm, uv:new Float32Array(tn*6), uv2:null, idx, col, tex:null,
            detail:null, alphaThreshold:null, alphaFlags:0x00ED, glow:null, glowUV:0, twoSided:true, drawMode:3,
            unlit:true, vcolMode:2, dayNight:0, zwrite:false, matAlpha:0.35, diffuse:[1,1,1], ambient:null,
            emissive:[0,0,0], uvAnim:null, colA:null, decal:null, uvDecal:null, clamp:0, uvAnim2:null,
            env:null, envClamp:0, bump:null, bumpLuma:null, bumpMat:null});
        }
      }
    }catch(e){ console.warn('collision', path, e); }
    this._col.set(path,out);
    return out;
  },

  /** The maps the drawn shapes use, a switch each (the renderer's opts.mapsOff). */
  mapList(){
    const box=$('#mvMaps'); if(!box) return;
    const kinds=[['normal','Normal','glNrm'],['specular','Specular','glSpec'],['gloss','Gloss','gloss'],['detail','Detail','detail'],['dark','Dark','dark'],
                 ['glow','Glow','glow'],['decal','Decal','decal'],['env','Environment','env']];
    const rows=kinds.map(([k,t,f])=>[k,t,this.shapes.filter(s=>s.part[f]).length]).filter(r=>r[2]>0);
    if(!rows.length){ box.textContent='none beyond the base texture'; return; }
    box.innerHTML=rows.map(([k,t,n])=>'<label style="display:inline-flex;gap:4px;margin-right:10px"><input type="checkbox" data-map="'+k+'"'+
      (this.mapsOff[k]?'':' checked')+'>'+t+' <span>('+n+')</span></label>').join('');
    box.querySelectorAll('[data-map]').forEach(x=>x.onchange=()=>{
      this.mapsOff[x.dataset.map]=!x.checked;
      if(App.R){ App.R.opts.mapsOff=this.mapsOff; App.R.dirty=true; }
    });
  },

  /** What a shape is made of, as a line rather than controls: which shape wants a
      cutout or has lost its glow map reads well enough from this. */
  summary(p){
    const tris=(p.idx? p.idx.length/3 : 0)|0, verts=(p.pos? p.pos.length/3 : 0)|0;
    const bits=[tris+' tri', verts+' vert'];
    if(!p.uv || !p.uv.length) bits.push('no UVs');
    if(p.col) bits.push('vertex colours');
    const blend=typeof alphaBlend==='function'? alphaBlend(p) : null;
    if(blend) bits.push('blend'+(p.matAlpha!=null && p.matAlpha<1? ' @ '+Math.round(p.matAlpha*100)+'%' : ''));
    if(p.alphaThreshold!=null) bits.push('cutout @ '+p.alphaThreshold);
    if(p.twoSided) bits.push('two-sided');
    if(p.unlit) bits.push('unlit');
    const maps=[];
    if(p.tex) maps.push('base');
    if(p.glNrm) maps.push('normal');
    if(p.glSpec) maps.push('specular');
    if(p.gloss) maps.push('gloss');
    if(p.detail) maps.push('detail');
    if(p.dark) maps.push('dark');
    if(p.glow) maps.push('glow');
    if(p.decal) maps.push('decal');
    if(p.env) maps.push('env'+(p.bump? '+bump' : ''));
    if(maps.length) bits.push('maps: '+maps.join(', '));
    if(p.uvAnim) bits.push('UV animated');
    return bits.join(' · ');
  },

  /** The shapes drawn, with the stats; click one to show it alone, again for all. */
  shapeList(){
    const box=$('#mvShapes'), st=$('#mvStats');
    let tris=0, verts=0;
    for(const s of this.shapes){ tris+=(s.part.idx? s.part.idx.length/3 : 0); verts+=(s.part.pos? s.part.pos.length/3 : 0); }
    if(st) st.textContent=this.shapes.length+' shape(s), '+Math.round(tris).toLocaleString()+' triangles, '+
                          Math.round(verts).toLocaleString()+' vertices';
    if(!box) return;
    if(!this.shapes.length){ box.textContent='none'; return; }
    const multi=new Set(this.shapes.map(s=>s.version)).size>1;
    let h='', last=-1;
    for(const s of this.shapes){
      if(multi && s.version!==last){ h+='<div style="margin-top:4px"><b>'+escHtml(s.label)+'</b></div>'; last=s.version; }
      h+='<div class="mvShape'+(this.solo===s.key?' on':'')+'" data-shape="'+s.key+'" title="Click to show only this shape; click again for all">'+
         '<code>'+escHtml(s.part.name||'(unnamed)')+'</code><br><span>'+escHtml(this.summary(s.part))+'</span></div>';
    }
    box.innerHTML=h;
    box.querySelectorAll('[data-shape]').forEach(el=>el.onclick=()=>{
      this.solo = this.solo===el.dataset.shape? null : el.dataset.shape;
      const R=App.R;
      if(R){ R.setStatics(this.solo? this._all.filter(bt=>bt._wgShape===this.solo) : this._all); R.dirty=true; }
      box.querySelectorAll('[data-shape]').forEach(x=>x.classList.toggle('on', x.dataset.shape===this.solo));
    });
  },

  /** Draws the meshes (called by the preview rebuild while the viewer is on). */
  async render(){
    const R=App.R; if(!R) return;
    const gen=++this._gen;
    R.disposeBatches(); R.setBatches([]);
    // The shapes an isolate left out of the renderer's list are this viewer's to free.
    if(this._all){ const live=new Set(R.statics); for(const bt of this._all) if(!live.has(bt)) R._dropVao(bt); this._all=null; }
    R.disposeStatics(); R.setStatics([]);
    R.setPickables([]); R.setMarkers(null); R.setMissing(null);
    R.setArrows(null); R.arrowFrom=null;
    R.setCellTerrain([]); R.setWater(null);
    if(R.clearCellPaint) R.clearCellPaint();
    R.opts.room=null;
    if(App.syncAtmosphereSwitch) App.syncAtmosphereSwitch(false);
    App._scene=null;
    const st=$('#stats'); if(st) st.hidden=true;
    const L=this.list.length>1? this.layout : 'one';
    /* Overlaid: every version is loaded and framed, the unticked ones just not drawn, so
       the camera stays put as they are switched. */
    const show = L==='one' ? [this.list[this.idx]] : this.list;
    const drawn = L==='overlay' ? this.shown : show.map(()=>true);
    const infos=[];
    // A file on disk goes to the engine as `file:<path>`, which survives the page's
    // load-order path normalising (a leading slash would not).
    const key=p=>/^([a-z]:[\\/]|\/|\\\\)/i.test(p)? 'file:'+p : p;
    for(const m of show) infos.push(await loadMesh(key(m.path)));
    if(gen!==this._gen || !this.active()) return;          // a newer render has the viewport
    // Side by side: in a row along x, each by its own width, a gap between. Otherwise
    // all at the origin.
    const batches=[], picks=[], texRows=[], shapes=[];
    let x=0, zMin=Infinity, zMax=-Infinity, x0=Infinity, x1=-Infinity, yMax=0, nrmN=0;
    show.forEach((m,k)=>{
      const info=infos[k];
      if(!info || info.err || !(info.parts.length || (info.particles && info.particles.length))){
        texRows.push('<div><b>'+escHtml(m.label)+'</b>: '+escHtml((info&&info.err)||'nothing to draw')+'</div>');
        return;
      }
      const a=info.visAabb||info.aabb||{x0:-64,x1:64,y0:-64,y1:64,z0:0,z1:128};
      const w=a.x1-a.x0, ox= L==='side' ? x-a.x0 : 0;
      (this._ox||(this._ox=[]))[k]=ox;
      const mm=new Float32Array(refMatrix([ox,0,0],[0,0,0],1));
      x0=Math.min(x0,a.x0+ox); x1=Math.max(x1,a.x1+ox);
      zMin=Math.min(zMin,a.z0); zMax=Math.max(zMax,a.z1); yMax=Math.max(yMax,a.y1-a.y0);
      x+=w+Math.max(32,w*0.25);
      if(!drawn[k]) return;
      info.parts.forEach((part0,pi)=>{
        const part=this.alphaOverride(part0);
        const bt=R.buildStaticBatch(part,{m:mm,n:1},part.glTex? part.glTex.gl : null);
        bt.box=[a.x0+ox,a.y0,a.z0,a.x1+ox,a.y1,a.z1];
        bt._wgShape=k+':'+pi;
        batches.push(bt);
        if(part.glNrm) nrmN++;
        shapes.push({key:k+':'+pi, version:k, label:m.label, part:part0});
      });
      if(info.particles && info.particles.length && R.addParticleSystems) R.addParticleSystems(info.particles, mm, null, m.path);
      if(info.visAabb) picks.push({id:m.label, model:m.path, m:Array.from(mm), parts:info.parts,
        aabb:worldAabb(info.visAabb,Array.from(mm)), plugin:'', refKey:'', scale:1});
      const tex=[...new Set(info.parts.map(p=>p.tex).filter(Boolean))];
      texRows.push((show.length>1? '<div style="margin-top:4px"><b>'+escHtml(m.label)+'</b></div>' : '')+
        (tex.length? tex.map(t=>'<div><code>'+escHtml(t)+'</code></div>').join('') : '<div>none</div>'));
    });
    // The collision shape over them, when asked for: translucent green, both sides.
    const colNote=[];
    if(this.showCol){
      for(let k=0;k<show.length;k++){
        if(!drawn[k] || !infos[k] || infos[k].err) continue;
        const c=await this.collision(key(show[k].path));
        if(gen!==this._gen || !this.active()) return;
        const a=infos[k].visAabb||infos[k].aabb||{x0:0,x1:0};
        const ox= L==='side' ? this._ox[k] : 0;
        const mm=new Float32Array(refMatrix([ox,0,0],[0,0,0],1));
        for(const part of c.parts){ const bt=R.buildStaticBatch(part,{m:mm,n:1},null); bt._wgShape='col:'+k; batches.push(bt); }
        colNote.push((show.length>1? show[k].label+': ' : '')+(c.shape===0? c.tris+' triangles (collision node)'
                     : c.shape===1? 'none of its own - the visible mesh stands in' : 'none (not solid)'));
      }
    }
    const cn=$('#mvColN'); if(cn) cn.textContent=colNote.length? '· '+colNote.join('; ') : '';
    this._all=batches; this.shapes=shapes;
    this.mapList();
    R.setStatics(this.solo? batches.filter(bt=>bt._wgShape===this.solo) : batches);
    R.setPickables(picks);
    const tb=$('#mvTex'); if(tb) tb.innerHTML=texRows.join('');
    this.shapeList();
    if(isFinite(x0)){
      const span=Math.max(64, Math.max(x1-x0, zMax-zMin, yMax)*1.6);
      const cx=(x0+x1)/2, cz=(zMin+zMax)/2;
      const fk='mesh:'+L+':'+show.map(m=>m.path).join('|');
      if(App._framedKey!==fk){ R.frameAt(span, cz, cx, 0); App._framedKey=fk; }
    }
    R.dirty=true;
  },
};
