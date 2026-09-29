/* =====================================================================================
   THE CELL PICKER'S MAP — round 18cp.

   Robin: "In the cell picker you reach when pressing the cell button in the preview
   section, I want to have an option between 'List' (which is what is there now) and a
   'Map'. When Map is selected, I want to paint a map of the world a similar way as a
   fully discovered map is done by OpenMW, in the place where the list is."

   A revealed OpenMW map is every cell's *local* map — a top-down picture of the ground —
   pasted onto the world at eighteen pixels a cell. The engine paints those pictures
   (`gdn_core::worldmap`: the ground textures' colours blended, under the vertex colours,
   under the sea) because it holds every cell's landscape and the page holds one; what
   crosses the shell is a 32×32 tile per cell, in batches, and this file is everything
   that happens to them afterwards.

   **What this is and is not.** It is a picker: the point of the map is to find a cell
   and click it, so the picture is there to be recognised rather than admired — terrain
   only, no objects, one colour per ground texture, at Robin's word. The interactions are
   the ones he listed and no more: wheel to zoom, middle button to pan, hover for a name,
   click to load, right-click to star, a filter that lights the matches and dims the rest
   without moving the view, the loaded cell marked with its adjacent ring, starred cells
   starred. Interiors are never reachable from here; the List still has them.

   **Where the colours come from.** The tiles and the sea are the *scene* — a picture of
   Morrowind — and are painted in fixed colours the way the viewport is, whatever the
   window is wearing. Everything drawn *on* the picture is chrome and reads the theme:
   the grid is `--line2`, a match's ring and the hovered cell are the accent, the loaded
   cell and its ring are `--info` (the palette's stand-in for blue, which is what a "you
   are here" wants), a star is `--warn`, and the dimming is the window's own `--bg`.

   **Drawing a whole continent.** Robin's world runs from (−139, −58) to (49, 22) — some
   fifteen thousand landscapes in a 189 × 81 box — and at six pixels a cell all of it is
   on screen at once. A `drawImage` per cell per frame was fifteen thousand calls a frame,
   and "when I zoom out a lot, it becomes very slow to render". Measured on a world that
   size: 1.3 s a frame. So the tiles are also painted, once, as they arrive, into **two
   atlases** of the whole world — sixteen pixels a cell and eight (halved again when a
   world is so wide that sixteen would pass sixteen megapixels) — and at any zoom up to
   sixteen a frame is *one* `drawImage` of the atlas whose scale is the first at or above
   the zoom, so the picture is never scaled by more than half and never up. Above sixteen
   the per-cell tiles are drawn, at thirty-two pixels each, and there are at most a few
   thousand of them on screen - which was the lag Robin felt at the default zoom (round
   18cv): above sixteen the picture is now drawn from lazily made **chunks** of the world
   at thirty-two pixels a cell, a few draws a frame. See `CHUNK`.

   Two things the measuring decided. **The atlas a frame draws is an `ImageBitmap`, not
   the canvas it was painted on**: a canvas source is re-uploaded on every draw where a
   bitmap stays resident, and on the software renderer this was measured in that was the
   difference between half a second and fifteen milliseconds; the bitmaps are remade as
   batches land, coalesced, and once more when the last one has. And **the filter is
   composed once, not per frame**: dimming everything but the matches by clipping to a
   path of fifteen thousand rectangles cost 1.8 s a frame, so the dimmed picture is made
   on the CPU when the filter changes — the atlas's pixels read out, every non-matching
   cell's block darkened, put back, one bitmap — and a frame under a filter is the same one
   draw as a frame without. The matches' rings are one path stroked once.

   **Geometry.** North is up. A cell (x, y) sits at map units u = x − minX (east
   positive) and v = maxY − y (south positive); the view is the map-unit position of the
   canvas's top-left corner, `cx0`/`cy0`, and `z` pixels per cell. Kept in map units
   rather than pixels so a zoom is one line: the point under the cursor stays put by
   construction. Tile edges are rounded to whole pixels per cell, otherwise bilinear
   smoothing leaves hairline seams between neighbours at any zoom that is not a multiple
   of the tile size.
   ===================================================================================== */
const CellMap={
  TILE:32,
  ZMIN:6, ZMAX:36, Z0:18,
  BATCH:256,
  /* Round 18cv: above sixteen pixels a cell the picture comes from **chunks** of the world
     at the tiles' own thirty-two - `CHUNK` cells to a side, made from the tiles when the
     view first reaches them and kept while they are near, `CHUNKS_MAX` of them at most.
     A frame at the default zoom used to be two or three thousand `drawImage` calls, one
     tile each; Robin: "at some medium to close zoom levels (including the standard zoom
     level), the map becomes very laggy to pan over certain cells" - the cells with land,
     as it turned out, since the open sea has no tiles to draw. A frame is a handful of
     chunk draws now, at any zoom. */
  CHUNK:32, CHUNKS_MAX:12,
  /* Round 18cw: the island the map opens on. Robin: "I want to center it so the entire
     island of Vvardenfell (which stretches from 3,-15 in the south to -2,25 in the north)
     to be revealed first." South and north are his; west and east are where the vanilla
     landscapes end on those rows - the coast west of Gnisis at x = -18, the Azura's Coast
     at 23 - so Solstheim, which begins at -16 and runs west, is not in the frame, though it
     is on the map. See `home`. */
  ISLAND:{x0:-18, y0:-15, x1:23, y1:25},
  /* The sea, where there is no landscape at all — the same blue the tiles lay over a
     seabed, at its deep end, so open water and a drowned cell read as one water. */
  SEA:'rgb(35, 74, 104)',

  world:null,        // {sig, min, max, cells, tiles:Map, land:Set, done, total, levels, near}
  loading:null,
  view:null,         // {z, cx0, cy0} — the zoom is kept for the session; where it looks is `home`'s
  box:null, canvas:null, ctx:null, tip:null, note:null,
  dpr:1, W:0, H:0,
  hover:null,        // {x,y} under the pointer, or null
  ptr:null,          // {px,py} where the pointer last was on the canvas, null once it has left (18cx)
  drag:null, press:null, rpress:null,
  q:'', kind:'all', matches:null,   // null: no filter in force
  cellsByKey:new Map(),             // 'x,y' -> the picker's cell object (label, region…)
  onPick:null,
  _raf:0, _ro:null, prof:null,

  /* ---- opening --------------------------------------------------------------------- */
  /** Takes over `box` (an empty div in the picker), draws into it, and calls `onPick(cell)`
      when a cell is clicked. `cells` is the picker's own list of exteriors. */
  open(box, cells, onPick){
    this.box=box; this.onPick=onPick;
    this.cellsByKey=new Map(cells.filter(c=>c.kind==='ext').map(c=>[c.x+','+c.y, c]));
    if(!this.canvas || this.canvas.parentNode!==box){
      box.innerHTML='';
      const cv=document.createElement('canvas'); cv.id='cellMapCanvas';
      const tip=document.createElement('div'); tip.className='maptip'; tip.hidden=true;
      const note=document.createElement('div'); note.className='mapnote'; note.hidden=true;
      box.appendChild(cv); box.appendChild(tip); box.appendChild(note);
      this.canvas=cv; this.ctx=cv.getContext('2d'); this.tip=tip; this.note=note;
      this.bind(cv);
      if(window.ResizeObserver){
        this._ro=new ResizeObserver(()=>this.resize());
        this._ro.observe(box);
      }
    }
    this.resize();
    /* Round 18cw: every opening looks where `home` says - on the loaded cell or over the
       island - rather than where the last one left off. `load` homes too, when the world's
       extent first arrives; this is for the openings after that. */
    this.home();
    this.load().catch(e=>{ this.say(T('cell.map_failed',{err:e.message||String(e)})); });
    this.redraw();
  },

  /** The picker's filter, as it types: the same text and kind the list uses. */
  setFilter(q,kind){
    this.q=String(q||'').trim().toLowerCase(); this.kind=kind||'all';
    this.matches = (!this.q && this.kind==='all')? null : new Set(
      Array.from(this.cellsByKey.values()).filter(c=>cellMatches(c,this.q,this.kind)).map(c=>c.x+','+c.y));
    const w=this.world;
    if(w && w.levels.length){
      const m=this.matches;
      (async()=>{ for(const L of w.levels){ if(this.matches!==m) return; await this.composeLevel(w,L); } this.redraw(); })();
    }
    // The chunks' dimmed pictures were for the filter before this one (round 18cv).
    if(w && w.near) for(const ch of w.near.chunks.values()) this.dropDim(ch);
    this.redraw();
    return this.matches? this.matches.size : this.cellsByKey.size;
  },

  resize(){
    if(!this.box||!this.canvas) return;
    const r=this.box.getBoundingClientRect();
    this.dpr=window.devicePixelRatio||1;
    this.W=Math.max(1,Math.round(r.width)); this.H=Math.max(1,Math.round(r.height));
    this.canvas.width=Math.round(this.W*this.dpr); this.canvas.height=Math.round(this.H*this.dpr);
    this.canvas.style.width=this.W+'px'; this.canvas.style.height=this.H+'px';
    this.redraw();
  },

  /* ---- the tiles ------------------------------------------------------------------- */
  /** Reads the world's tiles once per install, in batches, drawing as they land.

      Round 18cx: read at the connect, not at the first opening. Robin: "I want to load in
      the map and cache it on opening the install (including changing install, or changing
      which .esp files are ticked), so it's zero load time for the map when the user opens
      the map cell picker" - and "Keep the loading bar until the map is readied too". So
      `openInstallRun` (13_mo2.js) calls this with the connect's busy card still up, after
      the first scene, and hands it `progress(done, total)` for the card's line; the world
      is keyed by `GameData.sig`, which every connect bumps, so a changed load order is a
      new read and an unchanged one is not. `open` still calls this and finds it done. The
      batches are pipelined: the next one is asked for before this one is unpacked, so the
      engine packs while the page paints. */
  async load(progress){
    const sig=(typeof GameData==='object' && GameData.sig)||'';
    if(this.world && this.world.sig===sig && this.world.done) return;
    /* A read already under way for *this* world is the answer; one for a world that has
       since been replaced is not — the new read takes `this.world`, and the old loop sees
       that and stops at its next batch. */
    if(this.loading && this.world && this.world.sig===sig) return this.loading;
    const job=(async()=>{
      const head=await Engine.call('world_map');
      const cells=(head&&head.cells)||[];
      const w={sig, min:head.min, max:head.max, cells, tiles:new Map(), land:new Set(cells.map(c=>c[0]+','+c[1])),
               done:false, total:cells.length, got:0, levels:[], filterFor:null};
      const old=this.world; this.world=w;
      if(old) this.forget(old);
      if(!cells.length){ w.done=true; this.say(T('cell.map_empty')); this.home(); this.redraw(); return; }
      this.makeAtlas(w);
      this.home();
      if(progress) progress(0,w.total);
      const ask=from=>Engine.bytes('world_map_tiles',{from, count:this.BATCH});
      let next=ask(0);
      for(let from=0; from<cells.length; from+=this.BATCH){
        if(this.world!==w){ next.catch(()=>{}); return; }   // a new install arrived underneath
        const ab=await next;
        next = from+this.BATCH<cells.length? ask(from+this.BATCH) : Promise.resolve(null);
        await this.unpack(ab,w);
        this.say(T('cell.map_reading',{done:w.got,total:w.total}));
        if(progress) progress(w.got,w.total);
        this.refreshAtlas(w);
        this.redraw();
      }
      // The last remake first, then "read": the frame that follows is the finished picture.
      await this.refreshAtlas(w,true);
      if(this.world!==w) return;
      w.done=true; this.say('');
      this.redraw();
    })();
    this.loading=job;
    // Only this read's end clears the slot: an older read stopping late must not clear a newer one's.
    job.catch(()=>{}).then(()=>{ if(this.loading===job) this.loading=null; });
    return job;
  },

  /** Lets a world go: every bitmap closed, the canvases emptied (round 18cx). Called when
      a read replaces the world and when the install is stood down (`drop`). */
  forget(w){
    for(const bm of w.tiles.values()) bm.close();
    w.tiles.clear();
    for(const L of (w.levels||[])){
      if(L.bm){ L.bm.close(); L.bm=null; }
      if(L.fbm){ L.fbm.close(); L.fbm=null; }
      L.cv.width=1; L.cv.height=1;
    }
    if(w.near){ for(const ch of w.near.chunks.values()) this.dropChunk(ch); w.near.chunks.clear(); }
  },
  /** The install is going: the map goes with it, and the filter with the map (round 18cx).
      The connect that follows reads the next world. */
  drop(){
    const w=this.world; this.world=null; this.view=null; this.zoom=null;
    this.matches=null; this.q=''; this.kind='all';
    this.hover=null; this.showTip(null);
    if(w) this.forget(w);
    this.redraw();
  },

  /** One batch: `GDNM`, a count, then x, y and 32×32 RGBA per cell. */
  async unpack(ab,w){
    const dv=new DataView(ab);
    if(ab.byteLength<8 || String.fromCharCode(dv.getUint8(0),dv.getUint8(1),dv.getUint8(2),dv.getUint8(3))!=='GDNM')
      throw new Error('world_map_tiles');
    const n=dv.getUint32(4,true); const T4=this.TILE*this.TILE*4;
    let o=8; const jobs=[];
    for(let i=0;i<n;i++){
      const x=dv.getInt32(o,true), y=dv.getInt32(o+4,true); o+=8;
      const px=new Uint8ClampedArray(ab,o,T4).slice(); o+=T4;
      const img=new ImageData(px,this.TILE,this.TILE);
      jobs.push(createImageBitmap(img).then(bm=>{
        if(this.world!==w){ bm.close(); return; }      // landed after the world was let go
        w.tiles.set(x+','+y,bm);
        // And into the atlases, scaled down once, so a frame never has to.
        const u=x-w.min[0], sv=w.max[1]-y;
        for(const L of w.levels) L.ctx.drawImage(bm,u*L.S,sv*L.S,L.S,L.S);
        // And into its chunk, when the view has already made that one (round 18cv).
        if(w.near){
          const N=w.near, ch=N.chunks.get(Math.floor(u/N.CH)+','+Math.floor(sv/N.CH));
          if(ch){ ch.ctx.drawImage(bm,(u-ch.u0)*N.S,(sv-ch.v0)*N.S,N.S,N.S); ch.stale=true; this.dropDim(ch); }
        }
      }));
    }
    await Promise.all(jobs);
    w.got+=n;
  },

  /** The whole world at two scales, each on one canvas: sixteen pixels a cell and eight,
      or half those when sixteen would put the larger canvas over sixteen megapixels. Sea
      everywhere a tile does not land, the same sea the map is cleared to. */
  makeAtlas(w){
    const U=w.max[0]-w.min[0]+1, V=w.max[1]-w.min[1]+1;
    let S=0;
    for(const s of [16,8,4]){ if(U*s*V*s<=16e6){ S=s; break; } }
    // Round 18cv: the near level, in chunks made on demand - see `chunkAt`.
    w.near={S:this.TILE, CH:this.CHUNK, chunks:new Map(), tick:0};
    if(!S) return;               // a world too wide even for four: the chunks draw it all
    w.levels=[S,S/2].map(sc=>{
      const cv=document.createElement('canvas');
      cv.width=U*sc; cv.height=V*sc;
      const c=cv.getContext('2d',{willReadFrequently:true});
      c.imageSmoothingEnabled=true; c.imageSmoothingQuality='high';
      c.fillStyle=this.SEA; c.fillRect(0,0,cv.width,cv.height);
      return {S:sc, cv, ctx:c, bm:null, fbm:null, job:null, queued:false};
    });
  },

  /** Remakes each level's bitmap from its canvas — coalesced, so a run of batches landing
      faster than a bitmap can be made costs one remake, not one per batch — and, when a
      filter is in force, the dimmed one with it. Each level keeps one chain of remakes:
      a call while one is queued joins it, a call while one is *running* queues the next,
      so the last batch to land is always followed by a remake that saw it. `force` waits
      for the chain; the end of the read uses it, so the first frame after "read" is the
      finished picture and not the canvas it was painted on. */
  refreshAtlas(w,force){
    const jobs=w.levels.map(L=>{
      if(!L.queued){
        L.queued=true;
        L.job=(L.job||Promise.resolve()).then(async()=>{
          L.queued=false;
          const bm=await createImageBitmap(L.cv);
          if(this.world!==w){ bm.close(); return; }
          if(L.bm) L.bm.close();
          L.bm=bm;
          if(this.matches) await this.composeLevel(w,L);
          this.redraw();
        });
      }
      return L.job;
    });
    return force? Promise.all(jobs) : undefined;
  },

  /* ---- the near level: chunks of the world at the tiles' own size (round 18cv) --------- */
  /** The chunk at chunk coordinates (cu, cv) - `CHUNK` cells square, sea where no tile has
      landed - made from the tiles the first time the view reaches it, and remembered.
      Beyond `CHUNKS_MAX` the one drawn longest ago is let go. */
  chunkAt(w,cu,cv){
    const N=w.near, key=cu+','+cv;
    let ch=N.chunks.get(key);
    if(ch){ ch.last=++N.tick; return ch; }
    const U=w.max[0]-w.min[0]+1, V=w.max[1]-w.min[1]+1;
    const u0=cu*N.CH, v0=cv*N.CH;
    const cw=Math.min(N.CH,U-u0), cvh=Math.min(N.CH,V-v0);
    if(cw<=0||cvh<=0) return null;
    const c2=document.createElement('canvas');
    c2.width=cw*N.S; c2.height=cvh*N.S;
    const ctx=c2.getContext('2d',{willReadFrequently:true});
    ctx.fillStyle=this.SEA; ctx.fillRect(0,0,c2.width,c2.height);
    for(let sv=v0;sv<v0+cvh;sv++) for(let u=u0;u<u0+cw;u++){
      const bm=w.tiles.get((u+w.min[0])+','+(w.max[1]-sv)); if(!bm) continue;
      ctx.drawImage(bm,(u-u0)*N.S,(sv-v0)*N.S,N.S,N.S);
    }
    ch={key,u0,v0,cw,ch:cvh,cv:c2,ctx,bm:null,fbm:null,fkey:null,stale:true,making:false,last:++N.tick};
    N.chunks.set(key,ch);
    if(N.chunks.size>this.CHUNKS_MAX){
      let old=null;
      for(const x of N.chunks.values()) if(!old || x.last<old.last) old=x;
      if(old && old!==ch){ this.dropChunk(old); N.chunks.delete(old.key); }
    }
    return ch;
  },
  dropDim(ch){ if(ch.fbm){ ch.fbm.close(); ch.fbm=null; } ch.fkey=null; },
  dropChunk(ch){ if(ch.bm){ ch.bm.close(); ch.bm=null; } this.dropDim(ch); ch.cv.width=1; ch.cv.height=1; },
  /** What a frame draws for a chunk: its dimmed bitmap under a filter, its bitmap, or its
      canvas while a bitmap is being made - a canvas source is re-uploaded on every draw,
      which is fine for a frame or two of a handful. The bitmaps are made behind the frame. */
  chunkSource(w,ch){
    const N=w.near, m=this.matches;
    if(ch.stale && !ch.making){
      ch.making=true; ch.stale=false;
      createImageBitmap(ch.cv).then(bm=>{
        ch.making=false;
        if(this.world!==w || !N.chunks.has(ch.key)){ bm.close(); return; }
        if(ch.bm) ch.bm.close();
        ch.bm=bm;
        this.redraw();   // and if a tile landed meanwhile, `stale` is set again and the next frame remakes it
      }).catch(()=>{ ch.making=false; ch.stale=true; });
    }
    if(m){
      if(ch.fbm && ch.fkey===m) return {src:ch.fbm, dimmed:true};
      if(!ch.fmaking && ch.bm && !ch.stale) this.composeChunk(w,ch,m);
      return {src:ch.bm||ch.cv, dimmed:false};
    }
    return {src:ch.bm||ch.cv, dimmed:false};
  },
  /** The dimmed picture of one chunk for the filter `m`, made behind the frame. */
  composeChunk(w,ch,m){
    const N=w.near;
    ch.fmaking=true;
    (async()=>{
      try{
        const fbm=await this.composePixels(w, ch.ctx, ch.cv.width, ch.cv.height, N.S, ch.u0, ch.v0, ch.cw, ch.ch, m);
        if(!fbm) return;
        if(this.world!==w || this.matches!==m || !N.chunks.has(ch.key)){ fbm.close(); return; }
        if(ch.fbm) ch.fbm.close();
        ch.fbm=fbm; ch.fkey=m;
        this.redraw();
      }finally{ ch.fmaking=false; }
    })();
  },

  /** The dimmed picture for one level: the atlas's pixels with every cell that does not
      match stepped back into the panel, made once and drawn as one bitmap. Whichever of
      the two sets is smaller is the one walked — a filter for one town dims fifteen
      thousand cells, a filter for "wilderness" lights them. */
  async composeLevel(w,L){
    const m=this.matches; if(!m){ if(L.fbm){ L.fbm.close(); L.fbm=null; } return; }
    const U=w.max[0]-w.min[0]+1, V=w.max[1]-w.min[1]+1;
    const fbm=await this.composePixels(w, L.ctx, L.cv.width, L.cv.height, L.S, 0, 0, U, V, m);
    if(!fbm) return;
    if(this.world!==w || this.matches!==m){ fbm.close(); return; }
    if(L.fbm) L.fbm.close();
    L.fbm=fbm;
  },
  /** The dimming itself, on a canvas that holds the cells from (u0, v0), `cw` by `ch` of
      them at `S` pixels each: a bitmap of its pixels with every non-matching cell stepped
      back into the panel. Whichever of the two sets is smaller is the one walked. Shared
      by the whole-world levels and the chunks (round 18cv). */
  async composePixels(w,ctx,cwPx,chPx,S,u0,v0,cw,ch,m){
    const img=ctx.getImageData(0,0,cwPx,chPx);
    const d=img.data, row=cwPx*4;
    const th=this.theme(); const bg=this.chans(th.bg); const a=0.55, b=1-a;
    const dimCell=(u,sv)=>{
      let o=((sv-v0)*S*cwPx+(u-u0)*S)*4;
      for(let py=0;py<S;py++,o+=row){
        for(let px=0,q=o;px<S;px++,q+=4){
          d[q]=d[q]*b+bg[0]*a; d[q+1]=d[q+1]*b+bg[1]*a; d[q+2]=d[q+2]*b+bg[2]*a;
        }
      }
    };
    if(m.size*2 < cw*ch){
      /* Few matches: dim the lot, then paint the matches back from the untouched pixels. */
      for(let i=0;i<d.length;i+=4){ d[i]=d[i]*b+bg[0]*a; d[i+1]=d[i+1]*b+bg[1]*a; d[i+2]=d[i+2]*b+bg[2]*a; }
      const src=ctx.getImageData(0,0,cwPx,chPx).data;
      for(const k of m){
        const [x,y]=k.split(',').map(Number);
        const u=x-w.min[0], sv=w.max[1]-y;
        if(u<u0||sv<v0||u>=u0+cw||sv>=v0+ch) continue;
        let o=((sv-v0)*S*cwPx+(u-u0)*S)*4;
        for(let py=0;py<S;py++,o+=row) d.set(src.subarray(o,o+S*4),o);
      }
    }else{
      /* Many matches: dim only what does not match. */
      for(let sv=v0;sv<v0+ch;sv++) for(let u=u0;u<u0+cw;u++){
        if(!m.has((u+w.min[0])+','+(w.max[1]-sv))) dimCell(u,sv);
      }
    }
    return createImageBitmap(img);
  },

  /** A colour's three channels, from a hex or rgb() string. */
  chans(col){
    const m=/^#([0-9a-f]{6})$/i.exec(col||'');
    if(m){ const n=parseInt(m[1],16); return [n>>16,(n>>8)&255,n&255]; }
    const r=/rgba?\(([^)]+)\)/.exec(col||'');
    if(r){ const p=r[1].split(/[\s,\/]+/).filter(Boolean).map(Number); return [p[0],p[1],p[2]]; }
    return [0,0,0];
  },

  /** Where the view looks when the map opens (round 18cw). Opened from a Real cell preview
      it is centred on the loaded cell, at whatever zoom the map was left at; opened any
      other way - the first time, from Simplified mode, with an interior loaded - it shows
      the whole of Vvardenfell (`ISLAND`), or the whole world when the world has no
      Vvardenfell in it, zoomed out as far as that takes and never closer than the default.
      Robin: "If I open the map cell picker for the first time, or from simplified preview
      or an interior, I want to center it so the entire island of Vvardenfell ... to be
      revealed first. If I open it from a full cell preview, I want it centered over the
      actual cell I have open." Before this the view was kept for the session, and a map
      reopened from Simplified mode landed wherever the last look had ended - "centered
      over the sea far off". */
  home(){
    const w=this.world; if(!w||!w.min) return;
    /* Round 18da: nothing to frame for until the map has a size - the read-ahead (18cx)
       homes with the picker never opened, and a fit worked out over a 0×0 box is the
       minimum zoom, which the next opening from a Real cell preview then kept as "the zoom
       the map was left at". `open` homes again once the box has its size. */
    if(!(this.W>1 && this.H>1)) return;
    this.zoom=null;                       // a glide still going was heading for the old view
    const sel=App.cellSel;
    let z, cx, cy;
    if(App.mode==='cell' && sel && sel.kind==='ext'){
      z=(this.view && this.view.sig===w.sig)? this.view.z : this.Z0;
      cx=sel.x-w.min[0]+0.5; cy=w.max[1]-sel.y+0.5;
    }else{
      const b=this.frame(w);
      const U=b.x1-b.x0+1, V=b.y1-b.y0+1;
      // All of it on screen with a cell of sea around; the default zoom when that is closer.
      z=Math.max(this.ZMIN, Math.min(this.Z0, this.W/(U+2), this.H/(V+2)));
      cx=(b.x0+b.x1+1)/2-w.min[0]; cy=w.max[1]-(b.y0+b.y1)/2+0.5;
    }
    this.view={sig:w.sig, z, cx0:cx-this.W/(2*z), cy0:cy-this.H/(2*z)};
    this.clamp();
  },
  /** The box `home` frames: Vvardenfell where the world has a landscape in it - as much of
      that box as the world reaches, so a small world near the origin is framed as itself
      and not as an empty corner of the island - else the world's own extent. */
  frame(w){
    const I=this.ISLAND, W={x0:w.min[0], y0:w.min[1], x1:w.max[0], y1:w.max[1]};
    if(!w.cells.some(c=>c[0]>=I.x0 && c[0]<=I.x1 && c[1]>=I.y0 && c[1]<=I.y1)) return W;
    return {x0:Math.max(I.x0,W.x0), y0:Math.max(I.y0,W.y0), x1:Math.min(I.x1,W.x1), y1:Math.min(I.y1,W.y1)};
  },

  say(text){ if(!this.note) return; this.note.textContent=text; this.note.hidden=!text; },

  /* ---- coordinates ----------------------------------------------------------------- */
  cellAt(px,py){
    const w=this.world, v=this.view; if(!w||!w.min||!v) return null;
    const u=Math.floor(v.cx0+px/v.z), s=Math.floor(v.cy0+py/v.z);
    return {x:u+w.min[0], y:w.max[1]-s};
  },
  /** Screen rectangle of a cell, edges rounded so neighbours share a pixel column. */
  rect(x,y){
    const w=this.world, v=this.view;
    const u=x-w.min[0], s=w.max[1]-y;
    const x0=Math.round((u-v.cx0)*v.z), x1=Math.round((u+1-v.cx0)*v.z);
    const y0=Math.round((s-v.cy0)*v.z), y1=Math.round((s+1-v.cy0)*v.z);
    return [x0,y0,x1-x0,y1-y0];
  },
  /** Keeps some of the world on screen: a pan cannot lose the map entirely. */
  clamp(){
    const w=this.world, v=this.view; if(!w||!w.min||!v) return;
    const U=w.max[0]-w.min[0]+1, V=w.max[1]-w.min[1]+1;
    const wu=this.W/v.z, wv=this.H/v.z;
    v.cx0=Math.min(Math.max(v.cx0, -wu+1), U-1);
    v.cy0=Math.min(Math.max(v.cy0, -wv+1), V-1);
  },

  /* ---- drawing --------------------------------------------------------------------- */
  redraw(){
    if(this._raf) return;
    this._raf=requestAnimationFrame(()=>{ this._raf=0; this.draw(); });
  },
  /* Wraithguard: the theme's colours and the body font, read once and kept. They were read
     with getComputedStyle on every frame, which makes the browser settle the whole page's
     styles before each draw - a large share of a pan's frame. `forgetTheme` drops them
     when the theme changes (Themes.apply). */
  theme(){
    if(this._th) return this._th;
    const s=getComputedStyle(document.body);
    const g=k=>s.getPropertyValue(k).trim();
    this._font=s.fontFamily;
    return (this._th={ac:g('--ac'), ac2:g('--ac2'), line:g('--line2'), bg:g('--bg'), info:g('--info'),
                      warn:g('--warn'), tx:g('--tx')});
  },
  forgetTheme(){ this._th=null; this._font=null; this.redraw(); },
  draw(){
    const c=this.ctx; if(!c) return;
    const {W,H,dpr}=this;
    c.setTransform(dpr,0,0,dpr,0,0);
    c.fillStyle=this.SEA; c.fillRect(0,0,W,H);
    const w=this.world, v=this.view;
    if(!w||!w.min||!v) return;
    const th=this.theme();
    const U=w.max[0]-w.min[0]+1, V=w.max[1]-w.min[1]+1;
    const u0=Math.max(0,Math.floor(v.cx0)), u1=Math.min(U-1,Math.ceil(v.cx0+W/v.z));
    const s0=Math.max(0,Math.floor(v.cy0)), s1=Math.min(V-1,Math.ceil(v.cy0+H/v.z));
    c.imageSmoothingEnabled=true; c.imageSmoothingQuality='high';
    /* Where a frame's time goes, section by section, when `CellMap.prof` is an object —
       the question a slow map raises, answered from inside rather than guessed at. */
    const prof=this.prof; let tp=prof? performance.now() : 0;
    const lap=k=>{ if(prof){ const t=performance.now(); prof[k]=(prof[k]||0)+(t-tp); tp=t; } };

    // 1. The picture: the atlas in one draw at or under its scale, the per-cell tiles
    //    above it, where there are few enough on screen to draw one by one.
    const dim=this.matches;
    /* The level whose scale is the first at or above the zoom, so the atlas is scaled
       down by at most half and never up; none above the largest, where the tiles draw. */
    let L=null;
    for(let i=w.levels.length-1;i>=0;i--){ if(v.z<=w.levels[i].S){ L=w.levels[i]; break; } }
    const level = L && (dim? (L.fbm||L.bm||L.cv) : (L.bm||L.cv));
    if(level){
      if(dim){ c.fillStyle=this.rgba(th.bg,0.55); c.fillRect(0,0,W,H); }   // the open sea steps back too
      /* The source rectangle the view covers, in atlas pixels, cut to the atlas — a source
         outside the image is clipped by the browser, but not every browser scales the
         destination to match, so the intersection is taken here. */
      const S=L.S, aw=L.cv.width, ah=L.cv.height;
      let sx=v.cx0*S, sy=v.cy0*S, sw=W/v.z*S, sh=H/v.z*S;
      let dx=0, dy=0, dw=W, dh=H;
      if(sx<0){ const k=-sx/sw; dx+=dw*k; dw-=dw*k; sw+=sx; sx=0; }
      if(sy<0){ const k=-sy/sh; dy+=dh*k; dh-=dh*k; sh+=sy; sy=0; }
      if(sx+sw>aw){ const k=(sx+sw-aw)/sw; dw-=dw*k; sw=aw-sx; }
      if(sy+sh>ah){ const k=(sy+sh-ah)/sh; dh-=dh*k; sh=ah-sy; }
      if(sw>0&&sh>0) c.drawImage(level,sx,sy,sw,sh,dx,dy,dw,dh);
      /* A filter composed for the level is drawn already dimmed; until it is (the compose
         runs behind a keystroke), or with no atlas at all, the dim goes on by hand. */
      if(dim && !L.fbm){
        c.fillStyle=this.rgba(th.bg,0.55); c.fillRect(0,0,W,H);
        for(const k of dim){
          const [x,y]=k.split(',').map(Number);
          const bm=w.tiles.get(k); if(!bm) continue;
          const [rx,ry,rw,rh]=this.rect(x,y);
          if(rx+rw<0||ry+rh<0||rx>W||ry>H) continue;
          c.drawImage(bm,rx,ry,rw,rh);
        }
      }
      lap('picture');
    }else if(w.near){
      /* Round 18cv: the chunks the view reaches, a handful of draws. Each chunk's screen
         edges are rounded so neighbours share a pixel column, as the cells' are. A chunk
         whose dimmed picture is not made yet is drawn plain and dimmed by hand below, with
         the matches on it painted back per tile; the composed one arrives a frame later. */
      const N=w.near, CH=N.CH;
      const cu0=Math.max(0,Math.floor(u0/CH)), cu1=Math.floor(u1/CH);
      const cv0=Math.max(0,Math.floor(s0/CH)), cv1=Math.floor(s1/CH);
      if(dim){ c.fillStyle=this.rgba(th.bg,0.55); c.fillRect(0,0,W,H); }   // the open sea steps back too
      let handDim=false;
      for(let cv=cv0;cv<=cv1;cv++) for(let cu=cu0;cu<=cu1;cu++){
        const ch=this.chunkAt(w,cu,cv); if(!ch) continue;
        const {src,dimmed}=this.chunkSource(w,ch);
        const x0=Math.round((ch.u0-v.cx0)*v.z), x1=Math.round((ch.u0+ch.cw-v.cx0)*v.z);
        const y0=Math.round((ch.v0-v.cy0)*v.z), y1=Math.round((ch.v0+ch.ch-v.cy0)*v.z);
        if(x1<=0||y1<=0||x0>=W||y0>=H) continue;
        if(dim && !dimmed){
          c.drawImage(src,x0,y0,x1-x0,y1-y0);
          c.fillStyle=this.rgba(th.bg,0.55); c.fillRect(x0,y0,x1-x0,y1-y0);
          handDim=true;
        }else c.drawImage(src,x0,y0,x1-x0,y1-y0);
      }
      lap('picture');
      if(dim && handDim){
        for(let s=s0;s<=s1;s++){
          const y=w.max[1]-s;
          for(let u=u0;u<=u1;u++){
            const x=u+w.min[0], k=x+','+y;
            if(!dim.has(k)) continue;
            const ch=N.chunks.get(Math.floor(u/CH)+','+Math.floor(s/CH));
            if(ch && ch.fbm && ch.fkey===dim) continue;   // that chunk was drawn dimmed already
            const bm=w.tiles.get(k); if(!bm) continue;
            const [rx,ry,rw,rh]=this.rect(x,y);
            c.drawImage(bm,rx,ry,rw,rh);
          }
        }
      }
      lap('dim');
    }else{
      // The tiles, one by one - only when there is no near level at all.
      for(let s=s0;s<=s1;s++){
        const y=w.max[1]-s;
        for(let u=u0;u<=u1;u++){
          const x=u+w.min[0];
          const bm=w.tiles.get(x+','+y); if(!bm) continue;
          const [rx,ry,rw,rh]=this.rect(x,y);
          c.drawImage(bm,rx,ry,rw,rh);
        }
      }
      lap('picture');
      // 2. The filter, per cell: everything steps back, then the matches on screen are
      //    painted again on top. The same look the composed atlas gives at lower zooms.
      if(dim){
        c.fillStyle=this.rgba(th.bg,0.55); c.fillRect(0,0,W,H);
        for(let s=s0;s<=s1;s++){
          const y=w.max[1]-s;
          for(let u=u0;u<=u1;u++){
            const x=u+w.min[0], k=x+','+y;
            if(!dim.has(k)) continue;
            const bm=w.tiles.get(k); if(!bm) continue;
            const [rx,ry,rw,rh]=this.rect(x,y);
            c.drawImage(bm,rx,ry,rw,rh);
          }
        }
      }
      lap('dim');
    }

    // Wraithguard: the mods-per-cell heat, as translucent masks over the picture.
    if(typeof WgCoverage==='object') WgCoverage.drawOverlay(c,this);
    lap('coverage');
    // 3. The grid, over the world's own extent and no further — there are no cells in
    //    the open sea — and fainter as the cells get small, so at six pixels a cell it
    //    is a texture rather than a cage.
    /* About a third at the default eighteen pixels, three quarters at the largest: the
       lines are there to say where one cell ends, not to draw a cage over the picture. */
    const ga=Math.min(0.75,Math.max(0.2,(v.z-4)/40));
    const gx0=Math.round((0-v.cx0)*v.z)+0.5, gx1=Math.round((U-v.cx0)*v.z)+0.5;
    const gy0=Math.round((0-v.cy0)*v.z)+0.5, gy1=Math.round((V-v.cy0)*v.z)+0.5;
    c.strokeStyle=this.rgba(th.line,ga); c.lineWidth=1;
    c.beginPath();
    for(let u=u0;u<=u1+1;u++){ const X=Math.round((u-v.cx0)*v.z)+0.5; c.moveTo(X,Math.max(0,gy0)); c.lineTo(X,Math.min(H,gy1)); }
    for(let s=s0;s<=s1+1;s++){ const Y=Math.round((s-v.cy0)*v.z)+0.5; c.moveTo(Math.max(0,gx0),Y); c.lineTo(Math.min(W,gx1),Y); }
    c.stroke();

    lap('grid');
    // 4. The matches, ringed in the accent — one path, one stroke, however many.
    if(dim){
      c.strokeStyle=th.ac2; c.lineWidth=1;
      c.beginPath();
      for(const k of dim){
        const [x,y]=k.split(',').map(Number);
        const [rx,ry,rw,rh]=this.rect(x,y);
        if(rx+rw<0||ry+rh<0||rx>W||ry>H) continue;
        c.rect(rx+0.5,ry+0.5,rw-1,rh-1);
      }
      c.stroke();
    }

    lap('rings');
    // 5. Stars.
    if(typeof Favourites==='object'){
      const starred=Favourites.list('cell');
      if(starred.length){
        c.fillStyle=th.warn;
        const fs=Math.max(8,Math.min(14,v.z*0.6));
        /* The window's own face, at the size the cell allows — read off the body rather
           than named here, which is also what keeps the wording check from taking a font
           for a sentence. */
        c.font=Math.round(fs)+'px '+(this._font||getComputedStyle(document.body).fontFamily);
        c.textAlign='right'; c.textBaseline='top';
        for(const k of starred){
          const m=/^(-?\d+),(-?\d+)$/.exec(String(k)); if(!m) continue;
          const [rx,ry,rw,rh]=this.rect(+m[1],+m[2]);
          if(rx+rw<0||ry+rh<0||rx>W||ry>H) continue;
          if(v.z>=12) c.fillText('★',rx+rw-2,ry+1);
          else { c.beginPath(); c.arc(rx+rw-3,ry+3,2,0,Math.PI*2); c.fill(); }
        }
      }
    }

    lap('stars');
    // 6. The loaded cell and its adjacent ring.
    const sel=App.cellSel;
    if(sel && sel.kind==='ext'){
      const r=App.showAdjacent|0;
      const [rx,ry,rw,rh]=this.rect(sel.x,sel.y);
      if(r>0){
        const [ax,ay]=this.rect(sel.x-r,sel.y+r), [bx,by,bw,bh]=this.rect(sel.x+r,sel.y-r);
        c.setLineDash([4,3]); c.strokeStyle=th.info; c.lineWidth=1;
        c.strokeRect(ax+0.5,ay+0.5,(bx+bw)-ax-1,(by+bh)-ay-1);
        c.setLineDash([]);
      }
      c.strokeStyle=th.info; c.lineWidth=2;
      c.strokeRect(rx+1,ry+1,rw-2,rh-2);
    }

    lap('current');
    // 7. The cell under the pointer.
    const h=this.hover;
    if(h && this.known(h.x,h.y)){
      const [rx,ry,rw,rh]=this.rect(h.x,h.y);
      c.fillStyle='rgba(255,255,255,0.09)'; c.fillRect(rx,ry,rw,rh);
      c.strokeStyle=th.ac; c.lineWidth=2;
      c.strokeRect(rx+1,ry+1,rw-2,rh-2);
    }
  },
  /** A hex or rgb() colour at an alpha, for the canvas. */
  rgba(col,a){
    const m=/^#([0-9a-f]{6})$/i.exec(col||'');
    if(m){ const n=parseInt(m[1],16); return 'rgba('+(n>>16)+','+((n>>8)&255)+','+(n&255)+','+a+')'; }
    const r=/rgba?\(([^)]+)\)/.exec(col||'');
    if(r){ const p=r[1].split(/[\s,\/]+/).filter(Boolean); return 'rgba('+p[0]+','+p[1]+','+p[2]+','+a+')'; }
    return col;
  },
  /** A cell the picker knows — in the world's grid, whether or not it has a landscape. */
  known(x,y){ return this.cellsByKey.has(x+','+y); },

  /* ---- the pointer ----------------------------------------------------------------- */
  bind(cv){
    cv.addEventListener('pointermove',e=>this.onMove(e));
    cv.addEventListener('pointerleave',()=>{ this.ptr=null; this.hover=null; this.showTip(null); this.redraw(); });
    /* Round 18cw: while the middle button is down the map is being panned and nothing
       else. Robin: "hide selection and tooltip immediately, and not reveal it again until I
       release middle mouse button. As long as middle mouse button is pressed, I don't want
       to accidentally press anything else (either select a cell or right click to favorite
       it), so block all other mouse input than panning then." The highlight and the note go
       at the press, not at the first move; a left or right press meanwhile is not
       remembered, so nothing it began can land later (`onClick` wants a press, `onStar` a
       right press); the wheel waits too; and the release is what brings the highlight and
       the note back, where the pointer is, without a move.

       The pan is tied to the *button*, not to the events: a pointer gets one `pointerdown`
       when its first button goes down and one `pointerup` when its last comes up, and a
       button pressed or released while another is held is a `pointermove` with `button`
       set. So a middle button that comes up while the left is still down, or goes down
       while it is, is caught in `onMove` by `buttons` - otherwise the map would go on
       panning with nothing pressed, and a pan begun under a held left button would not
       begin at all. */
    cv.addEventListener('pointerdown',e=>{
      if(e.button===1){ e.preventDefault(); this.startPan(e); return; }
      if(this.drag){ e.preventDefault(); return; }
      /* Where the left button went down, so a click can be told from a drag that
         happened to end over the map: the browser sends `click` for both. */
      if(e.button===0) this.press={x:e.clientX,y:e.clientY};
      /* And which cell the right button went down on. Robin: "I don't want to accidentally
         favorite something, so lets make it so that both click and release click on right
         mouse button needs to happen in the same cell for it to be favorited." The
         browser's `contextmenu` arrives at the *release*, so the press is remembered and
         the star only lands when the two agree. */
      if(e.button===2){ const [px,py]=this.local(e); this.rpress=this.cellAt(px,py); }
    });
    cv.addEventListener('pointerup',e=>{
      if(this.drag){ if(!(e.buttons&(this.drag.left? 1 : 4))) this.endPan(e,true); return; }
      /* The star lands on the *release* of the right button, and only where it went down.
         Not on `contextmenu`: Windows sends that on the release and Linux on the press, and
         a rule about press and release agreeing cannot be built on an event that is one or
         the other depending on the machine. */
      if(e.button===2) this.onStar(e);
    });
    cv.addEventListener('pointercancel',e=>{ if(this.drag) this.endPan(e,false); });
    /* The middle button's own default in Chromium is autoscroll, which starts on
       mousedown and would fight the pan; and the browser would offer a context menu on
       the right button, which is the star's now. */
    cv.addEventListener('mousedown',e=>{ if(e.button===1) e.preventDefault(); });
    cv.addEventListener('auxclick',e=>e.preventDefault());
    cv.addEventListener('contextmenu',e=>e.preventDefault());
    cv.addEventListener('click',e=>{ if(e.button===0 && !this.drag) this.onClick(e); });
    cv.addEventListener('wheel',e=>{ e.preventDefault(); if(!this.drag) this.onWheel(e); },{passive:false});
    this.bindKeys();
  },

  /* ---- Wraithguard: the keyboard ------------------------------------------------------
     Arrow keys pan while held - smoothly, per frame, Shift for faster - and + / - zoom
     about the middle of the map with the same glide as the wheel. Only while the map is
     showing, and never while a text field (the picker's filter) has the keys. */
  _keys:null, _keyRaf:0, _keyLast:0,
  showing(){ return !!(this.canvas && this.canvas.offsetParent && this.view); },
  bindKeys(){
    if(this._keys) return;
    this._keys=new Set();
    const typing=t=>t && (t.tagName==='INPUT'||t.tagName==='TEXTAREA'||t.isContentEditable);
    const ARROWS={ArrowLeft:1, ArrowRight:1, ArrowUp:1, ArrowDown:1};
    document.addEventListener('keydown',e=>{
      if(!this.showing()) return;
      if(ARROWS[e.key]){
        if(typing(e.target) && e.target.type!=='checkbox') { if(e.target.value) return; e.target.blur(); }
        e.preventDefault();
        this._keys.add(e.key); this._keys.shift=e.shiftKey;
        if(!this._keyRaf){ this._keyLast=0; this._keyRaf=requestAnimationFrame(t=>this.keyStep(t)); }
        return;
      }
      if(typing(e.target)) return;
      if(e.key==='+'||e.key==='='||e.key==='-'||e.key==='_'){
        e.preventDefault();
        const r=this.canvas.getBoundingClientRect();
        this.onWheel({preventDefault(){}, deltaY:(e.key==='-'||e.key==='_')? 1 : -1,
                      clientX:r.left+this.W/2, clientY:r.top+this.H/2});
      }
    });
    document.addEventListener('keyup',e=>{ this._keys.delete(e.key); this._keys.shift=e.shiftKey; });
    window.addEventListener('blur',()=>this._keys.clear());
  },
  /** One frame of a keyboard pan: screen pixels per second, so it feels the same at
      every zoom. */
  keyStep(now){
    this._keyRaf=0;
    const k=this._keys, v=this.view;
    if(!k.size || !v || !this.showing()){ k.clear(); return; }
    const dt=this._keyLast? Math.min(0.05,(now-this._keyLast)/1000) : 1/60;
    this._keyLast=now;
    const px=(k.shift? 1500 : 650)*dt;
    const dx=(k.has('ArrowRight')?1:0)-(k.has('ArrowLeft')?1:0);
    const dy=(k.has('ArrowDown')?1:0)-(k.has('ArrowUp')?1:0);
    if(this.zoom){ this.zoom.u+=dx*px/v.z; this.zoom.s+=dy*px/v.z; }
    v.cx0+=dx*px/v.z; v.cy0+=dy*px/v.z;
    this.clamp(); this.hoverAt(); this.redraw();
    this._keyRaf=requestAnimationFrame(t=>this.keyStep(t));
  },
  local(e){ const r=this.canvas.getBoundingClientRect(); return [e.clientX-r.left, e.clientY-r.top]; },
  /** The middle button is down: the map pans and does nothing else (round 18cw). */
  startPan(e){
    this.drag={x:e.clientX,y:e.clientY,moved:false};
    try{ this.canvas.setPointerCapture(e.pointerId); }catch(_){ }
    this.press=null; this.rpress=null;
    this.hover=null; this.showTip(null); this.redraw();
  },
  /** And up: the highlight and the note return under the pointer, when it is over the map. */
  endPan(e,rehover){
    this.drag=null;
    try{ this.canvas.releasePointerCapture(e.pointerId); }catch(_){ }
    if(!rehover) return;
    const [px,py]=this.local(e);
    if(px>=0 && py>=0 && px<this.W && py<this.H) this.onMove(e);
  },
  onMove(e){
    /* Wraithguard: a left press that travels becomes a pan too - a click stays a click,
       but dragging the map with the button everybody reaches for first now moves it.
       `startPan` forgets the press, so no cell is picked at the release. */
    if(!this.drag && this.press && (e.buttons&1) &&
       Math.hypot(e.clientX-this.press.x, e.clientY-this.press.y)>5){
      this.startPan(e); this.drag.left=true; return;
    }
    // The middle button changing under another button arrives here, not as down or up.
    if(typeof e.buttons==='number' && e.buttons>=0){
      const held=!!(e.buttons&(this.drag && this.drag.left? 1 : 4));
      if(this.drag && !held){ this.endPan(e,true); return; }
      if(!this.drag && (e.buttons&4)){ this.startPan(e); return; }
    }
    const [px,py]=this.local(e);
    this.ptr={px,py};
    if(this.drag){
      const dx=e.clientX-this.drag.x, dy=e.clientY-this.drag.y;
      this.drag.x=e.clientX; this.drag.y=e.clientY; this.drag.moved=true;
      // A pan under a glide moves the glide's anchor with it, so the two do not fight.
      if(this.zoom){ this.zoom.u-=dx/this.view.z; this.zoom.s-=dy/this.view.z; }
      if(this.view){ this.view.cx0-=dx/this.view.z; this.view.cy0-=dy/this.view.z; this.clamp(); }
      this.hover=null; this.showTip(null); this.redraw();
      return;
    }
    this.hoverAt();
  },
  /** The highlight and the note, under the pointer where it is now - `ptr`, the last
      position a move reported, or nothing when it has left or the map is being panned.
      Round 18cx: the one place that decides what is hovered, for a move and for every
      frame of a zoom's glide alike. Robin: "if I zoom and move the mouse at the same
      time, but stop the mouse before the zoom motion has settled, the selected cell will
      be where my cursor was when the zoom started (the pivot / center the zoom happens
      around). The selection should always be where the cursor is." The glide used to
      re-hover at its anchor each frame, so a moved pointer was overruled until the
      next move; the anchor is still what the zoom is about, and the hover is not. */
  hoverAt(){
    const p=this.ptr, was=this.hover;
    let c=null;
    if(p && !this.drag){ c=this.cellAt(p.px,p.py); if(c && !this.known(c.x,c.y)) c=null; }
    this.hover=c;
    if((was&&was.x)!==(c&&c.x) || (was&&was.y)!==(c&&c.y)) this.redraw();
    if(p) this.showTip(c, p.px, p.py); else this.showTip(null);
  },
  /* Round 18cv: a wheel notch names where the zoom is going and the view glides there,
     with the same easing the scene's camera uses (`_orbitStep` in 06_gl.js): each frame
     closes the gap by 1 - exp(-dt * k * 0.35), k the orbit smoothing setting, and lands
     when what is left could not move a pixel. Robin: "The zoom on the map should be
     smoothed to the steps. Use the same smoothing formula as for the camera movement in
     the scene." The map unit under the cursor at the notch stays under the cursor for the
     whole glide, by construction. */
  onWheel(e){
    e.preventDefault();
    const v=this.view; if(!v) return;
    const [px,py]=this.local(e);
    const k=e.deltaY<0? 1.15 : 1/1.15;
    const g=this.zoom||{goal:v.z};
    const z2=Math.min(this.ZMAX,Math.max(this.ZMIN,g.goal*k));
    if(z2===g.goal && !this.zoom) return;
    // The map unit under the cursor stays under the cursor - remembered at the notch,
    // and held through every frame of the glide.
    const u=v.cx0+px/v.z, s=v.cy0+py/v.z;
    this.ptr={px,py};
    this.zoom={goal:z2, px, py, u, s, last:0};
    this.zoomStep(performance.now());
  },
  /** One frame of the zoom's glide. Returns true while it is still going. */
  zoomStep(now){
    const g=this.zoom, v=this.view; if(!g||!v){ this.zoom=null; return false; }
    const dt=g.last? Math.min(0.1,Math.max(0,(now-g.last)/1000)) : 0; g.last=now;
    const R=(typeof App==='object' && App.R)||null;
    const k=Math.max(1,Math.min(100,(R && +R.orbitSmooth)||40));
    const f=1-Math.exp(-dt*k*0.35);
    /* What may be thrown away: a zoom change dz moves the point r pixels from the cursor
       by r·dz/z, and the furthest point on the canvas is a diagonal away. */
    const halfDiag=Math.hypot(this.W,this.H)/2||1;
    const settle=Math.max(1e-4, SETTLE_PX*v.z/halfDiag);
    let z;
    if(Math.abs(g.goal-v.z)<settle){ z=g.goal; this.zoom=null; }
    else z=v.z+(g.goal-v.z)*f;
    v.z=z; v.cx0=g.u-g.px/z; v.cy0=g.s-g.py/z;
    this.clamp();
    this.hoverAt();          // under the pointer where it is, not where the notch was (18cx)
    this.redraw();
    if(this.zoom) requestAnimationFrame(t=>this.zoomStep(t));
    return !!this.zoom;
  },
  onClick(e){
    const pr=this.press; this.press=null;
    if(!pr) return;                                   // a press that began during a pan (round 18cw)
    if(Math.hypot(e.clientX-pr.x, e.clientY-pr.y)>4) return;   // a drag, not a click
    const [px,py]=this.local(e);
    const c=this.cellAt(px,py);
    if(!c||!this.known(c.x,c.y)) return;
    const cell=this.cellsByKey.get(c.x+','+c.y);
    if(this.onPick) this.onPick(cell);
  },
  onStar(e){
    const [px,py]=this.local(e);
    const c=this.cellAt(px,py);
    const down=this.rpress; this.rpress=null;
    if(!c||!this.known(c.x,c.y)||typeof Favourites!=='object') return;
    if(!down || down.x!==c.x || down.y!==c.y) return;   // pressed on one cell, released on another
    Favourites.toggle('cell', c.x+','+c.y);
    this.redraw(); this.showTip(this.hover, px, py);
  },

  /* ---- the note under the pointer -------------------------------------------------- */
  tipText(c){
    const cell=this.cellsByKey.get(c.x+','+c.y); if(!cell) return '';
    const region=cell.region||'';
    /* Two lines, joined here rather than in the catalogue: a newline inside a catalogue
       string breaks the language template, whose English rides in a comment. */
    let s;
    if(cell.name) s = cell.name+'\n'+(region? T('cell.map_tip_where',{x:c.x,y:c.y,region})
                                            : T('cell.map_tip_where_noregion',{x:c.x,y:c.y}));
    else s=(region||T('cell.map_wilderness'))+'\n'+T('cell.map_tip_where_noregion',{x:c.x,y:c.y});
    const sel=App.cellSel;
    if(sel && sel.kind==='ext' && sel.x===c.x && sel.y===c.y) s+=' · '+T('cell.map_current');
    // Wraithguard: which mods touch it.
    if(typeof WgCoverage==='object') s+=WgCoverage.tip({kind:'ext',x:c.x,y:c.y});
    const on=typeof Favourites==='object' && Favourites.has('cell', c.x+','+c.y);
    s+='\n'+T(on? 'cell.map_star_note' : 'cell.map_star_note_off');
    return s;
  },
  showTip(c,px,py){
    const t=this.tip; if(!t) return;
    if(!c){ t.hidden=true; return; }
    t.textContent=this.tipText(c);
    t.hidden=false;
    // Beside the pointer, and kept inside the box: flipped to the left near the right
    // edge and above near the bottom, so the note never runs off the map.
    const pad=14;
    let x=px+pad, y=py+pad;
    const tw=t.offsetWidth, thh=t.offsetHeight;
    if(x+tw>this.W-4) x=Math.max(4,px-pad-tw);
    if(y+thh>this.H-4) y=Math.max(4,py-pad-thh);
    t.style.left=x+'px'; t.style.top=y+'px';
  },
};
