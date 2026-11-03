/* =====================================================================================
   Wraithguard: the Land view's brush engine (67_wg_land.js calls it): the falloff curves,
   the height tools' algorithms, and how a stroke turns into dabs.

   - Falloff: smooth (cosine), linear, sphere, sharp, gaussian or flat, over the feathered
     rim of the brush.
   - Tools, each a function of the heights around the dab (`field`: read and write a world
     vertex's height): raise, lower, smooth (average, gaussian, or wide: a blur as wide as a
     share of the brush), sharpen (the inverse: the height pushed away from its
     neighbours'), flatten (to where the stroke began, a fixed height, the sea, or the
     brush's average; both ways, only up, or only down), ramp (a slope from where the stroke
     began to the brush, its sides by the falloff), noise (fractal value noise: scale,
     octaves, roughness, seed), terrace (steps of a height, their edges hard or soft) and
     erode (thermal: material slides down slopes steeper than the talus angle).
   - The brush's shape: a circle, square or diamond, stretched by an aspect ratio and turned
     by an angle (GIMP's brush shape, aspect ratio and angle).
   - The stroke: a click gives one dab at the click intensity; a drag a dab every spacing
     (a share of the radius) along the path; held still, dabs at the hold rate (a second's
     worth spread over the frames: the airbrush's build-up). Through the stroke its strength
     rises over the attack and falls with the decay (its half-life in seconds), and fades
     over a distance as GIMP's paint dynamics fade it (none, or repeating as a loop, a
     sawtooth or a triangle). Velocity can strengthen a fast stroke or weaken it, a random
     share varies each dab, and jitter scatters them across the path. Smooth stroke (GIMP's
     stabilizer) follows the pointer at a weight, so a shaky hand draws a clean line. And
     strokes are incremental, or capped: no vertex moves further in one stroke than the
     cap however many dabs cross it (GIMP's non-incremental mode), so layered strokes come
     out even. Alt inverts (raise and lower, smooth and sharpen), Shift smooths.

   Written for this page from the usual terrain-editor and paint-program techniques (GIMP's
   brush options and paint dynamics as the reference for what to offer); no code from either. Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
const WgLandBrush={
  FALLOFFS:[['smooth','Smooth'],['linear','Linear'],['sphere','Sphere'],['sharp','Sharp'],['gauss','Gaussian'],['flat','Flat']],
  /** The weight at `t` of the rim (0 at its inner edge, 1 at the brush's edge). */
  falloff(kind, t){
    if(!(t>0)) return 1;
    if(t>=1) return 0;
    switch(kind){
      case 'linear': return 1-t;
      case 'sphere': return Math.sqrt(1-t*t);
      case 'sharp': return (1-t)*(1-t);
      case 'gauss': return Math.max(0, (Math.exp(-4.5*t*t)-Math.exp(-4.5))/(1-Math.exp(-4.5)));
      case 'flat': return 1;
      default: return 0.5+0.5*Math.cos(Math.PI*t);
    }
  },
  SHAPES:[['circle','Circle'],['square','Square'],['diamond','Diamond']],
  /** How far (dx, dy) is from the centre in the brush's own measure: its shape, stretched
   *  along its angle by the aspect ratio (1 round, 0.1 a narrow blade). */
  dist(dx, dy, o){
    const a=(o && o.angle||0)*Math.PI/180, ca=Math.cos(a), sa=Math.sin(a);
    let u=dx*ca+dy*sa, v=(-dx*sa+dy*ca)/Math.max(0.05, Math.min(1, o && o.aspect!=null? o.aspect : 1));
    u=Math.abs(u); v=Math.abs(v);
    switch(o && o.shape){
      case 'square': return Math.max(u, v);
      case 'diamond': return (u+v)/Math.SQRT2;
      default: return Math.hypot(u, v);
    }
  },
  /** The weight of a point `d` from the centre of a brush of radius `r`, feathered by `f` %. */
  weight(kind, d, r, f){
    if(d>r) return 0;
    const inner=r*(1-f/100);
    return d<=inner || r<=inner? 1 : this.falloff(kind, (d-inner)/(r-inner));
  },

  /* ---- noise: value noise on a hashed lattice, summed in octaves ---------------------- */
  hash(x, y, seed){
    let h=(x|0)*374761393+(y|0)*668265263+(seed|0)*2147483647;
    h=(h^(h>>>13))*1274126177;
    h=h^(h>>>16);
    return (h>>>0)/4294967295;
  },
  value(x, y, seed){
    const x0=Math.floor(x), y0=Math.floor(y), fx=x-x0, fy=y-y0;
    const sx=fx*fx*(3-2*fx), sy=fy*fy*(3-2*fy);
    const a=this.hash(x0,y0,seed), b=this.hash(x0+1,y0,seed), c=this.hash(x0,y0+1,seed), d=this.hash(x0+1,y0+1,seed);
    return (a+(b-a)*sx)+((c+(d-c)*sx)-(a+(b-a)*sx))*sy;
  },
  /** -1..1 fractal noise at world (x, y). */
  fbm(x, y, o){
    let sum=0, amp=1, norm=0, f=1/Math.max(1, o.scale||1024);
    for(let i=0;i<Math.max(1, Math.min(8, o.octaves|0||1));i++){
      sum+=amp*(this.value(x*f, y*f, (o.seed|0)+i*101)*2-1); norm+=amp;
      amp*=Math.max(0, Math.min(1, o.rough==null? 0.5 : o.rough)); f*=2;
    }
    return norm? sum/norm : 0;
  },

  /* ---- the tools ---------------------------------------------------------------------- */

  /** One dab of a height tool. `field`: {get(vx, vy), set(vx, vy, h)} over world vertices
   *  (STEP units apart); `verts` the brush's vertices [vx, vy, w ...]; `p` the tool's
   *  settings and the stroke's: {tool, amount (units), k (0..1, the share of the way to a
   *  target a dab goes), ...}. Reads the surface once and writes after, so a dab never
   *  reads its own changes. Answers the changes [vx, vy, h ...]. */
  dab(field, verts, p){
    const out=[], get=(x,y)=>field.get(x,y);
    const avg=(x, y, rad)=>{
      let s=0, n=0;
      for(let j=-rad;j<=rad;j++) for(let i=-rad;i<=rad;i++){ if(!i && !j && rad>0) continue; const v=get(x+i,y+j); if(v!=null){ s+=v; n++; } }
      return n? s/n : null;
    };
    const G=[[1,4,6,4,1],[4,16,24,16,4],[6,24,36,24,6],[4,16,24,16,4],[1,4,6,4,1]];
    const gauss=(x, y)=>{
      let s=0, n=0;
      for(let j=-2;j<=2;j++) for(let i=-2;i<=2;i++){ const v=get(x+i,y+j); if(v!=null){ s+=v*G[j+2][i+2]; n+=G[j+2][i+2]; } }
      return n? s/n : null;
    };
    const tool=p.tool, k=Math.max(0, Math.min(1, p.k));
    for(let i=0;i<verts.length;i+=3){
      const vx=verts[i], vy=verts[i+1], w=verts[i+2], h=get(vx, vy);
      if(h==null || !(w>0)) continue;
      let to=h;
      switch(tool){
        case 'raise': to=h+p.amount*w; break;
        case 'lower': to=h-p.amount*w; break;
        case 'smooth': {
          const a=p.smooth==='gauss'? gauss(vx,vy) : p.smooth==='wide'? avg(vx, vy, Math.max(1, p.wide|0)) : avg(vx, vy, 1);
          if(a!=null) to=h+(a-h)*w*k;
          break;
        }
        case 'sharpen': { const a=avg(vx, vy, 1); if(a!=null) to=h+(h-a)*w*k; break; }
        case 'flatten': {
          if(p.target==null) break;
          const d=(p.target-h)*w*k;
          if((p.only==='up' && d<0) || (p.only==='down' && d>0)) break;
          to=h+d; break;
        }
        case 'ramp': {
          const r=p.ramp; if(!r) break;
          const ux=r.x1-r.x0, uy=r.y1-r.y0, L2=ux*ux+uy*uy;
          const t=L2>0? Math.max(0, Math.min(1, ((vx*p.step-r.x0)*ux+(vy*p.step-r.y0)*uy)/L2)) : 0;
          const px=r.x0+ux*t, py=r.y0+uy*t, side=Math.hypot(vx*p.step-px, vy*p.step-py);
          const ws=this.weight(p.falloff, side, p.radius, p.feather);
          to=h+((r.z0+(r.z1-r.z0)*t)-h)*ws*k; break;
        }
        case 'noise': to=h+p.amount*w*this.fbm(vx*p.step, vy*p.step, p.noise||{}); break;
        case 'terrace': {
          const s=Math.max(1, p.terrace||128), base=Math.floor(h/s)*s, f=(h-base)/s;
          const e=Math.max(0, Math.min(1, p.edge==null? 0.2 : p.edge));
          // Flat treads, the riser kept within the edge share of each step.
          const g=e<=0? 0 : f<1-e? 0 : (f-(1-e))/e;
          to=h+((base+s*g*g*(3-2*g))-h)*w*k; break;
        }
        case 'erode': {
          // Thermal: the share of the drop past the talus that slides to the lowest neighbour.
          const tal=Math.tan(Math.max(1, Math.min(89, p.talus||35))*Math.PI/180)*p.step;
          let low=null, drop=0;
          for(const [dx,dy] of [[1,0],[-1,0],[0,1],[0,-1]]){ const v=get(vx+dx,vy+dy); if(v!=null && h-v>drop){ drop=h-v; low=v; } }
          if(low!=null && drop>tal) to=h-(drop-tal)*0.5*w*k;
          else{
            let high=0;
            for(const [dx,dy] of [[1,0],[-1,0],[0,1],[0,-1]]){ const v=get(vx+dx,vy+dy); if(v!=null && v-h>tal) high=Math.max(high, v-h-tal); }
            to=h+high*0.25*w*k;
          }
          break;
        }
      }
      if(to!==h && isFinite(to)) out.push(vx, vy, to);
    }
    return out;
  },

  /* ---- the stroke --------------------------------------------------------------------- */

  /** A stroke begun at `now` (ms) with these settings (copied: a setting changed mid-stroke
   *  waits for the next). */
  begin(now, s){
    return {t0:now, last:null, travel:0, dist:0, moved:now, cursor:null, smooth:null, speed:0, acc:new Map(),
      seed:(now*7919)|0, n:0, s:Object.assign({}, s)};
  },
  /** A repeatable 0..1 for the stroke's n-th draw. */
  rand(S){ S.n++; return this.hash(S.n, S.seed, 7); },
  /** The fade over the distance travelled: GIMP's fade length and repeat. */
  fade(S){
    const s=S.s, L=+s.fadeLen||0;
    if(!(L>0)) return 1;
    let f=S.dist/L;
    switch(s.fadeRepeat){
      case 'loop': f=f%1; break;
      case 'saw': f=f%1; return f;   // rising again each length
      case 'tri': f=f%2; return f<1? 1-f : f-1;
      default: if(f>=1) return 0;
    }
    return 1-f;
  },
  /** The stroke's strength at `now`: the attack's rise, the decay's half-life, the fade. */
  envelope(S, now){
    const s=S.s, t=Math.max(0, (now-S.t0)/1000);
    let m=1;
    if(s.attack>0) m*=Math.min(1, t/s.attack);
    if(s.decay>0) m*=Math.pow(0.5, t/s.decay);
    return m*this.fade(S);
  },
  /** What a dab's strength becomes from the stroke's speed (`velocity` -100..100: a fast
   *  stroke weaker or stronger, as GIMP maps velocity) and the random share. */
  dynamics(S, base){
    const s=S.s;
    let m=base;
    if(s.velocity){ const v=Math.min(1, S.speed/4000); m*=Math.max(0, 1+s.velocity/100*(v*2-1)); }
    if(s.random>0) m*=1-s.random/100*this.rand(S);
    return m;
  },
  /** The points the stroke puts dabs at for a move to (x, y): the first (at the click
   *  intensity), then one every spacing along the way. Each [x, y, mult]. */
  along(S, x, y, r, now){
    const out=[], s=S.s;
    // Smooth stroke: the brush follows the pointer at a weight (0 off, 100 slowest).
    if(s.stabilize>0 && S.smooth){
      const a=1-Math.min(0.97, s.stabilize/100);
      x=S.smooth[0]+(x-S.smooth[0])*a; y=S.smooth[1]+(y-S.smooth[1])*a;
    }
    S.smooth=[x, y];
    const dtm=Math.max(1, now-S.moved);
    if(S.cursor) S.speed=S.speed*0.6+0.4*Math.hypot(x-S.cursor[0], y-S.cursor[1])/(dtm/1000);
    S.moved=now; S.cursor=[x, y];
    const jit=(px, py)=>{
      if(!(s.jitter>0)) return [px, py];
      const a=this.rand(S)*Math.PI*2, d=this.rand(S)*r*s.jitter/100;
      return [px+Math.cos(a)*d, py+Math.sin(a)*d];
    };
    if(!S.last){
      S.last=[x, y];
      const env=this.envelope(S, now);
      out.push([...jit(x, y), this.dynamics(S, (s.click==null? 100 : s.click)/100*Math.max(env, s.attack>0? 0.15 : env))]);
      return out;
    }
    const gap=Math.max(1, r*Math.max(1, s.spacing||25)/100);
    let [lx, ly]=S.last, dx=x-lx, dy=y-ly, d=Math.hypot(dx, dy);
    if(!(d>0)) return out;
    let need=gap-S.travel;
    while(d>=need){
      lx+=dx/d*need; ly+=dy/d*need; S.dist+=need; dx=x-lx; dy=y-ly; d=Math.hypot(dx, dy);
      out.push([...jit(lx, ly), this.dynamics(S, (s.drag==null? 100 : s.drag)/100*this.envelope(S, now))]);
      need=gap; S.travel=0;
    }
    S.travel+=d; S.dist+=d; S.last=[x, y];
    return out;
  },
  /** Held still: the share of a dab this frame owes (the hold rate a second, after the hold
   *  delay), or 0. */
  hold(S, now, dt){
    const s=S.s;
    if(!S.cursor || !(s.rate>0) || now-S.moved<(s.delay==null? 150 : s.delay)) return 0;
    S.speed=0;
    return this.dynamics(S, s.rate*dt*this.envelope(S, now));
  },
  /** A change kept within the stroke's cap: how far the vertex may still go. */
  capped(S, key, from, to){
    const cap=S.s.cap;
    if(!(cap>0)) return to;
    const was=S.acc.has(key)? S.acc.get(key) : from;
    if(!S.acc.has(key)) S.acc.set(key, from);
    return Math.max(was-cap, Math.min(was+cap, to));
  },
};
