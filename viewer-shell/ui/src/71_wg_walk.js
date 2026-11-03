/* =====================================================================================
   Wraithguard: the first-person body (06_gl.js's walking camera, `R.walkBody`).

   - Its pace is OpenMW's, from the body's stats and the load order's game settings
     (`game_settings`' `move`): walk fMinWalkSpeed + Speed% of (fMaxWalkSpeed - fMinWalkSpeed),
     less fEncumberedMoveEffect of the encumbrance; run that times fBaseRunMultiplier +
     Athletics% of fAthleticsRunBonus; sneak fSneakSpeedMultiplier of the walk; swim the walk
     or run times (1 + Swift Swim%) times fSwimRunBase + Athletics% of fSwimRunAthleticsMult;
     levitate fMinFlySpeed + (Speed + Levitate)% of (fMaxFlySpeed - fMinFlySpeed); the jump
     fJumpAcrobaticsBase + (Acrobatics up to 50 / 15) ^ fJumpAcroMultiplier, 3 x
     fJumpAcroMultiplier a point past 50, 64 a point of Jump, by the encumbrance, the run and
     fatigue terms, then (x + gravity) / 3; a sideways step is 3/4 (OpenMW's strafing). The
     stats are sliders (0.01-255), a preset, or a save's player (`walk_player_from_save`).
   - It collides with the objects' collision shapes (`collision_bundle`): a mesh's
     RootCollisionNode, else its drawn triangles, and nothing for a mesh with no collision,
     as OpenMW has it - not with every drawn triangle, as the camera's picking does. Actors
     stand in the way as upright cylinders.
   - It hears its own steps (Preview's Sounds on): the left and right foot by the boots'
     weight (FootBare/footLight/FootMed/footHeavy), FootWater wading, Swim Left/Right
     swimming, DefaultLand(Water) landing; at the footsteps volume.

   The formulas and sound ids are read from OpenMW's behaviour (apps/openmw/mwclass/npc.cpp,
   GPL-3: a reference for what the game does, not code). Copyright (c) 2026 StaticNation,
   GPL-2.0 as part of this page.
   ===================================================================================== */
const WgWalk={
  st:{speed:50, athletics:30, acrobatics:30, encumbrance:0, fatigue:100, levitate:0, jump:0, swiftSwim:0,
      levOn:false, feet:'bare', footVol:20, collide:true},
  KEEP:['speed','athletics','acrobatics','encumbrance','fatigue','levitate','jump','swiftSwim','levOn','feet','footVol','collide'],
  STATS:[['speed','Speed','The attribute: walking, running and levitating speed'],
    ['athletics','Athletics','The skill: running and swimming speed'],
    ['acrobatics','Acrobatics','The skill: how high the body jumps'],
    ['encumbrance','Encumbrance %','How much of what the body can carry it carries: slower, and lower jumps; over 100 it cannot move'],
    ['fatigue','Fatigue %','How rested the body is: a tired body jumps lower'],
    ['levitate','Levitate','The effect\'s magnitude (L turns levitation on and off)'],
    ['jump','Jump','The effect\'s magnitude: 64 units of jump a point'],
    ['swiftSwim','Swift Swim','The effect\'s magnitude: a percent faster in water']],
  PRESETS:{
    fresh:{label:'A new character', speed:40, athletics:5, acrobatics:5},
    trained:{label:'Athletics and Acrobatics as majors', speed:50, athletics:30, acrobatics:30},
    seasoned:{label:'Well into the game', speed:70, athletics:60, acrobatics:60},
    master:{label:'Mastered (100)', speed:100, athletics:100, acrobatics:100},
    boosted:{label:'Fortified to 255', speed:255, athletics:255, acrobatics:255},
  },
  // Morrowind's defaults, until the load order's are read.
  GM:{fMinWalkSpeed:100, fMaxWalkSpeed:200, fBaseRunMultiplier:1.75, fAthleticsRunBonus:1, fEncumberedMoveEffect:0.3,
      fSneakSpeedMultiplier:0.75, fSwimRunBase:0.5, fSwimRunAthleticsMult:0.1, fSwimHeightScale:0.9, fMinFlySpeed:5,
      fMaxFlySpeed:50, fJumpAcrobaticsBase:128, fJumpAcroMultiplier:4, fJumpRunMultiplier:1, fJumpEncumbranceBase:0.5,
      fJumpEncumbranceMultiplier:1, fFatigueBase:1.25, fFatigueMult:0.5},
  GRAVITY:9.8*69.99125,   // OpenMW: GravityConst x UnitsPerMeter
  BODY_H:128,             // the height swimming is measured against (fSwimHeightScale of it)
  gm:null, _gmSig:undefined,
  _el:null, _stride:0, _left:false, _sounds:new Map(),
  // The collision world: shapes by mesh, and the triangles of the objects near the body.
  _shapes:new Map(), _world:null, _picksRef:null, _building:false, _builtAt:null,
  CELL:256, NEAR:12000,

  restore(){
    try{
      const v=JSON.parse(localStorage.getItem('wgWalk')||'{}');
      for(const k of this.KEEP) if(v[k]!=null) this.st[k]=v[k];
    }catch(_){}
  },
  keep(){
    try{ const v={}; for(const k of this.KEEP) v[k]=this.st[k]; localStorage.setItem('wgWalk', JSON.stringify(v)); }catch(_){}
  },

  /** On the renderer, once it is there. */
  attach(){
    const R=App.R; if(!R || R.walkBody===this) return;
    this.restore();
    R.walkBody=this;
    const b=$('#tbBody'); if(b && !b._wired){ b._wired=true; b.onclick=()=>this.toggle(); }
    window.addEventListener('keydown', e=>{
      if(!R.walk || e.code!=='KeyL' || e.ctrlKey || e.altKey || e.metaKey) return;
      const t=e.target; if(t && (t.tagName==='INPUT' || t.tagName==='TEXTAREA' || t.isContentEditable)) return;
      e.preventDefault(); this.setLevitate(!this.st.levOn);
    });
  },

  /** The load order's movement settings (read again when the install changes). */
  async settings(){
    const sig=(typeof GameData==='object')? GameData.sig : null;
    if(this.gm && this._gmSig===sig) return this.gm;
    this._gmSig=sig;
    try{ const g=await Engine.call('game_settings'); this.gm=Object.assign({}, this.GM, (g && g.move)||{}); }
    catch(_){ this.gm=Object.assign({}, this.GM); }
    return this.gm;
  },
  g(k){ const m=this.gm||this.GM; return +(m[k]!=null? m[k] : this.GM[k]); },

  levitating(){ return !!(this.st.levOn && this.st.levitate>0); },
  setLevitate(on){
    this.st.levOn=!!on;
    if(on && !(this.st.levitate>0)) this.st.levitate=10;
    this.keep(); this.draw();
    if(App.R) App.R.walkSt.ground=false;
    if(typeof toast==='function') toast(on? 'Levitating (L to stop)' : 'Levitation off', 'ok', 1500);
  },

  /** OpenMW's speeds for the body as it stands (units a second), and its jump. */
  speeds(){
    if(!this.gm) this.settings();
    const s=this.st, g=k=>this.g(k);
    const enc=Math.max(0, +s.encumbrance||0)/100, fat=Math.max(0, Math.min(100, +s.fatigue))/100;
    const speed=+s.speed||0, ath=+s.athletics||0, acro=+s.acrobatics||0;
    if(enc>1) return {walk:0, run:0, sneak:0, swimWalk:0, swimRun:0, fly:0, jump:0, jumpWalk:0, gravity:this.GRAVITY, swimDepth:g('fSwimHeightScale')*this.BODY_H, strafe:0.75};
    let walk=g('fMinWalkSpeed')+0.01*speed*(g('fMaxWalkSpeed')-g('fMinWalkSpeed'));
    walk=Math.max(0, walk*(1-g('fEncumberedMoveEffect')*enc));
    const run=walk*(0.01*ath*g('fAthleticsRunBonus')+g('fBaseRunMultiplier'));
    const sneak=walk*g('fSneakSpeedMultiplier');
    const swimMult=(1+0.01*(+s.swiftSwim||0))*(g('fSwimRunBase')+0.01*ath*g('fSwimRunAthleticsMult'));
    let fly=g('fMinFlySpeed')+0.01*(speed+(+s.levitate||0))*(g('fMaxFlySpeed')-g('fMinFlySpeed'));
    fly=Math.max(0, fly*(1-g('fEncumberedMoveEffect')*enc));
    const a=Math.min(acro, 50), b=Math.max(0, acro-50);
    let x=g('fJumpAcrobaticsBase')+Math.pow(Math.max(0, a)/15, g('fJumpAcroMultiplier'));
    x+=3*b*g('fJumpAcroMultiplier');
    x+=(+s.jump||0)*64;
    x*=g('fJumpEncumbranceBase')+g('fJumpEncumbranceMultiplier')*(1-enc);
    x*=g('fFatigueBase')-g('fFatigueMult')*(1-fat);
    const jumpWalk=(x+this.GRAVITY)/3;
    x=(x*g('fJumpRunMultiplier')+this.GRAVITY)/3;   // running (the game's default stance)
    return {walk, run, sneak, swimWalk:walk*swimMult, swimRun:run*swimMult, fly, jump:x, jumpWalk, gravity:this.GRAVITY,
      swimDepth:g('fSwimHeightScale')*this.BODY_H, strafe:0.75};
  },

  /* ---- collision --------------------------------------------------------------------- */

  /** A ray against the collision world: the nearest hit within `max`, null for none, or
   *  undefined while the world is not built (06_gl.js then asks the drawn meshes). */
  ray(o, d, max){
    if(!this.st.collide) return undefined;
    const R=App.R; if(!R) return undefined;
    this.watch(o);
    const W=this._world; if(!W) return undefined;
    max=max||1e5;
    const keys=new Set(), C=this.CELL;
    if(!d[0] && !d[1]) keys.add(Math.floor(o[0]/C)+','+Math.floor(o[1]/C));
    else{
      const L=Math.min(max, 4*C);
      for(let t=0;t<=L;t+=C/2){ const x=o[0]+d[0]*t, y=o[1]+d[1]*t;
        for(const dx of [-1,0,1]) for(const dy of [-1,0,1]) keys.add((Math.floor(x/C)+dx)+','+(Math.floor(y/C)+dy)); }
    }
    let best=Infinity;
    const T=W.tris, seen=new Set();
    for(const k of keys){
      const list=W.grid.get(k); if(!list) continue;
      for(const i of list){
        if(seen.has(i)) continue; seen.add(i);
        const t=this.hit(o, d, T, i*9);
        if(t>=0 && t<best && t<=max) best=t;
      }
    }
    return best<Infinity? {t:best} : null;
  },
  /** Möller-Trumbore, both faces: the distance along d, or -1. */
  hit(o, d, T, j){
    const ax=T[j], ay=T[j+1], az=T[j+2];
    const e1x=T[j+3]-ax, e1y=T[j+4]-ay, e1z=T[j+5]-az, e2x=T[j+6]-ax, e2y=T[j+7]-ay, e2z=T[j+8]-az;
    const px=d[1]*e2z-d[2]*e2y, py=d[2]*e2x-d[0]*e2z, pz=d[0]*e2y-d[1]*e2x;
    const det=e1x*px+e1y*py+e1z*pz;
    if(Math.abs(det)<1e-9) return -1;
    const inv=1/det, sx=o[0]-ax, sy=o[1]-ay, sz=o[2]-az;
    const u=(sx*px+sy*py+sz*pz)*inv; if(u<0 || u>1) return -1;
    const qx=sy*e1z-sz*e1y, qy=sz*e1x-sx*e1z, qz=sx*e1y-sy*e1x;
    const v=(d[0]*qx+d[1]*qy+d[2]*qz)*inv; if(v<0 || u+v>1) return -1;
    const t=(e2x*qx+e2y*qy+e2z*qz)*inv;
    return t>=0? t : -1;
  },
  /** An actor where the body would stand: an upright cylinder from its bounds. */
  actorAt(x, y, feet, r){
    const W=this._world; if(!W || !this.st.collide) return false;
    for(const a of W.actors){
      if(feet+WALK.eye<a.z0 || feet>a.z1) continue;
      if(Math.hypot(x-a.x, y-a.y)<a.r+r) return true;
    }
    return false;
  },
  isActor(p){
    if(String(p.model||'').startsWith('__npc/')) return true;
    return typeof CellData==='object' && CellData.actors && CellData.actors.has(String(p.id||'').toLowerCase());
  },
  /** Builds the world again when the scene's objects change or the body walks far from
   *  where it was built. */
  watch(o){
    const R=App.R, picks=(R && R.pickables)||null;
    const far=this._builtAt && Math.hypot(o[0]-this._builtAt[0], o[1]-this._builtAt[1])>this.NEAR/3;
    if((picks!==this._picksRef || far) && !this._building) this.build(picks, [o[0], o[1]]);
  },
  async build(picks, at){
    this._building=true;
    try{
      const near=(picks||[]).filter(p=>p && p.m && Math.hypot(p.m[3]-at[0], p.m[7]-at[1])<this.NEAR);
      const actors=[], solid=[];
      for(const p of near){
        if(this.isActor(p)){
          const a=p.aabb;
          if(a) actors.push({x:(a.x0+a.x1)/2, y:(a.y0+a.y1)/2, z0:a.z0, z1:a.z1, r:Math.min(40, Math.max(18, Math.min(a.x1-a.x0, a.y1-a.y0)/2))});
          else actors.push({x:p.m[3], y:p.m[7], z0:p.m[11], z1:p.m[11]+128, r:30});
        }else if(p.model && p.model!==(typeof CORPSE_SLAB==='string'? CORPSE_SLAB : '')) solid.push(p);
      }
      const want=[...new Set(solid.map(p=>p.model))].filter(m=>!this._shapes.has(m));
      for(let i=0;i<want.length;i+=150){
        const part=want.slice(i, i+150);
        let buf;
        try{ buf=await Engine.bytes('collision_bundle',{meshes:part.map(m=>/^meshes[\\/]/i.test(m)? m : 'meshes\\'+m)}); }
        catch(_){ for(const m of part) this._shapes.set(m, null); continue; }
        const dv=new DataView(buf); let o=8; const n=dv.getUint32(4,true);
        for(let j=0;j<n;j++){
          const pl=dv.getUint16(o,true); o+=2+pl;
          const bl=dv.getUint32(o,true); o+=4; const s=o; o+=bl;
          if(bl<9){ this._shapes.set(part[j], null); continue; }
          const shape=dv.getUint8(s+4), nt=dv.getUint32(s+5,true);
          this._shapes.set(part[j], shape===2? {shape, f:new Float32Array(0)} : {shape, f:new Float32Array(buf.slice(s+9, s+9+nt*36))});
        }
      }
      let count=0;
      for(const p of solid){ const sh=this._shapes.get(p.model); if(sh) count+=sh.f.length/9; }
      const tris=new Float32Array(count*9), grid=new Map(), C=this.CELL;
      let t=0;
      for(const p of solid){
        const sh=this._shapes.get(p.model); if(!sh || !sh.f.length) continue;
        const m=p.m, f=sh.f;
        for(let i=0;i+8<f.length;i+=9){
          let x0=Infinity, y0=Infinity, x1=-Infinity, y1=-Infinity;
          for(let v=0;v<3;v++){
            const x=f[i+v*3], y=f[i+v*3+1], z=f[i+v*3+2];
            const wx=m[0]*x+m[1]*y+m[2]*z+m[3], wy=m[4]*x+m[5]*y+m[6]*z+m[7], wz=m[8]*x+m[9]*y+m[10]*z+m[11];
            tris[t*9+v*3]=wx; tris[t*9+v*3+1]=wy; tris[t*9+v*3+2]=wz;
            if(wx<x0) x0=wx; if(wx>x1) x1=wx; if(wy<y0) y0=wy; if(wy>y1) y1=wy;
          }
          for(let gx=Math.floor(x0/C); gx<=Math.floor(x1/C); gx++)
            for(let gy=Math.floor(y0/C); gy<=Math.floor(y1/C); gy++){
              const k=gx+','+gy; let l=grid.get(k); if(!l){ l=[]; grid.set(k, l); } l.push(t);
            }
          t++;
        }
      }
      this._world={tris, grid, actors, count:t};
      this._picksRef=picks; this._builtAt=at;
    }finally{ this._building=false; }
  },

  /* ---- sound ------------------------------------------------------------------------- */

  FEET:{bare:['FootBareLeft','FootBareRight'], light:['footLightLeft','footLightRight'],
        medium:['FootMedLeft','FootMedRight'], heavy:['footHeavyLeft','footHeavyRight']},
  /** The body moved `dist`: a foot down every stride, as the game's walk and run cycles put
   *  them (about 70 units a step walking, 110 running, 60 sneaking, 180 a stroke). */
  step(dist, o){
    if(o.mode==='fly' || (o.mode==='walk' && !o.ground)) { this._stride=0; return; }
    this._stride+=dist;
    const L=o.mode==='swim'? 180 : o.sneak? 60 : o.running? 110 : 70;
    if(this._stride<L) return;
    this._stride-=L; this._left=!this._left;
    const side=this._left? 0 : 1;
    const id=o.mode==='swim'? ['Swim Left','Swim Right'][side]
      : o.wading? ['FootWaterLeft','FootWaterRight'][side]
      : (this.FEET[this.st.feet]||this.FEET.bare)[side];
    this.play(id, o.sneak? 0.5 : 1);
  },
  /** Down after a fall or a jump: DefaultLand, or DefaultLandWater in water. */
  land(speed, wet){
    if(speed>150) this.play(wet? 'DefaultLandWater' : 'DefaultLand', Math.min(1, speed/600));
  },
  async play(id, gain){
    if(typeof WgSound!=='object' || !WgSound.place.on) return;
    let d=this._sounds.get(id);
    if(d===undefined){
      try{ const rows=await Engine.call('sound_records',{ids:[id]}); d=Array.isArray(rows) && rows[0]? rows[0] : null; }
      catch(_){ d=null; }
      this._sounds.set(id, d);
    }
    if(!d) return;
    WgSound.oneShot(Object.assign({}, d, {gain:d.gain*gain*Math.max(0, this.st.footVol)/100}));
  },

  /* ---- the panel --------------------------------------------------------------------- */

  toggle(){
    if(this._el && !this._el.hidden){ this._el.hidden=true; $('#tbBody')?.classList.remove('on'); return; }
    this.panel(); this._el.hidden=false; $('#tbBody')?.classList.add('on');
  },
  panel(){
    if(this._el) return this.draw();
    const d=document.createElement('div');
    d.id='wgWalkPanel'; d.className='ori'; d.hidden=true;
    d.innerHTML='<div class="orihead"><b>First-person body</b><span style="flex:1"></span>'+
      '<button class="btn dim ic" id="wgWalkX" title="Close">&#x2715;</button></div><div class="oribody" id="wgWalkBody"></div>';
    if(typeof WgUI==='object' && WgUI.mount) WgUI.mount(d); else ($('#vpwrap')||document.body).appendChild(d);
    d.querySelector('#wgWalkX').onclick=()=>this.toggle();
    this._el=d;
    this.draw();
  },
  draw(){
    const box=this._el && this._el.querySelector('#wgWalkBody'); if(!box) return;
    const s=this.st, sp=this.speeds();
    const opt=(v, t)=>'<option value="'+v+'">'+escHtml(t)+'</option>';
    // A slider over 0.01..255 that is fine where most stats sit: the bar is the square root.
    const pos=v=>Math.round(Math.sqrt(Math.max(0.01, Math.min(255, +v||0)))/Math.sqrt(255)*1000);
    const val=p=>Math.max(0.01, Math.round(Math.pow(p/1000*Math.sqrt(255), 2)*100)/100);
    let h='<div class="row"><label>Preset</label><div class="ctl"><select class="fld" id="wgWalkPre"><option value="">Choose…</option>'+
      Object.entries(this.PRESETS).map(([k,p])=>opt(k, p.label+' ('+p.speed+' / '+p.athletics+' / '+p.acrobatics+')')).join('')+
      '</select> <button class="btn sm" id="wgWalkSave" title="The player\'s Speed, Athletics, Acrobatics and fatigue from an OpenMW save">From a save…</button></div></div>';
    for(const [k, label, tip] of this.STATS){
      const pct=k==='encumbrance' || k==='fatigue';
      const max=k==='encumbrance'? 110 : k==='fatigue'? 100 : 255;
      const v=+s[k]||0;
      h+='<div class="row"><label title="'+escHtml(tip)+'">'+escHtml(label)+'</label><div class="ctl"><div class="rng">'+
        (pct? '<input type="range" min="0" max="'+max+'" step="1" value="'+v+'" data-k="'+k+'" data-pct="1">'
            : '<input type="range" min="0" max="1000" step="1" value="'+pos(v)+'" data-k="'+k+'">')+
        '<input class="fld val" style="width:62px" type="number" min="'+(pct? 0 : 0.01)+'" max="'+max+'" step="0.01" value="'+v+'" data-n="'+k+'"></div></div></div>';
    }
    h+='<div class="row"><label title="What the steps sound like: the boots\' armor weight">Feet</label><div class="ctl"><select class="fld" id="wgWalkFeet">'+
      opt('bare','Bare (or shoes)')+opt('light','Light boots')+opt('medium','Medium boots')+opt('heavy','Heavy boots')+'</select></div></div>';
    h+='<div class="row"><label title="How loud the steps are, under the Preview\'s sound volume (OpenMW\'s footsteps volume is 20 by default)">Footsteps</label><div class="ctl"><div class="rng">'+
      '<input type="range" min="0" max="100" step="1" value="'+(+s.footVol||0)+'" id="wgWalkVol"><span class="val">'+(+s.footVol||0)+'</span></div></div></div>';
    h+='<div class="lrow" style="display:flex;gap:6px;flex-wrap:wrap;margin:6px 0">'+
      '<button class="btn sm'+(this.levitating()? ' pri' : '')+'" id="wgWalkLev" title="Levitate (L): fly where you look, E up and Q down">Levitate</button>'+
      '<button class="btn sm'+(s.collide? ' pri' : '')+'" id="wgWalkCol" title="Collide with the objects\' collision shapes, as the game does; off, with what is drawn">Collision shapes</button></div>';
    const n=v=>Math.round(v);
    h+='<div class="hint">Walk '+n(sp.walk)+' · run '+n(sp.run)+' · sneak '+n(sp.sneak)+' · swim '+n(sp.swimWalk)+'/'+n(sp.swimRun)+
      ' · levitate '+n(sp.fly)+' units a second · jump '+n(sp.jump*sp.jump/(2*sp.gravity))+' units high. '+
      'Shift walks, Ctrl sneaks, Space jumps, L levitates; in water or levitating, W goes where you look, E up, Q down.'+
      (this._world? ' Colliding with '+this._world.count.toLocaleString()+' triangles nearby.' : '')+'</div>';
    box.innerHTML=h;
    const feet=box.querySelector('#wgWalkFeet'); feet.value=s.feet||'bare';
    feet.onchange=()=>{ s.feet=feet.value; this.keep(); };
    box.querySelectorAll('input[data-k]').forEach(r=>r.oninput=()=>{
      const k=r.dataset.k; s[k]=r.dataset.pct? +r.value : val(+r.value);
      box.querySelector('[data-n="'+k+'"]').value=s[k]; this.keep(); this.hint();
    });
    box.querySelectorAll('input[data-n]').forEach(r=>r.onchange=()=>{
      const k=r.dataset.n, pct=k==='encumbrance' || k==='fatigue';
      let v=parseFloat(r.value); if(!isFinite(v)) v=0;
      s[k]=pct? Math.max(0, Math.min(k==='fatigue'? 100 : 110, v)) : Math.max(0.01, Math.min(255, v));
      this.keep(); this.draw();
    });
    box.querySelector('#wgWalkVol').oninput=e=>{ s.footVol=+e.target.value; e.target.nextElementSibling.textContent=s.footVol; this.keep(); };
    box.querySelector('#wgWalkPre').onchange=e=>{
      const p=this.PRESETS[e.target.value]; if(!p) return;
      Object.assign(s, {speed:p.speed, athletics:p.athletics, acrobatics:p.acrobatics}); this.keep(); this.draw();
    };
    box.querySelector('#wgWalkSave').onclick=()=>this.fromSave();
    box.querySelector('#wgWalkLev').onclick=()=>this.setLevitate(!s.levOn);
    box.querySelector('#wgWalkCol').onclick=()=>{ s.collide=!s.collide; this.keep(); this.draw(); };
  },
  /** The speeds line again, without rebuilding the sliders under the pointer. */
  hint(){ clearTimeout(this._hintT); this._hintT=setTimeout(()=>this.draw(), 400); },

  async fromSave(){
    let p;
    try{ p=await Engine.pick('open',{filters:[{name:'OpenMW saves', extensions:['omwsave']}]}); }catch(_){ return; }
    if(!p) return;
    try{
      let r=await Engine.call('walk_player_from_save',{path:String(p)});
      if(typeof r==='string') r=JSON.parse(r);
      const s=this.st;
      s.speed=Math.round(r.attributes[4]*100)/100;
      s.athletics=Math.round(r.skills[8]*100)/100;
      s.acrobatics=Math.round(r.skills[20]*100)/100;
      const f=r.fatigue||[1,1]; s.fatigue=f[1]>0? Math.round(Math.max(0, Math.min(1, f[0]/f[1]))*100) : 100;
      this.keep(); this.draw();
      toast((r.name||'The player')+': Speed '+s.speed+', Athletics '+s.athletics+', Acrobatics '+s.acrobatics,'ok',4000);
    }catch(e){ toast(String(e.message||e),'err',6000); }
  },
};
