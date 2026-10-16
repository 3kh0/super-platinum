import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';

const source=readFileSync(new URL('../src/desktop/view/toast_position.js',import.meta.url),'utf8');
const geometry=vm.createContext({document:{getElementById:()=>null}});
vm.runInContext(source,geometry);
const rect=(top,height=100,left=0,width=600)=>({top,bottom:top+height,height,left,right:left+width,width});
const place=(height,obstacles,minimum=44)=>JSON.parse(JSON.stringify(geometry.toastPlacement({width:668,height},obstacles,18,8,minimum)));
const focusPlace=(focus,base={bottom:18,maxHeight:344},height=60)=>JSON.parse(JSON.stringify(
  geometry.toastFocusedPlacement(base,{width:668,height:380},{width:420,height},focus,18,8,44)));

test('normal and narrow notices clear the higher composer with bounded height',()=>{
  assert.deepEqual(place(800,[rect(680),rect(600)]),{bottom:208,maxHeight:574});
  assert.deepEqual(place(380,[rect(280),rect(160)]),{bottom:228,maxHeight:134});
  assert.deepEqual(place(380,[rect(280),rect(90,290)]),{bottom:298,maxHeight:64});
});
test('completion controls also reserve clearance without a fixed top offset',()=>{
  assert.deepEqual(place(380,[rect(250),rect(120,130)]),{bottom:268,maxHeight:94});
});
test('hidden zero-size and offscreen panes do not consume notice space',()=>{
  assert.deepEqual(place(380,[rect(20,0),rect(20,100,700),rect(400),rect(-100,50)]),{bottom:18,maxHeight:344});
  assert.equal(geometry.toastPlacement({width:0,height:0},[],18,8),null);
});
test('insufficient room does not expose clipped padding or dismiss chrome',()=>{
  assert.equal(place(380,[rect(0,380)]),null);
  assert.equal(place(380,[rect(69,311)]),null); // 43px available, 44px chrome
  assert.deepEqual(place(380,[rect(70,310)]),{bottom:318,maxHeight:44});
  assert.equal(place(380,[rect(90,290)],65),null); // actual larger text/control size
});
test('notice protects overlapping focused actions without reacting to unrelated sidebar focus',()=>{
  assert.deepEqual(focusPlace(rect(320,32,300,150)),{bottom:68,maxHeight:294});
  assert.deepEqual(focusPlace(rect(0,32,0,100)),{bottom:18,maxHeight:344});
  assert.deepEqual(focusPlace(rect(20,32,300,150)),{bottom:18,maxHeight:344});
});
test('focused action can leave safe space below or temporarily require hiding',()=>{
  const high={bottom:220,maxHeight:142};
  assert.deepEqual(focusPlace(rect(40,30,300,100),high,142),{bottom:220,maxHeight:82});
  assert.equal(focusPlace(rect(40,100,300,100),high,142),null);
});

function mounted() {
  const callbacks={}, queued=new Map(), style=new Map(), observed=new Set();
  let next=0,reads=0,disconnected=0;
  const composer={top:220,getBoundingClientRect:()=>{reads++;return rect(composer.top,380-composer.top);}};
  const dismiss={height:24,getBoundingClientRect(){return {height:this.height};}};
  const toast={isConnected:true,dataset:{},scrollHeight:58,contains:node=>node===dismiss,
    getBoundingClientRect:()=>rect(0,60,230,420),querySelector:()=>dismiss,
    style:{getPropertyValue:k=>style.get(k),setProperty:(k,v)=>style.set(k,v)}};
  const events=new Map();
  const context={innerWidth:668,innerHeight:380,
    getComputedStyle:()=>({right:'18px',paddingTop:'9px',paddingBottom:'9px',borderTopWidth:'1px',borderBottomWidth:'1px',lineHeight:'20px',getPropertyValue:()=> '8px'}),
    requestAnimationFrame:fn=>{queued.set(++next,fn);return next;},cancelAnimationFrame:id=>queued.delete(id),
    ResizeObserver:class {constructor(fn){callbacks.resize=fn;}observe(node){observed.add(node);}unobserve(node){observed.delete(node);}disconnect(){observed.clear();disconnected++;}},
    MutationObserver:class {constructor(fn){callbacks.mutation=fn;}observe(){}disconnect(){disconnected++;}},
    document:{hidden:false,body:{},getElementById:()=>toast,querySelectorAll:()=>[composer],addEventListener:(key,fn)=>events.set(key,fn),removeEventListener:key=>events.delete(key)},
    window:{addEventListener:(key,fn)=>events.set(key,fn),removeEventListener:key=>events.delete(key)},
  };
  vm.runInNewContext(source,context);
  return {toast,composer,dismiss,callbacks,queued,events,observed,context,reads:()=>reads,disconnected:()=>disconnected,
    flush:()=>{const work=[...queued.values()];queued.clear();for(const fn of work)fn();}};
}
test('resize work coalesces and unrelated message mutations do not read layout',()=>{
  const m=mounted();assert.equal(m.reads(),1);assert.equal(m.toast.dataset.positioned,'true');
  m.callbacks.mutation([{type:'childList',addedNodes:[{nodeType:1,matches:()=>false,querySelector:()=>null}],removedNodes:[]}]);
  assert.equal(m.queued.size,0);assert.equal(m.reads(),1);
  m.callbacks.resize();m.callbacks.resize();m.events.get('resize')();
  assert.equal(m.queued.size,1);m.flush();assert.equal(m.reads(),2);
  m.context.document.hidden=true;m.callbacks.resize();m.flush();assert.equal(m.reads(),2);
});
test('unmount disconnects observers, cancels scheduled work and releases listeners',()=>{
  const m=mounted();m.callbacks.resize();m.toast.isConnected=false;m.callbacks.mutation([]);
  assert.equal(m.disconnected(),2);assert.equal(m.queued.size,0);assert.equal(m.events.size,0);
  assert.equal(m.observed.size,0);assert.equal(m.toast.__disposePosition,undefined);
});
test('no safe space hides current notice and resize restores it with actual chrome size',()=>{
  const m=mounted();
  m.composer.top=69;m.callbacks.resize();m.flush();assert.equal(m.toast.dataset.positioned,undefined);
  m.composer.top=70;m.callbacks.resize();m.flush();assert.equal(m.toast.dataset.positioned,'true');
  m.dismiss.height=40;m.callbacks.resize();m.flush();assert.equal(m.toast.dataset.positioned,undefined);
  m.composer.top=90;m.callbacks.resize();m.flush();assert.equal(m.toast.dataset.positioned,'true');
});
test('hidden document preserves observers and reveals on visibility change',()=>{
  const m=mounted();
  m.context.document.hidden=true;m.events.get('visibilitychange')();m.flush();
  assert.equal(m.toast.dataset.positioned,undefined);assert.equal(m.disconnected(),0);
  m.context.document.hidden=false;m.events.get('visibilitychange')();m.flush();
  assert.equal(m.toast.dataset.positioned,'true');
});
test('composer replacement rescans observations and repositions against the successor',()=>{
  const m=mounted();
  const successor={nodeType:1,matches:()=>true,getBoundingClientRect:()=>rect(100)};
  m.context.document.querySelectorAll=()=>[successor];
  m.callbacks.mutation([{type:'childList',addedNodes:[successor],removedNodes:[]}]);
  assert.equal(m.queued.size,1);m.flush();
  assert.equal(m.observed.has(m.composer),false);assert.equal(m.observed.has(successor),true);
  assert.equal(m.toast.style.getPropertyValue('--toast-bottom'),'288px');
});

test('an unfocused modal protects its footer and removal restores composer placement',()=>{
  const m=mounted();
  const dialog={nodeType:1,top:100,modal:true,
    matches:selector=>dialog.modal&&selector.includes('[role="dialog"][aria-modal="true"]'),
    getBoundingClientRect:()=>rect(dialog.top,180,114,440)};
  m.context.document.querySelectorAll=selector=>[m.composer,...(dialog.matches(selector)?[dialog]:[])];
  m.callbacks.mutation([{type:'childList',addedNodes:[dialog],removedNodes:[]}]);m.flush();
  assert.equal(m.observed.has(dialog),true);
  assert.equal(m.toast.style.getPropertyValue('--toast-bottom'),'288px');
  assert.equal(m.toast.dataset.positioned,'true');
  // A near-full-height modal leaves no safe notice space. Keep it hidden until
  // the modal closes rather than covering its actions or clipping dismissal.
  dialog.top=20;m.callbacks.resize();m.flush();
  assert.equal(m.toast.dataset.positioned,undefined);
  dialog.modal=false;
  m.callbacks.mutation([{type:'attributes',target:dialog}]);m.flush();
  assert.equal(m.observed.has(dialog),false);
  assert.equal(m.toast.style.getPropertyValue('--toast-bottom'),'168px');
  assert.equal(m.toast.dataset.positioned,'true');
});
test('replacing the notice cancels predecessor work without tearing down the successor',()=>{
  const m=mounted(), oldMutation=m.callbacks.mutation;
  m.callbacks.resize();m.toast.isConnected=false;oldMutation([]);
  const successor={...m.toast,isConnected:true,dataset:{}};
  m.context.document.getElementById=()=>successor;
  vm.runInNewContext(source,m.context);
  assert.equal(successor.dataset.positioned,'true');assert.equal(m.events.size,5);
  oldMutation([]); // a late predecessor observer cannot release successor listeners
  assert.equal(m.events.size,5);assert.equal(m.observed.has(successor),true);
  m.callbacks.resize();m.flush();assert.equal(successor.dataset.positioned,'true');
});
test('focus and relevant scroll coalesce; own dismiss and unrelated scroll remain safe',()=>{
  const m=mounted();
  const button={isConnected:true,top:190,matches:()=>true,getBoundingClientRect(){return rect(this.top,30,300,100);}};
  m.context.document.activeElement=button;
  m.events.get('focusin')();m.events.get('focusout')();assert.equal(m.queued.size,1);m.flush();
  assert.equal(m.observed.has(button),true);
  assert.equal(m.toast.style.getPropertyValue('--toast-bottom'),'198px');
  m.events.get('scroll')({target:{contains:()=>false}});assert.equal(m.queued.size,0);
  button.top=110;
  m.events.get('scroll')({target:{contains:node=>node===button}});m.flush();
  assert.equal(m.toast.style.getPropertyValue('--toast-bottom'),'168px');
  m.context.document.activeElement=m.dismiss;m.events.get('focusin')();m.flush();
  assert.equal(m.observed.has(button),false);assert.equal(m.toast.dataset.positioned,'true');
  m.toast.isConnected=false;m.callbacks.mutation([]);assert.equal(m.events.size,0);
});

test('short viewer near top uses the safe viewer-to-composer band at both widths',()=>{
  assert.deepEqual(place(780,[rect(62.4,164),rect(700,80)]),{bottom:88,maxHeight:457});
  assert.deepEqual(place(380,[rect(30.4,164),rect(280,100)]),{bottom:108,maxHeight:69});
  // Order, overlap and contained completion surfaces do not invent a gap.
  assert.deepEqual(place(380,[rect(280,100),rect(30.4,164),rect(50,100)]),{bottom:108,maxHeight:69});
  assert.equal(place(380,[rect(30.4,164),rect(200,180)]),null);
});
test('fallback focus adjustment cannot cross the protected upper boundary',()=>{
  const band={bottom:100,maxHeight:60}; // 220..280, protected dialog ends at212
  assert.equal(focusPlace(rect(230,40,300,100),band,60),null);
  assert.equal(focusPlace(rect(265,10,300,100),band,60),null);
  assert.deepEqual(focusPlace(rect(272,10,300,100),band,60),{bottom:116,maxHeight:44});
});
test('fallback band keeps dismiss chrome and fractional boundaries inside the gap',()=>{
  const obstacles=[rect(20,160.5),rect(240.25,139.75)];
  assert.equal(place(380,obstacles),null);
  assert.deepEqual(place(380,obstacles,43),{bottom:148,maxHeight:43});
});
test('mounted viewer fallback reveals notice without new idle work and restores on removal',()=>{
  const m=mounted();m.composer.top=280;
  const dialog={nodeType:1,matches:()=>true,getBoundingClientRect:()=>rect(30.4,164,114,440)};
  m.context.document.querySelectorAll=()=>[m.composer,dialog];
  m.callbacks.mutation([{type:'childList',addedNodes:[dialog],removedNodes:[]}]);m.flush();
  assert.equal(m.toast.dataset.positioned,'true');assert.equal(m.toast.style.getPropertyValue('--toast-bottom'),'108px');
  assert.equal(m.toast.style.getPropertyValue('--toast-max-height'),'69px');assert.equal(m.queued.size,0);
  const reads=m.reads();m.callbacks.mutation([{type:'childList',addedNodes:[{nodeType:1,matches:()=>false,querySelector:()=>null}],removedNodes:[]}]);
  assert.equal(m.reads(),reads);assert.equal(m.queued.size,0);
  m.context.document.querySelectorAll=()=>[m.composer];
  m.callbacks.mutation([{type:'childList',addedNodes:[],removedNodes:[dialog]}]);m.flush();
  assert.equal(m.toast.style.getPropertyValue('--toast-bottom'),'108px');
  assert.equal(m.toast.style.getPropertyValue('--toast-max-height'),'254px');
});

test('unfocused attachment menu remains clear while the same toast stays visible',()=>{
  const m=mounted();m.composer.top=264;
  // Native3 narrow thread-menu measured bounds, not the focused Save control.
  const menu={nodeType:1,top:115.83333587646484,
    matches:selector=>selector.split(',').some(part=>part.trim()==='.file-action-panel'),
    querySelector:()=>null,
    getBoundingClientRect(){return rect(this.top,124,356.66668701171875,285.3333435058594);}};
  let present=true;
  m.context.document.querySelectorAll=selector=>[m.composer,...(present&&menu.matches(selector)?[menu]:[])];
  m.callbacks.mutation([{type:'childList',addedNodes:[menu],removedNodes:[]}]);
  assert.equal(m.queued.size,1);m.flush();
  assert.equal(m.observed.has(menu),true);
  assert.equal(m.context.document.activeElement,undefined);
  assert.equal(m.toast.dataset.positioned,'true');
  assert.equal(m.toast.style.getPropertyValue('--toast-bottom'),'273px');
  assert.equal(m.toast.style.getPropertyValue('--toast-max-height'),'89px');
  const bottom=380-parseFloat(m.toast.style.getPropertyValue('--toast-bottom'));
  assert.ok(bottom+8<=menu.top,'entire panel, including nonfocused Delete, has clearance');
  assert.equal(m.queued.size,0);
  menu.top=100;m.callbacks.resize();m.callbacks.resize();
  assert.equal(m.queued.size,1);m.flush();
  assert.equal(m.toast.style.getPropertyValue('--toast-bottom'),'288px');
  assert.equal(m.toast.dataset.positioned,'true');
  present=false;
  m.callbacks.mutation([{type:'childList',addedNodes:[],removedNodes:[menu]}]);m.flush();
  assert.equal(m.observed.has(menu),false);
  assert.equal(m.toast.style.getPropertyValue('--toast-bottom'),'124px');
  assert.equal(m.toast.dataset.positioned,'true');
});

test('file menu ancestor scrolling repositions the toast and teardown releases observation',()=>{
  const m=mounted();m.composer.top=264;
  const menu={nodeType:1,top:115.83333587646484,
    matches:selector=>selector.split(',').some(part=>part.trim()==='.file-action-panel'),
    querySelector:()=>null,
    getBoundingClientRect(){return rect(this.top,124,356.66668701171875,285.3333435058594);}};
  m.context.document.querySelectorAll=selector=>[m.composer,...(menu.matches(selector)?[menu]:[])];
  m.callbacks.mutation([{type:'childList',addedNodes:[menu],removedNodes:[]}]);m.flush();
  menu.top=85;
  m.events.get('scroll')({target:{contains:node=>node===menu}});
  assert.equal(m.queued.size,1);m.flush();
  assert.equal(m.toast.style.getPropertyValue('--toast-bottom'),'303px');
  assert.equal(m.toast.style.getPropertyValue('--toast-max-height'),'59px');
  assert.equal(m.toast.dataset.positioned,'true');
  m.toast.isConnected=false;m.callbacks.mutation([]);
  assert.equal(m.observed.size,0);assert.equal(m.events.size,0);
  assert.equal(m.queued.size,0);assert.equal(m.disconnected(),2);
});
