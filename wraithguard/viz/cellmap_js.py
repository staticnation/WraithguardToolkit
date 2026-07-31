"""The cell map's stylesheet and client script, kept out of the page builder.

Separated for the same reason the page itself was moved out of the engine: a
216-line f-string containing CSS, HTML and JavaScript together meant every brace
had to be doubled, nothing could be tested in isolation, and a typo in the
script was indistinguishable from a typo in the markup. Here both are plain
module constants with no interpolation, so the JavaScript reads as JavaScript.

Kept inline in the page (rather than written as sibling files) because this map
is a **single self-contained document** the user can move, keep or attach to a
bug report. That is a property worth preserving for the one page most likely to
be shared.
"""

from __future__ import annotations

from typing import Final

#: The cell map's own styling. Deliberately unlike the shared viz shell: this
#: page predates it and is recognisable as itself.
CELLMAP_CSS: Final[str] = """
 body{background:#101013;color:#c8c8c8;font-family:Segoe UI,Arial,sans-serif;margin:16px;}
 h1{color:#e8905a;font-size:20px;margin-bottom:4px;}
 .sub{color:#8f8f8f;font-size:13px;}
 .stamp{color:#6f6f6f;font-size:12px;margin-top:2px;}
 .legend{margin-top:12px;line-height:1.7;}
 .legend span{display:inline-block;padding:2px 8px;margin-right:6px;border-radius:3px;
   font-size:12px;}
 .tabs{margin-top:24px;margin-bottom:4px;}
 .tabs button{background:#20242a;color:#ddd;border:1px solid #3a3a3a;padding:6px 14px;
   margin-right:4px;cursor:pointer;}
 .tabs button.on{background:#8a3a12;color:#fff;}
 .tab{display:none;margin-top:10px;} .tab.on{display:block;}
 .mapwrap{overflow:auto;max-height:74vh;border:1px solid #333;background:#06111c;
   display:block;max-width:100%;resize:vertical;}
 .mapwrap svg{display:block;}
 rect.cell{cursor:pointer;} rect.cell:hover{stroke:#fff;stroke-width:1.4;}
 rect.cell.dim{opacity:.13;}
 #tt{position:fixed;pointer-events:none;display:none;z-index:99;max-width:440px;
   background:#000;color:#eee;border:1px solid #555;border-radius:3px;padding:3px 7px;
   font-size:12px;}
 /* The lists scroll in their own pane, like the map, so a 9,000-row exterior
    list does not push the tab strip off the top of the window. */
 .listwrap{overflow:auto;max-height:74vh;border:1px solid #262626;resize:vertical;}
 table.list{border-collapse:collapse;width:100%;font-size:13px;}
 .list td,.list th{border-bottom:1px solid #262626;padding:4px 8px;text-align:left;
   vertical-align:top;}
 .list th{color:#9a9a9a;position:sticky;top:0;background:#101013;}
 tr.cust td{color:#ff9b6b;}
 tr.hl td{background:#3a2a10;}
 input.f{background:#1c1c22;color:#ddd;border:1px solid #3a3a3a;padding:6px;width:320px;
   margin:6px 0;}
 .focusbar{margin-top:10px;}
 .focusbar select{background:#1c1c22;color:#ddd;border:1px solid #3a3a3a;padding:5px;
   max-width:420px;}
 .focusbar button{background:#20242a;color:#ddd;border:1px solid #3a3a3a;padding:5px 10px;
   margin-left:6px;cursor:pointer;}
 #focusinfo{margin-top:4px;max-width:900px;}
"""

#: Tab switching, the instant tooltip, list filtering and the mod focus filter.
CELLMAP_JS: Final[str] = r"""
function show(n){
  for(var i=0;i<3;i++){
    document.getElementById('t'+i).className=i==n?'tab on':'tab';
    document.getElementById('b'+i).className=i==n?'on':'';
  }
}
function jump(a){
  show(1);
  var el=document.getElementById(a);
  if(el){
    el.scrollIntoView({block:'center'});
    el.classList.add('hl');
    setTimeout(function(){el.classList.remove('hl');},2200);
  }
}
// One delegated listener rather than per-rect handlers: with thousands of cells,
// attaching individually is what makes a tooltip feel laggy. Native SVG <title>
// carries a browser-controlled ~1s delay and cannot be styled, which is exactly
// wrong for a grid meant to be swept over with the cursor.
(function(){
  var tt=document.getElementById('tt');
  document.addEventListener('mouseover',function(e){
    var r=e.target;
    if(r&&r.classList&&r.classList.contains('cell')){
      tt.textContent=r.getAttribute('data-t');tt.style.display='block';
    }
  });
  document.addEventListener('mousemove',function(e){
    if(tt.style.display=='block'){
      var x=e.clientX+12,y=e.clientY+12;
      if(x+450>window.innerWidth){x=e.clientX-450;}
      if(y+80>window.innerHeight){y=e.clientY-60;}
      tt.style.left=x+'px';tt.style.top=y+'px';
    }
  });
  document.addEventListener('mouseout',function(e){
    var r=e.target;
    if(r&&r.classList&&r.classList.contains('cell')){tt.style.display='none';}
  });
})();
var Q={xt:'',it:''}, FOCUS='';
function match(r){
  return !FOCUS||(r.getAttribute('data-m')||'').indexOf('|'+FOCUS+'|')>-1;
}
function apply(id){
  document.querySelectorAll('#'+id+' tbody tr').forEach(function(r){
    var okQ=!Q[id]||r.innerText.toLowerCase().indexOf(Q[id])>-1;
    r.style.display=(okQ&&match(r))?'':'none';
  });
}
function ff(id){
  Q[id]=(event.target.value||'').toLowerCase();
  apply(id);
}
function setFocus(v){
  FOCUS=(v||'').toLowerCase();
  document.querySelectorAll('rect.cell').forEach(function(r){
    r.classList.toggle('dim',!!FOCUS&&!match(r));
  });
  apply('xt');apply('it');
  var info=document.getElementById('focusinfo');
  if(!FOCUS){info.textContent='';return;}
  var nE=0,nI=0,co={};
  document.querySelectorAll('#xt tbody tr').forEach(function(r){
    if(match(r)){nE++;countCo(r,co);}
  });
  document.querySelectorAll('#it tbody tr').forEach(function(r){
    if(match(r)){nI++;countCo(r,co);}
  });
  var names=Object.keys(co).sort(function(a,b){return co[b]-co[a];});
  var top=names.slice(0,14).map(function(n){return n+' ('+co[n]+')';}).join(', ');
  info.textContent='Touches '+nE+' exterior + '+nI+' interior cell(s). '+
    (names.length?'Shares cells with '+names.length+' other mod(s): '+top+
      (names.length>14?', …':''):'No other mod touches these cells.');
}
function countCo(r,co){
  (r.getAttribute('data-m')||'').split('|').forEach(function(m){
    if(m&&m!=FOCUS){co[m]=(co[m]||0)+1;}
  });
}
"""
