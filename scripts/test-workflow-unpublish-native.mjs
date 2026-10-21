import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const base='src/desktop/app_home/about/management/';
const source=fs.readFileSync(base+'native.js','utf8')+'\n'+fs.readFileSync(base+'unpublish/native.js','utf8');
let passed=0;
function scene() {
  const expected={id:7,visit:11,document:13,revision:17,title:'  Draft \u4e2d e\u0301  ',description:'Retained\n\u{1f98a}'};
  const title={value:expected.title,readOnly:false,isConnected:true,validity:{badInput:false},selectionStart:2,selectionEnd:5};
  const description={...title,value:expected.description};
  const notice={textContent:''};
  const root={isConnected:true,dataset:{managementVisit:'11',managementDocument:'13',managementRevision:'17',managementEditable:'true'},
    __workflowManagement:{composing:false,lease:null},
    querySelector(id){return nodes[id]??null;},contains(n){return Object.values(nodes).includes(n);}};
  const nodes={'#workflow-management-title':title,'#workflow-management-description':description,'#workflow-unpublish-native-notice':notice};
  const document={root,modal:null,getElementById(){return this.root;},querySelector(){return this.modal;}};
  const context=vm.createContext({document,JSON,Promise});vm.runInContext(source,context);
  const sends=[];let release;const channel={send(x){sends.push(x);},recv(){return new Promise(resolve=>release=resolve);}};
  return {expected,title,description,notice,root,nodes,document,context,channel,sends,release(){release?.();}};
}
async function test(name,run){await run();passed++;console.log('PASS '+name);}
await test('original confirmation retains actual nodes and Cancel restores editing even without revision change',async()=>{
 const s=scene(),p=s.context.workflowUnpublishLock(s.expected,s.channel);assert.deepEqual(s.sends,[true]);
 assert.equal(s.context.workflowUnpublishCurrent(s.expected),true);assert.equal(s.title.readOnly,true);
 assert.equal(s.description.readOnly,true);s.release();await p;
 assert.equal(s.title.readOnly,false);assert.equal(s.description.readOnly,false);
 assert.equal(s.title.value,s.expected.title);assert.equal(s.description.value,s.expected.description);
 assert.equal(s.title.selectionStart,2);assert.equal(s.title.selectionEnd,5);assert.equal(s.root.__workflowManagement.unpublishConfirmation,undefined);
});
for(const [name,change] of [
 ['native title ahead',s=>s.title.value+=' newer'],['native description ahead',s=>s.description.value+=' newer'],
 ['IME composition',s=>s.root.__workflowManagement.composing=true],
 ['same-ID title replacement',s=>s.nodes['#workflow-management-title']={...s.title}],
 ['same-ID description replacement',s=>s.nodes['#workflow-management-description']={...s.description}],
 ['same-ID root replacement',s=>s.document.root={...s.root}],
 ['document revision',s=>s.root.dataset.managementRevision='18'],
 ['visit',s=>s.root.dataset.managementVisit='12'],['document',s=>s.root.dataset.managementDocument='14'],
 ['modal',s=>s.document.modal={}],['bad native input',s=>s.title.validity.badInput=true],
 ['detached original',s=>s.title.isConnected=false],
 ['replaced lease',s=>s.root.__workflowManagement.lease={}],
 ]) await test(name+' refuses confirmation without replacing native buffers',async()=>{
  const s=scene(),p=s.context.workflowUnpublishLock(s.expected,s.channel);assert.deepEqual(s.sends,[true]);change(s);
  const before=[s.title.value,s.description.value,s.title.selectionStart,s.title.selectionEnd];
  assert.equal(s.context.workflowUnpublishCheck(s.expected),false);
  assert.deepEqual([s.title.value,s.description.value,s.title.selectionStart,s.title.selectionEnd],before);
  s.release();await p;
  assert.deepEqual([s.title.value,s.description.value,s.title.selectionStart,s.title.selectionEnd],before);
 });
await test('second admission cannot replace original confirmation or lease',async()=>{
 const s=scene(),p=s.context.workflowUnpublishLock(s.expected,s.channel),saved=s.root.__workflowManagement.unpublishConfirmation,lease=s.root.__workflowManagement.lease;
 const other=[];await s.context.workflowUnpublishLock({...s.expected,id:8},{send:v=>other.push(v),recv:()=>Promise.resolve()});
 assert.deepEqual(other,[false]);assert.equal(s.root.__workflowManagement.unpublishConfirmation,saved);assert.equal(s.root.__workflowManagement.lease,lease);
 assert.equal(s.context.workflowUnpublishCurrent(s.expected),true);s.release();await p;
});
await test('initial ahead native buffer never admits or overwrites',async()=>{
 const s=scene();s.title.value+=' ahead';const before=s.title.value;
 await s.context.workflowUnpublishLock(s.expected,s.channel);assert.deepEqual(s.sends,[false]);assert.equal(s.title.value,before);
 assert.equal(s.root.__workflowManagement.lease,null);assert.equal(s.title.readOnly,false);
});
await test('retired cleanup does not unlock same-ID replacement controls',async()=>{
 const s=scene(),p=s.context.workflowUnpublishLock(s.expected,s.channel);
 const replacement={...s.title,readOnly:true};s.nodes['#workflow-management-title']=replacement;s.release();await p;
 assert.equal(replacement.readOnly,true);assert.equal(replacement.value,s.expected.title);
});
await test('policy-disabled current surface stays readonly on cancellation',async()=>{
 const s=scene(),p=s.context.workflowUnpublishLock(s.expected,s.channel);s.root.dataset.managementEditable='false';s.release();await p;
 assert.equal(s.title.readOnly,true);assert.equal(s.description.readOnly,true);
});
await test('finish-only snapshot retains ahead text but does not admit it for mutation',async()=>{
 const s=scene(),p=s.context.workflowUnpublishLock(s.expected,s.channel);s.title.value+=' latest';s.description.value+=' latest';
 const captured=s.context.workflowUnpublishFinish(s.expected);
 assert.equal(captured.title,s.title.value);assert.equal(captured.description,s.description.value);
 assert.equal(s.context.workflowUnpublishCurrent(s.expected),false);
 let restored=null;s.title.setSelectionRange=(...args)=>restored=args;
 s.release();await p;assert.deepEqual(restored,[2,5,undefined]);
});
for(const [name,change] of [
 ['IME',s=>s.root.__workflowManagement.composing=true],
 ['same-ID title replacement',s=>s.nodes['#workflow-management-title']={...s.title}],
 ['same-ID description replacement',s=>s.nodes['#workflow-management-description']={...s.description}],
 ['same-ID root replacement',s=>s.document.root={...s.root}],
 ['retired revision',s=>s.root.dataset.managementRevision='18'],
]) await test('finish snapshot refuses '+name+' but releases original busy lease',async()=>{
 const s=scene(),p=s.context.workflowUnpublishLock(s.expected,s.channel);change(s);
 const before=[s.title.value,s.description.value];assert.equal(s.context.workflowUnpublishFinish(s.expected),null);
 s.release();await p;assert.equal(s.root.__workflowManagement.lease,null);
 assert.deepEqual([s.title.value,s.description.value],before);
});

await test('sealed operation readback owns actual readonly controls and verified baseline restores editing',async()=>{
 const s=scene();s.root.dataset.managementEditable='false';s.title.readOnly=true;s.description.readOnly=true;
 const p=s.context.workflowUnpublishLock(s.expected,s.channel);assert.deepEqual(s.sends,[true]);
 assert.equal(s.context.workflowUnpublishCurrent(s.expected),true);
 const finish=s.context.workflowUnpublishFinish(s.expected);assert.equal(finish.title,s.expected.title);
 s.root.dataset.managementRevision='18';s.root.dataset.managementEditable='true';
 s.release();await p;assert.equal(s.title.readOnly,false);assert.equal(s.description.readOnly,false);
 assert.equal(s.title.value,s.expected.title);assert.equal(s.description.value,s.expected.description);
});
await test('matching partial readback cannot unlock policy sealed fields on lease release',async()=>{
 const s=scene();s.root.dataset.managementEditable='false';
 const p=s.context.workflowUnpublishLock(s.expected,s.channel);assert.deepEqual(s.sends,[true]);
 assert.equal(s.context.workflowUnpublishFinish(s.expected).title,s.expected.title);
 s.root.dataset.managementRevision='18';s.release();await p;
 assert.equal(s.title.readOnly,true);assert.equal(s.description.readOnly,true);
 assert.equal(s.root.__workflowManagement.lease,null);
});
// Model the pinned desktop query owner's lifetime: completion releases the
// channel, so a queued IPC message must be consumed before finalization/drop.
for (const value of [true,false,null,{title:'Native \u4e2d',description:'Retained'}]) {
 await test('one proof survives delayed Rust polling until original-channel release: '+JSON.stringify(value),async()=>{
  const s=scene();let closed=false,release;const queued=[];
  const channel={send(v){queued.push(v);},recv(){return new Promise(resolve=>release=resolve);}};
  const old=async()=>{channel.send(value);};
  await old().finally(()=>closed=true);
  assert.equal(closed,true,'old send-then-return permits owner drop before polling');
  closed=false;queued.length=0;
  const promise=s.context.workflowUnpublishReply(value,channel).finally(()=>closed=true);
  await new Promise(resolve=>setImmediate(resolve));
  assert.equal(closed,false);assert.equal(queued.length,1);assert.equal(queued[0],value);
  assert.equal(s.root.__workflowManagement.lease,null,'proof channel never replaces mutation lease');
  release();await promise;assert.equal(closed,true);assert.equal(queued.length,1,'no reevaluation or resend');
 });
}
console.log(`${passed} workflow Unpublish native tests passed`);
