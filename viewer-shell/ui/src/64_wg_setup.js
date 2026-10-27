/* =====================================================================================
   The Setup page - Wraithguard (port step 4.5). Every path the program runs on, in one
   place: the openmw.cfg, the customizations TOML, the rule files, MOMW's plugin-order.yml
   and the list, tes3cmd, the folders. What Wraithguard's main window held, now the
   program's own, kept in the settings file every page reads (`toolscore::settings`).

   - Each field is checked as the program will read it (`setup_check`, toolscore::setup):
     a cfg's data folders that are gone, a TOML that does not parse, a rule file that is not
     there are named, with what to do, before anything is loaded with them.
   - It opens by itself on a first start (no openmw.cfg yet) and after a start that never
     finished loading its install (`setup_mark`), and loads nothing until told to: a
     malformed install is put right here, not behind a load that hangs on it.
   - The rest of the time it is the top bar's Setup button, and a button on the loading card:
     it lies over everything, the loading card included. "Restart" starts the program again
     when the load in hand will not finish.
   - `--setup` opens on it and loads nothing.

   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */

const WgSetup={
  on:false,
  checks:{},
  _t:0,

  /** The sections and their fields: [name, label, kind, help]. Kinds: file, dir, save (a file
      written), lines (one a line), plain, cfg. Names are the settings file's page names. */
  SECTIONS:[
    ['Your install', [
      ['cfg','openmw.cfg','cfg','The cfg OpenMW starts with: its data= folders and content= files are the load order every page reads.'],
      ['openmwInstall','OpenMW folder','dir','Where OpenMW is installed: the Lua checks read its Lua API from it. Optional.'],
    ]],
    ['Sorting (the Load order page)', [
      ['rules','Rule files','lines','mlox rule files, one a line, lowest priority first: mlox_base.txt, then mlox_user.txt, then your own. A folder means its .txt files.'],
      ['userRules','Your rule file','save','Where the rule maker adds your own rules. Keep it last among the rule files.'],
      ['customizations','customizations.toml','file','A momw-customizations.toml: the mods you add to a curated list. Optional.'],
      ['subsetFile','Subset file','file','A plain list of your plugins and their folders, one a line, instead of a TOML. Optional.'],
      ['scanDir','Mods folder','dir','A folder of mods scanned into the sort. Optional.'],
      ['pluginOrderYml','plugin-order.yml','file','MOMW\'s file of curated plugins: they are kept out of the sort. Optional.'],
      ['listName','List name','plain','The curated list you follow (total-overhaul, expanded-vanilla ...).'],
      ['dataPathsCache','Data-paths cache','file','The list\'s data= order, fetched by the Load order page. Optional.'],
      ['emitToml','Write customizations to','save','Where Export writes the corrected customizations TOML. Optional.'],
      ['groundcover','Grass plugins','lines','Plugins written as groundcover= rather than content=, one a line.'],
    ]],
    ['Tools', [
      ['tes3cmd','tes3cmd','file','The tes3cmd program, for the Tools page\'s clean and header. Optional.'],
      ['mergeLandsFolder','Merge Lands writes to','dir','The folder Merged Lands.esp goes in. Empty: the one holding most of the load order.'],
      ['exclude','Leave out of scans','plain','Plugins the conflict and cell scans leave out: patterns, commas between.'],
    ]],
  ],

  view(){ return window.__WG_VIEW__||{}; },
  file(){ return String((this.view().extra||{}).settingsFile||''); },

  /** Start-up, before anything is loaded: the buttons, and whether to load at all.
      Answers false when the program is to stay on this page (nothing is loaded). */
  async boot(){
    this.button();
    // The pages that need no install open from here too, loaded or not.
    if(typeof WgLoadOrder==='object') WgLoadOrder.button();
    if(typeof WgConflicts==='object') WgConflicts.button();
    if(typeof WgToolsPage==='object') WgToolsPage.button();
    if(typeof WgHelp==='object') WgHelp.button();
    this.busyButton();
    const want=this.view();
    let st=null;
    try{ st=await Engine.call('setup_state',{settingsFile:this.file()}); }catch(_){ }
    this.state=st;
    if(want.setup){ this.enter('Opened with --setup: nothing is loaded until you say so.'); return false; }
    if(!want.app) return true; // a window the Tk app opened: it named its cfg
    if(!want.cfg){
      this.enter(st && st.fields && st.fields.cfg
        ? 'The openmw.cfg in the settings is not there any more. Choose it again.'
        : 'Welcome. Choose your openmw.cfg (Find looks where OpenMW keeps it), and the rule files if you sort. Then Save and load.');
      return false;
    }
    const last=st && st.lastStart;
    if(last && last.state==='connecting'){
      this.enter('The last start did not finish loading '+(last.cfg||'the install')+'. Check what is marked below, then Save and load (or load it as it is).');
      return false;
    }
    return true;
  },

  button(){
    if($('#btnSetup')) return;
    const after=$('#btnReport'); if(!after) return;
    const b=document.createElement('button');
    b.className='btn dim'; b.id='btnSetup'; b.textContent='Setup';
    b.title='The paths the program runs on: openmw.cfg, customizations, rule files, plugin-order.yml, tes3cmd. Checked, and saved for every page.';
    b.onclick=()=> this.on? this.leave() : this.enter();
    after.insertAdjacentElement('beforebegin', b);
  },

  /** A way to Setup from the loading card, for a load that does not finish. */
  busyButton(){
    const card=document.querySelector('#busy .card');
    if(!card || $('#busySetup')) return;
    const d=document.createElement('div');
    d.id='busySetup'; d.style.cssText='margin-top:8px;text-align:right';
    d.innerHTML='<button class="btn dim sm" title="Stuck? The paths it is loading, checked - and a restart">Setup…</button>';
    d.querySelector('button').onclick=()=>this.enter('The program is loading. If it does not finish, check what is marked below, save, and Restart.');
    card.appendChild(d);
  },

  enter(note){
    this.build();
    this.on=true;
    $('#wgSetup').hidden=false;
    $('#suNote').textContent=note||'';
    $('#suNote').hidden=!note;
    const b=$('#btnSetup'); if(b) b.classList.add('on');
    this.fill();
  },

  leave(){
    if(!this.on) return;
    this.on=false;
    $('#wgSetup').hidden=true;
    const b=$('#btnSetup'); if(b) b.classList.remove('on');
  },

  fields(){
    const f={};
    for(const [,rows] of this.SECTIONS) for(const [k] of rows) f[k]=$('#su_'+k).value;
    return f;
  },

  async fill(){
    let st=null;
    try{ st=await Engine.call('setup_state',{settingsFile:this.file()}); }catch(e){ toast(String(e.message||e),'err',5000); }
    this.state=st;
    const f=(st&&st.fields)||{};
    for(const [,rows] of this.SECTIONS) for(const [k] of rows) $('#su_'+k).value=f[k]!=null? String(f[k]) : '';
    // A Tk window's own cfg wins, as its field says the pages open on it.
    const x=this.view().extra||{};
    if(x.loadorderCfg && !$('#su_cfg').value) $('#su_cfg').value=x.loadorderCfg;
    $('#suFile').textContent=st? (st.exists? 'Saved in ' : 'Will be saved in ')+st.settingsFile : '';
    this.dirty(false);
    this.check();
  },

  dirty(v){ this._dirty=v; $('#suSave').textContent=v? 'Save *' : 'Save'; },

  async check(){
    let c;
    try{ c=await Engine.call('setup_check',{fields:JSON.stringify(this.fields())}); }catch(e){ return; }
    this.checks=c;
    for(const [,rows] of this.SECTIONS) for(const [k] of rows){
      const r=c[k], box=$('#suc_'+k);
      if(!r || r.level==='empty'){ box.innerHTML=''; box.className='su-c'; continue; }
      box.className='su-c '+r.level;
      box.innerHTML=r.lines.map(l=>'<div>'+escHtml(l)+'</div>').join('');
    }
    const bad=Object.values(c).filter(r=>r.level==='error').length;
    $('#suSum').textContent=bad? bad+' field'+(bad===1?'':'s')+' cannot be used as '+(bad===1?'it is':'they are') : 'Nothing in the way';
    $('#suSum').className=bad? 'su-sum error' : 'su-sum ok';
  },

  async save(){
    try{
      const got=await Engine.call('settings_write',{settingsFile:this.file(), fields:JSON.stringify(this.fields())});
      if(typeof WgSettings==='object') WgSettings.reset();
      // The Load order page, if it is open, takes the paths too.
      if(typeof WgLoadOrder==='object' && $('#wgLO')) WgLoadOrder.take(got);
      this.dirty(false);
      toast('Saved','ok',2000);
      return true;
    }catch(e){ toast(String(e.message||e),'err',6000); return false; }
  },

  /** Save, then load the cfg: now when nothing is loading, else after a restart. */
  async saveAndLoad(){
    if(!await this.save()) return;
    const cfg=$('#su_cfg').value.trim();
    if(!cfg || (this.checks.cfg && this.checks.cfg.level==='error')) return toast('The openmw.cfg cannot be used as it is: see what is marked','warn',5000);
    if(App.swapping){ toast('Something is loading now: Restart to load this instead','warn',6000); return; }
    window.__WG_VIEW__=Object.assign(this.view(), {cfg});
    this.leave();
    try{ await openWraithguardSetup(); }catch(e){ toast(String(e.message||e),'err',6000); }
  },

  async find(){
    let got=[];
    try{ got=await Engine.call('setup_find_cfgs',{}); }catch(_){ }
    if(!got.length) return toast('No openmw.cfg where OpenMW keeps it: choose it with ...','warn',5000);
    if(got.length===1 || !$('#su_cfg').value){ $('#su_cfg').value=got[0]; }
    if(got.length>1) toast('Found '+got.length+': '+got.join('  |  '),'ok',9000);
    this.dirty(true); this.check();
  },

  async pick(k, kind){
    const cur=$('#su_'+k).value.split('\n').map(s=>s.trim()).filter(Boolean);
    let p;
    if(kind==='save') p=await Engine.pick('save',{defaultPath:cur[0]||undefined});
    else p=await Engine.pick('open',{directory:kind==='dir', multiple:kind==='lines', defaultPath:cur[cur.length-1]||undefined});
    if(!p) return;
    if(kind==='lines') $('#su_'+k).value=[...cur, ...(Array.isArray(p)?p:[p]).map(String)].join('\n');
    else $('#su_'+k).value=String(Array.isArray(p)?p[0]:p);
    this.dirty(true); this.check();
  },

  build(){
    if($('#wgSetup')) return;
    const st=document.createElement('style');
    st.textContent=`
#wgSetup{position:fixed;left:0;right:0;top:var(--top-h,40px);bottom:0;z-index:320;background:var(--bg);color:var(--tx);
  display:flex;flex-direction:column;font-size:12px}
#wgSetup[hidden]{display:none}
#wgSetup .su-scroll{flex:1;overflow:auto;padding:12px 16px}
#wgSetup .su-in{max-width:980px;margin:0 auto;display:flex;flex-direction:column;gap:14px}
#wgSetup h2{margin:0;font-size:15px}
#wgSetup h3{margin:0 0 4px;font-size:12px;color:var(--tx2);text-transform:uppercase;letter-spacing:.06em}
#wgSetup .su-sec{background:var(--bg2);border:1px solid var(--line);border-radius:var(--r-box);padding:8px 10px;display:flex;flex-direction:column;gap:8px}
#wgSetup .su-row{display:grid;grid-template-columns:170px 1fr;gap:4px 10px;align-items:start}
#wgSetup .su-row label{padding-top:4px;color:var(--tx)}
#wgSetup .su-row .help{grid-column:2;color:var(--tx3);font-size:11px}
#wgSetup .su-f{display:flex;gap:4px;min-width:0}
#wgSetup input[type=text],#wgSetup textarea{flex:1;min-width:0;background:var(--bg3);color:var(--tx);border:1px solid var(--line2);border-radius:var(--r-fld);padding:3px 6px;font:inherit;font-family:var(--mono)}
#wgSetup textarea{height:54px;resize:vertical}
#wgSetup .su-c{grid-column:2;font-size:11.5px;border-left:3px solid transparent;padding-left:6px}
#wgSetup .su-c:empty{display:none}
#wgSetup .su-c.ok{border-color:var(--ac);color:var(--tx2)}
#wgSetup .su-c.warn{border-color:var(--warn)}
#wgSetup .su-c.error{border-color:var(--err);color:var(--tx)}
#wgSetup #suNote{background:var(--acd);border:1px solid var(--ac);border-radius:var(--r-box);padding:8px 10px}
#wgSetup .su-bar{display:flex;gap:8px;align-items:center;padding:8px 16px;border-top:1px solid var(--line);background:var(--bg2);flex-wrap:wrap}
#wgSetup .su-sum.error{color:var(--err)} #wgSetup .su-sum.ok{color:var(--tx2)}
#wgSetup #suFile{color:var(--tx3);font-family:var(--mono);font-size:11px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;min-width:0;flex:1}
@media (max-width:700px){#wgSetup .su-row{grid-template-columns:1fr} #wgSetup .su-row .help,#wgSetup .su-c{grid-column:1}}`;
    document.head.appendChild(st);
    const el=document.createElement('div');
    el.id='wgSetup'; el.hidden=true;
    const row=([k,label,kind,help])=>{
      const input=kind==='lines'? `<textarea id="su_${k}" spellcheck="false"></textarea>` : `<input type="text" id="su_${k}" spellcheck="false">`;
      const pick=kind==='plain'? '' : kind==='cfg'
        ? `<button class="btn dim" id="suFind" title="Look where OpenMW keeps its openmw.cfg">Find</button><button class="btn dim" data-pick="${k}" data-kind="file">…</button>`
        : kind==='lines' && k==='groundcover' ? '' : `<button class="btn dim" data-pick="${k}" data-kind="${kind==='lines'?'lines':kind}">…</button>`;
      return `<div class="su-row"><label for="su_${k}">${label}</label><div class="su-f">${input}${pick}</div><div class="help">${help}</div><div class="su-c" id="suc_${k}"></div></div>`;
    };
    el.innerHTML=`<div class="su-scroll"><div class="su-in">
  <h2>Setup</h2>
  <div id="suNote" hidden></div>
  ${this.SECTIONS.map(([t,rows])=>`<div class="su-sec"><h3>${t}</h3>${rows.map(row).join('')}</div>`).join('')}
</div></div>
<div class="su-bar"><button class="btn" id="suLoad" title="Save, then load the openmw.cfg">Save and load</button><button class="btn dim" id="suSave">Save</button>
  <button class="btn dim" id="suRestart" title="Start the program again: for a load that does not finish">Restart</button>
  <span class="su-sum" id="suSum"></span><span id="suFile"></span><button class="btn dim" id="suClose">Close</button></div>`;
    document.body.appendChild(el);
    el.addEventListener('input', ()=>{ this.dirty(true); clearTimeout(this._t); this._t=setTimeout(()=>this.check(), 350); });
    for(const b of el.querySelectorAll('[data-pick]')) b.onclick=()=>this.pick(b.dataset.pick, b.dataset.kind);
    $('#suFind').onclick=()=>this.find();
    $('#suSave').onclick=()=>this.save();
    $('#suLoad').onclick=()=>this.saveAndLoad();
    $('#suRestart').onclick=async()=>{
      if(this._dirty && !await this.save()) return;
      // the start it ends is not one that failed: the next one loads what was saved
      try{ await Engine.call('setup_mark',{state:'ok', cfg:''}); }catch(_){ }
      try{ await Engine.call('app_restart',{}); }catch(e){ toast(String(e.message||e),'err',5000); }
    };
    $('#suClose').onclick=()=>this.leave();
    // Esc closes it, as the other pages' dialogues close
    el.addEventListener('keydown', e=>{ if(e.key==='Escape'){ e.stopPropagation(); this.leave(); } });
  },
};
