/* =====================================================================================
   Wraithguard Editor (50_wg_editor.js): the Object Window: its tabs, rows, search by any field, long lists a page at a time, and new records.

   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
Object.assign(WgEditor, {
  fillTabs(){
    const t=this.part('edTabs'); if(!t) return;
    const changed={};
    for(const k of this.edited){ const tag=k.slice(0,4); changed[tag]=(changed[tag]||0)+1; }
    t.innerHTML=this.tags.map(([tag,n])=>
      '<button class="btn sm'+(tag===this.tag?' on':'')+(changed[tag]?' edited':'')+'" data-tag="'+escHtml(tag)+'" title="'+escHtml(tag)+' - '+n+' records'+(changed[tag]? ', '+changed[tag]+' in the patch' : '')+'">'+
      escHtml(this.NAMES[tag]||tag)+(changed[tag]? ' <span class="from">'+changed[tag]+'</span>' : '')+'</button>').join('');
    t.querySelectorAll('[data-tag]').forEach(b=>b.onclick=()=>{
      this.tag=b.dataset.tag; this.resetLimit('objects');
      t.querySelectorAll('.on').forEach(x=>x.classList.remove('on')); b.classList.add('on');
      this._search=null;
      this.loadTag(this.tag).then(()=>{ if(this.filterAll && this.filter.trim()) this.searchSoon(); });
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
      {label:'Set a field on all of them…', act:()=>this.setManyAsk(ids), disabled:!this.links().editSetMany},
      {label:'Open the first', act:()=>this.openRecord(this.tag, ids[0], null)},
      {label:'Clear the selection', act:()=>{ this._rowsSel.clear(); this.drawRows(); }},
    ]);
  },

  /** The fields a record of the tab has that can be changed, from one of them (`{path,
   *  value, options}`), read once per tab. */
  async editableFields(id){
    this._fieldsOf=this._fieldsOf||{};
    if(this._fieldsOf[this.tag]) return this._fieldsOf[this.tag];
    const v=await this.ask('editRecord', {tag:this.tag, id});
    const out=(v.fields||[]).filter(f=>f.editable && !Array.isArray(f.value) && (f.value==null || typeof f.value!=='object'));
    this._fieldsOf[this.tag]=out;
    return out;
  },

  /** "Set a field on all of them": which field (offered from the type's), what value. */
  async setManyAsk(ids){
    let fields=[];
    try{ fields=await this.editableFields(ids[0]); }catch(_){ }
    const path=await WgUI.askText('Set which field on '+ids.length+' records?', '', 'a field, e.g. data.value', fields.map(f=>f.path));
    if(!path) return null;
    const f=fields.find(x=>x.path===path);
    const value=await WgUI.askText(path+' on '+ids.length+' records', f && f.value!=null? String(f.value) : '', 'the value', f && f.options && f.options.length? f.options : null);
    if(value==null) return null;
    return this.setMany(ids, path, value);
  },

  /** `editSetMany`: one value on many records, one step (Ctrl+Z puts all of them back). */
  async setMany(ids, path, value){
    let r;
    try{ r=await this.ask('editSetMany', {tag:this.tag, ids, path, value}); }
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
    const sel=this.selectedRows(), ids=sel.length>1 && sel.includes(id)? sel : [id];
    const before=td.textContent;
    td.innerHTML='<input class="fld edCellIn" spellcheck="false">';
    const inp=td.firstChild; inp.value=before;
    inp.title=ids.length>1? 'Enter: '+col.path+' on the '+ids.length+' selected records' : 'Enter: set '+col.path;
    const done=async ok=>{
      if(!inp.isConnected) return;
      const v=inp.value;
      td.textContent=before;
      if(ok && v!==before) await this.setMany(ids, col.path, v);
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
