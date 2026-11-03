/* =====================================================================================
   Wraithguard: OpenMW Lua in the scene (the Preview's Scripts, beside MWScript).

   The Lua runs in the engine (viewcore::luarun), on the install's own LuaJIT where it finds
   one and Lua 5.4 where not: OpenMW's built-ins from its resources folder (I.AI,
   activation, the music playlists, weather) and the load order's registered scripts, each
   on what it is registered for - GLOBAL, PLAYER, and NPC, CREATURE, LIGHT, DOOR and the
   rest on every loaded object of that type.

   This side keeps the engine's copy of the scene current and does what the scripts did.
   Each frame it sends what changed (objects, the player, the clock, the weather, MWScript's
   globals and locals, actors, what is playing, activations) and applies what comes back,
   through the same layer as MWScript's (73_wg_scripts.js): objects enabled and disabled,
   moved, scaled, created and removed; sounds and music; the weather and the hour;
   globals; actors' health, inventories and AI packages; messages. One frame is in flight
   at a time. Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
const WgLua={
  running:false, busy:false, note:'', backend:'', resources:'', _status:null,
  types:new Map(),       // lowercased object id -> record tag
  sent:new Map(),        // object key -> what was last sent of it (a signature)
  _sentG:new Map(), _sentA:new Map(), _sentL:new Map(), _sentP:'',
  _dt:0, _acts:[], _all:true,

  say(path, kind, text){ WgScripts.say(path? String(path).replace(/^scripts\//i, '') : 'lua', kind==='print'? 'do' : kind, text); },
  weathers(){
    const w=typeof $==='function' && $('#p_weather');
    return w && w.options && w.options.length? [...w.options].map(o=>o.text||o.value||'') : ['Clear','Cloudy','Foggy','Overcast','Rain','Thunderstorm','Ashstorm','Blight','Snow','Blizzard'];
  },

  /** The engine's Lua started; the whole scene is sent with the first frame. */
  async start(){
    await this.stop();
    let got;
    try{ got=await Engine.call('lua_run_start',{weathers:this.weathers()}); }
    catch(e){ this.say('', 'error', 'OpenMW Lua did not start: '+String(e && e.message || e)); return; }
    this.note=got.note||''; this.backend=got.backend||''; this.resources=got.resources||'';
    if(!got.answer){ if(this.note) this.say('', 'skip', this.note); return; }
    this.running=true; this._all=true;
    this.sent.clear(); this._sentG.clear(); this._sentA.clear(); this._sentL.clear(); this._sentP='';
    // OpenMW's playlists choose the music while its built-ins run.
    if(this.resources && typeof WgSound==='object') WgSound.place.scriptMusic=true;
    this.say('', 'do', 'OpenMW Lua on '+this.backend+(this.note? ' - '+this.note : ''));
    this.apply(got.answer);
    await this.gather();
  },
  async stop(){
    const was=this.running;
    this.running=false; this.busy=false; this._dt=0; this._acts=[]; this._status=null;
    if(typeof WgLuaUI==='object') WgLuaUI.clear();
    if(typeof WgSound==='object') WgSound.place.scriptMusic=false;
    if(was){ try{ await Engine.call('lua_run_stop',{}); }catch(_){ } }
  },
  /** The record types of the scene's objects (which decide the scripts they get). */
  async gather(){
    if(!this.running) return;
    const ids=new Set();
    for(const c of (App._scene && App._scene.cells)||[]) for(const r of c.refs||[]) if(r && r.id) ids.add(String(r.id).toLowerCase());
    for(const p of WgScripts.placed) ids.add(String(p.id).toLowerCase());
    const ask=[...ids].filter(id=>!this.types.has(id));
    if(ask.length){
      try{ const got=await Engine.call('lua_scene',{ids:ask}); for(const [id, tg] of Object.entries(got.types||{})) this.types.set(id, tg); }catch(_){ }
      for(const id of ask) if(!this.types.has(id)) this.types.set(id, '');
    }
    this._all=true;
  },
  activate(key){ if(this.running) this._acts.push({key, by:'player'}); },

  /* ---- what is sent ------------------------------------------------------------------ */

  /** One object as the engine's scene holds it; with its model and the renderer's matrix
   *  where it is drawn, for the collision nearby.castRay casts against. */
  desc(key, r, cell){
    const S=WgScripts, t=this.t(key, r);
    const p=S.posOf(t), q=S.rotOf(t), m=S.moved.get(key);
    const ext=!cell || cell.kind!=='int';
    const pk=this._picks && this._picks.get(key);
    return {id:String(r.id).toLowerCase(), tag:this.types.get(String(r.id).toLowerCase())||'', pos:p.map(x=>Math.round(x*100)/100), rot:q.map(x=>Math.round(x*1e4)/1e4),
      model:pk && pk.model || undefined, m:pk && pk.m? pk.m.map(x=>Math.round(x*1e4)/1e4) : undefined,
      scale:m && m.scale!=null? m.scale : (r.scale||1), enabled:!S.disabled.has(key),
      cell:ext? {name:(cell && cell.name)||'', ext:true, gx:Math.floor(p[0]/8192), gy:Math.floor(p[1]/8192), region:(cell && cell.region)||''} : {name:cell.name||'', ext:false}};
  },
  t(key, r){ return WgScripts.inst.get(key) || {key, id:String(r.id).toLowerCase(), _ref:r, base:{pos:(r.pos||[0,0,0]).slice(), rot:(r.rot||[0,0,0]).slice()}}; },
  refOf(key){ for(const c of (App._scene && App._scene.cells)||[]) for(const r of c.refs||[]) if(r.key===key) return r; return null; },
  cellOfRef(key){ for(const c of (App._scene && App._scene.cells)||[]) if((c.refs||[]).some(r=>r.key===key)) return c; return null; },
  /** What changed since the last frame sent. */
  delta(dt){
    const S=WgScripts, out={dt};
    // What is drawn where, by key (the renderer's own matrices, in the scene's frame).
    const R=App.R;
    if(R && R.pickables!==this._picksOf){ this._picksOf=R.pickables; this._picks=new Map((R.pickables||[]).filter(o=>o.refKey).map(o=>[o.refKey, o])); this._all=true; }
    out.origin=(App._scene && App._scene.origin)||[0,0];
    const objects={}, seen=new Set();
    const one=(key, r, cell)=>{
      seen.add(key);
      const d=this.desc(key, r, cell), sig=JSON.stringify(d);
      if(this.sent.get(key)!==sig){ this.sent.set(key, sig); objects[key]=d; }
    };
    if(this._all){
      // The whole scene: every object of the cells loaded, the scripts' own too. One the
      // scripts disabled is out of the scene but still there.
      for(const c of (App._scene && App._scene.cells)||[]) for(const r of c.refs||[]) if(r && r.key && r.id && !r.gc) one(r.key, r, c);
      for(const k of S.disabled) if(this.sent.has(k)) seen.add(k);
      for(const p of S.placed) if(!seen.has(p.key)) one(p.key, p, p.cell? {kind:'int', name:p.cell} : null);
      for(const k of [...this.sent.keys()]) if(!seen.has(k)){ this.sent.delete(k); objects[k]=false; }
      this._all=false;
    }else{
      // Between scene builds only what the scripts move, place or switch can change.
      const keys=new Set([...S.moved.keys(), ...S.disabled, ...S.placed.map(p=>p.key), ...(this._wasOff||[])]);
      for(const k of keys){
        const p=S.placed.find(q=>q.key===k);
        if(p){ one(k, p, p.cell? {kind:'int', name:p.cell} : null); continue; }
        const it=S.inst.get(k), r=(it && it._ref) || this.refOf(k);
        if(r) one(k, r, this.cellOfRef(k));
        else if(this.sent.has(k)){ const was=JSON.parse(this.sent.get(k)); const en=!S.disabled.has(k); if(was.enabled!==en){ was.enabled=en; this.sent.set(k, JSON.stringify(was)); objects[k]=was; } }
      }
    }
    this._wasOff=new Set(S.disabled);
    if(Object.keys(objects).length) out.objects=objects;
    // The player (the camera), the clock and the weather.
    const pp=S.posOf({player:true}), az=R && R.cam? R.cam.az : 0, c=App._scene && App._scene.centre;
    const player={pos:pp.map(x=>Math.round(x)), yaw:Math.round(Math.atan2(-Math.cos(az), -Math.sin(az))*1e3)/1e3,
      cell:c && c.kind==='int'? {name:c.name||'', ext:false} : {name:(c && c.name)||'', ext:true, gx:Math.floor(pp[0]/8192), gy:Math.floor(pp[1]/8192)}};
    const ps=JSON.stringify(player); if(ps!==this._sentP){ this._sentP=ps; out.player=player; }
    const h=typeof $==='function' && $('#p_hour'), w=typeof $==='function' && $('#p_weather');
    out.hour=h? +h.value : 12; out.day=S.state.day; out.month=S.state.month; out.year=S.state.year;
    out.weather=w? Math.max(0, w.selectedIndex) : 0;
    out.musicPlaying=typeof WgSound==='object' && !!WgSound.place.music;
    // MWScript's globals, the actors and MWScript's locals: what changed.
    const globals={};
    for(const [k, g] of S.state.globals){ const sig=g.kind+':'+g.v; if(this._sentG.get(k)!==sig){ this._sentG.set(k, sig); globals[k]={name:g.name, kind:g.kind, v:g.v}; } }
    if(Object.keys(globals).length) out.globals=globals;
    const actors={};
    for(const [k, A] of S.actors){
      const a={hp:A.hp, mp:A.mp, fp:A.fp, max:A.max, dead:A.dead, items:Object.fromEntries(A.items),
        ai:A.ai? {kind:A.ai.kind, done:!!A.ai.done, target:A.ai.who && (A.ai.who.player? 'player' : A.ai.who.key) || null, dest:A.ai.goal||null} : null,
        combat:A.combat? (A.combat.player? 'player' : A.combat.key) : null};
      const sig=JSON.stringify(a); if(this._sentA.get(k)!==sig){ this._sentA.set(k, sig); actors[k]=a; }
    }
    if(Object.keys(actors).length) out.actors=actors;
    const mw={};
    for(const [k, it] of S.inst){
      if(!it.locals) continue;
      const vars={}; for(const [n, l] of it.locals) vars[n]=l.v;
      const m={script:it.script, vars}, sig=JSON.stringify(m);
      if(this._sentL.get(k)!==sig){ this._sentL.set(k, sig); mw[k]=m; }
    }
    if(Object.keys(mw).length) out.mwlocals=mw;
    const now=performance.now(), playing=[...S.loops.keys()];
    for(const [k, until] of S.playing) if(until>now) playing.push(k);
    out.sounds=playing;
    if(this._acts.length){ out.activations=this._acts; this._acts=[]; }
    // The scripts' windows: what was clicked or typed, and the screen they are drawn on.
    if(typeof WgLuaUI==='object'){ const ev=WgLuaUI.take(); if(ev.length) out.ui=ev; out.screen=WgLuaUI.screen(); }
    return out;
  },

  /** One frame: sent when the last has come back (the time between is added up). */
  frame(dt){
    if(!this.running) return;
    this._dt+=dt;
    if(this.busy) return;
    const step=this._dt; this._dt=0;
    let body;
    try{ body=JSON.stringify(this.delta(step)); }catch(e){ this.say('', 'error', 'the scene for Lua: '+String(e && e.message || e)); return; }
    this.busy=true;
    return Engine.call('lua_run_step',{frame:body}).then(a=>{ this.busy=false; if(this.running) this.apply(a); })
      .catch(e=>{ this.busy=false; if(!this.running) return; this.say('', 'error', 'OpenMW Lua stopped: '+String(e && e.message || e)); this.stop(); });
  },

  /* ---- what comes back ---------------------------------------------------------------- */

  apply(a){
    if(!a) return;
    for(const l of a.log||[]) this.say(l.path, l.kind, l.text);
    for(const e of a.effects||[]){
      const fx=this.FX[e.k];
      if(!fx){ this.say('', 'skip', 'an effect the page does not know: '+e.k); continue; }
      try{ fx.call(this, e); }catch(err){ this.say('', 'error', e.k+': '+String(err && err.message || err)); }
    }
    if(a.status) this._status=a.status;
  },
  /** A thing the scripts name by key, as the scene layer takes it. */
  thing(key){
    if(key==='player') return {player:true, key:'player', id:'player'};
    const S=WgScripts, it=S.inst.get(key); if(it) return it;
    const p=S.placed.find(q=>q.key===key); if(p) return {key, id:String(p.id).toLowerCase(), base:{pos:p.pos.slice(), rot:p.rot.slice()}};
    const r=this.refOf(key);
    return r? this.t(key, r) : null;
  },
  name(key){ const t=this.thing(key); return t? t.id : String(key); },
  FX:{
    enable(e){ const S=WgScripts; const did=e.on? S.disabled.delete(e.key) : (!S.disabled.has(e.key) && (S.disabled.add(e.key), true)); if(did){ S._dirtyScene=true; this.say('', 'do', (e.on? 'enabled ' : 'disabled ')+this.name(e.key)); } },
    place(e){
      const t=this.thing(e.key); if(!t || t.player) return;
      const ch={};
      if(e.pos) ch.pos=e.pos;
      if(e.rz!=null){ const r=WgScripts.rotOf(t); r[2]=e.rz; ch.rot=r; }
      if(e.scale!=null) ch.scale=e.scale;
      WgScripts.place(t, ch);
    },
    create(e){
      const S=WgScripts;
      S.placed.push({id:e.id, pos:e.pos, rot:[0,0,e.rz||0], scale:1, key:e.key, cell:e.cell? String(e.cell).toLowerCase() : '', from:-1, door:null, gc:false, mark:'', hist:null, moved:0});
      S._dirtyScene=true; this.say('', 'do', 'placed '+e.id+' at '+e.pos.map(Math.round).join(' '));
    },
    remove(e){
      const S=WgScripts, i=S.placed.findIndex(p=>p.key===e.key);
      if(i>=0) S.placed.splice(i, 1); else S.disabled.add(e.key);
      S._dirtyScene=true; this.say('', 'do', 'removed '+this.name(e.key));
    },
    teleportPlayer(e){ this.say('', 'do', 'teleport the player to '+(e.cell||'')+' '+(e.pos||[]).map(Math.round).join(' ')+' (the camera stays)'); },
    sound3d(e){
      const t=this.thing(e.key), S=WgScripts;
      if(e.loop){ S.loop(t, String(e.id), e.vol==null? 1 : e.vol, {run:{script:'lua'}}); return; }
      this.say('', 'do', 'playSound3d '+e.id+' at '+(t && t.id || 'the player')); S.sound(String(e.id), e.vol==null? 1 : e.vol, t && !t.player? t : null);
    },
    stop3d(e){ WgScripts.loopStop(e.key+'|'+String(e.id).toLowerCase()); },
    sound(e){ this.say('', 'do', 'playSound '+e.id); WgScripts.sound(String(e.id), e.vol==null? 1 : e.vol, null); },
    say(e){ this.say('', 'do', 'say '+e.file+(e.text? ': '+e.text : '')); WgScripts.voice(String(e.file), this.thing(e.key)); },
    music(e){ this.say('', 'do', 'music '+e.path); WgScripts.music(String(e.path)); },
    stopMusic(){ if(typeof WgSound==='object' && WgSound.musicStop) WgSound.musicStop(); },
    weather(e){
      const w=typeof $==='function' && $('#p_weather'); if(!w) return;
      const names=this.weathers().map(s=>String(s).toLowerCase()), i=names.indexOf(String(e.name).toLowerCase());
      if(i>=0 && w.selectedIndex!==i){ w.selectedIndex=i; w.dispatchEvent(new Event('change')); this.say('', 'do', 'weather '+names[i]); }
    },
    hour(e){ const el=typeof $==='function' && $('#p_hour'); if(el){ el.value=String(((e.v%24)+24)%24); el.dispatchEvent(new Event('input')); } },
    global(e){ const g=WgScripts.state.globals.get(e.name); if(g){ g.v=e.v; this._sentG.set(e.name, g.kind+':'+g.v); WgScripts.drawState(); } },
    mwlocal(e){ const it=WgScripts.inst.get(e.key), l=it && it.locals && it.locals.get(e.name); if(l) l.v=l.kind==='float'? +e.v||0 : Math.trunc(+e.v||0); },
    msg(e){ this.say('', 'do', 'message: '+e.text); if(typeof toast==='function') try{ toast(String(e.text), 'ok', 3000); }catch(_){ } },
    dyn(e){
      const A=WgScripts.actorOf(this.thing(e.key)); if(!A) return;
      const f=['hp','mp','fp'][e.which];
      if(e.base!=null) A.max[e.which]=e.base;
      if(e.cur!=null) A[f]=e.cur;
      if(e.which===0) WgScripts.hurt(A, 0, null, null);
      WgScripts.drawState();
    },
    inv(e){
      const A=WgScripts.actorOf(this.thing(e.key)); if(!A) return;
      const k=String(e.id).toLowerCase(), now=Math.max(0, (A.items.get(k)||0)+e.n);
      if(now) A.items.set(k, now); else A.items.delete(k);
      this.say('', 'do', (e.n>0? 'gave ' : 'took ')+Math.abs(e.n)+' '+k+(e.n>0? ' to ' : ' from ')+A.id); WgScripts.drawState();
    },
    ai(e){
      const S=WgScripts, t=this.thing(e.key); if(!t) return;
      const o=e.opt||{}, who=o.target? this.thing(o.target) : null, d=o.dest;
      const a=e.kind==='wander'? [o.distance||0, o.duration||0] :
        e.kind==='travel'? (d? d : []) :
        e.kind==='activate'? [who? who.id : ''] :
        [who? (who.player? 'player' : who.id) : 'player', o.duration||0].concat(d||[]);
      S.ai(t, e.kind, a, {run:{script:'lua'}});
      const A=S.actorOf(t); if(A && A.ai && who) A.ai.who=who;
      this.say('', 'do', 'AI '+e.kind+' for '+t.id);
    },
    aiClear(e){ const A=WgScripts.actorOf(this.thing(e.key)); if(A){ A.ai=null; A.combat=null; } },
    combat(e){ const S=WgScripts, A=S.actorOf(this.thing(e.key)), foe=e.target? this.thing(e.target) : null; if(A && foe){ A.combat=foe.player? {player:true} : foe; this.say('', 'do', A.id+' attacks '+foe.id); } },
    activate(e){ const it=WgScripts.inst.get(e.key); if(it){ it._activate=true; this.say(it.script, 'do', 'activated ('+it.id+')'); } },
    ui(e){ if(typeof WgLuaUI==='object') WgLuaUI.draw(e.id, e.layer, e.tree); },
    uiDestroy(e){ if(typeof WgLuaUI==='object') WgLuaUI.remove(e.id); },
  },
  /** For the panel: the last frame's count of running scripts. */
  status(){ return this._status; },
};
