/* =====================================================================================
   Wraithguard: the windows OpenMW Lua scripts make (`ui.create`), drawn over the Preview.

   The engine (viewcore::luarun) sends each element as a tree with its templates resolved
   (OpenMW's own `I.MWUI` ones included): Widget, Container, Flex, Image, Text, TextEdit,
   with OpenMW's layout props - position and relativePosition, size and relativeSize,
   anchor, visible, alpha; a Flex's horizontal, arrange and align; a Text's text, textSize,
   textColor, textShadow, multiline and wordWrap; an Image's resource (a texture from the
   data folders; `white` is a flat color), color and tiling. A click (mouseClick,
   mousePress, mouseRelease) or typed text (textChanged) goes back with the next frame to
   the node's own callback. Each layer is its own height, as OpenMW stacks them.

   A Container is as big as what it holds, as in OpenMW: its children share one grid cell,
   each moved by its position. Copyright (c) 2026 StaticNation, GPL-2.0 as part of this page.
   ===================================================================================== */
const WgLuaUI={
  els:new Map(),          // element id -> its top div
  events:[],              // what the person did, for the next frame
  imgs:new Map(),         // texture path -> Promise<url>
  LAYERS:['HUD', 'Windows', 'Notification', 'MessageBox', 'Popup', 'Console', 'Settings', 'Modal'],

  root(){
    let r=$('#wgLuaUI');
    if(!r){
      r=document.createElement('div'); r.id='wgLuaUI';
      r.style.cssText='position:absolute;inset:0;pointer-events:none;z-index:6;overflow:hidden;font-family:"Magic Cards","Pelagiad",Georgia,serif';
      ($('#vpwrap')||document.body).appendChild(r);
      this.font();
    }
    return r;
  },
  /** The viewport's size, for ui.screenSize. */
  screen(){ const r=($('#vpwrap')||document.body).getBoundingClientRect(); return [Math.round(r.width)||1920, Math.round(r.height)||1080]; },
  draw(id, layer, tree){
    this.remove(id);
    if(!tree) return;
    const el=this.node(tree, id, null);
    const z=this.LAYERS.indexOf(layer);
    el.style.zIndex=String(z<0? 2 : z+1);
    el.dataset.luaEl=id;
    this.root().appendChild(el);
    this.els.set(id, el);
  },
  remove(id){ const el=this.els.get(id); if(el) el.remove(); this.els.delete(id); },
  clear(){ for(const id of [...this.els.keys()]) this.remove(id); this.events=[]; },
  take(){ const e=this.events; this.events=[]; return e; },

  px(v){ return v==null? 0 : +v||0; },
  /** A calc() of a part of the parent and pixels. */
  len(rel, abs){ const r=this.px(rel), a=this.px(abs); return r? 'calc('+(r*100)+'% + '+a+'px)' : a+'px'; },
  color(c, alpha){ if(!c) return ''; const a=(c.a==null? 1 : c.a)*(alpha==null? 1 : alpha); return 'rgba('+Math.round(c.r*255)+','+Math.round(c.g*255)+','+Math.round(c.b*255)+','+a+')'; },

  /** One node and what it holds. `parentType` decides how it is placed in its parent. */
  node(n, id, parentType){
    const p=n.props||{}, t=n.type||'Widget';
    let el;
    if(t==='TextEdit'){
      el=document.createElement(p.multiline? 'textarea' : 'input');
      el.value=p.text==null? '' : String(p.text);
      el.readOnly=!!p.readOnly;
      el.style.cssText='background:transparent;border:none;outline:none;resize:none;color:inherit;font:inherit';
      if((n.events||[]).includes('textChanged')) el.addEventListener('input', ()=>this.events.push({id, path:n.path, event:'textChanged', text:el.value}));
    }else el=document.createElement('div');
    const s=el.style;
    const flexChild=parentType==='Flex', inContainer=parentType==='Container';
    // Where it sits in its parent.
    if(parentType==null || (!flexChild && !inContainer)){
      s.position='absolute';
      s.left=this.len(p.relativePosition && p.relativePosition.x, p.position && p.position.x);
      s.top=this.len(p.relativePosition && p.relativePosition.y, p.position && p.position.y);
    }else if(inContainer){
      s.position='relative'; s.gridArea='1 / 1';
      s.marginLeft=this.px(p.position && p.position.x)+'px'; s.marginTop=this.px(p.position && p.position.y)+'px';
    }else s.position='relative';
    // Its size: given (parts of the parent and pixels), else as big as what it holds.
    const rs=p.relativeSize||{}, sz=p.size||{};
    if(this.px(rs.x) || this.px(sz.x)) s.width=this.len(rs.x, sz.x);
    if(this.px(rs.y) || this.px(sz.y)) s.height=this.len(rs.y, sz.y);
    if(p.anchor && (this.px(p.anchor.x) || this.px(p.anchor.y))) s.transform='translate('+(-this.px(p.anchor.x)*100)+'%,'+(-this.px(p.anchor.y)*100)+'%)';
    if(p.visible===false) s.display='none';
    if(p.alpha!=null) s.opacity=String(p.alpha);
    s.boxSizing='border-box';
    switch(t){
      case 'Text':
        el.textContent=p.text==null? '' : String(p.text);
        s.whiteSpace=p.wordWrap? 'pre-wrap' : 'pre';
        s.fontSize=(this.px(p.textSize)||10)+'px'; s.lineHeight='1.2';
        s.color=this.color(p.textColor)||'rgb(202,165,96)';
        if(p.textShadow) s.textShadow='1px 1px 0 '+(this.color(p.textShadowColor)||'#000');
        if(p.textAlignH==='Center') s.textAlign='center'; else if(p.textAlignH==='End') s.textAlign='right';
        break;
      case 'TextEdit':
        s.fontSize=(this.px(p.textSize)||10)+'px'; s.color=this.color(p.textColor)||'rgb(202,165,96)';
        break;
      case 'Image': {
        const path=p.resource && p.resource.path;
        if(!path || path==='white') s.background=this.color(p.color)||'#fff';
        else{
          s.backgroundSize=(p.tileH || p.tileV)? 'auto' : '100% 100%';
          s.backgroundRepeat=p.tileH && p.tileV? 'repeat' : p.tileH? 'repeat-x' : p.tileV? 'repeat-y' : 'no-repeat';
          // A color over a texture tints it (multiplied), the texture's own shape kept.
          const tint=p.color && !(p.color.r===1 && p.color.g===1 && p.color.b===1)? this.color(p.color) : '';
          this.image(path).then(url=>{
            if(!url) return;
            s.backgroundImage='url("'+url+'")';
            if(tint){
              s.backgroundColor=tint; s.backgroundBlendMode='multiply';
              for(const k of ['maskImage', 'webkitMaskImage']){ s[k]='url("'+url+'")'; }
              for(const k of ['maskSize', 'webkitMaskSize']) s[k]=s.backgroundSize;
              for(const k of ['maskRepeat', 'webkitMaskRepeat']) s[k]=s.backgroundRepeat;
            }
          });
        }
        break;
      }
      case 'Flex':
        s.display=p.visible===false? 'none' : 'flex';
        s.flexDirection=p.horizontal? 'row' : 'column';
        s.alignItems={Start:'flex-start', Center:'center', End:'flex-end'}[p.arrange]||'flex-start';
        s.justifyContent={Start:'flex-start', Center:'center', End:'flex-end'}[p.align]||'flex-start';
        break;
      case 'Container':
        if(p.visible!==false) s.display='inline-grid';
        break;
    }
    // A Container sizes to its children; any other node holds them where they say.
    const kids=Array.isArray(n.content)? n.content : [];
    for(const c of kids) el.appendChild(this.node(c, id, t==='Flex'? 'Flex' : t==='Container'? 'Container' : t));
    const ev=n.events||[];
    if(ev.length){
      s.pointerEvents='auto'; s.cursor=ev.includes('mouseClick')? 'pointer' : '';
      const send=(event, e)=>{ const r=el.getBoundingClientRect(); this.events.push({id, path:n.path, event, x:Math.round(e.clientX-r.left), y:Math.round(e.clientY-r.top)}); };
      if(ev.includes('mouseClick')) el.addEventListener('click', e=>{ e.stopPropagation(); send('mouseClick', e); });
      if(ev.includes('mousePress')) el.addEventListener('pointerdown', e=>{ e.stopPropagation(); send('mousePress', e); });
      if(ev.includes('mouseRelease')) el.addEventListener('pointerup', e=>send('mouseRelease', e));
      // A move, at most ten a second; the pointer coming in and going out.
      if(ev.includes('mouseMove')){ let last=-Infinity; el.addEventListener('pointermove', e=>{ const now=performance.now(); if(now-last<100) return; last=now; send('mouseMove', e); }); }
      if(ev.includes('focusGain')) el.addEventListener('pointerenter', ()=>this.events.push({id, path:n.path, event:'focusGain'}));
      if(ev.includes('focusLoss')) el.addEventListener('pointerleave', ()=>this.events.push({id, path:n.path, event:'focusLoss'}));
      // Keys, to a node that takes them once it is clicked.
      if(ev.includes('keyPress') || ev.includes('keyRelease')){
        el.tabIndex=0; s.outline='none';
        const key=(event, e)=>{ e.stopPropagation(); this.events.push({id, path:n.path, event, x:WgLuaUI.scancode(e.code), y:e.shiftKey? 1 : 0, text:e.key.length===1? e.key : ''}); };
        if(ev.includes('keyPress')) el.addEventListener('keydown', e=>key('keyPress', e));
        if(ev.includes('keyRelease')) el.addEventListener('keyup', e=>key('keyRelease', e));
      }
    }else if(t==='TextEdit') s.pointerEvents='auto';
    return el;
  },
  /** SDL's scancode for a key (`KeyboardEvent.code`), as OpenMW's input.KEY numbers them. */
  scancode(code){
    let m;
    if((m=/^Key([A-Z])$/.exec(code))) return 4+m[1].charCodeAt(0)-65;
    if((m=/^Digit([0-9])$/.exec(code))) return m[1]==='0'? 39 : 29+(+m[1]);
    if((m=/^F([0-9]+)$/.exec(code)) && +m[1]<=12) return 57+(+m[1]);
    if((m=/^Numpad([0-9])$/.exec(code))) return m[1]==='0'? 98 : 88+(+m[1]);
    return {Enter:40, Escape:41, Backspace:42, Tab:43, Space:44, Minus:45, Equal:46, BracketLeft:47, BracketRight:48, Backslash:49, Semicolon:51,
      Quote:52, Backquote:53, Comma:54, Period:55, Slash:56, CapsLock:57, Insert:73, Home:74, PageUp:75, Delete:76, End:77, PageDown:78,
      ArrowRight:79, ArrowLeft:80, ArrowDown:81, ArrowUp:82, ControlLeft:224, ShiftLeft:225, AltLeft:226, MetaLeft:227,
      ControlRight:228, ShiftRight:229, AltRight:230, MetaRight:231}[code]||0;
  },
  /** OpenMW's own font (its resources' MysticCards), for the windows' text, once. */
  async font(){
    if(this._font!==undefined) return this._font;
    this._font=null;
    try{
      const buf=await Engine.bytes('lua_ui_font',{});
      if(buf && buf.byteLength && typeof FontFace==='function'){
        const f=new FontFace('OpenMW Mystic Cards', buf); await f.load(); document.fonts.add(f);
        this._font='OpenMW Mystic Cards';
        const r=$('#wgLuaUI'); if(r) r.style.fontFamily='"OpenMW Mystic Cards","Magic Cards","Pelagiad",Georgia,serif';
      }
    }catch(_){ }
    return this._font;
  },
  /** A texture from the data folders as an image the page can show (decoded by the engine). */
  image(path){
    const k=String(path).toLowerCase();
    if(this.imgs.has(k)) return this.imgs.get(k);
    const p=(async()=>{
      try{
        const buf=await Engine.bytes('texture_data',{path:k.replace(/\\/g,'/'), bc:false, maxSize:512, bcx:[]});
        const t=readTextureBytes(buf);
        if(t.kind==='file'){
          const raw=t.levels[0].data, mime=raw[0]===0xFF? 'image/jpeg' : raw[0]===0x42? 'image/bmp' : 'image/png';
          return URL.createObjectURL(new Blob([raw], {type:mime}));
        }
        const l=t.levels[0];
        return rgbaToDataURL({w:l.w, h:l.h, data:l.data}, 512);
      }catch(_){ return null; }
    })();
    this.imgs.set(k, p);
    return p;
  },
};
