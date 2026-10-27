
function show(n){
  for(var i=0;i<3;i++){
    document.getElementById('t'+i).className=i==n?'tab on':'tab';
    document.getElementById('b'+i).className=i==n?'on':'';
  }
}
// Open a cell in Wraithguard's Cell Preview: a cell's double-click, or the list rows'
// play buttons. Asks the app through its loopback server (the page's own token), so it
// works when Wraithguard opened this map, and says so when the file was opened by hand.
function openPreview(spec){
  var t=new URLSearchParams(location.search).get('t');
  if(!t){ alert('Open this map from Wraithguard to launch Cell Preview from it.'); return; }
  fetch('/wg_open_cell?t='+encodeURIComponent(t),{method:'POST',body:spec})
    .catch(function(){ alert('Wraithguard is not answering - is it still open?'); });
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
