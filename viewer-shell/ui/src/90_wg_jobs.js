/* =====================================================================================
   Wraithguard: the jobs panel (Static 2026-11-24, "lets do all this": "a jobs panel
   (progress, cancel, open output folder)").

   Every long command the pages start - Merge Lands, Merge objects, the Mesh optimizer, Check
   meshes, the Lua check, tes3cmd, Save Check, the Conflicts scans, the Load order's sort and
   export - is seen as it goes through the engine (`Engine.inv`), with no page changed for it.
   The top bar's **Jobs** says how many run; its panel lists the last ones: how long each has
   taken, how far it has got where the engine counts (the mesh tools' meshes, Merge Lands'
   last line), **Cancel** where the engine can stop it (Merge Lands, the mesh writes, the Lua
   check), and **Open folder** for what one wrote, once done (a folder the person chose,
   `open_path`). A job shorter than half a second is not listed. Built with the DOM.
   Copyright (c) 2026 StaticNation.
   ===================================================================================== */
const WgJobs={
  list:[], _n:0, _poll:0, SHOW_AFTER:500, MAX:20,
  /** What each long command is: its label, its output (from its arguments), how to stop it, how
   *  far it has got. */
  KINDS:{
    land_merge:{label:'Merge Lands', out:a=>a.folder, cancel:'land_merge_cancel', lines:true},
    tools_merge_objects:{label:'Merge objects', out:a=>a.folder},
    tools_merge_master:{label:'Merge to master', out:a=>a.out},
    meshopt_analyse:{label:'Mesh optimizer: reading the meshes', count:true},
    meshopt_write:{label:'Mesh optimizer: writing', out:a=>a.outDir, cancel:'meshes_cancel', count:true},
    meshcheck_scan:{label:'Check meshes: reading the meshes', count:true},
    meshcheck_write:{label:'Check meshes: writing the repairs', out:a=>a.outDir, cancel:'meshes_cancel', count:true},
    lua_check:{label:'Lua mods: running each mod', cancel:'lua_check_cancel'},
    tools_tes3cmd:{label:a=>'tes3cmd '+(a.cmd||'')},
    tools_save_check:{label:'Save Check'},
    conflicts_scan:{label:'Conflicts: scanning the records'},
    conflicts_resources:{label:'Conflicts: scanning the files'},
    conflicts_by_plugin:{label:'Conflicts: by plugin'},
    loadorder_compute:{label:'Load order: sort'},
    loadorder_export:{label:'Load order: export'},
    loadorder_plain_sort:{label:'Load order: sort by the rules'},
    loadorder_plain_write:{label:'Load order: export', out:a=>a.cfg},
  },

  /** A command started: a job, if it is a long one. */
  begin(cmd, args){
    const K=this.KINDS[cmd]; if(!K) return null;
    let a=args||{};
    // Commands that take their arguments as one JSON string.
    if(typeof a.args==='string'){ try{ a=Object.assign({}, a, JSON.parse(a.args)); }catch(_){ } }
    const j={id:++this._n, cmd, K, label:typeof K.label==='function'? K.label(a) : K.label, out:K.out? String(K.out(a)||'').trim() : '',
      t0:performance.now(), t1:0, status:'running', done:0, total:0, line:'', from:0, err:''};
    this.list.unshift(j);
    if(this.list.length>this.MAX) this.list.length=this.MAX;
    this.watch();
    return j;
  },
  end(j, err){
    if(!j) return;
    j.t1=performance.now();
    j.status=err? (/cancel/i.test(String(err))? 'cancelled' : 'failed') : (j.asked? 'cancelled' : 'done');
    j.err=err? String(err && err.message || err).slice(0, 300) : '';
    this.draw();
  },
  running(){ return this.list.filter(j=>j.status==='running'); },
  /** Shown once it has run long enough to matter. */
  shown(j){ return (j.t1 || performance.now())-j.t0>=this.SHOW_AFTER; },

  /** While any runs: how far each has got, twice a second. */
  watch(){
    if(this._poll) return;
    this._poll=setInterval(()=>this.tick(), 500);
  },
  async tick(){
    const run=this.running();
    if(!run.length){ clearInterval(this._poll); this._poll=0; this.draw(); return; }
    for(const j of run){
      try{
        if(j.K.count){ const [d, t]=await Engine.call('meshopt_progress', {}); j.done=+d||0; j.total=+t||0; }
        if(j.K.lines){
          let r=await Engine.call('land_merge_progress', {from:j.from});
          if(typeof r==='string') r=JSON.parse(r);
          if(r && r.lines && r.lines.length){ j.line=String(r.lines[r.lines.length-1]); j.from=r.next; }
        }
      }catch(_){ }
    }
    this.draw();
  },
  async cancel(j){
    if(!j || j.status!=='running' || !j.K.cancel) return;
    j.asked=true; j.line='Stopping…';
    try{ await Engine.call(j.K.cancel, {}); }catch(e){ toast(String(e.message||e), 'err', 4000); }
    this.draw();
  },
  async open(j){
    try{ await Engine.call('open_path', {path:j.out}); }catch(e){ toast(String(e.message||e), 'err', 5000); }
  },

  /* ---- the top bar's button, and the panel ---- */
  el(tag, cls, text){ const e=document.createElement(tag); if(cls) e.className=cls; if(text!=null) e.textContent=text; return e; },
  button(){
    let b=document.getElementById('btnJobs');
    if(b) return b;
    const near=document.getElementById('btnReport'); if(!near) return null;
    b=this.el('button', 'btn dim', 'Jobs'); b.id='btnJobs'; b.hidden=true;
    b.title='The long jobs: how far each has got, Cancel, and the folder one wrote';
    b.onclick=()=>{ this._open=!this._open; this.draw(); };
    near.after(b);
    return b;
  },
  secs(ms){ const s=ms/1000; return s<60? s.toFixed(s<10? 1 : 0)+' s' : Math.floor(s/60)+' min '+Math.round(s%60)+' s'; },
  draw(){
    const b=this.button();
    const shown=this.list.filter(j=>this.shown(j)), run=shown.filter(j=>j.status==='running');
    if(b){ b.hidden=!shown.length; b.textContent=run.length? 'Jobs ('+run.length+')' : 'Jobs'; b.classList.toggle('pri', !!run.length); }
    let p=document.getElementById('wgJobs');
    if(!this._open || !shown.length){ if(p) p.hidden=true; return; }
    if(!p){
      p=this.el('div'); p.id='wgJobs';
      p.style.cssText='position:fixed;z-index:90;right:12px;top:calc(var(--top-h,40px) + 8px);width:min(440px,92vw);max-height:60vh;overflow:auto;'+
        'background:var(--bg2);color:var(--tx);border:1px solid var(--line2);border-radius:var(--r-box,6px);box-shadow:0 6px 24px rgba(0,0,0,.45);font-size:12px;padding:6px 8px';
      document.body.append(p);
    }
    p.hidden=false; p.textContent='';
    const h=this.el('div'); h.style.cssText='display:flex;align-items:center;gap:6px;margin-bottom:4px';
    h.append(this.el('b', '', 'Jobs'), this.el('span'));
    h.lastChild.style.flex='1';
    const clr=this.el('button', 'btn dim', 'Clear finished'); clr.onclick=()=>{ this.list=this.list.filter(j=>j.status==='running'); this.draw(); };
    const x=this.el('button', 'btn dim', '✕'); x.title='Close'; x.onclick=()=>{ this._open=false; this.draw(); };
    h.append(clr, x); p.append(h);
    for(const j of shown){
      const r=this.el('div'); r.style.cssText='border-top:1px solid var(--line);padding:5px 0';
      const top=this.el('div'); top.style.cssText='display:flex;gap:6px;align-items:center';
      const name=this.el('b', '', j.label); name.style.cssText='flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap';
      const st=this.el('span', '', j.status==='running'? this.secs(performance.now()-j.t0) : j.status+' · '+this.secs(j.t1-j.t0));
      st.style.color=j.status==='failed'? 'var(--err)' : j.status==='cancelled'? 'var(--warn)' : j.status==='done'? 'var(--grow)' : 'var(--tx2)';
      top.append(name, st);
      if(j.status==='running' && j.K.cancel){ const c=this.el('button', 'btn dim', j.asked? 'Stopping…' : 'Cancel'); c.disabled=!!j.asked; c.onclick=()=>this.cancel(j); top.append(c); }
      if(j.status!=='running' && j.out){ const o=this.el('button', 'btn dim', 'Open folder'); o.title=j.out; o.onclick=()=>this.open(j); top.append(o); }
      r.append(top);
      if(j.status==='running' && j.total>0){
        const bar=this.el('div'); bar.style.cssText='height:4px;background:var(--bg4,var(--bg3));border-radius:2px;margin-top:4px;overflow:hidden';
        const fill=this.el('div'); fill.style.cssText='height:100%;background:var(--ac);width:'+Math.min(100, Math.round(j.done/j.total*100))+'%';
        bar.append(fill); r.append(bar, this.el('div', 'hint', j.done+' of '+j.total));
      }
      if(j.line && j.status==='running'){ const l=this.el('div', 'hint', j.line); l.style.cssText='white-space:nowrap;overflow:hidden;text-overflow:ellipsis'; r.append(l); }
      if(j.err){ const e=this.el('div', 'hint', j.err); e.style.color='var(--err)'; r.append(e); }
      p.append(r);
    }
  },

  /** Every call through the engine watched: the long ones become jobs. */
  bind(){
    if(typeof Engine!=='object' || Engine._jobs) return;
    Engine._jobs=true;
    const inv=Engine.inv;
    Engine.inv=async function(cmd, args, opts){
      const j=WgJobs.begin(cmd, args);
      if(j) setTimeout(()=>WgJobs.draw(), WgJobs.SHOW_AFTER+20);
      try{ const r=await inv.call(this, cmd, args, opts); WgJobs.end(j, null); return r; }
      catch(e){ WgJobs.end(j, e); throw e; }
    };
  }
};
WgJobs.bind();
