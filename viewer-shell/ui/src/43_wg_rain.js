
/* =====================================================================================
   Rain ripples - Wraithguard. A port of MGE XE's precipitation ripples (renderwater.cpp
   simulateDynamicWaves and XE Main.fx WaveStepPS; GPL-2.0 like the other MGE XE code, see
   ui/LICENSE): drops falling on the water in Rain, Thunderstorm, Snow and Blizzard.

   As MGE does it:
   - a 512 x 512 half-float field that wraps, laid on the water every 527 units (the close
     normal texcoords): red is the surface now, green the surface a step ago, blue/alpha
     the normal the ripples give;
   - drops at 150 a second (a thunderstorm half as many again), each stamped as MGE's
     three ColorFills stamp it: a 4x2 and a 2x4 rect at 0x6060 and a 2x2 at 0x4040 in its
     middle, into green - the "previous" surface, so the next step kicks the water down;
   - the damped two-dimensional wave equation, stepped at 80 Hz whatever the frame rate
     (the frame's time capped at half a second):
     u(t+1) = 0.14 * nsum + (1.96 - 0.56) * u(t) - 0.98 * u(t-1);
   - its normal, 2 * (n.xy - n.zw) + 0.5 * (n2.xy - n2.zw) over the neighbours one and
     one and a half texels away, added onto the close normal (26_water.js waterNormal).

   When: with the dynamic ripples on (MGE samples the field only with DYNAMIC_RIPPLES), in
   an exterior or a room that behaves as one, while the Preview's weather is one of the
   four. MoMW's viewer picks one weather rather than blending two, so the rate is simply
   that weather's. When the rain stops the rings run out on their own, and the field is
   let go a few seconds later. Needs a float colour target (EXT_color_buffer_float or
   _half_float); without one there are no ripples and nothing else changes.

   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page (the port of MGE XE's
   ripples). (No backticks inside the shader strings: template literals.)
   ===================================================================================== */

const RAIN_RES=512, RAIN_STEP=0.0125, RAIN_DROPS=150, RAIN_LINGER=8;

const FS_RAIN_STEP=`#version 300 es
precision highp float;
in vec2 vUV;
uniform sampler2D uTex;
out vec4 o;
const float R = 1.0 / ${RAIN_RES}.0;
float at(vec2 d){ return texture(uTex, vUV + d).r; }
void main(){
  vec4 c = textureLod(uTex, vUV, 0.0);
  vec4 n  = vec4(at(vec2(R, 0.0)), at(vec2(-R, 0.0)), at(vec2(0.0, R)), at(vec2(0.0, -R)));
  vec4 n2 = vec4(at(vec2(1.5*R, 0.0)), at(vec2(-1.5*R, 0.0)), at(vec2(0.0, 1.5*R)), at(vec2(0.0, -1.5*R)));
  // dampened discrete two-dimensional wave equation (red u(t), green u(t - 1))
  float nsum = n.x + n.y + n.z + n.w;
  o.r = 0.14 * nsum + (1.96 - 0.56) * c.r - 0.98 * c.g;
  o.g = c.r;
  // the normal map
  o.ba = 2.0 * (n.xy - n.zw) + 0.5 * (n2.xy - n2.zw);
}`;

Object.assign(Renderer.prototype,{
  /** Drops a second for the weather now: MGE's 150 for rain and snow, 1.5 times that in a
   *  thunderstorm, none in a true interior or any other weather. */
  rainRate(){
    const room=this.opts.room||null;
    if(room && !room.quasi) return 0;
    const w=(typeof Sky==='object' && Sky.weather) || 'Clear';
    const p=(w==='Rain'||w==='Snow'||w==='Blizzard')? 1 : w==='Thunderstorm'? 1.5 : 0;
    return RAIN_DROPS*p;
  },
  /** The two fields, created once (null when the GPU cannot draw into half floats). */
  _rainTargets(){
    if(this._rainT!==undefined) return this._rainT;
    const gl=this.gl;
    let ok=false;
    try{ ok=!!(gl.getExtension('EXT_color_buffer_float')||gl.getExtension('EXT_color_buffer_half_float')); }catch(_){ ok=false; }
    if(!ok) return (this._rainT=null);
    const mk=()=>{
      const t={tex:gl.createTexture(), fb:gl.createFramebuffer()};
      gl.bindTexture(gl.TEXTURE_2D,t.tex);
      gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA16F,RAIN_RES,RAIN_RES,0,gl.RGBA,gl.HALF_FLOAT,null);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.REPEAT);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.REPEAT);
      gl.bindFramebuffer(gl.FRAMEBUFFER,t.fb);
      gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,t.tex,0);
      t.ok=gl.checkFramebufferStatus(gl.FRAMEBUFFER)===gl.FRAMEBUFFER_COMPLETE;
      return t;
    };
    const a=mk(), b=mk();
    gl.bindFramebuffer(gl.FRAMEBUFFER,null); gl.bindTexture(gl.TEXTURE_2D,null);
    this._texU=null;
    if(!a.ok || !b.ok){
      for(const t of [a,b]){ try{ gl.deleteFramebuffer(t.fb); gl.deleteTexture(t.tex); }catch(_){ } }
      return (this._rainT=null);
    }
    this._rainT={src:a, dst:b, live:false, clean:false, left:0, drops:0, lastDrop:0, at:0,
                 seed:0.546372819};
    return this._rainT;
  },
  /** The field the water samples this frame, or null when there are no ripples. */
  rainTexture(){
    const R=this._rainT;
    return (R && R.live && this.wavesOn())? R.src.tex : null;
  },
  /** One frame of MGE's simulation: the drops this frame's time brings, then the steps. */
  rainStep(){
    const now=performance.now()/1000;
    const rate=this.wavesOn()? this.rainRate() : 0;
    let R=this._rainT;
    if(!rate && !(R && R.live)){ if(R) R.at=now; return; }
    R=this._rainTargets();
    if(!R) return;
    const gl=this.gl;
    const dt=R.at? Math.min(Math.max(now-R.at,0),0.5) : 0;
    R.at=now;
    if(!R.live){
      // A fresh field: cleared, as MGE clears it on leaving the weather.
      for(const t of [R.src,R.dst]){
        gl.bindFramebuffer(gl.FRAMEBUFFER,t.fb);
        gl.viewport(0,0,RAIN_RES,RAIN_RES);
        gl.clearColor(0,0,0,0); gl.clear(gl.COLOR_BUFFER_BIT);
      }
      R.live=true; R.left=0; R.drops=0; R.lastDrop=now;
    }
    if(rate>0) R.lastDrop=now;
    else if(now-R.lastDrop>RAIN_LINGER){ R.live=false; return; }   // the rings have run out
    gl.disable(gl.DEPTH_TEST); gl.disable(gl.BLEND); gl.depthMask(false);
    // The drops, into the current field - MGE's three ColorFills, as scissored clears.
    R.drops+=rate*dt;
    let n=Math.floor(R.drops); R.drops-=n;
    if(n>0){
      gl.bindFramebuffer(gl.FRAMEBUFFER,R.src.fb);
      gl.viewport(0,0,RAIN_RES,RAIN_RES);
      gl.enable(gl.SCISSOR_TEST);
      const rnd=()=>{   // MGE's own scrambler, rand() standing in as Math.random
        R.seed=R.seed*(1337.134511337451+0.0001*Math.floor(Math.random()*32768))+0.12351523;
        R.seed-=Math.floor(R.seed);
        return Math.floor(R.seed*RAIN_RES);
      };
      const fill=(l,t,r,b,v)=>{ gl.scissor(l,t,r-l,b-t); gl.clearColor(0,v,v,0); gl.clear(gl.COLOR_BUFFER_BIT); };
      const hi=0x60/255, lo=0x40/255;
      while(n-->0){
        const x=rnd(), y=rnd();
        fill(x-2,y-1,x+2,y+1,hi);
        fill(x-1,y-2,x+1,y+2,hi);
        fill(x-1,y-1,x+1,y+1,lo);
      }
      gl.disable(gl.SCISSOR_TEST);
    }
    // The steps, ping-ponged.
    R.left+=dt;
    const steps=Math.floor(R.left/RAIN_STEP);
    R.left-=steps*RAIN_STEP;
    if(steps>0){
      if(!this.progRainStep) this.progRainStep=this._prog(VS_BLIT,FS_RAIN_STEP);
      const pr=this.progRainStep;
      gl.useProgram(pr.p);
      gl.bindVertexArray(null);
      gl.viewport(0,0,RAIN_RES,RAIN_RES);
      gl.activeTexture(gl.TEXTURE0);
      gl.uniform1i(pr.u.uTex,0);
      for(let i=0;i<steps;i++){
        gl.bindFramebuffer(gl.FRAMEBUFFER,R.dst.fb);
        gl.bindTexture(gl.TEXTURE_2D,R.src.tex);
        gl.drawArrays(gl.TRIANGLES,0,3);
        const t=R.src; R.src=R.dst; R.dst=t;
      }
      gl.bindTexture(gl.TEXTURE_2D,null);
      this._texU=null;
    }
    gl.bindFramebuffer(gl.FRAMEBUFFER,null);
    gl.depthMask(true);
    this.rainDrawn=true;   // for the tests: the simulation ran this frame
  },
});
