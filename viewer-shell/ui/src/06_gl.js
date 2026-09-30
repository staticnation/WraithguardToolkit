
/* =====================================================================================
   Minimal Z-up math + WebGL2 renderer with instanced grass
   ===================================================================================== */
const M4={
  ident:()=>new Float32Array([1,0,0,0, 0,1,0,0, 0,0,1,0, 0,0,0,1]),
  mul:(a,b,o)=>{
    o=o||new Float32Array(16);
    for(let c=0;c<4;c++)for(let r=0;r<4;r++){
      o[c*4+r]=a[r]*b[c*4]+a[4+r]*b[c*4+1]+a[8+r]*b[c*4+2]+a[12+r]*b[c*4+3];
    }
    return o;
  },
  persp:(fovy,asp,zn,zf)=>{
    const f=1/Math.tan(fovy/2), nf=1/(zn-zf);
    return new Float32Array([f/asp,0,0,0, 0,f,0,0, 0,0,(zf+zn)*nf,-1, 0,0,2*zf*zn*nf,0]);
  },
  /* Round 18ee: the sun's cascades are orthographic. D3DXMatrixOrthoRH's shape, in GL's
     clip space (z to -1..+1 rather than 0..1), so MGE's box sizes carry over as written. */
  ortho:(w,h,zn,zf)=>{
    const d=1/(zf-zn);
    return new Float32Array([2/w,0,0,0, 0,2/h,0,0, 0,0,-2*d,0, 0,0,-(zf+zn)*d,1]);
  },
  lookAt:(eye,ctr,up)=>{
    let zx=eye[0]-ctr[0], zy=eye[1]-ctr[1], zz=eye[2]-ctr[2];
    let l=Math.hypot(zx,zy,zz)||1; zx/=l; zy/=l; zz/=l;
    let xx=up[1]*zz-up[2]*zy, xy=up[2]*zx-up[0]*zz, xz=up[0]*zy-up[1]*zx;
    l=Math.hypot(xx,xy,xz)||1; xx/=l; xy/=l; xz/=l;
    const yx=zy*xz-zz*xy, yy=zz*xx-zx*xz, yz=zx*xy-zy*xx;
    return new Float32Array([
      xx,yx,zx,0, xy,yy,zy,0, xz,yz,zz,0,
      -(xx*eye[0]+xy*eye[1]+xz*eye[2]),
      -(yx*eye[0]+yy*eye[1]+yz*eye[2]),
      -(zx*eye[0]+zy*eye[1]+zz*eye[2]), 1]);
  }
};

const FOV=48*Math.PI/180;   // vertical field of view, shared by projection and pan scaling

/** The tint for ground painted "grass here whatever else says", as the shader wants it.
 *
 *  Kept beside `--grow` in the stylesheet and worth saying why they are two: the swatch on
 *  the button is CSS and the mark on the terrain is a uniform, and the day they drift apart
 *  the tool is showing one colour on the button and another on the ground it paints.
 *  `#6f9f4a` — leaf green, a few degrees off the olive accent rather than across the wheel
 *  from it. It was a cooler mint, which Robin reported as reading cold beside everything
 *  else here. */
const GROW_TINT=[0.435,0.624,0.290];

/* The grass vertex shader, in two forms that draw the same blade (round 18cs). The
   instance - world position, yaw, scale, ground normal - used to arrive as three
   instanced attributes, and a cell run of a batch was drawn by re-pointing all three at
   the run's offset: seven calls a run, four thousand a frame at forty-nine cells. Now the
   instance is two texels of an RGBA32F texture (`buildBatch`), fetched by index the way
   the objects' groups fetch theirs (VS_STATIC_M): `base` is the run's first instance, from
   a uniform array by `gl_DrawID` under WEBGL_multi_draw, else a plain uniform. A batch is
   one multi-draw call then, every visible run a sub-draw. Same floats in, same blade out.
   `mode` is 'multi' or 'base'. (No backticks in the body: template literal.) */
const VS_GRASS_M=(mode)=>`#version 300 es
${mode==='multi'? '#extension GL_ANGLE_multi_draw : require' : ''}
precision highp float;
precision highp int;
layout(location=0) in vec3 aPos;
layout(location=1) in vec3 aNrm;
layout(location=2) in vec2 aUV;
layout(location=6) in vec4 aCol;     // the mesh's own vertex colour and alpha; white when it has none (18df: RGBA)
layout(location=7) in vec2 aUV2;     // the detail map's UV set (round 17h); (0,0) without one
uniform highp sampler2D uInst;       // the batch's instances, two texels each (round 18cs)
${mode==='multi'? 'uniform ivec4 uRunBase['+RUN4+'];' : 'uniform int uBase;'}
uniform mat4 uVP;
uniform int  uAlign;
/* The ground's colour under each blade - round 11 item 11, Robin's "diffuse
   transmission": a blade tinted by the ground it stands on at its base, fading to its
   own colour up the blade. uGround is the ground map, the terrain drawn from straight
   above with nothing but its textures and vertex colours (see bakeGroundMap);
   uGroundXf maps a scene xy onto it (xy: origin, zw: 1/extent). Sampled once per
   vertex at the *instance* position, so the whole blade reads one ground colour and
   the fade is by height alone - a blade that changed colour across its width would look
   painted, not lit. uBladeH is how tall this mesh is, which is what "up the blade" is
   measured against (uBlade). uTint 0 leaves vG white and the fragment shader does nothing.
   (No backticks in here: template literal.) */
uniform sampler2D uGround;
uniform vec4  uGroundXf;
uniform vec3  uBlade;    // x: z of the mesh's base, y: its height above that, z: the rule's lift above the ground (world units)
uniform int   uTint;
/* Wind - round 12 item 6, redone in round 14 after MGE XE's own grass shader
   (XE Mod Grass.fx, grassDisplacement). uWind is the strength, 0..1, uTime the clock
   in seconds. MGE's rule: a wind vector leans every vertex by
   saturate(0.02 * z) * harmonics * (2 * windVec + 0.1) - the lean grows with the
   vertex's height in the mesh up to 50 units and is flat above, the harmonics are two
   sine waves of period one unit over the world (so every vertex flutters on its own)
   sitting on offsets of 16 and 8, and the vertex colour's red channel is a stiffness
   mask (a blade a mod painted black stands still). What is Gardenfell's: MGE's wind
   vector comes from the game's weather; here it is the slider's strength along one
   fixed direction, breathed on by a slow gust that travels across the field so the
   grass is seen to move rather than only to lean. Preview only: it moves nothing the
   plugin records. (No backticks in here.) */
uniform float uWind, uTime;
/* How far the grass is drawn, and where it starts going (round 17v). x is the distance in
   world units and y the width of the band it shrinks away over; x at zero draws the lot,
   which is what every earlier round did. Robin frames all forty-nine cells at once, so
   nothing can be culled by the frustum and fifteen million triangles of grass go in every
   frame whichever way he is facing - this is the other lever, and the one the game itself
   pulls: MGE XE's grass distance shrinks a blade away rather than popping it.
   uEyeG is where the camera is; the grass program has no lighting block to borrow one
   from. (No backticks in here: template literal.) */
uniform vec2 uGrassFar;
uniform vec3 uEyeG;
/* Round 18dn: pixels per world unit at unit distance - the viewport's height over twice
   the tangent of half the field of view - so a length L at distance d covers L*uPxK/d
   pixels. What decides where a blade is under a pixel (see far, below). */
uniform float uPxK;
out vec2 vUV; out vec2 vUV2; out vec2 vUVD; out vec2 vUVDc; out vec3 vN; out vec3 vW; out vec3 vC; out vec3 vG; out float vH;
out vec2 vEnv;   // a blade has no environment map (18dn); the mesh shaders fill this
out float vCA;   // the vertex alpha (round 18df)
/* 1 where the ground map covers this blade, 0 where it does not (round 17u). */
out float vGK;
flat out vec2 vLamp;   // a blade is no lamp (round 17w)
vec2 grassDisplacement(vec3 world, float h, vec2 windVec){
  float v=length(windVec);
  vec2 displace=2.0*windVec+0.1;
  float gtime=uTime*0.5;
  float ph=world.x+world.y+world.z+gtime;
  vec2 harmonics=vec2(abs((0.5+0.03*v)*sin(-2.0*3.14*ph)+16.0),
                      abs((1.0+0.044*v)*sin(-3.0*3.14*ph)+8.0));
  return clamp(0.02*h,0.0,1.0)*harmonics*displace;
}
void main(){
  ${mode==='multi'? 'int base=uRunBase[gl_DrawID>>2][gl_DrawID&3];' : 'int base=uBase;'}
  int t=(base+gl_InstanceID)*2;
  ivec2 at=ivec2(t&${INST_W-1}, t>>${INST_SHIFT});
  vec4 i0=texelFetch(uInst,at,0);
  vec4 i1=texelFetch(uInst,at+ivec2(1,0),0);
  vec3 iPos=i0.xyz;                  // world position
  vec3 iRSA=vec3(i0.w, i1.x, 0.0);   // x=yaw, y=scale, z=unused
  vec3 iNrm=i1.yzw;                  // ground normal (for alignment)
  vC=aCol.rgb; vCA=aCol.a; vUVDc=vec2(0.0); vLamp=vec2(-1.0);
  /* How far up the blade this vertex is, for the ground tint's fade - 0 where the tint is
     whole, 1 where it has gone. Round 18a: measured from where the mesh meets the ground
     to a third of the way up to the top. Robin: "Make the grass tint stretch from where
     the grass intersects with the ground and up to a third way up to the top of the
     meshes from there [...] Currently it stretches too far up." It used to run the whole
     height.

     Round 18d: where the ground *is*. 18a took it for the mesh's z = 0, and Robin's grass
     said otherwise: "leave 2/3 of the mesh that is above the terrain untinted, not paint
     two thirds as it does now." Remiros' tufts run from z = -22 to +26 - the origin in the
     middle - and a blade stands 16 units (plus its rule's height offset) *above* the ground
     along the ground's normal, so the terrain cuts the mesh at z = -lift/scale, well below
     zero: everything from there up to zero was taking the full tint, and the fade only
     started above that. uBlade.z is the rule's lift in world units; divided by the
     instance's scale it is the ground's z in the mesh's own space, and the tint runs from
     there (or from the mesh's foot, when it floats above that) to a quarter of the way up
     to the top. A straight ramp, in the fragment shader: "a gradient from the terrain
     intersection". Round 18q: a quarter, where it was a third - Robin's own figure,
     "25% up the mesh on the intersection with the ground".

     Round 18bf: "along the ground's normal" above is now exactly true rather than nearly.
     The lift travels up the *mesh* - the surface normal for a blade that follows the
     ground, world up for one that does not - so for a tilted blade the terrain really is
     the plane at mesh z = -lift/scale, and this ramp starts where the ground meets the
     mesh instead of a little off it on every slope.

     Not clamped here, on purpose. A grass quad is two vertices tall, and a value clamped
     at the vertices is interpolated straight up the face between them - 0 at the foot, 1
     at the tip, whatever the numbers said the fade's reach was. That is *why* it ran the
     whole height once. The fraction is affine in z, so it interpolates exactly; the
     fragment shader clamps it where it is used. */
  {
    float ground = -uBlade.z / max(iRSA.y, 1e-3);
    float base = max(uBlade.x, ground);
    float top  = uBlade.x + uBlade.y;
    float run  = (top - base) / 4.0;
    vH = (uBlade.y>0.0 && run>0.0) ? (aPos.z-base)/run : 1.0;
  }
  /* The ground under this blade, and whether there *is* any.
     Round 17u: the lookup is checked rather than clamped. CLAMP_TO_EDGE answers a sample
     from outside the map with the nearest edge texel, so a map that does not cover the
     blade — one baked for another cell, one baked before the patch moved, one whose
     extent is short — hands every blade outside it the same colour, and the feet all go
     that colour together. That is what "the grass goes white from bottom and upwards"
     looks like, and it is indistinguishable from the tint working on ground that happens
     to be pale. A blade with no ground map under it now takes no tint at all, which is
     the honest answer and one that can never whiten anything. */
  /* Inside the map is not the same as standing on ground. The map is baked over the
     ground's bounding box, and the ground does not fill its own box: Simplified previews
     a patch a fifth of a cell wide inside a full cell, so nine tenths of its map is the
     grey the bake clears to, and a blade reading that grey goes pale at the foot and
     bright once it is lit - "the grass go white from bottom and upwards". So the bake
     leaves alpha at zero where it drew nothing, and a blade standing there takes no tint
     at all rather than a tint of nothing. Round 17v. */
  vec2 guv = (iPos.xy-uGroundXf.xy)*uGroundXf.zw;
  vec4 gnd = textureLod(uGround, clamp(guv, 0.0, 1.0), 0.0);
  vGK = (uTint==1 && guv.x>=0.0 && guv.x<=1.0 && guv.y>=0.0 && guv.y<=1.0 && gnd.a>=0.5)
        ? 1.0 : 0.0;
  vG  = vGK>0.0 ? gnd.rgb : vec3(1.0);
  float c=cos(iRSA.x), s=sin(iRSA.x);
  vec3 p=vec3(aPos.x*c-aPos.y*s, aPos.x*s+aPos.y*c, aPos.z)*iRSA.y;
  /* Out past the distance, the blade shrinks into the ground over the last band and is a
     degenerate triangle beyond it - no fragments, and no hard line where the grass stops.
     Scaled here, before the ground alignment and the wind, so a blade on the way out
     leans and lies with the rest of them. */
  float fade=1.0;
  float dEye=length(iPos.xy-uEyeG.xy);
  if(uGrassFar.x>0.0){
    fade=1.0-smoothstep(uGrassFar.x-uGrassFar.y, uGrassFar.x, dEye);
    p*=fade;
  }
  /* Where a blade is under a pixel, the wind's lean and the ground's tilt are skipped:
     they move it by less than the pixel it is in. Robin (18dm): "Remove wind and tint
     work where the contribution is under a pixel" - the wind and the tilt; the tint
     stays, since the colour of a far field is the sum of its blades' colours and is not
     under a pixel. 18dm gated this at two thirds of the grass distance; Robin: "setting
     a hard limit for when the grass gets simplified is better than a relative one. Lets
     start having that limit at a length where the grass is less than a pixel, it could
     probably be measured". Measured here, per blade, from the two lengths that bound
     what the skipped work can move: the blade's own height (the tilt turns it about
     its foot, so its tip moves at most that far) and the wind's lean, which the MGE rule
     above caps at 55 units at full strength (harmonics up to 16.5 and 9, times a
     displacement of at most 2.9). The longer of the two at this distance under one pixel
     - L*uPxK/d < 1 - is "less than a pixel", whatever the window's height and
     whatever the grass distance is set to. d is the straight-line distance to the eye,
     not the horizontal one the grass distance uses: from the overview, high above the
     field, the horizontal distance is short and the blades are still specks. On a
     1440-pixel-tall window that is 13 cells for the wind and 12 cells for a vanilla
     blade of 60 units, 30 for a Lush Synthesis blade of 150; on a 1080 one three
     quarters of that - in practice the zoomed-out overview, where every blade is far. */
  float motion = max(uBlade.y*iRSA.y, uWind>0.0 ? 55.0 : 0.0);
  bool far = motion*uPxK < length(iPos-uEyeG);
  vec3 n=vec3(aNrm.x*c-aNrm.y*s, aNrm.x*s+aNrm.y*c, aNrm.z);
  if(uAlign==1 && !far){
    vec3 g=normalize(iNrm);
    vec3 axis=cross(vec3(0.0,0.0,1.0), g);
    float al=length(axis);
    if(al>0.0005){
      axis/=al;
      /* Not g.z and the cross product's length for the cosine and sine, though they are
         the same numbers: 18dl tried it and the rounding moved 256 pixels of a
         nine-cell frame by up to 54 - a blade's edge landing on the other side of a
         pixel - and a renderer change here is pixel-identical or it is not made. */
      float ang=acos(clamp(g.z,-1.0,1.0));
      float ca=cos(ang), sa=sin(ang), ic=1.0-ca;
      mat3 R=mat3(
        ca+axis.x*axis.x*ic,        axis.y*axis.x*ic+axis.z*sa, axis.z*axis.x*ic-axis.y*sa,
        axis.x*axis.y*ic-axis.z*sa, ca+axis.y*axis.y*ic,        axis.z*axis.y*ic+axis.x*sa,
        axis.x*axis.z*ic+axis.y*sa, axis.y*axis.z*ic-axis.x*sa, ca+axis.z*axis.z*ic);
      p=R*p; n=R*n;
    }
  }
  vW=p+iPos;
  if(uWind>0.0 && !far){
    // The gust: two waves travelling over the field, keyed by where the blade stands.
    float gp=dot(iPos.xy, vec2(0.7,0.7))*0.004;
    float gust=sin(uTime*1.3+gp)*0.6 + sin(uTime*2.9+gp*2.3)*0.4;
    // Full strength is a wind vector of two units - a tip lean of some sixty units.
    vec2 windVec=vec2(0.7,0.7)*uWind*2.0*(0.6+0.4*gust);
    /* Round 18bd (F9): the blade's height as it stands, which is the mesh height scaled
       by the blade's own scale and by the distance fade. Two bugs in one number. Past the
       grass distance p collapses to zero so every vertex lands on iPos - and the wind
       then displaced them by an amount worked out from the un-shrunk mesh height, so the
       degenerate triangle became a flat horizontal sliver: about a pixel at wind 0.25,
       four at wind 1, tens of thousands of them shimmering where the comment above
       promises no fragments and no hard line where the grass stops. And MGE's rule is
       that the lean grows with a vertex's height up to 50 units, which is a fact about
       how tall the blade is: one at scale 0.5 used to lean as far as one at scale 2. The
       displacement clamps, so a tall blade is unchanged. (No backticks in here.) */
    vW.xy+=aCol.r*grassDisplacement(vW, aPos.z*iRSA.y*fade, windVec);
  }
  vN=n; vUV=aUV; vUV2=aUV2; vUVD=vec2(0.0); vEnv=vec2(0.5);
  gl_Position=uVP*vec4(vW,1.0);
}`;

/* The point lights a cell's LIGH records put in it - round 17m, Robin: "For all .nif
   files, if they have a light source, I would want to lit the scene with them, but ONLY
   when it's dark [...] Lights is it's own form of record in Morrowind engine, and in
   that record there is information about how far they light things up. For light falloff
   etc. look at OpenMW."

   The record is LIGH and the number is its LHDT radius. OpenMW's falloff, with the
   attenuation values Morrowind.ini ships (files/openmw.cfg: UseConstant 0, UseLinear 1,
   LinearMethod 1, LinearValue 3.0, LinearRadiusMult 1.0, UseQuadratic 0), comes to

       illumination = 1 / (3 * dist / radius)

   and then, in OpenMW's legacy method, a smooth fade to nothing across the band
   radius..2*radius (lighting_util.glsl's illumination *= 1.0 - quickstep(dist/radius -
   1.0), a hard stop past twice the radius). Round 18h: the page caps it at 1 as OpenMW
   does and fades it out over the outer half of the radius instead, stopping at the
   radius - which is what Robin's game does, measured (see gdnLights). The light's
   colour multiplies the material's diffuse and the lambert term, exactly as the sun's
   does; OpenMW applies no dimmer to it at all. Radius is floored at 16 by
   createLightSource, which the engine does when it sends them.

   uLightN of them, each a position, a colour and a radius. Off entirely when uLightN
   is 0, which is what the daytime does.
   (No backticks in here: this is pasted into template literals.) */
const LIGHT_GLSL=`
/* The cell's lamps, in a texture rather than a uniform array, binned into a grid.

   Round 17q had them as two uniform vec4 arrays. That put a hard ceiling on how many
   there could be - GL_MAX_FRAGMENT_UNIFORM_VECTORS, two vectors a lamp - and, worse, made
   every fragment loop every lamp in the scene whether or not any of them could reach it:
   at 49 cells, going from 32 lamps to 128 tripled the frame (63 ms to 196 ms, measured in
   PERFORMANCE.md). Both problems are the same problem, and the answer is not a bigger
   array.

   So: every lamp lives in uLights, a floating-point texture, two texels each - xyz
   position and radius, then rgb colour and the sign a Negative light carries. The world is
   divided into square columns GDN_BIN units across, and uBins holds, for each column, an
   offset and a count into uIndex, a flat list of the lamps whose *reach* covers that
   column. A fragment works out which column it is in and loops that column's lamps and no
   others. A canton with three hundred lanterns costs a fragment the four or five that are
   near it.

   Columns rather than boxes: Morrowind's lamps sit at head height above whatever they are
   near, so the interesting variation is horizontal. A third dimension would buy little and
   cost a dimension of grid.
   (No backticks in here: this is a template literal.) */
uniform highp sampler2D uLights;    // 3 texels a lamp: (xyz, radius), (rgb, sign), (c, l, q, ambient)
uniform highp isampler2D uBins;     // one texel a column: (offset, count)
/* Round 18i: which renderer's falloff. 0 the fixed pipeline (vanilla, MGE with per-pixel
   lighting off, OpenMW's legacy method): 1/(c + l·d + q·d²), the sum clamped after. 1 MGE
   XE's FixedFuncEmu: 1/(q·d² + c) - the linear term dropped, as its shader drops it -
   with the light's own ambient term, unclamped, tonemapped after. 2 OpenMW's shader
   methods: the fixed pipeline's term clamped to 1 per light and faded out between the
   light's bounds radius (the record's times the bounds multiplier) and twice it. */
uniform int   uLampModel;
uniform float uLampBounds;          // OpenMW's light bounds multiplier (1 elsewhere)
/* MGE XE's static tonemap (XE FixedFuncEmu.fx): "curve maps 0 -> 0, 1.0 -> 0.84, up to
   2.2 -> 1.0". What its per-pixel lighting does instead of clamping the light sum. */
vec3 gdnTonemap(vec3 c){
  c = clamp(c, 0.0, 2.2);
  return (((0.0548303*c - 0.189786)*c - 0.154732)*c + 1.12969)*c;
}
uniform highp isampler2D uIndex;    // the lamp indices, columns end to end
uniform vec4 uBinGrid;              // xy the grid's origin, z the column size, w the width
uniform int  uLightN;               // 0 turns the whole thing off

float gdnQuickstep(float x){
  x = clamp(x, 0.0, 1.0);
  x = 1.0 - x*x;
  return 1.0 - x*x;
}
ivec2 gdnAt(int i, int w){ return ivec2(i % w, i / w); }
/* The most lamps one column may hold. A column is GDN_BIN units square, and what lands in
   it is the lamps whose reach covers it, so this is not "lamps in the scene" but "lamps
   overlapping this patch of ground" - a number that stays small even in Vivec. The loop
   below is bounded by it, and the JS side caps each column to match. */
#define GDN_BIN_MAX 32

/* How much of a lamp reaches the faces turned away from it.
   0 is OpenMW's one-sided max(N.L, 0); 1 is its GROUNDCOVER wrap, abs(N.L). Round 17m had
   1 everywhere and Robin said the lights were "uniformly a tiny bit too bright"; 17n had 0
   everywhere and he said the look had been better before, with a lamp lighting one side of
   one mesh and the other side of the next.

   Both readings are right, and the reason is the geometry rather than the shader. A lamp
   mounted against a wall hangs a few tens of units off it, so L is nearly tangent to that
   wall and one-sided lambert gives it almost nothing; anything that tilts a little towards
   the lamp - the next wall segment up, with its own smoothed normals - lights up instead,
   and the join between two meshes becomes a seam. Morrowind lights statics per *vertex*
   across large flat pieces, which smears that pool into a wash and is why the game does
   not show it. Half a wrap is the nearest honest thing a per-pixel renderer can do. */
#define GDN_WRAP 0.5
/* And the preview's own gain on top of OpenMW's attenuation, at Robin's word across two
   rounds - "I want the lights to be slightly brighter than they are now", then "up their
   light intensity a bit more", then, once the grid was in, "a bit brighter light still".
   Named rather than folded into the falloff, so what is the engine's and what is ours
   stays separable. 1.0 is OpenMW exactly. */
#define GDN_GAIN 1.0

/* What the lamps add at this point, given the surface normal.
   The falloff was OpenMW's legacy one whole - 1/(3 dist/radius) unclamped, faded across
   radius..2*radius - with GDN_GAIN 1.75 on top. Round 18h, Robin: "The actual light from
   lanterns etc. is a bit blown out", with his game beside it: the game's pool on a Hlaalu
   wall, measured off his screenshot against a 256-radius lantern, is 1 : 0.7 : 0.25 : 0.03
   at 70, 114, 171 and 229 units - the linear 1/(3d/r) clamped to one (OpenMW clamps it so
   too), and gone *at* the radius, not at twice it. So: 1/(3d/r), capped at 1, faded out
   linearly over the outer half of the radius and stopped there; the gain is 1. Unclamped,
   the 1.75 gain made the inner 150 units of every lantern a flat disc of the lamp's colour
   clipped per channel to yellow-green; the game's stays the lamp's orange.
   (No backticks in here: this is a template literal.) */
vec3 gdnLights(vec3 w, vec3 n, float wrap){
  vec3 sum = vec3(0.0);
  if(uLightN <= 0) return sum;
  int gw = int(uBinGrid.w);
  ivec2 cell = ivec2(floor((w.xy - uBinGrid.xy) / uBinGrid.z));
  if(cell.x < 0 || cell.y < 0 || cell.x >= gw || cell.y >= gw) return sum;
  ivec4 bin = texelFetch(uBins, cell, 0);
  int at = bin.x, cnt = bin.y;
  int iw = textureSize(uIndex, 0).x;
  for(int k=0; k<GDN_BIN_MAX; k++){
    if(k >= cnt) break;
    int i = texelFetch(uIndex, gdnAt(at + k, iw), 0).x;
    vec4 P = texelFetch(uLights, ivec2(i*3,   0), 0);
    vec4 C = texelFetch(uLights, ivec2(i*3+1, 0), 0);
    vec4 A = texelFetch(uLights, ivec2(i*3+2, 0), 0);
    vec3 d = P.xyz - w;
    float dist = length(d);
    float r = P.w;
    /* Round 18i: the falloff is the install's (see uLampModel). Where the engine stops
       a light is a matter of which *objects* it attaches to - anything whose bounds the
       radius reaches, lit whole - which has no per-pixel shape; the preview fades each
       model out over the last quarter of twice its radius, where the terms above have
       fallen to a few percent anyway, and the light grid bins to that reach. */
    float ill, reach;
    if(uLampModel == 1){
      ill = 1.0 / max(A.z*dist*dist + A.x, 1e-4);
      reach = 2.0*r;
      ill *= clamp((reach - dist) / max(0.5*r, 1e-4), 0.0, 1.0);
    } else if(uLampModel == 2){
      float R = r*uLampBounds;
      ill = clamp(1.0 / max(A.x + A.y*dist + A.z*dist*dist, 1e-4), 0.0, 1.0);
      ill *= 1.0 - gdnQuickstep(dist/max(R,1e-4) - 1.0);
      reach = 2.0*R;
    } else {
      ill = 1.0 / max(A.x + A.y*dist + A.z*dist*dist, 1e-4);
      reach = 2.0*r;
      ill *= clamp((reach - dist) / max(0.5*r, 1e-4), 0.0, 1.0);
    }
    if(dist >= reach) continue;
    float nl = dot(n, d/max(dist,1e-4));
    /* MGE's PPL is a plain one-sided lambert plus the light's ambient term (lightAmbient,
       0 for a standard lamp); the half-wrap below is the preview's own for the other two,
       see GDN_WRAP. */
    float lam = uLampModel == 1 ? max(nl, 0.0) + A.w
                                : mix(mix(max(nl, 0.0), abs(nl), GDN_WRAP), abs(nl), wrap);
    sum += C.rgb * C.w * GDN_GAIN * lam * ill;
  }
  return sum;
}
`;

const SKY_GLSL=`
/* The atmosphere's part in a surface shader - round 13, a port of MGE XE's
   XE Common.fx (fogColourScatter / fogColour / fogColourSky), fed the game's own
   weather colours by 25_sky.js. uScat 0 is the flat fog as it always was.
   uSkyCol, uFogColFar: the ramp's sky and fog colours now. uSunPos: the sun, or its
   mirror under the horizon at night. uSunAlt: MGE's three sun-altitude terms, worked
   out once a frame. uNice: 1 in clear or cloudy weather, when the scatter runs.
   uFogStart / uFogRange: the exponential fog's start and divisor in units.
   uSunCol / uAmbCol: the ramp's sun and ambient, white when the atmosphere is off.
   (No backticks in here.) */
uniform int   uScat;
/* Round 17y: 1 in a room lit by its own AMBI record - uAmbCol and uSunCol are then the
   record's ambient and sunlight, summed the plain way, with no atmosphere and no fog. */
uniform int   uRoom;
uniform vec3  uEye;
/* Round 15: the reflection pass (26_water.js) draws the scene from under the water and
   must not see what is under it - a fragment below uClipZ is dropped while uClip is 1.
   Round 18du: with the eye under the water the mirror shows the water's own inside
   (MGE flips its clip plane, renderwater.cpp) - a fragment above uClipZ is dropped
   while uClip is 2. */
uniform int   uClip;
uniform float uClipZ;
uniform vec3  uSunCol;
uniform vec3  uAmbCol;
uniform vec3  uSkyCol, uFogColFar, uSunPos, uSunAlt;
uniform vec3  uInscatter, uOutscatter;
uniform vec4  uSkyScatter;
uniform float uNice, uFogStart, uFogRange;
/* Round 18f: the Unlit switch. 1 draws every surface at its texture - no sun, no ambient,
   no lamps - with the fog and everything else as they were. What a modelling tool calls
   unlit or flat: the colours as they are painted, not as they are lit. */
uniform int   uNoLight;
/* Round 18dt/18du: the eye is under the water. The game's fog then is linear in distance
   (MGE: "shaders use all linear fogging in this case", and XE Common.fx's fogColour takes
   fogMWScalar within the view range; OpenMW: fog.glsl's start/end pair for the camera's
   side of the water), in one colour, with no scattering - XE Common.fx switches the
   scatter off with the eye below sea level. uUnderRange is (start, end) in units, start
   usually negative: the fog is already on the eye. uUnderCol is UnderwaterColor blended
   with the weather's fog colour (25_sky.js underwaterFog) - MGE's fogColFar, read back
   from the game.

   18du, Robin: "It feels weird that the water surface effect continues outside of the
   water plane when it's cut into a square." The game's sea has no edge; the tool's has
   four, and past them is air. So uUnderBox (centre x, y, half width, on) bounds the
   water sideways: the ray's distance *inside* the box takes the water's fog, and the rest
   of the way, past a side wall, the atmosphere's own - a pixel that looks out past the
   square's edge clears up there instead of drowning in fog that has nowhere to be. Not
   the top: through the surface the game fogs by the whole distance, and so does this.
   Off (w = 0) for the reflection pass, whose eye is the mirror image. */
uniform int   uUnder;
uniform vec3  uUnderCol;
uniform vec2  uUnderRange;
uniform vec4  uUnderBox;
/* Round 18dy: how high the eye stands over the water. 0 while it is under - the water
   starts at the eye - and a height while it is over the line inside the crossing, where
   the sheet is clipped away by the near plane and the floor would otherwise draw in plain
   air. Then the water starts where the ray meets the line instead. */
uniform float uUnderTop;
/* Round 18dz: the blend's two halves - the water's own colour and its weight - for the
   crossing, where the eye is over the line and only part of the picture is under it.
   Under the line the whole frame's light is blended already (18dx, in airOn) and this is
   off (w = 0); during the crossing it is on and every lit fragment asks for itself.
   Robin: "run the full effect of being under water on all the pixels that are beneath the
   water surface when the surface cuts the near clipping plane [...] changing only fog
   doesn't really solve the problem", and it would not: the light is five times the fog. */
uniform vec4  uUnderMix;
/* Round 18ea: and inside the water's footprint, not merely below its height. 18dz tested
   the line alone, so every rock, ruin and tentacle in the cell that happened to stand
   below that height went dark with the sea - a dead straight edge across the whole frame,
   wherever the water actually was. Robin: "I seem to get a black line with the under water
   effect at the point where the water surface was when I rise up through the water again."
   That was not a line drawn on the picture; it was the world cut in half at sea level. */
vec3 underLight(vec3 c, vec3 w){
  if(uUnder != 1 || uUnderMix.w <= 0.0) return c;   // 18eb: never on a frame with no water in it
  if(w.z > uEye.z - uUnderTop) return c;
  if(uUnderBox.w > 0.0 && (abs(w.x - uUnderBox.x) > uUnderBox.z || abs(w.y - uUnderBox.y) > uUnderBox.z)) return c;
  return mix(c, uUnderMix.rgb, uUnderMix.w);
}
/* Where a ray from the eye along dir enters the water: nought under the line, and the
   crossing while the eye is over it. Rays that never go down never enter. */
float underEntry(vec3 dir){
  if(uUnderTop <= 0.0) return 0.0;
  return dir.z < -1e-6 ? uUnderTop / -dir.z : 1e9;
}
/* How far a ray from the eye along dir runs before it leaves the water's footprint. */
float underExit(vec3 dir){
  if(uUnderBox.w <= 0.0) return 1e9;
  float tx = dir.x > 1e-6 ? (uUnderBox.x + uUnderBox.z - uEye.x) / dir.x
           : dir.x < -1e-6 ? (uUnderBox.x - uUnderBox.z - uEye.x) / dir.x : 1e9;
  float ty = dir.y > 1e-6 ? (uUnderBox.y + uUnderBox.z - uEye.y) / dir.y
           : dir.y < -1e-6 ? (uUnderBox.y - uUnderBox.z - uEye.y) / dir.y : 1e9;
  return max(0.0, min(tx, ty));
}
/* The water's own fog over a distance: the clear share in .a, the haze in .rgb. */
vec4 underFogDist(float wet){
  float f = clamp((uUnderRange.y - wet) / max(1.0, uUnderRange.y - uUnderRange.x), 0.0, 1.0);
  return vec4(uUnderCol * (1.0 - f), f);
}
vec4 fogColourScatter(vec3 dir, float fogdist, float fog, vec3 skyColDir){
  skyColDir *= 1.0 - fog;
  if(uNice > 0.001){
    float suncos = dot(dir, uSunPos);
    float mie = (1.58 / (1.24 - suncos)) * uSunAlt.z;
    float rayl = 1.0 - 0.09 * mie;
    float atmdep = 1.33 * exp(-2.0 * clamp(dir.z, 0.0, 1.0));
    vec3 sunscatter = mix(uInscatter, uOutscatter, 0.5 * (1.0 + suncos));
    vec3 att = atmdep * sunscatter * (uSunAlt.x + 0.7 * mie);
    att = (1.0 - exp(-fogdist * att)) / att;
    vec3 newSky = mix(uSkyCol, uSkyScatter.rgb, uSkyScatter.a);
    vec3 color = 0.125 * mie + newSky * rayl;
    color *= att * (1.17 * atmdep + 0.89) * uSunAlt.y;
    color = mix(skyColDir, color, uNice);
    return vec4(color, fog);
  }
  return vec4(skyColDir, fog);
}
/* MGE's fogColour in the air: the amount in .a (1 = clear), the haze to add in .rgb. */
vec4 fogColourAir(vec3 dir, float dist){
  float fogdist = (dist - uFogStart) / uFogRange;
  float fog = clamp(exp(-fogdist), 0.0, 1.0);
  fogdist = clamp(0.224 * fogdist, 0.0, 1.0);
  return fogColourScatter(dir, fogdist, fog, uFogColFar);
}
/* Under the water: the water's fog for the way inside the box, the air's for the rest.
   Composed so that a * c + rgb is the water's haze laid over the air-fogged colour. */
vec4 underFog(vec3 dir, float dist){
  float tIn = underEntry(dir);
  float wet = max(0.0, min(dist, underExit(dir)) - tIn);
  vec4 f = underFogDist(wet);
  float rest = dist - wet;
  if(rest > 0.5){
    vec4 a = uScat == 1 ? fogColourAir(dir, rest)
           : vec4(uFogCol * clamp(1.0 - exp(-uFogK * rest), 0.0, 1.0), clamp(exp(-uFogK * rest), 0.0, 1.0));
    return vec4(f.a * a.rgb + f.rgb, f.a * a.a);
  }
  return f;
}
vec4 fogColour(vec3 dir, float dist){
  if(uUnder == 1) return underFog(dir, dist);
  return fogColourAir(dir, dist);
}
/* The sky: MGE's fogColourSky. Under the water the horizon is the water's colour and the
   sky above it is the sky's own, with no scatter - XE Common.fx keeps drawing the sky,
   which is what shows through the surface from beneath. */
vec4 fogColourSky(vec3 dir){
  vec3 s = mix(uFogColFar, uSkyCol, 1.0 - pow(clamp(1.0 - 2.22 * clamp(dir.z - 0.075, 0.0, 1.0), 0.0, 1.0), 1.15));
  /* Under the water both of those are the water's colour (18dv, set in airOn), so this is
     that colour flat - the sky at infinite distance, wholly fogged - and the scatter is
     off, as XE Common.fx switches it off below sea level. 18dy: with the eye still over
     the line during the crossing (uUnderTop > 0) the sky is the sky, so this waits for
     the eye itself, as the colours it reads do. */
  if(uUnder == 1 && uUnderTop <= 0.0) return vec4(s, 0.0);
  return fogColourScatter(dir, 1.0, 0.0, s);
}
/* Round 18h: 1 while what is being drawn adds onto the picture (an additive blend - a
   flame, a glow, a spark). The fog cannot lay its haze on such a thing: the haze is on
   the picture already, and adding it again is how a torch three cells off became a white
   speck brighter than the night around it (Robin: "they become bright white specs at a
   far distance. It feels like fog should render on top of them"). What the fog does to an
   additive thing is take it away - the colour times what the air lets through, and no
   haze - which is the fixed pipeline's own answer when the fog colour is black. */
uniform int   uFogBlack;
/* What the air lets through from world position w: the fog's clear share, 1 near. */
float airClear(vec3 w){
  if(uUnder == 1){ vec3 ev = w - uEye; float dist = length(ev); return underFog(ev / max(dist, 0.001), dist).a; }   // 18dt: whatever the atmosphere does
  if(uScat == 1){
    vec3 ev = w - uEye;
    float dist = length(ev);
    return fogColour(ev / max(dist, 0.001), dist).a;
  }
  return clamp(exp(-uFogK * length(w.xy)), 0.0, 1.0);
}
/* ---------------- the sun's shadow, round 18ee ----------------------------------------
   A port of MGE XE's own shadow receiver (XE Mod Shadow.fx + XE Mod Shadow Data.fx,
   G7 fork), because Robin plays with them on and asked for the tool to look like the
   game rather than like a shadow-mapping tutorial. Everything below is MGE's, at its
   numbers; where OpenMW differs it is noted.

   Two cascades, near and far, in one atlas side by side - 37_shadow.js draws them and
   says what the boxes are. The stored value is an exponential shadow map: linear depth
   over the cascade's 16,384-unit range, blurred while it is still depth, and read back
   as 1 - saturate(exp(c*dz + bias)). That exponential is what gives the game's soft
   edge and what hides the bias problems a hard comparison has; OpenMW instead compares
   hard and pushes the sample along the surface normal (its normal offset distance, 1
   unit) with a polygon offset behind it.

   The shade is not a light being switched off. MGE darkens what was drawn - shade 0.4
   asymptotically, tinted (1, 0.97, 0.81) so a shadow goes slightly blue - and scales
   that by how sunlit the surface was to begin with (shadowSunEstimate): a wall already
   facing away from the sun has nothing to lose and is left alone, which is what keeps
   the picture from going muddy when the ambient is high. Shadows also fade out with the
   fog, by its square. */
uniform int   uShadow;        /* 1 when the map for this frame was drawn */
uniform mat4  uShadowVP[2];   /* world -> cascade clip, near then far */
uniform sampler2D uShadowTex;
uniform vec2  uShadowP;       /* x: one texel of a cascade in clip units; y: the sun-visibility term */
uniform vec2  uShadowFog;     /* Morrowind's near fog, start and end in units: MGE's nearFogStart, nearFogRange */
/* MGE's core-mod constants, verbatim. */
const float GDN_ESM_C = 60.0;
const float GDN_ESM_BIAS = 2e-3 * GDN_ESM_C;
const float GDN_ESM_SCALE = 32768.0;
const float GDN_SHADE = 0.4;
const vec3  GDN_SHADECOLOR = vec3(1.0, 0.97, 0.81);
/* The difference between the depth the light saw and the depth this fragment is at, in
   the near cascade where the fragment is well inside it and the far one otherwise.
   A margin of four texels keeps the blur kernel from reading the next cascade along. */
float gdnShadowDz(vec3 w){
  float m = 1.0 - 8.0 * uShadowP.x;
  for(int i=0;i<2;i++){
    vec4 sp = uShadowVP[i] * vec4(w, 1.0);
    vec3 c = sp.xyz / sp.w;
    if(i == 0 && (abs(c.x) > m || abs(c.y) > m || abs(c.z) > 1.0)) continue;
    if(i == 1 && (abs(c.x) > 1.0 || abs(c.y) > 1.0 || abs(c.z) > 1.0)) return 1e-6;
    vec2 uv = vec2((float(i) + 0.5 * c.x + 0.5) * 0.5, 0.5 * c.y + 0.5);
    return texture(uShadowTex, uv).r / GDN_ESM_SCALE - (0.5 * c.z + 0.5);
  }
  return 1e-6;
}
/* MGE's shadowSunEstimate: x / (shade + x), where x is the lambert term weighted by the
   sun's own luminance and its visibility. */
float gdnShadowLight(float lambert){
  float x = max(lambert, 0.0) * dot(uSunCol, vec3(0.36, 0.53, 0.11)) * uShadowP.y;
  return x / (GDN_SHADE + x);
}
/* Darkens a lit colour where the sun cannot reach it. Called just before the fog, which
   is where MGE's grass shader does it and where its blend over the finished frame
   effectively lands. */
vec3 gdnShade(vec3 c, vec3 w, float lambert){
  if(uShadow != 1 || uNoLight == 1) return c;   /* Unlit draws the textures as painted */
  /* Wraithguard: nor what adds light - a flame, a glow, a spell's sheet. MGE skips every
     additive draw when it lays its shadows over the frame (rendershadow.cpp renderShadow:
     "Additive alphas do not receive shadows"); uFogBlack is 1 for exactly those batches. */
  if(uFogBlack == 1) return c;
  float light = gdnShadowLight(lambert);
  if(light < 2.0 / 255.0) return c;
  float dz = gdnShadowDz(w);
  if(dz >= 0.0) return c;
  float v = (1.0 - clamp(exp(GDN_ESM_C * dz + GDN_ESM_BIAS), 0.0, 1.0)) * light;
  /* Out with the fog, by its square, as MGE fades it - and four times as fast under the
     water, where it also clamps the fade up (isAboveSeaLevel). Wraithguard: by the fog
     MGE fades them by, fogMWScalar - Morrowind's own linear near fog, over the pair MGE's
     adjustFog fits to the weather (uShadowFog) - not the distant land's exponential fog,
     which reaches much further and let shadows run on past where the game's stop. */
  float sd = length(w - uEye);
  float att = clamp((uShadowFog.y - sd) / max(1.0, uShadowFog.y - uShadowFog.x), 0.0, 1.0);
  att *= att;
  v *= uUnder == 1 ? clamp(4.0 * att, 0.0, 1.0) : att;
  /* The far cascade's own edge, so a shadow does not end on a straight line. */
  vec4 fp = uShadowVP[1] * vec4(w, 1.0);
  vec2 fade = clamp(25.0 * (1.0 - abs(fp.xy / fp.w)), 0.0, 1.0);
  v *= fade.x * fade.y;
  return c * (1.0 - v * GDN_SHADECOLOR);
}
/* Fogs a lit colour at world position w: MGE's way under the atmosphere, the flat
   fog by distance from the scene's centre without it. */
vec3 airFog(vec3 c, vec3 w){
  if(uFogBlack == 1) return c * airClear(w);
  if(uUnder == 1){ vec3 ev = w - uEye; float dist = length(ev); vec4 f = underFog(ev / max(dist, 0.001), dist); return f.a * c + f.rgb; }
  if(uScat == 1){
    vec3 ev = w - uEye;
    float dist = length(ev);
    vec4 f = fogColour(ev / max(dist, 0.001), dist);
    return f.a * c + f.rgb;
  }
  float fog = clamp(1.0 - exp(-uFogK * length(w.xy)), 0.0, 1.0);
  return mix(c, uFogCol, fog);
}
`;

/* Round 18q: the cull mask - one copy, shared by the grass and the ground.
 *
 * Both shaders ask exactly the same question of a fragment, and `_cullUniforms` sets one
 * set of uniforms for whichever of them is current, so the two have to agree down to the
 * type of every uniform. They did not: a hand-copied second draft still declared
 * `uCullCanopy` as a `vec2` while the setter had moved to `uniform4f`, and a `uniform4f`
 * on a `vec2` is INVALID_OPERATION on every draw of that program - which is how the
 * ground pass started poisoning `t_render`s error checks. One text, interpolated twice,
 * is the only way that stays true. */
const CULL_GLSL=`
/* Round 18q: the cull mask, every rule that culls.
 *
 * Robin: "I want all rules that cull grass to be represented in the cull mask on terrain
 * and on meshes. Default is to show the selected Rule's culling mask [...] When hovering
 * a grasscard, the culling mask [...] should update to reflect any override settings."
 *
 * What a fragment knows about itself answers most of it: how steep it is and which way it
 * faces (its normal), how high it is (its position), and how far it is from the sea -
 * which for a height field is (z - sea) * n.z / |n.xy|, the engine's own formula
 * (Ground::shore), exact on planar ground and the same first-order approximation
 * everywhere else.
 *
 * The two that need to look around - the ground's curvature and the distance to the
 * nearest object - and the one that needs the world's meshes - what hangs over the spot -
 * come from a field the engine bakes (cull_field), sampled here. A pixel outside the
 * field passes: there is no honest answer for ground the field does not cover.
 *
 * Every band is a low and a high, with (-1e9, 1e9) meaning "not asked". */
uniform vec2  uCullOn;      // x: the mask is on at all; y: the field is there
uniform vec2  uCullSlope;   // shallowest, steepest (degrees)
uniform vec2  uCullH;       // lowest, highest
uniform vec3  uCullFace;    // the rule's: heading, half width, 1 when asked
uniform vec3  uCullFace2;   // the hovered card's, the same shape - two arcs, both applied
uniform vec2  uCullShore;   // low, high (units from the sea's edge)
uniform vec2  uCullCurve;   // low, high (units; negative is a hollow)
uniform vec2  uCullObs;     // low, high (units from the nearest object)
/* The canopies, as bits of the field's blue channel: every bit in x has to be over the
   spot, no bit in y may be, and z/w say whether *anything* must be over it or must not.
   Several rules on one card are exactly this, however many there are. */
uniform vec4  uCullCanopy;
uniform vec4  uFieldXf;     // x0, y0, 1/width, 1/height of the field over the world
uniform float uFieldReach;  // what a full red channel means, in units
uniform sampler2D uField;
bool gdnCulled(vec3 wpos, vec3 nrm){
  if(uCullOn.x<0.5) return false;
  vec3 n=normalize(nrm);
  float slope=degrees(acos(clamp(n.z,-1.0,1.0)));
  if(slope<uCullSlope.x || slope>uCullSlope.y) return true;
  if(wpos.z<uCullH.x || wpos.z>uCullH.y) return true;
  if(n.z<0.9999 && (uCullFace.z>0.5 || uCullFace2.z>0.5)){
    float f=degrees(atan(n.x,n.y));
    if(uCullFace.z>0.5 && abs(mod(f-uCullFace.x+540.0,360.0)-180.0)>uCullFace.y) return true;
    if(uCullFace2.z>0.5 && abs(mod(f-uCullFace2.x+540.0,360.0)-180.0)>uCullFace2.y) return true;
  }
  if(uCullShore.x>-1e8 || uCullShore.y<1e8){
    float g=length(n.xy);
    float sh = g<1e-4 ? (wpos.z>=0.0? 1e6 : -1e6) : (wpos.z*n.z/g);
    if(sh<uCullShore.x || sh>uCullShore.y) return true;
  }
  bool wantCanopy = uCullCanopy.x>0.5 || uCullCanopy.y>0.5 || uCullCanopy.z>0.5 || uCullCanopy.w>0.5;
  bool wantField = uCullCurve.x>-1e8 || uCullCurve.y<1e8 || uCullObs.x>-1e8 || uCullObs.y<1e8 || wantCanopy;
  if(wantField && uCullOn.y>0.5){
    vec2 uv=vec2((wpos.x-uFieldXf.x)*uFieldXf.z, (wpos.y-uFieldXf.y)*uFieldXf.w);
    if(uv.x>=0.0 && uv.x<=1.0 && uv.y>=0.0 && uv.y<=1.0){
      vec3 fld=texture(uField,uv).rgb;
      float dist=fld.r*uFieldReach;
      if(dist<uCullObs.x || dist>uCullObs.y) return true;
      float curve=(fld.g*255.0-128.0)/127.0*512.0;
      if(curve<uCullCurve.x || curve>uCullCurve.y) return true;
      if(wantCanopy){
        int bits=int(fld.b*255.0+0.5);
        int under=int(uCullCanopy.x+0.5), clear=int(uCullCanopy.y+0.5);
        if((bits & under)!=under) return true;              // every named one, over it
        if((bits & clear)!=0) return true;                  // and none of the others
        if(uCullCanopy.z>0.5 && bits==0) return true;       // under *something*
        if(uCullCanopy.w>0.5 && bits!=0) return true;       // clear of everything
      }
    }
  }
  return false;
}
`;

const FS_GRASS=`#version 300 es
precision highp float;
in vec2 vUV; in vec2 vUV2; in vec2 vUVD; in vec2 vUVDc; in vec3 vN; in vec3 vW; in vec3 vC; in vec3 vG; in float vH;
in float vCA;
/* 1 where the ground map covers this blade, 0 where it does not (round 17u). VS_STATIC
   writes 0: a rock takes no ground tint and never did. */
in float vGK;
/* Round 17w: which lamp this instance is, as the hours it lights and goes out at, or
   (-1,-1) for a shape that is no lamp at all. Flat: it is one fact per instance. What it
   decides is whether the shape's own light - its emissive colour and its glow map - is on:
   a lamp follows the Lights rule, staggered by its own hours exactly as its point light
   is; anything else with a glow glows always, which is what a Telvanni crystal does in the
   game. Robin: "Things which has a glow map, but NO light source, should be always
   emissive / glowing [...] Only meshes with a light AND a glow map turns on and off."
   (No backticks in here: template literal.) */
flat in vec2 vLamp;
uniform float uHour;       // the clock, or -1 for no clock
uniform int   uLightMode;  // 0 the night rule, 1 always, 2 off
/* Round 17w: the NiVertexColorProperty. uUnlit 1 draws the shape as its texture times its
   vertex colours and nothing else - no sun, no ambient, no material. uVColMode says what
   the vertex colours are for: 0 nothing, 1 an emissive term added in, 2 multiplied into
   the colour (the default). */
uniform int   uUnlit;
uniform int   uVColMode;
/* Round 17x: Glow in the Dahrk. uDayNight is which branch of the mesh's NightDaySwitch
   this batch is - 0 none (always drawn), 1 OFF (the window by day), 2 ON (lit, at night),
   3 INT-DAY (an interior's window with the sun coming through). Outdoors the OFF branch
   shows by day and ON at night; in a room INT-DAY shows by day and OFF at night, as the
   mod's Lua does it. "Night" is the instance's own hours in vLamp - the mod's sunrise
   and sunset, with its variance when that is on - under the Lights rule. */
uniform int   uDayNight;
uniform int   uIndoors;
uniform sampler2D uTex;
uniform int   uHasTex;
/* How much of the ground's colour a blade takes at its base, 0..1 - the viewport's
   "Ground tint" slider. Off (0) for placed objects and thumbnails, whose vG is white
   anyway. (No backticks in here: template literal.) */
uniform float uTintK;
// 1: multiply by the mesh's vertex colours, as the game does; 0: the bare texture.
uniform int   uVCol;
/* The mesh's own alpha test, as NiAlphaProperty spells it: uAlphaFn is the compare
   function (0 ALWAYS, 1 LESS, 2 EQUAL, 3 LEQUAL, 4 GREATER, 5 NOTEQUAL, 6 GEQUAL,
   7 NEVER; -1 = no test at all) and uAlphaRef the threshold it compares against, 0..1.
   A fragment that fails the compare is discarded. Round 11: Vurt's dark fern tests
   GREATER against 0 - "anything but fully clear survives" - and the old
   "discard below the threshold, and a threshold of zero means no test" read that as
   "draw every texel", so the fern was an opaque sheet. See alphaCut() for how the
   flags become these two numbers. (No backticks in here: template literal.) */
uniform int   uAlphaFn;
uniform float uAlphaRef;
/* Round 14: 1 in the grass pass only - the blades are then drawn the way MGE XE's own
   grass shader draws them (XE Mod Grass.fx). Its cutout is a fixed one, discard below
   64/255 and the rest turned into coverage, sat(1.3 * (a - 128/255) + 128/255), for
   the multisample to soften the edge with; the mesh's own alpha property is not
   consulted, as MGE does not. Its light is wrap lighting: the lambert term is bent so
   a blade edge-on to the sun still reads, and a blade the sun is behind glows a
   little through (the backlight), which is what makes a field of grass in the game
   look lit from within rather than flat. And its vertex colours are ignored - MGE:
   "due to problem with some grass mods" - so a mod's black-painted blades are not
   black. Placed objects and thumbnails keep the game's plain rules. (No backticks.) */
uniform int   uGrassLit;
uniform vec3  uDiffuse;
/* Round 18de: the NiMaterialProperty's ambient colour, which scales the scene's ambient
   where the diffuse scales the sun and the lamps. The two are one and the same on most
   shapes, and the diffuse is sent here for a part that lists no ambient of its own.
   (No backticks in here: template literal.) */
uniform vec3  uMatAmbient;
/* Round 18de: 1 when the shape carries vertex colours (a shape without them is drawn with
   aCol white, which multiplies as nothing but must not count as the colour source). */
uniform int   uVColHas;
uniform vec3  uSun;
uniform vec3  uFogCol;
/* What a batch that is not the highlighted one recedes towards. The viewport's own
   background, never the weather's fog: under the atmosphere the fog is a pale sky
   colour, and dimming towards it made the rest of the grass *lighter* than it started
   (round 17f, Robin: "They should be the same color as when atmosphere is off no
   matter the setting"). */
uniform vec3  uDimCol;
uniform float uFogK;
uniform float uBright;
/* The NiMaterialProperty's alpha, 1 unless the shape says otherwise (round 17h). */
uniform float uMatAlpha;
/* The NiTexturingProperty's *detail* map and its own UV set: the base map modulated by
   it, doubled, the way the game's texture stage does and OpenMW's TexEnvCombine copies
   (setScale_RGB 2). pc_flora_scumalpha_03.nif is base pc_flora_scumalpha.dds - a white
   shape with an alpha cutout - times detail tx_bc_scum.dds, which is where all of its
   colour is: without this it draws as white cutouts, which is what Robin was seeing.
   (No backticks in here: template literal.) */
uniform sampler2D uDetail; uniform int uHasDetail;
/* Round 17y: the dark map, NiTexturingProperty's slot 1 - multiplied over the base with no
   doubling, the way OpenMW's objects.frag does (gl_FragData[0] *= texture2D(darkMap)). */
uniform sampler2D uDark; uniform int uHasDark;
uniform sampler2D uDecal; uniform int uHasDecal;   // round 18df: NiTexturingProperty slot 6
/* Round 18dl: the environment map a NiTextureEffect puts on the shape - sphere-mapped
   from the reflection of the eye ray in the camera's frame (per vertex since 18dn, see
   ENV_VS_GLSL: vEnv), added onto the lit colour and under the fog, the way the fixed
   pipeline's stage adds it (unlit, which is why an env-mapped thing glows in the dark
   in the game) - and the bump map on slot 5 that perturbs where it is read: the
   texel's red and green through the property's 2x2 matrix, and its blue through the
   luma scale and offset as the reflection's strength. 18dn: the red and green are taken
   *raw*, as MGE XE's bumpmapStage takes them (offset = mul(dUdV.rg, mat)) - the bump
   textures are plain DXT, and 18dl's signed reading (x2-1) doubled every ripple. */
uniform sampler2D uEnv; uniform int uHasEnv;
uniform sampler2D uBump; uniform int uHasBump; uniform vec2 uBumpLuma; uniform vec4 uBumpMat;
in vec2 vEnv;
/* Round 17p: the NiTexturingProperty's *glow* map, slot 4 - the light a shape makes
   rather than the light it takes. This is what a lamp's glass is: OAAB's three Telvanni
   lamps all carry textures/oaab/ab_telv_sphere_02_g.dds (the _g suffix is the whole
   convention), and Morrowind's own lanterns
   the same. OpenMW adds it to the final colour outright and *after the fog*
   (objects.frag: gl_FragData[0].xyz += texture2D(emissiveMap, emissiveMapUV).xyz), which
   is both why a lamp reads as lit at midnight and why it stays a point of light at the
   far end of a foggy street. Sent black by day, with the lamps.
   (No backticks in here: template literal.) */
uniform sampler2D uGlow; uniform int uHasGlow;
/* Wraithguard: an OpenMW-style normal map (the diffuse's _n/_nh sibling), tangent-space,
   on NRM_UNIT; and the mesh viewer's view mode - 0 as lit, 1 flat colour (no textures, so
   a shape reads as its shape), 2 the surface normals as colour. */
uniform sampler2D uNrm; uniform int uHasNrm; uniform int uViewMode;
/* A two-channel normal map (BC5, passed through): X and Y only, Z rebuilt. And the
   camera's right, up and back, for the mesh viewer's key light (views 1 and 2). */
uniform int uNrmRG; uniform mat3 uViewBasis;
/* A normal map with height in its alpha (_nh): OpenMW's parallax - every map read at a
   coordinate shifted along the eye's direction in the surface's own frame by
   height * 0.04 - 0.02 (shaders' parallax.glsl). */
uniform int uNrmH;
/* The studio views' light, as the old three.js viewer's lit material had it: a key light
   (direction in the world, its strength) over an ambient, and the gloss map (slot 3), a
   luminance mask on the highlight - which neither the game nor OpenMW draws, so it is
   read in these views only. */
uniform vec3 uKeyDir; uniform float uKeyInt, uAmbInt;
uniform sampler2D uGloss; uniform int uHasGloss;
/* And its specular map (the _spec sibling, OpenMW's): rgb the specular colour, alpha the
   shininess over 255, lit by the sun (objects.frag's specular term). */
uniform sampler2D uSpec; uniform int uHasSpec;
/* Round 17m: the NiUVController's texture matrix for this shape, at this instant —
   xy the tiling, zw the offset, applied as uv*xy + zw, which is the order OpenMW's
   UVController builds it in (scale, then setTrans). (1,1,0,0) is a still shape.
   It is what moves a waterfall, a stream ripple and the Gnisis fence's banner: the
   geometry never moves, the texture slides across it. (No backticks: template
   literal.) */
uniform vec4  uUVXform;
/* Round 18dj: the transform of the shape's other maps, and which of them it moves - 1
   the dark map, 2 the detail, 4 the glow, 8 the decal. A NiUVController names the UV
   set it drives, and a map takes the controller of the set it reads: the Ghostgate
   dome scrolls its ghostfence pattern (the dark map) on one controller while its base
   map's alpha drifts on another. 0 on every vanilla shape. */
uniform vec4  uUVXform2; uniform int uUVMaps2;
/* And the NiMaterialProperty's emissive colour, which the game adds to the lighting
   outright rather than multiplying it in - so a shape with a white emissive is bright
   at midnight. Zeroed while the lights are out, which is what makes the difference
   between light_paper_lantern_01.nif and light_paper_lantern_off.nif. */
uniform vec3  uEmissive;
${SKY_GLSL}
${LIGHT_GLSL}
// -1 nothing is being highlighted, 1 this batch is, 0 it is one of the others
uniform int   uHi;
/* 1 while a thumbnail is being drawn: what survives the alpha test is written solid.
   A thumbnail is composited against a panel rather than against the scene, and a mesh
   whose texture carries partial alpha - most of Morrowind's foliage - came out a ghost of
   itself, or with the fixture's flat-alpha texture, invisible while plainly being drawn.
   The cutout still happens; uAlphaRef above is what decides the shape, not this.
   (No backticks in here: this shader is a template literal.) */
uniform int   uOpaque;
/* The paint brush, on the objects. Off (w<0) everywhere except the placed-object pass,
   and there only while painting with "Paint on statics too" on.
   The brush is a sphere at uBrushP, and this draws where that sphere cuts the surface -
   so the outline forms itself to whatever is on screen, and the two outlines are the two
   radii: where the paint stops, and where the feather begins. The terrain shader does the
   identical thing with the identical numbers, which is what makes the ring one ring
   across ground and rock rather than two that meet by arrangement.
   The fragment's own world position is all it takes. There is no eye ray and no
   projection plane in it, which is why nothing between you and the sphere ever wears the
   outline: it is not in the sphere.
   (No backticks in here: this is inside a template literal.) */
uniform vec4  uBrushS;   // z outer radius, w inner (feather) radius; w<0 = off (xy unused)
/* The orbit pivot's stamp (round 17j): xyz where the ball is, w its radius; w<=0 = off. */
uniform vec4  uPivot;
/* Round 18cj: and the colour it stamps in, which is the theme's own orbit colour rather
   than the salmon this used to have baked in — the same salmon the "no grass" brush paints
   with, which it resembled by accident. (No backticks in here: template literal.) */
uniform vec3  uPivotCol;
uniform vec3  uBrushP;   // the centre of the sphere - where the pointer is
/* The culling overlay, on the placed objects: the same shade, the same limits, the same
   toggle as the ground's - Robin asked for the "Too steep" and "Too high / Too low"
   story to be one story across ground and rock. Off (0) in every other pass that shares
   this shader: the grass blades, and the thumbnails. The slope is read from the raw
   world normal, the way the placement pass reads it, not from the viewer-flipped one -
   and only tested when a limit is actually set, since a rock's underside is past ninety
   degrees by nature and a limit of ninety means "no slope limit".
   (No backticks in here: template literal.) */
uniform int   uShowSlope;
uniform float uMaxAngle, uMinH, uMaxH;
${CULL_GLSL}
out vec4 o;
void main(){
  /* The day/night switch first, before any texture is fetched: a branch that is not the
     one for this hour draws nothing at all. */
  if(uDayNight>0){
    bool night = vLamp.x<0.0 ? false
               : uLightMode==1 ? true
               : uLightMode==2 ? false
               : (uHour>=0.0 && (uHour>=vLamp.x || uHour<vLamp.y));
    bool show = uIndoors==1 ? (uDayNight==1 ? night : uDayNight==3 ? !night : false)
                            : (uDayNight==1 ? !night : uDayNight==2 ? night : false);
    if(!show) discard;
  }
  if(uClip==1 && vW.z<uClipZ) discard;
  if(uClip==2 && vW.z>uClipZ) discard;   // 18du: the mirror from under the water keeps what is under it
  vec2 auv = vUV*uUVXform.xy + uUVXform.zw;
  if(uHasNrm==1 && uNrmH==1){
    vec3 pn=normalize(vN); if(!gl_FrontFacing) pn=-pn;
    vec3 q1=dFdx(vW), q2=dFdy(vW);
    vec2 s1=dFdx(auv), s2=dFdy(auv);
    vec3 q2p=cross(q2,pn), q1p=cross(pn,q1);
    vec3 Tp=q2p*s1.x+q1p*s2.x, Bp=q2p*s1.y+q1p*s2.y;
    float ip=inversesqrt(max(max(dot(Tp,Tp),dot(Bp,Bp)),1e-20));
    vec3 ev=normalize(uEye-vW);
    vec3 te=vec3(dot(ev,Tp*ip), dot(ev,Bp*ip), dot(ev,pn));
    float hgt=texture(uNrm,auv).a;
    auv += te.xy*(hgt*0.04-0.02);
  }
  vec4 t = uHasTex==1 ? texture(uTex,auv) : vec4(1.0);
  if(uViewMode==1) t.rgb=vec3(0.6);
  /* Round 18df: where the fragment's alpha comes from, beyond the texture. With the
     vertex colours as the colour source the fixed pipeline takes the diffuse - alpha and
     all - from the vertex (D3DMCS_COLOR1; OpenMW's getDiffuseColor()), and the material's
     alpha is not consulted; otherwise it is the material's. Not one vanilla shape has a
     vertex alpha below 1, so nothing in the game moves; OAAB's root pillars fade their
     ends through it, and a veteran saw them drawn solid. The alpha test sees the product
     too, as the pipeline's does. The grass pass keeps its own rule below.
     (No backticks in here: template literal.) */
  bool vcolSrc = uVCol==1 && uGrassLit==0 && uVColMode==2 && uVColHas==1;
  /* 18dj: the vertex alpha *times* the material's, not instead of it. Memento Mori's
     Ghostgate dome carries vertex colours and a material at 0.5 and 0.2, and the game
     shows it faint - at the material's alpha - so the material is consulted even with
     the vertex as the colour source; OAAB's root pillar (material 1) still fades by its
     vertices. The product is the one rule both fit. */
  float srcA = vcolSrc ? vCA*uMatAlpha : uMatAlpha;
  /* The dark map (round 17y) multiplied in whole, alpha and all, before the alpha test -
     OpenMW's objects.frag order and rule (gl_FragData[0] *= texture2D(darkMap, ...), then
     alphaTest; no backticks in here: template literal). 18dj multiplied the colour alone: Memento Mori's Ghostgate dome
     reads the ghostfence texture as its dark map, and that texture's alpha is a pattern
     at 0.47-0.93 - Robin, asked: "Do it the OpenMW way, multiply the dark map's alpha
     into the fragment." A Glow in the Dahrk window's dark map is DXT1, alpha 1 throughout. */
  if(uHasDark==1) t*=texture(uDark, (uUVMaps2&1)!=0 ? vUVD*uUVXform2.xy+uUVXform2.zw : vUVD);
  if(uGrassLit==1){
    if(uHasTex==1 && t.a < 64.0/255.0) discard;
    t.a = clamp(1.3*(t.a-128.0/255.0)+128.0/255.0, 0.0, 1.0);
  }
  else if(uAlphaFn>=0){
    float ta = t.a*srcA;
    bool pass = uAlphaFn==0 ? true
              : uAlphaFn==1 ? ta <  uAlphaRef
              : uAlphaFn==2 ? abs(ta-uAlphaRef) <  0.002
              : uAlphaFn==3 ? ta <= uAlphaRef
              : uAlphaFn==4 ? ta >  uAlphaRef
              : uAlphaFn==5 ? abs(ta-uAlphaRef) >= 0.002
              : uAlphaFn==6 ? ta >= uAlphaRef
              : uAlphaFn==8 ? t.a > uAlphaRef     // 18dg: the low cut, on the texture alone
              : false;
    if(!pass) discard;
  }
  /* The detail map over the base (round 17h), on its own UV set, modulated and doubled
     the way the game's second texture stage does. */
  if(uHasDetail==1) t.rgb=clamp(t.rgb*texture(uDetail, (uUVMaps2&2)!=0 ? vUV2*uUVXform2.xy+uUVXform2.zw : vUV2).rgb*2.0, 0.0, 1.0);
  /* Round 18df: the decal (slot 6) laid over the base by its own alpha, after the dark
     and detail stages, as OpenMW's objects.frag orders them - a poster on a wall, a road
     sign's board. Nothing in vanilla; OAAB and Tamriel Rebuilt have thirteen. */
  if(uHasDecal==1){ vec4 dc=texture(uDecal, (uUVMaps2&8)!=0 ? vUVDc*uUVXform2.xy+uUVXform2.zw : vUVDc); t.rgb=mix(t.rgb, dc.rgb, dc.a); }
  /* The mesh's vertex colours, multiplied in the way the game does - a basket's weave
     and a pot's shading are mostly these. White when the shape has none. */
  if(uVCol==1 && uGrassLit==0 && uVColMode==2) t.rgb*=vC;
  /* And the ground under the blade, at the base, fading out up the blade. The ground
     map is albedo and so is t at this point; both are lit together below, so a blade
     that took the ground's colour is lit as the ground beside it is. */
  if(uTintK>0.0 && vGK>0.0){
    // vH is clamped here, per fragment, and not at the vertices - see where it is set.
    // A straight ramp (round 18d): the tint whole at the ground, gone a third of the way up.
    float f=clamp(vH,0.0,1.0);
    t.rgb=mix(t.rgb, mix(vG,t.rgb,f), uTintK);
  }
  vec3 n=normalize(vN);
  if(!gl_FrontFacing) n=-n;
  if(uHasNrm==1){
    /* The tangent frame from the screen-space derivatives of position and UV (no tangent
       attribute needed), then the map's tangent-space normal through it. B is the
       direction V grows, which in a NIF's UVs (and a DDS's rows) is *down* the image - so
       green is read as the down component: DirectX-style maps, the convention OpenMW's
       own normal maps use (checked on tx_ashl_a_banner_n.dds). No flip. */
    vec3 dp1=dFdx(vW), dp2=dFdy(vW);
    vec2 du1=dFdx(auv), du2=dFdy(auv);
    vec3 dp2p=cross(dp2,n), dp1p=cross(n,dp1);
    vec3 Tn=dp2p*du1.x+dp1p*du2.x, Bn=dp2p*du1.y+dp1p*du2.y;
    float inv=inversesqrt(max(max(dot(Tn,Tn),dot(Bn,Bn)),1e-20));
    vec3 tn=texture(uNrm,auv).xyz*2.0-1.0;
    if(uNrmRG==1) tn.z=sqrt(max(0.0, 1.0-dot(tn.xy,tn.xy)));
    n=normalize(mat3(Tn*inv,Bn*inv,n)*tn);
  }
  float d=max(dot(n,uSun),0.0);
  if(uGrassLit==1 && vC.r>0.5){
    /* MGE's wrap lighting for grass: the normal turned to face the eye (its
       -sign(dot(eyevec, n)), which gl_FrontFacing has already done), then
       lambert = pow(sat((l + w) / (1 + w)), N) * (N + 1) / (2 (1 + w)) + max(0, -l) * back
       with w 0.6, N 1.5 and a backlight of 0.4. */
    float l=dot(n,uSun);
    const float w=0.6, N=1.5;
    d = pow(clamp((l+w)/(1.0+w),0.0,1.0),N)*(N+1.0)/(2.0*(1.0+w)) + max(0.0,-l)*0.4;
  }
  /* Lit the way MGE lights its statics under the atmosphere - sun colour by the
     lambert term plus the ambient, both the ramp's - and the old fixed 0.46 + 0.85
     without it. */
  vec3 sunU = underLight(uSunCol, vW), ambU = underLight(uAmbCol, vW);   // 18dz: the crossing, per fragment
  vec3 light = (uScat==1 || uRoom==1) ? (ambU + d*sunU) : (0.46*ambU + 0.85*d*sunU);
  vec3 specAdd=vec3(0.0);
  if(uHasSpec==1 && d>0.0){
    vec4 sp=texture(uSpec,auv);
    vec3 H=normalize(uSun+normalize(uEye-vW));
    specAdd=sp.rgb*pow(max(dot(n,H),0.0),max(sp.a*255.0,1.0))*sunU;
  }
  /* Round 17m: and the cell's own lamps, after dark. Added to the light the sky gives,
     the way OpenMW sums the sun and the point lights into one diffuse term before it
     multiplies the material - so a rock beside a lantern is the rock's own colour lit
     brighter, not a rock with an orange decal on it. */
  // Round 18i: MGE XE's grass shader has no point lights in it at all.
  if(!(uLampModel == 1 && uGrassLit == 1)) light += gdnLights(vW, n, uGrassLit==1 ? 1.0 : 0.0);
  /* Round 18h: the material's diffuse scales what the lights give and nothing else. It
     used to scale the whole sum, emissive included, and Improved Lights for All Shaders'
     lanterns - a glass with diffuse 0.25 and a white emissive, so the lamp's own light
     does not double up on its glass - drew at a quarter. The fixed pipeline and OpenMW's
     objects.frag both add the emissive *after* the material: lighting = diffuse *
     diffuseLight + ambient * ambientLight + emission.
     Round 18de: the two halves of that the shader had wrong, found by the NIF audit.
     (a) The ambient material was never read - "every mesh met so far has it equal to the
     diffuse" - and 1,784 of Morrowind's own static shapes disagree: the shipwrecks, the
     colony piers, the Daedric walls sit at 0.1 ambient under a white diffuse, which is
     what keeps them dark in shade. So the ambient part of the light is scaled by
     uMatAmbient and the rest by uDiffuse. Written as the old product plus the ambient's
     correction, so a shape whose two colours agree is lit to the bit as before.
     (b) When the vertex colours are the colour source - vertex mode 2, the default for
     any shape that carries them - the fixed pipeline takes BOTH the ambient and the
     diffuse from the vertex (D3DMCS_COLOR1; OpenMW's AMBIENT_AND_DIFFUSE colour mode) and
     the material's two are not consulted at all. They were multiplied in on top of the
     vertex colour here, so the fire ferns (diffuse 0.15,0.08,0.04) drew near black, the
     willow flowers dark green, Vurt's snow trees and SHotN's houses at half. The vertex
     colour is already in t; the material stays out. The grass pass ignores vertex colours
     as MGE does, so it keeps the material.
     (No backticks in here: template literal.) */
  if(!(uVCol==1 && uGrassLit==0 && uVColMode==2 && uVColHas==1)){
    vec3 ambPart = (uScat==1 || uRoom==1) ? ambU : 0.46*ambU;
    light = light*uDiffuse + ambPart*(uMatAmbient - uDiffuse);
  }
  /* Whether this instance's own light is burning - see vLamp above. A lamp keeps the
     rule, staggered by its own two hours; everything else is always on. */
  /* Round 18i, Robin: "the surfaces of the lamps are now emissive no matter if the light
     is on or not. I want the emissive surface to only be active when the light is also
     on." vLamp tells three things apart now: (-2,-2) a lamp that keeps no hours - a fire,
     a room's - which glows unless the picker says Off, as lampLitAt puts its light out
     under Off and nothing else; (-1,-1) no lamp at all, whose own glow is always on
     (round 17w, Robin: "Things which has a glow map, but NO light source, should be
     always emissive"); and a lamp's two hours, which the picker overrides either way.
     Before this, a fire and a plain glowing static were the same -1 to the shader, so
     Off could not put the fire's glow out without putting out every crystal.
     (No backticks in here: template literal.) */
  float glowOn = vLamp.x<=-1.5 ? (uLightMode==2 ? 0.0 : 1.0)
               : vLamp.x<0.0 ? 1.0
               : uLightMode==1 ? 1.0
               : uLightMode==2 ? 0.0
               : (uHour>=0.0 && (uHour>=vLamp.x || uHour<vLamp.y)) ? 1.0 : 0.0;
  /* The shape's own glow - the material's emissive, and the vertex colours when the
     NiVertexColorProperty says they are the emissive term - goes into the *lighting*, before
     the clamp and before the texture, which is where the fixed pipeline and OpenMW's
     objects.frag both put it: lighting = diffuse*diffuseLight + ambient*ambientLight +
     emission, clamped, then times the texture. Rounds 17m-17w added it *after* the lit
     texture, as a second helping of the texture, and a window whose emissive is white came
     out at twice its texture - Robin (round 17y): "ex_nord_win_02 which is supposed to be a
     warm glow, but they are quite bright or a white looking light." A white emissive means
     the texture at full brightness, and no more than that. Gated per instance (round 17w). */
  light += uEmissive*glowOn;
  if(uVCol==1 && uVColMode==1) light += vC;
  /* Clamped before it multiplies the texture, the way OpenMW does (clampLightingResult,
     on by default). Its illumination is 1/(3·dist/radius) with no ceiling of its own, so
     close to a lamp the sum runs past 1 and a surface goes to white. Round 17n. */
  /* Round 18i: clamped under the fixed pipeline and OpenMW (its clamp lighting, on by
     default); MGE's per-pixel lighting clamps nothing and tonemaps the lit colour instead
     - except the grass, which MGE draws with its own shader: sun and ambient only, no
     lamps, no tonemap (XE Mod Grass.fx). */
  if(uLampModel != 1 || uGrassLit == 1) light = min(light, vec3(1.0));
  if(uNoLight==1) light = vec3(1.0);   // the Unlit switch (round 18f)
  /* Unlit (round 17w): the fixed pipeline with lighting off draws the current colour - the
     vertex colour, already in t when it is to be multiplied - times the texture, and the
     material is not consulted at all. */
  /* The glow map (round 17p), added onto the lit colour and *under* the fog. Round 17p
     put it after the fog, "exactly as OpenMW adds it", and that was misread: OpenMW's
     objects.frag adds the emissive map to the lit colour and applies the fog last, and
     the fixed pipeline's fog goes over every texture stage. Robin (18dj), of Better
     Telvanni Crystals' in_t_crystal_01, which glows by its own texture on slot 4: "It
     also emits a light that is multiplied or put on top of the fog instead of being
     covered by the fog. I want the fog to cover it fully." */
  /* Round 18dl: the environment map, read where the vertex shader's sphere mapping says
     (vEnv), shifted by the bump map's red and green through the property's matrix,
     scaled by its blue through the luma scale and offset - the fixed pipeline's
     BUMPENVMAPLUMINANCE stage as MGE XE's FixedFuncEmu and G7's DXVK PPL shader both
     write it (raw texel, implicit mip level from the perturbed coordinates).
     Round 18dp: where it goes. 18dl-18dn added it to the *lit texture* - the base map's
     stage first, the map's stage after, OpenMW's objects.frag order - and the sheen
     read at full strength, a white laid over the pod; Robin: "it kind of blends a bit
     with the underlying color in the game engine, but we just slap a white on top of
     the mesh". The fixed pipeline's stage order is the other way about: an effect
     texture's stage comes *before* the base map's (MGE XE's own precache of the
     enchantment effect, an environment map too, is stage 0 the effect with the
     reflection texgen, stage 1 the base map modulating what came before), so the map is
     added to the lighting and the base map then multiplies the sum - the sheen is the
     base's own colour brightened, a quarter as strong on a dark pod, and nothing at all
     in its black cracks: c = base * (light + env*luma). The glow map's stage is after
     the base (MGE's second precached stage, ADD onto current), as before. Under MGE's
     lamp model the whole goes into the tonemap (every stage, then tonemap, then fog);
     OpenMW's model keeps its own order, the map added after. */
  vec3 envAdd=vec3(0.0);
  if(uHasEnv==1){
    vec2 euv=vEnv;
    float luma=1.0;
    if(uHasBump==1){
      vec4 bt=texture(uBump, auv);
      euv+=vec2(bt.r*uBumpMat.x+bt.g*uBumpMat.z, bt.r*uBumpMat.y+bt.g*uBumpMat.w);
      luma=clamp(bt.b*uBumpLuma.x+uBumpLuma.y, 0.0, 1.0);
    }
    envAdd = texture(uEnv, euv).rgb*luma;
  }
  vec3 glowAdd = uHasGlow==1 ? texture(uGlow, (uUVMaps2&4)!=0 ? vUV*uUVXform2.xy+uUVXform2.zw : auv).rgb*glowOn : vec3(0.0);
  vec3 c = uUnlit==1 ? t.rgb*(1.0 + envAdd)*uBright + glowAdd
         : (uLampModel == 1 && uGrassLit == 0) ? gdnTonemap(t.rgb*(light + envAdd) + glowAdd)*uBright
         : uLampModel == 2 ? t.rgb*light*uBright + envAdd + glowAdd
         : t.rgb*min(light + envAdd, vec3(1.0))*uBright + glowAdd;   // the fixed pipeline saturates each stage
  if(uUnlit!=1) c+=specAdd*uBright;
  if(uShowSlope==1 && gdnCulled(vW,vN)) c=mix(c,vec3(0.812,0.416,0.298),0.38);
  // Picking one slot out of a field of grass only reads if the rest recede: a tint
  // on its own disappears among thousands of near-identical blades.
  if(uHi==1) c=mix(c,vec3(1.00,0.82,0.30),0.60);
  else if(uHi==0) c=mix(c,uDimCol,0.72);
  /* Plain exponential in distance. A squared ramp was tried — clearer up close, thicker
     far off — and read as too much fog at every distance a person actually looks from,
     so it went back. */
  if(uBrushS.w>=0.0){
    /* Distance to the sphere's centre, and two outlines: the radius the paint stops at
       and the radius the feather starts at. Every surface in the scene is tested the same
       way, so the outline is exactly the curve where the brush meets what you can see. */
    float bd=length(vW-uBrushP);
    float bt=fwidth(bd)*1.5+2.0;
    float rim=min(abs(bd-uBrushS.z), abs(bd-uBrushS.w));
    if(rim<bt) c=mix(c,vec3(1.000,0.627,0.322),0.75);
    else if(bd<uBrushS.z) c=mix(c,vec3(1.000,0.627,0.322),0.10);
  }
  /* Round 17j: the orbit pivot's stamp — a ring straight down from the ball onto
     whatever is under it, terrain or rock, the same radius as the ball. Robin asked for
     it to read like the paint brush, so it is the brush's own shape one dimension down:
     the brush tests a sphere and this tests a *cylinder*, which is what "straight
     downwards" means. The same colour as the ball, which since 18cj is the theme's own
     orbit colour (round 17m had it salmon, and salmon is a paint colour).
     Round 17m: and only onto what the ball is actually above. Robin: "Never stamp the
     terrain when the orbit view pivot ball is beneath the terrain, only stamp it when it
     is above the terrain." Tested per fragment against that fragment's own height, so a
     ball halfway into a slope stamps the ground below it and not the bank beside it.
     (No backticks in here: template literal.) */
  if(uPivot.w>0.0 && uPivot.z>vW.z){
    float pd=length(vW.xy-uPivot.xy);
    float pt=fwidth(pd)*1.5+1.5;
    if(abs(pd-uPivot.w)<pt) c=mix(c,uPivotCol,0.75);
    else if(pd<uPivot.w) c=mix(c,uPivotCol,0.10);
  }
  /* Round 18ee: the sun s shadow, where MGE puts it - over the lit colour and under the
     fog. The lambert term is the one the sun was given above, so a surface already
     facing away from the sun is not darkened twice. */
  c=gdnShade(c,vW,d);
  c=airFog(c,vW);
  /* Round 17h: the material's own alpha, and the NiAlphaProperty's blending with it.
     A shape that says it is translucent - OAAB's water_rectf256_01.nif at 0.7, and every
     other little pool, fountain and canal like it - was drawn solid before, since only
     the alpha test was honoured. (No backticks in here: template literal.) */
  /* Wraithguard: the mesh viewer's studio views. The normal map is never drawn as a
     picture: it bends the light on the surface, as the game uses it. Both views light the
     shape with one key light that rides with the camera (up and to the left of it, raking
     across the surface) over a little fill, so the relief the normal map gives reads from
     any angle, and the specular map's highlight follows the same light.
     1 Flat colour: clay grey - the shape and the normal map alone.
     2 Relief: the base texture, lit the same way. */
  if(uViewMode==1 || uViewMode==2){
    vec3 L=normalize(uKeyDir);
    float dk=max(dot(n,L),0.0);
    /* Flat colour is the old viewer's diffuse-off grey (0x9aa0aa), so turning the
       texture off never reads as a broken switch. */
    vec3 base = uViewMode==1 ? vec3(0.604,0.627,0.667) : t.rgb;
    vec3 sk=vec3(0.0);
    if(uHasSpec==1 && dk>0.0){
      vec4 sp=texture(uSpec,auv);
      vec3 Hk=normalize(L+normalize(uEye-vW));
      // OpenMW's reading of a _spec map: RGB the highlight's colour, alpha its shininess.
      sk=sp.rgb*pow(max(dot(n,Hk),0.0),max(sp.a*255.0,1.0))*uKeyInt;
      if(uHasGloss==1) sk*=texture(uGloss,auv).r;
    }
    c=base*(uAmbInt+uKeyInt*dk)+sk;
  }
  o=vec4(c, uOpaque==1 ? 1.0 : t.a*srcA);
}`;

const VS_GROUND=`#version 300 es
precision highp float;
layout(location=0) in vec3 aPos;
layout(location=1) in vec3 aNrm;
layout(location=2) in vec2 aUV;
layout(location=3) in vec3 aCol;   // VCLR, the record's vertex colour; white without one
uniform mat4 uVP;
out vec2 vUV; out vec3 vN; out vec3 vW; out float vSlope; out vec3 vC;
void main(){ vUV=aUV; vN=aNrm; vW=aPos; vC=aCol; vSlope=degrees(acos(clamp(aNrm.z,-1.0,1.0)));
  gl_Position=uVP*vec4(aPos,1.0); }`;

const FS_GROUND=`#version 300 es
precision highp float;
in vec2 vUV; in vec3 vN; in vec3 vW; in float vSlope; in vec3 vC;
uniform sampler2D uTex;
uniform sampler2D uBlend;
uniform int uHasTex, uGrid, uShowSlope, uUseBlend, uFirstLayer, uFlat;
/* Wraithguard: a land texture's own maps, as OpenMW's terrain uses them - its normal map
   (_n / _nh beside it) bending the light, and its _diffusespec, which is the diffuse
   itself with the highlight's strength in its alpha (uSpecA). DirectX-style, as the
   objects' (see FS_GRASS). */
uniform sampler2D uLNrm; uniform int uHasLNrm, uSpecA;
/* uVCol 1: multiply by the record's vertex colours, the way the game draws land - on
   for the texture layers, off for the paint and highlight overlays, which are marks on
   the tool and not on the world. uUnlit 1: albedo only - no sun, no fog, no grid, no
   overlays - for the ground map the grass reads its tint from (bakeGroundMap).
   (No backticks in here: template literal.) */
uniform int uVCol, uUnlit;
uniform float uMaxAngle, uMinH, uMaxH;
${CULL_GLSL}
uniform vec3 uSun, uFogCol, uFlatCol;
uniform float uFogK, uTile;
uniform float uBright;
${SKY_GLSL}
${LIGHT_GLSL}
// Scales the coverage map when the same layer is drawn again as a highlight.
uniform float uAlphaMul;
/* How far this pass tints towards the highlight gold, before the fog (round 17k). 0 off. */
uniform float uHiTint;
// Maps cell UV onto the coverage map. The map carries a one-texel ring of the
// neighbouring cells' textures, so the cell itself occupies the inner square.
uniform vec2 uBlendXf;   // x: scale, y: offset
// Paint mode. uGridCol turns the grid orange to say the mode is on without a second
// overlay; uBrushP is the centre of the brush's sphere and uBrushS its two radii -- where
// the paint stops (z) and where the feather begins (w), w<0 meaning no brush. Drawn as
// the curve where that sphere cuts the landscape, which is the same test and the same
// numbers the placed-object shader uses. (No backticks in here: this is inside a template
// literal, and one would end the shader.)
uniform vec3 uGridCol;
uniform vec4 uBrushS;
uniform vec4 uPivot;    // round 17j: the orbit pivot's stamp, xyz and radius (w<=0 off)
uniform vec3 uPivotCol; // round 18cj: in the theme's own orbit colour
uniform vec3 uBrushP;
out vec4 o;
void main(){
  if(uClip==1 && vW.z<uClipZ) discard;
  if(uClip==2 && vW.z>uClipZ) discard;   // 18du: the mirror from under the water keeps what is under it
  vec4 tx = uHasTex==1 ? texture(uTex,vUV*uTile) : vec4(uFlatCol,0.0);
  vec3 base = tx.rgb;
  // No image at all is a fault worth seeing, so it gets a checker — unless the
  // layer supplied a colour to stand in with, which the engine default does.
  if(uHasTex==0 && uFlat==0){
    vec2 g=floor(vW.xy/256.0);
    base = mod(g.x+g.y,2.0)<0.5 ? vec3(0.250,0.245,0.215) : vec3(0.305,0.300,0.265);
  }
  if(uVCol==1) base*=vC;
  if(uUnlit==1){
    float ua = 1.0;
    if(uFirstLayer==0){
      float um = texture(uBlend, vUV*uBlendXf.x + uBlendXf.y).r;
      ua = (uUseBlend==1) ? um : step(0.5, um);
    }
    if(ua<=0.004) discard;
    o=vec4(base,ua);
    return;
  }
  /* Splatting: each land texture is a layer with its own coverage map. Linear filtering
     across the 16x16 map turns the hard sub-cell grid into gradients, which is what
     Morrowind's terrain looks like in motion. Round 18ct: read *here*, before the
     lighting, so a fragment the map says nothing of is dropped before the lamps and the
     haze are worked out for it - the coverage decided the same discard at the end of
     the shader, after all of that. (No backticks in here: template literal.) */
  float a = 1.0;
  if(uFirstLayer==0){
    float m = texture(uBlend, vUV*uBlendXf.x + uBlendXf.y).r;
    // blending off still needs the coverage map, just thresholded: step() puts the
    // cut exactly on the sub-cell boundary, giving Morrowind's hard-edged look
    a = (uUseBlend==1) ? m : step(0.5, m);
  }
  a *= uAlphaMul;
  if(a<=0.004) discard;
  vec3 gn=normalize(vN);
  if(uHasLNrm==1){
    vec2 luv=vUV*uTile;
    vec3 dp1=dFdx(vW), dp2=dFdy(vW);
    vec2 du1=dFdx(luv), du2=dFdy(luv);
    vec3 dp2p=cross(dp2,gn), dp1p=cross(gn,dp1);
    vec3 Tn=dp2p*du1.x+dp1p*du2.x, Bn=dp2p*du1.y+dp1p*du2.y;
    float inv=inversesqrt(max(max(dot(Tn,Tn),dot(Bn,Bn)),1e-20));
    vec3 tn=texture(uLNrm,luv).xyz*2.0-1.0;
    gn=normalize(mat3(Tn*inv,Bn*inv,gn)*tn);
  }
  float d=max(dot(gn,uSun),0.0);
  vec3 sunU = underLight(uSunCol, vW), ambU = underLight(uAmbCol, vW);   // 18dz: the crossing, per fragment
  vec3 light = uScat==1 ? (ambU + d*sunU) : (0.40*ambU + 0.85*d*sunU);
  // Round 17m: the cell's lamps light the ground too, or a lantern would hang over a
  // black street.
  light += gdnLights(vW, gn, 0.0);
  // The _diffusespec highlight: the alpha its strength, OpenMW's terrain shininess (128).
  vec3 lspec=vec3(0.0);
  if(uSpecA==1 && d>0.0){
    vec3 H=normalize(uSun+normalize(uEye-vW));
    lspec=vec3(tx.a)*pow(max(dot(gn,H),0.0),128.0)*sunU;
  }
  if(uLampModel != 1) light = min(light, vec3(1.0));   // as above (round 17n); round 18i: MGE tonemaps instead
  if(uNoLight==1) light = vec3(1.0);   // the Unlit switch (round 18f)
  vec3 c = uLampModel == 1 ? gdnTonemap(base*light)*uBright : base*light*uBright;
  c += lspec*uBright;
  if(uShowSlope==1 && gdnCulled(vW,vN)) c=mix(c,vec3(0.812,0.416,0.298),0.38);
  if(uGrid==1){
    vec2 f=abs(fract(vW.xy/512.0)-0.5);
    float ln=min(f.x,f.y);
    float w=fwidth(vW.x/512.0)*1.4;
    if(ln<w) c=mix(c,uGridCol,0.45);
  }
  if(uBrushS.w>=0.0){
    float d=length(vW-uBrushP);
    float t=fwidth(d)*1.5+2.0;
    // Two outlines: where the brush stops, and where the feather begins.
    float rim=min(abs(d-uBrushS.z), abs(d-uBrushS.w));
    if(rim<t) c=mix(c,vec3(1.000,0.627,0.322),0.75);
    else if(d<uBrushS.z) c=mix(c,vec3(1.000,0.627,0.322),0.10);
  }
  /* Round 17j: the orbit pivot's stamp — a ring straight down from the ball onto
     whatever is under it, terrain or rock, the same radius as the ball. Robin asked for
     it to read like the paint brush, so it is the brush's own shape one dimension down:
     the brush tests a sphere and this tests a *cylinder*, which is what "straight
     downwards" means. The same colour as the ball, which since 18cj is the theme's own
     orbit colour (round 17m had it salmon, and salmon is a paint colour).
     Round 17m: and only onto what the ball is actually above. Robin: "Never stamp the
     terrain when the orbit view pivot ball is beneath the terrain, only stamp it when it
     is above the terrain." Tested per fragment against that fragment's own height, so a
     ball halfway into a slope stamps the ground below it and not the bank beside it.
     (No backticks in here: template literal.) */
  if(uPivot.w>0.0 && uPivot.z>vW.z){
    float pd=length(vW.xy-uPivot.xy);
    float pt=fwidth(pd)*1.5+1.5;
    if(abs(pd-uPivot.w)<pt) c=mix(c,uPivotCol,0.75);
    else if(pd<uPivot.w) c=mix(c,uPivotCol,0.10);
  }
  /* Round 17k: the ground-texture highlight, tinted *before* the haze rather than laid
     over it. It was a flat gold drawn as its own pass, so under the atmosphere the gold
     itself went through airFog and came out pale - Robin: "The ground texture highlight
     is still too white compared when atmosphere is not on." The grass and the meshes have
     always tinted inside the shader and fogged afterwards, which is why they stay gold;
     this is now the same thing on the ground. (No backticks in here: template literal.) */
  if(uHiTint>0.0) c=mix(c,vec3(1.000,0.820,0.300),uHiTint);
  /* Plain exponential in distance. A squared ramp was tried — clearer up close, thicker
     far off — and read as too much fog at every distance a person actually looks from,
     so it went back. */
  /* Round 18ee: the sun s shadow, where MGE puts it - over the lit colour and under the
     fog. The lambert term is the one the sun was given above, so a surface already
     facing away from the sun is not darkened twice. */
  c=gdnShade(c,vW,d);
  c=airFog(c,vW);
  o=vec4(c,a);
}`;

/* Round 18dn: an environment map's sphere-map coordinates, per *vertex*, the way the
   fixed pipeline makes them. NetImmerse fills a texture-coordinate set for a
   NiTextureEffect's SPHERE_MAP with D3D's camera-space reflection vector - MGE XE's
   emulation (ffeshader.cpp, `texgenReflection(viewpos, normal)` in its vertex shader,
   then the engine's texture matrix) is the reference - and the pipeline interpolates
   that across the triangle. 18dl worked the reflection out per pixel from the
   interpolated normal, which is sharper than the game on every curved pod: Robin, "It
   also seems too sharp [...] and visible from too far away". The matrix that takes the
   reflection to the map is the sphere-map one, u = 0.5 + 0.5 rx, v = 0.5 - 0.5 ry (the
   texture's first row at the top, sky-up), with x and y the camera's right and up;
   the engine sets it at run time and this is the only linear one that makes a sphere
   map of a reflection vector. Shared by the two mesh vertex shaders; the grass writes
   the centre. Worked out only when the batch has a map. (No backticks in here.) */
const ENV_VS_GLSL=`
uniform vec3 uEye; uniform vec3 uCamR, uCamU;
uniform mediump int uHasEnv;   // the fragment shader's default int precision, or the link fails
out vec2 vEnv;
vec2 envCoord(vec3 w, vec3 n){
  if(uHasEnv!=1) return vec2(0.5);
  vec3 rw=reflect(normalize(w-uEye), n);
  return vec2(0.5+0.5*dot(rw,uCamR), 0.5-0.5*dot(rw,uCamU));
}`;

const VS_STATIC=`#version 300 es
precision highp float;
layout(location=0) in vec3 aPos;
layout(location=1) in vec3 aNrm;
layout(location=2) in vec2 aUV;
layout(location=3) in vec4 iR0;   // rows of a 3x4 model matrix
layout(location=4) in vec4 iR1;
layout(location=5) in vec4 iR2;
layout(location=6) in vec4 aCol;   // vertex colour and alpha; white when the shape has none (18df: RGBA)
layout(location=7) in vec2 aUV2;   // the detail map's UV set (round 17h)
/* Round 17w: per instance, the hours this lamp lights and goes out at, or (-1,-1) when the
   instance is no lamp. Disabled on a batch with no lamps in it, where the generic value
   (-1,-1) stands in. */
layout(location=8) in vec2 aLamp;
layout(location=9) in vec2 aUVD;   // the dark map's UV set (round 17y); (0,0) without one
layout(location=10) in vec2 aUVDc; // the decal's UV set (round 18df); (0,0) without one
uniform mat4 uVP;
/* Round 17y: where the shape's animated node stands this frame, in the mesh's own space
   - the identity on the still majority. The geometry of a shape under a
   NiKeyframeController is baked in that node's frame, and this puts it back. */
uniform mat4 uAnim;
out vec2 vUV; out vec2 vUV2; out vec2 vUVD; out vec2 vUVDc; out vec3 vN; out vec3 vW; out vec3 vC; out vec3 vG; out float vH;
out float vCA;   // the vertex alpha (round 18df)
out float vGK;   // no ground tint on a rock, and never was (round 17u)
flat out vec2 vLamp;
${ENV_VS_GLSL}
void main(){
  vC=aCol.rgb; vCA=aCol.a; vG=vec3(1.0); vH=1.0; vGK=0.0; vUV2=aUV2; vUVD=aUVD; vUVDc=aUVDc; vLamp=aLamp;
  vec4 p=uAnim*vec4(aPos,1.0);
  vec3 w=vec3(dot(iR0,p), dot(iR1,p), dot(iR2,p));
  /* The normal through the same rows the position goes through. This used to be
     mat3(iR0.xyz,iR1.xyz,iR2.xyz)*aNrm, and a mat3 built from three vectors takes them
     as *columns* - so every normal went through the transpose, the inverse turn, and a
     rock placed with a yaw was lit from the wrong side (round 17y). */
  vec3 n=mat3(uAnim)*aNrm;
  vN=normalize(vec3(dot(iR0.xyz,n), dot(iR1.xyz,n), dot(iR2.xyz,n)));
  vW=w; vUV=aUV; vEnv=envCoord(w, vN);
  gl_Position=uVP*vec4(w,1.0);
}`;

/* Painted objects, tinted the way painted ground is.
 *
 * Robin asked to see what he has covered before scattering it. The grass already shows
 * where a rule *placed* something; this shows where the paint is, including the parts a
 * rule's slope limit or an empty slot will leave bare — which is the difference between
 * "I have not painted that" and "I painted it and nothing grew".
 *
 * **The strokes, replayed per fragment.** The mask on an object is not a grid (§68 606):
 * it is the dabs themselves, in the object's own space. So the fragment shader is handed
 * those dabs and does what `statics::replay` does — the same falloff, the same "a new
 * colour takes the point when it is at least as strong as what is there, and otherwise
 * only weakens it", the same eraser. Anything less than that would be a second opinion
 * about what is painted, drawn over the first.
 *
 * `vObj` is the vertex in the mesh's own space, which is where the dabs are. Nothing has
 * to be transformed per fragment. */
const VS_STATIC_PAINT=`#version 300 es
precision highp float;
layout(location=0) in vec3 aPos;
layout(location=1) in vec3 aNrm;
layout(location=3) in vec4 iR0;
layout(location=4) in vec4 iR1;
layout(location=5) in vec4 iR2;
uniform mat4 uVP;
out vec3 vObj; out vec3 vN; out vec3 vW;
void main(){
  vec4 p=vec4(aPos,1.0);
  vec3 w=vec3(dot(iR0,p), dot(iR1,p), dot(iR2,p));
  // Through the rows, like the position - see VS_STATIC (round 17y).
  vN=normalize(vec3(dot(iR0.xyz,aNrm), dot(iR1.xyz,aNrm), dot(iR2.xyz,aNrm)));
  vObj=aPos;
  vW=w;
  gl_Position=uVP*vec4(w,1.0);
}`;

/* Was 64, the most a uniform array could hold — and a long stroke on one rock silently
   lost its oldest marks past it. The dabs ride a data texture now (two RGBA32F texels a
   dab, fetched with texelFetch, which needs no extension). The engine's own cap matches.

   The texture used to be a single row, which made 2048 texels — WebGL2's guaranteed
   maximum width — the whole of the ceiling: 1024 dabs and not one more, whatever the
   machine could actually hold. It is laid out in rows of DAB_TEX_W now, so width stops
   being the limit and the ceiling is a number chosen for what a fragment can afford to
   walk rather than for what a texture can be. Measured on Robin's largest mask (9,854
   dabs on a Gnisis cliff): at most 61 of them can reach any one point, so the loop below
   is the thing that decides how high this can go, not this. */
const DAB_TEX_W=2048;
/* Raised from 1,024 once the two things that held it there were dealt with: the texture
   was one row wide, and the fragment walked every mark. Then from 64,000 to match the
   engine's cap the day the cap moved — the two ceilings diverging is precisely the bug
   the `trimmed` counter once hid, an object whose scatter used marks its tint no longer
   drew. One number at both ends: the engine's `TINT_DABS` is `MARKS_CAP` and this is its
   value spelled for the page. What it costs at the top end, per object: two RGBA32F
   texels a mark, about three megabytes, plus the bucket grid — paid only by an object
   actually holding that much. When a mask does pass it the answer still says how many
   marks it left out, and the statistics panel still shows that. */
const MAX_TINT_DABS=128000;
/* The longest shade run one mark can carry: one shade per ray of the brush's fan, with
   headroom. The shader's run scan is bounded by it; the engine lays at most 47. */
const MAX_SHADE_RUN=64;
/* Below this many marks the fragment simply walks them all. A grid costs an index list to
   build, memory to hold and two fetches to enter, and none of that buys anything against
   a list this short — measured on his own small masks, a grid over 385 dabs stores 3,318
   indices to save walking 385. */
const DAB_GRID_MIN=256;
/* Cell size, in median dab radii. Chosen against his largest mask (9,854 dabs on a Gnisis
   cliff, 6,620 units across): at 2 radii it holds 83,608 indices, the longest cell list is
   308 and the mean is 57, where the ungridded loop walks all 9,854. Finer cells shorten
   the lists a little and grow the index list fast — at 1 radius, 217k indices to get the
   longest list from 308 to 219. Sized in radii rather than in cells so a boulder and a
   cliff get the same *shape* of grid, which is what makes one constant fit both. */
const DAB_GRID_CELL_R=2.0;
/* Ceilings on the grid itself, so a pathological mask degrades to a coarser grid rather
   than to a hundred megabytes: no axis beyond this, and no more cells than that. */
const DAB_GRID_AXIS=64;
const DAB_GRID_CELLS=32768;
/* How many colours the painted-object tint can draw at once — round 18bf (§I21).

   It was 24, of which two are the fixed cover colours, so the twenty-third rule colour in
   a palette had nowhere to go: `tintPalette` stopped filling the table and `tintRows` then
   marked those dabs -2, which means "a colour these grass rules no longer have". So the
   paint was on the ground and not on the rock, with the wrong explanation on screen. 64 is
   past any palette a person builds by hand — 62 rule colours — and is 64 of the 224 vec4
   uniform slots GLES3 guarantees a fragment shader. `18_cellpreview.js` reads this rather
   than declaring its own copy of the number, which is how the two came to be one edit
   apart in the first place. */
const MAX_TINT_PAL=64;
const FS_STATIC_PAINT=`#version 300 es
precision highp float;
in vec3 vObj; in vec3 vN; in vec3 vW;
uniform sampler2D uDabs;   // texel 2i: centre.xyz, radius; texel 2i+1: feather, strength,
                           // palette index, and which plane/layer laid it: 0..n is a rule
                           // layer, -1 the cover plane ("no grass" / "grass anyway")
uniform vec3 uPal[${MAX_TINT_PAL}];
uniform int  uCount;
/* The bucket grid over the marks, and where it sits in object space. uGridCells is 0 when
   there is no grid, which is the small-mask case and the only branch this shader takes on
   a uniform. uGridBase is where the index lists start, past the cell offsets. */
uniform sampler2D uGrid;
uniform vec3 uGridLo;
uniform vec3 uGridInv;
uniform ivec3 uGridN;
uniform int  uGridBase;
uniform int  uGridCells;
uniform vec3 uSun;
uniform float uAlpha;
uniform vec3 uFogCol;
uniform float uFogK;
${SKY_GLSL}
uniform float uBright;
out vec4 o;
/* Planes, plural, and they compose.
 *
 * The painted ground keeps its rule layers beside each other and its cover paint over the
 * top of them, and all of it shows at once: two layers over one spot both draw, and "no
 * grass" laid across a colour shows as both. An object's mask says the same thing now —
 * it carries a plane per dab — so this replays each plane on its own and blends the
 * winners, instead of one stream of dabs where the last stroke took the picture.
 *
 * Four layers' worth of accumulators. Somebody with five gets the fifth folded into the
 * fourth, which is a tint that is slightly wrong rather than a tint that is missing, and
 * the number can go up the day anyone has five. (No backticks in here: template literal.)
 */
const int NL=4;
/* Which layers are showing, by index — the eye toggles in the layer list. 1 shows,
   0 hides. Hiding is a fact about the *picture* only: the marks still replay, the
   contest still runs, and the scatter is untouched — the same meaning the toggle has
   for the painted ground. */
uniform float uLayerOn[NL];
float held[NL]; int which[NL];
float cHeld; int cWhich;

/* One entry of the bucket grid: starts first, then the index lists. Held as floats
   because a float carries every integer up to 2^24 exactly and this way there is one
   texture format in this shader instead of two. */
int gridAt(int j){
  return int(texelFetch(uGrid, ivec2(j % ${DAB_TEX_W}, j / ${DAB_TEX_W}), 0).r);
}

/* One texel pair of one mark. */
vec4 rowA(int i){ int t=i*2;   return texelFetch(uDabs, ivec2(t % ${DAB_TEX_W}, t / ${DAB_TEX_W}), 0); }
vec4 rowB(int i){ int t=i*2+1; return texelFetch(uDabs, ivec2(t % ${DAB_TEX_W}, t / ${DAB_TEX_W}), 0); }
/* A mark's reach at this fragment — centre, radius, feather, strength, nothing else. */
float reachAt(vec4 A, vec4 B){
    vec3  c=A.xyz;
    float r=max(A.w,1e-6);
    float d=length(vObj-c);
    if(d>=r) return 0.0;
    float inner=r*(1.0-clamp(B.x,0.0,1.0));
    float w=1.0;
    if(d>inner && inner<r){
      float t=clamp((r-d)/(r-inner),0.0,1.0);
      w=t*t*(3.0-2.0*t);
    }
    return B.y*w;
}
/* One mark, laid on the accumulators. Lifted out of the loop so the grid and the plain
   walk hand it the same marks in the same order — the order is the law here ("the last
   stroke wins"), and a second copy of this body is a second law. */
void applyDab(int i){
    vec4 A=rowA(i), B=rowB(i);
    /* -4 is a shade mark: not ink, but a bite out of the mark before it. It acts
       through the run scan below and never on its own — the engine's reach_run,
       said in GLSL. */
    if(B.w<-3.5) return;
    float a=reachAt(A,B);
    if(a<=0.0) return;
    /* The shade run riding this mark: the marks immediately after it flagged -4, each
       taking its bite where the stroke's eye could not see this surface. Bounded by the
       fan's own size — a run is at most one mark per ray. */
    for(int j=1;j<${MAX_SHADE_RUN};j++){
      if(i+j>=uCount) break;
      vec4 SB=rowB(i+j);
      if(SB.w>-3.5) break;
      a*=1.0-reachAt(rowA(i+j),SB);
      if(a<=0.0) return;
    }
    int slot=int(B.z);
    /* Below -1 is a colour these grass rules do not have. Skipped rather than drawn or
       erased: the paint is still yours and is waiting for that rule to come back, and
       showing it in somebody else's colour would be worse than not showing it.
       -1 is the eraser, and it only ever weakens — the same law the engine replays, so a
       patch you rubbed out shows as rubbed out rather than as never painted. */
    if(slot<-1) return;
    /* B.w is the mark's draw layer, resolved by the engine (see dab_bytes): a rule
       layer's index, -1 the cover plane, -2 a rule eraser over every layer, -3 a mark
       whose layer no longer exists. The engine says because the engine knows: the page
       used to look layers up by slot, and an eraser has no slot — so every rule eraser
       dimmed layer 0 in this picture, whatever it actually erased. */
    if(B.w<-2.5) return;
    if(B.w<-1.5){
      // A rule eraser over every layer — what every eraser was before masks wrote
      // down which layer they were rubbing out, and what old masks still mean.
      if(slot>=0) return;
      for(int k=0;k<NL;k++){
        held[k]=min(held[k],1.0-a);
        if(held[k]<=0.0){ held[k]=0.0; which[k]=-1; }
      }
    }else if(B.w<-0.5){
      // The cover plane: one answer, its own eraser, over the top of every layer.
      if(slot<0)            cHeld=min(cHeld,1.0-a);
      else if(cWhich==slot) cHeld=max(cHeld,a);
      else if(a>=cHeld){ cWhich=slot; cHeld=a; }
      else                  cHeld=min(cHeld,1.0-a);
      if(cHeld<=0.0){ cHeld=0.0; cWhich=-1; }
    }else{
      int L=clamp(int(B.w),0,NL-1);
      if(slot<0)                 held[L]=min(held[L],1.0-a);
      else if(which[L]==slot)    held[L]=max(held[L],a);
      else if(a>=held[L]){ which[L]=slot; held[L]=a; }
      else                       held[L]=min(held[L],1.0-a);
      if(held[L]<=0.0){ held[L]=0.0; which[L]=-1; }
    }
}

void main(){
  for(int k=0;k<NL;k++){ held[k]=0.0; which[k]=-1; }
  cHeld=0.0; cWhich=-1;
  if(uGridCells<=0){
    /* Few enough marks that finding the near ones costs more than testing them all. */
    for(int i=0;i<uCount;i++) applyDab(i);
  }else{
    /* The marks whose box covers this point, and nothing else. A point outside the grid
       clamps into the edge cell and is then turned away by the exact distance test in
       applyDab, exactly as it would have been by the full walk — the grid only ever skips
       marks that could not have reached here, so the picture is the same picture. */
    vec3 g=(vObj-uGridLo)*uGridInv;
    ivec3 gi=clamp(ivec3(floor(g)), ivec3(0), uGridN-ivec3(1));
    int cell=(gi.z*uGridN.y+gi.y)*uGridN.x+gi.x;
    int s=gridAt(cell), e=gridAt(cell+1);
    for(int k=s;k<e;k++) applyDab(gridAt(uGridBase+k));
  }
  vec3 acc=vec3(0.0); float sum=0.0;
  for(int k=0;k<NL;k++){
    if(which[k]>=0 && held[k]>0.002 && uLayerOn[k]>0.5){
      acc+=uPal[clamp(which[k],0,${MAX_TINT_PAL}-1)]*held[k];
      sum+=held[k];
    }
  }
  if(cWhich>=0 && cHeld>0.002){ acc+=uPal[clamp(cWhich,0,${MAX_TINT_PAL}-1)]*cHeld; sum+=cHeld; }
  if(sum<=0.002) discard;
  vec3 col=acc/sum;
  float ink=min(sum,1.0);
  /* Shaded and fogged with the terrain tint's own numbers — the same lambert curve, the
     same brightness, the same exponential haze — because Robin looked at the two side by
     side and saw two different paints. There is one paint; the drawings have to agree. */
  /* Lit like the ground's own paint, and never brighter than the colour itself. The
     terrain mixes its swatch *into* a texture, so it cannot pass the colour; this
     multiplies, and 0.40 + 0.85 reaches 1.25 — which for a colour already at full red
     (the No grass tool's terracotta) clips the other channels up towards it and comes out
     white on any face turned towards the sun. Robin: "The No Grass rule is painted with a
     white color when the face it's painted on is angled upwards." Capped at 1: shading
     still darkens a face turned away, and no face is drawn as a colour nobody chose. */
  float lam=min(0.40+0.85*max(dot(normalize(vN),normalize(uSun)),0.0), 1.0);
  if(uNoLight==1) lam=1.0;   // the Unlit switch (round 18f)
  vec3 c=col*lam*uBright;
  c=airFog(c,vW);
  o=vec4(c, uAlpha*ink);
}`;

/* The sky — round 12 item 4. A full-screen triangle; each pixel's view ray is rebuilt
   from the camera's own axes and looked up in the baked sky map, with the sun drawn as
   a small disc and glow. Below the horizon the horizon's colour darkens off, which is
   what shows under the slab when the camera dips beneath the ground. */
const VS_SKY=`#version 300 es
precision highp float;
out vec2 vP;
void main(){
  // One triangle covering the screen: ids 0,1,2 -> (-1,-1) (3,-1) (-1,3).
  float x = gl_VertexID==1 ? 3.0 : -1.0;
  float y = gl_VertexID==2 ? 3.0 : -1.0;
  vP=vec2(x,y);
  gl_Position=vec4(x,y,0.999999,1.0);
}`;
const FS_SKY=`#version 300 es
precision highp float;
in vec2 vP;
uniform vec3 uCamF, uCamR, uCamU;   // forward, right, up
uniform float uTanH, uAspect;
uniform vec3 uSunDir, uSunDiscCol;
uniform float uSunVis;
uniform vec3 uFogCol; uniform float uFogK;   // unused here; the snippet names them
/* The weather's clouds: the game's own sheet, laid flat over the sky and drifting,
   in the fog colour the way the game tints them. */
uniform sampler2D uCloud;
uniform int uHasCloud;
uniform float uCloudDrift;
/* Round 15 item 2: the game's own star sheet (tx_stars.dds, what sky_night_01.nif
   wears), tiled over the dome and faded in with the night; the clouds draw over it. */
uniform sampler2D uStars;
uniform int uHasStars;
uniform float uStarK;
/* Round 17s: and how far round the sheet has turned. Robin: "If we can get a bit movement
   on the stars in the sky depending on time (so it rotates like a nightsky does over time)
   [...] A simplified solution is good here, not anything physically accurate."
   So: one turn a day, about the zenith rather than about a tilted celestial pole. Turning
   about the zenith leaves d.z alone, which means the thinning towards the horizon below is
   untouched and the sheet cannot slide up out of the sky - a real pole would need the
   latitude Morrowind does not have. Fifteen degrees a game hour, which is the real rate
   and slow enough that at the default speed of the clock it reads as drift.
   (No backticks in here: template literal.) */
uniform float uStarRot;
${SKY_GLSL}
out vec4 o;
void main(){
  vec3 d=normalize(uCamF + uCamR*(vP.x*uTanH*uAspect) + uCamU*(vP.y*uTanH));
  vec3 c=fogColourSky(d).rgb;
  /* Wraithguard: MGE XE's ordered dither over the sky's gradient (XE Mod Sky.fx, SkyPS:
     ditherSky[vpos.x % 4][vpos.y % 4]), so a dusk or night sky does not band into steps
     in an 8-bit frame. Its numbers, a few thousandths either way. */
  { const float DITHER_SKY[16]=float[16](0.001176, 0.001961, -0.001176, -0.001699, -0.000654, -0.000915, 0.000392, 0.000131,
                                         -0.000131, -0.001961, 0.000654, 0.000915, 0.001699, 0.001438, -0.000392, -0.001438);
    ivec2 px=ivec2(gl_FragCoord.xy) % 4;
    c+=DITHER_SKY[px.x*4+px.y]; }
  /* Round 18dv: under the water that is the water's colour, and nothing the sky carries
     survives the whole depth of it - no sun disc, no glare, no clouds, no stars. In the
     game the water plane covers the hemisphere and its refraction has faded to the fog
     colour within a few hundred units; what light does come through the surface comes
     through the surface shader (its sun-refraction term), not from the sky behind it. */
  if(uUnder==1 && uUnderTop<=0.0){ o=vec4(c,1.0); return; }
  // Under the horizon - only ever seen from below the slab - the horizon colour, darkening.
  if(d.z<0.0) c*=mix(1.0,0.35,clamp(-d.z*6.0,0.0,1.0));
  if(uHasStars==1 && uStarK>0.0 && d.z>0.0){
    float ca=cos(uStarRot), sa=sin(uStarRot);
    vec2 dr=vec2(d.x*ca - d.y*sa, d.x*sa + d.y*ca);
    vec2 suv=dr/(d.z+0.35)*1.1;
    vec3 st=texture(uStars,suv).rgb;
    // Thinning to the horizon, where the haze is; the sheet's own black is the gaps.
    c+=st*uStarK*smoothstep(0.0,0.25,d.z);
  }
  if(uHasCloud==1 && d.z>0.0){
    /* The game's own cloud dome, sky_clouds_01.nif, measured (round 17x): a shallow cap
       2000 units across, 307 high at the zenith and 100 at its rim, with the texture laid
       flat over it five times across - u = 2.5 + x/400, v = -1.5 - y/400. The rim stands
       5.7 degrees above the horizon; below that the game shows its edge. The point the eye's
       ray meets the dome is looked up by elevation from the rings the mesh is built of, and
       the clouds scroll along the dome's v, which is world north, at OpenMW's rate
       (SkyManager::update: timer += dt * Cloud Speed * 0.003). So the clouds are the size
       the game draws them and cross the sky in the time it takes them there. */
    float el=asin(clamp(d.z,0.0,1.0));
    float r;
    if(el>0.8098)      r=mix(275.0,   0.0, (el-0.8098)/(1.5708-0.8098));
    else if(el>0.5045) r=mix(460.0, 275.0, (el-0.5045)/(0.8098-0.5045));
    else if(el>0.2827) r=mix(688.0, 460.0, (el-0.2827)/(0.5045-0.2827));
    else if(el>0.0997) r=mix(1000.0,688.0, (el-0.0997)/(0.2827-0.0997));
    else               r=1000.0+ (0.0997-el)*3100.0;
    vec2 xy=normalize(d.xy+vec2(1e-6,0.0))*r;
    vec2 uv=vec2(2.5 + xy.x/400.0, -1.5 - xy.y/400.0 + uCloudDrift);
    vec4 cl=texture(uCloud,uv);
    float fade=smoothstep(0.0,0.10,d.z);
    c=mix(c, cl.rgb*uFogColFar, cl.a*fade);
  }
  /* The sun: a disc about a degree and a half across in the disc's colour, and the
     glare the game draws round it, both gone with uSunVis at night. */
  float mu=dot(d,uSunDir);
  float disc=smoothstep(0.99955,0.9998,mu);
  float glare=pow(max(mu,0.0),160.0)*0.55 + pow(max(mu,0.0),24.0)*0.12;
  c+=uSunVis*(uSunDiscCol*disc*1.3 + uSunDiscCol*glare);
  o=vec4(c,1.0);
}`;

/* =====================================================================================
   THE ORBIT COLOUR — round 18cj.

   Robin: "The orbit pivot, and the navigation mode button's orbit state color in the
   viewport should also have a color assigned to the theme."

   The pivot ball, its three rings, its centre marker and the ring it stamps on the ground
   are one colour with the viewport's Orbit button, and that colour is now the theme's
   `--orbit` (`01_head.html`) rather than `--hot`, which is the "no grass" paint and was
   never meant to be borrowed.

   Read from the stylesheet rather than duplicated here, so there is one place to change it
   and the renderer cannot drift from the button. Cached, because this is called inside the
   draw: `Themes.apply` clears it (`orbitCol.forget()`) when the window changes theme, which
   is the only time the answer can move. A window with no `--orbit` — an old page opened on
   its own — falls back to the salmon the pivot has always been.
   ===================================================================================== */
let _orbitCol=null;
function orbitCol(){
  if(_orbitCol) return _orbitCol;
  let c=null;
  try{
    const v=getComputedStyle(document.body).getPropertyValue('--orbit').trim();
    const m=/^#([0-9a-f]{6})$/i.exec(v);
    if(m) c=[parseInt(m[1].slice(0,2),16)/255, parseInt(m[1].slice(2,4),16)/255,
             parseInt(m[1].slice(4,6),16)/255];
  }catch(e){ /* no document (a worker, a test harness): the fallback below */ }
  _orbitCol = c || [1.0,0.627,0.471];
  return _orbitCol;
}
orbitCol.forget=()=>{ _orbitCol=null; };

/* =====================================================================================
   Round 18dd: the exterior's backdrop, with the atmosphere off, is the theme's.

   Robin: "The dark background color in the viewport when Atmosphere is turned off, feels
   like a warm gray. I think maybe it would be good to adapt it to the different themes a
   little bit. I think especially the Firmament's cool blacks make the viewport background
   too warm in contrast. Let the Gardenfell Green one be the same as it is now, and same
   for Of Ash and Blight, Redoran Bone and Hlaalu Clay. Adapt it a little for Telvanni Bio
   and The Firmament. I only want the exterior cell backdrop gray in the viewport when the
   atmosphere is toggled off to be themed. All other settings for the viewport should
   always be the same no matter what theme is active. Especially I want atmosphere, and
   interiors (where the background color is the ambient light of the scene) to stay as
   they are. I don't want the background color change with themes in the exterior to
   change the ambient light of the scene in any way from what it is now." And then:
   "Important to read the backdrop of the viewport as darker than the UI though, also for
   the Firmament."

   The colour is the theme's `--vp-back`, read from the stylesheet the way `--orbit` is
   (`orbitCol` above): once, cached, dropped by `Themes.apply` when the window changes
   theme. A theme that sets none gets `BACK_LEGACY` - the renderer's own numbers, not a
   hex rounded to them, so the four palettes that keep the old grey keep it to the bit;
   only Telvanni Bio and The Firmament set one (`01_head.html`).

   What follows the backdrop, because it *is* the backdrop: the frame's clear, and the
   flat distance fog every surface fades into when there is no sky (`airFog`'s second
   branch, `uFogCol`) - far ground fading to a warm grey over a cool backdrop would draw
   the horizon as a seam - and the water's stand-in sky and far-fog colours when there is
   no sky, which its depth colour is made from and its reflection clears to.

   What does not: the atmosphere (its own `now.fog`, and `uFogCol` is never read under it),
   a room (its record's fog, on every path - see `backdrop()`), the ambient and the sun
   (`uAmbCol`/`uSunCol`, never derived from here), and the two dims that are the
   viewport's own whatever is drawn: `uDimCol` behind a highlight and the arrows' body
   behind terrain, which keep `fogCol`'s grey.
   ===================================================================================== */
const BACK_LEGACY=[0.082,0.082,0.078];
let _backCol=null;
function backCol(){
  if(_backCol) return _backCol;
  let c=null;
  try{
    const v=getComputedStyle(document.body).getPropertyValue('--vp-back').trim();
    const m=/^#([0-9a-f]{6})$/i.exec(v);
    if(m) c=[parseInt(m[1].slice(0,2),16)/255, parseInt(m[1].slice(2,4),16)/255,
             parseInt(m[1].slice(4,6),16)/255];
  }catch(e){ /* no document (a worker, a test harness): the legacy grey */ }
  _backCol = c || BACK_LEGACY;
  return _backCol;
}
backCol.forget=()=>{ _backCol=null; };

/* Round 17i: the orbit pivot, while Ctrl is held. Robin: "visualize where the pivot is
   as an mostly invisible ball, but with outlines around the equator, and from north to
   south of the ball along each cardinal direction." Three rings — the equator and two
   meridians — drawn twice: once depth-tested and once through whatever stands in front
   of it, fainter, so the pivot is never lost inside a hill.

   Round 17j made those rings solid bands with a cross-section; 17m took that back out
   (Robin: "revert back to the previous version of just lines") and drew them as quads
   expanded in screen space, to get a thickness WebGL's own lineWidth does not give.
   17m again, after seeing it: "Make it into just 1px line again if it's the only thing
   supported natively." So it is plain GL_LINES at the one width every desktop driver
   actually honours, and the shader is back to a line's worth of work.
   (No backticks in here: this is inside a template literal.) */
const VS_PIVOT=`#version 300 es
precision highp float;
layout(location=0) in vec3 aPos;
uniform mat4 uVP; uniform vec3 uC; uniform float uR;
out vec3 vW;
void main(){ vW=uC+aPos*uR; gl_Position=uVP*vec4(vW,1.0); }`;
const FS_PIVOT=`#version 300 es
precision highp float;
in vec3 vW;
uniform vec4 uCol;
uniform vec3 uFogCol; uniform float uFogK;
${SKY_GLSL}
out vec4 o;
void main(){
  /* The haze, as on everything else: under the atmosphere a pivot a cell away is as far
     away as the ground beside it (Robin: "Make the ball be affected by fog when in
     atmosphere mode"). No shading term — a line has no facing to shade, and the one the
     bands wore is what made them read as a lumpy solid. */
  vec3 eyeVec=vW-uEye; float dist=length(eyeVec); eyeVec/=max(dist,0.001);
  vec4 fog = (uScat==1 || uUnder==1) ? fogColour(eyeVec,dist)
           : vec4(uFogCol*clamp(1.0-exp(-uFogK*length(vW.xy)),0.0,1.0), exp(-uFogK*length(vW.xy)));
  o=vec4(uCol.rgb*fog.a+fog.rgb, uCol.a);
}`;

const VS_FLAT=`#version 300 es
precision highp float;
layout(location=0) in vec3 aPos;
layout(location=3) in vec3 iPos;
layout(location=4) in vec3 iRSA;
uniform mat4 uVP;
void main(){ gl_Position=uVP*vec4(aPos*iRSA.y+iPos,1.0); }`;
const FS_FLAT=`#version 300 es
precision highp float;
uniform vec3 uCol; out vec4 o;
void main(){ o=vec4(uCol,1.0); }`;

/* The cell-stepping arrows. Their own pair rather than a reuse of the flat one,
   because a solid shape lit by nothing reads as a hole in the picture rather than as an
   object; two terms of directional shading are enough to make a prism look like a prism.
   No fog: an arrow is a control, and a control that fades into the distance is a control
   somebody cannot find. */
/* `uSwell` pushes every vertex out along its own normal, which turns the same geometry
   into a slightly larger copy of itself — the rim drawn behind the arrow so the occluded
   pass has a bright edge. The prism already carries outward normals on its sides and
   up or down on its caps, so this is the shape it wants, rather than a scale about a
   centre the arrow does not have. `uFlat` drops the shading for that rim: a silhouette
   reads as an outline, a shaded one reads as a second arrow. */
const VS_ARROW=`#version 300 es
precision highp float;
layout(location=0) in vec3 aPos;
layout(location=1) in vec3 aNrm;
uniform mat4 uVP;
uniform float uSwell;
out vec3 vN;
void main(){ vN=aNrm; gl_Position=uVP*vec4(aPos+normalize(aNrm)*uSwell,1.0); }`;
const FS_ARROW=`#version 300 es
precision highp float;
in vec3 vN;
uniform vec3 uCol;
uniform float uFlat;
out vec4 o;
void main(){
  vec3 n=normalize(vN);
  float k=0.55+0.45*clamp(dot(n,normalize(vec3(0.35,0.25,0.90))),0.0,1.0);
  o=vec4(uCol*mix(k,1.0,uFlat),1.0);
}`;

// One flat sheet, no instancing: the water surface.
const VS_WATER=`#version 300 es
precision highp float;
layout(location=0) in vec3 aPos;
uniform mat4 uVP;
out vec3 vW;
void main(){ vW=aPos; gl_Position=uVP*vec4(aPos,1.0); }`;
const FS_WATER=`#version 300 es
precision highp float;
in vec3 vW;
uniform vec3 uCol, uFogCol;
uniform float uFogK, uAlpha;
out vec4 o;
void main(){
  /* Plain exponential in distance. A squared ramp was tried — clearer up close, thicker
     far off — and read as too much fog at every distance a person actually looks from,
     so it went back. */
  float fog=1.0-exp(-uFogK*length(vW.xy));
  o=vec4(mix(uCol,uFogCol,clamp(fog,0.0,1.0)), uAlpha);
}`;

/** "#rrggbb" as the three floats the shader wants. Anything unreadable comes back mid
 *  grey rather than as an error: a colour is a label, and a swatch with a typo in it
 *  should still be visible enough to fix. */
function hexToRgb(hex){
  const m=/^#?([0-9a-f]{6})$/i.exec(String(hex||''));
  if(!m) return [0.55,0.55,0.55];
  const n=parseInt(m[1],16);
  return [((n>>16)&255)/255, ((n>>8)&255)/255, (n&255)/255];
}

/* What a shape's NiAlphaProperty asks for, as the two numbers the shader tests with:
   `{fn, ref}` — fn the compare function (-1 for none, else the record's own 0..7) and
   ref the threshold as 0..1.
   Round 11 item 9, from Vurt_FrnDrk.nif. Its shapes carry flags 0x12ed and 0x12ec:
   alpha *testing* on (bit 9), function GREATER (bits 10-12 = 4), and thresholds of 0
   and 100. The old rule, "discard below the threshold, and zero means no test", read
   the first as "keep everything" and drew the fern as an opaque sheet with the texture's
   clear parts painted black. GREATER against 0 is "anything but fully clear survives",
   which is a cutout - so the function is honoured as written.
   Two judgement calls on top of the record:
   - **Blending** (bit 0) is not done here. The scene is drawn in one pass with depth
     writes on, and unsorted blending draws the far leaf over the near one; every
     blended shape is drawn as a cutout instead. When blend is on and the test as
     written would keep nearly every texel (no test, ALWAYS, or GREATER/GEQUAL against a
     threshold below 0.34), the cut is taken at 0.34 - the number the grass pass always
     used - so the soft edge the game would have faded is cut where the eye expects the
     edge to be rather than drawn as a solid halo.
   - **No alpha property at all** means the game draws the shape opaque, texture alpha
     and all, and so does this. The grass pass used to cut at 0.34 regardless; the mesh
     editor never cut. Now both do what the mesh says. */
/* Round 17h: what the NiAlphaProperty asks for when it asks for *blending*, and the
   material alpha that goes with it. Bit 0 is "blending enabled"; the source and
   destination factors are four bits each after it, numbered in NiAlphaProperty's own
   order. Only the ordinary translucent pair is honoured - the water meshes Robin named
   (OAAB's `water_rectf256_01.nif`, CAP's `nb_ex_vivec_p_water_01.nif`, the water inside
   `tr_ex_hla_fountain.nif`) are all SRC_ALPHA / ONE_MINUS_SRC_ALPHA with a material
   alpha under 1, which is the common thread: a shape that says it is see-through.
   Additive and the other exotic pairs are left opaque rather than guessed at. */
const NIF_BLEND_FACTOR=['ONE','ZERO','SRC_COLOR','ONE_MINUS_SRC_COLOR','DST_COLOR','ONE_MINUS_DST_COLOR',
                        'SRC_ALPHA','ONE_MINUS_SRC_ALPHA','DST_ALPHA','ONE_MINUS_DST_ALPHA','SRC_ALPHA_SATURATE'];
/** Round 18df: a part's vertex colours as RGBA bytes - its RGB with the vertex alpha
 *  (`colA`, from the mesh payload's tail) beside each, 255 where the shape has no
 *  colours or every vertex is at 1 (all of vanilla). What every colour stream carries
 *  now, so the shader can take the fragment's alpha from the vertex where the fixed
 *  pipeline does. Remembered on the part: the merged builder asks once per rebuild. */
function rgba4(part){
  const nv=part.pos.length/3|0;
  const c=part.col, a=part.colA;
  const ok=!!(c && c.length===nv*3);
  if(part._rgba && part._rgba.length===nv*4 && part._rgbaOf===c && part._rgbaA===a) return part._rgba;
  const out=new Uint8Array(nv*4);
  if(!ok){ out.fill(255); }
  else{
    const hasA=!!(a && a.length===nv);
    for(let i=0,j=0;i<nv;i++,j+=3){ out[i*4]=c[j]; out[i*4+1]=c[j+1]; out[i*4+2]=c[j+2]; out[i*4+3]=hasA? a[i] : 255; }
  }
  part._rgba=out; part._rgbaOf=c; part._rgbaA=a;
  return out;
}
function alphaBlend(part){
  const f=(part&&part.alphaFlags)|0;
  if(!(f&1)) return null;
  const src=NIF_BLEND_FACTOR[(f>>1)&15], dst=NIF_BLEND_FACTOR[(f>>5)&15];
  /* Round 17y: additive too - SRC_ALPHA over ONE, and ONE over ONE. The blight's sheets
     (`f\active_blight_large.nif`) are the first pair, and an additive sheet is the one
     kind of translucency that needs no sorting at all: light added is light added in any
     order. Drawn as a cutout it was "a still cut out thing in the world" - the cut took
     the smoke's soft alpha off and left the solid middle. */
  const additive=(dst==='ONE' && (src==='SRC_ALPHA' || src==='ONE'));
  /* 18dj: every other pair as the file says it, now that the pass sorts and the sheets
     write no depth - the "More wells" Telvanni wellpod's water is SRC_ALPHA over
     DST_COLOR (0x008d) and was drawn as an opaque cutout for want of a rule. The one
     factor the card refuses as a destination is SRC_ALPHA_SATURATE. */
  if(!src || !dst || dst==='SRC_ALPHA_SATURATE') return null;
  const a=(part&&part.matAlpha!=null)? +part.matAlpha : 1;
  /* Round 18dg: NoSorter (bit 13) with a test - the Bloodmoon trees, the holly, the
     colony houses, 1,007 vanilla shapes at a test of 192 or more. The game keeps these
     out of its far-to-near sort and draws them among the solid things, blending and
     testing, depth written; so does the viewport now (`nosort` puts the group in the
     solid pass with its blend on). Before this they were cut at their own threshold when
     their texture was a stencil and sorted with the foliage when it was a gradient. */
  if((f&0x2000) && (f&0x200)) return {src, dst, alpha:a, nosort:true};
  /* A material whose alpha is animated is see-through by definition: it is on its way
     from clear to something and back (round 17y). The alpha here is only the resting
     value; `_meshTex` reads the keys each frame. */
  if(part && part.anim && part.anim.alpha) return {src, dst, alpha:a, additive};
  if(additive) return {src, dst, alpha:a, additive};
  /* The material says it is see-through: blend it, whatever the texture holds (round
     17h — OAAB's pools and fountains at 0.7). */
  if(a<0.999) return {src, dst, alpha:a};
  /* Round 17n: and so does a shape whose *texture* carries a real gradient.
     17h's rule was "material alpha under 1, or else cut", on the grounds that blending
     foliage sorts badly for no gain. That is right for a stencil and wrong for a
     gradient, and `alphaCut` below then cut every gradient at 0.34 — which is what
     turned Vivec's waterfalls into a hard patchwork (Robin: "a geometric pattern of
     darker and brighter areas where it shouldn't be") and left the Ghostfence solid.
     The engine classifies the alpha channel at full resolution, so the question is
     answered from what the texture actually is rather than guessed from the flags:
     across Morrowind's own meshes, 1533 shapes are gradients and 506 are stencils, and
     only the gradients change. */
  if(part && part.texAlpha==='soft') return {src, dst, alpha:1};
  return null;
}

/* The pivot ball's radius (round 17i). Robin: "The balls size should be roughly 1.5
   times the size of the brush ball's smallest size (don't connect these values, just use
   the brush ball size as a reference)". The brush's smallest is a 16-unit radius
   (`BrushSize.MIN`), so this started at 24 — written out here rather than read from the
   brush, which is a different control that may move on its own. Round 17m: 64, on Robin's
   word ("Make the size of the orbit navigation ball bigger, say 64 units instead"), now
   that it takes the perspective and shrinks into the distance like everything else.

   It is a number of *world units*, so the ball shrinks with distance like anything else
   in the scene. Round 17m tried making it hold a constant apparent size, gizmo-fashion,
   to answer "the ball gets invisible when it should be visible" — and that was the wrong
   read of the complaint. Robin: "The pivot origin ball should scale as any other 3D
   object. If it doesn't get smaller in view the further away it is, it's useless to
   determine where it is in space." Quite right: a marker whose size carries no
   information is a marker that cannot say how far away it is. The real fix for the
   invisibility was the near plane (see the Ctrl-pull's floor in the wheel handler). */
const PIVOT_R=64;
/* How much of the camera's glide may be thrown away when it lands on its goal, in
   pixels (round 17m). An exponential ease never quite arrives, so its tail is snapped;
   the snap is only invisible if it is smaller than a pixel, and the old thresholds were
   fractions of the orbit distance, which at 6,000 units came to most of one. Five
   channels settle at once, so the worst case is a few times this — measured at 0.05px,
   against 0.86 before. */
const SETTLE_PX=0.02;
/* The clipping planes, and they are constants (round 17m).
 *
 * Robin: "Zooming in a lot still moves the far clipping plane, and it shouldn't. Zooming
 * now should only mean 'move the camera closer to the pivot'. The near and far clipping
 * planes should be constant." They used to be `max(4, viewDist*0.004)` and
 * `viewDist*14 + 20000`, which is the usual trick for keeping depth precision across any
 * zoom — and it meant that zooming in far enough pulled the far plane in to 40,000 units
 * and the distant half of a cell simply stopped being drawn, which is what he
 * photographed.
 *
 * So they are fixed. `GL_FAR` covers the furthest the camera can be wound out (60,000)
 * plus the widest scene it can be looking at, with room over. `GL_NEAR` is the price:
 * with a 24-bit depth buffer the resolution at distance z is about z²/(near·2^24), so 16
 * units gives 0.4 of a unit at 10,000 away and 37 at 100,000 — coarser than the old
 * planes were at a middling zoom, finer than they were when zoomed right in, and well
 * inside what a heightfield and the polygon offsets need. Every pass has to use the same
 * pair or the depth buffer means one thing to the scene and another to whoever reads it
 * back (the water does), so they are read through `nearFar()` and never rebuilt. */
const GL_NEAR=16, GL_FAR=400000;
/* Round 17m: the hours the cell's lamps burn. Robin picked them: "between 18 in the
   evening and 7 in the morning". */
/* Round 17s: the window a lamp may light in, and the window it may go out in. Robin:
   "make the exact time a light start stagger a bit, with up to one in world hours
   difference, but no earlier than 18.00. Same for turning off, but no later than 6.30."
   `lightsLit` uses the outer bounds — is it the part of the day when any lamp burns —
   and `lampLit` picks each lamp's own hour inside them. */
const LAMP_ON_FIRST=18, LAMP_ON_LAST=19;
const LAMP_OFF_FIRST=5.5, LAMP_OFF_LAST=6.5;
/* How many lamps one frame may draw by.
   This used to be bounded by the shader's uniform array, and every fragment paid for every
   lamp in it: at 49 cells, 32 lamps cost 63 ms a frame and 128 cost 196 (PERFORMANCE.md).
   Round 17r put the lamps in a texture and binned them into columns, so a fragment loops
   only the lamps whose reach covers it. The scene budget is therefore no longer what a
   fragment costs, and can be generous; what a fragment costs is GDN_BIN_MAX, the per-
   column cap in the shader. */
const LIGHT_BUDGET=512;
/* How wide a column of the light grid is, in world units. A Morrowind cell is 8192, and a
   lamp's reach is a few hundred, so 1024 puts a handful of lamps in a column and keeps the
   grid itself small: nine cells is a 24x24 grid, forty-nine is 56x56. */
const LIGHT_BIN=1024, LIGHT_BIN_MAX=32, LIGHT_GRID_MAX=96;
/* How far above the budget's cut a lamp has to shine to be sent at full strength. See
   `_lightUniforms`: the lamp that did not fit is the zero point, one shining this many
   times as brightly is full, and everything between is faded across that band. */
const LIGHT_FADE_K=1.5;
/* OpenMW's `quickstep` — a smoothstep by another name, and the curve its own light fade
   uses. Here it shapes the budget's edge (`_lightUniforms`); it used to be in the shader
   too, until round 17o took the distance cutoff it was hiding out of `gdnLights`. */
function quickstep(x){ x=Math.max(0,Math.min(1,x)); x=1-x*x; return 1-x*x; }

/* Round 18i: a light's (constant, linear, quadratic) from `Morrowind.ini
   [LightAttenuation]` - the engine's own NiPointLight::setAttenuationForRadius, as MWSE
   decompiles it and OpenMW's configureLight reproduces it (gdn_core::renderer has the
   same function and the tests against Robin's MWSE readings). `att` is the engine's
   `renderer.attenuation`; null means the game's shipped values, linear 3/r. */
const LAMP_ATT_VANILLA={useConstant:false, constantValue:0, useLinear:true, linearMethod:1, linearValue:3,
                        linearRadiusMult:1, useQuadratic:false, quadraticMethod:2, quadraticValue:16,
                        quadraticRadiusMult:1, outQuadInLin:false};
function lampCoefficients(att, radius, interior){
  const a=(att && att.useLinear!=null)? att : LAMP_ATT_VANILLA;
  const r=Math.max(16, +radius||0);
  let c=0, l=0, q=0;
  if(a.useConstant) c=+a.constantValue||0;
  if(a.useLinear || (interior && a.outQuadInLin)){
    const rr=r*(+a.linearRadiusMult||0); l=0.01;
    const m=+a.linearMethod||0, v=+a.linearValue||0;
    if(m===0) l=v; else if(m===1 && rr>0) l=v/rr; else if(m===2 && rr>0) l=v/(rr*rr);
  }
  if(a.useQuadratic || (!interior && a.outQuadInLin)){
    const rr=r*(+a.quadraticRadiusMult||0); q=0.01;
    const m=+a.quadraticMethod||0, v=+a.quadraticValue||0;
    if(m===0) q=v; else if(m===1 && rr>0) q=v/r; else if(m===2 && rr>0) q=v/(rr*rr);
  }
  if(c===0 && l===0 && q===0) c=1;
  return [c,l,q];
}
/* MGE XE's decodeMorrowindPointLight (d3d8/cpp/mge/ffeshader.cpp), one light at a time:
   what its FixedFuncEmu makes of the coefficients above. A standard lamp (constant > 0)
   keeps its colour and falls off as 1/(q·d² + c); a quadratic-only one is scaled by the
   shared constant (0.33 when no standard light has set it) and given an ambient term; a
   linear-only one is the Light spell's - white, brightness 0.25 + 1e-4/l, ambient 1. */
function mgeDecode(co, colour){
  const [c,l,q]=co;
  if(c>0) return {c, l:0, q, amb:0, col:colour};
  if(q>0) return {c:0.33, l:0, q:0.33*q, amb:1+1e-4/Math.sqrt(q), col:[colour[0]*0.33, colour[1]*0.33, colour[2]*0.33]};
  if(Math.abs(l-0.10000001)<1e-7) return {c:0.33, l:0, q:5e-5, amb:0, col:colour};
  if(l>0){ const b=0.25+1e-4/l; return {c:0.33, l:0, q:0.5555*l*l, amb:1, col:[b,b,b]}; }
  return {c:0.33, l:0, q:0, amb:0, col:colour};
}
/* How many segments a ring is made of. The rings are plain GL_LINES, one pixel wide —
   the only width a desktop driver honours, and the one Robin asked for once he had seen
   the alternative: "Make it into just 1px line again if it's the only thing supported
   natively." */
const PIVOT_SEGS=72;

/* ---- the material groups' numbers (round 18cr; see `_buildGroups`) ----------------- */
/* A batch whose whole cost is under this many triangles is one run, not one per cell:
   drawing it entire costs the GPU less than the page testing its cells, and a thousand
   such batches were most of the draw calls on a 49-cell load. */
const WHOLE_TRIS=2000;
/* How many runs one multi-draw call may carry: their first instances ride a uniform
   `ivec4` array, and GLES 3.0 promises 256 vectors to a vertex shader in all. */
const RUN4=192;
const MAXRUN=RUN4*4;
/* The instance texture's width in texels; four texels an instance, so a row is 512
   instances and the four never straddle a row. */
const INST_W=2048;
const INST_SHIFT=11;   // log2(INST_W)
/* The texture unit the instance textures are bound on - the objects' and the grass's
   (round 18cs). Eleven: 0 is the base map, 1 the terrain's blend map, 2 the ground map, 3
   the dark map, 5 the detail map, 6 the glow map, 7-9 the light grid and 10 the culling
   field (`_cullUniforms`). 18cr had the objects' instances on 10 as well, and the field
   went over them in every pass that set the cull uniforms after the bind - which drew no
   object at all through the groups once the order of the two moved. */
const INST_UNIT=11;
/* Round 18df/18dg: the decal map's unit - past the instance texture's, which a sign's
   decal must not bind over. */
const DECAL_UNIT=12;
/* Round 18dl: the environment map's unit and the bump map's, past the decal's. */
const ENV_UNIT=13, BUMP_UNIT=14;
/* Wraithguard: the normal map's unit (7-9 are the lamps', 11-15 taken above). */
const NRM_UNIT=10, SPEC_UNIT=4;
const NO_MAPS_OFF=Object.freeze({});
/* Round 18ee: the sun's shadow atlas, on the last unit GLES 3.0 promises a fragment
   shader (sixteen, 0-15). Every surface program reads it, so it is bound once a frame and
   left alone - which is why it sits past every map a material can ask for. */
const SHADOW_UNIT=15;
/* How many vertices the bake may copy in all (round 18cs, see `_buildGroups`): a whole
   batch is copied once per instance, and a forty-nine cell load of detailed meshes could
   otherwise ask for a few hundred megabytes. Two million is some ninety megabytes. */
const BAKE_VERTS=2000000;
/* How far past the water's own screen rectangle the reflection pass still draws, in clip
   units (round 18cs, `_waterRect`). The water shader displaces its reflection sample by
   at most a few hundredths of the screen; a tenth each way is room to spare. */
const WATER_RECT_MARGIN=0.2;
/* How many consecutive frames' times the loop keeps for the report (round 18cr). */
const FRAME_RING=240;
/* The land's flat colour where a layer has no texture and no colour of its own. */
const FLAT_LAND=[0.29,0.285,0.255];

/* VS_STATIC, reading its instance from a texture rather than from instanced attributes
   (round 18cr). `base` is the run's first instance: from the uniform array by `gl_DrawID`
   when the context has WEBGL_multi_draw (`mode` 'multi'), else a plain uniform set per run
   ('base'). Everything after the fetch is VS_STATIC line for line, so the two draw the
   same picture. Round 18cs adds 'baked': the instance number is the vertex's own, an
   integer attribute on the baked copies (see `_buildGroups`), and `gl_DrawID` is never
   read - which is what spares the driver a uniform write per sub-draw. */
const VS_STATIC_M=(mode)=>`#version 300 es
${mode==='multi'? '#extension GL_ANGLE_multi_draw : require' : ''}
precision highp float;
precision highp int;
layout(location=0) in vec3 aPos;
layout(location=1) in vec3 aNrm;
layout(location=2) in vec2 aUV;
layout(location=6) in vec4 aCol;   // RGBA since 18df
layout(location=7) in vec2 aUV2;
${mode==='baked'? 'layout(location=8) in int aInst;' : ''}
layout(location=9) in vec2 aUVD;
layout(location=10) in vec2 aUVDc;
uniform mat4 uVP;
uniform mat4 uAnim;
uniform highp sampler2D uInst;
${mode==='multi'? 'uniform ivec4 uRunBase['+RUN4+'];' : mode==='base'? 'uniform int uBase;' : ''}
out vec2 vUV; out vec2 vUV2; out vec2 vUVD; out vec2 vUVDc; out vec3 vN; out vec3 vW; out vec3 vC; out vec3 vG; out float vH;
out float vCA;
out float vGK;
flat out vec2 vLamp;
${ENV_VS_GLSL}
void main(){
  ${mode==='multi'? 'int t=(uRunBase[gl_DrawID>>2][gl_DrawID&3]+gl_InstanceID)*4;'
   : mode==='base'? 'int t=(uBase+gl_InstanceID)*4;' : 'int t=aInst*4;'}
  ivec2 at=ivec2(t&${INST_W-1}, t>>${INST_SHIFT});
  vec4 iR0=texelFetch(uInst,at,0);
  vec4 iR1=texelFetch(uInst,at+ivec2(1,0),0);
  vec4 iR2=texelFetch(uInst,at+ivec2(2,0),0);
  vec4 iL =texelFetch(uInst,at+ivec2(3,0),0);
  vC=aCol.rgb; vCA=aCol.a; vG=vec3(1.0); vH=1.0; vGK=0.0; vUV2=aUV2; vUVD=aUVD; vUVDc=aUVDc; vLamp=iL.xy;
  vec4 p=uAnim*vec4(aPos,1.0);
  vec3 w=vec3(dot(iR0,p), dot(iR1,p), dot(iR2,p));
  vec3 n=mat3(uAnim)*aNrm;
  vN=normalize(vec3(dot(iR0.xyz,n), dot(iR1.xyz,n), dot(iR2.xyz,n)));
  vW=w; vUV=aUV; vEnv=envCoord(w, vN);
  gl_Position=uVP*vec4(w,1.0);
}`;
/* And the solid marker at the centre. Robin: "render a solid sphere with a radius of 25%
   of the radius of the orbit ball, to help visualize it." A fraction of the ball's own
   radius, so it keeps its proportion at every distance. */
const PIVOT_BALL=0.25;

function alphaCut(part,blended){
  const f=(part&&part.alphaFlags)|0;
  const test=(f&0x200)!==0, blend=(f&1)!==0 && !blended;
  const th=part&&part.alphaThreshold!=null? part.alphaThreshold/255 : 0;
  let fn=test? (f>>10)&7 : -1, ref=test? th : 0;
  /* `blended` is round 17h: this shape is actually being drawn see-through, so the
     "blending we cannot do, so cut instead" rule below does not apply to it. */
  if(blend){
    const keepsMost = fn<0 || fn===0 || ((fn===4||fn===6) && ref<0.34);
    if(keepsMost){ fn=6; ref=0.34; }
  }
  /* Round 18dg: a blended cutout writes depth now, as the game's do, and is sorted far
     to near - so a texel that is all but clear must not write its depth, or the sky
     shows through the leaf behind it wherever a shape's own triangles come out of order
     (the game's halo). Cut at a tenth of the *texture's* alpha (fn 8: the texture alone,
     not the product with the material's), and only where the shape has no test of its
     own and does write depth (`blendWritesDepth`). Robin chose "the game's look minus
     its halos". */
  if(blended && (f&1) && !(f&0x2000) && (fn<0 || fn===0) && blendWritesDepth(part, blended)){ fn=8; ref=0.1; }
  return {fn,ref};
}
/** Round 18dh: whether a see-through shape writes the depth buffer. The cutouts do - a
 *  leaf cluster's texels are mostly solid or clear, and its own quads have to hide one
 *  another as the game's do. A *sheet* does not: a shape see-through by its material
 *  (alpha under 1), by an alpha controller or by an additive blend is one layer of
 *  translucency with nothing of its own to hide, and the game's depth write costs it
 *  z-fighting where a mesh stacks such sheets a unit apart - the Ghostfence is three,
 *  and with the writes on it "flickered as if z-fighting" for Robin at a distance,
 *  as 24 bits of depth cannot tell 1 unit apart past ten thousand. Sorted far to near
 *  the sheets need no writes to be right; before 18dg none of this pass wrote, and the
 *  sheets keep that. A NiZBufferProperty saying test-only turns the writes off either
 *  way. */
function blendWritesDepth(part, blend){
  if(!part || part.zwrite===false) return false;
  if(!blend) return true;
  if(blend.nosort) return true;
  /* Round 18dq: a shape see-through by its material writes depth, as the game's does
     (NiZBufferProperty's default; OpenMW leaves it on too) - 18dh had it off as a
     "sheet", and a crystal's near shard, drawn before a far one, was painted over by
     it: "the close crystal is almost invisible". Additive shapes and alpha controllers
     keep 18dh's exception, on purpose: a flame writing depth over its own layers is the
     halo Robin asked to be rid of. */
  if(blend.additive) return false;
  if(part.anim && part.anim.alpha) return false;
  /* A scrolling texture is a flowing sheet - the waterfalls, the streams, the
     Ghostfence - and Vivec's waterfall is several such layers a few units apart, which
     is the z-fighting case again. No cutout scrolls. */
  if(part.uvAnim) return false;
  return true;
}

class Renderer{
  constructor(canvas){
    this.cv=canvas;
    const gl=canvas.getContext('webgl2',{antialias:true,alpha:false,preserveDrawingBuffer:false});
    if(!gl) throw new Error('WebGL2 is not available in this browser.');
    this.gl=gl;
    /* How many lamps a scene may hold. No longer a question for the driver: round 17r
       moved them out of the uniform arrays and into a texture, so the ceiling that used to
       be GL_MAX_FRAGMENT_UNIFORM_VECTORS is gone and what a fragment pays is the per-
       column cap in the shader instead. */
    this.maxLights=LIGHT_BUDGET;
    /* Round 18cs: the grass reads its instances from a texture, by `gl_DrawID` where the
       context has WEBGL_multi_draw (looked up here, before the program is built). */
    this._mdraw=null;
    try{ this._mdraw=gl.getExtension('WEBGL_multi_draw')||null; }catch(_){ this._mdraw=null; }
    this.progGrass=this._prog(VS_GRASS_M(this._mdraw? 'multi' : 'base'),FS_GRASS);
    this.progGrass.multi=!!this._mdraw;
    this.progGround=this._prog(VS_GROUND,FS_GROUND);
    this.progFlat=this._prog(VS_FLAT,FS_FLAT);
    this.progWater=this._prog(VS_WATER,FS_WATER);
    this.progStatic=this._prog(VS_STATIC,FS_GRASS);
    this.progStaticPaint=this._prog(VS_STATIC_PAINT,FS_STATIC_PAINT);
    this.progArrow=this._prog(VS_ARROW,FS_ARROW);
    this.progSky=this._prog(VS_SKY,FS_SKY);
    // The brush ring compiled into FS_GRASS starts off in both its programs; only the
    // placed-object pass ever turns it on, per frame, and turns it back off.
    for(const pr of [this.progGrass,this.progStatic]){
      gl.useProgram(pr.p); gl.uniform4fv(pr.u.uBrushS,[0,0,0,-1]);
    }
    gl.useProgram(this.progGrass.p); gl.uniform1i(this.progGrass.u.uInst,INST_UNIT);   // the instance texture's unit, for good
    this.statics=[];
    this.hlStatics=[];      // the ones being pointed at, drawn again in gold
    this.hlMode='fill';     // round 18dq: 'fill' (the gold wash) or 'outline' (34_outline.js)
    /* Round 18cr: the material groups the still objects are drawn through, and the
       extension that lets a group be one call. `noMultiDraw` is the test hook that walks
       the fallback path on a context that has the extension. */
    this._groups=null;
    this._legacyStatics=null;
    this.noMultiDraw=false;   // `_mdraw` itself is looked up above, before the grass program
    this.batches=[];        // {vao,count,instances,tex,alpha:{fn,ref},diffuse}
    this.ground=null;
    this.cellChunks=[];
    this.markers=null;
    this.texCache=new Map();
    this.white=this._tex1();
    /* Stand-ins for the three light textures (round 17r). A sampler that is never assigned
       reads texture unit 0, and unit 0 holds an ordinary RGBA texture — so on a frame with
       no lamps, `uBins` and `uIndex` (isampler2D) would be pointed at a float texture and
       every draw would fail with INVALID_OPERATION. The sampler's type has to match what
       is bound whether or not the shader reads it, so these are bound and pointed at
       always, and `uLightN` is what actually turns the lamps off. */
    this._noLights=this._texN(gl.RGBA32F, gl.RGBA, gl.FLOAT, new Float32Array(4));
    this._noBins  =this._texN(gl.RG32I, gl.RG_INTEGER, gl.INT, new Int32Array(2));
    this._noIndex =this._texN(gl.R32I, gl.RED_INTEGER, gl.INT, new Int32Array(1));
    this._bindLights(null);
    /* Round 17w: attribute 8 is the lamp's hours on a static instance. A batch with no
       lamps leaves the array disabled, and the shader then reads this generic value -
       (-1,-1), "no lamp", which is what makes a crystal glow at noon. Context state, not
       vertex-array state, so once is enough. */
    gl.vertexAttrib2f(8,-1,-1);
    // Round 17y: attribute 9 is the dark map's UV set; (0,0) where a shape has none.
    gl.vertexAttrib2f(9,0,0);
    /* `vs` (round 17m) is how much the orbit distance has been shrunk by Ctrl-pulls that
       did not move the eye; `viewDist` divides it back out. 1 = the pivot is where the
       camera is actually looking from `dist` away. */
    this.cam={az:-0.62,el:0.30,dist:2600,tx:0,ty:0,tz:120,vs:1};
    /* Round 17m: the cell's LIGH lamps ({p, r, c, neg}), set by the cell preview, and
       null everywhere else — a mesh being inspected on its own lights by the sky. */
    this.lights=null;
    this._anyUvAnim=false;
    this.nav='orbit';          // round 17: or 'wasd' - see setNav
    this.hovered=false;        // round 18cw: the pointer is over the picture (Tab swaps the scheme)
    this.lookSmooth=35;        // the WASD look's catch-up rate, 1..100 (round 17h: 35)
    /* Round 18ag: how far the head turns per pixel of mouse, as a multiple of the rate
       the scheme shipped with - 1 is what every earlier round turned at, 0.25 a quarter
       of it, 4 four times. A multiple rather than a rate in radians so the number a
       person sees in the dialogue means something on its own ("half as fast"), and so
       the default is a round 1 and not 0.0045. Clamped where it is used. */
    this.lookSens=1;
    this.moveSmooth=12;        // how fast the flying camera's speed follows the keys (round 17h: 12)
    this.orbitSmooth=40;       // round 17f: the same easing on the orbit camera's own moves, 1..100
    this.orbitLookSmooth=30;   // round 17g: the same lag on the orbit camera's turn (round 17h: 30)
    try{ this.cv.tabIndex=-1; }catch(_){ }   // so the viewport can take the keyboard
    // The sun starts high and the scene starts a little brighter than daylight: this
    // is a tool for judging placement, not a screenshot of the game, and dark ground
    // hides exactly what the user is trying to look at.
    this.opts={grid:false,ground:true,markers:true,fog:true,sun:90,bright:1.4,showSlope:true,
               /* The same overlay on the placed objects, its own switch: the Highlight
                  culling button cycles off / terrain / terrain + statics, because a
                  cliff's every underside wearing the shade is the loudest thing in the
                  picture when what you care about is the ground. */
               showSlopeStatics:false,
               /* Round 18l: the mesh editor's own cull switch, and whether the scene *is*
                  the editor - one mesh at the origin rather than a cell. The statics pass
                  reads the two together: in the editor the shade follows `showSlopeMesh`
                  and asks only about the slope, since a mesh standing at the origin has
                  no world height to be above or below. */
               inspecting:false, showSlopeMesh:false,
               arrows:true,
               /* Round 11 item 11. `tint`: how much of the ground's colour a blade
                  takes at its base, 0..1 - the "Ground tint" slider. `vcol`: whether
                  the landscape and the meshes are drawn with their vertex colours, as
                  the game draws them; off shows the bare textures. */
               tint:1.0, vcol:true,
               /* Round 12 item 4: the atmosphere - a sky, the sun's colour from its
                  height, the haze the sky's own colour at the horizon. */
               scatter:false,
               // The game clock the sky is drawn for (round 13). The weather was a
               // picker until round 17h; it is Clear and only Clear now.
               hour:15,
               // Round 12 item 6: how hard the wind blows the grass, 0 (still) to 1.
               wind:0.25,
               // Round 15 item 3: the atmosphere's fog density, 1 = MGE's distances.
               fogDensity:1,
               /* Round 14: the blades drawn the way MGE XE's grass shader draws them -
                  wrap lighting, a fixed cutout softened by coverage, vertex colours
                  ignored. Off is the plain rules every other mesh gets. */
               mgeGrass:true,
               /* Round 15 items 8-10: the water drawn MGE XE's way (26_water.js). Off is
                  the plain translucent sheet. */
               mgeWater:false,
               // Round 16: MGE's sunshafts, under the atmosphere; off unless asked for.
               sunshafts:false,
               // Round 18a: screen-space ambient occlusion (29_ssao.js); off unless asked for.
               ssao:false,
               // Round 18do: FXAA over the finished frame (33_fxaa.js); off unless asked for.
               fxaa:false,
               /* Round 18ee: the sun's shadow (37_shadow.js), MGE's two cascades. Off
                  unless the install says the game has them on. */
               shadows:false,
               unlit:false,   // round 18f: the Unlit switch, every surface at its texture
               // Paint mode: whether the mask is drawn, and whether the window is in it.
               paintShow:false, paintMode:false,
               /* Whether an object's back faces are drawn. Off outdoors; on indoors,
                  where a room's near wall is a back face and drawing it means looking at
                  a box. See the note where it is read. */
               cullStatics:false,
               maxAngle:90,minH:-1e9,maxH:1e9,tile:4};
    /* Vertex arrays kept between rescatters, keyed by the mesh part and then by the
       batch key. See `buildBatch`: the mesh half of a batch never changes, and building
       it again for every edit was 51 ms a rebuild at 168,000 blades. */
    this._batchGpu=new Map();
    this.paintTex=new Map();   // "gx,gy" -> R8 texture of that cell's "no grass" paint
    /* The mask size at which a bucket grid starts paying for itself, as a field rather
       than the bare constant, because the only way to know the grid is *exact* is to draw
       the same mask both ways and compare the pixels — which needs the threshold to move.
       `t_statics` does exactly that. Nothing in the interface touches it. */
    this.dabGridMin=DAB_GRID_MIN;
    this.growTex=new Map();    // and of its "grass here" paint
    /* And the rule layer, one texture per (cell, swatch): the strength where that swatch
       owns the texel and nothing elsewhere. Per swatch rather than one texture of slot
       numbers because a slot is a *name* — a shader interpolating between two of them
       would draw a colour nobody painted, exactly as it would place a rule nobody
       painted (see `Paint::rule_at`). Keyed "gx,gy|slot". */
    this.ruleTex=new Map();
    this.rulePalette=[];       // [{slot, colour, rule, layer}], in the order they are drawn
    this.hiddenLayers=new Set();  // paint layers whose colour is not drawn
    this.brush=null;           // {x,y,r,inner} while the pointer is over the ground
    /* The viewport's own grey. Since round 18dd this is *not* the exterior's backdrop any
       more — that is `backdrop()`, which a theme can set — but the two dims that must not
       move with a theme: `uDimCol` (everything outside a highlight) and the arrows' body
       behind terrain. It is also what `backdrop()` answers for a theme that sets nothing. */
    this.fogCol=BACK_LEGACY.slice();
    this._bindCam();
    this.dirty=true;
    this._loop=this._loop.bind(this);
    requestAnimationFrame(this._loop);
  }
  _sh(t,src){
    const gl=this.gl,s=gl.createShader(t);
    gl.shaderSource(s,src); gl.compileShader(s);
    if(!gl.getShaderParameter(s,gl.COMPILE_STATUS))
      throw new Error('shader: '+gl.getShaderInfoLog(s));
    return s;
  }
  _prog(vs,fs){
    const gl=this.gl,p=gl.createProgram();
    gl.attachShader(p,this._sh(gl.VERTEX_SHADER,vs));
    gl.attachShader(p,this._sh(gl.FRAGMENT_SHADER,fs));
    gl.linkProgram(p);
    if(!gl.getProgramParameter(p,gl.LINK_STATUS))
      throw new Error('link: '+gl.getProgramInfoLog(p));
    const u={};
    const n=gl.getProgramParameter(p,gl.ACTIVE_UNIFORMS);
    for(let i=0;i<n;i++){ const nm=gl.getActiveUniform(p,i).name.replace(/\[0\]$/,''); u[nm]=gl.getUniformLocation(p,nm); }
    /* The three light samplers, pinned to their units here rather than per frame.
     *
     * They are `isampler2D`, and a sampler that has never been assigned reads unit 0 —
     * which holds an ordinary RGBA texture, so *any* draw of a program that declares them
     * fails with INVALID_OPERATION until something sets them. Setting them in `airOn`
     * was not enough: `thumbDraw` shares this program and does not go through `airOn`, so
     * a thumbnail drawn before the first frame hit exactly that. The unit numbers are
     * fixed and the textures on them are always type-correct (see `_noBins`), so the
     * honest place for this is once, here. Round 17r. */
    if(u.uLights||u.uBins||u.uIndex){
      const prev=gl.getParameter(gl.CURRENT_PROGRAM);
      gl.useProgram(p);
      if(u.uLights) gl.uniform1i(u.uLights,7);
      if(u.uBins)   gl.uniform1i(u.uBins,8);
      if(u.uIndex)  gl.uniform1i(u.uIndex,9);
      gl.useProgram(prev);
    }
    return {p,u};
  }
  /** The lamp textures onto units 7, 8 and 9 — this frame's, or the stand-ins.
   *
   *  Bound whether or not there are lamps: the samplers are pinned to these units for the
   *  life of each program (see `_prog`), so what sits on them has to be type-correct
   *  always. `uLightN` is what turns the lamps off, not an unbound unit. Round 17r. */
  _bindLights(g){
    const gl=this.gl;
    const on=(g && g.n)? g : null;
    gl.activeTexture(gl.TEXTURE7); gl.bindTexture(gl.TEXTURE_2D, on? on.lights : this._noLights);
    gl.activeTexture(gl.TEXTURE8); gl.bindTexture(gl.TEXTURE_2D, on? on.bins   : this._noBins);
    gl.activeTexture(gl.TEXTURE9); gl.bindTexture(gl.TEXTURE_2D, on? on.index  : this._noIndex);
    gl.activeTexture(gl.TEXTURE0);
    // Round 18cr: every pass starts here, and unit 0 is whatever the last pass left.
    this._tex0=undefined;
    this._texU=null;   // 18cs: the map units too
  }
  /** The six frustum planes of a view-projection, as (a,b,c,d) with the inside positive.
   *
   *  Gribb-Hartmann, off the rows of the matrix. `VP` is column-major, the way GL wants
   *  it, so row i is [m[i], m[4+i], m[8+i], m[12+i]]. Round 17r. */
  _frustum(VP){
    const r=i=>[VP[i],VP[4+i],VP[8+i],VP[12+i]];
    const [rx,ry,rz,rw]=[r(0),r(1),r(2),r(3)];
    const add=(a,b,sg)=>{
      const p=[a[0]+sg*b[0], a[1]+sg*b[1], a[2]+sg*b[2], a[3]+sg*b[3]];
      const L=Math.hypot(p[0],p[1],p[2])||1;
      return [p[0]/L,p[1]/L,p[2]/L,p[3]/L];
    };
    return [add(rw,rx,1), add(rw,rx,-1), add(rw,ry,1),
            add(rw,ry,-1), add(rw,rz,1), add(rw,rz,-1)];
  }
  /** The frustum of a screen rectangle `[x0,y0,x1,y1]` (clip coordinates, -1..1) of a
   *  view-projection - the same six planes with the sides moved in (round 18cs). The
   *  whole screen gives `_frustum` exactly. */
  _frustumRect(VP,rect){
    const r=i=>[VP[i],VP[4+i],VP[8+i],VP[12+i]];
    const [rx,ry,rz,rw]=[r(0),r(1),r(2),r(3)];
    const norm=p=>{ const L=Math.hypot(p[0],p[1],p[2])||1; return [p[0]/L,p[1]/L,p[2]/L,p[3]/L]; };
    const side=(a,ka,b,kb)=>norm([ka*a[0]+kb*b[0], ka*a[1]+kb*b[1], ka*a[2]+kb*b[2], ka*a[3]+kb*b[3]]);
    const [x0,y0,x1,y1]=rect;
    return [side(rx,1,rw,-x0), side(rw,x1,rx,-1), side(ry,1,rw,-y0),
            side(rw,y1,ry,-1), norm([rw[0]+rz[0],rw[1]+rz[1],rw[2]+rz[2],rw[3]+rz[3]]),
            norm([rw[0]-rz[0],rw[1]-rz[1],rw[2]-rz[2],rw[3]-rz[3]])];
  }
  /** Where the water can show on screen, for the reflection pass (round 18cs).
   *
   *  The reflection is read back only where a water fragment is drawn, and the sheet is
   *  drawn depth-tested under the ground - so it shows only over the cells whose land dips
   *  under the water line, and over the cells the load has no land for. Those footprints,
   *  at the water's height, projected through the reflection's own view-projection, are a
   *  screen rectangle: `[x0,y0,x1,y1]` in clip coordinates, widened by more than the
   *  ripples can displace a sample. `null` is the whole screen (no land to judge by, or
   *  the rectangle is the screen) and `false` is nowhere: the water is under ground
   *  everywhere, or wholly off screen, and the pass can be skipped whole. */
  _waterRect(VPR){
    const w=this.water, chunks=this.cellChunks||[];
    if(!w || !chunks.length || !this.slab) return null;
    const wz=w.z;
    const boxes=[];
    let cx0=Infinity,cy0=Infinity,cx1=-Infinity,cy1=-Infinity;
    const cells=new Set();
    for(const c of chunks){
      const b=c.box3; if(!b) return null;
      // A chunk is a cell: its column and row from where it stands.
      const cx=Math.round(b[0]/CELL), cy=Math.round(b[1]/CELL);
      cells.add(cx+','+cy);
      if(cx<cx0)cx0=cx; if(cx>cx1)cx1=cx; if(cy<cy0)cy0=cy; if(cy>cy1)cy1=cy;
      /* Under the line anywhere - with a little room, since a fragment a whisker above
         the sheet and a sheet a whisker above the fragment resolve differently. */
      if(b[2]<wz+2) boxes.push([b[0],b[1],b[3],b[4]]);
    }
    // The cells of the load's rectangle with no land: the sheet shows over the sky there.
    for(let cx=cx0;cx<=cx1;cx++) for(let cy=cy0;cy<=cy1;cy++)
      if(!cells.has(cx+','+cy)) boxes.push([cx*CELL,cy*CELL,(cx+1)*CELL,(cy+1)*CELL]);
    if(!boxes.length) return false;
    let x0=Infinity,y0=Infinity,x1=-Infinity,y1=-Infinity;
    const M=VPR;
    const clip=(x,y)=>[M[0]*x+M[4]*y+M[8]*wz+M[12], M[1]*x+M[5]*y+M[9]*wz+M[13],
                       M[2]*x+M[6]*y+M[10]*wz+M[14], M[3]*x+M[7]*y+M[11]*wz+M[15]];
    for(const b of boxes){
      /* The footprint's four corners in clip space, cut to the near plane (z >= -w) -
         a corner behind the mirrored eye is cut off rather than projected, since a point
         behind the eye lands nowhere on the screen. What is left has w > 0 throughout. */
      let poly=[clip(b[0],b[1]), clip(b[2],b[1]), clip(b[2],b[3]), clip(b[0],b[3])];
      const out=[];
      for(let i=0;i<poly.length;i++){
        const a=poly[i], c=poly[(i+1)%poly.length];
        const da=a[2]+a[3], dc=c[2]+c[3];
        if(da>=0) out.push(a);
        if((da>=0)!==(dc>=0)){
          const t=da/(da-dc);
          out.push([a[0]+(c[0]-a[0])*t, a[1]+(c[1]-a[1])*t, a[2]+(c[2]-a[2])*t, a[3]+(c[3]-a[3])*t]);
        }
      }
      for(const v of out){
        const cw=Math.max(v[3],1e-6);
        const cx=v[0]/cw, cy=v[1]/cw;
        if(cx<x0)x0=cx; if(cx>x1)x1=cx; if(cy<y0)y0=cy; if(cy>y1)y1=cy;
      }
    }
    if(!(x0<=x1 && y0<=y1)) return false;   // every footprint behind the eye
    const m=WATER_RECT_MARGIN;
    x0=Math.max(-1,x0-m); y0=Math.max(-1,y0-m); x1=Math.min(1,x1+m); y1=Math.min(1,y1+m);
    if(x0>=x1 || y0>=y1) return false;   // wholly off screen
    if(x0<=-1 && y0<=-1 && x1>=1 && y1>=1) return null;
    return [x0,y0,x1,y1];
  }
  /** Is this world box anywhere inside those planes?
   *
   *  The usual conservative test: for each plane, take the corner furthest along its
   *  normal, and if even that is behind the plane the whole box is. It can say "yes" for a
   *  box that is in fact outside — a box straddling two planes' outsides — which costs a
   *  draw that draws nothing, and never the reverse. Round 17r. */
  _boxInView(pl,b){
    for(let i=0;i<6;i++){
      const p=pl[i];
      const x=p[0]>=0? b[3] : b[0], y=p[1]>=0? b[4] : b[1], z=p[2]>=0? b[5] : b[2];
      if(p[0]*x + p[1]*y + p[2]*z + p[3] < 0) return false;
    }
    return true;
  }
  /** A 1x1 texture in whatever format, with nearest sampling and no mips. */
  _texN(internal,format,type,data){
    const gl=this.gl,t=gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D,t);
    gl.texImage2D(gl.TEXTURE_2D,0,internal,1,1,0,format,type,data);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.CLAMP_TO_EDGE);
    gl.bindTexture(gl.TEXTURE_2D,null);
    return t;
  }
  _tex1(){
    const gl=this.gl,t=gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D,t);
    gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,1,1,0,gl.RGBA,gl.UNSIGNED_BYTE,new Uint8Array([255,255,255,255]));
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.LINEAR);
    return t;
  }
  /** The sampling every texture gets, once it is bound and filled. */
  _texParams(mipped){
    const gl=this.gl;
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,
                     mipped? gl.LINEAR_MIPMAP_LINEAR : gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.REPEAT);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.REPEAT);
    const ext=this.anis||(this.anis=gl.getExtension('EXT_texture_filter_anisotropic'));
    if(ext) gl.texParameterf(gl.TEXTURE_2D,ext.TEXTURE_MAX_ANISOTROPY_EXT,
      Math.min(8,gl.getParameter(ext.MAX_TEXTURE_MAX_ANISOTROPY_EXT)));
  }
  makeTexture(img){
    const gl=this.gl,t=gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D,t);
    gl.pixelStorei(gl.UNPACK_ALIGNMENT,1);
    gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,img.w,img.h,0,gl.RGBA,gl.UNSIGNED_BYTE,
      img.data instanceof Uint8Array?img.data:new Uint8Array(img.data));
    gl.generateMipmap(gl.TEXTURE_2D);
    this._texParams(true);
    return t;
  }

  /** Which block-compressed formats this machine can be handed, if any.
   *
   *  `WEBGL_compressed_texture_s3tc` is what lets a DDS reach the GPU as the blocks it
   *  already holds — no decode on the drawing thread, and the card keeps the file's own
   *  bytes rather than eight times as many. Every desktop Chromium on Windows has it, and
   *  the engine is told whether we do: without it, it sends pixels instead of a texture
   *  we could not upload. */
  bcFormats(){
    if(this._bc!==undefined) return this._bc;
    const gl=this.gl, e=gl.getExtension('WEBGL_compressed_texture_s3tc');
    this._bc = e? {bc1:e.COMPRESSED_RGBA_S3TC_DXT1_EXT,
                   bc2:e.COMPRESSED_RGBA_S3TC_DXT3_EXT,
                   bc3:e.COMPRESSED_RGBA_S3TC_DXT5_EXT} : null;
    /* Wraithguard: and BC4/BC5 (RGTC) and BC7 (BPTC) where the webview has them - WebView2
       does - so a normal map (BC5, or BC3 with a height in its alpha, or BC1) and a modern
       pack's BC7 go to the GPU as they are. Without them the engine decodes (bcx.rs). */
    if(this._bc){
      const r=gl.getExtension('EXT_texture_compression_rgtc');
      if(r){ this._bc.bc4=r.COMPRESSED_RED_RGTC1_EXT; this._bc.bc5=r.COMPRESSED_RED_GREEN_RGTC2_EXT; }
      const p=gl.getExtension('EXT_texture_compression_bptc');
      if(p){ this._bc.bc7=p.COMPRESSED_RGBA_BPTC_UNORM_EXT; }
    }
    return this._bc;
  }
  /** The block formats beyond S3TC the engine may send as blocks (`bcx`). */
  bcExtra(){
    const f=this.bcFormats()||{};
    const out=[];
    if(f.bc5) out.push('rgtc');
    if(f.bc7) out.push('bptc');
    return out;
  }

  /** A texture from compressed blocks, one `compressedTexImage2D` per level.
   *
   *  The mip chain comes out of the file: `generateMipmap` cannot make mips for a
   *  compressed texture, which is why the engine decodes a file that has none rather than
   *  sending one level and leaving distant ground to shimmer. */
  makeCompressed(kind,levels){
    const fmt=this.bcFormats(); if(!fmt||!fmt[kind]) return null;
    const gl=this.gl, t=gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D,t);
    gl.pixelStorei(gl.UNPACK_ALIGNMENT,1);
    levels.forEach((l,i)=>
      gl.compressedTexImage2D(gl.TEXTURE_2D,i,fmt[kind],l.w,l.h,0,l.data));
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_BASE_LEVEL,0);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAX_LEVEL,levels.length-1);
    this._texParams(levels.length>1);
    return t;
  }
  _buf(data,target){
    const gl=this.gl,b=gl.createBuffer();
    target=target||gl.ARRAY_BUFFER;
    gl.bindBuffer(target,b); gl.bufferData(target,data,gl.STATIC_DRAW);
    return b;
  }
  /** Frees a vertex array **and the buffers it was built from** — round 18bd (F16, F17).
   *
   *  `gl.deleteVertexArray` frees the array object and nothing else: the buffers it
   *  referenced stay allocated until they are deleted themselves, and the handles were
   *  being thrown away at creation, so nothing could ever delete them. `setMarkers` runs
   *  on every scatter with two instance streams that are hundreds of kilobytes on a
   *  49-cell load and had no delete at all; `setMissing`, `setWater` and the MGE water's
   *  own mesh deleted the array alone. Reclaimable by the driver once unreachable, so
   *  churn rather than a permanent leak — but unbounded across rescatters and invisible.
   *
   *  Pass the object the setter stored, with the buffer handles collected on `bufs`. */
  _dropVao(o){
    if(!o) return;
    const gl=this.gl;
    try{ gl.deleteVertexArray(o.vao); }catch(_){ }
    if(o.bufs) for(const b of o.bufs){ try{ gl.deleteBuffer(b); }catch(_){ } }
  }
  /** The vertex-colour stream for a part: its own RGB bytes, or white for a shape
   *  without any. Always a real buffer rather than a disabled attribute, so no vertex
   *  array is ever left reading the generic attribute's default of black. Three bytes a
   *  vertex: the land's VCLR. A mesh part takes `_colBuf4`. */
  _colBuf(part){
    const nv=part.pos.length/3|0;
    let c=part.col;
    if(!c || c.length!==nv*3){ c=new Uint8Array(nv*3); c.fill(255); }
    return this._buf(c);
  }
  /** Round 18df: a mesh part's colours as RGBA - its RGB bytes with its vertex alpha
   *  (`colA`) beside them, 255 where it has none - so the shader can take the alpha from
   *  the vertex where the fixed pipeline does. `rgba4(part)` builds the array; the merged
   *  builder writes the same bytes into its shared stream. */
  _colBuf4(part){
    return this._buf(rgba4(part));
  }
  /** How tall a part is, for the ground tint's fade: its span in z, remembered on the
   *  part since the part is shared by every batch that draws it. */
  _bladeH(part){
    if(part._zspan!=null) return part._zspan;
    let lo=Infinity, hi=-Infinity;
    const v=part.pos;
    for(let i=2;i<v.length;i+=3){ if(v[i]<lo)lo=v[i]; if(v[i]>hi)hi=v[i]; }
    part._zspan = hi>lo ? hi-lo : 0;
    part._zlo = hi>lo ? lo : 0;
    return part._zspan;
  }
  /** Build one instanced batch from a NIF part and an instance array.
   *
   *  **Reused where it can be.** A rescatter used to create a vertex array and six
   *  buffers for every batch and delete last time's — 51 ms of it at 168,000 blades,
   *  measured, on the thread that draws. The mesh half of a batch (positions, normals,
   *  UVs, indices) does not change between rescatters at all: it is the same part of the
   *  same NIF. Only the instance streams move, and they can be written into buffers that
   *  already exist.
   *
   *  Keyed by the part object's identity and `hold` together. The part is shared by every
   *  instance of a mesh — `loadMesh` caches it — so identity is a sound key and needs no
   *  string comparison.
   *
   *  **`hold` is not the batch key, and the difference is a bug that shipped.** It was the
   *  batch key — `sel:slot`, the rule and the slot inside it — which is unique per rule and
   *  *not* unique per batch: a nine-cell scene draws the same rule nine times, once per
   *  cell, so all nine batches found the same held record, each overwrote the instance
   *  buffers the last one had filled, and every one of them ended up drawing the ninth
   *  cell's blades. Eight cells of grass went in and came back out as a copy of the last
   *  one — which, when the last cell is a corner and you are looking at the middle, is a
   *  viewport with no grass in it. The caller passes something unique per batch now (§49
   *  443b); the batch key stays what it is, because the highlight compares against it.
   *
   *  **The instances are a texture now (round 18cs)**, not three instanced attribute
   *  streams: two RGBA32F texels a blade - position and yaw, scale and ground normal -
   *  that the vertex shader fetches by index (VS_GRASS_M), so a cell run of the batch is
   *  a sub-draw with a first-instance number rather than three attribute pointers moved
   *  to its offset. One texture per *instance array*: the parts of one mesh share the
   *  array the cell preview built, and so share the texture (`_instTexOf`), counted by
   *  the held records that hold it. Re-uploaded whole on every rescatter, as the streams
   *  were - the count changes on nearly every one. What is saved is the vertex array, the
   *  mesh buffers and the attribute wiring — the part that does not depend on how much
   *  grass there is. */
  buildBatch(part,instances,tex,key,hold){
    const gl=this.gl;
    const id=hold!=null? String(hold) : (key||'');
    let slot=this._batchGpu.get(part);
    if(!slot){ slot=new Map(); this._batchGpu.set(part,slot); }
    let held=slot.get(id);
    if(!held){
      const vao=gl.createVertexArray(); gl.bindVertexArray(vao);
      const bufs=[];
      bufs.push(this._buf(part.pos)); gl.enableVertexAttribArray(0); gl.vertexAttribPointer(0,3,gl.FLOAT,false,0,0);
      bufs.push(this._buf(part.nrm)); gl.enableVertexAttribArray(1); gl.vertexAttribPointer(1,3,gl.FLOAT,false,0,0);
      bufs.push(this._buf(part.uv));  gl.enableVertexAttribArray(2); gl.vertexAttribPointer(2,2,gl.FLOAT,false,0,0);
      // The detail map's own UV set (round 17h); left at (0,0) when the shape has none.
      if(part.uv2 && part.uv2.length){ bufs.push(this._buf(part.uv2)); gl.enableVertexAttribArray(7); gl.vertexAttribPointer(7,2,gl.FLOAT,false,0,0); }
      bufs.push(this._colBuf4(part)); gl.enableVertexAttribArray(6); gl.vertexAttribPointer(6,4,gl.UNSIGNED_BYTE,true,0,0);
      const ib=gl.createBuffer(); bufs.push(ib);
      gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER,ib);
      gl.bufferData(gl.ELEMENT_ARRAY_BUFFER,part.idx,gl.STATIC_DRAW);
      gl.bindVertexArray(null);
      held={vao,bufs,it:null,n:0};
      slot.set(id,held);
    }
    const it=this._instTexOf(instances);
    if(held.it!==it){
      if(held.it && --held.it.refs<=0){ held.it.dead=true; try{ gl.deleteTexture(held.it.tex); }catch(_){ } }
      it.refs++; held.it=it;
    }
    /* How many instances the texture now holds, kept on the shared record rather than
       only on the batch. A rebuild rewrites it while the *previous* batch list is still
       the one being drawn, and a frame can land in between — `draw` takes the smaller of
       the two counts so it can never ask for instances the texture no longer has. Fewer
       blades for one frame in the middle of a rebuild is what already happens; reading
       past the end is not. */
    held.n=it.n;
    return {vao:held.vao,hold:held,count:part.idx.length,n:instances.n,tex:tex||null,
            key:key||null,
            /* Grass keeps the old answer: a blended blade is cut out rather than
               blended (round 17h). The scattered field is thousands of instances with no
               sensible draw order, and a cutout is what it has always looked like. The
               placed objects are the ones that got translucency. */
            alpha:alphaCut(part), blend:null, bladeH:this._bladeH(part), bladeLo:part._zlo||0,
            // Round 17w: lit or not, and what the colours are for (see _meshMode).
            unlit:!!part.unlit, vcolMode:part.vcolMode==null? 2 : part.vcolMode, drawMode:part.drawMode|0, dayNight:part.dayNight|0,
            detail:(part.glDetail&&part.glDetail.gl)||null,
            glow:(part.glGlow&&part.glGlow.gl)||null,
            /* Round 17m: the shape's UV animation and its emissive colour ride with the
               batch, since both are per-shape facts the draw has to set. */
            uvAnim:part.uvAnim||null, uvAnim2:part.uvAnim2||null, emissive:part.emissive||null,
            // Round 17y: the shape's node animation and material fade, run per frame.
            anim:part.anim||null,
            diffuse:part.diffuse||[1,1,1], ambient:part.ambient||part.diffuse||[1,1,1],
            // Round 18de: whether the shape carries vertex colours at all (see uVColHas).
            vcolHas:!!(part.col && part.col.length),
            zwrite:blendWritesDepth(part, alphaBlend(part)),   // rounds 18dg/18dh: see blendWritesDepth
            /* Round 18df: the decal (slot 6) with its own UVs, and which edges of the base
               map are clamped (see `_wrapTex0`). */
            decal:(part.glDecal&&part.glDecal.gl&&part.uvDecal&&part.uvDecal.length)? part.glDecal.gl : null,
            clamp:part.clamp|0,
            // Round 18dl: the environment map and the bump map that perturbs it.
            env:(part.glEnv&&part.glEnv.gl)||null, envClamp:part.envClamp|0,
            bump:(part.glEnv&&part.glEnv.gl&&part.glBump&&part.glBump.gl)? part.glBump.gl : null,
            bumpLuma:part.bumpLuma||[1,0], bumpMat:part.bumpMat||[1,0,0,1]};
  }
  /** The instance texture for one instance array (round 18cs): two RGBA32F texels a
   *  blade, `[x, y, z, yaw]` and `[scale, nx, ny, nz]`, `INST_W` texels to a row so the
   *  two never straddle one. Built once per array object and shared by every held record
   *  drawing it - the parts of one mesh - with `refs` counting them; the last one to let
   *  go deletes it (`buildBatch`, `dropBatchGpu`). */
  _instTexOf(instances){
    const gl=this.gl;
    if(!this._instTexes) this._instTexes=new WeakMap();
    let it=this._instTexes.get(instances);
    if(it && !it.dead) return it;
    const n=instances.n|0, rows=Math.max(1,Math.ceil(n*2/INST_W));
    const data=new Float32Array(INST_W*rows*4);
    const pos=instances.pos, rsa=instances.rsa, nrm=instances.nrm;
    for(let k=0;k<n;k++){
      const o=k*8, p=k*3;
      data[o]=pos[p]; data[o+1]=pos[p+1]; data[o+2]=pos[p+2]; data[o+3]=rsa[p];
      data[o+4]=rsa[p+1]; data[o+5]=nrm[p]; data[o+6]=nrm[p+1]; data[o+7]=nrm[p+2];
    }
    const tex=gl.createTexture();
    gl.activeTexture(gl.TEXTURE0+INST_UNIT);
    gl.bindTexture(gl.TEXTURE_2D,tex);
    gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA32F,INST_W,rows,0,gl.RGBA,gl.FLOAT,data);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.CLAMP_TO_EDGE);
    gl.activeTexture(gl.TEXTURE0);
    it={tex,n,rows,refs:0,bytes:data.byteLength};
    this._instTexes.set(instances,it);
    return it;
  }
  /** Instances `at` onward of a grass batch, read back out of the texture the draw
   *  reads - eight floats each, as `_instTexOf` laid them out. For the tests: what the
   *  screen is drawn from, not what the page meant to send. */
  readInstances(b,at,n){
    const gl=this.gl, it=b&&b.hold&&b.hold.it;
    const out=new Float32Array(n*8);
    if(!it) return out;
    // Asked for here, once: a float texture is a complete attachment only with this on.
    if(this._cbf===undefined){ try{ this._cbf=gl.getExtension('EXT_color_buffer_float')||null; }catch(_){ this._cbf=null; } }
    const fb=gl.createFramebuffer();
    gl.bindFramebuffer(gl.FRAMEBUFFER,fb);
    gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,it.tex,0);
    if(gl.checkFramebufferStatus(gl.FRAMEBUFFER)!==gl.FRAMEBUFFER_COMPLETE){
      // No EXT_color_buffer_float: a float texture cannot be read back this way.
      gl.bindFramebuffer(gl.FRAMEBUFFER,null); gl.deleteFramebuffer(fb); return null;
    }
    for(let k=0;k<n;k++){
      const t=(at+k)*2, row=t>>INST_SHIFT, col=t&(INST_W-1);
      gl.readPixels(col,row,2,1,gl.RGBA,gl.FLOAT,out.subarray(k*8,k*8+8));
    }
    gl.bindFramebuffer(gl.FRAMEBUFFER,null);
    gl.deleteFramebuffer(fb);
    return out;
  }
  /** The per-batch texture state every pass that draws meshes shares (round 17h): the
      base map, the alpha test, the detail map on its second unit, and the material's
      own alpha. `pr` is the program, `b` the batch. */
  /* ---- uniforms sent only when they change (round 18cr) --------------------------------
   *
   *  Measured on the 49-cell Balmora load: 1,813 batches on screen became 71,700 WebGL
   *  calls a frame, of which 27,000 were `uniform1i` and 2,800 were a full 4×4 `uAnim`
   *  that is the identity on nearly every batch. The scene has 354 textures and a handful
   *  of material states, so nearly all of that was the same value sent again. Each
   *  program keeps what it was last told (`pr.s`), and a setter says nothing when the
   *  value has not moved.
   *
   *  The shadow is only as good as the pass's discipline, so it is *reset* wherever a
   *  pass takes a program (`_shReset`) and everything inside that pass that touches one
   *  of these uniforms goes through the setters. Texture unit 0 is shadowed the same way
   *  (`_tex0`), reset by `_bindLights` at the head of every pass, and only `_meshTex`
   *  binds unit 0 inside the passes that read it. Output is the output it was: the same
   *  values reach the shader, a few thousand calls later. */
  _shReset(pr){ pr.s=Object.create(null); }
  _u1i(pr,name,v){
    const s=pr.s||(pr.s=Object.create(null));
    if(s[name]===v) return;
    s[name]=v; this.gl.uniform1i(pr.u[name],v);
  }
  _u1f(pr,name,v){
    const s=pr.s||(pr.s=Object.create(null));
    if(s[name]===v) return;
    s[name]=v; this.gl.uniform1f(pr.u[name],v);
  }
  _u2(pr,name,a){
    const s=pr.s||(pr.s=Object.create(null)), o=s[name];
    if(o && o[0]===a[0] && o[1]===a[1]) return;
    s[name]= o? (o[0]=a[0], o[1]=a[1], o) : [a[0],a[1]];
    this.gl.uniform2fv(pr.u[name],a);
  }
  _u3(pr,name,a){
    const s=pr.s||(pr.s=Object.create(null)), o=s[name];
    if(o && o[0]===a[0] && o[1]===a[1] && o[2]===a[2]) return;
    s[name]= o? (o[0]=a[0], o[1]=a[1], o[2]=a[2], o) : [a[0],a[1],a[2]];
    this.gl.uniform3fv(pr.u[name],a);
  }
  _u4(pr,name,a){
    const s=pr.s||(pr.s=Object.create(null)), o=s[name];
    if(o && o[0]===a[0] && o[1]===a[1] && o[2]===a[2] && o[3]===a[3]) return;
    s[name]= o? (o[0]=a[0], o[1]=a[1], o[2]=a[2], o[3]=a[3], o) : [a[0],a[1],a[2],a[3]];
    this.gl.uniform4fv(pr.u[name],a);
  }
  /** The grass batch's three blade numbers, as one `uniform3f` when any of them moved. */
  _uBlade(pr,b){
    const s=pr.s||(pr.s=Object.create(null)), o=s.uBlade;
    const x=b.bladeLo||0, y=b.bladeH||0, z=b.lift||0;
    if(o && o[0]===x && o[1]===y && o[2]===z) return;
    s.uBlade= o? (o[0]=x, o[1]=y, o[2]=z, o) : [x,y,z];
    this.gl.uniform3f(pr.u.uBlade,x,y,z);
  }
  /** The identity, sent once; anything else, sent every time (an animated node moves). */
  _uAnim(pr,m){
    const s=pr.s||(pr.s=Object.create(null));
    if(!m){ if(s.uAnim==='I') return; s.uAnim='I'; this.gl.uniformMatrix4fv(pr.u.uAnim,false,ANIM_IDENT); return; }
    s.uAnim=null; this.gl.uniformMatrix4fv(pr.u.uAnim,false,m);
  }
  /** Unit 0's texture, bound only when it changes. `undefined` means "unknown, bind". */
  _bindTex0(t){
    if(this._tex0===t) return;
    const gl=this.gl;
    this._tex0=t;
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D,t);
  }
  /** A map unit's texture (3 dark, 5 detail, 6 glow), bound only when it changes - round
   *  18cs, after a thousand such binds a frame on Robin's load. The shadow is forgotten
   *  with unit 0's, at the head of every pass (`_bindLights`), and wherever a pass parks
   *  something else on one of these units (the sky's stars on 5, the water's scene on 6). */
  _bindUnit(u,t){
    const s=this._texU||(this._texU=[]);
    if(s[u]===t) return;
    const gl=this.gl;
    s[u]=t;
    gl.activeTexture(gl.TEXTURE0+u);
    gl.bindTexture(gl.TEXTURE_2D,t);
    gl.activeTexture(gl.TEXTURE0);
  }
  /** Round 18df: the base map's edges, as the shape's NiTexturingProperty asks - bit 0
   *  clamps S, bit 1 clamps T, 0 wraps both. Set on the texture object itself, which is
   *  bound on unit 0 by the time this is called, and remembered there (`_wrap`), so a
   *  texture every shape wraps is never touched and one that two shapes disagree about
   *  is switched at the draw. A sampler object would do it without touching the texture,
   *  but a sampler left on unit 0 outlives the mesh pass - the water, the occlusion and
   *  the particles all read unit 0 with textures of their own filters - and a mip filter
   *  on a texture without mips is a black draw. The land binds its tiles below and puts
   *  any tile a mesh clamped back to wrapping. 447 vanilla static shapes clamp. */
  _wrapTex0(t,bits){
    const gl=this.gl;
    gl.activeTexture(gl.TEXTURE0);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,(bits&1)? gl.CLAMP_TO_EDGE : gl.REPEAT);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,(bits&2)? gl.CLAMP_TO_EDGE : gl.REPEAT);
    t._wrap=bits;
  }
  /** The same on another unit's bound texture (round 18dl: the environment map, whose
   *  NiTextureEffect carries its own clamp mode - 0 clamp both, 1 clamp S, 2 clamp T, 3
   *  wrap - given here as the bits `_wrapTex0` takes: bit 0 clamps S, bit 1 clamps T). */
  _wrapUnit(u,t,bits){
    const gl=this.gl;
    gl.activeTexture(gl.TEXTURE0+u);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,(bits&1)? gl.CLAMP_TO_EDGE : gl.REPEAT);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,(bits&2)? gl.CLAMP_TO_EDGE : gl.REPEAT);
    gl.activeTexture(gl.TEXTURE0);
    t._wrap=bits;
  }
  _meshTex(pr,b){
    const gl=this.gl;
    // Round 18ag: a skinned shape is posed for this instant before any pass draws it.
    if(b.skin) this._skinUpdate(b.skin, this._uvSecs||0);
    this._bindTex0(b.tex||this.white);
    if(b.tex && (b.tex._wrap|0)!==(b.clamp|0)) this._wrapTex0(b.tex, b.clamp|0);
    this._u1i(pr,'uHasTex',b.tex?1:0);
    this._u1i(pr,'uAlphaFn',b.tex?b.alpha.fn:-1);
    this._u1f(pr,'uAlphaRef',b.alpha.ref);
    /* Round 17y: the material's alpha at this instant when a NiAlphaController fades it,
       else the resting value. And the animated node's transform, the identity for the
       still majority - a mat4 uniform starts out all zeros, so it is set on every batch
       (18cr: once per pass, now that the program remembers). */
    // Round 18ai: a phase bin's batch reads the clock shifted into its clip - and, 18aj,
    // at its own rate.
    const secs=(this._uvSecs||0)*(b.animSpeed||1)+(b.animShift||0);
    if(pr.u.uMatAlpha){
      const rest=b.blend? b.blend.alpha : 1;
      this._u1f(pr,'uMatAlpha', b.anim? animAlphaAt(b.anim, secs, rest) : rest);
    }
    if(pr.u.uAnim) this._uAnim(pr, b.anim? animMatrixAt(b.anim, secs) : null);
    /* Wraithguard: the mesh viewer's per-map switches ({detail, dark, decal, env, glow,
       normal, specular} - true is off). */
    const off=this.opts.mapsOff||NO_MAPS_OFF;
    if(pr.u.uHasDetail){
      const d=off.detail? null : (b.detail||null);
      this._u1i(pr,'uHasDetail',d?1:0);
      if(d){ this._bindUnit(5,d); this._u1i(pr,'uDetail',5); }
    }
    // Round 17y: the dark map, on unit 3 - a Glow in the Dahrk window's colour.
    if(pr.u.uHasDark){
      const d=off.dark? null : (b.dark||null);
      this._u1i(pr,'uHasDark',d?1:0);
      if(d){ this._bindUnit(3,d); this._u1i(pr,'uDark',3); }
    }
    /* Round 18df: the decal, on unit 12 - a poster, a sign's board. Not 11: that is
       INST_UNIT, the groups' instance texture, and a decal bound over it would have drawn
       every sign's instances out of the poster (found in 18dg, before any sign was). */
    if(pr.u.uHasDecal){
      const d=off.decal? null : (b.decal||null);
      this._u1i(pr,'uHasDecal',d?1:0);
      if(d){ this._bindUnit(DECAL_UNIT,d); this._u1i(pr,'uDecal',DECAL_UNIT); }
    }
    // Round 18dl: the environment map on unit 13, clamped as its effect says, and the
    // bump map on 14 with its luma and matrix.
    if(pr.u.uHasEnv){
      const e=off.env? null : (b.env||null);
      this._u1i(pr,'uHasEnv',e?1:0);
      if(e){
        this._bindUnit(ENV_UNIT,e); this._u1i(pr,'uEnv',ENV_UNIT);
        if((e._wrap|0)!==(b.envClamp|0)) this._wrapUnit(ENV_UNIT, e, b.envClamp|0);
        const bm=b.bump||null;
        this._u1i(pr,'uHasBump',bm?1:0);
        if(bm){ this._bindUnit(BUMP_UNIT,bm); this._u1i(pr,'uBump',BUMP_UNIT);
                this._u2(pr,'uBumpLuma',b.bumpLuma); this._u4(pr,'uBumpMat',b.bumpMat); }
      }
    }
    /* The glow map, on the same terms as the emissive colour beside it: only while the
       lamps are lit. In OpenMW it is on at noon too - the glow only *reads* as a glow
       because the surroundings are dark - but the hours are Robin's rule and the two
       halves of one lamp should not disagree about whether it is burning. Round 17p. */
    if(pr.u.uHasNrm){
      const nm=(this.opts.normalMaps===true && !off.normal)? (b.nrm||null) : null;
      this._u1i(pr,'uHasNrm', nm?1:0);
      if(pr.u.uNrmRG) this._u1i(pr,'uNrmRG', (nm && b.nrmRG)?1:0);
      if(pr.u.uNrmH) this._u1i(pr,'uNrmH', (nm && b.nrmH)?1:0);
      if(nm) this._bindUnit(NRM_UNIT,nm);
      this._u1i(pr,'uNrm',NRM_UNIT);
    }
    if(pr.u.uHasSpec){
      const sp=(this.opts.normalMaps===true && !off.specular)? (b.spec||null) : null;
      this._u1i(pr,'uHasSpec', sp?1:0);
      if(sp) this._bindUnit(SPEC_UNIT,sp);
      this._u1i(pr,'uSpec',SPEC_UNIT);
    }
    if(pr.u.uViewMode) this._u1i(pr,'uViewMode', this.opts.viewMode|0);
    /* Keyed on the view mode, not on uViewBasis: the shader no longer reads that matrix,
       so the compiler drops it, its location is null, and gating on it left the key light,
       its strength and the ambient at zero - Flat colour and Relief drew black. */
    if((this.opts.viewMode|0)>0){
      if(pr.u.uViewBasis) this.gl.uniformMatrix3fv(pr.u.uViewBasis,false,this.viewBasis());
      /* The key light, turned with the camera: `angle` round the view and `elevation`
         above it, as the old viewer's drag set them - so the light stays where it was put
         relative to what is seen while the view orbits. */
      const kl=this.opts.keyLight||{angle:0.6, elevation:0.5, key:1.4, ambient:0.25};
      const B=this.viewBasis(), a=+kl.angle||0, e=+kl.elevation||0;
      const x=Math.cos(e)*Math.sin(a), y=Math.sin(e), z=Math.cos(e)*Math.cos(a);
      if(pr.u.uKeyDir) this.gl.uniform3f(pr.u.uKeyDir, B[0]*x+B[3]*y+B[6]*z, B[1]*x+B[4]*y+B[7]*z, B[2]*x+B[5]*y+B[8]*z);
      if(pr.u.uKeyInt) this.gl.uniform1f(pr.u.uKeyInt, kl.key==null? 1.4 : +kl.key);
      if(pr.u.uAmbInt) this.gl.uniform1f(pr.u.uAmbInt, kl.ambient==null? 0.25 : +kl.ambient);
      if(pr.u.uHasGloss){
        const gl2=off.gloss? null : (b.gloss||null);
        this._u1i(pr,'uHasGloss', gl2?1:0);
        // Unit 1: the only one the static program leaves free (7-9 are the lamps).
        if(gl2){ this._bindUnit(1,gl2); this._u1i(pr,'uGloss',1); }
      }
    }
    if(pr.u.uHasGlow){
      /* Round 17w: sent whenever the shape has one. Whether it shows is the shader's, per
         instance - a lamp by its own hours, anything else always (see vLamp). */
      const g=off.glow? null : (b.glow||null);
      this._u1i(pr,'uHasGlow', g?1:0);
      if(g){ this._bindUnit(6,g); this._u1i(pr,'uGlow',6); }
    }
    /* Round 17m: the shape's texture matrix at this instant, and its glow. `_uvSecs` is
       the clock the whole frame shares, so the two sheets of a waterfall stay in the
       phase relationship the file gave them. */
    if(pr.u.uUVXform){
      const x=b.uvAnim? uvAt(b.uvAnim, this._uvSecs||0) : null;
      this._u4(pr,'uUVXform', x||UV_IDENT);
    }
    // Round 18dj: and the other maps' transform, with the maps it moves (see uUVMaps2).
    if(pr.u.uUVMaps2){
      const a=b.uvAnim2||null;
      this._u1i(pr,'uUVMaps2', a? a.maps|0 : 0);
      if(a) this._u4(pr,'uUVXform2', uvAt(a, this._uvSecs||0));
    }
    if(pr.u.uEmissive){
      // Round 17w: always sent; the shader gates it per instance, like the glow map.
      this._u3(pr,'uEmissive', b.emissive||BLACK3);
    }
    this._u3(pr,'uDiffuse',b.diffuse);
    // Round 18de: and the material's ambient, its own or the diffuse (see the shader).
    if(pr.u.uMatAmbient) this._u3(pr,'uMatAmbient',b.ambient||b.diffuse);
    if(pr.u.uVColHas) this._u1i(pr,'uVColHas', b.vcolHas? 1 : 0);
  }
  /** How a shape is drawn, per batch (round 17w): whether it is lit, what its vertex
   *  colours are for, and which of its faces are drawn.
   *
   *  The game culls back faces on every shape unless a NiStencilProperty says both sides,
   *  and so does OpenMW. The preview drew both sides of everything outdoors — "a badly
   *  wound mod mesh away from a hole", the note said — and the Vivec waterfall showed why
   *  that is the wrong call for a translucent sheet: where it folds over the lip it was
   *  drawn twice, back over front, and the edge of the second copy was a hard line across
   *  the water. Robin had already said it, two rounds before: "Morrowind renders statics
   *  with back-face culling."
   *
   *  `cull` false leaves the face state alone — the grass pass is two-sided by nature and
   *  handles its own. */
  _meshMode(pr,b,cull){
    const gl=this.gl;
    if(pr.u.uUnlit)    this._u1i(pr,'uUnlit', b.unlit?1:0);
    if(pr.u.uVColMode) this._u1i(pr,'uVColMode', b.vcolMode==null? 2 : b.vcolMode);
    if(pr.u.uDayNight) this._u1i(pr,'uDayNight', b.dayNight|0);
    if(cull){
      const dm=b.drawMode|0;
      if(dm===3){ if(this._culling){ gl.disable(gl.CULL_FACE); this._culling=false; } }
      else      { if(!this._culling){ gl.enable(gl.CULL_FACE); this._culling=true; } }
      const cw=(dm===2);
      if(cw!==this._cw){ gl.frontFace(cw? gl.CW : gl.CCW); this._cw=cw; }
    }
  }
  /** The two hours a lamp at scene position `p` lights and goes out at — the stagger of
   *  round 17s, said once so the point light, the glow map and the flame all read the same
   *  two numbers. Robin: "toggle the glow maps at the same time as the light for the
   *  different meshes, so they come and go together." */
  lampHours(p){
    /* Round 18i shipped The Midnight Oil's hours here for a day, read from the mod's
       config where the load order ran it, and Robin took them out: "I deliberately don't
       want Midnight Oil settings for toggling lights to be attached to the viewport in
       the engine, as I instead added the three options (Night time, Always, and Off, with
       some exceptions added)." The Lights picker is the rule. */
    const frac=(a,b,c)=>{ const v=Math.sin(a*12.9898+b*78.233+c)*43758.5453; return v-Math.floor(v); };
    return [LAMP_ON_FIRST + frac(p[0], p[1], 0.0)*(LAMP_ON_LAST-LAMP_ON_FIRST),
            LAMP_OFF_FIRST+ frac(p[1], p[0], 4.7)*(LAMP_OFF_LAST-LAMP_OFF_FIRST)];
  }
  /** Is a lamp with these two hours burning at hour `h`, under the current Lights rule? */
  lampLitAt(on,off,h){
    if(this.opts.lights==='always') return true;
    if(this.opts.lights==='off') return false;
    /* Round 17y: no hours at all means always - a fire, or any light in a room. Robin:
       "Campfires (like Furn_De_Firepit_F_400) and fireplaces should always be on (both
       particle system and light)"; "All other light sources in interiors [...] should
       always be on (both light and particle systems)." Off still puts them out: that
       switch means no lamps at all. */
    if(on<0) return true;   // -2 on the instance since round 18i; -1 is "no lamp"

    if(h==null) return false;
    return h>=on || h<off;
  }
  /** Which hours the cell's lamps burn (round 17m).
   *
   *  Robin: "ONLY when it's dark (lets say according to the time of day slider, make them
   *  be on when it is between 18 in the evening and 7 in the morning, and off during the
   *  days)." The game itself never switches an exterior light off — OpenMW's
   *  `MWClass::Light` gates only on the record's OffDefault flag and knows nothing about
   *  the hour — so this is the preview's own rule, and it is his. */
  lightsLit(){
    /* Round 17s: and whether the hours are consulted at all. Robin asked for a picker —
       "Off, Always, Night time" — where night time is the rule that was already here.
       Anything unrecognised means the rule, so an older profile that never heard of this
       setting keeps behaving as it did.

       This answers for the *scene*: is it the part of the day when lamps burn at all.
       Which lamps are lit at this moment inside that window is `lampLit` below. */
    const m=this.opts.lights;
    if(m==='off') return false;
    if(m==='always') return true;
    // Round 17y: a fire burns at noon, and a window lights a room by day, so a scene with
    // either in it is never "no lamps".
    if(this.lights && this.lights.some(L=>L && (L.always || L.window))) return true;
    const h=this.opts.hour;
    if(h==null) return false;
    return h>=LAMP_ON_FIRST || h<LAMP_OFF_LAST;
  }
  /* =====================================================================================
     Round 18q: the cull mask's uniforms, and the field behind two of them.

     `opts.cull` is what the mask is asking, worked out by the page from the selected
     rule and, while one is hovered, the card that narrows it (`cullFor` in 09_ui.js):
     `{slope:[lo,hi], h:[lo,hi], face:[at,width]|null, shore:[lo,hi], curve:[lo,hi],
       obs:[lo,hi], canopy:{bits,under}|null}`, every band a pair with ±1e9 for "not
       asked". The field is set by `setCullField`, and is the engine's answer for the
     cells on screen.
     ===================================================================================== */
  setCullField(f){
    const gl=this.gl;
    if(this.cullField && this.cullField.tex){ try{ gl.deleteTexture(this.cullField.tex); }catch(_){ } }
    this.cullField=null;
    if(!f || !f.data || !f.w || !f.h) return;
    const tex=gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D,tex);
    gl.pixelStorei(gl.UNPACK_ALIGNMENT,1);
    gl.texImage2D(gl.TEXTURE_2D,0,gl.RGB8,f.w,f.h,0,gl.RGB,gl.UNSIGNED_BYTE,f.data);
    gl.pixelStorei(gl.UNPACK_ALIGNMENT,4);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.CLAMP_TO_EDGE);
    gl.bindTexture(gl.TEXTURE_2D,null);
    this.cullField={tex, box:f.box.slice(), reach:f.reach||1};
    this.dirty=true;
  }
  /** Round 18q: what the mask is asking, as one object.
   *
   *  `opts.cull` is the whole answer wherever the page has worked one out (`cullFor`).
   *  Anything that only ever set the three limits the mask grew out of - a thumbnail, a
   *  test, a viewport nobody has given a rule to - is still described by them, so the
   *  shade and the legend cannot disagree about what is being refused. One reading, used
   *  by both: they were two, and drifting apart is what round 18q spent an afternoon on.
   *  Returns null when nothing is being refused. */
  cullAsk(){
    const o=this.opts;
    if(o.cull) return o.cull;
    const lo=o.minH==null? -1e9 : o.minH, hi=o.maxH==null? 1e9 : o.maxH;
    const top=o.maxAngle==null? 90 : o.maxAngle;
    if(top>=90 && lo<=-1e9 && hi>=1e9) return null;
    return {slope:[0,top], h:[lo,hi], shore:[-1e9,1e9], curve:[-1e9,1e9], obs:[-1e9,1e9],
            face:null, face2:null, canopy:null};
  }

  /** The mask's uniforms on one program. Unit 10 carries the field. */
  _cullUniforms(pr,on){
    const gl=this.gl, u=pr.u;
    if(!u.uCullOn) return;
    let c=(on && this.cullAsk())||null;
    /* In the mesh editor the ground is one mesh at the origin: its height means nothing,
       and the curvature, the shoreline, the distance to other objects and what hangs over
       it have no answer here at all - which is the same reading the engine takes on a
       rock's face (`mesh_terrain`, and the mesh pass's `None` for the two). Only the
       steepness and the facing carry over, so only those are asked. */
    if(c && this.opts.inspecting){
      c={slope:c.slope, h:[-1e9,1e9], shore:[-1e9,1e9], curve:[-1e9,1e9], obs:[-1e9,1e9],
         face:c.face, face2:c.face2, canopy:null};
    }
    const F=(c && !this.opts.inspecting)? this.cullField : null;
    /* Every one of these is set only where the program kept it. A GLSL compiler is free
       to drop a uniform whose branch it can prove does nothing, and the two programs that
       share this block do not always keep the same ones - and `uniform3f` on a location
       that is not there is an error the driver puts on the queue for somebody else's test
       to find, which is exactly how this was found. */
    const f1=(n,a)=>{ if(u[n]) gl.uniform1f(u[n],a); };
    const i1=(n,a)=>{ if(u[n]) gl.uniform1i(u[n],a); };
    const f2=(n,a,b)=>{ if(u[n]) gl.uniform2f(u[n],a,b); };
    const f3=(n,a,b,d)=>{ if(u[n]) gl.uniform3f(u[n],a,b,d); };
    const f4=(n,a,b,d,e)=>{ if(u[n]) gl.uniform4f(u[n],a,b,d,e); };
    f2('uCullOn', c?1:0, (c&&F)?1:0);
    const pair=(name,v,dlo,dhi)=>f2(name, v? v[0] : dlo, v? v[1] : dhi);
    pair('uCullSlope', c&&c.slope, 0, 90);
    pair('uCullH',     c&&c.h,   -1e9, 1e9);
    pair('uCullShore', c&&c.shore, -1e9, 1e9);
    pair('uCullCurve', c&&c.curve, -1e9, 1e9);
    pair('uCullObs',   c&&c.obs,  -1e9, 1e9);
    const arc=(name,v)=>f3(name, v? v[0] : 0, v? v[1] : 180, v?1:0);
    arc('uCullFace',  c&&c.face);
    arc('uCullFace2', c&&c.face2);
    const k=(c&&c.canopy)||null;
    f4('uCullCanopy', k? (k.under|0) : 0, k? (k.clear|0) : 0,
       (k&&k.underAny)?1:0, (k&&k.clearAny)?1:0);
    f4('uFieldXf', F? F.box[0] : 0, F? F.box[1] : 0,
       F? 1/Math.max(1,F.box[2]-F.box[0]) : 0,
       F? 1/Math.max(1,F.box[3]-F.box[1]) : 0);
    f1('uFieldReach', F? F.reach : 1);
    if(u.uField){
      /* Unit 10: 6 is a shape's glow map, 5 its detail, 7-9 the light grid, and two
         samplers of different kinds on one unit is an error the driver is entitled to
         raise on every draw after it. */
      i1('uField',10);
      gl.activeTexture(gl.TEXTURE10);
      gl.bindTexture(gl.TEXTURE_2D, (F&&F.tex)||this.white);
      gl.activeTexture(gl.TEXTURE0);
    }
  }

  /** Round 18r: the pointer, hidden and kept in the middle of the viewport.
   *
   *  Robin: "Can we center the cursor on the viewport, while hiding it, and just don't
   *  take input from left mouse button, while we hold right mouse button? That is the
   *  effect we want."
   *
   *  It is, and it is what a native game does. The pointer lock would give the same thing
   *  for nothing, but every engine that draws this window announces it - "If you want to
   *  see the cursor, press ESC", `tauri.localhost` and all - and no page can suppress
   *  that. So: hide the pointer, and put it back in the middle whenever it wanders a
   *  third of the way out. Nothing is locked and nothing is announced.
   *
   *  `show` is `true` to reveal, `false` to hide, `null` to leave the visibility alone -
   *  which is what a mid-turn park wants, because a visibility flag toggled fifteen times
   *  a second is a good way to find out that some platform counts them rather than
   *  storing them. `at` is where to put it, in client pixels; the middle of the canvas
   *  when it is left out.
   *
   *  `onCursor` is the shell's, and there is none in a browser: `_park` then does nothing
   *  at all and the CSS `nocursor` class is the whole of the effect, with the turn ending
   *  at the edge of the screen as it must. */
  _park(show, at){
    /* Nothing to ask: in a browser this is the whole of the method, the CSS class is the
       whole of the effect, and the turn is the plain difference of two positions with the
       edge of the screen at the end of it. */
    if(!this.onCursor) return null;
    /* `at` null is "do not move it" - which is what pressing the button does, and the
       whole of round 18r's second fix. See the press handler. */
    if(at) this._lookPark={at:at.slice(), t:performance.now()};
    try{
      const r=this.onCursor(show, at? Math.round(at[0]) : null, at? Math.round(at[1]) : null);
      return (r && r.then)? r.catch(()=>{}) : null;
    }catch(_){ return null; }
  }

  /** Round 18q: whether the nocturnal moths are out.
   *
   *  Robin: "Use the same time as night lights, even if the lights are always on, turn on
   *  the moths only during the same times." So the *hour* rule of the Lights picker's
   *  "Night time" answers this, whatever the picker is actually set to - a scene lit at
   *  noon by Always has no moths in it - and Off puts them out with everything else.
   *
   *  And the mod's own weather rule: moths in clear, cloudy, foggy and overcast, none in
   *  rain, ash, blight, a storm or snow (`util.isNiceWeather`). The viewport offers only
   *  Clear today, so this passes; it is here so it stays right if the others come back.
   *
   *  They are an effect on a lamp, so the Particles switch carries them - Robin asked for
   *  them there rather than under a toggle of their own. */
  mothsVisible(){
    const o=this.opts;
    if(o.particles===false || o.moths===false) return false;
    if(o.lights==='off') return false;
    if(this.underNow) return false;   // 18dt: none seen from under the water (Robin: "don't draw under water")
    const w=String(o.weather||'Clear');
    if(!/^(clear|cloudy|foggy|overcast)$/i.test(w)) return false;
    const h=o.hour;
    if(h==null) return false;
    return h>=LAMP_ON_FIRST || h<LAMP_OFF_LAST;
  }
  /** Is *this* lamp burning at this hour?
   *
   *  Round 17s, Robin: "For the Night time option for the lights turning on, make the
   *  exact time a light start stagger a bit, with up to one in world hours difference,
   *  but no earlier than 18.00. Same for turning off, but no later than 6.30."
   *
   *  So a lamp lights somewhere in 18:00–19:00 and goes out somewhere in 05:30–06:30,
   *  and the two are independent — a lamp lit late need not be put out late. The offsets
   *  come from the lamp's own position rather than from a counter, so they are the same
   *  every frame, the same after a reload, and the same for you as for me: nothing to
   *  store, and no flicker from a list that reorders. Two different mixes of the
   *  coordinates, so the two hours do not move together.
   *
   *  On "Always" and "Off" there is nothing to stagger — the scene answers for them. */
  lampLit(L,h){
    // Round 17y: a fire, or a room's lamp, keeps no hours (see lampLitAt).
    if(L.always) return this.opts.lights!=='off';
    // Round 17y: a window's light into a room burns while the sun is up outside.
    if(L.window){ if(this.opts.lights==='off') return false; const w=this.windowLight(L,h); return !!w && w.k>0.001; }
    if(L.on==null){ const [on,off]=this.lampHours(L.p); L.on=on; L.off=off; }
    return this.lampLitAt(L.on,L.off,h);
  }
  /** Glow in the Dahrk's light into a room (round 17y): how strong, and what colour, a
   *  window's point light is at hour `h`.
   *
   *  The mod's interop.lua, `resetConfigurableState`: while the sunlit (INT-DAY) branch
   *  shows, a light is attached at the mesh's AttachLight — the file's own light when it
   *  has one, else white at radius 200 — with `diffuse = light.diffuse * regionSunColor`
   *  (the weather's sun colour for the hour, never shorter than 0.4) and a dimmer that is
   *  the weather's brightness (Clear 1.0) between the sunrise and sunset midpoints, ramps
   *  up from sunriseStart to the sunrise midpoint and down from the sunset midpoint to
   *  sunsetStop, and is nothing at night. The midpoints are the weather controller's:
   *  sunriseStart = Sunrise Time - Sun Pre-Sunrise, the total = pre + Sunrise Duration +
   *  post, the midpoint halfway; the same for sunset. `L.off` carries the window's own
   *  sunriseStart, variance included, so the shift the mod gives each window lands here too. */
  windowLight(L,h){
    if(h==null) return null;
    const T=(typeof Sky==='object' && Sky.timing)? Sky.timing() : null;
    const sunrise=T? T.sunrise : 6, sunset=T? T.sunset : 18;
    const riseDur=T? T.sunriseDuration : 2, setDur=T? T.sunsetDuration : 2;
    const [preR,postR,preS,postS]=T? T.sun : [0,0,1,1.25];
    const riseStart=sunrise-preR, riseMid=riseStart+(postR+riseDur+preR)/2;
    const setStart=sunset-preS, setTotal=postS+setDur+preS, setMid=setStart+setTotal/2, setStop=setStart+setTotal;
    const v=(L.off!=null)? riseStart-L.off : 0;   // the window's variance, as windowHours wrote it
    const hh=h+v;
    let k;
    if(riseMid<hh && hh<setMid) k=1;
    else if(riseStart<=hh && hh<=riseMid) k=riseMid>riseStart? (hh-riseStart)/(riseMid-riseStart) : 1;
    else if(setMid<=hh && hh<=setStop) k=setStop>setMid? (setStop-hh)/(setStop-setMid) : 0;
    else k=0;
    const now=(typeof Sky==='object')? Sky.at(h) : null;
    let sc=now? now.sun.slice() : [1,1,1];
    const len=Math.hypot(sc[0],sc[1],sc[2])||1;
    if(len<0.4){ sc=[sc[0]/len*0.4, sc[1]/len*0.4, sc[2]/len*0.4]; }
    const wb={clear:1, cloudy:0.9, foggy:0.5, overcast:0.6, rain:0.4, thunderstorm:0.3, ashstorm:0.5, blight:0.5, snow:0.7, blizzard:0.6};   // the game's weather names (Thunderstorm, Ashstorm)
    const bright=wb[String((now&&now.weather)||'clear').toLowerCase()]||1;
    k*=bright;
    return {k, col:[L.c[0]*sc[0]*k, L.c[1]*sc[1]*k, L.c[2]*sc[2]*k]};
  }
  /** The lamps this frame draws by, packed for the shaders.
   *
   *  At most `maxLights` of them, brightest where the camera stands first: a
   *  Vivec canton has more lanterns than any sensible uniform array, and the ones behind
   *  you contribute nothing you can see. OpenMW does the same thing per object with its
   *  `max lights` (8 in 0.49); this is per frame, which is coarser and enough for a
   *  preview - the budget is `maxLights`, what the card would carry, rather than a flat
   *  number. Radius comes floored at 16 from the engine, as `createLightSource` floors
   *  it. A negative light — the Negative flag, "darkness" — is sent with its sign in the
   *  colour's w, which is how OpenMW subtracts it. */
  _lightUniforms(){
    const on=this.lightsLit();
    this._lightsOn=on;
    /* Each lamp's own hour, inside the window `lightsLit` opened (round 17s). A lamp
       whose hour has not come round yet is simply not in the frame's list. */
    const h=this.opts.hour;
    const src=(on && this.lights && this.lights.length)
      ? this.lights.filter(L=>this.lampLit(L,h)) : null;
    /* The grid has to be emptied here as well as filled below. It is *state*, not a
       return value: leaving last frame's grid in place while `_lights` went null meant
       the shader went on reading it — the lamps stayed lit with the lamps turned off,
       and a test that measured "with the lamp" against "without it" measured nothing. */
    if(!src || !src.length){ this._lights=null; if(this._lgrid) this._lgrid.n=0; return; }
    const cap=this.maxLights||LIGHT_BUDGET;
    /* What a lamp is worth where the camera stands: the shader's own illumination term,
       `r/(3*dist)`. Both the ranking and the fade are this one number, which is why they
       agree. With the grid (round 17r) the budget is generous and this rarely bites at
       all — a vanilla exterior runs to about forty lamps across nine cells — but a heavily
       modded canton can still exceed it, and when it does the tail fades rather than
       vanishing. */
    const e=this.cameraEye();
    const lit=L=>L.r/Math.max(3*Math.hypot(L.p[0]-e[0],L.p[1]-e[1],L.p[2]-e[2]),1e-4);
    let list=src, cut=0;
    if(list.length>cap){
      const all=src.slice().sort((a,b)=>lit(b)-lit(a));
      cut=lit(all[cap]);                 // the brightest lamp that did not fit
      list=all.slice(0,cap);
    }
    const n=list.length;
    const p=new Float32Array(n*4), col=new Float32Array(n*4), att=new Float32Array(n*4);
    const lm=this.lampModel();
    for(let i=0;i<n;i++){
      const L=list[i];
      /* Round 18i: the light's attenuation, as the engine hands it to its renderer -
         `[LightAttenuation]` through NiPointLight::setAttenuationForRadius - and, under
         MGE's per-pixel lighting, as its FixedFuncEmu decodes that (`mgeDecode`). */
      const co=lampCoefficients(lm.att, L.r, !!this.opts.room);
      const dec= lm.model===1? mgeDecode(co, L.c) : {c:co[0], l:co[1], q:co[2], amb:0, col:L.c};
      att[i*4]=dec.c; att[i*4+1]=dec.l; att[i*4+2]=dec.q; att[i*4+3]=dec.amb;
      /* The tail of the budget is faded out rather than dropped, by what a lamp is worth
         against the worth of the one that did not fit. Round 17n faded by *rank* and the
         bound that made it safe stopped holding once the budget grew: the cut fell among
         lamps bunched together, where a degree of orbit reshuffles six places at once and
         the worst step went from a twelfth to a half. Measuring against the cut's own
         illumination instead, the two move together, and lamps bunched at the cut — the
         ones rank handles worst — are exactly the ones whose ratio to it barely changes.
         Worst step on eighty lamps: 0.06. On four hundred: 0.15, where rank gives 0.83. */
      const f=cut<=0? 1 : quickstep((lit(L)/cut - 1)/(LIGHT_FADE_K-1));
      // Round 17y: a window's light takes the sun's colour and the hour's fade (windowLight).
      const cc=L.window? ((this.windowLight(L,h)||{}).col||L.c) : dec.col;
      p[i*4]=L.p[0]; p[i*4+1]=L.p[1]; p[i*4+2]=L.p[2]; p[i*4+3]=L.r;
      col[i*4]=cc[0]*f; col[i*4+1]=cc[1]*f; col[i*4+2]=cc[2]*f; col[i*4+3]=L.neg? -1 : 1;
    }
    this._lights={n, p, c:col, att};
    this._lightGrid(list, p, col, att);
  }
  /** Round 18i: which renderer's lamp falloff the install runs, from the engine's
   *  `renderer` reply (25_sky.js keeps it): `model` 0 the fixed pipeline, 1 MGE XE's
   *  per-pixel lighting, 2 OpenMW's shader methods; `att` the `[LightAttenuation]` the
   *  game reads; `bounds` OpenMW's light bounds multiplier; the sun and ambient
   *  multipliers MGE applies. Without an install it is the fixed pipeline with the
   *  game's shipped attenuation, which is what vanilla is. */
  lampModel(){
    const r=(typeof Sky==='object' && Sky.data && Sky.data.renderer) || null;
    const L=(r && r.lighting) || {};
    const model = L.model==='mge-ppl'? 1 : L.model==='openmw-shaders'? 2 : 0;
    return {model, att:(r && r.attenuation) || null, bounds: model===2? (+L.lightBoundsMultiplier||1.65) : 1,
            clamp: L.clampLighting!==false, engine: r? r.engine : '', label: r? r.label : '',
            sunMult: (model===1 && L.sunMult!=null)? +L.sunMult : 1,
            ambMult: (model===1 && L.ambientMult!=null)? +L.ambientMult : 1};
  }
  /** The lamps as textures, and the grid that says which of them a patch of ground needs.
   *
   *  Round 17r. The old arrangement was two uniform vec4 arrays and a loop over all of
   *  them at every fragment, which is why 128 lamps cost three times the frame of 32. Now:
   *
   *    uLights  three texels a lamp — (xyz, radius), (rgb, sign), and since round 18i
   *             (constant, linear, quadratic, ambient): the install's falloff
   *    uBins    one texel per square column of world, (offset, count) into uIndex
   *    uIndex   the lamp indices, column by column, end to end
   *
   *  A lamp goes into every column its *reach* covers — twice its radius, a margin over
   *  where `gdnLights` stops since round 18h (the radius itself) — so a fragment can loop
   *  its own column and be sure it has missed nothing. What a fragment pays is no longer
   *  the scene's lamp count but the handful overlapping the ground under it.
   *
   *  The grid is sized to the lamps, not to the scene: its origin and extent come from the
   *  lamps' own reach, so an empty quarter of the view costs no columns. LIGHT_GRID_MAX
   *  caps the side, and a scene wider than that gets larger columns rather than a refused
   *  grid — coarser bins, never a wrong answer. */
  _lightGrid(list, p, col, att){
    const gl=this.gl, n=list.length;
    if(!n){ this._lgrid=null; return; }
    // Round 18i: the shader's reach - twice the radius, or twice OpenMW's bounds radius.
    const lm=this.lampModel();
    const reachOf=L=>2*L.r*lm.bounds;
    let x0=Infinity, y0=Infinity, x1=-Infinity, y1=-Infinity;
    for(const L of list){
      const reach=reachOf(L);
      if(L.p[0]-reach<x0) x0=L.p[0]-reach;  if(L.p[0]+reach>x1) x1=L.p[0]+reach;
      if(L.p[1]-reach<y0) y0=L.p[1]-reach;  if(L.p[1]+reach>y1) y1=L.p[1]+reach;
    }
    let bin=LIGHT_BIN;
    let gw=Math.max(1, Math.ceil(Math.max(x1-x0, y1-y0)/bin));
    if(gw>LIGHT_GRID_MAX){ bin=Math.max(x1-x0, y1-y0)/LIGHT_GRID_MAX; gw=LIGHT_GRID_MAX; }
    // Which lamps each column needs. A column is kept as a plain array until the flat
    // list is laid out, because a lamp lands in as many columns as its reach covers.
    const cells=new Array(gw*gw);
    let total=0;
    for(let i=0;i<n;i++){
      const L=list[i], reach=reachOf(L);
      const cx0=Math.max(0, Math.floor((L.p[0]-reach-x0)/bin));
      const cx1=Math.min(gw-1, Math.floor((L.p[0]+reach-x0)/bin));
      const cy0=Math.max(0, Math.floor((L.p[1]-reach-y0)/bin));
      const cy1=Math.min(gw-1, Math.floor((L.p[1]+reach-y0)/bin));
      for(let cy=cy0;cy<=cy1;cy++) for(let cx=cx0;cx<=cx1;cx++){
        const k=cy*gw+cx;
        let a=cells[k]; if(!a){ a=cells[k]=[]; }
        /* Past the per-column cap the lamp is dropped from *that column only*, nearest
           first — the shader's loop is bounded by the same number, so a column that
           overflowed would otherwise have its tail silently ignored anyway. Dropping the
           furthest is the same choice the budget makes, made locally. */
        if(a.length<LIGHT_BIN_MAX){ a.push(i); total++; }
      }
    }
    const bins=new Int32Array(gw*gw*2);
    const index=new Int32Array(Math.max(1,total));
    let at=0;
    for(let k=0;k<gw*gw;k++){
      const a=cells[k];
      bins[k*2]=at; bins[k*2+1]=a? a.length : 0;
      if(a){ for(const i of a) index[at++]=i; }
    }
    // uIndex is laid out as rows so a long list does not run past MAX_TEXTURE_SIZE.
    const iw=Math.min(2048, Math.max(1,total));
    const ih=Math.ceil(Math.max(1,total)/iw);
    const idx=new Int32Array(iw*ih); idx.set(index.subarray(0, Math.min(index.length, iw*ih)));
    const lite=new Float32Array(n*3*4);
    for(let i=0;i<n;i++){
      lite[i*12]=p[i*4]; lite[i*12+1]=p[i*4+1]; lite[i*12+2]=p[i*4+2]; lite[i*12+3]=p[i*4+3];
      lite[i*12+4]=col[i*4]; lite[i*12+5]=col[i*4+1]; lite[i*12+6]=col[i*4+2]; lite[i*12+7]=col[i*4+3];
      // Round 18i: the third texel - constant, linear, quadratic, and the light's ambient term.
      lite[i*12+8]=att? att[i*4] : 0; lite[i*12+9]=att? att[i*4+1] : 0; lite[i*12+10]=att? att[i*4+2] : 0; lite[i*12+11]=att? att[i*4+3] : 0;
    }
    const g=this._lgrid||(this._lgrid={});
    gl.activeTexture(gl.TEXTURE0);   // build on unit 0, not on whatever was last used
    const tex=(name,fn)=>{ if(!g[name]) g[name]=gl.createTexture();
      gl.bindTexture(gl.TEXTURE_2D,g[name]); fn();
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.NEAREST);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.NEAREST);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.CLAMP_TO_EDGE);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.CLAMP_TO_EDGE); };
    tex('lights',()=>gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA32F,n*3,1,0,gl.RGBA,gl.FLOAT,lite));
    tex('bins',  ()=>gl.texImage2D(gl.TEXTURE_2D,0,gl.RG32I,gw,gw,0,gl.RG_INTEGER,gl.INT,bins));
    tex('index', ()=>gl.texImage2D(gl.TEXTURE_2D,0,gl.R32I,iw,ih,0,gl.RED_INTEGER,gl.INT,idx));
    gl.bindTexture(gl.TEXTURE_2D,null);
    g.grid=[x0,y0,bin,gw];
    g.n=n; g.total=total; g.gw=gw; g.bin=bin;
  }
  /** The pivot ball's three rings — the equator and a meridian each way — as plain lines
   *  on the unit sphere.
   *
   *  Round 17j made them solid bands with a cross-section, and round 17m took that back
   *  out at Robin's word twice over: first to screen-space quads for a thickness WebGL's
   *  `lineWidth` will not give, and then, having seen it, to this — "Make it into just
   *  1px line again if it's the only thing supported natively." It is: every desktop
   *  driver clamps `lineWidth` to 1, so a line here is a line.
   *
   *  Two vertices a segment, drawn with `gl.LINES`. Built once, on the unit sphere; the
   *  draw scales it by whatever radius the ball has at this distance. */
  _pivotGeom(){
    if(this._pivot) return this._pivot;
    const gl=this.gl;
    if(!this.progPivot) this.progPivot=this._prog(VS_PIVOT,FS_PIVOT);
    const N=PIVOT_SEGS, v=[];
    const ring=(ax,ay)=>{
      const at=t=>{ const c=Math.cos(t), s=Math.sin(t);
        return [ax[0]*c+ay[0]*s, ax[1]*c+ay[1]*s, ax[2]*c+ay[2]*s]; };
      for(let i=0;i<N;i++){
        const p=at(i/N*Math.PI*2), q=at((i+1)/N*Math.PI*2);
        v.push(p[0],p[1],p[2], q[0],q[1],q[2]);
      }
    };
    ring([1,0,0],[0,1,0]);   // the equator
    ring([1,0,0],[0,0,1]);   // north to south, east-west
    ring([0,1,0],[0,0,1]);   // north to south, north-south
    const nLines=v.length/3;
    /* Round 17m, Robin: "inside the orbit pivot ball, render a solid sphere with a radius
       of 25% of the radius of the orbit ball, to help visualize it." Three hairline rings
       say where the ball's *surface* is; they say much less about where its centre is,
       which is the point the camera actually turns around. A small solid marker at that
       point does, and having something opaque in the middle is what lets the eye read
       which half of each ring is in front.
       An ordinary UV sphere, wound counter-clockwise seen from outside so the back half
       culls. It shares the buffer and the attribute with the rings — both are nothing but
       a position on the unit sphere — and the draw picks it out by offset. */
    const S=20, R=10;   // segments round, rings top to bottom
    const sp=(i,j)=>{
      const th=j/R*Math.PI, ph=i/S*Math.PI*2;
      return [Math.sin(th)*Math.cos(ph), Math.sin(th)*Math.sin(ph), Math.cos(th)];
    };
    for(let j=0;j<R;j++) for(let i=0;i<S;i++){
      const a=sp(i,j), b=sp(i+1,j), c=sp(i+1,j+1), d=sp(i,j+1);
      v.push(a[0],a[1],a[2], c[0],c[1],c[2], b[0],b[1],b[2]);
      v.push(a[0],a[1],a[2], d[0],d[1],d[2], c[0],c[1],c[2]);
    }
    const vao=gl.createVertexArray(); gl.bindVertexArray(vao);
    const b=gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER,b);
    gl.bufferData(gl.ARRAY_BUFFER,new Float32Array(v),gl.STATIC_DRAW);
    gl.enableVertexAttribArray(0); gl.vertexAttribPointer(0,3,gl.FLOAT,false,0,0);
    gl.bindVertexArray(null);
    this._pivot={vao, n:nLines, ballAt:nLines, ballN:v.length/3-nLines};
    return this._pivot;
  }
  /** How big the ball is drawn, in world units. A fixed size in the world, so it takes
   *  the perspective every other object takes and its size on screen says how far away
   *  it is. The stamp on the ground uses the same number, which is what "the same radius
   *  as the ball itself" asks for. */
  /** Wraithguard: the orbit pivot's size - 64 units at most, and less when the camera is
      close (a twentieth of its distance, 8 at least), so inside a room it is a marker and
      not a wall. Still a size in the world, as Robin asked: it shrinks with distance. */
  pivotRadius(){ return Math.max(8, Math.min(PIVOT_R, (+this.cam.dist||1000)*0.05)); }
  /** The camera's right, up and back as the columns of a 3x3 (the mesh viewer's key light). */
  viewBasis(){
    const c=this.cam, ce=Math.cos(c.el), se=Math.sin(c.el), ca=Math.cos(c.az), sa=Math.sin(c.az);
    const right=[-sa, ca, 0], up=[-se*ca, -se*sa, ce], back=[ce*ca, ce*sa, se];
    return new Float32Array([...right, ...up, ...back]);
  }
  setBatches(b){ this.batches=b; this.dirty=true; }

  /* ---- hover highlights ------------------------------------------------------
     Both are pure render state. Nothing is rebuilt, nothing is rescattered — a
     hover has to be free, or moving the pointer down a list would stutter. */

  /** Light up the ground a land texture covers. Pass null to clear. */
  setTexHighlight(id){
    const v=id? String(id).toLowerCase() : null;
    if(this.hlTex===v) return;
    this.hlTex=v; this.dirty=true;
  }
  /** Light up every grass instance a slot placed, dimming the rest. Pass null to clear. */
  setGrassHighlight(key){
    const v=key==null? null : String(key);
    if(this.hlSlot===v) return;
    this.hlSlot=v; this.dirty=true;
  }
  /** The lamps the cell puts in the scene, and whether anything in it moves.
   *
   *  Round 17m. Set by the cell preview when it builds the batches; both are read once a
   *  frame. `_anyUvAnim` is what keeps the loop drawing while a waterfall falls. */
  /** The placed objects, as batches.
   *
   *  Round 17m: and a note of whether any of them moves. A shape with a NiUVController —
   *  a waterfall, a stream ripple, the Gnisis fence's banner — needs a frame every frame
   *  whether or not anything else in the picture has changed; a cell with none of them
   *  goes on drawing only when something asks for it. */
  setStatics(b){
    this.statics=b||[];
    // Round 18q: the objects are in the ground map now, so swapping them changes it.
    this.groundMapStale=true;
    // Round 17y: a moving node or a fading material keeps the frames coming the same way.
    // Round 18ag: and a skinned shape under an animated skeleton, the same way.
    this._anyUvAnim=this.statics.some(x=>x&&(x.uvAnim||x.uvAnim2||x.anim||x.skin));   // `skin` is any posed part (18au: morphs too)
    // Round 18cr: the still ones into material groups, the rest drawn one by one.
    this._buildGroups(this.statics);
    /* And the lamps go with the objects they belong to. Cleared here rather than at each
       of the six places that swap the statics out, so a mesh being inspected on its own -
       or an empty viewport - never lights by the cell you were looking at before. The
       cell preview sets them again straight after this. */
    this.lights=null;
    this.dirty=true;
  }
  /** The painted objects, as batches carrying their own masks. See `FS_STATIC_PAINT`. */
  setPaintedStatics(b){ this.paintedStatics=b||[]; this.dirty=true; }

  /* ---- the material groups (round 18cr) ---------------------------------------------
   *
   *  Robin: "I think rendering is too slow when running 49 cells [...] I am wondering if
   *  we can gain something from batch render statics in the scene?" Measured on that load
   *  (49 cells at Balmora, the vanilla install): 2,775 batches, 1,813 of them on screen
   *  framed wide, **2,799 draw calls and 71,700 WebGL calls a frame** - twenty-five calls
   *  a batch to re-send what the last batch had already said, and a draw per cell run
   *  because WebGL2 has no base instance to start a draw partway along a stream. Half the
   *  batches hold one to four instances; nothing but merging across meshes reaches those.
   *
   *  So the still objects are drawn from **scene-wide buffers**: every drawn part's
   *  vertices laid end to end in one set of buffers under one vertex array, every
   *  instance's matrix (and lamp hours) in one RGBA32F texture the vertex shader fetches
   *  by index, and the batches sorted into **material groups** - one per distinct
   *  (texture, alpha, blend, faces, lighting flags, maps, colours). A group is one
   *  `multiDrawElementsInstancedWEBGL`: each visible cell run of each part in it is a
   *  sub-draw (index range, instance count), and `gl_DrawID` picks that run's first
   *  instance out of a uniform array. The cull is exactly 17r's, per run; it just no
   *  longer costs a draw and three pointer moves per run. Without the extension (on by
   *  default in Chromium and WebView2 - checked) the fallback is a `uBase` uniform and one
   *  draw per run: two calls where there were six.
   *
   *  What stays on its own path, drawn exactly as before from its own vertex array: a
   *  batch with a posed skin (its vertices are re-uploaded every frame), the moths, the
   *  inspect view's single batch, anything without cell runs. The fragment shader is the
   *  same shader with the same inputs, so the picture is the picture: the same floats
   *  reach the same lighting, by a texel fetch instead of an instanced attribute.
   *
   *  A batch under `WHOLE_TRIS` triangles in all is one run rather than one per cell:
   *  drawing it entire costs the GPU less than testing its cells costs the page, and the
   *  thousand-odd of those were most of the draw calls.
   *
   *  **Round 18cs: the bake.** Robin's report after 18cr: 4,711 draws submitted a frame,
   *  and no faster to the eye. A sub-draw inside a multi-draw is still a draw to the
   *  driver, and one whose `gl_DrawID` changes is a uniform update on top - ANGLE writes
   *  the program's whole constant block to the GPU before each - so two thousand of them
   *  a pass was where the frame went, whatever the page's own submit cost. Of those
   *  sub-draws, five in six were *whole* batches: a small mesh with one to four instances,
   *  drawn entire. So every whole batch of a solid group is **baked**: its vertices are
   *  copied into the scene buffers once per instance, each copy carrying its instance's
   *  number as an integer attribute (`aInst`), and the vertex shader (VS_STATIC_B) fetches
   *  the same matrix from the same texel by that number instead of by `gl_InstanceID` -
   *  the same floats through the same arithmetic, so the same pixels. A group's baked
   *  batches lie end to end in the index buffer, and its whole bake is *one* sub-draw
   *  (visible neighbours merged) under a program that never reads `gl_DrawID`, so no
   *  uniform moves between them. The GPU shades exactly the vertices it shaded before -
   *  a whole batch was drawn entire either way. What it costs is memory: a copy per
   *  instance, `BAKE_VERTS` at most.
   *
   *  The see-through groups are not baked: blending is order-dependent, and the bake
   *  would draw a group's whole batches before its others where the batch order used to
   *  interleave them. They are few. Nor is anything over `WHOLE_TRIS`: those are drawn
   *  per cell run through `gl_DrawID` as before, and a few hundred of those a pass is the
   *  price of culling a forest by the cell.
   *
   *  And the animated batches (a node animation, a UV animation) are in the groups now,
   *  each in a group of its own: the group's material is read off its one batch, clock
   *  and all, so nothing is lost by drawing them through the scene buffers. */
  _groupable(b){
    return !!(b && b.n && b.part && b.mats && b.groups && b.groups.length &&
              !b.skin && !b.moths && b.texKey==null);
  }
  /** The key two batches must share to be drawn in one call: everything `_meshTex` and
   *  `_meshMode` would set differently between them. An animated batch keys on itself
   *  (round 18cs): its transform and material alpha are its own clock's. */
  _groupKey(b){
    const id=t=>t? this._texId(t) : 0;
    return [id(b.tex), b.alpha.fn, b.alpha.ref,
            b.blend? b.blend.src+'|'+b.blend.dst+'|'+b.blend.alpha+(b.blend.nosort? '|ns' : '') : '',
            b.drawMode|0, b.unlit?1:0, b.dayNight|0, b.vcolMode==null? 2 : b.vcolMode,
            id(b.detail), id(b.glow), id(b.dark),
            (b.emissive||BLACK3).join(','), (b.diffuse||[1,1,1]).join(','),
            /* Rounds 18de-18dg: the ambient, whether the shape carries colours, its decal,
               its clamp and its depth write - each something `_meshTex`, `_meshMode` or
               the pass sets from the group's representative, so two batches that differ
               in one must not share a group. */
            (b.ambient||b.diffuse||[1,1,1]).join(','), b.vcolHas?1:0, id(b.decal), b.clamp|0, b.zwrite===false? 'z':'',
            id(b.env), b.envClamp|0, id(b.bump), b.bump? (b.bumpLuma.join(',')+'/'+b.bumpMat.join(',')) : '',
            (b.anim||b.uvAnim||b.uvAnim2)? 'own'+this._texId(b) : ''].join('');
  }
  _texId(t){
    if(!this._texIds) this._texIds=new WeakMap();
    let id=this._texIds.get(t);
    if(!id){ id=(this._texIdNext=(this._texIdNext||0)+1); this._texIds.set(t,id); }
    return id;
  }
  _dropGroups(){
    const G=this._groups;
    this._groups=null; this._legacyStatics=null;
    if(!G) return;
    const gl=this.gl;
    try{ gl.deleteVertexArray(G.vao); }catch(_){ }
    for(const b of G.bufs){ try{ gl.deleteBuffer(b); }catch(_){ } }
    try{ gl.deleteTexture(G.instTex); }catch(_){ }
  }
  /** The program that reads instances from the texture - with `gl_DrawID` or without. */
  _progM(multi){
    const gl=this.gl;
    const key=multi? 'progStaticM' : 'progStaticF';
    if(!this[key]){
      const pr=this._prog(VS_STATIC_M(multi? 'multi' : 'base'),FS_GRASS);
      const prev=gl.getParameter(gl.CURRENT_PROGRAM);
      gl.useProgram(pr.p);
      gl.uniform1i(pr.u.uInst,INST_UNIT);              // the instance texture's unit, for good
      gl.uniform4fv(pr.u.uBrushS,[0,0,0,-1]);   // as the constructor does for its siblings
      gl.useProgram(prev);
      this[key]=pr;
    }
    return this[key];
  }
  /** The baked batches' program (round 18cs): the instance number is a vertex attribute. */
  _progB(){
    const gl=this.gl;
    if(!this.progStaticB){
      const pr=this._prog(VS_STATIC_M('baked'),FS_GRASS);
      const prev=gl.getParameter(gl.CURRENT_PROGRAM);
      gl.useProgram(pr.p);
      gl.uniform1i(pr.u.uInst,INST_UNIT);
      gl.uniform4fv(pr.u.uBrushS,[0,0,0,-1]);
      gl.useProgram(prev);
      this.progStaticB=pr;
    }
    return this.progStaticB;
  }
  /** The grass program without `gl_DrawID` - one draw a run - for a context without
   *  WEBGL_multi_draw, and for the test hook that walks that path on one that has it. */
  _progGrassF(){
    const gl=this.gl;
    if(!this.progGrassF){
      const pr=this._prog(VS_GRASS_M('base'),FS_GRASS);
      const prev=gl.getParameter(gl.CURRENT_PROGRAM);
      gl.useProgram(pr.p);
      gl.uniform1i(pr.u.uInst,INST_UNIT);
      gl.uniform4fv(pr.u.uBrushS,[0,0,0,-1]);
      gl.useProgram(prev);
      pr.multi=false;
      this.progGrassF=pr;
    }
    return this.progGrassF;
  }
  _buildGroups(list){
    const gl=this.gl;
    this._dropGroups();
    const able=[], legacy=[];
    for(const b of list||[]){ if(this._groupable(b)) able.push(b); else legacy.push(b); }
    this._legacyStatics=legacy;
    if(!able.length){ this._groups=null; return; }
    const vcOf=p=>p.pos.length/3|0;
    const whole=b=>(b.count/3)*b.n<=WHOLE_TRIS;
    /* 1. The groups: (texture, alpha, blend, faces, lighting flags, maps, colours) - and
       which of their batches are baked (round 18cs): the whole ones of the solid groups,
       within the memory budget, in group order. */
    const byKey=new Map();
    for(const b of able){
      const k=this._groupKey(b);
      let g=byKey.get(k);
      if(!g){ g={key:k, rep:b, blend:b.blend, tid:b.tex? this._texId(b.tex) : 0, items:[]}; byKey.set(k,g); }
      g.items.push(b);
    }
    const all=[...byKey.values()];
    // Solid before see-through, then by texture, so the binds inside a pass are fewest.
    // Round 18dg: a NoSorter group (blend + test, drawn among the solid things as the
    // game does) counts as solid here and in `solid`/`clear` below.
    const seeThrough=g=>(g.blend && !g.blend.nosort)? 1 : 0;
    all.sort((a,b)=>seeThrough(a)-seeThrough(b) || a.tid-b.tid || (a.key<b.key? -1 : a.key>b.key? 1 : 0));
    const baked=new Set();
    let bakeV=0, bakeN=0;
    if(!this.noBake){
      for(const g of all){
        if(g.blend) continue;
        for(const b of g.items){
          if(!whole(b)) continue;
          const v=vcOf(b.part)*b.n;
          if(bakeV+v>BAKE_VERTS) continue;
          baked.add(b); bakeV+=v; bakeN++;
        }
      }
    }
    /* 2. Every part a run draws, once, at its place in the scene buffers; then the baked
       copies, group by group, batch by batch, instance by instance. */
    const parts=new Map();
    let nv=0, ni=0, anyUv2=false, anyUvd=false, anyUvdc=false;
    const sees=p=>{ const vc=vcOf(p); if(p.uv2 && p.uv2.length===vc*2) anyUv2=true; if(p.uvDark && p.uvDark.length===vc*2) anyUvd=true;
                    if(p.uvDecal && p.uvDecal.length===vc*2) anyUvdc=true; };
    for(const b of able){
      if(baked.has(b)) continue;
      const p=b.part;
      if(parts.has(p)) continue;
      parts.set(p,{id:parts.size, vbase:nv, ibase:ni, count:p.idx.length});
      nv+=vcOf(p); ni+=p.idx.length; sees(p);
    }
    const bakeAt=nv, bakeIAt=ni;
    for(const b of baked){ nv+=vcOf(b.part)*b.n; ni+=b.part.idx.length*b.n; sees(b.part); }
    const pos=new Float32Array(nv*3), nrm=new Float32Array(nv*3), uv=new Float32Array(nv*2);
    const col=new Uint8Array(nv*4);   // RGBA since 18df
    const uv2=anyUv2? new Float32Array(nv*2) : null;
    const uvd=anyUvd? new Float32Array(nv*2) : null;
    const uvdc=anyUvdc? new Float32Array(nv*2) : null;
    const inst=new Int32Array(nv);
    const idx=new Uint32Array(ni);
    const copy=(p,vb,ib)=>{
      const vc=vcOf(p);
      pos.set(p.pos,vb*3); nrm.set(p.nrm,vb*3); uv.set(p.uv,vb*2);
      col.set(rgba4(p),vb*4);
      if(uv2 && p.uv2 && p.uv2.length===vc*2) uv2.set(p.uv2,vb*2);
      if(uvd && p.uvDark && p.uvDark.length===vc*2) uvd.set(p.uvDark,vb*2);
      if(uvdc && p.uvDecal && p.uvDecal.length===vc*2) uvdc.set(p.uvDecal,vb*2);
      const pi=p.idx;
      for(let k=0;k<pi.length;k++) idx[ib+k]=pi[k]+vb;
    };
    for(const [p,s] of parts) copy(p,s.vbase,s.ibase);
    /* 3. Every instance once: the parts of one mesh share one matrix array, and so share
       one stretch of the texture. Lamp hours beside the matrix, (-1,-1) for no lamp -
       the same generic value the attribute path reads. */
    const slots=new Map();   // matrix array -> its first instance in the texture
    const firsts=[];         // one batch per matrix array, to read the hours from
    let ninst=0;
    for(const b of able){ if(!slots.has(b.mats)){ slots.set(b.mats,ninst); firsts.push(b); ninst+=b.n; } }
    const rows=Math.max(1,Math.ceil(ninst*4/INST_W));
    const itex=new Float32Array(INST_W*rows*4);
    for(const b of firsts){
      const base=slots.get(b.mats), mats=b.mats;
      for(let k=0;k<b.n;k++){
        const t=(base+k)*16;
        itex.set(mats.subarray(k*12,k*12+12),t);
        itex[t+12]=b.lamp? b.lamp[k*2]   : -1;
        itex[t+13]=b.lamp? b.lamp[k*2+1] : -1;
      }
    }
    /* 4. The runs of every batch drawn by instance - (part, first instance, count, world
       box), in an order that lets neighbouring runs of one batch merge - and the bake
       runs of every baked batch: (index range, world box), laid end to end per group. */
    let R=0, BR=0;
    for(const b of able){ if(baked.has(b)) BR++; else R+=whole(b)? 1 : b.groups.length; }
    const runBox=new Float32Array(R*6), runBase=new Int32Array(R), runN=new Int32Array(R), runPart=new Int32Array(R);
    const bkBox=new Float32Array(BR*6), bkOff=new Int32Array(BR), bkCount=new Int32Array(BR), bkInst=new Int32Array(BR);
    const unionBox=(box,o,groups)=>{
      box[o]=box[o+1]=box[o+2]=Infinity; box[o+3]=box[o+4]=box[o+5]=-Infinity;
      for(const cg of groups){
        const bb=cg.b;
        if(bb[0]<box[o])box[o]=bb[0]; if(bb[1]<box[o+1])box[o+1]=bb[1]; if(bb[2]<box[o+2])box[o+2]=bb[2];
        if(bb[3]>box[o+3])box[o+3]=bb[3]; if(bb[4]>box[o+4])box[o+4]=bb[4]; if(bb[5]>box[o+5])box[o+5]=bb[5];
      }
    };
    let r=0, br=0, vb=bakeAt, ib=bakeIAt;
    for(const g of all){
      g.r0=r; g.b0=br;
      for(const b of g.items){
        const base=slots.get(b.mats);
        if(baked.has(b)){
          const p=b.part, vc=vcOf(p), ic=p.idx.length;
          unionBox(bkBox,br*6,b.groups);
          bkOff[br]=ib*4; bkCount[br]=ic*b.n; bkInst[br]=b.n;
          for(let k=0;k<b.n;k++){ copy(p,vb,ib); inst.fill(base+k,vb,vb+vc); vb+=vc; ib+=ic; }
          br++;
          continue;
        }
        const pid=parts.get(b.part).id;
        if(whole(b)){
          unionBox(runBox,r*6,b.groups);
          runBase[r]=base; runN[r]=b.n; runPart[r]=pid; r++;
        }else{
          for(const cg of b.groups){
            const o=r*6, bb=cg.b;
            runBox[o]=bb[0]; runBox[o+1]=bb[1]; runBox[o+2]=bb[2]; runBox[o+3]=bb[3]; runBox[o+4]=bb[4]; runBox[o+5]=bb[5];
            runBase[r]=base+cg.at; runN[r]=cg.n; runPart[r]=pid; r++;
          }
        }
      }
      g.r1=r; g.b1=br;
      const cap=g.r1-g.r0, bcap=g.b1-g.b0;
      // What a frame's cull writes: one sub-draw per surviving (merged) run.
      g.dcount=new Int32Array(cap); g.doff=new Int32Array(cap); g.dinst=new Int32Array(cap);
      g.dbase=new Int32Array(cap+4);   // padded to a whole ivec4 for `uniform4iv`
      g.dpart=new Int32Array(cap);     // round 18dg: the part each sub-draw draws, for the sort
      g.vis=0; g.visInst=0;
      g.bcount=new Int32Array(bcap); g.boff=new Int32Array(bcap);
      g.bvis=0; g.bvisInst=0;
    }
    const P=parts.size;
    const partCount=new Int32Array(P), partOff=new Int32Array(P), partMid=new Float32Array(P*3);
    for(const [p,s] of parts){ partCount[s.id]=s.count; partOff[s.id]=s.ibase*4;
      /* Round 18dq: the part's own bound centre in the mesh's space, for the see-through
         sort - the middle of its vertices' box, which is what NetImmerse's world bound
         and OSG's bounding sphere both sit at for a shape. */
      const v=p.pos; let lo0=Infinity,lo1=Infinity,lo2=Infinity,hi0=-Infinity,hi1=-Infinity,hi2=-Infinity;
      for(let i=0;i<v.length;i+=3){ const x=v[i],y=v[i+1],z=v[i+2]; if(x<lo0)lo0=x; if(x>hi0)hi0=x; if(y<lo1)lo1=y; if(y>hi1)hi1=y; if(z<lo2)lo2=z; if(z>hi2)hi2=z; }
      const o=s.id*3; if(v.length){ partMid[o]=(lo0+hi0)/2; partMid[o+1]=(lo1+hi1)/2; partMid[o+2]=(lo2+hi2)/2; } }
    /* 5. On the GPU. */
    const vao=gl.createVertexArray(); gl.bindVertexArray(vao);
    const bufs=[];
    const attr=(loc,data,size,type,norm)=>{
      const b=this._buf(data); bufs.push(b);
      gl.bindBuffer(gl.ARRAY_BUFFER,b); gl.enableVertexAttribArray(loc);
      gl.vertexAttribPointer(loc,size,type,!!norm,0,0);
    };
    attr(0,pos,3,gl.FLOAT); attr(1,nrm,3,gl.FLOAT); attr(2,uv,2,gl.FLOAT);
    attr(6,col,4,gl.UNSIGNED_BYTE,true);
    if(uv2) attr(7,uv2,2,gl.FLOAT);
    if(uvd) attr(9,uvd,2,gl.FLOAT);
    if(uvdc) attr(10,uvdc,2,gl.FLOAT);
    if(BR){
      // The baked copies' instance numbers, an integer attribute (round 18cs).
      const b=this._buf(inst); bufs.push(b);
      gl.bindBuffer(gl.ARRAY_BUFFER,b); gl.enableVertexAttribArray(8);
      gl.vertexAttribIPointer(8,1,gl.INT,0,0);
    }
    const ibuf=this._buf(idx,gl.ELEMENT_ARRAY_BUFFER); bufs.push(ibuf);
    gl.bindVertexArray(null);
    const instTex=gl.createTexture();
    gl.activeTexture(gl.TEXTURE0+INST_UNIT);
    gl.bindTexture(gl.TEXTURE_2D,instTex);
    gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA32F,INST_W,rows,0,gl.RGBA,gl.FLOAT,itex);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.CLAMP_TO_EDGE);
    gl.activeTexture(gl.TEXTURE0);
    const multi=(!this.noMultiDraw && this._mdraw)? this._mdraw : null;
    this._groups={src:list, vao, bufs, instTex, all, solid:all.filter(g=>!seeThrough(g)), clear:all.filter(g=>seeThrough(g)),
                  runBox, runBase, runN, runPart, partCount, partOff, partMid, runs:R, parts:P, inst:ninst,
                  /* Round 18dg: what the far-to-near sort of the see-through instances
                     reads - the instance matrices as uploaded - and what it writes each
                     frame (`_sortClear`). */
                  itex, sorted:null,
                  bkBox, bkOff, bkCount, bkInst, baked:BR, bakedVerts:bakeV, bakedInst:bkInst.reduce((a,b)=>a+b,0),
                  batches:able.length, multi, cullVP:null,
                  bytes:(pos.byteLength+nrm.byteLength+uv.byteLength+col.byteLength+idx.byteLength+
                         (uv2? uv2.byteLength:0)+(uvd? uvd.byteLength:0)+(uvdc? uvdc.byteLength:0)+(BR? inst.byteLength:0)+itex.byteLength)};
  }
  /** Which runs of each group are on screen for this view, as the sub-draws of a frame.
   *  Cached on the view matrix: the opaque and the translucent phase of one frame look
   *  through the same one, the reflection through its own. Round 18cs: the bake runs
   *  too, neighbours merged into one index range. */
  _cullGroups(G,planes,VP,clipZ){
    if(G.cullVP===VP) return;
    G.cullVP=VP;
    /* Round 18ct: in the reflection, what stands wholly under the water line is skipped
       here rather than discarded fragment by fragment - a jetty's piles, a sunk boat, the
       river bed's clutter. `clipZ` is null in every other pass. */
    const zc=(clipZ==null)? -Infinity : clipZ;
    const rb=G.runBox, rbase=G.runBase, rn=G.runN, rp=G.runPart, pc=G.partCount, po=G.partOff;
    const kb=G.bkBox, ko=G.bkOff, kc=G.bkCount, ki=G.bkInst;
    const p0=planes[0],p1=planes[1],p2=planes[2],p3=planes[3],p4=planes[4],p5=planes[5];
    const pl=[p0,p1,p2,p3,p4,p5];
    const seen=(box,o)=>{
      if(box[o+5]<zc) return false;
      for(let i=0;i<6;i++){
        const p=pl[i];
        const x=p[0]>=0? box[o+3] : box[o], y=p[1]>=0? box[o+4] : box[o+1], z=p[2]>=0? box[o+5] : box[o+2];
        if(p[0]*x+p[1]*y+p[2]*z+p[3]<0) return false;
      }
      return true;
    };
    for(const g of G.all){
      let vis=0, inst=0, lastPart=-1, lastEnd=-1;
      const dc=g.dcount, dof=g.doff, din=g.dinst, dbs=g.dbase, dpt=g.dpart;
      for(let r=g.r0;r<g.r1;r++){
        if(!seen(rb,r*6)) continue;
        const part=rp[r], base=rbase[r], n=rn[r];
        if(vis && lastPart===part && lastEnd===base){ din[vis-1]+=n; lastEnd+=n; }
        else{ dc[vis]=pc[part]; dof[vis]=po[part]; din[vis]=n; dbs[vis]=base; dpt[vis]=part; vis++; lastPart=part; lastEnd=base+n; }
        inst+=n;
      }
      g.vis=vis; g.visInst=inst;
      let bvis=0, binst=0, lastR=-2;
      const bc=g.bcount, bo=g.boff;
      for(let r=g.b0;r<g.b1;r++){
        if(!seen(kb,r*6)) continue;
        if(bvis && lastR===r-1) bc[bvis-1]+=kc[r];
        else{ bc[bvis]=kc[r]; bo[bvis]=ko[r]; bvis++; }
        lastR=r; binst+=ki[r];
      }
      g.bvis=bvis; g.bvisInst=binst;
    }
    /* The solid groups this pass draws, in their texture order. Round 18ct sorted them
       nearest first for the depth test's sake and round 18cu took that back: Robin's GPU
       clock showed the passes bound by triangle count, not by shading, so the early
       rejections bought nothing measurable and the sort cost a millisecond of binds. */
    G.order=this.drawOrder(G.solid);
    this._sortClear(G,VP);
  }
  /** Round 18dg: the see-through instances of this view, far to near, as the sub-draws
   *  of the translucent pass.
   *
   *  The game draws its blended shapes with depth writes on and sorts them each frame
   *  by the distance of each shape's bound centre; drawn in batch order with the depth
   *  left alone, a far leaf cluster painted over a near one wherever its texels were
   *  solid (nif-audit.md §3). Robin: "the game's look minus its halos", and the sort
   *  always on. So every visible run of every see-through group is opened into its
   *  instances, each given the camera-space depth of its part's centre (the clip-space w
   *  of VP, which is exactly that), and the lot sorted descending. Drawn back in that
   *  order, consecutive instances of one group make one segment (its material set once),
   *  and inside a segment consecutive instances of one part make one sub-draw.
   *
   *  **Binned, and packed** (18dh). The first cut sorted exact depths with a comparator
   *  and made a segment at every change of group: Seyda Neen 3x3 was 2,500 instances,
   *  1,100 segments and ten GL calls a segment, and Robin's 25 cells were "a bit laggy".
   *  The depth is quantized to a bin about 1% wide (64 bins a doubling of the distance,
   *  log-spaced, so a bin is 50 units at 5,000 and 200 at 20,000), and the bin and the
   *  instance's index are packed into one 32-bit key sorted by the typed array's own
   *  sort - numeric, native, a millisecond for ten thousand. Within a bin the order is
   *  the index's: group by group, part by part - which is both what merges the segments
   *  and what NetImmerse does for shapes it finds at one distance (its insertion order,
   *  the file's). Two overlapping clusters of *different* species within 1% of one
   *  distance are drawn group-first rather than by their centres; the game's own order
   *  between those flips as you move anyway. A tree's own clusters span many bins and
   *  keep their order. */
  _sortClear(G,VP){
    let total=0;
    for(const g of G.clear) total+=g.visInst;
    if(!total){ G.sorted=null; return; }
    const S=G.sorted && G.sorted.cap>=total? G.sorted : (G.sorted={cap:total,
      key:new Float64Array(total), grp:new Int32Array(total), inst:new Int32Array(total), part:new Int32Array(total),
      dcount:new Int32Array(total), doff:new Int32Array(total), dinst:new Int32Array(total),
      dbase:new Int32Array(total+4), segs:[], run:[]});
    const key=S.key, grp=S.grp, inst=S.inst, part=S.part, it=G.itex;
    const w0=VP[3], w1=VP[7], w2=VP[11], w3=VP[15];
    /* Round 18dq: the *shape* is what is sorted, by the depth of its own bound centre in
       the scene - which is what NetImmerse's alpha accumulator sorts (each NiGeometry by
       its world bound) and what OpenMW's transparent bin sorts (each drawable by its
       bound, back to front; nifloader.cpp `handleAlphaBlending`: a blended shape goes to
       the TRANSPARENT_BIN unless its NiAlphaProperty says no sorter, and its depth
       writes are whatever its NiZBufferProperty says - on, by default). 18dj sorted the
       *object* and kept its parts in file order, after the crystal's shells flickered
       when they were sorted by centre with the depth writes off; with the writes on
       (`blendWritesDepth`, 18dq) the order inside an object matters only where two
       shells overlap, as in the game, and a near shard is no longer painted over by a
       far one drawn after it in the file - Robin: "the close crystal is almost invisible
       in the tool". Twelve bits of bin, twenty of instance, twenty of entry, in a
       double - 52 bits, exact; within a bin the order is the entries', part by part. */
    const BINS=64/Math.LN2, LN16=Math.log(16), M20=1048576, pm=G.partMid;
    let n=0;
    for(let gi=0;gi<G.clear.length;gi++){
      const g=G.clear[gi];
      for(let d=0;d<g.vis;d++){
        const base=g.dbase[d], cnt=g.dinst[d], pid=g.dpart[d];
        const cx=pm[pid*3], cy=pm[pid*3+1], cz=pm[pid*3+2];
        for(let k=0;k<cnt;k++){
          const i=base+k, t=i*16;
          // The part's centre through the instance's rows, then the clip-space w of that.
          const wx=it[t]*cx+it[t+1]*cy+it[t+2]*cz+it[t+3];
          const wy=it[t+4]*cx+it[t+5]*cy+it[t+6]*cz+it[t+7];
          const wz=it[t+8]*cx+it[t+9]*cy+it[t+10]*cz+it[t+11];
          const w=w0*wx+w1*wy+w2*wz+w3;
          let bin=w>16? ((Math.log(w)-LN16)*BINS)|0 : 0;
          if(bin>4095) bin=4095;
          key[n]=(bin*M20+i)*M20+n; grp[n]=gi; inst[n]=i; part[n]=pid; n++;
        }
      }
    }
    const keys=n===key.length? key : key.subarray(0,n);
    keys.sort();   // ascending: nearest bins last, read back to front below
    const dc=S.dcount, dof=S.doff, din=S.dinst, dbs=S.dbase, pc=G.partCount, po=G.partOff;
    const segs=S.segs; segs.length=0;
    const run=S.run;   // one object's entries, put into the file's order before they are emitted
    let nd=0, seg=null, lastPart=-1, lastEnd=-1;
    const emit=(i)=>{
      const gi=grp[i], pid=part[i], ii=inst[i];
      if(!seg || seg.gi!==gi){ seg={gi, g:G.clear[gi], at:nd, len:0, inst:0}; segs.push(seg); lastPart=-1; }
      if(lastPart===pid && lastEnd===ii){ din[nd-1]++; lastEnd++; }
      else{ dc[nd]=pc[pid]; dof[nd]=po[pid]; din[nd]=1; dbs[nd]=ii; nd++; seg.len++; lastPart=pid; lastEnd=ii+1; }
      seg.inst++;
    };
    let k=n-1;
    while(k>=0){
      const head=Math.floor(keys[k]/M20);   // bin and instance: what one object's entries share
      run.length=0;
      while(k>=0 && Math.floor(keys[k]/M20)===head){ run.push(keys[k]%M20); k--; }
      if(run.length>1) run.sort((a,b)=>part[a]-part[b]);
      for(let r=0;r<run.length;r++) emit(run[r]);
    }
    S.n=n; S.draws=nd;
  }
  /** Round 18dg: the see-through groups' instances in `_sortClear`'s order, each
   *  segment's material once, depth written unless the shape's NiZBufferProperty says
   *  test-only. */
  _drawSorted(pm,G){
    const S=G.sorted; if(!S || !S.n) return;
    const gl=this.gl;
    let zw=null;
    for(const seg of S.segs){
      const g=seg.g;
      this._meshTex(pm,g.rep);
      this._meshMode(pm,g.rep,true);
      gl.blendFunc(gl[g.blend.src], gl[g.blend.dst]);
      // Round 18h: what adds onto the picture is fogged to black, not to the haze.
      if(pm.u.uFogBlack) this._u1i(pm,'uFogBlack', g.blend.dst==='ONE'? 1 : 0);
      /* Round 18ea: nothing see-through writes depth into the picture the water reads.
         MGE's depth frame (`XE Depth.fx`) is a pass of its own and carries no blended
         geometry at all, so the `depth` its water shader measures at a pixel is the depth
         of the *solid* thing behind the blended one - which is what fades a crystal
         standing in the shallows into the water's own colour along with the sea floor
         around it. Drawn into the depth here instead, its own depth was a few units and
         `depthscale` came back at 1: the refraction showed it whole. Robin: "Transparent
         objects like the telvanni crystals [...] are also much more visible than other
         objects through the water surface." */
      const w=g.rep.zwrite!==false && !this._clearNoZ;
      if(w!==zw){ gl.depthMask(w); zw=w; }
      if(G.multi){
        for(let at=seg.at;at<seg.at+seg.len;at+=MAXRUN){
          const n=Math.min(MAXRUN,seg.at+seg.len-at);
          gl.uniform4iv(pm.u.uRunBase, S.dbase, at, ((n+3)>>2)<<2);
          G.multi.multiDrawElementsInstancedWEBGL(gl.TRIANGLES, S.dcount, at, gl.UNSIGNED_INT, S.doff, at, S.dinst, at, n);
          this._multiCalls++;
        }
      }else{
        for(let i=seg.at;i<seg.at+seg.len;i++){
          this._u1i(pm,'uBase',S.dbase[i]);
          gl.drawElementsInstanced(gl.TRIANGLES,S.dcount[i],gl.UNSIGNED_INT,S.doff[i],S.dinst[i]);
        }
      }
      this._drawn+=seg.len; this._drawnInst+=seg.inst;
    }
    if(pm.u.uFogBlack) this._u1i(pm,'uFogBlack',0);
  }
  drawOrder(groups){
    return groups.filter(g=>g.vis||g.bvis);
  }
  /** One group: its material once, then every visible run in as few calls as the
   *  context allows. */
  _drawGroup(pm,g,G){
    if(!g.vis) return;
    const gl=this.gl;
    this._meshTex(pm,g.rep);
    this._meshMode(pm,g.rep,true);
    // Round 18dg: a NoSorter group draws among the solid ones with its blend on - and
    // (18di) an additive one fogged to black like any other thing that adds light.
    const ns=!!(g.blend && g.blend.nosort);
    if(ns){ gl.enable(gl.BLEND); gl.blendFunc(gl[g.blend.src], gl[g.blend.dst]);
            if(pm.u.uFogBlack) this._u1i(pm,'uFogBlack', g.blend.dst==='ONE'? 1 : 0); }
    if(G.multi){
      for(let at=0;at<g.vis;at+=MAXRUN){
        const n=Math.min(MAXRUN,g.vis-at);
        gl.uniform4iv(pm.u.uRunBase, g.dbase, at, ((n+3)>>2)<<2);
        G.multi.multiDrawElementsInstancedWEBGL(gl.TRIANGLES, g.dcount, at, gl.UNSIGNED_INT, g.doff, at, g.dinst, at, n);
        this._multiCalls++;
      }
    }else{
      for(let i=0;i<g.vis;i++){
        this._u1i(pm,'uBase',g.dbase[i]);
        gl.drawElementsInstanced(gl.TRIANGLES,g.dcount[i],gl.UNSIGNED_INT,g.doff[i],g.dinst[i]);
      }
    }
    if(ns){ gl.disable(gl.BLEND); gl.blendFunc(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA);
            if(pm.u.uFogBlack) this._u1i(pm,'uFogBlack',0); }
    this._drawn+=g.vis; this._drawnInst+=g.visInst;
  }
  /** One group's bake (round 18cs): its material once, then its visible index ranges -
   *  usually one - with no uniform between them. */
  _drawBake(pb,g,G){
    if(!g.bvis) return;
    const gl=this.gl;
    this._meshTex(pb,g.rep);
    this._meshMode(pb,g.rep,true);
    if(G.multi){
      G.multi.multiDrawElementsWEBGL(gl.TRIANGLES, g.bcount, 0, gl.UNSIGNED_INT, g.boff, 0, g.bvis);
      this._multiCalls++;
    }else{
      for(let i=0;i<g.bvis;i++) gl.drawElements(gl.TRIANGLES,g.bcount[i],gl.UNSIGNED_INT,g.boff[i]);
    }
    this._drawn+=g.bvis; this._drawnInst+=g.bvisInst; this._drawnBaked+=g.bvis;
  }
  /** The dabs of one object's mask, as the RGBA32F row `FS_STATIC_PAINT` fetches from:
      two texels a dab, A = centre.xyz + radius, B = feather, strength, palette index.
      `dabs` is a flat array of eight floats per dab, capped at MAX_TINT_DABS. */
  makeDabTex(dabs){
    const gl=this.gl;
    const n=Math.min((dabs.length/8)|0, MAX_TINT_DABS);
    if(!n) return null;
    const tex=gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D,tex);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.CLAMP_TO_EDGE);
    const src=dabs.length===n*8? dabs : dabs.subarray(0,n*8);
    /* Two texels a dab, laid out row-major in rows of DAB_TEX_W — the same linear order
       the shader indexes with `t % DAB_TEX_W, t / DAB_TEX_W`.
       A mask that fits in one row is uploaded as one row of exactly its own width, which
       is what this always did: for every texel it holds, `t < width <= DAB_TEX_W`, so the
       shader's arithmetic gives back `(t, 0)` unchanged. Only a mask too wide for a row
       pays for the padding to a full rectangle. */
    const texels=n*2;
    if(texels<=DAB_TEX_W){
      gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA32F,texels,1,0,gl.RGBA,gl.FLOAT,src);
    }else{
      const rows=Math.ceil(texels/DAB_TEX_W);
      const data=new Float32Array(rows*DAB_TEX_W*4);
      data.set(src);
      gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA32F,DAB_TEX_W,rows,0,gl.RGBA,gl.FLOAT,data);
    }
    gl.bindTexture(gl.TEXTURE_2D,null);
    return {tex,count:n,grid:this.makeDabGrid(src,n)};
  }

  /** A bucket grid over one object's marks, so a fragment walks the near ones only.
   *
   *  `FS_STATIC_PAINT` tests every mark against every pixel of a painted object, which is
   *  linear in how much you have painted it and is paid every frame you can see it. It is
   *  mostly waste: measured on Robin's largest mask, 9,854 marks on a Gnisis cliff, at
   *  most 61 of them can reach any single point and the mean is under one. This is the
   *  same move `RayGrid` made for `Surface::ray` in the engine — a uniform grid, lists
   *  built once — and it is exact for the same reason: a mark is left out of a cell only
   *  when its own sphere cannot touch that cell, so every mark the shader skips is one the
   *  distance test would have turned away anyway.
   *
   *  **The order is preserved and has to be.** Within a plane the last mark wins, so a
   *  cell's list is filled in mark order and walked in mark order; the fragment then sees
   *  the same marks in the same sequence as the full walk, with only the unreachable ones
   *  missing. That is what makes this an optimisation rather than a second answer.
   *
   *  Returns null below `DAB_GRID_MIN`, where the grid would cost more than the walk.
   */
  makeDabGrid(src,n){
    const g=this.buildDabGrid(src,n);
    if(!g) return null;
    const gl=this.gl;
    // Offsets then lists, in one R32F texture the shader reads with `gridAt`.
    const len=g.base+g.idx.length;
    const rows=Math.ceil(len/DAB_TEX_W);
    const data=new Float32Array(rows*DAB_TEX_W);
    data.set(g.starts,0);
    data.set(g.idx,g.base);
    const tex=gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D,tex);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.CLAMP_TO_EDGE);
    gl.pixelStorei(gl.UNPACK_ALIGNMENT,4);
    gl.texImage2D(gl.TEXTURE_2D,0,gl.R32F,DAB_TEX_W,rows,0,gl.RED,gl.FLOAT,data);
    gl.bindTexture(gl.TEXTURE_2D,null);
    return {tex,lo:g.lo,inv:g.inv,N:g.N,base:g.base,cells:g.cells,indices:g.idx.length};
  }

  /** The grid itself, worked out and handed back rather than uploaded.
   *
   *  Separate from the texture it ends up in for one reason: the property this whole
   *  thing rests on — that a box holds every mark able to reach any point inside it — is
   *  checkable against a brute-force sweep, and only if the lists can be read. `t_statics`
   *  does that. A structure whose correctness can only be judged by looking at a picture
   *  is one nobody will check again.
   */
  buildDabGrid(src,n){
    if(n<this.dabGridMin) return null;
    // Bounds of the marks themselves, in object space.
    const lo=[Infinity,Infinity,Infinity], hi=[-Infinity,-Infinity,-Infinity];
    const rad=new Float32Array(n);
    for(let i=0;i<n;i++){
      const o=i*8, r=src[o+3];
      rad[i]=r;
      for(let k=0;k<3;k++){
        if(src[o+k]-r<lo[k]) lo[k]=src[o+k]-r;
        if(src[o+k]+r>hi[k]) hi[k]=src[o+k]+r;
      }
    }
    const span=[Math.max(hi[0]-lo[0],1e-3),Math.max(hi[1]-lo[1],1e-3),
                Math.max(hi[2]-lo[2],1e-3)];
    /* Cells sized in median radii, not in a count: a boulder and a cliff then get the
       same shape of grid rather than the same number of boxes, which is what lets one
       constant serve both. The median rather than the mean because one enormous mark
       from a cell-wide brush should not coarsen the grid for a thousand small ones. */
    const sorted=Float32Array.from(rad).sort();
    const med=sorted[sorted.length>>1]||1;
    let cell=Math.max(med*DAB_GRID_CELL_R,1e-3);
    let N=[1,1,1], cells=0;
    for(;;){
      for(let k=0;k<3;k++)
        N[k]=Math.min(Math.max(Math.ceil(span[k]/cell),1),DAB_GRID_AXIS);
      cells=N[0]*N[1]*N[2];
      // A mask spread over a huge object can still ask for too many boxes; coarsen until
      // it fits rather than refuse it a grid.
      if(cells<=DAB_GRID_CELLS) break;
      cell*=1.5;
    }
    const inv=[N[0]/span[0],N[1]/span[1],N[2]/span[2]];
    // Which boxes each mark's own box covers — counted first, then filled, so the lists
    // land in one flat array with no per-cell allocation.
    const a=new Int32Array(n*3), b=new Int32Array(n*3);
    const counts=new Int32Array(cells);
    /* Shade marks get no boxes: they act only through the mark before them — the
       shader reads a parent's shade run forward from the parent's own index — so a
       list entry for one would be walked and skipped, and a subset list cannot keep a
       shade beside its parent anyway. Its sphere lies inside the parent's, so the
       parent's boxes already cover everywhere the run matters. Same rule as the
       engine's `DabGrid::build`. */
    const isShade=i=>src[i*8+7]<-3.5;
    for(let i=0;i<n;i++){
      if(isShade(i)){ for(let k=0;k<3;k++){ a[i*3+k]=1; b[i*3+k]=0; } continue; }
      const o=i*8, r=rad[i];
      for(let k=0;k<3;k++){
        let f0=Math.floor((src[o+k]-r-lo[k])*inv[k]);
        let f1=Math.floor((src[o+k]+r-lo[k])*inv[k]);
        if(f0<0) f0=0; if(f0>N[k]-1) f0=N[k]-1;
        if(f1<0) f1=0; if(f1>N[k]-1) f1=N[k]-1;
        a[i*3+k]=f0; b[i*3+k]=f1;
      }
      for(let z=a[i*3+2];z<=b[i*3+2];z++)
        for(let y=a[i*3+1];y<=b[i*3+1];y++)
          for(let x=a[i*3];x<=b[i*3];x++) counts[(z*N[1]+y)*N[0]+x]++;
    }
    const starts=new Int32Array(cells+1);
    for(let c=0;c<cells;c++) starts[c+1]=starts[c]+counts[c];
    const total=starts[cells];
    const fill=Int32Array.from(starts);
    const idx=new Int32Array(total);
    for(let i=0;i<n;i++)
      for(let z=a[i*3+2];z<=b[i*3+2];z++)
        for(let y=a[i*3+1];y<=b[i*3+1];y++)
          for(let x=a[i*3];x<=b[i*3];x++) idx[fill[(z*N[1]+y)*N[0]+x]++]=i;

    return {lo,inv,N,starts,idx,cells,base:cells+1};
  }

  /** Light up particular placed objects, by drawing them again over themselves.
   *
   *  `items` are pickable records — the ones `setPickables` was given, each carrying the
   *  model's parts and that instance's matrix. Passing the pickables rather than a mesh
   *  name is what keeps this honest: the thing that lights up is the same record the
   *  pointer resolved to, so what you see highlighted is what a right-click would act on.
   *  If the two were looked up separately they could disagree, and a highlight that
   *  points at the wrong object is worse than none.
   *
   *  Drawn as a second pass rather than by tinting the original, because the originals
   *  are instanced per mesh — every rock in the cell is one draw call — and picking one
   *  instance out of a batch would mean a per-instance attribute re-uploaded on every
   *  pointer move. This costs one small buffer per distinct mesh in the highlight.
   *
   *  Round 18al: an animated object is drawn again *on the same clock*. Robin: "The
   *  yellow highlight on hovering the cursor over animated gifs is out of sync. It feels
   *  like the yellow highlight is another mesh is playing the same animation on top of
   *  the base one, but out of sync with the animation, thus drifting and creating an
   *  unwanted effect." It was that: the instance plays in a phase bin (18ai/18aj) with
   *  its own shift into the clip and its own rate, and the highlight was built from the
   *  bare part with neither, so it played the file's clock over a bin running at 80-90%
   *  of it. Each pickable now carries its bin's `phase`, the highlight batch takes the
   *  same shift and rate, and a skinned part comes through `phasedPart` - the very copy
   *  the bin draws, buffers and all, so the gold is the posed vertices already there.
   */
  /** Lights up every face wearing one texture, by the key the batches carry.
   *
   *  For the inspect view's texture grid: there each part is its own batch and carries
   *  `texKey`, so "the faces wearing this texture" is exactly "the batches keyed with
   *  it" — gold like every other highlight, the rest dimmed, no second geometry pass.
   *  Cleared with null. Batches without a `texKey` (the cell preview's) are never
   *  dimmed by this, because nothing sets it outside the inspect view. */
  setTextureFaceHighlight(key){
    this.hiTexKey=key==null? null : key;
    this.dirty=true;
  }
  setStaticHighlight(items){
    for(const b of (this.hlStatics||[])) this._dropVao(b);
    this.hlStatics=[];
    const list=items||[];
    if(list.length){
      // Grouped by the model's parts array, which is shared by every instance of a mesh
      // — so identity is the grouping key and no string comparison is needed — and, 18al,
      // within a mesh by phase bin, since each bin is its own clock.
      const byParts=new Map();
      for(const it of list){
        if(!it || !it.parts || !it.m) continue;
        const ph=it.phase||null;
        const key=ph? ph.shift.toFixed(4)+'@'+ph.speed.toFixed(4) : '';
        let g=byParts.get(it.parts); if(!g){ g=new Map(); byParts.set(it.parts,g); }
        let bin=g.get(key); if(!bin){ bin={ph, mats:[]}; g.set(key,bin); }
        bin.mats.push(it.m);
      }
      for(const [parts,bins] of byParts){
        for(const {ph,mats} of bins.values()){
          const m=new Float32Array(mats.length*12);
          mats.forEach((x,k)=>m.set(x,k*12));
          for(const part of parts){
            // The bin's own copy of a skinned part: the buffers the bin is drawing from.
            const pp=(ph && (part.skin||part.morph) && typeof phasedPart==='function')? phasedPart(part, ph.shift, ph.speed) : part;
            const bt=this.buildStaticBatch(pp,{m,n:mats.length}, part.glTex? part.glTex.gl : null);
            if(ph){ bt.animShift=ph.shift; bt.animSpeed=ph.speed; }
            this.hlStatics.push(bt);
          }
        }
      }
    }
    this.dirty=true;
  }
  /** A part's vertex data on the GPU, uploaded once and kept on the part (round 17y).
   *
   *  Every rebuild of a cell - and a step to the next cell is a rebuild - used to upload
   *  every drawn part's positions, normals, UVs, colours and indices again, for a thousand
   *  parts a scene, and let the old copies fall to the garbage collector. The part record
   *  in `App.meshCache` outlives any one scene, so its buffers do too: a batch now makes a
   *  vertex array that *points at* them and uploads only its own instances. Robin: "the
   *  rest should be the same, which should make it possible to reuse scattering etc. too."
   *  Freed with the mesh cache (`dropPartGpu`), which is the only thing that lets a part go. */
  _partGpu(part){
    const gl=this.gl;
    if(part._gpu && part._gpu.gl===gl) return part._gpu;
    /* Round 18cr: off any vertex array first. The index buffer below is bound to make
       it, and ELEMENT_ARRAY_BUFFER is *vertex-array state* - built lazily from inside
       the draw loop, with the last drawn batch's array still bound, it replaced that
       batch's indices with this part's, and the next frame drew the big batch off a
       48-index buffer ("insufficient buffer size", every frame, for ever). */
    gl.bindVertexArray(null);
    const g={gl, pos:this._buf(part.pos), nrm:this._buf(part.nrm), uv:this._buf(part.uv),
             uv2:(part.uv2 && part.uv2.length)? this._buf(part.uv2) : null,
             uvd:(part.uvDark && part.uvDark.length)? this._buf(part.uvDark) : null,
             uvdc:(part.uvDecal && part.uvDecal.length)? this._buf(part.uvDecal) : null,   // round 18df
             col:this._colBuf4(part),
             ib:this._buf(part.idx, gl.ELEMENT_ARRAY_BUFFER)};
    part._gpu=g;
    (this._partGpuAll||(this._partGpuAll=new Set())).add(g);
    return g;
  }
  /** Really frees every part's vertex data. The mesh cache going is what calls for it. */
  dropPartGpu(){
    const gl=this.gl;
    for(const g of (this._partGpuAll||[])){
      for(const k of ['pos','nrm','uv','uv2','uvd','uvdc','col','ib']) if(g[k]){ try{ gl.deleteBuffer(g[k]); }catch(_){ } }
      g.gl=null;
    }
    this._partGpuAll=new Set();
  }
  /* Round 18cr: `lazy` leaves the vertex array unbuilt. The cell preview's batches go
     through the material groups (`_buildGroups`), which draw from scene-wide buffers and
     never touch a batch's own array - so building 2,775 of them per rebuild, and uploading
     the same instance matrices once per *part*, was 180 ms and 22 MB of work nothing read.
     `_ensureVao` builds one on demand for whatever still draws a batch on its own: the
     animated and skinned ones, the moths, a thumbnail, the highlight, a test. */
  buildStaticBatch(part,inst,tex,lazy){
    const b={vao:null,count:part.idx.length,n:inst.n,tex:tex||null,lb:null,
            // Round 17w: how the shape is drawn - see _meshMode. 17x: which branch of a
            // day/night switch it is, 0 for none.
            drawMode:part.drawMode|0, unlit:!!part.unlit, dayNight:part.dayNight|0,
            vcolMode:part.vcolMode==null? 2 : part.vcolMode,
            /* Round 17r: what the cull needs — the instance buffer, so the attribute
               pointers can be moved into it, and the per-cell runs with their world
               boxes. Absent (or empty) means "draw the lot", which is what a thumbnail
               and the highlight pass want. */
            mb:null, groups:inst.groups||null,
            /* Round 18cr: the part and the instances themselves, for the groups - the
               matrices are the caller's array (one per mesh, shared by its parts), the
               lamp hours likewise or null. */
            part, mats:inst.m, lamp:(inst.lamp && inst.lamp.length===inst.n*2)? inst.lamp : null,
            alpha:alphaCut(part,alphaBlend(part)), blend:alphaBlend(part),
            detail:(part.glDetail&&part.glDetail.gl)||null,
            glow:(part.glGlow&&part.glGlow.gl)||null,
            nrm:(part.glNrm&&part.glNrm.gl)||null,
            nrmRG:!!(part.glNrm && part.glNrm.kind==='bc5'),
            // An _nh map (height in alpha) that really has a height: parallax.
            nrmH:!!(part.glNrm && part.nrmHeight && part.glNrm.alpha && part.glNrm.alpha!=='opaque'),
            spec:(part.glSpec&&part.glSpec.gl)||null,
            gloss:(part.glGloss&&part.glGloss.gl)||null,
            dark:(part.glDark&&part.glDark.gl&&part.uvDark&&part.uvDark.length)? part.glDark.gl : null,   // round 17y
            /* Round 17m: the shape's UV animation and its emissive colour ride with the
               batch, since both are per-shape facts the draw has to set. */
            uvAnim:part.uvAnim||null, uvAnim2:part.uvAnim2||null, emissive:part.emissive||null,
            // Round 17y: the shape's node animation and material fade, run per frame.
            anim:part.anim||null,
            /* Round 18ag: the part itself when it is skinned to an animated skeleton -
               its vertex buffers are rewritten each frame (`_skinUpdate`), and they are
               the part's, shared by every batch that draws it. Round 18au: and when it is
               morph-animated - the same buffers, the same update. */
            skin:(part.skin||part.morph)? part : null,
            diffuse:part.diffuse||[1,1,1], ambient:part.ambient||part.diffuse||[1,1,1],
            // Round 18de: whether the shape carries vertex colours at all (see uVColHas).
            vcolHas:!!(part.col && part.col.length),
            zwrite:blendWritesDepth(part, alphaBlend(part)),   // rounds 18dg/18dh: see blendWritesDepth
            /* Round 18df: the decal (slot 6) with its own UVs, and which edges of the base
               map are clamped (see `_wrapTex0`). */
            decal:(part.glDecal&&part.glDecal.gl&&part.uvDecal&&part.uvDecal.length)? part.glDecal.gl : null,
            clamp:part.clamp|0,
            // Round 18dl: the environment map and the bump map that perturbs it.
            env:(part.glEnv&&part.glEnv.gl)||null, envClamp:part.envClamp|0,
            bump:(part.glEnv&&part.glEnv.gl&&part.glBump&&part.glBump.gl)? part.glBump.gl : null,
            bumpLuma:part.bumpLuma||[1,0], bumpMat:part.bumpMat||[1,0,0,1]};
    if(!lazy) this._ensureVao(b);
    return b;
  }
  /** The batch's own vertex array and instance buffers, built the first time something
   *  draws it on its own (round 18cr - see `buildStaticBatch`). */
  _ensureVao(b){
    if(b.vao) return b;
    const gl=this.gl, part=b.part;
    const inst={m:b.mats, n:b.n, lamp:b.lamp};
    const g=this._partGpu(part);
    const vao=gl.createVertexArray(); gl.bindVertexArray(vao);
    gl.bindBuffer(gl.ARRAY_BUFFER,g.pos); gl.enableVertexAttribArray(0); gl.vertexAttribPointer(0,3,gl.FLOAT,false,0,0);
    gl.bindBuffer(gl.ARRAY_BUFFER,g.nrm); gl.enableVertexAttribArray(1); gl.vertexAttribPointer(1,3,gl.FLOAT,false,0,0);
    gl.bindBuffer(gl.ARRAY_BUFFER,g.uv);  gl.enableVertexAttribArray(2); gl.vertexAttribPointer(2,2,gl.FLOAT,false,0,0);
    if(g.uv2){ gl.bindBuffer(gl.ARRAY_BUFFER,g.uv2); gl.enableVertexAttribArray(7); gl.vertexAttribPointer(7,2,gl.FLOAT,false,0,0); }
    // Round 17y: the dark map's UV set, when the shape has one.
    if(g.uvd){ gl.bindBuffer(gl.ARRAY_BUFFER,g.uvd); gl.enableVertexAttribArray(9); gl.vertexAttribPointer(9,2,gl.FLOAT,false,0,0); }
    // Round 18df: the decal's UV set, when the shape has one.
    if(g.uvdc){ gl.bindBuffer(gl.ARRAY_BUFFER,g.uvdc); gl.enableVertexAttribArray(10); gl.vertexAttribPointer(10,2,gl.FLOAT,false,0,0); }
    gl.bindBuffer(gl.ARRAY_BUFFER,g.col); gl.enableVertexAttribArray(6); gl.vertexAttribPointer(6,4,gl.UNSIGNED_BYTE,true,0,0);
    gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER,g.ib);
    const mb=gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER,mb);
    gl.bufferData(gl.ARRAY_BUFFER,inst.m,gl.STATIC_DRAW);
    for(let k=0;k<3;k++){
      gl.enableVertexAttribArray(3+k);
      gl.vertexAttribPointer(3+k,4,gl.FLOAT,false,48,k*16);
      gl.vertexAttribDivisor(3+k,1);
    }
    /* Round 17w: the lamp hours, two floats an instance, when any instance in the batch
       is a lamp. Left disabled otherwise, so the generic (-1,-1) answers for the lot. */
    let lb=null;
    if(inst.lamp && inst.lamp.length===inst.n*2){
      lb=gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER,lb);
      gl.bufferData(gl.ARRAY_BUFFER,inst.lamp,gl.STATIC_DRAW);
      gl.enableVertexAttribArray(8);
      gl.vertexAttribPointer(8,2,gl.FLOAT,false,0,0);
      gl.vertexAttribDivisor(8,1);
    }
    gl.bindVertexArray(null);
    b.vao=vao; b.mb=mb; b.lb=lb;
    return b;
  }
  /** Frees a batch's own vertex array and instance buffers, if it ever built them. */
  _dropVao(b){
    const gl=this.gl;
    if(!b) return;
    if(b.vao){ try{ gl.deleteVertexArray(b.vao); }catch(_){ } b.vao=null; }
    if(b.mb){ try{ gl.deleteBuffer(b.mb); }catch(_){ } b.mb=null; }
    if(b.lb){ try{ gl.deleteBuffer(b.lb); }catch(_){ } b.lb=null; }
  }
  /** Round 18ag: a skinned part's vertices at `secs`, written over its buffers - once a
   *  frame however many batches and passes draw it, keyed on the clock. The bind pose the
   *  engine baked (`part.pos`) is what the CPU keeps for picking and bounds; what is drawn
   *  is what the bones are doing now. */
  _skinUpdate(part, secs){
    const sk=part.skin, mo=part.morph, g=part._gpu;
    if((!sk && !mo) || !g || g.gl!==this.gl || part._skinAt===secs) return;
    part._skinAt=secs;
    const np=sk? sk.pos.length : part.pos.length;
    if(!part._skinPos || part._skinPos.length!==np){
      part._skinPos=new Float32Array(np);
      part._skinNrm=(sk && sk.nrm)? new Float32Array(sk.nrm.length) : null;
    }
    // Round 18ai: a phase bin's copy of the part carries its own shift into the clip, and
    // (18aj) its own rate.
    const t=secs*(part._animSpeed||1)+(part._animShift||0);
    if(sk) skinVerts(sk, skinBoneMats(sk, t), part._skinPos, part._skinNrm);
    else part._skinPos.set(part.pos);
    // Round 18au: and the morph targets on top - the moths' wingbeat.
    if(mo) morphVerts(mo, t, part._skinPos);
    const gl=this.gl;
    gl.bindBuffer(gl.ARRAY_BUFFER,g.pos); gl.bufferSubData(gl.ARRAY_BUFFER,0,part._skinPos);
    if(part._skinNrm){ gl.bindBuffer(gl.ARRAY_BUFFER,g.nrm); gl.bufferSubData(gl.ARRAY_BUFFER,0,part._skinNrm); }
    gl.bindBuffer(gl.ARRAY_BUFFER,null);
  }
  /* ---------------- one mesh, on its own, as pixels ----------------

     Robin: "I want the thumbnails for the meshes in the 'Inspect a mesh' picker to be the
     actual 3D item", and the same on the grass slots, where "most are just black
     currently". They were: a card showed the *first texture* of the mesh, which for a
     tuft of grass is a mostly-transparent alpha sheet and for a rock is a slab of stone —
     neither of which tells you what the thing looks like, and a mesh whose first part has
     no texture at all showed nothing.

     So the mesh is drawn, through the same shader the viewport draws objects with, into a
     small framebuffer that is read back. Same geometry down the same path: a thumbnail
     that is wrong is wrong in the viewport too, which is the only kind of wrong worth
     having.

     Split in three because rotating one on hover means drawing the same mesh twenty times
     a second: `thumbBegin` buys the GPU buffers, `thumbDraw` spends them at an angle, and
     `thumbEnd` gives them back. A still thumbnail is all three in a row. */

  /** Uploads one mesh's parts and works out how to frame it. */
  thumbBegin(parts){
    if(!parts||!parts.length) return null;
    const m=new Float32Array([1,0,0,0, 0,1,0,0, 0,0,1,0]);
    let lo=[Infinity,Infinity,Infinity], hi=[-Infinity,-Infinity,-Infinity];
    const batches=[];
    for(const part of parts){
      batches.push(this.buildStaticBatch(part,{m,n:1},part.glTex? part.glTex.gl : null));
      const v=part.pos;
      for(let i=0;i<v.length;i+=3)
        for(let k=0;k<3;k++){ const c=v[i+k];
                              if(c<lo[k])lo[k]=c; if(c>hi[k])hi[k]=c; }
    }
    if(!(hi[0]>=lo[0])){ lo=[-1,-1,-1]; hi=[1,1,1]; }
    const c=[(lo[0]+hi[0])/2,(lo[1]+hi[1])/2,(lo[2]+hi[2])/2];
    /* Two radii, not one, and both taken about the centre.
     *
     * A single bounding-sphere radius is the easy answer and it wastes most of the tile:
     * a grass tuft is two hundred units across and thirty tall, so its diagonal is nearly
     * all footprint and framing to it leaves the thing a smudge in the middle of a lot of
     * air — which is what the first attempt looked like.
     *
     * `rxy` is how far it reaches horizontally, which is the same however it is turned;
     * `rz` is how tall it is. `thumbDraw` combines them with the elevation it looks from
     * to get the half-extent it actually has to fit. One number per axis rather than per
     * angle, because a frame that breathed as the mesh turned would be worse than a loose
     * one. */
    let rxy=1, rz=1;
    for(const part of parts){
      const v=part.pos;
      for(let i=0;i<v.length;i+=3){
        rxy=Math.max(rxy,Math.hypot(v[i]-c[0],v[i+1]-c[1]));
        rz=Math.max(rz,Math.abs(v[i+2]-c[2]));
      }
    }
    return {batches,c,rxy,rz};
  }

  /** Draws a prepared mesh at one angle and reads the pixels back. */
  thumbDraw(h,angle,size){
    if(!h) return null;
    const gl=this.gl;
    size=size||96;
    if(!this._thumbFb || this._thumbSize!==size){
      if(this._thumbFb){
        try{ gl.deleteFramebuffer(this._thumbFb); gl.deleteTexture(this._thumbTex);
             gl.deleteRenderbuffer(this._thumbRb); }catch(_){ }
      }
      this._thumbTex=gl.createTexture();
      gl.bindTexture(gl.TEXTURE_2D,this._thumbTex);
      gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,size,size,0,gl.RGBA,gl.UNSIGNED_BYTE,null);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.LINEAR);
      this._thumbRb=gl.createRenderbuffer();
      gl.bindRenderbuffer(gl.RENDERBUFFER,this._thumbRb);
      gl.renderbufferStorage(gl.RENDERBUFFER,gl.DEPTH_COMPONENT16,size,size);
      this._thumbFb=gl.createFramebuffer();
      gl.bindFramebuffer(gl.FRAMEBUFFER,this._thumbFb);
      gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,this._thumbTex,0);
      gl.framebufferRenderbuffer(gl.FRAMEBUFFER,gl.DEPTH_ATTACHMENT,gl.RENDERBUFFER,this._thumbRb);
      this._thumbSize=size;
      if(gl.checkFramebufferStatus(gl.FRAMEBUFFER)!==gl.FRAMEBUFFER_COMPLETE){
        gl.bindFramebuffer(gl.FRAMEBUFFER,null);
        this._thumbFb=null;
        return null;
      }
    }
    gl.bindFramebuffer(gl.FRAMEBUFFER,this._thumbFb);
    gl.viewport(0,0,size,size);
    /* Transparent, so a tile shows the page behind the mesh rather than a black square
       that reads as "failed to load" — which is what the old texture swatches looked
       like whenever the first part had no texture. */
    gl.clearColor(0,0,0,0);
    gl.enable(gl.DEPTH_TEST); gl.depthFunc(gl.LESS);
    gl.disable(gl.BLEND);
    /* Both sides drawn. A thumbnail is a thing on a shelf rather than a room you are
       standing in, and half of Morrowind's foliage is single-sided sheets that would
       vanish for half a turn. */
    gl.disable(gl.CULL_FACE);
    gl.clear(gl.COLOR_BUFFER_BIT|gl.DEPTH_BUFFER_BIT);

    /* How far down to look from, decided by the shape.
     *
     * A fixed elevation suits a cube and wastes a square tile on everything else: a grass
     * tuft is two hundred units across and thirty tall, so seen from a low angle it is a
     * thin band with air above and below it. Looking further down turns the footprint into
     * the picture. Tall things keep the low angle, where their height is the point. */
    const flat=Math.min(1,h.rz/Math.max(1,h.rxy));
    const el=0.36+0.36*(1-flat), az=angle||0;
    /* How much has to fit, and how far back that puts the eye.
     *
     * Across the screen the mesh reaches `rxy` whatever the angle. Up the screen it reaches
     * `rxy*sin(el)` — the footprint, foreshortened by looking down at it — plus `rz*cos(el)`
     * for its height. The square has to hold the larger of the two, and `R/tan(fov/2)` is
     * the exact distance that does, so the only judgement in here is the 1.14 of air. */
    const FOV_T=Math.tan(15*Math.PI/180);
    const R=Math.max(h.rxy, h.rxy*Math.sin(el)+h.rz*Math.cos(el));
    const d=Math.max(1,R/FOV_T)*1.14;
    const eye=[h.c[0]+d*Math.cos(el)*Math.cos(az),
               h.c[1]+d*Math.cos(el)*Math.sin(az),
               h.c[2]+d*Math.sin(el)];
    const V=M4.lookAt(eye,h.c,[0,0,1]);
    // Radians, like `FOV` above — the one degrees-shaped number in this file is the sun
    // angle, and passing 28 here framed every mesh as a speck in the middle of the square.
    const P=M4.persp(30*Math.PI/180, 1, Math.max(0.05,d*0.02), d*4+(h.rxy+h.rz)*4);
    const VP=M4.mul(P,V);
    const st=this.progStatic; gl.useProgram(st.p);
    this._shReset(st); this._tex0=undefined; this._texU=null;   // round 18cr: a borrowed program, told afresh
    gl.uniformMatrix4fv(st.u.uVP,false,VP);
    /* Lit from over the viewer's shoulder rather than from the scene's sun: a thumbnail
       is looked at from one direction and a low sun would leave half of them in shadow. */
    gl.uniform3fv(st.u.uSun,[-Math.cos(az)*0.5,-Math.sin(az)*0.5,0.78]);
    /* Brighter than the scene. A thumbnail is 90 pixels of a thing against a dark panel,
       and Morrowind's grass textures are dark to begin with — lit like a cell they read as
       the black squares Robin was complaining about. */
    gl.uniform1f(st.u.uBright,1.45);
    gl.uniform3fv(st.u.uFogCol,[0,0,0]);
    gl.uniform1f(st.u.uFogK,0);          // no distance haze on a thing 90 pixels wide
    gl.uniform1i(st.u.uScat,0);          // and no sky: a portrait is lit plainly
    if(st.u.uUnder) gl.uniform1i(st.u.uUnder,0);   // 18dt: and not under the water
    if(st.u.uUnderMix) gl.uniform4f(st.u.uUnderMix,0,0,0,0);   // 18dz: nor crossing it
    /* Round 18dl: the portrait's eye and camera frame, for an environment map's sphere
       mapping (the scene's `airOn` sets these for a frame; a portrait sets its own). */
    if(st.u.uEye) gl.uniform3fv(st.u.uEye,eye);
    if(st.u.uCamR){
      const f=[h.c[0]-eye[0],h.c[1]-eye[1],h.c[2]-eye[2]]; const fl=Math.hypot(f[0],f[1],f[2])||1; f[0]/=fl; f[1]/=fl; f[2]/=fl;
      const r=[f[1],-f[0],0]; const rl=Math.hypot(r[0],r[1],r[2])||1; r[0]/=rl; r[1]/=rl; r[2]/=rl;
      const u=[r[1]*f[2]-r[2]*f[1], r[2]*f[0]-r[0]*f[2], r[0]*f[1]-r[1]*f[0]];
      if(st.u.uCamF) gl.uniform3fv(st.u.uCamF,f); gl.uniform3fv(st.u.uCamR,r); gl.uniform3fv(st.u.uCamU,u);
    }
    gl.uniform3fv(st.u.uSunCol,[1,1,1]); gl.uniform3fv(st.u.uAmbCol,[1,1,1]);
    gl.uniform1i(st.u.uTex,0);
    gl.uniform1i(st.u.uHi,-1);
    gl.uniform1i(st.u.uOpaque,1);
    gl.uniform1f(st.u.uTintK,0.0);   // no ground under a portrait
    gl.uniform1i(st.u.uVCol,1);
    gl.uniform1i(st.u.uShowSlope,0); // a thumbnail is a portrait, not a report
    this._cullUniforms(st,false);
    gl.uniform4fv(st.u.uBrushS,[0,0,0,-1]);   // no brush ring on a shelf
    // Round 17w: a portrait shows a lamp lit, whatever the clock says; no clock here.
    gl.uniform1i(st.u.uLightMode,1); gl.uniform1f(st.u.uHour,-1);
    /* Round 18bd (F8): and the five the scene's last frame left behind. A portrait
       borrows the program the viewport just used, so every uniform it does not set is
       still the *scene's* - and `uRoom` is the one that shows: preview an interior, open
       the mesh picker, and the shader takes the room branch, everything clamps to 1 and
       every thumbnail comes out completely flat, which is the class of complaint
       thumbnails exist to answer. The scene's lanterns would light the portrait, an MGE
       install would tonemap it darker, and with Ctrl held the orbit pivot's salmon ring
       would be stamped into it. The same list `bakeGroundMap` sets for the same reason -
       "what a lit frame adds that a portrait must not have" - which its own comment
       already claimed this shared and did not. */
    if(st.u.uRoom)      gl.uniform1i(st.u.uRoom,0);
    if(st.u.uNoLight)   gl.uniform1i(st.u.uNoLight,0);
    if(st.u.uLightN)    gl.uniform1i(st.u.uLightN,0);
    if(st.u.uLampModel) gl.uniform1i(st.u.uLampModel,0);
    if(st.u.uPivot)     gl.uniform4fv(st.u.uPivot,[0,0,0,-1]);
    for(const b of h.batches){
      this._meshTex(st,b);
      this._meshMode(st,b,false);   // lit or unlit as the shape says; both sides, as above
      gl.bindVertexArray(b.vao);
      gl.drawElementsInstanced(gl.TRIANGLES,b.count,gl.UNSIGNED_SHORT,0,1);
    }
    const px=new Uint8Array(size*size*4);
    gl.readPixels(0,0,size,size,gl.RGBA,gl.UNSIGNED_BYTE,px);
    gl.uniform1i(st.u.uOpaque,0);      // the scene draws its own alpha
    gl.bindVertexArray(null);
    gl.bindFramebuffer(gl.FRAMEBUFFER,null);
    /* The viewport is drawn again from scratch every frame and sets its own viewport, so
       nothing here has to be put back — but it does have to be *told*, or the window keeps
       showing the frame it had before this borrowed the context. */
    this.dirty=true;
    /* GL reads bottom-up and every consumer of this draws top-down. Flipped here, once,
       rather than by every caller remembering to. */
    const flip=new Uint8Array(size*size*4);
    const row=size*4;
    for(let y=0;y<size;y++) flip.set(px.subarray((size-1-y)*row,(size-y)*row), y*row);
    return {w:size,h:size,data:flip};
  }

  /** Gives back what `thumbBegin` bought. */
  thumbEnd(h){
    if(!h) return;
    for(const b of h.batches) this._dropVao(b);
    h.batches.length=0;
  }

  disposeStatics(){
    // Round 17w: the particle systems are the objects', and go with them.
    if(this.clearParticles) this.clearParticles();
    for(const b of this.statics) this._dropVao(b);
    for(const b of (this.hlStatics||[])) this._dropVao(b);
    this.hlStatics=[];
    this.statics=[];
    this._dropGroups();   // round 18cr
    this.disposePaintedStatics();
  }
  /** The painted objects' own batches. Their own call as well as part of the one above,
      because the paint changes on every stroke while the objects do not. */
  disposePaintedStatics(){
    const gl=this.gl;
    for(const b of (this.paintedStatics||[])){
      try{ gl.deleteVertexArray(b.vao); }catch(_){ }
      // Batches for one object share a dab texture; delete it once.
      if(b.dabTex && !b.dabTexShared) this.disposeDabTex(b.dabTex);
    }
    this.paintedStatics=[];
  }
  /** A dab texture is two textures once a mask is big enough to carry a bucket grid, so
   *  freeing one is one call and not two remembered separately. `makeDabTex` is the only
   *  thing that makes them and this is the only thing that unmakes them. */
  disposeDabTex(t){
    const gl=this.gl;
    if(!t) return;
    try{ gl.deleteTexture(t.tex); }catch(_){ }
    if(t.grid){ try{ gl.deleteTexture(t.grid.tex); }catch(_){ } }
  }
  setGround(mesh,tex,tile){
    const gl=this.gl;
    for(const c of (this.cellChunks||[])){ try{ gl.deleteVertexArray(c.vao); }catch(_){ } }
    this.cellChunks=[];
    const vao=gl.createVertexArray(); gl.bindVertexArray(vao);
    this._buf(mesh.pos); gl.enableVertexAttribArray(0); gl.vertexAttribPointer(0,3,gl.FLOAT,false,0,0);
    this._buf(mesh.nrm); gl.enableVertexAttribArray(1); gl.vertexAttribPointer(1,3,gl.FLOAT,false,0,0);
    this._buf(mesh.uv);  gl.enableVertexAttribArray(2); gl.vertexAttribPointer(2,2,gl.FLOAT,false,0,0);
    this._colBuf(mesh);  gl.enableVertexAttribArray(3); gl.vertexAttribPointer(3,3,gl.UNSIGNED_BYTE,true,0,0);
    const ib=gl.createBuffer(); gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER,ib);
    gl.bufferData(gl.ELEMENT_ARRAY_BUFFER,mesh.idx,gl.STATIC_DRAW);
    gl.bindVertexArray(null);
    /* Where the patch is, in world units. Round 17s: the ground map is baked over this,
       so the grass tint works in Simplified mode too — and, more to the point, so the map
       is never left describing ground that is no longer on screen. */
    let bx0=Infinity,by0=Infinity,bx1=-Infinity,by1=-Infinity;
    for(let i=0;i<mesh.pos.length;i+=3){
      const x=mesh.pos[i], y=mesh.pos[i+1];
      if(x<bx0)bx0=x; if(x>bx1)bx1=x; if(y<by0)by0=y; if(y>by1)by1=y;
    }
    this.ground={vao,count:mesh.idx.length,tex:tex||null,box:[bx0,by0,bx1,by1]};
    this.opts.tile=tile||4;
    /* The old map described the cell that was on screen a moment ago. Leaving it was the
       bug Robin found: in Simplified mode `bakeGroundMap` was never reached at all (its
       gate wanted cell chunks), so a map baked for a real cell stayed, the patch's blades
       sampled far outside it, and CLAMP_TO_EDGE handed every one of them the same edge
       texel — "makes the grass go white from bottom and upwards". */
    this.groundMapStale=true;
    this.dirty=true;
  }
  makeBlendMap(data,size){
    const gl=this.gl,t=gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D,t);
    gl.pixelStorei(gl.UNPACK_ALIGNMENT,1);
    gl.texImage2D(gl.TEXTURE_2D,0,gl.R8,size,size,0,gl.RED,gl.UNSIGNED_BYTE,data);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.CLAMP_TO_EDGE);
    return t;
  }
  /* ---- painted ground ------------------------------------------------------------
     One R8 texture per cell, the same shape as a coverage map, drawn as one more layer
     over the terrain. Keyed by grid rather than by chunk index: the cells on screen
     change every time somebody steps a cell, and the paint for one of them does not.
     ---------------------------------------------------------------------------------- */

  /** Which colour each rule swatch is drawn in, in the order they are drawn. */
  setPaintPalette(p){
    /* `rule` is kept as well as the colour, and the legend is why: a swatch is a colour
       *and* the rule it stands for, and dropping half of it here made every rule entry in
       the legend read "undefined". Nothing else needed it, which is exactly how it went
       missing — the draw call only ever wanted the colour. */
    this.rulePalette=(p||[]).map(w=>({slot:w.slot|0, colour:w.colour||'#888888',
                                      rule:w.rule||'', layer:w.layer|0}));
    this.dirty=true;
  }
  /** Replaces the paint on some cells.
      `cells` is `[{gx,gy,res,data,grow,layers:[{layer,rule,ruleStrength}]}]` — the two
      "whether" planes, each drawn as its own coloured pass, and one "which rule" pair per
      *paint layer* that has anything on this cell, each of those drawn one pass per
      swatch present in it.

      Keyed `"gx,gy|slot"` and not by layer, deliberately: a slot belongs to exactly one
      layer, so the slot alone identifies the pass, and the draw loop walks the palette
      rather than the layers. What the layer decides is only whether the pass runs at all
      — see `setLayerVisibility`. */
  setCellPaint(cells){
    const gl=this.gl;
    /* Which layer each swatch is on, for deciding whose masks a partial answer replaces.
       Built once per call rather than per cell: a wide stroke touches four cells and the
       palette does not change between them. */
    const layerOf=new Map((this.rulePalette||[]).map(w=>[w.slot|0, w.layer|0]));
    for(const c of (cells||[])){
      const key=c.gx+','+c.gy;
      /* **Absent means "leave it", empty means "there is nothing here".**
         A stroke's answer carries only the planes its brush can have moved, so a rule
         stroke must not clear the cover paint and a cover stroke must not clear the rule
         layers. Presence, not truthiness: a plane erased back to nothing arrives as an
         empty array and has to take its texture with it. */
      for(const [map,plane] of [[this.paintTex,c.data],[this.growTex,c.grow]]){
        if(plane==null) continue;
        const old=map.get(key);
        if(old){ try{ gl.deleteTexture(old); }catch(_){ } map.delete(key); }
        // A plane erased back to nothing keeps no texture: the draw skips a chunk with
        // no entry, which is both cheaper and the only way an erase becomes visible.
        if(!plane.length || !plane.some(v=>v)) continue;
        map.set(key,this.makeBlendMap(plane,c.res||64));
      }
      if(c.layers==null) continue;
      /* The masks this answer replaces, dropped first. For a full answer that is every
         mask on the cell — the engine sends only the layers that have paint, so a layer
         rubbed clean arrives as an absence and its colour has to go with it. For a
         partial one it is only the layers named, because the others were not looked at.
         A mask whose swatch the palette no longer knows is dropped either way: it belongs
         to a colour that has been removed. */
      const named=new Set((c.layers||[]).map(L=>L.layer|0));
      for(const k of [...this.ruleTex.keys()]){
        const bar=k.indexOf('|');
        if(k.slice(0,bar)!==key) continue;
        if(c.partial && named.size){
          const slot=+k.slice(bar+1);
          const owner=layerOf.get(slot);
          if(owner!=null && !named.has(owner)) continue;
        }
        try{ gl.deleteTexture(this.ruleTex.get(k)); }catch(_){ }
        this.ruleTex.delete(k);
      }
      for(const L of (c.layers||[])){
        const slots=L.rule, str=L.ruleStrength;
        if(!slots || !str || !slots.length) continue;
        const present=new Set();
        for(let i=0;i<slots.length;i++) if(slots[i] && str[i]) present.add(slots[i]);
        for(const slot of present){
          const mask=new Uint8Array(slots.length);
          for(let i=0;i<slots.length;i++) if(slots[i]===slot) mask[i]=str[i];
          this.ruleTex.set(key+'|'+slot,this.makeBlendMap(mask,c.res||64));
        }
      }
    }
    this.dirty=true;
  }

  /** Which paint layers are not to be drawn.
   *
   *  The colour and nothing else: the grass these layers place is untouched, in the
   *  preview and in the export. Held as a set of ids rather than as a flag on each swatch
   *  because the layers are the engine's and this is the page passing on what it was
   *  told — a copy per swatch would be four copies of one fact. */
  setLayerVisibility(hidden,hiddenIx){
    this.hiddenLayers=new Set((hidden||[]).map(v=>+v));
    /* And the same layers by *index*, for the paint on placed objects: their marks name
       a layer by its position in the layer list (the number `dab_bytes` resolves), not
       by its id. Two spellings of one fact, both handed over by the caller that knows
       the list. */
    this.hiddenLayerIx=new Set((hiddenIx||[]).map(v=>+v));
    this.dirty=true;
  }
  /** Forgets every paint texture. Called when the install changes under the viewport. */
  clearCellPaint(){
    const gl=this.gl;
    for(const map of [this.paintTex,this.growTex,this.ruleTex]){
      for(const t of map.values()){ try{ gl.deleteTexture(t); }catch(_){ } }
      map.clear();
    }
    this.dirty=true;
  }
  /** Where the brush is, in world units, or null. */
  setBrush(b){
    const a=this.brush, n=b||null;
    if(!a && !n) return;
    /* One centre, three numbers. The brush is a sphere and `x, y, z` is where it is —
       the point under the pointer, which is the rock when the pointer is on a rock and
       the landscape when it is not. Two sets of coordinates were two chances to draw
       the ring somewhere the paint does not go. */
    if(a && n && a.x===n.x && a.y===n.y && a.z===n.z && a.r===n.r && a.inner===n.inner
       && a.onStatics===n.onStatics && a.free===n.free) return;
    this.brush=n;
    this.dirty=true;
  }

  /** one terrain mesh, drawn once per land texture with that texture's coverage map */
  /** The index buffer of a chunk with a range per layer (round 18ct): the whole chunk
   *  first - what the first layer, the overlays and the ground-map bake draw - then, for
   *  each layer after the first that has a coverage map, only the quads of the sub-cells
   *  whose texel or any neighbouring texel is set. The map is sampled with linear
   *  filtering, so a point inside a sub-cell reads its own texel and the ones beside it
   *  and no others: a sub-cell with nothing set in that three-by-three is exactly zero
   *  everywhere, and the shader discarded every fragment of it after lighting it. A
   *  layer whose map covers every sub-cell keeps the whole chunk's range.
   *  `{idx, off[], count[]}`, offsets in bytes. */
  _layerRanges(mesh, ch){
    const full=mesh.idx, nv=mesh.pos.length/3|0, N=Math.round(Math.sqrt(nv));
    const inner=ch.inner||16, per=Math.max(1,(N-1)/inner|0);   // quads a sub-cell is across
    const layers=ch.layers||[];
    const off=new Array(layers.length).fill(0), count=new Array(layers.length).fill(full.length);
    const parts=[full];
    let at=full.length;
    const cov=new Uint8Array(inner*inner);
    for(let i=1;i<layers.length;i++){
      const L=layers[i], m=L.blend, P=L.size|0;
      if(!m || !P || m.length!==P*P) continue;
      const border=Math.max(0,(P-inner)>>1);
      cov.fill(0);
      let covered=0;
      for(let sy=0;sy<inner;sy++) for(let sx=0;sx<inner;sx++){
        const tx=sx+border, ty=sy+border;
        let any=false;
        for(let dy=-1;dy<=1 && !any;dy++){
          const y=ty+dy; if(y<0||y>=P) continue;
          for(let dx=-1;dx<=1;dx++){ const x=tx+dx; if(x<0||x>=P) continue; if(m[y*P+x]){ any=true; break; } }
        }
        if(any){ cov[sy*inner+sx]=1; covered++; }
      }
      if(covered===inner*inner) continue;   // the whole chunk, as before
      const sub=[];
      for(let t=0;t+5<full.length;t+=6){
        const a=full[t], x=a%N, y=(a/N)|0;
        const sx=Math.min(inner-1,(x/per)|0), sy=Math.min(inner-1,(y/per)|0);
        if(cov[sy*inner+sx]) for(let k=0;k<6;k++) sub.push(full[t+k]);
      }
      off[i]=at*4; count[i]=sub.length;
      if(sub.length){ parts.push(Uint32Array.from(sub)); at+=sub.length; }
    }
    if(parts.length===1) return {idx:full, off, count};
    const idx=new Uint32Array(at);
    let w=0; for(const p of parts){ idx.set(p,w); w+=p.length; }
    return {idx, off, count};
  }
  setCellTerrain(chunks){
    const gl=this.gl;
    /* Round 17v: the coverage maps go with the chunks. Every layer of every chunk gets a
       texture of its own from `makeBlendMap`, and only the vertex arrays were being freed
       — so a 49-cell load left 472 of them behind and the next load left 472 more. Small
       textures, but they are made and dropped on every reload and nothing else was ever
       going to collect them. */
    /* Round 17y: a chunk handed over again is kept as it is. A step to the next cell
       rebuilds the scene with most of its chunks unchanged - the cell preview hands the
       same chunk objects back - and uploading their vertex arrays and coverage maps
       again was paid for nothing. What is freed is what did not come back. */
    const keep=new Set(chunks||[]);
    const held=new Map();
    for(const c of (this.cellChunks||[])){
      if(c.src && keep.has(c.src)){ held.set(c.src,c); continue; }
      try{ gl.deleteVertexArray(c.vao); }catch(_){ }
      for(const L of (c.layers||[])) if(L.blend){ try{ gl.deleteTexture(L.blend); }catch(_){ } }
    }
    this.cellChunks=[];
    for(const ch of (chunks||[])){
      const had=held.get(ch);
      if(had){ this.cellChunks.push(had); continue; }
      const mesh=ch.mesh;
      const vao=gl.createVertexArray(); gl.bindVertexArray(vao);
      this._buf(mesh.pos); gl.enableVertexAttribArray(0); gl.vertexAttribPointer(0,3,gl.FLOAT,false,0,0);
      this._buf(mesh.nrm); gl.enableVertexAttribArray(1); gl.vertexAttribPointer(1,3,gl.FLOAT,false,0,0);
      this._buf(mesh.uv);  gl.enableVertexAttribArray(2); gl.vertexAttribPointer(2,2,gl.FLOAT,false,0,0);
      this._colBuf(mesh);  gl.enableVertexAttribArray(3); gl.vertexAttribPointer(3,3,gl.UNSIGNED_BYTE,true,0,0);
      /* Round 18ct: the index buffer holds the whole chunk first and then, per layer
         after the first, only the quads its coverage map can touch (`_layerRanges`) -
         the rest of a layer's fragments were shaded whole and then discarded. */
      const ranges=this._layerRanges(mesh, ch);
      const ib=gl.createBuffer(); gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER,ib);
      gl.bufferData(gl.ELEMENT_ARRAY_BUFFER,ranges.idx,gl.STATIC_DRAW);
      gl.bindVertexArray(null);
      /* Where the chunk is in scene xy, from the vertices themselves, for the ground
         map: a blade's tint is looked up by position, so the map has to know what
         ground it covers. */
      let bx0=Infinity,by0=Infinity,bx1=-Infinity,by1=-Infinity,bz0=Infinity,bz1=-Infinity;
      for(let i=0;i<mesh.pos.length;i+=3){
        const x=mesh.pos[i], y=mesh.pos[i+1], z=mesh.pos[i+2];
        if(x<bx0)bx0=x; if(x>bx1)bx1=x; if(y<by0)by0=y; if(y>by1)by1=y; if(z<bz0)bz0=z; if(z>bz1)bz1=z;
      }
      // What this chunk actually covers, in world units. Kept because the water sheet is
      // cut to the ground and something has to be able to say what the ground was.
      // Round 18cs: and in z (`box3`), so a chunk off screen is not drawn - the terrain
      // pass never culled - and the water knows which cells it can show through.
      this.cellChunks.push({vao,count:mesh.idx.length,span:ch.span||null,box:[bx0,by0,bx1,by1],box3:[bx0,by0,bz0,bx1,by1,bz1],
        src:ch,   // the chunk object this was uploaded from, so it can be recognised next time
        // Which cell this is, so the paint for it can be found. The chunks are rebuilt
        // whenever the view moves and the paint is not, so they cannot hold it directly.
        grid:ch.grid||null,
        inner:ch.inner||16,total:ch.total||(ch.inner||16),
        layers:(ch.layers||[]).map((L,i)=>({id:L.id||'', tex:L.tex||null, flat:L.flat||null,
          nrm:L.nrm||null, specA:!!L.specA,
          blend:L.blend? this.makeBlendMap(L.blend,L.size||16):null,
          off:ranges.off[i]|0, count:ranges.count[i]|0}))});
    }
    if(chunks&&chunks.length) this.ground=null;   // cell terrain replaces the plane
    this.groundMapStale=true;   // the grass reads its tint from what was just replaced
    this._buildSlab(chunks||[]);
    this.dirty=true;
  }

  /* ---- the ground as a slab --------------------------------------------------------
     Round 12 item 5. Robin: "make the terrain look like a slab of ground cut up, by
     extruding a box down from the terrain sides, painted with the default dirt texture.
     Same with a plane as a bottom." A sheet of landscape floating in fog reads as a
     picture; a block of ground reads as a place.

     Built from the chunks themselves: every side of a chunk that has no loaded chunk
     beside it gets a wall from its edge vertices straight down to the base, and one
     plane closes the bottom under the whole of it. The base is the lowest drawn vertex
     less a fixed fraction of the ground's width — Robin's ask, so that an eighth of a
     cell, a cell and nine cells all read as the same block rather than a thin slice
     under a wide one and a deep plinth under a small one. Dirt on all of it: the
     default land texture when the install has it, its flat stand-in colour when not.
     ---------------------------------------------------------------------------------- */
  _buildSlab(chunks){
    const gl=this.gl;
    if(this.slab){ try{ gl.deleteVertexArray(this.slab.vao); }catch(_){ } this.slab=null; }
    if(!chunks.length) return;
    const N=65, DEPTH=1/16, TILE=512;
    let x0=Infinity,y0=Infinity,x1=-Infinity,y1=-Infinity,zmin=Infinity;
    const info=[];
    for(const ch of chunks){
      const pos=ch.mesh.pos, span=ch.span||[-4096,4096];
      if(!pos || pos.length<N*N*3) continue;
      const v0=Math.max(0,Math.round((span[0]+4096)/128)), v1=Math.min(N-1,Math.round((span[1]+4096)/128));
      for(let y=v0;y<=v1;y++)for(let x=v0;x<=v1;x++){
        const k=(y*N+x)*3;
        if(pos[k]<x0)x0=pos[k]; if(pos[k]>x1)x1=pos[k];
        if(pos[k+1]<y0)y0=pos[k+1]; if(pos[k+1]>y1)y1=pos[k+1];
        if(pos[k+2]<zmin)zmin=pos[k+2];
      }
      info.push({pos,v0,v1,grid:ch.grid||[0,0]});
    }
    if(!info.length || !(x1>x0) || !(y1>y0)) return;
    const base=zmin-Math.max(x1-x0,y1-y0)*DEPTH;
    const has=(gx,gy)=>info.some(c=>c.grid[0]===gx && c.grid[1]===gy);
    const P=[],Nn=[],U=[],I=[];
    let nv=0;
    /* The edges the walls stand on, kept for the water's skirt (round 17i): it fills the
       band between the water line and the ground, and needs the same profile the slab's
       walls are cut to. */
    const edges=[];
    // One wall: the edge's vertices in order, then the same xy at the base.
    const wall=(pts,nrm)=>{
      edges.push(pts.map(q=>[q[0],q[1],q[2]]));
      const first=nv;
      let along=0;
      for(let i=0;i<pts.length;i++){
        const [x,y,z]=pts[i];
        if(i>0) along+=Math.hypot(x-pts[i-1][0],y-pts[i-1][1]);
        P.push(x,y,z, x,y,base); Nn.push(...nrm,...nrm);
        U.push(along/TILE,z/TILE, along/TILE,base/TILE);
        nv+=2;
      }
      for(let i=0;i+1<pts.length;i++){
        const a=first+i*2, b=a+1, c=a+2, d=a+3;   // a top, b bottom, c next top, d next bottom
        I.push(a,b,c, b,d,c);
      }
    };
    for(const c of info){
      const {pos,v0,v1,grid}=c;
      const at=(x,y)=>{ const k=(y*N+x)*3; return [pos[k],pos[k+1],pos[k+2]]; };
      const full=(v0===0 && v1===N-1);
      const row=(y)=>{ const o=[]; for(let x=v0;x<=v1;x++) o.push(at(x,y)); return o; };
      const col=(x)=>{ const o=[]; for(let y=v0;y<=v1;y++) o.push(at(x,y)); return o; };
      if(!(full && has(grid[0],grid[1]-1))) wall(row(v0),[0,-1,0]);
      if(!(full && has(grid[0],grid[1]+1))) wall(row(v1),[0,1,0]);
      if(!(full && has(grid[0]-1,grid[1]))) wall(col(v0),[-1,0,0]);
      if(!(full && has(grid[0]+1,grid[1]))) wall(col(v1),[1,0,0]);
    }
    // The bottom: one plane under everything, facing down.
    {
      const first=nv;
      for(const [x,y] of [[x0,y0],[x1,y0],[x1,y1],[x0,y1]]){
        P.push(x,y,base); Nn.push(0,0,-1); U.push(x/TILE,y/TILE); nv++;
      }
      I.push(first,first+2,first+1, first,first+3,first+2);
    }
    const mesh={pos:new Float32Array(P), nrm:new Float32Array(Nn), uv:new Float32Array(U),
                idx:new Uint32Array(I)};
    const vao=gl.createVertexArray(); gl.bindVertexArray(vao);
    this._buf(mesh.pos); gl.enableVertexAttribArray(0); gl.vertexAttribPointer(0,3,gl.FLOAT,false,0,0);
    this._buf(mesh.nrm); gl.enableVertexAttribArray(1); gl.vertexAttribPointer(1,3,gl.FLOAT,false,0,0);
    this._buf(mesh.uv);  gl.enableVertexAttribArray(2); gl.vertexAttribPointer(2,2,gl.FLOAT,false,0,0);
    this._colBuf(mesh);  gl.enableVertexAttribArray(3); gl.vertexAttribPointer(3,3,gl.UNSIGNED_BYTE,true,0,0);
    const ib=gl.createBuffer(); gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER,ib);
    gl.bufferData(gl.ELEMENT_ARRAY_BUFFER,mesh.idx,gl.STATIC_DRAW);
    gl.bindVertexArray(null);
    this.slab={vao,count:mesh.idx.length,base,box:[x0,y0,x1,y1],edges,rev:(this._slabRev=(this._slabRev||0)+1),
               tex:this.slabLook? this.slabLook.tex : null,
               flat:this.slabLook? this.slabLook.flat : null};
  }
  /** What the slab is painted with: the default land texture, or its flat colour.
   *  Set by the page once it knows; the slab keeps drawing with what it had until then. */
  setSlabLook(look){
    this.slabLook=look||null;
    if(this.slab){ this.slab.tex=look? look.tex : null; this.slab.flat=look? look.flat : null; }
    this.dirty=true;
  }
  /* ---- the ground map -------------------------------------------------------------
     Round 11 item 11, Robin's "diffuse transmission": grass tinted by the ground it
     stands on at its base, its own colour by the tip. The ground's colour at a point
     is what the terrain pass paints there - every land texture through its coverage
     map, times the record's vertex colours - so rather than work it out again per
     blade on the CPU (which texture is under it, what that texture's average colour
     is, what VCLR says there), the terrain is drawn once more from straight above,
     unlit, into a square texture, and the grass vertex shader reads its tint from
     that at the blade's position. Whatever the ground pass draws, the grass takes -
     a new texture, a retexture, a plugin's VCLR - with nothing to keep in step.

     1024 square over the loaded ground: nine cells is 24 units a texel, four times
     finer than the sub-cell a texture change happens on. Baked when the terrain is
     replaced or the blend switch moves, not per frame; it is not read at all while
     the slider is at zero. Paint and highlights are left out - marks on the tool.
     ---------------------------------------------------------------------------------- */
  bakeGroundMap(o){
    const gl=this.gl, chunks=this.cellChunks||[];
    this.groundMapStale=false;
    /* Round 17v: a record of what this bake did, for the report. Three rounds of "the
       grass goes white at the foot" have each had a different cause and none of them
       reproduces in the sandbox, where the map comes out right cell after cell. The map
       is written on a framebuffer nobody can see, so the only honest way to tell a bake
       that did not run from one that ran and drew nothing is to count both. */
    this._gmBakes=(this._gmBakes||0)+1;
    const bk=this._gmLast={n:this._gmBakes, drew:0, skipped:0, chunks:chunks.length,
                           layers:0, patch:false, why:''};
    this._gmBlend=o.blend; this._gmVcol=o.vcol;
    /* Round 17s: the invented patch is ground too. Simplified mode has no cell chunks, so
       this used to give up here — without clearing the map, which left the last real
       cell's describing a piece of world nowhere near the patch. */
    /* Round 18r: and not in the editor. `setCellTerrain([])` does not clear `this.ground`,
       so the patch a Simplified preview invented survives being left behind - and in the
       editor it would then win the extent from the mesh that is the only surface there. */
    const patch=(!chunks.length && !this.opts.inspecting && this.ground && this.ground.box)
      ? this.ground : null;
    /* The mesh being inspected is ground - **and nothing else is**.
     *
     * Round 18q put every placed object in the map, reading "the tint [...] also on
     * meshes" as "a blade standing on a rock should take the rock's colour". That is not
     * what it does. The map is a square looked at from straight above, so an object's
     * footprint claims the ground *under* it: a blade on open ground beneath a tree read
     * the canopy, twenty feet over its head. Robin, round 18r: "The tint should be what is
     * underneath the blades and nothing else. This has worked perfectly before [...] If
     * the color is NOT the same as the thing the blade intersects the whole concept of the
     * tint doesn't work (which is to blend into the ground)." Exactly so, and a top-down
     * map cannot tell "standing on it" from "standing under it" - so in a cell the map is
     * the terrain, as it was before 18q.
     *
     * The editor is the one place an object *is* what the blade intersects: there is no
     * terrain there, the inspected mesh is the only surface, and the grass is scattered
     * onto it. That was the half of his sentence that was really missing - "no matter if
     * in the preview cell, simplified preview, or mesh editing mode" - and it is all that
     * is left of 18q's change.
     *
     * A static batch carries its per-cell runs and their world boxes (`groups`) rather
     * than one box of its own; in the editor there is one batch and no runs at all. */
    const objs=this.opts.inspecting? (this.statics||[]).filter(b=>b && b.n) : [];
    const objBox=b=>{
      if(b.box) return b.box;
      if(!b.groups || !b.groups.length) return null;
      const o=[Infinity,Infinity,Infinity,-Infinity,-Infinity,-Infinity];
      for(const g of b.groups){
        for(let k=0;k<3;k++){ if(g.b[k]<o[k]) o[k]=g.b[k]; if(g.b[k+3]>o[k+3]) o[k+3]=g.b[k+3]; }
      }
      return o;
    };
    if(!chunks.length && !patch && !objs.length){
      bk.why='nothing to bake'; if(this.groundMap) this.groundMap.xf=null; return;
    }
    let x0=Infinity,y0=Infinity,x1=-Infinity,y1=-Infinity;
    if(patch){ [x0,y0,x1,y1]=patch.box; }
    for(const ch of chunks){
      if(!ch.box) continue;
      x0=Math.min(x0,ch.box[0]); y0=Math.min(y0,ch.box[1]);
      x1=Math.max(x1,ch.box[2]); y1=Math.max(y1,ch.box[3]);
    }
    /* The editor's own extent: the mesh is the ground, so the mesh sets the square. (In a
       cell `objs` is empty and this is dead - the terrain sets it, as it always did.) */
    if(!chunks.length && !patch && objs.length){
      for(const b of objs){
        const bb=objBox(b); if(!bb) continue;
        x0=Math.min(x0,bb[0]); y0=Math.min(y0,bb[1]);
        x1=Math.max(x1,bb[3]); y1=Math.max(y1,bb[4]);
      }
      if(!isFinite(x0)||!isFinite(y0)||!isFinite(x1)||!isFinite(y1)){
        // Objects with no box to speak of - a thumbnail's batch, say. Nothing to bake.
        bk.why='no extent'; if(this.groundMap) this.groundMap.xf=null; return;
      }
      // A mesh standing at a point has no extent at all; give it something to draw into.
      if(x1-x0<1){ const c=(x0+x1)/2; x0=c-64; x1=c+64; }
      if(y1-y0<1){ const c=(y0+y1)/2; y0=c-64; y1=c+64; }
    }
    if(!(x1>x0 && y1>y0)){ bk.why='no extent'; if(this.groundMap) this.groundMap.xf=null; return; }
    const S=1024;
    if(!this.groundMap){
      const tex=gl.createTexture();
      gl.bindTexture(gl.TEXTURE_2D,tex);
      gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,S,S,0,gl.RGBA,gl.UNSIGNED_BYTE,null);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.CLAMP_TO_EDGE);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.CLAMP_TO_EDGE);
      const fb=gl.createFramebuffer();
      gl.bindFramebuffer(gl.FRAMEBUFFER,fb);
      gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,tex,0);
      this.groundMap={tex,fb,xf:null};
    }
    /* Round 17v, second pass. Robin's report from an RTX 5080: bake #6, 49 chunks, 436
       draws issued — and 100% of the map never drawn on. The clear landed and the readback
       came off the same framebuffer, so the target is sound; the *draws* are being
       swallowed, and only after the first load. The sandbox's software GL draws them every
       time, so whatever swallows them is the driver's, and the honest move is to leave it
       nothing to object to:

       - The map's own texture is still bound on unit 2 from the last grass pass. No sampler
         of the ground program points there, so by the letter of the spec that is not a
         feedback loop — but it is the one thing that differs between the first bake (the
         texture did not exist yet) and every bake after it, and a render target that is
         also a bound texture is exactly what a D3D11 driver is entitled to refuse quietly.
         So it is taken off the unit before a single draw goes in.
       - The three light samplers read whatever the frame left on units 7-9. The stand-ins
         are type-correct by construction; the frame's own grid is rebound after the bake
         anyway (`_bindLights` in `draw`), so nothing is lost by putting them there now.
       - `uClip` belongs to the reflection pass and the bake never set it. A frame that
         ended in the reflection pass would leave the ground shader discarding everything
         under the water line, which is the whole cell when the cell is above the sea.
       And the GL error state, the framebuffer's completeness and whether the first chunk's
       centre actually took paint are all recorded, so the report names the failure rather
       than describing its shadow. */
    this._bindLights(null);
    gl.activeTexture(gl.TEXTURE2); gl.bindTexture(gl.TEXTURE_2D,this.white);
    gl.activeTexture(gl.TEXTURE0);
    for(let i=0;i<8 && gl.getError()!==gl.NO_ERROR;i++){}   // drain what came before
    gl.bindFramebuffer(gl.FRAMEBUFFER,this.groundMap.fb);
    gl.viewport(0,0,S,S);
    gl.disable(gl.DEPTH_TEST); gl.disable(gl.CULL_FACE); gl.disable(gl.BLEND);
    /* Alpha zero: nothing has been drawn here yet. The unlit branch of the ground
       shader writes 1 for a layer's own ground and its coverage for the layers above,
       so anything the terrain touched comes out well above a half and anything it did
       not stays at nothing. The grey is only so that a look at this texture reads. */
    gl.clearColor(0.5,0.5,0.5,0); gl.clear(gl.COLOR_BUFFER_BIT);
    bk.fbo = gl.checkFramebufferStatus(gl.FRAMEBUFFER)===gl.FRAMEBUFFER_COMPLETE;
    /* Straight down: x and y across the square, z ignored. Column-major, as `M4` is. */
    const VP=new Float32Array([
      2/(x1-x0),0,0,0,  0,2/(y1-y0),0,0,  0,0,-1e-6,0,
      -(x1+x0)/(x1-x0), -(y1+y0)/(y1-y0), 0, 1]);
    const g=this.progGround; gl.useProgram(g.p);
    gl.uniformMatrix4fv(g.u.uVP,false,VP);
    gl.uniform1i(g.u.uUnlit,1);
    if(g.u.uClip)  gl.uniform1i(g.u.uClip,0);
    if(g.u.uClipZ) gl.uniform1f(g.u.uClipZ,-1e9);
    gl.uniform1i(g.u.uVCol, o.vcol===false?0:1);
    gl.uniform1f(g.u.uTile,16.0);
    gl.uniform1i(g.u.uTex,0);
    gl.uniform1i(g.u.uBlend,1);
    gl.uniform1i(g.u.uUseBlend, o.blend===false?0:1);
    gl.uniform1f(g.u.uAlphaMul,1.0);
    /* The invented patch, when that is what is on screen (round 17s). One layer, no
       coverage map, its own tiling — the same three uniforms the plain-patch branch of
       the ground pass sets, for the same reason: without them the shader samples whatever
       sits on unit 1 and discards where it reads dark. */
    if(patch){
      gl.uniform1f(g.u.uTile,o.tile||4);
      gl.uniform1i(g.u.uFirstLayer,1);
      gl.uniform1i(g.u.uFlat,0);
      gl.uniform2f(g.u.uBlendXf,1,0);
      gl.disable(gl.BLEND);
      gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D,patch.tex||this.white);
      gl.uniform1i(g.u.uHasTex,patch.tex?1:0);
      gl.activeTexture(gl.TEXTURE1); gl.bindTexture(gl.TEXTURE_2D,this.white);
      gl.bindVertexArray(patch.vao);
      gl.drawElements(gl.TRIANGLES,patch.count,gl.UNSIGNED_INT,0);
      bk.patch=true; bk.drew++;
    }
    for(const ch of chunks){
      gl.bindVertexArray(ch.vao);
      const inner=ch.inner||16, total=ch.total||inner;
      gl.uniform2f(g.u.uBlendXf, inner/total, (total-inner)/(2*total));
      for(let i=0;i<ch.layers.length;i++){
        const L=ch.layers[i], first=(i===0);
        if(first) gl.disable(gl.BLEND);
        else { gl.enable(gl.BLEND); gl.blendFunc(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA); }
        gl.uniform1i(g.u.uFirstLayer, first?1:0);
        gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D,L.tex||this.white);
        gl.uniform1i(g.u.uHasTex,L.tex?1:0);
        gl.uniform1i(g.u.uFlat, (!L.tex&&L.flat)?1:0);
        gl.uniform3fv(g.u.uFlatCol, L.flat||[0.29,0.285,0.255]);
        gl.activeTexture(gl.TEXTURE1); gl.bindTexture(gl.TEXTURE_2D,L.blend||this.white);
        /* A layer the install could not supply — no texture and no stand-in colour — is
           skipped rather than drawn (round 17u). It used to draw white, which is worse
           than leaving the layer beneath it showing: the grass reads its tint from this
           map, so a texture that failed to load turned every blade standing on it white
           at the foot. Robin: "Not all textures actually work for the ground tint." */
        bk.layers++;
        if(!first && !L.tex && !L.flat){ bk.skipped++; continue; }
        gl.drawElements(gl.TRIANGLES,ch.count,gl.UNSIGNED_INT,0);
        bk.drew++;
      }
    }
    gl.disable(gl.BLEND);
    /* And the inspected mesh, in the editor. Its own program, the bake's projection, unlit
       and with nothing else on: no slope shade (that is a report about ground, not a
       colour of it), no highlight, no ground tint of its own. */
    if(objs.length){
      const st=this.progStatic; gl.useProgram(st.p);
      this._shReset(st); this._tex0=undefined; this._texU=null;   // round 18cr
      gl.uniformMatrix4fv(st.u.uVP,false,VP);
      gl.uniform1f(st.u.uBright,1.0);
      gl.uniform1i(st.u.uShowSlope,0);
      this._cullUniforms(st,false);
      gl.uniform1i(st.u.uHi,-1);
      gl.uniform1f(st.u.uTintK,0.0);
      gl.uniform1i(st.u.uVCol, o.vcol===false?0:1);
      gl.uniform1i(st.u.uOpaque,0);
      if(st.u.uClip) gl.uniform1i(st.u.uClip,0);
      if(st.u.uGrid) gl.uniform1i(st.u.uGrid,0);
      if(st.u.uHour) gl.uniform1f(st.u.uHour,-1);
      if(st.u.uLightMode) gl.uniform1i(st.u.uLightMode,0);
      if(st.u.uIndoors) gl.uniform1i(st.u.uIndoors,0);
      /* Round 18r: everything between the albedo and the pixel, turned off.
       *
       * The map is albedo - the grass shader lights the blade and the colour it took
       * together, which is what makes a tinted blade read as lit like the ground beside
       * it. The ground pass gets that for free: its `uUnlit` branch writes the colour and
       * returns. The object pass does not - `FS_GRASS` computes the unlit colour and then
       * goes on through the brush ring, the orbit pivot's stamp and `airFog`, and with the
       * atmosphere on `airFog` ignores `uFogK` entirely and runs aerial perspective from
       * `uEye`, which this pass never sets. So every object baked into the map came out
       * washed towards the sky: Robin, "the ground tint picks the wrong color, it is now
       * very white in places". The pivot's salmon ring would have been baked in too.
       *
       * Set as a group, and named, because the list is "what a lit frame adds that a
       * colour map must not have" - the same list `thumbDraw` turns off for the same
       * reason. */
      if(st.u.uScat)     gl.uniform1i(st.u.uScat,0);
      if(st.u.uUnder)    gl.uniform1i(st.u.uUnder,0);   // 18dt: nor the water's fog, whatever the last frame's eye was in
      if(st.u.uUnderMix) gl.uniform4f(st.u.uUnderMix,0,0,0,0);   // 18dz: nor crossing it
      if(st.u.uFogBlack) gl.uniform1i(st.u.uFogBlack,0);
      if(st.u.uFogK)     gl.uniform1f(st.u.uFogK,0.0);
      /* Round 18dl: the bake looks straight down, and an environment map is read from
         the reflection in the camera's frame - given that frame, and an eye far above,
         rather than whatever the last frame left in these. */
      if(st.u.uEye)      gl.uniform3fv(st.u.uEye,[0,0,1e6]);
      if(st.u.uCamR){ if(st.u.uCamF) gl.uniform3fv(st.u.uCamF,[0,0,-1]); gl.uniform3fv(st.u.uCamR,[1,0,0]); gl.uniform3fv(st.u.uCamU,[0,1,0]); }
      if(st.u.uBrushS)   gl.uniform4fv(st.u.uBrushS,[0,0,0,-1]);
      if(st.u.uPivot)    gl.uniform4fv(st.u.uPivot,[0,0,0,-1]);
      if(st.u.uGrassLit) gl.uniform1i(st.u.uGrassLit,0);
      for(const b of objs){
        this._meshTex(st,b);
        this._meshMode(st,b,true);
        /* After `_meshMode`, which sets `uUnlit` from the shape's own material flag - so
           setting it once before the loop set it for the first batch and no other. A
           rock's albedo is what the map wants whatever its NIF says about lighting. */
        this._u1i(st,'uUnlit',1);
        gl.bindVertexArray(b.vao);
        /* The whole batch, from the start of its instance stream. The frame's own pass
           leaves the attribute pointers wherever its last visible *run* began (there is
           no `baseInstance` in WebGL2, so a run is drawn by moving them), and drawing
           every instance from an offset walks off the end of the buffer - which is an
           error, and the one that turned up on the queue two tests later. */
        if(b.mb){
          gl.bindBuffer(gl.ARRAY_BUFFER,b.mb);
          for(let k=0;k<3;k++) gl.vertexAttribPointer(3+k,4,gl.FLOAT,false,48,k*16);
          if(b.lb){ gl.bindBuffer(gl.ARRAY_BUFFER,b.lb); gl.vertexAttribPointer(8,2,gl.FLOAT,false,0,0); }
        }
        gl.drawElementsInstanced(gl.TRIANGLES,b.count,gl.UNSIGNED_SHORT,0,b.n);
        bk.drew++; bk.objects=(bk.objects||0)+1;
      }
      gl.disable(gl.BLEND);
    }
    gl.bindVertexArray(null);
    /* What the draws left behind, read while the target is still bound. The error is
       the first the draws raised, by number; the report names it. The landing test reads
       one 2x2 block at the centre of the first thing drawn — a chunk's box, or the patch
       — and asks whether it took paint. A few hundred bytes back from the card once a
       load, for a question three rounds could not answer. */
    bk.err=gl.getError();
    {
      const first=(patch? patch.box : (chunks.find(c=>c.box)||{}).box)||null;
      if(first){
        const cx=(first[0]+first[2])/2, cy=(first[1]+first[3])/2;
        const px=Math.max(0,Math.min(S-2,Math.round((cx-x0)/(x1-x0)*(S-1))));
        const py=Math.max(0,Math.min(S-2,Math.round((cy-y0)/(y1-y0)*(S-1))));
        const q=new Uint8Array(16);
        gl.readPixels(px,py,2,2,gl.RGBA,gl.UNSIGNED_BYTE,q);
        bk.landed=(q[3]+q[7]+q[11]+q[15])>=2*128;
        bk.sample=[q[0],q[1],q[2],q[3]];
      }
    }
    gl.bindFramebuffer(gl.FRAMEBUFFER,null);
    gl.activeTexture(gl.TEXTURE0);
    this.groundMap.xf=[x0,y0,1/(x1-x0),1/(y1-y0)];
    bk.box=[x0,y0,x1,y1];
  }

  /** What is actually on the ground map, read back off its framebuffer.
   *
   *  Round 17v. The map is 1024x1024 and read once, when the report is opened, so this is
   *  a megabyte of readback and no cost at all to a frame. It answers the question three
   *  rounds of the white-grass bug have not been able to: whether the bake drew the
   *  ground, drew nothing (leaving the grey it clears to), or drew white.
   *
   *  `at` is a scene xy to sample as well - a blade's foot, so the report can say what
   *  the tint that blade reads actually is rather than what the average is.
   */
  readGroundMap(at){
    const gl=this.gl, g=this.groundMap;
    if(!g || !g.fb || !g.xf) return null;
    const S=1024, a=new Uint8Array(S*S*4);
    const prev=gl.getParameter(gl.FRAMEBUFFER_BINDING);
    gl.bindFramebuffer(gl.FRAMEBUFFER,g.fb);
    gl.readPixels(0,0,S,S,gl.RGBA,gl.UNSIGNED_BYTE,a);
    gl.bindFramebuffer(gl.FRAMEBUFFER,prev||null);
    let s0=0,s1=0,s2=0,clear=0,white=0;
    for(let i=0;i<a.length;i+=4){
      s0+=a[i]; s1+=a[i+1]; s2+=a[i+2];
      // Alpha says it: the bake clears to nothing and writes ground at full strength.
      if(a[i+3]<128) clear++;
      if(a[i]>=250&&a[i+1]>=250&&a[i+2]>=250) white++;
    }
    const n=S*S;
    const out={mean:[Math.round(s0/n),Math.round(s1/n),Math.round(s2/n)],
               clear:100*clear/n, white:100*white/n};
    if(at){
      const u=(at[0]-g.xf[0])*g.xf[2], v=(at[1]-g.xf[1])*g.xf[3];
      out.uv=[u,v];
      if(u>=0&&u<=1&&v>=0&&v<=1){
        const px=Math.min(S-1,Math.max(0,Math.round(u*(S-1))));
        const py=Math.min(S-1,Math.max(0,Math.round(v*(S-1))));
        const i=(py*S+px)*4;
        out.at=[a[i],a[i+1],a[i+2]];
      }
    }
    return out;
  }

  /** Objects the pointer can pick out of the viewport: world bounds and, where the
      mesh has geometry, the triangles to confirm against. */
  setPickables(list){ this.pickables=list||[]; }

  /** Where the camera is, in scene coordinates.
   *
   *  The same arithmetic `rayAt` does for its origin, said once so the brush and the
   *  picker cannot end up with two ideas of where you are standing.
   */
  cameraEye(){
    const c=this.cam;
    const ce=Math.cos(c.el), se=Math.sin(c.el);
    const ca=Math.cos(c.az), sa=Math.sin(c.az);
    return [c.tx+c.dist*ce*ca, c.ty+c.dist*ce*sa, c.tz+c.dist*se];
  }

  /** The ray from the eye through a point on screen: `{eye, dir}`, both world. */
  rayAt(clientX,clientY){
    const rect=this.cv.getBoundingClientRect();
    const W=rect.width||1, H=rect.height||1;
    const ndcX=((clientX-rect.left)/W)*2-1;
    const ndcY=1-((clientY-rect.top)/H)*2;
    const c=this.cam;
    const ce=Math.cos(c.el), se=Math.sin(c.el);
    const ca=Math.cos(c.az), sa=Math.sin(c.az);
    const back=[ce*ca, ce*sa, se];
    const right=[-sa, ca, 0];
    const up=[-se*ca, -se*sa, ce];
    const t=Math.tan(FOV/2), asp=W/H;
    const eye=this.cameraEye();
    let d=[0,0,0];
    for(let k=0;k<3;k++) d[k]=-back[k]+right[k]*(ndcX*t*asp)+up[k]*(ndcY*t);
    const dl=Math.hypot(d[0],d[1],d[2])||1;
    return {eye, dir:[d[0]/dl,d[1]/dl,d[2]/dl]};
  }

  /** Where a screen point meets the ground, as `[x,y,z]`, or null.
   *
   *  Marched rather than solved. The ground is a heightfield with no closed form and
   *  sixty-five squared vertices a cell, and this runs on every pointer move — so it
   *  walks the ray in steps scaled to the view, finds the first step that ends up under
   *  the surface, and bisects that step. Twenty-odd samples, accurate to a fraction of a
   *  unit, and it needs no copy of the terrain geometry.
   *
   *  `groundZ` is supplied by the page and reads the same cells the terrain mesh was
   *  built from. It has to be the same surface: a brush that painted where the ground
   *  *is not drawn* would be the same class of mistake as scattering on one surface and
   *  drawing another (§19c).
   */
  groundAt(clientX,clientY){
    if(!this.groundZ) return null;
    const {eye,dir}=this.rayAt(clientX,clientY);
    /* Round 17m: from the *view* distance, not the orbit distance. Ctrl-pulling the
       pivot in towards the eye leaves the eye where it is, so the ground under the
       pointer is exactly as far away as it was; marching from `cam.dist` would give up
       short of it and report no ground. */
    const far=this.viewDist()*4+8192;
    const step=Math.max(24, this.viewDist()*0.01);
    const at=t=>[eye[0]+dir[0]*t, eye[1]+dir[1]*t, eye[2]+dir[2]*t];
    const below=p=>{ const z=this.groundZ(p[0],p[1]); return z==null? null : p[2]<=z; };
    let t=0, prev=below(at(0));
    // Starting already under the ground happens when the camera is inside a hill.
    if(prev===true) return at(0);
    for(t=step; t<far; t+=step){
      const p=at(t);
      const b=below(p);
      if(b===null){ prev=null; continue; }   // off the loaded cells: keep going
      if(b===true && prev===false){
        let lo=t-step, hi=t;
        for(let i=0;i<24;i++){
          const mid=(lo+hi)/2;
          if(below(at(mid))===true) hi=mid; else lo=mid;
        }
        return at(hi);
      }
      prev=b;
    }
    return null;
  }

  /** Nearest object under a screen point, or null.
   *
   *  The ray is built from the same basis the view matrix uses rather than by
   *  inverting the projection: the camera is a turntable, so its axes are already
   *  to hand and there is nothing to invert.
   */
  /** Where the pointer meets the drawn objects: `{obj, p:[x,y,z]}`, or null.
   *
   *  The brush's anchor — the centre of the sphere. Same broad phase and the same
   *  `_hitParts` triangles as `pickAt`, so the point the brush lands on is on the surface
   *  the pointer picks — the two disagreeing would mean painting a thing you could not
   *  right-click. */
  surfaceAt(clientX,clientY){
    const list=this.pickables;
    if(!list||!list.length) return null;
    const {eye,dir}=this.rayAt(clientX,clientY);
    const near=[];
    for(const o of list){
      const a=o.aabb;
      let t0=0, t1=Infinity, miss=false;
      for(let k=0;k<3;k++){
        const lo=[a.x0,a.y0,a.z0][k], hi=[a.x1,a.y1,a.z1][k];
        if(Math.abs(dir[k])<1e-9){ if(eye[k]<lo||eye[k]>hi){ miss=true; break; } continue; }
        let n=(lo-eye[k])/dir[k], f=(hi-eye[k])/dir[k];
        if(n>f){ const s2=n; n=f; f=s2; }
        if(n>t0) t0=n;
        if(f<t1) t1=f;
        if(t0>t1){ miss=true; break; }
      }
      if(!miss) near.push({o,t0});
    }
    near.sort((a,b)=>a.t0-b.t0);
    /* One ray, and the nearest face it meets that is turned towards you — the brush is a
       sphere centred there, so the anchor's only job is to say where you are pointing.
       (It had a rule that skipped small objects with something behind them, for
       Addamasartus's stalactites. The sphere makes it unnecessary: anchoring on the
       stalactite still paints the floor around it, because the floor is inside the sphere
       and in plain sight, and nothing between you and the sphere is touched at all.) */
    let pick=null;
    for(const cand of near){
      if(pick && cand.t0>=pick.t) break;   // no part of this box is nearer than the hit
      const t=this._hitParts(cand.o,eye,dir,pick? pick.t : Infinity);
      if(t!=null && (!pick || t<pick.t)) pick={o:cand.o,t};
    }
    if(!pick) return null;
    const t=pick.t;
    return {obj:pick.o, p:[eye[0]+dir[0]*t, eye[1]+dir[1]*t, eye[2]+dir[2]*t],
            eye, dir, t};
  }

  pickAt(clientX,clientY){
    const {eye,dir}=this.rayAt(clientX,clientY);
    const hit=this.pickRay(eye,dir);
    return hit? hit.obj : null;
  }

  /** The nearest placed object a ray meets, as `{obj, t, p}` — or null.
   *
   *  Split out of `pickAt` in round 17m so the pointer and the view centre ask the same
   *  question of the same code. The pointer wants the object; entering orbit mode wants
   *  how far away it is, to put the pivot there.
   *
   *  Broad phase: which objects the ray could possibly reach, nearest first. The box is a
   *  filter, never an answer — see below.
   */
  pickRay(eye,d){
    const list=this.pickables;
    if(!list||!list.length) return null;
    const near=[];
    for(const o of list){
      const a=o.aabb;
      let t0=0, t1=Infinity, miss=false;
      for(let k=0;k<3;k++){
        const lo=[a.x0,a.y0,a.z0][k], hi=[a.x1,a.y1,a.z1][k];
        if(Math.abs(d[k])<1e-9){ if(eye[k]<lo||eye[k]>hi){ miss=true; break; } continue; }
        let n=(lo-eye[k])/d[k], f=(hi-eye[k])/d[k];
        if(n>f){ const s2=n; n=f; f=s2; }
        if(n>t0) t0=n;
        if(f<t1) t1=f;
        if(t0>t1){ miss=true; break; }
      }
      if(!miss) near.push({o,t0});
    }
    near.sort((a,b)=>a.t0-b.t0);

    /* Narrow phase: the drawn triangles, and nothing else.
     *
     * The box used to be the whole answer, and it was the *collision* box at that, so
     * right-clicking picked things the pointer was nowhere near — a tree's collision
     * cylinder is far wider than its trunk, and the box of a long diagonal object is
     * mostly empty air. Two objects overlapping in screen space would hand you whichever
     * box began nearer, which is not the one you were pointing at. There is no way to
     * reason about that from the outside: the thing you clicked and the thing you got
     * have no visible relationship.
     *
     * Boxes stay as the filter — testing every triangle in nine cells per click would be
     * absurd — and the answer comes from the geometry. Objects are tried nearest-box
     * first and the walk stops once no remaining box can start closer than the best
     * confirmed hit, so a click normally tests one object.
     */
    let best=null, bestT=Infinity;
    for(const cand of near){
      if(cand.t0>=bestT) break;             // nothing left can be nearer
      const t=this._hitParts(cand.o,eye,d,bestT);
      if(t!=null && t<bestT){ best=cand.o; bestT=t; }
    }
    return best? {obj:best, t:bestT,
                  p:[eye[0]+d[0]*bestT, eye[1]+d[1]*bestT, eye[2]+d[2]*bestT]} : null;
  }

  /** Where the ray meets an object's drawn triangles, or null.
   *
   *  Done in the object's own space: the ray is pushed through the inverse of the
   *  instance matrix once, and every triangle is then tested against the vertices as they
   *  are stored. The alternative — transforming every vertex into the world — would be
   *  hundreds of times the arithmetic for the same answer, and would mean holding a
   *  transformed copy of every mesh per instance.
   */
  _hitParts(o,eye,d,limit){
    const m=o.m, parts=o.parts;
    if(!m||!parts||!parts.length) return null;
    /* Whatever the viewport is culling, the pointer culls too — see the note in the loop.
       Round 17w: which is every shape's back faces, indoors and out, unless its own
       NiStencilProperty says both sides; the flag rides on the part. */
    // Inverse of the 3x3, by cofactors. Not assuming rotation-times-uniform-scale: a
    // cofactor inverse costs nothing here and cannot be wrong about a matrix that is not
    // what it was assumed to be.
    const a=m[0],b=m[1],c=m[2], e=m[4],f=m[5],g=m[6], h=m[8],i=m[9],j=m[10];
    const A=f*j-g*i, B=g*h-e*j, C=e*i-f*h;
    const det=a*A+b*B+c*C;
    if(!det || !isFinite(det)) return null;
    const id=1/det;
    const n00=A*id,        n01=(c*i-b*j)*id, n02=(b*g-c*f)*id;
    const n10=B*id,        n11=(a*j-c*h)*id, n12=(c*e-a*g)*id;
    const n20=C*id,        n21=(b*h-a*i)*id, n22=(a*f-b*e)*id;
    const ox=eye[0]-m[3], oy=eye[1]-m[7], oz=eye[2]-m[11];
    const px=n00*ox+n01*oy+n02*oz, py=n10*ox+n11*oy+n12*oz, pz=n20*ox+n21*oy+n22*oz;
    const dx=n00*d[0]+n01*d[1]+n02*d[2];
    const dy=n10*d[0]+n11*d[1]+n12*d[2];
    const dz=n20*d[0]+n21*d[1]+n22*d[2];
    /* The ray is no longer unit length in object space when the instance is scaled, so
       the `t` that comes out is in *world* units already — it multiplies a direction that
       was scaled by exactly the same factor the object was. That is what makes the
       distances comparable between a shrub at scale 1 and one at scale 3. */
    let bestT=null;
    for(const part of parts){
      let V=part.pos;
      const I=part.idx;
      /* Round 17y: an animated shape's vertices are baked under its moving node; put them
         where the node is this instant, so the pointer finds the sheet where it is drawn.
         Round 18bd (F7): **on the instance's own clock.** The draw evaluates at
         `secs*animSpeed + animShift` — the 18ai phase bins, running at 80-90% of the
         file's rate with their own shift into the clip — and this evaluated at plain
         `secs`, so the two diverged without bound: ten to twenty seconds apart after a
         hundred seconds of a session, which for a looping clip is an arbitrary pose. So
         pointing at a banner, a swinging sign or a strider's leg highlighted nothing, and
         pointing at empty air beside it picked it. 18al fixed exactly this for the gold
         highlight and gave every pickable its bin's `phase`; the pick ignored it. */
      if(part.anim && part.anim.nodes && part.anim.nodes.length){
        const ph=o.phase;
        const at=(this._uvSecs||0)*((ph && ph.speed) || 1)+((ph && ph.shift) || 0);
        const A=animMatrixAt(part.anim, at), S=part.pos;
        V=new Float32Array(S.length);
        for(let q=0;q<S.length;q+=3){
          const x=S[q], y=S[q+1], z=S[q+2];
          V[q]=A[0]*x+A[4]*y+A[8]*z+A[12]; V[q+1]=A[1]*x+A[5]*y+A[9]*z+A[13]; V[q+2]=A[2]*x+A[6]*y+A[10]*z+A[14];
        }
      }
      const oneSided = part.drawMode!==3;
      // A clockwise-wound shape (draw mode 2) shows the other side: the sign flips.
      const sgn = part.drawMode===2? -1 : 1;
      for(let k=0;k<I.length;k+=3){
        const a0=I[k]*3, b0=I[k+1]*3, c0=I[k+2]*3;
        const ax=V[a0],ay=V[a0+1],az=V[a0+2];
        const e1x=V[b0]-ax, e1y=V[b0+1]-ay, e1z=V[b0+2]-az;
        const e2x=V[c0]-ax, e2y=V[c0+1]-ay, e2z=V[c0+2]-az;
        /* Möller–Trumbore. Two-sided outdoors: Morrowind meshes are full of single-sided
           geometry seen from behind, and a back face you can see is a face you can click.
           Indoors that argument inverts — a back face is precisely the one that is *not*
           drawn — so the same rule that lets you see into a room has to let you point into
           it. Robin: "I want the raycast to test to the actual thing I as a user see. So
           if there is an interior room, and I see an object in that room, I want the
           object to be selected when I point my mouse there, even if I also point it
           through the backface of that room."

           `det = dot(e1, cross(d, e2)) = -dot(d, cross(e1, e2))`, so a positive
           determinant is a ray running against the triangle's own geometric normal — which
           by the measured winding (§64 567) is the side that gets drawn. The pick and the
           cull are then the same test, written twice because one is on the GPU. */
        const hx=dy*e2z-dz*e2y, hy=dz*e2x-dx*e2z, hz=dx*e2y-dy*e2x;
        const dt=e1x*hx+e1y*hy+e1z*hz;
        if(oneSided? dt*sgn<1e-9 : (dt>-1e-9 && dt<1e-9)) continue;
        const iv=1/dt;
        const sx=px-ax, sy=py-ay, sz=pz-az;
        const u=(sx*hx+sy*hy+sz*hz)*iv;
        if(u<0||u>1) continue;
        const qx=sy*e1z-sz*e1y, qy=sz*e1x-sx*e1z, qz=sx*e1y-sy*e1x;
        const v=(dx*qx+dy*qy+dz*qz)*iv;
        if(v<0||u+v>1) continue;
        const t=(e2x*qx+e2y*qy+e2z*qz)*iv;
        if(t<=1e-4) continue;
        if(t<(bestT==null?limit:bestT)) bestT=t;
      }
    }
    return bestT;
  }

  /** A translucent sheet at a fixed height, standing in for the water surface.
      Nothing about it is physical — it is there so that height limits, and grass
      that would end up under the sea, can be judged by eye rather than by number. */
  setWater(z,half){
    const gl=this.gl;
    if(this.water){ this._dropVao(this.water); this.water=null; }
    if(z==null){ this.dirty=true; return; }
    const h=half||12288, c=this.waterCentre||[0,0];
    const pos=new Float32Array([
      c[0]-h,c[1]-h,z,  c[0]+h,c[1]-h,z,  c[0]+h,c[1]+h,z,  c[0]-h,c[1]+h,z]);
    const idx=new Uint16Array([0,1,2, 0,2,3]);
    const vao=gl.createVertexArray(); gl.bindVertexArray(vao);
    const vb=this._buf(pos); gl.enableVertexAttribArray(0); gl.vertexAttribPointer(0,3,gl.FLOAT,false,0,0);
    const ib=gl.createBuffer(); gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER,ib);
    gl.bufferData(gl.ELEMENT_ARRAY_BUFFER,idx,gl.STATIC_DRAW);
    gl.bindVertexArray(null);
    this.water={vao,count:6,z,half:h,centre:[c[0],c[1]],bufs:[vb,ib]};
    this.dirty=true;
  }

  setMarkers(instances,color){
    const gl=this.gl;
    // Round 18bd (F16): the set this replaces, freed — `setMissing` just below has always
    // done it and every other setter in this file is symmetric.
    if(this.markers){ this._dropVao(this.markers); this.markers=null; }
    if(!instances||!instances.n){ this.dirty=true; return; }
    // small upright tetra-ish blade so object-ID and filler slots are visible
    const s=1;
    const pos=new Float32Array([ 0,0,0, -s,0,0, s,0,0, 0,0,3*s, -s,0,0, s,0,0 ]);
    const idx=new Uint16Array([0,1,3, 0,3,2, 1,2,3]);
    const vao=gl.createVertexArray(); gl.bindVertexArray(vao);
    const vb=this._buf(pos); gl.enableVertexAttribArray(0); gl.vertexAttribPointer(0,3,gl.FLOAT,false,0,0);
    const ib=gl.createBuffer(); gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER,ib);
    gl.bufferData(gl.ELEMENT_ARRAY_BUFFER,idx,gl.STATIC_DRAW);
    const pb=this._buf(instances.pos); gl.enableVertexAttribArray(3); gl.vertexAttribPointer(3,3,gl.FLOAT,false,0,0); gl.vertexAttribDivisor(3,1);
    const rb=this._buf(instances.rsa); gl.enableVertexAttribArray(4); gl.vertexAttribPointer(4,3,gl.FLOAT,false,0,0); gl.vertexAttribDivisor(4,1);
    gl.bindVertexArray(null);
    this.markers={vao,count:idx.length,n:instances.n,color:color||[1.0,0.8,0.0],bufs:[vb,ib,pb,rb]};
    this.dirty=true;
  }

  /* Positions won by a grass card whose mesh the install does not have.
     ------------------------------------------------------------------
     These used to be nothing at all: the slot won its positions, the mesh failed to
     load, and the loop moved on — so a rule with a missing mesh looked exactly like a
     rule that was placing nothing, and the only sign was a count in the statistics
     panel that you had to know to read.

     Marked rather than substituted. Drawing a stand-in shape would put grass in the
     preview that the game will not draw, which is the opposite of what a preview is
     for; a marker says "something was meant to be here" without pretending to be it.

     Not behind the Empty positions switch, either. That one is a way of looking at a
     deliberate arrangement — placeholders you meant to leave empty. This is a fault. */
  setMissing(instances){
    const gl=this.gl;
    if(this.missing){ this._dropVao(this.missing); this.missing=null; }
    if(!instances||!instances.n){ this.dirty=true; return; }
    // A tall thin pyramid: legible from any angle, and unlike the blade marker it is
    // wider at the top, so a field of them does not read as grass.
    const s=1;
    const pos=new Float32Array([ 0,0,4*s, -s,-s,0, s,-s,0, s,s,0, -s,s,0 ]);
    const idx=new Uint16Array([0,1,2, 0,2,3, 0,3,4, 0,4,1]);
    const vao=gl.createVertexArray(); gl.bindVertexArray(vao);
    const vb=this._buf(pos); gl.enableVertexAttribArray(0); gl.vertexAttribPointer(0,3,gl.FLOAT,false,0,0);
    const ib=gl.createBuffer(); gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER,ib);
    gl.bufferData(gl.ELEMENT_ARRAY_BUFFER,idx,gl.STATIC_DRAW);
    const pb=this._buf(instances.pos); gl.enableVertexAttribArray(3); gl.vertexAttribPointer(3,3,gl.FLOAT,false,0,0); gl.vertexAttribDivisor(3,1);
    const rb=this._buf(instances.rsa); gl.enableVertexAttribArray(4); gl.vertexAttribPointer(4,3,gl.FLOAT,false,0,0); gl.vertexAttribDivisor(4,1);
    gl.bindVertexArray(null);
    this.missing={vao,count:idx.length,n:instances.n,color:[1.0,0.42,0.22],bufs:[vb,ib,pb,rb]};
    this.dirty=true;
  }
  /* ---------------------------------------------------------------------------------
     Stepping to the next cell.

     Four arrows just beyond the edges of the ground that is drawn, one per compass
     direction; clicking one loads the neighbouring cell. It replaces typing coordinates
     into the cell picker to walk a coastline, which is the thing people actually do with
     a cell preview and the thing the picker was worst at.

     Two things settle the geometry:

     **The padding is fixed.** One cell or nine, the arrows sit the same distance beyond
     the last drawn ground. Scaling the gap with the view would put them a whole cell away
     from a nine-cell load — off screen at the framing that load arrives at.

     **They are solid.** A billboarded sprite would always face you, which is the usual
     reason to use one, but it also stops reading as part of the scene: what makes these
     legible is that they lie on the ground and point along it, so which one is "north"
     is answered by looking rather than by remembering which way the camera is turned.
     --------------------------------------------------------------------------------- */

  /** How far past the drawn ground the arrows sit, in game units. */
  static get ARROW_PAD(){ return 1400; }

  /** Builds the four arrows. `list` is `[{dir,pos:[x,y,z],size}]`, or null for none. */
  setArrows(list){
    const gl=this.gl;
    if(this.arrows){
      try{ gl.deleteVertexArray(this.arrows.vao); }catch(_){ }
      this.arrows=null;
    }
    if(!list || !list.length){ this.dirty=true; return; }

    /* One arrow, pointing along +x, in units of its own size: a broad head and a short
       stem, given real thickness in z so it is a solid rather than a decal. Written out
       as a prism — top face, bottom face, and a wall around the outline — because an
       extruder for one shape used once is more code than the shape.

       Chunkier than it was: the first version was a thin dart that read as a scratch on
       the ground at anything but a close orbit. */
    const OUTLINE=[[1.00,0.00],[0.20,0.78],[0.20,0.34],[-0.72,0.34],
                   [-0.72,-0.34],[0.20,-0.34],[0.20,-0.78]];
    const TH=0.26;

    const pos=[], nrm=[], idx=[];
    const centres=[], ranges=[];
    const push=(x,y,z,nx,ny,nz)=>{ pos.push(x,y,z); nrm.push(nx,ny,nz); return pos.length/3-1; };
    /* A closed outline in the XY plane, lifted into a slab. Used for the arrow and for
       every stroke of every letter — one routine, so they cannot end up looking like
       parts of two different objects. */
    const prism=(outline,z,half)=>{
      const top=[], bot=[];
      for(const [x,y] of outline){
        top.push(push(x,y,z+half, 0,0,1));
        bot.push(push(x,y,z-half, 0,0,-1));
      }
      for(let i=1;i<outline.length-1;i++){
        idx.push(top[0],top[i],top[i+1]);
        idx.push(bot[0],bot[i+1],bot[i]);
      }
      for(let i=0;i<outline.length;i++){
        const j=(i+1)%outline.length;
        const [x0,y0]=outline[i], [x1,y1]=outline[j];
        let nx=y1-y0, ny=-(x1-x0);
        const nl=Math.hypot(nx,ny)||1; nx/=nl; ny/=nl;
        const a0=push(x0,y0,z+half,nx,ny,0);
        const b0=push(x0,y0,z-half,nx,ny,0);
        const a1=push(x1,y1,z+half,nx,ny,0);
        const b1=push(x1,y1,z-half,nx,ny,0);
        idx.push(a0,b0,b1, a0,b1,a1);
      }
    };

    for(const a of list){
      const start=idx.length;
      const s=a.size||220;
      // Rotation baked in, so every arrow is the same geometry at a different angle and
      // the draw needs no per-arrow matrix.
      const ang={e:0, n:Math.PI/2, w:Math.PI, s:-Math.PI/2}[a.dir]||0;
      const ca=Math.cos(ang), sa=Math.sin(ang);
      const at=(u,v)=>[a.pos[0]+(u*ca-v*sa)*s, a.pos[1]+(u*sa+v*ca)*s];
      prism(OUTLINE.map(([u,v])=>at(u,v)), a.pos[2], TH*s);

      /* A compass letter beside each one was tried and taken out again: four labelled
         arrows on the ground is more furniture than the question needs, and which way is
         north is not what somebody stepping through cells is asking. */

      centres.push({dir:a.dir, p:[a.pos[0],a.pos[1],a.pos[2]], size:s, label:a.label||''});
      ranges.push({dir:a.dir, first:start, count:idx.length-start, size:s});
    }

    const vao=gl.createVertexArray(); gl.bindVertexArray(vao);
    this._buf(new Float32Array(pos));
    gl.enableVertexAttribArray(0); gl.vertexAttribPointer(0,3,gl.FLOAT,false,0,0);
    this._buf(new Float32Array(nrm));
    gl.enableVertexAttribArray(1); gl.vertexAttribPointer(1,3,gl.FLOAT,false,0,0);
    const ib=gl.createBuffer(); gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER,ib);
    gl.bufferData(gl.ELEMENT_ARRAY_BUFFER,new Uint16Array(idx),gl.STATIC_DRAW);
    gl.bindVertexArray(null);
    this.arrows={vao, ranges, centres};
    this.dirty=true;
  }

  /** Which arrow is under this point on screen, or null.

      Tested against the projected centre rather than against the triangles: an arrow is
      a target the size of a thumbnail with three hundred units of empty ground around
      it, so a circle is both easier to hit than the shape and easier to write than a
      second ray caster. */
  arrowAt(clientX,clientY){
    const A=this.arrows;
    // A hidden arrow is not a target. Anything else and the cursor changes over a
    // control nobody can see, and a click steps a cell for no visible reason.
    if(!A || this.opts.arrows===false) return null;
    const rect=this.cv.getBoundingClientRect();
    const W=rect.width||1, H=rect.height||1;
    const px=clientX-rect.left, py=clientY-rect.top;
    const c=this.cam;
    const ce=Math.cos(c.el), se=Math.sin(c.el);
    const eye=[c.tx+c.dist*ce*Math.cos(c.az), c.ty+c.dist*ce*Math.sin(c.az), c.tz+c.dist*se];
    const V=M4.lookAt(eye,[c.tx,c.ty,c.tz],[0,0,1]);
    // The same planes the frame was drawn with (round 17m), so this projects where the
    // arrows actually are rather than where a differently-clipped camera would put them.
    const [anP,afP]=this.nearFar();
    const P=M4.persp(FOV, W/H, anP, afP);
    const VP=M4.mul(P,V);
    const project=p=>{
      const x=VP[0]*p[0]+VP[4]*p[1]+VP[8]*p[2]+VP[12];
      const y=VP[1]*p[0]+VP[5]*p[1]+VP[9]*p[2]+VP[13];
      const w=VP[3]*p[0]+VP[7]*p[1]+VP[11]*p[2]+VP[15];
      if(!(w>0)) return null;
      return [((x/w)*0.5+0.5)*W, (1-((y/w)*0.5+0.5))*H, w];
    };
    let best=null;
    for(const a of A.centres){
      const s=project(a.p); if(!s) continue;
      // The on-screen radius of the arrow, worked out from a point one size to its side,
      // so the target grows and shrinks with the view the way the drawing does.
      const edge=project([a.p[0]+a.size, a.p[1], a.p[2]]);
      const r=edge? Math.max(14,Math.hypot(edge[0]-s[0],edge[1]-s[1])) : 20;
      const d=Math.hypot(px-s[0],py-s[1]);
      if(d<=r && (!best || s[2]<best.w)) best={dir:a.dir, label:a.label, w:s[2]};
    }
    return best;
  }

  /** Lights one arrow, by direction, or none. */
  setArrowHover(dir){
    if(this.arrowHover===(dir||null)) return;
    this.arrowHover=dir||null;
    this.dirty=true;
  }

  frame(radius){ this.frameAt(radius,0); }
  /** Same framing as frame(), but looking at a point `cz` units up. Real cells sit at
      whatever height their landscape has, so aiming at z=0 can leave the eye buried. */
  /** Same framing as frame(), aimed at a point.
   *
   *  `cx`/`cy` default to the origin, which is where everything the viewport draws sits
   *  except one thing: an interior has no grid position, so its author's coordinates are
   *  the only ones it has and a room can be a hundred thousand units from anywhere. */
  frameAt(radius,cz,cx,cy){
    this.cam.dist=radius*1.55; this.cam.tx=cx||0; this.cam.ty=cy||0; this.cam.tz=cz||0;
    this.cam.vs=1;   // a new framing: the pivot is what the camera looks at again
    this.cam.az=-0.62; this.cam.el=0.34; this.dirty=true;
  }
  topView(radius){ this.cam.az=0; this.cam.el=1.5533; this.cam.dist=radius*1.5; this.cam.vs=1; this.dirty=true; }
  _bindCam(){
    const cv=this.cv; let drag=null;
    const pos=e=>({x:e.clientX,y:e.clientY});
    /* In paint mode the right button paints instead of picking, which is Robin's
       "instead of using right mouse button to select Objects, I want the right mouse
       button to paint on the ground". The menu is suppressed either way — the browser's
       own would land on top of whichever it was. */
    /* The browser menu never opens over the viewport. The right button used to pick an
       object here; round 17h moved that to the left in both schemes (Robin: "For orbit
       viewport navigation mode - Switch what left mouse button and right mouse button
       do. I want to select objects and paint using the left mousebutton, and orbit the
       camera using the right mouse button"), so the right button now only ever moves the
       camera - orbiting it here, flying it in the WASD scheme. */
    cv.addEventListener('contextmenu',e=>{ e.preventDefault(); });
    /* Round 17: "WASD on Right mouse button" - a second way to move, chosen in the
       viewport. Hold the right button and the camera is a head: the mouse turns it
       where it stands, W/S/A/D walk and strafe along its own axes, E lifts and Q lowers
       it straight up and down the world. The move eases in and out towards a top
       speed the wheel sets (and shows on the bar at the right edge). The left button
       takes over what the right one did: a still click opens the object, and in
       paint mode it is the brush. */
    let look=null;
    this.fly={speed:1500, vel:[0,0,0], keys:{}, active:false, last:0};
    const keyOf=e=>{ const c=e.code||''; return /^Key[WASDQE]$/.test(c)? c[3] : null; };
    const typing=e=>{ const t=e.target; return !!(t && (t.tagName==='INPUT'||t.tagName==='TEXTAREA'||t.tagName==='SELECT'||t.isContentEditable)); };
    window.addEventListener('keydown',e=>{
      if(this.nav!=='wasd' || typing(e)) return;
      const k=keyOf(e); if(!k) return;
      if(look){ this.fly.keys[k]=true; e.preventDefault(); }
    });
    window.addEventListener('keyup',e=>{
      const k=keyOf(e); if(k) this.fly.keys[k]=false;
    });
    /* Round 17i: Ctrl held, in the orbit scheme, shows the pivot the view turns around
       (and is what the wheel then moves). Tracked from the modifier itself rather than
       from the key, so a window that comes back with the key already down is right. */
    const ctrl=e=>{
      const on=!!e.ctrlKey && this.nav!=='wasd';
      if(this._ctrlDown!==on){ this._ctrlDown=on; this.dirty=true; }
    };
    window.addEventListener('keydown',ctrl);
    window.addEventListener('keyup',ctrl);
    cv.addEventListener('pointermove',ctrl);
    window.addEventListener('blur',()=>{ if(this._ctrlDown){ this._ctrlDown=false; this.dirty=true; } });
    window.addEventListener('blur',()=>{ this.fly.keys={}; });
    /* Pointing at something lights it up, using the same pick the right button uses. The
       two must agree: a highlight that lands on a different object from the one the menu
       would act on is worse than no highlight, because it teaches the wrong thing about
       what the pointer is doing.

       Throttled to one pick per animation frame. `pickAt` tests real triangles now, and a
       pointer move fires far more often than the screen redraws — doing the work per
       event would be spending it on answers nobody sees. */
    let hoverPending=false, hoverAt=null, hoverKey=null;
    const hoverPick=()=>{
      hoverPending=false;
      if(!this.onHoverPick || !hoverAt) return;
      const hit=this.pickAt(hoverAt.x,hoverAt.y);
      // Only when it changes: rebuilding the highlight buffers every frame for the same
      // object would be the cost this is here to avoid.
      const key=hit? (hit.id+'@'+(hit.m? hit.m.join(',') : '')) : null;
      if(key===hoverKey) return;
      hoverKey=key;
      this.onHoverPick(hit);
    };
    /* The arrows are tested before the scene is, and on every move rather than once a
       frame: it is four circle tests, and an arrow that lights up a frame late feels
       broken in a way a static that does does not. */
    const hoverArrow=e=>{
      const a=this.arrows? this.arrowAt(e.clientX,e.clientY) : null;
      this.setArrowHover(a? a.dir : null);
      this.cv.style.cursor = a? 'pointer' : '';
      return !!a;
    };
    /* Painting.
     *
     * The stroke is collected here and handed over a frame at a time. A pointer reports
     * far more often than the screen redraws and each report is a round trip to the
     * engine, so sending one per event would be spending a call on ground the last call
     * already covered — and the engine fills the gaps between the points it is given
     * anyway, which is the whole reason it takes a list.
     */
    let stroke=null;
    const flush=()=>{
      if(!stroke || !stroke.pts.length) return;
      const pts=stroke.pts; stroke.pts=[];
      const anchors=stroke.anchors||null; stroke.anchors=null;
      if(this.onPaint) this.onPaint(pts,anchors);
    };
    const addPoint=e=>{
      /* In a room the stroke is anchored on the objects themselves — there is no ground
         to march — and each point carries its height, because that point is the centre of
         the brush's sphere. `paintFree` is the scene saying which world this is. */
      if(this.opts.paintFree){
        const h=this.surfaceAt(e.clientX,e.clientY);
        if(!h) return;
        stroke.pts.push(h.p[0],h.p[1],h.p[2]);
      }else{
        const g=this.groundAt(e.clientX,e.clientY);
        if(!g) return;
        stroke.pts.push(g[0],g[1]);
        /* And where the pointer actually is: the centre of the brush's sphere. An object
           when the cursor meets one before it meets the ground, the ground point
           otherwise — the same rule the ring is drawn by, so what you saw is what the
           stroke does. */
        const h=this.surfaceAt(e.clientX,e.clientY);
        // Nearer wins: the object hit's distance against the ground point's, both from
        // the eye along the same cursor ray.
        let a=g;
        if(h){
          const eye=this.cameraEye();
          const gd=Math.hypot(g[0]-eye[0],g[1]-eye[1],(g[2]||0)-eye[2]);
          if(h.t<gd+1.0) a=h.p;
        }
        stroke.anchors=stroke.anchors||[];
        stroke.anchors.push(a[0],a[1],a[2]);
      }
      if(!stroke.pending){
        stroke.pending=true;
        requestAnimationFrame(()=>{ if(stroke) stroke.pending=false; flush(); });
      }
    };

    cv.addEventListener('pointermove',e=>{
      /* The ring under the cursor, whether or not a button is down. It is the only thing
         that says how big the brush is before you commit a stroke with it, so it has to
         be there while you are still deciding. */
      if(this.opts.paintMode && this.onBrushMove) this.onBrushMove(e);
      if(stroke){ addPoint(e); return; }
      if(drag || look) return;               // orbiting or flying, not pointing
      if(hoverArrow(e)) return;              // an arrow swallows the pick under it
      hoverAt={x:e.clientX,y:e.clientY};
      if(hoverPending) return;
      hoverPending=true;
      requestAnimationFrame(hoverPick);
    });
    /* Round 18q, Robin: "When I hold right click to navigate in WASD mode or Rotate
       around the orbit in Orbit mode, don't show the yellow highlight on meshes that
       comes with hovering the cursor on them." The pick was already suspended while the
       camera moves (`if(drag || look) return` above) - but the *highlight from before it
       started* stayed lit, and stayed under a cursor that is no longer pointing at
       anything. So a camera drag puts it out, and the first move after the button comes
       up picks again. Pan too: the camera moving is the camera moving. */
    const dropHover=()=>{
      hoverAt=null;
      hoverKey=null;
      if(this.onHoverPick) this.onHoverPick(null);
      /* And the highlight itself, not only the page's notion of what is hovered. The
         page's handler answers "nothing" by clearing it - but only while its own switch
         is on, and the left column lights objects through the same call for its own
         reasons. Whatever put a highlight up, the camera moving takes it down. */
      this.setStaticHighlight(null);
    };
    cv.addEventListener('pointerleave',()=>{
      this.hovered=false;
      this.setBrush(null);
      this.setArrowHover(null); this.cv.style.cursor='';
      dropHover();
    });
    /* Round 18cw: whether the pointer is over the picture, for the page's Tab (11_events.js)
       - Robin: "make it so that the TAB button works to switch between navigation modes as
       long as my mouse hovers the viewport". Nothing else reads it. */
    cv.addEventListener('pointerenter',()=>{ this.hovered=true; });
    /* Round 17j, Robin: "When I am focused on the viewport, make TAB switch between WASD
       navigation mode and Orbit navigation mode." The viewport takes the focus on any
       press in it, so a click is enough to make the key reachable, and the button in the
       corner follows through `setNav`'s `onNavChange`. Round 18cw: and hovering it is
       enough too - that Tab arrives on the document, see 11_events.js. */
    cv.addEventListener('keydown',e=>{
      if(e.key!=='Tab' || e.altKey || e.ctrlKey || e.metaKey) return;
      e.preventDefault();
      this.setNav(this.nav==='wasd'? 'orbit' : 'wasd');
    });
    cv.addEventListener('pointerdown',e=>{
      const wasd=this.nav==='wasd';
      /* Round 18r: nothing else while the head is turning. Robin: "just don't take input
         from left mouse button, while we hold right mouse button". The pointer is hidden
         and being put back in the middle of the viewport, so a click has no place it
         could mean - and a stroke or a pick begun in the middle of a flight is not
         something anybody asked for. */
      if(look){ e.preventDefault(); return; }
      // The keys the viewport answers to - TAB here, W A S D while flying - need it
      // focused; a press in the picture is what focuses it.
      try{ cv.focus({preventScroll:true}); }catch(_){ }
      // Round 17h: the brush is the left button in both schemes.
      if(e.button===0 && this.opts.paintMode && this.onPaint){
        e.preventDefault();
        try{ cv.setPointerCapture(e.pointerId); }catch(_){ }
        stroke={pts:[],pending:false,id:e.pointerId};
        addPoint(e);
        return;
      }
      if(wasd && e.button===2){
        // The head turns where it stands; the keys move it while the button is down.
        e.preventDefault();
        try{ cv.setPointerCapture(e.pointerId); }catch(_){ }
        look={...pos(e), id:e.pointerId};
        dropHover();                       // round 18q: not while the head is turning
        this.fly.active=true; this.fly.last=performance.now();
        this.fly.keys={};
        this.setBrush(null);
        /* Round 17c, Robin: the keys went to whatever had the focus - the speed bar, a
           text field. The viewport takes it: whatever was focused lets go, and W A S D
           reach the camera. */
        try{ if(document.activeElement && document.activeElement!==document.body && document.activeElement!==cv) document.activeElement.blur(); }catch(_){ }
        try{ cv.focus({preventScroll:true}); }catch(_){ }
        /* Round 17b, Robin: "make the mouse cursor invisible ... when right mouse button
           is released, show the cursor again", and round 18r: "WASD movement when holding
           right mouse button must lock cursor to work. We can't have that kind of movement
           and lock the mouse into a certain range, it breaks user expectation."
         *
         * Both, without the pointer lock - which is what draws the browser's "If you want
         * to see the cursor, press ESC" bubble, in the built app as much as in a browser
         * (it names `tauri.localhost` there). That bubble is the engine's own chrome and
         * no page can suppress it, so the page does what a native game does instead:
         * **hide the pointer and put it back in the middle of the viewport whenever it
         * strays** (`_park`). Nothing is locked, so nothing is announced, and the turn
         * still never runs out of desk. Robin's own suggestion, and the right one.
         *
         * `cursor:none` rides along because it costs nothing and covers the moment before
         * the shell has answered; where there is no shell to ask - a browser, a test - it
         * is the whole of it, and the turn there has an edge again. */
        cv.classList.add('nocursor');
        look.home=[e.clientX, e.clientY];
        /* A fresh look waits for nothing. The park left over from the *last* look's
           release - it puts the pointer back where it was picked up - is long since
           landed, and a stale one would eat the first move of this one. */
        this._lookPark=null;
        /* Hidden where it stands, and **not moved**.
         *
         * Round 18r, third try. Robin: "I still see the mouse getting to the center of the
         * viewport before it's hidden", after the shell had already been told to hide
         * before moving - and "I still get a big rotational movement when I enter WASD
         * navigation by pressing right mouse button".
         *
         * One cause under both. A cursor change - the CSS `none` as much as the shell's
         * hide - is applied on the next mouse message, and if the very next mouse message
         * is a jump the page asked for, the pointer is redrawn at the far end *and then*
         * hidden: you watch it cross to the middle and wink out. The same jump is a
         * pointer move like any other, and on the way in it can be any size at all - a
         * press near the middle moves it a hundred pixels, which is a quarter turn of the
         * head and far too small for any "that was not a hand" rule to catch.
         *
         * Both go away by not moving it here. The next mouse message hides it where it
         * stands, and the first park that follows moves something nobody can see. There is
         * no jump on the way in to mistake for input, because there is no jump. */
        this._park(false, null);
        if(this.onFlyState) this.onFlyState(true);
        return;
      }
      /* Middle drags pan. The right button orbits (round 17h; it used to be the left),
         and a left press is a click in the making: it picks if it stays still and does
         nothing if it moves. In the WASD scheme the right button was taken by the look
         above, so it never reaches here. */
      try{ cv.setPointerCapture(e.pointerId); }catch(_){ }
      const mode = e.button===1? 'pan' : (e.button===2? 'orbit' : 'click');
      drag={...pos(e), mode, btn:e.button, x0:e.clientX, y0:e.clientY};
      // Round 18q: a click is still a point at something; orbiting and panning are not.
      if(mode!=='click') dropHover();
    });
    cv.addEventListener('pointermove',e=>{
      if(look){
        /* Turning on the spot: the eye stays, the pivot moves to keep it there. The
           orbit camera is kept as the one camera, so everything that reads it - the
           picker, the brush, the arrows, the profile - goes on working. */
        // The difference of two positions - the pointer is somewhere, it is only hidden.
        const p=pos(e);
        let dx=p.x-look.x, dy=p.y-look.y;
        look.x=p.x; look.y=p.y;
        /* **Nothing turns while a park is in the air.**
         *
         * The jump a park makes is the page's own doing and must never reach the camera,
         * and there is no telling *which* move carries it: the park is a round trip to the
         * shell, the hand does not stop while it is in flight, and the jump can be any
         * size - a park from near the middle of the viewport moves the pointer no further
         * than a brisk flick would. Counting moves fails (round 18r's first try), and so
         * does measuring them (its second).
         *
         * So neither. From the moment a park is asked for, moves only move the anchor -
         * they turn nothing - until the pointer is seen to have arrived where it was sent.
         * The jump then has no delta to give, whenever it lands and however big it is. The
         * cost is the round trip's worth of turning, a millisecond or two, during which
         * the hand is aiming at something it cannot see anyway.
         *
         * `near` ends it in the ordinary case; the two beside it are for the case where
         * the pointer never lands where it was sent - a shell that refused, or window
         * coordinates that are not the page's - so that a park can never wedge the turn. */
        if(this._lookPark){
          const pk=this._lookPark;
          const near=Math.hypot(p.x-pk.at[0], p.y-pk.at[1])<=80;
          const leapt=Math.abs(dx)>200 || Math.abs(dy)>200;
          if(near || leapt || performance.now()-pk.t>150) this._lookPark=null;
          return;                    // the anchor has moved; the view has not
        }
        /* And back to the middle before it reaches the edge of anything.
         *
         * Late rather than often. The pointer is hidden, so where it sits means nothing
         * until it would leave the window - and each park costs the turn a round trip.
         * Ninety per cent of the way out keeps that to a few times a second even in a fast
         * continuous turn, and still leaves a tenth of the viewport of room: at three
         * thousand pixels a second, two milliseconds of it is six pixels against sixty.
         *
         * Nothing to park onto in a browser: `_park` does nothing, nothing is skipped, and
         * the turn simply has an edge again. */
        /* One at a time: a press near the edge is already past the threshold, so the
           first real move would ask for a second park while the first is still in the
           air - two jumps arriving with one guard between them. */
        if(!this._lookPark){
          const r=cv.getBoundingClientRect();
          const mid=[r.left+r.width/2, r.top+r.height/2];
          if(Math.abs(p.x-mid[0])>r.width*0.45 ||
             Math.abs(p.y-mid[1])>r.height*0.45) this._park(null, mid);
        }
        /* Round 18ag: the sensitivity slider scales the pixel-to-radian rate here, and
           nowhere else - the orbit drag below keeps its own rate, because Robin asked
           for this on the WASD look ("when I hold right mouse button in WASD navigation
           mode and move the mouse around") and the two are different motions: one is a
           head turning, the other a scene being dragged past. The smoothing then acts
           on the scaled turn, so a slow rate is still smooth and a fast one is still
           direct at 100. */
        const k=0.0045*this.lookSensitivity();
        this.turn(-dx*k, dy*k, true);
        return;
      }
      if(!drag) return;
      const p=pos(e), dx=p.x-drag.x, dy=p.y-drag.y; drag.x=p.x; drag.y=p.y;
      const c=this.cam;
      if(drag.mode==='click') return;
      if(drag.mode==='orbit'){
        // Round 17g: through `turn`, so this scheme has its own rotation smoothing too.
        this.turn(-dx*0.006, dy*0.006, false);
      } else {
        // Pan along the CAMERA's screen axes, not the world azimuth.
        // These mirror the basis M4.lookAt builds with up = +Z:
        //   back  = ( ce·cos az,  ce·sin az, se )
        //   right = normalize(cross(up, back)) = ( -sin az, cos az, 0 )
        //   camUp = cross(back, right)         = ( -se·cos az, -se·sin az, ce )
        const ca=Math.cos(c.az), sa=Math.sin(c.az);
        const ce=Math.cos(c.el), se=Math.sin(c.el);
        const rx=-sa,      ry=ca,       rz=0;
        const ux=-se*ca,   uy=-se*sa,   uz=ce;
        // world units per pixel at the pivot distance, so the scene tracks the cursor
        const H=this.cv.clientHeight||800;
        const sc=(2*Math.tan(FOV/2)*c.dist)/H;
        // drag right -> scene moves right -> pivot moves left; drag down -> pivot moves up
        // Round 17f: into the goal, not the camera - `_orbitStep` eases the camera there.
        const g=this._orbitGoal();
        g.tx+=(-rx*dx+ux*dy)*sc;
        g.ty+=(-ry*dx+uy*dy)*sc;
        g.tz+=(-rz*dx+uz*dy)*sc;
      }
      this.dirty=true;
    });
    /* A left button that went down and came up without the view moving is a click, not
       the end of an orbit. Tested by distance rather than by whether a move fired at
       all, because a hand on a mouse always moves a pixel or two. */
    const endLook=(pointerId)=>{
      const home=look && look.home;
      look=null; this.fly.active=false;
      // The keys let go with the button; the move eases out on its own.
      this.fly.keys={};
      try{ if(pointerId!=null) cv.releasePointerCapture(pointerId); }catch(_){ }
      /* Back where it was picked up, and only then visible - Robin: "when releasing right
         mouse button it should first be reset to it's original position and only after
         that be revealed again." The shell does those two in that order; the CSS class is
         held until it says it has, so the page cannot reveal it half way home. */
      const done=this._park(true, home);
      // Nothing turns any more, so nothing needs to wait for that one to land.
      this._lookPark=null;
      if(done && done.then) done.then(()=>cv.classList.remove('nocursor'));
      else cv.classList.remove('nocursor');
      if(this.onFlyState) this.onFlyState(false);
    };
    /* The window losing focus ends the look too: alt-tabbing away with the button down
       would otherwise leave the pointer hidden and the keys flying.
       Round 18bd (F14): and it ends an orbit drag and a paint stroke, for exactly that
       reasoning. If the button is let go while the page has no focus no `pointerup`
       arrives, so coming back and moving the pointer went on orbiting with nothing held,
       or went on laying paint — and that stroke was never committed as one undo step
       either, because `onPaintEnd` is what closes it. A stroke is *flushed* rather than
       dropped: the paint the person laid is theirs, and only the pointer is gone. */
    window.addEventListener('blur',()=>{
      if(look) endLook(look.id);
      if(stroke){
        flush();
        stroke=null;
        if(this.onPaintEnd) this.onPaintEnd();
      }
      if(drag){ drag=null; }
    });
    const up=e=>{
      if(stroke){
        flush();
        stroke=null;
        try{cv.releasePointerCapture(e.pointerId);}catch(_){ }
        if(this.onPaintEnd) this.onPaintEnd();
        return;
      }
      if(look && (e.button===2 || e.type==='pointercancel')){
        endLook(e.pointerId);
        return;
      }
      if(!drag){ return; }
      const still=Math.hypot(e.clientX-drag.x0, e.clientY-drag.y0)<5;
      try{cv.releasePointerCapture(e.pointerId);}catch(_){ }
      drag=null;
      // Round 17h: a still left click is the pick, in both schemes.
      if(still && e.button===0){
        const a=this.onArrowClick? this.arrowAt(e.clientX,e.clientY) : null;
        if(a){ this.onArrowClick(a.dir,a); return; }
        if(!this.opts.paintMode && this.onPick){
          const hit=this.pickAt(e.clientX,e.clientY);
          this.onPick(hit,e);
        }
      }
    };
    cv.addEventListener('pointerup',up); cv.addEventListener('pointercancel',up);
    cv.addEventListener('wheel',e=>{
      e.preventDefault();
      if(this.nav==='wasd'){
        // The wheel sets the pace here - the top speed and, with it, how fast it is reached.
        this.setFlySpeed(this.fly.speed*(e.deltaY>0? 1/1.18 : 1.18), true);
        return;
      }
      const g=this._orbitGoal();
      const f=(e.deltaY>0? 1.11 : 0.9);
      if(e.ctrlKey){
        /* Round 17j, Robin: "The distance of the orbit seems to decide what is rendered,
           and how fog is rendered, rather than the distance of the eye now [...] CTRL
           zooming the orbit ball shouldn't change how fog is rendered etc." So the pull
           is remembered as an offset: the camera's own idea of how far it stands from the
           scene (`viewDist`) is what the near and far planes and the fog are worked out
           from, and this leaves it exactly where it was. */
        /* Round 17i, Robin: "if I hold CTRL down when I use the mouse wheel, make the
           pivot which we move the camera around move closer to the camera. It is as if
           we zoom, but instead of moving the camera, we move the pivot." So the eye
           stays exactly where it is and the pivot slides along the view axis towards it
           (or away): the picture does not change at all, only what the next orbit will
           turn around. */
        /* Round 17m: the pivot cannot be pulled inside the near plane. It could before,
           and the ball then simply stopped being drawn — half of what Robin saw as "the
           orbit pivot ball gets invisible when it should be visible". The floor leaves
           room for the ball's own radius as well, so the whole of it stays in front of
           the plane and not just its centre. */
        const floor=Math.max(40, this.nearFar()[0]*1.5+this.pivotRadius());
        const nd=clamp(g.dist*f,floor,60000), d=g.dist-nd;
        const c=this.cam, ce=Math.cos(c.el), se=Math.sin(c.el), ca=Math.cos(c.az), sa=Math.sin(c.az);
        // From the pivot towards the eye — the basis M4.lookAt builds with up = +Z.
        g.tx+=ce*ca*d; g.ty+=ce*sa*d; g.tz+=se*d;
        /* And the view scale takes exactly the ratio the distance took, so `viewDist` -
           the near and far planes, and the fog - comes out where it already was. Robin:
           "It should ONLY move the position around which the camera pivots when rotated
           in orbit mode, and nothing else should be touched." */
        if(g.dist>0) g.vs=clamp((g.vs==null? 1 : g.vs)*(nd/g.dist), 0.002, 500);
        g.dist=nd;
        this._ctrlDown=true;
        this.dirty=true;
        return;
      }
      /* Wraithguard: zoom to cursor (Settings, Orbit navigation). The pivot and the
         distance are both scaled about the point under the pointer, which leaves that
         point where it is on screen - the object, else the ground, else a point on the
         ray at the current view distance. */
      if(this.opts.zoomToCursor){
        const nd=clamp(g.dist*f,40,60000), k=g.dist>0? nd/g.dist : 1;
        const P=this._zoomAnchor(e.clientX,e.clientY);
        if(P && k!==1){
          g.tx=P[0]+(g.tx-P[0])*k; g.ty=P[1]+(g.ty-P[1])*k; g.tz=P[2]+(g.tz-P[2])*k;
          g.dist=nd;
          this.dirty=true;
          return;
        }
      }
      // A plain zoom moves the eye, so it moves `viewDist` with it: the scale stays.
      g.dist=clamp(g.dist*f,40,60000);
      this.dirty=true;
    },{passive:false});
  }
  /** What zoom-to-cursor zooms towards: the drawn object under the pointer, else the
      ground, else the point on the ray as far away as the view is. */
  _zoomAnchor(x,y){
    const s=this.surfaceAt(x,y);
    if(s && s.p) return s.p;
    const gnd=this.groundAt(x,y);
    if(gnd) return gnd;
    const {eye,dir}=this.rayAt(x,y), d=this.viewDist();
    return [eye[0]+dir[0]*d, eye[1]+dir[1]*d, eye[2]+dir[2]*d];
  }
  /** A turn of the view by `dAz`,`dEl` radians.
   *
   *  Robin's own camera smoothing, ported from his MWSE mod (round 17c): the raw turn is
   *  applied, and a *lag* takes the same amount away, so the view has not moved yet;
   *  every frame the lag decays towards zero — `lag = lerp(lag, 0, 1 - exp(-dt·k))` —
   *  and the view catches up with where the mouse already is. A low `k` lags long and
   *  smooth, 100 is direct. `keepEye` is the flying head: the eye stays put and the
   *  pivot moves round it, both for the raw turn and for the catching up (`_drainTurn`).
   *
   *  Each scheme has its own rate and its own slider — `lookSmooth` for the WASD head
   *  (round 17c), `orbitLookSmooth` for dragging the orbit round its pivot (round 17g,
   *  Robin: "Also add rotation smoothing to the orbit camera [...] slider is only
   *  visible when in the orbit camera mode"). */
  /** Round 18ag: the WASD head's turn per pixel, as a multiple of the built-in rate.
   *  Kept to a quarter and four times: outside that the view is either not turning or
   *  not steerable, and a slider that reaches either is a slider with dead ends. */
  lookSensitivity(){
    const v=+this.lookSens;
    return Math.max(0.25, Math.min(4, (v>0 && isFinite(v))? v : 1));
  }
  lookRate(){
    const wasd=(this.nav==='wasd');
    const v=wasd? this.lookSmooth : this.orbitLookSmooth;
    return Math.max(1,Math.min(100,+v||(wasd? 35 : 30)));
  }
  turn(dAz,dEl,keepEye){
    if(!(this.lookRate()<100) || (this.nav==='wasd' && !keepEye)){
      this._rotate(dAz,dEl,!!keepEye);
      return;
    }
    /* The view shown is `goal + lag`. The mouse moves the goal; the lag takes the same
       amount away, so nothing moves this instant, and `_drainTurn` lets the lag decay
       until the view has caught up. Only the lag is kept: the goal is the view minus it,
       which also means anything else that moves the camera moves the goal with it. */
    const r=this._turn||(this._turn={az:0,el:0,keepEye:!!keepEye});
    r.keepEye=!!keepEye;
    r.az-=dAz; r.el-=dEl;
    // The goal's pitch stays within the stops, so the catch-up never runs into them.
    const goalEl=this.cam.el-r.el;
    if(goalEl<-1.45) r.el=this.cam.el+1.45;
    if(goalEl>1.553) r.el=this.cam.el-1.553;
    this._turnLast=this._turnLast||performance.now();
  }
  /** Turns the camera by the angles, keeping the eye or the pivot. */
  _rotate(dAz,dEl,keepEye){
    const c=this.cam;
    if(!dAz && !dEl) return;
    const eye=keepEye? this.cameraEye() : null;
    c.az+=dAz; c.el=clamp(c.el+dEl,-1.45,1.553);
    if(eye){
      const ce=Math.cos(c.el), se=Math.sin(c.el), ca=Math.cos(c.az), sa=Math.sin(c.az);
      c.tx=eye[0]-c.dist*ce*ca; c.ty=eye[1]-c.dist*ce*sa; c.tz=eye[2]-c.dist*se;
    }
    this.dirty=true;
  }
  /** The lag decaying, a frame at a time: the view catches up with the mouse. */
  _drainTurn(now){
    const r=this._turn; if(!r || (r.az===0 && r.el===0)){ this._turnLast=0; return; }
    const dt=Math.min(0.1, Math.max(0, (now-(this._turnLast||now))/1000)); this._turnLast=now;
    const k=this.lookRate();
    let f=Math.exp(-dt*k);
    const nAz=r.az*f, nEl=r.el*f;
    // The last hundredth of a degree lands whole rather than trickling for ever.
    const done=Math.abs(nAz)<1.5e-4 && Math.abs(nEl)<1.5e-4;
    const dAz=(done? 0 : nAz)-r.az, dEl=(done? 0 : nEl)-r.el;
    r.az=done? 0 : nAz; r.el=done? 0 : nEl;
    this._rotate(dAz,dEl,r.keepEye);
  }
  /** Which way the mouse moves the view: 'orbit' (the default) or 'wasd' (round 17). */
  /** How far down the middle of the view the first solid thing is, and where — as
   *  `{p, t}`, or null when the view centre is looking at nothing but sky.
   *
   *  Round 17m. Both halves of the scene get asked, and the nearer wins: the placed
   *  objects through `pickRay` (real triangles, not boxes) and the landscape through
   *  `groundAt`'s march. It is the same pair of tests the pointer already uses, aimed at
   *  the centre of the picture rather than at the cursor. */
  centreHit(){
    const rect=this.cv.getBoundingClientRect();
    const W=rect.width||1, H=rect.height||1;
    const cx=rect.left+W/2, cy=rect.top+H/2;
    const {eye,dir}=this.rayAt(cx,cy);
    let best=null;
    const obj=this.pickRay(eye,dir);
    if(obj) best={p:obj.p, t:obj.t};
    const g=this.groundAt(cx,cy);
    if(g){
      const t=Math.hypot(g[0]-eye[0], g[1]-eye[1], g[2]-eye[2]);
      if(!best || t<best.t) best={p:g, t};
    }
    return best;
  }
  /** Puts the pivot on whatever the middle of the view is looking at, without moving the
   *  eye (round 17m).
   *
   *  Robin, on coming back from the flying camera: WASD never touches `dist`, so the
   *  orbit distance you left with is the one you return with, and after half a cell of
   *  flying that puts the pivot in mid-air wherever that number happens to land — orbiting
   *  a point that means nothing, sometimes underground, sometimes past the far side of the
   *  cell. So the view is asked what it is actually looking at, and the pivot goes there.
   *
   *  Nothing under the crosshair — sky, or a cell not loaded — leaves the distance alone,
   *  which is the old behaviour and the only honest answer. The hit is clamped to the same
   *  range the wheel works in, with the near end far enough out that the pivot ball itself
   *  still clears the near plane. */
  focusOnView(){
    const hit=this.centreHit();
    if(!hit) return false;
    const c=this.cam;
    const floor=Math.max(40, this.nearFar()[0]*1.5+this.pivotRadius());
    const t=clamp(hit.t, floor, 60000);
    const ce=Math.cos(c.el), se=Math.sin(c.el), ca=Math.cos(c.az), sa=Math.sin(c.az);
    const u=[ce*ca, ce*sa, se];   // from the pivot towards the eye
    const eye=[c.tx+c.dist*u[0], c.ty+c.dist*u[1], c.tz+c.dist*u[2]];
    c.tx=eye[0]-t*u[0]; c.ty=eye[1]-t*u[1]; c.tz=eye[2]-t*u[2];
    c.dist=t; c.vs=1;
    this._orbitG=null;   // the goal described the old pivot
    this.dirty=true;
    return true;
  }
  setNav(scheme){
    const was=this.nav;
    this.nav = scheme==='wasd'? 'wasd' : 'orbit';
    this.fly.keys={}; this.fly.vel=[0,0,0];
    // Round 17g: a turn still catching up belongs to the scheme it started in.
    this._turn=null; this._turnLast=0;
    this.unpull();   // and so does a pivot pulled in with Ctrl (round 17j)
    /* Round 17m: coming back from the flying camera, the pivot goes on whatever the view
       is pointed at rather than staying at whatever distance orbit mode was left with. */
    if(this.nav==='orbit' && was==='wasd') this.focusOnView();
    if(this.onNavChange) this.onNavChange(this.nav);
  }
  /** The flying camera's top speed, units a second, 50..50000. `announce` shows it. */
  setFlySpeed(v,announce){
    this.fly.speed=clamp(+v||1500,50,50000);
    if(this.onSpeedChange) this.onSpeedChange(this.fly.speed,!!announce);
  }
  /** Where the orbit camera is heading. Panning and the wheel move this; the camera
      itself closes on it in `_orbitStep`. Anything else that sets the camera outright -
      Recentre, framing a new cell, a test - is noticed there and adopted, so a goal
      never drags the view back to where the scene used to be. */
  /** How far the camera stands from what it is looking at, for the things that are about
      the *view* rather than about the orbit: the near and far planes, and how far the fog
      reaches (round 17j).
      Round 17m: as a *ratio* rather than an offset. `cam.vs` is how much smaller the
      orbit distance has been made by Ctrl-pulls that did not move the eye, so dividing it
      out gives back the distance the camera would stand at if the pivot had never been
      dragged in. An offset (`_pivotPull`, round 17j) had to be kept in step with the
      distance by hand at every place either changed, and the easing changed one of them a
      frame before the other — which is why Robin still saw the fog and the clipping move
      when he Ctrl-zoomed. A ratio survives the easing exactly: both numbers are eased by
      the same lerp, and a lerp divided by the same lerp is the ratio it started with. */
  viewDist(){ return Math.max(1, this.cam.dist/Math.max(1e-4, this.cam.vs||1)); }
  /** The near and far planes every pass projects with (round 17m). Fixed: see `GL_NEAR`.
   *  The water reads the depth buffer back and must linearise it with the same pair. */
  nearFar(){ return [GL_NEAR, GL_FAR]; }

  /** What the frame is cleared to, and what the flat fog fades into, on every path that
   *  draws the scene - the main frame and the offscreen targets (26_water.js) alike: a
   *  room's own fog (round 17y), else the theme's backdrop (`backCol`, round 18dd), which
   *  is the old grey unless the theme says otherwise. Under the atmosphere the sky pass
   *  clears again to the sky's fog, so this only ever shows with the atmosphere off. */
  backdrop(){
    const room=this.opts.room||null;
    return room? room.fog : backCol();
  }
  /** Round 18dt: what the frame clears to - the underwater colour with the eye under the
   *  water, the backdrop otherwise. For the offscreen frame (26_water.js), which clears
   *  after `draw()` has settled `underNow`. */
  frameClear(){
    return this.underNow? this.underNow.col : this.backdrop();
  }
  /* Round 18dt: is the eye under the water, and what is the game's fog there.
   *
   * Robin: "whenever the user is below the water surface, within the rectangle it sits on,
   * use under water effects as they do in the game." And, 18du, having seen a sea drawn
   * with walls without end under a slab: "change it back to how it worked before with the
   * water plane standing alone if it was ever beneath the terrain, and make it so that
   * under water effects are ONLY active when beneath the water plane, but above the
   * terrain." So: inside the water's footprint, below its line, and above the ground under
   * the eye where there is ground there - under the terrain is dirt, not water, and looking
   * up at a slab from beneath it is not being in the sea. Where no ground is known (beside
   * the loaded cells, inside the sea's square) the water is all there is.
   *
   * The fog is 25_sky.js's `underwaterFog` - the install's rule, in the install's numbers -
   * with the density slider's factor on the distances, as above water. Null with the
   * switch off (`opts.underwater`) or no water in the scene. */
  underwaterAt(eye, now, fogCol, fogScale){
    const o=this.opts, w=this.water;
    if(!w || o.underwater===false) return null;
    /* Round 18dy: the effects are MGE's water seen from the other side, so they stand
       down with it. Robin: "The Underwater effects should be automatically off when MGE
       water is turned off." Here rather than in the switch, so a profile that remembers
       the effects on cannot turn them on over a plain sheet. */
    if(o.mgeWater===false) return null;
    /* Round 18dy: and the water starts one near plane early. Robin: "when I move the
       camera towards the water surface, I first see the grass and such through the water
       shader, and then I see them fully visible for a short while as the camera clips
       through the water surface, and then the under water effects toggle on. I want all
       that is rendered below the water surface, when the camera clips the water surface,
       to render the under water effect." That short while is the near plane cutting the
       surface away: with the eye within `GL_NEAR` of the line the sheet is clipped over
       most of the screen, so nothing stands between the eye and the sea floor and the
       floor draws in plain air. So the fog begins while the eye is still that far over
       the line, and the shader starts the water where the ray *meets* it rather than at
       the eye (`over`, `underFog`) - what is above the line keeps the air's fog, what is
       below takes the water's. The rest of the effects (the wobble, the rays, the
       occlusion standing down) still wait for the eye itself to go under, as MGE's do:
       its post shaders test `IsUnderwater(eyePos.z)` and nothing softer. */
    const reach=this.nearFar()[0];
    if(eye[2] >= w.z + reach) return null;
    if(Math.abs(eye[0]-w.centre[0]) > w.half || Math.abs(eye[1]-w.centre[1]) > w.half) return null;
    if(typeof this.groundZ==='function'){
      let g=null; try{ g=this.groundZ(eye[0],eye[1]); }catch(_){ g=null; }
      if(g!=null && isFinite(g) && eye[2] <= g) return null;   // under the ground: dirt
    }
    const over = eye[2] >= w.z;
    const k=(fogScale==null || !(fogScale>0))? 1 : fogScale;
    const f=(typeof Sky==='object' && Sky.underwaterFog)? Sky.underwaterFog(o.hour, o.room||null, fogCol) : null;
    if(f){
      /* Wraithguard: Wonders of Water (NullCascade, MIT; Preview switch `waterDepth`) -
         the deeper the eye, the less it sees and the darker the water: depthFactor =
         saturate(depth / 1500), the fog's start and end times max(1 - depthFactor, 0.1)
         and the underwater colour times (1 - depthFactor), as its interop.lua and main.lua
         work it out (without Night Eye, which needs a player). */
      if(o.waterDepth && !over){
        const df=Math.min(1, Math.max(0, (w.z-eye[2])/1500));
        const vis=Math.max(1-df, 0.1);
        return {col:f.col.map(c=>c*(1-df)), start:f.start*k*vis, end:f.end*k*vis, source:f.source, over, depthFactor:df};
      }
      return {col:f.col, start:f.start*k, end:f.end*k, source:f.source, over};
    }
    // No sky module (a harness): vanilla's day, the ini's own colour over the frame's fog.
    const col=[12/255*0.85+fogCol[0]*0.15, 30/255*0.85+fogCol[1]*0.15, 37/255*0.85+fogCol[2]*0.15];
    return {col, start:7168*(1-2.5)*k, end:7168*k, source:'vanilla', over};
  }
  /** Undoes a Ctrl-pull without moving the eye or changing anything you can see.
   *
   *  Round 17m. Setting `vs` back to 1 on its own is not free: `viewDist` is dist/vs, so
   *  dropping the divisor drops the view distance with it, and the near and far planes
   *  and the fog all jump. Robin: "These settings are weirdly carried over in some way to
   *  the WASD navigation mode, where the far clipping plane was moved very far back when
   *  I switched from orbit mode to wasd navigation."
   *
   *  So the pivot slides back out along the view axis to where the camera has been
   *  treating it as being all along, the eye stays exactly where it is, `dist` becomes
   *  the view distance and `vs` becomes 1 — the same camera, described the plain way. */
  unpull(){
    const c=this.cam;
    if((c.vs||1)===1) return;
    const D=this.viewDist();
    const ce=Math.cos(c.el), se=Math.sin(c.el), ca=Math.cos(c.az), sa=Math.sin(c.az);
    const u=[ce*ca, ce*sa, se];
    const eye=[c.tx+c.dist*u[0], c.ty+c.dist*u[1], c.tz+c.dist*u[2]];
    c.tx=eye[0]-D*u[0]; c.ty=eye[1]-D*u[1]; c.tz=eye[2]-D*u[2];
    c.dist=D; c.vs=1;
    this._orbitG=null;   // the goal described the old pivot; it describes nothing now
    this.dirty=true;
  }
  _orbitGoal(){
    const c=this.cam;
    if(c.vs==null) c.vs=1;
    if(!this._orbitG) this._orbitG={tx:c.tx, ty:c.ty, tz:c.tz, dist:c.dist, vs:c.vs,
                                    was:{tx:c.tx, ty:c.ty, tz:c.tz, dist:c.dist, vs:c.vs}};
    return this._orbitG;
  }
  /** Round 17f, Robin: "For orbit viewport navigation mode, add positional smoothing on
      the camera in the mode too, same way as with the WASD camera, but its own value."
      The same exponential lerp the flight's speed uses - `x += (goal-x)·(1-exp(-dt·k))`,
      `k` the Camera position smoothing slider for this scheme - on the pivot and the
      zoom. The tail is snapped so it does not creep for ever. Returns true while it is
      still moving. */
  _orbitStep(now){
    if(this.nav==='wasd') return false;
    const c=this.cam, g=this._orbitGoal();
    const dt=Math.min(0.1, Math.max(0, (now-(this._orbitLast||now))/1000)); this._orbitLast=now;
    // Someone else put the camera somewhere: that is where it is, and where it is going.
    const w=g.was;
    if(c.vs==null) c.vs=1;
    if(c.tx!==w.tx || c.ty!==w.ty || c.tz!==w.tz || c.dist!==w.dist || c.vs!==w.vs){
      g.tx=c.tx; g.ty=c.ty; g.tz=c.tz; g.dist=c.dist; g.vs=c.vs;
      w.tx=c.tx; w.ty=c.ty; w.tz=c.tz; w.dist=c.dist; w.vs=c.vs;
      return false;
    }
    const k=Math.max(1,Math.min(100,+this.orbitSmooth||40));
    const f=1-Math.exp(-dt*k*0.35);
    let moved=false;
    // The pivot in units, the zoom in its own scale: a snap that suits both.
    /* Where the glide stops, in *pixels* rather than in units (round 17m). Robin: "The
       whole view also jumps a few pixels (like the camera or the world doesn't know
       exactly what position it should get) when zooming in and out."

       It did, and this is why. The tail of an exponential ease is snapped to the goal so
       it does not creep for ever, and the threshold used to be a fraction of the orbit
       distance — 1/1000 of it for the zoom, 1/2000 for the pivot. At the distances a
       person actually works from that is most of a pixel: measured, the last frame of a
       wheel click moved the picture 0.86px after the frame before it had moved 0.21px. An
       ease only ever slows down, so a frame that moves further than the one before it is
       the jump, and there it was.

       So the thresholds come from the screen instead. `SETTLE_PX` of movement is what may
       be thrown away, and each channel is asked how many world units that is:

         a pivot coordinate  moves the picture by H / (2·dist·tan(FOV/2)) pixels per unit
         the orbit distance  moves a point r pixels off centre by r·(dist ratio), and the
                             furthest anything sits is the half-diagonal

       `vs` is a ratio and moves nothing on screen at all — it is the fog and the clipping
       planes — so it takes the zoom's proportional threshold.

       And they settle *together*: one channel snapping a frame before another is a lurch
       in whatever direction that channel happened to be going, which is exactly what the
       Ctrl-pull was doing (the pivot arriving before `vs` moved the fog for a frame). */
    const H=this.cv.clientHeight||this.cv.height||800;
    const unitsPerPx=2*Math.tan(FOV/2)*Math.max(1,g.dist)/H;
    const halfDiag=Math.hypot(this.cv.clientWidth||this.cv.width||1200, H)/2;
    const tPivot=Math.max(1e-3, SETTLE_PX*unitsPerPx);
    const tDist =Math.max(1e-3, SETTLE_PX/Math.max(1,halfDiag)*Math.max(1,g.dist));
    const thresh=k2=>k2==='vs'? Math.max(1e-6, (g.vs||1)*SETTLE_PX/Math.max(1,halfDiag))
                  : k2==='dist'? tDist : tPivot;
    const KEYS=['tx','ty','tz','dist','vs'];
    /* Under threshold on every channel at once: then, and only then, the whole camera
       lands on the goal in one step that is by construction too small to see. */
    if(KEYS.every(k2=>Math.abs(g[k2]-c[k2])<thresh(k2))){
      for(const k2 of KEYS) if(c[k2]!==g[k2]){ c[k2]=g[k2]; moved=true; }
    }else{
      for(const k2 of KEYS){
        const d=g[k2]-c[k2];
        if(d===0) continue;
        c[k2]+=d*f; moved=true;
      }
    }
    if(moved){ w.tx=c.tx; w.ty=c.ty; w.tz=c.tz; w.dist=c.dist; w.vs=c.vs; }
    return moved;
  }
  /** One step of the flying camera: the keys held become a wanted velocity along the
      head's own axes (Q/E straight up and down the world), and the actual velocity
      eases towards it - in and out - at an acceleration tied to the top speed, so a
      faster camera also gets going and stops faster. Moves the pivot, which moves the
      eye with it. Returns true when the view moved. */
  _flyStep(now){
    const f=this.fly;
    const dt=Math.min(0.1, Math.max(0, (now-(f.last||now))/1000)); f.last=now;
    const c=this.cam;
    const ce=Math.cos(c.el), se=Math.sin(c.el), ca=Math.cos(c.az), sa=Math.sin(c.az);
    const fwd=[-ce*ca,-ce*sa,-se], right=[-sa,ca,0];
    let want=[0,0,0];
    const k=f.keys;
    if(k.W){ want[0]+=fwd[0]; want[1]+=fwd[1]; want[2]+=fwd[2]; }
    if(k.S){ want[0]-=fwd[0]; want[1]-=fwd[1]; want[2]-=fwd[2]; }
    if(k.D){ want[0]+=right[0]; want[1]+=right[1]; }
    if(k.A){ want[0]-=right[0]; want[1]-=right[1]; }
    // Round 17b, Robin: E up, Q down (the way most editors have it).
    if(k.E) want[2]+=1;
    if(k.Q) want[2]-=1;
    const wl=Math.hypot(want[0],want[1],want[2]);
    if(wl>0){ want=[want[0]/wl*f.speed, want[1]/wl*f.speed, want[2]/wl*f.speed]; }
    /* Ease (round 17d, Robin's "Camera position smoothing"): the velocity closes on the
       wanted one the way the look closes on the mouse - `vel = lerp(vel, want,
       1 - exp(-dt·k))`, k the slider 1..100. Low is a slow, soft start and stop; 100 is
       nearly instant. The whole vector at once, so the path never bends. */
    const km=Math.max(1,Math.min(100,+this.moveSmooth||12));
    const a=1-Math.exp(-dt*km);
    let moved=false;
    for(let i=0;i<3;i++){
      f.vel[i]+=(want[i]-f.vel[i])*a;
      // The tail lands: below a unit a second with nothing asked for, it is stopped.
      if(Math.abs(f.vel[i])<1 && want[i]===0) f.vel[i]=0;
    }
    const vl=Math.hypot(f.vel[0],f.vel[1],f.vel[2]);
    if(vl>0){
      c.tx+=f.vel[0]*dt; c.ty+=f.vel[1]*dt; c.tz+=f.vel[2]*dt;
      moved=true;
    }
    return moved;
  }
  /** The plain translucent sheet. Its own method (round 17b) because two paths draw it:
      the direct one, and the offscreen frame the sunshafts need when the MGE water is
      off — where it was missing, and "turning on simplified water made all water
      disappear" (Robin). */
  drawPlainWater(VP,airOn,fogK){
    const gl=this.gl;
    if(!this.water) return;
    this._gpuMark('water');
    const wp=this.progWater; gl.useProgram(wp.p);
    gl.uniformMatrix4fv(wp.u.uVP,false,VP);
    gl.uniform3fv(wp.u.uCol,[0.180,0.322,0.353]);
    gl.uniform3fv(wp.u.uFogCol,this.backdrop());   // 18dd (airOn sets it again for the pass)
    airOn(wp);
    gl.uniform1f(wp.u.uFogK,fogK);
    gl.uniform1f(wp.u.uAlpha,0.42);
    gl.enable(gl.BLEND); gl.blendFunc(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA);
    gl.depthMask(false);
    gl.disable(gl.CULL_FACE);          // the surface is worth seeing from below too
    gl.bindVertexArray(this.water.vao);
    gl.drawElements(gl.TRIANGLES,this.water.count,gl.UNSIGNED_SHORT,0);
    /* No skirt under this sheet. Round 16b hung the MGE water's walls under it too,
       in the sheet's own colour; Robin, a day later: "it looks better without it".
       The MGE water keeps its skirt (26_water.js). */
    gl.enable(gl.CULL_FACE);
    gl.depthMask(true); gl.disable(gl.BLEND);
    gl.bindVertexArray(null);
  }
  /** The wind that blows this frame: the slider's, 0..1. (It could be read from the
      weather's own `Wind Speed` until round 17h, when the weathers went.) */
  windNow(){
    const o=this.opts;
    return o.wind>0? Math.min(1,o.wind) : 0;
  }
  _loop(){
    // The wind moves every frame, so while it blows there is always a frame to draw.
    if(this.windNow()>0 && this.batches && this.batches.length) this.dirty=true;
    /* Round 17m: and so do the animated shapes — a waterfall's texture slides whether or
       not anything else in the picture moves. Only while there is one on screen: a cell
       with no moving shape goes on drawing a frame when something asks for it, as it
       always has. */
    if(this._anyUvAnim) this.dirty=true;
    /* Round 18bd: and the water. Its normals slide through `uTime` on their own, so it
       was never the shader that was still — it was that nothing asked for a frame.
       Robin: "In simplified preview, the water shaders doesn't move when the wind is
       still and time doesn't pass. The water should always animate in all different
       preview modes." Asked of the same two things `wantsOffscreen` asks, so a sheet
       that is not being drawn costs nothing, and the plain translucent sheet — which has
       no clock at all — does not keep the loop awake either. */
    if(this.water && (this.opts.mgeWater!==false || this.underNow)) this.dirty=true;   // 18dt: the wobble moves too
    // Round 17b: the rest of a smoothed turn.
    this._drainTurn(performance.now());
    // Round 17: the flying camera, while it is moving or asked to.
    if(this.nav==='wasd' && this.fly){
      const k=this.fly.keys, held=k.W||k.A||k.S||k.D||k.Q||k.E;
      const moving=this.fly.vel[0]||this.fly.vel[1]||this.fly.vel[2];
      if(held||moving){ if(this._flyStep(performance.now())) this.dirty=true; }
      else this.fly.last=0;
    }
    // Round 17f: and the orbit camera easing towards where the pan and the wheel put it.
    else if(this._orbitStep(performance.now())) this.dirty=true;
    /* Round 18bd (F15): everything inside the guard, and the next frame asked for in a
       `finally`. `_tellLegend` and `_tellNorth` call out to the page (`renderLegend`, the
       compass writer), and an exception from either — a removed element, a malformed
       `opts.cull`, a missing string — escaped, so the next frame was never scheduled and
       the viewport froze for good with one console line. Everything else in the program
       kept working, which is why it presented as "the 3D view stopped". */
    /* Round 18cr: the cadence, as the screen sees it. Robin: "the performance report
       doesn't show correct amount of FPS, as I can feel the scene stutter a bit". The
       report's own number is twenty frames back to back, a throughput; the screen shows
       one frame per vertical refresh, so a 25 ms frame lands every other refresh and the
       eye feels the unevenness the average hides. This keeps the time between drawn
       frames on *consecutive* ticks - an idle gap is not a slow frame - for the last
       `FRAME_RING` of them, and the report reads the median, the tail and the worst. */
    this._tick=(this._tick|0)+1;
    try{
      /* Wraithguard: nothing is drawn while the window is minimized or hidden, or while a
         dialogue covers most of the viewport (`covered`, asked of the page); the frame is
         kept owing and drawn when the view is back. And a frame-rate cap (Settings): a
         frame that comes sooner than the cap allows waits for a later tick. */
      const paused=document.hidden || (this.covered && this.covered());
      const capMs=this.opts.fpsCap>0? 1000/this.opts.fpsCap : 0;
      const tooSoon=capMs>0 && this._drawAt!=null && performance.now()-this._drawAt < capMs-1;
      if(this.dirty && !paused && !tooSoon){
        this.dirty=false;
        const now=performance.now();
        if(this._drawTick===this._tick-1){
          const ring=this._frameRing||(this._frameRing=new Float32Array(FRAME_RING));
          ring[(this._frameAt=(this._frameAt|0)%FRAME_RING)]=now-this._drawAt;
          this._frameAt++; this._frameN=Math.min(FRAME_RING,(this._frameN|0)+1);
        }
        this._drawAt=now; this._drawTick=this._tick;
        this.draw();
        this._tellLegend();
        this._tellNorth();
      }
    }catch(e){ console.error(e); }
    finally{ requestAnimationFrame(this._loop); }
  }
  /** What the last frames cost between them: `{n, median, p95, worst, over}` in ms, with
   *  `over` the share past one 60 Hz refresh. Null until twenty consecutive frames have
   *  been drawn (a still scene draws none). */
  /* ---- the GPU's time, pass by pass (round 18ct) --------------------------------------
   *
   *  Robin's release-build report: the page's submit 6.7 ms, the GPU at 100%, and the
   *  frame just inside one refresh with one in sixteen over it - the stutter he feels.
   *  The frame line's one GPU number cannot say which pass the GPU spends it in, so the
   *  report can ask for a frame timed by pass: `gpuTiming(ext)` arms it, every pass in
   *  `draw` calls `_gpuMark(name)` - which ends the running query and starts one for the
   *  next stretch - and `gpuTimes()` answers once the driver has every query's result
   *  (null until then, false when the clock was disjoint). Unarmed, a mark is one `if`.
   *  Only one TIME_ELAPSED query may be open at a time, which is why they are sequential
   *  and why a name may come up more than once (the reflection and the real pass both
   *  draw the ground): the report sums by name. */
  gpuTiming(ext){
    this._gpuMark(null);
    // Armed for the next `draw`, and that one only: the loop's frames after it must not
    // add their passes to the same list while the driver is still answering.
    this._gpuT = ext? {ext, list:[], cur:null, armed:true, active:false} : null;
  }
  _gpuMark(name){
    const T=this._gpuT; if(!T || !T.active) return;
    const gl=this.gl, ext=T.ext;
    if(T.cur){ try{ gl.endQuery(ext.TIME_ELAPSED_EXT); }catch(_){ } T.cur=null; }
    if(name==null) return;
    try{ const q=gl.createQuery(); gl.beginQuery(ext.TIME_ELAPSED_EXT,q); T.list.push({name,q}); T.cur=q; }
    catch(_){ T.cur=null; }
  }
  /** The timed frame's passes, `[{name, ms}]` summed by name in draw order - or null
   *  while the driver is still working, false when the clock jumped. Frees the queries
   *  once answered. */
  gpuTimes(){
    const T=this._gpuT; if(!T) return null;
    const gl=this.gl, ext=T.ext;
    if(T.armed) return null;   // the timed frame has not been drawn yet
    if(T.cur){ T.active=true; this._gpuMark(null); T.active=false; }
    if(!T.list.length){ this._gpuT=null; return []; }
    let disjoint=false;
    try{ disjoint=!!gl.getParameter(ext.GPU_DISJOINT_EXT); }catch(_){ }
    if(disjoint){ for(const e of T.list){ try{ gl.deleteQuery(e.q); }catch(_){ } } this._gpuT=null; return false; }
    for(const e of T.list){
      let ok=false;
      try{ ok=!!gl.getQueryParameter(e.q,gl.QUERY_RESULT_AVAILABLE); }catch(_){ ok=true; }
      if(!ok) return null;
    }
    const out=[], by=new Map();
    for(const e of T.list){
      let ns=0; try{ ns=gl.getQueryParameter(e.q,gl.QUERY_RESULT)||0; }catch(_){ }
      try{ gl.deleteQuery(e.q); }catch(_){ }
      let r=by.get(e.name);
      if(!r){ r={name:e.name, ms:0}; by.set(e.name,r); out.push(r); }
      r.ms+=ns/1e6;
    }
    this._gpuT=null;
    return out;
  }
  frameStats(){
    const n=this._frameN|0;
    if(n<20) return null;
    const a=Array.from(this._frameRing.subarray(0,n)).sort((x,y)=>x-y);
    const q=f=>a[Math.min(n-1,Math.floor(f*(n-1)))];
    let over=0; for(const v of a) if(v>17) over++;
    return {n, median:q(0.5), p95:q(0.95), worst:a[n-1], over:over/n, mean:a.reduce((s,v)=>s+v,0)/n};
  }
  /** Round 18ay: where north lies on the screen, for the compass rose.
   *
   *  The camera's basis (see the pan in `_pointer`): with the eye at
   *  `target + dist·(ce·cos az, ce·sin az, se)`, screen-right is `(−sin az, cos az)` on
   *  the ground and screen-up, on the ground, is the way the camera looks: `(−cos az,
   *  −sin az)`. North is the game's +Y, so on the screen it lies at `(cos az, −sin az)`
   *  (right, up) - which as a clockwise turn from straight up is `atan2(cos az, −sin az)`.
   *  Checks: az = 0 has the eye east of the pivot looking west, and north is to the
   *  right (90°); az = −π/2 looks north, and the needle points up (0°).
   *
   *  Handed to `onNorth` in degrees, and only when it has moved a tenth of one, so a
   *  still camera costs nothing. Unwrapped - the angle runs on past ±180 rather than
   *  jumping - so that anything easing between values (the rose did, by CSS transition,
   *  until 18az turned it inside the SVG) never whips the long way round at the seam. */
  _tellNorth(){
    if(!this.onNorth) return;
    const az=this.cam.az;
    const deg=Math.atan2(Math.cos(az),-Math.sin(az))*180/Math.PI;
    if(this._northDeg==null){ this._northDeg=deg; this._northAcc=deg; this.onNorth(deg); return; }
    let d=deg-this._northDeg;
    if(d>180) d-=360; else if(d<-180) d+=360;
    if(Math.abs(d)<0.1) return;
    this._northDeg=deg;
    this._northAcc+=d;
    this.onNorth(this._northAcc);
  }

  /* ---- the legend -------------------------------------------------------------------
   *
   *  What every colour on the ground means, and only the ones that are on it.
   *
   *  The renderer answers this rather than the panel, for the reason the whole tool is
   *  built on: it is the thing that decides what is drawn, and a legend assembled from
   *  the page's idea of the current state is a second implementation of that decision —
   *  which is how you get a key that names a colour nobody can see, or misses one that is
   *  right there. Every entry below reads the same field the draw call reads, and the
   *  colours are the literal constants the shader is given.
   *
   *  Robin: "adding a suscinct legend with what colors mean what on the landscape [...]
   *  Only show the colors that are currently active on the landscape."
   */
  legendItems(){
    const o=this.opts, out=[];
    const hex=c=>'#'+c.map(v=>Math.round(v*255).toString(16).padStart(2,'0')).join('');
    const deg=v=>(Math.round(v*10)/10)+'°';
    const num=v=>String(Math.round(v));

    /* The cull overlay is one colour for three limits, so it is one entry that says which
       of them is doing anything. Naming all three when only the slope is set would send
       somebody looking for a height limit they never set. */
    /* Round 18q: one colour, every rule that culls - so the note has to say which of them
       is doing anything, or the shade is a mystery. The editor asks the two that mean
       something on a mesh (see `_cullUniforms`); a cell asks them all. */
    {
      /* The same reading the shade is drawn from, so the key and the picture cannot
         disagree about what is being refused. */
      const NO=1e8;
      const c=this.cullAsk();
      const why=[];
      if(c){
        if(c.slope[0]>0)  why.push(T('legend.shallower',{deg:deg(c.slope[0])}));
        if(c.slope[1]<90) why.push(T('legend.steeper',{deg:deg(c.slope[1])}));
        const arc=f=>f? T('legend.facing_away',{deg:num(f[0])+'\u00b0'}) : null;
        if(arc(c.face))  why.push(arc(c.face));
        if(arc(c.face2)) why.push(arc(c.face2)+' '+T('legend.this_card'));
        if(!o.inspecting){
          if(c.h[0]>-NO) why.push(T('legend.below',{n:num(c.h[0])}));
          if(c.h[1]<NO)  why.push(T('legend.above',{n:num(c.h[1])}));
          if(c.shore[0]>-NO || c.shore[1]<NO) why.push(T('legend.wrong_shore'));
          if(c.curve[0]>-NO || c.curve[1]<NO) why.push(T('legend.wrong_curve'));
          if(c.obs[0]>-NO || c.obs[1]<NO) why.push(T('legend.wrong_object'));
          if(c.canopy) why.push(T('legend.canopy'));
        }
      }
      const on = o.inspecting? o.showSlopeMesh : o.showSlope;
      if(on && why.length)
        out.push({col:hex([0.812,0.416,0.298]), label:T('legend.no_grass'), note:why.join(' \u00b7 ')});
    }
    if(o.paintShow){
      /* Round 18bd (F11): the same question the draw asks — "is this colour on a chunk
         that is being drawn?" — and not "does the cache hold a mask for it anywhere?".
         The draw loop below walks `cellChunks` and looks the mask up by that chunk's
         grid; this asked the whole map, and `paint_cells` deliberately re-asks about
         every cell the renderer still holds a mask for, so the keys never age out. Paint
         a cell, step two cells away, enter paint mode, and the key listed "Painted: no
         grass here" and one entry per rule swatch with none of it in sight — precisely
         the failure this method's own doc says it exists to prevent. */
      const drawn=new Set();
      for(const ch of this.cellChunks||[]) if(ch.grid) drawn.add(ch.grid[0]+','+ch.grid[1]);
      const onGround=(map,suffix)=>{
        for(const g of drawn) if(map.has(g+suffix)) return true;
        return false;
      };
      if(onGround(this.paintTex,'')) out.push({col:hex([0.760,0.352,0.180]), label:T('legend.painted_bare')});
      if(onGround(this.growTex,''))  out.push({col:hex(GROW_TINT), label:T('legend.painted_grass')});
      /* One entry per swatch that has ground under it in this scene — a colour you made
         and have not used yet is in the Paint tools, where you made it, and not here. */
      for(const w of this.rulePalette||[]){
        // A hidden layer's colour is not on the ground, so it is not in the key to it.
        if(this.hiddenLayers && this.hiddenLayers.has(w.layer)) continue;
        if(onGround(this.ruleTex,'|'+w.slot)){
          /* The rule's own name, not its tag — `ruleName` is the page's, because turning
             a tag into the name you gave a rule needs the config and the renderer does
             not hold one. A swatch pointing at a rule the loaded set no longer has falls
             back to the tag, which is at least the name it was made from. */
          const nm=(this.ruleName && this.ruleName(w.rule)) || w.rule || T('legend.a_rule');
          out.push({col:w.colour||'#888', label:T('legend.painted_rule',{rule:nm})});
        }
      }
    }
    if(o.paintMode) out.push({col:hex([1.0,0.627,0.322]), label:T('legend.brush'), note:T('legend.brush_note')});
    if(this.hlTex) out.push({col:hex([0.55,0.44,0.10]), label:T('legend.hover_texture'),
                             note:T('legend.hover_texture_note')});
    if(this.hlSlot!=null) out.push({col:hex([1.0,0.82,0.30]), label:T('legend.hover_slot'),
                                    note:T('legend.hover_slot_note')});
    if(this.missing) out.push({col:hex([1.0,0.42,0.22]), label:T('legend.mesh_missing'),
                               note:T('legend.mesh_missing_note')});
    if(this.markers && o.markers) out.push({col:hex(this.markers.color),
                                            label:T('legend.reserved'),
                                            note:T('legend.reserved_note')});
    return out;
  }

  /** Hands the legend to whoever is drawing it, and only when it has changed. */
  _tellLegend(){
    if(!this.onLegend) return;
    const items=this.legendItems();
    const sig=items.map(i=>i.col+'\u0000'+i.label+'\u0000'+(i.note||'')).join('\u0001');
    if(sig===this._legendSig) return;
    this._legendSig=sig;
    this.onLegend(items);
  }
  resize(){
    const cv=this.cv, dpr=Math.min(devicePixelRatio||1,2);
    const w=Math.max(1,Math.round(cv.clientWidth*dpr)), h=Math.max(1,Math.round(cv.clientHeight*dpr));
    if(cv.width!==w||cv.height!==h){ cv.width=w; cv.height=h; this.dirty=true; return true; }
    return false;
  }
  draw(){
    const gl=this.gl, o=this.opts, c=this.cam;
    if(this._gpuT && this._gpuT.armed){ this._gpuT.armed=false; this._gpuT.active=true; }   // 18ct: this frame, timed by pass
    const grew=this.resize();
    if(grew) this.dirty=false;   // consume the flag resize() just set; we are drawing now
    /* The ground map the grass tints from, baked only when it is going to be read and
       only when what it shows has changed. */
    if(o.tint>0 && ((this.cellChunks && this.cellChunks.length) || this.ground ||
                    (this.statics && this.statics.length)) &&
       (this.groundMapStale || this._gmBlend!==o.blend || this._gmVcol!==o.vcol))
      this.bakeGroundMap(o);
    gl.viewport(0,0,this.cv.width,this.cv.height);
    /* Round 17y: a room lit by its own record. `opts.room` is set by the cell preview for
       an interior that is not flagged to behave like an exterior: its AMBI's ambient and
       "sunlight" colours, and its fog colour, which is what the game shows past the walls
       and so is the backdrop here. No atmosphere, no clock on the light, no flat fog. */
    const room=o.room||null;
    /* Round 18dd: what is past everything - the room's fog, or the theme's backdrop
       (`backdrop()`); the atmosphere, when it is on, clears again to its own fog below. */
    const back=this.backdrop();
    gl.clearColor(back[0],back[1],back[2],1);
    gl.enable(gl.DEPTH_TEST); gl.depthFunc(gl.LEQUAL);
    gl.clear(gl.COLOR_BUFFER_BIT|gl.DEPTH_BUFFER_BIT);
    gl.disable(gl.CULL_FACE);
    gl.disable(gl.BLEND);

    const ce=Math.cos(c.el), se=Math.sin(c.el);
    const eye=[c.tx+c.dist*ce*Math.cos(c.az), c.ty+c.dist*ce*Math.sin(c.az), c.tz+c.dist*se];
    const V=M4.lookAt(eye,[c.tx,c.ty,c.tz],[0,0,1]);
    const [nearP,farP]=this.nearFar();
    const P=M4.persp(FOV, this.cv.width/this.cv.height, nearP, farP);
    const VP=M4.mul(P,V);
    /* The sun and the sky for this hour, from the game's ramp (25_sky.js). The sun
       height slider is gone: the clock places the sun, atmosphere on or off. */
    const scat=o.scatter!==false && !room;
    /* Round 17m: the clock the animated shapes and the lamps read. One number a frame,
       so every waterfall in the picture is at the same instant and the phases the files
       carry are the only thing that separates them. */
    this._uvSecs=(typeof performance==='object'? performance.now() : Date.now())/1000;
    this._lightUniforms();
    /* What this frame actually submitted, counted across every pass that culls — the
       grass and the objects both, since round 17u. Reset here rather than in one of the
       passes: doing it in the statics pass wiped the grass's count, because the grass is
       drawn first. */
    this._drawn=0; this._drawnInst=0; this._multiCalls=0; this._drawnBaked=0;
    /* The three light textures, bound once for the whole frame on units 7, 8 and 9. Every
       surface shader reads the same ones, and nothing else in the frame uses those units,
       so this is the only place they are touched (round 17r). */
    this._bindLights(this._lgrid);
    let now=(typeof Sky==='object')? Sky.at(o.hour) : null;
    /* A room's "sun" is the AMBI sunlight colour from a fixed direction above - OpenMW's
       configureAmbient puts it at (-0.15, 0.15, 1) (round 17y). */
    const sun = room? [-0.15,0.15,1] : now? (now.sunLight||now.sunDir).slice() : [0.42*Math.cos(0.8),0.30,Math.sin(0.8)];
    if(!now || room){ const sl=Math.hypot(sun[0],sun[1],sun[2]); sun[0]/=sl; sun[1]/=sl; sun[2]/=sl; }
    this._sunNow=sun;   // 18du: for the caustics pass (26_water.js)
    const fogK=(o.fog && !room)? 0.000085 : 0.0;
    const fr=(scat && now && typeof Sky==='object')? Sky.fogRange() : null;
    /* Round 15 item 3: the fog density slider, 1 = MGE's own distances for the weather.
       Denser fog is nearer distances - both the start and the fall-off divided by it.

       Round 17m: and *nothing else* scales them. There used to be a
       `max(1, viewDist/12000)` in here that stretched the fog out as you zoomed away, so
       that an overview from five cells up was not a sheet of fog. It meant the same cell
       under the same settings was hazy close up and clear far off - Robin: "Fog etc. are
       dependent on zoom level, even when the camera doesn't move much at all [...] the
       only thing that should affect it is the camera's distance to the rendered object."
       Which is what fog is: `airFog` already measures eye to fragment per pixel, and the
       weather's start and fall-off are numbers about the weather, not about where anyone
       is standing. The slider is the control for a preview that wants to see further. */
    const fogScale=1/Math.max(0.05, o.fogDensity==null? 1 : o.fogDensity);
    /* MGE's three sun-altitude terms, worked out here rather than per fragment. */
    const sp=now? now.sunPos : sun;
    const sunalt=Math.pow(1+sp[2],10);
    const sunAlt=[2.8+4.3/sunalt, Math.max(0,Math.min(1,1-Math.pow(2,-1.9*sunalt))),
                  Math.max(0,Math.min(1,Math.exp(-4*sp[2])))*Math.max(0,Math.min(1,sunalt))];
    /* Round 18i: MGE's per-weather sun and ambient multipliers ([lighting.weather.clear]
       in mgeXE.toml, [Per Pixel Lighting] in MGE.ini) scale what its per-pixel lighting
       gives the sun and the scene ambient (buildPplDrawData) - the objects, not the sky. */
    const lm=this.lampModel();
    const mul=(c,k)=> k===1? c : [c[0]*k, c[1]*k, c[2]*k];
    const fogCol = room? room.fog : (scat&&now)? now.fog : back;   // 18dd: the backdrop's, off the sky
    this.skyNow=now;
    /* Round 18dt: the eye under the water. `underwaterAt` says whether it is (the water's
       footprint, below its line, above the ground) and what the game's fog is there for
       this install and hour (25_sky.js); the density slider scales those distances as it
       scales the fog above (Robin: "a multiplier to whatever the engine gives us [...]
       keeping the ratio as it was"). Off the switch, or in a scene with no water, null:
       the frame draws as it always did. The main pass's own; the reflection pass looks
       from the eye's mirror image, in the air, and airOn leaves it off there. */
    /* Round 18dy: two answers, one call. `underFogNow` covers the crossing - the eye
       within a near plane of the line, where the sheet is clipped away and the floor
       would otherwise draw in plain air - and drives the fog alone, per fragment, from
       where each ray meets the water. `underNow` is the eye itself being under, which is
       what the wobble, the rays, the standing-down of the occlusion and the sunshafts,
       the clear colour and the light's blend all wait for, as MGE's own do. */
    this.dofDrawn=false;                     // 18dy: per frame, whether the offscreen path runs or not
    this.shadowDrawn=false;                  // 18ee: whether the sun's cascades were drawn
    this.underMixNow=[0,0,0,0];              // 18ea: what the crossing's blend was set to, per frame
    const underFog=this.underwaterAt(eye, now, fogCol, fogScale);
    const under=(underFog && !underFog.over)? underFog : null;
    this.underNow=under; this.underFogNow=underFog;
    /* Round 18dx: and the light goes with it. The game blends *every* weather colour
       toward the water's while the eye is under it - `UnderwaterColor x weight + colour x
       (1 - weight)` - not the fog colour alone. An MWSE probe read Robin's own engine
       above and below the surface at Vos and the two colours it could reach, the current
       fog and the current sky, both move by exactly that rule to four decimal places
       (25_sky.js `underwaterTint`). His build exposes no ambient or sun to read, but the
       same rule is what closes the gap in his screenshot: the sea floor was five times
       too bright with the light left alone, and lands on the bright end of his measured
       range with it blended. Robin had it before the measurement did - "Maybe the
       lighting change when going into water?"

       It is laid over the weather's colours only, so the atmosphere switch off (a flat
       white light, no weather at all) is left as it is - there is nothing there to blend
       - and a room takes it on the room's own ambient and sunlight. */
    const lit = !!(room || (scat&&now));
    const tint = c => (under && lit)? Sky.underwaterTint(c) : c;
    const sunCol = tint(room? room.sunlight : (scat&&now)? mul(now.sun, lm.sunMult) : [1,1,1]);
    const ambCol = tint(room? room.ambient : (scat&&now)? mul(now.ambient, lm.ambMult) : [1,1,1]);
    /* The sky's own colour takes the sky blend, not the fog's: the probe reads them
       separately and under the water they differ (0.1117,0.1776,0.2161 against
       0.1639,0.2241,0.2587 in his frame). `uFogColFar` stays the fog blend, which is
       what the game clears the screen to and therefore what MGE captures as horizonCol. */
    const underSky = (under && lit)? Sky.underwaterTint(room? room.fog : now.sky) : null;
    /* What the frame settled on, for the tests and the probes - the same three colours
       the uniforms below get. `skyNow` is the weather's, `lightNow` is what reached the
       shaders after the multipliers and the water. */
    this.lightNow={sun:sunCol, amb:ambCol, sky:underSky};
    const clearCol=under? under.col : fogCol;
    if(under && !(scat&&now)){ gl.clearColor(under.col[0],under.col[1],under.col[2],1); gl.clear(gl.COLOR_BUFFER_BIT); }
    /* The same uniforms on every program that lights or fogs a surface. `curEye` and
       `curClip` are the pass's: the reflection pass (round 15) looks from under the
       water and drops everything beneath it. */
    let curEye=eye, curClip=null, curClipMode=1, curReflect=false;
    /* Round 18dl: the pass's camera frame - right, up, forward in world space - for the
       environment maps' sphere mapping, from the eye to the point the pass looks at (the
       reflection pass looks from under the water at the pivot's mirror image). The same
       basis the sky builds below. */
    const camBasis=(pass)=>{
      const tgt=(pass&&pass.target)||[c.tx,c.ty,c.tz];
      const f=[tgt[0]-curEye[0],tgt[1]-curEye[1],tgt[2]-curEye[2]];
      const fl=Math.hypot(f[0],f[1],f[2])||1; f[0]/=fl; f[1]/=fl; f[2]/=fl;
      const r=[f[1], -f[0], 0];   // f × up(0,0,1)
      const rl=Math.hypot(r[0],r[1],r[2])||1; r[0]/=rl; r[1]/=rl; r[2]/=rl;
      const u=[r[1]*f[2]-r[2]*f[1], r[2]*f[0]-r[0]*f[2], r[0]*f[1]-r[1]*f[0]];   // r × f
      return {f,r,u};
    };
    let cam=camBasis(null);
    const airOn=(pr)=>{
      const u=pr.u;
      if(u.uCamR){ if(u.uCamF) gl.uniform3fv(u.uCamF,cam.f); gl.uniform3fv(u.uCamR,cam.r); gl.uniform3fv(u.uCamU,cam.u); }
      if(u.uScat) gl.uniform1i(u.uScat, (scat&&now)?1:0);
      if(u.uRoom) gl.uniform1i(u.uRoom, room?1:0);
      if(u.uNoLight) gl.uniform1i(u.uNoLight, o.unlit?1:0);   // round 18f
      if(u.uFogBlack) gl.uniform1i(u.uFogBlack, 0);             // round 18h: set per additive batch
      if(u.uEye) gl.uniform3fv(u.uEye,curEye);
      if(u.uClip) gl.uniform1i(u.uClip, curClip==null? 0 : curClipMode);
      if(u.uClipZ) gl.uniform1f(u.uClipZ, curClip==null? -1e9 : curClip);
      if(u.uSunCol) gl.uniform3fv(u.uSunCol,sunCol);
      if(u.uAmbCol) gl.uniform3fv(u.uAmbCol,ambCol);
      if(u.uFogCol) gl.uniform3fv(u.uFogCol,fogCol);
      if(u.uDimCol) gl.uniform3fv(u.uDimCol,this.fogCol);   // the dim is the viewport's own, always (18dd: not the theme's backdrop)
      /* Round 17j: the pivot's stamp on the ground and on the rocks — every surface is
         told where the ball is, and each draws the ring where it cuts it. Off (w<=0)
         unless Ctrl is held in the orbit scheme, which is when the ball itself is up. */
      if(u.uPivot){
        const pc=this.cam;
        gl.uniform4fv(u.uPivot, (this._ctrlDown && this.nav!=='wasd')? [pc.tx,pc.ty,pc.tz,this.pivotRadius()] : [0,0,0,-1]);
      }
      if(u.uPivotCol) gl.uniform3fv(u.uPivotCol, orbitCol());
      /* Round 17m: the cell's lamps — and since round 17r, three textures rather than an
         array. `_lightGrid` built them once this frame; every surface shader reads the
         same three, so they are bound once (below, before the passes) and only the
         sampler slots and the grid's shape are set here. */
      if(u.uLightN){
        const g=this._lgrid||null;
        gl.uniform1i(u.uLightN, (g && g.n)? g.n : 0);
        gl.uniform4fv(u.uBinGrid, (g && g.n)? g.grid : [0,0,1,1]);
        // The samplers themselves are pinned in `_prog`, once per program.
      }
      // Round 18i: the install's lamp model (see LIGHT_GLSL and `lampModel`).
      if(u.uLampModel){ const lm=this.lampModel(); gl.uniform1i(u.uLampModel, lm.model); if(u.uLampBounds) gl.uniform1f(u.uLampBounds, lm.bounds); }
      /* Round 18dt: the underwater fog. 18du: on every pass - MGE fogs its mirror with
         the same state - with the footprint's box on the main pass alone (the mirror's
         eye is the image, in the air; MGE fogs it by plain distance). */
      /* Round 18eb: **every one of these is written every frame**, whether the eye is in
         the water or not. They were set inside the `on` branch, so leaving the water set
         `uUnder` back to 0 and left `uUnderTop` and `uUnderMix` at whatever the last wet
         frame put there - and `underLight` asks only `uUnderMix.w`, so from then on the
         program went on blending everything below that stale line inside that stale box,
         for the rest of the session. Robin: "The line under which everything is darker
         appears still after going down into water once, and then up again. I guess it is
         not updating properly after leaving water." It was not updating at all. A uniform
         that is read unconditionally has to be written unconditionally. */
      if(u.uUnder){
        const on=!!underFog, w=this.water;
        gl.uniform1i(u.uUnder, on?1:0);
        if(on){
          gl.uniform3fv(u.uUnderCol, underFog.col);
          gl.uniform2f(u.uUnderRange, underFog.start, underFog.end);
        }
        if(u.uUnderBox) gl.uniform4f(u.uUnderBox, w? w.centre[0] : 0, w? w.centre[1] : 0, w? w.half : 0, (on && !curReflect && w)? 1 : 0);
        /* Round 18dy: where the water begins along the ray. Zero with the eye under it;
           with the eye over the line and still inside the crossing, the height of the
           eye over the water, which the shader turns into a distance per fragment. */
        if(u.uUnderTop) gl.uniform1f(u.uUnderTop, (on && underFog.over)? Math.max(0, eye[2]-(w? w.z : 0)) : 0);
        /* 18dz: and the light's blend for the crossing. Off once the eye is under, where
           the whole frame's sun and ambient carry it already (18dx), and off in the air. */
        if(u.uUnderMix){
          const cross=!!(on && underFog.over && lit && typeof Sky==='object' && Sky.underwaterMix);
          const m=cross? Sky.underwaterMix() : null;
          if(m) gl.uniform4f(u.uUnderMix, m.col[0], m.col[1], m.col[2], m.weight);
          else gl.uniform4f(u.uUnderMix, 0,0,0, 0);
          this.underMixNow = m? [m.col[0],m.col[1],m.col[2],m.weight] : [0,0,0,0];   // for the tests
        }
      }
      /* Round 18ee: the sun's shadow. Written every frame whether or not the map was
         drawn - 18eb's lesson, and the same shape of bug: `gdnShade` reads `uShadowVP`
         and the atlas unconditionally, so a frame that skipped the pass must say so
         rather than leave the last frame's boxes standing. */
      if(u.uShadow){
        const sm=this.shadowDrawn? this._shadowNow : null;
        gl.uniform1i(u.uShadow, sm? 1 : 0);
        if(sm){
          if(u.uShadowVP) gl.uniformMatrix4fv(u.uShadowVP, false, sm.vp);
          if(u.uShadowP) gl.uniform2f(u.uShadowP, sm.rcp, sm.sunK);
          if(u.uShadowFog) gl.uniform2f(u.uShadowFog, sm.fog[0], sm.fog[1]);
          if(u.uShadowTex) gl.uniform1i(u.uShadowTex, SHADOW_UNIT);
        }
      }
      /* Round 18dv: and the two "far" colours are the water's. In MGE `fogColFar` is
         `horizonCol` - the colour Morrowind *clears the screen to*, captured in the d3d8
         wrapper (`Clear` -> `setHorizonColour`), not the weather's fog colour - and under
         the water the game clears to its underwater colour. `skyCol` likewise: the sky is
         at infinite distance, so it is wholly fogged, and in the game you never see it
         anyway because the water plane covers the whole hemisphere (this tool's plane is
         a square and would otherwise leak a bright sky past its edge). With both set to
         the water's colour, `fogColourSky` collapses to that colour on its own, and the
         water's own depth colour - made of skyCol and fogColFar - goes with it. Robin,
         comparing his game with the tool: "the in game image is darker and more tinted".
         Measured on his own screenshots: the game converges to about (27,38,43) where the
         tool sat at (67,86,92). */
      if(under){
        if(u.uSkyCol) gl.uniform3fv(u.uSkyCol, underSky || under.col);
        if(u.uFogColFar) gl.uniform3fv(u.uFogColFar, under.col);
      }
      if(!(scat&&now)) return;
      if(!under){
        if(u.uSkyCol) gl.uniform3fv(u.uSkyCol,now.sky);
        if(u.uFogColFar) gl.uniform3fv(u.uFogColFar,now.fog);
      }
      if(u.uSunPos) gl.uniform3fv(u.uSunPos,now.sunPos);
      if(u.uSunAlt) gl.uniform3fv(u.uSunAlt,sunAlt);
      if(u.uInscatter) gl.uniform3fv(u.uInscatter,now.scatter.inscatter);
      if(u.uOutscatter) gl.uniform3fv(u.uOutscatter,now.scatter.outscatter);
      if(u.uSkyScatter) gl.uniform4fv(u.uSkyScatter,[...now.scatter.skylight, now.scatter.skylightMix]);
      if(u.uNice) gl.uniform1f(u.uNice, fr&&fr.scattering? now.nice : 0);
      if(u.uFogStart) gl.uniform1f(u.uFogStart, o.fog? fr.start*fogScale : 1e9);
      if(u.uFogRange) gl.uniform1f(u.uFogRange, fr.divisor*fogScale);
    };
    /* The scene - sky, slab, land, grass, objects - as one callable, because the water
       (26_water.js) draws it twice: once from under the water for the reflection, with
       no grass and nothing beneath the surface, and once for real. `VP` and `eye` are
       the pass's own; `pass.reflect` is the cheap one. */
    const drawScene=(VP,eye,pass)=>{
    const reflect=!!(pass&&pass.reflect);
    /* Round 17x: the scene in two phases when the water asks for it. `opaque` is the sky,
       the ground, the grass and every object that writes depth; `translucent` is the
       blended objects — the waterfalls — and the particles, which write none. The water
       goes between them: drawn after the translucent things it was drawn *over* them,
       since nothing of theirs was in the depth buffer to stop it, and a waterfall seen
       from the side vanished under the reflection of the sky. Robin: "the sorting with the
       water is off when looking at them from the side [...] the waterfall mesh is above
       the water surface." `all`, the default, is both at once, for the reflection. */
    const phase=(pass&&pass.phase)||'all';
    /* Round 18d: two more phases, for the ambient occlusion. `solid` is the opaque phase
       without the grass and `grass` is the grass alone, so the occlusion can be worked
       out from a depth buffer the blades are not in (MGE's depth pass has none either, and
       with them in it every blade shaded the soil around it into a dark lawn) and applied
       after they are drawn, over them - which is where MGE's combine lands too. */
    /* Round 18f: and `particlesBelow` - the particles under the water line alone, drawn
       before the surface so it tints or refracts them (28_particles.js). Nothing else. */
    const belowP=phase==='particlesBelow';
    const opaqueP=phase!=='translucent' && phase!=='grass' && !belowP;
    const grassP=(phase==='all'||phase==='opaque'||phase==='grass') && !reflect;
    /* Round 18dz: the below phase draws the see-through things too - but only when the
       pass carries the line to clip them at. The plain path (no offscreen targets) asks
       for the same two phases to put particles under its sheet and names no line, and
       with both of them drawing the see-through things unclipped every sheet was blended
       twice: t_foliage's three stacked quads read 162,31,18 where they read 162,45,26. */
    const clearP=phase==='all' || phase==='translucent' || (belowP && pass.waterSplit!=null);
    // A frame's passes, for the tests: an array put here is filled, and nothing is done otherwise.
    if(this._passLog) this._passLog.push(phase+':'+(pass.waterSplit==null? '-' : pass.waterSplit)+':'+this._drawn);
    /* Round 18ea: the half of the see-through pass that goes in before the surface writes
       no depth, so the water measures the solid thing behind it, as MGE's depth frame does. */
    this._clearNoZ = belowP;
    /* Round 18dz: and the water line splits the see-through pass in two. Robin:
       "Transparent objects like the telvanni crystals, and particle effects like fire,
       and waterfalls are also much more visible than other objects through the water
       surface. Like the underwater effect is not added fully on top on them the same way
       as other objects." They were: the whole see-through pass is drawn *after* the
       surface (step 5 in 26_water.js, so a waterfall standing over the water is not under
       its own reflection), which for anything below the line means drawn over the surface
       at full strength, out of the picture the surface refracts. The particles already
       had a side; this gives the geometry one, by the clip the mirror pass already uses -
       `waterSplit` with the below phase keeps what is under the line, and the late pass
       keeps what is over it, so a crystal half in the water is cut exactly at it. */
    curEye=eye; curReflect=!!reflect;
    curClip = reflect? pass.clipZ : (pass.waterSplit!=null? pass.waterSplit : null);
    curClipMode = reflect? ((pass.clipMode)? pass.clipMode : 1) : (belowP? 2 : 1);
    cam=camBasis(pass);   // 18dl: the pass's frame, for the environment maps
    /* Round 18cs: the pass's frustum, once for every cull below - the reflection's
       narrowed to the water's screen rectangle when the water pass names one
       (`_waterRect`): nothing outside it can be sampled off the reflection. */
    const planes=(reflect && pass.rect)? this._frustumRect(VP,pass.rect) : this._frustum(VP);
    /* The lamp textures back on units 7-9 at the start of every pass. The MGE water pass
       parks its own on those units (the reflection on 8, the wave volume on 9), and a
       program whose isampler2D then reads an RGBA texture is refused outright — the whole
       translucent phase after the water drew nothing, without an error in sight. Same
       lesson as round 17r and the bake in 17v: a sampler unit is state, and state left by
       a neighbour is what you draw with. */
    this._bindLights(this._lgrid);
    if(opaqueP && scat && now && !(reflect && under)){   // 18du: no sky in the mirror from under the water
      this._gpuMark(reflect? 'reflection:sky' : 'sky');
      { const cc=reflect? fogCol : clearCol;   // 18dt: the underwater colour past everything
        gl.clearColor(cc[0],cc[1],cc[2],1); }
      gl.clear(gl.COLOR_BUFFER_BIT);
      const sk=this.progSky; gl.useProgram(sk.p);
      /* The point this pass is looking at — the *pass's*, not the camera's. The
         reflection draws from the eye's mirror image under the water and must aim at the
         pivot's mirror image too; aiming at the real pivot tilts the reflected sky by
         twice however far the pivot sits above the water line, and slides it whenever the
         pivot moves. Round 17m, Robin: "you can see how the water changes it's reflection
         and refraction when the ball is zoomed further away" — Ctrl-pulling the pivot
         changes its height, so the sky in the sea swung about while the sea itself stood
         still. It was wrong for the whole life of the reflection pass; moving the pivot is
         just the first thing that made it obvious. */
      const tgt=(pass&&pass.target)||[c.tx,c.ty,c.tz];
      const f=[tgt[0]-eye[0],tgt[1]-eye[1],tgt[2]-eye[2]];
      const fl=Math.hypot(f[0],f[1],f[2])||1; f[0]/=fl; f[1]/=fl; f[2]/=fl;
      const r=[f[1]*1-f[2]*0, f[2]*0-f[0]*1, 0];   // f × up(0,0,1)
      const rl=Math.hypot(r[0],r[1],r[2])||1; r[0]/=rl; r[1]/=rl; r[2]/=rl;
      const u=[r[1]*f[2]-r[2]*f[1], r[2]*f[0]-r[0]*f[2], r[0]*f[1]-r[1]*f[0]];   // r × f
      gl.uniform3fv(sk.u.uCamF,f); gl.uniform3fv(sk.u.uCamR,r); gl.uniform3fv(sk.u.uCamU,u);
      gl.uniform1f(sk.u.uTanH,Math.tan(FOV/2));
      gl.uniform1f(sk.u.uAspect,this.cv.width/this.cv.height);
      gl.uniform3fv(sk.u.uSunDir,now.sunDir);
      gl.uniform3fv(sk.u.uSunDiscCol,now.sunDisc);
      gl.uniform1f(sk.u.uSunVis,now.sunVis);
      const cloud=(typeof Sky==='object' && o.clouds!==false)? Sky.cloudTex(now.cloud) : null;
      gl.activeTexture(gl.TEXTURE4); gl.bindTexture(gl.TEXTURE_2D,cloud||this.white);
      gl.uniform1i(sk.u.uCloud,4);
      gl.uniform1i(sk.u.uHasCloud,cloud?1:0);
      /* Round 17x: the clouds' scroll, OpenMW's `mCloudAnimationTimer += duration *
         mCloudSpeed * 0.003` with the weather's Cloud Speed (1.25 for Clear in the game's
         ini), accumulated frame by frame so nothing jumps when a clock wraps. */
      {
        const t=performance.now()/1000;
        const dt=this._cloudLast==null? 0 : Math.min(0.5, Math.max(0, t-this._cloudLast));
        this._cloudLast=t;
        const speed=(now && now.cloudSpeed!=null)? +now.cloudSpeed : 1.25;
        /* Round 17y item 10: and the Time scale's say. Twenty game minutes a real second
           is the game's own pace and drifts the clouds at the game's speed; a faster clock
           hurries them in step. Read from the slider whether or not the clock is running
           — Robin: "have the current speed always no matter if the Time passes is toggled
           on or off" — so a paused clock still has drifting clouds, at the pace its scale
           would give it. */
        const pace=(o.timeScale>0? o.timeScale : 20)/20;
        this._cloudTimer=(this._cloudTimer||0)+dt*speed*0.003*pace;
        gl.uniform1f(sk.u.uCloudDrift,this._cloudTimer%5.0);
      }
      const stars=(typeof Sky==='object' && now.stars>0)? Sky.cloudTex('tx_stars.dds') : null;
      gl.activeTexture(gl.TEXTURE5); gl.bindTexture(gl.TEXTURE_2D,stars||this.white); this._texU=null;
      gl.uniform1i(sk.u.uStars,5);
      gl.uniform1i(sk.u.uHasStars,stars?1:0);
      gl.uniform1f(sk.u.uStarK,now.stars||0);
      /* A quarter turn a day — see uStarRot in FS_SKY. Round 17s gave them a full turn,
         which is a night sky wheeling four times too fast: the game's own is a slow drift
         you notice only between dusk and dawn (round 17y item 9). Round 18h: a quarter turn
         a day is not a whole number of turns a day, so an angle taken straight from the
         clock jumped back ninety degrees each midnight - Robin: "The night sky snaps after
         a while in its rotation. I want it to be a continuous rotation." The angle is
         carried forward from the last hour seen instead, by the shortest way round the
         clock, so midnight is one more minute and a slider dragged from 23 to 1 is two
         hours, not twenty-two. Same rate; nothing to snap to. */
      { const h=o.hour==null? 0 : o.hour;
        if(this._starHour==null){ this._starHour=h; this._starRot=(h/24)*2*Math.PI*0.25; }
        else if(h!==this._starHour){
          let dh=h-this._starHour; dh=((dh+12)%24+24)%24-12;
          this._starRot+=(dh/24)*2*Math.PI*0.25; this._starHour=h;
        }
        gl.uniform1f(sk.u.uStarRot, this._starRot); }
      gl.activeTexture(gl.TEXTURE0);
      airOn(sk);
      gl.depthMask(false); gl.disable(gl.DEPTH_TEST);
      gl.drawArrays(gl.TRIANGLES,0,3);
      gl.depthMask(true); gl.enable(gl.DEPTH_TEST);
    }

    if(opaqueP && this.cellChunks && this.cellChunks.length && o.ground){
      this._gpuMark(reflect? 'reflection:ground' : 'ground');
      const g=this.progGround; gl.useProgram(g.p);
      // Wraithguard: no layer maps until a layer says so (the slab, the overlays).
      this._u1i(g,'uHasLNrm',0); this._u1i(g,'uSpecA',0);
      gl.uniformMatrix4fv(g.u.uVP,false,VP);
      gl.uniform3fv(g.u.uSun,sun);
      gl.uniform1f(g.u.uBright, o.bright==null?1.0:o.bright);
      gl.uniform3fv(g.u.uFogCol,fogCol);
      airOn(g);
      gl.uniform1f(g.u.uFogK,fogK);
      gl.uniform1f(g.u.uTile,16.0);       // Morrowind tiles land once per sub-cell
      gl.uniform1i(g.u.uGrid,o.grid?1:0);
      gl.uniform1i(g.u.uShowSlope,o.showSlope?1:0);
      this._cullUniforms(g,o.showSlope);
      gl.uniform1f(g.u.uMaxAngle,o.maxAngle);
      gl.uniform1f(g.u.uMinH,o.minH);
      gl.uniform1f(g.u.uMaxH,o.maxH);
      gl.uniform1i(g.u.uTex,0);
      gl.uniform1i(g.u.uBlend,1);
      gl.uniform1i(g.u.uUseBlend, o.blend===false?0:1);
      gl.uniform1f(g.u.uAlphaMul,1.0);
      gl.uniform1i(g.u.uUnlit,0);
      gl.uniform1i(g.u.uVCol, o.vcol===false?0:1);   // the record's VCLR, unless switched off
      gl.uniform3fv(g.u.uGridCol, o.paintMode? [1.000,0.627,0.322] : [0.690,0.702,0.635]);
      if(this.brush && o.paintMode){
        gl.uniform4fv(g.u.uBrushS,[0,0,this.brush.r,this.brush.inner]);
        gl.uniform3fv(g.u.uBrushP,[this.brush.x,this.brush.y,this.brush.z||0]);
      }else gl.uniform4fv(g.u.uBrushS,[0,0,0,-1]);
      /* The slab first: the walls and the bottom under the ground, dirt all over, lit
         and fogged like the land and wearing none of its overlays. */
      if(this.slab && o.slab!==false){
        gl.disable(gl.BLEND); gl.depthMask(true);
        gl.uniform1i(g.u.uFirstLayer,1);
        gl.uniform1i(g.u.uUseBlend,0);
        gl.uniform1i(g.u.uVCol,0);
        gl.uniform1i(g.u.uGrid,0);
        gl.uniform1i(g.u.uShowSlope,0);
        gl.uniform4fv(g.u.uBrushS,[0,0,0,-1]);
        gl.uniform2f(g.u.uBlendXf,1.0,0.0);
        gl.uniform1f(g.u.uTile,1.0);   // the slab's UVs are already in tiles
        gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D,this.slab.tex||this.white);
        gl.uniform1i(g.u.uHasTex,this.slab.tex?1:0);
        gl.uniform1i(g.u.uFlat,(!this.slab.tex&&this.slab.flat)?1:0);
        gl.uniform3fv(g.u.uFlatCol,this.slab.flat||[0.42,0.37,0.30]);
        gl.activeTexture(gl.TEXTURE1); gl.bindTexture(gl.TEXTURE_2D,this.white);
        gl.bindVertexArray(this.slab.vao);
        gl.drawElements(gl.TRIANGLES,this.slab.count,gl.UNSIGNED_INT,0);
        gl.uniform1f(g.u.uTile,16.0);
        /* Everything the slab switched off, back on for the land - the blend included:
           left at 0 it cut every join hard, which t_render caught once its own probe
           stopped reading the fog's ramp as the blend. */
        gl.uniform1i(g.u.uUseBlend, o.blend===false?0:1);
        gl.uniform1i(g.u.uGrid,o.grid?1:0);
        gl.uniform1i(g.u.uShowSlope,o.showSlope?1:0);
        this._cullUniforms(g,o.showSlope);
        gl.uniform1i(g.u.uVCol, o.vcol===false?0:1);
        if(this.brush && o.paintMode){
          gl.uniform4fv(g.u.uBrushS,[0,0,this.brush.r,this.brush.inner]);
        }
      }
      /* Round 18cr: the layers' state sent on change. Forty-nine chunks of nine layers
         were 450 draws with a dozen calls each - the blend switched off and on per
         layer, both units re-bound, the flat colour re-sent - and once the objects had
         gone into their groups this loop was the largest caller left. The shadow is
         reset here, so whatever the slab above or the last pass set is sent again once. */
      this._shReset(g);
      let blendOn=null, t0=undefined, t1=undefined, unit=0;
      const onUnit=(u)=>{ if(unit!==u){ unit=u; gl.activeTexture(gl.TEXTURE0+u); } };
      for(const ch of this.cellChunks){
        // Round 18cs: only the chunks on screen. Nine layers a chunk, and never culled.
        // 18ct: and in the reflection, none wholly under the water line.
        if(ch.box3 && (!this._boxInView(planes,ch.box3) || (curClip!=null && (curClipMode===2? ch.box3[2]>curClip : ch.box3[5]<curClip)))) continue;
        gl.bindVertexArray(ch.vao);
        // inner = the cell's own sub-cells, border = the ring copied from its neighbours
        const inner=ch.inner||16, total=ch.total||inner;
        gl.uniform2f(g.u.uBlendXf, inner/total, (total-inner)/(2*total));
        for(let i=0;i<ch.layers.length;i++){
          const L=ch.layers[i];
          const first=(i===0);
          if(first){ if(blendOn!==false){ gl.disable(gl.BLEND); gl.depthMask(true); blendOn=false; } }
          else if(blendOn!==true){ gl.enable(gl.BLEND); gl.blendFunc(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA); gl.depthMask(false); blendOn=true; }
          this._u1i(g,'uFirstLayer', first?1:0);
          const tex=L.tex||this.white;
          if(t0!==tex){ onUnit(0); gl.bindTexture(gl.TEXTURE_2D,tex); t0=tex;
            // Round 18df: a tile some mesh clamped wraps again here (see `_wrapTex0`).
            if(tex._wrap){ gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.REPEAT); gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.REPEAT); tex._wrap=0; } }
          this._u1i(g,'uHasTex',L.tex?1:0);
          this._u1i(g,'uFlat', (!L.tex&&L.flat)?1:0);
          this._u3(g,'uFlatCol', L.flat||FLAT_LAND);
          // Wraithguard: the layer's normal map on unit 3, and its _diffusespec highlight.
          const ln=(o.normalMaps===true && L.nrm)? L.nrm : null;
          this._u1i(g,'uHasLNrm', ln?1:0);
          if(ln){ onUnit(3); gl.bindTexture(gl.TEXTURE_2D,ln); this._u1i(g,'uLNrm',3); }
          this._u1i(g,'uSpecA', (o.normalMaps===true && L.specA && L.tex)?1:0);
          const bl=L.blend||this.white;
          if(t1!==bl){ onUnit(1); gl.bindTexture(gl.TEXTURE_2D,bl); t1=bl; }
          // 18ct: the layer's own quads (the first layer, and any without a map, the whole chunk).
          if(L.count>0) gl.drawElements(gl.TRIANGLES,L.count,gl.UNSIGNED_INT,L.off);
        }
      }
      onUnit(0);
      this._tex0=undefined;   // unit 0 holds a land texture now, whatever the shadow says
      this._u1i(g,'uHasLNrm',0); this._u1i(g,'uSpecA',0);   // the overlays after are marks, not land

      /* Painted ground, over the terrain and under everything else.
       *
       * The same trick as the texture highlight below — the terrain drawn once more in a
       * flat colour with a coverage map deciding where — except the map is the paint and
       * it covers the cell squarely, so the transform is the identity rather than the
       * inset the land textures need. Alpha rather than additive: paint says "nothing
       * grows here", and a mark that brightens the ground reads as the opposite.
       *
       * Only in paint mode or while the section is hovered (`paintShow`). Robin: "The
       * paint should be visible during this mode, but not when outside of it, unless
       * hovering the [Painting tools] section." Which is right — it is a mark on the tool,
       * not on the world, and leaving it on top of every screenshot would make the
       * viewport a worse picture of what the export will contain.
       *
       * Two passes, one per colour, in the same colours the tool buttons wear: burnt
       * orange for "no grass here" and green for "grass here". They cannot overlap on
       * the ground — the brush keeps one texel to one colour — so the order between them
       * says nothing and either could go first. */
      // The overlays are marks on the tool, not on the world: no vertex colour on them.
      gl.uniform1i(g.u.uVCol,0);
      if(!reflect && o.paintShow && (this.paintTex.size||this.growTex.size||this.ruleTex.size)){
        gl.enable(gl.BLEND); gl.blendFunc(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA);
        gl.depthMask(false);
        gl.uniform1i(g.u.uFirstLayer,0);
        gl.uniform1i(g.u.uUseBlend,1);
        gl.uniform1i(g.u.uHasTex,0);
        gl.uniform1i(g.u.uFlat,1);
        gl.uniform1f(g.u.uAlphaMul,0.72);
        gl.uniform1i(g.u.uShowSlope,0);
        gl.uniform2f(g.u.uBlendXf,1.0,0.0);
        /* One pass per layer: the two "whether" colours, then one per rule swatch. They
           cannot overlap within a layer — the brush keeps a texel to one colour — and the
           rule layer is drawn last because it is the one you are usually arranging. */
        const passes=[[this.paintTex,[0.760,0.352,0.180],''],
                      [this.growTex, GROW_TINT,'']];
        for(const w of this.rulePalette){
          // A hidden layer's colours are simply not drawn. Its grass is still there.
          if(this.hiddenLayers && this.hiddenLayers.has(w.layer)) continue;
          passes.push([this.ruleTex, hexToRgb(w.colour), '|'+w.slot]);
        }
        for(const [map,col,suffix] of passes){
          if(!map.size) continue;
          gl.uniform3fv(g.u.uFlatCol,col);
          for(const ch of this.cellChunks){
            if(!ch.grid) continue;
            const t=map.get(ch.grid[0]+','+ch.grid[1]+suffix);
            if(!t) continue;
            gl.bindVertexArray(ch.vao);
            gl.activeTexture(gl.TEXTURE1); gl.bindTexture(gl.TEXTURE_2D,t);
            gl.drawElements(gl.TRIANGLES,ch.count,gl.UNSIGNED_INT,0);
          }
        }
        gl.uniform1f(g.u.uAlphaMul,1.0);
        gl.uniform1i(g.u.uShowSlope,o.showSlope?1:0);
        this._cullUniforms(g,o.showSlope);
      }

      /* The highlight is the same layer drawn once more in flat colour over the ground,
         its own coverage map supplying the shape so the edges feather exactly as the
         blend does.
         
         Round 17i: *mixed* in rather than added. Adding a yellow to ground that is
         already bright — which is what the atmosphere makes it — runs the sum past white
         and the highlight loses its colour. Robin: "The yellow tint highlight on the
         ground gets very white when having atmosphere toggled on. All other paint on the
         ground, and all other highlights [...] stays more yellow." They do because they
         mix: the paint blends over the ground, and the grass and mesh highlights are
         `mix(c, gold, 0.6)` in the shader. This is the same gold, mixed the same way. */
      if(this.hlTex && !reflect){
        gl.enable(gl.BLEND); gl.blendFunc(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA); gl.depthMask(false);
        gl.uniform1i(g.u.uFirstLayer,0);
        gl.uniform1i(g.u.uUseBlend,1);
        gl.uniform1f(g.u.uAlphaMul,1.0);
        gl.uniform1f(g.u.uHiTint,0.6);   // the strength the grass and the meshes tint by
        gl.uniform1i(g.u.uShowSlope,0);
        gl.uniform1i(g.u.uGrid,0);
        for(const ch of this.cellChunks){
          const inner=ch.inner||16, total=ch.total||inner;
          gl.uniform2f(g.u.uBlendXf, inner/total, (total-inner)/(2*total));
          let bound=false;
          for(const L of ch.layers){
            if((L.id||'').toLowerCase()!==this.hlTex || !L.blend) continue;
            if(!bound){ gl.bindVertexArray(ch.vao); bound=true; }
            /* The layer itself, drawn once more and tinted: its own texture, its own
               coverage. Drawing a flat colour instead is what made the highlight a
               different kind of thing from the ground under it. */
            gl.uniform1i(g.u.uHasTex,L.tex?1:0);
            gl.uniform1i(g.u.uFlat, (!L.tex&&L.flat)?1:0);
            gl.uniform3fv(g.u.uFlatCol, L.flat||[0.29,0.285,0.255]);
            gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D,L.tex||this.white);
            gl.activeTexture(gl.TEXTURE1); gl.bindTexture(gl.TEXTURE_2D,L.blend);
            gl.drawElements(gl.TRIANGLES,ch.count,gl.UNSIGNED_INT,0);
          }
        }
        gl.uniform1f(g.u.uHiTint,0);
        gl.activeTexture(gl.TEXTURE0);
      }

      gl.disable(gl.BLEND); gl.depthMask(true);
      gl.activeTexture(gl.TEXTURE0);
    }
    else if(this.ground && o.ground){
      const g=this.progGround; gl.useProgram(g.p);
      gl.uniformMatrix4fv(g.u.uVP,false,VP);
      gl.uniform3fv(g.u.uSun,sun);
      gl.uniform1f(g.u.uBright, o.bright==null?1.0:o.bright);
      gl.uniform3fv(g.u.uFogCol,fogCol);
      airOn(g);
      gl.uniform1f(g.u.uFogK,fogK);
      gl.uniform1f(g.u.uTile,o.tile);
      gl.uniform1i(g.u.uGrid,o.grid?1:0);
      gl.uniform1i(g.u.uShowSlope,o.showSlope?1:0);
      this._cullUniforms(g,o.showSlope);
      gl.uniform1f(g.u.uMaxAngle,o.maxAngle);
      gl.uniform1f(g.u.uMinH,o.minH);
      gl.uniform1f(g.u.uMaxH,o.maxH);
      gl.activeTexture(gl.TEXTURE0);
      gl.bindTexture(gl.TEXTURE_2D,this.ground.tex||this.white);
      gl.uniform1i(g.u.uTex,0);
      gl.uniform1i(g.u.uHasTex,this.ground.tex?1:0);
      // The plain patch has no coverage map. Without these the shader samples whatever
      // sits on unit 1 and discards where it reads dark, punching holes in the ground.
      gl.uniform1i(g.u.uFirstLayer,1);
      gl.uniform1i(g.u.uUseBlend,0);
      gl.uniform1i(g.u.uFlat,0);
      gl.uniform1f(g.u.uAlphaMul,1.0);
      gl.uniform1i(g.u.uUnlit,0);
      gl.uniform1i(g.u.uVCol,1);
      // Simplified mode has no cell to paint, so neither the orange grid nor the brush
      // belongs here — and leaving the uniforms unset would show whatever cell mode left.
      gl.uniform3fv(g.u.uGridCol,[0.690,0.702,0.635]);
      gl.uniform4fv(g.u.uBrushS,[0,0,0,-1]);
      gl.uniform2f(g.u.uBlendXf,1.0,0.0);
      gl.uniform1i(g.u.uBlend,1);
      gl.activeTexture(gl.TEXTURE1); gl.bindTexture(gl.TEXTURE_2D,this.white);
      gl.activeTexture(gl.TEXTURE0);
      gl.bindVertexArray(this.ground.vao);
      gl.drawElements(gl.TRIANGLES,this.ground.count,gl.UNSIGNED_INT,0);
    }

    // No grass in the reflection: thousands of blades for a shimmer nobody could read.
    if(grassP){
    this._gpuMark('grass');
    // Round 18cs: the multi-draw program, or - the test hook - the one-draw-a-run one.
    const s=(this.noMultiDraw && this.progGrass.multi)? this._progGrassF() : this.progGrass;
    gl.useProgram(s.p);
    this._shReset(s);   // round 18cr: the per-batch uniforms below are sent on change
    gl.uniformMatrix4fv(s.u.uVP,false,VP);
    gl.uniform3fv(s.u.uSun,sun);
      gl.uniform1f(s.u.uBright, o.bright==null?1.0:o.bright);
    gl.uniform3fv(s.u.uFogCol,fogCol);
    airOn(s);
    gl.uniform1f(s.u.uFogK,fogK);
    gl.uniform1i(s.u.uAlign,o.align?1:0);
    gl.uniform1i(s.u.uTex,0);
    /* The ground tint: the ground map on unit 2, and where it is. Only with cell terrain
       under the grass and the slider above zero - the plain patch of simplified mode has
       no ground to speak of, and the map is not baked for it. */
    const tintK=(o.tint>0 && this.groundMap && this.groundMap.xf) ? Math.min(1,o.tint) : 0;
    gl.uniform1f(s.u.uTintK,tintK);
    gl.uniform1i(s.u.uTint,tintK>0?1:0);
    gl.uniform1i(s.u.uVCol, o.vcol===false?0:1);
    /* Round 14: the blades drawn MGE's way - see uGrassLit in FS_GRASS. The coverage
       the shader writes into alpha becomes the multisample's, which is what softens
       the cutout's edge; off again after the pass, the objects are drawn plainly. */
    gl.uniform1i(s.u.uGrassLit, o.mgeGrass===false?0:1);
    if(o.mgeGrass!==false) gl.enable(gl.SAMPLE_ALPHA_TO_COVERAGE);
    /* Round 18aw: the wind is set per batch below - a batch that says `wind:false` (an
       object placed by id, a mesh from outside the grass folder) is drawn still. */
    const windNow=this.windNow();
    gl.uniform1f(s.u.uWind, windNow);
    gl.uniform1f(s.u.uTime, (performance.now()/1000)%3600);
    gl.uniform1i(s.u.uGround,2);
    gl.activeTexture(gl.TEXTURE2);
    gl.bindTexture(gl.TEXTURE_2D, tintK>0? this.groundMap.tex : this.white);
    gl.uniform4fv(s.u.uGroundXf, tintK>0? this.groundMap.xf : [0,0,0,0]);
    const hl=this.hlSlot;
    /* Round 17u: and the grass is culled too. It was not before — 15.2 million triangles
       of it went in every frame on Robin's 49-cell Balmora, whichever way he was facing.
       Round 17v: a batch is one rule slot's blades across *every* loaded cell now, with a
       run per cell inside it (`groups`, built in 18_cellpreview.js) — the same shape the
       objects took in 17r, for the same reason: 3,472 grass batches on a 49-cell frame
       were 3,472 draw calls, and at 3.6 us a call that was a good part of the frame. So
       the test is in two steps: the whole batch against its union box, and then each
       cell's run, with the runs that survive and sit next to each other drawn together.
       Everything in view is still one call a batch. */
    const gplanes=planes;
    /* Round 17v: how far the grass goes. Zero is everything, which is what it was.
       The band is a fifth of the distance, so the grass thins out rather than ending. */
    const gfar=+o.grassFar||0;
    gl.uniform2f(s.u.uGrassFar, gfar, gfar*0.2);
    gl.uniform3fv(s.u.uEyeG, eye);
    /* Round 18dn: pixels per unit at unit distance, for the vertex shader's "is this
       blade under a pixel" (its `far`). The main buffer's height even in the reflection
       pass, whose target is no taller: a taller guess draws more, never less. */
    gl.uniform1f(s.u.uPxK, this._pxKTest!=null? this._pxKTest : gl.drawingBufferHeight/(2*Math.tan(FOV/2)));   // _pxKTest: t_tint holds the rule off to see what it saves
    /* A box wholly past the grass distance is not submitted at all: shrinking a blade
       away still costs its vertices, and at forty-nine cells that is most of them. */
    const farOff=(bx)=>{
      const dx=Math.max(bx[0]-eye[0], 0, eye[0]-bx[3]);
      const dy=Math.max(bx[1]-eye[1], 0, eye[1]-bx[4]);
      return dx*dx+dy*dy > gfar*gfar;
    };
    /* Round 18cs: a batch is one multi-draw call. Its instances are a texture on unit 10
       (`buildBatch`), each visible cell run a sub-draw whose first instance rides the
       `uRunBase` array by `gl_DrawID` - as the objects' groups are drawn - so a run is no
       longer three attribute pointers moved and moved back. Without WEBGL_multi_draw,
       `uBase` and one draw per run: two calls where there were seven. The sub-draw arrays
       are the renderer's, reused across batches and frames. */
    const multi=(s.multi && this._mdraw)? this._mdraw : null;
    const gCount=this._gCount||(this._gCount=new Int32Array(MAXRUN)), gOff=this._gOff||(this._gOff=new Int32Array(MAXRUN));
    const gInst=this._gInst||(this._gInst=new Int32Array(MAXRUN)), gBase=this._gBase||(this._gBase=new Int32Array(MAXRUN+4));
    let curInst=null;
    const flush=(b,n)=>{
      if(multi){
        gl.uniform4iv(s.u.uRunBase, gBase, 0, ((n+3)>>2)<<2);
        multi.multiDrawElementsInstancedWEBGL(gl.TRIANGLES, gCount, 0, gl.UNSIGNED_SHORT, gOff, 0, gInst, 0, n);
        this._multiCalls++;
      }else{
        for(let i=0;i<n;i++){ this._u1i(s,'uBase',gBase[i]); gl.drawElementsInstanced(gl.TRIANGLES,b.count,gl.UNSIGNED_SHORT,0,gInst[i]); }
      }
      this._drawn+=n;
    };
    /* Round 18ct drew the batches nearest first, for the depth test's sake, and 18cu took
       it back: Robin's GPU clock put the grass at 9.4 ms for 16.6 M triangles - bound by
       the triangles, not by the shading the early rejections would have saved - and the
       sort's second pass over the runs was page time for nothing. Batch order, as built. */
    for(const b of this.batches){
      if(!b.n || !b.hold || !b.hold.it) continue;
      if(b.box && !this._boxInView(gplanes,b.box)) continue;
      if(gfar>0 && b.box && farOff(b.box)) continue;
      // Never more than the texture holds — see the note in `buildBatch`.
      const cap=Math.min(b.n,b.hold.n);
      if(cap<=0) continue;
      let nr=0;
      if(!b.groups || !b.groups.length){
        gCount[0]=b.count; gOff[0]=0; gInst[0]=cap; gBase[0]=0; nr=1;
        this._drawnInst+=cap;
      }else{
        for(const gr of b.groups){
          if(!this._boxInView(gplanes,gr.b)) continue;
          if(gfar>0 && farOff(gr.b)) continue;
          const n=Math.min(gr.n, cap-gr.at);
          if(n<=0) continue;
          if(nr && gBase[nr-1]+gInst[nr-1]===gr.at) gInst[nr-1]+=n;
          else if(nr<MAXRUN){ gCount[nr]=b.count; gOff[nr]=0; gInst[nr]=n; gBase[nr]=gr.at; nr++; }
          else gInst[nr-1]+=gr.at+n-(gBase[nr-1]+gInst[nr-1]);   // past the array: widen the last run over the gap
          this._drawnInst+=n;
        }
        if(!nr) continue;
      }
      this._meshTex(s,b);
      this._meshMode(s,b,false);
      this._uBlade(s,b);
      // Round 18o: the tilt is the batch's (a card's own, or its rule's); the scene's
      // switch only stands in for a batch that does not say.
      this._u1i(s,'uAlign', b.align==null? (o.align?1:0) : (b.align?1:0));
      this._u1f(s,'uWind', b.wind===false? 0 : windNow);                      // round 18aw
      this._u1i(s,'uHi', hl==null? -1 : (b.key===hl? 1 : 0));
      const it=b.hold.it;
      if(curInst!==it){ gl.activeTexture(gl.TEXTURE0+INST_UNIT); gl.bindTexture(gl.TEXTURE_2D,it.tex); gl.activeTexture(gl.TEXTURE0); curInst=it; }
      gl.bindVertexArray(b.vao);
      flush(b,nr);
    }
    this._u1i(s,'uHi',-1);
    gl.disable(gl.SAMPLE_ALPHA_TO_COVERAGE);
    }

    if(this.statics.length && o.statics!==false){
      this._gpuMark(reflect? 'reflection:objects' : (clearP && !opaqueP)? 'translucent' : 'objects');
      /* Indoors, only the faces pointing at you.
       *
       * Robin: "For viewing interiors, we need to get rid of back face rendering of
       * meshes. Otherwise you can't see into a room, you will just see a box." A Morrowind
       * interior is a shell whose walls are single-sided with their normals pointing
       * *inward*, so the wall between you and the room is a back face — cull it and you
       * are looking in, which is what the game and the Construction Set both do.
       *
       * **Which way round, measured rather than assumed.** Across `In_AR_01.nif` (1,804
       * triangles) and `in_6th_chalk00.nif`, 100.00% of triangles are wound
       * counter-clockwise as seen from the side their own vertex normal points to — so
       * GL's defaults, `frontFace(CCW)` and `cullFace(BACK)`, keep exactly the faces whose
       * normals face the camera. Nothing to set but the enable.
       *
       * Only the objects, and only indoors. Outdoors a mesh is looked at from outside and
       * culling would take the far side of a rock nobody can see anyway — no gain, and one
       * badly-wound mod mesh away from a hole in the scenery. */
      /* Round 17w: back faces culled on every shape, as the game draws them, and two-sided
         only where a NiStencilProperty asks (see _meshMode). `cullStatics` used to turn
         this on indoors only; the picker still reads it for the rooms. */
      gl.enable(gl.CULL_FACE); gl.frontFace(gl.CCW); this._culling=true; this._cw=false;
      /* Round 18cr: two programs draw the objects now - the groups' (`_progM`, instances
         from the texture) and the batches' own (`progStatic`) - and both are told the
         same things about this frame. Told afresh: the shadow of what each was last told
         is reset here, so a uniform this pass sets is one it has actually sent. */
      const setup=(pr)=>{
      gl.useProgram(pr.p); this._shReset(pr); this._tex0=undefined;
      const st=pr;
      gl.uniform1f(st.u.uHour, o.hour==null? -1 : o.hour);
      gl.uniform1i(st.u.uLightMode, o.lights==='always'? 1 : o.lights==='off'? 2 : 0);
      /* Round 17x: a room shows its windows' INT-DAY branch by day; outside, ON at night.
         Round 17y: "a room" here is one lit by its own record (`opts.room`) - a room flagged
         to behave like an exterior has the sky and keeps the exterior's windows too. */
      gl.uniform1i(st.u.uIndoors, o.room? 1 : 0);
      gl.uniformMatrix4fv(st.u.uVP,false,VP);
      gl.uniform3fv(st.u.uSun,sun);
      gl.uniform1f(st.u.uBright, o.bright==null?1.0:o.bright);
      gl.uniform3fv(st.u.uFogCol,fogCol);
      airOn(st);
      gl.uniform1f(st.u.uFogK,fogK);
      gl.uniform1i(st.u.uTex,0);
      this._u1i(st,'uHi',-1);   // objects are never the thing being highlighted
      // Stated rather than assumed: `thumbDraw` shares this program and turns it on.
      gl.uniform1i(st.u.uOpaque,0);
      gl.uniform1f(st.u.uTintK,0.0);   // the ground tint is the grass's; a rock is its own colour
      gl.uniform1i(st.u.uVCol, o.vcol===false?0:1);
      /* The culling overlay, exactly as the ground pass sets it. Grass and thumbnails
         share this program and leave uShowSlope at 0, so the shade lands only on the
         placed objects a rule's limits are actually about. */
      // Round 18l: in the editor the shade is the editor's own switch, slope only.
      gl.uniform1i(st.u.uShowSlope,(o.inspecting? o.showSlopeMesh : o.showSlopeStatics)?1:0);
      this._cullUniforms(st,(o.inspecting? o.showSlopeMesh : o.showSlopeStatics));
      gl.uniform1f(st.u.uMaxAngle,o.maxAngle);
      gl.uniform1f(st.u.uMinH,o.inspecting? -1e9 : o.minH);
      gl.uniform1f(st.u.uMaxH,o.inspecting? 1e9 : o.maxH);
      /* The brush outline, on the rocks, only while the brush can actually land on them —
         painting, with "Paint on statics too" on (which a room forces). The sphere's
         centre and its two radii, the same three numbers the terrain shader is given:
         one brush, drawn wherever it cuts something. */
      const rb=this.brush;
      if(rb && o.paintMode && rb.onStatics){
        gl.uniform4fv(st.u.uBrushS,[0,0,rb.r,rb.inner]);
        gl.uniform3fv(st.u.uBrushP,[rb.x,rb.y,rb.z||0]);
      }else{
        gl.uniform4fv(st.u.uBrushS,[0,0,0,-1]);
      }
      };
      const st=this.progStatic;
      // Which texture's faces are lit — the inspect view's grid sets it; null otherwise.
      const htk=this.hiTexKey==null? null : this.hiTexKey;
      /* Round 18q: the moths, when the mod that supplies them is installed, are one
         mesh's instances among the statics - so they are skipped here rather than left
         out of the scene, and the clock can move across dusk without rebuilding a cell
         for it. */
      const moths=this.mothsVisible();
      /* Round 18cr: the groups first - one call a group - through their own program. Only
         when they were built from *this* list: a test that swaps `statics` for a subset
         draws the subset, on the path below, as it always did. */
      const G=(this._groups && this._groups.src===this.statics)? this._groups : null;
      if(G && G.all.length){
        gl.activeTexture(gl.TEXTURE0+INST_UNIT); gl.bindTexture(gl.TEXTURE_2D,G.instTex); gl.activeTexture(gl.TEXTURE0);
        this._cullGroups(G,planes,VP, reflect? pass.clipZ : null);
        gl.bindVertexArray(G.vao);
        /* Round 18cs: two programs - the bakes' (one index range a group, no uniform
           between) and the runs' (by instance). Round 18ct: the runs first, then the
           bakes. The runs are the big meshes - the walls, the trees - and drawn before
           the small things behind them, they leave the depth buffer to reject those
           before they are shaded; the order between solid things is nothing to the
           picture, and this order costs nothing. */
        const pm=this._progM(!!G.multi);
        setup(pm);
        /* Round 18di: the NoSorter groups after every other solid thing. The game draws
           them in scene order among the solids, which for a mesh puts a pool's water
           after the pool's bottom; here the groups go by texture, and a water drawn
           before its bottom wrote the depth that then rejected the bottom - Robin's
           wellpod "should be transparent for one". Last, they blend over what they
           stand in front of, depth written, unsorted among themselves as in the game. */
        const ns=g=>!!(g.blend && g.blend.nosort);
        if(opaqueP) for(const g of G.order) if(!ns(g)) this._drawGroup(pm,g,G);
        if(opaqueP && G.baked){
          const pb=this._progB();
          setup(pb);
          for(const g of G.order) this._drawBake(pb,g,G);
          gl.useProgram(pm.p);   // the see-through groups draw through `pm`; its uniforms and shadow still stand
        }
        if(opaqueP) for(const g of G.order) if(ns(g)) this._drawGroup(pm,g,G);
        if(clearP && G.clear.length){
          /* Round 18dg: far to near, instance by instance, depth written (see
             `_sortClear`). Until now: group by group in texture order with the depth
             left alone, which is what painted a far leaf cluster over a near one. */
          gl.enable(gl.BLEND);
          this._drawSorted(pm,G);
          gl.depthMask(true); gl.disable(gl.BLEND);
          gl.blendFunc(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA);
        }
        gl.bindVertexArray(null);
      }
      /* The see-through ones last, and over the solid ones (round 17h): a pool of water
         has to have the bottom of the pool drawn behind it before it can be seen
         through. Depth writes off while they draw, so two sheets of water do not cut
         each other out. */
      const solid=[], clear=[];
      const own=G? this._legacyStatics : this.statics;
      // Round 18dg: a NoSorter batch is a solid one that blends (see alphaBlend).
      for(const b of own){ if(b.n && (!b.moths || moths)) ((b.blend && !b.blend.nosort)? clear : solid).push(b); }
      /* Told every pass whether or not anything below draws with it: the objects'
         program is where a test (and the inspect view's bake) reads what the pass was
         asked - the cull limits, the brush - and a frame that skipped it would leave
         last frame's answers there. Some thirty calls; the groups above are the saving. */
      const fillHi=(this.hlStatics||[]).length && this.hlMode!=='outline';   // 18dq: the outline is an image pass
      const legacyDraws=(opaqueP && solid.length) || (clearP && clear.length) ||
                        (opaqueP && fillHi && !reflect);
      setup(st);
      /* Round 17r: only what is on screen.
       *
       * Nothing was culled before this — every batch and every instance in it went in
       * every frame, so looking one way at forty-nine cells submitted about eight times
       * the scenery that could be seen. A batch is one part of one mesh across every
       * loaded cell, so its own box is the whole scene and testing *it* would reject
       * nothing; what carries the box is each cell's run of instances inside it
       * (`groups`, built in 18_cellpreview.js). Runs that survive the test and sit next
       * to each other are drawn together, so a batch wholly on screen is still one call.
       *
       * The instance attributes are re-pointed into the same buffer at the run's offset.
       * WebGL2 has no `baseInstance`, and a VAO remembers its attribute pointers, so
       * moving them is how a draw starts partway along an instance stream. */
      /* Round 18cr: the runs a frame merges, in two arrays reused across batches and
         frames rather than an object a run - a few thousand short-lived objects a frame
         were a garbage-collector pause every few frames, felt as a hitch. */
      const runAt=this._runAt||(this._runAt=new Int32Array(256));
      const runN=this._runN||(this._runN=new Int32Array(256));
      const drawStatic=b=>{
        this._ensureVao(b);   // 18cr: built on first use for what draws on its own
        this._meshTex(st,b);
        this._meshMode(st,b,true);
        /* Per batch rather than once outside the loop, only while a texture is being
           hovered: its faces gold, the rest dimmed — the same two answers every other
           highlight speaks in. */
        this._u1i(st,'uHi', htk==null? -1 : (b.texKey===htk? 1 : 0));
        gl.bindVertexArray(b.vao);
        if(!b.groups || !b.groups.length || !b.mb){
          gl.drawElementsInstanced(gl.TRIANGLES,b.count,gl.UNSIGNED_SHORT,0,b.n);
          this._drawn++; this._drawnInst+=b.n;
          return;
        }
        let nr=0;
        const zc=reflect? pass.clipZ : -Infinity;   // 18ct: nothing wholly under the water line
        for(const g of b.groups){
          if(g.b[5]<zc || !this._boxInView(planes,g.b)) continue;
          if(nr && runAt[nr-1]+runN[nr-1]===g.at) runN[nr-1]+=g.n;
          else if(nr<runAt.length){ runAt[nr]=g.at; runN[nr]=g.n; nr++; }
          else runN[nr-1]+=g.at+g.n-(runAt[nr-1]+runN[nr-1]);   // past the array: widen the last run over the gap
        }
        if(!nr) return;
        for(let i=0;i<nr;i++){
          const at=runAt[i], n=runN[i];
          gl.bindBuffer(gl.ARRAY_BUFFER,b.mb);
          for(let k=0;k<3;k++) gl.vertexAttribPointer(3+k,4,gl.FLOAT,false,48,at*48+k*16);
          // Round 17w: and the lamp hours, when the batch carries them.
          if(b.lb){ gl.bindBuffer(gl.ARRAY_BUFFER,b.lb); gl.vertexAttribPointer(8,2,gl.FLOAT,false,0,at*8); }
          gl.drawElementsInstanced(gl.TRIANGLES,b.count,gl.UNSIGNED_SHORT,0,n);
          this._drawn++; this._drawnInst+=n;
        }
      };
      // 18di: the NoSorter batches after the rest of the solid ones (see the groups).
      const nsb=b=>!!(b.blend && b.blend.nosort);
      if(opaqueP){
        for(const b of solid) if(!nsb(b)) drawStatic(b);
        for(const b of solid) if(nsb(b)){
          gl.enable(gl.BLEND); gl.blendFunc(gl[b.blend.src], gl[b.blend.dst]);
          if(st.u.uFogBlack) this._u1i(st,'uFogBlack', b.blend.dst==='ONE'? 1 : 0);
          drawStatic(b);
          gl.disable(gl.BLEND); gl.blendFunc(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA);
          if(st.u.uFogBlack) this._u1i(st,'uFogBlack',0);
        }
      }
      if(clearP && clear.length){
        /* Round 18dg: these few (the batches the groups cannot take - skinned, moths,
           a hovered texture's) write depth like the rest, unless their shape says
           test-only; they are not sorted, and draw after the sorted groups. */
        gl.enable(gl.BLEND);
        let zw=null;
        for(const b of clear){
          gl.blendFunc(gl[b.blend.src], gl[b.blend.dst]);
          // Round 18h: what adds onto the picture is fogged to black, not to the haze.
          if(st.u.uFogBlack) this._u1i(st,'uFogBlack', b.blend.dst==='ONE'? 1 : 0);
          const w=b.zwrite!==false && !this._clearNoZ;   // 18ea: see `_drawSorted`
          if(w!==zw){ gl.depthMask(w); zw=w; }
          drawStatic(b);
        }
        if(st.u.uFogBlack) this._u1i(st,'uFogBlack', 0);
        gl.depthMask(true); gl.disable(gl.BLEND);
        gl.blendFunc(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA);
        this._u1f(st,'uMatAlpha',1);
      }
      if(htk!=null && legacyDraws) this._u1i(st,'uHi',-1);
      /* The highlighted ones again, over themselves, in the same gold the grass
         highlight uses. `LEQUAL` because this is the identical geometry at the identical
         depth: under the default `LESS` every fragment would lose to the copy already
         there and nothing would show. */
      if(opaqueP && fillHi && !reflect){
        gl.depthFunc(gl.LEQUAL);
        this._u1i(st,'uHi',1);
        for(const b of this.hlStatics){
          if(!b.n) continue;
          this._ensureVao(b);
          this._meshTex(st,b);
          this._meshMode(st,b,true);
          this._u1i(st,'uHi',1);
          gl.bindVertexArray(b.vao);
          gl.drawElementsInstanced(gl.TRIANGLES,b.count,gl.UNSIGNED_SHORT,0,b.n);
        }
        this._u1i(st,'uHi',-1);
        gl.depthFunc(gl.LEQUAL);   // 18dh: the frame's own function, not LESS - the sorted see-through pass draws after this
      }
      /* And the paint on them, over the objects themselves.
       *
       * `LEQUAL` for the same reason the highlight above uses it: this is the identical
       * geometry at the identical depth, and under the default `LESS` every fragment would
       * lose to the copy already drawn and nothing would show. Blended rather than
       * replaced, and with the depth buffer left alone, so the tint sits *on* the rock
       * instead of standing in for it. */
      const pnt=(this.paintedStatics||[]);
      /* Shown under the same switch the painted *ground* is: while you are painting, or
         while the Painting tools section is hovered. Robin asked for the ground tint to
         come and go that way, and a rock that stayed coloured after you put the brush down
         would be a second answer to the same question. */
      if(opaqueP && pnt.length && o.paintShow && !reflect){
        const pp=this.progStaticPaint; gl.useProgram(pp.p);
        gl.uniformMatrix4fv(pp.u.uVP,false,VP);
        gl.uniform3fv(pp.u.uSun,sun);
        /* The terrain tint's own numbers — alpha, brightness, fog — so paint reads as one
           paint wherever it lies. The lambert curve is matched inside the shader. */
        gl.uniform1f(pp.u.uAlpha,0.72);
        gl.uniform1f(pp.u.uBright, o.bright==null?1.0:o.bright);
        gl.uniform3fv(pp.u.uFogCol,fogCol);
        airOn(pp);
        gl.uniform1f(pp.u.uFogK,fogK);
        gl.uniform1i(pp.u.uDabs,0);
        gl.uniform1i(pp.u.uGrid,1);
        {
          const on=new Float32Array([1,1,1,1]);
          for(const i of (this.hiddenLayerIx||[])) if(i>=0&&i<4) on[i]=0;
          gl.uniform1fv(pp.u.uLayerOn,on);
        }
        gl.depthMask(false);
        gl.enable(gl.BLEND);
        gl.blendFunc(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA);
        /* Nudged toward the eye, because this is the identical geometry at the identical
           depth: without the offset the tint and the rock resolve their tie per pixel per
           frame, which is the shimmer Robin reported as bad visibility. */
        gl.enable(gl.POLYGON_OFFSET_FILL);
        gl.polygonOffset(-1.0,-2.0);
        const drawTint=()=>{
          for(const b of pnt){
            if(!b.n || !b.count || !b.dabTex) continue;
            this._meshMode(pp,b,true);   // the same faces the object itself shows
            gl.uniform1i(pp.u.uCount,b.dabTex.count|0);
            gl.uniform3fv(pp.u.uPal,b.pal);
            gl.activeTexture(gl.TEXTURE0);
            gl.bindTexture(gl.TEXTURE_2D,b.dabTex.tex);
            /* The bucket grid, when the mask is big enough to have earned one. A mask
               without one leaves uGridCells at 0 and the shader walks the lot — but the
               sampler still has to be given something bound, or a driver reading an
               incomplete texture unit is entitled to draw nothing at all. */
            const g=b.dabTex.grid;
            gl.activeTexture(gl.TEXTURE1);
            gl.bindTexture(gl.TEXTURE_2D, g? g.tex : b.dabTex.tex);
            gl.uniform1i(pp.u.uGridCells, g? g.cells : 0);
            if(g){
              gl.uniform3fv(pp.u.uGridLo,g.lo);
              gl.uniform3fv(pp.u.uGridInv,g.inv);
              gl.uniform3iv(pp.u.uGridN,g.N);
              gl.uniform1i(pp.u.uGridBase,g.base);
            }
            gl.activeTexture(gl.TEXTURE0);
            gl.bindVertexArray(b.vao);
            gl.drawElementsInstanced(gl.TRIANGLES,b.count,gl.UNSIGNED_SHORT,0,b.n);
          }
        };
        gl.depthFunc(gl.LEQUAL);
        drawTint();
        /* There was a second pass here — the same tint faint through the stone, so paint
           on the far side said "I am here, turn the camera". Robin asked for it gone:
           paint reading through a rock looked like paint *on* things behind it, which is
           a worse lie than a hidden far slope. The surface shows what the surface has. */
        gl.disable(gl.POLYGON_OFFSET_FILL);
        gl.disable(gl.BLEND);
        gl.depthMask(true);
        gl.depthFunc(gl.LEQUAL);   // 18dh: the frame's own function (see the highlight above)
      }
      // Handed back the way it was found, so nothing below has to know this happened.
      gl.disable(gl.CULL_FACE); gl.frontFace(gl.CCW); this._culling=false; this._cw=false;
    }
    /* Round 17w: the particle systems — mist, flame, smoke — over the objects and under
       the water, in the real pass only. See 28_particles.js. */
    if((clearP||belowP) && !reflect && o.particles!==false && this.drawParticles && (this._parts || (this.wantsPrecip && this.wantsPrecip()))){   // Wraithguard: or the weather's (44_wg_precip.js)
      this._gpuMark('particles');
      // With water: what is under it before the surface, what is over it after. Without: all.
      const side = !this.water? 0 : belowP? -1 : (phase==='translucent'? 1 : 0);
      if(!belowP || this.water) this.drawParticles(VP,eye,airOn,sun,fogK,side);
    }
    };   // drawScene

    /* Round 18ee: the sun's cascades, before anything that reads them. Once a frame,
       whatever the scene is drawn into afterwards - the map is in world space and every
       later pass, reflection included, samples the same one. It hands the framebuffer,
       the viewport and the sampler units back as it found them. */
    if(o.shadows && this.drawShadowMap) this.drawShadowMap(eye, now, room, scat);

    /* Round 15 items 8-10: the water drawn MGE XE's way (26_water.js) - the scene twice,
       into offscreen targets, and a surface that refracts, reflects and glints. Off, the
       plain translucent sheet as before. */
    if(this.wantsOffscreen && this.wantsOffscreen(now,scat) && this.drawWaterMGE(drawScene,VP,eye,airOn,fogK,now,scat,P,V)){
      // drawn, and the scene with it
    }
    else {
    drawScene(VP,eye,{phase:'opaque'});
    /* Water after everything that writes depth and before everything that does not: it
       is translucent, so what is under it has to be on the screen already for it to
       tint anything — and a waterfall over it has to come after it, or it is tinted too
       (round 17x). Round 18f: the particles under the water line are among what it tints. */
    drawScene(VP,eye,{phase:'particlesBelow'});
    this.drawPlainWater(VP,airOn,fogK);
    drawScene(VP,eye,{phase:'translucent'});
    }   // plain water

    /* The stepping arrows, last and with the depth buffer cleared under them. They are
       controls rather than scenery: an arrow that disappeared behind a hill would be a
       button you cannot press, and the ground beyond the edge of the load is exactly
       where a hill is most likely to be. */
    /* The stepping arrows, in two passes.
     *
     * They have to be reachable — an arrow that disappears behind a hill is a button you
     * cannot press, and the ground just past the edge of the load is exactly where a hill
     * is most likely to be. The first version cleared the depth buffer and drew them flat
     * on top, which made them reachable and also made them look pasted onto the screen:
     * nothing about the picture said the far one was behind anything.
     *
     * So: the part in front is drawn normally, and the part behind is drawn again with
     * the depth test inverted. Neither pass writes depth — the arrows are furniture, and
     * grass drawn after them should not be cut out by them.
     *
     * Three passes, not two. Robin: "Make the arrows just a bit brighter when looking
     * through terrain, now they are a bit too muted, to the point of being hard to
     * notice. Maybe add an outline that is the same color as unobstructed view." Which is
     * the right shape for it — a body dimmed far enough to read as *behind* something is
     * a body too dim to find, and turning the whole thing up just puts it back in front.
     * An outline at full strength around a muted body says both at once: the edge catches
     * the eye, the flat interior says there is a hill in the way.
     *
     * The occluded pass goes first so the solid one overwrites it wherever both are
     * visible, which is the seam between the two.
     */
    if(this.arrows && o.arrows!==false){
      const ap=this.progArrow; gl.useProgram(ap.p);
      gl.uniformMatrix4fv(ap.u.uVP,false,VP);
      gl.bindVertexArray(this.arrows.vao);
      gl.depthMask(false);
      const hot=[1.000,0.627,0.471], warm=[0.851,0.463,0.310];
      /* A quarter of the way to the fog — back where it started, once there was an
         outline to carry the shape. That is the point of the outline: the body no longer
         has to be bright enough to *find*, only dark enough to say "behind something",
         so it can be as quiet as it looks right rather than as loud as it needs to be.
         Round 18dd: "the fog" here is the viewport's fixed grey (`fogCol`), not the
         theme's backdrop - the arrows are controls, and a control looks the same in
         every theme. */
      const dim=c=>[c[0]*0.34+this.fogCol[0]*0.66,
                    c[1]*0.34+this.fogCol[1]*0.66,
                    c[2]*0.34+this.fogCol[2]*0.66];
      const draw=(r,col,swell,flat)=>{
        gl.uniform3fv(ap.u.uCol,col);
        gl.uniform1f(ap.u.uSwell,swell);
        gl.uniform1f(ap.u.uFlat,flat);
        gl.drawElements(gl.TRIANGLES,r.count,gl.UNSIGNED_SHORT,r.first*2);
      };
      for(const pass of ['rim','behind','front']){
        gl.depthFunc(pass==='front'? gl.LEQUAL : gl.GREATER);
        /* The solid pass writes depth; the two that draw through terrain do not.
         *
         * Robin, with a screenshot from overhead: "Looks a bit like back face rendering,
         * inverted normals or something else. I would like it to be a solid arrow shape
         * please." Neither, as it turned out — it was the arrow failing to sort against
         * *itself*. With no depth writing, back-face culling is the only thing keeping
         * one surface off another, and culling is enough only for a convex shape. An
         * arrow is not convex: from overhead the walls of the notches behind the barbs
         * face the camera just as the top does, and being drawn after it, they painted
         * over it. Hence the step in the middle of a shape that should be flat.
         *
         * Writing depth in the solid pass costs nothing that was being protected. The
         * original reason not to was grass, and grass is drawn *before* the arrows; the
         * only things after them are the empty-position markers, and a marker behind a
         * solid arrow should be behind it. The two GREATER passes still must not write —
         * they are drawing where the arrow is *behind* the ground, and stamping the
         * arrow's depth there would put a hole in the terrain for everything after. */
        gl.depthMask(pass==='front');
        for(const r of this.arrows.ranges){
          const c=this.arrowHover===r.dir? hot : warm;
          // Proportional to the arrow, so the outline keeps its weight at any size.
          /* The occluded body is flat as well as dim. Two reasons and both matter: an
             object seen through a hillside does not show you its shading, and a flat
             fill cannot show the self-overlap the GREATER passes still have — they
             cannot write depth, so a concave arrow would step there for exactly the
             reason the solid pass used to. */
          if(pass==='rim')          draw(r, c,      (r.size||220)*0.05, 1);
          else if(pass==='behind')  draw(r, dim(c), 0, 1);
          else                      draw(r, c,      0, 0);
        }
      }
      gl.depthFunc(gl.LEQUAL);
      gl.uniform1f(ap.u.uSwell,0); gl.uniform1f(ap.u.uFlat,0);
      gl.depthMask(true);
    }

    // Wraithguard: the line overlays - path grids, radii, links, markers (47_wg_overlay.js).
    if(this.drawOverlays) this.drawOverlays(VP);

    /* Round 17i: the orbit pivot while Ctrl is held — three rings at the point the view
       turns around, so the wheel's Ctrl-zoom has something to aim. Radius half of 1.5×
       the brush's smallest ball; the brush is only the reference for the size, not
       wired to it. */
    if(this._ctrlDown && this.nav!=='wasd'){
      const pv=this._pivotGeom();
      const pp=this.progPivot; gl.useProgram(pp.p);
      gl.uniformMatrix4fv(pp.u.uVP,false,VP);
      const c=this.cam;
      gl.uniform3fv(pp.u.uC,[c.tx,c.ty,c.tz]);
      gl.uniform1f(pp.u.uR,this.pivotRadius());
      if(airOn) airOn(pp);
      gl.uniform3fv(pp.u.uFogCol,fogCol); gl.uniform1f(pp.u.uFogK,fogK>0? fogK : 0.00004);
      gl.bindVertexArray(pv.vao);
      gl.enable(gl.BLEND); gl.blendFunc(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA); gl.depthMask(false);
      gl.disable(gl.CULL_FACE);
      /* The colour the Orbit navigation button wears, which since 18cj is the theme's
         `--orbit` rather than the paint's salmon. Through whatever stands in front of it,
         faintly, then solid where it is in view, so the ball is never lost inside a hill
         and still says when it is. */
      const oc=orbitCol();
      gl.disable(gl.DEPTH_TEST);
      gl.uniform4fv(pp.u.uCol,[oc[0],oc[1],oc[2],0.28]);
      gl.drawArrays(gl.LINES,0,pv.n);
      gl.enable(gl.DEPTH_TEST);
      gl.uniform4fv(pp.u.uCol,[oc[0],oc[1],oc[2],0.95]);
      gl.drawArrays(gl.LINES,0,pv.n);
      /* And the marker at the centre, a quarter of the radius (round 17m). Culled, so
         each pixel of it is one fragment rather than a front face blended over a back
         one, and drawn in the same two passes as the rings so it is never lost inside a
         hill either. */
      gl.enable(gl.CULL_FACE);
      if(pv.ballN){
        gl.uniform1f(pp.u.uR,this.pivotRadius()*PIVOT_BALL);
        gl.disable(gl.DEPTH_TEST);
        gl.uniform4fv(pp.u.uCol,[oc[0],oc[1],oc[2],0.28]);
        gl.drawArrays(gl.TRIANGLES,pv.ballAt,pv.ballN);
        gl.enable(gl.DEPTH_TEST);
        gl.uniform4fv(pp.u.uCol,[oc[0],oc[1],oc[2],0.95]);
        gl.drawArrays(gl.TRIANGLES,pv.ballAt,pv.ballN);
      }
      gl.depthMask(true); gl.disable(gl.BLEND);
      gl.bindVertexArray(null);
    }

    if(this.markers && o.markers){
      const f=this.progFlat; gl.useProgram(f.p);
      gl.uniformMatrix4fv(f.u.uVP,false,VP);
      gl.uniform3fv(f.u.uCol,this.markers.color);
      gl.bindVertexArray(this.markers.vao);
      gl.drawElementsInstanced(gl.TRIANGLES,this.markers.count,gl.UNSIGNED_SHORT,0,this.markers.n);
    }

    if(this.missing){
      const f=this.progFlat; gl.useProgram(f.p);
      gl.uniformMatrix4fv(f.u.uVP,false,VP);
      gl.uniform3fv(f.u.uCol,this.missing.color);
      gl.bindVertexArray(this.missing.vao);
      gl.drawElementsInstanced(gl.TRIANGLES,this.missing.count,gl.UNSIGNED_SHORT,0,this.missing.n);
    }
    gl.bindVertexArray(null);
    // The MGE water drew the frame offscreen; this is where it reaches the canvas.
    this._gpuMark('present');
    if(this.endWaterFrame) this.endWaterFrame();
    this._gpuMark(null);
    if(this._gpuT) this._gpuT.active=false;
  }
  /** Stops drawing the grass, and keeps the GPU objects for the next scatter.
   *
   *  What is thrown away here is the *list*, not the buffers: a rescatter asks for the
   *  same meshes with new instance streams a moment later, and re-wiring a vertex array
   *  for that is work with nothing to show for it. `dropBatchGpu` is the one that really
   *  frees, and it is called when the meshes themselves go — a different install, or a
   *  cell whose meshes are no longer wanted. */
  disposeBatches(){
    this.batches=[];
  }
  /** Really frees every held vertex array. The mesh cache going is what calls for it. */
  dropBatchGpu(){
    const gl=this.gl;
    for(const slot of this._batchGpu.values())
      for(const held of slot.values()){
        try{ gl.deleteVertexArray(held.vao); }catch(_){ }
        for(const b of (held.bufs||[])){ try{ gl.deleteBuffer(b); }catch(_){ } }
        // Round 18cs: the instance texture, once its last holder lets go.
        if(held.it && --held.it.refs<=0){ held.it.dead=true; try{ gl.deleteTexture(held.it.tex); }catch(_){ } }
        held.it=null;
      }
    this._batchGpu=new Map();
    this.batches=[];
    this.dirty=true;
  }
}
