/* Gardenfell, 28_particles.js. Copyright (c) 2026 Robin Hjelte. MIT License; see page-source/LICENSE. */
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
   additive (src alpha, one), smoke and mist are the usual alpha blend. Lit as the game
   lights each of its two kinds (round 18hn, see FS_PART): a rotating system by the
   ambient and its emissive, an auto-normal system as a sheet facing the camera under the
   sun, the lamps and the ambient; the colour routed as the NiVertexColorProperty says
   (18hm); and a glow map, when the system has one, added unlit on top. So the mist over
   a waterfall takes the hour's ambient, a torch's white emissive burns at full whatever
   the hour, and a cave's crystal flakes glow. Depth-tested and not depth-written; drawn in the real pass only — the
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
/* Round 18hn: the glow map, the texturing property's slot 4, added onto the lit colour
   unlit as the statics' is (FS_GRASS, glowAdd). Unit 1. */
uniform sampler2D uGlow; uniform int uHasGlow;
uniform float uMatAlpha;
/* The NiMaterialProperty's three colours, and how the NiVertexColorProperty routes the
   particle's own colour (round 18hm): 0 the material's diffuse and ambient are used and
   the particle colour is not (SOURCE_IGNORE), 1 the particle colour is the emissive
   (SOURCE_EMISSIVE), 2 the particle colour is diffuse and ambient (SOURCE_AMB_DIFF, the
   default). uLit is 0 for lighting mode 0: the texture times the colour, nothing else. */
uniform vec3 uEmissive, uMatDiffuse, uMatAmbient;
uniform int uVColMode;
/* Round 18hn: the lighting kind, 0 rotating (ambient and emissive), 1 auto-normal (a lit
   sheet). See the fragment's note. */
uniform int uPartLight;
uniform vec3 uSun; uniform float uBright;
uniform int uLit;
uniform vec3 uFogCol; uniform float uFogK;   // the flat fog, when there is no sky
/* Round 18f: which side of the water this pass draws. -1 keeps what is under the water
   line and is drawn before the surface, so the surface tints or refracts it; +1 keeps
   what is over it and is drawn after; 0 draws everything, where there is no water. */
uniform float uWaterZ; uniform int uSide;
${SKY_GLSL}
${LIGHT_GLSL}
out vec4 o;
void main(){
  if(uSide<0 && vW.z>=uWaterZ) discard;
  if(uSide>0 && vW.z<uWaterZ) discard;
  /* Round 18hj: a particle dissolves with the room's view distance exactly as the objects
     do (FS_GRASS, uViewFade): the same band, the same dither, so a flame does not hang in
     the air after its sconce has gone. Robin: "Particles should also fade out with the
     camera view distance, currently they don't resulting in floating particles". */
  if(uViewFade.z>0.5){
    float dv=distance(vW,uEye);
    if(dv>=uViewFade.y) discard;
    if(dv>uViewFade.x){
      const int bayer[16]=int[16](0,8,2,10,12,4,14,6,3,11,1,9,15,7,13,5);
      ivec2 q=ivec2(mod(gl_FragCoord.xy,4.0));
      float th=(float(bayer[q.x+q.y*4])+0.5)/16.0;
      if((dv-uViewFade.x)/max(uViewFade.y-uViewFade.x,1.0)>th) discard;
    }
  }
  vec4 t = uHasTex==1 ? texture(uTex, vUV) : vec4(1.0);
  /* Lit the way the game lights a particle: two kinds and a glow map (round 18hn,
     measured by Robin in a cell with no light at all, the Caverns of Karstaag, and read
     against the engine's own class notes). uPartLight says which kind.

     0, a NiRotatingParticles. Its data carries no normals and the engine generates none,
     so the fixed pipeline has no diffuse term for it: the ambient and the emissive and
     nothing else. The waterfall mist went black in that cell; round 18d's four canalworks
     screenshots (the splash at ~0.4 of its texture at noon, ~0.33 and ~0.17 at the two
     dusks, ~0.11 at midnight, "the sun playing no part") were this. The torch's flames and
     the candles' are this kind too and burn by their white emissive. What 17x and 18d
     drew, back for this kind.

     1, a NiAutoNormalParticles. NetImmerse's note on the class (MWSE carries it): "Normals
     are not provided in the particle data, but are automatically generated to face the
     camera when rendered." A lit sheet whose normal points at the eye: the sun by that
     normal's lambert, the scene ambient, every lamp in reach by a one-sided lambert and
     the install's falloff, the material routed by the NiVertexColorProperty as a shape's
     is (MGE XE's FixedFuncEmu has no particle case; this is what it does to any vertex).
     The Dwemer steam and the furn mist went black in that cell: lit, by nothing. Round
     18hm's model, kept for this kind. OpenMW lights both kinds this way with one fixed
     normal (0.3,0.3,0.3).

     And the glow map. OAAB's crystal flakes (The Wake of Hanin's tomb) are an auto-normal
     system whose texturing property names its own sheet twice, in the base slot and in
     the glow slot, so the game adds the sheet unlit on top of the lit sheet: at the
     texture's full brightness in that lightless cell, at every distance and angle, with
     the lamps' colours as a faint cast on top in the tomb, brighter under the fixed
     pipeline than under per-pixel lighting's tonemap. OpenMW draws them dim, which Robin
     confirmed on his OpenMW; the game is the reference here. (A rule keyed on their
     NiParticleRotation modifier was tried first and disproved by swapping the modifier
     between the flakes and the furn mist in that cell: nothing changed.) The glow goes
     inside the tonemap and under the clamp's product as the statics' does.

     The routing, MGE's three (vertexMaterialNone / DiffAmb / Emissive), alpha included:
     the material's diffuse alpha when the material is the colour source, the particle's
     when it is. The alpha blend stays the NiAlphaProperty's.
     (No backticks in here.) */
  vec3 glowAdd = uHasGlow==1 ? texture(uGlow, vUV).rgb : vec3(0.0);
  vec3 c; float alpha = t.a;
  if(uLit==0){
    // Lighting mode 0: the fixed pipeline with lighting off draws the current colour times the texture.
    c = t.rgb*vCol.rgb + glowAdd; alpha *= vCol.a;
  } else {
    vec3 n = normalize(uEye - vW);
    float d = max(dot(n, uSun), 0.0);
    vec3 sunU = underLight(uSunCol, vW), ambU = underLight(uAmbCol, vW);
    // The same sky terms the statics take (FS_GRASS): the ramp's under the atmosphere, the fixed pair without.
    vec3 dif = (uScat==1 || uRoom==1) ? d*sunU : 0.85*d*sunU;
    vec3 amb = (uScat==1 || uRoom==1) ? ambU : 0.46*ambU;
    if(uPartLight==0){ dif = vec3(0.0); }                               // no normals: no diffuse at all
    else { dif += gdnLights(vW, n, -1.0); }                              // -1: one-sided, the game's lambert on a sheet
    vec3 light;
    if(uVColMode==0){ light = uMatDiffuse*dif + uMatAmbient*amb + uEmissive; alpha *= uMatAlpha; }
    else if(uVColMode==1){ light = uMatDiffuse*dif + uMatAmbient*amb + vCol.rgb; alpha *= uMatAlpha; }
    else { light = vCol.rgb*(dif + amb) + uEmissive; alpha *= vCol.a; }
    // The Unlit switch (round 18f): the texture times the colour as painted, no sun, no ambient, no lamps.
    if(uNoLight==1) light = uVColMode==2 ? vCol.rgb : vec3(1.0);
    // Clamped under the fixed pipeline and OpenMW, tonemapped under MGE's per-pixel lighting, as the statics are.
    c = uLampModel == 1 ? gdnTonemap(t.rgb*light + glowAdd) : min(t.rgb*min(light, vec3(1.0)) + glowAdd, vec3(1.0));
  }
  c *= uBright;
  c = airFog(c, vW);
  o = vec4(c, alpha);
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
      /* Round 18hm: the material's ambient and the NiVertexColorProperty's routing (see
         FS_PART). A payload from before this round has neither: white ambient, the
         particle colour as diffuse and ambient, lit - the game's defaults. */
      ambient:s.ambient||[1,1,1], vcolMode:s.vcolMode==null? 2 : (s.vcolMode|0),
      /* Round 18hn: the lighting kind the engine worked out from the shape type (0
         rotating, 1 auto-normal). An older payload has none: it is read from what it does
         carry, the data's type. And the glow map's texture, when the system has one. */
      partLight: s.lighting!=null ? (s.lighting|0) : ((s.data&&s.data.rotating)? 0 : 1),
      glow:(s.glGlow&&s.glGlow.gl)||null,
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
      lit:!s.unlit,
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
  drawParticles(VP, eye, airOn, sun, fogK, side, first){
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
       first packed.
       Round 18ep: which side comes first depends on where the eye is - under the water it
       is the puffs ABOVE the line that go in before the surface - so `first` says which of
       the pair this call is rather than the sign of `side` saying it. Without water there
       is no pair and nothing is ever reused. */
    side = side|0;
    const again = !first && !!this.water && P.packedFor!=null;
    const now=this.clock();   // 18fh: the pausable clock
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
    gl.uniform1i(pr.u.uTex,0); gl.uniform1i(pr.u.uGlow,1);
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
      /* Round 18hj: past the room's view distance nothing of a particle survives the shader,
         so it is not packed either; its emitter still steps, so coming back into reach finds
         it where it would have been. The limit is 0 (none) outside a room. */
      const far=+o.viewFar||0, ex=eye[0], ey=eye[1], ez=eye[2];
      for(const r of g.runs){
        const p=r.p, n=r.n;
        for(let i=0;i<n;i++){
          if(drawn+at/9>=PART_MAX_DRAW) break;
          const k=i*12, age=p[k+6], life=p[k+7];
          if(far>0){ const dx=p[k]-ex, dy=p[k+1]-ey, dz=p[k+2]-ez, sz0=p[k+8]*r.sc;
            if(dx*dx+dy*dy+dz*dz>(far+sz0)*(far+sz0)) continue; }
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
      // Round 18hn: the glow map on unit 1, the white stand-in when there is none.
      gl.activeTexture(gl.TEXTURE1);
      gl.bindTexture(gl.TEXTURE_2D, t.glow||this.white);
      gl.uniform1i(pr.u.uHasGlow, t.glow?1:0);
      gl.activeTexture(gl.TEXTURE0);
      gl.uniform3fv(pr.u.uEmissive, t.emissive);
      gl.uniform3fv(pr.u.uMatDiffuse, t.diffuse);
      gl.uniform3fv(pr.u.uMatAmbient, t.ambient);
      gl.uniform1i(pr.u.uVColMode, t.vcolMode);
      gl.uniform1i(pr.u.uPartLight, t.partLight);
      gl.uniform1f(pr.u.uMatAlpha, t.matAlpha);
      gl.uniform1i(pr.u.uLit, t.lit?1:0);
      gl.drawElementsInstanced(gl.TRIANGLES,6,gl.UNSIGNED_SHORT,0,n);
      drawn+=n; live+=count;
    }
    P.drawn=drawn; P.live=live;
    if(!again) P.packedFor = first? now : null;   // 18ep: the other half of the pair may follow
    gl.bindVertexArray(null);
    gl.blendFunc(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA);
    gl.disable(gl.BLEND);
    gl.depthMask(true);
    // Something is moving, so the next frame has to be drawn: this is what animates.
    // Round 18fh: unless the viewport is held - then the flames stand with everything
    // else, and this asking for another frame was what kept the GPU at full tilt
    // behind a dialogue and with "Pause rendering when still" on.
    if(any && !this._hold) this.dirty=true;
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
