/* =====================================================================================
   Wraithguard: the Editor's quality-of-life batch (Static 2026-11-24, "lets do all this").

   - **Back and forward** (Alt+Left, Alt+Right; the ◀ ▶ in the record dialog's title): the
     records and references opened, as a browser keeps pages.
   - **The changes field by field** (Pending: "Diff", and "Diff all"): each changed field
     with what the load order has and what the patch writes, and ↶ to take back that field
     alone (`editRevert` with its path).
   - **Script Edit**: Ctrl+Space completes the word at the caret from the load order's record
     ids (`record_search`) and the script's own variables; "Where is it set?" lists each line
     of every script that sets the variable under the caret (`script_writes`).
   - **A response's placeholders** (a dialogue response's text, as you type): a `%name` that is
     neither one of the game's text defines nor a global variable, and an `@topic` without its
     closing `#`. (Spelling is the text box's own spell check, on for responses.)
   The Object Window's "Set a field on all of them" takes +10, -10, *1.25 and +20% as well
   (50_wg_editor_objects.js `byAmount`, editcore `change_many`).
   Copyright (c) 2026 StaticNation.
   ===================================================================================== */
const WgEdHistory={
  back:[], fwd:[], cur:null, _nav:false, MAX:60,
  same(a, b){ return !!a && !!b && a.kind===b.kind && (a.kind==='rec'? a.tag===b.tag && String(a.id).toLowerCase()===String(b.id).toLowerCase() : JSON.stringify(a.ref)===JSON.stringify(b.ref)); },
  push(e){
    if(this._nav || this.same(this.cur, e)) return;
    if(this.cur){ this.back.push(this.cur); if(this.back.length>this.MAX) this.back.shift(); }
    this.cur=e; this.fwd=[];
    this.draw();
  },
  async go(dir){
    const from=dir<0? this.back : this.fwd, to=dir<0? this.fwd : this.back;
    if(!from.length || typeof WgEditor!=='object') return false;
    const e=from.pop();
    if(this.cur) to.push(this.cur);
    this.cur=e;
    this._nav=true;
    try{ e.kind==='rec'? await WgEditor.openRecord(e.tag, e.id, e.plugins||null) : await WgEditor.openRef(e.ref); }
    finally{ this._nav=false; this.draw(); }
    return true;
  },
  label(e){ return !e? '' : e.kind==='rec'? ((WgEditor.NAMES||{})[e.tag]||e.tag)+' '+e.id : 'reference '+(e.ref.origin? e.ref.origin+':'+e.ref.refr : e.ref.uid||''); },
  /** The ◀ ▶ in the record dialog's title. */
  draw(){
    const d=typeof WgEditor==='object' && WgEditor.dlg; if(!d) return;
    const head=d.querySelector('.orihead'); if(!head) return;
    let box=head.querySelector('#edHist');
    if(!box){
      box=document.createElement('span'); box.id='edHist'; box.style.cssText='display:inline-flex;gap:2px;margin-right:6px';
      for(const [dir, t] of [[-1, '◀'], [1, '▶']]){
        const b=document.createElement('button'); b.className='btn dim ic'; b.textContent=t; b.dataset.dir=dir;
        b.onclick=()=>this.go(dir); box.append(b);
      }
      head.insertBefore(box, head.firstChild);
    }
    const [bk, fw]=box.querySelectorAll('button');
    bk.disabled=!this.back.length; fw.disabled=!this.fwd.length;
    bk.title=this.back.length? 'Back to '+this.label(this.back[this.back.length-1])+' (Alt+Left)' : 'Back (Alt+Left)';
    fw.title=this.fwd.length? 'Forward to '+this.label(this.fwd[this.fwd.length-1])+' (Alt+Right)' : 'Forward (Alt+Right)';
  },
  bind(){
    if(typeof WgEditor!=='object' || WgEditor._histBound) return;
    WgEditor._histBound=true;
    const openRecord=WgEditor.openRecord, openRef=WgEditor.openRef, dialog=WgEditor.dialog;
    WgEditor.openRecord=function(tag, id, plugins){ WgEdHistory.push({kind:'rec', tag, id, plugins:plugins||null}); return openRecord.apply(this, arguments); };
    WgEditor.openRef=function(ref){ WgEdHistory.push({kind:'ref', ref:Object.assign({}, ref)}); return openRef.apply(this, arguments); };
    WgEditor.dialog=function(){ const d=dialog.apply(this, arguments); WgEdHistory.draw(); return d; };
    window.addEventListener('keydown', e=>{
      if(typeof WgKeys!=='object' || !document.body.classList.contains('wgEditMode')) return;
      const dir=WgKeys.is(e, 'histBack')? -1 : WgKeys.is(e, 'histFwd')? 1 : 0;
      if(!dir) return;
      e.preventDefault(); e.stopPropagation();
      this.go(dir);
    }, true);
  }
};

const WgEdDiff={
  bind(){
    if(typeof WgEditor!=='object' || WgEditor._diffBound) return;
    WgEditor._diffBound=true;
    const drawPending=WgEditor.drawPending;
    WgEditor.drawPending=function(list){ drawPending.apply(this, arguments); WgEdDiff.decorate(list||[]); };
  },
  /** A "Diff" beside each record's Open in Pending, and "Diff all" at the top. */
  decorate(list){
    const E=WgEditor, body=E.pend && E.pend.querySelector('#edPendBody'); if(!body) return;
    const opens=[...body.querySelectorAll('[data-open]')];
    if(!opens.length) return;
    for(const b of opens){
      const d=document.createElement('button'); d.className='btn sm'; d.textContent='Diff';
      d.title='Each changed field: what the load order has, and what the patch writes';
      d.onclick=()=>this.toggle(b.closest('.orirow'), b.dataset.open, b.dataset.id);
      b.after(' ', d);
    }
    const all=document.createElement('button'); all.className='btn sm'; all.textContent='Diff all';
    all.title='Every record\'s changes, field by field (the first 60)';
    all.onclick=async()=>{ for(const b of opens.slice(0, 60)) await this.toggle(b.closest('.orirow'), b.dataset.open, b.dataset.id, true); };
    const top=document.createElement('div'); top.className='orirow'; const v=document.createElement('span'); v.className='v'; v.append(all); top.append(v);
    body.insertBefore(top, body.children[1]||null);
  },
  async toggle(row, tag, id, open){
    if(!row) return;
    const next=row.nextElementSibling;
    if(next && next.classList.contains('edDiff')){ if(!open) next.remove(); return; }
    const box=document.createElement('div'); box.className='edDiff'; box.style.cssText='margin:2px 0 8px 8px;font-size:11px';
    box.textContent='Reading '+id+'…';
    row.after(box);
    let v;
    try{ v=await WgEditor.ask('editRecord', {tag, id}); }catch(e){ box.textContent=String(e.message||e); return; }
    this.fill(box, tag, id, v);
  },
  show(x){ if(x==null) return '—'; if(typeof x==='string') return x.length>160? x.slice(0, 160)+'…' : x; return JSON.stringify(x); },
  fill(box, tag, id, v){
    box.textContent='';
    const changed=(v.fields||[]).filter(f=>('queued' in f));
    if(!changed.length){ box.textContent='Nothing differs field by field (a whole record, or a new one).'; return; }
    const t=document.createElement('table'); t.style.cssText='border-collapse:collapse;width:100%';
    const head=document.createElement('tr');
    for(const h of ['Field', 'The load order has', 'The patch writes', '']){ const th=document.createElement('th'); th.textContent=h; th.style.cssText='text-align:left;opacity:.7;font-weight:normal;padding:1px 4px'; head.append(th); }
    t.append(head);
    for(const f of changed){
      const tr=document.createElement('tr');
      const cell=(text, style)=>{ const td=document.createElement('td'); td.textContent=text; td.style.cssText='padding:1px 4px;vertical-align:top;'+(style||''); tr.append(td); return td; };
      cell(f.path, 'font-family:var(--mono)');
      cell(this.show(f.value), 'opacity:.75');
      cell(this.show(f.queued)+(f.source && f.source!=='typed'? '  (from '+f.source+')' : ''), 'color:var(--acc,#6cb6ff)');
      const td=cell('');
      const b=document.createElement('button'); b.className='btn dim ic'; b.textContent='↶'; b.title='Take back this field alone';
      b.onclick=async()=>{
        try{ await WgEditor.ask('editRevert', {tag, id, path:f.path}); }catch(e){ toast(String(e.message||e), 'err', 4000); return; }
        await WgEditor.refreshPending();
      };
      td.append(b);
      t.append(tr);
    }
    box.append(t);
  }
};

const WgScriptHelp={
  bind(){
    if(typeof WgEditor!=='object' || WgEditor._scrHelpBound) return;
    WgEditor._scrHelpBound=true;
    const openScript=WgEditor.openScript;
    WgEditor.openScript=async function(){ const r=await openScript.apply(this, arguments); WgScriptHelp.attach(this.scr); return r; };
  },
  /** The word at the caret: [start, end, word]. */
  wordAt(ta){
    const s=ta.value, at=ta.selectionStart;
    let a=at, b=at;
    while(a>0 && /[\w.\-'"]/.test(s[a-1])) a--;
    while(b<s.length && /[\w.\-'"]/.test(s[b])) b++;
    return [a, b, s.slice(a, b)];
  },
  /** The script's own variables (`short`, `long`, `float` lines). */
  locals(text){
    const out=[];
    for(const m of String(text).matchAll(/^\s*(?:short|long|float)\s+([A-Za-z_][\w]*)/gim)) out.push(m[1]);
    return out;
  },
  attach(d){
    if(!d || d._help) return;
    d._help=true;
    const ta=d.querySelector('#edScriptText'); if(!ta) return;
    const row=d.querySelector('#edScriptSave') && d.querySelector('#edScriptSave').parentElement;
    if(row){
      const b=document.createElement('button'); b.className='btn sm'; b.textContent='Where is it set?';
      b.title='Every line of every script that sets the variable under the caret';
      b.onclick=()=>this.writes(d, ta);
      row.append(' ', b);
    }
    const keys=ta.onkeydown;
    ta.onkeydown=e=>{
      if(this._pop){
        if(e.key==='ArrowDown' || e.key==='ArrowUp'){ e.preventDefault(); e.stopPropagation(); this.move(e.key==='ArrowDown'? 1 : -1); return; }
        if(e.key==='Enter' || e.key==='Tab'){ e.preventDefault(); e.stopPropagation(); this.accept(ta); return; }
        if(e.key==='Escape'){ e.preventDefault(); e.stopPropagation(); this.close(); return; }
      }
      if(e.key===' ' && e.ctrlKey){ e.preventDefault(); e.stopPropagation(); this.complete(ta); return; }
      if(keys) keys.call(ta, e);
    };
    ta.addEventListener('blur', ()=>setTimeout(()=>this.close(), 150));
  },
  async complete(ta){
    const [a, b, word]=this.wordAt(ta);
    const w=word.replace(/^['"]/, '');
    const local=this.locals(ta.value).filter(v=>v.toLowerCase().startsWith(w.toLowerCase()) && v.toLowerCase()!==w.toLowerCase()).map(v=>({text:v, hint:'variable'}));
    let recs=[];
    if(w.length>=2){
      try{ recs=await Engine.call('record_search', {q:w, limit:20}); if(typeof recs==='string') recs=JSON.parse(recs); }catch(_){ recs=[]; }
    }
    const items=local.concat((recs||[]).map(r=>({text:r.id, hint:r.tag})));
    if(!items.length){ toast('Nothing to complete '+(w||'here'), 'warn', 1500); return; }
    this.open(ta, a, b, items);
  },
  open(ta, a, b, items){
    this.close();
    const pop=document.createElement('div');
    pop.style.cssText='position:fixed;z-index:9999;background:var(--panel,#1d1f22);border:1px solid var(--line,#444);border-radius:4px;max-height:220px;overflow:auto;min-width:220px;font-size:12px';
    const r=ta.getBoundingClientRect(), lines=ta.value.slice(0, a).split('\n'), lh=parseFloat(getComputedStyle(ta).lineHeight)||16;
    pop.style.left=Math.min(window.innerWidth-240, r.left+8+Math.min(60, lines[lines.length-1].length)*7)+'px';
    pop.style.top=Math.min(window.innerHeight-230, r.top+4+(lines.length-ta.scrollTop/lh)*lh)+'px';
    this._pop={el:pop, a, b, items, sel:0};
    items.forEach((it, i)=>{
      const row=document.createElement('div'); row.style.cssText='padding:2px 6px;cursor:pointer;display:flex;gap:8px';
      const t=document.createElement('span'); t.textContent=it.text; t.style.fontFamily='var(--mono)';
      const h=document.createElement('span'); h.className='hint'; h.textContent=it.hint;
      row.append(t, h);
      row.onmousedown=e=>{ e.preventDefault(); this._pop.sel=i; this.accept(ta); };
      pop.append(row);
    });
    document.body.append(pop);
    this.move(0);
  },
  move(d){
    const P=this._pop; if(!P) return;
    P.sel=Math.max(0, Math.min(P.items.length-1, P.sel+d));
    [...P.el.children].forEach((c, i)=>{ c.style.background=i===P.sel? 'var(--sel,#2d4a6b)' : ''; });
    const c=P.el.children[P.sel]; if(c && c.scrollIntoView) c.scrollIntoView({block:'nearest'});
  },
  accept(ta){
    const P=this._pop; if(!P) return;
    const it=P.items[P.sel], s=ta.value;
    const q=/^['"]/.test(s.slice(P.a, P.b))? s[P.a] : '';
    // An id with a space in it is quoted, as MWScript wants it.
    const text=q? q+it.text+q : /\s/.test(it.text)? '"'+it.text+'"' : it.text;
    ta.value=s.slice(0, P.a)+text+s.slice(P.b);
    ta.selectionStart=ta.selectionEnd=P.a+text.length;
    this.close();
    ta.dispatchEvent(new Event('input'));
    ta.focus();
  },
  close(){ if(this._pop){ this._pop.el.remove(); this._pop=null; } },
  async writes(d, ta){
    const [, , word]=this.wordAt(ta);
    const name=word.split('.').pop().replace(/^['"]|['"]$/g, '');
    let box=d.querySelector('#edScriptWrites');
    if(!box){ box=document.createElement('div'); box.id='edScriptWrites'; box.style.cssText='font-size:11px;max-height:200px;overflow:auto;margin-top:4px'; (d.querySelector('#edScriptNote')||ta).after(box); }
    box.textContent='';
    if(!name){ box.textContent='Put the caret on a variable first.'; return; }
    box.textContent='Looking for where '+name+' is set…';
    let hits=[];
    try{ hits=await Engine.call('script_writes', {name}); if(typeof hits==='string') hits=JSON.parse(hits); }catch(e){ box.textContent=String(e.message||e); return; }
    box.textContent='';
    const h=document.createElement('div'); h.className='hint';
    h.textContent=hits.length? name+' is set in '+hits.length+' line'+(hits.length===1? '' : 's')+' (dialogue results: the Dialogue window\'s Flags view)' : 'No script sets '+name+'.';
    box.append(h);
    for(const x of hits){
      const r=document.createElement('div'); r.style.cssText='display:flex;gap:6px;cursor:pointer;padding:1px 0';
      const a=document.createElement('span'); a.textContent=x.id+':'+x.line; a.style.cssText='flex:none;color:var(--acc,#6cb6ff)';
      const t=document.createElement('span'); t.textContent=x.text; t.style.fontFamily='var(--mono)';
      r.append(a, t);
      r.title='Open '+x.id;
      r.onclick=()=>WgEditor.openScript('SCPT', x.id, null);
      box.append(r);
    }
  }
};

/** A dialogue response's placeholders, checked as it is typed. */
const WgTextCheck={
  /** The game's text defines (what a response's `%` names besides a global variable). */
  DEFINES:['name','pcname','race','pcrace','class','pcclass','faction','rank','pcrank','nextpcrank','pcnextrank','cell','pccrimelevel'],
  /** `[{what, at}]`: the placeholders that are not defines (each to be looked up as a global),
   *  and each `@` without its `#`. */
  scan(text){
    const out={unknown:[], links:[]};
    const s=String(text||'');
    for(const m of s.matchAll(/%([A-Za-z_][\w]*)/g)) if(!this.DEFINES.includes(m[1].toLowerCase()) && !out.unknown.includes(m[1])) out.unknown.push(m[1]);
    for(const m of s.matchAll(/@([^@#]*)(#|$)/g)) if(m[2]!=='#') out.links.push(m[1].slice(0, 30));
    return out;
  },
  bind(){
    if(typeof WgEditor!=='object' || WgEditor._textBound) return;
    WgEditor._textBound=true;
    const drawRecord=WgEditor.drawRecord;
    WgEditor.drawRecord=function(v){ const r=drawRecord.apply(this, arguments); if(r!==false) WgTextCheck.attach(this); return r; };
  },
  attach(E){
    const d=E.dlg; if(!d || !E.record || E.record.tag!=='INFO') return;
    const ta=d.querySelector('textarea[data-path="text"]'); if(!ta) return;
    const note=document.createElement('div'); note.className='hint'; note.style.cssText='margin:2px 0 4px';
    ta.after(note);
    const run=async()=>{
      const r=this.scan(ta.value), bad=[];
      for(const u of r.unknown){
        let tag='';
        try{ let x=await Engine.call('editor_record_tag', {id:u}); if(typeof x==='string') x=JSON.parse(x); tag=x && x.tag || ''; }catch(_){ }
        if(tag!=='GLOB') bad.push('%'+u+' is neither a text define nor a global variable');
      }
      for(const l of r.links) bad.push('@'+l+' has no closing #');
      note.textContent=bad.join(' · ');
      note.style.color=bad.length? 'var(--warn,#d8a14a)' : '';
    };
    let t=0;
    ta.addEventListener('input', ()=>{ clearTimeout(t); t=setTimeout(run, 400); });
    run();
  }
};

WgEdHistory.bind(); WgEdDiff.bind(); WgScriptHelp.bind(); WgTextCheck.bind();
