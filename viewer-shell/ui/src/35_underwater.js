/* =====================================================================================
   Under the water - round 18dt.

   Robin: "whenever the user is below the water surface, within the rectangle it sits on,
   use under water effects as they do in the game. Look at OpenMW, MGE XE G7's source
   code, and these shaders to get help" - Hrnchamd's Underwater Effects.fx (exteriors:
   the wobble and the light rays) and Underwater Interior Effects.fx (the wobble and the
   caustics), the two post shaders in his chain. And, asked: post effects always on, the
   MGE surface shader for every install, no SSAO and no sunshafts under water, the fog
   range and colour by install, the grass drawn, the moths not.

   What "under the water" is and what the fog is there: `underwaterAt` in 06_gl.js and
   `underwaterFog` in 25_sky.js. The surface seen from beneath: `FS_WATER_MGE`'s uBelow
   branch in 26_water.js. This part is the post pass alone - the last thing before the
   outline and the FXAA, in the plain copy's place (`endWaterFrame`):

   1. The wobble, both shaders alike: the frame read through an offset from the wave
      volume's red and green, 0.01 of the screen, fading to nothing at the edges by
      (2t-1)^32 so the picture's border stays put.
   2. Outside: light rays. Three shells at 800, 1400 and 2000 units along the eye's ray,
      shifted sideways by the sun's direction across the sky, each sampling the volume's
      blue channel (pow 0.25) fading with depth below the surface (exp(z/90)), gated by
      the frame's depth so nothing shines past a wall, weighted 0.294 / 0.132 / 0.065,
      in the shader's own light colour (0.6, 0.91, 1.0) scaled by the sun's visibility
      and the sun colour's luma. Added to the frame.
   3. Inside a room: caustics that darken. The world position under each pixel from the
      depth; below the water line (and within a hundred units of it) the frame is dimmed
      by up to a fifth where the volume's blue is dark.

   The volume is MGE's `water_NRM.dds` (26_water.js `waterVolume`); an install without
   it (OpenMW, vanilla) gets a value noise in its place, so the effects stand there too.
   The depth is the frame's resolved depth, linearised to the view axis, which is what
   MGE's depth frame holds and what `toWorld` (the shader's own) expects: the eye's ray
   with a forward component of one. (No backticks inside the shader: template literal.) */

const FS_UNDERWATER=`#version 300 es
precision highp float;
precision highp sampler3D;
in vec2 vUV;
uniform sampler2D uTex, uDepth;
uniform sampler3D uWater3d; uniform int uHas3d;
uniform vec3 uCamF, uCamR, uCamU, uEyeP, uSunP, uSunColP;
uniform float uTanH, uAspect, uSunVisP, uTimeP, uWaterZ;
uniform vec2 uNearFar;
uniform int uInterior;
/* Round 18dz: the eye's height over the water while it is crossing the line, and nought
   once it is under. Over the line the pass still runs - Robin: "run the full effect of
   being under water on all the pixels that are beneath the water surface when the surface
   cuts the near clipping plane" - but only what is under the line takes it, so the sky
   and the shore in the same frame are left alone. */
uniform float uCrossTop;
uniform vec4 uBox;     // 18du: the water's footprint (centre x, y, half, on) - the rays stop at its walls
out vec4 o;
float linZ(float d){
  float z=d*2.0-1.0;
  return 2.0*uNearFar.x*uNearFar.y/(uNearFar.y+uNearFar.x-z*(uNearFar.y-uNearFar.x));
}
/* The eye's ray through a screen point, forward component one: eye + depth * toWorld(t). */
vec3 toWorld(vec2 t){
  return uCamF + uCamR*((2.0*t.x-1.0)*uTanH*uAspect) + uCamU*((2.0*t.y-1.0)*uTanH);
}
/* 1 where this pixel takes the underwater pass. Everywhere with the eye under the line;
   during the crossing, only where what the pixel shows stands under it. Read off the
   *undistorted* pixel, so the mask does not chase the wobble it is masking. */
float crossMask(){
  if(uCrossTop <= 0.0) return 1.0;
  float dd=linZ(texture(uDepth,vUV).r);
  vec3 wp=uEyeP + dd*toWorld(vUV);
  return step(wp.z, uWaterZ);
}
/* A value noise standing in for the volume where the install has none. */
float hash3(vec3 p){ p=fract(p*0.3183099+vec3(0.1,0.2,0.3)); p*=17.0; return fract(p.x*p.y*p.z*(p.x+p.y+p.z)); }
float vnoise(vec3 x){
  vec3 i=floor(x), f=fract(x); f=f*f*(3.0-2.0*f);
  return mix(mix(mix(hash3(i),hash3(i+vec3(1,0,0)),f.x), mix(hash3(i+vec3(0,1,0)),hash3(i+vec3(1,1,0)),f.x),f.y),
             mix(mix(hash3(i+vec3(0,0,1)),hash3(i+vec3(1,0,1)),f.x), mix(hash3(i+vec3(0,1,1)),hash3(i+vec3(1,1,1)),f.x),f.y),f.z);
}
vec4 vol(vec3 p){
  if(uHas3d==1) return texture(uWater3d,p);
  return vec4(vnoise(p*8.0), vnoise(p*8.0+vec3(3.7,1.3,2.9)), vnoise(p*6.0+vec3(9.1,4.2,7.7)), 0.5);
}
float fetch(vec3 t, float uw){
  float c=vol(vec3(t.xy/3308.0, 0.22*uTimeP)).b;
  return pow(max(c,0.0),0.25)*clamp(exp(t.z/90.0),0.0,1.0)*clamp(1.0+t.z/uw,0.0,1.0);
}
void main(){
  vec2 tex=vUV;
  vec2 wob=0.01*(2.0*vol(vec3(0.25*tex, 0.2*uTimeP)).rg-1.0);
  vec2 e=2.0*tex-1.0; vec2 e32=e*e; e32*=e32; e32*=e32; e32*=e32; e32*=e32;   // (2t-1)^32
  wob*=1.0-e32;
  tex=clamp(tex+wob, vec2(0.0), vec2(1.0));
  /* 18du: where the ray leaves the water's square sideways, in the same depth units as
     the frame's (t along the ray, whose forward component is one). Past it the water is
     not there: no wobble on what is seen through the wall, and the rays end at it. */
  float tExit=1e9;
  if(uBox.w>0.0){
    vec3 v0=toWorld(vUV);
    float tx = v0.x>1e-6 ? (uBox.x+uBox.z-uEyeP.x)/v0.x : v0.x<-1e-6 ? (uBox.x-uBox.z-uEyeP.x)/v0.x : 1e9;
    float ty = v0.y>1e-6 ? (uBox.y+uBox.z-uEyeP.y)/v0.y : v0.y<-1e-6 ? (uBox.y-uBox.z-uEyeP.y)/v0.y : 1e9;
    tExit=max(0.0,min(tx,ty));
    if(linZ(texture(uDepth,vUV).r)>tExit) tex=vUV;   // seen past the wall: the picture stands still there
  }
  vec3 v=toWorld(tex);
  vec4 c=texture(uTex,tex);
  float d=min(linZ(texture(uDepth,tex).r), tExit);
  if(uInterior==1){
    vec3 w=uEyeP + d*v;
    float k=1.0-vol(vec3(w.xy/1783.0, 0.4*uTimeP)).b;
    c.rgb*=1.0-0.2*step(w.z, uWaterZ-1.0)*clamp(exp((w.z-uWaterZ)/100.0),0.0,1.0)*k;
    o=vec4(mix(texture(uTex,vUV).rgb, c.rgb, crossMask()),1.0);
    return;
  }
  // MGE's eyepos.z is the eye's height over a sea at zero; here, over the water line.
  float ez=uEyeP.z-uWaterZ;
  vec3 e3=vec3(0.1*uEyeP.x, 0.1*uEyeP.y, ez);
  d=min(d, -ez/max(1e-5, v.z));
  v=v/max(1e-5,length(v.xy));
  vec2 r=-uSunP.xy - v.xy*dot(-uSunP.xy, v.xy);
  vec3 s1=e3+800.0*v;  s1.xy+=0.8*r*s1.z;
  float rayz=fetch(s1, min(ez,400.0))*clamp(d/800.0,0.0,1.0)*0.294;
  vec3 s2=e3+1400.0*v; s2.xy+=1.0*r*s2.z;
  rayz+=fetch(s2, 2.0*ez)*clamp(d/800.0-1.0,0.0,1.0)*0.132;
  vec3 s3=e3+2000.0*v; s3.xy+=1.2*r*s3.z;
  rayz+=fetch(s3, 3.0*ez)*clamp(d/800.0-2.0,0.0,1.0)*0.065;
  float lf=(0.2+0.8*uSunVisP)*(0.32*uSunColP.r+0.47*uSunColP.g+0.21*uSunColP.b);
  vec3 lit = c.rgb + rayz*lf*vec3(0.6,0.91,1.0);
  o=vec4(mix(texture(uTex,vUV).rgb, lit, crossMask()), 1.0);
}`;

/* Round 18du: MGE XE's water caustics - `XE Mod Caustics.fx`, a core shader, drawn in
   `renderStageBlend` before the water plane in every exterior with a caustics intensity
   above zero (`distant_land.water.caustics_intensity`, `Water Caustics Intensity`; the
   engine reports it as `renderer.water.caustics`), from above the water and from under it
   alike. Over the frame: for each pixel under the water line, the point is carried up to
   the surface along the sun's direction, the volume's blue channel read there (the same
   pattern the Interior Effects shader reads for its caustics), and the colour scaled by
   `1 + (caust - 0.3) * exp(z/400) * saturate(z/-30) * fog` - brighter where the pattern
   is bright, a little darker where it is not, fading with depth below the surface, gone
   with the fog, with `caust = 0.05 * intensity * saturate(0.75 * sunlightFactor + 0.35 *
   |fogColFar|) * blue`, and a `fwidth` term that fades the pattern where it would alias.
   Applied to the resolved frame and written back into the scene, so the surface refracts
   the caustic bottom the way MGE's does (its water samples the frame after this pass). */
const FS_CAUSTICS=`#version 300 es
precision highp float;
precision highp sampler3D;
in vec2 vUV;
uniform sampler2D uTex, uDepth;
uniform sampler3D uWater3d; uniform int uHas3d;
uniform vec3 uCamF, uCamR, uCamU, uEyeP, uSunV;
uniform float uTanH, uAspect, uTimeP, uWaterZ, uStrength, uSunVisP;
uniform vec2 uNearFar;
uniform vec3 uFogCol; uniform float uFogK;
${SKY_GLSL}
out vec4 o;
float linZ(float d){
  float z=d*2.0-1.0;
  return 2.0*uNearFar.x*uNearFar.y/(uNearFar.y+uNearFar.x-z*(uNearFar.y-uNearFar.x));
}
vec3 toWorld(vec2 t){
  return uCamF + uCamR*((2.0*t.x-1.0)*uTanH*uAspect) + uCamU*((2.0*t.y-1.0)*uTanH);
}
float hash3(vec3 p){ p=fract(p*0.3183099+vec3(0.1,0.2,0.3)); p*=17.0; return fract(p.x*p.y*p.z*(p.x+p.y+p.z)); }
float vnoise(vec3 x){
  vec3 i=floor(x), f=fract(x); f=f*f*(3.0-2.0*f);
  return mix(mix(mix(hash3(i),hash3(i+vec3(1,0,0)),f.x), mix(hash3(i+vec3(0,1,0)),hash3(i+vec3(1,1,0)),f.x),f.y),
             mix(mix(hash3(i+vec3(0,0,1)),hash3(i+vec3(1,0,1)),f.x), mix(hash3(i+vec3(0,1,1)),hash3(i+vec3(1,1,1)),f.x),f.y),f.z);
}
float volB(vec3 p){ return uHas3d==1 ? texture(uWater3d,p).b : vnoise(p*6.0+vec3(9.1,4.2,7.7)); }
void main(){
  vec3 c=texture(uTex,vUV).rgb;
  float d=linZ(texture(uDepth,vUV).r);
  vec3 w=uEyeP + d*toWorld(vUV);
  float uz=w.z-uWaterZ;
  if(uz>=0.0 || d>=uNearFar.y*0.999){ o=vec4(c,1.0); return; }
  float fog=airClear(w);
  vec3 sunray=vec3(w.xy, uz) - uSunV*(uz/uSunV.z);
  float sunF=1.0-pow(1.0-uSunVisP,2.0);
  vec3 far = uUnder==1 ? uUnderCol : (uScat==1 ? uFogColFar : uFogCol);
  float strength=0.05*uStrength*clamp(0.75*sunF+0.35*length(far),0.0,1.0);
  float caust=strength*volB(vec3(sunray.xy/1104.0, 0.4*uTimeP));
  caust*=clamp(125.0/max(d,1.0)*min(fwidth(sunray.x),fwidth(sunray.y)),0.0,1.0);
  c*=1.0+(caust-0.3)*clamp(exp(uz/400.0),0.0,1.0)*clamp(uz/-30.0,0.0,1.0)*fog;
  o=vec4(c,1.0);
}`;

Object.assign(Renderer.prototype,{
  /** Whether this frame gets MGE's caustics: an exterior with water, the MGE water on, and
   *  an MGE install whose caustics intensity is above zero (the switch is the game's). */
  wantsCaustics(){
    const o=this.opts;
    if(!this.water || o.mgeWater===false || o.room || o.underwater===false) return false;
    const d=(typeof Sky==='object' && Sky.data)||null;
    const r=d && d.renderer;
    if(!r || r.engine!=='mgexe' || !r.water) return false;
    const k=+r.water.caustics;
    return isFinite(k) && k>0;
  },
  causticsIntensity(){
    const d=(typeof Sky==='object' && Sky.data)||null;
    const r=d && d.renderer; const k=r && r.water? +r.water.caustics : 0;
    return isFinite(k)? k : 0;
  },
  /** The caustics over the resolved frame, back into the scene (`dst`, the multisampled
   *  scene target) before the surface is drawn. `airOn` sets the fog the pass fades by. */
  drawCaustics(T,f,dst,airOn){
    const gl=this.gl;
    if(!f || !this.wantsCaustics()) return false;
    if(!this.progCaustics) this.progCaustics=this._prog(VS_BLIT,FS_CAUSTICS);
    const pr=this.progCaustics; gl.useProgram(pr.p);
    gl.bindFramebuffer(gl.FRAMEBUFFER,dst); gl.viewport(0,0,T.W,T.H);
    gl.disable(gl.DEPTH_TEST); gl.disable(gl.BLEND); gl.depthMask(false);
    gl.bindVertexArray(null);
    gl.uniform3fv(pr.u.uFogCol,this.backdrop()); gl.uniform1f(pr.u.uFogK,f.fogK||0);
    airOn(pr);
    gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D,T.colTex); gl.uniform1i(pr.u.uTex,0);
    gl.activeTexture(gl.TEXTURE1); gl.bindTexture(gl.TEXTURE_2D,T.depTex); gl.uniform1i(pr.u.uDepth,1);
    const vol=this.waterVolume();
    gl.activeTexture(gl.TEXTURE2);
    gl.bindTexture(gl.TEXTURE_3D, vol||this._vol1||(this._vol1=this.makeVolume(1,1,1,new Uint8Array([128,128,255,128]))));
    gl.uniform1i(pr.u.uWater3d,2); gl.uniform1i(pr.u.uHas3d,vol?1:0);
    gl.activeTexture(gl.TEXTURE0); this._texU=null;
    const V=f.V;
    gl.uniform3f(pr.u.uCamR,V[0],V[4],V[8]);
    gl.uniform3f(pr.u.uCamU,V[1],V[5],V[9]);
    gl.uniform3f(pr.u.uCamF,-V[2],-V[6],-V[10]);
    gl.uniform3fv(pr.u.uEyeP,f.eye);
    const [near,far]=this.nearFar(); gl.uniform2f(pr.u.uNearFar,near,far);
    gl.uniform1f(pr.u.uTanH,Math.tan(FOV/2)); gl.uniform1f(pr.u.uAspect,T.W/T.H);
    const sv=f.sun||[0.3,0.2,0.93];
    gl.uniform3f(pr.u.uSunV, sv[0], sv[1], Math.max(0.05, sv[2]));
    gl.uniform1f(pr.u.uSunVisP, f.now? f.now.sunVis : 1);
    gl.uniform1f(pr.u.uTimeP,(performance.now()/1000)%3600);
    gl.uniform1f(pr.u.uWaterZ,this.water.z);
    gl.uniform1f(pr.u.uStrength,this.causticsIntensity());
    gl.drawArrays(gl.TRIANGLES,0,3);
    gl.activeTexture(gl.TEXTURE1); gl.bindTexture(gl.TEXTURE_2D,null);
    gl.activeTexture(gl.TEXTURE2); gl.bindTexture(gl.TEXTURE_3D,null);
    gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D,null);
    gl.enable(gl.DEPTH_TEST); gl.depthMask(true);
    this.causticsDrawn=true;
    return true;
  },
  /** The post pass, from the resolved frame in `T` into `dst` (null: the canvas). Only
   *  with the eye under the water (`underNow`); returns false otherwise and the plain
   *  copy is drawn. `f` is the frame's own matrices and sun. */
  drawUnderwater(T,f,dst){
    const gl=this.gl, u=this.underFogNow||this.underNow;
    if(!u || !f || !this.water) return false;
    if(!this.progUnder) this.progUnder=this._prog(VS_BLIT,FS_UNDERWATER);
    const pr=this.progUnder; gl.useProgram(pr.p);
    gl.bindFramebuffer(gl.FRAMEBUFFER,dst||null); gl.viewport(0,0,T.W,T.H);
    gl.disable(gl.DEPTH_TEST); gl.disable(gl.BLEND); gl.depthMask(false);
    gl.bindVertexArray(null);
    gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D,T.colTex); gl.uniform1i(pr.u.uTex,0);
    gl.activeTexture(gl.TEXTURE1); gl.bindTexture(gl.TEXTURE_2D,T.depTex); gl.uniform1i(pr.u.uDepth,1);
    const vol=this.waterVolume();
    gl.activeTexture(gl.TEXTURE2);
    gl.bindTexture(gl.TEXTURE_3D, vol||this._vol1||(this._vol1=this.makeVolume(1,1,1,new Uint8Array([128,128,255,128]))));
    gl.uniform1i(pr.u.uWater3d,2); gl.uniform1i(pr.u.uHas3d,vol?1:0);
    gl.activeTexture(gl.TEXTURE0); this._texU=null;
    // The pass's basis out of its view matrix (M4.lookAt, column-major: right, up, back).
    const V=f.V;
    gl.uniform3f(pr.u.uCamR,V[0],V[4],V[8]);
    gl.uniform3f(pr.u.uCamU,V[1],V[5],V[9]);
    gl.uniform3f(pr.u.uCamF,-V[2],-V[6],-V[10]);
    gl.uniform3fv(pr.u.uEyeP,f.eye);
    const [near,far]=this.nearFar(); gl.uniform2f(pr.u.uNearFar,near,far);
    gl.uniform1f(pr.u.uTanH,Math.tan(FOV/2)); gl.uniform1f(pr.u.uAspect,T.W/T.H);
    const now=f.now;
    gl.uniform3fv(pr.u.uSunP, now? now.sunPos : [0.3,0.2,0.93]);
    /* Round 18dy: the sun the rays are worked out from is the one the game hands its post
       shaders - `SetFloatArray(EV_suncol, sunCol, 3)`, and `sunCol` is the Direct3D sun
       light's diffuse, which under the water is the blended colour (18dx), not the
       weather's. Robin: "The light pillar shader effect that is happening under water is
       much too strong in Gardenfell compared to my game." It was: the shader's `lf` is
       the luminance of that colour, 0.98 for a clear day unblended and 0.23 blended, so
       the rays were running four times over. No number in the port changed. */
    const sun0 = now? now.sun : [1,1,1];
    const sunPost = (typeof Sky==='object' && Sky.underwaterTint)? Sky.underwaterTint(sun0) : sun0;
    gl.uniform3fv(pr.u.uSunColP, sunPost);
    gl.uniform1f(pr.u.uSunVisP, now? now.sunVis : 1);
    gl.uniform1f(pr.u.uTimeP,(performance.now()/1000)%3600);
    gl.uniform1f(pr.u.uWaterZ,this.water.z);
    /* 18dz: nought with the eye under the line, its height over it while it crosses. */
    gl.uniform1f(pr.u.uCrossTop, (u && u.over)? Math.max(0, f.eye[2]-this.water.z) : 0);
    gl.uniform1i(pr.u.uInterior, this.opts.room? 1 : 0);
    gl.uniform4f(pr.u.uBox, this.water.centre[0], this.water.centre[1], this.water.half, 1);
    gl.drawArrays(gl.TRIANGLES,0,3);
    gl.activeTexture(gl.TEXTURE1); gl.bindTexture(gl.TEXTURE_2D,null);
    gl.activeTexture(gl.TEXTURE2); gl.bindTexture(gl.TEXTURE_3D,null);
    gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D,null);
    return true;
  },
});
