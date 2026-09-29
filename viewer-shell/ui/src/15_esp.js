/* =====================================================================================
   What the install contains, as the engine reported it.

   Textures, regions, named cells, the grid, where each texture is actually painted,
   the plugin order. All of it comes over when an install is opened — the engine merged
   the load order to build the world and had the answers already.

   This used to be a TES3 record scanner: a blob reader with a sliding window, master
   index resolution, a landscape pass. It could not survive a real load order — pulling
   583 MB through a webview is not something to optimise — and more to the point it was
   a second implementation of merge rules that only the side holding the whole world
   can get right. `crates/gf_core/tests/merge.rs` is where those rules are pinned now.

   What is left is lookup: does this name exist, what is it really spelt like, where is
   it painted. Filled by `openInstall`, read by the pickers.
   ===================================================================================== */

const GameData={
  ltex:new Map(),      // lc id -> {id,index,file,from}
  regn:new Map(),      // lc id -> {id,name,from}
  cells:new Map(),     // lc name -> {name,from}   (named exteriors, for the Scope field)
  // every accepted spelling for the Scope field -> its exact-case canonical form.
  // A region is reachable by its NAME (id) and by its FNAM (display name); which of
  // the two the generator matches is not documented, so both are treated as valid.
  scopeIx:new Map(),
  pluginLtexIx:[],     // per plugin: Map(INTV -> LTEX id). Indices are PLUGIN-LOCAL in
                       // Morrowind, so a VTEX value only means anything alongside the
                       // plugin index of the LAND record that used it.
  pluginMasters:[],    // per plugin: the MAST names from its TES3 header, in order
  masterIx:[],         // per plugin: [load-order index of master 1, of master 2, ...]
  _found:[],           // plugin filenames actually opened, in load order. Index into
                       // this — not into the requested order — is what every per-plugin
                       // array above is keyed by.
  grid:new Map(),      // "x,y" -> {name,region}
  /* Interior cells: `[{name, refs}]`, as the engine set them aside. Not in `grid`, and
     they cannot be — that map is keyed by grid position and an interior has none. */
  interiors:[],
  usage:new Map(),     // lc texture id -> {cells:Set, regions:Set}
  landScanned:false, landCells:0, landUnresolved:0,
  plugins:[],          // resolved order actually scanned
  missing:[],          // listed but not found through the overlay
  scanning:false, scanned:false,
  /* Which world this is. Everything cached against a cell — the built scene, a loaded
     landscape — is only valid for the install it was read from, and a preview already
     in flight when a different one is connected has to be able to tell that it is now
     answering about a world nobody is looking at. Bumped by `openInstall`, never
     reused. */
  sig:'', _gen:0,
  orderSource:'',

  /** A new world is starting. Every rebuild in flight is now working for a world that
      is going away, and this is how they find out — they compare `sig` after each await
      and stop when it has moved.

      Separate from `clear()` because *when* it happens matters. The engine's world is
      replaced the moment `open_install` is called, several round trips before the page
      has anything to clear, and a rebuild that keeps going through that window asks the
      new world about a cell only the old one had. So this is called first, on its own,
      and `clear()` does not move the signature again — one connect, one world, one
      bump. Two bumps is not a harmless belt and braces: the second lands in the middle
      of whatever the first let through. */
  newWorld(){ this.sig=String(++this._gen); },

  clear(){ this.ltex.clear(); this.regn.clear(); this.cells.clear();
           this.scopeIx.clear();
           this.pluginLtexIx=[]; this.pluginMasters=[]; this.masterIx=[]; this._found=[];
           this.grid.clear(); this.interiors=[]; this.usage.clear();
           this.landScanned=false; this.landCells=0; this.landUnresolved=0;
           this.plugins=[]; this.missing=[]; this.scanned=false; this.orderSource=''; },
  _addScope(text,kind,from){
    const t=String(text||'').trim(); if(!t) return;
    this.scopeIx.set(t.toLowerCase(),{canon:t,kind,from});
  },

  /** The masters plugin `i` names in its header, in order; none when unknown. */
  masterNames(i){ return this.pluginMasters[i]||[]; },

  /** Whether a plugin of that name is in the resolved load order. Case-insensitive:
      a load order is a list of filenames and Windows does not care. */
  knowsPlugin(name){
    const k=String(name||'').trim().toLowerCase();
    return this.plugins.some(p=>p.name.toLowerCase()===k);
  },

  /** LTEX DATA gives the real texture file for an ini section name */
  textureFileFor(id){
    const key=String(id||'').trim().toLowerCase();
    const r=this.ltex.get(key);
    if(r&&r.file) return r.file;
    // The default has no LTEX record to read a filename out of.
    if(key===DEFAULT_LTEX) return DEFAULT_LTEX_FILE;
    return null;
  },
  /* Case does not come into it.
   *
   * These used to look a name up case-insensitively and then compare the case strictly,
   * because the generator matched exactly and `tx_bc_grass` really did place nothing
   * where `Tx_BC_grass` placed grass — Groundcover Generator's behaviour, which this tool
   * inherited along with the file format. `selectors_for` ignores case now, so a name in
   * another case is a rule that works, and saying otherwise here would put a red tag on a
   * rule that is placing grass. `suggestTexture`/`suggestScope` went with it: there is
   * nothing left to suggest.
   *
   * The install's own spelling is still what a picker inserts, and `canonTexture` below
   * is how anything that wants to *display* the canonical form gets it — but nothing is
   * rewritten behind your back any more. */
  knowsTexture(id){
    const v=String(id||'').trim();
    if(v.toLowerCase()===String(DEFAULT_LTEX).toLowerCase()) return true;
    return this.ltex.has(v.toLowerCase());
  },
  /** The install's own capitalisation of a name it has, for showing beside yours. */
  canonTexture(id){
    const r=this.ltex.get(String(id||'').trim().toLowerCase());
    return (r && r.id!==String(id||'').trim())? r.id : null;
  },
  knowsScope(v){
    return this.scopeIx.has(String(v||'').trim().toLowerCase());
  },
  canonScope(v){
    const t=String(v||'').trim();
    const r=this.scopeIx.get(t.toLowerCase());
    return (r && r.canon!==t)? r.canon : null;
  },
  /** Every pickable cell: named and unnamed exteriors. Interiors are not in the
      world at all — see `cellList` below. */
  /** Every pickable place. `withInteriors` adds the rooms.
   *
   *  Exteriors are the world: keyed by grid position, carrying a landscape, and the only
   *  thing anything can be scattered on today. Interiors have no grid and no landscape —
   *  the engine sets them aside by name rather than dropping them (`World.interiors`) so
   *  that the picker can offer them at all. Picking one shows nothing yet and says so;
   *  that is the honest first step rather than a tab that is always empty. */
  cellList(withInteriors){
    const out=[];
    for(const [k,v] of this.grid){
      const [x,y]=k.split(',').map(Number);
      out.push({kind:'ext', x, y, name:v.name||'', region:v.region||'',
                label: v.name || ('('+x+', '+y+')'),
                sub: v.name? ('('+x+', '+y+')'+(v.region?' · '+v.region:''))
                           : (v.region||'wilderness')});
    }
    /* Named cells first, alphabetically; then the wilderness by **grid**, not by the text
       of its label — round 18bf (§I20). `localeCompare` on "(10, 0)" and "(2, 0)"
       puts the ten first, so a column of unnamed cells read 0, 1, 10, 11, 2, 20 … which is
       an ordering nobody can scan for the cell they want. Sorted north-up, west to east,
       the way a map is read. */
    out.sort((a,b)=> (a.name?0:1)-(b.name?0:1)
                  || (a.name? a.label.localeCompare(b.label) : (a.y-b.y || a.x-b.x)));
    if(withInteriors){
      const ints=(this.interiors||[]).map(c=>({
        kind:'int', x:0, y:0, name:c.name, region:'',
        label:c.name,
        sub:'interior'+(c.refs? ' · '+c.refs+' object'+(c.refs===1?'':'s') : '')}));
      ints.sort((a,b)=>a.label.localeCompare(b.label));
      out.push(...ints);
    }
    return out;
  },

  /** cells and regions this texture is actually painted in, or null if unknown */
  usageFor(id){
    const u=this.usage.get(String(id||'').trim().toLowerCase());
    if(!u) return null;
    return {cells:Array.from(u.cells).sort((a,b)=>a.localeCompare(b)),
            regions:Array.from(u.regions).sort((a,b)=>a.localeCompare(b))};
  },
  scopeKind(v){
    const r=this.scopeIx.get(String(v||'').trim().toLowerCase());
    return r? r.kind : null;
  },
  textureNames(){ return Array.from(this.ltex.values()).map(r=>r.id).sort((a,b)=>a.localeCompare(b)); },
  scopeNames(){
    const rs=[],cs=[];
    for(const r of this.scopeIx.values()) (r.kind==='region'?rs:cs).push(r.canon);
    const uniq=a=>Array.from(new Set(a)).sort((x,y)=>x.localeCompare(y));
    return {regions:uniq(rs), cells:uniq(cs)};
  },

  /** resolve a VTEX value against the plugin that wrote it (indices are plugin-local) */
  _ltexForVtex(v,pluginIdx){
    // 0 is not "unpainted, draw nothing" — it is the engine's own land texture, and
    // large stretches of vanilla landscape use it (the bed of the Odai in Balmora is
    // one). It has no LTEX record, so it gets a reserved id instead.
    if(!v) return DEFAULT_LTEX;
    const ix=v-1;
    const own=this.pluginLtexIx[pluginIdx];
    if(own && own.has(ix)) return own.get(ix);
    // the plugin edited land without shipping its own LTEX: fall back down the order
    for(let i=pluginIdx-1;i>=0;i--){
      const m=this.pluginLtexIx[i];
      if(m && m.has(ix)) return m.get(ix);
    }
    return null;
  },

};
