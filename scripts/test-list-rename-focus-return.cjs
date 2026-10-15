// Actual production return-focus source with deterministic DOM/frame scheduling.
// No app, browser, transport or native input is started by these tests.
const test=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const vm=require('node:vm');
const source=fs.readFileSync(path.join(__dirname,'../src/desktop/view/slack_lists/rename_restore.js'),'utf8');

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
  return {document,dialog,root,done,first,target,close,other,grid,headers,frames,timers,observers,
    arm:()=>context.armListRenameReturn(root),mutate,frame,unmount,emit,
    expire:()=>{for(const fn of [...timers.values()])fn()},
    assertStopped(){assert.equal(timers.size,0);assert.equal(frames.size,0);assert(observers.every(o=>!o.connected));assert([...listeners.values()].every(set=>set.size===0))},
  };
}

test('early frame waits for real unmount, then restores exact second column over generic manager focus',()=>{
  const h=setup();h.arm();h.frame();assert.equal(h.document.activeElement,h.done);
  h.unmount();h.document.activeElement=h.dialog;h.frame();
  assert.equal(h.document.activeElement,h.target);assert.notEqual(h.document.activeElement,h.first);h.assertStopped();
});
test('generic grid manager focus does not defeat exact origin restoration',()=>{
  const h=setup();h.arm();h.unmount();h.document.activeElement=h.grid;h.frame();assert.equal(h.document.activeElement,h.target);
});
test('a newly chosen control keeps focus',()=>{
  const h=setup();h.arm();h.unmount();h.other.focus();h.frame();assert.equal(h.document.activeElement,h.other);h.assertStopped();
});
test('trusted pointer or keyboard input cancels pending restoration',()=>{
  for(const kind of ['pointerdown','keydown']){const h=setup();h.arm();h.emit(kind,{isTrusted:true});h.unmount();h.frame();assert.equal(h.document.activeElement,h.document.body);h.assertStopped()}
});
test('same element with changed visit refuses restoration',()=>{
  const h=setup();h.arm();h.unmount();h.dialog.dataset.dialogGeneration='visit2';h.frame();assert.equal(h.document.activeElement,h.document.body);h.assertStopped();
});
test('replacement dialog with equal visit refuses old mount authority',()=>{
  const h=setup();h.arm();h.unmount();h.document.dialog={dataset:{dialogGeneration:'visit1'}};h.frame();assert.equal(h.document.activeElement,h.document.body);h.assertStopped();
});
test('another top modal prevents restoration into covered List',()=>{
  const h=setup();h.arm();h.unmount();h.document.dialogs=[h.dialog,h.other];h.frame();assert.equal(h.document.activeElement,h.document.body);h.assertStopped();
});
test('replacement rename receipt never receives old return focus',()=>{
  const h=setup();h.arm();h.document.panel={dataset:{renameSerial:'receipt8'}};h.mutate();h.frame();assert.equal(h.document.activeElement,h.done);h.assertStopped();
});
test('disabled or missing original header falls back to List close, never another column',()=>{
  for(const remove of [false,true]){const h=setup();h.arm();h.unmount();if(remove)h.headers.pop();else h.target.disabled=true;h.frame();assert.equal(h.document.activeElement,h.close);assert(h.target.disabled||remove);h.assertStopped()}
});
test('refused close remains mounted and lease expires without moving focus',()=>{
  const h=setup();h.arm();h.frame();h.expire();h.unmount();h.frame();assert.equal(h.document.activeElement,h.document.body);h.assertStopped();
});
test('unobserved live outside control also wins before final frame',()=>{
  const h=setup();h.arm();h.unmount();h.document.activeElement=h.other;h.frame();assert.equal(h.document.activeElement,h.other);h.assertStopped();
});
test('actual mount source arms on capture-phase Done/Cancel before framework unmount',()=>{
  const focusSource=fs.readFileSync(path.join(__dirname,'../src/desktop/view/slack_lists/rename_focus.js'),'utf8');
  const h=setup();let click,armed=0;
  h.root.addEventListener=(type,fn,capture)=>{assert.equal(type,'click');assert.equal(capture,true);click=fn};
  h.done.closest=selector=>selector==='#slack-list-rename-close'?h.done:null;
  const ctx=vm.createContext({document:h.document,armListRenameReturn:root=>{assert.equal(root,h.root);armed++},
    MutationObserver:class{observe(){}disconnect(){}},requestAnimationFrame:()=>1});
  vm.runInContext(focusSource,ctx);click({target:h.done});assert.equal(armed,1);
  h.done.disabled=true;click({target:h.done});assert.equal(armed,1);
});

function viewSetup(){const h=setup();h.root.dataset.renameColumn='';h.root.dataset.renameView='V_CUSTOM';h.dialog.dataset.viewSelection='selection1';h.target.dataset={renameView:'V_CUSTOM'};h.first.dataset={renameView:'V_OTHER'};h.picker={value:'V_CUSTOM'};h.dialog.querySelector=s=>s==='#slack-list-view-select'?h.picker:s==='#slack-list-close'?h.close:null;return h;}
test('view close restores exact saved view action after actual unmount',()=>{const h=viewSetup();h.arm();h.frame();h.unmount();h.document.activeElement=h.grid;h.frame();assert.equal(h.document.activeElement,h.target);h.assertStopped()});
test('changed selected view refuses original view focus return',()=>{const h=viewSetup();h.arm();h.unmount();h.picker.value='V_OTHER';h.frame();assert.equal(h.document.activeElement,h.document.body);h.assertStopped()});
test('disabled view action falls back to close without enabling mutation',()=>{const h=viewSetup();h.arm();h.unmount();h.target.disabled=true;h.frame();assert.equal(h.document.activeElement,h.close);assert.equal(h.target.disabled,true);h.assertStopped()});
test('chosen control wins over pending view return',()=>{const h=viewSetup();h.arm();h.unmount();h.other.focus();h.frame();assert.equal(h.document.activeElement,h.other);h.assertStopped()});

test('same selected id with newer selection clock refuses view focus return',()=>{const h=viewSetup();h.arm();h.unmount();h.dialog.dataset.viewSelection='selection2';h.frame();assert.equal(h.document.activeElement,h.document.body);h.assertStopped()});
