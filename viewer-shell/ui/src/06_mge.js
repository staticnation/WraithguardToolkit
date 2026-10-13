/* Gardenfell, 06_mge.js. Copyright (c) 2026 Robin Hjelte. GNU General Public License, version 2
   (see page-source/LICENSE): this file carries code ported from MGE XE, which is GPL-2.0. */

/* =====================================================================================
   What the renderer took from MGE XE, in one place - round 18gl.

   06_gl.js is Robin's and MIT; these are the pieces of it that were ported from MGE XE
   (https://github.com/Hrnchamd/MGE-XE, with G7's fork), and so carry MGE XE's licence,
   the GPL version 2. Each is a string of GLSL that 06_gl.js interpolates into its own
   shaders at the place the code used to stand, or a JavaScript function it calls, so the
   shader text the browser compiles is the text it compiled before the split, and the
   pictures are the same pictures. This file is listed before 06_gl.js in build.js,
   because 06_gl.js reads these constants while its own are being defined.

   What is here, and where it came from:
   - the grass wind, XE Mod Grass.fx's grassDisplacement (MGE_GRASS_WIND_GLSL);
   - the grass cutout and wrap lighting, XE Mod Grass.fx (MGE_GRASS_FS_GLSL);
   - the static tonemap of XE FixedFuncEmu.fx (MGE_TONEMAP_GLSL);
   - the atmosphere, XE Common.fx's fogColourScatter, fogColour and fogColourSky
     (MGE_SCATTER_GLSL, MGE_SKY_GLSL), with the per-frame sun-altitude terms
     (mgeSunAltitude);
   - the sun's shadow receiver, XE Mod Shadow.fx and XE Mod Shadow Data.fx, with the
     core-mod constants verbatim (mgeShadowGLSL);
   - the point-light decode of ffeshader.cpp's decodeMorrowindPointLight (mgeDecode).
   The uniforms these read are declared in 06_gl.js beside the shaders that use them; a
   list of names is interface, not expression, and stays with the MIT file.
   ===================================================================================== */

/* The wind - round 12 item 6, redone in round 14 after MGE XE's own grass shader
   (XE Mod Grass.fx, grassDisplacement). The vertex shader in 06_gl.js says what the
   uniforms mean; this is the function. (No backticks in here.) */
const MGE_GRASS_WIND_GLSL=`vec2 grassDisplacement(vec3 world, float h, vec2 windVec){
  float v=length(windVec);
  vec2 displace=2.0*windVec+0.1;
  float gtime=uTime*0.5;
  float ph=world.x+world.y+world.z+gtime;
  vec2 harmonics=vec2(abs((0.5+0.03*v)*sin(-2.0*3.14*ph)+16.0),
                      abs((1.0+0.044*v)*sin(-3.0*3.14*ph)+8.0));
  return clamp(0.02*h,0.0,1.0)*harmonics*displace;
}`;

const MGE_GRASS_FS_GLSL=`/* The grass pass, MGE XE's way (XE Mod Grass.fx; 06_gl.js's FS_GRASS calls these when uGrassLit
   is 1). The cutout is a fixed one: discard below 64/255, and the rest turned into coverage,
   sat(1.3 * (a - 128/255) + 128/255), for the multisample to soften the edge with. The light
   is wrap lighting: the normal already turned to face the eye, then
   lambert = pow(sat((l + w) / (1 + w)), N) * (N + 1) / (2 (1 + w)) + max(0, -l) * back
   with w 0.6, N 1.5 and a backlight of 0.4, so a blade edge-on to the sun still reads and
   a blade the sun is behind glows a little through. (No backticks in here.) */
bool mgeGrassCut(float a){
  return a < 64.0/255.0;
}
float mgeGrassCoverage(float a){
  return clamp(1.3*(a-128.0/255.0)+128.0/255.0, 0.0, 1.0);
}
float mgeGrassLambert(vec3 n, vec3 sun){
  float l=dot(n,sun);
  const float w=0.6, N=1.5;
  return pow(clamp((l+w)/(1.0+w),0.0,1.0),N)*(N+1.0)/(2.0*(1.0+w)) + max(0.0,-l)*0.4;
}`;

/* The static tonemap, inside LIGHT_GLSL in 06_gl.js. (No backticks in here.) */
const MGE_TONEMAP_GLSL=`/* MGE XE's static tonemap (XE FixedFuncEmu.fx): "curve maps 0 -> 0, 1.0 -> 0.84, up to
   2.2 -> 1.0". What its per-pixel lighting does instead of clamping the light sum. */
vec3 gdnTonemap(vec3 c){
  c = clamp(c, 0.0, 2.2);
  return (((0.0548303*c - 0.189786)*c - 0.154732)*c + 1.12969)*c;
}`;

/* The atmosphere, inside SKY_GLSL in 06_gl.js: XE Common.fx's fogColourScatter, and its
   fogColour for the eye in the air (the under-water half is 06_gl.js's own, built around
   this). The uniforms are 06_gl.js's. (No backticks in here.) */
const MGE_SCATTER_GLSL=`vec4 fogColourScatter(vec3 dir, float fogdist, float fog, vec3 skyColDir){
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
}`;

/* The sky, after the fog functions in SKY_GLSL. (No backticks in here.) */
const MGE_SKY_GLSL=`/* The sky: MGE's fogColourSky. Under the water the horizon is the water's colour and the
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
}`;

/* MGE's three sun-altitude terms, worked out once a frame and handed to the shaders as
   uSunAlt (XE Common.fx, the sunalt / sunaltdot / sunaltslope block). */
function mgeSunAltitude(sp){
  const sunalt=Math.pow(1+sp[2],10);
  return [2.8+4.3/sunalt, Math.max(0,Math.min(1,1-Math.pow(2,-1.9*sunalt))),
          Math.max(0,Math.min(1,Math.exp(-4*sp[2])))*Math.max(0,Math.min(1,sunalt))];
}

/* The sun's shadow receiver, inside SKY_GLSL in 06_gl.js after the shadow uniforms: MGE
   XE's own (XE Mod Shadow.fx + XE Mod Shadow Data.fx, G7 fork), at its numbers, with the
   cascade count and the blend band 06_gl.js chose (SHADOW_MAX_CASCADES, SHADOW_BLEND)
   handed in. 06_gl.js's comment above its shadow uniforms says what the exponential map
   is and where OpenMW differs. (No backticks in here.) */
function mgeShadowGLSL(cascades, blend){
  return `/* MGE's core-mod constants, verbatim - these are what decide how dark a shadow is, and
   round 18eg left every one of them alone while it lengthened the cascades. */
const float GDN_ESM_C = 60.0;
const float GDN_ESM_BIAS = 2e-3 * GDN_ESM_C;
const float GDN_ESM_SCALE = 32768.0;
const float GDN_SHADE = 0.4;
const vec3  GDN_SHADECOLOR = vec3(1.0, 0.97, 0.81);
const int   GDN_CASCADES = ${cascades};
const float GDN_BLEND = ${blend.toFixed(3)};
/* How much of the sun one cascade says this point has lost. The stored depth is linear
   over the cascade's own 16,384 units - the same range in every one of them, so the
   exponential means the same thing however far out the box is - and the exponent is what
   softens the edge. */
float gdnShadowOne(vec3 c, int layer){
  vec2 uv = vec2((float(layer) + 0.5 * c.x + 0.5) / float(uShadowN), 0.5 * c.y + 0.5);
  float dz = texture(uShadowTex, uv).r / GDN_ESM_SCALE - (0.5 * c.z + 0.5);
  /* 18fc: dz is a share of this cascade's own window; the exponent grows with the
     window so the same distance in units gives the same shade. The bias is a distance
     too (2e-3 of MGE's window, 33 units) and so stays as it is. */
  return 1.0 - clamp(exp(GDN_ESM_C * uShadowZ[layer] * dz + GDN_ESM_BIAS), 0.0, 1.0);
}
/* This point in one cascade's clip space. */
vec3 gdnShadowAt(vec3 w, int layer){
  vec4 sp = uShadowVP[layer] * vec4(w, 1.0);
  return sp.xyz / sp.w;
}
/* The nearest cascade that holds this point, blended into the next across its outer band
   (18eg item 3). A margin of four texels keeps the blur kernel from reading the cascade
   next door in the atlas; the band is well inside that.
   
   Two lookups in the band and one everywhere else. Before this the switch was a hard one
   and it showed as a line on the ground where a fine cascade gave way to a coarse one. */
float gdnShadowLoss(vec3 w){
  float m = 1.0 - 8.0 * uShadowP.x;
  for(int i = 0; i < GDN_CASCADES; i++){
    if(i >= uShadowN) break;
    vec3 c = gdnShadowAt(w, i);
    bool last = (i == uShadowN - 1);
    float lim = last ? 1.0 : m;
    if(abs(c.x) > lim || abs(c.y) > lim || abs(c.z) > 1.0) continue;
    float loss = gdnShadowOne(c, i);
    if(last) return loss;
    /* How far into this cascade's outer band the point is: nought inside, one at the
       margin. Past that the next cascade is the one that answers. */
    float t = clamp((max(abs(c.x), abs(c.y)) - (m - GDN_BLEND)) / GDN_BLEND, 0.0, 1.0);
    if(t > 0.0){
      vec3 c2 = gdnShadowAt(w, i + 1);
      float lim2 = (i + 1 == uShadowN - 1) ? 1.0 : m;
      if(abs(c2.x) <= lim2 && abs(c2.y) <= lim2 && abs(c2.z) <= 1.0)
        loss = mix(loss, gdnShadowOne(c2, i + 1), t);
    }
    return loss;
  }
  return 0.0;
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
  float v = gdnShadowLoss(w) * light;
  if(v <= 0.0) return c;
  /* Out with the fog, by its square, as MGE fades it - and four times as fast under the
     water, where it also clamps the fade up (isAboveSeaLevel). Wraithguard: by the fog
     MGE fades them by, fogMWScalar - Morrowind's own linear near fog, over the pair MGE's
     adjustFog fits to the weather (uShadowFog) - not the distant land's exponential fog,
     which reaches much further and let shadows run on past where the game's stop. */
  float sd = length(w - uEye);
  float att = clamp((uShadowFog.y - sd) / max(1.0, uShadowFog.y - uShadowFog.x), 0.0, 1.0);
  att *= att;
  v *= uUnder == 1 ? clamp(4.0 * att, 0.0, 1.0) : att;
  /* The end of the world (18fc): the outermost box's own edge, and the last of its depth
     window, each over its outer eighth so nothing ends on a line - and a round fade by
     distance from the eye over the box's outer quarter, so what the fog has not already
     taken goes out in a circle rather than at the corners of a square. Under MGE's fog
     the box reaches a tenth past where the fog has taken everything, so this is only ever
     seen with the fog off. */
  vec4 fp = uShadowVP[uShadowN - 1] * vec4(w, 1.0);
  vec3 fade = clamp(8.0 * (1.0 - abs(fp.xyz / fp.w)), 0.0, 1.0);
  v *= fade.x * fade.y * fade.z;
  if(uShadowFar > 0.0) v *= 1.0 - smoothstep(0.75 * uShadowFar, 0.95 * uShadowFar, length(w - uEye));
  return c * (1.0 - v * GDN_SHADECOLOR);
}`;
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

/* Wraithguard: MGE XE's ordered dither over the sky's gradient (XE Mod Sky.fx, SkyPS:
   ditherSky[vpos.x % 4][vpos.y % 4]), so a dusk or night sky does not band into steps in an
   8-bit frame. Its numbers, a few thousandths either way. 06_gl.js's sky pass interpolates
   it right after the sky colour `c` is worked out. */
const MGE_SKY_DITHER_GLSL=`  { const float DITHER_SKY[16]=float[16](0.001176, 0.001961, -0.001176, -0.001699, -0.000654, -0.000915, 0.000392, 0.000131,
                                         -0.000131, -0.001961, 0.000654, 0.000915, 0.001699, 0.001438, -0.000392, -0.001438);
    ivec2 px=ivec2(gl_FragCoord.xy) % 4;
    c+=DITHER_SKY[px.x*4+px.y]; }`;
