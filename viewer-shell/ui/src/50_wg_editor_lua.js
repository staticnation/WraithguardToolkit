/* =====================================================================================
   Wraithguard Editor (50_wg_editor.js): the Lua panel: the load order's OpenMW Lua scripts checked, and their charts.

   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
Object.assign(WgEditor, {
  /* ---- the Lua panel ------------------------------------------------------------------- */

  /** The load order's OpenMW Lua scripts, scanned by the engine (`lua_scan`: luacore over
      the overlay, so packed scripts count). Findings first, most serious first; a click on
      one shows its script's line in the list below. */
  async showLua(){
    if(!this.lua){
      const d=document.createElement('div');
      d.id='edLuaPanel'; d.className='ori'; d.hidden=true;
      d.innerHTML='<div class="orihead"><b>Lua scripts</b><span style="flex:1"></span>'+
        '<label class="from" title="Show notes as well as errors and warnings"><input type="checkbox" id="edLuaInfo"> Notes</label> '+
        '<button class="btn sm" id="edLuaRe" title="Scan again">Rescan</button> '+
        '<button class="btn sm" id="edLuaCopy" title="The report as text, for a bug report or a mod page">Copy report</button> '+
        '<button class="btn dim ic" id="edLuaX" title="Close">&#x2715;</button></div><div class="oribody" id="edLuaBody"></div>';
      WgUI.mount(d);
      d.querySelector('#edLuaX').onclick=()=>{ d.hidden=true; };
      d.querySelector('#edLuaRe').onclick=()=>this.showLua();
      d.querySelector('#edLuaInfo').onchange=()=>{ if(this._lua) this.drawLua(this._lua); };
      d.querySelector('#edLuaCopy').onclick=async()=>{
        if(!this._lua) return;
        try{ await navigator.clipboard.writeText(this._lua.report||''); toast('Lua report copied','ok',2500); }
        catch(e){ toast('Could not copy: '+String(e.message||e),'err',4000); }
      };
      this.lua=d;
    }
    const d=this.lua, body=d.querySelector('#edLuaBody');
    d.hidden=false;
    body.innerHTML='<div class="hint">Reading the load order\'s Lua scripts…</div>';
    let r;
    try{ r=await Engine.call('lua_scan',{}); }
    catch(e){ body.innerHTML='<div class="hint">'+escHtml(String(e.message||e))+'</div>'; return null; }
    if(typeof r==='string') r=JSON.parse(r);
    this._lua=r;
    this.drawLua(r);
    return r;
  },

  drawLua(r){
    const body=this.lua.querySelector('#edLuaBody');
    const notes=this.lua.querySelector('#edLuaInfo').checked;
    const all=r.findings||[], scripts=r.scripts||[];
    const count=s=>all.filter(f=>f.severity===s).length;
    let h='<div class="hint">'+escHtml(r.note||'')+'</div>';
    if(r.openmw) h+='<div class="hint">OpenMW '+escHtml(r.openmw)+(r.revision? ' (Lua API '+r.revision+')' : '')+': '+
      scripts.length+' script'+(scripts.length===1?'':'s')+', '+count('error')+' error(s), '+count('warn')+' warning(s), '+count('info')+' note(s)</div>';
    const shown=all.filter(f=>notes || f.severity!=='info');
    if(shown.length){
      h+='<div class="orisec">Findings</div><table class="edT"><tbody>';
      shown.forEach(f=>{
        const mark=f.severity==='error'? 'ERROR' : f.severity==='warn'? 'warn' : 'note';
        h+='<tr data-p="'+escHtml(f.path)+'" title="'+escHtml(f.message)+'"><td class="'+(f.severity==='error'? 'bad' : 'from')+'">'+mark+'</td>'+
          '<td>'+escHtml(f.code)+'</td><td>'+escHtml((f.path||'(load order)')+(f.line? ':'+f.line : ''))+'</td>'+
          '<td class="from">'+escHtml(f.message)+'</td></tr>';
      });
      h+='</tbody></table>';
    }else if(scripts.length){
      h+='<div class="hint">Nothing to report'+(notes? '' : ' (notes hidden)')+'.</div>';
    }
    if(scripts.length){
      h+='<div class="orisec">Scripts, in load order</div><table class="edT"><tbody>';
      scripts.forEach(s=>{
        h+='<tr data-s="'+escHtml(s.path)+'"><td>'+escHtml(s.path)+'</td><td class="from">'+escHtml(s.flags.join(', '))+'</td>'+
          '<td class="from">'+escHtml(s.interface? 'interface '+s.interface : '')+'</td>'+
          '<td class="from">'+escHtml(s.handlers.join(', '))+'</td>'+
          '<td class="'+(s.file? 'from' : 'bad')+'" title="'+escHtml(s.file||'in no data folder or archive')+'">'+
            (s.file? (s.providers>1? s.providers+' providers' : '') : 'missing')+'</td>'+
          '<td>'+(s.file? '<button class="btn dim sm" data-lflow="'+escHtml(s.path)+'" title="The script\'s control flow as a flowchart - the whole script, or one of its functions - in a Wraithguard window">Flowchart ▾</button>'+
            '<button class="btn dim sm" data-lcalls="'+escHtml(s.path)+'" title="Which of the script\'s functions call which, in a Wraithguard window">Calls</button>' : '')+'</td></tr>';
      });
      h+='</tbody></table>';
    }
    body.innerHTML=h;
    body.querySelectorAll('[data-lflow]').forEach(b=>b.onclick=e=>{ e.stopPropagation(); this.luaFlowMenu(b, b.dataset.lflow); });
    body.querySelectorAll('[data-lcalls]').forEach(b=>b.onclick=e=>{ e.stopPropagation(); this.luaChart(b.dataset.lcalls, 'calls'); });
    body.querySelectorAll('tr[data-p]').forEach(tr=>{
      tr.onclick=()=>{
        const row=[...body.querySelectorAll('tr[data-s]')].find(x=>x.dataset.s===tr.dataset.p);
        if(row){ row.scrollIntoView({block:'center'}); row.classList.add('sel'); setTimeout(()=>row.classList.remove('sel'),1200); }
      };
    });
  },

  /** A script's text as the setup has it (the winning file, packed or loose). */
  async luaText(path){
    const buf=await Engine.bytes('read_asset', {path});
    return new TextDecoder('utf-8').decode(new Uint8Array(buf));
  },
  /** A Lua script's flowchart (whole, or the function at `line`) or call graph, opened in
   *  Wraithguard's chart window (`editLuaChart`). */
  async luaChart(path, kind, line){
    if(!this.canEdit()) return toast('Charts open in Wraithguard: open the viewer from Wraithguard (Cell Preview)','warn',4000);
    try{
      const text=await this.luaText(path);
      const r=await this.ask('editLuaChart', Object.assign({path, text, kind}, line? {line} : {}));
      if(r && r.shown===false) toast('This Wraithguard has no chart window','warn',4000);
      return r;
    }catch(e){ toast(String(e.message||e).replace(/^Wraithguard answered 400:\s*/,''),'err',6000); return null; }
  },
  /** "Flowchart ▾": the whole script, or one of its functions (Wraithguard lists them). */
  async luaFlowMenu(at, path){
    let fns=[];
    try{ const r=await this.luaChart(path, 'functions'); fns=(r && r.functions)||[]; }catch(_){ fns=[]; }
    WgUI.menu(at, [{head:path}, {label:'The whole script', act:()=>this.luaChart(path, 'flow')}].concat(
      fns.length? [{sep:true}].concat(fns.slice(0,60).map(f=>({label:f.name, key:'line '+f.line, act:()=>this.luaChart(path, 'flow', f.line)}))) : []));
  },
});
