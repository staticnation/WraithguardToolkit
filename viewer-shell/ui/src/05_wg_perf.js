/* ---- Wraithguard: where a frame's CPU time goes (Static 2026-11-23) ------------------------
   "CPU breakdown in the perf report": the report timed the GPU by pass and the page's submit
   as one number; the walking actors' slowdown took three patches of reading code to find
   because nothing said which part of the page's own work it was. Each part adds what it took
   (`add`) and counts what it did (`count`); a drawn frame closes the books (`frame`), and the
   last 120 frames are kept for the report (`summary`) and the actor overlay.

   What is timed, by name:
     mwscript    the scene's MWScript, every running script once a frame (73_wg_scripts.js)
     ai          the actors' AI: walking, turning, the next package (73_wg_scripts.js)
     lua.apply   OpenMW Lua's answer put into the scene, on the page (74_wg_lua.js)
     lua.trip    a Lua step there and back, waited for off the frame (not in the frame's sum)
     pose        posing skinned parts: the bones' matrices and their upload (06_gl.js)
     move        moving live actors: their matrices into the instance buffers (18_cellpreview.js)
     swap        a live actor's mesh swapped for another animation (18_cellpreview.js)
     draw        the renderer's whole frame, the passes above that it runs included
   and long tasks (Chromium's PerformanceObserver; the collector's pauses show up as these),
   counted when the engine has them. A few `performance.now()` calls a frame: always on. */
const WgPerf={
  N:120,
  cur:{}, counts:{}, ring:[], trips:[], tripParts:[], long:[],
  _t:0,
  now(){ return performance.now(); },
  /** `ms` more for `name` this frame. */
  add(name, ms){ this.cur[name]=(this.cur[name]||0)+ms; },
  /** `n` more of `name` this frame (actors posed, matrices moved...). */
  count(name, n){ this.counts[name]=(this.counts[name]||0)+(n==null? 1 : n); },
  /** `fn` timed as `name`, its result returned. */
  time(name, fn){ const t=performance.now(); try{ return fn(); } finally{ this.add(name, performance.now()-t); } },
  /** A Lua step's round trip (off the frame). */
  trip(ms){ this.trips.push(ms); if(this.trips.length>this.N) this.trips.shift(); },
  /** Where a Lua step's round trip went (Static 2026-11-24): `p` ms by part - the page making the
   *  frame, the call across to the program, the collision, the pipe to the worker, and in the
   *  worker reading the frame, the scripts and writing the answer. */
  tripPart(p){ this.tripParts.push(p); if(this.tripParts.length>this.N) this.tripParts.shift(); },
  /** A frame was drawn: its books closed. */
  frame(){
    const t=performance.now();
    this.ring.push({at:t, gap:this._t? t-this._t : 0, ms:this.cur, n:this.counts});
    if(this.ring.length>this.N) this.ring.shift();
    this._t=t; this.cur={}; this.counts={};
  },
  /** Each part of the round trip's median, in the order a step goes through them (none: not split). */
  tripSplit(){
    const P=this.tripParts; if(!P.length) return null;
    const out={};
    for(const k of ['page', 'ipc', 'collision', 'pipe', 'read', 'scripts', 'write']){
      const v=P.map(p=>+p[k]||0).sort((a, b)=>a-b);
      out[k]=v[Math.floor(v.length/2)];
    }
    return out;
  },
  /** The last frames: each name's mean and worst a frame, the counts' means, the frames'
   *  gaps, Lua's round trips and the long tasks over the same stretch. */
  summary(){
    const R=this.ring, n=R.length;
    if(!n) return null;
    const names=new Set(), cnames=new Set();
    for(const f of R){ for(const k in f.ms) names.add(k); for(const k in f.n) cnames.add(k); }
    const parts=[...names].map(k=>{
      let sum=0, worst=0;
      for(const f of R){ const v=f.ms[k]||0; sum+=v; if(v>worst) worst=v; }
      return {name:k, mean:sum/n, worst};
    }).sort((a, b)=>b.mean-a.mean);
    const counts={};
    for(const k of cnames){ let s=0; for(const f of R) s+=f.n[k]||0; counts[k]=s/n; }
    const gaps=R.map(f=>f.gap).filter(g=>g>0).sort((a, b)=>a-b);
    const pct=p=>gaps.length? gaps[Math.min(gaps.length-1, Math.floor(p*gaps.length))] : 0;
    const since=R[0].at;
    const long=this.long.filter(l=>l.at>=since);
    const trips=this.trips.slice().sort((a, b)=>a-b);
    return {frames:n, parts, counts,
            gap:{median:pct(0.5), p95:pct(0.95), worst:gaps.length? gaps[gaps.length-1] : 0},
            trip:trips.length? {median:trips[Math.floor(trips.length/2)], worst:trips[trips.length-1], n:trips.length, parts:this.tripSplit()} : null,
            long:{n:long.length, ms:long.reduce((a, l)=>a+l.ms, 0), worst:long.reduce((a, l)=>Math.max(a, l.ms), 0)}};
  },
};
// The engine's long tasks, where it says (WebView2 does; WebKit has no such entry type).
try{
  if(typeof PerformanceObserver==='function' && (PerformanceObserver.supportedEntryTypes||[]).includes('longtask')){
    new PerformanceObserver(list=>{
      for(const e of list.getEntries()){ WgPerf.long.push({at:e.startTime, ms:e.duration}); }
      if(WgPerf.long.length>400) WgPerf.long.splice(0, WgPerf.long.length-400);
    }).observe({entryTypes:['longtask']});
  }
}catch(_){ }
