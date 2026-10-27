/* =====================================================================================
   Splitters for the Wraithguard pages - drag the bar between two columns to make one wider;
   the widths are remembered (in this viewer) by the pane's key. Static: "can't resize ...
   various" (2026-10-27): the Load order, Conflicts and Tools pages had fixed columns.

   WgSplit.make(container, key, widths): `container` is a grid whose element children are its
   columns; each but the last gets the width in `widths` (px) and a bar after it, the last
   takes what is left. Pointer events, so it works where HTML drag-and-drop does not (the
   shell's webview hands drags to the window as file drops).

   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */

const WgSplit={
  styled:false,
  style(){
    if(this.styled) return; this.styled=true;
    const st=document.createElement('style');
    st.textContent=`.wg-split{cursor:col-resize;background:transparent;border-radius:3px;min-height:0;align-self:stretch;touch-action:none}
.wg-split:hover,.wg-split.on{background:var(--acd)}`;
    document.head.appendChild(st);
  },
  load(key){ try{ return JSON.parse(localStorage.getItem('wgSplit.'+key)||'null'); }catch(_){ return null; } },
  keep(key, w){ try{ localStorage.setItem('wgSplit.'+key, JSON.stringify(w)); }catch(_){ } },

  make(container, key, widths){
    if(!container || container.dataset.split) return;
    this.style();
    container.dataset.split=key;
    const cols=[...container.children].filter(c=>!c.classList.contains('wg-split'));
    const w=(this.load(key)||widths).slice(0, cols.length-1);
    while(w.length<cols.length-1) w.push(300);
    const apply=()=>{ container.style.gridTemplateColumns=w.map(x=>Math.round(x)+'px 6px').join(' ')+' minmax(0,1fr)'; };
    cols.slice(0,-1).forEach((col,i)=>{
      const bar=document.createElement('div');
      bar.className='wg-split'; bar.title='Drag to resize';
      col.insertAdjacentElement('afterend', bar);
      bar.addEventListener('pointerdown', e=>{
        e.preventDefault(); bar.setPointerCapture(e.pointerId); bar.classList.add('on');
        const x0=e.clientX, w0=w[i];
        const move=ev=>{ w[i]=Math.max(140, Math.min(container.clientWidth-200, w0+ev.clientX-x0)); apply(); };
        const up=()=>{ bar.classList.remove('on'); bar.removeEventListener('pointermove',move); bar.removeEventListener('pointerup',up); this.keep(key, w); };
        bar.addEventListener('pointermove', move); bar.addEventListener('pointerup', up);
      });
      bar.addEventListener('dblclick', ()=>{ w[i]=widths[i]||300; apply(); this.keep(key, w); });
    });
    container.style.columnGap='0'; container.style.gap='0';
    apply();
  },
};
