/* =====================================================================================
   Wraithguard: MWScript running in the scene (the Preview's "Scripts").

   The scene's objects' scripts (and the start scripts, and those they start) run as the
   game runs them: each object's own copy every frame, with its own locals, against a game
   state the panel sets - the hour (the Preview's clock), the day, the weather, the globals,
   journal indexes, the player (the camera). What they do shows in the scene:
   - Enable / Disable: objects taken out of the scene and put back (static swaps, lamps by
     the hour), and GetDisabled answers so;
   - StreamMusic, PlaySound(3D)(VP), PlayLoopSound3D(VP), StopSound, Say: through the
     Preview's sound (Sounds on);
   - SetPos, SetAngle, Move, Rotate, MoveWorld, RotateWorld, Position(Cell), PlaceAtMe,
     PlaceAtPC, SetScale, ModScale: placed objects moved, turned, scaled and added;
   - ChangeWeather, ForceWeather: the Preview's weather; FadeIn, FadeOut, FadeTo: the view;
     GameHour set: the Preview's clock;
   - actors: what each carries (the load order's NPCO; AddItem, RemoveItem, Drop,
     GetItemCount), its health, magicka and fatigue (Set/Mod/ModCurrent*, GetHealth, OnDeath,
     Resurrect), its AI package (AiWander, AiTravel, AiFollow, AiEscort, AiActivate walk it
     through the scene; GetCurrentAIPackage, GetAIPackageDone) and its fights (StartCombat,
     StopCombat: it closes in and strikes until one falls; GetAttacked, GetTarget);
   - GetSoundPlaying and SayDone from what is playing; GetLOS and GetDetected from the
     scene's own geometry; a MessageBox's buttons put to the person, the choice the script's
     GetButtonPressed; AddTopic noted; ForceGreeting shows who greets (the dialogue itself
     is not chosen or shown).
   Anything else a script calls is logged (once per script and function), not guessed at.

   The language as the Construction Set reads it: `begin`/`end`, `short`/`long`/`float`
   locals, `set x to expr`, `if`/`elseif`/`else`/`endif`, `while`/`endwhile`, `return`,
   `ref->Function args`, `ref.variable`, functions in expressions taking their arguments
   without parentheses (each function's count in `FN`). Written for this page from the
   language's documentation (the MWEdit port in editcore::mwscript checks and compiles it;
   this runs it). Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
const WgScripts={
  on:false, paused:false,
  state:{day:16, month:7, year:427, globals:new Map(), journal:new Map()},
  inst:new Map(),        // ref key -> {key, id, script, locals, cell}
  globalRun:new Map(),   // lowercased script id -> {script, locals}: the start and started scripts
  parsed:new Map(),      // lowercased script id -> parsed script (or {error})
  objScript:new Map(),   // lowercased object id -> lowercased script id
  // What the scripts did to the scene: by ref key.
  disabled:new Set(), moved:new Map(), placed:[], _placeN:0,
  // The animation group a script plays on an object (PlayGroup, LoopGroup, openmw.animation),
  // by ref key: {group, loops, speed, at (the renderer's clock when it began), t0, ends}.
  anims:new Map(), _groups:new Map(),
  // MWScripts turned off in Which scripts run (lower-case ids), kept across sessions.
  mwOff:(()=>{ try{ return new Set(JSON.parse(localStorage.getItem('wgMwOff')||'[]')); }catch(_){ return new Set(); } })(),
  _whichQ:'',
  log:[], _logged:new Set(), _dirtyScene:false, _lastRebuild:0, _raf:0, _last:0, _sceneSeen:null,
  loops:new Map(),       // key|sound -> the looping voice
  playing:new Map(),     // key|sound -> when a one-shot ends (GetSoundPlaying, SayDone)
  // The actors and containers the scripts touch, by ref key ('player' for the player):
  // what each carries, its health, magicka and fatigue, its AI package and its fight.
  actors:new Map(), actorDefs:new Map(),
  topics:new Set(),
  msg:null,              // a MessageBox with buttons, waiting: {run, text, buttons}

  /* ---- the language: tokens ----------------------------------------------------------- */

  tokens(line){
    const out=[]; let i=0;
    while(i<line.length){
      const c=line[i];
      if(c===';') break;
      if(/\s|,/.test(c)){ i++; continue; }
      if(c==='"'){ const j=line.indexOf('"', i+1); const end=j<0? line.length : j; out.push({t:'str', v:line.slice(i+1, end)}); i=end+1; continue; }
      const two=line.substr(i,2);
      if(['==','!=','<=','>=','->'].includes(two)){ out.push({t:'op', v:two}); i+=2; continue; }
      if('<>+-*/()'.includes(c)){ out.push({t:'op', v:c}); i++; continue; }
      if(c==='.' && /[A-Za-z_]/.test(line[i+1]||'') && out.length && out[out.length-1].t==='id'){ out.push({t:'op', v:'.'}); i++; continue; }
      const m=/^(?:\d+\.?\d*|\.\d+)/.exec(line.slice(i));
      if(m && !/[A-Za-z_]/.test(line[i+m[0].length]||'')){ out.push({t:'num', v:parseFloat(m[0])}); i+=m[0].length; continue; }
      const w=/^[A-Za-z0-9_'.]+/.exec(line.slice(i));
      if(w){
        // An id may hold a dot (`ref.variable` splits at it).
        let s=w[0].replace(/\.+$/,'');
        const dot=s.indexOf('.'); if(dot>0 && !/^\d/.test(s)) s=s.slice(0, dot);
        out.push({t:'id', v:s}); i+=s.length; continue;
      }
      i++;
    }
    return out;
  },

  /* ---- the language: the functions it knows, by how many arguments they take ----------- */

  // In expressions (they answer a value).
  EXPR:{getsecondspassed:0, menumode:0, onactivate:0, cellchanged:0, getdisabled:0, getdistance:1, getjournalindex:1,
    random:1, getpos:1, getangle:1, getpccell:1, getcurrentweather:0, getlocked:0, getitemcount:1, getbuttonpressed:0,
    getstandingpc:0, getstandingactor:0, scriptrunning:1, getsoundplaying:1, getdeadcount:1, gethealth:0, getpcrunning:0,
    getpcsneaking:0, getinterior:0, getwaterlevel:0, getscale:0, getstartingpos:1, getstartingangle:1, getpcinjail:0,
    getcurrentaipackage:0, getplayercontrolsdisabled:0, geteffect:1, getspell:1, getspelleffects:1, getrace:1,
    ondeath:0, onknockout:0, onmurder:0, onpcadd:0, onpcdrop:0, onpchitme:0, onpcsoulgemuse:0, onrepair:0, getlos:1,
    getdetected:1, gettarget:1, getaipackagedone:0, getsquareroot:1, getcurrenttime:0, getlevel:0,
    getcommondisease:0, getblightdisease:0, getwerewolfkills:0, ispcwerewolf:0, getpcfacrep:1, getfight:0, getflee:0,
    gethello:0, getalarm:0, getweapondrawn:0, getspellreadied:0, getforcesneak:0, getforcerun:0, hitonme:1,
    getresistmagicka:0, getattacked:0, getcollidingpc:0, getcollidingactor:0, getpcsleep:0, getpctraveling:0,
    getmagicka:0, getfatigue:0, getstrength:0, getspeed:0, getpcjumping:0, saydone:0},
  // As statements, the arguments each takes (the rest of the line is its own).
  STMT:{enable:0, disable:0, streammusic:1, playsound:1, playsound3d:1, playsoundvp:3, playsound3dvp:3, playloopsound3d:1,
    playloopsound3dvp:3, stopsound:1, say:2, setpos:2, setangle:2, move:2, rotate:2, moveworld:2, rotateworld:2, position:4,
    positioncell:5, placeatme:4, placeatpc:4, setscale:1, modscale:1, changeweather:2, forceweather:1, fadein:1, fadeout:1,
    fadeto:2, startscript:1, stopscript:1, setjournalindex:2, journal:2, messagebox:-1, lock:-1, unlock:0, activate:0,
    setdelete:1, additem:2, removeitem:2, playgroup:-1, loopgroup:-1, enableplayercontrols:0, disableplayercontrols:0,
    togglesky:0, setatstart:0, setfight:1, setflee:1, sethello:1, setalarm:1, aiwander:-1, aitravel:-1, aiactivate:-1,
    aifollow:-1, aiescort:-1, forcegreeting:0, showmap:1, centeroncell:1, coc:1, cow:2, wakeuppc:0, playbink:-1,
    modpcfacrep:2, setpccrimelevel:1, modpccrimelevel:1, cast:2, explodespell:1, addspell:1, removespell:1, addtopic:1,
    payfine:0, gotojail:0, resurrect:0, sethealth:1, modhealth:1, setmagicka:1, setfatigue:1, modcurrenthealth:1,
    skipanim:0, drop:2, equip:1, startcombat:1, stopcombat:0, modmagicka:1, modfatigue:1, modcurrentmagicka:1, modcurrentfatigue:1, choice:-1},

  // The functions whose argument is a name (an id, an axis), not a value.
  WORDS:{getdistance:1, getpccell:1, getjournalindex:1, getitemcount:1, scriptrunning:1, getsoundplaying:1, getdeadcount:1,
    geteffect:1, getspell:1, getspelleffects:1, getrace:1, getlos:1, getdetected:1, gettarget:1, getpcfacrep:1, hitonme:1,
    getpos:1, getangle:1, getstartingpos:1, getstartingangle:1},
  // AI packages as GetCurrentAIPackage numbers them.
  AI:{wander:0, travel:1, escort:2, follow:3, activate:4},

  /* ---- the language: parsing ----------------------------------------------------------- */

  /** A script's source, parsed: {name, locals: [{name, kind}], body: [statements]}. */
  parse(text){
    const lines=String(text||'').replace(/\r/g,'').split('\n');
    const locals=[], top=[];
    let name='', i=0;
    const stack=[{k:'root', body:top}];
    const cur=()=>stack[stack.length-1];
    const body=()=>{ const c=cur(); return c.k==='if'? c.branches[c.branches.length-1].body : c.k==='else'? c.body : c.body; };
    for(; i<lines.length; i++){
      const toks=this.tokens(lines[i]);
      if(!toks.length) continue;
      const w=toks[0].t==='id'? toks[0].v.toLowerCase() : '';
      const ln=i+1;
      if(w==='begin'){ name=(toks[1]&&toks[1].v)||''; continue; }
      if(w==='end'){ break; }
      if(w==='short' || w==='long' || w==='float'){ for(const t of toks.slice(1)) if(t.t==='id') locals.push({name:t.v.toLowerCase(), kind:w}); continue; }
      if(w==='if'){ const s={k:'if', line:ln, branches:[{cond:this.expr(toks.slice(1), ln), body:[]}], els:null}; body().push(s); stack.push(s); continue; }
      if(w==='elseif'){ const s=cur(); if(s.k==='if'){ s.branches.push({cond:this.expr(toks.slice(1), ln), body:[]}); } continue; }
      if(w==='else'){ const s=cur(); if(s.k==='if'){ s.els=[]; s.branches.push({cond:null, body:s.els}); } continue; }
      if(w==='endif'){ if(cur().k==='if') stack.pop(); continue; }
      if(w==='while'){ const s={k:'while', line:ln, cond:this.expr(toks.slice(1), ln), body:[]}; body().push(s); stack.push(s); continue; }
      if(w==='endwhile'){ if(cur().k==='while') stack.pop(); continue; }
      if(w==='return'){ body().push({k:'return', line:ln}); continue; }
      if(w==='set'){
        const to=toks.findIndex(t=>t.t==='id' && t.v.toLowerCase()==='to');
        if(to<2) continue;
        const tgt=toks.slice(1, to);
        let target;
        if(tgt.length===3 && tgt[1].v==='.') target={ref:tgt[0].v.toLowerCase(), name:tgt[2].v.toLowerCase()};
        else if(tgt.length===3 && tgt[1].v==='->') target={ref:tgt[0].v.toLowerCase(), name:tgt[2].v.toLowerCase()};
        else target={name:tgt[0].v.toLowerCase()};
        body().push({k:'set', line:ln, target, expr:this.expr(toks.slice(to+1), ln)});
        continue;
      }
      // A call: `ref->Fn args` or `Fn args`.
      let ref=null, at=0;
      if(toks.length>2 && toks[1].v==='->'){ ref=toks[0].v.toLowerCase(); at=2; }
      const fn=toks[at] && toks[at].t==='id'? toks[at].v.toLowerCase() : '';
      if(!fn) continue;
      const args=this.args(toks.slice(at+1), ln);
      body().push({k:'call', line:ln, ref, fn, args, raw:lines[i].trim()});
    }
    return {name, locals, body:top};
  },
  /** A statement's arguments: primaries one after another (a parenthesised one an
   *  expression), the line's own commas and spaces between. */
  args(toks, ln){
    const out=[]; const p={toks, i:0, ln};
    while(p.i<toks.length){
      const t=toks[p.i];
      const e=t.t==='id' || t.t==='str'? this.unary(p) : this.term(p);
      if(!e) break; out.push(e);
    }
    return out;
  },
  expr(toks, ln){ const p={toks, i:0, ln}; return this.compare(p)||{k:'num', v:0}; },
  compare(p){
    let a=this.additive(p);
    while(p.i<p.toks.length && ['==','!=','<','>','<=','>='].includes(p.toks[p.i].v)){
      const op=p.toks[p.i++].v; const b=this.additive(p); a={k:'bin', op, a, b};
    }
    return a;
  },
  additive(p){
    let a=this.term(p);
    while(p.i<p.toks.length && (p.toks[p.i].v==='+' || p.toks[p.i].v==='-') && p.toks[p.i].t==='op'){
      const op=p.toks[p.i++].v; const b=this.term(p); a={k:'bin', op, a, b};
    }
    return a;
  },
  term(p){
    let a=this.unary(p);
    while(p.i<p.toks.length && (p.toks[p.i].v==='*' || p.toks[p.i].v==='/')){
      const op=p.toks[p.i++].v; const b=this.unary(p); a={k:'bin', op, a, b};
    }
    return a;
  },
  unary(p){
    const t=p.toks[p.i]; if(!t) return null;
    if(t.v==='-' && t.t==='op'){ p.i++; const e=this.unary(p); return {k:'neg', e}; }
    if(t.v==='('){ p.i++; const e=this.compare(p); if(p.toks[p.i] && p.toks[p.i].v===')') p.i++; return e; }
    if(t.t==='num'){ p.i++; return {k:'num', v:t.v}; }
    if(t.t==='str'){ p.i++; return {k:'str', v:t.v}; }
    if(t.t!=='id') { p.i++; return null; }
    p.i++;
    let ref=null, name=t.v.toLowerCase();
    if(p.toks[p.i] && p.toks[p.i].v==='->' && p.toks[p.i+1]){ ref=name; name=p.toks[p.i+1].v.toLowerCase(); p.i+=2; }
    else if(p.toks[p.i] && p.toks[p.i].v==='.' && p.toks[p.i+1]){ const v=p.toks[p.i+1].v.toLowerCase(); p.i+=2; return {k:'remote', ref:name, name:v}; }
    const n=this.EXPR[name];
    if(n!=null){
      const args=[];
      for(let j=0;j<n && p.i<p.toks.length;j++){
        const a=p.toks[p.i];
        // An argument is a word or a value, never an operator that ends the call.
        if(a.t==='op' && a.v!=='(' && a.v!=='-') break;
        if(a.t==='id' && this.WORDS[name]){ p.i++; args.push({k:'word', v:a.v}); }
        else args.push(this.unary(p));
      }
      return {k:'fn', ref, name, args};
    }
    return {k:'var', ref, name};
  },

  /* ---- running ------------------------------------------------------------------------- */

  script(id){
    const k=String(id||'').toLowerCase();
    if(!this.parsed.has(k)) return null;
    return this.parsed.get(k);
  },
  /** One frame of every running script. */
  frame(dt){
    this._dt=dt;
    for(const g of this.globalRun.values()) this.runOne(g, null);
    // A disabled object's script runs on (it is how a lamp turns itself back on).
    for(const it of this.inst.values()) this.runOne(it, it);
    // OpenMW Lua (74_wg_lua.js), against the same scene.
    if(typeof WgLua==='object') WgLua.frame(dt);
    this.actorsFrame(dt);
    this.animsFrame();
    this.loopsFrame();
    if(this._dirtyAI && performance.now()-this._lastRebuild>1500){ this._dirtyAI=false; this._dirtyScene=true; }
    /* A change to what they play is drawn once it has lasted 300 ms: a group played once and
       played again by its script the next frame (the mist's) is gone for a frame between. */
    if(this._dirtyAnim || this._animSince){
      this._dirtyAnim=false;
      const t=performance.now();
      if(this.animSig()===this._animSigDrawn) this._animSince=0;
      else if(!this._animSince) this._animSince=t;
      else if(t-this._animSince>=300){ this._animSince=0; this._dirtyScene=true; }
    }
    if(this._dirtyScene) this.flushScene();
    this.keepLive();
  },
  /** Once the scene has been drawn again, every live actor put straight where the scripts
   *  have it: a drawing takes where each stood when it began, and they walk on meanwhile -
   *  left alone, each snapped back until its next step (Static: "the movement rubberbands"). */
  keepLive(){
    const L=App._liveRefs;
    if(!L || L===this._liveSeen) return;
    this._liveSeen=L;
    if(this._warmWant) for(const k of [...this._warmWant]) if(L.has(k)){ this._warmWant.delete(k); this.warm(k); }
    if(typeof previewMoveRef!=='function') return;
    for(const [k, m] of this.moved){
      if(!m.pos || !L.has(k)) continue;
      const r=this.refByKey(k);
      previewMoveRef(k, m.pos, m.rot || (r && r.rot) || [0,0,0], m.scale!=null? m.scale : ((r && r.scale) || 1));
    }
  },
  runOne(run, self){
    if(this.mwOff.has(String(run.script||'').toLowerCase())) return;
    const sc=this.script(run.script);
    if(!sc || sc.error) return;
    if(!run.locals){ run.locals=new Map(); for(const l of sc.locals) run.locals.set(l.name, {kind:l.kind, v:0}); }
    const ctx={run, self, sc, steps:0, act:run._activate? (run._activate=false, true) : false};
    try{ this.block(sc.body, ctx); }
    catch(e){ if(e!==this.RETURN) this.say(run.script, 'error', String(e && e.message || e)); }
  },
  RETURN:{},
  block(list, ctx){
    for(const s of list){
      if(++ctx.steps>20000) throw new Error('more than 20000 steps in one frame (a loop that never ends?)');
      switch(s.k){
        case 'set': this.assign(s.target, this.val(s.expr, ctx), ctx); break;
        case 'if': for(const b of s.branches){ if(b.cond==null || this.val(b.cond, ctx)){ this.block(b.body, ctx); break; } } break;
        case 'while': { let n=0; while(this.val(s.cond, ctx)){ if(++n>10000) throw new Error('a while loop ran 10000 times in one frame (line '+s.line+')'); this.block(s.body, ctx); } break; }
        case 'return': throw this.RETURN;
        case 'call': this.call(s, ctx); break;
      }
    }
  },
  num(v){ return typeof v==='number'? v : (+v||0); },
  val(e, ctx){
    if(!e) return 0;
    switch(e.k){
      case 'num': return e.v;
      case 'str': return e.v;
      case 'word': return e.v;
      case 'neg': return -this.num(this.val(e.e, ctx));
      case 'bin': {
        const a=this.val(e.a, ctx), b=this.val(e.b, ctx);
        if(typeof a==='string' || typeof b==='string'){ const x=String(a).toLowerCase(), y=String(b).toLowerCase(); return e.op==='=='? +(x===y) : e.op==='!='? +(x!==y) : 0; }
        switch(e.op){
          case '+': return a+b; case '-': return a-b; case '*': return a*b; case '/': return b? a/b : 0;
          case '==': return +(a===b); case '!=': return +(a!==b); case '<': return +(a<b); case '>': return +(a>b);
          case '<=': return +(a<=b); case '>=': return +(a>=b);
        }
        return 0;
      }
      case 'var': return this.read(e.ref, e.name, ctx);
      case 'remote': return this.readRemote(e.ref, e.name, ctx);
      case 'fn': return this.fn(e, ctx);
    }
    return 0;
  },
  read(ref, name, ctx){
    if(ref) return this.readRemote(ref, name, ctx);
    const l=ctx.run.locals.get(name); if(l) return l.v;
    const g=this.global(name); if(g) return g.v;
    // A bare word that is neither: a function with no arguments, or an id.
    if(name in this.EXPR) return this.fn({name, args:[]}, ctx);
    return name;
  },
  readRemote(ref, name, ctx){
    const it=this.firstOf(ref);
    const run=it || this.globalRun.get(ref);
    if(run && run.locals && run.locals.has(name)) return run.locals.get(name).v;
    return 0;
  },
  global(name){
    const k=String(name).toLowerCase();
    if(k==='gamehour'){ const h=$('#p_hour'); return {v:h? +h.value : 12, kind:'float'}; }
    if(k==='day') return {v:this.state.day, kind:'short'};
    if(k==='month') return {v:this.state.month, kind:'short'};
    if(k==='year') return {v:this.state.year, kind:'short'};
    return this.state.globals.get(k)||null;
  },
  assign(t, v, ctx){
    v=this.num(v);
    const fit=(kind, x)=> kind==='float'? x : Math.trunc(x);
    if(t.ref){
      const it=this.firstOf(t.ref) || this.globalRun.get(t.ref);
      if(it && it.locals && it.locals.has(t.name)){ const l=it.locals.get(t.name); l.v=fit(l.kind, v); }
      return;
    }
    const l=ctx.run.locals.get(t.name);
    if(l){ l.v=fit(l.kind, v); return; }
    const k=t.name;
    if(k==='gamehour'){ const h=$('#p_hour'); if(h){ h.value=String(((v%24)+24)%24); h.dispatchEvent(new Event('input')); } return; }
    if(k==='day'){ this.state.day=Math.trunc(v); this.drawState(); return; }
    if(k==='month'){ this.state.month=Math.trunc(v); this.drawState(); return; }
    if(k==='year'){ this.state.year=Math.trunc(v); this.drawState(); return; }
    const g=this.state.globals.get(k);
    if(g){ g.v=g.kind==='f'? v : Math.trunc(v); this.drawState(); return; }
    this.say(ctx.run.script, 'warn', 'sets '+t.name+', which is neither a local nor a global');
  },
  /** The scene object a script's `ref->` names: its own reference, the player, or the
   *  first loaded reference of that id (as the game takes it). */
  firstOf(ref){
    if(!ref) return null;
    for(const it of this.inst.values()) if(it.id===ref) return it;
    return null;
  },
  target(ref, ctx){
    if(!ref) return ctx.self;
    if(ref==='player') return {player:true};
    const it=this.firstOf(ref); if(it) return it;
    // An object of the scene without a script.
    const r=this.refById(ref);
    return r? {key:r.key, id:ref, ref:r} : null;
  },
  /** A reference of the scene by its key (an index kept per scene: the AI asks every frame). */
  refByKey(key){
    const sc=App._scene;
    if(this._keyIx!==sc){ this._keyIx=sc; this._byKey=new Map(); for(const c of (sc && sc.cells)||[]) for(const r of c.refs||[]) if(r && r.key) this._byKey.set(r.key, r); }
    return this._byKey.get(key) || this.placed.find(p=>p.key===key) || null;
  },
  refById(id){
    const sc=App._scene;
    for(const c of (sc && sc.cells)||[]) for(const r of c.refs||[]) if(String(r.id).toLowerCase()===id) return r;
    return null;
  },
  refOf(t){
    if(!t || t.player) return null;
    if(t.ref) return t.ref;
    const sc=App._scene;
    for(const c of (sc && sc.cells)||[]) for(const r of c.refs||[]) if(r.key===t.key) return r;
    return t._ref||null;
  },
  /** Where something stands now: its position as the scripts have left it. */
  posOf(t){
    if(!t) return [0,0,0];
    if(t.player){ const R=App.R, o=(App._scene && App._scene.origin)||[0,0]; if(!R || !R.cam) return [0,0,0]; const c=R.cam, ce=Math.cos(c.el);
      return [c.tx+c.dist*ce*Math.cos(c.az)+o[0], c.ty+c.dist*ce*Math.sin(c.az)+o[1], c.tz+c.dist*Math.sin(c.el)-116]; }
    const m=this.moved.get(t.key); if(m && m.pos) return m.pos.slice();
    const r=this.refOf(t); return r && r.pos? r.pos.slice() : (t.base && t.base.pos? t.base.pos.slice() : [0,0,0]);
  },
  rotOf(t){
    const m=t && this.moved.get(t.key); if(m && m.rot) return m.rot.slice();
    const r=this.refOf(t); return r && r.rot? r.rot.slice() : (t && t.base && t.base.rot? t.base.rot.slice() : [0,0,0]);
  },
  axis(a){ return {x:0, y:1, z:2}[String(a).toLowerCase()]; },

  fn(e, ctx){
    const a=e.args.map(x=>this.val(x, ctx)), name=e.name;
    const t=this.target(e.ref, ctx);
    switch(name){
      case 'getsecondspassed': return this._dt||0;
      case 'menumode': return 0;
      case 'onactivate': return ctx.act? 1 : 0;
      case 'cellchanged': { const was=ctx.run._cellSeen, now=this._sceneKey||''; ctx.run._cellSeen=now; return was!==undefined && was!==now? 1 : (was===undefined? 1 : 0); }
      case 'getdisabled': return t && this.disabled.has(t.key)? 1 : 0;
      case 'getdistance': {
        const other=this.target(String(a[0]).toLowerCase(), ctx), p=this.posOf(t), q=this.posOf(other);
        return Math.hypot(p[0]-q[0], p[1]-q[1], p[2]-q[2]);
      }
      case 'getjournalindex': return this.state.journal.get(String(a[0]).toLowerCase())||0;
      case 'random': return Math.floor(Math.random()*Math.max(1, this.num(a[0])));
      case 'getpos': { const i=this.axis(a[0]); return i==null? 0 : this.posOf(t)[i]; }
      case 'getangle': { const i=this.axis(a[0]); return i==null? 0 : this.rotOf(t)[i]*180/Math.PI; }
      case 'getstartingpos': { const i=this.axis(a[0]); const r=this.refOf(t); return i==null || !r? 0 : (t.base? t.base.pos[i] : r.pos[i]); }
      case 'getstartingangle': { const i=this.axis(a[0]); const r=this.refOf(t); return i==null || !r? 0 : (t.base? t.base.rot[i] : r.rot[i])*180/Math.PI; }
      case 'getpccell': { const want=String(a[0]).toLowerCase(), sc=App._scene; return (sc && sc.cells||[]).some(c=>String(c.name||'').toLowerCase().startsWith(want))? 1 : 0; }
      case 'getcurrentweather': { const w=$('#p_weather'); return w? Math.max(0, w.selectedIndex) : 0; }
      case 'getinterior': { const sc=App._scene; return sc && sc.centre && sc.centre.kind==='int'? 1 : 0; }
      case 'getwaterlevel': { const R=App.R; return R && R.water? R.water.z : 0; }
      case 'getscale': { const m=t && this.moved.get(t.key); const r=this.refOf(t); return m && m.scale!=null? m.scale : (r && r.scale) || 1; }
      case 'scriptrunning': return this.globalRun.has(String(a[0]).toLowerCase())? 1 : 0;
      case 'getsoundplaying': return t && this.soundOn(t, String(a[0]))? 1 : 0;
      case 'saydone': return t && this.soundOn(t, '*say')? 0 : 1;
      case 'getlos': return this.los(t, this.target(String(a[0]).toLowerCase(), ctx))? 1 : 0;
      case 'getdetected': { const o=this.target(String(a[0]).toLowerCase(), ctx); return this.los(o, t) && this.dist(o, t)<2000? 1 : 0; }
      case 'getitemcount': { const A=this.actorOf(t); return A? A.items.get(String(a[0]).toLowerCase())||0 : 0; }
      case 'gethealth': { const A=this.actorOf(t); return A? A.hp : 0; }
      // Spells: those the actor has (AddSpell), and those cast on it still running (ten
      // seconds; their effects themselves are not worked out).
      case 'getspell': { const A=this.actorOf(t); return A && A.spells && A.spells.has(String(a[0]).toLowerCase())? 1 : 0; }
      case 'getspelleffects': { const A=this.actorOf(t), k=String(a[0]).toLowerCase(); return A && ((A.spells && A.spells.has(k)) || ((A.active && A.active.get(k))||0)>performance.now())? 1 : 0; }
      case 'getmagicka': { const A=this.actorOf(t); return A? A.mp : 0; }
      case 'getfatigue': { const A=this.actorOf(t); return A? A.fp : 0; }
      case 'ondeath': case 'onmurder': { const A=this.actorOf(t); if(A && A.dead && !A[name+'Seen']){ A[name+'Seen']=true; return 1; } return 0; }
      case 'getdeadcount': { const id=String(a[0]).toLowerCase(); let n=0; for(const A of this.actors.values()) if(A.id===id && A.dead) n++; return n; }
      case 'getattacked': { const A=this.actorOf(t); return A && A.attacked? 1 : 0; }
      case 'gettarget': { const A=this.actorOf(t), o=this.target(String(a[0]).toLowerCase(), ctx); return A && A.combat && o && (A.combat.player? o.player : A.combat.key===o.key)? 1 : 0; }
      case 'getcurrentaipackage': { const A=this.actorOf(t); return A && A.ai && !A.ai.done? this.AI[A.ai.kind] : -1; }
      case 'getaipackagedone': { const A=this.actorOf(t); return A && A.ai && A.ai.done? 1 : 0; }
      case 'getsquareroot': return Math.sqrt(Math.max(0, this.num(a[0])));
      case 'getcurrenttime': { const h=$('#p_hour'); return h? +h.value : 12; }
      case 'getbuttonpressed': {
        // The button chosen in this script's MessageBox, once; -1 while none is.
        const b=ctx.run._button; if(b==null) return -1;
        ctx.run._button=null; return b;
      }
    }
    this.unknown(ctx.run.script, name);
    return 0;
  },

  call(s, ctx){
    const a=s.args.map(x=>this.val(x, ctx)), fn=s.fn;
    const t=this.target(s.ref, ctx);
    const did=(what)=>this.say(ctx.run.script, 'do', what+(t && t.id && t!==ctx.self? ' ('+t.id+')' : ''));
    switch(fn){
      case 'enable': if(t && t.key && this.disabled.delete(t.key)){ this._dirtyScene=true; did('Enable'); } return;
      case 'disable': if(t && t.key && !this.disabled.has(t.key)){ this.disabled.add(t.key); this._dirtyScene=true; did('Disable'); } return;
      case 'setdelete': if(t && t.key && this.num(a[0])){ this.disabled.add(t.key); this._dirtyScene=true; did('SetDelete'); } return;
      case 'streammusic': did('StreamMusic '+a[0]); this.music(String(a[0])); return;
      case 'playsound': case 'playsoundvp': did('PlaySound '+a[0]); this.sound(String(a[0]), fn==='playsoundvp'? this.num(a[1]) : 1, null); return;
      case 'playsound3d': case 'playsound3dvp': did(s.raw); this.sound(String(a[0]), fn==='playsound3dvp'? this.num(a[1]) : 1, t); return;
      case 'playloopsound3d': case 'playloopsound3dvp': this.loop(t, String(a[0]), fn==='playloopsound3dvp'? this.num(a[1]) : 1, ctx); return;
      case 'stopsound': if(t) this.loopStop(t.key+'|'+String(a[0]).toLowerCase()); return;
      case 'say': did('Say '+a[0]+(a[1]? ': '+a[1] : '')); this.voice(String(a[0]), t); return;
      case 'setpos': { const i=this.axis(a[0]); if(i==null || !t || t.player) return; const p=this.posOf(t); p[i]=this.num(a[1]); this.place(t, {pos:p}); return; }
      case 'setangle': { const i=this.axis(a[0]); if(i==null || !t || t.player) return; const r=this.rotOf(t); r[i]=this.num(a[1])*Math.PI/180; this.place(t, {rot:r}); return; }
      case 'move': case 'moveworld': {
        const i=this.axis(a[0]); if(i==null || !t || t.player) return;
        const d=this.num(a[1])*(this._dt||0), p=this.posOf(t);
        if(fn==='moveworld') p[i]+=d;
        else{ const v=[0,0,0]; v[i]=d; const w=this.local(this.rotOf(t), v); p[0]+=w[0]; p[1]+=w[1]; p[2]+=w[2]; }
        this.place(t, {pos:p}); return;
      }
      case 'rotate': case 'rotateworld': { const i=this.axis(a[0]); if(i==null || !t || t.player) return; const r=this.rotOf(t); r[i]+=this.num(a[1])*(this._dt||0)*Math.PI/180; this.place(t, {rot:r}); return; }
      case 'position': case 'positioncell': {
        if(!t || t.player) return;
        this.place(t, {pos:[this.num(a[0]), this.num(a[1]), this.num(a[2])], rot:[0, 0, this.num(a[3])*Math.PI/180]});
        did(fn==='positioncell'? 'PositionCell '+a.slice(0,4).join(' ')+' '+(a[4]||'') : 'Position '+a.slice(0,4).join(' ')); return;
      }
      case 'setscale': if(t && !t.player){ this.place(t, {scale:this.num(a[0])}); did('SetScale '+a[0]); } return;
      case 'modscale': if(t && !t.player){ const m=this.moved.get(t.key); const r=this.refOf(t); const sc=(m && m.scale!=null? m.scale : (r && r.scale) || 1)+this.num(a[0]); this.place(t, {scale:sc}); } return;
      case 'placeatme': case 'placeatpc': {
        const at=fn==='placeatpc'? {player:true} : t; if(!at) return;
        const id=String(a[0]), n=Math.max(1, Math.min(20, this.num(a[1])|0)), dist=this.num(a[2]), dir=this.num(a[3])|0;
        const p=this.posOf(at), r=this.rotOf(at.player? null : at), cell=this.cellOf(at);
        const off=[[0,1],[0,-1],[-1,0],[1,0]][dir]||[0,1];
        for(let k=0;k<n;k++){
          const w=this.local(r, [off[0]*dist, off[1]*dist, 0]);
          this.placed.push({id, pos:[p[0]+w[0], p[1]+w[1], p[2]], rot:[0,0,r[2]], scale:1, key:'scr:'+(++this._placeN), cell, from:-1, door:null, gc:false, mark:'', hist:null, moved:0});
        }
        this._dirtyScene=true; did(fn==='placeatpc'? 'PlaceAtPC '+id : 'PlaceAtMe '+id); return;
      }
      case 'changeweather': case 'forceweather': {
        const n=this.num(a[fn==='changeweather'? 1 : 0])|0, w=$('#p_weather');
        if(w && w.options[n] && w.selectedIndex!==n){ w.selectedIndex=n; w.dispatchEvent(new Event('change')); did(fn==='changeweather'? 'ChangeWeather '+a[0]+' '+n : 'ForceWeather '+n); }
        return;
      }
      case 'fadein': this.fade(0, this.num(a[0])); did('FadeIn '+a[0]); return;
      case 'fadeout': this.fade(1, this.num(a[0])); did('FadeOut '+a[0]); return;
      case 'fadeto': this.fade(this.num(a[0])/100, this.num(a[1])); did('FadeTo '+a[0]); return;
      case 'startscript': this.start(String(a[0])); did('StartScript '+a[0]); return;
      case 'stopscript': { const k=String(a[0]).toLowerCase(); if(this.globalRun.delete(k)) did('StopScript '+a[0]); return; }
      case 'setjournalindex': case 'journal': {
        const k=String(a[0]).toLowerCase(), v=this.num(a[1])|0;
        if(fn==='setjournalindex' || v>(this.state.journal.get(k)||0)) this.state.journal.set(k, v);
        did((fn==='journal'? 'Journal ' : 'SetJournalIndex ')+a[0]+' '+v); this.drawState(); return;
      }
      case 'messagebox': {
        // The text's %s, %g, %.2f filled from the arguments after it; the rest are buttons.
        const raw=String(a[0]==null? '' : a[0]); let k=1;
        const text=raw.replace(/%(\.\d+)?[sgfd]/gi, m=>{ const v=a[k++]; return typeof v==='number' && /f/i.test(m)? v.toFixed(+((m.match(/\.(\d+)/)||[])[1]||2)) : String(v==null? '' : v); });
        const buttons=a.slice(k).map(String);
        did('MessageBox '+text+(buttons.length? ' ['+buttons.join(' | ')+']' : ''));
        if(buttons.length){ this.msg={run:ctx.run, text, buttons}; this.drawMsg(); }
        return;
      }
      case 'choice': did('Choice '+a.filter((_, i)=>i%2===0).join(' | ')+' (only in a dialogue result)'); return;
      case 'addtopic': { const k=String(a[0]); this.topics.add(k); did('AddTopic '+k); this.drawState(); return; }
      case 'forcegreeting': if(t && !t.player){
        this.flash(t);
        // The greeting itself in the Dialogue window's In game tab, when the Editor has it.
        const open=typeof WgDial==='object' && WgDial.greetFrom? WgDial.greetFrom(t.id) : Promise.resolve(false);
        Promise.resolve(open).then(ok=>did('ForceGreeting: '+t.id+' greets the player'+(ok? ' (in the Dialogue window\'s In game tab)' : ' (open the Editor to see the dialogue)')));
      } return;
      case 'additem': case 'removeitem': case 'drop': {
        const A=this.actorOf(t); if(!A) return;
        const id=String(a[0]).toLowerCase(), n=Math.trunc(this.num(a[1]));
        const was=A.items.get(id)||0, now=fn==='additem'? was+n : Math.max(0, was-n);
        if(now) A.items.set(id, now); else A.items.delete(id);
        did((fn==='additem'? 'AddItem ' : fn==='drop'? 'Drop ' : 'RemoveItem ')+a[0]+' '+n+' ('+now+' now)');
        if(fn==='drop' && was>0){ const p=this.posOf(t); this.placed.push({id:String(a[0]), pos:[p[0]+40, p[1], p[2]], rot:[0,0,0], scale:1, key:'scr:'+(++this._placeN), cell:this.cellOf(t), from:-1, door:null, gc:false, mark:'', hist:null, moved:0}); this._dirtyScene=true; }
        this.drawState(); return;
      }
      case 'equip': did('Equip '+a[0]); return;
      case 'addspell': case 'removespell': {
        const A=this.actorOf(t); if(!A) return;
        A.spells=A.spells||new Set();
        const k=String(a[0]).toLowerCase();
        if(fn==='addspell') A.spells.add(k); else A.spells.delete(k);
        did((fn==='addspell'? 'AddSpell ' : 'RemoveSpell ')+a[0]); this.drawState(); return;
      }
      case 'cast': case 'explodespell': {
        // On the target (Cast) or the caster itself (ExplodeSpell): flashed, and counted as
        // on it for GetSpellEffects for ten seconds.
        const on=fn==='cast'? this.target(String(a[1]||'').toLowerCase(), ctx) : t;
        const A=this.actorOf(on); if(!A) return;
        A.active=A.active||new Map(); A.active.set(String(a[0]).toLowerCase(), performance.now()+10000);
        this.flash(on);
        did((fn==='cast'? 'Cast ' : 'ExplodeSpell ')+a[0]+(fn==='cast'? ' on '+(on.player? 'the player' : on.id) : '')+' (its effects are not worked out)'); return;
      }
      case 'playgroup': case 'loopgroup': {
        // Played on the object's mesh (`playAnim`): once for PlayGroup, the count's times
        // more for LoopGroup, then back to its idle. The flag: 0 (the default) after what a
        // script already plays on it, 1 at once, 2 at once and looping until something else.
        if(!t || t.player) return;
        const lg=fn==='loopgroup', flag=this.num(a[lg? 2 : 1])|0;
        // Asked again while it plays (every frame, in some scripts): nothing to do or to say.
        if(flag!==0 && this.animPlaying(t.key, String(a[0])) && !lg) return;
        const loops=flag===2? Infinity : lg? Math.max(0, this.num(a[1])|0) : 0;
        this.playAnim(t.key, String(a[0]), loops, 1, lg? 'LoopGroup' : 'PlayGroup', {queued:flag===0});
        did((fn==='playgroup'? 'PlayGroup ' : 'LoopGroup ')+a.join(' ')); this.drawLive(); return;
      }
      case 'skipanim': return;
      case 'sethealth': case 'setmagicka': case 'setfatigue':
      case 'modhealth': case 'modmagicka': case 'modfatigue':
      case 'modcurrenthealth': case 'modcurrentmagicka': case 'modcurrentfatigue': {
        const A=this.actorOf(t); if(!A) return;
        const which=/health/.test(fn)? 0 : /magicka/.test(fn)? 1 : 2, v=this.num(a[0]);
        const field=['hp','mp','fp'][which];
        if(fn.startsWith('set')){ A[field]=v; A.max[which]=v; }
        else if(fn.startsWith('modcurrent')) A[field]=Math.min(A.max[which], A[field]+v);
        else { A.max[which]+=v; A[field]+=v; }
        did(s.raw);
        if(which===0) this.hurt(A, 0, null, ctx);
        this.drawState(); return;
      }
      case 'resurrect': { const A=this.actorOf(t); if(A && A.dead){ A.dead=false; A.hp=A.max[0]; A.ondeathSeen=A.onmurderSeen=false; const r=this.rotOf(t); r[0]=0; this.place(t, {rot:r}); did('Resurrect'); } return; }
      case 'aiwander': case 'aitravel': case 'aifollow': case 'aiescort': case 'aiactivate': this.ai(t, fn.slice(2), a, ctx); did(s.raw); return;
      case 'startcombat': { const A=this.actorOf(t), o=this.target(String(a[0]).toLowerCase(), ctx); if(A && o && !A.dead){ A.combat=o.player? {player:true} : o; did('StartCombat '+a[0]); } return; }
      case 'stopcombat': { const A=this.actorOf(t); if(A && A.combat){ A.combat=null; did('StopCombat'); } return; }
      case 'activate': if(t) t._activate=true; return;
    }
    this.unknown(ctx.run.script, fn);
  },
  /* ---- actors: what they carry, their health, AI and fights ------------------------------ */

  /** The cell an object stands in: an interior's lowercased name, or '' for an exterior. */
  cellOf(t){
    const sc=App._scene, cells=(sc && sc.cells)||[];
    if(t && t.player){ const c=sc && sc.centre; return c && c.kind==='int'? String(c.name||'').toLowerCase() : ''; }
    const key=t && t.key;
    for(const c of cells) if(c.kind==='int' && (c.refs||[]).some(r=>r.key===key)) return String(c.name||'').toLowerCase();
    const it=key && this.inst.get(key); if(it && it.cell && !/^-?\d+,-?\d+$/.test(it.cell)) return it.cell;
    const p=key && this.placed.find(q=>q.key===key); if(p) return p.cell||'';
    const int=cells.find(c=>c.kind==='int');
    return int && !cells.some(c=>c.kind!=='int')? String(int.name||'').toLowerCase() : '';
  },
  /** An actor's (or container's) state, made the first time a script asks: what the load
   *  order says it carries and its health, magicka and fatigue (an NPC whose stats the game
   *  works out starts at 100 each). The player starts with nothing and 100s; the panel sets
   *  both. */
  actorOf(t, quiet){
    if(!t) return null;
    const key=t.player? 'player' : t.key; if(!key) return null;
    let A=this.actors.get(key);
    if(A) return A;
    const def=t.player? null : this.actorDefs.get(t.id);
    const st=(def && def.stats)||[100, 100, 100];
    A={key, id:t.player? 'player' : t.id, tag:t.player? 'NPC_' : def? def.tag : '', t:t.player? {player:true} : t,
      items:new Map(((def && def.items)||[]).filter(([, n])=>n).map(([i, n])=>[i, Math.abs(n)])),
      hp:st[0], mp:st[1], fp:st[2], max:st.slice(0, 3), ai:null, combat:null, dead:false, attacked:false, _hitT:0,
      // How it fights (its NPDT's level, strength, agility, attack skill and, for a creature,
      // its strongest attack); an auto-calculated NPC and the player: middling numbers.
      fight:{level:st[3]||1, str:st[4]||50, agi:st[5]||50, attack:st[6]||(t.player? 50 : 35), most:st[7]||0},
      // How it moves (its Speed and Athletics; a creature has no Athletics) and its AI
      // ratings, [hello, fight, flee, alarm], for 77_wg_ai.js. An auto-calculated NPC moves
      // at a middling Speed.
      move:{speed:st[8]!=null? st[8] : 50, athletics:st[9]!=null? st[9] : (def && def.tag==='CREA'? -1 : 30), creature:!!(def && def.tag==='CREA')},
      rates:(def && def.rates)||[0, 0, 0, 0]};
    this.actors.set(key, A);
    if(!quiet) this.drawState();
    return A;
  },
  dist(a, b){ if(!a || !b) return Infinity; const p=this.posOf(a), q=this.posOf(b); return Math.hypot(p[0]-q[0], p[1]-q[1], p[2]-q[2]); },
  /** Whether a can see b: nothing the scene draws between their heads. */
  los(a, b){
    if(!a || !b) return false;
    const R=App.R; if(!R || !R.pickRay) return true;
    const o=(App._scene && App._scene.origin)||[0,0];
    const p=this.posOf(a), q=this.posOf(b), up=a.player? 116 : 100, upB=b.player? 116 : 100;
    const eye=[p[0]-o[0], p[1]-o[1], p[2]+up], to=[q[0]-o[0], q[1]-o[1], q[2]+upB];
    const d=[to[0]-eye[0], to[1]-eye[1], to[2]-eye[2]], len=Math.hypot(...d)||1;
    const dir=d.map(x=>x/len);
    // Start past a's own body, so it does not hide b from itself.
    const start=eye.map((x, i)=>x+dir[i]*40);
    const hit=R.pickRay(start, dir);
    if(!hit || hit.t>=len-80) return true;
    const k=hit.obj && hit.obj.refKey;
    return !!k && (k===b.key || k===a.key);
  },
  /** An AI package for an actor: `AiWander range, duration, time, idles…`, `AiTravel x, y,
   *  z`, `AiFollow target, duration, x, y, z`, `AiEscort target, duration, x, y, z`,
   *  `AiActivate target`. A duration is game hours (30 game seconds a real one). */
  /** An AI package on an actor, from MWScript's Ai* or a record's own (`own`: no redraw of
   *  the panel per actor). The Reset flag, last, has it start again once done (OpenMW's
   *  repeat); AiWander's `time` is kept as read (OpenMW keeps it and does not act on it). */
  ai(t, kind, a, ctx, own){
    const A=this.actorOf(t, own); if(!A || A.dead || t.player) return;
    const hours=x=>this.num(x)>0? this.num(x)*3600/30 : Infinity;
    const p=this.posOf(t), reset=x=>this.num(x)!==0;
    // AiWander range duration time idle2 ... idle9 [reset]: the idles' chances, for the waits.
    if(kind==='wander') A.ai={kind, home:p, range:Math.max(0, this.num(a[0])), left:hours(a[1]), time:this.num(a[2]), goal:null, wait:0, idle:a.slice(3, 11).map(x=>this.num(x)), repeat:reset(a[11])};
    else if(kind==='travel') A.ai={kind, goal:[this.num(a[0]), this.num(a[1]), this.num(a[2])], repeat:reset(a[3])};
    else if(kind==='follow' || kind==='escort'){
      const who=this.target(String(a[0]).toLowerCase(), ctx);
      const goal=a.length>=5 && (this.num(a[2]) || this.num(a[3]) || this.num(a[4]))? [this.num(a[2]), this.num(a[3]), this.num(a[4])] : null;
      A.ai={kind, who, left:hours(a[1]), goal, repeat:reset(a[5])};
    }
    else if(kind==='activate') A.ai={kind, who:this.target(String(a[0]).toLowerCase(), ctx), repeat:reset(a[1])};
    if(A.ai){ A.ai.done=false; A.ai.args=a.slice(); A.ai.own=!!own; A._route=null; }
    if(!own){ A.packages=null; this.drawState(); }
  },
  /** A package done: again when it repeats, else the record's next one, if it has more. */
  nextPackage(A){
    const ai=A.ai; if(!ai || !ai.done || A.dead) return;
    const t=A.t;
    if(ai.repeat){ this.ai(t, ai.kind, ai.args, {self:t}, ai.own); if(!ai.own) A.packages=null; return; }
    if(A.packages && A.pkg<A.packages.length-1){
      A.pkg++;
      const [k, args]=A.packages[A.pkg];
      this.ai(t, k, args, {self:t}, true);
    }
  },
  /** One frame of every actor that has somewhere to go: a fight first, then its package.
   *  Routes over the pathgrid and the walk while it moves are 77_wg_ai.js's (`go`). */
  actorsFrame(dt){
    const walk=150, run=300, W=typeof WgAI==='object'? WgAI : null;
    for(const A of this.actors.values()){
      if(A.key==='player') continue;
      A._moved=false; A._running=false;
      // Its own pace, from its record (77_wg_ai.js `speeds`).
      const pace=W? W.speeds(A) : {walk, run};
      // One actor's failure is said once and does not stop the others' frame.
      if(!A.dead && !this.disabled.has(A.key)){
        try{
          // A Lua script that took the AI off drives it itself (`self.controls`); else the AI does.
          if(A.luaAI===false){ if(A.ctl) this.byControls(A, dt, pace); }
          else this.actorStep(A, dt, pace.walk, pace.run, W);
        }
        catch(e){ const k='aierr|'+A.key; if(!this._logged.has(k)){ this._logged.add(k); this.say(A.id, 'error', 'AI: '+String(e && e.message || e)); } }
      }
      // The walk while it moves; its idle once it stops (a script's own group wins, `WgAI.loop`).
      // Only near the camera: farther off a walk is not seen, and each change is a new mesh.
      if(W && A.key && W.near(A)){ if(A._moved && !A.dead) W.loop(A.key, A._running? 'RunForward' : 'WalkForward', A._running? pace.run : pace.walk); else W.still(A.key); }
    }
  },
  /** An actor moved by its Lua script's controls, as OpenMW's character controller takes them:
   *  `yawChange` turned (radians, this frame), then forward (`movement`) and to the right
   *  (`sideMovement`), each -1 to 1 of its walk or run, on the ground. */
  byControls(A, dt, pace){
    const c=A.ctl; if(!c) return;
    const r=this.rotOf(A.t);
    if(c.y){ r[2]+=c.y; }
    const sp=(c.run? pace.run : pace.walk)*dt, f=Math.max(-1, Math.min(1, c.m)), sd=Math.max(-1, Math.min(1, c.s));
    if(!f && !sd){ if(c.y) this.place(A.t, {rot:r}); return; }
    const p=this.posOf(A.t), yaw=r[2];
    // Forward is +y turned by the yaw (as `stepTo` faces: atan2(dx, dy)); right is +x turned.
    p[0]+=(Math.sin(yaw)*f+Math.cos(yaw)*sd)*sp; p[1]+=(Math.cos(yaw)*f-Math.sin(yaw)*sd)*sp;
    p[2]=this.floorAt(p, null, 0);
    if(typeof WgAI==='object' && A.key) WgAI.room(A.key, p);
    this.place(A.t, {pos:p, rot:r});
    A._moved=true; A._running=!!c.run;
  },
  actorStep(A, dt, walk, run, W){
    if(A.combat){
      const foe=A.combat, fa=this.actorOf(foe);
      if(!fa || fa.dead){ A.combat=null; A._route=null; A._fleeing=false; return; }
      const d=this.dist(A.t, foe);
      // Fleeing: its Flee and its lost health come to 100 - it runs, and stops once clear.
      if(W && W.flees(A)){
        if(!A._fleeing){ A._fleeing=true; A._route=null; this.say(A.id, 'do', A.id+' flees from '+(fa.id||'?')); }
        if(d>=W.FLEE){ A.combat=null; A._fleeing=false; A._route=null; this.say(A.id, 'do', A.id+' stops fleeing'); return; }
        A._running=true;
        if(!A._fleeTo || (A._fleeAge=(A._fleeAge||0)+dt)>2){ A._fleeTo=W.fleePoint(this.posOf(A.t), this.posOf(foe)); A._fleeAge=0; }
        this.go(A, A._fleeTo, run, 64, dt, false);
        return;
      }
      if(d>120){ A._running=true; this.go(A, this.posOf(foe), run, 100, dt, true); }
      else{
        this.face(A.t, this.posOf(foe));
        A._hitT+=dt;
        if(A._hitT>=1){ A._hitT=0; this.blow(A, fa); }
      }
      return;
    }
    if(A.ai && A.ai.done) this.nextPackage(A);
    const ai=A.ai;
    // A greeting: within its Hello's reach of the player, one standing or wandering turns to it and stands.
    if(W && (!ai || ai.done || ai.kind==='wander') && W.greet(A, dt)){ A._state='greeting'; return; }
    if(!ai || ai.done) return;
    if(ai.left!=null && ai.left!==Infinity){ ai.left-=dt; if(ai.left<=0){ ai.done=true; A._route=null; return; } }
    switch(ai.kind){
      case 'travel': if(this.go(A, ai.goal, walk, 64, dt, false)) ai.done=true; break;
      case 'wander': {
        // A wait, with the idle it chose playing out; then the next grid point in its range.
        const own=W && W.own(A.key);
        if(ai.wait>0 || (own && own.loops!==Infinity)){ ai.wait-=dt; A._state='waiting'; break; }
        if(!ai.range) { ai.wait=4+Math.random()*4; if(W) W.idle(A.key, W.pickIdle(ai.idle)); break; }
        if(!ai.goal) ai.goal=W? W.wanderPoint(ai.home, ai.range) : (()=>{ const r=Math.random()*ai.range, th=Math.random()*Math.PI*2; return [ai.home[0]+r*Math.cos(th), ai.home[1]+r*Math.sin(th), ai.home[2]]; })();
        A._state='walking';
        if(this.go(A, ai.goal, walk, 32, dt, false)){ ai.goal=null; ai.wait=2+Math.random()*4; if(W) W.idle(A.key, W.pickIdle(ai.idle)); }
        break;
      }
      case 'follow': case 'escort': {
        if(ai.goal && this.dist({pos:ai.goal, key:'#'}, A.t)<200 && ai.kind==='escort'){ ai.done=true; break; }
        if(ai.kind==='follow' && ai.goal && Math.hypot(...this.posOf(A.t).map((x, i)=>x-ai.goal[i]))<200){ ai.done=true; break; }
        const toGoal=ai.kind==='escort' && ai.goal;
        const lead=toGoal? ai.goal : this.posOf(ai.who);
        const far=this.dist(A.t, ai.who)>600;
        A._running=far;
        this.go(A, lead, far? run : walk, toGoal? 64 : 180, dt, !toGoal);
        break;
      }
      case 'activate':
        if(this.go(A, this.posOf(ai.who), walk, 120, dt, false)){ if(ai.who) ai.who._activate=true; ai.done=true; this.say(A.id, 'do', 'activated '+(ai.who && ai.who.id || '')); }
        break;
    }
  },
  /** An actor along a route to `dest` (`WgAI.route`, over the pathgrid; a straight line
   *  without one), at `speed` units a second; true once within `near` of it. Planned again
   *  when the destination moves away from where the route ends, and every two seconds after
   *  one that moves (`moving`: a target it follows or fights). */
  go(A, dest, speed, near, dt, moving){
    if(!dest) return true;
    const W=typeof WgAI==='object'? WgAI : null;
    let R=A._route;
    A._routeAge=(A._routeAge||0)+dt;
    if(!R || Math.hypot(R.to[0]-dest[0], R.to[1]-dest[1])>(moving? 150 : 1) || (moving && A._routeAge>2)){
      R=A._route={to:dest.slice(), pts:W? W.route(this.posOf(A.t), dest) : [dest.slice()]};
      A._routeAge=0;
    }
    if(moving) R.pts[R.pts.length-1]=dest.slice();
    for(let n=0;n<64;n++){
      const last=R.pts.length===1;
      const st=this.stepTo(A.t, R.pts[0], speed*dt, last? near : 48);
      if(st==='blocked'){
        // Held by a wall or an object: a moment, then this leg is given up - a wanderer
        // picks somewhere else, anything else plans its way again.
        A._blockedT=(A._blockedT||0)+dt;
        if(A._blockedT>0.75){ A._blockedT=0; A._route=null; return true; }
        return false;
      }
      A._blockedT=0;
      if(!st){ A._moved=true; return false; }
      if(last){ A._route=null; A._moved=false; return true; }   // arrived: it stands (its idle, or one the AI plays)
      R.pts.shift();
    }
    return false;
  },
  /** An actor a step toward a point, facing it, on the ground; true once within `near`. */
  stepTo(t, goal, step, near){
    if(!goal) return true;
    const p=this.posOf(t), dx=goal[0]-p[0], dy=goal[1]-p[1], d=Math.hypot(dx, dy);
    if(d<=near) return true;
    const s=Math.min(step, d-near*0.9);
    const o=(App._scene && App._scene.origin)||[0,0];
    /* What the walking camera collides with stops an actor too (71_wg_walk.js `aiBlocked`:
       the objects' collision shapes, at the knee and the chest): it stands, facing the way
       it meant to go, and `go` gives the leg up after a moment. */
    if(typeof WgWalk==='object' && WgWalk.aiBlocked && t.key && WgWalk.aiBlocked([p[0]-o[0], p[1]-o[1], p[2]], dx/d*s, dy/d*s)){
      const r=this.rotOf(t); r[2]=Math.atan2(dx, dy); this.place(t, {rot:r});
      return 'blocked';
    }
    p[0]+=dx/d*s; p[1]+=dy/d*s;
    // Room: out of the other actors' way, and the walking player's (77_wg_ai.js).
    if(typeof WgAI==='object' && t.key) WgAI.room(t.key, p);
    p[2]=this.floorAt(p, goal, s/d);
    const r=this.rotOf(t); r[2]=Math.atan2(dx, dy);
    this.place(t, {pos:p, rot:r});
    return false;
  },
  /** The height an actor stepping to `p` (world units, its feet's height still the old one)
   *  stands at: on the land and on the objects' collision surfaces, a step up at most
   *  (71_wg_walk.js `aiFloor`); with neither under it, the land, else toward the goal's
   *  height by `frac` of the way, as before. */
  floorAt(p, goal, frac){
    const R=App.R, o=(App._scene && App._scene.origin)||[0,0];
    const gz=R && R.groundZ? R.groundZ(p[0]-o[0], p[1]-o[1]) : null;
    if(typeof WgWalk==='object' && WgWalk.aiFloor){
      const f=WgWalk.aiFloor(p[0]-o[0], p[1]-o[1], p[2], gz);
      if(f!=null) return f;
    }
    if(gz!=null) return gz;
    return goal && goal[2]!=null && Math.abs(goal[2]-p[2])<400? p[2]+(goal[2]-p[2])*Math.min(1, frac) : p[2];
  },
  face(t, at){ const p=this.posOf(t), r=this.rotOf(t); const z=Math.atan2(at[0]-p[0], at[1]-p[1]); if(Math.abs(r[2]-z)>0.05){ r[2]=z; this.place(t, {rot:r}); } },
  /** One swing of a fight, roughly as the game weighs it: the chance to hit is the
   *  attacker's skill and a fifth of its agility, less a fifth of the defender's; the blow is
   *  a creature's attack (up to its strongest) or what an NPC's strength and skill give a
   *  blow - with no weapons, armor or fatigue. */
  blow(A, D){
    const f=A.fight||{}, g=D.fight||{};
    const chance=Math.max(5, Math.min(95, (f.attack||35)+(f.agi||50)/5+5-((g.agi||50)/5+5)));
    if(Math.random()*100>=chance){ this.say(A.id, 'do', A.id+' misses '+D.id+' ('+Math.round(chance)+'% to hit)'); return; }
    const n=f.most? 1+Math.floor(Math.random()*f.most) : Math.max(1, Math.round((1+(f.str||50)/10+(f.attack||35)/20)*(0.5+Math.random())));
    this.hurt(D, n, A, null);
  },
  /** A blow (or a health change): at 0 an actor dies, lies down, and stops fighting; the one
   *  struck fights back. */
  hurt(A, n, by, ctx){
    if(A.dead) return;
    if(n){ A.hp-=n; A.attacked=true; this.say(by? by.id : '', 'do', (by? by.id : '?')+' hits '+A.id+' for '+n+' ('+Math.max(0, Math.round(A.hp))+' left)'); }
    if(by && !A.combat && A.key!=='player') A.combat=by.t;
    if(A.hp<=0){
      A.hp=0; A.dead=true; A.combat=null; if(A.ai) A.ai.done=true;
      for(const B of this.actors.values()) if(B.combat && (B.combat.player? A.key==='player' : B.combat.key===A.key)) B.combat=null;
      this.say(A.id, 'do', A.id+' dies');
      if(A.key!=='player'){ const r=this.rotOf(A.t); r[0]=-Math.PI/2; this.place(A.t, {rot:r}); }
    }
    this.drawState();
  },
  /** A greeting shown on the object for a moment (lit gold, as a hovered object is). */
  flash(t){
    const R=App.R; if(!R || !t || t.player || !R.setStaticHighlight) return;
    const pk=(R.pickables||[]).find(o=>o.refKey===t.key); if(!pk) return;
    try{ R.setStaticHighlight([pk]); setTimeout(()=>{ try{ R.setStaticHighlight(null); }catch(_){ } }, 1500); }catch(_){ }
  },

  /** (x, y, z) in an object's own frame, turned into the world's (the game's z, then y, x). */
  local(rot, v){
    const [rx, ry, rz]=rot||[0,0,0];
    let [x, y, z]=v;
    // x
    let c=Math.cos(-rx), s=Math.sin(-rx); [y, z]=[y*c-z*s, y*s+z*c];
    c=Math.cos(-ry); s=Math.sin(-ry); [x, z]=[x*c+z*s, -x*s+z*c];
    c=Math.cos(-rz); s=Math.sin(-rz); [x, y]=[x*c-y*s, x*s+y*c];
    return [x, y, z];
  },
  place(t, change){
    const m=Object.assign({}, this.moved.get(t.key)||{});
    let same=true;
    for(const k of Object.keys(change)){
      const a=change[k], b=m[k];
      if(!(Array.isArray(a) && Array.isArray(b) && a.every((x,i)=>Math.abs(x-b[i])<1e-4)) && a!==b){ same=false; m[k]=Array.isArray(a)? a.slice() : a; }
    }
    if(same) return;
    this.moved.set(t.key, m);
    /* An actor drawn live is moved where it stands (18_cellpreview.js `previewMoveRef`), so
       it glides; anything else waits for the scene to be drawn again, a few times a second. */
    const r=t._ref || t.ref || this.refByKey(t.key);
    const live=typeof previewMoveRef==='function' && r &&
      previewMoveRef(t.key, m.pos || r.pos || [0,0,0], m.rot || r.rot || [0,0,0], m.scale!=null? m.scale : (r.scale||1));
    if(!live) this._dirtyScene=true;
  },

  /* ---- animation groups ------------------------------------------------------------------ */

  /** The groups a model's `.kf` marks, `[[group, start, stop], ...]` (the engine's
   *  `anim_groups`), asked once a model. */
  async groupsOf(model){
    // An NPC's groups are its skeleton's, whatever it is dressed in: one ask for every flag.
    const k=String(model||'').replace(/^(__npc\/[^?]*)\?.*$/, '$1');
    if(!k) return [];
    if(!this._groups.has(k)){
      this._groups.set(k, (async()=>{ try{ const r=await Engine.call('anim_groups',{models:[k]}); const o=typeof r==='string'? JSON.parse(r) : r; const l=(o && o[k])||[]; (this._groupsDone||(this._groupsDone=new Map())).set(k, l); return l; }catch(_){ return []; } })());
    }
    return this._groups.get(k);
  },
  /** The groups of a model once they are known, at once (null until then; asked for then). */
  groupsKnown(model){
    const k=String(model||'').replace(/^(__npc\/[^?]*)\?.*$/, '$1');
    const l=this._groupsDone && this._groupsDone.get(k);
    if(!l && k) this.groupsOf(k);
    return l||null;
  },
  /** The model an object is drawn with (the renderer's pickable), without a group a
   *  script is playing on it. */
  modelOfKey(key){
    const pk=((App.R && App.R.pickables)||[]).find(o=>o.refKey===key);
    const m=pk && pk.model || '';
    return m.replace(/^__grp\/[^\/]+\//, '').replace(/([?&])group=[^&]*&?/, '$1').replace(/[?&]$/, '');
  },
  /** A script plays `group` on the object `key`, on the bone groups in `mask` (OpenMW's
   *  BLEND_MASK: 1 lower body, 2 torso, 4 left arm, 8 right arm; 15, the default, the whole
   *  body). Drawn from the group's first frame, `loops` times more after the first
   *  (Infinity: until something else is played on those bones), at `speed`, then those
   *  bones go back to the idle. `queued`: after what a script already plays on any of those
   *  bones has ended (playQueued; PlayGroup's flag 0), else at once. The group's length comes
   *  from the `.kf`; one the file does not have is reported and nothing changes. `by` names
   *  the caller for the log. Returns the play, or null when it waits in the queue. */
  playAnim(key, group, loops, speed, by, opt){
    const o=opt||{}, mask=(o.mask|0)&15 || 15;
    let st=this.anims.get(key);
    if(!st){ st={slots:[null, null, null, null], queue:[]}; this.anims.set(key, st); }
    /* The same group already on those bones (a script that calls PlayGroup every frame, as the
       mist and banner mods do): left playing, not started over - which would draw the scene
       again every frame. */
    const same=[0, 1, 2, 3].filter(i=>(mask>>i)&1).map(i=>st.slots[i]);
    if(!o.queued && same[0] && same.every(x=>x===same[0]) && same[0].group.toLowerCase()===String(group).toLowerCase() && same[0].mask===mask) return same[0];
    const p={group:String(group), loops:loops==null? 0 : loops, speed:speed>0? speed : 1, mask, by:by||'animation', at:0, t0:0, ends:null, len:null, quiet:!!o.quiet, onLack:o.onLack||null, fixedAt:o.at};
    if(o.queued && st.slots.some((x, i)=>x && (mask>>i)&1)){
      // Queued once: a script asking every frame does not pile up a queue.
      const g=p.group.toLowerCase();
      if(!st.queue.some(q=>q.group.toLowerCase()===g && q.mask===mask) && !st.slots.some((x, i)=>x && (mask>>i)&1 && x.group.toLowerCase()===g)) st.queue.push(p);
      return null;
    }
    this.startPlay(key, st, p);
    return p;
  },
  /** A play put on its bones now: the renderer's clock noted (the engine times the clip by
   *  it), the actor told, the scene drawn again, and its length asked of the `.kf`. */
  startPlay(key, st, p){
    const R=App.R;
    // `fixedAt`: the AI's looping walk begins at 0, so an actor's walk is one request whenever it starts.
    p.at=p.fixedAt!=null? p.fixedAt : Math.round((((R && R._uvSecs)||0))*1000)/1000; p.t0=performance.now();
    for(let i=0;i<4;i++) if((p.mask>>i)&1) st.slots[i]=p;
    const A=this.actorOf(this.inst.get(key) || {key, id:''}, true); if(A) A.anim={group:p.group, loops:p.loops};
    this.redraw(p, key);
    const model=this.modelOfKey(key);
    if(!model) return;
    this.groupsOf(model).then(list=>{
      if(!st.slots.includes(p)) return;
      const g=list.find(x=>String(x[0]).toLowerCase()===p.group.toLowerCase());
      // A group with no length (a banner's Idle, its still pose): the mesh's own idle.
      if(g && !((+g[2])-(+g[1])>1e-6)){ this.endPlay(key, st, p); return; }
      if(!g){
        this.endPlay(key, st, p);
        if(p.onLack) p.onLack();
        if(p.quiet) return;   // the AI's walk on a mesh without one: it just slides, as before
        // Said once a mesh and group, however often a script asks.
        const said='nogroup|'+model+'|'+p.group.toLowerCase();
        if(this._logged.has(said)) return;
        this._logged.add(said);
        this.say('', 'skip', p.by+': '+model+' has no group '+p.group+(list.length? ' (it has '+list.map(x=>x[0]).join(', ')+')' : ' (no .kf beside it)'));
        return;
      }
      p.group=String(g[0]); p.len=Math.max(0, (+g[2])-(+g[1]));
      if(A && A.anim) A.anim.group=p.group;   // the file's spelling
      if(isFinite(p.loops)) p.ends=p.t0+p.len*(p.loops+1)/p.speed*1000;
      this.redraw(p, key);
    });
  },
  /** A play off its bones (they go back to the idle), and whatever waited for them started. */
  endPlay(key, st, p){
    for(let i=0;i<4;i++) if(st.slots[i]===p) st.slots[i]=null;
    this.redraw(p, key);
    while(st.queue.length && !st.slots.some((x, i)=>x && (st.queue[0].mask>>i)&1)) this.startPlay(key, st, st.queue.shift());
    const live=st.slots.find(Boolean);
    const A=this.actors.get(key); if(A) A.anim=live? {group:live.group, loops:live.loops} : null;
    if(!live && !st.queue.length) this.anims.delete(key);
  },
  /** The scene drawn again for a play that changed: at once for a script's; for the AI's walks
   *  and idles, gathered up and drawn at most every second and a half (`frame`) - a town of
   *  wanderers would otherwise have it drawn again all the time. */
  redraw(p, key){
    // A live actor (an actor while the Scripts run) has its own mesh swapped, alone and at once
    // (18_cellpreview.js `previewSwapActor`); the scene is drawn again only if that cannot be.
    if(key && typeof previewSwapActor==='function' && App._liveRefs && App._liveRefs.has(key)){
      const spec=this.animSpec(key);
      previewSwapActor(key, spec).then(ok=>{ if(!ok && this.animSpec(key)===spec){ if(p && p.by==='AI') this._dirtyAI=true; else this._dirtyScene=true; } }, ()=>{ this._dirtyScene=true; });
      return;
    }
    /* Anything else shows a group only by the scene being drawn again - but only when what
       the objects play has changed (`animSig`): a mist that plays its idle2 again and again
       (MWScript `PlayGroup idle2 1`, every frame) drew the scene again three times a second,
       and each time put every walking actor back where it stood when the drawing began. */
    this._dirtyAnim=true;
  },
  /** What the objects not drawn live play, as the scene draws it: each one's groups and
   *  speeds, not when each began (a group begun again looks the same). */
  animSig(){
    const live=App._liveRefs, out=[];
    for(const k of this.anims.keys()){
      if(live && live.has(k)) continue;
      const sp=this.animSpec(k);
      if(sp) out.push(k+'='+sp.replace(/@[^,*]*/g, ''));
    }
    return out.sort().join('|');
  },
  /** Stop `group` (every group, when not given) on `key`, playing or queued. */
  stopAnim(key, group){
    const st=this.anims.get(key); if(!st) return;
    const is=p=>!group || p.group.toLowerCase()===String(group).toLowerCase();
    st.queue=st.queue.filter(p=>!is(p));
    for(const p of new Set(st.slots.filter(Boolean))) if(is(p)) this.endPlay(key, st, p);
  },
  /** What waits on `key` dropped (clearAnimationQueue). */
  clearQueue(key){ const st=this.anims.get(key); if(st){ st.queue=[]; if(!st.slots.some(Boolean)) this.anims.delete(key); } },
  /** Whether `group` (any, when not given) is playing on `key` now, on any bone group. */
  animPlaying(key, group){
    const st=this.anims.get(key);
    return !!st && st.slots.some(p=>p && (!group || p.group.toLowerCase()===String(group).toLowerCase()));
  },
  /** The four bone groups' groups on `key`, '' where the idle plays. */
  animSlots(key){ const st=this.anims.get(key); return [0, 1, 2, 3].map(i=>st && st.slots[i]? st.slots[i].group : ''); },
  /** The engine's request for what plays on `key` (viewcore anim.rs `parse_spec`): one slot
   *  when one play has the whole body, else four; null when only the idle plays. */
  animSpec(key){
    const st=this.anims.get(key); if(!st || !st.slots.some(Boolean)) return null;
    const one=p=>p? String(p.group).replace(/[\/\\?&=,@*]/g, '')+'@'+p.at+(p.speed!==1? '*'+p.speed : '') : '';
    const s=st.slots;
    return (s[0] && s.every(p=>p===s[0]))? one(s[0]) : s.map(one).join(',');
  },
  /** The plays whose time is up leave their bones, and the queue moves on. */
  animsFrame(){
    if(!this.anims.size) return;
    const now=performance.now();
    for(const [k, st] of [...this.anims]) for(const p of new Set(st.slots.filter(Boolean))){
      if(p.ends!=null && now>=p.ends) this.endPlay(k, st, p);
      /* A play once whose length never came (the model not known when it began, or the
         engine not answering): ended after five seconds, rather than holding whatever waits
         on it - a wanderer waiting out its idle - for ever. */
      else if(p.ends==null && isFinite(p.loops) && now-p.t0>5000) this.endPlay(k, st, p);
    }
  },

  /* ---- what the scene shows ------------------------------------------------------------ */

  /** A cell as the scripts leave it (17_cell.js `live`): disabled references out, moved
   *  ones where the scripts put them, placed ones in the cell they were placed in. */
  overlay(rec){
    if(!this.on || !rec || !rec.refs) return rec;
    const touch=this.disabled.size || this.moved.size || this.placed.length || this.anims.size;
    if(!touch) return rec;
    const refs=[];
    // A group a script plays: on the reference for the renderer (18_cellpreview.js `animModel`).
    const withAnim=(r)=>{ const sp=this.anims.size? this.animSpec(r.key) : null; return sp? Object.assign({}, r, {anim:sp}) : r; };
    for(const r0 of rec.refs){
      if(this.disabled.has(r0.key)) continue;
      const r=withAnim(r0);
      const m=this.moved.get(r.key);
      if(!m){ refs.push(r); continue; }
      const n=Object.assign({}, r);
      if(m.pos) n.pos=m.pos.slice(); if(m.rot) n.rot=m.rot.slice(); if(m.scale!=null) n.scale=m.scale;
      refs.push(n);
    }
    const isInt=rec.kind==='int';
    for(const p of this.placed){
      if(this.disabled.has(p.key)) continue;
      // An exterior's placed objects go in the cell they stand in.
      if(!isInt){ const gx=Math.floor(p.pos[0]/8192), gy=Math.floor(p.pos[1]/8192); if(p.cell || gx!==rec.gx || gy!==rec.gy) continue; }
      else if(p.cell!==String(rec.name||'').toLowerCase()) continue;
      const m=this.moved.get(p.key);
      refs.push(withAnim(m? Object.assign({}, p, m.pos? {pos:m.pos.slice()} : {}, m.rot? {rot:m.rot.slice()} : {}, m.scale!=null? {scale:m.scale} : {}) : p));
    }
    return Object.assign({}, rec, {refs});
  },
  /** The scene drawn again, at most a few times a second, when the scripts changed it. */
  /* What the scripts changed that is not moved in place - an object turned off or placed, an
     animation on something not drawn live - used to ask for the preview again here, every
     350 ms at most (and every 1.5 s while the AI walked). The scene key does not change while
     they run, so that never built the scene again: it scattered the grass of every loaded cell
     anew and changed nothing else - on a big load order, a stall several times a second while
     the actors walked (Static 2026-11-23: "while moving ... performance tanks"). It asks for
     nothing now; those changes are drawn when the scene is next built (closing the Scripts
     builds it again when there are any, `reset`). Actors are moved and swapped in place. */
  flushScene(){
    const now=performance.now();
    if(now-this._lastRebuild<350) return;
    this._dirtyScene=false; this._dirtyAI=false; this._lastRebuild=now;
    this._animSigDrawn=this.animSig();
  },
  fade(to, secs){
    let el=$('#wgScrFade');
    if(!el && to<=0) return;
    if(!el){ el=document.createElement('div'); el.id='wgScrFade';
      el.style.cssText='position:absolute;inset:0;background:#000;opacity:0;pointer-events:none;z-index:5;transition:opacity 0s linear';
      ($('#vpwrap')||document.body).appendChild(el); }
    el.style.transition='opacity '+Math.max(0, secs)+'s linear';
    el.style.opacity=String(Math.max(0, Math.min(1, to)));
  },

  /* ---- sound ----------------------------------------------------------------------------- */

  async rec(id){
    try{ const rows=await Engine.call('sound_records',{ids:[id]}); return Array.isArray(rows) && rows[0]? rows[0] : null; }catch(_){ return null; }
  },
  /** PlaySound(3D): the record's file, at the object when it has one (heard from where the
   *  camera is), and remembered as playing until it ends (GetSoundPlaying). */
  async sound(id, vol, at){
    const k=(at && at.key || 'player')+'|'+id.toLowerCase();
    this.playing.set(k, performance.now()+1000);   // until its length is known
    if(typeof WgSound!=='object' || !WgSound.place.on) return;
    const d=await this.rec(id); if(!d){ this.playing.delete(k); return; }
    this.playAt(d, d.gain*Math.max(0, Math.min(1, vol)), at && !at.player? at : null, k);
  },
  /** Say: the voice file at the speaker; SayDone is false until it ends. */
  async voice(path, at){
    const k=(at && at.key || 'player')+'|#say';
    this.playing.set(k, performance.now()+3000);
    if(typeof WgSound!=='object' || !WgSound.place.on) return;
    this.playAt({file:path, gain:1, min:100, max:2000}, 1, at && !at.player? at : null, k);
  },
  async playAt(d, gain, at, k){
    const S=WgSound;
    try{
      const ac=S.audio(); S.placeGraph(ac);
      const buf=await S.playable(d);
      this.playing.set(k, performance.now()+buf.duration*1000);
      if(!S.place.fx) return;
      const src=ac.createBufferSource(), g=ac.createGain();
      src.buffer=buf; g.gain.value=Math.min(1, gain);
      let out=g;
      if(at && ac.createPanner){
        const pan=ac.createPanner(), o=(App._scene && App._scene.origin)||[0,0], p=this.posOf(at);
        pan.panningModel='HRTF'; pan.distanceModel='inverse'; pan.refDistance=Math.max(1, d.min||100); pan.maxDistance=Math.max(pan.refDistance+1, d.max||2000);
        try{ pan.positionX.value=p[0]-o[0]; pan.positionY.value=p[1]-o[1]; pan.positionZ.value=p[2]; }catch(_){ }
        g.connect(pan); out=pan;
      }
      src.connect(g); out.connect(S.place.fx); src.start();
    }catch(_){ this.playing.delete(k); }
  },
  soundOn(t, id){
    const k=(t.player? 'player' : t.key)+'|'+id.toLowerCase();
    if(this.loops.has(k)) return true;
    const until=this.playing.get(k);
    if(until==null) return false;
    if(until<performance.now()){ this.playing.delete(k); return false; }
    return true;
  },
  /** StreamMusic: the file, through the Preview's music volume, in place of what plays. */
  async music(path){
    if(typeof WgSound!=='object' || !WgSound.place.on || WgSound.place.musicOn===false) return;
    const S=WgSound, P=S.place, ctx=S.audio();
    S.placeGraph(ctx);
    S.musicStop();
    const p=String(path).replace(/\\/g,'/'), file=/^music\//i.test(p)? p : 'Music/'+p;
    const m=P.music={src:null, file, asked:true};
    try{
      const buf=await S.buffer({path:file, cache:null, info:null});
      if(P.music!==m) return;
      const src=ctx.createBufferSource(); src.buffer=buf; src.connect(P.mus); src.start(); m.src=src;
      src.onended=()=>{ if(P.music===m) P.music=null; };
    }catch(err){ this.say('', 'warn', 'StreamMusic '+path+' did not play: '+String(err && err.message || err)); }
  },
  /** PlayLoopSound3D on an object: one loop a key and sound, at the object. */
  async loop(t, id, vol, ctx){
    if(!t || typeof WgSound!=='object' || !WgSound.place.on) return;
    const k=t.key+'|'+id.toLowerCase();
    if(this.loops.has(k)) return;
    this.loops.set(k, {t, pending:true});
    this.say(ctx.run.script, 'do', 'PlayLoopSound3D '+id);
    const d=await this.rec(id); const v=this.loops.get(k);
    if(!d || !v){ this.loops.delete(k); return; }
    try{
      const S=WgSound, ac=S.audio(); S.placeGraph(ac);
      const buf=await S.playable(d);
      if(this.loops.get(k)!==v) return;
      const src=ac.createBufferSource(), g=ac.createGain(), pan=ac.createPanner();
      pan.panningModel='HRTF'; pan.distanceModel='inverse'; pan.refDistance=Math.max(1, d.min||100); pan.maxDistance=Math.max(pan.refDistance+1, d.max||2000); pan.rolloffFactor=1;
      src.buffer=buf; src.loop=true; g.gain.value=Math.min(1, d.gain*vol);
      src.connect(g).connect(pan).connect(S.place.fx); src.start();
      Object.assign(v, {src, g, pan, pending:false, max:d.max||2000});
    }catch(_){ this.loops.delete(k); }
  },
  loopStop(k){
    const v=this.loops.get(k); if(!v) return;
    this.loops.delete(k);
    try{ if(v.src) v.src.stop(); }catch(_){}
  },
  loopsFrame(){
    const o=(App._scene && App._scene.origin)||[0,0];
    for(const [k, v] of this.loops){
      if(!v.pan) continue;
      if(this.disabled.has(v.t.key)){ this.loopStop(k); continue; }
      const p=this.posOf(v.t);
      try{ v.pan.positionX.value=p[0]-o[0]; v.pan.positionY.value=p[1]-o[1]; v.pan.positionZ.value=p[2]; }catch(_){ }
    }
  },

  /* ---- the run ----------------------------------------------------------------------------- */

  say(script, kind, text){
    // The same line again straight after (a script doing one thing every frame): counted, not repeated.
    const last=this.log[this.log.length-1];
    if(last && last.script===(script||'') && last.kind===kind && last.text===text){ last.n=(last.n||1)+1; last.t=Date.now(); this.drawLog(); return; }
    this.log.push({t:Date.now(), script:script||'', kind, text});
    if(this.log.length>400) this.log.splice(0, this.log.length-400);
    this.drawLog();
  },
  unknown(script, fn){
    const k=script+'|'+fn;
    if(this._logged.has(k)) return;
    this._logged.add(k);
    this.say(script, 'skip', fn+' is not run in the preview');
  },
  start(id){
    const k=String(id).toLowerCase();
    if(this.globalRun.has(k)) return;
    this.globalRun.set(k, {script:k, locals:null, key:'global:'+k});
    if(!this.parsed.has(k)) this.fetch([k]);
  },
  async fetch(names){
    const want=names.filter(n=>!this.parsed.has(n));
    if(!want.length) return;
    for(const n of want) this.parsed.set(n, {error:'loading', locals:[], body:[]});
    let got={};
    // WGTB (`script_texts`): `scripts` [name, id, text] - back into {name: {id, text}}.
    try{ for(const [k,id,text] of unpackTables(await Engine.bytes('script_texts',{names:want})).scripts||[]) got[k]={id, text}; }catch(_){ }
    for(const n of want) this.learn(n, got && got[n]);
  },
  learn(k, s){
    if(!s){ this.parsed.set(k, {error:'not in the load order', locals:[], body:[]}); this.say(k, 'warn', 'the script '+k+' is not in the load order'); return; }
    try{ const p=this.parse(s.text); p.id=s.id; this.parsed.set(k, p); }
    catch(e){ this.parsed.set(k, {error:String(e && e.message || e), locals:[], body:[]}); this.say(k, 'error', 'did not parse: '+String(e && e.message || e)); }
  },
  /** The load order's globals at their start values, when none are known yet (the
   *  Dialogue window's In game tab checks conditions on them, the scripts not running). */
  /** A save's game: its globals, its journal and what the player carries, into the game
   *  state (the scripts, OpenMW Lua's quests and the dialogue rehearsal read them). Answers
   *  what the save holds (`dialogue_state_from_save`), null when none was read. */
  async fromSave(){
    let p;
    try{ p=await Engine.pick('open',{filters:[{name:'OpenMW saves', extensions:['omwsave']}]}); }catch(_){ return null; }
    if(!p) return null;
    let g;
    try{ g=this.readSave(unpackTables(await Engine.bytes('dialogue_state_from_save',{path:String(p)}))); }
    catch(e){ toast(String(e.message||e),'err',6000); return null; }
    const S=this.state;
    for(const x of g.globals||[]) S.globals.set(x.name.toLowerCase(), {name:x.name, kind:x.kind, v:x.value});
    S.journal.clear();
    for(const [q, i] of g.journal||[]) S.journal.set(String(q).toLowerCase(), i);
    const A=this.actorOf({player:true});
    if(A && g.inventory){ A.items.clear(); for(const [id, n] of g.inventory) A.items.set(String(id).toLowerCase(), n); }
    this.drawState(); this.drawPlayer && this.drawPlayer();
    this.say('', 'do', 'from '+(g.name||'a save')+': '+(g.globals||[]).length+' globals, '+(g.journal||[]).length+' quests, '+(g.inventory||[]).length+' kinds of item');
    return g;
  },
  async loadGlobals(){
    if(this.state.globals.size) return;
    let got;
    try{ got=this.readScene(unpackTables(await Engine.bytes('scene_scripts',{ids:[]}))); }catch(_){ return; }
    for(const g of (got && got.globals)||[]){ const k=g.name.toLowerCase(); if(!this.state.globals.has(k)) this.state.globals.set(k, {name:g.name, kind:g.kind, v:g.value}); }
  },
  /** `dialogue_state_from_save`'s tables (WGTB, commands) as the object its JSON was: {name,
   *  sex, race, class, level, factions, globals:[{name, kind, value}], journal, inventory}. */
  readSave(tb){
    const [name,sex,race,cls,level]=(tb.save||[])[0]||['','','','',0];
    return {name, sex, race, class:cls, level, factions:tb.factions||[],
            globals:(tb.globals||[]).map(([name,kind,value])=>({name, kind, value})), journal:tb.journal||[], inventory:tb.inventory||[]};
  },
  /** `scene_scripts`' tables (WGTB, commands) as the object its JSON was: {objects:{id:
   *  script}, scripts:{name:{id, text}}, start, globals:[{name, kind, value}], actors:{id:
   *  {tag, items:[[item, count]], stats:[...] | null}}}. */
  readScene(tb){
    const actors={}, rows=(tb.actors||[]).map(([id,tag,has])=>(actors[id]={tag, items:[], stats:has? [] : null}));
    for(const [row,item,n] of tb.actor_items||[]) rows[row].items.push([item,n]);
    for(const [row,v] of tb.actor_stats||[]) rows[row].stats.push(v);
    // Wraithguard: the AI ratings, [hello, fight, flee, alarm] (77_wg_ai.js).
    for(const [row,h,f,fl,al] of tb.actor_rates||[]) if(rows[row]) rows[row].rates=[h,f,fl,al];
    // Its own AI packages, in order: [kind, args] (MWScript's Ai* arguments, `|` between).
    for(const [row,kind,args] of tb.actor_packages||[]) if(rows[row]) (rows[row].packages=rows[row].packages||[]).push([kind, String(args).split('|')]);
    return {objects:Object.fromEntries(tb.objects||[]), scripts:Object.fromEntries((tb.scripts||[]).map(([k,id,text])=>[k,{id,text}])),
            start:(tb.start||[]).map(r=>r[0]), globals:(tb.globals||[]).map(([name,kind,value])=>({name, kind, value})), actors};
  },
  /** The scene's scripted objects, read again when the scene changes (state kept by key). */
  async gather(){
    const sc=App._scene; if(!sc) return;
    this._sceneSeen=sc;
    this._sceneKey=(sc.cells||[]).map(c=>c.kind==='int'? 'i:'+c.name : c.gx+','+c.gy).join(';');
    const refs=[];
    for(const c of sc.cells||[]) for(const r of c.refs||[]) if(r && r.id && !r.gc) refs.push([c, r]);
    // Only the ids not asked about yet (the scene is built again often while scripts move things).
    this._asked=this._asked||new Set();
    const ids=[...new Set(refs.map(([,r])=>String(r.id).toLowerCase()))].filter(id=>!this._asked.has(id));
    for(const id of ids) this._asked.add(id);
    let got={objects:{}, scripts:{}, start:[], globals:[]};
    if(ids.length || !this._globalsRead) try{ got=this.readScene(unpackTables(await Engine.bytes('scene_scripts',{ids}))); }
    catch(e){ for(const id of ids) this._asked.delete(id); this.say('', 'error', 'the scene\'s scripts did not load: '+String(e && e.message || e)); return; }
    if(!this.on) return;
    /* What came back is kept even when the scene was built again meanwhile (the scripts,
       objects and actors are by id, not by scene): those ids are marked asked, so a gathering
       of the newer scene does not ask again, and dropping this answer left a big load order's
       scene with no object scripts and no actor - none of them moved. */
    for(const [k, s] of Object.entries(got.scripts||{})) if(!this.parsed.has(k) || this.parsed.get(k).error==='loading') this.learn(k, s);
    for(const [id, s] of Object.entries(got.objects||{})) this.objScript.set(id, s);
    for(const [id, d] of Object.entries(got.actors||{})) this.actorDefs.set(id, d);
    if(!this._globalsRead){
      this._globalsRead=true;
      for(const g of got.globals||[]){ const k=g.name.toLowerCase(); if(!this.state.globals.has(k)) this.state.globals.set(k, {name:g.name, kind:g.kind, v:g.value}); }
      for(const s of got.start||[]) this.start(s);
    }
    this.bindScene();
  },
  /** An actor the AI walks, its walk mesh asked for ahead (18_cellpreview.js
   *  `previewWarmActor`), one at a time behind the others: built when it first walked, it
   *  glided there meanwhile. The AI's walk begins at 0 (`loop`), so it is the very mesh. */
  warm(key){
    if(typeof previewWarmActor!=='function') return;
    // Not drawn live yet (the AI takes them up before the scene is drawn with them live):
    // kept until it is (`keepLive`).
    if(!(App._liveRefs && App._liveRefs.has(key))){ (this._warmWant=this._warmWant||new Set()).add(key); return; }
    this._warmQ=(this._warmQ||Promise.resolve()).then(async()=>{
      if(!this.on) return false;
      // The very mesh its walk will be: at its pace against the walk's velocity (77_wg_ai.js `playRate`).
      const A=this.actors.get(key), model=this.modelOfKey(key);
      await this.groupsOf(model);
      const pace=(A && typeof WgAI==='object')? WgAI.speeds(A).walk : 0;
      const rate=typeof WgAI==='object'? WgAI.playRate(model, 'WalkForward', pace) : 1;
      return previewWarmActor(key, 'WalkForward@0'+(rate!==1? '*'+rate : ''));
    }).catch(()=>false);
  },
  /** The current scene's references to what is known of them: an instance for each scripted
   *  one, and each actor's own AI packages started. Run after each gathering, whichever scene
   *  it asked about. */
  bindScene(){
    const sc=App._scene; if(!sc || !this.on) return;
    const refs=[];
    for(const c of sc.cells||[]) for(const r of c.refs||[]) if(r && r.id && !r.gc) refs.push([c, r]);
    for(const [c, r] of refs){
      const id=String(r.id).toLowerCase(), s=this.objScript.get(id);
      if(!s || this.inst.has(r.key)) continue;
      this.inst.set(r.key, {key:r.key, id, script:s, locals:null, cell:c.kind==='int'? String(c.name||'').toLowerCase() : c.gx+','+c.gy,
        base:{pos:(r.pos||[0,0,0]).slice(), rot:(r.rot||[0,0,0]).slice()}, _ref:r});
    }
    /* The actors' own AI packages (their records' AI_W, AI_T, AI_F, AI_E, AI_A): the first
       started on each actor of the scene that has any, the rest after it in order - which is
       what has a town's people walk about. Once per reference; a script's own Ai* replaces it. */
    this._packed=this._packed||new Set();
    const started={};
    for(const [, r] of refs){
      if(this._packed.has(r.key) || this.disabled.has(r.key)) continue;
      const d=this.actorDefs.get(String(r.id).toLowerCase());
      if(!d || !d.packages || !d.packages.length) continue;
      this._packed.add(r.key);
      const t=this.inst.get(r.key) || {key:r.key, id:String(r.id).toLowerCase(), ref:r};
      const A=this.actorOf(t, true); if(!A || A.ai) continue;
      A.packages=d.packages; A.pkg=0;
      this.ai(t, A.packages[0][0], A.packages[0][1], {self:t}, true);
      if(A.ai){ const k=A.ai.kind+(A.ai.kind==='wander'? (A.ai.range>0? ' with a distance' : ' in place') : ''); started[k]=(started[k]||0)+1; }
      if(A.ai && !(A.ai.kind==='wander' && !(A.ai.range>0))) this.warm(r.key);
    }
    // Said once a gathering: how many actors the AI took up, and how (the panel's Actors list has each).
    const total=Object.values(started).reduce((a, b)=>a+b, 0);
    if(total) this.say('', 'do', 'AI: '+total+' actor'+(total===1? '' : 's')+' on their own packages ('+Object.entries(started).map(([k, n])=>n+' '+k).join(', ')+')');
    else if(!this._aiSaid && [...this.actorDefs.values()].some(d=>d.tag==='NPC_' || d.tag==='CREA') && ![...this.actorDefs.values()].some(d=>d.packages)){
      this._aiSaid=true;
      this.say('', 'warn', 'AI: no actor in the scene came with an AI package (an engine older than this page?)');
    }
    this.drawState(); this.drawWhich();
    if(typeof WgAI==='object') WgAI.load();
  },
  loopRun(){
    cancelAnimationFrame(this._raf);
    const step=now=>{
      if(!this.on){ this._raf=0; return; }
      this._raf=requestAnimationFrame(step);
      const dt=Math.min(0.1, Math.max(0, (now-(this._last||now))/1000)); this._last=now;
      if(App._scene!==this._sceneSeen && App._scene && performance.now()-this._lastRebuild>300) this.gather().then(()=>{ if(typeof WgLua==='object') WgLua.gather(); });
      if(!this.paused && dt>0) this.frame(dt);
      this._uiT=(this._uiT||0)+dt; if(this._uiT>0.5){ this._uiT=0; this.drawLive(); }
    };
    this._raf=requestAnimationFrame(step);
  },
  setOn(on){
    this.on=!!on;
    const b=$('#tbScripts'); if(b) b.classList.toggle('on', this.on);
    if(this.on){
      // The scene is kept: its actors already draw on their own (18_cellpreview.js `live`).
      this.panel(); this._el.hidden=false; this.gather().then(()=>this.startLua()); this._last=0; this.loopRun();
    }
    else this.reset(true);
  },
  /** OpenMW Lua beside MWScript: started once the scene's objects are known. */
  async startLua(){
    if(typeof WgLua!=='object' || !this.on) return;
    await WgLua.start();
    this.drawLive();
  },
  /** Back to the scene as the load order has it: every change the scripts made undone, their
   *  locals and the started scripts forgotten. */
  reset(off){
    /* The scene is kept when only actors moved or played: each is put back where the load order
       has it, with its own idle (18_cellpreview.js `previewMoveRef`, `previewSwapActor`). What
       else the scripts changed - an object turned off, one placed - needs the scene built again. */
    const rebuild=this.disabled.size>0 || this.placed.length>0;
    if(App._liveRefs && typeof previewMoveRef==='function'){
      for(const k of new Set([...this.moved.keys(), ...this.anims.keys()])){
        if(!App._liveRefs.has(k)) continue;
        const r=this.refByKey(k);
        if(r && r.pos) previewMoveRef(k, r.pos, r.rot||[0,0,0], r.scale||1);
        if(typeof previewSwapActor==='function') previewSwapActor(k, null).catch(()=>{});
      }
    }
    if(rebuild) App._scene=null;
    this.disabled.clear(); this.moved.clear(); this.anims.clear(); this.placed=[]; this.inst.clear(); this.globalRun.clear();
    for(const k of [...this.loops.keys()]) this.loopStop(k);
    this._globalsRead=false; this._asked=null; this._packed=null; this.objScript.clear(); this.state.globals.clear(); this.state.journal.clear(); this._logged.clear(); this.log=[];
    this.actors.clear(); this.actorDefs.clear(); this.playing.clear(); this.topics.clear(); this.msg=null; this.drawMsg();
    this.fade(0, 0);
    if(typeof WgLua==='object') WgLua.stop();
    if(off){ cancelAnimationFrame(this._raf); this._raf=0; if(this._el) this._el.hidden=true; }
    this._sceneSeen=null;
    if(rebuild && typeof schedulePreview==='function') schedulePreview();
    this.drawState(); this.drawLog();
  },

  /* ---- the panel --------------------------------------------------------------------------- */

  wire(){
    const b=$('#tbScripts'); if(!b || b._wired) return;
    b._wired=true; b.onclick=()=>this.setOn(!this.on);
  },
  panel(){
    if(this._el) return;
    const d=document.createElement('div');
    d.id='wgScrPanel'; d.className='ori'; d.hidden=true;
    d.dataset.anywhere='';   // the Preview's own: shown docked outside the Editor too (51_wg_editor_ui.js)
    d.innerHTML='<div class="orihead"><b>Scripts</b><span style="flex:1"></span>'+
      '<button class="btn sm" id="wgScrPause" title="Stop the scripts where they are">Pause</button>'+
      '<button class="btn sm" id="wgScrStep" title="One frame of every script">Step</button>'+
      '<button class="btn sm" id="wgScrReset" title="Undo what the scripts did and start them again">Reset</button>'+
      '<button class="btn dim ic" id="wgScrX" title="Stop the scripts">&#x2715;</button></div>'+
      '<div class="oribody"><div class="hint">The scene\'s MWScript and the load order\'s OpenMW Lua run as the game runs them, against the state below; the hour is the Preview\'s clock and the weather its weather. Activate an object: <b>Ctrl+click</b> it.</div>'+
      '<div id="wgScrMsg" hidden></div>'+
      '<div class="orisec">Game state</div><div id="wgScrState"></div>'+
      '<div class="orisec">Running</div><div id="wgScrLive" class="hint"></div>'+
      '<div class="orisec">Which scripts run</div><div id="wgScrWhich"></div>'+
      '<div class="orisec">The player</div><div id="wgScrPlayer"></div>'+
      '<div class="orisec">Actors</div><div id="wgScrActors" class="hint"></div>'+
      '<div class="orisec">What they did <button class="btn dim sm" id="wgScrClear">Clear</button></div><div id="wgScrLog" style="max-height:220px;overflow:auto;font-family:var(--mono);font-size:11px"></div></div>';
    if(typeof WgUI==='object' && WgUI.mount) WgUI.mount(d); else ($('#vpwrap')||document.body).appendChild(d);
    this._el=d;
    d.querySelector('#wgScrX').onclick=()=>this.setOn(false);
    d.querySelector('#wgScrPause').onclick=e=>{ this.paused=!this.paused; e.target.classList.toggle('pri', this.paused); e.target.textContent=this.paused? 'Resume' : 'Pause'; };
    d.querySelector('#wgScrStep').onclick=()=>{ this.frame(1/30); this.drawLive(); };
    d.querySelector('#wgScrReset').onclick=()=>{ this.reset(false); this.gather().then(()=>this.startLua()); };
    d.querySelector('#wgScrClear').onclick=()=>{ this.log=[]; this.drawLog(); };
    this.drawWhich();
    // Ctrl+click on an object activates it (OnActivate is true for its script's next frame).
    const R=App.R;
    if(R && R.cv && !this._actWired){
      this._actWired=true;
      R.cv.addEventListener('pointerdown', e=>{
        if(!this.on || !e.ctrlKey || e.button!==0) return;
        const hit=R.pickAt? R.pickAt(e.clientX, e.clientY) : null;
        const it=hit && hit.refKey? this.inst.get(hit.refKey) : null;
        if(it){ it._activate=true; this.say(it.script, 'do', 'activated ('+it.id+')'); }
        if(hit && hit.refKey && typeof WgLua==='object' && WgLua.running) WgLua.activate(hit.refKey);
      }, true);
    }
    this.drawState();
  },
  /** Which scripts run: a switch for each of the scene's MWScripts and the load order's
   *  Lua scripts (and one for OpenMW's built-ins), a filter over both, and All/None for what
   *  the filter shows. A Lua change starts the Lua again; an MWScript change is at once. */
  async drawWhich(force){
    const box=this._el && this._el.querySelector('#wgScrWhich'); if(!box) return;
    if(!force && box.contains(document.activeElement) && document.activeElement.id==='wgWhichQ') return;   // not under the person typing
    const lua=typeof WgLua==='object'? await WgLua.scriptList() : [];
    const mw=[...new Set([...[...this.inst.values()].map(i=>String(i.script||'').toLowerCase()), ...[...this.globalRun.keys()]].filter(Boolean))].sort();
    const q=this._whichQ.toLowerCase();
    const show=t=>!q || t.toLowerCase().includes(q);
    const row=(kind, key, label, on, sub)=>'<label class="wgWhich" style="display:flex;gap:6px;align-items:center;font-size:12px"><input type="checkbox" data-kind="'+kind+'" data-key="'+escHtml(key)+'"'+(on? ' checked' : '')+'>'+
      '<span style="font-family:var(--mono);overflow:hidden;text-overflow:ellipsis;white-space:nowrap" title="'+escHtml(key)+'">'+escHtml(label)+'</span>'+(sub? '<span class="hint">'+escHtml(sub)+'</span>' : '')+'</label>';
    let h='<div style="display:flex;gap:4px;margin-bottom:4px"><input id="wgWhichQ" class="inp" placeholder="Filter scripts" style="flex:1" value="'+escHtml(this._whichQ)+'">'+
      '<button class="btn sm" id="wgWhichAll" title="Run every script the filter shows">All</button><button class="btn sm" id="wgWhichNone" title="Run none of the scripts the filter shows">None</button>'+
      '<button class="btn sm'+(this.whichByFile()? ' pri' : '')+'" id="wgWhichByFile" title="Turn Lua mods on and off by the .omwscripts (or .omwaddon) file that registers them, or one script at a time">'+(this.whichByFile()? 'By file' : 'By script')+'</button></div>';
    const luaRows=lua.filter(r=>show(r.path));
    const bi=typeof WgLua==='object' && !WgLua.isOff('@builtins');
    h+='<div class="hint">OpenMW Lua</div>';
    if(show('openmw built-ins @builtins')) h+=row('lua', '@builtins', 'OpenMW\'s built-ins', bi, 'I.AI, activation, music, combat');
    // MWSE mods (MWSE/mods/**/main.lua), all at once: their events, timers and core tes3 calls run on the scene.
    if(show('mwse mods @mwse')) h+=row('lua', '@mwse', 'MWSE mods', typeof WgLua==='object' && !WgLua.isOff('@mwse'), 'main.lua: events, timers, core tes3');
    /* By file (Static 2026-11-23: "can openmw lua mods be selected by its omwscripts file not the
       individual lua files as an alternative mode?"): one switch a content file that registers
       scripts, turning all of them on or off together; part-on shows as neither. */
    const byFile=this.whichByFile(), files=new Map();
    if(byFile){
      for(const r of lua) for(const f of (r.files&&r.files.length? r.files : ['(registered elsewhere)'])){ if(!files.has(f)) files.set(f, []); files.get(f).push(r.path); }
      this._whichFiles=files;
      h+=[...files.entries()].filter(([f, ps])=>show(f) || ps.some(show)).map(([f, ps])=>{
        const on=ps.filter(p=>!WgLua.isOff(p)).length;
        return row('luafile', f, f, on===ps.length, ps.length+' script'+(ps.length===1? '' : 's')+(on && on<ps.length? ', '+on+' on' : ''));
      }).join('');
    }
    else h+=luaRows.map(r=>row('lua', r.path, r.path.replace(/^scripts\//i, ''), !WgLua.isOff(r.path), r.flags.join(' '))).join('');
    if(!lua.length) h+='<div class="hint">'+escHtml((typeof WgLua==='object' && WgLua._note) || 'No registered Lua scripts.')+'</div>';
    h+='<div class="hint" style="margin-top:4px">MWScript (this scene)</div>';
    const mwRows=mw.filter(show);
    h+=mwRows.map(k=>row('mw', k, k, !this.mwOff.has(k), '')).join('') || '<div class="hint">None in the scene.</div>';
    box.innerHTML=h;
    const qi=box.querySelector('#wgWhichQ');
    qi.oninput=()=>{ this._whichQ=qi.value; clearTimeout(this._whichT); this._whichT=setTimeout(()=>this.drawWhich(true).then(()=>{ const n=box.querySelector('#wgWhichQ'); if(n){ n.focus(); n.setSelectionRange(n.value.length, n.value.length); } }), 150); };
    // A file part-on is shown as neither.
    if(byFile) box.querySelectorAll('input[data-kind=luafile]').forEach(cb=>{ const ps=files.get(cb.dataset.key)||[]; const on=ps.filter(p=>!WgLua.isOff(p)).length; cb.indeterminate=on>0 && on<ps.length; });
    const set=(kind, keys, on)=>{
      if(!keys.length) return;
      // A file stands for the scripts it registers.
      if(kind==='luafile'){ const ps=[].concat(...keys.map(k=>files.get(k)||[])); if(ps.length) WgLua.setOff(ps, !on); this.say('', 'do', (on? 'turned on ' : 'turned off ')+(keys.length===1? keys[0] : keys.length+' files')+' ('+ps.length+' script'+(ps.length===1? '' : 's')+')'+(WgLua.running? ' (the Lua starts again)' : '')); return; }
      if(kind==='lua') WgLua.setOff(keys, !on);
      else{ for(const k of keys){ if(on) this.mwOff.delete(k); else this.mwOff.add(k); } try{ localStorage.setItem('wgMwOff', JSON.stringify([...this.mwOff])); }catch(_){ } }
      this.say('', 'do', (on? 'turned on ' : 'turned off ')+(keys.length===1? keys[0].replace('@builtins', 'OpenMW\'s built-ins') : keys.length+' scripts')+(kind==='lua' && WgLua.running? ' (the Lua starts again)' : ''));
    };
    box.querySelectorAll('input[type=checkbox]').forEach(cb=>cb.onchange=()=>{ set(cb.dataset.kind, [cb.dataset.key], cb.checked); if(cb.dataset.kind==='luafile') this.drawWhich(true); });
    const all=on=>{
      const shownLua=[...box.querySelectorAll('input[data-kind=lua]')].map(c=>c.dataset.key);
      const shownFiles=[...box.querySelectorAll('input[data-kind=luafile]')].map(c=>c.dataset.key);
      const shownMw=[...box.querySelectorAll('input[data-kind=mw]')].map(c=>c.dataset.key);
      set('lua', shownLua, on); set('luafile', shownFiles, on); set('mw', shownMw, on); this.drawWhich(true);
    };
    box.querySelector('#wgWhichAll').onclick=()=>all(true);
    box.querySelector('#wgWhichNone').onclick=()=>all(false);
    box.querySelector('#wgWhichByFile').onclick=()=>{ try{ localStorage.setItem('wgWhichByFile', this.whichByFile()? '0' : '1'); }catch(_){ } this.drawWhich(true); };
  },
  /** Whether Which scripts run lists the Lua mods by the files that register them (kept). */
  whichByFile(){ try{ return localStorage.getItem('wgWhichByFile')==='1'; }catch(_){ return false; } },
  /** The game state drawn again - gathered up, at most every 400 ms, and held while the
   *  pointer is over it (Static, 2026-11-19: "part above journal below globals wont scroll"):
   *  a script that does something every frame redrew it every frame, and a list rebuilt under
   *  the wheel never scrolls. Drawn once the pointer leaves. */
  drawState(){
    if(typeof setTimeout!=='function'){ this.drawStateNow(); return; }   // no clock to wait on
    this._stateDue=true;
    if(this._stateT) return;
    const go=()=>{
      this._stateT=0;
      if(!this._stateDue) return;
      const box=this._el && this._el.querySelector('#wgScrState');
      if(box && box.matches && box.matches(':hover')){ this._stateT=setTimeout(go, 400); return; }
      this._stateDue=false;
      this.drawStateNow();
    };
    const since=performance.now()-(this._stateAt||0);
    if(since>=400){ this._stateAt=performance.now(); go(); }
    else this._stateT=setTimeout(()=>{ this._stateAt=performance.now(); go(); }, 400-since);
  },
  drawStateNow(){
    const box=this._el && this._el.querySelector('#wgScrState'); if(!box) return;
    if(box.contains(document.activeElement)) return;   // not under the person typing
    /* Drawn again as the scripts change things: the globals list keeps where it was
       scrolled and what it was filtered by (Static, 2026-11-03: "cant scroll down the
       first inner window it resets to top when I try"). */
    const oldG=box.querySelector('#wgScrGlobals'), keepTop=oldG? oldG.scrollTop : 0;
    const oldF=box.querySelector('#wgScrGF'), keepF=oldF? oldF.value : '';
    const S=this.state, row=(label, input)=>'<div class="orirow"><span class="k">'+label+'</span><span class="v">'+input+'</span></div>';
    let h=row('Day / month / year','<input class="fld" type="number" data-st="day" value="'+S.day+'" style="width:52px"> <input class="fld" type="number" data-st="month" value="'+S.month+'" style="width:44px"> <input class="fld" type="number" data-st="year" value="'+S.year+'" style="width:60px">');
    h+='<div class="orirow"><span class="k">Game</span><span class="v"><button class="btn sm" id="wgScrSave" title="A save\'s globals, journal and what the player carries">From a save…</button></span></div>';
    h+='<div class="orirow"><span class="k">Globals</span><span class="v"><input class="fld" id="wgScrGF" placeholder="Filter" style="width:100%"></span></div><div id="wgScrGlobals" style="max-height:160px;overflow:auto"></div>';
    h+='<div class="orirow"><span class="k">Journal</span><span class="v"><input class="fld" id="wgScrJId" placeholder="Quest id" style="width:110px"> <input class="fld" type="number" id="wgScrJIx" placeholder="Index" style="width:60px"> <button class="btn sm" id="wgScrJSet">Set</button></span></div>';
    h+='<div class="hint">'+[...S.journal].map(([k,v])=>escHtml(k)+' '+v).join(' · ')+'</div>';
    if(this.topics.size) h+=row('Topics added', '<span class="hint">'+escHtml([...this.topics].join(', '))+'</span>');
    box.innerHTML=h;
    this.drawPlayer();
    box.querySelectorAll('[data-st]').forEach(i=>i.onchange=()=>{ S[i.dataset.st]=Math.trunc(+i.value||0); });
    box.querySelector('#wgScrSave').onclick=()=>this.fromSave();
    const drawG=()=>{
      const f=(box.querySelector('#wgScrGF').value||'').toLowerCase();
      const list=[...S.globals.entries()].filter(([k])=>!f || k.includes(f)).sort((a,b)=>a[0]<b[0]?-1:1).slice(0, 200);
      const g=box.querySelector('#wgScrGlobals');
      g.innerHTML=list.map(([k,v])=>'<div class="orirow"><span class="k" title="'+(v.kind==='f'? 'float' : v.kind==='l'? 'long' : 'short')+'">'+escHtml(v.name)+'</span><span class="v"><input class="fld" type="number" step="any" data-g="'+escHtml(k)+'" value="'+Number(v.v)+'" style="width:90px"></span></div>').join('')||'<div class="hint">No global matches.</div>';
      g.querySelectorAll('[data-g]').forEach(i=>i.onchange=()=>{ const v=S.globals.get(i.dataset.g); if(v) v.v=v.kind==='f'? +i.value||0 : Math.trunc(+i.value||0); });
    };
    box.querySelector('#wgScrGF').value=keepF;
    box.querySelector('#wgScrGF').oninput=drawG;
    box.querySelector('#wgScrJSet').onclick=()=>{
      const id=(box.querySelector('#wgScrJId').value||'').trim().toLowerCase(); if(!id) return;
      S.journal.set(id, Math.trunc(+box.querySelector('#wgScrJIx').value||0)); this.drawStateNow();
    };
    drawG();
    box.querySelector('#wgScrGlobals').scrollTop=keepTop;
  },
  drawLive(){
    const box=this._el && this._el.querySelector('#wgScrLive'); if(!box) return;
    const by=new Map();
    for(const it of this.inst.values()) by.set(it.script, (by.get(it.script)||0)+1);
    const bad=[...this.parsed].filter(([,p])=>p.error && p.error!=='loading').map(([k])=>k);
    box.innerHTML=(this.inst.size+' object'+(this.inst.size===1?'':'s')+' with scripts, '+this.globalRun.size+' global script'+(this.globalRun.size===1?'':'s')+
      ' ('+escHtml([...this.globalRun.keys()].slice(0, 8).join(', '))+(this.globalRun.size>8? '…' : '')+'). ')+
      (this.disabled.size? this.disabled.size+' disabled. ' : '')+(this.moved.size? this.moved.size+' moved. ' : '')+(this.placed.length? this.placed.length+' placed. ' : '')+
      (bad.length? '<br>Not run: '+escHtml(bad.slice(0, 6).join(', ')) : '');
    if(typeof WgLua==='object'){
      const st=WgLua.status(), n=x=>x+' ';
      box.innerHTML+='<br>'+(st? 'OpenMW Lua on '+escHtml(WgLua.backend.replace(/ - .*/, ''))+': '+n(st.global)+'global, '+n(st.player)+'player and '+st['local']+' object script'+(st['local']===1?'':'s')+(st.mwse? ', '+st.mwse+' MWSE mod'+(st.mwse===1?'':'s') : '')+(st.timers? ', '+st.timers+' timer'+(st.timers===1?'':'s')+' waiting' : '')+'.'+
        (st.failing && st.failing.length? ' Failing: '+escHtml([].concat(st.failing).slice(0, 6).join(', ')) : '')+(WgLua.note? ' '+escHtml(WgLua.note) : '') :
        'OpenMW Lua: '+escHtml(WgLua.note||(WgLua.running? 'starting…' : 'not running.')));
      let dir=''; try{ dir=localStorage.getItem('wgOpenMWDir')||''; }catch(_){ }
      box.innerHTML+=' <button class="btn sm" id="wgScrOmw" title="The OpenMW folder (the one with openmw.exe): its LuaJIT runs the scripts and its built-in scripts (AI, activation, music, combat) run with them'+(dir? '. Now: '+escHtml(dir) : '')+'">OpenMW folder…</button>'+
        (WgLua.resources? ' <span class="hint">Built-ins from '+escHtml(WgLua.resources)+'</span>' : '');
      const ob=box.querySelector('#wgScrOmw'); if(ob) ob.onclick=()=>WgLua.chooseOpenMW();
    }
    const ab=this._el.querySelector('#wgScrActors');
    if(ab){
      const AIN={wander:'wandering', travel:'traveling', follow:'following', escort:'escorting', activate:'going to activate'};
      // Those with an AI package first, nearest the camera first, with what each is doing now.
      const cam=this.posOf({player:true}), far=A=>{ const p=this.posOf(A.t); return Math.hypot(p[0]-cam[0], p[1]-cam[1]); };
      const all=[...this.actors.values()].filter(A=>A.key!=='player');
      all.sort((a, b)=>(!!b.ai-!!a.ai) || (far(a)-far(b)));
      const rows=all.slice(0, 40).map(A=>{
        let what=A.dead? 'dead' : A.combat? 'fighting '+(A.combat.player? 'the player' : A.combat.id) : A.ai && !A.ai.done? AIN[A.ai.kind]+(A.ai.who? ' '+(A.ai.who.player? 'the player' : A.ai.who.id) : '') : A.ai? A.ai.kind+' done' : '';
        if(A.ai && !A.ai.done && A.ai.kind==='wander') what+=' '+Math.round(A.ai.range)+(A._state? ' ('+A._state+')' : '');
        if(A.ai){ const r=this.refOf(A.t), p=this.posOf(A.t); if(r && r.pos) what+=', moved '+Math.round(Math.hypot(p[0]-r.pos[0], p[1]-r.pos[1]))+(App._liveRefs && App._liveRefs.has(A.key)? '' : ' (not drawn live)'); }
        return '<div>'+escHtml(A.id)+(A.tag==='CONT'? '' : ': '+Math.max(0, Math.round(A.hp))+'/'+Math.round(A.max[0])+' health')+(what? ', '+escHtml(what) : '')+
          (A.anim? ', playing '+escHtml(A.anim.group) : '')+(A.spells && A.spells.size? ', '+A.spells.size+' spell'+(A.spells.size===1? '' : 's') : '')+(A.items.size? ', carries '+A.items.size+' kinds' : '')+'</div>';
      });
      ab.innerHTML=rows.join('')||'None yet: an actor is listed once a script asks about it.';
    }
    const pa=this.actors.get('player');
    if(pa && this._el.querySelector('#wgScrPlayer [data-pf="hp"]') && Math.round(+this._el.querySelector('#wgScrPlayer [data-pf="hp"]').value)!==Math.round(pa.hp)) this.drawPlayer();
  },
  /** The player's health and what they carry: what `GetItemCount` and combat read. */
  drawPlayer(){
    const box=this._el && this._el.querySelector('#wgScrPlayer'); if(!box || box.contains(document.activeElement)) return;
    const A=this.actorOf({player:true});
    box.innerHTML='<div class="orirow"><span class="k">Health / magicka / fatigue</span><span class="v">'+
      ['hp','mp','fp'].map(f=>'<input class="fld" type="number" data-pf="'+f+'" value="'+Math.round(A[f])+'" style="width:52px">').join(' ')+'</span></div>'+
      '<div class="orirow"><span class="k">Carries</span><span class="v"><input class="fld" id="wgScrPItem" placeholder="Item id" style="width:110px"> <input class="fld" type="number" id="wgScrPN" value="1" style="width:52px"> <button class="btn sm" id="wgScrPAdd">Set</button></span></div>'+
      '<div class="hint">'+([...A.items].map(([k, n])=>escHtml(k)+' ×'+n).join(' · ')||'Nothing.')+(A.dead? ' <b>Dead.</b>' : '')+'</div>';
    box.querySelectorAll('[data-pf]').forEach(i=>i.onchange=()=>{ A[i.dataset.pf]=+i.value||0; if(A.dead && A.hp>0) A.dead=false; });
    box.querySelector('#wgScrPAdd').onclick=()=>{
      const id=(box.querySelector('#wgScrPItem').value||'').trim().toLowerCase(); if(!id) return;
      const n=Math.max(0, Math.trunc(+box.querySelector('#wgScrPN').value||0));
      if(n) A.items.set(id, n); else A.items.delete(id);
      this.drawPlayer();
    };
  },
  /** A MessageBox's buttons, waiting for the person: the choice is that script's next
   *  GetButtonPressed. */
  drawMsg(){
    const box=this._el && this._el.querySelector('#wgScrMsg'); if(!box) return;
    const m=this.msg;
    box.hidden=!m;
    if(!m){ box.innerHTML=''; return; }
    box.innerHTML='<div class="orisec">'+escHtml(m.run.script)+' asks</div><div style="margin:4px 0">'+escHtml(m.text)+'</div>'+
      m.buttons.map((b, i)=>'<button class="btn sm'+(i? '' : ' pri')+'" data-b="'+i+'">'+escHtml(b)+'</button>').join(' ');
    box.querySelectorAll('[data-b]').forEach(b=>b.onclick=()=>this.choose(+b.dataset.b));
  },
  choose(i){
    const m=this.msg; if(!m) return;
    m.run._button=i; this.msg=null; this.drawMsg();
    this.say(m.run.script, 'do', 'chose '+m.buttons[i]);
  },
  /** The log drawn again as `drawState` is: gathered up, held while the pointer is over it. */
  drawLog(){
    if(typeof setTimeout!=='function'){ this.drawLogNow(); return; }
    this._logDue=true;
    if(this._logT) return;
    const go=()=>{
      this._logT=0;
      if(!this._logDue) return;
      const box=this._el && this._el.querySelector('#wgScrLog');
      if(box && box.matches && box.matches(':hover')){ this._logT=setTimeout(go, 400); return; }
      this._logDue=false; this._logAt=performance.now();
      this.drawLogNow();
    };
    const since=performance.now()-(this._logAt||0);
    if(since>=400) go(); else this._logT=setTimeout(go, 400-since);
  },
  drawLogNow(){
    const box=this._el && this._el.querySelector('#wgScrLog'); if(!box) return;
    const col={do:'', warn:'color:var(--warn)', error:'color:var(--bad,#e66)', skip:'opacity:0.6'};
    box.innerHTML=this.log.slice(-200).reverse().map(l=>'<div style="'+(col[l.kind]||'')+'">'+escHtml(l.script)+': '+escHtml(l.text)+(l.n>1? ' <span class="hint">×'+l.n+'</span>' : '')+'</div>').join('');
  },
};
