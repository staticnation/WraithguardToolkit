/* Gardenfell, 37_shadow.js. Copyright (c) 2026 Robin Hjelte. GNU General Public License, version 2
   (see page-source/LICENSE): this file carries code ported from MGE XE, which is GPL-2.0. */
/* =====================================================================================
   The sun's shadow - round 18ee.

   Robin: "Lets see if we can also get shadows into the renderer. Only shadows from the
   sun, and never cast by grass. Only from statics, the terrain, and moving activators",
   and then: "Grab inspiration from MGE XE G7's fork, and from OpenMW to make sure they
   look like in game."

   So this is MGE XE's own shadow system, at its numbers, read out of the G7 fork
   (`d3d8/cpp/mge/rendershadow.cpp`) and the shaders his install actually runs
   (`XE Shadowmap.fx`, `XE Mod Shadow.fx`, `XE Mod Shadow Data.fx`). The receiving half -
   the exponential lookup, the sun estimate, the blue-tinted shade, the fade with the
   fog - is in `SKY_GLSL` (06_gl.js) beside the rest of the atmosphere. This file draws
   the map.

   WHAT MGE DOES, AND WHAT WE DO

   * **Two cascades**, near and far, radius 1,000 and 4,000 units, side by side in one
     atlas. MGE: `shadowNearRadius`, `shadowFarRadius`, `shadowCascades = 2`.
   * **Centred a radius ahead of the camera**, along the view direction, and only half a
     radius of that in z - "the player is likely looking at the ground plane rather than
     below". The *view* is built on the eye quantized to 16 units and the remainder is
     added to the matrix's translation quantized to whole texels, which is what stops the
     shadows swimming as the camera moves and turns. Both, verbatim.
   * **An orthographic box** 2*radius wide, (1 + |lightVec.z|) * radius tall - narrower
     with the sun overhead, because a box tilted toward the horizon covers more ground
     for the same number of texels - and 16,384 units deep, the light camera pulled one
     cell-size back from the centre. MGE: `D3DXMatrixOrthoRH(proj, 2*radius,
     (1+fabs(lightVec.z))*radius, 0, 2*zrange)` with `zrange = kCellSize`.
   * **An exponential shadow map** (ESM): the stored value is linear depth over that
     16,384-unit range times 32,768 in a half-float, blurred while it is still depth by a
     separable five-tap, and read back as `1 - saturate(exp(60*dz + 0.12))`. The
     exponential is what softens the edge and what hides the bias problems a hard
     comparison has. OpenMW instead compares hard, with three cascades, a polygon offset
     and a normal offset of one unit; MGE's is what Robin sees, so MGE's is what this is.
   * **Casters**: the distant land (terrain) and the statics, and *not* the grass - MGE's
     `renderShadowLayerGeneric` draws exactly those two passes. That is Robin's rule
     already, and the reason for it is visible in his game: a shadow map at this scale
     cannot resolve a blade, so grass casting into it reads as noise on the ground.
     Alpha is respected, so a leaf cluster casts its leaves and not its quads.

   WHERE WE DIFFER, AND WHY

   * MGE's caster pass alpha-tests every static at a fixed 180/255, because its distant
     statics live in one texture atlas with no per-mesh alpha rule to hand. We have the
     mesh's own function and reference (`_meshTex` sets them) and use those, which is what
     OpenMW does too - so a mesh whose alpha rule is unusual casts the shadow its own
     material asks for rather than a guess.
   * MGE also draws **moving things** - an NPC, a swinging sign - through its recorded
     Morrowind draws, not through this pass. Robin asked for "moving activators" to cast,
     and in this tool an animated banner is one of the statics, so it casts with the rest:
     `_meshTex` carries its `uAnim` matrix and skins it if it is skinned, so what casts is
     the pose being drawn this frame.
   * MGE centres each cascade **one radius ahead of the player**, "as the player is likely
     looking at the ground plane rather than below", and guesses its height from the view.
     We take the same point along the view and ask the terrain how high the ground is
     there, because a camera looking steeply down from above leaves that guess in the air
     with the ground outside the box. Either way the centre is worked out from the eye and
     the two view angles alone - never from the orbit pivot, which is not where the camera
     is. See `shadowLookAt`.
   * MGE limits each cascade to the camera frustum's silhouette with a stencil hull, to
     spend no texels where nothing can be seen. That is a speed trick for a game drawing
     forty cells; here the cascade is culled to its own box and the hull is left out.
   * MGE renders the far layer to the atlas and blurs it in place with its own stencil
     still standing. We blur the whole atlas into a second texture and back, which is the
     same separable filter with one more clear.

   The whole pass is off unless `opts.shadows` is on, and the switch is off unless the
   install says the game has them on (`Sky.installShadows`) - on a MoMW setup, which is
   OpenMW, that is settings.cfg's [Shadows] enable shadows, and the map's size its shadow
   map resolution unless the Preview's Shadow detail names one (Wraithguard: an OpenMW
   setup has no MGE XE settings to take either from).

   Wraithguard: an additive draw - a flame, a glow - is never darkened, as MGE skips
   every additive batch when it lays its shadows over the frame (gdnShade, SKY_GLSL). Nothing here writes to any
   file and nothing reads the plugin state.

   (No backticks inside the shader strings: template literals.)
   ===================================================================================== */

/* MGE has two cascades reaching 1,000 and 4,000 units. Round 18eg item 3, Robin: "I
   think the shadows look good, but we don't need to have exact parity with MGE XE or
   similar for those, and with that in mind, can we make the shadow distances longer, and
   the shadow cascade transitions nicer? The only thing I want parity on is how dark the
   shadows look etc., which we currently have succeeded with."
   
   So three cascades reaching 20,000 units - five times MGE's, about two and a half cells,
   which is what an overview of a few cells actually shows - blended into one another
   across the outer sixth of each box (`gdnShadowLoss`, SKY_GLSL) instead of switching at
   a line. Every constant that decides how dark a shadow is stayed where MGE set it: the
   exponent, the bias, the shade, the colouration, the sun estimate, the fog fade. The
   atlas is one cascade per column, so it is `uShadowN` times as wide as it is tall.
   
   The steps are roughly four-fold, which is what keeps the texel size from jumping at a
   boundary: at a 2,048 map that is 1.5, 5.9 and 19.5 units a texel.

   Round 18fa, Robin: "If the shadows are cheap, maybe we can have shadows even further
   away? Would this be possible at the same time as we solve better quality shadows close
   up?" The outermost box reaches exactly as far as a shadow can show through the frame's
   fog (`shadowReach`): `gdnShade` scales the shade by the fog's clearness squared, so past
   the distance where that falls under half a level of 255 the box is drawing into nothing
   - and short of it, it was leaving shadows out. MGE's 2-5 cell fog puts that at 30,720
   units; an OpenMW install with an eight-cell view distance at 45,400; no fog at all is
   capped at twelve cells. Four-fold steps in to 1,500, five boxes at most, and a box that
   would only add a half-step is folded into the last one. What makes the far boxes
   affordable is the cache below: a box that did not move is not redrawn, and the big ones
   move least.

   **The boxes are fixed for a given fog (18fb).** 18fa's first cut also sized the nearest
   box to the zoom - six tenths of the distance to the ground the camera looked at, in
   steps of root two - and Robin saw what that does in motion: "the shadows flickers
   between different cascades when I move the camera [...] maybe it comes from the dynamic
   size of the cascades. We shouldn't keep it. Shadows when the camera is in motion
   matters a lot, the transitions needs to be smooth and never flicker." Every step of
   that rule re-partitioned the whole scene into boxes of different texel sizes, so every
   shadow in view changed its edge at once. The near box is 1,500 again, always; sharper
   close up is a question of texels, not of boxes, and stays open. */const SHADOW_R0=1500.0;        // the nearest box's radius
const SHADOW_STEP=4.0;         // from one cascade to the next
const SHADOW_R_MAX=98304.0;    // twelve cells: further than any fog lets a shadow show
/* MGE: zrange = kCellSize. The box's depth is twice this, the light camera one of them
   back from the centre, so a caster within a cell-size of the ground either way is in.

   **The depth window grows with the box (18fc).** MGE's boxes reach 4,000 units, so a
   16,384-unit window holds all of one whatever the sun's height. Ours reach 33,000 and
   more, and the window did not grow with them: a point on the ground `d` units from the
   centre along the sun's azimuth sits `d * cos(elevation)` deep in the light's frame, so
   the outermost box only ever answered for the ground within 8,192 / cos(elevation) of
   its centre in that direction - 9,500 units with the sun at 30 degrees - and nothing
   past that line, in the direction of the sun or away from it, had a shadow at all,
   while across the sun the same box reached 33,000. The centre walks with the camera, so
   the line walked with it: Robin, "shadows are still popping in at the furthest distance
   from an object, instead of smoothly fading in over a distance". Each box's window is
   now at least its own radius deep either side of the centre (`shadowZRange`), and the
   receiver rescales the exponential by the same factor so a shadow's edge is the same
   273 units soft in every box (SKY_GLSL, `uShadowZ`). The two boxes MGE's window already
   held - 1,500 and 6,000 - keep it to the bit. */
const SHADOW_ZRANGE=8192.0;
function shadowZRange(radius){ return Math.max(SHADOW_ZRANGE, radius); }
/* **And the window reaches back up the light (18fd).** Robin: "For long shadows, if I have
   the shadow caster behind me, and move towards the end of the shadow (as in away from
   the caster), at some distance the shadow dissolves and disappears. [...] Tel Vos tower,
   or Vivec cantons." A caster shadows a point `h / sin(elevation)` units up the light's
   ray from it - a 3,000-unit tower at a 20-degree sun is 8,800 units up the ray from the
   end of its shadow - and the light camera sat one `shadowZRange` back from the centre,
   so a caster further up the ray than that was clipped at the near plane, part by part,
   top first: the far end of a long shadow dissolved exactly as the camera walked out to
   it. MGE has the same limit (16,384 for a 4,000 box) and the same dissolve. The light
   camera now sits this much further back for every box, so the window is
   `[-(zRange + SHADOW_ZBACK), +zRange]` about the centre along the light. That is four
   cells up the ray - a tower of 3,000 casts to a 5-degree sun - and it costs nothing in
   picture because the atlas is R32F where the context allows (`_shadowTargets`):
   a half-float atlas quantizes depth at 1/2048 of the window, which is 8 units in MGE's
   and would be 24 in this one, so on a context without float rendering the window stays
   MGE's depth and long shadows keep MGE's limit. The receiver's exponent scales with the
   whole window (`uShadowZ`), as 18fc arranged. */
const SHADOW_ZBACK=32768.0;
/* MGE: ESM_scale, the factor that puts linear depth in [0,1] into (most of) the
   half-float range. The receiving side divides by the same number. */
const SHADOW_ESM_SCALE=32768.0;
/* What the map is drawn at when the install has nothing to say. MGE's own default, and
   the range its runtime clamps the setting to. */
const SHADOW_RES_DEFAULT=1024, SHADOW_RES_MIN=256, SHADOW_RES_MAX=2048;
/* Round 18gi: what a see-through shape casts. Robin: `f\active_blight_large.NIF` "interact
   weirdly with sun shadows. I am pretty sure this file doesn't cast shadows in morrowind
   the game. We should have a way to understand what should cast shadows." The file is ten
   cloud cylinders, alpha-blended with a material alpha animated between 0 and 0.4, and
   this pass drew every static through its own alpha rule, which for a blended shape with
   no test of its own is no rule at all: the cylinders cast as if they were solid. MGE's
   caster pass (`renderShadowLayerGeneric`) alpha-tests every static at 180/255, which is
   what hides them in his game, and so does this now: a shape that blends casts only where
   its texture's alpha times its material's passes MGE's threshold. An opaque shape and a
   cutout keep their own rule, as before. */
const SHADOW_CAST_ALPHA=180/255;

/* The caster: nothing but depth, and the mesh's own alpha rule. Driven by whichever
   vertex shader the geometry already has (VS_STATIC, VS_STATIC_M, VS_GROUND), all of
   which output vUV; a vertex output with no fragment input is fine, and the depth needs
   no varying of its own because an orthographic gl_FragCoord.z *is* the linear depth.
   The alpha block is the object pass's, so a cut-out casts its cut-out. */
const FS_SHADOW_CAST=`#version 300 es
precision highp float;
in vec2 vUV;
uniform sampler2D uTex; uniform int uHasTex; uniform int uAlphaFn; uniform float uAlphaRef; uniform float uMatAlpha; uniform vec4 uUVXform;
out vec4 o;
void main(){
  vec2 auv=vUV*uUVXform.xy+uUVXform.zw;
  vec4 t = uHasTex==1 ? texture(uTex,auv) : vec4(1.0);
  if(uAlphaFn>=0){
    float ta=t.a*uMatAlpha;
    bool pass = uAlphaFn==0 ? true
              : uAlphaFn==1 ? ta <  uAlphaRef
              : uAlphaFn==2 ? abs(ta-uAlphaRef) <  0.002
              : uAlphaFn==3 ? ta <= uAlphaRef
              : uAlphaFn==4 ? ta >  uAlphaRef
              : uAlphaFn==5 ? abs(ta-uAlphaRef) >= 0.002
              : uAlphaFn==6 ? ta >= uAlphaRef
              : uAlphaFn==8 ? t.a > uAlphaRef
              : false;
    if(!pass) discard;
  }
  o=vec4(${SHADOW_ESM_SCALE.toFixed(1)} * gl_FragCoord.z);
}`;

/* MGE's shadowSoften, one axis at a time: weights 0.2, 0.8, 1, 0.8, 0.2 at 0, +/-0.71
   and +/-1.42 texels, summing to 3, "looks better without exp-space filtering, with a
   side effect of expanding silhouettes by about 1 pixel". The offset is in texels of a
   *cascade*, so the horizontal pass steps by one atlas texel and stays inside its own
   half as long as the receiver keeps its four-texel margin (SKY_GLSL). */
const FS_SHADOW_SOFTEN=`#version 300 es
precision highp float;
in vec2 vUV;
uniform sampler2D uTex; uniform vec2 uStep;
out vec4 o;
void main(){
  float d = texture(uTex, vUV).r;
  d += 0.2 * texture(uTex, vUV - 1.42*uStep).r;
  d += 0.8 * texture(uTex, vUV - 0.71*uStep).r;
  d += 0.8 * texture(uTex, vUV + 0.71*uStep).r;
  d += 0.2 * texture(uTex, vUV + 1.42*uStep).r;
  o = vec4(d / 3.0);
}`;

Object.assign(Renderer.prototype,{
  /** The shadow programs, built once; `warmShaders` (06_gl.js, round 18gt) builds them at
   *  start-up so the first lit frame does not pay for them. */
  _ensureShadowCasters(){
    if(!this.progShadowGround) this.progShadowGround=this._prog(VS_GROUND,FS_SHADOW_CAST);
    if(!this.progShadowStatic) this.progShadowStatic=this._prog(VS_STATIC,FS_SHADOW_CAST);
    return this.progShadowStatic;
  },
  /** Wraithguard: how far the shadows reach - MGE's fogMWScalar, Morrowind's own linear
   *  near fog (nearFogStart .. nearFogRange), which is the pair its SSAO fades by too
   *  (`ssaoFade`: `adjustFog`'s fit, the Fog density slider applied). `uShadowFog`. */
  _shadowNearFog(now,scat){
    return this.ssaoFade? this.ssaoFade(now,scat) : [1280, 7168];
  },
  _ensureShadowSoften(){
    if(!this.progShadowSoften) this.progShadowSoften=this._prog(VS_BLIT,FS_SHADOW_SOFTEN);
    return this.progShadowSoften;
  },
  /** The map's size in texels, one cascade square. The Preview's Shadow detail when it
   *  names a size; otherwise the install's (`renderer.shadows.resolution` - on a MoMW
   *  setup, OpenMW's own [Shadows] shadow map resolution), clamped to what a texture can
   *  be here. */
  shadowRes(){
    const s=(typeof Sky==='object' && Sky.installShadows)? Sky.installShadows() : null;
    const want=+this.opts.shadowRes||0;
    let r=want>0? want : (s && isFinite(s.resolution) && s.resolution>0)? +s.resolution : SHADOW_RES_DEFAULT;
    if(this._shadowResTest) r=this._shadowResTest;   // the tests shrink it to see the artefacts
    r=Math.max(SHADOW_RES_MIN, Math.min(SHADOW_RES_MAX, Math.round(r)));
    const max=this.gl.getParameter(this.gl.MAX_TEXTURE_SIZE)||2048;
    return Math.min(r, Math.floor(max/SHADOW_MAX_CASCADES));
  },

  /** 18fd: how far the window reaches back up the light beyond `shadowZRange` - see
   *  SHADOW_ZBACK. Nought on a context whose atlas is half-float, where the reach would
   *  cost the near shadows their depth precision. The targets decide the format, so they
   *  are asked first; the answer is per context and does not change. */
  shadowBack(){
    if(this._shR32===undefined) this._shadowTargets(this.shadowRes(), 1);
    return (this._shR32 && !this._shForceHalf)? SHADOW_ZBACK : 0;
  },

  /** Round 18fa: how far a shadow can show through this frame's fog, in units from the
   *  eye. `gdnShade` multiplies the shade by the fog's clearness squared (MGE's own
   *  fade), and the shade takes at most one unit of a pixel's light, so once clearness²
   *  is under half a level of 255 nothing a cascade holds can change a pixel. The fog
   *  is the shader's own curve solved for that: `exp(-(d - start) / range)` under the
   *  atmosphere, `exp(-k d)` without it, taken from the scene's centre there and so
   *  pushed out by the eye's own distance from it. No fog at all is the cap. */
  shadowReach(fr, fogScale, fogK, fogOn){
    const LN=-Math.log(Math.sqrt(0.5/255));   // clearness at half a level: 3.117 fall-offs
    if(!fogOn) return SHADOW_R_MAX;
    if(fr) return Math.max(SHADOW_R0, Math.min(SHADOW_R_MAX, fr.start*fogScale + LN*fr.divisor*fogScale));
    /* The flat fog is measured from the scene's centre, not the eye; taken as a radius
       from the eye it is a little short on the far side of the scene and long on the near
       one, and (18fb) it must not carry the eye's position, or the outermost box would
       resize as the camera moves. */
    if(fogK>0) return Math.max(SHADOW_R0, Math.min(SHADOW_R_MAX, LN/fogK));
    return SHADOW_R_MAX;
  },

  /** Round 18fa: the distance from the eye to the ground along the middle of the view,
   *  or Infinity when the view never meets it - the number the nearest cascade is sized
   *  by. `groundAt` walks the terrain the page supplied; a camera with no terrain under
   *  it (the mesh editor) looks far by definition. */
  shadowHit(eye){
    if(!this.groundZ || !this.groundAt || !this.cv) return {hit:null, d:Infinity};
    let hit=null;
    try{
      const r=this.cv.getBoundingClientRect();
      hit=this.groundAt(r.left+r.width/2, r.top+r.height/2);
    }catch(_){ hit=null; }
    if(!hit) return {hit:null, d:Infinity};
    return {hit, d:Math.hypot(hit[0]-eye[0], hit[1]-eye[1], hit[2]-eye[2])};
  },
  shadowHitDist(eye){ return this.shadowHit(eye).d; },

  /** Round 18fa: the cascade radii for this fog, nearest first. See the note above
   *  `SHADOW_R0` for the rule; this is the rule. Nothing about the camera is in it (18fb). */
  shadowRadii(reach){
    const out=[SHADOW_R0];
    let r=SHADOW_R0;
    while(out.length<SHADOW_MAX_CASCADES-1 && r*SHADOW_STEP*1.5<reach){ r*=SHADOW_STEP; out.push(r); }
    // The fog's reach plus the snap margin its own centre may sit off by (see the pass).
    const last=Math.round(Math.min(reach*1.1, SHADOW_R_MAX));
    if(last>r) out.push(last);
    return out;
  },

  /** The atlas and the scratch it is blurred through: two cascades wide, one tall, a
   *  single red half-float channel (MGE stores its ESM in one channel of an FP16 too).
   *  Linear filtering, because the lookup reads the blurred depth between texels.
   *  Null when this context cannot render half-float colour, which is what the switch
   *  stands down on. */
  _shadowTargets(res, n){
    const gl=this.gl, W=n*res, H=res, t=this._shT;
    if(this._shR32===undefined){   // 18fd: decided once per context, before the reuse test reads it
      try{ this._shR32=!!(gl.getExtension('EXT_color_buffer_float') && gl.getExtension('OES_texture_float_linear')); }
      catch(_){ this._shR32=false; }
    }
    if(t && t.res===res && t.n===n && t.r32===(this._shR32 && !this._shForceHalf)) return t.ok? t : null;
    if(t){
      for(const f of ['fbA','fbB','fbC']) try{ gl.deleteFramebuffer(t[f]); }catch(_){ }
      for(const x of ['texA','texB','texC']) try{ gl.deleteTexture(t[x]); }catch(_){ }
      for(const r of ['dep','depC']) try{ gl.deleteRenderbuffer(t[r]); }catch(_){ }
    }
    if(this._shHalf===undefined){
      try{ this._shHalf=!!(gl.getExtension('EXT_color_buffer_half_float')||gl.getExtension('EXT_color_buffer_float')); }
      catch(_){ this._shHalf=false; }
    }
    /* 18fd: a single-float atlas where the context can both render to one and filter one
       (the blur and the lookup read between texels). What it buys is the window reaching
       four cells back up the light at no cost in depth precision - see SHADOW_ZBACK. A
       test may plant `_shForceHalf` to see the half-float path. */
    const r32=!!(this._shR32 && !this._shForceHalf);
    const n_={res,n,W,H,ok:false,r32};
    this._shT=n_;
    this._shCache=null;            // the cache belongs to the targets; new targets, new cache
    if(!this._shHalf && !r32) return null;
    const mk=()=>{
      const x=gl.createTexture(); gl.bindTexture(gl.TEXTURE_2D,x);
      if(r32) gl.texImage2D(gl.TEXTURE_2D,0,gl.R32F,W,H,0,gl.RED,gl.FLOAT,null);
      else    gl.texImage2D(gl.TEXTURE_2D,0,gl.R16F,W,H,0,gl.RED,gl.HALF_FLOAT,null);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.CLAMP_TO_EDGE);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.CLAMP_TO_EDGE);
      return x;
    };
    const rb=()=>{ const r=gl.createRenderbuffer(); gl.bindRenderbuffer(gl.RENDERBUFFER,r);
                   gl.renderbufferStorage(gl.RENDERBUFFER,gl.DEPTH_COMPONENT16,W,H); return r; };
    n_.texA=mk(); n_.texB=mk(); n_.texC=mk();
    n_.dep=rb(); n_.depC=rb();
    n_.fbA=gl.createFramebuffer(); gl.bindFramebuffer(gl.FRAMEBUFFER,n_.fbA);
    gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,n_.texA,0);
    gl.framebufferRenderbuffer(gl.FRAMEBUFFER,gl.DEPTH_ATTACHMENT,gl.RENDERBUFFER,n_.dep);
    const okA=gl.checkFramebufferStatus(gl.FRAMEBUFFER)===gl.FRAMEBUFFER_COMPLETE;
    n_.fbB=gl.createFramebuffer(); gl.bindFramebuffer(gl.FRAMEBUFFER,n_.fbB);
    gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,n_.texB,0);
    const okB=gl.checkFramebufferStatus(gl.FRAMEBUFFER)===gl.FRAMEBUFFER_COMPLETE;
    /* Round 18ej: the still half of the scene, kept unblurred with its own depth, so a
       frame that only has to redraw the banners can start from it instead of from the
       terrain and every static in three boxes. */
    n_.fbC=gl.createFramebuffer(); gl.bindFramebuffer(gl.FRAMEBUFFER,n_.fbC);
    gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,n_.texC,0);
    gl.framebufferRenderbuffer(gl.FRAMEBUFFER,gl.DEPTH_ATTACHMENT,gl.RENDERBUFFER,n_.depC);
    const okC=gl.checkFramebufferStatus(gl.FRAMEBUFFER)===gl.FRAMEBUFFER_COMPLETE;
    gl.bindFramebuffer(gl.FRAMEBUFFER,null); gl.bindTexture(gl.TEXTURE_2D,null);
    n_.ok=okA&&okB&&okC;
    return n_.ok? n_ : null;
  },

  /** Where a cascade of this radius is centred: a radius ahead of the camera along the
   *  view, at the height of the ground under that point.
   *
   *  MGE's own rule is the first half of that - `lookAt = eyePos + radius * eyeVec`, and
   *  only half a radius of it in z, "as the player is likely looking at the ground plane
   *  rather than below". It is written for a camera at eye height; from a camera looking
   *  steeply down from above, a centre a radius along the view is still high in the air,
   *  the ground below falls outside the box, and nothing is shaded at all. So the height
   *  is asked of the terrain instead of guessed, which is something MGE could not do
   *  cheaply and this renderer can: shadows live on the ground, so the box is put there.
   *  Off the loaded cells there is nothing to ask, and MGE's guess stands.
   *
   *  **It reads the camera and nothing else** (18ef). Robin: "I don't have any shadows in
   *  my build [...] maybe at one time I saw something in a far difference shift when I
   *  toggled", and then "The camera position should always and only be the exact position
   *  of the actual camera that is capturing the scene [...] The WASD and Orbit modes
   *  should only move the camera, never affect any other attributes." 18ee centred the
   *  cascades on `cam.tx/ty/tz`, the orbit pivot - true enough while orbiting, and wrong
   *  the moment he flew: flying moves the pivot and keeps `cam.dist` (`_flyStep`), so the
   *  pivot sits `dist` ahead of the eye, and `dist` is whatever the orbit was before he
   *  took off - thousands of units. Both cascades went out into the distance with it:
   *  the shift he saw far off, and no shadow anywhere near him. Measured on his own cell,
   *  flying with the pivot 9,000 units ahead: 28,825 pixels changed by the pass before,
   *  343,533 after. The eye and the two angles are the camera; the pivot is the orbit's
   *  bookkeeping, and nothing that draws should read it. */
  shadowLookAt(eye, radius){
    const c=this.cam, ce=Math.cos(c.el), se=Math.sin(c.el);
    const dir=[-ce*Math.cos(c.az), -ce*Math.sin(c.az), -se];
    const x=eye[0] + radius*dir[0], y=eye[1] + radius*dir[1];
    let z=null;
    try{ z=this.groundZ(x,y); }catch(_){ z=null; }
    if(z==null || !isFinite(z)) z=eye[2] + 0.5*radius*dir[2];
    return [x,y,z];
  },

  /** One cascade's world-to-clip matrix, MGE's `renderShadowLayer`. `lightVec` is the
   *  direction the light travels; `look` is the point the cascade is centred on, which
   *  `shadowLookAt` works out per layer. */
  _shadowLayerVP(radius, look, lightVec, res, snap){
    const zr=shadowZRange(radius);   // 18fc: the window holds the box, whatever the sun's height
    const zb=this.shadowBack();      // 18fd: and reaches back up the light for the tall casters
    const cam=[look[0]-(zr+zb)*lightVec[0], look[1]-(zr+zb)*lightVec[1], look[2]-(zr+zb)*lightVec[2]];
    /* Straight down the light, the up vector has to be off the light's own axis. The
       sun's path in this tool is tilted (25_sky.js `sunDir`) and never exactly vertical,
       but a planted sun in a test can be, and a degenerate lookAt draws nothing at all. */
    const up=Math.abs(lightVec[2])>0.999? [0,1,0] : [0,0,1];
    const V=M4.lookAt(cam, look, up);
    const P=M4.ortho(2*radius, (1+Math.abs(lightVec[2]))*radius, 0, 2*zr+zb);
    const VP=M4.mul(P,V);
    /* The translation quantized to whole texels of this cascade, all three axes. This
       is what stops the shimmer when the camera turns: the box slides by whole texels,
       so a texel keeps covering the same ground - and (18fa) it is what lets the cache
       keep a column at all, because a matrix that changed by a hair is a column redrawn.
       Until 18fa the centre was snapped to 16 units first and the *remainder* quantized,
       which left the sum stepping with every 16-unit snap, and the depth was not
       quantized at all; so every camera motion redrew every cascade. The rotation is the
       light's and does not change with the camera, so the translation is the whole of
       what moves, and quantizing it here is exact. The depth cannot show: the stored
       depth and the receiver's are both taken through this same matrix. */
    const qz=2.0/res;
    /* 18fb: the coarse boxes step by `snap` whole texels rather than one, so a walk of a
       few hundred units leaves the 24,000 box where it was and the cache keeps that
       column - it is the column that costs. Done here, in the light's own texels, and
       not by snapping the centre in world units first: a world-space grid does not land
       on the texel grid once the light's rotation is applied, so a jump of it shifted
       the map by a fraction of a texel and every far shadow with it. In whole texels the
       map's texels are the same texels before and after a jump, and nothing on screen
       moves when the box does. */
    const q=qz*Math.max(1, snap|0);
    VP[12]=q*Math.floor(VP[12]/q);
    VP[13]=q*Math.floor(VP[13]/q);
    /* The depth by the same step: the centre's height is the ground's under it, and a
       walk over uneven ground would otherwise move every box's depth window by a texel
       and redraw every column. A coarse box's step is a thirty-second of its window,
       and the window is at least the box's own radius either side of the centre (18fc),
       so the step never puts the box's own ground outside it. */
    VP[14]=q*Math.floor(VP[14]/q);
    return VP;
  },

  /** The groups' runs and bakes that this cascade can see, into arrays of the pass's
   *  own. A copy of `_cullGroups`'s merge rather than a call to it, deliberately: that
   *  one caches on the matrix it last culled for (`G.cullVP`) and is called again by the
   *  camera pass a moment later, so borrowing it would make every frame cull three times
   *  and keep no answer. */
  _shadowCullGroups(G,planes,tiny,pick){
    const rb=G.runBox, rbase=G.runBase, rn=G.runN, rp=G.runPart, pc=G.partCount, po=G.partOff;
    const kb=G.bkBox, ko=G.bkOff, kc=G.bkCount;
    const pl=planes;
    /* Round 18eg item 3: and, in the coarse cascades, a mesh whose whole box is smaller
       than a texel or two of that cascade is skipped. It cannot leave a mark there - the
       blur alone is three texels wide - and the outermost box holds a hundred times the
       ground of the near one, so this is what keeps the cost of reaching five times
       further from being five times the geometry. The same reasoning as the sub-pixel
       skip in the grass shader, and at the same kind of threshold. */
    const cut=tiny>0? tiny : 0;
    const seen=(box,o)=>{
      if(cut>0 && (box[o+3]-box[o])<cut && (box[o+4]-box[o+1])<cut && (box[o+5]-box[o+2])<cut) return false;
      for(let i=0;i<6;i++){
        const p=pl[i];
        const x=p[0]>=0? box[o+3] : box[o], y=p[1]>=0? box[o+4] : box[o+1], z=p[2]>=0? box[o+5] : box[o+2];
        if(p[0]*x+p[1]*y+p[2]*z+p[3]<0) return false;
      }
      return true;
    };
    for(const g of G.all){
      // Round 18fa: a group the pass is not drawing is not culled either - its counts
      // are left at nought so the draw below passes it by.
      if(pick && !pick(g)){ g.svis=0; g.svisInst=0; g.sbvis=0; continue; }
      if(!g.scount){
        const cap=g.r1-g.r0, bcap=g.b1-g.b0;
        g.scount=new Int32Array(cap); g.soff=new Int32Array(cap); g.sinst=new Int32Array(cap);
        g.sbase=new Int32Array(cap+4);   // padded to a whole ivec4, as the camera cull's is
        g.sbcount=new Int32Array(bcap); g.sboff=new Int32Array(bcap);
      }
      let vis=0, inst=0, lastPart=-1, lastEnd=-1;
      for(let r=g.r0;r<g.r1;r++){
        if(!seen(rb,r*6)) continue;
        const part=rp[r], base=rbase[r], n=rn[r];
        if(vis && lastPart===part && lastEnd===base){ g.sinst[vis-1]+=n; lastEnd+=n; }
        else{ g.scount[vis]=pc[part]; g.soff[vis]=po[part]; g.sinst[vis]=n; g.sbase[vis]=base; vis++; lastPart=part; lastEnd=base+n; }
        inst+=n;
      }
      g.svis=vis; g.svisInst=inst;
      let bvis=0, lastR=-2;
      for(let r=g.b0;r<g.b1;r++){
        if(!seen(kb,r*6)) continue;
        if(bvis && lastR===r-1) g.sbcount[bvis-1]+=kc[r];
        else{ g.sbcount[bvis]=kc[r]; g.sboff[bvis]=ko[r]; bvis++; }
        lastR=r;
      }
      g.sbvis=bvis;
    }
  },

  /** Which casters move, and therefore cannot be kept in the still cache.
   *
   *  Round 18ej. `_meshTex` poses a skinned shape and hands a node's animation matrix to
   *  whichever program is drawing, so an animated banner casts the pose it is drawn in -
   *  and a scrolling texture changes what an alpha-tested caster cuts out. All of those
   *  are redrawn every frame; everything else is drawn once and kept.
   *
   *  A *group* that moves turns the cache off altogether rather than joining the moving
   *  pass: a group is drawn through the per-cascade cull, and culling three boxes to draw
   *  one banner would cost more than it saves. Animated batches key on themselves
   *  (`_groupKey`, 18cs) so a moving group is a group of its own; in Robin's cells there
   *  are none - the eleven movers in Tel Vos are all skinned, and skinned shapes are not
   *  groupable at all. Recomputed when the caster set changes, not per frame. */
  _shadowMovers(){
    const rev=this._casterRev|0, G=(this._groups && this._groups.src===this.statics)? this._groups : null;
    const c=this._shMov;
    if(c && c.rev===rev && c.G===G && c.statics===this.statics) return c;
    /* Round 18fa: and a flipbook (18ey) - its cut-out is a different frame's - and a
       billboard (18ez), which turns with the camera. */
    const moves=b=>!!(b && (b.skin || b.anim || b.uvAnim || b.uvAnim2 || b.flip || b.billboard));
    const own=G? (this._legacyStatics||[]) : (this.statics||[]);
    const list=[];
    for(const b of own) if(moves(b) && b.n && b.part) list.push(b);
    /* Round 18fa: a moving *group* joins the moving pass instead of switching the cache
       off. 18ej had it veto the cache on the grounds that culling three boxes to draw
       one banner costs more than it saves - and then 18eq measured that at Balmora 85
       groups holding 5,000 triangles of banner and sign were vetoing the cache for the
       287,000 triangles of rock and wall around them, in every real scene, every frame.
       The cull is per group over its own runs, so culling only the movers is the cheap
       half of that arithmetic. `groupMoves` stays as a count for the report. */
    const groups=new Set();
    if(G) for(const g of G.all) if(moves(g.rep)) groups.add(g);
    const n={rev, G, statics:this.statics, list, groups, groupMoves:groups.size>0};
    this._shMov=n;
    return n;
  },

  /** Round 18gi: MGE's alpha test for a see-through caster, after `_meshTex` has set the
   *  shape's own rule and its material alpha for this instant. Function 4 is "greater
   *  than", on the texture's alpha times the material's (`FS_SHADOW_CAST`), so a cloud at
   *  0.4 casts nothing and a sign's gradient casts its solid middle. */
  _shadowAlpha(pr,b){
    if(!b || !b.blend) return;
    this._u1i(pr,'uAlphaFn',4);
    this._u1f(pr,'uAlphaRef',SHADOW_CAST_ALPHA);
  },

  /** The casters of one cascade, into whatever framebuffer and viewport is bound.
   *  `want` is 'all' (no cache), 'still' (everything that does not move) or 'moving'
   *  (only what does). `stat` collects what was submitted, for the report. */
  _shadowCast(VP, planes, tiny, want, stat){
    const gl=this.gl;
    const moving=want==='moving';
    const G=(this._groups && this._groups.src===this.statics)? this._groups : null;
    const mov=this._shadowMovers();
    if(!mov.groups) mov.groups=new Set();
    const skip=(want==='still' && mov.list.length)? new Set(mov.list) : null;

    /* The terrain, in world space already, so the matrix is all it needs. Layer 0's
       indices only: the later layers are the paint and the blends over the same
       quads and would cast the same shadow twice. Ground never moves. */
    if(!moving){
      const anyGround=!!((this.cellChunks && this.cellChunks.length) || this.ground);
      if(anyGround){
        const pg=this.progShadowGround;
        gl.useProgram(pg.p); this._shReset(pg); this._tex0=undefined;
        gl.uniformMatrix4fv(pg.u.uVP,false,VP);
        gl.uniform1i(pg.u.uHasTex,0);
        this._u1i(pg,'uAlphaFn',-1);
        gl.uniform4f(pg.u.uUVXform,1,1,0,0);
        if(this.cellChunks && this.cellChunks.length){
          for(const ch of this.cellChunks){
            if(ch.box3 && !this._boxInView(planes,ch.box3)) continue;
            gl.bindVertexArray(ch.vao);
            gl.drawElements(gl.TRIANGLES,ch.count,gl.UNSIGNED_INT,0);
            stat.drawn++; stat.tris+=ch.count/3;
          }
        }else if(this.ground){
          gl.bindVertexArray(this.ground.vao);
          gl.drawElements(gl.TRIANGLES,this.ground.count,gl.UNSIGNED_SHORT,0);
          stat.drawn++; stat.tris+=this.ground.count/3;
        }
      }
    }

    /* The statics - the groups through their own program, then whatever was left out of
       them (skinned meshes, the moths, a hovered texture's batch) through the batch
       program. Culling is the mesh's own, so a two-sided leaf quad casts from both
       sides, and MGE's own note applies: casters drawn with one winding "causes false
       shadows when cast on the reverse side of a two-sided poly". */
    /* Round 18fa: the still pass leaves the moving groups out, the moving pass draws
       them alone, 'all' takes the lot - the same three answers the batches give. */
    const pickG = want==='all'? null
                : moving? (g=>mov.groups.has(g))
                : (mov.groups.size? (g=>!mov.groups.has(g)) : null);
    if(G && G.all.length && !(moving && !mov.groups.size)){
      gl.activeTexture(gl.TEXTURE0+INST_UNIT); gl.bindTexture(gl.TEXTURE_2D,G.instTex); gl.activeTexture(gl.TEXTURE0);
      this._shadowCullGroups(G,planes,tiny,pickG);
      gl.bindVertexArray(G.vao);
      const pm=this._progShadowGroup(G.multi? 'multi' : 'base');
      gl.useProgram(pm.p); this._shReset(pm); this._tex0=undefined;
      gl.uniformMatrix4fv(pm.u.uVP,false,VP);
      gl.uniform1i(pm.u.uTex,0);
      gl.uniform1i(pm.u.uInst,INST_UNIT);
      for(const g of G.all){
        if(!g.svis) continue;
        this._meshTex(pm,g.rep);
        this._shadowAlpha(pm,g.rep);
        this._meshMode(pm,g.rep,true);
        if(G.multi){
          for(let at=0;at<g.svis;at+=MAXRUN){
            const n=Math.min(MAXRUN,g.svis-at);
            gl.uniform4iv(pm.u.uRunBase, g.sbase, at, ((n+3)>>2)<<2);
            G.multi.multiDrawElementsInstancedWEBGL(gl.TRIANGLES, g.scount, at, gl.UNSIGNED_INT, g.soff, at, g.sinst, at, n);
            stat.calls++;
          }
        }else{
          for(let i=0;i<g.svis;i++){
            this._u1i(pm,'uBase',g.sbase[i]);
            gl.drawElementsInstanced(gl.TRIANGLES,g.scount[i],gl.UNSIGNED_INT,g.soff[i],g.sinst[i]);
            stat.calls++;
          }
        }
        for(let i=0;i<g.svis;i++) stat.tris+=(g.scount[i]/3)*(g.sinst[i]||1);
        stat.drawn+=g.svis;
      }
      /* The bakes - the small meshes copied into the group's own vertices - share
         that VAO and read their instance from a vertex attribute, so they need the
         third fetch mode and a program of their own. */
      const pb=this._progShadowGroup('baked');
      let anyBake=false;
      for(const g of G.all) if(g.sbvis){ anyBake=true; break; }
      if(anyBake){
        gl.useProgram(pb.p); this._shReset(pb); this._tex0=undefined;
        gl.uniformMatrix4fv(pb.u.uVP,false,VP);
        gl.uniform1i(pb.u.uTex,0);
        gl.uniform1i(pb.u.uInst,INST_UNIT);
        for(const g of G.all){
          if(!g.sbvis) continue;
          this._meshTex(pb,g.rep);
          this._shadowAlpha(pb,g.rep);
          this._meshMode(pb,g.rep,true);
          if(G.multi){ G.multi.multiDrawElementsWEBGL(gl.TRIANGLES, g.sbcount, 0, gl.UNSIGNED_INT, g.sboff, 0, g.sbvis); stat.calls++; }
          else for(let i=0;i<g.sbvis;i++){ gl.drawElements(gl.TRIANGLES,g.sbcount[i],gl.UNSIGNED_INT,g.sboff[i]); stat.calls++; }
          for(let i=0;i<g.sbvis;i++) stat.tris+=g.sbcount[i]/3;
          stat.drawn+=g.sbvis;
        }
      }
    }

    const own=moving? mov.list : (G? this._legacyStatics : this.statics);
    if(own && own.length){
      const ps=this.progShadowStatic;
      let used=false;
      for(const b of own){
        if(!b.n || !b.part) continue;
        if(skip && skip.has(b)) continue;
        if(tiny>0 && b.box && (b.box[3]-b.box[0])<tiny && (b.box[4]-b.box[1])<tiny
           && (b.box[5]-b.box[2])<tiny) continue;
        /* Wraithguard: only what is in this cascade's box. These were drawn whole into
           every cascade - a town's NPCs are a batch each, over a thousand draws a frame for
           shadows the cascades mostly could not hold. */
        if(b.groups && b.groups.length && !b.groups.some(g=>this._boxInView(planes,g.b))) continue;
        this._ensureVao(b);
        if(!b.vao) continue;
        if(!used){
          gl.useProgram(ps.p); this._shReset(ps); this._tex0=undefined;
          gl.uniformMatrix4fv(ps.u.uVP,false,VP);
          gl.uniform1i(ps.u.uTex,0);
          used=true;
        }
        this._meshTex(ps,b);
        this._shadowAlpha(ps,b);
        this._meshMode(ps,b,true);
        gl.bindVertexArray(b.vao);
        /* The whole batch, from its first instance - and the instance attributes
           pointed back at it, because the camera pass moves them along the stream run
           by run (`drawStatic`) and a VAO remembers where they were left. */
        if(b.mb){
          gl.bindBuffer(gl.ARRAY_BUFFER,b.mb);
          for(let k=0;k<3;k++) gl.vertexAttribPointer(3+k,4,gl.FLOAT,false,48,k*16);
        }
        if(b.lb){ gl.bindBuffer(gl.ARRAY_BUFFER,b.lb); gl.vertexAttribPointer(8,2,gl.FLOAT,false,0,0); }
        gl.drawElementsInstanced(gl.TRIANGLES,b.count,gl.UNSIGNED_SHORT,0,b.n);
        stat.drawn++; stat.calls++; stat.tris+=(b.count/3)*b.n;
      }
    }
  },

  /** The whole pass: every cascade into the atlas, then the separable blur. Called once
   *  a frame from `draw()`, before anything that reads it. Hands the framebuffer, the
   *  viewport, the depth state and the sampler units back as it found them.
   *
   *  # What it does not do again (round 18ej)
   *
   *  Robin: "I think shadows are a bit performance heavy, could you optimize them some?"
   *  Measured on Tel Vos with two rings - 25 chunks, 1,670 statics - the pass submitted
   *  **2.97 million triangles and 1,711 draws a frame**: the terrain and every static,
   *  three times over, every frame, whether or not anything had changed.
   *
   *  Nothing about the map depends on the frame. It depends on three things: the cascade
   *  matrices (which are quantized, so they stand still while the camera does), the set
   *  of casters, and the pose of the ones that move. So the still casters are drawn into
   *  a cache of their own and kept, with their depth, and a frame that changes nothing
   *  reuses the blurred atlas whole - nought triangles, nought draws. A frame in which
   *  only the banners moved copies the cache back and redraws the banners: measured on
   *  Tel Vos, sixteen triangles apiece instead of 2.97 million.
   *
   *  The picture is the same picture. The casters are the same geometry with the same
   *  matrices; the depth test keeps the nearest fragment whatever order it arrives in, so
   *  drawing the movers after the copy is the same map as drawing everything at once, and
   *  `t_render` asserts exactly that by reading both back. */
  drawShadowMap(eye, now, room, scat, sunCol){
    const gl=this.gl, o=this.opts;
    this.shadowDrawn=false;
    this._shadowNow=null;
    this._shadowStat=null;
    /* A room has no sun to cast from - its light is the cell's own record from a fixed
       direction, and MGE masks its shadows indoors too (mgeflags bit 22). */
    if(room) return false;
    const res=this.shadowRes();
    /* Which way the light travels: MGE takes -sunPos by day and sunVec by night, both of
       which this renderer has already resolved into `_sunNow` - the direction *towards*
       the sun, mirrored above the horizon after dark (25_sky.js `sunLight`). */
    const s=this._sunNow;
    if(!s) return false;
    const lightVec=[-s[0], -s[1], -s[2]];
    /* Nothing to cast onto, and nothing to cast: the pass is the geometry twice over, so
       it stands down rather than clearing an atlas nobody will read. */
    const G=(this._groups && this._groups.src===this.statics)? this._groups : null;
    const anyStatic=!!((G && G.all.length) || (this.statics && this.statics.length));
    const anyGround=!!((this.cellChunks && this.cellChunks.length) || this.ground);
    if(!anyStatic && !anyGround) return false;

    /* Round 18ej: a sun this dim cannot darken a single pixel, so there is nothing to
       draw a map for. `gdnShade` gives up below 2/255 of light, and its light is
       x/(0.4+x) with x = lambert * luma(sunCol) * sunK; lambert is at most 1, so this is
       the brightest shadow the frame could possibly have. At night the sun's colour is
       all but black and the whole pass is waste - MGE mirrors the light below the horizon
       and its own shade term goes to nothing there the same way. */
    const vis=(scat && now && now.sunVis!=null)? now.sunVis : 1;
    const sunK=0.25+0.75*vis;
    if(sunCol){
      const luma=sunCol[0]*0.36+sunCol[1]*0.53+sunCol[2]*0.11;
      const x=Math.max(0,luma)*sunK;
      if(x/(0.4+x) < 2/255) return false;
    }

    /* Round 18fa: this frame's boxes - the nearest sized to what the camera looks at,
       the outermost to the fog (`shadowRadii`). The atlas is one column per box and is
       made for the count, so a frame that needs four columns and one that needs three
       do not share targets; the count changes when the fog or the zoom does, which is
       when the cache is stale anyway. */
    const {hit, d:dHit}=this.shadowHit(eye);
    // The reach in steps of a quarter cell: a number about the fog, never about the frame.
    const reach=Math.min(SHADOW_R_MAX, 2048*Math.ceil(((this._shadowReach!=null)? this._shadowReach : SHADOW_R_MAX)/2048));
    const radii=this.shadowRadii(reach);
    const N=radii.length;
    const T=this._shadowTargets(res, N);
    if(!T) return false;

    this._gpuMark('shadow');
    /* One centre for all the boxes, worked out for the nearest cascade's radius (18eg
       item 3). MGE centres each cascade its own radius ahead of the camera, which is right
       when the two boxes are 1,000 and 4,000 units and wrong once the outermost is 20,000:
       centred that far ahead it starts at the camera's feet, so the camera's own
       surroundings sit against its edge, and the band where one cascade blends into the
       next can fall outside the next box entirely - a hard switch again, in a new place.
       Concentric boxes are properly nested, so every band has the next cascade under it. */
    let look=this.shadowLookAt(eye, radii[0]);
    /* Round 18fa: from an overview - the eye far above and outside the loaded cells -
       a radius ahead along the view is a point in the air with no ground under it, and
       `shadowLookAt` can only guess its height; the boxes then hang 16,384 units deep
       around a point thousands of units over the scene and miss the ground entirely
       (measured at Balmora from 30,000 units: 2,120 triangles a cascade where the near
       view casts a million). Where the view does meet the ground, that meeting point is
       what the camera is looking at, so the boxes are centred there instead. */
    if(hit && this.groundZ){
      let gz=null; try{ gz=this.groundZ(look[0],look[1]); }catch(_){ gz=null; }
      if(gz==null || !isFinite(gz)) look=hit;
    }
    const vp=new Float32Array(16*N);
    const mats=[];
    /* 18fc: each box's depth window over MGE's, for the receiver's exponential. */
    const zf=new Float32Array(SHADOW_MAX_CASCADES);
    const zback=this.shadowBack();
    for(let layer=0;layer<N;layer++){
      zf[layer]=(2*shadowZRange(radii[layer])+zback)/(2*SHADOW_ZRANGE);
      /* Round 18fa/18fb: the coarse boxes move in steps of a thirty-second of their
         width (`_shadowLayerVP`), the nearest box texel by texel. The box can then sit up
         to a thirty-second of its width off the camera's own centre; the next box in is a
         quarter of its size, so it stays nested with room to spare, and the outermost is
         given a tenth over the fog's reach in `shadowRadii`. */
      const m=this._shadowLayerVP(radii[layer], look, lightVec, res, layer>0? res/32 : 1);
      mats.push(m);
      vp.set(m, layer*16);
    }

    this._ensureShadowCasters();

    /* ---- which columns of the cache are still the answer? (18ej, per column since 18fa) ----
       Each cascade quantizes its own translation to its own texels, and they are wildly
       different sizes: the nearest box moves with almost every step of the camera, the
       outermost survives a walk - and the outermost is the one that costs. So the cache
       is kept and compared a column at a time: a frame that turned the camera a little
       redraws the near column and copies the rest. */
    const mov=this._shadowMovers();
    if(!mov.groups) mov.groups=new Set();   // a test may plant its own list of movers
    const cacheable=!this._shadowNoCache;
    const C=this._shCache;
    const stale=new Array(N).fill(true);
    let still=false;
    if(cacheable && C && C.res===res && C.n===N && C.rev===(this._casterRev|0)
       && C.G===G && C.statics===this.statics && C.chunks===this.cellChunks && C.ground===this.ground){
      still=true;
      for(let layer=0;layer<N;layer++){
        let same=true;
        for(let k=0;k<16;k++) if(C.vp[layer*16+k]!==vp[layer*16+k]){ same=false; break; }
        stale[layer]=!same;
        if(!same) still=false;
      }
    }
    const cols=cacheable? stale.reduce((a,b)=>a+(b?1:0),0) : N;
    const stat={drawn:0, tris:0, calls:0, still, movers:mov.list.length+mov.groups.size, cached:cacheable,
                res, cascades:N, cols, radii, reach:Math.round(reach), dHit:isFinite(dHit)? Math.round(dHit) : null};

    /* Nothing moved and nothing changed: the atlas on unit SHADOW_UNIT is still the
       answer, blur and all. This is the whole of a still frame's shadow cost. */
    if(still && !mov.list.length && !mov.groups.size){
      gl.activeTexture(gl.TEXTURE0+SHADOW_UNIT); gl.bindTexture(gl.TEXTURE_2D,T.texA); gl.activeTexture(gl.TEXTURE0);
      this._gpuMark(null);
      this._shadowNow={vp, n:N, rcp:1/res, sunK, res, mats, radii, zf, far:radii[N-1], zback, fog:this._shadowNearFog(now,scat)};
      this.shadowDrawn=true;
      this._shadowStat=stat;
      if(this._passLog) this._passLog.push('shadow:'+res+':0');
      return true;
    }

    gl.disable(gl.BLEND);
    gl.enable(gl.DEPTH_TEST); gl.depthFunc(gl.LEQUAL); gl.depthMask(true);
    /* Cleared to the far depth, as MGE's clear pass does: a texel nothing was drawn into
       says "the light reaches the ground here", whatever the receiver's depth is. */
    const clearFar=()=>{
      gl.clearColor(SHADOW_ESM_SCALE,SHADOW_ESM_SCALE,SHADOW_ESM_SCALE,SHADOW_ESM_SCALE);
      gl.clear(gl.COLOR_BUFFER_BIT|gl.DEPTH_BUFFER_BIT);
    };
    const layers=[];
    /* 18eg item 3 skipped, in the coarse cascades, any caster whose box was under two
       texels of that cascade - and a tree is drawn as its parts, so in the outermost box
       its leaf clusters were skipped one by one while its trunk was kept: Robin, at Tel
       Vos, "shadows basically turning off at a certain distance [...] especially those
       with a large canopy". A two-texel object is a mark in the map. Nothing is skipped
       now (18fb); the cache is what keeps the outermost box cheap. */
    for(let layer=0;layer<N;layer++)
      layers.push({VP:mats[layer], planes:this._frustum(mats[layer]), tiny:0});

    if(!cacheable){
      /* No cache: the pass as it was, straight into the atlas. */
      gl.bindFramebuffer(gl.FRAMEBUFFER,T.fbA);
      gl.viewport(0,0,T.W,T.H); clearFar();
      for(let layer=0;layer<N;layer++){
        gl.viewport(layer*res,0,res,res);
        this._shadowCast(layers[layer].VP, layers[layer].planes, layers[layer].tiny, 'all', stat);
      }
    }else{
      if(!still){
        /* The still casters of the columns that moved into their own target, kept
           unblurred with their own depth; the other columns stand. The scissor keeps
           the clear to the column - a clear without one is the whole atlas. */
        gl.bindFramebuffer(gl.FRAMEBUFFER,T.fbC);
        gl.enable(gl.SCISSOR_TEST);
        for(let layer=0;layer<N;layer++){
          if(!stale[layer]) continue;
          gl.viewport(layer*res,0,res,res); gl.scissor(layer*res,0,res,res);
          clearFar();
          this._shadowCast(layers[layer].VP, layers[layer].planes, layers[layer].tiny, 'still', stat);
        }
        gl.disable(gl.SCISSOR_TEST);
        this._shCache={res, n:N, rev:this._casterRev|0, G, statics:this.statics,
                       chunks:this.cellChunks, ground:this.ground, vp:new Float32Array(vp)};
      }
      /* The cache into the atlas - colour and depth both, so what moves is occluded by
         what does not exactly as it would have been had they been drawn together. */
      gl.bindFramebuffer(gl.READ_FRAMEBUFFER,T.fbC);
      gl.bindFramebuffer(gl.DRAW_FRAMEBUFFER,T.fbA);
      gl.blitFramebuffer(0,0,T.W,T.H, 0,0,T.W,T.H,
                         gl.COLOR_BUFFER_BIT|gl.DEPTH_BUFFER_BIT, gl.NEAREST);
      gl.bindFramebuffer(gl.READ_FRAMEBUFFER,null);
      gl.bindFramebuffer(gl.FRAMEBUFFER,T.fbA);
      if(mov.list.length || mov.groups.size){
        gl.enable(gl.DEPTH_TEST); gl.depthFunc(gl.LEQUAL); gl.depthMask(true);
        for(let layer=0;layer<N;layer++){
          gl.viewport(layer*res,0,res,res);
          this._shadowCast(layers[layer].VP, layers[layer].planes, layers[layer].tiny, 'moving', stat);
        }
      }
    }

    /* ---- the blur, MGE's separable five-tap: the atlas into B across, back into A down.
       One atlas texel across, one down; the cascade's own texel is the same size. ---- */
    const pf=this._ensureShadowSoften();
    gl.useProgram(pf.p);
    gl.bindVertexArray(null);
    gl.disable(gl.DEPTH_TEST); gl.depthMask(false);
    gl.viewport(0,0,T.W,T.H);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindFramebuffer(gl.FRAMEBUFFER,T.fbB);
    gl.bindTexture(gl.TEXTURE_2D,T.texA);
    gl.uniform1i(pf.u.uTex,0); gl.uniform2f(pf.u.uStep,1/T.W,0);
    gl.drawArrays(gl.TRIANGLES,0,3);
    gl.bindFramebuffer(gl.FRAMEBUFFER,T.fbA);
    gl.bindTexture(gl.TEXTURE_2D,T.texB);
    gl.uniform2f(pf.u.uStep,0,1/T.H);
    gl.drawArrays(gl.TRIANGLES,0,3);

    /* ---- handed back the way the frame expects it ---- */
    gl.bindTexture(gl.TEXTURE_2D,null);
    gl.activeTexture(gl.TEXTURE0+SHADOW_UNIT); gl.bindTexture(gl.TEXTURE_2D,T.texA); gl.activeTexture(gl.TEXTURE0);
    gl.bindFramebuffer(gl.FRAMEBUFFER,null);
    gl.viewport(0,0,this.cv.width,this.cv.height);
    gl.enable(gl.DEPTH_TEST); gl.depthFunc(gl.LEQUAL); gl.depthMask(true);
    gl.disable(gl.BLEND);
    gl.bindVertexArray(null);
    /* The face culling as `draw()` leaves it for the sky, the terrain and the grass -
       off, counter-clockwise - because `_meshMode` turned it on for the casters and the
       mirror of that state is kept on this object. */
    gl.disable(gl.CULL_FACE); this._culling=false;
    gl.frontFace(gl.CCW); this._cw=false;
    /* Units 7-9 and the unit-0 shadow, because `_meshTex` bound textures on 0 and the
       group instances on 11 - the same discipline every other pass follows. */
    this._bindLights(this._lgrid);
    this._gpuMark(null);

    /* MGE's sun-visibility term, on the CPU: (0.25 + 0.75 * sunVis), which is what keeps
       a shadow from being drawn at full strength under a sun the weather has half
       hidden. No atmosphere, no weather: the sun is whole. */
    this._shadowNow={vp, n:N, rcp:1/res, sunK, res, mats, radii, zf, far:radii[N-1], zback, fog:this._shadowNearFog(now,scat)};
    this.shadowDrawn=true;
    this._shadowStat=stat;
    if(this._passLog) this._passLog.push('shadow:'+res+':'+stat.drawn);
    return true;
  },

  /** The group caster program for one instance-fetch mode - the same three the object
   *  pass has (`_progM`, `_progB`), over the depth-only fragment shader. */
  _progShadowGroup(mode){
    this._shProg=this._shProg||{};
    if(!this._shProg[mode]) this._shProg[mode]=this._prog(VS_STATIC_M(mode),FS_SHADOW_CAST);
    return this._shProg[mode];
  },
});
