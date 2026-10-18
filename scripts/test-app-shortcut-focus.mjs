import fs from 'node:fs';import path from 'node:path';import vm from 'node:vm';import assert from 'node:assert/strict';
const source=fs.readFileSync(process.argv[2]||'src/desktop/message_buttons/shortcuts/focus.js','utf8').replace('__OWNER__','7');
function setup({owner='7',newer=false,query=false}={}) {
 let callbacks=[],raf=[],focus=0,removed=0,hasQuery=query,dataInput=false;const body={};const listeners={};
 const root={isConnected:true,dataset:{dialogGeneration:owner},parentNode:{},querySelectorAll:()=>hasQuery&&dataInput?[field]:[],getClientRects:()=>[{}],closest:()=>null,contains:e=>e?.root===root,
 addEventListener:(k,f)=>listeners[k]=f,removeEventListener:()=>removed++,querySelector:selector=>selector==='#app-shortcut-query'?(hasQuery?field:null):close};
 const close={root,isConnected:true,disabled:false,focus(){doc.activeElement=this;focus++;listeners.focusin?.({target:this});},closest:selector=>selector==='[data-keyboard-dialog]'?root:null};
 const field={root,isConnected:true,disabled:false,dataset:{},getClientRects:()=>[{}],select(){},focus(){doc.activeElement=this;focus++;listeners.focusin?.({target:this});},closest:selector=>selector==='[data-keyboard-dialog]'?root:null};
 const old={root,isConnected:true,disabled:false,closest:selector=>selector==='[data-keyboard-dialog]'?root:null};
 const other={isConnected:true,getClientRects:()=>[{}],closest:()=>null};
 const wrapper={querySelectorAll:()=>[root]};const doc={body,activeElement:body,addEventListener(){},getElementById:id=>id==='shell-overlays'?wrapper:root,querySelectorAll:()=>newer?[root,other]:[root]};
 class MutationObserver {constructor(f){callbacks.push(f)} observe(){}disconnect(){this.stopped=true}}
 const context=vm.createContext({document:doc,MutationObserver,requestAnimationFrame:f=>raf.push(f),window:{},setTimeout,clearTimeout,getComputedStyle:()=>({visibility:'visible'})});vm.runInContext(source,context);
 return {root,old,close,field,doc,body,callbacks,raf,listeners,context,setQuery(v,attr){hasQuery=v;dataInput=attr},get focus(){return focus},get removed(){return removed}};
}
let groups=0;function test(name,f){f();groups++;console.log('PASS '+name)}
test('initial owned mount focuses close while catalog loads',()=>{const x=setup();x.raf[0]();assert.equal(x.doc.activeElement,x.close)});
test('query takes explicit preference over earlier Close',()=>{const x=setup({query:true});x.raf[0]();assert.equal(x.doc.activeElement,x.field)});
test('old generation cannot install focus controller',()=>{const x=setup({owner:'8'});assert.equal(x.callbacks.length,0);assert.equal(x.raf.length,0)});
test('removed radio returns focus to stable close',()=>{const x=setup();x.listeners.focusin({target:x.old});x.old.isConnected=false;x.callbacks[0]();assert.equal(x.doc.activeElement,x.close)});
test('disabled Run returns focus to stable close',()=>{const x=setup();x.listeners.focusin({target:x.old});x.old.disabled=true;x.doc.activeElement=x.old;x.callbacks[0]();assert.equal(x.doc.activeElement,x.close)});
test('intervening explicit focus is retained',()=>{const x=setup();x.listeners.focusin({target:x.old});x.old.isConnected=false;const chosen={};x.doc.activeElement=chosen;x.callbacks[0]();assert.equal(x.doc.activeElement,chosen)});
test('replacement dialog prevents mount and recovery theft',()=>{const x=setup({newer:true});x.listeners.focusin({target:x.old});x.old.disabled=true;x.raf[0]();x.callbacks[0]();assert.equal(x.focus,0)});
test('unknown prior focus does not trigger recovery',()=>{const x=setup();x.callbacks[0]();assert.equal(x.focus,0)});
test('detached controller cleans up without focus',()=>{const x=setup();x.root.isConnected=false;x.callbacks[0]();x.raf[0]();assert.equal(x.removed,1);assert.equal(x.focus,0)});
test('actual shared observer does not steal newer Close focus on catalog arrival',()=>{
 const x=setup();x.raf[0]();
 vm.runInContext(fs.readFileSync('src/desktop/overlay_keyboard.js','utf8'),x.context);
 x.close.focus();
 const view=fs.readFileSync(path.join(path.dirname(process.argv[2]||'src/desktop/message_buttons/shortcuts/focus.js'),'view.rs'),'utf8');
 assert(!view.includes('data-dialog-input'));
 x.setQuery(true,false);for(const f of [...x.callbacks])f();assert.equal(x.doc.activeElement,x.close);
 // Negative control exercises the actual shared helper's historical behavior.
 x.setQuery(true,true);for(const f of [...x.callbacks])f();assert.equal(x.doc.activeElement,x.field);
});
console.log(JSON.stringify({pass:true,groups}));
