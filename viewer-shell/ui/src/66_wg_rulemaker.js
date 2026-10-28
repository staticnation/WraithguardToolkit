/* =====================================================================================
   The rule maker - Wraithguard (the Tk window's "New mlox rule", 2026-10-27). Every rule the
   guidelines describe, built as data and checked by ordercore's authoring (`loadorder_author`)
   as it is written: the rule as it will read in the file, and what is wrong with it.

   - The rule's kind, each with what it does.
   - Its plugins (an ordering rule) or expressions (a warning rule), one a row: taken from the
     Load order page's picked content= rows, or typed (a plugin name, mlox wildcards, or any
     of mlox's forms - [DESC /regex/ a.esp], [SIZE 1234 a.esp], [VER < 1.2 a.esp] ...);
     dragged into order; picked rows grouped as [ANY ...], [ALL ...] or [NOT ...], or a group
     taken apart.
   - The message, the source it cites (Ref:), an @section heading, a highlight (!, !!, !!!)
     and a ; comment.
   - Append rule writes it to your rule file (refused while there is an error: mlox drops a
     rule it cannot read without a word); Rule guide opens MLOX_RULES.md.

   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */

const WgRuleMaker={
  el:null,
  rows:[],      // [{text, sel}]
  _t:0,
  problems:[],

  KINDS:[
    ['Order','These plugins load in this order, first one first. The rule most people need, and the one the guidelines steer you towards.'],
    ['NearStart','Pull each plugin toward the start of the load order. Discouraged: prefer [Order] to place plugins against each other.'],
    ['NearEnd','Pull each plugin toward the end. Also discouraged, and it means "closer to the end where possible", not "last".'],
    ['Note','Print a message when the listed expressions are all true. The general-purpose rule.'],
    ['Requires','The first plugin needs the second to be present. Warns, in red, when it is not.'],
    ['Conflict','Warns, in yellow, when any two of the listed plugins are active at once.'],
    ['Patch','A mutual dependency: the patch is pointless without what it patches, and what it patches wants the patch. Warns both ways.'],
  ],
  PRIORITY:[['','no highlight'],['!','blue: worth noticing, little or no effect on play'],['!!','yellow: could affect the game, should be seen to'],['!!!','red: could break the mod or the game, should be fixed']],

  kind(){ return (this.el.querySelector('input[name=rmKind]:checked')||{}).value||'Order'; },
  ordering(){ return ['Order','NearStart','NearEnd'].includes(this.kind()); },

  open(){
    this.build();
    this.el.hidden=false;
    this.centre();
    $('#rmFile').value=(typeof WgLoadOrder==='object' && $('#lo_userRules'))? WgLoadOrder.val('userRules') : '';
    this.refresh();
  },

  build(){
    if(this.el) return;
    const st=document.createElement('style');
    st.textContent=`
/* over the Load order page (z-index 60), as the Setup page lies over the rest */
#wgRM{z-index:330 !important;width:min(860px,92vw);min-width:420px;max-width:98vw;max-height:calc(100vh - 80px);display:flex;flex-direction:column;overflow:hidden}
#wgRM[hidden]{display:none !important}
#wgRM .orihead{flex:none}
#wgRM .rm-body{display:flex;flex-direction:column;gap:8px;flex:1;min-height:0;overflow:auto;padding:8px 10px;font-size:12px}
#wgRM fieldset{border:1px solid var(--line);border-radius:var(--r-box);padding:6px 8px;margin:0}
#wgRM legend{color:var(--tx2);padding:0 4px}
#wgRM .kinds{display:grid;grid-template-columns:repeat(auto-fill,minmax(150px,1fr));gap:2px 10px}
#wgRM .help{color:var(--tx3);margin-top:4px}
#wgRM .row{display:flex;gap:6px;align-items:center}
#wgRM input[type=text],#wgRM textarea{flex:1;min-width:0;background:var(--bg3);color:var(--tx);border:1px solid var(--line2);border-radius:var(--r-fld);padding:3px 6px;font:inherit}
#wgRM textarea{font-family:inherit;resize:vertical}
#wgRM .list{display:grid;grid-template-columns:1fr auto;gap:6px}
#wgRM ol{margin:0;padding:2px;list-style:none;font-family:var(--mono);min-height:90px;max-height:220px;overflow:auto;background:var(--bg3);border:1px solid var(--line2);border-radius:var(--r-fld)}
#wgRM li{padding:1px 6px;cursor:grab;white-space:nowrap}
#wgRM li.hit{background:var(--acd)}
#wgRM li.drop{box-shadow:0 -2px 0 var(--ac) inset}
#wgRM .btns{display:flex;flex-direction:column;gap:4px}
#wgRM pre{margin:0;padding:6px;font-family:var(--mono);background:var(--bg3);border:1px solid var(--line2);border-radius:var(--r-fld);white-space:pre;overflow:auto;min-height:70px}
#wgRM .probs div{padding:1px 0} #wgRM .probs .error{color:#ff6b6b} #wgRM .probs .warning{color:#ffd24a}
#wgRM .lbl{width:110px;color:var(--tx2);flex:none}`;
    document.head.appendChild(st);
    const d=document.createElement('div');
    d.id='wgRM'; d.className='ori'; d.hidden=true;
    d.innerHTML=`<div class="orihead"><b>New mlox rule</b><span style="flex:1"></span><button class="btn dim ic" id="rmX" title="Close">&#x2715;</button></div>
<div class="rm-body">
  <div class="row"><span class="lbl">Rule file</span><input type="text" id="rmFile" spellcheck="false" title="Your own rule file: the rule is added to its end (made with a header when new). Not mlox_base / mlox_user - Update rules writes over those. Keep it last among the rule files, so your rules win."><button class="btn dim" id="rmFilePick">…</button></div>
  <fieldset><legend>Rule type</legend><div class="kinds">${this.KINDS.map(([k,h],i)=>`<label title="${loEsc(h)}"><input type="radio" name="rmKind" value="${k}"${i===0?' checked':''}> [${k}]</label>`).join('')}</div><div class="help" id="rmKindHelp"></div></fieldset>
  <fieldset><legend id="rmListL">Plugins (order matters for [Order])</legend>
    <div class="list"><ol id="rmList"></ol>
      <div class="btns">
        <button class="btn dim" id="rmFromOrder" title="Add the rows picked in the Load order page's content= list (click, Ctrl+click, Shift+click there), in their order">From load order</button>
        <button class="btn dim" id="rmRemove">Remove</button><button class="btn dim" id="rmClear">Clear</button>
        <button class="btn dim" id="rmAny" title="Wrap the picked rows in [ANY ...]: satisfied by whichever of several versions is installed (an XB edition, a Tribunal edition ...). Pick a group and press again to take it apart.">Group as ANY</button>
        <button class="btn dim" id="rmAll" title="Wrap the picked rows in [ALL ...]: true only when every one is">Group as ALL</button>
        <button class="btn dim" id="rmNot" title="Wrap the picked row in [NOT ...]: true when it is not (a [Patch] cannot say NOT)">NOT</button>
      </div></div>
    <div class="row" style="margin-top:6px"><input type="text" id="rmAdd" spellcheck="false" placeholder="A plugin name (? * <VER> allowed), or [DESC /regex/ a.esp], [SIZE 1234 a.esp], [VER < 1.2 a.esp] ... then Enter"><button class="btn dim" id="rmAddBtn">Add</button></div>
    <div class="help">Drag rows into order; double-click one to edit it. For Requires and Patch the first is the subject.</div>
  </fieldset>
  <fieldset><legend>Message and source</legend>
    <div class="row"><span class="lbl">Message</span><textarea id="rmMsg" rows="3" title="What to tell the person, and what to do about it. Ordering rules carry none. One with a ] is written in the block form."></textarea></div>
    <div class="row" style="margin-top:4px"><span class="lbl">(Ref:) source</span><input type="text" id="rmRef" title="Where the rule's claim comes from: a readme, a forum post, a URL. The guidelines ask for one on every rule."></div>
    <div class="row" style="margin-top:4px"><span class="lbl">@section</span><input type="text" id="rmSection" title="A heading written above the rule; the rule-base groups rules by mod."></div>
    <div class="row" style="margin-top:4px"><span class="lbl">Highlight</span>${this.PRIORITY.map(([m,t],i)=>`<label title="${loEsc(t)}"><input type="radio" name="rmPri" value="${i}"${i===0?' checked':''}> ${m||'none'}</label>`).join(' ')}</div>
    <div class="row" style="margin-top:4px"><span class="lbl">; comment</span><input type="text" id="rmComment" title="A ; comment above the rule; mlox skips it."></div>
  </fieldset>
  <fieldset><legend>Preview</legend><pre id="rmPreview"></pre><div class="probs" id="rmProbs"></div></fieldset>
  <div class="row"><button class="btn" id="rmAppend" title="Add the rule to the rule file. Refused while there is an error: mlox drops a rule it cannot read without a word.">Append rule</button>
    <button class="btn dim" id="rmGuide" title="What each rule says, how expressions nest, and the rule-base's conventions">Rule guide</button><span style="flex:1"></span><button class="btn dim" id="rmClose">Close</button></div>
</div>`;
    // On the window, not in the viewport (mount's place), so it shows over the Load order
    // page, which covers the viewport. data-host tells WgUI.place/dragPanel to measure from
    // the window, not from #vpwrap. Its width is the panel's own (not the body's inside a
    // 360px .ori), and it is placed in px, not left:50% + translateX(-50%): dragging or
    // resizing writes left/top/width inline and the transform made it jump half a width left.
    d.dataset.host='body';
    document.body.appendChild(d);
    Object.assign(d.style, {position:'fixed', zIndex:'330'});
    WgUI.adopt(d);
    this.el=d;
    const close=()=>{ d.hidden=true; };
    d.querySelector('#rmX').onclick=close; d.querySelector('#rmClose').onclick=close;
    d.addEventListener('input', ()=>this.later());
    d.addEventListener('change', ()=>this.later());
    d.querySelector('#rmFilePick').onclick=async()=>{ const p=await Engine.pick('save',{defaultPath:$('#rmFile').value||'my_rules.txt'}); if(p){ $('#rmFile').value=String(p); this.keepFile(); } };
    $('#rmFile').addEventListener('change', ()=>this.keepFile());
    d.querySelector('#rmFromOrder').onclick=()=>{
      const names=(typeof WgLoadOrder==='object')? WgLoadOrder.picked() : [];
      if(!names.length) return toast('Pick plugins in the content= list first (click, Ctrl+click, Shift+click)','warn',4000);
      for(const n of names) this.rows.push({text:n, sel:false});
      this.draw(); this.refresh();
    };
    d.querySelector('#rmRemove').onclick=()=>{ this.rows=this.rows.filter(r=>!r.sel); this.draw(); this.refresh(); };
    d.querySelector('#rmClear').onclick=()=>{ this.rows=[]; this.draw(); this.refresh(); };
    d.querySelector('#rmAny').onclick=()=>this.group('ANY');
    d.querySelector('#rmAll').onclick=()=>this.group('ALL');
    d.querySelector('#rmNot').onclick=()=>this.group('NOT');
    const add=()=>{ const v=$('#rmAdd').value.trim(); if(!v) return; this.rows.push({text:v, sel:false}); $('#rmAdd').value=''; this.draw(); this.refresh(); };
    d.querySelector('#rmAddBtn').onclick=add;
    $('#rmAdd').addEventListener('keydown', e=>{ if(e.key==='Enter'){ e.preventDefault(); add(); } e.stopPropagation(); });
    d.querySelector('#rmAppend').onclick=()=>this.append();
    d.querySelector('#rmGuide').onclick=()=>{ if(typeof WgHelp==='object'){ WgHelp.enter(); WgHelp.open('MLOX_RULES.md'); } };
    // typing in the panel is not a shortcut of the window underneath
    d.addEventListener('keydown', e=>{ if(e.key==='Escape') close(); e.stopPropagation(); });
  },

  /** Centred under the top bar the first time (a place the person dragged it to is kept), and
   *  never wider or lower than the window. */
  centre(){
    const d=this.el; if(!d) return;
    const W=window.innerWidth||1200, w=d.offsetWidth;
    if(d.style.left===''||d.style.top===''){
      d.style.left=Math.max(8,Math.round((W-w)/2))+'px';
      d.style.top='60px';
    }else{
      const x=parseFloat(d.style.left)||0, y=parseFloat(d.style.top)||0;
      d.style.left=Math.max(0,Math.min(x,W-Math.min(w,W)))+'px';
      d.style.top=Math.max(0,Math.min(y,(window.innerHeight||800)-40))+'px';
    }
    d.style.right='auto';
  },

  keepFile(){
    const v=$('#rmFile').value.trim();
    if($('#lo_userRules')){ $('#lo_userRules').value=v; WgLoadOrder.save(); }
  },

  /** Picked rows wrapped in a group, or a picked group taken apart. */
  group(op){
    const picked=this.rows.filter(r=>r.sel);
    if(picked.length===1){
      const m=/^\[(ANY|ALL|NOT)\s+([\s\S]*)\]$/i.exec(picked[0].text.trim());
      if(m && m[1].toUpperCase()===op){
        // apart: its parts as rows of their own (top-level forms and names)
        const parts=this.split(m[2]);
        const at=this.rows.indexOf(picked[0]);
        this.rows.splice(at, 1, ...parts.map(t=>({text:t, sel:true})));
        this.draw(); this.refresh(); return;
      }
    }
    if(!picked.length) return toast('Pick the rows to group first','warn',3000);
    if(op==='NOT' && picked.length!==1) return toast('NOT takes one row: group several as ANY or ALL first','warn',4000);
    const at=this.rows.indexOf(picked[0]);
    const g={text:'['+op+' '+picked.map(r=>r.text).join(' ')+']', sel:true};
    this.rows=this.rows.filter(r=>!r.sel);
    this.rows.splice(Math.min(at,this.rows.length), 0, g);
    this.draw(); this.refresh();
  },

  /** A group's inside split into its top-level parts: bracketed forms whole, names between. */
  split(s){
    const out=[]; let depth=0, cur='';
    for(const ch of s){
      if(ch==='['){ if(depth===0 && cur.trim()){ out.push(...this.names(cur)); cur=''; } depth++; cur+=ch; }
      else if(ch===']'){ depth--; cur+=ch; if(depth===0){ out.push(cur.trim()); cur=''; } }
      else cur+=ch;
    }
    if(cur.trim()) out.push(...this.names(cur));
    return out.filter(Boolean);
  },
  /** Names in a run of text: each ends at a plugin extension. */
  names(s){ return (s.match(/\S.*?\.(?:esp|esm|omwaddon|omwgame|omwscripts)\*?(?=\s|$)/gi)||[s.trim()]).map(x=>x.trim()); },

  draw(){
    const ol=$('#rmList');
    $('#rmListL').textContent=this.ordering()? 'Plugins (order matters for [Order])' : 'Expressions (each a row: a plugin, a group or a [DESC]/[SIZE]/[VER] test)';
    ol.innerHTML='';
    this.rows.forEach((r,i)=>{
      const li=document.createElement('li');
      li.textContent=r.text; li.dataset.i=i; if(r.sel) li.classList.add('hit');
      li.ondblclick=async()=>{ const v=await WgUI.askText('Edit the row', r.text, ''); if(v!=null && v.trim()){ r.text=v.trim(); this.draw(); this.refresh(); } };
      li.addEventListener('pointerdown', e=>{
        if(e.button!==0) return;
        if(e.ctrlKey||e.metaKey) r.sel=!r.sel;
        else if(e.shiftKey && this._a!=null){ const [a,b]=[Math.min(this._a,i),Math.max(this._a,i)]; this.rows.forEach((x,k)=>{ x.sel=k>=a&&k<=b; }); }
        else{ if(!r.sel) this.rows.forEach(x=>{ x.sel=false; }); r.sel=true; this._a=i; }
        [...ol.children].forEach((c,k)=>c.classList.toggle('hit', !!this.rows[k].sel));
        const y0=e.clientY; let moved=false, target=null;
        li.setPointerCapture(e.pointerId);
        const mv=ev=>{
          if(!moved && Math.abs(ev.clientY-y0)<4) return; moved=true;
          const t=document.elementFromPoint(ev.clientX, ev.clientY); const l=t && t.closest && t.closest('li');
          if(target && target!==l) target.classList.remove('drop');
          target=l && l.parentElement===ol? l : null; if(target) target.classList.add('drop');
        };
        const up=()=>{
          li.removeEventListener('pointermove',mv); li.removeEventListener('pointerup',up);
          if(target){ target.classList.remove('drop'); }
          if(moved && target){
            const at=+target.dataset.i, picked=this.rows.filter(x=>x.sel), before=this.rows.slice(0,at).filter(x=>!x.sel).length;
            const rest=this.rows.filter(x=>!x.sel); rest.splice(before,0,...picked); this.rows=rest; this.draw(); this.refresh();
          }
        };
        li.addEventListener('pointermove',mv); li.addEventListener('pointerup',up);
      });
      ol.appendChild(li);
    });
  },

  rule(){
    const kind=this.kind(), ordering=this.ordering(), names=this.rows.map(r=>r.text);
    return {kind, message: ordering? '' : $('#rmMsg').value.trim(), ref:$('#rmRef').value.trim(),
      priority:+((this.el.querySelector('input[name=rmPri]:checked')||{}).value||0),
      section:$('#rmSection').value.trim(), comment:$('#rmComment').value.trim(),
      plugins: ordering? names : [], expressions:[], lines: ordering? [] : names};
  },

  later(){ clearTimeout(this._t); this._t=setTimeout(()=>this.refresh(), 250); },

  /** The rule as it will read, and what is wrong with it. */
  async refresh(){
    if(!this.el) return;
    const k=this.KINDS.find(x=>x[0]===this.kind());
    $('#rmKindHelp').textContent=k? k[1] : '';
    $('#rmMsg').disabled=this.ordering();
    this.draw();
    let got;
    try{ got=await Engine.call('loadorder_author',{op:'check', rule:JSON.stringify(this.rule()), path:''}); }
    catch(e){ $('#rmPreview').textContent=''; $('#rmProbs').innerHTML=`<div class="error">${loEsc(String(e.message||e))}</div>`; this.problems=[['error']]; $('#rmAppend').disabled=true; return; }
    $('#rmPreview').textContent=got.text||'';
    this.problems=got.problems||[];
    $('#rmProbs').innerHTML=this.problems.map(([sev,m,r])=>`<div class="${loEsc(sev)}">${loEsc(sev)}: ${loEsc(m)}${r? ' - '+loEsc(r) : ''}</div>`).join('')
      || '<div style="color:var(--tx3)">No problems.</div>';
    $('#rmAppend').disabled=this.problems.some(p=>p[0]==='error');
  },

  async append(){
    const path=$('#rmFile').value.trim();
    if(!path) return toast('Choose your rule file first','warn',3000);
    this.keepFile();
    try{
      const got=await Engine.call('loadorder_author',{op:'append', rule:JSON.stringify(this.rule()), path});
      toast('Added to '+path,'ok',3000);
      $('#rmPreview').textContent=got.written||'';
    }catch(e){ toast(String(e.message||e),'err',6000); }
  },
};
