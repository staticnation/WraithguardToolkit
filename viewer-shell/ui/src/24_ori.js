
/* =====================================================================================
   ORI - the object inspector.

   Click a placed object in the viewport and this panel says what it is and where every
   part of it comes from in the Wraithguard setup, after the console's `ori`: the
   reference and the cell it stands in, the plugin that created it and every plugin that
   changed it, the plugins that define its base record, and the data folder or archive
   its mesh and each texture resolve from. The engine answers (`ori` in the shell).

   Wraithguard: and the game's Toggle Full Help, and more - the reference's owner (an NPC,
   or a faction and the rank that may take it), the global that makes it owned, its lock,
   key and trap, soul, charge, uses and count; the base record's script, and a
   container's contents or an actor's inventory, spells, race, class and faction; and, on
   request, an actor's dialogue across the load order - its topics, how many lines of its
   own each plugin gives it, and the quests those lines touch. The contents of a container
   belong to whoever owns the container, as in the game.

   Every record named here opens in the Editor when it is on, else on the Conflicts page
   (the "Conflicts" buttons, and the ids themselves; `Ori.openRecord`).
   Copyright (c) 2026 StaticNation.
   ===================================================================================== */
/** Wraithguard: the inspector's and the full help's sections as accordions. Every
    `.orisec` heading among `root`'s children becomes the summary of a <details> holding
    what follows it, up to the next heading; whether each is open is remembered by panel
    and title (for the session, and in the page's storage when it has one), so a section
    closed on one object stays closed on the next. */
const OriAcc={
  _open:null,
  state(){
    if(this._open) return this._open;
    let s={};
    try{ s=JSON.parse(localStorage.getItem('wg.oriAcc')||'{}')||{}; }catch(_){ s={}; }
    return (this._open=s);
  },
  save(){ try{ localStorage.setItem('wg.oriAcc', JSON.stringify(this._open||{})); }catch(_){} },
  apply(root, panel){
    if(!root) return;
    const st=this.state();
    const kids=[...root.children];
    for(let i=0;i<kids.length;i++){
      const h=kids[i];
      if(!h.classList || !h.classList.contains('orisec')) continue;
      const det=document.createElement('details');
      det.className='oriacc';
      const key=panel+':'+h.textContent.trim();
      det.open = st[key]!==false;
      const sum=document.createElement('summary');
      sum.className='orisec';
      sum.innerHTML=h.innerHTML;
      det.appendChild(sum);
      root.insertBefore(det,h);
      h.remove();
      for(let j=i+1;j<kids.length && !(kids[j].classList && kids[j].classList.contains('orisec'));j++) det.appendChild(kids[j]);
      det.addEventListener('toggle',()=>{ st[key]=det.open; this.save(); });
    }
  },
};

const Ori={
  el:null,
  _ask:0,
  _hit:null,

  /** The viewport's object click (`App.R.onPick`). A click opens ORI on the object;
      Shift+click on a door goes through it (ORI's own "Go through" button does too). */
  pick(hit,e){
    // Wraithguard's editor: Ctrl+click picks several (50_wg_editor.js); a plain click one.
    if(typeof WgEditor==='object' && WgEditor.on){
      if(e && (e.ctrlKey||e.metaKey)){
        // Ctrl+click adds or takes away; one that hits nothing keeps the selection.
        if(hit) WgEditor.toggleGroup(hit);
        return;
      }
      if(WgEditor._group.length){ WgEditor._group=[]; WgEditor.syncSel(); }
    }
    if(!hit){ this.hide(); return; }
    if(e && e.shiftKey && doorTarget(hit.door)){ this.hide(); goThroughDoor(hit); return; }
    this.show(hit);
  },

  /** The panel, made once, inside the viewport. */
  box(){
    if(this.el) return this.el;
    const d=document.createElement('div');
    d.id='oriPanel'; d.className='ori'; d.hidden=true;
    d.innerHTML='<div class="orihead"><b id="oriTitle"></b><span style="flex:1"></span>'+
      '<button class="btn dim ic" id="oriCopy" title="Copy">&#x2398;</button>'+
      '<button class="btn dim ic" id="oriX" title="Close">&#x2715;</button></div>'+
      '<div class="oribody" id="oriBody"></div>';
    ($('#vpwrap')||document.body).appendChild(d);
    d.querySelector('#oriX').onclick=()=>this.hide();
    d.querySelector('#oriCopy').onclick=()=>{
      const t=d.querySelector('#oriBody').innerText;
      if(navigator.clipboard) navigator.clipboard.writeText(t).then(()=>toast('Copied','ok',1500)).catch(()=>{});
    };
    this.el=d;
    return d;
  },

  hide(){
    if(this.el) this.el.hidden=true;
    if(typeof WgEditor==='object'){ WgEditor._sel=null; if(WgEditor.syncSel) WgEditor.syncSel(); }
    if(typeof Tfh==='object') Tfh.hide();
    // The object's own highlight goes; a highlighted mod's stays.
    if(typeof WgModHl==='object' && WgModHl.active()) WgModHl.apply();
    else if(App.R) App.R.setStaticHighlight(null);
  },

  /** Shows the inspector for a viewport pick (`App.R.onPick`). */
  async show(hit){
    if(!hit || !hit.refKey){ this.hide(); return; }
    // Wraithguard: a reference the editor is adding has no record yet; its dialog shows it.
    if(typeof WgEditor==='object' && WgEditor.showNew(hit)) return;
    this._hit=hit;
    const d=this.box(), ask=++this._ask;
    d.hidden=false;
    d.querySelector('#oriTitle').textContent=hit.id||hit.refKey;
    const body=d.querySelector('#oriBody');
    body.innerHTML='<div class="hint">…</div>';
    if(App.R) App.R.setStaticHighlight([hit]);
    if(typeof Tfh==='object') Tfh.loading(hit);
    let r;
    try{ r=await Engine.call('ori',{key:hit.refKey, model:hit.model||''}); }
    catch(e){ if(ask===this._ask) body.innerHTML='<div class="hint">'+escHtml(String(e.message||e))+'</div>'; return; }
    if(ask!==this._ask) return;          // a later click has the panel now
    body.innerHTML=this.render(r);
    const go=body.querySelector('#oriDoorGo');
    if(go) go.onclick=()=>{ this.hide(); goThroughDoor(hit); };
    this.wire(body, r);
    // Wraithguard: each asset's providers, a compare of the loose versions, Where used.
    if(typeof WgTools==='object') WgTools.decorateOri(body, r);
    // Wraithguard: the Editor mode's "Edit record" (F2) for the clicked object.
    if(typeof WgEditor==='object') WgEditor.decorateOri(body, r);
    OriAcc.apply(body,'ori');
    // The full help - owner, contents, inventory, dialogue - on the right (46_wg_tfh.js).
    if(typeof Tfh==='object') Tfh.show(r, hit);
    // A plugin's name highlights everything it placed in the loaded cells (40_wg_modhl.js).
    body.querySelectorAll('[data-mod]').forEach(b=>{
      b.onclick=()=>{ if(typeof WgModHl==='object') WgModHl.choose(b.dataset.mod===WgModHl.mod? '' : b.dataset.mod); };
    });
  },

  /** Whether the Conflicts page (60_wg_conflicts.js) is in this build: a record link opens
   *  the record there (port step 4.3's flip; it used to ask Wraithguard's conflict window). */
  recordLink(){ return typeof WgConflicts==='object'; },
  /** In the Editor a record link opens the record there instead (50_wg_editor.js). */
  inEditor(){ return typeof WgEditor==='object' && WgEditor.on; },
  /** Whether a record id is a link at all: to the Editor, or to the conflict viewer. */
  linked(){ return this.inEditor() || !!this.recordLink(); },
  /** What following a record link does, for its tooltip. */
  linkNote(){ return this.inEditor()? 'Open this record in the Editor' : 'Show this record on the Conflicts page'; },
  /** Shows the record `tag`/`id`: in the Editor when it is on (its Object Window and its
   *  dialog), else on the Conflicts page. */
  async openRecord(tag,id){
    if(this.inEditor()) return WgEditor.showEntry(tag, id);
    return this.openConflicts(tag, id);
  },
  /** The record on the Conflicts page, its fields across its plugins (the Editor's record
   *  dialog keeps a "Conflicts" button for this). */
  async openConflicts(tag,id){
    if(!this.recordLink() || !id) return;
    return WgConflicts.goto(tag||'', id);
  },
  /** The buttons and links a render put in the panel. */
  wire(body, r){
    body.querySelectorAll('[data-rec]').forEach(el=>{
      el.onclick=e=>{ e.preventDefault(); this.openRecord(el.dataset.tag||'', el.dataset.rec); };
    });
  },
  /** A record id: a link into the Editor or the conflict viewer when there is one. */
  recId(tag,id){
    if(!id) return '—';
    if(!this.linked()) return '<code>'+escHtml(id)+'</code>';
    return '<a class="orimod" data-rec="'+escHtml(id)+'" data-tag="'+escHtml(tag||'')+'" title="'+escHtml(this.linkNote())+'"><code>'+escHtml(id)+'</code></a>';
  },
  conflictBtn(tag,id){
    if(!this.linked() || !id) return '';
    return ' <button class="btn sm" data-rec="'+escHtml(id)+'" data-tag="'+escHtml(tag||'')+'" title="'+escHtml(this.linkNote())+'">'+(this.inEditor()? 'Edit' : 'Conflicts')+'</button>';
  },
  render(r){
    const row=(k,v)=>'<div class="orirow"><span class="k">'+escHtml(k)+'</span><span class="v">'+v+'</span></div>';
    const mono=v=>'<code>'+escHtml(String(v||'—'))+'</code>';
    const list=a=>(a&&a.length)? a.map(x=>mod(x)).join('<br>') : '—';
    // A plugin name, clickable: highlights (or stops highlighting) everything it placed.
    const mod=v=>v? '<a class="orimod" data-mod="'+escHtml(v)+'" title="Highlight everything this plugin placed in the loaded cells">'+escHtml(v)+'</a>' : '—';
    const defs=(r.definedIn||[]);
    let h='';
    h+=row('Reference', mono(r.key));
    const baseTag=defs.length? defs[defs.length-1].tag : '';
    h+=row('Base', mono(r.id)+(baseTag? ' <span class="tag">'+escHtml(baseTag)+'</span>' : '')+
                   (r.name? ' '+escHtml(r.name) : '')+this.conflictBtn(baseTag, r.id));
    h+=row('Cell', escHtml(r.cell||'—')+this.conflictBtn('CELL', r.cellKey||''));
    if(r.doorTo) h+=row('Door to', escHtml(r.doorTo)+
      (this._hit && doorTarget(this._hit.door)? ' <button class="btn sm" id="oriDoorGo">Go through</button>' : ''));
    h+='<div class="orisec">Load order</div>';
    h+=row('Created by', mod(r.createdBy));
    h+=row('Changed by', list(r.touchedBy));
    h+=row('Base defined in', defs.length? defs.map(x=>mono(x.plugin)).join('<br>') : '—');
    h+='<div class="orisec">Assets</div>';
    h+=row('Mesh', mono(r.mesh)+'<br><span class="from">'+escHtml(r.meshFrom||'not found')+'</span>');
    for(const t of (r.textures||[])){
      // Wraithguard: a texture the mesh carries inside itself (NiPixelData) has no file.
      const emb=/^__nifpx\/(\d+)/.exec(String(t.texture||''));
      h+=row('Texture', (emb? mono('embedded (record '+emb[1]+')') : mono(t.texture))+'<br><span class="from">'+escHtml(t.from||'not found')+'</span>');
    }
    h+='<div class="orisec">Placement</div>';
    h+=row('Position', mono((r.pos||[]).join(', ')));
    h+=row('Rotation', mono((r.rot||[]).join(', ')));
    h+=row('Scale', mono(r.scale));
    return h;
  },
};

/* `goThroughDoor` is Gardenfell's (18_cellpreview.js, round 18hf), which this file's
   Shift+click and door button call: the same act, one copy. */
