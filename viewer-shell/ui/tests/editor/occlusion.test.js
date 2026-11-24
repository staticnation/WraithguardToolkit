/* Occlusion culling (29_wg_occlusion.js; Static 2026-11-24: "occlusion culling"): a
   read-back of the drawn depth, reprojected into the current view, hides a run's box only
   when everything over its rectangle is nearer than its nearest point - and never at an
   occluder's edge, where nothing was read, or for a box that reaches the eye. */
'use strict';
const vm = require('vm');
const fs = require('fs');
const path = require('path');

module.exports = async (t) => {
  const ctx = { Math, Float32Array, Float64Array, Uint8Array, Infinity, isFinite, Object, Array, Renderer: class {} };
  vm.createContext(ctx);
  vm.runInContext(fs.readFileSync(path.join(__dirname, '..', '..', 'src', '29_wg_occlusion.js'), 'utf8')
    .replace(/^const (\w+)=/mg, 'var $1='), ctx);
  const Occ = ctx.Occ;

  // The renderer's matrices (06_gl.js M4), column-major.
  const mul = (a, b) => { const o = new Float32Array(16);
    for (let c = 0; c < 4; c++) for (let r = 0; r < 4; r++) o[c*4+r] = a[r]*b[c*4]+a[4+r]*b[c*4+1]+a[8+r]*b[c*4+2]+a[12+r]*b[c*4+3]; return o; };
  const persp = (fovy, asp, zn, zf) => { const f = 1/Math.tan(fovy/2), nf = 1/(zn-zf);
    return new Float32Array([f/asp,0,0,0, 0,f,0,0, 0,0,(zf+zn)*nf,-1, 0,0,2*zf*zn*nf,0]); };
  const lookAt = (eye, ctr, up) => {
    let zx = eye[0]-ctr[0], zy = eye[1]-ctr[1], zz = eye[2]-ctr[2]; let l = Math.hypot(zx, zy, zz); zx /= l; zy /= l; zz /= l;
    let xx = up[1]*zz-up[2]*zy, xy = up[2]*zx-up[0]*zz, xz = up[0]*zy-up[1]*zx; l = Math.hypot(xx, xy, xz); xx /= l; xy /= l; xz /= l;
    const yx = zy*xz-zz*xy, yy = zz*xx-zx*xz, yz = zx*xy-zy*xx;
    return new Float32Array([xx,yx,zx,0, xy,yy,zy,0, xz,yz,zz,0, -(xx*eye[0]+xy*eye[1]+xz*eye[2]), -(yx*eye[0]+yy*eye[1]+yz*eye[2]), -(zx*eye[0]+zy*eye[1]+zz*eye[2]), 1]); };
  const W = 320, H = 200, near = 16;
  const P = persp(48*Math.PI/180, W/H, near, 400000);
  const view = (eye, at) => mul(P, lookAt(eye, at, [0, 0, 1]));
  // Looking along +y from the origin; a wall across the view at y = 1000.
  const VP = view([0, 0, 0], [0, 1, 0]);

  /* What the GPU would read back: per 8-pixel block, the furthest depth over it, as the
     depth buffer stores it (0..1), for a scene of a wall at y = 1000 that covers the
     screen left of `edgeX` (in NDC) and sky elsewhere. */
  const I = Occ.inv(VP);
  const depthOf = (w) => ((-P[10]*w + P[14])/w + 1)/2;
  const readBack = (edgeX) => {
    const OW = Math.ceil(W/8), OH = Math.ceil(H/8), f = new Float32Array(OW*OH);
    for (let j = 0; j < OH; j++) for (let i = 0; i < OW; i++) {
      let m = 0;
      for (let y = j*8; y < Math.min(H, j*8+8); y++) for (let x = i*8; x < Math.min(W, i*8+8); x++) {
        const nx = (x+0.5)/W*2-1, ny = (y+0.5)/H*2-1;
        if (nx > edgeX) { m = 1; continue; }
        // The wall's distance along this pixel's ray: a point on the ray at w=1, scaled to y=1000.
        const c = [nx, ny, P[14]-P[10], 1];
        const p = [0, 1, 2, 3].map(r => I[r]*c[0]+I[4+r]*c[1]+I[8+r]*c[2]+I[12+r]*c[3]);
        const wy = p[1]/p[3], w = 1000/wy;
        m = Math.max(m, depthOf(w));
      }
      f[j*OW+i] = m;
    }
    return { w: Occ.decode(new Uint8Array(f.buffer), P[10], P[14]), W, H, VP: Float64Array.from(VP), p10: P[10], p14: P[14] };
  };
  const box = (cx, cy, cz, r) => [cx-r, cy-r, cz-r, cx+r, cy+r, cz+r];

  // The bytes are the depth's own bits: decoded, they are the distance it was drawn at.
  { const d = new Float32Array([depthOf(1000), depthOf(37.5), 1]);
    const w = Occ.decode(new Uint8Array(d.buffer), P[10], P[14]);
    t.ok(Math.abs(w[0]-1000) < 0.5 && Math.abs(w[1]-37.5) < 0.01 && w[2] === Infinity, 'decoded distances: ' + Array.from(w).join(', ')); }

  // The same view: behind the wall is hidden, in front is not, and a box at the eye is drawn.
  { const o = Occ.build(readBack(2), VP, near);
    t.ok(o.hidden(box(0, 2000, 0, 50), 0), 'a box behind the wall is hidden');
    t.ok(!o.hidden(box(0, 500, 0, 50), 0), 'a box in front of the wall is drawn');
    t.ok(!o.hidden(box(0, 1010, 0, 30), 0), 'a box through the wall (its near face in front) is drawn');
    t.ok(!o.hidden(box(0, 5, 0, 30), 0), 'a box round the eye is drawn');
    t.ok(!o.hidden(box(0, -2000, 0, 50), 0), 'a box behind the eye is left to the frustum');
    t.ok(o.hidden(box(0, 30000, 0, 3000), 0), 'a big far box wholly behind it is hidden too'); }

  // A wall over the left half only: behind it on the left hidden; on the right, or across its edge, drawn.
  { const o = Occ.build(readBack(0), VP, near);
    t.ok(o.hidden(box(-1200, 3000, 0, 100), 0), 'behind the wall, left: hidden');
    t.ok(!o.hidden(box(1200, 3000, 0, 100), 0), 'right of the wall, where only sky was read: drawn');
    t.ok(!o.hidden(box(0, 3000, 0, 100), 0), 'across the wall\'s edge: drawn');
    t.ok(!o.hidden(box(-60, 3000, 0, 30), 0), 'just past the edge, within a block or so: drawn'); }

  // The camera moved since the read-back: the wall is reprojected, not taken as it was.
  { const src = readBack(0);
    const VP2 = view([300, 0, 0], [300, 1, 0]);   // 300 units to the right, same heading
    const o = Occ.build(src, VP2, near);
    t.ok(o.hidden(box(-1500, 3000, 0, 100), 0), 'moved right: still behind the wall on the left - hidden');
    t.ok(!o.hidden(box(900, 3000, 0, 100), 0), 'moved right: what is now on the screen right of the wall\'s edge is drawn');
    const VP3 = view([0, 0, 0], [0, -1, 0]);   // turned round
    const o3 = Occ.build(src, VP3, near);
    t.ok(!o3.hidden(box(0, -3000, 0, 100), 0), 'turned round: nothing read covers the new view, so nothing is hidden');
    const VP4 = view([0, 1500, 0], [0, 2500, 0]);   // walked through the wall
    t.ok(!Occ.build(src, VP4, near).hidden(box(-800, 3000, 0, 100), 0), 'past the wall: it is behind the eye now and hides nothing'); }

  // The counters the Report reads.
  { const o = Occ.build(readBack(2), VP, near);
    o.hidden(box(0, 2000, 0, 50), 0); o.hidden(box(0, 500, 0, 50), 0);
    t.ok(o.tested === 2 && o.hid === 1, 'tested and hidden counted: ' + o.tested + '/' + o.hid); }
};
