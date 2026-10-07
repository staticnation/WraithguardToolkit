/* =====================================================================================
   Wraithguard Editor (50_wg_editor.js): the Cell View: every cell, the references in the one selected, the cell on screen and copying one.

   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
Object.assign(WgEditor, {
  /* ---- the cell view ---------------------------------------------------------------- */

  async loadCells(){
    try{
      const c=await Engine.call('cells',{});
      const ext=(c.grid||[]).map(g=>({kind:'ext', x:g[0], y:g[1], name:g[2]||'', label:(g[2]? g[2]+' ' : 'Wilderness ')+'('+g[0]+', '+g[1]+')',
        refs:g[4], path:g[5]}));
      ext.sort((a,b)=>a.x-b.x || a.y-b.y);
      const int=(c.interiors||[]).map(r=>({kind:'int', name:r[0], label:r[0], refs:r[1], path:r[2]}));
      this.cells=[...int, ...ext];
    }catch(e){ toast(String(e.message||e),'err',5000); this.cells=[]; }
    this.drawCells();
  },

  drawCells(){
    const box=this.part('edCellList'); if(!box) return;
    const f=this.cellFilter.trim().toLowerCase();
    let list=this.cells||[];
    if(f) list=list.filter(c=>c.label.toLowerCase().includes(f));
    const edOf=c=>this.editedCells.has(c.kind==='int'? String(c.name).toLowerCase() : '('+c.x+', '+c.y+')');
    if(this.cellsModOnly) list=list.filter(edOf);
    // The Construction Set's columns: name, grid, references, pathgrid; sortable.
    const CS=[['Cell name', c=>c.kind==='int'? c.name : (c.name||'Wilderness')],
              ['Grid', c=>c.kind==='int'? '' : c.x+', '+c.y],
              ['Ref count', c=>c.refs==null? '' : c.refs],
              ['Path', c=>c.path==null? '' : (c.path? 'Y' : 'N')]];
    const sc=this.cellSort||{col:-1, dir:1};
    if(sc.col>=0){
      const g=CS[sc.col][1];
      const key=c=>sc.col===1? (c.kind==='int'? -1e9 : c.x*1e5+c.y) : (typeof g(c)==='number'? g(c) : String(g(c)).toLowerCase());
      list=list.slice().sort((a,b)=>{ const x=key(a), y=key(b); return (x<y?-1:x>y?1:0)*sc.dir; });
    }
    let h='<table class="edT"><thead><tr>'+CS.map(([l],k)=>'<th data-ccol="'+k+'">'+l+(k===sc.col? (sc.dir>0?' ▲':' ▼') : '')+'</th>').join('')+'</tr></thead><tbody>';
    for(const c of list.slice(0,this.limit('cells'))){
      const k=c.kind==='int'? 'int:'+c.name : c.x+','+c.y;
      const ed=edOf(c);
      const cls=[this.cellSel===k? 'sel' : '', ed? 'edited' : ''].filter(Boolean).join(' ');
      h+='<tr data-cell="'+escHtml(k)+'"'+(cls? ' class="'+cls+'"' : '')+(ed? ' title="The patch changes or adds references here"' : '')+'>'+
        '<td>'+escHtml(CS[0][1](c))+'</td><td class="ctr">'+escHtml(CS[1][1](c))+'</td><td class="num">'+escHtml(String(CS[2][1](c)))+'</td>'+
        '<td class="ctr'+(c.path? ' yes' : '')+'">'+CS[3][1](c)+'</td></tr>';
    }
    h+='</tbody></table>';
    h+=this.moreHtml(list.length, 'cells');
    const keep=box.scrollTop;
    box.innerHTML=h;
    box.scrollTop=keep;
    WgUI.sizeColumns(box.querySelector('table'), 'cells');
    this.wireMore(box, 'cells', list.length, ()=>this.drawCells());
    box.querySelectorAll('th[data-ccol]').forEach(th=>th.onclick=()=>{
      const k=+th.dataset.ccol, cur=this.cellSort||{col:-1, dir:1};
      this.cellSort={col:k, dir:cur.col===k? -cur.dir : 1}; this.drawCells();
    });
    box.querySelectorAll('tr[data-cell]').forEach(tr=>{
      tr.onclick=()=>{ this.cellSel=tr.dataset.cell; box.querySelectorAll('tr.sel').forEach(x=>x.classList.remove('sel')); tr.classList.add('sel'); this.loadRefs(tr.dataset.cell); };
      tr.ondblclick=()=>{ if(typeof WgNav==='object') WgNav.go(tr.dataset.cell); };
      tr.oncontextmenu=e=>{
        e.preventDefault();
        const k=tr.dataset.cell, inner=k.startsWith('int:');
        WgUI.menu({x:e.clientX, y:e.clientY}, [
          {head:inner? k.slice(4) : 'Exterior '+k},
          {label:'Go there', act:()=>{ if(typeof WgNav==='object') WgNav.go(k); }},
          {label:'Make a copy as…', disabled:!inner || !this.canEdit(), title:inner? 'A new interior of the patch\'s own: every reference placed anew, the path grid with it' : 'Only an interior can be copied: an exterior is a grid square',
           act:()=>this.copyCell(k.slice(4))},
          {sep:true},
          {label:'New interior…', disabled:!this.links().editNewCell, title:'A blank interior of the patch\'s own, under a name', act:()=>WgFlow.newCell(false)},
          {label:'New exterior square…', disabled:!this.links().editNewCell, title:'A grid square no plugin has, of the patch\'s own (no land: the game draws it flat, at the sea)', act:()=>WgFlow.newCell(true)},
        ]);
      };
    });
  },

  /** One of the Editor's parts by id, wherever its pane is docked (a pane moved into
   *  another panel is no longer inside `this.el`). */
  part(id){
    return (this.el && this.el.querySelector('#'+id)) || document.getElementById(id);
  },

  async loadRefs(cell){
    const box=this.part('edRefList'); if(!box) return;
    box.innerHTML='<div class="hint">…</div>';
    let r;
    try{ r=await Engine.call('editor_cell_refs',{cell}); }
    catch(e){ box.innerHTML='<div class="hint">'+escHtml(String(e.message||e))+'</div>'; return; }
    this.refs=r.rows||[];
    const refCell=r.refCell||'', cellPlugins=r.plugins||null;
    const nm=this.part('edCellName'); if(nm) nm.textContent=(r.name||cell)+' - '+this.refs.length+' references';
    // The Construction Set's columns, Ownership among them (an NPC, or a faction and rank).
    let h='<table class="edT"><thead><tr><th>Object ID</th><th>Type</th><th>Ownership</th><th>From</th></tr></thead><tbody>';
    this.refs.forEach((x,i)=>{
      const ic=this.ICONS[x[2]]||'';
      h+='<tr data-i="'+i+'"><td>'+(ic? '<span class="edIco">'+ic+'</span>' : '')+escHtml(x[1])+'</td><td>'+escHtml(this.NAMES[x[2]]||x[2])+'</td>'+
        '<td title="'+escHtml(x[5]||'')+'">'+escHtml(x[5]||'')+'</td><td class="from">'+escHtml(x[4])+'</td></tr>';
    });
    box.innerHTML=h+'</tbody></table>';
    WgUI.sizeColumns(box.querySelector('table'), 'refs4');
    box.querySelectorAll('tr[data-i]').forEach(tr=>{
      const x=this.refs[+tr.dataset.i];
      tr.title='Double-click: go to it in the render window. Right-click: edit this reference (Shift: its base record)';
      tr.ondblclick=()=>this.goToRef(cell, x);
      tr.oncontextmenu=e=>{
        e.preventDefault();
        const at=x[0].lastIndexOf(':');
        if(e.shiftKey || at<0 || !refCell) this.openRecord(x[2], x[1], null);
        else this.openRef({cell:refCell, origin:x[0].slice(0,at), refr:+x[0].slice(at+1), plugins:cellPlugins});
      };
    });
  },

  /** Opens the reference's cell, framed on it, and selects it (the inspector opens). */
  goToRef(cell, x){
    const target = cell.startsWith('int:')
      ? {kind:'int', name:cell.slice(4), label:cell.slice(4)}
      : (()=>{ const [a,b]=cell.split(',').map(Number); return {kind:'ext', x:a, y:b, name:'', label:a+', '+b}; })();
    App.mode='cell'; App.cellSel=target;
    App._doorAim={pos:x[3].slice(), rot:0, key:target.kind==='int'? 'i:'+target.name : target.x+','+target.y};
    App._findOri=x[0];
    if(typeof syncCellButton==='function') syncCellButton();
    schedulePreview();
  },
});
