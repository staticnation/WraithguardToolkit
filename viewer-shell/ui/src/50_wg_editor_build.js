/* =====================================================================================
   Wraithguard Editor (50_wg_editor.js): what waits in the pool, and the patch written from here (Build patch).

   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
Object.assign(WgEditor, {
  /* ---- what waits in the pool -------------------------------------------------------- */

  async refreshPending(){
    if(!this.canEdit()) return [];
    this._hist=null;
    let list=[];
    try{ list=await this.ask('editPending', {}); }catch(_){ return []; }
    this.edited=new Set(list.filter(p=>p.tag).map(p=>p.tag+':'+String(p.id).toLowerCase()));
    this.made=list.filter(p=>p.made && p.tag);
    const cellsBefore=[...this.editedCells].sort().join('|');
    this.editedCells=new Set(list.filter(p=>p.ref||p.new).map(p=>String((p.ref||p.new).cell).toLowerCase()));
    if(this.el && this.tags.length) this.fillTabs();
    if(this.el && this.cells && cellsBefore!==[...this.editedCells].sort().join('|')) this.drawCells();
    this.setLive(list);
    this._pendN=list.length;
    const b=document.getElementById('edPendBtn');
    if(b) b.textContent=list.length? 'Pending ('+list.length+')' : 'Pending';
    if(this.el) this.drawRows();
    if(this.pend && !this.pend.hidden) this.drawPending(list);
    // The revision this list is of, so the poll does not read it again (and the
    // toolbar's Undo / Redo know what there is).
    if(this.links().editRevision){
      try{ const r=await this.ask('editRevision', {}); this._rev=r.rev; this._hist={undo:r.undo|0, redo:r.redo|0}; }catch(_){ }
      if(typeof WgUI==='object') WgUI.refresh();
    }
    return list;
  },

  /** Test in OpenMW (`editTestRun`): the pool written to a scratch plugin (the pool kept)
   *  and OpenMW started on it past its menu, in the cell on screen. What the build and the
   *  game print goes to Messages as it comes; OpenMW not found, it asks where it is
   *  (remembered). `exe`: the executable or its folder, as typed. */
  async testRun(exe){
    if(this._testing) return toast('OpenMW is running a test already - close it first','warn',3000);
    const c=this.currentCell();
    const body=Object.assign({}, c? {cell:c.key} : {}, exe? {exe} : {});
    let job;
    try{ job=(await this.ask('editTestRun', body)).job; }
    catch(e){
      const msg=String(e.message||e).replace(/^Wraithguard answered 400:\s*/,'');
      if(/^needExe: /.test(msg)){
        const path=await WgUI.askText(msg.slice(9), exe||'', 'openmw.exe, or the folder it is in');
        return path? this.testRun(path) : null;
      }
      toast(msg,'err',6000);
      return null;
    }
    const log=(m, k)=>{ if(typeof WgLog==='object') WgLog.add(m, k||'info'); };
    this._testing=true;
    toast('Writing the test plugin…','info',2500);
    let seen=0, st=null, started=false;
    try{
      for(let i=0;;i++){
        await new Promise(r=>setTimeout(r, started? 1000 : (i<10? 150 : 400)));
        try{ st=await this.ask('editBuildStatus', {job, from:seen}); }
        catch(e){ log('Test in OpenMW: '+String(e.message||e), 'err'); break; }
        for(const l of st.lines||[]) log((started? 'OpenMW: ' : 'Test plugin: ')+l);
        seen=typeof st.total==='number'? st.total : seen+(st.lines||[]).length;
        if(!started && st.result && st.result.pid){
          started=true;
          toast('OpenMW started'+(c? ' in '+c.label : '')+', the pool as '+st.result.records+' record'+(st.result.records===1? '' : 's')+' - what it prints is in Messages','ok',6000);
        }
        if(st.state!=='running') break;
      }
    }finally{ this._testing=false; }
    if(st && st.state==='failed') toast('The test did not start: '+st.error,'err',8000);
    else if(st && st.state==='done'){
      const code=st.result && st.result.exit;
      toast('OpenMW closed'+(code? ' (exit code '+code+')' : ''), code? 'warn' : 'ok', 3000);
    }
    return st;
  },

  /** The verifier (`editVerify`): its findings over what the patch carries, in a table -
   *  level, check, record, field, what is wrong - filtered by check; double-click a row
   *  to open its record. */
  async showVerify(){
    if(!this.verEl){
      const d=document.createElement('div');
      d.id='edVerifyPanel'; d.className='ori'; d.hidden=true;
      d.innerHTML='<div class="orihead"><b>Verify</b><span style="flex:1"></span>'+
        '<select id="edVerCheck" title="Only one check"><option value="">Every check</option></select> '+
        '<button class="btn sm" id="edVerRe" title="Check again">Check again</button> '+
        '<button class="btn dim ic" id="edVerX" title="Close">&#x2715;</button></div><div class="oribody" id="edVerBody"></div>';
      WgUI.mount(d);
      d.querySelector('#edVerX').onclick=()=>{ d.hidden=true; };
      d.querySelector('#edVerRe').onclick=()=>this.showVerify();
      d.querySelector('#edVerCheck').onchange=()=>this.drawVerify();
      this.verEl=d;
    }
    const d=this.verEl, body=d.querySelector('#edVerBody');
    d.hidden=false;
    body.innerHTML='<div class="hint">Checking what the patch carries…</div>';
    try{ this._verify=await this.ask('editVerify', {}); }
    catch(e){ body.innerHTML='<div class="hint edBad">'+escHtml(String(e.message||e).replace(/^Wraithguard answered 400:\s*/,''))+'</div>'; return null; }
    const sel=d.querySelector('#edVerCheck'), was=sel.value;
    const checks=[...new Set(this._verify.findings.map(f=>f.check))].sort();
    sel.innerHTML='<option value="">Every check</option>'+checks.map(c=>'<option'+(c===was?' selected':'')+'>'+escHtml(c)+'</option>').join('');
    this.drawVerify();
    return this._verify;
  },
  drawVerify(){
    const d=this.verEl, v=this._verify; if(!d || !v) return;
    const body=d.querySelector('#edVerBody'), only=d.querySelector('#edVerCheck').value;
    const rows=v.findings.filter(f=>!only || f.check===only);
    const errs=v.findings.filter(f=>f.level==='error').length;
    const head='<div class="hint">'+v.checked+' record'+(v.checked===1?'':'s')+' checked: '+
      (v.findings.length? errs+' error'+(errs===1?'':'s')+', '+(v.findings.length-errs)+' warning'+(v.findings.length-errs===1?'':'s') : 'nothing found')+'</div>';
    body.innerHTML=head+(rows.length? '<table class="edVerTable"><thead><tr><th></th><th>Check</th><th>Record</th><th>Field</th><th>What is wrong</th></tr></thead><tbody>'+
      rows.map((f,i)=>'<tr data-i="'+i+'" class="'+escHtml(f.level)+'" title="Double-click: open '+escHtml(f.id)+'"><td>'+(f.level==='error'? '✖' : '⚠')+'</td><td>'+escHtml(f.check)+'</td><td>'+escHtml((this.NAMES[f.tag]||f.tag)+' '+f.id)+'</td><td>'+escHtml(f.path)+'</td><td>'+escHtml(f.message)+'</td></tr>').join('')+
      '</tbody></table>' : '');
    body.querySelectorAll('tr[data-i]').forEach(tr=>tr.ondblclick=()=>{
      const f=rows[+tr.dataset.i];
      if(f.tag==='PGRD'){ toast('Path grid of '+f.id+': open its cell and use Path grid mode','info',4000); return; }
      if(f.tag==='INFO') return toast('A dialogue response: open its topic in the Dialogue window','info',4000);
      this.showEntry(f.tag, f.id);
    });
    const b=document.getElementById('edVerifyBtn');
    if(b){ b.classList.toggle('badge', errs>0); b.dataset.n=errs? String(errs) : ''; }
  },

  /** Test in OpenMW's launch setups (`editTestSetups`, kept by Wraithguard): `{setups,
   *  use}` - read, or changed by `change` (`{setups}` and/or `{use}`). */
  async testSetups(change){
    const r=await this.ask('editTestSetups', change||{});
    this._setups=r;
    return r;
  },

  /** The launch setups (OpenMW-CS's debug profiles): one at a time - its name, the content
   *  files loaded before the test plugin (one per line, from the load order's data
   *  folders), and console commands run once the game is in the cell. `pick`: the one to
   *  show first. */
  async showSetups(pick){
    let r;
    try{ r=await this.testSetups(); }
    catch(e){ return toast(String(e.message||e).replace(/^Wraithguard answered 400:\s*/,''),'err',5000); }
    if(!this.setupsEl){
      const d=document.createElement('div');
      d.id='edSetupsPanel'; d.className='ori'; d.hidden=true;
      d.innerHTML='<div class="orihead"><b>Launch setups</b><span style="flex:1"></span>'+
        '<button class="btn dim ic" id="edSetX" title="Close">&#x2715;</button></div><div class="oribody" id="edSetBody"></div>';
      WgUI.mount(d);
      d.querySelector('#edSetX').onclick=()=>{ d.hidden=true; };
      this.setupsEl=d;
    }
    const d=this.setupsEl, body=d.querySelector('#edSetBody');
    d.hidden=false;
    const names=Object.keys(r.setups).sort();
    let cur=pick!=null? pick : (r.use || names[0] || '');
    const draw=()=>{
      const s=r.setups[cur]||{content:[], script:''};
      body.innerHTML=
        '<div class="orirow"><span class="k">Setup</span><span class="v"><select id="edSetPick">'+
          names.map(n=>'<option'+(n===cur?' selected':'')+'>'+escHtml(n)+'</option>').join('')+
          '<option value=""'+(cur===''? ' selected' : '')+'>New setup…</option></select></span></div>'+
        '<div class="orirow"><span class="k">Name</span><span class="v"><input id="edSetName" value="'+escHtml(cur)+'" placeholder="e.g. With my test mods"></span></div>'+
        '<div class="orirow"><span class="k" title="Loaded before the test plugin, in this order; from the load order\'s data folders">Content files</span><span class="v"><textarea id="edSetContent" rows="4" placeholder="One plugin per line, e.g. Test Helpers.esp">'+escHtml(s.content.join('\n'))+'</textarea></span></div>'+
        '<div class="orirow"><span class="k" title="Console commands, one per line, run once the game is in the cell">Start script</span><span class="v"><textarea id="edSetScript" rows="4" placeholder="e.g. tgm&#10;player->additem gold_001 1000">'+escHtml(s.script)+'</textarea></span></div>'+
        '<div class="orirow"><span class="v"><label class="from"><input type="checkbox" id="edSetUse"'+(cur && r.use===cur? ' checked' : '')+'> Test in OpenMW uses it</label> '+
          '<button class="btn sm" id="edSetSave">Save</button> '+
          (cur? '<button class="btn sm dim" id="edSetDel">Delete</button>' : '')+'</span></div>';
      body.querySelector('#edSetPick').onchange=e=>{ cur=e.target.value; draw(); };
      body.querySelector('#edSetSave').onclick=async()=>{
        const name=body.querySelector('#edSetName').value.trim();
        if(!name) return toast('A launch setup needs a name','warn',2500);
        const setups=Object.assign({}, r.setups);
        if(cur && cur!==name) delete setups[cur];
        setups[name]={content:body.querySelector('#edSetContent').value.split(/\r?\n/).map(x=>x.trim()).filter(Boolean),
                      script:body.querySelector('#edSetScript').value};
        const use=body.querySelector('#edSetUse').checked? name : (r.use===cur || r.use===name? '' : r.use);
        try{ r=await this.testSetups({setups, use}); }
        catch(e){ return toast(String(e.message||e).replace(/^Wraithguard answered 400:\s*/,''),'err',5000); }
        names.splice(0, names.length, ...Object.keys(r.setups).sort());
        cur=name; draw();
        toast('Launch setup '+name+' saved','ok',2000);
      };
      const del=body.querySelector('#edSetDel');
      if(del) del.onclick=async()=>{
        const setups=Object.assign({}, r.setups); delete setups[cur];
        try{ r=await this.testSetups({setups, use:r.use===cur? '' : r.use}); }
        catch(e){ return toast(String(e.message||e),'err',5000); }
        names.splice(0, names.length, ...Object.keys(r.setups).sort());
        cur=names[0]||''; draw();
      };
    };
    draw();
    return r;
  },

  /** The pool kept in step with Wraithguard: `editRevision` asked every POLL_MS while
   *  the Editor is open and the page visible; when it moved - the Patch Builder, an
   *  undo, anything not this page - the lists and every open view are read again. */
  POLL_MS:1500,
  startPoll(){
    if(this._poll || !this.links().editRevision) return;
    this._poll=setInterval(()=>this.pollPool(), this.POLL_MS);
  },
  stopPoll(){ clearInterval(this._poll); this._poll=0; },
  async pollPool(){
    if(!this.on || document.hidden || this._polling) return;
    this._polling=true;
    try{
      const r=await this.ask('editRevision', {});
      const hist={undo:r.undo|0, redo:r.redo|0};
      if(this._rev!=null && r.rev!==this._rev){
        await this.refreshPending();
        await this.rereadViews(false);
      }
      this._rev=r.rev; this._hist=hist;
      if(typeof WgUI==='object') WgUI.refresh();
    }catch(_){ }
    finally{ this._polling=false; }
  },

  async showPending(){
    if(!this.pend){
      const d=document.createElement('div');
      d.id='edPend'; d.className='ori'; d.hidden=true;
      d.innerHTML='<div class="orihead"><b>Pending changes</b><span style="flex:1"></span>'+
        '<button class="btn dim ic" id="edPendX" title="Close">&#x2715;</button></div><div class="oribody" id="edPendBody"></div>';
      WgUI.mount(d);
      d.querySelector('#edPendX').onclick=()=>{ d.hidden=true; };
      this.pend=d;
    }
    this.pend.hidden=false;
    if(!this.canEdit()){
      this.pend.querySelector('#edPendBody').innerHTML='<div class="hint">Editing needs Wraithguard.</div>';
      return;
    }
    this.drawPending(await this.refreshPending());
  },

  drawPending(list){
    const body=this.pend && this.pend.querySelector('#edPendBody'); if(!body) return;
    let h='<div class="hint">Wraithguard\'s patch pool: these changes and the conflict viewer\'s choices, written together from its Patch Builder. '+
      'Saved as you go - a crash loses none of them.</div>';
    if(!list.length) h+='<div class="hint">Nothing waiting.</div>';
    for(const p of list){
      h+='<div class="orisec">'+escHtml((this.NAMES[p.tag]||p.tag||p.type)+' - '+p.id)+'</div>';
      if(p.whole) h+='<div class="orirow"><span class="v">the whole record from '+escHtml(p.whole)+'</span></div>';
      for(const c of p.changes.filter(c=>!(p.new && c.value==null)))
        h+='<div class="orirow"><span class="k">'+escHtml(c.path)+'</span><span class="v">'+
          escHtml('value' in c? JSON.stringify(c.value) : 'from '+c.plugin)+'</span></div>';
      if(p.tag) h+='<div class="orirow"><span class="v"><button class="btn sm" data-open="'+escHtml(p.tag)+'" data-id="'+escHtml(p.id)+'">Open</button></span></div>';
      else if(p.ref) h+='<div class="orirow"><span class="v"><button class="btn sm" data-ref="'+escHtml(JSON.stringify(p.ref))+'">Open</button></span></div>';
      else if(p.new) h+='<div class="orirow"><span class="v"><button class="btn sm" data-ref="'+escHtml(JSON.stringify({cell:p.new.cell, uid:p.new.uid}))+'">Open</button></span></div>';
    }
    h+='<div class="orirow" style="margin-top:8px"><span class="v">'+
      '<button class="btn sm pri" id="edBuildHere" title="Write the patch from here: where it goes, Append or Replace, and the build\'s report, without leaving the viewer">Build patch…</button> '+
      '<button class="btn sm" id="edReview" title="Open the Patch Builder in Wraithguard, to review and write the patch there">Review and write in Wraithguard</button></span></div>';
    body.innerHTML=h;
    body.querySelector('#edBuildHere').onclick=()=>this.showBuild();
    body.querySelectorAll('[data-open]').forEach(b=>b.onclick=()=>this.openRecord(b.dataset.open, b.dataset.id, null));
    body.querySelectorAll('[data-ref]').forEach(b=>b.onclick=()=>this.openRef(JSON.parse(b.dataset.ref)));
    body.querySelector('#edReview').onclick=async()=>{
      try{ await Engine.call('wg_post',{url:this.links().editReview, body:'{}'}); toast('The Patch Builder is open in Wraithguard','ok',3000); }
      catch(e){ toast(String(e.message||e),'err',5000); }
    };
  },

  /* ---- the patch, written from here ---------------------------------------------------- */

  /** "Build patch": the Patch Builder's sister in the viewer. What the pool carries (each
   *  entry openable, and droppable when the editor made it), where the patch goes (a data
   *  folder, the last patch, or anywhere through the system's dialog), Append or Replace
   *  when the file is there, and the build itself with its report as it goes. Wraithguard
   *  writes it (`editBuild`), exactly as its own Patch Builder does, and the pool is
   *  cleared once it is written. */
  async showBuild(){
    if(!this.bld){
      const d=document.createElement('div');
      d.id='edBuild'; d.className='ori'; d.hidden=true;
      d.innerHTML='<div class="orihead"><b>Build patch</b><span style="flex:1"></span>'+
        '<button class="btn dim ic" id="edBuildX" title="Close">&#x2715;</button></div><div class="oribody" id="edBuildBody"></div>';
      WgUI.mount(d);
      d.querySelector('#edBuildX').onclick=()=>{ d.hidden=true; };
      this.bld=d;
    }
    this.bld.hidden=false;
    const body=this.bld.querySelector('#edBuildBody');
    if(!this.canEdit() || !this.links().editBuild){
      body.innerHTML='<div class="hint">Writing the patch needs Wraithguard: open this viewer from Wraithguard\'s Cell Preview.</div>';
      return null;
    }
    body.innerHTML='<div class="hint">Reading the pool…</div>';
    let info, list;
    try{ [info, list]=await Promise.all([this.ask('editBuildInfo', {}), this.refreshPending()]); }
    catch(e){ body.innerHTML='<div class="hint">'+escHtml(String(e.message||e).replace(/^Wraithguard answered 400:\s*/,''))+'</div>'; return null; }
    this._buildInfo=info;
    this.drawBuild(info, list||[]);
    return info;
  },

  drawBuild(info, list){
    const body=this.bld && this.bld.querySelector('#edBuildBody'); if(!body) return;
    const S=info.summary||{};
    const parts=[[S.whole,'whole record','whole records'],[S.merged,'record changed field by field','records changed field by field'],
                 [S.new_records,'new record','new records'],[S.refs,'placed object changed','placed objects changed'],
                 [S.new_refs,'new placed object','new placed objects']].filter(([n])=>n);
    let h='<div class="hint">'+(S.records? '<b>'+S.records+'</b> record'+(S.records===1?'':'s')+' to write: '+parts.map(([n,one,many])=>n+' '+(n===1? one : many)).join(', ')+'.'
      : 'Nothing is waiting in the pool: change a record or place an object first.')+'</div>';
    // What it carries.
    const rows=list.map((p,i)=>{
      const what=p.new? 'New placed object' : p.ref? 'Placed object' : (this.NAMES[p.tag]||p.tag||p.type);
      const id=p.new? (p.new.id||p.id) : p.ref? p.id+' ('+p.ref.origin+':'+p.ref.refr+')' : p.id;
      const how=p.whole? 'whole, from '+p.whole : p.made? 'made by the patch' : (p.changes||[]).length+' change'+((p.changes||[]).length===1?'':'s');
      const drop=this.canDrop(p);
      return '<tr data-i="'+i+'"'+(this._bSel && this._bSel.has(this.pendKey(p))? ' class="sel"' : '')+'><td>'+escHtml(what)+'</td><td class="edWrapCell">'+escHtml(id)+'</td><td class="from">'+escHtml(how)+'</td>'+
        '<td><button class="btn dim ic" data-bopen="'+i+'" title="Open it">✎</button>'+
        (drop? '<button class="btn dim ic" data-bdrop="'+i+'" title="Take it out of the patch (its changes are dropped)">&#x2715;</button>' : '')+'</td></tr>';
    }).join('');
    const sec=[{key:'carries', title:'What it carries', n:list.length,
      html:list.length? '<div class="edBuildList" tabindex="0" title="Click to select (Ctrl+click: several, Shift+click: a run); Delete takes the selected out of the patch">'+
        '<table class="edT"><tbody>'+rows+'</tbody></table></div>'+
        '<div class="wgBtns"><button class="btn sm" id="edBuildDel" disabled title="Take the selected out of the patch: their changes are dropped (Delete)">✕ Delete selected</button>'+
        '<button class="btn dim sm" id="edBuildAll" title="Select everything in the list (Ctrl+A)">Select all</button></div>'
        : '<div class="hint">Nothing yet.</div>'}];
    // Where it goes.
    const picks=[];
    if(info.last) picks.push(['The last patch', info.last]);
    for(const f of info.folders||[]) picks.push([f, f.replace(/[\\/]+$/,'')+(f.includes('\\')? '\\' : '/')+info.defaultName]);
    sec.push({key:'where', title:'Where it goes', html:
      '<div class="edVec"><input class="fld" id="edBuildPath" spellcheck="false" placeholder="The patch file, folder and all" value="'+escHtml(this._buildPath||info.suggested||'')+'" title="The plugin the patch is written as. Load it last">'+
      '<button class="btn sm" id="edBuildBrowse" title="Choose the file with the system\'s dialog">Browse…</button></div>'+
      (picks.length? '<div class="edChips">'+picks.slice(0,8).map(([label, path])=>
        '<button class="btn dim sm" data-bpath="'+escHtml(path)+'" title="'+escHtml(path)+'">'+escHtml(label.length>46? '…'+label.slice(-45) : label)+'</button>').join('')+'</div>' : '')+
      '<div id="edBuildState" class="hint"></div>'+
      '<div id="edBuildMode" hidden><label title="Keep what the file already carries and add the pool on top: one patch built over several sessions"><input type="radio" name="edBuildMode" value="append" checked> Append</label> '+
      '<label title="Throw away what the file carries and write only the pool"><input type="radio" name="edBuildMode" value="replace"> Replace</label></div>'});
    sec.push({key:'report', title:'Report', html:'<pre id="edBuildLog" class="edLog">Nothing written yet.</pre>'});
    h+=WgUI.sectionsHtml('BUILD', sec)+
      '<div class="hint">Your mods are not changed: the patch is one new plugin. Load it last, and back up your saves.</div>'+
      '<div class="orirow"><span class="v"><button class="btn pri" id="edBuildGo"'+(S.records? '' : ' disabled')+' title="Write the patch where it says">Write the patch</button> '+
      '<button class="btn sm" id="edBuildRefresh" title="Read the pool again">Refresh</button></span></div>'+
      '<div id="edBuildDone"></div>';
    body.innerHTML=h;
    WgUI.wireSections(body);
    const path=body.querySelector('#edBuildPath');
    const check=async()=>{
      this._buildPath=path.value.trim();
      const st=body.querySelector('#edBuildState'), mode=body.querySelector('#edBuildMode');
      mode.hidden=true; path.classList.remove('bad');
      if(!this._buildPath){ st.textContent=''; return null; }
      try{
        const c=await this.ask('editBuildCheck', {path:this._buildPath});
        mode.hidden=!c.exists;
        st.textContent= c.exists? 'This file is there already: append the pool to it, or replace it.'+(c.inOrder? ' It is in the load order.' : '')
          : 'A new file.'+(c.inOrder? ' A plugin of this name is in the load order.' : '');
        this._buildExists=!!c.exists;
        return c;
      }catch(e){ path.classList.add('bad'); st.textContent=String(e.message||e).replace(/^Wraithguard answered 400:\s*/,''); return null; }
    };
    path.onchange=check;
    path.onkeydown=e=>{ if(e.key==='Enter') path.blur(); e.stopPropagation(); };
    body.querySelectorAll('[data-bpath]').forEach(b=>b.onclick=()=>{ path.value=b.dataset.bpath; check(); });
    body.querySelector('#edBuildBrowse').onclick=async()=>{
      let p=null;
      try{ p=await Engine.pick('save', {defaultPath:path.value||info.suggested||info.defaultName, filters:[{name:'Morrowind plugin', extensions:['esp']},{name:'OpenMW addon', extensions:['omwaddon']}]}); }
      catch(e){ toast(String(e.message||e),'err',4000); }
      if(p){ path.value=String(p); check(); }
    };
    body.querySelector('#edBuildRefresh').onclick=()=>this.showBuild();
    body.querySelector('#edBuildGo').onclick=()=>this.build();
    body.querySelectorAll('[data-bopen]').forEach(b=>b.onclick=()=>{
      const p=list[+b.dataset.bopen];
      if(p.new) this.openRef({cell:p.new.cell, uid:p.new.uid});
      else if(p.ref) this.openRef(p.ref);
      else if(p.tag) this.openRecord(p.tag, p.id, null);
    });
    body.querySelectorAll('[data-bdrop]').forEach(b=>b.onclick=e=>{ e.stopPropagation(); this.dropPending([list[+b.dataset.bdrop]]); });
    // Selecting rows: click, Ctrl+click, Shift+click; Delete (or the button) drops them.
    this._bSel=new Set([...(this._bSel||[])].filter(k=>list.some(p=>this.pendKey(p)===k)));
    this._bList=list;
    const box=body.querySelector('.edBuildList'), delBtn=body.querySelector('#edBuildDel');
    const paint=()=>{
      body.querySelectorAll('tr[data-i]').forEach(tr=>tr.classList.toggle('sel', this._bSel.has(this.pendKey(list[+tr.dataset.i]))));
      const n=[...this._bSel].length;
      if(delBtn){ delBtn.disabled=!n; delBtn.textContent='✕ Delete selected'+(n? ' ('+n+')' : ''); }
    };
    body.querySelectorAll('tr[data-i]').forEach(tr=>{
      tr.onclick=e=>{
        if(e.target.closest('button')) return;
        const i=+tr.dataset.i, k=this.pendKey(list[i]);
        if(e.shiftKey && this._bAnchor!=null){
          const [a,b]=[Math.min(this._bAnchor,i), Math.max(this._bAnchor,i)];
          if(!(e.ctrlKey||e.metaKey)) this._bSel.clear();
          for(let j=a;j<=b;j++) this._bSel.add(this.pendKey(list[j]));
        }else if(e.ctrlKey||e.metaKey){
          if(this._bSel.has(k)) this._bSel.delete(k); else this._bSel.add(k);
          this._bAnchor=i;
        }else{ this._bSel=new Set([k]); this._bAnchor=i; }
        paint();
        if(box) box.focus({preventScroll:true});
      };
      tr.ondblclick=()=>{ const b=tr.querySelector('[data-bopen]'); if(b) b.click(); };
    });
    if(delBtn) delBtn.onclick=()=>this.dropSelectedPending();
    const all=body.querySelector('#edBuildAll');
    if(all) all.onclick=()=>{ this._bSel=new Set(list.map(p=>this.pendKey(p))); paint(); if(box) box.focus({preventScroll:true}); };
    paint();
    check();
  },

  /** A pool entry's identity across redraws. */
  pendKey(p){
    if(p.new) return 'new:'+p.new.cell+':'+p.new.uid;
    if(p.ref) return 'ref:'+String(p.ref.cell||'')+':'+p.ref.origin+':'+p.ref.refr;
    return 'rec:'+(p.tag||p.type)+':'+String(p.id).toLowerCase();
  },
  canDrop(p){ return !!(p.tag || p.ref || p.new); },

  /** Takes entries out of the pool (a record's changes or whole copy, a placed object's
   *  changes, a new placed object), then reads the pool again. */
  async dropPending(items){
    items=(items||[]).filter(p=>p && this.canDrop(p));
    if(!items.length) return 0;
    let n=0;
    for(const p of items){
      try{
        if(p.new) await this.ask('editNewRemove', {cell:p.new.cell, uid:p.new.uid});
        else if(p.ref) await this.ask('editRefRevert', this.refBody(p.ref));
        else await this.ask('editRevert', {tag:p.tag, id:p.id});
        n++;
        if(this._bSel) this._bSel.delete(this.pendKey(p));
      }catch(e){ toast(String(e.message||e).replace(/^Wraithguard answered 400:\s*/,''),'err',5000); break; }
    }
    if(n>1) toast(n+' taken out of the patch','ok',2000);
    await this.showBuild();
    return n;
  },
  dropSelectedPending(){
    const list=this._bList||[], sel=this._bSel||new Set();
    const items=list.filter(p=>sel.has(this.pendKey(p)));
    if(!items.length){ toast('Select what to take out first (click a row)','warn',2000); return; }
    return this.dropPending(items);
  },

  /** Writes the patch where the panel says, and follows the build to its end. */
  async build(){
    const body=this.bld && this.bld.querySelector('#edBuildBody');
    if(!body) return null;
    const path=body.querySelector('#edBuildPath').value.trim();
    const c=await this.ask('editBuildCheck', {path}).catch(e=>{ toast(String(e.message||e).replace(/^Wraithguard answered 400:\s*/,''),'err',5000); return null; });
    if(!c) return null;
    const chosen=body.querySelector('input[name="edBuildMode"]:checked');
    const mode=c.exists? (chosen? chosen.value : 'append') : 'new';
    const go=body.querySelector('#edBuildGo'), log=body.querySelector('#edBuildLog'), done=body.querySelector('#edBuildDone');
    const rep=body.querySelector('details[data-sec="BUILD:report"]'); if(rep) rep.open=true;
    go.disabled=true; go.textContent='Writing…'; log.textContent=''; done.innerHTML='';
    let job;
    try{ job=(await this.ask('editBuild', {path, mode})).job; }
    catch(e){
      go.disabled=false; go.textContent='Write the patch';
      log.textContent=String(e.message||e).replace(/^Wraithguard answered 400:\s*/,'');
      return null;
    }
    let seen=0, st=null;
    for(let i=0;i<3600;i++){
      await new Promise(r=>setTimeout(r, i<10? 150 : 400));
      try{ st=await this.ask('editBuildStatus', {job, from:seen}); }
      catch(e){ log.textContent+='\n'+String(e.message||e); break; }
      if(st.lines && st.lines.length){ log.textContent+=(log.textContent? '\n' : '')+st.lines.join('\n'); seen=st.total; log.scrollTop=log.scrollHeight; }
      if(st.state!=='running') break;
    }
    go.textContent='Write the patch';
    if(!st || st.state!=='done'){
      go.disabled=false;
      const msg=st && st.error? st.error : 'The build did not finish';
      done.innerHTML='<div class="hint edBad">'+escHtml(msg)+'</div>';
      toast('The patch was not written: '+msg,'err',8000);
      return st;
    }
    const r=st.result;
    done.innerHTML='<div class="hint edGood">'+escHtml(r.note).replace(/\n/g,'<br>')+'</div>'+
      '<div class="orirow"><span class="v"><button class="btn sm" id="edBuildPreview" title="A new Cell Preview with the patch loaded last, on the cells it changes">Preview it in a new Cell Preview</button> '+
      '<button class="btn sm" id="edBuildCopy" title="The patch\'s path, to the clipboard">Copy the path</button></span></div>';
    done.querySelector('#edBuildPreview').onclick=async()=>{
      try{ await this.ask('editPreviewPatch', {path:r.output}); toast('Opening a Cell Preview with the patch','ok',3000); }
      catch(e){ toast(String(e.message||e).replace(/^Wraithguard answered 400:\s*/,''),'err',5000); }
    };
    done.querySelector('#edBuildCopy').onclick=async()=>{ try{ await navigator.clipboard.writeText(r.output); toast('Copied','ok',1500); }catch(_){ } };
    toast(r.records+' record'+(r.records===1?'':'s')+' written to '+String(r.output).split(/[\\/]/).pop(),'ok',6000);
    this._buildPath=r.output;
    await this.refreshPending();
    return st;
  },
});
