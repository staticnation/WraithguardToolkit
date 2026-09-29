/* =====================================================================================
   The game's sky — round 13.

   Robin sent four in-game shots of one spot (6:00, noon, 18:00, midnight) beside
   Gardenfell's, and G7's branch of MGE XE. What MGE draws turned out not to be a
   physical sky at all: it is Morrowind's own weather colour ramp — sky, fog, ambient
   and sun colours at sunrise, day, sunset and night, per weather, from Morrowind.ini's
   `[Weather X]` blocks — interpolated by the game hour exactly as the game does, bent
   by a small empirical formula (`fogColourScatter` in XE Common.fx) with three
   scattering constants, and fogged exponentially by distances out of `mge3\MGE.ini`.
   Weather Adjuster, an MWSE mod, swaps the ramp and the constants per *region*.

   This module holds the ramp and answers "what colour is everything at this hour" —
   `Sky.at(hour)`. The engine reads the three files (`gdn_core::weather`); the shaders
   in 06_gl.js are a port of MGE's formulas fed from here. Nothing in it touches where
   grass goes.

   The interpolation is OpenMW's `TimeOfDayInterpolator`, which reproduces the game's:
   night → sunrise colour → day → sunset colour → night, with each of the four colour
   kinds (sky, fog, ambient, sun) starting and finishing its transitions at its own
   pre/post times. Night ends at Sunrise Time, day starts Sunrise Duration after it,
   day ends at Sunset Time, night starts Sunset Duration after that.
   ===================================================================================== */
const Sky={
  data:null,          // the engine's `atmosphere` reply, once an install is connected
  /* Clear, and only Clear (round 17h). The picker and the other nine ramps went with
     the weather effects; the name stays because the ini, Weather Adjuster and MGE all
     key their settings by it. */
  weather:'Clear',
  hour:15,
  region:'',          // the loaded cell's region — picks the Weather Adjuster preset
  /* MGE's own scattering constants (distantinit.cpp), used when no preset says. */
  DEFAULT_SCATTER:{outscatter:[0.07,0.36,0.76], inscatter:[0.25,0.38,0.48],
                   skylight:[0.4456,0.6194,1.0], skylightMix:0.44},
  WEATHERS:['Clear'],

  /** Asks the engine. Quiet when nothing is connected: the sky then keeps its defaults. */
  async load(){
    this.data=null;
    if(typeof Engine==='undefined' || !Engine.has()) return null;
    try{ this.data=await Engine.call('atmosphere'); }catch(e){ this.data=null; }
    if(App.R) App.R.dirty=true;
    this.showSources();
    this.applyInstallPost();
    return this.data;
  },
  /* Round 18i: what the install's post chain says about the two post switches the preview
     has: `{ssao, sunshafts}` booleans, or null when no chain was read. */
  installPost(){
    const p=this.data && this.data.renderer && this.data.renderer.post;
    if(!p || !p.found) return null;
    return {ssao:!!p.ssao, sunshafts:!!p.sunshafts, fxaa:!!p.fxaa, dof:!!p.dof};   // 18do: and FXAA. 18dy: and the depth of field
  },
  /* The switches a profile left unset take the chain's answer, once (19_settings.js). */
  /* Round 18ee: what the install says about the sun's shadows - MGE's own two settings
     (`[distant_land.shadows] enabled`, `map_resolution`, or Sun Shadows / Sun Shadow Map
     Resolution in the GUI, or OpenMW's `[Shadows]`), read by the engine into
     `renderer.shadows`. Null when no install was read, so the switch keeps its default
     and the map its own. */
  installShadows(){
    const s=this.data && this.data.renderer && this.data.renderer.shadows;
    if(!s || !s.found) return null;
    return {enabled:!!s.enabled, resolution:(s.resolution!=null && isFinite(s.resolution) && s.resolution>0)? +s.resolution : null};
  },
  applyInstallPost(){
    const u=App._postUnset; const p=this.installPost();
    if(!u || !p) return;
    const set=(sel,v)=>{ const el=document.getElementById(sel); if(!el || el.checked===v) return; el.checked=v; el.dispatchEvent(new Event('change')); };
    if(u.ssao){ set('p_ssao', p.ssao); u.ssao=false; }
    if(u.sunshafts){ set('p_shafts', p.sunshafts); u.sunshafts=false; }
    if(u.fxaa){ set('p_fxaa', p.fxaa); u.fxaa=false; }
    if(u.dof){ set('p_dof', p.dof); u.dof=false; }
    /* The sun's shadows are not one of the post chain's shaders - they are Distant
       Land's own pass - so they have their own reply and their own first-run handshake. */
    const sh=this.installShadows();
    if(u.shadows && sh){ set('p_shadows', sh.enabled); u.shadows=false; }
  },
  has(){ return !!(this.data && this.data.ini && this.data.ini.weathers); },
  /** The Weather Adjuster preset for the current region, or null. */
  preset(){
    const a=this.data && this.data.adjuster;
    if(!a || !a.presets) return null;
    const name=(a.regions||{})[this.region] || (a.regions||{})[String(this.region).trim()];
    return a.presets[name] || a.presets['default'] || null;
  },
  presetName(){
    const a=this.data && this.data.adjuster;
    if(!a || !a.presets) return '';
    const name=(a.regions||{})[this.region];
    return (name && a.presets[name])? name : (a.presets['default']? 'default' : '');
  },

  /* The ramp for one weather: the ini's four-stop colours, with the preset's colours
     laid over where it has them, and the scattering constants. Colours are 0..1. */
  ramp(){
    const weather=this.weather;
    const ini=this.has()? (this.data.ini.weathers[weather] || this.data.ini.weathers.Clear) : null;
    const dflt=(c)=>[[c,c,c],[c,c,c],[c,c,c],[c,c,c]];
    const r={
      sky:     ini? ini.sky.map(c=>c.slice()) : [[0.46,0.55,0.64],[0.37,0.53,0.80],[0.22,0.35,0.51],[0.035,0.039,0.043]],
      fog:     ini? ini.fog.map(c=>c.slice()) : [[1,0.74,0.62],[0.81,0.89,1],[1,0.74,0.62],[0.035,0.039,0.043]],
      ambient: ini? ini.ambient.map(c=>c.slice()) : [[0.18,0.26,0.38],[0.54,0.55,0.63],[0.27,0.29,0.38],[0.13,0.14,0.16]],
      sun:     ini? ini.sun.map(c=>c.slice()) : [[0.95,0.62,0.47],[1,0.99,0.93],[1,0.45,0.31],[0.23,0.38,0.69]],
      sunDisc: ini? ini.sunDisc.slice() : [1,0.74,0.62],
      cloud:   ini? ini.cloud : '',
      scatter: Object.assign({}, this.DEFAULT_SCATTER),
      // How fast the cloud sheet drifts (the ini's `Cloud Speed`).
      cloudSpeed: (ini && ini.cloudSpeed!=null)? +ini.cloudSpeed : 1,
    };
    void dflt;
    const p=this.preset();
    if(p){
      const w=p[weather];
      const stops=['Sunrise','Day','Sunset','Night'];
      if(w){
        for(const [key,pre] of [['sky','sky'],['fog','fog'],['ambient','ambient'],['sun','sun']]){
          stops.forEach((s,i)=>{ const c=w[pre+s+'Color']; if(c && c.length>=3) r[key][i]=[+c[0],+c[1],+c[2]]; });
        }
        if(w.sundiscSunsetColor) r.sunDisc=w.sundiscSunsetColor.slice(0,3).map(Number);
      }
      if(p.outscatter) r.scatter.outscatter=p.outscatter.slice(0,3).map(Number);
      if(p.inscatter) r.scatter.inscatter=p.inscatter.slice(0,3).map(Number);
      if(p.skylightScatter) r.scatter.skylight=p.skylightScatter.slice(0,3).map(Number);
      if(p.skylightScatterMix!=null) r.scatter.skylightMix=+p.skylightScatterMix;
    }
    return r;
  },

  /** The game's timings, with the vanilla numbers behind them. */
  timing(){
    const t=(this.has() && this.data.ini.timing) || {};
    const g=(k,d)=> (t[k]!=null && isFinite(t[k]))? +t[k] : d;
    return {
      sunrise:g('sunrise',6), sunset:g('sunset',18),
      sunriseDuration:g('sunriseDuration',2), sunsetDuration:g('sunsetDuration',2),
      sky:     [g('skyPreSunrise',0.5), g('skyPostSunrise',1), g('skyPreSunset',1.5), g('skyPostSunset',0.5)],
      ambient: [g('ambientPreSunrise',0.5), g('ambientPostSunrise',2), g('ambientPreSunset',1), g('ambientPostSunset',1.25)],
      fog:     [g('fogPreSunrise',0.5), g('fogPostSunrise',1), g('fogPreSunset',2), g('fogPostSunset',1)],
      sun:     [g('sunPreSunrise',0), g('sunPostSunrise',0), g('sunPreSunset',1), g('sunPostSunset',1.25)],
      precipGravity: g('precipGravity',575),   // round 17b: the rain's fall, units a second
      snowGravityScale: g('snowGravityScale',0.1), snowLowKill: g('snowLowKill',150), snowHighKill: g('snowHighKill',700),   // round 17c
    };
  },

  /* One colour kind at one hour: OpenMW's TimeOfDayInterpolator, which is the game's
     rule. `stops` is [sunrise, day, sunset, night]; `pp` the kind's four pre/post times. */
  _lerp(a,b,t){ t=Math.max(0,Math.min(1,t)); return [a[0]+(b[0]-a[0])*t, a[1]+(b[1]-a[1])*t, a[2]+(b[2]-a[2])*t]; },
  interp(stops,pp,hour,T){
    const [preSunrise,postSunrise,preSunset,postSunset]=pp;
    const nightEnd=T.sunrise, dayStart=T.sunrise+T.sunriseDuration;
    const dayEnd=T.sunset, nightStart=T.sunset+T.sunsetDuration;
    const [sunrise,day,sunset,night]=stops;
    if(hour < nightEnd-preSunrise || hour > nightStart+postSunset) return night.slice();
    if(hour >= nightEnd-preSunrise && hour <= dayStart+postSunrise){
      if(hour < T.sunrise){
        const t=(hour-(nightEnd-preSunrise))/Math.max(1e-4,T.sunrise-(nightEnd-preSunrise));
        return this._lerp(night,sunrise,t);
      }
      const t=(hour-T.sunrise)/Math.max(1e-4,(dayStart+postSunrise)-T.sunrise);
      return this._lerp(sunrise,day,t);
    }
    if(hour > dayStart+postSunrise && hour < dayEnd-preSunset) return day.slice();
    if(hour < T.sunset){
      const t=(hour-(dayEnd-preSunset))/Math.max(1e-4,T.sunset-(dayEnd-preSunset));
      return this._lerp(day,sunset,t);
    }
    const t=(hour-T.sunset)/Math.max(1e-4,(nightStart+postSunset)-T.sunset);
    return this._lerp(sunset,night,t);
  },

  /* Where the sun is. The game's arc (OpenMW's `Sky::setSunDirection` rule) runs from
     sunrise over the top to the west by the end of the sunset, and then starts *again
     in the east* for the night light — which on screen was the sun teleporting 180°
     at half past seven while its disc was still fading (round 17, Robin: "it feels
     like the sun teleports"). So the arc is one circle now: the day's half above the
     horizon, sunrise to the end of the sunset, and the night's half below it, back to
     the east by sunrise. Continuous everywhere, and the disc sets on the horizon as it
     fades. The fixed tilt of -0.268 in y is the game's. `sunLight` (below) is what the
     night's light comes from. */
  sunDir(hour,T){
    const dayEnd=T.sunset+T.sunsetDuration;
    let theta;
    if(hour>=T.sunrise && hour<=dayEnd){
      theta=Math.PI*(hour-T.sunrise)/Math.max(1e-4,dayEnd-T.sunrise);
    } else {
      const h = hour < T.sunrise ? hour+24 : hour;
      theta=Math.PI + Math.PI*(h-dayEnd)/Math.max(1e-4,24-dayEnd+T.sunrise);
    }
    const d=[Math.cos(theta),-0.268,Math.sin(theta)];
    const l=Math.hypot(d[0],d[1],d[2]);
    return [d[0]/l,d[1]/l,d[2]/l];
  },
  /* How much of the sun is showing: one through the day, gone at night, faded over
     the half hour either side. MGE flips its sun below the horizon when this is zero,
     so the sky at night is lit from below rather than by a sun that never set. */
  sunVis(hour,T){
    const nightStart=T.sunset+T.sunsetDuration;
    const rise=(a,b)=>Math.max(0,Math.min(1,(hour-a)/Math.max(1e-4,b-a)));
    if(hour < T.sunrise-0.5 || hour > nightStart) return 0;
    if(hour < T.sunrise) return rise(T.sunrise-0.5,T.sunrise);
    if(hour > nightStart-0.5) return 1-rise(nightStart-0.5,nightStart);
    return 1;
  },

  /* How visible the stars are, 0..1 (round 15 item 2): the game's [Weather] timing -
     Stars Post-Sunset Start (1h after sunset they begin), Stars Fading Duration (2h),
     Stars Pre-Sunrise Finish (gone 2h before sunrise). The clock wraps past midnight. */
  stars(hour,T){
    const t=(this.has() && this.data.ini.timing) || {};
    const g=(k,d)=> (t[k]!=null && isFinite(t[k]))? +t[k] : d;
    const post=g('starsPostSunsetStart',1), fade=g('starsFadingDuration',2), pre=g('starsPreSunriseFinish',2);
    const on0=T.sunset+post, on1=on0+fade;             // fading in
    const off1=T.sunrise-pre, off0=off1-fade;          // fading out
    const h = hour<T.sunrise? hour+24 : hour;          // one night, sunset → next sunrise
    const O1=off1+24, O0=off0+24;
    if(h<=on0 || h>=O1) return 0;
    if(h<on1) return (h-on0)/Math.max(1e-4,on1-on0);
    if(h>O0) return 1-(h-O0)/Math.max(1e-4,O1-O0);
    return 1;
  },

  /** Everything the frame needs at one hour, for the chosen weather. */
  at(hour){
    hour = ((hour%24)+24)%24;
    const weather=this.weather;
    const T=this.timing(), r=this.ramp();
    const sky=this.interp(r.sky,T.sky,hour,T);
    const fog=this.interp(r.fog,T.fog,hour,T);
    const ambient=this.interp(r.ambient,T.ambient,hour,T);
    const sun=this.interp(r.sun,T.sun,hour,T);
    const sunDir=this.sunDir(hour,T);
    const vis=this.sunVis(hour,T);
    /* The disc's colour: the sunset disc colour near sunrise and sunset, white in the
       day — the game only names one, and uses it at both ends. */
    const dusk=Math.max(0, 1-Math.abs(hour-T.sunset)/2, 1-Math.abs(hour-T.sunrise)/2);
    const sunDisc=this._lerp([1,1,1],r.sunDisc,dusk);
    /* MGE's sunPos is the sun, or its reflection under the horizon once it has gone;
       the arc goes under the horizon by itself now, so it is the sun. The light on the
       ground and the objects at night is the game's night light, which the old arc
       ran over the top of the sky: the same direction mirrored back above the horizon
       — continuous, because the mirror is at the horizon. */
    const sunPos=sunDir;
    const sunLight=sunDir[2]<0? [sunDir[0],sunDir[1],-sunDir[2]] : sunDir;
    const nice = 1;   // Clear: MGE's 'nice weather' scattering is always on now
    return {sky,fog,ambient,sun,sunDir,sunPos,sunLight,sunVis:vis,sunDisc,nice,scatter:r.scatter,cloud:r.cloud,
            cloudSpeed:r.cloudSpeed, weather,
            stars:this.stars(hour,T),
            night: hour<T.sunrise || hour>T.sunset+T.sunsetDuration};
  },

  /* MGE's fog distances for the weather, in units: `adjustFog` in distantland.cpp with
     Use Exponential Fog. Start and end in cells from MGE.ini scaled by the weather's
     ratio, the exp curve rescaled by 4.4 so every start reaches the same fog at the end. */
  fogRange(){
    const weather=this.weather;
    const m=(this.data && this.data.mge) || {};
    const w=(m.weathers||{})[weather] || {};
    const ff = w.fogRatio!=null? +w.fogRatio : 1.0;
    const fo = (w.fogOffset!=null? +w.fogOffset : 0)/100;
    const startC = m.fogStart!=null? +m.fogStart : 2.0;
    const endC   = m.fogEnd!=null? +m.fogEnd : 5.0;
    const fogEnd=Math.max(0.875, ff*endC);
    let fogStart=ff*startC - fo*fogEnd;
    if(m.expFog!==false){
      const lg=Math.log(1-0.25*fo);
      const corr=lg/(1+lg);
      fogStart=ff*startC + corr*fogEnd;
    }
    const CELL=8192, SCALE=4.4;
    const s=fogStart*CELL, e=fogEnd*CELL;
    const expStart=s/SCALE, expDiv=(e-expStart)/SCALE;
    return {start:expStart, divisor:Math.max(1,expDiv), linearStart:s, linearEnd:e,
            exp:m.expFog!==false, scattering:m.scattering!==false};
  },

  /* The weather's cloud texture, as a GL texture once it has loaded - the game's own
     `Cloud Texture` (Tx_Sky_Clear and friends, grey with alpha). Asked for on first use
     and remembered; the sky draws without clouds until it arrives. */
  _clouds:new Map(),
  /** Forgotten with the textures (round 18dh): the sheets are `loadTexture`'s and go
   *  when the cache is dropped, so the map must not keep pointing at them. */
  dropClouds(){ this._clouds.clear(); },
  cloudTex(name){
    if(!name || typeof loadTexture!=='function') return null;
    const key=String(name).toLowerCase();
    if(this._clouds.has(key)){ const t=this._clouds.get(key); return t&&t.gl? t.gl : null; }
    this._clouds.set(key,null);
    loadTexture(name).then(t=>{ this._clouds.set(key,t||{gl:null}); if(App.R) App.R.dirty=true; })
                     .catch(()=>this._clouds.set(key,{gl:null}));
    return null;
  },

  /** "06:30" for the slider's label. */
  clock(h){ h=((h%24)+24)%24; const hh=Math.floor(h), mm=Math.round((h-hh)*60)%60;
            return String(hh).padStart(2,'0')+':'+String(mm).padStart(2,'0'); },

  /* Round 14: the sources line under the weather picker. Every install permutation
     says what the sky was read from — a plain install its Morrowind.ini and MGE's
     defaults, OpenMW its openmw.cfg, a modded one the Adjuster preset and MGE.ini. */
  sources(){
    if(!this.data) return T('preview.sky_src_none');
    const parts=[];
    const ini=this.data.ini||{};
    parts.push(ini.found? (ini.openmw? 'openmw.cfg' : 'Morrowind.ini') : T('preview.sky_src_builtin'));
    const a=this.data.adjuster;
    if(a && a.presets){
      const name=this.presetName();
      parts.push('Weather Adjuster'+(name? ' ('+name+')' : ''));
    }
    /* Round 17m: name the file. It used to say "MGE.ini fog" whatever had happened,
       which is how three months went by with the fog read from a file Robin's game does
       not use — he runs G7's fork of MGE XE, which keeps its settings in mgeXE.toml. */
    const m=this.data.mge||{};
    /* `found` means a config file was read; `name` says which. Unnamed but found is not
       "MGE's defaults" — that would assert the opposite of the truth — so it degrades to
       the bare word instead. */
    parts.push(m.found? T('preview.sky_src_mge2',{file:m.name||'MGE'})
                      : T('preview.sky_src_mge_default'));
    // Round 18i: and the renderer whose lighting the preview follows, by its own name.
    const r=this.data.renderer;
    if(r && r.label) parts.push(T('preview.sky_src_renderer',{label:r.label}));
    return T('preview.sky_src')+' '+parts.join(' · ');
  },
  /* Round 18i: the hover on the sources line - which renderer, which files, which
     entries, and the lamp rule. A detector whose reasoning is invisible is a bug with a
     three-month fuse (round 17m); this is the reasoning, in the reader's own numbers. */
  rendererNote(){
    const r=this.data && this.data.renderer;
    if(!r) return '';
    const L=r.lighting||{}, a=r.attenuation||{}, post=r.post||{};
    const lines=[];
    lines.push(r.label+(r.file? ' — '+r.file : '')+(r.how? ' ('+r.how+')' : ''));
    if(L.model==='mge-ppl') lines.push(T('preview.renderer_model_mge'));
    else if(L.model==='openmw-shaders') lines.push(T('preview.renderer_model_openmw',{bounds:String(L.lightBoundsMultiplier==null? 1.65 : +L.lightBoundsMultiplier)}));
    else lines.push(T('preview.renderer_model_ffp'));
    if(L.model==='mge-ppl' && (L.sunMult!=null || L.ambientMult!=null))
      lines.push('sun ×'+(+L.sunMult||1)+', ambient ×'+(+L.ambientMult||1)+' (Clear)');
    const num=v=>{ v=+v; return Number.isFinite(v)? (Math.abs(v)<0.001 && v!==0? v.toExponential(3) : String(+v.toFixed(6))) : '?'; };
    const ex=a.at256||[0,0,0];
    const file=r.attenuationFile||'Morrowind.ini';
    lines.push(a.found
      ? T('preview.renderer_att',{file, c:a.useConstant? num(a.constantValue) : 'off', l:a.useLinear? num(a.linearValue)+'/r^'+(a.linearMethod||0) : 'off',
                                  q:a.useQuadratic? num(a.quadraticValue)+'/r^'+(a.quadraticMethod||0) : 'off', c256:num(ex[0]), l256:num(ex[1]), q256:num(ex[2])})
      : T('preview.renderer_att_default',{file}));
    const known=['ssao','sunshafts','bloom','dof','underwater','fxaa'].filter(k=>post[k]);
    lines.push(T('preview.renderer_post',{list: post.found? (known.length? known.join(', ') : 'none') : 'no chain read'}));
    return lines.join('\n');
  },
  /* Round 17h: what the *install* says the fog is, for the "Reset to install" button
     beside the density slider. The slider is a multiplier on top of the distances read
     from the install, so the install's own answer is 100 by definition — what this adds
     is where those distances came from, in the same words the sources line uses:
     `mge3\MGE.ini`'s `Above Water Fog Start`/`End` and the weather's `Fog Ratio` and
     `Fog Offset`, or MGE XE's built-in numbers when there is no MGE.ini (which is what
     an OpenMW install has, the same as it has no MGE). */
  installFog(){
    const m=(this.data && this.data.mge) || {};
    const w=(m.weathers||{}).Clear || {};
    const r=this.fogRange();
    return {density:100, found:!!m.found,
            start:m.fogStart!=null? +m.fogStart : 2, end:m.fogEnd!=null? +m.fogEnd : 5,
            ratio:w.fogRatio!=null? +w.fogRatio : 1, offset:w.fogOffset!=null? +w.fogOffset : 0,
            exp:m.expFog!==false, cells:[r.linearStart/8192, r.linearEnd/8192],
            /* Round 17m: the file the engine actually read, its full path, and the
               reasoning — the resolver picks between mgeXE.toml and mge3\MGE.ini by
               asking the d3d8.dll which one it reads, and under Root Builder that dll is
               not even in the game folder. `gdn_core::mge` has the whole story. */
            source: m.found? (m.name||'') : (this.data? T('preview.sky_src_mge_default') : ''),
            path: m.file||'', how: m.how||'', dll: m.dll||''};
  },
  /* Round 18dx: the game's underwater blend, `UnderwaterColor x weight + colour x
     (1 - weight)`, applied to *one* colour. Measured, not inferred: an MWSE probe read
     Robin's own engine above and below the surface at Vos, and both colours it could
     reach move by exactly this rule, to four decimal places -

       below water   wc.currentFogColor  0.1639 0.2241 0.2587
                     uw x 0.85 + fogDay x 0.15 = 0.1639 0.2241 0.2587
                     wc.currentSkyColor  0.1117 0.1776 0.2161
                     uw x 0.85 + skyDay x 0.15 = 0.1117 0.1775 0.2161

     - with `wc.underwaterColor` 0.0471 0.1176 0.1451 and `wc.underwaterColorWeight` 0.85,
     the ini's own numbers. It is the *current* weather colour that is blended, so the
     hour's interpolation happens first and this is laid over the result.

     The probe's MWSE build exposes no `ambientColor` or `sunLightColor` to read, but the
     same rule accounts for the light as well: with the ambient and the sun blended too,
     Apel's Sand_02 (albedo 69.6,63.6,49.6, decoded from the DDS) under his Cloudy ramp
     renders at about (26,34,34) after 20% fog, which is the bright end of the sea floor
     his screenshot actually shows - (14,19,21) up to (25,31,37), the spread below it
     being slope shading and the caustics pass, which takes up to 30% off. Unblended it
     renders at (140,137,123), five times too bright, which was the whole of the gap
     Robin spotted: "Maybe the lighting change when going into water?" */
  /** The blend's two halves on their own, for a shader that has to do it per fragment. */
  underwaterMix(){
    const W=((this.data||{}).ini||{}).water||{};
    const uw=(W.underwaterColor && W.underwaterColor.length>=3)? W.underwaterColor.map(Number) : [12/255,30/255,37/255];
    const wgt=Math.max(0,Math.min(1,(W.colorWeight!=null && isFinite(W.colorWeight))? +W.colorWeight : 0.85));
    return {col:uw, weight:wgt};
  },
  underwaterTint(col){
    if(!col || col.length<3) return col;
    const W=((this.data||{}).ini||{}).water||{};
    const uw=(W.underwaterColor && W.underwaterColor.length>=3)? W.underwaterColor.map(Number) : [12/255,30/255,37/255];
    const wgt=Math.max(0,Math.min(1,(W.colorWeight!=null && isFinite(W.colorWeight))? +W.colorWeight : 0.85));
    return [uw[0]*wgt+col[0]*(1-wgt), uw[1]*wgt+col[1]*(1-wgt), uw[2]*wgt+col[2]*(1-wgt)];
  },
  /* Round 18dt: the game's fog with the eye under the water, for this install at this
     hour. Robin: "Fog range and color should be dependant on install."

     **The colour**, every install: `[Water] UnderwaterColor` weighted against the
     weather's fog colour by `UnderwaterColorWeight` (OpenMW `FogManager::getFogColor`;
     MGE reads the game's own colour back and skips its scattering, `adjustFog`, and its
     `fogColFar` is the colour the game *clears* to, which under the water is the same).
     In a room, against the room's own fog colour.

     **The range** is the game's own underwater fog, for every install:
       start = min(view, 7168) x (1 - f),  end = min(view, 7168)
     where f is `UnderwaterSunriseFog / DayFog / SunsetFog / NightFog` interpolated by the
     hour with the Fog timings (OpenMW `weather.cpp`, `fogmanager.cpp`), or
     `UnderwaterIndoorFog` in a room. Vanilla's day is 2.5 - sixty per cent of the colour
     is already the water at the eye, which is what makes the game's underwater read as
     one tinted wash rather than a clear view with haze behind it.

     Round 18dv read Robin's two screenshot pairs and put an MGE install on the game's own
     rule as well, on the grounds that his below-water pair could not make the picture.
     Round 18dw took that back, because the pair had been read as linear when his install
     is exponential and, more to the point, because a matched pair from one spot (the coast
     of Vos, Sand_02, both shots the same view) says something different. Measured across
     twenty bands of the frame: the game's far field - the one place it is fully fogged,
     the horizon under the surface - is 44.6,60.6,69.6, and the colour rule above gives
     41,60,70 for Clear by day. The colour is right. What is wrong is the shape. The game
     runs clear in the near field (the sand keeps its ripple detail, which fog flattens)
     and washes out hard behind it; the preview was a flat 60% milk everywhere, which is
     what the game's rule gives on the eye. So an MGE install takes MGE's own pair after
     all, and the vanilla rule is left to the installs that actually use it.

     **MGE's range**, from the fork's own adjustFog: under the water it takes the first
     branch, before interiors and before weather, and sets fogStart/fogEnd from
     BelowWaterFogStart/End; both are then multiplied by the cell size, 8192. His
     -0.2 / 0.8 is therefore -1638.4 to 6553.6 units - 20% on the eye, 32% at a thousand,
     56% at three thousand, and full at 6553.6. Exponential fog does not enter: underwater
     the fork sets fogNearStart/fogNearEnd to the same pair with the note that the shaders
     use all linear fogging in this case, and XE Common's fogMWScalar, which is what fogs
     everything Morrowind draws, is the linear one in both builds. (The exponential form is
     for the water plane and for distant land, and the fork draws no distant land at all
     under the water - see the underwater plan.)

     What is left over after this is not the fog. His seabed reads far darker than the
     preview's at the same hour, and the weather colours his game was running are not the
     ini's: his load order has an MWSE seasonal-weather mod, and the ambient and sun it
     sets are the half of the picture the preview cannot read. The underwater colour hides
     that almost entirely - 85% of it is the fixed UnderwaterColor - which is why the fog
     colour matches while the ground does not.

     The engine reads the block (`weather.rs` `water_json`), MGE's pair (`mge_json`) and
     OpenMW's (`renderer.rs`); vanilla's numbers stand behind anything missing. `source`
     names the file the range came from, for the hover. Units, not cells. */
  underwaterFog(hour, room, fogNow){
    hour = ((+hour||0)%24+24)%24;
    const d=this.data||{};
    const W=(d.ini && d.ini.water)||{};
    const g=(k,dv)=> (W[k]!=null && isFinite(W[k]))? +W[k] : dv;
    const base=fogNow || (room && room.fog) || [0.5,0.5,0.5];
    const col=this.underwaterTint(base);
    const r=d.renderer||{};
    const eng=r.engine||'vanilla';
    const T3=this.timing();
    const one=v=>[v,v,v];
    const amount = room? g('indoorFog',3)
           : this.interp([one(g('sunriseFog',3)), one(g('dayFog',2.5)), one(g('sunsetFog',3)), one(g('nightFog',4))], T3.fog, hour, T3)[0];
    const fog=r.fog||{};
    let start, end, source;
    if(eng==='openmw' && fog.useDistantFog){
      start=(fog.distantUnderwaterFogStart!=null && isFinite(fog.distantUnderwaterFogStart))? +fog.distantUnderwaterFogStart : -4096;
      end=(fog.distantUnderwaterFogEnd!=null && isFinite(fog.distantUnderwaterFogEnd))? +fog.distantUnderwaterFogEnd : 2457.6;
      end=Math.max(start+1,end);
      source='settings.cfg';
    } else if(eng==='mgexe'){
      const M=d.mge||{}, CELL=8192;
      const s0=(M.fogBelowStart!=null && isFinite(M.fogBelowStart))? +M.fogBelowStart : -0.5;
      const e0=(M.fogBelowEnd!=null && isFinite(M.fogBelowEnd))? +M.fogBelowEnd : 0.3;
      start=s0*CELL; end=e0*CELL;
      if(!(end>start)) end=start+1;
      source=M.name||'mgeXE.toml';
    } else {
      const vd=(eng==='openmw' && fog.viewingDistance!=null && isFinite(fog.viewingDistance))? +fog.viewingDistance : 7168;
      const view=Math.max(1,Math.min(vd,7168));
      start=view*(1-amount); end=view;
      source=(d.ini && d.ini.openmw)? 'openmw.cfg' : 'Morrowind.ini';
    }
    return {col, start, end, source, amount, engine:eng, found:!!W.found};
  },
  showSources(){
    const el=document.getElementById('p_skySrc');
    if(el){ el.textContent=this.sources(); el.title=this.rendererNote(); }
  },
};
