
/* =====================================================================================
   Wraithguard: line overlays in the viewport - path grids, light radii, collision, door
   links, NPC reach, the review's markers, the merged ground's contested vertices and the
   measuring tape (48_wg_tools.js builds them).

   Each overlay is a named set of line segments with a colour per vertex, in the scene's
   own space. They are drawn after the scene, where the orbit pivot is: once depth-tested,
   and - for a set that asks for it - once more faintly through whatever stands in front,
   so a marker inside a hill or behind a wall is still found. Plain GL_LINES, one pixel
   wide, the only width a desktop driver honours (the pivot's reasoning, 06_gl.js).
   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
const VS_WGOVL=`#version 300 es
precision highp float;
layout(location=0) in vec3 aPos;
layout(location=1) in vec4 aCol;
uniform mat4 uVP;
out vec4 vC;
void main(){ vC=aCol; gl_Position=uVP*vec4(aPos,1.0); }`;
const FS_WGOVL=`#version 300 es
precision highp float;
in vec4 vC;
uniform float uA;
out vec4 o;
void main(){ o=vec4(vC.rgb, vC.a*uA); }`;

/** Collects line segments: positions and a colour per vertex. */
class OvlLines{
  constructor(){ this.p=[]; this.c=[]; }
  get size(){ return this.p.length/3; }
  seg(a,b,col){
    this.p.push(a[0],a[1],a[2], b[0],b[1],b[2]);
    const al=col.length>3? col[3] : 1;
    this.c.push(col[0],col[1],col[2],al, col[0],col[1],col[2],al);
  }
  /** A ring of radius `r` round `c`, in the plane across `axis` ('z' flat, 'x', 'y'). */
  circle(c,r,col,axis,n){
    n=n||48; axis=axis||'z';
    let prev=null;
    for(let i=0;i<=n;i++){
      const t=i/n*Math.PI*2, a=Math.cos(t)*r, b=Math.sin(t)*r;
      const p= axis==='z'? [c[0]+a,c[1]+b,c[2]] : axis==='x'? [c[0],c[1]+a,c[2]+b] : [c[0]+a,c[1],c[2]+b];
      if(prev) this.seg(prev,p,col);
      prev=p;
    }
  }
  cross(p,s,col){
    this.seg([p[0]-s,p[1],p[2]],[p[0]+s,p[1],p[2]],col);
    this.seg([p[0],p[1]-s,p[2]],[p[0],p[1]+s,p[2]],col);
    this.seg([p[0],p[1],p[2]-s],[p[0],p[1],p[2]+s],col);
  }
  /** The twelve edges of a box `{x0,y0,z0,x1,y1,z1}`. */
  box(a,col){
    const X=[a.x0,a.x1], Y=[a.y0,a.y1], Z=[a.z0,a.z1];
    for(const y of Y) for(const z of Z) this.seg([X[0],y,z],[X[1],y,z],col);
    for(const x of X) for(const z of Z) this.seg([x,Y[0],z],[x,Y[1],z],col);
    for(const x of X) for(const y of Y) this.seg([x,y,Z[0]],[x,y,Z[1]],col);
  }
}

Object.assign(Renderer.prototype,{
  /** Puts overlay `name` up (an `OvlLines`), or takes it down (null / empty).
      `opts.xray` also draws it faintly through what stands in front. */
  setOverlay(name, lines, opts){
    if(!this.overlays) this.overlays=new Map();
    const old=this.overlays.get(name);
    if(old){
      try{ this.gl.deleteBuffer(old.pb); this.gl.deleteBuffer(old.cb); this.gl.deleteVertexArray(old.vao); }catch(_){ }
      this.overlays.delete(name);
    }
    if(lines && lines.size){
      this.overlays.set(name,{pos:new Float32Array(lines.p), col:new Float32Array(lines.c), n:lines.size,
                              xray:!!(opts&&opts.xray), vao:null});
    }
    this.dirty=true;
  },
  hasOverlay(name){ return !!(this.overlays && this.overlays.has(name)); },

  /** Draws every overlay; called by `draw` where the orbit pivot is drawn. */
  drawOverlays(VP){
    if(!this.overlays || !this.overlays.size) return;
    const gl=this.gl;
    if(!this.progWgOvl) this.progWgOvl=this._prog(VS_WGOVL,FS_WGOVL);
    const pr=this.progWgOvl;
    gl.useProgram(pr.p);
    gl.uniformMatrix4fv(pr.u.uVP,false,VP);
    // Put back as found: the renderer caches whether culling is on (`_culling`).
    const cull=gl.isEnabled? gl.isEnabled(gl.CULL_FACE) : true;
    gl.enable(gl.BLEND); gl.blendFunc(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA);
    gl.depthMask(false); gl.disable(gl.CULL_FACE);
    for(const o of this.overlays.values()){
      if(!o.vao){
        o.vao=gl.createVertexArray(); gl.bindVertexArray(o.vao);
        o.pb=gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER,o.pb);
        gl.bufferData(gl.ARRAY_BUFFER,o.pos,gl.STATIC_DRAW);
        gl.enableVertexAttribArray(0); gl.vertexAttribPointer(0,3,gl.FLOAT,false,0,0);
        o.cb=gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER,o.cb);
        gl.bufferData(gl.ARRAY_BUFFER,o.col,gl.STATIC_DRAW);
        gl.enableVertexAttribArray(1); gl.vertexAttribPointer(1,4,gl.FLOAT,false,0,0);
      }
      gl.bindVertexArray(o.vao);
      if(o.xray){
        gl.disable(gl.DEPTH_TEST);
        gl.uniform1f(pr.u.uA,0.3);
        gl.drawArrays(gl.LINES,0,o.n);
        gl.enable(gl.DEPTH_TEST);
      }
      gl.uniform1f(pr.u.uA,1.0);
      gl.drawArrays(gl.LINES,0,o.n);
    }
    gl.bindVertexArray(null);
    gl.depthMask(true); gl.disable(gl.BLEND); gl.enable(gl.DEPTH_TEST);
    if(cull) gl.enable(gl.CULL_FACE);
  },
});
