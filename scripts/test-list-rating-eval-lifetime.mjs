// Actual Rust eval invocation + pinned Dioxus desktop channel/wrapper, mocked DOM.
// This verifies IPC lifetime, not browser layout or native focus acceptance.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import vm from 'node:vm';
const base=process.argv[2]||'.', fallback=process.argv[3]||base;
const read=p=>fs.readFileSync(path.join(fs.existsSync(path.join(base,p))?base:fallback,p),'utf8');
const pinned=process.argv[4]||path.join(os.homedir(),'.cargo/git/checkouts/dioxus-d8abd2ecf6e8b5b4/57d6794');
const query=fs.readFileSync(path.join(pinned,'packages/desktop/src/query.rs'),'utf8');
const native=fs.readFileSync(path.join(pinned,'packages/desktop/src/js/native_eval.js'),'utf8');
const controller=read('src/desktop/slack_lists/rename/text_create/rating.rs');
const helper=read('src/desktop/slack_lists/rename/text_create/rating_restore.js');
const recovery=read('src/desktop/view/slack_lists/rename_focus.js');
const call=controller.match(/"\{\}\\n([^"\n]*restoreRatingEmoji\([^"\n]+)"/)[1]
 .replace('{serial}','7').replace('{visit}','9').replace('{}','11').replace('{token}','13');
// Extract the actual Rust format template, preserving its completion/close order.
const template=query.match(/r#"(\(function\(\)\{\{[\s\S]+?\}\}\)\(\);)"#/)[1];
const wrapper=template.replaceAll('{{','{').replaceAll('}}','}')
 .replaceAll('{request_id}','1').replace('{script:?}',JSON.stringify(helper+'\n'+call));
assert.match(query,/window\.getQuery\(\{queue_id\}\)\.rustSend\(\{data\}\)/);
const tick=()=>new Promise(setImmediate);
function harness(){
 const frames=new Map(),timers=new Map(),observers=new Set(),listeners=new Set(),events=[],calls=[];
 let sequence=0,currentPicker=true;
 const notify=()=>{for(const o of [...observers])o.callback();};
 const shown={isConnected:true,getClientRects:()=>[{}],getBoundingClientRect:()=>({width:100}),closest:()=>null};
 const body={},document={body};
 const focus=n=>{document.activeElement=n;for(const f of [...listeners])f({target:n});};
 const close={...shown,id:'picker-close'},picker={...shown,dataset:{ratingPicker:'13'},contains:n=>n===close};
 const dialog={...shown,dataset:{dialogGeneration:'9'}};
 const name={...shown,id:'slack-list-rename-name',focus(){calls.push('name');focus(name);}};
 const button={...shown,id:'slack-list-new-rating-emoji',disabled:false,dataset:{fieldChoice:'11'},focus(){calls.push('emoji');focus(button);}};
 const root={...shown,dataset:{renameSerial:'7'},inert:true,
  closest:s=>s==='.slack-list-dialog'?dialog:null,contains:n=>n===name||n===button,
  querySelector:s=>s==='#slack-list-new-rating-emoji'?button:s==='input:not([disabled])'?name:null,
  addEventListener(){},setAttribute(k,v){this.dataset.ratingReturn=v;notify();},removeAttribute(){delete this.dataset.ratingReturn;notify();}};
 Object.assign(document,{activeElement:close,
  getElementById:id=>id==='slack-list-rename'?root:id==='slack-list-rating-picker'&&currentPicker?picker:null,
  querySelector:()=>currentPicker?picker:null,querySelectorAll:()=>currentPicker?[dialog,root,picker]:[dialog,root],
  addEventListener:(t,f)=>listeners.add(f),removeEventListener:(t,f)=>listeners.delete(f)});
 const window={ipc:{postMessage:s=>events.push(JSON.parse(s))}};
 const context=vm.createContext({window,document,performance:{now:()=>0},getComputedStyle:()=>({visibility:'visible'}),
  requestAnimationFrame:f=>{frames.set(++sequence,f);return sequence;},cancelAnimationFrame:id=>frames.delete(id),
  setTimeout:f=>{timers.set(++sequence,f);return sequence;},clearTimeout:id=>timers.delete(id),
  MutationObserver:class{constructor(callback){this.callback=callback;}observe(){observers.add(this);}disconnect(){observers.delete(this);}}});
 vm.runInContext(native.replace(/export\{NativeDioxusChannel\};?\s*$/,''),context);
 vm.runInContext(recovery,context);vm.runInContext(wrapper,context);
 return {root,dialog,button,name,document,window,events,calls,timers,listeners,
  frame(){const q=[...frames.values()];frames.clear();q.forEach(f=>f());},
  remove(){currentPicker=false;picker.isConnected=false;root.inert=false;document.activeElement=body;notify();},
  async deliver(revision){vm.runInContext(`window.getQuery(1).rustSend(${JSON.stringify(revision)});`,context);await tick();},
  intent(){focus(name);},expire(){for(const f of [...timers.values()])f();}};
}
let passed=0;
async function test(name,f){await f();passed++;console.log('PASS',name);}
for(const revision of [11,12])for(const early of [true,false])await test(`actual eval channel survives disposition revision=${revision}, removalBeforeReply=${early}`,async()=>{
 const h=harness();await tick();assert.equal(h.events[0].params.data.data,true);
 assert.ok(h.window.getQuery(1),'outer eval must keep the actual desktop channel registered');
 if(early){h.remove();h.frame();assert.deepEqual(h.calls,[]);}
 h.button.dataset.fieldChoice=String(revision);await h.deliver(revision);
 if(!early)h.remove();h.frame();await tick();h.frame();
 assert.equal(h.document.activeElement,h.button);assert.deepEqual(h.calls,['emoji']);
 assert.equal(h.window.getQuery(1),null);assert.equal(h.listeners.size,0);assert.equal(h.timers.size,0);
 assert.equal(h.events.at(-1).params.data.method,'return');
});
for(const mode of ['deliberate-focus','stale-visit','rejected-disposition','lost-reply'])await test(`actual eval closes without stealing on ${mode}`,async()=>{
 const h=harness();await tick();assert.ok(h.window.getQuery(1));h.remove();
 if(mode==='deliberate-focus')h.intent();
 if(mode==='stale-visit')h.dialog.dataset.dialogGeneration='10';
 if(mode==='lost-reply')h.expire();else await h.deliver(mode==='rejected-disposition'?null:11);
 h.frame();await tick();h.frame();assert.ok(!h.calls.includes('emoji'));
 assert.equal(h.window.getQuery(1),null);assert.equal(h.root.__ratingEmojiReturn,undefined);
 assert.equal(h.listeners.size,0);assert.equal(h.timers.size,0);
});
console.log(`${passed} actual pinned desktop eval lifecycle cases passed; no native focus claim`);
