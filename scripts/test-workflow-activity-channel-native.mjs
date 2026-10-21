import {dateButton, dateWitnessSource} from './workflow_activity_date_fixture.mjs';
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
function scene(kind='channel') {
  const expected={chain:707,visit:101,document:202,revision:303,detail:404,generation:505,search:'  Cafe\u0301  ',button:'workflow-channel-0',channel:'CACTTEST1',
    filters:{revision:606,status:'',from:'',to:'',limit:'100',user:''}};
  const byId=new Map(),events=[],observers=[],timers=new Map(),frames=new Map();let serial=0;
  function node(id,parent=null,tagName='DIV') {
    const listeners=new Map();
    const n={id,parent,tagName,nodeType:1,isConnected:true,dataset:{},attributes:{},childNodes:[],inert:false,disabled:false,validity:{badInput:false},
      getAttribute(key){if(key.startsWith('data-'))return this.dataset[key.slice(5).replace(/-([a-z])/g,(_,c)=>c.toUpperCase())]??null;return this.attributes[key]??null;},
      contains(other){for(let at=other;at;at=at.parent)if(at===this)return true;return false;},
      closest(selector){assert.equal(selector,'#workflow-activity');for(let at=this;at;at=at.parent)if(at.id==='workflow-activity')return at;return null;},
      matches(selector){assert.equal(selector,':disabled');return !!this.disabled || !!this.fieldsetDisabled;},
      addEventListener(event,fn){if(!listeners.has(event))listeners.set(event,new Set());listeners.get(event).add(fn);},
      removeEventListener(event,fn){listeners.get(event)?.delete(fn);}};
    let value='';Object.defineProperties(n,{
      parentElement:{get:()=>n.parent},
      textContent:{get:()=>n.childNodes.map(c=>c.textContent).join(''),set:v=>{n.childNodes=v===''?[]:[{nodeType:3,textContent:v}];}},
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
  if(kind==='file') {
    delete expected.channel;
    expected.file={id:'FCANON003',kind:'list',label:'Révision & 中🙂',url:'https://fixture.invalid/lists/FCANON003?view=original',part:1};
    expected.file.fragments=[{kind:'text',text:expected.file.label}];
    button.tagName='A';delete button.dataset.channel;
    button.dataset.file=expected.file.id;button.dataset.fileKind=expected.file.kind;
    button.dataset.part='1';
    button.attributes={href:expected.file.url};
    button.getAttribute=key=>button.attributes[key]??null;
    const name=node('file-label',button,'SPAN');name.dataset.fileLabel=expected.file.label;
    const part=node('file-fragment-0',name,'SPAN');part.dataset={fileTitleKind:'text',fileTitleIndex:'0'};part.textContent=expected.file.label;name.childNodes=[part];
    button.querySelector=selector=>{assert.equal(selector,'.workflow-activity-file-name');return button.fileName;};
    button.fileName=name;
  }
  const notice=node('workflow-activity-navigation-notice',activity);notice.dataset.detailRevision='404';
  const input=node('workflow-activity-user-search',form,'INPUT');input.dataset.generation='505';input.setNative(expected.search);
  const fields=Object.fromEntries(['status','limit','user'].map(k=>{
    const n=node('workflow-activity-'+k,form,['from','to'].includes(k)?'INPUT':'SELECT');n.setNative(expected.filters[k]);return [k,n];
  }));
  const date=dateButton(expected.filters,node('workflow-activity-date-range',form,'BUTTON'));
  const dom={modal:null};
  const document={documentElement:{},getElementById(id){
    if(['workflow-management-title','workflow-management-description'].includes(id))throw Error('Guard inspected hidden metadata');
    return byId.get(id)??null;},querySelector(selector){assert.equal(selector,'[role="dialog"][aria-modal="true"]');return dom.modal;}};
  class MutationObserver {constructor(callback){this.callback=callback;this.active=false;observers.push(this);}observe(){this.active=true;}disconnect(){this.active=false;}}
  const realm=vm.createContext({document,MutationObserver,
    requestAnimationFrame:fn=>{const id=++serial;frames.set(id,fn);return id;},cancelAnimationFrame:id=>frames.delete(id),
    setTimeout:(fn,ms)=>{const id=++serial;timers.set(id,{fn,ms});return id;},clearTimeout:id=>timers.delete(id)});
  vm.runInContext(dateWitnessSource+"\n"+source,realm,{filename:url.pathname});
  const release=deferred(),sent=[];let task;
  const channel={send:v=>sent.push(v),recv:()=>release.promise};
  const s={expected,root,activity,content,form,detail,button,notice,input,fields,date,dom,byId,node,channel,release,sent,timers,frames,
    feedback:(message,expected=s.expected)=>realm.workflowActivityChannelFeedback(expected,message),
    pending:(expected=s.expected)=>realm.workflowActivityChannelPending(expected),
    settled:(expected=s.expected)=>realm.workflowActivityChannelSettled(expected),
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

for(const flag of ['calendarOpen','calendarDirty','calendarPending'])await test('calendar '+flag+' refuses channel navigation',async()=>{await refused(s=>{s.date.dataset[flag]='true';});});
await test('date replacement cannot reuse an admitted channel chain',async()=>{const s=scene();s.start();dateButton(s.expected.filters,s.node(s.date.id,s.form,'BUTTON'));s.frame();assert.equal(await s.result(),false);s.clean();});
await test('calendar rollback epoch retires original channel lease',async()=>{const s=scene();s.start();s.date.dataset.calendarEpoch='2';s.frame();assert.equal(await s.result(),false);s.clean();});
await test('feedback cannot use an original chain after date trigger replacement',async()=>{const s=scene();await s.acknowledge();dateButton(s.expected.filters,s.node(s.date.id,s.form,'BUTTON'));assert.equal(s.feedback('Owned notice'),false);await s.finish();});
await test('locks exact native controls before frame and retains until explicit release',async()=>{
  const s=scene();await s.acknowledge();assert.equal(s.form.inert,true);assert.equal(s.detail.inert,true);
  assert.equal(s.input.value,s.expected.search);assert.equal(s.input.disabled,false);assert.equal(s.input.readOnly,false);
  assert.equal(s.timers.size,0);s.frame();await flush();assert.equal(s.root.inert,true);assert.equal(await s.finish(),true);
});
await test('unchanged document revision still restores captured inert properties',async()=>{const s=scene();await s.acknowledge();await s.finish();assert.equal(s.root.dataset.managementRevision,'303');});
await test('release remains locked until controlled commit render opportunity',async()=>{const s=scene();await s.acknowledge();s.release.resolve();await flush();assert.equal(s.root.inert,true);s.frame();await s.result();s.clean();});
await test('release cleanup has bounded fallback only after Rust released',async()=>{const s=scene();await s.acknowledge();s.expire(2000);assert.equal(s.root.inert,true);s.release.resolve();await flush();s.expire(200);await s.result();s.clean();});
await test('native-ahead search refuses without changing exact Unicode buffer',async()=>{const s=await refused(s=>s.input.setNative('Cafe\u0301 newer'));assert.equal(s.input.value,'Cafe\u0301 newer');});
for(const key of ['status','limit','user'])await test(`native-ahead ${key} refuses without buffer restoration`,async()=>{const s=await refused(s=>s.fields[key].setNative('native-ahead'));assert.equal(s.fields[key].value,'native-ahead');});
await test('exact empty and opaque date buffers are compared without date validation',async()=>{const s=scene();s.expected.filters.from='uninterpreted';(s.date.dataset.from = 'uninterpreted');await s.acknowledge();await s.finish();});
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
for(const key of ['search','status','limit','user','button'])await test(`disabled and badInput ${key} refuse`,async()=>{
  const get=s=>key==='search'?s.input:key==='button'?s.button:s.fields[key];
  await refused(s=>{get(s).disabled=true;});await refused(s=>{get(s).validity.badInput=true;});
});
await test('ancestor fieldset disabled state refuses',async()=>{await refused(s=>{s.date.fieldsetDisabled=true;});});
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
  await s.acknowledge();await s.finish();
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
    ...['status','limit','user'].map(key=>s=>s.fields[key].setNative('newer'))];
  for(const change of changes){const s=scene();change(s);assert.equal(s.feedback('Late failure.'),false);assert.equal(s.notice.textContent,'');}
});
await test('failure feedback cannot cross another live chain or replaced leased control',async()=>{
  const s=scene();await s.acknowledge();assert.equal(s.feedback('Late failure.',{...s.expected,chain:708}),false);
  const replacement=s.node(s.input.id,s.form,'INPUT');replacement.dataset={...s.input.dataset};replacement.setNative(s.expected.search);
  assert.equal(s.feedback('Late failure.'),false);assert.equal(s.notice.textContent,'');await s.finish();
});
await test('List exact original anchor label and permalink retain the same renderer lease',async()=>{
  const s=scene('file');await s.acknowledge();
  assert.equal(s.button.getAttribute('href'),s.expected.file.url);
  assert.equal(s.feedback('File details could not be loaded.'),true);
  assert.equal(s.notice.textContent,'File details could not be loaded.');
  assert.equal(await s.finish(),true);
});
const fileChanges=[
  ['native search',s=>s.input.setNative('newer 中')],
  ...['status','limit','user'].map(key=>[`${key} buffer`,s=>s.fields[key].setNative('native-ahead')]),
  ['IME',s=>{s.root.__workflowManagement.composing=true;}],
  ...['calendarOpen','calendarDirty','calendarPending'].map(key=>[key,s=>{s.date.dataset[key]='true';}]),
  ['modal',s=>{s.dom.modal={};}],
  ['wrong file',s=>{s.button.dataset.file='FOTHER001';}],
  ['wrong type',s=>{s.button.dataset.fileKind='canvas';}],
  ['changed canonical label',s=>{s.button.fileName.textContent='Other title';}],
  ['changed original href',s=>{s.button.attributes.href+='&other=true';}],
  ['missing original href',s=>{delete s.button.attributes.href;}],
  ['button instead of anchor',s=>{s.button.tagName='BUTTON';}],
  ['detached occurrence',s=>{s.button.isConnected=false;}],
  ['foreign detail',s=>{s.button.parent=s.root;}],
  ['unsupported expected kind',s=>{s.expected.file.kind='canvas';}],
  ['empty expected id',s=>{s.expected.file.id='';}],
  ['empty expected url',s=>{s.expected.file.url='';}],
  ['empty expected label',s=>{s.expected.file.label='';}],
];
for(const [label,change] of fileChanges)await test(`List ${label} refuses without navigation acknowledgment`,async()=>{
  const s=scene('file');change(s);
  assert.equal(await s.start(),false);assert.deepEqual(s.sent,[false]);
  assert.equal(s.feedback('Late file notice.'),false);
});
for(const [label,change] of [
  ['file identity',s=>{s.button.dataset.file='FOTHER001';}],
  ['canonical title',s=>{s.button.fileName.textContent='Renamed';}],
  ['canonical permalink',s=>{s.button.attributes.href='https://fixture.invalid/new';}],
])await test(`List ${label} changed before acknowledgment releases original lock`,async()=>{
  const s=scene('file');s.start();change(s);s.mutate();s.frame();
  assert.equal(await s.result(),false);assert.deepEqual(s.sent,[false]);s.clean();
});
await test('List same-ID replacement anchor cannot inherit the captured occurrence',async()=>{
  const s=scene('file');s.start();
  const replacement=s.node(s.button.id,s.detail,'A');
  replacement.dataset={...s.button.dataset};
  replacement.getAttribute=s.button.getAttribute;
  replacement.querySelector=s.button.querySelector;
  s.mutate();s.frame();assert.equal(await s.result(),false);s.clean();
  assert.equal(replacement.inert,false);
});
await test('List duplicate click cannot take over a held original chain',async()=>{
  const s=scene('file');await s.acknowledge();const lease=s.root.__workflowManagement.lease;
  const next=another(s,{...s.expected,chain:708});assert.equal(await next.task,false);
  assert.deepEqual(next.sent,[false]);assert.equal(s.root.__workflowManagement.lease,lease);
  await s.finish();
});
await test('List same chain waits original cleanup without changing source href or draft buffers',async()=>{
  const s=scene('file');await s.acknowledge();const next=another(s);await flush();
  assert.deepEqual(next.sent,[]);s.release.resolve();await flush();s.frame();await s.result();await flush();
  s.frame();await flush();assert.deepEqual(next.sent,[true]);
  assert.equal(s.button.getAttribute('href'),s.expected.file.url);assert.equal(s.input.value,s.expected.search);
  next.release.resolve();await flush();s.frame();assert.equal(await next.task,true);s.clean();
});
await test('List changed permalink cannot reuse an old same-chain release or feedback',async()=>{
  const s=scene('file');await s.acknowledge();const lease=s.root.__workflowManagement.lease;
  const changed={...s.expected,file:{...s.expected.file,url:'https://fixture.invalid/new'}};
  const next=another(s,changed);assert.equal(await next.task,false);
  assert.equal(s.feedback('Late notice.',changed),false);
  assert.equal(s.root.__workflowManagement.lease,lease);await s.finish();
});
await test('admitted List retry clears original failure only after exact lease acknowledgment',async()=>{
  const s=scene('file');s.notice.textContent='Connect to Slack to open this List.';
  s.start();assert.equal(s.notice.textContent,'Connect to Slack to open this List.');
  s.frame();await flush();assert.deepEqual(s.sent,[true]);assert.equal(s.notice.textContent,'');
  await s.finish();
});
await test('List cannot admit or clear a successor footer',async()=>{
  const s=scene('file');s.notice.textContent='Successor notice';s.notice.dataset.detailRevision='999';
  s.start();s.frame();assert.equal(await s.result(),false);s.clean();
  assert.equal(s.notice.textContent,'Successor notice');assert.equal(s.feedback(''),false);
});
await test('channel lease preserves existing feedback behavior',async()=>{
  const s=scene();s.notice.textContent='Existing channel notice';
  await s.acknowledge();assert.equal(s.notice.textContent,'Existing channel notice');
  assert.equal(s.feedback(''),false);await s.finish();
});

// The semantic title deliberately differs from its painted textContent.
function emojiScene() {
  const s=scene('file');
  s.expected.file.label='Revision & \u4e2d :wave: :ready_alias: :unknown:';
  s.expected.file.fragments=[
    {kind:'text',text:'Revision & \u4e2d '},
    {kind:'emoji',text:'\u{1f44b}',title:':wave:'},
    {kind:'text',text:' '},
    {kind:'image',src:'http://super-platinum-media.localhost/emoji/fixture?v=7',alt:':ready_alias:',title:':ready_alias:'},
    {kind:'text',text:' '},
    {kind:'text',text:':unknown:'},
  ];
  s.paint=()=>{
    const name=s.node('file-label',s.button,'SPAN');name.dataset.fileLabel=s.expected.file.label;
    name.childNodes=s.expected.file.fragments.map((f,index)=>{
      const part=s.node('fragment-'+index,name,'SPAN');part.dataset={fileTitleKind:f.kind,fileTitleIndex:String(index)};
      if(f.kind==='image') {
        const image=s.node('file-image',part,'IMG');image.attributes={src:f.src,alt:f.alt,title:f.title};part.childNodes=[image];
      } else {part.textContent=f.text;if(f.kind==='emoji')part.attributes.title=f.title;}
      return part;
    });
    s.button.fileName=name;s.parts=name.childNodes;s.image=s.parts[3].childNodes[0];return name;
  };
  s.paint();return s;
}
await test('painted emoji and image title keeps semantic label distinct and admits original nodes',async()=>{
  const s=emojiScene();assert.notEqual(s.button.fileName.textContent,s.expected.file.label);
  await s.acknowledge();assert.equal(s.feedback('Exact image failure.'),true);await s.finish();
});
const paintChanges=[
  ['semantic label',s=>{s.button.fileName.dataset.fileLabel+='other';}],
  ['occurrence part',s=>{s.button.dataset.part='2';}],
  ['invalid expected part',s=>{s.expected.file.part=1.5;}],
  ['missing expected fragments',s=>{delete s.expected.file.fragments;}],
  ['empty expected fragments',s=>{s.expected.file.fragments=[];}],
  ['unknown typed kind',s=>{s.expected.file.fragments[0].kind='html';}],
  ['text data',s=>{s.parts[0].childNodes[0].textContent+='changed';}],
  ['glyph',s=>{s.parts[1].childNodes[0].textContent='x';}],
  ['emoji tooltip',s=>{s.parts[1].attributes.title=':other:';}],
  ['extra text title',s=>{s.parts[0].attributes.title='unowned';}],
  ['kind attribute',s=>{s.parts[1].dataset.fileTitleKind='text';}],
  ['index attribute',s=>{s.parts[1].dataset.fileTitleIndex='01';}],
  ['image src',s=>{s.image.attributes.src+='&replacement';}],
  ['image alt',s=>{s.image.attributes.alt='forged Activity file emoji';}],
  ['image title',s=>{s.image.attributes.title=':other:';}],
  ['missing image title',s=>{delete s.image.attributes.title;}],
  ['unexpected wrapper title',s=>{s.parts[3].attributes.title='extra';}],
  ['image descendant',s=>{s.image.childNodes=[s.node('extra',s.image,'SPAN')];}],
  ['detached image',s=>{s.image.isConnected=false;}],
  ['extra title fragment',s=>{s.button.fileName.childNodes.push(s.node('extra',s.button.fileName,'SPAN'));}],
  ['extra text descendant',s=>{s.parts[0].childNodes.push(s.node('extra',s.parts[0],'SPAN'));}],
  ['fragment reorder',s=>{[s.parts[0],s.parts[1]]=[s.parts[1],s.parts[0]];}],
  ['nested name',s=>{s.button.fileName.parent=s.node('nested',s.button,'SPAN');}],
  ['cold image fallback',s=>{s.parts[3].childNodes=[s.node('fallback',s.parts[3],'SPAN')];}],
];
for(const [name,change] of paintChanges)await test(`typed title ${name} refuses without dispatch`,async()=>{
  const s=emojiScene();change(s);assert.equal(await s.start(),false);assert.deepEqual(s.sent,[false]);
  assert.equal(s.feedback('Forged feedback'),false);
});
for(const phase of ['before-ack','held'])for(const replacement of ['name','fragment','text','image'])
await test(`same-valued ${replacement} replacement ${phase} retires original painted lease`,async()=>{
  const s=emojiScene();if(phase==='held')await s.acknowledge();else s.start();
  if(replacement==='name')s.paint();
  else if(replacement==='fragment') {
    const part=s.node('replacement',s.button.fileName,'SPAN');part.dataset={...s.parts[0].dataset};part.textContent=s.parts[0].textContent;s.parts[0]=part;
  } else if(replacement==='text')s.parts[0].childNodes=[{nodeType:3,textContent:s.parts[0].textContent}];
  else {const image=s.node('replacement',s.parts[3],'IMG');image.attributes={...s.image.attributes};s.parts[3].childNodes=[image];}
  assert.equal(s.feedback('Stale original feedback'),false);s.mutate();await flush();s.frame();
  assert.equal(await s.result(),phase==='held');s.clean();
});
await test('image mutation before acknowledgment preserves original shortcode and rejects lease',async()=>{
  const s=emojiScene();s.start();s.image.attributes.alt='forged';s.mutate();s.frame();
  assert.equal(await s.result(),false);assert.equal(s.expected.file.fragments[3].alt,':ready_alias:');s.clean();
});
await test('restoring exact original image attributes permits a fresh explicit attempt',async()=>{
  const s=emojiScene(),alt=s.image.attributes.alt;s.image.attributes.alt='forged';
  assert.equal(await s.start(),false);s.image.attributes.alt=alt;s.expected.chain++;s.sent.length=0;
  await s.acknowledge();assert.equal(s.notice.textContent,'');await s.finish();
});
await test('channel feedback and pending require actual prior native admission',async()=>{
  const s=scene();assert.equal(s.feedback('Unadmitted failure'),false);assert.equal(s.pending(),false);
  assert.equal(s.notice.textContent,'');assert.equal(s.root.__workflowManagement.channelNavigationNotice,undefined);
});
await test('known native admission does not flash a pending notice',async()=>{
  const s=scene();s.notice.textContent='Earlier offline message';await s.acknowledge();
  assert.equal(s.notice.textContent,'Earlier offline message');
  assert.equal(s.root.__workflowManagement.channelNavigationNotice.phase,'admitted');await s.finish();
});
await test('missing read pending replaces offline after original admission and releases original lease',async()=>{
  const s=scene();s.notice.textContent='Connect to Slack to open this conversation.';
  await s.acknowledge();const lease=s.root.__workflowManagement.lease;
  assert.equal(s.pending(),true);assert.equal(s.notice.textContent,'Opening conversation…');
  const owner=s.root.__workflowManagement.channelNavigationNotice;
  assert.equal(owner.chain,'707');assert.equal(owner.detail,'404');assert.equal(owner.phase,'pending');
  assert.equal(owner.notice,s.notice);assert.equal(owner.nodes[3],s.button);
  assert.equal(s.root.__workflowManagement.lease,lease);await s.finish();
  assert.equal(s.notice.textContent,'Opening conversation…');assert.equal(s.settled(),true);
  assert.equal(s.notice.textContent,'');assert.equal(owner.phase,'settled');s.clean();
});
await test('missing status cannot begin after original lease has released',async()=>{
  const s=scene();await s.acknowledge();await s.finish();assert.equal(s.pending(),false);
});
await test('settlement before delayed pending acknowledgment keeps a tombstone',async()=>{
  const s=scene();await s.acknowledge();assert.equal(s.settled(),false);
  assert.equal(s.root.__workflowManagement.channelNavigationNotice.phase,'settled');
  assert.equal(s.pending(),false);assert.equal(s.notice.textContent,'');await s.finish();
});
await test('own failure replaces pending and cleanup cannot clear that failure',async()=>{
  const s=scene();await s.acknowledge();assert.equal(s.pending(),true);await s.finish();
  assert.equal(s.feedback('Current unavailable'),true);assert.equal(s.settled(),false);
  assert.equal(s.notice.textContent,'Current unavailable');assert.equal(s.root.__workflowManagement.channelNavigationNotice.phase,'failure');
});
await test('retired native buffer can clear only original pending without restoring input',async()=>{
  const s=scene();await s.acknowledge();s.pending();await s.finish();s.input.setNative('new native draft');
  assert.equal(s.feedback('Late unavailable'),false);assert.equal(s.settled(),true);
  assert.equal(s.input.value,'new native draft');assert.equal(s.notice.textContent,'');
});
await test('newer pending survives old cleanup and late failure',async()=>{
  const s=scene();await s.acknowledge();s.pending();await s.finish();
  const expected={...s.expected,chain:708},next=another(s,expected);s.frame();await flush();
  assert.deepEqual(next.sent,[true]);assert.equal(s.pending(expected),true);
  const lease=s.root.__workflowManagement.lease,owner=s.root.__workflowManagement.channelNavigationNotice;
  assert.equal(s.settled(),false);assert.equal(s.feedback('Old failure'),false);
  assert.equal(s.notice.textContent,'Opening conversation…');assert.equal(s.root.__workflowManagement.lease,lease);
  assert.equal(s.root.__workflowManagement.channelNavigationNotice,owner);
  next.release.resolve();await flush();s.frame();await next.task;s.clean();
});
for(const terminal of ['failure','settled'])await test(`latest ${terminal} rejects old delayed feedback even without lease or pending`,async()=>{
  const s=scene();await s.acknowledge();s.pending();s.feedback('First failure');await s.finish();
  const expected={...s.expected,chain:708},next=another(s,expected);s.frame();await flush();
  assert.deepEqual(next.sent,[true]);assert.equal(s.pending(expected),true);
  next.release.resolve();await flush();s.frame();await next.task;s.clean();
  if(terminal==='failure')assert.equal(s.feedback('Second failure',expected),true);
  else assert.equal(s.settled(expected),true);
  const text=s.notice.textContent,owner=s.root.__workflowManagement.channelNavigationNotice;
  assert.equal(s.feedback('Old failure after successor terminal'),false);assert.equal(s.settled(),false);
  assert.equal(s.notice.textContent,text);assert.equal(owner.phase,terminal);
});
await test('later native refusal cannot be cleared or overwritten by old pending owner',async()=>{
  const s=scene();await s.acknowledge();s.pending();
  const next=another(s,{...s.expected,chain:708});assert.equal(await next.task,false);
  const text=s.notice.textContent;assert.equal(s.root.__workflowManagement.channelNavigationNotice.phase,'refused');
  assert.equal(s.settled(),false);assert.equal(s.feedback('Old error'),false);assert.equal(s.notice.textContent,text);
  await s.finish();
});
for(const target of ['notice','detail','button','input','date'])await test(`same-ID ${target} replacement refuses pending and old feedback`,async()=>{
  const s=scene();await s.acknowledge();
  const original=s[target],replacement=s.node(original.id,original.parent,original.tagName);
  replacement.dataset={...original.dataset};replacement.attributes={...original.attributes};
  if(target==='input')replacement.setNative(s.expected.search);
  if(target==='date')dateButton(s.expected.filters,replacement);
  const text=replacement.textContent;
  assert.equal(s.pending(),false);assert.equal(s.feedback('Old replacement failure'),false);
  assert.equal(replacement.textContent,text);await s.finish();
});
for(const target of ['notice','detail','root'])await test(`settlement never clears replacement ${target} or unlocks successor`,async()=>{
  const s=scene();await s.acknowledge();s.pending();await s.finish();
  const original=s[target],replacement=s.node(original.id,original.parent,original.tagName);
  replacement.dataset={...original.dataset};replacement.textContent='Successor feedback';replacement.inert=true;
  if(target==='root')replacement.__workflowManagement={lease:{newer:true}};
  assert.equal(s.settled(),false);assert.equal(replacement.textContent,'Successor feedback');assert.equal(replacement.inert,true);
});
await test('List pending helpers stay inert while its admitted owner retains file feedback',async()=>{
  const s=scene('file');await s.acknowledge();assert.equal(s.pending(),false);assert.equal(s.settled(),false);
  assert.equal(s.root.__workflowManagement.channelNavigationNotice.phase,'admitted');
  assert.equal(s.feedback('Existing List feedback'),true);assert.equal(s.notice.textContent,'Existing List feedback');await s.finish();
});
await test('copied latest record cannot transfer ownership into a replacement management state',async()=>{
  const s=scene();await s.acknowledge();s.pending();await s.finish();
  s.root.__workflowManagement={...s.root.__workflowManagement};
  assert.equal(s.feedback('Transferred failure'),false);assert.equal(s.settled(),false);
  assert.equal(s.notice.textContent,'Opening conversation…');
});
// Two original occurrences share one real detail, form, footer and buffer owner.
function otherOccurrence(s, kind, chain=708) {
  const expected={...s.expected,chain,button:'workflow-mixed-'+kind};
  delete expected.file;delete expected.channel;
  const button=s.node(expected.button,s.detail,kind==='file'?'A':'BUTTON');
  if(kind==='file') {
    expected.file={id:'FCANON003',kind:'list',label:'Mixed List',url:'https://fixture.invalid/lists/FCANON003',part:2,
      fragments:[{kind:'text',text:'Mixed List'}]};
    button.dataset={file:expected.file.id,fileKind:'list',part:'2'};button.attributes.href=expected.file.url;
    const name=s.node('mixed-file-name',button,'SPAN');name.dataset.fileLabel=expected.file.label;
    const part=s.node('mixed-file-part',name,'SPAN');part.dataset={fileTitleKind:'text',fileTitleIndex:'0'};
    part.textContent=expected.file.label;name.childNodes=[part];button.fileName=name;
    button.querySelector=selector=>{assert.equal(selector,'.workflow-activity-file-name');return button.fileName;};
  } else {expected.channel='CMIXED001';button.dataset.channel=expected.channel;}
  return {expected,button};
}
async function admitOther(s,expected) {
  const next=another(s,expected);s.frame();await flush();assert.deepEqual(next.sent,[true]);return next;
}
async function finishOther(s,next) {
  next.release.resolve();await flush();s.frame();assert.equal(await next.task,true);s.clean();
}
await test('mixed newer List failure rejects delayed original channel failure',async()=>{
  const s=scene();await s.acknowledge();assert.equal(s.pending(),true);await s.finish();
  const {expected}=otherOccurrence(s,'file'),next=await admitOther(s,expected);
  assert.equal(s.feedback('Newer List offline',expected),true);await finishOther(s,next);
  assert.equal(s.feedback('Delayed channel unavailable'),false);
  assert.equal(s.notice.textContent,'Newer List offline');
});

for(const firstKind of ['channel','file']) {
  const secondKind=firstKind==='channel'?'file':'channel';
  const phases=secondKind==='channel'?['admitted','pending','failure','settled','refused']:['admitted','failure','refused'];
  for(const phase of phases)await test(`mixed ${firstKind} cannot reacquire or write after newer ${secondKind} ${phase}`,async()=>{
    const s=scene(firstKind);await s.acknowledge();if(firstKind==='channel')s.pending();await s.finish();
    const {expected}=otherOccurrence(s,secondKind);let next;
    if(phase==='refused') {
      s.root.__workflowManagement.composing=true;next=another(s,expected);
      assert.equal(await next.task,false);s.root.__workflowManagement.composing=false;
    } else {
      next=await admitOther(s,expected);
      if(phase==='pending'||phase==='settled')assert.equal(s.pending(expected),true);
      if(phase==='failure')assert.equal(s.feedback('Newer failure',expected),true);
      // A stale final success cannot steal a still-held successor lease either.
      const lease=s.root.__workflowManagement.lease, text=s.notice.textContent;
      const stale=another(s);assert.equal(await stale.task,false);assert.deepEqual(stale.sent,[false]);
      assert.equal(s.root.__workflowManagement.lease,lease);assert.equal(s.notice.textContent,text);
      await finishOther(s,next);
      if(phase==='settled')assert.equal(s.settled(expected),true);
    }
    const owner=s.root.__workflowManagement.channelNavigationNotice,text=s.notice.textContent;
    const refs=[...owner.nodes],refusals=s.root.__workflowManagement.channelRefusals;
    assert.equal(owner.phase,phase);
    assert.equal(s.feedback('Delayed old error'),false);assert.equal(s.feedback(''),false);
    assert.equal(s.pending(),false);assert.equal(s.settled(),false);
    const stale=another(s);assert.equal(await stale.task,false);assert.deepEqual(stale.sent,[false]);
    assert.equal(s.root.__workflowManagement.channelNavigationNotice,owner);
    assert.equal(owner.phase,phase);assert.deepEqual([...owner.nodes],refs);
    assert.equal(s.root.__workflowManagement.channelRefusals,refusals);
    assert.equal(s.notice.textContent,text);assert.equal(s.input.value,s.expected.search);s.clean();
  });
}
for(const kind of ['channel','file'])await test(`mixed ${kind} failure cannot overwrite a later other-kind failure`,async()=>{
  const s=scene(kind);await s.acknowledge();assert.equal(s.feedback('Earlier failure'),true);await s.finish();
  const {expected}=otherOccurrence(s,kind==='file'?'channel':'file'),next=await admitOther(s,expected);
  assert.equal(s.feedback('Latest failure',expected),true);await finishOther(s,next);
  assert.equal(s.feedback('Earlier failure delayed again'),false);assert.equal(s.notice.textContent,'Latest failure');s.clean();
});
for(const kind of ['channel','file'])for(const phase of ['failure','refused'])
await test(`${kind} same-chain terminal ${phase} cannot revive or overwrite its terminal notice`,async()=>{
  const s=scene(kind);await s.acknowledge();await s.finish();
  if(phase==='failure')assert.equal(s.feedback('Terminal failure'),true);
  else {s.root.__workflowManagement.composing=true;assert.equal(await another(s).task,false);s.root.__workflowManagement.composing=false;}
  const owner=s.root.__workflowManagement.channelNavigationNotice,text=s.notice.textContent;
  assert.equal(await another(s).task,false);assert.equal(owner.phase,phase);assert.equal(s.notice.textContent,text);
  assert.equal(s.root.__workflowManagement.channelNavigationNotice,owner);s.clean();
});
await test('settled channel cannot reacquire the terminal chain but its queued current error remains scoped',async()=>{
  const s=scene();await s.acknowledge();s.pending();await s.finish();assert.equal(s.settled(),true);
  assert.equal(await another(s).task,false);assert.equal(s.notice.textContent,'');
  assert.equal(s.root.__workflowManagement.channelNavigationNotice.phase,'settled');
  // Rust schedules cleanup and error evaluation separately. This is still the latest exact owner.
  assert.equal(s.feedback('Current error evaluated after cleanup'),true);s.clean();
});
for(const kind of ['channel','file'])await test(`${kind} current failure does not release an acknowledged original lease`,async()=>{
  const s=scene(kind);await s.acknowledge();const lease=s.root.__workflowManagement.lease;
  assert.equal(s.feedback('Current failure'),true);s.mutate();await flush();s.frame();await flush();
  assert.equal(s.root.__workflowManagement.lease,lease);assert.equal(s.root.inert,true);
  await s.finish();assert.equal(s.notice.textContent,'Current failure');
});
await test('unadmitted List feedback cannot claim another occurrence footer',async()=>{
  const s=scene('file');assert.equal(s.feedback('Unadmitted List error'),false);assert.equal(s.feedback(''),false);
  assert.equal(s.root.__workflowManagement.channelNavigationNotice,undefined);assert.equal(s.notice.textContent,'');
});
await test('newer mixed native refusal remains after original live lease cleanup',async()=>{
  const s=scene();await s.acknowledge();s.pending();const lease=s.root.__workflowManagement.lease;
  const {expected}=otherOccurrence(s,'file'),next=another(s,expected);assert.equal(await next.task,false);
  const owner=s.root.__workflowManagement.channelNavigationNotice,text=s.notice.textContent;
  assert.equal(owner.phase,'refused');assert.equal(owner.nodes[3],s.byId.get(expected.button));
  assert.equal(s.root.__workflowManagement.lease,lease);assert.equal(s.feedback('Old failure'),false);assert.equal(s.settled(),false);
  await s.finish();assert.equal(s.root.__workflowManagement.channelNavigationNotice,owner);assert.equal(s.notice.textContent,text);
});
await test('mixed chains compare numeric order rather than decimal lexical order',async()=>{
  const s=scene();s.expected.chain='9';await s.acknowledge();await s.finish();
  const {expected}=otherOccurrence(s,'file','10'),next=await admitOther(s,expected);await finishOther(s,next);
  assert.equal(await another(s).task,false);assert.equal(s.root.__workflowManagement.channelNavigationNotice.chain,'10');s.clean();
});
for(const target of ['button','input','date','notice','paint'])await test(`List same-chain reentry cannot adopt a same-ID replacement ${target}`,async()=>{
  const s=scene('file');await s.acknowledge();await s.finish();const owner=s.root.__workflowManagement.channelNavigationNotice;
  if(target==='paint') {
    const original=s.button.fileName.childNodes[0];
    const replacement=s.node(original.id,s.button.fileName,'SPAN');replacement.dataset={...original.dataset};replacement.textContent=original.textContent;
    s.button.fileName.childNodes=[replacement];
  } else {
    const original=s[target],replacement=s.node(original.id,original.parent,original.tagName);
    replacement.dataset={...original.dataset};replacement.attributes={...original.attributes};
    if(target==='button'){replacement.fileName=original.fileName;replacement.fileName.parent=replacement;replacement.querySelector=selector=>original.querySelector(selector);}
    if(target==='input')replacement.setNative(s.expected.search);
    if(target==='date')dateButton(s.expected.filters,replacement);
  }
  const text=s.notice.textContent;assert.equal(await another(s).task,false);assert.equal(s.feedback('Replacement error'),false);
  assert.equal(s.root.__workflowManagement.channelNavigationNotice,owner);assert.equal(s.notice.textContent,text);s.clean();
});
console.log(JSON.stringify({ok:true,count:names.length,tests:names,sourceSha256:crypto.createHash('sha256').update(source).digest('hex')}));
