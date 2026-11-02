/* =====================================================================================
   Wraithguard Editor (50_wg_editor.js): the Editor's keys.

   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
Object.assign(WgEditor, {
  /* ---- keys ----------------------------------------------------------------------- */

  keys(e){
    if(!this.on) return false;
    const inBuild=this.bld && !this.bld.hidden && document.activeElement && document.activeElement.closest && document.activeElement.closest('.edBuildList');
    if(inBuild){
      if(e.key==='Delete' || e.key==='Backspace'){ this.dropSelectedPending(); return true; }
      if((e.ctrlKey||e.metaKey) && String(e.key).toLowerCase()==='a'){ const b=document.getElementById('edBuildAll'); if(b) b.click(); return true; }
      if(e.key==='Escape' && this._bSel && this._bSel.size){ this._bSel.clear(); this.showBuild(); return true; }
    }
    if(typeof WgPath==='object' && WgPath.keys(e)) return true;
    const typing=/^(INPUT|TEXTAREA|SELECT)$/.test((document.activeElement||{}).tagName||'');
    const k=String(e.key||'').toLowerCase();
    // The shortcuts are the key binding page's (69_wg_keys.js): `is(e, action)`.
    const K=(id)=>typeof WgKeys==='object'? WgKeys.is(e, id) : false;
    if(!typing && (k==='z'||k==='x'||k==='y') && !e.ctrlKey && !e.metaKey) this.held.add(k);
    if(K('drop') && !typing && this.selected()) return this.drop();
    if(K('quick') && !typing && (this.selected() || this.group().length>=1) && !(App.R && App.R.fly && App.R.fly.active)){
      this.quickMenu(this._pointer? this._pointer[0] : null, this._pointer? this._pointer[1] : null);
      return true;
    }
    if(e.key==='Escape' && this.qm && !this.qm.hidden){ this.qm.hidden=true; return true; }
    // The Construction Set's own: snaps, undo, copy and paste, delete, centre on it.
    // Alt is part of a chord now (an exact match), so it no longer rules a key out.
    const plain=!typing && !(App.R && App.R.fly && App.R.fly.active);
    if(plain && K('angle')){ this.angleOn=!this.angleOn; toast('Snap to angle '+(this.angleOn? 'on: '+this.angle+'° steps' : 'off'),'ok',1800); this.savePrefs(); return true; }
    if(plain && K('grid')){ this.gridOn=!this.gridOn; toast('Snap to grid '+(this.gridOn? 'on: '+this.grid+' units' : 'off'),'ok',1800); this.savePrefs(); return true; }
    if(plain && K('undo')){ this.undo(); return true; }
    if(plain && K('redo')){ this.redo(); return true; }
    if(plain && K('copy') && this.selected()) return this.copySel();
    if(plain && K('paste') && this._clip){ this.paste(); return true; }
    if(plain && K('duplicate') && this.selected()){
      const sel=this.selected(), rec=this.selectedRecord(sel);
      if(rec.tag){ this.quickAction('dup', sel, rec); return true; }
    }
    if(plain && K('delete') && this.selected()){
      const sel=this.selected(); this.quickAction('del', sel, this.selectedRecord(sel)); return true;
    }
    if(plain && K('frame') && this.transformTargets().length) return this.frameSel();
    // Selection sets (Ctrl+1..9 keeps, 1..9 recalls), hide (H), isolate (Shift+H), Ctrl+T.
    if(plain && !e.shiftKey && /^Digit[1-9]$/.test(e.code||'')){
      const n=+e.code.slice(5);
      if(e.ctrlKey||e.metaKey) return this.keepSet(n);
      return this.recallSet(n);
    }
    if(plain && K('hide')) return this.hideSel();
    if(plain && K('isolate')) return this.isolateSel();
    if(plain && K('transform')){ this.showTransform(); return true; }
    if(plain && K('tfh')){ this.setPanels('tfh', this.showTfh===false); return true; }
    if(plain && K('ori')){ this.setPanels('ori', this.showOri===false); return true; }
    if(K('messages') && typeof WgMessages==='object'){ WgMessages.toggle(); return true; }
    if(K('testRun') && this.links().editTestRun){ this.testRun(); return true; }
    if(plain && K('verify') && this.links().editVerify){ this.showVerify(); return true; }
    if(e.key==='Escape' && !typing && this._group.length){ this.clearGroup(); return true; }
    // Layers: Ctrl+Shift+1..9 shows or hides layer N (the Layers panel numbers them).
    if(!typing && (e.ctrlKey||e.metaKey) && e.shiftKey && /^Digit[1-9]$/.test(e.code||'')){
      const L=this.layers(), l=L[+e.code.slice(5)-1];
      if(l){ l.visible=!l.visible; this.layersChanged(); toast('Layer '+l.name+(l.visible? ' shown' : ' hidden'),'ok',1500); }
      return true;
    }
    if(plain && K('prefab') && typeof WgFlow==='object'){ WgFlow.prefabMenu(this._pointer && this._pointer[0], this._pointer && this._pointer[1]); return true; }
    if(K('searchAll') && this.links().editSearchAll && typeof WgFlow==='object'){ WgFlow.showSearch(); return true; }
    if(K('filter')){
      const f=this.part('edFilter'); if(f){ f.focus(); f.select(); }
      return true;
    }
    if(K('record') && !typing){
      const r=this._oriRecord;
      if(r && Ori.el && !Ori.el.hidden){ this.openRecord(r.tag, r.id, r.plugins); return true; }
    }
    if(K('reference') && !typing){
      const r=this._sel && this._sel.ref;
      if(r){ this.openRef(r); return true; }
    }
    if(e.key==='Escape' && this.dlg && !this.dlg.hidden && !typing){ this.dlg.hidden=true; return true; }
    return false;
  },
});


document.addEventListener('keydown', e=>{ if(WgEditor.keys(e)){ e.preventDefault(); e.stopPropagation(); } }, true);
document.addEventListener('keyup', e=>WgEditor.held.delete(String(e.key||'').toLowerCase()), true);
document.addEventListener('pointermove', e=>{ WgEditor._pointer=[e.clientX, e.clientY]; }, true);
document.addEventListener('pointerdown', e=>{ const q=WgEditor.qm; if(q && !q.hidden && !q.contains(e.target)) q.hidden=true; }, true);
window.addEventListener('blur', ()=>WgEditor.held.clear());
