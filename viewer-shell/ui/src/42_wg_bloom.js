
/* =====================================================================================
   Bloom - Wraithguard. A port of MGE XE's "Bloom Fine.fx" (Hrnchamd, v2; GPL-2.0 like
   the other MGE XE shaders, see ui/LICENSE), pass for pass and at its numbers.

   The glare of bright things: a lamp, a lit window, the sun on the water, a bright sky.
   Three passes, as the .fx has them:

   1. energyLevels - how much each pixel is over the threshold. The colour is pulled
      towards its own luminance by bloomColourSens (so one saturated channel does not
      bloom on its own), cut down with distance through the fog - "cut sky/distant
      illumination down", 1 - 0.45 * saturate(2 * (depth - fogstart) / (fogrange -
      fogstart)) with MGE's post-shader fog pair - and what is over bloomThreshold kept.
   2. blurVert - a seven-tap vertical blur, taps at 0, +-2.5, +-4.5 and +-6.5 pixels.
   3. blurHorzCombine - the same blur across, added onto the frame in a linear-ish space:
      pow(pow(base, gamma) + bloomLevel * radiance, 1 / gamma).

   Where it runs: MGE orders its post chain by category, and Bloom Fine is "sensor" -
   after the sunshafts ("atmosphere") and before the depth of field ("lens") and the
   FXAA. So here it takes the frame the sunshafts (or the plain copy, or the underwater
   pass) wrote, and hands on to the depth of field's target, the FXAA's, or the canvas
   (26_water.js endWaterFrame).

   A Preview switch (`opts.bloom`), saved in the profile; a profile that never stored it
   takes the install's post chain's answer, as SSAO and the sunshafts do, and otherwise
   starts off. MoMW's OpenMW setups have no MGE XE chain, so for them it is simply the
   switch.

   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page (the port of Hrnchamd's
   shader). (No backticks inside the shader strings: template literals.)
   ===================================================================================== */

/* Bloom Fine.fx's settings, verbatim. */
const BLOOM_THRESHOLD=0.48, BLOOM_LEVEL=0.23, BLOOM_COLOUR_SENS=0.39, BLOOM_GAMMA=2.45;

const FS_BLOOM_ENERGY=`#version 300 es
precision highp float;
in vec2 vUV;
uniform sampler2D uTex, uDepth;
uniform vec2 uNearFar;
uniform float uFogStart, uFogRange;
out vec4 o;
float linZ(float d){
  float z=d*2.0-1.0;
  return 2.0*uNearFar.x*uNearFar.y/(uNearFar.y+uNearFar.x-z*(uNearFar.y-uNearFar.x));
}
void main(){
  vec4 radiance = textureLod(uTex, vUV, 0.0);
  float depth = linZ(textureLod(uDepth, vUV, 0.0).r);
  float lum = dot(vec3(0.27, 0.54, 0.19), radiance.rgb);
  // Mix with luminance to avoid oversaturated single channels
  radiance = mix(vec4(lum), radiance, ${BLOOM_COLOUR_SENS});
  // Cut sky/distant illumination down
  radiance *= 1.0 - 0.45 * clamp(2.0 * (depth - uFogStart) / max(1.0, uFogRange - uFogStart), 0.0, 1.0);
  o = clamp(radiance - ${BLOOM_THRESHOLD}, 0.0, 1.0);
}`;

/* The seven taps and their weights, and the two blurs that use them. */
const BLOOM_BLUR_GLSL=`
const float K[7] = float[7](0.05, 0.14, 0.31, 0.22, 0.31, 0.14, 0.05);
const float P[7] = float[7](-6.5, -4.5, -2.5, 0.0, 2.5, 4.5, 6.5);
`;

const FS_BLOOM_VERT=`#version 300 es
precision highp float;
in vec2 vUV;
uniform sampler2D uPass;
uniform vec2 uRcpRes;
out vec4 o;
${BLOOM_BLUR_GLSL}
void main(){
  vec4 radiance = vec4(0.0);
  for(int i = 0; i < 7; ++i)
    radiance += K[i] * texture(uPass, vUV + vec2(0.0, uRcpRes.y * P[i]));
  o = radiance;
}`;

const FS_BLOOM_COMBINE=`#version 300 es
precision highp float;
in vec2 vUV;
uniform sampler2D uTex, uPass;
uniform vec2 uRcpRes;
out vec4 o;
${BLOOM_BLUR_GLSL}
void main(){
  vec4 base = textureLod(uTex, vUV, 0.0);
  vec4 radiance = vec4(0.0);
  for(int i = 0; i < 7; ++i)
    radiance += K[i] * texture(uPass, vUV + vec2(uRcpRes.x * P[i], 0.0));
  // Mix in linear space
  base = pow(max(base, vec4(0.0)), vec4(${BLOOM_GAMMA}));
  o = vec4(pow(base + ${BLOOM_LEVEL} * radiance, vec4(1.0 / ${BLOOM_GAMMA})).rgb, 1.0);
}`;

Object.assign(Renderer.prototype,{
  /** Whether this frame gets the bloom: the Preview switch. */
  bloomOn(){ return !!this.opts.bloom; },
  /** A full-size colour target of the bloom's own (S: the frame in; E: the energy; V: the
   *  vertical blur). Linear filtering, clamped - the blur reads between texels. */
  _bloomTarget(which,W,H){
    const gl=this.gl, key='_bloomT'+which, t=this[key];
    if(t && t.W===W && t.H===H) return t;
    if(t){ try{ gl.deleteFramebuffer(t.fb); }catch(_){ } try{ gl.deleteTexture(t.tex); }catch(_){ } }
    const n={W,H};
    n.tex=gl.createTexture(); gl.bindTexture(gl.TEXTURE_2D,n.tex);
    gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,W,H,0,gl.RGBA,gl.UNSIGNED_BYTE,null);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.CLAMP_TO_EDGE);
    n.fb=gl.createFramebuffer(); gl.bindFramebuffer(gl.FRAMEBUFFER,n.fb);
    gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,n.tex,0);
    n.ok=gl.checkFramebufferStatus(gl.FRAMEBUFFER)===gl.FRAMEBUFFER_COMPLETE;
    gl.bindFramebuffer(gl.FRAMEBUFFER,null); gl.bindTexture(gl.TEXTURE_2D,null);
    this[key]=n;
    return n;
  },
  /** The framebuffer the frame goes to when the bloom will run, or null. */
  bloomDest(W,H){
    if(!this.bloomOn()) return null;
    const t=this._bloomTarget('S',W,H);
    return t.ok? t.fb : null;
  },
  /** MGE's post-shader fog pair, `fogstart`/`fogrange`: the exponential fog's start and
   *  divisor (distantland.cpp updatePostShader), with the Fog density slider applied, as
   *  the depth of field reads it; with no fog to speak of, far enough to cut nothing. */
  bloomFogPair(){
    const p=this.dofFogPair? this.dofFogPair() : null;
    return p || [1e7, 2e7];
  },
  /** The three passes: the frame in the bloom's source, into `dst` (null: the canvas).
   *  `T` carries the depth the frame resolved. */
  drawBloom(T,dst){
    const gl=this.gl, src=this._bloomTS;
    if(!this.bloomOn() || !src || !src.ok) return false;
    const E=this._bloomTarget('E',T.W,T.H), V=this._bloomTarget('V',T.W,T.H);
    if(!E.ok || !V.ok) return false;
    if(!this.progBloomE){
      this.progBloomE=this._prog(VS_BLIT,FS_BLOOM_ENERGY);
      this.progBloomV=this._prog(VS_BLIT,FS_BLOOM_VERT);
      this.progBloomC=this._prog(VS_BLIT,FS_BLOOM_COMBINE);
    }
    gl.disable(gl.DEPTH_TEST); gl.disable(gl.BLEND); gl.depthMask(false);
    gl.bindVertexArray(null);
    const [fs,fr]=this.bloomFogPair();
    const [near,far]=this.nearFar();
    // 1. energyLevels
    let pr=this.progBloomE; gl.useProgram(pr.p);
    gl.bindFramebuffer(gl.FRAMEBUFFER,E.fb); gl.viewport(0,0,T.W,T.H);
    gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D,src.tex); gl.uniform1i(pr.u.uTex,0);
    gl.activeTexture(gl.TEXTURE1); gl.bindTexture(gl.TEXTURE_2D,T.depTex); gl.uniform1i(pr.u.uDepth,1);
    gl.uniform2f(pr.u.uNearFar,near,far);
    gl.uniform1f(pr.u.uFogStart,fs); gl.uniform1f(pr.u.uFogRange,fr);
    gl.drawArrays(gl.TRIANGLES,0,3);
    // 2. blurVert
    pr=this.progBloomV; gl.useProgram(pr.p);
    gl.bindFramebuffer(gl.FRAMEBUFFER,V.fb); gl.viewport(0,0,T.W,T.H);
    gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D,E.tex); gl.uniform1i(pr.u.uPass,0);
    gl.uniform2f(pr.u.uRcpRes,1/T.W,1/T.H);
    gl.drawArrays(gl.TRIANGLES,0,3);
    // 3. blurHorzCombine
    pr=this.progBloomC; gl.useProgram(pr.p);
    gl.bindFramebuffer(gl.FRAMEBUFFER,dst||null); gl.viewport(0,0,T.W,T.H);
    gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D,src.tex); gl.uniform1i(pr.u.uTex,0);
    gl.activeTexture(gl.TEXTURE1); gl.bindTexture(gl.TEXTURE_2D,V.tex); gl.uniform1i(pr.u.uPass,1);
    gl.uniform2f(pr.u.uRcpRes,1/T.W,1/T.H);
    gl.drawArrays(gl.TRIANGLES,0,3);
    gl.activeTexture(gl.TEXTURE1); gl.bindTexture(gl.TEXTURE_2D,null);
    gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D,null);
    this._texU=null;
    this.bloomDrawn=true;     // for the tests: this frame went through the pass
    return true;
  },
});
