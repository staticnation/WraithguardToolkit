/* =====================================================================================
   Wraithguard: the key binding page (TODO, "The world": remappable Editor shortcuts, with
   the controller's buttons below them).

   Every Editor shortcut is an action with its keys (`ACTIONS`); the Editor asks
   `WgKeys.is(e, 'undo')` instead of reading the key itself, so a chord changed here is
   the chord the Editor answers. The page lists them by group: click a key to press a
   new one in its place, + adds another, x takes one away, a key two actions share is
   marked. The controller's buttons (49_wg_gamepad.js) are a table of their own: each
   function the pad has (switch mode, Escape, the clicks...) on the button you choose.
   Kept in this browser's storage (`wgKeys`, `wgPadMap`); Reset puts the defaults back.

   Fixed, and listed as such in the Keys help: the held axis keys (Z, X, Y while
   dragging), the selection sets (1-9, Ctrl+1-9), the layers (Ctrl+Shift+1-9), Escape,
   and the path grid mode's own keys.
   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
const WgKeys={
  ACTIONS:[
    {id:'undo',      group:'Editing',  label:'Undo the last change',                    def:['Ctrl+Z']},
    {id:'redo',      group:'Editing',  label:'Redo it',                                 def:['Ctrl+Y','Ctrl+Shift+Z']},
    {id:'copy',      group:'Editing',  label:'Copy the selected object',                def:['Ctrl+C']},
    {id:'paste',     group:'Editing',  label:'Paste it',                                def:['Ctrl+V']},
    {id:'duplicate', group:'Editing',  label:'Duplicate it beside itself',              def:['Ctrl+D']},
    {id:'delete',    group:'Editing',  label:'Delete the selected object',              def:['Delete']},
    {id:'drop',      group:'Editing',  label:'Drop it to the ground',                   def:['F']},
    {id:'quick',     group:'Editing',  label:'Quick menu under the pointer',            def:['Q']},
    {id:'transform', group:'Editing',  label:'Transform by numbers',                    def:['Ctrl+T']},
    {id:'grid',      group:'Snapping', label:'Snap to grid on or off',                  def:['G']},
    {id:'angle',     group:'Snapping', label:'Snap to angle on or off',                 def:['Shift+G']},
    {id:'frame',     group:'View',     label:'Centre the view on the selection',        def:['C']},
    {id:'hide',      group:'View',     label:'Hide the selection',                      def:['H']},
    {id:'isolate',   group:'View',     label:'Show only the selection (again: all)',    def:['Shift+H']},
    {id:'ori',       group:'View',     label:'Inspector (ORI) on or off',               def:['I']},
    {id:'tfh',       group:'View',     label:'Full help (TFH) on or off',               def:['Shift+I']},
    {id:'messages',  group:'Windows',  label:'Messages',                                def:['Ctrl+Shift+M']},
    {id:'filter',    group:'Windows',  label:'Filter the Object Window',                def:['Ctrl+F']},
    {id:'searchAll', group:'Windows',  label:'Search everything, and replace',          def:['Ctrl+Shift+F']},
    {id:'record',    group:'Windows',  label:'Edit the record shown in ORI',            def:['F2']},
    {id:'reference', group:'Windows',  label:'Edit the selected reference',             def:['F3']},
    {id:'verify',    group:'Patch',    label:'Verify what the patch carries',           def:['Ctrl+Shift+V']},
    {id:'testRun',   group:'Patch',    label:'Test in OpenMW',                          def:['Ctrl+F5']},
    {id:'prefab',    group:'Patch',    label:'Prefabs under the pointer',               def:['Ctrl+Shift+P']},
  ],
  /** The pad's functions (49_wg_gamepad.js), each on a standard-mapping button. */
  PAD:[
    {slot:0,  label:'Switch FLY / CURSOR'},
    {slot:1,  label:'Escape (CURSOR)'},
    {slot:2,  label:'Shift+click (CURSOR)'},
    {slot:3,  label:'Orbit / WASD (FLY)'},
    {slot:4,  label:'Zoom out / slower (FLY)'},
    {slot:5,  label:'Zoom in / faster (FLY)'},
    {slot:6,  label:'Down (FLY) / right click (CURSOR)'},
    {slot:7,  label:'Up (FLY) / left click (CURSOR)'},
    {slot:12, label:'North cell / list up'},
    {slot:13, label:'South cell / list down'},
    {slot:14, label:'West cell / slider down'},
    {slot:15, label:'East cell / slider up'},
  ],
  BUTTONS:['A','B','X','Y','LB','RB','LT','RT','View','Menu','L3','R3','D-pad up','D-pad down','D-pad left','D-pad right','Home'],

  _map:null, _pad:null,

  /** The chords of each action: the user's, else the default. */
  map(){
    if(!this._map){
      let v={}; try{ v=JSON.parse(localStorage.getItem('wgKeys')||'{}')||{}; }catch(_){ v={}; }
      this._map={};
      for(const a of this.ACTIONS) this._map[a.id]=Array.isArray(v[a.id])? v[a.id].slice() : a.def.slice();
    }
    return this._map;
  },
  save(){
    const out={};
    for(const a of this.ACTIONS){ const m=this.map()[a.id]; if(m.join('|')!==a.def.join('|')) out[a.id]=m; }
    try{ localStorage.setItem('wgKeys', JSON.stringify(out)); }catch(_){}
  },
  /** The pad: function slot -> the physical button it is on. */
  pad(){
    if(!this._pad){
      let v={}; try{ v=JSON.parse(localStorage.getItem('wgPadMap')||'{}')||{}; }catch(_){ v={}; }
      this._pad={};
      for(const p of this.PAD) this._pad[p.slot]=Number.isInteger(v[p.slot])? v[p.slot] : p.slot;
    }
    return this._pad;
  },
  savePad(){
    const out={};
    for(const p of this.PAD) if(this.pad()[p.slot]!==p.slot) out[p.slot]=this.pad()[p.slot];
    try{ localStorage.setItem('wgPadMap', JSON.stringify(out)); }catch(_){}
  },
  /** The physical button a pad function is on (49_wg_gamepad.js reads through this). */
  padButton(slot){ const m=this.pad(); return (slot in m)? m[slot] : slot; },

  /** A key press as a chord: Ctrl, Alt, Shift, then the key (a letter by its place on
   *  the keyboard, so another layout's letters still find it). */
  chordOf(e){
    const c=e.code||'';
    let k=/^Key[A-Z]$/.test(c)? c.slice(3) : /^Digit\d$/.test(c)? c.slice(5) : String(e.key||'');
    if(k===' ') k='Space';
    if(k.length===1) k=k.toUpperCase();
    if(['Control','Shift','Alt','Meta'].includes(k)) return '';
    return (e.ctrlKey||e.metaKey? 'Ctrl+' : '')+(e.altKey? 'Alt+' : '')+(e.shiftKey? 'Shift+' : '')+k;
  },
  is(e, id){ const ch=this.chordOf(e); return !!ch && (this.map()[id]||[]).includes(ch); },
  /** An action's keys, for menus and help. */
  label(id){ return (this.map()[id]||[]).join(' / ')||'(none)'; },

  /* ---- the page ------------------------------------------------------------------ */

  page(){
    if(!this.el){
      const d=document.createElement('div');
      d.id='wgKeysPage'; d.className='ori'; d.hidden=true;
      Object.assign(d.style,{width:'min(560px,92vw)', maxHeight:'80vh'});
      d.innerHTML='<div class="orihead"><b>Keys</b><span style="flex:1"></span>'+
        '<button class="btn dim sm" id="wgKeysReset" title="Every key and button back as it came">Reset all</button>'+
        '<button class="btn dim ic" id="wgKeysX" title="Close">&#x2715;</button></div><div class="oribody" id="wgKeysBody" style="overflow:auto"></div>';
      if(typeof WgUI==='object' && WgUI.mount) WgUI.mount(d); else document.body.appendChild(d);
      d.querySelector('#wgKeysX').onclick=()=>{ this.stopCapture(); d.hidden=true; };
      d.querySelector('#wgKeysReset').onclick=()=>{
        this._map=null; this._pad=null;
        try{ localStorage.removeItem('wgKeys'); localStorage.removeItem('wgPadMap'); }catch(_){}
        this.draw();
      };
      this.el=d;
    }
    this.el.hidden=false;
    this.draw();
  },

  draw(){
    const body=this.el && this.el.querySelector('#wgKeysBody'); if(!body) return;
    const m=this.map(), uses=new Map();
    for(const a of this.ACTIONS) for(const ch of m[a.id]) uses.set(ch, (uses.get(ch)||0)+1);
    let h='<div class="hint">Click a key to press a new one in its place. A key two actions share is marked, and the first in the list wins.</div>';
    let group='';
    h+='<table class="edT" style="width:100%">';
    for(const a of this.ACTIONS){
      if(a.group!==group){ group=a.group; h+='<tr><th colspan="3" style="text-align:left;padding-top:8px">'+escHtml(group)+'</th></tr>'; }
      const keys=m[a.id].map((ch,i)=>'<button class="btn sm'+(uses.get(ch)>1? ' warn' : '')+'" data-a="'+a.id+'" data-i="'+i+'" title="'+(uses.get(ch)>1? 'Another action has this key too' : 'Press a new key for this')+'"'+
        (uses.get(ch)>1? ' style="border-color:var(--warn);color:var(--warn)"' : '')+'>'+escHtml(ch)+'</button>'+
        '<button class="btn dim ic" data-x="'+a.id+'" data-i="'+i+'" title="Take this key away">&#x2715;</button>').join(' ');
      const changed=m[a.id].join('|')!==a.def.join('|');
      h+='<tr><td>'+escHtml(a.label)+'</td><td>'+keys+' <button class="btn dim sm" data-add="'+a.id+'" title="Another key for this">+</button></td>'+
        '<td>'+(changed? '<button class="btn dim sm" data-def="'+a.id+'" title="Back to '+escHtml(a.def.join(' / '))+'">Reset</button>' : '')+'</td></tr>';
    }
    h+='</table>';
    h+='<div style="margin-top:12px;font-weight:600">Controller</div><div class="hint">Each of the pad\'s functions on the button you choose (a standard, Xbox-style layout; the Steam Deck reports its own that way).</div>';
    h+='<table class="edT" style="width:100%">';
    const pm=this.pad();
    for(const p of this.PAD){
      h+='<tr><td>'+escHtml(p.label)+'</td><td><select class="fld" data-pad="'+p.slot+'">'+
        this.BUTTONS.map((b,i)=>'<option value="'+i+'"'+(pm[p.slot]===i? ' selected' : '')+'>'+escHtml(b)+'</option>').join('')+'</select></td></tr>';
    }
    h+='</table>';
    h+='<div class="hint" style="margin-top:8px">Fixed: Z, X and Y held while dragging, the selection sets (1-9, Ctrl+1-9), the layers (Ctrl+Shift+1-9), Esc, and the path grid mode\'s keys.</div>';
    body.innerHTML=h;
    body.querySelectorAll('[data-a]').forEach(b=>b.onclick=()=>this.capture(b, b.dataset.a, +b.dataset.i));
    body.querySelectorAll('[data-add]').forEach(b=>b.onclick=()=>this.capture(b, b.dataset.add, -1));
    body.querySelectorAll('[data-x]').forEach(b=>b.onclick=()=>{ m[b.dataset.x].splice(+b.dataset.i, 1); this.save(); this.draw(); });
    body.querySelectorAll('[data-def]').forEach(b=>b.onclick=()=>{ const a=this.ACTIONS.find(x=>x.id===b.dataset.def); m[a.id]=a.def.slice(); this.save(); this.draw(); });
    body.querySelectorAll('[data-pad]').forEach(s=>s.onchange=()=>{ pm[+s.dataset.pad]=+s.value; this.savePad(); });
  },

  /** The next key pressed goes to the action (in place of key `i`, or added when -1);
   *  Esc gives up. */
  capture(btn, id, i){
    this.stopCapture();
    btn.textContent='press a key…';
    this._cap=e=>{
      if(['Control','Shift','Alt','Meta'].includes(e.key)) return;
      e.preventDefault(); e.stopPropagation();
      this.stopCapture();
      if(e.key==='Escape' && !e.ctrlKey && !e.shiftKey && !e.altKey){ this.draw(); return; }
      const ch=this.chordOf(e); if(!ch){ this.draw(); return; }
      const list=this.map()[id];
      if(i>=0) list[i]=ch; else if(!list.includes(ch)) list.push(ch);
      this.map()[id]=[...new Set(list)];
      this.save(); this.draw();
    };
    window.addEventListener('keydown', this._cap, true);
  },
  stopCapture(){ if(this._cap){ window.removeEventListener('keydown', this._cap, true); this._cap=null; } },
};
