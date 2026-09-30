/* =====================================================================================
   FXAA - round 18do, a port of the FXAA 3.11 shader Robin's game runs (Timothy Lottes,
   NVIDIA; J. Böttcher's MGE XE port 1.2, "FXAA.fx", the last entry of his post chain).

   Robin: "add the FXAA shader to the viewport too (together with the AA we already
   have)". The frame is drawn multisampled (4x, `_waterTargets`) as it was; this runs over
   the resolved picture on its way to the canvas, after the sunshafts, where MGE runs it
   after everything. A Preview switch, profile key `fxaa`; a profile that never stored it
   takes the install's chain's answer, as the occlusion and the sunshafts do (18i).

   The shader is the .fx's, number for number, with its own tuning kept: quality preset
   24 (seven search steps of 1, 1.5, 2, 2, 2, 3, 8 pixels - "less dither, more
   expensive"), sub-pixel removal 0.32, edge threshold 0.063, edge threshold minimum
   0.0312. Its first pass writes the frame's luma into alpha for the second to read; here
   the luma is worked out at each tap instead (the same dot product, 0.299/0.587/0.114),
   which saves a full-screen pass and changes no number. The unrolled `#if PS > n`
   cascade is a loop over the step table.

   Provenance, for the licence file: FXAA 3.11 is NVIDIA's, under its BSD-3 licence
   (License/FXAA/LICENSE, from the FXAA3_11.h NVIDIA publishes in its GameWorks samples); the MGE XE port is J. Böttcher's, posted for MGE XE
   by Hrnchamd; the tuning is Robin's copy. (No backticks inside the shader strings: template literals.) */

const FXAA_SUBPIX=0.32, FXAA_EDGE_THRESHOLD=0.063, FXAA_EDGE_THRESHOLD_MIN=0.0312;
const FXAA_STEPS=[1.0,1.5,2.0,2.0,2.0,3.0,8.0];   // preset 24

const FS_FXAA=`#version 300 es
precision highp float;
in vec2 vUV; uniform sampler2D uTex; uniform vec2 uRcpRes; out vec4 o;
const float SUBPIX=${FXAA_SUBPIX.toFixed(4)}, EDGE=${FXAA_EDGE_THRESHOLD.toFixed(4)}, EDGE_MIN=${FXAA_EDGE_THRESHOLD_MIN.toFixed(4)};
const int PS=${FXAA_STEPS.length};
const float P[${FXAA_STEPS.length}]=float[${FXAA_STEPS.length}](${FXAA_STEPS.map(v=>v.toFixed(1)).join(',')});
float luma(vec4 c){ return dot(c.rgb, vec3(0.299, 0.587, 0.114)); }
vec4 top(vec2 p){ return textureLod(uTex, p, 0.0); }
float lumaOff(vec2 p, vec2 off){ return luma(textureLod(uTex, p + off*uRcpRes, 0.0)); }
void main(){
  vec2 posM=vUV;
  vec4 rgbyM=top(posM);
  float lumaM=luma(rgbyM);
  float lumaS=lumaOff(posM, vec2( 0.0, 1.0));
  float lumaE=lumaOff(posM, vec2( 1.0, 0.0));
  float lumaN=lumaOff(posM, vec2( 0.0,-1.0));
  float lumaW=lumaOff(posM, vec2(-1.0, 0.0));
  float maxSM=max(lumaS, lumaM), minSM=min(lumaS, lumaM);
  float maxESM=max(lumaE, maxSM), minESM=min(lumaE, minSM);
  float maxWN=max(lumaN, lumaW), minWN=min(lumaN, lumaW);
  float rangeMax=max(maxWN, maxESM), rangeMin=min(minWN, minESM);
  float rangeMaxScaled=rangeMax*EDGE;
  float range=rangeMax-rangeMin;
  float rangeMaxClamped=max(EDGE_MIN, rangeMaxScaled);
  if(range < rangeMaxClamped){ o=vec4(rgbyM.rgb, 1.0); return; }
  float lumaNW=lumaOff(posM, vec2(-1.0,-1.0));
  float lumaSE=lumaOff(posM, vec2( 1.0, 1.0));
  float lumaNE=lumaOff(posM, vec2( 1.0,-1.0));
  float lumaSW=lumaOff(posM, vec2(-1.0, 1.0));
  float lumaNS=lumaN+lumaS, lumaWE=lumaW+lumaE;
  float subpixRcpRange=1.0/range;
  float subpixNSWE=lumaNS+lumaWE;
  float edgeHorz1=(-2.0*lumaM)+lumaNS, edgeVert1=(-2.0*lumaM)+lumaWE;
  float lumaNESE=lumaNE+lumaSE, lumaNWNE=lumaNW+lumaNE;
  float edgeHorz2=(-2.0*lumaE)+lumaNESE, edgeVert2=(-2.0*lumaN)+lumaNWNE;
  float lumaNWSW=lumaNW+lumaSW, lumaSWSE=lumaSW+lumaSE;
  float edgeHorz4=(abs(edgeHorz1)*2.0)+abs(edgeHorz2), edgeVert4=(abs(edgeVert1)*2.0)+abs(edgeVert2);
  float edgeHorz3=(-2.0*lumaW)+lumaNWSW, edgeVert3=(-2.0*lumaS)+lumaSWSE;
  float edgeHorz=abs(edgeHorz3)+edgeHorz4, edgeVert=abs(edgeVert3)+edgeVert4;
  float subpixNWSWNESE=lumaNWSW+lumaNESE;
  float lengthSign=uRcpRes.x;
  bool horzSpan=edgeHorz>=edgeVert;
  float subpixA=subpixNSWE*2.0+subpixNWSWNESE;
  if(!horzSpan){ lumaN=lumaW; lumaS=lumaE; }
  if(horzSpan) lengthSign=uRcpRes.y;
  float subpixB=(subpixA*(1.0/12.0))-lumaM;
  float gradientN=lumaN-lumaM, gradientS=lumaS-lumaM;
  float lumaNN=lumaN+lumaM, lumaSS=lumaS+lumaM;
  bool pairN=abs(gradientN)>=abs(gradientS);
  float gradient=max(abs(gradientN), abs(gradientS));
  if(pairN) lengthSign=-lengthSign;
  float subpixC=clamp(abs(subpixB)*subpixRcpRange, 0.0, 1.0);
  vec2 posB=posM;
  vec2 offNP=vec2(horzSpan? uRcpRes.x : 0.0, horzSpan? 0.0 : uRcpRes.y);
  if(!horzSpan) posB.x+=lengthSign*0.5;
  if( horzSpan) posB.y+=lengthSign*0.5;
  vec2 posN=posB-offNP*P[0];
  vec2 posP=posB+offNP*P[0];
  float subpixD=(-2.0*subpixC)+3.0;
  float lumaEndN=luma(top(posN));
  float subpixE=subpixC*subpixC;
  float lumaEndP=luma(top(posP));
  if(!pairN) lumaNN=lumaSS;
  float gradientScaled=gradient*(1.0/4.0);
  float lumaMM=lumaM-lumaNN*0.5;
  float subpixF=subpixD*subpixE;
  bool lumaMLTZero=lumaMM<0.0;
  lumaEndN-=lumaNN*0.5;
  lumaEndP-=lumaNN*0.5;
  bool doneN=abs(lumaEndN)>=gradientScaled;
  bool doneP=abs(lumaEndP)>=gradientScaled;
  if(!doneN) posN-=offNP*P[1];
  bool doneNP=(!doneN)||(!doneP);
  if(!doneP) posP+=offNP*P[1];
  /* The .fx's cascade of "if(doneNP) { ... }" blocks, one per remaining step. */
  for(int i=2; i<PS; i++){
    if(!doneNP) break;
    if(!doneN) lumaEndN=luma(top(posN));
    if(!doneP) lumaEndP=luma(top(posP));
    if(!doneN) lumaEndN=lumaEndN-lumaNN*0.5;
    if(!doneP) lumaEndP=lumaEndP-lumaNN*0.5;
    doneN=abs(lumaEndN)>=gradientScaled;
    doneP=abs(lumaEndP)>=gradientScaled;
    if(!doneN) posN-=offNP*P[i];
    doneNP=(!doneN)||(!doneP);
    if(!doneP) posP+=offNP*P[i];
  }
  float dstN=posM.x-posN.x, dstP=posP.x-posM.x;
  if(!horzSpan){ dstN=posM.y-posN.y; dstP=posP.y-posM.y; }
  bool goodSpanN=(lumaEndN<0.0)!=lumaMLTZero;
  float spanLength=dstP+dstN;
  bool goodSpanP=(lumaEndP<0.0)!=lumaMLTZero;
  float spanLengthRcp=1.0/spanLength;
  bool directionN=dstN<dstP;
  float dst=min(dstN, dstP);
  bool goodSpan=directionN? goodSpanN : goodSpanP;
  float subpixG=subpixF*subpixF;
  float pixelOffset=(dst*(-spanLengthRcp))+0.5;
  float subpixH=subpixG*SUBPIX;
  float pixelOffsetGood=goodSpan? pixelOffset : 0.0;
  float pixelOffsetSubpix=max(pixelOffsetGood, subpixH);
  if(!horzSpan) posM.x+=pixelOffsetSubpix*lengthSign;
  if( horzSpan) posM.y+=pixelOffsetSubpix*lengthSign;
  o=vec4(top(posM).rgb, 1.0);
}`;

Object.assign(Renderer.prototype,{
  /** The full-size target the finished frame is drawn into when FXAA is on, for the
   *  pass to read; clamped and linear, as the .fx's sampler. Rebuilt with the frame. */
  _fxaaTarget(W,H){
    const gl=this.gl, t=this._fxaaT;
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
    this._fxaaT=n;
    return n;
  },
  /** The framebuffer the finished frame goes to: the FXAA target when the pass is on and
   *  can be had, else the canvas (null). `endWaterFrame` asks, draws there, then calls
   *  `drawFXAA` for the last step. */
  fxaaDest(W,H){
    if(!this.opts.fxaa) return null;
    const t=this._fxaaTarget(W,H);
    return t.ok? t.fb : null;
  },
  /** The pass itself: the frame in the FXAA target, onto the canvas. */
  drawFXAA(W,H){
    const gl=this.gl, t=this._fxaaT;
    if(!t || !t.ok) return false;
    if(!this.progFXAA) this.progFXAA=this._prog(VS_BLIT,FS_FXAA);
    const pr=this.progFXAA; gl.useProgram(pr.p);
    gl.bindFramebuffer(gl.FRAMEBUFFER,null); gl.viewport(0,0,W,H);
    gl.disable(gl.DEPTH_TEST); gl.disable(gl.BLEND); gl.depthMask(false);
    gl.bindVertexArray(null);
    gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D,t.tex);
    gl.uniform1i(pr.u.uTex,0); gl.uniform2f(pr.u.uRcpRes,1/W,1/H);
    gl.drawArrays(gl.TRIANGLES,0,3);
    gl.bindTexture(gl.TEXTURE_2D,null);
    this.fxaaDrawn=true;      // for the tests: this frame went through the pass
    return true;
  },
});
