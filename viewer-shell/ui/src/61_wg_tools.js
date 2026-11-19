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
   - tes3cmd (`tools_tes3cmd`, toolscore::tes3cmd): tes3cmd built in (tes3cmd-rs), or an
     external one when "Use external" is ticked, run on each plugin staged with its masters
     (clean, custom) or in place (header); master sizes synced in-app. A command running Perl
     code (modify --run and the like) goes to an external tes3cmd when there is one.
   - Merge objects (`tools_merge_objects`, editcore::mergeobjects): Merged Objects.esp, the records
     two or more plugins change merged record by record (a port of NullCascade's TES3Merge), or
     only the leveled lists into multipatch.esp.
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
    ['mergeobjects','Merge objects','Merged Objects.esp: every mod\'s changes to the same records kept, not only the last one\'s (TES3Merge)'],
    ['mergesettings','Merge settings','A plugin\'s .mergedlands.toml: which of its land layers Merge Lands takes, and how'],
    ['m2m','Merge to master','A plugin folded into its master, or a whole load order into one master (merge_to_master)'],
    ['meshopt','Mesh optimizer','Draw-order copies of the load order\'s meshes, only where it helps'],
    ['meshcheck','Check meshes','The load order\'s meshes checked for the faults exporters write, and repaired copies of those that can be mended']],
  t3:{files:[], res:null},
  m2:{plugin:'', master:'', files:[]},
  jr:{data:null, quest:null},
  ms:{path:'', list:[]},
  mg:{plugin:'', data:null},
  ml:{home:null, running:false},
  mo:{rows:[], stats:null, shown:0, running:false},
  mc:{rows:[], head:null, running:false},
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
    const t=this.extra().tool; if(t){ this.show(t); this.extra().tool=''; } else this.show(this.tool);
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
#wgTL table.mo{border-collapse:collapse;width:100%;font-family:var(--mono);font-size:11px}
#wgTL table.mo th,#wgTL table.mo td{padding:2px 6px;text-align:right;white-space:nowrap;border-bottom:1px solid var(--line)}
#wgTL table.mo th{color:var(--tx3);font-weight:normal;position:sticky;top:0;background:var(--bg2)}
#wgTL table.mo td.p,#wgTL table.mo th.p{text-align:left;white-space:normal;word-break:break-all}
#wgTL table.mo tr.no td{color:var(--tx3)}
#wgTL .pane[hidden]{display:none}
#wgTL .acts[hidden]{display:none !important}`;
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
    <label title="Run an external tes3cmd (the MOMW Tools Pack's tes3cmd.exe, or the Perl script) instead of the one built in. Commands that run Perl code (modify --run, --program-file, a /.../e replacement) use it either way."><input type="checkbox" id="tlT3Ext"> Use external</label>
    <input type="text" id="tlT3Exe" placeholder="external tes3cmd (found by itself when empty)" style="flex:1;min-width:200px"><button class="btn dim" id="tlT3Browse">Browse...</button>
    <button class="btn dim" id="tlT3Add" title="Add plugin files">Add files...</button><button class="btn dim" id="tlT3Mine" title="Your own mods: the ones the sort places into the curated list">Add your mods</button><button class="btn dim" id="tlT3List" title="The curated list's plugins (the Load order page's list name)">Add list mods</button><button class="btn dim" id="tlT3Clear">Clear</button></div>
  <div class="scroll"><div id="tlT3Files" class="empty">Add the plugins to run on. clean and a custom command run on a copy of each plugin, its masters read from your data folders and never changed, and copy back what changed (a one-time .preclean.bak / .precustom.bak kept); fixit, multipatch and resetdates run once over all the plugins added, common and diff on two; Morrowind.esm, Tribunal.esm and Bloodmoon.esm are never cleaned. sync rewrites only the master sizes (.masterfix.bak kept).</div>
  <div id="tlT3Res" hidden style="border-top:1px solid var(--line)">
    <div class="acts" style="display:flex;gap:6px;padding:6px 8px;align-items:center;flex-wrap:wrap">
      <b>What changed</b><span id="tlT3ResN" class="sp" style="color:var(--tx3)"></span>
      <input type="text" id="tlT3ResFind" placeholder="filter: type, id, plugin, subrecord" style="flex:1;min-width:160px">
      <select id="tlT3ResFile" title="One plugin's changes, or every plugin's"></select>
      <span id="tlT3ResKinds"></span>
      <button class="btn dim" id="tlT3ResAll" title="Tick every change the filter shows">Select shown</button>
      <button class="btn" id="tlT3ResUndo" title="Put the ticked records back as they were before the run: a removed record restored where it was, a changed one as it was, an added one taken out">Undo selected</button></div>
    <div id="tlT3ResBody" style="max-height:340px;overflow:auto"></div></div>
  <details id="tlT3Log" open><summary style="padding:4px 8px;cursor:pointer">Log</summary>
  <pre id="tlT3Out" style="white-space:pre-wrap;font-family:var(--mono);margin:0;padding:6px 8px"></pre></details></div></div>
<div class="col pane" id="tlLands" hidden><h3>Merge Lands <span class="sp"></span>
  <label title="Report every plugin's settings and everything that could not be read"><input type="checkbox" id="tlMlVerbose"> Verbose</label>
  <label title="Report what would be merged without writing anything"><input type="checkbox" id="tlMlDry"> Dry run</label>
  <label title="Stop, writing nothing, if a height step is left that a record cannot store and the slope limiter could not repair (off: the step is clamped when saved, and the log says where)"><input type="checkbox" id="tlMlNoClamp"> Stop instead of clamping</label>
  <label title="Write the world maps (WNAM) as the merge has them, as OpenMWMergedLands does (off: an entry is made again where a repair moved the height under it, and a cell with no map gets the one OpenMW generates)"><input type="checkbox" id="tlMlKeepWnam"> Keep world maps as merged</label>
  <select id="tlMlModel" title="What a plugin's change is measured against"><option value="loadorder">Against the load order (follow the winners, fix the seams)</option><option value="masters">Against the masters (keep every mod's edits, blend where two meet)</option></select>
  <button class="btn" id="tlMlRun">Build Merged Lands.esp</button><button class="btn dim" id="tlMlCancel" hidden title="Stop at the next step; nothing is written">Cancel</button></h3>
  <div class="acts" style="display:flex;gap:6px;padding:6px 8px;border-bottom:1px solid var(--line);align-items:center;flex-wrap:wrap">
    <span>Leave out</span><input type="text" id="tlMlIgnore" placeholder="plugin names or folders, comma separated" style="flex:1;min-width:200px" title="Plugins with nothing to merge that are slow to read (generated patches), by name or by folder. The output's own name is always left out. Empty: none."><button class="btn dim" id="tlMlIgnoreDefault" title="Put back the default list: the generated patches (delta-merged, deleted_groundcover, groundcover, S3LightFixes, OMWLLFMod, merged, Merged Objects)">Default</button>
  </div>
  <div class="acts" style="display:flex;gap:6px;padding:6px 8px;border-bottom:1px solid var(--line);align-items:center;flex-wrap:wrap">
    <label title="Also repair seams the load order already has between neighboring cells that no plugin changed (off: only the borders of cells a plugin changed). A wider gap is taken to be meant."><input type="checkbox" id="tlMlSeams"> Also repair seams the terrain already has, up to</label>
    <input type="number" id="tlMlSeamGap" min="1" max="1024" value="24" style="width:70px" title="The widest gap, in world units, that is repaired (24 is three eight-unit steps; wider ones make steep slopes the format cannot always hold)"><span>units wide, leaving out</span>
    <input type="text" id="tlMlSeamExcept" placeholder="cells as X,Y; X,Y" style="flex:1;min-width:140px" title="Cells whose borders are never repaired, either side of them (a cliff or an edge the load order means to have)">
  </div>
  <div class="acts" style="display:flex;gap:6px;padding:6px 8px;border-bottom:1px solid var(--line);align-items:center;flex-wrap:wrap">
    <span title="Cells the merge leaves alone: not merged, not repaired, not written. X,Y for every plugin, Plugin.esp=X,Y;X,Y for one plugin's terrain there (the others still merged); entries split by |">Leave cells out</span><input type="text" id="tlMlCellIgnore" placeholder="TR_Mainland.esm=-9,-46;-9,-47;-10,-46;-10,-47" style="flex:1;min-width:240px" title="X,Y for every plugin, or Plugin.esp=X,Y;X,Y for one; entries split by |. Empty: none. The default is Tamriel Rebuilt's Hunza Camp (its four cells, as OpenMWMergedLands has it)."><button class="btn dim" id="tlMlCellDefault" title="Put back the default: Tamriel Rebuilt's Hunza Camp">Default</button><button class="btn dim" id="tlMlFork" title="Read OpenMWMergedLands' merged_lands.toml (beside the setup's openmw.cfg, or in OpenMW's config folder) and fill in the plugins and cells it leaves out, and where it writes. Shift+click to pick the file.">Import from OpenMWMergedLands</button>
  </div>
  <div class="acts" style="display:flex;gap:6px;padding:6px 8px;border-bottom:1px solid var(--line);align-items:center;flex-wrap:wrap">
    <span>Write into</span><input type="text" id="tlMlFolder" placeholder="the data folder holding most of the load order" style="flex:1;min-width:240px"><button class="btn dim" id="tlMlBrowse">Browse...</button></div>
  <div class="scroll"><div id="tlMlIntro" class="empty">Where two mods reshape different vertices of the same exterior cell, the load order keeps only the last one's LAND record and discards the other; this recovers both into a new plugin, Merged Lands.esp. Seams between cells are repaired and the terrain is conditioned so every height fits the format. Your mods are never modified, and deleting the merged plugin restores your previous behavior exactly. Load it LAST. Merge settings (a plugin's .mergedlands.toml) decide, per layer, what a mod contributes and how its edits resolve.</div>
  <pre id="tlMlOut" style="white-space:pre-wrap;font-family:var(--mono);margin:0;padding:6px 8px"></pre></div></div>
<div class="col pane" id="tlObj" hidden><h3>Merge objects <span class="sp"></span>
  <label title="Merge only the leveled lists (and fix summons and fogbugged cells), as TES3Merge's multipatch, into multipatch.esp"><input type="checkbox" id="tlObLists"> Leveled lists only</label>
  <label title="Report what would be merged without writing anything"><input type="checkbox" id="tlObDry"> Dry run</label>
  <button class="btn" id="tlObRun">Build Merged Objects.esp</button></h3>
  <div class="acts" style="display:flex;gap:10px;padding:6px 8px;border-bottom:1px solid var(--line);align-items:center;flex-wrap:wrap">
    <span>Fixes</span>
    <label title="Flag the known summoned creatures persistent (a crash when leaving a cell where one was summoned; the Morrowind Code Patch also fixes it)"><input type="checkbox" id="tlObSummons" checked> summons</label>
    <label title="Give interiors with fog density 0 a small one (pitch-black interiors on some video cards)"><input type="checkbox" id="tlObFog" checked> fogbug</label>
    <label title="Keep an exterior cell's new name when a later plugin reverts it to the old one (scripts that name the renamed cell break)"><input type="checkbox" id="tlObNames" checked> cellnames</label>
    <label title="Merge NPC and creature inventories inclusively: every item any plugin gives them (off: the last plugin to change the inventory wins it whole)"><input type="checkbox" id="tlObIncl"> Inclusive inventories</label></div>
  <div class="acts" id="tlObTypes" style="display:flex;gap:4px 10px;padding:6px 8px;border-bottom:1px solid var(--line);align-items:center;flex-wrap:wrap"></div>
  <div class="acts" style="display:flex;gap:6px;padding:6px 8px;border-bottom:1px solid var(--line);align-items:center;flex-wrap:wrap">
    <span>Leave out</span><input type="text" id="tlObIgnore" placeholder="plugin names, comma separated" style="flex:1;min-width:200px" title="Plugins left out of the merge, by name (TES3Merge.ini's [FileFilters])"></div>
  <div class="acts" style="display:flex;gap:6px;padding:6px 8px;border-bottom:1px solid var(--line);align-items:flex-start;flex-wrap:wrap">
    <span title="Which ids are merged, as TES3Merge.ini's [ObjectFilters]: a regular expression over the id in lower case, then = true or = false; the last line that matches decides">Id filters</span>
    <textarea id="tlObFilters" spellcheck="false" rows="2" placeholder="^ttm_ = false" style="flex:1;min-width:200px;font-family:var(--mono)"></textarea></div>
  <div class="acts" style="display:flex;gap:6px;padding:6px 8px;border-bottom:1px solid var(--line);align-items:center;flex-wrap:wrap">
    <span>Write into</span><input type="text" id="tlObFolder" placeholder="the data folder holding most of the load order" style="flex:1;min-width:240px"><button class="btn dim" id="tlObBrowse">Browse...</button></div>
  <div class="scroll"><div id="tlObIntro" class="empty">When two mods change the same record (an NPC, a potion, a leveled list), the game uses only the last one's version, so the other's changes are lost. This merges them record by record into a new plugin, Merged Objects.esp, loaded last: each mod's changes kept where they do not overlap, the later mod's where they do, and leveled lists combined. A port of NullCascade's TES3Merge, for OpenMW, Mod Organizer 2 and the game's own install alike. Your mods are never changed; delete the plugin to undo it. Run it again after changing the load order.</div>
  <pre id="tlObOut" style="white-space:pre-wrap;font-family:var(--mono);margin:0;padding:6px 8px"></pre></div></div>
<div class="col pane" id="tlM2m" hidden><h3>Merge to master <span class="sp"></span>
  <select id="tlM2Mode" title="What is merged"><option value="plugin">A plugin into its master</option><option value="order">Plugins into one master</option></select>
  <label title="Drop the records a plugin deletes instead of carrying the deletion"><input type="checkbox" id="tlM2Del"> Remove deleted</label>
  <label title="Put moved references in the cell they were moved to"><input type="checkbox" id="tlM2Moved"> Apply moved references</label>
  <label title="Keep references that are exact duplicates of one another"><input type="checkbox" id="tlM2Dup"> Keep duplicate references</label>
  <button class="btn" id="tlM2Run">Merge...</button></h3>
  <div class="acts" id="tlM2One" style="display:flex;gap:6px;padding:6px 8px;border-bottom:1px solid var(--line);align-items:center;flex-wrap:wrap">
    <span>Plugin</span><input type="text" id="tlM2Plugin" placeholder="the plugin to fold in" style="flex:1;min-width:200px"><button class="btn dim" id="tlM2PickP">Browse...</button>
    <span>into master</span><input type="text" id="tlM2Master" placeholder="its master" style="flex:1;min-width:200px"><button class="btn dim" id="tlM2PickM">Browse...</button></div>
  <div class="acts" id="tlM2Many" hidden style="display:flex;gap:6px;padding:6px 8px;border-bottom:1px solid var(--line);align-items:center;flex-wrap:wrap">
    <button class="btn dim" id="tlM2Add">Add files...</button><button class="btn dim" id="tlM2Mine">Add your mods</button><button class="btn dim" id="tlM2List">Add list mods</button><button class="btn dim" id="tlM2Clear">Clear</button>
    <span class="empty" style="padding:0">masters first, in load order: later plugins win</span></div>
  <div class="scroll"><div id="tlM2Files" class="empty">greatness7's merge_to_master: the plugin's records written into a copy of its master, as the game would resolve them (or a whole list of plugins into one master). You pick where the merged master is written; the files merged are never changed, and a file the merge replaces is kept once as .premerge.bak.</div>
  <pre id="tlM2Out" style="white-space:pre-wrap;font-family:var(--mono);margin:0;padding:6px 8px"></pre></div></div>
<div class="col pane" id="tlMerge" hidden><h3>Merge settings <span class="sp"></span><select id="tlMgMine" title="One of your mods"><option value="">Your mods...</option></select>
  <button class="btn dim" id="tlMgPick">Pick a plugin...</button><button class="btn" id="tlMgWrite" disabled>Write .mergedlands.toml</button></h3>
  <div class="scroll" style="padding:8px"><div id="tlMgIntro" class="empty" style="padding:0 0 8px">The per-plugin Merge Lands control file, beside the plugin: uncheck a layer to keep the plugin from touching it, set a strategy for how its edits resolve where they collide with another mod's. The defaults behave as if there were no file, and Merge Lands runs without one: write it only when a conflict needs it.</div>
  <div id="tlMgForm"></div>
  <div class="empty" style="padding:8px 0 0">Auto = later plugin wins for entries it changed · Overwrite = this plugin wins collisions · Ignore = keep the earlier edit · Resolve = blend conflicting numbers · Curvature = structure-weighted blend (ours).</div>
  <pre id="tlMgPreview" style="white-space:pre;font-family:var(--mono);margin:8px 0 0;padding:8px;background:var(--bg3);border:1px solid var(--line);border-radius:6px"></pre></div></div>
<div class="col pane" id="tlMo" hidden><h3>Mesh optimizer <span class="sp"></span><span id="tlMoStats" class="empty"></span>
  <button class="btn dim" id="tlMoAll" title="Tick every mesh that qualifies, or none">All</button>
  <button class="btn dim" id="tlMoWrite" disabled title="Write the ticked meshes, optimized, into the output folder">Write copies</button><button class="btn" id="tlMoRun">Analyse</button></h3>
  <div class="acts" style="display:flex;gap:6px;padding:6px 8px;border-bottom:1px solid var(--line);align-items:center;flex-wrap:wrap">
    <label title="A mesh is rewritten only when it draws at least this much better (vertex cache, overdraw or vertex fetch, weighted by triangles)">Least gain <input type="number" id="tlMoGain" value="3" min="0" step="0.5" style="width:56px"> %</label>
    <label title="Meshes with fewer triangles than this are left alone">Least triangles <input type="number" id="tlMoTris" value="64" min="0" style="width:64px"></label>
    <input type="text" id="tlMoFolder" placeholder="only meshes from data folders containing..." style="flex:1;min-width:180px" title="Part of a data folder's path; empty: every mesh the load order provides"></div>
  <div class="acts" style="display:flex;gap:6px;padding:6px 8px;border-bottom:1px solid var(--line);align-items:center;flex-wrap:wrap">
    <span>Write into</span><input type="text" id="tlMoOut" placeholder="a new, empty folder (not one of your mods); add it last in the data= lines" style="flex:1;min-width:240px"><button class="btn dim" id="tlMoBrowse">Browse...</button></div>
  <div class="scroll"><div id="tlMoIntro" class="empty">Puts the load order's meshes in the order a graphics card draws fastest (vertex cache, then overdraw, then vertex fetch), only the meshes where it pays: each shape is moved only if it really draws better, and a mesh only if the gain reaches the threshold above. Analyse measures the winning copy of every mesh, loose or in the load order's archives (a loose file wins, then the later archive); Write copies puts the ticked ones, optimized and checked to be the same mesh, into the folder above, at their own paths. Your mods are not modified; the folder goes last in the load order, or is deleted to undo. What was measured is remembered, so the next run measures only what changed. A mesh with a fault Check meshes proves it can mend (a UV flag left off) is measured and optimized from the repaired copy, and the copy written is repaired too.</div>
  <div id="tlMoBody"></div></div></div>
<div class="col pane" id="tlMc" hidden><h3>Check meshes <span class="sp"></span><span id="tlMcStats" class="empty"></span>
  <button class="btn dim" id="tlMcWrite" disabled title="Write the ticked meshes, repaired, into the output folder">Write repaired</button><button class="btn" id="tlMcRun">Check</button></h3>
  <div class="acts" style="display:flex;gap:6px;padding:6px 8px;border-bottom:1px solid var(--line);align-items:center;flex-wrap:wrap">
    <label title="Also write the meshes that can be mended only by a guess: a node whose lists after its transform do not read is kept with no properties, children or effects. Nothing else in the mesh links to such a node, but what the garbage meant is lost."><input type="checkbox" id="tlMcGuess"> Also write guesses</label>
    <input type="text" id="tlMcFolder" placeholder="only meshes from data folders containing..." style="flex:1;min-width:180px" title="Part of a data folder's path; empty: every mesh the load order provides"></div>
  <div class="acts" style="display:flex;gap:6px;padding:6px 8px;border-bottom:1px solid var(--line);align-items:center;flex-wrap:wrap">
    <span>Write into</span><input type="text" id="tlMcOut" placeholder="a new, empty folder (not one of your mods); add it last in the data= lines" style="flex:1;min-width:240px"><button class="btn dim" id="tlMcBrowse">Browse...</button></div>
  <div class="scroll"><div id="tlMcIntro" class="empty">Reads the winning copy of every mesh, loose or in the load order's archives (a loose file wins, then the later archive), and lists those that are not sound. A mesh from an archive is written as a loose file, which wins over the archive. Mendable: every fault is one an exporter is known to write, and the repair is proven to keep the mesh as it was (a UV flag left off where there are UV sets, which OpenMW and older tools misread). Mendable by a guess: a node whose lists do not read, which nothing else links to, emptied. Unreadable: another NIF version, a block type outside Morrowind's format, or not a NIF at all. Write repaired puts the ticked ones into the folder above, at their own paths. Your mods are not modified; the folder goes last in the load order, or is deleted to undo. The Mesh optimizer reads proven repairs the same way, so an optimized copy is repaired too.</div>
  <div id="tlMcBody"></div></div></div>`;
    document.body.appendChild(el);
    for(const d of el.querySelectorAll('#tlNav [data-t]')) d.onclick=()=>this.show(d.dataset.t);
    $('#tlLintRun').onclick=()=>this.runLint();
    $('#tlBkScan').onclick=()=>this.scanBackups();
    $('#tlMsPick').onclick=()=>this.pickPlugin();
    $('#tlSvPick').onclick=()=>this.saveCheck(1);
    $('#tlSvTwo').onclick=()=>this.saveCheck(2);
    $('#tlJrRead').onclick=()=>this.readJournal();
    $('#tlMoRun').onclick=()=>this.moRun();
    $('#tlMoWrite').onclick=()=>this.moWrite();
    $('#tlMoAll').onclick=()=>{ const bx=[...document.querySelectorAll('#tlMoBody input[data-p]')]; const on=bx.some(b=>!b.checked); for(const b of bx) b.checked=on; this.moTicked(); };
    $('#tlMcRun').onclick=()=>this.mcRun();
    $('#tlMcWrite').onclick=()=>this.mcWrite();
    $('#tlMcBrowse').onclick=async()=>{ const p=await Engine.pick('open',{directory:true}); if(p){ $('#tlMcOut').value=String(p); this.mcSave(); } };
    this.mcLoad();
    $('#tlMoBrowse').onclick=async()=>{ const p=await Engine.pick('open',{directory:true}); if(p){ $('#tlMoOut').value=String(p); this.moSave(); } };
    for(const id of ['tlMoGain','tlMoTris','tlMoFolder','tlMoOut']) $('#'+id).onchange=()=>this.moSave();
    this.moLoad();
    try{ $('#tlT3Exe').value=localStorage.getItem('wgTools.tes3cmd')||''; }catch(_){}
    $('#tlT3Exe').onchange=()=>{ try{ localStorage.setItem('wgTools.tes3cmd', $('#tlT3Exe').value.trim()); }catch(_){} if(typeof WgSettings==='object') WgSettings.write({tes3cmd:$('#tlT3Exe').value.trim()}); };
    try{ $('#tlT3Ext').checked=localStorage.getItem('wgTools.tes3cmdExternal')==='1'; }catch(_){}
    $('#tlT3Ext').onchange=()=>{ try{ localStorage.setItem('wgTools.tes3cmdExternal', $('#tlT3Ext').checked?'1':'0'); }catch(_){} };
    $('#tlT3Browse').onclick=async()=>{ const p=await Engine.pick('open',{}); if(p){ $('#tlT3Exe').value=String(p); $('#tlT3Exe').onchange(); } };
    $('#tlT3Add').onclick=async()=>{
      const p=await Engine.pick('open',{multiple:true, filters:[{name:'Plugins', extensions:['esp','esm','omwaddon','omwgame','ESP','ESM']}]});
      if(!p) return; this.t3Add((Array.isArray(p)?p:[p]).map(String));
    };
    const addNamed=async(kind)=>{
      try{
        const names=await this.mods(kind);
        if(!names.length) return toast(kind==='mine'
          ? 'The sort found no mods of yours. Name them on the Load order page - a Subset file, a Mods folder to scan, or "Pull unmanaged entries from the cfg" - then Sort.'
          : 'The sort found no curated list. Set plugin-order.yml and the List name on the Load order page (the list comes from there), then Sort.','warn',8000);
        let r=await Engine.call('tools_locate',{cfg:this.cfg(), names}); if(typeof r==='string') r=JSON.parse(r);
        this.t3Add(r.filter(x=>x[1]).map(x=>String(x[1])));
      }catch(e){ toast(String(e.message||e),'err',5000); }
    };
    $('#tlT3Mine').onclick=()=>addNamed('mine');
    $('#tlT3List').onclick=()=>addNamed('list');
    const pickPlugin=async(multiple)=>Engine.pick('open',{multiple, filters:[{name:'Plugins', extensions:['esp','esm','omwaddon','omwgame','ESP','ESM']}]});
    $('#tlM2Mode').onchange=()=>{ const one=$('#tlM2Mode').value==='plugin'; $('#tlM2One').hidden=!one; $('#tlM2Many').hidden=one; this.m2Draw(); };
    $('#tlM2PickP').onclick=async()=>{ const p=await pickPlugin(false); if(p) $('#tlM2Plugin').value=String(p); };
    $('#tlM2PickM').onclick=async()=>{ const p=await pickPlugin(false); if(p) $('#tlM2Master').value=String(p); };
    $('#tlM2Add').onclick=async()=>{ const p=await pickPlugin(true); if(p) this.m2Add((Array.isArray(p)?p:[p]).map(String)); };
    const m2Named=async(kind)=>{
      try{
        const names=await this.mods(kind);
        if(!names.length) return toast(kind==='mine'? 'The sort found no mods of yours' : 'The sort found no curated list','warn',4000);
        let r=await Engine.call('tools_locate',{cfg:this.cfg(), names}); if(typeof r==='string') r=JSON.parse(r);
        this.m2Add(r.filter(x=>x[1]).map(x=>String(x[1])));
      }catch(e){ toast(String(e.message||e),'err',5000); }
    };
    $('#tlM2Mine').onclick=()=>m2Named('mine');
    $('#tlM2List').onclick=()=>m2Named('list');
    $('#tlM2Clear').onclick=()=>{ this.m2.files=[]; this.m2Draw(); };
    $('#tlM2Run').onclick=()=>this.m2Run();
    $('#tlT3Clear').onclick=()=>{ this.t3.files=[]; this.t3Draw(); };
    $('#tlT3ResFind').oninput=()=>this.t3ResDraw();
    $('#tlT3ResFile').onchange=()=>this.t3ResDraw();
    $('#tlT3ResAll').onclick=()=>{ const res=this.t3.res; if(!res) return; for(const r of this.t3Shown()) res.sel.add(r.key); this.t3ResDraw(); };
    $('#tlT3ResUndo').onclick=()=>this.t3Undo();
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
    $('#tlMlCancel').onclick=()=>this.landsCancel();
    try{ const m=localStorage.getItem('wgTools.mergeLandsModel'); if(m) $('#tlMlModel').value=m; const ig=localStorage.getItem('wgTools.mergeLandsIgnore'); if(ig!=null) $('#tlMlIgnore').value=ig; }catch(_){}
    $('#tlMlModel').onchange=()=>{ try{ localStorage.setItem('wgTools.mergeLandsModel', $('#tlMlModel').value); }catch(_){} };
    $('#tlMlIgnore').onchange=()=>{ try{ localStorage.setItem('wgTools.mergeLandsIgnore', $('#tlMlIgnore').value); }catch(_){} };
    // The seam repair settings, kept like the others (off until asked for).
    try{ $('#tlMlSeams').checked=localStorage.getItem('wgTools.mergeLandsSeams')==='1'; const g=localStorage.getItem('wgTools.mergeLandsSeamGap'); if(g) $('#tlMlSeamGap').value=g; const e=localStorage.getItem('wgTools.mergeLandsSeamExcept'); if(e!=null) $('#tlMlSeamExcept').value=e; }catch(_){}
    try{ $('#tlMlNoClamp').checked=localStorage.getItem('wgTools.mergeLandsNoClamp')==='1'; }catch(_){}
    try{ $('#tlMlKeepWnam').checked=localStorage.getItem('wgTools.mergeLandsKeepWnam')==='1'; }catch(_){}
    $('#tlMlKeepWnam').onchange=()=>{ try{ localStorage.setItem('wgTools.mergeLandsKeepWnam', $('#tlMlKeepWnam').checked?'1':'0'); }catch(_){} };
    $('#tlMlNoClamp').onchange=()=>{ try{ localStorage.setItem('wgTools.mergeLandsNoClamp', $('#tlMlNoClamp').checked?'1':'0'); }catch(_){} };
    // The cells left out, kept like the others; never set, the default (Hunza Camp).
    const CELL_DEFAULT='TR_Mainland.esm=-9,-46;-9,-47;-10,-46;-10,-47';
    try{ const v=localStorage.getItem('wgTools.mergeLandsCellIgnore'); $('#tlMlCellIgnore').value=v==null? CELL_DEFAULT : v; }catch(_){ $('#tlMlCellIgnore').value=CELL_DEFAULT; }
    $('#tlMlCellIgnore').onchange=()=>{ try{ localStorage.setItem('wgTools.mergeLandsCellIgnore', $('#tlMlCellIgnore').value); }catch(_){} };
    /* OpenMWMergedLands' merged_lands.toml (Static, 2026-11-16: "read the fork's merged_lands.toml"):
       read once, into the fields - what it leaves out, by plugin and folder and by cell, and where it
       writes. Wraithguard's settings stay the ones used; this only fills them in. Shift+click, or
       nothing found, picks the file by hand. */
    $('#tlMlFork').onclick=async(e)=>{
      try{
        let path=null;
        if(e && e.shiftKey){ path=await Engine.pick('open',{filters:[{name:'merged_lands.toml',extensions:['toml']}]}); if(!path) return; }
        let r=await Engine.call('land_merge_fork_config',{cfg:this.cfg(), path:path? String(path) : null});
        if(typeof r==='string') r=JSON.parse(r);
        if(!r.found){
          const pick=await Engine.pick('open',{filters:[{name:'merged_lands.toml',extensions:['toml']}]});
          if(!pick){ toast('No merged_lands.toml in '+(r.looked||[]).join(', '),'warn',6000); return; }
          r=await Engine.call('land_merge_fork_config',{cfg:this.cfg(), path:String(pick)}); if(typeof r==='string') r=JSON.parse(r);
        }
        const said=[];
        if(r.ignore && r.ignore.length){ $('#tlMlIgnore').value=r.ignore.join(', '); $('#tlMlIgnore').onchange(); said.push(r.ignore.length+' plugin(s) or folder(s) left out'); }
        $('#tlMlCellIgnore').value=r.cells||''; $('#tlMlCellIgnore').onchange(); said.push(r.cells? 'cells left out: '+r.cells : 'no cells left out');
        if(r.output){ $('#tlMlFolder').value=r.output; if($('#tlMlFolder').onchange) $('#tlMlFolder').onchange(); said.push('writes into '+r.output); }
        toast('Read '+r.path+': '+said.join('; '),'ok',8000);
      }catch(err){ toast('OpenMWMergedLands\' settings: '+String(err.message||err),'err',8000); }
    };
    $('#tlMlCellDefault').onclick=()=>{ $('#tlMlCellIgnore').value=CELL_DEFAULT; $('#tlMlCellIgnore').onchange(); };
    /* The plugins left out put back (Static, 2026-11-17): an emptied box is kept as empty, so the
       default never came back by itself and every generated patch was read. The list is the
       engine's own (land_merge_home's `ignore`, service::DEFAULT_IGNORE). */
    $('#tlMlIgnoreDefault').onclick=async()=>{
      try{
        let h=(this.ml && this.ml.home) || await Engine.call('land_merge_home',{cfg:this.cfg()});
        if(typeof h==='string') h=JSON.parse(h);
        $('#tlMlIgnore').value=((h && h.ignore)||[]).join(', '); $('#tlMlIgnore').onchange();
      }catch(err){ toast('The default list: '+String(err.message||err),'err',5000); }
    };
    $('#tlMlSeams').onchange=()=>{ try{ localStorage.setItem('wgTools.mergeLandsSeams', $('#tlMlSeams').checked?'1':'0'); }catch(_){} };
    $('#tlMlSeamGap').onchange=()=>{ try{ localStorage.setItem('wgTools.mergeLandsSeamGap', $('#tlMlSeamGap').value); }catch(_){} };
    $('#tlMlSeamExcept').onchange=()=>{ try{ localStorage.setItem('wgTools.mergeLandsSeamExcept', $('#tlMlSeamExcept').value); }catch(_){} };
    $('#tlJrFind').oninput=()=>this.drawQuests();
    $('#tlBkRestore').onclick=()=>this.restoreBackups();
    $('#tlBkDelete').onclick=()=>this.deleteBackups();
    $('#tlBkAll').onclick=()=>{ const boxes=[...document.querySelectorAll('#tlBkBody input')]; const on=!boxes.every(b=>b.checked); boxes.forEach(b=>b.checked=on); };
    $('#tlLintCopy').onclick=async()=>{
      try{ await navigator.clipboard.writeText(this.lint.warnings.join('\n')); toast('Copied','ok',2000); }catch(e){ toast(String(e.message||e),'err',4000); }
    };
  },

  show(tool){
    // Save Check reads OpenMW's saves: not offered for an MO2 or Morrowind.ini install.
    // (an MO2 install that feeds OpenMW keeps it: wgSetupProbe asks, then this runs again)
    const omw=typeof wgSetupOpenmw!=='function' || wgSetupOpenmw(this.cfg());
    if(!omw && typeof wgSetupProbe==='function' && !this._probing){
      this._probing=true;
      wgSetupProbe(this.cfg()).then(r=>{ this._probing=false; if(r.openmw && this.on) this.show(tool); });
    }
    const sv=document.querySelector('#tlNav [data-t="save"]');
    if(sv){ sv.hidden=!omw; sv.style.display=omw? '' : 'none'; }
    if(tool==='save' && !omw) tool='lint';
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
    $('#tlObj').hidden=tool!=='mergeobjects';
    $('#tlMo').hidden=tool!=='meshopt';
    $('#tlMc').hidden=tool!=='meshcheck';
    $('#tlM2m').hidden=tool!=='m2m';
    if(tool==='mergelands') this.landsHome();
    if(tool==='mergeobjects') this.obInit();
    if(tool==='mergesettings') this.mergeMine();
  },

  /* ---- Merge objects (editcore::mergeobjects, TES3Merge's rules) ---- */
  /** TES3Merge's record types (`DEFAULT_TYPES`; FACT off by default, as TES3Merge has it). */
  OB_TYPES:['ACTI','ALCH','APPA','ARMO','BODY','BOOK','BSGN','CELL','CLAS','CLOT','CONT','CREA','DOOR','ENCH','FACT','GMST','INGR','LEVC','LEVI',
    'LIGH','LOCK','MGEF','MISC','NPC_','PROB','RACE','REPA','SKIL','SNDG','SOUN','SPEL','STAT','WEAP'],
  OB_KEEP:['tlObLists','tlObDry','tlObSummons','tlObFog','tlObNames','tlObIncl','tlObIgnore','tlObFilters','tlObFolder'],
  obInit(){
    if(this.ob) return;
    this.ob={};
    let kept={}; try{ kept=JSON.parse(localStorage.getItem('wgTools.mergeObjects')||'{}'); }catch(_){}
    const off=new Set(kept.typesOff||['FACT']);
    $('#tlObTypes').innerHTML='<span title="The record types merged; FACT is off as in TES3Merge">Types</span>'+this.OB_TYPES.map(t=>`<label style="font-family:var(--mono)"><input type="checkbox" data-obt="${t}"${off.has(t)?'':' checked'}> ${t}</label>`).join('');
    for(const id of this.OB_KEEP){ const el=$('#'+id); if(!(id in kept)) continue; if(el.type==='checkbox') el.checked=!!kept[id]; else el.value=kept[id]; }
    const save=()=>{ const k={typesOff:[...document.querySelectorAll('#tlObTypes [data-obt]')].filter(b=>!b.checked).map(b=>b.dataset.obt)};
      for(const id of this.OB_KEEP){ const el=$('#'+id); k[id]=el.type==='checkbox'? el.checked : el.value; }
      try{ localStorage.setItem('wgTools.mergeObjects', JSON.stringify(k)); }catch(_){} };
    for(const el of document.querySelectorAll('#tlObj input, #tlObj textarea')) el.addEventListener('change', save);
    $('#tlObBrowse').onclick=async()=>{ const p=await Engine.pick('open',{directory:true}); if(p){ $('#tlObFolder').value=String(p); save(); } };
    $('#tlObRun').onclick=()=>this.obRun();
    $('#tlObLists').onchange=()=>{ $('#tlObRun').textContent=$('#tlObLists').checked? 'Build multipatch.esp' : 'Build Merged Objects.esp'; save(); };
    $('#tlObLists').onchange();
  },
  async obRun(){
    const lists=$('#tlObLists').checked, dry=$('#tlObDry').checked;
    const name=lists? 'multipatch.esp' : 'Merged Objects.esp';
    if(!dry && !await wgConfirm(`Merge the load order's records into ${name}? A ${name} already there is replaced.`)) return;
    const btn=$('#tlObRun'); btn.disabled=true;
    $('#tlObIntro').hidden=true; $('#tlObOut').textContent='Reading the load order...';
    try{
      const patches=[['tlObSummons','summons'],['tlObFog','fogbug'],['tlObNames','cellnames']].filter(([id])=>$('#'+id).checked).map(x=>x[1]);
      const types=[...document.querySelectorAll('#tlObTypes [data-obt]')].filter(b=>b.checked).map(b=>b.dataset.obt);
      if(!lists && !types.length) throw new Error('Tick at least one record type.');
      const r=unpackTables(await Engine.bytes('tools_merge_objects',{cfg:this.cfg(), folder:$('#tlObFolder').value.trim(), types, inclusive:$('#tlObIncl').checked,
        patches, ignore:$('#tlObIgnore').value.split(',').map(s=>s.trim()).filter(Boolean), filters:$('#tlObFilters').value, listsOnly:lists, dryRun:dry}));
      const head=(r.head||[])[0]||[], status=head[0]||'';
      const kinds=(r.types||[]).map(x=>x[1]+' '+x[0]).join(', ');
      $('#tlObOut').textContent=status+(kinds? '\n'+kinds : '')+'\n\n'+(r.lines||[]).map(x=>x[0]).join('\n');
      toast(status,'ok',6000);
    }catch(e){ $('#tlObOut').textContent=String(e.message||e); toast(String(e.message||e),'err',6000); }
    finally{ btn.disabled=false; }
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
    const btn=$('#tlMlRun'), stop=$('#tlMlCancel'); btn.disabled=true;
    stop.hidden=false; stop.disabled=false;
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
      const seams=$('#tlMlSeams').checked;
      const r=await Engine.call('land_merge',{cfg:this.cfg(), folder, dryRun:dry, verbose:$('#tlMlVerbose').checked, model:$('#tlMlModel').value, ignore, seamGap: seams? Math.max(1, parseInt($('#tlMlSeamGap').value,10)||24) : null, seamExcept: $('#tlMlSeamExcept').value, noClamp: $('#tlMlNoClamp').checked, keepWorldMap: $('#tlMlKeepWnam').checked, cellIgnore: $('#tlMlCellIgnore').value}).finally(()=>{ live=false; });
      // What the log showed while it ran (every line, with its time) stays: `r.log`. `r.lines` is the
      // report alone, without the timings or the progress lines, for a backend that sends no log.
      const said=Array.isArray(r.log) && r.log.length? r.log : r.lines;
      const lines=['='.repeat(70),' MERGE LANDS','='.repeat(70), ...said.map(l=>'  '+l)];
      if(r.cancelled) lines.push('', 'Cancelled: nothing was written.');
      else if(r.error) lines.push('', 'Merge failed: '+r.error);
      if(r.notes && r.notes.length) lines.push('', 'Left out or untranslated:', ...r.notes.map(n=>'  '+n));
      if(!r.error && r.output){
        const res=r.result;
        let detail=`${res.contested} vertex/vertices were edited by more than one mod; the later one in your load order won each.`;
        if(res.major) detail+=` ${res.major} were blended far from a mod's intent where a .mergedlands.toml asked to Resolve.`;
        lines.push('', `Written to: ${r.output}`, 'Enable it and place it LAST in your load order.', `${res.cells} land record(s), ${res.textures} land texture(s).`, detail);
        toast('Merged Lands written: '+res.cells+' cell(s)','ok',5000);
      }else if(r.cancelled){ toast('Merge Lands cancelled','ok',3000); }
      else if(r.error){ toast('Merge failed -- see the log','err',6000); }
      else toast('Merge produced nothing to write','ok',4000);
      out.textContent=lines.join('\n');
    }catch(e){ out.textContent+='\n\nERROR: '+String(e.message||e); toast(String(e.message||e),'err',6000); }
    finally{ this.ml.running=false; btn.disabled=false; stop.hidden=true; }
  },

  /** The running merge stopped at its next step (`land_merge_cancel`); it writes nothing. */
  async landsCancel(){
    if(!this.ml.running) return;
    const stop=$('#tlMlCancel'); stop.disabled=true;
    try{ await Engine.call('land_merge_cancel',{}); }
    catch(e){ stop.disabled=false; toast(String(e.message||e),'err',5000); }
  },

  /* ---- Mesh optimizer (conflictcore::nif_batch) ---- */
  moOpts(){
    return {minGain:parseFloat($('#tlMoGain').value)||0, minTriangles:parseInt($('#tlMoTris').value,10)||0, folder:$('#tlMoFolder').value.trim()};
  },
  moSave(){
    try{ localStorage.setItem('wgTools.meshopt', JSON.stringify({gain:$('#tlMoGain').value, tris:$('#tlMoTris').value, folder:$('#tlMoFolder').value, out:$('#tlMoOut').value.trim()})); }catch(_){}
  },
  moLoad(){
    try{
      const v=JSON.parse(localStorage.getItem('wgTools.meshopt')||'{}');
      if(v.gain!=null) $('#tlMoGain').value=v.gain;
      if(v.tris!=null) $('#tlMoTris').value=v.tris;
      if(v.folder!=null) $('#tlMoFolder').value=v.folder;
      if(v.out!=null) $('#tlMoOut').value=v.out;
    }catch(_){}
  },
  /** The run's progress (meshopt_progress) in the header, every half second. Returns the stop. */
  moTick(label){
    let live=true; const t0=performance.now(), stat=$('#tlMoStats');
    (async()=>{
      while(live){
        try{
          let p=await Engine.call('meshopt_progress',{}); if(typeof p==='string') p=JSON.parse(p);
          if(live && Array.isArray(p)) stat.textContent=`${label} ${p[0]} of ${p[1]} (${((performance.now()-t0)/1000).toFixed(0)} s)`;
        }catch(_){}
        await new Promise(r=>setTimeout(r,500));
      }
    })();
    return ()=>{ live=false; };
  },
  async moRun(){
    if(this.mo.running) return;
    this.mo.running=true; const btn=$('#tlMoRun'); btn.disabled=true; $('#tlMoWrite').disabled=true;
    $('#tlMoBody').innerHTML='<div class="empty">Reading the load order\'s meshes...</div>';
    const stop=this.moTick('Measuring');
    try{
      this.moSave();
      let r=await Engine.call('meshopt_analyse',{cfg:this.cfg(), opts:this.moOpts(), outDir:$('#tlMoOut').value.trim(), limit:5000});
      if(typeof r==='string') r=JSON.parse(r);
      this.mo.rows=r.rows||[]; this.mo.stats=r.stats; this.mo.shown=r.shown||0;
      stop(); this.moDraw();
    }catch(e){ stop(); $('#tlMoStats').textContent=''; $('#tlMoBody').innerHTML=`<div class="empty">${loEsc(String(e.message||e))}</div>`; toast(String(e.message||e),'err',6000); }
    finally{ stop(); this.mo.running=false; btn.disabled=false; }
  },
  moDraw(){
    const s=this.mo.stats, rows=this.mo.rows;
    const mb=b=>(b/1048576).toFixed(1)+' MB', two=p=>p[0].toFixed(2)+' → '+p[1].toFixed(2);
    $('#tlMoStats').textContent=`${s.meshes} meshes · ${s.qualifying} worth rewriting (${s.qualifyingTriangles.toLocaleString()} triangles, ${mb(s.qualifyingBytes)}) · ${s.improvable} could improve · ${s.unreadable} unreadable`;
    if(!rows.length){ $('#tlMoBody').innerHTML='<div class="empty">No meshes found in the load order\'s data folders.</div>'; $('#tlMoWrite').disabled=true; return; }
    const tail=d=>String(d).replace(/[\\/]+$/,'').split(/[\\/]/).pop();
    const body=rows.map((r,i)=>{
      const ok=r.qualifies;
      return `<tr class="${ok?'':'no'}"><td>${ok?`<input type="checkbox" data-p="${i}" checked>`:''}</td><td class="p">${loEsc(r.path)}</td><td class="p" title="${loEsc(r.dir)}">${loEsc(tail(r.dir))}</td>`+
        `<td>${r.triangles.toLocaleString()}</td><td>${r.improvable?(r.gain*100).toFixed(1)+'%':''}</td>`+
        `<td>${r.improvable?two(r.acmr):''}</td><td>${r.improvable?two(r.overdraw):''}</td><td>${r.improvable?two(r.overfetch):''}</td><td class="p">${loEsc([r.note, (r.repaired&&r.repaired.length)? 'repaired first (Check meshes): '+r.repaired.join('; ') : ''].filter(Boolean).join(' · '))}</td></tr>`;
    }).join('');
    const more=s.meshes>this.mo.shown? `<div class="empty">The ${this.mo.shown.toLocaleString()} best of ${s.meshes.toLocaleString()} are listed; Write copies takes every mesh that qualifies when none is ticked.</div>` : '';
    $('#tlMoBody').innerHTML=`<table class="mo"><tr><th></th><th class="p">Mesh</th><th class="p">From</th><th>Triangles</th><th>Gain</th><th>Cache (ACMR)</th><th>Overdraw</th><th>Fetch</th><th class="p"></th></tr>${body}</table>${more}`;
    for(const b of document.querySelectorAll('#tlMoBody input[data-p]')) b.onchange=()=>this.moTicked();
    this.moTicked();
  },
  moTicked(){
    $('#tlMoWrite').disabled=!(this.mo.stats && this.mo.stats.qualifying>0);
  },
  async moWrite(){
    if(this.mo.running) return;
    const out=$('#tlMoOut').value.trim();
    if(!out) return toast('Choose the folder the optimized meshes go to first','warn',3500);
    const boxes=[...document.querySelectorAll('#tlMoBody input[data-p]')];
    const ticked=boxes.filter(b=>b.checked).map(b=>this.mo.rows[+b.dataset.p].path);
    // Every box ticked and every qualifying mesh listed: all of them, even those not listed.
    const all=boxes.length>0 && ticked.length===boxes.length && this.mo.stats.qualifying>boxes.length;
    if(boxes.length && !ticked.length) return toast('Tick the meshes to write','warn',3000);
    const n=all? this.mo.stats.qualifying : (ticked.length||this.mo.stats.qualifying);
    const ok=await wgConfirm(`${n.toLocaleString()} optimized mesh(es) will be written into:\n${out}\n\nYour mods are NOT modified. Each copy is checked to be the same mesh as the original. Add that folder LAST in your data= lines to use them, or delete it to undo.`);
    if(!ok) return;
    this.mo.running=true; $('#tlMoRun').disabled=true; $('#tlMoWrite').disabled=true;
    const stop=this.moTick('Writing');
    try{
      this.moSave();
      let r=await Engine.call('meshopt_write',{cfg:this.cfg(), opts:this.moOpts(), outDir:out, paths:all? [] : ticked});
      if(typeof r==='string') r=JSON.parse(r);
      stop();
      const saved=r.bytesBefore-r.bytesAfter;
      $('#tlMoStats').textContent=`${r.written} written${r.failed?`, ${r.failed} failed`:''} to ${out}`;
      toast(`${r.written} optimized mesh(es) written`+(r.failed?`; ${r.failed} failed`:''), r.failed?'warn':'ok', 6000);
      if(r.failures && r.failures.length){
        $('#tlMoBody').insertAdjacentHTML('afterbegin', `<div class="warn">Not written:\n${r.failures.map(f=>loEsc(f.path+': '+f.why)).join('\n')}</div>`);
      }
      void saved;
    }catch(e){ stop(); toast(String(e.message||e),'err',6000); }
    finally{ stop(); this.mo.running=false; $('#tlMoRun').disabled=false; this.moTicked(); }
  },

  /* ---- Check meshes (conflictcore::nif_repair); progress is the optimizer's ---- */
  MC_STATUS:['sound','mendable','mendable by a guess','unreadable'],
  mcOpts(){ return {folder:$('#tlMcFolder').value.trim()}; },
  mcSave(){
    try{ localStorage.setItem('wgTools.meshcheck', JSON.stringify({folder:$('#tlMcFolder').value, out:$('#tlMcOut').value.trim(), guesses:$('#tlMcGuess').checked})); }catch(_){}
  },
  mcLoad(){
    try{
      const v=JSON.parse(localStorage.getItem('wgTools.meshcheck')||'{}');
      if(v.folder!=null) $('#tlMcFolder').value=v.folder;
      if(v.out!=null) $('#tlMcOut').value=v.out;
      $('#tlMcGuess').checked=!!v.guesses;
    }catch(_){}
  },
  mcTick(label){
    let live=true; const t0=performance.now(), stat=$('#tlMcStats');
    (async()=>{
      while(live){
        try{
          let p=await Engine.call('meshopt_progress',{}); if(typeof p==='string') p=JSON.parse(p);
          if(live && Array.isArray(p)) stat.textContent=`${label} ${p[0]} of ${p[1]} (${((performance.now()-t0)/1000).toFixed(0)} s)`;
        }catch(_){}
        await new Promise(r=>setTimeout(r,500));
      }
    })();
    return ()=>{ live=false; };
  },
  /** `meshcheck_scan`'s tables (conflicts.rs) as rows: {path, dir, status, bytes, why, faults}. */
  mcRead(tb){
    const h=tb.head[0], dirs=tb.dirs.map(r=>r[0]);
    const rows=tb.rows.map(r=>({path:r[0], dir:dirs[r[1]]||'', status:r[2], bytes:+r[3], why:r[4], faults:[]}));
    for(const f of tb.faults) rows[f[0]].faults.push({label:f[1], name:f[2], block:f[3], certain:!!f[4], note:f[5]});
    return {head:{meshes:h[0], sound:h[1], mendable:h[2], guesses:h[3], unreadable:h[4]}, rows};
  },
  async mcRun(){
    if(this.mc.running) return;
    this.mc.running=true; const btn=$('#tlMcRun'); btn.disabled=true; $('#tlMcWrite').disabled=true;
    $('#tlMcBody').innerHTML='<div class="empty">Reading the load order\'s meshes...</div>';
    const stop=this.mcTick('Checking');
    try{
      this.mcSave();
      const got=this.mcRead(unpackTables(await Engine.bytes('meshcheck_scan',{cfg:this.cfg(), opts:this.mcOpts(), outDir:$('#tlMcOut').value.trim()})));
      this.mc.rows=got.rows; this.mc.head=got.head;
      stop(); this.mcDraw();
    }catch(e){ stop(); $('#tlMcStats').textContent=''; $('#tlMcBody').innerHTML=`<div class="empty">${loEsc(String(e.message||e))}</div>`; toast(String(e.message||e),'err',6000); }
    finally{ stop(); this.mc.running=false; btn.disabled=false; }
  },
  mcDraw(){
    const h=this.mc.head, rows=this.mc.rows, guesses=$('#tlMcGuess').checked;
    $('#tlMcStats').textContent=`${h.meshes} meshes · ${h.sound} sound · ${h.mendable} mendable · ${h.guesses} by a guess · ${h.unreadable} unreadable`;
    if(!rows.length){ $('#tlMcBody').innerHTML=`<div class="empty">${h.meshes? 'Every mesh is sound.' : 'No meshes found in the load order\'s data folders.'}</div>`; this.mcTicked(); return; }
    const tail=d=>String(d).replace(/[\\/]+$/,'').split(/[\\/]/).pop();
    const body=rows.map((r,i)=>{
      const can=r.status===1 || r.status===2;
      const box=can? `<input type="checkbox" data-c="${i}"${r.status===1||guesses?' checked':''}>` : '';
      const what=r.faults.map(f=>`<div>${loEsc(f.label)}${f.certain?'':' <b>(a guess)</b>'}: block ${f.block} (${loEsc(f.name)}): ${loEsc(f.note)}</div>`).join('')+(r.why? `<div>${loEsc(r.why)}</div>` : '');
      return `<tr class="${can?'':'no'}"><td>${box}</td><td class="p">${loEsc(r.path)}</td><td class="p" title="${loEsc(r.dir)}">${loEsc(tail(r.dir))}</td><td>${this.MC_STATUS[r.status]}</td><td class="p">${what}</td></tr>`;
    }).join('');
    $('#tlMcBody').innerHTML=`<table class="mo"><tr><th></th><th class="p">Mesh</th><th class="p">From</th><th>Stands</th><th class="p">What was found</th></tr>${body}</table>`;
    for(const b of document.querySelectorAll('#tlMcBody input[data-c]')) b.onchange=()=>this.mcTicked();
    this.mcTicked();
  },
  mcTicked(){
    $('#tlMcWrite').disabled=![...document.querySelectorAll('#tlMcBody input[data-c]')].some(b=>b.checked);
  },
  async mcWrite(){
    if(this.mc.running) return;
    const out=$('#tlMcOut').value.trim();
    if(!out) return toast('Choose the folder the repaired meshes go to first','warn',3500);
    const ticked=[...document.querySelectorAll('#tlMcBody input[data-c]')].filter(b=>b.checked).map(b=>this.mc.rows[+b.dataset.c]);
    if(!ticked.length) return toast('Tick the meshes to write','warn',3000);
    // A ticked guess is written only with guesses on: ticking one asks for it.
    const guesses=ticked.some(r=>r.status===2);
    const n=ticked.length, g=ticked.filter(r=>r.status===2).length;
    const ok=await wgConfirm(`${n.toLocaleString()} repaired mesh(es) will be written into:\n${out}\n\n${g? `${g} of them are mended by a guess: a node that does not read is emptied.\n\n` : ''}Your mods are NOT modified. Add that folder LAST in your data= lines to use them, or delete it to undo.`);
    if(!ok) return;
    this.mc.running=true; $('#tlMcRun').disabled=true; $('#tlMcWrite').disabled=true;
    const stop=this.mcTick('Writing');
    try{
      this.mcSave();
      const tb=unpackTables(await Engine.bytes('meshcheck_write',{cfg:this.cfg(), opts:this.mcOpts(), outDir:out, paths:ticked.map(r=>r.path), guesses}));
      stop();
      const [written, failed]=tb.head[0];
      $('#tlMcStats').textContent=`${written} written${failed?`, ${failed} failed`:''} to ${out}`;
      toast(`${written} repaired mesh(es) written`+(failed?`; ${failed} failed`:''), failed?'warn':'ok', 6000);
      if(tb.failures.length) $('#tlMcBody').insertAdjacentHTML('afterbegin', `<div class="warn">Not written:\n${tb.failures.map(f=>loEsc(f[0]+': '+f[1])).join('\n')}</div>`);
    }catch(e){ stop(); toast(String(e.message||e),'err',6000); }
    finally{ stop(); this.mc.running=false; $('#tlMcRun').disabled=false; this.mcTicked(); }
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
      // The fields as the Load order page would sort them: what it remembered, the settings
      // file's over that (its own opening order), every field and flag - the Mods folder and
      // "pull unmanaged entries" are where "your mods" come from, so leaving them out found none.
      let seed=await WgSettings.seed(); if(typeof seed==='string') seed=JSON.parse(seed);
      const lo=(typeof WgLoadOrder==='object')? WgLoadOrder : null;
      const f=Object.assign({}, lo? lo.saved() : {}, (seed && seed.loadorder) || {});
      const lines=v=>String(v||'').split(/\r?\n/).map(x=>x.trim()).filter(Boolean);
      const a={cfg:f.cfg||this.cfg(), customizations:f.customizations||'', subsetFile:f.subsetFile||'',
        pluginOrderYml:f.pluginOrderYml||'', listName:f.listName||'', dataPathsCache:f.dataPathsCache||'',
        rules:lines(f.rules), groundcover:lines(f.groundcover), subset:[], emitToml:false, allowEmptySort:true,
        sortDataPaths:!!f.sortDataPaths, subsetFromCfg:!!f.subsetFromCfg, noPredicateWarnings:true, verbose:false,
        writeCfg:false, dryRun:true, noBackup:true};
      if(f.scanDir){
        const scan=loReadScan(unpackTables(await Engine.bytes('loadorder_scan',{dir:f.scanDir})));
        a.subsetLines=scan.lines;
      }
      const got=loReadPlan(unpackTables(await Engine.bytes('loadorder_compute',{args:JSON.stringify(a)})));
      if(got.refused) throw new Error('The sort refused: '+got.refused);
      plan=got.plan;
      this._plan=null; // never kept: the fields may be filled in a moment later
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
  m2Add(paths){ for(const p of paths) if(!this.m2.files.includes(p)) this.m2.files.push(p); this.m2Draw(); },
  m2Draw(){
    const box=$('#tlM2Files'); if(!box || $('#tlM2Mode').value!=='order') return;
    if(!this.m2.files.length){ box.className='empty'; box.textContent='Add the plugins to merge, masters first, in load order.'; return; }
    box.className='';
    box.innerHTML=this.m2.files.map((f,i)=>`<div style="font-family:var(--mono);padding:1px 8px;display:flex;gap:6px"><span style="flex:1">${loEsc(f)}</span>`+
      `<a href="#" data-up="${i}">up</a><a href="#" data-dn="${i}">down</a><a href="#" data-rm="${i}">remove</a></div>`).join('');
    const move=(i,d)=>{ const j=i+d; if(j<0||j>=this.m2.files.length) return; [this.m2.files[i],this.m2.files[j]]=[this.m2.files[j],this.m2.files[i]]; this.m2Draw(); };
    box.querySelectorAll('a[data-up]').forEach(a=>a.onclick=e=>{ e.preventDefault(); move(+a.dataset.up,-1); });
    box.querySelectorAll('a[data-dn]').forEach(a=>a.onclick=e=>{ e.preventDefault(); move(+a.dataset.dn,1); });
    box.querySelectorAll('a[data-rm]').forEach(a=>a.onclick=e=>{ e.preventDefault(); this.m2.files.splice(+a.dataset.rm,1); this.m2Draw(); });
  },
  async m2Run(){
    const mode=$('#tlM2Mode').value, plugin=$('#tlM2Plugin').value.trim(), master=$('#tlM2Master').value.trim();
    if(mode==='plugin' && (!plugin || !master)) return toast('Pick the plugin and its master first','warn',3000);
    if(mode==='order' && this.m2.files.length<2) return toast('Add at least two plugins, masters first','warn',3000);
    const base=(mode==='plugin'? master : this.m2.files[0]).replace(/\.[^.\\/]+$/,'');
    const out=await Engine.pick('save',{defaultPath:base+' (merged).esm', filters:[{name:'Master', extensions:['esm']}]});
    if(!out) return;
    const btn=$('#tlM2Run'); btn.disabled=true;
    const o=$('#tlM2Out'); o.textContent='Merging...';
    try{
      let r=await Engine.call('tools_merge_master',{mode, plugin, master, plugins:this.m2.files, out:String(out),
        removeDeleted:$('#tlM2Del').checked, applyMoved:$('#tlM2Moved').checked, keepDuplicates:$('#tlM2Dup').checked});
      if(typeof r==='string') r=JSON.parse(r);
      o.textContent=`Wrote ${r.out} (${r.bytes.toLocaleString()} bytes).`+(r.backup? `\nThe file it replaced: ${r.backup}` : '')+
        '\nThe plugins merged are unchanged. Load the merged master in place of what it holds, not beside it.';
      toast('Merged into '+String(r.out).split(/[\\/]/).pop(),'ok',4000);
    }catch(e){ o.textContent='Merge failed: '+String(e.message||e); toast('Merge failed','err',5000); }
    finally{ btn.disabled=false; }
  },
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
      // The engine picks the tes3cmd: built in, or the external one (found by itself when
      // the path is empty) when ticked or when the command runs Perl code.
      const exe=$('#tlT3Exe').value.trim(), external=$('#tlT3Ext').checked;
      const r=this.t3Read(await Engine.bytes('tools_tes3cmd',{cfg:this.cfg(), exe, cmd, extra, files:this.t3.files, external}));
      $('#tlT3Out').textContent=r.lines.join('\n')+'\n\n'+r.status;
      this.t3.res={run:r.run, files:r.files, changes:r.changes, sel:new Set(), kind:''};
      // With records to list, the list leads and the log folds away (one click to open).
      $('#tlT3Log').open=!r.changes.length;
      this.t3ResDraw();
      toast(r.status,'ok',5000);
    }catch(e){ $('#tlT3Out').textContent=String(e.message||e); $('#tlT3Log').open=true; }
    finally{ btn.disabled=false; }
  },

  /** `tools_tes3cmd`'s and `tools_tes3cmd_undo`'s bytes (WGTB, laid out beside them in
   *  commands/tools.rs): the status and run, the log, the plugins changed, and each record
   *  change as {file, row (its place in its file's list, what undo names), kind, tag, id, nth,
   *  detail, key}. */
  t3Read(buf){
    const t=unpackTables(buf), head=(t.head||[])[0]||[];
    const files=(t.files||[]).map(r=>({path:r[0], count:r[1]}));
    const seen=files.map(()=>0);
    const changes=(t.changes||[]).map(r=>{ const row=seen[r[0]]++; return {file:r[0], row, kind:r[1], tag:r[2], id:r[3], nth:r[4], detail:r[5], key:r[0]+':'+r[2]+':'+r[3]+':'+r[4]}; });
    return {status:head[0]||'', run:head[1]||'', lines:(t.lines||[]).map(r=>r[0]), files, changes};
  },
  /** The changes the filter, the kind chip and the plugin picker leave. */
  t3Shown(){
    const res=this.t3.res; if(!res) return [];
    const q=$('#tlT3ResFind').value.trim().toLowerCase(), f=$('#tlT3ResFile').value;
    return res.changes.filter(c=>c.tag!=='TES3' && (!res.kind || c.kind===res.kind) && (f==='' || String(c.file)===f) &&
      (!q || (c.tag+' '+c.id+' '+c.detail+' '+(res.files[c.file]||{}).path).toLowerCase().includes(q)));
  },
  t3ResDraw(){
    const res=this.t3.res, box=$('#tlT3Res');
    const list=res? res.changes.filter(c=>c.tag!=='TES3') : [];
    box.hidden=!res || !res.files.length;
    if(box.hidden) return;
    const base=p=>String(p).split(/[\\/]/).pop();
    const pick=$('#tlT3ResFile'), was=pick.value;
    pick.innerHTML='<option value="">every plugin ('+res.files.length+')</option>'+res.files.map((f,i)=>`<option value="${i}">${loEsc(base(f.path))} (${f.count})</option>`).join('');
    pick.value=[...pick.options].some(o=>o.value===was)? was : '';
    const counts={removed:0, changed:0, added:0}; for(const c of list) counts[c.kind]=(counts[c.kind]||0)+1;
    $('#tlT3ResKinds').innerHTML=[['','all',list.length],...Object.entries(counts).map(([k,n])=>[k,k,n])].filter(x=>x[2]||!x[0])
      .map(([k,l,n])=>`<span class="chip${res.kind===k?' on':''}" data-k="${k}" style="cursor:pointer;margin-right:4px">${l} (${n})</span>`).join('');
    for(const s of $('#tlT3ResKinds').querySelectorAll('[data-k]')) s.onclick=()=>{ res.kind=s.dataset.k; this.t3ResDraw(); };
    const shown=this.t3Shown(), cap=2000;
    // A tick stays only on a change still listed (an undo takes changes off the list).
    const live=new Set(list.map(c=>c.key)); for(const k of [...res.sel]) if(!live.has(k)) res.sel.delete(k);
    $('#tlT3ResN').textContent=list.length? `${list.length} record(s) in ${res.files.length} plugin(s), ${res.sel.size} ticked` : 'every change undone';
    const rows=shown.slice(0,cap).map(c=>`<tr><td><input type="checkbox" data-key="${loEsc(c.key)}"${res.sel.has(c.key)?' checked':''}></td>`+
      `<td class="p">${loEsc(base(res.files[c.file].path))}</td><td class="p">${c.kind}</td><td class="p">${loEsc(c.tag)}</td>`+
      `<td class="p">${loEsc(c.id)}${c.nth?' <small>('+(c.nth+1)+')</small>':''}</td><td class="p">${loEsc(c.detail)}</td></tr>`).join('');
    $('#tlT3ResBody').innerHTML=shown.length
      ? `<table class="mo"><tr><th></th><th class="p">Plugin</th><th class="p">Change</th><th class="p">Type</th><th class="p">Record</th><th class="p">Subrecords</th></tr>${rows}</table>`+
        (shown.length>cap? `<div class="empty">${shown.length-cap} more: narrow the filter to see them.</div>` : '')
      : `<div class="empty">${list.length? 'Nothing matches the filter.' : 'Nothing left to undo.'}</div>`;
    for(const b of $('#tlT3ResBody').querySelectorAll('input[data-key]')) b.onchange=()=>{ if(b.checked) res.sel.add(b.dataset.key); else res.sel.delete(b.dataset.key); $('#tlT3ResN').textContent=`${list.length} record(s) in ${res.files.length} plugin(s), ${res.sel.size} ticked`; };
  },
  /** Undo the ticked changes, a plugin at a time (`tools_tes3cmd_undo`); the list is redrawn
   *  from the engine's answer, so what is shown is what the files hold. */
  async t3Undo(){
    const res=this.t3.res; if(!res || !res.sel.size) return toast('Tick the changes to undo first','warn',3000);
    const byFile=new Map();
    for(const c of res.changes) if(res.sel.has(c.key)){ if(!byFile.has(c.file)) byFile.set(c.file,[]); byFile.get(c.file).push(c.row); }
    if(!await wgConfirm(`Put back ${res.sel.size} record(s) in ${byFile.size} plugin(s) as they were before the run?`)) return;
    const btn=$('#tlT3ResUndo'); btn.disabled=true;
    const said=[];
    try{
      for(const [file, rows] of byFile){
        try{
          const r=this.t3Read(await Engine.bytes('tools_tes3cmd_undo',{run:res.run, file, rows}));
          said.push(r.status); res.files=r.files; res.changes=r.changes;
        }catch(e){ said.push(String(e.message||e)); }
      }
      res.sel.clear();
      this.t3ResDraw();
      $('#tlT3Out').textContent+='\n\nUndo: '+said.join('\n      ');
      toast(said.join(' '),'ok',6000);
    }finally{ btn.disabled=false; }
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
      // WGTB (`tools_lint`): `stats` [scanned, unreadable, warnings, interiorCells, pathgrids], `warnings`.
      const tb=unpackTables(await Engine.bytes('tools_lint',{cfg:this.cfg(), subset:await this.mods('mine').catch(()=>[])}));
      const st=(tb.stats||[])[0]||[0,0,0,0,0];
      const r={warnings:(tb.warnings||[]).map(x=>x[0]), stats:{scanned:st[0], unreadable:st[1], warnings:st[2], interiorCells:st[3], pathgrids:st[4]}};
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
