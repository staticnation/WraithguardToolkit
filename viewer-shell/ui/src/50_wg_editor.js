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
   - The parts: this file holds the state, the links and the dock; each window's methods are
     in 50_wg_editor_<window>.js, added to WgEditor in ORDER after it (Object.assign).
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
    this.startPoll();
  },

  leave(){
    this.on=false;
    this.stopPoll();
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
          '<label class="from" title="Search every field of the records, as CSSE\'s filter does - the words must all match:\n  text - any field contains it\n  path:text - a field whose path has \'path\' contains it (script:, data.weight:5; path: alone - it is set)\n  path=value, path!=value - equal or not\n  path>n, path<n, path>=n, path<=n - a number compared\nQuote a phrase with spaces in it."><input type="checkbox" id="edFilterAll"> All fields</label>'+
          '<button class="btn dim ic" id="edSaved" title="Saved searches (OpenMW-CS\'s named filters): keep this one under a name, or run one">★</button>'+
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
    d.querySelector('#edFilter').oninput=e=>{ this.filter=e.target.value; this.resetLimit('objects'); if(this.filterAll) this.searchSoon(); else this.drawRows(); };
    d.querySelector('#edFilterAll').onchange=e=>{
      this.filterAll=e.target.checked;
      d.querySelector('#edFilter').placeholder=this.filterAll? 'Search every field: text, path:text, path=value, path>n (Ctrl+F)' : 'Filter: id, name or model (Ctrl+F)';
      if(this.filterAll && !this.canEdit()){ toast('Searching every field needs Wraithguard: open the viewer from Wraithguard (Cell Preview)','warn',4000); }
      this._search=null; this.resetLimit('objects');
      if(this.filterAll) this.searchSoon(); else this.drawRows();
    };
    d.querySelector('#edPatchOnly').onchange=e=>{ this.patchOnly=e.target.checked; this.drawRows(); };
    d.querySelector('#edSaved').onclick=e=>this.savedMenu(e);
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

};
