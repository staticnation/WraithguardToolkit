
/* =====================================================================================
   Wraithguard: a game controller in the viewer - the Steam Deck's controls or any pad
   the system reports, through the browser's Gamepad API (WebKitGTK reads it through
   libmanette, WebView2 through Windows). Standard (Xbox-style) mapping only, so any
   controller works the same; Steam's ISteamInput is not used (it needs the Steamworks
   SDK and an app id of our own) - Steam hands a non-Steam-game shortcut a standard pad.

   Two modes, A switches between them (Robin's design):

   FLY - the pad moves the camera.
     right stick   look / orbit (as dragging the mouse)
     left stick    WASD: fly forward/back/sideways at the stick's pace
                   orbit: slide the pivot across the ground
     RT / LT       up / down the world's vertical axis (both schemes; analog)
     RB / LB       orbit: zoom in / out      WASD: faster / slower
     Y             switch orbit <-> WASD
     D-pad         step to the neighbouring cell (as the cell arrows)

   CURSOR - the pad is a mouse, for the menus and everything else.
     left stick    move the cursor (faster the further it is pushed)
     right stick   scroll what is under the cursor
     RT            left click            LT   right click
     X             Shift+click (through a door, in the viewport)
     D-pad         up/down: change the focused list; left/right: nudge the focused slider
     B             Escape (close the dialog / panel)

   A real mouse or touch (the Deck's trackpads) always works as itself, and moving it
   leaves FLY mode; a dialog opening switches to CURSOR. Nothing here writes anything;
   the camera moves through the renderer's own turn, fly and orbit goal, so smoothing and
   every other camera rule apply to the pad as to the mouse.
   Copyright (c) 2026 StaticNation.
   ===================================================================================== */
const WgPad={
  DEAD:0.16,            // stick dead zone (a Deck's sticks rest a few hundredths off centre)
  LOOK:2.6,             // radians a second at full deflection
  CURSOR:1100,          // cursor pixels a second at full deflection
  mode:'fly',
  _raf:0, _last:0, _prev:[], _cx:0, _cy:0, _el:null, _badge:null, _hover:null,

  init(){
    if(typeof navigator==='undefined' || !navigator.getGamepads) return;
    window.addEventListener('gamepadconnected',()=>this.start());
    window.addEventListener('gamepaddisconnected',()=>{ if(!this.pad()) this.stop(); });
    // A real mouse or touch takes over: out of FLY (Robin: "keep touch as mouse input
    // always ... and make that automatically exit the fly around mode").
    window.addEventListener('pointermove',e=>{
      if(!e.isTrusted || !this._raf) return;
      if(Math.abs(e.movementX||0)+Math.abs(e.movementY||0)<3) return;
      this._cx=e.clientX; this._cy=e.clientY;
      if(this.mode==='fly') this.setMode('cursor', true);
      this.showCursor(false);   // the system pointer is the cursor now
    },true);
    if(this.pad()) this.start();
  },
  pad(){
    try{ for(const p of navigator.getGamepads()||[]) if(p && p.connected) return p; }catch(_){ }
    return null;
  },
  /** Whether a pad is being read. */
  active(){ return !!this._raf; },
  start(){
    if(this._raf) return;
    this._last=0;
    if(!this._cx){ this._cx=innerWidth/2; this._cy=innerHeight/2; }
    this.setMode(this.mode);
    this._raf=requestAnimationFrame(t=>this.tick(t));
  },
  stop(){
    if(this._raf) cancelAnimationFrame(this._raf);
    this._raf=0;
    const R=App.R; if(R && R.fly) R.fly.pad=null;
    this.showCursor(false);
    if(this._badge) this._badge.hidden=true;
  },

  /* ---- the cursor and the mode badge ---- */
  cursorEl(){
    if(this._el) return this._el;
    const d=document.createElement('div');
    d.id='wgPadCursor';
    d.style.cssText='position:fixed;left:0;top:0;width:18px;height:18px;margin:-2px 0 0 -2px;'+
      'pointer-events:none;z-index:500;border-left:2px solid var(--ac,#e6a33a);'+
      'border-top:2px solid var(--ac,#e6a33a);transform:rotate(-12deg);'+
      'filter:drop-shadow(0 0 2px #000)';
    d.hidden=true;
    document.body.appendChild(d);
    return (this._el=d);
  },
  showCursor(on){
    const d=this.cursorEl();
    d.hidden=!on;
    if(on){ d.style.left=this._cx+'px'; d.style.top=this._cy+'px'; }
  },
  badge(){
    if(this._badge) return this._badge;
    const b=document.createElement('div');
    b.id='wgPadMode';
    b.style.cssText='position:fixed;left:50%;bottom:34px;transform:translateX(-50%);z-index:480;'+
      'pointer-events:none;font-size:11px;padding:3px 10px;border-radius:10px;'+
      'background:rgba(20,20,20,.82);color:var(--tx2,#ccc);border:1px solid var(--line2,#444)';
    document.body.appendChild(b);
    return (this._badge=b);
  },
  setMode(m, byMouse){
    this.mode=m==='cursor'? 'cursor' : 'fly';
    const R=App.R; if(R && R.fly && this.mode!=='fly') R.fly.pad=null;
    if(this.mode==='cursor' && !byMouse) this.showCursor(true);
    if(this.mode==='fly') this.showCursor(false);
    const b=this.badge();
    b.hidden=false;
    b.textContent=this.mode==='fly'
      ? 'Pad: fly - A cursor · RT/LT up/down · RB/LB '+((R&&R.nav==='wasd')? 'speed' : 'zoom')+' · Y orbit/WASD'
      : 'Pad: cursor - A fly · RT click · LT right-click · X shift-click · B back';
    clearTimeout(this._badgeT);
    this._badgeT=setTimeout(()=>{ if(this._badge) this._badge.hidden=true; }, 3500);
  },

  /* ---- reading the pad ---- */
  axis(v){
    v=+v||0; const a=Math.abs(v);
    if(a<this.DEAD) return 0;
    return Math.sign(v)*(a-this.DEAD)/(1-this.DEAD);
  },
  level(p,i){ const b=p.buttons[i]; return b? (typeof b==='object'? (b.value||(b.pressed?1:0)) : +b) : 0; },
  /** True on the frame a button goes down. Every button is read every frame. */
  edges(p){
    const out=[];
    for(let i=0;i<p.buttons.length;i++){
      const now=this.level(p,i)>0.5;
      out[i]=now && !this._prev[i];
      this._prev[i]=now;
    }
    return out;
  },
  modalOpen(){ return !!document.querySelector('.modal:not([hidden])'); },

  tick(t){
    this._raf=0;
    const p=this.pad();
    if(!p){ this.stop(); return; }
    const dt=this._last? Math.min(0.1,(t-this._last)/1000) : 0; this._last=t;
    try{ this.apply(p,dt); }catch(e){ console.warn('gamepad:',e); }
    this._raf=requestAnimationFrame(tt=>this.tick(tt));
  },

  apply(p,dt){
    const down=this.edges(p);
    // A dialog needs the cursor: one opening while flying switches over.
    if(this.mode==='fly' && this.modalOpen()) this.setMode('cursor');
    if(down[0]) this.setMode(this.mode==='fly'? 'cursor' : 'fly');
    if(this.mode==='fly') this.fly(p,dt,down); else this.cursor(p,dt,down);
  },

  /* ---- FLY ---- */
  fly(p,dt,down){
    const R=App.R; if(!R || !R.cam) return;
    const ax=p.axes||[];
    const lx=this.axis(ax[0]), ly=this.axis(ax[1]), rx=this.axis(ax[2]), ry=this.axis(ax[3]);
    const vert=this.level(p,7)-this.level(p,6);   // RT up, LT down
    const wasd=R.nav==='wasd';

    if((rx||ry) && dt>0){ const k=this.LOOK*dt; R.turn(-rx*k, ry*k, wasd); }

    if(wasd && R.fly){
      R.fly.pad=[-ly, lx, Math.abs(vert)>0.05? vert : 0];
      if(down[4]) R.setFlySpeed(R.fly.speed/1.4, true);
      if(down[5]) R.setFlySpeed(R.fly.speed*1.4, true);
    }else if(dt>0){
      if(R.fly) R.fly.pad=null;
      const g=R._orbitGoal? R._orbitGoal() : null;
      if(g){
        const c=R.cam, ca=Math.cos(c.az), sa=Math.sin(c.az);
        const v=g.dist*0.9*dt;   // a pace that scales with how far out the view is
        if(lx||ly){
          g.tx+=(ca*ly - sa*lx)*v;
          g.ty+=(sa*ly + ca*lx)*v;
          R.dirty=true;
        }
        if(Math.abs(vert)>0.05){ g.tz+=vert*v; R.dirty=true; }
        const z=(this.level(p,5)>0.5?1:0)-(this.level(p,4)>0.5?1:0);   // RB in, LB out
        if(z){
          const nd=Math.max(40,Math.min(60000,g.dist*Math.exp(-z*1.6*dt))), k=g.dist>0? nd/g.dist : 1;
          /* Zoom to cursor (Settings) has no pointer to follow in FLY, so it zooms to
             what is in the middle of the view - the wheel's own rule (06_gl.js), with
             the screen centre as the pointer: that point stays put on screen. */
          // Found once per press (a ray through the scene), not every frame it is held.
          if(this._zAnchor===undefined){
            const r=R.opts.zoomToCursor && R._zoomAnchor && R.cv? R.cv.getBoundingClientRect() : null;
            this._zAnchor=r? R._zoomAnchor(r.left+r.width/2, r.top+r.height/2) : null;
          }
          const P=this._zAnchor;
          if(P && k!==1){
            g.tx=P[0]+(g.tx-P[0])*k; g.ty=P[1]+(g.ty-P[1])*k; g.tz=P[2]+(g.tz-P[2])*k;
          }
          g.dist=nd;
          R.dirty=true;
        }else this._zAnchor=undefined;
      }
    }
    if(down[3] && R.setNav){ R.setNav(wasd? 'orbit' : 'wasd'); this.setMode('fly'); }
    const dirs={12:'n', 13:'s', 14:'w', 15:'e'};
    for(const i in dirs) if(down[+i] && R.onArrowClick) R.onArrowClick(dirs[i]);
  },

  /* ---- CURSOR ---- */
  cursor(p,dt,down){
    const ax=p.axes||[];
    const lx=this.axis(ax[0]), ly=this.axis(ax[1]), ry=this.axis(ax[3]), rx=this.axis(ax[2]);
    if((lx||ly) && dt>0){
      // Squared, so a small push is precise and a full one crosses the screen.
      const s=this.CURSOR*dt;
      this._cx=Math.max(0,Math.min(innerWidth-1, this._cx+Math.sign(lx)*lx*lx*s));
      this._cy=Math.max(0,Math.min(innerHeight-1, this._cy+Math.sign(ly)*ly*ly*s));
      this.showCursor(true);
      this.hover();
    }
    if((rx||ry) && dt>0) this.scroll(rx*900*dt, ry*900*dt);
    if(down[7]) this.click(0, false);
    if(down[6]) this.click(2, false);
    if(down[2]) this.click(0, true);
    if(down[1]) this.key('Escape');
    // The D-pad works the focused control: a list (a native list cannot be opened from
    // script, so it is stepped through instead) or a slider.
    const f=document.activeElement;
    if(f && f.tagName==='SELECT'){
      const d=(down[13]?1:0)-(down[12]?1:0);
      if(d){
        const i=Math.max(0,Math.min(f.options.length-1,f.selectedIndex+d));
        if(i!==f.selectedIndex){ f.selectedIndex=i; f.dispatchEvent(new Event('input',{bubbles:true})); f.dispatchEvent(new Event('change',{bubbles:true})); }
      }
    }else if(f && f.tagName==='INPUT' && f.type==='range'){
      const d=(down[15]?1:0)-(down[14]?1:0);
      if(d){
        (d>0? f.stepUp : f.stepDown).call(f);
        f.dispatchEvent(new Event('input',{bubbles:true})); f.dispatchEvent(new Event('change',{bubbles:true}));
      }
    }
  },
  /** What is under the pad's cursor (never the cursor itself: it takes no pointer events). */
  target(){ return document.elementFromPoint(this._cx,this._cy); },
  pointerInit(button){
    return {bubbles:true, cancelable:true, composed:true, clientX:this._cx, clientY:this._cy,
            screenX:this._cx, screenY:this._cy, button, buttons:button===2? 2 : 1,
            pointerId:999, pointerType:'mouse', isPrimary:true};
  },
  hover(){
    const el=this.target(); if(!el) return;
    const o=this.pointerInit(0); o.buttons=0;
    if(el!==this._hover){
      if(this._hover){ this._hover.dispatchEvent(new PointerEvent('pointerout',o)); this._hover.dispatchEvent(new MouseEvent('mouseout',o)); }
      el.dispatchEvent(new PointerEvent('pointerover',o)); el.dispatchEvent(new MouseEvent('mouseover',o));
      this._hover=el;
    }
    el.dispatchEvent(new PointerEvent('pointermove',o));
    el.dispatchEvent(new MouseEvent('mousemove',o));
  },
  click(button, shift){
    const el=this.target(); if(!el) return;
    const R=App.R;
    // The viewport answers a click with a pick (06_gl.js `up`); asked directly, since a
    // scripted press cannot capture the pointer the way its drag handling expects.
    if(R && el===R.cv){
      const ev={clientX:this._cx, clientY:this._cy, button, shiftKey:!!shift};
      if(button===0){
        const a=R.onArrowClick && R.arrowAt? R.arrowAt(this._cx,this._cy) : null;
        if(a){ R.onArrowClick(a.dir,a); return; }
        if(R.onPick && R.pickAt) R.onPick(R.pickAt(this._cx,this._cy), ev);
      }else{
        el.dispatchEvent(new MouseEvent('contextmenu',Object.assign(this.pointerInit(2),{shiftKey:!!shift})));
      }
      return;
    }
    const o=Object.assign(this.pointerInit(button),{shiftKey:!!shift});
    if(button===2){ el.dispatchEvent(new MouseEvent('contextmenu',o)); return; }
    el.dispatchEvent(new PointerEvent('pointerdown',o));
    el.dispatchEvent(new MouseEvent('mousedown',o));
    if(el.focus) try{ el.focus({preventScroll:true}); }catch(_){ }
    el.dispatchEvent(new PointerEvent('pointerup',o));
    el.dispatchEvent(new MouseEvent('mouseup',o));
    // A checkbox's label, a button, a link: the real activation, which also fires `click`.
    if(el.click) el.click(); else el.dispatchEvent(new MouseEvent('click',o));
  },
  scroll(dx,dy){
    let el=this.target();
    while(el && el!==document.body){
      const cs=getComputedStyle(el);
      if((/(auto|scroll)/.test(cs.overflowY) && el.scrollHeight>el.clientHeight) ||
         (/(auto|scroll)/.test(cs.overflowX) && el.scrollWidth>el.clientWidth)){ el.scrollBy(dx,dy); return; }
      el=el.parentElement;
    }
    // Over the viewport: the wheel, which zooms (orbit) or sets the pace (WASD).
    const R=App.R;
    if(R && this.target()===R.cv && Math.abs(dy)>4){
      R.cv.dispatchEvent(new WheelEvent('wheel',{bubbles:true,cancelable:true,clientX:this._cx,clientY:this._cy,deltaY:dy*4}));
    }
  },
  key(k){
    const t=document.activeElement||document.body;
    t.dispatchEvent(new KeyboardEvent('keydown',{key:k,code:k,bubbles:true,cancelable:true}));
    t.dispatchEvent(new KeyboardEvent('keyup',{key:k,code:k,bubbles:true,cancelable:true}));
  },
};
WgPad.init();
