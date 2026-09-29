/* =====================================================================================
   The outline highlight - round 18dq.

   Robin: "I want to add another highlight mode for objects, and add it to the toggle for
   'Highlight objects' in the viewport upper right corner (similar to how we toggle
   through Highlight culling). It should not show the entire mesh as a highlight, but
   instead show a outline. If part of the line ends up behind other meshes, it should
   have a ghost effect where it's a bit dimmer but we still see it (reads behind the
   other object). If the outline stretches outside of the viewport (due to the mesh being
   too big for the viewport), the outline gets drawn at the same width as around the
   mesh, but around the screen in those places the mesh 'touches' the edge of the screen.
   Similar to how Unreal Engine's outline selection work."

   The Highlight objects button cycles off, fill (the gold wash 17k had, `hlMode` 'fill'),
   outline (`hlMode` 'outline'). The outline is an image effect on the finished frame,
   which is why it lives here beside the SSAO and the FXAA rather than in the object
   pass: what it needs is a *mask* of the pointed-at objects and the frame's depth.

   1. The mask (`drawOutlineMask`): the highlighted batches drawn into a frame-sized
      texture through a shader that writes nothing but 1 - twice. Once with the depth
      test off, into red: every pixel the objects cover, seen or not. Once tested against
      the frame's resolved depth (the same `depTex` the water and the occlusion read),
      into green: the pixels where they are actually in front. The alpha cut is the
      object pass's own (`_meshTex` sets the same uniforms), so a leaf cluster's outline
      follows its leaves and not its quads.
   2. The line (`drawOutline`): a full-screen pass over the frame. A pixel outside the
      mask within OUTLINE_W pixels of a covered one is outline; it is drawn at full
      strength where the nearest covered pixels are visible and at OUTLINE_GHOST where
      they are all hidden - the ghost, reading through whatever stands in front. A
      covered pixel within OUTLINE_W of the viewport's edge is outline too - the mesh
      "touches" the edge there, and the line runs along the edge at the same width.
      The colour is the highlight gold every other highlight uses.

   Drawn before the FXAA pass, so the line takes the same anti-aliasing as the frame.
   Both passes need the offscreen frame (`wantsOffscreen`). No plugin state, no export.
   (No backticks inside the shader strings: template literals.) */

const OUTLINE_W=2;          // pixels, either side of the silhouette
const OUTLINE_GHOST=0.42;   // the hidden part's opacity
const OUTLINE_COL=[1.00,0.82,0.30];

/* The mask writer, driven by VS_STATIC: the object's texture and alpha rule alone. Only
   the varyings it reads are declared; a vertex output with no fragment input is fine. */
const FS_OUTLINE_MASK=`#version 300 es
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
  o=vec4(1.0);
}`;

const FS_OUTLINE=`#version 300 es
precision highp float;
in vec2 vUV; uniform sampler2D uMask; uniform vec2 uRcpRes; uniform vec3 uCol; uniform float uGhost; out vec4 o;
const int W=${OUTLINE_W};
void main(){
  vec2 m=texture(uMask, vUV).rg;      // r: covered by the objects; g: covered and in front
  vec2 px=vUV/uRcpRes, size=1.0/uRcpRes;
  float hit=0.0, vis=0.0;
  if(m.r<0.5){
    for(int dy=-W; dy<=W; dy++) for(int dx=-W; dx<=W; dx++){
      if(dx*dx+dy*dy > W*W) continue;
      vec2 s=texture(uMask, vUV+vec2(float(dx),float(dy))*uRcpRes).rg;
      hit=max(hit,s.r); vis=max(vis,s.g);
    }
  } else if(px.x<float(W) || px.y<float(W) || px.x>size.x-float(W) || px.y>size.y-float(W)){
    hit=1.0; vis=m.g;   // the mesh runs off the screen here: the line follows the edge
  }
  if(hit<0.5) discard;
  o=vec4(uCol, vis>0.5 ? 1.0 : uGhost);
}`;

Object.assign(Renderer.prototype,{
  /** The mask target at the frame's size, its depth the frame's resolved depth texture. */
  _outlineTarget(T){
    const gl=this.gl, W=T.W, H=T.H, t=this._olT;
    if(t && t.W===W && t.H===H && t.dep===T.depTex) return t;
    if(t){ try{ gl.deleteFramebuffer(t.fb); }catch(_){ } try{ gl.deleteTexture(t.tex); }catch(_){ } }
    const n={W,H,dep:T.depTex};
    n.tex=gl.createTexture(); gl.bindTexture(gl.TEXTURE_2D,n.tex);
    gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,W,H,0,gl.RGBA,gl.UNSIGNED_BYTE,null);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.CLAMP_TO_EDGE);
    n.fb=gl.createFramebuffer(); gl.bindFramebuffer(gl.FRAMEBUFFER,n.fb);
    gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,n.tex,0);
    gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.DEPTH_ATTACHMENT,gl.TEXTURE_2D,T.depTex,0);
    n.ok=gl.checkFramebufferStatus(gl.FRAMEBUFFER)===gl.FRAMEBUFFER_COMPLETE;
    gl.bindFramebuffer(gl.FRAMEBUFFER,null); gl.bindTexture(gl.TEXTURE_2D,null);
    this._olT=n;
    return n;
  },
  /** Whether this frame draws an outline: the mode, and something pointed at. */
  wantsOutline(){
    return this.hlMode==='outline' && !!(this.hlStatics && this.hlStatics.length);
  },
  /** Step 1: the mask, from the frame's resolved depth. Called by `endWaterFrame` once
   *  the depth is resolved and before the frame is drawn to its destination. */
  drawOutlineMask(T){
    const gl=this.gl;
    if(!this.wantsOutline() || !this._frame) return false;
    const A=this._outlineTarget(T);
    if(!A.ok) return false;
    if(!this.progOutlineMask) this.progOutlineMask=this._prog(VS_STATIC,FS_OUTLINE_MASK);
    const pr=this.progOutlineMask; gl.useProgram(pr.p);
    gl.uniformMatrix4fv(pr.u.uVP,false,this._frame.VP);
    gl.bindFramebuffer(gl.FRAMEBUFFER,A.fb); gl.viewport(0,0,T.W,T.H);
    gl.disable(gl.BLEND); gl.depthMask(false);
    gl.clearColor(0,0,0,0); gl.clear(gl.COLOR_BUFFER_BIT);
    const draw=()=>{
      for(const b of this.hlStatics){
        if(!b.n) continue;
        this._ensureVao(b);
        this._meshTex(pr,b);      // the texture, the alpha rule, the animated node's matrix
        this._meshMode(pr,b,true);
        gl.bindVertexArray(b.vao);
        gl.drawElementsInstanced(gl.TRIANGLES,b.count,gl.UNSIGNED_SHORT,0,b.n);
      }
    };
    // Red: covered, seen or not.
    gl.disable(gl.DEPTH_TEST); gl.colorMask(true,false,false,false); draw();
    // Green: covered and in front of what the frame drew.
    gl.enable(gl.DEPTH_TEST); gl.depthFunc(gl.LEQUAL); gl.colorMask(false,true,false,false); draw();
    gl.colorMask(true,true,true,true);
    gl.bindVertexArray(null);
    gl.depthMask(true);
    this._olReady=true;
    return true;
  },
  /** Step 2: the line, blended over the frame in `dst` (null: the canvas). */
  drawOutline(T,dst){
    const gl=this.gl, A=this._olT;
    if(!A || !this._olReady) return false;
    this._olReady=false;
    if(!this.progOutline) this.progOutline=this._prog(VS_BLIT,FS_OUTLINE);
    const pr=this.progOutline; gl.useProgram(pr.p);
    gl.bindFramebuffer(gl.FRAMEBUFFER,dst||null); gl.viewport(0,0,T.W,T.H);
    gl.disable(gl.DEPTH_TEST); gl.depthMask(false);
    gl.enable(gl.BLEND); gl.blendFunc(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA);
    gl.bindVertexArray(null);
    gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D,A.tex);
    gl.uniform1i(pr.u.uMask,0); gl.uniform2f(pr.u.uRcpRes,1/T.W,1/T.H);
    gl.uniform3fv(pr.u.uCol,OUTLINE_COL); gl.uniform1f(pr.u.uGhost,OUTLINE_GHOST);
    gl.drawArrays(gl.TRIANGLES,0,3);
    gl.disable(gl.BLEND);
    gl.bindTexture(gl.TEXTURE_2D,null);
    this.outlineDrawn=true;   // for the tests
    return true;
  },
});
