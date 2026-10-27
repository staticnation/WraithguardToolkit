/* =====================================================================================
   The Help page - Wraithguard (port step 4.4). The program's own documents (Quick start,
   Read me, Writing mlox rules), rendered by `toolscore::docs` (`help_doc`, the renderer
   `wraithguard/viz/docs.py` had) and read here: the document, its contents beside it,
   a link to another of them opening here, a web link in the browser (`help_open_link`).

   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */

const WgHelp={
  on:false,
  docs:[], doc:'',

  view(){ return window.__WG_VIEW__||{}; },
  extra(){ return this.view().extra||{}; },
  dirs(){ const d=this.extra().docsDir; return d? [String(d)] : []; },

  button(){
    if($('#btnHelp')) return;
    const after=$('#btnTools')||$('#btnConflicts')||$('#btnLoadOrder')||$('#btnReport'); if(!after) return;
    const b=document.createElement('button');
    b.className='btn dim'; b.id='btnHelp'; b.textContent='Help';
    b.title='The program\'s documents: Quick start, Read me, Writing mlox rules';
    b.onclick=()=> this.on? this.leave() : this.enter();
    after.insertAdjacentElement('afterend', b);
  },

  enter(){
    if(this.on) return;
    if(typeof WgLoadOrder==='object' && WgLoadOrder.on) WgLoadOrder.leave();
    if(typeof WgConflicts==='object' && WgConflicts.on) WgConflicts.leave();
    if(typeof WgTools==='object' && WgTools.on) WgTools.leave();
    this.on=true;
    this.build();
    $('#wgHP').hidden=false;
    const i=document.querySelector('#brand i'); if(i){ this._brand=i.textContent; i.textContent='Help'; }
    document.title='Wraithguard - Help';
    const b=$('#btnHelp'); if(b) b.classList.add('on');
    const want=this.extra().helpDoc; if(want) this.extra().helpDoc='';
    this.list(want||this.doc||'');
  },

  leave(){
    if(!this.on) return;
    this.on=false;
    $('#wgHP').hidden=true;
    const i=document.querySelector('#brand i'); if(i && this._brand) i.textContent=this._brand;
    const b=$('#btnHelp'); if(b) b.classList.remove('on');
  },

  build(){
    if($('#wgHP')) return;
    const st=document.createElement('style');
    st.textContent=`
#wgHP{position:fixed;left:0;right:0;top:var(--top-h,40px);bottom:0;z-index:60;background:var(--bg);color:var(--tx);
  display:grid;grid-template-columns:240px 1fr;gap:8px;padding:8px;overflow:hidden;font-size:13px}
#wgHP[hidden]{display:none}
#wgHP .side{display:flex;flex-direction:column;gap:6px;min-height:0;background:var(--bg2);border:1px solid var(--line);border-radius:var(--r-box);padding:8px;overflow:auto}
#wgHP .side b{color:var(--tx3);font-size:11px;text-transform:uppercase;letter-spacing:.05em;margin-top:6px}
#wgHP .side a{display:block;color:var(--tx);text-decoration:none;padding:2px 0 2px 8px;border-left:2px solid transparent;cursor:pointer}
#wgHP .side a:hover{color:var(--ac);border-left-color:var(--ac)}
#wgHP .side a.on{border-left-color:var(--ac);color:var(--ac)}
#wgHP .side a.sub2{padding-left:20px;color:var(--tx2);font-size:12px}
#wgHP .side .missing{color:var(--tx3);padding-left:10px;font-size:12px}
#wgHP .doc{position:relative;overflow:auto;min-height:0;background:var(--bg2);border:1px solid var(--line);border-radius:var(--r-box)}
#wgHP main{max-width:860px;margin:0 auto;padding:12px 26px 40px;line-height:1.65}
#wgHP main .src{color:var(--tx3);font-size:12px}
#wgHP main h1,#wgHP main h2,#wgHP main h3{line-height:1.3;margin:1.4em 0 .5em}
#wgHP main h1{font-size:22px}#wgHP main h2{font-size:18px;border-bottom:1px solid var(--line);padding-bottom:4px}#wgHP main h3{font-size:15px}
#wgHP main a{color:var(--ac)}
#wgHP main code{font-family:var(--mono);background:var(--bg3);padding:1px 4px;border-radius:3px;font-size:.92em}
#wgHP main pre{background:var(--bg3);border:1px solid var(--line);border-radius:6px;padding:10px 12px;overflow:auto}
#wgHP main pre code{background:none;padding:0}
#wgHP main blockquote{margin:1em 0;padding:4px 14px;border-left:3px solid var(--ac);color:var(--tx2)}
#wgHP main table{border-collapse:collapse;margin:1em 0;display:block;overflow:auto}
#wgHP main th,#wgHP main td{border:1px solid var(--line);padding:4px 8px;text-align:left;vertical-align:top}
#wgHP main th{background:var(--bg3)}
#wgHP main hr{border:0;border-top:1px solid var(--line);margin:2em 0}
#wgHP main img{max-width:100%}`;
    document.head.appendChild(st);
    const el=document.createElement('div');
    el.id='wgHP'; el.hidden=true;
    el.innerHTML=`<div class="side" id="hpSide"><div class="empty">Reading...</div></div>
<div class="doc" id="hpDoc"><main id="hpMain"><div class="empty">Pick a document.</div></main></div>`;
    document.body.appendChild(el);
    el.addEventListener('click', e=>this.click(e));
  },

  async list(want){
    try{ this.docs=await Engine.call('help_docs',{dirs:this.dirs()}); }
    catch(e){ this.docs=[]; toast(String(e.message||e),'err',5000); }
    const first=this.docs.find(d=>d.found);
    const pick=this.docs.find(d=>d.name===want && d.found)||this.docs.find(d=>d.name===this.doc && d.found)||first;
    if(pick) await this.open(pick.name);
    else this.drawSide(null);
  },

  async open(name, anchor){
    try{
      const d=await Engine.call('help_doc',{name, dirs:this.dirs()});
      this.doc=name;
      $('#hpMain').innerHTML=`<div class="src">${loEsc(d.path)}</div>${d.body}`;
      this.drawSide(d);
      const box=$('#hpDoc');
      const at=anchor? document.getElementById(anchor) : null;
      box.scrollTop= at? at.offsetTop-8 : 0;
    }catch(e){ $('#hpMain').innerHTML=`<div class="empty">${loEsc(String(e.message||e))}</div>`; }
  },

  /* The documents, and the open one's contents (`_contents`: levels 2 and 3, when there are three or more). */
  drawSide(d){
    const out=['<b>Documents</b>'];
    for(const doc of this.docs){
      out.push(doc.found? `<a data-doc="${loEsc(doc.name)}" class="${doc.name===this.doc?'on':''}">${loEsc(doc.title)}</a>`
        : `<div class="missing" title="Not found beside the program">${loEsc(doc.title)} (not found)</div>`);
    }
    if(!this.docs.some(x=>x.found)) out.push('<div class="empty">None of the documents was found beside the program. They ship with it; in a checkout they are in the project folder.</div>');
    const entries=d? d.headings.filter(h=>h[0]===2||h[0]===3) : [];
    if(entries.length>=3){
      out.push('<b>On this page</b>');
      for(const [level,text,id] of entries) out.push(`<a data-anchor="${loEsc(id)}" class="${level===3?'sub2':''}">${loEsc(text)}</a>`);
    }
    $('#hpSide').innerHTML=out.join('');
  },

  /* Links: an anchor scrolls, another document opens here, the web goes to the browser. */
  click(e){
    const a=e.target.closest('a'); if(!a) return;
    if(a.dataset.doc){ e.preventDefault(); this.open(a.dataset.doc); return; }
    if(a.dataset.anchor){ e.preventDefault(); this.scrollTo(a.dataset.anchor); return; }
    const href=a.getAttribute('href'); if(href==null) return;
    e.preventDefault();
    if(href.startsWith('#')){ this.scrollTo(decodeURIComponent(href.slice(1))); return; }
    if(/^(https?:|mailto:)/i.test(href)){
      Engine.call('help_open_link',{url:href}).catch(err=>toast(String(err.message||err),'err',5000));
      return;
    }
    const [file, anchor]=href.split('#');
    const name=file.split(/[\\/]/).pop();
    if(this.docs.some(d=>d.name.toLowerCase()===name.toLowerCase() && d.found)) this.open(this.docs.find(d=>d.name.toLowerCase()===name.toLowerCase()).name, anchor);
    else if(/\.md$/i.test(name)) this.open(name, anchor);
    else toast(`${href} is a file in the project, not one of these documents`,'warn',4000);
  },

  scrollTo(id){
    const at=document.getElementById(id);
    if(at) $('#hpDoc').scrollTop=at.offsetTop-8;
  },
};
