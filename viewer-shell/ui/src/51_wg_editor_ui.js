/* =====================================================================================
   The Editor's window furniture - Wraithguard. What makes the Editor (50_wg_editor.js)
   work like a desktop tool rather than a stack of popups:

   - Panels (the record dialog, Use Report, Layers, Dialogue, Script Edit, Pending, Lua)
     float over the render window, or dock in a column beside it: the right-hand rail, or
     under the Object Window and Cell View. Drag a panel by its title bar to move it; drop
     it on the window's right edge, or on the Object Window column's edge, to dock it there;
     drag a docked one out to float it again. ▾ (or a double-click on the title bar) folds
     a panel to its title, ⧉ says where it sits. Floating panels keep the size they are
     dragged to (the corner grip). All of it is remembered in this viewer (`wg.edUI`).
   - The Object Window column: drag its edge to widen it, the bar between the Object Window
     and the Cell View to share the height, a pane's title to fold it; « folds the column
     to a strip, ⇄ moves it to the other side of the render window.
   - The toolbar over the render window: the windows (Dialogue, Layers, Lua, Pending with
     its count), what can be done to the selected object (the Q menu's, with their keys),
     and flyouts for the transform settings and the view. Drag its grip to float it,
     double-click the grip to put it back; right-click the grip to dock it at the top or the
     bottom, or stand it up.
   - Flyout menus with submenus (`menu`), shared by the toolbar and the Q menu.
   - Sections: the record and reference dialogs' fields in folding groups, the state of
     each remembered per record type, with a field filter that opens what matches.
   - Previews: a record's mesh, drawn and turning, when the pointer rests on its row in the
     Object Window and under the list for the selected one; a drag into the render window
     carries the picture with it, and a marker shows where the new reference will land.

   Tooltips are the page's own (30_tip.js): every control here says what it does in its
   `title`, its key in brackets.

   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
const WgUI={
  _st:null,
  tb:null,
  _timer:0,
  _dragRec:null,

  state(){
    if(this._st) return this._st;
    let s={};
    try{ s=JSON.parse(localStorage.getItem('wg.edUI')||'{}')||{}; }catch(_){ s={}; }
    for(const k of ['panels','panes','sections']) if(!s[k] || typeof s[k]!=='object') s[k]={};
    return (this._st=s);
  },
  save(){ try{ localStorage.setItem('wg.edUI', JSON.stringify(this._st||{})); }catch(_){ } },

  /* ---- flyout menus ------------------------------------------------------------------ */

  /** Takes every open flyout down. */
  closeMenus(){ document.querySelectorAll('.edFly.edFlyRoot').forEach(m=>m.remove()); },

  /** A flyout at an element (under it) or at `{x, y}`. Items: `{label, title, key, act,
   *  checked, radio, disabled, sub:[items]}`, `{sep:true}` or `{head:'text'}`. */
  menu(at, items){
    this.closeMenus();
    const m=this.menuEl(items);
    m.classList.add('edFlyRoot');
    document.body.appendChild(m);
    let x, y;
    if(at && at.getBoundingClientRect){ const r=at.getBoundingClientRect(); x=r.left; y=r.bottom+3; }
    else{ x=at? at.x : (window.innerWidth||1200)/2; y=at? at.y : (window.innerHeight||800)/2; }
    const W=window.innerWidth||1200, H=window.innerHeight||800;
    m.style.left=Math.max(2, Math.min(x, W-(m.offsetWidth||240)-4))+'px';
    m.style.top=Math.max(2, Math.min(y, H-(m.offsetHeight||200)-4))+'px';
    return m;
  },

  menuEl(items){
    const m=document.createElement('div');
    m.className='edFly ori';
    for(const it of items){
      if(!it) continue;
      if(it.sep){ const hr=document.createElement('hr'); hr.className='edQsep'; m.appendChild(hr); continue; }
      if(it.head){ const h=document.createElement('div'); h.className='edFlyHead'; h.textContent=it.head; m.appendChild(h); continue; }
      const el=document.createElement('div');
      el.className='it'+(it.info? ' info' : it.disabled? ' off' : '')+(it.sub? ' sub' : '');
      if(it.title) el.title=it.title;
      el.innerHTML='<span class="ck">'+(it.checked? (it.radio? '●' : '✓') : '')+'</span><span class="lb">'+escHtml(it.label)+'</span>'+
        (it.key? '<span class="kb">'+escHtml(it.key)+'</span>' : '')+(it.sub? '<span class="ar">▸</span>' : '');
      if(it.sub){
        const sub=this.menuEl(it.sub);
        sub.classList.add('edFlySub');
        el.appendChild(sub);
        el.onclick=e=>{ e.stopPropagation(); el.classList.toggle('open'); this.fitSub(el, sub); };
        el.onmouseenter=()=>this.fitSub(el, sub);
      }else if(!it.disabled && !it.info){
        el.onclick=e=>{ e.stopPropagation(); this.closeMenus(); if(it.act) it.act(); };
      }
      m.appendChild(el);
    }
    return m;
  },

  /** The Editor's keys: a sheet in as many columns as the window has room for, each
   *  section kept whole, scrolling when it is still too tall, with a filter. Closes
   *  as a flyout does (a click elsewhere, Esc). */
  keysSheet(at){
    this.closeMenus();
    const secs=[]; let cur=null;
    for(const it of this.helpItems()){
      if(!it) continue;
      if(it.head){ cur={head:it.head, items:[]}; secs.push(cur); continue; }
      if(!cur){ cur={head:'', items:[]}; secs.push(cur); }
      cur.items.push(it);
    }
    const m=document.createElement('div');
    m.className='edFly ori edFlyRoot edKeys';
    m.innerHTML='<div class="edKeysTop"><input class="fld" type="search" placeholder="Filter the keys" spellcheck="false"></div>'+
      '<div class="edKeysCols">'+secs.map(s=>'<section class="edKeysSec"><div class="edFlyHead">'+escHtml(s.head)+'</div>'+
        s.items.map(it=>'<div class="it info"><span class="lb">'+escHtml(it.label)+'</span><span class="kb">'+escHtml(it.key||'')+'</span></div>').join('')+
        '</section>').join('')+'</div>';
    document.body.appendChild(m);
    const W=window.innerWidth||1200, H=window.innerHeight||800;
    let x, y;
    if(at && at.getBoundingClientRect){ const r=at.getBoundingClientRect(); x=r.left; y=r.bottom+3; }
    else{ x=W/2; y=60; }
    const w=m.offsetWidth||Math.min(980, W-16);
    m.style.left=Math.max(4, Math.min(x, W-w-6))+'px';
    m.style.top=Math.max(4, Math.min(y, H-120))+'px';
    m.style.maxHeight=Math.max(160, H-Math.max(4, Math.min(y, H-120))-8)+'px';
    const f=m.querySelector('input');
    f.addEventListener('keydown', e=>{ if(e.key==='Escape'){ this.closeMenus(); } e.stopPropagation(); });
    f.oninput=()=>{
      const q=f.value.trim().toLowerCase();
      m.querySelectorAll('.edKeysSec').forEach(sec=>{
        // A section whose heading matches shows whole.
        const all=!q || sec.querySelector('.edFlyHead').textContent.toLowerCase().includes(q);
        let any=false;
        sec.querySelectorAll('.it').forEach(it=>{ const on=all || it.textContent.toLowerCase().includes(q); it.hidden=!on; any=any||on; });
        sec.hidden=!any;
      });
    };
    setTimeout(()=>{ try{ f.focus({preventScroll:true}); }catch(_){ } }, 0);
    return m;
  },

  /** A submenu that would leave the window opens to the left instead. */
  fitSub(el, sub){
    const r=el.getBoundingClientRect(), W=window.innerWidth||1200;
    sub.classList.toggle('left', r.right+(sub.offsetWidth||220)>W-4);
  },

  /* ---- panels -------------------------------------------------------------------------- */

  /** Puts a panel the editor made over the render window and makes it movable, dockable
   *  and foldable (once). */
  mount(d){
    ($('#vpwrap')||document.body).appendChild(d);
    return this.adopt(d);
  },

  adopt(d){
    if(!d || d._wgPanel) return d;
    d._wgPanel=true;
    d.classList.add('edPanel');
    const head=d.querySelector('.orihead');
    if(head){
      const tools=document.createElement('span');
      tools.className='edPanTools';
      tools.innerHTML=
        '<button class="btn dim ic" data-pan="fold" title="Fold the panel to its title bar, or open it again (double-click the title bar does the same)">▾</button>'+
        '<button class="btn dim ic" data-pan="dock" title="Where the panel sits: over the render window, or docked in a column beside it. Drag the title bar to move it; let go on the window\'s right edge, or on the Object Window column\'s edge, to dock it">⧉</button>';
      const close=[...head.querySelectorAll(':scope > button')].pop();
      head.insertBefore(tools, close||null);
      tools.querySelector('[data-pan="fold"]').onclick=e=>{ e.stopPropagation(); this.fold(d); };
      tools.querySelector('[data-pan="dock"]').onclick=e=>{ e.stopPropagation(); this.dockMenu(d, e.currentTarget); };
      head.addEventListener('dblclick', e=>{ if(!e.target.closest('button,input,select,textarea,a')) this.fold(d); });
      head.addEventListener('pointerdown', e=>this.dragPanel(d, e));
      head.classList.add('edDragBar');
    }
    d.addEventListener('pointerdown', ()=>this.front(d), true);
    const st=this.state().panels[d.id]||{};
    if(st.folded) d.classList.add('folded');
    this.place(d, st.dock||'float');
    if(typeof ResizeObserver==='function'){
      let t=0;
      new ResizeObserver(()=>{ clearTimeout(t); t=setTimeout(()=>this.rememberSize(d), 400); }).observe(d);
    }
    return d;
  },

  /** The size a floating panel was dragged to (the browser writes it inline). */
  rememberSize(d){
    const s=this.state().panels[d.id];
    if(!s || s.dock!=='float' || !d.style.width) return;
    if(s.w===d.style.width && s.h===d.style.height) return;
    s.w=d.style.width; s.h=d.style.height||'';
    this.save();
  },

  front(d){
    document.querySelectorAll('.edPanel.front').forEach(x=>{ if(x!==d) x.classList.remove('front'); });
    d.classList.add('front');
  },

  fold(d, to){
    const on=to==null? !d.classList.contains('folded') : !!to;
    d.classList.toggle('folded', on);
    const s=this.state().panels[d.id]=this.state().panels[d.id]||{};
    s.folded=on; this.save();
  },

  /** The right-hand rail docked panels stack in (made once, beside the render window). */
  rail(){
    let r=$('#edRail');
    if(r) return r;
    r=document.createElement('div');
    r.id='edRail';
    r.innerHTML='<div class="edGrip edGripL" title="Drag to make the column wider or narrower"></div>';
    ($('#main')||document.body).appendChild(r);
    const w=this.state().railW; if(w) r.style.width=w+'px';
    this.gripWidth(r.querySelector('.edGrip'), r, -1, v=>{ this.state().railW=v; this.save(); });
    // Panels come and go, and are opened and closed (`hidden` on a child): the rail follows.
    if(typeof MutationObserver==='function')
      new MutationObserver(()=>this.syncRail()).observe(r, {childList:true, subtree:true, attributes:true, attributeFilter:['hidden']});
    return r;
  },

  /** The rail shows only while something docked in it is open. */
  syncRail(){
    const r=$('#edRail'); if(!r) return;
    r.classList.toggle('empty', ![...r.children].some(c=>c.classList.contains('edPanel') && !c.hidden));
  },

  /** Docks (`right`, `left`) or floats a panel. */
  place(d, dock, at){
    const all=this.state().panels, s=all[d.id]=all[d.id]||{};
    if(dock==='left' && !$('#edDock')) dock='float';
    d.classList.remove('edDockedL', 'edDockedR');
    if(dock==='right'){
      this.rail().appendChild(d);
      d.classList.add('edDockedR');
    }else if(dock==='left'){
      $('#edDock').appendChild(d);
      d.classList.add('edDockedL');
    }else{
      dock='float';
      const host=$('#vpwrap')||document.body;
      if(d.parentNode!==host) host.appendChild(d);
      if(at){ s.x=at.x; s.y=at.y; }
    }
    if(dock==='float'){
      if(s.x!=null){ d.style.left=s.x+'px'; d.style.top=s.y+'px'; d.style.right='auto'; }
      if(s.w){ d.style.width=s.w; if(s.h) d.style.height=s.h; }
    }else{
      for(const k of ['left','top','right','width','height']) d.style[k]='';
    }
    s.dock=dock;
    this.save();
    this.syncRail();
  },

  dockMenu(d, at){
    const s=this.state().panels[d.id]||{};
    const dock=s.dock||'float';
    this.menu(at, [
      {head:'Where it sits'},
      {label:'Floating over the render window', radio:true, checked:dock==='float', act:()=>this.place(d, 'float')},
      {label:'Docked on the right', radio:true, checked:dock==='right', act:()=>this.place(d, 'right'),
       title:'In a column beside the render window, under any other docked panel'},
      {label:'Docked under the Object Window', radio:true, checked:dock==='left', disabled:!$('#edDock'), act:()=>this.place(d, 'left')},
      {sep:true},
      {label:d.classList.contains('folded')? 'Open it out' : 'Fold to the title bar', act:()=>this.fold(d)},
      {label:'Back where it started', title:'Its first place and size', act:()=>{
        const all=this.state().panels; delete all[d.id]; this.save();
        for(const k of ['left','top','right','width','height']) d.style[k]='';
        d.classList.remove('folded'); this.place(d, 'float');
      }},
    ]);
  },

  /** The left edge a panel is docked under the Object Window by: its column's right edge. */
  leftEdge(){
    const dk=$('#edDock');
    if(!dk || !document.body.classList.contains('wgEditMode') || dk.classList.contains('onRight')) return null;
    return dk.getBoundingClientRect().right;
  },

  dockMark(){
    let m=$('#edDockMark');
    if(!m){ m=document.createElement('div'); m.id='edDockMark'; document.body.appendChild(m); }
    return m;
  },

  /** A title-bar drag: moves a floating panel, floats a docked one, docks on an edge. */
  dragPanel(d, e){
    if(e.button!==0 || e.target.closest('button,input,select,textarea,a,label')) return;
    const host=$('#vpwrap')||document.body;
    const start={x:e.clientX, y:e.clientY};
    let moved=false, off={x:40, y:12}, zone=null;
    const mark=this.dockMark();
    const mv=ev=>{
      if(!moved){
        if(Math.hypot(ev.clientX-start.x, ev.clientY-start.y)<6) return;
        moved=true;
        const s=this.state().panels[d.id]||{};
        if(s.dock && s.dock!=='float'){
          const hr=host.getBoundingClientRect();
          this.place(d, 'float', {x:ev.clientX-hr.left-off.x, y:ev.clientY-hr.top-off.y});
        }else{
          const r=d.getBoundingClientRect(); off={x:ev.clientX-r.left, y:ev.clientY-r.top};
        }
        d.classList.add('dragging');
        this.front(d);
      }
      const hr=host.getBoundingClientRect();
      d.style.left=Math.max(0, Math.min(ev.clientX-off.x-hr.left, hr.width-80))+'px';
      d.style.top=Math.max(0, Math.min(ev.clientY-off.y-hr.top, hr.height-32))+'px';
      d.style.right='auto';
      const W=window.innerWidth||1200, le=this.leftEdge();
      zone= ev.clientX>W-40? 'right' : (le!=null && Math.abs(ev.clientX-le)<36)? 'left' : null;
      mark.className=zone? 'on '+zone : '';
      if(zone==='left') mark.style.left=(le-4)+'px'; else mark.style.left='';
    };
    const up=()=>{
      window.removeEventListener('pointermove', mv);
      mark.className='';
      d.classList.remove('dragging');
      if(!moved) return;
      if(zone) this.place(d, zone);
      else{
        const s=this.state().panels[d.id]=this.state().panels[d.id]||{};
        s.dock='float'; s.x=parseFloat(d.style.left)||0; s.y=parseFloat(d.style.top)||0;
        this.save();
      }
    };
    window.addEventListener('pointermove', mv);
    window.addEventListener('pointerup', up, {once:true});
  },

  /** A grip that drags an element's width (`dir` 1: the grip is on its right edge). */
  gripWidth(grip, el, dir, done){
    if(!grip) return;
    grip.addEventListener('pointerdown', e=>{
      if(e.button!==0) return;
      e.preventDefault();
      const x0=e.clientX, w0=el.getBoundingClientRect().width;
      document.body.classList.add('edResizing');
      const mv=ev=>{ const w=Math.max(220, Math.min((window.innerWidth||1600)*0.7, w0+dir*(ev.clientX-x0))); el.style.width=w+'px'; el.style.flexBasis=w+'px'; };
      const up=()=>{
        window.removeEventListener('pointermove', mv);
        document.body.classList.remove('edResizing');
        if(done) done(Math.round(el.getBoundingClientRect().width));
        if(App.R) App.R.dirty=true;
      };
      window.addEventListener('pointermove', mv);
      window.addEventListener('pointerup', up, {once:true});
    });
  },

  /* ---- the Object Window column --------------------------------------------------------- */

  /** The dock's grips, folds and side, once it is built (50_wg_editor.js `dock`). */
  enhanceDock(dk){
    if(!dk || dk._wgUI) return;
    dk._wgUI=true;
    const st=this.state();
    // The column's width.
    const grip=document.createElement('div');
    grip.className='edGrip edGripR';
    grip.title='Drag to make the column wider or narrower';
    dk.appendChild(grip);
    if(st.dockW){ dk.style.width=st.dockW+'px'; dk.style.flexBasis=st.dockW+'px'; }
    this.gripWidth(grip, dk, 1, w=>{ st.dockW=w; this.save(); });
    // The share of height between the two panes.
    const objs=dk.querySelector('#edObjects'), cells=dk.querySelector('#edCells');
    if(objs && cells){
      const bar=document.createElement('div');
      bar.className='edHSplit';
      bar.title='Drag to share the height between the Object Window and the Cell View';
      dk.insertBefore(bar, cells);
      if(st.split) { objs.style.flex=st.split+' 1 0'; cells.style.flex=(1-st.split)+' 1 0'; }
      bar.addEventListener('pointerdown', e=>{
        if(e.button!==0) return;
        e.preventDefault();
        const a=objs.getBoundingClientRect(), b=cells.getBoundingClientRect(), top=a.top, tot=a.height+b.height;
        const mv=ev=>{
          const f=Math.max(0.12, Math.min(0.88, (ev.clientY-top)/tot));
          objs.style.flex=f+' 1 0'; cells.style.flex=(1-f)+' 1 0'; st.split=+f.toFixed(3);
        };
        window.addEventListener('pointermove', mv);
        window.addEventListener('pointerup', ()=>{ window.removeEventListener('pointermove', mv); this.save(); }, {once:true});
      });
    }
    // Each pane folds by its title, and pops out over the render window (⤢).
    dk.querySelectorAll('.edPane').forEach(p=>{
      const head=p.querySelector('.edHead');
      if(head && !head.querySelector('.edPopBtn')){
        const pop=document.createElement('button');
        pop.className='btn dim ic edPopBtn';
        pop.textContent='⤢';
        pop.title='Pop this pane out over the render window, as big as you like (drag its corner); ⤢ again, or its ✕, puts it back in the column';
        pop.onclick=e=>{ e.stopPropagation(); this.popPane(p); };
        head.appendChild(pop);
      }
      if(st.popped && st.popped[p.id]) setTimeout(()=>this.popPane(p, true), 0);
      const h=p.querySelector('.edHead b');
      if(!h) return;
      h.classList.add('edFoldable');
      h.title='Fold or open this pane';
      if(st.panes[p.id]) p.classList.add('folded');
      h.onclick=()=>{ p.classList.toggle('folded'); st.panes[p.id]=p.classList.contains('folded'); this.save(); };
    });
    // The column folds to a strip, and changes sides.
    const head=objs && objs.querySelector('.edHead');
    if(head){
      const side=document.createElement('button');
      side.className='btn dim ic'; side.id='edSide';
      side.title='Move this column to the other side of the render window';
      side.textContent='⇄';
      side.onclick=()=>{ st.dockSide=st.dockSide==='right'? 'left' : 'right'; this.save(); this.applySide(); };
      const fold=document.createElement('button');
      fold.className='btn dim ic'; fold.id='edFoldDock';
      fold.title='Fold the column to a strip, to give the render window the room (click the strip to bring it back)';
      fold.textContent='«';
      fold.onclick=()=>this.foldDock(true);
      head.insertBefore(side, head.firstChild);
      head.insertBefore(fold, head.firstChild);
    }
    const strip=document.createElement('div');
    strip.className='edStrip';
    strip.title='Open the Object Window and Cell View again';
    strip.innerHTML='<span>Object Window · Cell View</span>';
    strip.onclick=()=>this.foldDock(false);
    dk.appendChild(strip);
    if(st.dockFolded) this.foldDock(true, true);
    this.applySide();
  },

  /** A pane of the column (Object Window, Cell View) out in a panel of its own over the
   *  render window - moved, sized and folded as every panel is - or back in the column. */
  popPane(p, on){
    const st=this.state(), dk=$('#edDock');
    if(!p || !dk) return;
    st.popped=st.popped||{};
    const out=on==null? !p.closest('.edPopout') : on;
    const id='edPop_'+p.id;
    let w=$('#'+id);
    if(out){
      if(p.closest('.edPopout')) return;
      if(!w){
        w=document.createElement('div');
        w.id=id; w.className='ori edPopout';
        const title=(p.querySelector('.edHead b')||{}).textContent||'Pane';
        w.innerHTML='<div class="orihead"><b>'+escHtml(title)+'</b><span style="flex:1"></span>'+
          '<button class="btn dim ic" data-popback="1" title="Put it back in the column">&#x2715;</button></div><div class="oribody"></div>';
        w.querySelector('[data-popback]').onclick=()=>this.popPane(p, false);
        this.mount(w);
      }
      p._home={next:p.nextSibling};
      w.querySelector('.oribody').appendChild(p);
      w.hidden=false;
      this.front(w);
    }else{
      const home=p._home && p._home.next && p._home.next.parentNode===dk? p._home.next : (p.id==='edObjects'? dk.querySelector('.edHSplit') : dk.querySelector('.edStrip'));
      dk.insertBefore(p, home||null);
      if(w) w.hidden=true;
    }
    const b=p.querySelector('.edPopBtn'); if(b) b.classList.toggle('on', out);
    st.popped[p.id]=out; this.save();
    dk.classList.toggle('allOut', !dk.querySelector(':scope > .edPane'));
    const bar=dk.querySelector('.edHSplit'); if(bar) bar.hidden=dk.querySelectorAll(':scope > .edPane').length<2;
    if(App.R) App.R.dirty=true;
  },

  /* ---- columns ----------------------------------------------------------------------- */

  /** A table whose columns are dragged to the width wanted (a grip on each heading's
   *  right edge; a double-click on one puts them all back), kept per `key`. A cell still
   *  too narrow for its text says it in full when the pointer rests on it. */
  sizeColumns(table, key){
    if(!table) return;
    const ths=[...table.querySelectorAll('thead th')];
    const st=this.state(); st.cols=st.cols||{};
    const saved=st.cols[key];
    const apply=ws=>{
      table.classList.add('sized');
      ths.forEach((th,i)=>{ th.style.width=ws[i]+'px'; });
      table.style.width=ws.reduce((a,b)=>a+b, 0)+'px';
    };
    if(Array.isArray(saved) && saved.length===ths.length) apply(saved);
    ths.forEach((th,i)=>{
      if(th.querySelector('.edColGrip')) return;
      th.style.position='sticky';
      const g=document.createElement('span');
      g.className='edColGrip';
      g.title='Drag to widen or narrow this column; double-click to fit them all again';
      g.onclick=e=>e.stopPropagation();
      g.ondblclick=e=>{
        e.stopPropagation();
        delete st.cols[key]; this.save();
        table.classList.remove('sized'); table.style.width='';
        ths.forEach(t=>{ t.style.width=''; });
      };
      g.addEventListener('pointerdown', e=>{
        if(e.button!==0) return;
        e.preventDefault(); e.stopPropagation();
        const ws=ths.map(t=>Math.round(t.getBoundingClientRect().width)||80);
        apply(ws);
        const x0=e.clientX, w0=ws[i];
        document.body.classList.add('edResizing');
        const mv=ev=>{ ws[i]=Math.max(36, w0+ev.clientX-x0); apply(ws); };
        const up=()=>{
          window.removeEventListener('pointermove', mv);
          document.body.classList.remove('edResizing');
          st.cols[key]=ws.slice(); this.save();
        };
        window.addEventListener('pointermove', mv);
        window.addEventListener('pointerup', up, {once:true});
      });
      th.appendChild(g);
    });
    // The whole text, on demand, for a cell that cuts it short.
    if(!table._wgFull){
      table._wgFull=true;
      table.addEventListener('mouseover', e=>{
        const td=e.target.closest && e.target.closest('td');
        if(td && !td.title && td.scrollWidth>td.clientWidth+1) td.title=td.textContent;
      });
    }
  },

  foldDock(on, quiet){
    const dk=$('#edDock'); if(!dk) return;
    dk.classList.toggle('stripped', on);
    if(!quiet){ this.state().dockFolded=on; this.save(); }
    if(App.R) App.R.dirty=true;
  },

  applySide(){
    const dk=$('#edDock'); if(!dk) return;
    const right=this.state().dockSide==='right';
    dk.classList.toggle('onRight', right);
    const fold=dk.querySelector('#edFoldDock'); if(fold) fold.textContent=right? '»' : '«';
    const g=dk.querySelector('.edGripR'); if(g) g.classList.toggle('flip', right);
  },

  /* ---- the toolbar ---------------------------------------------------------------------- */

  /** The toolbar over the render window (made once). */
  toolbar(){
    if(this.tb) return this.tb;
    const host=$('#vpwrap');
    if(!host) return null;
    const t=document.createElement('div');
    t.id='edTools';
    const b=(id, glyph, label, title)=>'<button class="btn sm edTb" data-tb="'+id+'" title="'+escHtml(title)+'"><span class="g">'+glyph+'</span><span class="l">'+escHtml(label)+'</span></button>';
    t.innerHTML=
      '<span class="edTbGrip" title="Drag to move the toolbar; double-click to put it back at the top; right-click for where it docks">⋮⋮</span>'+
      '<span class="grp" data-g="win"></span><span class="sepv"></span>'+
      '<span class="grp" data-g="sel">'+
        b('record', '✎', 'Record', 'Edit the selected object\'s base record (F2)')+
        b('ref', '⌖', 'Reference', 'Edit the selected placed object: where it stands, owner, lock, trap (F3)')+
        b('uses', '⇶', 'Uses', 'The selected object\'s record: every record that names it, and the cells it is placed in (Use Report)')+
        b('drop', '⤓', 'Drop', 'Drop the selected object onto what is under it (F)')+
        b('dup', '⧉', 'Duplicate', 'A new reference of the same record beside the selected one')+
        b('hide', '◌', 'Hide', 'Hide the selected object (layer "Hidden"; Layers brings it back)')+
        b('del', '✕', 'Delete', 'Delete the selected reference (one the patch made is taken back out of it)')+
      '</span><span class="sepv"></span>'+
      '<span class="grp" data-g="snap">'+
        b('undo', '↶', 'Undo', 'Take back the last change to a placed object made here (Ctrl+Z)')+
        b('redo', '↷', 'Redo', 'Make it again (Ctrl+Y)')+
        b('grid', '#', 'Grid', 'Snap to grid: moves and placing land on the grid (G). The size is in Transform')+
        b('angle', '∠', 'Angle', 'Snap to angle: turns go in steps (Shift+G). The step is in Transform')+
      '</span><span class="sepv"></span>'+
      '<span class="grp" data-g="fly">'+
        b('xform', '⟲', 'Transform ▾', 'How drags move and turn objects, and how far a nudge goes')+
        b('view', '◐', 'View ▾', 'The render window: landscape, hidden objects, layers, QuickStart')+
        b('ori', 'ⓘ', 'ORI', 'Show or hide the inspector (ORI) when you pick an object (I). Picking still selects')+
        b('tfh', '☷', 'Full help', 'Show or hide the full help panel (TFH) beside it (Shift+I)')+
        b('help', '?', 'Keys', 'The Editor\'s keys and mouse')+
      '</span>';
    host.appendChild(t);
    this.tb=t;
    t.querySelectorAll('[data-tb]').forEach(el=>el.onclick=e=>this.tbAction(el.dataset.tb, el, e));
    const grip=t.querySelector('.edTbGrip');
    grip.addEventListener('pointerdown', e=>this.dragToolbar(e));
    grip.addEventListener('dblclick', ()=>this.dockToolbar('top'));
    grip.addEventListener('contextmenu', e=>{
      e.preventDefault();
      const p=this.state().tb||{};
      this.menu({x:e.clientX, y:e.clientY}, [
        {head:'Toolbar'},
        {label:'At the top of the render window', radio:true, checked:(p.pos||'top')==='top', act:()=>this.dockToolbar('top')},
        {label:'At the bottom', radio:true, checked:p.pos==='bottom', act:()=>this.dockToolbar('bottom')},
        {label:'Floating (drag the grip)', radio:true, checked:p.pos==='float', act:()=>this.dockToolbar('float')},
        {sep:true},
        {label:'Standing up (a column)', checked:!!p.vert, act:()=>{ p.vert=!p.vert; this.state().tb=p; this.save(); this.applyToolbar(); }},
        {label:'Words beside the icons', checked:p.words!==false, act:()=>{ p.words=p.words===false; this.state().tb=p; this.save(); this.applyToolbar(); }},
      ]);
    });
    this.applyToolbar();
    return t;
  },

  /** The editor's window buttons (made in its dock) go into the toolbar's first group. */
  adoptWindowButtons(dk){
    const t=this.toolbar(); if(!t || !dk) return;
    const g=t.querySelector('[data-g="win"]');
    for(const [id, glyph] of [['edDialBtn','☰'],['edLayersBtn','▤'],['edLuaBtn','☾'],['edPendBtn','⏳']]){
      const btn=dk.querySelector('#'+id);
      if(!btn) continue;
      btn.classList.add('edTb');
      btn.dataset.glyph=glyph;
      g.appendChild(btn);
    }
    // And the patch itself, written from here (the Patch Builder's sister).
    if(!t.querySelector('#edBuildBtn')){
      const b=document.createElement('button');
      b.className='btn sm edTb'; b.id='edBuildBtn'; b.dataset.glyph='⚒'; b.textContent='Build patch';
      b.title='Write the pool as the patch, from here: where it goes, Append or Replace, and the build\'s report';
      b.onclick=()=>WgEditor.showBuild();
      g.appendChild(b);
    }
    // Path grid mode: the cells' PGRD points edited in the render window.
    if(!t.querySelector('#edPathBtn')){
      const b=document.createElement('button');
      b.className='btn sm edTb'; b.id='edPathBtn'; b.dataset.glyph='⋰'; b.textContent='Path grid';
      b.title='Path grid mode: the loaded cells\' path grids drawn and edited here - click a point to select, drag to move, Shift+click the ground to add, Ctrl+click to link or unlink, J / U, Delete. Right-click: drop the queued grids of these cells';
      b.onclick=()=>WgPath.toggle();
      b.oncontextmenu=ev=>{ ev.preventDefault(); this.menu({x:ev.clientX, y:ev.clientY}, [
        {head:'Path grid'},
        {label:WgPath.on? 'Leave path grid mode' : 'Path grid mode', act:()=>WgPath.toggle()},
        {label:'Drop the queued path grids of the loaded cells', act:()=>WgPath.revertAll()},
        {label:'Navmesh overlay (Tools)', checked:!!(typeof WgTools==='object' && WgTools.ovl.navmesh), act:()=>{
          WgTools.ovl.navmesh=!WgTools.ovl.navmesh; const sw=document.getElementById('wgOvNavmesh'); if(sw) sw.checked=WgTools.ovl.navmesh; WgTools.overlay('navmesh'); }},
      ]); };
      g.appendChild(b);
    }
  },

  applyToolbar(){
    const t=this.tb; if(!t) return;
    const p=this.state().tb||{};
    const pos=p.pos||'top';
    t.className=[pos, p.vert? 'vert' : '', p.words===false? 'iconly' : ''].filter(Boolean).join(' ');
    if(pos==='float' && p.x!=null){ t.style.left=p.x+'px'; t.style.top=p.y+'px'; }
    else{ t.style.left=''; t.style.top=''; }
  },

  dockToolbar(pos){
    const p=this.state().tb=this.state().tb||{};
    p.pos=pos;
    if(pos==='float' && p.x==null){ p.x=80; p.y=60; }
    this.save(); this.applyToolbar();
  },

  dragToolbar(e){
    if(e.button!==0) return;
    const t=this.tb, host=$('#vpwrap'); if(!t || !host) return;
    e.preventDefault();
    const r=t.getBoundingClientRect(), off={x:e.clientX-r.left, y:e.clientY-r.top};
    let moved=false;
    const mv=ev=>{
      const hr=host.getBoundingClientRect();
      if(!moved){ if(Math.hypot(ev.clientX-e.clientX, ev.clientY-e.clientY)<5) return; moved=true; }
      const p=this.state().tb=this.state().tb||{};
      p.pos='float';
      p.x=Math.round(Math.max(0, Math.min(ev.clientX-off.x-hr.left, hr.width-60)));
      p.y=Math.round(Math.max(0, Math.min(ev.clientY-off.y-hr.top, hr.height-30)));
      // Close to the top or the bottom edge: dock there.
      p.snap= p.y<10? 'top' : (p.y>hr.height-r.height-12? 'bottom' : null);
      this.applyToolbar();
    };
    const up=()=>{
      window.removeEventListener('pointermove', mv);
      const p=this.state().tb||{};
      if(moved && p.snap){ p.pos=p.snap; }
      delete p.snap;
      this.save(); this.applyToolbar();
    };
    window.addEventListener('pointermove', mv);
    window.addEventListener('pointerup', up, {once:true});
  },

  /** What a toolbar button does. */
  tbAction(a, el){
    const E=WgEditor;
    if(a==='xform') return this.menu(el, this.transformItems());
    if(a==='view') return this.menu(el, this.viewItems());
    if(a==='help') return this.keysSheet(el);
    if(a==='ori') return E.setPanels('ori', E.showOri===false);
    if(a==='tfh') return E.setPanels('tfh', E.showTfh===false);
    if(a==='undo') return E.undo();
    if(a==='redo') return E.redo();
    if(a==='grid' || a==='angle'){
      if(a==='grid') E.gridOn=!E.gridOn; else E.angleOn=!E.angleOn;
      E.savePrefs(); this.refresh();
      return toast(a==='grid'? 'Snap to grid '+(E.gridOn? 'on: '+E.grid+' units' : 'off') : 'Snap to angle '+(E.angleOn? 'on: '+E.angle+'° steps' : 'off'),'ok',1800);
    }
    const sel=E.selected();
    if(!sel) return toast('Select an object in the render window first (click it)','warn',2500);
    const rec=E.selectedRecord(sel);
    if((a==='record' || a==='uses' || a==='dup') && !rec.tag) return toast('The selected object\'s record is not known yet','warn',2500);
    return E.quickAction(a, sel, rec);
  },

  transformItems(){
    const E=WgEditor;
    const axes=[['+z','Up (+Z)'],['-z','Down (−Z)'],['+x','+X'],['-x','−X'],['+y','+Y'],['-y','−Y'],['','Keep the turn']];
    const steps=[1, 4, 8, 16, 32, 64, 128];
    const grids=[1, 2, 4, 8, 16, 32, 64, 128, 256, 512];
    const angles=[1, 5, 10, 15, 22.5, 30, 45, 90];
    return [
      {head:'Snap'},
      {label:'Snap to grid', key:'G', checked:E.gridOn, act:()=>{ E.gridOn=!E.gridOn; E.savePrefs(); this.refresh(); },
       title:'Moves, Alt-drags, pastes and drops into the render window land on a multiple of the grid size (in the world\'s units)'},
      {label:'Grid size: '+E.grid, sub:grids.map(n=>({label:n+' units'+(n===16? ' (default)' : ''), radio:true, checked:E.grid===n,
        act:()=>{ E.grid=n; E.gridOn=true; E.savePrefs(); this.refresh(); }}))},
      {label:'Snap to angle', key:'Shift+G', checked:E.angleOn, act:()=>{ E.angleOn=!E.angleOn; E.savePrefs(); this.refresh(); },
       title:'Shift-drag turns go in steps of the angle'},
      {label:'Angle step: '+E.angle+'°', sub:angles.map(n=>({label:n+'°'+(n===15? ' (default)' : ''), radio:true, checked:E.angle===n,
        act:()=>{ E.angle=n; E.angleOn=true; E.savePrefs(); this.refresh(); }}))},
      {head:'Alt-drag onto a surface'},
      {label:'The object\'s axis out of the surface', sub:axes.map(([v,t])=>({label:t, radio:true, checked:E.snapAxis===v,
        act:()=>{ E.snapAxis=v; E.savePrefs(); toast('Alt-drag: '+(v? 'the object\'s '+v+' axis out of the surface' : 'the turn kept'),'ok',2000); }}))},
      {head:'Shift-drag turns about'},
      {label:'The world\'s axes', radio:true, checked:E.rotWorld, act:()=>{ E.rotWorld=true; E.savePrefs(); }},
      {label:'The object\'s own axes', radio:true, checked:!E.rotWorld, act:()=>{ E.rotWorld=false; E.savePrefs(); }},
      {head:'Nudge (the reference dialog\'s − and +)'},
      {label:'Step: '+E.NUDGE, sub:steps.map(n=>({label:n+(n===8? ' (default)' : ''), radio:true, checked:E.NUDGE===n,
        act:()=>{ E.NUDGE=n; const p=this.state(); p.nudge=n; this.save(); }})),
       title:'Units a nudge moves, degrees it turns; Shift takes eight steps'},
    ];
  },

  viewItems(){
    const E=WgEditor, R=App.R, L=E.layers();
    const hidden=L.find(l=>l.name==='Hidden' && l.keys.length);
    return [
      {label:'Inspector (ORI)', key:'I', checked:E.showOri!==false, act:()=>E.setPanels('ori', E.showOri===false)},
      {label:'Full help (TFH)', key:'Shift+I', checked:E.showTfh!==false, act:()=>E.setPanels('tfh', E.showTfh===false)},
      {sep:true},
      {label:(R && R.opts && R.opts.ground===false)? 'Show the landscape' : 'Hide the landscape', act:()=>{
        if(!R || !R.opts) return; R.opts.ground=R.opts.ground===false; R.dirty=true; }},
      {label:'Restore hidden references', disabled:!hidden, act:()=>{ hidden.visible=true; hidden.keys=[]; E.layersChanged(); }},
      {label:'Layers', sub:[
        ...L.map((l,i)=>({label:l.name+' ('+l.keys.length+')', checked:l.visible, key:i<9? 'Ctrl+Shift+'+(i+1) : '',
          act:()=>{ l.visible=!l.visible; E.layersChanged(); }})),
        L.length? {sep:true} : null,
        {label:'Show all', act:()=>{ L.forEach(l=>{ l.visible=true; }); E.layersChanged(); }},
        {label:'Layers panel…', act:()=>E.showLayers()},
      ]},
      {sep:true},
      {label:'QuickStart: open this cell and view at start', act:()=>E.setQuickStart()},
      {label:'QuickStart: clear', disabled:!E.quickStart(), act:()=>{ try{ localStorage.removeItem('wgEditorQuickStart'); }catch(_){ } toast('QuickStart cleared','ok',2000); }},
      {sep:true},
      {label:'Fold every panel', act:()=>document.querySelectorAll('.edPanel').forEach(d=>this.fold(d, true))},
      {label:'Close every panel', act:()=>document.querySelectorAll('.edPanel').forEach(d=>{ d.hidden=true; })},
      {label:'Put the panels and toolbar back', title:'Every panel floating where it started, the toolbar at the top, the column as it was',
       act:()=>this.resetLayout()},
    ];
  },

  helpItems(){
    const k=(label, key)=>({label, key, info:true});
    return [
      {head:'Render window'},
      k('Pick an object', 'Click'), k('Pick several', 'Ctrl+click'),
      k('Select everything in a box', 'Drag on empty ground'), k('Add a box to the selection', 'Ctrl / Shift + drag'),
      k('Move everything selected', 'Drag one of them'),
      k('Let them go', 'Esc'),
      k('Move on the ground plane', 'Drag'), k('Lift / lower', 'Z + drag'), k('Keep to an axis', 'X / Y + drag'),
      k('Onto the surface under the pointer', 'Alt + drag'), k('Turn', 'Shift + drag'), k('Drop to the ground', 'F'),
      k('What can be done to it', 'Q'),
      k('Centre the view on it', 'C'), k('Delete it', 'Delete'),
      k('Copy it / place a copy under the pointer', 'Ctrl+C / Ctrl+V'),
      k('Duplicate it beside itself', 'Ctrl+D'),
      k('Undo / redo a change to it', 'Ctrl+Z / Ctrl+Y'),
      k('Snap to grid / to angle', 'G / Shift+G'),
      k('Inspector (ORI) / full help (TFH) on or off', 'I / Shift+I'),
      {head:'Path grid mode (toolbar: Path grid)'},
      k('Select a point / add one to the selection', 'Click / Shift+click'), k('Move it', 'Drag (Z: up and down, Alt: onto the surface)'),
      k('A new point, linked to the selected one', 'Shift+click the ground'), k('Link or unlink two points', 'Ctrl+click'),
      k('Link the selected in a chain / unlink them', 'J / U'), k('Delete the selected points', 'Delete'),
      k('Undo / redo', 'Ctrl+Z / Ctrl+Y'), k('Let go / leave the mode', 'Esc'),
      {head:'Windows'},
      k('Edit record', 'F2'), k('Edit reference', 'F3'), k('Filter the Object Window', 'Ctrl+F'),
      k('Show / hide layer N', 'Ctrl+Shift+N'), k('Close the dialog or menu', 'Esc'),
      {head:'Placing'},
      k('Drag a record into the render window', 'Drag'), k('At the view\'s pivot', 'Right-click a record'),
    ];
  },

  /** Every panel, the toolbar and the column back where they started. */
  resetLayout(){
    const st=this.state();
    st.panels={}; st.panes={}; delete st.tb; delete st.dockW; delete st.split; delete st.railW; delete st.dockSide; delete st.dockFolded;
    this.save();
    document.querySelectorAll('.edPanel').forEach(d=>{
      for(const k of ['left','top','right','width','height']) d.style[k]='';
      d.classList.remove('folded');
      this.place(d, 'float');
    });
    const dk=$('#edDock');
    if(dk){
      dk.style.width=''; dk.style.flexBasis='';
      dk.querySelectorAll('.edPane').forEach(p=>{ p.classList.remove('folded'); p.style.flex=''; });
      this.foldDock(false, true);
      this.applySide();
    }
    const r=$('#edRail'); if(r) r.style.width='';
    this.applyToolbar();
    if(App.R) App.R.dirty=true;
  },

  /** The toolbar's state, while the Editor is on: what applies to the selection, and the
   *  pool's count. */
  refresh(){
    const t=this.tb; if(!t || !WgEditor.on) return;
    const E=WgEditor, sel=E.selected();
    t.querySelectorAll('[data-g="sel"] [data-tb]').forEach(b=>b.classList.toggle('off', !sel));
    const tb=k=>t.querySelector('[data-tb="'+k+'"]');
    if(tb('grid')){ tb('grid').classList.toggle('on', E.gridOn); tb('grid').querySelector('.l').textContent=E.gridOn? 'Grid '+E.grid : 'Grid'; }
    if(tb('angle')){ tb('angle').classList.toggle('on', E.angleOn); tb('angle').querySelector('.l').textContent=E.angleOn? E.angle+'°' : 'Angle'; }
    if(tb('ori')) tb('ori').classList.toggle('on', E.showOri!==false);
    if(tb('tfh')) tb('tfh').classList.toggle('on', E.showTfh!==false);
    if(tb('undo')) tb('undo').classList.toggle('off', !E._undo.length);
    if(tb('redo')) tb('redo').classList.toggle('off', !E._redo.length);
    const p=t.querySelector('#edPendBtn');
    if(p){
      const n=WgEditor._pendN||0;
      p.classList.toggle('badge', n>0);
      p.dataset.n=n? String(n) : '';
    }
  },

  onEnter(){
    const st=this.state();
    if(st.nudge) WgEditor.NUDGE=st.nudge;
    this.toolbar();
    const dk=$('#edDock');
    if(dk){ this.enhanceDock(dk); this.adoptWindowButtons(dk); }
    this.syncRail();
    clearInterval(this._timer);
    this._timer=setInterval(()=>this.refresh(), 400);
    this.refresh();
  },

  onLeave(){
    clearInterval(this._timer);
    this.closeMenus();
    this.hideHover();
  },

  /* ---- sections ----------------------------------------------------------------------- */

  /** Folding groups for a dialog's fields: `groups` is `[{key, title, html, edited, n}]`;
   *  each opens as it was left for `scope` (a record type), the first ones open by default. */
  sectionsHtml(scope, groups){
    const st=this.state().sections;
    return groups.map((g,i)=>{
      const k=scope+':'+g.key;
      const open= (k in st)? st[k] : (i<3 || g.edited);
      return '<details class="edSec'+(g.edited? ' edited' : '')+'" data-sec="'+escHtml(k)+'"'+(open? ' open' : '')+'>'+
        '<summary title="Fold or open">'+escHtml(g.title)+(g.n!=null? ' <span class="from">'+g.n+'</span>' : '')+
        (g.edited? ' <span class="edDot" title="Changes are waiting here">●</span>' : '')+'</summary>'+g.html+'</details>';
    }).join('');
  },

  wireSections(body){
    const st=this.state().sections;
    body.querySelectorAll('details.edSec').forEach(d=>d.addEventListener('toggle', ()=>{
      if(d._filtering) return;
      st[d.dataset.sec]=d.open; this.save();
    }));
  },

  /** The bar over a dialog's sections: a field filter, open all, fold all. */
  sectionBar(){
    return '<div class="edSecBar"><input class="fld" id="edFieldFilter" placeholder="Find a field…" spellcheck="false" title="Shows the fields whose name or value has this in it, their groups opened">'+
      '<button class="btn dim ic" data-secall="1" title="Open every group">⊞</button>'+
      '<button class="btn dim ic" data-secall="0" title="Fold every group">⊟</button></div>';
  },

  wireSectionBar(body){
    const f=body.querySelector('#edFieldFilter');
    const secs=()=>[...body.querySelectorAll('details.edSec')];
    body.querySelectorAll('[data-secall]').forEach(b=>b.onclick=()=>{
      const on=b.dataset.secall==='1';
      secs().forEach(d=>{ d.open=on; });
    });
    if(!f) return;
    f.onkeydown=e=>{ if(e.key==='Escape'){ f.value=''; f.oninput(); } e.stopPropagation(); };
    f.oninput=()=>{
      const q=f.value.trim().toLowerCase();
      for(const d of secs()){
        let any=false;
        d.querySelectorAll('tbody > tr').forEach(tr=>{
          const hit=!q || tr.textContent.toLowerCase().includes(q)
            || [...tr.querySelectorAll('input,textarea,select')].some(el=>String(el.value||'').toLowerCase().includes(q) || String(el.dataset.path||el.dataset.rpath||'').toLowerCase().includes(q));
          tr.classList.toggle('edFiltered', !hit);
          any=any||hit;
        });
        d.classList.toggle('edFiltered', !!q && !any);
        if(q && any){ d._filtering=true; d.open=true; setTimeout(()=>{ d._filtering=false; }, 0); }
      }
    };
  },

  /** A label for a field path: `data.ai_data.hello` -> "Hello" (the path is its tooltip). */
  label(path){
    const last=String(path).split('.').pop();
    return this.words(last);
  },
  words(s){
    const w=String(s).replace(/_/g, ' ').replace(/([a-z])([A-Z])/g, '$1 $2').trim();
    const up=w.replace(/\b(ai|id|npc|fx|ui)\b/gi, m=>m.toUpperCase());
    return up.charAt(0).toUpperCase()+up.slice(1);
  },

  /** A textarea that grows with what is in it (to a limit), wired once. */
  autoGrow(root){
    root.querySelectorAll('textarea.grow').forEach(t=>{
      const fit=()=>{ t.style.height='auto'; t.style.height=Math.min(Math.max(t.scrollHeight+2, 44), Math.round((window.innerHeight||800)*0.45))+'px'; };
      t.addEventListener('input', fit);
      fit();
      // In a folded group it has no height yet: fit once it is opened.
      const d=t.closest('details'); if(d) d.addEventListener('toggle', fit);
    });
  },

  /* ---- previews ------------------------------------------------------------------------- */

  /** The path a record's mesh is read by: an NPC is assembled by the engine (`__npc/<id>`). */
  modelPath(rec){
    const m=rec && rec.model;
    if(m && this.isTexture(m)) return '';
    if(m) return /^meshes[\\/]/i.test(m)? m : 'Meshes\\'+m;
    if(rec && rec.tag==='NPC_' && rec.id) return '__npc/'+rec.id;
    return '';
  },

  /** Whether a path names a texture (a land texture's file, an icon) rather than a mesh. */
  isTexture(p){ return /\.(dds|tga|bmp|png|jpe?g)$/i.test(String(p||'')); },

  /** A texture drawn into the preview's canvas (a land texture has no mesh to turn). */
  textureInto(cv, path){
    const p=/^textures[\\/]/i.test(path)? path : 'textures\\'+path;
    if(typeof loadTexture!=='function'){ cv.classList.add('none'); return; }
    loadTexture(p).then(t=>{
      const url=t && (t.thumb||t.url);
      if(!url){ cv.classList.add('none'); return; }
      const img=new Image();
      img.onload=()=>{
        const cx=cv.getContext('2d'); if(!cx) return;
        cx.imageSmoothingEnabled=true;
        cx.clearRect(0,0,cv.width,cv.height);
        cx.drawImage(img, 0, 0, cv.width, cv.height);
        this._thumbs.set(String(path).toLowerCase(), url);
      };
      img.src=url;
    }).catch(()=>cv.classList.add('none'));
  },

  /** A mesh turning in a canvas while it is shown (or a texture, still): `.stop()` when done. */
  spinner(cv, rec){
    const S={on:true, h:null, a:0.6};
    S.stop=()=>{ S.on=false; if(S.h && App.R){ App.R.thumbEnd(S.h); } S.h=null; };
    if(rec && rec.model && this.isTexture(rec.model)){ this.textureInto(cv, rec.model); return S; }
    const path=this.modelPath(rec);
    const R=App.R;
    if(!path || !R || !R.thumbBegin || typeof loadMeshes!=='function'){ cv.classList.add('none'); return S; }
    (async()=>{
      try{
        const [m]=await loadMeshes([path]);
        if(!S.on) return;
        S.h=(m && !m.err && m.parts && m.parts.length)? R.thumbBegin(m.parts) : null;
      }catch(_){ S.h=null; }
      if(!S.on){ if(S.h) R.thumbEnd(S.h); S.h=null; return; }
      if(!S.h){ cv.classList.add('none'); return; }
      const cx=cv.getContext('2d');
      let last=0;
      const tick=t=>{
        if(!S.on || !S.h || !cv.isConnected){ S.stop(); return; }
        if(t-last>70){
          last=t; S.a+=0.06;
          try{
            const px=R.thumbDraw(S.h, S.a, cv.width);
            if(px) cx.putImageData(new ImageData(new Uint8ClampedArray(px.data.buffer, px.data.byteOffset||0, px.w*px.h*4), px.w, px.h), 0, 0);
            if(!S.url){ S.url=cv.toDataURL(); this._thumbs.set(path.toLowerCase(), S.url); }
          }catch(_){ S.stop(); return; }
        }
        requestAnimationFrame(tick);
      };
      requestAnimationFrame(tick);
    })();
    return S;
  },
  _thumbs:new Map(),

  /** The card the pointer resting on a row brings up, beside the column. */
  hoverPreview(tr, rec){
    clearTimeout(this._hoverT);
    this._hoverT=setTimeout(()=>{
      if(!tr.isConnected || !tr.matches(':hover')) return;
      this.hideHover();
      const c=document.createElement('div');
      c.id='edHover'; c.className='ori';
      c.innerHTML=this.previewHtml(rec, 140)+'<div class="hint">Double-click to open it'+(WgEditor.placeable(rec.tag)? '; drag it into the render window to place one' : '')+'.</div>';
      document.body.appendChild(c);
      const r=tr.getBoundingClientRect(), dk=$('#edDock');
      const right=dk && dk.classList.contains('onRight');
      const W=window.innerWidth||1200, H=window.innerHeight||800;
      const x= right? (dk.getBoundingClientRect().left-c.offsetWidth-8) : ((dk? dk.getBoundingClientRect().right : r.right)+8);
      c.style.left=Math.max(4, Math.min(x, W-c.offsetWidth-4))+'px';
      c.style.top=Math.max(4, Math.min(r.top-20, H-c.offsetHeight-4))+'px';
      this._hoverSpin=this.spinner(c.querySelector('canvas'), rec);
    }, 450);
  },
  hideHover(){
    clearTimeout(this._hoverT);
    if(this._hoverSpin){ this._hoverSpin.stop(); this._hoverSpin=null; }
    const c=$('#edHover'); if(c) c.remove();
  },

  previewHtml(rec, size){
    const defs=rec.defined||[];
    return '<div class="edPrevCard"><canvas width="'+size+'" height="'+size+'" class="edPrevCv" title="'+escHtml(rec.model||'')+'"></canvas>'+
      '<div class="edPrevInfo"><b>'+escHtml(rec.id)+'</b>'+(rec.name? '<div>'+escHtml(rec.name)+'</div>' : '')+
      '<div class="from">'+escHtml(WgEditor.NAMES[rec.tag]||rec.tag)+(rec.placed!=null? ' · placed '+rec.placed+'×' : '')+'</div>'+
      (rec.model? '<div class="from edWrap">'+escHtml(rec.model)+'</div>' : '')+
      (defs.length? '<div class="from edWrap" title="'+escHtml(defs.join(' > '))+'">'+escHtml(defs.join(' › '))+'</div>' : '')+
      (rec.edited? '<div class="edDotTxt">● changes waiting in the patch</div>' : '')+'</div></div>';
  },

  /** The selected record, under the Object Window's list: its mesh turning, what it is,
   *  and what to do with it (it drags into the render window too). */
  selectedPreview(rec){
    const box=$('#edPrev'); if(!box) return;
    if(this._selSpin){ this._selSpin.stop(); this._selSpin=null; }
    if(!rec){ box.hidden=true; box.innerHTML=''; return; }
    box.hidden=false;
    box.innerHTML=this.previewHtml(rec, 96)+
      '<div class="edPrevBtns"><button class="btn sm" data-pv="open" title="Open the record (double-click the row does the same)">Open</button>'+
      '<button class="btn sm" data-pv="place" title="A new reference of it at the view\'s pivot (drag the picture into the render window to put it where you let go)">Place</button>'+
      '<button class="btn sm" data-pv="uses" title="Every record that names it, and the cells it is placed in">Use Report</button>'+
      '<button class="btn dim ic" data-pv="x" title="Close the preview">&#x2715;</button></div>';
    const cv=box.querySelector('canvas');
    this._selSpin=this.spinner(cv, rec);
    box._wgDragRec=rec;
    this.armDrag(box, ()=>box._wgDragRec);
    box.querySelector('[data-pv="open"]').onclick=()=>WgEditor.openRecord(rec.tag, rec.id, rec.defined && rec.defined.length? rec.defined : null);
    box.querySelector('[data-pv="place"]').onclick=()=>WgEditor.placeAt(rec, null, null);
    box.querySelector('[data-pv="uses"]').onclick=()=>WgEditor.showUses(rec.tag, rec.id, rec.defined && rec.defined.length? rec.defined : null);
    box.querySelector('[data-pv="x"]').onclick=()=>this.selectedPreview(null);
  },

  /* ---- dragging records into the render window ------------------------------------------ */

  /* The drag is the page's own, made of pointer events - not the browser's drag and drop.
     HTML5 drag and drop inside the webview is unreliable: on Windows the shell's native
     file-drop handler takes the drag (WebView2 then never sends dragover or drop to the
     page), and elsewhere a drag that starts on a row with text selects it, or never
     starts. Pointer events always arrive. */

  /** Makes `el` a drag source: a press that moves 6 px becomes a drag of `recFn()`'s
   *  record; let go over the render window and it is placed there. Esc cancels. */
  armDrag(el, recFn){
    if(!el || el._wgArm) return;
    el._wgArm=true;
    el.draggable=false;
    el.addEventListener('dragstart', e=>e.preventDefault());   // no native drag, ever
    el.addEventListener('pointerdown', e=>{
      if(e.button!==0 || e.ctrlKey || e.metaKey || e.altKey) return;
      if(e.target.closest && e.target.closest('button,input,select,textarea,a,.edColGrip,.edGrip')) return;
      // No text selection from this press: a selection dragged sideways scrolls the
      // list under it (the Object Window slid right instead of the record dragging).
      e.preventDefault();
      const x0=e.clientX, y0=e.clientY;
      let live=false;
      // Every scrolling box round the source stays where it is until the drag ends.
      const held=[];
      for(let p=el.parentElement; p; p=p.parentElement){
        if(p.scrollWidth>p.clientWidth || p.scrollHeight>p.clientHeight) held.push([p, p.scrollLeft, p.scrollTop]);
      }
      const pin=()=>{ for(const [p, l, t] of held){ if(p.scrollLeft!==l) p.scrollLeft=l; if(p.scrollTop!==t) p.scrollTop=t; } };
      const mv=ev=>{
        if(!live){
          if(Math.hypot(ev.clientX-x0, ev.clientY-y0)<6) return;
          const rec=recFn(); if(!rec){ stop(); return; }
          live=true; this.dragStart(rec);
        }
        ev.preventDefault();
        pin();
        this.dragMove(ev);
      };
      const up=ev=>{ stop(); if(live) this.dragDrop(ev); };
      const key=ev=>{ if(ev.key==='Escape'){ ev.preventDefault(); ev.stopPropagation(); stop(); this.dragEnd(); } };
      const stop=()=>{
        window.removeEventListener('pointermove', mv, true);
        window.removeEventListener('pointerup', up, true);
        window.removeEventListener('pointercancel', cancel, true);
        window.removeEventListener('keydown', key, true);
        window.removeEventListener('scroll', pin, true);
      };
      const cancel=()=>{ stop(); this.dragEnd(); };
      window.addEventListener('scroll', pin, true);
      window.addEventListener('pointermove', mv, true);
      window.addEventListener('pointerup', up, true);
      window.addEventListener('pointercancel', cancel, true);
      window.addEventListener('keydown', key, true);
    });
  },

  /** A record's drag begins: its picture follows the pointer. */
  dragStart(rec){
    this.hideHover();
    this._dragRec=rec;
    try{ const sel=window.getSelection(); if(sel) sel.removeAllRanges(); }catch(_){ }
    document.body.classList.add('edDragging');
    let g=$('#edDragImg');
    if(!g){ g=document.createElement('div'); g.id='edDragImg'; document.body.appendChild(g); }
    const url=this._thumbs.get((this.modelPath(rec)||String(rec.model||'')).toLowerCase())||'';
    g.innerHTML=(url? '<img src="'+url+'" alt="">' : '<span class="ph"></span>')+'<span>'+escHtml(rec.id)+'</span>';
    g.classList.add('live');
  },
  /** The render window's canvas when the pointer is over it with nothing of the
   *  Editor's in the way (a panel, the toolbar, a dialog), else null. */
  canvasAt(x, y){
    const cv=App.R && App.R.cv; if(!cv) return null;
    const r=cv.getBoundingClientRect();
    if(x<r.left || x>r.right || y<r.top || y>r.bottom) return null;
    const el=document.elementFromPoint(x, y);
    if(!el) return null;
    if(el===cv) return cv;
    const host=$('#vpwrap');
    if(host && host.contains(el) && !(el.closest && el.closest('.edPanel,#edTools,#edDlg,.edDlg,.edFly,.edPopout,button,input,select,textarea,a'))) return cv;
    return null;
  },
  dragMove(e){
    const g=$('#edDragImg');
    if(g){ g.style.left=(e.clientX+14)+'px'; g.style.top=(e.clientY+10)+'px'; }
    const over=!!this.canvasAt(e.clientX, e.clientY);
    if(g) g.classList.toggle('no', !over);
    if(over) this.dragOver(e); else this.dropMarker(null);
  },
  dragDrop(e){
    const rec=this._dragRec, over=this.canvasAt(e.clientX, e.clientY);
    this.dragEnd();
    if(rec && over) WgEditor.placeAt(rec, e.clientX, e.clientY);
    else if(rec) toast('Let go over the render window to place '+rec.id,'ok',2000);
  },
  dragEnd(){
    this._dragRec=null; this._dragAt=0; this.dropMarker(null);
    document.body.classList.remove('edDragging');
    // The picture goes: it was moved with inline left/top, which outlast the class.
    const g=$('#edDragImg'); if(g) g.remove();
  },

  /** While a record is dragged over the render window: where it would land. */
  dragOver(e){
    const rec=this._dragRec;
    if(!rec) return;
    const now=performance.now();
    if(this._dragAt && now-this._dragAt<60){ this.dropMarker(e, this._lastAt); return; }
    this._dragAt=now;
    let at=null;
    try{ at=WgEditor.placePoint(e.clientX, e.clientY); }catch(_){ at=null; }
    this._lastAt=at;
    this.dropMarker(e, at);
  },

  dropMarker(e, at){
    let m=$('#edDropAt');
    if(!e){ if(m) m.hidden=true; return; }
    const host=$('#vpwrap'); if(!host) return;
    if(!m){ m=document.createElement('div'); m.id='edDropAt'; host.appendChild(m); }
    const hr=host.getBoundingClientRect();
    m.hidden=false;
    m.style.left=(e.clientX-hr.left)+'px';
    m.style.top=(e.clientY-hr.top)+'px';
    const rec=this._dragRec||{};
    const w=at && at.world;
    const cell=w && typeof CELL==='number'? ' · cell '+Math.floor(w[0]/CELL)+', '+Math.floor(w[1]/CELL) : '';
    m.innerHTML='<span class="x"></span><span class="t">Place <b>'+escHtml(rec.id||'')+'</b>'+
      (w? '<span class="from"> at '+w.map(n=>Math.round(n)).join(', ')+cell+'</span>' : '')+'</span>';
  },
};

document.addEventListener('pointerdown', e=>{
  if(!e.target.closest || !e.target.closest('.edFly')) WgUI.closeMenus();
}, true);
// On the window, so a flyout takes Esc before the editor's own keys (on the document) do.
window.addEventListener('keydown', e=>{
  if(e.key==='Escape' && document.querySelector('.edFly.edFlyRoot')){ WgUI.closeMenus(); e.preventDefault(); e.stopPropagation(); }
}, true);
window.addEventListener('blur', ()=>WgUI.hideHover());
