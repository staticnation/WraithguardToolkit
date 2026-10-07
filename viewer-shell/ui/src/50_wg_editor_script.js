/* =====================================================================================
   Wraithguard Editor (50_wg_editor.js): the Script Edit window: checked as typed, compiled, saved to the patch.

   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
Object.assign(WgEditor, {
  /* ---- the Script Edit window ------------------------------------------------------------ */

  async openScript(tag, id, plugins){
    if(!this.scr){
      const d=document.createElement('div');
      d.id='edScript'; d.className='ori'; d.hidden=true;
      d.innerHTML='<div class="orihead"><b id="edScriptTitle">Script</b><span style="flex:1"></span>'+
        '<button class="btn sm" data-stab="src">Source</button> <button class="btn sm" data-stab="lst">Compiled</button> '+
        '<button class="btn dim ic" id="edScriptX" title="Close">&#x2715;</button></div>'+
        '<div class="oribody"><div id="edScriptSrc"><textarea id="edScriptText" spellcheck="false" wrap="off"></textarea>'+
        '<div id="edScriptFind"></div><div class="orirow"><span class="v">'+
        '<button class="btn sm" id="edScriptSave" title="The text goes to the patch pool, as a change to this script">Save to the patch</button> '+
        '<button class="btn sm" id="edScriptRevert" title="Drop the changed text waiting in the pool">Revert</button> '+
        '<button class="btn sm" id="edScriptFlow" title="The script\'s control flow - its ifs, whiles and returns - as a flowchart, in a Wraithguard window">Flowchart</button></span></div>'+
        '<div class="hint" id="edScriptNote"></div></div>'+
        '<pre id="edScriptListing" hidden></pre></div>';
      WgUI.mount(d);
      d.querySelector('#edScriptX').onclick=()=>{ d.hidden=true; };
      d.querySelectorAll('[data-stab]').forEach(b=>b.onclick=()=>{
        const src=b.dataset.stab==='src';
        d.querySelector('#edScriptSrc').hidden=!src; d.querySelector('#edScriptListing').hidden=src;
        d.querySelectorAll('[data-stab]').forEach(x=>x.classList.toggle('on', x===b));
      });
      const ta=d.querySelector('#edScriptText');
      ta.onkeydown=e=>{
        e.stopPropagation();
        if(e.key==='Tab'){ e.preventDefault(); const a=ta.selectionStart; ta.value=ta.value.slice(0,a)+'    '+ta.value.slice(ta.selectionEnd); ta.selectionStart=ta.selectionEnd=a+4; ta.oninput(); }
      };
      ta.oninput=()=>{ clearTimeout(this._scrT); this._scrT=setTimeout(()=>this.checkScript(), 400); };
      d.querySelector('#edScriptSave').onclick=()=>this.saveScript();
      d.querySelector('#edScriptFlow').onclick=async()=>{
        const s=this._script; if(!s) return;
        const text=d.querySelector('#edScriptText').value;
        try{
          const r=await this.ask('editFlowchart', Object.assign({tag:s.tag, id:s.id, text}, s.plugins? {plugins:s.plugins} : {}));
          if(!r.shown) toast('This Wraithguard has no chart window','warn',4000);
        }catch(e){ toast(String(e.message||e),'err',5000); }
      };
      d.querySelector('#edScriptRevert').onclick=async()=>{
        const s=this._script; if(!s) return;
        try{ await this.ask('editRevert', Object.assign({tag:s.tag, id:s.id, path:'text'}, s.plugins? {plugins:s.plugins} : {})); }
        catch(e){ toast(String(e.message||e),'err',5000); }
        await this.refreshPending();
        this.openScript(s.tag, s.id, s.plugins);
      };
      this.scr=d;
      if(typeof WgFlow==='object') WgFlow.extendScript(this, d);
    }
    const d=this.scr;
    this._script={tag, id, plugins};
    d.hidden=false;
    d.querySelector('#edScriptTitle').textContent='Script - '+id;
    d.querySelector('[data-stab="src"]').onclick();
    let v;
    try{ v=await this.ask('editScript', Object.assign({tag, id}, plugins? {plugins} : {})); }
    catch(e){ d.querySelector('#edScriptFind').innerHTML='<div class="hint">'+escHtml(String(e.message||e))+'</div>'; return null; }
    d.querySelector('#edScriptText').value=v.text;
    d.querySelector('#edScriptListing').textContent=v.listing||'; no compiled data in this record';
    d.querySelector('#edScriptNote').textContent=(v.queued? 'A changed text waits in the pool. ' : '')+
      'OpenMW compiles this text when it loads; Morrowind.exe runs the compiled data, which saving here rebuilds when the text compiles.';
    this.drawFindings(this.withCompile(v));
    return v;
  },

  /** A response's result script, compiled for its errors (Morrowind compiles it when the
   *  response fires; the Construction Set checks it on save), shown under its fields. */
  async checkResult(v){
    const box=this.dlg && this.dlg.querySelector('#edResult'); if(!box) return;
    const f=v.fields.find(x=>x.path==='script_text');
    const text=f? String(('queued' in f)? f.queued : f.value||'') : '';
    if(!text.trim()){ box.innerHTML=''; return; }
    box.innerHTML='<div class="hint">Compiling the result…</div>';
    let r;
    try{ r=await this.ask('editResult', this.req()); }
    catch(e){ box.innerHTML='<div class="hint">'+escHtml(String(e.message||e))+'</div>'; return; }
    if(!this.dlg.contains(box)) return;
    const msgs=r.messages||[];
    box.innerHTML='<div class="k">Result script'+(r.speaker_script? ' (with its speaker\'s script\'s locals)' : '')+'</div>'+
      (msgs.length
        ? msgs.map(m=>'<div class="it '+(m.level==='error'?'bad':'')+'"><span class="k">'+m.line+'</span> '+escHtml(m.message)+'</div>').join('')
        : '<div class="hint">It compiles.</div>');
  },

  async checkScript(){
    const s=this._script; if(!s || !this.scr) return;
    const text=this.scr.querySelector('#edScriptText').value;
    try{
      const v=await this.ask('editScript', Object.assign({tag:s.tag, id:s.id, text}, s.plugins? {plugins:s.plugins} : {}));
      if(this.scr.querySelector('#edScriptText').value===text) this.drawFindings(this.withCompile(v));
    }catch(_){ }
  },

  /** The checks' findings and the compiler's messages, in line order. */
  withCompile(v){
    const c=v.compile||{messages:[]};
    const msgs=(c.messages||[]).map(m=>({line:m.line, level:m.level, message:'Compiler: '+m.message}));
    return (v.findings||[]).concat(msgs).sort((a,b)=>a.line-b.line);
  },

  drawFindings(list){
    const box=this.scr.querySelector('#edScriptFind');
    this._findings=list||[];
    box.innerHTML=this._findings.length
      ? this._findings.map((f,i)=>'<div class="it '+(f.level==='error'?'bad':'')+'" data-f="'+i+'"><span class="k">'+f.line+'</span> '+escHtml(f.message)+'</div>').join('')
      : '<div class="hint">No problems found.</div>';
    box.querySelectorAll('[data-f]').forEach(el=>el.onclick=()=>{
      const f=this._findings[+el.dataset.f], ta=this.scr.querySelector('#edScriptText');
      const lines=ta.value.split('\n'); let a=0;
      for(let i=0;i<f.line-1 && i<lines.length;i++) a+=lines[i].length+1;
      ta.focus(); ta.selectionStart=a; ta.selectionEnd=a+(lines[f.line-1]||'').length;
    });
  },

  async saveScript(){
    const s=this._script; if(!s) return;
    const text=this.scr.querySelector('#edScriptText').value;
    if(this._findings.some(f=>f.level==='error') && !window.confirm('The script has errors the compiler would refuse. Save it to the patch anyway?')) return;
    try{
      const r=await this.ask('editSet', Object.assign({tag:s.tag, id:s.id, path:'text', value:text}, s.plugins? {plugins:s.plugins} : {}));
      const built=r && r.compile && r.compile.compiled;
      toast(built? 'The changed text and its compiled data are in the patch pool' : 'The changed text is in the patch pool (not compiled: '+((r && r.compile && r.compile.messages.some(m=>m.level==='error'))? 'it has errors' : 'the Rust backend is not built')+')', built?'ok':'warn', 4000);
      await this.refreshPending();
      this.scr.querySelector('#edScriptNote').textContent='A changed text waits in the pool. '+this.scr.querySelector('#edScriptNote').textContent.replace(/^A changed text waits in the pool\. /,'');
    }catch(e){ toast(String(e.message||e).replace(/^Wraithguard answered 400:\s*/,''),'err',6000); }
  },
});
