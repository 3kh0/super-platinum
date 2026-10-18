import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import crypto from 'node:crypto';

const url=new URL('../src/desktop/message_buttons/workflow_logs/native.js',import.meta.url);
const source=fs.readFileSync(url,'utf8');
const names=[];
const flush=async()=>{for(let i=0;i<8;i++)await Promise.resolve();};
async function test(name,body){await body();names.push(name);console.log(`PASS ${name}`);}
function scene(){
  const events=new Map(),observers=[],timers=new Map(),frames=new Map(),ids=new Map();let serial=0,release,task;
  const node=(id)=>{let value='draft 中🙂',readOnly=false;const n={id,isConnected:true,disabled:false,fieldsetDisabled:false,
    dataset:{},selectionStart:2,selectionEnd:4,rect:true,visibility:'visible',textContent:'',
    matches:()=>n.disabled||n.fieldsetDisabled,getClientRects:()=>n.rect?[{}]:[],
    getBoundingClientRect:()=>({width:n.rect?60:0,height:n.rect?20:0}),
    setNative:v=>{value=v;}};
    Object.defineProperties(n,{value:{get:()=>value,set:()=>{throw Error('buffer write');}},readOnly:{get:()=>readOnly,set:()=>{throw Error('readonly write');}}});
    ids.set(id,n);return n;};
  const body={inert:false,addEventListener:(name,fn)=>{if(!events.has(name))events.set(name,new Set());events.get(name).add(fn);},removeEventListener:(name,fn)=>events.get(name)?.delete(fn)};
  const button=node('button');button.dataset.workflowLogsSource='exact';
  const notice=node('notice');notice.dataset.workflowLogsNotice='exact';
  const composer=node('channel-composer'),thread=node('thread-composer');
  const dom={buttons:[button],composing:false,modal:false};
  const document={body,documentElement:{},getElementById:id=>ids.get(id)??null,
    querySelectorAll:selector=>selector==='[data-workflow-logs-source]'?dom.buttons:[notice],
    querySelector:selector=>selector==='[data-composing="true"]'?dom.composing:dom.modal};
  class MutationObserver{constructor(fn){this.fn=fn;observers.push(this);}observe(){this.active=true;}disconnect(){this.active=false;}}
  const realm=vm.createContext({document,MutationObserver,getComputedStyle:n=>({visibility:n.visibility}),
    requestAnimationFrame:fn=>{const id=++serial;frames.set(id,fn);return id;},cancelAnimationFrame:id=>frames.delete(id),
    setTimeout:(fn,ms)=>{const id=++serial;timers.set(id,{fn,ms});return id;},clearTimeout:id=>timers.delete(id)});
  vm.runInContext(source,realm,{filename:url.pathname});
  const promise=new Promise(r=>release=r),sent=[];
  const s={body,button,notice,composer,thread,dom,ids,node,sent,
    start(){task=realm.workflowLogsEntry('exact',{send:v=>sent.push(v),recv:()=>promise});return task;},
    frame(){const pending=[...frames.values()];frames.clear();pending.forEach(fn=>fn());},
    expire(){for(const [id,{fn}]of [...timers]){timers.delete(id);fn();}},
    mutate(){observers.filter(o=>o.active).forEach(o=>o.fn([]));},
    event(name){for(const fn of events.get(name)??[])fn({type:name});},
    async finish(){release();await flush();s.frame();await task;s.clean();},
    clean(){assert.equal(observers.filter(o=>o.active).length,0);assert.equal([...events.values()].reduce((n,v)=>n+v.size,0),0);assert.equal(frames.size,0);assert.equal(timers.size,0);},
  };return s;
}
async function refused(change,late=false){const s=scene();if(!late)change(s);s.start();if(late){change(s);s.mutate();}s.frame();await flush();assert.deepEqual(s.sent,[false]);await s.finish();return s;}
await test('short lease keeps both uncommitted buffers and selections untouched until released',async()=>{
  const s=scene();s.start();assert.equal(s.body.inert,true);s.frame();await flush();assert.deepEqual(s.sent,[true]);
  assert.equal(s.composer.value,'draft 中🙂');assert.equal(s.thread.value,'draft 中🙂');assert.equal(s.composer.selectionStart,2);assert.equal(s.composer.selectionEnd,4);
  assert.equal(s.composer.readOnly,false);await s.finish();assert.equal(s.body.inert,false);
});
for(const [name,change] of Object.entries({
  duplicate:s=>{const n=s.node('duplicate');n.dataset.workflowLogsSource='exact';s.dom.buttons.push(n);},
  replacement:s=>{const n=s.node('replacement');n.dataset.workflowLogsSource='exact';s.dom.buttons=[n];},
  hidden:s=>{s.button.rect=false;},invisible:s=>{s.button.visibility='hidden';},
  disabled:s=>{s.button.disabled=true;},fieldset:s=>{s.button.fieldsetDisabled=true;},
  source:s=>{s.button.dataset.workflowLogsSource='other';},detached:s=>{s.button.isConnected=false;},
  modal:s=>{s.dom.modal=true;},composition:s=>{s.dom.composing=true;},
}))for(const late of [false,true]){
  if(name==='replacement'&&!late)continue; // The original node is captured at invocation.
  await test(`${name} ${late?'after lease':'before lease'} refuses`,async()=>{await refused(change,late);});
}
await test('native-ahead buffer after capture refuses without restoring old text',async()=>{
  const s=await refused(s=>s.composer.setNative('new text'),true);assert.equal(s.composer.value,'new text');
});
await test('same-id editor replacement refuses',async()=>{await refused(s=>s.node('thread-composer'),true);});
await test('composition event remains retired even when composition ends before acknowledgment',async()=>{
  const s=scene();s.start();s.event('compositionstart');s.frame();await flush();assert.deepEqual(s.sent,[false]);await s.finish();
});
await test('an existing inert lease is never released by refusal',async()=>{
  const s=await refused(s=>{s.body.inert=true;});assert.equal(s.body.inert,true);
});
await test('bounded frame fallback admits and cleans without animation frames',async()=>{
  const s=scene();s.start();s.expire();await flush();assert.deepEqual(s.sent,[true]);await s.finish();assert.equal(s.body.inert,false);
});
console.log(JSON.stringify({passed:names.length,failed:0,source:url.pathname,sha256:crypto.createHash('sha256').update(source).digest('hex'),tests:names}));
