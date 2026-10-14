'use strict';
const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const vm=require('node:vm');

function deferred(){
  let resolve,reject;
  const promise=new Promise((res,rej)=>{resolve=res;reject=rej;});
  return {promise,resolve,reject};
}
function page(){
  const requests=[],fallbacks=[];
  const ctx=vm.createContext({console,SKY_GLSL:'',Renderer:function(){},App:{},VFS:{ok:()=>true},
    loadTexture(){const d=deferred();requests.push(d);return d.promise;}});
  for(const name of ['16_gamedata_ui.js','25_sky.js','26_water.js']){
    vm.runInContext(fs.readFileSync(path.join(__dirname,'../src',name),'utf8'),ctx);
  }
  vm.runInContext('var R=new Renderer(); App.R=R;',ctx);
  ctx.R.bundledWaterVolume=()=>{const d=deferred();fallbacks.push(d);return d.promise;};
  return {requests,fallbacks,R:ctx.R,run:s=>vm.runInContext(s,ctx)};
}
const flush=async()=>{for(let i=0;i<12;i++) await Promise.resolve();};

test('retired busy claimant cannot repaint or finish a later claim',()=>{
  const p=page();
  p.run(`var paints=[],hidden=0;
    Busy._paint=(...a)=>paints.push(a); Busy.hide=()=>hidden++; Busy._sub=t=>paints.push(t);
    var old=Busy.claim(); old.done(); var current=Busy.claim(); current.show('current');
    old.show('obsolete'); old.sub('obsolete'); old.done();`);
  assert.equal(p.run('paints.length'),1);
  assert.equal(p.run('hidden'),1);
  p.run("current.sub('current progress'); current.done()");
  assert.equal(p.run('paints.length'),2); assert.equal(p.run('hidden'),2);
});

test('free invalidates old progress callbacks even after another claim',()=>{
  const p=page();
  p.run(`var paints=0; Busy._paint=()=>paints++; Busy.hide=()=>{};
    var old=Busy.claim(); Busy.free(); var current=Busy.claim(); old.show('old'); current.show('new');`);
  assert.equal(p.run('paints'),1);
});

for(const failure of [false,true]) test('late cloud '+(failure?'failure':'success')+' cannot overwrite the new cloud texture',async()=>{
  const p=page();
  p.run("Sky.cloudTex('cloud.dds'); Sky.dropClouds(); Sky.cloudTex('cloud.dds')");
  const gl={current:true}; p.requests[1].resolve({gl}); await flush();
  if(failure) p.requests[0].reject(new Error('old failure')); else p.requests[0].resolve({gl:{old:true}});
  await flush(); assert.equal(p.run("Sky.cloudTex('cloud.dds')"),gl);
  assert.equal(p.requests.length,2);
});

test('cloud completion after reset does not refill an empty cloud cache',async()=>{
  const p=page(); p.run("Sky.cloudTex('cloud.dds'); Sky.dropClouds()");
  p.requests[0].resolve({gl:{}}); await flush();
  assert.equal(p.run('Sky._clouds.size'),0);
});

test('late install water texture cannot replace the new volume',async()=>{
  const p=page(); p.R.waterVolume(); p.R.dropWaterVolume(); p.R.waterVolume();
  const gl={current:true}; p.requests[1].resolve({gl,is3d:true}); await flush();
  p.requests[0].resolve({gl:{old:true},is3d:true}); await flush();
  assert.equal(p.R.waterVolume(),gl); assert.equal(p.fallbacks.length,0);
});

test('late water fallback cannot replace a new install volume',async()=>{
  const p=page(); p.R.waterVolume(); p.requests[0].resolve(null); await flush();
  assert.equal(p.fallbacks.length,1);
  p.R.dropWaterVolume(); p.R.waterVolume();
  const gl={current:true}; p.requests[1].resolve({gl,is3d:true}); await flush();
  p.fallbacks[0].resolve({old:true}); await flush(); assert.equal(p.R.waterVolume(),gl);
});

for(const failure of [false,true]) test('obsolete water '+(failure?'failure':'missing result')+' cannot start a fallback after reset',async()=>{
  const p=page(); p.R.waterVolume(); p.R.dropWaterVolume();
  if(failure) p.requests[0].reject(new Error('old failure')); else p.requests[0].resolve(null);
  await flush(); assert.equal(p.fallbacks.length,0); assert.equal(p.R._wnrm,undefined);
});
