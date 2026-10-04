/* =====================================================================================
   The Editor mode - Wraithguard. The Construction Set's working windows around the
   render window: a mode of its own, not the Cell Preview's settings.

   - The Object Window: every record of the load order by type (tabs), with a filter
     (Ctrl+F), sortable columns - id, name, model, the plugins that define it (the last
     wins) and how many are placed - and the records with changes waiting marked.
     Double-click opens one.
   - "Patch only" (the active file, as the Construction Set shows the plugin it edits):
     the Object Window narrowed to the records the patch changes or makes, the tabs
     counting them, and the Cell View marking the cells whose references it changes.
   - The Cell View: every cell, and the references in the one selected. Double-click a
     cell to open it in the render window; a reference to go to it, framed and selected.
   - The record dialog: each field (the conflict viewer's dotted paths), its value in
     the winning plugin, and an input that sends a change. Its Use Report lists every
     record of the load order that names it (live, or in a version a later mod
     overrides), and the cells it is placed in; "Replace with" repoints the live uses to
     another record (Search & Replace) - fields, and placed references (their key kept,
     the object changed) - scripts excepted. Changes go to Wraithguard's
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
   - The Script Edit window (a script's dialog, "Script Edit"): the source, checked as it
     is typed (begin/end, blocks, variables, functions the game knows), the compiled
     listing, and "Save to the patch". The toolkit does not compile: OpenMW compiles the
     text, Morrowind.exe runs the compiled data the record carries, which a changed text
     leaves as it was - the window says so.
   - The dialogue window (the dock's "Dialogue"): every topic by kind, and a topic's
     responses in the order the engine reads them (who says each, the plugin whose
     version wins, a response that has lost its place marked); a response opens in the
     record dialog, its changes written inside its topic.
   - Q on the selected object: a menu of what can be done to it (edit, Use Report, drop,
     duplicate, delete, hide, move to a layer). Layers (the dock's "Layers") are named
     groups of references shown or hidden in the render window - a view, kept in this
     viewer, never written to the patch.
   - Placing: drag a record from the Object Window into the render window (or right-click
     it: at the view's pivot) for a new reference, the patch's own (`editPlace`); it opens
     in the reference dialog, moves like any other, and is drawn with the rest.

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
  made:[],                   // records the patch makes ("Make a copy as"), from the pool
  patchOnly:false,           // the Object Window shows only what the patch changes or makes
  editedCells:new Set(),     // cell keys (lower) whose references the patch changes or adds
  record:null,               // the dialog's {tag, id, plugins}
  refRec:null,               // or, for a placed object, {cell, origin, refr, plugins}
  live:new Map(),            // "origin:refr" lower -> pending changes, drawn in place
  liveNew:new Map(),         // cell key lower -> [{uid, tag, fields}]: new references, drawn
  _sel:null,                 // the selected reference: {hit, ref}, ref {cell, origin, refr} or {cell, uid}
  _liveSig:'[[],[]]',        // changes when `live` does: part of the preview's scene key
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

  /** Part of the preview's scene key: the pending changes, and what the layers hide. */
  get liveSig(){ return this._liveSig+'|'+this.layerSig(); },

  /** A reference's key as the render window names it: `origin:refr`, or `new:uid`. */
  refKeyOf(ref){ return ref.uid? 'new:'+ref.uid : (ref.origin+':'+ref.refr).toLowerCase(); },
  /** What names a reference to Wraithguard. */
  refBody(ref){
    return ref.uid? {cell:ref.cell, uid:ref.uid}
      : Object.assign({cell:ref.cell, origin:ref.origin, refr:ref.refr}, ref.plugins? {plugins:ref.plugins} : {});
  },

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
    this.hookDrop();
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
          '<button class="btn sm" id="edDialBtn" title="Topics and their responses, in the order the engine reads them">Dialogue</button> '+
          '<button class="btn sm" id="edLayersBtn" title="Groups of references shown or hidden in the render window">Layers</button> '+
          '<button class="btn sm" id="edPendBtn" title="The changes waiting in Wraithguard\'s patch pool">Pending</button></div>'+
        '<div class="edTabs" id="edTabs"></div>'+
        '<div class="edBar"><input class="fld" id="edFilter" placeholder="Filter: id, name or model (Ctrl+F)" spellcheck="false">'+
          '<label class="from" title="Only the records the patch changes or makes - the active file"><input type="checkbox" id="edPatchOnly"> Patch only</label>'+
          '<input class="fld" id="edNewId" placeholder="New id" spellcheck="false" maxlength="31" style="width:110px;margin-left:6px">'+
          '<button class="btn sm" id="edNew" title="A blank record of this type under that id, made by the patch (Insert)">New</button>'+
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
    d.querySelector('#edPatchOnly').onchange=e=>{ this.patchOnly=e.target.checked; this.drawRows(); };
    d.querySelector('#edNewId').onkeydown=e=>{ if(e.key==='Enter') this.insertRecord(); e.stopPropagation(); };
    d.querySelector('#edNew').onclick=()=>this.insertRecord();
    d.querySelector('#edCellFilter').oninput=e=>{ this.cellFilter=e.target.value; this.drawCells(); };
    d.querySelector('#edPendBtn').onclick=()=>this.showPending();
    d.querySelector('#edLayersBtn').onclick=()=>this.showLayers();
    d.querySelector('#edDialBtn').onclick=()=>this.showDialogue();
  },

  fillTabs(){
    const t=this.el.querySelector('#edTabs');
    const changed={};
    for(const k of this.edited){ const tag=k.slice(0,4); changed[tag]=(changed[tag]||0)+1; }
    t.innerHTML=this.tags.map(([tag,n])=>
      '<button class="btn sm'+(tag===this.tag?' on':'')+(changed[tag]?' edited':'')+'" data-tag="'+escHtml(tag)+'" title="'+escHtml(tag)+' - '+n+' records'+(changed[tag]? ', '+changed[tag]+' in the patch' : '')+'">'+
      escHtml(this.NAMES[tag]||tag)+(changed[tag]? ' <span class="from">'+changed[tag]+'</span>' : '')+'</button>').join('');
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
    // The patch's own records of this type, with the load order's.
    const mine=this.made.filter(p=>p.tag===this.tag).map(p=>[p.id, p.made.name||'', p.made.mesh||'', [], 0]);
    let rows=mine.length? this.rows.concat(mine) : this.rows;
    if(this.patchOnly) rows=rows.filter(r=>this.isEdited(r[0]));
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
      const row=()=>this.rows.find(x=>x[0]===tr.dataset.id)
        || ((m=>m? [m.id, m.made.name||'', m.made.mesh||'', [], 0] : null)(this.made.find(p=>p.tag===this.tag && p.id===tr.dataset.id)));
      const placing=()=>{ const r=row(); return r? {tag:this.tag, id:r[0], model:r[2], defined:r[3].map(i=>this.plugins[i]).filter(Boolean)} : null; };
      tr.draggable=true;
      tr.title='Double-click: open the record. Drag into the render window, or right-click, to place one';
      tr.ondragstart=e=>{ const p=placing(); if(p && e.dataTransfer){ e.dataTransfer.setData('application/x-wg-record', JSON.stringify(p)); e.dataTransfer.effectAllowed='copy'; } };
      tr.oncontextmenu=e=>{ e.preventDefault(); const p=placing(); if(p) this.placeAt(p, null, null); };
      tr.onclick=()=>{ box.querySelectorAll('tr.sel').forEach(x=>x.classList.remove('sel')); tr.classList.add('sel'); };
      tr.ondblclick=()=>{
        const r=this.rows.find(x=>x[0]===tr.dataset.id);
        this.openRecord(this.tag, tr.dataset.id, r? r[3].map(i=>this.plugins[i]).filter(Boolean) : null);
      };
    });
  },

  isEdited(id){ return this.edited.has(this.tag+':'+String(id).toLowerCase()); },

  /** "New": a blank record of the tab's type, under the id typed, made by the patch. */
  async insertRecord(){
    const box=this.el.querySelector('#edNewId'), newId=box.value.trim();
    if(!newId){ box.classList.add('bad'); return null; }
    try{
      const v=await this.ask('editInsert', {tag:this.tag, newId});
      box.value=''; box.classList.remove('bad');
      this.record={tag:v.tag, id:v.id, plugins:null};
      this.dialog().hidden=false;
      this.drawRecord(v);
      await this.refreshPending();
      return v;
    }catch(e){
      box.classList.add('bad');
      toast(String(e.message||e).replace(/^Wraithguard answered 400:\s*/,''),'err',6000);
      return null;
    }
  },

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
      const ed=this.editedCells.has(c.kind==='int'? String(c.name).toLowerCase() : '('+c.x+', '+c.y+')');
      const cls=[this.cellSel===k? 'sel' : '', ed? 'edited' : ''].filter(Boolean).join(' ');
      h+='<tr data-cell="'+escHtml(k)+'"'+(cls? ' class="'+cls+'"' : '')+(ed? ' title="The patch changes or adds references here"' : '')+'><td>'+escHtml(c.label)+'</td></tr>';
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
    this._sel=ref? {hit:Ori._hit, ref} : null;
  },

  async ask(link, body){
    const url=this.links()[link];
    if(!url) throw new Error('Editing needs Wraithguard: open the viewer from Wraithguard (Cell Preview)');
    const t=await Engine.call('wg_post',{url, body:JSON.stringify(body)});
    if(typeof t!=='string' || !t.length) return t;
    try{ return JSON.parse(t); }catch(_){ return t; }      // `ok` and the like
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
    if(Array.isArray(f.value)) return this.listTable(f, Array.isArray(cur)? cur : f.value);
    if(f.value && typeof f.value==='object')
      return '<span class="from">a group - not edited here yet</span>';
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

  /** A list field as a table: one row per entry - an input for a value, one per column
   *  for a row (an inventory's count and item), the entry's JSON for a group (an AI
   *  package) - with a row removed by its ×, and added by "Add". Any change sends the
   *  whole list; Wraithguard checks each entry against the first. */
  listTable(f, list){
    this._lists=this._lists||{};
    const tpl=list.length? list[0] : (f.value.length? f.value[0] : null);
    this._lists[f.path]={list:JSON.parse(JSON.stringify(list)), tpl};
    const dis=f.editable? '' : ' disabled', P=escHtml(f.path);
    const cell=(v, r, c, like)=>{
      const num=typeof like==='number';
      return '<input class="fld" type="'+(num?'number':'text')+'"'+(num? (Number.isInteger(like)?' step="1"':' step="any"') : '')+
        ' data-lpath="'+P+'" data-r="'+r+'"'+(c!=null? ' data-c="'+c+'"' : '')+' value="'+escHtml(v==null? '' : (typeof v==='object'? JSON.stringify(v) : String(v)))+'"'+dis+' spellcheck="false">';
    };
    // A group per entry (a dialogue condition, an AI package): a column per field.
    const group=tpl && typeof tpl==='object' && !Array.isArray(tpl);
    const cols=group? [...new Set(list.concat([tpl]).flatMap(e=>e && typeof e==='object'? Object.keys(e) : []))] : null;
    this._lists[f.path].cols=cols;
    let h='<table class="edT edList">'+(group? '<thead><tr>'+cols.map(k=>'<th>'+escHtml(k)+'</th>').join('')+'<th></th></tr></thead>' : '')+'<tbody>';
    if(tpl===null && !list.length) h+='<tr><td class="from">empty</td></tr>';
    list.slice(0,300).forEach((e,r)=>{
      h+='<tr>';
      if(Array.isArray(tpl) && Array.isArray(e)) e.forEach((v,c)=>{ h+='<td>'+cell(v, r, c, tpl[c])+'</td>'; });
      else if(group && e && typeof e==='object') cols.forEach(k=>{
        // A field this entry's kind does not have (AI packages differ): a blank, not an input.
        h+='<td>'+(k in e? cell(e[k], r, k, k in tpl? tpl[k] : e[k]) : '')+'</td>';
      });
      else h+='<td>'+cell(e, r, null, tpl)+'</td>';
      h+='<td>'+(f.editable? '<button class="btn dim ic" data-ldel="'+P+'" data-r="'+r+'" title="Remove this entry">&#x2715;</button>' : '')+'</td></tr>';
    });
    h+='</tbody></table>';
    if(list.length>300) h+='<div class="from">'+(list.length-300)+' more entries, not shown</div>';
    if(f.editable && tpl!==null) h+='<button class="btn sm" data-ladd="'+P+'" title="Another entry, like the last">Add</button>';
    return h;
  },

  /** The list as its table's inputs now have it. */
  readList(body, path){
    const L=this._lists[path], out=JSON.parse(JSON.stringify(L.list));
    body.querySelectorAll('[data-lpath]').forEach(el=>{
      if(el.dataset.lpath!==path) return;
      const r=+el.dataset.r, c=el.dataset.c;
      let v=el.value;
      const keyed=L.cols && c!=null;
      const like=c!=null? (keyed? (c in (L.tpl||{})? L.tpl[c] : (L.list[r]||{})[c]) : (Array.isArray(L.tpl)? L.tpl[+c] : null)) : L.tpl;
      if(like && typeof like==='object'){ try{ v=JSON.parse(v); }catch(_){ } }
      else if(el.type==='number') v= v===''? '' : Number(v);
      if(keyed) out[r][c]=v; else if(c!=null) out[r][+c]=v; else out[r]=v;
    });
    return out;
  },

  wireLists(body){
    const send=(path, list)=>this.change('editSet', {path, value:list});
    body.querySelectorAll('[data-lpath]').forEach(el=>{
      el.onchange=()=>send(el.dataset.lpath, this.readList(body, el.dataset.lpath));
      el.onkeydown=e=>{ if(e.key==='Enter') el.blur(); e.stopPropagation(); };
    });
    body.querySelectorAll('[data-ldel]').forEach(b=>b.onclick=()=>{
      const list=this.readList(body, b.dataset.ldel); list.splice(+b.dataset.r, 1); send(b.dataset.ldel, list);
    });
    body.querySelectorAll('[data-ladd]').forEach(b=>b.onclick=()=>{
      const path=b.dataset.ladd, list=this.readList(body, path), L=this._lists[path];
      list.push(JSON.parse(JSON.stringify(list.length? list[list.length-1] : L.tpl)));
      send(path, list);
    });
  },

  drawRecord(v){
    const d=this.dialog(), body=d.querySelector('#edDlgBody');
    this.record.id=v.id;
    d.querySelector('#edDlgTitle').textContent=(this.NAMES[v.tag]||v.tag)+' - '+v.id;
    let h='<div class="orirow"><span class="k">Defined in</span><span class="v">'+
      v.plugins.map((p,i)=>i===v.plugins.length-1? '<b>'+escHtml(p)+'</b>' : escHtml(p)).join(' &gt; ')+'</span></div>';
    if(v.whole) h+='<div class="hint">Wraithguard\'s patch takes this whole record from '+escHtml(v.whole)+'; a change here replaces that choice.</div>';
    if(v.new) h+='<div class="hint">Made by this patch: a change here is the record itself.</div>';
    const flags=v.fields.find(f=>f.path==='flags');
    const flagsNow=flags? String(('queued' in flags)? flags.queued : flags.value||'') : null;
    const deleted=flagsNow!=null && flagsNow.split('|').some(x=>x.trim()==='DELETED');
    h+='<div class="orirow"><span class="v">'+
      (v.new? '' : '<button class="btn sm" id="edShow" title="Where it stands in the world">Show in world</button> ')+
      (v.new? '' : '<button class="btn sm" id="edUses" title="Every record of the load order that names this one, and the cells it is placed in - the Construction Set\'s Use Report">Use Report</button> ')+
      (v.tag==='SCPT'? '<button class="btn sm" id="edScriptBtn" title="The source, checked as you type, and the compiled listing">Script Edit</button> ' : '')+
      (Ori.recordLink() && !v.new? '<button class="btn sm" id="edConf" title="This record in Wraithguard\'s conflict viewer">Conflicts</button> ' : '')+
      (v.new? '<button class="btn sm" id="edRevertAll" title="Take this record back out of the patch">Remove from the patch</button>'
            : '<button class="btn sm" id="edRevertAll" title="Drop every change waiting for this record">Revert record</button>')+
      (flags && !v.new? ' <button class="btn sm" id="edDelete" title="'+(deleted? 'Take the deleted flag off again' : 'Mark the record deleted, as the Construction Set deletes one: the patch carries it with its DELETED flag')+'">'+(deleted? 'Undelete' : 'Delete record')+'</button>' : '')+
      '</span></div>'+
      '<div class="orirow"><span class="v edVec"><input class="fld" id="edCopyId" placeholder="New id" spellcheck="false" maxlength="31">'+
      '<button class="btn sm" id="edCopy" title="A copy of this record under a new id, made by the patch - the Construction Set\'s way of making a record">Make a copy as</button></span></div>';
    h+='<table class="edT edFields"><tbody>';
    for(const f of v.fields){
      const q='queued' in f;
      h+='<tr class="'+(q?'edited':'')+'"><td class="k" title="'+escHtml(f.kind||'')+'">'+escHtml(f.path)+'</td><td>'+this.input(f)+
         (q? '<div class="from">'+escHtml(f.source==='typed'? 'was '+JSON.stringify(f.value) : 'from '+f.source)+'</div>' : '')+
         '</td><td>'+(q? '<button class="btn dim ic" data-revert="'+escHtml(f.path)+'" title="Drop this change">&#x21B6;</button>' : '')+'</td></tr>';
    }
    body.innerHTML=h+'</tbody></table>';
    const sb=body.querySelector('#edScriptBtn');
    if(sb) sb.onclick=()=>this.openScript(v.tag, v.id, v.plugins);
    const us=body.querySelector('#edUses');
    if(us) us.onclick=()=>this.showUses(v.tag, v.id, v.plugins);
    const sh=body.querySelector('#edShow');
    if(sh) sh.onclick=()=>{ if(typeof WgNav==='object') WgNav.go('find:'+v.tag+':'+v.id); };
    const del=body.querySelector('#edDelete');
    if(del) del.onclick=()=>{
      const parts=flagsNow.split('|').map(x=>x.trim()).filter(x=>x && x!=='DELETED');
      if(!deleted) parts.push('DELETED');
      this.change('editSet', {path:'flags', value:parts.join(' | ')});
    };
    const cp=body.querySelector('#edCopy'), cpId=body.querySelector('#edCopyId');
    cpId.onkeydown=e=>{ if(e.key==='Enter') cp.onclick(); e.stopPropagation(); };
    cp.onclick=async()=>{
      const newId=cpId.value.trim();
      if(!newId){ cpId.classList.add('bad'); return; }
      try{
        const c=await this.ask('editDuplicate', this.req({newId}));
        this.record={tag:c.tag, id:c.id, plugins:null};
        this.drawRecord(c);
        await this.refreshPending();
        toast(c.id+' is made - drag it from the Object Window to place one','ok',4000);
      }catch(e){
        cpId.classList.add('bad');
        toast(String(e.message||e).replace(/^Wraithguard answered 400:\s*/,''),'err',6000);
      }
    };
    const cf=body.querySelector('#edConf'); if(cf) cf.onclick=()=>Ori.openRecord(v.tag, v.id);
    body.querySelector('#edRevertAll').onclick=()=>this.change('editRevert', {});
    body.querySelectorAll('[data-revert]').forEach(b=>b.onclick=()=>this.change('editRevert', {path:b.dataset.revert}));
    this.wireLists(body);
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
      const v=await this.ask(link, this.req(extra));
      // Removing a record the patch made leaves nothing to show.
      if(link==='editRevert' && !(extra&&extra.path) && this.made.some(p=>p.tag===this.record.tag && p.id.toLowerCase()===String(this.record.id).toLowerCase())){
        if(this.dlg) this.dlg.hidden=true;
        await this.refreshPending();
        return;
      }
      this.drawRecord(v);
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
    d.querySelector('#edDlgTitle').textContent=ref.uid? 'New reference' : 'Reference - '+ref.origin+':'+ref.refr;
    const body=d.querySelector('#edDlgBody');
    if(!this.canEdit()){
      body.innerHTML='<div class="hint">Editing needs Wraithguard: open this viewer from Wraithguard\'s Cell Preview.</div>';
      return;
    }
    body.innerHTML='<div class="hint">Reading the cell from every plugin that has it…</div>';
    try{ this.drawRef(await this.ask(ref.uid? 'editNew' : 'editRef', this.refReq())); }
    catch(e){ body.innerHTML='<div class="hint">'+escHtml(String(e.message||e))+'</div>'; }
  },

  refReq(extra){ return Object.assign(this.refBody(this.refRec), extra||{}); },

  /** The labels the dialog shows, the Construction Set's names where it has them. */
  REF_LABELS:{translation:'Position', rotation:'Rotation (°)', scale:'Scale', deleted:'Deleted',
    owner:'Owner', owner_global:'Global variable', owner_faction:'Faction', owner_faction_rank:'Faction rank',
    lock_level:'Lock level', key:'Key', trap:'Trap', soul:'Soul', charge_left:'Charge', health_left:'Health',
    object_count:'Count', blocked:'Blocked', temporary:'Temporary', moved_cell:'Moved to cell', destination:'Door to'},

  drawRef(v){
    const d=this.dialog(), body=d.querySelector('#edDlgBody');
    this.refRec.cell=v.cell;
    if(!v.new) this.refRec.origin=v.origin;
    if(!this.refRec.plugins) this.refRec.plugins=v.plugins;
    this.refView=v;
    d.querySelector('#edDlgTitle').textContent=v.new? 'New reference - '+v.id : 'Reference - '+v.id+' ('+v.origin+':'+v.refr+')';
    const F={}; for(const f of v.fields) F[f.path]=f;
    const cur=f=> ('queued' in f)? f.queued : f.value;
    const DEG=180/Math.PI;
    let h='<div class="orirow"><span class="k">Cell</span><span class="v">'+escHtml(v.cell)+'</span></div>';
    if(v.new) h+='<div class="orirow"><span class="k">Created by</span><span class="v">the patch (new)</span></div>'+
      '<div class="orirow"><span class="v"><button class="btn sm" id="edRefBase" title="The base record this places">Base record</button> '+
      '<button class="btn sm" id="edRefRevertAll" title="Take this new reference back out of the patch">Remove from the patch</button></span></div>'+
      '<div class="hint">A reference the patch adds: numbered on from the patch\'s own when it is written.</div>';
    else h+='<div class="orirow"><span class="k">Created by</span><span class="v">'+escHtml(v.origin)+'</span></div>'+
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
      this.openRecord(v.tag || (r? r[2] : (this._oriRecord && this._oriRecord.id===v.id? this._oriRecord.tag : 'STAT')), v.id, null);
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
      if(this.refRec && this.refRec.uid){
        if(link==='editRefRevert'){
          // A new reference has nothing to revert to: the whole of it goes.
          if(extra && extra.path) return this.refChange('editRefSet', {path:extra.path, value:null}, el);
          await this.ask('editNewRemove', this.refReq());
          this.refRec=null; this._sel=null;
          if(this.dlg) this.dlg.hidden=true;
          if(App.R) App.R.setStaticHighlight(null);
          await this.refreshPending();
          return;
        }
        link='editNewSet';
      }
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
    return out;
  },
  layerSig(){ return this.layers().filter(l=>!l.visible).map(l=>l.name+':'+l.keys.length).join(','); },

  /** The layers changed: the panel and the render window follow, the camera kept. */
  layersChanged(){
    this.saveLayers();
    if(this.lay && !this.lay.hidden) this.drawLayers();
    if(App.R && App.R.cam) App._camAfter=Object.assign({}, App.R.cam);
    if(typeof schedulePreview==='function' && App.cellSel) schedulePreview();
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
      ($('#vpwrap')||document.body).appendChild(d);
      d.querySelector('#edLayersX').onclick=()=>{ d.hidden=true; };
      this.lay=d;
    }
    this.lay.hidden=false;
    this.drawLayers();
  },

  drawLayers(){
    const body=this.lay.querySelector('#edLayersBody'), L=this.layers();
    let h='<div class="hint">Groups of references, shown or hidden in the render window. Q on an object puts it on one. '+
      'A view only: nothing here is written to the patch.</div>';
    if(!L.length) h+='<div class="hint">No layers yet.</div>';
    h+='<table class="edT"><tbody>'+L.map((l,i)=>
      '<tr><td><input type="checkbox" data-lvis="'+i+'"'+(l.visible?' checked':'')+' title="Shown"></td>'+
      '<td><input class="fld" data-lname="'+i+'" value="'+escHtml(l.name)+'" spellcheck="false"></td>'+
      '<td class="num">'+l.keys.length+'</td>'+
      '<td><button class="btn dim ic" data-ldrop="'+i+'" title="Remove the layer (its references are shown again)">&#x2715;</button></td></tr>').join('')+'</tbody></table>'+
      '<div class="orirow"><span class="v"><button class="btn sm" id="edLayerNew">New layer</button> '+
      '<button class="btn sm" id="edLayerAll" title="Every layer shown">Show all</button></span></div>';
    body.innerHTML=h;
    body.querySelectorAll('[data-lvis]').forEach(c=>c.onchange=()=>{ L[+c.dataset.lvis].visible=c.checked; this.layersChanged(); });
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

  /** Q: what can be done to the selected object, at the pointer. */
  quickMenu(x, y){
    const sel=this.selected(); if(!sel) return false;
    if(!this.qm){
      const m=document.createElement('div');
      m.id='edQ'; m.className='ori'; m.hidden=true;
      document.body.appendChild(m);
      this.qm=m;
    }
    const m=this.qm, rec=this.selectedRecord(sel), L=this.layers();
    const items=[
      ['ref', 'Edit reference (F3)'],
      rec.tag? ['record', 'Edit record (F2)'] : null,
      rec.tag? ['uses', 'Use Report'] : null,
      ['drop', 'Drop to ground (F)'],
      rec.tag? ['dup', 'Duplicate'] : null,
      ['del', sel.ref.uid? 'Remove from the patch' : 'Delete reference'],
      ['hide', 'Hide (layer "Hidden")'],
      ...L.filter(l=>l.name!=='Hidden').map((l,i)=>['layer:'+l.name, 'Move to layer: '+l.name]),
      ['newlayer', 'Move to a new layer'],
    ].filter(Boolean);
    m.innerHTML='<div class="oribody">'+items.map(([a,t])=>'<div class="it" data-q="'+escHtml(a)+'">'+escHtml(t)+'</div>').join('')+'</div>';
    const W=window.innerWidth||1200, H=window.innerHeight||800;
    m.style.left=Math.min(Math.max(0,(x==null? W/2 : x)), W-240)+'px';
    m.style.top=Math.min(Math.max(0,(y==null? H/2 : y)), H-items.length*24-20)+'px';
    m.hidden=false;
    m.querySelectorAll('[data-q]').forEach(el=>el.onclick=()=>{ m.hidden=true; this.quickAction(el.dataset.q, sel, rec); });
    return true;
  },

  async quickAction(a, sel, rec){
    const key=this.refKeyOf(sel.ref);
    if(a==='ref') return this.openRef(sel.ref);
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
    if(a==='hide'){ this.toLayer(key, 'Hidden').visible=false; this._sel=null; return this.layersChanged(); }
    if(a.startsWith('layer:')) return this.toLayer(key, a.slice(6));
    if(a==='newlayer'){
      let n=1; while(this.layers().some(l=>l.name==='Layer '+n)) n++;
      this.toLayer(key, 'Layer '+n);
      return this.showLayers();
    }
  },

  /* ---- placing new references -------------------------------------------------------- */

  /** The render window takes records dropped on it from the Object Window (once). */
  hookDrop(){
    const cv=App.R && App.R.cv;
    if(!cv || cv._wgDrop) return;
    cv._wgDrop=true;
    const ours=e=>this.on && e.dataTransfer && [...(e.dataTransfer.types||[])].includes('application/x-wg-record');
    cv.addEventListener('dragover', e=>{ if(ours(e)){ e.preventDefault(); e.dataTransfer.dropEffect='copy'; } });
    cv.addEventListener('drop', e=>{
      if(!ours(e)) return;
      e.preventDefault();
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

  /* ---- the dialogue window ------------------------------------------------------------- */

  async showDialogue(){
    if(!this.dial){
      const d=document.createElement('div');
      d.id='edDial'; d.className='ori'; d.hidden=true;
      d.innerHTML='<div class="orihead"><b id="edDialTitle">Dialogue</b><span style="flex:1"></span>'+
        '<button class="btn dim ic" id="edDialX" title="Close">&#x2715;</button></div>'+
        '<div class="oribody"><div class="edTabs" id="edDialTabs"></div>'+
        '<input class="fld" id="edDialFilter" placeholder="Filter topics" spellcheck="false">'+
        '<div class="edSplit"><div class="edTable" id="edDialTopics"></div><div class="edTable" id="edDialResp"></div></div></div>';
      ($('#vpwrap')||document.body).appendChild(d);
      d.querySelector('#edDialX').onclick=()=>{ d.hidden=true; };
      d.querySelector('#edDialFilter').oninput=()=>this.drawTopics();
      d.querySelector('#edDialFilter').onkeydown=e=>e.stopPropagation();
      this.dial=d;
    }
    this.dial.hidden=false;
    if(!this.canEdit()){ this.dial.querySelector('#edDialTopics').innerHTML='<div class="hint">The dialogue window needs Wraithguard.</div>'; return; }
    if(!this.topicList){
      this.dial.querySelector('#edDialTopics').innerHTML='<div class="hint">Reading every plugin\'s topics…</div>';
      try{ this.topicList=await this.ask('editTopics', {}); }
      catch(e){ this.dial.querySelector('#edDialTopics').innerHTML='<div class="hint">'+escHtml(String(e.message||e))+'</div>'; return; }
      const kinds=[...new Set(this.topicList.map(t=>t.type))];
      this.dialKind=kinds.includes('Topic')? 'Topic' : kinds[0]||'';
      const tabs=this.dial.querySelector('#edDialTabs');
      tabs.innerHTML=kinds.map(k=>'<button class="btn sm'+(k===this.dialKind?' on':'')+'" data-kind="'+escHtml(k)+'">'+escHtml(k||'?')+'</button>').join('');
      tabs.querySelectorAll('[data-kind]').forEach(b=>b.onclick=()=>{
        this.dialKind=b.dataset.kind; tabs.querySelectorAll('.on').forEach(x=>x.classList.remove('on')); b.classList.add('on'); this.drawTopics();
      });
    }
    this.drawTopics();
  },

  drawTopics(){
    const box=this.dial.querySelector('#edDialTopics');
    const f=this.dial.querySelector('#edDialFilter').value.trim().toLowerCase();
    const list=(this.topicList||[]).filter(t=>t.type===this.dialKind && (!f || t.id.toLowerCase().includes(f)));
    box.innerHTML='<table class="edT"><tbody>'+list.slice(0,this.ROW_CAP).map(t=>
      '<tr data-topic="'+escHtml(t.id)+'" title="'+escHtml(t.plugins.join(' > '))+'"><td>'+escHtml(t.id)+'</td><td class="from">'+t.plugins.length+'</td></tr>').join('')+'</tbody></table>'+
      (list.length>this.ROW_CAP? '<div class="hint">'+(list.length-this.ROW_CAP)+' more - filter to narrow.</div>' : '');
    box.querySelectorAll('tr[data-topic]').forEach(tr=>tr.onclick=()=>{
      box.querySelectorAll('tr.sel').forEach(x=>x.classList.remove('sel')); tr.classList.add('sel');
      this.openTopic(tr.dataset.topic);
    });
  },

  async openTopic(topic){
    const box=this.dial.querySelector('#edDialResp');
    box.innerHTML='<div class="hint">…</div>';
    let t;
    try{ t=await this.ask('editTopic', {topic}); }
    catch(e){ box.innerHTML='<div class="hint">'+escHtml(String(e.message||e))+'</div>'; return null; }
    this.dialTopic=t;
    const journal=t.type==='Journal';
    box.innerHTML='<table class="edT"><thead><tr><th>#</th><th>'+(journal? 'Index' : 'Who')+'</th><th>Text</th><th>From</th></tr></thead><tbody>'+
      t.responses.map((r,i)=>'<tr data-r="'+i+'"'+(this.isEditedInfo(r.id)? ' class="edited"' : '')+' title="'+escHtml(r.text)+'">'+
        '<td class="num">'+(i+1)+(r.orphan? ' <span class="bad" title="Its predecessor is not in the topic: it is read last">!</span>' : '')+'</td>'+
        '<td>'+escHtml(journal? String(r.disposition==null? '' : r.disposition) : r.speaker)+'</td>'+
        '<td>'+escHtml(r.text.length>90? r.text.slice(0,90)+'…' : r.text)+'</td>'+
        '<td class="from" title="'+escHtml(r.plugins.join(' > '))+'">'+escHtml(r.winner)+'</td></tr>').join('')+'</tbody></table>';
    box.querySelectorAll('tr[data-r]').forEach(tr=>{
      const r=t.responses[+tr.dataset.r];
      tr.ondblclick=()=>this.openRecord('INFO', r.id, r.plugins);
    });
    return t;
  },

  isEditedInfo(id){ return this.edited.has('INFO:'+String(id).toLowerCase()); },

  /* ---- the Script Edit window ------------------------------------------------------------ */

  async openScript(tag, id, plugins){
    if(!this.scr){
      const d=document.createElement('div');
      d.id='edScript'; d.className='ori'; d.hidden=true;
      d.innerHTML='<div class="orihead"><b id="edScriptTitle">Script</b><span style="flex:1"></span>'+
        '<button class="btn sm" data-stab="src">Source</button> <button class="btn sm" data-stab="lst">Compiled</button> '+
        '<button class="btn dim ic" id="edScriptX" title="Close">&#x2715;</button></div>'+
        '<div class="oribody"><div id="edScriptSrc"><textarea id="edScriptText" spellcheck="false" wrap="off"></textarea>'+
        '<div id="edScriptFind"></div><div class="orirow"><span class="v">'+
        '<button class="btn sm" id="edScriptSave" title="The text goes to the patch pool, as a change to this script">Save to the patch</button> '+
        '<button class="btn sm" id="edScriptRevert" title="Drop the changed text waiting in the pool">Revert</button></span></div>'+
        '<div class="hint" id="edScriptNote"></div></div>'+
        '<pre id="edScriptListing" hidden></pre></div>';
      ($('#vpwrap')||document.body).appendChild(d);
      d.querySelector('#edScriptX').onclick=()=>{ d.hidden=true; };
      d.querySelectorAll('[data-stab]').forEach(b=>b.onclick=()=>{
        const src=b.dataset.stab==='src';
        d.querySelector('#edScriptSrc').hidden=!src; d.querySelector('#edScriptListing').hidden=src;
        d.querySelectorAll('[data-stab]').forEach(x=>x.classList.toggle('on', x===b));
      });
      const ta=d.querySelector('#edScriptText');
      ta.onkeydown=e=>{
        e.stopPropagation();
        if(e.key==='Tab'){ e.preventDefault(); const a=ta.selectionStart; ta.value=ta.value.slice(0,a)+'    '+ta.value.slice(ta.selectionEnd); ta.selectionStart=ta.selectionEnd=a+4; ta.oninput(); }
      };
      ta.oninput=()=>{ clearTimeout(this._scrT); this._scrT=setTimeout(()=>this.checkScript(), 400); };
      d.querySelector('#edScriptSave').onclick=()=>this.saveScript();
      d.querySelector('#edScriptRevert').onclick=async()=>{
        const s=this._script; if(!s) return;
        try{ await this.ask('editRevert', Object.assign({tag:s.tag, id:s.id, path:'text'}, s.plugins? {plugins:s.plugins} : {})); }
        catch(e){ toast(String(e.message||e),'err',5000); }
        await this.refreshPending();
        this.openScript(s.tag, s.id, s.plugins);
      };
      this.scr=d;
    }
    const d=this.scr;
    this._script={tag, id, plugins};
    d.hidden=false;
    d.querySelector('#edScriptTitle').textContent='Script - '+id;
    d.querySelector('[data-stab="src"]').onclick();
    let v;
    try{ v=await this.ask('editScript', Object.assign({tag, id}, plugins? {plugins} : {})); }
    catch(e){ d.querySelector('#edScriptFind').innerHTML='<div class="hint">'+escHtml(String(e.message||e))+'</div>'; return null; }
    d.querySelector('#edScriptText').value=v.text;
    d.querySelector('#edScriptListing').textContent=v.listing||'; no compiled data in this record';
    d.querySelector('#edScriptNote').textContent=(v.queued? 'A changed text waits in the pool. ' : '')+(v.compiled
      ? 'OpenMW compiles this text when it loads. Morrowind.exe runs the compiled data the record carries, which the toolkit does not rebuild: a changed text reaches Morrowind.exe only once the Construction Set recompiles it.'
      : 'This record carries no compiled data: OpenMW compiles the text; Morrowind.exe needs the Construction Set to compile it.');
    this.drawFindings(v.findings);
    return v;
  },

  async checkScript(){
    const s=this._script; if(!s || !this.scr) return;
    const text=this.scr.querySelector('#edScriptText').value;
    try{
      const v=await this.ask('editScript', Object.assign({tag:s.tag, id:s.id, text}, s.plugins? {plugins:s.plugins} : {}));
      if(this.scr.querySelector('#edScriptText').value===text) this.drawFindings(v.findings);
    }catch(_){ }
  },

  drawFindings(list){
    const box=this.scr.querySelector('#edScriptFind');
    this._findings=list||[];
    box.innerHTML=this._findings.length
      ? this._findings.map((f,i)=>'<div class="it '+(f.level==='error'?'bad':'')+'" data-f="'+i+'"><span class="k">'+f.line+'</span> '+escHtml(f.message)+'</div>').join('')
      : '<div class="hint">No problems found.</div>';
    box.querySelectorAll('[data-f]').forEach(el=>el.onclick=()=>{
      const f=this._findings[+el.dataset.f], ta=this.scr.querySelector('#edScriptText');
      const lines=ta.value.split('\n'); let a=0;
      for(let i=0;i<f.line-1 && i<lines.length;i++) a+=lines[i].length+1;
      ta.focus(); ta.selectionStart=a; ta.selectionEnd=a+(lines[f.line-1]||'').length;
    });
  },

  async saveScript(){
    const s=this._script; if(!s) return;
    const text=this.scr.querySelector('#edScriptText').value;
    if(this._findings.some(f=>f.level==='error') && !window.confirm('The script has errors the compiler would refuse. Save it to the patch anyway?')) return;
    try{
      await this.ask('editSet', Object.assign({tag:s.tag, id:s.id, path:'text', value:text}, s.plugins? {plugins:s.plugins} : {}));
      toast('The changed text is in the patch pool','ok',3000);
      await this.refreshPending();
      this.scr.querySelector('#edScriptNote').textContent='A changed text waits in the pool. '+this.scr.querySelector('#edScriptNote').textContent.replace(/^A changed text waits in the pool\. /,'');
    }catch(e){ toast(String(e.message||e).replace(/^Wraithguard answered 400:\s*/,''),'err',6000); }
  },

  /* ---- the Use Report ------------------------------------------------------------------ */

  /** Opens the Use Report for a record: Wraithguard reads every plugin for it. */
  async showUses(tag, id, plugins){
    if(!this.uses){
      const d=document.createElement('div');
      d.id='edUsesPanel'; d.className='ori'; d.hidden=true;
      d.innerHTML='<div class="orihead"><b id="edUsesTitle">Use Report</b><span style="flex:1"></span>'+
        '<button class="btn dim ic" id="edUsesX" title="Close">&#x2715;</button></div><div class="oribody" id="edUsesBody"></div>';
      ($('#vpwrap')||document.body).appendChild(d);
      d.querySelector('#edUsesX').onclick=()=>{ d.hidden=true; };
      this.uses=d;
    }
    const d=this.uses, body=d.querySelector('#edUsesBody');
    d.hidden=false;
    d.querySelector('#edUsesTitle').textContent='Use Report - '+id;
    body.innerHTML='<div class="hint">Reading every plugin of the load order for '+escHtml(id)+'…</div>';
    let r;
    try{ r=await this.ask('editUses', Object.assign({tag, id}, plugins? {plugins} : {})); }
    catch(e){ body.innerHTML='<div class="hint">'+escHtml(String(e.message||e))+'</div>'; return null; }
    this._usesOf={tag, id, plugins};
    this.drawUses(r);
    return r;
  },

  drawUses(r){
    const body=this.uses.querySelector('#edUsesBody');
    const uses=r.uses||[];
    let h='<div class="hint">'+(uses.length
      ? r.live+' live use'+(r.live===1?'':'s')+(r.cells? ', '+r.cells+' placed in cells' : '')+'. Greyed: in a version a later plugin overrides.'
      : 'Nothing in the load order names '+escHtml(r.id)+'.')+'</div>';
    const cells=uses.filter(u=>u.type==='Cell'), other=uses.filter(u=>u.type!=='Cell');
    if(other.length){
      h+='<div class="orisec">Records</div><table class="edT"><tbody>';
      other.forEach((u,i)=>{
        h+='<tr data-u="'+uses.indexOf(u)+'"'+(u.wins? '' : ' class="from" title="'+escHtml(u.plugin)+'\'s version is overridden by a later plugin"')+'>'+
          '<td>'+escHtml(this.NAMES[u.tag]||u.tag||u.type)+'</td><td>'+escHtml(u.key)+'</td>'+
          '<td class="from" title="'+escHtml(u.paths.join(', '))+'">'+escHtml(u.paths.slice(0,2).join(', ')+(u.paths.length>2? '…' : ''))+'</td>'+
          '<td class="from">'+escHtml(u.plugin)+'</td></tr>';
      });
      h+='</tbody></table>';
    }
    if(cells.length){
      h+='<div class="orisec">Placed in</div><table class="edT"><tbody>';
      cells.forEach(u=>{
        h+='<tr data-u="'+uses.indexOf(u)+'"><td>'+escHtml(u.key)+'</td><td class="num">'+u.count+'×</td><td class="from">'+escHtml(u.plugin)+'</td></tr>';
      });
      h+='</tbody></table>';
    }
    if(uses.some(u=>u.wins && u.type!=='Script'))
      h+='<div class="orirow" style="margin-top:6px"><span class="v edVec"><input class="fld" id="edReplId" placeholder="Another '+escHtml(this.NAMES[r.tag]||r.tag)+' id" spellcheck="false">'+
        '<button class="btn sm" id="edRepl" title="Every live use above names that record instead, and every placed one becomes it - queued in the patch pool. Scripts are left as they are">Replace with</button></span></div>';
    body.innerHTML=h;
    const rb=body.querySelector('#edRepl'), ri=body.querySelector('#edReplId');
    if(rb){
      ri.onkeydown=e=>{ if(e.key==='Enter') rb.onclick(); e.stopPropagation(); };
      rb.onclick=async()=>{
        const newId=ri.value.trim(), of=this._usesOf;
        if(!newId || !of){ ri.classList.add('bad'); return; }
        try{
          const x=await this.ask('editReplace', Object.assign({tag:of.tag, id:of.id, newId}, of.plugins? {plugins:of.plugins} : {}));
          const recs=x.changed-(x.refs||0);
          toast(recs+' record'+(recs===1?'':'s')+' now name '+newId+(x.refs? ', '+x.refs+' placed reference'+(x.refs===1?'':'s')+' became it' : '')+
                (x.scripts? '; '+x.scripts+' script'+(x.scripts===1?'':'s')+' left as they are' : ''),'ok',6000);
          await this.loadModels(of.tag);
          await this.refreshPending();
        }catch(e){
          ri.classList.add('bad');
          toast(String(e.message||e).replace(/^Wraithguard answered 400:\s*/,''),'err',6000);
        }
      };
    }
    body.querySelectorAll('tr[data-u]').forEach(tr=>{
      const u=uses[+tr.dataset.u];
      tr.style.cursor='pointer';
      tr.onclick=()=>{
        if(u.type!=='Cell'){ if(u.tag) this.openRecord(u.tag, u.key, null); return; }
        const m=/^\((-?\d+), (-?\d+)\)$/.exec(u.key);
        if(typeof WgNav==='object') WgNav.go(m? m[1]+','+m[2] : 'int:'+u.key);
      };
    });
  },

  /* ---- what waits in the pool -------------------------------------------------------- */

  async refreshPending(){
    if(!this.canEdit()) return [];
    let list=[];
    try{ list=await this.ask('editPending', {}); }catch(_){ return []; }
    this.edited=new Set(list.filter(p=>p.tag).map(p=>p.tag+':'+String(p.id).toLowerCase()));
    this.made=list.filter(p=>p.made && p.tag);
    const cellsBefore=[...this.editedCells].sort().join('|');
    this.editedCells=new Set(list.filter(p=>p.ref||p.new).map(p=>String((p.ref||p.new).cell).toLowerCase()));
    if(this.el && this.tags.length) this.fillTabs();
    if(this.el && this.cells && cellsBefore!==[...this.editedCells].sort().join('|')) this.drawCells();
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
      for(const c of p.changes.filter(c=>!(p.new && c.value==null)))
        h+='<div class="orirow"><span class="k">'+escHtml(c.path)+'</span><span class="v">'+
          escHtml('value' in c? JSON.stringify(c.value) : 'from '+c.plugin)+'</span></div>';
      if(p.tag) h+='<div class="orirow"><span class="v"><button class="btn sm" data-open="'+escHtml(p.tag)+'" data-id="'+escHtml(p.id)+'">Open</button></span></div>';
      else if(p.ref) h+='<div class="orirow"><span class="v"><button class="btn sm" data-ref="'+escHtml(JSON.stringify(p.ref))+'">Open</button></span></div>';
      else if(p.new) h+='<div class="orirow"><span class="v"><button class="btn sm" data-ref="'+escHtml(JSON.stringify({cell:p.new.cell, uid:p.new.uid}))+'">Open</button></span></div>';
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
    if(k==='q' && !typing && !e.ctrlKey && !e.metaKey && !e.altKey && this.selected() && !(App.R && App.R.fly && App.R.fly.active)){
      this.quickMenu(this._pointer? this._pointer[0] : null, this._pointer? this._pointer[1] : null);
      return true;
    }
    if(e.key==='Escape' && this.qm && !this.qm.hidden){ this.qm.hidden=true; return true; }
    if((e.ctrlKey||e.metaKey) && e.key.toLowerCase()==='f'){
      const f=this.el && this.el.querySelector('#edFilter'); if(f){ f.focus(); f.select(); }
      return true;
    }
    if(e.key==='F2' && !typing){
      const r=this._oriRecord;
      if(r && Ori.el && !Ori.el.hidden){ this.openRecord(r.tag, r.id, r.plugins); return true; }
    }
    if(e.key==='F3' && !typing){
      const r=this._sel && this._sel.ref;
      if(r){ this.openRef(r); return true; }
    }
    if(e.key==='Escape' && this.dlg && !this.dlg.hidden && !typing){ this.dlg.hidden=true; return true; }
    return false;
  },
};

document.addEventListener('keydown', e=>{ if(WgEditor.keys(e)){ e.preventDefault(); e.stopPropagation(); } }, true);
document.addEventListener('keyup', e=>WgEditor.held.delete(String(e.key||'').toLowerCase()), true);
document.addEventListener('pointermove', e=>{ WgEditor._pointer=[e.clientX, e.clientY]; }, true);
document.addEventListener('pointerdown', e=>{ const q=WgEditor.qm; if(q && !q.hidden && !q.contains(e.target)) q.hidden=true; }, true);
window.addEventListener('blur', ()=>WgEditor.held.clear());
