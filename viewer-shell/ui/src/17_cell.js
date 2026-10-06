/* =====================================================================================
   One cell's contents, as the engine read them.

   The engine has the whole load order merged: which reference a later plugin replaced,
   what a moved one now belongs to, which plugin wrote a landscape and therefore what
   its texture indices mean. One call per cell hands all of that over.

   The page used to do this itself — walk every plugin, parse CELL and LAND records,
   resolve reference numbers against master lists. It could not survive a real load
   order: pulling 583 MB through a webview is not something to optimise, it is
   something to stop doing. And it was the second implementation of merge rules that
   only one of the two could get right, because only one of them holds the whole world.

   What is left here is the geometry the renderer needs from a cell it has been given.
   ===================================================================================== */

/** The engine sends a coverage map as one bit per texel; the shader wants an alpha
    channel. 41 bytes a texture a cell, against the 84 KB of vertices the page builds
    for the same ground — size is not why this moved, the rule is: the outermost texel
    of a cell's map comes from the *neighbouring* landscape, and the engine holds every
    landscape while the page holds at most nine. */
function unpackLayer(l){
  const P=18, hex=String(l.mask||'');
  const bytes=new Uint8Array(hex.length>>1);
  for(let b=0;b<bytes.length;b++) bytes[b]=parseInt(hex.substr(b*2,2),16);
  const blend=new Uint8Array(P*P);
  // 255, not 1: this is an alpha channel the shader filters, not a flag.
  for(let t=0;t<P*P;t++) if((bytes[t>>3]>>(t&7))&1) blend[t]=255;
  return {id:l.id||'', blend, size:P, coverage:l.coverage||0};
}

/** A landscape record is 65x65 height samples over one cell, and 16x16 texture
    sub-cells. Both are fixed by the format. */
const LAND_VERTS=65;
const LAND_SUBDIV=16;

/** The vertex normals `cell_data` sends: three signed 16-bit values per vertex, hex.

    Signed 16-bit rather than bytes because these also drive the red slope overlay, which
    is compared against `fMaximumAngle` — a coarser encoding would leave a visible margin
    of error along the exact edge the overlay exists to show. */
function unpackNormals(hex){
  if(!hex) return null;
  const n=hex.length/4|0;
  const out=new Float32Array(n);
  for(let i=0;i<n;i++){
    let v=parseInt(hex.substr(i*4,4),16);
    if(v>0x7fff) v-=0x10000;
    out[i]=v/32767;
  }
  return out;
}

/** The record's vertex colours, VCLR as bytes in hex — 65x65 RGB — or null when the
 *  record has none, which the ground is then drawn without (white). Round 11 item 11. */
function unpackColours(hex){
  if(!hex || hex.length<6) return null;
  const n=hex.length/2|0;
  const out=new Uint8Array(n);
  for(let i=0;i<n;i++) out[i]=parseInt(hex.substr(i*2,2),16);
  return out;
}

const CellData={
  models:new Map(),        // lc object id -> mesh path
  /* Round 17m: the LIGH records among them — {radius, colour, negative} by lowercased
     id, for the lamps the preview lights the cell with after dark. Only the ones that
     actually burn where they stand: the engine leaves out anything flagged OffDefault
     ("does not burn while placed in a cell"). */
  lights:new Map(),
  actors:new Map(),        // lc actor id -> {kind,model,twin,health,persistent,corpse} (twin: the x mesh, round 18ag)
  cache:new Map(),         // "x,y" -> {heights,vtex,refs,textures}
  movedIn:new Map(),       // dest "x,y" -> [{key,ref}] references moved in from elsewhere
  movedSeen:new Map(),     // ref key -> {dest,plugin} so a later move supersedes an earlier
  loadWarnings:[],         // plugins in the order that could not be read at all
  loading:false,

  clear(){ this.models.clear(); this.actors.clear(); this.lights.clear();
           this.cache.clear();
           this.movedIn.clear(); this.movedSeen.clear(); this.loadWarnings=[]; },

  /* There used to be a `loadModels` here — one pass over the whole load order
     collecting object id -> mesh — and a busy card that said "Indexing object meshes,
     first cell only — reading every plugin" while it ran. The engine builds that map
     while merging the world and hands each cell its slice with the cell, so the pass
     had already shrunk to `modelsReady = true`. What survived was the card: a progress
     dialogue in front of an assignment, telling a person to wait for work that was
     done before the window opened. Removed, card and all. */

  /** One cell, by grid position — or one interior, by name.
   *
   *  Cached: the viewport asks for the same nine over and over as settings move, and none
   *  of it changes until the install does.
   *
   *  **The key carries the kind.** An interior has no grid, so it used to arrive here as
   *  (0, 0) and be asked for with `cell_data{gx:0, gy:0}` — which answered, honestly, with
   *  whichever exterior sits at the origin. Robin: "Viewing interiors doesn't work, it
   *  just loads an exterior cell instead." Two failures in one: the wrong command, and a
   *  cache key that would have handed a room to the next caller asking for (0, 0). */
  async loadCell(target){
    const interior = target.kind==='int';
    /* Wraithguard: the review and landscape tools (48_wg_tools.js) ask for a cell with a
       mod left out, marked for a mod, or on merged ground - each its own entry here. */
    const rv=(typeof WgTools==='object')? WgTools.cellArgs() : {};
    const key = (interior? 'i:'+String(target.name||'').toLowerCase()
                         : target.x+','+target.y) + (rv.sig? '|'+rv.sig : '');
    if(this.cache.has(key)) return this.live(this.cache.get(key));
    /* Round 18bd (F3): which world this read belongs to, taken before the await.
       A cell load in flight when a different install is connected used to be filed
       anyway: `CellData.clear()` runs, the reply lands *after* it, and the outgoing
       install's heights, vtex, references and coverage were cached under `"x,y"` for the
       **new** world — and every later visit to that grid returned them until the program
       was restarted. This is the exact hazard round 18ab was written about, and the fix
       reached only the asset caches (`meshKeeper`, `keepTexture`); this one took no token
       at all. Same test, said the same way: the token must still be current, which
       refuses a read that spanned a connect, and no connect may be open, which refuses a
       read made wholly inside one. */
    const world=(typeof worldToken==='function')? worldToken() : null;
    const d = interior
      ? await Engine.call('interior_data',{name:String(target.name||''), review:rv.review||'', without:!!rv.without})
      : await Engine.call('cell_data',{gx:target.x|0, gy:target.y|0, review:rv.review||'', without:!!rv.without, land:rv.land||''});

    /* Still the world this read was asked of? Everything below files something in
       page-wide state — the LTEX table, the model, light and actor maps, the cache — and
       none of it is this cell's to give to a different install. Built and returned all
       the same, because the caller asked for this cell and this is what came back; it is
       simply not kept. Round 18bd (F3). */
    const mine=(world==null || (!App.swapping && worldToken()===world));

    /* A texture index means nothing without knowing which plugin wrote the land it
       came from, so the cell brings its own resolved table rather than an index into
       a shared one. */
    const map=new Map();
    for(const [k,v] of Object.entries(d.ltex)) map.set(+k,v);
    const landPlugin=mine? GameData.pluginLtexIx.push(map)-1 : -1;

    if(mine){
      for(const [id,mesh] of Object.entries(d.models)) this.models.set(id,mesh);
      for(const [id,li] of Object.entries(d.lights||{})) this.lights.set(id,li);
      // Corpse rules are applied where the NPDT bytes are; the page only needs to know
      // which references are actors and which of those are dead.
      for(const [id,a] of Object.entries(d.actors||{})) this.actors.set(id,a);
    }

    const rec={
      kind: interior? 'int' : 'ext', name:d.name||'', gx:target.x, gy:target.y,
      // The region, for the sky: Weather Adjuster's presets are per region (round 13).
      region: d.region||'',
      heights: d.hasLand? Float32Array.from(d.heights) : null,
      // The engine's own ground normal per vertex, signed 16-bit in hex. Unpacked here
      // rather than derived: see the note in `cellTerrain`.
      normals: d.hasLand? unpackNormals(d.normals) : null,
      colours: d.hasLand? unpackColours(d.colours) : null,
      vtex:    d.hasLand? Uint16Array.from(d.vtex) : null,
      landPlugin,
      /* A room has no place in the world, so its own coordinates are the world: its
         references are already written about its own origin, and shifting them by half a
         cell would move a room nobody can find to a grid it is not on. */
      origin: interior? [0,0] : [target.x*CELL+CELL/2, target.y*CELL+CELL/2],
      /* An exterior's sea is at zero; an interior's water is wherever its own record
         put it — WHGT, or vanilla's INTV — and absent means the room has none. The
         preview draws it to judge height limits by. */
      water: d.water==null? null : d.water,
      /* And who decided it: the last plugin to state the water flag, and the last to
         state a level — often two different plugins, because a patch that re-saves a cell
         header commonly carries no level at all. Kept so the panel can say it: when a
         room's water is somewhere nobody expects, the plugin's name is the answer. */
      waterFlagFrom: d.waterFlagFrom||'',
      waterLevelFrom: d.waterLevelFrom||'',
      /* Round 17y: a room's own light, and whether it is a room at all in the sense that
         matters to the sky. `quasi` is the CELL flag "behave like exterior" (0x80): such
         a room has the weather's sky, sun and hours; any other has only its AMBI record -
         ambient, a sunlight colour, fog colour and density - and no atmosphere to draw.
         Both null on an exterior. `dodt` on a reference is a door's destination (17y). */
      quasi: interior? !!d.quasi : null,
      ambi: (interior && d.ambi)? d.ambi : null,
      /* `from` is the index of the plugin that supplied the reference, in the same load
         order the plugin list is in. Carried per reference so that right-clicking an
         object can say who put it there — which is the question the old "Objects by
         plugin" totals in the left column were a poor answer to. */
      /* `key` names the reference the way the mask does, so the object under the pointer
         can be asked how much paint it carries. Carried rather than derived: the page has
         no way to work out a RefNum, and inventing one would be a second way of naming a
         reference. */
      refs: d.refs.map(r=>({id:r.id, pos:r.pos, rot:r.rot, scale:r.scale,
                            key:r.key||'',
                            from:(r.from==null? -1 : r.from|0),
                            // Round 17y: where a door leads - {pos, rot, cell} - or absent.
                            door:r.door||null,
                            // Wraithguard: grass from a `groundcover=` plugin.
                            gc:!!r.gc,
                            // Wraithguard: its review mark, every plugin that supplied a
                            // version, and how far the load order moved it.
                            mark:r.mark||'', hist:r.hist||null, moved:+r.moved||0})),
      // Wraithguard: who edits this cell's ground, and the merged-lands answer.
      landEditors: d.landEditors||[], landMerged: d.landMerged||null, reviewed:!!d.reviewed,
      // The alpha maps, built against every landscape in the load order — including
      // the neighbours this cell blends into, which the page could never see.
      layers: d.hasLand? (d.layers||[]).map(unpackLayer) : null,
      /* Who supplied what, and what the rest of the order did to this cell on the
         way. `sources` arrives as an object because it crosses as JSON; the panel
         wants a Map, and converting it here keeps the panel from knowing about the
         wire. */
      prov: d.prov? {sources:new Map(Object.entries(d.prov.sources||{})),
                     replaced:d.prov.replaced|0, deleted:d.prov.deleted|0,
                     movedOut:d.prov.movedOut|0, movedIn:d.prov.movedIn|0,
                     unresolved:0} : null,
    };
    if(mine) this.cache.set(key,rec);
    return this.live(rec);
  },

  /** Wraithguard: a cell as the editor's pending reference changes would leave it -
   *  moved, turned, scaled or deleted objects (50_wg_editor.js). The cached record is
   *  kept as the load order has it; this is a copy, made only when a change touches it. */
  live(rec){
    return (typeof WgEditor==='object' && rec)? WgEditor.overlay(rec) : rec;
  },

  /** Ground height at a cell-local point, on the triangle the game draws.
   *
   *  Only the camera asks — everything about placement is the engine's — but it asks the
   *  same question, so it has to get the same answer. Morrowind cuts every land quad from
   *  (x+1, y) to (x, y+1); this was a bilinear tap until the surface was measured, and a
   *  bilinear tap is out by up to 320 units on real terrain (§11a). A camera aimed at a
   *  surface nothing else uses is a small bug, but it is also how a second surface gets
   *  back in.
   */
  heightAt(cell,lx,ly){
    if(!cell.heights) return 0;
    const s=8192/(LAND_VERTS-1), N=LAND_VERTS, H=cell.heights;
    const fx=clamp(lx/s,0,N-1.001), fy=clamp(ly/s,0,N-1.001);
    const x0=Math.floor(fx), y0=Math.floor(fy), tx=fx-x0, ty=fy-y0;
    const h=(x,y)=>H[y*N+x];
    return tx+ty<=1
      ? h(x0,y0)*(1-tx-ty) + h(x0+1,y0)*tx + h(x0,y0+1)*ty
      : h(x0+1,y0+1)*(tx+ty-1) + h(x0,y0+1)*(1-tx) + h(x0+1,y0)*(1-ty);
  },
  /* `normalAt` and `textureAt` used to sit here. Both were the page's own answer to a
     question the engine answers — the ground normal a blade stands on arrives with the
     placement (§17a 248e), and which texture covers a point is decided by
     `gf_core::coverage`, whose ring the page cannot see (§17c 248aa). Neither had a
     caller left. `heightAt` stays because the camera has to aim at the ground before
     any of that has been asked for. */
};
