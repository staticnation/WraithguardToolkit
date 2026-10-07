/* =====================================================================================
   Wraithguard Editor (50_wg_editor.js): the Use Report: every record and cell that names a record, and Search & Replace.

   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
Object.assign(WgEditor, {
  /* ---- the Use Report ------------------------------------------------------------------ */

  /** Opens the Use Report for a record: Wraithguard reads every plugin for it. */
  async showUses(tag, id, plugins){
    if(!this.uses){
      const d=document.createElement('div');
      d.id='edUsesPanel'; d.className='ori'; d.hidden=true;
      d.innerHTML='<div class="orihead"><b id="edUsesTitle">Use Report</b><span style="flex:1"></span>'+
        '<button class="btn dim ic" id="edUsesX" title="Close">&#x2715;</button></div><div class="oribody" id="edUsesBody"></div>';
      WgUI.mount(d);
      d.querySelector('#edUsesX').onclick=()=>{ d.hidden=true; };
      this.uses=d;
    }
    const d=this.uses, body=d.querySelector('#edUsesBody');
    d.hidden=false;
    d.querySelector('#edUsesTitle').textContent='Use Report - '+id;
    body.innerHTML='<div class="hint">Reading every plugin of the load order for '+escHtml(id)+'…</div>';
    let r;
    try{ r=await this.ask('editUses', Object.assign({tag, id}, plugins? {plugins} : {})); }
    catch(e){ body.innerHTML='<div class="hint">'+escHtml(String(e.message||e))+'</div>'; return null; }
    this._usesOf={tag, id, plugins};
    this.drawUses(r);
    return r;
  },

  drawUses(r){
    const body=this.uses.querySelector('#edUsesBody');
    const uses=r.uses||[];
    let h='<div class="hint">'+(uses.length
      ? r.live+' live use'+(r.live===1?'':'s')+(r.cells? ', '+r.cells+' placed in cells' : '')+'. Greyed: in a version a later plugin overrides.'
      : 'Nothing in the load order names '+escHtml(r.id)+'.')+'</div>';
    const cells=uses.filter(u=>u.type==='Cell'), other=uses.filter(u=>u.type!=='Cell');
    if(other.length){
      h+='<div class="orisec">Records</div><table class="edT"><tbody>';
      other.forEach((u,i)=>{
        h+='<tr data-u="'+uses.indexOf(u)+'"'+(u.wins? '' : ' class="from" title="'+escHtml(u.plugin)+'\'s version is overridden by a later plugin"')+'>'+
          '<td>'+escHtml(this.NAMES[u.tag]||u.tag||u.type)+'</td><td>'+escHtml(u.key)+'</td>'+
          '<td class="from" title="'+escHtml(u.paths.join(', '))+'">'+escHtml(u.paths.slice(0,2).join(', ')+(u.paths.length>2? '…' : ''))+'</td>'+
          '<td class="from">'+escHtml(u.plugin)+'</td></tr>';
      });
      h+='</tbody></table>';
    }
    if(cells.length){
      h+='<div class="orisec">Placed in</div><table class="edT"><tbody>';
      cells.forEach(u=>{
        h+='<tr data-u="'+uses.indexOf(u)+'"><td>'+escHtml(u.key)+'</td><td class="num">'+u.count+'×</td><td class="from">'+escHtml(u.plugin)+'</td></tr>';
      });
      h+='</tbody></table>';
    }
    if(uses.some(u=>u.wins && u.type!=='Script'))
      h+='<div class="orirow" style="margin-top:6px"><span class="v edVec"><input class="fld" id="edReplId" placeholder="Another '+escHtml(this.NAMES[r.tag]||r.tag)+' id" spellcheck="false">'+
        '<button class="btn sm" id="edRepl" title="Every live use above names that record instead, and every placed one becomes it - queued in the patch pool. Scripts are left as they are">Replace with</button>'+
        (cells.length && this.currentCell()? '<button class="btn sm" id="edReplCell" title="Only the placed ones in the cell on screen ('+escHtml(this.currentCell().label)+') become it - no record changes">In this cell</button>' : '')+
        '</span></div>';
    body.innerHTML=h;
    const rb=body.querySelector('#edRepl'), ri=body.querySelector('#edReplId'), rc=body.querySelector('#edReplCell');
    if(rb){
      ri.onkeydown=e=>{ if(e.key==='Enter') rb.onclick(); e.stopPropagation(); };
      const run=async(cell)=>{
        const newId=ri.value.trim(), of=this._usesOf;
        if(!newId || !of){ ri.classList.add('bad'); return; }
        try{
          const x=await this.ask('editReplace', Object.assign({tag:of.tag, id:of.id, newId}, of.plugins? {plugins:of.plugins} : {}, cell? {cell:cell.key} : {}));
          const recs=x.changed-(x.refs||0);
          toast(cell? (x.refs? x.refs+' placed in '+cell.label+' became '+newId : 'Nothing placed in '+cell.label+' names it')
                : recs+' record'+(recs===1?'':'s')+' now name '+newId+(x.refs? ', '+x.refs+' placed reference'+(x.refs===1?'':'s')+' became it' : '')+
                (x.scripts? '; '+x.scripts+' in script text, scripts recompiled' : ''),'ok',6000);
          await this.loadModels(of.tag);
          await this.refreshPending();
        }catch(e){
          ri.classList.add('bad');
          toast(String(e.message||e).replace(/^Wraithguard answered 400:\s*/,''),'err',6000);
        }
      };
      rb.onclick=()=>run(null);
      if(rc) rc.onclick=()=>run(this.currentCell());
    }
    body.querySelectorAll('tr[data-u]').forEach(tr=>{
      const u=uses[+tr.dataset.u];
      tr.style.cursor='pointer';
      tr.onclick=()=>{
        if(u.type!=='Cell'){ if(u.tag) this.openRecord(u.tag, u.key, null); return; }
        const m=/^\((-?\d+), (-?\d+)\)$/.exec(u.key);
        if(typeof WgNav==='object') WgNav.go(m? m[1]+','+m[2] : 'int:'+u.key);
      };
    });
  },
});
