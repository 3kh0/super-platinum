import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
const path=process.env.WORKFLOW_REFRESH_NATIVE_SOURCE || new URL('../src/desktop/app_home/about/management/activity/refresh/native.js',import.meta.url);
const source=fs.readFileSync(path,'utf8'),tests=[];
const flush=async()=>{for(let i=0;i<14;i++)await Promise.resolve();};
async function test(name,run){await run();tests.push(name);console.log('PASS '+name);}
function scene(){
  const expected={visit:1,document:2,documentRevision:3,detailRevision:4,memberGeneration:5,search:'Cafe\u0301',filters:{revision:6,status:'',from:'',to:'',limit:'100',user:''}};
  const map=new Map(),frames=new Map(),timers=new Map(),observers=[],events=[];let sequence=0;
  function node(id,parent=null,tagName='DIV'){
    const listeners=new Map();let value='';
    const n={id,parent,tagName,dataset:{},isConnected:true,inert:false,disabled:false,validity:{badInput:false},
      contains(other){for(let p=other;p;p=p.parent)if(p===this)return true;return false;},
      matches(selector){assert.equal(selector,':disabled');return this.disabled||!!this.fieldsetDisabled;},
      closest(selector){assert.equal(selector,'fieldset[disabled]');return this.fieldsetDisabled?{}:null;},
      addEventListener(e,f){if(!listeners.has(e))listeners.set(e,new Set());listeners.get(e).add(f);},
      removeEventListener(e,f){listeners.get(e)?.delete(f);},setNative:v=>{value=v;}};
    Object.defineProperty(n,'value',{get:()=>value,set:()=>{throw Error('Input write');}});
    map.set(id,n);events.push({n,listeners});return n;
  }
  const root=node('workflow-management');root.dataset={managementVisit:'1',managementDocument:'2',managementRevision:'3'};
  root.__workflowManagement={composing:false,lease:null};
  const activity=node('workflow-activity',root);activity.dataset.activityRead='0';
  const form=node('workflow-activity-filters',activity,'FORM');form.dataset.revision='6';
  const detail=node('workflow-activity-detail',activity);detail.dataset.detailRevision='4';
  const input=node('workflow-activity-user-search',form,'INPUT');input.dataset.generation='5';input.setNative(expected.search);
  const fields=Object.fromEntries(['status','from','to','limit','user'].map(k=>{const n=node('workflow-activity-'+k,form);n.setNative(expected.filters[k]);return[k,n];}));
  const dom={modal:null},sent=[],queue=[],receivers=[];
  const channel={send:v=>sent.push(v),recv:()=>queue.length?Promise.resolve(queue.shift()):new Promise(resolve=>receivers.push(resolve))};
  class MutationObserver{constructor(fn){this.fn=fn;this.active=false;observers.push(this);}observe(){this.active=true;}disconnect(){this.active=false;}}
  const realm=vm.createContext({document:{documentElement:{},getElementById:id=>map.get(id),querySelector:()=>dom.modal},MutationObserver,
    requestAnimationFrame:fn=>{const id=++sequence;frames.set(id,fn);return id;},cancelAnimationFrame:id=>frames.delete(id),
    setTimeout:(fn,ms)=>{const id=++sequence;timers.set(id,{fn,ms});return id;},clearTimeout:id=>timers.delete(id)});
  vm.runInContext(source,realm);let task;
  const s={expected,root,activity,form,detail,input,fields,dom,map,node,sent,channel,frames,timers,
    start(){task=realm.workflowActivityRefresh(expected,channel);return task;},
    command(value){if(receivers.length)receivers.shift()(value);else queue.push(value);},
    mutate(){for(const o of observers.filter(o=>o.active))o.fn();},
    event(e){for(const {listeners}of events)for(const f of [...(listeners.get(e)??[])])f({type:e});},
    frame(){const a=[...frames.values()];frames.clear();a.forEach(f=>f());},
    expire(ms){for(const[id,t]of [...timers])if(t.ms===ms){timers.delete(id);t.fn();}},
    async captured(){s.start();s.frame();await flush();assert.deepEqual(JSON.parse(JSON.stringify(sent)),[{serial:0,ok:true}]);assert.equal(root.inert,true);},
    async active(){await s.captured();activity.dataset.activityRead='77';input.disabled=true;s.mutate();s.command({kind:'activate',serial:1,read:77});await flush();assert.equal(sent.at(-1).ok,true);
      s.command({kind:'unlock',serial:0});await flush();s.frame();await flush();assert.equal(root.inert,false);},
    async lock(serial=2){s.command({kind:'lock',serial});await flush();s.frame();await flush();return sent.at(-1);},
    async cancel(){s.command(null);await flush();s.frame();await task;s.clean();},
    clean(){for(const n of [root,activity,form,detail])assert.equal(n.inert,false);assert.equal(root.__workflowManagement.lease,null);
      assert.equal(observers.filter(o=>o.active).length,0);assert.equal(frames.size,0);assert.equal(timers.size,0);
      assert.equal(events.reduce((a,{listeners})=>a+[...listeners.values()].reduce((b,set)=>b+set.size,0),0),0);},
    successor(){const replies=[],pending=[];let deliver;const bridge={send:v=>replies.push(v),recv:()=>pending.length?Promise.resolve(pending.shift()):new Promise(r=>{deliver=r;})};const work=realm.workflowActivityRefresh(expected,bridge);return{replies,work,cancel(){if(deliver){const f=deliver;deliver=null;f(null);}else pending.push(null);}};},
    result:()=>task};return s;
}
async function refused(change){const s=scene();change(s);s.start();await flush();assert.equal(s.sent[0].ok,false);await s.cancel();return s;}
await test('initial short lock activates owned read and releases all inert state during IO',async()=>{const s=scene();await s.active();assert.equal(s.input.disabled,true);assert.equal(s.input.value,s.expected.search);await s.cancel();});
await test('reusable locks protect multiple state writes without a second evaluator',async()=>{const s=scene();await s.active();for(const serial of [2,3,4]){assert.equal((await s.lock(serial)).ok,true);assert.equal(s.root.inert,true);s.command({kind:'unlock',serial});await flush();s.frame();await flush();assert.equal(s.root.inert,false);}await s.cancel();});
await test('queued lock awaits exact preceding unlock cleanup before acknowledgement',async()=>{const s=scene();await s.active();await s.lock(2);s.command({kind:'unlock',serial:2});s.command({kind:'lock',serial:3});await flush();assert.equal(s.root.inert,true);s.frame();await flush();assert.equal(s.root.inert,true);assert.equal(s.sent.at(-1).serial,2);s.frame();await flush();assert.equal(s.sent.at(-1).serial,3);await s.cancel();});
await test('activation waits for actual owned read marker under initial lock',async()=>{const s=scene();await s.captured();s.command({kind:'activate',serial:1,read:77});await flush();assert.equal(s.sent.length,1);s.activity.dataset.activityRead='77';s.input.disabled=true;s.mutate();await flush();assert.equal(s.sent.at(-1).ok,true);await s.cancel();});
await test('activation wrong marker times out rather than assuming a read',async()=>{const s=scene();await s.captured();s.command({kind:'activate',serial:1,read:77});await flush();s.expire(1000);await flush();assert.equal(s.sent.at(-1).ok,false);await s.cancel();});
for(const key of ['visit','document','documentRevision','detailRevision','memberGeneration'])await test('initial '+key+' mismatch refuses without input writes',()=>refused(s=>{s.expected[key]=99;}));
for(const key of ['status','from','to','limit','user'])await test('initial native-ahead '+key+' refuses',()=>refused(s=>s.fields[key].setNative('newer')));
await test('native-ahead search and IME modal badInput disabled refuse',async()=>{for(const change of [s=>s.input.setNative('newer'),s=>{s.root.__workflowManagement.composing=true;},s=>{s.dom.modal={};},s=>{s.input.validity.badInput=true;},s=>{s.input.disabled=true;},s=>{s.fields.from.fieldsetDisabled=true;}])await refused(change);});
await test('opaque date buffers are compared with no calendar validation',async()=>{const s=scene();s.expected.filters.from='opaque';s.fields.from.setNative('opaque');await s.active();await s.cancel();});
for(const target of ['root','activity','form','detail','input'])await test('original '+target+' replacement permanently retires custody',async()=>{const s=scene();await s.active();const old=s[target],replacement=s.node(old.id,old.parent,old.tagName);replacement.dataset={...old.dataset};replacement.setNative(old.value);s.mutate();assert.equal((await s.lock()).ok,false);assert.equal(replacement.inert,false);await s.cancel();});
await test('edited then restored buffer still retires original task',async()=>{const s=scene();await s.active();s.input.setNative('different');s.event('input');s.input.setNative(s.expected.search);assert.equal((await s.lock()).ok,false);await s.cancel();});
await test('IME and changed read identity during IO refuse final lock',async()=>{for(const change of [s=>{s.root.__workflowManagement.composing=true;s.event('compositionstart');},s=>{s.activity.dataset.activityRead='88';s.mutate();}]){const s=scene();await s.active();change(s);assert.equal((await s.lock()).ok,false);await s.cancel();}});
await test('only owned search disable is accepted; disabled filter refuses',async()=>{const s=scene();await s.active();s.fields.status.disabled=true;s.mutate();assert.equal((await s.lock()).ok,false);await s.cancel();});
await test('foreign live lease is never overwritten',async()=>{const s=scene();await s.active();const foreign={close(){}};s.root.__workflowManagement.lease=foreign;assert.equal((await s.lock()).ok,false);s.root.__workflowManagement.lease=null;await s.cancel();});
await test('wrong unlock serial does not release active hold',async()=>{const s=scene();await s.active();await s.lock(2);s.command({kind:'unlock',serial:99});await flush();assert.equal(s.root.inert,true);await s.cancel();});
await test('same held lock cannot be acquired twice',async()=>{const s=scene();await s.active();await s.lock(2);s.command({kind:'lock',serial:3});await flush();assert.equal(s.sent.at(-1).ok,false);assert.equal(s.root.inert,true);await s.cancel();});
await test('capture refusal remains alive until Rust receives and cancels',async()=>{const s=scene();s.expected.visit=99;let done=false;s.start().then(()=>{done=true;});await flush();assert.equal(done,false);assert.equal(s.sent[0].ok,false);await s.cancel();assert.equal(done,true);});
await test('dropped Guard during IO cleans evaluator observer without freezing controls',async()=>{const s=scene();await s.active();await s.cancel();});
await test('mount close callback cleans held original nodes after removal',async()=>{const s=scene();await s.active();await s.lock();s.root.isConnected=false;s.root.__workflowManagement.lease.close();await flush();s.frame();await s.result();s.clean();});
await test('release fallback is bounded when rendering is backgrounded',async()=>{const s=scene();await s.active();await s.lock();s.command({kind:'unlock',serial:2});await flush();s.expire(200);await flush();assert.equal(s.root.inert,false);await s.cancel();});
await test('final commit may retire read before release without touching controlled buffers',async()=>{const s=scene();await s.active();await s.lock();s.activity.dataset.activityRead='0';s.input.disabled=false;s.mutate();assert.equal(s.root.inert,true);s.command({kind:'unlock',serial:2});await flush();s.frame();await flush();assert.equal(s.input.disabled,false);assert.equal(s.input.value,s.expected.search);await s.cancel();});
await test('disabled ancestor cannot masquerade as owned search disable',async()=>{const s=scene();await s.active();s.input.fieldsetDisabled=true;s.mutate();assert.equal((await s.lock()).ok,false);await s.cancel();});
await test('initial frame timeout refuses and cleans held properties',async()=>{const s=scene();s.start();s.expire(1000);await flush();s.expire(200);await flush();assert.equal(s.sent[0].ok,false);await s.cancel();});
await test('receive failure cleans held properties without a Rust release command',async()=>{const s=scene();s.channel.recv=()=>Promise.reject(Error('closed'));s.start();s.frame();await flush();s.frame();await assert.rejects(s.result(),/closed/);s.clean();});
await test('send failure cleans initial held nodes',async()=>{const s=scene();s.channel.send=()=>{throw Error('closed');};s.start();s.frame();await flush();s.frame();await assert.rejects(s.result(),/closed/);s.clean();});
await test('cancel arriving during queued lock releases after its acknowledgment',async()=>{const s=scene();await s.active();s.command({kind:'lock',serial:2});await flush();s.command(null);s.frame();await flush();s.frame();await s.result();s.clean();});
await test('composition capture retires before management listener updates its state',async()=>{const s=scene();await s.active();s.event('compositionstart');assert.equal(s.root.__workflowManagement.composing,false);assert.equal((await s.lock()).ok,false);await s.cancel();});
await test('terminal close waits delayed paint before successor capture can acquire original nodes',async()=>{
  const s=scene();await s.active();await s.lock(2);
  s.activity.dataset.activityRead='0';s.input.disabled=false;s.mutate();
  const before=s.sent.length;s.command({kind:'close',serial:3,lease:2});await flush();
  assert.equal(s.sent.length,before,'No cleanup acknowledgment before queued paint');
  assert.equal(s.root.inert,true);assert.ok(s.root.__workflowManagement.lease);
  s.frame();await flush();assert.deepEqual(JSON.parse(JSON.stringify(s.sent.at(-1))),{serial:3,ok:true});
  s.clean();
  const next=s.successor();s.frame();await flush();assert.equal(next.replies[0].ok,true);
  assert.equal(s.root.inert,true);
  // Old evaluator shutdown must not clear the successor's newly owned hold.
  s.command(null);await s.result();assert.equal(s.root.inert,true);
  next.cancel();await flush();s.frame();await next.work;s.clean();
});
await test('terminal close can acknowledge bounded background paint fallback',async()=>{
  const s=scene();await s.active();await s.lock(2);s.activity.dataset.activityRead='0';s.input.disabled=false;s.mutate();
  s.command({kind:'close',serial:3,lease:2});await flush();assert.equal(s.sent.at(-1).serial,2);
  s.expire(200);await flush();assert.equal(s.sent.at(-1).serial,3);assert.equal(s.sent.at(-1).ok,true);await s.cancel();
});
await test('terminal close never acknowledges foreign lease or wrong held serial',async()=>{
  for(const wrongLease of [false,true]){const s=scene();await s.active();await s.lock(2);
    s.activity.dataset.activityRead='0';s.input.disabled=false;s.mutate();
    if(wrongLease)s.root.__workflowManagement.lease={foreign:true};
    s.command({kind:'close',serial:3,lease:wrongLease?2:99});await flush();assert.equal(s.sent.at(-1).ok,false);assert.equal(s.root.inert,true);
    if(wrongLease)s.root.__workflowManagement.lease=null;
    // A foreign lease is untouched; restore original fixture state solely for cleanup.
    if(wrongLease){for(const n of [s.root,s.activity,s.form,s.detail])n.inert=false;}
    await s.cancel();
  }
});
await test('terminal close rejects changed or edited-then-restored original inputs',async()=>{
  for(const restore of [false,true]){const s=scene();await s.active();await s.lock(2);
    s.input.setNative('changed');s.event('input');if(restore)s.input.setNative(s.expected.search);
    s.activity.dataset.activityRead='0';s.input.disabled=false;s.mutate();
    s.command({kind:'close',serial:3,lease:2});await flush();s.frame();await flush();assert.equal(s.sent.at(-1).ok,false);await s.cancel();
  }
});
await test('terminal close rejects retired read mismatch and IME despite later restoration',async()=>{
  for(const change of [s=>{s.activity.dataset.activityRead='88';s.mutate();},s=>s.event('compositionstart')]){
    const s=scene();await s.active();await s.lock(2);change(s);s.activity.dataset.activityRead='0';s.input.disabled=false;s.mutate();
    s.command({kind:'close',serial:3,lease:2});await flush();s.frame();await flush();assert.equal(s.sent.at(-1).ok,false);await s.cancel();
  }
});
await test('terminal close validates identity again after delayed paint',async()=>{
  const s=scene();await s.active();await s.lock(2);s.activity.dataset.activityRead='0';s.input.disabled=false;s.mutate();
  s.command({kind:'close',serial:3,lease:2});await flush();s.input.setNative('newer');s.mutate();s.frame();await flush();assert.equal(s.sent.at(-1).ok,false);assert.equal(s.input.value,'newer');await s.cancel();
});
await test('cancel during terminal close still restores exact held nodes and releases evaluator',async()=>{
  const s=scene();await s.active();await s.lock(2);s.activity.dataset.activityRead='0';s.input.disabled=false;s.mutate();
  s.command({kind:'close',serial:3,lease:2});await flush();s.command(null);s.frame();await s.result();s.clean();
});
await test('replacement during terminal close refuses and never changes successor nodes',async()=>{
  const s=scene();await s.active();await s.lock(2);s.activity.dataset.activityRead='0';s.input.disabled=false;s.mutate();
  s.command({kind:'close',serial:3,lease:2});await flush();const next=s.node('workflow-activity-detail',s.activity);next.dataset={...s.detail.dataset};s.mutate();
  s.frame();await flush();assert.equal(s.sent.at(-1).ok,false);assert.equal(next.inert,false);await s.cancel();
});
await test('terminal close waits for delayed DOM read retirement rather than the first frame',async()=>{
  const s=scene();await s.active();await s.lock(2);const before=s.sent.length;
  s.command({kind:'close',serial:3,lease:2});await flush();s.frame();await flush();
  assert.equal(s.sent.length,before);assert.equal(s.root.inert,true);assert.ok(s.root.__workflowManagement.lease);
  // The controlled read marker and search disabled property may commit separately.
  s.activity.dataset.activityRead='0';s.mutate();await flush();s.frame();await flush();
  assert.equal(s.sent.length,before);assert.equal(s.root.inert,true);
  s.input.disabled=false;s.mutate();await flush();assert.equal(s.sent.length,before);
  s.frame();await flush();assert.equal(s.sent.at(-1).serial,3);assert.equal(s.sent.at(-1).ok,true);
  await s.cancel();
});
await test('terminal close accepts search enabled before the owned read marker retires',async()=>{
  const s=scene();await s.active();await s.lock(2);const before=s.sent.length;
  s.command({kind:'close',serial:3,lease:2});await flush();s.input.disabled=false;s.mutate();await flush();
  assert.equal(s.sent.length,before);assert.equal(s.root.inert,true);
  s.activity.dataset.activityRead='0';s.mutate();await flush();s.frame();await flush();assert.equal(s.sent.at(-1).ok,true);await s.cancel();
});
await test('terminal close times out if exact read retirement never commits',async()=>{
  for(const marker of ['77','0']){const s=scene();await s.active();await s.lock(2);
    s.command({kind:'close',serial:3,lease:2});await flush();s.activity.dataset.activityRead=marker;s.mutate();
    s.expire(1000);await flush();s.frame();await flush();assert.equal(s.sent.at(-1).serial,3);assert.equal(s.sent.at(-1).ok,false);
    assert.equal(s.root.inert,false);assert.equal(s.input.disabled,true);await s.cancel();
  }
});
await test('terminal close rejects foreign read during pending DOM retirement',async()=>{
  const s=scene();await s.active();await s.lock(2);s.command({kind:'close',serial:3,lease:2});await flush();
  s.activity.dataset.activityRead='88';s.mutate();await flush();s.frame();await flush();assert.equal(s.sent.at(-1).ok,false);
  assert.equal(s.activity.dataset.activityRead,'88');await s.cancel();
});
await test('cancellation while awaiting DOM retirement releases without successor admission',async()=>{
  const s=scene();await s.active();await s.lock(2);s.command({kind:'close',serial:3,lease:2});await flush();
  s.command(null);await flush();await s.result();assert.equal(s.sent.at(-1).ok,false);s.clean();
});
await test('closed bridge while awaiting DOM retirement cancels and cleans its held lease',async()=>{
  const s=scene();await s.active();await s.lock(2);s.channel.recv=()=>Promise.reject(Error('closed'));
  s.command({kind:'close',serial:3,lease:2});await flush();await s.result();assert.equal(s.sent.at(-1).ok,false);s.clean();
});
console.log(JSON.stringify({ok:true,count:tests.length,tests,sourceSha256:crypto.createHash('sha256').update(source).digest('hex')}));
