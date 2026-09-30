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
      : {cfg:CFG, cell:CELL};
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
    w.matchMedia=w.matchMedia||(()=>({matches:false,addEventListener(){},removeEventListener(){},addListener(){}}));
    w.ResizeObserver=class{observe(){}unobserve(){}disconnect(){}};
    w.requestAnimationFrame=f=>setTimeout(()=>f(Date.now()),16);
    w.addEventListener('error',e=>errors.push('error: '+(e.error&&e.error.stack||e.message).split('\n').slice(0,3).join(' | ')));
    w.addEventListener('unhandledrejection',e=>errors.push('rejection: '+String(e.reason&&e.reason.stack||e.reason).split('\n').slice(0,3).join(' | ')));
  }});
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
  console.log('scene:', w.eval("(document.getElementById('stats')||{}).textContent||''").replace(/\s+/g,' ').slice(0,160));
  console.log('page errors:', errors.length); errors.slice(0,40).forEach(e=>console.log(' -',e));
  try{ eng.kill(); }catch(_){ } process.exit(errors.length?1:0);
}, WAIT);
