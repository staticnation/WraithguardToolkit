/* =====================================================================================
   Wraithguard: "Check my Lua mods" - the Tools page's Lua mods section (Static 2026-11-23).

   Each Lua mod of the load order run on its own by the engine (`lua_check`,
   viewcore/src/luacheck.rs) and what its scripts said sorted into what can be done about it:
   a dependency missing (a module, an interface, or the mod's own "X is not installed"), a
   plugin not loaded (a global it reads), the mod's own fault (its Lua does not parse, a
   script ran away), and other errors; then what the Preview does not run yet, ranked by how
   many mods reach for it - the Preview's own to-do list. Above them the load-order lint:
   interfaces used that no script provides, settings groups registered twice, a mod in two
   data folders. Built with the DOM (no markup from the mods goes into the page as HTML).
   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
const WgLuaCheck={
  data:null, filter:'problems', busy:false,
  CLASS:{dependency:'needs another mod', plugin:'a plugin is not loaded', mod:'the mod\'s own fault', runtime:'error', failed:'did not run', ok:'ok'},
  FILTERS:[['problems','With problems'], ['dependency','Dependencies'], ['plugin','Plugins'], ['mod','Mod faults'], ['runtime','Errors'], ['ok','OK'], ['all','All']],

  /** The section's pane (61_wg_tools.js puts it beside the others). */
  pane(){
    const col=document.createElement('div');
    col.className='col pane'; col.id='tlLua'; col.hidden=true;
    const h=document.createElement('h3');
    h.append('Lua mods ');
    const sp=document.createElement('span'); sp.className='sp'; h.append(sp);
    const st=document.createElement('span'); st.id='tlLuaStats'; st.className='empty'; h.append(st);
    const copy=document.createElement('button'); copy.className='btn dim'; copy.id='tlLuaCopy'; copy.title='The findings as text'; copy.textContent='Copy';
    const run=document.createElement('button'); run.className='btn'; run.id='tlLuaRun'; run.textContent='Run';
    run.title='Run each Lua mod of the load order on its own, on a small scene: a minute or two for a big load order';
    h.append(copy, run);
    const chips=document.createElement('div'); chips.className='chips'; chips.id='tlLuaChips';
    const body=document.createElement('div'); body.className='scroll'; body.id='tlLuaBody';
    const e=document.createElement('div'); e.className='empty';
    e.textContent='Run to start each Lua mod of the load order on its own and see what each needs that is missing.';
    body.append(e);
    col.append(h, chips, body);
    return col;
  },
  wire(){
    const run=$('#tlLuaRun'), copy=$('#tlLuaCopy');
    if(run) run.onclick=()=>this.run();
    if(copy) copy.onclick=()=>{ try{ navigator.clipboard.writeText(this.text()); }catch(_){ } };
  },

  async run(){
    if(this.busy) return;
    const btn=$('#tlLuaRun'), body=$('#tlLuaBody');
    this.busy=true; if(btn) btn.disabled=true;
    body.textContent='';
    const w=document.createElement('div'); w.className='empty';
    w.textContent='Running each Lua mod on its own…';
    body.append(w);
    let dir=''; try{ dir=localStorage.getItem('wgOpenMWDir')||''; }catch(_){ }
    try{ this.data=await Engine.call('lua_check', {openmw:dir||null}); }
    catch(e){ this.data=null; w.textContent='The check did not run: '+String(e && e.message || e); }
    this.busy=false; if(btn) btn.disabled=false;
    if(this.data) this.draw();
  },

  counts(){
    const c={};
    for(const m of (this.data && this.data.mods)||[]) c[m.status]=(c[m.status]||0)+1;
    return c;
  },
  shown(m){
    const f=this.filter;
    if(f==='all') return true;
    if(f==='problems') return m.status!=='ok';
    return m.status===f || (m.found||[]).some(x=>x.class===f);
  },

  draw(){
    const d=this.data, body=$('#tlLuaBody'), chips=$('#tlLuaChips'), st=$('#tlLuaStats');
    if(!d || !body) return;
    const c=this.counts(), n=(d.mods||[]).length;
    if(st) st.textContent=n+' mod'+(n===1? '' : 's')+' · '+(c.ok||0)+' ok'+(d.resources? '' : ' · OpenMW\'s resources not found: its built-ins did not run');
    chips.textContent='';
    for(const [k, label] of this.FILTERS){
      const ch=document.createElement('span');
      ch.className='chip'+(this.filter===k? ' on' : '');
      const num=k==='all'? n : k==='problems'? n-(c.ok||0) : (c[k]||0);
      ch.textContent=label+' '+num;
      ch.onclick=()=>{ this.filter=k; this.draw(); };
      chips.append(ch);
    }
    body.textContent='';
    const section=(title)=>{ const h=document.createElement('div'); h.style.cssText='font-weight:700;margin:8px 0 4px'; h.textContent=title; body.append(h); };
    // The load-order lint.
    if((d.lint||[]).length){
      section('The load order');
      for(const l of d.lint){
        const w=document.createElement('div'); w.className='warn';
        const k=document.createElement('span'); k.className='kind';
        k.textContent=l.kind==='interface'? 'interface nobody provides' : l.kind==='settings'? 'settings registered twice' : 'a mod in two data folders';
        w.append(k, l.text);
        const f=document.createElement('div'); f.className='kind'; f.textContent=(l.files||[]).join(' · ');
        w.append(f);
        body.append(w);
      }
    }
    // Each mod.
    const mods=(d.mods||[]).filter(m=>this.shown(m));
    section('Mods'+(this.filter==='all'? '' : ' ('+mods.length+')'));
    if(!mods.length){ const e=document.createElement('div'); e.className='empty'; e.textContent='None.'; body.append(e); }
    for(const m of mods){
      const w=document.createElement('div'); w.className='warn';
      w.style.borderLeftColor=m.status==='ok'? 'var(--grow)' : (m.status==='mod' || m.status==='failed')? 'var(--err)' : 'var(--warn)';
      const k=document.createElement('span'); k.className='kind';
      k.textContent=(m.kind==='mwse'? 'MWSE · ' : '')+(this.CLASS[m.status]||m.status)+' · '+(m.scripts||[]).length+' script'+((m.scripts||[]).length===1? '' : 's');
      const name=document.createElement('b'); name.textContent=m.name;
      w.append(k, name);
      if(m.failed){ const f=document.createElement('div'); f.textContent='did not run: '+m.failed; w.append(f); }
      for(const x of m.found||[]){
        const f=document.createElement('div');
        f.style.marginTop='3px';
        const cl=document.createElement('span'); cl.className='kind'; cl.style.display='inline';
        cl.textContent=(this.CLASS[x.class]||x.class)+(x.about? ': '+x.about : '')+' — ';
        f.append(cl, x.text);
        w.append(f);
      }
      body.append(w);
    }
    // What the Preview does not run yet.
    if((d.gaps||[]).length){
      section('What the Preview does not run yet, by how many mods use it');
      const t=document.createElement('table'); t.className='mo';
      const hr=document.createElement('tr');
      for(const [txt, cls] of [['mods', ''], ['call', 'p'], ['for example', 'p']]){ const th=document.createElement('th'); th.textContent=txt; if(cls) th.className=cls; hr.append(th); }
      t.append(hr);
      for(const g of d.gaps.slice(0, 200)){
        const tr=document.createElement('tr');
        const a=document.createElement('td'); a.textContent=String(g.mods);
        const b=document.createElement('td'); b.className='p'; b.textContent=g.name;
        const e=document.createElement('td'); e.className='p'; e.textContent=(g.examples||[]).join(', ');
        tr.append(a, b, e); t.append(tr);
      }
      body.append(t);
    }
  },

  /** The findings as text, for a report. */
  text(){
    const d=this.data; if(!d) return '';
    const out=[];
    for(const l of d.lint||[]) out.push('[load order] '+l.text+' ('+(l.files||[]).join(', ')+')');
    for(const m of d.mods||[]){
      if(m.status==='ok') continue;
      out.push('['+(this.CLASS[m.status]||m.status)+'] '+m.name+(m.failed? ': did not run: '+m.failed : ''));
      for(const x of m.found||[]) out.push('    '+(this.CLASS[x.class]||x.class)+(x.about? ' ('+x.about+')' : '')+': '+x.text);
    }
    if((d.gaps||[]).length){ out.push('', 'Not run in the Preview yet:'); for(const g of d.gaps) out.push('  '+g.mods+'  '+g.name); }
    return out.join('\n');
  },
};
