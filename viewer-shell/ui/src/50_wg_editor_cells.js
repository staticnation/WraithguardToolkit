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

  /** The cells the patch makes itself (`editNewCell`, "Make a copy as…"): the engine lists
   *  only the loaded world's, so these are merged in from the pending list - an interior by
   *  its name, an exterior square by `(x, y)`; their references are the patch's own. */
  pendingCells(){
    const out=[], seen=new Set((this.cells||[]).map(c=>c.kind==='int'? 'i:'+String(c.name).toLowerCase() : c.x+','+c.y));
    for(const p of this.made||[]){
      if(p.tag!=='CELL' || p.id==null) continue;
      const id=String(p.id), m=id.match(/^\(\s*(-?\d+)\s*,\s*(-?\d+)\s*\)$/);
      const c= m? {kind:'ext', x:+m[1], y:+m[2], name:'', label:'Wilderness ('+(+m[1])+', '+(+m[2])+')'}
                : {kind:'int', name:id, label:id};
      const k=c.kind==='int'? 'i:'+id.toLowerCase() : c.x+','+c.y;
      if(seen.has(k)) continue;
      seen.add(k);
      c.refs=(this.liveNew.get(c.kind==='int'? id.toLowerCase() : '('+c.x+', '+c.y+')')||[]).length;
      c.path=false; c.pending=true;
      out.push(c);
    }
    return out;
  },

  /** The engine's cells and the patch's own. */
  allCells(){
    const mine=this.pendingCells();
    if(!mine.length) return this.cells||[];
    const ext=mine.filter(c=>c.kind==='ext').sort((a,b)=>a.x-b.x || a.y-b.y);
    return [...mine.filter(c=>c.kind==='int'), ...(this.cells||[]), ...ext];
  },

  /** An exterior square the engine has no cell for (an open-sea square opened from the Cell
   *  map to be built in): the page draws it empty, and placing in it makes its cell. */
  isEmptySquare(t){
    return !!t && t.kind==='ext' && !(this.cells||[]).some(c=>c.kind==='ext' && c.x===(t.x|0) && c.y===(t.y|0));
  },

  /** The patch-made cell a target (`{kind, name | x, y}`) or a cell string (`int:Name`,
   *  `x,y`) names, or null - a cell of the load order is not one. */
  pendingCell(target){
    if(typeof target==='string'){
      if(target.startsWith('int:')) target={kind:'int', name:target.slice(4)};
      else{ const [x,y]=target.split(',').map(Number); target={kind:'ext', x, y}; }
    }
    if(!target) return null;
    return this.pendingCells().find(c=>target.kind==='int'
      ? c.kind==='int' && String(c.name).toLowerCase()===String(target.name||'').toLowerCase()
      : c.kind==='ext' && c.x===(target.x|0) && c.y===(target.y|0)) || null;
  },

  /** The cell key the patch's endpoints and `liveNew` know a pending cell by. */
  pendingKey(c){ return c.kind==='int'? String(c.name) : '('+c.x+', '+c.y+')'; },

  /** What the engine would answer for a cell of the patch's own (`interior_data`,
   *  `cell_data`): nothing in it but what the patch places - an empty room, or a square
   *  with no land (drawn flat, at the sea). `overlay` then adds the patch's references. */
  emptyCell(target){
    return {name:target.kind==='int'? String(target.name||'') : '', ltex:{}, models:{}, lights:{}, actors:{},
            refs:[], hasLand:false, water:null, quasi:false, ambi:null};
  },

  /** Shows a cell in the Cell View: the filter and "Modified only" off, the list long enough
   *  to hold it, its row selected and scrolled to, and its references listed. A cell the
   *  patch has just made would otherwise sit among thousands, out of sight.
   *  @param k {string} `int:Name` or `x,y`. */
  revealCell(k){
    this.cellFilter=''; this.cellsModOnly=false;
    const f=this.part('edCellFilter'); if(f) f.value='';
    const m=this.part('edCellsMod'); if(m) m.checked=false;
    this.cellSel=k;
    this.drawCells();
    const at=(this._cellKeys||[]).indexOf(k);
    if(at>=this.limit('cells')){ this._limits=this._limits||{}; this._limits.cells=at+25; this.drawCells(); }
    const box=this.part('edCellList');
    const row=box && [...box.querySelectorAll('tr[data-cell]')].find(tr=>tr.dataset.cell===k);
    if(row && row.scrollIntoView) row.scrollIntoView({block:'center'});
    if(row) this.loadRefs(k);
  },

  drawCells(){
    const box=this.part('edCellList'); if(!box) return;
    const f=this.cellFilter.trim().toLowerCase();
    let list=this.allCells();
    if(f) list=list.filter(c=>c.label.toLowerCase().includes(f));
    const edOf=c=>c.pending || this.editedCells.has(c.kind==='int'? String(c.name).toLowerCase() : '('+c.x+', '+c.y+')');
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
    this._cellKeys=list.map(c=>c.kind==='int'? 'int:'+c.name : c.x+','+c.y);   // for revealCell
    let h='<table class="edT"><thead><tr>'+CS.map(([l],k)=>'<th data-ccol="'+k+'">'+l+(k===sc.col? (sc.dir>0?' ▲':' ▼') : '')+'</th>').join('')+'</tr></thead><tbody>';
    for(const c of list.slice(0,this.limit('cells'))){
      const k=c.kind==='int'? 'int:'+c.name : c.x+','+c.y;
      const ed=edOf(c);
      const cls=[this.cellSel===k? 'sel' : '', ed? 'edited' : ''].filter(Boolean).join(' ');
      h+='<tr data-cell="'+escHtml(k)+'"'+(cls? ' class="'+cls+'"' : '')+(c.pending? ' title="A cell of the patch\'s own: no plugin of the load order has it"' : ed? ' title="The patch changes or adds references here"' : '')+'>'+
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
          {label:'New exterior square with land…', disabled:!this.links().editNewLand, title:'A grid square no plugin has, with flat land at the sea\'s level (what the Construction Set makes on the first brush stroke)', act:()=>WgFlow.newCell(true, true)},
          {label:'Add flat land to this square', disabled:inner || !this.links().editNewLand, title:inner? 'Land belongs to an exterior square' : 'Flat land at the sea\'s level, for a square with none (the patch refuses one that has land)', act:()=>WgFlow.newLand(k)},
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
    const mine=this._refsN=(this._refsN||0)+1;   // only the latest cell asked for may draw
    box.innerHTML='<div class="hint">…</div>';
    let r;
    const fresh=this.pendingCell(cell);
    if(fresh){
      // A cell of the patch's own: the engine has no such cell; its references are the patch's.
      const key=this.pendingKey(fresh);
      r={refCell:key, plugins:null, name:fresh.label, rows:(this.liveNew.get(key.toLowerCase())||[])
        .filter(a=>a.fields && !a.fields.deleted)
        .map(a=>['new:'+a.uid, a.fields.id||'', a.tag||'', (a.fields.translation||[0,0,0]).slice(), '(this patch)', ''])};
    }
    else try{ r=await Engine.call('editor_cell_refs',{cell}); }
    catch(e){ if(mine===this._refsN) box.innerHTML='<div class="hint">'+escHtml(String(e.message||e))+'</div>'; return; }
    if(mine!==this._refsN) return;
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
        if(x[0].startsWith('new:') && refCell && !e.shiftKey) this.openRef({cell:refCell.toLowerCase(), uid:x[0].slice(4)});
        else if(e.shiftKey || at<0 || !refCell) this.openRecord(x[2], x[1], null);
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
