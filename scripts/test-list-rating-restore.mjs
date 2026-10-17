import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
const base=process.argv[2]||'.';
const code=fs.readFileSync(path.join(base,'src/desktop/slack_lists/rename/text_create/rating_restore.js'),'utf8');
function setup(initial){
 const frames=new Map(),timers=new Map(),calls=[],body={},oldFocus={},outside={},sent=[];let serial=0,receive,mutation,focus,now=0;
 const shown={isConnected:true,getClientRects:()=>[{}],closest:()=>null};
 const picker={...shown,dataset:{ratingPicker:'13'},contains:n=>n===oldFocus};
 const dialog={...shown,dataset:{dialogGeneration:'9'}};
 const button={...shown,dataset:{fieldChoice:'11'},disabled:false,focus:o=>{calls.push(o);document.activeElement=button;}};
 const root={...shown,setAttribute(){},removeAttribute(){},dataset:{renameSerial:'7'},inert:true,closest:s=>s==='.slack-list-dialog'?dialog:null,querySelector:()=>button};
 let currentRoot=root,currentPicker=picker,top=root,emoji=picker;
 const document={body,activeElement:oldFocus,getElementById:id=>id==='slack-list-rename'?currentRoot:currentPicker,querySelector:()=>emoji,querySelectorAll:()=>[dialog,root,...(top===root?[]:[top])],addEventListener:(n,f)=>{focus=f;},removeEventListener:()=>{focus=null;}};
 let disconnected=false;
 const context=vm.createContext({document,performance:{now:()=>now},getComputedStyle:()=>({visibility:'visible'}),requestAnimationFrame:f=>{frames.set(++serial,f);return serial;},cancelAnimationFrame:n=>frames.delete(n),setTimeout:f=>{timers.set(++serial,f);return serial;},clearTimeout:n=>timers.delete(n),MutationObserver:class{constructor(f){mutation=f;}observe(){}disconnect(){disconnected=true;}},dioxus:{send:v=>sent.push(v),recv:()=>new Promise(r=>{receive=r;})}});
 vm.runInContext(code,context);initial?.({document,root,dialog,button,picker,outside});
 const promise=vm.runInContext('restoreRatingEmoji(7,9,11,13)',context);
 return {document,root,dialog,button,picker,outside,calls,sent,promise,frames,timers,advance:n=>{now=n;},
  async commit(revision=11){receive(revision);await new Promise(setImmediate);},
  remove(){picker.isConnected=false;currentPicker=null;emoji=null;root.inert=false;document.activeElement=body;mutation?.();},
  frame(){const q=[...frames.values()];frames.clear();q.forEach(f=>f());},
  mutate(f){f();mutation?.();},timeout(){[...timers.values()].forEach(f=>f());},
  replace(){currentRoot={...root};},newPicker(){currentPicker={...picker};},newDialog(){top={...shown};},
  outsideFocus(){document.activeElement=outside;focus?.({target:outside});},clean(){assert.equal(frames.size,0);assert.equal(timers.size,0);assert.equal(disconnected,true);}
 };
}
let n=0;async function test(label,f){await f();n++;console.log('PASS',label);}
await test('capture precedes controller disposition; removal before continuation restores once',async()=>{const h=setup();assert.deepEqual(h.sent,[true]);h.remove();await h.commit();h.frame();await h.promise;assert.equal(h.calls.length,1);assert.equal(h.calls[0].preventScroll,true);h.clean();});
await test('removal later than first frame remains observed',async()=>{const h=setup();await h.commit();h.frame();assert.equal(h.calls.length,0);h.remove();h.frame();await h.promise;assert.equal(h.calls.length,1);h.clean();});
await test('chosen configuration restores only new exact revision',async()=>{const h=setup();await h.commit(12);h.remove();h.frame();assert.equal(h.calls.length,0);h.mutate(()=>h.button.dataset.fieldChoice='12');h.frame();await h.promise;assert.equal(h.calls.length,1);h.clean();});
for(const [label,mutate] of [
 ['replacement modal',h=>h.replace()],['new serial',h=>h.root.dataset.renameSerial='8'],['new visit',h=>h.dialog.dataset.dialogGeneration='10'],
 ['new picker token',h=>h.newPicker()],['new top dialog',h=>h.newDialog()],['outside focus',h=>h.outsideFocus()],
 ['disabled button',h=>h.button.disabled=true],['disconnected button',h=>h.button.isConnected=false],['choice ABA',h=>h.button.dataset.fieldChoice='14'],['still inert',h=>h.root.inert=true],
])await test(label+' never steals focus',async()=>{const h=setup();await h.commit();h.remove();h.mutate(()=>mutate(h));h.frame();h.timeout();await h.promise;assert.equal(h.calls.length,0);h.clean();});
await test('unremoved picker times out and cleans its observation',async()=>{const h=setup();await h.commit();h.frame();h.timeout();await h.promise;assert.equal(h.calls.length,0);h.clean();});
await test('failed controller disposition has no return authority',async()=>{const h=setup();await h.commit(null);await h.promise;assert.equal(h.calls.length,0);assert.equal(h.frames.size,0);assert.equal(h.timers.size,0);});
await test('focus moved away and back while waiting still cancels',async()=>{const h=setup();await h.commit();h.outsideFocus();h.document.activeElement=h.document.body;h.remove();h.frame();await h.promise;assert.equal(h.calls.length,0);h.clean();});
await test('snapshot with unowned focus grants no restoration',async()=>{const h=setup(h=>h.document.activeElement=h.outside);assert.deepEqual(h.sent,[false]);await h.commit();await h.promise;assert.equal(h.calls.length,0);assert.equal(h.timers.size,0);});
await test('wrong picker token cannot arm return',async()=>{const h=setup(h=>h.picker.dataset.ratingPicker='14');assert.deepEqual(h.sent,[false]);await h.commit();await h.promise;assert.equal(h.calls.length,0);});
await test('focus changed during snapshot wait never restores even after it returns to body',async()=>{const h=setup();h.outsideFocus();h.document.activeElement=h.document.body;h.remove();await h.commit();await h.promise;assert.equal(h.calls.length,0);assert.equal(h.timers.size,0);});
await test('abandoned snapshot wait is self bounded',async()=>{const h=setup();h.timeout();await h.promise;assert.equal(h.calls.length,0);assert.equal(h.timers.size,0);});
await test('late animation frame cannot restore past the final deadline',async()=>{const h=setup();await h.commit();h.remove();h.advance(1500);h.frame();await h.promise;assert.equal(h.calls.length,0);h.clean();});
console.log(`${n} actual two-phase helper cases passed; no native focus claim`);


// Execute BOTH actual product helpers, including the generic editor recovery
// observer omitted from the original single-helper cases.
const renameCode=fs.readFileSync(path.join(base,'src/desktop/view/slack_lists/rename_focus.js'),'utf8');
function coordinated({send,recv,observe}={}){
 const frames=new Map(),timers=new Map(),observers=new Set(),listeners=new Set(),sent=[],calls=[];
 let sequence=0,receive,currentPicker=true;
 const notify=()=>{for(const o of [...observers])o.callback();};
 const shown={isConnected:true,getClientRects:()=>[{}],getBoundingClientRect:()=>({width:100,height:32}),closest:()=>null};
 const body={},document={body,activeElement:null};
 const focus=n=>{document.activeElement=n;for(const listener of [...listeners])listener({target:n});};
 const query={...shown,id:'emoji-query'},close={...shown,id:'picker-close'};
 const picker={...shown,dataset:{ratingPicker:'13'},contains:n=>n===query||n===close};
 const dialog={...shown,dataset:{dialogGeneration:'9'}};
 const name={...shown,id:'slack-list-rename-name',disabled:false,focus(){calls.push('name');focus(name);}};
 const button={...shown,id:'slack-list-new-rating-emoji',dataset:{fieldChoice:'11'},disabled:false,focus(){calls.push('emoji');focus(button);}};
 const root={...shown,dataset:{renameSerial:'7'},inert:true,
  closest:s=>s==='.slack-list-dialog'?dialog:null,contains:n=>n===name||n===button,
  querySelector:s=>s==='#slack-list-new-rating-emoji'?button:s==='input:not([disabled])'?name:null,
  addEventListener(){},
  setAttribute(k,v){assert.equal(k,'data-rating-return');this.dataset.ratingReturn=v;notify();},
  removeAttribute(k){assert.equal(k,'data-rating-return');delete this.dataset.ratingReturn;notify();}};
 Object.assign(document,{activeElement:close,
  getElementById:id=>id==='slack-list-rename'?root:id==='slack-list-rating-picker'&&currentPicker?picker:null,
  querySelector:()=>currentPicker?picker:null,
  querySelectorAll:()=>currentPicker?[dialog,root,picker]:[dialog,root],
  addEventListener:(type,f)=>{assert.equal(type,'focusin');listeners.add(f);},
  removeEventListener:(type,f)=>listeners.delete(f)});
 const context=vm.createContext({document,performance:{now:()=>0},getComputedStyle:()=>({visibility:'visible'}),
  requestAnimationFrame:f=>{frames.set(++sequence,f);return sequence;},cancelAnimationFrame:id=>frames.delete(id),
  setTimeout:f=>{timers.set(++sequence,f);return sequence;},clearTimeout:id=>timers.delete(id),
  MutationObserver:class{constructor(callback){this.callback=callback;}observe(){if(root.__ratingEmojiReturn)observe?.();observers.add(this);}disconnect(){observers.delete(this);}},
  dioxus:{send:v=>{sent.push(v);send?.({root,document});},recv:()=>recv?recv():new Promise(r=>{receive=r;})}});
 vm.runInContext(code,context);vm.runInContext(renameCode,context);
 const begin=()=>vm.runInContext('restoreRatingEmoji(7,9,11,13)',context);
 const promise=begin();
 const frame=reverse=>{const all=[...frames.values()];frames.clear();if(reverse)all.reverse();for(const f of all)f();};
 return{root,button,name,document,sent,calls,promise,begin,frame,listeners,frames,timers,
  async commit(revision=11){receive(revision);await new Promise(setImmediate);},
  remove(){currentPicker=false;picker.isConnected=false;root.inert=false;document.activeElement=body;notify();},
  intentName(){focus(name);},
  timeout(){for(const f of [...timers.values()])f();},
  settle(){frame(false);assert.equal(root.__ratingEmojiReturn,undefined);assert.equal(root.dataset.ratingReturn,undefined);assert.equal(listeners.size,0);assert.equal(timers.size,0);},
 };
}
for(const beforeCommit of [false,true])for(const reverse of [false,true])await test(`actual generic recovery waits for exact return (removedBeforeCommit=${beforeCommit},reverseFrames=${reverse})`,async()=>{
 const h=coordinated();assert.deepEqual(h.sent,[true]);
 if(beforeCommit){h.remove();h.frame(reverse);assert.deepEqual(h.calls,[]);}
 await h.commit();if(!beforeCommit)h.remove();h.frame(reverse);await h.promise;h.settle();
 assert.equal(h.document.activeElement,h.button);assert.deepEqual(h.calls,['emoji']);
});
await test('actual generic recovery also preserves the new chosen revision return',async()=>{
 const h=coordinated();await h.commit(12);h.button.dataset.fieldChoice='12';h.remove();h.frame(false);await h.promise;h.settle();assert.deepEqual(h.calls,['emoji']);
});
await test('explicit user focus inside editor cancels return without being overwritten',async()=>{
 const h=coordinated();await h.commit();h.remove();h.intentName();h.frame(false);await h.promise;h.settle();assert.equal(h.document.activeElement,h.name);assert.deepEqual(h.calls,[]);
});
await test('failed disposition releases bounded ownership and ordinary recovery resumes',async()=>{
 const h=coordinated();h.remove();h.frame(false);await h.commit(null);await h.promise;h.settle();assert.equal(h.document.activeElement,h.name);assert.deepEqual(h.calls,['name']);
});
await test('abandoned snapshot releases ownership and does not suppress later ordinary recovery',async()=>{
 const h=coordinated();h.remove();h.frame(false);h.timeout();await h.promise;h.settle();assert.deepEqual(h.calls,['name']);
});
console.log('8 combined actual product-helper cases passed; native verification remains required');


await test('synchronous IPC send exception releases custody and preserves the primary error',async()=>{
 const error=new Error('IPC send refused'),h=coordinated({send(){throw error;}});
 await assert.rejects(h.promise,e=>e===error);h.remove();h.settle();
 assert.deepEqual(h.calls,['name']);assert.equal(h.document.activeElement,h.name);
});
await test('synchronous receive setup exception releases custody without return focus',async()=>{
 const error=new Error('IPC receive setup refused'),h=coordinated({recv(){throw error;}});
 await assert.rejects(h.promise,e=>e===error);h.remove();h.settle();assert.deepEqual(h.calls,['name']);
});
await test('observation setup exception clears its timer and marker with exact error identity',async()=>{
 const error=new Error('observe refused'),h=coordinated({observe(){throw error;}});
 const rejected=assert.rejects(h.promise,e=>e===error);await h.commit();await rejected;
 h.remove();h.settle();assert.deepEqual(h.calls,['name']);
});
await test('failed older bridge never deletes a newer return owner',async()=>{
 const newer={},error=new Error('old IPC refused');
 const h=coordinated({send({root}){root.__ratingEmojiReturn=newer;root.setAttribute('data-rating-return','newer');throw error;}});
 await assert.rejects(h.promise,e=>e===error);h.frame(false);
 assert.equal(h.root.__ratingEmojiReturn,newer);assert.equal(h.root.dataset.ratingReturn,'newer');
 assert.equal(h.listeners.size,0);assert.equal(h.timers.size,0);assert.deepEqual(h.calls,[]);
});
console.log('4 exception cleanup cases passed; original errors and newer-owner custody preserved');
