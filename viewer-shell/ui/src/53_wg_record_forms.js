/* =====================================================================================
   The record dialog as the Construction Set lays it out - Wraithguard. Each record type
   has its own form there: a weapon's damage in a Minimum/Maximum grid, a light's colour
   with a picker and its flicker as one choice, an NPC's flags as checkboxes, a book's
   text in a pane of its own, the art file and inventory image with their pictures. This
   draws that form at the top of the record dialog (50_wg_editor.js drawRecord), every
   input the same field the dialog's sections below have ("All fields", folded), so a
   change goes the same way - to Wraithguard's patch pool - and a field no form shows is
   still there.

   Flags are checkboxes: ticking one writes the flags field with that flag set or taken
   off (References Persist and Blocked are the record's own flags, as the CS has them at
   the foot of every form).

   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
const WgForms={
  /* A layout is a list of parts:
       'path'                       a field, its label from F_LABELS (or the path's words)
       ['path', 'Label']            a field with its own label
       {flag:'path', name:'X', label:'Y', not:true}  one flag as a checkbox (not: inverted)
       {radio:'path', label:'Flicker', names:[['FLICKER','Flicker'], ...]}  flags as one choice
       {group:'Title', parts:[...], cols:2}          a box of parts
       {grid:'Damage', rows:[['Chop','data.chop_min','data.chop_max'], ...], heads:['Minimum','Maximum']}
       {list:'path', label:'Items'}                  a list field (its table)
       {text:'path', label:'Book text'}              a long text, in its own pane
       {color:'path'}                                a colour, with a picker
     The art file (mesh) and inventory image (icon) are drawn by the form itself, with
     their pictures, when the record has them. */
  PERSIST:{flag:'flags', name:'PERSISTENT', label:'References persist'},
  BLOCKED:{flag:'flags', name:'BLOCKED', label:'Blocked'},
  F_LABELS:{id:'ID', name:'Name', script:'Script', 'data.weight':'Weight', 'data.value':'Value', 'data.quality':'Quality',
    'data.uses':'Uses', 'data.health':'Health', 'data.enchantment':'Enchantment', enchanting:'Enchanting',
    'data.armor_rating':'AR', 'data.armor_type':'Type', 'data.clothing_type':'Type', 'data.weapon_type':'Type',
    'data.apparatus_type':'Type', 'data.speed':'Speed', 'data.reach':'Reach', 'data.level':'Level', race:'Race', class:'Class',
    faction:'Faction', 'data.rank':'Rank', 'data.disposition':'Disp.', 'data.reputation':'Rep.', 'data.creature_type':'Type',
    scale:'Scale', blood_type:'Blood texture', sound:'Sound gen', open_sound:'Open sound', close_sound:'Close sound',
    'data.radius':'Radius', 'data.time':'Time', encumbrance:'Weight', chance_none:'Chance none', 'data.skill':'Teaches',
    'data.part':'Part', 'data.bodypart_type':'Part type', 'data.vampire':'Vampire', 'data.spell_type':'Type', 'data.cost':'Cost',
    'data.enchant_type':'Cast type', 'data.max_charge':'Charge amount', head:'Head', hair:'Hair', 'data.soul':'Soul',
    'ai_data.fight':'Fight', 'ai_data.flee':'Flee', 'ai_data.alarm':'Alarm', 'ai_data.hello':'Hello'},

  layouts(){
    const P=this.PERSIST, B=this.BLOCKED;
    const use=['id','name','script',{group:'', cols:2, parts:['data.weight','data.value','data.uses','data.quality']},P,B];
    return {
      ACTI:['id','name','script',P,B],
      STAT:['id',P,B],
      DOOR:['id','name','script','open_sound','close_sound',P,B],
      MISC:['id','name','script',{group:'', cols:2, parts:['data.weight','data.value']},{flag:'data.flags', name:'KEY', label:'Key'},P,B],
      LOCK:use, PROB:use, REPA:use,
      APPA:['id','name','data.apparatus_type','script',{group:'', cols:2, parts:['data.weight','data.value','data.quality']},P,B],
      ALCH:['id','name','script','data.weight','data.value',{list:'effects', label:'Results'},
        {flag:'data.flags', name:'AUTO_CALCULATE', label:'Auto calculate value'},P,B],
      INGR:['id','name','script',{group:'', cols:2, parts:['data.weight','data.value']},
        {group:'Effects', parts:[{list:'data.effects', label:'Effects'},{list:'data.attributes', label:'Attributes'},{list:'data.skills', label:'Skills'}]},P,B],
      BOOK:['id','name','script','data.skill',{group:'', cols:2, parts:['data.weight','data.value','data.enchantment']},
        {flag:'data.book_type', name:'SCROLL', label:'Scroll', enum:['Book','Scroll']},'enchanting',P,B,{text:'text', label:'Book text'}],
      CLOT:['id','name','data.clothing_type','script',{group:'', cols:2, parts:['data.weight','data.value']},'data.enchantment','enchanting',
        {list:'biped_objects', label:'Biped object, male and female clothing'},P,B],
      ARMO:['id','name','data.armor_type','script',{group:'', cols:2, parts:['data.weight','data.armor_rating','data.health','data.value']},{armorClass:true},
        'data.enchantment','enchanting',{list:'biped_objects', label:'Biped object, male and female armor'},P,B],
      WEAP:['id','name','data.weapon_type','script',{group:'', cols:2, parts:['data.weight','data.value','data.health','data.speed','data.enchantment','data.reach']},
        'enchanting',{grid:'Damage', heads:['Minimum','Maximum'], rows:[['Chop','data.chop_min','data.chop_max'],['Slash','data.slash_min','data.slash_max'],['Thrust','data.thrust_min','data.thrust_max']]},
        {flag:'data.flags', name:'IGNORES_NORMAL_WEAPON_RESISTANCE', label:'Ignores normal weapon resistance'},
        {flag:'data.flags', name:'SILVER', label:'Silver weapon'},P,B],
      LIGH:['id','script','sound',{flag:'data.flags', name:'CAN_CARRY', label:'This light can be carried'},'name',
        {group:'', cols:2, parts:['data.weight','data.value','data.time']},{flag:'data.flags', name:'OFF_BY_DEFAULT', label:'Off by default'},
        {group:'Color', parts:[{color:'data.color'}]},'data.radius',
        {radio:'data.flags', label:'Flicker effect', names:[['FLICKER','Flicker'],['FLICKER_SLOW','Flicker slow'],['PULSE','Pulse'],['PULSE_SLOW','Pulse slow'],['','None']]},
        {flag:'data.flags', name:'FIRE', label:'Fire'},{flag:'data.flags', name:'NEGATIVE', label:'Negative'},{flag:'data.flags', name:'DYNAMIC', label:'Dynamic'},P,B],
      CONT:['id','name','script',{flag:'container_flags', name:'ORGANIC', label:'Organic container'},{flag:'container_flags', name:'RESPAWNS', label:'Respawns'},
        'encumbrance',{list:'inventory', label:'Count / Object ID'},P,B],
      BODY:['id','data.part',['data.bodypart_type','Part type'],{flag:'data.flags', name:'FEMALE', label:'Female'},
        {flag:'data.flags', name:'NOT_PLAYABLE', label:'Playable', not:true},{group:'Skin info', parts:['race','data.vampire']},B],
      NPC_:['id','name','script',{group:'', cols:2, parts:['race','class','faction','data.rank','data.level']},
        {flag:'npc_flags', name:'FEMALE', label:'Female'},{flag:'npc_flags', name:'ESSENTIAL', label:'Essential'},
        {flag:'npc_flags', name:'RESPAWN', label:'Respawn'},{flag:'npc_flags', name:'AUTO_CALCULATE', label:'Auto calc all'},'blood_type',
        {group:'Stats', cols:2, parts:['data.disposition','data.reputation','data.gold']},
        {group:'Head and hair', cols:2, parts:['head','hair']},
        {group:'AI', cols:2, parts:['ai_data.fight','ai_data.flee','ai_data.alarm','ai_data.hello']},
        {list:'inventory', label:'Items'},{list:'spells', label:'Spells'},P,B],
      CREA:['id','name','script',{group:'', cols:2, parts:['data.creature_type','data.level','scale']},
        {flag:'creature_flags', name:'WEAPON_AND_SHIELD', label:'Weapon & shield'},{flag:'creature_flags', name:'ESSENTIAL', label:'Essential'},
        {flag:'creature_flags', name:'RESPAWN', label:'Respawn'},'blood_type',
        {group:'Stats', cols:3, parts:[['data.strength','Str'],['data.speed','Spd'],['data.health','HP'],['data.intelligence','Int'],['data.endurance','End'],
          ['data.magicka','MP'],['data.willpower','Wil'],['data.personality','Per'],['data.fatigue','FP'],['data.agility','Agi'],['data.luck','Luc'],['data.soul','Soul']]},
        {group:'Skills', cols:3, parts:[['data.combat','Combat'],['data.magic','Magic'],['data.stealth','Stealth']]},
        'sound',{group:'Movement', parts:[{flag:'creature_flags', name:'FLIES', label:'Flies'},{flag:'creature_flags', name:'WALKS', label:'Walks'},
          {flag:'creature_flags', name:'SWIMS', label:'Swims'},{flag:'creature_flags', name:'BIPED', label:'Biped'}]},
        {list:'inventory', label:'Items'},{list:'spells', label:'Spells'},P,B],
      LEVC:['id','chance_none',{flag:'leveled_creature_flags', name:'CALCULATE_FROM_ALL_LEVELS', label:"Calculate from all levels <= PC's level"},
        {list:'creatures', label:'PC level / Creature name'},B],
      LEVI:['id','chance_none',{flag:'leveled_item_flags', name:'CALCULATE_FROM_ALL_LEVELS', label:"Calculate from all levels <= PC's level"},
        {flag:'leveled_item_flags', name:'CALCULATE_FOR_EACH_ITEM', label:'Calculate for each item in count'},{list:'items', label:'PC level / Item name'},B],
      SPEL:['id','name',{list:'effects', label:'Effects'},'data.spell_type',{flag:'data.flags', name:'ALWAYS_SUCCEEDS', label:'Always succeeds'},
        {flag:'data.flags', name:'PC_START_SPELL', label:'PC start spell'},{flag:'data.flags', name:'AUTO_CALCULATE', label:'Auto calculate cost'},['data.cost','Spell cost'],B],
      ENCH:['id','data.enchant_type',{list:'effects', label:'Effects'},'data.max_charge',['data.flags','Auto calculate'],['data.cost','Enchantment cost'],B],
    };
  },

  /** Whether a flags value (`A | B`, or a list) holds a flag. */
  has(raw, name){
    if(Array.isArray(raw)) return raw.map(x=>String(x).toUpperCase()).includes(name);
    return String(raw||'').split('|').map(x=>x.trim().toUpperCase()).includes(name);
  },
  /** A flags value with a flag set or taken off, written back the way it came. */
  setFlag(raw, name, on){
    const list=Array.isArray(raw)? raw.map(String) : String(raw||'').split('|').map(x=>x.trim()).filter(Boolean);
    const out=list.filter(x=>x.toUpperCase()!==name);
    if(on) out.push(name);
    return Array.isArray(raw)? out : out.join(' | ');
  },

  /** The form for a record, or '' for a type without one. */
  html(E, v){
    const L=this.layouts()[v.tag];
    if(!L) return '';
    const F={}; for(const f of v.fields) F[f.path]=f;
    this._F=F; this._E=E;
    const cur=f=> ('queued' in f)? f.queued : f.value;
    const label=(p, own)=>escHtml(own || this.F_LABELS[p] || WgUI.label(p));
    const field=(p, own)=>{
      const f=F[p]; if(!f) return '';
      const q='queued' in f;
      let input=E.input(f);
      if(p==='script' && cur(f)) input='<span class="edVec">'+input+'<button class="btn sm" data-fscript="'+escHtml(cur(f))+'" title="Open the script in Script Edit">…</button></span>';
      return '<div class="edFRow'+(q? ' edited' : '')+'"><label title="'+escHtml(p)+'">'+label(p, own)+'</label><div>'+input+'</div></div>';
    };
    const part=x=>{
      if(typeof x==='string') return field(x);
      if(Array.isArray(x)) return field(x[0], x[1]);
      if(x.flag){
        const f=F[x.flag]; if(!f) return '';
        if(x.enum){
          const on=String(cur(f)||'').toLowerCase()===x.enum[1].toLowerCase();
          return '<label class="edFCheck"><input type="checkbox" data-fenum="'+escHtml(x.flag)+'" data-on="'+escHtml(x.enum[1])+'" data-off="'+escHtml(x.enum[0])+'"'+(on?' checked':'')+(f.editable?'':' disabled')+'> '+escHtml(x.label)+'</label>';
        }
        const on=this.has(cur(f), x.name)!==!!x.not;
        return '<label class="edFCheck'+(('queued' in f)? ' edited' : '')+'" title="'+escHtml(x.flag+': '+x.name)+'"><input type="checkbox" data-fflag="'+escHtml(x.flag)+'" data-name="'+escHtml(x.name)+'"'+(x.not? ' data-not="1"' : '')+(on?' checked':'')+(f.editable?'':' disabled')+'> '+escHtml(x.label)+'</label>';
      }
      if(x.radio){
        const f=F[x.radio]; if(!f) return '';
        const names=x.names.map(n=>n[0]).filter(Boolean);
        const now=names.find(n=>this.has(cur(f), n))||'';
        return '<fieldset class="edFGroup"><legend>'+escHtml(x.label)+'</legend><div class="edFRadios">'+x.names.map(([n,l])=>
          '<label class="edFCheck"><input type="radio" name="edFR_'+escHtml(x.radio)+'" data-fradio="'+escHtml(x.radio)+'" data-name="'+escHtml(n)+'" data-all="'+escHtml(names.join(','))+'"'+(n===now?' checked':'')+(f.editable?'':' disabled')+'> '+escHtml(l)+'</label>').join('')+'</div></fieldset>';
      }
      if(x.group!==undefined){
        return '<fieldset class="edFGroup">'+(x.group? '<legend>'+escHtml(x.group)+'</legend>' : '')+
          '<div class="edFCols" style="grid-template-columns:repeat('+(x.cols||1)+',minmax(0,1fr))">'+x.parts.map(part).join('')+'</div></fieldset>';
      }
      if(x.grid){
        return '<fieldset class="edFGroup"><legend>'+escHtml(x.grid)+'</legend><table class="edFGrid"><thead><tr><th></th>'+x.heads.map(h=>'<th>'+escHtml(h)+'</th>').join('')+'</tr></thead><tbody>'+
          x.rows.map(([l,...ps])=>'<tr><td>'+escHtml(l)+'</td>'+ps.map(p=>'<td>'+(F[p]? E.input(F[p]) : '')+'</td>').join('')+'</tr>').join('')+'</tbody></table></fieldset>';
      }
      if(x.list){
        const f=F[x.list]; if(!f) return '';
        return '<fieldset class="edFGroup edFList'+(('queued' in f)? ' edited' : '')+'"><legend>'+escHtml(x.label)+' <span class="from">'+(Array.isArray(cur(f))? cur(f).length : 0)+'</span></legend>'+E.input(f)+'</fieldset>';
      }
      if(x.text){
        const f=F[x.text]; if(!f) return '';
        return '<fieldset class="edFGroup edFText"><legend>'+escHtml(x.label)+'</legend>'+E.input(f)+'</fieldset>';
      }
      if(x.armorClass){
        return '<div class="edFRow"><label title="Not stored in the record: the game works it out from the slot\'s base weight game setting times fLightMaxMod or fMedMaxMod">Weight class</label>'+
          '<div><b id="edFArmorClass"></b> <span class="from" id="edFArmorLim"></span></div></div>';
      }
      if(x.color){
        const f=F[x.color]; if(!f) return '';
        const c=cur(f), rgb=Array.isArray(c)? c : [255,255,255,0];
        const hex='#'+rgb.slice(0,3).map(n=>Math.max(0,Math.min(255,n|0)).toString(16).padStart(2,'0')).join('');
        return '<div class="edFColor"><input type="color" data-fcolor="'+escHtml(x.color)+'" value="'+hex+'"'+(f.editable?'':' disabled')+' title="Pick the light\'s colour">'+
          '<span class="from">R '+(rgb[0]|0)+' G '+(rgb[1]|0)+' B '+(rgb[2]|0)+'</span></div>';
      }
      return '';
    };
    const main=L.filter(x=>!(x && x.text)).map(part).join('');
    const text=L.filter(x=>x && x.text).map(part).join('');
    const art=F.mesh? '<div class="edFArt"><canvas class="edPrevCv" width="128" height="128" id="edFMesh" title="The art file, turning"></canvas>'+field('mesh','Art file')+'</div>' : '';
    const inv=F.icon? '<div class="edFArt"><div class="edFIcon"><img id="edFIcon" alt=""></div>'+field('icon','Inventory image')+'</div>' : '';
    return '<div class="edForm'+(text? ' withText' : '')+'"><div class="edFMain">'+main+'</div>'+
      '<div class="edFSide">'+art+inv+text+'</div></div>';
  },

  /** Morrowind.esm's settings, until Wraithguard sends the load order's. */
  ARMOR_DEFAULT:{slots:{Helmet:5, Cuirass:30, LeftPauldron:10, RightPauldron:10, Greaves:15, Boots:20,
    LeftGauntlet:5, RightGauntlet:5, LeftBracer:5, RightBracer:5, Shield:15}, light:0.6, medium:0.9},

  /** Light, Medium or Heavy for a slot and weight (the game's rule; `armor.py` has it
   *  for the Object Window's column). */
  armorClass(t, type, weight){
    const base=t.slots[type]; if(base==null || !Number.isFinite(weight)) return null;
    const light=base*t.light, med=base*t.medium, slack=5e-4;
    return {cls: weight<=light+slack? 'Light' : weight<=med+slack? 'Medium' : 'Heavy', light, med};
  },

  /** The armor form's Weight class, worked out again as the weight or the slot changes. */
  wireArmorClass(E, body){
    const out=body.querySelector('#edFArmorClass'), lim=body.querySelector('#edFArmorLim');
    const w=body.querySelector('.edFMain [data-path="data.weight"]'), ty=body.querySelector('.edFMain [data-path="data.armor_type"]');
    const F=this._F||{}, cur=f=> f? (('queued' in f)? f.queued : f.value) : null;
    const table=()=>{ const c=(E._cols||{}).ARMO; return (c && c.armorClass) || null; };
    const draw=()=>{
      const t=table()||this.ARMOR_DEFAULT;
      const type=ty? ty.value : cur(F['data.armor_type']);
      const weight=parseFloat(w? w.value : cur(F['data.weight']));
      const r=this.armorClass(t, String(type||''), weight);
      out.textContent=r? r.cls : '—';
      out.className='edArmor'+(r? ' '+r.cls.toLowerCase() : '');
      const fmt=n=>String(Math.round(n*100)/100);
      lim.textContent=r? '(Light up to '+fmt(r.light)+', Medium up to '+fmt(r.med)+(table()? '' : '; Morrowind\'s settings')+')' : '';
    };
    for(const el of [w, ty]) if(el){ el.addEventListener('input', draw); el.addEventListener('change', draw); }
    draw();
    // The load order's own settings, when the Object Window has not fetched them yet.
    if(!table() && E.canEdit && E.canEdit() && E.links().editColumns){
      E.ask('editColumns', {tag:'ARMO'}).then(c=>{ E._cols=E._cols||{}; E._cols.ARMO=c; if(document.body.contains(out)) draw(); }).catch(()=>{});
    }
  },

  /** The form's own controls: flags, choices, colour, script buttons, the pictures. */
  wire(E, body, v){
    const F=this._F||{};
    const cur=f=> ('queued' in f)? f.queued : f.value;
    const send=(path, value, el)=>E.change('editSet', {path, value}, el);
    body.querySelectorAll('[data-fflag]').forEach(cb=>cb.onchange=()=>{
      const f=F[cb.dataset.fflag]; if(!f) return;
      const on=cb.checked!==(cb.dataset.not==='1');
      send(cb.dataset.fflag, this.setFlag(cur(f), cb.dataset.name, on), cb);
    });
    body.querySelectorAll('[data-fenum]').forEach(cb=>cb.onchange=()=>send(cb.dataset.fenum, cb.checked? cb.dataset.on : cb.dataset.off, cb));
    body.querySelectorAll('[data-fradio]').forEach(rb=>rb.onchange=()=>{
      const f=F[rb.dataset.fradio]; if(!f || !rb.checked) return;
      let raw=cur(f);
      for(const n of rb.dataset.all.split(',')) if(n) raw=this.setFlag(raw, n, false);
      if(rb.dataset.name) raw=this.setFlag(raw, rb.dataset.name, true);
      send(rb.dataset.fradio, raw, rb);
    });
    body.querySelectorAll('[data-fcolor]').forEach(ci=>ci.onchange=()=>{
      const f=F[ci.dataset.fcolor]; if(!f) return;
      const h=ci.value.replace('#',''), old=Array.isArray(cur(f))? cur(f) : [0,0,0,0];
      send(ci.dataset.fcolor, [parseInt(h.slice(0,2),16), parseInt(h.slice(2,4),16), parseInt(h.slice(4,6),16), old[3]||0], ci);
    });
    body.querySelectorAll('[data-fscript]').forEach(b=>b.onclick=()=>E.openScript('SCPT', b.dataset.fscript, null));
    if(body.querySelector('#edFArmorClass')) this.wireArmorClass(E, body);
    // The pictures: the art file turning, the inventory image.
    const cv=body.querySelector('#edFMesh');
    if(this._spin){ this._spin.stop(); this._spin=null; }
    if(cv && F.mesh && cur(F.mesh)) this._spin=WgUI.spinner(cv, {tag:v.tag, id:v.id, model:cur(F.mesh)});
    else if(cv && v.tag==='NPC_') this._spin=WgUI.spinner(cv, {tag:'NPC_', id:v.id, model:''});
    const img=body.querySelector('#edFIcon');
    if(img && F.icon && cur(F.icon) && typeof loadTexture==='function'){
      loadTexture('icons\\'+cur(F.icon)).then(t=>{ if(t && t.thumb) img.src=t.thumb; }).catch(()=>{});
    }
  },
};
