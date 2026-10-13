
/* =====================================================================================
   Wraithguard: distant objects - what stands on the distant land (57_wg_distant.js).

   OpenMW's object paging, for the viewer: the engine lists the references of every
   exterior past the loaded rings and out to the distant land's edge (`distant_objects`),
   leaving out what OpenMW does not page - actors, grass, markers - and anything too small
   for its distance (`object paging min size`, 0.01). Each model is drawn simplified, at
   one of three levels by how far past the rings it stands (`__lod/<level>/<mode>/<model>`
   through the ordinary mesh loader, so textures, caching and batching are the cell's
   own; viewcore/src/lod.rs simplifies), and the batches join the scene's own objects
   (`R.setStatics`), lit and fogged as they are.

   The simplify switch (Static's "step 3"): plain keeps shape first; texture-aware keeps
   more where the texture under the surface is busy and where it bends (after Zhao, Jiang
   & Guo, ISPRS 2022; see lod.rs).

   On with distant land unless switched apart (Preview, Scene, "Distant objects");
   loaded after the cell and the land, never before them.
   Copyright (c) 2026 StaticNation.
   ===================================================================================== */
const WgDistantObjects={
  /** The switch: null follows Distant land. */
  on:null,
  /** 'plain' or 'texture'. */
  mode:'plain',
  _gen:0, _key:null, _scene:null, batches:[], stats:null, err:'',

  wanted(){
    if(this.on!=null) return !!this.on;
    return (typeof WgDistant==='object')? WgDistant.wanted() : true;
  },
  /** The level for a reference `d` rings past the loaded ones: 0 nearest. */
  level(d){ return d<=4? 0 : d<=12? 1 : 2; },

  _drop(R){
    for(const b of this.batches){ try{ R._dropVao(b); }catch(_){ } }
    this.batches=[];
  },

  /** After a scene's objects are set (`sceneStatics`, its own batches) or when a switch
      changes (no `sceneStatics`: the last scene's). */
  async refresh(R, centre, origin, rings, sceneStatics){
    const gen=++this._gen;
    if(this.sync) this.sync();
    if(sceneStatics){
      // A new scene: `disposeStatics` freed the last one's batches, ours among them.
      this._scene=sceneStatics; this.batches=[]; this._key=null; this.stats=null;
      this._at={centre, origin, rings};
    }
    if(!R || !this._scene) return;
    const at=this._at||{};
    centre=centre||at.centre; origin=origin||at.origin; rings=rings==null? at.rings : rings;
    const meshView=(typeof WgMeshView==='object' && WgMeshView.active && WgMeshView.active());
    if(!centre || centre.kind!=='ext' || App.mode!=='cell' || meshView || !this.wanted()){
      if(this.batches.length){ this._drop(R); R.setStatics(this._scene); R.dirty=true; }
      this._key=null; this.stats=null;
      return;
    }
    /* No further than ten rings past the loaded ones, however far the land goes: a long
       draw distance on a big install is hundreds of thousands of references, and the page
       closed for want of memory (Static, 2026-10-13). The engine keeps a budget as well. */
    const radius=Math.min((typeof WgDistant==='object')? WgDistant.radius(rings) : rings+8, rings+10);
    const key=[centre.gx,centre.gy,rings,radius,origin[0],origin[1],this.mode,
               (typeof GameData!=='undefined' && GameData.sig)||''].join(',');
    if(key===this._key) return;
    const t0=performance.now();
    let data;
    try{
      const r=await Engine.call('distant_objects',{x:centre.gx, y:centre.gy, inner:rings, radius});
      data=typeof r==='string'? JSON.parse(r) : r;
    }catch(e){ if(gen===this._gen) this.err=String(e&&e.message||e); return; }
    if(gen!==this._gen) return;
    // One mesh per model and level.
    const groups=new Map();
    for(const m of (data.models||[])){
      for(const r of m.refs){
        const path='__lod/'+(m.t|0)+'/'+this.mode+'/'+m.m;   // one copy per model, at its size tier
        let g=groups.get(path);
        if(!g) groups.set(path, g={path, refs:[]});
        g.refs.push(r);
      }
    }
    const list=[...groups.values()];
    let infos;
    try{ infos=await loadMeshes(list.map(g=>g.path)); }
    catch(e){ if(gen===this._gen) this.err=String(e&&e.message||e); return; }
    if(gen!==this._gen) return;
    const made=[];
    let tris=0, refs=0;
    list.forEach((g,k)=>{
      const info=infos[k];
      if(!info || info.err || !info.parts || !info.parts.length) return;
      const mm=new Float32Array(g.refs.length*12);
      g.refs.forEach((r,j)=>mm.set(refMatrix([r[0]-origin[0], r[1]-origin[1], r[2]], [r[3],r[4],r[5]], r[6]), j*12));
      for(const part of info.parts){
        if(part.skin || part.morph) continue;   // posed parts move; a far copy stands still
        const bt=R.buildStaticBatch(part,{m:mm,n:g.refs.length},part.glTex? part.glTex.gl : null);
        bt.distant=true;
        made.push(bt);
        tris+=((part.idx? part.idx.length : 0)/3)*g.refs.length;
      }
      refs+=g.refs.length;
    });
    if(gen!==this._gen || !this._scene){ for(const b of made){ try{ R._dropVao(b); }catch(_){ } } return; }
    this._drop(R);
    this.batches=made;
    R.setStatics(this._scene.concat(made));
    this._key=key; this.err='';
    this.stats={models:list.length, refs, beyond:data.beyond|0, over:data.over|0, triangles:Math.round(tris), mode:this.mode,
                ms:Math.round(performance.now()-t0)};
    R.dirty=true;
  },
};
