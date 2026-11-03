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
     topics they answer, the line the engine would pick for each, hyperlinks and Choice
     buttons that work. Checked: the speaker's conditions; the cell (chosen, the
     Preview's, or where the speaker is placed), NotCell with it; the player's sex, race,
     faction, rank and level, and the speaker's rank, as set or read from a save; globals
     and quest indexes from the Scripts panel's game state (shared), and with New game what
     is not set at 0. What is still unknown is said beside the line and assumed true, or
     skipped with Strict.

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
    /* Rings by depth, each as wide as its topics need (Static, 2026-11-02: "it gets crowded
       and unreadable"): a ring's length is its labels' widths, so a busy ring is a big one.
       On each, what leads here on the left and the rest on the right. The drawing is as
       large as that makes it; the view zooms and pans over it. */
    const short=l=>l.length>22? l.slice(0,21)+'…' : l;
    const width=l=>Math.max(60, l.length*6.6+16);
    const pos=new Map();
    const byDepth=new Map();
    for(const n of m.nodes){ if(!byDepth.has(n.depth)) byDepth.set(n.depth, []); byDepth.get(n.depth).push(n); }
    const incoming=new Set(m.edges.filter(e=>e.to===m.centre).map(e=>e.from));
    let r=0, reach=120;
    for(const d of [...byDepth.keys()].sort((a,b)=>a-b)){
      const list=byDepth.get(d);
      if(d===0){ list.forEach(n=>pos.set(n.id, [0, 0])); continue; }
      // Around an ellipse 1.35 times wider than tall: each label its width plus a gap.
      const need=list.reduce((t, n)=>t+width(short(n.label))*0.75+26, 0);
      r=Math.max(r+130, 220, need/(2*Math.PI));
      const left=list.filter(n=>incoming.has(n.id)), right=list.filter(n=>!incoming.has(n.id));
      // Each side gets the share of the ring its count asks for.
      const share=list.length? left.length/list.length : 0.5;
      const lw=Math.PI*2*Math.min(0.75, Math.max(0.25, share));
      const place=(arr, a0, a1)=>arr.forEach((n,i)=>{
        const a=a0+(a1-a0)*((i+0.5)/Math.max(1, arr.length));
        pos.set(n.id, [Math.cos(a)*r*1.35, Math.sin(a)*r]);
      });
      place(left, Math.PI-lw/2, Math.PI+lw/2);
      place(right, -(Math.PI*2-lw)/2, (Math.PI*2-lw)/2);
      reach=r;
    }
    const W=Math.ceil(reach*1.35*2+260), H=Math.ceil(reach*2+120), cx=W/2, cy=H/2;
    for(const [k, p] of pos) pos.set(k, [p[0]+cx, p[1]+cy]);
    const COL={addtopic:'#6cb6ff', link:'#9a9a9a', quest:'#c9a227', sets:'#3fb950', reads:'#e5534b'};
    let svg='<svg class="edDMap" viewBox="0 0 '+W+' '+H+'" xmlns="http://www.w3.org/2000/svg" preserveAspectRatio="xMidYMid meet">'+
      '<defs>'+Object.entries(COL).map(([k,c])=>'<marker id="edAr_'+k+'" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M0 0L10 5L0 10z" fill="'+c+'"/></marker>').join('')+'</defs>'+
      '<rect class="edDBack" x="0" y="0" width="'+W+'" height="'+H+'" fill="transparent"/><g class="edDMapG">';
    for(const e of m.edges){
      const a=pos.get(e.from), b=pos.get(e.to); if(!a || !b) continue;
      const dx=b[0]-a[0], dy=b[1]-a[1], L=Math.hypot(dx,dy)||1, k=Math.min(0.42, 46/L);
      svg+='<line x1="'+(a[0]+dx*k).toFixed(1)+'" y1="'+(a[1]+dy*k).toFixed(1)+'" x2="'+(b[0]-dx*k).toFixed(1)+'" y2="'+(b[1]-dy*k).toFixed(1)+'" stroke="'+COL[e.kind]+'" stroke-width="1.4"'+
        (e.kind==='link'? ' stroke-dasharray="4 3"' : '')+' marker-end="url(#edAr_'+e.kind+')"><title>'+escHtml(e.kind)+'</title></line>';
    }
    for(const n of m.nodes){
      const p=pos.get(n.id); if(!p) continue;
      const label=short(n.label), w=width(label), h=24;
      const cls='edDNode '+(n.kind==='quest'? 'quest' : n.kind==='var'? 'var' : n.depth===0? 'centre' : 'topic');
      svg+='<g class="'+cls+'" data-nid="'+escHtml(n.id)+'" transform="translate('+p[0].toFixed(1)+','+p[1].toFixed(1)+')">'+
        '<rect x="'+(-w/2)+'" y="'+(-h/2)+'" width="'+w+'" height="'+h+'" rx="'+(n.kind==='var'? 12 : 4)+'" data-w="'+w+'" data-fw="'+width(n.label)+'"/>'+
        '<text text-anchor="middle" dy="4" data-short="'+escHtml(label)+'" data-full="'+escHtml(n.label)+'">'+escHtml(label)+'</text><title>'+escHtml(n.label+(n.responses? ' - '+n.responses+' responses' : '')+
        (n.kind==='quest'? ' (quest)' : n.kind==='var'? ' (variable)' : ''))+'</title></g>';
    }
    svg+='</g>';
    svg+='</svg>';
    box.innerHTML='<div class="edDTop"><b>'+escHtml(topic)+'</b> <span class="from">'+m.nodes.length+' nodes'+(m.capped? ' (the first '+m.nodes.length+')' : '')+'</span><span style="flex:1"></span>'+
      '<label class="from" title="Two steps out: the neighbours\' neighbours too"><input type="checkbox" id="edDDepth"'+(this.depth===2?' checked':'')+'> 2 steps</label> '+
      '<button class="btn sm" id="edDZoomOut" title="Zoom out (or the wheel)">−</button><button class="btn sm" id="edDZoomIn" title="Zoom in (or the wheel)">+</button>'+
      '<button class="btn sm" id="edDFit" title="The whole map in view">Fit</button> '+
      '<button class="btn sm" id="edDFlowIt">Flow</button></div>'+
      '<div class="edDLegend">'+Object.entries({addtopic:'AddTopic', link:'hyperlinked word', quest:'advances a quest', sets:'sets a variable', reads:'tests it'})
        .map(([k,l])=>'<span><i style="background:'+COL[k]+'"></i>'+l+'</span>').join('')+'</div>'+
      '<div class="hint">Click a topic to centre it, double-click for its flow; a quest opens its stages, a variable its readers and writers. Wheel to zoom, drag to move; the corner makes the map taller.</div>'+
      '<div class="edDMapWrap">'+svg+'</div>';
    this.zoomPan(box.querySelector('.edDMap'));
    box.querySelector('#edDDepth').onchange=e=>{ this.depth=e.target.checked? 2 : 1; this.map(topic); };
    box.querySelector('#edDFlowIt').onclick=()=>this.show('flow', topic);
    box.querySelectorAll('g[data-nid]').forEach(g=>{
      const id=g.dataset.nid, kind=id.slice(0,2), name=(m.nodes.find(n=>n.id===id)||{}).label||id.slice(2);
      g.onclick=()=>{ if(kind==='t:') this.map(name); else if(kind==='q:') this.show('flow', name); else this.flags(name); };
      g.ondblclick=()=>{ if(kind==='t:') this.show('flow', name); };
    });
  },

  /** The map's view: the wheel zooms about the pointer, a drag moves it, the buttons zoom
   *  about the middle and Fit shows the whole map. Zoomed in, labels are written in full. */
  zoomPan(svg){
    if(!svg) return;
    const g=svg.querySelector('.edDMapG'), box=this.box;
    const v=this._mapView={k:1, x:0, y:0};
    const draw=()=>{
      g.setAttribute('transform', 'translate('+v.x.toFixed(1)+','+v.y.toFixed(1)+') scale('+v.k.toFixed(4)+')');
      const full=v.k>=1.6;
      if(full!==v.full){
        v.full=full;
        g.querySelectorAll('text[data-full]').forEach(t=>{ t.textContent=full? t.dataset.full : t.dataset.short; });
        g.querySelectorAll('rect[data-fw]').forEach(rc=>{ const w=+(full? rc.dataset.fw : rc.dataset.w); rc.setAttribute('x', -w/2); rc.setAttribute('width', w); });
      }
    };
    const vb=svg.viewBox && svg.viewBox.baseVal;
    // The drawing's own units under a screen point.
    const at=(cx, cy)=>{
      const r=svg.getBoundingClientRect(), W=vb? vb.width : 1, H=vb? vb.height : 1;
      const s=Math.min(r.width/W, r.height/H)||1, ox=(r.width-W*s)/2, oy=(r.height-H*s)/2;
      return [(cx-r.left-ox)/s, (cy-r.top-oy)/s];
    };
    const zoom=(f, px, py)=>{
      const k=Math.max(0.2, Math.min(8, v.k*f)), f2=k/v.k;
      v.x=px-(px-v.x)*f2; v.y=py-(py-v.y)*f2; v.k=k; draw();
    };
    const mid=()=>[vb? vb.width/2 : 0, vb? vb.height/2 : 0];
    svg.addEventListener('wheel', e=>{ e.preventDefault(); const [px, py]=at(e.clientX, e.clientY); zoom(Math.exp(-e.deltaY*0.0015), px, py); }, {passive:false});
    let drag=null;
    svg.addEventListener('pointerdown', e=>{ if(e.button!==0 && e.button!==1) return; drag={x:e.clientX, y:e.clientY, vx:v.x, vy:v.y, moved:false}; });
    svg.addEventListener('pointermove', e=>{
      if(!drag) return;
      const [ax, ay]=at(drag.x, drag.y), [bx, by]=at(e.clientX, e.clientY);
      if(!drag.moved && Math.hypot(e.clientX-drag.x, e.clientY-drag.y)<4) return;
      if(!drag.moved){ drag.moved=true; try{ svg.setPointerCapture(e.pointerId); }catch(_){ } svg.classList.add('dragging'); }
      v.x=drag.vx+(bx-ax); v.y=drag.vy+(by-ay); draw();
    });
    const end=e=>{ if(!drag) return; if(drag.moved){ this._dragged=performance.now(); svg.classList.remove('dragging'); try{ svg.releasePointerCapture(e.pointerId); }catch(_){ } } drag=null; };
    svg.addEventListener('pointerup', end); svg.addEventListener('pointercancel', end);
    // A drag that ends on a node is not a click on it.
    svg.addEventListener('click', e=>{ if(this._dragged && performance.now()-this._dragged<250){ e.stopPropagation(); e.preventDefault(); } }, true);
    const q=s=>box.querySelector(s);
    q('#edDZoomIn').onclick=()=>zoom(1.3, ...mid());
    q('#edDZoomOut').onclick=()=>zoom(1/1.3, ...mid());
    q('#edDFit').onclick=()=>{ v.k=1; v.x=0; v.y=0; draw(); };
    draw();
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

  /** A script's ForceGreeting: the Dialogue window's In game tab, talking to `id` in the
   *  Preview's cell, the game state the scripts have. False when the Editor's window is not
   *  there to open. */
  async greetFrom(id){
    const E=this.E || (typeof WgEditor==='object'? WgEditor : null);
    if(!E || !E.showDialogue || !(E.canEdit && E.canEdit())) return false;
    await E.showDialogue();
    if(!this.box) return false;
    this._play=this._play||null;
    await this.show('play');
    const P=this._play;
    P.speaker=String(id); P.cellFrom='current'; P.newGame=false;
    await this.cellFor();
    P.log=[];
    await this.play();
    await this.say('', 0);
    return true;
  },

  async play(){
    const box=this.box;
    const P=this._play=this._play||{speaker:'', log:[], cell:'', cellFrom:'set', disposition:50, strict:false, topics:[], name:'',
      pcSex:'Any', pcRace:'', pcFaction:'', pcRank:'', pcLevel:'', npcRank:'', newGame:true};
    const opt=(v, l, on)=>'<option value="'+v+'"'+(on? ' selected' : '')+'>'+l+'</option>';
    box.innerHTML='<div class="edDTop edVec">'+
      '<input class="fld" id="edPSpeaker" list="edPNpcs" placeholder="Talk to: an NPC or creature id" value="'+escHtml(P.speaker)+'" spellcheck="false" title="Who the player talks to">'+
      '<datalist id="edPNpcs"></datalist>'+
      '<label class="from" title="The NPC\'s disposition: responses needing more are not said">Disp <input type="number" class="fld" id="edPDisp" min="0" max="100" value="'+P.disposition+'" style="width:56px"></label>'+
      '<label class="from" title="Skip responses with conditions only the running game knows, rather than assuming they hold"><input type="checkbox" id="edPStrict"'+(P.strict?' checked':'')+'> Strict</label>'+
      '<button class="btn sm pri" id="edPGreet" title="Start the conversation: their greeting">Talk</button></div>'+
      '<div class="edDTop edVec">'+
      '<label class="from" title="Where the conversation is: responses for one cell (and NotCell) are checked against it">Cell</label>'+
      '<select class="fld" id="edPCellFrom" style="max-width:150px">'+opt('set','Chosen', P.cellFrom==='set')+opt('current','The Preview\'s cell', P.cellFrom==='current')+opt('npc','Where they are', P.cellFrom==='npc')+'</select>'+
      '<input class="fld" id="edPCell" list="edPCells" placeholder="Any cell (assumed to fit)" value="'+escHtml(P.cell)+'" spellcheck="false" style="max-width:220px">'+
      '<datalist id="edPCells"></datalist></div>'+
      '<details class="edPState"'+(P.stateOpen? ' open' : '')+'><summary>The player and the game <span class="from" id="edPStateSum"></span></summary>'+
      '<div class="edDTop edVec">'+
        '<label class="from">Sex <select class="fld" id="edPSex">'+['Any','Male','Female'].map(x=>opt(x, x, P.pcSex===x)).join('')+'</select></label>'+
        '<label class="from">Race <input class="fld" id="edPRace" list="edPRaces" value="'+escHtml(P.pcRace)+'" placeholder="Any" style="width:110px"></label><datalist id="edPRaces"></datalist>'+
        '<label class="from">Faction <input class="fld" id="edPFaction" list="edPFactions" value="'+escHtml(P.pcFaction)+'" placeholder="None set" style="width:130px"></label><datalist id="edPFactions"></datalist>'+
        '<label class="from" title="The player\'s rank in that faction">Rank <input type="number" class="fld" id="edPRank" min="0" max="9" value="'+escHtml(P.pcRank)+'" style="width:50px"></label>'+
        '<label class="from">Level <input type="number" class="fld" id="edPLevel" min="1" value="'+escHtml(P.pcLevel)+'" style="width:56px"></label>'+
        '<label class="from" title="The speaker\'s rank in their faction; empty: their record\'s">Their rank <input type="number" class="fld" id="edPNpcRank" min="0" max="9" value="'+escHtml(P.npcRank)+'" placeholder="theirs" style="width:60px"></label></div>'+
      '<div class="edDTop edVec">'+
        '<label class="from" title="Quests, items and deaths not set are at 0, as when a game starts; off, they are only assumed"><input type="checkbox" id="edPNew"'+(P.newGame?' checked':'')+'> New game</label>'+
        '<button class="btn sm" id="edPSave" title="The player, globals and quest indexes from an OpenMW save">From a save…</button>'+
        '<button class="btn sm" id="edPGlobals" title="Globals and quest indexes: the Scripts panel\'s game state, shared">Globals and journal…</button>'+
        '<span class="from" id="edPGameSum"></span></div></details>'+
      '<div class="mwDlg"><div class="mwDlgL"><div class="mwDlgName" id="edPName">'+escHtml(P.name||'')+'</div><div class="mwDlgLog" id="edPLog"></div></div>'+
      '<div class="mwDlgR"><div class="mwDlgTopics" id="edPTopics"></div><div class="mwDlgBye"><button class="mwBtn" id="edPBye">Goodbye</button></div></div></div>';
    const $b=s=>box.querySelector(s);
    const read=()=>{
      P.speaker=$b('#edPSpeaker').value.trim(); P.cell=$b('#edPCell').value.trim(); P.cellFrom=$b('#edPCellFrom').value;
      P.disposition=+$b('#edPDisp').value||0; P.strict=$b('#edPStrict').checked;
      P.pcSex=$b('#edPSex').value; P.pcRace=$b('#edPRace').value.trim(); P.pcFaction=$b('#edPFaction').value.trim();
      P.pcRank=$b('#edPRank').value; P.pcLevel=$b('#edPLevel').value; P.npcRank=$b('#edPNpcRank').value; P.newGame=$b('#edPNew').checked;
      P.stateOpen=$b('.edPState').open;
      this.drawStateSum();
    };
    for(const id of ['#edPSpeaker','#edPCell','#edPDisp']) $b(id).onkeydown=e=>{ if(e.key==='Enter') $b('#edPGreet').onclick(); e.stopPropagation(); };
    box.querySelectorAll('.edPState input, .edPState select').forEach(el=>el.onchange=read);
    $b('#edPCellFrom').onchange=async()=>{ read(); await this.cellFor(); };
    $b('#edPGreet').onclick=async()=>{ read(); await this.cellFor(); P.log=[]; this.say('', 0); };
    $b('#edPBye').onclick=()=>{ P.log.push({bye:true}); this.drawLog(); };
    $b('#edPSave').onclick=()=>this.stateFromSave();
    $b('#edPGlobals').onclick=()=>{ if(typeof WgScripts==='object'){ WgScripts.panel(); WgScripts._el.hidden=false; WgScripts.drawState(); } };
    this.fillNpcs($b('#edPNpcs'));
    this.fillLists();
    if(typeof WgScripts==='object' && WgScripts.loadGlobals) WgScripts.loadGlobals().then(()=>this.drawStateSum());
    this.drawLog(); this.drawTopics(); this.drawStateSum();
  },

  /** The cell the conversation is in, as chosen: typed or picked, the Preview's, or where
   *  the speaker is placed (the first cell the load order puts them in). */
  async cellFor(){
    const P=this._play, el=this.box.querySelector('#edPCell');
    if(P.cellFrom==='current'){
      const sc=(typeof App==='object' && App._scene)||{}, c=sc.centre||(sc.cells||[])[0];
      P.cell=c? String(c.name||'') : '';
      if(!P.cell && c && c.kind!=='int') toast('The Preview\'s square has no name: any cell is assumed','warn',3000);
    }else if(P.cellFrom==='npc' && P.speaker){
      try{
        const r=await this.E.ask('editUses', {tag:'NPC_', id:P.speaker});
        const cells=[...new Set(((r && r.uses)||[]).filter(u=>u.type==='Cell' && u.wins!==false).map(u=>u.key))];
        const named=cells.map(k=>{
          const m=/^\(\s*(-?\d+)\s*,\s*(-?\d+)\s*\)$/.exec(k);
          if(!m) return k;
          const c=(this.E.cells||[]).find(c=>c.kind==='ext' && c.x===+m[1] && c.y===+m[2]);
          return (c && c.name) || '';
        });
        P.cell=named.find(Boolean)||'';
        if(!cells.length) toast(P.speaker+' is placed in no cell: any cell is assumed','warn',3000);
        else if(cells.length>1) toast(P.speaker+' is in '+cells.length+' cells; the first: '+(P.cell||cells[0]),'ok',3000);
      }catch(e){ toast(String(e.message||e),'err',4000); }
    }
    if(el) el.value=P.cell;
  },

  /** The cells, races and factions the boxes offer. */
  async fillLists(){
    const b=this.box, fill=(sel, items)=>{ const dl=b.querySelector(sel); if(dl) dl.innerHTML=items.slice(0, 5000).map(v=>'<option value="'+escHtml(v)+'">').join(''); };
    fill('#edPCells', [...new Set((this.E.cells||[]).map(c=>c.name).filter(Boolean))].sort());
    if(!this._lists){
      this._lists={race:[], fact:[]};
      try{
        const [r, f]=await Promise.all([WgEditor.recordRows('RACE'), WgEditor.recordRows('FACT')]);
        this._lists={race:r.rows.map(x=>x[0]), fact:f.rows.map(x=>x[0])};
      }catch(_){ }
    }
    fill('#edPRaces', this._lists.race); fill('#edPFactions', this._lists.fact);
  },

  /** The player, globals and quest indexes from an OpenMW save: the player into the boxes,
   *  globals and the journal into the Scripts panel's game state (shared). */
  async stateFromSave(){
    const g=typeof WgScripts==='object'? await WgScripts.fromSave() : null;
    if(!g) return;
    const P=this._play;
    Object.assign(P, {pcSex:g.sex||'Any', pcRace:g.race||'', pcLevel:String(g.level||''), newGame:false, stateOpen:true});
    const f=(g.factions||[])[0];
    P.pcFaction=f? f[0] : ''; P.pcRank=f && f[1]>=0? String(f[1]) : '';
    // Every faction the save has the player in: each condition asks the rank in its own.
    P.pcFactions=(g.factions||[]).map(([id, r])=>[String(id), +r]);
    this.play();
    toast((g.name||'The player')+': '+[g.sex, g.race, g.class, 'level '+g.level].filter(Boolean).join(', ')+'; '+(g.globals||[]).length+' globals, '+(g.journal||[]).length+' quests'+
      ((g.factions||[]).length>1? '; '+g.factions.length+' factions, each ranked' : '')+((g.inventory||[]).length? '; carries '+g.inventory.length+' kinds of item' : ''),'ok',6000);
  },

  /** The game state the rehearsal is given, as the engine takes it. */
  playState(){
    const P=this._play, S=typeof WgScripts==='object'? WgScripts.state : null;
    const num=v=>v===''||v==null? null : +v;
    const items={};
    if(typeof WgScripts==='object' && WgScripts.actors.get('player')) for(const [k, n] of WgScripts.actors.get('player').items) items[k]=n;
    // The player's factions: a save's, with the one set here over it.
    const factions={};
    for(const [id, r] of P.pcFactions||[]) factions[id.toLowerCase()]=r;
    if(P.pcFactions && P.pcFactions.length && P.pcFaction) factions[P.pcFaction.toLowerCase()]=num(P.pcRank)==null? -1 : num(P.pcRank);
    // The speaker's MWScript locals as the scene has them (a placed one's script running).
    const locals={};
    if(typeof WgScripts==='object' && WgScripts.inst && P.speaker){
      const who=String(P.speaker).toLowerCase();
      for(const it of WgScripts.inst.values()) if(it.id===who && it.locals){ for(const [n, l] of it.locals) locals[n]=l.v; break; }
    }
    return {pcSex:P.pcSex, pcRace:P.pcRace, pcFaction:P.pcFaction, pcRank:num(P.pcRank), pcLevel:num(P.pcLevel), npcRank:num(P.npcRank), newGame:P.newGame,
      globals:S? Object.fromEntries([...S.globals].map(([k, g])=>[k, g.v])) : {}, journal:S? Object.fromEntries(S.journal) : {}, items, factions, locals};
  },
  drawStateSum(){
    const P=this._play, b=this.box; if(!P || !b) return;
    const a=b.querySelector('#edPStateSum'), g=b.querySelector('#edPGameSum');
    const set=[P.pcSex!=='Any'? P.pcSex : '', P.pcRace, P.pcFaction? P.pcFaction+(P.pcRank!==''? ' '+P.pcRank : '') : '', P.pcLevel!==''? 'level '+P.pcLevel : ''].filter(Boolean);
    if(a) a.textContent=set.length? '- '+set.join(', ') : '- nothing set: what they test is assumed';
    const S=typeof WgScripts==='object'? WgScripts.state : null;
    if(g) g.textContent=S? S.globals.size+' globals, '+S.journal.size+' quests set'+(P.newGame? '; the rest at 0' : '') : '';
  },

  /** The NPC ids for the speaker box, once (the engine's list). */
  async fillNpcs(dl){
    if(!this._npcs){
      this._npcs=[];
      try{
        const [n, c]=await Promise.all([WgEditor.recordRows('NPC_'), WgEditor.recordRows('CREA')]);
        this._npcs=[...n.rows, ...c.rows].map(r=>[r[0], r[1]]);
      }catch(e){ if(typeof WgLog==='object') WgLog.add('The actor list did not load: '+String(e && e.message || e), 'err'); }
    }
    dl.innerHTML=this._npcs.slice(0, 5000).map(([id, name])=>'<option value="'+escHtml(id)+'">'+escHtml(name||'')+'</option>').join('');
  },

  /** Asks the rehearsal: a topic (or the greeting), with a choice made. */
  async say(topic, choice){
    const P=this._play;
    if(!P.speaker) return toast('Choose who to talk to','warn',2500);
    let r;
    try{ r=await this.E.ask('editDialoguePlay', {speaker:P.speaker, topic, choice, cell:P.cell, disposition:P.disposition, strict:P.strict, state:this.playState()}); }
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
