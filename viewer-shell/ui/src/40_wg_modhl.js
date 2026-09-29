
/* =====================================================================================
   Highlight a mod: outline every object in the loaded cells that a chosen plugin supplied.

   Every drawn object (`R.pickables`) already carries the plugin its reference came from,
   and the renderer has the object highlight (34_outline.js); this puts the two together.
   Chosen from the Preview section's "Highlight mod" list (the plugins with objects in the
   loaded cells, busiest first) or from ORI's plugin names. Re-applied whenever the loaded
   cells change. While it is on, hovering does not move the highlight - the mod's set is
   what is being looked at - and it is always drawn as the outline.
   Copyright (c) 2026 StaticNation.
   ===================================================================================== */
const WgModHl={
  mod:'',            // the plugin being highlighted, '' for none
  _mode:null,        // the highlight style to go back to

  bind(){
    const R=App.R; if(!R) return;
    // The loaded objects change with every cell load; the highlight follows them.
    const set=R.setPickables.bind(R);
    R.setPickables=list=>{ set(list); this.fill(); this.apply(); };
    const sel=$('#p_modhl');
    if(sel) sel.onchange=()=>this.choose(sel.value);
  },

  /** The plugins with objects in the loaded cells, most objects first, into the list. */
  fill(){
    const sel=$('#p_modhl'); if(!sel) return;
    const n=new Map();
    for(const p of ((App.R&&App.R.pickables)||[])) if(p && p.plugin) n.set(p.plugin,(n.get(p.plugin)||0)+1);
    const rows=[...n.entries()].sort((a,b)=>b[1]-a[1]||a[0].localeCompare(b[0]));
    sel.innerHTML='';
    const none=document.createElement('option'); none.value=''; none.textContent='None'; sel.appendChild(none);
    for(const [name,count] of rows){
      const o=document.createElement('option'); o.value=name;
      o.textContent=name+'  ('+count+')'; sel.appendChild(o);
    }
    // A mod no longer in view stays chosen, listed, so moving on and back keeps it.
    if(this.mod && !n.has(this.mod)){
      const o=document.createElement('option'); o.value=this.mod; o.textContent=this.mod+'  (0)'; sel.appendChild(o);
    }
    sel.value=this.mod;
  },

  /** Highlight a plugin's objects ('' clears). */
  choose(name){
    this.mod=String(name||'');
    const sel=$('#p_modhl'); if(sel && sel.value!==this.mod){ this.fill(); }
    this.apply();
  },

  active(){ return !!this.mod; },

  apply(){
    const R=App.R; if(!R) return;
    if(!this.mod){
      if(this._mode!=null){ R.hlMode=this._mode; this._mode=null; }
      R.setStaticHighlight(null); R.dirty=true; return;
    }
    if(this._mode==null) this._mode=R.hlMode||'fill';
    R.hlMode='outline';
    const want=this.mod.toLowerCase();
    R.setStaticHighlight((R.pickables||[]).filter(p=>p && String(p.plugin||'').toLowerCase()===want));
    R.dirty=true;
  },
};
