/* =====================================================================================
   Depth of field - round 18dy, a port of "Depth of Field.fx" (v12 by Knu, tweaked by
   peachykeen; MGE XE 0), the entry second from last in Robin's own shader chain.

   Robin: "I get a nice blur effect from my depth of field effect too, that comes from its
   distance blur in fog conditions. Look through that and see if we can implement it. Add
   the DOF as a toggle, but I always want the underwater blur in fog no matter if the
   other behaviors for DOF is active or not."

   The shader adds two blurs into one circle of confusion and then blurs by it:

     1. **The eye's own.** It models an eye focused on whatever is at the middle of the
        screen: `s` is that pixel's distance in metres, `fpf = clamp(1/s + fr, fp, fp+fpa)`
        the focal power it accommodates to, and `c = pupil * (fr - fpf + 1/z) / fr / k *
        blur_radius` the blur circle for a pixel at distance z. `savemyhands` keeps
        anything nearer than about 0.6 m sharp, which is what the name says it is for.
     2. **The distance blur in fog conditions**, which is the part Robin is after:
        `fogoffset = saturate(-fogstart / (fogrange - fogstart))` is nought whenever the
        fog starts at a positive distance and rises as the fog starts *behind* the eye,
        and `fog = fogoffset * saturate(z / (4 * fogrange * unit2m))` grows with distance
        up to it. Above the water his `above_water_start` is 2.32 cells, so `fogoffset` is
        nought and this term does nothing at all; under the water MGE's below-water start
        is -0.2 cells, so it is 0.19 there and the far field softens. The fog does it, not
        the water - it is simply that only the water's fog starts behind the eye.

   So the switch turns 1 on and off, and 2 runs whenever the fog asks for it, switch or
   no: `dofMode()` returns 'full', 'fog' or null. The fog pair is the one MGE hands its
   post shaders - `fogstart`/`fogrange` are `isExpFog ? fogExpStart : fogStart` and
   `isExpFog ? fogExpDivisor : fogEnd` (distantland.cpp, updatePostShader) - which under
   the water is the below-water pair through the same 4.4 rescaling.

   Two passes, as the .fx has them: one writes the frame with the blur radius in alpha,
   the other spreads twelve taps around a disc, weighting each by how near its own radius
   is to the middle one so a sharp thing in front of a soft background keeps its edge.
   The random rotation reads `MGE/noise64.dds` there and a hash here, which is the same
   idea without the file. Its constants are kept number for number.

   (No backticks inside the shader strings: template literals.) */

const DOF_FR=60.0, DOF_FP=60.0, DOF_FPA=10.0, DOF_PUPIL=0.006;
const DOF_BLUR_RADIUS=0.273, DOF_BLUR_FALLOFF=2.0, DOF_R=6.0;
const DOF_K=0.00001, DOF_UNIT2M=0.0142;
const DOF_TAPS=[[-0.326212,-0.40581],[-0.840144,-0.07358],[-0.695914,0.457137],[-0.203345,0.620716],
                [0.96234,-0.194983],[0.473434,-0.480026],[0.519456,0.767022],[0.185461,-0.893124],
                [0.507431,0.064425],[0.89642,0.412458],[-0.32194,-0.932615],[-0.791559,-0.59771]];

const FS_DOF_COC=`#version 300 es
precision highp float;
in vec2 vUV;
uniform sampler2D uTex, uDepth;
uniform vec2 uNearFar, uRcpRes;
uniform float uTanH, uAspect, uFogStart, uFogRange;
uniform int uFull;          // 1: the eye's blur as well; 0: the fog's distance blur alone
out vec4 o;
float linZ(float d){
  float z=d*2.0-1.0;
  return 2.0*uNearFar.x*uNearFar.y/(uNearFar.y+uNearFar.x-z*(uNearFar.y-uNearFar.x));
}
void main(){
  /* Rfixed = R / (1280 * rcpres.x) in the .fx, and rcpres.x is 1/width, so this is
     R * width / 1280 - the blur grows with the resolution rather than shrinking with
     it. Round 18dy had the reciprocal, which at 1920 wide gave 4 pixels where the game
     gives 9, and Robin: "The blur far away in the fog when under water doesn't seem to be
     as strong as in the game." It was 2.25 times short there, and more the wider the
     window. The taps below are measured in pixels and scaled back into UV by rcpres. */
  float Rfixed = ${DOF_R.toFixed(1)} / max(1280.0 * uRcpRes.x, 1e-6);
  float fogoffset = clamp(-uFogStart / max(1e-4, uFogRange - uFogStart), 0.0, 1.0);
  float s = linZ(textureLod(uDepth, vec2(0.5, 0.5), 0.0).r) * ${DOF_UNIT2M};
  vec2 p = vUV*2.0-1.0;
  float z_corr = length(vec3(p.x*uTanH*uAspect, p.y*uTanH, 1.0));
  float z = z_corr * ${DOF_UNIT2M} * linZ(textureLod(uDepth, vUV, 0.0).r);
  float savemyhands = smoothstep(0.568, 0.781, z);
  float c = 0.0;
  if(uFull == 1){
    float fpf = clamp(1.0/max(s,1e-4) + ${DOF_FR.toFixed(1)}, ${DOF_FP.toFixed(1)}, ${(DOF_FP+DOF_FPA).toFixed(1)});
    c = ${DOF_PUPIL} * (${DOF_FR.toFixed(1)} - fpf + 1.0/max(z,1e-4)) / ${DOF_FR.toFixed(1)} / ${DOF_K} * ${DOF_BLUR_RADIUS};
    c = abs(c / max(Rfixed, 1e-4));
  }
  float fog = fogoffset * clamp(z / max(1e-4, 4.0 * uFogRange * ${DOF_UNIT2M}), 0.0, 1.0);
  c = min(c + fog, 1.0) * savemyhands;
  o = vec4(textureLod(uTex, vUV, 0.0).rgb, c);
}`;

const FS_DOF_BLUR=`#version 300 es
precision highp float;
in vec2 vUV;
uniform sampler2D uTex;      // the pass above: colour, blur radius in alpha
uniform vec2 uRcpRes;
out vec4 o;
const int M=${DOF_TAPS.length};
const vec2 TAPS[${DOF_TAPS.length}]=vec2[${DOF_TAPS.length}](${DOF_TAPS.map(t=>'vec2('+t[0]+','+t[1]+')').join(',')});
float hash21(vec2 p){ p=fract(p*vec2(123.34,456.21)); p+=dot(p,p+45.32); return fract(p.x*p.y); }
void main(){
  float Rfixed = ${DOF_R.toFixed(1)} / max(1280.0 * uRcpRes.x, 1e-6);
  vec4 color = textureLod(uTex, vUV, 0.0);
  float c = color.a * Rfixed;
  /* The .fx reflects each tap about a vector out of MGE's noise texture; a hash of the
     pixel does the same job - a different rotation per pixel, so the twelve taps do not
     line up into a pattern. */
  float a = hash21(floor(vUV / max(uRcpRes.x, 1e-6) * vec2(1.0, 1.0))) * 6.2831853;
  vec2 rnd = vec2(cos(a), sin(a));
  float amount = 1.0;
  for(int i=0;i<M;i++){
    vec2 dir = reflect(TAPS[i], rnd);
    vec4 s_color = textureLod(uTex, vUV + uRcpRes*dir*c, 0.0);
    float s_c = s_color.a * Rfixed;
    float weight = exp2(-abs(c - s_c) / ${DOF_BLUR_FALLOFF.toFixed(1)});
    color += s_color * weight;
    amount += weight;
  }
  o = vec4(color.rgb / amount, 1.0);
}`;

Object.assign(Renderer.prototype,{
  /** What the pass should do this frame, or null for nothing.
   *
   *  'full' with the switch on. 'fog' with it off but the fog starting behind the eye,
   *  which is the distance blur Robin wants either way - and which only the water's fog
   *  ever asks for, since every above-water and interior start is a positive distance. */
  dofMode(){
    if(this.opts.dof) return 'full';
    const p=this.dofFogPair();
    return (p && p[0] < 0)? 'fog' : null;
  },
  /** The fog pair MGE hands its post shaders, in units: `fogExpStart`/`fogExpDivisor`
   *  under exponential fog, `fogStart`/`fogEnd` under linear. Under the water that is the
   *  below-water pair rescaled the same way (distantland.cpp `adjustFog`), which is the
   *  one case where the start is behind the eye. Null when there is no fog to speak of. */
  dofFogPair(){
    const u=this.underFogNow;
    if(u){
      const start=u.start, end=u.end;
      const SC=4.4;                       // MGE's expFogDistScale
      const es=start/SC, ed=(end-es)/SC;
      return [es, ed];
    }
    const fr=this.fogRange? this.fogRange() : null;
    if(!fr || !this.opts.fog) return null;
    const k=1/Math.max(0.05, this.opts.fogDensity==null? 1 : this.opts.fogDensity);
    return [fr.start*k, fr.divisor*k];
  },
  _dofTarget(which,W,H){
    const gl=this.gl, key='_dofT'+which, t=this[key];
    if(t && t.W===W && t.H===H) return t;
    if(t){ try{ gl.deleteFramebuffer(t.fb); }catch(_){ } try{ gl.deleteTexture(t.tex); }catch(_){ } }
    const n={W,H};
    n.tex=gl.createTexture(); gl.bindTexture(gl.TEXTURE_2D,n.tex);
    gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,W,H,0,gl.RGBA,gl.UNSIGNED_BYTE,null);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.MIRRORED_REPEAT);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.MIRRORED_REPEAT);
    n.fb=gl.createFramebuffer(); gl.bindFramebuffer(gl.FRAMEBUFFER,n.fb);
    gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,n.tex,0);
    n.ok=gl.checkFramebufferStatus(gl.FRAMEBUFFER)===gl.FRAMEBUFFER_COMPLETE;
    gl.bindFramebuffer(gl.FRAMEBUFFER,null); gl.bindTexture(gl.TEXTURE_2D,null);
    this[key]=n;
    return n;
  },
  /** The framebuffer the finished frame goes to when the pass will run, or null. */
  dofDest(W,H){
    if(!this.dofMode()) return null;
    const t=this._dofTarget('S',W,H);
    return t.ok? t.fb : null;
  },
  /** The two passes: the frame in the DoF source, through the circle of confusion and the
   *  disc blur, into `dst` (null: the canvas). `T` carries the depth the frame resolved. */
  drawDof(T,dst){
    const gl=this.gl, mode=this.dofMode(), src=this._dofTS;
    if(!mode || !src || !src.ok) return false;
    const mid=this._dofTarget('A',T.W,T.H);
    if(!mid.ok) return false;
    if(!this.progDofCoc){ this.progDofCoc=this._prog(VS_BLIT,FS_DOF_COC); this.progDofBlur=this._prog(VS_BLIT,FS_DOF_BLUR); }
    gl.disable(gl.DEPTH_TEST); gl.disable(gl.BLEND); gl.depthMask(false);
    gl.bindVertexArray(null);
    const pair=this.dofFogPair()||[0,1];
    let pr=this.progDofCoc; gl.useProgram(pr.p);
    gl.bindFramebuffer(gl.FRAMEBUFFER,mid.fb); gl.viewport(0,0,T.W,T.H);
    gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D,src.tex); gl.uniform1i(pr.u.uTex,0);
    gl.activeTexture(gl.TEXTURE1); gl.bindTexture(gl.TEXTURE_2D,T.depTex); gl.uniform1i(pr.u.uDepth,1);
    const [near,far]=this.nearFar();
    gl.uniform2f(pr.u.uNearFar,near,far);
    gl.uniform2f(pr.u.uRcpRes,1/T.W,1/T.H);
    gl.uniform1f(pr.u.uTanH,Math.tan(FOV/2)); gl.uniform1f(pr.u.uAspect,T.W/T.H);
    gl.uniform1f(pr.u.uFogStart,pair[0]); gl.uniform1f(pr.u.uFogRange,Math.max(1,pair[1]));
    gl.uniform1i(pr.u.uFull, mode==='full'?1:0);
    gl.drawArrays(gl.TRIANGLES,0,3);
    pr=this.progDofBlur; gl.useProgram(pr.p);
    gl.bindFramebuffer(gl.FRAMEBUFFER,dst||null); gl.viewport(0,0,T.W,T.H);
    gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D,mid.tex); gl.uniform1i(pr.u.uTex,0);
    gl.uniform2f(pr.u.uRcpRes,1/T.W,1/T.H);
    gl.drawArrays(gl.TRIANGLES,0,3);
    gl.activeTexture(gl.TEXTURE1); gl.bindTexture(gl.TEXTURE_2D,null);
    gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D,null);
    this._texU=null;
    /* For the tests: the blur radius in pixels this frame used, the same arithmetic the
       shader does. It grows with the width - 6 px at 1280, 9 at 1920 - which is the way
       round the .fx has it and the way round 18dy had wrong. */
    this.dofRadius=DOF_R/(1280/T.W);
    this.dofDrawn=true;       // for the tests: this frame went through the pass
    return true;
  },
});
