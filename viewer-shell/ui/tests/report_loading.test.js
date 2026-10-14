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
