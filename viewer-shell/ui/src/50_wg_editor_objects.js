/* =====================================================================================
   Wraithguard Editor (50_wg_editor.js): the Object Window: its tabs, rows, search by any field, long lists a page at a time, and new records.

   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
Object.assign(WgEditor, {
  /** The tabs: the engine's record types (`editor_tags`, with their counts of records in the
   *  load order), and those the patch has records of that the load order has none of (a
   *  type the patch made the first record of), and the tab on screen when it is one still
   *  empty (just chosen with "+ Type", before its first record is made). Sorted by tag. */
  allTabs(){
    const have=new Map((Array.isArray(this.tags)? this.tags : []).map(([tag,n])=>[tag,n]));
    // Only the record types the Object Window lists: the patch's cells and land are the Cell View's.
    for(const p of this.made||[]) if(p.tag && this.NAMES[p.tag] && !have.has(p.tag)) have.set(p.tag, 0);
    if(this.tag && !have.has(this.tag) && this._newType===this.tag) have.set(this.tag, 0);
    return [...have].sort((a,b)=>a[0]<b[0]? -1 : a[0]>b[0]? 1 : 0);
  },

  /** The record types the editor can make a first record of (the dialogue panel makes
   *  dialogue; skills, magic effects, game settings and land textures are the engine's). */
  NOT_MADE:new Set(['DIAL','GMST','LTEX','SKIL','MGEF']),
  creatableTags(){ return Object.keys(this.NAMES).filter(tag=>!this.NOT_MADE.has(tag)); },

  fillTabs(){
    const t=this.part('edTabs'); if(!t) return;
    const changed={};
    for(const k of this.edited){ const tag=k.slice(0,4); changed[tag]=(changed[tag]||0)+1; }
    const tabs=this.allTabs();
    t.innerHTML=tabs.map(([tag,n])=>
      '<button class="btn sm'+(tag===this.tag?' on':'')+(changed[tag]?' edited':'')+'" data-tag="'+escHtml(tag)+'" title="'+escHtml(tag)+' - '+(n? n+' records' : 'none in the load order')+(changed[tag]? ', '+changed[tag]+' in the patch' : '')+'">'+
      escHtml(this.NAMES[tag]||tag)+(changed[tag]? ' <span class="from">'+changed[tag]+'</span>' : '')+'</button>').join('');
    t.querySelectorAll('[data-tag]').forEach(b=>b.onclick=()=>{
      this.tag=b.dataset.tag; this.resetLimit('objects'); this._fromTab=true;
      t.querySelectorAll('.on').forEach(x=>x.classList.remove('on')); b.classList.add('on');
      this._search=null;
      this.loadTag(this.tag).then(()=>{ if(this.filterAll && this.filter.trim()) this.searchSoon(); });
    });
    // "+ Type": a record type the load order has none of yet (the first of it is made here).
    let nb=this.part('edTabsNew');
    if(!nb){
      const row=t.parentNode && t.parentNode.querySelector('.edTabsMore');
      if(row){
        nb=document.createElement('button');
        nb.id='edTabsNew'; nb.className='btn dim sm';
        nb.textContent='＋ Type';
        nb.title='A record type this load order has none of yet - the first record of it is made by the patch';
        nb.onclick=()=>this.newTypeMenu(nb);
        row.appendChild(nb);
      }
    }
    if(nb) nb.hidden=!this.canEdit();
  },

  /** "+ Type": the types with no tab, to choose one from. */
  newTypeMenu(at){
    const have=new Set(this.allTabs().map(t=>t[0]));
    const items=this.creatableTags().filter(tag=>!have.has(tag))
      .sort((a,b)=>String(this.NAMES[a]).localeCompare(String(this.NAMES[b])))
      .map(tag=>({label:this.NAMES[tag]+' ('+tag+')', act:()=>this.startType(tag)}));
    WgUI.menu(at, [{head:'Record types with none yet'}, ...(items.length? items : [{label:'Every type already has a tab', info:true}])]);
  },

  /** A type with no records in the load order chosen: its (empty) tab on screen, the id box
   *  ready for the first record's id ("New" makes it; the tab then stays with it). */
  async startType(tag){
    this._newType=tag; this.tag=tag; this.resetLimit('objects'); this._search=null;
    this.fillTabs();
    await this.loadTag(tag);
    const box=this.part('edNewId'); if(box){ try{ box.focus(); }catch(_){ } }
    toast('No '+(this.NAMES[tag]||tag).toLowerCase()+' records yet - type an id and press New','info',4000);
  },

  /** The world changed (a connect): the tabs and the rows are asked for again. */
  async worldChanged(){
    this._cols=null; this._colGen=(this._colGen|0)+1; this._rowsProblem=null;
    if(!this.on) return;                                  // `enter` asks when it opens
    try{ this.tags=await Engine.call('editor_tags',{}); }catch(_){ }
    if(!Array.isArray(this.tags)) this.tags=[];
    // The tab on screen stays when the patch has records of it, though the load order has none.
    const tabs=this.allTabs();
    if(!tabs.some(t=>t[0]===this.tag) && tabs.length) this.tag=tabs[0][0];
    this.fillTabs();
    await this.loadTag(this.tag);
  },

  /** `editor_rows`' bytes -> {plugins, rows:[[id, name, model, [plugin ix...], placed]...]}
   *  (the layout is beside `pack_rows` in commands.rs). Throws on anything malformed. */
  unpackRows(buf){
    const dv=new DataView(buf), td=new TextDecoder('utf-8'), u8=new Uint8Array(buf);
    let at=0;
    const need=n=>{ if(at+n>buf.byteLength) throw new Error('the rows ended early, at byte '+at+' of '+buf.byteLength); };
    const u32=()=>{ need(4); const v=dv.getUint32(at,true); at+=4; return v; };
    const str=()=>{ const n=u32(); need(n); const s=td.decode(u8.subarray(at,at+n)); at+=n; return s; };
    need(8);
    if(String.fromCharCode(u8[0],u8[1],u8[2],u8[3])!=='WGOR') throw new Error('the rows are not WGOR bytes');
    at=4; if(u32()!==1) throw new Error('the rows are a version this page does not read');
    const plugins=[]; for(let i=u32(); i>0; i--) plugins.push(str());
    const rows=[];
    for(let i=u32(); i>0; i--){
      const id=str(), name=str(), model=str(), ix=[];
      for(let k=u32(); k>0; k--) ix.push(u32());
      rows.push([id, name, model, ix, u32()]);
    }
    return {plugins, rows};
  },

  /** One type's records, `{plugins, rows:[[id, name, model, [plugin ix...], placed]...]}`,
   *  for every list in the Editor: the bytes (`editor_rows`) first, the JSON list
   *  (`editor_records`) only if those fail (said in Messages). Throws when both fail. */
  async recordRows(tag){
    let first;
    try{ return this.unpackRows(await Engine.bytes('editor_rows',{tag})); }
    catch(e){ first=String(e && e.message || e); }
    let problem;
    try{
      const r=await Engine.call('editor_records',{tag});
      if(r && typeof r==='object' && Array.isArray(r.rows)){
        if(typeof WgLog==='object') WgLog.add('The '+((this.NAMES||{})[tag]||tag)+' list came the second way: '+first, 'warn');
        return {plugins:r.plugins||[], rows:r.rows};
      }
      const txt=typeof r==='string'? r : JSON.stringify(r);
      problem='the engine\'s answer was not a list of records ('+(txt==null? 'nothing' : txt.length+' characters, starting '+JSON.stringify(String(txt).slice(0,120)))+')';
    }catch(e){ problem=String(e && e.message || e); }
    throw new Error(first+'; then '+problem);
  },

  async loadTag(tag){
    // Static, 2026-10-13: never a silent empty list. A failed call, an answer that is not
    // the rows (`Engine.call` hands back text it cannot read as JSON), or no rows for a tab
    // the engine counts records in - each is said in Messages and in the table, with a
    // Reload, rather than shown as "0 records".
    // The rows come as bytes (`editor_rows`, no JSON); should that fail, the JSON list
    // (`editor_records`) is the second way, and only both failing is a problem.
    const want=(this.tags.find(t=>t[0]===tag)||[])[1]|0;
    let problem=null, got=null;
    // A type the load order has no record of (the patch makes its first) has nothing to ask
    // the engine for: its rows are the patch's own, which `drawRows` adds.
    const inWorld=!(this.tags||[]).length || this.tags.some(t=>t[0]===tag);
    if(!inWorld) got={plugins:[], rows:[]};
    else try{ got=await this.recordRows(tag); }catch(e){ problem=String(e && e.message || e); }
    // A later tab may be showing by now: its rows and plugins are not this answer's to replace.
    if(tag!==this.tag){
      this._fieldsOf=this._fieldsOf||{};
      return;
    }
    if(got){ this.plugins=got.plugins; this.rows=got.rows; } else this.rows=[];
    // Rows that came but are none, for a tab counted with some: said in the table (with
    // its Reload), not logged - the call itself worked.
    const failed=!!problem;
    if(!problem && !this.rows.length && want>0) problem='the engine counts '+want+' but sent none';
    if(tag===this.tag){
      this._rowsProblem=problem? {tag, text:problem} : null;
      if(failed && typeof WgLog==='object') WgLog.add('The '+(this.NAMES[tag]||tag)+' list did not load: '+problem, 'err');
      this.drawRows();
    }
    // The type's own columns, as the Construction Set has them (Wraithguard reads them).
    this.scheduleColumns(tag);   // not waited for: the rows are on screen, the columns follow
  },

  /** A type's own columns (`editColumns`), as the patch would leave the records: queued
   *  values and the patch's own records included. Kept per tab; `force` asks again. An
   *  answer that was asked for before the cache was dropped (an edit landed since) is
   *  not kept - the drop asked again. */
  /** A tab's columns, soon: at once for a load that was not a click on a tab (the first, a new
   *  world), and for a click only if the tab is still the one on screen 200 ms later. Clicking
   *  down a row of tabs asked for every tab's columns (each is every plugin's records of that
   *  type) and the answers queued up behind each other until they timed out; now only the tab
   *  that is stopped on is asked for. The rows are on screen already, either way. */
  scheduleColumns(tag){
    clearTimeout(this._colTimer);
    const viaTab=this._fromTab; this._fromTab=false;
    if(!this.canEdit() || !this.links().editColumns || (this._cols||{})[tag]) return;
    if(!viaTab){ this.loadColumns(tag); return; }
    this._colTimer=setTimeout(()=>{ if(this.tag===tag) this.loadColumns(tag); }, 200);
  },

  async loadColumns(tag, force=false){
    if(!this.canEdit() || !this.links().editColumns) return;
    if(!force && (this._cols||{})[tag]) return;
    // One request per tab at a time: asked again while it is out, it is the same answer.
    this._colAsk=this._colAsk||new Map();
    if(!force && this._colAsk.has(tag)) return this._colAsk.get(tag);
    const run=this._loadColumns(tag).finally(()=>{ if(this._colAsk.get(tag)===run) this._colAsk.delete(tag); });
    this._colAsk.set(tag, run);
    return run;
  },

  async _loadColumns(tag){
    const gen=this._colGen|0;
    try{
      const c=await this.ask('editColumns', {tag});
      if(gen!==(this._colGen|0)) return;
      this._cols=this._cols||{}; this._cols[tag]=c;
      if(tag===this.tag && c.columns && c.columns.length && this.el) this.drawRows();
    }catch(e){
      // Said, not swallowed: without them the table shows only the engine's columns.
      if(typeof WgLog==='object') WgLog.add('The '+(this.NAMES[tag]||tag)+' columns did not load: '+String(e && e.message || e).replace(/^Wraithguard answered \d+:\s*/,''), 'err');
    }
  },

  /** The patch changed: the columns read before it show the values from before it. The
   *  other tabs' are dropped (asked again when shown); this tab's stay on screen until the
   *  fresh ones arrive, so the table does not flicker back to the engine's columns. */
  refreshColumns(){
    this._colGen=(this._colGen|0)+1;
    const mine=(this._cols||{})[this.tag];
    this._cols=mine? {[this.tag]:mine} : {};
    return this.loadColumns(this.tag, true);
  },

  /** "All fields": Wraithguard searches every field of the tab's records (`editSearch`),
   *  a moment after the typing stops; the rows then are the records that matched. */
  searchSoon(){
    clearTimeout(this._searchT);
    this._searchT=setTimeout(()=>this.runSearch(), 350);
  },
  async runSearch(){
    const q=this.filter.trim(), tag=this.tag;
    if(!q || !this.canEdit()){ this._search=null; this.drawRows(); return; }
    const ask=this._searchN=(this._searchN||0)+1;
    let r;
    try{ r=await this.ask('editSearch', {tag, query:q}); }
    catch(e){ if(ask===this._searchN){ this._search=null; toast(String(e.message||e).replace(/^Wraithguard answered 400:\s*/,''),'err',5000); this.drawRows(); } return; }
    if(ask!==this._searchN || tag!==this.tag) return;          // a later search has it
    this._search={q, tag, hits:r.hits||{}, count:r.count||0, capped:!!r.capped};
    this.drawRows();
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
      own.columns.slice(0, n-2).forEach((c,i)=>cols.push({label:c.label, kind:c.kind, get:r=>val(r,i), title:c.title, path:c.path||null}));
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
    const box=this.part('edTable'); if(!box) return;
    const f=this.filter.trim().toLowerCase();
    // The patch's own records of this type, with the load order's.
    const mine=this.made.filter(p=>p.tag===this.tag).map(p=>[p.id, p.made.name||'', p.made.mesh||'', [], 0]);
    let rows=mine.length? this.rows.concat(mine) : this.rows;
    if(this.patchOnly) rows=rows.filter(r=>this.isEdited(r[0]));
    const S=this.filterAll && f && this._search && this._search.tag===this.tag? this._search : null;
    if(S) rows=rows.filter(r=>S.hits[r[0].toLowerCase()]!=null);
    else if(f && !this.filterAll) rows=rows.filter(r=>r[0].toLowerCase().includes(f) || r[1].toLowerCase().includes(f) || r[2].toLowerCase().includes(f));
    const cols=this.objColumns();
    // What matched, beside the ID, while searching every field.
    if(S) cols.splice(1, 0, {label:'Matched', kind:'text', get:r=>S.hits[r[0].toLowerCase()]||'', title:'The first field that matched the search'});
    const c=Math.min(this.sort.col, cols.length-1), dir=this.sort.dir, col=cols[c];
    const key=r=>{ const v=col.get(r); return col.kind==='defs'? v.length : (typeof v==='number'? v : String(v==null? '' : v).toLowerCase()); };
    rows=rows.slice().sort((a,b)=>{ const x=key(a), y=key(b); return (x<y?-1:x>y?1:0)*dir; });
    (this.part('edCount')||{}).textContent=rows.length+(rows.length===1?' record':' records')+(S && S.capped? ' (of '+S.count+')' : '')+
      (this.filterAll && f && !S? ' - searching…' : '');
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
    // The rows in the order shown: a Shift+click selects the run between two.
    this._shown=rows.slice(0,this.limit('objects')).map(r=>r[0]);
    if(this._rowsSel){ const there=new Set(rows.map(r=>r[0])); for(const id of [...this._rowsSel]) if(!there.has(id)) this._rowsSel.delete(id); }
    for(const r of rows.slice(0,this.limit('objects'))){
      const ed=this.isEdited(r[0]);
      h+='<tr class="'+(ed?'edited':'')+'" data-id="'+escHtml(r[0])+'">'+cols.map(t=>cell(t, r)).join('')+'</tr>';
    }
    h+='</tbody></table>';
    const P=this._rowsProblem && this._rowsProblem.tag===this.tag && !this.rows.length? this._rowsProblem : null;
    if(P) h+='<div class="edEmpty err" style="padding:8px 10px;color:var(--err,#e06c6c)">The list did not load: '+escHtml(P.text)+' <button class="btn sm" data-reload>Reload</button></div>';
    h+=this.moreHtml(rows.length, 'objects');
    const keep=box.scrollTop;
    box.innerHTML=h;
    box.scrollTop=keep;
    WgUI.sizeColumns(box.querySelector('table'), 'objects:'+this.tag+':'+cols.length);
    this.wireMore(box, 'objects', rows.length, ()=>this.drawRows());
    const rl=box.querySelector('[data-reload]'); if(rl) rl.onclick=()=>this.worldChanged();
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
      tr.title=(canPlace? 'Double-click: open the record. Drag into the render window, or right-click, to place one' : 'Double-click: open the record')+
        '. Drag onto a field of a record to fill it. Ctrl+click or Shift+click for several (right-click: set a field on them all)';
      WgUI.armDrag(tr, card);
      tr.oncontextmenu=e=>{
        e.preventDefault();
        const many=this.selectedRows();
        if(many.length>1 && many.includes(tr.dataset.id)) return this.rowsMenu(e, many);
        const p=placing(); if(p && canPlace) this.placeAt(p, null, null);
      };
      tr.onmouseenter=()=>{ const p=card(); if(p) WgUI.hoverPreview(tr, p); };
      tr.onmouseleave=()=>WgUI.hideHover();
      tr.onclick=e=>{
        this.pickRow(tr.dataset.id, e||{});
        box.querySelectorAll('tr[data-id]').forEach(x=>x.classList.toggle('sel', this._rowsSel.has(x.dataset.id)));
        WgUI.hideHover(); WgUI.selectedPreview(card());
      };
      if(this._rowsSel? this._rowsSel.has(tr.dataset.id) : this._rowSel===tr.dataset.id) tr.classList.add('sel');
      tr.ondblclick=e=>{
        // A cell showing a field as it is: edited in place (on every selected row).
        const td=e && e.target && e.target.closest && e.target.closest('td');
        const col=td? cols[[...tr.children].indexOf(td)] : null;
        if(col && col.path && this.canEdit() && this.links().editSetMany) return this.editCell(td, tr.dataset.id, col);
        const r=this.rows.find(x=>x[0]===tr.dataset.id);
        this.openRecord(this.tag, tr.dataset.id, r? r[3].map(i=>this.plugins[i]).filter(Boolean) : null);
      };
    });
  },

  isEdited(id){ return this.edited.has(this.tag+':'+String(id).toLowerCase()); },

  /* ---- saved searches ----------------------------------------------------------------- */

  /** The searches kept by name (this viewer's): `{name: {tag, query, all}}`. */
  savedSearches(){
    try{ const v=JSON.parse(localStorage.getItem('wgEditorSearches')||'{}'); return v && typeof v==='object'? v : {}; }
    catch(_){ return {}; }
  },
  keepSearches(v){ try{ localStorage.setItem('wgEditorSearches', JSON.stringify(v)); }catch(_){ } },

  /** ★ beside the filter: keep this search, or run (or forget) one. */
  savedMenu(e){
    const all=this.savedSearches(), names=Object.keys(all).sort((a,b)=>a.localeCompare(b));
    const r=e.target.getBoundingClientRect? e.target.getBoundingClientRect() : {left:e.clientX, bottom:e.clientY};
    WgUI.menu({x:r.left, y:r.bottom}, [
      {head:'Saved searches'},
      {label:'Keep this search…', disabled:!this.filter.trim(), act:async()=>{
        const name=await WgUI.askText('Keep this search as', '', 'a name, e.g. Heavy weapons');
        if(!name) return;
        const v=this.savedSearches(); v[name]={tag:this.tag, query:this.filter, all:!!this.filterAll}; this.keepSearches(v);
        toast('Kept as '+name,'ok',1500);
      }},
      ...(names.length? [{sep:true}] : []),
      ...names.map(n=>({label:n, title:(this.NAMES[all[n].tag]||all[n].tag)+': '+all[n].query+(all[n].all? ' (all fields)' : ''), act:()=>this.runSaved(all[n])})),
      ...(names.length? [{label:'Forget one', sub:names.map(n=>({label:n, act:()=>{ const v=this.savedSearches(); delete v[n]; this.keepSearches(v); }}))}] : []),
    ]);
  },
  /** A saved search run: its tab, its text, "All fields" as it was. */
  async runSaved(s){
    const d=this.el; if(!d || !s) return;
    if(s.tag && s.tag!==this.tag){ this.tag=s.tag; this.resetLimit('objects'); this.fillTabs(); await this.loadTag(this.tag); }
    const f=d.querySelector('#edFilter'), a=d.querySelector('#edFilterAll');
    f.value=s.query||''; this.filter=f.value;
    if(a && a.checked!==!!s.all){ a.checked=!!s.all; a.onchange({target:a}); }
    else{ this._search=null; this.resetLimit('objects'); if(this.filterAll) this.searchSoon(); else this.drawRows(); }
  },

  /* ---- many rows at once ------------------------------------------------------------ */

  /** A click on a row: it alone; Ctrl+click adds or takes it away; Shift+click the run
   *  from the last one clicked. */
  pickRow(id, e){
    const sel=this._rowsSel=this._rowsSel||new Set();
    if(e.shiftKey && this._rowSel && (this._shown||[]).includes(this._rowSel)){
      const a=this._shown.indexOf(this._rowSel), b=this._shown.indexOf(id);
      if(!(e.ctrlKey||e.metaKey)) sel.clear();
      for(let i=Math.min(a,b); i<=Math.max(a,b); i++) sel.add(this._shown[i]);
      return;
    }
    if(e.ctrlKey||e.metaKey){ if(sel.has(id)) sel.delete(id); else sel.add(id); }
    else{ sel.clear(); sel.add(id); }
    this._rowSel=id;
  },
  /** The selected rows' ids, in the order shown. */
  selectedRows(){
    const sel=this._rowsSel||new Set();
    return (this._shown||[]).filter(id=>sel.has(id));
  },

  /** The right-click on several selected rows. */
  rowsMenu(e, ids){
    WgUI.menu({x:e.clientX, y:e.clientY}, [
      {head:ids.length+' '+(this.NAMES[this.tag]||this.tag)+' records'},
      {label:'Set a field on all of them…', act:()=>this.setManyAsk(ids, this.tag), disabled:!this.links().editSetMany},
      {label:'Open the first', act:()=>this.openRecord(this.tag, ids[0], null)},
      {label:'Clear the selection', act:()=>{ this._rowsSel.clear(); this.drawRows(); }},
    ]);
  },

  /** The fields a record of the tab has that can be changed, from one of them (`{path,
   *  value, options}`), read once per tab. */
  async editableFields(id, tag=this.tag){
    this._fieldsOf=this._fieldsOf||{};
    if(this._fieldsOf[tag]) return this._fieldsOf[tag];
    const v=await this.ask('editRecord', {tag, id});
    const out=(v.fields||[]).filter(f=>f.editable && !Array.isArray(f.value) && (f.value==null || typeof f.value!=='object'));
    this._fieldsOf[tag]=out;
    return out;
  },

  /** "Set a field on all of them": which field (offered from the type's), what value. */
  async setManyAsk(ids, tag=this.tag){
    // The type and the ids are the ones the menu opened on: a tab switched while the prompts
    // are up must not turn the request into one about other records.
    let fields=[];
    try{ fields=await this.editableFields(ids[0], tag); }catch(_){ }
    const path=await WgUI.askText('Set which field on '+ids.length+' records?', '', 'a field, e.g. data.value', fields.map(f=>f.path));
    if(!path) return null;
    const f=fields.find(x=>x.path===path);
    const value=await WgUI.askText(path+' on '+ids.length+' records', f && f.value!=null? String(f.value) : '', 'the value', f && f.options && f.options.length? f.options : null);
    if(value==null) return null;
    return this.setMany(ids, path, value, tag);
  },

  /** `editSetMany`: one value on many records, one step (Ctrl+Z puts all of them back). */
  async setMany(ids, path, value, tag=this.tag){
    let r;
    try{ r=await this.ask('editSetMany', {tag, ids, path, value}); }
    catch(e){ toast(String(e.message||e).replace(/^Wraithguard answered 400:\s*/,''),'err',6000); return null; }
    const left=(r.failed||[]).length+(r.missing||[]).length;
    toast(path+' set on '+r.changed+' record'+(r.changed===1?'':'s')+(left? ' - '+left+' left as they were (see Messages)' : ''), left? 'warn' : 'ok', 4000);
    for(const f of r.failed||[]) if(typeof WgLog==='object') WgLog.add(f.id+': '+f.error, 'warn');
    await this.refreshPending();
    return r;
  },

  /** A cell of the table edited where it is: Enter sets it (on every selected row, when
   *  this one is among them), Esc leaves it. */
  editCell(td, id, col){
    if(td.querySelector('input')) return;
    const sel=this.selectedRows(), ids=sel.length>1 && sel.includes(id)? sel : [id], tag=this.tag;
    const before=td.textContent;
    td.innerHTML='<input class="fld edCellIn" spellcheck="false">';
    const inp=td.firstChild; inp.value=before;
    inp.title=ids.length>1? 'Enter: '+col.path+' on the '+ids.length+' selected records' : 'Enter: set '+col.path;
    const done=async ok=>{
      if(!inp.isConnected) return;
      const v=inp.value;
      td.textContent=before;
      if(ok && v!==before) await this.setMany(ids, col.path, v, tag);
    };
    inp.onkeydown=e=>{ e.stopPropagation(); if(e.key==='Enter') done(true); if(e.key==='Escape') done(false); };
    inp.onblur=()=>done(false);
    inp.focus(); inp.select();
  },

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
    const box=this.part('edNewId'); if(!box) return null;
    const newId=box.value.trim();
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
});
