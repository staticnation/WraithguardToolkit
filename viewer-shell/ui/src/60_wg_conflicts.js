/* =====================================================================================
   The Conflicts page - Wraithguard (port step 4.3, slice 11). Wraithguard's conflict
   window, in the viewer, as a page beside Load order, the Cell viewer and the Editor.

   - Records (`conflicts_scan`): the records two or more plugins of the load order define,
     or those only your mods (or only the others) define; filtered by text and to your own.
     A record opens its fields across its plugins (`conflicts_fields`), each cell coloured by
     what that plugin does to it (master, identical, override wins, conflict wins / loses),
     with the patch steps beside it: the whole record from a plugin, a field from a plugin,
     or a value typed for a field (`conflicts_patch`, into the journal Wraithguard and the
     Editor share).
   - A `LAND` record's terrain in each plugin, and what each merge strategy would make of it
     (`conflicts_land_surfaces`, landcore::preview), shaded or as a difference.
   - Files (`conflicts_resources`): the loose files two or more data folders provide, the
     ones that differ first, a mesh's finding when the winner loses something; a texture
     compared with what it replaced (`conflicts_texture`): both sides and where they differ.
   - Plugins (`conflicts_by_plugin`, `conflicts_plugin_group`): the same scan as plugin ->
     record type -> records, a plugin judged when opened (each record by what this plugin
     does to it, the type and the plugin by their worst), the plugins it shares records with;
     Merge fields from a plugin (`conflicts_plugin_bulk`, `_apply`): the fields it would
     change across every record it shares, picked and queued in one go.
   - Map: the record conflicts gathered on the world grid, coloured by how many, a plugin
     focused to see only its own.
   - The reports and CSVs (`conflicts_report`, `conflicts_csv`).

   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */

const WgConflicts={
  on:false,
  tab:'records',
  list:[], stats:null, cells:[], mode:'conflicts',
  files:[], fstats:null, mstats:null,
  sel:null, fsel:null, focus:'',
  branches:[], judged:{}, openP:{}, openT:{}, psel:'',
  KEY:'wgConflicts.fields',
  STATUS:{'master':'cf-master','identical to master':'cf-itm','override, wins':'cf-ovr','conflict, wins':'cf-cw',
    'conflict, loses':'cf-cl','deleted':'cf-del','unknown':'cf-unk','ignored':'cf-unk'},
  ALL:['Unknown','Only one','No conflict','Override (nothing lost)','Conflict (an edit is lost)'],

  view(){ return window.__WG_VIEW__||{}; },
  extra(){ return this.view().extra||{}; },
  cfg(){ return this.view().cfg||''; },

  button(){
    if($('#btnConflicts')) return;
    const after=$('#btnLoadOrder')||$('#btnReport'); if(!after) return;
    const b=document.createElement('button');
    b.className='btn dim'; b.id='btnConflicts'; b.textContent='Conflicts';
    b.title='Which records and loose files more than one mod provides, who wins, what is lost, and patching it';
    b.onclick=()=> this.on? this.leave() : this.enter();
    after.insertAdjacentElement('afterend', b);
  },

  enter(){
    if(this.on) return;
    if(typeof WgLoadOrder==='object' && WgLoadOrder.on) WgLoadOrder.leave();
    if(typeof WgToolsPage==='object' && WgToolsPage.on) WgToolsPage.leave();
    if(typeof WgHelp==='object' && WgHelp.on) WgHelp.leave();
    this.on=true;
    this.build();
    $('#wgCF').hidden=false;
    const i=document.querySelector('#brand i'); if(i){ this._brand=i.textContent; i.textContent='Conflicts'; }
    document.title='Wraithguard - Conflicts';
    const b=$('#btnConflicts'); if(b) b.classList.add('on');
    const tab=this.extra().conflictsTab; if(tab==='files' || tab==='records' || tab==='plugins'){ this.show(tab); this.extra().conflictsTab=''; }
  },

  leave(){
    if(!this.on) return;
    this.on=false;
    $('#wgCF').hidden=true;
    const i=document.querySelector('#brand i'); if(i && this._brand) i.textContent=this._brand;
    const b=$('#btnConflicts'); if(b) b.classList.remove('on');
  },

  /** The type name a conflict row carries for each four-byte tag (the tes3 crate's names). */
  TYPES:{ACTI:'Activator',ALCH:'Alchemy',APPA:'Apparatus',ARMO:'Armor',BODY:'Bodypart',BOOK:'Book',BSGN:'Birthsign',
    CELL:'Cell',CLAS:'Class',CLOT:'Clothing',CONT:'Container',CREA:'Creature',DIAL:'Dialogue',DOOR:'Door',ENCH:'Enchanting',
    FACT:'Faction',GLOB:'GlobalVariable',GMST:'GameSetting',INFO:'DialogueInfo',INGR:'Ingredient',LAND:'Landscape',
    LEVC:'LeveledCreature',LEVI:'LeveledItem',LIGH:'Light',LOCK:'Lockpick',LTEX:'LandscapeTexture',MGEF:'MagicEffect',
    MISC:'MiscItem',NPC_:'Npc',PGRD:'PathGrid',PROB:'Probe',RACE:'Race',REGN:'Region',REPA:'RepairItem',SCPT:'Script',
    SKIL:'Skill',SNDG:'SoundGen',SOUN:'Sound',SPEL:'Spell',SSCR:'StartScript',STAT:'Static',WEAP:'Weapon'},

  /** A record from elsewhere in the viewer (the object inspector, the Editor's record dialog):
   *  the page opened on Records, the record conflicts scan run if none has been, and the
   *  record's fields shown (`goto_conflict_record`). A record no two plugins define is not
   *  in the list, and is said so. */
  async goto(tag, id){
    this.enter();
    this.show('records');
    if(this.mode!=='conflicts' || !this.list.length){
      $('#cfMode').value='conflicts';
      await this.scan();
    }
    const want=String(id).toLowerCase(), type=this.TYPES[tag]||'';
    const hits=this.list.filter(c=>String(c.id).toLowerCase()===want);
    const c=hits.find(h=>!type || h.type===type)||hits[0];
    if(!c){ toast(`${id} is not a conflict: only one plugin of the load order defines it`,'ok',4000); return; }
    $('#cfMine').checked=false;
    $('#cfFind').value=c.id;
    this.sel=c;
    this.draw();
    this.fields(c);
  },

  saved(){ try{ return JSON.parse(localStorage.getItem(this.KEY)||'{}')||{}; }catch(_){ return {}; } },
  save(){
    const v={exclude:$('#cfExclude').value, exts:$('#cfExts').value, compare:$('#cfCompare').checked, meshes:$('#cfMeshes').checked};
    try{ localStorage.setItem(this.KEY, JSON.stringify(v)); }catch(_){}
  },
  words(id){ return $(id).value.split(/[,\n]/).map(s=>s.trim()).filter(Boolean); },

  build(){
    if($('#wgCF')) return;
    const st=document.createElement('style');
    st.textContent=`
#wgCF{position:fixed;left:0;right:0;top:var(--top-h,40px);bottom:0;z-index:60;background:var(--bg);color:var(--tx);
  display:flex;flex-direction:column;gap:6px;padding:8px;overflow:hidden;font-size:12px}
#wgCF[hidden]{display:none}
#wgCF .bar{display:flex;gap:6px;align-items:center;flex-wrap:wrap}
#wgCF .bar .sp{flex:1}
#wgCF .tabs .chip{background:var(--bg3);border:1px solid var(--line2);border-radius:10px;padding:2px 10px;cursor:pointer;color:var(--tx2)}
#wgCF .tabs .chip.on{background:var(--acd);border-color:var(--ac);color:var(--tx)}
#wgCF input[type=text],#wgCF select{background:var(--bg3);color:var(--tx);border:1px solid var(--line2);border-radius:var(--r-fld);padding:3px 5px;font:inherit}
#wgCF .pane{flex:1;min-height:0;display:grid;grid-template-columns:minmax(320px,2fr) 3fr;gap:8px}
#wgCF .pane[hidden]{display:none}
#wgCF .col{display:flex;flex-direction:column;min-height:0;background:var(--bg2);border:1px solid var(--line);border-radius:var(--r-box)}
#wgCF h3{margin:0;padding:6px 8px;font-size:12px;border-bottom:1px solid var(--line);display:flex;gap:6px;align-items:center}
#wgCF h3 .sp{flex:1}
#wgCF .scroll{overflow:auto;flex:1;min-height:0}
#wgCF .rows div.r{padding:2px 8px;cursor:pointer;font-family:var(--mono);white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
#wgCF .rows div.r:hover{background:var(--bg3)}
#wgCF .rows div.r.sel{background:var(--acd)}
#wgCF .rows div.r.mine::before{content:'* ';color:var(--ac)}
#wgCF .rows .t{color:var(--tx3)}
#wgCF .rows .w{color:var(--tx2)}
#wgCF .rows .same{color:var(--tx3)}
#wgCF .rows .note{color:var(--warn)}
#wgCF table{border-collapse:collapse;font-family:var(--mono);width:max-content;min-width:100%}
#wgCF th,#wgCF td{border:1px solid var(--line);padding:2px 6px;text-align:left;vertical-align:top;max-width:360px;overflow:hidden;text-overflow:ellipsis;white-space:pre-wrap;word-break:break-all}
#wgCF th{position:sticky;top:0;background:var(--bg3);z-index:1}
#wgCF td.name{color:var(--tx2);white-space:nowrap}
#wgCF tr.same td{opacity:.55}
#wgCF .cf-master{background:color-mix(in srgb,var(--bg2) 85%,#6a8)}
#wgCF .cf-itm{background:color-mix(in srgb,var(--bg2) 85%,#888)}
#wgCF .cf-ovr{background:color-mix(in srgb,var(--bg2) 80%,#4a9)}
#wgCF .cf-cw{background:color-mix(in srgb,var(--bg2) 75%,#c90)}
#wgCF .cf-cl{background:color-mix(in srgb,var(--bg2) 70%,#c44)}
#wgCF .cf-del{background:color-mix(in srgb,var(--bg2) 70%,#a4a)}
#wgCF .acts{display:flex;gap:6px;padding:6px 8px;border-bottom:1px solid var(--line);flex-wrap:wrap;align-items:center}
#wgCF .empty{color:var(--tx3);padding:8px}
#wgCF pre{margin:0;padding:8px;font-family:var(--mono);white-space:pre-wrap;color:var(--tx2)}
#wgCF .imgs{display:flex;gap:8px;flex-wrap:wrap;padding:8px}
#wgCF .imgs figure{margin:0}
#wgCF .imgs canvas{max-width:360px;image-rendering:pixelated;background:repeating-conic-gradient(#666 0 25%,#999 0 50%) 0 0/16px 16px;border:1px solid var(--line)}
#wgCF .imgs figcaption{color:var(--tx2);font-size:11px;max-width:360px;overflow-wrap:anywhere}
#wgCF .tree div.r{padding-left:8px}
#wgCF .tree div.r.k{padding-left:22px}
#wgCF .tree div.r.i{padding-left:36px}
#wgCF .tree .tw{display:inline-block;width:12px;color:var(--tx3)}
#wgCF .tree .n{color:var(--tx3)}
#wgCF .tree .cf-master,#wgCF .tree .cf-itm,#wgCF .tree .cf-ovr,#wgCF .tree .cf-cw,#wgCF .tree .cf-cl,#wgCF .tree .cf-del{border-radius:3px}
#wgCF .peer{color:var(--warn)}
#wgCF #cfPDetail>.col{flex:1}
#wgCF .bulk td.c{text-align:right}
#wgCF #cfMapWrap{position:relative;flex:1;min-height:0}
#wgCF #cfMap{position:absolute;inset:0;width:100%;height:100%}
#cfTip{position:fixed;z-index:80;pointer-events:none;background:var(--bg3);border:1px solid var(--line2);padding:4px 6px;font-size:11px;white-space:pre;color:var(--tx);display:none}`;
    document.head.appendChild(st);

    const el=document.createElement('div');
    el.id='wgCF'; el.hidden=true;
    el.innerHTML=`
<div class="bar tabs"><span class="chip on" data-tab="records">Records</span><span class="chip" data-tab="files">Files</span><span class="chip" data-tab="plugins" title="The scan as plugin -> record type -> records">Plugins</span><span class="chip" data-tab="map">Map</span>
  <span class="sp"></span><label><input type="checkbox" id="cfMine"> Your mods only</label>
  <input type="text" id="cfFind" placeholder="Find (type, id, plugin, path)" style="width:220px">
  <button class="btn dim" id="cfReport" title="The scan as text">Report</button><button class="btn dim" id="cfCsv" title="Save the scan as CSV">CSV...</button></div>
<div class="bar" id="cfRecBar"><select id="cfMode"><option value="conflicts">Conflicting records</option><option value="mine">Only your mods define</option><option value="other">Only one other plugin defines</option></select>
  <input type="text" id="cfExclude" placeholder="Leave out plugins (globs or words, comma separated)" style="flex:1;min-width:200px">
  <button class="btn" id="cfScan">Scan records</button><span id="cfStats" class="empty"></span></div>
<div class="bar" id="cfFileBar" hidden><input type="text" id="cfExts" placeholder="Leave out extensions (.txt, .md)" style="width:220px">
  <label><input type="checkbox" id="cfCompare" checked> Compare bytes</label><label><input type="checkbox" id="cfMeshes" checked> Read the meshes that differ</label>
  <button class="btn" id="cfScanFiles">Scan files</button><span id="cfFStats" class="empty"></span></div>
<div class="pane" id="cfRecords"><div class="col"><h3>Records <span class="sp"></span><span id="cfCount"></span></h3><div class="scroll rows" id="cfList"><div class="empty">Scan to see the records.</div></div></div>
  <div class="col"><h3 id="cfTitle">-</h3><div class="acts" id="cfActs"></div><div class="scroll" id="cfFields"><div class="empty">Pick a record.</div></div></div></div>
<div class="pane" id="cfFiles" hidden><div class="col"><h3>Files <span class="sp"></span><span id="cfFCount"></span></h3><div class="scroll rows" id="cfFList"><div class="empty">Scan to see the files.</div></div></div>
  <div class="col"><h3 id="cfFTitle">-</h3><div class="scroll" id="cfFDetail"><div class="empty">Pick a file.</div></div></div></div>
<div class="pane" id="cfPlugins" hidden><div class="col"><h3>Plugins <span class="sp"></span><span id="cfPCount"></span>
  <button class="btn dim" id="cfJudgeAll" title="Judge every plugin's records, so the whole tree is coloured">Colour all</button>
  <button class="btn dim" id="cfBulk" title="Pick fields the selected plugin should win, across every record it shares">Merge fields...</button></h3>
  <div class="empty" id="cfPeers" style="padding:4px 8px"></div><div class="scroll rows tree" id="cfPTree"><div class="empty">Scan to see the plugins.</div></div></div>
  <div id="cfPDetail" style="display:flex;flex-direction:column;min-height:0"></div></div>
<div class="pane" id="cfMapPane" hidden style="grid-template-columns:1fr"><div class="col"><h3>Conflicts by cell <span class="sp"></span>
  <select id="cfFocus"><option value="">Every plugin</option></select></h3><div id="cfMapWrap"><canvas id="cfMap"></canvas></div><div id="cfMapNote" class="empty"></div></div></div>
<div class="pane" id="cfText" hidden style="grid-template-columns:1fr"><div class="col"><h3>Report <span class="sp"></span><button class="btn dim" id="cfTextClose">Close</button></h3><div class="scroll"><pre id="cfTextBody"></pre></div></div></div>`;
    document.body.appendChild(el);
    const tip=document.createElement('div'); tip.id='cfTip'; document.body.appendChild(tip);

    const v=this.saved();
    $('#cfExclude').value=v.exclude||''; $('#cfExts').value=v.exts||'';
    if(v.compare!=null) $('#cfCompare').checked=!!v.compare;
    if(v.meshes!=null) $('#cfMeshes').checked=!!v.meshes;
    // what the scans leave out: the settings file's (the Setup page's) when it has it
    if(typeof WgSettings==='object') WgSettings.seed().then(sd=>{
      const ex=((sd||{}).conflicts||{}).exclude; if(ex){ $('#cfExclude').value=ex; this.save(true); }
    });
    $('#cfExclude').addEventListener('change', ()=>{ if(typeof WgSettings==='object') WgSettings.write({exclude:$('#cfExclude').value}); });
    el.addEventListener('change', ()=>this.save());
    for(const c of el.querySelectorAll('.tabs .chip')) c.onclick=()=>this.show(c.dataset.tab);
    $('#cfScan').onclick=()=>this.scan();
    $('#cfScanFiles').onclick=()=>this.scanFiles();
    $('#cfMine').onchange=()=>{ this.draw(); if(this.tab==='plugins') this.drawTree(); };
    $('#cfFind').oninput=()=>{ this.draw(); if(this.tab==='plugins') this.drawTree(); };
    $('#cfReport').onclick=()=>this.report();
    $('#cfCsv').onclick=()=>this.csv();
    $('#cfTextClose').onclick=()=>this.show(this.tab);
    $('#cfJudgeAll').onclick=()=>this.judgeAll();
    $('#cfBulk').onclick=()=>this.bulk();
    $('#cfFocus').onchange=e=>{ this.focus=e.target.value; this.drawMap(); };
    window.addEventListener('resize', ()=>{ if(this.on && this.tab==='map') this.drawMap(); });
  },

  show(tab){
    this.tab=tab;
    for(const c of document.querySelectorAll('#wgCF .tabs .chip')) c.classList.toggle('on', c.dataset.tab===tab);
    $('#cfRecords').hidden=tab!=='records';
    $('#cfFiles').hidden=tab!=='files';
    $('#cfMapPane').hidden=tab!=='map';
    $('#cfPlugins').hidden=tab!=='plugins';
    // One record detail, shown beside whichever list picked it.
    const detail=$('#cfTitle').parentElement;
    const host= tab==='plugins'? $('#cfPDetail') : $('#cfRecords');
    if(detail.parentElement!==host) host.appendChild(detail);
    if(tab==='plugins') this.drawTree();
    $('#cfText').hidden=true;
    $('#cfRecBar').hidden=tab==='files';
    $('#cfFileBar').hidden=tab!=='files';
    if(tab==='map') this.drawMap();
  },

  subset(){ return (this.extra().subset||[]).map(String); },

  async scan(){
    const btn=$('#cfScan'); btn.disabled=true;
    this.mode=$('#cfMode').value;
    $('#cfStats').textContent='Reading every plugin...';
    try{
      const got=await Engine.call('conflicts_scan',{cfg:this.cfg(), subset:this.subset(), mode:this.mode, exclude:this.words('#cfExclude')});
      this.list=got.list; this.stats=got.stats; this.cells=got.cells||[]; this.sel=null;
      const s=got.stats;
      $('#cfStats').textContent=`${s.scanned} plugin(s), ${s.records} record(s): ${s.found} ${this.mode==='conflicts'?'conflicting':'single'}`
        +(s.unreadable.length? `; ${s.unreadable.length} not found` : '')+(s.excluded.length? `; ${s.excluded.length} left out` : '');
      const plugins=new Set(); for(const c of this.cells) for(const p of c.plugins) plugins.add(p);
      $('#cfFocus').innerHTML='<option value="">Every plugin</option>'+[...plugins].sort().map(p=>`<option>${loEsc(p)}</option>`).join('');
      this.focus='';
      this.branches=[]; this.judged={}; this.openP={}; this.openT={}; this.psel='';
      this.draw();
      await this.loadBranches();
      if(this.tab==='map') this.drawMap();
    }catch(e){ $('#cfStats').textContent=''; toast(String(e.message||e),'err',6000); }
    finally{ btn.disabled=false; }
  },

  async scanFiles(){
    const btn=$('#cfScanFiles'); btn.disabled=true;
    $('#cfFStats').textContent='Walking the data folders...';
    try{
      const got=await Engine.call('conflicts_resources',{cfg:this.cfg(), subsetDirs:this.subsetDirs(), excludeExts:this.words('#cfExts'),
        compare:$('#cfCompare').checked, meshes:$('#cfMeshes').checked, meshLimit:0, planFile:this.extra().loadorderPlan||''});
      this.files=got.list; this.fstats=got.stats; this.mstats=got.meshStats; this.fsel=null;
      const s=got.stats, m=got.meshStats;
      $('#cfFStats').textContent=`${s.dirs} folder(s), ${s.files} file(s): ${s.conflicts} conflicting, ${s.differing} differ`
        +(m.analysed? `; ${m.analysed} mesh(es) read, ${m.findings} lose something` : '');
      this.draw();
    }catch(e){ $('#cfFStats').textContent=''; toast(String(e.message||e),'err',6000); }
    finally{ btn.disabled=false; }
  },

  subsetDirs(){ return (this.extra().subsetDirs||[]).map(String); },

  match(text){ const q=$('#cfFind').value.trim().toLowerCase(); return !q || text.toLowerCase().includes(q); },

  draw(){
    const mine=$('#cfMine').checked;
    const recs=this.list.filter(c=>(!mine||c.involvesSubset) && this.match(c.type+' '+c.id+' '+c.plugins.join(' ')));
    const box=$('#cfList');
    $('#cfCount').textContent=this.list.length? recs.length+' of '+this.list.length : '';
    box.innerHTML='';
    const shown=recs.slice(0,5000);
    for(const c of shown){
      const d=document.createElement('div');
      d.className='r'+(c.involvesSubset?' mine':'')+(this.sel===c?' sel':'');
      d.innerHTML=`<span class="t">[${loEsc(c.type)}]</span> ${loEsc(c.id)} <span class="w">- ${c.plugins.length} plugin(s), wins: ${loEsc(c.winner)}</span>`
        +(c.deletedBy.length? ' <span class="note">deleted</span>' : '');
      d.title=c.plugins.join('  ->  ');
      d.onclick=()=>{ this.sel=c; for(const x of box.querySelectorAll('.sel')) x.classList.remove('sel'); d.classList.add('sel'); this.fields(c); };
      box.appendChild(d);
    }
    if(recs.length>shown.length) box.insertAdjacentHTML('beforeend', `<div class="empty">... and ${recs.length-shown.length} more: narrow it with Find.</div>`);
    if(!this.list.length) box.innerHTML='<div class="empty">Scan to see the records.</div>';

    const files=this.files.filter(f=>(!mine||f.involvesSubset) && this.match(f.path+' '+f.providers.join(' ')));
    const fb=$('#cfFList');
    $('#cfFCount').textContent=this.files.length? files.length+' of '+this.files.length : '';
    fb.innerHTML='';
    for(const f of files.slice(0,5000)){
      const d=document.createElement('div');
      d.className='r'+(f.involvesSubset?' mine':'')+(this.fsel===f?' sel':'');
      d.innerHTML=`${loEsc(f.path)} <span class="w">- ${f.providers.length}, wins: ${loEsc(this.base(f.winner))}</span>`
        +(f.identical? ' <span class="same">[identical]</span>' : '')+(f.mesh? ` <span class="note">${loEsc(f.mesh)}</span>` : '');
      d.title=f.providers.join('  ->  ');
      d.onclick=()=>{ this.fsel=f; for(const x of fb.querySelectorAll('.sel')) x.classList.remove('sel'); d.classList.add('sel'); this.file(f); };
      fb.appendChild(d);
    }
    if(!this.files.length) fb.innerHTML='<div class="empty">Scan to see the files.</div>';
  },

  base(p){ return String(p).replace(/[\\/]+$/,'').split(/[\\/]/).pop(); },

  /* ---- one record ---- */
  async fields(c){
    $('#cfTitle').textContent=`[${c.type}] ${c.id}`;
    const box=$('#cfFields'); box.innerHTML='<div class="empty">Reading...</div>';
    this.drawActs(c, null);
    try{
      const d=await Engine.call('conflicts_fields',{cfg:this.cfg(), rtype:c.type, id:c.id});
      this.drawActs(c, d);
      const head='<tr><th>Field</th>'+d.plugins.map((p,i)=>`<th class="${this.STATUS[d.this[i]]||''}" title="${loEsc(d.this[i])}">${loEsc(p)}${d.read[i]?'':' (not read)'}</th>`).join('')+'</tr>';
      const rows=d.fields.map(f=>{
        const cells=f.values.map((v,i)=>`<td class="${f.differs? (this.STATUS[f.this[i]]||'') : ''}" title="${loEsc(f.this[i])}">${f.present[i]? loEsc(this.text(v)) : '<span class="empty">-</span>'}</td>`).join('');
        return `<tr class="${f.differs?'':'same'}" data-f="${loEsc(f.name)}"><td class="name" title="${loEsc(this.ALL[f.all]||'')}">${loEsc(f.name)}</td>${cells}</tr>`;
      }).join('');
      box.innerHTML=`<div class="empty">${loEsc(this.ALL[d.all]||'')} - ${d.fields.filter(f=>f.differs).length} of ${d.fields.length} field(s) differ</div><table>${head}${rows}</table>`;
      for(const tr of box.querySelectorAll('tr[data-f]')) tr.onclick=()=>{ $('#cfField').value=tr.dataset.f; };
      this._diff=d;
    }catch(e){ box.innerHTML=`<div class="empty">${loEsc(String(e.message||e))}</div>`; }
  },

  text(v){ return v==null? 'null' : typeof v==='string'? v : JSON.stringify(v); },

  /* ---- a LAND record's terrain, and the merge strategies (landcore::preview) ---- */
  async terrain(c){
    const box=$('#cfFields'); box.innerHTML='<div class="empty">Reading each plugin\'s terrain...</div>';
    let d;
    try{ d=await Engine.call('conflicts_land_surfaces',{cfg:this.cfg(), id:c.id}); }
    catch(e){ box.innerHTML=`<div class="empty">${loEsc(String(e.message||e))}</div>`; return; }
    const surfaces=[...d.plugins.map(p=>({label:p.name, h:p.heights})), ...d.strategies.map(s=>({label:'Merged: '+s.name, h:s.heights, merged:true}))];
    let shown=surfaces.length-d.strategies.length, against=-1;
    box.innerHTML=`<div class="acts" id="cfTerChips"></div>
      <div class="acts"><label>Difference against <select id="cfTerAgainst"><option value="-1">nothing (shade the terrain)</option>${surfaces.map((s,i)=>`<option value="${i}">${loEsc(s.label)}</option>`).join('')}</select></label><span id="cfTerStats" class="empty"></span></div>
      <div style="padding:8px"><canvas id="cfTerCanvas" width="390" height="390" style="width:390px;height:390px;image-rendering:pixelated;border:1px solid var(--line)"></canvas></div>
      <div class="empty">Shading follows slope rather than height, which reads as terrain; north is up. A difference is red where the shown surface is higher, blue where lower. Strategy choice only matters where two or more plugins move the same vertex.</div>
      <div class="acts"><label>Choose the strategy in Merge settings for <select id="cfTerMerge"><option value="">a plugin...</option>${d.plugins.map(p=>`<option>${loEsc(p.name)}</option>`).join('')}</select></label></div>`;
    // the strategy is a plugin's .mergedlands.toml (the Tools page's Merge settings), which
    // Merge Lands reads: the comparison above is what that choice does to this cell
    $('#cfTerMerge').onchange=async e=>{
      const name=e.target.value; e.target.value=''; if(!name || typeof WgToolsPage!=='object') return;
      try{
        let r=await Engine.call('tools_locate',{cfg:this.cfg(), names:[name]}); if(typeof r==='string') r=JSON.parse(r);
        const path=r[0] && r[0][1]; if(!path) return toast(name+' was not found in the data folders','warn',4000);
        WgToolsPage.enter(); WgToolsPage.show('mergesettings'); WgToolsPage.mergeOpen(String(path));
      }catch(err){ toast(String(err.message||err),'err',5000); }
    };
    const draw=()=>{
      const chips=$('#cfTerChips');
      chips.innerHTML=surfaces.map((s,i)=>`<span class="chip${i===shown?' on':''}" data-i="${i}" style="cursor:pointer;border:1px solid var(--line2);border-radius:10px;padding:2px 8px;${i===shown?'background:var(--acd)':''}">${loEsc(s.label)}</span>`).join(' ');
      for(const ch of chips.querySelectorAll('[data-i]')) ch.onclick=()=>{ shown=+ch.dataset.i; draw(); };
      const h=surfaces[shown].h, base=against>=0? surfaces[against].h : null;
      const cv=$('#cfTerCanvas'), g=cv.getContext('2d'), N=65, px=6;
      const img=g.createImageData(N*px, N*px);
      let lo=Infinity, hi=-Infinity, moved=0, worst=0;
      for(const v of h){ lo=Math.min(lo,v); hi=Math.max(hi,v); }
      if(base) for(let i=0;i<h.length;i++){ const dv=h[i]-base[i]; if(dv){ moved++; worst=Math.max(worst,Math.abs(dv)); } }
      for(let y=0;y<N;y++) for(let x=0;x<N;x++){
        const i=y*N+x, row=N-1-y;
        let r,gg,b;
        if(base){
          const dv=h[i]-base[i], t=worst? Math.min(1,Math.abs(dv)/worst) : 0;
          if(dv>0){ r=255; gg=b=Math.round(255*(1-t)); } else if(dv<0){ b=255; r=gg=Math.round(255*(1-t)); } else { r=gg=b=200; }
        }else{
          const e=h[y*N+Math.min(N-1,x+1)]-h[i], n=h[Math.min(N-1,y+1)*N+x]-h[i];
          const shade=Math.max(0,Math.min(1,0.65-(e*0.6+n*0.6)/256));
          const tone=(h[i]-lo)/Math.max(1,hi-lo);
          r=Math.round((90+80*tone)*shade+30); gg=Math.round((110+70*tone)*shade+30); b=Math.round((70+50*tone)*shade+30);
          if(h[i]<0){ r=Math.round(r*0.5); gg=Math.round(gg*0.6); b=Math.min(255,b+90); }
        }
        for(let dy=0;dy<px;dy++) for(let dx=0;dx<px;dx++){
          const o=((row*px+dy)*N*px+(x*px+dx))*4; img.data[o]=r; img.data[o+1]=gg; img.data[o+2]=b; img.data[o+3]=255;
        }
      }
      g.putImageData(img,0,0);
      $('#cfTerStats').textContent=`${surfaces[shown].label}: ${lo} to ${hi} units`+(base? `; ${moved} vertex/vertices differ from ${surfaces[against].label}, by up to ${worst}` : '');
    };
    $('#cfTerAgainst').onchange=e=>{ against=+e.target.value; draw(); };
    draw();
  },

  drawActs(c, d){
    const acts=$('#cfActs');
    if(c.plugins.length<2){ acts.innerHTML='<span class="empty">Only one plugin defines this record: nothing to patch.</span>'; return; }
    const opts=c.plugins.map((p,i)=>`<option${i===c.plugins.length-1?' selected':''}>${loEsc(p)}</option>`).join('');
    acts.innerHTML=`<label>From <select id="cfFrom">${opts}</select></label>
      <button class="btn dim" id="cfTakeRec" title="The patch carries this plugin's whole record and loads last; your mods are not modified">Add record to patch</button>
      <input type="text" id="cfField" placeholder="field (click a row)" style="width:180px">
      <button class="btn dim" id="cfTakeField" title="Take this field from the chosen plugin, keeping the rest of the record as it is now">Take field</button>
      <button class="btn dim" id="cfDefine" title="Type a value for the field; it is written to the patch as typed">Define value...</button>`;
    if(c.type==='Landscape'){
      acts.insertAdjacentHTML('beforeend', '<button class="btn dim" id="cfTerrain" title="Each plugin\'s terrain for this cell, and what Overwrite, Resolve, Ignore and Curvature would make of it - preview here, then set the strategy in the plugin\'s .mergedlands.toml with Merge settings">Terrain and strategies...</button>');
      $('#cfTerrain').onclick=()=>this.terrain(c);
    }
    $('#cfTakeRec').onclick=()=>this.patch('record', c, $('#cfFrom').value, '', '');
    $('#cfTakeField').onclick=()=>{ const f=$('#cfField').value.trim(); if(!f) return toast('Pick a field first','warn',2500); this.patch('field', c, $('#cfFrom').value, f, ''); };
    $('#cfDefine').onclick=()=>this.define(c);
  },

  define(c){
    const f=$('#cfField').value.trim(); if(!f) return toast('Pick a field first','warn',2500);
    if(['type','id','grid','data.grid'].includes(f)) return toast(f+' says which record this is: a typed value would make a different record','warn',4000);
    const d=this._diff, row=d && d.fields.find(x=>x.name===f);
    const cur=row && row.present[row.present.length-1] ? row.values[row.values.length-1] : null;
    const seed=cur==null? '' : typeof cur==='string'? cur : JSON.stringify(cur);
    const typed=window.prompt(`A value for ${f} (as JSON, or plain text for a string)`, seed);
    if(typed==null) return;
    let val; try{ val=JSON.parse(typed); }catch(_){ val=typed; }
    if(typeof cur==='string' && typeof val!=='string') val=typed;
    this.patch('value', c, '', f, JSON.stringify(val));
  },

  async patch(op, c, plugin, field, value){
    const journal=String((this.extra().editor||{}).journal||'');
    try{
      const n=await Engine.call('conflicts_patch',{journal, op, rtype:c.type, id:c.id, plugin, field, value});
      toast(`Queued for the patch (${n} record(s) queued)`,'ok',3000);
    }catch(e){ toast(String(e.message||e),'err',6000); }
  },

  /* ---- by plugin (pluginview.py) ---- */
  async loadBranches(){
    try{ this.branches=await Engine.call('conflicts_by_plugin',{cfg:this.cfg(), subset:this.subset()}); }
    catch(e){ this.branches=[]; toast(String(e.message||e),'err',5000); }
    if(this.tab==='plugins') this.drawTree();
  },

  /* What one plugin's records say, judged once: `{records, tally, worst, byType}`. */
  async judge(plugin){
    if(this.judged[plugin]) return this.judged[plugin];
    const got=await Engine.call('conflicts_plugin_group',{cfg:this.cfg(), plugin, rtype:''});
    const byType={};
    const rank=s=>({'conflict, loses':5,'conflict, wins':4,'override, wins':3,'master':2,'identical to master':1}[s]||0);
    for(const r of got.records){ const t=byType[r.type]; if(!t || rank(r.this)>rank(t)) byType[r.type]=r.this; }
    got.byType=byType;
    this.judged[plugin]=got;
    return got;
  },

  async judgeAll(){
    const btn=$('#cfJudgeAll'); btn.disabled=true;
    try{
      let n=0;
      for(const b of this.branches){
        if(!this.on) break;
        $('#cfPCount').textContent=`judging ${++n} of ${this.branches.length}...`;
        try{ await this.judge(b.plugin); }catch(_){}
        if(n%5===0) this.drawTree();
      }
    }finally{ btn.disabled=false; this.drawTree(); }
  },

  tallyText(t){
    const lose=t['conflict, loses']||0, itm=t['identical to master']||0;
    const parts=[]; if(lose) parts.push(`loses ${lose}`); if(itm) parts.push(`${itm} unchanged`);
    return parts.join(', ');
  },

  drawTree(){
    const box=$('#cfPTree'); if(!box) return;
    const mine=$('#cfMine').checked;
    const q=$('#cfFind').value.trim().toLowerCase();
    const shown=this.branches.filter(b=>(!mine||b.mine) && (!q || b.plugin.toLowerCase().includes(q)));
    $('#cfPCount').textContent=this.branches.length? `${shown.length} of ${this.branches.length}` : '';
    if(!this.branches.length){ box.innerHTML=`<div class="empty">${this.list.length? 'Reading...' : 'Scan to see the plugins.'}</div>`; return; }
    const out=[];
    for(const b of shown){
      const j=this.judged[b.plugin], open=!!this.openP[b.plugin];
      const cls=j? (this.STATUS[j.worst]||'') : '';
      const note=j? this.tallyText(j.tally) : '';
      out.push(`<div class="r${b.mine?' mine':''}${this.psel===b.plugin?' sel':''}" data-p="${loEsc(b.plugin)}"><span class="tw">${open?'-':'+'}</span><span class="${cls}" title="${loEsc(j? j.worst : 'not judged yet: open it')}">${loEsc(b.plugin)}</span> <span class="n">${b.records}</span>${note? ` <span class="note">${loEsc(note)}</span>` : ''}</div>`);
      if(!open) continue;
      for(const g of b.groups){
        const key=b.plugin+'\u0000'+g.type, topen=!!this.openT[key];
        const tc=j? (this.STATUS[j.byType[g.type]]||'') : '';
        out.push(`<div class="r k" data-p="${loEsc(b.plugin)}" data-t="${loEsc(g.type)}"><span class="tw">${topen?'-':'+'}</span><span class="${tc}">${loEsc(g.type)}</span> <span class="n">${g.count}</span></div>`);
        if(!topen || !j) continue;
        const recs=j.records.filter(r=>r.type===g.type);
        for(const r of recs.slice(0,3000)){
          out.push(`<div class="r i${this.sel && this.sel.type===r.type && this.sel.id===r.id?' sel':''}" data-p="${loEsc(b.plugin)}" data-t="${loEsc(r.type)}" data-i="${loEsc(r.id)}" title="${loEsc(r.this+' - '+this.ALL[r.all]+'\n'+r.plugins.join('  ->  '))}"><span class="${this.STATUS[r.this]||''}">${loEsc(r.id)}</span> <span class="w">wins: ${loEsc(r.winner)}</span></div>`);
        }
        if(recs.length>3000) out.push(`<div class="r i empty">... and ${recs.length-3000} more</div>`);
      }
    }
    box.innerHTML=out.join('');
    for(const d of box.querySelectorAll('div.r[data-p]')) d.onclick=()=>this.treeClick(d.dataset.p, d.dataset.t, d.dataset.i);
  },

  async treeClick(plugin, type, id){
    if(id!=null){
      const c=this.list.find(x=>x.type===type && x.id===id);
      if(c){ this.sel=c; this.fields(c); this.drawTree(); }
      return;
    }
    this.psel=plugin; this.peers(plugin);
    if(type!=null){
      const key=plugin+'\u0000'+type;
      this.openT[key]=!this.openT[key];
    } else {
      this.openP[plugin]=!this.openP[plugin];
    }
    if(!this.judged[plugin]){
      this.drawTree();
      $('#cfPCount').textContent='judging '+plugin+'...';
      try{ await this.judge(plugin); }catch(e){ toast(String(e.message||e),'err',5000); }
    }
    this.drawTree();
  },

  /* The plugins this one shares records with, and those whose edits it overrides (`_highlight_conflicts`). */
  peers(plugin){
    const shares=new Map(), beats=new Set(), beaten=new Set();
    for(const c of this.list){
      const at=c.plugins.indexOf(plugin); if(at<0) continue;
      c.plugins.forEach((p,i)=>{ if(p===plugin) return; shares.set(p,(shares.get(p)||0)+1); if(c.winner===plugin && i<at) beats.add(p); if(c.winner===p) beaten.add(p); });
    }
    const top=[...shares.entries()].sort((a,b)=>b[1]-a[1]).slice(0,12);
    $('#cfPeers').innerHTML=top.length? `${loEsc(plugin)} shares records with `+top.map(([p,n])=>`<span class="${beaten.has(p)?'peer':''}" title="${beaten.has(p)?'wins some of the records they share':''}">${loEsc(p)} (${n})</span>`).join(', ')
      +(shares.size>top.length? ` and ${shares.size-top.length} more` : '')+(beaten.size? '. Highlighted: those that win records over it.' : '.') : '';
  },

  async bulk(){
    const plugin=this.psel;
    if(!plugin) return toast('Select a plugin in the tree first','warn',3000);
    const pane=$('#cfFields'); const title=$('#cfTitle');
    title.textContent='Merge fields from '+plugin; $('#cfActs').innerHTML='';
    pane.innerHTML='<div class="empty">Reading every record it shares...</div>';
    let cands;
    try{ cands=await Engine.call('conflicts_plugin_bulk',{cfg:this.cfg(), plugin}); }
    catch(e){ pane.innerHTML=`<div class="empty">${loEsc(String(e.message||e))}</div>`; return; }
    if(!cands.length){ pane.innerHTML=`<div class="empty">${loEsc(plugin)} either already wins, or matches the winner, for every field - so there is nothing to take.</div>`; return; }
    const val=v=>{ const t=this.text(v); return t.length>120? t.slice(0,117)+'...' : t; };
    pane.innerHTML=`<div class="empty">Pick the fields ${loEsc(plugin)} should win, across every record it defines. The number is how many records each field would change. Your mods are not modified.</div>
      <div class="acts"><button class="btn dim" id="cfBulkAll">Select all</button><button class="btn" id="cfBulkGo">Merge selected</button><span id="cfBulkN" class="empty"></span></div>
      <table class="bulk"><tr><th></th><th>Field</th><th>Records</th><th>This plugin's value</th></tr>${cands.map(c=>`<tr><td><input type="checkbox" data-path="${loEsc(c.path)}"></td><td class="name">${loEsc(c.path)}</td><td class="c">${c.count}</td><td>${loEsc(val(c.value))}</td></tr>`).join('')}</table>`;
    const picked=()=>[...pane.querySelectorAll('input[data-path]:checked')].map(i=>i.dataset.path);
    const recount=async()=>{
      const paths=picked();
      const n=paths.length? await Engine.call('conflicts_plugin_bulk_apply',{journal:'', plugin, paths, dry:true}).catch(()=>0) : 0;
      $('#cfBulkN').textContent=`${paths.length} field(s) selected, ${n} record change(s) queued.`;
    };
    for(const i of pane.querySelectorAll('input[data-path]')) i.onchange=recount;
    $('#cfBulkAll').onclick=()=>{ for(const i of pane.querySelectorAll('input[data-path]')) i.checked=true; recount(); };
    $('#cfBulkGo').onclick=async()=>{
      const paths=picked(); if(!paths.length) return;
      const journal=String((this.extra().editor||{}).journal||'');
      try{
        const n=await Engine.call('conflicts_plugin_bulk_apply',{journal, plugin, paths, dry:false});
        toast(`Queued ${n} field take(s) from ${plugin}`,'ok',4000);
      }catch(e){ toast(String(e.message||e),'err',6000); }
    };
    recount();
  },

  /* ---- one file ---- */
  file(f){
    $('#cfFTitle').textContent=f.path;
    const box=$('#cfFDetail');
    const prov=f.providers.map((p,i)=>`<div class="r">${i+1}. ${loEsc(p)}${p===f.winner?' <span class="note">(wins)</span>':''}</div>`).join('');
    const tex=/\.(dds|tga|bmp|png)$/i.test(f.path) && f.providers.length>1;
    const mesh=f.meshColumns? `<pre>${loEsc(f.mesh||'Nothing lost against the meshes it overrides.')}${f.meshColumns[3]? '\nTriangles against the one it replaced: '+f.meshColumns[3] : ''}</pre>` : '';
    box.innerHTML=`<div class="rows">${prov}</div>${mesh}${f.identical?'<div class="empty">Every provider holds the same bytes: nothing to decide.</div>':''}`
      +(tex? '<div class="acts"><button class="btn dim" id="cfCompareTex">Compare with what it replaced</button></div><div id="cfTex"></div>' : '');
    if(tex) $('#cfCompareTex').onclick=()=>this.texture(f);
  },

  async texture(f){
    const out=$('#cfTex'); out.innerHTML='<div class="empty">Reading...</div>';
    try{
      const t=await Engine.call('conflicts_texture',{path:f.path, providers:f.providers, max:1024});
      out.innerHTML=`<div class="empty">${loEsc(t.verdict)}: ${loEsc(t.detail)}</div><div class="imgs"></div>`;
      const imgs=out.querySelector('.imgs');
      const fig=(label, img)=>{
        const fg=document.createElement('figure');
        if(img && img.rgba){
          const cv=document.createElement('canvas'); cv.width=img.w; cv.height=img.h;
          const bytes=Uint8ClampedArray.from(atob(img.rgba), ch=>ch.charCodeAt(0));
          cv.getContext('2d').putImageData(new ImageData(bytes, img.w, img.h), 0, 0);
          fg.appendChild(cv);
        }
        const cap=document.createElement('figcaption'); cap.textContent=label+(img && img.error? ' - '+img.error : ''); fg.appendChild(cap);
        imgs.appendChild(fg);
      };
      fig(t.left.label, t.left); fig(t.right.label+' (wins)', t.right);
      if(t.diff) fig('Where they differ (amplified)', t.diff);
    }catch(e){ out.innerHTML=`<div class="empty">${loEsc(String(e.message||e))}</div>`; }
  },

  /* ---- the map ---- */
  drawMap(){
    const cv=$('#cfMap'), wrap=$('#cfMapWrap');
    if(!cv || !wrap) return;
    const W=wrap.clientWidth, H=wrap.clientHeight;
    cv.width=W; cv.height=H;
    const g=cv.getContext('2d');
    g.clearRect(0,0,W,H);
    const cells=this.cells;
    const note=$('#cfMapNote');
    if(!cells.length){ note.textContent=this.list.length? 'No conflict carries a grid (objects, dialogue and interiors are in the list).' : 'Scan for conflicting records to see the map.'; return; }
    const count=c=> this.focus? Object.values(c.byPlugin[this.focus]||{}).reduce((a,b)=>a+b,0) : c.total;
    let x0=Infinity,x1=-Infinity,y0=Infinity,y1=-Infinity,worst=1;
    for(const c of cells){ x0=Math.min(x0,c.x); x1=Math.max(x1,c.x); y0=Math.min(y0,c.y); y1=Math.max(y1,c.y); worst=Math.max(worst,count(c)); }
    const px=Math.max(2, Math.floor(Math.min((W-20)/(x1-x0+1), (H-20)/(y1-y0+1))));
    const ox=Math.floor((W-px*(x1-x0+1))/2), oy=Math.floor((H-px*(y1-y0+1))/2);
    const css=getComputedStyle(document.body);
    g.fillStyle=css.getPropertyValue('--bg3')||'#333';
    g.fillRect(ox, oy, px*(x1-x0+1), px*(y1-y0+1));
    for(const c of cells){
      const n=count(c);
      const t=Math.log(1+n)/Math.log(1+worst);
      g.fillStyle = n? `hsl(${Math.round(55-55*t)} 85% ${Math.round(60-25*t)}%)` : 'rgba(128,128,128,.25)';
      g.fillRect(ox+(c.x-x0)*px, oy+(y1-c.y)*px, px-(px>4?1:0), px-(px>4?1:0));
      if(c.mine && !this.focus){ g.strokeStyle=css.getPropertyValue('--ac')||'#fa0'; g.strokeRect(ox+(c.x-x0)*px+.5, oy+(y1-c.y)*px+.5, px-1, px-1); }
    }
    note.textContent=`${cells.length} cell(s) with conflicts, at most ${worst} in one`+(this.focus? ` - ${this.focus} only` : '')+'; outlined: your mods take part.';
    const tip=$('#cfTip');
    cv.onmousemove=e=>{
      const r=cv.getBoundingClientRect();
      const gx=Math.floor((e.clientX-r.left-ox)/px)+x0, gy=y1-Math.floor((e.clientY-r.top-oy)/px);
      const c=cells.find(k=>k.x===gx && k.y===gy);
      if(!c){ tip.style.display='none'; return; }
      const types=Object.entries(c.types).map(([k,v])=>`${k}: ${v}`).join(', ');
      tip.textContent=`(${c.x}, ${c.y}): ${c.total} conflicting record(s)\n${types}\n${c.plugins.join('\n')}`;
      tip.style.left=(e.clientX+14)+'px'; tip.style.top=(e.clientY+14)+'px'; tip.style.display='block';
    };
    cv.onmouseleave=()=>{ tip.style.display='none'; };
    cv.onclick=e=>{
      const r=cv.getBoundingClientRect();
      const gx=Math.floor((e.clientX-r.left-ox)/px)+x0, gy=y1-Math.floor((e.clientY-r.top-oy)/px);
      $('#cfFind').value=`(${gx}, ${gy})`; this.show('records'); this.draw();
    };
  },

  /* ---- reports ---- */
  async report(){
    const kind=this.tab==='files'? 'files' : 'records';
    try{
      const text=await Engine.call('conflicts_report',{kind, subsetOnly:$('#cfMine').checked, limit:0});
      $('#cfTextBody').textContent=text;
      for(const id of ['#cfRecords','#cfFiles','#cfMapPane','#cfPlugins']) $(id).hidden=true;
      $('#cfText').hidden=false;
    }catch(e){ toast(String(e.message||e),'err',5000); }
  },

  async csv(){
    const kind=this.tab==='files'? 'files' : 'records';
    const path=await Engine.pick('save',{defaultPath: kind==='files'? 'resource_conflicts.csv' : 'conflicts.csv'});
    if(!path) return;
    try{ await Engine.call('conflicts_csv',{kind, path:String(path)}); toast('Saved '+path,'ok',3000); }
    catch(e){ toast(String(e.message||e),'err',5000); }
  },
};
