
/* =====================================================================================
   A cell, named the one way - Wraithguard. The page's copy of `ordercore::cells`, which
   the engine and the Editor's pool use: `int:Name` (any case of `int:`), `x,y`, `x, y`,
   `(x, y)` and a bare name (an interior) all read as one cell, and two spellings of one
   cell share a `fold`. Each panel used to split specs itself (`startsWith('int:')`,
   `split(',')`, a regex here, a paren strip there), and each read a different set.
   Cases: tests/fixtures/cell_cases.json, run by ui/tests/cellkey.test.js as well as by
   ordercore's tests and the Python ones.
   ===================================================================================== */
const CellKey={
  /** Any spelling, or a cell object (`{kind:'int', name}`, `{kind:'ext', x, y}` or
   *  `{gx, gy}`), to `{kind:'int', name}` or `{kind:'ext', x, y}`; null for no name. */
  parse(spec){
    if(spec && typeof spec==='object'){
      if(spec.kind==='int'){ const n=String(spec.name==null? '' : spec.name).trim(); return n? {kind:'int', name:n} : null; }
      const x=spec.x!=null? spec.x : spec.gx, y=spec.y!=null? spec.y : spec.gy;
      return Number.isInteger(+x) && Number.isInteger(+y)? {kind:'ext', x:+x, y:+y} : null;
    }
    const s=String(spec==null? '' : spec).trim();
    if(s.slice(0,4).toLowerCase()==='int:'){
      const n=s.slice(4).trim();
      return n? {kind:'int', name:n} : null;
    }
    const g=this._grid(s);
    if(g) return {kind:'ext', x:g[0], y:g[1]};
    return s? {kind:'int', name:s} : null;
  },
  /** The key two spellings of one cell share: `x,y`, or `int:` and the folded name. */
  fold(spec){ const k=this.parse(spec); return !k? null : k.kind==='ext'? k.x+','+k.y : 'int:'+this.foldName(k.name); },
  /** The page's and the engine's spelling: `int:Name` or `x,y`. */
  spec(spec){ const k=this.parse(spec); return !k? null : k.kind==='ext'? k.x+','+k.y : 'int:'+k.name; },
  /** The Editor pool's spelling (and its records'): the name, or `(x, y)`. */
  pool(spec){ const k=this.parse(spec); return !k? null : k.kind==='ext'? '('+k.x+', '+k.y+')' : k.name; },
  /** An exterior's square, `[x, y]`; null for an interior. */
  grid(spec){ const k=this.parse(spec); return k && k.kind==='ext'? [k.x, k.y] : null; },
  /** The cell the viewer opens (`App.cellSel`): `{kind:'int', name, label}` or
   *  `{kind:'ext', x, y, name:'', label}`; null for no name. */
  target(spec){
    const k=this.parse(spec);
    if(!k) return null;
    return k.kind==='int'? {kind:'int', name:k.name, label:k.name} : {kind:'ext', x:k.x, y:k.y, name:'', label:k.x+', '+k.y};
  },
  /** The key a door's aim (`App._doorAim.key`) is matched by when the cell is framed
   *  (18_cellpreview.js): `i:` and the name as written, or `x,y`. */
  doorKey(spec){ const k=this.parse(spec); return !k? '' : k.kind==='int'? 'i:'+k.name : k.x+','+k.y; },
  /** Whether two spellings name one cell. */
  same(a,b){ const x=this.fold(a); return x!=null && x===this.fold(b); },
  /** An interior's name without regard to case (`ordercore::cells::fold_name`). */
  foldName(n){ return String(n).toLowerCase(); },
  /** `x,y`, `x, y` or `(x, y)` to the square (Rust's i32 parse: an optional sign, digits). */
  _grid(s){
    const open=s.startsWith('('), close=s.endsWith(')');
    if(open!==close) return null;
    const inner=open? s.slice(1,-1) : s, at=inner.indexOf(',');
    if(at<0) return null;
    const n=t=>{ t=t.trim(); if(!/^[+-]?\d+$/.test(t)) return null; const v=Number(t); return v>=-2147483648 && v<=2147483647? v : null; };
    const x=n(inner.slice(0,at)), y=n(inner.slice(at+1));
    return x==null || y==null? null : [x,y];
  },
};
