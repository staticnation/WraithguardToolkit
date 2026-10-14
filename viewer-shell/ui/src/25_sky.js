/* Gardenfell, 25_sky.js. Copyright (c) 2026 Robin Hjelte. MIT License; see page-source/LICENSE. */
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
   `Sky.at(hour)`. The engine reads the three files (`gardenfell_install::weather`); the shaders
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
  /** The ten weathers, as the ini and MGE name them (weather.rs WEATHERS). */
  WEATHERS:['Clear','Cloudy','Foggy','Overcast','Rain','Thunderstorm','Ashstorm','Blight','Snow','Blizzard'],
  /** Wonders of Water's per-weather multipliers (NullCascade, MIT; its config.lua), on
      MGE's caustics and wave height: calm and faint in fair weather, rough and bright in
      storms. Keyed by the game's weather names. */
  WOW_WEATHER:{Clear:{caustics:0.2,waveHeight:0.2}, Cloudy:{caustics:0.4,waveHeight:0.4}, Foggy:{caustics:0.1,waveHeight:0.1},
               Overcast:{caustics:0.4,waveHeight:0.4}, Rain:{caustics:1.2,waveHeight:1.2}, Thunderstorm:{caustics:2.0,waveHeight:2.0},
               Ashstorm:{caustics:1.5,waveHeight:1.5}, Blight:{caustics:1.8,waveHeight:1.8}, Snow:{caustics:0.2,waveHeight:0.2},
               Blizzard:{caustics:0.5,waveHeight:0.5}},
  /** The multipliers for the current weather: Wonders of Water's in an exterior (or a room
      that behaves as one), and in a true interior caustics as they are and no waves. */
  wowNow(room){
    if(room && !room.quasi) return {caustics:1.0, waveHeight:0.0};
    return this.WOW_WEATHER[this.weather] || {caustics:1.0, waveHeight:1.0};
  },
  hour:15,
  region:'',          // the loaded cell's region — picks the Weather Adjuster preset
  /* MGE's own scattering constants (distantinit.cpp), used when no preset says. */
  DEFAULT_SCATTER:{outscatter:[0.07,0.36,0.76], inscatter:[0.25,0.38,0.48],
                   skylight:[0.4456,0.6194,1.0], skylightMix:0.44},

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
    return {ssao:!!p.ssao, sunshafts:!!p.sunshafts, fxaa:!!p.fxaa, dof:!!p.dof, bloom:!!p.bloom};   // Wraithguard: and the bloom   // 18do: and FXAA. 18dy: and the depth of field
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
  /* Wraithguard: not taken - the install's post chain still decides sunshafts, FXAA, DOF and
     bloom here (the viewer previews what the install will look like). Gardenfell's note:
     Round 18ew: **the post chain no longer decides anything but the occlusion.** Robin:
     "Depth of field, FXAA and Sunshafts I want to have always on, no matter what the
     install says. MGE water should also be always on. Sun shadows and fog distance can be
     kept as is now, read from the install."
     So those four start on in the markup and nothing here reaches for them. The occlusion
     keeps its handshake because it is the one 18i was written for and the one 18v then
     pinned *off* in a new profile's own file - a profile written before either round still
     has no `ssao` key, and this is what answers it. The sun's shadows keep theirs at his
     word. `installPost` still reads all four, because the Settings note reports what the
     install has whether or not the preview follows it. */
  applyInstallPost(){
    // Wraithguard: the normal and specular maps in cells, as the setup has them.
    if(typeof WgViewport==='object' && WgViewport.installMaps) WgViewport.installMaps();
    const u=App._postUnset; const p=this.installPost();
    if(!u) return;
    const set=(sel,v)=>{ const el=document.getElementById(sel); if(!el || el.checked===v) return; el.checked=v; el.dispatchEvent(new Event('change')); };
    // Wraithguard keeps the handshake for all of these: the install decides when it says
    // anything (`p` null when it says nothing). Sunshafts, FXAA and DOF start on in the
    // markup, as Gardenfell 18ew has them, so "else on" (Static, 2026-10-12). The `&& p`
    // guard is 18ew's.
    if(u.ssao && p){ set('p_ssao', p.ssao); u.ssao=false; }
    if(u.sunshafts && p){ set('p_shafts', p.sunshafts); u.sunshafts=false; }
    if(u.fxaa && p){ set('p_fxaa', p.fxaa); u.fxaa=false; }
    if(u.dof && p){ set('p_dof', p.dof); u.dof=false; }
    if(u.bloom && p){ set('p_bloom', p.bloom); u.bloom=false; }
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
    /* The weather's own share of the sun (Wraithguard, with the weathers back): the ini's
       Glare View - 1 in clear and cloudy, a quarter in fog, none under overcast, rain or
       a storm - which is how the game scales its sun visibility, and MGE reads that back. */
    const wi=this.has()? this.data.ini.weathers[weather] : null;
    const glare=(wi && wi.glareView!=null && isFinite(wi.glareView))? Math.max(0, Math.min(1, +wi.glareView)) : 1;
    const vis=this.sunVis(hour,T)*glare;
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
    /* MGE's niceWeather (distantland.cpp): the scattering runs in Clear and Cloudy - weather
       index 0 and 1 - and not in anything heavier. */
    const nice = /^(clear|cloudy)$/i.test(weather)? 1 : 0;
    return {sky,fog,ambient,sun,sunDir,sunPos,sunLight,sunVis:vis,sunDisc,nice,scatter:r.scatter,cloud:r.cloud,
            cloudSpeed:r.cloudSpeed, weather,
            stars:this.stars(hour,T),
            night: hour<T.sunrise || hour>T.sunset+T.sunsetDuration};
  },

  /* MGE's fog distances for the weather, in units: `adjustFog` in distantland.cpp with
     Use Exponential Fog. Start and end in cells from MGE.ini scaled by the weather's
     ratio, the exp curve rescaled by 4.4 so every start reaches the same fog at the end. */
  /* Round 18ey: OpenMW's per-weather distant-fog pair, `dlFactor` and `dlOffset`.
     Hardcoded in the game rather than configurable - OpenMW's `weather.cpp` passes them
     to each Weather's constructor - so they are a table here too. Clear is 1.0 / 0.0,
     which is why its distant fog comes out at exactly the `distant land fog start` and
     `end` in settings.cfg (16384 and 40960, or 2 and 5 cells - the same numbers MGE XE
     defaults to). The other nine went with the weather picker in round 17h and their
     factors go in beside this one when it comes back; Robin: "Prepare for it to be per
     weather, but as we only have clear for now, we use that portion only for now." */
  OMW_DL:{Clear:[1.0, 0.0]},
  /* Round 18ey: the fog an OpenMW install draws, in cells, or null for anything else.
     Robin: "Use the OpenMW view distance to set fog distances."
     Until this round an OpenMW install got MGE XE's built-in 2-5 cells, because
     `installFog` reads `mge3\MGE.ini` and OpenMW has none - plausible-looking numbers
     from the wrong engine. OpenMW's own rule is `FogManager::configure`, and it has two
     branches:
       `use distant fog = false` (the default): no fog at all when the weather's land fog
         depth is nought, and otherwise `start = viewDistance x (1 - depth)`, `end =
         viewDistance`. So the view distance *is* the fog distance, which is the thing he
         asked for, and the depth is the weather's - `Land Fog Day Depth` in the ini, or
         the `fallback=Weather_Clear_Land_Fog_Day_Depth` line in openmw.cfg, both of which
         the weather reader already carries as `fogDepthDay`.
       `use distant fog = true`: `start = factor x (dlStart - offset x dlEnd)`, `end =
         factor x (1 - offset) x dlEnd`, off `[Fog] distant land fog start`/`end` and the
         weather's own factor and offset.
     The night depth is read and carried and deliberately not used yet: the game crossfades
     between the two over the sunrise and sunset windows, and guessing that blend would be
     inventing a rule rather than reading one. Day is what the preview's default hour is. */
  openmwFog(){
    const r=this.data && this.data.renderer;
    const f=r && r.fog;
    // Only when the install really is OpenMW: an MGE install's renderer block has no fog.
    if(!f || f.viewingDistance==null) return null;
    const CELL=8192;
    const w=(this.data.ini && this.data.ini.weathers && this.data.ini.weathers[this.weather]) || {};
    if(f.useDistantFog){
      const [ff,fo]=this.OMW_DL[this.weather] || this.OMW_DL.Clear;
      const dls=f.distantLandFogStart!=null? +f.distantLandFogStart : 16384;
      const dle=f.distantLandFogEnd!=null? +f.distantLandFogEnd : 40960;
      return {start:(ff*(dls-fo*dle))/CELL, end:(ff*(1-fo)*dle)/CELL,
              exp:!!f.exponential, why:'distant', depth:null};   // `why` is a tag, not prose
    }
    const vd=+f.viewingDistance || 7168;
    const depth=w.fogDepthDay!=null? +w.fogDepthDay : 0.7;
    /* Depth nought is the game's "no fog": start 0, end the largest float there is. The
       preview wants a number it can divide by, so the fog is pushed out past anything
       drawn rather than given an infinity that would come back NaN. */
    if(!(depth>0)) return {start:1e4/CELL, end:1e5/CELL, exp:!!f.exponential, why:'nofog', depth:0};
    return {start:(vd*(1-depth))/CELL, end:vd/CELL, exp:!!f.exponential, why:'viewdist', depth};
  },
  fogRange(){
    const weather=this.weather;
    const o=this.openmwFog();
    if(o){
      const CELL=8192, SCALE=4.4;
      const s=o.start*CELL, e=Math.max(o.start+0.001, o.end)*CELL;
      const expStart=s/SCALE, expDiv=(e-expStart)/SCALE;
      return {start:expStart, divisor:Math.max(1,expDiv), linearStart:s, linearEnd:e,
              /* OpenMW's own `[Fog] exponential fog`, which is off by default where MGE's
                 is on: drawing OpenMW's distances with MGE's curve would be a blend of two
                 engines that matches neither. Scattering is left as it was - it is MGE's
                 atmosphere model and nothing in settings.cfg bears on it. */
              exp:o.exp, scattering:((this.data && this.data.mge) || {}).scattering!==false};
    }
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
    const pending={gl:null};
    this._clouds.set(key,pending);
    const keep=t=>{
      // A reset, or another request after it, owns the slot now.
      if(this._clouds.get(key)!==pending) return;
      this._clouds.set(key,t||{gl:null});
      if(App.R) App.R.dirty=true;
    };
    loadTexture(name).then(keep).catch(()=>keep(null));
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
    // Round 18fc: and Wonders of Water, when the install runs it.
    if(this.wondersActive()) parts.push(T('preview.sky_src_wow'));
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
    /* Round 18fc: Wonders of Water's rule, and where its settings came from - the
       person's own config in the manager's overwrite folder, or the mod's defaults. */
    const wow=this.data.wondersOfWater;
    if(wow && wow.found){
      const on=this.wondersActive();
      lines.push(T(on? 'preview.renderer_wow' : 'preview.renderer_wow_off',
                   {units:String(wow.depthUnits||1500), min:String(wow.minVision==null? 0.1 : wow.minVision),
                    file: wow.configFile || T('preview.renderer_wow_defaults')}));
      if(wow.trouble) lines.push(wow.trouble);
    }
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
    /* Round 18ey: an OpenMW install answers out of its own settings.cfg, so the button's
       hover says which entry the distances came from rather than naming an MGE.ini that
       is not there. `ratio` and `offset` are MGE's two words for the same shape - the
       weather's factor and offset - so the line reads the same either way. */
    const o=this.openmwFog();
    if(o){
      const f=(this.data.renderer && this.data.renderer.fog) || {};
      const [ff,fo]=this.OMW_DL[this.weather] || this.OMW_DL.Clear;
      /* `how` is the engine's own line about which renderer it found and how - the same
         string the MGE path shows. Naming the individual settings.cfg entries here would
         be new prose in the page, and the wording round has not opened yet (Robin: "I
         will let you know when I am ready for that, and we shouldn't act on that before I
         am"), so the numbers speak and the entry names wait their turn. */
      return {density:100, found:true,
              start:+o.start.toFixed(3), end:+o.end.toFixed(3),
              ratio:o.why==='distant'? ff : 1, offset:o.why==='distant'? fo : 0,
              exp:o.exp, cells:[r.linearStart/8192, r.linearEnd/8192],
              from:o.why, view:(+f.viewingDistance)||7168, depth:o.depth,
              source:'settings.cfg',
              path:(this.data.renderer && this.data.renderer.file) || '',
              how:(this.data.renderer && this.data.renderer.how) || '',
              dll:''};
    }
    return {density:100, found:!!m.found,
            start:m.fogStart!=null? +m.fogStart : 2, end:m.fogEnd!=null? +m.fogEnd : 5,
            ratio:w.fogRatio!=null? +w.fogRatio : 1, offset:w.fogOffset!=null? +w.fogOffset : 0,
            exp:m.expFog!==false, cells:[r.linearStart/8192, r.linearEnd/8192],
            /* Round 17m: the file the engine actually read, its full path, and the
               reasoning — the resolver picks between mgeXE.toml and mge3\MGE.ini by
               asking the d3d8.dll which one it reads, and under Root Builder that dll is
               not even in the game folder. `gardenfell_install::mge` has the whole story. */
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
  underwaterMix(scale){
    const W=((this.data||{}).ini||{}).water||{};
    const k=(scale==null || !isFinite(scale))? 1 : Math.max(0,Math.min(1,+scale));
    const uw=((W.underwaterColor && W.underwaterColor.length>=3)? W.underwaterColor.map(Number) : [12/255,30/255,37/255]).map(v=>v*k);
    const wgt=Math.max(0,Math.min(1,(W.colorWeight!=null && isFinite(W.colorWeight))? +W.colorWeight : 0.85));
    return {col:uw, weight:wgt};
  },
  /* `scale` (18fc): what Wonders of Water does to the water's colour with depth - the
     game's `[Water]` colour times (1 - depth/1500) - so the same blend the eye takes at
     the surface darkens as it goes down. One where nothing scales it. */
  underwaterTint(col, scale){
    if(!col || col.length<3) return col;
    const W=((this.data||{}).ini||{}).water||{};
    const k=(scale==null || !isFinite(scale))? 1 : Math.max(0,Math.min(1,+scale));
    const uw=((W.underwaterColor && W.underwaterColor.length>=3)? W.underwaterColor.map(Number) : [12/255,30/255,37/255]).map(v=>v*k);
    const wgt=Math.max(0,Math.min(1,(W.colorWeight!=null && isFinite(W.colorWeight))? +W.colorWeight : 0.85));
    return [uw[0]*wgt+col[0]*(1-wgt), uw[1]*wgt+col[1]*(1-wgt), uw[2]*wgt+col[2]*(1-wgt)];
  },
  /* Round 18fc: Wonders of Water (NullCascade, MWSE-Lua), when the install runs it.
     Robin: "For MGE XE installs that support MWSE mods, if I have Wonders of Waters
     installed, I want its rules for fog depending on depth under water to be reflected in
     Gardenfell." The engine finds the mod through the overlay and reads its config from
     the manager's overwrite folder, or reports the mod's defaults (weather.rs
     `wonders_json`, which quotes the rule). What the mod does, per frame, with the head
     `depth` units under the surface:
       depthFactor = clamp(depth / 1500, 0, 1)
       vision      = max(1 - depthFactor, 0.1)         (Night Eye would add to the 1)
       MGE's below-water fog start and end  x vision
       the game's [Water] underwater colour x (1 - depthFactor)
     Its per-weather "clarity" is read and then not used by the mod's own fog, so it is
     not modelled either. It only runs where MWSE runs, on MGE XE - the mod is an MWSE
     script that sets MGE's own render config - so an OpenMW install with the same files
     in its data folders is left alone. `null` where it does not act. */
  wondersOfWater(depth){
    const d=this.data||{}, w=d.wondersOfWater, r=d.renderer||{};
    if(!w || !w.found || w.enabled===false || w.waterClarity===false) return null;
    if(r.engine!=='mgexe' || w.mwse===false) return null;
    const units=(w.depthUnits>0)? +w.depthUnits : 1500;
    const minV=(w.minVision!=null && isFinite(w.minVision))? +w.minVision : 0.1;
    const f=Math.max(0, Math.min(1, Math.max(0, +depth||0)/units));
    return {depthFactor:f, vision:Math.max(1-f, minV), scale:1-f, units, minVision:minV,
            configFile:w.configFile||null, source:T('preview.sky_src_wow')};
  },
  /** Whether the mod acts on this install at all - for the sources line. */
  wondersActive(){ return !!this.wondersOfWater(0); },
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
  underwaterFog(hour, room, fogNow, wow){
    hour = ((+hour||0)%24+24)%24;
    const d=this.data||{};
    const W=(d.ini && d.ini.water)||{};
    const g=(k,dv)=> (W[k]!=null && isFinite(W[k]))? +W[k] : dv;
    const base=fogNow || (room && room.fog) || [0.5,0.5,0.5];
    // 18fc: `wow` is Wonders of Water's state for this depth (`wondersOfWater`), or null.
    const col=this.underwaterTint(base, wow? wow.scale : 1);
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
    /* 18fc: Wonders of Water closes the fog in with depth - MGE's own below-water pair
       times its vision factor (`wondersOfWater`). It sets MGE's render config, so it
       only ever reaches the MGE branch above; the source line says so. */
    if(wow && eng==='mgexe'){
      start*=wow.vision; end*=wow.vision;
      if(!(end>start)) end=start+1;
      source=source+' · '+wow.source;
    }
    return {col, start, end, source, amount, engine:eng, found:!!W.found, wow:(eng==='mgexe'? wow : null)||null};
  },
  showSources(){
    const el=document.getElementById('p_skySrc');
    if(el){ el.textContent=this.sources(); el.title=this.rendererNote(); }
  },
};
