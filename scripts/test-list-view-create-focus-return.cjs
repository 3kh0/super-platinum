// Actual production return-focus source with deterministic DOM/frame scheduling.
// No app, browser, transport or native input is started by these tests.
const test=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const vm=require('node:vm');
const source=fs.readFileSync(path.join(__dirname,'../src/desktop/view/slack_lists/view_create_restore.js'),'utf8');

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
  const dialog=element('SECTION',{dataset:{dialogGeneration:'10',viewSelection:'20'}});
  const root=element('SECTION',{dataset:{renameSerial:'30',viewCreate:'table'}});
  const done=element('BUTTON',{parent:root}),first=element('BUTTON',{dataset:{renameColumn:'FIRST'}});
  const target=element('BUTTON',{dataset:{renameColumn:'SECOND'}}),close=element('BUTTON');
  const other=element('BUTTON'),grid=element('TD'),picker=element('SELECT',{value:'V_CUSTOM'});
  const headers=[first,target];
  dialog.querySelectorAll=()=>headers;
  dialog.querySelector=selector=>selector==='#slack-list-close'?close:selector==='#slack-list-view-select'?picker:selector==='#slack-list-new-view'?target:null;
  document.dialog=dialog;document.panel=root;document.dialogs=[dialog,root];document.activeElement=done;
  function emit(type,event){for(const fn of [...(listeners.get(type)||[])])fn(event)}
  class Observer{constructor(cb){this.cb=cb;this.connected=false;observers.push(this)}observe(){this.connected=true}disconnect(){this.connected=false}}
  const window={};const context=vm.createContext({window,document,MutationObserver:Observer,
    requestAnimationFrame(fn){const id=++serial;frames.set(id,fn);return id},cancelAnimationFrame(id){frames.delete(id)},
    setTimeout(fn){const id=++serial;timers.set(id,fn);return id},clearTimeout(id){timers.delete(id)},
  });
  vm.runInContext(source,context);
  const mutate=(records=[])=>{for(const o of observers)if(o.connected)o.cb(records)};
  const frame=()=>{const pending=[...frames.values()];frames.clear();for(const fn of pending)fn()};
  const unmount=()=>{root.isConnected=false;done.isConnected=false;document.panel=null;document.dialogs=[dialog];document.activeElement=document.body;mutate()};
  return {document,dialog,root,done,first,target,close,other,grid,headers,frames,timers,observers,
    picker,window,
    arm:(opening=false)=>context.armListViewCreateReturn(root,opening),mutate,frame,unmount,emit,
    commit:(proof={serial:'30',visit:'10',selection:'21',id:'V_SERVER_NEW'})=>window.__listViewCreateReturn?.commit(proof),
    select(value='21',id='V_SERVER_NEW'){const old=dialog.dataset.viewSelection;dialog.dataset.viewSelection=value;picker.value=id;mutate([{type:'attributes',target:dialog,attributeName:'data-view-selection',oldValue:old}])},
    expire:()=>{for(const fn of [...timers.values()])fn()},
    assertStopped(){assert.equal(timers.size,0);assert.equal(frames.size,0);assert(observers.every(o=>!o.connected));assert([...listeners.values()].every(set=>set.size===0))},
  };
}


test('cancel waits actual unmount and restores exact Add control over manager focus',()=>{const h=setup();h.arm();h.frame();assert.equal(h.document.activeElement,h.done);h.unmount();h.document.activeElement=h.grid;h.frame();assert.equal(h.document.activeElement,h.target);h.assertStopped()});
test('Open waits exact reducer proof and actual selected DOM, then focuses picker',()=>{const h=setup();h.arm(true);h.frame();h.select();h.unmount();h.frame();assert.equal(h.document.activeElement,h.document.body);h.commit();h.frame();assert.equal(h.document.activeElement,h.picker);h.assertStopped()});
test('proof before DOM also waits actual transition',()=>{const h=setup();h.arm(true);h.commit();h.frame();assert.equal(h.document.activeElement,h.done);h.select();h.unmount();h.frame();assert.equal(h.document.activeElement,h.picker);h.assertStopped()});
test('same ID ABA or extra selection clock invalidates pending navigation focus',()=>{for(const opening of [false,true]){const h=setup();h.arm(opening);h.select('21','V_OTHER');h.select('22','V_CUSTOM');h.commit();h.unmount();h.frame();assert.equal(h.document.activeElement,h.document.body);h.assertStopped()}});
test('wrong receipt or visit proof is never accepted',()=>{for(const field of ['serial','visit']){const h=setup();h.arm(true);h.commit({serial:'30',visit:'10',selection:'21',id:'V_SERVER_NEW',[field]:'wrong'});h.select();h.unmount();h.frame();assert.equal(h.document.activeElement,h.document.body);h.assertStopped()}});
test('trusted input and a newly chosen control cancel both modes',()=>{for(const opening of [false,true]){for(const kind of ['pointerdown','keydown','focusin']){const h=setup();h.arm(opening);if(kind==='focusin')h.other.focus();else h.emit(kind,{isTrusted:true});h.select();h.unmount();h.commit();h.frame();assert.equal(h.document.activeElement,h.document.body);h.assertStopped()}}});
test('replacement mount or visit and competing modal refuse restoration',()=>{for(const changed of ['mount','visit','modal','picker']){const h=setup();h.arm(true);h.select();h.unmount();h.commit();if(changed==='mount')h.document.dialog={dataset:{dialogGeneration:'10'}};if(changed==='visit')h.dialog.dataset.dialogGeneration='11';if(changed==='modal')h.document.dialogs.push(h.other);if(changed==='picker')h.dialog.querySelector=s=>s==='#slack-list-view-select'?{}:h.close;h.frame();assert.equal(h.document.activeElement,h.document.body);h.assertStopped()}});
test('disabled Add uses Close without enabling it',()=>{const h=setup();h.arm();h.target.disabled=true;h.unmount();h.frame();assert.equal(h.document.activeElement,h.close);assert(h.target.disabled);h.assertStopped()});
test('missing proof or refused close expires all observers/listeners',()=>{for(const opening of [false,true]){const h=setup();h.arm(opening);h.frame();h.expire();h.unmount();h.commit();h.frame();assert.equal(h.document.activeElement,h.document.body);h.assertStopped()}});
test('actual mounted click listener arms cancel and Open separately before framework',()=>{
  const h=setup();let click,opens=[];h.root.addEventListener=(_type,fn,capture)=>{assert(capture);click=fn};
  h.done.closest=s=>s==='#slack-list-rename-close'?h.done:null;
  const ctx=vm.createContext({document:h.document,armListViewCreateReturn:(root,opening)=>{assert.equal(root,h.root);opens.push(opening)},MutationObserver:class{observe(){}disconnect(){}},requestAnimationFrame:()=>1});
  vm.runInContext(fs.readFileSync(path.join(__dirname,'../src/desktop/view/slack_lists/rename_focus.js'),'utf8'),ctx);
  click({target:h.done});h.done.closest=s=>s==='#slack-list-new-view-open'?h.done:null;click({target:h.done});assert.deepEqual(opens,[false,true]);h.done.disabled=true;click({target:h.done});assert.deepEqual(opens,[false,true]);
});
