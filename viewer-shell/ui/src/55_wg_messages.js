/* =====================================================================================
   Wraithguard: the Editor's Messages panel.

   Every toast goes after a few seconds; what it said stays here (WgLog, 03_core.js):
   results, warnings, errors from Wraithguard and the engine, and the page's own
   failures that no toast reports. A dockable panel like the others (WgUI.mount), with
   a filter by kind and by text, Copy (what is shown, as text, for a bug report) and
   Clear. The toolbar's Messages button counts the errors since the panel was last open.
   ===================================================================================== */
const WgMessages={
  el:null, unseen:0, kinds:{err:true, warn:true, ok:true, info:true},

  /** The panel (made once, kept listening to the log). */
  panel(){
    if(this.el) return this.el;
    const d=document.createElement('div');
    d.id='edMsgPanel'; d.className='ori'; d.hidden=true;
    d.innerHTML='<div class="orihead"><b>Messages</b><span style="flex:1"></span>'+
      '<span id="edMsgKinds">'+
        [['err','Errors'],['warn','Warnings'],['ok','Done'],['info','Notes']].map(([k,l])=>
          '<label class="from" title="Show '+l.toLowerCase()+'"><input type="checkbox" data-k="'+k+'" checked> '+l+'</label> ').join('')+
      '</span>'+
      '<input type="search" id="edMsgFind" placeholder="Filter" title="Only the messages with this text" style="width:9em"> '+
      '<button class="btn sm" id="edMsgCopy" title="The messages shown, as text (time, kind, message), for a bug report">Copy</button> '+
      '<button class="btn sm" id="edMsgClear" title="Forget every message">Clear</button> '+
      '<button class="btn dim ic" id="edMsgX" title="Close">&#x2715;</button></div>'+
      '<div class="oribody" id="edMsgBody"></div>';
    if(typeof WgUI==='object' && WgUI.mount) WgUI.mount(d); else document.body.appendChild(d);
    d.querySelector('#edMsgX').onclick=()=>{ d.hidden=true; };
    d.querySelector('#edMsgClear').onclick=()=>WgLog.clear();
    d.querySelector('#edMsgFind').oninput=()=>this.draw();
    for(const c of d.querySelectorAll('#edMsgKinds input')) c.onchange=()=>{ this.kinds[c.dataset.k]=c.checked; this.draw(); };
    d.querySelector('#edMsgCopy').onclick=async()=>{
      const text=WgLog.text(this.shown());
      try{ await navigator.clipboard.writeText(text); toast('Messages copied','ok',1500); }
      catch(e){ toast('Could not copy: '+String(e.message||e),'err',4000); }
    };
    this.el=d;
    return d;
  },

  /** Starts listening to the log (from the page's start, so nothing is missed). */
  listen(){
    if(this._off) return;
    this._off=WgLog.on(e=>{
      const open=this.el && !this.el.hidden;
      if(!e) this.unseen=0;
      else if(e.kind==='err' && !open) this.unseen++;
      if(open) this.draw(); else this.badge();
    });
  },

  /** The messages that pass the panel's filters. */
  shown(){
    const q=this.el? (this.el.querySelector('#edMsgFind').value||'').trim().toLowerCase() : '';
    // A kind the panel has no box for counts as a note.
    return WgLog.items.filter(e=>this.kinds[e.kind in this.kinds? e.kind : 'info'] && (!q || e.msg.toLowerCase().includes(q)));
  },

  /** The list, newest at the bottom, scrolled there when it was already. */
  draw(){
    const body=this.panel().querySelector('#edMsgBody');
    const atEnd=body.scrollTop+body.clientHeight>=body.scrollHeight-4;
    const rows=this.shown();
    const p=n=>String(n).padStart(2,'0');
    body.innerHTML=rows.length? rows.map(e=>
      '<div class="edMsg '+escHtml(e.kind)+'"><span class="edMsgT">'+p(e.t.getHours())+':'+p(e.t.getMinutes())+':'+p(e.t.getSeconds())+'</span>'+
      '<span class="edMsgM">'+escHtml(e.msg)+'</span></div>').join('')
      : '<div class="hint">'+(WgLog.items.length? 'No message passes the filter.' : 'No messages yet. Results, warnings and errors are kept here after their toast goes.')+'</div>';
    if(atEnd) body.scrollTop=body.scrollHeight;
    this.badge();
  },

  /** Opens it (or closes it, when open): the errors are seen. */
  toggle(){
    const d=this.panel();
    d.hidden=!d.hidden;
    if(!d.hidden){ this.unseen=0; this.draw(); const b=d.querySelector('#edMsgBody'); b.scrollTop=b.scrollHeight; }
    this.badge();
  },

  /** The toolbar button's count of errors not yet seen. */
  badge(){
    const b=document.getElementById('edMsgBtn'); if(!b) return;
    b.classList.toggle('badge', this.unseen>0);
    b.dataset.n=this.unseen? String(this.unseen) : '';
  },
};
WgMessages.listen();
