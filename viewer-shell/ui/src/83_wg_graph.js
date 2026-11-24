/* =====================================================================================
   Wraithguard: a tree drawn as a graph (Static 2026-11-23: "would also be nice to have a
   graph view in dialogue and journal / quests"). Boxes top-down joined by branches, each
   branch with its label (the order a response is tried, a choice, "set by"); the wheel
   zooms about the pointer, dragging the background pans, Fit shows it all; a box's ▾ folds
   what is under it. Used by the Dialogue window's Graph view (52_wg_dialogue_views.js) and
   Tools → Journal's Graph (61_wg_tools.js). Tools → HTN AI has its own, which also edits.

   `WgGraph.tree(host, root, opts)`: root = {id, kind, title, sub, tip, edge, children}, where
   `edge` is the label on the branch into it. opts: {colors: {kind: '#hex'}, onClick(node),
   onDbl(node), sel: id, legend: [[kind, words]...]}. SVG built with the DOM (the text of mods
   goes into the page as text).
   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
const WgGraph={
  NS:'http://www.w3.org/2000/svg',
  W:200, H:52, GX:16, GY:46,
  svg(tag, attrs, text){ const e=document.createElementNS(this.NS, tag); for(const k in attrs||{}) e.setAttribute(k, attrs[k]); if(text!=null) e.textContent=text; return e; },
  el(tag, cls, text){ const e=document.createElement(tag); if(cls) e.className=cls; if(text!=null) e.textContent=text; return e; },
  short(s, n){ s=String(s==null? '' : s).replace(/\s+/g, ' ').trim(); return s.length>n? s.slice(0, n-1)+'…' : s; },
  /** Each node's place: Map id -> {x, y, n, parent}, and the drawing's size. Folded nodes'
   *  children are left out. */
  layout(root, fold){
    const pos=new Map(), W=this.W, H=this.H, GX=this.GX, GY=this.GY;
    let left=0, bottom=0, guard=0;
    const place=(n, parent, depth)=>{
      if(++guard>20000) return left;
      const kids=(n.children && n.children.length && !fold.has(n.id))? n.children : [];
      let x;
      if(!kids.length){ x=left+W/2; left+=W+GX; }
      else{ const xs=kids.map(c=>place(c, n, depth+1)); x=(xs[0]+xs[xs.length-1])/2; }
      const y=depth*(H+GY);
      bottom=Math.max(bottom, y+H);
      pos.set(n.id, {x, y, n, parent});
      return x;
    };
    place(root, null, 0);
    return {pos, w:Math.max(left, W), h:bottom};
  },
  /** Draws `root` into `host` (emptied first). Returns {redraw, fit}. */
  tree(host, root, opts){
    opts=opts||{};
    const st={fold:opts.fold||new Set(), v:null};
    const COL=Object.assign({}, opts.colors||{});
    host.textContent='';
    host.style.position='relative';
    const bar=this.el('div'); bar.style.cssText='display:flex;gap:6px;align-items:center;flex-wrap:wrap;margin:2px 0 4px';
    const btn=(t, f, tip)=>{ const b=this.el('button', 'btn sm', t); b.onclick=f; if(tip) b.title=tip; return b; };
    const area=this.el('div', 'wgGraph');
    area.style.cssText='position:relative;height:'+(opts.height||'calc(100% - 34px)')+';min-height:360px;overflow:hidden;border:1px solid var(--line, rgba(128,128,128,.3));border-radius:6px;touch-action:none';
    bar.append(btn('Fit', ()=>{ st.v=null; draw(); }, 'Show it all'), btn('−', ()=>zoom(1/1.25), 'Zoom out (or the wheel)'), btn('+', ()=>zoom(1.25), 'Zoom in (or the wheel)'));
    for(const [k, words] of opts.legend||[]){
      const s=this.el('span', 'hint'); s.style.cssText='display:inline-flex;align-items:center;gap:4px';
      const i=this.el('i'); i.style.cssText='display:inline-block;width:10px;height:10px;border-radius:2px;background:'+(COL[k]||'#888');
      s.append(i, words); bar.append(s);
    }
    if(opts.hint) bar.append(this.el('span', 'hint', opts.hint));
    host.append(bar, area);
    let g=null;
    const apply=()=>{ if(g) g.setAttribute('transform', 'translate('+st.v.x+','+st.v.y+') scale('+st.v.k+')'); };
    const zoom=(f, cx, cy)=>{
      if(!st.v) return;
      const r=area.getBoundingClientRect(); cx=cx==null? r.width/2 : cx; cy=cy==null? r.height/2 : cy;
      const k=Math.max(0.15, Math.min(3, st.v.k*f)), q=k/st.v.k;
      st.v={x:cx-(cx-st.v.x)*q, y:cy-(cy-st.v.y)*q, k}; apply();
    };
    const draw=()=>{
      area.textContent='';
      const L=this.layout(root, st.fold), W=this.W, H=this.H;
      const r=area.getBoundingClientRect(), bw=r.width||900, bh=r.height||420;
      if(!st.v){ const k=Math.max(0.15, Math.min(1, (bw-40)/L.w, (bh-40)/(L.h||1))); st.v={x:Math.max(20, (bw-L.w*k)/2), y:20, k}; }
      const svg=this.svg('svg', {width:'100%', height:'100%'});
      svg.style.cssText='display:block;width:100%;height:100%;cursor:grab;user-select:none';
      g=this.svg('g', {}); apply(); svg.append(g);
      for(const [id, p] of L.pos){
        if(!p.parent) continue;
        const q=L.pos.get(p.parent.id); if(!q) continue;
        const mid=q.y+H+this.GY/2, col=COL[p.n.kind]||'#888';
        g.append(this.svg('path', {d:'M'+q.x+','+(q.y+H)+' V'+mid+' H'+p.x+' V'+p.y, fill:'none', stroke:col, 'stroke-width':1.5, opacity:.75}));
        if(p.n.edge!=null && p.n.edge!==''){
          const t=String(p.n.edge), w=Math.max(18, t.length*6+10);
          const c=this.svg('g', {transform:'translate('+p.x+','+(p.y-12)+')'});
          c.append(this.svg('rect', {x:-w/2, y:-8, width:w, height:15, rx:7, fill:col}), this.svg('text', {'text-anchor':'middle', y:3, 'font-size':9.5, fill:'#fff'}, this.short(t, 28)));
          g.append(c);
        }
      }
      for(const [id, p] of L.pos){
        const n=p.n, col=COL[n.kind]||'#888';
        const b=this.svg('g', {transform:'translate('+(p.x-W/2)+','+p.y+')', 'data-gid':id});
        b.style.cursor=opts.onClick? 'pointer' : 'default';
        b.append(this.svg('rect', {width:W, height:H, rx:7, fill:'var(--bg2, #222)', stroke:col, 'stroke-width':id===opts.sel? 3 : 1.5}));
        if(n.badge) b.append(this.svg('text', {x:8, y:14, 'font-size':9.5, fill:col}, this.short(n.badge, 34)));
        b.append(this.svg('text', {x:8, y:n.badge? 29 : 21, 'font-size':12, 'font-weight':600, fill:'currentColor'}, this.short(n.title, 28)));
        if(n.sub) b.append(this.svg('text', {x:8, y:n.badge? 44 : 38, 'font-size':9.5, fill:'currentColor', opacity:.68}, this.short(n.sub, 36)));
        b.append(this.svg('title', {}, [n.badge, n.title, n.tip||n.sub].filter(Boolean).join('\n')));
        if(n.children && n.children.length){
          const folded=st.fold.has(id);
          const f=this.svg('g', {transform:'translate('+(W-14)+','+(H-12)+')', 'data-fold':id});
          f.append(this.svg('rect', {x:-8, y:-8, width:16, height:16, rx:4, fill:'var(--bg, #111)', stroke:col}), this.svg('text', {'text-anchor':'middle', y:4, 'font-size':10, fill:col}, folded? '+' : '−'));
          f.append(this.svg('title', {}, folded? 'Show the '+n.children.length+' under it' : 'Fold away what is under it'));
          f.onpointerdown=e=>e.stopPropagation();
          f.onclick=e=>{ e.stopPropagation(); if(folded) st.fold.delete(id); else st.fold.add(id); draw(); };
          b.append(f);
        }
        b.onpointerdown=e=>e.stopPropagation();
        b.onclick=()=>{ if(opts.onClick) opts.onClick(n); };
        b.ondblclick=()=>{ if(opts.onDbl) opts.onDbl(n); };
        g.append(b);
      }
      svg.onpointerdown=e=>{
        if(e.button!==0) return;
        const x0=e.clientX, y0=e.clientY, v0=Object.assign({}, st.v);
        const mv=ev=>{ st.v={x:v0.x+ev.clientX-x0, y:v0.y+ev.clientY-y0, k:v0.k}; apply(); };
        const up=()=>{ document.removeEventListener('pointermove', mv); document.removeEventListener('pointerup', up); };
        document.addEventListener('pointermove', mv); document.addEventListener('pointerup', up);
      };
      svg.onwheel=e=>{ e.preventDefault(); const rr=area.getBoundingClientRect(); zoom(e.deltaY<0? 1.15 : 1/1.15, e.clientX-rr.left, e.clientY-rr.top); };
      area.append(svg);
    };
    draw();
    return {redraw:draw, fit:()=>{ st.v=null; draw(); }, state:st};
  },
};
