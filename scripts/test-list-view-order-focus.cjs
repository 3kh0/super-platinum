// Actual production return-focus source with deterministic DOM/frame scheduling.
// No app, browser, transport or native input is started by these tests.
const test=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const vm=require('node:vm');
const source=fs.readFileSync(path.join(process.argv[2]||process.cwd(),'src/desktop/view/slack_lists/rename_restore.js'),'utf8');

function setup() {
  const listeners=new Map(),frames=new Map(),timers=new Map(),observers=[];
  let serial=0;
  const document={body:{isConnected:true},activeElement:null,
    addEventListener(type,fn){if(!listeners.has(type))listeners.set(type,new Set());listeners.get(type).add(fn)},
    removeEventListener(type,fn){listeners.get(type)?.delete(fn)},
    querySelector(selector){return selector==='.slack-list-dialog'?document.dialog:null},
    getElementById(id){return id==='slack-list-rename'?document.panel:null},
    querySelectorAll(){return document.dialogs},
  };
  const element=(tag,props={})=>Object.assign({tagName:tag,isConnected:true,disabled:false,dataset:{},
    getBoundingClientRect(){return {width:100,height:24}},
    closest(selector){return selector==='.slack-list-dialog'?document.dialog:null},
    matches(){return ['BUTTON','INPUT','SELECT','TEXTAREA','A'].includes(this.tagName)},
    contains(e){return e===this||e?.parent===this},
    focus(){document.activeElement=this;emit('focusin',{target:this})},
  },props);
  const dialog=element('SECTION',{dataset:{dialogGeneration:'visit1'}});
  const root=element('SECTION',{dataset:{renameSerial:'receipt7',renameColumn:'SECOND'}});
  const done=element('BUTTON',{parent:root}),first=element('BUTTON',{dataset:{renameColumn:'FIRST'}});
  const target=element('BUTTON',{dataset:{renameColumn:'SECOND'}}),close=element('BUTTON');
  const other=element('BUTTON'),grid=element('TD');
  const headers=[first,target];
  dialog.querySelectorAll=()=>headers;
  dialog.querySelector=selector=>selector==='#slack-list-close'?close:null;
  document.dialog=dialog;document.panel=root;document.dialogs=[dialog,root];document.activeElement=done;
  function emit(type,event){for(const fn of [...(listeners.get(type)||[])])fn(event)}
  class Observer{constructor(cb){this.cb=cb;this.connected=false;observers.push(this)}observe(){this.connected=true}disconnect(){this.connected=false}}
  const context=vm.createContext({document,MutationObserver:Observer,
    requestAnimationFrame(fn){const id=++serial;frames.set(id,fn);return id},cancelAnimationFrame(id){frames.delete(id)},
    setTimeout(fn){const id=++serial;timers.set(id,fn);return id},clearTimeout(id){timers.delete(id)},
  });
  vm.runInContext(source,context);
  const mutate=()=>{for(const o of observers)if(o.connected)o.cb([])};
  const frame=()=>{const pending=[...frames.values()];frames.clear();for(const fn of pending)fn()};
  const unmount=()=>{root.isConnected=false;done.isConnected=false;document.panel=null;document.dialogs=[dialog];document.activeElement=document.body;mutate()};
  return {context,document,dialog,root,done,first,target,close,other,grid,headers,frames,timers,observers,
    arm:()=>context.armListRenameReturn(root),mutate,frame,unmount,emit,
    expire:()=>{for(const fn of [...timers.values()])fn()},
    assertStopped(){assert.equal(timers.size,0);assert.equal(frames.size,0);assert(observers.every(o=>!o.connected));assert([...listeners.values()].every(set=>set.size===0))},
  };
}


const focusSource=fs.readFileSync(path.join(process.argv[2]||process.cwd(),'src/desktop/view/slack_lists/rename_focus.js'),'utf8');
const openerSource=fs.readFileSync(path.join(process.argv[2]||process.cwd(),'src/desktop/view/slack_lists/rename/view_order_opener.js'),'utf8');
function order(){
  const h=setup();h.dialog.dataset.viewSelection='8';
  h.root.dataset.renameView='V';h.root.dataset.renameColumn='';h.root.dataset.reorderDirection='up';
  h.target.dataset={reorderView:'V',reorderDirection:'up',reorderSelection:'8'};h.target.parent=h.dialog;
  h.picker={value:'V'};
  h.dialog.querySelector=s=>s==='#slack-list-view-move-up'?h.target:s==='#slack-list-view-select'?h.picker:s==='#slack-list-close'?h.close:null;
  h.dialog.addEventListener=(type,fn)=>{h.capture=fn};
  h.root.addEventListener=()=>{};
  h.root.querySelector=s=>s==='#slack-list-rename-close:not([disabled])'?h.done:null;
  vm.runInContext(openerSource,h.context);
  h.capture({target:{closest:()=>h.target}});
  h.document.activeElement=h.document.body;
  vm.runInContext(focusSource,h.context);h.frame();
  return h;
}
test('actual native capture precedes focus and initial focus is Cancel',()=>{
  const h=order();assert.equal(h.root.__reorderOpener,h.target);assert.equal(h.document.activeElement,h.done);
});
test('Cancel restores exact selected-view move opener',()=>{
  const h=order();h.arm();h.unmount();h.frame();assert.equal(h.document.activeElement,h.target);h.assertStopped();
});
test('successful boundary move falls back to List Close when exact opener is disabled',()=>{
  const h=order();h.arm();h.target.disabled=true;h.unmount();h.frame();assert.equal(h.document.activeElement,h.close);h.assertStopped();
});
test('same-ID replacement cannot inherit return',()=>{
  const h=order();h.arm();h.dialog.querySelector=s=>s==='#slack-list-view-move-up'?h.first:s==='#slack-list-view-select'?h.picker:null;h.unmount();h.frame();assert.equal(h.document.activeElement,h.document.body);h.assertStopped();
});
test('selection ABA, changed selected ID and replaced visit each retire return',()=>{
  for(const which of ['selection','id','visit']){const h=order();h.arm();if(which==='selection')h.dialog.dataset.viewSelection='9';else if(which==='id')h.picker.value='OTHER';else h.dialog.dataset.dialogGeneration='new';h.unmount();h.frame();assert.equal(h.document.activeElement,h.document.body);h.assertStopped();}
});
test('missing opener proof refuses restoration',()=>{
  const h=order();delete h.root.__reorderOpener;h.arm();assert.equal(h.frames.size,0);assert.equal(h.timers.size,0);
});
test('new trusted input or chosen interactive control takes precedence',()=>{
  for(const kind of ['pointerdown','keydown','focus']){const h=order();h.arm();if(kind==='focus')h.other.focus();else h.emit(kind,{isTrusted:true});h.unmount();h.frame();assert.equal(h.document.activeElement,h.document.body);h.assertStopped();}
});
test('captured action must match selection, view, direction and visit at mount',()=>{
  for(const key of ['selection','view','direction','visit']){const h=order();delete h.root.__reorderOpener;h.capture({target:{closest:()=>h.target}});h.dialog.__viewOrderOpener[key]='different';vm.runInContext(focusSource,h.context);h.frame();assert.equal(h.root.__reorderOpener,undefined);}
});
test('disabled or wrong-view native click cannot replace captured proof',()=>{
  for(const which of ['disabled','view','selection']){const h=order();delete h.dialog.__viewOrderOpener;if(which==='disabled')h.target.disabled=true;else if(which==='view')h.picker.value='OTHER';else h.target.dataset.reorderSelection='stale';h.capture({target:{closest:()=>h.target}});assert.equal(h.dialog.__viewOrderOpener,undefined);}
});

test('native opener proof is consumed by its first mounted modal',()=>{
  const h=order();assert.equal(h.dialog.__viewOrderOpener,undefined);delete h.root.__reorderOpener;vm.runInContext(focusSource,h.context);h.frame();assert.equal(h.root.__reorderOpener,undefined);
});
test('Saving falls back to Close list and Checking recovers to the newly enabled Check action',()=>{
  const h=order();h.done.disabled=true;h.close.parent=h.root;
  h.root.querySelector=s=>s==='#slack-list-rename-leave'?h.close:null;
  h.mutate();h.frame();assert.equal(h.document.activeElement,h.close);
  h.close.disabled=true;h.first.parent=h.root;
  h.root.querySelector=s=>s==='#slack-list-rename-check:not([disabled])'?h.first:null;
  h.mutate();h.frame();assert.equal(h.document.activeElement,h.first);
});
test('replaced serial cannot focus or consume another receipt action',()=>{
  const h=order();h.document.activeElement=h.document.body;h.root.dataset.renameSerial='new';
  h.mutate();h.frame();assert.equal(h.document.activeElement,h.document.body);
});
