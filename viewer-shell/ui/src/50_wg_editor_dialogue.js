/* =====================================================================================
   Wraithguard Editor (50_wg_editor.js): the Dialogue window: topics, responses in the engine's order, drag to reorder, copying a topic.

   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
Object.assign(WgEditor, {
  /* ---- the dialogue window ------------------------------------------------------------- */

  async showDialogue(){
    if(!this.dial){
      const d=document.createElement('div');
      d.id='edDial'; d.className='ori'; d.hidden=true;
      d.innerHTML='<div class="orihead"><b id="edDialTitle">Dialogue</b><span style="flex:1"></span>'+
        '<button class="btn dim ic" id="edDialX" title="Close">&#x2715;</button></div>'+
        '<div class="oribody"><div class="edTabs" id="edDialTabs"></div>'+
        '<input class="fld" id="edDialFilter" placeholder="Filter topics" spellcheck="false">'+
        '<div class="edSplit"><div class="edTable" id="edDialTopics"></div><div class="edTable" id="edDialResp"></div></div></div>';
      WgUI.mount(d);
      d.querySelector('#edDialX').onclick=()=>{ d.hidden=true; };
      d.querySelector('#edDialFilter').oninput=()=>this.drawTopics();
      d.querySelector('#edDialFilter').onkeydown=e=>e.stopPropagation();
      this.dial=d;
      // Flow, Map, Flags and In game beside the list (52_wg_dialogue_views.js).
      if(typeof WgDial==='object') WgDial.attach(this);
    }
    this.dial.hidden=false;
    if(!this.canEdit()){ this.dial.querySelector('#edDialTopics').innerHTML='<div class="hint">The dialogue window needs Wraithguard.</div>'; return; }
    if(!this.topicList){
      this.dial.querySelector('#edDialTopics').innerHTML='<div class="hint">Reading every plugin\'s topics…</div>';
      try{ this.topicList=await this.ask('editTopics', {}); }
      catch(e){ this.dial.querySelector('#edDialTopics').innerHTML='<div class="hint">'+escHtml(String(e.message||e))+'</div>'; return; }
      const kinds=[...new Set(this.topicList.map(t=>t.type))];
      this.dialKind=kinds.includes('Topic')? 'Topic' : kinds[0]||'';
      const tabs=this.dial.querySelector('#edDialTabs');
      tabs.innerHTML=kinds.map(k=>'<button class="btn sm'+(k===this.dialKind?' on':'')+'" data-kind="'+escHtml(k)+'">'+escHtml(k||'?')+'</button>').join('');
      tabs.querySelectorAll('[data-kind]').forEach(b=>b.onclick=()=>{
        this.dialKind=b.dataset.kind; tabs.querySelectorAll('.on').forEach(x=>x.classList.remove('on')); b.classList.add('on'); this.drawTopics();
      });
    }
    this.drawTopics();
  },

  drawTopics(){
    const box=this.dial.querySelector('#edDialTopics');
    const f=this.dial.querySelector('#edDialFilter').value.trim().toLowerCase();
    const list=(this.topicList||[]).filter(t=>t.type===this.dialKind && (!f || t.id.toLowerCase().includes(f)));
    box.innerHTML='<table class="edT"><tbody>'+list.slice(0,this.limit('topics')).map(t=>
      '<tr data-topic="'+escHtml(t.id)+'" title="'+escHtml(t.plugins.join(' > '))+'"><td>'+escHtml(t.id)+'</td><td class="from">'+t.plugins.length+'</td></tr>').join('')+'</tbody></table>'+
      this.moreHtml(list.length, 'topics');
    this.wireMore(box, 'topics', list.length, ()=>this.drawTopics());
    box.querySelectorAll('tr[data-topic]').forEach(tr=>tr.onclick=()=>{
      box.querySelectorAll('tr.sel').forEach(x=>x.classList.remove('sel')); tr.classList.add('sel');
      this.openTopic(tr.dataset.topic);
    });
  },

  async openTopic(topic){
    this._dialSel=topic;
    // The latest opening wins: an earlier one answered late (a response added, the pool
    // read again) must not draw over it - nor wipe what was marked on it since.
    const turn=this._topicTurn=(this._topicTurn||0)+1;
    const box=this.dial.querySelector('#edDialResp');
    box.innerHTML='<div class="hint">…</div>';
    let t;
    try{ t=await this.ask('editTopic', {topic}); }
    catch(e){ if(turn===this._topicTurn) box.innerHTML='<div class="hint">'+escHtml(String(e.message||e))+'</div>'; return null; }
    if(turn!==this._topicTurn) return this.dialTopic||null;
    this.dialTopic=t;
    const journal=t.type==='Journal';
    if(journal){
      // A quest: its stages by index (the order means nothing to a journal), its name and
      // its end marked.
      t.responses=t.responses.slice().sort((a,b)=>(a.disposition|0)-(b.disposition|0));
      const name=t.responses.find(r=>r.quest==='Name');
      if(name) t.questName=name.text;
    }
    box.innerHTML=(journal && t.questName? '<div class="orisec">Quest: '+escHtml(t.questName)+'</div>' : '')+
      '<div class="orirow"><span class="v edVec"><button class="btn sm" id="edRespTop" title="A new response at the top of the topic, made by the patch">Add at top</button>'+
      '<input class="fld" id="edTopicCopyId" placeholder="New topic name" spellcheck="false">'+
      '<button class="btn sm" id="edTopicCopy" title="This topic and its responses, in their order, under a new name - made by the patch">Copy topic as</button></span></div>'+
      '<table class="edT"><thead><tr><th>#</th><th>'+(journal? 'Index' : 'Who')+'</th><th>Text</th><th>From</th><th></th></tr></thead><tbody>'+
      t.responses.map((r,i)=>'<tr data-r="'+i+'"'+(this.isEditedInfo(r.id)? ' class="edited"' : '')+' title="'+escHtml(r.text)+'">'+
        '<td class="num">'+(i+1)+(r.orphan? ' <span class="bad" title="Its predecessor is not in the topic: it is read last">!</span>' : '')+'</td>'+
        '<td>'+escHtml(journal? String(r.disposition==null? '' : r.disposition) : r.speaker)+
          (journal && r.quest? ' <span class="from" title="'+escHtml(r.quest==='Name'? 'The quest\'s name in the journal' : r.quest==='Finished'? 'This stage ends the quest' : 'This stage restarts the quest')+'">'+escHtml(r.quest)+'</span>' : '')+'</td>'+
        '<td>'+escHtml(r.text.length>90? r.text.slice(0,90)+'…' : r.text)+'</td>'+
        '<td class="from" title="'+escHtml(r.plugins.join(' > '))+'">'+escHtml(r.winner)+'</td>'+
        '<td><button class="btn dim ic" data-after="'+escHtml(r.id)+'" title="A new response after this one, made by the patch">+</button></td></tr>').join('')+'</tbody></table>';
    box.querySelectorAll('tr[data-r]').forEach(tr=>{
      const r=t.responses[+tr.dataset.r];
      tr.ondblclick=()=>this.openRecord('INFO', r.id, r.winner==='(this patch)'? null : r.plugins);
    });
    box.querySelector('#edRespTop').onclick=()=>this.addResponse(t.id, '');
    const cpIn=box.querySelector('#edTopicCopyId');
    cpIn.onkeydown=e=>{ if(e.key==='Enter') this.copyTopic(t.id); e.stopPropagation(); };
    box.querySelector('#edTopicCopy').onclick=()=>this.copyTopic(t.id);
    box.querySelectorAll('[data-after]').forEach(b=>b.onclick=e=>{ e.stopPropagation(); this.addResponse(t.id, b.dataset.after); });
    if(!journal) this.wireResponseDrag(box, t);
    return t;
  },

  /** Drag a response up or down its topic: let go on the upper half of a row to put it
   *  before that one, on the lower half after it. The move is queued as its `prev_id`
   *  (Wraithguard's `editMoveResponse`), the order the engine reads redrawn after. */
  wireResponseDrag(box, t){
    const rows=[...box.querySelectorAll('tr[data-r]')];
    rows.forEach(tr=>{
      tr.style.userSelect='none';
      tr.title=(tr.title? tr.title+'\n\n' : '')+'Drag to move it in the topic';
      tr.addEventListener('pointerdown', e=>{
        if(e.button!==0 || (e.target.closest && e.target.closest('button,input,a'))) return;
        e.preventDefault();
        const from=t.responses[+tr.dataset.r], y0=e.clientY;
        let live=false, target=null, below=false;
        const clear=()=>rows.forEach(r=>r.classList.remove('edDropBefore','edDropAfter','edDragging'));
        const mv=ev=>{
          if(!live){ if(Math.abs(ev.clientY-y0)<5) return; live=true; tr.classList.add('edDragging'); }
          const over=document.elementFromPoint(ev.clientX, ev.clientY);
          const row=over && over.closest && over.closest('tr[data-r]');
          rows.forEach(r=>r.classList.remove('edDropBefore','edDropAfter'));
          target=null;
          if(row && rows.includes(row) && row!==tr){
            const rc=row.getBoundingClientRect(); below=ev.clientY>rc.top+rc.height/2;
            row.classList.add(below? 'edDropAfter' : 'edDropBefore'); target=row;
          }
        };
        const up=async()=>{
          window.removeEventListener('pointermove', mv, true); window.removeEventListener('pointerup', up, true);
          clear();
          if(!live || !target) return;
          const i=+target.dataset.r;
          // "After" names the response it follows: the target, or the one before it.
          let after= below? t.responses[i].id : (i>0? t.responses[i-1].id : '');
          if(after===from.id) return;
          try{ await this.ask('editMoveResponse', {topic:t.id, id:from.id, after}); }
          catch(err){ return toast(String(err.message||err).replace(/^Wraithguard answered 400:\s*/,''),'err',6000); }
          await this.refreshPending();
          await this.openTopic(t.id);
          toast('Moved: it reads '+(after? 'after '+after : 'first')+' now (queued in the patch)','ok',3000);
        };
        window.addEventListener('pointermove', mv, true); window.addEventListener('pointerup', up, true);
      });
    });
  },

  isEditedInfo(id){ return this.edited.has('INFO:'+String(id).toLowerCase()); },

  /** "Copy topic as": the topic and its responses under a new name, made by the patch;
   *  the topic list then has it, and it opens. */
  async copyTopic(topic){
    const box=this.dial.querySelector('#edTopicCopyId'), newId=box.value.trim();
    if(!newId){ box.classList.add('bad'); return null; }
    let t;
    try{ t=await this.ask('editCopyTopic', {topic, newId}); }
    catch(e){ box.classList.add('bad'); toast(String(e.message||e).replace(/^Wraithguard answered 400:\s*/,''),'err',6000); return null; }
    this.topicList=null;
    await this.showDialogue();
    await this.refreshPending();
    await this.openTopic(t.id);
    toast(t.id+': '+t.responses.length+' response'+(t.responses.length===1?'':'s')+' copied','ok',4000);
    return t;
  },

  /** A new response in a topic, after another (or at the top): made by the patch, opened
   *  in the record dialog, and shown in its place. */
  async addResponse(topic, after){
    let v;
    try{ v=await this.ask('editNewResponse', {topic, after}); }
    catch(e){ toast(String(e.message||e).replace(/^Wraithguard answered 400:\s*/,''),'err',6000); return null; }
    this.record={tag:'INFO', id:v.id, plugins:null};
    this.dialog().hidden=false;
    this.drawRecord(v);
    await this.refreshPending();
    await this.openTopic(topic);
    return v;
  },
});
