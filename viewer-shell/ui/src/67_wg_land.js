/* =====================================================================================
   Wraithguard: the Land view - landscape editing with Gardenfell's brush.

   Gardenfell's grass paint was taken out of this viewer (the agreement with Robin: it is
   not a grass generator), but its brush stayed in the renderer: the ring that shows where
   a sphere of the brush's radius meets the ground, its feather, the exponential size
   slider, and strokes batched once a frame (`opts.paintMode`, `onBrushMove`, `onPaint`,
   `setBrush` in 06_gl.js, `BrushSize` in 09_ui.js). "Brushes may come back later for
   landscape editing" (viewer-shell/GARDENFELL.md) - this is that, as a view mode of its own, with
   OpenMW-CS's three terrain modes (a reference for what a modder expects, not for code):

   - Height: raise, lower, smooth and flatten (flatten to the height under the brush where
     the stroke began). A vertex inside the inner radius takes the whole strength, and the
     feather fades it to nothing at the ring.
   - Texture: a land texture painted onto the 16 x 16 texture cells whose centres the brush
     covers. The coverage maps are rebuilt here as the engine builds them
     (`viewcore::coverage::layers_with`), the ring round each cell from its neighbours.
   - Color: the vertex colors (the tint the game multiplies the ground by) blended toward
     a color, by strength and feather.
   Each dab changes the cells in the page at once and the touched cells are rebuilt, so the
   ground changes under the brush; a vertex on a border belongs to both cells and changes
   in both, so no seam opens. A moment after the stroke ends the touched cells go to the
   Editor's patch (`editLandSet`, `editLandTextures`, `editLandColors`; editcore
   `set_land*`): heights with the normals and world map derived as Merge Lands writes them,
   textures through land texture records of the patch's own (a VTEX value means nothing
   outside its plugin). Undo is the Editor's.
   Copyright (c) 2026 StaticNation.
   ===================================================================================== */
const WgLand={
  on:false,
  mode:'height',        // height | texture | color
  tool:'raise',         // the height tool
  texture:'',           // the texture brush's land texture id
  color:'#806040',     // the color brush's color
  sizePos:BrushSize.pos(384),
  strength:40,          // height: world units a dab moves the centre; texture/color: %
  feather:60,           // % of the radius that fades
  preview:false,        // strokes wait for Keep or Drop instead of going to the patch
  /* The brush engine's settings (72_wg_landbrush.js): its shape and falloff, the stroke's
     dynamics, and each height tool's own. */
  b:{falloff:'smooth', shape:'circle', aspect:1, angle:0, spacing:25,
     click:100, drag:100, rate:8, delay:150, attack:0, decay:0, fadeLen:0, fadeRepeat:'none',
     velocity:0, random:0, jitter:0, stabilize:0, cap:0,
     smooth:'average', wide:3, flatTarget:'start', flatHeight:0, flatOnly:'both',
     noiseScale:1024, noiseOct:4, noiseRough:0.5, noiseSeed:1, terrace:128, edge:0.2, talus:35},
  _stroke:null, _mods:{shift:false, alt:false}, _pressed:false, _raf:0, _lastT:0,
  /* The map overlays over the ground while the Land view is on (06_gl.js's ground shader),
     as the old 3D terrain view drew them: a hillshade (lit from the azimuth, at the
     altitude, by 1, 3 or 6 lights), a height tint (hypsometric, rainbow or grey) and
     contours (an interval, or a round one picked for about a dozen lines). */
  ov:{hill:0, hillGrey:false, az:225, alt:45, lights:1, tint:'none', tintA:55, contours:false, interval:0},
  OV_DEFAULT:{hill:0.6, hillGrey:false, az:225, alt:45, lights:1, tint:'hypso', tintA:55, contours:true, interval:0},
  dirty:new Map(),      // "gx,gy" -> Set of 'h', 't', 'c', 'n' changed since the last commit
  saved:new Map(),      // "gx,gy" -> {rev, heights?, colors?, ids?}: what the patch has
  _rev:0, _patchRev:null, _poll:0,
  _commitT:0, _flatZ:null, _hooks:null, _el:null, _picking:false,
  _before:null,         // "gx,gy" -> the cell's land before the strokes waiting in a preview
  _edited:new Map(),    // "gx,gy" -> the scene's cell object the strokes not yet written changed

  /** The brush's settings, kept between runs (the page's storage, when it has one). */
  KEEP:['mode','tool','texture','color','sizePos','strength','feather','preview','b','ov'],
  restore(){
    try{
      const v=JSON.parse(localStorage.getItem('wgLand')||'{}');
      // Kept before 2026-11-02 under the British spelling.
      if(v.color==null && v.colour!=null) v.color=v.colour;
      if(v.mode==='colour') v.mode='color';
      for(const k of this.KEEP) if(v[k]!=null) this[k]= (k==='b' || k==='ov')? Object.assign({}, this[k], v[k]) : v[k];
    }catch(_){}
  },
  keep(){
    try{ const v={}; for(const k of this.KEEP) v[k]=this[k]; localStorage.setItem('wgLand', JSON.stringify(v)); }catch(_){}
  },

  CELL:8192, N:65, STEP:128, TS:16, TSTEP:512,

  radius(){ return BrushSize.radius(this.sizePos); },

  /** The topbar's "Land" switch, once an install is connected (not in the mesh viewer). */
  button(){
    if($('#btnLand') || (typeof WgMeshView==='object' && WgMeshView.on)) return;
    const after=$('#btnEditor')||$('#btnReport'); if(!after) return;
    const b=document.createElement('button');
    b.className='btn dim'; b.id='btnLand'; b.textContent='Land';
    b.title='Shape and paint the landscape with a brush: height, land textures and vertex colors - the changes go to the Editor\'s patch';
    b.onclick=()=> this.on? this.leave() : this.enter();
    after.insertAdjacentElement('afterend', b);
  },

  async enter(){
    if(this.on) return;
    const R=App.R;
    if(!R || !App._scene){ toast('Open an exterior cell first','warn',3000); return; }
    if(typeof WgEditor!=='object'){ toast('Land editing needs the Editor','err',4000); return; }
    try{ await WgEditor.nativeLinks(); }catch(_){}
    if(!WgEditor.canEdit()){ toast('Land editing needs the Editor open on an OpenMW setup (see Messages)','warn',5000); return; }
    if(WgEditor.on) WgEditor.leave();
    this.restore();
    this.on=true;
    /* A view mode of its own (Static, 2026-11-02: "it just overlayed a small panel not opened
       its own mode"): the Land dock where the Cell Preview's settings are, as the Editor
       docks its windows there, the brand and the hint saying so. */
    document.body.classList.add('wgLandMode');
    const bi=document.querySelector('#brand i'); if(bi){ this._brand=bi.textContent; bi.textContent='Land'; }
    document.title='Wraithguard - Land';
    const b=$('#btnLand'); if(b) b.classList.add('on');
    this._hooks={paint:R.opts.paintMode, move:R.onBrushMove, onPaint:R.onPaint};
    R.opts.paintMode=true;
    R.onBrushMove=e=>this.brushMove(e);
    R.onPaint=(pts)=>this.paint(pts);
    /* Over a square with no land the brush still needs ground to stand on: the flat ground
       the CS and OpenMW show there (DEFAULT_LAND_HEIGHT), so a stroke makes land (New
       land) where it is drawn and shapes it. */
    this._groundZ=R.groundZ;
    const base=R.groundZ;
    R.groundZ=(x,y)=>{
      const z=base? base(x,y) : null;
      if(z!=null) return z;
      const sc=App._scene; if(!sc || !sc.origin) return null;
      const gx=Math.floor((x+sc.origin[0])/this.CELL), gy=Math.floor((y+sc.origin[1])/this.CELL);
      return (sc.cells||[]).some(c=>c.kind==='ext' && c.gx===gx && c.gy===gy)? DEFAULT_LAND_HEIGHT : null;
    };
    // The patch can change under the view (an undo in the Editor): follow it.
    this._poll=setInterval(()=>this.pollPatch(), 1500);
    this.sync();
    // A stroke is the button's press to its release: the engine times it (click, drag, hold).
    this._down=e=>{ if(e.button!==0) return; this._flatZ=null; this._pressed=true; this.beginStroke(); this.holdLoop(); };
    this._up=e=>{ if(e && e.button!=null && e.button!==0) return; this._pressed=false; this.endStroke(); this.scheduleCommit(); };
    R.gl.canvas.addEventListener('pointerdown', this._down);
    R.gl.canvas.addEventListener('pointerup', this._up);
    this.panel();
    // Ctrl+Z / Ctrl+Y as in the Editor (whose keys are off while this mode is on).
    this._mod=e=>{ this._mods.shift=!!e.shiftKey; this._mods.alt=!!e.altKey; };
    window.addEventListener('keydown', this._mod); window.addEventListener('keyup', this._mod);
    this._keys=e=>{
      const t=e.target; if(t && (t.tagName==='INPUT' || t.tagName==='TEXTAREA' || t.isContentEditable)) return;
      // [ and ] the size, { and } the strength, as paint programs have them.
      if(!e.ctrlKey && !e.metaKey && !e.altKey && (e.key==='[' || e.key===']' || e.key==='{' || e.key==='}')){
        e.preventDefault();
        if(e.key==='[') this.sizePos=Math.max(0, this.sizePos-4);
        else if(e.key===']') this.sizePos=Math.min(100, this.sizePos+4);
        else this.strength=Math.max(1, Math.min(200, Math.round(this.strength*(e.key==='}'? 1.25 : 0.8))));
        if(this._show) this._show();
        const R=App.R; if(R && R._ptr) this.brushMove({clientX:R._ptr[0], clientY:R._ptr[1]});
        return;
      }
      if(this.waiting() && !e.ctrlKey && !e.metaKey && !e.altKey && (e.key==='Enter' || e.key==='Escape')){
        e.preventDefault(); e.stopPropagation();
        if(e.key==='Enter') this.commit(); else this.drop();
        return;
      }
      if(!(e.ctrlKey||e.metaKey) || e.altKey) return;
      const k=e.key.toLowerCase();
      if(k==='z' && !e.shiftKey){ e.preventDefault(); this.step('undo'); }
      else if(k==='y' || (k==='z' && e.shiftKey)){ e.preventDefault(); this.step('redo'); }
    };
    window.addEventListener('keydown', this._keys);
    if(App.syncVpHint) App.syncVpHint();
    R.dirty=true;
    // The renderer's size follows the dock taking the left column's place.
    window.dispatchEvent(new Event('resize'));
  },

  /** Out of the mode: the last stroke into the patch first (a preview waiting is kept,
   *  not lost), then the pool read again so the Editor's Pending and Build patch have it. */
  async leave(){
    if(!this.on) return;
    await this.commit();
    const R=App.R;
    if(R){
      R.opts.paintMode=!!(this._hooks&&this._hooks.paint);
      R.onBrushMove=this._hooks? this._hooks.move : null;
      R.onPaint=this._hooks? this._hooks.onPaint : null;
      if(this._groundZ!==undefined){ R.groundZ=this._groundZ; this._groundZ=undefined; }
      R.setBrush(null);
      R.gl.canvas.removeEventListener('pointerdown', this._down);
      R.gl.canvas.removeEventListener('pointerup', this._up);
      R.dirty=true;
    }
    this.on=false;
    clearInterval(this._poll); this._poll=0;
    if(this._keys){ window.removeEventListener('keydown', this._keys); this._keys=null; }
    if(this._mod){ window.removeEventListener('keydown', this._mod); window.removeEventListener('keyup', this._mod); this._mod=null; }
    cancelAnimationFrame(this._raf); this._raf=0; this._stroke=null; this._pressed=false;
    this.applyOverlays(true);
    const b=$('#btnLand'); if(b) b.classList.remove('on');
    if(this._el){ this._el.remove(); this._el=null; }
    document.body.classList.remove('wgLandMode');
    const bi=document.querySelector('#brand i'); if(bi) bi.textContent=this._brand||'Cell Preview';
    document.title='Wraithguard - '+(this._brand||'Cell Preview');
    if(App.syncVpHint) App.syncVpHint();
    window.dispatchEvent(new Event('resize'));
    if(typeof WgEditor==='object' && WgEditor.refreshPending) try{ await WgEditor.refreshPending(); }catch(_){}
  },

  /** The land textures the install has, by their own spelling. */
  textureIds(){
    const out=new Set();
    try{ for(const r of GameData.ltex.values()) if(r && r.id) out.add(r.id); }catch(_){}
    for(const c of this.cells().values()) for(const L of (c.layers||[])) if(L.id && L.id!==DEFAULT_LTEX) out.add(L.id);
    return [...out].sort((a,b)=>a.localeCompare(b));
  },

  /** Ready-made brushes: each sets the stroke's dynamics (and the falloff), not the tool,
   *  size or strength. */
  PRESETS:{
    standard:{label:'Standard', b:{falloff:'smooth', spacing:25, click:100, drag:100, rate:8, delay:150, attack:0, decay:0, fadeLen:0, velocity:0, random:0, jitter:0, stabilize:0, cap:0}},
    gentle:{label:'Gentle sculpt (layers evenly)', b:{falloff:'smooth', spacing:15, click:40, drag:35, rate:3, delay:200, attack:0.3, decay:0, fadeLen:0, velocity:0, random:0, jitter:0, stabilize:30, cap:64}},
    airbrush:{label:'Airbrush (builds while held)', b:{falloff:'gauss', spacing:10, click:20, drag:25, rate:20, delay:0, attack:0, decay:0, fadeLen:0, velocity:-40, random:0, jitter:0, stabilize:0, cap:0}},
    stamp:{label:'Stamp (one dab a click)', b:{falloff:'sphere', spacing:100, click:100, drag:0, rate:0, delay:150, attack:0, decay:0, fadeLen:0, velocity:0, random:0, jitter:0, stabilize:0, cap:0}},
    line:{label:'Clean line (stabilized)', b:{falloff:'sharp', spacing:10, click:60, drag:60, rate:0, delay:150, attack:0, decay:0, fadeLen:0, velocity:0, random:0, jitter:0, stabilize:70, cap:0}},
    taper:{label:'Tapering stroke (fades out)', b:{falloff:'smooth', spacing:15, click:100, drag:100, rate:0, delay:150, attack:0, decay:0, fadeLen:4096, fadeRepeat:'none', velocity:0, random:0, jitter:0, stabilize:20, cap:0}},
    rough:{label:'Rough (scattered, uneven)', b:{falloff:'linear', spacing:30, click:100, drag:100, rate:4, delay:150, attack:0, decay:0, fadeLen:0, velocity:0, random:60, jitter:40, stabilize:0, cap:0}},
  },
  TOOLS:[['raise','Raise','Lifts the ground under the brush by the strength (units) each dab.'],
    ['lower','Lower','Lowers the ground under the brush by the strength (units) each dab.'],
    ['smooth','Smooth','Pulls each vertex toward its neighbours\' height (Shift smooths with any tool).'],
    ['sharpen','Sharpen','Pushes each vertex away from its neighbours\' height: the detail the smoothing takes out.'],
    ['flatten','Flatten','Levels toward a height: where the stroke began, a fixed height, the sea, or the brush\'s average.'],
    ['ramp','Ramp','A straight slope from where the stroke began to the brush, as wide as the brush.'],
    ['noise','Noise','Adds fractal noise, up to the strength (units) either way.'],
    ['terrace','Terrace','Steps the ground into flat treads of the step height.'],
    ['erode','Erode','Thermal erosion: slopes steeper than the talus angle slide down.']],

  panel(){
    if(this._el) this._el.remove();
    const d=document.createElement('div');
    d.id='wgLandDock';
    const b=this.b;
    const modes=[['height','Height'],['texture','Texture'],['color','Color']];
    const sec=(id, title, body)=>'<div class="lsec" id="'+id+'"><div class="lsecT">'+title+'</div>'+body+'</div>';
    const fold=(id, title, body, open)=>'<details class="lsec" id="'+id+'"'+(open?' open':'')+'><summary class="lsecT" style="cursor:pointer">'+title+'</summary>'+body+'</details>';
    const opt=(v, t, cur)=>'<option value="'+v+'"'+(String(cur)===String(v)?' selected':'')+'>'+escHtml(t)+'</option>';
    // A slider on the brush engine's settings (`b`): data-k names it, its value beside it.
    const rng=(k, label, min, max, step, tip)=>'<div class="row" data-row="'+k+'"><label title="'+escHtml(tip||'')+'">'+label+'</label><div class="ctl"><div class="rng">'+
      '<input type="range" data-k="'+k+'" min="'+min+'" max="'+max+'" step="'+step+'" value="'+b[k]+'"><span class="val" data-v="'+k+'"></span></div></div></div>';
    const sel=(k, label, list, tip)=>'<div class="row" data-row="'+k+'"><label title="'+escHtml(tip||'')+'">'+label+'</label><div class="ctl"><select class="fld" data-s="'+k+'">'+list.map(([v,t])=>opt(v,t,b[k])).join('')+'</select></div></div>';
    d.innerHTML=
      '<div class="edHead"><b>Land</b><span style="flex:1"></span><button class="btn sm" id="wgLandDone" title="Back to the Cell Preview">Done</button></div>'+
      /* The page's own controls, so the theme dresses them as it dresses the rest (Static,
         2026-11-02: "our Wraithguard Land page needs to use the themes we set"): the chosen
         button lit as `.btn.pri`, the sliders in `.row`/`.rng` with their `.val`. */
      sec('wgLandModeS','Mode','<div class="lrow" id="wgLandModes">'+modes.map(([k,t])=>'<button class="btn sm'+(this.mode===k?' pri':'')+'" data-m="'+k+'">'+t+'</button>').join('')+'</div>')+
      sec('wgLandHeight','Tool','<div class="lrow">'+this.TOOLS.map(([k,t,tip])=>'<button class="btn sm'+(this.tool===k?' pri':'')+'" data-t="'+k+'" title="'+escHtml(tip)+'">'+t+'</button>').join('')+'</div>'+
        '<div class="hint" id="wgLandToolHint"></div>'+
        sel('smooth','Smoothing',[['average','Average (its 8 neighbours)'],['gauss','Gaussian (5 x 5)'],['wide','Wide blur']],'How a vertex\'s neighbourhood is averaged')+
        rng('wide','Blur reach',1,8,1,'How many vertices out the wide blur reaches')+
        sel('flatTarget','Level to',[['start','Where the stroke began'],['average','The brush\'s average'],['fixed','A fixed height'],['sea','The sea (0)']],'The height Flatten levels toward, fixed for the stroke')+
        '<div class="row" data-row="flatHeight"><label>Height</label><div class="ctl"><input class="fld" type="number" step="8" data-n="flatHeight" value="'+b.flatHeight+'" style="width:90px"> <button class="btn sm" id="wgLandFlatPick" title="Take the height under the next click">Pick</button></div></div>'+
        sel('flatOnly','Direction',[['both','Up and down'],['up','Only raise toward it'],['down','Only lower toward it']],'Fill hollows only, cut tops only, or both')+
        rng('noiseScale','Scale',64,8192,64,'The size of the noise\'s largest features, in units')+
        rng('noiseOct','Octaves',1,8,1,'Layers of finer detail')+
        rng('noiseRough','Roughness',0,1,0.05,'How strong each finer layer is')+
        '<div class="row" data-row="noiseSeed"><label>Seed</label><div class="ctl"><input class="fld" type="number" step="1" data-n="noiseSeed" value="'+b.noiseSeed+'" style="width:90px"> <button class="btn sm" id="wgLandSeed" title="Another seed">Shuffle</button></div></div>'+
        rng('terrace','Step height',16,1024,8,'The height of each terrace, in units')+
        rng('edge','Riser',0,1,0.05,'How much of each step is the slope between treads: 0 a sheer edge')+
        rng('talus','Talus angle',5,80,1,'Slopes steeper than this slide down'))+
      sec('wgLandTexture','Land texture',
        '<div class="lrow"><input class="fld" id="wgLandTexF" placeholder="Filter textures" spellcheck="false" style="flex:1;min-width:0">'+
        '<button class="btn sm" id="wgLandPick" title="Take the texture under the next click">Pick</button></div>'+
        '<div class="hint">Painting: <b id="wgLandTexNow"></b></div>'+
        '<div class="ltexList" id="wgLandTexList"></div>')+
      sec('wgLandColor','Color','<div class="lrow"><input type="color" id="wgLandCol" value="'+this.color+'"><span class="hint">the tint the ground\'s textures are multiplied by; white is none</span></div>')+
      sec('wgLandBrush','Brush',
        '<div class="row"><label>Preset</label><div class="ctl"><select class="fld" id="wgLandPre">'+opt('','Choose a brush…','')+
          Object.entries(this.PRESETS).map(([k,p])=>opt(k,p.label,'')).join('')+'</select></div></div>'+
        '<div class="row"><label for="wgLandSize" title="[ and ] while painting">Size</label><div class="ctl"><div class="rng"><input type="range" id="wgLandSize" min="0" max="100" value="'+this.sizePos+'"><span class="val" id="wgLandSizeV"></span></div></div></div>'+
        '<div class="row" id="wgLandStrL"><label for="wgLandStr" title="{ and } while painting">Strength</label><div class="ctl"><div class="rng"><input type="range" id="wgLandStr" min="1" max="200" value="'+this.strength+'"><span class="val" id="wgLandStrV"></span></div></div></div>'+
        '<div class="row" id="wgLandFeaL"><label for="wgLandFea">Feather</label><div class="ctl"><div class="rng"><input type="range" id="wgLandFea" min="0" max="100" value="'+this.feather+'"><span class="val" id="wgLandFeaV"></span></div></div></div>'+
        sel('falloff','Falloff',WgLandBrush.FALLOFFS,'How the strength fades across the feathered rim')+
        '<canvas id="wgLandCurve" width="240" height="44" style="width:100%;height:44px;display:block;margin:0 0 6px"></canvas>'+
        sel('shape','Shape',WgLandBrush.SHAPES,'The brush\'s outline (the ring shows its reach)')+
        rng('aspect','Aspect',0.1,1,0.05,'1 round; less a narrower blade, along the angle')+
        rng('angle','Angle',0,180,1,'Which way a narrow or square brush is turned')+
        rng('spacing','Spacing',1,200,1,'How far apart a drag\'s dabs fall, as a share of the radius'))+
      fold('wgLandOv','Overlays',
        '<div class="row"><label title="Shading from the ground\'s own slope, as a relief map has it">Hillshade</label><div class="ctl"><div class="rng"><input type="range" id="wgOvHill" min="0" max="1" step="0.05" value="'+this.ov.hill+'"><span class="val" id="wgOvHillV"></span></div></div></div>'+
        '<div class="row"><label>Shade</label><div class="ctl"><select class="fld" id="wgOvGrey">'+opt('0','Over the textures',this.ov.hillGrey?'1':'0')+opt('1','Grey relief (the old terrain view)',this.ov.hillGrey?'1':'0')+'</select></div></div>'+
        '<div class="row"><label title="The compass bearing the light comes from: 225 south-west, the old view\'s">Azimuth</label><div class="ctl"><div class="rng"><input type="range" id="wgOvAz" min="0" max="359" step="1" value="'+this.ov.az+'"><span class="val" id="wgOvAzV"></span></div></div></div>'+
        '<div class="row"><label title="How high the light stands over the horizon">Altitude</label><div class="ctl"><div class="rng"><input type="range" id="wgOvAlt" min="5" max="85" step="1" value="'+this.ov.alt+'"><span class="val" id="wgOvAltV"></span></div></div></div>'+
        '<div class="row"><label title="Several lights round the compass fill the shadow side without flattening the relief; the azimuth\'s stays strongest">Lights</label><div class="ctl"><select class="fld" id="wgOvLights">'+opt('1','One',this.ov.lights)+opt('3','Three',this.ov.lights)+opt('6','Six',this.ov.lights)+'</select></div></div>'+
        '<div class="row"><label title="The ground colored by its height">Tint</label><div class="ctl"><select class="fld" id="wgOvTint">'+opt('none','None',this.ov.tint)+opt('hypso','Green, tan and white (hypsometric)',this.ov.tint)+opt('rainbow','Rainbow',this.ov.tint)+opt('grey','Grey',this.ov.tint)+'</select></div></div>'+
        '<div class="row"><label>Tint opacity</label><div class="ctl"><div class="rng"><input type="range" id="wgOvTintA" min="0" max="100" step="1" value="'+this.ov.tintA+'"><span class="val" id="wgOvTintAV"></span></div></div></div>'+
        '<div class="row"><label title="A line every interval of height; every fifth heavier">Contours</label><div class="ctl"><input type="checkbox" id="wgOvCont"'+(this.ov.contours?' checked':'')+'> <input class="fld" type="number" id="wgOvInt" min="0" step="8" value="'+this.ov.interval+'" style="width:80px" title="Units between lines; 0 picks a round one for about a dozen lines"> <span class="hint" id="wgOvIntV"></span></div></div>'+
        '<div class="lrow"><button class="btn sm" id="wgOvRelief" title="Hillshade, the green-tan-white tint and contours, as the old terrain view opened">Relief map</button><button class="btn sm" id="wgOvOff">Off</button></div>', false)+
      fold('wgLandStroke','Stroke and dynamics',
        rng('click','Click',0,200,1,'A single click\'s dab, as a share of the strength')+
        rng('drag','Drag',0,200,1,'Each dab of a drag, as a share of the strength')+
        rng('rate','Hold rate',0,60,0.5,'Held still: dabs a second where the brush stands (0: nothing)')+
        rng('delay','Hold delay',0,1000,10,'How long the brush stands still before the hold starts (ms)')+
        rng('attack','Attack',0,3,0.05,'Seconds the stroke takes to reach full strength')+
        rng('decay','Decay',0,10,0.1,'The stroke\'s half-life in seconds: weaker the longer it is held (0: none)')+
        rng('fadeLen','Fade length',0,32768,256,'The stroke fades out over this many units of travel (0: no fade)')+
        sel('fadeRepeat','Fade repeat',[['none','None'],['loop','Loop'],['saw','Sawtooth'],['tri','Triangle']],'What happens after the fade length, as GIMP\'s paint dynamics have it')+
        rng('velocity','Velocity',-100,100,5,'A fast stroke stronger (+) or weaker (-)')+
        rng('random','Random',0,100,5,'How much each dab\'s strength varies')+
        rng('jitter','Jitter',0,100,5,'How far dabs scatter from the path, as a share of the radius')+
        rng('stabilize','Smooth stroke',0,95,5,'The brush trails the pointer: a steadier line the higher it is')+
        rng('cap','Stroke cap',0,1024,8,'The most any vertex moves in one stroke, in units (0: incremental, no limit)'), false)+
      '<div class="lsec"><div class="hint">Shift smooths, Alt inverts (raise and lower, smooth and sharpen), [ ] the size, { } the strength. A click is one dab, a drag lays them along the path, and holding still builds up.</div></div>'+
      sec('wgLandPatchS','In the patch',
        '<div class="lrow"><button class="btn sm'+(this.preview?' pri':'')+'" id="wgLandPrev" title="Strokes wait to be kept or dropped instead of going to the patch on their own">Preview first</button></div>'+
        '<div class="lrow" id="wgLandWait" style="display:none"><span class="hint" id="wgLandWaitN" style="flex:1;margin:0"></span>'+
        '<button class="btn sm pri" id="wgLandKeep" title="Put the changes into the patch (Enter)">Keep</button>'+
        '<button class="btn sm" id="wgLandDrop" title="Put the ground back as it was before them (Esc)">Drop</button></div>'+
        '<div class="hint" id="wgLandSt"></div>'+
        '<div class="lrow"><button class="btn sm" id="wgLandUndo" title="Take back the last change to the patch (Ctrl+Z)">Undo</button>'+
        '<button class="btn sm" id="wgLandRedo" title="Make it again (Ctrl+Y)">Redo</button></div>'+
        '<div class="lcells" id="wgLandCells"></div>'+
        '<div class="hint" id="wgLandHow"></div>');
    const main=$('#main')||document.body;
    main.insertBefore(d, main.firstChild);
    this._el=d;
    const drawTex=()=>{
      const f=($('#wgLandTexF').value||'').toLowerCase();
      const ids=this.textureIds().filter(id=>!f || id.toLowerCase().includes(f)).slice(0,400);
      $('#wgLandTexList').innerHTML=ids.map(id=>'<div class="ltex'+(id===this.texture?' on':'')+'" data-id="'+escHtml(id)+'" title="'+escHtml(GameData.textureFileFor? (GameData.textureFileFor(id)||'') : '')+'">'+escHtml(id)+'</div>').join('')||
        '<div class="hint">No land texture matches.</div>';
      $('#wgLandTexList').querySelectorAll('[data-id]').forEach(r=>r.onclick=()=>{ this.texture=r.dataset.id; this._picking=false; drawTex(); show(); });
    };
    const show=()=>{
      $('#wgLandHeight').style.display=this.mode==='height'? '' : 'none';
      $('#wgLandTexture').style.display=this.mode==='texture'? '' : 'none';
      $('#wgLandColor').style.display=this.mode==='color'? '' : 'none';
      $('#wgLandFeaL').style.display=this.mode==='texture'? 'none' : '';
      $('#wgLandStrL').style.display=this.mode==='texture'? 'none' : '';
      $('#wgLandSizeV').textContent=this.radius()+' u';
      const pct=this.mode!=='height' || !['raise','lower','noise'].includes(this.tool);
      $('#wgLandStrV').textContent=(pct? Math.min(100,this.strength) : this.strength)+(pct? ' %' : ' u');
      // Each tool's own settings, only for it.
      const own={smooth:['smooth','wide'], flatten:['flatTarget','flatHeight','flatOnly'], noise:['noiseScale','noiseOct','noiseRough','noiseSeed'],
        terrace:['terrace','edge'], erode:['talus']}[this.tool]||[];
      d.querySelectorAll('#wgLandHeight [data-row]').forEach(r=>{
        const k=r.dataset.row;
        r.style.display=own.includes(k) && (k!=='wide' || this.b.smooth==='wide') && (k!=='flatHeight' || this.b.flatTarget==='fixed')? '' : 'none';
      });
      d.querySelectorAll('[data-v]').forEach(v=>{ const k=v.dataset.v, x=this.b[k];
        v.textContent=k==='angle'? x+'°' : k==='talus'? x+'°' : ['click','drag','spacing','random','jitter','stabilize'].includes(k)? x+' %'
          : k==='rate'? x+'/s' : k==='delay'? x+' ms' : (k==='attack' || k==='decay')? (x? x+' s' : 'off') : k==='fadeLen'? (x? x+' u' : 'off')
          : k==='cap'? (x? x+' u' : 'incremental') : k==='velocity'? (x>0? '+' : '')+x+' %' : (k==='terrace' || k==='noiseScale')? x+' u' : String(x); });
      this.drawCurve();
      // The ring follows a change of shape, aspect or angle at once.
      { const R=App.R; if(R && R._ptr && this.on) this.brushMove({clientX:R._ptr[0], clientY:R._ptr[1]}); }
      $('#wgLandFeaV').textContent=this.feather+' %';
      $('#wgLandPick').classList.toggle('pri', this._picking);
      $('#wgLandTexNow').textContent=this.texture||'(none: pick one)';
      $('#wgLandToolHint').textContent=(this.TOOLS.find(t=>t[0]===this.tool)||[,,''])[2];
      this.keep();
    };
    d.querySelectorAll('#wgLandModes [data-m]').forEach(b=>b.onclick=()=>{
      this.mode=b.dataset.m; this._picking=false;
      d.querySelectorAll('#wgLandModes [data-m]').forEach(x=>x.classList.toggle('pri', x===b));
      show();
    });
    d.querySelectorAll('#wgLandHeight [data-t]').forEach(b=>b.onclick=()=>{
      this.tool=b.dataset.t;
      d.querySelectorAll('#wgLandHeight [data-t]').forEach(x=>x.classList.toggle('pri', x===b));
      show();
    });
    $('#wgLandTexF').oninput=drawTex;
    $('#wgLandPick').onclick=()=>{ this._picking=!this._picking; show(); };
    $('#wgLandCol').oninput=e=>{ this.color=e.target.value; };
    $('#wgLandSize').oninput=e=>{ this.sizePos=+e.target.value; show(); };
    $('#wgLandStr').oninput=e=>{ this.strength=+e.target.value; show(); };
    $('#wgLandFea').oninput=e=>{ this.feather=+e.target.value; show(); };
    // The brush engine's settings: sliders (data-k), lists (data-s), numbers (data-n).
    d.querySelectorAll('input[data-k]').forEach(r=>r.oninput=()=>{ this.b[r.dataset.k]=+r.value; show(); });
    d.querySelectorAll('select[data-s]').forEach(r=>r.onchange=()=>{ this.b[r.dataset.s]=r.value; show(); });
    d.querySelectorAll('input[data-n]').forEach(r=>r.onchange=()=>{ const v=parseFloat(r.value); if(isFinite(v)) this.b[r.dataset.n]=v; show(); });
    $('#wgLandPre').onchange=e=>{
      const pr=this.PRESETS[e.target.value]; if(!pr) return;
      Object.assign(this.b, pr.b); e.target.value='';
      this.panel(); toast('Brush: '+pr.label,'ok',2000);
    };
    $('#wgLandSeed').onclick=()=>{ this.b.noiseSeed=Math.floor(Math.random()*100000); const n=d.querySelector('[data-n="noiseSeed"]'); if(n) n.value=this.b.noiseSeed; show(); };
    $('#wgLandFlatPick').onclick=()=>{ this._pickHeight=true; toast('Click the ground to take its height','ok',2500); };
    // The overlays.
    const ov=()=>{ this.applyOverlays(); this.keep();
      $('#wgOvHillV').textContent=Math.round(this.ov.hill*100)+' %'; $('#wgOvAzV').textContent=this.ov.az+'°';
      $('#wgOvAltV').textContent=this.ov.alt+'°'; $('#wgOvTintAV').textContent=this.ov.tintA+' %'; };
    $('#wgOvHill').oninput=e=>{ this.ov.hill=+e.target.value; ov(); };
    $('#wgOvGrey').onchange=e=>{ this.ov.hillGrey=e.target.value==='1'; ov(); };
    $('#wgOvAz').oninput=e=>{ this.ov.az=+e.target.value; ov(); };
    $('#wgOvAlt').oninput=e=>{ this.ov.alt=+e.target.value; ov(); };
    $('#wgOvLights').onchange=e=>{ this.ov.lights=+e.target.value; ov(); };
    $('#wgOvTint').onchange=e=>{ this.ov.tint=e.target.value; ov(); };
    $('#wgOvTintA').oninput=e=>{ this.ov.tintA=+e.target.value; ov(); };
    $('#wgOvCont').onchange=e=>{ this.ov.contours=e.target.checked; ov(); };
    $('#wgOvInt').onchange=e=>{ this.ov.interval=Math.max(0, +e.target.value||0); ov(); };
    $('#wgOvRelief').onclick=()=>{ Object.assign(this.ov, this.OV_DEFAULT, {hillGrey:true}); this.panel(); this.applyOverlays(); this.keep(); };
    $('#wgOvOff').onclick=()=>{ Object.assign(this.ov, {hill:0, tint:'none', contours:false}); this.panel(); this.applyOverlays(); this.keep(); };
    ov();
    $('#wgLandDone').onclick=()=>this.leave();
    $('#wgLandUndo').onclick=()=>this.step('undo');
    $('#wgLandRedo').onclick=()=>this.step('redo');
    $('#wgLandPrev').onclick=async()=>{
      // Turned off with strokes waiting: they go in, as they would have without it.
      this.preview=!this.preview; this.keep();
      $('#wgLandPrev').classList.toggle('pri', this.preview);
      if(!this.preview) await this.commit();
      this.showWait();
    };
    $('#wgLandKeep').onclick=()=>this.commit();
    $('#wgLandDrop').onclick=()=>this.drop();
    this._show=show; this._drawTex=drawTex;
    drawTex(); show();
    this.showWait();
    this.drawPatch(null);
  },

  /** The patch's land cells, each with a way to take it out of the patch. `list` is
   *  `editLandPending`'s answer; null asks for it. */
  async drawPatch(list){
    const box=this._el && $('#wgLandCells'); if(!box) return;
    if(!list){ try{ list=await WgEditor.ask('editLandPending',{}); }catch(_){ list=[]; } }
    const cells=(Array.isArray(list)? list : []).map(e=>String(e.cell||'')).filter(Boolean);
    box.innerHTML=cells.length? cells.map(c=>'<div class="lcell"><span>'+escHtml(c)+'</span><span style="flex:1"></span>'+
      '<button class="btn dim sm" data-go="'+escHtml(c)+'" title="Show this cell">Go</button>'+
      '<button class="btn dim sm" data-out="'+escHtml(c)+'" title="Take this cell\'s land changes out of the patch">✕</button></div>').join('')
      : '<div class="hint">No land changes in the patch yet.</div>';
    box.querySelectorAll('[data-out]').forEach(b=>b.onclick=async()=>{
      try{ await WgEditor.ask('editRevert',{tag:'LAND', id:b.dataset.out}); }
      catch(e){ return toast(String(e.message||e),'err',5000); }
      await this.sync();
      if(WgEditor.refreshPending) try{ await WgEditor.refreshPending(); }catch(_){}
    });
    box.querySelectorAll('[data-go]').forEach(b=>b.onclick=()=>{
      const m=/\((-?\d+),\s*(-?\d+)\)/.exec(b.dataset.go);
      if(!m) return;
      // As the cell arrows go to a neighbour: the target set, the scene built round it.
      const x=+m[1], y=+m[2], named=GameData.grid? GameData.grid.get(x+','+y) : null;
      App.cellSel={kind:'ext', x, y, name:named? named.name : '', label:(named&&named.name? named.name+'  ' : '')+'('+x+', '+y+')'};
      if(typeof syncCellButton==='function') syncCellButton();
      schedulePreview();
    });
  },

  /** The Editor's undo or redo of the patch, then the ground as the patch now has it. */
  async step(which){
    // Undo with strokes waiting in a preview takes those back: they never reached the patch.
    if(which==='undo' && this.waiting()){ this.drop(); return; }
    await this.commit();
    try{ if(which==='undo') await WgEditor.undo(); else await WgEditor.redo(); }catch(e){ toast(String(e.message||e),'err',4000); }
    await this.sync();
  },

  status(t){ const s=$('#wgLandSt'); if(s) s.textContent=t||''; },

  /** Whether strokes are waiting in a preview for Keep or Drop. */
  waiting(){ return this.preview && this.dirty.size>0; },

  /** The preview's Keep / Drop row, and the line saying how strokes reach the patch. */
  showWait(){
    const row=$('#wgLandWait'); if(!row) return;
    const n=this.dirty.size, on=this.waiting();
    row.style.display=on? '' : 'none';
    if(on) $('#wgLandWaitN').textContent=n+' cell'+(n===1?'':'s')+' changed, shown only here';
    const how=$('#wgLandHow');
    if(how) how.textContent=(this.preview? 'Strokes are shown here first: Keep puts them into the Editor\'s patch, Drop puts the ground back. Leaving the Land view keeps them.'
      : 'Each stroke goes to the Editor\'s patch a moment after it ends.')+
      ' Take a cell out here or in the Editor\'s Pending and the ground goes back to the load order\'s.';
  },

  /** The scene's land before the first stroke of a preview, so Drop can put it back. The
   *  arrays a stroke changes in place are copied; a neighbour's texture ring changes with
   *  the cell's, so every exterior of the scene is kept (a few dozen small arrays). */
  snapshot(){
    if(!this.preview || this._before) return;
    const sc=App._scene, cp=a=>a? a.slice() : a;
    this._before=new Map();
    for(const c of (sc && sc.cells)||[]){
      if(c.kind!=='ext') continue;
      this._before.set(c.gx+','+c.gy, {c, heights:cp(c.heights), normals:cp(c.normals), colours:cp(c.colours),
        layers:c.layers? c.layers.map(L=>Object.assign({}, L, {blend:cp(L.blend)})) : c.layers,
        ids:cp(c._landIds), orig:c._landOrig, ring:c._ringOrig, applied:c._landApplied});
    }
  },

  /** The preview's strokes thrown away: the ground as it was before them, nothing written. */
  drop(){
    clearTimeout(this._commitT);
    const before=this._before; this._before=null;
    this.dirty.clear(); this._edited.clear(); this._stroke=null;
    if(before){
      const sc=App._scene, back=new Set(), gone=new Set();
      for(const [k,b] of before){
        const c=b.c;
        // A cell the scene no longer has (it moved on): nothing drawn to put back.
        if(!sc || !(sc.cells||[]).includes(c)) continue;
        const had=!!c.heights;
        c.heights=b.heights; c.normals=b.normals; c.colours=b.colours; c.layers=b.layers; c._landIds=b.ids;
        c._landOrig=b.orig; c._ringOrig=b.ring; c._landApplied=b.applied;
        if(c.heights) back.add(k); else if(had) gone.add(k);
      }
      // Land made here for a square that had none goes again.
      if(gone.size && sc && sc.chunks) sc.chunks=sc.chunks.filter(ch=>!(ch.grid && gone.has(ch.grid[0]+','+ch.grid[1])));
      this.rebuild(back);
      if(typeof WgCellGrid==='object' && WgCellGrid.on) WgCellGrid._scene=null;
    }
    this.status('Dropped: the ground is as it was');
    this.showWait();
  },

  /** The ring under the pointer: the brush's sphere on the drawn ground. */
  brushMove(e){
    const R=App.R; if(!R) return;
    const g=R.groundAt(e.clientX,e.clientY);
    if(!g){ R.setBrush(null); return; }
    const r=this.radius();
    const z=(g.length>2 && g[2]!=null)? g[2] : (R.groundZ? R.groundZ(g[0],g[1]) : 0);
    const inner=this.mode==='texture'? r : r*(1-this.feather/100);
    // The ring is the brush's own outline: its shape, aspect and angle (texture paints whole
    // texture cells, so a plain circle).
    const b=this.b, shaped=this.mode!=='texture';
    R.setBrush({x:g[0], y:g[1], z, r, inner, shape:shaped? (b.shape||'circle') : 'circle', aspect:shaped? b.aspect : 1, angle:shaped? b.angle : 0});
  },

  /** The cells of the scene with land, by "gx,gy". */
  cells(){
    const sc=App._scene; const m=new Map();
    for(const c of (sc && sc.cells)||[]) if(c.kind!=='int' && c.heights) m.set(c.gx+','+c.gy, c);
    return m;
  },

  /** The holders of a world vertex: each cell it belongs to, with its index there. */
  holders(cells, vx, vy){
    const N=this.N, out=[];
    const gx=Math.floor(vx/(N-1)), gy=Math.floor(vy/(N-1));
    for(const [dx,dy] of [[0,0],[-1,0],[0,-1],[-1,-1]]){
      const key=(gx+dx)+','+(gy+dy), c=cells.get(key); if(!c) continue;
      const lx=vx-(gx+dx)*(N-1), ly=vy-(gy+dy)*(N-1);
      if(lx>=0 && lx<N && ly>=0 && ly<N) out.push({key, c, k:ly*N+lx});
    }
    return out;
  },

  /** The world vertices inside the brush around (wx, wy), with their weights. */
  vertices(wx, wy){
    const r=this.radius(), S=this.STEP, out=[], b=this.b;
    const vx0=Math.ceil((wx-r)/S), vx1=Math.floor((wx+r)/S), vy0=Math.ceil((wy-r)/S), vy1=Math.floor((wy+r)/S);
    for(let vy=vy0; vy<=vy1; vy++) for(let vx=vx0; vx<=vx1; vx++){
      const d=WgLandBrush.dist(vx*S-wx, vy*S-wy, b);
      if(d>r) continue;
      out.push(vx, vy, WgLandBrush.weight(b.falloff, d, r, this.feather));
    }
    // A brush smaller than the land's grid still moves the vertex nearest it.
    if(!out.length && this.mode!=='texture') out.push(Math.round(wx/S), Math.round(wy/S), 1);
    return out;
  },

  mark(key, kind){ if(!this.dirty.has(key)) this.dirty.set(key,new Set()); this.dirty.get(key).add(kind); },

  /** What a cell had from the load order, kept before the first change, so an undo can
      put it back. */
  original(c){
    if(!c._landOrig) c._landOrig={heights:c.heights? Float32Array.from(c.heights) : null, normals:c.normals,
      colors:c.colours? Uint8Array.from(c.colours) : null, layers:c._ringOrig||c.layers, ids:c.heights? this.ids(c).slice() : null};
    c._ringOrig=null;
    return c._landOrig;
  },

  /** Flat land for a cell of the scene with none, at the empty square's own ground
      (DEFAULT_LAND_HEIGHT): drawn at once, made in the patch (New land) at the next commit. */
  newLand(c){
    const N=this.N, P=18;
    this.original(c);
    c.heights=new Float32Array(N*N).fill(DEFAULT_LAND_HEIGHT);
    const up=new Float32Array(N*N*3); for(let i=0;i<N*N;i++) up[i*3+2]=1;
    c.normals=up;
    c.layers=[{id:DEFAULT_LTEX, blend:new Uint8Array(P*P).fill(255), size:P, coverage:1}];
    c._landIds=new Array(this.TS*this.TS).fill(DEFAULT_LTEX);
    this.mark(c.gx+','+c.gy,'n');
    this._edited.set(c.gx+','+c.gy, c);
  },

  /** The patch's land over a cell as the scene is built (and after a sync): true when the
      cell changed and its terrain has to be built again. */
  apply(c, all){
    const key=c.gx+','+c.gy, s=this.saved.get(key);
    /* Strokes not yet written (a preview waiting, or the moment before a commit) on a cell
       the scene has just built again (a cell arrow, Go): carried onto the new cell object,
       so they are drawn, and what Keep writes is them rather than the load order's. */
    const ed=this.dirty.has(key)? this._edited.get(key) : null;
    if(ed && ed!==c){
      for(const f of ['heights','normals','colours','layers','_landIds','_landOrig','_ringOrig','_landApplied']) c[f]=ed[f];
      this._edited.set(key, c);
      if(this._before && this._before.has(key)) this._before.get(key).c=c;
      return true;
    }
    if(!s){
      /* Nothing in the patch for it: the load order's own, whether the change came from the
         patch (an undo) or from a stroke here that the patch no longer has (taken out of
         the pool). Static, 2026-11-02: "deleting it from the patch pool doesnt revert the
         changes" - a stroke's cell was only put back when it had been drawn from the
         patch. */
      if(!c._landOrig || this.dirty.has(key)) return false;
      const o=c._landOrig;
      c.heights=o.heights; c.normals=o.normals; c.colours=o.colors; c.layers=o.layers; c._landIds=o.ids;
      c._landOrig=null; c._landApplied=0;
      return true;
    }
    if(c._landApplied===s.rev || this.dirty.has(key)) return false;
    const o=this.original(c);
    const N=this.N;
    const map=new Map(); for(const x of (all||[])) if(x.kind!=='int') map.set(x.gx+','+x.gy,x);
    map.set(key,c);
    c.heights=s.heights? Float32Array.from(s.heights) : (o.heights? Float32Array.from(o.heights) : new Float32Array(N*N));
    c.colours=s.colors? Uint8Array.from(s.colors) : o.colors;
    if(s.heights) this.renormal(c,map); else c.normals=o.normals;
    if(s.ids){
      const was=o.ids||new Array(this.TS*this.TS).fill(DEFAULT_LTEX);
      c._landIds=s.ids.map((id,i)=>id==null? was[i] : id);
      this.relayer(c,map);
    }else if(!c.layers){
      c._landIds=new Array(this.TS*this.TS).fill(DEFAULT_LTEX); this.relayer(c,map);
    }
    c._landApplied=s.rev;
    return true;
  },

  /** What the patch has for every cell's land, read again; the scene's cells redrawn. */
  async sync(){
    if(typeof WgEditor!=='object' || !WgEditor.canEdit()) return;
    // A stroke not yet in the patch is newer than what the patch says.
    if(this._committing || this.dirty.size) return;
    let list;
    try{ list=await WgEditor.ask('editLandPending',{}); }catch(_){ return; }
    if(!Array.isArray(list) || this._committing || this.dirty.size) return;
    const next=new Map();
    for(const e of list){
      const m=/^\((-?\d+),\s*(-?\d+)\)$/.exec(String(e.cell||'')); if(!m) continue;
      next.set(m[1]+','+m[2], {rev:++this._rev, heights:e.heights||null, colors:e.colors||null, ids:e.ids||null});
    }
    const keys=new Set([...this.saved.keys(), ...next.keys()]);
    this.saved=next;
    const sc=App._scene; if(!sc) return;
    const changed=new Set();
    for(const c of (sc.cells||[])){
      if(c.kind!=='ext') continue;
      const k=c.gx+','+c.gy;
      // A cell drawn changed here (`_landOrig`) and no longer in the patch goes back too.
      if(!keys.has(k) && !c._landOrig) continue;
      if(this.apply(c, sc.cells)) changed.add(k);
    }
    if(changed.size){
      // Their neighbours' texture rings follow: worked out again from the ids where the
      // neighbour is changed too, else the load order's own put back.
      const cells=this.cells(), all=new Set(changed);
      for(const k of changed){
        const [gx,gy]=k.split(',').map(Number);
        for(let dy=-1;dy<=1;dy++) for(let dx=-1;dx<=1;dx++){
          const nk=(gx+dx)+','+(gy+dy), n=cells.get(nk); if(!n || changed.has(nk)) continue;
          if(n._landOrig){ this.relayer(n, cells); all.add(nk); }
          else if(n._ringOrig){ n.layers=n._ringOrig; n._ringOrig=null; all.add(nk); }
        }
      }
      this.rebuild(all);
    }
    this.drawPatch(list);
  },

  async pollPatch(){
    if(!this.on || document.hidden || this._polling) return;
    this._polling=true;
    try{
      const r=await WgEditor.ask('editRevision',{});
      if(this._patchRev!=null && r.rev!==this._patchRev && !this.dirty.size && !this._committing) await this.sync();
      this._patchRev=r.rev;
    }catch(_){}
    finally{ this._polling=false; }
  },

  /** One batch of a stroke: `pts` the ground points [x, y, ...] in viewport units. */
  /** The falloff's profile, centre at the left, the brush's edge at the right. */
  drawCurve(){
    const cv=this._el && this._el.querySelector('#wgLandCurve'); if(!cv || !cv.getContext) return;
    if(/jsdom/i.test(navigator.userAgent||'')) return;   // the page tests have no canvas
    const g=cv.getContext('2d'); if(!g) return;
    const W=cv.width, H=cv.height, cs=getComputedStyle(cv);
    const ac=cs.getPropertyValue('--ac').trim()||'#e0a050', tx=cs.getPropertyValue('--tx3').trim()||'#888';
    g.clearRect(0,0,W,H);
    g.strokeStyle=tx; g.globalAlpha=0.35; g.beginPath(); g.moveTo(0,H-2); g.lineTo(W,H-2); g.stroke(); g.globalAlpha=1;
    g.strokeStyle=ac; g.lineWidth=2; g.beginPath();
    for(let i=0;i<=W;i++){ const y=H-3-(H-6)*WgLandBrush.weight(this.b.falloff, i/W*100, 100, this.feather); i? g.lineTo(i,y) : g.moveTo(i,y); }
    g.stroke();
  },

  /** The round contour interval (1, 2 or 5 times a power of ten) that puts about a dozen
   *  lines over the loaded ground, as the old terrain view chose it; and the range. */
  heightRange(){
    let lo=Infinity, hi=-Infinity;
    for(const c of this.cells().values()) for(let i=0;i<c.heights.length;i+=7){ const h=c.heights[i]; if(h<lo) lo=h; if(h>hi) hi=h; }
    if(!isFinite(lo)) return {lo:0, hi:1024, interval:128};
    const span=Math.max(16, hi-lo), raw=span/12, p=Math.pow(10, Math.floor(Math.log10(raw)));
    const interval=[1,2,5,10].map(m=>m*p).reduce((a,b)=>Math.abs(Math.log(b/raw))<Math.abs(Math.log(a/raw))? b : a);
    return {lo, hi, interval};
  },
  /** The overlays onto the renderer (none outside the Land view). */
  applyOverlays(off){
    const R=App.R; if(!R) return;
    const o=R.opts, v=this.ov;
    if(off || !this.on){ o.hill=0; o.elev=0; o.contour=0; R.dirty=true; return; }
    const rg=this.heightRange();
    // The lights round the compass, weighted toward the azimuth; the weights sum to one.
    const n=Math.max(1, v.lights|0), L=[], alt=v.alt*Math.PI/180;
    let sum=0; const ws=[];
    for(let i=0;i<6;i++){
      if(i>=n){ ws.push(0); continue; }
      const w=n===1? 1 : 1+Math.cos(2*Math.PI*i/n); ws.push(w); sum+=w;
    }
    for(let i=0;i<6;i++){
      const az=(v.az+360*i/n)*Math.PI/180;
      L.push(Math.sin(az)*Math.cos(alt), Math.cos(az)*Math.cos(alt), Math.sin(alt), sum? ws[i]/sum : 0);
    }
    o.hill=+v.hill||0; o.hillGrey=!!v.hillGrey; o.hillLights=L;
    o.elev={none:0, hypso:1, rainbow:2, grey:3}[v.tint]||0; o.elevA=(+v.tintA||0)/100;
    o.elevLo=Math.min(-256, rg.lo); o.elevHi=Math.max(256, rg.hi);
    o.contour=v.contours? (v.interval>0? v.interval : rg.interval) : 0;
    const iv=this._el && this._el.querySelector('#wgOvIntV');
    if(iv) iv.textContent=v.contours? 'every '+o.contour+' u' : '';
    R.dirty=true;
  },

  /** The tool a dab uses: Shift smooths, Alt turns raise into lower (and back) and smooth
   *  into sharpen (and back). */
  activeTool(){
    if(this._mods.shift && this.mode==='height') return 'smooth';
    const t=this.tool;
    if(!this._mods.alt) return t;
    return {raise:'lower', lower:'raise', smooth:'sharpen', sharpen:'smooth'}[t]||t;
  },
  beginStroke(){
    this._stroke=WgLandBrush.begin(performance.now(), this.b);
    this._stroke.target=null; this._stroke.ramp=null;
    return this._stroke;
  },
  endStroke(){ this._stroke=null; cancelAnimationFrame(this._raf); this._raf=0; },
  /** While the button is held: the hold's dabs where the brush stands (the airbrush). */
  holdLoop(){
    cancelAnimationFrame(this._raf);
    this._lastT=performance.now();
    const step=()=>{
      if(!this.on || !this._pressed || !this._stroke){ this._raf=0; return; }
      this._raf=requestAnimationFrame(step);
      const now=performance.now(), dt=Math.min(0.1, (now-this._lastT)/1000); this._lastT=now;
      if(this.mode==='texture' || this._picking) return;
      const S=this._stroke, share=WgLandBrush.hold(S, now, dt);
      if(share>0 && S.cursor) this.applyDabs([[S.cursor[0], S.cursor[1], share]]);
    };
    this._raf=requestAnimationFrame(step);
  },

  /** One batch of a stroke: `pts` the ground points [x, y, ...] in viewport units. The
   *  engine decides where the dabs fall (the click, then the spacing along the path). */
  paint(pts){
    const sc=App._scene; if(!sc || !sc.origin) return;
    const cells=this.cells(); if(!cells.size) return;
    const [ox,oy]=sc.origin;
    if(this._picking && pts.length>=2){ this.pickTexture(cells, pts[0]+ox, pts[1]+oy); return; }
    if(this._pickHeight && pts.length>=2){
      this._pickHeight=false;
      const vx=Math.round((pts[0]+ox)/this.STEP), vy=Math.round((pts[1]+oy)/this.STEP), h=this.holders(cells,vx,vy)[0];
      if(h){ this.b.flatHeight=Math.round(h.c.heights[h.k]); this.b.flatTarget='fixed'; this.panel(); toast('Flatten to '+this.b.flatHeight,'ok',2000); }
      return;
    }
    const S=this._stroke||this.beginStroke(), now=performance.now(), r=this.radius();
    const dabs=[];
    for(let p=0;p+1<pts.length;p+=2) for(const d of WgLandBrush.along(S, pts[p]+ox, pts[p+1]+oy, r, now)) dabs.push(d);
    this.applyDabs(dabs);
  },

  /** Dabs [wx, wy, mult] in world units onto the ground, then the ground drawn again. */
  applyDabs(dabs){
    const sc=App._scene; if(!sc || !sc.origin || !dabs.length) return;
    const cells=this.cells(); if(!cells.size) return;
    this.snapshot();
    // Cells of the scene the brush reaches that have no land yet get flat land at the sea floor.
    const r=this.radius();
    for(const c of (sc.cells||[])){
      if(c.kind!=='ext' || c.heights) continue;
      for(const [wx, wy] of dabs){
        const x0=c.gx*this.CELL, y0=c.gy*this.CELL;
        if(wx+r>=x0 && wx-r<=x0+this.CELL && wy+r>=y0 && wy-r<=y0+this.CELL){ this.newLand(c); cells.set(c.gx+','+c.gy,c); break; }
      }
    }
    const touched=new Set();
    for(const [wx, wy, m] of dabs){
      if(!(m>0)) continue;
      if(this.mode==='height') this.dabHeight(cells, wx, wy, m, touched);
      else if(this.mode==='texture') this.dabTexture(cells, wx, wy, touched);
      else this.dabColor(cells, wx, wy, touched, m);
    }
    if(!touched.size) return;
    for(const k of touched) this._edited.set(k, cells.get(k));
    if(this.mode==='height') for(const k of touched) this.renormal(cells.get(k), cells);
    let rebuild=touched;
    if(this.mode==='texture'){
      // A cell's coverage maps reach one texture cell into each neighbour: theirs change too.
      rebuild=new Set();
      for(const k of touched){
        const [gx,gy]=k.split(',').map(Number);
        for(let dy=-1;dy<=1;dy++) for(let dx=-1;dx<=1;dx++){ const n=(gx+dx)+','+(gy+dy); if(cells.has(n)) rebuild.add(n); }
      }
      for(const k of rebuild) this.relayer(cells.get(k), cells);
    }
    this.rebuild(rebuild);
    if(this.ov.tint!=='none' || this.ov.contours){ clearTimeout(this._ovT); this._ovT=setTimeout(()=>this.applyOverlays(), 300); }
    if(this.preview){ this.status(''); this.showWait(); return; }
    this.status(this.dirty.size+' cell'+(this.dirty.size===1?'':'s')+' changed, not yet in the patch');
    this.scheduleCommit();
  },

  /** One dab of a height tool at world (wx, wy), at `mult` of the strength. */
  dabHeight(cells, wx, wy, mult, touched){
    const S=this._stroke||this.beginStroke(), b=this.b, tool=this.activeTool(), STEP=this.STEP;
    const at=(vx,vy)=>{ const h=this.holders(cells,vx,vy)[0]; return h? h.c.heights[h.k] : null; };
    const vs=this.vertices(wx,wy);
    if(!vs.length) return;
    const here=at(Math.round(wx/STEP), Math.round(wy/STEP));
    // What the stroke aims at, fixed where it began.
    if(tool==='flatten' && S.target==null){
      if(b.flatTarget==='fixed') S.target=+b.flatHeight||0;
      else if(b.flatTarget==='sea') S.target=0;
      else if(b.flatTarget==='average'){
        let s=0, n=0; for(let i=0;i<vs.length;i+=3){ const h=at(vs[i],vs[i+1]); if(h!=null){ s+=h*vs[i+2]; n+=vs[i+2]; } }
        S.target=n? s/n : here;
      }else S.target=here;
      if(S.target==null) return;
    }
    if(tool==='ramp'){
      if(!S.ramp){ if(here==null) return; S.ramp={x0:wx, y0:wy, z0:here}; }
      S.ramp.x1=wx; S.ramp.y1=wy; S.ramp.z1=here!=null? here : S.ramp.z0;
    }
    const k=Math.min(1, this.strength/100)*mult;
    const p={tool, amount:this.strength*mult, k, smooth:b.smooth, wide:b.wide, target:S.target, only:b.flatOnly,
      ramp:S.ramp, step:STEP, radius:this.radius(), feather:this.feather, falloff:b.falloff,
      noise:{scale:b.noiseScale, octaves:b.noiseOct, rough:b.noiseRough, seed:b.noiseSeed},
      terrace:b.terrace, edge:b.edge, talus:b.talus};
    // The ramp's sides follow the line, not the dab: every vertex of the brush gets weight 1.
    let verts=vs;
    if(tool==='ramp'){ verts=vs.slice(); for(let i=2;i<verts.length;i+=3) verts[i]=1; }
    const changes=WgLandBrush.dab({get:at}, verts, p);
    // Applied after the dab is measured, so smoothing reads one surface, not a moving one.
    for(let i=0;i<changes.length;i+=3){
      const vx=changes[i], vy=changes[i+1], from=at(vx,vy);
      const to=WgLandBrush.capped(S, vx+','+vy, from, changes[i+2]);
      if(to===from) continue;
      for(const h of this.holders(cells,vx,vy)){
        this.original(h.c); h.c.heights[h.k]=to; touched.add(h.key); this.mark(h.key,'h');
      }
    }
  },

  dabColor(cells, wx, wy, touched, mult){
    const hex=this.color.replace('#',''), rgb=[0,2,4].map(i=>parseInt(hex.substr(i,2),16));
    const k=Math.min(1,this.strength/100*(mult==null? 1 : mult)), vs=this.vertices(wx,wy);
    for(let i=0;i<vs.length;i+=3) for(const h of this.holders(cells,vs[i],vs[i+1])){
      const c=h.c;
      this.original(c);
      if(!c.colours || c.colours.length<this.N*this.N*3){ c.colours=new Uint8Array(this.N*this.N*3).fill(255); }
      for(let j=0;j<3;j++){ const o=h.k*3+j, v=c.colours[o]; c.colours[o]=Math.round(v+(rgb[j]-v)*vs[i+2]*k); }
      touched.add(h.key); this.mark(h.key,'c');
    }
  },

  /** A cell's 16 x 16 land texture ids, row by row from the south-west, read once from its
      coverage maps (each inner texel belongs to exactly one layer). */
  ids(c){
    if(c._landIds) return c._landIds;
    const P=18, TS=this.TS, out=new Array(TS*TS).fill(DEFAULT_LTEX);
    for(const L of (c.layers||[])) for(let j=0;j<TS;j++) for(let i=0;i<TS;i++) if(L.blend[(j+1)*P+i+1]===255) out[j*TS+i]=L.id||DEFAULT_LTEX;
    return (c._landIds=out);
  },

  dabTexture(cells, wx, wy, touched){
    if(!this.texture){ this.status('Name a land texture to paint with (or Pick one off the ground)'); return; }
    const r=this.radius(), TS=this.TS, T=this.TSTEP, CELL=this.CELL;
    const tx0=Math.floor((wx-r)/T), tx1=Math.floor((wx+r)/T), ty0=Math.floor((wy-r)/T), ty1=Math.floor((wy+r)/T);
    for(let ty=ty0; ty<=ty1; ty++) for(let tx=tx0; tx<=tx1; tx++){
      if(Math.hypot((tx+0.5)*T-wx, (ty+0.5)*T-wy)>r) continue;
      const gx=Math.floor(tx*T/CELL), gy=Math.floor(ty*T/CELL), key=gx+','+gy, c=cells.get(key);
      if(!c) continue;
      const ids=this.ids(c), k=(ty-gy*TS)*TS+(tx-gx*TS);
      if(ids[k]===this.texture) continue;
      this.original(c); ids[k]=this.texture; touched.add(key); this.mark(key,'t');
    }
  },

  pickTexture(cells, wx, wy){
    const T=this.TSTEP, TS=this.TS, tx=Math.floor(wx/T), ty=Math.floor(wy/T);
    const gx=Math.floor(tx/TS), gy=Math.floor(ty/TS), c=cells.get(gx+','+gy);
    this._picking=false;
    if(c) this.texture=this.ids(c)[(ty-gy*TS)*TS+(tx-gx*TS)];
    if(this._drawTex) this._drawTex();
    if(this._show) this._show();
  },

  /** A cell's coverage maps rebuilt from the texture ids, as `viewcore::coverage::layers_with`
      builds them: an 18 x 18 field, the cell's 16 x 16 and a ring of its neighbours' (the
      edge held where there is no neighbour), the commonest texture first. */
  relayer(c, cells){
    if(!c) return;
    const P=18, TS=this.TS;
    const at=(gi,gj)=>{
      const dx=gi<0?-1:gi>=TS?1:0, dy=gj<0?-1:gj>=TS?1:0;
      if(dx||dy){
        const n=cells.get((c.gx+dx)+','+(c.gy+dy));
        if(n) return this.ids(n)[((gj+TS)%TS)*TS+((gi+TS)%TS)];
      }
      return this.ids(c)[Math.min(TS-1,Math.max(0,gj))*TS+Math.min(TS-1,Math.max(0,gi))];
    };
    const ids=[], blends=[], counts=[];
    for(let j=0;j<P;j++) for(let i=0;i<P;i++){
      const gi=i-1, gj=j-1, id=at(gi,gj);
      let k=ids.indexOf(id);
      if(k<0){ ids.push(id); blends.push(new Uint8Array(P*P)); counts.push(0); k=ids.length-1; }
      blends[k][j*P+i]=255;
      if(gi>=0 && gi<TS && gj>=0 && gj<TS) counts[k]++;
    }
    // A neighbour only re-ringed keeps its own layers, to be put back (`sync`).
    if(!c._landOrig && !c._ringOrig) c._ringOrig=c.layers;
    const layers=ids.map((id,k)=>({id, blend:blends[k], size:P, coverage:counts[k]/(TS*TS), _first:k}));
    layers.sort((a,b)=>(b.coverage-a.coverage) || (a._first-b._first));
    for(const L of layers) delete L._first;
    c.layers=layers;
  },

  /** A cell's lighting normals from its new heights (the engine's own are for the old
      ground): the slopes across each vertex, unit length. */
  renormal(c, cells){
    if(!c) return;
    const N=this.N, S=this.STEP, H=c.heights;
    const at=(x,y)=>{
      if(x>=0 && x<N && y>=0 && y<N) return H[y*N+x];
      const nb=cells.get((c.gx+(x<0?-1:x>=N?1:0))+','+(c.gy+(y<0?-1:y>=N?1:0)));
      if(!nb) return H[Math.min(N-1,Math.max(0,y))*N+Math.min(N-1,Math.max(0,x))];
      // A neighbour square with no land: its flat ground (Static, 2026-11-02: new land
      // beside one threw "Cannot read properties of null (reading '63')").
      if(!nb.heights) return DEFAULT_LAND_HEIGHT;
      const lx=x<0? x+N-1 : x>=N? x-N+1 : x, ly=y<0? y+N-1 : y>=N? y-N+1 : y;
      return nb.heights[ly*N+lx];
    };
    const out=new Float32Array(N*N*3);
    for(let y=0;y<N;y++) for(let x=0;x<N;x++){
      const dx=(at(x+1,y)-at(x-1,y))/(2*S), dy=(at(x,y+1)-at(x,y-1))/(2*S);
      const l=Math.hypot(dx,dy,1), k=(y*N+x)*3;
      out[k]=-dx/l; out[k+1]=-dy/l; out[k+2]=1/l;
    }
    c.normals=out;
  },

  /** The given cells' terrain built again, the others' kept. */
  rebuild(keys){
    const sc=App._scene, R=App.R; if(!sc || !R) return;
    const cells=this.cells();
    const chunks=(sc.chunks||[]).slice();
    for(const key of keys){
      const c=cells.get(key); if(!c) continue;
      const i=chunks.findIndex(ch=>ch.grid && ch.grid[0]===c.gx && ch.grid[1]===c.gy);
      const old=i>=0? chunks[i] : null;
      const T=cellTerrain(c,(dx,dy)=>cells.get((c.gx+dx)+','+(c.gy+dy))||null,0);
      const off=cellOffsetFrom(sc.origin,c);
      if(off[0]||off[1]){ const p=T.mesh.pos; for(let j=0;j<p.length;j+=3){ p[j]+=off[0]; p[j+1]+=off[1]; } }
      // The layers' textures carried over by id; a texture new to the cell is loaded.
      const was=new Map(((old&&old.layers)||[]).map(L=>[L.id,L]));
      const loads=[];
      for(const L of T.layers){
        const o=was.get(L.id);
        if(o){ L.tex=o.tex; L.nrm=o.nrm; L.flat=o.flat; L.specA=o.specA; L.absent=o.absent; continue; }
        const file=L.id? GameData.textureFileFor(L.id) : null;
        if(L.id===DEFAULT_LTEX) L.flat=DEFAULT_LTEX_COLOR;
        if(file) loads.push(loadTexture(file).then(t=>{ if(t && t.gl){ L.tex=t.gl; L.flat=null; } }).catch(()=>{}));
        else if(L.id && L.id!==DEFAULT_LTEX) L.absent=true;
      }
      const ch={mesh:T.mesh,layers:T.layers,inner:T.inner,total:T.total,span:T.span,grid:[c.gx,c.gy]};
      if(i>=0) chunks[i]=ch; else chunks.push(ch);
      /* The renderer copies each layer's texture when the terrain is handed over, so one
         loaded afterwards needs the chunk handed over again - as a new object, since the
         same one is kept as it was. */
      if(loads.length) Promise.all(loads).then(()=>{
        const s2=App._scene; if(!s2 || !s2.chunks) return;
        const j=s2.chunks.indexOf(ch); if(j<0) return;
        s2.chunks=s2.chunks.slice(); s2.chunks[j]=Object.assign({}, ch);
        R.setCellTerrain(s2.chunks); R.dirty=true;
      });
    }
    sc.chunks=chunks;
    R.setCellTerrain(chunks);
    R.dirty=true;
  },

  scheduleCommit(){
    clearTimeout(this._commitT);
    if(this.preview) return;
    this._commitT=setTimeout(()=>this.commit(), 700);
  },

  /** The changed cells into the Editor's patch. */
  async commit(){
    clearTimeout(this._commitT);
    this._before=null;
    if(!this._pressed) this._stroke=null;
    if(!this.dirty.size){ this.showWait(); return; }
    // A changed cell the scene has since left is written from the object the strokes changed.
    const cells=this.cells(), work=[...this.dirty], edited=new Map(this._edited); this.dirty.clear(); this._edited.clear();
    this._committing=true;
    let clamped=0, done=0, made=0;
    for(const [key,kinds] of work){
      const c=cells.get(key)||edited.get(key); if(!c || !c.heights) continue;
      const [gx,gy]=key.split(',').map(Number), cell='('+gx+', '+gy+')';
      try{
        if(kinds.has('n')) await WgEditor.ask('editNewLand',{cell, height:0});
        if(kinds.has('h')){ const r=await WgEditor.ask('editLandSet',{cell, heights:Array.from(c.heights, v=>Math.round(v))}); clamped+=(r&&r.clamped)||0; }
        if(kinds.has('t')){ const r=await WgEditor.ask('editLandTextures',{cell, ids:this.ids(c)}); made+=(r&&r.textures)||0; }
        if(kinds.has('c') && c.colours) await WgEditor.ask('editLandColors',{cell, colors:Array.from(c.colours)});
        done++;
      }catch(e){ toast(cell+': '+String(e.message||e),'err',6000); }
    }
    this.status(done? done+' cell'+(done===1?'':'s')+' in the patch'+
      (made? ' · '+made+' land texture'+(made===1?'':'s')+' added to the patch' : '')+
      (clamped? ' · '+clamped+' step(s) too steep for the format, clamped' : '') : '');
    this._committing=false;
    this.showWait();
    // What the patch now has, so a later reconnect or undo draws it.
    await this.sync();
    // The cell borders lie on the ground: drawn again on the new ground.
    if(typeof WgCellGrid==='object' && WgCellGrid.on) WgCellGrid._scene=null;
    if(typeof WgEditor==='object' && WgEditor.refreshPending) try{ WgEditor.refreshPending(); }catch(_){}
  },
};
