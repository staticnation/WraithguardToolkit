/* =====================================================================================
   Wraithguard: the Load order and Conflicts pages' batch (Static 2026-11-24, "lets do all
   this").

   - **Why here?** (Load order: the button over content=, or double-click a plugin): what put
     the plugin where it is - the curated list or where it came from, the plugin it was
     anchored to, every rule block that names it (the file and line, what it puts before and
     after, and which of those the order does not follow), the masters its header lists and
     whether each loads before it, and the rules refused for it (`loadorder_why`,
     `ordercore::why`). Each plugin named is a link to its own answer.
   - **Snapshots** (Load order): the order on the page kept by name; one is kept on its own
     before each Sort and after each Export that writes (the last 10 of those). Compare says
     what was added, taken out, ticked on or off, and moved (what is left once the longest
     run in the same order is set aside); Restore puts it on the page for Export to write.
     Kept per openmw.cfg, on this machine.
   - **Reviewed** (Conflicts): a record or a file marked as looked at, with the plugins (or
     folders) it had then; when another plugin comes to touch it the mark says it changed since.
     The lists show all, the ones not reviewed, the reviewed, or the changed.
   Built with the DOM. Copyright (c) 2026 StaticNation.
   ===================================================================================== */
const WgLoQol={
  SNAP_KEY:'wgLoadOrder.snaps', REV_KEY:'wgConflicts.reviewed', AUTO_KEEP:10, MAX:40,
  el(tag, cls, text){ const e=document.createElement(tag); if(cls) e.className=cls; if(text!=null) e.textContent=text; return e; },
  btn(text, title, fn, cls){ const b=this.el('button', 'btn'+(cls? ' '+cls : ''), text); if(title) b.title=title; b.onclick=fn; return b; },

  /* ---------------- a floating box over the page ---------------- */
  box(id, title){
    let d=document.getElementById(id);
    if(!d){
      d=this.el('div'); d.id=id;
      d.style.cssText='position:fixed;z-index:80;right:16px;top:calc(var(--top-h,40px) + 16px);width:min(520px,90vw);max-height:75vh;display:flex;flex-direction:column;'+
        'background:var(--bg2);color:var(--tx);border:1px solid var(--line2);border-radius:var(--r-box,6px);box-shadow:0 6px 24px rgba(0,0,0,.45);font-size:12px';
      const h=this.el('div'); h.style.cssText='display:flex;align-items:center;gap:6px;padding:6px 8px;border-bottom:1px solid var(--line)';
      const t=this.el('b'); t.className='ttl';
      const x=this.btn('✕', 'Close (Esc)', ()=>{ d.hidden=true; }, 'dim');
      h.append(t, this.el('span'), x); h.children[1].style.flex='1';
      const body=this.el('div'); body.className='body'; body.style.cssText='overflow:auto;padding:8px;display:flex;flex-direction:column;gap:6px';
      d.append(h, body);
      d.addEventListener('keydown', e=>{ if(e.key==='Escape'){ e.stopPropagation(); d.hidden=true; } });
      document.body.append(d);
    }
    d.hidden=false;
    d.querySelector('.ttl').textContent=title;
    const body=d.querySelector('.body'); body.textContent='';
    return body;
  },

  /* ---------------- why here ---------------- */
  LO(){ return typeof WgLoadOrder==='object'? WgLoadOrder : null; },
  order(){ const L=this.LO(); return L? L.content.map(r=>r.name) : []; },
  async why(name){
    const L=this.LO(); if(!L || !name) return;
    const body=this.box('loWhyBox', 'Why is '+name+' here?');
    body.append(this.el('div', 'empty', 'Reading the rules…'));
    let w;
    try{
      w=await Engine.call('loadorder_why', {name, order:this.order(), rules:L.lines('rules'), cfg:L.val('cfg')});
      if(typeof w==='string') w=JSON.parse(w);
    }catch(e){ body.textContent=String(e.message||e); return; }
    this.drawWhy(body, w);
  },
  /** A plugin's name as a link to its own answer. */
  link(n){ const a=this.el('a', '', n); a.href='#'; a.onclick=e=>{ e.preventDefault(); this.why(n); }; return a; },
  names(list){ const s=this.el('span'); list.forEach((n, i)=>{ if(i) s.append(', '); s.append(this.link(n)); }); return s; },
  line(...parts){ const d=this.el('div'); d.append(...parts); return d; },
  head(text){ const h=this.el('div', '', text); h.style.cssText='font-weight:bold;margin-top:4px'; return h; },
  drawWhy(body, w){
    body.textContent='';
    const order=this.order(), n=order.length, at=w.at;
    const where=this.el('div');
    if(at==null) where.textContent=w.name+' is not in the order on the page.';
    else{
      where.append(w.name+' loads '+(at+1)+' of '+n);
      if(at>0) where.append(', after ', this.link(order[at-1]));
      if(at<n-1) where.append(at>0? ' and before ' : ', before ', this.link(order[at+1]));
      where.append('.');
    }
    body.append(where);
    // What the sort said of it.
    if(w.sorted){
      if(w.curated) body.append(this.line('On the curated list: its place there is kept, and the rules move only your own plugins round it.'));
      else if(w.mine) body.append(this.line('One of your plugins'+(w.origin? ' ('+w.origin+')' : '')+': placed by its masters and the rules.'));
      const a=w.anchor;
      if(a){
        if(a.how==='after' && a.other) body.append(this.line('Placed right after ', this.link(a.other), ', the latest plugin it must follow.'));
        else if(a.how==='before' && a.other) body.append(this.line('Placed right before ', this.link(a.other), ', the earliest plugin it must precede.'));
        else if(a.how==='nearstart') body.append(this.line('Pulled toward the start by a [NearStart] rule.'));
        else if(a.how==='nearend') body.append(this.line('Pulled toward the end by a [NearEnd] rule.'));
        else body.append(this.line('Nothing places it: it keeps its place in the cfg, or goes at the end if it is new.'));
      }
    }else body.append(this.el('div', 'empty', 'Sorted by the rules alone (or not sorted yet): the rules and masters below are what the sort reads.'));
    // Masters.
    body.append(this.head('Masters'));
    if(w.masters==null) body.append(this.el('div', 'empty', 'Its file was not found in the setup\'s folders, so its header was not read.'));
    else if(!w.masters.length) body.append(this.el('div', 'empty', 'Its header lists none.'));
    else for(const m of w.masters){
      const ok=m.at!=null && at!=null && m.at<at;
      const d=this.line(ok? '✓ ' : '✗ ', this.link(m.name), m.at==null? ': not in the order (OpenMW will not load it)' : ok? '' : ': loads after it (OpenMW will refuse it)');
      if(!ok) d.style.color='var(--err)';
      body.append(d);
    }
    // Rules.
    body.append(this.head('Rules that name it'));
    if(!w.rules.length) body.append(this.el('div', 'empty', 'No rule names it.'));
    for(const r of w.rules){
      const card=this.el('div'); card.style.cssText='border:1px solid var(--line);border-radius:4px;padding:4px 6px';
      const k=r.kind==='order'? '[Order]' : r.kind==='nearstart'? '[NearStart]' : '[NearEnd]';
      const h=this.el('div', '', r.file+', line '+r.line+': '+k); h.style.opacity='.8';
      card.append(h);
      if(r.kind!=='order') card.append(this.line(r.kind==='nearstart'? 'Wants it as near the start as the other rules allow.' : 'Wants it as near the end as the other rules allow.'));
      else{
        if(r.before.length) card.append(this.line('Before it: ', this.names(r.before)));
        if(r.after.length) card.append(this.line('After it: ', this.names(r.after)));
        if(!r.before.length && !r.after.length) card.append(this.el('div', 'empty', 'Nothing else in the block is installed.'));
        if(r.broken.length){ const b=this.line('Not followed for ', this.names(r.broken), ': the curated list, another rule, or a cycle came first.'); b.style.color='var(--warn)'; card.append(b); }
      }
      const all=this.el('div', 'empty', r.names.join(' → ')); all.style.cssText='font-family:var(--mono);font-size:11px;white-space:normal';
      card.append(all);
      body.append(card);
    }
    if(w.refused && w.refused.length){
      body.append(this.head('Rules not applied'));
      for(const t of w.refused) body.append(this.el('div', '', t));
    }
  },

  /* ---------------- snapshots ---------------- */
  allSnaps(){ try{ return JSON.parse(localStorage.getItem(this.SNAP_KEY)||'{}')||{}; }catch(_){ return {}; } },
  snaps(){ const L=this.LO(); return (this.allSnaps()[(L && L.val('cfg'))||'']||[]); },
  putSnaps(list){
    const L=this.LO(), all=this.allSnaps(), k=(L && L.val('cfg'))||'';
    all[k]=list;
    try{ localStorage.setItem(this.SNAP_KEY, JSON.stringify(all)); }catch(e){ toast('Snapshots not kept: '+String(e.message||e), 'err', 4000); }
  },
  /** The page's order kept: `auto` one of the kept-on-its-own, the oldest of which go. */
  snap(name, auto){
    const L=this.LO(); if(!L || !L.content.length) return null;
    const s={name:name||('Snapshot '+new Date().toLocaleString()), t:Date.now(), auto:!!auto,
      content:L.content.map(r=>[r.name, r.on? 1 : 0]), data:L.data.map(r=>[r.line, r.on? 1 : 0])};
    let list=this.snaps().concat([s]);
    const autos=list.filter(x=>x.auto);
    if(autos.length>this.AUTO_KEEP){ const drop=new Set(autos.slice(0, autos.length-this.AUTO_KEEP)); list=list.filter(x=>!drop.has(x)); }
    if(list.length>this.MAX) list=list.slice(list.length-this.MAX);
    this.putSnaps(list);
    return s;
  },
  /** The longest run of `a`'s items in the same order in `b` (by index in b): what did not move. */
  steady(a, b){
    const ix=new Map(b.map((n, i)=>[n.toLowerCase(), i]));
    const seq=a.map(n=>ix.get(n.toLowerCase())).filter(i=>i!=null);
    const tails=[], prev=new Array(seq.length);
    seq.forEach((v, i)=>{
      let lo=0, hi=tails.length;
      while(lo<hi){ const m=(lo+hi)>>1; if(seq[tails[m]]<v) lo=m+1; else hi=m; }
      prev[i]=lo? tails[lo-1] : -1; tails[lo]=i;
    });
    const keep=new Set(); let k=tails.length? tails[tails.length-1] : -1;
    while(k>=0){ keep.add(b[seq[k]].toLowerCase()); k=prev[k]; }
    return keep;
  },
  /** What differs from snapshot `s` to the page now: {added, removed, on, off, moved:[{name, from, to}]}. */
  diff(s){
    const L=this.LO(), now=L.content, was=s.content;
    const W=new Map(was.map(([n, on], i)=>[n.toLowerCase(), {on, i}])), N=new Map(now.map((r, i)=>[r.name.toLowerCase(), {r, i}]));
    const out={added:[], removed:[], on:[], off:[], moved:[]};
    for(const r of now){ const w=W.get(r.name.toLowerCase()); if(!w) out.added.push(r.name); else if(!!w.on!==!!r.on) (r.on? out.on : out.off).push(r.name); }
    for(const [n] of was) if(!N.has(n.toLowerCase())) out.removed.push(n);
    const keep=this.steady(was.map(x=>x[0]), now.map(r=>r.name));
    for(const r of now){ const w=W.get(r.name.toLowerCase()); if(w && !keep.has(r.name.toLowerCase())) out.moved.push({name:r.name, from:w.i+1, to:N.get(r.name.toLowerCase()).i+1}); }
    return out;
  },
  restore(s){
    const L=this.LO(); if(!L) return;
    const cur=new Map(L.content.map(r=>[r.name.toLowerCase(), r]));
    const rows=s.content.map(([n, on])=>{ const r=cur.get(n.toLowerCase()); cur.delete(n.toLowerCase()); return Object.assign({}, r||{name:n, mine:false, bad:false}, {name:n, on:!!on, sel:false}); });
    // Plugins on the page the snapshot did not have: kept, at the end, off.
    const extra=[...cur.values()].map(r=>Object.assign({}, r, {on:false, sel:false}));
    L.content=rows.concat(extra);
    if(s.data && s.data.length && L.data.length){
      const cd=new Map(L.data.map(r=>[r.line, r]));
      const drows=s.data.map(([l, on])=>{ const r=cd.get(l); cd.delete(l); return Object.assign({}, r||{line:l, isNew:false}, {on:!!on, sel:false}); });
      L.data=drows.concat([...cd.values()].map(r=>Object.assign({}, r, {on:false, sel:false})));
    }
    L.drawOrders();
    toast('Restored "'+s.name+'" on the page'+(extra.length? ' ('+extra.length+' plugin'+(extra.length===1? '' : 's')+' it did not have left at the end, off)' : '')+': Export writes it', 'ok', 6000);
  },
  showSnaps(){
    const body=this.box('loSnapBox', 'Load order snapshots');
    const row=this.el('div'); row.style.cssText='display:flex;gap:6px';
    const name=this.el('input'); name.type='text'; name.placeholder='Name (optional)'; name.style.cssText='flex:1;min-width:0';
    row.append(name, this.btn('Save the page\'s order', 'Keep the order as the page has it now', ()=>{ if(this.snap(name.value.trim(), false)) this.showSnaps(); else toast('Sort first: nothing on the page to keep', 'warn', 3000); }));
    body.append(row);
    const list=this.snaps().slice().reverse();
    if(!list.length) body.append(this.el('div', 'empty', 'None yet. One is kept on its own before each Sort and after each Export that writes.'));
    const out=this.el('div');
    for(const s of list){
      const r=this.el('div'); r.style.cssText='display:flex;gap:6px;align-items:center;border-bottom:1px solid var(--line);padding:3px 0';
      const t=this.el('span', '', s.name); t.style.cssText='flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap';
      t.title=new Date(s.t).toLocaleString()+' · '+s.content.length+' plugins';
      if(s.auto) t.style.opacity='.75';
      r.append(t,
        this.btn('Compare', 'What differs from it to the page now', ()=>this.showDiff(out, s), 'dim'),
        this.btn('Restore', 'Put its order on the page (Export writes it)', ()=>this.restore(s), 'dim'),
        this.btn('✕', 'Forget it', ()=>{ this.putSnaps(this.snaps().filter(x=>x.t!==s.t || x.name!==s.name)); this.showSnaps(); }, 'dim'));
      body.append(r);
    }
    body.append(out);
  },
  showDiff(out, s){
    out.textContent='';
    const d=this.diff(s), h=this.head('From "'+s.name+'" to the page now');
    out.append(h);
    const part=(label, list, fmt)=>{ if(!list.length) return; const l=this.el('div'); l.append(this.el('b', '', label+' ('+list.length+'): ')); list.slice(0, 200).forEach((x, i)=>{ if(i) l.append(', '); l.append(fmt? fmt(x) : x); }); if(list.length>200) l.append(' …'); out.append(l); };
    part('Added', d.added); part('Taken out', d.removed); part('Ticked on', d.on); part('Ticked off', d.off);
    part('Moved', d.moved, m=>m.name+' '+m.from+'→'+m.to);
    if(!d.added.length && !d.removed.length && !d.on.length && !d.off.length && !d.moved.length) out.append(this.el('div', 'empty', 'The same order.'));
  },

  /* ---------------- reviewed (Conflicts) ---------------- */
  CF(){ return typeof WgConflicts==='object'? WgConflicts : null; },
  revAll(){ try{ return JSON.parse(localStorage.getItem(this.REV_KEY)||'{}')||{}; }catch(_){ return {}; } },
  rev(){ const C=this.CF(), k=(C && C.cfg())||'', a=this.revAll(); const r=a[k]||{}; r.rec=r.rec||{}; r.file=r.file||{}; return r; },
  putRev(r){ const C=this.CF(), a=this.revAll(); a[(C && C.cfg())||'']=r; try{ localStorage.setItem(this.REV_KEY, JSON.stringify(a)); }catch(_){ } },
  recKey(c){ return c.type+' '+String(c.id).toLowerCase(); },
  recSig(c){ return c.plugins.map(p=>String(p).toLowerCase()).join('|'); },
  fileSig(f){ return f.providers.map(p=>String(p).toLowerCase()).join('|'); },
  /** 'no', 'yes' or 'changed'. */
  stateRec(R, c){ const s=R.rec[this.recKey(c)]; return s==null? 'no' : s===this.recSig(c)? 'yes' : 'changed'; },
  stateFile(R, f){ const s=R.file[String(f.path).toLowerCase()]; return s==null? 'no' : s===this.fileSig(f)? 'yes' : 'changed'; },
  toggleRec(c){ const R=this.rev(), k=this.recKey(c); if(this.stateRec(R, c)==='yes') delete R.rec[k]; else R.rec[k]=this.recSig(c); this.putRev(R); },
  toggleFile(f){ const R=this.rev(), k=String(f.path).toLowerCase(); if(this.stateFile(R, f)==='yes') delete R.file[k]; else R.file[k]=this.fileSig(f); this.putRev(R); },
  revFilter(){ const s=document.getElementById('cfRev'); return s? s.value : 'all'; },
  /** The bar's controls, made once. */
  revBar(){
    if(document.getElementById('cfRev')) return;
    const bar=document.querySelector('#wgCF .bar.tabs'); if(!bar) return;
    const sel=this.el('select'); sel.id='cfRev'; sel.title='Show the records and files by whether you have reviewed them';
    for(const [v, t] of [['all', 'Reviewed or not'], ['no', 'Not reviewed'], ['yes', 'Reviewed'], ['changed', 'Changed since reviewed']]){ const o=this.el('option', '', t); o.value=v; sel.append(o); }
    sel.onchange=()=>this.CF().draw();
    const all=this.btn('Mark shown reviewed', 'Mark every record (or file) the list shows as reviewed', ()=>this.markShown(), 'dim');
    const clr=this.btn('Clear marks', 'Forget every reviewed mark for this setup', async()=>{ if(typeof wgConfirm==='function' && !(await wgConfirm('Forget every reviewed mark for this setup?'))) return; this.putRev({rec:{}, file:{}}); this.CF().draw(); }, 'dim');
    const find=document.getElementById('cfFind');
    bar.insertBefore(sel, find||null); bar.insertBefore(all, find||null); bar.insertBefore(clr, find||null);
  },
  _shown:{rec:[], file:[]},
  markShown(){
    const C=this.CF(), R=this.rev(), files=C.tab==='files';
    for(const c of files? [] : this._shown.rec) R.rec[this.recKey(c)]=this.recSig(c);
    for(const f of files? this._shown.file : []) R.file[String(f.path).toLowerCase()]=this.fileSig(f);
    this.putRev(R); C.draw();
  },
  /** The rows the page drew, each given its mark (✓ reviewed, ↻ changed since, ○ not), which a
   *  click turns on or off. */
  decorate(recs, files, total, ftotal){
    const R=this.rev(), mk=(state, title, fn)=>{
      const b=this.el('span', 'cfRevMk', state==='yes'? '✓' : state==='changed'? '↻' : '○');
      b.title=state==='yes'? 'Reviewed (click to unmark)' : state==='changed'? 'Changed since you reviewed it: another plugin touches it now (click to mark again)' : 'Not reviewed (click to mark)';
      b.style.cssText='cursor:pointer;margin-right:4px;'+(state==='yes'? 'color:var(--grow)' : state==='changed'? 'color:var(--warn)' : 'opacity:.45');
      b.onclick=e=>{ e.stopPropagation(); fn(); this.CF().draw(); };
      return b;
    };
    const rows=[...document.querySelectorAll('#cfList div.r')];
    rows.forEach((d, i)=>{ const c=recs[i]; if(!c) return; const s=this.stateRec(R, c); const id=d.querySelector('.id'); if(id) id.prepend(mk(s, '', ()=>this.toggleRec(c))); if(s==='yes') d.style.opacity='.7'; });
    const frows=[...document.querySelectorAll('#cfFList div.r')];
    frows.forEach((d, i)=>{ const f=files[i]; if(!f) return; const s=this.stateFile(R, f); d.prepend(mk(s, '', ()=>this.toggleFile(f))); if(s==='yes') d.style.opacity='.7'; });
    const n=document.getElementById('cfCount'); if(n && total) n.textContent=recs.length+(recs.length>=5000? '+' : '')+' of '+total;
    const fn=document.getElementById('cfFCount'); if(fn && ftotal) fn.textContent=files.length+' of '+ftotal;
    if(total && !rows.length){ const b=document.getElementById('cfList'); if(b){ b.textContent=''; b.append(this.el('div', 'empty', 'No record matches.')); } }
    if(ftotal && !frows.length){ const b=document.getElementById('cfFList'); if(b){ b.textContent=''; b.append(this.el('div', 'empty', 'No file matches.')); } }
  },

  /* ---------------- hooks ---------------- */
  bind(){
    const L=this.LO(), C=this.CF(), Q=this;
    if(L && !L._qol){
      L._qol=true;
      const build=L.build, sort=L.sort, exportPlan=L.exportPlan;
      L.build=function(){
        const r=build.apply(this, arguments);
        const h=document.querySelector('#loContent') && document.querySelector('#loContent').closest('.lo-col').querySelector('h3');
        if(h && !document.getElementById('loWhy')){
          const w=Q.btn('Why here?', 'What put the picked plugin where it is (or double-click a plugin)', ()=>{ const r=this.content.find(x=>x.sel); if(r) Q.why(r.name); else toast('Pick a plugin first', 'warn', 2500); }, 'dim'); w.id='loWhy';
          const s=Q.btn('Snapshots', 'Keep the order, compare it, put an older one back', ()=>Q.showSnaps(), 'dim'); s.id='loSnaps';
          h.insertBefore(s, h.querySelector('.sp')); h.insertBefore(w, s);
          document.getElementById('loContent').addEventListener('dblclick', e=>{ const li=e.target.closest && e.target.closest('li[data-i]'); if(!li || e.target.tagName==='INPUT') return; const r=this.content[+li.dataset.i]; if(r) Q.why(r.name); });
        }
        return r;
      };
      L.sort=async function(){ if(this.content.length) Q.snap('Before Sort, '+new Date().toLocaleString(), true); return sort.apply(this, arguments); };
      L.exportPlan=async function(){
        const r=await exportPlan.apply(this, arguments);
        if(this.content.length && !this.flag('dryRun')) Q.snap('Exported, '+new Date().toLocaleString(), true);
        return r;
      };
    }
    if(C && !C._qol){
      C._qol=true;
      const draw=C.draw;
      C.draw=function(){
        Q.revBar();
        const all=this.list, allF=this.files, f=Q.revFilter(), R=Q.rev();
        if(f!=='all'){ this.list=all.filter(c=>Q.stateRec(R, c)===f); this.files=allF.filter(x=>Q.stateFile(R, x)===f); }
        let recs=[], files=[];
        try{
          draw.apply(this, arguments);
          const mine=$('#cfMine').checked;
          recs=this.list.filter(c=>(!mine||c.involvesSubset) && this.match(c.type+' '+c.id+' '+c.plugins.join(' '))).slice(0, 5000);
          files=this.files.filter(x=>(!mine||x.involvesSubset) && this.match(x.path+' '+x.providers.join(' '))).slice(0, 5000);
        }finally{ this.list=all; this.files=allF; }
        Q._shown={rec:recs, file:files};
        Q.decorate(recs, files, all.length, allF.length);
      };
    }
  }
};
WgLoQol.bind();
