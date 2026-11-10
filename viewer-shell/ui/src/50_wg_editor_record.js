/* =====================================================================================
   Wraithguard Editor (50_wg_editor.js): the record dialog: a record's fields (its CS form, 53_wg_record_forms.js, around them), lists, renaming, and every change sent to the pool.

   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
Object.assign(WgEditor, {
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

  /** The plugin being edited (the Construction Set's active file), by name; '' for none: the
   *  pool then makes a new patch. Kept between runs. */
  activePlugin(){ try{ return String(localStorage.getItem('wgEditorActive')||''); }catch(_){ return ''; } },
  /** The Editor's load order with the active plugin last, as the Construction Set loads its
   *  active file: a record is then edited as that plugin has it, whatever loads after it in
   *  the game. The setup's plugins (`[[name, path]]`) are kept for the Build panel. */
  withActiveLast(ed){
    this._edPlugins=ed.plugins.slice();
    const want=this.activePlugin().toLowerCase();
    const at=want? ed.plugins.findIndex(p=>String(p[0]).toLowerCase()===want) : -1;
    this._activeAt=at;
    if(at<0) return ed.plugins;
    const order=ed.plugins.slice(); const [mine]=order.splice(at, 1); order.push(mine);
    return order;
  },
  /** The journal's pool was made editing another plugin than the one chosen here: that one
   *  is chosen, and the Editor opens on its load order (once). With an empty pool the
   *  journal takes this side's choice. */
  async followPoolPlugin(){
    if(this._followed) return; this._followed=true;
    try{
      const mine=this.activePlugin();
      const r=await this.ask('editActive', {name:mine});
      if(!r.differs) return;
      try{ if(r.active) localStorage.setItem('wgEditorActive', r.active); else localStorage.removeItem('wgEditorActive'); }catch(_){ }
      this._native=null; this._ownLinks=null;
      await this.nativeLinks();
      toast('The pool was made editing '+(r.active||'a new patch')+': that is the plugin being edited','ok',5000);
    }catch(_){ }
  },

  /** Chooses the plugin to edit ('' for a new patch): the Editor opens again on the load
   *  order with it last. The pool is kept (the journal is the same). */
  async setActivePlugin(name){
    // The pool is made against the load order with its plugin last: one that holds changes
    // goes to another plugin only when asked.
    if(this.links().editActive){
      try{
        let r=await this.ask('editActive', {name});
        if(r.differs){
          const ok=await wgConfirm('The pool holds '+r.records+' change'+(r.records===1?'':'s')+' made editing '+(r.active||'a new patch')+'.\n\nKeep them and write them into '+(name||'a new patch')+' instead? Each was made from the record as '+(r.active||'the load order')+' had it.\n\nCancel leaves the choice as it was.', 'Edit a plugin');
          if(!ok){ if(this.bld && !this.bld.hidden) await this.showBuild(); return; }
          await this.ask('editActive', {name, force:true});
        }
      }catch(e){ toast(String(e.message||e),'err',5000); return; }
    }
    try{ if(name) localStorage.setItem('wgEditorActive', name); else localStorage.removeItem('wgEditorActive'); }catch(_){ }
    if(this._cols) this._cols={};
    this._native=null; this._ownLinks=null;
    await this.nativeLinks();
    this._buildPath='';
    await this.refreshPending();
    if(this.bld && !this.bld.hidden) await this.showBuild();
    toast(name? name+' is the plugin being edited: it is read last, and Build patch writes into it' : 'A new patch: the load order as it is','ok',4000);
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

  /** The cell on screen, as Wraithguard keys it: an interior's name, the exterior under
   *  the view's pivot as "(x, y)" - `{key, label}`, or null with nothing loaded. */
  currentCell(){
    const sc=App._scene, cells=(sc && sc.cells)||[];
    if(!cells.length) return null;
    const i=cells.find(c=>c.kind==='int');
    if(i) return {key:String(i.name), label:String(i.name)};
    const R=App.R, o=sc.origin||[0,0];
    let gx=cells[0].gx, gy=cells[0].gy;
    if(R && R.cam && typeof CELL==='number'){ gx=Math.floor((R.cam.tx+o[0])/CELL); gy=Math.floor((R.cam.ty+o[1])/CELL); }
    const c=cells.find(c=>c.gx===gx && c.gy===gy)||cells[0];
    return {key:'('+c.gx+', '+c.gy+')', label:(c.name? c.name+' ' : '')+'('+c.gx+', '+c.gy+')'};
  },

  /** "Make a copy as" for a whole interior (the Cell View's right-click). */
  async copyCell(name){
    const to=await WgUI.askText('Make a copy of '+name+' as', name+' copy', 'The new cell\'s name');
    if(to==null) return;
    try{
      const r=await this.ask('editDuplicateCell', {cell:name, newName:to});
      toast(r.cell+': '+r.refs+' reference'+(r.refs===1?'':'s')+(r.pathgrid? ' and its path grid' : '')+' queued in the patch. It is drawn once the patch is built and loaded','ok',7000);
      await this.refreshPending();
      this.revealCell('int:'+r.cell);   // in the Cell View, selected, wherever it sorts
    }catch(e){ toast(String(e.message||e).replace(/^Wraithguard answered 400:\s*/,''),'err',6000); }
  },

  /** What the renderer keeps lit as selected (`R.setSelection`): the objects picked
   *  together, else the one picked - whatever the hover highlight is doing. */
  syncSel(){
    const R=App.R; if(!R || !R.setSelection) return;
    if(!this.on){ R.setSelection(null); return; }
    R.setSelection(this._group.length? this._group.map(g=>g.hit) : (this._sel && this._sel.hit? [this._sel.hit] : null));
  },

  /** One Editor link. Any that takes 1.5 s or more is said in Messages with its name and the
   *  links that were still out when it began: the Editor answers one link at a time, so a
   *  slow one (a type's columns, a cell read) holds the others, and this names which. */
  async ask(link, body){
    const out=this._out||(this._out=new Map()), tok={}, t0=Date.now();
    const others=[...out.values()];
    out.set(tok, link);
    try{ return await this._ask(link, body); }
    finally{
      out.delete(tok);
      const ms=Date.now()-t0;
      if(ms>=1500 && typeof WgLog==='object' && WgLog.add)
        WgLog.add('Editor link '+link+' took '+(ms/1000).toFixed(1)+' s'+(others.length? ' (out when it began: '+others.join(', ')+')' : ''), 'warn');
    }
  },

  async _ask(link, body){
    const url=this.links()[link];
    if(!url) throw new Error('Editing needs the Editor open on an OpenMW setup (see Messages)');
    /* The links the viewer's own Editor answers (editcore::links) go to the engine, the same
       JSON in and out; the rest to Wraithguard. Both write the patch queue's journal, so
       either sees what the other changed. Wraithguard hands the viewer's links over as
       `native:<key>`: it no longer serves them. */
    const own=String(url).startsWith('native:'), native=(await this.nativeLinks()).has(link);
    if(own && !native) throw new Error('This viewer cannot answer '+link+' itself: it is older than Wraithguard - update the viewer');
    const t=native
      ? await Engine.call('editor_link',{name:link, body:JSON.stringify(body)})
      : await Engine.call('wg_post',{url, body:JSON.stringify(body)});
    if(typeof t!=='string' || !t.length) return t;
    try{ return JSON.parse(t); }catch(_){ return t; }      // `ok` and the like
  },

  /** The links the viewer's own Editor answers: opened once (`editor_open`) on the load
   *  order, the journal, the data folders, the patch last written and Test in OpenMW's
   *  settings file the launch named (`extra.editor`), a set of link names. None
   *  without them, or with an engine that has no Editor (an older build), said in
   *  Messages: Wraithguard answers every link then. */
  nativeLinks(){
    if(!this._native){
      const extra=(window.__WG_VIEW__||{}).extra||{};
      // No host (the program on its own): the setup comes from the install the viewer has
      // open (editor_setup), and the links are the Editor's own.
      const own=!(extra.links && extra.links.editRecord);
      const setup=(extra.editor && Array.isArray(extra.editor.plugins))? Promise.resolve(extra.editor)
        : own? Engine.call('editor_setup',{}).then(t=>typeof t==='string'? JSON.parse(t) : t) : Promise.resolve(null);
      this._native=setup.then(ed=>(!ed || !Array.isArray(ed.plugins))? new Set()
        : Engine.call('editor_open',{plugins:this.withActiveLast(ed), journal:String(ed.journal||''), folders:ed.folders||[],
            last:String(ed.last||''), testSettings:String(ed.testSettings||'')})
          .then(t=>new Set(typeof t==='string'? JSON.parse(t) : t))
          .then(names=>{
            if(own && names.size){ this._ownLinks={}; names.forEach(n=>{ this._ownLinks[n]='native:'+n; }); }
            // Land the patch already changes (a journal from an earlier run) drawn as it will be.
            if(names.has('editLandPending') && typeof WgLand==='object') setTimeout(()=>WgLand.sync(), 0);
            // The pool's own plugin (the journal names it) is the one edited: a pool left from
            // a run with another chosen opens on that one again.
            if(names.has('editActive')) setTimeout(()=>this.followPoolPlugin(), 0);
            return names;
          }))
          .catch(e=>{
            if(typeof WgLog==='object' && WgLog.add) WgLog.add('The Editor did not open on this install ('+String(e && e.message || e)+')'+(own? ': editing is off' : ': Wraithguard answers every link'), 'warn');
            return new Set();
          });
    }
    return this._native;
  },

  async openRecord(tag, id, plugins){
    this.record={tag, id, plugins:plugins||null}; this.refRec=null;
    const d=this.dialog();
    d.hidden=false;
    d.querySelector('#edDlgTitle').textContent=(this.NAMES[tag]||tag)+' - '+id;
    const body=d.querySelector('#edDlgBody');
    if(!this.canEdit()){
      body.innerHTML='<div class="hint">Editing needs the Editor open on an OpenMW setup (see Messages), '+
        'and the record\'s fields open here, with changes going to its patch pool.</div>';
      return;
    }
    body.innerHTML='<div class="hint"></div>';
    const mine=this.record;
    /* What the hint says follows what is happening. A click that names the plugins
       (`plugins`, the row's `defined`) reads only those; one that does not reads the type
       from every plugin of the load order, which on a 600-plugin order is slow the first
       time and quick after. And if the tab's columns (`editColumns`, every plugin's records
       of the type) are still being read, say that is what is awaited rather than a read of
       this record. The engine now reads columns without the Editor's lock, so an Open no
       longer queues behind them, but one that is still out is worth saying, not hiding
       behind a sentence that never changes. */
    const named=!!(plugins && plugins.length);
    const t0=Date.now(), hint=body.querySelector('.hint');
    const say=()=>{
      const s=Math.round((Date.now()-t0)/1000);
      const cols=this._colAsk && this._colAsk.size>0;
      let t=named? 'Opening '+id+'…' : 'Reading '+id+' from every plugin that defines it…';
      if(s>=1 && cols) t='Waiting for the list\'s columns to finish loading… '+s+' s';
      else if(s>=2) t+=' '+s+' s'+((!named && s>=6)? ' (the first lookup of a record type reads every plugin of the load order; later ones are quick)' : '');
      hint.textContent=t;
    };
    say();
    const tick=setInterval(()=>{
      if(this.record!==mine || !hint.isConnected){ clearInterval(tick); return; }
      say();
    }, 1000);
    try{ const v=await this.ask('editRecord', this.req()); if(this.record===mine) this.drawRecord(v); }
    catch(e){ if(this.record!==mine) return; body.innerHTML='<div class="hint">'+escHtml(String(e.message||e))+'</div>'; }
    finally{
      clearInterval(tick);
      const ms=Date.now()-t0;
      if(ms>=3000 && typeof WgLog==='object' && WgLog.add) WgLog.add('Opening '+tag+' '+id+' took '+(ms/1000).toFixed(1)+' s'+(plugins? ' ('+plugins.length+' plugin(s) named)' : ' (read from every plugin)'), 'warn');
    }
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
    if(!this.el || !this.allTabs().some(t=>t[0]===tag)) return false;
    if(this.tag!==tag){
      this.tag=tag;
      this.fillTabs();
      await this.loadTag(tag);
    }
    const low=String(id).toLowerCase();
    const find=()=>[...((this.part('edTable')||document.createElement('div')).querySelectorAll('tr[data-id]'))].find(tr=>tr.dataset.id.toLowerCase()===low);
    let tr=find();
    if(!tr){
      const f=this.part('edFilter');
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
    // A late answer for a record that is no longer the dialog's is dropped, whole.
    const cur=this.record;
    if(!cur || String(cur.tag)!==String(v.tag) || String(cur.id).toLowerCase()!==String(v.id).toLowerCase()) return false;
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
      (Ori.recordLink() && !v.new? '<button class="btn sm" id="edConf" title="This record on the Conflicts page, its fields across its plugins">Conflicts</button> ' : '')+
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
      // A field naming a record: its ids offered as it is typed; a record from the Object
      // Window dropped on it fills it.
      const tags=el.tagName==='INPUT' && el.type==='text'? this.idTags(el.dataset.path) : null;
      if(tags){ el.dataset.ids=tags.join(' '); el.addEventListener('focus', ()=>this.offerIds(el, tags)); }
      if(el.tagName==='TEXTAREA') el.onkeydown=e=>{ if(e.key==='Enter' && (e.ctrlKey||e.metaKey)) el.blur(); e.stopPropagation(); };
    });
    const allF=body.querySelector('details.edAllFields');
    if(allF) allF.addEventListener('toggle', ()=>{ WgUI.state().allFields=allF.open; WgUI.save(); });
    if(typeof WgForms==='object') WgForms.wire(this, body, v);
    if(typeof WgFlow==='object') WgFlow.extendRecord(this, body, v);
  },

  /** The records a field names, by its name (the last part of its path) - what is
   *  offered as it is typed, and what may be dropped on it. */
  ID_FIELDS:{script:['SCPT'], enchanting:['ENCH'], race:['RACE'], class:['CLAS'], faction:['FACT'],
    head:['BODY'], hair:['BODY'], male_bodypart:['BODY'], female_bodypart:['BODY'], sound:['SOUN'],
    open_sound:['SOUN'], close_sound:['SOUN'], cast_sound:['SOUN'], bolt_sound:['SOUN'], hit_sound:['SOUN'],
    area_sound:['SOUN'], sleep_creature:['LEVC'], region:['REGN'], creature:['CREA'], speaker_id:['NPC_','CREA'],
    speaker_race:['RACE'], speaker_class:['CLAS'], speaker_faction:['FACT'], player_faction:['FACT'],
    key:['MISC'], trap:['SPEL','ENCH'], owner:['NPC_'], owner_faction:['FACT'], soul:['CREA']},
  idTags(path){ const last=String(path||'').split('.').pop(); return this.ID_FIELDS[last]||null; },

  /** Every id of a type: the load order's and the patch's own (read once per type). */
  async idsOf(tag){
    this._ids=this._ids||{};
    if(!this._ids[tag]){
      let rows=[];
      try{ rows=(await this.recordRows(tag)).rows; }catch(e){ if(typeof WgLog==='object') WgLog.add('The '+tag+' ids did not load: '+String(e && e.message || e), 'err'); }
      this._ids[tag]=rows.map(r=>r[0]).concat(this.made.filter(m=>m.tag===tag).map(m=>m.id));
    }
    return this._ids[tag];
  },
  async offerIds(el, tags){
    const all=[];
    for(const t of tags) all.push(...await this.idsOf(t));
    WgUI.offer(el, all.sort((a,b)=>a.localeCompare(b)));
  },

  /** A record dropped on a field (from the Object Window): the field set to its id, when
   *  the field names records of its type (or any text field, which names nothing in
   *  particular). */
  dropOnField(el, rec){
    if(!el || el.disabled || !rec) return false;
    const want=(el.dataset.ids||'').split(' ').filter(Boolean);
    if(want.length && !want.includes(rec.tag)){
      toast(el.dataset.path+' names '+want.map(t=>this.NAMES[t]||t).join(' or ')+', not '+(this.NAMES[rec.tag]||rec.tag),'warn',3000);
      return true;
    }
    el.value=rec.id;
    if(el.onchange) el.onchange();
    return true;
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
});
