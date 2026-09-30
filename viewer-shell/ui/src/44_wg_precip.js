
/* =====================================================================================
   Weather effects - Wraithguard. What falls and blows in the weather the Preview picks,
   drawn around the camera as OpenMW's sky draws it (weather.cpp / sky.cpp):

   - Rain and Thunderstorm: rain. OpenMW's own rain rather than a mesh - drops in a box
     around the eye, `Rain Diameter` across (600) and from `Rain Height Min` to `Max` above
     it (200..700), falling at `Weather Precip Gravity` (575 units a second), `Max
     Raindrops` of them (650 in rain, 1350 in a thunderstorm), each a thin vertical streak
     with the game's raindrop texture (`tx_raindrop_01.dds`). A thunderstorm's rain leans
     with the wind.
   - Snow, Blizzard, Ashstorm and Blight: the game's own particle meshes for them
     (`snow.nif`, `blizzard.nif`, `ashcloud.nif`, `blightcloud.nif`), run by the particle
     systems (28_particles.js) and carried along with the eye. An ash or blight storm's is
     turned to blow away from Red Mountain, as OpenMW turns it.

   Outdoors only (and in rooms that behave as outdoors), not seen from under the water,
   and with the Particles switch: these are particles like the rest.

   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   (No backticks inside the shader strings: template literals.)
   ===================================================================================== */

const PRECIP_BOX=600, PRECIP_H_MIN=200, PRECIP_H_MAX=700, PRECIP_GRAVITY=575;
const PRECIP_DROPS={Rain:650, Thunderstorm:1350};
const WX_MESH={Snow:'snow.nif', Blizzard:'blizzard.nif', Ashstorm:'ashcloud.nif', Blight:'blightcloud.nif'};
/** Where the storms blow from: Red Mountain, in world units (OpenMW's storm direction is
 *  from the mountain to the player). */
const RED_MOUNTAIN=[25000, 70000];

/* A drop: a streak standing upright, turned about the vertical to face the eye. The
   instance layout is the particles' (position and half-length, colour, a spare float). */
const VS_PRECIP_RAIN=`#version 300 es
precision highp float;
layout(location=0) in vec2 aQ;
layout(location=1) in vec4 iPS;
layout(location=2) in vec4 iCol;
layout(location=3) in float iLean;
uniform mat4 uVP;
uniform vec3 uEye;
uniform vec2 uWind;
out vec2 vUV; out vec4 vCol; out vec3 vW;
void main(){
  vec3 toEye = uEye - iPS.xyz;
  vec3 side = normalize(vec3(-toEye.y, toEye.x, 0.0) + vec3(1e-4, 0.0, 0.0));
  vec3 along = normalize(vec3(uWind * iLean, 1.0));
  vec3 w = iPS.xyz + side * aQ.x * 0.6 + along * aQ.y * iPS.w;
  vUV = aQ * 0.5 + 0.5;
  vCol = iCol;
  vW = w;
  gl_Position = uVP * vec4(w, 1.0);
}`;

Object.assign(Renderer.prototype,{
  /** The weather's precipitation, or null: `{kind:'rain', drops}` or `{kind:'mesh', mesh}`. */
  precipNow(){
    if(this.opts.particles===false || this.underNow) return null;
    const room=this.opts.room||null;
    if(room && !room.quasi) return null;
    const w=(typeof Sky==='object' && Sky.weather) || 'Clear';
    if(PRECIP_DROPS[w]) return {kind:'rain', drops:PRECIP_DROPS[w], storm:w==='Thunderstorm'};
    if(WX_MESH[w]) return {kind:'mesh', mesh:WX_MESH[w], storm:w==='Ashstorm'||w==='Blight'};
    return null;
  },
  /** Whether the particle pass should run for the weather alone. */
  wantsPrecip(){ return !!this.precipNow(); },

  /* ---- rain ---- */
  _rainInit(){
    if(this._rain) return this._rain;
    const gl=this.gl;
    this._partInit();
    const R={prog:this._prog(VS_PRECIP_RAIN,FS_PART), n:0, p:null, buf:null, tex:null, texAsked:false, last:null};
    R.vao=gl.createVertexArray(); gl.bindVertexArray(R.vao);
    this._buf(new Float32Array([-1,-1, 1,-1, 1,1, -1,1]));
    gl.enableVertexAttribArray(0); gl.vertexAttribPointer(0,2,gl.FLOAT,false,0,0);
    const ib=gl.createBuffer(); gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER,ib);
    gl.bufferData(gl.ELEMENT_ARRAY_BUFFER,new Uint16Array([0,1,2, 0,2,3]),gl.STATIC_DRAW);
    R.inst=gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER,R.inst);
    gl.enableVertexAttribArray(1); gl.vertexAttribPointer(1,4,gl.FLOAT,false,36,0);  gl.vertexAttribDivisor(1,1);
    gl.enableVertexAttribArray(2); gl.vertexAttribPointer(2,4,gl.FLOAT,false,36,16); gl.vertexAttribDivisor(2,1);
    gl.enableVertexAttribArray(3); gl.vertexAttribPointer(3,1,gl.FLOAT,false,36,32); gl.vertexAttribDivisor(3,1);
    gl.bindVertexArray(null);
    this._rain=R;
    return R;
  },
  /** Moves the drops on by `dt`; a drop that falls below the box, or that the eye has left
   *  behind, comes back in at the top on the far side, so the box stays full. */
  _rainStep(R, n, eye, dt){
    const half=PRECIP_BOX/2;
    if(!R.p || R.n!==n){
      R.n=n; R.p=new Float32Array(n*5); R.buf=new Float32Array(n*9);
      for(let i=0;i<n;i++){
        const k=i*5;
        R.p[k]=eye[0]+(Math.random()*2-1)*half;
        R.p[k+1]=eye[1]+(Math.random()*2-1)*half;
        R.p[k+2]=eye[2]-PRECIP_H_MIN+Math.random()*(PRECIP_H_MAX+PRECIP_H_MIN);
        R.p[k+3]=8+Math.random()*8;     // half-length
        R.p[k+4]=0.7+Math.random()*0.3; // how far it leans with the wind
      }
    }
    const fall=PRECIP_GRAVITY*dt, wind=R.wind||[0,0];
    for(let i=0;i<n;i++){
      const k=i*5, lean=R.p[k+4];
      R.p[k]+=wind[0]*lean*PRECIP_GRAVITY*dt; R.p[k+1]+=wind[1]*lean*PRECIP_GRAVITY*dt;
      R.p[k+2]-=fall;
      let dx=R.p[k]-eye[0], dy=R.p[k+1]-eye[1];
      if(dx>half) R.p[k]-=PRECIP_BOX; else if(dx<-half) R.p[k]+=PRECIP_BOX;
      if(dy>half) R.p[k+1]-=PRECIP_BOX; else if(dy<-half) R.p[k+1]+=PRECIP_BOX;
      if(R.p[k+2]<eye[2]-PRECIP_H_MIN || R.p[k+2]>eye[2]+PRECIP_H_MAX+50){
        R.p[k+2]=eye[2]+PRECIP_H_MIN+Math.random()*(PRECIP_H_MAX-PRECIP_H_MIN);
      }
    }
  },
  _drawRain(VP, eye, airOn, sun, fogK, side, pr0){
    const want=this.precipNow();
    if(!want || want.kind!=='rain'){ if(this._rain) this._rain.p=null; return; }
    const gl=this.gl, o=this.opts, R=this._rainInit();
    if(!R.texAsked && typeof loadTexture==='function'){
      R.texAsked=true;
      loadTexture('textures\\tx_raindrop_01.dds').then(t=>{ R.tex=(t&&t.gl)||null; this.dirty=true; }).catch(()=>{});
    }
    const now=performance.now()/1000;
    if(side<=0){
      const dt=R.last==null? 0 : Math.min(0.1, Math.max(0, now-R.last));
      R.last=now;
      // A thunderstorm's rain leans, a little, one way.
      R.wind = want.storm? [0.18, 0.08] : [0, 0];
      this._rainStep(R, want.drops, eye, dt);
    }
    const n=R.n, B=R.buf;
    for(let i=0;i<n;i++){
      const k=i*5, b=i*9;
      B[b]=R.p[k]; B[b+1]=R.p[k+1]; B[b+2]=R.p[k+2]; B[b+3]=R.p[k+3];
      B[b+4]=0.75; B[b+5]=0.8; B[b+6]=0.85; B[b+7]=R.tex? 0.9 : 0.35;
      B[b+8]=R.p[k+4];
    }
    const pr=R.prog; gl.useProgram(pr.p);
    gl.uniformMatrix4fv(pr.u.uVP,false,VP);
    gl.uniform3fv(pr.u.uEye,eye);
    gl.uniform2fv(pr.u.uWind,R.wind||[0,0]);
    gl.uniform3fv(pr.u.uSun,sun);
    gl.uniform1f(pr.u.uBright, o.bright==null?1.0:o.bright);
    gl.uniform1i(pr.u.uTex,0);
    gl.uniform3fv(pr.u.uFogCol,this.backdrop());
    if(fogK!=null) gl.uniform1f(pr.u.uFogK,fogK);
    airOn(pr);
    gl.uniform1i(pr.u.uSide, side|0);
    gl.uniform1f(pr.u.uWaterZ, this.water? this.water.z : 0);
    if(pr.u.uFogBlack) gl.uniform1i(pr.u.uFogBlack,0);
    gl.uniform3fv(pr.u.uEmissive,[0.25,0.25,0.25]);
    gl.uniform1f(pr.u.uMatAlpha,1.0);
    gl.uniform1i(pr.u.uLit,1);
    gl.enable(gl.DEPTH_TEST); gl.depthMask(false);
    gl.enable(gl.BLEND); gl.blendFunc(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA);
    gl.disable(gl.CULL_FACE);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, R.tex||this.white);
    gl.uniform1i(pr.u.uHasTex, R.tex?1:0);
    gl.bindVertexArray(R.vao);
    gl.bindBuffer(gl.ARRAY_BUFFER,R.inst);
    gl.bufferData(gl.ARRAY_BUFFER,B.subarray(0,n*9),gl.DYNAMIC_DRAW);
    gl.drawElementsInstanced(gl.TRIANGLES,6,gl.UNSIGNED_SHORT,0,n);
    gl.bindVertexArray(null);
    gl.disable(gl.BLEND);
    gl.depthMask(true);
    this._texU=null;
    this.rainDropsDrawn=n;   // for the tests
    this.dirty=true;
  },

  /* ---- snow, blizzard, ash and blight: the game's meshes, carried with the eye ---- */
  _wxKey(mesh){ return '__wx:'+mesh; },
  _wxUpdate(eye){
    const want=this.precipNow();
    const mesh=(want && want.kind==='mesh')? want.mesh : null;
    const W=this._wx||(this._wx={mesh:null, at:null, loading:null, failed:new Set()});
    const P=this._parts;
    // The particle runs were cleared with the objects: load again.
    if(W.mesh && P && ![...P.groups.keys()].some(k=>k.startsWith(this._wxKey(W.mesh)))) W.mesh=null;
    if(W.mesh!==mesh){
      if(P && W.mesh) for(const k of [...P.groups.keys()]) if(k.startsWith('__wx:')) P.groups.delete(k);
      W.mesh=null; W.at=null;
      if(mesh && W.loading!==mesh && !W.failed.has(mesh) && typeof loadMeshes==='function'){
        W.loading=mesh;
        loadMeshes(['Meshes\\'+mesh]).then(([info])=>{
          W.loading=null;
          const again=this.precipNow();
          // A setup without the mesh (or one with no systems in it) is asked once.
          if(!info || info.err || !info.particles || !info.particles.length){ W.failed.add(mesh); return; }
          if(!again || again.mesh!==mesh) return;
          // Turned so its +Y runs from Red Mountain to here, for the storms.
          let c=1, s=0;
          if(again.storm){
            const e=this._wxEye||[0,0,0];
            const dx=e[0]-RED_MOUNTAIN[0], dy=e[1]-RED_MOUNTAIN[1], L=Math.hypot(dx,dy)||1;
            const th=Math.atan2(-dx/L, dy/L); c=Math.cos(th); s=Math.sin(th);
          }
          const e=this._wxEye||[0,0,0];
          this.addParticleSystems(info.particles, [c,-s,0,e[0], s,c,0,e[1], 0,0,1,e[2]], null, this._wxKey(mesh));
          W.mesh=mesh; W.at=e.slice();
        }).catch(()=>{ W.loading=null; W.failed.add(mesh); });
      }
    }
    this._wxEye=eye.slice();
    // Carry the systems with the eye: their emitters move, what they shot stays.
    if(W.mesh && W.at && P){
      const d=[eye[0]-W.at[0], eye[1]-W.at[1], eye[2]-W.at[2]];
      if(d[0]||d[1]||d[2]){
        for(const [k,g] of P.groups){
          if(!k.startsWith('__wx:')) continue;
          for(const r of g.runs){
            for(let i=0;i<3;i++){ r.pos[i]+=d[i]; if(r.npos) r.npos[i]+=d[i]; }
            r.gpos=[];
          }
        }
        W.at=eye.slice();
      }
    }
  },
});

/* The particle pass, with the weather's effects: the meshes kept up to date before it,
   the rain drawn after it. */
{
  const base=Renderer.prototype.drawParticles;
  Renderer.prototype.drawParticles=function(VP, eye, airOn, sun, fogK, side){
    this._wxUpdate(eye);
    if(this._parts && this._parts.groups.size) base.call(this, VP, eye, airOn, sun, fogK, side);
    this._drawRain(VP, eye, airOn, sun, fogK, side|0);
  };
}
