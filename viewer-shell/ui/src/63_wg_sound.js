/* =====================================================================================
   Sounds - Wraithguard (SOUND.md, step 1: hear a file). A player in the Editor's record
   dialog for a Sound record (SOUN) and a dialogue response's voice file (INFO): what the
   file is, its waveform, play / stop / loop / volume.

   The engine decodes (`sound_info`, `sound_pcm`: viewcore::audio, Symphonia) and this page
   plays with Web Audio: the samples go into an AudioBuffer here, so what plays does not
   depend on which codecs the machine's webview has. A record's sound path is relative to
   the data folders' `Sound/`, as the game reads it; a `.wav` nothing supplies is played as
   its `.mp3`, as OpenMW does.

   Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */

const WgSound={
  ctx:null,
  playing:null,   // {src, gain, st, start, btn?}
  /* What a path has read, by path, the newest last: {info, buf}. A record dialog opened
     twice, or a field's play button pressed again, reads nothing the second time. It
     belongs to the install: `forget` (on a switch, beside the meshes) empties it, and the
     record dialog's own player still asks once per opening, so a file changed on disk is
     heard the next time its record is opened. */
  cache:new Map(),
  CACHE_MAX:48,

  audio(){
    if(!this.ctx){
      const A=window.AudioContext||window.webkitAudioContext;
      if(!A) throw new Error('This webview has no Web Audio');
      this.ctx=new A();
    }
    if(this.ctx.state==='suspended') this.ctx.resume();
    return this.ctx;
  },

  /** A record field's sound path as the data folders hold it. */
  dataPath(p){
    const s=String(p||'').replace(/\\/g,'/').replace(/^\/+/,'');
    return /^sound\//i.test(s) ? s : 'Sound/'+s;
  },

  /** Drop what was read (another install, another set of data folders). */
  forget(){ this.stop(); this.cache.clear(); this.place.failed.clear(); this._musicList=null; this._fb=null; this._fbLow=null; this.musicStop(); this.weatherOff(true); this.place.region=''; this.place.weather=''; for(const k of [...this.place.voices.keys()]) this.voiceOff(k, true); this.place.scene=null; this.place.emit=[]; },

  /** One path's facts and waveform, read once (`fresh` asks again). */
  async infoOf(path, fresh){
    const k=path.toLowerCase();
    let c=this.cache.get(k);
    if(c && !fresh){ this.cache.delete(k); this.cache.set(k, c); return c; }
    const info=await Engine.call('sound_info',{path, n:400});
    c={info, buf:null};
    this.cache.delete(k); this.cache.set(k, c);
    while(this.cache.size>this.CACHE_MAX) this.cache.delete(this.cache.keys().next().value);
    return c;
  },

  /** The fields that name a Sound record: a door's, a light's, a magic effect's, a sound
   *  generator's, a region's (`sounds.N.0`), as editcore::verify checks them. */
  namesSound(E, tag, path){
    if(tag==='REGN') return /^sounds\.\d+\.0$/.test(path);
    const t=E.idTags && E.idTags(path);
    return !!(t && t.includes('SOUN'));
  },

  /** A Sound record's file and volume, as the patch has it now. */
  async soundRecord(E, id){
    const v=await E.ask('editRecord', {tag:'SOUN', id});
    const get=p=>{ const f=(v && v.fields||[]).find(x=>x.path===p); return f? (('queued' in f)? f.queued : f.value) : null; };
    const file=get('sound_path'), vol=Number(get('data.volume'));
    if(!file) throw new Error('The sound '+id+' names no file');
    return {path:this.dataPath(file), gain:Number.isFinite(vol)? vol/255 : 1};
  },

  /** A small play button beside a field: what the field holds when it is pressed. */
  fieldButton(el, resolve){
    if(el.dataset.sndBtn) return;
    el.dataset.sndBtn='1';
    const b=document.createElement('button');
    b.type='button'; b.className='btn sm dim edSndQuick'; b.textContent='▶';
    b.title='Play it (again: stop)';
    b.style.marginLeft='4px';
    b.onclick=async e=>{
      e.preventDefault(); e.stopPropagation();
      const val=String(el.value||'').trim();
      if(!val) return toast('The field is empty','warn',2000);
      try{ const r=await resolve(val); await this.quick(b, r.path, r.gain); }
      catch(err){ toast(String(err && err.message || err),'err',4000); }
    };
    el.insertAdjacentElement('afterend', b);
    return b;
  },

  /** Play a file from a button, without the full player: the button shows ■ while it plays. */
  async quick(btn, path, gain){
    if(this.playing && this.playing.btn===btn){ this.stop(); return; }
    const c=await this.infoOf(path);
    const st={info:c.info, path, cache:c, box:null, cv:null};
    const i=c.info;
    btn.title=i.path+' · '+i.seconds.toFixed(2)+' s'+(i.from && i.from[0]? ' · from '+i.from[0] : '')+' (again: stop)';
    await this.play(st, 0, {gain, btn});
  },

  /** The record dialog's player, for the types that name a sound file (drawRecord calls it),
   *  and a play button on every field naming a sound. */
  extendRecord(E, body, v){
    const x=E.dialog && E.dialog().querySelector('#edDlgX');
    if(x && !x.dataset.sound){ x.dataset.sound='1'; x.addEventListener('click', ()=>this.stop()); }
    body.querySelectorAll('input[data-path]').forEach(el=>{
      const p=el.dataset.path;
      if(p==='sound_path' && (v.tag==='SOUN' || v.tag==='INFO')) this.fieldButton(el, val=>({path:this.dataPath(val), gain:1}));
      else if(this.namesSound(E, v.tag, p)) this.fieldButton(el, id=>this.soundRecord(E, id));
    });
    if(v.tag!=='SOUN' && v.tag!=='INFO') return;
    const f=(v.fields||[]).find(x=>x.path==='sound_path');
    const val=f? (('queued' in f)? f.queued : f.value) : '';
    if(!val) return;
    const vol=v.tag==='SOUN'? (v.fields.find(x=>x.path==='data.volume')||{}) : {};
    const volume=Number(('queued' in vol)? vol.queued : vol.value);
    const box=document.createElement('details');
    box.className='edSound'; box.open=true;
    body.insertBefore(box, body.querySelector('details.edAllFields')||body.querySelector('.edSecBar')||null);
    this.player(box, this.dataPath(val), Number.isFinite(volume)? volume/255 : 1);
  },

  /** A player for one file into `box`: facts, waveform, controls. */
  async player(box, path, gain){
    box.innerHTML='<summary>Sound</summary><div class="hint">Reading '+escHtml(path)+'...</div>';
    let info;
    let c;
    try{ c=await this.infoOf(path, true); info=c.info; }
    catch(e){ box.innerHTML='<summary>Sound</summary><div class="hint">'+escHtml(String(e.message||e))+'</div>'; return; }
    const db=x=> x>0? (20*Math.log10(x)).toFixed(1)+' dB' : 'silent';
    const facts=[info.container.toUpperCase()+(info.codec? ' ('+info.codec+')' : ''), info.rate+' Hz',
      info.channels===1? 'mono' : info.channels===2? 'stereo' : info.channels+' channels', (info.bits? info.bits+'-bit, ' : '')+info.seconds.toFixed(2)+' s',
      'peak '+db(info.peak)+', RMS '+db(info.rms)];
    const from=(info.from||[]);
    box.innerHTML='<summary>Sound</summary>'+
      '<div class="hint" style="font-family:var(--mono)">'+escHtml(info.path)+'</div>'+
      '<div class="hint">'+escHtml(facts.join(' · '))+(from.length? '<br>From '+escHtml(from[0])+(from.length>1? ' (over '+escHtml(from.slice(1).join(', '))+')' : '') : '')+'</div>'+
      (info.peak>=0.999? '<div class="hint" style="color:var(--warn)">It reaches full scale: it may clip.</div>' : '')+
      (info.peak===0? '<div class="hint" style="color:var(--warn)">It is silent.</div>' : '')+
      '<canvas class="edWave" width="400" height="56" style="width:100%;height:56px;display:block;cursor:pointer;background:var(--bg3);border:1px solid var(--line)"></canvas>'+
      '<div class="orirow"><button class="btn sm edSndPlay">Play</button> <button class="btn sm dim edSndStop">Stop</button> '+
      '<label><input type="checkbox" class="edSndLoop"> Loop</label> '+
      '<label>Volume <input type="range" class="edSndVol" min="0" max="1" step="0.01" value="'+Math.min(1,Math.max(0,gain))+'" style="width:90px"></label></div>';
    const cv=box.querySelector('.edWave');
    const st={info, path, box, cv, at:0, cache:c};
    this.drawWave(st, -1);
    box.querySelector('.edSndPlay').onclick=()=>this.play(st, 0);
    box.querySelector('.edSndStop').onclick=()=>this.stop();
    box.querySelector('.edSndVol').oninput=e=>{ if(this.playing && this.playing.st===st) this.playing.gain.gain.value=Number(e.target.value); };
    box.querySelector('.edSndLoop').onchange=e=>{ if(this.playing && this.playing.st===st) this.playing.src.loop=e.target.checked; };
    cv.onclick=e=>{ const r=cv.getBoundingClientRect(); this.play(st, Math.max(0, Math.min(1, (e.clientX-r.left)/r.width))*info.seconds); };
  },

  drawWave(st, pos){
    if(!st.cv) return;
    const cv=st.cv, g=cv.getContext('2d'), w=cv.width, h=cv.height, p=st.info.peaks||[];
    const css=getComputedStyle(document.body);
    g.clearRect(0,0,w,h);
    g.fillStyle=css.getPropertyValue('--ac').trim()||'#7aa2f7';
    for(let i=0;i<p.length;i++){
      const x=Math.floor(i*w/p.length), x2=Math.max(x+1, Math.floor((i+1)*w/p.length));
      const y1=h/2-p[i][1]*h/2, y2=h/2-p[i][0]*h/2;
      g.fillRect(x, y1, x2-x, Math.max(1, y2-y1));
    }
    if(pos>=0){
      g.fillStyle=css.getPropertyValue('--tx').trim()||'#fff';
      g.fillRect(Math.floor(pos/st.info.seconds*w), 0, 1, h);
    }
  },

  async buffer(st){
    if(st.buf) return st.buf;
    if(st.cache && st.cache.buf) return (st.buf=st.cache.buf);
    const ab=await Engine.bytes('sound_pcm',{path:st.path});
    const dv=new DataView(ab);
    if(new TextDecoder().decode(new Uint8Array(ab,0,4))!=='WGS1') throw new Error('The engine\'s sound did not read');
    const hl=dv.getUint32(4,true);
    const head=JSON.parse(new TextDecoder().decode(new Uint8Array(ab,8,hl)));
    const ctx=this.audio(), n=head.frames, ch=head.channels;
    const buf=ctx.createBuffer(ch, Math.max(1,n), head.rate);
    let o=8+hl;
    const chans=[]; for(let c=0;c<ch;c++) chans.push(buf.getChannelData(c));
    for(let i=0;i<n;i++) for(let c=0;c<ch;c++){ chans[c][i]=dv.getInt16(o,true)/32768; o+=2; }
    st.buf=buf;
    if(st.cache) st.cache.buf=buf;
    return buf;
  },

  /** Play `st` from `from` seconds: the player's own controls, or `o` ({gain, loop, btn}). */
  async play(st, from, o){
    this.stop();
    o=o||{};
    let buf;
    try{ buf=await this.buffer(st); }catch(e){ return toast(String(e.message||e),'err',5000); }
    const ctx=this.audio();
    const src=ctx.createBufferSource(), gain=ctx.createGain();
    src.buffer=buf;
    src.loop=st.box? st.box.querySelector('.edSndLoop').checked : !!o.loop;
    gain.gain.value=st.box? Number(st.box.querySelector('.edSndVol').value) : Math.min(1, Math.max(0, o.gain==null? 1 : o.gain));
    src.connect(gain).connect(ctx.destination);
    src.start(0, from||0);
    const p={src, gain, st, start:ctx.currentTime-(from||0), btn:o.btn||null};
    this.playing=p;
    if(p.btn) p.btn.textContent='■';
    src.onended=()=>{ if(this.playing===p){ this.playing=null; this.drawWave(st,-1); if(p.btn) p.btn.textContent='▶'; } };
    const tick=()=>{
      if(this.playing!==p) return;
      let t=ctx.currentTime-p.start;
      if(src.loop) t%=st.info.seconds||1;
      this.drawWave(st, Math.min(t, st.info.seconds));
      requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
  },

  /* ---- Sounds in place (SOUND.md step 2) -------------------------------------------
     The scene's lights that loop a sound, each heard from where it stands: a looping
     buffer through a PannerNode at the object, the AudioListener on the camera. Volume and
     ranges are OpenMW's (`sound_emitters`: soundbuffer.cpp's formula and the fAudio*
     settings); the distance model is the inverse one OpenAL's clamped model is, and a sound
     past its maximum range is stopped, as OpenMW culls it. At most BUDGET play, the
     nearest first, each faded in and out so a voice changing hands does not click. Off by
     default (Preview's "Sounds"). */
  place:{on:false, vol:0.7, scene:null, emit:[], voices:new Map(), raf:0, tick:0, master:null, failed:new Set(), asked:0,
         fx:null, fxVol:1, mus:null, musVol:0.5, musicOn:false, scripts:true,
         region:'', regionList:null, regionAsk:'', regionWait:0, regionNext:0,
         weather:'', weatherVoices:[], thunderLast:0,
         music:null},   // {src, file, list, asked}
  BUDGET:16,
  FADE:0.25,

  placeOn(on){
    const P=this.place;
    P.on=!!on;
    if(!P.on){ this.placeStop(); return; }
    if(!P.raf) P.raf=requestAnimationFrame(()=>this.placeFrame());
  },

  placeVolume(v){
    const P=this.place; P.vol=Math.max(0, Math.min(1, v));
    if(P.master) P.master.gain.value=P.vol;
  },
  /** Settings' gains under the master (OpenMW's "effects" and "music" volumes). */
  placeEffects(v){ const P=this.place; P.fxVol=Math.max(0, Math.min(1, v)); if(P.fx) P.fx.gain.value=P.fxVol; },
  placeMusicVolume(v){ const P=this.place; P.musVol=Math.max(0, Math.min(1, v)); if(P.mus) P.mus.gain.value=P.musVol; },
  placeMusic(on){ const P=this.place; P.musicOn=!!on; if(!P.musicOn) this.musicStop(); },
  /** Whether the loops objects' scripts start play. A script's loop is played whatever the
   *  script's conditions (a quest stage, a time of day, a switch thrown): off, only the
   *  loops that need no script (the lights') are heard. */
  placeScripts(on){
    const P=this.place; P.scripts=!!on;
    if(!P.scripts) for(const e of P.emit) if(e.d.via==='script' && P.voices.has(e.key)) this.voiceOff(e.key);
  },

  /** The graph: master <- effects (objects, region, weather), master <- music. */
  placeGraph(ctx){
    const P=this.place;
    if(P.master) return;
    P.master=ctx.createGain(); P.master.gain.value=P.vol; P.master.connect(ctx.destination);
    P.fx=ctx.createGain(); P.fx.gain.value=P.fxVol; P.fx.connect(P.master);
    P.mus=ctx.createGain(); P.mus.gain.value=P.musVol; P.mus.connect(P.master);
  },

  placeStop(){
    const P=this.place;
    if(P.raf) cancelAnimationFrame(P.raf);
    P.raf=0;
    for(const k of [...P.voices.keys()]) this.voiceOff(k, true);
    P.scene=null; P.emit=[];
    this.weatherOff(true); this.musicStop();
    P.region=''; P.regionList=null;
  },

  /** The scene's emitters: every light reference whose record loops a sound. */
  async placeGather(scene){
    const P=this.place, ask=++P.asked;
    const ids=new Set();
    for(const c of scene.cells||[]) for(const r of c.refs||[]) if(r && r.id && !r.gc) ids.add(String(r.id).toLowerCase());
    let rows=[];
    if(ids.size){
      try{ rows=await Engine.call('sound_emitters',{ids:[...ids]}); }
      catch(e){ if(typeof WgLog==='object') WgLog.add('The scene\'s sounds did not load: '+String(e && e.message || e), 'warn'); rows=[]; }
    }
    if(ask!==P.asked || P.scene!==scene) return;   // another scene since
    const by=new Map((Array.isArray(rows)? rows : []).map(r=>[r.id, r]));
    const o=scene.origin||[0,0];
    const emit=[];
    for(const c of scene.cells||[]) for(const r of c.refs||[]){
      const d=r && r.id && by.get(String(r.id).toLowerCase());
      if(!d || !r.pos) continue;
      emit.push({key:(c.gx+','+c.gy+','+(c.name||''))+'/'+emit.length, x:r.pos[0]-o[0], y:r.pos[1]-o[1], z:r.pos[2], d});
    }
    for(const k of [...P.voices.keys()]) this.voiceOff(k, true);
    P.emit=emit;
  },

  placeFrame(){
    const P=this.place;
    P.raf=0;
    if(!P.on) return;
    P.raf=requestAnimationFrame(()=>this.placeFrame());
    const sc=App._scene||null;
    if(sc!==P.scene){ P.scene=sc; P.emit=[]; for(const k of [...P.voices.keys()]) this.voiceOff(k, true); if(sc) this.placeGather(sc); }
    const R=App.R;
    if(!R || !R.cam || !sc || document.hidden){ if(document.hidden){ for(const k of [...P.voices.keys()]) this.voiceOff(k); this.weatherOff(); } return; }
    let ctx; try{ ctx=this.audio(); }catch(_){ return; }
    this.placeGraph(ctx);
    const c=R.cam, ce=Math.cos(c.el), se=Math.sin(c.el);
    const eye=[c.tx+c.dist*ce*Math.cos(c.az), c.ty+c.dist*ce*Math.sin(c.az), c.tz+c.dist*se];
    let f=[c.tx-eye[0], c.ty-eye[1], c.tz-eye[2]];
    const fl=Math.hypot(f[0],f[1],f[2])||1; f=f.map(v=>v/fl);
    const L=ctx.listener, t=ctx.currentTime;
    if(L.positionX){
      L.positionX.setTargetAtTime(eye[0], t, 0.03); L.positionY.setTargetAtTime(eye[1], t, 0.03); L.positionZ.setTargetAtTime(eye[2], t, 0.03);
      L.forwardX.value=f[0]; L.forwardY.value=f[1]; L.forwardZ.value=f[2];
      L.upX.value=0; L.upY.value=0; L.upZ.value=1;
    }else{ L.setPosition(eye[0], eye[1], eye[2]); L.setOrientation(f[0], f[1], f[2], 0, 0, 1); }
    // Who plays: chosen ten times a second, the nearest within range.
    if(++P.tick%6) return;
    this.worldSound(sc, eye);
    const near=[];
    for(const e of P.emit){
      const dd=Math.hypot(e.x-eye[0], e.y-eye[1], e.z-eye[2]);
      if(e.d.via==='script' && !P.scripts) continue;
      if(dd<=e.d.max && !P.failed.has(e.d.file.toLowerCase())) near.push([dd, e]);
    }
    near.sort((a,b)=>a[0]-b[0]);
    const want=new Set(near.slice(0, this.BUDGET).map(x=>x[1].key));
    for(const k of [...P.voices.keys()]) if(!want.has(k)) this.voiceOff(k);
    for(const [, e] of near.slice(0, this.BUDGET)) if(!P.voices.has(e.key)) this.voiceOn(e);
  },

  /* ---- The world's sound (SOUND.md step 3) -------------------------------------------
     Outdoors only, as OpenMW plays them, by the load order's own settings
     (`sound_fallbacks`: openmw.cfg's fallback= lines or Morrowind.ini's sections; the
     values below are Morrowind.ini's, used only where the setup has none).
     - A region's ambient sounds: every Weather_Minimum to _Maximum_Time_Between_
       Environmental_Sounds seconds the region's list is walked in order and the first whose
       chance comes up (a roll of 0-99 under it) plays once, flat, at its record's volume
       (OpenMW's RegionSoundSelector).
     - The weather's loops while Preview's weather is that weather: its
       Weather_<name>_Ambient_Loop_Sound_ID, and when it uses precipitation
       (Weather_<name>_Using_Precip) its _Rain_Loop_Sound_ID ("rain" when empty); "None" is
       none (OpenMW's Weather).
     - Thunder, in a weather whose Weather_<name>_Thunder_Frequency is above 0: each tenth
       of a second a strike with chance frequency * 10 / 60 * seconds (the weather fully in,
       so past its _Thunder_Threshold), one of _Thunder_Sound_ID_0..3 picked at random, 0
       the nearest - and the sky flashes by 1, 0.75, 0.5 or 0.25, fading at
       _Flash_Decrement a second (OpenMW's calculateThunder / lightningAndThunder).
     - Music: Music/Explore's files in a shuffled order, one after another (there is no
       fight to switch to Battle). */
  FALLBACK:{
    Weather_Minimum_Time_Between_Environmental_Sounds:'1.0', Weather_Maximum_Time_Between_Environmental_Sounds:'5.0',
    Weather_Rain_Using_Precip:'1', Weather_Rain_Rain_Loop_Sound_ID:'Rain', Weather_Rain_Ambient_Loop_Sound_ID:'None',
    Weather_Thunderstorm_Using_Precip:'1', Weather_Thunderstorm_Rain_Loop_Sound_ID:'rain heavy', Weather_Thunderstorm_Ambient_Loop_Sound_ID:'None',
    Weather_Thunderstorm_Thunder_Frequency:'.4', Weather_Thunderstorm_Thunder_Threshold:'0.6', Weather_Thunderstorm_Flash_Decrement:'4',
    Weather_Thunderstorm_Thunder_Sound_ID_0:'Thunder0', Weather_Thunderstorm_Thunder_Sound_ID_1:'Thunder3',
    Weather_Thunderstorm_Thunder_Sound_ID_2:'Thunder2', Weather_Thunderstorm_Thunder_Sound_ID_3:'Thunder1',
    Weather_Ashstorm_Ambient_Loop_Sound_ID:'ashstorm', Weather_Blight_Ambient_Loop_Sound_ID:'Blight',
    Weather_Blizzard_Ambient_Loop_Sound_ID:'BM Blizzard', Weather_Snow_Ambient_Loop_Sound_ID:'None',
  },

  /** One setting: the setup's (keys compared without case), else Morrowind.ini's. */
  fb(key){
    const F=this._fb||{};
    const k=key.toLowerCase();
    if(!this._fbLow){ this._fbLow={}; for(const [x,v] of Object.entries(F)) this._fbLow[x.toLowerCase()]=v; }
    if(k in this._fbLow) return String(this._fbLow[k]);
    const d=Object.keys(this.FALLBACK).find(x=>x.toLowerCase()===k);
    return d? this.FALLBACK[d] : '';
  },
  fbNum(key, def){ const v=parseFloat(this.fb(key)); return isFinite(v)? v : def; },
  /** A setting naming a Sound: '' for none ("None", empty). */
  fbSound(key){ const v=this.fb(key).trim(); return !v || v.toLowerCase()==='none'? '' : v; },

  loadFallbacks(){
    if(this._fb || this._fbAsk) return;
    this._fbAsk=Engine.call('sound_fallbacks',{})
      .then(f=>{ this._fb=(f && typeof f==='object')? f : {}; this._fbLow=null; })
      .catch(()=>{ this._fb={}; this._fbLow=null; })
      .finally(()=>{ this._fbAsk=null; });
  },

  /** The exterior cell the eye is over, or null indoors. */
  cellUnder(sc, eye){
    if(!sc || !sc.centre || sc.centre.kind==='int') return null;
    const o=sc.origin||[0,0], CELLW=8192;
    const gx=Math.floor((eye[0]+o[0])/CELLW), gy=Math.floor((eye[1]+o[1])/CELLW);
    return (sc.cells||[]).find(c=>c && c.kind!=='int' && c.gx===gx && c.gy===gy) || sc.centre;
  },

  /** A weather's loops, as Sound ids. */
  weatherLoops(w){
    const out=[];
    const amb=this.fbSound('Weather_'+w+'_Ambient_Loop_Sound_ID'); if(amb) out.push(amb);
    if(this.fbNum('Weather_'+w+'_Using_Precip', 0)){
      const raw=this.fb('Weather_'+w+'_Rain_Loop_Sound_ID').trim();
      const rain=!raw? 'rain' : raw.toLowerCase()==='none'? '' : raw;
      if(rain && !out.some(x=>x.toLowerCase()===rain.toLowerCase())) out.push(rain);
    }
    return out;
  },

  worldSound(sc, eye){
    const P=this.place, cell=this.cellUnder(sc, eye);
    const now=performance.now()/1000;
    this.loadFallbacks();
    if(!this._fb) return;   // the settings first
    // The region's.
    const reg=cell? String(cell.region||'').toLowerCase() : '';
    const tmin=this.fbNum('Weather_Minimum_Time_Between_Environmental_Sounds', 1);
    const tmax=Math.max(tmin, this.fbNum('Weather_Maximum_Time_Between_Environmental_Sounds', 5));
    if(reg!==P.region){
      P.region=reg; P.regionList=null; P.regionNext=now+tmin;
      if(reg){
        P.regionAsk=reg;
        Engine.call('region_sounds',{region:reg}).then(l=>{ if(P.regionAsk===reg) P.regionList=Array.isArray(l)? l : []; })
          .catch(()=>{ if(P.regionAsk===reg) P.regionList=[]; });
      }
    }
    if(reg && P.regionList && P.regionList.length && now>=P.regionNext){
      P.regionNext=now+tmin+Math.random()*(tmax-tmin);
      const hit=P.regionList.find(x=>Math.floor(Math.random()*100)<x.chance);
      if(hit && !P.failed.has(hit.file.toLowerCase())) this.oneShot(hit);
    }
    // The weather's loops.
    const wx=cell && typeof Sky==='object'? String(Sky.weather||'Clear') : '';
    const loops=wx? this.weatherLoops(wx) : [];
    const sig=wx+'|'+loops.join('|');
    if(sig!==P.weather){ P.weather=sig; this.weatherOff(); for(const id of loops) this.weatherOn(id, sig); }
    // Thunder.
    const freq=wx? this.fbNum('Weather_'+wx+'_Thunder_Frequency', 0) : 0;
    const dt=Math.min(0.5, Math.max(0, now-(P.thunderLast||now))); P.thunderLast=now;
    if(freq>0 && Math.random()<=freq*10/60*dt) this.thunder(wx);
    // Music.
    if(P.musicOn && !P.music) this.musicNext();
  },

  async thunder(wx){
    const P=this.place;
    const n=Math.floor(Math.random()*4);
    if(App.R && App.R.lightning) App.R.lightning(1-n*0.25, this.fbNum('Weather_'+wx+'_Flash_Decrement', 4));
    const id=this.fbSound('Weather_'+wx+'_Thunder_Sound_ID_'+n);
    if(!id) return;
    try{
      const rows=await Engine.call('sound_records',{ids:[id]});
      const d=Array.isArray(rows)? rows[0] : null;
      if(d && !P.failed.has(d.file.toLowerCase())) this.oneShot(d);
    }catch(_){ }
  },

  async playable(d){
    const path=this.dataPath(d.file);
    const c=await this.infoOf(path);
    return this.buffer({path, cache:c, info:c.info});
  },

  async oneShot(d){
    const P=this.place, ctx=this.audio();
    let buf;
    try{ buf=await this.playable(d); }
    catch(err){ P.failed.add(d.file.toLowerCase()); return; }
    if(!P.on || !P.fx) return;
    const src=ctx.createBufferSource(), g=ctx.createGain();
    src.buffer=buf; g.gain.value=Math.min(1, d.gain);
    src.connect(g).connect(P.fx);
    src.start();
  },

  async weatherOn(id, sig){
    const P=this.place, ctx=this.audio();
    let d, buf;
    try{
      const rows=await Engine.call('sound_records',{ids:[id]});
      d=Array.isArray(rows)? rows[0] : null;
      if(!d){ if(typeof WgLog==='object') WgLog.add('The weather\'s sound '+id+' is not in the load order', 'warn'); return; }
      buf=await this.playable(d);
    }catch(err){ if(typeof WgLog==='object') WgLog.add('The weather\'s sound '+id+' did not play: '+String(err && err.message || err), 'warn'); return; }
    if(P.weather!==sig || !P.on || !P.fx) return;
    const src=ctx.createBufferSource(), g=ctx.createGain(), t=ctx.currentTime;
    src.buffer=buf; src.loop=true;
    g.gain.setValueAtTime(0, t); g.gain.linearRampToValueAtTime(Math.min(1, d.gain), t+1.5);
    src.connect(g).connect(P.fx);
    src.start(0, Math.random()*(buf.duration||0));
    P.weatherVoices.push({src, g});
  },

  weatherOff(now){
    const P=this.place, list=P.weatherVoices||[];
    P.weatherVoices=[];
    for(const v of list){
      try{ const t=this.ctx.currentTime, end=now? t : t+1.5;
        v.g.gain.cancelScheduledValues(t); v.g.gain.setValueAtTime(v.g.gain.value, t); v.g.gain.linearRampToValueAtTime(0, end+0.01); v.src.stop(end+0.02); }catch(_){ }
    }
  },

  async musicNext(){
    const P=this.place, ctx=this.audio();
    const m=P.music={src:null, file:'', asked:true};
    try{
      if(!this._musicList){ const l=await Engine.call('music_list',{kind:'explore'}); this._musicList=Array.isArray(l)? l : []; this._musicAt=0; }
      const L=this._musicList;
      if(!L.length){ if(typeof WgLog==='object') WgLog.add('No music: nothing in the data folders\' Music/Explore', 'warn'); P.musicOn=false; return; }
      if(this._musicAt%L.length===0){ for(let i=L.length-1;i>0;i--){ const j=Math.floor(Math.random()*(i+1)); [L[i],L[j]]=[L[j],L[i]]; } }
      m.file=L[this._musicAt++%L.length];
      // Not kept in the cache: a track is tens of megabytes as samples.
      const buf=await this.buffer({path:m.file, cache:null, info:null});
      if(P.music!==m || !P.musicOn || !P.mus) return;
      const src=ctx.createBufferSource(); src.buffer=buf;
      src.connect(P.mus); src.start();
      m.src=src;
      src.onended=()=>{ if(P.music===m){ P.music=null; } };
    }catch(err){
      if(typeof WgLog==='object') WgLog.add('The music '+(m.file||'')+' did not play: '+String(err && err.message || err), 'warn');
      if(P.music===m) setTimeout(()=>{ if(P.music===m) P.music=null; }, 3000);   // the next one, in a moment
    }
  },

  musicStop(){
    const P=this.place, m=P.music; P.music=null;
    if(m && m.src) try{ m.src.onended=null; m.src.stop(); }catch(_){ }
  },

  async voiceOn(e){
    const P=this.place, ctx=this.audio();
    const v={e, src:null, gain:null, panner:null, dead:false};
    P.voices.set(e.key, v);
    const path=this.dataPath(e.d.file);
    let buf;
    try{ buf=await this.playable(e.d); }catch(err){
      P.failed.add(e.d.file.toLowerCase());
      if(typeof WgLog==='object') WgLog.add('The sound '+e.d.sound+' ('+path+') did not play: '+String(err && err.message || err), 'warn');
      if(P.voices.get(e.key)===v) P.voices.delete(e.key);
      return;
    }
    if(v.dead || P.voices.get(e.key)!==v || !P.fx) return;
    const src=ctx.createBufferSource(); src.buffer=buf; src.loop=true;
    const panner=ctx.createPanner();
    panner.panningModel='HRTF'; panner.distanceModel='inverse';
    panner.refDistance=e.d.min; panner.maxDistance=e.d.max; panner.rolloffFactor=1;
    if(panner.positionX){ panner.positionX.value=e.x; panner.positionY.value=e.y; panner.positionZ.value=e.z; }
    else panner.setPosition(e.x, e.y, e.z);
    const gain=ctx.createGain();
    const t=ctx.currentTime;
    gain.gain.setValueAtTime(0, t); gain.gain.linearRampToValueAtTime(Math.min(1, e.d.gain), t+this.FADE);
    src.connect(gain).connect(panner).connect(P.fx);
    // Each loop starts somewhere of its own: twenty braziers do not crackle in step.
    src.start(0, Math.random()*(buf.duration||0));
    Object.assign(v, {src, gain, panner});
  },

  voiceOff(key, now){
    const P=this.place, v=P.voices.get(key); if(!v) return;
    P.voices.delete(key); v.dead=true;
    if(!v.src) return;
    try{
      const t=this.ctx.currentTime, end=now? t : t+this.FADE;
      v.gain.gain.cancelScheduledValues(t); v.gain.gain.setValueAtTime(v.gain.gain.value, t); v.gain.gain.linearRampToValueAtTime(0, end+0.01);
      v.src.stop(end+0.02);
      v.src.onended=()=>{ try{ v.panner.disconnect(); }catch(_){ } };
    }catch(_){ }
  },

  stop(){
    const p=this.playing; if(!p) return;
    this.playing=null;
    try{ p.src.stop(); }catch(_){ }
    if(p.btn) p.btn.textContent='▶';
    this.drawWave(p.st,-1);
  },
};
