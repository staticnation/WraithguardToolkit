'use strict';
const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const {page}=require('./editor/harness');
const src=name=>fs.readFileSync(path.join(__dirname,'../src',name),'utf8');

function reportPage(){
  const styles=[...src('01_head.html').matchAll(/<style[^>]*>([\s\S]*?)<\/style>/g)].map(m=>m[0]).join('');
  const p=page({html:styles+src('02_body.html'),modules:['src/10_preview.js','src/16_gamedata_ui.js']});
  const events=src('11_events.js');
  p.w.eval(events.slice(events.indexOf('function logRowText('),events.indexOf('\n\n\n\n\n/* =====================================================================================\n   Starting up.')));
  p.w.eval(`
    function clamp(v,a,b){return Math.max(a,Math.min(b,v));}
    var GameData={plugins:[],missing:[]};
    App.R=null; App.texGL=new Map(); App.meshCache=new Map(); App.loadLog=[];
    LoadProf.pending.set(33,{started:performance.now()-176000,stage:'requesting',
      meshes:Array.from({length:8},(_,i)=>'Meshes/folder/full_mesh_'+i+'.nif')});
    LoadProf.decoding.set(50,{started:performance.now(),path:'Textures/full_texture.png'});
    document.getElementById('mLogCopy').onclick=()=>logCopyWhenReady(document.getElementById('logBody'),()=>logAllText(document.getElementById('logBody')),document.getElementById('mLogCopy'));
    document.getElementById('mLogX').onclick=()=>document.getElementById('mLog').hidden=true;
  `);
  return p;
}

test('Report remains above repeated busy refreshes and copying keeps every mesh path',()=>{
  const {w,d}=reportPage();
  try{
    w.Busy.show('Placing objects','184 of 192 meshes',.84);
    w.openLog();
    for(let i=0;i<3;i++) w.Busy.show('Placing objects','184 of 192 meshes',.84);
    const report=d.getElementById('mLog'), busy=d.getElementById('busy');
    assert.equal(report.hidden,false); assert.equal(busy.hidden,false);
    assert.ok(busy.classList.contains('overall'),'reproduce the refresh that raises the overlay');
    assert.ok(+w.getComputedStyle(report).zIndex > +w.getComputedStyle(busy).zIndex);
    const body=d.getElementById('logBody'), heading=body.firstElementChild;
    assert.match(heading.textContent,/Loading now/);
    const text=w.logSectionText(heading);
    assert.match(text,/bundle #33 requesting/);
    for(let i=0;i<8;i++) assert.ok(text.includes('Meshes/folder/full_mesh_'+i+'.nif'));
    assert.match(text,/Textures\/full_texture.png/); assert.ok(!text.includes('…'));
    assert.ok(w.logAllText(body).includes(text));
    let copied=''; w.logCopy=t=>{copied=t;};
    heading.querySelector('button').click(); assert.equal(copied,text);
    copied=''; d.getElementById('mLogCopy').click(); assert.equal(copied,w.logAllText(body));
    d.getElementById('mLogX').click(); assert.equal(report.hidden,true);
    assert.equal(busy.hidden,false,'closing Report must leave the load running');
  }finally{w.close();}
});

test('full diagnostic report includes all bundles and decodes while the busy card stays compact',()=>{
  const {w}=reportPage();
  try{
    w.eval(`for(let i=0;i<5;i++){
      LoadProf.pending.set(100+i,{started:performance.now(),stage:'requesting',meshes:['Meshes/extra_'+i+'.nif']});
      LoadProf.decoding.set(200+i,{started:performance.now(),path:'Textures/extra_'+i+'.png'});
    }`);
    assert.match(w.LoadProf.live(),/more bundles pending/);
    const full=w.LoadProf.live(true);
    for(let i=0;i<5;i++){
      assert.ok(full.includes('Meshes/extra_'+i+'.nif'));
      assert.ok(full.includes('Textures/extra_'+i+'.png'));
    }
    w.LoadProf.reset(); w.openLog();
    assert.ok(!w.document.getElementById('logBody').textContent.includes('Loading now'));
  }finally{w.close();}
});

test('a load\'s Report entry gives its timeline, its textures and every bundle, slowest first',()=>{
  const {w}=reportPage();
  try{
    w.eval(`App.loadTimes=[{label:'(-35, 27) + 24 cells', t0:0, cells:400, terrain:1200, objects:9000, meshes:90, textures:120,
      detail:512, marks:{placed:10600, grass:10650, grassDone:13900, land:10610, landDone:11800, far:11800},
      land:{cells:180, triangles:50000, radius:12}, far:null,
      prof:{engine:6000, engineKnown:true, wait:9000, bundles:2, tex:300, texN:3, texBytes:4e6, mesh:200, meshN:16, build:500, groupsMs:20,
        list:[
          {id:1, ms:800, engine:500, bytes:2e6, meshes:['meshes/a.nif','meshes/b.nif'],
           tex:[{name:'tx_a.dds', kind:'bc1', w:512, h:512, gpu:174000, dec:2, ext:'dds', srcW:4096, srcH:4096, read:350000, cut:'dds', us:4000}]},
          {id:2, ms:5200, engine:4800, bytes:9e6, meshes:['meshes/c.nif'],
           tex:[{name:'tx_big.tga', kind:'rgba', w:512, h:512, gpu:1e6, dec:40, ext:'tga', srcW:4096, srcH:4096, read:48e6, cut:'halved', us:3900000},
                {name:'tx_flat.dds', kind:'rgba', w:2048, h:2048, gpu:16e6, dec:60, ext:'dds', srcW:2048, srcH:2048, read:16e6, cut:'', us:200000}]}]}}];`);
    w.openLog();
    const text=w.document.getElementById('logBody').textContent;
    assert.match(text,/timeline from the start: cells read 0\.4 s · terrain 1\.6 s · objects placed 10\.6 s · grass 10\.7 s→13\.9 s · distant land 10\.6 s→11\.8 s · distant objects 11\.8 s→\(not finished\)/);
    assert.match(text,/distant land 180 cells/);
    assert.match(text,/textures by format: tga 1 \(46 MB read, engine 3\.9 s\) · dds 2 \(16 MB read/);
    assert.match(text,/texture detail 512 px: 1 DDS read in part · 1 read whole and cut here \(46 MB: tx_big\.tga 46 MB\) · 1 sent above it \(tx_flat\.dds 2048×2048\)/);
    const slow=text.indexOf('#2 5.2 s'), fast=text.indexOf('#1 0.8 s');
    assert.ok(slow>0 && fast>slow,'slowest bundle first');
    assert.match(text,/meshes: meshes\/c\.nif · textures \(2\): tx_big\.tga tga 4096×4096→512×512 rgba \[halved\] 46 MB read, engine 3900 ms, page 40 ms; tx_flat\.dds dds 2048×2048 rgba 15 MB read/);
  }finally{w.close();}
});
