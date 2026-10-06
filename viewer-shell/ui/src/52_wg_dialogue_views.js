/* =====================================================================================
   The dialogue window's other views - Wraithguard. The Editor's Dialogue window
   (50_wg_editor.js) lists topics and their responses as the Construction Set does; these
   are the other ways to look at the same lines (Wraithguard reads them:
   wraithguard/patch/dialogue_graph.py):

   - Flow: one topic's responses in the order the engine tries them - the first whose
     conditions all hold is the one said, so each falls through to the next - each with
     its conditions in words, its text with the words the game hyperlinks, and what its
     result script does. A response that offers a Choice carries, under each option, the
     responses that answer it: the tree inside the topic, nested as deep as it goes.
   - Map: the topics around one, drawn - what it leads to (AddTopic, a hyperlinked word)
     and what leads to it, the quests it advances and the variables it sets, and the
     topics that test those variables: separate trees linked through shared flags rather
     than through each other. Click a topic to centre it, double-click for its flow.
   - Flags: the blackboard - every variable, quest index, item and death count the
     dialogue writes or tests, with who writes it and who reads it.
   - In game: the game's dialogue window, rehearsed - pick an NPC, and their greeting, the
     topics they answer, the line the engine would pick for each (speaker conditions
     checked; the ones only the running game knows - globals, journal, items - said
     beside the line and assumed true, or skipped with Strict), hyperlinks and Choice
     buttons that work.

   Every response anywhere opens in the record dialog (✎).

   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
const WgDial={
  view:'list',
  E:null,
  depth:1,
  _play:null,           // {speaker, log:[...], cell, disposition, strict, topics}

  /** Adds the view switch to the Dialogue window (once). */
  attach(E){
    this.E=E;
    const d=E.dial; if(!d || d._wgViews) return;
    d._wgViews=true;
    const head=d.querySelector('.orihead');
    const seg=document.createElement('span');
    seg.className='edSeg'; seg.id='edDialViews';
    const views=[['list','List','The topics and their responses, as the Construction Set lists them'],
      ['flow','Flow','The selected topic as the engine reads it: each response falls through to the next, choices open their own branches'],
      ['map','Map','The topics around the selected one: what it leads to, what leads to it, the quests and variables in between'],
      ['flags','Flags','Every variable, quest index and item the dialogue writes or tests - who writes it, who reads it'],
      ['play','In game','The game\'s dialogue window, rehearsed with an NPC of your choice']];
    seg.innerHTML=views.map(([k,l,t])=>'<button class="btn sm'+(k===this.view? ' on' : '')+'" data-dview="'+k+'" title="'+escHtml(t)+'">'+l+'</button>').join('');
    head.insertBefore(seg, head.querySelector('span[style]')||null);
    seg.querySelectorAll('[data-dview]').forEach(b=>b.onclick=()=>this.show(b.dataset.dview));
    const body=d.querySelector('.oribody');
    const v=document.createElement('div');
    v.id='edDialView'; v.hidden=true;
    body.appendChild(v);
    this.box=v;
    if(this.view!=='list') this.show(this.view);
  },

  /** The topic the list has selected (or the last one shown). */
  topic(){ return (this.E && (this.E._dialSel || (this.E.dialTopic && this.E.dialTopic.id))) || this._last || ''; },

  show(view, topic){
    const E=this.E, d=E.dial; if(!d) return;
    this.view=view;
    d.querySelectorAll('#edDialViews [data-dview]').forEach(b=>b.classList.toggle('on', b.dataset.dview===view));
    const list=view==='list';
    for(const sel of ['#edDialTabs','#edDialFilter','.edSplit']){ const el=d.querySelector(sel); if(el) el.hidden=!list; }
    this.box.hidden=list;
    d.classList.toggle('edDialWide', !list);
    if(list) return;
    if(topic) this._last=topic;
    const t=topic||this.topic();
    if(view==='flow') return this.flow(t);
    if(view==='map') return this.map(t);
    if(view==='flags') return this.flags();
    if(view==='play') return this.play();
  },

  err(e){ return '<div class="hint">'+escHtml(String(e.message||e).replace(/^Wraithguard answered 400:\s*/,''))+'</div>'; },

  /** A line's text with its links and macros, the links clickable (data-dtopic). */
  textHtml(segs){
    return (segs||[]).map(s=> s.topic? '<a class="edDLink" data-dtopic="'+escHtml(s.topic)+'" title="Topic: '+escHtml(s.topic)+'">'+escHtml(s.t)+'</a>'
      : s.macro? '<span class="edDMacro" title="Filled in by the game">'+escHtml(s.t)+'</span>' : escHtml(s.t)).join('');
  },

  /* ---- flow ------------------------------------------------------------------------------ */

  async flow(topic){
    const box=this.box;
    if(!topic){ box.innerHTML='<div class="hint">Choose a topic in the List view (or a link anywhere) to see its flow.</div>'; return; }
    box.innerHTML='<div class="hint">Reading '+escHtml(topic)+'…</div>';
    let f;
    try{ f=await this.E.ask('editDialogueFlow', {topic}); }catch(e){ box.innerHTML=this.err(e); return; }
    this._last=f.id;
    const rows=f.responses;
    // The choice tree: a response's options hold the responses answering them.
    const nested=new Set();
    const kids=new Map();
    rows.forEach(r=>{
      if(!r.choices.length) return;
      for(const c of r.choices){
        const ans=rows.filter(x=>x!==r && x.gate===c.n);
        kids.set(r.id+':'+c.n, ans);
        ans.forEach(a=>nested.add(a.id));
      }
    });
    const chips=r=>{
      const c=[];
      r.addtopics.forEach(t=>c.push('<a class="edChip add" data-dtopic="'+escHtml(t)+'" title="AddTopic: the player learns it">+ '+escHtml(t)+'</a>'));
      r.journal.forEach(j=>c.push('<a class="edChip quest" data-dtopic="'+escHtml(j.quest)+'" title="Sets the quest\'s journal index">📜 '+escHtml(j.quest)+' '+j.index+'</a>'));
      r.sets.forEach(s=>c.push('<a class="edChip var" data-dflag="'+escHtml(s.name)+'" title="Sets a variable: who else reads it is in Flags">⚑ '+escHtml((s.target? s.target+'->' : '')+s.name)+'</a>'));
      r.items.forEach(i=>c.push('<span class="edChip item" title="'+(i.verb==='add'? 'Gives' : 'Takes')+' an item">'+(i.verb==='add'? '＋' : '－')+' '+escHtml(i.item)+'</span>'));
      if(r.goodbye) c.push('<span class="edChip bye" title="Ends the conversation">Goodbye</span>');
      r.other.slice(0,6).forEach(o=>c.push('<span class="edChip" title="Called by its result script">'+escHtml(o)+'</span>'));
      return c.length? '<div class="edChips">'+c.join('')+'</div>' : '';
    };
    const card=(r, depth)=>{
      let h='<div class="edDCard'+(r.orphan? ' orphan' : '')+(this.E.isEditedInfo(r.id)? ' edited' : '')+'" data-rid="'+escHtml(r.id)+'">'+
        '<div class="edDHead"><b>#'+r.n+'</b><span class="from">'+escHtml(r.speaker)+'</span>'+
        (r.gate!=null? '<span class="edChip gate" title="Said only after this choice">Choice '+r.gate+'</span>' : '')+
        (r.orphan? '<span class="bad" title="Its predecessor is not in the topic: the engine reads it last">read last</span>' : '')+
        '<span style="flex:1"></span><span class="from" title="'+escHtml(r.plugins.join(' > '))+'">'+escHtml(r.plugins[r.plugins.length-1]||'')+'</span>'+
        '<button class="btn dim ic" data-dedit="'+escHtml(r.id)+'" title="Open it in the record dialog">✎</button></div>'+
        (r.conditions.length? '<div class="edDCond">'+r.conditions.map(c=>'<div>if '+escHtml(c)+'</div>').join('')+'</div>' : '<div class="edDCond from">always (no conditions)</div>')+
        '<div class="edDText">'+this.textHtml(r.segments)+'</div>'+chips(r);
      if(r.choices.length && depth<6){
        h+='<div class="edDChoices">';
        for(const c of r.choices){
          const ans=kids.get(r.id+':'+c.n)||[];
          h+='<details class="edDChoice" open><summary><span class="edChip choice">'+c.n+'</span> '+escHtml(c.label)+' <span class="from">'+(ans.length? ans.length+' answer'+(ans.length===1?'':'s') : 'nothing answers it')+'</span></summary>'+
            ans.map(a=>card(a, depth+1)).join('<div class="edDElse">else ↓</div>')+'</details>';
        }
        h+='</div>';
      }
      return h+'</div>';
    };
    const top=rows.filter(r=>!nested.has(r.id));
    const edges=(list, dir)=>list.map(e=>'<a class="edChip '+e.kind+'" data-dtopic="'+escHtml(dir==='in'? e.from : e.to)+'" title="'+(e.kind==='addtopic'? 'AddTopic' : 'a hyperlinked word')+' (response '+escHtml(e.via)+')">'+escHtml(dir==='in'? e.from : e.to)+'</a>').join('');
    box.innerHTML='<div class="edDTop"><b>'+escHtml(f.id)+'</b> <span class="from">'+escHtml(f.type||'')+' · '+rows.length+' response'+(rows.length===1?'':'s')+'</span>'+
      '<span style="flex:1"></span><button class="btn sm" id="edDMapIt" title="This topic\'s neighbourhood as a map">Map it</button>'+
      '<button class="btn sm" id="edDPlayIt" title="Rehearse it in the game\'s window">In game</button></div>'+
      (f.into.length? '<div class="edDEdges"><span class="from">Led to from</span> '+edges(f.into,'in')+'</div>' : '')+
      (f.outof.length? '<div class="edDEdges"><span class="from">Leads to</span> '+edges(f.outof,'out')+'</div>' : '')+
      '<div class="hint">The engine tries these in order and says the first whose conditions all hold.</div>'+
      (top.length? top.map(r=>card(r,0)).join('<div class="edDElse">else ↓</div>') : '<div class="hint">No responses.</div>');
    this.wire(box);
    box.querySelector('#edDMapIt').onclick=()=>this.show('map', f.id);
    box.querySelector('#edDPlayIt').onclick=()=>this.show('play');
  },

  /** Links, chips and edit buttons in whatever a view drew. */
  wire(root){
    root.querySelectorAll('[data-dtopic]').forEach(a=>a.onclick=e=>{ e.preventDefault(); this.show(this.view==='map'? 'map' : 'flow', a.dataset.dtopic); });
    root.querySelectorAll('[data-dflag]').forEach(a=>a.onclick=e=>{ e.preventDefault(); this.flags(a.dataset.dflag); });
    root.querySelectorAll('[data-dedit]').forEach(b=>b.onclick=e=>{ e.stopPropagation(); this.E.openRecord('INFO', b.dataset.dedit, null); });
  },

  /* ---- map ------------------------------------------------------------------------------- */

  async map(topic){
    const box=this.box;
    if(!topic){ box.innerHTML='<div class="hint">Choose a topic first (List view, or a link).</div>'; return; }
    box.innerHTML='<div class="hint">Reading the topics around '+escHtml(topic)+'… (the first map reads every topic once)</div>';
    let m;
    try{ m=await this.E.ask('editDialogueMap', {topic, depth:this.depth}); }catch(e){ box.innerHTML=this.err(e); return; }
    this._last=topic;
    const W=900, H=620, cx=W/2, cy=H/2;
    // Rings by depth; on each ring, what leads here on the left and the rest on the right.
    const pos=new Map();
    const byDepth=new Map();
    for(const n of m.nodes){ if(!byDepth.has(n.depth)) byDepth.set(n.depth, []); byDepth.get(n.depth).push(n); }
    const incoming=new Set(m.edges.filter(e=>e.to===m.centre).map(e=>e.from));
    for(const [d, list] of byDepth){
      if(d===0){ list.forEach(n=>pos.set(n.id, [cx, cy])); continue; }
      const r=d===1? 220 : 290;
      const left=list.filter(n=>incoming.has(n.id)), right=list.filter(n=>!incoming.has(n.id));
      const place=(arr, a0, a1)=>arr.forEach((n,i)=>{
        const a=a0+(a1-a0)*((i+0.5)/Math.max(1, arr.length));
        pos.set(n.id, [cx+Math.cos(a)*r*1.35, cy+Math.sin(a)*r]);
      });
      place(left, Math.PI*0.62, Math.PI*1.38);
      place(right, -Math.PI*0.42, Math.PI*0.42);
    }
    const COL={addtopic:'#6cb6ff', link:'#9a9a9a', quest:'#c9a227', sets:'#3fb950', reads:'#e5534b'};
    let svg='<svg class="edDMap" viewBox="0 0 '+W+' '+H+'" xmlns="http://www.w3.org/2000/svg">'+
      '<defs>'+Object.entries(COL).map(([k,c])=>'<marker id="edAr_'+k+'" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M0 0L10 5L0 10z" fill="'+c+'"/></marker>').join('')+'</defs>';
    for(const e of m.edges){
      const a=pos.get(e.from), b=pos.get(e.to); if(!a || !b) continue;
      const dx=b[0]-a[0], dy=b[1]-a[1], L=Math.hypot(dx,dy)||1, k=Math.min(0.42, 46/L);
      svg+='<line x1="'+(a[0]+dx*k).toFixed(1)+'" y1="'+(a[1]+dy*k).toFixed(1)+'" x2="'+(b[0]-dx*k).toFixed(1)+'" y2="'+(b[1]-dy*k).toFixed(1)+'" stroke="'+COL[e.kind]+'" stroke-width="1.4"'+
        (e.kind==='link'? ' stroke-dasharray="4 3"' : '')+' marker-end="url(#edAr_'+e.kind+')"><title>'+escHtml(e.kind)+'</title></line>';
    }
    for(const n of m.nodes){
      const p=pos.get(n.id); if(!p) continue;
      const label=n.label.length>22? n.label.slice(0,21)+'…' : n.label;
      const w=Math.max(60, label.length*6.6+16), h=24;
      const cls='edDNode '+(n.kind==='quest'? 'quest' : n.kind==='var'? 'var' : n.depth===0? 'centre' : 'topic');
      svg+='<g class="'+cls+'" data-nid="'+escHtml(n.id)+'" transform="translate('+p[0].toFixed(1)+','+p[1].toFixed(1)+')">'+
        '<rect x="'+(-w/2)+'" y="'+(-h/2)+'" width="'+w+'" height="'+h+'" rx="'+(n.kind==='var'? 12 : 4)+'"/>'+
        '<text text-anchor="middle" dy="4">'+escHtml(label)+'</text><title>'+escHtml(n.label+(n.responses? ' - '+n.responses+' responses' : '')+
        (n.kind==='quest'? ' (quest)' : n.kind==='var'? ' (variable)' : ''))+'</title></g>';
    }
    svg+='</svg>';
    box.innerHTML='<div class="edDTop"><b>'+escHtml(topic)+'</b> <span class="from">'+m.nodes.length+' nodes'+(m.capped? ' (the first '+m.nodes.length+')' : '')+'</span><span style="flex:1"></span>'+
      '<label class="from" title="Two steps out: the neighbours\' neighbours too"><input type="checkbox" id="edDDepth"'+(this.depth===2?' checked':'')+'> 2 steps</label> '+
      '<button class="btn sm" id="edDFlowIt">Flow</button></div>'+
      '<div class="edDLegend">'+Object.entries({addtopic:'AddTopic', link:'hyperlinked word', quest:'advances a quest', sets:'sets a variable', reads:'tests it'})
        .map(([k,l])=>'<span><i style="background:'+COL[k]+'"></i>'+l+'</span>').join('')+'</div>'+
      '<div class="hint">Click a topic to centre it, double-click for its flow; a quest opens its stages, a variable its readers and writers.</div>'+svg;
    box.querySelector('#edDDepth').onchange=e=>{ this.depth=e.target.checked? 2 : 1; this.map(topic); };
    box.querySelector('#edDFlowIt').onclick=()=>this.show('flow', topic);
    box.querySelectorAll('g[data-nid]').forEach(g=>{
      const id=g.dataset.nid, kind=id.slice(0,2), name=(m.nodes.find(n=>n.id===id)||{}).label||id.slice(2);
      g.onclick=()=>{ if(kind==='t:') this.map(name); else if(kind==='q:') this.show('flow', name); else this.flags(name); };
      g.ondblclick=()=>{ if(kind==='t:') this.show('flow', name); };
    });
  },

  /* ---- flags ------------------------------------------------------------------------------ */

  async flags(name){
    const box=this.box;
    if(this.view!=='flags'){ this.view='flags'; this.E.dial.querySelectorAll('#edDialViews [data-dview]').forEach(b=>b.classList.toggle('on', b.dataset.dview==='flags')); }
    box.innerHTML='<div class="hint">Reading what the dialogue writes and tests…</div>';
    if(name){
      let f;
      try{ f=await this.E.ask('editDialogueFlags', {name}); }catch(e){ box.innerHTML=this.err(e); return; }
      const list=(rows, verb)=>rows.length? '<table class="edT"><tbody>'+rows.map(r=>
        '<tr><td><a class="edDLink" data-dtopic="'+escHtml(r.topic)+'">'+escHtml(r.topic)+'</a></td><td class="from">'+escHtml(r.how)+'</td>'+
        '<td class="edWrapCell">'+escHtml(r.text)+'</td><td><button class="btn dim ic" data-dedit="'+escHtml(r.id)+'" title="Open the response">✎</button></td></tr>').join('')+'</tbody></table>'
        : '<div class="hint">Nothing in the dialogue '+verb+' it'+(verb==='writes'? ' (a script may)' : '')+'.</div>';
      box.innerHTML='<div class="edDTop"><button class="btn sm" id="edDFlagsBack">‹ All flags</button> <b>'+escHtml(f.name)+'</b> <span class="from">'+escHtml(f.kind)+'</span></div>'+
        '<div class="orisec">Written by ('+f.writers.length+')</div>'+list(f.writers, 'writes')+
        '<div class="orisec">Tested by ('+f.readers.length+')</div>'+list(f.readers, 'tests');
      box.querySelector('#edDFlagsBack').onclick=()=>this.flags();
      this.wire(box);
      return;
    }
    let r;
    try{ r=await this.E.ask('editDialogueFlags', {}); }catch(e){ box.innerHTML=this.err(e); return; }
    const all=r.flags;
    const draw=q=>{
      const f=q.trim().toLowerCase();
      const rows=all.filter(x=>!f || x.name.toLowerCase().includes(f) || x.kind.includes(f));
      return '<table class="edT"><thead><tr><th>Kind</th><th>Name</th><th title="Responses that set it">Written</th><th title="Responses whose conditions test it">Tested</th></tr></thead><tbody>'+
        rows.slice(0,800).map(x=>'<tr data-dflag="'+escHtml(x.name)+'" class="'+(x.writers&&!x.readers? 'edDWOnly' : !x.writers&&x.readers? 'edDROnly' : '')+'" title="'+
          (x.writers&&!x.readers? 'Set, and nothing in the dialogue tests it' : !x.writers&&x.readers? 'Tested, and nothing in the dialogue sets it (a script may)' : 'Set in one place, tested in another')+'">'+
          '<td class="from">'+escHtml(x.kind)+'</td><td>'+escHtml(x.name)+'</td><td class="num">'+x.writers+'</td><td class="num">'+x.readers+'</td></tr>').join('')+'</tbody></table>'+
        (rows.length>800? '<div class="hint">'+(rows.length-800)+' more - filter to narrow.</div>' : '');
    };
    box.innerHTML='<div class="edDTop"><b>Flags</b> <span class="from">'+all.length+' names the dialogue writes or tests - the links between trees that never name each other</span></div>'+
      '<input class="fld" id="edDFlagFilter" placeholder="Filter: a name, or variable / journal / item / dead" spellcheck="false"><div id="edDFlagList">'+draw('')+'</div>';
    const fi=box.querySelector('#edDFlagFilter'), lst=box.querySelector('#edDFlagList');
    const wireRows=()=>lst.querySelectorAll('tr[data-dflag]').forEach(tr=>tr.onclick=()=>this.flags(tr.dataset.dflag));
    fi.oninput=()=>{ lst.innerHTML=draw(fi.value); wireRows(); };
    fi.onkeydown=e=>e.stopPropagation();
    wireRows();
  },

  /* ---- in game --------------------------------------------------------------------------- */

  async play(){
    const box=this.box;
    const P=this._play=this._play||{speaker:'', log:[], cell:'', disposition:50, strict:false, topics:[], name:''};
    box.innerHTML='<div class="edDTop edVec">'+
      '<input class="fld" id="edPSpeaker" list="edPNpcs" placeholder="Talk to: an NPC or creature id" value="'+escHtml(P.speaker)+'" spellcheck="false" title="Who the player talks to">'+
      '<datalist id="edPNpcs"></datalist>'+
      '<input class="fld" id="edPCell" placeholder="In cell (optional)" value="'+escHtml(P.cell)+'" spellcheck="false" title="Responses limited to a cell are checked against this; empty: they are assumed to fit" style="max-width:160px">'+
      '<label class="from" title="The NPC\'s disposition: responses needing more are not said">Disp <input type="number" class="fld" id="edPDisp" min="0" max="100" value="'+P.disposition+'" style="width:56px"></label>'+
      '<label class="from" title="Skip responses with conditions only the running game knows (globals, quests, items...), rather than assuming they hold"><input type="checkbox" id="edPStrict"'+(P.strict?' checked':'')+'> Strict</label>'+
      '<button class="btn sm pri" id="edPGreet" title="Start the conversation: their greeting">Talk</button></div>'+
      '<div class="mwDlg"><div class="mwDlgL"><div class="mwDlgName" id="edPName">'+escHtml(P.name||'')+'</div><div class="mwDlgLog" id="edPLog"></div></div>'+
      '<div class="mwDlgR"><div class="mwDlgTopics" id="edPTopics"></div><div class="mwDlgBye"><button class="mwBtn" id="edPBye">Goodbye</button></div></div></div>';
    const $b=s=>box.querySelector(s);
    const read=()=>{ P.speaker=$b('#edPSpeaker').value.trim(); P.cell=$b('#edPCell').value.trim(); P.disposition=+$b('#edPDisp').value||0; P.strict=$b('#edPStrict').checked; };
    for(const id of ['#edPSpeaker','#edPCell','#edPDisp']) $b(id).onkeydown=e=>{ if(e.key==='Enter') $b('#edPGreet').onclick(); e.stopPropagation(); };
    $b('#edPGreet').onclick=()=>{ read(); P.log=[]; this.say('', 0); };
    $b('#edPBye').onclick=()=>{ P.log.push({bye:true}); this.drawLog(); };
    this.fillNpcs($b('#edPNpcs'));
    this.drawLog(); this.drawTopics();
  },

  /** The NPC ids for the speaker box, once (the engine's list). */
  async fillNpcs(dl){
    if(!this._npcs){
      this._npcs=[];
      try{
        const [n, c]=await Promise.all([Engine.call('editor_records',{tag:'NPC_'}), Engine.call('editor_records',{tag:'CREA'})]);
        this._npcs=[...(n.rows||[]), ...(c.rows||[])].map(r=>[r[0], r[1]]);
      }catch(_){ }
    }
    dl.innerHTML=this._npcs.slice(0, 5000).map(([id, name])=>'<option value="'+escHtml(id)+'">'+escHtml(name||'')+'</option>').join('');
  },

  /** Asks the rehearsal: a topic (or the greeting), with a choice made. */
  async say(topic, choice){
    const P=this._play;
    if(!P.speaker) return toast('Choose who to talk to','warn',2500);
    let r;
    try{ r=await this.E.ask('editDialoguePlay', {speaker:P.speaker, topic, choice, cell:P.cell, disposition:P.disposition, strict:P.strict}); }
    catch(e){ P.log.push({error:String(e.message||e).replace(/^Wraithguard answered 400:\s*/,'')}); this.drawLog(); return; }
    P.name=r.speaker.name||r.speaker.id; P.topics=r.topics;
    P.log.push({topic, choice, line:r.line});
    this.drawLog(); this.drawTopics();
  },

  drawTopics(){
    const P=this._play, box=this.box.querySelector('#edPTopics'); if(!box) return;
    const name=this.box.querySelector('#edPName'); if(name) name.textContent=P.name||'';
    box.innerHTML=(P.topics||[]).map(t=>'<a class="mwTopic" data-ptopic="'+escHtml(t)+'">'+escHtml(t)+'</a>').join('')||'<div class="from">Talk to someone to see their topics.</div>';
    box.querySelectorAll('[data-ptopic]').forEach(a=>a.onclick=()=>this.say(a.dataset.ptopic, 0));
  },

  drawLog(){
    const P=this._play, box=this.box.querySelector('#edPLog'); if(!box) return;
    let h='';
    P.log.forEach((x, i)=>{
      if(x.error){ h+='<div class="mwErr">'+escHtml(x.error)+'</div>'; return; }
      if(x.bye){ h+='<div class="mwSys">(the conversation ends)</div>'; return; }
      if(x.topic) h+='<div class="mwHead">'+escHtml(x.topic)+(x.choice? ' <span class="from">- choice '+x.choice+'</span>' : '')+'</div>';
      const l=x.line;
      if(!l){ h+='<div class="mwSys">'+(x.topic? 'They have nothing to say about that.' : 'No greeting fits.')+'</div>'; return; }
      const tip=(l.conditions.length? 'Its conditions:\n'+l.conditions.join('\n') : 'No conditions')+(l.unchecked.length? '\n\nAssumed to hold (only the game knows):\n'+l.unchecked.join('\n') : '');
      h+='<div class="mwLine" title="'+escHtml(tip)+'">'+this.textHtml(l.segments)+
        ' <a class="mwEdit" data-dedit="'+escHtml(l.id)+'" title="'+escHtml(l.topic)+' #'+l.n+' - open it">✎</a>'+
        (l.unchecked.length? ' <span class="mwAssumed" title="'+escHtml(l.unchecked.join('\n'))+'">⚠ '+l.unchecked.length+' assumed</span>' : '')+'</div>';
      if(l.journal.length) h+='<div class="mwSys">Your journal has been updated.</div>';
      if(l.goodbye) h+='<div class="mwSys">(Goodbye)</div>';
      if(l.choices.length && i===P.log.length-1)
        h+='<div class="mwChoices">'+l.choices.map(c=>'<a class="mwChoice" data-pchoice="'+c.n+'" data-ptopicc="'+escHtml(l.topic)+'">'+escHtml(c.label)+'</a>').join('')+'</div>';
    });
    box.innerHTML=h||'<div class="mwSys">Choose who to talk to, and Talk.</div>';
    box.scrollTop=box.scrollHeight;
    box.querySelectorAll('[data-dtopic]').forEach(a=>a.onclick=e=>{ e.preventDefault(); this.say(a.dataset.dtopic, 0); });
    box.querySelectorAll('[data-pchoice]').forEach(a=>a.onclick=()=>this.say(a.dataset.ptopicc, +a.dataset.pchoice));
    box.querySelectorAll('[data-dedit]').forEach(a=>a.onclick=()=>this.E.openRecord('INFO', a.dataset.dedit, null));
  },
};
