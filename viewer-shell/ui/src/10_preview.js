









/* One mesh, parsed by the engine and sent as draw geometry.
   ==========================================================

   Not the raw NIF: everything the renderer does not use is dropped on the way, so the
   buffers that arrive are smaller than the file they came from. The page used to hold
   a complete NIF reader for this, and two readers of a format this awkward — record
   layouts that have to be *scanned* for when a version is unexpected — is not a thing
   to keep in step.

   `colTris` is deliberately absent. The engine is the only thing that tests a blade
   against a collision shape, so the triangles never cross; what comes over is the
   verdict (`colSource`, `colRadius`), which is all the page needs to describe an
   object. Anything here that starts reasoning from geometry it has not been sent is
   already wrong — that mistake shipped once, as a panel reporting no obstacles over a
   cell full of them. */
const COL_SOURCE=['collision','visible','bounds','none'];

/* What is already being fetched.

   The cache is a map of finished records, and the slot cards read it synchronously, so
   it cannot hold promises. But two callers asking for the same mesh at the same moment
   is the normal case — the scatter asks once per group — and the fix for that used to
   be to put the empty record in the cache *before* the fetch. The second caller then
   got a record with no geometry in it and drew nothing: connect an install and the
   first frame came up empty, until any redraw filled it in from the by-then-finished
   record. So the fetch is shared as a promise, and the cache only ever holds a record
   that is finished. */
const _meshLoading=new Map();
const _texLoading=new Map();

/** Forgets every loaded mesh, and the GPU objects keyed by them.
 *
 *  One function because they have to go together: `buildBatch` keeps a vertex array per
 *  mesh part between rescatters (which is what makes a rescatter cheap), and a part that
 *  the cache has dropped can never be asked for again — so its vertex array would sit in
 *  the driver until the window closed. Whatever tears one down owns tearing down the
 *  other, which is §30's rule about the scene cache in a smaller place. */
function dropMeshes(){
  App.meshCache.clear();
  /* And the reads still in the air. A fetch started under the install being left will
     answer about *that* install — "not found" for a mesh only the other one has — and
     `loadMesh` hands a pending read to whoever asks next. Forgetting them here means a
     caller after the switch starts its own read against the world it can actually see;
     the old promise still resolves for whoever already holds it, and `meshKeeper`
     keeps its answer out of the cache. */
  _meshLoading.clear(); _texLoading.clear();
  /* And the pictures drawn from them. A thumbnail is a picture of a mesh in *this*
     install; keeping it across a switch would show a rock from a mod that is no longer
     loaded, on a card naming a mesh that now resolves somewhere else. */
  if(typeof clearMeshThumbs==='function') clearMeshThumbs();
  if(App.R && App.R.dropBatchGpu) App.R.dropBatchGpu();
  // Round 17y: and the parts' own vertex data, which lives as long as the parts do.
  if(App.R && App.R.dropPartGpu) App.R.dropPartGpu();
}

function loadMesh(relPath){
  const key=VFS.norm(relPath).toLowerCase();
  if(!key) return Promise.resolve(null);
  if(App.meshCache.has(key)) return Promise.resolve(App.meshCache.get(key));
  const inflight=_meshLoading.get(key);
  if(inflight) return inflight;
  const p=readMesh(key,relPath).finally(()=>{ if(_meshLoading.get(key)===p) _meshLoading.delete(key); });
  _meshLoading.set(key,p);
  return p;
}

async function readMesh(key,relPath){
  const rec=newMeshRec(relPath);
  const keep=meshKeeper(key,rec);
  let ab;
  try{ ab=await Engine.bytes('mesh_data',{path:VFS.norm(relPath)}); }
  catch(e){
    rec.err=String(e.message||e);
    App.loadLog.push({k:'e', t:T('report.mesh_missing',{path:relPath})});
    return keep();
  }
  decodeMesh(ab,rec);
  await attachMeshTextures(rec);
  return keep();
}

function newMeshRec(relPath){
  return {parts:[], collision:[], aabb:null, err:null, tris:0, thumb:null,
          path:relPath, colTris:null, colSource:'none', colRadius:0};
}
/* Which install this was read from. A mesh that arrives after the person has
   connected a different one belongs to a world nobody is looking at, and caching it
   would hand the new world the old world's geometry.

   The signature alone is not enough, which is what round 18ab is about. A connect
   stands the old install down, bumps the signature, and only then asks the engine to
   swap — so there is a window in which the page's signature already says "the new
   world" while the engine still answers out of the old one. A read that lands in that
   window fails for every mesh the outgoing install did not have, and the guard, seeing
   its own signature unchanged, filed that failure as the new world's answer: the grass
   went missing and stayed missing until the program was restarted.

   So a read is filed only if two things hold. Its token — the world's signature and the
   connect counter, which ticks at both ends of a connect — must still be current, which
   refuses anything that spanned a boundary. And no connect may be open, which refuses
   the poisonous kind: a read made wholly inside the window, of an install already on
   its way out, whose token is current precisely because nothing moved around it. */
function worldToken(){
  return ((typeof GameData!=='undefined')? GameData.sig : '')+'/'+(App.swapEpoch|0);
}
function meshKeeper(key,rec,world){
  if(world==null) world=worldToken();
  return ()=>{ if(!App.swapping && worldToken()===world) App.meshCache.set(key,rec);
               return rec; };
}

/** One key curve at a time, linearly — `[t0,v0, t1,v1, ...]` flat, as decoded.
 *
 *  Round 17m. Morrowind's UV animations are linear ramps and the reader keeps only the
 *  time and the value of each key, so this is the interpolation the data asks for. Off
 *  the ends it holds the first or last value, which is what OpenMW's `interpKey` does.
 *  An empty curve answers `fallback` — 0 for the two offsets, 1 for the two tilings. */
function uvKey(a, t, fallback){
  if(!a || !a.length) return fallback;
  if(t<=a[0]) return a[1];
  const n=a.length;
  if(t>=a[n-2]) return a[n-1];
  for(let i=0;i+3<n;i+=2){
    if(t<=a[i+2]){
      const span=a[i+2]-a[i];
      const f=span>1e-9? (t-a[i])/span : 0;
      return a[i+1]+(a[i+3]-a[i+1])*f;
    }
  }
  return a[n-1];
}

/** A part's texture matrix at wall-clock time `secs`: `[scaleU, scaleV, offU, offV]`.
 *
 *  Round 17m, Robin: "Ex_Vivec_waterfall_03 [...] All have different kinds of
 *  translucency and movement in the meshes. Is that something we can get in our renderer
 *  too? All information should be in the .nif files."
 *
 *  The controller's clock first — `time = frequency·secs + phase`, then wrapped into
 *  `start..stop` — which is OpenMW's `ControllerFunction::calculate` with the Cycle
 *  extrapolation Morrowind's looping effects use. The phase is why the two sheets of
 *  Ex_Vivec_waterfall_01 (5510.32 and 7390.48) do not fall in step. */
function uvAt(an, secs){
  if(!an) return null;
  const span=an.stop-an.start;
  let t=(an.freq||1)*secs+(an.phase||0);
  if(span>1e-9){
    const c=(t-an.start)/span;
    t=an.start+(c-Math.floor(c))*span;
  } else t=an.start;
  return [uvKey(an.us,t,1), uvKey(an.vs,t,1), uvKey(an.u,t,0), uvKey(an.v,t,0)];
}

/* ---- Node and material animation (round 17y) -------------------------------------
   Robin: "The nif f\active_blight_large.NIF is a specific effect which has large flat
   surfaces with translucent smoke like effect rotating upwards. Currently it's shown as a
   still cut out thing in the world." The file is ten translucent sheets, each under a
   NiBSAnimationNode with a NiKeyframeController that turns it and moves it, and each
   with a NiAlphaController fading its material in and out — three clocks per sheet,
   every one of them looping in its own window with its own phase. The engine bakes a
   shape's geometry *under* its animated nodes and sends the chain (`rec.parts[i].anim`);
   these turn a wall-clock second into the matrix and the alpha for this instant. */

/** A NiTimeController's clock: `frequency·secs + phase`, then wrapped into
 *  `start..stop` by the controller's cycle type — bits 1-2 of its flags: 0 loop, 1
 *  reverse (back and forth), 2 clamp. OpenMW's `ControllerFunction::calculate`. */
function ctrlTime(c, secs){
  const start=+c.start||0, stop=+c.stop||0, span=stop-start;
  let t=(c.freq==null? 1 : +c.freq)*secs+(+c.phase||0);
  if(span<=1e-9) return start;
  const mode=((c.flags|0)>>1)&3;
  if(mode===2) return Math.min(stop, Math.max(start, t));
  const cyc=(t-start)/span;
  let f=cyc-Math.floor(cyc);
  if(mode===1 && (Math.floor(cyc)&1)) f=1-f;
  return start+f*span;
}

/** Linear key lookup in a list of `[t, v0, v1, ...]` keys: the values at `t`, held at
 *  the ends. `null` for an empty list. */
function keyAt(keys, t){
  if(!keys || !keys.length) return null;
  const first=keys[0], last=keys[keys.length-1];
  if(t<=first[0]) return first.slice(1);
  if(t>=last[0]) return last.slice(1);
  for(let i=0;i+1<keys.length;i++){
    const a=keys[i], b=keys[i+1];
    if(t<=b[0]){
      const span=b[0]-a[0], f=span>1e-9? (t-a[0])/span : 0;
      const out=new Array(a.length-1);
      for(let k=1;k<a.length;k++) out[k-1]=a[k]+(b[k]-a[k])*f;
      return out;
    }
  }
  return last.slice(1);
}

/** A quaternion key list at `t`, blended the short way round and renormalised —
 *  OpenMW's `Quat::slerp` for keys this close is a nlerp, and the sheets turn a few
 *  degrees a key. `[w,x,y,z]`. */
function quatAt(keys, t){
  if(!keys || !keys.length) return null;
  const first=keys[0], last=keys[keys.length-1];
  if(t<=first[0]) return first.slice(1);
  if(t>=last[0]) return last.slice(1);
  for(let i=0;i+1<keys.length;i++){
    const a=keys[i], b=keys[i+1];
    if(t<=b[0]){
      const span=b[0]-a[0], f=span>1e-9? (t-a[0])/span : 0;
      let dot=a[1]*b[1]+a[2]*b[2]+a[3]*b[3]+a[4]*b[4];
      const s=dot<0? -1 : 1;
      const q=[a[1]+(s*b[1]-a[1])*f, a[2]+(s*b[2]-a[2])*f, a[3]+(s*b[3]-a[3])*f, a[4]+(s*b[4]-a[4])*f];
      const L=Math.hypot(q[0],q[1],q[2],q[3])||1;
      return [q[0]/L,q[1]/L,q[2]/L,q[3]/L];
    }
  }
  return last.slice(1);
}

/** A unit quaternion `[w,x,y,z]` as a row-major 3x3. */
function quatMat3(q){
  const [w,x,y,z]=q;
  return [1-2*(y*y+z*z), 2*(x*y-w*z),   2*(x*z+w*y),
          2*(x*y+w*z),   1-2*(x*x+z*z), 2*(y*z-w*x),
          2*(x*z-w*y),   2*(y*z+w*x),   1-2*(x*x+y*y)];
}

/** Three Euler channels (radians about x, y, z, applied z·y·x as the format has it) as a
 *  row-major 3x3. */
function eulerMat3(rx,ry,rz){
  const cx=Math.cos(rx), sx=Math.sin(rx), cy=Math.cos(ry), sy=Math.sin(ry), cz=Math.cos(rz), sz=Math.sin(rz);
  const X=[1,0,0, 0,cx,-sx, 0,sx,cx], Y=[cy,0,sy, 0,1,0, -sy,0,cy], Z=[cz,-sz,0, sz,cz,0, 0,0,1];
  return mul3x3(mul3x3(Z,Y),X);
}
function mul3x3(a,b){
  return [a[0]*b[0]+a[1]*b[3]+a[2]*b[6], a[0]*b[1]+a[1]*b[4]+a[2]*b[7], a[0]*b[2]+a[1]*b[5]+a[2]*b[8],
          a[3]*b[0]+a[4]*b[3]+a[5]*b[6], a[3]*b[1]+a[4]*b[4]+a[5]*b[7], a[3]*b[2]+a[4]*b[5]+a[5]*b[8],
          a[6]*b[0]+a[7]*b[3]+a[8]*b[6], a[6]*b[1]+a[7]*b[4]+a[8]*b[7], a[6]*b[2]+a[7]*b[5]+a[8]*b[8]];
}

/** `parent × child` for the engine's `[3x3 row-major, translation]` + scale pairs. */
function mulTS(pm,ps,cm,cs){
  const r=mul3x3(pm,cm);
  return {m:[r[0],r[1],r[2],r[3],r[4],r[5],r[6],r[7],r[8],
             pm[9]+(pm[0]*cm[9]+pm[1]*cm[10]+pm[2]*cm[11])*ps,
             pm[10]+(pm[3]*cm[9]+pm[4]*cm[10]+pm[5]*cm[11])*ps,
             pm[11]+(pm[6]*cm[9]+pm[7]*cm[10]+pm[8]*cm[11])*ps],
          s:ps*cs};
}

/** One animated node's transform at wall-clock `secs`: its resting transform with each
 *  keyed channel replaced by the keys' value at the controller's time. */
function animNodeAt(L, secs){
  const t=ctrlTime(L, secs);
  let rot=L.local.slice(0,9), tr=L.local.slice(9,12), sc=L.localS==null? 1 : +L.localS;
  if(L.rot && L.rot.length){ const q=quatAt(L.rot,t); if(q) rot=quatMat3(q); }
  else if(L.xyz){
    const e=[0,1,2].map(k=>{ const v=keyAt(L.xyz[k],t); return v? v[0] : 0; });
    rot=eulerMat3(e[0],e[1],e[2]);
  }
  if(L.trans && L.trans.length){ const v=keyAt(L.trans,t); if(v) tr=v; }
  if(L.scale && L.scale.length){ const v=keyAt(L.scale,t); if(v) sc=v[0]; }
  return {m:[...rot,...tr], s:sc};
}

const ANIM_IDENT=new Float32Array([1,0,0,0, 0,1,0,0, 0,0,1,0, 0,0,0,1]);
// Round 18cr: the resting values `_meshTex` compares against, one object each rather
// than a literal a batch — the shadow keeps the last array it saw and a fresh literal
// would never be equal to itself.
const UV_IDENT=[1,1,0,0];
const BLACK3=[0,0,0];
/** A part's whole animation at `secs` as a column-major mat4 for the vertex shader:
 *  `pre₀ × node₀(t) × pre₁ × node₁(t) × …`, the geometry having been baked under the
 *  last link. The identity when nothing moves. */
/** Whether a part shows at `secs` under its NiVisControllers: each key holds until the
 *  next (a switch, never blended), and every controller over the part must say shown. */
function animVisibleAt(anim, secs){
  if(!anim || !anim.vis || !anim.vis.length) return true;
  for(const c of anim.vis){
    const k=c.keys; if(!k || !k.length) continue;
    const t=ctrlTime(c, secs);
    let v=k[0][1];
    for(let i=0;i<k.length && k[i][0]<=t;i++) v=k[i][1];
    if(!(v>0.5)) return false;
  }
  return true;
}
/** Hidden: every vertex to one point, so no pass (colour, depth, shadow) draws it. */
const ANIM_HIDDEN=new Float32Array([0,0,0,0, 0,0,0,0, 0,0,0,0, 0,0,0,1]);

function animMatrixAt(anim, secs){
  if(anim && anim.vis && anim.vis.length && !animVisibleAt(anim, secs)) return ANIM_HIDDEN;
  if(!anim || !anim.nodes || !anim.nodes.length) return ANIM_IDENT;
  let m=[1,0,0,0,1,0,0,0,1,0,0,0], s=1;
  for(const L of anim.nodes){
    const pre=mulTS(m,s,L.pre,L.preS==null? 1 : +L.preS);
    const n=animNodeAt(L,secs);
    const c=mulTS(pre.m,pre.s,n.m,n.s);
    m=c.m; s=c.s;
  }
  // Column-major, scale folded into the rotation, translation last.
  return new Float32Array([m[0]*s,m[3]*s,m[6]*s,0, m[1]*s,m[4]*s,m[7]*s,0, m[2]*s,m[5]*s,m[8]*s,0, m[9],m[10],m[11],1]);
}

/* ---- Skinned shapes under an animated skeleton (round 18ag) --------------------------
   Robin: "Lets also tackle the Siltstrider. If possible, I want it to animate properly
   too." The strider's legs are rigid pieces under moving nodes and go through
   `animMatrixAt` above like the blight's sheets; its feelers are two *skinned* shapes, a
   vertex pulled by up to four bones each, and a bone is a node with a chain of its own.
   The engine sends the skin-space vertices, the weights and the bones' chains; these move
   the vertices every frame the way the engine baked the bind pose once:
   v(t) = Σ w_i · chain_i(t) × post_i × trafo_i · v. Done here on the CPU rather than in
   the shader: a few hundred vertices, once a frame, shared by every instance of the mesh,
   against a bone-matrix array the vertex shader would have to be given room for. */

/** A chain of animated nodes at `secs` as the engine's `[3x3, translation]` + scale. */
function animChainTS(nodes, secs){
  let m=[1,0,0,0,1,0,0,0,1,0,0,0], s=1;
  for(const L of (nodes||[])){
    const pre=mulTS(m,s,L.pre,L.preS==null? 1 : +L.preS);
    const n=animNodeAt(L,secs);
    const c=mulTS(pre.m,pre.s,n.m,n.s);
    m=c.m; s=c.s;
  }
  return {m,s};
}

/** Every bone's skinning matrix at `secs`: its chain, then the still tail down to the
 *  bone, then the skin-to-bone transform. */
function skinBoneMats(skin, secs){
  return skin.bones.map(b=>{
    const c=animChainTS(b.chain,secs);
    const p=mulTS(c.m,c.s,b.post,b.postS==null? 1 : +b.postS);
    return mulTS(p.m,p.s,b.trafo,b.trafoS==null? 1 : +b.trafoS);
  });
}

/** The skin's vertices under `mats`, into `outP` (and `outN` when the skin has
 *  normals). A vertex no bone claims stays where the file put it, as the engine's bind
 *  pose leaves it. */
function skinVerts(skin, mats, outP, outN){
  const P=skin.pos, N=skin.nrm, I=skin.idx, W=skin.w, nv=P.length/3;
  for(let v=0;v<nv;v++){
    const px=P[v*3], py=P[v*3+1], pz=P[v*3+2];
    let x=0,y=0,z=0, nx=0,ny=0,nz=0, ws=0;
    for(let k=0;k<4;k++){
      const b=I[v*4+k]; if(b===255) continue;
      const w=W[v*4+k]; if(!(w>0)) continue;
      const M=mats[b]; if(!M) continue;
      const m=M.m, s=M.s;
      x+=w*((m[0]*px+m[1]*py+m[2]*pz)*s+m[9]);
      y+=w*((m[3]*px+m[4]*py+m[5]*pz)*s+m[10]);
      z+=w*((m[6]*px+m[7]*py+m[8]*pz)*s+m[11]);
      if(N && outN){
        const qx=N[v*3], qy=N[v*3+1], qz=N[v*3+2];
        nx+=w*(m[0]*qx+m[1]*qy+m[2]*qz); ny+=w*(m[3]*qx+m[4]*qy+m[5]*qz); nz+=w*(m[6]*qx+m[7]*qy+m[8]*qz);
      }
      ws+=w;
    }
    if(ws>0){
      outP[v*3]=x/ws; outP[v*3+1]=y/ws; outP[v*3+2]=z/ws;
      if(N && outN){ const l=Math.hypot(nx,ny,nz)||1; outN[v*3]=nx/l; outN[v*3+1]=ny/l; outN[v*3+2]=nz/l; }
    } else {
      outP[v*3]=px; outP[v*3+1]=py; outP[v*3+2]=pz;
      if(N && outN){ outN[v*3]=N[v*3]; outN[v*3+1]=N[v*3+1]; outN[v*3+2]=N[v*3+2]; }
    }
  }
}

/* ---- Morph-animated shapes (round 18au) ---------------------------------------------
   Robin: "The moths from Nocturnal Moths also animate in the game by flapping their
   wings. That motion is not in for the moths in the tool." A moth's wingbeat is a
   NiGeomMorpherController: the shape has a base pose and one or more targets, each a set
   of per-vertex offsets with a weight curve over the controller's clock, and the drawn
   vertex is the base plus every target's offsets times its weight now - OpenMW's
   `MorphGeometry`. The engine bakes the offsets into the part's space beside the
   positions; this is the sum, done on the CPU each frame like the skinning above, for the
   same reason: a few hundred vertices shared by every instance of the mesh. */

/** Every target's weight at wall-clock `secs`, by the controller's clock. */
function morphWeights(mo, secs){
  const t=ctrlTime(mo, secs);
  return mo.targets.map(tg=>{ const v=keyAt(tg.keys,t); return v? v[0] : 0; });
}

/** `outP` (already holding the base, or the skinned, positions) plus the weighted
 *  offsets. Normals are left as they are: a wing's turn is a few degrees and the game
 *  does not recompute them either. */
function morphVerts(mo, secs, outP){
  const ws=morphWeights(mo, secs);
  for(let i=0;i<mo.targets.length;i++){
    const w=ws[i]; if(!(Math.abs(w)>1e-6)) continue;
    const off=mo.targets[i].offsets, n=Math.min(off.length,outP.length);
    for(let k=0;k<n;k++) outP[k]+=w*off[k];
  }
}

/** The material's alpha at `secs` under its NiAlphaController, or `fallback`. */
function animAlphaAt(anim, secs, fallback){
  if(!anim || !anim.alpha || !anim.alpha.keys || !anim.alpha.keys.length) return fallback;
  const v=keyAt(anim.alpha.keys, ctrlTime(anim.alpha, secs));
  return v? Math.max(0,Math.min(1,v[0])) : fallback;
}

/** The engine's mesh payload (`GDN2`, see `preview::mesh_bytes`) into `rec`. */
function decodeMesh(ab,rec){
  const dv=new DataView(ab);
  if(dv.byteLength<33 || dv.getUint32(0,true)!==0x324e4447){   // "GDN2"
    rec.err=T('report.not_a_mesh');
    return rec;
  }
  let o=4;
  rec.colSource=COL_SOURCE[dv.getUint8(o)]||'none'; o+=1;
  rec.colRadius=dv.getFloat32(o,true); o+=4;
  const bb=[];
  for(let i=0;i<6;i++){ bb.push(dv.getFloat32(o,true)); o+=4; }
  rec.aabb = rec.colSource==='none' ? null
           : {x0:bb[0], y0:bb[1], z0:bb[2], x1:bb[3], y1:bb[4], z1:bb[5]};
  /* Round 17m: where a light hangs on this mesh. Morrowind looks for a node called
     AttachLight and puts the LIGH record's light there, falling back to the origin —
     it is what sets a street lamp's glow at the flame rather than at the foot of the
     post. 88 of the 131 light_*.nif in Morrowind.bsa carry one. */
  rec.attachLight=null;
  if(dv.getUint8(o)===1){ o+=1;
    rec.attachLight=[dv.getFloat32(o,true), dv.getFloat32(o+4,true), dv.getFloat32(o+8,true)]; o+=12; }
  else o+=1;
  const nParts=dv.getUint32(o,true); o+=4;
  const dec=new TextDecoder();
  const str=()=>{ const n=dv.getUint16(o,true); o+=2;
                  const v=dec.decode(new Uint8Array(ab,o,n)); o+=n; return v; };
  /* One UV animation as the payload carries it (round 17m): the controller's clock and
     four key curves - U offset, V offset, U tiling, V tiling. Shared (18dj) by the base
     map's and the other maps' tails. */
  const readUvAnim=()=>{
    const freq=dv.getFloat32(o,true), phase=dv.getFloat32(o+4,true),
          start=dv.getFloat32(o+8,true), stop=dv.getFloat32(o+12,true); o+=16;
    const curve=()=>{ const n=dv.getUint32(o,true); o+=4;
                      const a=new Float32Array(ab.slice(o,o+n*8)); o+=n*8; return a; };
    return {freq, phase, start, stop, u:curve(), v:curve(), us:curve(), vs:curve()};
  };
  for(let i=0;i<nParts;i++){
    const name=str(), tex=str();
    const alphaFlags=dv.getUint16(o,true); o+=2;
    const th=dv.getInt16(o,true); o+=2;
    const diffuse=[dv.getFloat32(o,true), dv.getFloat32(o+4,true), dv.getFloat32(o+8,true)]; o+=12;
    const matAlpha=dv.getFloat32(o,true); o+=4;
    const nv=dv.getUint32(o,true); o+=4;
    const nt=dv.getUint32(o,true); o+=4;
    // Copies rather than views: after the strings the offsets are only 2-byte
    // aligned, and a Float32Array view needs 4.
    const take=n=>{ const a=new Float32Array(ab.slice(o,o+n*4)); o+=n*4; return a; };
    const pos=take(nv*3), nrm=take(nv*3), uv=take(nv*2);
    const idx=new Uint16Array(ab.slice(o,o+nt*6)); o+=nt*6;
    /* Vertex colours, RGB bytes, when the shape has them — the game multiplies the
       texture by these, and a basket drawn without them is a basket without its
       shading. Null when it has none; the renderer draws that as white. Round 11. */
    let col=null;
    if(dv.getUint8(o)===1){ o+=1; col=new Uint8Array(ab.slice(o,o+nv*3)); o+=nv*3; }
    else o+=1;
    /* Round 17h: the detail map and its own UVs, when the shape's NiTexturingProperty
       names one. The base is multiplied by it and doubled, as the game's second
       texture stage does. */
    let detail=null, uv2=null;
    if(dv.getUint8(o)===1){ o+=1; detail=str(); uv2=take(nv*2); }
    else o+=1;
    /* Round 17p: the glow map, when the shape's NiTexturingProperty names one (slot 4).
       This is what makes a lamp's glass bright at midnight, and it is a texture rather
       than a lighting term: OpenMW adds it to the final colour outright, after the
       lighting and after the fog. Every glow map in Morrowind and OAAB reads UV set 0. */
    let glow=null, glowUV=0;
    if(dv.getUint8(o)===1){ o+=1; glow=str(); glowUV=dv.getUint8(o); o+=1; }
    else o+=1;
    /* Round 17m: the NiMaterialProperty's emissive colour — light the shape gives off
       rather than receives. Black on nearly everything; [1,1,1] is what makes a lit
       lantern's paper shade glow, and it is the whole difference between
       light_paper_lantern_01.nif and light_paper_lantern_off.nif. */
    const emissive=[dv.getFloat32(o,true), dv.getFloat32(o+4,true), dv.getFloat32(o+8,true)]; o+=12;
    /* And the NiUVController's animation, when the shape has one: a clock and four key
       curves — U offset, V offset, U tiling, V tiling. `uvAt` below turns a time into a
       texture matrix; the renderer applies it as uv*scale + offset, which is the order
       OpenMW's UVController builds it in. */
    let uvAnim=null;
    if(dv.getUint8(o)===1){ o+=1; uvAnim=readUvAnim(); }
    else o+=1;
    /* Round 17w: how the shape is drawn, in one byte. Bit 0: unlit — the
       NiVertexColorProperty's lighting mode 0, texture times vertex colour and no sun.
       Bits 1-2: what the vertex colours are for (0 ignored, 1 the emissive term, 2
       multiplied in — the default). Bits 3-4: the NiStencilProperty's draw mode; 3 is
       "both sides", and anything else is drawn with its back faces culled, as the game
       draws it. Robin's waterfalls had their sharp lines from being drawn from both
       sides: where a sheet folds over the lip, its back was drawn over its front. */
    const dflags=dv.getUint8(o); o+=1;
    const unlit=(dflags&1)===1, vcolMode=(dflags>>1)&3, drawMode=(dflags>>3)&3;
    /* Round 17x: Glow in the Dahrk's switch, bits 5-6 — 0 always drawn, 1 the window by
       day (OFF), 2 lit at night (ON), 3 an interior's sunlit window (INT-DAY). The
       renderer shows one branch at a time by the clock. */
    const dayNight=(dflags>>5)&3;
    /* Round 18dg: bit 7 - the shape's NiZBufferProperty says test only, no write. The
       blended foliage writes depth like the game's (sorted far to near); the kwama eggs,
       the magic targets and the mods' pools are drawn over without writing. */
    const zwrite=((dflags>>7)&1)===0;
    rec.parts.push({name, pos, nrm, uv, uv2, idx, col, tex:tex||null, detail:detail||null,
                    alphaThreshold: th<0? null : th, alphaFlags,
                    glow:glow||null, glowUV,
                    twoSided:drawMode===3, drawMode, unlit, vcolMode, dayNight, zwrite,
                    matAlpha, diffuse, ambient:null, emissive, uvAnim,
                    colA:null, decal:null, uvDecal:null, clamp:0, uvAnim2:null,
                    env:null, envClamp:0, bump:null, bumpLuma:null, bumpMat:null});
  }
  /* Round 17w: the mesh's particle systems — a waterfall's mist, a candle's flame, a
     torch's smoke — as the JSON the reader writes, after the parts. Empty on nearly
     every mesh. The page runs them (28_particles.js). */
  rec.particles=null;
  if(o+4<=dv.byteLength){
    const pn=dv.getUint32(o,true); o+=4;
    if(pn>0 && o+pn<=dv.byteLength){
      try{ const arr=JSON.parse(dec.decode(new Uint8Array(ab,o,pn)));
           if(Array.isArray(arr) && arr.length) rec.particles=arr; }catch(_){ }
      o+=pn;
    }
  }
  /* Round 17y: and what moves — the animated nodes above a shape and its material's
     fade, by part index, after the particles. Attached to the part it names
     (`part.anim`); `animMatrixAt` and `animAlphaAt` above run it. */
  rec.animated=false;
  if(o+4<=dv.byteLength){
    const an=dv.getUint32(o,true); o+=4;
    if(an>0 && o+an<=dv.byteLength){
      try{ const arr=JSON.parse(dec.decode(new Uint8Array(ab,o,an)));
           if(Array.isArray(arr)) for(const a of arr){
             const p=rec.parts[a.part|0];
             if(p && ((a.nodes&&a.nodes.length) || a.alpha || (a.vis&&a.vis.length))){
               p.anim={nodes:a.nodes||[], alpha:a.alpha||null, vis:a.vis||[]}; rec.animated=true; }
           } }catch(_){ }
      o+=an;
    }
  }
  /* Round 17y: the light under AttachLight, when the file put one there - Glow in the
     Dahrk's windows do, for the light they throw into a room by day: a colour and a radius
     (the mod keeps the radius as the light node's scale). */
  rec.attachLightDef=null;
  if(o+1<=dv.byteLength && dv.getUint8(o)===1 && o+17<=dv.byteLength){
    rec.attachLightDef={colour:[dv.getFloat32(o+1,true), dv.getFloat32(o+5,true), dv.getFloat32(o+9,true)],
                        radius:dv.getFloat32(o+13,true)};
    o+=17;
  } else if(o+1<=dv.byteLength) o+=1;
  /* Round 17y: the dark maps - NiTexturingProperty's slot 1, a second stage multiplied
     over the base with no doubling - by part, with the UVs they read. Glow in the Dahrk's
     lit windows keep their colour here: a grey gradient base, the warm (or Hlaalu green)
     in the dark map, the pane's pattern in the detail. Robin: "ex_nord_win_02 which is
     supposed to be a warm glow, but they are quite bright or a white looking light." */
  if(o+4<=dv.byteLength){
    const nd=dv.getUint32(o,true); o+=4;
    for(let i=0;i<nd && o+6<=dv.byteLength;i++){
      const pi=dv.getUint32(o,true); o+=4;
      const tex=str();
      const p=rec.parts[pi];
      const nv=p? p.pos.length/3 : 0;
      if(!p || o+nv*8>dv.byteLength) break;
      p.dark=tex||null; p.uvDark=new Float32Array(ab.slice(o,o+nv*8)); o+=nv*8;
    }
  }
  /* Round 18ag: the skinned shapes under an animated skeleton - the silt strider's
     feelers - by part: the bones (each a chain of moving nodes, a still tail and the
     skin-to-bone transform), then the skin-space vertices, normals, four bone slots and
     four weights a vertex. `skinBoneMats`/`skinVerts` above move them; the renderer
     writes the result over the part's buffers each frame. Nought on nearly every mesh. */
  if(o+4<=dv.byteLength){
    const ns=dv.getUint32(o,true); o+=4;
    for(let i=0;i<ns && o+8<=dv.byteLength;i++){
      const pi=dv.getUint32(o,true); o+=4;
      const jl=dv.getUint32(o,true); o+=4;
      let bones=null;
      try{ bones=JSON.parse(dec.decode(new Uint8Array(ab,o,jl))); }catch(_){ }
      o+=jl;
      if(o+4>dv.byteLength) break;
      const nv=dv.getUint32(o,true); o+=4;
      const take=n=>{ const a=new Float32Array(ab.slice(o,o+n*4)); o+=n*4; return a; };
      const pos=take(nv*3);
      const hasN=dv.getUint8(o)===1; o+=1;
      const nrm=hasN? take(nv*3) : null;
      const idx=new Uint8Array(ab.slice(o,o+nv*4)); o+=nv*4;
      const w=take(nv*4);
      const p=rec.parts[pi];
      if(p && Array.isArray(bones) && bones.length && pos.length===p.pos.length){
        p.skin={bones,pos,nrm,idx,w}; rec.animated=true;
      }
    }
  }
  /* Round 18au: the morph-animated shapes - the moths' wingbeat - by part: the
     controller's flags and clock, then per target its weight keys and its offsets, three
     floats a vertex in the part's own space. `morphVerts` below adds the weighted
     offsets; the renderer writes the result over the part's buffers each frame, the way
     it does a skinned part's. Nought on nearly every mesh. */
  if(o+4<=dv.byteLength){
    const nm=dv.getUint32(o,true); o+=4;
    for(let i=0;i<nm && o+30<=dv.byteLength;i++){
      const pi=dv.getUint32(o,true); o+=4;
      const flags=dv.getUint16(o,true); o+=2;
      const freq=dv.getFloat32(o,true), phase=dv.getFloat32(o+4,true), start=dv.getFloat32(o+8,true), stop=dv.getFloat32(o+12,true); o+=16;
      const nt=dv.getUint32(o,true); o+=4;
      const nv=dv.getUint32(o,true); o+=4;
      const targets=[];
      for(let t=0;t<nt && o+4<=dv.byteLength;t++){
        const nk=dv.getUint32(o,true); o+=4;
        const keys=[];
        for(let k=0;k<nk && o+8<=dv.byteLength;k++){ keys.push([dv.getFloat32(o,true), dv.getFloat32(o+4,true)]); o+=8; }
        if(o+nv*12>dv.byteLength) break;
        const offsets=new Float32Array(ab.slice(o,o+nv*12)); o+=nv*12;
        targets.push({keys, offsets});
      }
      const p=rec.parts[pi];
      if(p && targets.length && nv*3===p.pos.length){
        p.morph={flags, freq, phase, start, stop, targets}; rec.animated=true;
      }
    }
  }
  /* Round 18de: the material's ambient colour, by part, for the parts whose ambient is
     not their diffuse - about one static shape in twelve (the ships, the piers, the
     Daedric walls at 0.1 under a white diffuse). The renderer scales the scene's ambient
     by it and the sun and the lamps by the diffuse, as the fixed pipeline does; a part
     not listed here has `ambient` null and the renderer takes its diffuse for both. */
  if(o+4<=dv.byteLength){
    const na=dv.getUint32(o,true); o+=4;
    for(let i=0;i<na && o+16<=dv.byteLength;i++){
      const pi=dv.getUint32(o,true); o+=4;
      const amb=[dv.getFloat32(o,true), dv.getFloat32(o+4,true), dv.getFloat32(o+8,true)]; o+=12;
      const p=rec.parts[pi];
      if(p) p.ambient=amb;
    }
  }
  /* Round 18df, the audit's cheap round. Three more tails, each a count and then per
     entry a part index, and each nought on nearly every mesh:
     - the vertex alpha, one byte a vertex, for the parts with a vertex below 255 (none
       in vanilla; OAAB's root pillars fade both ends through it) - `colA`;
     - the decal map (NiTexturingProperty slot 6) and its UVs, laid out like the dark
       maps - `decal`, `uvDecal`;
     - the base map's clamp: bit 0 clamps S, bit 1 clamps T - `clamp`. 447 vanilla
       static shapes; the rest wrap, as every texture did. */
  if(o+4<=dv.byteLength){
    const nva=dv.getUint32(o,true); o+=4;
    for(let i=0;i<nva && o+8<=dv.byteLength;i++){
      const pi=dv.getUint32(o,true); o+=4;
      const nv=dv.getUint32(o,true); o+=4;
      if(o+nv>dv.byteLength) break;
      const p=rec.parts[pi];
      if(p && p.col && nv*3===p.col.length) p.colA=new Uint8Array(ab.slice(o,o+nv));
      o+=nv;
    }
  }
  if(o+4<=dv.byteLength){
    const ndc=dv.getUint32(o,true); o+=4;
    for(let i=0;i<ndc && o+6<=dv.byteLength;i++){
      const pi=dv.getUint32(o,true); o+=4;
      const tex=str();
      const p=rec.parts[pi];
      const nv=p? p.pos.length/3 : 0;
      if(!p || o+nv*8>dv.byteLength) break;
      p.decal=tex||null; p.uvDecal=new Float32Array(ab.slice(o,o+nv*8)); o+=nv*8;
    }
  }
  if(o+4<=dv.byteLength){
    const ncl=dv.getUint32(o,true); o+=4;
    for(let i=0;i<ncl && o+5<=dv.byteLength;i++){
      const pi=dv.getUint32(o,true); o+=4;
      const bits=dv.getUint8(o); o+=1;
      const p=rec.parts[pi];
      if(p) p.clamp=bits&3;
    }
  }
  /* Round 18dj: the UV animations of the other maps - a count, then per entry a part
     index, one byte of the maps the controller moves (1 the dark map, 2 the detail, 4
     the glow, 8 the decal) and the controller laid out as the base map's. A
     NiUVController names the UV set it moves, and each map takes the controller of the
     set it reads: the Ghostgate dome's ghostfence pattern (its dark map) scrolls while
     the alpha of its base map holds its own, slower course; OAAB's lava scrolls its
     decal; Tamriel Rebuilt's bulb mushrooms their glow map. Nought on every vanilla
     shape. `uvAnim2` carries the first such controller with its maps in `maps`; the
     renderer has one transform for the other maps, and no shape surveyed has two. */
  if(o+4<=dv.byteLength){
    const nua=dv.getUint32(o,true); o+=4;
    for(let i=0;i<nua && o+21<=dv.byteLength;i++){
      const pi=dv.getUint32(o,true); o+=4;
      const maps=dv.getUint8(o); o+=1;
      const a=readUvAnim(); a.maps=maps;
      const p=rec.parts[pi];
      if(p && !p.uvAnim2) p.uvAnim2=a;
    }
  }
  /* Round 18dl: the environment maps - a count, then per entry a part index, the
     texture's name, one byte of the NiTextureEffect's clamp mode (0 clamp both, 1 clamp S,
     2 clamp T, 3 wrap - kept here as the bits `_wrapTex0` takes) and one byte saying a
     bump map follows: its name, the luma scale and offset, and the four floats of its
     matrix. The Telvanni crystals and portals in vanilla; every shape of Telvanni Bump
     Maps. `env`, `envClamp`, `bump`, `bumpLuma`, `bumpMat`. */
  if(o+4<=dv.byteLength){
    const nen=dv.getUint32(o,true); o+=4;
    for(let i=0;i<nen && o+8<=dv.byteLength;i++){
      const pi=dv.getUint32(o,true); o+=4;
      const tex=str();
      const mode=dv.getUint8(o); o+=1;
      const hasBump=dv.getUint8(o); o+=1;
      const p=rec.parts[pi];
      if(p){ p.env=tex||null; p.envClamp= mode===0? 3 : mode===1? 1 : mode===2? 2 : 0; }
      if(hasBump===1){
        const bt=str();
        if(o+24>dv.byteLength) break;
        const ls=dv.getFloat32(o,true), lo=dv.getFloat32(o+4,true); o+=8;
        const m=[dv.getFloat32(o,true),dv.getFloat32(o+4,true),dv.getFloat32(o+8,true),dv.getFloat32(o+12,true)]; o+=16;
        if(p){ p.bump=bt||null; p.bumpLuma=[ls,lo]; p.bumpMat=m; }
      }
    }
  }
  // Wraithguard: the gloss maps (slot 3) - a count, then per entry a part index and a name.
  if(o+4<=dv.byteLength){
    const ng=dv.getUint32(o,true); o+=4;
    for(let i=0;i<ng && o+6<=dv.byteLength;i++){
      const pi=dv.getUint32(o,true); o+=4;
      const tex=str();
      const p=rec.parts[pi];
      if(p) p.gloss=tex||null;
    }
  }
  rec.tris=rec.parts.reduce((a,p)=>a+p.idx.length/3,0);
  /* The bounds of what is *drawn*, which is not `rec.aabb` above — that one is the
     collision shape's, and for a mesh with no collision at all it is null.
     Right-clicking used to be answered from the collision box, so the pointer picked
     things it was nowhere near (a tree's collision cylinder is far wider than its
     trunk) and could not pick things that had no collision at all, however plainly
     they were on screen. Picking is a question about what you can see. */
  if(rec.parts.length){
    let x0=Infinity,y0=Infinity,z0=Infinity,x1=-Infinity,y1=-Infinity,z1=-Infinity;
    for(const p of rec.parts){
      const v=p.pos;
      /* An animated shape's vertices are baked under its moving node (round 17y); for
         the box they are put everywhere the node takes them - sampled across its cycle,
         so a sheet that rises a thousand units is not culled once it has risen. */
      const moving=!!(p.anim && p.anim.nodes && p.anim.nodes.length);
      const span=moving? Math.max(...p.anim.nodes.map(L=>(+L.stop||0)-(+L.start||0)), 0) : 0;
      const samples=moving? (span>0? 12 : 1) : 1;
      for(let k=0;k<samples;k++){
        const A=moving? animMatrixAt(p.anim, samples>1? (k/samples)*span : 0) : null;
        for(let i=0;i<v.length;i+=3){
          let x=v[i], y=v[i+1], z=v[i+2];
          if(A){ const ax=A[0]*x+A[4]*y+A[8]*z+A[12], ay=A[1]*x+A[5]*y+A[9]*z+A[13], az=A[2]*x+A[6]*y+A[10]*z+A[14]; x=ax; y=ay; z=az; }
          if(x<x0)x0=x; if(x>x1)x1=x;
          if(y<y0)y0=y; if(y>y1)y1=y;
          if(z<z0)z0=z; if(z>z1)z1=z;
        }
      }
    }
    if(x1>=x0) rec.visAabb={x0,y0,z0,x1,y1,z1};
  }
  if(!rec.parts.length)
    App.loadLog.push({k:'w', t:T('report.no_geometry',{path:rec.path})});
  return rec;
}

/** Every texture this mesh names, asked for at once rather than one after another. A
    tree with eight parts used to be eight round trips end to end. After a bundle
    (`loadMeshes`) these are cache hits. */
/** The install's OpenMW-style sibling maps (`_n`, `_nh`, `_spec`) by name without
    extension, for finding a diffuse's normal and specular maps; built once per texture
    list. Null when the list is not there yet. */
function normalMapIndex(){
  const list=Assets.textures;
  if(!Array.isArray(list) || !list.length) return null;
  if(normalMapIndex._of===list) return normalMapIndex._map;
  const m=new Map();
  // Wraithguard: the suffixes the setup's OpenMW looks for (settings.cfg's map patterns).
  const suf=normalMapSuffixes();
  for(const t of list){
    // Keyed and answered without the textures/ folder, the way a NIF names its maps.
    const k=VFS.norm(String(t)).toLowerCase().replace(/^textures\//,'');
    const base=k.replace(/\.[a-z0-9]+$/,'');
    if([suf.n, suf.nh, suf.spec].some(x=>x && base.endsWith(x))) m.set(base, k);
  }
  normalMapIndex._of=list; normalMapIndex._map=m;
  return m;
}
/** The normal, normal-and-height and specular suffixes: settings.cfg's patterns when the
    setup's OpenMW names them, else OpenMW's defaults (`_n`, `_nh`, `_spec`). */
function normalMapSuffixes(){
  const m=(typeof Sky==='object' && Sky.data && Sky.data.renderer && Sky.data.renderer.maps) || {};
  const s=v=>String(v||'').trim().toLowerCase();
  return {n:s(m.normalPattern)||'_n', nh:s(m.normalHeightPattern)||'_nh', spec:s(m.specularPattern)||'_spec'};
}
/** Wraithguard: a land texture's own maps - `{nrm, dspec}`, either null - by the setup's
    suffixes (settings.cfg's terrain patterns; `_diffusespec` by default), and only as far
    as its OpenMW uses them (`auto use terrain normal / specular maps`; on when unsaid). */
function landMaps(file){
  const idx=normalMapIndexAll();
  if(!idx) return null;
  const m=(typeof Sky==='object' && Sky.data && Sky.data.renderer && Sky.data.renderer.maps) || {};
  const said=!!(m.found && m.setObjNormal);
  const wantN = said? !!m.terrainNormal : true, wantS = said? !!m.terrainSpecular : true;
  const suf=normalMapSuffixes();
  const ds=String(m.terrainSpecularPattern||'_diffusespec').trim().toLowerCase();
  const base=VFS.norm(file).toLowerCase().replace(/^textures\//,'').replace(/\.[a-z0-9]+$/,'');
  return {nrm: wantN? (idx.get(base+suf.nh)||idx.get(base+suf.n)||null) : null,
          dspec: wantS? (idx.get(base+ds)||null) : null};
}
/** Every texture by name without extension (and without textures/), for landMaps. */
function normalMapIndexAll(){
  const list=Assets.textures;
  if(!Array.isArray(list) || !list.length) return null;
  if(normalMapIndexAll._of===list) return normalMapIndexAll._map;
  const m=new Map();
  for(const t of list){
    const k=VFS.norm(String(t)).toLowerCase().replace(/^textures\//,'');
    m.set(k.replace(/\.[a-z0-9]+$/,''), k);
  }
  normalMapIndexAll._of=list; normalMapIndexAll._map=m;
  return m;
}
async function attachMeshTextures(rec){
  const withTex=rec.parts.filter(p=>p.tex);
  await inFlight(withTex, 8, async p=>{
    p.glTex=await loadTexture(p.tex);
    // Carried onto the part: the batch is built from the part, and by then the texture
    // has arrived (round 17n).
    p.texAlpha=(p.glTex && p.glTex.alpha) || 'opaque';
  });
  // The detail map beside it (round 17h), the same way.
  /* Wraithguard: the OpenMW-style normal map beside the diffuse - `<name>_nh` or
     `<name>_n`, any extension the install has - when the install ships one. Read while
     `App.loadNormalMaps` is on (the mesh viewer); a cell does without, as it always has. */
  const texSet=App.loadNormalMaps? normalMapIndex() : null;
  if(texSet){
    for(const p of withTex){
      const base=VFS.norm(p.tex).toLowerCase().replace(/^textures\//,'').replace(/\.[a-z0-9]+$/,'');
      const suf=normalMapSuffixes();
      p.nrmName=texSet.get(base+suf.nh)||texSet.get(base+suf.n)||null;
      p.nrmHeight=!!(p.nrmName && texSet.get(base+suf.nh)===p.nrmName);
      p.specName=texSet.get(base+suf.spec)||null;
    }
    await inFlight(withTex.filter(p=>p.nrmName), 8, async p=>{ p.glNrm=await loadTexture(p.nrmName); });
    await inFlight(withTex.filter(p=>p.specName), 8, async p=>{ p.glSpec=await loadTexture(p.specName); });
  }
  const withDetail=rec.parts.filter(p=>p.detail);
  await inFlight(withDetail, 8, async p=>{ p.glDetail=await loadTexture(p.detail); });
  // And the glow map (round 17p) — a lamp's glass, a lit sign.
  const withGlow=rec.parts.filter(p=>p.glow);
  await inFlight(withGlow, 8, async p=>{ p.glGlow=await loadTexture(p.glow); });
  // Round 17y: and the dark map — the colour of a Glow in the Dahrk window's light.
  const withDark=rec.parts.filter(p=>p.dark);
  await inFlight(withDark, 8, async p=>{ p.glDark=await loadTexture(p.dark); });
  // Round 18df: and the decal - a poster on a wall, a sign on its post.
  const withDecal=rec.parts.filter(p=>p.decal);
  await inFlight(withDecal, 8, async p=>{ p.glDecal=await loadTexture(p.decal); });
  // Round 18dl: and the environment map, and the bump map that perturbs it.
  // The gloss map only matters where the mesh viewer draws (its studio views).
  if(App.loadNormalMaps){
    const withGloss=rec.parts.filter(p=>p.gloss);
    await inFlight(withGloss, 8, async p=>{ p.glGloss=await loadTexture(p.gloss); });
  }
  const withEnv=rec.parts.filter(p=>p.env);
  await inFlight(withEnv, 8, async p=>{ p.glEnv=await loadTexture(p.env); });
  const withBump=rec.parts.filter(p=>p.env && p.bump);
  await inFlight(withBump, 8, async p=>{ p.glBump=await loadTexture(p.bump); });
  // Round 17w: and the particle systems' sheets — mist, flame, smoke.
  const systems=(rec.particles||[]).filter(s=>s.tex);
  await inFlight(systems, 8, async s=>{ s.glTex=await loadTexture(s.tex); });
  for(const p of withTex){
    if(!rec.thumb && p.glTex && p.glTex.thumb){ rec.thumb=p.glTex.thumb; break; }
  }
}

/* Round 16b: many meshes, and the textures they name, in one round trip each way.
   A nine-cell load asked the engine for 190 meshes and then 220 textures, a trip per
   file; the engine's own work behind them is a few milliseconds (bench: 220 textures
   decode in 9 ms, 190 meshes parse in 15 ms), the trips through the shell were the
   time. `assets_bundle` takes a batch of mesh paths and the texture keys the page
   already holds, and answers with every mesh and every texture it did not have.
   Everything lands in the same caches `loadMesh` and `loadTexture` fill, so nothing
   else changes. Returns the records in the order asked. `onProgress(done,total)` is
   for the busy bar. */
const BUNDLE=8;
/* Round 18cr: where the objects phase's time goes, for the report's Loading line. Reset
   by the cell preview at the head of its objects phase; the sums are read at its end.
   `engine` is what the engine clocked inside each bundle (its own threads); `wait` is the
   page's wait for the answer, summed over bundles in flight at once, so it overstates
   wall time on purpose - it is the transport plus the engine, as the page felt it. The
   texture and mesh sums are main-thread time and so are real. */
const LoadProf={engine:0, engineKnown:false, wait:0, bundles:0, tex:0, texN:0, texBytes:0, mesh:0, meshN:0, build:0, groupsMs:null,
  reset(){ this.engine=0; this.engineKnown=false; this.wait=0; this.bundles=0; this.tex=0; this.texN=0; this.texBytes=0; this.mesh=0; this.meshN=0; this.build=0; this.groupsMs=null; },
  take(){ return {engine:this.engine, engineKnown:this.engineKnown, wait:this.wait, bundles:this.bundles, tex:this.tex, texN:this.texN, texBytes:this.texBytes, mesh:this.mesh, meshN:this.meshN, build:this.build, groupsMs:this.groupsMs}; }};
async function loadMeshes(relPaths,onProgress){
  const out=new Array(relPaths.length);
  const need=new Map();   // key -> {relPath, resolve, reject, promise}
  relPaths.forEach((rp,i)=>{
    const key=VFS.norm(rp).toLowerCase();
    if(!key){ out[i]=Promise.resolve(null); return; }
    if(App.meshCache.has(key)){ out[i]=Promise.resolve(App.meshCache.get(key)); return; }
    const inflight=_meshLoading.get(key);
    if(inflight){ out[i]=inflight; return; }
    let e=need.get(key);
    if(!e){
      e={relPath:rp};
      e.promise=new Promise((res,rej)=>{ e.resolve=res; e.reject=rej; }).finally(()=>{ if(_meshLoading.get(key)===e.promise) _meshLoading.delete(key); });
      _meshLoading.set(key,e.promise);
      need.set(key,e);
    }
    out[i]=e.promise;
  });
  const entries=[...need.values()];
  let done=0;
  const total=entries.length;
  const batches=[];
  for(let i=0;i<entries.length;i+=BUNDLE) batches.push(entries.slice(i,i+BUNDLE));
  const bc=!!(App.R && App.R.bcFormats());
  const maxSize=textureDetail();
  await inFlight(batches, 6, async batch=>{
    const recs=batch.map(e=>newMeshRec(e.relPath));
    const world=worldToken();
    try{
      const have=[...App.texGL.keys(), ..._texLoading.keys()];
      const tw=performance.now();
      const bcx=(App.R && App.R.bcExtra)? App.R.bcExtra() : [];
      /* Wraithguard: the normal and specular maps beside the textures, in the same bundle
         (they were fetched one by one afterwards, eight at a time, and a cell of a
         thousand meshes waited minutes on them). */
      let maps=[];
      if(App.loadNormalMaps){ const s=normalMapSuffixes(); maps=[s.nh, s.n, s.spec]; }
      const ab=await Engine.bytes('assets_bundle',{meshes:batch.map(e=>VFS.norm(e.relPath)), have, bc, maxSize, bcx, maps});
      LoadProf.wait+=performance.now()-tw; LoadProf.bundles++;
      const b=readBundle(ab);
      if(b.ms!=null){ LoadProf.engine+=b.ms; LoadProf.engineKnown=true; }
      // The textures first, so the meshes' lookups below are hits.
      await Promise.all(b.textures.map(async t=>{
        const key=VFS.norm(t.name).toLowerCase();
        if(App.texGL.has(key) || _texLoading.get(key)) return;
        const p=(async()=>{
          const rec={gl:null,err:null,thumb:null,w:0,h:0};
          // Round 18cs: the decode's own main-thread time (`rec.msDecode`), not the wall
          // time around it - six bundles decode side by side, and a clock across an
          // `await` counted the others' work as this one's.
          if(t.ok){ await decodeTexture(t.data,rec,t.name);
                    LoadProf.tex+=rec.msDecode||0; LoadProf.texN++; LoadProf.texBytes+=rec.bytes||0; }
          else { rec.err=t.err; App.loadLog.push({k:'e',t:t.name+': '+t.err}); }
          keepTexture(key,rec,world); return rec;
        })().finally(()=>_texLoading.delete(key));
        _texLoading.set(key,p);
        await p;
      }));
      for(let k=0;k<batch.length;k++){
        const e=batch[k], rec=recs[k], m=b.meshes[k];
        if(!m || !m.ok){
          rec.err=m? m.err : T('report.missing_from_bundle');
          App.loadLog.push({k:'e', t:T('report.mesh_missing',{path:e.relPath})});
        }else{
          const tm=performance.now();
          decodeMesh(m.data,rec);
          LoadProf.mesh+=performance.now()-tm; LoadProf.meshN++;
          await attachMeshTextures(rec);
        }
      }
    }catch(err){
      /* The bundle itself failed (an older engine without the command, say): each
         mesh the old way, one trip each, so nothing is lost but time. */
      for(let k=0;k<batch.length;k++){
        const e=batch[k];
        try{ recs[k]=await readMeshInto(VFS.norm(e.relPath).toLowerCase(), e.relPath, recs[k]); }
        catch(e2){ recs[k].err=String(e2.message||e2); }
      }
    }
    for(let k=0;k<batch.length;k++){
      const key=VFS.norm(batch[k].relPath).toLowerCase();
      /* Judged against the token taken *before* the bundle was asked for, the same one
         its textures are judged against. Building the keeper here instead would date
         the read to the moment it finished, which is always current and so never
         refuses anything — the hole round 18ab's poisoned cache came through. */
      meshKeeper(key,recs[k],world)();
      batch[k].resolve(recs[k]);
    }
    done+=batch.length;
    if(onProgress) onProgress(done,total);
  });
  return Promise.all(out);
}

/** `readMesh` for a record that already exists — the bundle's fallback. */
async function readMeshInto(key,relPath,rec){
  let ab;
  try{ ab=await Engine.bytes('mesh_data',{path:VFS.norm(relPath)}); }
  catch(e){ rec.err=String(e.message||e); App.loadLog.push({k:'e', t:T('report.mesh_missing',{path:relPath})}); return rec; }
  decodeMesh(ab,rec);
  await attachMeshTextures(rec);
  return rec;
}

/** The bundle apart: `{meshes:[{path,ok,data|err}], textures:[{name,ok,data|err}]}`. */
function readBundle(ab){
  const dv=new DataView(ab);
  /* "GDNB", or since round 18cr "GDNC": the same bundle with the engine's own clock for
     it - a u32 of milliseconds - right after the tag, for the Loading line. */
  const tag=dv.byteLength>=8? dv.getUint32(0,true) : 0;
  if(tag!==0x424e4447 && tag!==0x434e4447) throw new Error('not a bundle');
  const dec=new TextDecoder();
  let o=4, ms=null;
  if(tag===0x434e4447){ ms=dv.getUint32(o,true); o+=4; }
  const str16=()=>{ const n=dv.getUint16(o,true); o+=2; const v=dec.decode(new Uint8Array(ab,o,n)); o+=n; return v; };
  const item=(nameKey)=>{
    const name=str16();
    const ok=dv.getUint8(o)===1; o+=1;
    const n=dv.getUint32(o,true); o+=4;
    const r={ok}; r[nameKey]=name;
    if(ok) r.data=ab.slice(o,o+n); else r.err=dec.decode(new Uint8Array(ab,o,n));
    o+=n; return r;
  };
  const nm=dv.getUint32(o,true); o+=4;
  const meshes=[]; for(let i=0;i<nm;i++) meshes.push(item('path'));
  const nt=dv.getUint32(o,true); o+=4;
  const textures=[]; for(let i=0;i<nt;i++) textures.push(item('name'));
  return {meshes,textures,ms};
}

/** The longest side a texture arrives at (round 16b) - `[viewport] texture_detail`,
    0 for the file's own size. `App.textureDetail` is set by the Preview picker. */
function textureDetail(){
  const v=App.textureDetail;
  return (v==null)? 2048 : Math.max(0,+v||0);
}

function loadTexture(relPath){
  const key=VFS.norm(relPath).toLowerCase();
  if(!key) return Promise.resolve(null);
  if(App.texGL.has(key)) return Promise.resolve(App.texGL.get(key));
  const inflight=_texLoading.get(key);
  if(inflight) return inflight;
  const p=readTexture(key,relPath).finally(()=>{ if(_texLoading.get(key)===p) _texLoading.delete(key); });
  _texLoading.set(key,p);
  return p;
}

/** The engine's answer for one texture: `GDN2`, a header, then the levels.
 *
 *  Views rather than copies wherever it can: a compressed level goes to
 *  `compressedTexImage2D` exactly as it arrived. See `img.rs` for the other end.
 */
function readTextureBytes(buf){
  const dv=new DataView(buf);
  const tag=String.fromCharCode(dv.getUint8(0),dv.getUint8(1),dv.getUint8(2),dv.getUint8(3));
  if(tag!=='GDN2') throw new Error('not a texture payload: '+tag);
  const headLen=dv.getUint32(4,true);
  const head=JSON.parse(new TextDecoder().decode(new Uint8Array(buf,8,headLen)));
  let at=8+headLen;
  head.levels=(head.levels||[]).map(l=>{
    const data=new Uint8Array(buf,at,l.bytes); at+=l.bytes;
    return {w:l.w, h:l.h, data};
  });
  if(head.thumb){ head.thumb.data=new Uint8Array(buf,at,head.thumb.bytes); at+=head.thumb.bytes; }
  return head;
}

/* ---------------- what a mesh looks like ----------------

   One picture per mesh, drawn rather than borrowed. `loadMesh` already hands back the
   swatch of the mesh's first *texture*, and that is what the grass cards and the picker
   used to show: for a tuft of grass an alpha sheet that is mostly nothing, for a rock a
   slab of stone, and for a mesh whose first part names no texture, black. Robin: "I want
   the thumbnails for the meshes in the 'Inspect a mesh' picker to be the actual 3D item",
   and "I want the same thumbnails on the Grass Slots, most are just black currently".

   Cached by path and shared by both, so a mesh is drawn once however many cards name it. */
const _meshThumb=new Map();      // lc path -> data URL, or '' for "nothing to draw"
const _meshThumbBusy=new Map();


/** Throws the cache away — the meshes belong to an install, and so do their pictures. */
function clearMeshThumbs(){ _meshThumb.clear(); _meshThumbBusy.clear(); }

function keepTexture(key,rec,world){
  if(!App.swapping && (world==null || worldToken()===world)){
    // Round 18bd (F4): a key that already holds a different texture gives it up first.
    const had=App.texGL.get(key);
    if(had && had!==rec) dropTexRec(had);
    App.texGL.set(key,rec);
  }
  return rec;
}

/** Frees one cached texture's GL object — round 18bd (F4). */
function dropTexRec(rec){
  if(!rec || !rec.gl) return;
  const gl=App.R && App.R.gl;
  if(!gl) return;
  try{ gl.deleteTexture(rec.gl); }catch(_){ }
  rec.gl=null;
}

/** Empties the texture cache **and deletes the textures in it** — round 18bd (F4).
 *
 *  `makeTexture` and `makeCompressed` hand back a bare `gl.createTexture()` and nothing
 *  registers it, so the only reference to a texture is `rec.gl` inside this map: a bare
 *  `Map.clear()` made every one of them unreachable *and* undeleted. Change "Texture
 *  detail" a few times, or switch installs a few times, and the earlier passes' textures
 *  are still on the card — the page's own report line sums `rec.bytes` and can read
 *  hundreds of megabytes for a busy cell. The part buffers have always got this right
 *  (`_partGpuAll` + `dropPartGpu`, "Freed with the mesh cache, which is the only thing
 *  that lets a part go"); the asset textures were dropped by a bare clear. */
function dropTextures(){
  if(App.texGL){
    for(const rec of App.texGL.values()) dropTexRec(rec);
    App.texGL.clear();
  }
  _texLoading.clear();
  /* Round 18dh: and the two places that keep a texture of their own out of this map -
     the MGE water's wave volume (`waterVolume`) and the sky's cloud sheets (`cloudTex`).
     Both were deleted with the rest and kept being sampled: change "Texture detail" and
     the water went flat, still and its reflection askew (Robin), the clouds gone. The
     install switch forgot the volume itself; now every caller of this does. */
  if(App.R && App.R.dropWaterVolume) App.R.dropWaterVolume();
  if(typeof Sky==='object' && Sky.dropClouds) Sky.dropClouds();
}

async function readTexture(key,relPath){
  const rec={gl:null,err:null,thumb:null,w:0,h:0};
  const world=worldToken();
  if(!VFS.ok()){ rec.err=T('card.tag_no_folder'); return keepTexture(key,rec,world); }
  try{
    /* One ask, and the engine does the reading, the resolving and — for anything that is
       not already blocks the GPU takes — the decoding. It used to be a `read_asset` and
       then a DDS or TGA decode in JavaScript on the thread the window is drawn on:
       11.7 ms for a 1024² DXT1, hundreds of them in a busy cell. See `img.rs`. */
    const bc=!!(App.R && App.R.bcFormats());
    const bcx=(App.R && App.R.bcExtra)? App.R.bcExtra() : [];
    const buf=await Engine.bytes('texture_data',{path:VFS.norm(relPath), bc, maxSize:textureDetail(), bcx});
    await decodeTexture(buf,rec,relPath);
  }catch(e){
    rec.err=e.message||String(e);
    App.loadLog.push({k:'e',t:relPath+': '+rec.err});
  }
  return keepTexture(key,rec,world);
}

/** The engine's texture payload onto the GPU, into `rec`. Throws on a bad payload. */
async function decodeTexture(buf,rec,relPath){
  // The main thread's own time in here, the browser's off-thread decode left out (18cs).
  let spent=0, t0=performance.now();
  try{
    const t=readTextureBytes(buf);
    rec.w=t.w; rec.h=t.h; rec.kind=t.kind; rec.from=t.from||'';
    /* Round 17n: what the alpha channel is for — 'opaque', 'binary' or 'soft', worked out
       by the engine at full resolution (`img::classify_alpha`). A NiAlphaProperty says
       how to draw a shape but not what its texture holds, and the two want opposite
       treatment: cutting a gradient at a threshold is what turned Vivec's waterfalls into
       hard stencils. `alphaBlend` in 06_gl.js is where it is used. */
    rec.alpha=t.alpha||'opaque';
    rec.bytes=t.levels.reduce((a,l)=>a+(l.data? l.data.byteLength : 0),0);   // what the GPU holds
    if(t.kind==='bc1'||t.kind==='bc2'||t.kind==='bc3'||t.kind==='bc4'||t.kind==='bc5'||t.kind==='bc7'){
      rec.gl=App.R? App.R.makeCompressed(t.kind,t.levels) : null;
    }else if(t.kind==='volume'){
      // Round 15: MGE's water normals, a 3D texture (26_water.js).
      const l=t.levels[0];
      rec.gl=App.R? App.R.makeVolume(l.w,l.h,t.depth||1,l.data) : null;
      rec.is3d=true;
    }else if(t.kind==='file'){
      // PNG, JPEG and BMP: the browser's own decoders, which run off this thread.
      const raw=t.levels[0].data;
      const mime = raw[0]===0xFF? 'image/jpeg' : raw[0]===0x42? 'image/bmp' : 'image/png';
      spent+=performance.now()-t0;
      const img=await decodeViaBrowser(raw.slice().buffer,mime);
      t0=performance.now();
      rec.w=img.w; rec.h=img.h;
      rec.gl=App.R? App.R.makeTexture(img) : null;
      if(!t.thumb) try{ rec.thumb=rgbaToDataURL(img,72); }catch(_){}
    }else{
      const l=t.levels[0];
      rec.gl=App.R? App.R.makeTexture({w:l.w,h:l.h,data:l.data}) : null;
    }
    /* The swatch comes back beside the texture — 72 pixels of RGBA the engine shrank —
       so drawing a card never means decoding four megabytes. */
    if(t.thumb) try{
      rec.thumb=rgbaToDataURL({w:t.thumb.w,h:t.thumb.h,data:t.thumb.data},72);
    }catch(_){}
  }catch(e){
    rec.err=e.message||String(e);
    App.loadLog.push({k:'e',t:relPath+': '+rec.err});
  }
  rec.msDecode=spent+(performance.now()-t0);
  return rec;
}

/** The "choose a rule" notice over an empty viewport, on or off.
 *
 *  One place decides, because two would eventually disagree about whether the viewport is
 *  empty — and the notice outliving the scene it described is worse than never showing it.
 *  Called with `false` by every path that puts something on screen.
 */
function showEmptyViewport(on){
  const el=document.getElementById('vpempty');
  if(el) el.hidden=!on;
}

let previewTimer=null;
function schedulePreview(){
  clearTimeout(previewTimer);
  previewTimer=setTimeout(rebuildPreview,180);
  // Almost every preview setting change asks for a redraw, so this is the one place that
  // reliably sees an edit; the viewer profile save is debounced and skipped when unchanged.
  if(typeof PrevSettings!=='undefined') PrevSettings.touch();
}

/** Runs the rebuild the last `schedulePreview` queued, now, and waits for the scene.
 *
 *  Round 18bl. `schedulePreview` debounces by 180 ms, so a caller that wants to *wait* for
 *  the viewport has nothing to await: `refreshAll()` returns before the timer has even
 *  fired. That is the gap the connect fell into — the busy card came down when the world
 *  arrived, and the minute the scene took to rebuild after it was spent looking at a
 *  checkerboard with nothing on screen saying why.
 *
 *  Cancelling the pending timer rather than letting it fire alongside matters: both would
 *  run, the second superseding the first, and the wait would be paid twice. */
function previewNow(){
  clearTimeout(previewTimer);
  previewTimer=null;
  return rebuildPreview();
}


/* =====================================================================================
   Simplified mode: one texture, on ground that does not exist.

   The point of it is speed — pick a texture out of a mod set and see what its rule
   does, without going to find a cell that paints it. It used to be a whole second
   pipeline: the page grew its own procedural terrain and its own grid scatter to serve
   it, which meant the fast way to look at a texture showed an arrangement no export
   would ever contain.

   Now the engine invents a cell — real landscape record, real texture, a band of the
   rule's own banned texture if you ask for one — and everything after this is the
   ordinary cell path. Same Poisson scatter, same coverage maps, same ban rules. There
   is no simplified renderer left, only a cell that happens not to be in anybody's
   Morrowind.
   ===================================================================================== */
/* Serialised, for the same reason the cell rebuild is — and this is the one that
   matters more, because this is where the *engine's* patch is installed.

   Two of these overlapping is not a redraw done twice: each one calls `preview_patch`,
   which replaces the invented cell in the engine's world, and then rebuilds the scene
   from whatever is there when it gets round to looking. Let an older run finish after a
   newer one and the engine is left holding the older patch while `App.patchSig` claims
   the newer — so the view shows the wrong ground, and the scene key says it is the right
   one. In practice: switch to a set of rules and get the bare default ground back a
   moment later, at random. */
let _previewQueue=Promise.resolve();
function rebuildPreview(){
  /* How many rebuilds are queued or running, so "is the scene still arriving?" is a
     question with an answer rather than a guess at a timer.
   *
   * A rebuild reframes the camera, and a reframe landing in the middle of a drag resets
   * the pivot the drag is measured against — which `t_input` sees as a drag that did
   * nothing at all. It waited for `_framedKey` to hold still for half a second and called
   * that settled, which is true most of the time and false on a slow machine. A counter
   * is the thing it was trying to approximate. */
  App.previewRuns=(App.previewRuns||0)+1;
  const done=()=>{ App.previewRuns=Math.max(0,(App.previewRuns||1)-1); };
  /* Round 18n: the newest ask wins. Robin: "it feels like scattering grass gets put on a
     queue on every change I do [...] I want to cancel the current scattering process, and
     start a new one whenever I make a change." Every ask takes a number; an ask that is
     no longer the newest by the time its turn comes does nothing, since the newest will
     do it all with the state as it is then. The one running keeps its promise chain but
     the engine is told the number too (`preview_cell`'s `ask`), stops throwing darts for
     a superseded number, and `scatterIntoScene` puts nothing stale on screen. So a
     burst of edits costs one wait, after the last of them. */
  const my=(App.previewAsk=(App.previewAsk|0)+1);
  tellPreviewAsk(my);
  const next=_previewQueue
    .then(()=>{ if(my!==App.previewAsk) return; return _rebuildPreview(); })
    .catch(e=>{ console.error(e); })
    .then(done,done);
  _previewQueue=next;
  /* What the caller gets is not this ask's run - which may be skipped or overtaken - but
     the moment the scene stands for the newest ask (see `previewSettled`): "await
     rebuildPreview()" has always meant "and now the viewport shows the state", and the
     tests lean on that. */
  return next.then(()=>previewSettled());
}
/** Whether an ask made when this number was current has been overtaken since. */
function previewSuperseded(ask){ return ask!==App.previewAsk; }
/** Resolves once both rebuild queues have drained and nothing was asked while they
    did. An ask that was skipped or overtaken resolves through this, so whoever awaited
    it waits for the ask that took its place rather than reading a scene that is still
    being replaced. Only ever awaited from outside the queues: a run awaiting it from
    inside would be waiting on itself. */
function previewSettled(){
  const a=_previewQueue, b=_cellQueue;
  return Promise.all([a,b]).then(()=>(_previewQueue!==a || _cellQueue!==b)? previewSettled() : undefined);
}
/** Tells the engine the newest ask's number the moment it is made. The rebuilds run one
    after another, so the newer ask's own cells would only reach the engine once the
    older rebuild had finished - which is the wait this round is meant to cut short.
    With the number there at once, every sampler still throwing darts for an older
    number stops at its next check, and the older rebuild is over in a few milliseconds
    instead of a few seconds. Fire and forget: nothing waits on the answer. */
function tellPreviewAsk(){ /* the scatter it told is gone */ }

async function _rebuildPreview(){
  const R=App.R;
  if(!R) return;
  // Wraithguard: the mesh viewer owns the viewport while it is on (41_wg_meshview.js).
  if(typeof WgMeshView==='object' && WgMeshView.active()) return WgMeshView.render();
  /* The painting button follows the mode wherever the mode was set — the two UI
     handlers sync it themselves, but nothing stops other code (or a test) writing
     `App.mode` directly, and a button describing a stale mode invites a click that
     does nothing. A rebuild is the one thing every mode change causes. */
  { const st=$('#stats'); if(st) st.hidden=false;
    const vt=$('#vptools'); if(vt) vt.hidden=false;
    R.opts.inspecting=false;
    if(typeof placeLegend==='function') placeLegend();
    R.opts.paintFree=false; }
  // Real cells only: the viewer has no synthetic-patch mode.
  showEmptyViewport(false);
  // The raw run: this is inside the queue, and the settled chain would wait on itself.
  try{ await rebuildCellPreview(true); }
  /* Round 18bf (§I22): keyed. `cell.status_failed` is the line the cell rebuild's own
     failure path already uses, so one failure reads one way wherever it is caught. */
  catch(e){ console.error(e); setCellStatus(T('cell.status_failed',{err:e.message||e}),'warn'); }

}







