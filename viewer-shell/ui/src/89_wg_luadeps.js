/* =====================================================================================
   Wraithguard: the Lua mods check's missing dependencies, found and turned on (Static
   2026-11-24, "lets do all this": "Lua mods one-click turn-on of missing dependencies, plus
   links for missing mods").

   After a run, what the mods need and the load order does not have (an interface, a module, a
   global a plugin defines) is gathered in one list at the top, with the mods that need each.
   Each is looked for in the setup (`lua_find`, viewcore/src/luafind.rs): a content file in the
   data folders that is not turned on, or a folder of the Load order page's mods folder not in
   data= yet. **Turn on** adds it on the Load order page as a drop would (content=, and data=
   for a folder), for Export to write; **Turn on all** does every one found. What is not
   installed gets **Search Nexus** (the mod pages, in the person's browser, `open_web`).
   Built with the DOM. Copyright (c) 2026 StaticNation.
   ===================================================================================== */
const WgLuaDeps={
  found:null, _for:null, busy:false,

  /** What the run's findings need: [{kind, name, mods:[...]}], one each. */
  needs(data){
    const by=new Map();
    const add=(kind, name, mod)=>{
      name=String(name||'').trim().replace(/^['"]|['"]$/g, '');
      if(!name) return;
      const k=kind+'|'+name.toLowerCase();
      if(!by.has(k)) by.set(k, {kind, name, mods:[]});
      const e=by.get(k); if(!e.mods.includes(mod)) e.mods.push(mod);
    };
    for(const m of (data && data.mods)||[]) for(const x of m.found||[]){
      const a=String(x.about||''), t=String(x.text||'');
      if(x.class==='plugin'){ const g=a.match(/^global\s+(\S+)/); if(g) add('global', g[1], m.name); continue; }
      if(x.class!=='dependency') continue;
      let r=a.match(/^interface\s+(\w+)/);
      if(r){ add('interface', r[1], m.name); continue; }
      if(/^module\b/.test(a)){
        r=a.match(/^module\s+['"]?([\w./-]+)/) || t.match(/module\s+['"]([^'"]+)['"]/) || t.match(/^(\S+) is not run in the preview/);
        if(r) add('module', r[1], m.name);
      }
    }
    return [...by.values()];
  },
  /** The mods folder's folders (the Load order page's, scanned), not yet in data=. */
  async extraDirs(){
    let dir='';
    try{ if(typeof WgLoadOrder==='object') dir=($('#lo_scanDir') && WgLoadOrder.val('scanDir')) || (WgLoadOrder.saved()||{}).scanDir || ''; }catch(_){ }
    if(!dir) return [];
    try{
      const scan=loReadScan(unpackTables(await Engine.bytes('loadorder_scan', {dir})));
      return scan.lines.filter(l=>/[\\/]/.test(l) && !/\.(esp|esm|omwaddon|omwscripts|omwgame)$/i.test(l)).map(l=>l.replace(/^data\s*=\s*/i, '').replace(/^"|"$/g, ''));
    }catch(_){ return []; }
  },
  async find(data){
    const needs=this.needs(data);
    this._for=data; this.found=null;
    if(!needs.length){ this.found=[]; return; }
    this.busy=true;
    let got=[];
    try{
      got=await Engine.call('lua_find', {needs:needs.map(n=>[n.kind, n.name]), extra:await this.extraDirs()});
      if(typeof got==='string') got=JSON.parse(got);
    }catch(e){ got=[]; toast('Looking for what is missing: '+String(e.message||e), 'warn', 5000); }
    this.busy=false;
    this.found=needs.map((n, i)=>Object.assign({}, n, {found:((got||[])[i]||{}).found||[]}));
  },
  label(n){ return n.kind==='interface'? 'I.'+n.name : n.kind==='global'? 'the global '+n.name : 'the module '+n.name; },
  /** A word to search for a missing one: the interface's name, a module's last part, the global. */
  word(n){ const s=n.kind==='module'? String(n.name).split(/[./]/).filter(Boolean).pop() : n.name; return String(s||'').replace(/([a-z])([A-Z])/g, '$1 $2').replace(/_/g, ' '); },
  nexus(q){ return 'https://www.nexusmods.com/games/morrowind/mods?keyword='+encodeURIComponent(q); },

  /** Turned on: each fix added on the Load order page, as dropping its file there would. */
  async turnOn(fixes){
    const L=typeof WgLoadOrder==='object'? WgLoadOrder : null;
    if(!L){ toast('The Load order page is not here', 'err', 4000); return; }
    if(!L.on) L.enter();
    for(let i=0; i<480 && !L.plan; i++) await new Promise(r=>setTimeout(r, 250));
    if(!L.plan){ toast('Sort on the Load order page first, then turn them on', 'warn', 6000); return; }
    for(const f of fixes){
      if(f.content) await L.dropDo([f.path], {kind:'content', index:-1, after:false});
      else if(f.data) await L.dropDo([f.data], {kind:'data', index:-1, after:false});
    }
  },

  /** The list at the top of the check's findings. */
  draw(){
    const body=$('#tlLuaBody'); if(!body) return;
    const old=document.getElementById('tlLuaDeps'); if(old) old.remove();
    const F=this.found;
    if(!F || !F.length) return;
    const box=document.createElement('div'); box.id='tlLuaDeps';
    const h=document.createElement('div'); h.style.cssText='font-weight:700;margin:8px 0 4px;display:flex;gap:6px;align-items:center';
    const all=F.flatMap(n=>n.found.slice(0, 1));
    h.append('What the mods need ('+F.length+')');
    const sp=document.createElement('span'); sp.style.flex='1'; h.append(sp);
    if(all.length){
      const b=document.createElement('button'); b.className='btn'; b.textContent='Turn on all '+all.length+' found';
      b.title='Add each one found to the Load order page (Export writes it)';
      b.onclick=()=>this.turnOn(all);
      h.append(b);
    }
    box.append(h);
    for(const n of F){
      const w=document.createElement('div'); w.className='warn';
      w.style.borderLeftColor=n.found.length? 'var(--ok,#5a5)' : 'var(--warn)';
      const k=document.createElement('span'); k.className='kind';
      k.textContent='needed by '+n.mods.slice(0, 4).join(', ')+(n.mods.length>4? ' and '+(n.mods.length-4)+' more' : '');
      const name=document.createElement('b'); name.textContent=this.label(n);
      w.append(k, name);
      if(n.found.length){
        for(const f of n.found){
          const r=document.createElement('div'); r.style.cssText='display:flex;gap:6px;align-items:center;margin-top:3px';
          const t=document.createElement('span'); t.style.cssText='flex:1;min-width:0';
          t.textContent=(f.content? f.content+(f.data? ' in '+f.data+' (not in data= yet)' : ' is installed and not turned on') : f.data+' is not in data= yet')+': '+f.via;
          const b=document.createElement('button'); b.className='btn sm'; b.textContent='Turn on';
          b.title='Add it on the Load order page: '+(f.content? 'content='+f.content : '')+(f.data? (f.content? ' and ' : '')+'data='+f.data : '')+' (Export writes it)';
          b.onclick=()=>this.turnOn([f]);
          r.append(t, b); w.append(r);
        }
      }else{
        const r=document.createElement('div'); r.style.cssText='display:flex;gap:6px;align-items:center;margin-top:3px';
        const t=document.createElement('span'); t.style.cssText='flex:1;min-width:0';
        t.textContent='Not in your setup: the mod that provides it is not installed (or its folder is outside data= and the mods folder).';
        const q=this.word(n);
        const b=document.createElement('button'); b.className='btn sm'; b.textContent='Search Nexus';
        b.title='Search Nexus Mods\' Morrowind mods for "'+q+'", in your browser';
        b.onclick=()=>Engine.call('open_web', {url:this.nexus(q)}).catch(e=>toast(String(e.message||e), 'err', 4000));
        r.append(t, b); w.append(r);
      }
      box.append(w);
    }
    body.insertBefore(box, body.firstChild);
  },

  bind(){
    if(typeof WgLuaCheck!=='object' || WgLuaCheck._deps) return;
    WgLuaCheck._deps=true;
    const draw=WgLuaCheck.draw, Q=this;
    WgLuaCheck.draw=function(){
      const r=draw.apply(this, arguments);
      if(this.data && Q._for!==this.data){
        Q.find(this.data).then(()=>{ if(WgLuaCheck.data===Q._for) Q.draw(); });
      }else Q.draw();
      return r;
    };
  }
};
WgLuaDeps.bind();
