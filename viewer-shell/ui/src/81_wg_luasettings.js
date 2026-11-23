/* =====================================================================================
   Wraithguard: Lua settings - the Tools page's view of the settings OpenMW keeps for Lua mods
   (Static 2026-11-23: "a Lua mod settings editor"). OpenMW keeps them in player_storage.bin
   and global_storage.bin beside openmw.cfg (`lua_settings_read`); each section's values are
   shown, and one can be changed (`lua_settings_write`): only that value, the file written only
   when it reads back as exactly the settings meant, and the untouched original kept once as
   <file>.wg-backup. OpenMW writes the file again when it quits, so it is changed with OpenMW
   closed. True/false, numbers, text, vectors and colours are edited; a list, a table or a
   value the page cannot show (a transform, a game object) is shown and kept as it is.
   Built with the DOM (a mod's setting names and values go into the page as text).
   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
const WgLuaSettings={
  data:null, which:'player', q:'', open:new Set(),

  pane(){
    const col=document.createElement('div');
    col.className='col pane'; col.id='tlLuaSet'; col.hidden=true;
    const h=document.createElement('h3');
    h.append('Lua settings ');
    const sp=document.createElement('span'); sp.className='sp'; h.append(sp);
    const sel=document.createElement('select'); sel.id='tlLsWhich';
    for(const [v, t] of [['player', 'player_storage.bin'], ['global', 'global_storage.bin']]){ const o=document.createElement('option'); o.value=v; o.textContent=t; sel.append(o); }
    const q=document.createElement('input'); q.id='tlLsQ'; q.placeholder='Filter sections and settings'; q.style.width='220px';
    const re=document.createElement('button'); re.className='btn'; re.id='tlLsLoad'; re.textContent='Read';
    h.append(sel, q, re);
    const note=document.createElement('div'); note.className='chips'; note.id='tlLsNote';
    note.textContent='The settings OpenMW keeps for Lua mods (beside openmw.cfg). Close OpenMW before changing one: it writes this file again when it quits.';
    const body=document.createElement('div'); body.className='scroll'; body.id='tlLsBody';
    col.append(h, note, body);
    return col;
  },
  wire(){
    const sel=$('#tlLsWhich'), q=$('#tlLsQ'), re=$('#tlLsLoad');
    if(sel) sel.onchange=()=>{ this.which=sel.value; this.draw(); };
    if(q) q.oninput=()=>{ this.q=q.value.toLowerCase(); this.draw(); };
    if(re) re.onclick=()=>this.load();
  },
  async load(){
    const body=$('#tlLsBody'); if(!body) return;
    body.textContent='';
    try{ this.data=await Engine.call('lua_settings_read', {}); }
    catch(e){ this.data=null; const d=document.createElement('div'); d.className='empty'; d.textContent=String(e && e.message || e); body.append(d); return; }
    this.draw();
  },

  /** What a value is, for its editor. */
  kind(v){
    if(typeof v==='boolean') return 'bool';
    if(typeof v==='number') return 'num';
    if(typeof v==='string') return 'str';
    if(v && typeof v==='object' && !Array.isArray(v)){
      for(const k of ['$v2', '$v3', '$v4', '$color']) if(Array.isArray(v[k])) return k;
      if(v.$raw) return 'raw';
    }
    return 'table';
  },
  /** A value shown short, as text. */
  show(v){
    const k=this.kind(v);
    if(k==='raw') return '(a value kept as it is)';
    if(k[0]==='$') return k.slice(1)+'('+v[k].map(x=>+(+x).toFixed(4)).join(', ')+')';
    const s=JSON.stringify(v);
    return s.length>160? s.slice(0, 157)+'…' : s;
  },

  draw(){
    const body=$('#tlLsBody'); if(!body) return;
    body.textContent='';
    const f=this.data && this.data[this.which];
    const empty=t=>{ const d=document.createElement('div'); d.className='empty'; d.textContent=t; body.append(d); };
    if(!this.data){ empty('Read to see the settings OpenMW keeps for your Lua mods.'); return; }
    if(!f || !f.sections){ empty('There is no '+(this.which==='player'? 'player_storage.bin' : 'global_storage.bin')+' beside openmw.cfg yet (OpenMW writes it the first time it quits after a Lua mod saved a setting).'); return; }
    const where=document.createElement('div'); where.className='empty';
    where.textContent=f.path+(f.backup? ' · the original is kept as '+f.path+'.wg-backup' : '');
    body.append(where);
    const names=Object.keys(f.sections).sort((a, b)=>a.toLowerCase().localeCompare(b.toLowerCase()));
    let shown=0;
    for(const sec of names){
      const vals=f.sections[sec]||{};
      const keys=Object.keys(vals).sort();
      const q=this.q, secHit=!q || sec.toLowerCase().includes(q);
      const ks=secHit? keys : keys.filter(k=>k.toLowerCase().includes(q));
      if(!ks.length) continue;
      shown++;
      const w=document.createElement('div'); w.className='warn'; w.style.borderLeftColor='var(--ac)';
      const head=document.createElement('div'); head.style.cursor='pointer';
      const isOpen=this.open.has(sec) || (q && !secHit);
      const b=document.createElement('b'); b.textContent=(isOpen? '▾ ' : '▸ ')+sec;
      const n=document.createElement('span'); n.className='kind'; n.style.display='inline'; n.textContent='  '+keys.length+' setting'+(keys.length===1? '' : 's');
      head.append(b, n);
      head.onclick=()=>{ if(this.open.has(sec)) this.open.delete(sec); else this.open.add(sec); this.draw(); };
      w.append(head);
      if(isOpen) for(const k of ks) w.append(this.row(sec, k, vals[k]));
      body.append(w);
    }
    if(!shown) empty(this.q? 'Nothing matches the filter.' : 'The file holds no settings.');
  },

  /** One setting: its name, an editor for its kind, and Save once it is changed. */
  row(sec, key, v){
    const r=document.createElement('div');
    r.style.cssText='display:flex;gap:6px;align-items:center;margin:3px 0 0 14px;font-family:var(--mono);font-size:11px';
    const name=document.createElement('span'); name.textContent=key; name.style.minWidth='180px';
    r.append(name);
    const k=this.kind(v);
    let get=null;
    const save=document.createElement('button'); save.className='btn sm'; save.textContent='Save'; save.disabled=true;
    const changed=()=>{ save.disabled=false; };
    if(k==='bool'){ const c=document.createElement('input'); c.type='checkbox'; c.checked=v; c.onchange=changed; r.append(c); get=()=>c.checked; }
    else if(k==='num'){ const i=document.createElement('input'); i.type='number'; i.step='any'; i.value=String(v); i.style.width='120px'; i.oninput=changed; r.append(i); get=()=>{ const x=Number(i.value); if(!isFinite(x)) throw new Error('not a number'); return x; }; }
    else if(k==='str'){ const i=document.createElement('input'); i.value=v; i.style.flex='1'; i.oninput=changed; r.append(i); get=()=>i.value; }
    else if(k[0]==='$' && k!=='$raw'){
      const ins=v[k].map(x=>{ const i=document.createElement('input'); i.type='number'; i.step='any'; i.value=String(x); i.style.width='70px'; i.oninput=changed; r.append(i); return i; });
      get=()=>{ const a=ins.map(i=>Number(i.value)); if(a.some(x=>!isFinite(x))) throw new Error('not a number'); return {[k]:a}; };
    }
    else{ const s=document.createElement('span'); s.className='hint'; s.textContent=this.show(v); s.title='Shown and kept as it is: a list, a table or a value the page cannot edit'; r.append(s); }
    if(get){
      save.onclick=async()=>{
        let value;
        try{ value=get(); }catch(e){ toast(String(e.message||e)); return; }
        const f=this.data[this.which];
        const ok=await wgConfirm('Change '+key+' in '+sec+'?\n\nIn '+f.path+'. Close OpenMW first: it writes this file again when it quits.'+
          (f.backup? '' : '\n\nThe file as it is now is kept as '+f.path+'.wg-backup.'));
        if(!ok) return;
        try{
          await Engine.call('lua_settings_write', {which:this.which, section:sec, key, value});
          toast('Saved '+key);
          await this.load();
        }catch(e){ toast('Not saved: '+String(e && e.message || e)); }
      };
      r.append(save);
    }
    return r;
  },
};
