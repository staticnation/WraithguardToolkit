'use strict';
/* `animVisibleAt` (10_preview.js): a NiVisController that clamps stands at its first frame, one
   that loops runs on the clock. The keys are the bonelord's own (`xtr_cr_bonelord_grt_01.nif`:
   Hood Node and Robe Node, controller flags 0xc = active + clamp). */
const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const vm=require('node:vm');

const src=fs.readFileSync(path.join(__dirname,'../src/10_preview.js'),'utf8');
const cut=(from,to)=>{ const a=src.indexOf(from), b=src.indexOf(to,a); assert.ok(a>=0 && b>a, from); return src.slice(a,b); };
const ctx=vm.createContext({});
vm.runInContext(cut('function ctrlTime(','/** Linear key lookup')+cut('function animVisibleAt(','/** Hidden: every vertex')+';this.animVisibleAt=animVisibleAt;',ctx);
const visible=(anim,secs)=>ctx.animVisibleAt(anim,secs);

const HOOD={flags:0xc, freq:1, phase:0, start:0.06666667, stop:22.6667, keys:[[0.067,1],[18.467,1],[18.533,0],[20.667,0]]};
const ROBE={flags:0xc, freq:1, phase:0, start:0.06666667, stop:22.6667, keys:[[0,0],[18.333,0],[18.4,1],[20.6,1],[20.667,0]]};

test('the hood is on and the robe off at the first frame', ()=>{
  assert.equal(visible({vis:[HOOD]}, 0), true);
  assert.equal(visible({vis:[ROBE]}, 0), false);
});

test('a clamped controller does not run out with the clock: a minute in, the hood is still on', ()=>{
  for(const secs of [5, 22.7, 60, 3600]){
    assert.equal(visible({vis:[HOOD]}, secs), true, 'hood at '+secs);
    assert.equal(visible({vis:[ROBE]}, secs), false, 'robe at '+secs);
  }
});

test('a looping controller still runs on the clock', ()=>{
  const flicker={flags:0x8, freq:1, phase:0, start:0, stop:2, keys:[[0,1],[1,0],[2,1]]};
  assert.equal(visible({vis:[flicker]}, 0.5), true);
  assert.equal(visible({vis:[flicker]}, 1.5), false);
  assert.equal(visible({vis:[flicker]}, 2.5), true);    // round again
  assert.equal(visible({vis:[flicker]}, 3.5), false);
});

test('every controller over the part must say shown', ()=>{
  assert.equal(visible({vis:[HOOD, ROBE]}, 0), false);
  assert.equal(visible({vis:[]}, 0), true);
  assert.equal(visible(null, 0), true);
});
