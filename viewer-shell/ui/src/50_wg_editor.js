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
     the object changed), and scripts and dialogue results that name it as a word (a
     script recompiled, so Morrowind.exe runs the new name too). Changes go to Wraithguard's
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
     X or Y to keep to that axis; Alt-drag puts it on the surface under the pointer
     (CSSE's); Shift-drag turns it (about Z, or X/Y held; the world's axes, or its own -
     the Q menu says which); F drops it
     onto what is under it. A gold copy follows the pointer and the change goes to the
     pool on release (`grab`, over the renderer's `onGrab`).
   - The Script Edit window (a script's dialog, "Script Edit"): the source, checked as it
     is typed (begin/end, blocks, variables, functions the game knows), the compiled
     listing, and "Save to the patch". The text is compiled as it is checked (MWEdit's
     compiler, ported: wraithguard/mwscript); a save that compiles queues the new bytecode
     with the text, so Morrowind.exe runs the change too (OpenMW compiles the text itself).
     Without the Rust backend the text alone is saved, and the window says so.
   - The dialogue window (the dock's "Dialogue"): every topic by kind, and a topic's
     responses in the order the engine reads them (who says each, the plugin whose
     version wins, a response that has lost its place marked); a response opens in the
     record dialog, its changes written inside its topic.
   - A left drag off the selected object draws a selection box (Ctrl or Shift: added to
     the selection). Ctrl+click picks several objects (Esc lets them go); Q on them is CSSE's: align
     position, rotation or scale to the last picked, randomize them, reset, drop, hide.
   - Q on the selected object: a menu of what can be done to it (edit, Use Report, drop,
     duplicate, delete, reset its rotation or scale, hide, restore the hidden, move to a
     layer) and the render window's settings, as CSSE's Q menu has them: the axis an
     Alt-drag turns out of the surface it snaps onto, world or local turns, the landscape
     hidden, and QuickStart (the cell and view the viewer opens on when launched without
     one). Layers (the dock's "Layers") are named groups of references shown or hidden in
     the render window, Ctrl+Shift+1-9 for the first nine - a view, kept in this viewer,
     never written to the patch; a layer can be tinted a colour of its own, as CSSE's are.
   - Placing: drag a record from the Object Window into the render window (or right-click
     it: at the view's pivot) for a new reference, the patch's own (`editPlace`); it opens
     in the reference dialog, moves like any other, and is drawn with the rest.

   - Build patch: the pool written as the patch from here - where it goes, Append or
     Replace, the build's report as it goes - by Wraithguard, as its Patch Builder writes it
     (`editBuildInfo`, `editBuildCheck`, `editBuild`, `editBuildStatus`, `editPreviewPatch`).
   - The window furniture - docking and folding panels, the toolbar, flyouts, the dialogs'
     sections, previews and drag feedback - is 51_wg_editor_ui.js (`WgUI`).

   The lists come from the shell (`editor_tags`, `editor_records`, `editor_cell_refs`,
   `cells`); a record's contents and every change from Wraithguard, over the `links` the
   launch handed over (`editRecord`, `editSet`, `editRevert`, `editRef`, `editRefSet`,
   `editRefRevert`, `editPending`, `editReview`; gui/editorlink.py). Opened without Wraithguard, it browses but cannot
   edit, and says so.

   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
/** Rotations as the game stores them (the Euler angles `refMatrix` reads), turned the
 *  ways the render window needs: about a world axis, and an axis laid along a normal. */
const EdMath={
  /** The 3x3 a rotation makes (rows), as `refMatrix` builds it. */
  mat(rot){ const m=refMatrix([0,0,0], rot, 1); return [[m[0],m[1],m[2]],[m[4],m[5],m[6]],[m[8],m[9],m[10]]]; },
  mul(a,b){ return a.map((r,i)=>[0,1,2].map(j=>r[0]*b[0][j]+r[1]*b[1][j]+r[2]*b[2][j])); },
  /** The angles back from a 3x3 (the inverse of `mat`). */
  euler(m){
    const sy=Math.max(-1, Math.min(1, m[0][2]));
    const b=-Math.asin(sy);
    if(Math.abs(sy)<0.99999){
      return [-Math.atan2(-m[1][2], m[2][2]), b, -Math.atan2(-m[0][1], m[0][0])];
    }
    // Gimbal lock: the X and Z turns are one turn; all of it goes to Z.
    return [0, b, -Math.atan2(m[1][0], m[1][1])];
  },
  /** `rot` turned by `a` radians about world axis `k` (0 X, 1 Y, 2 Z) - the same way a
   *  turn of that angle alone would turn an unrotated object. */
  turnWorld(rot, k, a){
    const d=[0,0,0]; d[k]=a;
    return this.euler(this.mul(this.mat(d), this.mat(rot)));
  },
  /** `rot` turned so its local axis `axis` ('+z', '-x'...) points along normal `n`, by
   *  the smallest turn (so it keeps its heading as far as it can). */
  alignAxis(rot, axis, n){
    const k={x:0, y:1, z:2}[axis[1]], sign=axis[0]==='-'? -1 : 1;
    const M=this.mat(rot);
    const a=[M[0][k]*sign, M[1][k]*sign, M[2][k]*sign];   // the axis in the world now
    const c=[a[1]*n[2]-a[2]*n[1], a[2]*n[0]-a[0]*n[2], a[0]*n[1]-a[1]*n[0]];
    const sn=Math.hypot(c[0],c[1],c[2]), cs=a[0]*n[0]+a[1]*n[1]+a[2]*n[2];
    if(sn<1e-9){
      if(cs>0) return rot.slice();
      // Opposite: half a turn about any axis square to it.
      const q=Math.abs(a[0])<0.9? [1,0,0] : [0,1,0];
      const u=[a[1]*q[2]-a[2]*q[1], a[2]*q[0]-a[0]*q[2], a[0]*q[1]-a[1]*q[0]], L=Math.hypot(u[0],u[1],u[2]);
      return this.euler(this.mul(this.axisAngle(u.map(v=>v/L), Math.PI), M));
    }
    return this.euler(this.mul(this.axisAngle(c.map(v=>v/sn), Math.atan2(sn, cs)), M));
  },
  /** Rodrigues: the 3x3 turning by `t` about unit axis `u`. */
  axisAngle(u, t){
    const [x,y,z]=u, c=Math.cos(t), s=Math.sin(t), C=1-c;
    return [[c+x*x*C, x*y*C-z*s, x*z*C+y*s],
            [y*x*C+z*s, c+y*y*C, y*z*C-x*s],
            [z*x*C-y*s, z*y*C+x*s, c+z*z*C]];
  },
};

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
  _group:[],                 // Ctrl+click's selection, in the order picked: [{hit, ref}]
  _liveSig:'[[],[]]',        // changes when `live` does: part of the preview's scene key
  NUDGE:8,                   // units a nudge moves (Shift: 8x); degrees a turn (Shift: 8x)
  held:new Set(),            // Z, X, Y held: the drag's axis keys
  TURN:0.01,                 // radians a pixel of Shift-drag turns
  snapAxis:'+z',             // Alt-drag onto a surface: the object's axis turned to its normal ('' keeps the turn)
  rotWorld:true,             // Shift-drag turns about the world's axes (CSSE's default) or the object's own
  gridOn:false, grid:16,     // the Construction Set's Snap to Grid: moves land on a multiple of `grid` units (G)
  angleOn:false, angle:15,   // and Snap to Angle: turns go in steps of `angle` degrees (Shift+G)
  _undo:[], _redo:[],        // reference changes made here, to take back (Ctrl+Z) and make again (Ctrl+Y)
  _clip:null,                // Ctrl+C's object: {tag, id, model, rotation, scale}, placed by Ctrl+V
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
    this.loadPrefs();
    this.hookTints();
    document.body.classList.add('wgEditMode');
    this.applyPanels();
    const i=document.querySelector('#brand i'); if(i) i.textContent='Editor';
    document.title='Wraithguard - Editor';
    const b=$('#btnEditor'); if(b) b.classList.add('on');
    this.hookDrop();
    this.dock();
    WgUI.onEnter();
    try{
      this.tags=await Engine.call('editor_tags',{});
    }catch(e){ toast(String(e.message||e),'err',5000); this.tags=[]; }
    if(!this.tags.some(t=>t[0]===this.tag) && this.tags.length) this.tag=this.tags[0][0];
    this.fillTabs();
    await Promise.all([this.loadTag(this.tag), this.loadCells(), this.refreshPending()]);
  },

  leave(){
    this.on=false;
    if(typeof WgPath==='object' && WgPath.on) WgPath.stop();
    if(App.R && App.R.setSelection) App.R.setSelection(null);
    if(App.R && App.R.setTintGroups) App.R.setTintGroups(null);
    document.body.classList.remove('wgEditMode');
    this.applyPanels();
    const i=document.querySelector('#brand i'); if(i) i.textContent='Cell Preview';
    document.title='Wraithguard - Cell Preview';
    const b=$('#btnEditor'); if(b) b.classList.remove('on');
    if(this.dlg) this.dlg.hidden=true;
    if(this.pend) this.pend.hidden=true;
    WgUI.onLeave();
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
          '<button class="btn sm" id="edLuaBtn" title="The load order\'s OpenMW Lua scripts, checked by the engine: conflicts, missing files, per-frame costs">Lua</button> '+
          '<button class="btn sm" id="edPendBtn" title="The changes waiting in Wraithguard\'s patch pool">Pending</button></div>'+
        '<div class="edTabs" id="edTabs"></div>'+
        '<div class="edTabsMore"><button class="btn dim sm" id="edTabsAll" title="Every record type, or the first rows of them">▾ All types</button></div>'+
        '<div class="edBar"><input class="fld" id="edFilter" placeholder="Filter: id, name or model (Ctrl+F)" spellcheck="false">'+
          '<label class="from" title="Only the records the patch changes or makes - the active file"><input type="checkbox" id="edPatchOnly"> Patch only</label>'+
          '<input class="fld" id="edNewId" placeholder="New id" spellcheck="false" maxlength="31" style="width:110px;margin-left:6px">'+
          '<button class="btn sm" id="edNew" title="A blank record of this type under that id, made by the patch (Insert)">New</button>'+
          '<button class="btn dim ic edWrapBtn" id="edWrapObj" title="Wrap long names and paths onto more lines, or keep each row to one line">↵</button>'+
          '<span class="from" id="edCount"></span></div>'+
        '<div class="edTable" id="edTable"></div>'+
        '<div class="edPrevBox" id="edPrev" hidden></div>'+
      '</div>'+
      '<div class="edPane" id="edCells">'+
        '<div class="edHead"><b>Cell View</b><span style="flex:1"></span><span class="from" id="edCellName"></span></div>'+
        '<div class="edBar"><input class="fld" id="edCellFilter" placeholder="Filter cells" spellcheck="false" title="Shows the cells whose name or grid has this in it">'+
          '<label class="from" title="Only the cells whose references the patch changes or adds"><input type="checkbox" id="edCellsMod"> Modified only</label>'+
          '<button class="btn dim ic edWrapBtn" id="edWrapCells" title="Wrap long cell and object names onto more lines, or keep each row to one line">↵</button></div>'+
        '<div class="edSplit"><div class="edTable" id="edCellList"></div><div class="edTable" id="edRefList"></div></div>'+
      '</div>';
    const main=$('#main')||document.body;
    main.insertBefore(d, main.firstChild);
    this.el=d;
    d.querySelector('#edFilter').oninput=e=>{ this.filter=e.target.value; this.resetLimit('objects'); this.drawRows(); };
    d.querySelector('#edPatchOnly').onchange=e=>{ this.patchOnly=e.target.checked; this.drawRows(); };
    d.querySelector('#edNewId').onkeydown=e=>{ if(e.key==='Enter') this.insertRecord(); e.stopPropagation(); };
    d.querySelector('#edNew').onclick=()=>this.insertRecord();
    d.querySelector('#edCellsMod').onchange=e=>{ this.cellsModOnly=e.target.checked; this.resetLimit('cells'); this.drawCells(); };
    d.querySelector('#edCellFilter').oninput=e=>{ this.cellFilter=e.target.value; this.resetLimit('cells'); this.drawCells(); };
    d.querySelector('#edPendBtn').onclick=()=>this.showPending();
    d.querySelector('#edLayersBtn').onclick=()=>this.showLayers();
    d.querySelector('#edDialBtn').onclick=()=>this.showDialogue();
    d.querySelector('#edLuaBtn').onclick=()=>this.showLua();
    const tabsAll=d.querySelector('#edTabsAll');
    const tabsOpen=on=>{ d.querySelector('#edTabs').classList.toggle('all', on); tabsAll.textContent=on? '▴ Fewer types' : '▾ All types';
                         const st=WgUI.state(); st.tabsAll=on; WgUI.save(); };
    tabsAll.onclick=()=>tabsOpen(!d.querySelector('#edTabs').classList.contains('all'));
    if(WgUI.state().tabsAll) tabsOpen(true);
    for(const [id, pane] of [['#edWrapObj','#edTable'],['#edWrapCells','#edCells']]){
      const b=d.querySelector(id), key='wrap'+id;
      const set=on=>{ d.querySelector(pane).classList.toggle('edWrapOn', on); b.classList.toggle('on', on); const st=WgUI.state(); st[key]=on; WgUI.save(); };
      b.onclick=()=>set(!b.classList.contains('on'));
      if(WgUI.state()[key]) set(true);
    }
  },

  fillTabs(){
    const t=this.el.querySelector('#edTabs');
    const changed={};
    for(const k of this.edited){ const tag=k.slice(0,4); changed[tag]=(changed[tag]||0)+1; }
    t.innerHTML=this.tags.map(([tag,n])=>
      '<button class="btn sm'+(tag===this.tag?' on':'')+(changed[tag]?' edited':'')+'" data-tag="'+escHtml(tag)+'" title="'+escHtml(tag)+' - '+n+' records'+(changed[tag]? ', '+changed[tag]+' in the patch' : '')+'">'+
      escHtml(this.NAMES[tag]||tag)+(changed[tag]? ' <span class="from">'+changed[tag]+'</span>' : '')+'</button>').join('');
    t.querySelectorAll('[data-tag]').forEach(b=>b.onclick=()=>{
      this.tag=b.dataset.tag; this.resetLimit('objects');
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
    // The type's own columns, as the Construction Set has them (Wraithguard reads them).
    if(this.canEdit() && this.links().editColumns && !(this._cols||{})[tag]){
      try{
        const c=await this.ask('editColumns', {tag});
        this._cols=this._cols||{}; this._cols[tag]=c;
        if(tag===this.tag && c.columns && c.columns.length) this.drawRows();
      }catch(_){ }
    }
  },

  /** Each type's icon in the ID column, as the Construction Set marks its rows. */
  ICONS:{ACTI:'⚙', ALCH:'⚗', APPA:'⚱', ARMO:'🛡', BODY:'✋', BOOK:'📕', CLOT:'👕', CONT:'📦', DOOR:'🚪', INGR:'🌿',
    LIGH:'💡', LOCK:'🗝', MISC:'⚬', PROB:'⚲', REPA:'🔨', STAT:'🏛', WEAP:'⚔', NPC_:'👤', CREA:'🐾', LEVC:'🎲', LEVI:'🎲',
    SPEL:'✦', ENCH:'✧', SCPT:'📜', GLOB:'🌐', GMST:'⚙', LTEX:'▦', SOUN:'🔊', REGN:'🗺', FACT:'⚑', RACE:'☺', CLAS:'☰'},

  /** The Object Window's columns for the tab: the engine's, and the type's own when
   *  Wraithguard has read them - `{label, kind, get(r), title}`. */
  objColumns(){
    const own=(this._cols||{})[this.tag], have=own && own.columns && own.columns.length;
    const val=(r,i)=>{ const row=own.rows[r[0].toLowerCase()]; return row? row[i] : ''; };
    const defs=r=>r[3].map(i=>this.plugins[i]||'?');
    const cols=[{label:'ID', kind:'id', get:r=>r[0]}, {label:'Count', kind:'num', get:r=>r[4], title:'How many are placed in the world'}];
    if(have){
      const n=own.columns.length;
      own.columns.slice(0, n-2).forEach((c,i)=>cols.push({label:c.label, kind:c.kind, get:r=>val(r,i), title:c.title}));
      cols.push({label:'Model', kind:'path', get:r=>r[2]});
      cols.push({label:'Defined in', kind:'defs', get:r=>defs(r)});
      cols.push({label:'Persists', kind:'bool', get:r=>val(r,n-2), title:own.columns[n-2].title});
      cols.push({label:'Blocked', kind:'bool', get:r=>val(r,n-1), title:own.columns[n-1].title});
    }else{
      cols.push({label:'Name', kind:'text', get:r=>r[1]}, {label:'Model', kind:'path', get:r=>r[2]}, {label:'Defined in', kind:'defs', get:r=>defs(r)});
    }
    cols.push({label:'Modified', kind:'mod', get:r=>this.isEdited(r[0])? 'yes' : 'no', title:'Changes waiting in the patch'});
    return cols;
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
    const cols=this.objColumns();
    const c=Math.min(this.sort.col, cols.length-1), dir=this.sort.dir, col=cols[c];
    const key=r=>{ const v=col.get(r); return col.kind==='defs'? v.length : (typeof v==='number'? v : String(v==null? '' : v).toLowerCase()); };
    rows=rows.slice().sort((a,b)=>{ const x=key(a), y=key(b); return (x<y?-1:x>y?1:0)*dir; });
    this.el.querySelector('#edCount').textContent=rows.length+(rows.length===1?' record':' records');
    const icon=this.ICONS[this.tag]||'';
    let h='<table class="edT edObj"><thead><tr>'+cols.map((t,k)=>
      '<th data-col="'+k+'"'+(t.title? ' title="'+escHtml(t.title)+'"' : '')+'>'+escHtml(t.label)+(k===c? (dir>0?' ▲':' ▼') : '')+'</th>').join('')+'</tr></thead><tbody>';
    const cell=(t, r)=>{
      const v=t.get(r);
      if(t.kind==='id') return '<td>'+(icon? '<span class="edIco">'+icon+'</span>' : '')+escHtml(v)+'</td>';
      if(t.kind==='defs') return '<td title="'+escHtml(v.join(' > '))+'">'+escHtml(v[v.length-1]||'')+(v.length>1? ' <span class="from">+'+(v.length-1)+'</span>' : '')+'</td>';
      if(t.kind==='num') return '<td class="num">'+escHtml(v==null? '' : String(v))+'</td>';
      if(t.kind==='bool' || t.kind==='mod') return '<td class="ctr'+(v==='yes'? ' yes' : '')+'">'+escHtml(v)+'</td>';
      if(t.kind==='path') return '<td class="from">'+escHtml(v)+'</td>';
      return '<td>'+escHtml(v==null? '' : String(v))+'</td>';
    };
    for(const r of rows.slice(0,this.limit('objects'))){
      const ed=this.isEdited(r[0]);
      h+='<tr class="'+(ed?'edited':'')+'" data-id="'+escHtml(r[0])+'">'+cols.map(t=>cell(t, r)).join('')+'</tr>';
    }
    h+='</tbody></table>';
    h+=this.moreHtml(rows.length, 'objects');
    const keep=box.scrollTop;
    box.innerHTML=h;
    box.scrollTop=keep;
    WgUI.sizeColumns(box.querySelector('table'), 'objects:'+this.tag+':'+cols.length);
    this.wireMore(box, 'objects', rows.length, ()=>this.drawRows());
    box.querySelectorAll('th').forEach(th=>th.onclick=()=>{
      const k=+th.dataset.col; this.sort={col:k, dir:this.sort.col===k? -this.sort.dir : 1}; this.drawRows();
    });
    box.querySelectorAll('tr[data-id]').forEach(tr=>{
      const row=()=>this.rows.find(x=>x[0]===tr.dataset.id)
        || ((m=>m? [m.id, m.made.name||'', m.made.mesh||'', [], 0] : null)(this.made.find(p=>p.tag===this.tag && p.id===tr.dataset.id)));
      const placing=()=>{ const r=row(); return r? {tag:this.tag, id:r[0], model:r[2], defined:r[3].map(i=>this.plugins[i]).filter(Boolean)} : null; };
      // What the preview and a drag say about it.
      const card=()=>{ const p=placing(), r=row(); return p? Object.assign(p, {name:r[1], placed:r[4], edited:this.isEdited(r[0])}) : null; };
      const canPlace=this.placeable(this.tag);
      tr.title=canPlace? 'Double-click: open the record. Drag into the render window, or right-click, to place one' : 'Double-click: open the record';
      if(canPlace) WgUI.armDrag(tr, card);
      tr.oncontextmenu=e=>{ e.preventDefault(); const p=placing(); if(p && canPlace) this.placeAt(p, null, null); };
      tr.onmouseenter=()=>{ const p=card(); if(p) WgUI.hoverPreview(tr, p); };
      tr.onmouseleave=()=>WgUI.hideHover();
      tr.onclick=()=>{
        box.querySelectorAll('tr.sel').forEach(x=>x.classList.remove('sel')); tr.classList.add('sel');
        this._rowSel=tr.dataset.id; WgUI.hideHover(); WgUI.selectedPreview(card());
      };
      if(this._rowSel===tr.dataset.id) tr.classList.add('sel');
      tr.ondblclick=()=>{
        const r=this.rows.find(x=>x[0]===tr.dataset.id);
        this.openRecord(this.tag, tr.dataset.id, r? r[3].map(i=>this.plugins[i]).filter(Boolean) : null);
      };
    });
  },

  isEdited(id){ return this.edited.has(this.tag+':'+String(id).toLowerCase()); },

  /** The record types a reference can place (a land texture, a script, a spell cannot be). */
  PLACEABLE:new Set(['STAT','ACTI','DOOR','CONT','LIGH','MISC','WEAP','ARMO','CLOT','BOOK','ALCH','INGR','APPA',
    'LOCK','PROB','REPA','NPC_','CREA','LEVC','LEVI']),
  placeable(tag){ return this.PLACEABLE.has(String(tag||'').toUpperCase()); },

  /* ---- long lists: a page at a time, more on request ------------------------------------ */

  /** How many rows a list shows now (`ROW_CAP` until more are asked for). */
  limit(key){ this._limits=this._limits||{}; return this._limits[key]||this.ROW_CAP; },
  resetLimit(key){ if(this._limits) delete this._limits[key]; },
  /** The foot of a capped list: how many more, and buttons to show them. */
  moreHtml(total, key){
    const shown=this.limit(key);
    if(total<=shown) return '';
    const rest=total-shown, step=Math.min(rest, this.ROW_CAP);
    return '<div class="edMore"><span class="from">'+rest+' more</span>'+
      '<button class="btn sm" data-more="'+key+'" title="The next '+step+' (scrolling to the bottom does the same)">Show '+step+' more</button>'+
      '<button class="btn dim sm" data-moreall="'+key+'" title="Every one of them - a long list is slower to draw; the filter narrows it">Show all '+total+'</button>'+
      '<span class="from">or type in the filter</span></div>';
  },
  /** The buttons, and scrolling to the bottom loading the next page. */
  wireMore(box, key, total, redraw){
    const more=box.querySelector('[data-more="'+key+'"]'), all=box.querySelector('[data-moreall="'+key+'"]');
    const grow=n=>{ this._limits=this._limits||{}; this._limits[key]=Math.min(total, n); redraw(); };
    if(more) more.onclick=()=>grow(this.limit(key)+this.ROW_CAP);
    if(all) all.onclick=()=>grow(total);
    box.onscroll=()=>{
      if(this.limit(key)>=total || box._growing) return;
      if(box.scrollTop+box.clientHeight>=box.scrollHeight-40){
        box._growing=true;
        setTimeout(()=>{ box._growing=false; grow(this.limit(key)+this.ROW_CAP); }, 30);
      }
    };
  },

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
      const ext=(c.grid||[]).map(g=>({kind:'ext', x:g[0], y:g[1], name:g[2]||'', label:(g[2]? g[2]+' ' : 'Wilderness ')+'('+g[0]+', '+g[1]+')',
        refs:g[4], path:g[5]}));
      ext.sort((a,b)=>a.x-b.x || a.y-b.y);
      const int=(c.interiors||[]).map(r=>({kind:'int', name:r[0], label:r[0], refs:r[1], path:r[2]}));
      this.cells=[...int, ...ext];
    }catch(e){ toast(String(e.message||e),'err',5000); this.cells=[]; }
    this.drawCells();
  },

  drawCells(){
    const box=this.el.querySelector('#edCellList');
    const f=this.cellFilter.trim().toLowerCase();
    let list=this.cells||[];
    if(f) list=list.filter(c=>c.label.toLowerCase().includes(f));
    const edOf=c=>this.editedCells.has(c.kind==='int'? String(c.name).toLowerCase() : '('+c.x+', '+c.y+')');
    if(this.cellsModOnly) list=list.filter(edOf);
    // The Construction Set's columns: name, grid, references, pathgrid; sortable.
    const CS=[['Cell name', c=>c.kind==='int'? c.name : (c.name||'Wilderness')],
              ['Grid', c=>c.kind==='int'? '' : c.x+', '+c.y],
              ['Ref count', c=>c.refs==null? '' : c.refs],
              ['Path', c=>c.path==null? '' : (c.path? 'Y' : 'N')]];
    const sc=this.cellSort||{col:-1, dir:1};
    if(sc.col>=0){
      const g=CS[sc.col][1];
      const key=c=>sc.col===1? (c.kind==='int'? -1e9 : c.x*1e5+c.y) : (typeof g(c)==='number'? g(c) : String(g(c)).toLowerCase());
      list=list.slice().sort((a,b)=>{ const x=key(a), y=key(b); return (x<y?-1:x>y?1:0)*sc.dir; });
    }
    let h='<table class="edT"><thead><tr>'+CS.map(([l],k)=>'<th data-ccol="'+k+'">'+l+(k===sc.col? (sc.dir>0?' ▲':' ▼') : '')+'</th>').join('')+'</tr></thead><tbody>';
    for(const c of list.slice(0,this.limit('cells'))){
      const k=c.kind==='int'? 'int:'+c.name : c.x+','+c.y;
      const ed=edOf(c);
      const cls=[this.cellSel===k? 'sel' : '', ed? 'edited' : ''].filter(Boolean).join(' ');
      h+='<tr data-cell="'+escHtml(k)+'"'+(cls? ' class="'+cls+'"' : '')+(ed? ' title="The patch changes or adds references here"' : '')+'>'+
        '<td>'+escHtml(CS[0][1](c))+'</td><td class="ctr">'+escHtml(CS[1][1](c))+'</td><td class="num">'+escHtml(String(CS[2][1](c)))+'</td>'+
        '<td class="ctr'+(c.path? ' yes' : '')+'">'+CS[3][1](c)+'</td></tr>';
    }
    h+='</tbody></table>';
    h+=this.moreHtml(list.length, 'cells');
    const keep=box.scrollTop;
    box.innerHTML=h;
    box.scrollTop=keep;
    WgUI.sizeColumns(box.querySelector('table'), 'cells');
    this.wireMore(box, 'cells', list.length, ()=>this.drawCells());
    box.querySelectorAll('th[data-ccol]').forEach(th=>th.onclick=()=>{
      const k=+th.dataset.ccol, cur=this.cellSort||{col:-1, dir:1};
      this.cellSort={col:k, dir:cur.col===k? -cur.dir : 1}; this.drawCells();
    });
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
    let h='<table class="edT"><thead><tr><th>Object ID</th><th>Type</th><th>From</th></tr></thead><tbody>';
    this.refs.forEach((x,i)=>{
      const ic=this.ICONS[x[2]]||'';
      h+='<tr data-i="'+i+'"><td>'+(ic? '<span class="edIco">'+ic+'</span>' : '')+escHtml(x[1])+'</td><td>'+escHtml(this.NAMES[x[2]]||x[2])+'</td><td class="from">'+escHtml(x[4])+'</td></tr>';
    });
    box.innerHTML=h+'</tbody></table>';
    WgUI.sizeColumns(box.querySelector('table'), 'refs');
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
    this.syncSel();
  },

  /** After the scene is drawn again: the selection follows its references to the new
   *  objects (or lets go of those no longer there), and stays lit. */
  reselect(){
    const R=App.R; if(!R || !this.on) return;
    const byKey=new Map((R.pickables||[]).map(p=>[String(p.refKey||'').toLowerCase(), p]));
    this._group=this._group.filter(g=>byKey.has(String(g.hit.refKey||'').toLowerCase()));
    for(const g of this._group) g.hit=byKey.get(String(g.hit.refKey).toLowerCase());
    if(this._sel && this._sel.hit){
      const now=byKey.get(String(this._sel.hit.refKey||'').toLowerCase());
      if(now) this._sel.hit=now; else this._sel=null;
    }
    this.syncSel();
  },

  /** What the renderer keeps lit as selected (`R.setSelection`): the objects picked
   *  together, else the one picked - whatever the hover highlight is doing. */
  syncSel(){
    const R=App.R; if(!R || !R.setSelection) return;
    if(!this.on){ R.setSelection(null); return; }
    R.setSelection(this._group.length? this._group.map(g=>g.hit) : (this._sel && this._sel.hit? [this._sel.hit] : null));
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

  /** A record named by a link (the inspector, the full help, a list): shown here - its
   *  type's tab in the Object Window with its row selected and previewed, and its dialog
   *  open (a topic in the Dialogue window). The type a link gives is a guess where the
   *  link cannot know (a key, an owner); the engine says what the record really is. */
  async showEntry(tag, id){
    if(!id) return null;
    let plugins=null;
    try{
      const r=await Engine.call('editor_record_tag', {id});
      if(r && r.tag){ tag=r.tag; plugins=(r.plugins||[]).length? r.plugins : null; }
    }catch(_){ }
    if(!tag) return toast('No plugin of the load order has a record '+id,'warn',3500);
    if(tag==='DIAL') return this.showTopic(id);
    await this.reveal(tag, id);
    return this.openRecord(tag, id, plugins);
  },

  /** The Object Window on a record: its tab, its row selected, scrolled to and previewed
   *  (the filter set to it when the list is too long to show it). */
  async reveal(tag, id){
    if(!this.el || !this.tags.some(t=>t[0]===tag)) return false;
    if(this.tag!==tag){
      this.tag=tag;
      this.fillTabs();
      await this.loadTag(tag);
    }
    const low=String(id).toLowerCase();
    const find=()=>[...this.el.querySelectorAll('#edTable tr[data-id]')].find(tr=>tr.dataset.id.toLowerCase()===low);
    let tr=find();
    if(!tr){
      const f=this.el.querySelector('#edFilter');
      this.filter=id; if(f) f.value=id;
      this.drawRows();
      tr=find();
    }
    if(!tr) return false;
    tr.onclick();
    if(tr.scrollIntoView) tr.scrollIntoView({block:'center'});
    return true;
  },

  /** A topic, in the Dialogue window: its kind's tab, the list narrowed to it, its
   *  responses open. */
  async showTopic(id){
    await this.showDialogue();
    const t=(this.topicList||[]).find(x=>x.id.toLowerCase()===String(id).toLowerCase());
    if(!t) return toast('No topic '+id+' in the load order','warn',3500);
    this.dialKind=t.type;
    this.dial.querySelectorAll('#edDialTabs [data-kind]').forEach(b=>b.classList.toggle('on', b.dataset.kind===t.type));
    this.dial.querySelector('#edDialFilter').value=t.id;
    this.drawTopics();
    const tr=this.dial.querySelector('#edDialTopics tr[data-topic]');
    if(tr) tr.classList.add('sel');
    return this.openTopic(t.id);
  },

  req(extra){ const r=this.record; return Object.assign({tag:r.tag, id:r.id}, r.plugins? {plugins:r.plugins} : {}, extra||{}); },

  dialog(){
    if(this.dlg) return this.dlg;
    const d=document.createElement('div');
    d.id='edDlg'; d.className='ori'; d.hidden=true;
    d.innerHTML='<div class="orihead"><b id="edDlgTitle">Record</b><span style="flex:1"></span>'+
      '<button class="btn dim ic" id="edDlgX" title="Close (Esc)">&#x2715;</button></div>'+
      '<div class="oribody" id="edDlgBody"></div>';
    WgUI.mount(d);
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
    if(!num && this.isLongText(f, cur)){
      const mono=/script|bytecode/i.test(f.path) || (this.record && this.record.tag==='SCPT');
      return '<textarea class="fld grow'+(mono? ' mono' : '')+'" rows="2" data-path="'+escHtml(f.path)+'"'+dis+
        ' spellcheck="'+(mono? 'false' : 'true')+'" title="Ctrl+Enter, or leaving the box, sends the change">'+escHtml(cur==null? '' : String(cur))+'</textarea>';
    }
    const list=String(f.kind).startsWith('flags:') && f.options.length? ' list="edFlags_'+escHtml(f.path)+'"' : '';
    let h='<input class="fld" type="'+(num?'number':'text')+'"'+(num && Number.isInteger(f.value)? ' step="1"' : num? ' step="any"' : '')+
      ' data-path="'+escHtml(f.path)+'" value="'+escHtml(cur==null?'':String(cur))+'"'+list+dis+' spellcheck="false">';
    if(list) h+='<datalist id="edFlags_'+escHtml(f.path)+'">'+f.options.map(o=>'<option value="'+escHtml(o)+'">').join('')+'</datalist>';
    return h;
  },

  /** Text that wants room: a book's, a response's, a description, a script - or anything
   *  long. Shown in a box that wraps and grows rather than a one-line field. */
  LONG_TEXT:/(^|\.)(text|description|script_text|result|response|message|notes?)$/i,
  isLongText(f, cur){
    if(typeof f.value!=='string' && typeof cur!=='string') return false;
    if(f.options && f.options.length) return false;
    return this.LONG_TEXT.test(String(f.path)) || String(cur==null? '' : cur).length>48 || /\n/.test(String(cur||''));
  },

  /** A record's fields in groups, for the dialog's folding sections: what has no group
   *  first, then each dotted group (`data.*`, `ai_data.*`) in the order the record has
   *  them, the flags, the long texts, and each list on its own. */
  fieldGroups(fields){
    const groups=new Map();
    const add=(key, title, rank, f)=>{
      if(!groups.has(key)) groups.set(key, {key, title, rank, at:groups.size, fields:[]});
      groups.get(key).fields.push(f);
    };
    for(const f of fields){
      const p=String(f.path), cur=('queued' in f)? f.queued : f.value;
      if(Array.isArray(f.value)) add('list:'+p, WgUI.words(p.split('.').pop()), 4, f);
      else if(this.isLongText(f, cur)) add('text', 'Text', 3, f);
      else if(p==='flags' || String(f.kind).startsWith('flags:')) add('flags', 'Flags', 2, f);
      else if(p.includes('.')){ const top=p.split('.')[0]; add('g:'+top, WgUI.words(top), 1, f); }
      else add('general', 'General', 0, f);
    }
    return [...groups.values()].sort((a,b)=>a.rank-b.rank || a.at-b.at);
  },

  /** One group's fields as rows: the name (its dotted path in the tooltip), the input,
   *  where a waiting change came from, and its undo. A list takes the row's whole width. */
  fieldRows(fields){
    let h='<table class="edT edFields"><tbody>';
    for(const f of fields){
      const q='queued' in f;
      const was=q? '<div class="from">'+escHtml(f.source==='typed'? 'was '+JSON.stringify(f.value) : 'from '+f.source)+'</div>' : '';
      const undo='<td class="u">'+(q? '<button class="btn dim ic" data-revert="'+escHtml(f.path)+'" title="Drop this change">&#x21B6;</button>' : '')+'</td>';
      const tip=escHtml(f.path+(f.kind? ' ('+f.kind+')' : ''));
      if(Array.isArray(f.value) || this.isLongText(f, q? f.queued : f.value))
        h+='<tr class="wide'+(q?' edited':'')+'"><td colspan="2"><div class="k" title="'+tip+'">'+escHtml(WgUI.label(f.path))+
           (Array.isArray(f.value)? ' <span class="from">'+(f.count!=null? f.count : f.value.length)+'</span>' : '')+'</div>'+this.input(f)+was+'</td>'+undo+'</tr>';
      else
        h+='<tr class="'+(q?'edited':'')+'"><td class="k" title="'+tip+'">'+escHtml(WgUI.label(f.path))+'</td><td>'+this.input(f)+was+'</td>'+undo+'</tr>';
    }
    return h+'</tbody></table>';
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
      '<button class="btn sm" id="edCopy" title="A copy of this record under a new id, made by the patch - the Construction Set\'s way of making a record">Make a copy as</button>'+
      (v.new? '' : '<button class="btn sm" id="edRename" title="A copy under the new id, and every live use of this record - fields and placed references - repointed to it (the Use Report reads the load order). The original stays for anything still naming it">Rename to</button>')+'</span></div>';
    this._lists={};
    const groups=this.fieldGroups(v.fields);
    const all=WgUI.sectionBar()+WgUI.sectionsHtml(v.tag, groups.map(g=>({key:g.key, title:g.title, n:g.fields.length,
      edited:g.fields.some(f=>'queued' in f), html:this.fieldRows(g.fields)})));
    // The Construction Set's form for the type (53_wg_record_forms.js), every field below it.
    const form=typeof WgForms==='object'? WgForms.html(this, v) : '';
    if(form){
      const st=WgUI.state(), open=!!st.allFields;
      h+=form+'<details class="edAllFields"'+(open? ' open' : '')+'><summary title="Every field of the record, in groups, with a filter">All fields</summary>'+all+'</details>';
    }else h+=all;
    body.innerHTML=h+(v.tag==='INFO'? '<div id="edResult" class="edFind"></div>' : '');
    WgUI.wireSections(body);
    WgUI.wireSectionBar(body);
    WgUI.autoGrow(body);
    if(v.tag==='INFO') this.checkResult(v);
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
    const rn=body.querySelector('#edRename');
    if(rn) rn.onclick=()=>this.renameRecord(v);
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
    const cf=body.querySelector('#edConf'); if(cf) cf.onclick=()=>Ori.openConflicts(v.tag, v.id);
    body.querySelector('#edRevertAll').onclick=()=>this.change('editRevert', {});
    body.querySelectorAll('[data-revert]').forEach(b=>b.onclick=()=>this.change('editRevert', {path:b.dataset.revert}));
    this.wireLists(body);
    body.querySelectorAll('[data-path]').forEach(el=>{
      el.onchange=()=>{
        const value = el.type==='checkbox'? el.checked : el.type==='number'? (el.value===''? '' : Number(el.value)) : el.value;
        this.change('editSet', {path:el.dataset.path, value}, el);
      };
      if(el.tagName==='INPUT' && el.type!=='checkbox') el.onkeydown=e=>{ if(e.key==='Enter') el.blur(); e.stopPropagation(); };
      if(el.tagName==='TEXTAREA') el.onkeydown=e=>{ if(e.key==='Enter' && (e.ctrlKey||e.metaKey)) el.blur(); e.stopPropagation(); };
    });
    const allF=body.querySelector('details.edAllFields');
    if(allF) allF.addEventListener('toggle', ()=>{ WgUI.state().allFields=allF.open; WgUI.save(); });
    if(typeof WgForms==='object') WgForms.wire(this, body, v);
  },

  /** Rename: a copy under the new id, then Search & Replace from the original to it - the
   *  original stays (a master's record cannot be taken back without breaking whatever
   *  still names it), and what was moved, and what was not, is said. */
  async renameRecord(v){
    const box=this.dlg.querySelector('#edCopyId'), newId=box.value.trim();
    if(!newId){ box.classList.add('bad'); return null; }
    const from={tag:v.tag, id:v.id, plugins:this.record.plugins};
    try{
      const c=await this.ask('editDuplicate', Object.assign({tag:from.tag, id:from.id, newId}, from.plugins? {plugins:from.plugins} : {}));
      toast('Reading the load order for the uses of '+from.id+'…','ok',3000);
      const x=await this.ask('editReplace', Object.assign({tag:from.tag, id:from.id, newId:c.id}, from.plugins? {plugins:from.plugins} : {}));
      const recs=x.changed-(x.refs||0);
      toast(from.id+' renamed to '+c.id+': '+recs+' record'+(recs===1?'':'s')+' and '+(x.refs||0)+' placed reference'+((x.refs||0)===1?'':'s')+' now name it'+
            (x.scripts? ' ('+x.scripts+' in script text, scripts recompiled)' : '')+'. '+from.id+' itself stays.','ok',8000);
      this.record={tag:c.tag, id:c.id, plugins:null};
      this.drawRecord(c);
      await this.loadModels(c.tag);
      await this.refreshPending();
      return x;
    }catch(e){
      box.classList.add('bad');
      toast(String(e.message||e).replace(/^Wraithguard answered 400:\s*/,''),'err',6000);
      return null;
    }
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
    const row=(f, inner)=>{
      const q='queued' in f;
      return '<tr class="'+(q?'edited':'')+'"><td class="k" title="'+escHtml(f.path)+'">'+escHtml(this.REF_LABELS[f.path]||f.path)+'</td><td>'+inner+
        (q? '<div class="from">was '+escHtml(JSON.stringify(f.value))+'</div>' : '')+
        '</td><td>'+(q? '<button class="btn dim ic" data-rrevert="'+escHtml(f.path)+'" title="Drop this change">&#x21B6;</button>' : '')+'</td></tr>';
    };
    // The fields by what they are about, each group folding (its state kept for every reference).
    const GROUP={translation:'place', rotation:'place', scale:'place', destination:'door',
      owner:'own', owner_global:'own', owner_faction:'own', owner_faction_rank:'own',
      lock_level:'lock', key:'lock', trap:'lock', soul:'item', charge_left:'item', health_left:'item', object_count:'item',
      deleted:'state', blocked:'state', temporary:'state', moved_cell:'state'};
    const TITLES={place:'Placement', door:'Door', own:'Ownership', lock:'Lock & trap', item:'Item', state:'State', other:'Other'};
    const rows={};
    const put=(path, html)=>{ const g=GROUP[path]||'other'; (rows[g]=rows[g]||[]).push(html); };
    for(const path of ['translation','rotation']){
      const f=F[path]; if(!f) continue;
      const vec=(cur(f)||[0,0,0]).map(n=> path==='rotation'? n*DEG : n);
      put(path, row(f, ['X','Y','Z'].map((a,i)=>
        '<div class="edVec"><span class="from">'+a+'</span>'+
        '<button class="btn dim ic" data-nudge="'+path+'" data-i="'+i+'" data-s="-1" title="'+(path==='rotation'?'Turn':'Move')+' back (Shift: more)">&#x2212;</button>'+
        '<input class="fld" type="number" step="any" data-vec="'+path+'" data-i="'+i+'" value="'+(+vec[i].toFixed(path==='rotation'?2:1))+'">'+
        '<button class="btn dim ic" data-nudge="'+path+'" data-i="'+i+'" data-s="1" title="'+(path==='rotation'?'Turn':'Move')+' on (Shift: more)">+</button></div>').join('')));
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
      put(f.path, row(f, inner));
    }
    const order=['place','door','own','lock','item','state','other'];
    const qd=new Set(v.fields.filter(f=>'queued' in f).map(f=>GROUP[f.path]||'other'));
    h+=WgUI.sectionBar()+WgUI.sectionsHtml('REF', order.filter(g=>rows[g]).map(g=>({key:g, title:TITLES[g], edited:qd.has(g),
      html:'<table class="edT edFields"><tbody>'+rows[g].join('')+'</tbody></table>'})));
    body.innerHTML=h;
    WgUI.wireSections(body);
    WgUI.wireSectionBar(body);
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
    const u=this._undo.pop();
    if(!u) return toast('Nothing to undo','warn',1500);
    this._redo.push(u);
    await this.sendRef(u.ref, u.prior, {noUndo:true});
    toast('Undone: '+u.changes.map(c=>this.REF_LABELS[c[0]]||c[0]).join(', ').toLowerCase(),'ok',1800);
  },
  /** Ctrl+Y: what Ctrl+Z took back, made again. */
  async redo(){
    const u=this._redo.pop();
    if(!u) return toast('Nothing to redo','warn',1500);
    this._undo.push(u);
    await this.sendRef(u.ref, u.changes, {noUndo:true});
    toast('Redone','ok',1500);
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
    return out;
  },
  layerSig(){ return this.layers().filter(l=>!l.visible).map(l=>l.name+':'+l.keys.length).join(','); },

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
    ];
    return this.showMenu(m, x, y, items, a=>this.groupAction(a, group));
  },

  async groupAction(a, group){
    const last=group[group.length-1], rest=group.slice(0, -1);
    const rnd=(lo, hi)=>lo+Math.random()*(hi-lo);
    const each=async(fn)=>{ for(const g of a==='pos'||a==='rot'||a==='scl'? rest : group){ const c=fn(g); if(c && c.length) await this.sendRef(g.ref, c); } };
    if(a==='clear') return this.clearGroup();
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
      WgUI.mount(d);
      d.querySelector('#edDialX').onclick=()=>{ d.hidden=true; };
      d.querySelector('#edDialFilter').oninput=()=>this.drawTopics();
      d.querySelector('#edDialFilter').onkeydown=e=>e.stopPropagation();
      this.dial=d;
      // Flow, Map, Flags and In game beside the list (52_wg_dialogue_views.js).
      if(typeof WgDial==='object') WgDial.attach(this);
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
    box.innerHTML='<table class="edT"><tbody>'+list.slice(0,this.limit('topics')).map(t=>
      '<tr data-topic="'+escHtml(t.id)+'" title="'+escHtml(t.plugins.join(' > '))+'"><td>'+escHtml(t.id)+'</td><td class="from">'+t.plugins.length+'</td></tr>').join('')+'</tbody></table>'+
      this.moreHtml(list.length, 'topics');
    this.wireMore(box, 'topics', list.length, ()=>this.drawTopics());
    box.querySelectorAll('tr[data-topic]').forEach(tr=>tr.onclick=()=>{
      box.querySelectorAll('tr.sel').forEach(x=>x.classList.remove('sel')); tr.classList.add('sel');
      this.openTopic(tr.dataset.topic);
    });
  },

  async openTopic(topic){
    this._dialSel=topic;
    const box=this.dial.querySelector('#edDialResp');
    box.innerHTML='<div class="hint">…</div>';
    let t;
    try{ t=await this.ask('editTopic', {topic}); }
    catch(e){ box.innerHTML='<div class="hint">'+escHtml(String(e.message||e))+'</div>'; return null; }
    this.dialTopic=t;
    const journal=t.type==='Journal';
    if(journal){
      // A quest: its stages by index (the order means nothing to a journal), its name and
      // its end marked.
      t.responses=t.responses.slice().sort((a,b)=>(a.disposition|0)-(b.disposition|0));
      const name=t.responses.find(r=>r.quest==='Name');
      if(name) t.questName=name.text;
    }
    box.innerHTML=(journal && t.questName? '<div class="orisec">Quest: '+escHtml(t.questName)+'</div>' : '')+
      '<div class="orirow"><span class="v edVec"><button class="btn sm" id="edRespTop" title="A new response at the top of the topic, made by the patch">Add at top</button>'+
      '<input class="fld" id="edTopicCopyId" placeholder="New topic name" spellcheck="false">'+
      '<button class="btn sm" id="edTopicCopy" title="This topic and its responses, in their order, under a new name - made by the patch">Copy topic as</button></span></div>'+
      '<table class="edT"><thead><tr><th>#</th><th>'+(journal? 'Index' : 'Who')+'</th><th>Text</th><th>From</th><th></th></tr></thead><tbody>'+
      t.responses.map((r,i)=>'<tr data-r="'+i+'"'+(this.isEditedInfo(r.id)? ' class="edited"' : '')+' title="'+escHtml(r.text)+'">'+
        '<td class="num">'+(i+1)+(r.orphan? ' <span class="bad" title="Its predecessor is not in the topic: it is read last">!</span>' : '')+'</td>'+
        '<td>'+escHtml(journal? String(r.disposition==null? '' : r.disposition) : r.speaker)+
          (journal && r.quest? ' <span class="from" title="'+escHtml(r.quest==='Name'? 'The quest\'s name in the journal' : r.quest==='Finished'? 'This stage ends the quest' : 'This stage restarts the quest')+'">'+escHtml(r.quest)+'</span>' : '')+'</td>'+
        '<td>'+escHtml(r.text.length>90? r.text.slice(0,90)+'…' : r.text)+'</td>'+
        '<td class="from" title="'+escHtml(r.plugins.join(' > '))+'">'+escHtml(r.winner)+'</td>'+
        '<td><button class="btn dim ic" data-after="'+escHtml(r.id)+'" title="A new response after this one, made by the patch">+</button></td></tr>').join('')+'</tbody></table>';
    box.querySelectorAll('tr[data-r]').forEach(tr=>{
      const r=t.responses[+tr.dataset.r];
      tr.ondblclick=()=>this.openRecord('INFO', r.id, r.winner==='(this patch)'? null : r.plugins);
    });
    box.querySelector('#edRespTop').onclick=()=>this.addResponse(t.id, '');
    const cpIn=box.querySelector('#edTopicCopyId');
    cpIn.onkeydown=e=>{ if(e.key==='Enter') this.copyTopic(t.id); e.stopPropagation(); };
    box.querySelector('#edTopicCopy').onclick=()=>this.copyTopic(t.id);
    box.querySelectorAll('[data-after]').forEach(b=>b.onclick=e=>{ e.stopPropagation(); this.addResponse(t.id, b.dataset.after); });
    return t;
  },

  isEditedInfo(id){ return this.edited.has('INFO:'+String(id).toLowerCase()); },

  /** "Copy topic as": the topic and its responses under a new name, made by the patch;
   *  the topic list then has it, and it opens. */
  async copyTopic(topic){
    const box=this.dial.querySelector('#edTopicCopyId'), newId=box.value.trim();
    if(!newId){ box.classList.add('bad'); return null; }
    let t;
    try{ t=await this.ask('editCopyTopic', {topic, newId}); }
    catch(e){ box.classList.add('bad'); toast(String(e.message||e).replace(/^Wraithguard answered 400:\s*/,''),'err',6000); return null; }
    this.topicList=null;
    await this.showDialogue();
    await this.refreshPending();
    await this.openTopic(t.id);
    toast(t.id+': '+t.responses.length+' response'+(t.responses.length===1?'':'s')+' copied','ok',4000);
    return t;
  },

  /** A new response in a topic, after another (or at the top): made by the patch, opened
   *  in the record dialog, and shown in its place. */
  async addResponse(topic, after){
    let v;
    try{ v=await this.ask('editNewResponse', {topic, after}); }
    catch(e){ toast(String(e.message||e).replace(/^Wraithguard answered 400:\s*/,''),'err',6000); return null; }
    this.record={tag:'INFO', id:v.id, plugins:null};
    this.dialog().hidden=false;
    this.drawRecord(v);
    await this.refreshPending();
    await this.openTopic(topic);
    return v;
  },

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
        '<button class="btn sm" id="edScriptRevert" title="Drop the changed text waiting in the pool">Revert</button> '+
        '<button class="btn sm" id="edScriptFlow" title="The script\'s control flow - its ifs, whiles and returns - as a flowchart, in a Wraithguard window">Flowchart</button></span></div>'+
        '<div class="hint" id="edScriptNote"></div></div>'+
        '<pre id="edScriptListing" hidden></pre></div>';
      WgUI.mount(d);
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
      d.querySelector('#edScriptFlow').onclick=async()=>{
        const s=this._script; if(!s) return;
        const text=d.querySelector('#edScriptText').value;
        try{
          const r=await this.ask('editFlowchart', Object.assign({tag:s.tag, id:s.id, text}, s.plugins? {plugins:s.plugins} : {}));
          if(!r.shown) toast('This Wraithguard has no chart window','warn',4000);
        }catch(e){ toast(String(e.message||e),'err',5000); }
      };
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
    d.querySelector('#edScriptNote').textContent=(v.queued? 'A changed text waits in the pool. ' : '')+
      'OpenMW compiles this text when it loads; Morrowind.exe runs the compiled data, which saving here rebuilds when the text compiles.';
    this.drawFindings(this.withCompile(v));
    return v;
  },

  /** A response's result script, compiled for its errors (Morrowind compiles it when the
   *  response fires; the Construction Set checks it on save), shown under its fields. */
  async checkResult(v){
    const box=this.dlg && this.dlg.querySelector('#edResult'); if(!box) return;
    const f=v.fields.find(x=>x.path==='script_text');
    const text=f? String(('queued' in f)? f.queued : f.value||'') : '';
    if(!text.trim()){ box.innerHTML=''; return; }
    box.innerHTML='<div class="hint">Compiling the result…</div>';
    let r;
    try{ r=await this.ask('editResult', this.req()); }
    catch(e){ box.innerHTML='<div class="hint">'+escHtml(String(e.message||e))+'</div>'; return; }
    if(!this.dlg.contains(box)) return;
    const msgs=r.messages||[];
    box.innerHTML='<div class="k">Result script'+(r.speaker_script? ' (with its speaker\'s script\'s locals)' : '')+'</div>'+
      (msgs.length
        ? msgs.map(m=>'<div class="it '+(m.level==='error'?'bad':'')+'"><span class="k">'+m.line+'</span> '+escHtml(m.message)+'</div>').join('')
        : '<div class="hint">It compiles.</div>');
  },

  async checkScript(){
    const s=this._script; if(!s || !this.scr) return;
    const text=this.scr.querySelector('#edScriptText').value;
    try{
      const v=await this.ask('editScript', Object.assign({tag:s.tag, id:s.id, text}, s.plugins? {plugins:s.plugins} : {}));
      if(this.scr.querySelector('#edScriptText').value===text) this.drawFindings(this.withCompile(v));
    }catch(_){ }
  },

  /** The checks' findings and the compiler's messages, in line order. */
  withCompile(v){
    const c=v.compile||{messages:[]};
    const msgs=(c.messages||[]).map(m=>({line:m.line, level:m.level, message:'Compiler: '+m.message}));
    return (v.findings||[]).concat(msgs).sort((a,b)=>a.line-b.line);
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
      const r=await this.ask('editSet', Object.assign({tag:s.tag, id:s.id, path:'text', value:text}, s.plugins? {plugins:s.plugins} : {}));
      const built=r && r.compile && r.compile.compiled;
      toast(built? 'The changed text and its compiled data are in the patch pool' : 'The changed text is in the patch pool (not compiled: '+((r && r.compile && r.compile.messages.some(m=>m.level==='error'))? 'it has errors' : 'the Rust backend is not built')+')', built?'ok':'warn', 4000);
      await this.refreshPending();
      this.scr.querySelector('#edScriptNote').textContent='A changed text waits in the pool. '+this.scr.querySelector('#edScriptNote').textContent.replace(/^A changed text waits in the pool\. /,'');
    }catch(e){ toast(String(e.message||e).replace(/^Wraithguard answered 400:\s*/,''),'err',6000); }
  },

  /* ---- the Lua panel ------------------------------------------------------------------- */

  /** The load order's OpenMW Lua scripts, scanned by the engine (`lua_scan`: luacore over
      the overlay, so packed scripts count). Findings first, most serious first; a click on
      one shows its script's line in the list below. */
  async showLua(){
    if(!this.lua){
      const d=document.createElement('div');
      d.id='edLuaPanel'; d.className='ori'; d.hidden=true;
      d.innerHTML='<div class="orihead"><b>Lua scripts</b><span style="flex:1"></span>'+
        '<label class="from" title="Show notes as well as errors and warnings"><input type="checkbox" id="edLuaInfo"> Notes</label> '+
        '<button class="btn sm" id="edLuaRe" title="Scan again">Rescan</button> '+
        '<button class="btn sm" id="edLuaCopy" title="The report as text, for a bug report or a mod page">Copy report</button> '+
        '<button class="btn dim ic" id="edLuaX" title="Close">&#x2715;</button></div><div class="oribody" id="edLuaBody"></div>';
      WgUI.mount(d);
      d.querySelector('#edLuaX').onclick=()=>{ d.hidden=true; };
      d.querySelector('#edLuaRe').onclick=()=>this.showLua();
      d.querySelector('#edLuaInfo').onchange=()=>{ if(this._lua) this.drawLua(this._lua); };
      d.querySelector('#edLuaCopy').onclick=async()=>{
        if(!this._lua) return;
        try{ await navigator.clipboard.writeText(this._lua.report||''); toast('Lua report copied','ok',2500); }
        catch(e){ toast('Could not copy: '+String(e.message||e),'err',4000); }
      };
      this.lua=d;
    }
    const d=this.lua, body=d.querySelector('#edLuaBody');
    d.hidden=false;
    body.innerHTML='<div class="hint">Reading the load order\'s Lua scripts…</div>';
    let r;
    try{ r=await Engine.call('lua_scan',{}); }
    catch(e){ body.innerHTML='<div class="hint">'+escHtml(String(e.message||e))+'</div>'; return null; }
    if(typeof r==='string') r=JSON.parse(r);
    this._lua=r;
    this.drawLua(r);
    return r;
  },

  drawLua(r){
    const body=this.lua.querySelector('#edLuaBody');
    const notes=this.lua.querySelector('#edLuaInfo').checked;
    const all=r.findings||[], scripts=r.scripts||[];
    const count=s=>all.filter(f=>f.severity===s).length;
    let h='<div class="hint">'+escHtml(r.note||'')+'</div>';
    if(r.openmw) h+='<div class="hint">OpenMW '+escHtml(r.openmw)+(r.revision? ' (Lua API '+r.revision+')' : '')+': '+
      scripts.length+' script'+(scripts.length===1?'':'s')+', '+count('error')+' error(s), '+count('warn')+' warning(s), '+count('info')+' note(s)</div>';
    const shown=all.filter(f=>notes || f.severity!=='info');
    if(shown.length){
      h+='<div class="orisec">Findings</div><table class="edT"><tbody>';
      shown.forEach(f=>{
        const mark=f.severity==='error'? 'ERROR' : f.severity==='warn'? 'warn' : 'note';
        h+='<tr data-p="'+escHtml(f.path)+'" title="'+escHtml(f.message)+'"><td class="'+(f.severity==='error'? 'bad' : 'from')+'">'+mark+'</td>'+
          '<td>'+escHtml(f.code)+'</td><td>'+escHtml((f.path||'(load order)')+(f.line? ':'+f.line : ''))+'</td>'+
          '<td class="from">'+escHtml(f.message)+'</td></tr>';
      });
      h+='</tbody></table>';
    }else if(scripts.length){
      h+='<div class="hint">Nothing to report'+(notes? '' : ' (notes hidden)')+'.</div>';
    }
    if(scripts.length){
      h+='<div class="orisec">Scripts, in load order</div><table class="edT"><tbody>';
      scripts.forEach(s=>{
        h+='<tr data-s="'+escHtml(s.path)+'"><td>'+escHtml(s.path)+'</td><td class="from">'+escHtml(s.flags.join(', '))+'</td>'+
          '<td class="from">'+escHtml(s.interface? 'interface '+s.interface : '')+'</td>'+
          '<td class="from">'+escHtml(s.handlers.join(', '))+'</td>'+
          '<td class="'+(s.file? 'from' : 'bad')+'" title="'+escHtml(s.file||'in no data folder or archive')+'">'+
            (s.file? (s.providers>1? s.providers+' providers' : '') : 'missing')+'</td></tr>';
      });
      h+='</tbody></table>';
    }
    body.innerHTML=h;
    body.querySelectorAll('tr[data-p]').forEach(tr=>{
      tr.onclick=()=>{
        const row=[...body.querySelectorAll('tr[data-s]')].find(x=>x.dataset.s===tr.dataset.p);
        if(row){ row.scrollIntoView({block:'center'}); row.classList.add('sel'); setTimeout(()=>row.classList.remove('sel'),1200); }
      };
    });
  },

  /* ---- the Use Report ------------------------------------------------------------------ */

  /** Opens the Use Report for a record: Wraithguard reads every plugin for it. */
  async showUses(tag, id, plugins){
    if(!this.uses){
      const d=document.createElement('div');
      d.id='edUsesPanel'; d.className='ori'; d.hidden=true;
      d.innerHTML='<div class="orihead"><b id="edUsesTitle">Use Report</b><span style="flex:1"></span>'+
        '<button class="btn dim ic" id="edUsesX" title="Close">&#x2715;</button></div><div class="oribody" id="edUsesBody"></div>';
      WgUI.mount(d);
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
                (x.scripts? '; '+x.scripts+' in script text, scripts recompiled' : ''),'ok',6000);
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
    this._pendN=list.length;
    const b=document.getElementById('edPendBtn');
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
      WgUI.mount(d);
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
    h+='<div class="orirow" style="margin-top:8px"><span class="v">'+
      '<button class="btn sm pri" id="edBuildHere" title="Write the patch from here: where it goes, Append or Replace, and the build\'s report, without leaving the viewer">Build patch…</button> '+
      '<button class="btn sm" id="edReview" title="Open the Patch Builder in Wraithguard, to review and write the patch there">Review and write in Wraithguard</button></span></div>';
    body.innerHTML=h;
    body.querySelector('#edBuildHere').onclick=()=>this.showBuild();
    body.querySelectorAll('[data-open]').forEach(b=>b.onclick=()=>this.openRecord(b.dataset.open, b.dataset.id, null));
    body.querySelectorAll('[data-ref]').forEach(b=>b.onclick=()=>this.openRef(JSON.parse(b.dataset.ref)));
    body.querySelector('#edReview').onclick=async()=>{
      try{ await Engine.call('wg_post',{url:this.links().editReview, body:'{}'}); toast('The Patch Builder is open in Wraithguard','ok',3000); }
      catch(e){ toast(String(e.message||e),'err',5000); }
    };
  },

  /* ---- the patch, written from here ---------------------------------------------------- */

  /** "Build patch": the Patch Builder's sister in the viewer. What the pool carries (each
   *  entry openable, and droppable when the editor made it), where the patch goes (a data
   *  folder, the last patch, or anywhere through the system's dialog), Append or Replace
   *  when the file is there, and the build itself with its report as it goes. Wraithguard
   *  writes it (`editBuild`), exactly as its own Patch Builder does, and the pool is
   *  cleared once it is written. */
  async showBuild(){
    if(!this.bld){
      const d=document.createElement('div');
      d.id='edBuild'; d.className='ori'; d.hidden=true;
      d.innerHTML='<div class="orihead"><b>Build patch</b><span style="flex:1"></span>'+
        '<button class="btn dim ic" id="edBuildX" title="Close">&#x2715;</button></div><div class="oribody" id="edBuildBody"></div>';
      WgUI.mount(d);
      d.querySelector('#edBuildX').onclick=()=>{ d.hidden=true; };
      this.bld=d;
    }
    this.bld.hidden=false;
    const body=this.bld.querySelector('#edBuildBody');
    if(!this.canEdit() || !this.links().editBuild){
      body.innerHTML='<div class="hint">Writing the patch needs Wraithguard: open this viewer from Wraithguard\'s Cell Preview.</div>';
      return null;
    }
    body.innerHTML='<div class="hint">Reading the pool…</div>';
    let info, list;
    try{ [info, list]=await Promise.all([this.ask('editBuildInfo', {}), this.refreshPending()]); }
    catch(e){ body.innerHTML='<div class="hint">'+escHtml(String(e.message||e).replace(/^Wraithguard answered 400:\s*/,''))+'</div>'; return null; }
    this._buildInfo=info;
    this.drawBuild(info, list||[]);
    return info;
  },

  drawBuild(info, list){
    const body=this.bld.querySelector('#edBuildBody'), S=info.summary||{};
    const parts=[[S.whole,'whole record','whole records'],[S.merged,'record changed field by field','records changed field by field'],
                 [S.new_records,'new record','new records'],[S.refs,'placed object changed','placed objects changed'],
                 [S.new_refs,'new placed object','new placed objects']].filter(([n])=>n);
    let h='<div class="hint">'+(S.records? '<b>'+S.records+'</b> record'+(S.records===1?'':'s')+' to write: '+parts.map(([n,one,many])=>n+' '+(n===1? one : many)).join(', ')+'.'
      : 'Nothing is waiting in the pool: change a record or place an object first.')+'</div>';
    // What it carries.
    const rows=list.map((p,i)=>{
      const what=p.new? 'New placed object' : p.ref? 'Placed object' : (this.NAMES[p.tag]||p.tag||p.type);
      const id=p.new? (p.new.id||p.id) : p.ref? p.id+' ('+p.ref.origin+':'+p.ref.refr+')' : p.id;
      const how=p.whole? 'whole, from '+p.whole : p.made? 'made by the patch' : (p.changes||[]).length+' change'+((p.changes||[]).length===1?'':'s');
      const drop=this.canDrop(p);
      return '<tr data-i="'+i+'"'+(this._bSel && this._bSel.has(this.pendKey(p))? ' class="sel"' : '')+'><td>'+escHtml(what)+'</td><td class="edWrapCell">'+escHtml(id)+'</td><td class="from">'+escHtml(how)+'</td>'+
        '<td><button class="btn dim ic" data-bopen="'+i+'" title="Open it">✎</button>'+
        (drop? '<button class="btn dim ic" data-bdrop="'+i+'" title="Take it out of the patch (its changes are dropped)">&#x2715;</button>' : '')+'</td></tr>';
    }).join('');
    const sec=[{key:'carries', title:'What it carries', n:list.length,
      html:list.length? '<div class="edBuildList" tabindex="0" title="Click to select (Ctrl+click: several, Shift+click: a run); Delete takes the selected out of the patch">'+
        '<table class="edT"><tbody>'+rows+'</tbody></table></div>'+
        '<div class="wgBtns"><button class="btn sm" id="edBuildDel" disabled title="Take the selected out of the patch: their changes are dropped (Delete)">✕ Delete selected</button>'+
        '<button class="btn dim sm" id="edBuildAll" title="Select everything in the list (Ctrl+A)">Select all</button></div>'
        : '<div class="hint">Nothing yet.</div>'}];
    // Where it goes.
    const picks=[];
    if(info.last) picks.push(['The last patch', info.last]);
    for(const f of info.folders||[]) picks.push([f, f.replace(/[\\/]+$/,'')+(f.includes('\\')? '\\' : '/')+info.defaultName]);
    sec.push({key:'where', title:'Where it goes', html:
      '<div class="edVec"><input class="fld" id="edBuildPath" spellcheck="false" placeholder="The patch file, folder and all" value="'+escHtml(this._buildPath||info.suggested||'')+'" title="The plugin the patch is written as. Load it last">'+
      '<button class="btn sm" id="edBuildBrowse" title="Choose the file with the system\'s dialog">Browse…</button></div>'+
      (picks.length? '<div class="edChips">'+picks.slice(0,8).map(([label, path])=>
        '<button class="btn dim sm" data-bpath="'+escHtml(path)+'" title="'+escHtml(path)+'">'+escHtml(label.length>46? '…'+label.slice(-45) : label)+'</button>').join('')+'</div>' : '')+
      '<div id="edBuildState" class="hint"></div>'+
      '<div id="edBuildMode" hidden><label title="Keep what the file already carries and add the pool on top: one patch built over several sessions"><input type="radio" name="edBuildMode" value="append" checked> Append</label> '+
      '<label title="Throw away what the file carries and write only the pool"><input type="radio" name="edBuildMode" value="replace"> Replace</label></div>'});
    sec.push({key:'report', title:'Report', html:'<pre id="edBuildLog" class="edLog">Nothing written yet.</pre>'});
    h+=WgUI.sectionsHtml('BUILD', sec)+
      '<div class="hint">Your mods are not changed: the patch is one new plugin. Load it last, and back up your saves.</div>'+
      '<div class="orirow"><span class="v"><button class="btn pri" id="edBuildGo"'+(S.records? '' : ' disabled')+' title="Write the patch where it says">Write the patch</button> '+
      '<button class="btn sm" id="edBuildRefresh" title="Read the pool again">Refresh</button></span></div>'+
      '<div id="edBuildDone"></div>';
    body.innerHTML=h;
    WgUI.wireSections(body);
    const path=body.querySelector('#edBuildPath');
    const check=async()=>{
      this._buildPath=path.value.trim();
      const st=body.querySelector('#edBuildState'), mode=body.querySelector('#edBuildMode');
      mode.hidden=true; path.classList.remove('bad');
      if(!this._buildPath){ st.textContent=''; return null; }
      try{
        const c=await this.ask('editBuildCheck', {path:this._buildPath});
        mode.hidden=!c.exists;
        st.textContent= c.exists? 'This file is there already: append the pool to it, or replace it.'+(c.inOrder? ' It is in the load order.' : '')
          : 'A new file.'+(c.inOrder? ' A plugin of this name is in the load order.' : '');
        this._buildExists=!!c.exists;
        return c;
      }catch(e){ path.classList.add('bad'); st.textContent=String(e.message||e).replace(/^Wraithguard answered 400:\s*/,''); return null; }
    };
    path.onchange=check;
    path.onkeydown=e=>{ if(e.key==='Enter') path.blur(); e.stopPropagation(); };
    body.querySelectorAll('[data-bpath]').forEach(b=>b.onclick=()=>{ path.value=b.dataset.bpath; check(); });
    body.querySelector('#edBuildBrowse').onclick=async()=>{
      let p=null;
      try{ p=await Engine.pick('save', {defaultPath:path.value||info.suggested||info.defaultName, filters:[{name:'Morrowind plugin', extensions:['esp']},{name:'OpenMW addon', extensions:['omwaddon']}]}); }
      catch(e){ toast(String(e.message||e),'err',4000); }
      if(p){ path.value=String(p); check(); }
    };
    body.querySelector('#edBuildRefresh').onclick=()=>this.showBuild();
    body.querySelector('#edBuildGo').onclick=()=>this.build();
    body.querySelectorAll('[data-bopen]').forEach(b=>b.onclick=()=>{
      const p=list[+b.dataset.bopen];
      if(p.new) this.openRef({cell:p.new.cell, uid:p.new.uid});
      else if(p.ref) this.openRef(p.ref);
      else if(p.tag) this.openRecord(p.tag, p.id, null);
    });
    body.querySelectorAll('[data-bdrop]').forEach(b=>b.onclick=e=>{ e.stopPropagation(); this.dropPending([list[+b.dataset.bdrop]]); });
    // Selecting rows: click, Ctrl+click, Shift+click; Delete (or the button) drops them.
    this._bSel=new Set([...(this._bSel||[])].filter(k=>list.some(p=>this.pendKey(p)===k)));
    this._bList=list;
    const box=body.querySelector('.edBuildList'), delBtn=body.querySelector('#edBuildDel');
    const paint=()=>{
      body.querySelectorAll('tr[data-i]').forEach(tr=>tr.classList.toggle('sel', this._bSel.has(this.pendKey(list[+tr.dataset.i]))));
      const n=[...this._bSel].length;
      if(delBtn){ delBtn.disabled=!n; delBtn.textContent='✕ Delete selected'+(n? ' ('+n+')' : ''); }
    };
    body.querySelectorAll('tr[data-i]').forEach(tr=>{
      tr.onclick=e=>{
        if(e.target.closest('button')) return;
        const i=+tr.dataset.i, k=this.pendKey(list[i]);
        if(e.shiftKey && this._bAnchor!=null){
          const [a,b]=[Math.min(this._bAnchor,i), Math.max(this._bAnchor,i)];
          if(!(e.ctrlKey||e.metaKey)) this._bSel.clear();
          for(let j=a;j<=b;j++) this._bSel.add(this.pendKey(list[j]));
        }else if(e.ctrlKey||e.metaKey){
          if(this._bSel.has(k)) this._bSel.delete(k); else this._bSel.add(k);
          this._bAnchor=i;
        }else{ this._bSel=new Set([k]); this._bAnchor=i; }
        paint();
        if(box) box.focus({preventScroll:true});
      };
      tr.ondblclick=()=>{ const b=tr.querySelector('[data-bopen]'); if(b) b.click(); };
    });
    if(delBtn) delBtn.onclick=()=>this.dropSelectedPending();
    const all=body.querySelector('#edBuildAll');
    if(all) all.onclick=()=>{ this._bSel=new Set(list.map(p=>this.pendKey(p))); paint(); if(box) box.focus({preventScroll:true}); };
    paint();
    check();
  },

  /** A pool entry's identity across redraws. */
  pendKey(p){
    if(p.new) return 'new:'+p.new.cell+':'+p.new.uid;
    if(p.ref) return 'ref:'+String(p.ref.cell||'')+':'+p.ref.origin+':'+p.ref.refr;
    return 'rec:'+(p.tag||p.type)+':'+String(p.id).toLowerCase();
  },
  canDrop(p){ return !!(p.tag || p.ref || p.new); },

  /** Takes entries out of the pool (a record's changes or whole copy, a placed object's
   *  changes, a new placed object), then reads the pool again. */
  async dropPending(items){
    items=(items||[]).filter(p=>p && this.canDrop(p));
    if(!items.length) return 0;
    let n=0;
    for(const p of items){
      try{
        if(p.new) await this.ask('editNewRemove', {cell:p.new.cell, uid:p.new.uid});
        else if(p.ref) await this.ask('editRefRevert', this.refBody(p.ref));
        else await this.ask('editRevert', {tag:p.tag, id:p.id});
        n++;
        if(this._bSel) this._bSel.delete(this.pendKey(p));
      }catch(e){ toast(String(e.message||e).replace(/^Wraithguard answered 400:\s*/,''),'err',5000); break; }
    }
    if(n>1) toast(n+' taken out of the patch','ok',2000);
    await this.showBuild();
    return n;
  },
  dropSelectedPending(){
    const list=this._bList||[], sel=this._bSel||new Set();
    const items=list.filter(p=>sel.has(this.pendKey(p)));
    if(!items.length){ toast('Select what to take out first (click a row)','warn',2000); return; }
    return this.dropPending(items);
  },

  /** Writes the patch where the panel says, and follows the build to its end. */
  async build(){
    const body=this.bld && this.bld.querySelector('#edBuildBody');
    if(!body) return null;
    const path=body.querySelector('#edBuildPath').value.trim();
    const c=await this.ask('editBuildCheck', {path}).catch(e=>{ toast(String(e.message||e).replace(/^Wraithguard answered 400:\s*/,''),'err',5000); return null; });
    if(!c) return null;
    const chosen=body.querySelector('input[name="edBuildMode"]:checked');
    const mode=c.exists? (chosen? chosen.value : 'append') : 'new';
    const go=body.querySelector('#edBuildGo'), log=body.querySelector('#edBuildLog'), done=body.querySelector('#edBuildDone');
    const rep=body.querySelector('details[data-sec="BUILD:report"]'); if(rep) rep.open=true;
    go.disabled=true; go.textContent='Writing…'; log.textContent=''; done.innerHTML='';
    let job;
    try{ job=(await this.ask('editBuild', {path, mode})).job; }
    catch(e){
      go.disabled=false; go.textContent='Write the patch';
      log.textContent=String(e.message||e).replace(/^Wraithguard answered 400:\s*/,'');
      return null;
    }
    let seen=0, st=null;
    for(let i=0;i<3600;i++){
      await new Promise(r=>setTimeout(r, i<10? 150 : 400));
      try{ st=await this.ask('editBuildStatus', {job, from:seen}); }
      catch(e){ log.textContent+='\n'+String(e.message||e); break; }
      if(st.lines && st.lines.length){ log.textContent+=(log.textContent? '\n' : '')+st.lines.join('\n'); seen=st.total; log.scrollTop=log.scrollHeight; }
      if(st.state!=='running') break;
    }
    go.textContent='Write the patch';
    if(!st || st.state!=='done'){
      go.disabled=false;
      const msg=st && st.error? st.error : 'The build did not finish';
      done.innerHTML='<div class="hint edBad">'+escHtml(msg)+'</div>';
      toast('The patch was not written: '+msg,'err',8000);
      return st;
    }
    const r=st.result;
    done.innerHTML='<div class="hint edGood">'+escHtml(r.note).replace(/\n/g,'<br>')+'</div>'+
      '<div class="orirow"><span class="v"><button class="btn sm" id="edBuildPreview" title="A new Cell Preview with the patch loaded last, on the cells it changes">Preview it in a new Cell Preview</button> '+
      '<button class="btn sm" id="edBuildCopy" title="The patch\'s path, to the clipboard">Copy the path</button></span></div>';
    done.querySelector('#edBuildPreview').onclick=async()=>{
      try{ await this.ask('editPreviewPatch', {path:r.output}); toast('Opening a Cell Preview with the patch','ok',3000); }
      catch(e){ toast(String(e.message||e).replace(/^Wraithguard answered 400:\s*/,''),'err',5000); }
    };
    done.querySelector('#edBuildCopy').onclick=async()=>{ try{ await navigator.clipboard.writeText(r.output); toast('Copied','ok',1500); }catch(_){ } };
    toast(r.records+' record'+(r.records===1?'':'s')+' written to '+String(r.output).split(/[\\/]/).pop(),'ok',6000);
    this._buildPath=r.output;
    await this.refreshPending();
    return st;
  },

  /* ---- keys ----------------------------------------------------------------------- */

  keys(e){
    if(!this.on) return false;
    const inBuild=this.bld && !this.bld.hidden && document.activeElement && document.activeElement.closest && document.activeElement.closest('.edBuildList');
    if(inBuild){
      if(e.key==='Delete' || e.key==='Backspace'){ this.dropSelectedPending(); return true; }
      if((e.ctrlKey||e.metaKey) && String(e.key).toLowerCase()==='a'){ const b=document.getElementById('edBuildAll'); if(b) b.click(); return true; }
      if(e.key==='Escape' && this._bSel && this._bSel.size){ this._bSel.clear(); this.showBuild(); return true; }
    }
    if(typeof WgPath==='object' && WgPath.keys(e)) return true;
    const typing=/^(INPUT|TEXTAREA|SELECT)$/.test((document.activeElement||{}).tagName||'');
    const k=String(e.key||'').toLowerCase();
    if(!typing && (k==='z'||k==='x'||k==='y') && !e.ctrlKey && !e.metaKey) this.held.add(k);
    if(k==='f' && !typing && !e.ctrlKey && !e.metaKey && !e.altKey && this.selected()) return this.drop();
    if(k==='q' && !typing && !e.ctrlKey && !e.metaKey && !e.altKey && (this.selected() || this.group().length>1) && !(App.R && App.R.fly && App.R.fly.active)){
      this.quickMenu(this._pointer? this._pointer[0] : null, this._pointer? this._pointer[1] : null);
      return true;
    }
    if(e.key==='Escape' && this.qm && !this.qm.hidden){ this.qm.hidden=true; return true; }
    // The Construction Set's own: snaps, undo, copy and paste, delete, centre on it.
    const plain=!typing && !e.altKey && !(App.R && App.R.fly && App.R.fly.active);
    if(plain && !e.ctrlKey && !e.metaKey && k==='g'){
      if(e.shiftKey){ this.angleOn=!this.angleOn; toast('Snap to angle '+(this.angleOn? 'on: '+this.angle+'° steps' : 'off'),'ok',1800); }
      else{ this.gridOn=!this.gridOn; toast('Snap to grid '+(this.gridOn? 'on: '+this.grid+' units' : 'off'),'ok',1800); }
      this.savePrefs(); return true;
    }
    if(plain && (e.ctrlKey||e.metaKey) && k==='z'){ if(e.shiftKey) this.redo(); else this.undo(); return true; }
    if(plain && (e.ctrlKey||e.metaKey) && k==='y'){ this.redo(); return true; }
    if(plain && (e.ctrlKey||e.metaKey) && k==='c' && this.selected()) return this.copySel();
    if(plain && (e.ctrlKey||e.metaKey) && k==='v' && this._clip){ this.paste(); return true; }
    if(plain && (e.ctrlKey||e.metaKey) && k==='d' && this.selected()){
      const sel=this.selected(), rec=this.selectedRecord(sel);
      if(rec.tag){ this.quickAction('dup', sel, rec); return true; }
    }
    if(plain && !e.ctrlKey && !e.metaKey && e.key==='Delete' && this.selected()){
      const sel=this.selected(); this.quickAction('del', sel, this.selectedRecord(sel)); return true;
    }
    if(plain && !e.ctrlKey && !e.metaKey && !e.shiftKey && k==='c' && this.selected()) return this.centreOnSel();
    if(plain && !e.ctrlKey && !e.metaKey && k==='i'){
      if(e.shiftKey) this.setPanels('tfh', this.showTfh===false); else this.setPanels('ori', this.showOri===false);
      return true;
    }
    if(e.key==='Escape' && !typing && this._group.length){ this.clearGroup(); return true; }
    // Layers: Ctrl+Shift+1..9 shows or hides layer N (the Layers panel numbers them).
    if(!typing && (e.ctrlKey||e.metaKey) && e.shiftKey && /^Digit[1-9]$/.test(e.code||'')){
      const L=this.layers(), l=L[+e.code.slice(5)-1];
      if(l){ l.visible=!l.visible; this.layersChanged(); toast('Layer '+l.name+(l.visible? ' shown' : ' hidden'),'ok',1500); }
      return true;
    }
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
