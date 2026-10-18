import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import crypto from 'node:crypto';

const url = new URL('../src/desktop/app_home/about/management/activity/message/navigation/native.js', import.meta.url);
const source = fs.readFileSync(url,'utf8');
const names=[];
async function test(name,body) {await body();names.push(name);console.log(`PASS ${name}`);}
const flush=async()=>{for(let i=0;i<8;i++)await Promise.resolve();};
function deferred(){let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return {promise,resolve,reject};}
function scene() {
  const expected={chain:707,visit:101,document:202,revision:303,detail:404,generation:505,search:'  Cafe\u0301  ',button:'workflow-channel-0',channel:'CACTTEST1',
    filters:{revision:606,status:'',from:'',to:'',limit:'100',user:''}};
  const byId=new Map(),events=[],observers=[],timers=new Map(),frames=new Map();let serial=0;
  function node(id,parent=null,tagName='DIV') {
    const listeners=new Map();
    const n={id,parent,tagName,isConnected:true,dataset:{},inert:false,disabled:false,validity:{badInput:false},textContent:'',
      contains(other){for(let at=other;at;at=at.parent)if(at===this)return true;return false;},
      closest(selector){assert.equal(selector,'#workflow-activity');for(let at=this;at;at=at.parent)if(at.id==='workflow-activity')return at;return null;},
      matches(selector){assert.equal(selector,':disabled');return !!this.disabled || !!this.fieldsetDisabled;},
      addEventListener(event,fn){if(!listeners.has(event))listeners.set(event,new Set());listeners.get(event).add(fn);},
      removeEventListener(event,fn){listeners.get(event)?.delete(fn);}};
    let value='';Object.defineProperties(n,{
      value:{get:()=>value,set:()=>{throw Error('Guard wrote input buffer');}},
      readOnly:{get:()=>false,set:()=>{throw Error('Guard wrote readonly');}},
    });n.setNative=v=>{value=v;};
    events.push({n,listeners});byId.set(id,n);return n;
  }
  const root=node('workflow-management');root.dataset={managementVisit:'101',managementDocument:'202',managementRevision:'303'};
  root.__workflowManagement={composing:false,lease:null};
  const activity=node('workflow-activity',root);
  const content=node('workflow-activity-content',activity);
  const form=node('workflow-activity-filters',content,'FORM');form.dataset.revision='606';
  const detail=node('workflow-activity-detail',content);detail.dataset.detailRevision='404';
  const button=node(expected.button,detail,'BUTTON');button.dataset.channel=expected.channel;
  const notice=node('workflow-activity-navigation-notice',activity);notice.dataset.detailRevision='404';
  const input=node('workflow-activity-user-search',form,'INPUT');input.dataset.generation='505';input.setNative(expected.search);
  const fields=Object.fromEntries(Object.keys(expected.filters).filter(k=>k!=='revision').map(k=>{
    const n=node('workflow-activity-'+k,form,['from','to'].includes(k)?'INPUT':'SELECT');n.setNative(expected.filters[k]);return [k,n];
  }));
  const dom={modal:null};
  const document={documentElement:{},getElementById(id){
    if(['workflow-management-title','workflow-management-description'].includes(id))throw Error('Guard inspected hidden metadata');
    return byId.get(id)??null;},querySelector(selector){assert.equal(selector,'[role="dialog"][aria-modal="true"]');return dom.modal;}};
  class MutationObserver {constructor(callback){this.callback=callback;this.active=false;observers.push(this);}observe(){this.active=true;}disconnect(){this.active=false;}}
  const realm=vm.createContext({document,MutationObserver,
    requestAnimationFrame:fn=>{const id=++serial;frames.set(id,fn);return id;},cancelAnimationFrame:id=>frames.delete(id),
    setTimeout:(fn,ms)=>{const id=++serial;timers.set(id,{fn,ms});return id;},clearTimeout:id=>timers.delete(id)});
  vm.runInContext(source,realm,{filename:url.pathname});
  const release=deferred(),sent=[];let task;
  const channel={send:v=>sent.push(v),recv:()=>release.promise};
  const s={expected,root,activity,content,form,detail,button,notice,input,fields,dom,byId,node,channel,release,sent,timers,frames,
    feedback:(message,expected=s.expected)=>realm.workflowActivityChannelFeedback(expected,message),
    lock:(expected,channel)=>realm.workflowActivityChannelLock(expected,channel),
    start(){task=realm.workflowActivityChannelLock(expected,channel);return task;},
    frame(){const list=[...frames];frames.clear();for(const [,fn]of list)fn();},
    expire(ms){for(const [id,t]of [...timers])if(t.ms===ms){timers.delete(id);t.fn();}},
    mutate(){for(const o of observers.filter(o=>o.active))o.callback([]);},
    event(name){for(const e of events)for(const fn of [...(e.listeners.get(name)??[])])fn({type:name});},
    async acknowledge(){s.start();assert.equal(root.inert,true);assert.deepEqual(sent,[]);s.frame();await flush();assert.deepEqual(sent,[true]);},
    async finish(){release.resolve();await flush();s.frame();const result=await task;s.clean();return result;},
    clean(){assert.equal(root.inert,false);assert.equal(form.inert,false);assert.equal(detail.inert,false);assert.equal(root.__workflowManagement.lease,null);
      assert.equal(observers.filter(o=>o.active).length,0);assert.equal(timers.size,0);assert.equal(frames.size,0);
      assert.equal(events.reduce((n,e)=>n+[...e.listeners.values()].reduce((m,set)=>m+set.size,0),0),0);},
    result:()=>task,
  };return s;
}
async function refused(change,feedback=true){const s=scene();change(s);assert.equal(await s.start(),false);assert.deepEqual(s.sent,[false]);
  assert.equal(s.notice.textContent,feedback?'Finish editing the current text before opening this conversation. Your draft is unchanged.':'');
  assert.equal(s.root.__workflowManagement?.channelRefusals,feedback?1:undefined);return s;}

await test('locks exact native controls before frame and retains until explicit release',async()=>{
  const s=scene();await s.acknowledge();assert.equal(s.form.inert,true);assert.equal(s.detail.inert,true);
  assert.equal(s.input.value,s.expected.search);assert.equal(s.input.disabled,false);assert.equal(s.input.readOnly,false);
  assert.equal(s.timers.size,0);s.frame();await flush();assert.equal(s.root.inert,true);assert.equal(await s.finish(),true);
});
await test('unchanged document revision still restores captured inert properties',async()=>{const s=scene();await s.acknowledge();await s.finish();assert.equal(s.root.dataset.managementRevision,'303');});
await test('release remains locked until controlled commit render opportunity',async()=>{const s=scene();await s.acknowledge();s.release.resolve();await flush();assert.equal(s.root.inert,true);s.frame();await s.result();s.clean();});
await test('release cleanup has bounded fallback only after Rust released',async()=>{const s=scene();await s.acknowledge();s.expire(2000);assert.equal(s.root.inert,true);s.release.resolve();await flush();s.expire(200);await s.result();s.clean();});
await test('native-ahead search refuses without changing exact Unicode buffer',async()=>{const s=await refused(s=>s.input.setNative('Cafe\u0301 newer'));assert.equal(s.input.value,'Cafe\u0301 newer');});
for(const key of ['status','from','to','limit','user'])await test(`native-ahead ${key} refuses without buffer restoration`,async()=>{const s=await refused(s=>s.fields[key].setNative('native-ahead'));assert.equal(s.fields[key].value,'native-ahead');});
await test('exact empty and opaque date buffers are compared without date validation',async()=>{const s=scene();s.expected.filters.from='uninterpreted';s.fields.from.setNative('uninterpreted');await s.acknowledge();await s.finish();});
await test('IME composition refuses with scoped counter',async()=>{await refused(s=>{s.root.__workflowManagement.composing=true;});});
await test('fixed sibling feedback outside detail scroll area receives current refusal',async()=>{
  const s=await refused(s=>{s.root.__workflowManagement.composing=true;});
  assert.equal(s.detail.contains(s.notice),false);assert.equal(s.content.contains(s.notice),false);
  assert.equal(s.activity.contains(s.notice),true);
});
await test('wrong or missing feedback detail revision cannot receive refusal',async()=>{
  for(const revision of ['999',undefined])await refused(s=>{
    s.root.__workflowManagement.composing=true;s.notice.dataset.detailRevision=revision;
  },false);
});
await test('foreign Activity sibling feedback cannot receive refusal',async()=>{
  await refused(s=>{s.root.__workflowManagement.composing=true;s.notice.parent=s.node('workflow-activity',s.root);},false);
});
await test('feedback outside captured root cannot receive refusal',async()=>{
  await refused(s=>{s.root.__workflowManagement.composing=true;s.notice.parent=s.node('workflow-activity');},false);
});
await test('moved original detail cannot transfer refusal to another Activity',async()=>{
  const s=scene();s.start();const foreign=s.node('workflow-activity',s.root);s.detail.parent=foreign;s.notice.parent=foreign;
  s.input.setNative('newer');s.event('input');s.frame();assert.equal(await s.result(),false);
  assert.equal(s.notice.textContent,'');assert.equal(s.root.__workflowManagement.channelRefusals,undefined);s.clean();
});
await test('modal refuses with scoped notice',async()=>{await refused(s=>{s.dom.modal={};});});
for(const key of ['search','status','from','to','limit','user','button'])await test(`disabled and badInput ${key} refuse`,async()=>{
  const get=s=>key==='search'?s.input:key==='button'?s.button:s.fields[key];
  await refused(s=>{get(s).disabled=true;});await refused(s=>{get(s).validity.badInput=true;});
});
await test('ancestor fieldset disabled state refuses',async()=>{await refused(s=>{s.fields.from.fieldsetDisabled=true;});});
await test('foreign channel and missing button refuse',async()=>{await refused(s=>{s.button.dataset.channel='COTHER001';});await refused(s=>{s.byId.delete(s.expected.button);});});
await test('wrong root identity never writes refusal into replacement owner',async()=>{
  for(const key of ['managementVisit','managementDocument','managementRevision'])await refused(s=>{s.root.dataset[key]='999';},false);
});
await test('wrong detail revision never writes refusal into later detail',async()=>{await refused(s=>{s.detail.dataset.detailRevision='999';},false);});
await test('wrong filter revision or search generation refuses',async()=>{await refused(s=>{s.form.dataset.revision='999';});await refused(s=>{s.input.dataset.generation='999';});});
await test('disconnected or wrong containment refuses',async()=>{
  await refused(s=>{s.root.isConnected=false;},false);await refused(s=>{s.detail.isConnected=false;},false);
  await refused(s=>{s.form.parent=null;});await refused(s=>{s.input.parent=s.root;});await refused(s=>{s.button.parent=s.root;});
});
await test('preexisting lease and inert owner are never overwritten',async()=>{
  const s=scene(),lease={close(){}};s.root.__workflowManagement.lease=lease;assert.equal(await s.start(),false);assert.equal(s.root.__workflowManagement.lease,lease);
  await refused(s=>{s.form.inert=true;});
});
await test('replaced form between inspection and ack refuses and cleans originals',async()=>{
  const s=scene();s.start();const replacement=s.node(s.form.id,s.root,'FORM');replacement.dataset={...s.form.dataset};s.mutate();s.frame();assert.equal(await s.result(),false);s.clean();assert.equal(replacement.inert,false);
});
await test('replaced detail between inspection and ack does not receive old notice',async()=>{
  const s=scene();s.start();const replacement=s.node(s.detail.id,s.root);replacement.dataset={...s.detail.dataset};s.mutate();s.frame();assert.equal(await s.result(),false);s.clean();assert.equal(s.notice.textContent,'');assert.equal(replacement.inert,false);
});
await test('same-ID input replacement before ack cannot retain old buffer authority',async()=>{
  const s=scene();s.start();const replacement=s.node(s.input.id,s.form,'INPUT');replacement.dataset={...s.input.dataset};replacement.setNative(s.expected.search);s.mutate();s.frame();assert.equal(await s.result(),false);s.clean();
});
await test('native input event between lock and ack refuses',async()=>{const s=scene();s.start();s.input.setNative('programmatic newer');s.event('input');s.frame();assert.equal(await s.result(),false);s.clean();assert.equal(s.input.value,'programmatic newer');});
await test('missing animation frame times out before ack and unlocks',async()=>{const s=scene();s.start();s.expire(2000);assert.equal(await s.result(),false);assert.deepEqual(s.sent,[false]);s.clean();});
await test('Rust cancellation before ack releases without true acknowledgment',async()=>{const s=scene();s.start();s.release.resolve();assert.equal(await s.result(),false);assert.deepEqual(s.sent,[false]);s.clean();});
await test('synchronous send failure cleans lease and native properties',async()=>{const s=scene();s.channel.send=()=>{throw Error('closed send');};s.start();s.frame();assert.equal(await s.result(),false);s.clean();});
await test('receive failure before ack cleans lease',async()=>{const s=scene();s.channel.recv=()=>{throw Error('closed recv');};s.start();assert.equal(await s.result(),false);s.clean();});
await test('receive failure after ack waits render opportunity then cleans',async()=>{const s=scene();await s.acknowledge();s.release.reject(Error('closed recv'));await flush();assert.equal(s.root.inert,true);s.frame();await s.result();s.clean();});
await test('existing management mount close callback cleans a detached root',async()=>{const s=scene();await s.acknowledge();s.root.isConnected=false;s.root.__workflowManagement.lease.close();await flush();s.frame();await s.result();s.clean();});
await test('replacement root is never unlocked or assigned previous lease',async()=>{const s=scene();await s.acknowledge();const replacement=s.node(s.root.id);replacement.inert=true;replacement.__workflowManagement={lease:{other:true}};s.mutate();await flush();s.frame();await s.result();s.clean();assert.equal(replacement.inert,true);assert.equal(replacement.__workflowManagement.lease.other,true);});
function another(s, expected={...s.expected}) {
  const release=deferred(),sent=[];
  const task=s.lock(expected,{send:value=>sent.push(value),recv:()=>release.promise});
  return {release,sent,task};
}
await test('missing or invalid chain cannot obtain a navigation lease',async()=>{
  for(const chain of [undefined,null,'',0,-1,1.5])await refused(s=>{s.expected.chain=chain;});
});
for(const queued of [false,true])await test(`same chain awaits exact cleanup ${queued?'before release message arrives':'after release message arrives'}`,async()=>{
  const s=scene();await s.acknowledge();
  if(!queued){s.release.resolve();await flush();}
  const next=another(s);await flush();assert.deepEqual(next.sent,[]);assert.equal(s.root.inert,true);
  if(queued){s.release.resolve();await flush();}
  s.frame();await flush();await s.result();
  // Cleanup and reacquisition finish in microtasks before another paint.
  assert.equal(s.root.inert,true);assert.deepEqual(next.sent,[]);
  s.frame();await flush();assert.deepEqual(next.sent,[true]);
  next.release.resolve();await flush();s.frame();assert.equal(await next.task,true);s.clean();
});
await test('different click chain immediately refuses without waiting or taking over',async()=>{
  const s=scene();await s.acknowledge();const lease=s.root.__workflowManagement.lease;
  const next=another(s,{...s.expected,chain:708});assert.equal(await next.task,false);assert.deepEqual(next.sent,[false]);
  assert.equal(s.root.__workflowManagement.lease,lease);assert.equal(s.timers.size,0);await s.finish();
});
await test('same chain with different channel cannot await or take over a live lease',async()=>{
  const s=scene();await s.acknowledge();const lease=s.root.__workflowManagement.lease;
  const next=another(s,{...s.expected,channel:'COTHER001'});assert.equal(await next.task,false);
  assert.equal(s.root.__workflowManagement.lease,lease);await s.finish();
});
await test('same chain cannot reuse a replacement control even with identical attributes',async()=>{
  const s=scene();await s.acknowledge();const replacement=s.node(s.input.id,s.form,'INPUT');
  replacement.dataset={...s.input.dataset};replacement.setNative(s.expected.search);
  const next=another(s);assert.equal(await next.task,false);assert.equal(s.timers.size,0);await s.finish();
});
await test('changed native buffer during same-chain wait refuses after cleanup',async()=>{
  const s=scene();await s.acknowledge();const next=another(s);await flush();
  s.input.setNative('newer native text');s.release.resolve();await flush();s.frame();await s.result();
  assert.equal(await next.task,false);assert.deepEqual(next.sent,[false]);assert.equal(s.input.value,'newer native text');s.clean();
});
await test('same-chain wait timeout leaves previous live lease owned and locked',async()=>{
  const s=scene();await s.acknowledge();const lease=s.root.__workflowManagement.lease;
  const next=another(s);s.expire(1000);assert.equal(await next.task,false);assert.deepEqual(next.sent,[false]);
  assert.equal(s.root.__workflowManagement.lease,lease);assert.equal(s.root.inert,true);await s.finish();
});
await test('safe lookup failure writes exact text only into owned fixed footer',async()=>{
  const s=scene();const message='Channel details could not be loaded.';
  assert.equal(s.feedback(message),true);assert.equal(s.notice.textContent,message);
  assert.equal(s.root.__workflowManagement.channelRefusals,undefined);assert.equal(s.input.value,s.expected.search);
});
await test('safe failure feedback works while same-chain lease remains held',async()=>{
  const s=scene();await s.acknowledge();const lease=s.root.__workflowManagement.lease;
  assert.equal(s.feedback('Safe failure.'),true);assert.equal(s.root.__workflowManagement.lease,lease);
  assert.equal(s.root.inert,true);await s.finish();
});
await test('failure feedback refuses stale root detail and footer revisions',async()=>{
  for(const [node,key]of [['root','managementVisit'],['root','managementDocument'],['root','managementRevision'],['detail','detailRevision'],['notice','detailRevision']]){
    const s=scene();s[node].dataset[key]='999';assert.equal(s.feedback('Late failure.'),false);assert.equal(s.notice.textContent,'');
  }
});
await test('failure feedback refuses foreign footer Activity and detached detail',async()=>{
  for(const change of [s=>{s.notice.parent=s.node('workflow-activity',s.root);},s=>{s.detail.isConnected=false;},s=>{s.root.isConnected=false;}]){
    const s=scene();change(s);assert.equal(s.feedback('Late failure.'),false);assert.equal(s.notice.textContent,'');
  }
});
await test('failure feedback refuses changed native buffers generation composition and modal',async()=>{
  const changes=[s=>s.input.setNative('newer'),s=>{s.input.dataset.generation='999';},s=>{s.root.__workflowManagement.composing=true;},s=>{s.dom.modal={};},
    ...['status','from','to','limit','user'].map(key=>s=>s.fields[key].setNative('newer'))];
  for(const change of changes){const s=scene();change(s);assert.equal(s.feedback('Late failure.'),false);assert.equal(s.notice.textContent,'');}
});
await test('failure feedback cannot cross another live chain or replaced leased control',async()=>{
  const s=scene();await s.acknowledge();assert.equal(s.feedback('Late failure.',{...s.expected,chain:708}),false);
  const replacement=s.node(s.input.id,s.form,'INPUT');replacement.dataset={...s.input.dataset};replacement.setNative(s.expected.search);
  assert.equal(s.feedback('Late failure.'),false);assert.equal(s.notice.textContent,'');await s.finish();
});
console.log(JSON.stringify({ok:true,count:names.length,tests:names,sourceSha256:crypto.createHash('sha256').update(source).digest('hex')}));
