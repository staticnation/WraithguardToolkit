/* =====================================================================================
   The water, MGE XE's way — round 15 items 8, 9 and 10.

   Robin: "Make a water shader that is similar to how the MGE XE one works", a skirt
   under it that sits behind the terrain's, and no z-fighting at the shore. `XE Mod
   Water.fx` (G7's branch, 0.16.0) is the model, and this is a port of it fed by what the
   page already has:

   - The scene is drawn twice. Once from under the water, looking up, with nothing beneath
     the surface and no grass — the reflection, into a half-size target. Once for real,
     into a multisampled target that is then resolved into a colour texture and a depth
     texture the water reads: the colour is the refraction, the depth says how deep the
     water is under each pixel (MGE's `sampDepth`), which is what makes the shoreline
     soft and the shallows show the bottom.
   - The waves are MGE's own: `textures\MGE\water_NRM.dds`, a 256×256×32 volume of
     normals that MGE ships with the game and animates by sliding through it in time
     (`getFinalWaterNormal`). Wraithguard: an install without it - every OpenMW install,
     so every MoMW setup - gets the copy the viewer ships (ui/assets, GPL-2.0, see
     `bundledWaterVolume`); only if that cannot be read either does it fall back to sines.
   - Then the surface: refraction bent by the normal and faded to a depth colour with
     depth (`depthscale`, `shorefactor`), reflection bent the other way and faded into the
     haze, Fresnel between them, the sun's glint (`pow(vdotr,170)`), the shoreline eased
     into the refraction so the water meets the sand rather than cutting it. The numbers
     are MGE's, in its units, because the units are the game's.
   - The skirt (item 9, reworked in round 16): the water's edge dropped as four walls to
     the slab's base, in the water's depth colour and translucent. Round 15 put it a unit
     and a half *outside* the slab and Robin saw it in front of the terrain — of course:
     from outside the cell the outer wall is the nearer one, whichever is drawn first.
     It now stands four units *inside* the slab's walls, so the slab always wins the depth
     test and the skirt only shows where the slab is missing, and its sheet is cut
     exactly to the slab's box (an overhang drew as a line along the top of the wall).
     With the Waves switch off the surface is flat and the walls meet it exactly; with
     it on (MGE's dynamic ripples, VS_WATER_MGE) the height eases to nothing over the
     last 256 units before the edge, so they still meet.
   - The sunshafts (round 16): `Sunshafts.fx`, ported below the water — see drawSunshafts.
   - The shore (item 10): the surface is pulled a hair towards the eye with a polygon
     offset, and the depth-based shore fade hides what is left. The plain sheet used to
     fight the sand pixel for pixel wherever the land crossed zero.

   Everything here is preview: the plugin knows nothing of water. Off (`opts.mgeWater`),
   06_gl.js draws the translucent sheet it always did.
   ===================================================================================== */

/* The surface's vertices. With the waves off (`uWaves` 0) the sheet and the skirt are
   drawn where they are, flat, as MGE draws its water without dynamic ripples.

   With them on - MGE's DYNAMIC_RIPPLES, the second WaterVS in XE Mod Water.fx - the sheet
   is MGE's radial mesh (distantinit.cpp initWater: 150 spokes, 120 rings, dense near the
   eye, the last ring past the horizon), centred on the eye and cut to the loaded water
   (`uBox`), and every vertex is raised by the wave volume's height (its alpha), close waves
   at 1104 units and far ones at 3900, times the install's `wave_height`, faded out within
   200 units of the eye and past 6400. The reflection is read from the point pushed back
   under the crest (`screenposclamp`), as MGE does, so a crest never reflects what is below
   the water. Ours: the height eases to nothing over the last 256 units before the edge of
   the loaded water, where the skirt meets the sheet. */
const VS_WATER_MGE=`#version 300 es
precision highp float;
precision highp sampler3D;
layout(location=0) in vec3 aPos;
uniform mat4 uVP, uReflVP;
uniform int   uWaves;          // 1: aPos is the radial wave mesh, relative to the eye
uniform vec3  uEyeW;
uniform vec4  uBox;            // the loaded water: x0, y0, x1, y1
uniform float uWaterZ, uWaveH;
uniform float uTime;
uniform sampler3D uWater3d;
uniform mediump int uHas3d;   // mediump: the fragment shader's default for int, and a shared uniform must match
out vec3 vW; out vec4 vRefl; out float vClipW;
/* A height 0..1 at a point, when the install has no wave volume: three sines. */
float sineHeight(vec2 p, float t){
  return 0.5 + 0.18*sin(dot(p,vec2(0.011,0.007))+t*1.3) + 0.12*sin(dot(p,vec2(-0.006,0.013))+t*1.7)
             + 0.08*sin(dot(p,vec2(0.017,-0.012))+t*2.3);
}
void main(){
  vec3 w=aPos, wr=aPos;
  if(uWaves==1){
    w=vec3(clamp(uEyeW.xy+aPos.xy, uBox.xy, uBox.zw), uWaterZ);
    float t=fract(0.4*uTime);
    float hNear, hFar;
    if(uHas3d==1){
      hNear=textureLod(uWater3d, vec3(w.xy/1104.0, t), 0.0).a;
      hFar =textureLod(uWater3d, vec3(w.xy/3900.0, t), 0.0).a;
    }else{
      hNear=sineHeight(w.xy, uTime);
      hFar =sineHeight(w.xy*0.283, uTime);
    }
    float dist=length(uEyeW-w);
    float edge=min(min(w.x-uBox.x, uBox.z-w.x), min(w.y-uBox.y, uBox.w-w.y));
    float add=uWaveH*(mix(hNear,hFar,clamp(dist/8000.0,0.0,1.0))-0.5)
             *clamp(1.0-dist/6400.0,0.0,1.0)*clamp(dist/200.0,0.0,1.0)
             *clamp(edge/256.0,0.0,1.0);
    w.z+=add;
    wr=w-vec3(0.0,0.0,abs(add));
  }
  vW=w;
  vec4 p=uVP*vec4(w,1.0);
  vClipW=p.w;
  vRefl=uReflVP*vec4(wr,1.0);
  gl_Position=p;
}`;

const FS_WATER_MGE=`#version 300 es
precision highp float;
precision highp sampler3D;
in vec3 vW; in vec4 vRefl; in float vClipW;
uniform sampler2D uSceneCol;   // the frame so far, resolved
/* and its depth. highp: a sampler2D is lowp unless it says otherwise (GLSL ES), and Linux's
   drivers mean it - a depth of 0.9999 read at half precision is 1.0, so every distance read
   as the same two values and the water drew in bands (Windows' Direct3D ignores it). */
uniform highp sampler2D uSceneDepth;
uniform sampler2D uReflCol;    // the scene from under the water
uniform sampler3D uWater3d;    // MGE's water_NRM.dds
uniform int   uHas3d;
uniform vec2  uRcpRes;
uniform vec2  uNearFar;
uniform vec3  uViewF;          // the camera's forward, for MGE's depth slant
uniform float uTime, uWindLen, uSunVis;
uniform int   uReflBlur;       // the install's blur_reflections: MGE's FILTER_WATER_REFLECTION
uniform sampler2D uRain;       // Wraithguard: MGE's rain/snow ripple simulation (43_wg_rain.js)
uniform int   uRainOn;
uniform vec3  uFogCol; uniform float uFogK;   // the plain fog, when the sky is off
uniform int   uBelow;          // 18dt: the eye is under the water (06_gl.js underwaterAt)
/* Wraithguard: MGE XE's water, with the shore surf and shallows caustics of "OpenMW Water
   Shader Enhanced (OWSE)" by Vegetto (Nexus Mods, Morrowind mod 56186), ported from its water.frag with
   its author's permission ("Feel free to use this shader as a resource for your own
   projects"). Only the author's own foam and caustics functions are taken - not the
   OpenMW water shader they sit in, and not the fog (which the permission excludes).
   uTintHue/uTintAmt lay a colour over the water: its hue in degrees, and 0 (fully
   transparent - MGE XE's own water) to 1 (opaque). */
uniform float uTintHue, uTintAmt;
uniform int   uSewers;         // the sewer waves (below), on or off
${SKY_GLSL}
out vec4 o;

/* ---- Shore foam and caustics, from OWSE by Vegetto (mod 56186) ----
   OpenMW's 2D water normal map is sampled there; here it is MGE's water volume at its
   first slice, or the sines when the install has none. */
const float FOAM_DEPTH_THRESHOLD = 60.0;
const float FOAM_DEPTH_FALLOFF = 10.9;
const vec4  baseFoamColor = vec4(0.98, 0.98, 1.0, 0.4);
const float FOAM_EDGE_THICKNESS = 2.5;
const float FOAM_WAVE_SPEED = 0.0001;
const float FOAM_WAVE_SCALE = 35.0;
const float FOAM_INTENSITY = 0.35;
const float FOAM_DETAIL_SCALE = 3.8;
const float FOAM_DETAIL_INTENSITY = 0.4;
const float FOAM_SPREAD = 4.0;
const float FOAM_SHORE_BLEND = 0.45;
const float FOAM_TRANSPARENCY_FACTOR = 0.75;
const float FOAM_SHORELINE_SPEED = 0.01;
const float FOAM_OSCILLATION_AMOUNT = 0.003;
const float FOAM_TIDE_SPEED = 0.15;
const float FOAM_TIDE_SCALE = 15.0;
const float FOAM_TIDE_STRENGTH = 0.2;
const float FOAM_PATTERN_DEPTH_THRESHOLD = 45.0;
const float FOAM_PATTERN_BIAS = 0.6;
const float FOAM_PATTERN_SCALE_FACTOR = 0.7;
const float FOAM_PATTERN_INTENSITY_FACTOR = 0.7;
const float FOAM_SHORE_BIAS = 1.0;
const float FOAM_HEIGHT_SCALE = 1.8;
const float FOAM_VOLUME_INTENSITY = 0.7;
const vec2  FOAM_WIND_DIR = vec2(0.5, -0.8);
const vec2  SHORE_DIRECTION = vec2(0.3939193, 0.9191450);   // normalize(0.3, 0.7)
vec2 sineNormal(vec2 p, float t);
float linearDepth(float d);
vec3 nmap(vec2 uv){
  if(uHas3d==1){
    vec2 rg=texture(uWater3d, vec3(fract(uv), 0.0)).rg;
    vec2 n=rg*2.0-1.0;
    return vec3(rg, sqrt(max(0.0,1.0-dot(n,n)))*0.5+0.5);
  }
  vec2 rg=sineNormal(uv*900.0, 0.0);
  vec2 n=rg*2.0-1.0;
  return vec3(rg, sqrt(max(0.0,1.0-dot(n,n)))*0.5+0.5);
}
float foamNoise(vec2 uv, float time, float depthFactor){
  float depthScale = mix(1.0, FOAM_PATTERN_SCALE_FACTOR, 1.0 - depthFactor);
  float tideOffsetX = sin(time * FOAM_TIDE_SPEED * 0.7 + uv.y * 0.05) * FOAM_TIDE_STRENGTH;
  float tideOffsetY = cos(time * FOAM_TIDE_SPEED * 0.5 + uv.x * 0.03) * FOAM_TIDE_STRENGTH * 0.7;
  vec2 foamUVBase = uv * FOAM_WAVE_SCALE * depthScale + FOAM_WIND_DIR * time * FOAM_WAVE_SPEED;
  vec2 tideMovement = vec2(tideOffsetX, tideOffsetY);
  float oscPhase = sin(time * 3.0);
  float biasedOscX = oscPhase * (1.0 - FOAM_SHORE_BIAS * max(0.0, sign(oscPhase)));
  float biasedOscY = cos(time * 2.0) * (1.0 - FOAM_SHORE_BIAS * max(0.0, sign(cos(time * 0.5))));
  vec2 oscillation = SHORE_DIRECTION * FOAM_OSCILLATION_AMOUNT * vec2(biasedOscX, biasedOscY);
  vec2 foamUV = foamUVBase + oscillation + tideMovement;
  float noiseContribution = depthFactor * depthFactor;
  float noise = nmap(foamUV * 0.04).r * 0.5 * mix(1.0, 0.2, 1.0 - noiseContribution);
  noise += nmap(foamUV * 0.12 + vec2(time * 0.015, time * -0.01)).r * 0.35 * mix(1.0, 0.1, 1.0 - noiseContribution);
  if(depthFactor > 0.5) noise += nmap(foamUV * 0.25 + vec2(-time * 0.0075, time * 0.005)).r * 0.25 * min(1.0, (depthFactor - 0.5) * 2.0);
  if(depthFactor > 0.8) noise += nmap(foamUV * 0.7 + vec2(time * 0.004, -time * 0.0075)).b * 0.15 * min(1.0, (depthFactor - 0.8) * 5.0);
  return noise * 0.35 * mix(FOAM_PATTERN_INTENSITY_FACTOR, 1.0, depthFactor);
}
float foamDetailNoise(vec2 uv, float time, float depthFactor){
  float depthScale = mix(0.5, 1.0, depthFactor);
  float tideDetailOffsetX = sin(time * FOAM_TIDE_SPEED * 0.9 + uv.y * 0.04) * FOAM_TIDE_STRENGTH * 0.8;
  float tideDetailOffsetY = cos(time * FOAM_TIDE_SPEED * 0.6 + uv.x * 0.025) * FOAM_TIDE_STRENGTH * 0.6;
  vec2 detailUVBase = uv * FOAM_WAVE_SCALE * FOAM_DETAIL_SCALE * depthScale + FOAM_WIND_DIR * time * FOAM_WAVE_SPEED * 1.2;
  float oscPhaseDetail = sin(time * 2.4);
  float biasedOscDetailX = oscPhaseDetail * (1.0 - FOAM_SHORE_BIAS * max(0.0, sign(oscPhaseDetail)));
  float biasedOscDetailY = cos(time * 3.6) * (1.0 - FOAM_SHORE_BIAS * max(0.0, sign(cos(time * 0.9))));
  vec2 oscillation = SHORE_DIRECTION * FOAM_OSCILLATION_AMOUNT * 0.8 * vec2(biasedOscDetailX, biasedOscDetailY);
  vec2 detailUV = detailUVBase + oscillation + vec2(tideDetailOffsetX, tideDetailOffsetY);
  float detail = 0.0;
  if(depthFactor > 0.4){
    float detailMix = smoothstep(0.4, 0.8, depthFactor);
    detail += nmap(detailUV * 0.15 - vec2(time * 0.004, 0.0)).b * 0.6 * detailMix;
  }
  if(depthFactor > 0.6){
    float detailMix = smoothstep(0.6, 0.9, depthFactor);
    detail += nmap(detailUV * 0.4 + vec2(0.0, time * 0.0075)).b * 0.4 * detailMix;
    detail += nmap(detailUV * 0.25 + vec2(time * -0.005, time * 0.01)).r * 0.25 * detailMix;
  }
  return detail * 0.5 * depthFactor;
}
float calculatePatternDepthFactor(float depth){
  float depthFactor = 1.0 - smoothstep(0.0, FOAM_PATTERN_DEPTH_THRESHOLD, depth);
  return clamp(pow(depthFactor, FOAM_PATTERN_BIAS), 0.0, 1.0);
}
float shoreFadeFunction(float depth, float waveHeight){
  float baseTransition = pow(smoothstep(0.0, FOAM_DEPTH_THRESHOLD, depth), FOAM_DEPTH_FALLOFF);
  float waveInfluence = mix(0.7, 1.3, waveHeight);
  float shoreBias = smoothstep(0.9, 1.0, waveHeight);
  return clamp(baseTransition * waveInfluence * shoreBias, 0.0, 1.0);
}
float calculateFoamHeight(vec2 uv, float time, float depthFactor, vec3 waterNormal){
  float baseHeight = nmap(uv * 0.05 + time * 0.007).r * 0.5;
  baseHeight += nmap(uv * 0.15 - time * 0.005).g * 0.3;
  baseHeight += nmap(uv * 0.3 + vec2(time * 0.012, -time * 0.009)).b * 0.2;
  float dotNormal = pow(max(0.0, dot(normalize(vec3(waterNormal.xy * 1.2, waterNormal.z)), vec3(0.0, 0.0, 1.0))), 1.5);
  return baseHeight * FOAM_HEIGHT_SCALE * depthFactor * dotNormal;
}
vec3 calculateFoamNormal(vec2 uv, float depthFactor){
  float scale = 0.05;
  float h1 = nmap(uv * scale + vec2(0.001, 0.0)).r, h2 = nmap(uv * scale - vec2(0.001, 0.0)).r;
  float h3 = nmap(uv * scale + vec2(0.0, 0.001)).r, h4 = nmap(uv * scale - vec2(0.0, 0.001)).r;
  vec3 foamNormal = normalize(vec3(h1 - h2, h3 - h4, 0.05));
  return mix(vec3(0.0, 0.0, 1.0), foamNormal, depthFactor * 0.8);
}
float cellDist(vec2 uv){
  vec2 f = fract(uv) - 0.5, i = floor(uv);
  float m = 1.0;
  for(float y=-1.0; y<=1.0; y++) for(float x=-1.0; x<=1.0; x++){
    vec2 nb = vec2(x, y);
    vec2 pt = fract(sin(vec2(dot(i + nb, vec2(127.1, 311.7)), dot(i + nb, vec2(269.5, 183.3)))) * 43758.5453);
    m = min(m, length(nb + pt - f));
  }
  return m;
}
vec3 proceduralCaustics(vec3 wp, float time){
  vec2 uv1 = wp.xy / 40.0 + time * 1.5 * vec2(0.7, 0.3);
  vec2 uv2 = wp.xy / 20.0 + time * 2.3 * vec2(-0.4, 0.6);
  vec2 uv3 = wp.xy / 10.0 + time * 3.1 * vec2(0.2, -0.5);
  vec3 ns1 = nmap(wp.xy / 80.0 + time * 0.08) * 2.0 - 1.0;
  vec3 ns2 = nmap(wp.xy / 50.0 - time * 0.05) * 2.0 - 1.0;
  uv1 += ns1.xy * 0.15; uv2 += ns2.xy * 0.1; uv3 += (ns1.xy + ns2.xy) * 0.05;
  float d1 = cellDist(uv1), d2 = cellDist(uv2);
  float pattern3 = abs(sin(uv3.x * 6.28) * sin(uv3.y * 6.28)) * 0.5;
  float c = (1.0 - d1) * (1.0 - d1) * 0.6 + (1.0 - d2) * (1.0 - d2) * 0.3 + pattern3 * 0.1;
  c = pow(c, 3.0);
  c *= 1.0 + dot(ns1.xy, ns2.xy) * 0.2;
  return mix(vec3(0.6, 0.8, 1.0), vec3(1.0, 1.0, 0.95), c) * c * 1.5;
}
/* ---- end of the mod 56186 port ---- */


float linearDepth(float d){
  float z=d*2.0-1.0;
  return 2.0*uNearFar.x*uNearFar.y/(uNearFar.y+uNearFar.x-z*(uNearFar.y-uNearFar.x));
}
/* The sines that stand in for MGE's volume when the install has none. */
vec2 sineNormal(vec2 p, float t){
  float a=sin(dot(p,vec2(0.011,0.007))+t*0.9)+sin(dot(p,vec2(-0.006,0.013))+t*1.3)*0.7
         +sin(dot(p,vec2(0.021,-0.017))+t*2.1)*0.35;
  float b=sin(dot(p,vec2(0.009,-0.012))+t*1.1)+sin(dot(p,vec2(0.015,0.004))+t*0.7)*0.7
         +sin(dot(p,vec2(-0.019,0.020))+t*1.9)*0.35;
  return vec2(a,b)*0.5+0.5;
}
/* ---- The sewer waves, from "Enhanced Water Shader for MGE XE" by Krokantor (Nexus Mods,
   Morrowind mod 45432, 2.0 Green-Blue): rings spreading from the sewer outlets of Vivec
   and Molag Mar, added to the close wave normals as its getFinalWaterNormal adds them.
   Credits, per its readme: vtastek (peak fix, improved caustics, optimised sewer waves,
   code and help); phal and harnlarnm (original foam code); abot (sewer waves port);
   Hrnchamd (MGE XE). "You can do with this
   shader what you want as long as you give proper credit to the original authors and me."
   Its outlet positions and ring formula are taken as they are. ---- */
const vec2 SEWER_OUTLETS[70] = vec2[70](
  vec2(19160.,-86825.),vec2(46500.,-81930.),vec2(31475.,-104345.),vec2(32165.,-76700.),vec2(19160.,-81900.),vec2(20260.,-80775.),vec2(20300.,-87915.),vec2(24125.,-80775.),
  vec2(24225.,-87915.),vec2(25275.,-86700.),vec2(25275.,-81850.),vec2(25550.,-95230.),vec2(25550.,-91390.),vec2(25550.,-88840.),vec2(25550.,-84940.),vec2(26300.,-81750.),
  vec2(26300.,-77825.),vec2(26630.,-89955.),vec2(26650.,-96365.),vec2(26700.,-83850.),vec2(27185.,-90260.),vec2(27625.,-82800.),vec2(27635.,-76700.),vec2(31180.,-96925.),
  vec2(31200.,-103770.),vec2(31540.,-83850.),vec2(31615.,-96365.),vec2(31765.,-90260.),vec2(31895.,-89955.),vec2(32150.,-82800.),vec2(32675.,-95020.),vec2(32675.,-91785.),
  vec2(32675.,-88690.),vec2(32675.,-85300.),vec2(32975.,-95365.),vec2(32975.,-91190.),vec2(32975.,-89110.),vec2(32975.,-84720.),vec2(33450.,-81635.),vec2(33450.,-77945.),
  vec2(34095.,-96365.),vec2(34120.,-90260.),vec2(34175.,-104345.),vec2(34480.,-83850.),vec2(34585.,-89955.),vec2(34640.,-96925.),vec2(34695.,-103770.),vec2(38500.,-89955.),
  vec2(38965.,-96365.),vec2(38980.,-83850.),vec2(39060.,-90260.),vec2(40100.,-95300.),vec2(40100.,-91395.),vec2(40100.,-88790.),vec2(40100.,-84900.),vec2(40400.,-86900.),
  vec2(40400.,-81955.),vec2(41350.,-87915.),vec2(41475.,-80775.),vec2(45300.,-87915.),vec2(45480.,-80775.),vec2(46500.,-86790.),
  vec2(107090.,-63820.),vec2(114215.,-59905.),vec2(113110.,-64890.),vec2(107090.,-59875.),vec2(108085.,-58800.),vec2(108110.,-64890.),vec2(113170.,-58800.),vec2(114215.,-63780.));
vec2 sewerWaves(vec2 xy, float dist){
  vec2 sew = vec2(0.0);
  if(uSewers==0 || dist>=7200.0 || length(uSunCol)<=0.003) return sew;
  for(int i=0;i<70;i++){
    float d = length(xy - SEWER_OUTLETS[i]);
    sew += vec2(sin(mod(d,128.0)/20.371832716 - uTime*1.2) / max((d*d)/20480.0, 4.0));
  }
  return sew;
}
/* ---- end of the mod 45432 sewer waves ---- */
vec3 waterNormal(vec2 tc1, vec2 tc2, float dist){
  float t=fract(0.4*uTime);   // MGE's W coordinate into the volume
  vec2 farN, closeN;
  if(uHas3d==1){
    farN  =texture(uWater3d, vec3(tc1,t)).rg;
    closeN=texture(uWater3d, vec3(tc2,t)).rg;
  }else{
    farN  =sineNormal(tc1*3900.0, uTime);
    closeN=sineNormal(tc2*527.0,  uTime);
  }
  /* The mod's normals stand at z 0.295 where MGE's stand at 1, so its rings are scaled
     by the same 1/0.295 to lean the surface as far as they do there. */
  closeN += sewerWaves(vW.xy, dist) * 3.39;
  float farK=clamp(dist/8000.0,0.0,1.0);
  vec2 nR=2.0*mix(closeN, farN, farK)-1.0;
  /* MGE's precipitation ripples (DYNAMIC_RIPPLES: close_normal.rg += sampRain.ba, on the
     close texcoords - they tile every 527 units as MGE's do). */
  vec2 rain=vec2(0.0);
  if(uRainOn==1) rain=2.0*(1.0-farK)*textureLod(uRain, tc2, 0.0).ba;
  return normalize(vec3(nR+rain,1.0));
}
/* The reflection at a point. With the install's blur_reflections on, MGE's
   FILTER_WATER_REFLECTION: six taps along a short, mostly horizontal smear whose width grows
   with distance - 0.006 * saturate(0.11 + w/6000) * w in MGE's projected coordinates, which
   is that much of the screen once divided by w - squashed to square pixels. */
vec3 reflectAt(vec2 ruv){
  vec3 c=texture(uReflCol,ruv).rgb;
  if(uReflBlur!=1) return c;
  vec2 r=0.006*clamp(0.11+vClipW/6000.0,0.0,1.0)*vec2(1.0, uRcpRes.y/uRcpRes.x);
  const vec2 K[5]=vec2[5](vec2(0.60,0.10),vec2(0.30,-0.21),vec2(0.96,-0.03),vec2(-0.40,0.06),vec2(-0.70,0.18));
  for(int i=0;i<5;i++) c+=texture(uReflCol, clamp(ruv+r*K[i], vec2(0.001), vec2(0.999))).rgb;
  return c/6.0;
}
/* MGE's fogColourWater is fogColour; without the sky it is the plain fog. */
vec4 waterFog(vec3 dir, float dist, vec3 w){
  if(uScat==1 || uUnder==1) return fogColour(dir,dist);
  float f=clamp(1.0-exp(-uFogK*length(w.xy)),0.0,1.0);
  return vec4(uFogCol*f, 1.0-f);
}
void main(){
  vec3 eyeVec=vW-uEye;
  float dist=length(eyeVec);
  eyeVec/=max(dist,0.001);
  vec2 suv=gl_FragCoord.xy*uRcpRes;

  vec4 fog=waterFog(eyeVec,dist,vW);
  float sunlightFactor=1.0-pow(1.0-uSunVis,2.0);
  vec3 sunColAdjusted=uSunCol*sunlightFactor;
  vec3 depthBaseColor=sunColAdjusted*vec3(0.03,0.04,0.05)+(2.0*uSkyCol+uFogColFar)*vec3(0.075,0.08,0.085);
  vec3 depthColor=fog.a*depthBaseColor+fog.rgb;
  /* The colour over the water, lit as the water is: a mid-bright colour of the chosen
     hue under the sun, the sky and the far fog, then the fog over it as over the rest. */
  vec3 hueC=clamp(abs(mod(uTintHue/60.0+vec3(0.0,4.0,2.0),6.0)-3.0)-1.0, 0.0, 1.0);
  vec3 tintC=mix(vec3(1.0), hueC, 0.7)*0.45*(0.35*sunColAdjusted+1.0*uSkyCol+0.3*uFogColFar);
  tintC=fog.a*tintC+fog.rgb;
  depthColor=mix(depthColor, tintC, uTintAmt);

  vec3 normal=waterNormal(vW.xy/3900.0, vW.xy/527.0, dist);
  if(!gl_FrontFacing) normal=-normal;

  float windFactor=(uWindLen+1.5)/140.0;
  if(uBelow==1){
    /* Round 18dt: the surface from beneath - MGE's UnderwaterPS (XE Mod Water.fx), which
       Robin chose for every install ("Always the MGE shader"). The normal is the surface's
       negated (the facing flip above did that: from below the sheet is its back). The
       refraction is the frame behind the surface - the world above the water, drawn in the
       underwater fog like everything else this frame - bent twice as hard as from above and
       gone into the fog colour within a few hundred units (exp(-dist/500)); the reflection
       is the mirror pass, which looks from the eye's image in the air. The Fresnel is
       MGE's with total internal reflection folded in: pow(1.12 - 0.65 cos, 8), which is
       nothing looking straight up and everything at a grazing angle. The sun refracts
       through as a soft spot, faded by MGE's own exp(-dist/4096). The shore fade and the
       depth colour of the surface seen from above have no part here. */
    float fogU=clamp(exp(-dist/4096.0),0.0,1.0);
    vec2 reffactorU=2.0*(windFactor*dist+0.1)*normal.xy;
    vec2 offU=-2.0*reffactorU.yx/max(vClipW,1.0);
    vec3 refractedU=texture(uSceneCol, clamp(suv+offU, vec2(0.001), vec2(0.999))).rgb;
    refractedU=mix(uUnderCol, refractedU, exp(-dist/500.0));
    vec2 ruvU=vRefl.xy/max(vRefl.w,0.001)*0.5+0.5;
    ruvU-=vec2(2.1*reffactorU.x, -abs(reffactorU.y))/max(vClipW,1.0);
    ruvU=clamp(ruvU, vec2(0.001), vec2(0.999));
    vec3 reflectedU=reflectAt(ruvU);
    float fresnelU=pow(clamp(1.12-0.65*dot(-eyeVec,normal),0.0,1.0),8.0);
    vec3 resultU=mix(refractedU, reflectedU, fresnelU);
    float refractsun=dot(-eyeVec, normalize(-uSunPos+normal));
    resultU+=sunColAdjusted*pow(max(refractsun,0.0),6.0)*fogU;
    o=vec4(resultU,1.0);
    return;
  }
  vec2 reffactor=(windFactor*dist+0.1)*normal.xy;
  vec2 offUv=reffactor.yx/max(vClipW,1.0);

  float depth=max(0.0, linearDepth(texture(uSceneDepth, suv+offUv).r)-vClipW);
  vec3 refracted=depthColor;
  float shorefactor=0.0;
  float wdepth=1e6;   // how far below the surface the bottom is, straight down (the foam's)
  if(depth<4000.0){
    vec2 nuv=clamp(suv+clamp(depth/100.0,0.0,1.0)*offUv, vec2(0.001), vec2(0.999));
    refracted=texture(uSceneCol,nuv).rgb;
    depth=max(0.0, linearDepth(texture(uSceneDepth,nuv).r)-vClipW);
    wdepth=depth/max(0.05, dot(eyeVec,uViewF));   // along the view ray, as OpenMW's realWaterDepth
    depth/=max(0.05, dot(eyeVec,uViewF));
    depth+=300.0*(0.95-normal.z);
    float depthscale=clamp(exp(-depth/800.0),0.0,1.0);
    shorefactor=pow(depthscale,90.0);
    refracted=mix(depthColor, refracted, 0.8*depthscale+0.2*shorefactor);
  }

  vec2 ruv=vRefl.xy/max(vRefl.w,0.001)*0.5+0.5;
  ruv-=vec2(2.1*reffactor.x, -abs(reffactor.y))/max(vClipW,1.0);
  ruv=clamp(ruv, vec2(0.001), vec2(0.999));
  vec3 reflected=reflectAt(ruv);
  reflected=mix(fog.rgb, reflected, fog.a);

  vec3 adjustnormal=mix(vec3(0.0,0.0,0.1), normal, pow(clamp(1.05*fog.a,0.0,1.0),2.0));
  adjustnormal=mix(adjustnormal, vec3(0.0,0.0,1.0), (1.0+eyeVec.z)*(1.0-clamp(1.0/(dist/1000.0+1.0),0.0,1.0)));
  float fresnel=dot(-eyeVec, adjustnormal);
  fresnel=0.02+pow(clamp(0.9988-0.28*fresnel,0.0,1.0),16.0);
  /* The colour: over the bottom seen through the water too, so at opaque it is all
     there is. */
  refracted=mix(refracted, tintC, uTintAmt);
  if(wdepth<FOAM_DEPTH_THRESHOLD*2.5){
    /* Mod 56186's foam and caustics, over the refraction, as its main() does. */
    vec2 UV = vW.xy / (8192.0 * 5.0) * 3.0; UV.y *= -1.0;
    float waterTimer = uTime;
    float sunFade = clamp(sunlightFactor, 0.0, 1.0);
    vec3 sunDir = normalize(uSunPos);
    float realWaterDepth = wdepth;
    float foamFactor = 0.0, foamHeight = 0.0, foamLightScale = 0.0;
    if(realWaterDepth < FOAM_DEPTH_THRESHOLD){
      float waveHeight = (normal.z + 1.0) * 0.5;
      float tideFactor = sin(waterTimer * FOAM_TIDE_SPEED * 0.3 + vW.x * 0.002 * FOAM_TIDE_SCALE + vW.y * 0.003 * FOAM_TIDE_SCALE);
      float tideVariation = sin(vW.x * 0.003 * FOAM_TIDE_SCALE + vW.y * 0.002 * FOAM_TIDE_SCALE) * 0.5 + 0.5;
      float tideStrength = FOAM_TIDE_STRENGTH * (0.8 + tideVariation * 0.4);
      float dynamicFoamSpread = FOAM_SPREAD + tideFactor * tideStrength;
      float localTideOffset = sin(waterTimer * FOAM_TIDE_SPEED * 0.5 + vW.x * 0.005 * FOAM_TIDE_SCALE + vW.y * 0.007 * FOAM_TIDE_SCALE) * tideStrength;
      float depthFactor = 1.0 - smoothstep(0.0, FOAM_DEPTH_THRESHOLD + localTideOffset, realWaterDepth);
      depthFactor = pow(depthFactor, FOAM_DEPTH_FALLOFF) * (dynamicFoamSpread + localTideOffset);
      depthFactor *= max(0.3, abs(dot(normalize(vec3(eyeVec.xy, 0.0) + vec3(1e-5)), normalize(vec3(normal.xy, 0.0) + vec3(1e-5)))));
      float patternDepthFactor = calculatePatternDepthFactor(realWaterDepth);
      float foamPattern = foamNoise(UV, waterTimer, patternDepthFactor);
      float foamDetail = foamDetailNoise(UV, waterTimer * 0.7, patternDepthFactor);
      foamHeight = calculateFoamHeight(UV, waterTimer, patternDepthFactor, normal);
      float tideEdgeVariation = sin(waterTimer * FOAM_TIDE_SPEED * 0.4 + UV.x * 5.0 * FOAM_TIDE_SCALE * 0.1 + UV.y * 3.0 * FOAM_TIDE_SCALE * 0.1) * 0.15 + 1.0;
      float edgeFoam = pow(depthFactor, 1.2) * FOAM_EDGE_THICKNESS * tideEdgeVariation;
      edgeFoam *= mix(0.85, 1.15, sin(waterTimer * 0.3 + UV.x * 4.0 + UV.y * 6.0) * 0.5 + 0.5);
      float waveFoam = max(0.0, (waveHeight - 0.45) * 1.7) * depthFactor * 0.8;
      foamFactor = edgeFoam + waveFoam;
      foamFactor *= mix(0.8, 1.2, foamPattern);
      foamFactor += foamDetail * FOAM_DETAIL_INTENSITY * depthFactor;
      foamFactor *= 1.0 + 0.08 * sin(waterTimer * 0.2 + UV.x * 8.0 + UV.y * 7.0);
      foamFactor *= mix(1.0, shoreFadeFunction(realWaterDepth, waveHeight), FOAM_SHORE_BLEND);
      foamFactor *= 1.0 - smoothstep(FOAM_DEPTH_THRESHOLD * 0.6, FOAM_DEPTH_THRESHOLD, realWaterDepth);
      float depthTransparency = mix(1.0, FOAM_TRANSPARENCY_FACTOR, smoothstep(0.0, FOAM_DEPTH_THRESHOLD * 0.7, realWaterDepth));
      foamFactor = clamp(foamFactor * FOAM_INTENSITY * depthTransparency, 0.0, 1.0);
      foamFactor *= patternDepthFactor;
      foamLightScale = clamp(pow(sunFade, 3.0) * 0.9, 0.0, 0.7);
    }
    vec4 foamC = baseFoamColor;
    foamC.rgb *= sunFade;
    foamC.a *= foamLightScale;
    foamC.a *= foamFactor * mix(0.7, 1.0, smoothstep(FOAM_DEPTH_THRESHOLD * 0.5, 0.0, realWaterDepth));
    foamC.a *= sunFade;
    float foamVolumeFactor = foamHeight * FOAM_VOLUME_INTENSITY * foamFactor;
    vec3 foamVol = mix(foamC.rgb, foamC.rgb * 1.2, foamVolumeFactor) * foamLightScale;
    refracted = mix(refracted, foamVol, foamC.a);
    float causticDepthFactor = pow(1.0 - smoothstep(0.0, FOAM_DEPTH_THRESHOLD * 2.5, realWaterDepth), 5.0);
    float causticI = 0.3 * pow(max(0.1, sunDir.z), 1.2);
    refracted *= 1.0 + proceduralCaustics(vW, waterTimer * 2.0) * causticI * causticDepthFactor;
    reflected = mix(reflected, reflected * 1.15, foamVolumeFactor * fresnel);
  }
  vec3 result=mix(refracted, reflected, fresnel);

  float vdotr=dot(-eyeVec, reflect(-uSunPos, normal));
  vdotr=clamp(1.0025*vdotr,0.0,1.0);
  vec3 spec=sunColAdjusted*(pow(vdotr,170.0)+0.07*pow(vdotr,4.0));
  result+=spec*fog.a;

  result=mix(result, refracted, shorefactor*fog.a);
  o=vec4(result,1.0);
}`;

/* One triangle over the screen carrying a texture: the offscreen frame onto the canvas. */
const VS_BLIT=`#version 300 es
precision highp float;
out vec2 vUV;
void main(){
  vec2 p = gl_VertexID==0? vec2(-1.0,-1.0) : gl_VertexID==1? vec2(3.0,-1.0) : vec2(-1.0,3.0);
  vUV=p*0.5+0.5;
  gl_Position=vec4(p,0.0,1.0);
}`;
const FS_BLIT=`#version 300 es
precision highp float;
in vec2 vUV; uniform sampler2D uTex; out vec4 o;
void main(){ o=vec4(texture(uTex,vUV).rgb,1.0); }`;

/* The sunshafts' five passes - see drawSunshafts. Half-size textures, the sun in
   0..1 screen space, MGE's constants from the .fx: N 25, raysunradius 0.45,
   raystrength 1.4, rayfalloff 1.10, rayfalloffconst 0.13, raysunfalloff 1.4,
   centervis 0.3, sunrayocclude 0.85, brightnessadd 1.1. */
const FS_SHAFT_MASK=`#version 300 es
precision highp float;
in vec2 vUV; uniform highp sampler2D uDepth; uniform vec2 uRcpRes; out vec4 o;
void main(){
  // The sky is where nothing was drawn: the cleared depth. Four taps, as the .fx.
  float d = step(0.99999, texture(uDepth, vUV).r)
          + step(0.99999, texture(uDepth, vUV+vec2(uRcpRes.x,0.0)).r)
          + step(0.99999, texture(uDepth, vUV+vec2(0.0,uRcpRes.y)).r)
          + step(0.99999, texture(uDepth, vUV+uRcpRes).r);
  o=vec4(0.0,0.0,0.0,d*0.25);
}`;
const FS_SHAFT_BLURR=`#version 300 es
precision highp float;
in vec2 vUV; uniform sampler2D uTex; uniform vec2 uRcpRes, uSun; out vec4 o;
void main(){
  vec2 radial=normalize(vUV-uSun)*uRcpRes;
  float a=0.3333*texture(uTex,vUV).a;
  a+=0.2222*texture(uTex,vUV+radial).a + 0.2222*texture(uTex,vUV-radial).a;
  a+=0.1111*texture(uTex,vUV+2.0*radial).a + 0.1111*texture(uTex,vUV-2.0*radial).a;
  o=vec4(0.0,0.0,0.0,a);
}`;
const FS_SHAFT_RAYS=`#version 300 es
precision highp float;
in vec2 vUV; uniform sampler2D uTex; uniform vec2 uSun; uniform float uAspect, uStrength; uniform vec3 uSunCol; out vec4 o;
void main(){
  vec2 screendir=vUV-uSun;
  float screendist=length(screendir*vec2(1.0,uAspect));
  screendir/=max(screendist,1e-4);
  float sunr=min(0.45,screendist);
  float l=0.0;
  for(int i=1;i<=25;i++){
    float sundist=float(i)/25.0*sunr;
    l+=texture(uTex, clamp(uSun+sundist*screendir,0.0,1.0)).a
       * exp(-((screendist-sundist)/(0.13+sundist))*1.10)
       * pow(1.0-clamp(sundist/0.45,0.0,1.0),1.4);
  }
  l*=uStrength/25.0*(screendist/0.45*0.7+0.3);
  vec3 col=vec3(uSunCol.r,0.8*uSunCol.g,0.8*uSunCol.b);
  col*=1.0+1.1*pow(l,3.0);
  o=vec4(col,l);
}`;
const FS_SHAFT_BLURT=`#version 300 es
precision highp float;
in vec2 vUV; uniform sampler2D uTex; uniform vec2 uRcpRes, uSun; out vec4 o;
void main(){
  vec2 t=normalize(vUV-uSun).yx*vec2(uRcpRes.y,-uRcpRes.x);
  vec4 c=0.3333*texture(uTex,vUV);
  c+=0.2222*texture(uTex,vUV+t) + 0.2222*texture(uTex,vUV-t);
  c+=0.1111*texture(uTex,vUV+2.0*t) + 0.1111*texture(uTex,vUV-2.0*t);
  o=c;
}`;
const FS_SHAFT_COMBINE=`#version 300 es
precision highp float;
in vec2 vUV; uniform sampler2D uTex, uRays; out vec4 o;
void main(){
  vec4 ray=texture(uRays,vUV);
  vec3 col=texture(uTex,vUV).rgb;
  col*=clamp(1.0-0.85*ray.a,0.0,1.0);
  col=clamp(col+ray.rgb*ray.a,0.0,1.0);
  o=vec4(col,1.0);
}`;

/* The skirt: the water's depth colour down the walls, darker with depth. */
const FS_WSKIRT=`#version 300 es
precision highp float;
in vec3 vW; in vec4 vRefl; in float vClipW;
uniform float uTop, uBase, uSunVis;
uniform vec3 uFogCol; uniform float uFogK;
${SKY_GLSL}
out vec4 o;
void main(){
  vec3 eyeVec=vW-uEye; float dist=length(eyeVec); eyeVec/=max(dist,0.001);
  vec4 fog = (uScat==1 || uUnder==1) ? fogColour(eyeVec,dist)
           : vec4(uFogCol*clamp(1.0-exp(-uFogK*length(vW.xy)),0.0,1.0), exp(-uFogK*length(vW.xy)));
  vec3 sunColAdjusted=uSunCol*(1.0-pow(1.0-uSunVis,2.0));
  vec3 base=sunColAdjusted*vec3(0.03,0.04,0.05)+(2.0*uSkyCol+uFogColFar)*vec3(0.075,0.08,0.085);
  float down=clamp((uTop-vW.z)/max(1.0,uTop-uBase),0.0,1.0);
  base*=mix(1.0,0.45,down);
  // Translucent (round 16): the slab and whatever stands behind show through.
  o=vec4(fog.a*base+fog.rgb, mix(0.55,0.8,down)*fog.a+(1.0-fog.a));
}`;

Object.assign(Renderer.prototype,{
  /** A 3D texture from a volume the engine decoded (`Kind::Volume`). */
  makeVolume(w,h,d,data){
    const gl=this.gl; const t=gl.createTexture();
    gl.bindTexture(gl.TEXTURE_3D,t);
    gl.pixelStorei(gl.UNPACK_ALIGNMENT,1);
    gl.texImage3D(gl.TEXTURE_3D,0,gl.RGBA8,w,h,d,0,gl.RGBA,gl.UNSIGNED_BYTE,
                  data instanceof Uint8Array? data : new Uint8Array(data));
    gl.texParameteri(gl.TEXTURE_3D,gl.TEXTURE_MIN_FILTER,gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_3D,gl.TEXTURE_MAG_FILTER,gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_3D,gl.TEXTURE_WRAP_S,gl.REPEAT);
    gl.texParameteri(gl.TEXTURE_3D,gl.TEXTURE_WRAP_T,gl.REPEAT);
    gl.texParameteri(gl.TEXTURE_3D,gl.TEXTURE_WRAP_R,gl.REPEAT);
    gl.bindTexture(gl.TEXTURE_3D,null);
    return t;
  },

  /** MGE's wave volume, asked for once; null until it arrives or when the install has none. */
  waterVolume(){
    if(this._wnrm!==undefined) return this._wnrm;
    this._wnrm=null;
    const bundled=()=>this.bundledWaterVolume().then(v=>{ if(v){ this._wnrm=v; this.dirty=true; } });
    if(typeof loadTexture!=='function' || typeof VFS==='undefined' || !VFS.ok()){ bundled(); return null; }
    loadTexture('textures\\MGE\\water_NRM.dds').then(t=>{
      if(t && t.gl && t.is3d){ this._wnrm=t.gl; this.dirty=true; } else bundled();
    }).catch(()=>bundled());
    return null;
  },
  /** MGE XE's wave volume as the viewer ships it (ui/assets/water_NRM.dds, GPL-2.0), for
   *  an install that has none - which is every OpenMW install, so every MoMW setup. Without
   *  it the waves are three sines. The file is MGE's own: an uncompressed 32-bit DDS volume,
   *  256 x 256 x 32 with mips; the top level is read, BGRA to RGBA. Null if it cannot be
   *  fetched (the headless boot test serves only the page). */
  bundledWaterVolume(){
    if(this._wnrmBundled) return this._wnrmBundled;
    this._wnrmBundled=fetch('water_NRM.dds').then(r=>r.ok? r.arrayBuffer() : null).then(buf=>{
      if(!buf || buf.byteLength<128) return null;
      const dv=new DataView(buf);
      if(dv.getUint32(0,true)!==0x20534444) return null;              // 'DDS '
      const h=dv.getUint32(12,true), w=dv.getUint32(16,true), d=Math.max(1,dv.getUint32(24,true));
      const bits=dv.getUint32(88,true), rMask=dv.getUint32(92,true);
      const n=w*h*d*4;
      if(bits!==32 || buf.byteLength<128+n) return null;
      const src=new Uint8Array(buf,128,n), out=new Uint8Array(n);
      const bgra = rMask===0x00ff0000;                                  // A8R8G8B8 in memory is B,G,R,A
      for(let i=0;i<n;i+=4){
        out[i]  =bgra? src[i+2] : src[i];
        out[i+1]=src[i+1];
        out[i+2]=bgra? src[i] : src[i+2];
        out[i+3]=src[i+3];
      }
      return this.makeVolume(w,h,d,out);
    }).catch(()=>null);
    return this._wnrmBundled;
  },
  /** Forgotten with the install: another one may have MGE's file, or not. */
  dropWaterVolume(){ this._wnrm=undefined; },

  /** The offscreen targets at the canvas's size: the multisampled scene, its resolve,
      and the half-size reflection. Rebuilt when the canvas grows or shrinks. */
  _waterTargets(){
    const gl=this.gl, W=this.cv.width, H=this.cv.height;
    const t=this._wt;
    if(t && t.W===W && t.H===H) return t;
    if(t){
      for(const k of ['sceneFB','resolveFB','reflFB','shaftA','shaftB']) try{ gl.deleteFramebuffer(t[k]); }catch(_){ }
      for(const k of ['colRB','depRB','reflDepRB']) try{ gl.deleteRenderbuffer(t[k]); }catch(_){ }
      for(const k of ['colTex','depTex','reflTex','shaftATex','shaftBTex']) try{ gl.deleteTexture(t[k]); }catch(_){ }
    }
    /* Wraithguard: the depth is a 32-bit float, not DEPTH_COMPONENT24. On Linux (WebKitGTK
       over OpenGL, and the Steam Deck's AMD driver, whose hardware has no 24-bit depth of
       its own) MGE's water drew in straight horizontal bands wherever the sea lies almost
       level with the sand: the water and the shore came out at the same depth a band at a
       time - Windows (Direct3D) never showed it. A float depth is what that hardware stores
       natively, and it is the same precision on every driver. The scene, its resolve and
       the reflection all use it; a depth blit needs both ends in the same format. Both
       formats' multisample limits are honoured, since colour and depth must agree. */
    const DEPTH=gl.DEPTH_COMPONENT32F;
    let depthMax=4;
    try{ const s=gl.getInternalformatParameter(gl.RENDERBUFFER,DEPTH,gl.SAMPLES); if(s && s.length) depthMax=s[0]; }catch(_){ }
    const samples=Math.min(4, gl.getParameter(gl.MAX_SAMPLES)||0, depthMax);
    const n={W,H,samples};
    // The scene, multisampled, so the grass keeps its soft edges.
    n.sceneFB=gl.createFramebuffer(); gl.bindFramebuffer(gl.FRAMEBUFFER,n.sceneFB);
    n.colRB=gl.createRenderbuffer(); gl.bindRenderbuffer(gl.RENDERBUFFER,n.colRB);
    if(samples>0) gl.renderbufferStorageMultisample(gl.RENDERBUFFER,samples,gl.RGBA8,W,H);
    else gl.renderbufferStorage(gl.RENDERBUFFER,gl.RGBA8,W,H);
    gl.framebufferRenderbuffer(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.RENDERBUFFER,n.colRB);
    n.depRB=gl.createRenderbuffer(); gl.bindRenderbuffer(gl.RENDERBUFFER,n.depRB);
    if(samples>0) gl.renderbufferStorageMultisample(gl.RENDERBUFFER,samples,DEPTH,W,H);
    else gl.renderbufferStorage(gl.RENDERBUFFER,DEPTH,W,H);
    gl.framebufferRenderbuffer(gl.FRAMEBUFFER,gl.DEPTH_ATTACHMENT,gl.RENDERBUFFER,n.depRB);
    // Its resolve: a colour texture and a depth texture the water reads.
    const tex2=(fmt,ifmt,type,w,h)=>{ const x=gl.createTexture(); gl.bindTexture(gl.TEXTURE_2D,x);
      gl.texImage2D(gl.TEXTURE_2D,0,ifmt,w,h,0,fmt,type,null);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.CLAMP_TO_EDGE);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.CLAMP_TO_EDGE); return x; };
    n.resolveFB=gl.createFramebuffer(); gl.bindFramebuffer(gl.FRAMEBUFFER,n.resolveFB);
    n.colTex=tex2(gl.RGBA,gl.RGBA8,gl.UNSIGNED_BYTE,W,H);
    gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,n.colTex,0);
    n.depTex=tex2(gl.DEPTH_COMPONENT,DEPTH,gl.FLOAT,W,H);
    gl.bindTexture(gl.TEXTURE_2D,n.depTex);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.NEAREST);
    gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.DEPTH_ATTACHMENT,gl.TEXTURE_2D,n.depTex,0);
    // The reflection, half size: a shimmer needs no more.
    n.RW=Math.max(1,W>>1); n.RH=Math.max(1,H>>1);
    n.reflFB=gl.createFramebuffer(); gl.bindFramebuffer(gl.FRAMEBUFFER,n.reflFB);
    n.reflTex=tex2(gl.RGBA,gl.RGBA8,gl.UNSIGNED_BYTE,n.RW,n.RH);
    gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,n.reflTex,0);
    n.reflDepRB=gl.createRenderbuffer(); gl.bindRenderbuffer(gl.RENDERBUFFER,n.reflDepRB);
    gl.renderbufferStorage(gl.RENDERBUFFER,DEPTH,n.RW,n.RH);
    gl.framebufferRenderbuffer(gl.FRAMEBUFFER,gl.DEPTH_ATTACHMENT,gl.RENDERBUFFER,n.reflDepRB);
    n.ok = gl.checkFramebufferStatus(gl.FRAMEBUFFER)===gl.FRAMEBUFFER_COMPLETE;
    // Two half-size ping-pong targets for the sunshafts (round 16).
    n.shaftA=gl.createFramebuffer(); gl.bindFramebuffer(gl.FRAMEBUFFER,n.shaftA);
    n.shaftATex=tex2(gl.RGBA,gl.RGBA8,gl.UNSIGNED_BYTE,n.RW,n.RH);
    gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,n.shaftATex,0);
    n.shaftB=gl.createFramebuffer(); gl.bindFramebuffer(gl.FRAMEBUFFER,n.shaftB);
    n.shaftBTex=tex2(gl.RGBA,gl.RGBA8,gl.UNSIGNED_BYTE,n.RW,n.RH);
    gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,n.shaftBTex,0);
    gl.bindFramebuffer(gl.FRAMEBUFFER,n.sceneFB);
    n.ok = n.ok && gl.checkFramebufferStatus(gl.FRAMEBUFFER)===gl.FRAMEBUFFER_COMPLETE;
    gl.bindFramebuffer(gl.FRAMEBUFFER,n.resolveFB);
    n.ok = n.ok && gl.checkFramebufferStatus(gl.FRAMEBUFFER)===gl.FRAMEBUFFER_COMPLETE;
    gl.bindFramebuffer(gl.FRAMEBUFFER,null);
    gl.bindTexture(gl.TEXTURE_2D,null);
    this._wt=n;
    return n;
  },

  /** The surface and its skirt as one buffer: the sheet over the slab's footprint, then
      the skirt down the sides.
   *
   *  Round 16 hung the skirt four units *inside* the slab's walls and dropped it all the
   *  way to the slab's base, so the slab hid whatever of it stood behind ground. Round
   *  17i cuts it to the shape it is actually for. Robin: "Instead of making the skirt for
   *  water sit at 4 units inside the ground mesh, set it at the same width as the terrain
   *  edges and make the bottom edge of the water skirt go down to where the terrain starts
   *  but not longer. Should be similar to a reverse of what the ground slab skirt is
   *  doing." So it is exactly that reverse: the slab's wall runs from the terrain's edge
   *  *down* to a flat base; this runs from a flat water line *down to the terrain's edge*,
   *  along the same vertices (`slab.edges`), in the same plane. Where the ground stands
   *  above the water the band closes to nothing on its own. The two meet at the terrain
   *  and never overlap, so there is nothing to z-fight and nothing to hide.
   *
   *  Without a slab - no terrain loaded - it falls back to four straight walls dropped a
   *  little way, which is all there is to go on. */
  _waterMesh(){
    const gl=this.gl, w=this.water;
    if(!w) return null;
    const base = this.slab? this.slab.base : w.z-2000;
    const box = this.slab? this.slab.box : [w.centre[0]-w.half, w.centre[1]-w.half, w.centre[0]+w.half, w.centre[1]+w.half];
    const edges = (this.slab && this.slab.edges) || null;
    const key=[w.z,box.join(','),base,this.slab? this.slab.rev : 0].join('|');
    if(this._wm && this._wm.key===key) return this._wm;
    /* Round 18bd (F17): the buffers too. `_slabRev` bumps on every terrain rebuild, and
       deleting a vertex array does not free the buffers it referenced — so a long session
       in a coastal cell accumulated two per rebuild. */
    if(this._wm){ this._dropVao(this._wm); }
    const z=w.z, over=0;
    const pos=[], idx=[];
    const quad=(a,b,cc,d)=>{ const i=pos.length/3; pos.push(...a,...b,...cc,...d); idx.push(i,i+1,i+2, i,i+2,i+3); };
    // The sheet, to the slab's edge exactly: past it, its edge shows as a line on the wall.
    quad([box[0]-over,box[1]-over,z],[box[2]+over,box[1]-over,z],[box[2]+over,box[3]+over,z],[box[0]-over,box[3]+over,z]);
    const sheet=idx.length;
    let zb=Math.min(base, z-1);
    if(edges && edges.length){
      /* The band between the water line and the ground, one strip per edge the slab
         walled. Its bottom is the terrain's own height at each vertex, never above the
         water: a stretch of shore that stands proud of the sea contributes nothing. */
      let low=z;
      for(const pts of edges){
        for(let i=0;i+1<pts.length;i++){
          const a=pts[i], b=pts[i+1];
          const za=Math.min(a[2],z), zbz=Math.min(b[2],z);
          if(za>=z && zbz>=z) continue;
          quad([a[0],a[1],z],[b[0],b[1],z],[b[0],b[1],zbz],[a[0],a[1],za]);
          low=Math.min(low,za,zbz);
        }
      }
      // The depth gradient spans the band that is actually there, not the slab's depth.
      zb=Math.min(low, z-1);
    }else{
      const x0=box[0], x1=box[2], y0=box[1], y1=box[3];
      quad([x0,y0,z],[x1,y0,z],[x1,y0,zb],[x0,y0,zb]);
      quad([x1,y0,z],[x1,y1,z],[x1,y1,zb],[x1,y0,zb]);
      quad([x1,y1,z],[x0,y1,z],[x0,y1,zb],[x1,y1,zb]);
      quad([x0,y1,z],[x0,y0,z],[x0,y0,zb],[x0,y1,zb]);
    }
    const vao=gl.createVertexArray(); gl.bindVertexArray(vao);
    const vb=this._buf(new Float32Array(pos)); gl.enableVertexAttribArray(0); gl.vertexAttribPointer(0,3,gl.FLOAT,false,0,0);
    const ib=gl.createBuffer(); gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER,ib);
    gl.bufferData(gl.ELEMENT_ARRAY_BUFFER,new Uint16Array(idx),gl.STATIC_DRAW);
    gl.bindVertexArray(null);
    this._wm={key,vao,sheet,skirt:idx.length-sheet,base:zb,top:z,box:box.slice(),bufs:[vb,ib]};
    return this._wm;
  },

  /** MGE's dynamic ripples (XE Mod Water.fx's displaced WaterVS), and the rest of MGE XE's
   *  water settings, as Preview controls. Wraithguard is for MoMW's OpenMW setups, where
   *  there is no MGE XE configuration to read, so each is the preview's own, saved in the
   *  profile, starting at MGE XE's default: waves on at wave height 50 (MGE clamps it to
   *  0..250), reflections unblurred, caustics at 50. */
  wavesOn(){ return this.opts.waves!==false && this.waveHeightNow()>0; },
  waveHeightNow(){
    const h=this.opts.waveHeight==null? 50 : +this.opts.waveHeight;
    const base=isFinite(h)? Math.min(250, Math.max(0, h)) : 50;
    return base*this.wowScale('waveHeight');
  },
  /** Wonders of Water (NullCascade, MIT; the Preview's `waterDepth` switch): its multiplier
   *  on MGE's wave height or caustics for the weather now - 0.2 in Clear up to 2 in a
   *  thunderstorm, no waves in a true interior (Sky.wowNow). 1 with the switch off. */
  wowScale(which){
    if(!this.opts.waterDepth || typeof Sky!=='object' || !Sky.wowNow) return 1;
    const m=Sky.wowNow(this.opts.room||null);
    return (m && isFinite(m[which]))? +m[which] : 1;
  },
  reflBlurOn(){ return !!this.opts.reflBlur; },
  /** MGE's radial water mesh with dynamic ripples (distantinit.cpp initWater): a centre
   *  vertex, then 120 rings of 150 spokes whose radius runs 9600 * (0.9 r^3 + 0.1 r) - dense
   *  by the eye, wide further out - and a last ring past the horizon. Relative to the eye;
   *  the vertex shader moves it there and cuts it to the loaded water. Built once. */
  _waveMesh(){
    if(this._wvm) return this._wvm;
    const gl=this.gl, resS=150, resT=120, dS=Math.PI*2/resS;
    const pos=new Float32Array((resS*resT+1)*3);
    let k=3;                                   // vertex 0 is the centre, at 0,0,0
    for(let t=0;t<resT;t++){
      let r=t/resT; r=9600*(0.9*r*r*r+0.1*r);
      if(t+1===resT) r=500000;
      for(let si=0;si<resS;si++){ pos[k++]=r*Math.cos(dS*si); pos[k++]=r*Math.sin(dS*si); pos[k++]=0; }
    }
    const idx=new Uint16Array(3*resS + 6*resS*(resT-1)); let j=0;
    for(let si=0;si<resS;si++){ idx[j++]=0; idx[j++]=1+si; idx[j++]=1+(si+1)%resS; }
    for(let t=1;t<resT;t++){
      for(let si=0;si<resS;si++){
        const tb=1+resS*(t-1), s2=(si+1)%resS;
        idx[j++]=tb+si; idx[j++]=resS+tb+si; idx[j++]=tb+s2;
        idx[j++]=resS+tb+si; idx[j++]=resS+tb+s2; idx[j++]=tb+s2;
      }
    }
    const vao=gl.createVertexArray(); gl.bindVertexArray(vao);
    const vb=this._buf(pos); gl.enableVertexAttribArray(0); gl.vertexAttribPointer(0,3,gl.FLOAT,false,0,0);
    const ib=gl.createBuffer(); gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER,ib);
    gl.bufferData(gl.ELEMENT_ARRAY_BUFFER,idx,gl.STATIC_DRAW);
    gl.bindVertexArray(null);
    this._wvm={vao,count:idx.length,bufs:[vb,ib]};
    return this._wvm;
  },

  /** Whether the frame goes through the offscreen targets: the MGE water needs them
      for its refraction and depth, the sunshafts (round 16) for the depth mask. */
  wantsOffscreen(now,scat){
    const o=this.opts;
    // Round 18a: and the ambient occlusion (29_ssao.js), which reads the frame's depth.
    // Round 18do: and the FXAA pass (33_fxaa.js), which reads the finished frame.
    // Round 18dq: and the outline highlight (34_outline.js), which reads the frame's depth.
    // Round 18dt: and the underwater post pass, which reads the finished frame and its depth.
    // Round 18dy: and the depth of field (36_dof.js), which reads both.
    // Wraithguard: and the bloom (42_wg_bloom.js), which reads the finished frame and its depth.
    return !!((this.water && o.mgeWater!==false) || (o.sunshafts && scat && now) || o.ssao || o.fxaa || (this.wantsOutline && this.wantsOutline()) || this.underNow || o.dof || o.bloom);
  },

  /** The whole frame, offscreen: MGE's water when it is on, the sunshafts after.
      Returns false when it cannot (no targets), and the caller draws the plain frame. */
  drawWaterMGE(drawScene,VP,eye,airOn,fogK,now,scat,P,V){
    const gl=this.gl, o=this.opts, c=this.cam;
    const T=this._waterTargets();
    if(!T.ok) return false;
    if(!this.progWaterMGE){
      this.progWaterMGE=this._prog(VS_WATER_MGE,FS_WATER_MGE);
      this.progWSkirt=this._prog(VS_WATER_MGE,FS_WSKIRT);
    }
    const water=!!(this.water && o.mgeWater!==false);
    const W=T.W, H=T.H;
    // What the sunshafts need to know about this frame, once the scene is resolved.
    this._frame={VP,P,V,eye,now,scat,fogK,sun:this._sunNow||null};   // 18du: and the sun and the plain fog, for the caustics

    if(!water){
      gl.bindFramebuffer(gl.FRAMEBUFFER,T.sceneFB);
      gl.viewport(0,0,W,H);
      const bd=this.frameClear();   // 18dd: the room's fog or the theme's backdrop, as the main frame (18dt: or the water's)
      gl.clearColor(bd[0],bd[1],bd[2],1);
      gl.enable(gl.DEPTH_TEST); gl.depthFunc(gl.LEQUAL);
      gl.clear(gl.COLOR_BUFFER_BIT|gl.DEPTH_BUFFER_BIT);
      gl.disable(gl.CULL_FACE); gl.disable(gl.BLEND);
      // Round 18a: the ambient occlusion over what writes depth, before anything that does
      // not. 18d: worked out from the ground and the objects alone, applied once the grass
      // is on top of them (29_ssao.js).
      this.ssaoDrawn=false;
      if(o.ssao && this.ssaoCompute && !this.underNow){   // 18dt: SSAO HQ.fx is masked off under water (mgeflags 8)
        // Round 18g: the grass is in the depth again - see 29_ssao.js.
        drawScene(VP,eye,{phase:'opaque'});
        this._gpuMark('occlusion');
        if(this.ssaoCompute(T,P,V)) this.ssaoApply(T,this.ssaoFade(now,scat),airOn,fogK,V);
      } else drawScene(VP,eye,{phase:'opaque'});
      // The plain sheet, when there is water but MGE's is off (round 17b: it was left
      // out here, so the sunshafts switch made the sea vanish). Between the two phases,
      // as everywhere (round 17x). Round 18f: the particles under the line go under it.
      drawScene(VP,eye,{phase:'particlesBelow'});
      this.drawPlainWater(VP,airOn,fogK);
      drawScene(VP,eye,{phase:'translucent'});
      this._waterFrame=true;
      return true;
    }
    const wz=this.water.z;
    // Wraithguard: MGE's rain/snow ripples, stepped for this frame (43_wg_rain.js).
    if(this.rainStep) this.rainStep();

    // 1. The reflection: the scene from the eye's mirror image under the water.
    const eyeR=[eye[0],eye[1],2*wz-eye[2]];
    const VR=M4.lookAt(eyeR,[c.tx,c.ty,2*wz-c.tz],[0,0,1]);
    const VPR=M4.mul(P,VR);
    /* Round 18cs: only where the water can show. The reflection is read back through the
       water's fragments alone, and the sheet is drawn under the ground: a load whose land
       stands above the line everywhere never samples it, and the pass is skipped; a river
       through it is a band of the screen, and the pass draws for that band and its
       margin (`_waterRect`). Robin's 49-cell frame drew the whole scene twice. */
    const wr=this._waterRect(VPR);
    this.reflDrawn = wr!==false;
    this.reflRect = wr||null;
    if(wr!==false){
    gl.bindFramebuffer(gl.FRAMEBUFFER,T.reflFB);
    gl.viewport(0,0,T.RW,T.RH);
    // 18dd: what the water reflects past everything, when there is no sky. 18du: from under
    // the water the mirror shows the water's inside and clears to its colour (MGE's
    // renderWaterReflection clears to horizonCol and draws no sky underwater).
    const bd=this.underNow? this.underNow.col : this.backdrop();
    gl.clearColor(bd[0],bd[1],bd[2],1);
    gl.enable(gl.DEPTH_TEST); gl.depthFunc(gl.LEQUAL);
    gl.clear(gl.COLOR_BUFFER_BIT|gl.DEPTH_BUFFER_BIT);
    gl.disable(gl.CULL_FACE); gl.disable(gl.BLEND);
    /* `target` is the pivot's mirror image: the reflected pass aims at it, not at the
       real pivot. Without it the sky drawn into the sea is aimed twice however far the
       pivot stands above the water line off true, and swings whenever the pivot moves
       (round 17m). */
    /* Round 18du: with the eye under the water the mirror keeps what is *under* the line -
       MGE flips its clip plane there (renderwater.cpp: plane *= IsUnderwater ? -1 : 1) -
       which is total internal reflection: the sea floor and the kelp mirrored in the
       surface, not the sky. `clipMode` 2 drops fragments above the line. */
    const belowEye=!!this.underNow;
    /* With dynamic ripples MGE lowers the clip plane by half the wave height (renderwater.cpp:
       plane.d += 0.5 * WaterWaveHeight), so a trough still reflects what stands above it. */
    const wDrop=this.wavesOn()? 0.5*this.waveHeightNow() : 0;
    drawScene(VPR,eyeR,{reflect:true,clipZ:belowEye? wz+0.5 : wz-0.5-wDrop,clipMode:belowEye? 2 : 1,target:[c.tx,c.ty,2*wz-c.tz],rect:wr||null});
    }

    // 2. The scene, for real, multisampled.
    gl.bindFramebuffer(gl.FRAMEBUFFER,T.sceneFB);
    gl.viewport(0,0,W,H);
    { const bd=this.frameClear(); gl.clearColor(bd[0],bd[1],bd[2],1); }   // 18dd; 18dt: the water's colour under it
    gl.clear(gl.COLOR_BUFFER_BIT|gl.DEPTH_BUFFER_BIT);
    gl.disable(gl.CULL_FACE); gl.disable(gl.BLEND);
    // Round 17x: what writes depth. The blended objects and the particles come after
    // the surface, below, so a waterfall standing over the water is drawn over it.
    /* Round 18a: the ambient occlusion onto the opaque scene first (it resolves the depth
       itself), so the colour the water refracts is the darkened one - a creased bottom
       seen through the shallows, not a flat one. 18d: from the ground and the objects,
       applied over the grass (29_ssao.js). */
    this.ssaoDrawn=false;
    if(o.ssao && this.ssaoCompute && !this.underNow){   // 18dt: off under water, as MGE masks it
      // Round 18g: the grass is in the depth again - see 29_ssao.js.
      drawScene(VP,eye,{phase:'opaque'});
      this._gpuMark('occlusion');
      if(this.ssaoCompute(T,P,V)) this.ssaoApply(T,this.ssaoFade(now,scat),airOn,fogK,V);
    } else drawScene(VP,eye,{phase:'opaque'});
    /* Round 18f: the particles under the water line, into the picture the surface will
       refract - a puff that sank is seen through the water, dimmed by its depth, as the
       game shows it, rather than drawn over the surface at full strength. */
    /* Round 18eb: the split is for the eye in the air. Its whole point is to get what
       stands under the line into the picture the surface refracts - and with the eye
       already under the water there is no refraction to get into, the surface is overhead,
       and the split only costs the see-through things their depth writes (`_clearNoZ`),
       which is what makes a crossed pair of cut-out quads blend with itself instead of one
       half hiding the other. Under the water the game's own order is the right one. */
    const split = this.underNow? null : wz;
    drawScene(VP,eye,{phase:'particlesBelow', waterSplit:split});

    // 3. Resolved into textures the water can read.
    this._gpuMark('water');
    /* Wraithguard: the first frame on a set of targets records what the GL said about the
       resolve and the surface (`T.diag`, shown in the viewer's Report) - the water draws
       wrong on Linux (WebKitGTK over OpenGL) and right on Windows (Direct3D), and these are
       the steps where the two differ. */
    const diag=!T.diag;
    if(diag){ for(let i=0;i<8 && gl.getError()!==gl.NO_ERROR;i++){} }
    gl.bindFramebuffer(gl.READ_FRAMEBUFFER,T.sceneFB);
    gl.bindFramebuffer(gl.DRAW_FRAMEBUFFER,T.resolveFB);
    gl.blitFramebuffer(0,0,W,H,0,0,W,H,gl.COLOR_BUFFER_BIT,gl.NEAREST);
    const errCol=diag? gl.getError() : 0;
    gl.blitFramebuffer(0,0,W,H,0,0,W,H,gl.DEPTH_BUFFER_BIT,gl.NEAREST);
    const errDep=diag? gl.getError() : 0;

    /* Round 18du: MGE's caustics over what lies under the water line, into the scene
       before the surface reads it (35_underwater.js `drawCaustics`), and the colour
       resolved again so the refraction shows them. */
    this.causticsDrawn=false;
    if(this.drawCaustics && this.drawCaustics(T,this._frame,T.sceneFB,airOn)){
      gl.bindFramebuffer(gl.READ_FRAMEBUFFER,T.sceneFB);
      gl.bindFramebuffer(gl.DRAW_FRAMEBUFFER,T.resolveFB);
      gl.blitFramebuffer(0,0,W,H,0,0,W,H,gl.COLOR_BUFFER_BIT,gl.NEAREST);
    }

    // 4. The surface and its skirt, back into the multisampled scene.
    gl.bindFramebuffer(gl.FRAMEBUFFER,T.sceneFB);
    const m=this._waterMesh();
    /* The planes the scene was actually drawn with. These two numbers linearise the depth
       buffer the water reads (`uNearFar`, `linZ`), so they have to be the pair `draw()`
       built its projection from or the water reads every depth wrong: the sea's own depth
       colour, how far the refraction bends and how much reflection it mixes in all move.
       Round 17m, Robin: "you can see how the water changes it's reflection and refraction
       when the ball is zoomed further away." It did — this line worked the planes out
       from `cam.dist` while the scene used the view distance. Both are gone now: the
       planes are constants and both passes read the one `nearFar()`. */
    const [near,far]=this.nearFar();
    const fwd=[c.tx-eye[0],c.ty-eye[1],c.tz-eye[2]]; const fl=Math.hypot(...fwd)||1;
    const setCommon=(pr)=>{
      gl.useProgram(pr.p);
      gl.uniformMatrix4fv(pr.u.uVP,false,VP);
      if(pr.u.uReflVP) gl.uniformMatrix4fv(pr.u.uReflVP,false,VPR);
      gl.uniform3fv(pr.u.uFogCol,this.backdrop());   // 18dd (airOn sets it again for the pass)
      gl.uniform1f(pr.u.uFogK,fogK);
      airOn(pr);
      /* The sky's colours even with the atmosphere off - the depth colour is made from
         them - white sun, and the plain fog for a sky. Round 18dd: that "plain fog" is the
         backdrop (a room's fog, or the theme's), the same colour the reflection above was
         cleared to, so the water's stand-in sky is the one it is drawn against. */
      if(!(scat&&now)){
        const bd=this.frameClear();   // 18dv: the water's colour with the eye under it
        if(pr.u.uSkyCol) gl.uniform3fv(pr.u.uSkyCol,bd);
        if(pr.u.uFogColFar) gl.uniform3fv(pr.u.uFogColFar,bd);
        if(pr.u.uSunPos) gl.uniform3fv(pr.u.uSunPos,[0.3,0.2,0.93]);
      }
      gl.uniform1f(pr.u.uSunVis, now? now.sunVis : 1);
      if(pr.u.uWaves) gl.uniform1i(pr.u.uWaves,0);
      if(pr.u.uWater3d) gl.uniform1i(pr.u.uWater3d,9);   // the volume's unit, for every program the vertex shader serves
      if(pr.u.uTime) gl.uniform1f(pr.u.uTime,(performance.now()/1000)%3600);
    };
    const wp=this.progWaterMGE;
    setCommon(wp);
    gl.uniform1i(wp.u.uSceneCol,6); gl.activeTexture(gl.TEXTURE6); gl.bindTexture(gl.TEXTURE_2D,T.colTex); this._texU=null;
    gl.uniform1i(wp.u.uSceneDepth,7); gl.activeTexture(gl.TEXTURE7); gl.bindTexture(gl.TEXTURE_2D,T.depTex);
    gl.uniform1i(wp.u.uReflCol,8); gl.activeTexture(gl.TEXTURE8); gl.bindTexture(gl.TEXTURE_2D,T.reflTex);
    const vol=this.waterVolume();
    gl.uniform1i(wp.u.uWater3d,9); gl.activeTexture(gl.TEXTURE9); gl.bindTexture(gl.TEXTURE_3D,vol||this._vol1||(this._vol1=this.makeVolume(1,1,1,new Uint8Array([128,128,255,128]))));
    gl.uniform1i(wp.u.uHas3d,vol?1:0);
    gl.activeTexture(gl.TEXTURE0);
    gl.uniform2f(wp.u.uRcpRes,1/W,1/H);
    gl.uniform2f(wp.u.uNearFar,near,far);
    gl.uniform3f(wp.u.uViewF,fwd[0]/fl,fwd[1]/fl,fwd[2]/fl);
    gl.uniform1f(wp.u.uTime,(performance.now()/1000)%3600);
    gl.uniform1f(wp.u.uWindLen,this.windNow()*2);
    gl.uniform1i(wp.u.uBelow, this.underNow? 1 : 0);   // 18dt
    // Wraithguard: the colour over the water - its hue, and 0 (none) .. 1 (opaque).
    gl.uniform1f(wp.u.uTintHue, this.opts.waterHue!=null? +this.opts.waterHue : 190);
    gl.uniform1f(wp.u.uTintAmt, Math.min(1, Math.max(0, +this.opts.waterTint||0)));
    gl.uniform1i(wp.u.uSewers, this.opts.sewerWaves===false? 0 : 1);   // on unless switched off    gl.uniform1i(wp.u.uReflBlur, this.reflBlurOn()? 1 : 0);
    // Wraithguard: the rain ripples (43_wg_rain.js), stepped above, on unit 10.
    const rainT=this.rainTexture? this.rainTexture() : null;
    gl.uniform1i(wp.u.uRain,10); gl.activeTexture(gl.TEXTURE10); gl.bindTexture(gl.TEXTURE_2D,rainT); gl.activeTexture(gl.TEXTURE0);
    gl.uniform1i(wp.u.uRainOn, rainT? 1 : 0);
    const waves=this.wavesOn();
    this.wavesDrawn=waves;   // for the tests
    gl.disable(gl.BLEND);
    gl.depthMask(false);
    gl.disable(gl.CULL_FACE);
    gl.enable(gl.POLYGON_OFFSET_FILL); gl.polygonOffset(-1.0,-2.0);
    if(waves){
      const wm=this._waveMesh();
      gl.uniform1i(wp.u.uWaves,1);
      gl.uniform3f(wp.u.uEyeW,eye[0],eye[1],eye[2]);
      gl.uniform4f(wp.u.uBox,m.box[0],m.box[1],m.box[2],m.box[3]);
      gl.uniform1f(wp.u.uWaterZ,m.top);
      gl.uniform1f(wp.u.uWaveH,this.waveHeightNow());
      gl.bindVertexArray(wm.vao);
      gl.drawElements(gl.TRIANGLES,wm.count,gl.UNSIGNED_SHORT,0);
      gl.uniform1i(wp.u.uWaves,0);
    }else{
      gl.bindVertexArray(m.vao);
      gl.drawElements(gl.TRIANGLES,m.sheet,gl.UNSIGNED_SHORT,0);
    }
    gl.disable(gl.POLYGON_OFFSET_FILL);
    /* The skirt: translucent, depth-tested, and writing no depth - it is water, and what
       is behind it is meant to show through (round 16). It stands in the terrain's own
       edge plane now (round 17i), filling the band from the water line down to the
       ground; the polygon offset keeps it off the terrain it meets. */
    const sp=this.progWSkirt;
    setCommon(sp);
    gl.uniform1f(sp.u.uTop,m.top); gl.uniform1f(sp.u.uBase,m.base);
    gl.enable(gl.BLEND); gl.blendFunc(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA);
    gl.depthMask(false);
    gl.enable(gl.POLYGON_OFFSET_FILL); gl.polygonOffset(1.0,2.0);
    gl.bindVertexArray(m.vao);
    gl.drawElements(gl.TRIANGLES,m.skirt,gl.UNSIGNED_SHORT,m.sheet*2);
    gl.disable(gl.POLYGON_OFFSET_FILL);
    gl.disable(gl.BLEND);
    gl.depthMask(true);
    gl.enable(gl.CULL_FACE);
    gl.bindVertexArray(null);
    if(diag){
      let hp=null, depBits=null;
      try{ const f=gl.getShaderPrecisionFormat(gl.FRAGMENT_SHADER,gl.HIGH_FLOAT); hp=f? f.precision : null; }catch(_){ }
      try{ gl.bindFramebuffer(gl.FRAMEBUFFER,T.resolveFB); depBits=gl.getParameter(gl.DEPTH_BITS);
           gl.bindFramebuffer(gl.FRAMEBUFFER,T.sceneFB); }catch(_){ }
      T.diag={errCol, errDep, errSurface:gl.getError(), samples:T.samples, near, far, hp, depBits,
              W, H, RW:T.RW, RH:T.RH, refl:this.reflDrawn, rect:this.reflRect, vol:!!vol, waves};
    }
    /* 5. And now what writes no depth: the waterfalls, the mist, the flames — over the
       surface, tested against the scene's depth. Drawn before the surface they were under
       its reflection wherever the surface lay behind them (round 17x). */
    gl.disable(gl.CULL_FACE);
    drawScene(VP,eye,{phase:'translucent', waterSplit:split});
    this._waterFrame=true;
    return true;
  },

  /** The offscreen frame onto the canvas. */
  endWaterFrame(){
    if(!this._waterFrame){ this.shaftsDrawn=false; return; }
    this._waterFrame=false;
    const gl=this.gl, T=this._wt;
    if(!T) return;
    this._gpuMark(this.opts.sunshafts? 'sunshafts' : 'present');
    /* Resolved into the single-sample colour texture, then *drawn* onto the canvas: a
       blit onto an antialiased canvas is INVALID_OPERATION in WebGL2 (its own buffer
       counts as multisampled), found the first time this ran. */
    gl.bindFramebuffer(gl.READ_FRAMEBUFFER,T.sceneFB);
    gl.bindFramebuffer(gl.DRAW_FRAMEBUFFER,T.resolveFB);
    gl.blitFramebuffer(0,0,T.W,T.H,0,0,T.W,T.H,gl.COLOR_BUFFER_BIT,gl.NEAREST);
    // The depth too, for the sunshafts' sky mask - the water resolved it already.
    if(!(this.water && this.opts.mgeWater!==false))
      gl.blitFramebuffer(0,0,T.W,T.H,0,0,T.W,T.H,gl.DEPTH_BUFFER_BIT,gl.NEAREST);
    gl.disable(gl.DEPTH_TEST); gl.disable(gl.BLEND); gl.depthMask(false);
    gl.bindVertexArray(null);
    const f=this._frame;
    /* Round 18do: with FXAA on, the finished frame - the plain copy or the sunshafts'
       combine - goes to the pass's own target, and the pass draws the canvas. */
    const fxDst=this.fxaaDest? this.fxaaDest(T.W,T.H) : null;
    /* Round 18dy: and the depth of field before it, where Robin's chain has it - second
       from last, after the bloom and before the FXAA. When it will run the frame goes to
       its own target first and the pass hands on to the FXAA's (or the canvas). */
    const dofDst=this.dofDest? this.dofDest(T.W,T.H) : null;
    /* Wraithguard: and the bloom before that - MGE's "sensor" category, after the
       sunshafts and before the lens (42_wg_bloom.js). */
    const bloomDst=this.bloomDest? this.bloomDest(T.W,T.H) : null;
    const dst=bloomDst||dofDst||fxDst;
    this.fxaaDrawn=false; this.outlineDrawn=false; this.dofDrawn=false; this.bloomDrawn=false;
    // Round 18dq: the outline's mask, from the depth just resolved, before the frame moves on.
    const outline=!!(this.drawOutlineMask && this.drawOutlineMask(T));
    // 18dt: Sunshafts.fx is masked off under water (mgeflags 9), and the underwater post
    // pass (35_underwater.js) takes the plain copy's place: the frame through the wobble.
    /* 18dz: and the shafts stand down for the crossing too, so the pass that carries the
       water's look to the pixels under the line has the frame to itself. */
    const shafts = !!(this.opts.sunshafts && f && f.scat && f.now && !this.underFogNow && this.drawSunshafts(T,f,dst));
    this.shaftsDrawn=shafts;      // for the tests: whether this frame got its rays
    this.underDrawn=false;
    if(!shafts && this.underFogNow && this.drawUnderwater && this.drawUnderwater(T,f,dst)) this.underDrawn=true;
    else if(!shafts){
      gl.bindFramebuffer(gl.FRAMEBUFFER,dst);
      if(!this.progBlit) this.progBlit=this._prog(VS_BLIT,FS_BLIT);
      const bp=this.progBlit; gl.useProgram(bp.p);
      gl.viewport(0,0,T.W,T.H);
      gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D,T.colTex);
      gl.uniform1i(bp.u.uTex,0);
      gl.drawArrays(gl.TRIANGLES,0,3);
    }
    if(bloomDst) this.drawBloom(T,dofDst||fxDst);   // the glare, into the lens and then the FXAA
    if(dofDst) this.drawDof(T,fxDst);      // 18dy: the blur, then the outline over it
    if(outline) this.drawOutline(T,fxDst);   // over the frame, under the FXAA
    if(fxDst) this.drawFXAA(T.W,T.H);
    gl.enable(gl.DEPTH_TEST); gl.depthMask(true);
    gl.bindTexture(gl.TEXTURE_2D,null);
  },

  /* Round 16: the sunshafts - a port of `Sunshafts.fx` (phal, tweaked by Hrnchamd for
     MGE XE 0.11; the copy in Robin's "Watch the Skies" mod). Five passes on the
     resolved frame: the sky picked out of the depth at half size (`stretch`), blurred
     towards the sun (`blurRHalf`), the rays - twenty-five samples along the line to the
     sun, weighted by MGE's falloffs (`rays`) - smoothed across (`blurT`), and the frame
     combined with them (`combine`). The sun disc the shader also draws is left to the
     sky pass, which has one already. The sun's place on the screen and whether it is in
     front of the eye come from the frame's own matrices. Returns false when the sun is
     behind, and the plain copy is drawn instead. */
  drawSunshafts(T,f,dst){
    const gl=this.gl;
    if(!this.progShaftMask){
      this.progShaftMask=this._prog(VS_BLIT,FS_SHAFT_MASK);
      this.progShaftBlurR=this._prog(VS_BLIT,FS_SHAFT_BLURR);
      this.progShaftRays=this._prog(VS_BLIT,FS_SHAFT_RAYS);
      this.progShaftBlurT=this._prog(VS_BLIT,FS_SHAFT_BLURT);
      this.progShaftCombine=this._prog(VS_BLIT,FS_SHAFT_COMBINE);
    }
    const now=f.now, W=T.W, H=T.H, RW=T.RW, RH=T.RH;
    // The sun on the screen: its direction through the frame's view and projection.
    const sd=now.sunDir;
    const Vm=f.V, Pm=f.P;
    // view-space direction = V * (dir, 0)
    const vx=Vm[0]*sd[0]+Vm[4]*sd[1]+Vm[8]*sd[2];
    const vy=Vm[1]*sd[0]+Vm[5]*sd[1]+Vm[9]*sd[2];
    const vz=Vm[2]*sd[0]+Vm[6]*sd[1]+Vm[10]*sd[2];
    const forward=-vz;                       // the camera looks down -z
    this._shaftDbg={forward,vz,sd,sunVis:now.sunVis};
    if(forward<=0.05 || now.sunVis<=0.001) return false;
    const sx=(Pm[0]*vx)/(-vz), sy=(Pm[5]*vy)/(-vz);   // NDC
    const sunview=[sx*0.5+0.5, sy*0.5+0.5];
    const light=1-Math.pow(1-now.sunVis,2);
    const fade=x=>{ const t=Math.max(0,Math.min(1,(x+0.5)/0.5)); return t*t*(3-2*t); };
    const strength=1.4*light*fade(0.5-Math.abs(sunview[0]-0.5))*fade(0.5-Math.abs(sunview[1]-0.5));
    Object.assign(this._shaftDbg,{sunview,strength});
    if(strength<=0.001) return false;
    const rcp=[1/W,1/H];
    const half=(fb,w,h)=>{ gl.bindFramebuffer(gl.FRAMEBUFFER,fb); gl.viewport(0,0,w,h); };
    const tex=(unit,t)=>{ gl.activeTexture(gl.TEXTURE0+unit); gl.bindTexture(gl.TEXTURE_2D,t); };
    // 1. The sky mask, half size.
    let pr=this.progShaftMask; gl.useProgram(pr.p);
    half(T.shaftA,RW,RH);
    tex(0,T.depTex); gl.uniform1i(pr.u.uDepth,0); gl.uniform2fv(pr.u.uRcpRes,rcp);
    gl.drawArrays(gl.TRIANGLES,0,3);
    // 2. Blurred towards the sun.
    pr=this.progShaftBlurR; gl.useProgram(pr.p);
    half(T.shaftB,RW,RH);
    tex(0,T.shaftATex); gl.uniform1i(pr.u.uTex,0); gl.uniform2fv(pr.u.uRcpRes,[1/RW,1/RH]); gl.uniform2fv(pr.u.uSun,sunview);
    gl.drawArrays(gl.TRIANGLES,0,3);
    // 3. The rays.
    pr=this.progShaftRays; gl.useProgram(pr.p);
    half(T.shaftA,RW,RH);
    tex(0,T.shaftBTex); gl.uniform1i(pr.u.uTex,0); gl.uniform2fv(pr.u.uSun,sunview);
    gl.uniform1f(pr.u.uAspect,H/W); gl.uniform1f(pr.u.uStrength,strength); gl.uniform3fv(pr.u.uSunCol,now.sun);
    gl.drawArrays(gl.TRIANGLES,0,3);
    // 4. Smoothed across the rays.
    pr=this.progShaftBlurT; gl.useProgram(pr.p);
    half(T.shaftB,RW,RH);
    tex(0,T.shaftATex); gl.uniform1i(pr.u.uTex,0); gl.uniform2fv(pr.u.uRcpRes,[1/RW,1/RH]); gl.uniform2fv(pr.u.uSun,sunview);
    gl.drawArrays(gl.TRIANGLES,0,3);
    // 5. Onto the canvas (or the FXAA target, 18do), with the frame.
    pr=this.progShaftCombine; gl.useProgram(pr.p);
    gl.bindFramebuffer(gl.FRAMEBUFFER,dst||null); gl.viewport(0,0,W,H);
    tex(0,T.colTex); gl.uniform1i(pr.u.uTex,0);
    tex(1,T.shaftBTex); gl.uniform1i(pr.u.uRays,1);
    gl.drawArrays(gl.TRIANGLES,0,3);
    gl.activeTexture(gl.TEXTURE0);
    return true;
  },
});
