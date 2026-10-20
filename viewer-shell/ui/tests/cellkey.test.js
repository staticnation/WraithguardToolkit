'use strict';
/* The shared cell cases (tests/fixtures/cell_cases.json at the repository's root, which
   ordercore and the Python side run too) through the page's CellKey (38_wg_cellkey.js). */
const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const vm=require('node:vm');

const ctx=vm.createContext({});
vm.runInContext(fs.readFileSync(path.join(__dirname,'../src/38_wg_cellkey.js'),'utf8')+';this.CellKey=CellKey;',ctx);
const CellKey=ctx.CellKey;
const CASES=JSON.parse(fs.readFileSync(path.join(__dirname,'../../../tests/fixtures/cell_cases.json'),'utf8')).cases;

for(const c of CASES){
  test('reads '+JSON.stringify(c.in)+' as the cases say',()=>{
    assert.equal(CellKey.fold(c.in), c.fold);
    if(c.fold==null){ assert.equal(CellKey.parse(c.in), null); return; }
    assert.equal(CellKey.spec(c.in), c.spec);
    assert.equal(CellKey.pool(c.in), c.pool);
    assert.deepEqual(CellKey.grid(c.in) && [...CellKey.grid(c.in)], c.grid);
    // The page's and the pool's spellings read back as the same cell.
    assert.equal(CellKey.fold(CellKey.spec(c.in)), c.fold);
    assert.equal(CellKey.fold(c.grid? CellKey.pool(c.in) : 'int:'+CellKey.pool(c.in)), c.fold);
  });
}

test('cell objects read as their spellings do',()=>{
  assert.equal(CellKey.spec({kind:'ext', x:-2, y:9}), '-2,9');
  assert.equal(CellKey.spec({gx:3, gy:4}), '3,4');
  assert.equal(CellKey.spec({kind:'int', name:'1,2'}), 'int:1,2');
  assert.ok(CellKey.same({kind:'int', name:'Vivec, Arena'}, 'vivec, arena'));
  assert.ok(!CellKey.same('int:1,2', '1,2'));
  assert.equal(CellKey.parse({kind:'int', name:' '}), null);
});

test('the viewer target and the door key keep the name as written',()=>{
  assert.deepEqual({...CellKey.target('int:Vivec, Arena')}, {kind:'int', name:'Vivec, Arena', label:'Vivec, Arena'});
  assert.deepEqual({...CellKey.target('(-2, 9)')}, {kind:'ext', x:-2, y:9, name:'', label:'-2, 9'});
  assert.equal(CellKey.target('int:'), null);
  assert.equal(CellKey.doorKey('int:Vivec, Arena'), 'i:Vivec, Arena');
  assert.equal(CellKey.doorKey({kind:'ext', x:1, y:2, name:'', label:'1, 2'}), '1,2');
});
