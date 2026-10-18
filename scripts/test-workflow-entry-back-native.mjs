import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import crypto from 'node:crypto';
const url=new URL('../src/desktop/app_home/about/management/entry/back.js',import.meta.url);
const source=fs.readFileSync(url,'utf8'),names=[];
const flush=async()=>{for(let i=0;i<8;i++)await Promise.resolve();};
async function test(name,body){await body();names.push(name);console.log(`PASS ${name}`);}
function scene(){
  const ids=new Map(),listeners=new Map(),frames=new Map(),timers=new Map();let serial=0,release,task;
  const root={id:'workflow-management',isConnected:true,inert:false,dataset:{managementVisit:'1',managementDocument:'2',managementRevision:'3'},
    __workflowManagement:{composing:false,lease:null},contains:n=>n.parent===root,
    addEventListener:(name,fn)=>{if(!listeners.has(name))listeners.set(name,new Set());listeners.get(name).add(fn);},
    removeEventListener:(name,fn)=>listeners.get(name)?.delete(fn)};
  ids.set(root.id,root);
  const node=(id,value='')=>{const n={id,parent:root,isConnected:true,disabled:true,validity:{badInput:false},textContent:'',setNative:v=>{value=v;}};
    Object.defineProperty(n,'value',{get:()=>value,set:()=>{throw Error('buffer write');}});ids.set(id,n);return n;};
  const notice=node('workflow-entry-navigation-notice');
  const expected={visit:1,document:2,revision:3,fields:{'workflow-management-title':'Draft 中','workflow-management-description':'Exact\n🙂','workflow-activity-status':'failed'}};
  const fields=Object.fromEntries(Object.entries(expected.fields).map(([id,v])=>[id,node(id,v)]));
  const dom={modal:false};
  const realm=vm.createContext({document:{getElementById:id=>ids.get(id),querySelector:()=>dom.modal},
    requestAnimationFrame:fn=>{const id=++serial;frames.set(id,fn);return id;},cancelAnimationFrame:id=>frames.delete(id),
    setTimeout:fn=>{const id=++serial;timers.set(id,fn);return id;},clearTimeout:id=>timers.delete(id)});
  vm.runInContext(source,realm,{filename:url.pathname});
  const sent=[],promise=new Promise(r=>release=r);
  const s={root,notice,expected,fields,dom,ids,node,sent,start(){task=realm.workflowEntryBack(expected,{send:v=>sent.push(v),recv:()=>promise});return task;},
    frame(){const list=[...frames.values()];frames.clear();list.forEach(fn=>fn());},
    event(name){for(const fn of listeners.get(name)??[])fn();},
    async finish(){release();await flush();s.frame();await task;s.clean();},
    clean(){assert.equal([...listeners.values()].reduce((n,v)=>n+v.size,0),0);assert.equal(frames.size,0);assert.equal(timers.size,0);},
  };return s;
}
async function refused(change,late=false){const s=scene();if(!late)change(s);s.start();if(late)change(s);s.frame();await flush();assert.deepEqual(s.sent,[false]);await s.finish();return s;}
await test('disabled Activity controls are compared and successful lease retains exact buffers',async()=>{
  const s=scene();s.start();assert.equal(s.root.inert,true);assert.ok(s.root.__workflowManagement.lease);s.frame();await flush();assert.deepEqual(s.sent,[true]);
  for(const [id,value] of Object.entries(s.expected.fields))assert.equal(s.fields[id].value,value);
  await s.finish();assert.equal(s.root.inert,false);assert.equal(s.root.__workflowManagement.lease,null);
});
for(const [name,change] of Object.entries({
  title:s=>s.fields['workflow-management-title'].setNative('ahead'),
  description:s=>s.fields['workflow-management-description'].setNative('ahead'),
  disabledFilter:s=>s.fields['workflow-activity-status'].setNative('success'),
  badInput:s=>{s.fields['workflow-management-title'].validity.badInput=true;},
  composing:s=>{s.root.__workflowManagement.composing=true;},modal:s=>{s.dom.modal=true;},
  visit:s=>{s.root.dataset.managementVisit='99';},document:s=>{s.root.dataset.managementDocument='99';},revision:s=>{s.root.dataset.managementRevision='99';},
  fieldReplacement:s=>s.node('workflow-management-title','Draft 中'),
  moved:s=>{s.fields['workflow-management-title'].parent=null;},detached:s=>{s.root.isConnected=false;},
  stateReplacement:s=>{s.root.__workflowManagement={composing:false,lease:null};},
}))for(const late of [false,true]){
  // A new management state before capture is valid; only replacement after capture retires it.
  if(['stateReplacement','fieldReplacement'].includes(name)&&!late)continue;
  await test(`${name} ${late?'during lease':'before lease'} refuses`,async()=>{await refused(change,late);});
}
for(const event of ['input','change','compositionstart'])await test(`${event} during lease retires even unchanged buffers`,async()=>{
  await refused(s=>s.event(event),true);
});
await test('foreign lease and original inert state are preserved',async()=>{
  const lease={foreign:true};const s=await refused(s=>{s.root.__workflowManagement.lease=lease;});assert.equal(s.root.__workflowManagement.lease,lease);
  const inert=await refused(s=>{s.root.inert=true;});assert.equal(inert.root.inert,true);
});
await test('a successor lease is not cleared by original cleanup',async()=>{
  const s=scene();s.start();const successor={};s.root.__workflowManagement.lease=successor;s.frame();await flush();await s.finish();assert.deepEqual(s.sent,[false]);assert.equal(s.root.__workflowManagement.lease,successor);assert.equal(s.root.inert,true);
});
await test('management mount close releases original lease after a render opportunity',async()=>{
  const s=scene();s.start();s.frame();await flush();assert.deepEqual(s.sent,[true]);
  s.root.__workflowManagement.lease.close();await flush();assert.equal(s.root.inert,true);
  s.frame();await s.finish();assert.equal(s.root.inert,false);assert.equal(s.root.__workflowManagement.lease,null);
});
console.log(JSON.stringify({passed:names.length,failed:0,source:url.pathname,sha256:crypto.createHash('sha256').update(source).digest('hex'),tests:names}));
