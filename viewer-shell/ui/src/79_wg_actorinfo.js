/* =====================================================================================
   Wraithguard: what each actor is doing, over its head (Static 2026-11-23: "an actor debug
   overlay"). The viewport's "Actor info" switch, while the Scripts run.

   Walking in place, sliding during the warmup and vanishing on a turn were each a guess from
   a video; the label says it outright: the AI package the actor is on (and its combat or
   fleeing), the animation it plays (`L.spec`, its live mesh's), whether it is blending into
   it, whether it still waits for its walk mesh (`WgScripts.warming`), and how often the
   renderer poses it (`R.poseInterval`, by distance). Only actors within 4,000 units, at most
   40, the nearest first; the text is written ten times a second, the place every frame.
   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
const WgActorInfo={
  on:false,
  NEAR:4000, MAX:40,
  _raf:0, _box:null, _els:new Map(), _textAt:0,

  set(on){
    this.on=!!on;
    try{ localStorage.setItem('wgActorInfo', this.on? '1' : '0'); }catch(_){ }
    const b=$('#tbActorInfo'); if(b) b.classList.toggle('on', this.on);
    if(this.on) this.loop(); else this.clear();
  },
  wire(){
    const b=$('#tbActorInfo'); if(!b || b._wired) return;
    b._wired=true;
    b.onclick=()=>this.set(!this.on);
    let was=false; try{ was=localStorage.getItem('wgActorInfo')==='1'; }catch(_){ }
    this.set(was);
  },
  clear(){
    cancelAnimationFrame(this._raf); this._raf=0;
    if(this._box){ this._box.remove(); this._box=null; }
    this._els.clear();
  },
  loop(){
    cancelAnimationFrame(this._raf);
    const step=()=>{
      if(!this.on) return;
      this._raf=requestAnimationFrame(step);
      try{ this.place(); }catch(_){ }
    };
    this._raf=requestAnimationFrame(step);
  },

  /** One actor's lines (`A`: 73_wg_scripts.js `actorOf`). */
  lines(A, d2){
    const S=WgScripts, R=App.R, key=A.key;
    const out=[];
    out.push(A.id+(A.dead? ' (dead)' : ''));
    let ai=A.ai? A.ai.kind : 'no package';
    if(A.ai && A.ai.kind==='wander') ai+=A.ai.range>0? ' '+Math.round(A.ai.range) : ' in place';
    if(A.luaAI===false) ai='Lua controls';
    if(A.combat) ai='combat'+(A._fleeing? ', fleeing' : '');
    if(A.packages && A.packages.length>1) ai+=' ('+((A.pkg|0)+1)+'/'+A.packages.length+')';
    out.push('AI: '+ai+(A._moved? (A._running? ' · running' : ' · walking') : ''));
    const L=App._liveRefs && App._liveRefs.get(key);
    let anim='idle';
    if(L && L.spec) anim=String(L.spec).replace(/@[^,*]*/g, '').replace(/\*[\d.]+/g, '');
    else if(S.anims && S.anims.size){ const sp=S.animSpec(key); if(sp) anim=sp.replace(/@[^,*]*/g, ''); }
    const blending=!!(L && L.batches && L.batches.some(b=>b._blend || (b.part && b.part._blend)));
    const asked=L && L.want && L.want!==L.model;
    out.push('anim: '+anim+(blending? ' · blending' : '')+(asked? ' · loading' : ''));
    const flags=[];
    if(typeof S.warming==='function' && S.warming(key)) flags.push('waiting for its walk mesh');
    if(!L) flags.push('not drawn live');
    if(R && R.poseInterval){
      const iv=R.poseInterval(d2);
      flags.push(iv===null? 'posed every frame' : iv>0? 'posed '+Math.round(1/iv)+'/s' : 'pose held');
    }
    out.push(flags.join(' · '));
    return out;
  },

  place(){
    const R=App.R, S=typeof WgScripts==='object'? WgScripts : null;
    if(!R || !R.cam || !S || !S.on || !S.actors){ if(this._box) this._box.style.display='none'; return; }
    if(!this._box){
      const box=document.createElement('div');
      box.className='wgActorInfo';
      box.style.cssText='position:absolute;left:0;top:0;pointer-events:none;z-index:4';
      (R.cv.parentElement||document.body).appendChild(box);
      this._box=box;
    }
    this._box.style.display='';
    const rect=R.cv.getBoundingClientRect(), W=rect.width||1, H=rect.height||1;
    const c=R.cam, ce=Math.cos(c.el), se=Math.sin(c.el);
    const eye=[c.tx+c.dist*ce*Math.cos(c.az), c.ty+c.dist*ce*Math.sin(c.az), c.tz+c.dist*se];
    const [n, f]=R.nearFar();
    const VP=M4.mul(M4.persp(FOV, W/H, n, f), M4.lookAt(eye, [c.tx, c.ty, c.tz], [0, 0, 1]));
    const host=R.cv.parentElement, hr=host? host.getBoundingClientRect() : rect;
    const o=(App._scene && App._scene.origin)||[0, 0];
    // The nearest actors, in scene units (the scene is drawn about its origin), a head above the feet.
    const near=[];
    for(const A of S.actors.values()){
      if(!A || A.key==='player' || !A.t) continue;
      const w=S.posOf(A.t), p=[w[0]-o[0], w[1]-o[1], w[2]+150];
      const d2=(p[0]-eye[0])**2+(p[1]-eye[1])**2+(p[2]-eye[2])**2;
      if(d2<=this.NEAR*this.NEAR) near.push({A, p, d2});
    }
    near.sort((a, b)=>a.d2-b.d2);
    near.length=Math.min(near.length, this.MAX);
    const now=performance.now(), writeText=now-this._textAt>100;
    if(writeText) this._textAt=now;
    const keep=new Set();
    for(const {A, p, d2} of near){
      const x=VP[0]*p[0]+VP[4]*p[1]+VP[8]*p[2]+VP[12];
      const y=VP[1]*p[0]+VP[5]*p[1]+VP[9]*p[2]+VP[13];
      const w=VP[3]*p[0]+VP[7]*p[1]+VP[11]*p[2]+VP[15];
      if(!(w>0)) continue;
      const sx=((x/w)*0.5+0.5)*W, sy=(1-((y/w)*0.5+0.5))*H;
      if(sx<-150 || sx>W+150 || sy<-80 || sy>H+80) continue;
      let el=this._els.get(A.key);
      if(!el){
        el=document.createElement('div');
        el.style.cssText='position:absolute;left:0;top:0;white-space:nowrap;font:11px/1.3 var(--mono,monospace);'+
          'color:#e8e2d0;background:rgba(10,10,14,.72);border:1px solid rgba(255,210,120,.35);border-radius:4px;padding:2px 6px';
        this._box.appendChild(el);
        this._els.set(A.key, el);
      }
      keep.add(A.key);
      if(writeText || !el.textContent){
        // Text only (no markup): an actor's id is the plugin's, not ours to put into the page as HTML.
        const ls=this.lines(A, d2);
        while(el.childNodes.length<ls.length) el.appendChild(document.createElement('div'));
        while(el.childNodes.length>ls.length) el.removeChild(el.lastChild);
        ls.forEach((t, i)=>{ const d=el.childNodes[i]; if(d.textContent!==t) d.textContent=t; d.style.fontWeight=i? '400' : '700'; });
      }
      el.style.display='';
      el.style.transform='translate('+Math.round(sx+rect.left-hr.left)+'px,'+Math.round(sy+rect.top-hr.top)+'px) translate(-50%,-100%)';
    }
    for(const [k, el] of this._els) if(!keep.has(k)){ el.remove(); this._els.delete(k); }
  },
};
