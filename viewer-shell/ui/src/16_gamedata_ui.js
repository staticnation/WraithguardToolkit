
/* =====================================================================================
   Pickers, datalists and the "matches nothing" report
   ===================================================================================== */
/* `buildGameDatalists` used to live here, filling `ltexList` and `scopeList` for the
   texture and scope fields. Both fields open a searchable picker from their "…" now
   (round 11 item 4) and wear no datalist — a datalist drew an arrow on the field and
   opened a list nobody could search, which is what Robin kept finding. The one job left
   is telling the cell picker the install has changed. */
function buildGameDatalists(){
  if(typeof fillCellPicker==='function' && App.mode==='cell') fillCellPicker();
}


/* =====================================================================================
   Progress overlay — long passes block interaction anyway, so say so plainly
   ===================================================================================== */
/* =====================================================================================
   What the window is doing, at the right-hand end of the bottom bar.

   Robin: "Make a clear loading bar and message when grass is rescattering [...] Also add
   a message about what status the current state is in (scattering, idling, waiting for
   exit painting mode etc.)."

   Two kinds of thing share it, which is why it is one widget rather than two.

   * **Work**, while something is running. Fed from `Busy` — so every heavy stage that
     already announced itself in the full-screen card also announces itself down here,
     with no call sites to keep in step — and from the scatter loop, which deliberately
     shows no card at all. That last one is the whole point of this existing: rescattering
     is the wait people meet most often and it used to happen with nothing on screen to
     say so.
   * **Rest**, when nothing is. Idle, or painting, or painting with grass owed. A state
     rather than an event, so it is *set* and stays set until something sets it again.

   The two do not fight: work wins while it is running, and clearing it falls back to
   whatever rest state was last set rather than to a blank.
   ===================================================================================== */
const Status={
  standing:{text:'Idle', tone:''},
  running:null,
  /** What the window is doing when nothing is running. */
  rest(text,tone){
    this.standing={text:text||'Idle', tone:tone||''};
    if(!this.running) this.render();
  },
  /** Something is running. `frac` null means "no idea how far", not "nothing yet". */
  work(text,frac){
    this.running={text:text||'Working…', frac};
    this.render();
  },
  done(){ this.running=null; this.render(); },
  render(){
    const msg=$('#vpStatusMsg'), bar=$('#vpStatusBar'), fill=$('#vpStatusFill');
    if(!msg||!bar||!fill) return;
    const r=this.running;
    msg.textContent = r? r.text : this.standing.text;
    msg.className = 'msg '+(r? 'work' : this.standing.tone);
    bar.classList.toggle('on',!!r);
    if(!r) return;
    if(r.frac==null || !isFinite(r.frac)){
      fill.classList.add('indet'); fill.style.width='';
    } else {
      fill.classList.remove('indet');
      fill.style.width=(clamp(r.frac,0,1)*100).toFixed(1)+'%';
    }
  },
};

const Busy={
  depth:0,
  /** Round 18bn: `stop` is a function to call when somebody wants out, or nothing.
   *
   *  Given one, the card grows a Cancel button; given none, it does not — and that is the
   *  right default, because most of what shows this card is a few hundred milliseconds of
   *  reading and a Cancel on it would be a button nobody could hit on purpose. */
  /* ---- Round 18bo: one long job can own the card ---------------------------------
   *
   * Robin, on connecting an install: "I would like to make sure to bridge the time from
   * where I click 'Load these' until everything is loaded, by having the busy card /
   * progress bar show up immediately while the install is loading."
   *
   * The card was a global anybody could write to, which is fine while the only thing
   * showing one is the thing you are watching. A connect is minutes long and the page is
   * full of debounced work that wakes up inside it — a scheduled preview rebuild, a
   * settings save — and each of those shows and hides the card as it pleases. A `hide`
   * from one of them during a connect leaves the window looking idle over a scene that
   * has already been taken down, which is exactly what Robin photographed.
   *
   * So a long job *claims* it. While claimed, `show` and `hide` from anywhere else do
   * nothing, and the card says what the claimant last said. A claim supersedes the one
   * before it, so a leaked claim heals on the next one rather than deadening the card for
   * the rest of the session, and `free()` is there for the `finally` that always runs.
   */
  _owner:0,
  claim(){
    const tag=++this._owner;
    const mine=()=>this._owner===tag;
    return {
      show:(label,sub,frac,stop)=>{ if(mine()) this._paint(label,sub,frac,stop,true); },
      sub:(t)=>{ if(mine()) this._sub(t); },
      /** Hands the card back and takes it down. */
      done:()=>{ if(mine()){ this._owner=0; this.hide(); } },
    };
  },
  /** Drops any claim and takes the card down — for the `finally` of the job that made it. */
  free(){ this._owner=0; this.hide(); },

  show(label,sub,frac,stop){
    if(this._owner) return;          // somebody else's card; see `claim`
    this._paint(label,sub,frac,stop,false);
  },
  /** `overall` covers the whole window rather than the viewport alone. */
  _paint(label,sub,frac,stop,overall){
    // The bottom bar says the same thing without covering the window, and it is the
    // only thing that says it once the card is gone.
    Status.work(sub? label+' — '+sub : label, frac);
    const el=$('#busy'); if(!el) return;
    /* The clock starts where the card does, not where this call does: a job that reports
       progress calls `show` many times and the elapsed time is about the job. */
    if(el.hidden) this._started();
    el.hidden=false;
    /* Round 18bo: over the whole window when a dialogue started the work or a long job
       claimed it, over the viewport alone otherwise. For an unclaimed card the page is
       asked rather than the caller, because the answer is "is there a dialogue open right
       now" and the page is the one that knows — a flag would be one more thing for a new
       call site to get wrong, and the export's bar was invisible behind its own dialogue
       for exactly that kind of reason. Re-decided on every show, so a card that outlives
       the dialogue that started it drops back over the viewport by itself. */
    el.classList.toggle('overall', overall || !!document.querySelector('.modal:not([hidden])'));
    const box=$('#busyStop'), btn=$('#busyStopBtn');
    if(box && btn){
      box.hidden=!stop;
      btn.disabled=false;
      btn.textContent=T('busy.cancel');
      btn.onclick=stop||null;
    }
    $('#busyLbl').textContent=label||'Working…';
    $('#busySub').textContent=sub||'';
    const fill=$('#busyFill'), pct=$('#busyPct');
    if(frac==null||!isFinite(frac)){
      fill.classList.add('indet'); fill.style.width='';
      pct.textContent='';
    } else {
      fill.classList.remove('indet');
      const f=clamp(frac,0,1);
      fill.style.width=(f*100).toFixed(1)+'%';
      pct.textContent=Math.round(f*100)+'%';
    }
  },
  /** Just the second line, without disturbing the bar or the button. */
  sub(text){ if(this._owner) return; this._sub(text); },
  _sub(text){
    const el=$('#busySub'); if(el) el.textContent=text||'';
  },

  /* ---- the clock (round 18bo) ------------------------------------------------------
   *
   * Robin: "a timer counting up. First seconds only (format 00s, then when it reached a
   * minute, I want the format to be 0m 00s and so on) so there is feedback on how long the
   * process has been going on."
   *
   * It runs for every card, not only the export's, because the question it answers — how
   * long have I been waiting — is the same one a long install load raises. It stays blank
   * for the first second, so the hundreds of cards that are up for a few hundred
   * milliseconds never flash a `00s` at you; a card only grows a clock once it has been
   * there long enough for the answer to be worth having.
   */
  _t0:0, _clock:0,
  _started(){
    this._t0=Date.now();
    clearInterval(this._clock);
    const el=$('#busyTime'); if(el) el.textContent='';
    /* Round 18bo. Robin: "Pressing export .esp again after having exported it, opens the
       progress bar already filled for a little while before it's reset to the correct
       state." The bar keeps the width the last run left it at, and `.fill` carries a
       180 ms width transition — so a fresh card at 0% *animated down* from the previous
       run's 100%, which reads as a bar that was already nearly done and then thought
       better of it. Emptied here, on the hidden-to-visible edge, with the transition
       switched off for the one frame it takes to commit: a new card starts empty, and
       every move after it still slides. */
    const fill=$('#busyFill');
    if(fill){
      fill.classList.remove('indet');
      fill.style.transition='none';
      fill.style.width='0%';
      void fill.offsetWidth;                 // commit it before the transition comes back
      fill.style.transition='';
    }
    const pct=$('#busyPct'); if(pct) pct.textContent='';
    // A quarter of a second, so the seconds tick over promptly rather than up to a second
    // late; the text only changes when the whole seconds do.
    this._clock=setInterval(()=>this._tock(),250);
  },
  _tock(){
    const el=$('#busyTime'); if(!el) return;
    const ms=Date.now()-this._t0;
    el.textContent = ms<1000? '' : Busy.elapsed(ms);
  },
  /** `07s`, then `1m 07s`, then `1h 01m 07s`. Digits only, so it needs no translating. */
  elapsed(ms){
    const two=n=>String(n).padStart(2,'0');
    const s=Math.floor(Math.max(0,ms)/1000);
    if(s<60) return two(s)+'s';
    const m=Math.floor(s/60);
    if(m<60) return m+'m '+two(s%60)+'s';
    return Math.floor(m/60)+'h '+two(m%60)+'m '+two(s%60)+'s';
  },

  hide(){
    if(this._owner) return;          // somebody else's card; see `claim`
    clearInterval(this._clock); this._clock=0;
    const tm=$('#busyTime'); if(tm) tm.textContent='';
    const el=$('#busy'); if(el){ el.hidden=true; el.classList.remove('overall'); }
    // The button belongs to the work that is ending; left wired, the next card to come
    // up without a `stop` would still carry the last one's.
    const box=$('#busyStop'), btn=$('#busyStopBtn');
    if(box) box.hidden=true;
    if(btn) btn.onclick=null;
    Status.done();
  },
  /** Let the browser paint between heavy steps.
   *
   *  Raced against a timer, and that is not belt-and-braces — it is the difference
   *  between a pause and a stall. `requestAnimationFrame` only fires when frames are
   *  actually being produced, and a page that is composited lazily can go seconds
   *  without one. A rebuild waiting on a frame it will not get stops half-built, with
   *  the terrain loaded and nothing drawn from it, which is precisely the symptom this
   *  round is about — so the one thing this must never do is block.
   *
   *  50 ms is longer than a frame at 60 Hz, so on a page that *is* animating the frame
   *  wins every time and the behaviour is unchanged. */
  async tick(){
    await new Promise(r=>{
      let done=false;
      const go=()=>{ if(done) return; done=true; r(); };
      requestAnimationFrame(go);
      setTimeout(go,50);
    });
  }
};
