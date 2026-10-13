
/* =====================================================================================
   Wraithguard: distant land - the exteriors past the loaded rings, drawn coarse.

   The rings (the cells pill) load whole cells; past them the world used to stop at the
   edge of the slab. OpenMW draws on to its viewing distance, and so does this now: the
   engine (`distant_land`, viewcore/src/distant.rs) sends every exterior out to the
   install's draw distance and outside the rings as a coarse height grid - 17 points a
   side next to the rings, then 9, then 5 - with the ground's colour at each point (the
   land texture's one colour times the record's vertex colour), the seabed at its own depth
   with the water sheet carried out over it (26_water.js).
   One mesh, drawn by the terrain's own program right after the loaded cells
   (06_gl.js, `this.distant`), so it is lit and fogged as the land is. A skirt hangs
   under every cell's edge, so neither a coarser neighbour nor the loaded rings' edge
   opens a crack.

   On when the install draws it (OpenMW's `[Terrain] distant terrain`, MGE's distant
   land), and on when the viewer cannot tell; Preview, Scene, "Distant land" switches it.
   Out to the install's draw distance (`[Camera] viewing distance` in cells, MGE's own),
   at least two cells past the rings and at most forty (the far plane is 48).
   Loaded after the cell, never before it: the cell is what was asked for.
   Copyright (c) 2026 StaticNation.
   ===================================================================================== */
const WgDistant={
  /** The switch: null follows the install. */
  on:null,
  _gen:0, _key:null, stats:null, err:'', _cx:0, _cy:0,

  installSays(){
    const r=(typeof Sky==='object' && Sky.data)? Sky.data.renderer : null;
    return (r && typeof r.distantLand==='boolean')? r.distantLand : null;
  },
  wanted(){
    if(this.on!=null) return !!this.on;
    const i=this.installSays();
    return i==null? true : i;
  },
  /** Cells out from the centre: the install's draw distance, two past the rings at least. */
  radius(rings){
    const r=(typeof Sky==='object' && Sky.data)? Sky.data.renderer : null;
    const dd=(r && r.drawDistance!=null && isFinite(+r.drawDistance))? +r.drawDistance : null;
    const n=dd? Math.ceil(dd)+1 : 8;
    return Math.max(rings+2, Math.min(40, n));
  },

  /** After a scene is built: the distant land around `centre` (a loaded exterior), in the
      scene's frame (`origin`), outside `rings`. Anything else clears it. */
  async refresh(R, centre, origin, rings){
    const gen=++this._gen;
    if(this.sync) this.sync();   // the install's answer may have arrived since
    if(!R || !R.setDistant) return;
    const meshView=(typeof WgMeshView==='object' && WgMeshView.active && WgMeshView.active());
    if(!centre || centre.kind!=='ext' || App.mode!=='cell' || meshView || !this.wanted()){
      if(R.distant || this._key!==null){ R.setDistant(null); this._key=null; this.stats=null; R.dirty=true; }
      return;
    }
    const radius=this.radius(rings);
    const key=[centre.gx,centre.gy,rings,radius,origin[0],origin[1],(typeof GameData!=='undefined' && GameData.sig)||''].join(',');
    if(key===this._key) return;
    const t0=performance.now();
    let ab;
    try{ ab=await Engine.bytes('distant_land',{x:centre.gx, y:centre.gy, inner:rings, radius}); }
    catch(e){ if(gen===this._gen) this.err=String(e&&e.message||e); return; }
    if(gen!==this._gen) return;   // a newer scene asked since
    // The centre cell's middle in the scene's frame, for `nearest`.
    this._cx=centre.gx*8192+4096-origin[0]; this._cy=centre.gy*8192+4096-origin[1];
    const m=this.build(ab, origin);
    R.setDistant(m.mesh);
    this._key=key; this.err='';
    this.stats={cells:m.cells, triangles:(m.mesh.idx.length/3)|0, radius, nearest:m.nearest, ms:Math.round(performance.now()-t0)};
    R.dirty=true;
  },

  /** `GDND` (see distant.rs) into one mesh: positions in the scene's frame, normals from
      the heights, the colours as they came, a skirt under every cell's edge. */
  build(ab, origin){
    const dv=new DataView(ab);
    if(ab.byteLength<8 || String.fromCharCode(dv.getUint8(0),dv.getUint8(1),dv.getUint8(2),dv.getUint8(3))!=='GDND')
      throw new Error('distant_land');
    const count=dv.getUint32(4,true);
    const CELL=8192;
    // First pass: sizes.
    let o=8, nv=0, ni=0;
    const cells=[];
    for(let c=0;c<count;c++){
      const gx=dv.getInt32(o,true), gy=dv.getInt32(o+4,true), n=dv.getUint32(o+8,true);
      cells.push({gx,gy,n,at:o+12});
      o+=12+n*n*4+n*n*3;
      nv+=n*n + 4*n;               // the grid, and its skirt's lower edge
      ni+=(n-1)*(n-1)*6 + 4*(n-1)*6;
    }
    const pos=new Float32Array(nv*3), nrm=new Float32Array(nv*3), uv=new Float32Array(nv*2);
    const col=new Uint8Array(nv*3), idx=new Uint32Array(ni);
    let v=0, k=0;
    for(const c of cells){
      const {gx,gy,n}=c, step=CELL/(n-1);
      const hAt=(i,j)=>dv.getFloat32(c.at+(j*n+i)*4,true);
      const cAt=c.at+n*n*4;
      const base=v;
      const x0=gx*CELL-origin[0], y0=gy*CELL-origin[1];
      for(let j=0;j<n;j++) for(let i=0;i<n;i++){
        const h=hAt(i,j);
        pos[v*3]=x0+i*step; pos[v*3+1]=y0+j*step; pos[v*3+2]=h;
        // The normal from the neighbours inside the cell (one-sided at its edge).
        const hl=hAt(Math.max(0,i-1),j), hr=hAt(Math.min(n-1,i+1),j);
        const hd=hAt(i,Math.max(0,j-1)), hu=hAt(i,Math.min(n-1,j+1));
        const dx=(hr-hl)/(step*((i>0&&i<n-1)?2:1)), dy=(hu-hd)/(step*((j>0&&j<n-1)?2:1));
        const l=Math.hypot(dx,dy,1);
        nrm[v*3]=-dx/l; nrm[v*3+1]=-dy/l; nrm[v*3+2]=1/l;
        const q=cAt+(j*n+i)*3;
        col[v*3]=dv.getUint8(q); col[v*3+1]=dv.getUint8(q+1); col[v*3+2]=dv.getUint8(q+2);
        v++;
      }
      for(let j=0;j<n-1;j++) for(let i=0;i<n-1;i++){
        const a=base+j*n+i, b=a+1, d=a+n, e=d+1;
        idx[k++]=a; idx[k++]=b; idx[k++]=e;
        idx[k++]=a; idx[k++]=e; idx[k++]=d;
      }
      // The skirt: the edge loop (counter-clockwise from the south-west corner), each point
      // copied straight down, the walls facing out.
      const drop=Math.max(256, step*0.75);
      const loop=[];
      for(let i=0;i<n-1;i++) loop.push(base+i);                    // south, west to east
      for(let j=0;j<n-1;j++) loop.push(base+j*n+(n-1));            // east, south to north
      for(let i=n-1;i>0;i--) loop.push(base+(n-1)*n+i);            // north, east to west
      for(let j=n-1;j>0;j--) loop.push(base+j*n);                  // west, north to south
      const low=v;
      for(const t of loop){
        pos[v*3]=pos[t*3]; pos[v*3+1]=pos[t*3+1]; pos[v*3+2]=pos[t*3+2]-drop;
        nrm[v*3]=nrm[t*3]; nrm[v*3+1]=nrm[t*3+1]; nrm[v*3+2]=nrm[t*3+2];
        col[v*3]=col[t*3]; col[v*3+1]=col[t*3+1]; col[v*3+2]=col[t*3+2];
        v++;
      }
      const L=loop.length;
      for(let s=0;s<L;s++){
        const a=loop[s], b=loop[(s+1)%L], a2=low+s, b2=low+(s+1)%L;
        idx[k++]=a; idx[k++]=a2; idx[k++]=b2;
        idx[k++]=a; idx[k++]=b2; idx[k++]=b;
      }
    }
    // How near the centre cell's middle any of it comes (in the scene's frame, the
    // centre cell's middle is origin-relative): the rings' edge, when it is placed right.
    let nearest=Infinity;
    for(let q=0;q<v;q++) nearest=Math.min(nearest, Math.max(Math.abs(pos[q*3]-this._cx), Math.abs(pos[q*3+1]-this._cy)));
    return {cells:count, nearest, mesh:{pos:pos.subarray(0,v*3), nrm:nrm.subarray(0,v*3), uv:uv.subarray(0,v*2), col:col.subarray(0,v*3), idx:idx.subarray(0,k)}};
  },
};
