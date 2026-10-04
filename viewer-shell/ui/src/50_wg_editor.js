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
   - The inspector (24_ori.js) gets "Edit record" (F2) for whatever is clicked.

   The lists come from the shell (`editor_tags`, `editor_records`, `editor_cell_refs`,
   `cells`); a record's contents and every change from Wraithguard, over the `links` the
   launch handed over (`editRecord`, `editSet`, `editRevert`, `editPending`,
   `editReview`; gui/editorlink.py). Opened without Wraithguard, it browses but cannot
   edit, and says so.

   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
const WgEditor={
  on:false, el:null, dlg:null, pend:null,
  tag:'STAT', tags:[], rows:[], plugins:[], filter:'', sort:{col:0, dir:1},
  cells:null, cellFilter:'', cellSel:null, refs:[],
  edited:new Set(),          // "TAG:id lower" with changes waiting in the pool
  record:null,               // the dialog's {tag, id, plugins}
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
    this.el.querySelector('#edCellName').textContent=(r.name||cell)+' - '+this.refs.length+' references';
    let h='<table class="edT"><thead><tr><th>Object</th><th>Type</th><th>From</th></tr></thead><tbody>';
    this.refs.forEach((x,i)=>{
      h+='<tr data-i="'+i+'"><td>'+escHtml(x[1])+'</td><td>'+escHtml(this.NAMES[x[2]]||x[2])+'</td><td class="from">'+escHtml(x[4])+'</td></tr>';
    });
    box.innerHTML=h+'</tbody></table>';
    box.querySelectorAll('tr[data-i]').forEach(tr=>{
      const x=this.refs[+tr.dataset.i];
      tr.title='Double-click: go to it in the render window. Right-click: edit its base record';
      tr.ondblclick=()=>this.goToRef(cell, x);
      tr.oncontextmenu=e=>{ e.preventDefault(); this.openRecord(x[2], x[1], null); };
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
    row.innerHTML='<span class="k"></span><span class="v"><button class="btn sm" id="edOriEdit" title="This object\'s base record in the editor (F2)">Edit record</button></span>';
    body.insertBefore(row, body.firstChild);
    row.querySelector('#edOriEdit').onclick=()=>this.openRecord(tag, r.id, defs.map(d=>d.plugin));
    this._oriRecord={tag, id:r.id, plugins:defs.map(d=>d.plugin)};
  },

  async ask(link, body){
    const url=this.links()[link];
    if(!url) throw new Error('Editing needs Wraithguard: open the viewer from Wraithguard (Cell Preview)');
    const t=await Engine.call('wg_post',{url, body:JSON.stringify(body)});
    return typeof t==='string' && t.length? JSON.parse(t) : t;
  },

  async openRecord(tag, id, plugins){
    this.record={tag, id, plugins:plugins||null};
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

  /* ---- what waits in the pool -------------------------------------------------------- */

  async refreshPending(){
    if(!this.canEdit()) return [];
    let list=[];
    try{ list=await this.ask('editPending', {}); }catch(_){ return []; }
    this.edited=new Set(list.map(p=>p.tag+':'+String(p.id).toLowerCase()));
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
    }
    h+='<div class="orirow" style="margin-top:8px"><span class="v"><button class="btn sm" id="edReview" title="Open the Patch Builder in Wraithguard, to review and write the patch">Review and write in Wraithguard</button></span></div>';
    body.innerHTML=h;
    body.querySelectorAll('[data-open]').forEach(b=>b.onclick=()=>this.openRecord(b.dataset.open, b.dataset.id, null));
    body.querySelector('#edReview').onclick=async()=>{
      try{ await Engine.call('wg_post',{url:this.links().editReview, body:'{}'}); toast('The Patch Builder is open in Wraithguard','ok',3000); }
      catch(e){ toast(String(e.message||e),'err',5000); }
    };
  },

  /* ---- keys ----------------------------------------------------------------------- */

  keys(e){
    if(!this.on) return false;
    const typing=/^(INPUT|TEXTAREA|SELECT)$/.test((document.activeElement||{}).tagName||'');
    if((e.ctrlKey||e.metaKey) && e.key.toLowerCase()==='f'){
      const f=this.el && this.el.querySelector('#edFilter'); if(f){ f.focus(); f.select(); }
      return true;
    }
    if(e.key==='F2' && !typing){
      const r=this._oriRecord;
      if(r && Ori.el && !Ori.el.hidden){ this.openRecord(r.tag, r.id, r.plugins); return true; }
    }
    if(e.key==='Escape' && this.dlg && !this.dlg.hidden && !typing){ this.dlg.hidden=true; return true; }
    return false;
  },
};

document.addEventListener('keydown', e=>{ if(WgEditor.keys(e)){ e.preventDefault(); e.stopPropagation(); } }, true);
