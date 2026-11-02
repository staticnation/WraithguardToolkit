/* =====================================================================================
   Wraithguard: cell borders and names in the render window (TODO, "The world": beside the
   cell arrows). The viewport's "Cell borders" switch.

   Each exterior cell of the scene gets its four edges drawn on the ground - sampled every
   land vertex (128 units) and lifted a little so the ground does not swallow them - as an
   overlay (`setOverlay`, 47_wg_overlay.js), faintly through hills in front. Its name, or
   its grid where it has none, and its region stand at its middle as a label that follows
   the camera; a label behind the camera, or far off, is hidden.
   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
const WgCellGrid={
  on:false,
  CELL:8192, STEP:128, LIFT:12,
  COL:[1, 0.82, 0.25, 0.85],
  _scene:null, _raf:0, _box:null, _labels:[],

  /** The viewport switch. */
  set(on){
    this.on=!!on;
    try{ localStorage.setItem('wgCellGrid', this.on? '1' : '0'); }catch(_){}
    const b=$('#tbBorders'); if(b) b.classList.toggle('on', this.on);
    if(this.on){ this._scene=null; this.loop(); }
    else this.clear();
  },

  wire(){
    const b=$('#tbBorders'); if(!b || b._wired) return;
    b._wired=true;
    b.onclick=()=>this.set(!this.on);
    let was=false; try{ was=localStorage.getItem('wgCellGrid')==='1'; }catch(_){}
    this.set(was);
  },

  clear(){
    cancelAnimationFrame(this._raf); this._raf=0;
    if(App.R) App.R.setOverlay('cellgrid', null);
    if(this._box){ this._box.remove(); this._box=null; }
    this._labels=[]; this._scene=null;
  },

  loop(){
    cancelAnimationFrame(this._raf);
    const step=()=>{
      if(!this.on) return;
      this._raf=requestAnimationFrame(step);
      const sc=App._scene||null, R=App.R;
      if(!R) return;
      if(sc!==this._scene){ this._scene=sc; this.build(sc); }
      this.place();
    };
    this._raf=requestAnimationFrame(step);
  },

  /** The ground's height at a scene point: the drawn land, else the cell's flat zero. */
  z(x, y){
    const R=App.R, g=R && R.groundZ? R.groundZ(x, y) : null;
    return (g!=null && isFinite(g))? g : 0;
  },

  build(sc){
    const R=App.R;
    R.setOverlay('cellgrid', null);
    if(this._box){ this._box.remove(); this._box=null; }
    this._labels=[];
    if(!sc || !sc.origin || (sc.centre && sc.centre.kind==='int')) return;
    const o=sc.origin, C=this.CELL, S=this.STEP, L=new OvlLines();
    const seen=new Set();
    const edge=(x0,y0,x1,y1)=>{
      const k=[x0,y0,x1,y1].join(',');
      if(seen.has(k)) return; seen.add(k);
      const n=Math.round(Math.hypot(x1-x0, y1-y0)/S);
      let prev=null;
      for(let i=0;i<=n;i++){
        const x=x0+(x1-x0)*i/n, y=y0+(y1-y0)*i/n, p=[x, y, this.z(x, y)+this.LIFT];
        if(prev) L.seg(prev, p, this.COL);
        prev=p;
      }
    };
    const box=document.createElement('div');
    box.className='wgCellNames';
    const host=R.cv.parentElement||document.body;
    host.appendChild(box);
    this._box=box;
    for(const c of sc.cells||[]){
      if(!c || c.kind==='int') continue;
      // The cell's south-west corner in scene units (the scene is drawn about its origin).
      const x0=c.gx*C-o[0], y0=c.gy*C-o[1], x1=x0+C, y1=y0+C;
      edge(x0,y0,x1,y0); edge(x0,y1,x1,y1); edge(x0,y0,x0,y1); edge(x1,y0,x1,y1);
      const named=(c.name||'').trim();
      const el=document.createElement('div');
      el.className='wgCellName';
      el.innerHTML=(named? '<b>'+escHtml(named)+'</b> ' : '')+'<span>('+c.gx+', '+c.gy+')</span>'+
        (c.region? '<div class="r">'+escHtml(c.region)+'</div>' : '');
      box.appendChild(el);
      const mx=(x0+x1)/2, my=(y0+y1)/2;
      this._labels.push({el, p:[mx, my, this.z(mx, my)+160]});
    }
    R.setOverlay('cellgrid', L, {xray:true});
  },

  /** The labels where the camera now sees their cells' middles. */
  place(){
    const R=App.R; if(!R || !this._labels.length) return;
    const rect=R.cv.getBoundingClientRect(), W=rect.width||1, H=rect.height||1;
    const c=R.cam, ce=Math.cos(c.el), se=Math.sin(c.el);
    const eye=[c.tx+c.dist*ce*Math.cos(c.az), c.ty+c.dist*ce*Math.sin(c.az), c.tz+c.dist*se];
    const [n,f]=R.nearFar();
    const VP=M4.mul(M4.persp(FOV, W/H, n, f), M4.lookAt(eye,[c.tx,c.ty,c.tz],[0,0,1]));
    const host=R.cv.parentElement, hr=host? host.getBoundingClientRect() : rect;
    const far=this.CELL*4;
    for(const L of this._labels){
      const p=L.p;
      const x=VP[0]*p[0]+VP[4]*p[1]+VP[8]*p[2]+VP[12];
      const y=VP[1]*p[0]+VP[5]*p[1]+VP[9]*p[2]+VP[13];
      const w=VP[3]*p[0]+VP[7]*p[1]+VP[11]*p[2]+VP[15];
      const d=Math.hypot(p[0]-eye[0], p[1]-eye[1], p[2]-eye[2]);
      const sx=((x/w)*0.5+0.5)*W, sy=(1-((y/w)*0.5+0.5))*H;
      if(!(w>0) || d>far || sx<-200 || sx>W+200 || sy<-50 || sy>H+50){ L.el.style.display='none'; continue; }
      L.el.style.display='';
      L.el.style.opacity=String(Math.max(0.35, 1-d/far));
      L.el.style.transform='translate('+Math.round(sx+rect.left-hr.left)+'px,'+Math.round(sy+rect.top-hr.top)+'px) translate(-50%,-50%)';
    }
  },
};
