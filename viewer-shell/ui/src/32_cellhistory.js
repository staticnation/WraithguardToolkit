/* =====================================================================================
   THE CELL VISIT HISTORY — round 18cy.

   Robin: "We should keep a history of the cells we have loaded since we started this
   session of the program. Only count actual cells, not other states like Simplified
   preview or the mesh editing mode. When I press the left arrow, I want to load the last
   cell I visited. When I press forward, I want to move forward in the history."

   The two arrows stand on the heading row of the statistics box in the viewport's
   corner, in Real cell mode only (`drawCellStats`, 18_cellpreview.js). The list is the
   cells a person asked for - through the picker, a cell arrow in the scene, or a door's
   "Open cell door leads to" - in the order they asked; stepping back and forward walks
   it, and asking for a new cell from somewhere in the middle forgets the road ahead, the
   way a browser does. Only a Real cell counts: the simplified patch and the mesh editor
   are not visits, and switching to either and back leaves the list where it was.

   **What the camera does on a step depends on how the cells were joined**, in Robin's
   words: "The wanted camera position when switching cells in either direction of the
   cell visit history, should be a bit different depending on what source made me move".
   Each entry remembers how it was arrived at (`in`) and how it was left (`out`), the two
   ends of one transition; a step back walks the transition that brought you here, a
   step forward the one that took you on.

   * A door. Back: "we should save a position looking at the door that we used the 'Open
     cell door leads to' button from. The view should be similar as to when opening a
     new door, but as we now look at the door, and not a marker in front of it, we should
     have a little bit further distance to the door." So the door's middle and the side
     of it the eye was on are kept at the press (`doorSeen`), and the step back frames
     the door from that side the way a door arrival frames its marker, further out
     (`DOOR_BACK_FAR`) and never further than the eye stood - the person saw the door
     from there, so it is a clear place to stand. Forward is a door arrival again: the
     marker, as when the door was first opened.
   * A cell arrow. "The position should work the same when I step back and forward in the
     history as if I press a cell arrow in the scene (as in, same relative position to
     origin, and keeping its rotation)." The camera is carried: the same view, moved by
     the distance between the two cells' middles (`carry`), whether or not the scene on
     the renderer can be panned the way the arrows pan it.
   * The picker. "We save the position the camera had at the time I load a new cell, and
     reset it when I step through the history." Every entry keeps the camera it was last
     left with, in world units so a scene rebuilt about another origin puts it back in
     the same place; a step over a picker transition restores it.

   The step itself is a cell load like any other: `go` sets the target and a plan, and
   `buildCellPreview` applies the plan where it frames a new cell, before the camera
   moves - which is also where a person's own load is recorded, with the camera the cell
   being left had. Nothing here is saved between runs.
   ===================================================================================== */
/** How much further out than a door arrival the step back stands from the door itself. */
const DOOR_BACK_FAR=1.3;

const CellHistory={
  entries:[],        // [{key, cell:{kind,x,y,name,label}, cam, in:{via,doorAim}, out:{via,door}}]
  index:-1,
  pending:false,     // a step's load is on its way: the arrows wait for it

  keyOf(c){ return c? (c.kind==='int'? 'i:'+(c.name||'') : c.x+','+c.y) : ''; },
  /** The key of the cell a built scene is centred on. */
  sceneKey(sc){ const c=sc&&sc.centre; return c? (c.kind==='int'? 'i:'+(c.name||'') : c.gx+','+c.gy) : ''; },
  /** Whether the scene on the renderer is the cell the current entry names. */
  onEntry(){ const e=this.entries[this.index]; return !!(e && App._scene && this.sceneKey(App._scene)===e.key); },
  labelOf(c){ return c.label || c.name || (c.kind==='ext'? '('+c.x+', '+c.y+')' : ''); },

  /** The camera as it stands, in world units, with what the scene it looks at is aimed
      at - enough to put it back in another scene, or to carry it a cell along. */
  camWorld(){
    const R=App.R, sc=App._scene; if(!R||!R.cam||!sc) return null;
    const o=sc.origin||[0,0], c=R.cam, a=sc.aim||[0,0];
    return {pivot:[c.tx+o[0], c.ty+o[1], c.tz], az:c.az, el:c.el, dist:c.dist, vs:c.vs||1,
            aim:[o[0]+a[0], o[1]+a[1]], cz:sc.cz||0};
  },
  /** The current entry keeps the camera as it stands - called wherever the scene is
      about to stop being this cell without a cell being asked for (the mode switch). */
  snapshot(){ if(this.onEntry()) this.entries[this.index].cam=this.camWorld(); },

  /** A door as it is being used, for the step back to it: its middle in world units, the
      bearing of the eye from it, and how far the eye stood. */
  doorSeen(hit){
    const R=App.R, sc=App._scene; if(!R||!R.cam||!hit) return null;
    const o=(sc&&sc.origin)||[0,0], b=hit.aabb, m=hit.m||[];
    const c=b? [(b.x0+b.x1)/2,(b.y0+b.y1)/2,(b.z0+b.z1)/2] : [+m[3]||0, +m[7]||0, +m[11]||0];
    const eye=R.cameraEye();
    return {pos:[c[0]+o[0], c[1]+o[1], c[2]], az:Math.atan2(eye[1]-c[1], eye[0]-c[0]),
            eyeDist:Math.hypot(eye[0]-c[0], eye[1]-c[1], eye[2]-c[2])};
  },

  /** A person asked for `target`: `via` is how - `{via:'pick'|'arrow'|'door', door,
      doorAim}`. Called by the build before it moves the camera, so the entry being left
      keeps the camera it was left with. The same cell again is a rebuild, not a visit. */
  record(target, via){
    if(!target || (target.kind!=='ext' && target.kind!=='int')) return false;
    via=via||{};
    const key=this.keyOf(target);
    const cur=this.entries[this.index];
    if(cur && cur.key===key) return false;
    if(cur){
      if(this.onEntry()) cur.cam=this.camWorld();
      cur.out={via:via.via||'pick', door:via.door||null};
    }
    this.entries.length=this.index+1;
    this.entries.push({key, cell:{kind:target.kind, x:target.x, y:target.y, name:target.name||'', label:this.labelOf(target)},
                       cam:null, in:{via:via.via||'pick', doorAim:via.doorAim||null}, out:null});
    this.index=this.entries.length-1;
    this.sync();
    return true;
  },

  canBack(){ return this.index>0 && !this.pending; },
  canForward(){ return this.index>=0 && this.index<this.entries.length-1 && !this.pending; },

  /** One step: `dir` is -1 for back, +1 for forward. Loads the cell with a camera plan
      for the transition being walked; `buildCellPreview` applies it. */
  go(dir){
    const j=this.index+dir;
    if(this.pending || j<0 || j>=this.entries.length) return false;
    const cur=this.entries[this.index], to=this.entries[j];
    if(this.onEntry()) cur.cam=this.camWorld();
    /* The transition: back walks the one that brought you here (`cur.in`, with the door
       kept on the far side as `to.out`), forward the one that took you on (`to.in`). */
    const via=((dir<0? cur.in : to.in)||{}).via||'pick';
    let plan=null;
    if(via==='door'){
      if(dir<0) plan={kind:'door', door:(to.out&&to.out.door)||null};
      else{
        const d=(to.in&&to.in.doorAim)||null;
        if(d) App._doorAim={pos:d.pos.slice(), rot:d.rot, key:to.key};
      }
    }
    else if(via==='arrow') plan={kind:'carry', cam:this.camWorld()};
    else plan={kind:'restore', cam:to.cam};
    this.index=j; this.pending=true;
    App._histStep={key:to.key, plan};
    if(App.mode!=='cell'){ const ms=$('#pvMode'); if(ms){ ms.value='cell'; if(ms.onchange) ms.onchange(); } }
    App.cellSel={kind:to.cell.kind, x:to.cell.x, y:to.cell.y, name:to.cell.name, label:to.cell.label};
    if(typeof syncCellButton==='function') syncCellButton();
    if(typeof schedulePreview==='function') schedulePreview();
    this.sync();
    return true;
  },

  /** The build has taken the step's plan (or given up on the load): the arrows are free. */
  settle(){ this.pending=false; App._histStep=null; this.sync(); },

  /** Everything forgotten: the install is changing hands. */
  reset(){ this.entries=[]; this.index=-1; this.pending=false; App._histStep=null; App._histVia=null; this.sync(); },

  /** The two arrows in the statistics box, as the list stands. */
  sync(){
    const b=$('#stHistBack'), f=$('#stHistFwd'); if(!b||!f) return;
    const back=this.index>0? this.entries[this.index-1] : null;
    const fwd=(this.index>=0 && this.index<this.entries.length-1)? this.entries[this.index+1] : null;
    b.disabled=!back || this.pending; f.disabled=!fwd || this.pending;
    b.title=back? T('cell.hist_back_title',{cell:back.cell.label}) : T('cell.hist_back_none');
    f.title=fwd? T('cell.hist_fwd_title',{cell:fwd.cell.label}) : T('cell.hist_fwd_none');
    b.onclick=e=>{ e.stopPropagation(); this.go(-1); };
    f.onclick=e=>{ e.stopPropagation(); this.go(1); };
  },
};
