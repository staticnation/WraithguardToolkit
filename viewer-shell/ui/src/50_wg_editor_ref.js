/* =====================================================================================
   Wraithguard Editor (50_wg_editor.js): the reference dialog: one placed object's position, rotation, scale, owner, lock, key, trap, door.

   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
Object.assign(WgEditor, {
  /* ---- the reference dialog ---------------------------------------------------------- */

  /** A placed object's dialog: `ref` is {cell, origin, refr, plugins?} - the cell as its
   *  records key it, the plugin that created the reference and its index there. */
  async openRef(ref){
    this.refRec=Object.assign({}, ref);
    const d=this.dialog();
    d.hidden=false;
    d.querySelector('#edDlgTitle').textContent=ref.uid? 'New reference' : 'Reference - '+ref.origin+':'+ref.refr;
    const body=d.querySelector('#edDlgBody');
    if(!this.canEdit()){
      body.innerHTML='<div class="hint">Editing needs Wraithguard: open this viewer from Wraithguard\'s Cell Preview.</div>';
      return;
    }
    body.innerHTML='<div class="hint">Reading the cell from every plugin that has it…</div>';
    try{ this.drawRef(await this.ask(ref.uid? 'editNew' : 'editRef', this.refReq())); }
    catch(e){ body.innerHTML='<div class="hint">'+escHtml(String(e.message||e))+'</div>'; }
  },

  refReq(extra){ return Object.assign(this.refBody(this.refRec), extra||{}); },

  /** The labels the dialog shows, the Construction Set's names where it has them. */
  REF_LABELS:{translation:'Position', rotation:'Rotation (°)', scale:'Scale', deleted:'Deleted',
    owner:'Owner', owner_global:'Global variable', owner_faction:'Faction', owner_faction_rank:'Faction rank',
    lock_level:'Lock level', key:'Key', trap:'Trap', soul:'Soul', charge_left:'Charge', health_left:'Health',
    object_count:'Count', blocked:'Blocked', temporary:'Temporary', moved_cell:'Moved to cell', destination:'Door to'},

  drawRef(v){
    const d=this.dialog(), body=d.querySelector('#edDlgBody');
    this.refRec.cell=v.cell;
    if(!v.new) this.refRec.origin=v.origin;
    if(!this.refRec.plugins) this.refRec.plugins=v.plugins;
    this.refView=v;
    d.querySelector('#edDlgTitle').textContent=v.new? 'New reference - '+v.id : 'Reference - '+v.id+' ('+v.origin+':'+v.refr+')';
    const F={}; for(const f of v.fields) F[f.path]=f;
    const cur=f=> ('queued' in f)? f.queued : f.value;
    const DEG=180/Math.PI;
    let h='<div class="orirow"><span class="k">Cell</span><span class="v">'+escHtml(v.cell)+'</span></div>';
    if(v.new) h+='<div class="orirow"><span class="k">Created by</span><span class="v">the patch (new)</span></div>'+
      '<div class="orirow"><span class="v"><button class="btn sm" id="edRefBase" title="The base record this places">Base record</button> '+
      '<button class="btn sm" id="edRefRevertAll" title="Take this new reference back out of the patch">Remove from the patch</button></span></div>'+
      '<div class="hint">A reference the patch adds: numbered on from the patch\'s own when it is written.</div>';
    else h+='<div class="orirow"><span class="k">Created by</span><span class="v">'+escHtml(v.origin)+'</span></div>'+
      '<div class="orirow"><span class="k">Changed by</span><span class="v">'+
        v.plugins.map(p=>p===v.winner? '<b>'+escHtml(p)+'</b>' : escHtml(p)).join(' &gt; ')+'</span></div>'+
      '<div class="orirow"><span class="v"><button class="btn sm" id="edRefBase" title="The base record this places">Base record</button> '+
      '<button class="btn sm" id="edRefRevertAll" title="Drop every change waiting for this reference">Revert reference</button></span></div>'+
      '<div class="hint">Changes are drawn in the render window as you make them; the patch carries this one reference, keyed to '+escHtml(v.origin)+'.</div>';
    const row=(f, inner)=>{
      const q='queued' in f;
      return '<tr class="'+(q?'edited':'')+'"><td class="k" title="'+escHtml(f.path)+'">'+escHtml(this.REF_LABELS[f.path]||f.path)+'</td><td>'+inner+
        (q? '<div class="from">was '+escHtml(JSON.stringify(f.value))+'</div>' : '')+
        '</td><td>'+(q? '<button class="btn dim ic" data-rrevert="'+escHtml(f.path)+'" title="Drop this change">&#x21B6;</button>' : '')+'</td></tr>';
    };
    // The fields by what they are about, each group folding (its state kept for every reference).
    const GROUP={translation:'place', rotation:'place', scale:'place', destination:'door',
      owner:'own', owner_global:'own', owner_faction:'own', owner_faction_rank:'own',
      lock_level:'lock', key:'lock', trap:'lock', soul:'item', charge_left:'item', health_left:'item', object_count:'item',
      deleted:'state', blocked:'state', temporary:'state', moved_cell:'state'};
    const TITLES={place:'Placement', door:'Door', own:'Ownership', lock:'Lock & trap', item:'Item', state:'State', other:'Other'};
    const rows={};
    const put=(path, html)=>{ const g=GROUP[path]||'other'; (rows[g]=rows[g]||[]).push(html); };
    for(const path of ['translation','rotation']){
      const f=F[path]; if(!f) continue;
      const vec=(cur(f)||[0,0,0]).map(n=> path==='rotation'? n*DEG : n);
      put(path, row(f, ['X','Y','Z'].map((a,i)=>
        '<div class="edVec"><span class="from">'+a+'</span>'+
        '<button class="btn dim ic" data-nudge="'+path+'" data-i="'+i+'" data-s="-1" title="'+(path==='rotation'?'Turn':'Move')+' back (Shift: more)">&#x2212;</button>'+
        '<input class="fld" type="number" step="any" data-vec="'+path+'" data-i="'+i+'" value="'+(+vec[i].toFixed(path==='rotation'?2:1))+'">'+
        '<button class="btn dim ic" data-nudge="'+path+'" data-i="'+i+'" data-s="1" title="'+(path==='rotation'?'Turn':'Move')+' on (Shift: more)">+</button></div>').join('')));
    }
    for(const f of v.fields){
      if(f.path==='translation' || f.path==='rotation') continue;
      const c=cur(f), dis=f.editable? '' : ' disabled';
      let inner;
      if(f.kind==='bool') inner='<input type="checkbox" data-rpath="'+f.path+'"'+(c?' checked':'')+dis+'>';
      else if(f.kind==='door') inner=this.doorInputs(c);
      else if(f.kind==='grid') inner='<input class="fld" type="text" data-rpath="'+f.path+'" data-kind="grid" placeholder="x, y" value="'+escHtml(c? c.join(', ') : '')+'"'+dis+' spellcheck="false">';
      else {
        const num=f.kind==='int'||f.kind==='float';
        inner='<input class="fld" type="'+(num?'number':'text')+'"'+(f.kind==='int'?' step="1"':num?' step="any"':'')+
          (f.path==='scale'? ' min="0.5" max="2"' : '')+' data-rpath="'+f.path+'" value="'+escHtml(c==null? '' : String(c))+'"'+
          (f.path==='scale'? ' placeholder="1"' : '')+dis+' spellcheck="false">';
      }
      put(f.path, row(f, inner));
    }
    const order=['place','door','own','lock','item','state','other'];
    const qd=new Set(v.fields.filter(f=>'queued' in f).map(f=>GROUP[f.path]||'other'));
    h+=WgUI.sectionBar()+WgUI.sectionsHtml('REF', order.filter(g=>rows[g]).map(g=>({key:g, title:TITLES[g], edited:qd.has(g),
      html:'<table class="edT edFields"><tbody>'+rows[g].join('')+'</tbody></table>'})));
    body.innerHTML=h;
    WgUI.wireSections(body);
    WgUI.wireSectionBar(body);
    body.querySelector('#edRefBase').onclick=()=>{
      const r=(this.refs||[]).find(x=>x[1]===v.id);
      this.openRecord(v.tag || (r? r[2] : (this._oriRecord && this._oriRecord.id===v.id? this._oriRecord.tag : 'STAT')), v.id, null);
    };
    body.querySelector('#edRefRevertAll').onclick=()=>this.refChange('editRefRevert', {});
    body.querySelectorAll('[data-rrevert]').forEach(b=>b.onclick=()=>this.refChange('editRefRevert', {path:b.dataset.rrevert}));
    const vecOf=path=>{
      const vals=[...body.querySelectorAll('[data-vec="'+path+'"]')].map(el=>Number(el.value)||0);
      return path==='rotation'? vals.map(n=>n/DEG) : vals;
    };
    body.querySelectorAll('[data-vec]').forEach(el=>{
      el.onchange=()=>this.refChange('editRefSet', {path:el.dataset.vec, value:vecOf(el.dataset.vec)}, el);
      el.onkeydown=e=>{ if(e.key==='Enter') el.blur(); e.stopPropagation(); };
    });
    body.querySelectorAll('[data-nudge]').forEach(b=>b.onclick=e=>{
      const el=body.querySelector('[data-vec="'+b.dataset.nudge+'"][data-i="'+b.dataset.i+'"]');
      el.value=String((Number(el.value)||0)+(+b.dataset.s)*this.NUDGE*(e.shiftKey?8:1));
      el.onchange();
    });
    this.wireDoor(body, F.destination? cur(F.destination) : null);
    body.querySelectorAll('[data-rpath]').forEach(el=>{
      el.onchange=()=>{
        let value = el.type==='checkbox'? el.checked : el.value;
        if(el.dataset.kind==='grid') value = el.value.trim()? el.value.split(',').map(n=>n.trim()) : '';
        else if(el.type==='number') value = el.value===''? '' : Number(el.value);
        this.refChange('editRefSet', {path:el.dataset.rpath, value}, el);
      };
      if(el.type!=='checkbox') el.onkeydown=e=>{ if(e.key==='Enter') el.blur(); e.stopPropagation(); };
    });
  },

  /** A teleport door's destination: whether it is one, the cell (empty: the exterior),
   *  where the player lands and which way they face. */
  doorInputs(c){
    const t=c||{translation:[0,0,0], rotation:[0,0,0], cell:''};
    const face=((Math.atan2(Math.sin(t.rotation[2]), Math.cos(t.rotation[2]))*180/Math.PI)+360)%360;
    const rooms=(this.cells||[]).filter(x=>x.kind==='int').slice(0,4000);
    const on=!!c;
    return '<label><input type="checkbox" data-door="on"'+(on?' checked':'')+'> Teleport</label>'+
      '<div data-door-box'+(on?'':' hidden')+'>'+
      '<input class="fld" type="text" data-door="cell" list="edRooms" placeholder="Cell (empty: the exterior)" value="'+escHtml(t.cell||'')+'" spellcheck="false">'+
      '<datalist id="edRooms">'+rooms.map(r=>'<option value="'+escHtml(r.name)+'">').join('')+'</datalist>'+
      ['X','Y','Z'].map((a,i)=>'<div class="edVec"><span class="from">'+a+'</span><input class="fld" type="number" step="any" data-door="p" data-i="'+i+'" value="'+(+(+t.translation[i]).toFixed(1))+'"></div>').join('')+
      '<div class="edVec"><span class="from" title="Which way the player faces, degrees clockwise from north">⦟</span><input class="fld" type="number" step="any" data-door="face" value="'+(+face.toFixed(1))+'"></div>'+
      '<div class="edVec"><button class="btn sm" data-door="view" title="The view\'s pivot, facing the way the view looks, in the cell on screen">From the view</button>'+
      '<button class="btn sm" data-door="go" title="Go to where it leads">Go there</button></div></div>';
  },

  /** The destination's inputs: any change sends the whole destination. */
  wireDoor(body, cur){
    const q=sel=>body.querySelector(sel);
    const on=q('[data-door="on"]'); if(!on) return;
    const rot=(cur && cur.rotation)? cur.rotation.slice() : [0,0,0];
    const read=()=>{
      const p=[...body.querySelectorAll('[data-door="p"]')].map(el=>Number(el.value)||0);
      const face=(Number(q('[data-door="face"]').value)||0)*Math.PI/180;
      return {translation:p, rotation:[rot[0], rot[1], face], cell:q('[data-door="cell"]').value.trim()};
    };
    const send=v=>this.refChange('editRefSet', {path:'destination', value:v}, on);
    on.onchange=()=>{
      if(!on.checked){ send(null); return; }
      q('[data-door-box]').hidden=false;
      send(read());
    };
    body.querySelectorAll('input[data-door]:not([data-door="on"])').forEach(el=>{
      el.onchange=()=>send(read());
      el.onkeydown=e=>{ if(e.key==='Enter') el.blur(); e.stopPropagation(); };
    });
    const view=q('[data-door="view"]');
    if(view) view.onclick=()=>{
      const R=App.R, sc=App._scene;
      if(!R || !R.cam || !sc || !sc.origin) return toast('Open a cell first','warn',3000);
      const c=R.cam, look=[-Math.cos(c.az), -Math.sin(c.az)];
      const sel=App.cellSel||{};
      send({translation:[c.tx+sc.origin[0], c.ty+sc.origin[1], c.tz], rotation:[0,0,Math.atan2(look[0], look[1])],
            cell:sel.kind==='int'? (sel.name||'') : ''});
    };
    const go=q('[data-door="go"]');
    if(go) go.onclick=()=>{
      const d=read(), target=typeof doorTarget==='function'? doorTarget({pos:d.translation, rot:d.rotation, cell:d.cell}) : null;
      if(!target) return toast('No such cell in the loaded world','warn',3000);
      App.mode='cell'; App.cellSel=target;
      App._doorAim={pos:d.translation.slice(), rot:d.rotation[2],
                    key:target.kind==='int'? 'i:'+(target.name||'') : target.x+','+target.y};
      if(typeof syncCellButton==='function') syncCellButton();
      schedulePreview();
    };
  },

  async refChange(link, extra, el){
    try{
      if(this.refRec && this.refRec.uid){
        if(link==='editRefRevert'){
          // A new reference has nothing to revert to: the whole of it goes.
          if(extra && extra.path) return this.refChange('editRefSet', {path:extra.path, value:null}, el);
          await this.ask('editNewRemove', this.refReq());
          this.refRec=null; this._sel=null;
          if(this.dlg) this.dlg.hidden=true;
          if(App.R) App.R.setStaticHighlight(null);
          await this.refreshPending();
          return;
        }
        link='editNewSet';
      }
      this.drawRef(await this.ask(link, this.refReq(extra)));
      await this.refreshPending();
    }catch(e){
      if(el) el.classList.add('bad');
      toast(String(e.message||e).replace(/^Wraithguard answered 400:\s*/,''),'err',6000);
    }
  },
});
