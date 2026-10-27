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
  playing:null,   // {src, gain, start, buf, box}

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

  /** The record dialog's player, for the types that name a sound file (drawRecord calls it). */
  extendRecord(E, body, v){
    if(v.tag!=='SOUN' && v.tag!=='INFO') return;
    const x=E.dialog && E.dialog().querySelector('#edDlgX');
    if(x && !x.dataset.sound){ x.dataset.sound='1'; x.addEventListener('click', ()=>this.stop()); }
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
    try{ info=await Engine.call('sound_info',{path, n:400}); }
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
    const st={info, path, box, cv, at:0};
    this.drawWave(st, -1);
    box.querySelector('.edSndPlay').onclick=()=>this.play(st, 0);
    box.querySelector('.edSndStop').onclick=()=>this.stop();
    box.querySelector('.edSndVol').oninput=e=>{ if(this.playing && this.playing.st===st) this.playing.gain.gain.value=Number(e.target.value); };
    box.querySelector('.edSndLoop').onchange=e=>{ if(this.playing && this.playing.st===st) this.playing.src.loop=e.target.checked; };
    cv.onclick=e=>{ const r=cv.getBoundingClientRect(); this.play(st, Math.max(0, Math.min(1, (e.clientX-r.left)/r.width))*info.seconds); };
  },

  drawWave(st, pos){
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
    return buf;
  },

  async play(st, from){
    this.stop();
    let buf;
    try{ buf=await this.buffer(st); }catch(e){ return toast(String(e.message||e),'err',5000); }
    const ctx=this.audio();
    const src=ctx.createBufferSource(), gain=ctx.createGain();
    src.buffer=buf;
    src.loop=st.box.querySelector('.edSndLoop').checked;
    gain.gain.value=Number(st.box.querySelector('.edSndVol').value);
    src.connect(gain).connect(ctx.destination);
    src.start(0, from||0);
    const p={src, gain, st, start:ctx.currentTime-(from||0)};
    this.playing=p;
    src.onended=()=>{ if(this.playing===p){ this.playing=null; this.drawWave(st,-1); } };
    const tick=()=>{
      if(this.playing!==p) return;
      let t=ctx.currentTime-p.start;
      if(src.loop) t%=st.info.seconds||1;
      this.drawWave(st, Math.min(t, st.info.seconds));
      requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
  },

  stop(){
    const p=this.playing; if(!p) return;
    this.playing=null;
    try{ p.src.stop(); }catch(_){ }
    this.drawWave(p.st,-1);
  },
};
