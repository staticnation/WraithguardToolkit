/* =====================================================================================
   Screen-space ambient occlusion — round 18a, redone in 18c as a port of MGE XE's.

   Robin: "Add a SSAO image effect to the viewport that is toggleable under preview
   settings." A Preview switch, off by default, profile key `ssao`.

   What it is: the corners and creases of the picture darkened a little, from the depth
   buffer alone. Where a rock meets the ground, where a wall turns, under the eaves, the
   sky that would light an open surface is partly hidden, and the ambient term - which the
   game applies evenly everywhere - is too bright there. The effect estimates how much of
   the hemisphere over each pixel is blocked by what is drawn nearby and dims the ambient
   by that much. It is an *image* effect: it reads the frame's depth after the opaque
   phase and writes back onto the colour, and nothing in the scene or the plugin knows
   it happened.

   Round 18c. The first cut (18a) was a textbook SSAO with a 64-unit reach and a box blur,
   and in Vivec it gave "strong artefacts, and some very heavy handed results": a field of
   alpha-cut grass writes depth, and with that reach every blade shaded the ground for a
   yard around it. Robin sent the shader his screenshots go through - MGE XE's "SSAO HQ.fx",
   Knu's ssao v09 as shipped with MGE XE - and asked for the effect "as close as possible
   to the original shader", its code welcome. So this is a port of that shader, pass for
   pass, number for number, fed by what the page has:

   - `ssao`: the position from depth; the normal from the nearer neighbour on each axis
     and a tangent frame off it; sixteen fixed ray directions (Knu's table, the N=16 cut),
     each reflected through a per-pixel random normal, turned into the surface's own
     hemisphere (`ray *= sign(ray.z)`) and out into view space; a ray counts by how far it
     leans off the surface (`dot(ray, normal)`), and blocks when the drawn surface at its
     end stands in front of it, by `exp2(-diff / 15)` of how far in front. Reach 15 units.
   - `smartblur`, twice: twelve Poisson taps four pixels out, each weighted by how close
     its depth is to the pixel's own (`exp2(-|dz| / z / 0.06)`), the disc turned about on
     the second pass. MGE packs the depth into the AO texture's green and blue; the page
     keeps the frame's depth texture to hand and reads that instead, which is the same
     number at better precision.
   - `combine`: the frame lerped towards the fog's share of the pixel by `fog * 1.8 * AO`,
     where `fog` is 1 at the eye and 0 at the game's fog's end - what is far enough to be
     hazy is too far to shade, and the rest is 1.8 times the "physical" amount, which is
     the look. (Round 18f, below: which fog, and whose share.)
   - Nothing under the water line (MGE's `water - waterlevel < 0`), nothing over the sky.
   - Round 18d took the grass out of the depth it reads (the ground and the objects were
     drawn, the occlusion computed, the grass drawn, the combine over everything) because
     with the blades in it every one shaded the soil for a hand's breadth and a lawn read
     as one dark noise - which turned out to be the fade running five cells out (18f), not
     the blades. Round 18g puts the grass back: Robin, "it doesn't apply to the grass closer
     to the camera, and I can see ambient occlusion from geometry through the grass (as if
     the grass is transparent). This is not how it looks in the game." It is not: MGE's
     depth pass draws the grass too (XE Main.fx has a depth technique for it, alpha-tested),
     so a blade is in the depth the occlusion reads, takes its own shade and casts its own,
     and a blade in front of a wall's crease hides the crease. The whole opaque phase is
     drawn, then the four passes - `solid` and `grass` stay as phases, unused.

   - Round 18f, the fog. Robin: "it looks weird how it blends with the fog. The fog should
     render on top of the ambient occlusion, not the other way around" - and "still a bit
     noisy on far distances". Both were the combine's `fog`, and what it stood for. In MGE
     that pair (`fognearstart`, `fognearrange`) is not the distant-land fog at all: it is
     the *game's* fog, which `adjustFog` in distantland.cpp fits to the exponential curve
     between 1280 units and the game's own view range (7168) - and with the default
     distances that linear fog starts about 4200 units out and ends about 13000, so the
     occlusion is gone by a cell and a half, where a fifteen-unit ray is a fraction of a
     pixel and the picture is all noise. The page was handing it the exponential curve's
     own start and end - 16384 and 40960 - so the effect ran five cells out, into the
     noise. `ssaoFade` now does the same fit MGE does. And the colour: MGE lerps towards
     `fognearcol * (1 - fog)` because its linear fit is what the game's fog is, so that
     term *is* the fog's share of the pixel; the page's fog is exponential and scattered,
     so the same lerp pulled towards a colour the frame had never used, and the creases
     showed through the haze. The combine now asks `fogColour` - the same function the
     surfaces went through, at the same world position - what haze the frame laid on that
     pixel, and lerps towards exactly that: `scene = clear * lit + haze`, so
     `lerp(scene, haze, final)` darkens the lit part alone and the haze stays on top, to
     the number. Fog off, no atmosphere, a room: the haze is what those give, nought.

   Differences that are the page's, not the shader's: view space is GL's (z negative into
   the scene, y up), so the same geometry is written the other way up; and the random-
   normal tile is generated rather than read from `MGE/poisson_nrm.dds` (an MGE asset -
   same kind of numbers, no file to find).

   Provenance, for the licence file: "SSAO HQ.fx", based on ssao v09 by Knu, distributed
   with MGE XE (GPL-2.0), ported here with Robin's say-so: "Totally OK to use code in the
   SSAO shader I sent you."

   How it sits in the frame: the frame already goes through offscreen targets for MGE's
   water and the sunshafts (26_water.js), and this rides the same road - `wantsOffscreen`
   says yes while the switch is on. After the opaque phase the depth is resolved into
   `depTex`, the three passes run, and the combine is drawn onto the multisampled scene
   with a blend that does the lerp (`ONE, SRC_ALPHA`: src.rgb + dst * src.a) - before the
   water and the translucent phase, so the water's refraction sees the darkened bottom
   too, and the mist, the flames and the waterfalls, which write no depth and are not
   surfaces, are not darkened by it.

   One engine rule as ever: nothing here touches the plugin.
   ===================================================================================== */

// Knu's adjustable variables, as shipped in the HQ file.
const SSAO_N=32;                   // samples
const SSAO_R=15.0;                 // max ray radius, world units
const SSAO_MULTIPLIER=1.8;         // overall strength; 1.0 is the physically correct value
const SSAO_OCCLUSION_FALLOFF=15.0; // more means less precision and more strength
const SSAO_BLUR_FALLOFF=0.06;      // blur depth falloff; more blurs across a larger depth range
const SSAO_BLUR_RADIUS=4.0;        // in pixels
const SSAO_M=12;                   // blur taps

/* Knu's ray directions, the sixteen the N=16 build compiles (the N>=16, N>=10 and the
   unconditional blocks, in that order). */
/* Round 18bp: thirty-two, wearing the sixteen's radii.
   Knu's set is not unit vectors — the lengths run from 0.036 to 0.897 and that spread is
   what gives the kernel samples at many distances from the point, which is half of why it
   reads as ambient occlusion rather than as a contact line. So the directions are a
   Hammersley hemisphere (deterministic, low-discrepancy, cosine-weighted about +z, which
   is the frame the shader builds) and each of his sixteen lengths is used twice: min,
   median, max and mean of the new set are his to three places. */
const SSAO_DIRS=new Float32Array([
  -0.00450,  0.00000,  0.03572,
   0.00000,  0.00909,  0.04100,
  -0.00000, -0.01593,  0.05473,
   0.02970,  0.02970,  0.11985,
  -0.04640, -0.04640,  0.16223,
  -0.06918,  0.06918,  0.21476,
   0.09274, -0.09274,  0.25977,
   0.16325,  0.06762,  0.31937,
  -0.18951, -0.07850,  0.34107,
  -0.11614,  0.28039,  0.46706,
   0.14512, -0.35034,  0.54263,
   0.19179,  0.46302,  0.66913,
  -0.20856, -0.50351,  0.68070,
  -0.52387,  0.21699,  0.66378,
   0.55039, -0.22798,  0.65447,
   0.61229,  0.12179,  0.64411,
  -0.02535, -0.00504,  0.02505,
  -0.00606,  0.03046,  0.02827,
   0.00846, -0.04251,  0.03702,
   0.05508,  0.08243,  0.07938,
  -0.07782, -0.11646,  0.10491,
  -0.16084,  0.10747,  0.13519,
   0.20289, -0.13557,  0.15855,
   0.26007,  0.17378,  0.18812,
  -0.28956, -0.19348,  0.19268,
  -0.27624,  0.41342,  0.25104,
   0.33469, -0.50090,  0.27445,
   0.15119,  0.76010,  0.31350,
  -0.16055, -0.80712,  0.28839,
  -0.82210,  0.16353,  0.24401,
   0.84741, -0.16856,  0.19161,
   0.88568,  0.08723,  0.11213]);
/* And the blur's twelve Poisson taps. */
const SSAO_TAPS=new Float32Array([
  -0.326212,-0.40581,  -0.840144,-0.07358, -0.695914, 0.457137, -0.203345, 0.620716,
   0.96234, -0.194983,  0.473434,-0.480026, 0.519456, 0.767022,  0.185461,-0.893124,
   0.507431, 0.064425,  0.89642,  0.412458,-0.32194, -0.932615, -0.791559,-0.59771]);

const VS_SSAO=`#version 300 es
precision highp float;
out vec2 vUV;
void main(){
  // One triangle over the screen: three vertices, no buffer.
  vec2 p = vec2((gl_VertexID==1)? 3.0 : -1.0, (gl_VertexID==2)? 3.0 : -1.0);
  vUV = p*0.5+0.5;
  gl_Position = vec4(p, 0.0, 1.0);
}`;

/* Shared by the three passes: MGE's toView/fromView over the page's depth buffer.
   MGE's view space has depth positive along the view; here it is GL's, z negative into
   the scene, and `toView` hands back (x, y, depth) with depth positive so the shader's
   arithmetic reads as written. (No backticks in here.) */
const SSAO_VIEW_GLSL=`
uniform highp sampler2D uDepth;
uniform vec4 uProjInv;               // 1/P[0], 1/P[5], P[10], P[14] - the projection undone
uniform mat4 uProj;                  // the projection itself, for fromView
/* MGE's sky: a depth past everything drawn. The buffer's 1.0 is the clear value. */
bool isSky(vec2 uv){ return texture(uDepth, uv).r >= 1.0; }
/* Depth buffer value at uv -> (x, y, depth). ndc.z = (A z + B) / (-z), so the GL z is
   -B / (ndc.z + A); depth is its negative; x and y follow from the two scales. */
vec3 toView(vec2 uv){
  float d = texture(uDepth, uv).r;
  vec3 ndc = vec3(uv*2.0-1.0, d*2.0-1.0);
  float z = -uProjInv.w / (ndc.z + uProjInv.z);
  return vec3(ndc.x*uProjInv.x*(-z), ndc.y*uProjInv.y*(-z), -z);
}
/* (x, y, depth) -> uv, through the projection. */
vec2 fromView(vec3 v){
  vec4 c = uProj*vec4(v.x, v.y, -v.z, 1.0);
  return (c.xy/max(c.w, 1e-6))*0.5+0.5;
}
`;

/* Pass 1, MGE's ssao: the occlusion amount, 0..1. (No backticks in here.) */
const FS_SSAO=`#version 300 es
precision highp float;
in vec2 vUV;
${SSAO_VIEW_GLSL}
uniform sampler2D uNoise;            // the random normals (MGE's poisson_nrm), tiled
uniform vec2 uRcpRes;                // one pixel, in uv
uniform float uR, uFalloff;
uniform vec3 uUp;                    // world up, in view space - for the water line
uniform float uEyeZ, uWaterZ;        // the eye's height and the water's, world units
uniform vec3 uDirs[${SSAO_N}];
out vec4 o;
void main(){
  vec3 pos = toView(vUV);
  // MGE: the sky, and anything under the water line, take no occlusion.
  float water = uEyeZ + dot(vec3(pos.xy, -pos.z), uUp);
  if(isSky(vUV) || pos.z <= 0.0 || water < uWaterZ){ o = vec4(0.0, 0.0, 0.0, 1.0); return; }
  vec3 left  = pos - toView(vUV + uRcpRes*vec2(-1.0, 0.0));
  vec3 right = toView(vUV + uRcpRes*vec2(1.0, 0.0)) - pos;
  vec3 up    = pos - toView(vUV + uRcpRes*vec2(0.0, -1.0));
  vec3 down  = toView(vUV + uRcpRes*vec2(0.0, 1.0)) - pos;
  vec3 dx = length(left) < length(right) ? left : right;
  vec3 dy = length(up) < length(down) ? up : down;
  /* MGE's frame. Checked rather than assumed: with (x, y up, depth) a wall facing the eye
     has up = (0, +, 0) and dx = (+, 0, 0), and cross(dy, dx) = (0, 0, -) - depth-negative,
     towards the eye - which is the side the hemisphere below wants, as in the original. */
  vec3 normal = normalize(cross(dy, dx));
  dy = normalize(cross(dx, normal));
  dx = normalize(dx);
  vec3 rnd = texture(uNoise, vUV/uRcpRes/8.0).xyz*2.0-1.0;
  float AO = 0.0, amount = 0.0;
  for(int j=0; j<${SSAO_N}; j++){
    vec3 ray = reflect(uDirs[j]*uR, rnd);
    ray *= sign(ray.z);
    ray = dx*ray.x + dy*ray.y + normal*ray.z;
    float weight = dot(normalize(ray), normal);
    vec3 occ = toView(fromView(pos + ray));
    float diff = pos.z + ray.z - occ.z;
    amount += weight;
    AO += weight * step(0.0, diff) * exp2(-diff/uFalloff);
  }
  o = vec4(vec3(AO/max(amount, 1e-6)), 1.0);
}`;

/* Pass 2 and 3, MGE's smartblur: the taps' AO weighted by how close their depth is to
   the pixel's own. uRev is MGE's rev - +radius on the first pass, -radius on the second,
   which it carried in the alpha channel. (No backticks in here.) */
const FS_SSAO_BLUR=`#version 300 es
precision highp float;
in vec2 vUV;
${SSAO_VIEW_GLSL}
uniform sampler2D uAO;
uniform vec2 uRcpRes;
uniform float uRev, uBlurFalloff;
uniform vec2 uTaps[${SSAO_M}];
out vec4 o;
void main(){
  float total = texture(uAO, vUV).r;
  float depth = toView(vUV).z;
  float amount = 1.0;
  for(int i=0; i<${SSAO_M}; i++){
    vec2 s_tex = vUV + uRcpRes*uTaps[i]*uRev;
    float s_depth = toView(s_tex).z;
    float weight = exp2(-abs(depth - s_depth)/depth/uBlurFalloff);
    amount += weight;
    total += texture(uAO, s_tex).r*weight;
  }
  o = vec4(vec3(total/amount), 1.0);
}`;

/* Pass 4, MGE's combine: result = lerp(scene, fognearcol * (1 - fog), fog * multiplier
   * AO). Drawn with blendFunc(ONE, SRC_ALPHA): the colour written is the haze's share, the
   alpha is what is left of the scene. Round 18f: the haze is the frame's own - SKY_GLSL's
   `fogColour` at the pixel's world position, which is what `airFog` added to the lit
   colour there - rather than MGE's linear stand-in for it. (No backticks in here.) */
const FS_SSAO_APPLY=`#version 300 es
precision highp float;
in vec2 vUV;
${SSAO_VIEW_GLSL}
uniform vec3 uFogCol; uniform float uFogK;   // airFog's other branch: the plain backdrop's dim
${SKY_GLSL}
uniform sampler2D uAO;
uniform float uMultiplier;
uniform vec2 uFogNear;               // fognearstart, fognearrange - the game's fog as MGE fits it
uniform mat4 uView;
out vec4 o;
void main(){
  if(isSky(vUV)){ o = vec4(0.0, 0.0, 0.0, 1.0); return; }
  vec3 v = toView(vUV);
  float dist = length(v);
  float fog = clamp((uFogNear.y - dist)/max(uFogNear.y - uFogNear.x, 1.0), 0.0, 1.0);
  float final = min(1.0, fog*uMultiplier*texture(uAO, vUV).r);
  /* Back to the world: the view's rotation undone (its transpose), from the eye. */
  vec3 w = uEye + transpose(mat3(uView))*vec3(v.x, v.y, -v.z);
  vec3 haze;
  if(uScat == 1){
    vec3 ev = w - uEye;
    haze = fogColour(ev/max(dist, 0.001), dist).rgb;
  } else {
    haze = uFogCol*clamp(1.0 - exp(-uFogK*length(w.xy)), 0.0, 1.0);
  }
  o = vec4(haze*final, 1.0-final);
}`;

/* =====================================================================================
   SSGI - horizon-based ambient occlusion with screen-space global illumination.

   Wraithguard: the alternative to the SSAO above, one or the other (Preview, Ambient
   occlusion, Mode). A port of the MGE XE shader "SSGI.fx" (HBAO + SSGI, supplied by the
   project's author), pass for pass and number for number, onto the same road as SSAO:

   - `generate`: SSAO's kernel (N=16, the first sixteen of Knu's directions), with a
     radius that grows a little with distance and a surface-match weight, and - the GI -
     every ray that lands on a nearer surface in view also picks up that surface's lit
     colour from the frame (`lastshader`, here the resolved colour), weighted by how
     squarely it leans off the surface and how far it went. Out: rgb the bounced light,
     a the occlusion.
   - two bilateral blurs, rgb and a together, depth-weighted.
   - `combine`: the frame darkened towards the haze by the occlusion (as SSAO's combine)
     and the bounce light added on top, both faded out with the game's fog.

   Nothing under the water line, as with SSAO.
   ===================================================================================== */
const SSGI_N=16;
const SSGI_R=12.75, SSGI_AO_EXT=1.8, SSGI_AO_INT=2.0, SSGI_AO_FALLOFF=11.5, SSGI_AO_POWER=1.4;
const SSGI_GI_EXT=0.32, SSGI_GI_INT=0.35, SSGI_GI_MAX=0.57, SSGI_DEPTH_BIAS=1.00025;
const SSGI_MAX_DEPTH_DIFF=3.0, SSGI_SATURATION=2.15;
const SSGI_BLUR_FALLOFF=0.23, SSGI_BLUR_RADIUS=4.85;
const SSGI_DIRS=new Float32Array([
  -0.00941, -0.00326, -0.05597,   0.11686,  0.00831,  0.04915,
  -0.08125, -0.24638,  0.30141,   0.35193,  0.29639,  0.47544,
   0.32063, -0.70203, -0.40622,  -0.37344, -0.18112,  0.37140,
  -0.73605, -0.39320,  0.04992,   0.02274,  0.21583,  0.19429,
   0.00762, -0.01247,  0.03311,  -0.61057,  0.20510,  0.58876,
   0.55319,  0.67960, -0.19194,  -0.43533,  0.62404,  0.45133,
  -0.02386, -0.03104,  0.01502,  -0.20990,  0.10082,  0.03849,
   0.06331, -0.17620, -0.31359,  -0.12261,  0.00720, -0.12465]);
const FS_SSGI=`#version 300 es
precision highp float;
in vec2 vUV;
${SSAO_VIEW_GLSL}
uniform sampler2D uNoise;
uniform sampler2D uScene;            // the lit frame, resolved (MGE's lastshader)
uniform vec2 uRcpRes;
uniform vec3 uUp;
uniform float uEyeZ, uWaterZ;
uniform float uFogRange;             // fognearrange
uniform int uInterior;
uniform vec3 uDirs[${SSGI_N}];
out vec4 o;
void main(){
  vec3 pos = toView(vUV);
  float water = uEyeZ + dot(vec3(pos.xy, -pos.z), uUp);
  if(isSky(vUV) || pos.z <= 0.0 || water < uWaterZ){ o = vec4(0.0); return; }
  vec3 left  = pos - toView(vUV + uRcpRes*vec2(-1.0, 0.0));
  vec3 right = toView(vUV + uRcpRes*vec2(1.0, 0.0)) - pos;
  vec3 up    = pos - toView(vUV + uRcpRes*vec2(0.0, -1.0));
  vec3 down  = toView(vUV + uRcpRes*vec2(0.0, 1.0)) - pos;
  vec3 dx = length(left) < length(right) ? left : right;
  vec3 dy = length(up) < length(down) ? up : down;
  vec3 normal = normalize(cross(dy, dx));
  dy = normalize(cross(dx, normal));
  dx = normalize(dx);
  vec3 rnd = texture(uNoise, vUV/uRcpRes/8.0).xyz*2.0-1.0;
  float AO = 0.0, amount = 0.0, giAmount = 0.0;
  vec3 GI = vec3(0.0);
  float dynamicR = ${SSGI_R.toFixed(4)} * (1.0 + (pos.z/max(uFogRange, 1.0))*0.3);
  for(int j=0; j<${SSGI_N}; j++){
    vec3 ray = reflect(uDirs[j]*dynamicR, rnd);
    ray *= sign(ray.z);
    ray = dx*ray.x + dy*ray.y + normal*ray.z;
    float weight = dot(normalize(ray), normal);
    vec2 hitUV = fromView(pos + ray);
    float inside = step(0.0, hitUV.x)*step(0.0, hitUV.y)*step(hitUV.x, 1.0)*step(hitUV.y, 1.0);
    vec2 cuv = clamp(hitUV, 0.0, 1.0);
    vec3 occ = toView(cuv);
    float diff = pos.z + ray.z - ${SSGI_DEPTH_BIAS.toFixed(5)}*occ.z;
    float depthDiff = abs(pos.z - occ.z);
    float surfaceMatch = clamp(depthDiff/1.2, 0.0, 1.0);
    surfaceMatch = 1.0 - smoothstep(0.0, 2.0, 1.0 - surfaceMatch);
    amount += weight*surfaceMatch;
    AO += weight*step(0.0, diff)*exp2(-diff/${SSGI_AO_FALLOFF.toFixed(3)})*surfaceMatch;
    float hit = step(0.002, diff)*inside;
    float validDepth = step(depthDiff, ${SSGI_MAX_DEPTH_DIFF.toFixed(3)});
    float len = length(ray);
    float giWeight = clamp(weight, 0.0, 1.0)*hit*(1.0/(1.0 + len*len))*validDepth;
    if(giWeight > 0.0){
      GI += min(texture(uScene, cuv).rgb, vec3(${SSGI_GI_MAX.toFixed(3)}))*giWeight;
      giAmount += giWeight;
    }
  }
  float ao = pow(clamp(AO/max(amount, 1e-5), 0.0, 1.0), ${SSGI_AO_POWER.toFixed(3)});
  ao *= (uInterior == 1) ? ${SSGI_AO_INT.toFixed(3)} : ${SSGI_AO_EXT.toFixed(3)};
  vec3 gi = giAmount > 0.001 ? GI/giAmount : vec3(0.0);
  gi = min(gi, vec3(${SSGI_GI_MAX.toFixed(3)}));
  float luma = dot(gi, vec3(0.299, 0.587, 0.114));
  gi = min(vec3(luma) + (gi - vec3(luma))*${SSGI_SATURATION.toFixed(3)}, vec3(${SSGI_GI_MAX.toFixed(3)}));
  gi *= 1.0 - clamp(ao, 0.0, 1.0);
  o = vec4(max(gi, vec3(0.0)), clamp(ao, 0.0, 1.0));
}`;
/* Both of SSGI.fx's bilateral blurs: rgb (the bounce) and a (the occlusion) together,
   each tap weighted by how near its depth is to the pixel's. `uRadius` is the pass's. */
const FS_SSGI_BLUR=`#version 300 es
precision highp float;
in vec2 vUV;
${SSAO_VIEW_GLSL}
uniform sampler2D uAO;
uniform vec2 uRcpRes;
uniform float uRadius;
uniform vec2 uTaps[${SSAO_M}];
out vec4 o;
void main(){
  vec4 c = texture(uAO, vUV);
  float cd = toView(vUV).z;
  vec4 total = c; float amount = 1.0;
  for(int i=0; i<${SSAO_M}; i++){
    vec2 uv = vUV + uRcpRes*uTaps[i]*uRadius;
    float w = exp2(-abs(cd - toView(uv).z)/(max(cd, 1e-4)*${SSGI_BLUR_FALLOFF.toFixed(3)}));
    total += texture(uAO, uv)*w; amount += w;
  }
  o = total/amount;
}`;
/* SSGI.fx's combine, drawn like SSAO's with blendFunc(ONE, SRC_ALPHA): the colour written
   is the haze's share plus the bounce light, the alpha what is left of the scene. */
const FS_SSGI_APPLY=`#version 300 es
precision highp float;
in vec2 vUV;
${SSAO_VIEW_GLSL}
uniform vec3 uFogCol; uniform float uFogK;
${SKY_GLSL}
uniform sampler2D uAO;
uniform vec2 uFogNear;
uniform mat4 uView;
out vec4 o;
void main(){
  if(isSky(vUV)){ o = vec4(0.0, 0.0, 0.0, 1.0); return; }
  vec3 v = toView(vUV);
  float dist = length(v);
  float fog = clamp((uFogNear.y - dist)/max(uFogNear.y - uFogNear.x, 1.0), 0.0, 1.0);
  vec4 fx = texture(uAO, vUV);
  float final = clamp(fog*${SSGI_AO_EXT.toFixed(3)}*fx.a, 0.0, 1.0);
  vec3 w = uEye + transpose(mat3(uView))*vec3(v.x, v.y, -v.z);
  vec3 haze;
  if(uScat == 1){
    vec3 ev = w - uEye;
    haze = fogColour(ev/max(dist, 0.001), dist).rgb;
  } else {
    haze = uFogCol*clamp(1.0 - exp(-uFogK*length(w.xy)), 0.0, 1.0);
  }
  vec3 gi = fx.rgb*(${SSGI_GI_EXT.toFixed(3)}*fog);
  o = vec4(haze*final + gi, 1.0-final);
}`;

/* MGE's near fog for the post shaders: `adjustFog` in distantland.cpp, exponential mode.
   The game's own linear fog is fitted to the exponential curve so the two agree at 1280
   units and at the game's view range (7168, or the fog's end if that is nearer), and
   that fit is what `fognearstart` and `fognearrange` carry. `expStart` and `expDiv` are
   the curve's, as the frame's `uFogStart` and `uFogRange` hold them; `fogEnd` the
   curve's end in units. Returns [start, end]. */
const SSAO_FOG_NEAR=1280, SSAO_NEAR_VIEW=7168;
function ssaoNearFog(expStart,expDiv,fogEnd){
  const near=SSAO_FOG_NEAR, far=Math.min(fogEnd, SSAO_NEAR_VIEW);
  if(!(far>near+1) || !(expDiv>0)) return [near, near+1024];
  const eN=Math.exp(-(near-expStart)/expDiv), eF=Math.exp(-(far-expStart)/expDiv);
  const d=eF-eN;
  if(Math.abs(d)<1e-9) return [far, far+1024];
  return [near+(far-near)*(1-eN)/d, near+(far-near)*(-eN)/d];
}

Object.assign(Renderer.prototype,{
  /** MGE's fognearstart and fognearrange: the game's fog as `adjustFog` fits it to the
   *  weather's exponential curve (`ssaoNearFog`), the curve scaled by the Fog density
   *  slider as the frame scales it. Whether the fog is drawn or not: this is how far the
   *  occlusion reaches, and that does not change with the fog switch. Without the
   *  atmosphere, the fit of MGE's default distances - two cells to five. */
  ssaoFade(now,scat){
    const o=this.opts;
    const k=1/Math.max(0.05, o.fogDensity==null? 1 : o.fogDensity);
    if(scat && now && typeof Sky==='object'){
      const fr=Sky.fogRange();
      if(fr.exp===false) return [fr.linearStart*k, Math.max(fr.linearStart*k+1024, fr.linearEnd*k)];
      return ssaoNearFog(fr.start*k, fr.divisor*k, fr.linearEnd*k);
    }
    const CELL=8192, SCALE=4.4, s=2*CELL, e=5*CELL;
    return ssaoNearFog(s/SCALE*k, (e-s/SCALE)/SCALE*k, e*k);
  },

  /** The AO targets at the frame's size: two single-channel textures the passes ping-pong
   *  between, and the random-normal tile. Rebuilt with the frame. */
  _ssaoTargets(W,H){
    const gl=this.gl, t=this._ssaoT;
    if(t && t.W===W && t.H===H) return t;
    if(t){ for(const k of ['fbA','fbB']) try{ gl.deleteFramebuffer(t[k]); }catch(_){ }
           for(const k of ['texA','texB']) try{ gl.deleteTexture(t[k]); }catch(_){ } }
    const n={W,H};
    /* Linear, mirrored - MGE's sampler s4, which the blur reads through. */
    const tex=(w,h)=>{ const x=gl.createTexture(); gl.bindTexture(gl.TEXTURE_2D,x);
      // RGBA (Wraithguard): SSAO uses the red; SSGI the colour for its bounce and alpha for the AO.
      gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,w,h,0,gl.RGBA,gl.UNSIGNED_BYTE,null);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.MIRRORED_REPEAT);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.MIRRORED_REPEAT); return x; };
    n.texA=tex(W,H); n.fbA=gl.createFramebuffer(); gl.bindFramebuffer(gl.FRAMEBUFFER,n.fbA);
    gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,n.texA,0);
    n.ok = gl.checkFramebufferStatus(gl.FRAMEBUFFER)===gl.FRAMEBUFFER_COMPLETE;
    n.texB=tex(W,H); n.fbB=gl.createFramebuffer(); gl.bindFramebuffer(gl.FRAMEBUFFER,n.fbB);
    gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,n.texB,0);
    n.ok = n.ok && gl.checkFramebufferStatus(gl.FRAMEBUFFER)===gl.FRAMEBUFFER_COMPLETE;
    gl.bindFramebuffer(gl.FRAMEBUFFER,null);
    if(!this._ssaoNoise){
      /* The random normals MGE reads from poisson_nrm.dds, point-sampled and wrapped at
         the shader's tex/rcpres/8 - which is to say the pattern repeats every eight pixels
         whatever the texture's size, and the blur's four-pixel disc spans exactly one
         period of it. Round 18d: an 8x8 tile of unit vectors, then. The first cut (18c)
         gave every pixel its own random normal, and a blur cannot take out what does not
         repeat - "still looks a bit noisy in a way it doesn't in Morrowind". A fixed seed,
         so two frames of a still scene are the same frame. */
      let s=0x9e3779b9; const rnd=()=>{ s^=s<<13; s>>>=0; s^=s>>>17; s^=s<<5; s>>>=0; return s/4294967296; };
      const S=8, px=new Uint8Array(S*S*4);
      for(let i=0;i<S*S;i++){
        const z=rnd()*2-1, a=rnd()*Math.PI*2, r=Math.sqrt(1-z*z);
        px[i*4]=Math.round((Math.cos(a)*r*0.5+0.5)*255); px[i*4+1]=Math.round((Math.sin(a)*r*0.5+0.5)*255);
        px[i*4+2]=Math.round((z*0.5+0.5)*255); px[i*4+3]=255;
      }
      const x=gl.createTexture(); gl.bindTexture(gl.TEXTURE_2D,x);
      gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,S,S,0,gl.RGBA,gl.UNSIGNED_BYTE,px);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.NEAREST);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.NEAREST);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.REPEAT);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.REPEAT);
      this._ssaoNoise=x;
    }
    gl.bindTexture(gl.TEXTURE_2D,null);
    this._ssaoT=n;
    return n;
  },

  /** The first three of MGE's passes: the depth resolved, the occlusion computed into
   *  A.texA and blurred twice. Called after the solid phase - the ground and the objects,
   *  no grass - so the blades are not in the depth it reads. `T` is the water's target
   *  set, `P` and `V` the projection and view the scene was drawn with. Returns whether it
   *  did anything; `ssaoApply` then wants calling once the grass is drawn. */
  ssaoCompute(T,P,V){
    const gl=this.gl, W=T.W, H=T.H;
    const A=this._ssaoTargets(W,H);
    if(!A.ok) return false;
    if(this.opts.ssgi) return this._ssgiCompute(T,P,V,A);
    if(!this.progSSAO){
      this.progSSAO=this._prog(VS_SSAO,FS_SSAO);
      this.progSSAOBlur=this._prog(VS_SSAO,FS_SSAO_BLUR);
      this.progSSAOApply=this._prog(VS_SSAO,FS_SSAO_APPLY);
    }
    // The depth, resolved to a texture the passes can read.
    gl.bindFramebuffer(gl.READ_FRAMEBUFFER,T.sceneFB);
    gl.bindFramebuffer(gl.DRAW_FRAMEBUFFER,T.resolveFB);
    gl.blitFramebuffer(0,0,W,H,0,0,W,H,gl.DEPTH_BUFFER_BIT,gl.NEAREST);
    gl.disable(gl.DEPTH_TEST); gl.disable(gl.BLEND); gl.depthMask(false); gl.disable(gl.CULL_FACE);
    gl.bindVertexArray(this._ssaoVao||(this._ssaoVao=gl.createVertexArray()));
    const projInv=[1/P[0], 1/P[5], P[10], P[14]];
    this._ssaoP=P; this._ssaoProjInv=projInv; this._ssaoV=V;
    const common=(pr)=>{
      gl.useProgram(pr.p);
      gl.uniform1i(pr.u.uDepth,0);
      gl.uniform4fv(pr.u.uProjInv, projInv);
      gl.uniformMatrix4fv(pr.u.uProj,false,P);
      if(pr.u.uRcpRes) gl.uniform2f(pr.u.uRcpRes,1/W,1/H);
    };
    gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D,T.depTex);
    // 1. ssao -> A.
    gl.bindFramebuffer(gl.FRAMEBUFFER,A.fbA); gl.viewport(0,0,W,H);
    const pr=this.progSSAO; common(pr);
    gl.activeTexture(gl.TEXTURE2); gl.bindTexture(gl.TEXTURE_2D,this._ssaoNoise); gl.uniform1i(pr.u.uNoise,2);
    gl.uniform1f(pr.u.uR,SSAO_R);
    gl.uniform1f(pr.u.uFalloff,SSAO_OCCLUSION_FALLOFF);
    gl.uniform3fv(pr.u.uDirs,SSAO_DIRS);
    /* World up in view space is the view matrix's third column (the z-components of the
       camera's three axes), and the eye's height the camera's own. */
    const eye=this.cameraEye? this.cameraEye() : [0,0,0];
    gl.uniform3f(pr.u.uUp, V? V[8] : 0, V? V[9] : 1, V? V[10] : 0);
    gl.uniform1f(pr.u.uEyeZ, eye[2]);
    gl.uniform1f(pr.u.uWaterZ, this.water? this.water.z : -1e9);
    gl.drawArrays(gl.TRIANGLES,0,3);
    // 2, 3. smartblur, twice: A -> B with rev = +radius, B -> A with rev = -radius.
    const bl=this.progSSAOBlur; common(bl);
    gl.uniform1i(bl.u.uAO,1);
    gl.uniform1f(bl.u.uBlurFalloff,SSAO_BLUR_FALLOFF);
    gl.uniform2fv(bl.u.uTaps,SSAO_TAPS);
    for(const [from,to,rev] of (this._ssaoNoBlur? [] : [[A.texA,A.fbB,SSAO_BLUR_RADIUS],[A.texB,A.fbA,-SSAO_BLUR_RADIUS]])){   // _ssaoNoBlur: a probe's switch
      gl.bindFramebuffer(gl.FRAMEBUFFER,to);
      gl.activeTexture(gl.TEXTURE1); gl.bindTexture(gl.TEXTURE_2D,from);
      gl.uniform1f(bl.u.uRev,rev);
      gl.drawArrays(gl.TRIANGLES,0,3);
    }
    // Handed back the way the frame expects it, for the grass that follows.
    gl.bindFramebuffer(gl.FRAMEBUFFER,T.sceneFB); gl.viewport(0,0,W,H);
    gl.enable(gl.DEPTH_TEST); gl.depthMask(true);
    gl.bindVertexArray(null);
    for(const u of [2,1,0]){ gl.activeTexture(gl.TEXTURE0+u); gl.bindTexture(gl.TEXTURE_2D,null); }
    gl.activeTexture(gl.TEXTURE0);
    this._ssaoReady=true;
    return true;
  },

  /** MGE's `combine`, onto the multisampled scene in `sceneFB`: the occlusion from
   *  `ssaoCompute`, faded with the game's fog (`fade` its start and end). `airOn` is the
   *  frame's atmosphere-uniform setter and `fogK` its flat fog, so the shader can ask what
   *  haze the frame put on each pixel; `V` the view the scene was drawn with. Reads the
   *  depth as it was resolved before the grass - a blade against the sky takes none, a
   *  blade over the ground takes the ground's, as MGE's does. */
  ssaoApply(T,fade,airOn,fogK,V){
    const gl=this.gl, W=T.W, H=T.H, A=this._ssaoT;
    if(fade) this._ssaoFadeLast=fade;   // SSGI's generate reads fognearrange a frame late
    if(A && this._ssgiReady) return this._ssgiApply(T,fade,airOn,fogK,V,A);
    if(!A || !this._ssaoReady) return false;
    this._ssaoReady=false;
    gl.disable(gl.DEPTH_TEST); gl.depthMask(false); gl.disable(gl.CULL_FACE);
    gl.bindVertexArray(this._ssaoVao);
    gl.bindFramebuffer(gl.FRAMEBUFFER,T.sceneFB); gl.viewport(0,0,W,H);
    const ap=this.progSSAOApply; gl.useProgram(ap.p);
    if(typeof airOn==='function') airOn(ap);
    gl.uniform1f(ap.u.uFogK, fogK||0);
    gl.uniformMatrix4fv(ap.u.uView,false,V||this._ssaoV||[1,0,0,0, 0,1,0,0, 0,0,1,0, 0,0,0,1]);
    gl.uniform1i(ap.u.uDepth,0); gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D,T.depTex);
    gl.uniform4fv(ap.u.uProjInv, this._ssaoProjInv);
    gl.uniformMatrix4fv(ap.u.uProj,false,this._ssaoP);
    gl.uniform1i(ap.u.uAO,1); gl.activeTexture(gl.TEXTURE1); gl.bindTexture(gl.TEXTURE_2D,A.texA);
    gl.uniform1f(ap.u.uMultiplier,SSAO_MULTIPLIER);
    gl.uniform2f(ap.u.uFogNear, fade? fade[0] : 4096, fade? fade[1] : 13000);
    gl.enable(gl.BLEND); gl.blendFunc(gl.ONE,gl.SRC_ALPHA);
    gl.drawArrays(gl.TRIANGLES,0,3);
    gl.disable(gl.BLEND); gl.blendFunc(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA);
    gl.enable(gl.DEPTH_TEST); gl.depthMask(true);
    gl.bindVertexArray(null);
    for(const u of [1,0]){ gl.activeTexture(gl.TEXTURE0+u); gl.bindTexture(gl.TEXTURE_2D,null); }
    gl.activeTexture(gl.TEXTURE0);
    this.ssaoDrawn=true;      // for the tests: this frame had its occlusion
    return true;
  },

  /** SSGI's generate and blurs (Wraithguard), in place of SSAO's: the depth *and* the lit
   *  colour resolved, the effect into A.texA (rgb bounce, a occlusion), blurred twice. */
  _ssgiCompute(T,P,V,A){
    const gl=this.gl, W=T.W, H=T.H;
    if(!this.progSSGI){
      this.progSSGI=this._prog(VS_SSAO,FS_SSGI);
      this.progSSGIBlur=this._prog(VS_SSAO,FS_SSGI_BLUR);
      this.progSSGIApply=this._prog(VS_SSAO,FS_SSGI_APPLY);
    }
    gl.bindFramebuffer(gl.READ_FRAMEBUFFER,T.sceneFB);
    gl.bindFramebuffer(gl.DRAW_FRAMEBUFFER,T.resolveFB);
    gl.blitFramebuffer(0,0,W,H,0,0,W,H,gl.COLOR_BUFFER_BIT|gl.DEPTH_BUFFER_BIT,gl.NEAREST);
    gl.disable(gl.DEPTH_TEST); gl.disable(gl.BLEND); gl.depthMask(false); gl.disable(gl.CULL_FACE);
    gl.bindVertexArray(this._ssaoVao||(this._ssaoVao=gl.createVertexArray()));
    const projInv=[1/P[0], 1/P[5], P[10], P[14]];
    this._ssaoP=P; this._ssaoProjInv=projInv; this._ssaoV=V;
    const common=(pr)=>{
      gl.useProgram(pr.p);
      gl.uniform1i(pr.u.uDepth,0);
      gl.uniform4fv(pr.u.uProjInv, projInv);
      gl.uniformMatrix4fv(pr.u.uProj,false,P);
      if(pr.u.uRcpRes) gl.uniform2f(pr.u.uRcpRes,1/W,1/H);
    };
    gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D,T.depTex);
    // 1. generate -> A.
    gl.bindFramebuffer(gl.FRAMEBUFFER,A.fbA); gl.viewport(0,0,W,H);
    const pr=this.progSSGI; common(pr);
    gl.activeTexture(gl.TEXTURE2); gl.bindTexture(gl.TEXTURE_2D,this._ssaoNoise); gl.uniform1i(pr.u.uNoise,2);
    gl.activeTexture(gl.TEXTURE3); gl.bindTexture(gl.TEXTURE_2D,T.colTex); gl.uniform1i(pr.u.uScene,3);
    gl.uniform3fv(pr.u.uDirs,SSGI_DIRS);
    const eye=this.cameraEye? this.cameraEye() : [0,0,0];
    gl.uniform3f(pr.u.uUp, V? V[8] : 0, V? V[9] : 1, V? V[10] : 0);
    gl.uniform1f(pr.u.uEyeZ, eye[2]);
    gl.uniform1f(pr.u.uWaterZ, this.water? this.water.z : -1e9);
    gl.uniform1f(pr.u.uFogRange, (this._ssaoFadeLast||[4096,13000])[1]);
    gl.uniform1i(pr.u.uInterior, this.opts.room? 1 : 0);
    gl.drawArrays(gl.TRIANGLES,0,3);
    // 2, 3. the two bilateral blurs: A -> B at the full radius, B -> A at half.
    const bl=this.progSSGIBlur; common(bl);
    gl.uniform1i(bl.u.uAO,1);
    gl.uniform2fv(bl.u.uTaps,SSAO_TAPS);
    for(const [from,to,rad] of [[A.texA,A.fbB,SSGI_BLUR_RADIUS],[A.texB,A.fbA,SSGI_BLUR_RADIUS*0.5]]){
      gl.bindFramebuffer(gl.FRAMEBUFFER,to);
      gl.activeTexture(gl.TEXTURE1); gl.bindTexture(gl.TEXTURE_2D,from);
      gl.uniform1f(bl.u.uRadius,rad);
      gl.drawArrays(gl.TRIANGLES,0,3);
    }
    gl.bindFramebuffer(gl.FRAMEBUFFER,T.sceneFB); gl.viewport(0,0,W,H);
    gl.enable(gl.DEPTH_TEST); gl.depthMask(true);
    gl.bindVertexArray(null);
    for(const u of [3,2,1,0]){ gl.activeTexture(gl.TEXTURE0+u); gl.bindTexture(gl.TEXTURE_2D,null); }
    gl.activeTexture(gl.TEXTURE0);
    this._ssgiReady=true;
    return true;
  },

  /** SSGI.fx's combine onto the scene (Wraithguard), blended as SSAO's is. */
  _ssgiApply(T,fade,airOn,fogK,V,A){
    const gl=this.gl, W=T.W, H=T.H;
    this._ssgiReady=false;
    gl.disable(gl.DEPTH_TEST); gl.depthMask(false); gl.disable(gl.CULL_FACE);
    gl.bindVertexArray(this._ssaoVao);
    gl.bindFramebuffer(gl.FRAMEBUFFER,T.sceneFB); gl.viewport(0,0,W,H);
    const ap=this.progSSGIApply; gl.useProgram(ap.p);
    if(typeof airOn==='function') airOn(ap);
    gl.uniform1f(ap.u.uFogK, fogK||0);
    gl.uniformMatrix4fv(ap.u.uView,false,V||this._ssaoV||[1,0,0,0, 0,1,0,0, 0,0,1,0, 0,0,0,1]);
    gl.uniform1i(ap.u.uDepth,0); gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D,T.depTex);
    gl.uniform4fv(ap.u.uProjInv, this._ssaoProjInv);
    gl.uniformMatrix4fv(ap.u.uProj,false,this._ssaoP);
    gl.uniform1i(ap.u.uAO,1); gl.activeTexture(gl.TEXTURE1); gl.bindTexture(gl.TEXTURE_2D,A.texA);
    gl.uniform2f(ap.u.uFogNear, fade? fade[0] : 4096, fade? fade[1] : 13000);
    gl.enable(gl.BLEND); gl.blendFunc(gl.ONE,gl.SRC_ALPHA);
    gl.drawArrays(gl.TRIANGLES,0,3);
    gl.disable(gl.BLEND); gl.blendFunc(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA);
    gl.enable(gl.DEPTH_TEST); gl.depthMask(true);
    gl.bindVertexArray(null);
    for(const u of [1,0]){ gl.activeTexture(gl.TEXTURE0+u); gl.bindTexture(gl.TEXTURE_2D,null); }
    gl.activeTexture(gl.TEXTURE0);
    this.ssaoDrawn=true;
    return true;
  },

  /** Both, for a caller that has drawn the whole opaque phase at once. */
  applySSAO(T,P,fade,V,airOn,fogK){
    return this.ssaoCompute(T,P,V) && this.ssaoApply(T,fade,airOn,fogK,V);
  },
});
