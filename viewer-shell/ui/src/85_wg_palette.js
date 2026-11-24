/* =====================================================================================
   Wraithguard: the command palette (Static 2026-11-24, "lets do all this": "Command palette
   (Ctrl+P). Find any action, record, cell or tool by typing").

   Ctrl+P (remappable under Keys) opens a box over the window; what is typed is matched, word
   by word, against:
   - the pages (Preview, Editor, Land, Load order, Conflicts, Tools, Help, Report, Setup) and
     every tool of the Tools page;
   - the Editor's keyboard actions (`WgKeys.ACTIONS`, run as their own key would run them);
   - the buttons and switches on screen now, by their label (what a click would do);
   - this cell's saved views (84_wg_views.js);
   - the cells of the load order (`GameData.cellList`), opened in the Preview;
   - the records of the load order by id (`record_search`), opened in the Editor.
   Up and Down choose, Enter runs, Escape closes. The list is built with the DOM.
   Copyright (c) 2026 StaticNation.
   ===================================================================================== */
const WgPalette={
  PAGES:[['Preview', null], ['Editor', 'btnEditor'], ['Land', 'btnLand'], ['Load order', 'btnLoadOrder'],
    ['Conflicts', 'btnConflicts'], ['Tools', 'btnTools'], ['Help', 'btnHelp'], ['Report', 'btnReport'], ['Setup', 'btnSetup']],
  _el:null, _items:[], _sel:0, _q:'', _ask:0,

  /** `q`'s words all in `text` (any case): a score, lower is better, or -1. */
  score(text, q){
    const t=String(text||'').toLowerCase(), words=String(q||'').toLowerCase().split(/\s+/).filter(Boolean);
    if(!words.length) return 50;
    let s=0;
    for(const w of words){
      const i=t.indexOf(w);
      if(i<0) return -1;
      s+= i===0? 0 : /[\s_\-.(/›]/.test(t[i-1])? 1 : 3;
    }
    return s+Math.min(20, t.length/20);
  },
  /** A label for a control: its own text, or its row's label (a switch). */
  labelOf(el){
    const own=(el.textContent||'').replace(/\s+/g, ' ').trim();
    if(el.tagName==='INPUT'){
      const row=el.closest('.row'), l=row && row.querySelector('label:not(.sw)');
      return l? l.textContent.replace(/\s+/g, ' ').trim() : '';
    }
    return own;
  },
  visible(el){ return !!(el && el.offsetParent!==null && !el.disabled && !el.closest('#wgPalette') && !el.closest('#winctl')); },
  /** Every candidate for `q` that needs no engine call, best first. */
  local(q){
    const out=[], add=(kind, label, hint, run, extra)=>{ const s=this.score(label+' '+(extra||''), q); if(s>=0) out.push({kind, label, hint:hint||'', run, s}); };
    for(const [name, id] of this.PAGES) add('Page', name, 'Open the '+name+' page', ()=>this.page(id));
    if(typeof WgToolsPage==='object') for(const [k, name, what] of WgToolsPage.TOOLS||[]) add('Tool', name, what, ()=>this.tool(k));
    if(typeof WgKeys==='object') for(const a of WgKeys.ACTIONS) if(a.id!=='palette') add('Editor', a.label, WgKeys.label(a.id), ()=>this.fire(a.id), a.group);
    if(typeof WgViews==='object') WgViews.list().forEach((v, i)=>add('View', v.name, 'A view saved for this cell', ()=>WgViews.go(i), 'view camera'));
    const seen=new Set();
    for(const b of document.querySelectorAll('button, .row input[type=checkbox]')){
      if(!this.visible(b)) continue;
      const label=this.labelOf(b);
      if(!label || label.length<2 || /^[×✕✖]$/.test(label)) continue;
      const key=label+'|'+(b.id||'');
      if(seen.has(key)) continue; seen.add(key);
      if(b.tagName==='INPUT') add('Switch', label+(b.checked? ': turn off' : ': turn on'), b.closest('.row')?.querySelector('label')?.title||'', ()=>b.click());
      else add('Button', label, b.title||'', ()=>b.click());
    }
    if(q.trim().length>=2 && typeof GameData==='object' && GameData.cellList){
      let n=0;
      for(const c of GameData.cellList(true)||[]){
        const label=c.label||c.name||'';
        const s=this.score(label, q); if(s<0) continue;
        out.push({kind:'Cell', label, hint:'Open in the Preview', s:s+0.5, run:()=>this.cell(c)});
        if(++n>=40) break;
      }
    }
    out.sort((a, b)=>a.s-b.s);
    return out.slice(0, 60);
  },
  page(id){
    if(!id){ // the Preview: leave whatever page is open
      for(const [, b] of this.PAGES) if(b){ const el=document.getElementById(b); if(el && el.classList.contains('on')) el.click(); }
      if(document.body.classList.contains('wgEditMode') && typeof WgEditor==='object' && WgEditor.leave) WgEditor.leave();
      return;
    }
    const el=document.getElementById(id); if(el && !el.classList.contains('on')) el.click();
  },
  tool(k){
    if(typeof WgToolsPage!=='object') return;
    const b=document.getElementById('btnTools');
    if(b && !b.classList.contains('on')) b.click();
    WgToolsPage.show(k);
  },
  cell(c){
    if(typeof WgNav!=='object') return;
    WgNav.go(c.kind==='int'? 'int:'+c.name : c.x+','+c.y);
  },
  async record(tag, id){
    if(typeof WgEditor!=='object') return;
    if(!document.body.classList.contains('wgEditMode')){ const b=document.getElementById('btnEditor'); if(b) b.click(); }
    await WgEditor.showEntry(tag, id);
  },
  /** An Editor action as its first key would run it. */
  fire(id){
    const chord=(WgKeys.map()[id]||[])[0]; if(!chord) return;
    const parts=chord.split('+'), key=parts.pop();
    const ev=new KeyboardEvent('keydown', {key:key.length===1? key.toLowerCase() : key, code:key.length===1? 'Key'+key.toUpperCase() : key,
      ctrlKey:parts.includes('Ctrl'), shiftKey:parts.includes('Shift'), altKey:parts.includes('Alt'), bubbles:true, cancelable:true});
    (document.activeElement && document.activeElement!==document.body? document.body : document.body).dispatchEvent(ev);
  },

  open(){
    if(this._el){ this._el.querySelector('input').focus(); return; }
    const box=document.createElement('div'); box.id='wgPalette';
    box.style.cssText='position:fixed;left:50%;top:12%;transform:translateX(-50%);width:min(640px,90vw);z-index:9999;'+
      'background:var(--bg2);border:1px solid var(--line);border-radius:8px;box-shadow:0 10px 40px rgba(0,0,0,.5);padding:8px';
    const inp=document.createElement('input'); inp.className='fld'; inp.placeholder='Type an action, a tool, a cell or a record id…';
    inp.style.cssText='width:100%;box-sizing:border-box;font-size:14px;padding:6px 8px';
    const list=document.createElement('div'); list.style.cssText='max-height:55vh;overflow:auto;margin-top:6px';
    box.append(inp, list);
    document.body.append(box);
    this._el=box; this._list=list; this._sel=0;
    inp.oninput=()=>this.update(inp.value);
    inp.onkeydown=e=>{
      if(e.key==='Escape'){ e.preventDefault(); this.close(); }
      else if(e.key==='ArrowDown'){ e.preventDefault(); this._sel=Math.min(this._items.length-1, this._sel+1); this.draw(); }
      else if(e.key==='ArrowUp'){ e.preventDefault(); this._sel=Math.max(0, this._sel-1); this.draw(); }
      else if(e.key==='Enter'){ e.preventDefault(); this.run(this._items[this._sel]); }
      e.stopPropagation();
    };
    this._out=e=>{ if(this._el && !this._el.contains(e.target)) this.close(); };
    setTimeout(()=>document.addEventListener('mousedown', this._out, true), 0);
    this.update('');
    inp.focus();
  },
  close(){
    if(!this._el) return;
    document.removeEventListener('mousedown', this._out, true);
    this._el.remove(); this._el=null; this._items=[];
  },
  run(it){
    if(!it) return;
    this.close();
    try{ const r=it.run(); if(r && r.catch) r.catch(e=>toast(String(e.message||e), 'err', 4000)); }
    catch(e){ toast(String(e.message||e), 'err', 4000); }
  },
  /** The list for `q`: what is known at once, then the records the engine finds. */
  update(q){
    this._q=q; this._sel=0;
    this._items=this.local(q);
    this.draw();
    const ask=++this._ask;
    if(q.trim().length<2 || typeof Engine!=='object' || !Engine.has || !Engine.has()) return;
    clearTimeout(this._t);
    this._t=setTimeout(async()=>{
      let hits=[];
      try{ hits=await Engine.call('record_search', {q:q.trim(), limit:15}); if(typeof hits==='string') hits=JSON.parse(hits); }catch(_){ return; }
      if(ask!==this._ask || !this._el) return;
      const recs=(hits||[]).map((h, i)=>({kind:'Record', label:h.id, hint:h.tag, s:30+i, run:()=>this.record(h.tag, h.id)}));
      this._items=this.local(q).concat(recs).slice(0, 80);
      this.draw();
    }, 120);
  },
  draw(){
    const list=this._list; if(!list) return;
    list.textContent='';
    if(!this._items.length){ const h=document.createElement('div'); h.className='hint'; h.textContent='Nothing matches.'; list.append(h); return; }
    this._items.forEach((it, i)=>{
      const r=document.createElement('div');
      r.style.cssText='display:flex;gap:8px;align-items:baseline;padding:4px 6px;border-radius:4px;cursor:pointer'+(i===this._sel? ';background:var(--acd)' : '');
      const k=document.createElement('span'); k.textContent=it.kind; k.style.cssText='font-size:10px;opacity:.6;width:52px;flex:none';
      const l=document.createElement('span'); l.textContent=it.label; l.style.cssText='flex:none;max-width:60%;overflow:hidden;text-overflow:ellipsis;white-space:nowrap';
      const h=document.createElement('span'); h.className='hint'; h.textContent=it.hint; h.style.cssText='overflow:hidden;text-overflow:ellipsis;white-space:nowrap';
      r.append(k, l, h);
      r.onmousemove=()=>{ if(this._sel!==i){ this._sel=i; this.draw(); } };
      r.onclick=()=>this.run(it);
      list.append(r);
    });
    const cur=list.children[this._sel]; if(cur && cur.scrollIntoView) cur.scrollIntoView({block:'nearest'});
  },
  bind(){
    window.addEventListener('keydown', e=>{
      if(typeof WgKeys!=='object' || !WgKeys.is(e, 'palette')) return;
      e.preventDefault(); e.stopPropagation();
      this._el? this.close() : this.open();
    }, true);
  }
};
WgPalette.bind();
