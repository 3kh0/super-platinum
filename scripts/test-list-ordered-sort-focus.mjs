import fs from 'node:fs';import path from 'node:path';import vm from 'node:vm';import assert from 'node:assert/strict';import test from 'node:test';
const source=fs.readFileSync(path.join(process.argv[2]||'.','src/desktop/view/slack_lists/sort/focus.js'),'utf8');
function setup(){
 const proof={serial:'7',visit:'3',structure:'21',token:'101'},frames=[],listeners=new Set(),timers=new Map(),nodes=new Map();let reply,handshake;
 const set={id:'slack-list-sort-set',isConnected:true,dataset:{sortVisit:'3',sortStructure:'21'}};
 const active={id:'remove'};nodes.set(active.id,active);nodes.set(set.id,set);
 const root={id:'slack-list-rename',isConnected:true,dataset:{renameSerial:'7',renameStage:'Editing'},getClientRects:()=>[{}],contains:n=>Array.from(nodes.values()).includes(n),querySelector:s=>nodes.get(s.slice(1))};nodes.set(root.id,root);const dialogs=[root];
 const document={activeElement:active,getElementById:id=>nodes.get(id),querySelectorAll:()=>dialogs,addEventListener:(_,f)=>listeners.add(f),removeEventListener:(_,f)=>listeners.delete(f)};
 const context=vm.createContext({document,requestAnimationFrame:f=>frames.push(f),setTimeout:f=>{timers.set(1,f);return 1;},clearTimeout:id=>timers.delete(id),dioxus:{send:v=>{handshake=v;},recv:()=>new Promise(r=>{reply=r;})}});vm.runInContext(source,context);
 const running=context.orderedSortFocus(proof);
 const control=id=>{const n={id,isConnected:true,disabled:false,getClientRects:()=>[{}],focused:0,revealed:0,focus:()=>{n.focused++;document.activeElement=n;for(const f of listeners)f({target:n});},scrollIntoView:()=>n.revealed++};nodes.set(id,n);return n;};
 return {proof,root,set,active,nodes,dialogs,document,listeners,timers,running,control,handshake:()=>handshake,reply:v=>reply(v),frame:async()=>{await new Promise(setImmediate);frames.shift()?.();await new Promise(setImmediate);}};
}
test('exact successful row handoff waits for new structure and reveals only the target',async()=>{const x=setup();assert.equal(x.handshake(),true);const target=x.control('sort-field-2');x.reply({id:target.id,structure:'22'});await x.frame();assert.equal(target.focused,0);x.set.dataset.sortStructure='22';await x.frame();await x.running;assert.equal(target.focused,1);assert.equal(target.revealed,1);assert.equal(x.listeners.size,0);});
test('new intentional focus aborts delayed structural focus',async()=>{const x=setup(),target=x.control('sort-field-2'),other=x.control('other');x.reply({id:target.id,structure:'22'});other.focus();x.set.dataset.sortStructure='22';await x.frame();await x.running;assert.equal(target.focused,0);assert.equal(target.revealed,0);});
test('new modal old node replacement and retired serial refuse focus',async()=>{for(const mutate of [x=>x.dialogs.push({isConnected:true,getClientRects:()=>[{}]}),x=>x.nodes.set(x.root.id,{...x.root}),x=>x.root.dataset.renameSerial='8',x=>x.root.dataset.renameStage='Preparing']){const x=setup(),target=x.control('sort-field-2');x.reply({id:target.id,structure:'22'});x.set.dataset.sortStructure='22';mutate(x);await x.frame();await x.running;assert.equal(target.focused,0);assert.equal(x.listeners.size,0);}});
test('only unavailable Add picker falls back to exact Cancel',async()=>{const x=setup(),picker=x.control('slack-list-sort-add-field'),cancel=x.control('slack-list-rename-close');picker.disabled=true;x.reply({id:picker.id,structure:'22'});x.set.dataset.sortStructure='22';await x.frame();await x.running;assert.equal(cancel.focused,1);assert.equal(picker.focused,0);});
test('failed action and lost host reply clean listener without any focus',async()=>{for(const lost of [false,true]){const x=setup();if(lost){for(const f of x.timers.values())f();}else{x.reply(null);}await x.running;assert.equal(x.document.activeElement,x.active);assert.equal(x.listeners.size,0);assert.equal(x.timers.size,0);}});

test('paused animation frames cannot retain the focus listener beyond deadline',async()=>{const x=setup();x.reply({id:'missing',structure:'22'});await new Promise(setImmediate);for(const f of x.timers.values())f();await x.running;assert.equal(x.listeners.size,0);assert.equal(x.document.activeElement,x.active);});

test('disconnected or hidden later dialog does not steal the owned visible dialog proof',async()=>{for(const modal of [{isConnected:false,getClientRects:()=>[{}]},{isConnected:true,getClientRects:()=>[]}]){const x=setup(),target=x.control('sort-field-2');x.dialogs.push(modal);x.reply({id:target.id,structure:'22'});x.set.dataset.sortStructure='22';await x.frame();await x.running;assert.equal(target.focused,1);}});

test('retired token cannot focus or clear a newer DOM reservation',async()=>{const x=setup(),target=x.control('sort-field-2');x.reply({id:target.id,structure:'22'});x.root.dataset.sortFocus='102';x.set.dataset.sortStructure='22';await x.frame();await x.running;assert.equal(target.focused,0);assert.equal(x.root.dataset.sortFocus,'102');assert.equal(x.listeners.size,0);});

const parentSource=fs.readFileSync(path.join(process.argv[2]||'.','src/desktop/view/slack_lists/rename_focus.js'),'utf8');
function parentFocus(){
 const frames=[];let observer;const dialog={dataset:{dialogGeneration:'3'}},save={disabled:true},other={disabled:false};
 const document={activeElement:save,getElementById:()=>root,querySelectorAll:()=>[root]};
 const cancel={disabled:false,focus:()=>{document.activeElement=cancel;}};
 const root={isConnected:true,dataset:{renameSerial:'7',renameStage:'Preparing',sortFocus:'101'},closest:s=>s==='.slack-list-dialog'?dialog:null,addEventListener(){},contains:n=>[save,other,cancel].includes(n),querySelector:s=>s==='#slack-list-rename-close:not([disabled])'?cancel:null,getBoundingClientRect:()=>({width:100})};
 class MO{constructor(f){this.callback=f;observer=this;}observe(_target,options){this.options=options;}disconnect(){}}
 vm.runInNewContext(parentSource,{document,MutationObserver:MO,requestAnimationFrame:f=>frames.push(f)});
 const mutation=name=>{if(observer.options.attributeFilter.includes(name))observer.callback();};
 return {root,document,save,other,cancel,mutation,flush:()=>frames.shift()?.()};
}
test('actual parent resumes exact fallback after stage/disabled scheduling was skipped by the sort owner',()=>{
 const x=parentFocus();x.mutation('data-rename-stage');x.mutation('disabled');x.flush();assert.equal(x.document.activeElement,x.save);
 delete x.root.dataset.sortFocus;x.mutation('data-sort-focus');x.flush();assert.equal(x.document.activeElement,x.cancel);
});
test('actual parent marker cleanup preserves newer enabled in-modal focus',()=>{
 const x=parentFocus();x.flush();x.document.activeElement=x.other;delete x.root.dataset.sortFocus;x.mutation('data-sort-focus');x.flush();assert.equal(x.document.activeElement,x.other);
});
