
/* =====================================================================================
   The full help - Wraithguard. The game's Toggle Full Help, and more, on the right of the
   viewport while the object inspector (24_ori.js) is on the left:

   - the reference's own fields: its owner (an NPC, or a faction and the rank that may
     take it), the global that makes it owned, its lock, key and trap, soul, charge, uses
     and count;
   - the base record: its name and script; an actor's race, class, faction, level, AI,
     the services it sells and the places it takes travellers; a spawn point's leveled
     list, each entry with its level, and the chance it gives nothing;
   - what it holds - a container's contents (owned with the container, as in the game) or
     an actor's inventory - as the game's own icons, a count on each, a leveled list's
     entries under it; a click opens the item in Wraithguard's conflict viewer (when the
     viewer came from Wraithguard), and "View contents" shows every item's mesh in the
     mesh viewer, with a way back to the cell where the camera was;
   - its spells, and on request its dialogue: the topics it has lines of its own in and
     the quests those lines touch.

   The engine reads all of it from the plugin files when an object is clicked (`ori`,
   `ori_dialogue`; viewcore inspect.rs).

   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
const Tfh={
  el:null, _r:null,

  box(){
    if(this.el) return this.el;
    const d=document.createElement('div');
    d.id='tfhPanel'; d.className='ori'; d.hidden=true;
    d.innerHTML='<div class="orihead"><b id="tfhTitle">Full help</b><span style="flex:1"></span>'+
      '<button class="btn dim ic" id="tfhX" title="Close">&#x2715;</button></div>'+
      '<div class="oribody" id="tfhBody"></div>';
    ($('#vpwrap')||document.body).appendChild(d);
    d.querySelector('#tfhX').onclick=()=>this.hide();
    this.el=d;
    return d;
  },
  hide(){ if(this.el) this.el.hidden=true; },
  loading(hit){
    const d=this.box();
    d.querySelector('#tfhTitle').textContent=(hit&&hit.id)||'Full help';
    d.querySelector('#tfhBody').innerHTML='<div class="hint">…</div>';
    d.hidden=false;
  },

  /** Fills the panel from the `ori` answer; hidden when there is nothing to say. */
  show(r){
    this._r=r;
    const d=this.box();
    const h=this.render(r);
    if(!h){ this.hide(); return; }
    d.querySelector('#tfhTitle').textContent=((r.base&&r.base.name)||r.name||r.id||'Full help');
    const body=d.querySelector('#tfhBody');
    body.innerHTML=h;
    d.hidden=false;
    Ori.wire(body, r);
    this.icons(body);
    if(typeof OriAcc==='object') OriAcc.apply(body,'tfh');
    const dlg=body.querySelector('#tfhDialogue');
    if(dlg) dlg.onclick=async()=>{
      const box=body.querySelector('#tfhDialogueBox');
      dlg.disabled=true; box.innerHTML='<div class="hint">Reading the dialogue of every plugin…</div>';
      try{
        const dd=await Engine.call('ori_dialogue',{id:r.id});
        box.innerHTML=this.renderDialogue(dd);
        Ori.wire(box, r);
        if(typeof OriAcc==='object') OriAcc.apply(box,'tfh-dialogue');
      }catch(e){ box.innerHTML='<div class="hint">'+escHtml(String(e.message||e))+'</div>'; }
      dlg.hidden=true;
    };
    const view=body.querySelector('#tfhView');
    if(view) view.onclick=()=>this.viewContents(r);
  },

  /** The game's icons, fetched after the panel is drawn (the engine resolves `icons\`). */
  icons(body){
    body.querySelectorAll('img[data-icon]').forEach(img=>{
      const p=img.dataset.icon;
      if(!p || typeof loadTexture!=='function') return;
      loadTexture('icons\\'+p).then(t=>{ if(t && t.thumb) img.src=t.thumb; else img.replaceWith(Object.assign(document.createElement('div'),{className:'ph'})); })
        .catch(()=>{});
    });
  },

  /** One item as a tile: its icon, its count, its name; a leveled list dashed. */
  tile(it){
    const title=(it.name? it.name+' - ' : '')+it.id+(it.tag? ' ('+it.tag+')' : '')+
      (it.count<0? ' - restocks '+(-it.count) : '')+(Ori.recordLink()? '\nClick: show in Wraithguard\'s conflict viewer' : '');
    const n=it.count==null? '' : it.count<0? '↻'+(-it.count) : (it.count>1? String(it.count) : '');
    return '<div class="tfhitem'+(it.list? ' list' : '')+'"'+(Ori.recordLink()? ' data-rec="'+escHtml(it.id)+'" data-tag="'+escHtml(it.tag||'')+'"' : '')+
      ' title="'+escHtml(title)+'">'+
      (it.icon? '<img data-icon="'+escHtml(it.icon)+'" alt="">' : '<div class="ph"></div>')+
      (n? '<span class="n">'+escHtml(n)+'</span>' : '')+
      '<div class="l">'+escHtml(it.name||it.id)+'</div></div>';
  },

  /** A leveled list: its entries by level, and what it rolls. */
  renderList(l){
    if(!l) return '';
    let h='';
    const notes=[];
    if(l.chanceNone) notes.push(l.chanceNone+'% chance of nothing');
    for(const f of (l.flags||[])) notes.push(f);
    if(notes.length) h+='<div class="hint">'+escHtml(notes.join(' · '))+'</div>';
    const byLevel=new Map();
    for(const e of (l.entries||[])){ if(!byLevel.has(e.level)) byLevel.set(e.level,[]); byLevel.get(e.level).push(e); }
    for(const [lv,es] of [...byLevel.entries()].sort((a,b)=>a[0]-b[0]))
      h+='<div class="from" style="margin-top:4px">Level '+lv+'</div><div class="tfhgrid">'+es.map(e=>this.tile(e)).join('')+'</div>';
    return h;
  },

  render(r){
    const row=(k,v)=>'<div class="orirow"><span class="k">'+escHtml(k)+'</span><span class="v">'+v+'</span></div>';
    const rd=r.refData||{}, b=r.base||{};
    const id=(t,x)=>Ori.recId(t,x);
    let own='';
    if(rd.owner) own+=row('Owner', id('NPC_', rd.owner)+(rd.ownerName? ' '+escHtml(rd.ownerName) : ''));
    if(rd.faction) own+=row('Faction owner', id('FACT', rd.faction)+(rd.rank!=null && rd.rank>=0? ' <span class="from">rank '+rd.rank+' and up may take it</span>' : ''));
    if(rd.ownerGlobal) own+=row('Owned while', id('GLOB', rd.ownerGlobal)+' <span class="from">is 0 (a rented bed, a gift)</span>');
    if(rd.lock!=null) own+=row('Lock', rd.lock>0? 'level '+rd.lock : 'locked (level 0)');
    if(rd.key) own+=row('Key', id('MISC', rd.key));
    if(rd.trap) own+=row('Trap', id('SPEL', rd.trap));
    if(rd.soul) own+=row('Soul', id('CREA', rd.soul));
    if(rd.charge!=null) own+=row('Charge', escHtml(String(rd.charge)));
    if(rd.uses!=null) own+=row('Health / uses', escHtml(String(rd.uses)));
    if(rd.count!=null && rd.count!==1) own+=row('Count', escHtml(String(rd.count)));
    if(rd.blocked) own+=row('Blocked', 'yes');
    const actor=b.tag==='NPC_'||b.tag==='CREA';
    let base='';
    if(b.name) base+=row('Name', escHtml(b.name));
    if(b.script) base+=row('Script', id('SCPT', b.script));
    if(actor){
      if(b.race) base+=row('Race', id('RACE', b.race)+(b.tag==='NPC_'? (b.female? ' · female' : ' · male') : ''));
      if(b.class) base+=row('Class', id('CLAS', b.class));
      if(b.faction) base+=row('Faction', id('FACT', b.faction));
      if(b.level!=null) base+=row('Level', escHtml(String(b.level)));
      if(Array.isArray(b.ai)) base+=row('AI', 'hello '+b.ai[0]+' · fight '+b.ai[1]+' · flee '+b.ai[2]+' · alarm '+b.ai[3]);
    }
    if(b.capacity!=null) base+=row('Capacity', escHtml(String(b.capacity)));
    if((b.flags||[]).length) base+=row('Flags', escHtml(b.flags.join(', ')));
    const items=b.items||[];
    let h='';
    if(own) h+='<div class="orisec">Reference</div>'+own;
    if(base) h+='<div class="orisec">'+escHtml(b.tag||'Record')+'</div>'+base;
    if((b.services||[]).length) h+='<div class="orisec">Services</div><div>'+escHtml(b.services.join(', '))+'</div>';
    if((b.travel||[]).length){
      h+='<div class="orisec">Travel to</div>'+b.travel.map(t=>{
        const m=/^(-?\d+), (-?\d+)$/.exec(t);
        const g=m && typeof GameData==='object' && GameData.grid? GameData.grid.get(m[1]+','+m[2]) : null;
        return '<div>'+escHtml(g&&g.name? g.name+' ('+t+')' : t)+'</div>';
      }).join('');
    }
    if(b.list) h+='<div class="orisec">Leveled list</div>'+this.renderList(b.list);
    if(items.length || b.tag==='CONT'){
      h+='<div class="orisec">'+(b.tag==='CONT'? 'Contents' : 'Inventory')+'</div>';
      if(b.tag==='CONT' && (rd.owner||rd.faction))
        h+='<div class="hint">Owned with the container: '+escHtml(rd.ownerName||rd.owner||rd.faction)+'.</div>';
      if(items.length){
        h+='<div class="tfhgrid">'+items.map(it=>this.tile(it)).join('')+'</div>';
        for(const it of items) if(it.list)
          h+='<div class="from" style="margin-top:6px">'+escHtml(it.name||it.id)+' (leveled)</div>'+this.renderList(it.list);
        if(items.some(it=>it.model || (it.list && it.list.entries.some(e=>e.model))))
          h+='<div style="margin-top:6px"><button class="btn sm" id="tfhView" title="Every item\'s mesh in the mesh viewer; Back to cell returns here">View contents</button></div>';
      } else h+='<div class="hint">Empty.</div>';
    }
    if((b.spells||[]).length) h+='<div class="orisec">Spells</div>'+b.spells.map(x=>id('SPEL',x)).join('<br>');
    if(actor) h+='<div class="orisec">Dialogue</div><button class="btn sm" id="tfhDialogue">Topics and quests…</button><div id="tfhDialogueBox"></div>';
    return h;
  },

  renderDialogue(d){
    const kinds=['Topics','Voice','Greetings','Persuasion','Journal'];
    let h='';
    const topics=(d&&d.topics)||[];
    if(!topics.length) h+='<div class="hint">No lines of its own in any plugin.</div>';
    for(let k=0;k<kinds.length;k++){
      const rows=topics.filter(t=>t.kind===k);
      if(!rows.length) continue;
      h+='<div class="orisec">'+kinds[k]+'</div>';
      for(const t of rows)
        h+='<div class="orirow"><span class="k">'+Ori.recId('DIAL',t.topic)+'</span><span class="v">'+
           t.lines+' line'+(t.lines===1?'':'s')+' · <span class="from">'+escHtml((t.plugins||[]).join(', '))+'</span></span></div>';
    }
    const q=(d&&d.quests)||[];
    h+='<div class="orisec">Quests its lines touch</div>';
    h+=q.length? q.map(x=>Ori.recId('DIAL',x)).join('<br>') : '<div class="hint">None.</div>';
    return h;
  },

  /** The contents' meshes in the mesh viewer, and back to the cell after. */
  viewContents(r){
    const b=(r&&r.base)||{};
    const seen=new Set(), list=[];
    const add=(it)=>{ if(!it.model) return; const k=it.model.toLowerCase(); if(seen.has(k)) return; seen.add(k);
      list.push({path:it.model, label:(it.name||it.id)+(it.count>1? ' ×'+it.count : '')}); };
    for(const it of (b.items||[])){ add(it); if(it.list) for(const e of it.list.entries) add(e); }
    if(!list.length || typeof WgMeshView!=='object') return;
    Ori.hide();
    WgMeshView.openFrom(list, (b.name||r.id||'')+' - contents');
  },
};
