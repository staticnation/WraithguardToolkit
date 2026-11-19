/* =====================================================================================
   Wraithguard: animation blending - an actor's change of animation eased over a moment
   instead of snapping, as OpenMW's "Smooth animation transitions" does it
   (apps/openmw/mwrender/animblendcontroller.cpp, components/sceneutil/animblendrules.cpp,
   docs/source/reference/modding/animation-blending.rst; Static's references, 2026-11-19:
   Nexus 53842, the OpenMW build, and 53779, the MWSE port, both after Max Yari's work).

   - The rules are OpenMW's: `blending_rules`, each `{from, to, easing, duration}`, the
     names `group:key` lower-cased with `*` at either end (or alone) and `$` as `to` for the
     group it comes from; the bottom-most rule that fits wins. The load order's
     `animations/animation-config.yaml` (or `.json`; OpenMW's own from its resources folder
     when no mod has one), and a model's own beside its `.kf`, after it, as OpenMW adds them.
     OpenMW's defaults until they are read, or when there are none.
   - The blend is OpenMW's: when a live actor's animation changes (18_cellpreview.js
     `previewSwapActor`), each animated node of each part starts from the pose it was in -
     mid-blend, where the blend had it - and for the rule's duration is slerped (rotation)
     and lerped (translation) toward the new animation by the easing of the time gone; the
     scale is the new one's at once (10_preview.js `blendTS`). No rule fits: no blend.
   - The renderer reads it (06_gl.js): a skinned part's bones (`_skinUpdate`) and a rigid
     animated part's node (`animMatrixAt`) are posed through it while it lasts.

   The easings are ported from OpenMW (GPL-3: a reference for what the game does); the
   code is this page's. Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
const WgAnimBlend={
  on:true,
  /* OpenMW's files/data/animations/animation-config.yaml, as it ships. */
  DEFAULT:[
    ['*', '*', 'sineOut', 0.25], ['*', 'idlesneak*', 'springOutWeak', 0.4], ['*', 'sneakforward*', 'springOutWeak', 0.4],
    ['*', '*:shoot*', 'sineOut', 0.1], ['*', '*:chop*', 'sineOut', 0.1], ['*', '*:thrust*', 'sineOut', 0.1], ['*', '*:slash*', 'sineOut', 0.1],
    ['*:*start', '*:*attack', 'sineOut', 0.05], ['*', '*:*follow start', 'linear', 0], ['jump:start', '*', 'sineOut', 0.1],
    ['*', 'inventory*', 'linear', 0], ['inventory*', '*', 'linear', 0], ['', '*', 'linear', 0],
  ],
  /* OpenMW's easings (animblendcontroller.cpp `Easings`). */
  EASE:(()=>{
    const spring=l=>x=>1-Math.exp(-l*x)*Math.cos(1.5*Math.PI*x);
    return {
      linear:x=>x, sineOut:x=>Math.sin(x*Math.PI/2), sineIn:x=>1-Math.cos(x*Math.PI/2), sineInOut:x=>-(Math.cos(Math.PI*x)-1)/2,
      cubicOut:t=>1-(1-t)**3, cubicIn:x=>x*x*x, cubicInOut:x=>x<0.5? 4*x*x*x : 1-((-2*x+2)**3)/2,
      quartOut:t=>1-(1-t)**4, quartIn:t=>t**4, quartInOut:x=>x<0.5? 8*x**4 : 1-((-2*x+2)**4)/2,
      springOutWeak:spring(4), springOutMed:spring(3), springOutStrong:spring(2), springOutTooMuch:spring(1),
    };
  })(),
  _global:null,           // the load order's rules, once read (null: OpenMW's defaults)
  _files:new Map(),       // model -> its own rules (null: none)
  _asked:new Set(),

  /** `group:key` as OpenMW splits a rule's name: lower-cased, trimmed, the key after the first colon. */
  split(full){
    const s=String(full==null? '' : full).toLowerCase(), i=s.indexOf(':');
    return i<0? [s.trim(), ''] : [s.slice(0, i).trim(), s.slice(i+1).trim()];
  },
  rule(from, to, easing, duration){
    const [fg, fk]=this.split(from), [tg, tk]=this.split(to);
    return {fg, fk, tg, tk, easing:String(easing), duration:+duration||0};
  },
  /** A config file's text (YAML as OpenMW's files are written, or JSON) as rules; [] for none. */
  parse(text){
    if(!text) return [];
    const t=String(text).trim();
    let list=null;
    if(t[0]==='{' || t[0]==='['){ try{ const j=JSON.parse(t); list=(j && j.blending_rules) || null; }catch(_){ list=null; } }
    else{
      // The YAML OpenMW's files use: `blending_rules:`, then `- from: "..."` items with
      // `to`, `easing` and `duration` under them. Comments and quotes as YAML has them.
      list=[]; let cur=null, inRules=false;
      for(const raw of t.split(/\r?\n/)){
        const line=raw.replace(/\s+#.*$/, '').replace(/^\s*#.*$/, '');
        if(!line.trim()) continue;
        if(/^blending_rules\s*:/.test(line)){ inRules=true; continue; }
        if(!inRules) continue;
        if(/^\S/.test(line)){ inRules=false; continue; }
        const m=/^\s*(-\s*)?([A-Za-z_]+)\s*:\s*(.*)$/.exec(line);
        if(!m) continue;
        if(m[1]){ cur={}; list.push(cur); }
        if(!cur) continue;
        let v=m[3].trim();
        if((v[0]==='"' && v.endsWith('"')) || (v[0]==="'" && v.endsWith("'"))) v=v.slice(1, -1);
        cur[m[2]]=v;
      }
    }
    return (list||[]).filter(r=>r && r.from!=null && r.to!=null && r.duration!=null && r.easing!=null)
      .map(r=>this.rule(r.from, r.to, r.easing, r.duration));
  },
  rules(model){
    const base=this._global || (this._def=this._def||this.DEFAULT.map(r=>this.rule(...r)));
    const own=model && this._files.get(model);
    return own && own.length? base.concat(own) : base;
  },
  fits(str, rule){
    return rule==='*' || str===rule || (rule.startsWith('*') && str.endsWith(rule.slice(1))) || (rule.endsWith('*') && str.startsWith(rule.slice(0, -1)));
  },
  /** The rule for a change from one group to another (animblendrules.cpp `findBlendingRule`). */
  find(from, to, model){
    const [fg, fk]=this.split(from), [tg, tk]=this.split(to);
    const list=this.rules(model);
    for(let i=list.length-1;i>=0;i--){
      const r=list[i];
      const fromOk=this.fits(fg, r.fg) && (!r.fk || this.fits(fk, r.fk));
      const toOk=(this.fits(tg, r.tg) || (r.tg==='$' && tg===fg)) && (!r.tk || this.fits(tk, r.tk));
      if(fromOk && toOk) return r;
    }
    return null;
  },
  /** The load order's rules and the models' own, read once each (`anim_blend_config`). */
  async load(models){
    const want=(models||[]).filter(m=>m && !this._asked.has(m));
    if(this._global!==null && !want.length) return;
    for(const m of want) this._asked.add(m);
    try{
      const r=await Engine.call('anim_blend_config', {models:want});
      const o=typeof r==='string'? JSON.parse(r) : r;
      if(o && o.global){ const g=this.parse(o.global); if(g.length) this._global=g; }
      if(this._global===null) this._global=false;   // asked: OpenMW's defaults stand
      for(const [m, t] of Object.entries((o && o.files)||{})) this._files.set(m, t? this.parse(t) : null);
    }catch(_){ if(this._global===null) this._global=false; }
  },
  /** The group a model's spec plays (`__grp/<spec>/...`, 18_cellpreview.js `animModel`): its
   *  first slot's, as `group:start`; none (the mesh's own idle) is `idle`. */
  groupOf(spec){
    const first=String(spec||'').split(',').find(x=>x)||'';
    const g=first.replace(/[@*].*$/, '');
    return (g || 'idle')+':start';
  },
  /** Where a blend stands at `secs`: its eased factor, or null once it is over (and dropped). */
  factor(B, secs){
    if(!B) return null;
    if(B.t0==null) B.t0=secs;
    const x=B.dur>0? (secs-B.t0)/B.dur : 1;
    if(!(x<1)){ B.done=true; return null; }
    return B.ease(Math.max(0, x));
  },
  /** A live actor's animation changing from `oldBatches` to `newBatches` (one batch a part,
   *  in the same order): each new part told the pose its old one was in, as the blend's start.
   *  `fromSpec` / `toSpec`: what each plays (null: the mesh's own idle). Nothing when no rule
   *  fits, the duration is nought, or the parts are not the same skeleton. */
  begin(oldBatches, newBatches, fromSpec, toSpec, model, secs){
    if(!this.on || !oldBatches || !newBatches || oldBatches.length!==newBatches.length) return false;
    const r=this.find(this.groupOf(fromSpec), this.groupOf(toSpec), model);
    if(!r || !(r.duration>0)) return false;
    const ease=this.EASE[r.easing];
    if(!ease) return false;
    const B={t0:secs, dur:r.duration, ease, rule:r, done:false};
    let any=false;
    for(let i=0;i<oldBatches.length;i++){
      const ob=oldBatches[i], nb=newBatches[i], op=ob && ob.part, np=nb && nb.part;
      if(!op || !np) continue;
      const ot=secs*(op._animSpeed||1)+(op._animShift||0);
      // A skinned part: each bone's chain, where the old one has it now (mid-blend too).
      if(op.skin && np.skin && op.skin.bones.length===np.skin.bones.length &&
         op.skin.bones.every((b, k)=>(b.chain||[]).length===(np.skin.bones[k].chain||[]).length)){
        const ob2=op._blend, of=ob2 && this.factor(ob2.B, secs);
        np._blend={B, skin:op.skin.bones.map((b, k)=>animLinksAt(b.chain, ot, of!=null && ob2.skin? ob2.skin[k] : null, of))};
        np._skinAt=null; any=true;
      }
      // A rigid part under animated nodes: its chain, the same.
      const oa=op.anim && op.anim.nodes, na=np.anim && np.anim.nodes;
      if(oa && na && oa.length && oa.length===na.length){
        const ob3=ob._blend, of=ob3 && this.factor(ob3.B, secs);
        nb._blend={B, nodes:animLinksAt(oa, ot, of!=null? ob3.nodes : null, of)};
        any=true;
      }
    }
    return any;
  },
};
