/* =====================================================================================
   Wraithguard: occlusion culling (Static 2026-11-24: "occlusion culling").

   The view frustum and the view distance were all `_cullGroups` (06_gl.js) left out
   before this: every run of every group in front of the eye was drawn, whatever stood
   between it and the eye. In a town that is most of them. Static's Balmora frame drew
   4.9 million triangles on an Intel UHD at 220 ms of GPU a frame, the objects and their
   reflection 120 ms of it, and from the street the buildings hide most of what is behind
   them.

   How it works, in four steps, none of which waits on the GPU:

   1. **Capture** (`_occCapture`, once the opaque phase of the main view is drawn): the
      frame's depth, resolved, is cut into 8x8-pixel blocks, and each block keeps the
      depth of its *furthest* pixel. A block that shows a wall and the sky past its edge
      keeps the sky's, so it hides nothing; only a block the wall covers whole says
      "nothing past the wall shows here". The result (about 200x120 for a 1600x950
      view) is read back through a pixel buffer and a fence: the copy is asked for now
      and taken a frame or two later, when the GPU has done it, so the page never stalls.
   2. **Read back** (`_occPoll`): the bytes are the depths themselves (the float's own
      bits, packed into the four colour bytes), turned into distances from the eye by the
      projection they were drawn with.
   3. **Reprojected** (`occBuild`): the read-back is a frame or two old and the camera has
      moved since, so each block's far point is put back into the world through the old
      view and projected through the new one. Where several land in one block of the new
      view the furthest wins; where none lands (an edge the camera turned towards, a
      gap the move opened) the block is "unknown", which hides nothing. Then every block
      is widened to the furthest of its 3x3 neighbours, so the edge of an occluder never
      counts for more than it covered. A max pyramid over that answers "the furthest
      thing drawn anywhere in this rectangle" in at most 16 reads.
   4. **Tested** (`hidden`, inside `_cullGroups`): a run's box, through the new view, is
      a screen rectangle and a nearest distance. If the furthest occluder anywhere in the
      rectangle is still nearer than the box's nearest point (by 2% and 8 units), every
      pixel of the run would fail the depth test: it is not drawn.

   What keeps it safe: only what was actually drawn can hide anything (a run left out
   writes no depth, so it never hides another); the far pixel of every block decides,
   not the near one; an unknown block hides nothing; a box that reaches behind the eye
   is always drawn. A read-back is dropped when the groups are rebuilt, when an object is
   moved in the editor (`moveInstance` bumps `_occEdits`), and after 30 frames.
   The reflection has a read-back of its own (2026-11-24: its objects were 63 ms of Static's
   frame), its depth taken after it is drawn and tested from the mirrored eye; the main
   view's says nothing for it. The ground's chunks are tested too. The shadow passes do not
   use it: their light is orthographic, and a sun's eye sees what the camera does not.
   ===================================================================================== */

const OCC_DIV=8;          // the read-back is the view in blocks of this many pixels a side
const OCC_MARGIN=1.02;    // a run is hidden only when 2% further than what covers it...
const OCC_PAD=8;          // ...and 8 units further
const OCC_KEEP=30;        // frames a read-back is used for before it is too old
const OCC_EVERY=8;        // with the camera still, a fresh read-back this often (frames)

/* The block's furthest depth, as the float's own bits in the four colour bytes: exact,
   with no packing arithmetic to lose precision near 1.0 where the far depths are. */
const FS_OCC_DOWN=`#version 300 es
precision highp float; precision highp int; precision highp sampler2D;
uniform sampler2D uDepth;
uniform ivec2 uSize;
out vec4 oColor;
void main(){
  ivec2 b=ivec2(gl_FragCoord.xy)*${OCC_DIV};
  float m=0.0;
  for(int y=0;y<${OCC_DIV};y++) for(int x=0;x<${OCC_DIV};x++){
    ivec2 p=min(b+ivec2(x,y), uSize-1);
    m=max(m, texelFetch(uDepth,p,0).r);
  }
  uint u=floatBitsToUint(m);
  oColor=vec4(float(u&255u), float((u>>8)&255u), float((u>>16)&255u), float(u>>24))/255.0;
}`;
const VS_OCC_DOWN=`#version 300 es
void main(){
  gl_Position=vec4(gl_VertexID==1? 3.0 : -1.0, gl_VertexID==2? 3.0 : -1.0, 0.0, 1.0);
}`;

/** The CPU half, kept apart from the GL so it can be tested on its own. */
const Occ={
  /** A 4x4 column-major matrix's inverse, in doubles; null when it has none. */
  inv(m){
    const a=m, r=new Float64Array(16);
    r[0]=a[5]*a[10]*a[15]-a[5]*a[11]*a[14]-a[9]*a[6]*a[15]+a[9]*a[7]*a[14]+a[13]*a[6]*a[11]-a[13]*a[7]*a[10];
    r[4]=-a[4]*a[10]*a[15]+a[4]*a[11]*a[14]+a[8]*a[6]*a[15]-a[8]*a[7]*a[14]-a[12]*a[6]*a[11]+a[12]*a[7]*a[10];
    r[8]=a[4]*a[9]*a[15]-a[4]*a[11]*a[13]-a[8]*a[5]*a[15]+a[8]*a[7]*a[13]+a[12]*a[5]*a[11]-a[12]*a[7]*a[9];
    r[12]=-a[4]*a[9]*a[14]+a[4]*a[10]*a[13]+a[8]*a[5]*a[14]-a[8]*a[6]*a[13]-a[12]*a[5]*a[10]+a[12]*a[6]*a[9];
    r[1]=-a[1]*a[10]*a[15]+a[1]*a[11]*a[14]+a[9]*a[2]*a[15]-a[9]*a[3]*a[14]-a[13]*a[2]*a[11]+a[13]*a[3]*a[10];
    r[5]=a[0]*a[10]*a[15]-a[0]*a[11]*a[14]-a[8]*a[2]*a[15]+a[8]*a[3]*a[14]+a[12]*a[2]*a[11]-a[12]*a[3]*a[10];
    r[9]=-a[0]*a[9]*a[15]+a[0]*a[11]*a[13]+a[8]*a[1]*a[15]-a[8]*a[3]*a[13]-a[12]*a[1]*a[11]+a[12]*a[3]*a[9];
    r[13]=a[0]*a[9]*a[14]-a[0]*a[10]*a[13]-a[8]*a[1]*a[14]+a[8]*a[2]*a[13]+a[12]*a[1]*a[10]-a[12]*a[2]*a[9];
    r[2]=a[1]*a[6]*a[15]-a[1]*a[7]*a[14]-a[5]*a[2]*a[15]+a[5]*a[3]*a[14]+a[13]*a[2]*a[7]-a[13]*a[3]*a[6];
    r[6]=-a[0]*a[6]*a[15]+a[0]*a[7]*a[14]+a[4]*a[2]*a[15]-a[4]*a[3]*a[14]-a[12]*a[2]*a[7]+a[12]*a[3]*a[6];
    r[10]=a[0]*a[5]*a[15]-a[0]*a[7]*a[13]-a[4]*a[1]*a[15]+a[4]*a[3]*a[13]+a[12]*a[1]*a[7]-a[12]*a[3]*a[5];
    r[14]=-a[0]*a[5]*a[14]+a[0]*a[6]*a[13]+a[4]*a[1]*a[14]-a[4]*a[2]*a[13]-a[12]*a[1]*a[6]+a[12]*a[2]*a[5];
    r[3]=-a[1]*a[6]*a[11]+a[1]*a[7]*a[10]+a[5]*a[2]*a[11]-a[5]*a[3]*a[10]-a[9]*a[2]*a[7]+a[9]*a[3]*a[6];
    r[7]=a[0]*a[6]*a[11]-a[0]*a[7]*a[10]-a[4]*a[2]*a[11]+a[4]*a[3]*a[10]+a[8]*a[2]*a[7]-a[8]*a[3]*a[6];
    r[11]=-a[0]*a[5]*a[11]+a[0]*a[7]*a[9]+a[4]*a[1]*a[11]-a[4]*a[3]*a[9]-a[8]*a[1]*a[7]+a[8]*a[3]*a[5];
    r[15]=a[0]*a[5]*a[10]-a[0]*a[6]*a[9]-a[4]*a[1]*a[10]+a[4]*a[2]*a[9]+a[8]*a[1]*a[6]-a[8]*a[2]*a[5];
    const det=a[0]*r[0]+a[1]*r[4]+a[2]*r[8]+a[3]*r[12];
    if(!det || !isFinite(det)) return null;
    for(let i=0;i<16;i++) r[i]/=det;
    return r;
  },
  /** The read-back's bytes as distances from the eye (clip w), Infinity where the block
   *  reaches the cleared depth (sky, or nothing drawn). `p10`, `p14` are the projection's
   *  two depth terms: ndc z = -p10 + p14/w, so w = p14 / (ndc z + p10). */
  decode(bytes, p10, p14){
    const d=new Float32Array(bytes.buffer, bytes.byteOffset, bytes.byteLength>>2);
    const w=new Float32Array(d.length);
    for(let i=0;i<d.length;i++){
      const z=d[i];
      if(!(z<1)){ w[i]=Infinity; continue; }
      const v=p14/((2*z-1)+p10);
      w[i]=(v>0 && isFinite(v))? v : Infinity;
    }
    return w;
  },
  /** The read-back (`src`: {w, W, H, VP, p10, p14}, a frame or two old) as seen from the
   *  view `VP` now: a max pyramid of distances over the same 8-pixel blocks, and the test
   *  of a box against it. `near` is the near plane; a box reaching nearer is drawn. */
  build(src, VP, near){
    const W=src.W, H=src.H, OW=Math.ceil(W/OCC_DIV), OH=Math.ceil(H/OCC_DIV), n=OW*OH;
    const I=src.inv || (src.inv=Occ.inv(src.VP));
    const cov=new Float32Array(n).fill(-1);
    const V=Float64Array.from(VP);
    if(I){
      const p10=src.p10, p14=src.p14, sw=src.w;
      for(let j=0;j<OH;j++){
        const py0=j*OCC_DIV, py1=Math.min(H, py0+OCC_DIV), ny=((py0+py1)/H)-1;
        for(let i=0;i<OW;i++){
          const w=sw[j*OW+i];
          if(!(w<Infinity)) continue;
          const px0=i*OCC_DIV, px1=Math.min(W, px0+OCC_DIV), nx=((px0+px1)/W)-1;
          // The old view's clip position of the block's far point, back into the world.
          const cx=nx*w, cy=ny*w, cz=p14-p10*w, cw=w;
          let X=I[0]*cx+I[4]*cy+I[8]*cz+I[12]*cw, Y=I[1]*cx+I[5]*cy+I[9]*cz+I[13]*cw,
              Z=I[2]*cx+I[6]*cy+I[10]*cz+I[14]*cw, Q=I[3]*cx+I[7]*cy+I[11]*cz+I[15]*cw;
          if(!Q) continue;
          X/=Q; Y/=Q; Z/=Q;
          // And through the new one.
          const qw=V[3]*X+V[7]*Y+V[11]*Z+V[15];
          if(!(qw>near)) continue;
          const qx=(V[0]*X+V[4]*Y+V[8]*Z+V[12])/qw, qy=(V[1]*X+V[5]*Y+V[9]*Z+V[13])/qw;
          const bi=Math.floor((qx*0.5+0.5)*W/OCC_DIV), bj=Math.floor((qy*0.5+0.5)*H/OCC_DIV);
          if(bi<0 || bj<0 || bi>=OW || bj>=OH) continue;
          const k=bj*OW+bi;
          if(cov[k]<qw) cov[k]=qw;   // the furthest of what landed here (and -1 is "nothing yet")
        }
      }
    }
    // Unknown is "nothing in front", and each block is widened to its 3x3's furthest.
    const L0=new Float32Array(n);
    for(let j=0;j<OH;j++) for(let i=0;i<OW;i++){
      let m=0;
      for(let dj=-1;dj<=1 && m<Infinity;dj++){
        const y=j+dj; if(y<0 || y>=OH) continue;
        for(let di=-1;di<=1;di++){
          const x=i+di; if(x<0 || x>=OW) continue;
          const v=cov[y*OW+x];
          if(v<0){ m=Infinity; break; }
          if(v>m) m=v;
        }
      }
      L0[j*OW+i]=m;
    }
    const levels=[{d:L0, w:OW, h:OH}];
    for(let l=levels[0]; l.w>1 || l.h>1; ){
      const w=Math.ceil(l.w/2), h=Math.ceil(l.h/2), d=new Float32Array(w*h);
      for(let j=0;j<h;j++) for(let i=0;i<w;i++){
        let m=0;
        for(let dj=0;dj<2;dj++){ const y=2*j+dj; if(y>=l.h) continue;
          for(let di=0;di<2;di++){ const x=2*i+di; if(x>=l.w) continue;
            const v=l.d[y*l.w+x]; if(v>m) m=v; } }
        d[j*w+i]=m;
      }
      l={d, w, h}; levels.push(l);
    }
    return {src, VP:Float32Array.from(VP), W, H, levels, tested:0, hid:0,
      /** Whether the box (min x y z, max x y z at `o`) is wholly behind what was drawn. */
      hidden(b,o){
        const T=this.tally||this;   // the renderer's per-frame counts, when it gave some
        T.tested++;
        let minW=Infinity, x0=Infinity, x1=-Infinity, y0=Infinity, y1=-Infinity;
        for(let k=0;k<8;k++){
          const x=(k&1)? b[o+3] : b[o], y=(k&2)? b[o+4] : b[o+1], z=(k&4)? b[o+5] : b[o+2];
          const w=V[3]*x+V[7]*y+V[11]*z+V[15];
          if(!(w>near)) return false;   // reaches the eye: drawn
          const sx=(V[0]*x+V[4]*y+V[8]*z+V[12])/w, sy=(V[1]*x+V[5]*y+V[9]*z+V[13])/w;
          if(w<minW) minW=w;
          if(sx<x0) x0=sx; if(sx>x1) x1=sx; if(sy<y0) y0=sy; if(sy>y1) y1=sy;
        }
        if(x1<-1 || x0>1 || y1<-1 || y0>1) return false;   // off screen: the frustum's to say
        const f=(v,N,M)=>Math.min(M-1, Math.max(0, Math.floor((Math.max(-1,Math.min(1,v))*0.5+0.5)*N/OCC_DIV)));
        let i0=f(x0,W,OW), i1=f(x1,W,OW), j0=f(y0,H,OH), j1=f(y1,H,OH), l=0;
        while((i1-i0>3 || j1-j0>3) && l<levels.length-1){ i0>>=1; i1>>=1; j0>>=1; j1>>=1; l++; }
        const L=levels[l];
        let m=0;
        for(let j=j0;j<=j1;j++) for(let i=i0;i<=i1;i++){
          const v=L.d[j*L.w+i];
          if(v>m){ m=v; if(m===Infinity) return false; }
        }
        if(m*OCC_MARGIN+OCC_PAD<minW){ T.hid++; return true; }
        return false;
      }};
  }
};

Object.assign(Renderer.prototype,{
  /** One view's read-back state: `main` (the scene) or `refl` (the water's reflection, from
   *  the eye's mirror image; 2026-11-24, its 63 ms on Static's frame). */
  _occView(key){
    const all=this._occV||(this._occV={});
    return all[key]||(all[key]={pend:[], src:null, last:null, cov:null, D:null});
  },
  /** The view this frame tests its runs against, or null when there is nothing current
   *  to test with (off, nothing read back yet, the scene changed since, too old). */
  _occFor(VP, key){
    key=key||'main';
    if(this.occlusion===false){ this._occV=null; return null; }
    const S=this._occView(key);
    this._occPoll(S);
    const s=S.src;
    if(!s || s.G!==this._groups || s.edits!==(this._occEdits|0) || (this._occN|0)-s.f>OCC_KEEP) return null;
    /* The Report's counts are a whole frame's, over every view built in it: kept in one
       tally, handed over and restarted when the frame count moves (it moves at capture). */
    const T=this._occTally||(this._occTally={frame:-1, tested:0, hid:0, lag:0});
    if(T.frame!==(this._occN|0)){
      if(T.frame>=0 && T.tested) this._occStat={tested:T.tested, hidden:T.hid, lag:T.lag};
      T.frame=this._occN|0; T.tested=0; T.hid=0;
    }
    if(key==='main') T.lag=(this._occN|0)-s.f;
    let c=S.cov;
    if(!(c && c.src===s && Occ_same(c.VP,VP))){ c=S.cov=Occ.build(s, VP, this.nearFar()[0]); c.tally=T; }
    return c;
  },
  /** After the main view's opaque phase: its depth, cut to blocks, asked to be read back. */
  _occCapture(T,VP,P){
    // The scene is multisampled: its depth is resolved into T.depTex first.
    this._occN=(this._occN|0)+1;
    this._occTake('main', {W:T.W, H:T.H, depTex:T.depTex, back:T.sceneFB,
      resolve:()=>{ const gl=this.gl; gl.bindFramebuffer(gl.READ_FRAMEBUFFER,T.sceneFB);
        gl.bindFramebuffer(gl.DRAW_FRAMEBUFFER,T.resolveFB);
        gl.blitFramebuffer(0,0,T.W,T.H,0,0,T.W,T.H,gl.DEPTH_BUFFER_BIT,gl.NEAREST); }}, VP, P);
  },
  /** After the reflection is drawn: its depth (a texture of its own) the same way. */
  _occCaptureRefl(T,VP,P){
    if(!T.reflDepTex) return;
    const gl=this.gl;
    this._occTake('refl', {W:T.RW, H:T.RH, depTex:T.reflDepTex, back:T.reflFB}, VP, P);
    gl.viewport(0,0,T.RW,T.RH);
  },
  _occTake(key, view, VP, P){
    if(this.occlusion===false) return;
    const gl=this.gl;
    if(typeof gl.fenceSync!=='function' || !P || P[11]!==-1) return;   // a perspective view only: its w is the distance
    const S=this._occView(key);
    this._occPoll(S);
    const pend=S.pend;
    if(pend.length>=2) return;   // the GPU is behind: no more copies queued
    const G=this._groups, edits=this._occEdits|0, last=S.last;
    if(last && last.G===G && last.edits===edits && Occ_same(last.VP,VP) && this._occN-last.f<OCC_EVERY) return;
    const W=view.W, H=view.H, OW=Math.ceil(W/OCC_DIV), OH=Math.ceil(H/OCC_DIV);
    let D=S.D;
    if(!D || D.OW!==OW || D.OH!==OH){
      if(D){ try{ gl.deleteFramebuffer(D.fb); gl.deleteTexture(D.tex); for(const b of D.free) gl.deleteBuffer(b); }catch(_){ } }
      for(const r of pend){ try{ gl.deleteSync(r.sync); gl.deleteBuffer(r.pbo); }catch(_){ } }
      pend.length=0;
      D=S.D={OW, OH, free:[]};
      D.tex=gl.createTexture(); gl.bindTexture(gl.TEXTURE_2D,D.tex);
      gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,OW,OH,0,gl.RGBA,gl.UNSIGNED_BYTE,null);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.NEAREST);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.NEAREST);
      D.fb=gl.createFramebuffer(); gl.bindFramebuffer(gl.FRAMEBUFFER,D.fb);
      gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,D.tex,0);
      D.ok=gl.checkFramebufferStatus(gl.FRAMEBUFFER)===gl.FRAMEBUFFER_COMPLETE;
      for(let i=0;i<3;i++){ const b=gl.createBuffer(); gl.bindBuffer(gl.PIXEL_PACK_BUFFER,b);
        gl.bufferData(gl.PIXEL_PACK_BUFFER,OW*OH*4,gl.STREAM_READ); D.free.push(b); }
      gl.bindBuffer(gl.PIXEL_PACK_BUFFER,null);
    }
    if(!D.ok || !D.free.length){ gl.bindFramebuffer(gl.FRAMEBUFFER,view.back); return; }
    if(!this.progOccDown) this.progOccDown=this._prog(VS_OCC_DOWN,FS_OCC_DOWN);
    if(view.resolve) view.resolve();
    // Cut to blocks.
    gl.bindFramebuffer(gl.FRAMEBUFFER,D.fb);
    gl.viewport(0,0,OW,OH);
    gl.disable(gl.DEPTH_TEST); gl.disable(gl.BLEND); gl.disable(gl.CULL_FACE); gl.depthMask(false);
    const pr=this.progOccDown; gl.useProgram(pr.p);
    gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D,view.depTex);
    gl.uniform1i(pr.u.uDepth,0);
    gl.uniform2i(pr.u.uSize,W,H);
    gl.bindVertexArray(this._occVao||(this._occVao=gl.createVertexArray()));
    gl.drawArrays(gl.TRIANGLES,0,3);
    gl.bindVertexArray(null);
    // Asked for, not waited on.
    const pbo=D.free.pop();
    gl.bindFramebuffer(gl.READ_FRAMEBUFFER,D.fb);
    gl.bindBuffer(gl.PIXEL_PACK_BUFFER,pbo);
    gl.readPixels(0,0,OW,OH,gl.RGBA,gl.UNSIGNED_BYTE,0);
    gl.bindBuffer(gl.PIXEL_PACK_BUFFER,null);
    const sync=gl.fenceSync(gl.SYNC_GPU_COMMANDS_COMPLETE,0);
    pend.push({sync, pbo, W, H, OW, OH, VP:Float64Array.from(VP), p10:P[10], p14:P[14], G, edits, f:this._occN|0});
    S.last={VP:Float32Array.from(VP), G, edits, f:this._occN|0};
    // Handed back as found: the view's framebuffer, its viewport, the depth test on.
    gl.bindFramebuffer(gl.FRAMEBUFFER,view.back);
    gl.viewport(0,0,W,H);
    gl.enable(gl.DEPTH_TEST); gl.depthMask(true);
  },
  /** Whichever of a view's read-backs the GPU has finished, the newest kept. */
  _occPoll(S){
    const pend=S && S.pend; if(!pend || !pend.length) return;
    const gl=this.gl, D=S.D;
    while(pend.length){
      const r=pend[0];
      if(gl.getSyncParameter(r.sync,gl.SYNC_STATUS)!==gl.SIGNALED) break;
      pend.shift(); gl.deleteSync(r.sync);
      const bytes=new Uint8Array(r.OW*r.OH*4);
      gl.bindBuffer(gl.PIXEL_PACK_BUFFER,r.pbo);
      gl.getBufferSubData(gl.PIXEL_PACK_BUFFER,0,bytes);
      gl.bindBuffer(gl.PIXEL_PACK_BUFFER,null);
      if(D && D.OW===r.OW && D.OH===r.OH) D.free.push(r.pbo); else gl.deleteBuffer(r.pbo);
      S.src={w:Occ.decode(bytes,r.p10,r.p14), W:r.W, H:r.H, VP:r.VP, p10:r.p10, p14:r.p14,
             G:r.G, edits:r.edits, f:r.f, inv:null};
    }
  }
});
/** Two view matrices equal, entry for entry. */
function Occ_same(a,b){
  if(a===b) return true;
  if(!a || !b || a.length!==b.length) return false;
  for(let i=0;i<a.length;i++) if(a[i]!==b[i]) return false;
  return true;
}
