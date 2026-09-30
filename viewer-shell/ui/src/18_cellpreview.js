
/* =====================================================================================
   Cell preview: real terrain, real texture mask, placed statics
   ===================================================================================== */

/** One terrain mesh for the cell, plus a coverage map per land texture.
    Layers are ordered by how much of the cell they cover, so the most common
    texture is the opaque base and the rest blend over it.

    `neighbour(dx,dy)` returns the adjacent loaded cell, or null. The coverage maps
    are one texel larger on every side than the cell's own 16x16 sub-cell grid, and
    that ring is filled from the neighbours. Without it a map clamps at its edge and
    two adjoining cells each hold their border texture constant across the last half
    texel, which is exactly the hard straight line that shows up at a cell join. */
/** The name the engine uses for the corpse stand-in. It has no file: an NPC is
    assembled from body parts at runtime, so the engine builds a body-shaped slab and
    serves it under this name — the same geometry it keeps grass out of. */
const CORPSE_SLAB='__corpse_slab';
/** Wraithguard: the mesh path the engine assembles an NPC for (viewcore npc.rs NPC_PREFIX),
 *  followed by the NPC's id. */
const NPC_MESH_PREFIX='__npc/';
/** Wraithguard: night, for the NPCs' torches - OpenMW's NPCs take one out after dark. */
function npcNightHour(){ const h=+App.hour; return isFinite(h) && (h>=20 || h<6); }
/** How the engine is asked to dress an NPC (viewcore npc.rs `npc_request`): a torch at
 *  night out of doors, the weapon in the hand when the Preview says so. */
function npcFlags(indoors){
  const f=[];
  if(npcNightHour() && !indoors) f.push('night');
  if(App.npcDrawn) f.push('drawn');
  return f.length? '?'+f.join('&') : '';
}   // forward slash: VFS.norm turns every backslash into one

/* The one living creature the preview draws: the silt strider, by its vanilla model path
   or the `x` twin the game actually draws; the folder is not part of the match, so a mod
   shipping it elsewhere is still found. */
const STRIDER_MESH=/(^|[\\/])x?siltstrider\.nif$/i;

/** How much of a cell is meshed, in units, given the extent asked for.
 *
 *  Returns the inclusive vertex range covering a centred square of `extent` units, or
 *  the whole cell when nothing was asked for. Snapped outwards to whole vertices — the
 *  ground is a triangle mesh and half a triangle is not a thing to draw.
 */
function terrainSpan(extent){
  const N=LAND_VERTS, step=8192/(N-1);
  if(!(extent>0) || extent>=8192) return [0,N-1];
  const halfV=Math.ceil((extent/2)/step);
  const mid=(N-1)/2;
  return [Math.max(0,Math.floor(mid-halfV)), Math.min(N-1,Math.ceil(mid+halfV))];
}

function cellTerrain(cell,neighbour,extent){
  const N=LAND_VERTS, S=LAND_SUBDIV;
  const step=8192/(N-1), half=4096;
  const H=cell.heights;
  const hAt=(x,y)=> H? H[y*N+x] : 0;
  const [v0,v1]=terrainSpan(extent);
  const pos=new Float32Array(N*N*3), nrm=new Float32Array(N*N*3), uv=new Float32Array(N*N*2);
  /* The ground normal at every vertex, as the engine resolved it — from the record's
     VNML where there is one, from the triangle where there is not. Not computed here:
     which normal a point gets is a rule, and the page holding its own version of it is
     how the viewport came to shade one surface while the scatter used another. The
     stream also drives the red slope overlay, so this is what makes the overlay show
     the cull rather than approximate it. See §11a. */
  const EN=cell.normals;
  /* And the record's vertex colours, VCLR, which the game multiplies the ground texture
     by — white where the record has none. Round 11 item 11: the shadow under a cliff and
     the brown of a worn path are mostly these, not the textures. */
  const EC=cell.colours;
  const col=new Uint8Array(N*N*3);
  if(EC && EC.length>=N*N*3) col.set(EC.subarray(0,N*N*3)); else col.fill(255);
  for(let y=0;y<N;y++)for(let x=0;x<N;x++){
    const k=y*N+x;
    pos[k*3]=-half+x*step; pos[k*3+1]=-half+y*step; pos[k*3+2]=hAt(x,y);
    if(EN){ nrm[k*3]=EN[k*3]; nrm[k*3+1]=EN[k*3+1]; nrm[k*3+2]=EN[k*3+2]; }
    else { nrm[k*3]=0; nrm[k*3+1]=0; nrm[k*3+2]=1; }
    uv[k*2]=x/(N-1); uv[k*2+1]=y/(N-1);
  }
  /* Morrowind cuts every land quad from (x+1, y) to (x, y+1). This used to be the other
     diagonal, which made the drawn ground disagree with the game's by a median of 1.33
     and a maximum of 640 units — worse than the engine's old bilinear sampler, and in
     the opposite direction, so the two were furthest apart exactly where the terrain was
     most dramatic. Measured against 152,911 real placements; see §11a. */
  /* Only the quads inside the span are meshed. Every vertex keeps its own index and its
     own UV, so the coverage maps still line up with the cell they came from — a mesh
     that renumbered its vertices would have to renumber the texture coordinates with
     them, and the alpha maps are the engine's. */
  const q=Math.max(0,v1-v0);
  const idx=new Uint32Array(q*q*6);
  let t=0;
  for(let y=v0;y<v1;y++)for(let x=v0;x<v1;x++){
    const a=y*N+x, b=a+1, c=a+N, d=c+1;   // a=(x,y) b=(x+1,y) c=(x,y+1) d=(x+1,y+1)
    idx[t++]=a; idx[t++]=b; idx[t++]=c;   // below the cut
    idx[t++]=b; idx[t++]=d; idx[t++]=c;   // above it
  }

  /* What was actually drawn, so the water sheet and the blade filter can be the same
     size as the ground rather than a guess at it. */
  const span=[-half+v0*step, -half+v1*step];
  return {mesh:{pos,nrm,uv,col,idx}, layers:CellLayers.of(cell,neighbour),
          inner:S, total:S+2, span};
}

/** The ground height inside one cell, in that cell's own coordinates (0…8192).
 *
 *  The same surface `cellTerrain` meshes, evaluated instead of triangulated — Morrowind
 *  cuts every quad from (x+1, y) to (x, y+1), so a point is in the lower triangle when
 *  `tx + ty <= 1`. Kept immediately beside the mesh builder deliberately: they are two
 *  expressions of one rule, and the whole of §19c is what happens when two expressions of
 *  the surface drift apart. If the index buffer above ever changes, this changes with it.
 *
 *  Used for the brush, and for nothing that decides where grass goes — that is the
 *  engine's, and it reads the same rule in `land.rs`.
 */
function landHeightAt(cell,lx,ly){
  const N=LAND_VERTS, step=8192/(N-1), H=cell&&cell.heights;
  if(!H) return null;
  const fx=Math.min(Math.max(lx/step,0),N-1.001), fy=Math.min(Math.max(ly/step,0),N-1.001);
  const x0=Math.floor(fx), y0=Math.floor(fy), tx=fx-x0, ty=fy-y0;
  const h=(x,y)=>H[y*N+x];
  return tx+ty<=1
    ? h(x0,y0)*(1-tx-ty) + h(x0+1,y0)*tx + h(x0,y0+1)*ty
    : h(x0+1,y0+1)*(tx+ty-1) + h(x0,y0+1)*(1-tx) + h(x0+1,y0)*(1-ty);
}

/** The ground under a point in *viewport* coordinates, or null off the loaded cells.
 *
 *  The viewport draws relative to the centre of whichever cell the scene was built
 *  around, so this is where the two coordinate systems meet: add the origin to get the
 *  world, work out which cell that is, and ask it.
 */
function sceneGroundZ(vx,vy){
  const sc=App._scene;
  if(!sc || !sc.origin) return null;
  const wx=vx+sc.origin[0], wy=vy+sc.origin[1];
  const gx=Math.floor(wx/8192), gy=Math.floor(wy/8192);
  const c=(sc.cells||[]).find(c=>c.kind!=='int' && c.gx===gx && c.gy===gy);
  if(!c) return null;
  return landHeightAt(c, wx-gx*8192, wy-gy*8192);
}



/* ---- Glow in the Dahrk (round 17x) --------------------------------------------------
   The mod's meshes carry a NiSwitchNode called NightDaySwitch — the window by day, lit at
   night, and for a room with the sun coming through — and its MWSE add-on picks the branch
   by the clock. Its rule, from its main.lua: outdoors the window is lit when

       hour < sunriseStart  or  hour > sunsetStop

   where sunriseStart = Sunrise Time - Sun Pre-Sunrise Time and sunsetStop = Sunset Time +
   Sunset Duration + Sun Post-Sunset Time (the weather controller's numbers, which are the
   game's [Weather] ini values): on the vanilla ini that is after 21:15 and before 06:00.
   With its "variance" option on, the hour each window reads is shifted by
   sin(x*1.35 + y) * variance/60, from the reference's own world position, so a street
   lights up unevenly. The option is off by default; the mod's saved settings are read when
   they are there (MWSE/config/Glow in the Dahrk.json), so the preview agrees with the game
   whichever way it is set.

   Expressed as the two hours the renderer's lamp stream already carries — the hour it
   lights and the hour it goes out — so the switch, the glow map and the emissive all read
   one instance value. */
function windowHours(wx, wy){
  const T=(typeof Sky==='object' && Sky.timing)? Sky.timing() : null;
  const sunrise=T? T.sunrise : 6, sunset=T? T.sunset : 18, sunsetDur=T? T.sunsetDuration : 2;
  const pre=T? T.sun[0] : 0, post=T? T.sun[3] : 1.25;
  const sunriseStart=sunrise-pre;
  const sunsetStop=sunset+sunsetDur+post;
  const g=App.gitd||null;
  let v=0;
  if(g && g.useVariance){
    const minutes=(g.varianceInMinutes!=null)? +g.varianceInMinutes : 30;
    v=Math.sin(wx*1.35 + wy)*(minutes/60);
  }
  // hour+v > sunsetStop  <=>  hour > sunsetStop-v ; hour+v < sunriseStart <=> hour < sunriseStart-v
  return [sunsetStop - v, sunriseStart - v];
}

/** The mod's saved settings, once per session; null when it is not installed or has none. */
async function loadGitdConfig(){
  if(App.gitd!==undefined) return App.gitd;
  App.gitd=null;
  try{
    const buf=await Engine.bytes('read_asset',{path:'MWSE\\config\\Glow in the Dahrk.json'});
    const txt=new TextDecoder().decode(new Uint8Array(buf));
    const j=JSON.parse(txt);
    if(j && typeof j==='object') App.gitd={useVariance:!!j.useVariance,
                                          varianceInMinutes:j.varianceInMinutes!=null? +j.varianceInMinutes : 30};
  }catch(_){ }
  return App.gitd;
}









































































































/** Where the alpha maps come from. One seam, the same shape as CellScatter's.

    The mesh above is pure arithmetic on the height grid and stays here: it is
    derivable from what the page already holds, and shipping 84 KB of vertices per
    cell to avoid a loop nobody disputes would be a bad trade. The alpha maps are
    the opposite — a few hundred bytes carrying a rule the two sides can disagree
    about — so under Tauri they come from the engine. See `nativeCellLayers`. */
/** The ground's alpha maps, as the engine built them.
 *
 *  One 18x18 mask per land texture: the cell's own 16 sub-cells plus a ring taken from
 *  the *neighbouring* landscape, so two adjoining cells blend into each other instead
 *  of meeting on a hard line. The page could only honour that for cells it happened to
 *  have open, and clamped at the edge of whatever it held; the engine holds every
 *  landscape in the load order, so its ring is always the real neighbour.
 *
 *  Delivered with the cell — there is nothing to compute here any more, only somewhere
 *  to look. */
const CellLayers={
  of(cell){ return cell.layers||[]; },
};

/** Morrowind reference rotation: the three angles negated, composed X * Y * Z.
    That is, Z is applied first and X last — the order the engine uses.

    Order only shows when more than one angle is non-zero, which is why almost
    everything looked right: buildings and trees are placed with yaw alone. Rocks
    are tumbled about all three axes, and those came out at visibly wrong angles. */
function refMatrix(pos,rot,scale){
  const cx=Math.cos(-rot[0]), sx=Math.sin(-rot[0]);
  const cy=Math.cos(-rot[1]), sy=Math.sin(-rot[1]);
  const cz=Math.cos(-rot[2]), sz=Math.sin(-rot[2]);
  const m00=cy*cz,                m01=-cy*sz,               m02=sy;
  const m10=cx*sz + sx*sy*cz,     m11=cx*cz - sx*sy*sz,     m12=-sx*cy;
  const m20=sx*sz - cx*sy*cz,     m21=sx*cz + cx*sy*sz,     m22=cx*cy;
  const s=scale||1;
  return [m00*s,m01*s,m02*s,pos[0],
          m10*s,m11*s,m12*s,pos[1],
          m20*s,m21*s,m22*s,pos[2]];
}

/** Is this LIGH reference a fire — something that burns by day as by night?
 *
 *  Round 17y, Robin: "Campfires (like Furn_De_Firepit_F_400) and fireplaces should always
 *  be on (both particle system and light)." The record has no flag for it: the LIGH Fire
 *  bit (0x010) is set on the firepits *and* on every lantern, candle and chandelier in
 *  Morrowind.esm (574 records surveyed - 168 are 0x051, 100 are 0x053), so the name is
 *  the only thing that tells a hearth from a lamp. The id and the mesh path are both
 *  read: `Light_Fire` names a fire in its id; a mod's `light_hearth_01` in its mesh. */
const FIRE_LIGHT_RE=/fire|pit|brazier|logpile|hearth|campfire|bonfire|forge|kiln|furnace|smelter|ember|coal|cauldron|stove|oven/i;
function isFireLight(id, model){
  return FIRE_LIGHT_RE.test(String(id||'')) || FIRE_LIGHT_RE.test(String(model||''));
}

/** The room on screen, when the scene is one that is *not* flagged to behave like an
 *  exterior — the case with no sky, where lamps keep no hours and there is no
 *  atmosphere to draw (round 17y). Null for an exterior or a quasi-exterior room. */
function roomIndoors(cells){
  const room=(cells||[]).find(c=>c && c.kind==='int');
  return !!(room && !room.quasi);
}

/** A short, stable hash of a string — FNV-1a, 32-bit — for signatures that would
 *  otherwise carry the whole config text (round 17y). */
function hashStr(str){
  let h=0x811c9dc5;
  for(let i=0;i<str.length;i++){ h^=str.charCodeAt(i); h=Math.imul(h,0x01000193); }
  return (h>>>0).toString(16);
}

/** Where a door's DODT/DNAM leads, as a picker target — `{kind:'int', name, label}` for a
 *  named interior, `{kind:'ext', x, y, name, label}` for a position in the world — or null
 *  for a reference that is no teleporting door (round 17y). */
/** How much of a cell the camera takes in when it is framed on a door's marker (round 18a):
 *  a doorway and what is around it, at the distance frameAt's 1.55 makes of this. */
const DOOR_FRAME_SPAN=560;
/** And where the eye stands about it (round 18d, turned in 18f): 45 degrees off the way
 *  the player arrives facing, ahead of the marker and to one side, so it looks back past
 *  the marker at the door, and a little above. `az` is the eye's bearing from the pivot,
 *  so this is added to the facing's bearing. Robin, of 18d's 135: "The rotation of the
 *  camera is wrong [...] I want EITHER a result placed 180 degrees, or 90 degrees,
 *  rotation from where" - 135 is behind the marker, where the door and its house are, and
 *  180 from it is here. */
const DOOR_EYE_TURN=-45*Math.PI/180;
const DOOR_EYE_EL=0.40;
/** Round 18f: the eye stepped out of whatever it landed in. The nearest drawn surface, or
 *  the ground, on the line from the marker to where the eye would stand caps how far out
 *  it goes: this much short of the surface, and never nearer the marker than the floor. */
const DOOR_EYE_CLEAR=48;
const DOOR_EYE_MIN=96;
/** The camera a door framed (`App._doorEye`), pulled in to clear whatever stands between
 *  the marker and where the eye would be: the nearest drawn surface on that line, by the
 *  same triangles the pointer picks, and the ground, marched the way `groundAt` marches
 *  it. The pivot stays on the marker, so the eye still looks at it from wherever it ends
 *  up. Nothing is done when the camera is no longer the one the door set (the person
 *  moved it while the cell loaded), or when nothing is in the way. Returns the distance
 *  it settled on, for the tests. */
function settleDoorEye(R,groundAt){
  const d=App._doorEye; App._doorEye=null;
  if(!d || !R || !R.cam) return null;
  const c=R.cam;
  if(c.az!==d.az || c.el!==d.el || c.dist!==d.dist || c.tx!==d.pivot[0] || c.ty!==d.pivot[1] || c.tz!==d.pivot[2]) return null;
  const eye=R.cameraEye();
  const dir=[eye[0]-c.tx, eye[1]-c.ty, eye[2]-c.tz];
  const L=Math.hypot(dir[0],dir[1],dir[2]); if(!(L>1)) return null;
  for(let k=0;k<3;k++) dir[k]/=L;
  let t=c.dist;
  /* From a step out along the line rather than from the marker itself: the marker is on
     the floor, and a ray that starts on a surface finds that surface at nought. */
  const from=[c.tx+dir[0]*DOOR_EYE_CLEAR, c.ty+dir[1]*DOOR_EYE_CLEAR, c.tz+dir[2]*DOOR_EYE_CLEAR];
  const hit=(typeof R.pickRay==='function')? R.pickRay(from,dir) : null;
  if(hit && hit.t+DOOR_EYE_CLEAR<t) t=hit.t+DOOR_EYE_CLEAR;
  /* `groundAt` is the ground of the scene being built - not `R.groundZ`, which until the
     scene is installed still answers for the one on screen. */
  if(typeof groundAt==='function'){
    const step=16;
    for(let s=DOOR_EYE_CLEAR; s<t; s+=step){
      const x=c.tx+dir[0]*s, y=c.ty+dir[1]*s, z=c.tz+dir[2]*s;
      const g=groundAt(x,y);
      if(g!=null && z<g){ t=s; break; }
    }
  }
  if(t<c.dist){ c.dist=Math.max(DOOR_EYE_MIN, t-DOOR_EYE_CLEAR); R.dirty=true; }
  return c.dist;
}
function doorTarget(door){
  if(!door || !door.pos) return null;
  /* The name as the door writes it: the engine keys interiors by their exact name (any
     case, as the game matches), so trimming it here could miss a cell whose NAME carries
     a trailing space. Trimmed only to tell "no name" (a door to the exterior) apart. */
  const raw=String(door.cell||'');
  const cell=raw.trim();
  if(cell) return {kind:'int', name:raw, label:cell};
  const x=Math.floor((+door.pos[0]||0)/CELL), y=Math.floor((+door.pos[1]||0)/CELL);
  const named=(typeof GameData==='object' && GameData.grid)? GameData.grid.get(x+','+y) : null;
  return {kind:'ext', x, y, name:named? named.name : '',
          label:(named&&named.name? named.name+'  ' : '')+'('+x+', '+y+')'};
}

/* `alignMatrix` and `refAngles` used to live here: the page's own way of turning a
   ground normal into the three angles a reference record stores. They were two of four
   implementations of the same tilt, and the only two nothing called. The engine's
   `poisson::align_matrix`/`ref_angles` writes the plugin and `VS_GRASS` in 06_gl.js
   draws the preview; those two ship, and a third copy sitting unused beside them is how
   a rule change reaches one and not the others. §19a 275d. */

/** The middle of a room, in x and y, taken from what is in it.
 *
 *  A room has no grid position and no landscape, so the only thing that says where it is
 *  is where its contents are. Used for framing and for centring the water sheet. */
function interiorCentre(cell){
  let x0=Infinity,y0=Infinity,x1=-Infinity,y1=-Infinity;
  for(const r of (cell.refs||[])){
    x0=Math.min(x0,r.pos[0]); x1=Math.max(x1,r.pos[0]);
    y0=Math.min(y0,r.pos[1]); y1=Math.max(y1,r.pos[1]);
  }
  return x1>=x0? [(x0+x1)/2,(y0+y1)/2] : [0,0];
}

/** Every rule that governs this texture in this cell, in file order.

    Scope decides *which* rules apply — exact cell beats any-named-cell beats
    region beats the plain texture — and only the narrowest tier runs, so a rule
    written for one cell still overrides the region-wide one.

    Among rules that tie, all of them run. A config is assembled by importing one
    community .ini after another, and two files both having something to say about
    the same texture is the normal case, not a mistake: each rule scatters its own
    pass with its own spacing, and the ground carries both. Mirrors
    `gf_core::poisson::selectors_for`. */
function selectorsForTexture(cfg,texture,cellName,region){
  const t=String(texture||'').toLowerCase();
  const all=pred=>cfg.selectors.filter(s=>s.texture.toLowerCase()===t && pred(s));
  const cn=String(cellName||'').toLowerCase(), rg=String(region||'').toLowerCase();
  let hit=all(s=>s.qualKind==='name' && (s.qualValue||'').toLowerCase()===cn && cn!=='');
  if(!hit.length && cellName) hit=all(s=>s.qualKind==='named');
  if(!hit.length) hit=all(s=>s.qualKind==='name' && (s.qualValue||'').toLowerCase()===rg && rg!=='');
  if(!hit.length) hit=all(s=>s.qualKind==='none');
  return hit;
}



/** world-space bounds of a mesh AABB under a 3x4 reference matrix */
/** The name of the plugin at a load-order index, or '' when there is not one.
 *
 *  References carry the index rather than the name because a cell can hold thousands of
 *  them and the name would be repeated on every one. */
function pluginName(ix){
  if(ix==null || ix<0) return '';
  const p=(typeof GameData==='object' && GameData.plugins)? GameData.plugins[ix] : null;
  return p? (p.name||String(p)) : '';
}

function worldAabb(a,m){
  let x0=Infinity,y0=Infinity,z0=Infinity,x1=-Infinity,y1=-Infinity,z1=-Infinity;
  for(const cx of [a.x0,a.x1]) for(const cy of [a.y0,a.y1]) for(const cz of [a.z0,a.z1]){
    const X=m[0]*cx+m[1]*cy+m[2]*cz+m[3];
    const Y=m[4]*cx+m[5]*cy+m[6]*cz+m[7];
    const Z=m[8]*cx+m[9]*cy+m[10]*cz+m[11];
    if(X<x0)x0=X; if(X>x1)x1=X;
    if(Y<y0)y0=Y; if(Y>y1)y1=Y;
    if(Z<z0)z0=Z; if(Z>z1)z1=Z;
  }
  return {x0,y0,z0,x1,y1,z1};
}

/** world point -> object space, for a matrix that is rotation x uniform scale */
/* ---------------- build the whole cell preview ---------------- */
function cellOffsetFrom(origin,cell){
  return [cell.origin[0]-origin[0], cell.origin[1]-origin[1]];
}





/** Round 18n: a rescatter overtaken by a newer ask. Thrown by `CellScatter.cell` when
 *  the engine says so, and caught by `scatterIntoScene`, which then simply stops - the
 *  newer ask is already on its way and will put the right grass up. */
class SupersededError extends Error {
  constructor(){ super('superseded'); this.superseded=true; }
}

const CellScatter = {
  /** Scatter every enabled texture in one cell. Returns its groups and summed stats. */
  /** A cell's generated grass - none: the viewer displays grass (groundcover plugins,
      see addGroundcoverRuns), it does not generate it. */
  async cell(){ return {groups:[], stats:{}}; },

};

/** The room's grass: what the mesh rules grow on its painted statics.
 *
 *  The same shape of answer as `CellScatter.cell` — the same byte payload, the same
 *  groups — because it is the same scatter the export runs, pointed at a room. There is
 *  no landscape indoors, so there is nothing else for grass to stand on. */
CellScatter.interior=async function(){ return {groups:[], stats:{}}; };

/** The object walk's own account of itself, under the names the panel sums. */
const CENSUS_KEYS=['objects','unresolved','markerFiltered','aliveSkipped','corpses',
  'npcMarkers','meshMissing'];

/* One rebuild at a time.
   -----------------------
   Everything below assumes it is the only thing touching the renderer, and for a long
   while that was true by accident: a second rebuild starting while the first was still
   going would stall on the first frame it waited for, and the first would win. Once that
   stall was fixed the two started genuinely overlapping, and the results were exactly
   what you would expect from two functions writing the same fields — an object walk that
   ran while the other run's mesh was still being fetched installed an empty set of
   statics over a full one, at random, about one time in three.

   Serialised rather than made re-entrant. A rebuild is a dozen round trips with the
   renderer's contents as its working state; making that safe to interleave would mean
   giving every step its own copy of the scene, to no purpose — nobody wants two previews
   at once. Callers still get a promise that resolves when *their* rebuild has run, which
   is the only thing any of them wanted.

   Failures are logged and swallowed here so that one bad rebuild cannot poison the queue
   for every rebuild after it. */
let _cellQueue=Promise.resolve();
/* Round 18n: the number of the ask whose run is on the cell queue now. Read by
   `scatterIntoScene`, which used to read `App.previewAsk` at its own start - the newest
   number, not the run's own - so a run overtaken during its load scattered under the
   overtaker's number, was not refused, and the overtaker then did the same work again.
   Safe as one variable because the cell queue runs one rebuild at a time. */
let _cellAsk=0;
/** An engine failure as one line — round 18bd (F1).
 *
 *  `Engine.call` attaches the command and a stack trace to what it throws, which is right
 *  for the console and wrong for a status line under a button: the line the person needs
 *  is the first one, and the rest turns the panel into a wall of file URLs. */
function why(e){
  return String((e && e.message) || e || '').split('\n')[0].trim();
}

function rebuildCellPreview(raw){
  /* The simplified viewport's "choose a rule" notice belongs to the simplified viewport.
     Said here as well as in `_rebuildPreview`, because this is the function that owns the
     cell view and a notice outliving the thing it described is worse than never showing
     one — a cell that has not been chosen has its own line under the picker. */
  if(typeof showEmptyViewport==='function') showEmptyViewport(false);
  // Round 18n: the newest ask wins here too - a stroke's end asks straight here, and ten
  // strokes queued behind one another were ten rescatters where one would do.
  const my=(App.previewAsk=(App.previewAsk|0)+1);
  tellPreviewAsk(my);
  /* Round 18bd (F1): the busy card always comes down, and a failure is said somewhere a
     person can see it.
     There are seven `Busy.show` calls in the rebuild and one `Busy.hide`, with five early
     returns between them — so picking an interior the load order no longer has (one
     remembered from another install, or a door whose DNAM names a room a removed mod
     supplied) left the full-screen card sitting at 35% "Loading cell…" over the
     viewport, unclickable, until some later rebuild happened to run all the way through.
     And the error was invisible by construction: this `.catch` swallowed everything, so
     `10_preview.js`' own `catch { setCellStatus('Failed: …') }` was dead code that could
     never fire for anything thrown inside here.
     Hidden only by the newest ask, because a superseded run is not the one the card
     belongs to any more and its successor is still working. */
  const next=_cellQueue
    .then(()=>{ if(my!==App.previewAsk) return; _cellAsk=my; return _rebuildCellPreview(); })
    .catch(e=>{
      console.error(e);
      if(my===App.previewAsk) setCellStatus(T('cell.status_failed',{err:why(e)}),'warn');
    })
    .finally(()=>{ if(my===App.previewAsk) Busy.hide(); });
  _cellQueue=next;
  /* `raw` is `_rebuildPreview`'s, which awaits this from inside the other queue and would
     otherwise wait on itself; everyone else waits for the scene of the newest ask. */
  return raw? next : next.then(()=>previewSettled());
}

async function _rebuildCellPreview(){
  const R=App.R, s=activeSel();
  if(!R) return;
  R.opts.inspecting=false;   // round 18l: a cell, not the editor
  // Same reason as `_rebuildPreview`: the button follows the mode, however it was set.
  if(App.syncPaintButton) App.syncPaintButton();
  /* Which world this run is about. Loading a cell is a dozen round trips to the
     engine, and connecting a different install in the middle of one leaves the rest of
     the run asking about cells that no longer exist — an error in the console and a
     scene half-built from two worlds. Checked after every await that can outlive the
     install rather than at the top, because the top is the one place it cannot have
     changed yet. */
  const world=GameData.sig;
  const stale=()=>GameData.sig!==world;
  const target=App.cellSel;
  if(!target){
    R.disposeStatics(); R.setStatics([]);
    R.disposeBatches(); R.setBatches([]);
    R.setMarkers(null); R.setMissing(null); R.setCellTerrain([]); R.setWater(null);
    R.setPickables([]);
    R.setArrows(null); R.arrowFrom=null;
    /* The cached scene describes what is *installed in the renderer*, and this has just
       removed all of it. Leaving the cache behind is how "the grass came back but the
       terrain did not" happened: a later rebuild whose key matched took the fast path,
       which only rescatters, and drew grass over ground that had been torn down. The
       invariant is one line — whatever clears the renderer clears this. */
    App._scene=null;
    renderCellTextureList([]);
    $('#stats').innerHTML='<div class="ttl">'+T('cell.stats_title')+'</div>'+
      '<div><span class="k">'+T('cell.no_cell_chosen')+'</span></div>';
    setCellStatus(T('cell.status_choose'),'warn');
    return;
  }

  /* The heavy half of this — loading cells, building terrain, resolving every static
     mesh and its collision shape — depends on none of the grass settings. Rebuilding
     it for a slider drag is what made the "Scattering grass" bar appear constantly
     and take the interface with it, so it is cached against the things it actually
     depends on and skipped whenever those are unchanged. */
  const sceneKey=JSON.stringify([
    target.kind, target.x, target.y, target.name||'',
    App.showAdjacent, App.showCorpses, App.showActors!==false, npcNightHour(), !!App.npcDrawn,
    App.avoidStatics, App.avoidPad, App.avoidMinSize, (App.avoidExclude||[]).join('|'),
    GameData.sig||'',
    /* The simplified patch is a cell whose *contents* change while its grid position
       stays put, so it is the one cell whose identity is not its coordinates. Left out
       of this key, every simplified setting reached the engine, rebuilt the landscape,
       and then found a scene that matched on grid position and was kept. */
    App.mode==='cell'? '' : (App.patchSig||''),
  ]);
  /* Two conditions, not one. The key says the scene *would* be the same; the second
     asks whether it is still on the renderer at all. They can disagree — anything that
     clears the viewport without going through here leaves a cache describing terrain
     that is gone — and the fast path only rescatters, so believing the key alone draws
     grass in mid-air. Cheap to ask, and it turns a broken frame into a slow one. */
  /* "Still on the renderer" is a different question indoors: a room has no terrain
     chunks at all, so asking for them made `reuse` always false in interiors — and every
     stroke-end rescatter in a cave paid for a full rebuild (cell data, every static's
     mesh, the tint, the pickables) to change nothing but the grass. A room's presence
     is its objects. */
  const onScreen = target.kind==='int'
    ? !!(R.statics && R.statics.length)
    : !!(R.cellChunks && R.cellChunks.length);
  const reuse = App._scene && App._scene.key===sceneKey && onScreen;
  /* Round 17y: the same key with the target left out - what has to agree for a step to
     the next cell to keep what is already on the renderer (see `pan` below). */
  const panKey=JSON.stringify([
    target.kind, App.showAdjacent, App.showCorpses, App.showActors!==false, npcNightHour(), !!App.npcDrawn,
    App.avoidStatics, App.avoidPad, App.avoidMinSize, (App.avoidExclude||[]).join('|'),
    GameData.sig||'', App.mode==='cell'? '' : (App.patchSig||''),
  ]);

  /* Round 15 item 7: rings of neighbours - 1 is the eight around it, 2 is 25 cells, 3
     is 49. `true` from an older profile or test still means one ring. */
  const rings = target.kind==='ext'? adjacentRings() : 0;
  const wantNeighbours = rings>0;
  const targets=[target];
  if(wantNeighbours){
    for(let dy=-rings;dy<=rings;dy++) for(let dx=-rings;dx<=rings;dx++){
      if(!dx&&!dy) continue;
      targets.push({kind:'ext', x:target.x+dx, y:target.y+dy});
    }
  }

  if(reuse){
    // Nothing about the scene changed; go straight to the grass.
    const sc=App._scene;
    return scatterIntoScene(R,sc.cells,sc.origin,sc.centre,sc.texAgg,
                            sc.texMissing,sc.statics,sceneKey,sc.ring,sc.world,sc.failed,
                            {fresh:false, scene:{panKey:sc.panKey, anchor:sc.anchor, chunks:sc.chunks, aim:sc.aim, cz:sc.cz}});
  }

  Busy.show(T('busy.loading_cell'),target.label||target.name||'',0);
  await Busy.tick();
  if(stale()) return;
  /* Round 16b: where the time went, for the report - Robin: "the placing objects step
     was heavy". Wall time per phase, and what the objects phase moved. */
  const timing={label:(target.label||target.name||'')+(targets.length>1? ' + '+(targets.length-1)+' cells':''),
                t0:performance.now(), cells:0, terrain:0, objects:0,
                meshes:0, textures:0, bytes:0, calls:0, meshCached:0, texCached:0};
  const meshesBefore=App.meshCache.size, texBefore=App.texGL.size;
  const engineBefore=Engine.stats? {calls:Engine.stats.calls, bytes:Engine.stats.bytes} : null;
  /* A cell that fails to load is counted rather than passed over. `cell_data` answers
     for a cell with no landscape by saying so, not by throwing, so a throw here is a
     real failure — and a missing neighbour silently drops the coverage ring back to the
     clamped-edge behaviour §17c exists to prevent. Losing that quietly is the whole
     class of bug this round is about. */
  const cells=[]; const failed=[];
  /* Round 16: the cells asked for together, six at a time, rather than one after
     another. Twenty-five cells is twenty-five round trips to the engine, and each of
     them is answered on its own thread from a clone of the world; asked in sequence they
     cost their sum, asked together about the slowest few. The order of `cells` is the
     order of `targets` still - the centre first - because the scene is built from it. */
  {
    let done=0;
    const got=await inFlight(targets, 6, async(t,i)=>{
      try{ return {cell:await CellData.loadCell(t)}; }
      catch(e){ return {err:cellLabel(t)+': '+why(e)}; }
      finally{
        done++;
        Busy.show(T('busy.loading_cell'), (targets.length>1? T('busy.n_of',{done, total:targets.length}) : (t.label||t.name||'')), 0.35*done/targets.length);
        await Busy.tick();
      }
    });
    if(stale()) return;
    for(const g of got){
      if(!g) continue;
      if(g.cell) cells.push(g.cell); else failed.push(g.err);
    }
  }
  /* The coverage maps reach one sub-cell into each of the eight neighbours, so the
     ring has to be loaded whether or not any of it is drawn. It used to be loaded
     only when "Draw adjacent" was on, which meant the blend along every cell edge
     changed when that switch was flipped — and disagreed with the engine, which
     holds the whole world and always has the true ring. Loaded, not rendered: what
     the switch controls is what you see. */
  const ring=[];
  if(target.kind==='ext' && !wantNeighbours){
    const seen=new Set(cells.map(c=>c.gx+','+c.gy));
    const want=[];
    for(let dy=-1;dy<=1;dy++) for(let dx=-1;dx<=1;dx++){
      if(!dx&&!dy) continue;
      const gx=target.x+dx, gy=target.y+dy;
      if(seen.has(gx+','+gy)) continue;
      want.push({kind:'ext',x:gx,y:gy});
    }
    const got=await inFlight(want, 6, async t=>{
      try{ return {cell:await CellData.loadCell(t)}; }
      catch(e){ return {err:'('+t.x+', '+t.y+'): '+why(e)}; }
    });
    for(const g of got){ if(!g) continue; if(g.cell) ring.push(g.cell); else failed.push(g.err); }
  }
  if(stale()) return;
  /* Round 18bd (F1): and when nothing came back at all, the reasons are shown rather
     than dropped on the floor. `failed` has one line per cell that would not read; the
     status line under the cell button is where a person is already looking. */
  if(!cells.length){
    setCellStatus(failed.length
      ? T('cell.status_none_loaded',{list:failed.join('; ')})
      : T('cell.status_choose'),'warn');
    // Round 18cy: a history step whose cell would not load frees the arrows again.
    if(typeof CellHistory==='object' && App._histStep) CellHistory.settle();
    return;
  }
  const centre=cells[0];
  /* Round 17y: a step to a nearby cell keeps the scene's origin where it was.
   *
   * Robin: "clicking one arrow to focus on the next cell, at most changes one row of
   * cells, and the rest should be the same, which should make it possible to reuse
   * scattering etc. too." Everything on the renderer - the terrain chunks, the objects'
   * instances, the lamps, every blade of grass - is placed relative to the scene's origin,
   * and that origin used to be the centre cell's middle: one step and every position in
   * the scene was a different number, so nothing could be kept. Now the origin is
   * *anchored*: the first cell loaded is where it stays while the centre wanders within
   * PAN_REACH cells of it, and a chunk or a cell's scatter that was built for the old
   * scene is the right one for the new. Beyond the reach, or when anything else about the
   * scene changed (`panKey`), the origin moves to the new centre and the scene is built
   * afresh. Coordinates a dozen cells from the origin are still a few hundred thousand
   * units, well inside what a 32-bit float places to a fraction of a unit. */
  const PAN_REACH=12;
  const prev=App._scene;
  const pan = (App.mode==='cell' && target.kind==='ext' && prev && !reuse && prev.panKey===panKey
               && prev.origin && prev.anchor && prev.centre && prev.centre.kind==='ext'
               && Math.abs(target.x-prev.anchor.x)<=PAN_REACH && Math.abs(target.y-prev.anchor.y)<=PAN_REACH
               && onScreen) ? prev : null;
  const origin = pan? pan.origin : centre.origin;
  const anchor = pan? pan.anchor : {x:centre.gx, y:centre.gy};
  // The sky follows the cell: its region picks the Weather Adjuster preset (round 13).
  if(typeof Sky==='object'){ Sky.region=centre.region||''; Sky.showSources(); if(R) R.dirty=true; }

  /* How much of each cell to mesh. A real cell is drawn whole; the simplified patch is
     drawn to the size the engine painted it, plus the fade at its edge, which is the
     number `preview_patch` hands back. */
  const extent = App.mode==='cell'? 0 : (App.patchDrawn||0);

  /* Aim the camera at the ground in the middle of what was just loaded. A real cell can
     sit thousands of units above or below zero, so keeping the previous target would
     often leave the eye underneath the landscape looking at nothing.

     The simplified patch frames to the ground it draws. It used to frame to the raw
     "Patch size" number from its own dropdown — the one setting the camera did read —
     which is how a control that changed nothing else still looked like it was doing
     something. */
  let span = extent>0? extent : 8192*(targets.length>1?3:1);
  let cz=0;
  if(centre.kind!=='int' && centre.heights) cz=CellData.heightAt(centre,4096,4096);
  else if(centre.refs.length){
    let s=0; for(const r of centre.refs) s+=r.pos[2];
    cz=s/centre.refs.length;
  }
  /* A room is framed by what is in it. An exterior is 8192 units across by definition,
     which is why that number can be a constant; an interior is whatever its author built,
     from a closet to a Vivec canton, and framing every one of them to a cell's width puts
     a shop somewhere in the middle distance. There is no ground to measure, so the
     objects are the room. */
  if(centre.kind==='int'){
    let lo=[Infinity,Infinity,Infinity], hi=[-Infinity,-Infinity,-Infinity];
    for(const r of centre.refs)
      for(let k=0;k<3;k++){ lo[k]=Math.min(lo[k],r.pos[k]); hi[k]=Math.max(hi[k],r.pos[k]); }
    if(hi[0]>=lo[0]){
      // A little more than the objects themselves: a reference is a point, and the thing
      // it draws extends past it in every direction.
      span=Math.max(512,Math.max(hi[0]-lo[0], hi[1]-lo[1])*1.35+512);
      cz=(lo[2]+hi[2])/2;
    }else{
      span=1024;                       // an empty room still has to be looked at
    }
  }
  /* And the camera has to be aimed at the room, not at the origin — an exterior is always
     drawn about its own middle, a room is drawn where its author put it. Round 17y: an
     exterior's middle is its offset from the anchored origin, which is (0, 0) only for the
     cell the origin was anchored on. */
  const aim = centre.kind==='int'? interiorCentre(centre) : cellOffsetFrom(origin,centre);
  App._frameInfo={span,cz,aim};
  const tkey=(target.kind==='int'? 'i:'+(target.name||'') : target.x+','+target.y);
  const fkey=tkey+'|'+targets.length+'|'+span;
  /* Round 18a: a cell opened through a door is framed on the door's marker, not on the
     cell's middle. Robin: "Make the camera focus on the location the (invisible) door
     marker is in the new cell after it has loaded." The DODT is where the player lands, in
     the destination's own coordinates - a room's, or the world's, which the scene draws
     about its origin. The camera pivots on that point, close enough to see a doorway
     rather than a district. Round 18d: where the eye stands. 18a put it behind the marker,
     looking the way the player arrives facing - and behind the marker is the door, and
     the house the door is in: "I often end up teleported into meshes". Now it stands off
     to the side and ahead, at 135 degrees from the facing, a little above, looking back
     towards the marker - and so towards the door that leads back, which is the thing worth
     seeing. (An ESM rotation is clockwise from +Y, so the facing is (sin, cos); the eye's
     bearing from the pivot is that turned 135 degrees, and `az` is the eye's bearing.)
     Taken once, for the target it was set for; a different cell chosen in between drops it. */
  const doorAim=(App._doorAim && App._doorAim.key===tkey)? App._doorAim : null;
  App._doorAim=null;
  /* Round 18cy: the visit history (32_cellhistory.js). A step through it arrives with a
     plan for the camera, taken here; any other Real cell load is a person asking for a
     cell, and is recorded - now, while the camera still stands where the cell being left
     had it. `App._histVia` is how they asked (the arrows and the door button say; the
     picker and everything else is a pick). */
  let histPlan=null;
  if(App.mode==='cell' && typeof CellHistory==='object'){
    const step=App._histStep;
    if(step && step.key===tkey){ histPlan=step.plan; CellHistory.settle(); }
    else{
      if(step) CellHistory.settle();           // a step whose cell was overtaken by another load
      const via=App._histVia||null; App._histVia=null;
      CellHistory.record(target, via);
    }
  }
  const hp=histPlan && ((histPlan.kind==='door' && histPlan.door) || (histPlan.kind==='carry' && histPlan.cam)
                        || (histPlan.kind==='restore' && histPlan.cam))? histPlan : null;
  if(doorAim){
    const ax=doorAim.pos[0]-origin[0], ay=doorAim.pos[1]-origin[1];
    R.frameAt(DOOR_FRAME_SPAN, doorAim.pos[2], ax, ay);
    const facing=Math.atan2(Math.cos(doorAim.rot), Math.sin(doorAim.rot));   // the bearing the player faces
    R.cam.az=facing+DOOR_EYE_TURN;
    R.cam.el=DOOR_EYE_EL;
    App._framedKey=fkey; App._restoreCam=null;
    /* Round 18f, Robin: "I would want to move the camera outside any mesh it might spawn
       into as well, still looking at the door marker from whatever new location it
       found." The objects are not placed yet - `settleDoorEye`, once they are, walks the
       line from the marker to the eye and stops the eye short of the first thing in the
       way. Remembered as set, so a camera the person has since moved is left alone. */
    App._doorEye={az:R.cam.az, el:R.cam.el, dist:R.cam.dist, pivot:[R.cam.tx,R.cam.ty,R.cam.tz]};
  }
  else if(hp && hp.kind==='door'){
    /* Round 18cy: back through a door - looking at the door itself, from the side the eye
       was on when the button was pressed, framed as a door arrival is but further out
       (`DOOR_BACK_FAR`), and never further than the eye stood: the person saw the door
       from there, so it is a clear place to stand, where the framed distance might be
       inside the wall behind them. The pivot is the door's middle, so no pulling-in past
       whatever is in the way (`settleDoorEye` would meet the door's own faces first). */
    const d=hp.door;
    R.frameAt(DOOR_FRAME_SPAN*DOOR_BACK_FAR, d.pos[2], d.pos[0]-origin[0], d.pos[1]-origin[1]);
    if(d.eyeDist>0) R.cam.dist=Math.min(R.cam.dist, Math.max(DOOR_EYE_MIN, d.eyeDist));
    R.cam.az=d.az; R.cam.el=DOOR_EYE_EL;
    R._orbitG=null; R.dirty=true;
    App._framedKey=fkey; App._restoreCam=null;
  }
  else if(hp && hp.kind==='carry'){
    /* Round 18cy: over a cell-arrow transition the camera is carried, as the arrows carry
       it (`pan` below): the same view, moved by the distance between the two cells'
       middles - in world units, so it holds whether or not the scene could be panned. */
    const c=hp.cam;
    const dx=(origin[0]+aim[0])-c.aim[0], dy=(origin[1]+aim[1])-c.aim[1], dz=cz-c.cz;
    R.cam.tx=c.pivot[0]+dx-origin[0]; R.cam.ty=c.pivot[1]+dy-origin[1]; R.cam.tz=c.pivot[2]+dz;
    R.cam.az=c.az; R.cam.el=c.el; R.cam.dist=c.dist; R.cam.vs=c.vs;
    R._orbitG=null; R.dirty=true;
    App._framedKey=fkey; App._restoreCam=null;
  }
  else if(hp && hp.kind==='restore'){
    // Round 18cy: over a picker transition the camera the cell was left with comes back.
    const c=hp.cam;
    R.cam.tx=c.pivot[0]-origin[0]; R.cam.ty=c.pivot[1]-origin[1]; R.cam.tz=c.pivot[2];
    R.cam.az=c.az; R.cam.el=c.el; R.cam.dist=c.dist; R.cam.vs=c.vs;
    R._orbitG=null; R.dirty=true;
    App._framedKey=fkey; App._restoreCam=null;
  }
  else if(App._restoreCam && App._restoreCam.key===fkey){
    // Back from the mesh editor to the same scene: the camera it left with (round 15).
    /* Round 18bo: and the orbit pivot goes on whatever the restored view is pointed at.
       Robin: "I want the orbit pivot to reset every time I go back to the full cell preview
       from mesh edit mode. Now there are some cases where it doesn't (like entering mesh
       mode with orbit mode, and then going back)."

       First written as "only if the scheme changed while you were away", on the reasoning
       that round 15 promised the camera back untouched. Wrong reading of it: round 15 is
       about *where you are and what you are looking at*, and `focusOnView` moves neither —
       it slides the pivot along the view axis and leaves the eye exactly where it stands.
       The view is identical either way; what changes is what the next drag turns around.
       And a pivot left over from the editor is not about this scene whichever scheme you
       were in, so there is no case where keeping it is the better answer.

       Flagged rather than done here, because `focusOnView` casts a ray into the scene and
       there is no scene yet — the terrain and the statics are built further down. */
    App._refocusOrbit = R.nav==='orbit';
    Object.assign(R.cam, App._restoreCam.cam); App._restoreCam=null; App._framedKey=fkey; R.dirty=true;
  }
  else if(pan && pan.aim && App._framedKey!==fkey){
    /* Round 17y: a step to the next cell carries the camera with it - the same view over
       the new cell, your zoom and angle kept, a cell's width further on - rather than
       framing the cell afresh as a first load does. Walking a coastline is what the arrows
       are for, and a walk that reset the zoom at every step was not one. */
    const dx=aim[0]-pan.aim[0], dy=aim[1]-pan.aim[1], dz=(cz-(pan.cz||0));
    R.cam.tx+=dx; R.cam.ty+=dy; R.cam.tz+=dz;
    if(R.fly && R.fly.pos){ R.fly.pos[0]+=dx; R.fly.pos[1]+=dy; R.fly.pos[2]+=dz; }
    R._orbitG=null; R.dirty=true;
    App._framedKey=fkey; App._restoreCam=null;
  }
  else if(App._framedKey!==fkey){ R.frameAt(span,cz,aim[0],aim[1]); App._framedKey=fkey; App._restoreCam=null; }

  // ---- terrain ----
  timing.cells=performance.now()-timing.t0;
  Busy.show(T('busy.building_terrain'),'',0.4);
  await Busy.tick();
  const chunks=[]; const texAgg=new Map(); const texMissing=new Set();
  /* Round 18bf (§I19): how many of these cells have landscape to measure.

     The coverage of a ground texture was averaged over **every** cell in the scene, and a
     cell with no LAND record — the sea off a coastline, an unmade cell in the middle of a
     mod's landmass, an interior in the list — contributes nothing to the sum and was
     counted in the divisor all the same. So a texture covering 100% of every cell that
     actually has ground reported 36%, and the panel's own explanation of the number made
     it sound like a fact about the world. Divided by what was measured. */
  let landCells=0;
  /* The bounds of what is actually drawn, in world units, so the water sheet is the
     size of the ground rather than a guess at it — one cell, nine cells or half a patch,
     the same arithmetic. */
  let gx0=Infinity, gy0=Infinity, gx1=-Infinity, gy1=-Infinity;
  // A cell's coverage maps reach one sub-cell into each of its eight neighbours, so
  // the loaded set has to be addressable by grid position.
  const byGrid=new Map();
  for(const c of cells.concat(ring)){ if(c.kind!=='int') byGrid.set(c.gx+','+c.gy,c); }
  /* Round 16: every land texture the loaded cells wear, asked for together before the
     chunks are built. The loop below used to await each layer's texture in turn - a
     dozen textures a cell, twenty-five cells, one round trip each, with the page idle
     for every one. They are cached after the first load, so this is the first load's
     cost; and it is the same set of files whichever order they come in. */
  {
    const files=new Set();
    for(const c of cells){
      if(c.kind==='int'||!c.heights) continue;
      for(const L of (c.layers||[])){ const f=L.id? GameData.textureFileFor(L.id) : null; if(f) files.add(f); }
    }
    if(files.size) await inFlight([...files], 8, f=>loadTexture(f));
  }
  /* Round 17y: the chunks the previous scene built, by cell. On a step to the next cell
     the ones still in view are handed to the renderer again as the same objects, and it
     keeps their vertex arrays and coverage maps (`setCellTerrain`). */
  const oldChunks=new Map();
  if(pan) for(const ch of (pan.chunks||[])) if(ch.grid) oldChunks.set(ch.grid[0]+','+ch.grid[1],ch);
  let chunksKept=0;
  for(const c of cells){
    if(c.kind==='int'||!c.heights) continue;
    const off=cellOffsetFrom(origin,c);
    const old=oldChunks.get(c.gx+','+c.gy);
    if(old){
      chunks.push(old); chunksKept++;
      gx0=Math.min(gx0,old.span[0]+off[0]); gx1=Math.max(gx1,old.span[1]+off[0]);
      gy0=Math.min(gy0,old.span[0]+off[1]); gy1=Math.max(gy1,old.span[1]+off[1]);
      landCells++;
      for(const L of old.layers){
        if(L.id && !L.tex && !L.flat) texMissing.add(L.id);
        if(L.id){ texAgg.set(L.id,(texAgg.get(L.id)||0)+L.coverage); }
      }
      continue;
    }
    landCells++;
    const T=cellTerrain(c,(dx,dy)=>byGrid.get((c.gx+dx)+','+(c.gy+dy))||null,extent);
    if(off[0]||off[1]){
      const p=T.mesh.pos;
      for(let i=0;i<p.length;i+=3){ p[i]+=off[0]; p[i+1]+=off[1]; }
    }
    gx0=Math.min(gx0,T.span[0]+off[0]); gx1=Math.max(gx1,T.span[1]+off[0]);
    gy0=Math.min(gy0,T.span[0]+off[1]); gy1=Math.max(gy1,T.span[1]+off[1]);
    /* The cell's land textures (and, Wraithguard, their maps) asked for together rather
       than one after another. */
    await Promise.all(T.layers.map(async L=>{
      const file=L.id? GameData.textureFileFor(L.id):null;
      if(!file) return;
      /* Wraithguard: the layer's own maps, as the setup's OpenMW takes them - its
         _diffusespec *in place of* the diffuse (the same picture, the highlight's
         strength in its alpha) and its normal map beside it. */
      const maps=App.cellNormalMaps? landMaps(file) : null;
      const [t,n]=await Promise.all([loadTexture((maps&&maps.dspec)||file),
                                     (maps&&maps.nrm)? loadTexture(maps.nrm) : null]);
      if(t&&t.gl){ L.tex=t.gl; L.specA=!!(maps&&maps.dspec&&t.alpha!=='opaque'); }
      if(n&&n.gl) L.nrm=n.gl;
    }));
    for(const L of T.layers){
      // Unpainted ground has no file to load when the vanilla one is only inside a
      // BSA, and a missing-texture checker there would be a lie: the game draws dirt.
      if(!L.tex && L.id===DEFAULT_LTEX) L.flat=DEFAULT_LTEX_COLOR;
      // Worth naming rather than leaving as an unexplained checker on the ground.
      if(L.id && !L.tex && !L.flat) texMissing.add(L.id);
      if(L.id){
        const cur=texAgg.get(L.id)||0;
        texAgg.set(L.id,cur+L.coverage);
      }
    }
    chunks.push({mesh:T.mesh,layers:T.layers,inner:T.inner,total:T.total,span:T.span,
                 grid:[c.gx,c.gy]});
  }
  /* The slab under the ground wears the default dirt (round 12 item 5) — the texture
     when the install can supply it, its flat stand-in colour when it cannot. Before
     the terrain goes in, so the slab is built already dressed. */
  {
    const file=(typeof GameData!=='undefined' && GameData.textureFileFor)? GameData.textureFileFor(DEFAULT_LTEX) : null;
    let tex=null;
    if(file){ try{ const t=await loadTexture(file); if(t&&t.gl) tex=t.gl; }catch(_){ } }
    R.setSlabLook({tex, flat:DEFAULT_LTEX_COLOR});
  }
  /* Round 18bf (§I19): the average, over the cells that had ground to average. Done once
     here rather than a share at a time inside the loop, because the divisor is not known
     until the loop has been round. */
  if(landCells>1) for(const [id,sum] of texAgg) texAgg.set(id,sum/landCells);
  R.setCellTerrain(chunks);
  /* The brush reads the drawn surface through this, and the paint overlay needs the
     masks for whatever is now on screen. Both are set after the terrain rather than
     before: until `setCellTerrain` has run there is nothing for either to be about. */
  R.groundZ=sceneGroundZ;
  const ground = gx1>gx0
    ? {c:[(gx0+gx1)/2,(gy0+gy1)/2], half:Math.max(gx1-gx0,gy1-gy0)/2}
    : null;

  // ---- objects ----
  /* The old batches are *not* thrown away here.
   *
   * They used to be, and this walk is full of awaits — every mesh and every texture is
   * a round trip. A rebuild abandoned partway through (the install was swapped, the
   * cell would not load) had by then already emptied the viewport, so the scene on
   * screen lost its objects while the cached scene went on claiming they were there.
   * Robin saw the same shape of thing with the terrain and the water after an undo.
   *
   * So: build first, install second, and let the replacement be the thing that frees
   * the old one. A run that gives up leaves the previous scene exactly as it was, which
   * is the only honest thing an abandoned run can do. */
  const staticBatches=[];
  const pickables=[];
  // Round 17m: {p, r, c, neg} per LIGH reference in the nine cells being drawn.
  const cellLights=[];
  let tris=0, drawn=0, systemsPlaced=0;   // systemsPlaced: round 17w, the particle systems
  const pendingSystems=[];                 // and the systems themselves, placed after the disposal
  /* Round 17y: in a room that is not a quasi-exterior, every lamp burns and every flame
     runs whatever the clock says - there is no sky to go dark under. A room flagged to
     behave like an exterior keeps the exterior's hours. */
  const roomAlwaysOn = roomIndoors(cells);
  /* Round 18q: where the moths gather, if the mod that puts them there is installed. A
     lamp's AttachLight point apiece, filled while the lamps below are placed and drawn
     as one mesh's instances afterwards. Not in a room the sky never reaches. */
  const mothSpots=[];
  const mothsHere = !roomAlwaysOn && await Moths.check().catch(()=>false);
  if(stale()) return;
  /* Drawing only. The engine decides what blocks grass and reports what it stepped
     over; this walk exists so the viewport has something to show, and it is a
     different question with a different answer — a corpse that is not being drawn is
     still an obstacle, and an object on the never-avoid list is still drawn.

     There used to be a "Draw statics" switch in front of this block. Gating the
     *avoidance* on it was a mistake that shipped once — hiding the rocks moved the
     grass, in the preview only — and once that was fixed the switch did nothing but
     empty the cell of the objects the cell was loaded to be seen against. Objects are
     always drawn. */
  {
    timing.terrain=performance.now()-timing.t0-timing.cells;
    if(typeof LoadProf==='object') LoadProf.reset();   // round 18cr: the objects phase, apart
    Busy.show(T('busy.placing_objects'),'',0.5);
    await Busy.tick();
    const byMesh=new Map();
    App._sceneMarkers=[];   // Wraithguard: the editor markers, not drawn, for the overlay (48_wg_tools.js)
    for(const c of cells){

      const off=cellOffsetFrom(origin,c);
      for(const ref of c.refs){
        if(ref.gc) continue;   // grass from a groundcover plugin: drawn with the grass (addGroundcoverRuns)
        const key=(ref.id||'').toLowerCase();
        const actor=CellData.actors.get(key);
        let model;
        if(actor){
          /* Round 18af. A living actor is not drawn: it walks away from wherever the
             plugin put it, and a strider-shaped hole under a starting spot would be a
             lie about where the grass goes. The silt strider is the exception Robin
             asked for — "Silt striders are a special case, and for now I only want to
             render the Silstrider creature and no other. Don't avoid it however, let
             grass grow through." A parked strider is a landmark you place grass
             *around* by eye, and it is the one creature in the game that is furniture.

             Matched on the mesh rather than on the creature's id, so a replacer keeps
             working and a mod's own strider (a second port, a wrecked one) is caught by
             the same line. Drawing it changes nothing about placement: the engine still
             answers `LivingActor` for it, so it is never an obstacle, never something
             grass is scattered *onto*, and grass grows through it exactly as before.
             It is pickable like anything else drawn — right-clicking a thing you can see
             and being told nothing is the puzzle, not the feature. `t_engine` holds both
             halves: it is in the scene, the living rat beside it is not, and the cell's
             `aliveSkipped` and `rejObstacle` are what they were. */
          const strider=actor.kind==='crea' && STRIDER_MESH.test(actor.model||'');
          /* Wraithguard: the living are drawn too now - an NPC assembled by the engine from
             its skeleton, body parts and what it wears (`__npc/<id>`, viewcore npc.rs), a
             creature from its `x` twin with its idle, and a leveled-creature spawn point
             with the model the list resolved to. The dead keep the corpse rule below. */
          /* Dead only when the record says health 0. The engine's `corpse` also counts an
             actor with auto-calculated stats and the "Corpses persist" flag, which was
             right for keeping grass out and is wrong for drawing: most named NPCs -
             Caius Cosades among them - are exactly that, and alive. */
          const dead=actor.health===0;
          const living=App.showActors!==false && !dead;
          if(living){
            if(actor.kind==='npc') model=NPC_MESH_PREFIX+(ref.id||'')+npcFlags(roomAlwaysOn);
            else if(actor.kind==='lev'){
              model=actor.model||'';
              /* Wraithguard: the Construction Set's creature marker, drawn with its fix
                 (R-Zero's "Leveled Creature marker direction fix"): it faces the way the
                 creature will. Built in (viewcore markers.rs). */
              if(/(^|[\\/])marker_creature\.nif$/i.test(model)) model='__wg/marker_creature.nif';
            }
            else model=actor.twin||actor.model||'';
            if(!model) continue;
          }
          else if(actor.kind==='lev') continue;
          else if(!strider && (!App.showCorpses || !dead)) continue;
          // An NPC has no mesh — the game assembles one from body parts — so it is
          // drawn as the same body-shaped slab the engine keeps grass out of.
          /* Round 18ag: the living strider is drawn from its `x` twin when the load order
             has one - the file the game draws it with, the one that carries the lanterns
             and the `.kf` beside it - and a corpse from the plain file, still. */
          if(!living) model=strider? (actor.twin||actor.model)
              : (actor.kind==='npc'||!actor.model)? CORPSE_SLAB : actor.model;
        }else{
          model=CellData.models.get(key);
          if(!model) continue;
          // Editor markers are not drawn, for the same reason the engine does not avoid
          // them: the game does not draw them either.
          if(isMarkerMesh(model)){
            App._sceneMarkers.push({id:ref.id, model, rot:ref.rot, scale:ref.scale||1,
              local:[ref.pos[0]-c.origin[0]+off[0], ref.pos[1]-c.origin[1]+off[1], ref.pos[2]]});
            continue;
          }
        }
        const local=[ref.pos[0]-c.origin[0]+off[0], ref.pos[1]-c.origin[1]+off[1], ref.pos[2]];
        let a=byMesh.get(model); if(!a){ a=[]; byMesh.set(model,a); }
        const entry={ref,local,id:ref.id,model,cellKey:c.gx+','+c.gy};
        /* Round 17m: a reference to a LIGH record carries a light. Where exactly is not
           known until the mesh is read — the game hangs it on a node called AttachLight
           when the mesh has one — so the entry is marked here and placed below. */
        const li=CellData.lights.get(key);
        if(li) entry.light=li;
        a.push(entry);
      }
    }
    /* Every distinct mesh in the cell, asked for at once. This was a `for` loop with an
       `await` in it: one round trip after another, a hundred of them for a busy cell,
       with the page idle for each. The engine answers off the main thread and in
       parallel now (§43, §46), so the wait is the slowest few rather than the sum.
       The batches are still built in map order afterwards, so what is drawn does not
       depend on which mesh happened to answer first. */
    const meshList=[...byMesh.entries()];
    await loadGitdConfig();   // round 17x: the window mod's variance setting, if it has one
    /* Round 16b: bundled - a few round trips carrying every mesh and every texture
       the page does not have, instead of one per file (`loadMeshes`, 10_preview.js).
       The bar counts them in as they land. */
    const loaded=await loadMeshes(meshList.map(([model])=>(model===CORPSE_SLAB || model.startsWith(NPC_MESH_PREFIX) || model.startsWith('__wg/'))? model : 'Meshes\\'+model),
      (done,total)=>{ Busy.show(T('busy.placing_objects'), T('busy.n_of_meshes',{done, total}), 0.5+0.35*done/Math.max(1,total)); });
    timing.tBuild=performance.now();   // round 18cr: the scene from what arrived
    /* Wraithguard: what the loaded cells ask for and the setup does not have - meshes that
       did not load, and textures that did not - for the review tools' Missing files. */
    { const miss=[];
      meshList.forEach(([model,list],mi)=>{
        const info=loaded[mi];
        if(!info || info.err){ if(!model.startsWith(NPC_MESH_PREFIX) && model!==CORPSE_SLAB) miss.push({kind:'mesh', name:model, count:list.length, err:(info&&info.err)||''}); return; }
        const seen=new Set();
        for(const p of (info.parts||[])){
          const t=p.glTex;
          if(t && t.err && p.tex && !seen.has(p.tex)){ seen.add(p.tex); miss.push({kind:'texture', name:p.tex, count:list.length, err:t.err, mesh:model}); }
        }
      });
      App._sceneMissing=miss; }
    for(let mi=0; mi<meshList.length; mi++){
      const [model,list]=meshList[mi];
      const info=loaded[mi];
      if(!info||info.err) continue;
      /* Round 17w: a mesh with no geometry but a particle system — the waterfall mist,
         `ex_waterfall_mist_01.nif`, is nothing but one — still places its systems. */
      const hasParts=!!info.parts.length;
      if(!hasParts && !(info.particles && info.particles.length)) continue;
      /* Round 17r: the instances sorted by the cell they stand in, with a range and a
         world box per cell — which is what lets the renderer skip them.
         Nothing was culled before this: every batch and every instance in it was
         submitted every frame, so looking one way at forty-nine cells paid for eight
         times the scenery on screen. Splitting the batches per cell was the obvious fix
         and the wrong one — measured, it takes 2,767 batches to 12,150 — so the batch
         stays one per mesh part and the *instances* are grouped instead. Sorted by cell,
         each cell's instances are contiguous, so a run of visible cells is one draw at an
         offset rather than a batch of its own. */
      list.sort((a,b)=> a.cellKey<b.cellKey? -1 : a.cellKey>b.cellKey? 1 : 0);
      const m=new Float32Array(list.length*12);
      list.forEach((r,k)=>m.set(refMatrix(r.local,r.ref.rot,r.ref.scale),k*12));
      const groups=(hasParts && info.visAabb)? cellRuns(list, m, info.visAabb) : [];
      /* Round 17m: the lamps this mesh puts in the cell. `info.attachLight` is where the
         mesh says its flame is, in its own space, so it goes through the same matrix the
         geometry does; a mesh with no such node lights from its origin, which is what
         OpenMW falls back to. Colour is bytes as the record wrote them, over 255. */
      /* Round 17w: and the hours each lamp keeps, on the instance itself, so its glow map
         and its emissive follow the same two numbers its point light does — Robin: "so
         they come and go together." (-1,-1) marks an instance that is no lamp, whose glow
         is always on. The stream is only made when the mesh has a lamp on it at all. */
      let lamp=null;
      /* Wraithguard: an NPC carrying a torch at night - the engine puts the torch's light
         in the NPC's payload, at its AttachLight on the hand, with the LIGH record's
         colour and radius (viewcore npc.rs `CarriedLight`). Lit whenever it is carried. */
      const carried = model.startsWith(NPC_MESH_PREFIX) && info.attachLight && info.attachLightDef;
      if(carried) for(const r of list)
        r.light={radius:info.attachLightDef.radius, colour:info.attachLightDef.colour.map(c=>c*255), carried:true};
      for(let k=0;k<list.length;k++){
        const r=list[k];
        if(!r.light) continue;
        const t=m.slice(k*12,k*12+12);   // three rows of a 3x4, translation at 3, 7, 11
        const a=info.attachLight||[0,0,0];
        const p=[t[0]*a[0]+t[1]*a[1]+t[2]*a[2]+t[3],
                 t[4]*a[0]+t[5]*a[1]+t[6]*a[2]+t[7],
                 t[8]*a[0]+t[9]*a[1]+t[10]*a[2]+t[11]];
        /* Round 17y: a fire keeps no hours, and neither does any lamp in a room that is
           not a quasi-exterior - (-1,-1) on the instance is what the shader, the point
           light and the flame all read as "always" (see `lampLitAt`). Robin: "Campfires
           (like Furn_De_Firepit_F_400) and fireplaces should always be on"; "All other
           light sources in interiors [...] should always be on". */
        /* Round 18q: and whether the moths gather at this one. The mod hangs them on
           the same `AttachLight` node the flame is on, so the point is the one just
           worked out; a lamp whose mesh has no such node takes the mod's own offset from
           the reference origin. Exteriors and quasi-exteriors only, as the mod has it. */
        if(mothsHere && Moths.has(model)){
          const o=info.attachLight? null : Moths.offset(model);
          const at=o? [p[0]+o[0], p[1]+o[1], p[2]+o[2]] : p.slice();
          /* Round 18dt, Robin: "For Moths: don't draw under water." A lamp that stands
             below the water line - a sunken lantern - keeps its light and gets no swarm. */
          if(!(centre.water!=null && at[2] < centre.water)) mothSpots.push(at);
        }
        const always = roomAlwaysOn || r.light.carried || isFireLight(r.id, model);
        /* (-2,-2): a lamp that keeps no hours. Not -1, which the stream's fill means "no
           lamp at all" - the shader tells the two apart since round 18i, so Off can put a
           fire's glow out without putting out every glowing static (see FS_GRASS glowOn). */
        const [on,off]= always? [-2,-2] : R.lampHours(p);
        if(!lamp){ lamp=new Float32Array(list.length*2); lamp.fill(-1); }
        lamp[k*2]=on; lamp[k*2+1]=off;
        r.lampHours=[on,off];
        cellLights.push({
          p, on, off, always,
          r:Math.max(16, r.light.radius||0),
          c:[(r.light.colour[0]||0)/255, (r.light.colour[1]||0)/255, (r.light.colour[2]||0)/255],
          neg:!!r.light.negative});
      }
      /* Round 17x: a Glow in the Dahrk window keeps the mod's hours on every instance that
         is not already a lamp — the switch reads them the way the glow does. From the
         reference's world position, as the mod does, so its variance lands on the same
         windows here as in the game. */
      if(info.parts.some(p=>p.dayNight>0)){
        if(!lamp){ lamp=new Float32Array(list.length*2); lamp.fill(-1); }
        /* Round 17y: and, in a room lit by its own record, the light the window throws in
           while its sunlit branch shows - the mod's `addInteriorLights`. From the mesh's
           AttachLight, with the file's own light there when it has one (colour, and the
           radius kept as the node's scale) and white at 200 otherwise; without an
           AttachLight the mod invents a point a quarter of the way from the mesh's centre to
           the window, and the mesh's centre stands in for that here. The sun's colour and
           the dawn/dusk fade are the renderer's per frame (`windowLight`). */
        const roomWindow = roomAlwaysOn && info.parts.some(p=>p.dayNight===3);
        const def=info.attachLightDef||null;
        const a=info.attachLight || (info.visAabb? [(info.visAabb.x0+info.visAabb.x1)/2,(info.visAabb.y0+info.visAabb.y1)/2,(info.visAabb.z0+info.visAabb.z1)/2] : [0,0,0]);
        for(let k=0;k<list.length;k++){
          if(lamp[k*2]>=0) continue;
          const wp=list[k].ref.pos||[0,0,0];
          const [on,off]=windowHours(wp[0],wp[1]);
          lamp[k*2]=on; lamp[k*2+1]=off;
          if(roomWindow){
            const t=m.slice(k*12,k*12+12);
            const p=[t[0]*a[0]+t[1]*a[1]+t[2]*a[2]+t[3],
                     t[4]*a[0]+t[5]*a[1]+t[6]*a[2]+t[7],
                     t[8]*a[0]+t[9]*a[1]+t[10]*a[2]+t[11]];
            cellLights.push({p, on, off, window:true,
                             r:Math.max(16, (def && def.radius>0)? def.radius : 200),
                             c:def? def.colour.slice() : [1,1,1], neg:false});
          }
        }
      }
      /* Round 18ai, Robin: "make any animations (no matter if it's the silt strider or
         flags) start at little bit different positions in their keyframes (and then loop
         normally) to create a bit natural feeling differences." Every instance of a mesh
         shares one batch and so one clock, which is what had a row of banners flapping
         in lockstep. The instances of an animated mesh are dealt into a few phase bins
         instead, each bin a batch of its own with its own shift into the clip - the way
         the moths got their bins in 18r, with the same trade: a bin is a draw call, so a
         lone strider is one bin and a wall of banners at most eight. Each shift comes
         from where the bin's first instance stands, so the same cell looks the same after
         a reload and a lone banner keeps its own start wherever it is. */
      const bins=phaseBins(list, info);
      /* Round 18al: which bin each instance went into, for its pickable - the hover
         highlight draws the object again and has to read the same clock. Robin: "The
         yellow highlight on hovering the cursor over animated gifs is out of sync. It
         feels like the yellow highlight is another mesh is playing the same animation on
         top of the base one, but out of sync with the animation, thus drifting". It was
         exactly that: the highlight batch was built from the bare part with no shift and
         no rate, so it played the file's own clock over a bin running at 80-90% of it. */
      const phaseOf=new Array(list.length).fill(null);
      for(const bin of bins){
        const sub=bin.all? {m, n:list.length, groups, lamp} : subInstances(bin.idx, list, m, lamp, info);
        const phase=bin.speed? {shift:bin.shift, speed:bin.speed} : null;
        if(phase) for(const k of bin.idx) phaseOf[k]=phase;
        for(const part of info.parts){
          // A skinned (or, 18au, morph-animated) part's posed vertices live in its own GPU
          // buffers, so a bin with its own clock needs a copy of the part with buffers of its own.
          const pp=(bin.speed && (part.skin||part.morph))? phasedPart(part, bin.shift, bin.speed) : part;
          /* Round 18cr: `lazy` - the still ones are drawn through the renderer's material
             groups and never need a vertex array of their own (see `_buildGroups`). */
          const bt=R.buildStaticBatch(pp,{m:sub.m,n:sub.n,groups:sub.groups,lamp:sub.lamp},
            part.glTex? part.glTex.gl : null, true);
          if(bin.speed){ bt.animShift=bin.shift; bt.animSpeed=bin.speed; }
          staticBatches.push(bt);
          tris+=sub.n*(part.idx.length/3);
        }
      }
      /* Round 17w: the mesh's particle systems, one live set per placed instance — a
         waterfall's mist, a torch's flame. A lamp's systems keep the lamp's hours. */
      if(info.particles && info.particles.length && R.addParticleSystems){
        // Held until the old objects are disposed of below: disposing them clears the
        // runs, and these must outlive that.
        for(let k=0;k<list.length;k++)
          pendingSystems.push([info.particles, m.slice(k*12,k*12+12), list[k].lampHours||null, model]);
        systemsPlaced+=info.particles.length*list.length;
      }
      if(!hasParts) continue;
      drawn+=list.length;
      if(!info.visAabb) continue;
      /* Every drawn object is pickable, whether or not grass avoids it — one already
         on the never-avoid list is precisely the one you might right-click to take
         back off it. Nothing invisible is pickable, though: right-clicking empty air
         and getting a verdict on a rock you cannot see is a puzzle, not a feature.

         What goes over is the *visible* geometry: the drawn triangles in their own
         space, plus this instance's matrix. The box is only the broad phase — a first
         pass to find which few objects are worth testing properly — and the answer
         comes from the triangles. It used to be the collision box and nothing else,
         which is why the pointer picked a tree from several feet away and could not
         pick a mesh with no collision at all, however plainly it was on screen.

         `parts` is the model's array, shared by every instance of it and not copied:
         nine cells of the same shrub is one set of triangles and a matrix each. */
      list.forEach((r,k)=>pickables.push({
        id:r.id, model,
        m:Array.from(m.slice(k*12,k*12+12)),
        parts:info.parts,
        // Round 18al: the phase bin this instance plays in, or null for a still mesh.
        phase:phaseOf[k],
        aabb:worldAabb(info.visAabb,Array.from(m.slice(k*12,k*12+12))),
        plugin:pluginName(r.ref.from), refKey:r.ref.key||'',
        shape:info.colSource||'none', radius:info.colRadius||0,
        scale:r.ref.scale||1,
        // Round 17y: where a door leads, for the dialogue's "Open cell door leads to".
        door:r.ref.door||null,
        // Wraithguard: for the review and overlay tools (48_wg_tools.js).
        mark:r.ref.mark||'', hist:r.ref.hist||null, moved:r.ref.moved||0,
        wpos:r.ref.pos, rot:r.ref.rot, cellKey:r.cellKey, from:r.ref.from,
        visAabb:info.visAabb, light:r.light||null}));
    }
  }
  /* Round 18q: the moths, one copy of the mod's own mesh at each lamp it names. Built
     here rather than inside the loop above because the mesh is not a mesh any reference
     wears - it is an effect hung on top of one - and it is read once for the whole scene.
     Marked so the renderer can leave it out by daylight without a rebuild: the clock
     moves while you watch, and rebuilding a cell every time it crosses 18:00 would be an
     absurd price for a few hundred triangles. */
  if(mothSpots.length){
    try{
      const info=await loadMesh(Moths.MESH);
      if(stale()) return;
      if(info && !info.err && info.parts.length){
        /* Round 18r: and no two lanterns wear the same swarm. Robin: "Add a little
         * randomness to them in terms of speed the animation goes, what rotation they have
         * when they spawn, and offset time a little for animation, to make it look less
         * uniform when there are several lanterns next to each other."
         *
         * Three dials, and they cost different things.
         *
         * The **turn** is free: it goes in the instance's own matrix, so a lamp's whole
         * swarm is spun about the lantern and every lamp can have its own without another
         * draw call. That alone is most of the difference between two lanterns.
         *
         * The **speed** and the **phase** are the controller's, which is a uniform per
         * batch - so lamps that share a pair share a batch. `MOTH_PHASES` of them, and the
         * cost is that many batches per shape rather than one: four, which is eight moths
         * times four for a whole cell however many lanterns are in it. Splitting per lamp
         * would be a draw call each and a Balmora street would pay for it.
         *
         * Every number comes from the lamp's own position, the way `lampHours` staggers
         * the lamps themselves: the same swarm every frame, the same after a reload, and
         * the same for anyone else looking at that cell. */
        const spots=mothSpots.map(p=>({p, turn:frac2(p[0],p[1],3.1)*Math.PI*2,
                                       bin:Math.floor(frac2(p[1],p[0],9.7)*MOTH_PHASES)%MOTH_PHASES}));
        const bins=[];
        for(let i=0;i<MOTH_PHASES;i++) bins.push(spots.filter(s=>s.bin===i));
        for(let i=0;i<MOTH_PHASES;i++){
          const list=bins[i]; if(!list.length) continue;
          const mm=new Float32Array(list.length*12);
          list.forEach((s,k)=>mm.set(refMatrix(s.p,[0,0,s.turn],1),k*12));
          /* A whole cycle spread across the bins, and a speed within a tenth either side
             of the mesh's own - enough that two lanterns drift apart, little enough that
             none of them looks wrong on its own. */
          const phase=i/MOTH_PHASES, speed=1+(i-(MOTH_PHASES-1)/2)*(0.2/Math.max(1,MOTH_PHASES-1));
          for(const part of info.parts){
            /* Round 18au: the wingbeat is a morph on the shape, posed on the CPU into the
               part's own buffers - so each bin draws its own copy of the part, shifted
               into the flap's cycle by the same fraction `retimeAnim` shifts the flight. */
            const mspan=part.morph? ((+part.morph.stop||0)-(+part.morph.start||0)) : 0;
            const pp=(part.skin||part.morph)? phasedPart(part, phase*(mspan>1e-9? mspan : 1), speed) : part;
            const bt=R.buildStaticBatch(pp,{m:mm,n:list.length},part.glTex? part.glTex.gl : null);
            bt.moths=true;
            bt.anim=retimeAnim(part.anim, speed, phase);
            staticBatches.push(bt);
          }
        }
        App.mothLamps=mothSpots.length;
        // Where they ended up, relative to the scene's origin - for the report, and for
        // the test that the mod's attach point is the one Gardenfell used.
        App.mothAt=mothSpots.map(p=>p.slice());
        // And how they were varied, for the test that says they were.
        App.mothVary=spots.map(s=>({turn:s.turn, bin:s.bin}));
      }
    }catch(e){ console.error(e); }
  } else { App.mothLamps=0; App.mothAt=[]; App.mothVary=[]; }
  R.disposeStatics();
  { const tg=performance.now(); R.setStatics(staticBatches);
    if(typeof LoadProf==='object') LoadProf.groupsMs=performance.now()-tg; }
  for(const [sys,mm,lamp,model] of pendingSystems) R.addParticleSystems(sys,mm,lamp,model);
  R.setPickables(pickables);
  /* Wraithguard: a record the conflict viewer sent here (45_wg_nav.js) - inspect it once
     it is on screen. */
  // Wraithguard: back from the mesh viewer (a container's contents) - the camera as it was.
  if(App._camAfter){ Object.assign(R.cam, App._camAfter); App._camAfter=null; R.dirty=true; }
  if(App._findOri){
    const k=App._findOri; App._findOri=null;
    const hit=pickables.find(p=>p.refKey===k);
    if(hit && typeof Ori==='object') setTimeout(()=>Ori.show(hit),0);
  }
  if(typeof LoadProf==='object' && timing.tBuild) LoadProf.build=performance.now()-timing.tBuild;
  /* Round 18bo: the pivot, once there is something to put it on.
   *
   * After a *frame*, not here. `focusOnView` asks what the middle of the view is looking
   * at, and the ground half of that answer (`Renderer.groundAt`) is read out of the depth
   * buffer — which still holds the scene being replaced until this one has been drawn once.
   * Measured, not assumed: called here it returned false every time and the pivot stayed
   * where WASD had left it, which is the bug this is fixing.
   *
   * Three frames' grace and then let it be: "nothing under the crosshair" is a real answer
   * — sky, or ground that did not load — and a retry with no end condition is worse than a
   * pivot that stayed put. The timer beside the frame request is the same belt-and-braces
   * `Busy.tick` uses: a page that is composited lazily can go seconds without a frame.
   */
  if(App._refocusOrbit){
    App._refocusOrbit=false;
    let tries=3;
    const put=()=>{
      if(R.nav!=='orbit' || !R.focusOnView) return;
      if(R.focusOnView()) return;
      if(--tries>0) soon(put);
    };
    const soon=fn=>{ let ran=false; const go=()=>{ if(ran) return; ran=true; fn(); };
                     requestAnimationFrame(go); setTimeout(go,60); };
    soon(put);
  }
  // Round 18f: the door's eye, out of whatever it landed in - against this scene's ground.
  settleDoorEye(R,(vx,vy)=>{
    const wx=vx+origin[0], wy=vy+origin[1], gx=Math.floor(wx/8192), gy=Math.floor(wy/8192);
    const c=cells.find(c=>c.kind!=='int' && c.gx===gx && c.gy===gy);
    return c? landHeightAt(c, wx-gx*8192, wy-gy*8192) : null;
  });
  // Round 17m: and the lamps, which the renderer turns on between 18:00 and 07:00.
  R.lights=cellLights;
  App.particleSystems=systemsPlaced;   // round 17w, for the report
  timing.objects=performance.now()-timing.t0-timing.cells-timing.terrain;
  if(typeof LoadProf==='object') timing.prof=LoadProf.take();   // round 18cr
  timing.meshes=App.meshCache.size-meshesBefore; timing.textures=App.texGL.size-texBefore;
  if(engineBefore && Engine.stats){ timing.calls=Engine.stats.calls-engineBefore.calls; timing.bytes=Engine.stats.bytes-engineBefore.bytes; }
  App.loadTimes=(App.loadTimes||[]).slice(-4); App.loadTimes.push(timing);
  /* Round 18cs: the connect log's "first scene" is the first cell scene to stand after
     the connect began, on the page's clock - the line was written when `openInstall`
     stopped waiting, which on a boot onto a remembered cell is before this ran at all
     (Robin's log said "first scene 0 ms"). The line's text is rewritten in place. */
  { const ct=App.connectTimes;
    if(ct && ct.tOpen && !ct.sceneDone){
      ct.sceneDone=true; ct.scene=performance.now()-ct.tOpen;
      const msOf=(v)=>(v==null? "?" : String(Math.round(v)));
      if(ct.line) ct.line.t=T('connect.log_page',{probe:msOf(ct.probe), open:msOf(ct.open), listing:msOf(ct.list), scene:msOf(ct.scene)});
    } }

  /* Water: an exterior's sea is always at zero. Drawn relative to the same origin
     everything else is, and cut to exactly the ground that was drawn.

     It used to be the cell count times a fudge factor — 1.2 cells for one cell, 2.2 for
     nine — so the sheet always overhung the landscape and the preview showed sea in
     places the cell simply does not extend to. Sized from the terrain bounds instead:
     one cell of ground gets one cell of water, nine get nine, and a half-cell patch
     gets half a cell. Nothing to keep in step, because there is one number. */
  /* Where that sheet is centred. Outdoors, the middle of the drawn terrain. Indoors there
     is no terrain and the room may sit anywhere its author put it, so it is the middle of
     the room — the sheet is enormous either way, but a sheet centred a hundred thousand
     units from the thing it is meant to be under is a sheet that runs out on one side. */
  R.waterCentre = ground? ground.c
                : (centre.kind==='int'? interiorCentre(centre) : [0,0]);
  /* Water.
   *
   * An exterior's sea is at zero and is cut to the ground that was drawn — a sheet the
   * size of the terrain, because that is all there is to tint.
   *
   * A room is different in both halves. Robin: "For interiors: Read the water height and
   * place the water plane at that height. If there is no water in the interior (data you
   * should be able to read) then don't render the water plane." — which the engine now
   * answers from the room's own record, `null` for a room that has none. And: "the
   * waterplane needs to be much bigger, as interiors can be placed in weird locations.
   * Try stretching it to the
   * horizon." Right, and for a reason worth writing down: an interior's coordinates are
   * whatever its author used, so a room can sit a hundred thousand units from the origin
   * — and a sheet sized to the room, centred where the terrain would have been, is water
   * you cannot find. There is no shoreline indoors for it to be wrong about. */
  const indoors = centre.kind==='int';
  R.setWater(centre.water, indoors? 1<<20 : (ground? ground.half : 4096));
  /* And the walls. A room's shell is single-sided with its normals pointing in, so the
     wall between you and the room has to go or there is nothing to see. */
  R.opts.cullStatics=indoors;
  /* And the brush changes worlds with the scene: indoors it anchors on the objects and
     carries a height, because there is no ground for it to mean anything without one.
     The "Paint on statics too" switch follows — forced on and out of reach in a room,
     your own setting put aside until you leave. */
  R.opts.paintFree=indoors;
  if(App.syncStaticsSwitch) App.syncStaticsSwitch(indoors);
  /* Round 17y: a room's own light. A room that is not flagged to behave like an exterior
     has no sky: it is lit by its AMBI record - an ambient, a "sunlight" colour the game
     shines from above (OpenMW's configureAmbient puts it at (-0.15, 0.15, 1)) - and the
     fog colour is what shows past its walls. The Atmosphere switch is put out of reach
     while such a room is up and given back on the way out. Robin: "Interiors should not
     have atmosphere and ONLY have ambient light unless they are tagged to behave like
     exteriors [...] Interiors tagged as exteriors should be lit exactly as exteriors." A
     room with no AMBI at all gets a flat grey, so it is at least visible. */
  const room = (indoors && !centre.quasi)
    ? (()=>{ const a=centre.ambi||{};
             const c=(v,d)=>Array.isArray(v)? [(+v[0]||0)/255,(+v[1]||0)/255,(+v[2]||0)/255] : d;
             return {ambient:c(a.ambient,[0.35,0.35,0.35]), sunlight:c(a.sunlight,[0.45,0.45,0.45]),
                     fog:c(a.fog,[0.12,0.12,0.14]), fogDensity:+a.fogDensity||0}; })()
    : null;
  R.opts.room=room;
  if(App.syncAtmosphereSwitch) App.syncAtmosphereSwitch(!!room);

  /* Four arrows just past the edges of the drawn ground, one cell per click. Walking a
     coastline is what people do with a cell preview, and doing it through the picker
     means reading the coordinates off the button, adding one, and typing them back.

     Only in Real cell: the simplified patch is one invented cell with nothing beside it,
     so there is nowhere to step to. And only for an exterior — an interior has no grid
     neighbours in any sense the picker could name. */
  if(App.mode==='cell' && ground && centre.kind!=='int'){
    const pad=Renderer.ARROW_PAD;
    const z=(centre.heights? CellData.heightAt(centre,4096,4096) : 0)+180;
    /* Back to the first size. The chunkier one was too much furniture in the middle of
       the view — the shape carries it, and the colour is what makes it legible. */
    const size=Math.min(320,Math.max(150,ground.half*0.09));
    const at=(dx,dy)=>[ground.c[0]+dx*(ground.half+pad), ground.c[1]+dy*(ground.half+pad), z];
    R.setArrows([
      {dir:'e', pos:at( 1, 0), size, label:'east'},
      {dir:'w', pos:at(-1, 0), size, label:'west'},
      {dir:'n', pos:at( 0, 1), size, label:'north'},
      {dir:'s', pos:at( 0,-1), size, label:'south'},
    ]);
    R.arrowFrom={x:centre.gx, y:centre.gy};
  } else {
    R.setArrows(null);
    R.arrowFrom=null;
  }

  timing.chunksKept=chunksKept; timing.pan=!!pan;
  return scatterIntoScene(R,cells,origin,centre,texAgg,texMissing,
                          {tris,drawn},sceneKey,ring,world,failed,
                          {fresh:true, pan, scene:{panKey, anchor, chunks, aim, cz}});
}

/** The cheap half: scatter grass into a scene that is already built and drawn.
    Runs on every config edit, so it never shows the busy overlay — at this size it
    finishes well inside a frame or two, and blocking the interface to announce that
    is worse than the wait. */
/** Wraithguard: the references of `groundcover=` plugins, drawn as grass.
 *
 *  They arrive flagged (`ref.gc`) in the cell data and are kept out of the placed objects;
 *  here they join the grass batches, one run per cell per mesh, the same shape the scatter
 *  hands over - so they get the grass renderer's instancing, wind (grass-folder meshes, as
 *  for any grass), distance fade, MGE grass light and ground tint. A placed reference is
 *  already where it stands, so there is no lift, and its tilt is its own rotation: the
 *  instance normal is the reference's up axis and the yaw is its z rotation.
 *
 *  Returns the triangles added, for the statistics box. */
async function addGroundcoverRuns(cells, origin, merged){
  let tris=0;
  const byModel=new Map();
  for(const c of cells){
    if(!c || !c.refs) continue;
    const off=cellOffsetFrom(origin,c);
    for(const ref of c.refs){
      if(!ref.gc) continue;
      const model=CellData.models.get((ref.id||'').toLowerCase());
      if(!model || isMarkerMesh(model)) continue;
      let per=byModel.get(model); if(!per){ per=new Map(); byModel.set(model,per); }
      const ck=c.gx+','+c.gy;
      let list=per.get(ck); if(!list){ list=[]; per.set(ck,list); }
      list.push({ref, x:ref.pos[0]-c.origin[0]+off[0], y:ref.pos[1]-c.origin[1]+off[1], z:ref.pos[2]});
    }
  }
  for(const [model, per] of byModel){
    const info=await loadMesh(model);
    if(!info || info.err || !info.parts.length) continue;
    const a=info.visAabb;
    const key='gc:'+model;
    let m=merged.get(key);
    if(!m){ m={key, info, runs:[], n:0, lift:0, align:true, wind:VFS.isGrass(model)}; merged.set(key,m); }
    for(const list of per.values()){
      const n=list.length;
      const pos=new Float32Array(n*3), rsa=new Float32Array(n*3), nrm=new Float32Array(n*3);
      let bx0=Infinity,by0=Infinity,bz0=Infinity,bx1=-Infinity,by1=-Infinity,bz1=-Infinity,sMax=1;
      list.forEach((e,k)=>{
        const r=e.ref.rot||[0,0,0], s=e.ref.scale||1;
        pos[k*3]=e.x; pos[k*3+1]=e.y; pos[k*3+2]=e.z;
        rsa[k*3]=-(r[2]||0); rsa[k*3+1]=s; rsa[k*3+2]=0;
        // The reference's up axis: refMatrix's third column with no scale.
        const M=refMatrix([0,0,0],[r[0]||0,r[1]||0,0],1);
        nrm[k*3]=M[2]; nrm[k*3+1]=M[6]; nrm[k*3+2]=M[10];
        if(e.x<bx0)bx0=e.x; if(e.x>bx1)bx1=e.x; if(e.y<by0)by0=e.y; if(e.y>by1)by1=e.y;
        if(e.z<bz0)bz0=e.z; if(e.z>bz1)bz1=e.z; if(s>sMax) sMax=s;
      });
      const reach=a? sMax*Math.max(Math.abs(a.x0),Math.abs(a.x1),Math.abs(a.y0),
                                   Math.abs(a.y1),Math.abs(a.z0),Math.abs(a.z1)) : 128*sMax;
      m.runs.push({inst:{pos,rsa,nrm,n}, box:[bx0-reach,by0-reach,bz0-reach,bx1+reach,by1+reach,bz1+reach]});
      m.n+=n;
      tris+=n*(info.tris||0);
    }
  }
  return tris;
}

async function scatterIntoScene(R,cells,origin,centre,texAgg,texMissing,
                                sceneStatics,sceneKey,ring,world,failed,how){
  /* Which install these cells were read from. Passed in rather than read here: by the
     time this runs, a rebuild that began before an install was swapped is holding cells
     from a world the engine has forgotten, and asking it about them is how "no
     landscape for that cell" ends up in the console. */
  if(world!==undefined && GameData.sig!==world){ Status.done(); return; }
  /* Round 18ac: and not while a connect is open. The signature says the new world, the
     engine still holds the old one, and the cell this rebuild is about may exist in
     neither - the preview patch is built into the world and a connect replaces the world,
     so a rebuild starting inside the window asks for cell (-900, -900) and is told there
     is no landscape there. It is the same shape as the mesh cache's poisoning (18ab), and
     the same window closes it: the connect ends by refreshing the page, which asks for
     this rebuild again from outside. */
  if(App.swapping){ Status.done(); return; }
  const s=activeSel();
  const target=App.cellSel||{};
  const sceneRec=App._scene={key:sceneKey,cells,origin,centre,texAgg,texMissing,
              statics:sceneStatics,ring:ring||[],world,failed:failed||[],
              /* Round 17y: what a step to the next cell reuses - the key without the
                 target, where the origin is anchored, the terrain chunks by cell, and the
                 camera's aim; `scatter` is filled in below with each cell's blades. */
              ...((how&&how.scene)||{}), scatter:new Map(), scatterSig:''};
  /* If the brush is set to land on rocks, this cell's meshes have to have been read
     before it can. Asked here, where the cell becomes the current one, rather than at the
     moment of the stroke: reading a couple of hundred NIFs in the middle of a drag would
     be a stutter with no cause a person could see. Costs nothing when the switch is off,
     and nothing the second time a cell is visited. */
  /* Only when the scene itself is new. The fast path runs on every stroke-end
     rescatter, and re-reading the meshes and rebuilding every object's tint there
     doubled work the stroke's own end already does — the cells on the renderer have
     not changed, so neither has what the brush can land on. */
  if(!how || how.fresh!==false){
    if(App.readySurfaces) App.readySurfaces();
    if(App.refreshStaticPaint) App.refreshStaticPaint();
  }
  const {tris,drawn}=sceneStatics;
  /* The engine's account of its own obstacle walk, summed over the loaded cells.

     The page walks the objects too, but only to draw them, and it is never sent a
     collision triangle — the engine is the only thing that tests against those. So
     what the panel reports about obstacles comes from the engine or from nowhere. */
  const census={};
  /* The grass already on screen stays there until there is new grass to put in its
     place. Freeing it up front emptied the viewport for the whole length of a rescatter
     — one engine call and a mesh fetch per cell — so every edit blanked the grass and
     then filled it back in, and a rescatter that was abandoned left it blank for good.
     Same rule as the statics above: build, then install, and let the replacement free
     what it replaces. */
  const batches=[];
  /* Which instance buffers each mesh part's batches get to write into.
     `R.buildBatch` keeps one set of GPU buffers per (part, hold) and writes the instances
     into them rather than making new ones, so `hold` has to be unique per *batch*. It used
     to be the rule key, which is unique per rule: nine cells drawing one rule all claimed
     the same buffers, each overwrote the last, and eight cells' grass was replaced by a
     ninth copy of one cell's. Numbering each part's batches in build order instead is
     unique, and — unlike naming the cell — bounded by what is on screen rather than by
     every cell you have ever looked at, so panning does not accumulate vertex arrays. */
  const gpuSeq=new Map();
  // Round 17v: one entry per rule slot, gathering every cell's blades for it (see below).
  const merged=new Map();
  const markerPos=[],markerRsa=[];
  const missPos=[],missRsa=[];
  const total={grid:0,placed:0,culledAngle:0,culledHeight:0,culledBuried:0,culledBan:0,
               culledTexture:0,culledStatic:0,culledClump:0,culledSpacing:0,
               culledPaint:0,culledRule:0,culledSlot:0,forced:0};
  let invisible=0,missGrass=0,idCount=0,emptyCount=0,grassTris=0;

  const texRows=Array.from(texAgg.entries()).sort((a,b)=>b[1]-a[1]).map(([id,cov])=>{
    const sels=selectorsForTexture(App.cfg,id,centre.name||'',
                                   (GameData.grid.get(centre.gx+','+centre.gy)||{}).region||'');
    const sel=sels[0]||null;
    return {id,coverage:cov,sel:sels.length?selKey(sels[0]):null,selObj:sel,selObjs:sels,
            noImage:texMissing.has(id),
            on:App.texOn[id]!==false && !!sel};
  });

  /* Which textures are actually growing something here decides whether a fading edge
     meets other grass or open ground. Asked of the rules, not of the row toggles: the
     per-texture switches are a way of looking at one texture at a time, and letting
     them change how a *different* texture fades would make the preview disagree with
     the export the moment one was flipped. */
  const grassy=new Set(texRows.filter(r=>r.selObj).map(r=>r.id.toLowerCase()));
  const hasGrass=t=>grassy.has(t);
  const byGrid=new Map();
  for(const c of cells.concat(ring||[])){ if(c.kind!=='int') byGrid.set(c.gx+','+c.gy,c); }
  const nbOf=c=>(dx,dy)=>byGrid.get((c.gx+dx)+','+(c.gy+dy))||null;

  /* Every cell asked for at once, and one line at the bottom of the window saying how
     far through it is.
   *
   * Asked one at a time, nine cells cost nine scatters end to end. The engine runs each
   * command on its own thread now and holds the state lock only long enough to take a
   * handle (§46), so nine asked together cost about what the slowest one does. The
   * *accumulation* below stays sequential and in cell order — it writes shared arrays,
   * and the seed the batches are keyed by has to be the same every time.
   *
   * No busy card here on purpose: this runs on every edit, and blocking the interface to
   * announce a wait that is usually two frames is worse than the wait. The bottom bar
   * costs nothing and is the only thing that says anything at all on the fast path,
   * which is the path a rescatter after a paint stroke takes. */
  const live=cells.filter(c=>c.kind!=='int'&&c.heights);
  const room=cells.find(c=>c.kind==='int')||null;
  const scatterable=live.length+(room?1:0);
  let scattered=0;
  // Round 18n: which ask this run is, for the engine's stop flag and the stale check
  // below - the run's own number, not the newest (see `_cellAsk`).
  const ask=_cellAsk|0;
  /* Pushed once before the fan-out rather than once per cell. It is a no-op when nothing
     has changed, but nine identical asks arriving together is still nine asks. */
  await syncConfig(true);
  if(world!==undefined && GameData.sig!==world){ Status.done(); return; }
  /* Round 17, Robin: the card sat on "Placing objects, 95%" for the whole scatter. When
     the card is up - a fresh load - it goes on through this step too, 85% to 100%; on
     the fast path (a paint stroke's rescatter) there is no card and the bottom bar is
     all that speaks, as before. */
  const card=(()=>{ const el=document.getElementById('busy'); return !!(el && !el.hidden); })();
  const progress=(sub,frac)=>{
    if(card) Busy.show(T('busy.scattering'), sub, 0.85+0.13*frac);
    else Status.work(sub? T('busy.scattering_sub',{sub}) : T('busy.scattering'), frac);
  };
  progress(scatterable>1? T('busy.n_of',{done:0, total:scatterable}) : '', 0);
  if(card) await Busy.tick();
  /* Round 17y: a cell's blades from the last scatter, kept on the scene, are the right
     blades still when the centre steps to a neighbour - same origin, same rules, same
     paint - so only the cells that came into view are asked for. The signature is
     everything the engine's answer depends on that the scene key does not already carry:
     the rules as last pushed, the paint's revision, the seed, the switched-off textures
     and rules, the grass height. Anything else that changes the answer goes through a
     rebuild of its own, which rescatters every cell and refills this. */
  const scatterSig=JSON.stringify([App.seed, App.avoidHeight||96, typeof _syncedCfg==='string'? _syncedCfg.length+':'+hashStr(_syncedCfg) : '',
    App.paintRev|0, Object.keys(App.texOn||{}).filter(k=>App.texOn[k]===false).sort(), Object.keys(App.ruleOff||{}).filter(k=>App.ruleOff[k]).sort(),
    origin[0], origin[1]]);
  const kept=(how && how.pan && how.pan.scatter && how.pan.scatterSig===scatterSig)? how.pan.scatter : null;
  /* `sceneRec` is the record made above, held in hand since it was made — not whatever
     `App._scene` is when an answer lands. A scene can be torn down while its cells are
     still being asked about, and reading the field here meant reading a `null` that a
     connect had just put there: "Cannot set properties of null (setting 'scatterSig')",
     from a rebuild whose results were going to be thrown away anyway (round 18ab). */
  let reusedCells=0;
  const runs=live.map(c=>{
    const off=cellOffsetFrom(origin,c);
    const had=kept? kept.get(c.gx+','+c.gy) : null;
    if(had){ reusedCells++; scattered++; sceneRec.scatter.set(c.gx+','+c.gy,had); return Promise.resolve({c, off, res:had}); }
    return CellScatter.cell(c,{seed:App.seed,
      grassHeight:App.avoidHeight||96, offset:off,
      neighbour:nbOf(c), hasGrass, origin, ask}).then(res=>{
        scattered++;
        progress(scatterable>1? T('busy.cell_n_of',{done:scattered, total:scatterable}) : '', scattered/scatterable);
        sceneRec.scatter.set(c.gx+','+c.gy,res);
        return {c, off, res};
      }).catch(e=>{
        /* Round 18n: overtaken. Answered as a marker rather than thrown, so the other
           cells' promises are still awaited and never surface as unhandled rejections. */
        if(e && e.superseded) return {c, off, res:null, superseded:true};
        /* Round 18ag: overtaken by a connect. The guard at the top of this function
           catches a rebuild that *starts* inside the swap window; this catches one that
           started just before it and whose ask landed on the new world - "no landscape
           for that cell", from a cell the old install had and the new one does not. The
           answer is the same as 18n's: this rebuild's results were going to be thrown
           away, and the connect asks for a fresh one on its way out. It surfaced once in
           a sweep as a page error on `t_connect`, and the earlier "unexplained" ones on
           `t_export`, `t_layered` and `t_arrows` wore the same coat. */
        if(App.swapping || (world!==undefined && GameData.sig!==world)) return {c, off, res:null, superseded:true};
        throw e;
      });
  });
  sceneRec.scatterSig=scatterSig;
  if(App.loadTimes && App.loadTimes.length){ const lt=App.loadTimes[App.loadTimes.length-1]; if(how && how.fresh) lt.scatterKept=reusedCells; }
  /* A room scatters too — on its painted statics, the only ground it has. Same list,
     same installer below; its blades already arrive relative to the scene origin. */
  if(room) runs.push(CellScatter.interior(room,{origin}).then(res=>{
    scattered++;
    return {c:room, off:[0,0], res};
  }));
  const scattered_all=await Promise.all(runs);
  /* Round 18n: overtaken. Nothing goes on screen from this ask - the newer one is queued
     behind this rebuild and will scatter with the state as it is now. */
  if(scattered_all.some(r=>r&&r.superseded)){ Status.done(); return; }
  // The install underneath can be replaced while all of that is in flight.
  if(world!==undefined && GameData.sig!==world){ Status.done(); return; }
  // Round 18n: and the page can have asked again while all of that was in flight -
  // building batches for grass that is about to be replaced is the wait Robin felt.
  if(typeof previewSuperseded==='function' && previewSuperseded(ask)){ Status.done(); return; }
  if(card){ Busy.show(T('busy.building_grass'),'',0.98); await Busy.tick(); }
  /* Round 18av: a card naming an object from the load order draws that object's own
     model - a rock, a totem, a creature in its idle - the way the engine resolved it;
     a leveled creature has none and keeps the marker. Asked for once, before the groups
     are walked, so the answer is there when a group needs it. */
  for(const {c, off, res} of scattered_all){
    for(const k in total) if(res.stats[k]!=null) total[k]+=res.stats[k];
    for(const k of CENSUS_KEYS) if(res.stats[k]!=null) census[k]=(census[k]||0)+res.stats[k];
    for(const g of res.groups){
      const sl=g.slotObj, inst=g.inst;
      if(!inst.n) continue;
      const objModel='';   // references by id were a grass-generation feature
      if(sl.empty||(sl.id && !objModel)){
        if(sl.empty) emptyCount+=inst.n; else idCount+=inst.n;
        for(let k=0;k<inst.n;k++){
          markerPos.push(inst.pos[k*3],inst.pos[k*3+1],inst.pos[k*3+2]);
          markerRsa.push(inst.rsa[k*3],Math.max(6,inst.rsa[k*3+1]*10),0);
        } continue; }
      const info=await loadMesh(sl.id? objModel : sl.mesh);
      if(!info||info.err){
        missGrass+=inst.n;
        /* Marked rather than skipped. A rule whose mesh is not installed used to look
           exactly like a rule placing nothing at all, and the only sign was a number in
           the statistics panel you had to know to read. */
        for(let k=0;k<inst.n;k++){
          missPos.push(inst.pos[k*3],inst.pos[k*3+1],inst.pos[k*3+2]);
          missRsa.push(inst.rsa[k*3],Math.max(8,inst.rsa[k*3+1]*12),0);
        }
        continue;
      }
      if(!info.parts.length){ invisible+=inst.n; continue; }
      grassTris+=inst.n*info.tris;
      const key=g.sel+':'+g.slot;
      /* Round 17u: where this group of blades is, so the renderer can leave it out when it
         is off screen. Nothing culled the grass before — and on Robin's 49-cell Balmora
         that was 15.2 million triangles submitted every frame whichever way he was facing,
         against 9.3 million of statics that *were* culled.
         The blades' own positions, grown by how far the mesh reaches from its origin at
         the largest scale any of them was given. */
      let bx0=Infinity,by0=Infinity,bz0=Infinity,bx1=-Infinity,by1=-Infinity,bz1=-Infinity,sMax=1;
      for(let k=0;k<inst.n;k++){
        const x=inst.pos[k*3], y=inst.pos[k*3+1], z=inst.pos[k*3+2];
        if(x<bx0)bx0=x; if(x>bx1)bx1=x;
        if(y<by0)by0=y; if(y>by1)by1=y;
        if(z<bz0)bz0=z; if(z>bz1)bz1=z;
        const sc=inst.rsa[k*3+1]; if(sc>sMax) sMax=sc;
      }
      const a=info.visAabb;
      const reach=a? sMax*Math.max(Math.abs(a.x0),Math.abs(a.x1),Math.abs(a.y0),
                                   Math.abs(a.y1),Math.abs(a.z0),Math.abs(a.z1)) : 128*sMax;
      const box=[bx0-reach,by0-reach,bz0-reach,bx1+reach,by1+reach,bz1+reach];
      /* Round 17v: gathered per rule slot across every cell, not built per cell.
         A batch was one slot's blades in one cell, so the same tuft of grass was 49
         batches on Robin's 49-cell frame — 3,472 of them, each a draw call, and at 3.6 us
         a call that was a good part of his 25 ms. The objects were consolidated this way
         in 17r: one instance buffer per mesh, and a run per cell inside it that the
         renderer can test and skip on its own. Same shape here. */
      let m=merged.get(key);
      if(!m){
        /* Round 18d: how far above the ground this rule stands its grass - the engine's
           lift, GG_LIFT (16) plus the rule's own height offset - so the tint can find the
           terrain in the mesh's own space (VS_GRASS, uBlade.z). */
        /* `g.sel` is the rule's index (see `CellScatter.cell`), not its id. 18d looked
           it up by id, found nothing, and every batch's lift was the bare 16 whatever
           the rule's offset said - only the tint's ground-finding noticed. */
        const selObj=(App.cfg&&App.cfg.selectors||[])[g.sel]||null;
        /* Round 18o: the card's own offset and tilt where it states them - the same
           answer `Selector::lift_for` and `aligned` give the plugin, so the viewport and
           the .esp agree per card and not only per rule. The tilt used to be one switch
           for the whole scene, read off the *active* rule; it is the batch's now. */
        const zo=(sl && sl.zOff!=null)? +sl.zOff : (+((selObj&&selObj.b&&selObj.b.iZPositionModifier))||0);
        const lift=16+zo;
        // Round 18ax: a creature or a leveled list stands up whatever the card or the rule says.
        const align=(sl && sl.align!=null)? !!sl.align
                  : selObj? !!(selObj.b&&selObj.b.bAlignObjectNormalToGround) : null;
        /* Round 18aw: only a grass mesh leans in the wind - an object placed by id (a
           rock, a creature) and a mesh from any other folder stand still. Robin: "The
           only object that should have the grass wind shader effect are grass meshes
           from the grass folder." */
        const wind=!sl.id && VFS.isGrass(sl.mesh);
        m={key, info, runs:[], n:0, lift, align, wind}; merged.set(key,m);
      }
      m.runs.push({inst, box});
      m.n+=inst.n;
    }
  }
  /* One batch per (slot, part), the cells' blades laid end to end in one set of streams,
     with each cell's run remembered — where it starts, how many, and its box — so the
     renderer can draw the whole thing in one call when every cell is in view and only
     the cells that are when they are not. The box on the batch is the union, for the
     coarse test the renderer makes first. */
  grassTris+=await addGroundcoverRuns(cells, origin, merged);
  for(const m of merged.values()){
    const pos=new Float32Array(m.n*3), rsa=new Float32Array(m.n*3), nrm=new Float32Array(m.n*3);
    const groups=[];
    let at=0;
    let ux0=Infinity,uy0=Infinity,uz0=Infinity,ux1=-Infinity,uy1=-Infinity,uz1=-Infinity;
    for(const r of m.runs){
      const i=r.inst, n=i.n;
      pos.set(i.pos.subarray(0,n*3),at*3);
      rsa.set(i.rsa.subarray(0,n*3),at*3);
      nrm.set(i.nrm.subarray(0,n*3),at*3);
      groups.push({at, n, b:r.box});
      at+=n;
      const b=r.box;
      if(b[0]<ux0)ux0=b[0]; if(b[1]<uy0)uy0=b[1]; if(b[2]<uz0)uz0=b[2];
      if(b[3]>ux1)ux1=b[3]; if(b[4]>uy1)uy1=b[4]; if(b[5]>uz1)uz1=b[5];
    }
    const inst={pos,rsa,nrm,n:m.n};
    for(const p of m.info.parts){
      const seq=gpuSeq.get(p)||0; gpuSeq.set(p,seq+1);
      const bt=R.buildBatch(p,inst,p.glTex?p.glTex.gl:null,m.key,seq);
      bt.box=[ux0,uy0,uz0,ux1,uy1,uz1];
      bt.groups=groups;
      bt.lift=m.lift;   // round 18d
      bt.align=m.align; // round 18o: the card's tilt, or the rule's
      bt.wind=m.wind;   // round 18aw: grass meshes only
      batches.push(bt);
    }
  }
  R.disposeBatches();
  R.setBatches(batches);
  R.setMarkers(markerPos.length?{pos:new Float32Array(markerPos),rsa:new Float32Array(markerRsa),
                                 n:markerPos.length/3}:null);
  R.setMissing(missPos.length?{pos:new Float32Array(missPos),rsa:new Float32Array(missRsa),
                               n:missPos.length/3}:null);
  renderCellTextureList(texRows);

  if(s){
    R.opts.maxAngle=s.b.fMaximumAngle!=null?s.b.fMaximumAngle:90;
    R.opts.minH=s.b.fMinHeight!=null?s.b.fMinHeight:-1e9;
    R.opts.maxH=s.b.fMaxHeight!=null?s.b.fMaxHeight:1e9;
    R.opts.align=!!s.b.bAlignObjectNormalToGround;
  } else {
    /* No rule, so nothing rejects any ground. Left at the departed rule's numbers, the
       red overlay went on tinting a hillside for a slope limit that no longer existed
       anywhere in the config. */
    R.opts.maxAngle=90; R.opts.minH=-1e9; R.opts.maxH=1e9; R.opts.align=false;
  }
  /* Round 18q: and every other gate the mask draws, for the rule as it stands. The field
     behind three of them is asked for after the scene is up, since it is asked for the
     cells that are actually on screen; it answers in the background and the frame it
     lands on is the one that shows it. */
  if(typeof syncCullMask==='function') syncCullMask(null);
  if(typeof refreshCullField==='function') Promise.resolve().then(refreshCullField).catch(e=>console.error(e));

  const totalRefs=cells.reduce((a,c)=>a+c.refs.length,0);
  const prov={sources:new Map(),replaced:0,deleted:0,movedOut:0,movedIn:0,unresolved:0};
  for(const c of cells){
    const p=c.prov; if(!p) continue;
    prov.replaced+=p.replaced; prov.deleted+=p.deleted;
    prov.movedOut+=p.movedOut; prov.movedIn+=p.movedIn; prov.unresolved+=p.unresolved;
    for(const [k,v] of p.sources) prov.sources.set(k,(prov.sources.get(k)||0)+v);
  }
  /* What the cell holds, for the statistics box. */
  const statics = {
    placed:census.objects||0, missing:census.meshMissing||0,
    unresolved:census.unresolved||0, tris, drawn,
    corpses:census.corpses||0, npcMarkers:census.npcMarkers||0,
    aliveSkipped:census.aliveSkipped||0, markerFiltered:census.markerFiltered||0,
  };
  /* The engine's own account of what it turned away, kept where it can be read rather
     than only rendered into the panel. Differences between two whole-cell blade counts
     are a poor way to ask whether a setting did anything — the sampler refills the ground
     a rejection frees — so anything checking that a setting bites should read the count
     of candidates that setting rejected. */
  App.lastStats=total;
  drawCellStats({prov, cell:{refs:{length:totalRefs}},cellName:(target.label||target.name||''),
                 cells:cells.length, stats:total, grassTris, statics,
                 invisible,missGrass,idCount,emptyCount,textures:texRows.length,
                 /* A room's water, and who decided it. Only for rooms: an exterior's sea
                    is at zero by the format and there is nothing to explain. */
                 water:centre.kind==='int'? centre : null});
  const noLand=cells.filter(c=>c.kind==='ext'&&!c.heights).length;
  /* The two things worth saying that the retired "Objects by plugin" panel used to carry.
     Neither is a total: one is a plugin in the order that could not be read at all, and
     the other is a reference naming a master nobody loaded — which means its identity
     cannot be resolved, so it may end up duplicated. Both belong with the rest of what
     went wrong with this load rather than under a heading about counts. */
  const unread=(CellData.loadWarnings||[]);
  const bad = (failed&&failed.length) || unread.length || prov.unresolved;
  setCellStatus(T('cell.status_loaded',{name:target.label||target.name,
                                        cells:T('cell.n_cells',{n:cells.length}),
                                        refs:T('cell.n_references',{n:totalRefs})})+
    (target.kind==='int'? ' · '+T('cell.status_interior')
                        : (noLand? ' · '+T('cell.status_no_land',{n:noLand}):''))+
    (failed&&failed.length
       ? ' · '+T('cell.status_failed',{n:failed.length})
       : '')+
    (unread.length
       ? ' · '+T('cell.status_unread',{n:unread.length, list:unread.join('; ')})
       : '')+
    (prov.unresolved
       ? ' · '+T('cell.status_unresolved',{n:prov.unresolved})
       : ''),
    bad? 'warn' : 'ok');
  if(failed&&failed.length) console.warn('cells that would not load:', failed);
  Busy.hide();
}

/* Put the camera back where a freshly loaded view starts, in whichever mode is active. */
function recentreView(force){
  const R=App.R; if(!R) return;
  /* Whichever mode is showing, the framing that put the camera there is the framing to
     put it back — the simplified patch goes through the same cell path as everything
     else, and it sits at whatever height "Ground height" was set to, so recentring it on
     z = 0 aimed the eye under the hill. */
  if(App.inspect && App._meshFrame){
    /* Round 18az: the mesh editor. Its framing is the mesh's own (`inspectMesh`), at the
       origin; the cell's `_frameInfo` is still around underneath and used to win here,
       which aimed the camera at the middle of a cell that is not on the screen. Robin:
       "Recentre in the mesh editor doesn't put the camera back where it was when
       loading the mesh editor." */
    R.frameAt(App._meshFrame.span, App._meshFrame.cz);
  } else if(App._frameInfo){
    // The aim as well as the height: recentring a room on the origin would put the camera
    // wherever nothing is.
    const a=App._frameInfo.aim||[0,0];
    R.frameAt(App._frameInfo.span, App._frameInfo.cz, a[0], a[1]);
  } else {
    R.frame(2048);
  }
  R.dirty=true;
  if(force) toast(T('vp.recentred'),'ok',1500);
}

function drawCellStats(d){
  const grassTris=d.grassTris;
  /* A room's water, said out loud — the number on the row, and *whose* it is in the
     row's tooltip. The level and the flag routinely come from different plugins, so a
     room drawn flooded when it should be dry is a question about a load order rather
     than about this program; the hover answers it without spending two rows on every
     cell that has water. Exteriors have their sea at zero by the format and get no
     row. */
  const water=d.water? [
    [T('cell.st_water'), d.water.water==null? T('cell.st_water_none') : T('cell.st_water_units',{n:Math.round(d.water.water)}),
     T('cell.st_water_tip',{level:d.water.waterLevelFrom||T('cell.st_water_nobody'),
                            flag:d.water.waterFlagFrom||'\u2014'})],
  ] : [];
  /* The box holds the numbers that describe this cell; the accounting that explains
     them is shown only when it has something to say. Robin: "reduce the amount of data
     presented ... some is less interesting than others" — and a row reading zero is the
     least interesting row there is. `opt` rows appear when non-zero and disappear when
     not; the always-rows are the shape of the cell. A tooltip on the parent row carries
     what its sub-rows used to spell out. */
  const opt=(label,n,tip,err)=> (n|0)>0? [[label,n,tip,'',err]] : [];
  const sub=s=>'&nbsp;&nbsp;'+s;          // an indented sub-row of the row above it
  const rows=[
    /* The name is trimmed from the middle — both ends of a long cell name identify it,
       the middle rarely does — with the whole of it on the hover. Round 18cy: a sixth
       element marks it as the cell row, which carries the star (see below). */
    [T('cell.st_cell'), escHtml(midTrim(d.cellName,30)), d.cellName.length>30? d.cellName : '', '', false, 'cell'],
    ...water,
    [T('cell.st_cells_loaded'), d.cells],
    [T('cell.st_textures'), d.textures],
    [T('cell.st_references'), d.cell.refs.length],
    ...opt(sub(T('cell.st_overridden')), d.prov?d.prov.replaced:0),
    ...opt(sub(T('cell.st_deleted')), d.prov?d.prov.deleted:0),
    ...opt(sub(T('cell.st_moved_out')), d.prov?d.prov.movedOut:0),
    ...opt(sub(T('cell.st_moved_in')), d.prov?d.prov.movedIn:0),
    /* "found", not "drawn": the objects are read whenever they are drawn or avoided,
       so with Draw statics off this count stays right while nothing is on screen. */
    [T('cell.st_objects'), d.statics.placed],
    ...opt(sub(T('cell.st_no_mesh_record')), d.statics.unresolved, '', true),
    ...opt(sub(T('cell.st_mesh_missing')), d.statics.missing, '', true),
    ...opt(sub(T('cell.st_editor_markers')), d.statics.markerFiltered),
    ...opt(T('cell.st_corpses'), d.statics.corpses),
    ...opt(sub(T('cell.st_npc_markers')), d.statics.npcMarkers),
    ...opt(T('cell.st_actors_skipped'), d.statics.aliveSkipped),
    /* Round 18r: the moths, whenever the mod is installed - **including none**, which is
       the answer that was missing. Robin: "Moths are not visible in the scene no matter
       the time of day", and there was nowhere to look to tell "the mod is not found" from
       "no lantern in this cell is one it names" from "they are out but it is two in the
       afternoon". Not an `opt`: a zero here is the informative case. */
    ...(typeof Moths==='object' && Moths.installed
        ? [[T('cell.st_moths'), App.mothLamps||0,
            T('cell.st_moths_tip',{from:Moths.from.replace(/^t/,'T'), meshes:Moths.lamps.size,
                                    lamps:App.mothLamps||0})]]
        : []),
    null,
    [T('cell.st_triangles'), T('cell.st_triangles_v',{grass:nf(grassTris), statics:nf(d.statics.tris)})],
  ];
  /* Round 18cy: the heading row carries the visit history's two arrows, in Real cell mode
     only (Robin: "Only show these buttons when in the Real Cell Preview mode"), and the
     cell row a star that is the picker's own - filled while the cell is starred, an
     outline otherwise, and a click that stars it for the list and the map alike. The box
     used to open the report on a click; the Report button beside it does that, and a box
     that swallowed every click could not carry either. */
  const real=App.mode==='cell';
  let html='<div class="ttl"><span>'+T('cell.stats_title')+'</span>'+
    (real && typeof CellHistory==='object'
      ? '<span class="hist"><button id="stHistBack" disabled>◀</button><button id="stHistFwd" disabled>▶</button></span>'
      : '')+'</div>';
  const starred = real && App.cellSel && (App.cellSel.kind==='ext' || App.cellSel.kind==='int') && typeof Favourites==='object';
  for(const r of rows){
    if(!r){ html+='<hr>'; continue; }
    let cls='';
    // A fifth element marks the rows that read as a fault when non-zero.
    if(r[4] && parseInt(r[1],10)>0) cls=' e';
    /* A third element is a tooltip. `#stats` sets `pointer-events:auto`, so `title`
       actually reaches the pointer here — inside `#vpover` it would not, which is how
       the "Always on in interiors" tooltip was silently swallowed once already. */
    const tip=r[2]? ' title="'+escHtml(r[2])+'"' : '';
    // A fourth element names the value span, for the rows something updates in place.
    const id=r[3]? ' id="'+r[3]+'"' : '';
    // The star stands between the label and the name, at the name's side (round 18cy).
    const star=(r[5]==='cell' && starred)? '<span class="fav" id="stFav"></span>' : '';
    html+='<div'+tip+'><span class="k">'+r[0]+'</span>'+star+'<span class="v'+cls+'"'+id+'>'
         +r[1]+'</span></div>';
  }
  $('#stats').innerHTML=html;
  if(real && typeof CellHistory==='object') CellHistory.sync();
  if(starred) paintStatsStar();
}

/** The star on the statistics box's cell row (round 18cy): the loaded cell's, as the
    picker files it (`cellFavKey`), filled or an outline, toggled by a click. */
function paintStatsStar(){
  const el=$('#stFav'), sel=App.cellSel; if(!el || !sel) return;
  const key=cellFavKey(sel);
  const paint=()=>{
    const on=Favourites.has('cell',key);
    el.textContent=on? '★' : '☆';
    el.dataset.on=on? '1' : '';
    el.title=T(on? 'browse.star_on_title' : 'browse.star_off_title');
  };
  el.onclick=e=>{ e.stopPropagation(); e.preventDefault(); Favourites.toggle('cell',key); paint(); };
  paint();
}

/** What to call a load target in a message, or the loaded cell when asked about none.
 *
 *  One function where there were two: a second `cellLabel()` taking no argument was
 *  declared further down this file, and being the later declaration it *was* the one
 *  every caller got — so the failure line naming which neighbour would not load named
 *  the cell you were looking at instead, whichever neighbour it was. */
/** The rings of neighbours to load: 0..3. `true` (older profiles, tests) is one ring. */
function adjacentRings(){
  const v=App.showAdjacent;
  return v===true? 1 : Math.max(0, Math.min(3, (+v)||0));
}

function cellLabel(t){
  t = t || App.cellSel;
  if(!t) return '<none>';
  return t.kind==='int' ? (t.name||'interior')
                        : (t.label || t.name || ('('+t.x+', '+t.y+')'));
}

function setCellStatus(msg,kind){
  const el=$('#cellStatus'); if(!el) return;
  el.textContent=msg;
  el.className='hint'+(kind==='ok'?' ok':'');
  el.style.color = kind==='warn'? 'var(--warn)' : (kind==='ok'? 'var(--ac)':'var(--tx3)');
}



/* =====================================================================================
   Nocturnal Moths — round 18q

   Robin: "Identify if I have the mod Nocturnal Moths installed, and if I have, play the
   correct effect around lamps (not torches, candles or braziers, just lamps) during night
   times. Use the same time as night lights, even if the lights are always on, turn on the
   moths only during the same times."

   The mod is `MWSE - Nocturnal Moths`, and its effect is not a script: it is a mesh,
   `R0\\e\\moths_lntrn.NIF`, attached to a lamp's `AttachLight` node (or to the reference's
   origin where there is none), with a keyframe path per moth. So Gardenfell draws exactly
   what the game draws, by placing that mesh where the mod places it — no invented effect,
   nothing to keep in step with a mod that may be updated.

   **Which lamps** is the mod's own answer, not a copy of it: the whitelist lives in
   `MWSE\\mods\\Nocturnal Moths\\data\\init.lua` as a table of mesh paths, which is a
   simple enough shape to read, so the list follows the mod. The fifteen it ships with are
   kept here only for the case where that file cannot be read — the mesh being present is
   what says the mod is installed.

   **When**: the night window of the Lights picker's "Night time", whatever the picker is
   actually set to (Robin: "even if the lights are always on"). And the mod's own weather
   rule — moths in clear, cloudy, foggy and overcast, none in rain, ash, blight, storm or
   snow. It carries no switch of its own: the Particles toggle is what turns effects like
   this on and off, and Robin asked for it there.

   Not in a room. The mod attaches moths only where the cell `isOrBehavesAsExterior`,
   which is the same question `roomAlwaysOn` answers here.
   ===================================================================================== */
/* =====================================================================================
   Round 18q: the field the cull mask samples.

   Three of the things that cull grass cannot be answered from a fragment: the ground's
   curvature (it needs the heights around the point), the distance to the nearest object
   grass keeps clear of (it needs the obstacle index), and what hangs over the spot (it
   needs the world's meshes). The engine answers all three on a grid, per cell, with the
   same `Ground`, `Obstructions` and `Canopies` the scatter uses - so the mask cannot
   drift from the placement.

   What comes back is the *quantities*, not a verdict, so hovering a card - which changes
   only the bands - costs nothing. One texture over the cells on screen, rebuilt when the
   scene changes or when the rule's curvature radius or the reach of the bands does.
   ===================================================================================== */
const CULL_RES=96;          // samples across a cell: 85 units a texel, sampled smoothly
async function refreshCullField(){
  // Generated grass's cull field; there is no generated grass any more.
  const R=App.R; if(!R) return;
  R.setCullField(null); App._cullField=null;
  return;
  const c=R.opts.cull;
  if(!c || !cullWantsField(c) || !Engine.has()){ R.setCullField(null); App._cullField=null; return; }
  const sc=App._scene;
  const cells=(sc && sc.cells)? sc.cells.filter(x=>x.kind!=='int' && x.heights) : [];
  if(!cells.length){ R.setCullField(null); App._cullField=null; return; }
  const NO=1e8;
  /* How far the distance channel has to reach, and at what radius a hollow is a hollow -
     both the rule's, both part of what the field means, so a change to either rebuilds it. */
  const reach=Math.max(1, c.obs[1]<NO? c.obs[1] : (c.obs[0]>-NO? c.obs[0]*2 : 0)) ;
  const T=(activeSel()||{}).terrain||TERRAIN_DEF;
  const radius=(c.curve[0]>-NO || c.curve[1]<NO)? (+T.curveRadius||512) : 0;
  const gxs=cells.map(x=>x.gx), gys=cells.map(x=>x.gy);
  const g0=[Math.min(...gxs), Math.min(...gys)], g1=[Math.max(...gxs), Math.max(...gys)];
  const key=JSON.stringify([GameData.sig||'', App.paintRev|0, App.seed, App.avoidStatics, App.avoidPad,
                            App.avoidMinSize, (App.avoidExclude||[]).join('|'),
                            (App.cfg.canopies||[]).map(k=>k.id+':'+(k.match||[]).join(',')).join('|'),
                            g0, g1, Math.round(reach), Math.round(radius),
                            cells.map(x=>x.gx+','+x.gy).sort().join(';')]);
  if(App._cullField===key) return;
  App._cullField=key;
  const W=(g1[0]-g0[0]+1)*CULL_RES, H=(g1[1]-g0[1]+1)*CULL_RES;
  const data=new Uint8Array(W*H*3);
  // 255 everywhere is "as far from anything as anyone looked"; 128 is flat; 0 is nothing
  // overhead. Ground the field does not cover reads as refusing nothing.
  for(let i=0;i<data.length;i+=3){ data[i]=255; data[i+1]=128; data[i+2]=0; }
  const world=GameData.sig;
  for(const cell of cells){
    let buf;
    try{ buf=await Engine.bytes('cull_field',{gx:cell.gx, gy:cell.gy, res:CULL_RES, radius, reach}); }
    catch(e){ continue; }
    if(GameData.sig!==world || App._cullField!==key) return;   // the world, or the ask, moved on
    const src=new Uint8Array(buf);
    if(src.length<CULL_RES*CULL_RES*3) continue;
    const ox=(cell.gx-g0[0])*CULL_RES, oy=(cell.gy-g0[1])*CULL_RES;
    for(let j=0;j<CULL_RES;j++){
      const dst=((oy+j)*W+ox)*3;
      data.set(src.subarray(j*CULL_RES*3,(j+1)*CULL_RES*3), dst);
    }
  }
  if(App._cullField!==key) return;
  /* Where it sits in the scene: the cells' own square, less the origin everything else on
     the renderer is drawn relative to. The last texel of a cell is the first of the next,
     so the box runs to the far cell's far edge. */
  const o=(sc && sc.origin)||[0,0];
  R.setCullField({data, w:W, h:H, reach,
                  box:[g0[0]*8192-o[0], g0[1]*8192-o[1],
                       (g1[0]+1)*8192-o[0], (g1[1]+1)*8192-o[1]]});
}

/* Round 18r: how many (speed, phase) pairs the moths are dealt into. One batch per shape
   per pair, so a cell's whole moth population costs `parts x MOTH_PHASES` draw calls
   however many lanterns are in it. Four is enough that neighbours never beat together and
   few enough that the cost does not show. */
const MOTH_PHASES=4;
/* The staggering hash `lampHours` uses, said once. A number in [0,1) from a position and a
   salt: the same every frame, the same after a reload, the same for everyone. */
const frac2=(a,b,c)=>{ const v=Math.sin(a*12.9898+b*78.233+c)*43758.5453; return v-Math.floor(v); };
/** A copy of a shape's animation running at `speed` and started `phase` of a cycle in.
 *
 *  The controller's own `freq` and `phase` are what `ctrlTime` reads, so retiming is a
 *  copy of the chain with two numbers changed - no second evaluation, no shader change,
 *  and the geometry and the links above it are shared. `phase` is in cycles, so it means
 *  the same thing whatever the clip's length. Null in, null out: a still shape stays still.
 */
/** Round 17r's cell runs for an instance list: the instances are sorted by cell, so each
 *  cell's are contiguous and a run of visible cells is one draw at an offset. `list[k]`
 *  stands at `m[k*12..]`. */
function cellRuns(list, m, visAabb){
  const groups=[];
  for(let k=0;k<list.length;k++){
    const g=groups[groups.length-1];
    const box=worldAabb(visAabb, m.subarray(k*12,k*12+12));
    if(g && g.key===list[k].cellKey){
      g.n++;
      if(box.x0<g.b[0])g.b[0]=box.x0; if(box.y0<g.b[1])g.b[1]=box.y0; if(box.z0<g.b[2])g.b[2]=box.z0;
      if(box.x1>g.b[3])g.b[3]=box.x1; if(box.y1>g.b[4])g.b[4]=box.y1; if(box.z1>g.b[5])g.b[5]=box.z1;
    }else{
      groups.push({key:list[k].cellKey, at:k, n:1,
                   b:[box.x0,box.y0,box.z0,box.x1,box.y1,box.z1]});
    }
  }
  return groups;
}

/** Round 18ai: how long one loop of a mesh's animation is - the longest clip among its
 *  parts' links and its bones' - so a phase shift can be a fraction of it. */
function animSpan(info){
  let span=0;
  const links=L=>{ for(const l of (L||[])) span=Math.max(span, (+l.stop||0)-(+l.start||0)); };
  for(const p of info.parts||[]){
    if(p.anim) links(p.anim.nodes);
    if(p.skin) for(const b of p.skin.bones||[]) links(b.chain);
  }
  return span;
}

/** Round 18ai: the instances of a mesh dealt into phase bins - `[{all:true}]` for a mesh
 *  that does not move, else up to eight bins of `{idx, shift, speed}` with the instances
 *  shared out evenly and each bin's shift, in seconds into the clip, taken from where its
 *  first instance stands. Sorted by that same hash first, so which instances share a bin
 *  is as fixed as their positions.
 *
 *  Round 18aj, Robin: "randomize speed of animation playback per banner at 80 to 90% of
 *  original speed." Each bin plays at its own rate in that range, from a second hash of
 *  the same spot - so the same banner is the same banner tomorrow - and never faster
 *  than the file: a calmer street, not a busier one. */
const PHASE_BINS=8;
const SPEED_LO=0.8, SPEED_HI=0.9;
function phaseBins(list, info){
  if(!info.animated || !list.length) return [{all:true}];
  const span=animSpan(info);
  if(!(span>1e-6)) return [{all:true}];
  const at=r=>(r.ref&&r.ref.pos)||[0,0,0];
  const hash=r=>{ const p=at(r); return frac2(p[0]*0.01, p[1]*0.01, 5.7); };
  const order=list.map((r,k)=>({k, h:hash(r)})).sort((a,b)=>a.h-b.h || a.k-b.k);
  const n=Math.min(PHASE_BINS, list.length);
  const bins=[];
  for(let j=0;j<n;j++){
    const mine=order.filter((_,i)=>i%n===j).map(o=>o.k).sort((a,b)=>a-b);   // cell order kept
    const first=order[j], p=at(list[first.k]);
    const speed=SPEED_LO+(SPEED_HI-SPEED_LO)*frac2(p[1]*0.01, p[0]*0.01, 9.3);
    bins.push({idx:mine, shift:first.h*span, speed});
  }
  return bins;
}

/** Round 18ai: one bin's instances as a batch sees them - their matrices, lamp hours and
 *  cell runs, cut out of the whole list's by index. */
function subInstances(idx, list, m, lamp, info){
  const sub=idx.map(k=>list[k]);
  const mm=new Float32Array(idx.length*12);
  idx.forEach((k,i)=>mm.set(m.subarray(k*12,k*12+12), i*12));
  let lp=null;
  if(lamp){ lp=new Float32Array(idx.length*2); idx.forEach((k,i)=>{ lp[i*2]=lamp[k*2]; lp[i*2+1]=lamp[k*2+1]; }); }
  const groups=(info.parts.length && info.visAabb)? cellRuns(sub, mm, info.visAabb) : [];
  return {m:mm, n:idx.length, groups, lamp:lp};
}

/** Round 18ai: a skinned part as one phase bin draws it - the same part, the same skin,
 *  its own GPU buffers and its own shift into the clip. Kept on the part, keyed by the
 *  shift, so a rebuild of the same cell reuses the buffers rather than making more. */
function phasedPart(part, shift, speed){
  const key=shift.toFixed(3)+'@'+(speed==null? 1 : speed).toFixed(4);
  part._phased=part._phased||new Map();
  let pp=part._phased.get(key);
  if(!pp){
    pp=Object.create(part);
    pp._gpu=null; pp._skinAt=undefined; pp._skinPos=null; pp._skinNrm=null;
    pp._animShift=shift; pp._animSpeed=(speed==null? 1 : speed);
    part._phased.set(key, pp);
  }
  return pp;
}

function retimeAnim(anim, speed, phase){
  if(!anim || !anim.nodes || !anim.nodes.length) return anim||null;
  const shift=L=>{
    const span=(+L.stop||0)-(+L.start||0);
    const f=(L.freq==null? 1 : +L.freq)*speed;
    return Object.assign({},L,{freq:f, phase:(+L.phase||0)+phase*(span>1e-9? span : 1)});
  };
  return Object.assign({},anim,{nodes:anim.nodes.map(shift)});
}

const Moths={
  MESH:'R0\\e\\moths_lntrn.NIF',
  LIST:'MWSE\\mods\\Nocturnal Moths\\data\\init.lua',
  /* The mod's own offsets for a lamp whose mesh has no AttachLight node. Only one needs
     it: `light_de_streetlight_01.nif` hangs its lamp below its origin. */
  OFFSETS:{'l/light_de_streetlight_01.nif':[0,0,-23]},
  /* What the mod ships with, for when its own list cannot be read. Kept as the file
     spells them, lowercased and slash-normalised on the way in. */
  SHIPPED:['l/light_com_lantern_01.nif','l/light_com_lantern_02.nif',
           'l/light_de_lantern_03.nif','l/light_de_lantern_04.nif',
           'l/light_de_lantern_09.nif','l/light_de_lantern_13.nif',
           'l/light_paper_lantern_01.nif','l/light_de_streetlight_01.nif',
           'l/light_ashl_lantern_01.nif','l/light_ashl_lantern_02.nif',
           'l/light_ashl_lantern_03.nif','l/light_ashl_lantern_04.nif',
           'l/light_ashl_lantern_05.nif','l/light_ashl_lantern_06.nif',
           'l/light_ashl_lantern_07.nif'],
  installed:false, from:'', lamps:new Set(), _sig:null, _busy:null,

  /** Looks for the mod in the load order, once per install. */
  check(){
    const sig=GameData.sig||'';
    if(this._sig===sig) return this._busy||Promise.resolve(this.installed);
    this._sig=sig;
    this.installed=false; this.from=''; this.lamps=new Set();
    this._busy=(async()=>{
      if(!sig || !Engine.has()) return false;
      try{ await Engine.bytes('read_asset',{path:this.MESH}); }
      catch(e){ return false; }         // no mesh, no mod
      this.installed=true;
      /* Its own list, when the overlay carries the Lua. A table of `["path"] = true`
         lines; anything else in the file is ignored, and a line commented out with `--`
         is not a lamp. */
      let list=null;
      try{
        const buf=await Engine.bytes('read_asset',{path:this.LIST});
        const txt=new TextDecoder().decode(new Uint8Array(buf));
        const found=[];
        for(let line of txt.split(/\r?\n/)){
          const c=line.indexOf('--'); if(c===0) continue;
          if(c>0) line=line.slice(0,c);
          const m=line.match(/\[\s*"([^"]+)"\s*\]\s*=\s*true/);
          if(m) found.push(m[1]);
        }
        if(found.length) list=found;
      }catch(e){ /* the mod is there; only its list is not */ }
      this.from = list? T('cell.moths_from_own') : T('cell.moths_from_shipped');
      for(const p of (list||this.SHIPPED)) this.lamps.add(normSep(p).toLowerCase());
      return true;
    })();
    return this._busy;
  },
  /** Whether this mesh is one of the lamps the moths gather at. */
  has(model){ return this.installed && this.lamps.has(normSep(model||'').toLowerCase()); },
  /** Where the moths hang on a lamp whose mesh has no AttachLight of its own. */
  offset(model){ return this.OFFSETS[normSep(model||'').toLowerCase()]||null; },
};

function renderCellTextureList(rows){
  /* The ground textures in the loaded cells: how much of the ground each covers, and
     whether its image resolved in the setup (a checkerboard on the ground is otherwise
     unexplained). Hovering a row lights that texture on the terrain. */
  App.cellTex=rows.map(r=>({id:r.id, coverage:r.coverage}));
  const box=$('#cellTexList'); if(!box) return;
  box.innerHTML='';
  const count=$('#cellTexCount');
  if(count) count.textContent = rows.length? String(rows.length) : '';
  if(!rows.length){
    box.innerHTML='<div class="hint" style="margin:0">'+T('cell.no_textures')+'</div>'; return;
  }
  rows.forEach(r=>{
    const row=document.createElement('div');
    row.style.cssText='display:flex;align-items:center;gap:7px;padding:3px 0';
    const nm=document.createElement('span');
    nm.style.cssText='flex:1;min-width:0;font-family:var(--mono);font-size:11px;'+
      'overflow:hidden;text-overflow:ellipsis;white-space:nowrap';
    nm.textContent=r.id;
    nm.title=GameData.textureFileFor(r.id)||r.id;
    row.appendChild(nm);
    if(r.noImage){
      const w=document.createElement('span');
      w.style.cssText='flex:0 0 auto;font-size:10px;color:#c9a227';
      w.textContent=T('cell.tex_no_image');
      w.title=T('cell.tex_no_image_title',{file:GameData.textureFileFor(r.id)||T('cell.tex_no_file')});
      row.appendChild(w);
    }
    const pct=document.createElement('span');
    pct.style.cssText='flex:0 0 auto;font-family:var(--mono);font-size:10.5px;color:var(--tx3)';
    pct.textContent=(r.coverage*100).toFixed(0)+'%';
    row.appendChild(pct);
    onHover(row,()=>hoverTexture(r.id));
    box.appendChild(row);
  });
}



/* ---------------- mode + cell picker wiring ---------------- */

/** What the picker button says.
 *
 *  Both halves when there are two of them: most exteriors have no name, and for those the
 *  grid *is* the name, but for the ones that have one you want to know which Balmora cell
 *  you are in as well as that you are in Balmora. */
function cellButtonLabel(){
  const t=App.cellSel;
  if(!t) return T('cell.btn_none');
  if(t.kind==='int') return t.name||T('cell.btn_interior');
  const coord='('+t.x+', '+t.y+')';
  return t.name? t.name+'  '+coord : coord;
}

function syncCellButton(){
  const b=$('#cellPick'); if(!b) return;
  b.textContent=cellButtonLabel();
  /* Round 9 item 7: the button keeps the column's width and trims a long label with an
     ellipsis, so the hover carries the label whole — a trimmed name is still readable
     without opening the picker to find out which cell you are in. */
  b.title = App.cellSel? T('cell.btn_loaded_title',{cell:cellButtonLabel()})
                       : T('cell.btn_title');
}

/** How a cell is filed among the favourites.
 *
 *  Its grid position, not its name: most exteriors have no name, and the ones a person
 *  returns to — a stretch of coast they are tuning — are as often unnamed as not. An
 *  interior has no grid, so it goes by name.
 */
function cellFavKey(c){
  return c.kind==='int' ? 'i:'+(c.name||'') : c.x+','+c.y;
}

/** Whether a picker cell answers the filter text and the kind button — one predicate for
    the list and the map (round 18cp), so what the map lights is exactly what the list
    would show. `q` is already trimmed and lower-cased. */
function cellMatches(c,q,kind){
  if(kind==='named'&&!c.name) return false;
  if(kind==='wild'&&(c.name||c.kind==='int')) return false;
  if(kind==='int'&&c.kind!=='int') return false;
  if(!q) return true;
  return c.label.toLowerCase().includes(q) || c.sub.toLowerCase().includes(q) ||
         (c.kind==='ext' && (c.x+','+c.y).includes(q.replace(/\s+/g,'')));
}

function openCellPicker(){
  if(!GameData.scanned){ toast(T('connect.setup_first'),'warn'); return; }
  let m=$('#mCell');
  if(!m){
    m=document.createElement('div'); m.className='modal'; m.id='mCell';
    m.innerHTML='<div class="mbox"><div class="mhead"><b>'+T('cell.pick_head')+'</b>'+
      '<span class="pill" id="cellCount"></span><span style="flex:1"></span>'+
      '<button class="btn dim ic" id="cellX">✕</button></div>'+
      '<div class="mbody" style="display:flex;flex-direction:column;gap:9px">'+
      '<input class="fld" id="cellFilter" placeholder="'+escHtml(T('cell.pick_filter'))+'" spellcheck="false">'+
      '<div style="display:flex;gap:6px;align-items:center;flex-wrap:wrap">'+
      /* Round 18cp: the picker is a list or a map of the world. Robin: "I want to have an
         option between 'List' (which is what is there now) and a 'Map'." Two buttons in
         the kind buttons' own idiom rather than a switch, because the two are not on and
         off of one thing — they are two ways of looking. The choice is remembered on this
         machine (`picker_set`), so the picker opens the way it was left. */
      '<span class="seg" id="cellView">'+
      '<button class="btn sm" data-view="list" title="'+escHtml(T('cell.view_list_title'))+'">'+T('cell.view_list')+'</button>'+
      '<button class="btn sm" data-view="map" title="'+escHtml(T('cell.view_map_title'))+'">'+T('cell.view_map')+'</button>'+
      '</span><span class="vsep"></span>'+
      /* Interiors are in the list now, behind a switch that is off to begin with.
         Off because the great majority of what anybody wants here is a stretch of coast,
         and an install's interiors outnumber its named exteriors several times over — a
         list you have to wade through is a worse list. On, they are offered, and the
         Interiors button narrows to them alone.

         They still have no landscape and nothing scatters in one; picking one says so.
         That is the first honest step rather than a tab that is always empty, which is
         what was here before and read as a bug in the scan. */
      '<button class="btn sm pri" data-k="all">'+T('cell.pick_all')+'</button>'+
      '<button class="btn sm" data-k="named">'+T('cell.pick_named')+'</button>'+
      '<button class="btn sm" data-k="wild">'+T('cell.pick_wild')+'</button>'+
      '<button class="btn sm" data-k="int" id="cellIntOnly" hidden>'+T('cell.pick_int')+'</button>'+
      '<span style="flex:1"></span>'+
      // Wraithguard: the mods-per-cell heat over the map, on and off.
      /* Wraithguard: what the heat over the map shows - how many mods touch each cell, how
         contested its references are, how many plugins edit its ground, or one plugin's
         cells (39_wg_coverage.js). */
      '<select class="fld sm" id="cellHeat" style="width:auto" title="Colour the map by: how many mods touch each cell; how many of its references two or more plugins changed, deleted or moved; how many plugins edit its ground; or the cells one plugin touches. Hover a cell for the details; your own mods’ cells are ringed.">'+
        '<option value="off">Heat: off</option><option value="mods">Heat: mods</option>'+
        '<option value="conflicts">Heat: reference conflicts</option><option value="land">Heat: land edits</option>'+
        '<option value="plugin">Heat: one plugin…</option></select>'+
      '<select class="fld sm" id="cellHeatPlugin" style="width:auto;max-width:180px" hidden></select>'+
      '<span class="vsep"></span>'+
      '<label class="sw sm" title="'+escHtml(T('cell.pick_show_int_title'))+'"><input type="checkbox" id="cellShowInt"><span class="tr"></span></label>'+
      '<span style="font-size:11px;color:var(--tx2)">'+T('cell.pick_show_int')+'</span></div>'+
      '<div id="cellList" style="flex:1;min-height:0;overflow:auto;border:1px solid var(--line);border-radius:6px"></div>'+
      '<div id="cellMap" hidden></div>'+
      '</div><div class="mfoot">'+
      '<button class="btn" id="cellNone">'+T('cell.pick_clear')+'</button>'+
      '<button class="btn" id="cellCancel">'+T('dlg.cancel')+'</button></div></div>';
    document.body.appendChild(m);
    $('#cellX').onclick=$('#cellCancel').onclick=()=>m.hidden=true;
    $('#cellNone').onclick=()=>{ m.hidden=true;
      App.cellSel=null; syncCellButton(); schedulePreview(); };
  }
  m.hidden=false;
  // Wraithguard: the coverage for this world, and the Mods switch's state.
  if(typeof WgCoverage==='object'){
    WgCoverage.bindPicker();
    WgCoverage.load().then(()=>{ WgCoverage.bindPicker(); if(!isMap() && typeof render==='function') render(); });
  }
  let kind='all';
  /* Round 18cv: the kind starts at All every time, and the buttons say so. The dialogue
     is built once and kept, so the button lit last time stayed lit while `kind` had gone
     back to All underneath it. Robin: "the buttons for the filters should be reset to ALL
     when opening the map picker, no matter what it was before." Both, together. */
  $$('#mCell .mbody button[data-k]').forEach(x=>x.classList.toggle('pri',x.dataset.k==='all'));
  let all=GameData.cellList(App.showInteriors);
  /* Round 18cp: what a click does, list or map — one function, so the map cannot load a
     cell any differently from the list. Round 4 item 14 lives here too: choosing a scene
     while the mesh editor holds the viewport means "leave the editor and show me that".
     Round 18cv: and a cell chosen from Simplified mode (the picker opens from anywhere
     now, on the § key) switches the mode to Real cell first - a scene chosen is a scene
     asked for. Cancelling changes nothing at all. */
  const pick=c=>{
    m.hidden=true;
    if(App.mode!=='cell'){
      const ms=$('#pvMode');
      if(ms){ ms.value='cell'; if(ms.onchange) ms.onchange(); }
    }
    App.cellSel={kind:c.kind,x:c.x,y:c.y,name:c.name,label:c.label};
    syncCellButton(); schedulePreview();
  };
  const isMap=()=>App.cellPick==='map';
  const render=()=>{
    const q=$('#cellFilter').value.trim().toLowerCase();
    if(isMap()){
      /* The map filters by lighting rather than by hiding, and says how many it lit. The
         list is not rebuilt underneath it: it is hidden, and rebuilt when it comes back. */
      const n=CellMap.setFilter(q,kind);
      $('#cellCount').textContent=T('cell.pick_count',{n, total:CellMap.cellsByKey.size});
      return;
    }
    let hits=all.filter(c=>cellMatches(c,q,kind));
    /* Starred cells first. Keyed on the grid position rather than the name, because
       most exteriors have no name and the ones people come back to — a stretch of coast
       they are tuning — are as often unnamed as not. */
    hits=Favourites.sort('cell',hits,cellFavKey);
    $('#cellCount').textContent=T('cell.pick_count',{n:hits.length, total:all.length});
    const box=$('#cellList'); box.innerHTML='';
    const frag=document.createDocumentFragment();
    hits.slice(0,900).forEach(c=>{
      const row=document.createElement('div');
      row.style.cssText='display:flex;gap:10px;align-items:baseline;padding:5px 9px;'+
        'cursor:pointer;border-bottom:1px solid var(--line)';
      row.onmouseenter=()=>row.style.background='var(--bg3)';
      row.onmouseleave=()=>row.style.background='';
      const nm=document.createElement('span');
      nm.style.cssText='flex:1 1 55%;font-family:var(--mono);font-size:11.5px;overflow:hidden;'+
        'text-overflow:ellipsis;white-space:nowrap';
      nm.textContent=c.label;
      if(!c.name&&c.kind==='ext') nm.style.color='var(--tx2)';
      const sb=document.createElement('span');
      sb.style.cssText='flex:1 1 45%;font-size:10.5px;color:var(--tx3);overflow:hidden;'+
        'text-overflow:ellipsis;white-space:nowrap';
      sb.textContent=c.sub;
      row.appendChild(nm); row.appendChild(sb);
      // Wraithguard: how many mods touch the cell (and which, on hover) - interiors too.
      { const b=typeof WgCoverage==='object'? WgCoverage.badge(c) : null; if(b) row.appendChild(b); }
      row.appendChild(Favourites.star(row,'cell',cellFavKey(c),render));
      row.onclick=()=>pick(c);
      frag.appendChild(row);
    });
    if(hits.length>900){
      const d=document.createElement('div'); d.className='hint'; d.style.padding='8px 10px';
      d.textContent=T('cell.pick_first_900',{n:hits.length});
      frag.appendChild(d);
    }
    if(!hits.length){
      const d=document.createElement('div'); d.className='empty'; d.textContent=T('cell.pick_nothing');
      frag.appendChild(d);
    }
    box.appendChild(frag);
  };
  $$('#mCell .mbody button[data-k]').forEach(b=>{
    b.onclick=()=>{ kind=b.dataset.k;
      $$('#mCell .mbody button[data-k]').forEach(x=>x.classList.toggle('pri',x===b));
      render(); };
  });
  /* Round 18cp: list or map. The map is exteriors only — an interior has no place on it —
     so the interior controls go with the list, and a kind of "int" falls back to all. The
     dialogue grows for the map (`#mCell.map` in the stylesheet): a world is wider than a
     list. */
  const showView=()=>{
    const map=isMap();
    m.classList.toggle('map',map);
    $$('#cellView button').forEach(b=>b.classList.toggle('pri',(b.dataset.view==='map')===map));
    $('#cellList').hidden=map; $('#cellMap').hidden=!map;
    const intSw=$('#cellShowInt');
    if(intSw){ const lab=intSw.closest('label'); lab.hidden=map; if(lab.nextElementSibling) lab.nextElementSibling.hidden=map; }
    const only=$('#cellIntOnly'); if(only) only.hidden=map||!App.showInteriors;
    if(map && kind==='int'){ kind='all';
      $$('#mCell .mbody button[data-k]').forEach(x=>x.classList.toggle('pri',x.dataset.k==='all')); }
    if(map){
      CellMap.open($('#cellMap'), GameData.cellList(false), pick);
      requestAnimationFrame(()=>CellMap.resize());
    }
    render();
  };
  $$('#cellView button').forEach(b=>{
    b.onclick=()=>{
      const mode=b.dataset.view==='map'? 'map' : 'list';
      if(App.cellPick===mode) return;
      App.cellPick=mode;
      if(Engine.has()) Engine.call('picker_set',{mode}).catch(()=>{});
      showView();
    };
  });
  /* The switch rebuilds the list rather than filtering it: whether interiors are in the
     picker at all is a different question from which of what is there you want to see,
     and the count in the heading should say how many places you are choosing among. */
  const sw=$('#cellShowInt');
  const only=$('#cellIntOnly');
  const syncInt=()=>{
    if(only) only.hidden=!App.showInteriors;
    if(!App.showInteriors && kind==='int'){
      kind='all';
      $$('#mCell .mbody button[data-k]').forEach(x=>x.classList.toggle('pri',x.dataset.k==='all'));
    }
  };
  if(sw){
    sw.checked=!!App.showInteriors;
    sw.onchange=()=>{
      App.showInteriors=sw.checked;
      all=GameData.cellList(App.showInteriors);
      syncInt();
      if(typeof PrevSettings==='object' && PrevSettings.touch) PrevSettings.touch();
      render();
    };
  }
  syncInt();
  Favourites.load().then(render);
  $('#cellFilter').oninput=render;
  $('#cellFilter').value='';
  showView();
  setTimeout(()=>$('#cellFilter').focus(),40);
}

function fillCellPicker(){ syncCellButton(); }

function bindCellMode(){
  const modeSel=$('#pvMode');
  if(!modeSel) return;
  modeSel.onchange=()=>{
    /* Round 18cy: the cell being left keeps its camera for the visit history, and the
       history's arrows and the star leave the statistics box with the mode - the box
       itself may go on showing the last cell's numbers until the patch is drawn, but the
       arrows are Real cell's alone (Robin: "Only show these buttons when in the Real
       Cell Preview mode"). */
    if(App.mode==='cell' && modeSel.value!=='cell' && typeof CellHistory==='object'){
      CellHistory.snapshot();
      $$('#stats .hist, #stats .fav').forEach(el=>{ el.hidden=true; });
    }
    App.mode=modeSel.value;
    App._framedKey=null;              // each mode reframes when it next draws
    const on=App.mode==='cell';
    /* Entering Simplified puts the brush down and greys the way back in — painting
       belongs to real ground only. */
    if(!on && App.paintMode && App.setPaintMode) App.setPaintMode(false);
    if(App.syncPaintButton) App.syncPaintButton();
    /* Each pane belongs to one mode. Terrain relief, ground height, patch size and
       the ban strip describe a synthetic patch that does not exist in cell mode, and
       a control that cannot do anything is worse than no control — it invites you to
       turn it and wonder why nothing happened. */
    /* Two of them now. The cell picker sits directly under Mode rather than at the top
       of the pane, so it is hidden separately — but by the same rule, and on the same
       line, so the two cannot drift apart. */
    $('#cellPane').hidden=!on;
    const cr=$('#cellRow'); if(cr) cr.hidden=!on;
    /* Round 18cn: and the one row that belongs to both panes, moved on the same line and by
       the same rule, so it cannot be left parked in the pane that just went dark. */
    if(App.placeGrassFar) App.placeGrassFar(on);
    $('#pvModeHint').innerHTML = T(on? 'preview.mode_hint_cell' : 'preview.mode_hint_simple');
    if(on){
      if(!GameData.scanned){
        toast(T('connect.cell_preview'),'warn',6000);
        $('#cellPane').hidden=true;
        if(cr) cr.hidden=true;
        return;
      }
      /* Real cell starts empty, every time. Loading a cell is the most expensive thing
         the tool does — every landscape in the load order, every placed object, every
         mesh and collision shape behind them — and switching mode is not the same as
         asking for that. So the switch offers the picker and waits. */
      App.cellSel=null;
      syncCellButton();
      setCellStatus(T('cell.status_choose'),'warn');
    }
    schedulePreview();
  };
  $('#cellPick').onclick=openCellPicker;
  /* Clicking an arrow is the same act as choosing the neighbour in the picker, so it
     goes through the same field: set the target, relabel the button, rebuild. The name
     is looked up rather than invented — a cell with a name should say it, and the grid
     is the fallback for the great majority that have none. */
  if(App.R) App.R.onArrowClick=dir=>{
    const from=App.R.arrowFrom; if(!from) return;
    const step={e:[1,0], w:[-1,0], n:[0,1], s:[0,-1]}[dir]; if(!step) return;
    const x=from.x+step[0], y=from.y+step[1];
    const named=GameData.grid? GameData.grid.get(x+','+y) : null;
    App.cellSel={kind:'ext', x, y, name:named? named.name : '',
                 label:(named&&named.name? named.name+'  ' : '')+'('+x+', '+y+')'};
    App._histVia={via:'arrow'};              // round 18cy: how the history joins the two cells
    syncCellButton();
    schedulePreview();
  };
  $('#pvAdjacent').onchange=e=>{ App.showAdjacent=+e.target.value||0; schedulePreview(); };
  /* Round 16b: a new cap means the textures on the GPU are the wrong size, so they and
     the meshes that hold them are forgotten and the cell is loaded again. */
  $('#pvTexDetail').onchange=e=>{ App.textureDetail=+e.target.value||0; dropMeshes(); dropTextures();
                                  App._scene=null; schedulePreview(); };
  $('#cellReload').onclick=()=>{ CellData.cache.clear(); App._framedKey=null; schedulePreview(); };
  syncCellButton();
}


const normSep=v=>String(v||'').replace(/\\+/g,'/');












