
/* =====================================================================================
   ORI - the object inspector.

   Click a placed object in the viewport and this panel says what it is and where every
   part of it comes from in the Wraithguard setup, after the console's `ori`: the
   reference and the cell it stands in, the plugin that created it and every plugin that
   changed it, the plugins that define its base record, and the data folder or archive
   its mesh and each texture resolve from. The engine answers (`ori` in the shell).
   Copyright (c) 2026 StaticNation.
   ===================================================================================== */
const Ori={
  el:null,
  _ask:0,
  _hit:null,

  /** The viewport's object click (`App.R.onPick`). A click opens ORI on the object;
      Shift+click on a door goes through it (ORI's own "Go through" button does too). */
  pick(hit,e){
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
    // The object's own highlight goes; a highlighted mod's stays.
    if(typeof WgModHl==='object' && WgModHl.active()) WgModHl.apply();
    else if(App.R) App.R.setStaticHighlight(null);
  },

  /** Shows the inspector for a viewport pick (`App.R.onPick`). */
  async show(hit){
    if(!hit || !hit.refKey){ this.hide(); return; }
    this._hit=hit;
    const d=this.box(), ask=++this._ask;
    d.hidden=false;
    d.querySelector('#oriTitle').textContent=hit.id||hit.refKey;
    const body=d.querySelector('#oriBody');
    body.innerHTML='<div class="hint">…</div>';
    if(App.R) App.R.setStaticHighlight([hit]);
    let r;
    try{ r=await Engine.call('ori',{key:hit.refKey, model:hit.model||''}); }
    catch(e){ if(ask===this._ask) body.innerHTML='<div class="hint">'+escHtml(String(e.message||e))+'</div>'; return; }
    if(ask!==this._ask) return;          // a later click has the panel now
    body.innerHTML=this.render(r);
    const go=body.querySelector('#oriDoorGo');
    if(go) go.onclick=()=>{ this.hide(); goThroughDoor(hit); };
    // A plugin's name highlights everything it placed in the loaded cells (40_wg_modhl.js).
    body.querySelectorAll('[data-mod]').forEach(b=>{
      b.onclick=()=>{ if(typeof WgModHl==='object') WgModHl.choose(b.dataset.mod===WgModHl.mod? '' : b.dataset.mod); };
    });
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
    h+=row('Base', mono(r.id)+(defs.length? ' <span class="tag">'+escHtml(defs[defs.length-1].tag)+'</span>' : '')+
                   (r.name? ' '+escHtml(r.name) : ''));
    h+=row('Cell', escHtml(r.cell||'—'));
    if(r.doorTo) h+=row('Door to', escHtml(r.doorTo)+
      (this._hit && doorTarget(this._hit.door)? ' <button class="btn sm" id="oriDoorGo">Go through</button>' : ''));
    h+='<div class="orisec">Load order</div>';
    h+=row('Created by', mod(r.createdBy));
    h+=row('Changed by', list(r.touchedBy));
    h+=row('Base defined in', defs.length? defs.map(x=>mono(x.plugin)).join('<br>') : '—');
    h+='<div class="orisec">Assets</div>';
    h+=row('Mesh', mono(r.mesh)+'<br><span class="from">'+escHtml(r.meshFrom||'not found')+'</span>');
    for(const t of (r.textures||[]))
      h+=row('Texture', mono(t.texture)+'<br><span class="from">'+escHtml(t.from||'not found')+'</span>');
    h+='<div class="orisec">Placement</div>';
    h+=row('Position', mono((r.pos||[]).join(', ')));
    h+=row('Rotation', mono((r.rot||[]).join(', ')));
    h+=row('Scale', mono(r.scale));
    return h;
  },
};

/** Opens the cell a door leads to and frames the view on its landing marker (DODT), the
    same act as choosing that cell in the picker. The door is remembered in the visit
    history, so stepping back returns to it. */
function goThroughDoor(hit){
  const target=doorTarget(hit && hit.door);
  if(!target) return;
  App.mode='cell';
  App.cellSel=target;
  App._doorAim={pos:[+hit.door.pos[0]||0, +hit.door.pos[1]||0, +hit.door.pos[2]||0],
                rot:(hit.door.rot && +hit.door.rot[2])||0,
                key:target.kind==='int'? 'i:'+(target.name||'') : target.x+','+target.y};
  if(typeof CellHistory==='object')
    App._histVia={via:'door', door:CellHistory.doorSeen(hit),
                  doorAim:{pos:App._doorAim.pos.slice(), rot:App._doorAim.rot}};
  if(typeof syncCellButton==='function') syncCellButton();
  schedulePreview();
}
