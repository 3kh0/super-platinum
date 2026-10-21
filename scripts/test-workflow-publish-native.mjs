import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const base='src/desktop/app_home/about/management/';
const source=fs.readFileSync(base+'native.js','utf8')+'\n'+fs.readFileSync(base+'publish/native.js','utf8');
function scene(){
  const expected={id:7,visit:11,document:13,revision:17,title:'Saved 中 e\u0301',description:'Saved\n🦊'};
  const title={value:expected.title,readOnly:false,isConnected:true,validity:{badInput:false},selectionStart:2,selectionEnd:5,selectionDirection:'forward'};
  const description={...title,validity:{badInput:false},value:expected.description};
  const nodes={'#workflow-management-title':title,'#workflow-management-description':description};
  const root={isConnected:true,dataset:{managementVisit:'11',managementDocument:'13',managementRevision:'17',managementEditable:'true'},
    __workflowManagement:{composing:false,lease:null},querySelector(id){return nodes[id]??null;},contains(n){return Object.values(nodes).includes(n);}};
  const document={root,modal:null,getElementById(){return this.root;},querySelector(){return this.modal;}};
  const context=vm.createContext({document,JSON,Promise});vm.runInContext(source,context);
  let release;const sends=[],channel={send(v){sends.push(v);},recv(){return new Promise(r=>release=r);}};
  return {expected,title,description,nodes,root,document,context,sends,channel,release(){release?.();}};
}
let count=0;
async function test(name,run){await run();count++;console.log('PASS '+name);}
await test('direct publication owns original controls without a confirmation and releases unchanged revision',async()=>{
  const s=scene(),task=s.context.workflowPublishLock(s.expected,s.channel);
  assert.deepEqual(s.sends,[true]);assert.equal(s.context.workflowPublishCurrent(s.expected),true);
  assert.equal(s.title.readOnly,true);assert.equal(s.description.readOnly,true);
  s.release();await task;assert.equal(s.title.readOnly,false);assert.equal(s.description.readOnly,false);
  assert.equal(s.root.__workflowManagement.publication,undefined);
});
for(const [name,mutate] of [
  ['ahead title',s=>s.title.value+=' unsaved'],['ahead description',s=>s.description.value+=' unsaved'],
  ['IME',s=>s.root.__workflowManagement.composing=true],['bad input',s=>s.title.validity.badInput=true],
  ['same-ID title',s=>s.nodes['#workflow-management-title']={...s.title}],
  ['same-ID description',s=>s.nodes['#workflow-management-description']={...s.description}],
  ['same-ID root',s=>s.document.root={...s.root}],['revision',s=>s.root.dataset.managementRevision='18'],
  ['document',s=>s.root.dataset.managementDocument='14'],['visit',s=>s.root.dataset.managementVisit='12'],
  ['lease',s=>s.root.__workflowManagement.lease={}],['modal',s=>s.document.modal={}],
  ['detached node',s=>s.title.isConnected=false],
])await test('post-await '+name+' refuses mutation without changing text',async()=>{
  const s=scene(),task=s.context.workflowPublishLock(s.expected,s.channel);mutate(s);
  const before=[s.title.value,s.description.value,s.title.selectionStart,s.title.selectionEnd];
  assert.equal(s.context.workflowPublishCurrent(s.expected),false);s.release();await task;
  assert.deepEqual([s.title.value,s.description.value,s.title.selectionStart,s.title.selectionEnd],before);
});
for(const field of ['title','description'])await test('initial unsaved native '+field+' refuses admission',async()=>{
  const s=scene();s[field].value+=' ahead';const before=s[field].value;
  await s.context.workflowPublishLock(s.expected,s.channel);assert.deepEqual(s.sends,[false]);
  assert.equal(s[field].value,before);assert.equal(s.root.__workflowManagement.lease,null);
  assert.equal(s[field].readOnly,false);
});
await test('finish preserves ahead buffer and selection but never grants current authority',async()=>{
  const s=scene(),task=s.context.workflowPublishLock(s.expected,s.channel);s.title.value+=' latest';
  const value=s.context.workflowPublishFinish(s.expected);assert.equal(value.title,s.title.value);
  assert.equal(s.context.workflowPublishCurrent(s.expected),false);
  let selection;s.title.setSelectionRange=(...args)=>selection=args;
  s.release();await task;assert.deepEqual(selection,[2,5,'forward']);
});
for(const [name,mutate] of [
  ['IME',s=>s.root.__workflowManagement.composing=true],
  ['replaced controls',s=>s.nodes['#workflow-management-title']={...s.title,readOnly:true}],
])await test('unsafe finish '+name+' returns no buffer and releases only original lease',async()=>{
  const s=scene(),task=s.context.workflowPublishLock(s.expected,s.channel);mutate(s);
  assert.equal(s.context.workflowPublishFinish(s.expected),null);s.release();await task;
  assert.equal(s.root.__workflowManagement.lease,null);
  if(name==='replaced controls')assert.equal(s.nodes['#workflow-management-title'].readOnly,true);
});
await test('later action cannot replace original busy publication lease',async()=>{
  const s=scene(),task=s.context.workflowPublishLock(s.expected,s.channel),original=s.root.__workflowManagement.publication;
  const second=[];await s.context.workflowPublishLock({...s.expected,id:8},{send:v=>second.push(v),recv:()=>Promise.resolve()});
  assert.deepEqual(second,[false]);assert.equal(s.root.__workflowManagement.publication,original);s.release();await task;
});
await test('sealed readback stays readonly until actual editing policy changes',async()=>{
  const s=scene();s.root.dataset.managementEditable='false';
  const task=s.context.workflowPublishLock(s.expected,s.channel);assert.deepEqual(s.sends,[true]);
  assert.equal(s.context.workflowPublishCurrent(s.expected),true);s.release();await task;assert.equal(s.title.readOnly,true);
});
for(const value of [true,false,null,{title:'中',description:'Saved'}])await test('proof channel remains alive through delayed receive '+JSON.stringify(value),async()=>{
  const s=scene(),queued=[];let release,closed=false;
  const task=s.context.workflowPublishReply(value,{send:v=>queued.push(v),recv:()=>new Promise(r=>release=r)}).finally(()=>closed=true);
  await new Promise(r=>setImmediate(r));assert.equal(closed,false);assert.equal(queued[0],value);
  assert.equal(s.root.__workflowManagement.lease,null);release();await task;assert.equal(closed,true);assert.equal(queued.length,1);
});
console.log(`${count} workflow Publish native bridge tests passed`);
