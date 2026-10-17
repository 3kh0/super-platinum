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


function deletion() {
  const h=setup(); h.dialog.dataset.viewSelection='selection8';
  h.root.dataset.deleteColumn='HIDDEN'; h.root.__deleteOpener=h.target;
  h.target.dataset.deleteFieldSelection='selection8';
  h.dialog.querySelector=s=>s==='#slack-list-delete-field'?h.target:s==='#slack-list-close'?h.close:null;
  return h;
}
test('deletion Cancel/Done returns to the exact deletion opener, not a field rename',()=>{
  const h=deletion();h.arm();h.unmount();h.frame();assert.equal(h.document.activeElement,h.target);h.assertStopped();
});
test('changed selection retires deletion return',()=>{
  const h=deletion();h.arm();h.dialog.dataset.viewSelection='new';h.unmount();h.frame();assert.equal(h.document.activeElement,h.document.body);h.assertStopped();
});
test('replacement deletion opener cannot inherit return ownership',()=>{
  const h=deletion();h.arm();h.dialog.querySelector=s=>s==='#slack-list-delete-field'?h.first:null;h.unmount();h.frame();assert.equal(h.document.activeElement,h.document.body);h.assertStopped();
});
test('missing original deletion opener never arms a return',()=>{
  const h=deletion();delete h.root.__deleteOpener;h.arm();assert.equal(h.frames.size,0);assert.equal(h.timers.size,0);
});
test('disabled original deletion opener uses current List Close',()=>{
  const h=deletion();h.arm();h.target.disabled=true;h.unmount();h.frame();assert.equal(h.document.activeElement,h.close);h.assertStopped();
});
test('replacement serial and visit each retire deletion return',()=>{
  for(const which of ['serial','visit']){const h=deletion();h.arm();if(which==='serial')h.root.dataset.renameSerial='new';else h.dialog.dataset.dialogGeneration='new';h.mutate();h.frame();assert.equal(h.document.activeElement,h.done);h.assertStopped();}
});
test('trusted input and new interactive focus each override deletion restoration',()=>{
  for(const input of ['pointerdown','keydown','focus']){const h=deletion();h.arm();if(input==='focus')h.other.focus();else h.emit(input,{isTrusted:true});h.unmount();h.frame();assert.equal(h.document.activeElement,h.document.body);h.assertStopped();}
});

const focusSource=fs.readFileSync(path.join(process.argv[2]||process.cwd(),'src/desktop/view/slack_lists/rename_focus.js'),'utf8');
test('actual focus helper chooses field chooser or Cancel, never permanent confirmation',()=>{
  for(const disabled of [false,true]) {
    const h=deletion();h.document.activeElement=h.document.body;h.first.parent=h.root;h.first.disabled=disabled;
    h.root.addEventListener=()=>{};
    h.root.querySelector=s=>s==='#slack-list-delete-field-choice:not([disabled])'&&!disabled?h.first:s==='#slack-list-rename-close:not([disabled])'?h.done:null;
    vm.runInContext(focusSource,h.context);h.frame();assert.equal(h.document.activeElement,disabled?h.done:h.first);
  }
});
test('actual focus helper refuses stale serial before selecting a field',()=>{
  const h=deletion();h.document.activeElement=h.document.body;h.root.addEventListener=()=>{};h.root.querySelector=()=>h.first;
  vm.runInContext(focusSource,h.context);h.root.dataset.renameSerial='replacement';h.frame();assert.equal(h.document.activeElement,h.document.body);
});
