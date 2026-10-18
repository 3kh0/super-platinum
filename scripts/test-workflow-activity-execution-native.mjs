import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
const path=new URL('../src/desktop/app_home/about/management/activity/execution/native.js',import.meta.url);
const dateSource=fs.readFileSync(new URL('../src/desktop/app_home/about/management/activity/native.js',import.meta.url),'utf8');
const source=fs.readFileSync(path,'utf8'),tests=[];
const flush=async()=>{for(let i=0;i<14;i++)await Promise.resolve();};
async function test(name,run){await run();tests.push(name);console.log('PASS '+name);}
function scene(filters = {}){
  const expected={visit:1,document:2,documentRevision:3,memberGeneration:5,pageRevision:7,retiredRead:17,execution:'EXECUTION',search:'Cafe\u0301',filters:{revision:6,status:'',from:'',to:'',limit:'100',user:'',...filters}};
  const map=new Map(),frames=new Map(),timers=new Map(),observers=[],events=[];let sequence=0;
  function node(id,parent=null,tagName='DIV'){
    const listeners=new Map();let value='';
    const n={focus(){dom.focused=this;dom.focusCount=(dom.focusCount??0)+1;},id,parent,tagName,dataset:{},isConnected:true,inert:false,disabled:false,validity:{badInput:false},
      contains(other){for(let p=other;p;p=p.parent)if(p===this)return true;return false;},
      matches(selector){assert.equal(selector,':disabled');return this.disabled||!!this.fieldsetDisabled;},
      closest(selector){assert.equal(selector,'fieldset[disabled]');return this.fieldsetDisabled?{}:null;},
      addEventListener(e,f){if(!listeners.has(e))listeners.set(e,new Set());listeners.get(e).add(f);},
      removeEventListener(e,f){listeners.get(e)?.delete(f);},setNative:v=>{value=v;}};
    Object.defineProperty(n,'value',{get:()=>value,set:()=>{throw Error('Input write');}});
    n.querySelector=selector=>{const found=map.get(selector.slice(1));return found&&n.contains(found)?found:null;};
    map.set(id,n);events.push({n,listeners});return n;
  }
  const root=node('workflow-management');root.dataset={managementVisit:'1',managementDocument:'2',managementRevision:'3'};
  root.__workflowManagement={composing:false,lease:null};
  const activity=node('workflow-activity',root);activity.dataset.activityRead='0';activity.dataset.pageRevision='7';
  const form=node('workflow-activity-filters',activity,'FORM');form.dataset.revision='6';
  const row=node('workflow-activity-run-EXECUTION',activity,'BUTTON');
  
  const input=node('workflow-activity-user-search',form,'INPUT');input.dataset.generation='5';input.setNative(expected.search);
  const fields=Object.fromEntries(['status','from','to','limit','user'].map(k=>{const n=node('workflow-activity-'+k,form);n.setNative(expected.filters[k]);return[k,n];}));
  const dom={modal:null},sent=[],queue=[],receivers=[];
  const channel={send:v=>sent.push(v),recv:()=>queue.length?Promise.resolve(queue.shift()):new Promise(resolve=>receivers.push(resolve))};
  class MutationObserver{constructor(fn){this.fn=fn;this.active=false;observers.push(this);}observe(){this.active=true;}disconnect(){this.active=false;}}
  const realm=vm.createContext({Date,document:{documentElement:{},getElementById:id=>map.get(id),querySelector:()=>dom.modal},MutationObserver,
    requestAnimationFrame:fn=>{const id=++sequence;frames.set(id,fn);return id;},cancelAnimationFrame:id=>frames.delete(id),
    setTimeout:(fn,ms)=>{const id=++sequence;timers.set(id,{fn,ms});return id;},clearTimeout:id=>timers.delete(id)});
  vm.runInContext(dateSource+"\n"+source,realm);let task;
  const s={expected,root,activity,form,row,input,fields,dom,map,node,sent,channel,frames,timers,
    start(){task=realm.workflowActivityExecution(expected,channel);return task;},
    command(value){if(receivers.length)receivers.shift()(value);else queue.push(value);},
    mutate(){for(const o of observers.filter(o=>o.active))o.fn([{type:"attributes",target:activity}],o);},
    event(e){for(const {listeners}of events)for(const f of [...(listeners.get(e)??[])])f({type:e});},
    frame(){const a=[...frames.values()];frames.clear();a.forEach(f=>f());},
    expire(ms){for(const[id,t]of [...timers])if(t.ms===ms){timers.delete(id);t.fn();}},
    async captured(){s.start();await flush();s.frame();await flush();assert.deepEqual(JSON.parse(JSON.stringify(sent)),[{serial:0,ok:true}]);assert.equal(root.inert,true);},
    commitDetail(){activity.dataset.activityRead='77';input.disabled=true;map.delete(row.id);row.isConnected=false;
      const detail=node('workflow-activity-detail',activity,'SECTION');detail.dataset={detailRevision:'8',detailLoaded:'false'};s.detail=detail;s.mutate();},
    async active(){await s.captured();s.commitDetail();s.command({kind:'activate',serial:1,read:77,detail:'8'});await flush();assert.equal(sent.at(-1).ok,true);
      s.command({kind:'unlock',serial:0});await flush();s.frame();await flush();assert.equal(root.inert,false);},
    retire(loaded=true){activity.dataset.activityRead='0';input.disabled=false;s.detail.dataset.detailLoaded=String(loaded);s.mutate();},
    async lock(serial=2){s.command({kind:'lock',serial});await flush();s.frame();await flush();return sent.at(-1);},
    async cancel(){s.command(null);await flush();s.frame();await task;s.clean();},
    clean(){for(const n of [root,activity,form])assert.equal(n.inert,false);assert.equal(root.__workflowManagement.lease,null);
      assert.equal(observers.filter(o=>o.active).length,0);assert.equal(frames.size,0);assert.equal(timers.size,0);
      assert.equal(events.reduce((a,{listeners})=>a+[...listeners.values()].reduce((b,set)=>b+set.size,0),0),0);},
    result:()=>task};return s;
}
async function refused(change){const s=scene();change(s);s.start();await flush();assert.equal(s.sent[0].ok,false);await s.cancel();return s;}
await test('list-only capture activates the exact rendered detail and unlocks during IO',async()=>{const s=scene();assert.equal(s.map.has('workflow-activity-detail'),false);await s.active();assert.equal(s.root.inert,false);assert.equal(s.input.disabled,true);assert.equal(s.dom.focused,undefined);await s.cancel();});
await test('MutationObserver record arrays after unlock revalidate custody without pretending user interaction',async()=>{
 const s=scene();await s.active();s.mutate();assert.equal((await s.lock()).ok,true);
 s.retire();s.command({kind:'close',serial:3,lease:2,focus:true});await flush();s.frame();await flush();
 assert.equal(s.sent.at(-1).ok,true);assert.equal(s.dom.focused,s.detail);await s.cancel();
});
await test('accepted page waits for original list read retirement, enabled search and matching page row',async()=>{
 const s=scene();s.activity.dataset.activityRead='17';s.activity.dataset.pageRevision='6';s.input.disabled=true;s.map.delete(s.row.id);s.start();await flush();s.frame();await flush();assert.equal(s.sent.length,0);
 s.activity.dataset.activityRead='0';s.mutate();await flush();assert.equal(s.sent.length,0);
 s.input.disabled=false;s.activity.dataset.pageRevision='7';s.mutate();await flush();assert.equal(s.sent.length,0);
 s.map.set(s.row.id,s.row);s.mutate();await flush();s.frame();await flush();assert.equal(s.sent[0].ok,true);await s.cancel();
});
await test('missing actual row times out without fabricating a node or focus',async()=>{const s=scene();s.map.delete(s.row.id);s.start();await flush();s.expire(1000);await flush();assert.equal(s.sent[0].ok,false);assert.equal(s.dom.focused,undefined);await s.cancel();});
await test('another list read cannot be mistaken for the accepted read retirement',()=>refused(s=>{s.activity.dataset.activityRead='99';}));
await test('initial retired read cannot advance to a different read while waiting',async()=>{const s=scene();s.activity.dataset.activityRead='17';s.start();await flush();s.activity.dataset.activityRead='99';s.mutate();await flush();assert.equal(s.sent[0].ok,false);await s.cancel();});
for(const key of ['visit','document','documentRevision','memberGeneration'])await test('initial '+key+' mismatch refuses without writes',()=>refused(s=>{s.expected[key]=99;}));
for(const key of ['status','from','to','limit','user'])await test('initial native-ahead '+key+' refuses without input writes',()=>refused(s=>s.fields[key].setNative('newer')));
await test('IME modal badInput and invalid calendar dates refuse',async()=>{for(const change of [s=>{s.root.__workflowManagement.composing=true;},s=>{s.dom.modal={};},s=>{s.input.validity.badInput=true;},s=>{s.expected.filters.from='2026-02-30';s.fields.from.setNative('2026-02-30');},s=>{s.fields.from.fieldsetDisabled=true;}])await refused(change);});
await test('activation waits exact detail mount and owned read instead of trusting Rust preparation',async()=>{
 const s=scene();await s.captured();s.command({kind:'activate',serial:1,read:77,detail:'8'});await flush();assert.equal(s.sent.length,1);
 s.activity.dataset.activityRead='77';s.input.disabled=true;s.mutate();await flush();assert.equal(s.sent.length,1);
 s.commitDetail();await flush();assert.equal(s.sent.at(-1).ok,true);await s.cancel();
});
await test('activation without a matching detail times out and cleans original hold',async()=>{const s=scene();await s.captured();s.command({kind:'activate',serial:1,read:77,detail:'8'});await flush();s.expire(1000);await flush();assert.equal(s.sent.at(-1).ok,false);await s.cancel();});
for(const target of ['root','activity','form','input','detail'])await test('original '+target+' replacement retires loaded-detail focus',async()=>{
 const s=scene();await s.active();const old=s[target],next=s.node(old.id,old.parent,old.tagName);next.dataset={...old.dataset};next.setNative(old.value);s.mutate();assert.equal((await s.lock()).ok,false);assert.equal(s.dom.focused,undefined);assert.equal(next.inert,false);await s.cancel();
});
for(const event of ['input','change','compositionstart','pointerdown','keydown','focusin'])await test(event+' during IO prevents automatic focus even after buffers restore',async()=>{
 const s=scene();await s.active();s.event(event);assert.equal((await s.lock()).ok,false);assert.equal(s.dom.focused,undefined);await s.cancel();
});
await test('changed row/page revision cannot publish through original guard',async()=>{const s=scene();await s.active();s.activity.dataset.pageRevision='9';s.mutate();assert.equal((await s.lock()).ok,false);await s.cancel();});
await test('successful detail waits marker retirement and loaded DOM before focusing original section once',async()=>{
 const s=scene();await s.active();assert.equal((await s.lock(2)).ok,true);s.command({kind:'close',serial:3,lease:2,focus:true});await flush();s.frame();await flush();assert.equal(s.sent.at(-1).serial,2);assert.equal(s.dom.focused,undefined);
 s.activity.dataset.activityRead='0';s.mutate();await flush();assert.equal(s.sent.at(-1).serial,2);
 s.input.disabled=false;s.mutate();await flush();assert.equal(s.sent.at(-1).serial,2);
 s.detail.dataset.detailLoaded='true';s.mutate();await flush();assert.equal(s.dom.focused,undefined);s.frame();await flush();assert.equal(s.sent.at(-1).ok,true);assert.equal(s.dom.focused,s.detail);assert.equal(s.dom.focusCount,1);s.clean();await s.cancel();
});
await test('failed detail cleanup acknowledges retirement without stealing focus',async()=>{const s=scene();await s.active();await s.lock();s.retire(false);s.command({kind:'close',serial:3,lease:2,focus:false});await flush();s.frame();await flush();assert.equal(s.sent.at(-1).ok,true);assert.equal(s.dom.focused,undefined);await s.cancel();});
await test('loaded-detail cleanup revalidates original nodes after delayed paint',async()=>{const s=scene();await s.active();await s.lock();s.retire();s.command({kind:'close',serial:3,lease:2,focus:true});await flush();s.detail.dataset.detailRevision='9';s.mutate();s.frame();await flush();assert.equal(s.sent.at(-1).ok,false);assert.equal(s.dom.focused,undefined);await s.cancel();});
await test('close times out if loaded content or retirement never reaches original DOM',async()=>{for(const retire of [false,true]){const s=scene();await s.active();await s.lock();if(retire)s.retire(false);s.command({kind:'close',serial:3,lease:2,focus:true});await flush();s.expire(1000);await flush();s.frame();await flush();assert.equal(s.sent.at(-1).ok,false);assert.equal(s.dom.focused,undefined);await s.cancel();}});
await test('foreign read during terminal wait refuses and restores only owned nodes',async()=>{const s=scene();await s.active();await s.lock();s.command({kind:'close',serial:3,lease:2,focus:true});await flush();s.activity.dataset.activityRead='88';s.mutate();await flush();s.frame();await flush();assert.equal(s.sent.at(-1).ok,false);assert.equal(s.dom.focused,undefined);await s.cancel();});
await test('cancellation while waiting retirement never focuses and cleans all watchers',async()=>{const s=scene();await s.active();await s.lock();s.command({kind:'close',serial:3,lease:2,focus:true});await flush();s.command(null);await flush();await s.result();assert.equal(s.sent.at(-1).ok,false);assert.equal(s.dom.focused,undefined);s.clean();});
await test('mount cancellation during held lock releases exact original inert properties',async()=>{const s=scene();await s.active();await s.lock();s.root.isConnected=false;s.root.__workflowManagement.lease.close();await flush();s.frame();await s.result();s.clean();});
await test('foreign lease never overwritten by automatic detail guard',async()=>{const s=scene();await s.active();const foreign={};s.root.__workflowManagement.lease=foreign;assert.equal((await s.lock()).ok,false);assert.equal(s.root.__workflowManagement.lease,foreign);s.root.__workflowManagement.lease=null;await s.cancel();});
await test('owned search disabled permits IO but disabled filter remains refusal',async()=>{const s=scene();await s.active();s.fields.status.disabled=true;s.mutate();assert.equal((await s.lock()).ok,false);await s.cancel();});
await test('initial bounded paint timeout refuses and restores original controls',async()=>{const s=scene();s.start();await flush();s.expire(1000);await flush();s.expire(200);await flush();assert.equal(s.sent[0].ok,false);await s.cancel();});
console.log(JSON.stringify({ok:true,count:tests.length,sourceSha256:crypto.createHash('sha256').update(source).digest('hex'),tests}));
