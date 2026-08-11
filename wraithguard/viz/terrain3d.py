"""The cell as a rotatable 3D surface.

A 65x65 grid of numbers describes terrain exactly and conveys its shape to
nobody. This renders the same data as a surface you can turn, which is the one
view that answers "is that a ridge or a trench" without counting.

**Why this is hand-rolled rather than built on a 3D library.** The generated
pages are self-contained by design -- no CDN, no external script -- because the
tool runs offline and ships as a PyInstaller binary, and a page that loses its
script tag when the network is down is worse than one that never had it.
Pulling in Three.js would either break that or add a bundled dependency to
every build for one view. A height *field* is a much smaller problem than
general 3D: the mesh is a regular grid, so quads can be sorted back-to-front
and painted with no depth buffer, no camera library and no shaders. That fits
in the page.

**Everything is exposed.** Shading mode, hillshade on/off, light count, scale
count, sun azimuth and altitude, tint palette and opacity, contours and vertical
exaggeration are all controls on the page. Nothing was replaced to add them: the
defaults reproduce the view as it stood, including the light direction, which
was previously a hard-coded vector and is now the same direction written in
degrees. A control that is disabled greys out rather than vanishing, since a
setting that cannot act is otherwise indistinguishable from one that does
nothing.

**Two shading modes.** Relief is the default; the original *flat* mode -- one
color per face, mixing slope and height -- is one selection away, because a
faceted surface makes the mesh itself visible and "where are the vertices" is
occasionally the question being asked. The **geometry is identical in both**:
the vertical scale is a correctness matter rather than a style, so no shading
setting can change the shape.

**Shaded the way a relief map is.** A greyscale *hillshade* carries the shape
and a hypsometric *tint* carries the elevation, composited over it at around
half opacity. Keeping them as two layers is the point: flat-filling one blended
color per face -- which this did originally -- fuses "which way does this face"
with "how high is it" into a single number, so neither can be read on its own.

Shading is per pixel, not per face. The mesh is 32x32 after sampling, so a face
is tens of pixels across and its edges are plainly visible; interpolating the
normal and the height across each triangle removes them for a few multiplies.
The light is fixed to the *terrain* rather than the camera, so turning the model
turns it under the light like a real object -- a light pinned to the viewer
keeps every slope equally lit however you rotate, which is the one thing
hillshade exists to prevent.

**Multidirectional lighting** (3 or 6 lights) spreads lights evenly around the
compass at the chosen altitude and weights each by how closely it agrees with
the primary azimuth. One light leaves whole faces in flat black where nothing
can be read; several fill those shadows without flattening the relief, because
the primary direction still dominates. Total brightness is unchanged -- the
weights sum to one and every light shares an altitude, so flat ground is lit
identically at one light or six, and only the shadow side moves.

**Multiscale** blends slopes measured over three window widths. A narrow window
describes texture and a wide one describes landform; one radius has to choose,
and whichever it chooses the other is lost. The blend is of the *normals*
rather than of three finished hillshades -- a deviation from the usual
description of the technique, taken because it costs one pass in the per-pixel
loop instead of three, and the two differ only near the terminator, which the
shading floor already softens.

**Contours** are derived in the same pass, from the height already interpolated
at each pixel. The interval is a round number chosen to put about a dozen lines
on the cell, and line width is divided by the local slope so lines stay a
constant width on screen instead of fattening on flat ground. Where the spacing
would fall under three line-widths the lines are dropped rather than allowed to
merge into a smear -- paper maps do the same, and a smear is not a contour.

**Camera controls.** Plain drag rotates (yaw/pitch); shift-drag or right-drag
pans; the scroll wheel zooms. There is no separate "Z axis" control -- this is
a painter's-algorithm canvas, not a real camera with depth, so moving closer
to or further from the surface *is* the zoom control, not a fourth axis next
to it. **Isometric** and **Top down** are buttons because neither can be hit by
dragging: true isometric needs an exact pitch (arcsin of tan 30 degrees, so all
three axes foreshorten equally) and top-down needs an exact right angle.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, cast

from wraithguard import _
from wraithguard.tes3fields.landscape import (
    LAND_VERTEX_SPACING,
    LandscapeDecodeError,
    decode_vertex_heights,
)
from wraithguard.viz import html as h
from wraithguard.viz.palette import TINT_RAMPS, tint_ramp

if TYPE_CHECKING:
    from collections.abc import Mapping, Sequence


class Terrain3DError(Exception):
    """Raised when a terrain surface cannot be rendered."""


#: Drawn at reduced resolution: 65x65 is 4,096 quads per surface, which a 2D
#: canvas can paint but not at interactive frame rates while dragging. Sampling
#: every other vertex quarters the work and loses nothing visible at this size.
_STRIDE = 2

#: How strongly the elevation tint covers the hillshade beneath it. Relief maps
#: put the tint somewhere near half: enough that height reads at a glance, not
#: so much that it washes out the shading that carries the shape. Below about
#: 0.4 the color stops being legible as elevation; above about 0.7 the terrain
#: flattens out and the tint is doing all the work.
_TINT_ALPHA = 0.55

#: Where the sun sits by default, in the terrain's own compass frame: 0 is
#: north, 90 east. South-west at a shallow angle -- which is what this view has
#: always used, now stated as numbers you can change rather than as a hard-coded
#: vector. (The vector's own comment claimed north-west; the vector was right
#: and the comment was wrong, which is exactly the kind of thing that survives
#: until somebody exposes the value.)
_AZIMUTH = 225

#: Solar elevation, in degrees above the horizon.
_ALTITUDE = 39

_SCRIPT = """
(function(){
const D=window.__terrain;
const cv=document.getElementById('surface'),cx=cv.getContext('2d');
let yaw=0.7,pitch=0.55,zoom=D.zoom,panX=0,panY=0,drag=null,dragMode='rotate',which=D.default_surface||0;
let frame=null,blank=null;
let contourStep=0,cLo=0,cSpan=1;

// Every control's current value in one place, seeded from the payload so the
// defaults live in Python next to the reasoning for them rather than being
// spread through the markup.
const S={
  shading:D.shading, lights:D.lights, azimuth:D.azimuth, altitude:D.altitude,
  detail:D.detail,
  hillshade:D.hillshade, palette:D.palette, tint:D.tint_alpha,
  contours:D.contours, exaggeration:D.exaggeration
};

function el(id){return document.getElementById(id);}
function bind(id,key,parse){
  const node=el(id);
  if(!node)return;
  node.addEventListener(node.type==='range'?'input':'change',function(){
    S[key]=parse(node);
    if(node.type==='range'){const out=el(id+'-out');if(out)out.textContent=node.value;}
    syncEnabled();draw();
  });
}
const asNum=function(n){return parseFloat(n.value);};
const asStr=function(n){return n.value;};
const asBool=function(n){return n.checked;};
bind('shading','shading',asStr);
bind('lights','lights',asNum);
bind('detail','detail',asNum);
bind('azimuth','azimuth',asNum);
bind('altitude','altitude',asNum);
bind('hillshade','hillshade',asBool);
bind('palette','palette',asStr);
bind('tint','tint',function(n){return parseFloat(n.value)/100;});
bind('contours','contours',asBool);
bind('exag','exaggeration',asNum);

// A control that cannot affect the picture is worse than an absent one: it
// invites the reader to conclude the setting does nothing.
function syncEnabled(){
  const flat=S.shading==='flat';
  const lit=S.hillshade&&!flat;
  [['lights',!lit],['azimuth',!lit||S.lights>1],['altitude',!lit],
   ['detail',!lit],['hillshade',flat],['palette',flat||S.tint===0],
   ['tint',flat]].forEach(function(pair){
    const node=el(pair[0]);
    if(node){node.disabled=pair[1];
      const wrap=node.closest('.ctl');if(wrap)wrap.classList.toggle('off',pair[1]);}
  });
}

const isoBtn=el('isoView');
if(isoBtn)isoBtn.addEventListener('click',function(){
  // True isometric: yaw 45 degrees and a pitch whose sine is tan(30), so all
  // three axes foreshorten equally. Not reachable by dragging.
  yaw=Math.PI/4;pitch=Math.asin(Math.tan(Math.PI/6));panX=0;panY=0;draw();});
const topBtn=el('topView');
if(topBtn)topBtn.addEventListener('click',function(){
  yaw=0;pitch=Math.PI/2;panX=0;panY=0;draw();});

const btns=document.querySelectorAll('[data-surface]');
btns.forEach(function(b){b.addEventListener('click',function(){
  which=+b.dataset.surface;
  btns.forEach(function(o){o.className=(o===b)?'on':'';});draw();});});

cv.addEventListener('contextmenu',function(e){e.preventDefault();});
cv.addEventListener('mousedown',function(e){
  drag=[e.clientX,e.clientY];
  dragMode=(e.button===2||e.shiftKey)?'pan':'rotate';
  cv.style.cursor=dragMode==='pan'?'move':'grabbing';
});
window.addEventListener('mouseup',function(){drag=null;cv.style.cursor='grab';});
window.addEventListener('mousemove',function(e){
  if(!drag)return;
  const dx=e.clientX-drag[0],dy=e.clientY-drag[1];
  if(dragMode==='pan'){panX+=dx;panY+=dy;}
  else{yaw+=dx*0.01;pitch=Math.max(0.08,Math.min(1.5,pitch+dy*0.01));}
  drag=[e.clientX,e.clientY];draw();
});
cv.addEventListener('wheel',function(e){
  e.preventDefault();
  zoom=Math.max(1.5,Math.min(40,zoom*Math.exp(-e.deltaY*0.0015)));
  draw();
},{passive:false});

const resetBtn=el('resetView');
if(resetBtn)resetBtn.addEventListener('click',function(){
  yaw=0.7;pitch=0.55;zoom=D.zoom;panX=0;panY=0;
  Object.keys(D.defaults).forEach(function(k){S[k]=D.defaults[k];});
  ['shading','lights','detail','azimuth','altitude','palette','exag'].forEach(function(id){
    const node=el(id);if(!node)return;
    const key=id==='exag'?'exaggeration':id;
    node.value=String(S[key]);
    const out=el(id+'-out');if(out)out.textContent=node.value;
  });
  ['hillshade','contours'].forEach(function(id){const n=el(id);if(n)n.checked=S[id];});
  const tn=el('tint');
  if(tn){tn.value=String(Math.round(S.tint*100));const o=el('tint-out');if(o)o.textContent=tn.value;}
  syncEnabled();draw();
});

// Height is divided by the world-unit distance between two vertices, so one
// unit up the screen is the same distance as one unit across it. Normalising
// instead -- which this once did -- draws every cell the same height whatever
// its relief: right from directly overhead, a cliff from anywhere else.
function project(x,y,z,n,lo){
  const cxs=Math.cos(yaw),sxs=Math.sin(yaw);
  const u=(x-(n-1)/2),v=(y-(n-1)/2);
  const rx=u*cxs-v*sxs, ry=u*sxs+v*cxs;
  const h=((z-lo)/D.units_per_step)*S.exaggeration;
  return [cv.width/2+panX+rx*zoom, cv.height/2+panY+ry*zoom*Math.sin(pitch)-h*Math.cos(pitch)*zoom];
}

// Vertex normals by central difference, in a space where one grid step and one
// unit of height are the same length. Exaggeration is applied here too: a
// stretched surface must be shaded as the shape on screen, or the light comes
// from a direction the geometry does not agree with.
function normalsFor(g,scale,radius){
  const n=g.length,out=[],r=radius||1;
  for(let y=0;y<n;y++){
    const row=[];
    for(let x=0;x<n;x++){
      const xL=Math.max(0,x-r),xR=Math.min(n-1,x+r);
      const yD=Math.max(0,y-r),yU=Math.min(n-1,y+r);
      const dzdx=(g[y][xR]-g[y][xL])/((xR-xL)||1)*scale;
      const dzdy=(g[yU][x]-g[yD][x])/((yU-yD)||1)*scale;
      const nx=-dzdx,ny=-dzdy,len=Math.sqrt(nx*nx+ny*ny+1)||1;
      row.push(nx/len,ny/len,1/len);
    }
    out.push(row);
  }
  return out;
}

// MULTISCALE: the same slope measured over a wider window describes the
// landform; over a narrow one it describes the texture. One radius has to
// choose, and whichever it chooses the other is lost -- a fine radius turns a
// broad valley into noise, a coarse one erases every gully. Blending three
// radii keeps both, weighted toward the finest so detail still leads.
//
// The blend is of the *normals* rather than of three finished hillshades. That
// is a deviation from how the technique is usually described, and it is here
// because it costs one pass instead of three in the per-pixel loop; the results
// differ only where a face is near the terminator, which the shading floor
// already softens.
const SCALE_RADII=[1,2,4], SCALE_WEIGHTS=[0.55,0.30,0.15];
function blendedNormals(g,scale){
  const layers=SCALE_RADII.map(function(r){return normalsFor(g,scale,r);});
  const out=[];
  for(let y=0;y<layers[0].length;y++){
    const row=new Array(layers[0][y].length);
    for(let i=0;i<row.length;i+=3){
      let nx=0,ny=0,nz=0;
      for(let k=0;k<layers.length;k++){
        const src=layers[k][y],w=SCALE_WEIGHTS[k];
        nx+=src[i]*w;ny+=src[i+1]*w;nz+=src[i+2]*w;
      }
      const len=Math.sqrt(nx*nx+ny*ny+nz*nz)||1;
      row[i]=nx/len;row[i+1]=ny/len;row[i+2]=nz/len;
    }
    out.push(row);
  }
  return out;
}

// Lights, in the terrain's own compass frame: azimuth 0 is north (+y), 90 is
// east (+x), and altitude is the angle above the horizon. Fixed to the terrain
// rather than the camera, so turning the model turns it under the light like a
// real object -- a camera-fixed light keeps every slope equally lit however you
// rotate, which defeats the point.
//
// MULTIDIRECTIONAL puts six lights evenly around the compass, all at the chosen
// altitude, and weights each by how closely it agrees with the primary
// azimuth. One light leaves whole faces in flat black where nothing can be
// read; six fill those shadows without flattening the relief, because the
// primary direction still dominates. It is the standard cartographic answer to
// exactly that problem, and the weighting is what keeps it from becoming
// ambient light with no shape at all.
function lightSet(){
  const alt=S.altitude*Math.PI/180, ca=Math.cos(alt), sa=Math.sin(alt);
  const primary=S.azimuth*Math.PI/180;
  const out=[];
  if(S.lights<=1){
    out.push([ca*Math.sin(primary),ca*Math.cos(primary),sa,1]);
    return out;
  }
  let total=0;
  for(let i=0;i<S.lights;i++){
    const az=primary+i*2*Math.PI/S.lights;
    const w=0.35+0.65*(0.5+0.5*Math.cos(az-primary));
    out.push([ca*Math.sin(az),ca*Math.cos(az),sa,w]);
    total+=w;
  }
  for(const l of out)l[3]/=total;
  return out;
}

let lights=lightSet(),lightsKey='';
function currentLights(){
  const key=S.lights+':'+S.azimuth+':'+S.altitude;
  if(lightsKey!==key){lights=lightSet();lightsKey=key;}
  return lights;
}

let normals=null,normalsKey='';
function currentNormals(g){
  const key=which+':'+S.exaggeration+':'+S.detail;
  if(normalsKey!==key){
    const scale=S.exaggeration/D.units_per_step;
    normals=S.detail>1?blendedNormals(g,scale):normalsFor(g,scale,1);
    normalsKey=key;
  }
  return normals;
}

//: Half a contour line's width, in screen pixels.
const HALFLINE=0.75;

// A "nice" contour interval -- 1, 2 or 5 times a power of ten -- chosen so the
// cell carries roughly a dozen lines. Contours are only readable at a round
// interval: 137 units apart is a texture, 100 apart is a measurement.
function niceInterval(span){
  const rough=span/12, mag=Math.pow(10,Math.floor(Math.log10(rough||1)));
  const n=rough/mag;
  return (n<1.5?1:n<3.5?2:n<7.5?5:10)*mag;
}

// Rasterise one triangle, interpolating height and normal barycentrically.
// Painter's algorithm: quads arrive back-to-front, so a later write is in
// front. A regular height grid cannot self-intersect, which makes that exact.
function triangle(buf,W,H,p0,p1,p2,a0,a1,a2,flat){
  const minX=Math.max(0,Math.floor(Math.min(p0[0],p1[0],p2[0])));
  const maxX=Math.min(W-1,Math.ceil(Math.max(p0[0],p1[0],p2[0])));
  const minY=Math.max(0,Math.floor(Math.min(p0[1],p1[1],p2[1])));
  const maxY=Math.min(H-1,Math.ceil(Math.max(p0[1],p1[1],p2[1])));
  if(minX>maxX||minY>maxY)return;
  const x0=p0[0],y0=p0[1],x1=p1[0],y1=p1[1],x2=p2[0],y2=p2[1];
  const area=(x1-x0)*(y2-y0)-(x2-x0)*(y1-y0);
  if(!area)return;                       // degenerate: edge-on, nothing to fill
  const inv=1/area;
  const ramp=D.palettes[S.palette],last=ramp.length-1;
  const alpha=S.tint, shaded=S.hillshade&&!flat, L=currentLights();
  for(let py=minY;py<=maxY;py++){
    for(let px=minX;px<=maxX;px++){
      const cxp=px+0.5,cyp=py+0.5;
      const w0=((x1-cxp)*(y2-cyp)-(x2-cxp)*(y1-cyp))*inv;
      const w1=((x2-cxp)*(y0-cyp)-(x0-cxp)*(y2-cyp))*inv;
      const w2=1-w0-w1;
      // A shared edge can leave a pixel marginally outside both triangles,
      // which shows as a dark hairline crack across a continuous surface.
      if(w0<-1e-6||w1<-1e-6||w2<-1e-6)continue;
      const t=w0*a0[0]+w1*a1[0]+w2*a2[0];
      const nx=w0*a0[1]+w1*a1[1]+w2*a2[1];
      const ny=w0*a0[2]+w1*a1[2]+w2*a2[2];
      const nz=w0*a0[3]+w1*a1[3]+w2*a2[3];
      const nl=Math.sqrt(nx*nx+ny*ny+nz*nz)||1;
      let r,gc,b;
      if(flat){
        r=flat[0];gc=flat[1];b=flat[2];
      }else{
        let shade=255;
        if(shaded){
          let lit=0;
          for(let i=0;i<L.length;i++){
            const d=(nx*L[i][0]+ny*L[i][1]+nz*L[i][2])/nl;
            lit+=(d>0?d:0)*L[i][3];
          }
          // A floor, so a face turned away is dark rather than black: unlit
          // ground still has shape worth seeing.
          shade=38+lit*205;
        }
        if(alpha>0){
          const c=ramp[Math.round((t<0?0:t>1?1:t)*last)];
          r=shade+(c[0]-shade)*alpha;
          gc=shade+(c[1]-shade)*alpha;
          b=shade+(c[2]-shade)*alpha;
        }else{r=shade;gc=shade;b=shade;}
      }
      if(contourStep>0){
        const world=cLo+t*cSpan, f=world/contourStep;
        const dh=Math.abs(f-Math.round(f))*contourStep;
        // Divided by the slope so lines are a constant width on screen rather
        // than fat on flat ground and invisible on a cliff. nz is the normal's
        // vertical component, so sqrt(1-nz^2)/nz is the surface gradient.
        const nzc=nz/nl, grad=Math.sqrt(1-nzc*nzc)/(nzc>1e-4?nzc:1e-4);
        if(grad>1e-3){
          const perHeight=S.exaggeration/D.units_per_step*zoom;
          // Dropped where neighbouring lines would land closer than three line
          // widths apart: they merge into a smear over exactly the cliffs the
          // hillshade is describing. Paper maps drop them for the same reason.
          const spacing=contourStep*perHeight/grad;
          if(spacing>6*HALFLINE&&dh*perHeight/grad<HALFLINE){
            const k=0.55;            // darken rather than paint a flat color,
            r*=k;gc*=k;b*=k;         // so the tint still reads through the line
          }
        }
      }
      const o=(py*W+px)*4;
      buf[o]=r;buf[o+1]=gc;buf[o+2]=b;buf[o+3]=255;
    }
  }
}

function draw(){
  const g=D.surfaces[which].grid,n=g.length;
  let lo=Infinity,hi=-Infinity;
  for(const r of g)for(const z of r){if(z<lo)lo=z;if(z>hi)hi=z;}
  const span=(hi-lo)||1;
  const nrm=currentNormals(g);
  cLo=lo;cSpan=span;
  contourStep=S.contours?niceInterval(span):0;
  const W=cv.width,H=cv.height;
  if(!frame||frame.width!==W||frame.height!==H){
    frame=cx.createImageData(W,H);
    // Built once and memcpy'd per frame. Clearing 800,000 pixels in a JS loop
    // every mousemove costs more than rasterising the terrain does.
    blank=new Uint8ClampedArray(frame.data.length);
    for(let i=0;i<blank.length;i+=4){blank[i]=26;blank[i+1]=29;blank[i+2]=35;blank[i+3]=255;}
  }
  const buf=frame.data;
  buf.set(blank);
  const flatMode=S.shading==='flat';
  const quads=[];
  for(let y=0;y<n-1;y++)for(let x=0;x<n-1;x++){
    const zs=[g[y][x],g[y][x+1],g[y+1][x+1],g[y+1][x]];
    const pts=[project(x,y,zs[0],n,lo),project(x+1,y,zs[1],n,lo),
               project(x+1,y+1,zs[2],n,lo),project(x,y+1,zs[3],n,lo)];
    const ij=[[y,x],[y,x+1],[y+1,x+1],[y+1,x]];
    const attrs=ij.map(function(pos,k){
      const b=pos[1]*3;
      return [(zs[k]-lo)/span,nrm[pos[0]][b],nrm[pos[0]][b+1],nrm[pos[0]][b+2]];
    });
    quads.push({pts:pts,attrs:attrs,d:pts[0][1]+pts[1][1]+pts[2][1]+pts[3][1],
                slope:Math.abs(zs[0]-zs[2]),z:(zs[0]+zs[2])/2});
  }
  quads.sort(function(a,b){return a.d-b.d;});
  const maxs=flatMode?quads.reduce(function(m,q){return Math.max(m,q.slope);},1):1;
  for(const q of quads){
    let flat=null;
    if(flatMode){
      const ft=(q.z-lo)/span, s=1-Math.min(1,q.slope/maxs)*0.55;
      flat=[(60+150*ft)*s,(75+140*ft)*s,(85+110*ft)*s];
    }
    triangle(buf,W,H,q.pts[0],q.pts[1],q.pts[2],q.attrs[0],q.attrs[1],q.attrs[2],flat);
    triangle(buf,W,H,q.pts[0],q.pts[2],q.pts[3],q.attrs[0],q.attrs[2],q.attrs[3],flat);
  }
  cx.putImageData(frame,0,0);
  el('range').textContent=
    D.labels.range.replace('%(lo)s',Math.round(lo)).replace('%(hi)s',Math.round(hi))
    +(S.exaggeration===1?'':'  '+D.labels.exaggerated.replace('%(times)s',S.exaggeration))
    +(contourStep?'  '+D.labels.contours.replace('%(step)s',Math.round(contourStep)):'');
}
syncEnabled();
draw();
})();
"""


def _sample(grid: Sequence[Sequence[float]], stride: int) -> list[list[float]]:
    """Reduce a height grid by taking every ``stride``-th vertex.

    Args:
        grid: The full-resolution grid.
        stride: Sampling interval; 1 returns the grid unchanged.

    Returns:
        The reduced grid, always keeping at least a 2x2 surface so the
        renderer has something to draw.
    """
    if stride <= 1:
        return [list(row) for row in grid]
    out = [[float(v) for v in row[::stride]] for row in grid[::stride]]
    return out if len(out) >= 2 else [list(row) for row in grid]


def _is_encoded(value: object) -> bool:
    """Whether a surface value is encoded ``(data, offset)`` vs a decoded grid.

    Args:
        value: A ``build_terrain_3d`` surface value.

    Returns:
        ``True`` for the ``(vertex_heights.data, offset)`` pair a plugin ships,
        ``False`` for an already-decoded 65x65 grid.
    """
    return isinstance(value, tuple) and len(value) == 2 and isinstance(value[0], (str, bytes))


def build_terrain_3d(
    surfaces: Mapping[str, tuple[str | bytes, float] | Sequence[Sequence[float]]],
    *,
    cell_label: str = "",
) -> str:
    """Render one or more plugins' terrain as a rotatable, pannable 3D surface.

    Args:
        surfaces: Plugin filename to ``(vertex_heights.data, offset)``, in
            load order. Give more than one to make them switchable in place,
            which is what makes a difference in shape obvious; the page opens
            on the last one (the winner, i.e. the shape actually in the
            game), with the first labelled as the base/vanilla version for
            consistency with :mod:`~wraithguard.viz.heightdelta` and
            :mod:`~wraithguard.viz.pathgrid`.
        cell_label: Optional cell description.

    Returns:
        A complete HTML document.

    Raises:
        Terrain3DError: If no surface could be decoded.
    """
    decoded: list[dict[str, object]] = []
    failures: list[str] = []
    for name, value in surfaces.items():
        # A surface is either the encoded ``(vertex_heights.data, offset)`` a
        # plugin ships, or an already-decoded 65x65 grid -- the merge-strategy
        # preview computes the latter and hands them in directly rather than
        # round-tripping them back through VHGT encoding.
        try:
            if _is_encoded(value):
                data, offset = cast("tuple[str | bytes, float]", value)
                grid: Sequence[Sequence[float]] = decode_vertex_heights(data, offset)
            else:
                grid = cast("Sequence[Sequence[float]]", value)
        except LandscapeDecodeError as exc:
            failures.append(f"{name}: {exc}")
            continue
        decoded.append({"name": name, "grid": _sample(grid, _STRIDE)})

    if not decoded:
        detail = "; ".join(failures) if failures else "no landscape data was supplied"
        raise Terrain3DError(f"no terrain could be decoded ({detail})")

    # One source of truth for every control's starting value. The payload
    # carries it twice on purpose: once flat, which is what the script reads at
    # startup, and once under "defaults", which is what Reset restores to. The
    # keys here are the script's own state keys, so Reset needs no mapping
    # table -- a mapping table between two lists of setting names is precisely
    # the thing that goes stale when a tenth control is added.
    defaults: dict[str, object] = {
        "exaggeration": 1.0,
        "shading": "relief",
        "hillshade": True,
        "lights": 1,
        "detail": 1,
        "azimuth": _AZIMUTH,
        "altitude": _ALTITUDE,
        "palette": "hypsometric",
        "tint": _TINT_ALPHA,
        "contours": True,
    }
    payload = {
        **defaults,
        "defaults": defaults,
        "tint_alpha": _TINT_ALPHA,
        "surfaces": decoded,
        "default_surface": len(decoded) - 1,
        "zoom": 8.0,
        # World units between adjacent vertices *as sampled*: the stride
        # widens the horizontal step, and a renderer dividing height by the
        # unsampled spacing would exaggerate by exactly that factor.
        "units_per_step": LAND_VERTEX_SPACING * _STRIDE,
        # Pre-split into channel triples: the rasteriser reads these once per
        # pixel, and parsing "#rrggbb" there would be the most expensive thing
        # in the loop by a wide margin.
        "palettes": {
            name: [
                [int(color[1:3], 16), int(color[3:5], 16), int(color[5:7], 16)]
                for color in tint_ramp(name)
            ]
            for name in TINT_RAMPS
        },
        "labels": {
            "range": _("Height range: %(lo)s to %(hi)s units"),
            "exaggerated": _("(vertical exaggerated %(times)sx)"),
            "contours": _("contours every %(step)s units"),
        },
    }
    buttons = "".join(
        f'<button data-surface="{index}" class="{"on" if index == len(decoded) - 1 else ""}">'
        + (
            h.escape(_("Base: %(plugin)s") % {"plugin": surface["name"]})
            if index == 0 and len(decoded) > 1
            else (
                h.escape(_("Winner: %(plugin)s") % {"plugin": surface["name"]})
                if index == len(decoded) - 1 and len(decoded) > 1
                else h.escape(str(surface["name"]))
            )
        )
        + "</button>"
        for index, surface in enumerate(decoded)
    )

    def select(control_id: str, label: str, options: Sequence[tuple[object, str]]) -> str:
        """Render one labelled dropdown.

        Args:
            control_id: The element id the script binds to.
            label: The visible label.
            options: ``(value, caption)`` pairs; the first is preselected only
                if it matches the payload default for that control.

        Returns:
            The control markup.
        """
        default = defaults.get(control_id if control_id != "exag" else "exaggeration")
        body = "".join(
            f'<option value="{value}"{" selected" if value == default else ""}>'
            f"{h.escape(caption)}</option>"
            for value, caption in options
        )
        return (
            f'<span class="ctl"><label for="{control_id}">{h.escape(label)}</label>'
            f'<select id="{control_id}">{body}</select></span>'
        )

    def slider(control_id: str, label: str, low: int, high: int, value: int) -> str:
        """Render one labelled slider with a live numeric readout.

        A slider without its number is unusable for anything you might want to
        repeat or describe; azimuth especially is a value people quote.

        Args:
            control_id: The element id the script binds to.
            label: The visible label.
            low: Minimum.
            high: Maximum.
            value: Starting value.

        Returns:
            The control markup.
        """
        return (
            f'<span class="ctl"><label for="{control_id}">{h.escape(label)}</label>'
            f'<input type="range" id="{control_id}" min="{low}" max="{high}" value="{value}">'
            f'<output id="{control_id}-out">{value}</output></span>'
        )

    def check(control_id: str, label: str, *, on: bool) -> str:
        """Render one labelled checkbox.

        Args:
            control_id: The element id the script binds to.
            label: The visible label.
            on: Whether it starts checked.

        Returns:
            The control markup.
        """
        return (
            f'<span class="ctl"><label><input type="checkbox" id="{control_id}"'
            f'{" checked" if on else ""}> {h.escape(label)}</label></span>'
        )

    reset_button = f'<button id="resetView" type="button">{h.escape(_("Reset"))}</button>'
    # Two viewpoints worth a button because neither can be reached by dragging
    # with any accuracy: true isometric needs an exact pitch, and straight down
    # needs an exact 90 degrees.
    view_buttons = (
        f'<button id="isoView" type="button">{h.escape(_("Isometric"))}</button>'
        f'<button id="topView" type="button">{h.escape(_("Top down"))}</button>'
    )
    controls = "".join(
        (
            select(
                "shading",
                _("Shading:"),
                [("relief", _("Relief")), ("flat", _("Flat facets"))],
            ),
            check("hillshade", _("Hillshade"), on=True),
            select(
                "lights",
                _("Lights:"),
                # Six is the useful multidirectional count: enough to fill the
                # shadow side without the weighting collapsing into ambient
                # light, and it divides the compass into whole 60-degree steps.
                [(1, _("Single")), (3, _("3-way")), (6, _("6-way"))],
            ),
            select(
                "detail",
                _("Scales:"),
                # Multiscale: the same slope sampled over a wider window shows
                # the landform, a narrow one shows the texture. Blending them
                # keeps both, which one radius cannot.
                [(1, _("Single")), (3, _("Multiscale"))],
            ),
            slider("azimuth", _("Sun azimuth:"), 0, 359, _AZIMUTH),
            slider("altitude", _("Sun altitude:"), 1, 90, _ALTITUDE),
            select(
                "palette",
                _("Tint:"),
                [
                    ("hypsometric", _("Hypsometric")),
                    ("rainbow", _("Rainbow")),
                    ("grey", _("Greyscale")),
                ],
            ),
            slider("tint", _("Tint %:"), 0, 100, round(_TINT_ALPHA * 100)),
            check("contours", _("Contours"), on=True),
            # 1x is true scale and the default. The others are offered because a
            # cell with twenty units of relief is genuinely almost flat, and
            # "almost flat" is hard to read as a shape -- but exaggeration is a
            # thing you turn on knowingly, and the readout says so while it is on.
            select(
                "exag",
                _("Vertical:"),
                [(n, _("%(times)sx") % {"times": n}) for n in (1, 2, 5, 10, 25)],
            ),
        )
    )
    note = (
        _(
            "Drag to rotate, shift/right-drag to pan, scroll to zoom. Shading follows "
            "slope, which reads as terrain better than height does. Heights are drawn "
            "to the same scale as the ground, so a slope here is the slope in game."
        )
        if len(decoded) < 2
        else _(
            "Drag to rotate, shift/right-drag to pan, scroll to zoom; switch plugins to "
            "see the same cell as each one leaves it. Shading follows slope. Heights are "
            "drawn to the same scale as the ground, so a slope here is the slope in game."
        )
    )
    warning = (
        f'<div class="empty">{h.escape(_("Some records could not be decoded: %(detail)s") % {"detail": "; ".join(failures)})}</div>'
        if failures
        else ""
    )

    body = (
        h.summary({_("Cell"): cell_label or _("(unknown)"), _("Surfaces"): len(decoded)})
        + h.card(
            _("Terrain surface"),
            warning
            + f'<div class="tabs">{buttons}</div>'
            + f'<div class="tabs">{view_buttons}{reset_button}</div>'
            + f'<div class="panel">{controls}</div>'
            + '<canvas id="surface" width="1100" height="740"></canvas>'
            + f'<div class="legend"><span id="range"></span><span>{h.escape(note)}</span></div>',
        )
        + "<style>.panel{display:flex;flex-wrap:wrap;gap:6px 16px;align-items:center;"
        "margin:0 0 12px;padding:8px 10px;border:1px solid var(--line);border-radius:4px;"
        "background:#20242c;font-size:12.5px}"
        ".panel .ctl{display:flex;align-items:center;gap:5px}"
        ".panel .ctl.off{opacity:.4}"
        ".panel label{color:var(--dim)}"
        ".panel select{background:var(--panel);color:var(--ink);border:1px solid #333945;"
        "border-radius:3px;padding:2px 4px;font:inherit}"
        ".panel input[type=range]{width:104px;vertical-align:middle}"
        ".panel output{color:var(--ink);min-width:2.4em;display:inline-block;"
        "font-variant-numeric:tabular-nums}"
        ".tabs{margin-bottom:10px}.tabs button{background:#2c313a;color:#d7dae0;"
        "border:1px solid #333945;border-radius:4px;padding:5px 10px;margin-right:6px;"
        "cursor:pointer;font:inherit}.tabs button.on{background:#3d4450;border-color:#7cc5ff;"
        "color:#fff}canvas{background:#1a1d23;border-radius:4px;max-width:100%;cursor:grab}</style>"
        + f"<script>window.__terrain={h.script_json(payload)};</script><script>{_SCRIPT}</script>"
    )
    return h.page(
        _("Terrain surface"),
        _("The same 65x65 height grid the text view lists, as a shape you can turn."),
        body,
    )
