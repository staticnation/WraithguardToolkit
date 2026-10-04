// Cell viewer regression check: the page in jsdom, driven by the real engine.
//
// Boots the assembled page (../../ui-dist/index.html) against `wg-view-serve` (the
// command bodies, headless), connects the fixture install in ./fixture as the
// Wraithguard setup, loads a cell, and fails on any page error or if the cell never
// loads. jsdom has no GPU: WebGL is a no-op stub, so this checks scripts, DOM wiring
// and the engine round trips, not pixels.
//
//   cargo build --release --manifest-path ../../check-commands/Cargo.toml --bin wg-view-serve
//   npm install && node boot.js
//
// Env: WG_VIEW_SERVE (engine path), WG_VIEW_CFG / WG_VIEW_CELL (override the fixture),
// WG_DEBUG (an expression to evaluate in the page and print), WG_MESH_VIEW=1 (the mesh
// viewer on the fixture's anim.nif (its tr.dds has tr_n.dds and tr_spec.dds beside it), from the
// load order and from disk, instead of a cell).
const fs=require('fs'),{JSDOM,VirtualConsole}=require('jsdom');
const path=require('path');
const CFG=process.env.WG_VIEW_CFG||path.join(__dirname,'fixture','openmw.cfg');
const MESH=!!process.env.WG_MESH_VIEW;
const CELL=process.env.WG_VIEW_CELL?JSON.parse(process.env.WG_VIEW_CELL):{kind:'ext',x:9,y:9,name:'Lamptown',label:'Lamptown'};
/** The page as build.rs assembles it: ui/ORDER's parts, ui/LICENSE as a comment after
 *  <head>, and viewer_only.html before </head>. */
function assemble(){
  const ui=path.join(__dirname,'..');
  const order=fs.readFileSync(path.join(ui,'ORDER'),'utf8').split(/\r?\n/).map(l=>l.trim()).filter(l=>l && !l.startsWith('#'));
  let page=order.map(n=>{ const t=fs.readFileSync(path.join(ui,'src',n),'utf8'); return t.endsWith('\n')? t : t+'\n'; }).join('');
  const head=page.indexOf('<head>')+'<head>'.length;
  const licence=fs.readFileSync(path.join(ui,'LICENSE'),'utf8').split('-->').join('- ->');
  page=page.slice(0,head)+'\n<!--\n'+licence+'\n-->\n'+page.slice(head);
  const at=page.indexOf('</head>');
  const icon='data:image/png;base64,'+fs.readFileSync(path.join(ui,'wraithguard_icon.png')).toString('base64');
  return (page.slice(0,at)+fs.readFileSync(path.join(ui,'viewer_only.html'),'utf8')+page.slice(at))
    .split('__WG_ICON__').join(icon);
}
const html=process.argv[2]? fs.readFileSync(process.argv[2],'utf8') : assemble();

/* A stand-in for Wraithguard's editor endpoints (gui/editorlink.py, tested in pytest):
   one record, its changes kept, so the Editor mode's dialog and pool can be driven here.
   Reached the way the real one is - the engine's loopback POST (`wg_post`). */
const http=require('http');
const fakeWg={posts:[], queued:{}, reviewed:false, ref:null, refQueued:{}, refBase:{id:'', pos:[0,0,0]}, news:{}};
const fakeNewView=n=>({cell:n.cell, uid:n.uid, id:n.fields.id, tag:n.tag, new:true, plugins:n.plugins||[],
  fields:['translation','rotation','scale','deleted'].map(path=>({path, value:n.fields[path]==null? null : n.fields[path],
    kind:path==='scale'?'float':path==='deleted'?'bool':'vec3', editable:true}))});
const fakeRefView=()=>({cell:fakeWg.ref.cell, origin:fakeWg.ref.origin, refr:fakeWg.ref.refr, id:fakeWg.refBase.id,
  winner:'Lamp.esm', plugins:fakeWg.ref.plugins||['Lamp.esm'],
  fields:[
    {path:'translation', value:fakeWg.refBase.pos, kind:'vec3', editable:true},
    {path:'rotation', value:[0,0,0], kind:'vec3', editable:true},
    {path:'scale', value:null, kind:'float', editable:true},
    {path:'deleted', value:null, kind:'bool', editable:true},
    {path:'lock_level', value:null, kind:'int', editable:true},
    {path:'destination', value:null, kind:'door', editable:true},
  ].map(f=> f.path in fakeWg.refQueued? Object.assign(f,{queued:fakeWg.refQueued[f.path]}) : f)});
const fakeView=()=>({tag:'LIGH', type:'Light', id:'lamp_lit', winner:'Lamp.esm', plugins:['Lamp.esm'], whole:null,
  fields:[
    {path:'id', value:'lamp_lit', editable:false, kind:'str', options:[]},
    {path:'mesh', value:'x\\lamp.nif', editable:true, kind:'str', options:[]},
    {path:'data.radius', value:256, editable:true, kind:'int:0:4294967295', options:[]},
    {path:'flags', value:'', editable:true, kind:'flags:ObjectFlags', options:['DELETED','PERSISTENT']},
  ].map(f=> f.path in fakeWg.queued? Object.assign(f,{queued:fakeWg.queued[f.path], source:'typed'}) : f)});
fakeWg.made={};
const fakeServer=http.createServer((req,res)=>{
  let body=''; req.on('data',c=>body+=c); req.on('end',()=>{
    const name=req.url.split('?')[0].slice(1), b=body? JSON.parse(body) : {};
    fakeWg.posts.push([name,b]);
    let out;
    if(name==='editRecord') out=fakeView();
    else if(name==='editDuplicate'){
      if(fakeWg.made[b.newId.toLowerCase()] || b.newId.toLowerCase()==='lamp_lit'){ res.writeHead(400); res.end(b.newId+' is already a Light id in this load order'); return; }
      fakeWg.made[b.newId.toLowerCase()]={tag:'LIGH', id:b.newId};
      out=Object.assign(fakeView(), {id:b.newId, new:true, winner:'(this patch)', plugins:['(this patch)']});
    }
    else if(name==='editRevert' && fakeWg.made[String(b.id).toLowerCase()] && !b.path){ delete fakeWg.made[String(b.id).toLowerCase()]; out=fakeView(); }
    else if(name==='editSet'){
      if(b.path==='id'){ res.writeHead(400); res.end('id cannot be changed here'); return; }
      fakeWg.queued[b.path]=b.value; out=fakeView();
    } else if(name==='editRevert'){ if(b.path) delete fakeWg.queued[b.path]; else fakeWg.queued={}; out=fakeView(); }
    else if(name==='editRef'){ fakeWg.ref=b; out=fakeRefView(); }
    else if(name==='editPlace'){
      const uid='new-'+(Object.keys(fakeWg.news).length+1).toString(16).padStart(8,'0');
      const n={cell:b.cell, uid, tag:b.tag, plugins:b.plugins, fields:{id:b.id, translation:b.translation, rotation:b.rotation||[0,0,0]}};
      fakeWg.news[uid]=n; out=fakeNewView(n);
    }
    else if(name==='editNew'){ const n=fakeWg.news[b.uid]; if(!n){ res.writeHead(400); res.end('That new reference is no longer in the patch'); return; } out=fakeNewView(n); }
    else if(name==='editNewSet'){ const n=fakeWg.news[b.uid]; n.fields[b.path]=b.value; out=fakeNewView(n); }
    else if(name==='editNewRemove'){ delete fakeWg.news[b.uid]; res.writeHead(200); res.end('ok'); return; }
    else if(name==='editRefSet'){
      if(b.path==='scale' && b.value>2){ res.writeHead(400); res.end('scale is 0.5 to 2.0'); return; }
      fakeWg.ref=Object.assign({}, b); fakeWg.refQueued[b.path]=b.value; out=fakeRefView();
    } else if(name==='editRefRevert'){ if(b.path) delete fakeWg.refQueued[b.path]; else fakeWg.refQueued={}; out=fakeRefView(); }
    else if(name==='editPending'){
      out=Object.keys(fakeWg.queued).length?
        [{tag:'LIGH', type:'Light', id:'lamp_lit', whole:null, changes:Object.entries(fakeWg.queued).map(([path,value])=>({path,value}))}] : [];
      for(const m of Object.values(fakeWg.made))
        out.push({tag:m.tag, type:'Light', id:m.id, whole:null, made:{source:'Lamp.esm', name:'Lamp', mesh:'x\\lamp.nif'}, changes:[]});
      if(fakeWg.ref && Object.keys(fakeWg.refQueued).length)
        out.push({tag:'', type:'Reference', id:fakeWg.ref.origin+':'+fakeWg.ref.refr+' in '+fakeWg.ref.cell, whole:null,
                  ref:{cell:fakeWg.ref.cell, origin:fakeWg.ref.origin, refr:fakeWg.ref.refr, plugins:fakeWg.ref.plugins||[]},
                  changes:Object.entries(fakeWg.refQueued).map(([path,value])=>({path,value}))});
      for(const n of Object.values(fakeWg.news))
        out.push({tag:'', type:'NewReference', id:n.fields.id+' (new) in '+n.cell, whole:null,
                  new:{cell:n.cell, uid:n.uid, id:n.fields.id, tag:n.tag, plugins:n.plugins||[]},
                  changes:Object.entries(n.fields).map(([path,value])=>({path,value}))});
    }
    else if(name==='editReview'){ fakeWg.reviewed=true; res.writeHead(200); res.end('ok'); return; }
    else { res.writeHead(404); res.end(); return; }
    res.writeHead(200,{'Content-Type':'application/json'}); res.end(JSON.stringify(out));
  });
});
fakeServer.listen(0,'127.0.0.1');
const {spawn}=require('child_process');
const eng=spawn(process.env.WG_VIEW_SERVE||require('path').join(__dirname,'../../check-commands/target/release/wg-view-serve'+(process.platform==='win32'?'.exe':'')),[],{stdio:['pipe','pipe','inherit']});
let nextId=1; const waiting=new Map(); let buf='';
eng.stdout.on('data',d=>{buf+=d; let nl; while((nl=buf.indexOf('\n'))>=0){const line=buf.slice(0,nl); buf=buf.slice(nl+1); let m; try{m=JSON.parse(line);}catch(e){continue;} if(m.ready) continue; const w=waiting.get(m.id); if(w){waiting.delete(m.id); w(m);}}});
function engineCall(cmd,args){ return new Promise((res,rej)=>{ const id=nextId++; waiting.set(id,m=>{ if('err' in m) rej(m.err); else if('b64' in m){ const b=Buffer.from(m.b64,'base64'); res(b.buffer.slice(b.byteOffset,b.byteOffset+b.length)); } else res(m.ok); }); eng.stdin.write(JSON.stringify({id,cmd,args:args||{}})+'\n'); }); }
const errors=[], calls=[];
// A throw inside a page event handler surfaces here in jsdom: record it, keep going.
const short=e=>String(e&&e.stack||e).split('\n').filter(l=>!l.includes('node_modules')).slice(0,4).join(' | ');
process.on('uncaughtException',e=>errors.push('uncaught: '+short(e)));
process.on('unhandledRejection',e=>errors.push('unhandled: '+short(e)));
const vc=new VirtualConsole();
vc.on('jsdomError',e=>errors.push('jsdomError: '+(e.detail&&e.detail.stack||e.message).split('\n').slice(0,3).join(' | ')));
vc.on('error',(...a)=>errors.push('console.error: '+a.map(String).join(' ').slice(0,300)));
const dom=new JSDOM(html,{runScripts:'dangerously',pretendToBeVisual:true,virtualConsole:vc,url:'http://tauri.localhost/index.html',
  beforeParse(w){
    w.__TAURI__={core:{invoke:(cmd,args)=>{calls.push(cmd); return engineCall(cmd,args);}},
                 dialog:{open:async()=>null,save:async()=>null}};
    w.__WG_VIEW__= MESH ? {cfg:CFG, meshView:true, extra:{meshes:[
        {path:'meshes/x/anim.nif', label:'Load order'},
        {path:path.join(__dirname,'fixture','Data Files','meshes','x','anim.nif'), label:'On disk'}]}}
      : {cfg:CFG, cell:CELL, extra:{links:{}}};
    // No WebGL in jsdom (the viewport reports no context); 2D canvases get a no-op
    // context so map and swatch drawing run.
    const noop2d=()=>new Proxy({canvas:null},{get(t,k){ if(k in t) return t[k]; if(k==='getImageData'||k==='createImageData') return (x,y,wd,ht)=>({data:new Uint8ClampedArray(4*(wd||1)*(ht||1)),width:wd||1,height:ht||1}); if(k==='measureText') return ()=>({width:0}); if(/^create/.test(k)) return ()=>({addColorStop(){}}); return ()=>{}; }, set(t,k,v){ t[k]=v; return true; }});
    // A stub WebGL2: every call succeeds and draws nothing. Enough for the renderer to
    // initialise, so the cell-load path (engine calls, geometry decode, scene build) runs.
    const K={}; let kn=0x9000; const glConst=k=>K[k]||(K[k]=++kn);
    const handle=()=>({});
    const PARAMS={MAX_TEXTURE_SIZE:8192,MAX_SAMPLES:4,MAX_TEXTURE_IMAGE_UNITS:16,MAX_COMBINED_TEXTURE_IMAGE_UNITS:32,MAX_VERTEX_ATTRIBS:16,MAX_DRAW_BUFFERS:8,MAX_COLOR_ATTACHMENTS:8,MAX_UNIFORM_BLOCK_SIZE:65536,MAX_VERTEX_UNIFORM_VECTORS:1024,MAX_FRAGMENT_UNIFORM_VECTORS:1024,MAX_RENDERBUFFER_SIZE:8192,MAX_VIEWPORT_DIMS:[8192,8192],MAX_ARRAY_TEXTURE_LAYERS:256,MAX_3D_TEXTURE_SIZE:2048,VERSION:'WebGL 2.0 (stub)',SHADING_LANGUAGE_VERSION:'WebGL GLSL ES 3.00',VENDOR:'stub',RENDERER:'stub'};
    function stubGL(canvas){
      const gl=new Proxy({canvas, drawingBufferWidth:1400, drawingBufferHeight:900},{get(t,k){
        if(k in t) return t[k];
        if(typeof k!=='string') return undefined;
        if(/^[A-Z0-9_]+$/.test(k)) return glConst(k);
        switch(k){
          case 'getExtension': return n=>new Proxy({},{get(_,c){ return typeof c==='string'&&/^[A-Z0-9_]+$/.test(c)? glConst(c) : ()=>handle(); }});
          case 'getSupportedExtensions': return ()=>['EXT_color_buffer_float','EXT_texture_filter_anisotropic','OES_texture_float_linear','WEBGL_compressed_texture_s3tc','EXT_texture_compression_bptc','EXT_texture_compression_rgtc'];
          case 'getParameter': return p=>{ for(const n in PARAMS) if(glConst(n)===p) return PARAMS[n]; return 0; };
          case 'getShaderParameter': return ()=>true;
          case 'getProgramParameter': return (_,p)=> (p===glConst('ACTIVE_UNIFORMS')||p===glConst('ACTIVE_ATTRIBUTES')||p===glConst('ACTIVE_UNIFORM_BLOCKS'))? 0 : true;
          case 'getShaderInfoLog': case 'getProgramInfoLog': return ()=>'';
          case 'checkFramebufferStatus': return ()=>glConst('FRAMEBUFFER_COMPLETE');
          case 'getUniformLocation': return ()=>handle();
          case 'getAttribLocation': return ()=>0;
          case 'getUniformBlockIndex': return ()=>0;
          case 'getError': return ()=>0;
          case 'isContextLost': return ()=>false;
          case 'getContextAttributes': return ()=>({antialias:true,alpha:false,depth:true,stencil:false});
          case 'getShaderPrecisionFormat': return ()=>({rangeMin:127,rangeMax:127,precision:23});
          case 'fenceSync': return ()=>handle();
          case 'clientWaitSync': return ()=>glConst('ALREADY_SIGNALED');
          case 'getSyncParameter': return ()=>glConst('SIGNALED');
          case 'getQueryParameter': return ()=>0;
          case 'readPixels': return ()=>{};
        }
        if(/^(create|get)/.test(k)) return ()=>handle();
        return ()=>{};
      }, set(t,k,v){ t[k]=v; return true; }});
      return gl;
    }
    w.WebGL2RenderingContext=w.WebGL2RenderingContext||function(){};
    w.HTMLCanvasElement.prototype.getContext=function(kind){ if(kind==='2d') return noop2d(); if(/webgl/.test(kind)) return this.__gl||(this.__gl=stubGL(this)); return null; };
    w.HTMLCanvasElement.prototype.toDataURL=()=> 'data:,';
    w.TextDecoder=w.TextDecoder||require('util').TextDecoder; w.TextEncoder=w.TextEncoder||require('util').TextEncoder;
    w.ImageData=w.ImageData||class{constructor(a,b,c){ if(typeof a==='number'){this.width=a;this.height=b;this.data=new Uint8ClampedArray(4*a*b);} else {this.data=a;this.width=b;this.height=c||(a.length/4/b);} }};
    w.createImageBitmap=w.createImageBitmap||(async()=>({width:1,height:1,close(){}}));
    /* The page's own assets (ui/assets), served beside it as build.rs copies them - the
       bundled water_NRM.dds is fetched this way. Anything else is not there. */
    w.fetch=async url=>{
      const f=path.join(__dirname,'..','assets',path.basename(String(url).split('?')[0]));
      if(!fs.existsSync(f)) return {ok:false, status:404, arrayBuffer:async()=>new ArrayBuffer(0)};
      const b=fs.readFileSync(f);
      return {ok:true, status:200, arrayBuffer:async()=>b.buffer.slice(b.byteOffset,b.byteOffset+b.length)};
    };
    w.matchMedia=w.matchMedia||(()=>({matches:false,addEventListener(){},removeEventListener(){},addListener(){}}));
    w.ResizeObserver=class{observe(){}unobserve(){}disconnect(){}};
    w.requestAnimationFrame=f=>setTimeout(()=>f(Date.now()),16);
    w.addEventListener('error',e=>errors.push('error: '+(e.error&&e.error.stack||e.message).split('\n').slice(0,3).join(' | ')));
    w.addEventListener('unhandledrejection',e=>errors.push('rejection: '+String(e.reason&&e.reason.stack||e.reason).split('\n').slice(0,3).join(' | ')));
  }});
/* The Preview features since 4.2.0, each through the flag its code sets for the tests.
   The GL context is a stub, so these prove the paths run and set their state - not how
   the result looks (SMOKE_TEST.md rows 6b-6g are the visual check). Each check draws a
   frame itself with the options it needs, and puts them back after. */
async function features(w){
  // The page's globals are `const`s of its scripts, not window properties.
  const R=w.eval('App.R'), Sky=w.eval('Sky'), done=[];
  const fail=m=>errors.push('feature: '+m);
  const frame=()=>{ try{ R.draw(); }catch(e){ fail('a frame threw: '+short(e)); } };
  const saved={...R.opts}, weather=Sky.weather;
  const sleep=ms=>new Promise(r=>setTimeout(r,ms));
  R.opts.mgeWater=true; R.opts.room=null;
  // Waves: drawn with Waves on; not with it off, nor at Wave height 0.
  R.opts.waves=true; R.opts.waveHeight=50; frame();
  if(R.wavesDrawn!==true) fail('waves were not drawn with Waves on');
  R.opts.waves=false; frame();
  if(R.wavesDrawn!==false) fail('waves were drawn with Waves off');
  R.opts.waves=true; R.opts.waveHeight=0; frame();
  if(R.wavesDrawn!==false) fail('waves were drawn at Wave height 0');
  R.opts.waveHeight=50;
  done.push('waves');
  // Rain ripples: in the four wet weathers with Waves on; not in Clear, nor in a true interior.
  const rainIn=(wx,room)=>{ R._rainT=undefined; R.rainDrawn=false; Sky.weather=wx; R.opts.weather=wx; R.opts.room=room||null; frame(); R.opts.room=null; return R.rainDrawn===true; };
  for(const wx of ['Rain','Thunderstorm','Snow','Blizzard']) if(!rainIn(wx)) fail('no rain ripples in '+wx);
  if(rainIn('Clear')) fail('rain ripples in Clear');
  // A true interior as 18_cellpreview.js sets it: its AMBI light (a quasi-exterior has none).
  const room={ambient:[0.35,0.35,0.35], sunlight:[0.45,0.45,0.45], fog:[0.12,0.12,0.14], fogDensity:0};
  if(rainIn('Rain',room)) fail('rain ripples in a true interior');
  done.push('rain ripples');
  // Bloom: the pass runs with Bloom on, and not with it off.
  R.opts.bloom=true; frame();
  if(R.bloomDrawn!==true) fail('bloom did not run with Bloom on');
  R.opts.bloom=false; frame();
  if(R.bloomDrawn===true) fail('bloom ran with Bloom off');
  done.push('bloom');
  // Every weather: a frame without a throw, and Wonders of Water's numbers for it.
  for(const wx of Sky.WEATHERS){
    Sky.weather=wx; R.opts.weather=wx; R.opts.waterDepth=true; frame();
    const m=Sky.wowNow(null), want=Sky.WOW_WEATHER[wx];
    if(!m || !want || m.waveHeight!==want.waveHeight || m.caustics!==want.caustics) fail('Wonders of Water numbers wrong in '+wx);
    if(Math.abs(R.wowScale('waveHeight')-want.waveHeight)>1e-6) fail('the waves do not take Wonders of Water\'s height in '+wx);
    const c=Sky.at && Sky.at(12);
    if(c && JSON.stringify(c).includes('NaN')) fail('the sky ramp has NaN in '+wx);
  }
  if(Sky.wowNow(room).waveHeight!==0) fail('Wonders of Water gives waves in a true interior');
  R.opts.waterDepth=false;
  done.push('ten weathers');
  // Shadows: each Shadow detail builds its map at that size.
  Sky.weather='Clear'; R.opts.weather='Clear'; R.opts.shadows=true;
  for(const res of [512,1024,2048]){
    R.opts.shadowRes=res; R.shadowDrawn=false; frame();
    if(!(R._shT && R._shT.res===res && R._shT.ok)) fail('no '+res+' shadow map ('+JSON.stringify(R._shT&&{res:R._shT.res,ok:R._shT.ok})+')');
    if(R.shadowDrawn!==true) fail('the shadow pass did not run at '+res);
  }
  done.push('shadows');
  // The bundled water_NRM.dds: the fixture has no MGE folder, so the viewer's own is used.
  const mv=R.makeVolume; let vol=null;
  R.makeVolume=function(W,H,D,data){ vol=[W,H,D,data&&data.length]; return mv.apply(this,arguments); };
  R.dropWaterVolume(); R._wnrmBundled=undefined; frame();
  for(let i=0;i<100 && !R._wnrm;i++) await sleep(50);
  R.makeVolume=mv;
  if(!R._wnrm) fail('the bundled water_NRM.dds was not loaded');
  else if(!vol || vol[0]!==256 || vol[1]!==256 || vol[2]!==32 || vol[3]!==256*256*32*4) fail('the bundled water_NRM.dds decoded wrong: '+JSON.stringify(vol));
  else done.push('water_NRM.dds');
  Object.keys(R.opts).forEach(k=>{ if(!(k in saved)) delete R.opts[k]; });
  Object.assign(R.opts,saved); Sky.weather=weather; R.dirty=true;
  await tools(w, R, fail, done, sleep);
  console.log('features:', done.join(', '));
}

/* The review, landscape, overlay and link tools (48_wg_tools.js), against the fixture's
   Lamptown: Lamp.esm places its lamps and doors. */
async function tools(w, R, fail, done, sleep){
  const T=w.eval('WgTools'), App=w.eval('App'), d=w.document;
  const scene=async()=>{ for(let i=0;i<300 && !App._scene;i++) await sleep(50); return !!App._scene; };
  for(const id of ['acc_cell','acc_light','acc_scene','acc_water','acc_fx','acc_review','acc_land','acc_ovl','acc_links'])
    if(!d.getElementById(id)) fail('no Preview group '+id);
  const count=()=>R.pickables.length;
  const before=count();
  // Review: Lamp.esm's objects marked as placed; left out, they are gone.
  T.review='Lamp.esm'; T.without=false; T.reload();
  if(!await scene()) fail('the scene did not come back for the review');
  const added=R.pickables.filter(p=>p.mark==='added').length;
  if(!added) fail('Review a mod marked nothing Lamp.esm placed');
  if(!R.hasOverlay('review')) fail('the review drew no markers');
  T.without=true; T.reload(); await scene();
  if(count()>=before) fail('leaving Lamp.esm out did not take its objects away ('+count()+' of '+before+')');
  T.review=''; T.without=false; T.reload(); await scene();
  if(count()!==before) fail('the cell did not come back whole after the review ('+count()+' of '+before+')');
  done.push('review ('+added+' placed)');
  // Checks run and list.
  for(const k of ['float','dupes','moved','missing']){ try{ T.runCheck(k,true); }catch(e){ fail('check '+k+' threw: '+e); } }
  T.runCheck('clear',true);
  // Overlays.
  for(const k of ['lights','doors','collision','pathgrid','reach']){
    T.ovl[k]=true;
    try{ await T.overlay(k); }catch(e){ fail('overlay '+k+' threw: '+e); }
  }
  for(const k of ['lights','doors','collision']) if(!R.hasOverlay(k)) fail('the '+k+' overlay drew nothing');
  for(const k of Object.keys(T.ovl)){ T.ovl[k]=false; await T.overlay(k); }
  // The built-in Construction Set markers (R-Zero's) load, and draw as outlines.
  for(const f of ['marker_arrow','marker_travel','marker_creature','marker_character','marker_ruler']){
    const mk=await w.eval("loadMesh('__wg/"+f+".nif')");
    if(!mk || mk.err || !(mk.parts||[]).length) fail('the built-in '+f+'.nif did not load: '+(mk&&mk.err));
  }
  T.ovl.markers=true; await T.overlay('markers'); T.ovl.markers=false; await T.overlay('markers');
  // Measure.
  T.setMeasure(true); T.measure.a=[0,0,0]; T.measure.b=[300,400,0]; T.measureDraw();
  if(!/Distance 500 u/.test((d.getElementById('wgMeasure')||{}).textContent||'')) fail('the tape did not measure 500');
  T.setMeasure(false);
  done.push('overlays, measure');
  // Land: the merged-lands request goes through and the editors come back.
  T.land='overwrite'; T.reload(); await scene();
  const c0=App._scene.cells[0];
  if(!Array.isArray(c0.landEditors) || !c0.landEditors.length) fail('no land editors for the cell');
  T.land=''; T.reload(); await scene();
  // Links.
  const line=T.copySpot()||'';
  if(!/^coe 9 9\nplayer->position /.test(line)) fail('the console line is wrong: '+JSON.stringify(line));
  if(!/Lamptown/.test(T.reportText())) fail('the report does not name the cell');
  await T.whereUsed('mesh','x\\lamp.nif');
  if(!d.querySelectorAll('#wgUseList .it').length) fail('Where used found nothing for x\\lamp.nif');
  // The heat runs yellow to red with rank.
  const C=w.eval('WgCoverage');
  const lo=C.ramp(0), hi=C.ramp(1);
  if(!(lo[1]>0.8 && hi[1]<0.1 && hi[0]>0.4)) fail('the heat ramp does not run yellow to red');
  done.push('land, links, heat');
  // A teleporting door: "Go through" in the inspector and in the full help.
  const Ori=w.eval('Ori');
  const door=R.pickables.find(p=>p.door && p.door.cell);
  if(!door) fail('no teleporting door among the pickables');
  else{
    await Ori.show(door);
    if(!d.querySelector('#oriBody #oriDoorGo')) fail('the inspector has no Go through for '+door.id);
    const tfh=d.getElementById('tfhPanel');
    if(!tfh || tfh.hidden || !tfh.querySelector('#tfhDoorGo')) fail('the full help has no Go through for '+door.id);
    Ori.hide();
    done.push('door buttons');
  }
  await editor(w, R, fail, done, sleep);
}

/* The Editor's drag in the render window, with the pointer's rays stood in for: a slide
   across the ground plane, a Shift-drag turn about Z, and F onto the ground. Each is
   checked by what reaches the stand-in Wraithguard. */
async function dragging(E, R, lamp, fail, sleep){
  if(!E.selected()){ fail('the clicked object is not selectable for dragging'); return; }
  const m=lamp.m, L=[m[3],m[7],m[11]], W=lamp.wpos.slice();
  let ex=L[0];
  const gz=R.groundZ;
  R.pickAt=()=>{ const sel=E.selected(); return sel? sel.hit : lamp; };
  R.rayAt=()=>({eye:[ex, L[1], L[2]+1000], dir:[0,0,-1]});
  const sent=async()=>{
    const n=fakeWg.posts.length;
    for(let i=0;i<100 && !fakeWg.posts.slice(n).some(p=>p[0]==='editRefSet');i++) await sleep(30);
    await sleep(60);
    return fakeWg.posts.slice(n).filter(p=>p[0]==='editRefSet').map(p=>p[1]);
  };
  const near=(a,b)=>Math.abs(a-b)<0.05;
  try{
    const g=E.grab({clientX:10, clientY:10, shiftKey:false});
    if(!g) fail('a press on the selected object does not grab it');
    else{
      ex=L[0]+100; g.move({clientX:60, clientY:10, shiftKey:false});
      const posts=sent(); g.end({});
      const t=(await posts).find(b=>b.path==='translation');
      if(!t || !near(t.value[0], W[0]+100) || !near(t.value[1], W[1]) || !near(t.value[2], W[2]))
        fail('the slide was not sent as the moved position: '+JSON.stringify(t)+' from '+JSON.stringify(W));
      if(!(E.live.get(lamp.refKey.toLowerCase())||{}).translation) fail('the moved lamp is not drawn moved');
    }
    if(!E.selected()) await Ori.show(R.pickables.find(p=>p.refKey===lamp.refKey)||lamp);
    ex=L[0];
    const h=E.grab({clientX:10, clientY:10, shiftKey:true});
    if(h){
      h.move({clientX:110, clientY:10, shiftKey:true});
      const posts=sent(); h.end({});
      const r=(await posts).find(b=>b.path==='rotation');
      if(!r || !near(r.value[2], (h.state.sel.hit.rot||[0,0,0])[2]+100*E.TURN)) fail('the Shift-drag was not sent as a turn about Z: '+JSON.stringify(r));
    } else fail('a Shift-press on the selected object does not grab it');
    // F: onto the ground, its lowest point on it.
    const sel=E.selected();
    if(sel){
      const hit=sel.hit, hm=hit.m;
      R.groundZ=()=>hit.aabb.z0-500;
      R.pickRay=()=>null;
      const posts=sent();
      if(!E.drop()) fail('F did not drop the selected object');
      const t=(await posts).find(b=>b.path==='translation');
      if(!t || !near(t.value[2], hit.wpos[2]-500) || !near(t.value[0], hit.wpos[0])) fail('F did not drop it onto the ground: '+JSON.stringify(t)+' from '+JSON.stringify(hit.wpos)+' z0 '+hit.aabb.z0+' local '+hm[11]);
    } else fail('the object is not selected again after a move');
  } finally {
    delete R.pickAt; delete R.rayAt; delete R.pickRay; R.groundZ=gz;
    await E.ask('editRefRevert', {cell:fakeWg.ref.cell, origin:fakeWg.ref.origin, refr:fakeWg.ref.refr});
    await E.refreshPending();
  }
}

/* Placing a record (the Object Window's right-click: at the view's pivot): what reaches
   Wraithguard, the new reference drawn and pickable, its dialog, and taking it out. */
async function placing(w, E, R, fail, sleep){
  const d=w.document, CD=w.eval('CellData');
  const row=E.rows.find(r=>r[0]==='lamp_lit');
  if(!row){ fail('no lamp_lit row to place'); return; }
  const n0=fakeWg.posts.length;
  const v=await E.placeAt({tag:'LIGH', id:'lamp_lit', model:row[2], defined:['Lamp.esm']}, null, null);
  const sent=fakeWg.posts.slice(n0).find(p=>p[0]==='editPlace');
  if(!sent || !v){ fail('placing did not reach Wraithguard'); return; }
  const b=sent[1];
  if(b.cell!=='(9, 9)' || b.tag!=='LIGH' || b.id!=='lamp_lit' || !Array.isArray(b.translation) || b.translation.length!==3 || !Array.isArray(b.plugins))
    fail('the placement was sent wrongly: '+JSON.stringify(b));
  if(!d.querySelector('#edDlgBody [data-vec="translation"]') || !/New reference/.test(d.getElementById('edDlgTitle').textContent))
    fail('the new reference did not open in the dialog');
  const key='new:'+v.uid;
  const cell=await CD.loadCell({kind:'ext', x:9, y:9});
  const drawn=cell.refs.find(r=>r.key===key);
  if(!drawn || Math.abs(drawn.pos[0]-b.translation[0])>0.05) fail('the new reference is not in the cell the render window draws');
  let pick=null;
  for(let i=0;i<200 && !(pick=(R.pickables||[]).find(p=>p.refKey===key));i++) await sleep(30);
  if(!pick) fail('the new reference is not drawn (not pickable)');
  else{
    w.eval('Ori').show(pick);
    if(!E.selected() || E.selected().ref.uid!==v.uid) fail('picking the new reference does not select it');
    for(let i=0;i<100 && !d.getElementById('edRefRevertAll');i++) await sleep(30);
    d.getElementById('edRefRevertAll').onclick();
    for(let i=0;i<100 && Object.keys(fakeWg.news).length;i++) await sleep(30);
    for(let i=0;i<100 && E.liveNew.size;i++) await sleep(30);
    if(Object.keys(fakeWg.news).length) fail('Remove from the patch did not reach Wraithguard');
    if(E.liveNew.size) fail('the removed reference is still drawn');
  }
}

/* The Editor mode (50_wg_editor.js): the Object Window and Cell View from the engine, the
   record dialog and the pool from the stand-in Wraithguard above. */
async function editor(w, R, fail, done, sleep){
  const E=w.eval('WgEditor'), d=w.document;
  const port=fakeServer.address().port;
  const links=w.__WG_VIEW__.extra.links;
  for(const k of ['editRecord','editSet','editRevert','editRef','editRefSet','editRefRevert','editDuplicate','editPlace','editNew','editNewSet','editNewRemove','editPending','editReview']) links[k]='http://127.0.0.1:'+port+'/'+k+'?t=x';
  if(!d.getElementById('btnEditor')) fail('no Editor switch in the topbar');
  await E.enter();
  if(!d.body.classList.contains('wgEditMode')) fail('the Editor mode did not take over the page');
  if(!E.tags.some(t=>t[0]==='LIGH')) fail('the Object Window has no Light tab: '+JSON.stringify(E.tags));
  d.querySelector('#edTabs [data-tag="LIGH"]').onclick();
  for(let i=0;i<100 && !E.rows.some(r=>r[0]==='lamp_lit');i++) await sleep(30);
  const rows=()=>d.querySelectorAll('#edTable tbody tr').length;
  if(rows()<8) fail('the Light tab lists '+rows()+' rows');
  E.filter='lamp_lit'; E.drawRows();
  if(rows()!==1) fail('the filter left '+rows()+' rows');
  // The Cell View: the cells, and a cell's references.
  if(!(E.cells||[]).length) fail('the Cell View has no cells');
  await E.loadRefs('9,9');
  if(!d.querySelectorAll('#edRefList tbody tr').length) fail('the Cell View lists no references in 9,9');
  // The record dialog: from Wraithguard, a change sent and marked, a refusal shown.
  d.querySelector('#edTable tbody tr').ondblclick();
  for(let i=0;i<100 && !d.querySelector('#edDlgBody [data-path="data.radius"]');i++) await sleep(30);
  const radius=d.querySelector('#edDlgBody [data-path="data.radius"]');
  if(!radius) fail('the record dialog did not fill in');
  else{
    if(!d.querySelector('#edDlgBody [data-path="id"]').disabled) fail('the id is editable');
    radius.value='512'; radius.onchange();
    for(let i=0;i<100 && !E.edited.size;i++) await sleep(30);
    const sent=fakeWg.posts.find(p=>p[0]==='editSet');
    if(!sent || sent[1].value!==512 || sent[1].path!=='data.radius' || sent[1].tag!=='LIGH') fail('the change was not sent: '+JSON.stringify(sent));
    if(!E.isEdited('lamp_lit')) fail('the Object Window does not mark the changed record');
    if(!d.querySelector('#edDlgBody tr.edited [data-revert="data.radius"]')) fail('the dialog does not show the waiting change');
    await E.showPending();
    if(!/data\.radius/.test(d.getElementById('edPendBody').textContent)) fail('the pending list does not have the change');
    d.getElementById('edReview').onclick(); for(let i=0;i<50 && !fakeWg.reviewed;i++) await sleep(30);
    if(!fakeWg.reviewed) fail('Review did not reach Wraithguard');
    await E.change('editRevert', {path:'data.radius'});
    if(E.edited.size) fail('the revert left the record marked');
    // Delete record: the DELETED flag, and Undelete takes it off.
    d.getElementById('edDelete').onclick();
    for(let i=0;i<100 && !/DELETED/.test(fakeWg.queued.flags||'');i++) await sleep(30);
    if(!/DELETED/.test(fakeWg.queued.flags||'')) fail('Delete record did not send the DELETED flag');
    for(let i=0;i<100 && !/Undelete/.test((d.getElementById('edDelete')||{}).textContent||'');i++) await sleep(30);
    d.getElementById('edDelete').onclick();
    for(let i=0;i<100 && /DELETED/.test(fakeWg.queued.flags||'');i++) await sleep(30);
    if(/DELETED/.test(fakeWg.queued.flags||'')) fail('Undelete did not take the flag off');
    await E.change('editRevert', {});
    // Make a copy as: a record of the patch's own, in the Object Window, then removed.
    d.getElementById('edCopyId').value='lamp_copy';
    await d.getElementById('edCopy').onclick();
    const dup=fakeWg.posts.filter(p=>p[0]==='editDuplicate').pop();
    if(!dup || dup[1].newId!=='lamp_copy' || dup[1].id!=='lamp_lit') fail('the copy was not asked for: '+JSON.stringify(dup));
    if(!/lamp_copy/.test(d.getElementById('edDlgTitle').textContent)) fail('the dialog does not show the copy');
    E.filter='lamp'; E.drawRows();
    if(![...d.querySelectorAll('#edTable tbody tr')].some(tr=>tr.dataset.id==='lamp_copy')) fail('the Object Window does not list the copy');
    d.getElementById('edCopyId').value='lamp_lit';
    await d.getElementById('edCopy').onclick();
    if(!d.getElementById('edCopyId').classList.contains('bad')) fail('a taken id is not refused');
    d.getElementById('edRevertAll').onclick();
    for(let i=0;i<100 && Object.keys(fakeWg.made).length;i++) await sleep(30);
    if(Object.keys(fakeWg.made).length) fail('Remove from the patch did not reach Wraithguard');
    for(let i=0;i<100 && E.made.length;i++) await sleep(30);
    if(E.made.length) fail('the removed copy is still listed');
    E.filter='lamp_lit'; E.drawRows();
  }
  // The reference dialog, from the Cell View's right-click: what it asks Wraithguard
  // with, a nudge sent as the whole position, and the render window's copy of the cell.
  {
    const x=E.refs[0], at=x[0].lastIndexOf(':');
    fakeWg.refBase={id:x[1], pos:x[3].slice()};
    d.querySelector('#edRefList tbody tr').oncontextmenu({preventDefault(){}, shiftKey:false});
    for(let i=0;i<100 && !d.querySelector('#edDlgBody [data-vec="translation"]');i++) await sleep(30);
    const asked=fakeWg.posts.filter(p=>p[0]==='editRef').pop();
    if(!asked) fail('the reference dialog did not ask Wraithguard');
    else{
      const a=asked[1];
      if(a.cell!=='(9, 9)' || a.origin!==x[0].slice(0,at) || a.refr!==+x[0].slice(at+1) || !Array.isArray(a.plugins) || !a.plugins.length)
        fail('the reference was asked for wrongly: '+JSON.stringify(a));
      const plus=d.querySelector('#edDlgBody [data-nudge="translation"][data-i="0"][data-s="1"]');
      if(!plus) fail('the reference dialog has no nudge');
      else{
        plus.onclick({shiftKey:false});
        for(let i=0;i<100 && !E.live.size;i++) await sleep(30);
        const sent=fakeWg.posts.filter(p=>p[0]==='editRefSet').pop();
        if(!sent || sent[1].path!=='translation' || Math.abs(sent[1].value[0]-(x[3][0]+E.NUDGE))>0.11) fail('the nudge was not sent as the position: '+JSON.stringify(sent));
        const key=x[0].toLowerCase();
        const CD=w.eval('CellData');
        const cell=await CD.loadCell({kind:'ext', x:9, y:9});
        const moved=cell.refs.find(r=>String(r.key).toLowerCase()===key);
        if(!moved || !moved.edited || Math.abs(moved.pos[0]-(x[3][0]+E.NUDGE))>0.11) fail('the render window does not draw the moved reference: '+JSON.stringify(moved));
        const cached=[...CD.cache.values()].find(c=>c.refs && c.refs.some(r=>String(r.key).toLowerCase()===key));
        if(cached && cached.refs.find(r=>String(r.key).toLowerCase()===key).edited) fail('the overlay changed the cached cell');
        const del=d.querySelector('#edDlgBody [data-rpath="deleted"]');
        del.checked=true; del.onchange();
        for(let i=0;i<100 && !(E.live.get(key)||{}).deleted;i++) await sleep(30);
        if((await CD.loadCell({kind:'ext', x:9, y:9})).refs.some(r=>String(r.key).toLowerCase()===key)) fail('a deleted reference is still drawn');
        const scale=d.querySelector('#edDlgBody [data-rpath="scale"]');
        scale.value='3'; scale.onchange(); await sleep(150);
        if(!d.querySelector('#edDlgBody [data-rpath="scale"]').classList.contains('bad')) fail('a refused scale is not marked');
        // A teleport door: switched on, a cell typed, switched off - the whole
        // destination each time, then none.
        const lastSet=()=>fakeWg.posts.filter(p=>p[0]==='editRefSet').pop()[1];
        const door=()=>d.querySelector('#edDlgBody [data-door="on"]');
        door().checked=true; door().onchange();
        for(let i=0;i<100 && lastSet().path!=='destination';i++) await sleep(30);
        let v=lastSet().value;
        if(!v || !Array.isArray(v.translation) || v.translation.length!==3 || v.cell!=='') fail('switching teleport on did not send a destination: '+JSON.stringify(v));
        await sleep(100);
        const cellIn=d.querySelector('#edDlgBody [data-door="cell"]');
        if(!cellIn || d.querySelector('#edDlgBody [data-door-box]').hidden) fail('the destination inputs are not shown');
        else{
          cellIn.value='Lamp Cellar'; cellIn.onchange();
          for(let i=0;i<100 && (lastSet().value||{}).cell!=='Lamp Cellar';i++) await sleep(30);
          if((lastSet().value||{}).cell!=='Lamp Cellar') fail('the destination cell was not sent');
          const lr=CD && (await CD.loadCell({kind:'ext', x:9, y:9})).refs.find(r=>String(r.key).toLowerCase()===key);
          if(lr && !(lr.door && lr.door.cell==='Lamp Cellar')) fail('the render window does not carry the new destination');
          await sleep(100);
          door().checked=false; door().onchange();
          for(let i=0;i<100 && lastSet().value!==null;i++) await sleep(30);
          if(lastSet().value!==null) fail('switching teleport off did not clear the destination');
        }
        await E.showPending();
        if(!d.querySelector('#edPendBody [data-ref]')) fail('the pending list has no reference change to open');
        await E.refChange('editRefRevert', {});
        if(E.live.size) fail('the revert left the reference drawn moved');
      }
    }
  }
  // The inspector's "Edit record" and "Edit reference" for a clicked object.
  const Ori=w.eval('Ori'), lamp=R.pickables.find(p=>p.id==='lamp_lit');
  if(lamp){
    await Ori.show(lamp);
    for(let i=0;i<100 && !d.getElementById('edOriEdit');i++) await sleep(30);
    if(!d.getElementById('edOriEdit')) fail('the inspector has no Edit record in the Editor');
    if(!d.getElementById('edOriRef')) fail('the inspector has no Edit reference in the Editor');
    await dragging(E, R, lamp, fail, sleep);
    Ori.hide();
    await placing(w, E, R, fail, sleep);
  }
  E.leave();
  if(d.body.classList.contains('wgEditMode')) fail('leaving the Editor left the page in it');
  done.push('editor');
}

// ORI: a few seconds before the report, inspect the first pickable object.
const WAIT=+process.argv[3]||20000;
if(!MESH) setTimeout(()=>{ try{ dom.window.eval('(()=>{ const p=(App.R&&App.R.pickables||[])[0]; if(p) Ori.show(p); })()'); }
  catch(e){ errors.push('ori: '+short(e)); } }, Math.max(1000,WAIT-4000));
setTimeout(async ()=>{
  const d=dom.window.document;
  // A promise (a check that clicks and waits) is waited for.
  if(process.env.WG_DEBUG) console.log('debug:', await dom.window.eval(process.env.WG_DEBUG));
  console.log('calls:',[...new Set(calls)].join(' '));
  const w=dom.window;
  if(MESH){
    // The mesh viewer: the install opened for its files only, no cell work at all, and
    // the meshes drawn with their shape list.
    for(const c of ['preview_cell','world_map','cell_data']) if(calls.includes(c)) errors.push('the mesh viewer ran '+c);
    const n=w.eval("(App.R&&App.R.statics||[]).length");
    if(!n) errors.push('the mesh viewer drew nothing');
    if(!w.eval("!!(App.R&&App.R.statics.some(b=>b.nrm))")) errors.push('the normal map beside tr.dds was not found');
    if(!w.eval("!!(App.R&&App.R.statics.some(b=>b.spec))")) errors.push('the specular map beside tr.dds was not found');
    console.log('mesh view:', n, 'batches;', w.eval("(document.getElementById('mvStats')||{}).textContent||''"));
    console.log('page errors:', errors.length); errors.slice(0,40).forEach(e=>console.log(' -',e));
    try{ eng.kill(); }catch(_){ } process.exit(errors.length?1:0);
  }
  const loaded=calls.includes('cell_data') && !!w.eval('App._scene');
  if(!loaded) errors.push('the cell never loaded (no cell_data / scene)');
  for(const c of ['preview_cell','interior_scatter','cull_field','avoid_verdict','set_config','config_toml','parse_profile'])
    if(calls.includes(c)) errors.push('the page still asks for grass generation: '+c);
  const ori=(d.getElementById('oriBody')||{}).textContent||'';
  if(loaded && !/Created by/.test(ori)) errors.push('ORI did not fill in for the first object');
  else if(ori) console.log('ori:', ori.replace(/\s+/g,' ').slice(0,200));
  if(loaded && !d.querySelector('#oriBody details.oriacc')) errors.push('ORI sections are not folded into accordions');
  if(loaded) try{ await features(w); }catch(e){ errors.push('feature checks: '+short(e)); }
  console.log('scene:', w.eval("(document.getElementById('stats')||{}).textContent||''").replace(/\s+/g,' ').slice(0,160));
  console.log('page errors:', errors.length); errors.slice(0,40).forEach(e=>console.log(' -',e));
  try{ eng.kill(); }catch(_){ } process.exit(errors.length?1:0);
}, WAIT);
