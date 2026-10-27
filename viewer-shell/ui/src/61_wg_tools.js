/* =====================================================================================
   The Tools page - Wraithguard (port step 4.4). Wraithguard's tools that work on the whole
   load order, in the viewer, as a page beside Load order and Conflicts; each tool a section
   on the left, its work on the right. Each Tk window is redesigned as it moves here.

   - Lint (`tools_lint`, toolscore::lint): evil GMSTs, the fog bug, interiors without a path
     grid, blank headers and expansion-only script calls in your mods, missing .omwscripts
     twins; filtered by kind.
   - Backups (`tools_backups`, toolscore::backups): the copies Wraithguard and tes3cmd took
     before changing a file, in the data folders, the cfg's folder and the rule files'; each
     restored over its original or deleted.
   - Remove / rename master (`tools_masters`, `tools_remove_master`, `tools_rename_master`,
     toolscore::masters): a master the plugin lists but OpenMW cannot find, removed (its
     references renumbered, those into it dropped; a dry run first) or pointed at the file
     that replaced it; the original kept once as .premaster.bak.
   - Save Check (`tools_save_check`, toolscore::save): the save's content files the load order
     lacks (OpenMW refuses the save), and openmw-save-stats' report: the space each mod takes,
     what its Lua data is made of, how to shrink it; two saves compared.
   - Journal (`tools_journal`, editcore::journal): every quest's stages in index order, each
     with the calls that set it - a stage's own script, a standalone script, an NPC's response -
     their conditions as written and what else they do; a call into another quest is a link.
   - tes3cmd (`tools_tes3cmd`, toolscore::tes3cmd): the external tes3cmd, launched on each
     plugin staged with its masters (clean, custom) or in place (header); master sizes synced
     in-app. Its rewrite over the tes3 crate is planned in TES3CMD-REWRITE.md.
   - Merge Lands (`land_merge_home`, `land_merge`, landcore::service): Merged Lands.esp built
     from the cfg's load order and data folders, into the folder chosen (the one holding most
     of the order by default), the progress as Wraithguard's log had it. Each plugin measured
     against the masters (every mod's edits kept) or against the load order before it (the
     OpenMW fork: the winners followed, the seams fixed); the plugins left out by name or folder.
   - Merge settings (`land_meta_read`, `_preview`, `_write`, landcore::meta): a plugin's
     .mergedlands.toml - per land layer, whether Merge Lands takes the plugin's edits and how
     they resolve where they collide.

   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */

const WgToolsPage={
  on:false,
  tool:'lint',
  TOOLS:[['lint','Lint','Problems in the load order\'s plugins that the game does not report'],
    ['backups','Backups','The copies taken before a file was changed: restore or delete them'],
    ['masters','Remove / rename master','A plugin OpenMW will not load because a master is gone'],
    ['save','Save Check','Whether a save loads with this order, and the space each mod takes in it'],
    ['journal','Journal','Every quest\'s stages, and what sets each one'],
    ['tes3cmd','tes3cmd','Clean plugins, fix headers, sync master sizes'],
    ['mergelands','Merge Lands','Merged Lands.esp: the terrain of every mod that reshapes a cell, kept together'],
    ['mergesettings','Merge settings','A plugin\'s .mergedlands.toml: which of its land layers Merge Lands takes, and how']],
  t3:{files:[]},
  jr:{data:null, quest:null},
  ms:{path:'', list:[]},
  mg:{plugin:'', data:null},
  ml:{home:null, running:false},
  bk:{rows:[]},
  lint:{warnings:[], stats:null, filter:'all'},
  KINDS:{'EVLGMST':'Evil GMSTs','FOGBUG':'Fog bug','NO PATHGRID':'No path grid','HEADER':'Headers','EXP-DEP':'Expansion calls','TWIN':'Twins'},

  view(){ return window.__WG_VIEW__||{}; },
  extra(){ return this.view().extra||{}; },
  cfg(){ return this.view().cfg||''; },

  button(){
    if($('#btnTools')) return;
    const after=$('#btnConflicts')||$('#btnLoadOrder')||$('#btnReport'); if(!after) return;
    const b=document.createElement('button');
    b.className='btn dim'; b.id='btnTools'; b.textContent='Tools';
    b.title='The load order\'s tools: Lint, and the rest of Wraithguard\'s tools as they move here';
    b.onclick=()=> this.on? this.leave() : this.enter();
    after.insertAdjacentElement('afterend', b);
  },

  enter(){
    if(this.on) return;
    if(typeof WgLoadOrder==='object' && WgLoadOrder.on) WgLoadOrder.leave();
    if(typeof WgConflicts==='object' && WgConflicts.on) WgConflicts.leave();
    if(typeof WgHelp==='object' && WgHelp.on) WgHelp.leave();
    this.on=true;
    this.build();
    $('#wgTL').hidden=false;
    const i=document.querySelector('#brand i'); if(i){ this._brand=i.textContent; i.textContent='Tools'; }
    document.title='Wraithguard - Tools';
    const b=$('#btnTools'); if(b) b.classList.add('on');
    const t=this.extra().tool; if(t){ this.show(t); this.extra().tool=''; }
  },

  leave(){
    if(!this.on) return;
    this.on=false;
    $('#wgTL').hidden=true;
    const i=document.querySelector('#brand i'); if(i && this._brand) i.textContent=this._brand;
    const b=$('#btnTools'); if(b) b.classList.remove('on');
  },

  build(){
    if($('#wgTL')) return;
    const st=document.createElement('style');
    st.textContent=`
#wgTL{position:fixed;left:0;right:0;top:var(--top-h,40px);bottom:0;z-index:60;background:var(--bg);color:var(--tx);
  display:grid;grid-template-columns:200px 1fr;gap:8px;padding:8px;overflow:hidden;font-size:12px}
#wgTL[hidden]{display:none}
#wgTL input[type=text],#wgTL input[type=number],#wgTL input:not([type]),#wgTL textarea,#wgTL select{background:var(--bg3);color:var(--tx);border:1px solid var(--line2);border-radius:var(--r-fld);padding:3px 5px;font:inherit}
#wgTL input::placeholder,#wgTL textarea::placeholder{color:var(--tx3,var(--tx2));opacity:.8}
#wgTL select option{background:var(--bg3);color:var(--tx)}
#wgTL .col{display:flex;flex-direction:column;min-height:0;min-width:0;background:var(--bg2);border:1px solid var(--line);border-radius:var(--r-box)}
#wgTL h3{margin:0;padding:6px 8px;font-size:12px;border-bottom:1px solid var(--line);display:flex;gap:6px;align-items:center}
#wgTL h3 .sp{flex:1}
#wgTL .nav div{padding:6px 10px;cursor:pointer;border-left:3px solid transparent}
#wgTL .nav div:hover{background:var(--bg3)}
#wgTL .nav div.on{border-left-color:var(--ac);background:var(--acd)}
#wgTL .nav small{display:block;color:var(--tx3)}
#wgTL .scroll{overflow:auto;flex:1;min-height:0;padding:6px 8px}
#wgTL .chips{display:flex;flex-wrap:wrap;gap:4px;padding:6px 8px;border-bottom:1px solid var(--line)}
#wgTL .chip{background:var(--bg3);border:1px solid var(--line2);border-radius:10px;padding:1px 8px;cursor:pointer;color:var(--tx2)}
#wgTL .chip.on{background:var(--acd);border-color:var(--ac);color:var(--tx)}
#wgTL .warn{border-left:3px solid var(--warn);padding:4px 6px;margin:0 0 6px;background:var(--bg3);white-space:pre-wrap}
#wgTL .warn.FOGBUG,#wgTL .warn.EVLGMST{border-color:var(--err)}
#wgTL .warn .kind{color:var(--tx3);font-size:11px;display:block}
#wgTL .empty{color:var(--tx3);padding:8px}
#wgTL .pane[hidden]{display:none}`;
    document.head.appendChild(st);
    const el=document.createElement('div');
    el.id='wgTL'; el.hidden=true;
    el.innerHTML=`<div class="col"><h3>Tools</h3><div class="scroll nav" id="tlNav">${
      this.TOOLS.map(([k,l,h])=>`<div data-t="${k}"${k===this.tool?' class="on"':''}>${l}<small>${loEsc(h)}</small></div>`).join('')}</div></div>
<div class="col pane" id="tlLint"><h3>Lint <span class="sp"></span><span id="tlLintStats" class="empty"></span>
  <button class="btn dim" id="tlLintCopy" title="The warnings as text">Copy</button><button class="btn" id="tlLintRun">Run</button></h3>
  <div class="chips" id="tlLintChips"></div><div class="scroll" id="tlLintBody"><div class="empty">Run to lint the load order.</div></div></div>
<div class="col pane" id="tlBackups" hidden><h3>Backups <span class="sp"></span><span id="tlBkStats" class="empty"></span>
  <button class="btn dim" id="tlBkAll" title="Select all, or none">All</button>
  <button class="btn dim" id="tlBkRestore" title="Copy each selected backup over its original (overwriting it)">Restore</button>
  <button class="btn dim" id="tlBkDelete" title="Delete each selected backup file">Delete</button><button class="btn" id="tlBkScan">Scan</button></h3>
  <div class="scroll" id="tlBkBody"><div class="empty">Scan to find the backups.</div></div></div>
<div class="col pane" id="tlMasters" hidden><h3>Remove / rename master <span class="sp"></span><button class="btn" id="tlMsPick">Pick a plugin...</button></h3>
  <div class="scroll"><div id="tlMsPath" class="empty">A plugin OpenMW refuses because it lists a master it cannot find: remove the master when it is no longer needed (merged away, or its content now comes from loose files or a pluginless version), or point it at the file it was renamed to. The original is kept once as .premaster.bak.</div>
  <div id="tlMsList"></div><pre id="tlMsOut" class="empty" style="white-space:pre-wrap"></pre></div></div>
<div class="col pane" id="tlSave" hidden><h3>Save Check <span class="sp"></span>
  <label title="Detail this many of the largest mods">Detail <input type="number" id="tlSvDetails" value="10" min="0" style="width:48px"></label>
  <input type="text" id="tlSvMods" placeholder="or these mods (comma separated)" style="width:180px">
  <label title="The tables of the whole save too"><input type="checkbox" id="tlSvAll"> Whole save</label>
  <input type="text" id="tlSvDump" placeholder="show the data of scripts matching" style="width:180px">
  <button class="btn dim" id="tlSvTwo" title="Compare two saves, the older first">Compare two...</button><button class="btn" id="tlSvPick">Pick a save...</button></h3>
  <div class="scroll"><div id="tlSvDeps" class="empty">Whether OpenMW can load a save with this load order (every content file it was made with present), and how much space each mod takes in it: the Lua data of its scripts, the objects they made the game save, its journal and globals, with hints to shrink it.</div>
  <pre id="tlSvOut" style="white-space:pre;font-family:var(--mono);margin:0"></pre></div></div>
<div class="col pane" id="tlJournal" hidden style="display:none;grid-template-columns:260px 1fr;grid-template-rows:auto 1fr">
  <h3 style="grid-column:1/3">Journal <span class="sp"></span><span id="tlJrStats" class="empty"></span>
  <input type="text" id="tlJrFind" placeholder="Find a quest or text" style="width:200px"><button class="btn" id="tlJrRead">Read</button></h3>
  <div class="scroll nav" id="tlJrQuests" style="border-right:1px solid var(--line)"><div class="empty">Read the load order's quests.</div></div>
  <div class="scroll" id="tlJrStages"></div></div>
<div class="col pane" id="tlT3" hidden><h3>tes3cmd <span class="sp"></span>
  <select id="tlT3Cmd"><option value="clean">clean</option><option value="sync">sync master sizes</option><option value="header">header</option><option value="custom">custom command</option></select>
  <input type="text" id="tlT3Extra" placeholder="options, or the whole custom command" style="width:220px"><button class="btn" id="tlT3Run">Run</button></h3>
  <div class="acts" style="display:flex;gap:6px;padding:6px 8px;border-bottom:1px solid var(--line);align-items:center;flex-wrap:wrap">
    <input type="text" id="tlT3Exe" placeholder="tes3cmd (found by itself when empty)" style="flex:1;min-width:200px"><button class="btn dim" id="tlT3Browse">Browse...</button>
    <button class="btn dim" id="tlT3Add" title="Add plugin files">Add files...</button><button class="btn dim" id="tlT3Mine" title="Your own mods: the ones the sort places into the curated list">Add your mods</button><button class="btn dim" id="tlT3List" title="The curated list's plugins (the Load order page's list name)">Add list mods</button><button class="btn dim" id="tlT3Clear">Clear</button></div>
  <div class="scroll"><div id="tlT3Files" class="empty">Add the plugins to run on. clean and a custom command run on a copy staged with the plugin's masters, and copy back what changed (a one-time .preclean.bak / .precustom.bak kept); Morrowind.esm, Tribunal.esm and Bloodmoon.esm are never cleaned. sync rewrites only the master sizes (.masterfix.bak kept).</div>
  <pre id="tlT3Out" style="white-space:pre-wrap;font-family:var(--mono);margin:0;padding:6px 8px"></pre></div></div>
<div class="col pane" id="tlLands" hidden><h3>Merge Lands <span class="sp"></span>
  <label title="Report every plugin's settings and everything that could not be read"><input type="checkbox" id="tlMlVerbose"> Verbose</label>
  <label title="Report what would be merged without writing anything"><input type="checkbox" id="tlMlDry"> Dry run</label>
  <select id="tlMlModel" title="What a plugin's change is measured against"><option value="masters">Against the masters (keep every mod's edits)</option><option value="loadorder">Against the load order (follow the winners, fix the seams)</option></select>
  <button class="btn" id="tlMlRun">Build Merged Lands.esp</button></h3>
  <div class="acts" style="display:flex;gap:6px;padding:6px 8px;border-bottom:1px solid var(--line);align-items:center;flex-wrap:wrap">
    <span>Leave out</span><input type="text" id="tlMlIgnore" placeholder="plugin names or folders, comma separated" style="flex:1;min-width:200px" title="Plugins with nothing to merge that are slow to read (generated patches), by name or by folder. The output's own name is always left out.">
  </div>
  <div class="acts" style="display:flex;gap:6px;padding:6px 8px;border-bottom:1px solid var(--line);align-items:center;flex-wrap:wrap">
    <span>Write into</span><input type="text" id="tlMlFolder" placeholder="the data folder holding most of the load order" style="flex:1;min-width:240px"><button class="btn dim" id="tlMlBrowse">Browse...</button></div>
  <div class="scroll"><div id="tlMlIntro" class="empty">Where two mods reshape different vertices of the same exterior cell, the load order keeps only the last one's LAND record and discards the other; this recovers both into a new plugin, Merged Lands.esp. Seams between cells are repaired and the terrain is conditioned so every height fits the format. Your mods are never modified, and deleting the merged plugin restores your previous behaviour exactly. Load it LAST. Merge settings (a plugin's .mergedlands.toml) decide, per layer, what a mod contributes and how its edits resolve.</div>
  <pre id="tlMlOut" style="white-space:pre-wrap;font-family:var(--mono);margin:0;padding:6px 8px"></pre></div></div>
<div class="col pane" id="tlMerge" hidden><h3>Merge settings <span class="sp"></span><select id="tlMgMine" title="One of your mods"><option value="">Your mods...</option></select>
  <button class="btn dim" id="tlMgPick">Pick a plugin...</button><button class="btn" id="tlMgWrite" disabled>Write .mergedlands.toml</button></h3>
  <div class="scroll" style="padding:8px"><div id="tlMgIntro" class="empty" style="padding:0 0 8px">The per-plugin Merge Lands control file, beside the plugin: uncheck a layer to keep the plugin from touching it, set a strategy for how its edits resolve where they collide with another mod's. The defaults behave as if there were no file, and Merge Lands runs without one: write it only when a conflict needs it.</div>
  <div id="tlMgForm"></div>
  <div class="empty" style="padding:8px 0 0">Auto = later plugin wins for entries it changed · Overwrite = this plugin wins collisions · Ignore = keep the earlier edit · Resolve = blend conflicting numbers · Curvature = structure-weighted blend (ours).</div>
  <pre id="tlMgPreview" style="white-space:pre;font-family:var(--mono);margin:8px 0 0;padding:8px;background:var(--bg3);border:1px solid var(--line);border-radius:6px"></pre></div></div>`;
    document.body.appendChild(el);
    for(const d of el.querySelectorAll('#tlNav [data-t]')) d.onclick=()=>this.show(d.dataset.t);
    $('#tlLintRun').onclick=()=>this.runLint();
    $('#tlBkScan').onclick=()=>this.scanBackups();
    $('#tlMsPick').onclick=()=>this.pickPlugin();
    $('#tlSvPick').onclick=()=>this.saveCheck(1);
    $('#tlSvTwo').onclick=()=>this.saveCheck(2);
    $('#tlJrRead').onclick=()=>this.readJournal();
    try{ $('#tlT3Exe').value=localStorage.getItem('wgTools.tes3cmd')||''; }catch(_){}
    $('#tlT3Exe').onchange=()=>{ try{ localStorage.setItem('wgTools.tes3cmd', $('#tlT3Exe').value.trim()); }catch(_){} if(typeof WgSettings==='object') WgSettings.write({tes3cmd:$('#tlT3Exe').value.trim()}); };
    $('#tlT3Browse').onclick=async()=>{ const p=await Engine.pick('open',{}); if(p){ $('#tlT3Exe').value=String(p); $('#tlT3Exe').onchange(); } };
    $('#tlT3Add').onclick=async()=>{
      const p=await Engine.pick('open',{multiple:true, filters:[{name:'Plugins', extensions:['esp','esm','omwaddon','omwgame','ESP','ESM']}]});
      if(!p) return; this.t3Add((Array.isArray(p)?p:[p]).map(String));
    };
    const addNamed=async(kind)=>{
      try{
        const names=await this.mods(kind);
        if(!names.length) return toast(kind==='mine'? 'The sort found no mods of yours' : 'The sort found no curated list','warn',4000);
        let r=await Engine.call('tools_locate',{cfg:this.cfg(), names}); if(typeof r==='string') r=JSON.parse(r);
        this.t3Add(r.filter(x=>x[1]).map(x=>String(x[1])));
      }catch(e){ toast(String(e.message||e),'err',5000); }
    };
    $('#tlT3Mine').onclick=()=>addNamed('mine');
    $('#tlT3List').onclick=()=>addNamed('list');
    $('#tlT3Clear').onclick=()=>{ this.t3.files=[]; this.t3Draw(); };
    $('#tlT3Run').onclick=()=>this.t3Run();
    $('#tlMgPick').onclick=async()=>{
      const p=await Engine.pick('open',{filters:[{name:'Plugins', extensions:['esp','esm','omwaddon','omwgame','ESP','ESM']}]});
      if(p) this.mergeOpen(String(p));
    };
    $('#tlMgMine').onchange=e=>{ const v=e.target.value; e.target.value=''; if(v) this.mergeOpen(v); };
    $('#tlMgWrite').onclick=()=>this.mergeWrite();
    try{ $('#tlMlFolder').value=localStorage.getItem('wgTools.mergeLandsFolder')||''; }catch(_){}
    // tes3cmd and the Merge Lands folder: the settings file's (the Setup page's) when it has them
    if(typeof WgSettings==='object') WgSettings.seed().then(sd=>{
      const t=(sd||{}).tools||{};
      if(t.tes3cmd){ $('#tlT3Exe').value=t.tes3cmd; try{ localStorage.setItem('wgTools.tes3cmd', t.tes3cmd); }catch(_){} }
      if(t.mergeLandsFolder){ $('#tlMlFolder').value=t.mergeLandsFolder; try{ localStorage.setItem('wgTools.mergeLandsFolder', t.mergeLandsFolder); }catch(_){} }
    });
    $('#tlMlFolder').onchange=()=>{ try{ localStorage.setItem('wgTools.mergeLandsFolder', $('#tlMlFolder').value.trim()); }catch(_){} if(typeof WgSettings==='object') WgSettings.write({mergeLandsFolder:$('#tlMlFolder').value.trim()}); };
    $('#tlMlBrowse').onclick=async()=>{ const p=await Engine.pick('open',{directory:true}); if(p){ $('#tlMlFolder').value=String(p); $('#tlMlFolder').onchange(); } };
    $('#tlMlRun').onclick=()=>this.landsRun();
    try{ const m=localStorage.getItem('wgTools.mergeLandsModel'); if(m) $('#tlMlModel').value=m; const ig=localStorage.getItem('wgTools.mergeLandsIgnore'); if(ig!=null) $('#tlMlIgnore').value=ig; }catch(_){}
    $('#tlMlModel').onchange=()=>{ try{ localStorage.setItem('wgTools.mergeLandsModel', $('#tlMlModel').value); }catch(_){} };
    $('#tlMlIgnore').onchange=()=>{ try{ localStorage.setItem('wgTools.mergeLandsIgnore', $('#tlMlIgnore').value); }catch(_){} };
    $('#tlJrFind').oninput=()=>this.drawQuests();
    $('#tlBkRestore').onclick=()=>this.restoreBackups();
    $('#tlBkDelete').onclick=()=>this.deleteBackups();
    $('#tlBkAll').onclick=()=>{ const boxes=[...document.querySelectorAll('#tlBkBody input')]; const on=!boxes.every(b=>b.checked); boxes.forEach(b=>b.checked=on); };
    $('#tlLintCopy').onclick=async()=>{
      try{ await navigator.clipboard.writeText(this.lint.warnings.join('\n')); toast('Copied','ok',2000); }catch(e){ toast(String(e.message||e),'err',4000); }
    };
  },

  show(tool){
    this.tool=tool;
    for(const d of document.querySelectorAll('#tlNav [data-t]')) d.classList.toggle('on', d.dataset.t===tool);
    $('#tlLint').hidden=tool!=='lint';
    $('#tlBackups').hidden=tool!=='backups';
    $('#tlMasters').hidden=tool!=='masters';
    $('#tlSave').hidden=tool!=='save';
    $('#tlJournal').hidden=tool!=='journal';
    $('#tlJournal').style.display=tool==='journal'? 'grid' : 'none';
    $('#tlT3').hidden=tool!=='tes3cmd';
    $('#tlMerge').hidden=tool!=='mergesettings';
    $('#tlLands').hidden=tool!=='mergelands';
    if(tool==='mergelands') this.landsHome();
    if(tool==='mergesettings') this.mergeMine();
  },

  /* ---- Merge Lands (landcore::service) ---- */
  async landsHome(){
    if(this.ml.home) return;
    try{
      const h=await Engine.call('land_merge_home',{cfg:this.cfg()});
      this.ml.home=h;
      if(!$('#tlMlFolder').value && h.home) $('#tlMlFolder').value=h.home;
      let saved=null; try{ saved=localStorage.getItem('wgTools.mergeLandsIgnore'); }catch(_){}
      if(saved==null && !$('#tlMlIgnore').value) $('#tlMlIgnore').value=(h.ignore||[]).join(', ');
      if(!h.home && !$('#tlMlFolder').value) $('#tlMlOut').textContent='No data folder holds any of the load order: choose where to write.';
    }catch(e){ toast(String(e.message||e),'err',5000); }
  },
  async landsRun(){
    if(this.ml.running) return;
    const folder=$('#tlMlFolder').value.trim(), dry=$('#tlMlDry').checked;
    if(!folder && !dry) return toast('Choose the folder to write into first','warn',3000);
    if(!dry){
      const target=folder.replace(/[\\/]+$/,'')+(folder.includes('\\')?'\\':'/')+'Merged Lands.esp';
      const ok=await wgConfirm(`A new plugin will be written to:\n${target}\n\nIt carries terrain only -- no references, objects or scripts -- so everything placed in those cells stays where it is. Your mods are NOT modified, and deleting the merged plugin restores your previous behaviour completely.\n\nLoad it LAST, and back up your saves before playing with it the first time.\n\nIf a Merged Lands.esp is already there, it is REPLACED.`);
      if(!ok) return;
    }
    this.ml.running=true;
    const btn=$('#tlMlRun'); btn.disabled=true;
    const out=$('#tlMlOut');
    const head='='.repeat(70)+'\n MERGE LANDS\n'+'='.repeat(70)+'\nReading the load order\'s terrain...\n';
    out.textContent=head;
    // The merge's lines as it says them (land_merge_progress), every half second, with the
    // time it has taken: a big load order takes minutes, and the page should show it moving.
    let seen=0, shown=[], live=true; const t0=performance.now();
    const tick=async()=>{
      while(live){
        try{
          let p=await Engine.call('land_merge_progress',{from:seen}); if(typeof p==='string') p=JSON.parse(p);
          if(live && p && Array.isArray(p.lines)){ shown=shown.concat(p.lines); seen=p.next; }
          if(live) out.textContent=head+shown.map(l=>'  '+l).join('\n')+`\n\n  ... working (${((performance.now()-t0)/1000).toFixed(0)} s)`;
        }catch(_){}
        await new Promise(r=>setTimeout(r,500));
      }
    };
    tick();
    try{
      const ignore=$('#tlMlIgnore').value.split(',').map(x=>x.trim()).filter(Boolean);
      const r=await Engine.call('land_merge',{cfg:this.cfg(), folder, dryRun:dry, verbose:$('#tlMlVerbose').checked, model:$('#tlMlModel').value, ignore}).finally(()=>{ live=false; });
      const lines=['='.repeat(70),' MERGE LANDS','='.repeat(70), ...r.lines.map(l=>'  '+l)];
      if(r.error) lines.push('', 'Merge failed: '+r.error);
      if(r.notes && r.notes.length) lines.push('', 'Left out or untranslated:', ...r.notes.map(n=>'  '+n));
      if(!r.error && r.output){
        const res=r.result;
        let detail=`${res.contested} vertex/vertices were edited by more than one mod; the later one in your load order won each.`;
        if(res.major) detail+=` ${res.major} were blended far from a mod's intent where a .mergedlands.toml asked to Resolve.`;
        lines.push('', `Written to: ${r.output}`, 'Enable it and place it LAST in your load order.', `${res.cells} land record(s), ${res.textures} land texture(s).`, detail);
        toast('Merged Lands written: '+res.cells+' cell(s)','ok',5000);
      }else if(r.error){ toast('Merge failed -- see the log','err',6000); }
      else toast('Merge produced nothing to write','ok',4000);
      out.textContent=lines.join('\n');
    }catch(e){ out.textContent+='\n\nERROR: '+String(e.message||e); toast(String(e.message||e),'err',6000); }
    finally{ this.ml.running=false; btn.disabled=false; }
  },

  /** Your mods (`mine`) or the curated list's plugins (`list`), by name. A host that
   *  named your mods (`extra.subset`) is taken at its word; otherwise they are the sort's
   *  (the Load order page's plan, or a sort run here on the settings file's fields, as the
   *  page would run it - the program on its own has no host to ask). */
  async mods(kind){
    const host=(this.extra().subset||[]).map(String);
    if(kind==='mine' && host.length) return host;
    let plan=(typeof WgLoadOrder==='object' && WgLoadOrder.plan) || this._plan;
    if(!plan){
      toast('Sorting the load order to find the lists...','ok',3000);
      let seed=await WgSettings.seed(); if(typeof seed==='string') seed=JSON.parse(seed);
      const f=(seed && seed.loadorder) || {};
      const lines=v=>String(v||'').split(/\r?\n/).map(x=>x.trim()).filter(Boolean);
      const a={cfg:f.cfg||this.cfg(), customizations:f.customizations||'', subsetFile:f.subsetFile||'',
        pluginOrderYml:f.pluginOrderYml||'', listName:f.listName||'', dataPathsCache:f.dataPathsCache||'',
        rules:lines(f.rules), groundcover:lines(f.groundcover), subset:[], emitToml:false, allowEmptySort:true,
        sortDataPaths:!!f.sortDataPaths, subsetFromCfg:!!f.subsetFromCfg, noPredicateWarnings:true, verbose:false,
        writeCfg:false, dryRun:true, noBackup:true};
      let got=await Engine.call('loadorder_compute',{args:JSON.stringify(a)}); if(typeof got==='string') got=JSON.parse(got);
      if(got.refused) throw new Error('The sort refused: '+got.refused);
      plan=this._plan=got.plan;
    }
    return ((kind==='mine'? plan.subset : plan.curated) || []).map(String);
  },

  /* ---- Merge settings (landcore::meta) ---- */
  base(p){ return String(p).replace(/[\\/]+$/,'').split(/[\\/]/).pop(); },
  async mergeMine(){
    const sel=$('#tlMgMine'); if(sel.dataset.filled) return;
    const names=await this.mods('mine').catch(()=>[]);
    if(!names.length){ sel.hidden=true; return; }
    try{
      let r=await Engine.call('tools_locate',{cfg:this.cfg(), names}); if(typeof r==='string') r=JSON.parse(r);
      sel.innerHTML='<option value="">Your mods...</option>'+r.filter(x=>x[1]).map(x=>`<option value="${loEsc(String(x[1]))}">${loEsc(x[0])}</option>`).join('');
      sel.dataset.filled='1';
    }catch(_){ sel.hidden=true; }
  },
  async mergeOpen(plugin){
    try{
      let d=await Engine.call('land_meta_read',{plugin}); if(typeof d==='string') d=JSON.parse(d);
      this.mg={plugin, data:d};
      if(d.error) toast(`${this.base(d.path)} could not be read and will be replaced from the defaults: ${d.error}`,'warn',8000);
      const blurb={height_map:'Vertex heights - and the normals stored with them.', vertex_colors:'Baked vertex colors (terrain lighting).',
        texture_indices:'Which land texture paints each square.', world_map_data:'The low-resolution world-map heightmap.'};
      const opts=(list, cur)=>list.map(v=>`<option${v===cur?' selected':''}>${loEsc(v)}</option>`).join('');
      $('#tlMgForm').innerHTML=`<div style="margin-bottom:8px"><b>${loEsc(this.base(plugin))}</b> <span class="empty" style="padding:0">${d.exists? 'editing '+loEsc(d.path) : 'no file yet: '+loEsc(d.path)}</span></div>
        <label title="Patch = a hand-written settings file (the usual choice). Auto = default behaviour. MergedLands marks a plugin the tool generated so a re-run ignores it - do not set that on an ordinary mod.">meta_type <select id="tlMgType">${opts(d.metaTypes, d.metaType)}</select></label>
        <table style="margin-top:8px;border-collapse:collapse"><tr><th style="text-align:left;padding:2px 8px">include</th><th style="text-align:left;padding:2px 8px">layer</th><th style="text-align:left;padding:2px 8px">conflict strategy</th></tr>
        ${d.layers.map(l=>`<tr data-l="${loEsc(l.name)}"><td style="padding:2px 8px;text-align:center"><input type="checkbox"${l.included?' checked':''}></td>
          <td style="padding:2px 8px;font-family:var(--mono)" title="${loEsc(blurb[l.name]||'')}">${loEsc(l.name)}</td>
          <td style="padding:2px 8px"><select${l.included?'':' disabled'}>${opts(d.strategies, l.strategy)}</select></td></tr>`).join('')}</table>`;
      for(const tr of document.querySelectorAll('#tlMgForm tr[data-l]')){
        const box=tr.querySelector('input'), sel=tr.querySelector('select');
        box.onchange=()=>{ sel.disabled=!box.checked; this.mergePreview(); };
        sel.onchange=()=>this.mergePreview();
      }
      $('#tlMgType').onchange=()=>this.mergePreview();
      $('#tlMgWrite').disabled=false;
      this.mergePreview();
    }catch(e){ toast(String(e.message||e),'err',6000); }
  },
  mergeForm(){
    return {metaType:$('#tlMgType').value, layers:[...document.querySelectorAll('#tlMgForm tr[data-l]')].map(tr=>(
      {name:tr.dataset.l, included:tr.querySelector('input').checked, strategy:tr.querySelector('select').value}))};
  },
  async mergePreview(){
    try{ $('#tlMgPreview').textContent=await Engine.call('land_meta_preview', this.mergeForm()); }
    catch(e){ $('#tlMgPreview').textContent=String(e.message||e); }
  },
  async mergeWrite(){
    if(!this.mg.plugin) return;
    try{
      const path=await Engine.call('land_meta_write',{plugin:this.mg.plugin, ...this.mergeForm()});
      toast(`Wrote ${path}. Run Merge Lands to apply it.`,'ok',6000);
      this.mergeOpen(this.mg.plugin);
    }catch(e){ toast('Could not write .mergedlands.toml: '+String(e.message||e),'err',8000); }
  },

  /* ---- tes3cmd ---- */
  t3Add(paths){ for(const p of paths) if(!this.t3.files.includes(p)) this.t3.files.push(p); this.t3Draw(); },
  t3Draw(){
    const box=$('#tlT3Files');
    if(!this.t3.files.length){ box.className='empty'; box.textContent='No plugins listed.'; return; }
    box.className='';
    box.innerHTML=this.t3.files.map((f,i)=>`<div style="font-family:var(--mono);padding:1px 8px;display:flex;gap:6px"><span style="flex:1">${loEsc(f)}</span><a href="#" data-i="${i}">remove</a></div>`).join('');
    for(const a of box.querySelectorAll('a[data-i]')) a.onclick=e=>{ e.preventDefault(); this.t3.files.splice(+a.dataset.i,1); this.t3Draw(); };
  },
  async t3Run(){
    const cmd=$('#tlT3Cmd').value, extra=$('#tlT3Extra').value.split(/\s+/).filter(Boolean);
    if(!this.t3.files.length) return toast('Add at least one plugin file first','warn',3000);
    const what={clean:'Clean', sync:'Resync the master sizes of', header:'Run tes3cmd header on', custom:'Run tes3cmd '+extra.join(' ')+' on'}[cmd];
    if(!await wgConfirm(`${what} ${this.t3.files.length} plugin file(s)?`)) return;
    const btn=$('#tlT3Run'); btn.disabled=true;
    $('#tlT3Out').textContent='Running...';
    try{
      let exe=$('#tlT3Exe').value.trim();
      if(cmd!=='sync' && !exe) exe=await Engine.call('tools_tes3cmd_find',{cfg:this.cfg(), explicit:''});
      let r=await Engine.call('tools_tes3cmd',{cfg:this.cfg(), exe:String(exe||''), cmd, extra, files:this.t3.files});
      if(typeof r==='string') r=JSON.parse(r);
      $('#tlT3Out').textContent=r.lines.join('\n')+'\n\n'+r.status;
      toast(r.status,'ok',5000);
    }catch(e){ $('#tlT3Out').textContent=String(e.message||e); }
    finally{ btn.disabled=false; }
  },

  /* ---- Journal ---- */
  async readJournal(){
    const btn=$('#tlJrRead'); btn.disabled=true;
    $('#tlJrStats').textContent='Reading every topic and script...';
    try{
      let r=await Engine.call('tools_journal',{cfg:this.cfg()});
      if(typeof r==='string') r=JSON.parse(r);
      this.jr.data=r;
      const c=r.counts;
      $('#tlJrStats').textContent=`${c.quests} quest(s), ${c.stages} stage(s), ${c.calls} call site(s)`+(c.unattached? `, ${c.unattached} setting no known stage` : '');
      this.drawQuests();
    }catch(e){ $('#tlJrStats').textContent=''; toast(String(e.message||e),'err',6000); }
    finally{ btn.disabled=false; }
  },
  drawQuests(){
    const d=this.jr.data; if(!d) return;
    const q=$('#tlJrFind').value.trim().toLowerCase();
    const shown=d.quests.filter(x=>!q || x.id.toLowerCase().includes(q) || x.stages.some(s=>s.text.toLowerCase().includes(q)));
    const box=$('#tlJrQuests');
    box.innerHTML=shown.slice(0,3000).map(x=>`<div data-q="${loEsc(x.id)}"${this.jr.quest===x.id?' class="on"':''}>${loEsc(x.id)}<small>${x.stages.length} stage(s)</small></div>`).join('')||'<div class="empty">No quest matches.</div>';
    for(const el of box.querySelectorAll('[data-q]')) el.onclick=()=>this.showQuest(el.dataset.q);
  },
  showQuest(id, infoId){
    const d=this.jr.data, quest=d.quests.find(x=>x.id===id); if(!quest) return;
    this.jr.quest=id;
    for(const el of document.querySelectorAll('#tlJrQuests [data-q]')) el.classList.toggle('on', el.dataset.q===id);
    const conds=c=>c.length? `<div class="empty" style="padding:0">${c.map(loEsc).join(' &rsaquo; ')}</div>` : '';
    const eff=list=>list.length? `<div style="margin:2px 0 0 12px">${list.map(e=>`<div style="font-family:var(--mono)">${loEsc(e.raw)}${e.conditions.length?' <span class="empty" style="padding:0">('+e.conditions.map(loEsc).join(' &rsaquo; ')+')</span>':''}</div>`).join('')}</div>` : '';
    const link=c=>c.target? `<a href="#" data-jq="${loEsc(c.target.quest)}" data-ji="${loEsc(c.target.infoId)}">${loEsc(c.target.quest)} ${c.index}</a>` : `${loEsc(c.quest)} ${c.index} <span style="color:var(--warn)">(no such stage)</span>`;
    const box=$('#tlJrStages');
    box.innerHTML=quest.stages.map(st=>`<div class="warn" id="jr_${loEsc(st.infoId)}" style="border-color:${st.finished?'var(--ok,#4a9)':'var(--line2)'}">
      <span class="kind">${st.index}${st.state? ' - '+loEsc(st.state) : ''} - ${st.plugins.map(loEsc).join(' &rarr; ')}</span>${loEsc(st.text||'(no text)')}
      ${st.conditions.length? `<div class="empty" style="padding:2px 0">${st.conditions.map(loEsc).join('<br>')}</div>` : ''}
      ${st.effects.length? '<div><b>Its script does</b>'+eff(st.effects)+'</div>' : ''}
      ${st.alsoSets.length? '<div><b>Also sets</b> '+st.alsoSets.map(link).join(', ')+'</div>' : ''}
      ${st.setBy.length? '<div><b>Set by</b>'+st.setBy.map(c=>`<div style="margin:3px 0 0 12px">&#9666; ${loEsc(c.ownerType==='Script'?'script ':'response ')}${loEsc(c.ownerId)} <span class="empty" style="padding:0">(${loEsc(c.plugin)}${c.fromBytecode?', from compiled code':''})</span>${conds(c.conditions)}${c.speaker&&c.speaker.length?'<div class="empty" style="padding:0">'+c.speaker.map(loEsc).join('<br>')+'</div>':''}${eff(c.effects||[])}</div>`).join('')+'</div>' : '<div class="empty" style="padding:0">Nothing in the load order sets this stage.</div>'}
    </div>`).join('');
    for(const a of box.querySelectorAll('a[data-jq]')) a.onclick=e=>{ e.preventDefault(); this.showQuest(a.dataset.jq, a.dataset.ji); };
    const at=infoId && document.getElementById('jr_'+infoId);
    if(at){ at.scrollIntoView({block:'center'}); at.style.outline='1px solid var(--ac)'; setTimeout(()=>at.style.outline='',1500); } else box.scrollTop=0;
  },

  /* ---- Save Check ---- */
  async saveCheck(n){
    const saves=[];
    for(let i=0;i<n;i++){
      const p=await Engine.pick('open',{filters:[{name:'OpenMW saves', extensions:['omwsave']}]});
      if(!p) return;
      saves.push(String(p));
    }
    const out=$('#tlSvOut'), deps=$('#tlSvDeps');
    out.textContent=''; deps.textContent='Reading the save'+(n>1?'s':'')+'...';
    const words=id=>$(id).value.split(',').map(s=>s.trim()).filter(Boolean);
    try{
      let r=await Engine.call('tools_save_check',{cfg:this.cfg(), saves, details:Math.max(0, +$('#tlSvDetails').value||0),
        mods:words('#tlSvMods'), all:$('#tlSvAll').checked, dump:words('#tlSvDump')});
      if(typeof r==='string') r=JSON.parse(r);
      deps.className='';      // not the placeholder's dim text any more
      deps.innerHTML=r.saves.map(sv=>{
        if(sv.error) return `<div class="warn FOGBUG"><span class="kind">${loEsc(sv.name)}</span>${loEsc(sv.error)}</div>`;
        if(sv.missing.length) return `<div class="warn FOGBUG"><span class="kind">${loEsc(sv.name)}: ${sv.missing.length} content file(s) missing - OpenMW will refuse to load it</span>${sv.missing.map(loEsc).join('<br>')}</div>`;
        return `<div class="warn" style="border-color:var(--ok,#4a9)"><span class="kind">${loEsc(sv.name)}</span>All ${sv.files.length} content files it depends on are in this load order.</div>`;
      }).join('');
      out.textContent=r.report;
    }catch(e){ deps.textContent=String(e.message||e); }
  },

  /* ---- Remove / rename master ---- */
  async pickPlugin(){
    const p=await Engine.pick('open',{filters:[{name:'Plugins', extensions:['esp','esm','omwaddon','omwgame','ESP','ESM']}]});
    if(!p) return;
    this.ms.path=String(p);
    this.loadMasters();
  },
  async loadMasters(){
    $('#tlMsPath').textContent=this.ms.path;
    $('#tlMsOut').textContent='';
    try{
      let list=await Engine.call('tools_masters',{path:this.ms.path});
      if(typeof list==='string') list=JSON.parse(list);
      this.ms.list=list;
      const box=$('#tlMsList');
      if(!list.length){ box.innerHTML='<div class="empty">This plugin has no masters.</div>'; return; }
      box.innerHTML=list.map(([n,size],i)=>`<div class="warn" style="border-color:var(--line2);display:flex;gap:6px;align-items:center">`+
        `<span style="flex:1;font-family:var(--mono)">${i+1}. ${loEsc(n)} <span class="empty" style="padding:0">(${size} bytes)</span></span>`+
        `<button class="btn dim" data-a="preview" data-m="${loEsc(n)}" title="What removing it would do, without writing">Preview</button>`+
        `<button class="btn dim" data-a="remove" data-m="${loEsc(n)}" title="Remove it; references into it are dropped, those into later masters renumbered">Remove</button>`+
        `<button class="btn dim" data-a="rename" data-m="${loEsc(n)}" title="Point it at another file (renamed on disk); every reference kept">Rename...</button></div>`).join('');
      for(const b of box.querySelectorAll('button')) b.onclick=()=>this.masterAct(b.dataset.a, b.dataset.m);
    }catch(e){ toast(String(e.message||e),'err',6000); }
  },
  async masterAct(act, master){
    const out=$('#tlMsOut'), name=this.ms.path.split(/[\\/]/).pop();
    try{
      if(act==='rename'){
        const to=await Engine.pick('open',{filters:[{name:'Plugins', extensions:['esp','esm','omwaddon','omwgame','ESP','ESM']}]});
        if(!to) return;
        if(!await wgConfirm(`Point master '${master}' at '${String(to).split(/[\\/]/).pop()}' in '${name}'?\n\nReferences are kept; a one-time .premaster.bak of the original is kept.`)) return;
        let r=await Engine.call('tools_rename_master',{path:this.ms.path, old:master, newPath:String(to)});
        if(typeof r==='string') r=JSON.parse(r);
        out.textContent=r.renamed? `Renamed. Masters now: ${r.remaining.join(', ')||'(none)'}\nBackup: ${r.backup}` : `'${master}' is not a master of '${name}'.`;
      }else{
        const dry=act==='preview';
        if(!dry && !await wgConfirm(`Remove master '${master}' from '${name}'?\n\nReferences into it are dropped, later ones renumbered; a one-time .premaster.bak of the original is kept.`)) return;
        let r=await Engine.call('tools_remove_master',{path:this.ms.path, master, dryRun:dry});
        if(typeof r==='string') r=JSON.parse(r);
        if(!r.removed){ out.textContent=`'${master}' is not a master of '${name}' -- nothing to do.`; return; }
        const lines=[dry? 'Preview (nothing written):' : 'Removed.'];
        if(r.dropped){ lines.push(`  ${r.dropped} reference(s) into the master ${dry?'would be':'were'} dropped:`); for(const [c,n] of r.droppedByCell) lines.push(`      ${c}: ${n}`); }
        else lines.push('  No references pointed into it -- a clean removal.');
        if(r.remapped) lines.push(`  ${r.remapped} reference(s) into later masters ${dry?'would be':'were'} renumbered.`);
        lines.push(`  Masters ${dry?'after':'remaining'}: ${r.remaining.join(', ')||'(none)'}`);
        if(r.backup) lines.push(`  Backup: ${r.backup}`);
        out.textContent=lines.join('\n');
      }
      if(act!=='preview') this.loadMasters().then(()=>{ $('#tlMsOut').textContent=out.textContent; });
    }catch(e){ out.textContent=String(e.message||e); }
  },

  /* ---- Backups ---- */
  async scanBackups(){
    const btn=$('#tlBkScan'); btn.disabled=true;
    try{
      let r=await Engine.call('tools_backups',{cfg:this.cfg(), dirs:(this.extra().backupDirs||[]).map(String)});
      if(typeof r==='string') r=JSON.parse(r);
      this.bk.rows=r;
      $('#tlBkStats').textContent=r.length+' backup file(s)';
      this.drawBackups();
    }catch(e){ toast(String(e.message||e),'err',6000); }
    finally{ btn.disabled=false; }
  },
  drawBackups(){
    const box=$('#tlBkBody');
    if(!this.bk.rows.length){ box.innerHTML='<div class="empty">No backups.</div>'; return; }
    box.innerHTML=this.bk.rows.map((b,i)=>`<label style="display:block;font-family:var(--mono);padding:1px 0"><input type="checkbox" data-i="${i}"> `+
      `<span class="empty" style="padding:0">[${loEsc(b.kind)}]</span> ${loEsc(b.path)}${b.originalExists?'':' <span style="color:var(--warn)">[original missing]</span>'}</label>`).join('');
  },
  picked(){ return [...document.querySelectorAll('#tlBkBody input:checked')].map(c=>this.bk.rows[+c.dataset.i]); },
  async restoreBackups(){
    const sel=this.picked(); if(!sel.length) return toast('Select backups first','warn',2500);
    if(!await wgConfirm(`Copy ${sel.length} backup(s) over their originals (overwriting them)?`)) return;
    try{
      let r=await Engine.call('tools_backups_restore',{pairs:sel.map(b=>[b.path,b.original])});
      if(typeof r==='string') r=JSON.parse(r);
      toast(`Restored ${r.restored}`+(r.failed.length? `, ${r.failed.length} failed: `+r.failed.map(f=>f[1]).join('; ') : '')+'. Sort again to refresh the checks.', r.failed.length?'warn':'ok', 6000);
    }catch(e){ toast(String(e.message||e),'err',6000); }
  },
  async deleteBackups(){
    const sel=this.picked(); if(!sel.length) return toast('Select backups first','warn',2500);
    if(!await wgConfirm(`Permanently delete ${sel.length} backup file(s)?`)) return;
    try{
      const n=await Engine.call('tools_backups_delete',{paths:sel.map(b=>b.path)});
      toast(`Deleted ${n} backup file(s)`,'ok',3000);
      this.scanBackups();
    }catch(e){ toast(String(e.message||e),'err',6000); }
  },

  /* ---- Lint ---- */
  async runLint(){
    const btn=$('#tlLintRun'); btn.disabled=true;
    $('#tlLintStats').textContent='Reading every plugin...';
    try{
      let r=await Engine.call('tools_lint',{cfg:this.cfg(), subset:await this.mods('mine').catch(()=>[])});
      if(typeof r==='string') r=JSON.parse(r);
      this.lint.warnings=r.warnings; this.lint.stats=r.stats; this.lint.filter='all';
      const s=r.stats;
      $('#tlLintStats').textContent=`${s.scanned} plugin(s), ${s.interiorCells} new interior(s), ${s.pathgrids} interior path grid(s)`+(s.unreadable? `, ${s.unreadable} unreadable` : '');
      this.drawLint();
    }catch(e){ $('#tlLintStats').textContent=''; toast(String(e.message||e),'err',6000); }
    finally{ btn.disabled=false; }
  },

  kindOf(w){ const m=/^\[([^\]]+)\]/.exec(w); return m? m[1] : ''; },

  drawLint(){
    const ws=this.lint.warnings, counts={};
    for(const w of ws){ const k=this.kindOf(w); counts[k]=(counts[k]||0)+1; }
    const chips=$('#tlLintChips');
    chips.innerHTML=['all',...Object.keys(counts)].map(k=>
      `<span class="chip${this.lint.filter===k?' on':''}" data-k="${loEsc(k)}">${k==='all'? 'All ('+ws.length+')' : loEsc(this.KINDS[k]||k)+' ('+counts[k]+')'}</span>`).join('');
    for(const c of chips.querySelectorAll('.chip')) c.onclick=()=>{ this.lint.filter=c.dataset.k; this.drawLint(); };
    const box=$('#tlLintBody');
    const shown=ws.filter(w=>this.lint.filter==='all' || this.kindOf(w)===this.lint.filter);
    if(!shown.length){ box.innerHTML=`<div class="empty">${ws.length||!this.lint.stats? '-' : 'Nothing to report.'}</div>`; return; }
    box.innerHTML='';
    for(const w of shown){
      const k=this.kindOf(w);
      const d=document.createElement('div');
      d.className='warn '+k.replace(/\s+/g,'');
      if(k==='FOGBUG'||k==='EVLGMST') d.classList.add(k);
      d.innerHTML=`<span class="kind">${loEsc(this.KINDS[k]||k)}</span>${loEsc(w.replace(/^\[[^\]]+\]\s*/,''))}`;
      box.appendChild(d);
    }
  },
};
