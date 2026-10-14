// Exercise the real bundle parser, texture decoder and cache with controlled completions.
'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function deferred(){
  let resolve, reject;
  const promise=new Promise((res,rej)=>{ resolve=res; reject=rej; });
  return {promise,resolve,reject};
}
function texture(){
  const header=Buffer.from(JSON.stringify({kind:'file',w:1,h:1,levels:[{w:1,h:1,bytes:1}]}));
  const buf=Buffer.alloc(8+header.length+1);
  buf.write('GDN2'); buf.writeUInt32LE(header.length,4); header.copy(buf,8);
  return buf.buffer.slice(buf.byteOffset,buf.byteOffset+buf.length);
}
function bundle(names=['textures/shared.png']){
  const pieces=[Buffer.from('GDNB')];
  const u32=n=>{ const b=Buffer.alloc(4); b.writeUInt32LE(n); pieces.push(b); };
  u32(0); u32(names.length);
  for(const name of names){
    const str=Buffer.from(name), len=Buffer.alloc(2); len.writeUInt16LE(str.length);
    pieces.push(len,str,Buffer.from([1]));
    const data=Buffer.from(texture()); u32(data.length); pieces.push(data);
  }
  const buf=Buffer.concat(pieces);
  return buf.buffer.slice(buf.byteOffset,buf.byteOffset+buf.length);
}
function page(){
  const requests=[], decodes=[], intervals=new Map(); let timer=0;
  const App={texGL:new Map(),meshCache:new Map(),loadLog:[],textureDetail:512,swapEpoch:0,
    R:{bcFormats:()=>false,makeTexture:img=>img,gl:{deleteTexture(){}}}};
  const ctx=vm.createContext({App, TextDecoder, performance, console,
    VFS:{norm:s=>s.replace(/\\/g,'/'),ok:()=>true}, T:k=>k,
    Engine:{bytes(c,a){ const d=deferred(); requests.push({c,a,...d}); return d.promise; }},
    decodeViaBrowser(){ const d=deferred(); decodes.push(d); return d.promise; },
    rgbaToDataURL:()=>'',
    setInterval:fn=>{ const id=++timer; intervals.set(id,fn); return id; },
    clearInterval:id=>intervals.delete(id)});
  const core=fs.readFileSync(path.join(__dirname,'../src/03_core.js'),'utf8');
  vm.runInContext(core.slice(core.indexOf('async function inFlight('),core.indexOf('\nconst nf =')),ctx);
  vm.runInContext(fs.readFileSync(path.join(__dirname,'../src/10_preview.js'),'utf8'),ctx);
  // Mesh geometry is unrelated to cache ordering; leave missing mesh records in the bundle.
  vm.runInContext('newMeshRec=path=>({path}); attachMeshTextures=async()=>{};',ctx);
  return {App,requests,decodes,intervals,run:s=>vm.runInContext(s,ctx)};
}
const flush=async()=>{ for(let i=0;i<12;i++) await Promise.resolve(); };
const image=n=>({w:1,h:1,data:new Uint8Array([n,0,0,255])});

test('old bundle decode cannot delete or populate the replacement texture load',async()=>{
  const p=page(), old=p.run("loadMeshes(['old.nif'])");
  p.requests[0].resolve(bundle()); await flush();
  assert.equal(p.decodes.length,1);
  assert.match(p.run('LoadProf.live()'),/decoding textures/);
  assert.match(p.run('LoadProf.live()'),/decoding textures\/shared.png/);
  p.run('dropTextures()');
  const current=p.run("loadTexture('textures/shared.png')");
  const shared=p.run("loadTexture('textures/shared.png')"); assert.equal(current,shared);
  p.decodes[0].resolve(image(1)); await old;
  assert.equal(p.App.texGL.size,0,'old decode must not refill a cleared cache');
  assert.equal(p.run("loadTexture('textures/shared.png')"),current,'replacement promise survives older cleanup');
  assert.equal(p.requests.length,2);
  p.requests[1].resolve(texture()); await flush(); p.decodes[1].resolve(image(2));
  const rec=await current; assert.equal(p.App.texGL.get('textures/shared.png'),rec);
  assert.equal(p.run('_texLoading.size'),0); assert.equal(p.run('LoadProf.decoding.size'),0);
});

test('late bundle after cache reset does not register or decode obsolete textures',async()=>{
  const p=page(), old=p.run("loadMeshes(['old.nif'])");
  p.run('dropTextures()');
  p.requests[0].resolve(bundle()); await old;
  assert.equal(p.decodes.length,0); assert.equal(p.App.texGL.size,0);
  assert.equal(p.run('_texLoading.size'),0); assert.equal(p.run('LoadProf.pending.size'),0);
});

test('standalone decode finishing after replacement cannot overwrite its cache record',async()=>{
  const p=page(), old=p.run("loadTexture('textures/shared.png')");
  p.requests[0].resolve(texture()); await flush();
  p.run('dropTextures()'); const current=p.run("loadTexture('textures/shared.png')");
  p.requests[1].resolve(texture()); await flush(); p.decodes[1].resolve(image(2));
  const rec=await current; p.decodes[0].resolve(image(1)); await old;
  assert.equal(p.App.texGL.get('textures/shared.png'),rec);
  assert.notEqual(rec.gl,null,'old completion must not delete the new GPU texture');
});

test('concurrent bundles share a pending decode and wait for it',async()=>{
  const p=page(), a=p.run("loadMeshes(['a.nif'])"), b=p.run("loadMeshes(['b.nif'])");
  p.requests[0].resolve(bundle()); await flush(); p.requests[1].resolve(bundle()); await flush();
  let finished=false; b.then(()=>{ finished=true; }); await flush();
  assert.equal(p.decodes.length,1); assert.equal(finished,false);
  p.decodes[0].resolve(image(1)); await Promise.all([a,b]);
  assert.equal(p.run('LoadProf.pending.size'),0);
});

test('pending diagnostics refresh during a stall and timer clears on completion',async()=>{
  const p=page(); const progress=[]; p.run('progress=[]');
  const load=p.run("loadMeshes(['a.nif'],(done,total)=>progress.push([done,total,LoadProf.live()]))");
  assert.equal(p.intervals.size,1);
  [...p.intervals.values()][0](); progress.push(...p.run('progress'));
  assert.equal(progress[0][0],0); assert.equal(progress[0][1],1);
  assert.match(progress[0][2],/bundle #\d+ requesting/);
  p.requests[0].resolve(bundle([])); await load;
  assert.equal(p.intervals.size,0); assert.equal(p.run('LoadProf.pending.size'),0);
});

test('decode failure clears diagnostics and permits the mesh load to finish',async()=>{
  const p=page(), load=p.run("loadMeshes(['a.nif'])");
  p.requests[0].resolve(bundle()); await flush(); p.decodes[0].reject(new Error('bad image')); await load;
  assert.equal(p.App.texGL.get('textures/shared.png').err,'bad image');
  assert.equal(p.run('_texLoading.size'),0); assert.equal(p.run('LoadProf.decoding.size'),0);
});

test('bundles queued before reset retain the original texture generation',async()=>{
  const p=page(), load=p.run("loadMeshes(Array.from({length:49},(_,i)=>'mesh'+i+'.nif'))");
  assert.equal(p.requests.length,6);
  p.run('dropTextures(); App.textureDetail=256');
  for(const r of p.requests) r.resolve(bundle([]));
  await flush(); assert.equal(p.requests.length,7);
  assert.equal(p.requests[6].a.maxSize,512,'queued request keeps its original detail');
  p.requests[6].resolve(bundle()); await load;
  assert.equal(p.decodes.length,0,'old detail must not enter the new cache');
});

test('install switch discards returned bundle textures before registration',async()=>{
  const p=page(), load=p.run("loadMeshes(['a.nif'])");
  p.run('App.swapEpoch++'); p.requests[0].resolve(bundle()); await load;
  assert.equal(p.decodes.length,0); assert.equal(p.App.texGL.size,0);
});

test('failed bundle and fallback reads clear pending diagnostics and refresh timer',async()=>{
  const p=page(), load=p.run("loadMeshes(['a.nif'],()=>{})");
  p.requests[0].reject(new Error('bundle unavailable')); await flush();
  assert.equal(p.requests[1].c,'mesh_data');
  assert.match(p.run('LoadProf.live()'),/fallback mesh reads/);
  p.requests[1].reject(new Error('mesh missing')); await load;
  assert.equal(p.run('LoadProf.pending.size'),0); assert.equal(p.intervals.size,0);
});

test('mesh reset in the same world keeps late standalone reads out of the cache',async()=>{
  const p=page(), old=p.run("loadMesh('a.nif')");
  p.run('dropMeshes()');
  const current=p.run("loadMesh('a.nif')");
  p.requests[1].reject(new Error('current mesh')); const rec=await current;
  p.requests[0].reject(new Error('old mesh')); await old;
  assert.equal(p.App.meshCache.get('a.nif'),rec);
  assert.equal(p.run('_meshLoading.size'),0);
});

test('mesh reset keeps a late bundle from replacing a newer mesh record',async()=>{
  const p=page(), old=p.run("loadMeshes(['a.nif'])");
  p.run('dropMeshes()');
  const current=p.run("loadMeshes(['a.nif'])");
  p.requests[1].resolve(bundle([])); const [rec]=await current;
  p.requests[0].resolve(bundle([])); await old;
  assert.equal(p.App.meshCache.get('a.nif'),rec);
});

test('older mesh cleanup cannot release a replacement request',async()=>{
  const p=page(), old=p.run("loadMesh('a.nif')");
  p.run('dropMeshes()'); const current=p.run("loadMesh('a.nif')");
  p.requests[0].reject(new Error('old mesh')); await old;
  assert.equal(p.run("loadMesh('a.nif')"),current);
  p.requests[1].reject(new Error('current mesh')); await current;
});

for(const replacement of [false,true]) test('late mesh thumbnail cannot repopulate or overwrite the reset cache: '+replacement,async()=>{
  const p=page();
  p.run(fs.readFileSync(path.join(__dirname,'../src/46_wg_tfh.js'),'utf8'));
  p.run(`var document={createElement:()=>({})};
    var img={dataset:{model:'body.nif'},replaceWith(){}};
    App.R.thumbBegin=()=>null;`);
  const old=p.run('Tfh.meshIcon(img)');
  p.run('dropMeshes()');
  if(replacement) p.run("_meshThumb.set('body.nif','current thumbnail')");
  p.requests[0].resolve(bundle([])); await old;
  assert.equal(p.run("_meshThumb.get('body.nif')"),replacement?'current thumbnail':undefined);
});
