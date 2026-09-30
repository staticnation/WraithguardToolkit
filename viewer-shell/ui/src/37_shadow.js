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

/* MGE: shadowNearRadius, shadowFarRadius. Two cascades; the atlas is twice as wide as
   it is tall and each cascade owns half of it. */
const SHADOW_R=[1000.0, 4000.0];
/* MGE: zrange = kCellSize. The box's depth is twice this, the light camera one of them
   back from the centre, so a caster within a cell-size of the ground either way is in. */
const SHADOW_ZRANGE=8192.0;
/* MGE: ESM_scale, the factor that puts linear depth in [0,1] into (most of) the
   half-float range. The receiving side divides by the same number. */
const SHADOW_ESM_SCALE=32768.0;
/* What the map is drawn at when the install has nothing to say. MGE's own default, and
   the range its runtime clamps the setting to. */
const SHADOW_RES_DEFAULT=1024, SHADOW_RES_MIN=256, SHADOW_RES_MAX=2048;

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
    return Math.min(r, Math.floor(max/2));
  },

  /** The atlas and the scratch it is blurred through: two cascades wide, one tall, a
   *  single red half-float channel (MGE stores its ESM in one channel of an FP16 too).
   *  Linear filtering, because the lookup reads the blurred depth between texels.
   *  Null when this context cannot render half-float colour, which is what the switch
   *  stands down on. */
  _shadowTargets(res){
    const gl=this.gl, W=2*res, H=res, t=this._shT;
    if(t && t.res===res) return t.ok? t : null;
    if(t){
      try{ gl.deleteFramebuffer(t.fbA); }catch(_){ }
      try{ gl.deleteFramebuffer(t.fbB); }catch(_){ }
      try{ gl.deleteTexture(t.texA); }catch(_){ }
      try{ gl.deleteTexture(t.texB); }catch(_){ }
      try{ gl.deleteRenderbuffer(t.dep); }catch(_){ }
    }
    if(this._shHalf===undefined){
      try{ this._shHalf=!!(gl.getExtension('EXT_color_buffer_half_float')||gl.getExtension('EXT_color_buffer_float')); }
      catch(_){ this._shHalf=false; }
    }
    const n={res,W,H,ok:false};
    this._shT=n;
    if(!this._shHalf) return null;
    const mk=()=>{
      const x=gl.createTexture(); gl.bindTexture(gl.TEXTURE_2D,x);
      gl.texImage2D(gl.TEXTURE_2D,0,gl.R16F,W,H,0,gl.RED,gl.HALF_FLOAT,null);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.CLAMP_TO_EDGE);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.CLAMP_TO_EDGE);
      return x;
    };
    n.texA=mk(); n.texB=mk();
    n.dep=gl.createRenderbuffer(); gl.bindRenderbuffer(gl.RENDERBUFFER,n.dep);
    gl.renderbufferStorage(gl.RENDERBUFFER,gl.DEPTH_COMPONENT16,W,H);
    n.fbA=gl.createFramebuffer(); gl.bindFramebuffer(gl.FRAMEBUFFER,n.fbA);
    gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,n.texA,0);
    gl.framebufferRenderbuffer(gl.FRAMEBUFFER,gl.DEPTH_ATTACHMENT,gl.RENDERBUFFER,n.dep);
    const okA=gl.checkFramebufferStatus(gl.FRAMEBUFFER)===gl.FRAMEBUFFER_COMPLETE;
    n.fbB=gl.createFramebuffer(); gl.bindFramebuffer(gl.FRAMEBUFFER,n.fbB);
    gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,n.texB,0);
    const okB=gl.checkFramebufferStatus(gl.FRAMEBUFFER)===gl.FRAMEBUFFER_COMPLETE;
    gl.bindFramebuffer(gl.FRAMEBUFFER,null); gl.bindTexture(gl.TEXTURE_2D,null);
    n.ok=okA&&okB;
    return n.ok? n : null;
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
  _shadowLayerVP(radius, look, lightVec, res){
    /* The centre quantized to 16 units: the part of the motion the matrix is *built* on,
       so it changes in steps and the projection does not swim with every frame's drift. */
    const q=v=>16*Math.floor(0.0625*v);
    const at=[q(look[0]), q(look[1]), q(look[2])];
    const cam=[at[0]-SHADOW_ZRANGE*lightVec[0], at[1]-SHADOW_ZRANGE*lightVec[1], at[2]-SHADOW_ZRANGE*lightVec[2]];
    /* Straight down the light, the up vector has to be off the light's own axis. The
       sun's path in this tool is tilted (25_sky.js `sunDir`) and never exactly vertical,
       but a planted sun in a test can be, and a degenerate lookAt draws nothing at all. */
    const up=Math.abs(lightVec[2])>0.999? [0,1,0] : [0,0,1];
    const V=M4.lookAt(cam, at, up);
    const P=M4.ortho(2*radius, (1+Math.abs(lightVec[2]))*radius, 0, 2*SHADOW_ZRANGE);
    const VP=M4.mul(P,V);
    /* The remainder - what the 16-unit snap left over - added to the translation in clip
       units and quantized to whole texels. This is the half that stops the shimmer when
       the camera turns: the box slides by whole texels, so a texel keeps covering the
       same ground. */
    const d=[at[0]-look[0], at[1]-look[1], at[2]-look[2]];
    const dx=VP[0]*d[0]+VP[4]*d[1]+VP[8]*d[2];
    const dy=VP[1]*d[0]+VP[5]*d[1]+VP[9]*d[2];
    const dz=VP[2]*d[0]+VP[6]*d[1]+VP[10]*d[2];
    const qz=2.0/res;
    VP[12]+=qz*Math.floor(dx/qz);
    VP[13]+=qz*Math.floor(dy/qz);
    VP[14]+=dz;
    return VP;
  },

  /** The groups' runs and bakes that this cascade can see, into arrays of the pass's
   *  own. A copy of `_cullGroups`'s merge rather than a call to it, deliberately: that
   *  one caches on the matrix it last culled for (`G.cullVP`) and is called again by the
   *  camera pass a moment later, so borrowing it would make every frame cull three times
   *  and keep no answer. */
  _shadowCullGroups(G,planes){
    const rb=G.runBox, rbase=G.runBase, rn=G.runN, rp=G.runPart, pc=G.partCount, po=G.partOff;
    const kb=G.bkBox, ko=G.bkOff, kc=G.bkCount;
    const pl=planes;
    const seen=(box,o)=>{
      for(let i=0;i<6;i++){
        const p=pl[i];
        const x=p[0]>=0? box[o+3] : box[o], y=p[1]>=0? box[o+4] : box[o+1], z=p[2]>=0? box[o+5] : box[o+2];
        if(p[0]*x+p[1]*y+p[2]*z+p[3]<0) return false;
      }
      return true;
    };
    for(const g of G.all){
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

  /** The whole pass: both cascades into the atlas, then the separable blur. Called once
   *  a frame from `draw()`, before anything that reads it. Hands the framebuffer, the
   *  viewport, the depth state and the sampler units back as it found them. */
  drawShadowMap(eye, now, room, scat){
    const gl=this.gl, o=this.opts;
    this.shadowDrawn=false;
    this._shadowNow=null;
    /* A room has no sun to cast from - its light is the cell's own record from a fixed
       direction, and MGE masks its shadows indoors too (mgeflags bit 22). */
    if(room) return false;
    const res=this.shadowRes();
    const T=this._shadowTargets(res);
    if(!T) return false;
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

    this._gpuMark('shadow');
    const vp=new Float32Array(32);
    const mats=[];
    for(let layer=0;layer<2;layer++){
      const m=this._shadowLayerVP(SHADOW_R[layer], this.shadowLookAt(eye, SHADOW_R[layer]), lightVec, res);
      mats.push(m);
      vp.set(m, layer*16);
    }

    /* ---- the atlas ---- */
    gl.bindFramebuffer(gl.FRAMEBUFFER,T.fbA);
    gl.viewport(0,0,T.W,T.H);
    gl.disable(gl.BLEND);
    gl.enable(gl.DEPTH_TEST); gl.depthFunc(gl.LEQUAL); gl.depthMask(true);
    /* Cleared to the far depth, as MGE's clear pass does: a texel nothing was drawn into
       says "the light reaches the ground here", whatever the receiver's depth is. */
    gl.clearColor(SHADOW_ESM_SCALE,SHADOW_ESM_SCALE,SHADOW_ESM_SCALE,SHADOW_ESM_SCALE);
    gl.clear(gl.COLOR_BUFFER_BIT|gl.DEPTH_BUFFER_BIT);

    if(!this.progShadowGround) this.progShadowGround=this._prog(VS_GROUND,FS_SHADOW_CAST);
    if(!this.progShadowStatic) this.progShadowStatic=this._prog(VS_STATIC,FS_SHADOW_CAST);
    let drawn=0;
    for(let layer=0;layer<2;layer++){
      const VP=mats[layer];
      const planes=this._frustum(VP);
      gl.viewport(layer*res,0,res,res);

      /* The terrain, in world space already, so the matrix is all it needs. Layer 0's
         indices only: the later layers are the paint and the blends over the same
         quads and would cast the same shadow twice. */
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
            drawn++;
          }
        }else if(this.ground){
          gl.bindVertexArray(this.ground.vao);
          gl.drawElements(gl.TRIANGLES,this.ground.count,gl.UNSIGNED_SHORT,0);
          drawn++;
        }
      }

      /* The statics - the groups through their own program, then whatever was left out of
         them (skinned meshes, the moths, a hovered texture's batch) through the batch
         program. Culling is the mesh's own, so a two-sided leaf quad casts from both
         sides, and MGE's own note applies: casters drawn with one winding "causes false
         shadows when cast on the reverse side of a two-sided poly". */
      if(anyStatic){
        const own=G? this._legacyStatics : this.statics;
        if(G && G.all.length){
          gl.activeTexture(gl.TEXTURE0+INST_UNIT); gl.bindTexture(gl.TEXTURE_2D,G.instTex); gl.activeTexture(gl.TEXTURE0);
          this._shadowCullGroups(G,planes);
          gl.bindVertexArray(G.vao);
          const pm=this._progShadowGroup(G.multi? 'multi' : 'base');
          gl.useProgram(pm.p); this._shReset(pm); this._tex0=undefined;
          gl.uniformMatrix4fv(pm.u.uVP,false,VP);
          gl.uniform1i(pm.u.uTex,0);
          gl.uniform1i(pm.u.uInst,INST_UNIT);
          for(const g of G.all){
            if(!g.svis) continue;
            this._meshTex(pm,g.rep);
            this._meshMode(pm,g.rep,true);
            if(G.multi){
              for(let at=0;at<g.svis;at+=MAXRUN){
                const n=Math.min(MAXRUN,g.svis-at);
                gl.uniform4iv(pm.u.uRunBase, g.sbase, at, ((n+3)>>2)<<2);
                G.multi.multiDrawElementsInstancedWEBGL(gl.TRIANGLES, g.scount, at, gl.UNSIGNED_INT, g.soff, at, g.sinst, at, n);
              }
            }else{
              for(let i=0;i<g.svis;i++){
                this._u1i(pm,'uBase',g.sbase[i]);
                gl.drawElementsInstanced(gl.TRIANGLES,g.scount[i],gl.UNSIGNED_INT,g.soff[i],g.sinst[i]);
              }
            }
            drawn+=g.svis;
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
              this._meshMode(pb,g.rep,true);
              if(G.multi) G.multi.multiDrawElementsWEBGL(gl.TRIANGLES, g.sbcount, 0, gl.UNSIGNED_INT, g.sboff, 0, g.sbvis);
              else for(let i=0;i<g.sbvis;i++) gl.drawElements(gl.TRIANGLES,g.sbcount[i],gl.UNSIGNED_INT,g.sboff[i]);
              drawn+=g.sbvis;
            }
          }
        }
        if(own && own.length){
          const ps=this.progShadowStatic;
          gl.useProgram(ps.p); this._shReset(ps); this._tex0=undefined;
          gl.uniformMatrix4fv(ps.u.uVP,false,VP);
          gl.uniform1i(ps.u.uTex,0);
          for(const b of own){
            if(!b.n || !b.part) continue;
            this._ensureVao(b);
            if(!b.vao) continue;
            this._meshTex(ps,b);
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
            drawn++;
          }
        }
      }
    }

    /* ---- the blur, MGE's separable five-tap: the atlas into B across, back into A down.
       One atlas texel across, one down; the cascade's own texel is the same size. ---- */
    if(!this.progShadowSoften) this.progShadowSoften=this._prog(VS_BLIT,FS_SHADOW_SOFTEN);
    const pf=this.progShadowSoften;
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
    const vis=(scat && now && now.sunVis!=null)? now.sunVis : 1;
    /* Wraithguard: how far the shadows reach - MGE's fogMWScalar, Morrowind's own linear
       near fog (nearFogStart .. nearFogRange), which is the pair its SSAO fades by too
       (ssaoFade: `adjustFog`'s fit, the Fog density slider applied). */
    const nf=this.ssaoFade? this.ssaoFade(now,scat) : [1280, 7168];
    this._shadowNow={vp, rcp:1/res, sunK:0.25+0.75*vis, res, mats, fog:nf};
    this.shadowDrawn=true;
    if(this._passLog) this._passLog.push('shadow:'+res+':'+drawn);
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
