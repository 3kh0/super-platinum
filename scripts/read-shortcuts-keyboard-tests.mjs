import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';

const api = vm.runInNewContext(fs.readFileSync(new URL('../src/desktop/read_shortcuts/keyboard.js', import.meta.url), 'utf8'));
function fixture() {
  const node = {id:'message-row', tagName:'DIV', isConnected:true, parentElement:null,
    getClientRects:()=>[{}], getAttribute:()=>null, hasAttribute:()=>false,
    closest:()=>null, contains:other=>other === node};
  const doc = {activeElement:node, visibilityState:'visible', hasFocus:()=>true,
    querySelectorAll:()=>[], defaultView:{getComputedStyle:()=>({display:'block', visibility:'visible'})}};
  const event = {key:'Escape', target:node};
  return {node, doc, event};
}

test('plain and Shift Escape remain separate and never change text', () => {
  const {node,doc,event} = fixture();
  node.value = 'draft text';
  assert.equal(api.capture(event,doc).facts.shift,false);
  assert.equal(api.capture({...event,shiftKey:true},doc).facts.shift,true);
  assert.equal(node.value,'draft text');
});

test('IME final keyCode, modifiers, repeat and prevented keys are rejected', () => {
  const {doc,event} = fixture();
  for (const extra of [{ctrlKey:true},{metaKey:true},{altKey:true},{repeat:true},
    {isComposing:true},{keyCode:229},{defaultPrevented:true},{key:'Enter'}]) {
    assert.equal(api.capture({...event,...extra},doc),null);
  }
});

test('local native menu consumption and visible foreign popup block admission', () => {
  const {doc,event,node} = fixture();
  node.closest = selector => selector.includes('data-escape-local') ? node : null;
  assert.equal(api.capture(event,doc),null);
  node.closest = () => null;
  const popup = {...node};
  doc.querySelectorAll = () => [popup];
  assert.equal(api.capture(event,doc),null);
  popup.hidden = true;
  assert.ok(api.capture(event,doc));
  popup.hidden = false;
  popup.parentElement = {hidden:true};
  assert.ok(api.capture(event,doc));
});

test('captured focus is invalid after blur, detach, popup, or later preventDefault', () => {
  for (const change of [
    f=>{f.doc.activeElement={};}, f=>{f.node.isConnected=false;},
    f=>{f.doc.hasFocus=()=>false;}, f=>{f.doc.visibilityState='hidden';},
    f=>{f.event.defaultPrevented=true;},
    f=>{f.doc.querySelectorAll=()=>[f.node];},
  ]) {
    const f=fixture(), captured=api.capture(f.event,f.doc);
    assert.equal(captured.current(),true);
    change(f);
    assert.equal(captured.current(),false);
  }
});

test('empty and nonempty canonical composers differ from unrelated editors', () => {
  const {doc,event,node}=fixture();
  node.tagName='TEXTAREA'; node.id='thread-composer'; node.value='';
  node.closest=selector=>selector.startsWith('input,')?node:null;
  assert.equal(api.capture(event,doc).facts.editor,'emptyComposer');
  node.value='unsent local draft';
  assert.equal(api.capture(event,doc).facts.editor,'nonemptyComposer');
  node.id='message-edit';
  assert.equal(api.capture(event,doc).facts.editor,'other');
  assert.equal(node.value,'unsent local draft');
});

test('old editor cannot send an event using another focused row', () => {
  const {doc,event}=fixture();
  doc.activeElement={isConnected:true,id:'different'};
  assert.equal(api.capture(event,doc),null);
});

function bridgeFixture() {
  const f=fixture(), docListeners=new Map(), winListeners=new Map();
  const shell={nodeType:1,isConnected:true,dataset:{readShortcutKey:'visit-1'},contains:()=>true,
    matches:()=>false,closest:()=>null,querySelector:()=>null};
  f.node.nodeType=1;f.node.matches=()=>false;f.node.querySelector=()=>null;
  f.doc.querySelector=()=>shell;f.doc.documentElement={};f.doc.body={};
  f.doc.addEventListener=(name,fn)=>docListeners.set(name,fn);
  f.doc.removeEventListener=name=>docListeners.delete(name);
  const window={addEventListener:(name,fn)=>winListeners.set(name,fn),removeEventListener:name=>winListeners.delete(name)};
  const observers=[];
  class Observer {
    constructor(callback){this.callback=callback;this.records=[];observers.push(this);}
    observe(){this.active=true;}
    disconnect(){this.active=false;}
    takeRecords(){const result=this.records;this.records=[];return result;}
  }
  vm.runInNewContext(`const readKeyboard=${fs.readFileSync(new URL('../src/desktop/read_shortcuts/keyboard.js',import.meta.url),'utf8')};\n${fs.readFileSync(new URL('../src/desktop/read_shortcuts/bridge.js',import.meta.url),'utf8')}`,
    {document:f.doc,window,MutationObserver:Observer});
  return {...f,shell,service:window.superPlatinumReadShortcuts,observers,docListeners,winListeners};
}

test('bridge receipts are one-use and a stale token cannot disable the newer observer',()=>{
  const f=bridgeFixture(), first=f.service.capture(f.event), second=f.service.capture(f.event);
  assert.equal(f.service.take(first.token),false);
  assert.equal(f.observers.at(-1).active,true);
  assert.equal(f.service.take(second.token),true);
  assert.equal(f.service.take(second.token),false);
  assert.equal(f.observers.at(-1).active,false);
});

test('bridge latches popup appearance and disappearance in the same task',()=>{
  const f=bridgeFixture(), receipt=f.service.capture(f.event);
  const popup={nodeType:1,matches:()=>true};
  f.observers.at(-1).records.push({type:'childList',target:f.node,addedNodes:[popup],removedNodes:[popup]});
  assert.equal(f.service.take(receipt.token),false);
});

test('bridge latches shell key ABA and focus leave-return before dispatch',()=>{
  const f=bridgeFixture(), receipt=f.service.capture(f.event);
  f.observers.at(-1).records.push({type:'attributes',target:f.shell,attributeName:'data-read-shortcut-key',oldValue:'visit-1'},
    {type:'attributes',target:f.shell,attributeName:'data-read-shortcut-key',oldValue:'visit-2'});
  assert.equal(f.service.take(receipt.token),false);
  const next=f.service.capture(f.event);
  f.docListeners.get('focusout')(); f.docListeners.get('focusin')();
  assert.equal(f.service.take(next.token),false);
});

test('bridge latches blocker attributes added and removed within one task',()=>{
  for (const [attributeName,oldValue] of [['role','menu'],['role','listbox'],
    ['data-keyboard-dialog',''],['data-read-shortcut-blocker',''],
    ['data-escape-local',''],['data-local-keyboard','']]) {
    const f=bridgeFixture(), receipt=f.service.capture(f.event);
    // The existing node has returned to its initial non-blocking state by delivery.
    f.observers.at(-1).records.push({type:'attributes',target:f.node,attributeName,oldValue:null},
      {type:'attributes',target:f.node,attributeName,oldValue});
    assert.equal(f.service.take(receipt.token),false,attributeName);
  }
});

test('read-only reconciliation proof requires the same surviving DOM focus',()=>{
  const f=bridgeFixture(), receipt=f.service.capture(f.event);
  assert.equal(f.service.take(receipt.token),true);
  assert.equal(f.service.currentFocus(receipt.focusGeneration,receipt.key),true);
  f.node.isConnected=false;
  assert.equal(f.service.currentFocus(receipt.focusGeneration,receipt.key),false);
});

test('unchanged shell attributes and unrelated child updates preserve a queued key',()=>{
  const f=bridgeFixture(), receipt=f.service.capture(f.event);
  const hiddenPopup={nodeType:1,matches:()=>true};
  f.shell.querySelector=()=>hiddenPopup;
  f.shell.getAttribute=name=>name==='class'?'app-shell':null;
  const ordinary={nodeType:1,matches:()=>false,closest:()=>null,querySelector:()=>null};
  f.observers.at(-1).records.push(
    {type:'attributes',target:f.shell,attributeName:'data-read-shortcut-key',oldValue:'visit-1'},
    {type:'attributes',target:f.shell,attributeName:'class',oldValue:'app-shell'},
    {type:'childList',target:f.shell,addedNodes:[ordinary],removedNodes:[]});
  assert.equal(f.service.take(receipt.token),true);
});

test('empty completion response leaves an already hidden empty listbox unchanged',()=>{
  const f=bridgeFixture(), receipt=f.service.capture(f.event);
  const popup={nodeType:1,matches:()=>true,getAttribute:name=>name==='hidden'?'':null};
  f.observers.at(-1).records.push(
    {type:'childList',target:popup,addedNodes:[],removedNodes:[]},
    {type:'attributes',target:popup,attributeName:'hidden',oldValue:''});
  assert.equal(f.service.take(receipt.token),true);
});
