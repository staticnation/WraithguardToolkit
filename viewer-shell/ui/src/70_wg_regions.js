/* =====================================================================================
   Wraithguard: the region map (TODO, "The world") - the cell picker's map colored by
   region, regions painted onto cells, and exteriors made by dragging.

   The picker's **Regions** button (Map view) colors every exterior by its region's map
   color (REGN `CNAM`; a color of its own from the id where a region has none), and opens
   a bar under the picker's own:
     - Look           hover and pick as always; the region is in the cell's tip
     - Paint region   drag over cells: each gets the chosen region (or none) - its CELL's
                      `region` field in the Editor's patch (`editCellRegions`); with "Make
                      cells", a square without one gets the patch's own cell first
     - Make exteriors drag a box over the sea: every empty square in it gets flat land at
                      the sea floor (DEFAULT_LAND_HEIGHT) and its cell, the patch's own (`editNewLand`), and the
                      chosen region with them
   The middle button still pans and the wheel still zooms; Esc gives up a drag. What the
   patch has is drawn over the load order's at once (dashed: changed or made here).
   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
const WgRegions={
  on:false, tool:'look', region:'', makeCells:true,
  list:[], byId:new Map(),        // lower id -> {id, name, color}
  edits:new Map(),                // 'x,y' -> region id ('' none) the patch now gives
  made:new Set(),                 // 'x,y' the patch made a cell for here
  stroke:null,                    // {kind:'paint'|'box', cells:Set, a:[x,y], b:[x,y]}
  MAX_BOX:400,

  async load(){
    if(this.list.length || this._loading) return this._loading;
    this._loading=Engine.call('regions_list',{}).then(l=>{
      this.list=Array.isArray(l)? l : [];
      this.byId=new Map(this.list.map(r=>[String(r.id).toLowerCase(), r]));
    }).catch(()=>{ this.list=[]; }).finally(()=>{ this._loading=null; });
    return this._loading;
  },
  /** Another install: its regions are read again. */
  forget(){ this.list=[]; this.byId=new Map(); this.edits.clear(); this.made.clear(); },

  /** A region's color: its own, else one from its id (a region with a black map color
   *  would vanish into the sea). */
  color(id){
    const r=this.byId.get(String(id||'').toLowerCase());
    if(r && r.color && (r.color[0]+r.color[1]+r.color[2])>30) return r.color;
    let h=0; for(const ch of String(id||'').toLowerCase()) h=(h*31+ch.charCodeAt(0))>>>0;
    const hue=h%360, s=0.55, l=0.5, a=s*Math.min(l,1-l);
    const f=n=>{ const k=(n+hue/30)%12; return Math.round(255*(l-a*Math.max(-1,Math.min(k-3,9-k,1)))); };
    return [f(0), f(8), f(4)];
  },
  nameOf(id){ const r=this.byId.get(String(id||'').toLowerCase()); return r? (r.name||r.id) : (id||''); },

  /** The region a square has now: the patch's change here, else the load order's. */
  regionAt(map, k){
    if(this.edits.has(k)) return this.edits.get(k);
    const c=map.cellsByKey.get(k);
    return c? (c.region||'') : '';
  },

  /* ---- the picker's controls ------------------------------------------------------ */

  bindPicker(){
    const btn=$('#cellRegions'); if(!btn) return;
    btn.classList.toggle('pri', this.on);
    btn.onclick=async()=>{
      this.on=!this.on;
      btn.classList.toggle('pri', this.on);
      if(this.on){
        const mapBtn=document.querySelector('#cellView [data-view="map"]');
        if(App.cellPick!=='map' && mapBtn) mapBtn.click();
        await this.load();
      }else this.tool='look';
      this.bar();
      if(typeof CellMap==='object') CellMap.redraw();
    };
    this.bar();
  },

  bar(){
    let b=$('#cellRegionBar');
    const host=$('#cellMap');
    if(!this.on){ if(b) b.hidden=true; return; }
    if(!host) return;
    if(!b){
      b=document.createElement('div');
      b.id='cellRegionBar';
      b.style.cssText='display:flex;flex-wrap:wrap;gap:6px;align-items:center;margin:0 0 6px';
      host.parentNode.insertBefore(b, host);
    }
    b.hidden=false;
    const can=typeof WgEditor==='object' && WgEditor.canEdit && WgEditor.canEdit();
    const opts=['<option value="">(no region)</option>'].concat(this.list.map(r=>
      '<option value="'+escHtml(r.id)+'"'+(String(r.id).toLowerCase()===this.region.toLowerCase()? ' selected' : '')+'>'+escHtml(r.name? r.name+'  ('+r.id+')' : r.id)+'</option>')).join('');
    const col=this.color(this.region);
    b.innerHTML=
      '<span class="seg" id="cellRegionTools">'+
        [['look','Look'],['paint','Paint region'],['box','Make exteriors']].map(([k,t])=>
          '<button class="btn sm'+(this.tool===k? ' pri' : '')+'" data-tool="'+k+'"'+(k!=='look' && !can? ' disabled title="Needs the Editor open on an OpenMW setup"' : '')+'>'+t+'</button>').join('')+
      '</span>'+
      '<span title="The region painted, and given to the exteriors made" style="display:inline-block;width:14px;height:14px;border-radius:3px;border:1px solid var(--line2);background:'+(this.region? 'rgb('+col.join(',')+')' : 'transparent')+'"></span>'+
      '<select class="fld sm" id="cellRegionPick" style="max-width:240px">'+opts+'</select>'+
      '<label class="from" title="Painting a square with no cell anywhere gives it the patch\'s own cell first"><input type="checkbox" id="cellRegionMake"'+(this.makeCells? ' checked' : '')+'> Make cells</label>'+
      '<span class="from" id="cellRegionSt"></span>';
    b.querySelectorAll('[data-tool]').forEach(x=>x.onclick=()=>{ this.tool=x.dataset.tool; this.bar(); this.say(this.hint()); });
    b.querySelector('#cellRegionPick').onchange=e=>{ this.region=e.target.value; this.bar(); };
    b.querySelector('#cellRegionMake').onchange=e=>{ this.makeCells=e.target.checked; };
    this.say(this.hint());
  },
  hint(){
    return this.tool==='paint'? 'Drag over cells to give them '+(this.region? this.nameOf(this.region) : 'no region')+' · middle button pans'
      : this.tool==='box'? 'Drag a box over the sea: each empty square in it gets flat land and its cell'+(this.region? ', in '+this.nameOf(this.region) : '')
      : this.list.length+' regions · the colors are their map colors';
  },
  say(t){ const s=$('#cellRegionSt'); if(s) s.textContent=t||''; },

  /* ---- the map's pointer (31_cellmap.js asks first) ------------------------------- */

  active(){ return this.on && this.tool!=='look' && App.cellPick==='map'; },

  down(map, e){
    if(!this.active() || e.button!==0) return false;
    const [px,py]=map.local(e), c=map.cellAt(px,py); if(!c) return true;
    e.preventDefault();
    this.stroke={kind:this.tool==='paint'? 'paint' : 'box', cells:new Set([c.x+','+c.y]), a:[c.x,c.y], b:[c.x,c.y]};
    try{ map.canvas.setPointerCapture(e.pointerId); }catch(_){}
    map.redraw();
    return true;
  },
  move(map, e){
    const s=this.stroke; if(!s) return false;
    const [px,py]=map.local(e), c=map.cellAt(px,py); if(!c) return true;
    if(s.kind==='paint') s.cells.add(c.x+','+c.y);
    else s.b=[c.x,c.y];
    map.redraw();
    return true;
  },
  up(map, e){
    const s=this.stroke; if(!s) return false;
    this.stroke=null;
    try{ map.canvas.releasePointerCapture(e.pointerId); }catch(_){}
    if(s.kind==='paint') this.paint(map, [...s.cells]);
    else this.makeBox(map, s.a, s.b);
    map.redraw();
    return true;
  },
  cancel(map){ if(this.stroke){ this.stroke=null; map.redraw(); return true; } return false; },

  /** Squares of a box, as 'x,y'. */
  boxCells(a, b){
    const out=[];
    for(let y=Math.min(a[1],b[1]); y<=Math.max(a[1],b[1]); y++) for(let x=Math.min(a[0],b[0]); x<=Math.max(a[0],b[0]); x++) out.push(x+','+y);
    return out;
  },
  cellName(k){ const [x,y]=k.split(','); return '('+x+', '+y+')'; },

  async paint(map, keys){
    const has=k=>map.known(...k.split(',').map(Number)) || this.made.has(k);
    const want=keys.filter(k=>this.makeCells || has(k));
    if(!want.length){ this.say('Those squares have no cell: tick Make cells, or make them first'); return; }
    this.say('Painting '+want.length+' cell'+(want.length===1? '' : 's')+'…');
    let r;
    try{ r=await WgEditor.ask('editCellRegions',{cells:want.map(k=>this.cellName(k)), region:this.region, make:this.makeCells}); }
    catch(e){ this.say(''); return toast(String(e.message||e),'err',5000); }
    for(const k of want){
      if(!has(k)) this.made.add(k);
      this.edits.set(k, this.region);
    }
    this.say((r && r.set||0)+' painted'+(r && r.made? ', '+r.made+' cells made' : '')+(r && r.skipped? ', '+r.skipped+' without a cell left' : ''));
    this.afterEdit(map);
  },

  async makeBox(map, a, b){
    const keys=this.boxCells(a, b).filter(k=>!map.known(...k.split(',').map(Number)) && !this.made.has(k));
    if(!keys.length){ this.say('Every square in that box has a cell already'); return; }
    if(keys.length>this.MAX_BOX){ this.say('That is '+keys.length+' squares; at most '+this.MAX_BOX+' at a time'); return; }
    if(keys.length>40 && !confirm('Make '+keys.length+' new exteriors (flat land at the sea floor, and their cells) in the patch?')) return;
    let made=0;
    for(const k of keys){
      this.say('Making '+(made+1)+' of '+keys.length+'…');
      try{ await WgEditor.ask('editNewLand',{cell:this.cellName(k), height:DEFAULT_LAND_HEIGHT}); made++; this.made.add(k); this.edits.set(k, ''); }
      catch(e){ toast(this.cellName(k)+': '+String(e.message||e),'err',5000); break; }
    }
    if(made && this.region){
      const done=keys.filter(k=>this.made.has(k));
      try{ await WgEditor.ask('editCellRegions',{cells:done.map(k=>this.cellName(k)), region:this.region, make:false}); for(const k of done) this.edits.set(k, this.region); }
      catch(e){ toast(String(e.message||e),'err',5000); }
    }
    this.say(made+' exterior'+(made===1? '' : 's')+' made'+(this.region && made? ' in '+this.nameOf(this.region) : ''));
    this.afterEdit(map);
  },

  afterEdit(map){
    // The new cells are cells to the map: hovered, picked, filtered.
    for(const k of this.made){
      if(map.cellsByKey.has(k)) continue;
      const [x,y]=k.split(',').map(Number);
      map.cellsByKey.set(k, {kind:'ext', x, y, name:'', label:'('+x+', '+y+')', sub:'made by the patch', region:this.edits.get(k)||''});
    }
    for(const [k,reg] of this.edits){ const c=map.cellsByKey.get(k); if(c && this.made.has(k)) c.region=reg; }
    map.redraw();
    if(typeof WgEditor==='object' && WgEditor.refreshPending) try{ WgEditor.refreshPending(); }catch(_){}
  },

  /* ---- drawing (31_cellmap.js calls it after the picture) -------------------------- */

  drawOverlay(ctx, map){
    if(!this.on || !map.view) return;
    const W=map.W, H=map.H;
    const keys=new Set([...map.cellsByKey.keys(), ...this.made]);
    const byCol=new Map();
    const dashed=[];
    for(const k of keys){
      const reg=this.regionAt(map, k);
      const [x,y]=k.split(',').map(Number);
      const [rx,ry,rw,rh]=map.rect(x,y);
      if(rx+rw<0||ry+rh<0||rx>W||ry>H) continue;
      if(this.edits.has(k) || this.made.has(k)) dashed.push([rx,ry,rw,rh]);
      if(!reg) continue;
      const c=this.color(reg).join(',');
      if(!byCol.has(c)) byCol.set(c, []);
      byCol.get(c).push([rx,ry,rw,rh]);
    }
    for(const [c,rs] of byCol){
      ctx.fillStyle='rgba('+c+',0.5)';
      for(const [rx,ry,rw,rh] of rs) ctx.fillRect(rx,ry,rw,rh);
    }
    if(dashed.length){
      ctx.save(); ctx.setLineDash([3,3]); ctx.strokeStyle='rgba(255,255,255,0.85)'; ctx.lineWidth=1;
      ctx.beginPath(); for(const [rx,ry,rw,rh] of dashed) ctx.rect(rx+1.5,ry+1.5,rw-3,rh-3); ctx.stroke(); ctx.restore();
    }
    // The stroke under way.
    const s=this.stroke;
    if(s){
      const cells=s.kind==='paint'? [...s.cells] : this.boxCells(s.a, s.b);
      ctx.fillStyle=s.kind==='paint'? (this.region? 'rgba('+this.color(this.region).join(',')+',0.75)' : 'rgba(0,0,0,0.45)') : 'rgba(255,255,255,0.25)';
      for(const k of cells){
        const [x,y]=k.split(',').map(Number);
        if(s.kind==='box' && (map.known(x,y) || this.made.has(k))) continue;
        const [rx,ry,rw,rh]=map.rect(x,y);
        ctx.fillRect(rx,ry,rw,rh);
      }
      if(s.kind==='box'){
        const [ax,ay]=map.rect(Math.min(s.a[0],s.b[0]), Math.max(s.a[1],s.b[1]));
        const [bx,by,bw,bh]=map.rect(Math.max(s.a[0],s.b[0]), Math.min(s.a[1],s.b[1]));
        ctx.strokeStyle='#fff'; ctx.lineWidth=2; ctx.strokeRect(ax+1,ay+1,bx+bw-ax-2,by+bh-ay-2);
      }
    }
  },

  /** The cell tip's line: its region, by name. */
  tip(c){
    if(!this.on || !c) return '';
    const k=c.x+','+c.y, reg=this.regionAt(CellMap, k);
    return reg? 'Region: '+this.nameOf(reg)+(this.edits.has(k)? ' (the patch)' : '') : '';
  },
};
