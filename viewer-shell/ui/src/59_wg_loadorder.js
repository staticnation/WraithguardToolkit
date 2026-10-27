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
    ['emitToml','Write customizations to','save'],
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
    if(typeof WgConflicts==='object' && WgConflicts.on) WgConflicts.leave();
    if(typeof WgTools==='object' && WgTools.on) WgTools.leave();
    if(typeof WgHelp==='object' && WgHelp.on) WgHelp.leave();
    this.on=true;
    this.build();
    $('#wgLO').hidden=false;
    const i=document.querySelector('#brand i'); if(i){ this._brand=i.textContent; i.textContent='Load order'; }
    document.title='Wraithguard - Load order';
    const b=$('#btnLoadOrder'); if(b) b.classList.add('on');
    this.ages();
  },

  leave(){
    if(!this.on) return;
    this.on=false;
    $('#wgLO').hidden=true;
    const i=document.querySelector('#brand i'); if(i && this._brand) i.textContent=this._brand;
    const b=$('#btnLoadOrder'); if(b) b.classList.remove('on');
  },

  /* ---- the fields, remembered ---- */
  saved(){ try{ return JSON.parse(localStorage.getItem(this.KEY)||'{}')||{}; }catch(_){ return {}; } },
  save(){
    const v={};
    for(const [k] of this.FIELDS) v[k]=$('#lo_'+k).value;
    for(const [k] of this.FLAGS) v[k]=$('#lo_'+k).checked;
    try{ localStorage.setItem(this.KEY, JSON.stringify(v)); }catch(_){}
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
#wgLO li.drop{box-shadow:0 -2px 0 var(--ac) inset}
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
  <label class="f">Rule maker</label>
  <div class="row"><select id="loRuleKind">${['Requires','Conflict','Patch','Note','Order','NearStart','NearEnd'].map(k=>`<option>${k}</option>`).join('')}</select></div>
  <label class="f" for="loRulePlugins" id="loRulePluginsL">Plugins (one a line)</label>
  <textarea id="loRulePlugins" spellcheck="false"></textarea>
  <div class="hint" id="loRuleHelp" hidden>One expression a line, as mlox writes them: a plugin name, or
    <code>[DESC /regex/ a.esp]</code>, <code>[DESC !/regex/ a.esp]</code>, <code>[SIZE 1234 a.esp]</code>,
    <code>[SIZE !1234 a.esp]</code>, <code>[VER &lt; 1.2 a.esp]</code> (&lt; = &gt;), and groups
    <code>[ANY a.esp b.esp]</code>, <code>[ALL ...]</code>, <code>[NOT ...]</code>. For Requires and Patch the first is
    the subject; a Conflict needs two or more.</div>
  <label class="f" for="loRuleMsg">Message</label><input type="text" id="loRuleMsg">
  <label class="f" for="loRuleRef">Source (a link or where it was said; every rule cites one)</label><input type="text" id="loRuleRef">
  <pre id="loRuleOut" class="empty"></pre>
  <div class="row"><button class="btn dim" id="loRuleCheck">Check</button><button class="btn dim" id="loRuleAdd">Add to your rule file</button></div>
</div><div class="acts"><button class="btn" id="loSort">Sort</button><button class="btn" id="loExport" disabled>Export</button></div></div>
<div class="orders">
  <div class="lo-col"><h3>content= <span class="sp"></span><span id="loCount"></span></h3><div class="lo-scroll"><ol id="loContent"><li class="empty">Sort to see the order.</li></ol></div></div>
  <div class="lo-col"><h3>data=</h3><div class="lo-scroll"><ol id="loData"></ol></div></div>
</div>
<div class="lo-col"><h3>Warnings <span class="sp"></span><label><input type="checkbox" id="loText"> Full text</label></h3>
  <div class="chips" id="loChips"></div><div class="lo-scroll" id="loWarn"><div class="empty">-</div></div></div>`;
    document.body.appendChild(el);

    const v=this.saved();
    const view=window.__WG_VIEW__||{};
    for(const [k] of this.FIELDS) $('#lo_'+k).value = v[k]!=null ? v[k] : '';
    if(!this.val('cfg') && view.cfg) $('#lo_cfg').value=view.cfg;
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
    const kindShown=()=>{
      const k=$('#loRuleKind').value, ordering=['Order','NearStart','NearEnd'].includes(k);
      $('#loRuleHelp').hidden=ordering;
      $('#loRulePluginsL').textContent=ordering? 'Plugins (one a line, in order)' : 'Expressions (one a line)';
      $('#loRuleMsg').disabled=ordering;
    };
    $('#loRuleKind').onchange=kindShown; kindShown();
    $('#loRuleCheck').onclick=()=>this.rule('check');
    $('#loRuleAdd').onclick=()=>this.rule('append');
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

  /** The rule maker: the rule as written, its problems, or added to the person's rule file. */
  async rule(op){
    const kind=$('#loRuleKind').value, names=$('#loRulePlugins').value.split('\n').map(s=>s.trim()).filter(Boolean);
    const ordering=['Order','NearStart','NearEnd'].includes(kind);
    // An ordering rule is its plugins; a warning rule is its expressions, sent as the lines
    // typed and read by the sort's own reader ([DESC], [SIZE], [VER], [ANY]/[ALL]/[NOT]).
    const rule={kind, message: ordering? '' : $('#loRuleMsg').value.trim(), ref:$('#loRuleRef').value.trim(),
      plugins: ordering ? names : [], expressions: [], lines: ordering ? [] : names};
    const out=$('#loRuleOut');
    try{
      const got=await Engine.call('loadorder_author',{op, rule:JSON.stringify(rule), path:this.val('userRules')});
      if(op==='check'){
        out.textContent=got.text + (got.problems.length? '\n\n'+got.problems.map(([sev,m,r])=>sev+': '+m+(r?' ('+r+')':'')).join('\n') : '');
      }else{
        out.textContent=got.written; toast('Added to '+this.val('userRules'),'ok',3000);
      }
    }catch(e){ out.textContent=String(e.message||e); }
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
      if(got.refused){ this.plan=null; toast(got.refused,'err',6000); }
      else this.load(got.plan);
      this.drawWarnings();
    }catch(e){ toast(String(e.message||e),'err',6000); }
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
    this.drawList($('#loContent'), this.content, r=>r.name, r=>(r.mine?' mine':'')+(r.bad?' bad':''));
    this.drawList($('#loData'), this.data, r=>r.line, r=>r.isNew?' new':'');
    const on=this.content.filter(r=>r.on).length;
    $('#loCount').textContent=on+' of '+this.content.length;
  },

  /** A list whose rows drag to reorder and tick on and off. */
  drawList(ol, rows, text, cls){
    ol.innerHTML='';
    rows.forEach((r,i)=>{
      const li=document.createElement('li');
      li.className=(r.on?'':'off')+cls(r);
      li.draggable=true; li.dataset.i=i; li.dataset.key=text(r).toLowerCase();
      li.innerHTML=`<input type="checkbox" ${r.on?'checked':''}><span>${loEsc(text(r))}</span>`;
      li.firstChild.onchange=e=>{ r.on=e.target.checked; this.drawOrders(); };
      li.ondragstart=e=>{ e.dataTransfer.setData('text/plain', String(i)); this._drag={rows,i}; };
      li.ondragover=e=>{ if(this._drag && this._drag.rows===rows){ e.preventDefault(); li.classList.add('drop'); } };
      li.ondragleave=()=>li.classList.remove('drop');
      li.ondrop=e=>{
        e.preventDefault(); li.classList.remove('drop');
        const d=this._drag; this._drag=null; if(!d || d.rows!==rows) return;
        const [moved]=rows.splice(d.i,1); rows.splice(d.i<i? i-1 : i, 0, moved);
        this.drawOrders();
      };
      ol.appendChild(li);
    });
  },

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
      const a={cfg:this.val('cfg'), writeCfg:this.flag('writeCfg'), emitToml:this.val('emitToml'), dryRun:this.flag('dryRun'),
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
      toast(what.length? 'Wrote '+what.join(' and ') : 'Nothing written (a preview)', what.length?'ok':'warn', 4000);
    }catch(e){ toast(String(e.message||e),'err',6000); }
    finally{ btn.disabled=false; }
  },
};

/** Text for innerHTML. */
function loEsc(s){ return String(s).replace(/[&<>"']/g, c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c])); }
