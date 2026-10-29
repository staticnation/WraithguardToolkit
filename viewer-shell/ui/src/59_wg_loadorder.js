/* =====================================================================================
   The Load order page - Wraithguard (port step 4.2, slice 2k). Sort and Export, as
   Wraithguard's main window had them, in the viewer, which becomes the app's main window:
   the Load order, Cell viewer and Editor are its pages.

   - Inputs: the openmw.cfg, a customizations.toml, a subset file or a mods folder scanned
     into the sort, the rule files, MOMW's plugin-order.yml and the list's name, a data-paths
     cache; the options as the Tk window had them. Remembered in this viewer.
   - Sort (`loadorder_compute`, ordercore::plan): the two orders - content= and data= - each
     row draggable, a box to leave it out, the person's own mods marked, plugins with a
     master problem marked; and the warnings as a list, filterable by kind, each naming
     its plugins as links that find them in the orders. The run's full text behind a toggle.
   - Export (`loadorder_export`, ordercore::plan::export): the orders as the panels have
     them, what was left out (a removeContent / removeData where the curated list would
     bring it back), the cfg and/or the customizations TOML, a dry run, no backup. It also
     writes the plan file the Tk tools still waiting for 4.3 / 4.4 read their order from
     (`extra.loadorderPlan`, beside the patch journal).

   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */

const WgLoadOrder={
  on:false,
  plan:null,
  report:[],
  filter:'all',
  showText:false,
  q:{content:'', data:''},    // what each order's Find box says (lower case)
  cur:{content:-1, data:-1}, // the match Enter is on, per order
  content:[],  // [{name, on, mine}]
  data:[],     // [{line, on, isNew}]
  KEY:'wgLoadOrder.fields',

  /** The warning kinds the report tags, as the filter names them. */
  KINDS:{rule:'Rule warnings', yml:'plugin-order.yml', master:'Masters', master_size:'Master sizes',
    data_path:'Data paths', stale:'Stale merges', rule_conflict:'Rules not applied', drift:'Drift',
    groundcover:'Groundcover', orphan:'Unmanaged'},

  FIELDS:[
    ['cfg','openmw.cfg','file'], ['customizations','customizations.toml','file'],
    ['subsetFile','Subset file','file'], ['scanDir','Mods folder (scanned into the sort)','dir'],
    ['rules','Rule files (one a line, mlox_base.txt first)','text'],
    ['pluginOrderYml','plugin-order.yml','file'], ['listName','List name','plain'],
    ['dataPathsCache','Data-paths cache','file'], ['groundcover','Groundcover (one a line)','text'],
    ['emitToml','Write customizations to (empty: back into customizations.toml)','save'],
    ['userRules','Your rule file (the rule maker writes here)','save'],
  ],
  FLAGS:[
    ['sortDataPaths','Sort data paths'], ['subsetFromCfg','Pull unmanaged entries from the cfg'],
    ['noPredicateWarnings','No rule warnings'], ['verbose','List every missing data path'],
    ['writeCfg','Write openmw.cfg'], ['dryRun','Dry run'], ['noBackup','No backup'],
  ],

  button(){
    if($('#btnLoadOrder')) return;
    const after=$('#btnReport'); if(!after) return;
    const b=document.createElement('button');
    b.className='btn dim'; b.id='btnLoadOrder'; b.textContent='Load order';
    b.title='Sort and export the load order: your mods placed into the curated order by the mlox rules, the warnings, and openmw.cfg / customizations.toml written';
    b.onclick=()=> this.on? this.leave() : this.enter();
    after.insertAdjacentElement('afterend', b);
  },

  enter(){
    if(this.on) return;
    const first=!$('#wgLO');
    if(typeof WgConflicts==='object' && WgConflicts.on) WgConflicts.leave();
    if(typeof WgToolsPage==='object' && WgToolsPage.on) WgToolsPage.leave();
    if(typeof WgHelp==='object' && WgHelp.on) WgHelp.leave();
    this.on=true;
    this.build();
    $('#wgLO').hidden=false;
    const i=document.querySelector('#brand i'); if(i){ this._brand=i.textContent; i.textContent='Load order'; }
    document.title='Wraithguard - Load order';
    const b=$('#btnLoadOrder'); if(b) b.classList.add('on');
    if(first) this.seed(); else this.ages();
  },

  /** The first open: the fields Wraithguard knows (its settings file, its openmw.cfg), then a
   *  sort, so the page shows the install's order and warnings instead of empty panels. The
   *  settings file (the Setup page's, which this page writes back to) wins over what this
   *  page remembered; a Tk window's own openmw.cfg wins over both, as its field says the page
   *  opens on it. */
  async seed(){
    const x=(window.__WG_VIEW__||{}).extra||{};
    let seed={};
    try{ seed=(await WgSettings.seed()).loadorder||{}; }catch(_){}
    const v=this.saved();
    void v;
    this.take(seed, true);
    if(x.loadorderCfg) $('#lo_cfg').value=x.loadorderCfg;
    this.save(true);
    this.ages();
    if(!this.val('cfg')){
      $('#loContent').innerHTML='<li class="empty">Choose your openmw.cfg on the left, add the rule files (mlox_base.txt first), then Sort.</li>';
      return;
    }
    if(!this.lines('rules').length) toast('No rule files yet: the order is the cfg\'s as it is. Add mlox_base.txt (and your own) on the left, then Sort.','warn',7000);
    await this.sort();
  },

  leave(){
    if(!this.on) return;
    this.on=false;
    $('#wgLO').hidden=true;
    const i=document.querySelector('#brand i'); if(i && this._brand) i.textContent=this._brand;
    const b=$('#btnLoadOrder'); if(b) b.classList.remove('on');
  },

  /** Fields from the settings file (the Setup page's names, which are this page's). */
  take(f, quiet){
    for(const [k] of this.FIELDS) if(f[k]!=null && $('#lo_'+k)) $('#lo_'+k).value=String(f[k]);
    for(const [k] of this.FLAGS) if(typeof f[k]==='boolean' && $('#lo_'+k)) $('#lo_'+k).checked=f[k];
    if(!quiet) this.save(true);
  },

  /* ---- the fields, remembered ---- */
  saved(){ try{ return JSON.parse(localStorage.getItem(this.KEY)||'{}')||{}; }catch(_){ return {}; } },
  /** Kept in this viewer and in the settings file every page reads (`local`: this viewer only,
   *  for a value the settings already hold). */
  save(local){
    const v={};
    for(const [k] of this.FIELDS) v[k]=$('#lo_'+k).value;
    for(const [k] of this.FLAGS) v[k]=$('#lo_'+k).checked;
    try{ localStorage.setItem(this.KEY, JSON.stringify(v)); }catch(_){}
    if(!local && typeof WgSettings==='object') WgSettings.write(v);
  },
  lines(k){ return $('#lo_'+k).value.split('\n').map(s=>s.trim()).filter(Boolean); },
  val(k){ return $('#lo_'+k).value.trim(); },
  flag(k){ return $('#lo_'+k).checked; },

  build(){
    if($('#wgLO')) return;
    const st=document.createElement('style');
    st.textContent=`
#wgLO{position:fixed;left:0;right:0;top:var(--top-h,40px);bottom:0;z-index:60;background:var(--bg);color:var(--tx);
  display:grid;grid-template-columns:320px 1fr 1fr;gap:8px;padding:8px;overflow:hidden;font-size:12px}
#wgLO[hidden]{display:none}
#wgLO .lo-col{display:flex;flex-direction:column;min-height:0;background:var(--bg2);border:1px solid var(--line);border-radius:var(--r-box)}
#wgLO h3{margin:0;padding:6px 8px;font-size:12px;border-bottom:1px solid var(--line);display:flex;gap:6px;align-items:center}
#wgLO h3 .sp{flex:1}
#wgLO .lo-scroll{overflow:auto;flex:1;min-height:0;padding:6px 8px}
#wgLO label.f{display:block;margin:6px 0 2px;color:var(--tx2)}
#wgLO .row{display:flex;gap:4px}
#wgLO input[type=text],#wgLO textarea,#wgLO select{flex:1;min-width:0;background:var(--bg3);color:var(--tx);border:1px solid var(--line2);border-radius:var(--r-fld);padding:3px 5px;font:inherit}
#wgLO textarea{height:44px;resize:vertical;font-family:var(--mono)}
#wgLO .flags label{display:block;margin:3px 0}
#wgLO .acts{display:flex;gap:6px;padding:8px;border-top:1px solid var(--line)}
#wgLO ol{margin:0;padding:0;list-style:none;font-family:var(--mono)}
#wgLO li{display:flex;gap:6px;align-items:center;padding:1px 4px;border-radius:2px;cursor:grab}
#wgLO li.off span{opacity:.45;text-decoration:line-through}
#wgLO li.mine span{color:var(--ac)}
#wgLO li.new span{color:var(--ac)}
#wgLO li.bad span{color:var(--err)}
#wgLO li.hit{background:var(--acd);outline:1px solid var(--ac)}
#wgLO li.found{box-shadow:3px 0 0 var(--ac) inset;background:color-mix(in srgb,var(--ac) 14%,transparent)}
#wgLO li.found.cur{background:color-mix(in srgb,var(--ac) 32%,transparent);outline:1px dotted var(--ac)}
#wgLO h3 .lo-q{flex:none;width:170px;padding:1px 5px}
#wgLO h3 .qn{color:var(--tx3);font-weight:normal;min-width:34px;text-align:right}
#wgLO li.drop{box-shadow:0 -2px 0 var(--ac) inset}
#wgLO li.dropb{box-shadow:0 2px 0 var(--ac) inset}
#wgLO .lo-col.droptarget{outline:2px dashed var(--ac);outline-offset:-3px}
#wgLO .orders{display:grid;grid-template-rows:2fr 1fr;gap:8px;min-height:0}
#wgLO .chips{display:flex;flex-wrap:wrap;gap:4px;padding:6px 8px;border-bottom:1px solid var(--line)}
#wgLO .chip{background:var(--bg3);border:1px solid var(--line2);border-radius:10px;padding:1px 8px;cursor:pointer;color:var(--tx2)}
#wgLO .chip.on{background:var(--acd);border-color:var(--ac);color:var(--tx)}
#wgLO .warn{border-left:3px solid var(--warn);padding:4px 6px;margin:0 0 6px;background:var(--bg3);white-space:pre-wrap}
#wgLO .warn.master,#wgLO .warn.drift{border-color:var(--err)}
#wgLO .warn .kind{color:var(--tx3);font-size:11px;display:block}
#wgLO .warn a{color:var(--ac);cursor:pointer;margin-right:8px}
#wgLO pre{margin:0;font-family:var(--mono);white-space:pre-wrap;color:var(--tx2)}
#wgLO .empty{color:var(--tx3);padding:8px}`;
    document.head.appendChild(st);

    const el=document.createElement('div');
    el.id='wgLO'; el.hidden=true;
    const pick=(k,kind)=> kind==='plain'||kind==='text' ? '' : `<button class="btn dim" data-pick="${k}" data-kind="${kind}">…</button>`;
    const field=([k,label,kind])=> `<label class="f" for="lo_${k}">${label}</label><div class="row">${
      kind==='text' ? `<textarea id="lo_${k}"></textarea>` : `<input type="text" id="lo_${k}">`}${pick(k,kind)}</div>`;
    el.innerHTML=`
<div class="lo-col"><h3>Inputs</h3><div class="lo-scroll">
  ${this.FIELDS.map(field).join('')}
  <div class="flags">${this.FLAGS.map(([k,l])=>`<label><input type="checkbox" id="lo_${k}"> ${l}</label>`).join('')}</div>
  <label class="f">Rule files</label><div id="loAges" class="empty">-</div>
  <div class="row" style="margin-top:4px;flex-wrap:wrap">
    <button class="btn dim" id="loUpRules" title="Download the managed rule files again (each old one kept as a .bak)">Update rules</button>
    <button class="btn dim" id="loUpYml" title="Download MOMW's plugin-order.yml to the path above">Update plugin-order.yml</button>
    <button class="btn dim" id="loUpData" title="Fetch the list's data-path order to the data-paths cache above">Fetch data paths</button>
  </div>
  <label class="f">Rules of your own</label>
  <div class="row"><button class="btn dim" id="loRuleOpen" title="Write an mlox rule - every kind the guidelines describe - checked as you write it, and add it to your rule file">Rule maker...</button></div>
</div><div class="acts"><button class="btn" id="loSort">Sort</button><button class="btn" id="loExport" disabled>Export</button></div></div>
<div class="orders">
  <div class="lo-col"><h3>content= <span class="sp"></span><input type="text" class="lo-q" id="loQcontent" placeholder="Find...  (Enter: next)" spellcheck="false" title="Lights the plugins whose name has this text. Enter goes to the next, Shift+Enter to the one before, Esc clears."><span class="qn" id="loQncontent"></span><span id="loCount"></span></h3><div class="lo-scroll"><ol id="loContent"><li class="empty">Sort to see the order.</li></ol></div></div>
  <div class="lo-col"><h3>data= <span class="sp"></span><input type="text" class="lo-q" id="loQdata" placeholder="Find...  (Enter: next)" spellcheck="false" title="Lights the data folders whose line has this text. Enter goes to the next, Shift+Enter to the one before, Esc clears."><span class="qn" id="loQndata"></span></h3><div class="lo-scroll"><ol id="loData"></ol></div></div>
</div>
<div class="lo-col"><h3>Warnings <span class="sp"></span><label><input type="checkbox" id="loText"> Full text</label></h3>
  <div class="chips" id="loChips"></div><div class="lo-scroll" id="loWarn"><div class="empty">-</div></div></div>`;
    document.body.appendChild(el);
    // The inputs, the orders and the warnings, resized by the bars between them.
    if(typeof WgSplit==='object'){
      WgSplit.make(el, 'lo', [330, 620]);
      // content= over data=, the bar between them
      WgSplit.rows(el.querySelector('.orders'), 'lo.orders', 2/3);
    }
    this.dropArm();
    for(const k of ['content','data']){
      const inp=$('#loQ'+k);
      inp.addEventListener('input', ()=>{ this.q[k]=inp.value.trim().toLowerCase(); this.cur[k]=-1; this.markFound(k, 0); });
      inp.addEventListener('keydown', e=>{
        if(e.key==='Enter'){ e.preventDefault(); this.markFound(k, e.shiftKey? -1 : 1); }
        else if(e.key==='Escape'){ inp.value=''; this.q[k]=''; this.cur[k]=-1; this.markFound(k, 0); }
        e.stopPropagation(); // typing here is not a shortcut of the window underneath
      });
    }

    const v=this.saved();
    const view=window.__WG_VIEW__||{};
    for(const [k] of this.FIELDS) $('#lo_'+k).value = v[k]!=null ? v[k] : '';
    // the launch's cfg only when Wraithguard did not name its own (it can be a generated
    // setup copy, which must not be sorted and exported over)
    if(!this.val('cfg') && view.cfg && !(view.extra||{}).loadorderCfg) $('#lo_cfg').value=view.cfg;
    for(const [k] of this.FLAGS) $('#lo_'+k).checked = !!v[k];
    el.addEventListener('change', ()=>this.save());
    el.addEventListener('input', ()=>this.save());
    for(const b of el.querySelectorAll('[data-pick]')) b.onclick=()=>this.browse(b.dataset.pick, b.dataset.kind);
    $('#loSort').onclick=()=>this.sort();
    $('#loExport').onclick=()=>this.exportPlan();
    $('#loText').onchange=e=>{ this.showText=e.target.checked; this.drawWarnings(); };
    $('#lo_rules').addEventListener('change', ()=>this.ages());
    $('#loUpRules').onclick=()=>this.download('loadorder_update_rules',{rules:this.lines('rules')});
    $('#loUpYml').onclick=()=>{ const path=this.val('pluginOrderYml'); if(!path) return toast('Say where plugin-order.yml goes first','warn',3000); this.download('loadorder_update_yml',{path}); };
    $('#loUpData').onclick=()=>{
      const path=this.val('dataPathsCache'), listName=this.val('listName');
      if(!path || !listName) return toast('The data-paths cache and the list name are needed','warn',3000);
      this.download('loadorder_fetch_data_paths',{path, listName});
    };
    $('#loRuleOpen').onclick=()=>WgRuleMaker.open();
  },

  stamp(){
    const d=new Date(), p=n=>String(n).padStart(2,'0');
    return `${d.getFullYear()}${p(d.getMonth()+1)}${p(d.getDate())}-${p(d.getHours())}${p(d.getMinutes())}${p(d.getSeconds())}`;
  },

  /** A download (Update rules, Sources), its report added to the full text. */
  async download(cmd, args){
    toast('Downloading...','ok',2000);
    try{
      const lines=await Engine.call(cmd, Object.assign({stamp:this.stamp()}, args));
      this.report=this.report.concat([{kind:'subsection', text:cmd.replace('loadorder_','').replace(/_/g,' ')}], lines.map(t=>({kind:'line', text:t})));
      this.showText=true; $('#loText').checked=true; this.drawWarnings();
      this.ages();
      toast(lines[lines.length-1]||'Done','ok',4000);
    }catch(e){ toast(String(e.message||e),'err',6000); }
  },


  async browse(k, kind){
    const p = kind==='save'
      ? await Engine.pick('save',{defaultPath:this.val(k)||'customizations.toml'})
      : await Engine.pick('open',{directory:kind==='dir', defaultPath:this.val(k)||undefined});
    if(p){ $('#lo_'+k).value=String(p); this.save(); }
  },

  async ages(){
    const rules=this.lines('rules'), box=$('#loAges');
    if(!rules.length){ box.textContent='-'; return; }
    try{
      const got=await Engine.call('loadorder_rule_ages',{rules});
      box.innerHTML=got.map(([n,a])=>`<div>${loEsc(n)}: ${a==null? 'missing' : a+' day'+(a===1?'':'s')+' old'}</div>`).join('');
    }catch(e){ box.textContent=String(e.message||e); }
  },

  args(){
    const a={cfg:this.val('cfg'), customizations:this.val('customizations'), subsetFile:this.val('subsetFile'),
      pluginOrderYml:this.val('pluginOrderYml'), listName:this.val('listName'), dataPathsCache:this.val('dataPathsCache'),
      rules:this.lines('rules'), groundcover:this.lines('groundcover'), subset:[], emitToml:!!this.val('emitToml'),
      allowEmptySort:true};
    for(const [k] of this.FLAGS) a[k]=this.flag(k);
    return a;
  },

  async sort(){
    const btn=$('#loSort'); btn.disabled=true;
    $('#loContent').innerHTML='<li class="empty">Sorting...</li>';
    try{
      const a=this.args();
      const dir=this.val('scanDir');
      if(dir){
        const scan=await Engine.call('loadorder_scan',{dir});
        a.subsetLines=scan.lines;
        toast('Scanned '+scan.start+': '+scan.folders+' mod folder(s), '+scan.plugins+' plugin(s)','ok',3000);
      }
      const got=await Engine.call('loadorder_compute',{args:JSON.stringify(a)});
      this.report=got.report||[];
      if(got.refused){
        this.plan=null; toast(got.refused,'err',6000);
        $('#loContent').innerHTML='<li class="empty">'+loEsc(got.refused)+'</li>';
      }
      else this.load(got.plan);
      this.drawWarnings();
    }catch(e){
      toast(String(e.message||e),'err',6000);
      $('#loContent').innerHTML='<li class="empty">'+loEsc(String(e.message||e))+'</li>';
    }
    finally{ btn.disabled=false; }
  },

  load(plan){
    this.plan=plan;
    const order=plan.finalOrder || plan.baseOrder;
    const mine=new Set((plan.subset||[]).map(s=>s.toLowerCase()));
    const bad=new Set((plan.masterProblems||[]).map(s=>s.toLowerCase()));
    this.content=order.map(n=>({name:n, on:true, mine:mine.has(n.toLowerCase()), bad:bad.has(n.toLowerCase())}));
    const lines=plan.dataResult ? plan.dataResult.map(([l,isNew])=>({line:l, on:true, isNew})) : plan.dataOrder.map(l=>({line:l, on:true, isNew:false}));
    this.data=lines;
    this.drawOrders();
    $('#loExport').disabled=false;
  },

  drawOrders(){
    this.drawList($('#loContent'), this.content, r=>r.name, r=>(r.mine?' mine':'')+(r.bad?' bad':'')+(r.isNew?' new':''));
    this.drawList($('#loData'), this.data, r=>r.line, r=>r.isNew?' new':'');
    const on=this.content.filter(r=>r.on).length;
    $('#loCount').textContent=on+' of '+this.content.length;
    // the lists are made again on every tick and move: the Find lights stay (no scrolling)
    this.markFound('content'); this.markFound('data');
  },

  /** Lights the rows of an order (`content` or `data`) whose text has what its Find box says,
   *  and the count ("3/12"). `step`: none keeps the current match and does not scroll; 0 goes
   *  to the first; 1 / -1 to the next / the one before (round the ends), scrolled into view. */
  markFound(k, step){
    const ol=$(k==='content'? '#loContent' : '#loData'), q=this.q[k], out=$('#loQn'+k);
    if(!ol) return;
    const lis=[...ol.querySelectorAll('li[data-key]')], hits=[];
    for(const li of lis){
      const m=!!q && li.dataset.key.includes(q);
      li.classList.toggle('found', m); li.classList.remove('cur');
      if(m) hits.push(li);
    }
    if(!hits.length){ this.cur[k]=-1; if(out) out.textContent= q? '0' : ''; return; }
    let c=this.cur[k];
    if(step===0) c=0;
    else if(step) c=c<0? (step>0? 0 : hits.length-1) : (c+step+hits.length)%hits.length;
    else c=Math.max(0, Math.min(c, hits.length-1));
    this.cur[k]=c;
    hits[c].classList.add('cur');
    if(step!=null) hits[c].scrollIntoView({block:'center'});
    if(out) out.textContent=(c+1)+'/'+hits.length;
  },

  /** A list whose rows drag to reorder and tick on and off. */
  /** A list whose rows are picked (click, Ctrl+click, Shift+click), dragged to a new place
   *  (the picked rows move together), moved with Alt+Up / Alt+Down, and ticked on and off.
   *  Pointer events, not HTML drag-and-drop, which the shell's webview hands to the window as
   *  file drops (the rows would not move). */
  drawList(ol, rows, text, cls){
    ol.innerHTML='';
    ol.tabIndex=0;
    rows.forEach((r,i)=>{
      const li=document.createElement('li');
      li.className=(r.on?'':'off')+cls(r)+(r.sel?' hit':'');
      li.dataset.i=i; li.dataset.key=text(r).toLowerCase();
      li.innerHTML=`<input type="checkbox" ${r.on?'checked':''}><span>${loEsc(text(r))}</span>`;
      li.firstChild.onchange=e=>{ r.on=e.target.checked; this.drawOrders(); };
      li.addEventListener('pointerdown', e=>{
        if(e.target.tagName==='INPUT' || e.button!==0) return;
        e.preventDefault(); ol.focus();
        if(e.shiftKey && this._anchor!=null && this._anchorRows===rows){
          const [a,b]=[Math.min(this._anchor,i), Math.max(this._anchor,i)];
          rows.forEach((x,k)=>{ x.sel=k>=a && k<=b; });
        }else if(e.ctrlKey || e.metaKey){ r.sel=!r.sel; this._anchor=i; this._anchorRows=rows; }
        else{
          if(!r.sel) rows.forEach(x=>{ x.sel=false; });
          r.sel=true; this._anchor=i; this._anchorRows=rows;
        }
        for(const [k,x] of rows.entries()) ol.children[k] && ol.children[k].classList.toggle('hit', !!x.sel);
        // a drag from here moves the picked rows to where it is let go
        const y0=e.clientY; let target=null, moved=false;
        li.setPointerCapture(e.pointerId);
        const move=ev=>{
          if(!moved && Math.abs(ev.clientY-y0)<4) return;
          moved=true;
          const over=document.elementFromPoint(ev.clientX, ev.clientY);
          const t=over && over.closest && over.closest('li');
          if(target && target!==t) target.classList.remove('drop');
          target=t && t.parentElement===ol ? t : null;
          if(target) target.classList.add('drop');
          const box=ol.closest('.lo-scroll'); if(box){ const rr=box.getBoundingClientRect(); if(ev.clientY<rr.top+20) box.scrollTop-=12; else if(ev.clientY>rr.bottom-20) box.scrollTop+=12; }
        };
        const up=()=>{
          li.removeEventListener('pointermove', move); li.removeEventListener('pointerup', up);
          if(target) target.classList.remove('drop');
          if(moved && target) this.moveTo(rows, +target.dataset.i);
        };
        li.addEventListener('pointermove', move); li.addEventListener('pointerup', up);
      });
      ol.appendChild(li);
    });
    ol.onkeydown=e=>{
      if(!e.altKey || (e.key!=='ArrowUp' && e.key!=='ArrowDown')) return;
      e.preventDefault();
      const first=rows.findIndex(x=>x.sel); if(first<0) return;
      const last=rows.length-1-[...rows].reverse().findIndex(x=>x.sel);
      if(e.key==='ArrowUp' && first>0) this.moveTo(rows, first-1);
      else if(e.key==='ArrowDown' && last<rows.length-1) this.moveTo(rows, last+2);
    };
  },

  /** The picked rows of `rows` moved together to stand before row `at` (its index now). */
  moveTo(rows, at){
    const picked=rows.filter(x=>x.sel); if(!picked.length) return;
    const before=rows.slice(0,at).filter(x=>!x.sel).length;
    const rest=rows.filter(x=>!x.sel);
    rest.splice(before, 0, ...picked);
    rows.length=0; rows.push(...rest);
    this.drawOrders();
  },

  /* ---- files and folders dropped on the lists ------------------------------------------------
     The shell takes a file drag from the OS (the webview never sends dragover or drop to the
     page), and tells the page instead: tauri://drag-enter / -over / -leave / -drop, with the
     paths and the pointer's place (physical pixels). A dropped plugin or a folder of plugins
     goes into content= where it is let go, and its folder into data= unless that is there; a
     dropped folder goes into data= as a line. `loadorder_drop_probe` says what the paths are.
     It changes the lists the page shows - Export writes them; a new Sort starts again from the
     inputs. */

  dropArm(){
    if(this._dropArmed) return;
    const E=window.__TAURI__ && window.__TAURI__.event;
    if(!E || typeof E.listen!=='function') return;   // a browser: no paths to be had
    this._dropArmed=true;
    const at=pl=>{ const d=window.devicePixelRatio||1, p=(pl&&pl.position)||{x:0,y:0}; return {x:p.x/d, y:p.y/d}; };
    E.listen('tauri://drag-enter', e=>this.dropOver(at(e.payload)));
    E.listen('tauri://drag-over', e=>this.dropOver(at(e.payload)));
    E.listen('tauri://drag-leave', ()=>this.dropClear());
    E.listen('tauri://drag-drop', e=>{ const pl=e.payload||{}; const where=this.dropWhere(at(pl)); this.dropClear(); if(where) this.dropDo(pl.paths||[], where); });
  },

  /** The list a point is over: `{kind, ol, index, after}` (`index`: the row it is on; `after`:
   *  the lower half of it) or null. */
  dropWhere(pt){
    if(!this.on) return null;
    const hit=document.elementFromPoint(pt.x, pt.y); if(!hit || !hit.closest) return null;
    const col=hit.closest('#wgLO .orders > .lo-col'); if(!col) return null;
    const ol=col.querySelector('ol,ul'); if(!ol) return null;
    const kind=ol.id==='loContent'? 'content' : ol.id==='loData'? 'data' : null; if(!kind) return null;
    const li=hit.closest('li[data-i]');
    if(!li || li.parentElement!==ol) return {kind, ol, col, index:-1, after:false};
    const r=li.getBoundingClientRect();
    return {kind, ol, col, index:+li.dataset.i, after:pt.y>r.top+r.height/2};
  },

  dropClear(){
    for(const x of document.querySelectorAll('#wgLO .droptarget')) x.classList.remove('droptarget');
    for(const x of document.querySelectorAll('#wgLO li.drop, #wgLO li.dropb')) x.classList.remove('drop','dropb');
  },

  dropOver(pt){
    this.dropClear();
    const w=this.dropWhere(pt); if(!w) return;
    w.col.classList.add('droptarget');
    const li=w.index>=0? w.ol.children[w.index] : null;
    if(li) li.classList.add(w.after? 'dropb' : 'drop');
  },

  /** Whether the cfg quotes its data= paths (the lines the page has decide; ties: no). */
  dataQuoted(){
    const q=this.data.filter(r=>/^\s*data\s*=\s*"/i.test(r.line)).length;
    return q>this.data.length-q;
  },

  async dropDo(paths, w){
    if(!paths.length) return;
    if(!this.plan) return toast('Sort first, then drop what you want to add','warn',4000);
    let got;
    try{
      got=await Engine.call('loadorder_drop_probe',{paths, existing:this.data.map(r=>r.line), quoted:this.dataQuoted()});
      if(typeof got==='string') got=JSON.parse(got);
    }catch(e){ return toast(String(e.message||e),'err',6000); }
    const items=got.items||[];
    if(!items.length) return toast('Nothing to add: those are not files or folders','warn',4000);
    const live=!!this.plan.dataResult;   // the data= list is written only when its paths are sorted
    const rows=w.kind==='content'? this.content : this.data;
    let at=w.index<0? rows.length : w.index+(w.after?1:0);
    const lower=s=>String(s).toLowerCase();
    const haveData=new Set(this.data.map(r=>lower(r.line))), haveContent=new Set(this.content.map(r=>lower(r.name)));
    const note={plugins:0, folders:0, dups:0, none:0, unlisted:0}, fresh=[];
    for(const r of this.content) r.sel=false;
    for(const r of this.data) r.sel=false;
    const addData=(e, where)=>{
      if(e.dup || haveData.has(lower(e.line))){ note.dups++; return false; }
      haveData.add(lower(e.line));
      const row={line:e.line, on:true, isNew:true, sel:true};
      if(where==null) this.data.push(row); else this.data.splice(where,0,row);
      note.folders++; fresh.push(row); return true;
    };
    if(w.kind==='data'){
      if(!live) return toast('The data= list is only written when "Sort data paths" is ticked: tick it and Sort first','warn',6000);
      for(const it of items){ if(addData(it.self, at)) at++; }
    }else{
      const added=[];
      for(const it of items){
        if(!it.groups.length) note.none++;
        for(const g of it.groups){
          const mine=[];
          for(const n of g.plugins){
            if(haveContent.has(lower(n))){ note.dups++; continue; }
            haveContent.add(lower(n)); mine.push(n);
          }
          for(const n of mine){
            const row={name:n, on:true, mine:false, bad:false, isNew:true, sel:true};
            this.content.splice(at,0,row); at++; note.plugins++; fresh.push(row); added.push(row);
          }
          // The plugin has to be found: its folder in data=.
          if(mine.length && !g.dup && !haveData.has(lower(g.line))){
            if(live) addData(g, null); else note.unlisted++;
          }
        }
      }
    }
    this.drawOrders();
    const first=fresh[0], ol=$(w.kind==='content'? '#loContent' : '#loData');
    if(first && ol){ const i=(w.kind==='content'? this.content : this.data).indexOf(first); const li=ol.children[i]; if(li) li.scrollIntoView({block:'nearest'}); }
    const bits=[];
    if(note.plugins) bits.push(note.plugins+' plugin'+(note.plugins===1?'':'s')+' into content=');
    if(note.folders) bits.push(note.folders+' folder'+(note.folders===1?'':'s')+' into data=');
    if(note.dups) bits.push(note.dups+' already there');
    if(note.none) bits.push(note.none+' with no plugins');
    if(note.unlisted) bits.push('not in data= (tick "Sort data paths" to add them, or add the folder yourself)');
    if(!fresh.length && !bits.length) bits.push('nothing to add');
    toast((fresh.length? 'Added ' : 'Nothing added: ')+bits.join(', ')+(fresh.length? '. Export writes it; a new Sort starts again from the inputs.' : ''), fresh.length && !note.unlisted? 'ok':'warn', 7000);
  },

  /** The picked rows of the content= order, in order (the rule maker takes them). */
  picked(){ return this.content.filter(r=>r.sel).map(r=>r.name); },

  drawWarnings(){
    const items=this.report;
    const warns=items.filter(i=>i.warning);
    const counts={};
    for(const w of warns) counts[w.warning]=(counts[w.warning]||0)+1;
    const chips=$('#loChips');
    chips.innerHTML=['all',...Object.keys(counts)].map(k=>
      `<span class="chip${this.filter===k?' on':''}" data-k="${k}">${k==='all'?'All ('+warns.length+')':loEsc(this.KINDS[k]||k)+' ('+counts[k]+')'}</span>`).join('');
    for(const c of chips.querySelectorAll('.chip')) c.onclick=()=>{ this.filter=c.dataset.k; this.drawWarnings(); };
    const box=$('#loWarn');
    if(this.showText){
      const rule='='.repeat(70);
      box.innerHTML='<pre>'+loEsc(items.filter(i=>i.kind!=='log' && i.kind!=='scans' && i.kind!=='preview').map(i=>
        i.kind==='section' ? '\n'+rule+'\n '+i.text+'\n'+rule : i.kind==='subsection' ? '\n--- '+i.text+' ---' : i.text).join('\n'))+'</pre>';
      return;
    }
    const shown=warns.filter(w=>this.filter==='all' || w.warning===this.filter);
    if(!shown.length){ box.innerHTML='<div class="empty">'+(items.length? 'No warnings.' : '-')+'</div>'; return; }
    box.innerHTML='';
    for(const w of shown){
      const d=document.createElement('div');
      d.className='warn '+w.warning;
      d.innerHTML=`<span class="kind">${loEsc(this.KINDS[w.warning]||w.warning)}</span>${loEsc(w.text.replace(/^\n+/,''))}`;
      if(w.names && w.names.length){
        const links=document.createElement('div');
        for(const n of w.names){
          const a=document.createElement('a'); a.textContent=n; a.onclick=()=>this.find(n);
          links.appendChild(a);
        }
        d.appendChild(links);
      }
      box.appendChild(d);
    }
  },

  /** Finds a plugin (or a data folder) in the orders and lights it. */
  find(name){
    const key=name.toLowerCase();
    for(const li of document.querySelectorAll('#wgLO li.hit')) li.classList.remove('hit');
    const li=[...document.querySelectorAll('#wgLO li')].find(l=>l.dataset.key===key || (l.dataset.key||'').includes(key));
    if(li){ li.classList.add('hit'); li.scrollIntoView({block:'center'}); }
    else toast(name+' is not in the orders','warn',2500);
  },

  async exportPlan(){
    if(!this.plan) return;
    const btn=$('#loExport'); btn.disabled=true;
    try{
      const stamp=this.stamp();
      const extra=(window.__WG_VIEW__||{}).extra||{};
      // The durable fix is the customizations TOML (the configurator rebuilds openmw.cfg
      // from it): with no other target named, Export writes it back into customizations.toml
      // itself (the Tk window's "write directly back"), a backup kept unless No backup.
      const toml=this.val('emitToml') || this.val('customizations');
      const a={cfg:this.val('cfg'), writeCfg:this.flag('writeCfg'), emitToml:toml, dryRun:this.flag('dryRun'),
        noBackup:this.flag('noBackup'), sortDataPaths:this.flag('sortDataPaths'), listName:this.val('listName'), stamp,
        finalOrder:this.content.filter(r=>r.on).map(r=>r.name),
        dataOrder:this.plan.dataResult? this.data.filter(r=>r.on).map(r=>r.line) : null,
        disabledPlugins:this.content.filter(r=>!r.on).map(r=>r.name),
        disabledData:this.data.filter(r=>!r.on).map(r=>r.line),
        planFile:extra.loadorderPlan||''};
      const got=await Engine.call('loadorder_export',{args:JSON.stringify(a)});
      this.report=this.report.concat(got.report||[]);
      this.drawWarnings();
      const what=[got.wroteCfg&&'openmw.cfg', got.wroteToml&&'customizations.toml'].filter(Boolean);
      // Say why nothing was written: a preview is what Export does without a target ticked.
      const why=a.dryRun? 'Dry run is ticked' : !a.emitToml? 'name the customizations.toml (or "Write customizations to") to write the order into' : 'see Warnings';
      toast(what.length? 'Wrote '+what.join(' and ') : 'Nothing written (a preview): '+why, what.length?'ok':'warn', what.length? 4000 : 7000);
    }catch(e){ toast(String(e.message||e),'err',6000); }
    finally{ btn.disabled=false; }
  },
};

/** Text for innerHTML. */
function loEsc(s){ return String(s).replace(/[&<>"']/g, c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c])); }

/** Wraithguard's settings file read once for the pages that open on it (`settings_seed`). */
const WgSettings={
  _p:null,
  _w:null, _t:0,
  file(){ return String(((window.__WG_VIEW__||{}).extra||{}).settingsFile||''); },
  seed(){
    if(!this._p) this._p=Engine.call('settings_seed',{settingsFile:this.file()}).catch(()=>({}));
    return this._p;
  },
  /** Read again at the next seed (after the Setup page saved). */
  reset(){ this._p=null; },
  /** Page fields written back to the settings file (`settings_write`), a moment after the last
   *  change, so typing does not write a file a key. */
  write(fields){
    this._w=Object.assign(this._w||{}, fields);
    clearTimeout(this._t);
    this._t=setTimeout(()=>{
      const f=this._w; this._w=null;
      Engine.call('settings_write',{settingsFile:this.file(), fields:JSON.stringify(f)}).then(()=>this.reset()).catch(e=>toast(String(e.message||e),'err',5000));
    }, 600);
  },
};
