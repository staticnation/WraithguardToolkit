
/* =====================================================================================
   Asset browser — pick a mesh or a texture rather than typing its path.
   ===================================================================================== */
/* The lists the pickers show. Filled by the engine when an install is connected —
   the `assets` command already walked the overlay while loading the world, and this
   used to crawl the same folders a second time through a browser API a packaged app
   does not have. What is left is a place to put the answer. */
const Assets={ meshes:[], textures:[], indexing:false, done:false };



/* =====================================================================================
   Favourites — the handful of entries you actually use, kept at the top.

   Every picker in the tool filters a list that is thousands long, and for most people
   the same six meshes and two ground textures come up over and over. Typing the filter
   again each time is the sort of small cost that is paid a hundred times a session.

   Two decisions worth writing down:

   **They live beside the exe, not in the rules file.** A favourite is a fact about the
   mods *this* install has — star a Tamriel Rebuilt mesh and it means nothing on a
   machine without TR — so it belongs with the install path rather than travelling with
   a set of rules somebody might hand to a friend.

   **They are keyed by kind, not by picker.** Star a ground texture in a rule's picker
   and it is at the top of every list that offers ground textures, because it is the
   same texture and the same person. Five kinds: `texture`, `mesh`, `region`, `rule`,
   and — round 18bg — `object`, the load-order picker's ids. An id is an install fact
   exactly as a mesh path is (star a Tamriel Rebuilt shrine and it means nothing on a
   machine without TR), so it sits on the install side of the first decision without
   needing the exception `rule` needs.

   `rule` bends the first decision a little and is worth the exception. A rule is named by
   the set you have open rather than by the install, so a star on one means nothing under
   a different set of grass rules — but a starred rule that is not there simply does not
   match, and the cost of that is a list in its plain order. Against it: picking which
   rule a paint colour stands for is the one list you come back to over and over while
   painting, and it is your own list rather than a thousand of somebody else's.

   Order is settled when a list is drawn, not while you are looking at it: starring an
   entry does not make it leap to the top under the cursor. It is there next time the
   picker opens, which is what the star is for.
   ===================================================================================== */
const Favourites={
  /** kind -> [item], as the engine last reported it. */
  map:{},
  ready:false,

  async load(force){
    if(this.ready && !force) return;
    this.ready=true;
    if(!Engine.has()) return;
    /* A picker that cannot read the favourites is still a working picker, so this
       fails to an empty set rather than to an error: the cost of getting it wrong is
       a list in its plain order. */
    try{ this.map=(await Engine.call('favourites_list'))||{}; }
    catch(e){ this.map={}; }
  },

  list(kind){ const v=this.map[kind]; return Array.isArray(v)? v : []; },

  /* Case-insensitive throughout, because a mesh path is spelled three ways across a
     load order and a favourite that only matches one of them looks broken. */
  has(kind,item){
    const w=String(item||'').toLowerCase();
    return this.list(kind).some(v=>String(v).toLowerCase()===w);
  },

  async set(kind,item,on){
    const w=String(item).toLowerCase();
    const cur=this.list(kind).filter(v=>String(v).toLowerCase()!==w);
    if(on) cur.push(item);
    if(cur.length) this.map[kind]=cur; else delete this.map[kind];
    if(!Engine.has()) return;
    try{ await Engine.call('favourite_set',{kind,item,on:!!on}); }
    catch(e){ toast(T('browse.fav_failed',{err:e.message||e}),'warn'); }
  },

  toggle(kind,item){ const on=!this.has(kind,item); this.set(kind,item,on); return on; },

  /** Favourites first, everything else after, each keeping the order it arrived in. */
  sort(kind,items,key){
    key=key||(v=>v);
    const top=[],rest=[];
    for(const it of items) (this.has(kind,key(it))? top : rest).push(it);
    return top.concat(rest);
  },

  /** The star for one picker row. Filled and always visible once it is a favourite;
      an outline that appears on hover otherwise, so a list you are only reading is not
      a wall of grey stars.

      `after` is the picker's own redraw. Starring reorders the list at once — the first
      version settled the order only when the list was next drawn, on the theory that a
      row leaping out from under the cursor is startling. In use it reads as the star not
      working: you click it, nothing moves, and the only way to find out whether it took
      is to close the picker and open it again. Moving is the feedback. */
  star(row,kind,item,after){
    const el=document.createElement('span');
    el.className='fav';
    el.dataset.kind=kind; el.dataset.item=item;
    el.style.cssText='flex:0 0 auto;cursor:pointer;font-size:13px;line-height:1;'+
      'padding:0 3px;user-select:none;transition:opacity .08s';
    const paint=()=>{
      const on=Favourites.has(kind,item);
      el.textContent=on?'\u2605':'\u2606';
      el.dataset.on=on?'1':'';
      el.style.color=on?'var(--acc)':'var(--tx3)';
      el.style.opacity=on?'1':(row.dataset.favhover?'0.8':'0');
      el.title=on? T('browse.star_on_title') : T('browse.star_off_title');
    };
    el.onclick=e=>{ e.stopPropagation(); e.preventDefault();
                    Favourites.toggle(kind,item); paint();
                    if(after) after(); };
    row.addEventListener('mouseenter',()=>{ row.dataset.favhover='1'; paint(); });
    row.addEventListener('mouseleave',()=>{ row.dataset.favhover=''; paint(); });
    paint();
    return el;
  },
};



