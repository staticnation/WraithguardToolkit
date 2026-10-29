/* =====================================================================================
   Splitters for the Wraithguard pages - drag the bar between two columns to make one wider;
   the widths are remembered (in this viewer) by the pane's key. Static: "can't resize ...
   various" (2026-10-27): the Load order, Conflicts and Tools pages had fixed columns.

   WgSplit.rows(container, key, share): the same for two rows (the Load order's content= over data=).

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
.wg-split.wg-split-y{cursor:row-resize;min-height:6px}
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

  /** `container` is a grid of two rows (its two element children, one above the other): a bar
   *  between them, dragged up or down, gives the upper one more or less of the height. The
   *  share (0.12 - 0.88) is remembered by `key`; a double-click puts it back to `share`. */
  rows(container, key, share){
    if(!container || container.dataset.splitRows) return;
    const cells=[...container.children].filter(c=>!c.classList.contains('wg-split'));
    if(cells.length!==2) return;
    this.style();
    container.dataset.splitRows=key;
    let r=this.load(key); if(!(typeof r==='number' && r>=0.12 && r<=0.88)) r=share;
    const apply=()=>{ container.style.gridTemplateRows=`minmax(0,${r}fr) 6px minmax(0,${1-r}fr)`; };
    const bar=document.createElement('div');
    bar.className='wg-split wg-split-y'; bar.title='Drag to resize (double-click: reset)';
    cells[0].insertAdjacentElement('afterend', bar);
    bar.addEventListener('pointerdown', e=>{
      e.preventDefault(); bar.setPointerCapture(e.pointerId); bar.classList.add('on');
      const y0=e.clientY, r0=r, h=Math.max(1, container.getBoundingClientRect().height-6);
      const move=ev=>{ r=Math.max(0.12, Math.min(0.88, r0+(ev.clientY-y0)/h)); apply(); };
      const up=()=>{ bar.classList.remove('on'); bar.removeEventListener('pointermove',move); bar.removeEventListener('pointerup',up); this.keep(key, r); };
      bar.addEventListener('pointermove', move); bar.addEventListener('pointerup', up);
    });
    bar.addEventListener('dblclick', ()=>{ r=share; apply(); this.keep(key, r); });
    container.style.gap='0';
    apply();
  },
};
