/* =====================================================================================
   Wraithguard: the Editor's workflow (wraithguard/patch/workflow.py answers most of it).

   - Prefabs: a selection kept as a group (records, places relative to their middle,
     turns, scales) and placed anywhere, as one change (`editPlaceMany`).
   - New cells from nothing: a blank interior, a new exterior square (`editNewCell`).
   - The dialogue condition editor: a response's conditions as rows of lists (type,
     function, comparison, id offered from the load order, value), and "Who can say
     this" against the load order's NPCs (`editFilterChoices`, `editWhoCanSay`).
   - Script Edit: completion of functions, keywords, globals and ids (Ctrl+Space, or as
     a word is typed); Ctrl+click a word to open what it names; every script naming it
     (`editScriptWords`, `editScriptRefs`).
   - A leveled list rolled at a chosen level, lists inside it too (`editLeveledRoll`).
   - Search everything: text in every record type at once, and replaced in the fields
     chosen as one change (`editSearchAll`, `editReplaceAll`; Ctrl+Shift+F).

   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
const WgFlow={
  err(e){ return String(e && e.message || e).replace(/^Wraithguard answered 400:\s*/,''); },
  ask(link, body){ return WgEditor.ask(link, body); },
  has(link){ return !!WgEditor.links()[link]; },

  /* ---- prefabs ------------------------------------------------------------------------ */

  prefabs(){ try{ const v=JSON.parse(localStorage.getItem('wgEditorPrefabs')||'{}'); return v && typeof v==='object'? v : {}; }catch(_){ return {}; } },
  keepPrefabs(v){ try{ localStorage.setItem('wgEditorPrefabs', JSON.stringify(v)); }catch(_){ } },

  /** The selection as a prefab: each record, its place from the middle, its turn and size. */
  async keepPrefab(){
    const E=WgEditor, items=E.transformTargets();
    if(!items.length) return toast('Select the objects first (Ctrl+click for several)','warn',2500);
    const name=await WgUI.askText('Keep '+items.length+' object'+(items.length===1?'':'s')+' as a prefab named', '', 'e.g. Market stall');
    if(!name) return null;
    const mid=E.middleOf(items);
    const parts=[];
    for(const g of items){
      const rec=E.selectedRecord({hit:g.hit, ref:g.ref});
      if(!rec || !rec.tag) continue;
      parts.push({tag:rec.tag, id:rec.id, offset:[0,1,2].map(k=>g.hit.wpos[k]-mid[k]), rotation:(g.hit.rot||[0,0,0]).slice(), scale:g.hit.scale||1});
    }
    if(!parts.length) return toast('Their records are not known yet','warn',2500);
    const all=this.prefabs(); all[name]=parts; this.keepPrefabs(all);
    toast('Prefab '+name+' kept ('+parts.length+') - Ctrl+Shift+P places it under the pointer','ok',3500);
    return parts;
  },

  /** A prefab placed with its middle at `at` (the game's coordinates), in `cell`. */
  async placePrefab(name, at, cell){
    const parts=this.prefabs()[name];
    if(!parts) return null;
    const items=parts.map(p=>({tag:p.tag, id:p.id, translation:[0,1,2].map(k=>at[k]+p.offset[k]), rotation:p.rotation, scale:p.scale}));
    let r;
    try{ r=await this.ask('editPlaceMany', {cell, items}); }
    catch(e){ toast(this.err(e),'err',6000); return null; }
    toast(name+' placed ('+r.placed.length+')','ok',2000);
    await WgEditor.refreshPending();
    return r;
  },

  /** Ctrl+Shift+P: the prefabs, to place under the pointer (or keep the selection as one). */
  prefabMenu(x, y){
    const E=WgEditor, all=this.prefabs(), names=Object.keys(all).sort((a,b)=>a.localeCompare(b));
    const where=()=>{
      const pt=E._pointer? E.placePoint(E._pointer[0], E._pointer[1]) : null;
      const c=E.currentCell();
      if(!pt || !c) return null;
      const w=pt.world, key=c.key.startsWith('(')? '('+Math.floor(w[0]/8192)+', '+Math.floor(w[1]/8192)+')' : c.key;
      return {at:w, cell:key};
    };
    WgUI.menu({x:x||200, y:y||200}, [
      {head:'Prefabs'},
      {label:'Keep the selection as a prefab…', act:()=>this.keepPrefab(), disabled:!E.transformTargets().length},
      ...(names.length? [{sep:true}] : []),
      ...names.map(n=>({label:'Place '+n, title:all[n].length+' objects, their middle under the pointer', disabled:!this.has('editPlaceMany'),
        act:()=>{ const w=where(); if(!w) return toast('Point at the ground of a loaded cell','warn',2500); this.placePrefab(n, w.at, w.cell); }})),
      ...(names.length? [{label:'Forget one', sub:names.map(n=>({label:n, act:()=>{ const v=this.prefabs(); delete v[n]; this.keepPrefabs(v); }}))}] : []),
    ]);
  },

  /* ---- new cells ------------------------------------------------------------------- */

  async newCell(exterior){
    const name=await WgUI.askText(exterior? 'A new exterior square at' : 'A new interior named', '', exterior? '(x, y), e.g. (40, -12)' : 'e.g. Vivec, My Shop');
    if(!name) return null;
    let cell=name.trim();
    if(exterior){
      const m=cell.match(/^\(?\s*(-?\d+)\s*,\s*(-?\d+)\s*\)?$/);
      if(!m) return toast('An exterior square is two numbers: (x, y)','warn',3000);
      cell='('+(+m[1])+', '+(+m[2])+')';
    }
    try{
      const r=await this.ask('editNewCell', {cell});
      toast((r.interior? 'Interior ' : 'Exterior square ')+r.cell+' made by the patch - drag records into it from the Object Window','ok',4000);
      await WgEditor.refreshPending();
      return r;
    }catch(e){ toast(this.err(e),'err',5000); return null; }
  },

  /* ---- the record dialog: dialogue conditions, leveled lists ------------------------- */

  /** What the record dialog gets for its type (drawRecord calls it). */
  extendRecord(E, body, v){
    if(v.tag==='INFO' && this.has('editFilterChoices')) this.conditions(E, body, v);
    if((v.tag==='LEVI' || v.tag==='LEVC') && this.has('editLeveledRoll')) this.rollBox(E, body, v);
  },

  /** The conditions a filter type's id names (what is offered as it is typed). */
  FILTER_TAGS:{Global:['GLOB'], Journal:['DIAL'], Item:['ALCH','APPA','ARMO','BOOK','CLOT','INGR','LIGH','LOCK','MISC','PROB','REPA','WEAP'],
    Dead:['NPC_','CREA'], NotId:['NPC_','CREA'], NotFaction:['FACT'], NotClass:['CLAS'], NotRace:['RACE']},

  async conditions(E, body, v){
    if(!this._choices){ try{ this._choices=await this.ask('editFilterChoices', {}); }catch(_){ return; } }
    const C=this._choices;
    const f=v.fields.find(x=>x.path==='filters');
    let rows=JSON.parse(JSON.stringify(f? (('queued' in f)? f.queued : f.value)||[] : []));
    const box=document.createElement('details');
    box.className='edConds'; box.open=true;
    body.insertBefore(box, body.querySelector('details.edAllFields')||body.querySelector('.edSecBar')||null);
    const sel=(list, cur, attr)=>'<select class="fld" '+attr+'>'+list.map(o=>'<option'+(o===cur?' selected':'')+'>'+escHtml(o)+'</option>').join('')+'</select>';
    const draw=()=>{
      box.innerHTML='<summary>Conditions ('+rows.length+')</summary>'+
        rows.map((r,i)=>{
          const val=r.value && typeof r.value==='object'? r.value.data : r.value;
          return '<div class="orirow edCond" data-i="'+i+'">'+sel(C.types, r.filter_type||'None', 'data-k="filter_type"')+
            (r.filter_type==='Function'? sel(C.functions, r.function, 'data-k="function"') : '<input class="fld" data-k="id" spellcheck="false" placeholder="id or name" value="'+escHtml(r.id||'')+'">')+
            sel(C.comparisons, r.comparison||'Equal', 'data-k="comparison"')+
            '<input class="fld edCondV" data-k="value" type="number" step="any" value="'+escHtml(val==null? 0 : val)+'">'+
            '<button class="btn dim ic" data-del="'+i+'" title="Take this condition out">&#x2715;</button></div>';
        }).join('')+
        '<div class="orirow"><span class="v"><button class="btn sm" id="edCondAdd"'+(rows.length>=6? ' disabled title="A response has six conditions at most"' : '')+'>Add a condition</button> '+
        '<button class="btn sm pri" id="edCondSave" title="The conditions to the patch pool">Save conditions</button> '+
        (this.has('editWhoCanSay')? '<button class="btn sm" id="edWho" title="The load order\'s NPCs this response\'s speaker and id conditions let say it">Who can say this</button>' : '')+
        '</span></div><div id="edWhoOut"></div>';
      box.querySelectorAll('.edCond').forEach(row=>{
        const r=rows[+row.dataset.i];
        row.querySelectorAll('[data-k]').forEach(el=>{
          el.onkeydown=e=>e.stopPropagation();
          el.onchange=()=>{
            const k=el.dataset.k;
            if(k==='value'){ const n=Number(el.value); r.value={type:Number.isInteger(n)? 'Integer' : 'Float', data:n}; }
            else r[k]=el.value;
            if(k==='filter_type') draw();
          };
          if(el.dataset.k==='id'){
            const tags=this.FILTER_TAGS[r.filter_type];
            if(tags) el.addEventListener('focus', ()=>E.offerIds(el, tags));
            el.dataset.ids=(tags||[]).join(' ');
          }
        });
      });
      box.querySelectorAll('[data-del]').forEach(b=>b.onclick=()=>{ rows.splice(+b.dataset.del, 1); draw(); });
      box.querySelector('#edCondAdd').onclick=()=>{
        rows.push({index:rows.length, filter_type:'Function', function:C.functions[0], comparison:'Equal', id:'', value:{type:'Integer', data:0}});
        draw();
      };
      box.querySelector('#edCondSave').onclick=()=>{
        rows.forEach((r,i)=>{ r.index=i; if(r.filter_type==='Function'){ r.id=r.id||''; } });
        E.change('editSet', {path:'filters', value:rows});
      };
      const who=box.querySelector('#edWho');
      if(who) who.onclick=()=>this.whoCanSay(box.querySelector('#edWhoOut'), v.id);
    };
    draw();
  },

  async whoCanSay(out, id){
    out.innerHTML='<div class="hint">Reading the load order\'s NPCs…</div>';
    let r;
    try{ r=await this.ask('editWhoCanSay', {id}); }
    catch(e){ out.innerHTML='<div class="hint edBad">'+escHtml(this.err(e))+'</div>'; return null; }
    out.innerHTML='<div class="hint">'+r.count+' NPC'+(r.count===1?'':'s')+(r.capped? ' (the first '+r.npcs.length+' shown)' : '')+
      (r.unchecked.length? ' - not checked here, as they depend on the game: '+escHtml(r.unchecked.join(', ')) : '')+'</div>'+
      '<div class="edWho">'+r.npcs.map(n=>'<a href="#" data-npc="'+escHtml(n.id)+'" title="Open '+escHtml(n.id)+'">'+escHtml(n.name||n.id)+'</a>').join(', ')+'</div>';
    out.querySelectorAll('[data-npc]').forEach(a=>a.onclick=e=>{ e.preventDefault(); WgEditor.showEntry('NPC_', a.dataset.npc); });
    return r;
  },

  rollBox(E, body, v){
    const box=document.createElement('div');
    box.className='orirow edRoll';
    box.innerHTML='<span class="k" title="What the list gives a player of this level - lists inside it rolled too">Roll at level</span><span class="v">'+
      '<input class="fld" id="edRollLv" type="number" min="1" step="1" value="'+(this._rollLv||1)+'" style="width:60px"> '+
      '<button class="btn sm" id="edRollGo">Roll</button></span><div id="edRollOut"></div>';
    body.insertBefore(box, body.querySelector('details.edAllFields')||body.querySelector('.edSecBar')||null);
    const go=async()=>{
      const level=Math.max(1, Math.floor(+box.querySelector('#edRollLv').value||1));
      this._rollLv=level;
      const out=box.querySelector('#edRollOut');
      let r;
      try{ r=await this.ask('editLeveledRoll', {tag:v.tag, id:v.id, level}); }
      catch(e){ out.innerHTML='<div class="hint edBad">'+escHtml(this.err(e))+'</div>'; return; }
      out.innerHTML='<table class="edT edRollT"><tbody>'+r.outcomes.map(o=>'<tr><td>'+(o.id? escHtml(o.id) : '<i>nothing</i>')+'</td><td class="num">'+(o.chance*100).toFixed(o.chance<0.01? 2 : 1)+'%</td></tr>').join('')+'</tbody></table>'+
        (r.nested.length? '<div class="from">Lists inside rolled too: '+escHtml(r.nested.join(', '))+'</div>' : '');
    };
    box.querySelector('#edRollGo').onclick=go;
    box.querySelector('#edRollLv').onkeydown=e=>{ e.stopPropagation(); if(e.key==='Enter') go(); };
  },

  /* ---- Script Edit ----------------------------------------------------------------- */

  /** Completion, go to what a word names, and every script naming it (openScript calls it). */
  extendScript(E, d){
    if(d._flow) return;
    d._flow=true;
    const ta=d.querySelector('#edScriptText');
    const bar=d.querySelector('#edScriptSave').parentNode;
    if(this.has('editScriptRefs')){
      const b=document.createElement('button');
      b.className='btn sm'; b.textContent='Scripts naming the word';
      b.title='Every script whose code names the word at the cursor - a variable, an id';
      b.onclick=()=>this.scriptRefs(d, this.wordAt(ta));
      bar.appendChild(document.createTextNode(' ')); bar.appendChild(b);
    }
    const pop=document.createElement('div'); pop.className='edComplete'; pop.hidden=true;
    d.querySelector('#edScriptSrc').appendChild(pop);
    let items=[], pick=0;
    const close=()=>{ pop.hidden=true; items=[]; };
    const accept=()=>{
      const w=items[pick]; if(!w) return close();
      const [a, b]=this.wordSpan(ta);
      ta.value=ta.value.slice(0, a)+w+ta.value.slice(b);
      ta.selectionStart=ta.selectionEnd=a+w.length;
      close(); ta.oninput && ta.oninput();
    };
    const show=async force=>{
      const [a, b]=this.wordSpan(ta), word=ta.value.slice(a, b);
      if(!force && word.length<2){ close(); return; }
      const words=await this.scriptWordList(E);
      const low=word.toLowerCase();
      items=words.filter(w=>w.toLowerCase().startsWith(low) && w.toLowerCase()!==low).slice(0, 40);
      if(!items.length){ close(); return; }
      pick=0;
      pop.innerHTML=items.map((w,i)=>'<div class="it'+(i===0?' on':'')+'" data-i="'+i+'">'+escHtml(w)+'</div>').join('');
      pop.querySelectorAll('[data-i]').forEach(el=>el.onmousedown=e=>{ e.preventDefault(); pick=+el.dataset.i; accept(); });
      pop.hidden=false;
    };
    ta.addEventListener('keydown', e=>{
      if(!pop.hidden){
        if(e.key==='ArrowDown' || e.key==='ArrowUp'){
          e.preventDefault(); pick=(pick+(e.key==='ArrowDown'? 1 : items.length-1))%items.length;
          pop.querySelectorAll('[data-i]').forEach(el=>el.classList.toggle('on', +el.dataset.i===pick)); return;
        }
        if(e.key==='Enter' || e.key==='Tab'){ e.preventDefault(); e.stopImmediatePropagation(); accept(); return; }
        if(e.key==='Escape'){ e.preventDefault(); close(); return; }
      }
      if(e.key===' ' && (e.ctrlKey||e.metaKey)){ e.preventDefault(); show(true); }
    }, true);
    ta.addEventListener('input', ()=>{ clearTimeout(this._cT); this._cT=setTimeout(()=>show(false), 150); });
    ta.addEventListener('blur', ()=>setTimeout(close, 150));
    // Ctrl+click: open what the word names.
    ta.addEventListener('click', e=>{ if(e.ctrlKey||e.metaKey) this.goTo(this.wordAt(ta)); });
  },

  async scriptWordList(E){
    if(!this._words){
      let w={functions:[], keywords:[], globals:[]};
      try{ if(this.has('editScriptWords')) w=await this.ask('editScriptWords', {}); }catch(_){ }
      this._words=[...w.keywords, ...w.functions, ...w.globals];
      // The load order's ids: scripts, NPCs, creatures, items - what a script names most.
      for(const t of ['SCPT','NPC_','CREA','MISC','WEAP','ARMO','CLOT','BOOK','ALCH','INGR','CONT','DOOR','ACTI','LIGH','SPEL','DIAL','CELL']){
        try{ this._words.push(...await E.idsOf(t)); }catch(_){ }
      }
      this._words=[...new Set(this._words)].sort((a,b)=>a.localeCompare(b));
    }
    return this._words;
  },
  /** The word at the cursor: `[start, end]`, and its text. */
  wordSpan(ta){
    const t=ta.value, at=ta.selectionStart;
    let a=at, b=at;
    while(a>0 && /[\w]/.test(t[a-1])) a--;
    while(b<t.length && /[\w]/.test(t[b])) b++;
    return [a, b];
  },
  wordAt(ta){ const [a, b]=this.wordSpan(ta); return ta.value.slice(a, b); },

  /** What a word names, opened: a script in Script Edit, anything else in its dialog. */
  async goTo(word){
    if(!word) return null;
    const E=WgEditor;
    const scripts=await E.idsOf('SCPT');
    const s=scripts.find(x=>x.toLowerCase()===word.toLowerCase());
    if(s) return E.openScript('SCPT', s, null);
    return E.showEntry('', word);
  },

  async scriptRefs(d, word){
    const out=d.querySelector('#edScriptFind');
    if(!word) return toast('Put the cursor on a word','warn',2000);
    let r;
    try{ r=await this.ask('editScriptRefs', {word}); }
    catch(e){ toast(this.err(e),'err',4000); return null; }
    out.innerHTML='<div class="hint">'+escHtml(word)+': '+r.hits.length+' line'+(r.hits.length===1?'':'s')+' in '+r.scripts+' script'+(r.scripts===1?'':'s')+(r.capped? ' (the first shown)' : '')+'</div>'+
      '<div class="edRefs">'+r.hits.map(h=>'<div class="it" data-s="'+escHtml(h.script)+'" title="Open '+escHtml(h.script)+'"><b>'+escHtml(h.script)+'</b>:'+h.line+' <span class="mono">'+escHtml(h.text)+'</span></div>').join('')+'</div>';
    out.querySelectorAll('[data-s]').forEach(el=>el.onclick=()=>WgEditor.openScript('SCPT', el.dataset.s, null));
    return r;
  },

  /* ---- search everything ------------------------------------------------------------- */

  showSearch(){
    if(!this.srEl){
      const d=document.createElement('div');
      d.id='edSearchAll'; d.className='ori'; d.hidden=true;
      d.innerHTML='<div class="orihead"><b>Search everything</b><span style="flex:1"></span><button class="btn dim ic" id="edSaX" title="Close">&#x2715;</button></div>'+
        '<div class="oribody"><div class="orirow"><span class="v"><input class="fld" id="edSaText" placeholder="Text, in any field of any record" spellcheck="false" style="width:220px"> '+
        '<label class="from"><input type="checkbox" id="edSaWhole"> Whole word</label> <button class="btn sm pri" id="edSaGo">Search</button></span></div>'+
        '<div class="orirow"><span class="v"><input class="fld" id="edSaBy" placeholder="Replace with" spellcheck="false" style="width:220px"> '+
        '<button class="btn sm" id="edSaRep" title="The text replaced in the checked fields, as one change (Ctrl+Z puts it back). Ids are left: Search & Replace repoints them">Replace in the checked</button></span></div>'+
        '<div id="edSaOut"></div></div>';
      WgUI.mount(d);
      d.querySelector('#edSaX').onclick=()=>{ d.hidden=true; };
      const go=()=>this.runSearch();
      d.querySelector('#edSaGo').onclick=go;
      d.querySelector('#edSaText').onkeydown=e=>{ e.stopPropagation(); if(e.key==='Enter') go(); };
      d.querySelector('#edSaBy').onkeydown=e=>e.stopPropagation();
      d.querySelector('#edSaRep').onclick=()=>this.runReplace();
      this.srEl=d;
    }
    this.srEl.hidden=false;
    setTimeout(()=>{ try{ this.srEl.querySelector('#edSaText').focus(); }catch(_){ } }, 0);
    return this.srEl;
  },
  async runSearch(){
    const d=this.srEl, text=d.querySelector('#edSaText').value, whole=d.querySelector('#edSaWhole').checked;
    const out=d.querySelector('#edSaOut');
    if(!text.trim()) return;
    out.innerHTML='<div class="hint">Searching every record…</div>';
    try{ this._sa=await this.ask('editSearchAll', {text, whole}); }
    catch(e){ out.innerHTML='<div class="hint edBad">'+escHtml(this.err(e))+'</div>'; return null; }
    this._saText=text; this._saWhole=whole;
    const r=this._sa;
    out.innerHTML='<div class="hint">'+r.count+' field'+(r.count===1?'':'s')+(r.capped? ' (the first '+r.hits.length+' shown)' : '')+'</div>'+
      '<table class="edT edSaT"><thead><tr><th><input type="checkbox" id="edSaAll" checked title="All of them"></th><th>Type</th><th>Record</th><th>Field</th><th>Text</th></tr></thead><tbody>'+
      r.hits.map((h,i)=>'<tr data-i="'+i+'"><td><input type="checkbox" data-c="'+i+'" checked></td><td>'+escHtml(WgEditor.NAMES[h.tag]||h.tag)+'</td><td>'+escHtml(h.id)+'</td><td>'+escHtml(h.path)+'</td><td>'+escHtml(h.value.length>120? h.value.slice(0,120)+'…' : h.value)+'</td></tr>').join('')+
      '</tbody></table>';
    out.querySelector('#edSaAll').onchange=e=>out.querySelectorAll('[data-c]').forEach(c=>{ c.checked=e.target.checked; });
    out.querySelectorAll('tr[data-i]').forEach(tr=>tr.ondblclick=()=>{ const h=r.hits[+tr.dataset.i]; WgEditor.showEntry(h.tag, h.id); });
    return r;
  },
  async runReplace(){
    const d=this.srEl, r=this._sa;
    if(!r) return toast('Search first','warn',2000);
    const by=d.querySelector('#edSaBy').value;
    const hits=[...d.querySelectorAll('[data-c]')].filter(c=>c.checked).map(c=>r.hits[+c.dataset.c]);
    if(!hits.length) return toast('Check the fields to change','warn',2000);
    let out;
    try{ out=await this.ask('editReplaceAll', {hits, text:this._saText, by, whole:!!this._saWhole}); }
    catch(e){ toast(this.err(e),'err',5000); return null; }
    const left=out.failed.length;
    toast('Replaced in '+out.changed+' field'+(out.changed===1?'':'s')+(left? ' - '+left+' left (see Messages)' : ''), left? 'warn' : 'ok', 4000);
    for(const f of out.failed) if(typeof WgLog==='object') WgLog.add(f.tag+' '+f.id+' '+f.path+': '+f.error, 'warn');
    await WgEditor.refreshPending();
    await this.runSearch();
    return out;
  },
};
