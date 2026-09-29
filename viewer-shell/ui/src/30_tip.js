/* =====================================================================================
   The tooltip — round 18ay.

   Robin, of the redesign proposals: "I want the tooltips as scroll notes." It began as a
   parchment note and by 18ba was a plain, slightly lighter box in the page's own style
   (`#tip` in 01_head.html, which tells the story); what has stayed the same is this, the
   part that shows it.

   **The `title` is still the note.** Every tooltip in the page is a `title` attribute -
   most of them keyed (`data-txt`, so a language pack can reword them, and `t_wording`
   can check them), a good many written by script as a panel draws. Nothing about that
   changes: this reads the `title` of whatever the pointer is over and draws it in the
   note. What it has to do as well is keep the browser's own yellow box from coming up
   over it, and the only way to do that is for the element to have no `title` while the
   pointer rests on it. So the title is *parked*: moved into `data-tip` on the way in and
   put back on the way out. `Text.apply` and `Text.builtin` know to look in both places,
   so a language switch while a tooltip is up rewords the parked title too, and a script
   that writes a fresh `title` on an element the pointer is over wins - on the way out
   the parked one is put back only if the attribute is still empty.

   Timing is the browser's: a pause before the note, none once one is up and the pointer
   moves to the next titled thing, gone at once on leaving, pressing, scrolling or a key.

   Round 18bf closed three things the bug hunt found here.

   **Disabled controls** (§I11). The header used to claim they got their notes "the same as
   they did — the browser sends the pointer events for them". It does not: Chromium
   dispatches no mouse events on a disabled form control at all, so `mouseover` never
   arrived with one as its target and the note never came — while the browser's *own*
   yellow box did, which is the one thing this layer exists to prevent. And a disabled
   control is exactly where the note carries the only explanation of why it is disabled
   (the "this is the only one" Delete, the greyed Write options). So the element under the
   pointer is found by hit test (`elementFromPoint`) rather than by event target, which
   sees a disabled control like anything else.

   **The warm window** (§I12). `mouseout` fires *before* `mouseover`, and its handler took
   the note down — so `Tip.up` was already false by the time the new element was read and
   every note along a row of buttons waited the full pause. The browser's rule is about
   *recency*, not about whether a note is up this instant, so that is what this keeps:
   a note taken down within `WARM` counts as one being up.

   **A label's note is its control's note** (Robin, round 18bf): "If a label next to a
   control (like 'Atmosphere' next to the toggle element that activates it) has a tooltip
   on cursor hover, make the actual control that the label belongs to have the same tooltip
   on hover." Read rather than copied, so it holds for panels the page rewrites as it
   draws — see `labelNote`. A control with a `title` of its own says its own thing, and an
   **empty** one still means "no note here", which is how a control opts out of its row's. */
var Tip={   // `var`: `Text.start()` runs before this part, and asks `typeof Tip`
  el:null,
  /** The element whose title is parked in `data-tip`, or null. */
  over:null,
  timer:0,
  /** Whether a note is up, so the next one comes without the pause. */
  up:false,
  /** When the last note went down, so §I12's warm window can be measured. */
  downAt:-1e9,
  DELAY:480,
  /** How long after a note goes down the next one still comes quickly. */
  WARM:400,

  /** A title as the element itself states it: parked, or in the attribute. No label. */
  own(el){
    if(!el || !el.getAttribute) return '';
    const parked=el.dataset? el.dataset.tip : null;
    return parked!=null? parked : (el.getAttribute('title')||'');
  },

  /** Reads a title wherever it is: parked, in the attribute, or on the label that claims
   *  this control (round 18bf). */
  read(el){
    if(!el || !el.getAttribute) return '';
    const parked=el.dataset? el.dataset.tip : null;
    if(parked!=null) return parked;
    const t=el.getAttribute('title');
    /* An empty title is the way of saying "no tooltip here", and it has to say it for
       the label too, or a control could not opt out of its row's note. */
    if(t!=null) return t;
    return Tip.labelNote(el);
  },

  /** The note on the label this control belongs to, or ''.
   *
   *  Two associations, in the order they are trusted: the one HTML states (`label[for]`),
   *  and the page's own row layout — a `.row` holds one label and one `.ctl`, so anything
   *  in the `.ctl` belongs to the label beside it. A label the control sits *inside*
   *  needs no rule here: the ancestor walk in `titled` reaches it already.
   *
   *  Only for a control. Blank space in a row is not a control, and a row that explained
   *  itself wherever the pointer fell would put a note under the pointer constantly. */
  labelNote(el){
    if(!el.closest || !el.tagName) return '';
    const form=/^(INPUT|SELECT|TEXTAREA|BUTTON)$/.test(el.tagName);
    if(!form && !el.closest('.ctl')) return '';
    if(el.id){
      const esc=(typeof CSS==='object' && CSS.escape)? CSS.escape(el.id) : el.id;
      let l=null;
      try{ l=document.querySelector('label[for="'+esc+'"]'); }catch(_){ l=null; }
      const t=l? Tip.own(l) : '';
      if(t) return t;
    }
    const row=el.closest('.row');
    if(row){
      /* The row's own label, not one nested in the control — `<label class="sw">` wraps
         the toggle's checkbox and carries no note of its own. */
      for(const kid of row.children){
        if(kid.tagName==='LABEL' && kid!==el && !kid.contains(el)){
          const t=Tip.own(kid);
          if(t) return t;
          break;
        }
      }
    }
    return '';
  },

  /** Writes a title where it is being read from: parked while the pointer is on it. */
  write(el,text){
    if(el.dataset && el.dataset.tip!=null) el.dataset.tip=text;
    else el.setAttribute('title',text);
  },

  start(){
    if(Tip.el) return;
    const tip=document.createElement('div');
    tip.id='tip';
    tip.setAttribute('role','tooltip');
    document.body.appendChild(tip);
    Tip.el=tip;
    document.addEventListener('mouseover',e=>Tip.look(e));
    document.addEventListener('mouseout',e=>{
      if(!Tip.over) return;
      // Leaving for a child of the same element is not leaving.
      if(e.relatedTarget && Tip.over.contains(e.relatedTarget)) return;
      Tip.leave();
    });
    document.addEventListener('mousemove',e=>{
      if(Tip.over && !Tip.up) Tip.at=[e.clientX,e.clientY];
      /* Round 18bf (§I11): the hit test, which is the only way to see a disabled
         control — Chromium sends no mouse events on one at all, so `mouseover` above
         never hears of it. Not on every move: a few pixels of travel, and never while a
         button is held, which is a drag and not a hover. `elementFromPoint` is a hit
         test on a page with a few thousand nodes, so the throttle is about the paint
         brush rather than about this. */
      if(e.buttons) return;
      const [lx,ly]=Tip.tested||[-1e9,-1e9];
      if(Math.abs(e.clientX-lx)<4 && Math.abs(e.clientY-ly)<4) return;
      Tip.tested=[e.clientX,e.clientY];
      Tip.look(e);
    });
    for(const ev of ['mousedown','wheel','keydown','scroll'])
      document.addEventListener(ev,()=>Tip.hide(),{capture:true,passive:true});
    window.addEventListener('blur',()=>Tip.leave());
  },

  /** What the pointer is over, as the note sees it — and enter or leave accordingly.
   *
   *  One path for both the event and the hit test, so the two cannot come to disagree.
   *  The element under the pointer is asked of the *document* rather than taken from the
   *  event, because a disabled control is under the pointer and is never an event target
   *  (§I11). The event's target is the fallback, for a synthetic event with no useful
   *  coordinates — which is what the suites send. */
  look(e){
    let node=null;
    if(document.elementFromPoint && (e.clientX || e.clientY)){
      node=document.elementFromPoint(e.clientX,e.clientY);
    }
    const el=Tip.titled(node || e.target);
    if(el===Tip.over) return;
    const warm=Tip.warm();
    Tip.leave();
    if(!el) return;
    Tip.enter(el,e,warm);
  },

  /** Round 18bf (§I12): whether a note is up *or was just now*.
   *
   *  `mouseout` fires before `mouseover` and takes the note down, so asking `Tip.up` in
   *  the handler that follows always heard "no" and every note along a row of buttons
   *  waited the full pause. The browser's rule is recency. */
  warm(){
    return Tip.up || (Tip.now()-Tip.downAt) < Tip.WARM;
  },
  now(){
    return (typeof performance==='object' && performance.now)? performance.now() : Date.now();
  },

  /** The nearest ancestor with a title worth showing, or null. */
  titled(node){
    let el=node;
    while(el && el.nodeType===1){
      if(el===Tip.el) return null;
      const t=Tip.read(el);
      if(t) return el;
      /* An empty title of its own is the way of saying "no tooltip here" — and it stops
         the walk, so neither an ancestor's note nor the row's label speaks for it. */
      if(el.hasAttribute('title') || (el.dataset && el.dataset.tip!=null)) return null;
      el=el.parentElement;
    }
    return null;
  },

  /** `quick`: a note was up on the element the pointer came from, so this one comes
   *  almost at once, the way the browser's do. */
  enter(el,e,quick){
    Tip.over=el;
    Tip.at=[e.clientX,e.clientY];
    const t=el.getAttribute('title');
    if(t!=null){ el.dataset.tip=t; el.setAttribute('title',''); }
    clearTimeout(Tip.timer);
    Tip.timer=setTimeout(()=>Tip.show(),quick? 60 : Tip.DELAY);
  },

  leave(){
    const el=Tip.over;
    if(el){
      // Put the title back, unless a script has written a new one meanwhile.
      if(el.dataset.tip!=null){
        if(el.getAttribute('title')==='') el.setAttribute('title',el.dataset.tip);
        delete el.dataset.tip;
      }
    }
    Tip.over=null;
    Tip.hide();
  },

  hide(){
    clearTimeout(Tip.timer);
    Tip.timer=0;
    if(Tip.el) Tip.el.classList.remove('show');
    // When it went down, for the warm window above.
    if(Tip.up) Tip.downAt=Tip.now();
    Tip.up=false;
  },

  show(){
    const el=Tip.over, tip=Tip.el;
    if(!el || !tip || !document.body.contains(el)){ Tip.leave(); return; }
    const text=Tip.read(el);
    if(!text){ Tip.hide(); return; }
    tip.textContent=text;
    tip.classList.add('show');
    Tip.up=true;
    /* Under the pointer, a little below, kept inside the window: flipped above when
       there is no room below, slid left when it would run off the right edge. */
    const [mx,my]=Tip.at||[0,0];
    const W=window.innerWidth, H=window.innerHeight;
    const r=tip.getBoundingClientRect();
    let x=mx+12, y=my+18;
    if(x+r.width>W-8) x=Math.max(8,W-8-r.width);
    if(y+r.height>H-8) y=Math.max(8,my-10-r.height);
    tip.style.left=x+'px';
    tip.style.top=y+'px';
  },
};
if(typeof document!=='undefined' && document.body) Tip.start();
else if(typeof document!=='undefined') document.addEventListener('DOMContentLoaded',()=>Tip.start());
