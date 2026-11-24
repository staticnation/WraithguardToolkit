/* =====================================================================================
   Wraithguard: the Land view's quality-of-life batch (Static 2026-11-24, "lets do all this").

   - **Strokes**: the last strokes (and roads, rules and imports) listed as they are made;
     "Back to before this" takes that one and every one after it back, and the ground as it was
     goes to the patch as any stroke does (the Editor's undo still steps the patch itself).
   - **Mirror**: each dab laid again across a north-south line, an east-west line, or both
     (four ways), through a center set by a click or the loaded cell's middle. The ramp is not
     mirrored (its line runs from where the stroke began).
   - **Pick** for the Color brush: the tint under the next click (Height and Texture had theirs).
   - **Road or path**: clicks lay points; Build road levels a strip as wide as asked along a
     curve through them, its height the ground's along the way smoothed and held to a steepest
     slope, its shoulders blended into the ground, and paints a land texture along it.
   - **Paint by rules**: land textures put on the loaded cells by the ground's height, slope and
     distance from the water (the sea, at 0), the first rule that matches each texture cell
     winning; a texture cell no rule matches is left. Inside or outside the painted mask, or all.
   - **Heightmap PNG**: the loaded cells' heights saved as a 16-bit grey PNG (the range it spans
     kept in the file), and a PNG put onto the loaded cells, between a lowest and a highest
     height, at a strength. Any PNG: grey or color, 8 or 16 bits (color is read as its grey).
   Everything here goes through WgLand's own cells, dirty marks and commit; nothing is written
   to the patch another way. Copyright (c) 2026 StaticNation.
   ===================================================================================== */
const WgLandQol={
  hist:[], MAX:30, _cur:null, _dropMark:null,
  road:{laying:false, pts:[], width:384, shoulder:256, slope:12, curve:true, level:true, paint:true, tex:''},
  _eat:false, _pick:null, _import:null,

  /* ---------------- the land, read ---------------- */
  /** The ground's height at a world point (bilinear between its four vertices), or null. */
  heightAt(cells, x, y){
    const S=WgLand.STEP, fx=x/S, fy=y/S, x0=Math.floor(fx), y0=Math.floor(fy), tx=fx-x0, ty=fy-y0;
    const v=(vx, vy)=>{ const h=WgLand.holders(cells, vx, vy).find(h=>h.c.heights); return h? h.c.heights[h.k] : null; };
    const a=v(x0, y0), b=v(x0+1, y0), c=v(x0, y0+1), d=v(x0+1, y0+1);
    const have=[a, b, c, d].filter(z=>z!=null);
    if(!have.length) return null;
    const m=have.reduce((s, z)=>s+z, 0)/have.length, f=z=>z==null? m : z;
    return (f(a)*(1-tx)+f(b)*tx)*(1-ty)+(f(c)*(1-tx)+f(d)*tx)*ty;
  },
  /** The ground's slope at a point in degrees, across `span` units each way. */
  slopeAt(cells, x, y, span){
    const s=span||WgLand.STEP*2, h=(a, b)=>this.heightAt(cells, a, b);
    const e=h(x+s, y), w=h(x-s, y), n=h(x, y+s), so=h(x, y-s);
    if(e==null || w==null || n==null || so==null) return 0;
    return Math.atan(Math.hypot((e-w)/(2*s), (n-so)/(2*s)))*180/Math.PI;
  },

  /* ---------------- strokes, kept ---------------- */
  /** A cell's land, copied: what a stroke can change. */
  copy(c){ return {heights:c.heights? Float32Array.from(c.heights) : null, colours:c.colours? Uint8Array.from(c.colours) : null, ids:c.heights? WgLand.ids(c).slice() : null}; },
  same(a, b){ if(a===b) return true; if(!a || !b || a.length!==b.length) return false; for(let i=0;i<a.length;i++) if(a[i]!==b[i]) return false; return true; },
  /** The cells of the scene as they are before a stroke changes them (once a stroke). */
  begin(label){
    if(!this._cur) this._cur={label, t:Date.now(), before:new Map(), pending:WgLand.preview};
    const cells=WgLand.cells();
    for(const [k, c] of cells) if(!this._cur.before.has(k)) this._cur.before.set(k, this.copy(c));
    return this._cur;
  },
  /** The stroke kept in the list, with only what it changed; nothing kept for a stroke that
   *  changed nothing. */
  end(){
    const S=this._cur; this._cur=null;
    if(!S) return null;
    const cells=WgLand.cells(), out=new Map();
    for(const [k, c] of cells){
      const b=S.before.get(k);
      if(!b){ if(WgLand._edited.has(k)) out.set(k, {kinds:['h', 't', 'c'], heights:null, colours:null, ids:null, made:true}); continue; }
      const kinds=[];
      if(!this.same(b.heights, c.heights)) kinds.push('h');
      if(!this.same(b.colours, c.colours)) kinds.push('c');
      if(b.ids && !this.same(b.ids, WgLand.ids(c))) kinds.push('t');
      if(kinds.length) out.set(k, {kinds, heights:kinds.includes('h')? b.heights : null, colours:kinds.includes('c')? b.colours : null, ids:kinds.includes('t')? b.ids : null});
    }
    if(!out.size) return null;
    const e={label:S.label, t:S.t, cells:out, pending:S.pending};
    this.hist.push(e);
    if(this.hist.length>this.MAX) this.hist.splice(0, this.hist.length-this.MAX);
    this.drawHist();
    return e;
  },
  strokeLabel(){
    const W=WgLand;
    if(W.mode==='height') return (W.TOOLS.find(t=>t[0]===W.activeTool())||[0, W.activeTool()])[1];
    if(W.mode==='texture') return 'Texture '+(W.texture||'');
    return W.mode==='color'? 'Color '+W.color : W.mode;
  },
  /** The ground as it was before stroke `i`: it and every stroke after it taken back. */
  back(i){
    if(i<0 || i>=this.hist.length) return;
    const W=WgLand, cells=W.cells(), N=W.N, touched=new Map();
    let away=0;
    W.snapshot();
    for(let j=this.hist.length-1; j>=i; j--) for(const [k, b] of this.hist[j].cells){
      const c=cells.get(k); if(!c){ away++; continue; }
      W.original(c);
      const kinds=touched.get(k)||new Set();
      if(b.kinds.includes('h')){ c.heights=b.heights? Float32Array.from(b.heights) : new Float32Array(N*N).fill(DEFAULT_LAND_HEIGHT); kinds.add('h'); }
      if(b.kinds.includes('c')){ c.colours=b.colours? Uint8Array.from(b.colours) : null; kinds.add('c'); }
      if(b.kinds.includes('t') && b.ids){ c._landIds=b.ids.slice(); kinds.add('t'); }
      touched.set(k, kinds);
    }
    const n=this.hist.length-i;
    this.hist.length=i;
    this.finish(cells, touched, n+' stroke'+(n===1? '' : 's')+' taken back'+(away? ' ('+away+' cell'+(away===1? '' : 's')+' out of view left as they are)' : ''));
    this.drawHist();
  },
  /** Cells changed here drawn again and on their way to the patch, as a stroke's are. */
  finish(cells, touched, said){
    const W=WgLand;
    if(!touched.size){ W.status(said||'Nothing changed'); return; }
    const relay=new Set();
    for(const [k, kinds] of touched){
      W._edited.set(k, cells.get(k));
      for(const kd of kinds) W.mark(k, kd);
      if(kinds.has('t')){
        const [gx, gy]=k.split(',').map(Number);
        for(let dy=-1; dy<=1; dy++) for(let dx=-1; dx<=1; dx++){ const n=(gx+dx)+','+(gy+dy), c=cells.get(n); if(c){ c._drawnLayers=null; relay.add(n); } }
      }
    }
    for(const [k, kinds] of touched) if(kinds.has('h')) W.renormal(cells.get(k), cells);
    for(const k of relay) W.relayer(cells.get(k), cells);
    W.rebuild(new Set([...touched.keys(), ...relay]));
    if(W.ov.tint!=='none' || W.ov.contours) W.applyOverlays();
    if(W.preview){ W.status(said||''); W.showWait(); return; }
    W.status((said? said+' · ' : '')+W.dirty.size+' cell'+(W.dirty.size===1? '' : 's')+' changed, not yet in the patch');
    W.scheduleCommit();
  },
  /** A change made here at once (a road, the rules, an import): `fn(cells)` answers the cells
   *  it changed, "gx,gy" -> Set of kinds; kept in the list as one stroke. */
  run(label, fn){
    const W=WgLand;
    if(!App._scene){ toast('Open an exterior cell first', 'warn', 3000); return null; }
    W.snapshot();
    this._cur=null; this.begin(label);
    const cells=W.cells();
    const touched=fn(cells);
    if(!touched || !touched.size){ this._cur=null; W.status(label+': nothing to change'); return null; }
    for(const k of touched.keys()) W.original(cells.get(k));
    this.finish(cells, touched, label);
    return this.end();
  },
  setH(cells, vx, vy, z, touched){
    for(const h of WgLand.holders(cells, vx, vy)){
      if(!h.c.heights || h.c.heights[h.k]===z) continue;
      WgLand.original(h.c); h.c.heights[h.k]=z;
      (touched.get(h.key)||touched.set(h.key, new Set()).get(h.key)).add('h');
    }
  },
  setTex(cells, tx, ty, id, touched){
    const T=WgLand.TSTEP, TS=WgLand.TS, gx=Math.floor(tx*T/WgLand.CELL), gy=Math.floor(ty*T/WgLand.CELL), k=gx+','+gy, c=cells.get(k);
    if(!c || !id) return;
    const ids=WgLand.ids(c), i=(ty-gy*TS)*TS+(tx-gx*TS);
    if(ids[i]===id) return;
    WgLand.original(c); ids[i]=id;
    (touched.get(k)||touched.set(k, new Set()).get(k)).add('t');
  },

  /* ---------------- mirror ---------------- */
  center(){
    const b=WgLand.b;
    if(b.mirrorX!=null && b.mirrorY!=null) return [b.mirrorX, b.mirrorY];
    const s=App.cellSel, C=WgLand.CELL;
    return s && s.kind==='ext'? [(s.gx+0.5)*C, (s.gy+0.5)*C] : [0, 0];
  },
  /** The dabs and their reflections. */
  mirrored(dabs){
    const m=WgLand.b.mirror||'off';
    if(m==='off' || (WgLand.mode==='height' && WgLand.activeTool()==='ramp')) return dabs;
    const [cx, cy]=this.center(), out=[];
    for(const d of dabs){
      const [x, y, k]=d;
      out.push(d);
      if(m==='x' || m==='xy') out.push([2*cx-x, y, k]);
      if(m==='y' || m==='xy') out.push([x, 2*cy-y, k]);
      if(m==='xy') out.push([2*cx-x, 2*cy-y, k]);
    }
    return out;
  },

  /* ---------------- road ---------------- */
  /** The road's line: through the points, a curve when asked, a point every 64 units. */
  line(pts, curve){
    if(pts.length<2) return pts.slice();
    const out=[], P=i=>pts[Math.max(0, Math.min(pts.length-1, i))];
    for(let i=0;i<pts.length-1;i++){
      const a=P(i-1), b=P(i), c=P(i+1), d=P(i+2), len=Math.hypot(c[0]-b[0], c[1]-b[1]), n=Math.max(1, Math.ceil(len/64));
      for(let s=0;s<n;s++){
        const t=s/n;
        if(!curve){ out.push([b[0]+(c[0]-b[0])*t, b[1]+(c[1]-b[1])*t]); continue; }
        const t2=t*t, t3=t2*t, cr=(p0, p1, p2, p3)=>0.5*((2*p1)+(-p0+p2)*t+(2*p0-5*p1+4*p2-p3)*t2+(-p0+3*p1-3*p2+p3)*t3);
        out.push([cr(a[0], b[0], c[0], d[0]), cr(a[1], b[1], c[1], d[1])]);
      }
    }
    out.push(pts[pts.length-1].slice());
    return out;
  },
  /** The road's heights along its line: the ground's, smoothed over about the road's width
   *  and held to the steepest slope both ways. */
  profile(cells, L, o){
    let z=L.map(p=>this.heightAt(cells, p[0], p[1]));
    const known=z.filter(v=>v!=null), m=known.length? known.reduce((s, v)=>s+v, 0)/known.length : 0;
    z=z.map(v=>v==null? m : v);
    const w=Math.max(1, Math.round(o.width/64));
    for(let pass=0; pass<3; pass++){
      const s=z.slice();
      for(let i=0;i<z.length;i++){ let t=0, n=0; for(let j=Math.max(0, i-w); j<=Math.min(z.length-1, i+w); j++){ t+=s[j]; n++; } z[i]=t/n; }
    }
    const g=Math.tan(Math.max(0.5, Math.min(60, o.slope))*Math.PI/180);
    for(let i=1;i<z.length;i++){ const ds=Math.hypot(L[i][0]-L[i-1][0], L[i][1]-L[i-1][1])*g; z[i]=Math.max(z[i-1]-ds, Math.min(z[i-1]+ds, z[i])); }
    for(let i=z.length-2;i>=0;i--){ const ds=Math.hypot(L[i][0]-L[i+1][0], L[i][1]-L[i+1][1])*g; z[i]=Math.max(z[i+1]-ds, Math.min(z[i+1]+ds, z[i])); }
    return z;
  },
  /** Each vertex (or texture cell) within `reach` of the line: its distance and the road's
   *  height there. `step` the grid's spacing in units. */
  near(L, z, reach, step, half){
    const best=new Map(), off=half? 0.5 : 0;
    for(let i=0;i+1<L.length;i++){
      const [ax, ay]=L[i], [bx, by]=L[i+1], dx=bx-ax, dy=by-ay, ll=dx*dx+dy*dy||1;
      const x0=Math.ceil((Math.min(ax, bx)-reach)/step-off), x1=Math.floor((Math.max(ax, bx)+reach)/step-off);
      const y0=Math.ceil((Math.min(ay, by)-reach)/step-off), y1=Math.floor((Math.max(ay, by)+reach)/step-off);
      for(let vy=y0; vy<=y1; vy++) for(let vx=x0; vx<=x1; vx++){
        const px=(vx+off)*step, py=(vy+off)*step, t=Math.max(0, Math.min(1, ((px-ax)*dx+(py-ay)*dy)/ll));
        const d=Math.hypot(px-(ax+dx*t), py-(ay+dy*t));
        if(d>reach) continue;
        const k=vx+','+vy, was=best.get(k);
        if(!was || d<was.d) best.set(k, {vx, vy, d, z:z[i]+(z[i+1]-z[i])*t});
      }
    }
    return best;
  },
  buildRoad(){
    const o=this.road;
    if(o.pts.length<2){ toast('Click at least two points along the road first', 'warn', 3000); return null; }
    if(!o.level && !o.paint){ toast('Tick Level the ground or Paint the texture', 'warn', 3000); return null; }
    if(o.paint && !o.tex){ toast('Name the road\'s land texture (or untick Paint)', 'warn', 3500); return null; }
    const e=this.run('Road', cells=>{
      const L=this.line(o.pts, o.curve), z=this.profile(cells, L, o), touched=new Map(), half=o.width/2;
      if(o.level){
        for(const v of this.near(L, z, half+o.shoulder, WgLand.STEP, false).values()){
          const cur=this.heightAt(cells, v.vx*WgLand.STEP, v.vy*WgLand.STEP); if(cur==null) continue;
          const t=v.d<=half? 0 : (v.d-half)/Math.max(1, o.shoulder), w=v.d<=half? 1 : 0.5+0.5*Math.cos(Math.PI*Math.min(1, t));
          this.setH(cells, v.vx, v.vy, cur+(v.z-cur)*w, touched);
        }
      }
      // A texture cell is 512 units: the strip painted is at least one unbroken cell wide.
      if(o.paint) for(const v of this.near(L, z, Math.max(half, WgLand.TSTEP*0.71), WgLand.TSTEP, true).values()) this.setTex(cells, v.vx, v.vy, o.tex, touched);
      return touched;
    });
    if(e){ o.pts=[]; o.laying=false; this.drawOverlay(); this.drawRoad(); }
    return e;
  },

  /* ---------------- rules ---------------- */
  /** Whether a texture cell's ground matches a rule. `g` its height, slope and distance from
   *  the water. */
  matches(r, g){
    const num=v=>v===''||v==null? null : +v;
    const hmin=num(r.hmin), hmax=num(r.hmax), smin=num(r.smin), smax=num(r.smax);
    if(hmin!=null && g.h<hmin) return false;
    if(hmax!=null && g.h>hmax) return false;
    if(smin!=null && g.s<smin) return false;
    if(smax!=null && g.s>smax) return false;
    if(r.water==='near' && !(g.w<=(+r.wdist||0))) return false;
    if(r.water==='far' && !(g.w>(+r.wdist||0))) return false;
    return true;
  },
  /** The nearest ground under the water (below 0) to a point, within `reach`; Infinity if none. */
  waterDist(cells, x, y, reach){
    const S=WgLand.STEP, x0=Math.ceil((x-reach)/S), x1=Math.floor((x+reach)/S), y0=Math.ceil((y-reach)/S), y1=Math.floor((y+reach)/S);
    let best=Infinity;
    for(let vy=y0; vy<=y1; vy++) for(let vx=x0; vx<=x1; vx++){
      const d=Math.hypot(vx*S-x, vy*S-y); if(d>=best || d>reach) continue;
      const h=WgLand.holders(cells, vx, vy).find(h=>h.c.heights);
      if(h && h.c.heights[h.k]<0) best=d;
    }
    return best;
  },
  paintRules(){
    const rules=(WgLand.b.rules||[]).filter(r=>r.tex);
    if(!rules.length){ toast('Add a rule with a land texture first', 'warn', 3000); return null; }
    const where=WgLand.b.rulesWhere||'all', reach=Math.max(0, ...rules.filter(r=>r.water && r.water!=='any').map(r=>+r.wdist||0));
    return this.run('Paint by rules', cells=>{
      const touched=new Map(), T=WgLand.TSTEP, TS=WgLand.TS, N=WgLand.N;
      for(const [k, c] of cells){
        const [gx, gy]=k.split(',').map(Number);
        const pm=WgLand.pmaskCells.get(k);
        for(let j=0;j<TS;j++) for(let i=0;i<TS;i++){
          if(where!=='all'){
            const m=pm? pm[Math.min(N-1, j*4+2)*N+Math.min(N-1, i*4+2)]/255 : 0;
            if(where==='inside'? m<0.5 : m>=0.5) continue;
          }
          const tx=gx*TS+i, ty=gy*TS+j, x=(tx+0.5)*T, y=(ty+0.5)*T, h=this.heightAt(cells, x, y);
          if(h==null) continue;
          const g={h, s:this.slopeAt(cells, x, y), w:reach>0? this.waterDist(cells, x, y, reach) : Infinity};
          const r=rules.find(r=>this.matches(r, g));
          if(r) this.setTex(cells, tx, ty, r.tex, touched);
        }
      }
      return touched;
    });
  },

  /* ---------------- heightmap PNG ---------------- */
  CRC:null,
  crc(bytes){
    if(!this.CRC){ this.CRC=new Uint32Array(256); for(let n=0;n<256;n++){ let c=n; for(let k=0;k<8;k++) c=c&1? 0xedb88320^(c>>>1) : c>>>1; this.CRC[n]=c>>>0; } }
    let c=0xffffffff; for(let i=0;i<bytes.length;i++) c=this.CRC[(c^bytes[i])&255]^(c>>>8);
    return (c^0xffffffff)>>>0;
  },
  async zlib(bytes, inflate){
    const s=new Blob([bytes]).stream().pipeThrough(inflate? new DecompressionStream('deflate') : new CompressionStream('deflate'));
    return new Uint8Array(await new Response(s).arrayBuffer());
  },
  /** A 16-bit grey PNG of `w` x `h` values 0 to 65535, row 0 at the top, with tEXt entries. */
  async encodePng(w, h, vals, text){
    const raw=new Uint8Array(h*(1+w*2));
    for(let y=0;y<h;y++){ const o=y*(1+w*2); raw[o]=0; for(let x=0;x<w;x++){ const v=vals[y*w+x]; raw[o+1+x*2]=v>>8; raw[o+2+x*2]=v&255; } }
    const chunks=[], enc=new TextEncoder();
    const chunk=(type, data)=>{
      const b=new Uint8Array(12+data.length), dv=new DataView(b.buffer);
      dv.setUint32(0, data.length); b.set(enc.encode(type), 4); b.set(data, 8);
      dv.setUint32(8+data.length, this.crc(b.subarray(4, 8+data.length)));
      chunks.push(b);
    };
    const ih=new Uint8Array(13), dv=new DataView(ih.buffer);
    dv.setUint32(0, w); dv.setUint32(4, h); ih[8]=16; ih[9]=0; ih[10]=0; ih[11]=0; ih[12]=0;
    chunk('IHDR', ih);
    for(const [k, v] of Object.entries(text||{})){ const kb=enc.encode(k), vb=enc.encode(v), t=new Uint8Array(kb.length+1+vb.length); t.set(kb); t.set(vb, kb.length+1); chunk('tEXt', t); }
    chunk('IDAT', await this.zlib(raw, false));
    chunk('IEND', new Uint8Array(0));
    const sig=[137, 80, 78, 71, 13, 10, 26, 10], len=8+chunks.reduce((s, c)=>s+c.length, 0), out=new Uint8Array(len);
    out.set(sig); let o=8; for(const c of chunks){ out.set(c, o); o+=c.length; }
    return out;
  },
  /** A PNG read: {w, h, v: Float32Array of 0 to 1 (grey; color as its luminance), text}. */
  async decodePng(bytes){
    const b=bytes instanceof Uint8Array? bytes : new Uint8Array(bytes), dv=new DataView(b.buffer, b.byteOffset, b.byteLength);
    if(b.length<8 || b[0]!==137 || b[1]!==80 || b[2]!==78 || b[3]!==71) throw new Error('Not a PNG file');
    let o=8, w=0, h=0, depth=0, type=0, inter=0; const idat=[], text={}, dec=new TextDecoder('latin1');
    while(o+8<=b.length){
      const len=dv.getUint32(o), t=dec.decode(b.subarray(o+4, o+8)), d=b.subarray(o+8, o+8+len);
      if(t==='IHDR'){ const x=new DataView(d.buffer, d.byteOffset, d.length); w=x.getUint32(0); h=x.getUint32(4); depth=d[8]; type=d[9]; inter=d[12]; }
      else if(t==='IDAT') idat.push(d);
      else if(t==='tEXt'){ const z=d.indexOf(0); if(z>0) text[dec.decode(d.subarray(0, z))]=dec.decode(d.subarray(z+1)); }
      else if(t==='IEND') break;
      o+=12+len;
    }
    if(!w || !h) throw new Error('The PNG has no image header');
    if(type===3) throw new Error('A palette PNG: save it as grey or RGB');
    if(inter) throw new Error('An interlaced PNG: save it without interlacing');
    if(depth!==8 && depth!==16) throw new Error('A '+depth+'-bit PNG: 8 or 16 bits are read');
    const ch={0:1, 2:3, 4:2, 6:4}[type]; if(!ch) throw new Error('PNG color type '+type+' is not read');
    const all=new Uint8Array(idat.reduce((s, x)=>s+x.length, 0)); { let p=0; for(const x of idat){ all.set(x, p); p+=x.length; } }
    const raw=await this.zlib(all, true), bpp=ch*depth/8, stride=w*bpp, out=new Float32Array(w*h);
    let prev=new Uint8Array(stride), cur=new Uint8Array(stride);
    const max=depth===16? 65535 : 255;
    for(let y=0;y<h;y++){
      const f=raw[y*(stride+1)], line=raw.subarray(y*(stride+1)+1, (y+1)*(stride+1));
      for(let i=0;i<stride;i++){
        const a=i>=bpp? cur[i-bpp] : 0, up=prev[i], c=i>=bpp? prev[i-bpp] : 0;
        let v=line[i];
        if(f===1) v+=a; else if(f===2) v+=up; else if(f===3) v+=(a+up)>>1;
        else if(f===4){ const p=a+up-c, pa=Math.abs(p-a), pb=Math.abs(p-up), pc=Math.abs(p-c); v+=pa<=pb && pa<=pc? a : pb<=pc? up : c; }
        cur[i]=v&255;
      }
      for(let x=0;x<w;x++){
        const s=k=>depth===16? (cur[x*bpp+k*2]<<8)|cur[x*bpp+k*2+1] : cur[x*bpp+k];
        out[y*w+x]=(ch>=3? 0.2126*s(0)+0.7152*s(1)+0.0722*s(2) : s(0))/max;
      }
      const t=prev; prev=cur; cur=t;
    }
    return {w, h, v:out, text};
  },
  /** The loaded exteriors' extent: {gx0, gy0, cols, rows, w, h} (w x h vertices). */
  extent(cells){
    const keys=[...cells.keys()].map(k=>k.split(',').map(Number));
    if(!keys.length) return null;
    const gx0=Math.min(...keys.map(k=>k[0])), gy0=Math.min(...keys.map(k=>k[1]));
    const cols=Math.max(...keys.map(k=>k[0]))-gx0+1, rows=Math.max(...keys.map(k=>k[1]))-gy0+1, L=WgLand.N-1;
    return {gx0, gy0, cols, rows, w:cols*L+1, h:rows*L+1};
  },
  /** The loaded cells' heights as a PNG: {png, name, min, max}. */
  async exportPng(){
    const cells=WgLand.cells(), E=this.extent(cells);
    if(!E) throw new Error('No land loaded');
    const L=WgLand.N-1, at=(vx, vy)=>{ const h=WgLand.holders(cells, vx, vy).find(h=>h.c.heights); return h? h.c.heights[h.k] : null; };
    let min=Infinity, max=-Infinity;
    const z=new Float64Array(E.w*E.h).fill(NaN);
    for(let y=0;y<E.h;y++) for(let x=0;x<E.w;x++){ const v=at(E.gx0*L+x, E.gy0*L+(E.h-1-y)); if(v!=null){ z[y*E.w+x]=v; if(v<min) min=v; if(v>max) max=v; } }
    min=Math.floor(min/8)*8; max=Math.max(min+8, Math.ceil(max/8)*8);
    const vals=new Uint16Array(E.w*E.h);
    for(let i=0;i<vals.length;i++) vals[i]=isNaN(z[i])? 0 : Math.round((z[i]-min)/(max-min)*65535);
    const info={gx0:E.gx0, gy0:E.gy0, cols:E.cols, rows:E.rows, min, max};
    const png=await this.encodePng(E.w, E.h, vals, {Software:'Wraithguard', Wraithguard:JSON.stringify(info)});
    return {png, min, max, name:'heightmap_'+E.gx0+'_'+E.gy0+'_'+E.cols+'x'+E.rows+'_'+min+'_'+max+'.png'};
  },
  /** A decoded PNG put onto the loaded cells: the image stretched over their extent (north at
   *  the top), 0 the lowest height and 1 the highest, mixed in at `strength` (0 to 1). */
  applyImport(img, low, high, strength){
    const k=Math.max(0, Math.min(1, strength));
    return this.run('Heightmap '+(img.name||''), cells=>{
      const E=this.extent(cells), touched=new Map(), L=WgLand.N-1;
      if(!E) return touched;
      const sample=(u, v)=>{
        const fx=u*(img.w-1), fy=v*(img.h-1), x0=Math.floor(fx), y0=Math.floor(fy), x1=Math.min(img.w-1, x0+1), y1=Math.min(img.h-1, y0+1), tx=fx-x0, ty=fy-y0, I=img.v;
        return (I[y0*img.w+x0]*(1-tx)+I[y0*img.w+x1]*tx)*(1-ty)+(I[y1*img.w+x0]*(1-tx)+I[y1*img.w+x1]*tx)*ty;
      };
      for(let y=0;y<E.h;y++) for(let x=0;x<E.w;x++){
        const vx=E.gx0*L+x, vy=E.gy0*L+y, h=WgLand.holders(cells, vx, vy).find(h=>h.c.heights); if(!h) continue;
        const want=low+(high-low)*sample(x/Math.max(1, E.w-1), 1-y/Math.max(1, E.h-1)), cur=h.c.heights[h.k];
        this.setH(cells, vx, vy, cur+(want-cur)*k, touched);
      }
      return touched;
    });
  },

  /* ---------------- clicks the page takes ---------------- */
  /** A click the Land view's brush would have taken, when one of ours wants it: true if taken. */
  click(wx, wy){
    const p=this._pick;
    if(p==='color'){
      this._pick=null;
      const cells=WgLand.cells(), vx=Math.round(wx/WgLand.STEP), vy=Math.round(wy/WgLand.STEP), h=WgLand.holders(cells, vx, vy).find(h=>h.c.heights);
      if(h){
        const C=h.c.colours, rgb=C && C.length>=WgLand.N*WgLand.N*3? [0, 1, 2].map(j=>C[h.k*3+j]) : [255, 255, 255];
        WgLand.color='#'+rgb.map(v=>v.toString(16).padStart(2, '0')).join('');
        const inp=document.getElementById('wgLandCol'); if(inp) inp.value=WgLand.color;
        WgLand.keep(); toast('Color '+WgLand.color, 'ok', 2000);
      }
      this.drawExtras();
      return true;
    }
    if(p==='center'){
      this._pick=null; WgLand.b.mirrorX=Math.round(wx); WgLand.b.mirrorY=Math.round(wy); WgLand.keep();
      this.drawOverlay(); this.drawExtras();
      return true;
    }
    if(this.road.laying){ this.road.pts.push([wx, wy]); this.drawOverlay(); this.drawRoad(); return true; }
    return false;
  },

  /** The road's points and line, and the mirror's lines, over the ground. */
  drawOverlay(){
    const R=App.R, sc=App._scene; if(!R || !R.setOverlay) return;
    if(!WgLand.on || !sc || !sc.origin){ R.setOverlay('wgLandQol', null); return; }
    const [ox, oy]=sc.origin, Lz=new OvlLines();
    const gz=(x, y)=>{ const z=R.groundZ? R.groundZ(x-ox, y-oy) : null; return (z==null? 0 : z)+24; };
    const P=(x, y)=>[x-ox, y-oy, gz(x, y)];
    const pts=this.road.pts;
    if(pts.length){
      const col=[1, 0.75, 0.2, 1];
      for(const p of pts) Lz.circle(P(p[0], p[1]), 48, col, 'z', 16);
      const L=this.line(pts, this.road.curve);
      for(let i=0;i+1<L.length;i++) Lz.seg(P(L[i][0], L[i][1]), P(L[i+1][0], L[i+1][1]), col);
    }
    const m=WgLand.b.mirror||'off';
    if(m!=='off'){
      const [cx, cy]=this.center(), span=WgLand.CELL*3, col=[0.4, 0.8, 1, 1], st=256;
      const run=(ax, ay, bx, by)=>{ const n=Math.ceil(Math.hypot(bx-ax, by-ay)/st); for(let i=0;i<n;i++){ const t0=i/n, t1=(i+1)/n; Lz.seg(P(ax+(bx-ax)*t0, ay+(by-ay)*t0), P(ax+(bx-ax)*t1, ay+(by-ay)*t1), col); } };
      if(m==='x' || m==='xy') run(cx, cy-span, cx, cy+span);
      if(m==='y' || m==='xy') run(cx-span, cy, cx+span, cy);
    }
    R.setOverlay('wgLandQol', Lz.size? Lz : null, {xray:true});
  },

  /* ---------------- the panel's parts ---------------- */
  el(tag, cls, text){ const e=document.createElement(tag); if(cls) e.className=cls; if(text!=null) e.textContent=text; return e; },
  btn(text, title, fn, cls){ const b=this.el('button', 'btn sm'+(cls? ' '+cls : ''), text); if(title) b.title=title; b.onclick=fn; return b; },
  sec(id, title, fold){
    const s=this.el(fold? 'details' : 'div', 'lsec'); s.id=id;
    const t=this.el(fold? 'summary' : 'div', 'lsecT', title); if(fold) t.style.cursor='pointer';
    s.append(t);
    return s;
  },
  row(label, ctl, tip){
    const r=this.el('div', 'row'), l=this.el('label', '', label), c=this.el('div', 'ctl');
    if(tip) l.title=tip;
    c.append(...[].concat(ctl)); r.append(l, c);
    return r;
  },
  num(val, step, on, w){ const i=this.el('input', 'fld'); i.type='number'; i.step=step||1; i.value=val==null? '' : val; i.style.width=(w||80)+'px'; i.onchange=()=>on(i.value===''? null : +i.value); return i; },
  pick(opts, cur, on){ const s=this.el('select', 'fld'); for(const [v, t] of opts){ const o=this.el('option', '', t); o.value=v; if(String(v)===String(cur)) o.selected=true; s.append(o); } s.onchange=()=>on(s.value); return s; },
  texInput(val, on){
    const i=this.el('input', 'fld'); i.value=val||''; i.placeholder='Land texture id'; i.spellcheck=false; i.setAttribute('list', 'wgLandTexIds');
    i.style.cssText='flex:1;min-width:0'; i.onchange=()=>on(i.value.trim());
    return i;
  },
  texList(){
    let dl=document.getElementById('wgLandTexIds');
    if(!dl){ dl=this.el('datalist'); dl.id='wgLandTexIds'; document.body.append(dl); }
    dl.textContent='';
    for(const id of WgLand.textureIds()){ const o=this.el('option'); o.value=id; dl.append(o); }
  },

  /** Our sections, put into the Land dock each time it is built. */
  attach(d){
    if(!d) return;
    this.texList();
    const before=d.querySelector('#wgLandPatchS');
    const put=s=>d.insertBefore(s, before||null);
    // The Color brush's pick.
    const colRow=d.querySelector('#wgLandColor .lrow');
    if(colRow) colRow.append(this.btn('Pick', 'Take the tint under the next click', ()=>{ this._pick=this._pick==='color'? null : 'color'; this.drawExtras(); }), );
    if(colRow) colRow.lastChild.id='wgLandColPick';
    // Mirror.
    const mir=this.sec('wgLandMirror', 'Mirror');
    const b=WgLand.b;
    mir.append(this.row('Mirror', this.pick([['off', 'Off'], ['x', 'Left and right (across a north-south line)'], ['y', 'Top and bottom (across an east-west line)'], ['xy', 'Four ways']], b.mirror||'off', v=>{ b.mirror=v; WgLand.keep(); this.drawOverlay(); this.drawExtras(); }), 'Each dab is laid again, reflected through the center'));
    const cRow=this.el('div', 'lrow'); cRow.id='wgLandMirC';
    cRow.append(this.btn('Center here', 'The mirror\'s center at the next click', ()=>{ this._pick=this._pick==='center'? null : 'center'; this.drawExtras(); }),
      this.btn('Cell\'s middle', 'The center of the loaded cell', ()=>{ b.mirrorX=null; b.mirrorY=null; WgLand.keep(); this.drawOverlay(); this.drawExtras(); }, 'dim'),
      this.el('span', 'hint'));
    mir.append(cRow);
    put(mir);
    // Road.
    const rd=this.sec('wgLandRoad', 'Road or path'), o=this.road;
    const lay=this.btn('Lay points', 'Clicks on the ground add the road\'s points (Backspace takes the last back)', ()=>{ o.laying=!o.laying; this._pick=null; this.drawRoad(); this.drawOverlay(); });
    lay.id='wgLandRoadLay';
    const rr=this.el('div', 'lrow'); rr.append(lay, this.btn('Build road', 'Level and paint the road along the points', ()=>this.buildRoad(), 'pri'), this.btn('Clear', 'Forget the points', ()=>{ o.pts=[]; this.drawRoad(); this.drawOverlay(); }, 'dim'));
    rd.append(rr);
    rd.append(this.row('Width', this.num(o.width, 32, v=>{ o.width=Math.max(64, v||384); }), 'The road\'s level part, in units'));
    rd.append(this.row('Shoulder', this.num(o.shoulder, 32, v=>{ o.shoulder=Math.max(0, v||0); }), 'How far beyond the road the ground blends back to its own, in units'));
    rd.append(this.row('Steepest', this.num(o.slope, 1, v=>{ o.slope=Math.max(0.5, Math.min(60, v||12)); }), 'The steepest slope along the road, in degrees'));
    const ck=(k, label, tip)=>{ const l=this.el('label'); l.title=tip; const c=this.el('input'); c.type='checkbox'; c.checked=!!o[k]; c.onchange=()=>{ o[k]=c.checked; this.drawOverlay(); }; l.append(c, ' '+label); return l; };
    const cr=this.el('div', 'lrow'); cr.append(ck('curve', 'Curve', 'A smooth curve through the points, not straight lines'), ck('level', 'Level the ground', 'Shape the ground to the road'), ck('paint', 'Paint the texture', 'Paint the land texture along it'));
    rd.append(cr);
    const tr=this.el('div', 'lrow'); tr.append(this.texInput(o.tex, v=>{ o.tex=v; }), this.btn('Use paint texture', 'The texture the Texture brush paints with', ()=>{ o.tex=WgLand.texture||o.tex; WgLand.panel(); }, 'dim'));
    rd.append(tr, this.el('div', 'hint'));
    rd.lastChild.id='wgLandRoadHint';
    put(rd);
    // Rules.
    const ru=this.sec('wgLandRules', 'Paint by rules');
    ru.append(this.el('div', 'hint', 'Each texture cell of the loaded land gets the first rule\'s texture that its ground matches; one no rule matches is left. Blank is any.'));
    const list=this.el('div'); list.id='wgLandRuleList'; ru.append(list);
    const rb=this.el('div', 'lrow');
    rb.append(this.btn('Add rule', '', ()=>{ (b.rules=b.rules||[]).push({tex:WgLand.texture||'', hmin:null, hmax:null, smin:null, smax:null, water:'any', wdist:1024}); WgLand.keep(); this.drawRules(); }),
      this.pick([['all', 'All the loaded land'], ['inside', 'Inside the painted mask'], ['outside', 'Outside the painted mask']], b.rulesWhere||'all', v=>{ b.rulesWhere=v; WgLand.keep(); }),
      this.btn('Paint', 'Paint the loaded land by the rules', ()=>this.paintRules(), 'pri'));
    ru.append(rb);
    put(ru);
    // Heightmap.
    const hm=this.sec('wgLandHmap', 'Heightmap PNG', true);
    const hb=this.el('div', 'lrow');
    const file=this.el('input'); file.type='file'; file.accept='.png,image/png'; file.style.display='none';
    file.onchange=async()=>{ const f=file.files && file.files[0]; file.value=''; if(f) await this.chooseImport(f); };
    hb.append(this.btn('Export…', 'Save the loaded land\'s heights as a 16-bit grey PNG', ()=>this.saveExport()), this.btn('Import…', 'Put a PNG\'s heights onto the loaded land', ()=>file.click()), file);
    hm.append(hb);
    const imp=this.el('div'); imp.id='wgLandImp'; hm.append(imp);
    put(hm);
    // Strokes.
    const hs=this.sec('wgLandHist', 'Strokes');
    const hl=this.el('div'); hl.id='wgLandHistList'; hl.style.cssText='max-height:180px;overflow:auto;font-size:11px';
    hs.append(hl);
    put(hs);
    // The mode buttons show and hide ours too.
    const modes=d.querySelector('#wgLandModes'); if(modes) modes.addEventListener('click', ()=>setTimeout(()=>this.drawExtras(), 0));
    this.drawRules(); this.drawRoad(); this.drawImport(); this.drawHist(); this.drawExtras();
  },
  drawExtras(){
    const m=WgLand.mode, $$=id=>document.getElementById(id);
    const vis=(id, on)=>{ const e=$$(id); if(e) e.style.display=on? '' : 'none'; };
    vis('wgLandMirror', m!=='mask');
    vis('wgLandRoad', m==='height' || m==='texture');
    vis('wgLandRules', m==='texture');
    vis('wgLandHmap', m==='height');
    const cp=$$('wgLandColPick'); if(cp) cp.classList.toggle('pri', this._pick==='color');
    const mc=$$('wgLandMirC');
    if(mc){
      const on=(WgLand.b.mirror||'off')!=='off', [cx, cy]=this.center();
      mc.style.display=on? '' : 'none';
      mc.firstChild.classList.toggle('pri', this._pick==='center');
      mc.lastChild.textContent=(WgLand.b.mirrorX==null? 'the cell\'s middle ' : '')+'('+Math.round(cx)+', '+Math.round(cy)+')'+
        (WgLand.mode==='height' && WgLand.tool==='ramp'? ' · not for the ramp' : '');
    }
  },
  drawRoad(){
    const lay=document.getElementById('wgLandRoadLay'); if(lay) lay.classList.toggle('pri', this.road.laying);
    const h=document.getElementById('wgLandRoadHint');
    if(h) h.textContent=this.road.laying? this.road.pts.length+' point'+(this.road.pts.length===1? '' : 's')+': click the ground to add, Backspace takes the last back'
      : this.road.pts.length? this.road.pts.length+' points laid' : 'Lay points, then Build road. A texture cell is 512 units, so the painted strip is at least one cell wide.';
  },
  drawRules(){
    const box=document.getElementById('wgLandRuleList'); if(!box) return;
    box.textContent='';
    const rules=WgLand.b.rules||[];
    rules.forEach((r, i)=>{
      const card=this.el('div'); card.style.cssText='border:1px solid var(--line);border-radius:4px;padding:3px;margin:3px 0';
      const top=this.el('div', 'lrow');
      top.append(this.el('b', '', (i+1)+'.'), this.texInput(r.tex, v=>{ r.tex=v; WgLand.keep(); }),
        this.btn('↑', 'Earlier: tried first', ()=>{ if(i>0){ rules.splice(i-1, 0, rules.splice(i, 1)[0]); WgLand.keep(); this.drawRules(); } }, 'dim ic'),
        this.btn('✕', 'Remove this rule', ()=>{ rules.splice(i, 1); WgLand.keep(); this.drawRules(); }, 'dim ic'));
      const set=k=>v=>{ r[k]=v; WgLand.keep(); };
      const hrow=this.el('div', 'lrow'); hrow.append(this.el('span', 'hint', 'Height'), this.num(r.hmin, 8, set('hmin'), 70), this.el('span', 'hint', 'to'), this.num(r.hmax, 8, set('hmax'), 70));
      const srow=this.el('div', 'lrow'); srow.append(this.el('span', 'hint', 'Slope°'), this.num(r.smin, 1, set('smin'), 55), this.el('span', 'hint', 'to'), this.num(r.smax, 1, set('smax'), 55));
      const wrow=this.el('div', 'lrow'); wrow.append(this.pick([['any', 'Near or far from water'], ['near', 'Within of the water'], ['far', 'Farther from the water than']], r.water||'any', v=>{ r.water=v; WgLand.keep(); }), this.num(r.wdist, 64, set('wdist'), 70), this.el('span', 'hint', 'u'));
      card.append(top, hrow, srow, wrow);
      box.append(card);
    });
    if(!rules.length) box.append(this.el('div', 'hint', 'No rules yet. For example: sand below 64 near the water, rock steeper than 35°, grass the rest.'));
  },
  async saveExport(){
    let r;
    try{ r=await this.exportPng(); }catch(e){ toast(String(e.message||e), 'err', 4000); return; }
    let out;
    try{ out=await Engine.pick('save', {defaultPath:r.name, filters:[{name:'PNG', extensions:['png']}]}); }catch(e){ toast(String(e.message||e), 'err', 4000); return; }
    if(!out) return;
    try{ await Engine.send('write_bytes', r.png, {'x-path':String(out)}); toast('Saved '+String(out).split(/[\\/]/).pop()+' (heights '+r.min+' to '+r.max+')', 'ok', 4000); }
    catch(e){ toast(String(e.message||e), 'err', 5000); }
  },
  async chooseImport(f){
    let img;
    try{ img=await this.decodePng(new Uint8Array(await f.arrayBuffer())); }catch(e){ toast(f.name+': '+String(e.message||e), 'err', 5000); return; }
    img.name=f.name;
    let low=null, high=null;
    try{ const info=JSON.parse(img.text.Wraithguard||'null'); if(info){ low=info.min; high=info.max; } }catch(_){ }
    if(low==null){ const m=String(f.name).match(/_(-?\d+)_(-?\d+)\.png$/i); if(m){ low=+m[1]; high=+m[2]; } }
    if(low==null){
      const cells=WgLand.cells(); let a=Infinity, b=-Infinity;
      for(const c of cells.values()) for(const v of c.heights){ if(v<a) a=v; if(v>b) b=v; }
      low=isFinite(a)? Math.floor(a) : 0; high=isFinite(b)? Math.ceil(b) : 2048;
    }
    this._import={img, low, high, strength:100};
    this.drawImport();
  },
  drawImport(){
    const box=document.getElementById('wgLandImp'); if(!box) return;
    box.textContent='';
    const I=this._import;
    if(!I){ box.append(this.el('div', 'hint', 'Export keeps the heights\' range in the file; Import reads it back, or set it. The image covers the loaded cells, north at the top.')); return; }
    const E=this.extent(WgLand.cells());
    box.append(this.el('div', 'hint', I.img.name+' '+I.img.w+' x '+I.img.h+(E? ' onto '+E.cols+' x '+E.rows+' cells ('+E.w+' x '+E.h+' vertices)' : '')));
    box.append(this.row('Lowest', this.num(I.low, 8, v=>{ I.low=v||0; }), 'The height black is'));
    box.append(this.row('Highest', this.num(I.high, 8, v=>{ I.high=v||0; }), 'The height white is'));
    box.append(this.row('Strength', this.num(I.strength, 5, v=>{ I.strength=Math.max(0, Math.min(100, v||0)); }), '100 sets the heights; less mixes them in, %'));
    const r=this.el('div', 'lrow');
    r.append(this.btn('Apply', 'Put the image\'s heights on the loaded land', ()=>{ if(this.applyImport(I.img, +I.low, +I.high, I.strength/100)){ this._import=null; this.drawImport(); } }, 'pri'),
      this.btn('Cancel', '', ()=>{ this._import=null; this.drawImport(); }, 'dim'));
    box.append(r);
  },
  drawHist(){
    const box=document.getElementById('wgLandHistList'); if(!box) return;
    box.textContent='';
    if(!this.hist.length){ box.append(this.el('div', 'hint', 'Your strokes are listed here as you make them.')); return; }
    for(let i=this.hist.length-1; i>=0; i--){
      const e=this.hist[i], r=this.el('div', 'lrow');
      const t=new Date(e.t), n=e.cells.size;
      const s=this.el('span', '', t.toTimeString().slice(0, 8)+'  '+e.label+' · '+n+' cell'+(n===1? '' : 's'));
      s.style.cssText='flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap';
      s.title=[...e.cells.keys()].map(k=>'('+k+')').join(' ');
      r.append(s, this.btn('Back to before this', 'Take back this stroke and every one after it', ()=>this.back(i), 'dim'));
      box.append(r);
    }
  },

  /* ---------------- hooks into WgLand ---------------- */
  bind(){
    if(typeof WgLand!=='object' || WgLand._qol) return;
    const W=WgLand, Q=this;
    W._qol=true;
    const applyDabs=W.applyDabs, paint=W.paint, endStroke=W.endStroke, panel=W.panel, step=W.step, drop=W.drop, snapshot=W.snapshot, leave=W.leave, commit=W.commit;
    W.paint=function(pts){
      if(Q._eat) return;
      if((Q._pick || Q.road.laying) && pts.length>=2){
        const sc=App._scene; if(!sc || !sc.origin) return;
        if(Q.click(pts[0]+sc.origin[0], pts[1]+sc.origin[1])){ Q._eat=true; return; }
      }
      return paint.apply(this, arguments);
    };
    W.applyDabs=function(dabs){
      if(Q._eat || Q._pick || Q.road.laying) return;
      if(this.mode==='mask') return applyDabs.call(this, dabs);
      if(dabs && dabs.length) Q.begin(Q.strokeLabel());
      return applyDabs.call(this, Q.mirrored(dabs||[]));
    };
    W.endStroke=function(){ const r=endStroke.apply(this, arguments); Q._eat=false; if(Q._cur) Q.end(); return r; };
    W.panel=function(){ const r=panel.apply(this, arguments); Q.attach(this._el); return r; };
    // The patch stepped under the list (the Editor's undo or redo): what the list would put back
    // is no longer the ground before each stroke.
    W.step=async function(which){ if(!(which==='undo' && this.waiting())){ Q.hist=[]; Q.drawHist(); } return step.apply(this, arguments); };
    W.snapshot=function(){ if(this.preview && !this._before) Q._dropMark=Q.hist.length; return snapshot.apply(this, arguments); };
    W.drop=function(){ if(Q._dropMark!=null){ Q.hist.length=Math.min(Q.hist.length, Q._dropMark); Q._dropMark=null; Q.drawHist(); } return drop.apply(this, arguments); };
    W.commit=function(){ Q._dropMark=null; return commit.apply(this, arguments); };
    W.leave=async function(){ Q.road.laying=false; Q._pick=null; Q.hist=[]; const r=await leave.apply(this, arguments); Q.drawOverlay(); return r; };
    window.addEventListener('keydown', e=>{
      if(!W.on || !Q.road.laying || e.key!=='Backspace') return;
      const t=e.target; if(t && (t.tagName==='INPUT' || t.tagName==='TEXTAREA' || t.isContentEditable)) return;
      e.preventDefault(); Q.road.pts.pop(); Q.drawOverlay(); Q.drawRoad();
    });
  }
};
WgLandQol.bind();
