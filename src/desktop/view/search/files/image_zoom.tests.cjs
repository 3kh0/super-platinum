const {test} = require('node:test');
const assert = require('node:assert/strict');
const {nativeImageFit:fit, nativeImageScale:scale, nativeImageStep:step, installNativeImageZoom:install} = require('./image_zoom.js');

// Execute the unchanged product controller against a small DOM host. These
// tests establish math/listener behavior, never native layout or trusted input.
function host({w=512,h=512,vw=400,vh=320,complete=true,left=60,top=80,borderLeft=2,borderTop=3,barSize=0}={}) {
  const observers=[];
  class Element {
    constructor(){this.dataset={};this.style={};this.attrs={};this.listeners=new Map();this.listenerOptions=new Map();this.removed=[];this.disabled=true;this.value='0';this.isConnected=true;}
    addEventListener(k,f,options){this.listeners.set(k,f);this.listenerOptions.set(k,options);}
    removeEventListener(k,f,options){this.removed.push({type:k,listener:f,options});if(this.listeners.get(k)===f){this.listeners.delete(k);this.listenerOptions.delete(k);}}
    setAttribute(k,v){assert.notEqual(k,'src','controller must not assign a resource');this.attrs[k]=v;}
    removeAttribute(k){delete this.attrs[k];}
    getAttribute(k){return this.attrs[k]??null;}
    fire(k,target=this,fields={}){
      const event={type:k,target,currentTarget:this,defaultPrevented:false,preventDefault(){this.prevented=true;if(this.cancelable!==false)this.defaultPrevented=true;},stopPropagation(){this.stopped=true;},...fields};
      this.listeners.get(k)?.(event);return event;
    }
  }
  const root=new Element(),panel=new Element(),viewport=new Element(),canvas=new Element();
  const image=new Element(),slider=new Element(),button=new Element(),label=new Element();
  const plus=new Element(),minus=new Element();
  const nodes={viewport,canvas,image,slider,fit:button,label,plus,minus};
  const selectors=Object.fromEntries(Object.entries(nodes).map(([k,v])=>[`[data-image-zoom-${k}]`,[v]]));
  root.querySelectorAll=s=>selectors[s]??[];
  root.closest=()=>panel;
  root.dataset.imageZoomSource='http://super-platinum-media.localhost/image/opaque';
  image.attrs.src=root.dataset.imageZoomSource+'?v=1';
  image.naturalWidth=w;image.naturalHeight=h;image.complete=complete;
  viewport.clientWidth=vw;viewport.clientHeight=vh;
  // Ordinary untransformed LTR CSS boxes, with optional scrollbar layout.
  const box={left,top,borderLeft,borderTop,baseWidth:vw,baseHeight:vh,barSize};
  const bars=()=>{
    let horizontal=false,vertical=false;
    for(let n=0;n<3;n++) {
      horizontal=(parseFloat(canvas.style.width)||0)>box.baseWidth-(vertical?box.barSize:0);
      vertical=(parseFloat(canvas.style.height)||0)>box.baseHeight-(horizontal?box.barSize:0);
    }
    return {horizontal,vertical};
  };
  for(const [key,base,bar] of [['clientWidth','baseWidth','vertical'],['clientHeight','baseHeight','horizontal']])
    Object.defineProperty(viewport,key,{get:()=>box[base]-(bars()[bar]?box.barSize:0),set:value=>{box[base]=value;}});
  viewport.clientLeft=box.borderLeft;viewport.clientTop=box.borderTop;
  const rect=(x,y,width,height)=>({x,y,left:x,top:y,width,height,right:x+width,bottom:y+height});
  const canvasWidth=()=>Math.max(viewport.clientWidth,parseFloat(canvas.style.width)||0);
  const canvasHeight=()=>Math.max(viewport.clientHeight,parseFloat(canvas.style.height)||0);
  const imageWidth=()=>parseFloat(image.style.width)||image.naturalWidth;
  const imageHeight=()=>parseFloat(image.style.height)||image.naturalHeight;
  Object.defineProperty(viewport,'offsetWidth',{get:()=>box.baseWidth+box.borderLeft+2});
  Object.defineProperty(viewport,'offsetHeight',{get:()=>box.baseHeight+box.borderTop+3});
  viewport.getBoundingClientRect=()=>rect(box.left,box.top,viewport.offsetWidth,viewport.offsetHeight);
  canvas.getBoundingClientRect=()=>rect(box.left+viewport.clientLeft-viewport.scrollLeft,box.top+viewport.clientTop-viewport.scrollTop,canvasWidth(),canvasHeight());
  image.getBoundingClientRect=()=>{
    const c=canvas.getBoundingClientRect();return rect(c.left+(c.width-imageWidth())/2,c.top+(c.height-imageHeight())/2,imageWidth(),imageHeight());
  };
  for(const [node,width,height] of [[canvas,canvasWidth,canvasHeight],[image,imageWidth,imageHeight]]) {
    Object.defineProperty(node,'offsetWidth',{get:()=>Math.round(width())});Object.defineProperty(node,'offsetHeight',{get:()=>Math.round(height())});
    Object.defineProperty(node,'clientWidth',{get:()=>Math.round(width())});Object.defineProperty(node,'clientHeight',{get:()=>Math.round(height())});
  }
  const captures=new Set();
  viewport.setPointerCapture=id=>captures.add(id);
  viewport.hasPointerCapture=id=>captures.has(id);
  viewport.releasePointerCapture=id=>{captures.delete(id);viewport.fire('lostpointercapture',viewport,{pointerId:id});};
  for(const [axis,extent,size] of [['scrollWidth','width','clientWidth'],['scrollHeight','height','clientHeight']])
    Object.defineProperty(viewport,axis,{get:()=>Math.max(viewport[size],parseFloat(canvas.style[extent])||0)});
  for(const [axis,extent,size] of [['scrollLeft','width','clientWidth'],['scrollTop','height','clientHeight']]) {
    let value=0;
    Object.defineProperty(viewport,axis,{get:()=>value,set:v=>{
      const limit=Math.max(0,(parseFloat(canvas.style[extent])||viewport[size])-viewport[size]);
      value=Math.max(0,Math.min(limit,v));
    }});
  }
  const roots=[root];
  const body=new Element(),documentElement=new Element(),modal=new Element();
  // Real Search has .modal translateX(-50%); these DOMRects already include
  // that ancestor's client-coordinate translation.
  modal.computedStyle={transform:'matrix(1, 0, 0, 1, -250, 0)'};
  image.parentElement=canvas;canvas.parentElement=viewport;viewport.parentElement=root;root.parentElement=panel;panel.parentElement=modal;modal.parentElement=body;body.parentElement=documentElement;documentElement.parentElement=null;
  global.document={body,documentElement,querySelectorAll:()=>roots,
    addEventListener(){throw Error('no global input listener');},removeEventListener(){throw Error('no global input listener');}};
  global.CSS={escape:v=>v};
  global.getComputedStyle=node=>({direction:'ltr',transform:'none',zoom:'1',paddingLeft:'0px',paddingTop:'0px',paddingRight:'0px',paddingBottom:'0px',...node.computedStyle});
  for(const name of ['ResizeObserver','MutationObserver'])global[name]=class {
    constructor(cb){this.cb=cb;this.name=name;observers.push(this);}
    observe(){} disconnect(){this.disconnected=true;}
  };
  install('zoom-a');
  return {root,panel,viewport,canvas,image,slider,button,label,plus,minus,selectors,roots,observers,captures,box,modal,
    resize:()=>observers.findLast(o=>o.name==='ResizeObserver').cb(),
    mutate:()=>observers.findLast(o=>o.name==='MutationObserver').cb(),
    zoom:u=>{slider.value=String(u);slider.fire('input');}};
}
test('positive geometry and normalized slider map to fit through2x; invalid domains refuse',()=>{
  assert.equal(fit(512,512,400,320),.625);
  assert.equal(fit(10,20,400,320),1);
  for(const values of [[0,1,1,1],[1,-1,1,1],[1,1,Infinity,1],[1,1,1,NaN]])assert.equal(fit(...values),null);
  for(const f of [.01,.25,.625,1]) {
    assert.equal(scale(f,0),f);assert.equal(scale(f,1),2);assert.equal(scale(f,.5),f+.5*(2-f));
  }
  for(const u of [-.1,1.1,NaN,Infinity])assert.equal(scale(.5,u),null);
});
test('actual Fit/slider listeners change scale and centered scroll without resource assignment',()=>{
  const h=host(),src=h.image.attrs.src;
  assert.equal(h.label.textContent,'63%');assert.equal(h.slider.disabled,false);assert.equal(h.button.disabled,true);
  h.zoom(.5);assert.equal(h.image.style.width,'672px');assert.equal(h.label.textContent,'131%');
  assert.equal(h.viewport.scrollLeft,136);assert.equal(h.viewport.scrollTop,176);
  h.zoom(1);assert.equal(h.image.style.width,'1024px');assert.equal(h.label.textContent,'200%');
  h.viewport.scrollLeft=200;h.viewport.scrollTop=300;
  h.button.fire('click');assert.equal(h.image.style.width,'320px');assert.equal(h.label.textContent,'63%');
  assert.equal(h.viewport.scrollLeft,0);assert.equal(h.viewport.scrollTop,0);assert.equal(h.image.attrs.src,src);
});
test('incomplete or zero-sized geometry stays disabled until original image and viewport are ready',()=>{
  const h=host({complete:false});assert.equal(h.slider.disabled,true);assert.equal(h.canvas.style.width,undefined);
  h.image.complete=true;h.viewport.clientWidth=0;h.image.fire('load');assert.equal(h.slider.disabled,true);
  h.viewport.clientWidth=400;h.resize();assert.equal(h.label.textContent,'63%');assert.equal(h.slider.disabled,false);
  h.image.fire('error');assert.equal(h.slider.disabled,true);assert.equal(h.canvas.dataset.imageZoomReady,'false');
});
test('Source resize follows fit only from fit, otherwise preserves absolute zoom including below new fit',()=>{
  const h=host();h.viewport.clientWidth=512;h.viewport.clientHeight=512;h.resize();assert.equal(h.label.textContent,'100%');
  h.viewport.clientWidth=128;h.viewport.clientHeight=128;h.resize();assert.equal(h.label.textContent,'25%');
  h.zoom(.1);assert.equal(h.label.textContent,'43%');
  h.viewport.clientWidth=1000;h.viewport.clientHeight=1000;h.resize();
  assert.equal(h.label.textContent,'43%');assert.equal(h.slider.value,'0');assert.equal(h.button.disabled,true);
  h.zoom(.5);assert.equal(h.label.textContent,'150%');
});
test('actions remeasure geometry before a queued resize and reject invalid or peer input',()=>{
  const h=host();h.viewport.clientWidth=512;h.viewport.clientHeight=512;
  h.zoom(.5);assert.equal(h.label.textContent,'150%');
  const width=h.image.style.width;
  for(const value of ['NaN','Infinity','-1','2','']){h.zoom(value);assert.equal(h.image.style.width,width);}
  h.slider.value='0';h.slider.fire('input',h.button);assert.equal(h.image.style.width,width);
});
test('media epoch is harmless; source/geometry replacement retires the old controller',()=>{
  const h=host(),old=h.slider.listeners.get('input');
  h.image.attrs.src=h.root.dataset.imageZoomSource+'?v=99';h.mutate();h.zoom(1);assert.equal(h.label.textContent,'200%');
  h.image.naturalWidth=600;h.resize();assert.equal(h.slider.disabled,true);
  const width=h.image.style.width;h.slider.value='0';old({target:h.slider});assert.equal(h.image.style.width,width);
  assert(h.observers.every(o=>o.disconnected));
  const other=host();other.image.attrs.src='https://not-the-owned-source.invalid/image';other.mutate();assert.equal(other.slider.disabled,true);
});
test('reentry resets fit and old callbacks cannot disable or move the new owner',()=>{
  const h=host(),old=h.slider.listeners.get('input'),oldResize=h.observers[0].cb;
  h.zoom(1);install('zoom-a');assert.equal(h.label.textContent,'63%');assert.equal(h.slider.disabled,false);
  h.slider.value='1';old({target:h.slider});oldResize();assert.equal(h.label.textContent,'63%');assert.equal(h.slider.disabled,false);
  h.zoom(1);assert.equal(h.label.textContent,'200%');
});
test('root removal, duplicate root and same-key element replacement fail closed without touching peers',()=>{
  for(const kind of ['removed','duplicate','element']) {
    const h=host(),old=h.slider.listeners.get('input'),width=h.image.style.width;
    if(kind==='removed')h.root.isConnected=false;
    if(kind==='duplicate')h.roots.push({});
    if(kind==='element')h.selectors['[data-image-zoom-slider]']=[{}];
    h.mutate();assert.equal(h.slider.disabled,true);h.slider.value='1';old({target:h.slider});assert.equal(h.image.style.width,width);
    assert(h.observers.every(o=>o.disconnected));
  }
});

test('actual-scale steps use strict next/previous levels, clamp to fit and preserve endpoint no-ops',()=>{
  for(const [f,up,down] of [
    [.3,[.5,1,1.5,2],[1.5,1,.5,.3]],
    [.8,[1,1.5,2],[1.5,1,.8]],
    [1,[1.5,2],[1.5,1]],
  ]) {
    let s=f;
    for(const next of up){s=step(f,s,'in');assert.equal(s,next);}
    assert.equal(step(f,s,'in'),2);
    for(const next of down){s=step(f,s,'out');assert.equal(s,next);}
    assert.equal(step(f,s,'out'),f);
  }
  assert.equal(step(.3,.73,'in'),1);assert.equal(step(.3,.73,'out'),.5);
  assert.equal(step(.3,.31,'out'),.3); // found zero is retained then clamped
  assert.equal(step(.9,.5,'in'),1);assert.equal(step(.9,.5,'out'),.9);
  for(const [f,s,d] of [[0,1,'in'],[NaN,1,'in'],[1.1,1,'out'],[.3,0,'in'],[.3,NaN,'in'],[.3,Infinity,'out'],[.3,2.1,'out'],[.3,1,'peer']])assert.equal(step(f,s,d),null);
  assert.equal(step(.3,1,'out'),.5);assert.equal(step(.3,.5,'in'),1);
});

test('registered step buttons accept their SVG child, center inspection and retain the owned resource',()=>{
  const h=host({w:1000,h:1000,vw:300,vh:300}),src=h.image.attrs.src;
  assert.equal(h.plus.disabled,false);assert.equal(h.minus.disabled,false);
  h.minus.fire('click');assert.equal(h.label.textContent,'30%');
  h.plus.fire('click',{});assert.equal(h.label.textContent,'50%');
  assert.equal(h.viewport.scrollLeft,100);assert.equal(h.viewport.scrollTop,100);
  h.viewport.scrollLeft=150;h.viewport.scrollTop=50;
  h.plus.fire('click');assert.equal(h.label.textContent,'100%');
  assert.equal(h.viewport.scrollLeft,450);assert.equal(h.viewport.scrollTop,250);
  h.plus.fire('click');h.plus.fire('click');assert.equal(h.label.textContent,'200%');
  const center=[h.viewport.scrollLeft,h.viewport.scrollTop];
  h.plus.fire('click');assert.deepEqual([h.viewport.scrollLeft,h.viewport.scrollTop],center);
  for(const percent of ['150%','100%','50%','30%']){h.minus.fire('click');assert.equal(h.label.textContent,percent);}
  h.minus.fire('click');assert.equal(h.label.textContent,'30%');
  assert.equal(h.plus.disabled,false);assert.equal(h.minus.disabled,false);assert.equal(h.image.attrs.src,src);
});

test('step click remeasures queued viewport changes and handles an absolute scale below the new fit',()=>{
  for(const direction of ['plus','minus']) {
    const h=host({w:1000,h:1000,vw:400,vh:400});
    h.plus.fire('click');assert.equal(h.label.textContent,'50%');
    h.viewport.clientWidth=900;h.viewport.clientHeight=900;
    h[direction].fire('click');assert.equal(h.label.textContent,direction==='plus'?'100%':'90%');
  }
  const h=host({w:1000,h:1000,vw:400,vh:400});
  h.viewport.clientWidth=900;h.viewport.clientHeight=900;
  h.plus.fire('click');assert.equal(h.label.textContent,'100%');
  const width=h.image.style.width,callback=h.plus.listeners.get('click');
  callback({target:h.minus,currentTarget:h.minus});assert.equal(h.image.style.width,width);
});

test('step readiness and original-node retirement reject unavailable or replaced controls',()=>{
  const cold=host({complete:false});
  assert.equal(cold.plus.disabled,true);assert.equal(cold.minus.disabled,true);
  cold.plus.fire('click');assert.equal(cold.image.style.width,undefined);
  cold.image.complete=true;cold.viewport.clientWidth=0;cold.image.fire('load');
  assert.equal(cold.plus.disabled,true);assert.equal(cold.minus.disabled,true);
  cold.viewport.clientWidth=400;cold.resize();assert.equal(cold.plus.disabled,false);
  cold.image.fire('error');assert.equal(cold.plus.disabled,true);assert.equal(cold.minus.disabled,true);
  for(const kind of ['plus','minus','duplicate','source','geometry']) {
    const h=host(),old=h.plus.listeners.get('click'),width=h.image.style.width;
    if(kind==='plus'||kind==='minus')h.selectors[`[data-image-zoom-${kind}]`]=[{}];
    if(kind==='duplicate')h.selectors['[data-image-zoom-plus]'].push({});
    if(kind==='source')h.image.attrs.src='https://unowned.invalid/image';
    if(kind==='geometry')h.image.naturalWidth=600;
    old({target:h.plus,currentTarget:h.plus});
    assert.equal(h.plus.disabled,true);assert.equal(h.minus.disabled,true);
    assert.equal(h.image.style.width,width);assert(h.observers.every(o=>o.disconnected));
  }
});

test('retired step listeners and queued peer observers cannot borrow a reentered owner',()=>{
  const h=host(),oldPlus=h.plus.listeners.get('click'),oldMinus=h.minus.listeners.get('click');
  const queued=h.observers.map(o=>o.cb);
  h.plus.fire('click');install('zoom-a');
  oldPlus({target:h.plus,currentTarget:h.plus});oldMinus({target:h.minus,currentTarget:h.minus});
  for(const callback of queued)callback();
  assert.equal(h.label.textContent,'63%');assert.equal(h.plus.disabled,false);assert.equal(h.minus.disabled,false);
  h.plus.fire('click');assert.equal(h.label.textContent,'100%');
  h.minus.fire('click');assert.equal(h.label.textContent,'63%');
});

// Local pointer adapter: synthetic events only, not trusted/native input proof.
function pointer(h,kind,fields={},target=kind==='pointerdown'?h.image:h.viewport) {
  return h.viewport.fire(kind,target,{pointerId:7,pointerType:'mouse',isPrimary:true,
    button:kind==='pointermove'?-1:0,buttons:kind==='pointerup'?0:1,
    clientX:100,clientY:100,ctrlKey:false,metaKey:false,altKey:false,shiftKey:false,...fields});
}
function zoomed() {
  const h=host({w:1000,h:800,vw:400,vh:320});h.zoom(1);
  h.viewport.scrollLeft=600;h.viewport.scrollTop=500;return h;
}

test('local image drag follows the hand on both axes, clamps overflow and retains scale/resource',()=>{
  const h=zoomed(),src=h.image.attrs.src,width=h.image.style.width,label=h.label.textContent;
  assert.equal(h.canvas.dataset.imageZoomPanReady,'true');
  assert.equal(pointer(h,'pointerdown').stopped,true);assert(h.captures.has(7));
  pointer(h,'pointermove',{clientX:150,clientY:130});
  assert.deepEqual([h.viewport.scrollLeft,h.viewport.scrollTop],[550,470]);
  pointer(h,'pointermove',{clientX:-10000,clientY:-10000});
  assert.deepEqual([h.viewport.scrollLeft,h.viewport.scrollTop],[1600,1280]);
  pointer(h,'pointermove',{clientX:-9990,clientY:-9990});
  assert.deepEqual([h.viewport.scrollLeft,h.viewport.scrollTop],[1590,1270]);
  pointer(h,'pointermove',{clientX:10000,clientY:10000});
  assert.deepEqual([h.viewport.scrollLeft,h.viewport.scrollTop],[0,0]);
  pointer(h,'pointermove',{clientX:9990,clientY:9990});
  assert.deepEqual([h.viewport.scrollLeft,h.viewport.scrollTop],[10,10]);
  pointer(h,'pointermove',{clientX:9990,clientY:9990});
  pointer(h,'pointerup',{clientX:9990,clientY:9990});
  assert.deepEqual([h.viewport.scrollLeft,h.viewport.scrollTop],[10,10]);
  assert.equal(h.image.style.width,width);assert.equal(h.label.textContent,label);
  assert.equal(h.image.attrs.src,src);assert.equal(h.captures.size,0);
  assert.notEqual(h.canvas.dataset.imageZoomPanning,'true');
});

test('historical short release resets Fit through five moves; a longer drag retains inspection',()=>{
  for(const moves of [0,1,5,6]) {
    const h=zoomed();pointer(h,'pointerdown');
    for(let n=1;n<=moves;n++)pointer(h,'pointermove',{clientX:100+n,clientY:100+n});
    pointer(h,'pointerup',{clientX:100+moves,clientY:100+moves});
    assert.equal(h.label.textContent,moves<=5?'40%':'200%');
    assert.deepEqual([h.viewport.scrollLeft,h.viewport.scrollTop],moves<=5?[0,0]:[594,494]);
    assert.equal(h.captures.size,0);
  }
});

test('drag refuses Fit, unavailable images, peers, nonprimary buttons, modifiers and invalid capture',()=>{
  const fitHost=host();pointer(fitHost,'pointerdown');assert.equal(fitHost.captures.size,0);
  const cold=host({complete:false});pointer(cold,'pointerdown');assert.equal(cold.captures.size,0);
  for(const fields of [{pointerType:'touch'},{pointerType:'pen'},{isPrimary:false},{button:2},{buttons:3},
    {ctrlKey:true},{metaKey:true},{altKey:true},{clientX:NaN},{clientY:Infinity},{pointerId:NaN},{pointerId:1.5}]) {
    const h=zoomed();assert.notEqual(pointer(h,'pointerdown',fields).prevented,true);assert.equal(h.captures.size,0);
    assert.deepEqual([h.viewport.scrollLeft,h.viewport.scrollTop],[600,500]);
  }
  const peer=zoomed();pointer(peer,'pointerdown',{},peer.canvas);assert.equal(peer.captures.size,0);
  for(const setup of [h=>{h.viewport.setPointerCapture=undefined;},h=>{h.viewport.setPointerCapture=()=>{throw Error('inactive');};},
    h=>{h.viewport.hasPointerCapture=()=>false;},h=>{h.image.complete=false;},h=>{h.canvas.dataset.imageZoomReady='false';}]) {
    const h=zoomed();setup(h);pointer(h,'pointerdown');assert.notEqual(h.canvas.dataset.imageZoomPanning,'true');
    assert.equal(h.label.textContent,'200%');assert.deepEqual([h.viewport.scrollLeft,h.viewport.scrollTop],[600,500]);
  }
});

test('wrong-pointer and peer callbacks cannot move, cancel or release the original gesture',()=>{
  const h=zoomed();pointer(h,'pointerdown');
  for(const kind of ['pointermove','pointerup','pointercancel','lostpointercapture']) {
    pointer(h,kind,{pointerId:8,clientX:10000});assert(h.captures.has(7));
    pointer(h,kind,{currentTarget:h.canvas,clientX:10000});assert(h.captures.has(7));
    pointer(h,kind,{clientX:10000},h.canvas);assert(h.captures.has(7));
  }
  assert.deepEqual([h.viewport.scrollLeft,h.viewport.scrollTop],[600,500]);
  pointer(h,'pointermove',{clientX:120,clientY:110});
  assert.deepEqual([h.viewport.scrollLeft,h.viewport.scrollTop],[580,490]);
  pointer(h,'pointercancel');assert.equal(h.captures.size,0);assert.equal(h.label.textContent,'200%');
});

test('controls, resize, errors and queued geometry changes cancel without borrowing readiness',()=>{
  for(const [index,change] of [h=>h.plus.fire('click'),h=>h.button.fire('click'),h=>h.zoom(.5),h=>h.resize(),
    h=>h.image.fire('error'),h=>{h.image.complete=false;},h=>{h.viewport.clientWidth=450;},
    h=>{h.image.naturalWidth=1001;},h=>{h.canvas.style.width='2001px';},h=>{h.canvas.dataset.imageZoomReady='false';}].entries()) {
    const h=zoomed();pointer(h,'pointerdown');const queued=h.viewport.listeners.get('pointermove');
    change(h);const before=[h.viewport.scrollLeft,h.viewport.scrollTop,h.label.textContent,h.image.style.width];
    queued({pointerId:7,pointerType:'mouse',isPrimary:true,currentTarget:h.viewport,target:h.viewport,
      clientX:500,clientY:500,button:-1,buttons:1,ctrlKey:false,metaKey:false,altKey:false,preventDefault(){},stopPropagation(){}});
    if(index===7) {before[2]='Fit';assert.equal(h.slider.disabled,true);assert(h.observers.every(o=>o.disconnected));}
    assert.deepEqual([h.viewport.scrollLeft,h.viewport.scrollTop,h.label.textContent,h.image.style.width],before);
    assert.equal(h.captures.size,0);assert.notEqual(h.canvas.dataset.imageZoomPanning,'true');
  }
});

test('modifier changes, invalid coordinates, capture loss and cancellation never trigger short-release Fit',()=>{
  for(const fields of [{ctrlKey:true},{metaKey:true},{altKey:true},{clientX:Infinity},{clientY:NaN},{buttons:0}]) {
    const h=zoomed();pointer(h,'pointerdown');pointer(h,'pointermove',fields);
    pointer(h,'pointerup');assert.equal(h.label.textContent,'200%');assert.equal(h.captures.size,0);
  }
  for(const kind of ['pointercancel','lostpointercapture']) {
    const h=zoomed();pointer(h,'pointerdown');pointer(h,'pointermove',{clientX:120,clientY:110});
    pointer(h,kind);pointer(h,'pointerup');assert.equal(h.label.textContent,'200%');
    assert.deepEqual([h.viewport.scrollLeft,h.viewport.scrollTop],[580,490]);assert.equal(h.captures.size,0);
  }
});

test('queued callbacks from an ended gesture cannot act on a new gesture on the same owner',()=>{
  const h=zoomed();pointer(h,'pointerdown');
  const old=['pointermove','pointerup','pointercancel','lostpointercapture'].map(k=>h.viewport.listeners.get(k));
  pointer(h,'pointercancel');pointer(h,'pointerdown');
  const before=[h.viewport.scrollLeft,h.viewport.scrollTop,h.label.textContent];
  for(const cb of old)cb({pointerId:7,currentTarget:h.viewport,target:h.viewport,clientX:10000,clientY:10000,
    pointerType:'mouse',isPrimary:true,button:0,buttons:1,ctrlKey:false,metaKey:false,altKey:false});
  assert(h.captures.has(7));assert.deepEqual([h.viewport.scrollLeft,h.viewport.scrollTop,h.label.textContent],before);
  pointer(h,'pointermove',{clientX:150,clientY:120});assert.deepEqual([h.viewport.scrollLeft,h.viewport.scrollTop],[550,480]);
  pointer(h,'pointercancel');
});

test('reentry and source/node retirement isolate gesture capture, queued peers and new controls',()=>{
  for(const kind of ['reentry','source','node','root']) {
    const h=zoomed();pointer(h,'pointerdown');
    const old=['pointerdown','pointermove','pointerup','pointercancel','lostpointercapture'].map(k=>h.viewport.listeners.get(k));
    const oldStop=h.root.__nativeImageZoom;
    if(kind==='reentry') {install('zoom-a');h.zoom(1);pointer(h,'pointerdown');}
    if(kind==='source') {h.image.attrs.src='https://unowned.invalid/image';h.mutate();}
    if(kind==='node') {h.selectors['[data-image-zoom-image]']=[{}];h.mutate();}
    if(kind==='root') {h.root.isConnected=false;h.mutate();}
    const before=[h.viewport.scrollLeft,h.viewport.scrollTop,h.label.textContent,h.slider.disabled];
    oldStop();
    for(const cb of old)cb({pointerId:7,currentTarget:h.viewport,target:h.image,clientX:10000,clientY:10000,
      pointerType:'mouse',isPrimary:true,button:0,buttons:1,ctrlKey:false,metaKey:false,altKey:false});
    assert.deepEqual([h.viewport.scrollLeft,h.viewport.scrollTop,h.label.textContent,h.slider.disabled],before);
    assert.equal(h.captures.has(7),kind==='reentry');
    if(kind==='reentry')pointer(h,'pointercancel');
  }
});

// Local wheel adapter: synthetic CSS boxes/events, no trusted native input.
function wheel(h,deltaY,{x=100,y=100,...fields}={},target=h.image) {
  const r=h.viewport.getBoundingClientRect();
  return h.viewport.fire('wheel',target,{ctrlKey:true,metaKey:false,altKey:false,shiftKey:false,
    deltaMode:0,deltaX:0,deltaY,cancelable:true,clientX:r.left+h.viewport.clientLeft+x,
    clientY:r.top+h.viewport.clientTop+y,...fields});
}
function naturalPoint(h,x,y) {
  const v=h.viewport.getBoundingClientRect(),r=h.image.getBoundingClientRect();
  return [(v.left+h.viewport.clientLeft+x-r.left)/r.width*h.image.naturalWidth,
    (v.top+h.viewport.clientTop+y-r.top)/r.height*h.image.naturalHeight];
}
function near(a,b) {assert(Math.abs(a-b)<1e-8,`${a} != ${b}`);}

test('Ctrl pixel wheel changes actual scale additively and anchors off-origin bordered overflow on both axes',()=>{
  const h=host({w:1000,h:800,vw:400,vh:320,left:170,top:230,borderLeft:4,borderTop:6});
  h.zoom(.25);assert.equal(h.label.textContent,'80%');h.viewport.scrollLeft=150;h.viewport.scrollTop=100;
  const src=h.image.attrs.src,point=naturalPoint(h,100,120),event=wheel(h,-20,{x:100,y:120});
  assert.equal(event.defaultPrevented,true);assert.equal(event.stopped,true);assert.equal(h.label.textContent,'100%');
  near(h.viewport.scrollLeft,212.5);near(h.viewport.scrollTop,155);
  naturalPoint(h,100,120).forEach((v,n)=>near(v,point[n]));
  wheel(h,10,{x:100,y:120});assert.equal(h.label.textContent,'90%');
  naturalPoint(h,100,120).forEach((v,n)=>near(v,point[n]));assert.equal(h.image.attrs.src,src);
  assert.equal(h.viewport.listenerOptions.get('wheel').passive,false);
});

test('wheel crosses letterbox and post-paint scrollbars while queued geometry waits for original resize',()=>{
  const h=host({w:1000,h:1000,vw:400,vh:200}),point=naturalPoint(h,200,100);
  wheel(h,-30,{x:200,y:100});assert.equal(h.label.textContent,'50%');near(h.viewport.scrollLeft,50);near(h.viewport.scrollTop,150);
  naturalPoint(h,200,100).forEach((v,n)=>near(v,point[n]));
  h.button.fire('click');wheel(h,-30,{x:150,y:100});assert.equal(h.viewport.scrollLeft,0);
  const bars=host({w:1000,h:800,vw:400,vh:320,barSize:15}),before=naturalPoint(bars,200,160);
  wheel(bars,-20,{x:200,y:160});assert.equal(bars.label.textContent,'60%');
  assert.equal(bars.viewport.clientWidth,385);assert.equal(bars.viewport.clientHeight,305);
  naturalPoint(bars,200,160).forEach((v,n)=>near(v,before[n]));
  const width=bars.image.style.width;assert.equal(wheel(bars,-10,{x:200,y:160}).defaultPrevented,false);assert.equal(bars.image.style.width,width);
  bars.resize();assert.equal(wheel(bars,-10,{x:200,y:160}).defaultPrevented,true);assert.equal(bars.label.textContent,'70%');
});

test('wheel consumes qualified endpoint no-ops and clamps huge deltas including retained scale below fit',()=>{
  const h=host({w:1000,h:1000,vw:400,vh:400});
  for(const d of [100,0]){assert.equal(wheel(h,d,{x:200,y:200}).defaultPrevented,true);assert.equal(h.label.textContent,'40%');}
  wheel(h,-Number.MAX_VALUE,{x:200,y:200});assert.equal(h.label.textContent,'200%');
  const pos=[h.viewport.scrollLeft,h.viewport.scrollTop];wheel(h,-10,{x:200,y:200});assert.deepEqual([h.viewport.scrollLeft,h.viewport.scrollTop],pos);
  wheel(h,Number.MAX_VALUE,{x:200,y:200});assert.equal(h.label.textContent,'40%');
  h.zoom(.1);assert.equal(h.label.textContent,'56%');h.viewport.clientWidth=1000;h.viewport.clientHeight=1000;h.resize();
  assert.equal(h.label.textContent,'56%');wheel(h,0,{x:500,y:500});assert.equal(h.label.textContent,'100%');assert.equal(h.slider.value,'0');
});

test('wheel refuses ordinary foreign unsupported and unready events without consuming scrolling or minting readiness',()=>{
  for(const fields of [{ctrlKey:false},{metaKey:true},{altKey:true},{deltaMode:1},{deltaMode:2},{cancelable:false},
    {deltaY:NaN},{deltaY:Infinity},{clientX:NaN},{clientY:Infinity},{x:-1},{x:500},{y:-1},{y:400},{currentTarget:{}}]) {
    const h=zoomed(),before=[h.label.textContent,h.viewport.scrollLeft,h.viewport.scrollTop];
    assert.equal(wheel(h,-10,fields).defaultPrevented,false);assert.deepEqual([h.label.textContent,h.viewport.scrollLeft,h.viewport.scrollTop],before);
  }
  for(const target of ['canvas','slider','viewport']){const h=zoomed();assert.equal(wheel(h,-10,{},h[target]).defaultPrevented,false);assert.equal(h.label.textContent,'200%');}
  const box=host({w:1000,h:1000,vw:400,vh:200});assert.equal(wheel(box,-10,{x:50,y:100}).defaultPrevented,false);
  for(const change of [h=>{h.viewport.clientWidth=450;},h=>{h.canvas.style.width='2001px';},h=>{h.image.complete=false;},
    h=>{h.canvas.dataset.imageZoomReady='false';},h=>h.image.fire('error'),h=>{h.image.computedStyle={transform:'scale(2)'};},h=>{h.viewport.computedStyle={direction:'rtl'};}]) {
    const h=zoomed();change(h);const before=[h.label.textContent,h.image.style.width,h.viewport.scrollLeft,h.viewport.scrollTop];
    assert.equal(wheel(h,-10).defaultPrevented,false);assert.deepEqual([h.label.textContent,h.image.style.width,h.viewport.scrollLeft,h.viewport.scrollTop],before);
  }
  const denied=zoomed(),width=denied.image.style.width;
  assert.equal(wheel(denied,20,{preventDefault(){this.prevented=true;}}).defaultPrevented,false);assert.equal(denied.image.style.width,width);
});

test('wheel cancels drag without short-release Fit and isolates queued callbacks, reentry, retired sources and resources',()=>{
  const h=zoomed(),src=h.image.attrs.src;pointer(h,'pointerdown');
  const queued=['pointermove','pointerup','pointercancel','lostpointercapture'].map(k=>h.viewport.listeners.get(k));
  wheel(h,25);assert.equal(h.label.textContent,'175%');assert.equal(h.captures.size,0);
  const before=[h.label.textContent,h.viewport.scrollLeft,h.viewport.scrollTop];
  for(const cb of queued)cb({pointerId:7,currentTarget:h.viewport,target:h.viewport,clientX:10000,clientY:10000});
  assert.deepEqual([h.label.textContent,h.viewport.scrollLeft,h.viewport.scrollTop],before);assert.equal(h.image.attrs.src,src);
  const old=h.viewport.listeners.get('wheel'),options=h.viewport.listenerOptions.get('wheel');install('zoom-a');
  assert(h.viewport.removed.some(r=>r.type==='wheel'&&r.listener===old&&r.options===options));
  const event={target:h.image,currentTarget:h.viewport,ctrlKey:true,metaKey:false,altKey:false,deltaMode:0,deltaY:-20,cancelable:true,
    clientX:200,clientY:200,defaultPrevented:false,preventDefault(){this.defaultPrevented=true;},stopPropagation(){this.stopped=true;}};
  old(event);assert.equal(event.defaultPrevented,false);assert.equal(h.label.textContent,'40%');
  assert.equal(wheel(h,-10,{x:200,y:160}).defaultPrevented,true);assert.equal(h.label.textContent,'50%');
  for(const kind of ['source','node','root']) {
    const other=zoomed(),saved=other.viewport.listeners.get('wheel');
    if(kind==='source')other.image.attrs.src='https://unowned.invalid/image';
    if(kind==='node')other.selectors['[data-image-zoom-image]']=[{}];if(kind==='root')other.root.isConnected=false;
    other.mutate();const width=other.image.style.width,stale={...event,target:other.image,currentTarget:other.viewport,defaultPrevented:false};
    saved(stale);assert.equal(stale.defaultPrevented,false);assert.equal(other.image.style.width,width);assert.equal(other.slider.disabled,true);
  }
});

// Captureless click is separate; the original Fit never starts a drag.
function clickPointer(h,type,{x=h.viewport.clientWidth/2,y=h.viewport.clientHeight/2,...fields}={}) {
  const r=h.viewport.getBoundingClientRect();
  return pointer(h,type,{clientX:r.left+h.viewport.clientLeft+x,clientY:r.top+h.viewport.clientTop+y,
    cancelable:true,...fields},h.image);
}
test('captureless short Fit click uses the Source0.9 threshold and anchors release, including small-image return',()=>{
  for(const [extent,target]of [[400,'100%'],[890,'100%'],[900,'200%'],[950,'200%']]) {
    const h=host({w:1000,h:1000,vw:extent,vh:extent}),point=naturalPoint(h,extent/2,extent/2);
    assert.equal(h.canvas.dataset.imageZoomClickReady,'in');clickPointer(h,'pointerdown');assert.equal(h.captures.size,0);
    clickPointer(h,'pointerup');assert.equal(h.label.textContent,target);assert.equal(h.captures.size,0);
    naturalPoint(h,extent/2,extent/2).forEach((v,n)=>near(v,point[n]));
  }
  for(const moves of [0,1,5,6]) {
    const h=host({w:1000,h:800,vw:400,vh:320});clickPointer(h,'pointerdown');
    for(let n=1;n<=moves;n++)clickPointer(h,'pointermove',{x:200+n,y:160+n});
    const point=naturalPoint(h,200+moves,160+moves);
    clickPointer(h,'pointerup',{x:200+moves,y:160+moves});assert.equal(h.label.textContent,moves<=5?'100%':'40%');
    naturalPoint(h,200+moves,160+moves).forEach((v,n)=>near(v,point[n]));assert.equal(h.captures.size,0);
  }
  const small=host({w:10,h:20});clickPointer(small,'pointerdown');clickPointer(small,'pointerup');
  assert.equal(small.label.textContent,'200%');assert.equal(small.canvas.dataset.imageZoomClickReady,'out');assert.equal(small.captures.size,0);
  clickPointer(small,'pointerdown');clickPointer(small,'pointerup');assert.equal(small.label.textContent,'100%');assert.equal(small.captures.size,0);
});

test('captureless click cancels on leave controls error resize modifiers and wheel without reviving readiness',()=>{
  for(const change of [h=>h.viewport.fire('pointerleave',h.viewport,{pointerId:7}),
    h=>clickPointer(h,'pointercancel'),h=>clickPointer(h,'pointermove',{ctrlKey:true}),
    h=>clickPointer(h,'pointermove',{x:-1}),h=>h.plus.fire('click'),h=>h.resize(),h=>h.image.fire('error'),
    h=>wheel(h,-10,{x:200,y:160}),h=>{h.viewport.clientWidth=450;}]) {
    const h=host({w:1000,h:800,vw:400,vh:320});clickPointer(h,'pointerdown');
    change(h);const before=[h.label.textContent,h.image.style.width,h.viewport.scrollLeft,h.viewport.scrollTop,h.slider.disabled];
    clickPointer(h,'pointerup');assert.deepEqual([h.label.textContent,h.image.style.width,h.viewport.scrollLeft,h.viewport.scrollTop,h.slider.disabled],before);
    assert.equal(h.captures.size,0);
  }
  const h=host({w:1000,h:800,vw:400,vh:320});clickPointer(h,'pointerdown');
  clickPointer(h,'pointerup',{pointerId:8});clickPointer(h,'pointerup',{currentTarget:h.canvas});
  assert.equal(h.label.textContent,'40%');clickPointer(h,'pointerup');assert.equal(h.label.textContent,'100%');
  const cold=host({complete:false});clickPointer(cold,'pointerdown');clickPointer(cold,'pointerup');
  assert.equal(cold.slider.disabled,true);assert.equal(cold.captures.size,0);assert.equal(cold.image.style.width,undefined);
});

test('retired captureless callbacks cannot consume or cancel a reentered receipt or borrow a replaced source',()=>{
  for(const transform of ['matrix(2,0,0,2,0,0)','matrix(0,1,-1,0,0,0)','matrix(-1,0,0,1,0,0)',
    'matrix(1,.1,0,1,0,0)','matrix(1,0,0,1,Infinity,0)','matrix(1,0,0,1,,0)','matrix3d(1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1)']) {
    const changed=host({w:1000,h:800,vw:400,vh:320});changed.modal.computedStyle={transform};
    clickPointer(changed,'pointerdown');clickPointer(changed,'pointerup');
    assert.equal(changed.label.textContent,'40%');assert.equal(wheel(changed,-10,{x:200,y:160}).defaultPrevented,false);assert.equal(changed.captures.size,0);
  }
  const recovery=host({w:1000,h:800,vw:400,vh:320});clickPointer(recovery,'pointerdown');
  for(let n=0;n<6;n++)clickPointer(recovery,'pointermove');
  const missedUp=recovery.viewport.listeners.get('pointerup');
  clickPointer(recovery,'pointerdown');
  missedUp({currentTarget:recovery.viewport,target:recovery.image,pointerId:7,
    preventDefault(){throw Error('retired missing-up receipt consumed');}});
  assert.equal(recovery.label.textContent,'40%');clickPointer(recovery,'pointerup');assert.equal(recovery.label.textContent,'100%');
  recovery.button.fire('click');clickPointer(recovery,'pointerdown');
  const r=recovery.viewport.getBoundingClientRect();
  pointer(recovery,'pointerup',{clientX:r.left+recovery.viewport.clientLeft+200,
    clientY:r.top+recovery.viewport.clientTop+160,cancelable:true},recovery.canvas);
  clickPointer(recovery,'pointerup');assert.equal(recovery.label.textContent,'40%');
  clickPointer(recovery,'pointerdown');clickPointer(recovery,'pointerup');assert.equal(recovery.label.textContent,'100%');
  const h=host({w:1000,h:800,vw:400,vh:320});clickPointer(h,'pointerdown');
  const callbacks=['pointermove','pointerup','pointercancel','pointerleave'].map(k=>h.viewport.listeners.get(k));
  const oldDown=h.viewport.listeners.get('pointerdown'),oldStop=h.root.__nativeImageZoom,src=h.image.attrs.src;
  install('zoom-a');clickPointer(h,'pointerdown');
  const event={target:h.image,currentTarget:h.viewport,pointerId:7,pointerType:'mouse',isPrimary:true,button:0,buttons:0,
    clientX:262,clientY:243,ctrlKey:false,metaKey:false,altKey:false,cancelable:true,
    preventDefault(){throw Error('retired event consumed');},stopPropagation(){throw Error('retired event stopped');}};
  oldDown(event);oldStop();for(const cb of callbacks)cb(event);
  assert.equal(h.label.textContent,'40%');assert.equal(h.slider.disabled,false);assert.equal(h.captures.size,0);
  clickPointer(h,'pointerup');assert.equal(h.label.textContent,'100%');assert.equal(h.image.attrs.src,src);
  for(const kind of ['source','node','root']) {
    const other=host({w:1000,h:800,vw:400,vh:320});clickPointer(other,'pointerdown');const up=other.viewport.listeners.get('pointerup');
    if(kind==='source')other.image.attrs.src='https://unowned.invalid/image';
    if(kind==='node')other.selectors['[data-image-zoom-image]']=[{}];if(kind==='root')other.root.isConnected=false;
    other.mutate();const width=other.image.style.width;up({...event,target:other.image,currentTarget:other.viewport});
    assert.equal(other.image.style.width,width);assert.equal(other.slider.disabled,true);assert.equal(other.captures.size,0);
  }
});
