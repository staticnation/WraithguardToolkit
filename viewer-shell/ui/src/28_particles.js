/* =====================================================================================
   The particle systems — round 17w.

   Robin: "Lastly, I still want the particle systems to run." Asked for in round 17n and
   put off since: a waterfall's mist, a torch's flame and its smoke, a candle's flicker,
   the sparks over a brazier — every one of them a NiParticleSystemController on a
   NiAutoNormalParticles or NiRotatingParticles node, which the draw walk steps over
   because a still preview has nothing to do with them. This is the part of the preview
   that is not still.

   The reader (`nif::parse_particles`) survived the weather's removal in 17h for exactly
   this, and the mesh payload carries its answer after the parts now: for each system, the
   emitter's transform in the mesh's space, the controller's numbers (speed and its spread,
   the cone, the rate, the lifetime, how many at most), the modifiers (gravities, grow and
   fade, colour keys, rotation) and the sheet it is drawn with, blend and all.

   **The model is OpenMW's**, since that is the reference Robin gave for lighting and it
   is the one engine whose reading of these records is written down:
   - the shooter: a direction `(sin v cos h, sin v sin h, cos v)` with `v` and `h` each
     the controller's direction plus its angle times a roll in -1..1, a speed of
     `velocity ± velocityRandom/2`, a life of `lifetime + lifetimeRandom·roll`;
   - the rate: the file's `emitRate` when its flags say "no auto adjust", otherwise
     `numParticles / (lifetime + lifetimeRandom/2)` — enough to keep the pool full;
   - the emission window: the controller runs a cycle of `start..stop` at `frequency`, and
     emits while the cycle time is inside `emitStart..emitStop`;
   - gravity: `v += dir · force · dt · 1.6` for a wind, and towards the point for a point
     gravity — OpenMW's own magic 1.6;
   - grow and fade scale the size up over the first `grow` seconds and down over the last
     `fade`; the colour keys run over the particle's life as a fraction 0..1;
   - `ABSOLUTE_RF`: a particle already shot stays where it is in the world; only the
     emitter belongs to the node. Placed objects do not move, so the two coincide here.

   **What is drawn** is a camera-facing quad per particle, `size` units from centre to
   edge (the record's size is a radius), textured with the system's sheet, coloured by the
   controller's colour and the keys, blended as its NiAlphaProperty says — a flame is
   additive (src alpha, one), smoke and mist are the usual alpha blend. Lit by the ambient
   alone plus the material's emissive (OpenMW's way, and the game's by Robin's screenshots
   - see FS_PART), so the mist over a waterfall takes the hour's light and a flame does
   not need to. Depth-tested and not depth-written; drawn in the real pass only — the
   reflection has no grass either. Round 18f: where there is water, twice - what is under
   the water line before the surface, into the picture the surface tints or refracts, and
   what is over it after (`drawParticles`, `side`). Robin, of the waterfall mist: "They
   seem to be spraying in a diagonal away from the waterfall which looks off" - the puffs
   that shot sideways and sank were drawn over the surface at full strength, since the
   surface writes no depth and the particles came after it; the game's surface hides them.
   What the file says about that mist (ex_waterfall_mist_01), for the record: a full
   upward hemisphere (vertical 0 ± pi/2, horizontal 0 ± pi/2) at 300 ± 22 units a second
   under a 315 downward gravity, sixty-unit puffs, two thirds of a second each - and the
   exporter's own snapshot in the file, the 95 live particles' positions, spans ±180 across
   and -73..+97 up, which is exactly what this model draws. The game's mist is flatter and
   tighter than that (Robin's screenshots: about 200 across, some 40 over the water), so
   the game's engine does not run these numbers the way the exporter did; by how much, and
   in which of speed or gravity, is not knowable from here - see the round 18f note.

   **Every placed instance of a mesh gets its own live systems** (`addParticleSystems`),
   and a lamp's keep the lamp's hours: a candle at noon under the Night time rule does
   not burn, and its flame goes with its light and its glow, at the same staggered minute
   (round 17w, with the glow). Every loaded run is stepped and drawn whatever its
   distance (round 17y - there was a three-cell cut, and the Ghostgate pylons' beams went
   out as the camera drew back); `PART_MAX_DRAW` is the one fuse.

   One engine rule as ever: nothing here touches the plugin.
   ===================================================================================== */

const PART_MAX_DRAW=60000;  // quads a frame, at most: a fuse, not a target
const PART_DT_MAX=0.1;      // a hitch is not a fast-forward

const VS_PART=`#version 300 es
precision highp float;
layout(location=0) in vec2 aQ;          // the quad's corner, -1..1
layout(location=1) in vec4 iPS;         // xyz position, w half-size
layout(location=2) in vec4 iCol;        // the particle's colour and alpha
layout(location=3) in float iRot;       // its turn, radians
uniform mat4 uVP;
uniform vec3 uRight, uUp;               // the camera's, so the quad faces it
out vec2 vUV; out vec4 vCol; out vec3 vW;
void main(){
  float c=cos(iRot), s=sin(iRot);
  vec2 q=vec2(aQ.x*c-aQ.y*s, aQ.x*s+aQ.y*c)*iPS.w;
  vec3 w=iPS.xyz + uRight*q.x + uUp*q.y;
  vW=w; vCol=iCol; vUV=aQ*0.5+0.5;
  gl_Position=uVP*vec4(w,1.0);
}`;

const FS_PART=`#version 300 es
precision highp float;
in vec2 vUV; in vec4 vCol; in vec3 vW;
uniform sampler2D uTex; uniform int uHasTex;
uniform float uMatAlpha;
uniform vec3 uEmissive;
uniform vec3 uSun; uniform float uBright;
uniform int uLit;
uniform vec3 uFogCol; uniform float uFogK;   // the flat fog, when there is no sky
/* Round 18f: which side of the water this pass draws. -1 keeps what is under the water
   line and is drawn before the surface, so the surface tints or refracts it; +1 keeps
   what is over it and is drawn after; 0 draws everything, where there is no water. */
uniform float uWaterZ; uniform int uSide;
${SKY_GLSL}
out vec4 o;
void main(){
  if(uSide<0 && vW.z>=uWaterZ) discard;
  if(uSide>0 && vW.z<uWaterZ) discard;
  vec4 t = uHasTex==1 ? texture(uTex, vUV) : vec4(1.0);
  vec3 c = t.rgb*vCol.rgb;
  /* Lit the way OpenMW lights a particle (nifloader, handleParticleSystem: "Particles
     don't have normals, so can't be diffuse lit" - the material's diffuse is set black
     and its colour mode to AMBIENT): the particle's colour times the ambient light, plus
     the material's emissive. A flame's emissive is white, so it burns at full strength at
     midnight and at noon alike; the smoke beside it has none and takes the ambient.

     Round 17x had this; 18a lit them as sheets facing the eye, sun and lamps included,
     after "the waterfall mist is rendering too dark"; 18d puts it back at Robin's word,
     with four screenshots of the Vivec canalworks fall at 06:00, noon, 18:00 and midnight:
     "You are correct that the particles for waterfall splashes are unlit. Let's change
     back to that." Measured off those: the splash at the fall's foot is ~0.4 of its
     texture at noon, ~0.33 and ~0.17 at the two dusks, ~0.11 at midnight - the ambient
     and nothing else, the sun playing no part. */
  if(uLit==1){
    vec3 amb = (uScat==1 || uRoom==1) ? uAmbCol : vec3(0.55);
    if(uNoLight==1) amb = vec3(1.0);   // the Unlit switch (round 18f)
    c *= min(amb + uEmissive, vec3(1.0));
  }
  c *= uBright;
  c = airFog(c, vW);
  o = vec4(c, t.a*vCol.a*uMatAlpha);
}`;

/* GL blend factors by the NiAlphaProperty's four-bit codes. */
const PART_BLEND=['ONE','ZERO','SRC_COLOR','ONE_MINUS_SRC_COLOR','DST_COLOR','ONE_MINUS_DST_COLOR',
                  'SRC_ALPHA','ONE_MINUS_SRC_ALPHA','DST_ALPHA','ONE_MINUS_DST_ALPHA','SRC_ALPHA_SATURATE'];

Object.assign(Renderer.prototype,{
  /** Every live system of every placed instance: `{tpl, runs}` per system template, where
   *  a template is one system of one mesh (the sheet, the blend, the numbers) and a run is
   *  one placed instance of it (the emitter in the world, its pool of particles). */
  _partInit(){
    if(this._parts) return;
    const gl=this.gl;
    this._parts={groups:new Map(), last:null, drawn:0, live:0, runs:0};
    this.progPart=this._prog(VS_PART,FS_PART);
    // One quad, shared: two triangles over -1..1.
    const vao=gl.createVertexArray(); gl.bindVertexArray(vao);
    this._buf(new Float32Array([-1,-1, 1,-1, 1,1, -1,1]));
    gl.enableVertexAttribArray(0); gl.vertexAttribPointer(0,2,gl.FLOAT,false,0,0);
    const ib=gl.createBuffer(); gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER,ib);
    gl.bufferData(gl.ELEMENT_ARRAY_BUFFER,new Uint16Array([0,1,2, 0,2,3]),gl.STATIC_DRAW);
    // The instance stream: 9 floats a particle — position, half-size, colour, turn.
    const inst=gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER,inst);
    gl.enableVertexAttribArray(1); gl.vertexAttribPointer(1,4,gl.FLOAT,false,36,0);  gl.vertexAttribDivisor(1,1);
    gl.enableVertexAttribArray(2); gl.vertexAttribPointer(2,4,gl.FLOAT,false,36,16); gl.vertexAttribDivisor(2,1);
    gl.enableVertexAttribArray(3); gl.vertexAttribPointer(3,1,gl.FLOAT,false,36,32); gl.vertexAttribDivisor(3,1);
    gl.bindVertexArray(null);
    this._parts.vao=vao; this._parts.inst=inst;
    this._parts.buf=new Float32Array(9*4096);
  },

  /** One placed instance's systems. `systems` is the mesh payload's list; `m` the
   *  instance's 3x4 matrix (rows), `lamp` the instance's `[on, off]` hours or null. */
  addParticleSystems(systems, m, lamp, meshKey){
    this._partInit();
    const P=this._parts;
    // Instance rotation-scale (3x3) and translation, from the rows of the 3x4.
    const R3=[m[0],m[1],m[2], m[4],m[5],m[6], m[8],m[9],m[10]];
    const T=[m[3],m[7],m[11]];
    for(let si=0; si<systems.length; si++){
      const s=systems[si];
      const cap=Math.max(1, Math.min(2048, s.numParticles|0 || 16));
      const key=(meshKey||'')+'#'+si;
      let g=P.groups.get(key);
      if(!g){
        g={tpl:this._partTemplate(s), runs:[]};
        P.groups.set(key,g);
      }
      /* The emitter's transform in the world: the instance's rotation-scale times the
         emitter node's in the mesh (row-major 3x3 in the first nine, translation in the
         last three, the way `parse_particles` writes them). */
      const e=s.emitter||[1,0,0,0,1,0,0,0,1,0,0,0], es=s.emitterScale||1;
      const E3=[e[0]*es,e[1]*es,e[2]*es, e[3]*es,e[4]*es,e[5]*es, e[6]*es,e[7]*es,e[8]*es];
      const ET=[e[9],e[10],e[11]];
      const rot=mul3(R3,E3);
      const pos=[R3[0]*ET[0]+R3[1]*ET[1]+R3[2]*ET[2]+T[0],
                 R3[3]*ET[0]+R3[4]*ET[1]+R3[5]*ET[2]+T[1],
                 R3[6]*ET[0]+R3[7]*ET[1]+R3[8]*ET[2]+T[2]];
      /* Round 18g: and the *particle node's* transform, which is the frame the modifiers
         speak in - a gravity's direction and a point gravity's position are in the
         particle system's own space, as OpenMW's GravityAffector reads them, not the
         emitter's. The two coincide in the vanilla mist and differ in Better Waterfalls'
         (OAAB): there the particle node is turned so its local -X is world down and the
         emitter is turned another way, and a gravity taken through the emitter's frame
         pulled the puffs sideways - Robin: "the spray from them goes up to the right
         diagonally". `nodeScale` scales the point gravity's position with the rest. */
      const nd=s.node||[1,0,0,0,1,0,0,0,1,0,0,0], ns=(+s.nodeScale>1e-6)? +s.nodeScale : 1;
      const nrot=mul3(R3,[nd[0],nd[1],nd[2], nd[3],nd[4],nd[5], nd[6],nd[7],nd[8]]);
      const npos=[R3[0]*nd[9]+R3[1]*nd[10]+R3[2]*nd[11]+T[0],
                  R3[3]*nd[9]+R3[4]*nd[10]+R3[5]*nd[11]+T[1],
                  R3[6]*nd[9]+R3[7]*nd[10]+R3[8]*nd[11]+T[2]];
      // A uniform scale on the instance scales speeds and sizes too, as it does the mesh.
      const sc=Math.cbrt(Math.abs(det3(R3)))||1;
      g.runs.push({pos, rot, nrot, npos, ns, sc, lamp:lamp||null, cap, gdir:[], gpos:[],
                   p:new Float32Array(cap*12), n:0, acc:0,
                   // Each run starts its cycle at its own point, so a row of torches
                   // does not flicker in step.
                   phase:Math.random()*Math.max(0.001,(s.stop-s.start)||1),
                   warm:true});
      P.runs++;
    }
    this.dirty=true;
  },

  /** What is constant about a system: the numbers the runs step by, and how it is drawn. */
  _partTemplate(s){
    const flags=s.alphaFlags|0;
    const blend = (flags&1)? {src:PART_BLEND[(flags>>1)&15]||'SRC_ALPHA', dst:PART_BLEND[(flags>>5)&15]||'ONE_MINUS_SRC_ALPHA'}
                           : {src:'SRC_ALPHA', dst:'ONE_MINUS_SRC_ALPHA'};
    const life=+s.lifetime||0, lifeR=+s.lifetimeRandom||0;
    const auto=((s.emitFlags|0)&1)===0;
    const rate = auto ? ((life+lifeR/2)>0 ? (s.numParticles|0)/(life+lifeR/2) : 0) : (+s.emitRate||0);
    const keys=(s.colorKeys||[]).slice().sort((a,b)=>a[0]-b[0]);
    /* Round 17y: the particle node's own scale in the mesh. Everything the controller
       says - the size, the speed, the spread, the pull of a gravity - is in that node's
       units, and the game draws the node scaled like any other. Ashfall's kettles keep a
       size-100 steam under a node at 0.2, which is a 20-unit puff in the game and was a
       100-unit cloud here (Robin: "render far too big [...] Maybe it's a property not
       read correctly"). The emitter node's scale is left out of it on purpose: the same
       kettle's emitter is scaled 0, and a speed scaled by nought is no steam at all. */
    const k=(+s.nodeScale>1e-6)? +s.nodeScale : 1;
    return {
      tex:(s.glTex&&s.glTex.gl)||null, blend, diffuse:s.diffuse||[1,1,1], matAlpha:s.matAlpha==null?1:+s.matAlpha,
      emissive:s.emissive||[0,0,0],
      vel:(+s.velocity||0)*k, velR:(+s.velocityRandom||0)*k,
      vDir:+s.verticalDir||0, vAng:+s.verticalAngle||0, hDir:+s.horizontalDir||0, hAng:+s.horizontalAngle||0,
      col:s.color||[1,1,1,1], size:Math.max(0.01,(+s.size||1)*k),
      emitStart:+s.emitStart||0, emitStop:+s.emitStop||0, rate, life, lifeR,
      off:(s.offsetRandom||[0,0,0]).map(v=>(+v||0)*k),
      freq:+s.frequency||1, start:+s.start||0, stop:+s.stop||0,
      grav:(s.gravity||[]).map(g=>({d:g.direction||[0,0,1], p:g.position||[0,0,0], f:(+g.force||0)*k, kind:g.kind|0})),
      grow:+s.grow||0, fade:+s.fade||0, keys,
      spin:(s.rotation!=null)? +s.rotation : 0,
      rotating:!!(s.data&&s.data.rotating) || s.rotation!=null,
      lit:true,
    };
  },

  /** Forgets every run. Called with the objects, since the runs are theirs. */
  clearParticles(){
    if(!this._parts) return;
    for(const g of this._parts.groups.values()) g.runs.length=0;
    this._parts.groups.clear();
    this._parts.runs=0; this._parts.live=0;
  },

  /** Steps every run near the eye by `dt` and draws what is alive. */
  drawParticles(VP, eye, airOn, sun, fogK, side){
    const P=this._parts;
    if(!P || !P.groups.size) return;
    const gl=this.gl, o=this.opts;
    /* Round 18f: the water line. Robin, of the waterfall mist: "They seem to be spraying
       in a diagonal away from the waterfall which looks off." Half of that spray was
       under the water - the puffs that shot sideways and sank - drawn over the surface at
       full strength, because the surface writes no depth and the particles came after
       it. The game's surface hides what is beneath it. So, where there is water, the
       particles are drawn twice: what is under the line before the surface (`side` -1),
       into the picture the surface tints or refracts, and what is over it after (+1).
       The simulation steps once, on the first of the two; the second reuses what the
       first packed. */
    side = side|0;
    const again = side>0 && P.packedFor!=null;
    const now=performance.now()/1000;
    const dt = again? 0 : (P.last==null? 0 : Math.min(PART_DT_MAX, Math.max(0, now-P.last)));
    if(!again) P.last=now;
    const h=o.hour;
    // The camera's right and up, for the quads.
    const c=this.cam, ce=Math.cos(c.el), se=Math.sin(c.el), ca=Math.cos(c.az), sa=Math.sin(c.az);
    const right=[-sa, ca, 0], up=[-se*ca, -se*sa, ce];

    const pr=this.progPart; gl.useProgram(pr.p);
    gl.uniformMatrix4fv(pr.u.uVP,false,VP);
    gl.uniform3fv(pr.u.uRight,right); gl.uniform3fv(pr.u.uUp,up);
    gl.uniform3fv(pr.u.uSun,sun);
    gl.uniform1f(pr.u.uBright, o.bright==null?1.0:o.bright);
    gl.uniform1i(pr.u.uTex,0);
    gl.uniform3fv(pr.u.uFogCol,this.backdrop());   // 18dd (airOn sets it again for the pass)
    if(fogK!=null) gl.uniform1f(pr.u.uFogK,fogK);
    airOn(pr);
    gl.uniform1i(pr.u.uSide, side);
    gl.uniform1f(pr.u.uWaterZ, this.water? this.water.z : 0);
    gl.enable(gl.DEPTH_TEST); gl.depthMask(false);
    gl.enable(gl.BLEND);
    gl.disable(gl.CULL_FACE);
    gl.bindVertexArray(P.vao);
    let drawn=0, live=0, any=false;
    if(!again) P.packedFor=null;
    for(const g of P.groups.values()){
      const t=g.tpl;
      let count=0, at=0, B;
      if(again){
        if(!g.packed || !g.packed.n){ continue; }
        B=g.packed.data; at=g.packed.n*9; count=g.packed.live;
      } else {
      for(const r of g.runs){
        /* Round 17y: no distance cut. There was one at three cells - "a flame three cells
           away is a pixel" - and the Ghostgate pylons' beams went out as you drew back.
           Robin: "I want all particle systems to run no matter the distance if they are
           loaded." What is loaded runs; PART_MAX_DRAW is the only fuse left. */
        const lit = !r.lamp || this.lampLitAt(r.lamp[0], r.lamp[1], h);
        count+=this._partStep(t, r, dt, now, lit);
      }
      g.packed=null;
      if(!count) continue;
      // Pack the group's live particles, then one instanced draw. Kept on the group for
      // the pass over the water, which draws the same particles again (round 18f).
      let need=count*9;
      if(!g.pack || g.pack.length<need) g.pack=new Float32Array(Math.max(need, g.pack? g.pack.length*2 : 0));
      B=g.pack;
      for(const r of g.runs){
        const p=r.p, n=r.n;
        for(let i=0;i<n;i++){
          if(drawn+at/9>=PART_MAX_DRAW) break;
          const k=i*12, age=p[k+6], life=p[k+7];
          let sz=p[k+8]*r.sc;
          if(t.grow>0 && age<t.grow) sz*=age/t.grow;
          if(t.fade>0 && life-age<t.fade) sz*=Math.max(0,(life-age)/t.fade);
          B[at]=p[k]; B[at+1]=p[k+1]; B[at+2]=p[k+2]; B[at+3]=sz;
          const f=life>0? age/life : 1;
          if(t.keys.length){ const col=keyColour(t.keys,f);
            B[at+4]=t.col[0]*col[0]; B[at+5]=t.col[1]*col[1]; B[at+6]=t.col[2]*col[2]; B[at+7]=t.col[3]*col[3]; }
          else { B[at+4]=t.col[0]; B[at+5]=t.col[1]; B[at+6]=t.col[2]; B[at+7]=t.col[3]; }
          B[at+8]=p[k+9];
          at+=9;
        }
      }
      g.packed={data:B, n:at/9|0, live:count};
      }
      any=true;
      const n=at/9|0;
      if(!n) continue;
      gl.bindBuffer(gl.ARRAY_BUFFER,P.inst);
      gl.bufferData(gl.ARRAY_BUFFER,B.subarray(0,at),gl.DYNAMIC_DRAW);
      gl.blendFunc(gl[t.blend.src]||gl.SRC_ALPHA, gl[t.blend.dst]||gl.ONE_MINUS_SRC_ALPHA);
      // Round 18h: a flame adds onto the picture, so the fog takes it away rather than
      // laying its haze on it (SKY_GLSL, uFogBlack) - no more white specks far off.
      if(pr.u.uFogBlack) gl.uniform1i(pr.u.uFogBlack, t.blend.dst==='ONE'? 1 : 0);
      gl.activeTexture(gl.TEXTURE0);
      gl.bindTexture(gl.TEXTURE_2D, t.tex||this.white);
      gl.uniform1i(pr.u.uHasTex, t.tex?1:0);
      gl.uniform3fv(pr.u.uEmissive, t.emissive);
      gl.uniform1f(pr.u.uMatAlpha, t.matAlpha);
      gl.uniform1i(pr.u.uLit, t.lit?1:0);
      gl.drawElementsInstanced(gl.TRIANGLES,6,gl.UNSIGNED_SHORT,0,n);
      drawn+=n; live+=count;
    }
    P.drawn=drawn; P.live=live;
    if(!again) P.packedFor = side<0? now : null;   // the pass over the water may follow
    gl.bindVertexArray(null);
    gl.blendFunc(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA);
    gl.disable(gl.BLEND);
    gl.depthMask(true);
    // Something is moving, so the next frame has to be drawn: this is what animates.
    if(any) this.dirty=true;
  },

  /** Steps one run: ages and moves what is alive, drops the dead, emits the new.
   *  Returns how many are alive. The pool is 12 floats a particle:
   *  x y z, vx vy vz, age life, size, rot, spin, (spare). */
  _partStep(t, r, dt, now, lit){
    const p=r.p, cap=r.cap;
    let n=r.n;
    /* A run coming into range starts full, as if it had been running: a torch that lit
       from nothing as you walked up would be a torch you saw lighting. Stepped a whole
       life in small strides, once. */
    if(r.warm){
      r.warm=false; r.n=0; n=0; r.acc=0;
      if(lit && t.life>0){
        const steps=24, sdt=(t.life+t.lifeR)/steps;
        for(let i=0;i<steps;i++) n=this._partStep(t, r, sdt, now-(steps-i)*sdt, lit);
        return n;
      }
    }
    // Age and move.
    for(let i=0;i<n;){
      const k=i*12;
      p[k+6]+=dt;
      if(p[k+6]>=p[k+7]){
        // Dead: the last one takes its place.
        n--; if(i<n){ const l=n*12; for(let j=0;j<12;j++) p[k+j]=p[l+j]; }
        continue;
      }
      for(let gi=0; gi<t.grav.length; gi++){
        const g=t.grav[gi];
        const f=g.f*dt*1.6;
        /* Round 18g: the direction and the point in the world, through the particle
           node's frame (see addParticleSystems), and cached on the *run* - every placing
           of a mesh has its own rotation, and a cache on the shared template gave them
           all the first one's. */
        const NR=r.nrot||r.rot, NP=r.npos||r.pos, NS=r.ns||1;
        if(g.kind===0){
          const d=r.gdir[gi]||(r.gdir[gi]=rot3(NR,g.d,true));
          p[k+3]+=d[0]*f; p[k+4]+=d[1]*f; p[k+5]+=d[2]*f;
        }else{
          const gp=r.gpos[gi]||(r.gpos[gi]=[NR[0]*g.p[0]*NS+NR[1]*g.p[1]*NS+NR[2]*g.p[2]*NS+NP[0],
                                           NR[3]*g.p[0]*NS+NR[4]*g.p[1]*NS+NR[5]*g.p[2]*NS+NP[1],
                                           NR[6]*g.p[0]*NS+NR[7]*g.p[1]*NS+NR[8]*g.p[2]*NS+NP[2]]);
          let ax=gp[0]-p[k], ay=gp[1]-p[k+1], az=gp[2]-p[k+2];
          const L=Math.hypot(ax,ay,az)||1; ax/=L; ay/=L; az/=L;
          p[k+3]+=ax*f; p[k+4]+=ay*f; p[k+5]+=az*f;
        }
      }
      p[k]+=p[k+3]*dt; p[k+1]+=p[k+4]*dt; p[k+2]+=p[k+5]*dt;
      p[k+9]+=p[k+10]*dt;
      i++;
    }
    // Emit, while the controller's cycle is inside the emission window and the lamp is lit.
    let emitting=lit && t.rate>0;
    if(emitting){
      const span=t.stop-t.start;
      let ct=t.freq*(now+r.phase);
      if(span>1e-6){ const c=(ct-t.start)/span; ct=t.start+(c-Math.floor(c))*span; }
      emitting = ct>=t.emitStart && ct<=t.emitStop;
    }
    if(emitting){
      r.acc+=t.rate*dt;
      while(r.acc>=1 && n<cap){
        r.acc-=1;
        const k=n*12;
        const v=t.vDir+t.vAng*(2*Math.random()-1);
        const hh=t.hDir+t.hAng*(2*Math.random()-1);
        const sv=Math.sin(v);
        const dl=[sv*Math.cos(hh), sv*Math.sin(hh), Math.cos(v)];
        const d=rot3(r.rot,dl,true);
        const speed=(t.vel+(Math.random()-0.5)*t.velR)*r.sc;
        const ox=(Math.random()*2-1)*t.off[0], oy=(Math.random()*2-1)*t.off[1], oz=(Math.random()*2-1)*t.off[2];
        const ow=rot3(r.rot,[ox,oy,oz],false);
        p[k]=r.pos[0]+ow[0]; p[k+1]=r.pos[1]+ow[1]; p[k+2]=r.pos[2]+ow[2];
        p[k+3]=d[0]*speed; p[k+4]=d[1]*speed; p[k+5]=d[2]*speed;
        p[k+6]=0; p[k+7]=Math.max(0.05, t.life+t.lifeR*Math.random());
        p[k+8]=t.size;
        p[k+9]=t.rotating? Math.random()*Math.PI*2 : 0;
        p[k+10]=t.spin? (Math.random()<0.5?-1:1)*t.spin : 0;
        n++;
      }
      if(n>=cap) r.acc=0;
    } else r.acc=0;
    r.n=n;
    return n;
  },
});

/** 3x3 row-major product. */
function mul3(a,b){
  return [a[0]*b[0]+a[1]*b[3]+a[2]*b[6], a[0]*b[1]+a[1]*b[4]+a[2]*b[7], a[0]*b[2]+a[1]*b[5]+a[2]*b[8],
          a[3]*b[0]+a[4]*b[3]+a[5]*b[6], a[3]*b[1]+a[4]*b[4]+a[5]*b[7], a[3]*b[2]+a[4]*b[5]+a[5]*b[8],
          a[6]*b[0]+a[7]*b[3]+a[8]*b[6], a[6]*b[1]+a[7]*b[4]+a[8]*b[7], a[6]*b[2]+a[7]*b[5]+a[8]*b[8]];
}
function det3(m){
  return m[0]*(m[4]*m[8]-m[5]*m[7]) - m[1]*(m[3]*m[8]-m[5]*m[6]) + m[2]*(m[3]*m[7]-m[4]*m[6]);
}
/** A vector through a row-major 3x3; `unit` renormalises, for directions under a scale. */
function rot3(m,v,unit){
  const o=[m[0]*v[0]+m[1]*v[1]+m[2]*v[2], m[3]*v[0]+m[4]*v[1]+m[5]*v[2], m[6]*v[0]+m[7]*v[1]+m[8]*v[2]];
  if(unit){ const L=Math.hypot(o[0],o[1],o[2])||1; o[0]/=L; o[1]/=L; o[2]/=L; }
  return o;
}
/** The NiColorData keys `[t, r, g, b, a]` at fraction `f` of the life, linearly. */
function keyColour(keys,f){
  if(f<=keys[0][0]) return keys[0].slice(1);
  const last=keys[keys.length-1];
  if(f>=last[0]) return last.slice(1);
  for(let i=0;i+1<keys.length;i++){
    const a=keys[i], b=keys[i+1];
    if(f<=b[0]){
      const s=b[0]-a[0], u=s>1e-9? (f-a[0])/s : 0;
      return [a[1]+(b[1]-a[1])*u, a[2]+(b[2]-a[2])*u, a[3]+(b[3]-a[3])*u, a[4]+(b[4]-a[4])*u];
    }
  }
  return last.slice(1);
}
