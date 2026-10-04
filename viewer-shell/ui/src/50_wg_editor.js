/* =====================================================================================
   The Editor mode - Wraithguard. The Construction Set's working windows around the
   render window: a mode of its own, not the Cell Preview's settings.

   - The Object Window: every record of the load order by type (tabs), with a filter
     (Ctrl+F), sortable columns - id, name, model, the plugins that define it (the last
     wins) and how many are placed - and the records with changes waiting marked.
     Double-click opens one.
   - The Cell View: every cell, and the references in the one selected. Double-click a
     cell to open it in the render window; a reference to go to it, framed and selected.
   - The record dialog: each field (the conflict viewer's dotted paths), its value in
     the winning plugin, and an input that sends a change. Changes go to Wraithguard's
     patch pool - the queue its conflict viewer fills - and are reviewed and written in
     its Patch Builder; Wraithguard journals the pool, so a viewer that runs out of
     memory loses nothing.
   - The reference dialog: one placed object - position and rotation with nudges,
     scale, deleted, owner, lock, key, trap, count. Changes go to the pool too, and the
     patch carries them as merge_to_master does (patch/refedit.py): a CELL record with
     only that reference, keyed to the file that created it. The render window draws
     every pending reference change in place (`overlay`, over CellData.loadCell).
   - The inspector (24_ori.js) gets "Edit record" (F2) and "Edit reference" (F3) for
     whatever is clicked.
   - The render window moves the selected object as the Construction Set does: drag it
     (left button) across the ground plane at its height; hold Z to lift and lower it,
     X or Y to keep to that axis; Shift-drag turns it (about Z, or X/Y held); F drops it
     onto what is under it. A gold copy follows the pointer and the change goes to the
     pool on release (`grab`, over the renderer's `onGrab`).

   The lists come from the shell (`editor_tags`, `editor_records`, `editor_cell_refs`,
   `cells`); a record's contents and every change from Wraithguard, over the `links` the
   launch handed over (`editRecord`, `editSet`, `editRevert`, `editRef`, `editRefSet`,
   `editRefRevert`, `editPending`, `editReview`; gui/editorlink.py). Opened without Wraithguard, it browses but cannot
   edit, and says so.

   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
const WgEditor={
  on:false, el:null, dlg:null, pend:null,
  tag:'STAT', tags:[], rows:[], plugins:[], filter:'', sort:{col:0, dir:1},
  cells:null, cellFilter:'', cellSel:null, refs:[],
  edited:new Set(),          // "TAG:id lower" with changes waiting in the pool
  record:null,               // the dialog's {tag, id, plugins}
  refRec:null,               // or, for a placed object, {cell, origin, refr, plugins}
  live:new Map(),            // "origin:refr" lower -> pending changes, drawn in place
  liveSig:'[]',              // changes when `live` does: part of the preview's scene key
  NUDGE:8,                   // units a nudge moves (Shift: 8x); degrees a turn (Shift: 8x)
  held:new Set(),            // Z, X, Y held: the drag's axis keys
  TURN:0.01,                 // radians a pixel of Shift-drag turns
  ROW_CAP:600,

  /** The tags' names, for the tabs; anything else shows as its tag. */
  NAMES:{STAT:'Static', ACTI:'Activator', DOOR:'Door', CONT:'Container', LIGH:'Light', MISC:'Misc item',
    WEAP:'Weapon', ARMO:'Armor', CLOT:'Clothing', BOOK:'Book', ALCH:'Potion', INGR:'Ingredient', APPA:'Apparatus',
    LOCK:'Lockpick', PROB:'Probe', REPA:'Repair item', NPC_:'NPC', CREA:'Creature', LEVI:'Leveled item',
    LEVC:'Leveled creature', SPEL:'Spell', ENCH:'Enchantment', SCPT:'Script', FACT:'Faction', RACE:'Race',
    CLAS:'Class', BSGN:'Birthsign', REGN:'Region', SOUN:'Sound', SNDG:'Sound generator', BODY:'Body part',
    GMST:'Game setting', GLOB:'Global', DIAL:'Dialogue', LTEX:'Land texture', SKIL:'Skill', MGEF:'Magic effect'},

  links(){ return (((window.__WG_VIEW__||{}).extra)||{}).links||{}; },
  canEdit(){ return !!this.links().editRecord; },

  /** The topbar's "Editor" switch, once an install is connected (not in the mesh viewer). */
  button(){
    if($('#btnEditor') || (typeof WgMeshView==='object' && WgMeshView.on)) return;
    const after=$('#btnReport'); if(!after) return;
    const b=document.createElement('button');
    b.className='btn dim'; b.id='btnEditor'; b.textContent='Editor';
    b.title='The Construction Set\'s Object Window and Cell View beside the render window: browse every record and cell, and change records - the changes go to Wraithguard\'s patch pool';
    b.onclick=()=> this.on? this.leave() : this.enter();
    after.insertAdjacentElement('afterend', b);
  },

  async enter(){
    if(this.on) return;
    this.on=true;
    document.body.classList.add('wgEditMode');
    const i=document.querySelector('#brand i'); if(i) i.textContent='Editor';
    document.title='Wraithguard - Editor';
    const b=$('#btnEditor'); if(b) b.classList.add('on');
    this.dock();
    try{
      this.tags=await Engine.call('editor_tags',{});
    }catch(e){ toast(String(e.message||e),'err',5000); this.tags=[]; }
    if(!this.tags.some(t=>t[0]===this.tag) && this.tags.length) this.tag=this.tags[0][0];
    this.fillTabs();
    await Promise.all([this.loadTag(this.tag), this.loadCells(), this.refreshPending()]);
  },

  leave(){
    this.on=false;
    document.body.classList.remove('wgEditMode');
    const i=document.querySelector('#brand i'); if(i) i.textContent='Cell Preview';
    document.title='Wraithguard - Cell Preview';
    const b=$('#btnEditor'); if(b) b.classList.remove('on');
    if(this.dlg) this.dlg.hidden=true;
    if(this.pend) this.pend.hidden=true;
  },

  /* ---- the dock ------------------------------------------------------------------- */

  dock(){
    if(this.el){ this.el.hidden=false; return; }
    const d=document.createElement('div');
    d.id='edDock';
    d.innerHTML=
      '<div class="edPane" id="edObjects">'+
        '<div class="edHead"><b>Object Window</b><span style="flex:1"></span>'+
          '<button class="btn sm" id="edPendBtn" title="The changes waiting in Wraithguard\'s patch pool">Pending</button></div>'+
        '<div class="edTabs" id="edTabs"></div>'+
        '<div class="edBar"><input class="fld" id="edFilter" placeholder="Filter: id, name or model (Ctrl+F)" spellcheck="false">'+
          '<span class="from" id="edCount"></span></div>'+
        '<div class="edTable" id="edTable"></div>'+
      '</div>'+
      '<div class="edPane" id="edCells">'+
        '<div class="edHead"><b>Cell View</b><span style="flex:1"></span><span class="from" id="edCellName"></span></div>'+
        '<div class="edBar"><input class="fld" id="edCellFilter" placeholder="Filter cells" spellcheck="false"></div>'+
        '<div class="edSplit"><div class="edTable" id="edCellList"></div><div class="edTable" id="edRefList"></div></div>'+
      '</div>';
    const main=$('#main')||document.body;
    main.insertBefore(d, main.firstChild);
    this.el=d;
    d.querySelector('#edFilter').oninput=e=>{ this.filter=e.target.value; this.drawRows(); };
    d.querySelector('#edCellFilter').oninput=e=>{ this.cellFilter=e.target.value; this.drawCells(); };
    d.querySelector('#edPendBtn').onclick=()=>this.showPending();
  },

  fillTabs(){
    const t=this.el.querySelector('#edTabs');
    t.innerHTML=this.tags.map(([tag,n])=>
      '<button class="btn sm'+(tag===this.tag?' on':'')+'" data-tag="'+escHtml(tag)+'" title="'+escHtml(tag)+' - '+n+' records">'+
      escHtml(this.NAMES[tag]||tag)+'</button>').join('');
    t.querySelectorAll('[data-tag]').forEach(b=>b.onclick=()=>{
      this.tag=b.dataset.tag;
      t.querySelectorAll('.on').forEach(x=>x.classList.remove('on')); b.classList.add('on');
      this.loadTag(this.tag);
    });
  },

  async loadTag(tag){
    try{
      const r=await Engine.call('editor_records',{tag});
      this.plugins=r.plugins||[]; this.rows=r.rows||[];
    }catch(e){ toast(String(e.message||e),'err',5000); this.rows=[]; }
    if(tag===this.tag) this.drawRows();
  },

  /** The Object Window's table: filtered, sorted, capped (the filter narrows it). */
  drawRows(){
    const box=this.el.querySelector('#edTable');
    const f=this.filter.trim().toLowerCase();
    let rows=this.rows;
    if(f) rows=rows.filter(r=>r[0].toLowerCase().includes(f) || r[1].toLowerCase().includes(f) || r[2].toLowerCase().includes(f));
    const c=this.sort.col, dir=this.sort.dir;
    const key=r=> c===3? r[3].length : c===4? r[4] : c===5? (this.isEdited(r[0])?1:0) : String(r[c]).toLowerCase();
    rows=rows.slice().sort((a,b)=>{ const x=key(a), y=key(b); return (x<y?-1:x>y?1:0)*dir; });
    this.el.querySelector('#edCount').textContent=rows.length+(rows.length===1?' record':' records');
    const heads=['ID','Name','Model','Defined in','Placed','Edited'];
    let h='<table class="edT"><thead><tr>'+heads.map((t,k)=>
      '<th data-col="'+k+'">'+t+(k===c? (dir>0?' ▲':' ▼') : '')+'</th>').join('')+'</tr></thead><tbody>';
    for(const r of rows.slice(0,this.ROW_CAP)){
      const defs=r[3].map(i=>this.plugins[i]||'?');
      const ed=this.isEdited(r[0]);
      h+='<tr class="'+(ed?'edited':'')+'" data-id="'+escHtml(r[0])+'">'+
         '<td>'+escHtml(r[0])+'</td><td>'+escHtml(r[1])+'</td><td class="from">'+escHtml(r[2])+'</td>'+
         '<td title="'+escHtml(defs.join(' > '))+'">'+escHtml(defs[defs.length-1]||'')+(defs.length>1? ' <span class="from">+'+(defs.length-1)+'</span>' : '')+'</td>'+
         '<td class="num">'+r[4]+'</td><td>'+(ed?'✓':'')+'</td></tr>';
    }
    h+='</tbody></table>';
    if(rows.length>this.ROW_CAP) h+='<div class="hint">'+(rows.length-this.ROW_CAP)+' more - type in the filter to narrow the list.</div>';
    box.innerHTML=h;
    box.querySelectorAll('th').forEach(th=>th.onclick=()=>{
      const k=+th.dataset.col; this.sort={col:k, dir:this.sort.col===k? -this.sort.dir : 1}; this.drawRows();
    });
    box.querySelectorAll('tr[data-id]').forEach(tr=>{
      tr.onclick=()=>{ box.querySelectorAll('tr.sel').forEach(x=>x.classList.remove('sel')); tr.classList.add('sel'); };
      tr.ondblclick=()=>{
        const r=this.rows.find(x=>x[0]===tr.dataset.id);
        this.openRecord(this.tag, tr.dataset.id, r? r[3].map(i=>this.plugins[i]).filter(Boolean) : null);
      };
    });
  },

  isEdited(id){ return this.edited.has(this.tag+':'+String(id).toLowerCase()); },

  /* ---- the cell view ---------------------------------------------------------------- */

  async loadCells(){
    try{
      const c=await Engine.call('cells',{});
      const ext=(c.grid||[]).map(g=>({kind:'ext', x:g[0], y:g[1], name:g[2]||'', label:(g[2]? g[2]+' ' : 'Wilderness ')+'('+g[0]+', '+g[1]+')'}));
      ext.sort((a,b)=>a.x-b.x || a.y-b.y);
      const int=(c.interiors||[]).map(r=>({kind:'int', name:r[0], label:r[0], refs:r[1]}));
      this.cells=[...int, ...ext];
    }catch(e){ toast(String(e.message||e),'err',5000); this.cells=[]; }
    this.drawCells();
  },

  drawCells(){
    const box=this.el.querySelector('#edCellList');
    const f=this.cellFilter.trim().toLowerCase();
    let list=this.cells||[];
    if(f) list=list.filter(c=>c.label.toLowerCase().includes(f));
    let h='<table class="edT"><thead><tr><th>Cell</th></tr></thead><tbody>';
    for(const c of list.slice(0,this.ROW_CAP)){
      const k=c.kind==='int'? 'int:'+c.name : c.x+','+c.y;
      h+='<tr data-cell="'+escHtml(k)+'"'+(this.cellSel===k?' class="sel"':'')+'><td>'+escHtml(c.label)+'</td></tr>';
    }
    h+='</tbody></table>';
    if(list.length>this.ROW_CAP) h+='<div class="hint">'+(list.length-this.ROW_CAP)+' more - filter to narrow.</div>';
    box.innerHTML=h;
    box.querySelectorAll('tr[data-cell]').forEach(tr=>{
      tr.onclick=()=>{ this.cellSel=tr.dataset.cell; box.querySelectorAll('tr.sel').forEach(x=>x.classList.remove('sel')); tr.classList.add('sel'); this.loadRefs(tr.dataset.cell); };
      tr.ondblclick=()=>{ if(typeof WgNav==='object') WgNav.go(tr.dataset.cell); };
    });
  },

  async loadRefs(cell){
    const box=this.el.querySelector('#edRefList');
    box.innerHTML='<div class="hint">…</div>';
    let r;
    try{ r=await Engine.call('editor_cell_refs',{cell}); }
    catch(e){ box.innerHTML='<div class="hint">'+escHtml(String(e.message||e))+'</div>'; return; }
    this.refs=r.rows||[];
    const refCell=r.refCell||'', cellPlugins=r.plugins||null;
    this.el.querySelector('#edCellName').textContent=(r.name||cell)+' - '+this.refs.length+' references';
    let h='<table class="edT"><thead><tr><th>Object</th><th>Type</th><th>From</th></tr></thead><tbody>';
    this.refs.forEach((x,i)=>{
      h+='<tr data-i="'+i+'"><td>'+escHtml(x[1])+'</td><td>'+escHtml(this.NAMES[x[2]]||x[2])+'</td><td class="from">'+escHtml(x[4])+'</td></tr>';
    });
    box.innerHTML=h+'</tbody></table>';
    box.querySelectorAll('tr[data-i]').forEach(tr=>{
      const x=this.refs[+tr.dataset.i];
      tr.title='Double-click: go to it in the render window. Right-click: edit this reference (Shift: its base record)';
      tr.ondblclick=()=>this.goToRef(cell, x);
      tr.oncontextmenu=e=>{
        e.preventDefault();
        const at=x[0].lastIndexOf(':');
        if(e.shiftKey || at<0 || !refCell) this.openRecord(x[2], x[1], null);
        else this.openRef({cell:refCell, origin:x[0].slice(0,at), refr:+x[0].slice(at+1), plugins:cellPlugins});
      };
    });
  },

  /** Opens the reference's cell, framed on it, and selects it (the inspector opens). */
  goToRef(cell, x){
    const target = cell.startsWith('int:')
      ? {kind:'int', name:cell.slice(4), label:cell.slice(4)}
      : (()=>{ const [a,b]=cell.split(',').map(Number); return {kind:'ext', x:a, y:b, name:'', label:a+', '+b}; })();
    App.mode='cell'; App.cellSel=target;
    App._doorAim={pos:x[3].slice(), rot:0, key:target.kind==='int'? 'i:'+target.name : target.x+','+target.y};
    App._findOri=x[0];
    if(typeof syncCellButton==='function') syncCellButton();
    schedulePreview();
  },

  /* ---- the record dialog ------------------------------------------------------------ */

  /** The inspector's "Edit record" (and F2): the clicked object's base record. */
  decorateOri(body, r){
    if(!this.on || !body || !r) return;
    const defs=r.definedIn||[];
    const tag=defs.length? defs[defs.length-1].tag : '';
    if(!tag || !r.id) return;
    const row=document.createElement('div');
    row.className='orirow';
    const ref=(r.refCell && r.createdBy && r.refIndex!=null)
      ? {cell:r.refCell, origin:r.createdBy, refr:r.refIndex|0, plugins:r.cellPlugins||null} : null;
    row.innerHTML='<span class="k"></span><span class="v"><button class="btn sm" id="edOriEdit" title="This object\'s base record in the editor (F2)">Edit record</button>'+
      (ref? ' <button class="btn sm" id="edOriRef" title="This placed object: where it stands, its owner, lock and the rest (F3)">Edit reference</button>' : '')+'</span>';
    body.insertBefore(row, body.firstChild);
    row.querySelector('#edOriEdit').onclick=()=>this.openRecord(tag, r.id, defs.map(d=>d.plugin));
    if(ref) row.querySelector('#edOriRef').onclick=()=>this.openRef(ref);
    this._oriRecord={tag, id:r.id, plugins:defs.map(d=>d.plugin)};
    this._oriRef=ref;
  },

  async ask(link, body){
    const url=this.links()[link];
    if(!url) throw new Error('Editing needs Wraithguard: open the viewer from Wraithguard (Cell Preview)');
    const t=await Engine.call('wg_post',{url, body:JSON.stringify(body)});
    return typeof t==='string' && t.length? JSON.parse(t) : t;
  },

  async openRecord(tag, id, plugins){
    this.record={tag, id, plugins:plugins||null}; this.refRec=null;
    const d=this.dialog();
    d.hidden=false;
    d.querySelector('#edDlgTitle').textContent=(this.NAMES[tag]||tag)+' - '+id;
    const body=d.querySelector('#edDlgBody');
    if(!this.canEdit()){
      body.innerHTML='<div class="hint">Editing needs Wraithguard: open this viewer from Wraithguard\'s Cell Preview, '+
        'and the record\'s fields open here, with changes going to its patch pool.</div>';
      return;
    }
    body.innerHTML='<div class="hint">Reading '+escHtml(id)+' from every plugin that defines it…</div>';
    try{ this.drawRecord(await this.ask('editRecord', this.req())); }
    catch(e){ body.innerHTML='<div class="hint">'+escHtml(String(e.message||e))+'</div>'; }
  },

  req(extra){ const r=this.record; return Object.assign({tag:r.tag, id:r.id}, r.plugins? {plugins:r.plugins} : {}, extra||{}); },

  dialog(){
    if(this.dlg) return this.dlg;
    const d=document.createElement('div');
    d.id='edDlg'; d.className='ori'; d.hidden=true;
    d.innerHTML='<div class="orihead"><b id="edDlgTitle">Record</b><span style="flex:1"></span>'+
      '<button class="btn dim ic" id="edDlgX" title="Close (Esc)">&#x2715;</button></div>'+
      '<div class="oribody" id="edDlgBody"></div>';
    ($('#vpwrap')||document.body).appendChild(d);
    d.querySelector('#edDlgX').onclick=()=>{ d.hidden=true; };
    this.dlg=d;
    return d;
  },

  /** One field's input, by what the field holds. */
  input(f){
    const cur=('queued' in f)? f.queued : f.value;
    const dis=f.editable? '' : ' disabled';
    if(Array.isArray(f.value) || (f.value && typeof f.value==='object'))
      return '<span class="from">'+(f.count!=null? f.count+' entries' : 'a group')+' - not edited here yet</span>';
    if(typeof f.value==='boolean')
      return '<input type="checkbox" data-path="'+escHtml(f.path)+'"'+(cur?' checked':'')+dis+'>';
    if(f.options && f.options.length && !String(f.kind).startsWith('flags:')){
      const opts=f.options.includes(String(cur))? f.options : [String(cur), ...f.options];
      return '<select class="fld" data-path="'+escHtml(f.path)+'"'+dis+'>'+opts.map(o=>
        '<option'+(String(o)===String(cur)?' selected':'')+'>'+escHtml(o)+'</option>').join('')+'</select>';
    }
    const num=typeof f.value==='number';
    const list=String(f.kind).startsWith('flags:') && f.options.length? ' list="edFlags_'+escHtml(f.path)+'"' : '';
    let h='<input class="fld" type="'+(num?'number':'text')+'"'+(num && Number.isInteger(f.value)? ' step="1"' : num? ' step="any"' : '')+
      ' data-path="'+escHtml(f.path)+'" value="'+escHtml(cur==null?'':String(cur))+'"'+list+dis+' spellcheck="false">';
    if(list) h+='<datalist id="edFlags_'+escHtml(f.path)+'">'+f.options.map(o=>'<option value="'+escHtml(o)+'">').join('')+'</datalist>';
    return h;
  },

  drawRecord(v){
    const d=this.dialog(), body=d.querySelector('#edDlgBody');
    this.record.id=v.id;
    d.querySelector('#edDlgTitle').textContent=(this.NAMES[v.tag]||v.tag)+' - '+v.id;
    let h='<div class="orirow"><span class="k">Defined in</span><span class="v">'+
      v.plugins.map((p,i)=>i===v.plugins.length-1? '<b>'+escHtml(p)+'</b>' : escHtml(p)).join(' &gt; ')+'</span></div>';
    if(v.whole) h+='<div class="hint">Wraithguard\'s patch takes this whole record from '+escHtml(v.whole)+'; a change here replaces that choice.</div>';
    h+='<div class="orirow"><span class="v">'+
      '<button class="btn sm" id="edShow" title="Where it stands in the world">Show in world</button> '+
      (Ori.recordLink()? '<button class="btn sm" id="edConf" title="This record in Wraithguard\'s conflict viewer">Conflicts</button> ' : '')+
      '<button class="btn sm" id="edRevertAll" title="Drop every change waiting for this record">Revert record</button></span></div>';
    h+='<table class="edT edFields"><tbody>';
    for(const f of v.fields){
      const q='queued' in f;
      h+='<tr class="'+(q?'edited':'')+'"><td class="k" title="'+escHtml(f.kind||'')+'">'+escHtml(f.path)+'</td><td>'+this.input(f)+
         (q? '<div class="from">'+escHtml(f.source==='typed'? 'was '+JSON.stringify(f.value) : 'from '+f.source)+'</div>' : '')+
         '</td><td>'+(q? '<button class="btn dim ic" data-revert="'+escHtml(f.path)+'" title="Drop this change">&#x21B6;</button>' : '')+'</td></tr>';
    }
    body.innerHTML=h+'</tbody></table>';
    body.querySelector('#edShow').onclick=()=>{ if(typeof WgNav==='object') WgNav.go('find:'+v.tag+':'+v.id); };
    const cf=body.querySelector('#edConf'); if(cf) cf.onclick=()=>Ori.openRecord(v.tag, v.id);
    body.querySelector('#edRevertAll').onclick=()=>this.change('editRevert', {});
    body.querySelectorAll('[data-revert]').forEach(b=>b.onclick=()=>this.change('editRevert', {path:b.dataset.revert}));
    body.querySelectorAll('[data-path]').forEach(el=>{
      el.onchange=()=>{
        const value = el.type==='checkbox'? el.checked : el.type==='number'? (el.value===''? '' : Number(el.value)) : el.value;
        this.change('editSet', {path:el.dataset.path, value}, el);
      };
      if(el.tagName==='INPUT' && el.type!=='checkbox') el.onkeydown=e=>{ if(e.key==='Enter') el.blur(); e.stopPropagation(); };
    });
  },

  async change(link, extra, el){
    try{
      this.drawRecord(await this.ask(link, this.req(extra)));
      await this.refreshPending();
    }catch(e){
      if(el) el.classList.add('bad');
      toast(String(e.message||e).replace(/^Wraithguard answered 400:\s*/,''),'err',6000);
    }
  },

  /* ---- the reference dialog ---------------------------------------------------------- */

  /** A placed object's dialog: `ref` is {cell, origin, refr, plugins?} - the cell as its
   *  records key it, the plugin that created the reference and its index there. */
  async openRef(ref){
    this.refRec=Object.assign({}, ref);
    const d=this.dialog();
    d.hidden=false;
    d.querySelector('#edDlgTitle').textContent='Reference - '+ref.origin+':'+ref.refr;
    const body=d.querySelector('#edDlgBody');
    if(!this.canEdit()){
      body.innerHTML='<div class="hint">Editing needs Wraithguard: open this viewer from Wraithguard\'s Cell Preview.</div>';
      return;
    }
    body.innerHTML='<div class="hint">Reading the cell from every plugin that has it…</div>';
    try{ this.drawRef(await this.ask('editRef', this.refReq())); }
    catch(e){ body.innerHTML='<div class="hint">'+escHtml(String(e.message||e))+'</div>'; }
  },

  refReq(extra){
    const r=this.refRec;
    return Object.assign({cell:r.cell, origin:r.origin, refr:r.refr}, r.plugins? {plugins:r.plugins} : {}, extra||{});
  },

  /** The labels the dialog shows, the Construction Set's names where it has them. */
  REF_LABELS:{translation:'Position', rotation:'Rotation (°)', scale:'Scale', deleted:'Deleted',
    owner:'Owner', owner_global:'Global variable', owner_faction:'Faction', owner_faction_rank:'Faction rank',
    lock_level:'Lock level', key:'Key', trap:'Trap', soul:'Soul', charge_left:'Charge', health_left:'Health',
    object_count:'Count', blocked:'Blocked', temporary:'Temporary', moved_cell:'Moved to cell', destination:'Door to'},

  drawRef(v){
    const d=this.dialog(), body=d.querySelector('#edDlgBody');
    this.refRec.cell=v.cell; this.refRec.origin=v.origin;
    if(!this.refRec.plugins) this.refRec.plugins=v.plugins;
    this.refView=v;
    d.querySelector('#edDlgTitle').textContent='Reference - '+v.id+' ('+v.origin+':'+v.refr+')';
    const F={}; for(const f of v.fields) F[f.path]=f;
    const cur=f=> ('queued' in f)? f.queued : f.value;
    const DEG=180/Math.PI;
    let h='<div class="orirow"><span class="k">Cell</span><span class="v">'+escHtml(v.cell)+'</span></div>'+
      '<div class="orirow"><span class="k">Created by</span><span class="v">'+escHtml(v.origin)+'</span></div>'+
      '<div class="orirow"><span class="k">Changed by</span><span class="v">'+
        v.plugins.map(p=>p===v.winner? '<b>'+escHtml(p)+'</b>' : escHtml(p)).join(' &gt; ')+'</span></div>'+
      '<div class="orirow"><span class="v"><button class="btn sm" id="edRefBase" title="The base record this places">Base record</button> '+
      '<button class="btn sm" id="edRefRevertAll" title="Drop every change waiting for this reference">Revert reference</button></span></div>'+
      '<div class="hint">Changes are drawn in the render window as you make them; the patch carries this one reference, keyed to '+escHtml(v.origin)+'.</div>';
    h+='<table class="edT edFields"><tbody>';
    const row=(f, inner)=>{
      const q='queued' in f;
      return '<tr class="'+(q?'edited':'')+'"><td class="k" title="'+escHtml(f.path)+'">'+escHtml(this.REF_LABELS[f.path]||f.path)+'</td><td>'+inner+
        (q? '<div class="from">was '+escHtml(JSON.stringify(f.value))+'</div>' : '')+
        '</td><td>'+(q? '<button class="btn dim ic" data-rrevert="'+escHtml(f.path)+'" title="Drop this change">&#x21B6;</button>' : '')+'</td></tr>';
    };
    for(const path of ['translation','rotation']){
      const f=F[path]; if(!f) continue;
      const vec=(cur(f)||[0,0,0]).map(n=> path==='rotation'? n*DEG : n);
      h+=row(f, ['X','Y','Z'].map((a,i)=>
        '<div class="edVec"><span class="from">'+a+'</span>'+
        '<button class="btn dim ic" data-nudge="'+path+'" data-i="'+i+'" data-s="-1" title="'+(path==='rotation'?'Turn':'Move')+' back (Shift: more)">&#x2212;</button>'+
        '<input class="fld" type="number" step="any" data-vec="'+path+'" data-i="'+i+'" value="'+(+vec[i].toFixed(path==='rotation'?2:1))+'">'+
        '<button class="btn dim ic" data-nudge="'+path+'" data-i="'+i+'" data-s="1" title="'+(path==='rotation'?'Turn':'Move')+' on (Shift: more)">+</button></div>').join(''));
    }
    for(const f of v.fields){
      if(f.path==='translation' || f.path==='rotation') continue;
      const c=cur(f), dis=f.editable? '' : ' disabled';
      let inner;
      if(f.kind==='bool') inner='<input type="checkbox" data-rpath="'+f.path+'"'+(c?' checked':'')+dis+'>';
      else if(f.kind==='door') inner=this.doorInputs(c);
      else if(f.kind==='grid') inner='<input class="fld" type="text" data-rpath="'+f.path+'" data-kind="grid" placeholder="x, y" value="'+escHtml(c? c.join(', ') : '')+'"'+dis+' spellcheck="false">';
      else {
        const num=f.kind==='int'||f.kind==='float';
        inner='<input class="fld" type="'+(num?'number':'text')+'"'+(f.kind==='int'?' step="1"':num?' step="any"':'')+
          (f.path==='scale'? ' min="0.5" max="2"' : '')+' data-rpath="'+f.path+'" value="'+escHtml(c==null? '' : String(c))+'"'+
          (f.path==='scale'? ' placeholder="1"' : '')+dis+' spellcheck="false">';
      }
      h+=row(f, inner);
    }
    body.innerHTML=h+'</tbody></table>';
    body.querySelector('#edRefBase').onclick=()=>{
      const r=(this.refs||[]).find(x=>x[1]===v.id);
      this.openRecord(r? r[2] : (this._oriRecord && this._oriRecord.id===v.id? this._oriRecord.tag : 'STAT'), v.id, null);
    };
    body.querySelector('#edRefRevertAll').onclick=()=>this.refChange('editRefRevert', {});
    body.querySelectorAll('[data-rrevert]').forEach(b=>b.onclick=()=>this.refChange('editRefRevert', {path:b.dataset.rrevert}));
    const vecOf=path=>{
      const vals=[...body.querySelectorAll('[data-vec="'+path+'"]')].map(el=>Number(el.value)||0);
      return path==='rotation'? vals.map(n=>n/DEG) : vals;
    };
    body.querySelectorAll('[data-vec]').forEach(el=>{
      el.onchange=()=>this.refChange('editRefSet', {path:el.dataset.vec, value:vecOf(el.dataset.vec)}, el);
      el.onkeydown=e=>{ if(e.key==='Enter') el.blur(); e.stopPropagation(); };
    });
    body.querySelectorAll('[data-nudge]').forEach(b=>b.onclick=e=>{
      const el=body.querySelector('[data-vec="'+b.dataset.nudge+'"][data-i="'+b.dataset.i+'"]');
      el.value=String((Number(el.value)||0)+(+b.dataset.s)*this.NUDGE*(e.shiftKey?8:1));
      el.onchange();
    });
    this.wireDoor(body, F.destination? cur(F.destination) : null);
    body.querySelectorAll('[data-rpath]').forEach(el=>{
      el.onchange=()=>{
        let value = el.type==='checkbox'? el.checked : el.value;
        if(el.dataset.kind==='grid') value = el.value.trim()? el.value.split(',').map(n=>n.trim()) : '';
        else if(el.type==='number') value = el.value===''? '' : Number(el.value);
        this.refChange('editRefSet', {path:el.dataset.rpath, value}, el);
      };
      if(el.type!=='checkbox') el.onkeydown=e=>{ if(e.key==='Enter') el.blur(); e.stopPropagation(); };
    });
  },

  /** A teleport door's destination: whether it is one, the cell (empty: the exterior),
   *  where the player lands and which way they face. */
  doorInputs(c){
    const t=c||{translation:[0,0,0], rotation:[0,0,0], cell:''};
    const face=((Math.atan2(Math.sin(t.rotation[2]), Math.cos(t.rotation[2]))*180/Math.PI)+360)%360;
    const rooms=(this.cells||[]).filter(x=>x.kind==='int').slice(0,4000);
    const on=!!c;
    return '<label><input type="checkbox" data-door="on"'+(on?' checked':'')+'> Teleport</label>'+
      '<div data-door-box'+(on?'':' hidden')+'>'+
      '<input class="fld" type="text" data-door="cell" list="edRooms" placeholder="Cell (empty: the exterior)" value="'+escHtml(t.cell||'')+'" spellcheck="false">'+
      '<datalist id="edRooms">'+rooms.map(r=>'<option value="'+escHtml(r.name)+'">').join('')+'</datalist>'+
      ['X','Y','Z'].map((a,i)=>'<div class="edVec"><span class="from">'+a+'</span><input class="fld" type="number" step="any" data-door="p" data-i="'+i+'" value="'+(+(+t.translation[i]).toFixed(1))+'"></div>').join('')+
      '<div class="edVec"><span class="from" title="Which way the player faces, degrees clockwise from north">⦟</span><input class="fld" type="number" step="any" data-door="face" value="'+(+face.toFixed(1))+'"></div>'+
      '<div class="edVec"><button class="btn sm" data-door="view" title="The view\'s pivot, facing the way the view looks, in the cell on screen">From the view</button>'+
      '<button class="btn sm" data-door="go" title="Go to where it leads">Go there</button></div></div>';
  },

  /** The destination's inputs: any change sends the whole destination. */
  wireDoor(body, cur){
    const q=sel=>body.querySelector(sel);
    const on=q('[data-door="on"]'); if(!on) return;
    const rot=(cur && cur.rotation)? cur.rotation.slice() : [0,0,0];
    const read=()=>{
      const p=[...body.querySelectorAll('[data-door="p"]')].map(el=>Number(el.value)||0);
      const face=(Number(q('[data-door="face"]').value)||0)*Math.PI/180;
      return {translation:p, rotation:[rot[0], rot[1], face], cell:q('[data-door="cell"]').value.trim()};
    };
    const send=v=>this.refChange('editRefSet', {path:'destination', value:v}, on);
    on.onchange=()=>{
      if(!on.checked){ send(null); return; }
      q('[data-door-box]').hidden=false;
      send(read());
    };
    body.querySelectorAll('input[data-door]:not([data-door="on"])').forEach(el=>{
      el.onchange=()=>send(read());
      el.onkeydown=e=>{ if(e.key==='Enter') el.blur(); e.stopPropagation(); };
    });
    const view=q('[data-door="view"]');
    if(view) view.onclick=()=>{
      const R=App.R, sc=App._scene;
      if(!R || !R.cam || !sc || !sc.origin) return toast('Open a cell first','warn',3000);
      const c=R.cam, look=[-Math.cos(c.az), -Math.sin(c.az)];
      const sel=App.cellSel||{};
      send({translation:[c.tx+sc.origin[0], c.ty+sc.origin[1], c.tz], rotation:[0,0,Math.atan2(look[0], look[1])],
            cell:sel.kind==='int'? (sel.name||'') : ''});
    };
    const go=q('[data-door="go"]');
    if(go) go.onclick=()=>{
      const d=read(), target=typeof doorTarget==='function'? doorTarget({pos:d.translation, rot:d.rotation, cell:d.cell}) : null;
      if(!target) return toast('No such cell in the loaded world','warn',3000);
      App.mode='cell'; App.cellSel=target;
      App._doorAim={pos:d.translation.slice(), rot:d.rotation[2],
                    key:target.kind==='int'? 'i:'+(target.name||'') : target.x+','+target.y};
      if(typeof syncCellButton==='function') syncCellButton();
      schedulePreview();
    };
  },

  async refChange(link, extra, el){
    try{
      this.drawRef(await this.ask(link, this.refReq(extra)));
      await this.refreshPending();
    }catch(e){
      if(el) el.classList.add('bad');
      toast(String(e.message||e).replace(/^Wraithguard answered 400:\s*/,''),'err',6000);
    }
  },

  /* ---- moving the selected object in the render window -------------------------------- */

  /** The selected object, when it is a reference the editor can change: the inspector's
   *  pick and its {cell, origin, refr, plugins}. */
  selected(){
    const hit=typeof Ori==='object'? Ori._hit : null, ref=this._oriRef;
    if(!this.on || !this.canEdit() || !hit || !ref || !Ori.el || Ori.el.hidden) return null;
    if(String(hit.refKey).toLowerCase()!==(ref.origin+':'+ref.refr).toLowerCase() || !hit.m || !hit.wpos) return null;
    return {hit, ref};
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
    const sel=this.selected(), R=App.R;
    if(!sel || !R || e.altKey || e.ctrlKey || e.metaKey) return null;
    if(R.pickAt(e.clientX,e.clientY)!==sel.hit) return null;
    const hit=sel.hit, m=hit.m;
    const local=[m[3], m[7], m[11]];
    const off=[hit.wpos[0]-local[0], hit.wpos[1]-local[1], hit.wpos[2]-local[2]];
    const st={sel, off, cur:local.slice(), rot:(hit.rot||[0,0,0]).slice(), scale:hit.scale||1,
              mode:null, base:null, rotBase:null, x0:0, y0:0, p0:null, moved:false, turned:false};
    const modeOf=ev=> ev.shiftKey? 'turn'+(this.held.has('x')? 0 : this.held.has('y')? 1 : 2)
      : this.held.has('z')? 'lift' : this.held.has('x')? 'x' : this.held.has('y')? 'y' : 'slide';
    const rebase=(ev, mode)=>{
      st.mode=mode; st.base=st.cur.slice(); st.rotBase=st.rot.slice();
      st.x0=ev.clientX; st.y0=ev.clientY;
      st.p0=this.onPlane(R.rayAt(ev.clientX,ev.clientY), st.base[2]);
    };
    const ghost=()=>R.setStaticHighlight([Object.assign({}, hit, {m:refMatrix(st.cur, st.rot, st.scale)})]);
    rebase(e, modeOf(e));       // from the press, so the first few pixels count
    return {
      move:ev=>{
        const mode=modeOf(ev);
        if(mode!==st.mode) rebase(ev, mode);
        if(mode.startsWith('turn')){
          const k=+mode.slice(4);
          st.rot[k]=st.rotBase[k]+(ev.clientX-st.x0)*this.TURN;
          st.turned=true;
        } else if(mode==='lift'){
          const ray=R.rayAt(ev.clientX,ev.clientY);
          const dist=Math.hypot(ray.eye[0]-st.base[0], ray.eye[1]-st.base[1], ray.eye[2]-st.base[2]);
          const H=(R.cv && R.cv.clientHeight)||800;
          const perPx=2*Math.tan((typeof FOV==='number'? FOV : 1)/2)*dist/H;
          st.cur=[st.base[0], st.base[1], st.base[2]-(ev.clientY-st.y0)*perPx];
          st.moved=true;
        } else {
          const p=this.onPlane(R.rayAt(ev.clientX,ev.clientY), st.base[2]);
          if(!p || !st.p0) return;
          let dx=p[0]-st.p0[0], dy=p[1]-st.p0[1];
          if(mode==='x') dy=0; else if(mode==='y') dx=0;
          st.cur=[st.base[0]+dx, st.base[1]+dy, st.base[2]];
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

  /** Queue changes to a reference (`[[path, value], ...]`), one after another; the dialog,
   *  when it shows that reference, and the render window follow. */
  async sendRef(ref, changes){
    if(!changes.length) return;
    let v=null;
    try{
      for(const [path, value] of changes)
        v=await this.ask('editRefSet', Object.assign({cell:ref.cell, origin:ref.origin, refr:ref.refr},
                                                     ref.plugins? {plugins:ref.plugins} : {}, {path, value}));
    }catch(e){
      toast(String(e.message||e).replace(/^Wraithguard answered 400:\s*/,''),'err',6000);
    }
    const same=this.refRec && this.dlg && !this.dlg.hidden &&
      String(this.refRec.origin).toLowerCase()===String(ref.origin).toLowerCase() && this.refRec.refr===ref.refr;
    if(v && same) this.drawRef(v);
    if(!this.refRec || !same) this.refRec=Object.assign({}, ref);
    await this.refreshPending();
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

  /* ---- the render window, as the pending changes leave it ------------------------------ */

  /** A loaded cell with the pending reference changes applied (CellData.live). The same
   *  record when none touches it; else a copy - moved, turned, scaled, deleted ones gone. */
  overlay(rec){
    if(!this.live.size || !rec.refs || !rec.refs.some(r=>this.live.has(String(r.key).toLowerCase()))) return rec;
    const refs=[];
    for(const r of rec.refs){
      const c=this.live.get(String(r.key).toLowerCase());
      if(!c){ refs.push(r); continue; }
      if(c.deleted) continue;
      const n=Object.assign({}, r);
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
    return Object.assign({}, rec, {refs});
  },

  /** The pending reference changes, from the pool's list; redraws the render window
   *  when they changed, the camera kept and the edited object still selected. */
  setLive(list){
    const live=new Map();
    for(const p of list) if(p.ref){
      const c={}; for(const ch of p.changes) if('value' in ch) c[ch.path]=ch.value;
      live.set((p.ref.origin+':'+p.ref.refr).toLowerCase(), c);
    }
    const sig=JSON.stringify([...live].sort((a,b)=>a[0]<b[0]?-1:a[0]>b[0]?1:0));
    if(sig===this.liveSig) return false;
    this.live=live; this.liveSig=sig;
    if(App.R && App.R.cam) App._camAfter=Object.assign({}, App.R.cam);
    if(this.refRec) App._findOri=(this.refRec.origin+':'+this.refRec.refr).toLowerCase();
    if(typeof schedulePreview==='function' && App.cellSel) schedulePreview();
    return true;
  },

  /* ---- what waits in the pool -------------------------------------------------------- */

  async refreshPending(){
    if(!this.canEdit()) return [];
    let list=[];
    try{ list=await this.ask('editPending', {}); }catch(_){ return []; }
    this.edited=new Set(list.filter(p=>p.tag).map(p=>p.tag+':'+String(p.id).toLowerCase()));
    this.setLive(list);
    const b=this.el && this.el.querySelector('#edPendBtn');
    if(b) b.textContent=list.length? 'Pending ('+list.length+')' : 'Pending';
    if(this.el) this.drawRows();
    if(this.pend && !this.pend.hidden) this.drawPending(list);
    return list;
  },

  async showPending(){
    if(!this.pend){
      const d=document.createElement('div');
      d.id='edPend'; d.className='ori'; d.hidden=true;
      d.innerHTML='<div class="orihead"><b>Pending changes</b><span style="flex:1"></span>'+
        '<button class="btn dim ic" id="edPendX" title="Close">&#x2715;</button></div><div class="oribody" id="edPendBody"></div>';
      ($('#vpwrap')||document.body).appendChild(d);
      d.querySelector('#edPendX').onclick=()=>{ d.hidden=true; };
      this.pend=d;
    }
    this.pend.hidden=false;
    if(!this.canEdit()){
      this.pend.querySelector('#edPendBody').innerHTML='<div class="hint">Editing needs Wraithguard.</div>';
      return;
    }
    this.drawPending(await this.refreshPending());
  },

  drawPending(list){
    const body=this.pend.querySelector('#edPendBody');
    let h='<div class="hint">Wraithguard\'s patch pool: these changes and the conflict viewer\'s choices, written together from its Patch Builder. '+
      'Saved as you go - a crash loses none of them.</div>';
    if(!list.length) h+='<div class="hint">Nothing waiting.</div>';
    for(const p of list){
      h+='<div class="orisec">'+escHtml((this.NAMES[p.tag]||p.tag||p.type)+' - '+p.id)+'</div>';
      if(p.whole) h+='<div class="orirow"><span class="v">the whole record from '+escHtml(p.whole)+'</span></div>';
      for(const c of p.changes)
        h+='<div class="orirow"><span class="k">'+escHtml(c.path)+'</span><span class="v">'+
          escHtml('value' in c? JSON.stringify(c.value) : 'from '+c.plugin)+'</span></div>';
      if(p.tag) h+='<div class="orirow"><span class="v"><button class="btn sm" data-open="'+escHtml(p.tag)+'" data-id="'+escHtml(p.id)+'">Open</button></span></div>';
      else if(p.ref) h+='<div class="orirow"><span class="v"><button class="btn sm" data-ref="'+escHtml(JSON.stringify(p.ref))+'">Open</button></span></div>';
    }
    h+='<div class="orirow" style="margin-top:8px"><span class="v"><button class="btn sm" id="edReview" title="Open the Patch Builder in Wraithguard, to review and write the patch">Review and write in Wraithguard</button></span></div>';
    body.innerHTML=h;
    body.querySelectorAll('[data-open]').forEach(b=>b.onclick=()=>this.openRecord(b.dataset.open, b.dataset.id, null));
    body.querySelectorAll('[data-ref]').forEach(b=>b.onclick=()=>this.openRef(JSON.parse(b.dataset.ref)));
    body.querySelector('#edReview').onclick=async()=>{
      try{ await Engine.call('wg_post',{url:this.links().editReview, body:'{}'}); toast('The Patch Builder is open in Wraithguard','ok',3000); }
      catch(e){ toast(String(e.message||e),'err',5000); }
    };
  },

  /* ---- keys ----------------------------------------------------------------------- */

  keys(e){
    if(!this.on) return false;
    const typing=/^(INPUT|TEXTAREA|SELECT)$/.test((document.activeElement||{}).tagName||'');
    const k=String(e.key||'').toLowerCase();
    if(!typing && (k==='z'||k==='x'||k==='y') && !e.ctrlKey && !e.metaKey) this.held.add(k);
    if(k==='f' && !typing && !e.ctrlKey && !e.metaKey && !e.altKey && this.selected()) return this.drop();
    if((e.ctrlKey||e.metaKey) && e.key.toLowerCase()==='f'){
      const f=this.el && this.el.querySelector('#edFilter'); if(f){ f.focus(); f.select(); }
      return true;
    }
    if(e.key==='F2' && !typing){
      const r=this._oriRecord;
      if(r && Ori.el && !Ori.el.hidden){ this.openRecord(r.tag, r.id, r.plugins); return true; }
    }
    if(e.key==='F3' && !typing){
      const r=this._oriRef;
      if(r && Ori.el && !Ori.el.hidden){ this.openRef(r); return true; }
    }
    if(e.key==='Escape' && this.dlg && !this.dlg.hidden && !typing){ this.dlg.hidden=true; return true; }
    return false;
  },
};

document.addEventListener('keydown', e=>{ if(WgEditor.keys(e)){ e.preventDefault(); e.stopPropagation(); } }, true);
document.addEventListener('keyup', e=>WgEditor.held.delete(String(e.key||'').toLowerCase()), true);
window.addEventListener('blur', ()=>WgEditor.held.clear());
