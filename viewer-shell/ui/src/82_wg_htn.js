/* =====================================================================================
   Wraithguard: HTN AI - Tools → HTN AI (Static 2026-11-23: "a tool/frontend that would make
   [OdaiHTN, now FluadHTN] usable, easier, automated, logical ... make the framework more
   accessible"; "for the first case, but with an advanced toggle for the second"; "depend";
   "OpenMW and MWSE"; "Game-action library"; "Editor + export + preview run").

   An actor's behaviour drawn as an HTN domain - Selects (the first branch that can be done),
   Sequences (every step in order), Actions and Pauses - from ready-made game blocks, saved as
   a `*.wghtn.json`, checked, and exported as a mod for OpenMW, MWSE or both that runs it on
   FluadHTN. FluadHTN itself is never carried: the exported mod `require`s the copy the player
   installed from its Nexus page (52111), and on OpenMW Rubic0n (58557), whose LuaJIT gives the
   OpenMW build the `table.size`, `table.new` and `table.removevalue` it calls. `htn_deps` says
   what the setup has.

   - The blocks (`ACTIONS`, `CHECKS`): each said in plain words, with its fields; each written
     as engine-neutral Lua over a small game layer (`G`) that each engine's export defines
     (`omwGame`, `mwseGame`), and played in the Preview by `simAct`/`simCheck`.
   - World state: named true/false or number values. A value can be "watched": set each tick
     from a game check (the player near, health low), which is how the plan notices the world
     change and plans again (FluadHTN plans again when a state changes).
   - Advanced: custom Lua for a check or an action, per engine; the effect kinds (plan and
     run, plan only, permanent); the generated Lua shown.
   - The Preview run: the domain planned and run by this page on an actor of the loaded
     cell while the Scripts run - its own planner, by FluadHTN's rules as its documentation
     and tests describe them (a Select takes the first branch that decomposes; a Sequence all
     of its steps, each step's effects applied to the plan's state as it goes; conditions
     checked again as each step starts; a plan replaced only by one that takes an earlier
     branch; plan-and-run effects applied when a step succeeds), close enough to design with.
     Custom Lua is not run there.

   Built with the DOM (names, text and Lua the person types go into the page as text).
   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
const WgHtn={
  FORMAT:'wraithguard-htn', VERSION:1,
  NEXUS_FLUADHTN:'https://www.nexusmods.com/morrowind/mods/52111',
  NEXUS_RUBIC0N:'https://www.nexusmods.com/morrowind/mods/58557',
  doc:null, sel:'', adv:false, path:'', deps:null, _n:0, run:null, _showLua:'',

  /* ---- the blocks ------------------------------------------------------------------------ */
  /* A field: [key, label, type, default]. Types: num, text, id (an object's record id),
     state (a world-state value), bool, group (an animation group), lua (Advanced). */
  ACTIONS:{
    wait:{label:'Wait', help:'Stand still for a while.', fields:[['seconds', 'Seconds', 'num', 5]]},
    wander:{label:'Wander', help:'Wander around where it stands, for a while.', fields:[['distance', 'Distance', 'num', 512], ['seconds', 'Seconds', 'num', 20]]},
    walk_to:{label:'Walk to a point', help:'Walk to a place in the world (game units). Fails after two minutes.', fields:[['x', 'X', 'num', 0], ['y', 'Y', 'num', 0], ['z', 'Z', 'num', 0]], here:true},
    walk_to_object:{label:'Walk to an object', help:'Walk up to the nearest loaded object with this record id. Fails when none is loaded.', fields:[['id', 'Object id', 'id', ''], ['near', 'Stop within', 'num', 128]]},
    go_to_player:{label:'Go to the player', help:'Walk up to the player.', fields:[['near', 'Stop within', 'num', 200]]},
    follow_player:{label:'Follow the player', help:'Follow the player for a while.', fields:[['seconds', 'Seconds', 'num', 30]]},
    flee_player:{label:'Run from the player', help:'Run away until it is this far from the player. Fails after 30 seconds.', fields:[['distance', 'Until', 'num', 1500]]},
    use_item:{label:'Use or equip an item', help:'Use an item it carries: equip a weapon or armour, drink a potion (as the game\'s Use does).', fields:[['id', 'Item id', 'id', '']]},
    attack_player:{label:'Attack the player', help:'Start a fight with the player (done at once; the fight goes on by the game\'s AI).', fields:[]},
    say:{label:'Show a message', help:'Show a message on the player\'s screen.', fields:[['text', 'Text', 'text', '']]},
    play_anim:{label:'Play an animation', help:'Play an animation group (Idle2 ... Idle9, a mod\'s own) for a while.', fields:[['group', 'Group', 'group', 'Idle2'], ['seconds', 'Seconds', 'num', 3]]},
    set_state:{label:'Set a value', help:'Set a world-state value, at once.', fields:[['var', 'Value', 'state', ''], ['value', 'To', 'val', 1]]},
    lua:{label:'Custom Lua', help:'Your own Lua, for each engine (Advanced). Start runs once; Update runs each tick and returns T.Continue, T.Success or T.Failure.', adv:true,
      fields:[['omw_start', 'OpenMW start', 'lua', ''], ['omw_update', 'OpenMW update', 'lua', 'return T.Success'], ['mwse_start', 'MWSE start', 'lua', ''], ['mwse_update', 'MWSE update', 'lua', 'return T.Success']]},
  },
  CHECKS:{
    state:{label:'A value is', help:'A world-state value compared.', fields:[['var', 'Value', 'state', ''], ['op', 'Is', 'op', '=='], ['value', 'To', 'val', 1]], plan:true},
    player_within:{label:'The player is near', help:'The player is within this distance (game units).', fields:[['distance', 'Distance', 'num', 512]]},
    health_below:{label:'Health is below', help:'Its health is below this percent of its most.', fields:[['percent', 'Percent', 'num', 30]]},
    in_combat:{label:'It is in a fight', help:'It is fighting.', fields:[]},
    hour_between:{label:'The hour is between', help:'The game hour is from the first up to the second (it can wrap past midnight: 20 to 6).', fields:[['from', 'From', 'num', 8], ['to', 'To', 'num', 20]]},
    has_item:{label:'It carries', help:'It carries at least this many of an item.', fields:[['id', 'Item id', 'id', ''], ['count', 'Count', 'num', 1]]},
    object_within:{label:'An object is near', help:'A loaded object with this record id is within this distance.', fields:[['id', 'Object id', 'id', ''], ['distance', 'Distance', 'num', 512]]},
    magicka_below:{label:'Magicka is below', help:'Its magicka is below this percent of its most.', fields:[['percent', 'Percent', 'num', 30]]},
    fatigue_below:{label:'Fatigue is below', help:'Its fatigue is below this percent of its most.', fields:[['percent', 'Percent', 'num', 30]]},
    player_health_below:{label:'The player\'s health is below', help:'The player\'s health is below this percent of its most.', fields:[['percent', 'Percent', 'num', 50]]},
    player_carries:{label:'The player carries', help:'The player carries at least this many of an item.', fields:[['id', 'Item id', 'id', ''], ['count', 'Count', 'num', 1]]},
    journal:{label:'A quest\'s stage', help:'A journal entry\'s index (stage) compared.', fields:[['quest', 'Quest id', 'id', ''], ['op', 'Is', 'op', '>='], ['value', 'Stage', 'num', 10]]},
    random_chance:{label:'A chance of', help:'True this percent of the times it is asked.', fields:[['percent', 'Percent', 'num', 50]]},
    lua:{label:'Custom Lua', help:'A Lua expression for each engine (Advanced): `a` is the actor, `ctx` the context, `G` the game layer.', adv:true,
      fields:[['omw', 'OpenMW', 'lua', 'true'], ['mwse', 'MWSE', 'lua', 'true']]},
  },
  OPS:['==', '~=', '<', '<=', '>', '>='],
  EFFECT_KINDS:[['run', 'Plan and run', 'Counted while planning, and set when the step succeeds'], ['plan', 'Plan only', 'Counted while planning; never set'], ['permanent', 'Permanent', 'Set as soon as the plan is made']],
  NODE_TYPES:{select:'Select', sequence:'Sequence', action:'Action', pause:'Pause plan', splice:'Use part'},

  /* ---- the domain ------------------------------------------------------------------------ */
  id(){ return 'n'+(++this._n).toString(36)+Date.now().toString(36).slice(-3); },
  defaults(fields){ const o={}; for(const [k, , , d] of fields) o[k]=d; return o; },
  node(type, name, more){
    const n={id:this.id(), type, name:name||this.NODE_TYPES[type], conds:[]};
    if(type==='select' || type==='sequence') n.children=[];
    if(type==='action'){ n.do={block:'wait', args:this.defaults(this.ACTIONS.wait.fields)}; n.effects=[]; n.exec=[]; }
    return Object.assign(n, more||{});
  },
  check(block, args, not){ return {block, args:Object.assign(this.defaults(this.CHECKS[block].fields), args||{}), not:!!not}; },
  act(block, args){ return {block, args:Object.assign(this.defaults(this.ACTIONS[block].fields), args||{})}; },
  /** A new domain: a guard who greets the player once, steps back when hurt, and wanders. */
  example(){
    const N=(t, name, more)=>this.node(t, name, more);
    return {format:this.FORMAT, version:this.VERSION, name:'Guard', modId:'wg_htn_guard', engines:{omw:true, mwse:true},
      attach:{ids:['imperial guard'], all:false}, tick:0.25,
      state:[
        {name:'PlayerNear', kind:'bool', init:0, sensor:this.check('player_within', {distance:600})},
        {name:'Hurt', kind:'bool', init:0, sensor:this.check('health_below', {percent:30})},
        {name:'Greeted', kind:'bool', init:0, sensor:null},
        {name:'Yielded', kind:'bool', init:0, sensor:null}],
      root:N('select', 'Guard', {children:[
        N('select', 'Back off when hurt', {conds:[this.check('state', {var:'Hurt', op:'==', value:1})], children:[
          N('action', 'Yield', {conds:[this.check('state', {var:'Yielded', op:'==', value:0})], do:this.act('say', {text:'Enough! I yield.'}), effects:[{var:'Yielded', op:'set', value:1, kind:'run'}]}),
          N('action', 'Step back', {do:this.act('wander', {distance:300, seconds:10})})]}),
        N('action', 'Greet the player', {conds:[this.check('state', {var:'PlayerNear', op:'==', value:1}), this.check('state', {var:'Greeted', op:'==', value:0})],
          do:this.act('say', {text:'Halt! Keep the peace.'}), effects:[{var:'Greeted', op:'set', value:1, kind:'run'}]}),
        N('sequence', 'Patrol', {children:[
          N('action', 'Wander', {do:this.act('wander', {distance:400, seconds:20})}),
          N('action', 'Rest', {do:this.act('play_anim', {group:'Idle2', seconds:4})})]})]})};
  },
  /** The templates New offers: [key, label, make]. */
  TEMPLATES:[['guard', 'Guard: greets once, backs off when hurt, patrols'], ['merchant', 'Merchant: greets customers, tidies, closes at night'],
    ['wildlife', 'Wildlife: grazes, runs from the player, fights when cornered'], ['companion', 'Companion: follows the player, warns when they are hurt'],
    ['empty', 'Empty']],
  template(k){
    const N=(t, name, more)=>this.node(t, name, more), C=(b, a, not)=>this.check(b, a, not), S=(v, x)=>this.check('state', {var:v, op:'==', value:x});
    const base=(name, modId, ids)=>({format:this.FORMAT, version:this.VERSION, name, modId, engines:{omw:true, mwse:true}, attach:{ids, all:false}, tick:0.25, subs:[]});
    if(k==='guard') return Object.assign(this.example(), {subs:[]});
    if(k==='merchant') return Object.assign(base('Merchant', 'wg_htn_merchant', []), {
      state:[{name:'PlayerNear', kind:'bool', init:0, sensor:C('player_within', {distance:400})}, {name:'Night', kind:'bool', init:0, sensor:C('hour_between', {from:20, to:6})}, {name:'Greeted', kind:'bool', init:0, sensor:null}],
      root:N('select', 'Merchant', {children:[
        N('sequence', 'Close at night', {conds:[S('Night', 1)], children:[N('action', 'Go home', {do:this.act('walk_to')}), N('action', 'Rest', {do:this.act('wait', {seconds:30})})]}),
        N('action', 'Greet a customer', {conds:[S('PlayerNear', 1), S('Greeted', 0)], do:this.act('say', {text:'Welcome! Have a look around.'}), effects:[{var:'Greeted', op:'set', value:1, kind:'run'}]}),
        N('action', 'Forget the customer', {conds:[S('PlayerNear', 0), S('Greeted', 1)], do:this.act('set_state', {var:'Greeted', value:0})}),
        N('action', 'Wait on the customer', {conds:[S('PlayerNear', 1)], do:this.act('play_anim', {group:'Idle3', seconds:6})}),
        N('action', 'Tidy the shop', {do:this.act('wander', {distance:150, seconds:30})})]})});
    if(k==='wildlife') return Object.assign(base('Wildlife', 'wg_htn_wildlife', ['rat']), {
      state:[{name:'PlayerNear', kind:'bool', init:0, sensor:C('player_within', {distance:700})}, {name:'Hurt', kind:'bool', init:0, sensor:C('health_below', {percent:50})},
        {name:'Fighting', kind:'bool', init:0, sensor:C('in_combat')}],
      root:N('select', 'Wildlife', {children:[
        N('action', 'Keep fighting', {conds:[S('Fighting', 1)], do:this.act('wait', {seconds:2})}),
        N('action', 'Fight when cornered', {conds:[S('Hurt', 1), S('PlayerNear', 1)], do:this.act('attack_player')}),
        N('action', 'Run from the player', {conds:[S('PlayerNear', 1)], do:this.act('flee_player', {distance:1500})}),
        N('sequence', 'Graze', {children:[N('action', 'Roam', {do:this.act('wander', {distance:600, seconds:25})}), N('action', 'Sniff about', {do:this.act('play_anim', {group:'Idle2', seconds:4})})]})]})});
    if(k==='companion') return Object.assign(base('Companion', 'wg_htn_companion', []), {
      state:[{name:'PlayerHurt', kind:'bool', init:0, sensor:C('player_health_below', {percent:40})}, {name:'Warned', kind:'bool', init:0, sensor:null}],
      root:N('select', 'Companion', {children:[
        N('action', 'Warn the player', {conds:[S('PlayerHurt', 1), S('Warned', 0)], do:this.act('say', {text:'You\'re hurt! Fall back, I\'ll cover you.'}), effects:[{var:'Warned', op:'set', value:1, kind:'run'}]}),
        N('action', 'Forget the warning', {conds:[S('PlayerHurt', 0), S('Warned', 1)], do:this.act('set_state', {var:'Warned', value:0})}),
        N('action', 'Stay close', {do:this.act('follow_player', {seconds:20})})]})});
    return this.empty();
  },
  empty(){
    return {format:this.FORMAT, version:this.VERSION, name:'New behaviour', modId:'wg_htn_new', engines:{omw:true, mwse:true},
      attach:{ids:[], all:false}, tick:0.25, state:[], root:this.node('select', 'New behaviour'), subs:[]};
  },
  /** Every node, depth first, with its parent and depth: from `n`, or (none given) the
   *  domain's root and then each reusable part's. */
  walk(fn, n, parent, depth){
    if(!n){ this.walk(fn, this.doc.root, null, 0); for(const sp of this.doc.subs||[]) this.walk(fn, sp, null, 0); return; }
    depth=depth|0;
    fn(n, parent||null, depth);
    for(const c of n.children||[]) this.walk(fn, c, n, depth+1);
  },
  /** A reusable part by id. */
  sub(id){ return (this.doc.subs||[]).find(x=>x.id===id) || null; },
  /** The reusable parts in the order they must be built: a part after the parts it uses. Null
   *  when parts use each other round in a circle. */
  subOrder(d){
    d=d||this.doc;
    const subs=d.subs||[], by=new Map(subs.map(x=>[x.id, x])), out=[], state=new Map();
    const uses=r=>{ const u=[]; const go=n=>{ if(n.type==='splice' && by.has(n.sub)) u.push(n.sub); (n.children||[]).forEach(go); }; go(r); return u; };
    const visit=id=>{
      const st=state.get(id); if(st===2) return true; if(st===1) return false;
      state.set(id, 1);
      for(const u of uses(by.get(id))) if(!visit(u)) return false;
      state.set(id, 2); out.push(by.get(id)); return true;
    };
    for(const x of subs) if(!visit(x.id)) return null;
    return out;
  },
  find(id){ let hit=null, up=null; this.walk((n, p)=>{ if(n.id===id){ hit=n; up=p; } }); return hit? {n:hit, parent:up} : null; },
  /** A loaded domain made whole: ids where missing, fields a newer page added. */
  normalize(d){
    if(!d || typeof d!=='object' || d.format!==this.FORMAT) throw new Error('not a Wraithguard HTN domain');
    if((d.version|0)>this.VERSION) throw new Error('made by a newer Wraithguard (version '+d.version+')');
    d.engines=d.engines||{omw:true, mwse:true}; d.attach=d.attach||{ids:[], all:false}; d.attach.ids=d.attach.ids||[];
    d.state=Array.isArray(d.state)? d.state : []; d.tick=+d.tick>0? +d.tick : 0.25;
    const fix=n=>{ n.id=n.id||this.id(); n.conds=n.conds||[]; if(n.type==='action'){ n.effects=n.effects||[]; n.exec=n.exec||[]; n.do=n.do||this.act('wait'); } (n.children||[]).forEach(fix); };
    if(!d.root || d.root.type!=='select') throw new Error('the domain has no root Select');
    fix(d.root);
    d.subs=Array.isArray(d.subs)? d.subs.filter(x=>x && x.type==='select') : [];
    d.subs.forEach(fix);
    return d;
  },

  /* ---- checking -------------------------------------------------------------------------- */
  /** The domain's problems: [{level:'error'|'warn', at:node id or '', text}]. Errors stop the
   *  export; warnings are said. */
  validate(d){
    d=d||this.doc;
    const keep=this.doc; this.doc=d;
    try{ return this.validateDoc(d); } finally{ this.doc=keep; }
  },
  validateDoc(d){
    const out=[], err=(at, text)=>out.push({level:'error', at, text}), warn=(at, text)=>out.push({level:'warn', at, text});
    if(!/^[a-z][a-z0-9_]{0,39}$/.test(d.modId||'')) err('', 'The mod id must be 1-40 lower-case letters, digits and _ , starting with a letter (it names the mod\'s folders).');
    if(!d.engines.omw && !d.engines.mwse) err('', 'Choose at least one engine to export for.');
    if(!d.attach.all && !d.attach.ids.filter(s=>String(s).trim()).length) err('', 'Say which actors use this behaviour (their record ids), or choose every NPC and creature.');
    if(d.attach.all) warn('', 'Every NPC and creature will run this behaviour; that takes their own AI from all of them.');
    const names=new Map();
    for(const v of d.state){
      if(!/^[A-Za-z_][A-Za-z0-9_]{0,39}$/.test(v.name||'')) err('', 'The value "'+(v.name||'')+'" needs a name of letters, digits and _ (not starting with a digit).');
      else if(names.has(v.name)) err('', 'Two values are called '+v.name+'.');
      names.set(v.name, v);
      if(v.sensor){ if(v.kind!=='bool') err('', v.name+' is watched from the game, so it must be true/false.'); this.checkArgs(v.sensor, this.CHECKS, err, '', 'The game check watched for '+v.name, names, true); }
      if(!isFinite(+v.init)) err('', v.name+' needs a starting number.');
    }
    if(!(+d.tick>=0.05 && +d.tick<=5)) err('', 'Plan every 0.05 to 5 seconds.');
    const root=d.root;
    if(!root.children.length) err(root.id, 'The domain is empty: add an Action, a Select or a Sequence.');
    const subIds=new Set((d.subs||[]).map(x=>x.id));
    for(const x of d.subs||[]) if(!x.children.length) err(x.id, 'The part "'+(x.name||'')+'" is empty.');
    if((d.subs||[]).length && !this.subOrder(d)) err('', 'Reusable parts use each other in a circle: one of them must stop using the other.');
    this.walk((n, parent)=>{
      const where=n.name||this.NODE_TYPES[n.type];
      if(n.type==='splice' && !subIds.has(n.sub)) err(n.id, where+': choose which reusable part it uses.');
      if((n.type==='select' || n.type==='sequence') && n!==root && parent && !n.children.length) err(n.id, where+': a '+this.NODE_TYPES[n.type]+' with nothing in it can never be done.');
      if(n.type==='pause' && (!parent || parent.type!=='sequence')) err(n.id, 'A Pause plan only works inside a Sequence.');
      if(n.type==='pause' && parent && parent.children[0]===n) warn(n.id, 'A Pause plan first in its Sequence plans nothing before it.');
      for(const c of n.conds||[]) this.checkArgs(c, this.CHECKS, err, n.id, where, names);
      if(n.type==='action'){
        if(!n.do || !this.ACTIONS[n.do.block]) err(n.id, where+': choose what it does.');
        else this.checkArgs(n.do, this.ACTIONS, err, n.id, where, names);
        for(const c of n.exec||[]) this.checkArgs(c, this.CHECKS, err, n.id, where+' (while running)', names);
        // Done at once and changing nothing: planned again every tick it can be (FluadHTN plans
        // again as soon as a plan ends), so a message shows twice a tick.
        if(['say', 'use_item'].includes(n.do && n.do.block) && !(n.effects||[]).length)
          warn(n.id, where+': it is done at once and changes nothing, so it repeats every tick it can be planned. Give it an effect (set a value) and a condition on that value, so it is done once.');
        for(const e of n.effects||[]){
          const v=names.get(e.var);
          if(!v) err(n.id, where+': an effect sets a value that is not there ('+(e.var||'none chosen')+').');
          else if(e.op==='add' && v.kind!=='number') err(n.id, where+': only a number can be added to ('+v.name+').');
          else if(v.sensor) warn(n.id, where+': '+v.name+' is watched from the game; what the effect sets is replaced on the next tick.');
          if(!isFinite(+e.value)) err(n.id, where+': an effect needs a number.');
        }
      }
      // A Select whose earlier branch can always be done hides the ones after it.
      if(n.type==='select'){
        const i=n.children.findIndex(c=>this.always(c));
        if(i>=0 && i<n.children.length-1) warn(n.id, where+': "'+(n.children[i].name||'')+'" can always be done, so the '+(n.children.length-1-i)+' after it never are.');
      }
    });
    return out;
  },
  /** Whether a node can always be done: no conditions anywhere that it needs. */
  always(n, depth){
    if((depth|0)>20) return false;
    if(n.type==='splice'){ const x=this.sub(n.sub); return !!x && this.always(x, (depth|0)+1); }
    if((n.conds||[]).length) return false;
    if(n.type==='action') return true;
    if(n.type==='pause') return true;
    if(n.type==='sequence') return n.children.length>0 && n.children.every(c=>this.always(c));
    if(n.type==='select') return n.children.some(c=>this.always(c));
    return false;
  },
  checkArgs(b, LIB, err, at, where, names, sensor){
    const def=LIB[b.block];
    if(!def){ err(at, where+': an unknown block ('+b.block+').'); return; }
    if(sensor && b.block==='state'){ err(at, where+': a watched value must watch the game, not another value.'); return; }
    for(const [k, label, type] of def.fields){
      const v=b.args[k];
      if(type==='num' && !isFinite(+v)) err(at, where+' - '+def.label+': '+label+' must be a number.');
      if((type==='id' || type==='group') && !String(v||'').trim()) err(at, where+' - '+def.label+': '+label+' is empty.');
      if(type==='state'){
        const s=names.get(b.args[k]);
        if(!s) err(at, where+' - '+def.label+': choose a value'+(b.args[k]? ' ('+b.args[k]+' is not there)' : '')+'.');
      }
      if(type==='val' && !isFinite(+v)) err(at, where+' - '+def.label+': '+label+' must be a number (true is 1, false 0).');
      if(type==='op' && !this.OPS.includes(v)) err(at, where+' - '+def.label+': an unknown comparison.');
    }
    if(b.block==='say' && !String(b.args.text||'').trim()) err(at, where+' - Show a message: the text is empty.');
  },

  /* ---- the Lua it writes ----------------------------------------------------------------- */
  lstr(s){ return '"'+String(s==null? '' : s).replace(/\\/g, '\\\\').replace(/"/g, '\\"').replace(/\n/g, '\\n').replace(/\r/g, '\\r').replace(/\0/g, '')+'"'; },
  lnum(x){ const v=+x; return isFinite(v)? String(v) : '0'; },
  /** A check as a Lua expression for `eng` ('omw'|'mwse'), over `a` (the actor) and `ctx`. */
  luaCheck(c, eng){
    const A=c.args||{}, S=v=>'S['+this.lstr(v)+']';
    let e;
    switch(c.block){
      case 'state': e='ctx:GetState('+S(A.var)+') '+(this.OPS.includes(A.op)? A.op : '==')+' '+this.lnum(A.value); break;
      case 'player_within': e='G.playerDist(a) <= '+this.lnum(A.distance); break;
      case 'health_below': e='G.health(a) < '+this.lnum(A.percent); break;
      case 'in_combat': e='G.inCombat(a)'; break;
      case 'hour_between': {
        const f=this.lnum(A.from), t=this.lnum(A.to);
        e=(+A.from<=+A.to)? '(G.hour() >= '+f+' and G.hour() < '+t+')' : '(G.hour() >= '+f+' or G.hour() < '+t+')';
        break;
      }
      case 'has_item': e='G.count(a, '+this.lstr(String(A.id).toLowerCase())+') >= '+this.lnum(A.count); break;
      case 'random_chance': e='math.random() * 100 < '+this.lnum(A.percent); break;
      case 'object_within': e='G.objDistId(a, '+this.lstr(String(A.id).toLowerCase())+') <= '+this.lnum(A.distance); break;
      case 'magicka_below': e='G.magicka(a) < '+this.lnum(A.percent); break;
      case 'fatigue_below': e='G.fatigue(a) < '+this.lnum(A.percent); break;
      case 'player_health_below': e='G.playerHealth() < '+this.lnum(A.percent); break;
      case 'player_carries': e='G.playerCount('+this.lstr(String(A.id).toLowerCase())+') >= '+this.lnum(A.count); break;
      case 'journal': e='G.journal('+this.lstr(String(A.quest).toLowerCase())+') '+(this.OPS.includes(A.op)? A.op : '>=')+' '+this.lnum(A.value); break;
      case 'lua': e='('+(String(A[eng]||'').trim() || 'true')+')'; break;
      default: e='false';
    }
    return c.not? 'not ('+e+')' : '('+e+')';
  },
  /** An action as Lua: [start body, update body, stop body], over `a`, `ctx` and `r` (its own
   *  table on the context, cleared at each start). Start and update return a task status. */
  luaAct(d, eng){
    const A=d.args||{}, n=x=>this.lnum(x);
    switch(d.block){
      case 'wait': return ['r.t0 = G.now(a)', 'if G.now(a) - r.t0 >= '+n(A.seconds)+' then return T.Success end', ''];
      case 'wander': return ['r.t0 = G.now(a)\nG.wander(a, '+n(A.distance)+')', 'if G.now(a) - r.t0 >= '+n(A.seconds)+' then G.clear(a) return T.Success end', 'G.clear(a)'];
      case 'walk_to': return ['r.t0 = G.now(a)\nG.travel(a, '+n(A.x)+', '+n(A.y)+', '+n(A.z)+')',
        'if G.distTo(a, '+n(A.x)+', '+n(A.y)+', '+n(A.z)+') < 150 then G.clear(a) return T.Success end\nif G.now(a) - r.t0 > 120 then G.clear(a) return T.Failure end', 'G.clear(a)'];
      case 'walk_to_object': return ['r.o = G.find(a, '+this.lstr(String(A.id).toLowerCase())+')\nif not r.o then return T.Failure end\nr.t0 = G.now(a)\nG.travelTo(a, r.o)',
        'if G.objDist(a, r.o) < '+n(A.near)+' then G.clear(a) return T.Success end\nif G.now(a) - r.t0 > 120 then G.clear(a) return T.Failure end', 'G.clear(a)'];
      case 'go_to_player': return ['r.t0 = G.now(a)\nG.follow(a)', 'if G.playerDist(a) < '+n(A.near)+' then G.clear(a) return T.Success end\nif G.now(a) - r.t0 > 120 then G.clear(a) return T.Failure end', 'G.clear(a)'];
      case 'follow_player': return ['r.t0 = G.now(a)\nG.follow(a)', 'if G.now(a) - r.t0 >= '+n(A.seconds)+' then G.clear(a) return T.Success end', 'G.clear(a)'];
      case 'flee_player': return ['r.t0 = G.now(a)\nG.fleeFrom(a, '+n(A.distance)+')', 'if G.playerDist(a) >= '+n(A.distance)+' then G.clear(a) return T.Success end\nif G.now(a) - r.t0 > 30 then G.clear(a) return T.Failure end', 'G.clear(a)'];
      case 'use_item': return ['if G.count(a, '+this.lstr(String(A.id).toLowerCase())+') < 1 then return T.Failure end\nG.use(a, '+this.lstr(String(A.id).toLowerCase())+')\nreturn T.Success', 'return T.Success', ''];
      case 'attack_player': return ['G.fight(a)\nreturn T.Success', 'return T.Success', ''];
      case 'say': return ['G.say(a, '+this.lstr(A.text)+')\nreturn T.Success', 'return T.Success', ''];
      case 'play_anim': return ['r.t0 = G.now(a)\nG.anim(a, '+this.lstr(A.group)+')', 'if G.now(a) - r.t0 >= '+n(A.seconds)+' then return T.Success end', ''];
      case 'set_state': return ['ctx:SetState(S['+this.lstr(A.var)+'], '+n(A.value)+', true)\nreturn T.Success', 'return T.Success', ''];
      case 'lua': return [String(A[eng+'_start']||''), String(A[eng+'_update']||'') || 'return T.Success', ''];
    }
    return ['return T.Failure', 'return T.Failure', ''];
  },
  ind(s, k){ const p=' '.repeat(k); return String(s).split('\n').map(l=>l? p+l : l).join('\n'); },
  /** The domain's builder chain and what it needs, for `eng`: the body of `build(htn)`. */
  luaDomain(d, eng, root){
    root=root||d.root;
    const L=[], E={run:'E.PlanAndExecute', plan:'E.PlanOnly', permanent:'E.Permanent'};
    const conds=(cs, how)=>{ for(const c of cs||[]) L.push('  :'+how+'('+this.lstr(this.checkText(c))+', function(ctx) local a = ctx.actor return '+this.luaCheck(c, eng)+' end)'); };
    const emit=n=>{
      if(n.type==='pause'){ L.push('  :PausePlan()'); return; }
      if(n.type==='splice'){ L.push('  :Splice(PART['+this.lstr(n.sub)+'])'); return; }
      if(n.type==='select' || n.type==='sequence'){
        L.push('  :'+(n.type==='select'? 'Select' : 'Sequence')+'('+this.lstr(n.name||this.NODE_TYPES[n.type])+')');
        conds(n.conds, 'Condition');
        for(const c of n.children) emit(c);
        L.push('  :End()');
        return;
      }
      const [st, up, sp]=this.luaAct(n.do, eng), key=this.lstr(n.id);
      L.push('  :Action('+this.lstr(n.name||'Action')+')');
      conds(n.conds, 'Condition');
      conds(n.exec, 'ExecutingCondition');
      // Each body in its own block, so one that returns is still followed by the default.
      L.push('  :Do(function(ctx)\n      local a, r = ctx.actor, ctx.run['+key+']\n      do\n'+this.ind(up, 8)+'\n      end\n      return T.Continue\n    end,');
      L.push('    function(ctx)\n      local a = ctx.actor\n      local r = {}\n      ctx.run['+key+'] = r\n      do\n'+this.ind(st, 8)+'\n      end\n      return T.Continue\n    end,');
      L.push('    function(ctx)\n      local a, r = ctx.actor, ctx.run['+key+'] or {}\n      do\n'+this.ind(sp||'-- nothing to stop', 8)+'\n      end\n    end)');
      for(const e of n.effects||[]){
        const v=this.lstr(e.var), set=e.op==='add'? 'ctx:GetState(S['+v+']) + '+this.lnum(e.value) : this.lnum(e.value);
        L.push('  :Effect('+this.lstr((e.op==='add'? 'Add to ' : 'Set ')+e.var)+', '+(E[e.kind]||E.run)+', function(ctx, t) ctx:SetState(S['+v+'], '+set+', true, t) end)');
      }
      L.push('  :End()');
    };
    for(const c of root.children) emit(c);
    return 'htn.DomainBuilder:new(Ctx, '+this.lstr(root===d.root? (d.name||'Domain') : (root.name||'Part'))+')\n'+L.join('\n')+'\n  :Build()';
  },
  /** What both engines' scripts share: the values, the context, the watched values, the
   *  domain. Expects `htn` and `G` to be there. */
  luaCore(d, eng){
    const S=d.state.map((v, i)=>'  ['+this.lstr(v.name)+'] = '+(i+1)+',').join('\n');
    const init=d.state.map(v=>this.lnum(v.kind==='bool'? (+v.init? 1 : 0) : v.init)).join(', ');
    const sense=d.state.map(v=>v.sensor? '  do local a = ctx.actor ctx:SetState(S['+this.lstr(v.name)+'], '+this.luaCheck(v.sensor, eng)+' and 1 or 0, true) end' : '').filter(Boolean).join('\n');
    return [
      '-- The world state: each value\'s index, and where it starts.',
      'local S = {\n'+S+'\n}',
      'local INIT = { '+init+' }',
      '',
      'local T, E, Ctx',
      'local function makeContext(actor, saved)',
      '  local ctx = Ctx:new()',
      '  ctx.actor = actor',
      '  for i, v in ipairs(INIT) do ctx.WorldState[i] = (saved and saved[i]) or v end',
      '  ctx:Init()',
      '  return ctx',
      'end',
      '',
      '-- The values watched from the game, set each tick before planning.',
      'local function sense(ctx)',
      sense || '  -- none',
      'end',
      '',
      'local function build(htn)',
      '  T = htn.Tasks.ETaskStatus',
      '  E = htn.Effects.EEffectType',
      '  local Base = htn.Contexts.BaseContext',
      '  Ctx = Base:subclass('+this.lstr('WgHtn_'+d.modId)+')',
      '  function Ctx:initialize()',
      '    Base.initialize(self)',
      '    self.WorldState = {}',
      '    self.Factory = htn.Factory.DefaultFactory:new()',
      '    self.PlannerState = htn.Planners.DefaultPlannerState:new()',
      '    self.run = {}',
      '  end',
      '  -- The reusable parts, each its own domain, spliced in where they are used.',
      '  local PART = {}',
      ...(this.subOrder(d)||[]).map(x=>'  PART['+this.lstr(x.id)+'] = '+this.ind(this.luaDomain(d, eng, x), 2).trimStart()),
      '  local domain = '+this.ind(this.luaDomain(d, eng), 2).trimStart(),
      '  return domain, htn.Planners.Planner:new(Ctx)',
      'end',
    ].join('\n');
  },
  one(s){ return String(s==null? '' : s).replace(/[\r\n]+/g, ' '); },
  header(d, eng){
    return ['-- '+this.one(d.name||'HTN behaviour')+' ('+d.modId+'), made with WraithguardToolkit\'s HTN AI tool.',
      '-- Runs on FluadHTN by Safebox (not included): install it from '+this.NEXUS_FLUADHTN+'.',
      eng==='omw'? '-- OpenMW also needs Rubic0n\'s LuaJIT: '+this.NEXUS_RUBIC0N+'.' : '-- MWSE: needs MWSE and FluadHTN\'s MWSE build.',
      '-- Edit the domain (' + d.modId + '.wghtn.json) in Wraithguard and export again rather than this file.', ''].join('\n');
  },
  idsTable(d){ return d.attach.all? 'nil' : '{ '+d.attach.ids.map(s=>String(s).trim().toLowerCase()).filter(Boolean).map(s=>'['+this.lstr(s)+'] = true').join(', ')+' }'; },
  /** OpenMW's game layer, for an NPC or creature's local script (`a` is `self`). */
  omwGame(d){
    return [
      'local self = require("openmw.self")',
      'local types = require("openmw.types")',
      'local nearby = require("openmw.nearby")',
      'local core = require("openmw.core")',
      'local util = require("openmw.util")',
      'local I = require("openmw.interfaces")',
      'local anim = require("openmw.animation")',
      '',
      'local G = {}',
      'function G.player(a) return nearby.players[1] end',
      'function G.playerDist(a) local p = nearby.players[1] if not p then return math.huge end return (self.position - p.position):length() end',
      'function G.distTo(a, x, y, z) return (self.position - util.vector3(x, y, z)):length() end',
      'function G.objDist(a, o) if not o or not o:isValid() then return math.huge end return (self.position - o.position):length() end',
      'function G.health(a) local h = types.Actor.stats.dynamic.health(self) if h.base <= 0 then return 100 end return h.current / h.base * 100 end',
      'function G.inCombat(a) local p = I.AI.getActivePackage() return p ~= nil and p.type == "Combat" end',
      'function G.hour() return (core.getGameTime() / 3600) % 24 end',
      'function G.count(a, id) return types.Actor.inventory(self):countOf(id) end',
      'function G.now(a) return core.getSimulationTime() end',
      'function G.find(a, id)',
      '  local best, bd = nil, math.huge',
      '  for _, list in ipairs({ nearby.actors, nearby.items, nearby.activators, nearby.containers, nearby.doors }) do',
      '    for _, o in ipairs(list) do',
      '      if o.recordId == id then local dd = (self.position - o.position):length() if dd < bd then best, bd = o, dd end end',
      '    end',
      '  end',
      '  return best',
      'end',
      'function G.wander(a, d) I.AI.startPackage({ type = "Wander", distance = d }) end',
      'function G.travel(a, x, y, z) I.AI.startPackage({ type = "Travel", destPosition = util.vector3(x, y, z) }) end',
      'function G.travelTo(a, o) I.AI.startPackage({ type = "Travel", destPosition = o.position }) end',
      'function G.follow(a) local p = nearby.players[1] if p then I.AI.startPackage({ type = "Follow", target = p }) end end',
      'function G.fight(a) local p = nearby.players[1] if p then I.AI.startPackage({ type = "Combat", target = p }) end end',
      'function G.clear(a) I.AI.removePackages() end',
      'function G.say(a, text) local p = nearby.players[1] if p then p:sendEvent('+this.lstr('WgHtnSay_'+d.modId)+', { text = text }) end end',
      'function G.anim(a, group) I.AnimationController.playBlendedAnimation(group, { priority = anim.PRIORITY.Scripted }) end',
      'local function pct(s) if s.base <= 0 then return 100 end return s.current / s.base * 100 end',
      'function G.magicka(a) return pct(types.Actor.stats.dynamic.magicka(self)) end',
      'function G.fatigue(a) return pct(types.Actor.stats.dynamic.fatigue(self)) end',
      'function G.playerHealth() local p = nearby.players[1] if not p then return 100 end return pct(types.Actor.stats.dynamic.health(p)) end',
      'function G.playerCount(id) local p = nearby.players[1] if not p then return 0 end return types.Actor.inventory(p):countOf(id) end',
      'function G.journal(id) local p = nearby.players[1] if not p then return 0 end local ok, q = pcall(function() return types.Player.quests(p)[id] end) return ok and q and q.stage or 0 end',
      'function G.objDistId(a, id) return G.objDist(a, G.find(a, id)) end',
      'function G.fleeFrom(a, d)',
      '  local p = nearby.players[1] if not p then return end',
      '  local away = self.position - p.position',
      '  if away:length() < 1 then away = util.vector3(1, 0, 0) end',
      '  I.AI.startPackage({ type = "Travel", destPosition = p.position + away:normalize() * (d + 200) })',
      'end',
      '-- Use as the game\'s Use does (OpenMW\'s own UseItem handler, a global script).',
      'function G.use(a, id) local o = types.Actor.inventory(self):find(id) if o then core.sendGlobalEvent("UseItem", { object = o, actor = self }) end end',
    ].join('\n');
  },
  /** The OpenMW mod's files: [[path, text]...]. `opt.trace`: the planner's events printed for
   *  the Preview's run on FluadHTN (`fromLua`), never in an export. */
  exportOmw(d, opt){
    const id=d.modId, dir='scripts/'+id+'/', trace=!!(opt && opt.trace);
    const actor=[this.header(d, 'omw'), this.omwGame(d), '', this.luaCore(d, 'omw'), '',
      '-- Which actors run it: their record ids (nil: every one this script is on).',
      'local IDS = '+this.idsTable(d),
      'local TICK = '+this.lnum(d.tick),
      'local domain, planner, ctx, on, saved, wait, said = nil, nil, nil, nil, nil, 0, false',
      ...(trace? [
        '-- The Preview\'s run: what the planner does, printed for Wraithguard\'s HTN AI page.',
        'local lastWs',
        'local function say(k, s) print("[wg_htn_trace] " .. k .. "|" .. (s or "")) end',
        'local function names(q) local t = {} for _, x in ipairs(q and q.list or {}) do t[#t + 1] = tostring(x.Name) end return table.concat(t, "|") end',
        'local function traced(ps)',
        '  ps.OnNewPlan = function(_, p) say("plan", names(p)) end',
        '  ps.OnReplacePlan = function(_, old, cur, p) say("replan", names(p)) end',
        '  ps.OnNewTask = function(_, t) say("task", t.Name) end',
        '  ps.OnCurrentTaskCompletedSuccessfully = function(_, t) say("done", t.Name) end',
        '  ps.OnCurrentTaskFailed = function(_, t) say("fail", t.Name) end',
        '  ps.OnNewTaskConditionFailed = function(_, t) say("condfail", t.Name) end',
        '  ps.OnCurrentTaskExecutingConditionFailed = function(_, t) say("execfail", t.Name) end',
        '  ps.OnStopCurrentTask = function(_, t) say("stop", t.Name) end',
        'end'] : []),
      'local function start()',
      '  if on ~= nil then return on end',
      '  on = IDS == nil or IDS[self.recordId] == true',
      '  if not on then return false end',
      '  local ok, mod = pcall(function() return require("scripts.sb_htn.interface") end)',
      '  if not ok then print('+this.lstr('['+id+'] FluadHTN (OpenMW build) is not installed: ')+' .. tostring(mod)) on = false return false end',
      '  if type(table.size) ~= "function" then print('+this.lstr('['+id+'] FluadHTN on OpenMW needs Rubic0n\'s LuaJIT ('+this.NEXUS_RUBIC0N+')')+') on = false return false end',
      '  domain, planner = build(mod.interface)',
      '  ctx = makeContext(self, saved)',
      ...(trace? ['  traced(ctx.PlannerState)', '  say("start", self.recordId)'] : []),
      '  return true',
      'end',
      '',
      'return {',
      '  engineHandlers = {',
      '    onUpdate = function(dt)',
      '      if dt <= 0 or not start() then return end',
      '      wait = wait + dt',
      '      if wait < TICK then return end',
      '      wait = 0',
      '      local ok, err = pcall(function() sense(ctx) planner:Tick(domain, ctx) end)',
      '      if not ok and not said then said = true print('+this.lstr('['+id+'] ')+' .. tostring(err)) end',
      ...(trace? ['      local ws = table.concat(ctx.WorldState, ",")', '      if ws ~= lastWs then lastWs = ws say("ws", ws) end'] : []),
      '    end,',
      '    onSave = function() if ctx then return { ws = ctx.WorldState } end return saved and { ws = saved } or nil end,',
      '    onLoad = function(data) saved = data and data.ws or nil end,',
      '  },',
      '}', ''].join('\n');
    const player=[this.header(d, 'omw'), 'local ui = require("openmw.ui")', '',
      '-- The messages its actors show.', 'return {', '  eventHandlers = {',
      '    ['+this.lstr('WgHtnSay_'+id)+'] = function(e) ui.showMessage(tostring(e.text)) end,', '  },', '}', ''].join('\n');
    const reg='NPC, CREATURE: '+dir+'actor.lua\nPLAYER: '+dir+'player.lua\n';
    return [[id+'.omwscripts', reg], [dir+'actor.lua', actor], [dir+'player.lua', player]];
  },
  /** MWSE's game layer (`a` is the actor's reference). */
  mwseGame(){
    return [
      'local G = { t = 0 }',
      'local function dist(p, x, y, z) local dx, dy, dz = p.x - x, p.y - y, p.z - z return math.sqrt(dx * dx + dy * dy + dz * dz) end',
      'function G.player(a) return tes3.player end',
      'function G.playerDist(a) local p = tes3.player.position return dist(a.position, p.x, p.y, p.z) end',
      'function G.distTo(a, x, y, z) return dist(a.position, x, y, z) end',
      'function G.objDist(a, o) if not o then return math.huge end local p = o.position return dist(a.position, p.x, p.y, p.z) end',
      'function G.health(a) local m = a.mobile if not m or m.health.base <= 0 then return 100 end return m.health.current / m.health.base * 100 end',
      'function G.inCombat(a) return a.mobile ~= nil and a.mobile.inCombat == true end',
      'function G.hour() return tes3.worldController.hour.value end',
      'function G.count(a, id) return tes3.getItemCount({ reference = a, item = id }) end',
      'function G.now(a) return G.t end',
      'function G.find(a, id) return tes3.getReference(id) end',
      'local IDLES = { 60, 20, 10, 10, 0, 0, 0, 0 }',
      'function G.wander(a, d) tes3.setAIWander({ reference = a, range = d, idles = IDLES }) end',
      'function G.travel(a, x, y, z) tes3.setAITravel({ reference = a, destination = tes3vector3.new(x, y, z) }) end',
      'function G.travelTo(a, o) tes3.setAITravel({ reference = a, destination = o.position }) end',
      'function G.follow(a) tes3.setAIFollow({ reference = a, target = tes3.player }) end',
      'function G.fight(a) if a.mobile then a.mobile:startCombat(tes3.mobilePlayer) end end',
      'function G.clear(a) tes3.setAIWander({ reference = a, range = 0, idles = IDLES }) end',
      'function G.say(a, text) tes3.messageBox(text) end',
      'function G.anim(a, group)',
      '  local g = tes3.animationGroup[group:sub(1, 1):lower() .. group:sub(2)]',
      '  if g then tes3.playAnimation({ reference = a, group = g }) end',
      'end',
      'local function pct(s) if not s or s.base <= 0 then return 100 end return s.current / s.base * 100 end',
      'function G.magicka(a) return a.mobile and pct(a.mobile.magicka) or 100 end',
      'function G.fatigue(a) return a.mobile and pct(a.mobile.fatigue) or 100 end',
      'function G.playerHealth() return pct(tes3.mobilePlayer and tes3.mobilePlayer.health) end',
      'function G.playerCount(id) return tes3.getItemCount({ reference = tes3.player, item = id }) end',
      'function G.journal(id) return tes3.getJournalIndex({ id = id }) or 0 end',
      'function G.objDistId(a, id) return G.objDist(a, tes3.getReference(id)) end',
      'function G.fleeFrom(a, d)',
      '  local p, q = a.position, tes3.player.position',
      '  local x, y = p.x - q.x, p.y - q.y',
      '  local l = math.sqrt(x * x + y * y)',
      '  if l < 1 then x, y, l = 1, 0, 1 end',
      '  G.travel(a, q.x + x / l * (d + 200), q.y + y / l * (d + 200), p.z)',
      'end',
      'function G.use(a, id) if a.mobile then a.mobile:equip({ item = id }) end end',
    ].join('\n');
  },
  /** The MWSE mod's files. */
  exportMwse(d){
    const id=d.modId;
    const main=[this.header(d, 'mwse'), this.mwseGame(), '', this.luaCore(d, 'mwse'), '',
      'local IDS = '+this.idsTable(d),
      'local TICK = '+this.lnum(d.tick),
      'local domain, planner',
      'local actors = {}   -- reference -> its context',
      'local scanWait, tickWait, said = 0, 0, false',
      '',
      'local function wanted(ref)',
      '  if ref.disabled then return false end',
      '  local obj = ref.baseObject or ref.object',
      '  return IDS == nil or IDS[obj.id:lower()] == true',
      'end',
      '-- The actors of the loaded cells that run it, looked for once a second.',
      'local function scan()',
      '  local seen = {}',
      '  local function take(ref)',
      '    if ref and not seen[ref] and wanted(ref) then',
      '      seen[ref] = true',
      '      if not actors[ref] then actors[ref] = makeContext(ref, ref.data and ref.data.' + id + ') end',
      '    end',
      '  end',
      '  local cells = tes3.getActiveCells()',
      '  if type(cells) == "table" then',
      '    for _, cell in ipairs(cells) do',
      '      for ref in cell:iterateReferences({ tes3.objectType.npc, tes3.objectType.creature }) do take(ref) end',
      '    end',
      '  elseif IDS then',
      '    -- Without the loaded cells to look through, each id\'s reference.',
      '    for id in pairs(IDS) do take(tes3.getReference(id)) end',
      '  end',
      '  for ref, ctx in pairs(actors) do',
      '    if not seen[ref] then',
      '      ref.data.' + id + ' = ctx.WorldState',
      '      actors[ref] = nil',
      '    end',
      '  end',
      'end',
      '',
      'local function onSimulate(e)',
      '  G.t = G.t + e.delta',
      '  scanWait = scanWait - e.delta',
      '  if scanWait <= 0 then scanWait = 1 scan() end',
      '  tickWait = tickWait + e.delta',
      '  if tickWait < TICK then return end',
      '  tickWait = 0',
      '  for ref, ctx in pairs(actors) do',
      '    local ok, err = pcall(function() sense(ctx) planner:Tick(domain, ctx) end)',
      '    if not ok and not said then said = true mwse.log('+this.lstr('['+id+'] %s')+', tostring(err)) end',
      '  end',
      'end',
      '',
      'local function onLoaded() actors = {} scanWait = 0 end',
      '-- Each actor\'s world state kept in its reference\'s data, so a save keeps it.',
      'local function onSave() for ref, ctx in pairs(actors) do ref.data.' + id + ' = ctx.WorldState end end',
      '',
      'local function onInitialized()',
      '  local ok, htn = pcall(function() return require("sb_htn.interop") end)',
      '  if not ok then mwse.log('+this.lstr('['+id+'] FluadHTN (MWSE build) is not installed: %s')+', tostring(htn)) return end',
      '  domain, planner = build(htn)',
      '  event.register(tes3.event.simulate, onSimulate)',
      '  event.register(tes3.event.loaded, onLoaded)',
      '  event.register(tes3.event.save, onSave)',
      'end',
      'event.register(tes3.event.initialized, onInitialized)', ''].join('\n');
    return [['MWSE/mods/'+id+'/main.lua', main]];
  },
  readme(d){
    const L=['# '+this.one(d.name||'HTN behaviour'), '', 'Made with WraithguardToolkit\'s HTN AI tool from '+d.modId+'.wghtn.json (kept beside this file; open it in Wraithguard to change the behaviour and export again).', '',
      'Actors: '+(d.attach.all? 'every NPC and creature' : d.attach.ids.join(', ')), '', '## Requirements', '',
      '- FluadHTN by Safebox: '+this.NEXUS_FLUADHTN+' (not included; install it from its page)'];
    if(d.engines.omw) L.push('- OpenMW: FluadHTN\'s OpenMW build and Rubic0n ('+this.NEXUS_RUBIC0N+'), and `content='+d.modId+'.omwscripts` in openmw.cfg (or the mod enabled in your launcher)');
    if(d.engines.mwse) L.push('- MWSE: FluadHTN\'s MWSE build');
    L.push('');
    return L.join('\n');
  },
  /** Every file of the export, for the chosen engines. */
  files(d){
    d=d||this.doc;
    const out=[];
    if(d.engines.omw) out.push(...this.exportOmw(d));
    if(d.engines.mwse) out.push(...this.exportMwse(d));
    out.push([d.modId+'.wghtn.json', JSON.stringify(d, null, 2)+'\n'], [d.modId+' - README.md', this.readme(d)]);
    return out;
  },

  /* ---- plain-words text for the tree ----------------------------------------------------- */
  checkText(c){
    const A=c.args||{}, def=this.CHECKS[c.block];
    let t;
    switch(c.block){
      case 'state': { const v=this.doc && this.doc.state.find(s=>s.name===A.var); const val=v && v.kind==='bool'? (+A.value? 'true' : 'false') : A.value;
        t=(A.var||'?')+' '+({'==':'is', '~=':'is not', '<':'<', '<=':'≤', '>':'>', '>=':'≥'}[A.op]||A.op)+' '+val; break; }
      case 'player_within': t='the player is within '+A.distance; break;
      case 'health_below': t='health below '+A.percent+'%'; break;
      case 'in_combat': t='in a fight'; break;
      case 'hour_between': t='the hour is '+A.from+' to '+A.to; break;
      case 'has_item': t='carries '+A.count+' '+(A.id||'?'); break;
      case 'random_chance': t=A.percent+'% chance'; break;
      case 'object_within': t=(A.id||'?')+' within '+A.distance; break;
      case 'magicka_below': t='magicka below '+A.percent+'%'; break;
      case 'fatigue_below': t='fatigue below '+A.percent+'%'; break;
      case 'player_health_below': t='the player\'s health below '+A.percent+'%'; break;
      case 'player_carries': t='the player carries '+A.count+' '+(A.id||'?'); break;
      case 'journal': t=(A.quest||'?')+' '+A.op+' '+A.value; break;
      case 'lua': t='custom Lua'; break;
      default: t=def? def.label : c.block;
    }
    return (c.not? 'not: ' : '')+t;
  },
  actText(dd){
    const A=dd.args||{};
    switch(dd.block){
      case 'wait': return 'wait '+A.seconds+' s';
      case 'wander': return 'wander '+A.distance+' for '+A.seconds+' s';
      case 'walk_to': return 'walk to '+[A.x, A.y, A.z].map(v=>Math.round(+v)).join(', ');
      case 'walk_to_object': return 'walk to '+(A.id||'?');
      case 'go_to_player': return 'go to the player';
      case 'follow_player': return 'follow the player '+A.seconds+' s';
      case 'attack_player': return 'attack the player';
      case 'flee_player': return 'run from the player';
      case 'use_item': return 'use '+(A.id||'?');
      case 'say': return '"'+String(A.text||'').slice(0, 40)+'"';
      case 'play_anim': return 'play '+A.group;
      case 'set_state': return 'set '+(A.var||'?')+' to '+A.value;
      case 'lua': return 'custom Lua';
    }
    return dd.block;
  },

  /* ---- keeping it ------------------------------------------------------------------------ */
  KEY:'wgHtnDraft',
  keep(){ try{ localStorage.setItem(this.KEY, JSON.stringify({doc:this.doc, path:this.path})); }catch(_){ } },
  restore(){
    try{ const s=JSON.parse(localStorage.getItem(this.KEY)||'null'); if(s && s.doc){ this.doc=this.normalize(s.doc); this.path=s.path||''; return true; } }catch(_){ }
    return false;
  },
  changed(){ this.record(); this.keep(); this.draw(); },
  /* Undo and redo (Ctrl+Z, Ctrl+Y or Ctrl+Shift+Z, and the buttons): the domain as it was
     after each change, the last 100. */
  hist:[], fut:[], _snap:null,
  record(){
    const now=JSON.stringify(this.doc);
    if(this._snap!=null && now!==this._snap){ this.hist.push(this._snap); if(this.hist.length>100) this.hist.shift(); this.fut=[]; }
    this._snap=now;
  },
  fresh(){ this.hist=[]; this.fut=[]; this._snap=JSON.stringify(this.doc); },
  undo(){ this.step(this.hist, this.fut); },
  redo(){ this.step(this.fut, this.hist); },
  step(from, to){
    if(!from.length) return;
    to.push(this._snap); this._snap=from.pop();
    this.doc=this.normalize(JSON.parse(this._snap));
    if(this.sel && !this.find(this.sel)) this.sel='';
    this.keep(); this.draw();
  },
  async open(){
    let p;
    try{ p=await Engine.pick('open', {filters:[{name:'HTN domain', extensions:['json']}]}); }catch(e){ toast(String(e && e.message || e), 'err', 4000); return; }
    if(!p) return;
    try{ const d=await Engine.call('htn_read', {path:String(p)}); this.doc=this.normalize(typeof d==='string'? JSON.parse(d) : d); this.path=String(p); this.sel=''; this.stopRun(); this.fresh(); this.changed(); toast('Opened '+this.doc.name, 'ok', 2500); }
    catch(e){ toast(String(e && e.message || e), 'err', 5000); }
  },
  async save(as){
    let p=this.path;
    if(as || !p){
      try{ p=await Engine.pick('save', {defaultPath:this.doc.modId+'.wghtn.json', filters:[{name:'HTN domain', extensions:['json']}]}); }catch(e){ toast(String(e && e.message || e), 'err', 4000); return; }
      if(!p) return;
      p=String(p); if(!/\.wghtn\.json$/i.test(p)) p=p.replace(/\.json$/i, '')+'.wghtn.json';
    }
    try{ await Engine.call('write_text', {path:p, text:JSON.stringify(this.doc, null, 2)+'\n'}); this.path=p; this.keep(); this.draw(); toast('Saved', 'ok', 2000); }
    catch(e){ toast(String(e && e.message || e), 'err', 5000); }
  },
  async exportMod(){
    const probs=this.validate().filter(p=>p.level==='error');
    if(probs.length){ toast('Fix the '+probs.length+' problem'+(probs.length===1? '' : 's')+' first', 'warn', 3500); this.drawProblems(); return; }
    let dir;
    try{ dir=await Engine.pick('open', {directory:true}); }catch(e){ toast(String(e && e.message || e), 'err', 4000); return; }
    if(!dir) return;
    const files=this.files();
    try{
      const out=await Engine.call('htn_export', {folder:String(dir), files});
      const n=Array.isArray(out)? out.length : files.length;
      toast('Exported '+n+' files to '+dir, 'ok', 4000);
      this._exported=String(dir); this.draw();
    }catch(e){ toast(String(e && e.message || e), 'err', 6000); }
  },
  async loadDeps(){
    let dir=''; try{ dir=localStorage.getItem('wgOpenMWDir')||''; }catch(_){ }
    try{ this.deps=await Engine.call('htn_deps', {openmw:dir||null}); if(typeof this.deps==='string') this.deps=JSON.parse(this.deps); }
    catch(e){ this.deps={error:String(e && e.message || e)}; }
    this.drawDeps();
  },

  /* ---- the Preview run: this page's planner, on an actor of the loaded cell -------------- */
  /** The scene's actors: [{key, id, tag}] (NPCs and creatures). */
  sceneActors(){
    const S=typeof WgScripts==='object'? WgScripts : null, out=[];
    if(!S || !App._scene) return out;
    for(const c of App._scene.cells||[]) for(const r of c.refs||[]){
      const id=String(r.id||'').toLowerCase(), def=S.actorDefs && S.actorDefs.get(id);
      if(def && (def.tag==='NPC_' || def.tag==='CREA') && r.key) out.push({key:r.key, id, tag:def.tag, ref:r});
    }
    return out;
  },
  thingOf(a){ const S=WgScripts; return S.inst.get(a.key) || {key:a.key, id:a.id, _ref:a.ref, base:{pos:(a.ref.pos||[0,0,0]).slice(), rot:(a.ref.rot||[0,0,0]).slice()}}; },
  startRun(key){
    const S=typeof WgScripts==='object'? WgScripts : null;
    if(!S || !S.on){ toast('Turn on the Scripts in the Preview first (the actors move while they run)', 'warn', 4000); return; }
    const probs=this.validate().filter(p=>p.level==='error');
    if(probs.length){ toast('Fix the problems first', 'warn', 3000); this.drawProblems(); return; }
    const a=this.sceneActors().find(x=>x.key===key);
    if(!a){ toast('That actor is not in the loaded cell', 'warn', 3000); return; }
    const t=this.thingOf(a), A=S.actorOf(t);
    if(!A){ toast('That actor cannot be run', 'warn', 3000); return; }
    // Its own packages give way: the domain drives it now.
    A.packages=null; A.ai=null; A._route=null;
    this.run={key, t, A, doc:JSON.parse(JSON.stringify(this.doc)), ws:this.doc.state.map(v=>+v.init||0), plan:[], mtr:null, cur:null, op:null,
      partial:null, dirty:false, wait:0, clock:0, log:[], status:'', _drawAt:0};
    this.say('Running "'+this.doc.name+'" on '+a.id);
    this.drawRun();
  },
  stopRun(){
    const R=this.run; if(!R) return;
    if(R.mode==='fluad'){
      // The Lua started again without the page's mod; the actor's AI left to its own.
      this.run=null; R.A.ai=null; R.A._route=null;
      if(typeof WgLua==='object'){ WgLua.extra=null; if(WgScripts.on) WgLua.start(); }
    }
    else{ if(R.cur && R.op) this.simStop(R, R.cur); this.run=null; }
    this.drawRun(); this.drawTree();
  },
  /** The run on FluadHTN itself: the domain exported for OpenMW with its planner's events printed,
   *  for this actor alone, added to the Preview's Lua (`WgLua.extra`) and run on the FluadHTN the
   *  load order has. Without Rubic0n's LuaJIT the Preview supplies its table functions (and says
   *  so: OpenMW itself needs Rubic0n). */
  async startFluad(key){
    const S=typeof WgScripts==='object'? WgScripts : null;
    if(!S || !S.on || typeof WgLua!=='object'){ toast('Turn on the Scripts in the Preview first', 'warn', 4000); return; }
    if(!this.deps || !this.deps.omw){ toast('FluadHTN\'s OpenMW build is not in the data folders: running on Wraithguard\'s planner', 'warn', 4500); return this.startRun(key); }
    const probs=this.validate().filter(p=>p.level==='error');
    if(probs.length){ toast('Fix the problems first', 'warn', 3000); this.drawProblems(); return; }
    const a=this.sceneActors().find(x=>x.key===key);
    if(!a){ toast('That actor is not in the loaded cell', 'warn', 3000); return; }
    const d=JSON.parse(JSON.stringify(this.doc));
    d.attach={ids:[a.id], all:false}; d.engines={omw:true, mwse:false};
    const dir='scripts/'+d.modId+'/';
    const files=this.exportOmw(d, {trace:true}).filter(f=>/\.lua$/.test(f[0]));
    const tableExt=this.deps.rubic0n!==true;
    const t=this.thingOf(a), A=S.actorOf(t);
    if(!A){ toast('That actor cannot be run', 'warn', 3000); return; }
    A.packages=null; A.ai=null; A._route=null;
    this.stopRun();
    this.run={mode:'fluad', key, t, A, doc:d, ws:d.state.map(v=>+v.init||0), plan:[], curName:'', log:[], clock:0, status:'', _drawAt:0};
    this.say('Running "'+d.name+'" on FluadHTN, on '+a.id);
    if(tableExt) this.say('Rubic0n\'s table functions are supplied by the Preview: OpenMW itself needs Rubic0n installed to run this');
    WgLua.extra={files, scripts:[[dir+'actor.lua', ['NPC', 'CREATURE']], [dir+'player.lua', ['PLAYER']]], tableExt};
    await WgLua.start();
    this.drawRun();
  },
  /** A line the Preview's Lua printed: the run's planner events (`[wg_htn_trace] kind|text`),
   *  taken here; its script's errors noted in the run's log too. */
  fromLua(path, kind, text){
    const R=this.run; if(!R || R.mode!=='fluad') return false;
    const m=/^\[wg_htn_trace\] (\w+)\|([\s\S]*)$/.exec(String(text||''));
    if(!m){
      if((kind==='error' || /^\[/.test(String(text))) && String(path||'').toLowerCase().includes('scripts/'+R.doc.modId+'/')) this.say((kind==='error'? 'Lua error: ' : '')+text);
      return false;
    }
    const k=m[1], v=m[2];
    switch(k){
      case 'start': this.say('FluadHTN is planning for '+v); break;
      case 'plan': case 'replan': R.plan=v? v.split('|') : []; this.say((k==='plan'? 'Plan: ' : 'New plan: ')+R.plan.join(' → ')); R.status='planned'; break;
      case 'task': { R.curName=v; const i=R.plan.indexOf(v); if(i>=0) R.plan.splice(i, 1); this.say('Start: '+v); break; }
      case 'done': this.say('Done: '+v); R.curName=''; break;
      case 'fail': this.say('Failed: '+v+'; planning again'); R.curName=''; R.plan=[]; break;
      case 'condfail': this.say(v+': its conditions no longer hold; planning again'); R.curName=''; R.plan=[]; break;
      case 'execfail': this.say(v+': a while-running condition failed'); R.curName=''; R.plan=[]; break;
      case 'stop': this.say('Stopped '+v+' for a better plan'); R.curName=''; break;
      case 'ws': R.ws=v.split(',').map(Number); break;
      default: return false;
    }
    this.drawTree();
    return true;
  },
  say(text){ const R=this.run; if(!R) return; R.log.push([R.clock, text]); if(R.log.length>60) R.log.shift(); },
  /** One frame of the Scripts (73_wg_scripts.js `frame`): the run ticks at the domain's rate. */
  frame(dt){
    const R=this.run; if(!R || !(dt>0)) return;
    R.clock+=dt; R.wait+=dt;
    // On FluadHTN the Lua runs it; the page only keeps the clock and the panel.
    if(R.mode==='fluad'){ if(R.clock-R._drawAt>0.4){ R._drawAt=R.clock; this.drawRun(); } return; }
    if(R.A.dead){ this.say(R.A.id+' is dead'); this.stopRun(); return; }
    if(R.wait<R.doc.tick) return;
    const step=R.wait; R.wait=0;
    try{ this.tick(R, step); }
    catch(e){ this.say('Error: '+String(e && e.message || e)); this.stopRun(); return; }
    if(R.clock-R._drawAt>0.4){ R._drawAt=R.clock; this.drawRun(); }
  },
  idx(R, name){ return R.doc.state.findIndex(v=>v.name===name); },
  /** A check against `ws` (the world state as planned, or as it is). */
  simCheck(R, c, ws){
    const A=c.args||{}, S=WgScripts, me=R.t;
    let v;
    switch(c.block){
      case 'state': { const i=this.idx(R, A.var), x=i>=0? ws[i] : 0, y=+A.value;
        v=A.op==='=='? x===y : A.op==='~='? x!==y : A.op==='<'? x<y : A.op==='<='? x<=y : A.op==='>'? x>y : x>=y; break; }
      case 'player_within': v=S.dist(me, {player:true})<=+A.distance; break;
      case 'health_below': v=(R.A.max && R.A.max[0]>0? R.A.hp/R.A.max[0]*100 : 100)<+A.percent; break;
      case 'in_combat': v=!!R.A.combat; break;
      case 'hour_between': { const h=this.hour(), f=+A.from, t=+A.to; v=f<=t? (h>=f && h<t) : (h>=f || h<t); break; }
      case 'has_item': v=(R.A.items.get(String(A.id).toLowerCase())||0)>=+A.count; break;
      case 'random_chance': v=Math.random()*100<+A.percent; break;
      case 'object_within': { const r=S.refById(String(A.id).toLowerCase()); v=!!r && S.dist(me, {key:r.key, ref:r})<=+A.distance; break; }
      case 'magicka_below': v=(R.A.max && R.A.max[1]>0? R.A.mp/R.A.max[1]*100 : 100)<+A.percent; break;
      case 'fatigue_below': v=(R.A.max && R.A.max[2]>0? R.A.fp/R.A.max[2]*100 : 100)<+A.percent; break;
      case 'player_health_below': { const P=S.actorOf({player:true}, true); v=(P && P.max && P.max[0]>0? P.hp/P.max[0]*100 : 100)<+A.percent; break; }
      case 'player_carries': { const P=S.actorOf({player:true}, true); v=(P && P.items? P.items.get(String(A.id).toLowerCase())||0 : 0)>=+A.count; break; }
      case 'journal': { const j=S.state && S.state.journal, x=+((j && j.get(String(A.quest).toLowerCase()))||0), y=+A.value;
        v=A.op==='=='? x===y : A.op==='~='? x!==y : A.op==='<'? x<y : A.op==='<='? x<=y : A.op==='>'? x>y : x>=y; break; }
      case 'lua': v=true; break;
      default: v=false;
    }
    return c.not? !v : v;
  },
  hour(){ const h=typeof $==='function' && $('#p_hour'); return h? +h.value : 12; },
  condsOk(R, cs, ws){ return (cs||[]).every(c=>this.simCheck(R, c, ws)); },
  applyEffects(n, ws, kinds, R){
    for(const e of n.effects||[]){
      if(!kinds.includes(e.kind||'run')) continue;
      const i=this.idx(R, e.var); if(i<0) continue;
      ws[i]=e.op==='add'? ws[i]+(+e.value) : +e.value;
    }
  },
  /** A node decomposed against `ws` (changed as its steps' effects are counted): its steps and
   *  method record, or null. A Pause in a Sequence stops the plan there: the rest is kept to
   *  plan from once the steps before it are done. */
  decompose(R, n, ws, mtr){
    if(!this.condsOk(R, n.conds, ws)) return null;
    if(n.type==='action'){ this.applyEffects(n, ws, ['run', 'plan', 'permanent'], R); return {tasks:[n], partial:null}; }
    if(n.type==='select'){
      for(let i=0; i<n.children.length; i++){
        const w=ws.slice(), m=[];
        const r=n.children[i].type==='pause'? null : this.decompose(R, n.children[i], w, m);
        if(r){ mtr.push(i, ...m); ws.splice(0, ws.length, ...w); return r; }
      }
      return null;
    }
    if(n.type==='sequence') return this.decomposeSeq(R, n, 0, ws, mtr);
    if(n.type==='splice'){
      const x=(R.doc.subs||[]).find(y=>y.id===n.sub);
      if(!x || (R._depth|0)>40) return null;
      R._depth=(R._depth|0)+1;
      try{ return this.decompose(R, Object.assign({}, x, {conds:[]}), ws, mtr); } finally{ R._depth--; }
    }
    return null;
  },
  decomposeSeq(R, n, from, ws, mtr){
    const w=ws.slice(), tasks=[], m=[];
    for(let i=from; i<n.children.length; i++){
      const c=n.children[i];
      if(c.type==='pause'){ if(tasks.length){ ws.splice(0, ws.length, ...w); mtr.push(...m); return {tasks, partial:{seq:n, at:i+1}}; } continue; }
      const r=this.decompose(R, c, w, m);
      if(!r) return null;
      tasks.push(...r.tasks);
      if(r.partial){ ws.splice(0, ws.length, ...w); mtr.push(...m); return {tasks, partial:r.partial}; }
    }
    ws.splice(0, ws.length, ...w); mtr.push(...m);
    return {tasks, partial:null};
  },
  /** Whether method record `a` takes an earlier branch than `b`. */
  better(a, b){
    if(!b) return true;
    for(let i=0; i<Math.min(a.length, b.length); i++) if(a[i]!==b[i]) return a[i]<b[i];
    return false;
  },
  findPlan(R){
    const ws=R.ws.slice(), mtr=[];
    if(R.partial && !R.cur && !R.plan.length){
      const p=R.partial;
      const r=this.decomposeSeq(R, p.seq, p.at, ws, mtr);
      R.partial=null;
      if(r) return {r, mtr:R.mtr||[], cont:true};
    }
    const root=R.doc.root;
    const r=this.decompose(R, Object.assign({}, root, {conds:[]}), ws, mtr);
    return r? {r, mtr} : null;
  },
  /** One planner tick: the watched values set, a plan found when there is none or the world
   *  changed (kept unless the new one takes an earlier branch), the next step started, the
   *  current one run. */
  tick(R, dt){
    // Watched values.
    R.doc.state.forEach((v, i)=>{
      if(!v.sensor) return;
      const x=this.simCheck(R, v.sensor, R.ws)? 1 : 0;
      if(R.ws[i]!==x){ R.ws[i]=x; R.dirty=true; this.say(v.name+' is now '+(x? 'true' : 'false')); }
    });
    if(R.dirty || (!R.cur && !R.plan.length)){
      const replacing=!!(R.cur || R.plan.length);
      const f=this.findPlan(R);
      R.dirty=false;
      if(f && (!replacing || f.cont || this.better(f.mtr, R.mtr))){
        if(R.cur){ this.simStop(R, R.cur); this.say('Stopped '+R.cur.name+' for a better plan'); R.cur=null; R.op=null; }
        R.plan=f.r.tasks.slice(); R.mtr=f.mtr; R.partial=f.r.partial;
        for(const n of R.plan) this.applyEffects(n, R.ws, ['permanent'], R);
        this.say((replacing? 'New plan: ' : 'Plan: ')+R.plan.map(n=>n.name).join(' → ')+(R.partial? ' … (paused)' : ''));
        R.status='planned';
      }
      else if(!f && !replacing){ if(R.status!=='none') this.say('No plan: nothing in the domain can be done now'); R.status='none'; R.mtr=null; }
    }
    if(!R.cur && R.plan.length){
      const n=R.plan.shift();
      if(!this.condsOk(R, n.conds, R.ws)){ this.say(n.name+': its conditions no longer hold; planning again'); R.plan=[]; R.mtr=null; R.partial=null; return; }
      R.cur=n; R.op={t0:R.clock, did:false};
      this.say('Start: '+n.name+' ('+this.actText(n.do)+')');
      const st=this.simAct(R, n, true);
      if(st!=='continue'){ this.finish(R, n, st); return; }
      this.drawTree();
    }
    if(R.cur){
      const n=R.cur;
      if(!this.condsOk(R, n.exec, R.ws)){ this.say(n.name+': a while-running condition failed'); this.simStop(R, n); R.cur=null; R.op=null; R.plan=[]; R.mtr=null; R.partial=null; this.drawTree(); return; }
      const st=this.simAct(R, n, false);
      if(st!=='continue') this.finish(R, n, st);
    }
  },
  finish(R, n, st){
    if(st==='success'){
      const before=R.ws.slice();
      this.applyEffects(n, R.ws, ['run'], R);
      if(R.ws.some((x, i)=>x!==before[i])) R.dirty=true;
      this.say('Done: '+n.name);
    }
    else{ this.say('Failed: '+n.name+'; planning again'); this.simStop(R, n); R.plan=[]; R.mtr=null; R.partial=null; }
    R.cur=null; R.op=null;
    if(!R.plan.length && !R.partial) R.mtr=null;
    this.drawTree();
  },
  /** An action played on the actor through the Scripts' AI layer: 'continue', 'success' or
   *  'failure'. `first`: its start. */
  simAct(R, n, first){
    const S=WgScripts, A=n.do.args||{}, t=R.t, o=R.op, el=R.clock-o.t0;
    const ai=(kind, args)=>S.ai(t, kind, args.map(String), {self:t}, true);
    const clear=()=>{ R.A.ai=null; R.A._route=null; };
    const me=S.posOf(t);
    switch(n.do.block){
      case 'wait': if(first) clear(); return el>=+A.seconds? 'success' : 'continue';
      case 'wander': if(first) ai('wander', [A.distance, 0, 0]); if(el>=+A.seconds){ clear(); return 'success'; } return 'continue';
      case 'walk_to': {
        if(first) ai('travel', [A.x, A.y, A.z]);
        if(Math.hypot(me[0]-A.x, me[1]-A.y, me[2]-A.z)<150 || (R.A.ai && R.A.ai.done)){ clear(); return 'success'; }
        return el>120? (clear(), 'failure') : 'continue';
      }
      case 'walk_to_object': {
        if(first){
          const id=String(A.id).toLowerCase(), r=S.refById(id);
          if(!r){ this.say('No '+id+' is loaded'); return 'failure'; }
          o.goal=r.pos.slice(); ai('travel', o.goal);
        }
        if(!o.goal) return 'failure';
        if(Math.hypot(me[0]-o.goal[0], me[1]-o.goal[1])<+A.near){ clear(); return 'success'; }
        return el>120? (clear(), 'failure') : 'continue';
      }
      case 'go_to_player':
        if(first) ai('follow', ['player', 0]);
        if(S.dist(t, {player:true})<+A.near){ clear(); return 'success'; }
        return el>120? (clear(), 'failure') : 'continue';
      case 'follow_player': if(first) ai('follow', ['player', 0]); if(el>=+A.seconds){ clear(); return 'success'; } return 'continue';
      case 'flee_player': {
        if(first){
          const pl=S.posOf({player:true}); let x=me[0]-pl[0], y=me[1]-pl[1]; let l=Math.hypot(x, y); if(l<1){ x=1; y=0; l=1; }
          ai('travel', [pl[0]+x/l*(+A.distance+200), pl[1]+y/l*(+A.distance+200), me[2]]); R.A._running=true;
        }
        if(S.dist(t, {player:true})>=+A.distance){ clear(); return 'success'; }
        return el>30? (clear(), 'failure') : 'continue';
      }
      case 'use_item': { const id=String(A.id).toLowerCase(); if(!(R.A.items.get(id)>0)){ this.say(R.A.id+' carries no '+id); return 'failure'; } this.say(R.A.id+' uses '+id); return 'success'; }
      case 'attack_player': R.A.combat={player:true}; this.say(R.A.id+' attacks the player'); return 'success';
      case 'say': S.say(R.A.id, 'do', String(A.text)); if(typeof toast==='function') toast(String(A.text), 'info', 3000); return 'success';
      case 'play_anim': if(first){ clear(); S.playAnim(t.key, String(A.group), 1, 1, 'HTN'); } return el>=+A.seconds? 'success' : 'continue';
      case 'set_state': { const i=this.idx(R, A.var); if(i>=0 && R.ws[i]!==+A.value){ R.ws[i]=+A.value; R.dirty=true; } return 'success'; }
      case 'lua': if(first) this.say(n.name+': custom Lua is not run in the Preview; taken as done after a second'); return el>=1? 'success' : 'continue';
    }
    return 'failure';
  },
  simStop(R, n){
    if(!n || n.type!=='action') return;
    if(['wander', 'walk_to', 'walk_to_object', 'go_to_player', 'follow_player', 'flee_player'].includes(n.do.block)){ R.A.ai=null; R.A._route=null; }
  },

  /* ---- the page --------------------------------------------------------------------------- */
  el(tag, cls, text){ const e=document.createElement(tag); if(cls) e.className=cls; if(text!=null) e.textContent=text; return e; },
  btn(text, fn, title, cls){ const b=this.el('button', 'btn '+(cls||'sm'), text); b.onclick=fn; if(title) b.title=title; return b; },
  pane(){
    const col=this.el('div', 'col pane'); col.id='tlHtn'; col.hidden=true;
    const h=this.el('h3'); h.append('HTN AI ');
    h.append(this.el('span', 'sp'));
    const name=this.el('span', 'kind'); name.id='tlHtnName'; name.style.display='inline';
    const tpl=this.el('select'); tpl.id='tlHtnTpl'; tpl.title='The template New starts from';
    for(const [k, t] of this.TEMPLATES){ const o=this.el('option', '', t); o.value=k; tpl.append(o); }
    h.append(name, ' ',
      tpl,
      this.btn('New', ()=>this.newDoc(tpl.value), 'Start a new behaviour from the template chosen (Undo has the one open now back)'),
      this.btn('↶', ()=>this.undo(), 'Undo (Ctrl+Z)'), this.btn('↷', ()=>this.redo(), 'Redo (Ctrl+Y)'),
      this.btn('Open…', ()=>this.open(), 'Open a saved domain (*.wghtn.json)'),
      this.btn('Save', ()=>this.save(false), 'Save the domain'),
      this.btn('Save as…', ()=>this.save(true)),
      this.btn('Export mod…', ()=>this.exportMod(), 'Write the mod for the engines chosen into a folder', ''));
    const adv=this.el('label'); adv.style.marginLeft='8px';
    const ac=this.el('input'); ac.type='checkbox'; ac.id='tlHtnAdv'; adv.append(ac, ' Advanced');
    adv.title='Custom Lua blocks, effect kinds, while-running conditions and the generated Lua';
    const view=this.el('select'); view.id='tlHtnView'; view.title='The tree as a list, or as a graph of its branches';
    for(const [k, t] of [['list', 'List'], ['graph', 'Graph']]){ const o=this.el('option', '', t); o.value=k; view.append(o); }
    h.append(adv, ' ', view);
    const deps=this.el('div', 'chips'); deps.id='tlHtnDeps';
    const grid=this.el('div'); grid.id='tlHtnGrid'; grid.style.cssText='display:grid;grid-template-columns:minmax(260px,1fr) minmax(320px,1.3fr);gap:10px;min-height:0;flex:1';
    const left=this.el('div', 'scroll'); left.id='tlHtnTree';
    const right=this.el('div', 'scroll'); right.id='tlHtnSide';
    grid.append(left, right);
    col.append(h, deps, grid);
    return col;
  },
  wire(){
    if(!this._keys){
      this._keys=true;
      document.addEventListener('keydown', e=>{
        const pane=$('#tlHtn'); if(!pane || pane.hidden || !(e.ctrlKey || e.metaKey)) return;
        const t=e.target && e.target.tagName; if(t==='INPUT' || t==='TEXTAREA' || t==='SELECT') return;
        const k=String(e.key).toLowerCase();
        if(k==='z' && !e.shiftKey){ e.preventDefault(); this.undo(); }
        else if(k==='y' || (k==='z' && e.shiftKey)){ e.preventDefault(); this.redo(); }
      });
    }
    const vw=$('#tlHtnView');
    if(vw){ try{ this.view=localStorage.getItem('wgHtnView')||'list'; }catch(_){ } vw.value=this.view; vw.onchange=()=>{ this.view=vw.value; try{ localStorage.setItem('wgHtnView', this.view); }catch(_){ } this.draw(); }; }
    const ac=$('#tlHtnAdv');
    if(ac){ try{ this.adv=localStorage.getItem('wgHtnAdv')==='1'; }catch(_){ } ac.checked=this.adv; ac.onchange=()=>{ this.adv=ac.checked; try{ localStorage.setItem('wgHtnAdv', this.adv? '1' : '0'); }catch(_){ } this.draw(); }; }
  },
  show(){
    if(!this.doc && !this.restore()) this.doc=this.example();
    if(this._snap==null) this.fresh();
    if(!this.deps) this.loadDeps();
    this.draw();
  },
  /** A new behaviour from a template (`k`); the one open now can be had back with Undo. */
  newDoc(k){
    this.stopRun();
    this.doc=this.template(k||'empty'); this.path=''; this.sel='';
    this.changed();
  },
  draw(){
    if(!this.doc) return;
    const nm=$('#tlHtnName'); if(nm) nm.textContent=(this.doc.name||'')+(this.path? ' · '+this.path.split(/[\\/]/).pop() : ' · not saved');
    this.drawDeps(); this.drawTree(); this.drawSide();
  },
  drawDeps(){
    const box=$('#tlHtnDeps'); if(!box) return;
    box.textContent='';
    const D=this.deps, chip=(ok, text, title)=>{ const s=this.el('span', 'chip', (ok===true? '✓ ' : ok===false? '✗ ' : '? ')+text); if(title) s.title=title; s.style.opacity=ok===false? '1' : '.85'; if(ok===false) s.style.color='var(--warn, #d8a14a)'; box.append(s, ' '); };
    if(!D){ box.append('Checking what the export needs…'); return; }
    if(D.error){ box.append('Load a setup to check FluadHTN and Rubic0n ('+D.error+').'); return; }
    const d=this.doc||{engines:{}};
    if(d.engines.omw){
      chip(D.omw, 'FluadHTN (OpenMW)', D.omw? 'scripts/sb_htn/interface.lua is in the data folders' : 'Install FluadHTN\'s OpenMW build: '+this.NEXUS_FLUADHTN);
      chip(D.rubic0n==null? null : D.rubic0n, 'Rubic0n', D.rubic0n? 'OpenMW loads Rubic0n\'s LuaJIT ('+D.luajit+')' : D.luajit? 'OpenMW loads the stock LuaJIT ('+D.luajit+'); FluadHTN\'s OpenMW build needs Rubic0n: '+this.NEXUS_RUBIC0N : 'No OpenMW install was found to look at (Settings: OpenMW folder)');
    }
    if(d.engines.mwse) chip(D.mwse, 'FluadHTN (MWSE)', D.mwse? D.mwse_path : 'Install FluadHTN\'s MWSE build: '+this.NEXUS_FLUADHTN);
    box.append(this.btn('Check again', ()=>this.loadDeps()));
  },

  /* The tree: each node a row - what it is, its name, its conditions in short - the domain
     first, then each reusable part. A row is dragged to move it (pointer events: the shell's
     webview hands HTML drag-and-drop to the window as file drops): onto a Select or Sequence's
     right half it goes in, else above or below the row. The running step is marked during a
     Preview run. */
  drawTree(){
    const box=$('#tlHtnTree'); if(!box || !this.doc) return;
    box.textContent='';
    const grid=$('#tlHtnGrid'); if(grid) grid.style.gridTemplateColumns=this.view==='graph'? 'minmax(360px,2fr) minmax(320px,1fr)' : 'minmax(260px,1fr) minmax(320px,1.3fr)';
    if(this.view==='graph'){ this.drawGraph(box); return; }
    const probs=this.validate(), bad=new Map();
    for(const p of probs) if(p.at) bad.set(p.at, (bad.get(p.at)==='error' || p.level==='error')? 'error' : 'warn');
    const R=this.run, fl=R && R.mode==='fluad';
    const byName=nm=>{ let id=''; if(nm) this.walk(n=>{ if(!id && n.type==='action' && n.name===nm) id=n.id; }); return id; };
    const cur=R && (fl? byName(R.curName) : R.cur && R.cur.id), planned=new Set(R? (fl? R.plan.map(byName) : R.plan.map(n=>n.id)) : []);
    const row=(n, parent, depth, isPart)=>{
      const r=this.el('div', 'wgHtnRow');
      r.dataset.id=n.id;
      r.style.cssText='display:flex;gap:6px;align-items:center;padding:2px 4px;cursor:pointer;border-radius:4px;user-select:none;margin-left:'+(depth*16)+'px';
      if(n.id===this.sel) r.style.background='var(--sel, rgba(120,160,255,.18))';
      if(n.id===cur) r.style.outline='2px solid var(--ac)';
      else if(planned.has(n.id)) r.style.outline='1px dashed var(--ac)';
      const tag=this.el('span', 'kind', isPart? 'Part' : this.NODE_TYPES[n.type]); tag.style.cssText='display:inline;min-width:64px;opacity:.75';
      const nm=this.el('b', '', n.type==='pause'? '⏸' : (n.name||''));
      r.append(tag, nm);
      if(n.type==='action') r.append(this.el('span', 'hint', this.actText(n.do)));
      if(n.type==='splice'){ const x=this.sub(n.sub); r.append(this.el('span', 'hint', x? 'uses "'+x.name+'"' : 'uses nothing yet')); }
      if((n.conds||[]).length) r.append(this.el('span', 'hint', 'if '+n.conds.map(c=>this.checkText(c)).join(' and ')));
      if(bad.has(n.id)){ const m=this.el('span', '', bad.get(n.id)==='error'? '⛔' : '⚠'); m.title=probs.filter(p=>p.at===n.id).map(p=>p.text).join('\n'); r.append(m); }
      r.onclick=()=>{ if(this._dragged) return; this.sel=n.id; this.drawTree(); this.drawSide(); };
      if(parent) r.onpointerdown=e=>this.dragStart(e, n.id);
      box.append(r);
    };
    this.walk((n, parent, depth)=>row(n, parent, depth, false), this.doc.root);
    const partsHead=this.el('div', 'hint', 'Reusable parts: a Select you build once and use in several places ("Use part").');
    partsHead.style.margin='10px 0 2px';
    box.append(partsHead);
    for(const x of this.doc.subs||[]) this.walk((n, parent, depth)=>row(n, parent, depth, !parent), x);
    box.append(this.btn('+ Reusable part', ()=>{ const x=this.node('select', 'Part '+((this.doc.subs||[]).length+1)); (this.doc.subs=this.doc.subs||[]).push(x); this.sel=x.id; this.changed(); }));
    const add=this.el('div'); add.style.marginTop='8px';
    const into=this.find(this.sel), target=into && (into.n.children? into.n : into.parent) || this.doc.root;
    add.append(this.el('span', 'hint', 'Add to "'+(target.name||this.NODE_TYPES[target.type])+'": '));
    for(const t of ['action', 'select', 'sequence', 'pause', 'splice']){
      if(t==='pause' && target.type!=='sequence') continue;
      if(t==='splice' && !(this.doc.subs||[]).length) continue;
      add.append(this.btn('+ '+this.NODE_TYPES[t], ()=>{ const n=this.node(t); if(t==='splice'){ const x=this.doc.subs.find(y=>y!==this.partOf(target.id)); n.sub=x? x.id : ''; n.name=x? x.name : 'Use part'; } target.children.push(n); this.sel=n.id; this.changed(); }));
    }
    box.append(add);
  },
  /* ---- the graph view ----------------------------------------------------------------------
     The domain drawn top-down as boxes joined by branches (SVG, built with the DOM): a Select's
     branches numbered in the order they are tried, a Sequence's in the order they run; each
     reusable part beside it as its own tree. Click a box to edit it; drag it onto another box
     (onto a Select or Sequence's lower half to go in, else to its left or right); the + under a
     Select or Sequence adds a step; double-click folds one away. Wheel zooms, dragging the
     background pans, Fit shows it all. */
  view:'list', fold:new Set(), gv:null,
  GW:172, GH:48, GX:18, GY:58,
  NS:'http://www.w3.org/2000/svg',
  svg(tag, attrs, text){ const e=document.createElementNS(this.NS, tag); for(const k in attrs||{}) e.setAttribute(k, attrs[k]); if(text!=null) e.textContent=text; return e; },
  /** Where each box goes: {id: {x, y, n, parent, depth, part}} and the drawing's width and height. */
  layout(){
    const pos=new Map(), W=this.GW, H=this.GH, GX=this.GX, GY=this.GY;
    let left=0, bottom=0;
    const place=(n, parent, depth, part)=>{
      const kids=(n.children && !this.fold.has(n.id))? n.children : [];
      let x;
      if(!kids.length){ x=left+W/2; left+=W+GX; }
      else{
        const xs=kids.map(c=>place(c, n, depth+1, part));
        x=(xs[0]+xs[xs.length-1])/2;
      }
      const y=depth*(H+GY);
      bottom=Math.max(bottom, y+H);
      pos.set(n.id, {x, y, n, parent, depth, part});
      return x;
    };
    place(this.doc.root, null, 0, null);
    for(const x of this.doc.subs||[]){ left+=W/2; place(x, null, 0, x); }
    return {pos, w:Math.max(left, W), h:bottom};
  },
  drawGraph(box){
    box.style.position='relative';
    const bar=this.el('div'); bar.style.cssText='display:flex;gap:6px;align-items:center;margin-bottom:4px';
    bar.append(this.btn('Fit', ()=>{ this.gv=null; this.drawGraph2(); }, 'Show it all'), this.btn('−', ()=>this.zoom(1/1.25)), this.btn('+', ()=>this.zoom(1.25)),
      this.btn('+ Reusable part', ()=>{ const x=this.node('select', 'Part '+((this.doc.subs||[]).length+1)); (this.doc.subs=this.doc.subs||[]).push(x); this.sel=x.id; this.changed(); }),
      this.el('span', 'hint', 'Click to edit · drag onto a box to move · + adds a step · double-click folds'));
    const host=this.el('div'); host.id='tlHtnGraph'; host.style.cssText='position:relative;flex:1;min-height:420px;height:calc(100% - 34px);overflow:hidden;border:1px solid var(--line, rgba(128,128,128,.3));border-radius:6px;touch-action:none';
    box.append(bar, host);
    this.drawGraph2();
  },
  zoom(f, cx, cy){
    const host=$('#tlHtnGraph'); if(!host || !this.gv) return;
    const r=host.getBoundingClientRect(); cx=cx==null? r.width/2 : cx; cy=cy==null? r.height/2 : cy;
    const k=Math.max(0.2, Math.min(3, this.gv.k*f)), q=k/this.gv.k;
    this.gv={x:cx-(cx-this.gv.x)*q, y:cy-(cy-this.gv.y)*q, k};
    const g=host.querySelector('g[data-view]'); if(g) g.setAttribute('transform', 'translate('+this.gv.x+','+this.gv.y+') scale('+this.gv.k+')');
  },
  drawGraph2(){
    const host=$('#tlHtnGraph'); if(!host) return;
    host.textContent='';
    const L=this.layout(), W=this.GW, H=this.GH;
    const probs=this.validate(), bad=new Map();
    for(const p of probs) if(p.at) bad.set(p.at, (bad.get(p.at)==='error' || p.level==='error')? 'error' : 'warn');
    const R=this.run, fl=R && R.mode==='fluad';
    const byName=nm=>{ let id=''; if(nm) this.walk(n=>{ if(!id && n.type==='action' && n.name===nm) id=n.id; }); return id; };
    const cur=R && (fl? byName(R.curName) : R.cur && R.cur.id), planned=new Set(R? (fl? R.plan.map(byName) : R.plan.map(n=>n.id)) : []);
    const box=host.getBoundingClientRect(), bw=box.width||800, bh=box.height||420;
    if(!this.gv){ const k=Math.max(0.2, Math.min(1, (bw-40)/L.w, (bh-40)/(L.h||1))); this.gv={x:(bw-L.w*k)/2, y:20, k}; }
    const svg=this.svg('svg', {width:'100%', height:'100%'});
    svg.style.cssText='display:block;width:100%;height:100%;cursor:grab;user-select:none';
    const g=this.svg('g', {'data-view':'1', transform:'translate('+this.gv.x+','+this.gv.y+') scale('+this.gv.k+')'});
    svg.append(g);
    const COL={select:'#6f9cf0', sequence:'#6cc38a', action:'#c9a85c', pause:'#999', splice:'#c07de0'};
    // Branches first, under the boxes.
    for(const [id, p] of L.pos){
      if(!p.parent) continue;
      const q=L.pos.get(p.parent.id); if(!q) continue;
      const midY=q.y+H+this.GY/2;
      g.append(this.svg('path', {d:'M'+q.x+','+(q.y+H)+' V'+midY+' H'+p.x+' V'+p.y, fill:'none', stroke:COL[p.parent.type]||'#888', 'stroke-width':planned.has(id) || cur===id? 3 : 1.5, opacity:.8}));
      const i=p.parent.children.indexOf(p.n)+1;
      const c=this.svg('g', {transform:'translate('+p.x+','+(p.y-11)+')'});
      c.append(this.svg('circle', {r:9, fill:COL[p.parent.type]||'#888'}), this.svg('text', {'text-anchor':'middle', y:4, 'font-size':10, fill:'#fff'}, String(i)));
      const tt=this.svg('title', {}, p.parent.type==='select'? 'Tried '+(i===1? 'first' : 'next ('+i+')') : 'Step '+i);
      c.append(tt); g.append(c);
    }
    // Part names above each part's tree.
    for(const x of this.doc.subs||[]){ const p=L.pos.get(x.id); if(p) g.append(this.svg('text', {x:p.x, y:p.y-8, 'text-anchor':'middle', 'font-size':11, fill:'currentColor', opacity:.7}, 'Reusable part')); }
    for(const [id, p] of L.pos){
      const n=p.n, x0=p.x-W/2;
      const b=this.svg('g', {'data-id':id, transform:'translate('+x0+','+p.y+')', class:'wgHtnBox'});
      b.style.cursor='pointer';
      const isRoot=!p.parent;
      b.append(this.svg('rect', {width:W, height:H, rx:7, fill:'var(--bg2, #222)', stroke:COL[n.type]||'#888', 'stroke-width':id===this.sel? 3 : 1.5}));
      if(id===cur) b.append(this.svg('rect', {x:-4, y:-4, width:W+8, height:H+8, rx:9, fill:'none', stroke:'var(--ac, #4af)', 'stroke-width':2.5}));
      else if(planned.has(id)) b.append(this.svg('rect', {x:-3, y:-3, width:W+6, height:H+6, rx:8, fill:'none', stroke:'var(--ac, #4af)', 'stroke-dasharray':'4 3'}));
      const kind=isRoot && p.part? 'Part' : this.NODE_TYPES[n.type];
      const icon={select:'?', sequence:'→', action:'●', pause:'⏸', splice:'⧉'}[n.type]||'';
      b.append(this.svg('text', {x:8, y:15, 'font-size':10, fill:COL[n.type]||'#888'}, icon+' '+kind+(this.fold.has(id)? '  (+'+this.count(n)+' folded)' : '')));
      const name=n.type==='pause'? 'Pause plan' : (n.name||'');
      b.append(this.svg('text', {x:8, y:30, 'font-size':12, 'font-weight':600, fill:'currentColor'}, name.length>24? name.slice(0, 23)+'…' : name));
      let sub='';
      if(n.type==='action') sub=this.actText(n.do);
      else if(n.type==='splice'){ const x=this.sub(n.sub); sub=x? 'uses "'+x.name+'"' : 'uses nothing yet'; }
      if((n.conds||[]).length) sub=(sub? sub+' · ' : '')+'if '+n.conds.map(c=>this.checkText(c)).join(' & ');
      if(sub) b.append(this.svg('text', {x:8, y:43, 'font-size':9.5, fill:'currentColor', opacity:.65}, sub.length>34? sub.slice(0, 33)+'…' : sub));
      if(bad.has(id)){ const m=this.svg('text', {x:W-16, y:15, 'font-size':12}, bad.get(id)==='error'? '⛔' : '⚠'); m.append(this.svg('title', {}, probs.filter(q=>q.at===id).map(q=>q.text).join('\n'))); b.append(m); }
      b.append(this.svg('title', {}, kind+': '+name+(sub? '\n'+sub : '')));
      // The + under a Select or Sequence: a step added to it.
      if(n.children && !this.fold.has(id)){
        const plus=this.svg('g', {transform:'translate('+(W/2)+','+(H+9)+')', 'data-plus':id});
        plus.append(this.svg('circle', {r:8, fill:'var(--bg, #111)', stroke:COL[n.type], 'stroke-width':1.5}), this.svg('text', {'text-anchor':'middle', y:4, 'font-size':12, fill:COL[n.type]}, '+'));
        plus.append(this.svg('title', {}, 'Add a step to '+(name||kind)));
        plus.onpointerdown=e=>e.stopPropagation();
        plus.onclick=e=>{ e.stopPropagation(); this.addMenu(n, e); };
        b.append(plus);
      }
      b.onpointerdown=e=>this.graphDrag(e, id, !isRoot);
      b.ondblclick=e=>{ e.stopPropagation(); if(n.children){ if(this.fold.has(id)) this.fold.delete(id); else this.fold.add(id); this.drawGraph2(); } };
      g.append(b);
    }
    // Pan and zoom.
    svg.onpointerdown=e=>{
      if(e.button!==0) return;
      const x0=e.clientX, y0=e.clientY, v0=Object.assign({}, this.gv);
      const mv=ev=>{ this.gv={x:v0.x+ev.clientX-x0, y:v0.y+ev.clientY-y0, k:v0.k}; g.setAttribute('transform', 'translate('+this.gv.x+','+this.gv.y+') scale('+this.gv.k+')'); };
      const up=()=>{ document.removeEventListener('pointermove', mv); document.removeEventListener('pointerup', up); };
      document.addEventListener('pointermove', mv); document.addEventListener('pointerup', up);
    };
    svg.onwheel=e=>{ e.preventDefault(); const r=host.getBoundingClientRect(); this.zoom(e.deltaY<0? 1.15 : 1/1.15, e.clientX-r.left, e.clientY-r.top); };
    host.append(svg);
  },
  /** How many nodes are below `n`. */
  count(n){ let c=-1; this.walk(()=>c++, n); return c; },
  /** The step menu by a + : Action, Select, Sequence, Pause (in a Sequence), Use part. */
  addMenu(target, e){
    const host=$('#tlHtnGraph'); if(!host) return;
    const old=host.querySelector('.wgHtnMenu'); if(old) old.remove();
    const r=host.getBoundingClientRect();
    const m=this.el('div', 'wgHtnMenu');
    m.style.cssText='position:absolute;z-index:5;background:var(--bg2, #222);border:1px solid var(--line, #555);border-radius:6px;padding:4px;display:flex;flex-direction:column;gap:2px;left:'+Math.max(0, (e.clientX||0)-r.left)+'px;top:'+Math.max(0, (e.clientY||0)-r.top)+'px';
    for(const t of ['action', 'select', 'sequence', 'pause', 'splice']){
      if(t==='pause' && target.type!=='sequence') continue;
      if(t==='splice' && !(this.doc.subs||[]).some(x=>x!==this.partOf(target.id))) continue;
      m.append(this.btn('+ '+this.NODE_TYPES[t], ()=>{
        m.remove();
        const n=this.node(t);
        if(t==='splice'){ const x=this.doc.subs.find(y=>y!==this.partOf(target.id)); n.sub=x.id; n.name=x.name; }
        target.children.push(n); this.sel=n.id; this.changed();
      }));
    }
    m.append(this.btn('Cancel', ()=>m.remove()));
    host.append(m);
  },
  /** A box dragged onto another: into a Select or Sequence (the lower half of its box), else
   *  beside it (left half: before; right half: after). A click when it did not move. */
  graphDrag(e, id, movable){
    if(e.button!==0) return;
    e.stopPropagation();
    const host=$('#tlHtnGraph'); if(!host) return;
    const box=host.querySelector('[data-id="'+(window.CSS && CSS.escape? CSS.escape(id) : id)+'"]');
    const x0=e.clientX, y0=e.clientY, t0=box && box.getAttribute('transform');
    let on=false, where=null, mark=null;
    const clear=()=>{ if(mark){ mark.remove(); mark=null; } };
    const move=ev=>{
      if(!movable) return;
      if(!on && Math.hypot(ev.clientX-x0, ev.clientY-y0)<5) return;
      on=true;
      if(box){ box.style.pointerEvents='none'; box.style.opacity='.6'; const k=this.gv.k; const m=/translate\(([-\d.]+),([-\d.]+)\)/.exec(t0||''); if(m) box.setAttribute('transform', 'translate('+(+m[1]+(ev.clientX-x0)/k)+','+(+m[2]+(ev.clientY-y0)/k)+')'); }
      clear(); where=null;
      const el=document.elementFromPoint? document.elementFromPoint(ev.clientX, ev.clientY) : null;
      const tg=el && el.closest && el.closest('[data-id]'); if(!tg || tg.dataset.id===id) return;
      where=this.graphWhere(tg.dataset.id, tg.getBoundingClientRect(), ev.clientX, ev.clientY);
      if(!where) return;
      const rect=tg.querySelector('rect');
      mark=this.svg('rect', {x:-5, y:-5, width:this.GW+10, height:this.GH+10, rx:9, fill:'none', stroke:'var(--ac, #4af)', 'stroke-width':3, 'stroke-dasharray':where.inside? '' : '6 4'});
      if(!where.inside){ mark.setAttribute('width', 6); mark.setAttribute('x', where.after? this.GW+2 : -8); }
      if(rect) tg.append(mark);
    };
    const up=()=>{
      document.removeEventListener('pointermove', move); document.removeEventListener('pointerup', up);
      clear();
      if(!on){ this.sel=id; this.drawTree(); this.drawSide(); return; }
      if(!(where && this.moveTo(id, where))) this.drawGraph2();
    };
    document.addEventListener('pointermove', move); document.addEventListener('pointerup', up);
  },
  /** Where a box let go over `to` goes: into it (a Select or Sequence, the lower half), else
   *  before or after it by the side; nothing beside a root. */
  graphWhere(to, r, x, y){
    const t=this.find(to); if(!t) return null;
    if(t.n.children && y>r.top+r.height/2) return {to, inside:true};
    if(!t.parent) return t.n.children? {to, inside:true} : null;
    return {to, inside:false, after:x>r.left+r.width/2};
  },

  /** The reusable part a node is in (null: the domain itself). */
  partOf(id){ for(const x of this.doc.subs||[]){ let hit=false; this.walk(n=>{ if(n.id===id) hit=true; }, x); if(hit) return x; } return null; },
  /** Whether `id` is `anc` or below it. */
  under(id, anc){ let hit=false; this.walk(n=>{ if(n.id===id) hit=true; }, anc); return hit; },
  /** A row dragged: past a few pixels it moves with the pointer, and where it would go is
   *  shown; let go, it goes there (`moveTo`). */
  dragStart(e, id){
    if(e.button!==0) return;
    const box=$('#tlHtnTree'); if(!box) return;
    const x0=e.clientX, y0=e.clientY;
    let on=false, where=null;
    this._dragged=false;
    const clear=()=>{ for(const r of box.querySelectorAll('.wgHtnRow')){ r.style.boxShadow=''; } };
    const move=ev=>{
      if(!on && Math.hypot(ev.clientX-x0, ev.clientY-y0)<5) return;
      on=true; this._dragged=true;
      clear(); where=null;
      const el=document.elementFromPoint? document.elementFromPoint(ev.clientX, ev.clientY) : null;
      const r=el && el.closest && el.closest('.wgHtnRow'); if(!r || r.dataset.id===id) return;
      const t=this.find(r.dataset.id); if(!t) return;
      const b=r.getBoundingClientRect();
      const inside=!!t.n.children && ev.clientX>b.left+b.width/2;
      const after=!inside && ev.clientY>b.top+b.height/2;
      if(!t.parent && !inside) return;   // nothing goes beside a root
      where={to:r.dataset.id, inside, after};
      r.style.boxShadow=inside? 'inset 0 0 0 2px var(--ac)' : after? 'inset 0 -2px 0 var(--ac)' : 'inset 0 2px 0 var(--ac)';
    };
    const up=()=>{
      document.removeEventListener('pointermove', move); document.removeEventListener('pointerup', up);
      clear();
      if(on && where) this.moveTo(id, where);
      setTimeout(()=>{ this._dragged=false; }, 0);
    };
    document.addEventListener('pointermove', move); document.addEventListener('pointerup', up);
  },
  /** A node moved: into `to` (a Select or Sequence, last), or before or after it. Not into
   *  itself or below itself; a Pause only into a Sequence. */
  moveTo(id, w){
    const f=this.find(id), t=this.find(w.to);
    if(!f || !f.parent || !t || this.under(w.to, f.n)) return false;
    const dest=w.inside? t.n : t.parent;
    if(!dest || !dest.children) return false;
    if(f.n.type==='pause' && dest.type!=='sequence') return false;
    f.parent.children.splice(f.parent.children.indexOf(f.n), 1);
    if(w.inside) dest.children.push(f.n);
    else dest.children.splice(dest.children.indexOf(t.n)+(w.after? 1 : 0), 0, f.n);
    this.sel=id; this.changed();
    return true;
  },
  /** Move a node among its siblings (`d` -1/1), or out (`out`) or into the sibling above. */
  move(id, d){
    const f=this.find(id); if(!f || !f.parent) return;
    const sib=f.parent.children, i=sib.indexOf(f.n), j=i+d;
    if(j<0 || j>=sib.length) return;
    sib.splice(i, 1); sib.splice(j, 0, f.n); this.changed();
  },
  remove(id){
    const f=this.find(id); if(!f || !f.parent) return;
    f.parent.children.splice(f.parent.children.indexOf(f.n), 1);
    this.sel=f.parent.id; this.changed();
  },
  duplicate(id){
    const f=this.find(id); if(!f || !f.parent) return;
    const c=JSON.parse(JSON.stringify(f.n)); const re=n=>{ n.id=this.id(); (n.children||[]).forEach(re); }; re(c);
    c.name=(c.name||'')+' (copy)';
    f.parent.children.splice(f.parent.children.indexOf(f.n)+1, 0, c); this.sel=c.id; this.changed();
  },

  /* The side: the selected node's inspector, or the domain's settings and world state; the
     problems; the Preview run; the generated Lua (Advanced). */
  drawSide(){
    const box=$('#tlHtnSide'); if(!box || !this.doc) return;
    box.textContent='';
    const f=this.sel && this.find(this.sel);
    if(f && f.n!==this.doc.root) box.append(this.inspector(f.n, f.parent));
    else box.append(this.settings(), this.stateEditor());
    const pr=this.el('div'); pr.id='tlHtnProbs'; box.append(pr); this.drawProblems();
    const run=this.el('div'); run.id='tlHtnRun'; box.append(run); this.drawRun();
    if(this.adv) box.append(this.luaView());
  },
  section(title){ const w=this.el('div', 'warn'); w.style.borderLeftColor='var(--ac)'; w.style.marginBottom='8px'; w.append(this.el('b', '', title)); return w; },
  line(label, input, hint){
    const r=this.el('div'); r.style.cssText='display:flex;gap:6px;align-items:center;margin:4px 0';
    const l=this.el('span', '', label); l.style.minWidth='110px'; r.append(l, input);
    if(hint) r.append(this.el('span', 'hint', hint));
    return r;
  },
  input(v, on, type, w){
    const i=this.el('input'); if(type) i.type=type; if(type==='number') i.step='any';
    i.value=v==null? '' : String(v); if(w) i.style.width=w;
    i.onchange=()=>on(type==='number'? +i.value : i.value);
    return i;
  },
  settings(){
    const d=this.doc, w=this.section('Behaviour');
    w.append(this.line('Name', this.input(d.name, v=>{ d.name=v; d.root.name=v; this.changed(); }, 'text', '220px')));
    w.append(this.line('Mod id', this.input(d.modId, v=>{ d.modId=String(v).trim().toLowerCase(); this.changed(); }, 'text', '180px'), 'names the exported folders'));
    const eng=this.el('span');
    for(const [k, t] of [['omw', 'OpenMW'], ['mwse', 'MWSE']]){
      const l=this.el('label'), c=this.el('input'); c.type='checkbox'; c.checked=!!d.engines[k];
      c.onchange=()=>{ d.engines[k]=c.checked; this.changed(); };
      l.append(c, ' '+t+' '); eng.append(l);
    }
    w.append(this.line('Export for', eng));
    const ids=this.el('textarea'); ids.rows=2; ids.style.width='100%'; ids.value=d.attach.ids.join('\n'); ids.placeholder='Record ids, one a line (e.g. imperial guard)';
    ids.onchange=()=>{ d.attach.ids=ids.value.split(/\r?\n|,/).map(s=>s.trim()).filter(Boolean); this.changed(); };
    const all=this.el('label'), ac=this.el('input'); ac.type='checkbox'; ac.checked=!!d.attach.all; ac.onchange=()=>{ d.attach.all=ac.checked; this.changed(); };
    all.append(ac, ' every NPC and creature');
    w.append(this.line('Actors', ids), this.line('', all));
    w.append(this.line('Plan every', this.input(d.tick, v=>{ d.tick=v; this.changed(); }, 'number', '70px'), 'seconds'));
    return w;
  },
  stateEditor(){
    const d=this.doc, w=this.section('World state');
    w.append(this.el('div', 'hint', 'Values the behaviour plans with. A watched value is set from the game every tick, and a change makes the actor plan again.'));
    d.state.forEach((v, i)=>{
      const r=this.el('div'); r.style.cssText='display:flex;gap:6px;align-items:center;margin:4px 0;flex-wrap:wrap';
      const nm=this.input(v.name, x=>{ const old=v.name; v.name=String(x).trim(); this.rename(old, v.name); this.changed(); }, 'text', '130px');
      const kind=this.el('select');
      for(const [k, t] of [['bool', 'true/false'], ['number', 'number']]){ const o=this.el('option', '', t); o.value=k; kind.append(o); }
      kind.value=v.kind; kind.onchange=()=>{ v.kind=kind.value; if(v.kind!=='bool') v.sensor=null; this.changed(); };
      let init;
      if(v.kind==='bool'){ init=this.el('select'); for(const [k, t] of [['0', 'starts false'], ['1', 'starts true']]){ const o=this.el('option', '', t); o.value=k; init.append(o); } init.value=+v.init? '1' : '0'; init.onchange=()=>{ v.init=+init.value; this.changed(); }; }
      else init=this.input(v.init, x=>{ v.init=x; this.changed(); }, 'number', '70px');
      r.append(nm, kind, init);
      if(v.kind==='bool'){
        const watch=this.el('select');
        const none=this.el('option', '', 'set by actions'); none.value=''; watch.append(none);
        for(const [k, def] of Object.entries(this.CHECKS)){ if(k==='state' || (def.adv && !this.adv)) continue; const o=this.el('option', '', 'watched: '+def.label.toLowerCase()); o.value=k; watch.append(o); }
        watch.value=v.sensor? v.sensor.block : '';
        watch.onchange=()=>{ v.sensor=watch.value? this.check(watch.value) : null; this.changed(); };
        r.append(watch);
        if(v.sensor) r.append(...this.fields(v.sensor, this.CHECKS[v.sensor.block]));
      }
      r.append(this.btn('✕', ()=>{ d.state.splice(i, 1); this.changed(); }, 'Remove this value'));
      w.append(r);
    });
    w.append(this.btn('+ Value', ()=>{ let k=1; while(d.state.some(v=>v.name==='Value'+k)) k++; d.state.push({name:'Value'+k, kind:'bool', init:0, sensor:null}); this.changed(); }));
    return w;
  },
  /** A value renamed: every reference to it follows. */
  rename(a, b){
    if(!a || a===b) return;
    const fix=c=>{ if(c && c.args && c.args.var===a) c.args.var=b; };
    this.walk(n=>{ (n.conds||[]).forEach(fix); (n.exec||[]).forEach(fix); if(n.do) fix(n.do); for(const e of n.effects||[]) if(e.var===a) e.var=b; });
  },
  /** A block's fields as inputs. */
  fields(b, def){
    const out=[];
    for(const [k, label, type] of def.fields){
      const on=v=>{ b.args[k]=v; this.changed(); };
      let i;
      if(type==='state'){
        i=this.el('select');
        const z=this.el('option', '', '(choose a value)'); z.value=''; i.append(z);
        for(const v of this.doc.state){ const o=this.el('option', '', v.name); o.value=v.name; i.append(o); }
        i.value=b.args[k]||''; i.onchange=()=>on(i.value);
      }
      else if(type==='op'){ i=this.el('select'); for(const o of this.OPS){ const e=this.el('option', '', o); e.value=o; i.append(e); } i.value=b.args[k]; i.onchange=()=>on(i.value); }
      else if(type==='val'){
        const v=this.doc.state.find(s=>s.name===b.args.var);
        if(v && v.kind==='bool'){ i=this.el('select'); for(const [x, t] of [['1', 'true'], ['0', 'false']]){ const o=this.el('option', '', t); o.value=x; i.append(o); } i.value=+b.args[k]? '1' : '0'; i.onchange=()=>on(+i.value); }
        else i=this.input(b.args[k], on, 'number', '70px');
      }
      else if(type==='lua'){ i=this.el('textarea'); i.rows=3; i.style.cssText='width:100%;font-family:var(--mono);font-size:11px'; i.value=b.args[k]||''; i.onchange=()=>on(i.value); }
      else i=this.input(b.args[k], on, type==='num'? 'number' : 'text', type==='num'? '80px' : '170px');
      i.title=label;
      const l=this.el('label'); l.style.cssText=type==='lua'? 'display:block;width:100%' : 'display:inline-flex;gap:4px;align-items:center';
      l.append(this.el('span', 'hint', label), i);
      out.push(l);
    }
    return out;
  },
  /** A list of checks (conditions, or while-running ones), with add and remove. */
  checkList(list, title, hint){
    const w=this.section(title);
    if(hint) w.append(this.el('div', 'hint', hint));
    list.forEach((c, i)=>{
      const r=this.el('div'); r.style.cssText='display:flex;gap:6px;align-items:center;margin:4px 0;flex-wrap:wrap';
      const not=this.el('select'); for(const [k, t] of [['', 'if'], ['1', 'if not']]){ const o=this.el('option', '', t); o.value=k; not.append(o); }
      not.value=c.not? '1' : ''; not.onchange=()=>{ c.not=!!not.value; this.changed(); };
      const kind=this.el('select');
      for(const [k, def] of Object.entries(this.CHECKS)){ if(def.adv && !this.adv && c.block!==k) continue; const o=this.el('option', '', def.label); o.value=k; kind.append(o); }
      kind.value=c.block; kind.onchange=()=>{ list[i]=this.check(kind.value, null, c.not); this.changed(); };
      kind.title=(this.CHECKS[c.block]||{}).help||'';
      r.append(not, kind, ...this.fields(c, this.CHECKS[c.block]), this.btn('✕', ()=>{ list.splice(i, 1); this.changed(); }));
      w.append(r);
    });
    w.append(this.btn('+ Condition', ()=>{ list.push(this.check(this.doc.state.length? 'state' : 'player_within', this.doc.state.length? {var:this.doc.state[0].name} : null)); this.changed(); }));
    return w;
  },
  inspector(n, parent){
    if(!parent) return this.partInspector(n);
    if(n.type==='splice') return this.spliceInspector(n);
    const frag=document.createDocumentFragment();
    const w=this.section(this.NODE_TYPES[n.type]);
    const help={select:'Does the first thing below it that can be done, from the top.', sequence:'Does everything below it, in order; it can only be done if every step can.',
      action:'One thing the actor does.', pause:'Plans only up to here; the rest is planned once the steps before it are done (a long walk, then decide).'}[n.type];
    w.append(this.el('div', 'hint', help));
    if(n.type!=='pause') w.append(this.line('Name', this.input(n.name, v=>{ n.name=v; this.changed(); }, 'text', '220px')));
    const bar=this.el('div'); bar.style.margin='6px 0';
    bar.append(this.btn('↑', ()=>this.move(n.id, -1), 'Earlier (in a Select, earlier is tried first)'), ' ', this.btn('↓', ()=>this.move(n.id, 1), 'Later'), ' ',
      this.btn('Duplicate', ()=>this.duplicate(n.id)), ' ', this.btn('Remove', ()=>this.remove(n.id)), ' ', this.btn('Done', ()=>{ this.sel=''; this.draw(); }, 'Back to the behaviour\'s settings'));
    w.append(bar);
    frag.append(w);
    if(n.type==='pause') return frag;
    if(n.type==='action'){
      const a=this.section('Does');
      const kind=this.el('select');
      for(const [k, def] of Object.entries(this.ACTIONS)){ if(def.adv && !this.adv && n.do.block!==k) continue; const o=this.el('option', '', def.label); o.value=k; kind.append(o); }
      kind.value=n.do.block; kind.onchange=()=>{ n.do=this.act(kind.value); this.changed(); };
      const def=this.ACTIONS[n.do.block];
      a.append(kind, ' ', this.el('span', 'hint', def? def.help : ''));
      if(def){ const fs=this.el('div'); fs.style.cssText='display:flex;gap:8px;flex-wrap:wrap;margin-top:4px'; fs.append(...this.fields(n.do, def)); a.append(fs); }
      if(def && def.here) a.append(this.btn('Use the camera\'s target', ()=>this.here(n.do), 'Where the Preview\'s camera looks'));
      frag.append(a);
    }
    frag.append(this.checkList(n.conds, 'Only when', n.type==='action'? 'Checked while planning, and again as it starts.' : 'Checked while planning.'));
    if(n.type==='action'){
      if(this.adv) frag.append(this.checkList(n.exec, 'While running, stop unless', 'Checked every tick while it runs; failing stops the plan.'));
      const e=this.section('Then');
      e.append(this.el('div', 'hint', 'What it changes in the world state when it is done; counted while planning, so later steps can rely on it.'));
      n.effects.forEach((x, i)=>{
        const r=this.el('div'); r.style.cssText='display:flex;gap:6px;align-items:center;margin:4px 0;flex-wrap:wrap';
        const op=this.el('select'); for(const [k, t] of [['set', 'set'], ['add', 'add to']]){ const o=this.el('option', '', t); o.value=k; op.append(o); }
        op.value=x.op||'set'; op.onchange=()=>{ x.op=op.value; this.changed(); };
        const v=this.el('select'); const z=this.el('option', '', '(choose a value)'); z.value=''; v.append(z);
        for(const s of this.doc.state){ const o=this.el('option', '', s.name); o.value=s.name; v.append(o); }
        v.value=x.var||''; v.onchange=()=>{ x.var=v.value; this.changed(); };
        const st=this.doc.state.find(s=>s.name===x.var);
        let val;
        if(st && st.kind==='bool' && x.op!=='add'){ val=this.el('select'); for(const [k, t] of [['1', 'to true'], ['0', 'to false']]){ const o=this.el('option', '', t); o.value=k; val.append(o); } val.value=+x.value? '1' : '0'; val.onchange=()=>{ x.value=+val.value; this.changed(); }; }
        else val=this.input(x.value, y=>{ x.value=y; this.changed(); }, 'number', '70px');
        r.append(op, v, val);
        if(this.adv){ const k=this.el('select'); for(const [kk, t, h] of this.EFFECT_KINDS){ const o=this.el('option', '', t); o.value=kk; o.title=h; k.append(o); } k.value=x.kind||'run'; k.onchange=()=>{ x.kind=k.value; this.changed(); }; r.append(k); }
        r.append(this.btn('✕', ()=>{ n.effects.splice(i, 1); this.changed(); }));
        e.append(r);
      });
      e.append(this.btn('+ Effect', ()=>{ const s=this.doc.state.find(v=>!v.sensor) || this.doc.state[0]; n.effects.push({var:s? s.name : '', op:'set', value:1, kind:'run'}); this.changed(); }));
      frag.append(e);
    }
    return frag;
  },
  /** A reusable part: its name, where it is used, and Remove when it is used nowhere. */
  partInspector(x){
    const w=this.section('Reusable part');
    w.append(this.el('div', 'hint', 'A Select built once and used where a "Use part" step names it: the first thing in it that can be done is done there. FluadHTN splices it in (Splice).'));
    w.append(this.line('Name', this.input(x.name, v=>{ x.name=v; this.walk(n=>{ if(n.type==='splice' && n.sub===x.id) n.name=v; }); this.changed(); }, 'text', '220px')));
    let uses=0; this.walk(n=>{ if(n.type==='splice' && n.sub===x.id) uses++; });
    w.append(this.el('div', 'hint', uses? 'Used in '+uses+' place'+(uses===1? '' : 's')+'.' : 'Not used yet: add a "Use part" step where it should be done.'));
    const rm=this.btn('Remove part', ()=>{ this.doc.subs.splice(this.doc.subs.indexOf(x), 1); this.sel=''; this.changed(); }, uses? 'Remove the steps that use it first' : 'Remove this part');
    rm.disabled=uses>0;
    w.append(rm, ' ', this.btn('Done', ()=>{ this.sel=''; this.draw(); }));
    return w;
  },
  spliceInspector(n){
    const w=this.section('Use part');
    w.append(this.el('div', 'hint', 'Does the reusable part here, as if its steps were written in this place.'));
    const pick=this.el('select');
    const z=this.el('option', '', '(choose a part)'); z.value=''; pick.append(z);
    const own=this.partOf(n.id);
    for(const x of this.doc.subs||[]){ if(x===own) continue; const o=this.el('option', '', x.name); o.value=x.id; pick.append(o); }
    pick.value=n.sub||''; pick.onchange=()=>{ n.sub=pick.value; const x=this.sub(n.sub); n.name=x? x.name : 'Use part'; this.changed(); };
    w.append(this.line('Part', pick));
    const bar=this.el('div'); bar.style.margin='6px 0';
    bar.append(this.btn('↑', ()=>this.move(n.id, -1)), ' ', this.btn('↓', ()=>this.move(n.id, 1)), ' ', this.btn('Remove', ()=>this.remove(n.id)), ' ', this.btn('Done', ()=>{ this.sel=''; this.draw(); }));
    w.append(bar);
    return w;
  },
  here(dd){
    const R=App.R, o=(App._scene && App._scene.origin)||[0, 0];
    if(!R || !R.cam){ toast('Open a cell in the Preview first', 'warn', 2500); return; }
    dd.args.x=Math.round(R.cam.tx+o[0]); dd.args.y=Math.round(R.cam.ty+o[1]); dd.args.z=Math.round(R.cam.tz);
    this.changed();
  },
  drawProblems(){
    const box=$('#tlHtnProbs'); if(!box || !this.doc) return;
    box.textContent='';
    const probs=this.validate();
    const w=this.section(probs.length? 'Problems ('+probs.length+')' : 'Problems');
    if(!probs.length) w.append(this.el('div', 'hint', 'None: ready to export.'));
    for(const p of probs){
      const r=this.el('div', '', (p.level==='error'? '⛔ ' : '⚠ ')+p.text);
      if(p.at){ r.style.cursor='pointer'; r.onclick=()=>{ this.sel=p.at; this.draw(); }; }
      w.append(r);
    }
    if(this._exported) w.append(this.el('div', 'hint', 'Last exported to '+this._exported));
    box.append(w);
  },
  drawRun(){
    const box=$('#tlHtnRun'); if(!box) return;
    box.textContent='';
    const w=this.section('Try it in the Preview');
    const R=this.run;
    const S=typeof WgScripts==='object'? WgScripts : null;
    if(R && !(S && S.on)){ this.run=null; if(typeof WgLua==='object') WgLua.extra=null; return this.drawRun(); }
    if(!R){
      w.append(this.el('div', 'hint', 'Runs the behaviour on an actor of the loaded cell, by this page\'s own planner (close to FluadHTN\'s; custom Lua is not run). Turn on the Scripts first.'));
      if(!(S && S.on)){ w.append(this.el('div', 'hint', 'Turn on the Scripts in the Preview (its Scripts button), then come back to pick an actor.')); box.append(w); return; }
      const acts=this.sceneActors();
      if(!acts.length){ w.append(this.el('div', 'hint', 'Open a cell with NPCs or creatures in the Preview.')); box.append(w); return; }
      const sel=this.el('select');
      const want=new Set((this.doc.attach.ids||[]).map(s=>String(s).toLowerCase()));
      acts.sort((a, b)=>(want.has(b.id)-want.has(a.id)) || a.id.localeCompare(b.id));
      for(const a of acts.slice(0, 400)){ const o=this.el('option', '', a.id+(want.has(a.id)? ' ★' : '')); o.value=a.key; sel.append(o); }
      const fl=this.deps && this.deps.omw;
      const onF=this.btn('Run on FluadHTN', ()=>this.startFluad(sel.value), fl? 'The exported Lua, run in the Preview on the FluadHTN you installed' : 'FluadHTN\'s OpenMW build is not in the data folders', '');
      onF.disabled=!fl;
      w.append(sel, ' ', onF, ' ', this.btn('Run on Wraithguard\'s planner', ()=>this.startRun(sel.value), 'This page\'s own planner: no FluadHTN needed; custom Lua is not run'));
      box.append(w); return;
    }
    const doing=R.mode==='fluad'? R.curName : R.cur && R.cur.name;
    w.append(this.btn('Stop', ()=>this.stopRun()), ' ', this.el('b', '', R.A.id), ' ', this.el('span', 'hint', (R.mode==='fluad'? 'on FluadHTN · ' : 'on Wraithguard\'s planner · ')+(doing? 'doing: '+doing : R.status==='none'? 'no plan' : 'between steps')));
    const ws=this.el('div'); ws.style.cssText='font-family:var(--mono);font-size:11px;margin:4px 0';
    ws.textContent=R.doc.state.map((v, i)=>v.name+'='+(v.kind==='bool'? (R.ws[i]? 'true' : 'false') : R.ws[i])).join('  ') || '(no world state)';
    w.append(ws);
    if(R.plan.length) w.append(this.el('div', 'hint', 'Next: '+R.plan.map(n=>typeof n==='string'? n : n.name).join(' → ')));
    const log=this.el('div'); log.style.cssText='font-family:var(--mono);font-size:11px;max-height:180px;overflow:auto';
    for(const [t, s] of R.log.slice(-30)) log.append(this.el('div', '', t.toFixed(1)+'s  '+s));
    w.append(log);
    box.append(w);
  },
  luaView(){
    const w=this.section('Generated Lua');
    const pick=this.el('select');
    const files=(()=>{ try{ return this.files(); }catch(e){ return [['error', String(e && e.message || e)]]; } })();
    for(const [p] of files){ const o=this.el('option', '', p); o.value=p; pick.append(o); }
    if(files.some(f=>f[0]===this._showLua)) pick.value=this._showLua;
    const pre=this.el('pre'); pre.style.cssText='max-height:360px;overflow:auto;font-size:11px;white-space:pre';
    const show=()=>{ this._showLua=pick.value; const f=files.find(x=>x[0]===pick.value); pre.textContent=f? f[1] : ''; };
    pick.onchange=show; show();
    w.append(pick, pre);
    return w;
  },
};
