
/* =====================================================================================
   From Wraithguard to here - Wraithguard. The conflict viewer's "Show in Cell Preview":
   a record, found where it stands in the loaded world, the cell opened on it, the view
   framed on it and the inspector opened (`find_record` in the shell). A CELL record
   opens that cell; anything else the first reference to it (and says how many more).

   Wraithguard asks either by launching the viewer on `--cell find:<tag>:<id>`, or - while
   a viewer from it is open - by queueing the request on its loopback server, which this
   window asks for every second and a half (`links.poll` in the launch's extra file,
   `wg_post` in the shell). Cell specs ("x,y", "int:<name>") work that way too.

   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
const WgNav={
  _busy:false, _timer:null,
  links(){ return (((window.__WG_VIEW__||{}).extra)||{}).links||{}; },

  /** Opens `spec`: "find:<tag>:<id>", "int:<name>", "x,y" or "plugin:<name>". */
  async go(spec){
    spec=String(spec||'').trim();
    if(!spec) return;
    /* Wraithguard: a plugin - its cells on the map, and the cells opened from there
       reviewed for it (48_wg_tools.js). */
    if(spec.startsWith('plugin:')){
      const name=spec.slice(7);
      if(typeof WgTools==='object'){ WgTools.review=name; WgTools.without=false; }
      if(typeof WgCoverage==='object'){ await WgCoverage.load(); WgCoverage.showPlugin(name); }
      const acc=$('#acc_review'); if(acc) acc.classList.remove('closed');
      return;
    }
    let target=null, aim=null, key=null;
    if(spec.startsWith('find:')){
      const rest=spec.slice(5), i=rest.indexOf(':');
      const tag=i<0? '' : rest.slice(0,i), id=i<0? rest : rest.slice(i+1);
      let r;
      try{ r=await Engine.call('find_record',{tag,id}); }
      catch(e){ toast(String(e.message||e),'err',6000); return; }
      target = r.kind==='int'
        ? {kind:'int', name:r.name, label:r.name}
        : {kind:'ext', x:r.x, y:r.y, name:r.name||'', label:(r.name? r.name+'  ' : '')+'('+r.x+', '+r.y+')'};
      if(Array.isArray(r.pos)) aim={pos:r.pos.slice(), rot:+r.rot||0};
      key=r.key||null;
      if(r.count>1) toast(id+' stands in '+r.count+' places - showing the first','ok',5000);
    } else if(spec.startsWith('int:')){
      const n=spec.slice(4); target={kind:'int', name:n, label:n};
    } else {
      const m=/^(-?\d+)\s*,\s*(-?\d+)$/.exec(spec);
      if(!m) return;
      target={kind:'ext', x:+m[1], y:+m[2], name:'', label:m[1]+', '+m[2]};
    }
    App.mode='cell';
    App.cellSel=target;
    if(aim) App._doorAim={pos:aim.pos, rot:aim.rot, key:target.kind==='int'? 'i:'+target.name : target.x+','+target.y};
    App._findOri=key;
    if(typeof syncCellButton==='function') syncCellButton();
    schedulePreview();
  },

  /** Asks Wraithguard, now and then, whether it has something to show here. */
  poll(){
    const url=this.links().poll;
    if(!url || this._timer) return;
    this._timer=setInterval(async()=>{
      if(this._busy) return;
      this._busy=true;
      try{
        const t=await Engine.call('wg_post',{url, body:''});
        if(typeof t==='string' && t.trim()) await this.go(t.trim());
      }catch(_){ /* Wraithguard closed: nothing to ask any more */ }
      this._busy=false;
    }, 1500);
  },
};
