import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const base='src/desktop/app_home/about/management/';
const source=fs.readFileSync(base+'native.js','utf8')+'\n'+fs.readFileSync(base+'permissions/native.js','utf8');
let passed=0;
function scene() {
  const expected={id:7,visit:11,document:13,revision:17,title:'  Draft \u4e2d e\u0301  ',description:'Retained\n\u{1f98a}'};
  const title={value:expected.title,readOnly:false,isConnected:true,validity:{badInput:false},selectionStart:2,selectionEnd:5};
  const description={...title,value:expected.description};
  const notice={textContent:''};
  const root={isConnected:true,dataset:{managementVisit:'11',managementDocument:'13',managementRevision:'17',managementEditable:'true'},
    __workflowManagement:{composing:false,lease:null},
    querySelector(id){return nodes[id]??null;},contains(n){return Object.values(nodes).includes(n);}};
  const nodes={'#workflow-management-title':title,'#workflow-management-description':description,'#workflow-permissions-native-notice':notice};
  const choice={tagName:'SELECT',value:'collaborators',disabled:false,isConnected:true,validity:{badInput:false}};
  const panel={dataset:{confirmation:'7'},contains:n=>n===choice};
  nodes['#workflow-permissions']=panel;nodes['#workflow-permissions-copy']=choice;
  const document={root,modal:null,getElementById(){return this.root;},querySelector(){return this.modal;}};
  const context=vm.createContext({document,JSON,Promise});vm.runInContext(source,context);
  const sends=[];let release;const channel={send(x){sends.push(x);},recv(){return new Promise(resolve=>release=resolve);}};
  return {expected,title,description,notice,root,nodes,document,context,channel,sends,choice,panel,release(){release?.();}};
}
async function test(name,run){await run();passed++;console.log('PASS '+name);}
await test('original confirmation retains actual nodes and Cancel restores editing even without revision change',async()=>{
 const s=scene(),p=s.context.workflowPermissionsLock(s.expected,s.channel);assert.deepEqual(s.sends,[true]);
 assert.equal(s.context.workflowPermissionsCurrent(s.expected),true);assert.equal(s.title.readOnly,true);
 assert.equal(s.description.readOnly,true);s.release();await p;
 assert.equal(s.title.readOnly,false);assert.equal(s.description.readOnly,false);
 assert.equal(s.title.value,s.expected.title);assert.equal(s.description.value,s.expected.description);
 assert.equal(s.title.selectionStart,2);assert.equal(s.title.selectionEnd,5);assert.equal(s.root.__workflowManagement.permissionsConfirmation,undefined);
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
  const s=scene(),p=s.context.workflowPermissionsLock(s.expected,s.channel);assert.deepEqual(s.sends,[true]);change(s);
  const before=[s.title.value,s.description.value,s.title.selectionStart,s.title.selectionEnd];
  assert.equal(s.context.workflowPermissionsCheck(s.expected),false);
  assert.deepEqual([s.title.value,s.description.value,s.title.selectionStart,s.title.selectionEnd],before);
  s.release();await p;
  assert.deepEqual([s.title.value,s.description.value,s.title.selectionStart,s.title.selectionEnd],before);
 });
await test('second admission cannot replace original confirmation or lease',async()=>{
 const s=scene(),p=s.context.workflowPermissionsLock(s.expected,s.channel),saved=s.root.__workflowManagement.permissionsConfirmation,lease=s.root.__workflowManagement.lease;
 const other=[];await s.context.workflowPermissionsLock({...s.expected,id:8},{send:v=>other.push(v),recv:()=>Promise.resolve()});
 assert.deepEqual(other,[false]);assert.equal(s.root.__workflowManagement.permissionsConfirmation,saved);assert.equal(s.root.__workflowManagement.lease,lease);
 assert.equal(s.context.workflowPermissionsCurrent(s.expected),true);s.release();await p;
});
await test('initial ahead native buffer never admits or overwrites',async()=>{
 const s=scene();s.title.value+=' ahead';const before=s.title.value;
 await s.context.workflowPermissionsLock(s.expected,s.channel);assert.deepEqual(s.sends,[false]);assert.equal(s.title.value,before);
 assert.equal(s.root.__workflowManagement.lease,null);assert.equal(s.title.readOnly,false);
});
await test('retired cleanup does not unlock same-ID replacement controls',async()=>{
 const s=scene(),p=s.context.workflowPermissionsLock(s.expected,s.channel);
 const replacement={...s.title,readOnly:true};s.nodes['#workflow-management-title']=replacement;s.release();await p;
 assert.equal(replacement.readOnly,true);assert.equal(replacement.value,s.expected.title);
});
await test('policy-disabled current surface stays readonly on cancellation',async()=>{
 const s=scene(),p=s.context.workflowPermissionsLock(s.expected,s.channel);s.root.dataset.managementEditable='false';s.release();await p;
 assert.equal(s.title.readOnly,true);assert.equal(s.description.readOnly,true);
});
await test('finish-only snapshot retains ahead text but does not admit it for mutation',async()=>{
 const s=scene(),p=s.context.workflowPermissionsLock(s.expected,s.channel);s.title.value+=' latest';s.description.value+=' latest';
 const captured=s.context.workflowPermissionsFinish(s.expected);
 assert.equal(captured.title,s.title.value);assert.equal(captured.description,s.description.value);
 assert.equal(s.context.workflowPermissionsCurrent(s.expected),false);
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
 const s=scene(),p=s.context.workflowPermissionsLock(s.expected,s.channel);change(s);
 const before=[s.title.value,s.description.value];assert.equal(s.context.workflowPermissionsFinish(s.expected),null);
 s.release();await p;assert.equal(s.root.__workflowManagement.lease,null);
 assert.deepEqual([s.title.value,s.description.value],before);
});

await test('sealed operation readback owns actual readonly controls and verified baseline restores editing',async()=>{
 const s=scene();s.root.dataset.managementEditable='false';s.title.readOnly=true;s.description.readOnly=true;
 const p=s.context.workflowPermissionsLock(s.expected,s.channel);assert.deepEqual(s.sends,[true]);
 assert.equal(s.context.workflowPermissionsCurrent(s.expected),true);
 const finish=s.context.workflowPermissionsFinish(s.expected);assert.equal(finish.title,s.expected.title);
 s.root.dataset.managementRevision='18';s.root.dataset.managementEditable='true';
 s.release();await p;assert.equal(s.title.readOnly,false);assert.equal(s.description.readOnly,false);
 assert.equal(s.title.value,s.expected.title);assert.equal(s.description.value,s.expected.description);
});
await test('matching partial readback cannot unlock policy sealed fields on lease release',async()=>{
 const s=scene();s.root.dataset.managementEditable='false';
 const p=s.context.workflowPermissionsLock(s.expected,s.channel);assert.deepEqual(s.sends,[true]);
 assert.equal(s.context.workflowPermissionsFinish(s.expected).title,s.expected.title);
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
  const promise=s.context.workflowPermissionsReply(value,channel).finally(()=>closed=true);
  await new Promise(resolve=>setImmediate(resolve));
  assert.equal(closed,false);assert.equal(queued.length,1);assert.equal(queued[0],value);
  assert.equal(s.root.__workflowManagement.lease,null,'proof channel never replaces mutation lease');
  release();await promise;assert.equal(closed,true);assert.equal(queued.length,1,'no reevaluation or resend');
 });
}
await test('original chooser binds once, unchanged Save and changed Save preserve original metadata authority',async()=>{
 const s=scene(),p=s.context.workflowPermissionsLock(s.expected,s.channel);
 assert.equal(s.context.workflowPermissionsBind(s.expected),true);
 assert.equal(s.context.workflowPermissionsChoice(s.expected,'collaborators'),true);
 s.choice.value='everyone';assert.equal(s.context.workflowPermissionsChoice(s.expected,'everyone'),true);
 assert.equal(s.context.workflowPermissionsChoice(s.expected,'collaborators'),false);
 s.release();await p;assert.equal(s.title.readOnly,false);
});
for(const [name,change] of [
 ['same-ID chooser replacement',s=>s.nodes['#workflow-permissions-copy']={...s.choice}],
 ['same-ID panel replacement',s=>s.nodes['#workflow-permissions']={...s.panel}],
 ['unacknowledged selection',s=>s.choice.value='everyone'],
 ['invalid choice',s=>s.choice.value='named_entities'],
 ['disabled choice',s=>s.choice.disabled=true],
 ['ahead metadata',s=>s.title.value+=' ahead'],
 ['IME',s=>s.root.__workflowManagement.composing=true]
]) await test('Save/Cancel chooser witness refuses '+name,async()=>{
 const s=scene(),p=s.context.workflowPermissionsLock(s.expected,s.channel);assert.equal(s.context.workflowPermissionsBind(s.expected),true);change(s);
 const before=[s.title.value,s.description.value,s.choice.value];
 assert.equal(s.context.workflowPermissionsChoice(s.expected,'collaborators'),false);
 s.release();await p;assert.equal(s.root.__workflowManagement.lease,null);assert.deepEqual([s.title.value,s.description.value,s.choice.value],before);
});


function withSettings(s){
 const n=(tag,value='')=>({tagName:tag,value,disabled:false,isConnected:true,validity:{badInput:false},dataset:{}});
 const mode=n('SELECT','named_entities'),query=n('INPUT','run'),external=n('INPUT');external.checked=true;
 const externalQuery=n('INPUT','partner');
 const selected=n('BUTTON');selected.dataset.runSelected='UWORKFLOW';
 const result=n('BUTTON');result.dataset.runResult='URUNADA01';
 const externalResult=n('BUTTON');externalResult.dataset.runResult='TRUNEXTER';
 const fields={'#workflow-permissions-run':mode,'#workflow-permissions-target-query':query,'#workflow-permissions-external':external,'#workflow-permissions-external-query':externalQuery};
 const box={isConnected:true,dataset:{settingsEpoch:'19'},querySelector:k=>fields[k]??null,
  querySelectorAll:k=>k==='[data-run-selected]'?selectedNodes:resultNodes,
  contains:n=>n===box||Object.values(fields).includes(n)||selectedNodes.includes(n)||resultNodes.includes(n)};
 const selectedNodes=[selected],resultNodes=[result,externalResult];
 s.nodes['#workflow-permissions-run-settings']=box;s.panel.contains=n=>n===s.choice||n===box;
 const expected={epoch:19,copy:'collaborators',mode:'named_entities',ids:['UWORKFLOW'],visibleIds:['UWORKFLOW'],external:true,query:'run',externalQuery:'partner',results:['URUNADA01'],externalResults:['TRUNEXTER']};
 return Object.assign(s,{settingsExpected:expected,box,fields,selectedNodes,resultNodes,mode,query,external,externalQuery});
}
await test('run settings bind exact original chooser/chips/results and admit explicit actual selection',async()=>{
 const s=withSettings(scene()),p=s.context.workflowPermissionsLock(s.expected,s.channel);assert.equal(s.context.workflowPermissionsBind(s.expected),true);
 assert.equal(s.context.workflowPermissionsSettings(s.expected,s.settingsExpected,null),true);
 assert.equal(s.context.workflowPermissionsSettings(s.expected,s.settingsExpected,{kind:'add',value:'URUNADA01'}),true);
 assert.equal(s.context.workflowPermissionsSettings(s.expected,s.settingsExpected,{kind:'remove',value:'UWORKFLOW'}),true);
 assert.equal(s.context.workflowPermissionsSettings(s.expected,s.settingsExpected,{kind:'add',value:'UFORGED01'}),false);
 s.mode.value='everyone';assert.equal(s.context.workflowPermissionsSettings(s.expected,s.settingsExpected,null),false);
 assert.equal(s.context.workflowPermissionsSettings(s.expected,s.settingsExpected,{kind:'mode',value:'everyone'}),true);
 s.release();await p;assert.equal(s.title.readOnly,false);
});
for(const [name,change] of [
 ['run select replacement',s=>s.fields['#workflow-permissions-run']={...s.mode}],
 ['query replacement',s=>s.fields['#workflow-permissions-target-query']={...s.query}],
 ['missing named query',s=>delete s.fields['#workflow-permissions-target-query']],
 ['selected target replacement',s=>s.selectedNodes[0]={...s.selectedNodes[0]}],
 ['result replacement',s=>s.resultNodes[0]={...s.resultNodes[0]}],
 ['changed target identity',s=>s.selectedNodes[0].dataset.runSelected='UFORGED01'],
 ['ahead run mode',s=>s.mode.value='everyone'],
 ['ahead copy mode',s=>s.choice.value='everyone'],
 ['ahead target query',s=>s.query.value='new text'],
 ['ahead external state',s=>s.external.checked=false],
 ['ahead external query',s=>s.externalQuery.value='new partner'],
 ['missing selected target',s=>s.selectedNodes.length=0],
 ['reordered results',s=>s.resultNodes.reverse()],
 ['detached target',s=>s.selectedNodes[0].isConnected=false],
 ['disabled run selector',s=>s.mode.disabled=true],
 ['native IME',s=>s.root.__workflowManagement.composing=true],
 ['ahead metadata',s=>s.title.value+=' ahead'],
 ['settings epoch mismatch',s=>s.box.dataset.settingsEpoch='20'],
 ])await test('run Save/Cancel refuses '+name+' without altering native drafts',async()=>{
 const s=withSettings(scene()),p=s.context.workflowPermissionsLock(s.expected,s.channel);s.context.workflowPermissionsBind(s.expected);
 assert.equal(s.context.workflowPermissionsSettings(s.expected,s.settingsExpected,null),true);change(s);
 const before=[s.title.value,s.description.value,s.query.value,s.externalQuery.value];
 assert.equal(s.context.workflowPermissionsSettings(s.expected,s.settingsExpected,null),false);
 assert.deepEqual([s.title.value,s.description.value,s.query.value,s.externalQuery.value],before);
 s.release();await p;assert.equal(s.root.__workflowManagement.lease,null);
 });
await test('only newer acknowledged render can replace settings nodes; old callbacks cannot claim it',async()=>{
 const s=withSettings(scene()),p=s.context.workflowPermissionsLock(s.expected,s.channel);s.context.workflowPermissionsBind(s.expected);
 assert.equal(s.context.workflowPermissionsSettings(s.expected,s.settingsExpected,null),true);
 s.fields['#workflow-permissions-run']={...s.mode};s.box.dataset.settingsEpoch='20';const next={...s.settingsExpected,epoch:20};
 assert.equal(s.context.workflowPermissionsSettings(s.expected,next,{kind:'add',value:'URUNADA01'}),false);
 assert.equal(s.context.workflowPermissionsSettings(s.expected,next,null),true);
 assert.equal(s.context.workflowPermissionsSettings(s.expected,s.settingsExpected,null),false);
 s.box.dataset.settingsEpoch='19';assert.equal(s.context.workflowPermissionsSettings(s.expected,s.settingsExpected,null),false);
 s.release();await p;
});
await test('actual query input may advance its own buffer but never a different selector',async()=>{
 const s=withSettings(scene()),p=s.context.workflowPermissionsLock(s.expected,s.channel);s.context.workflowPermissionsBind(s.expected);
 s.context.workflowPermissionsSettings(s.expected,s.settingsExpected,null);s.query.value='@ada';
 assert.equal(s.context.workflowPermissionsSettings(s.expected,s.settingsExpected,{kind:'query',value:'@ada'}),true);
 s.mode.value='everyone';assert.equal(s.context.workflowPermissionsSettings(s.expected,s.settingsExpected,{kind:'query',value:'@ada'}),false);
 s.release();await p;
});

function renderClock(s) {
 let now=0,serial=0;const frames=new Map(),timers=new Map();
 s.context.Date={now:()=>now};
 s.context.requestAnimationFrame=cb=>{const id=++serial;frames.set(id,cb);return id;};
 s.context.cancelAnimationFrame=id=>frames.delete(id);
 s.context.setTimeout=(cb,delay)=>{const id=++serial;timers.set(id,{cb,at:now+delay});return id;};
 s.context.clearTimeout=id=>timers.delete(id);
 return {async frame(){now+=16;const pending=[...frames.values()];frames.clear();for(const cb of pending)cb();await Promise.resolve();},
  async expire(){now+=1500;for(const [id,t] of [...timers])if(t.at<=now){timers.delete(id);t.cb();}await Promise.resolve();},
  pending:()=>frames.size+timers.size};
}
function laggedBinding(){
 const s=withSettings(scene()),lease=s.context.workflowPermissionsLock(s.expected,s.channel);
 assert.equal(s.context.workflowPermissionsBind(s.expected),true);
 assert.equal(s.context.workflowPermissionsSettings(s.expected,s.settingsExpected,null),true);
 const next={...s.settingsExpected,epoch:20,copy:'everyone'};
 s.choice.value='everyone';
 return {s,lease,next,clock:renderClock(s)};
}
await test('passive binding waits for the exact render instead of losing the one-shot epoch bind',async()=>{
 const {s,lease,next,clock}=laggedBinding();
 assert.equal(s.context.workflowPermissionsSettings(s.expected,next,null),false,'old one-shot binding sees pre-render epoch');
 const proof=s.context.workflowPermissionsBindSettings(s.expected,next);
 await clock.frame();await clock.frame();
 assert.equal(s.root.__workflowManagement.permissionsConfirmation.runSettings.epoch,'19');
 s.box.dataset.settingsEpoch='20';await clock.frame();assert.equal(await proof,true);
 assert.equal(s.root.__workflowManagement.permissionsConfirmation.runSettings.epoch,'20');
 assert.equal(clock.pending(),0);
 s.release();await lease;
});
for(const [name,change] of [
 ['newer rendered epoch',s=>s.box.dataset.settingsEpoch='21'],
 ['invalid rendered epoch',s=>s.box.dataset.settingsEpoch='20x'],
 ['same-ID panel replacement',s=>s.nodes['#workflow-permissions']={...s.panel}],
 ['same-ID settings container replacement',s=>s.nodes['#workflow-permissions-run-settings']={...s.box}],
 ['same-ID run selector replacement',s=>s.fields['#workflow-permissions-run']={...s.mode}],
 ['same-ID copy selector replacement',s=>s.nodes['#workflow-permissions-copy']={...s.choice}],
 ['same-ID original title replacement',s=>s.nodes['#workflow-management-title']={...s.title}],
 ['retired document',s=>s.root.dataset.managementRevision='18'],
 ['IME',s=>s.root.__workflowManagement.composing=true],
 ['ahead native metadata',s=>s.title.value+=' ahead'],
 ['retired lease',s=>s.root.__workflowManagement.lease={}],
])await test('passive render wait aborts '+name+' without adopting or editing buffers',async()=>{
 const {s,lease,next,clock}=laggedBinding(),saved=s.root.__workflowManagement.permissionsConfirmation;
 const proof=s.context.workflowPermissionsBindSettings(s.expected,next);
 change(s);const before=[s.title.value,s.description.value,s.choice.value,s.mode.value];
 await clock.frame();assert.equal(await proof,false);assert.equal(saved.runSettings.epoch,'19');
 assert.deepEqual([s.title.value,s.description.value,s.choice.value,s.mode.value],before);
 assert.equal(clock.pending(),0);s.release();await lease;
});
await test('passive wait is bounded even when the WebView produces no animation frame',async()=>{
 const {s,lease,next,clock}=laggedBinding();const proof=s.context.workflowPermissionsBindSettings(s.expected,next);
 await clock.expire();assert.equal(await proof,false);assert.equal(clock.pending(),0);
 assert.equal(s.root.__workflowManagement.permissionsConfirmation.runSettings.epoch,'19');s.release();await lease;
});
await test('expected render still refuses a different ahead selector and does not retry that refusal',async()=>{
 const {s,lease,next,clock}=laggedBinding();const proof=s.context.workflowPermissionsBindSettings(s.expected,next);
 s.box.dataset.settingsEpoch='20';s.mode.value='everyone';await clock.frame();
 assert.equal(await proof,false);assert.equal(clock.pending(),0);
 assert.equal(s.root.__workflowManagement.permissionsConfirmation.runSettings.epoch,'19');
 s.mode.value='named_entities';assert.equal(clock.pending(),0,'no delayed adoption after a strict refusal');s.release();await lease;
});
await test('passive same-epoch bind cannot adopt replaced nodes or authorize an early action',async()=>{
 const {s,lease,next,clock}=laggedBinding();
 assert.equal(s.context.workflowPermissionsSettings(s.expected,next,{kind:'copy',value:'everyone'}),false);
 s.box.dataset.settingsEpoch='20';assert.equal(await s.context.workflowPermissionsBindSettings(s.expected,next),true);
 s.selectedNodes[0]={...s.selectedNodes[0]};
 assert.equal(await s.context.workflowPermissionsBindSettings(s.expected,next),false);
 assert.equal(clock.pending(),0);s.release();await lease;
});


function pendingEveryoneExternal(){
 const s=withSettings(scene()),lease=s.context.workflowPermissionsLock(s.expected,s.channel);
 s.query.value='';s.externalQuery.value='';s.resultNodes.length=0;
 s.selectedNodes.push({...s.selectedNodes[0],dataset:{runSelected:'TRUNEXTER'}});
 const original={...s.settingsExpected,ids:['UWORKFLOW','TRUNEXTER'],visibleIds:['UWORKFLOW','TRUNEXTER'],query:'',externalQuery:'',results:[],externalResults:[]};
 s.context.workflowPermissionsBind(s.expected);
 assert.equal(s.context.workflowPermissionsSettings(s.expected,original,null),true);
 s.mode.value='everyone';
 assert.equal(s.context.workflowPermissionsSettings(s.expected,original,{kind:'mode',value:'everyone'}),true);
 const pending={...original,epoch:20,mode:'everyone',visibleIds:['TRUNEXTER']};
 const clock=renderClock(s);
 return {s,lease,pending,clock,render(){s.box.dataset.settingsEpoch='20';delete s.fields['#workflow-permissions-target-query'];s.selectedNodes.shift();}};
}
await test('Everyone external prerequisite waits for pending mode render before strict read admission',async()=>{
 const {s,lease,pending,clock,render}=pendingEveryoneExternal();
 assert.equal(s.context.workflowPermissionsSettings(s.expected,pending,null),false,'old immediate read check refuses pre-render state');
 let dispatched=0;
 const proof=(async()=>{
  if(!await s.context.workflowPermissionsBindSettings(s.expected,pending))return false;
  if(!s.context.workflowPermissionsSettings(s.expected,pending,null))return false;
  dispatched++;return true;
 })();
 await clock.frame();await clock.frame();assert.equal(dispatched,0,'no prerequisite dispatch before actual controls render');
 render();await clock.frame();assert.equal(await proof,true);assert.equal(dispatched,1);
 assert.equal(s.mode.value,'everyone');assert.equal(s.choice.value,'collaborators');
 assert.equal(s.title.value,s.expected.title);assert.equal(s.description.value,s.expected.description);
 assert.deepEqual(pending.ids,['UWORKFLOW','TRUNEXTER'],'pending view does not fabricate acquired external target list');
 s.release();await lease;
});
for(const [name,change] of [
 ['retired original source DOM',s=>s.root.dataset.managementDocument='99'],
 ['IME during pending render',s=>s.root.__workflowManagement.composing=true],
 ['ahead external query',s=>s.externalQuery.value='different organization'],
 ['same-ID run replacement',s=>s.fields['#workflow-permissions-run']={...s.mode}],
])await test('Everyone prerequisite refuses '+name+' without dispatch or action replay',async()=>{
 const {s,lease,pending,clock,render}=pendingEveryoneExternal();let dispatched=0;
 const proof=(async()=>{
  if(!await s.context.workflowPermissionsBindSettings(s.expected,pending))return false;
  if(!s.context.workflowPermissionsSettings(s.expected,pending,null))return false;
  dispatched++;return true;
 })();
 change(s);render();await clock.frame();assert.equal(await proof,false);assert.equal(dispatched,0);
 assert.equal(s.mode.value,'everyone','failed admission never rewrites original native mode');
 assert.equal(s.title.value,s.expected.title);s.release();await lease;
});
await test('strict read recheck refuses native edit after passive pending bind',async()=>{
 const {s,lease,pending,clock,render}=pendingEveryoneExternal();
 const bound=s.context.workflowPermissionsBindSettings(s.expected,pending);render();await clock.frame();assert.equal(await bound,true);
 s.externalQuery.value='new native text';
 assert.equal(s.context.workflowPermissionsSettings(s.expected,pending,null),false);
 assert.equal(s.externalQuery.value,'new native text');s.release();await lease;
});

console.log(`${passed} workflow Permissions native tests passed`);
