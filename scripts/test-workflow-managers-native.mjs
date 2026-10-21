import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const base='src/desktop/app_home/about/management/';
const source=fs.readFileSync(base+'native.js','utf8')+'\n'+fs.readFileSync(base+'permissions/native.js','utf8')+'\n'+fs.readFileSync(base+'permissions/managers/native.js','utf8');
let passed=0;
function scene(){
 const original={id:7,visit:11,document:13,revision:17,title:'Draft \u4e2d',description:'Original \u{1f642}'};
 const expected={epoch:21,ids:['UOLD001'],query:'ada',results:['UADA001'],speedBump:false};
 const node=(extra={})=>({isConnected:true,disabled:false,validity:{badInput:false},...extra});
 const title=node({value:original.title}),description=node({value:original.description});
 const query=node({tagName:'INPUT',value:'ada'}),selected=node({dataset:{managerSelected:'UOLD001'}}),result=node({dataset:{managerResult:'UADA001'}});
 const box=node({dataset:{managerEpoch:'21'},querySelector(id){return id==='#workflow-permissions-manager-query'?query:null;},querySelectorAll(id){return id==='[data-manager-selected]'?[selected]:[result];}});
 const choice=node({tagName:'SELECT',value:'collaborators'});
 const panel=node({dataset:{confirmation:'7'},contains(n){return [box,query,selected,result,choice,bump].includes(n);},querySelector(){return bump;}});
 let bump=null;
 const nodes={'#workflow-management-title':title,'#workflow-management-description':description,'#workflow-permissions':panel,'#workflow-permissions-copy':choice,'#workflow-permissions-manager-settings':box};
 const root={isConnected:true,dataset:{managementVisit:'11',managementDocument:'13',managementRevision:'17',managementEditable:'true'},__workflowManagement:{composing:false},querySelector(id){return nodes[id]??null;},contains(n){return [title,description,panel,box,query,selected,result,choice].includes(n);}};
 const doc={root,getElementById(){return this.root;},querySelector(){return null;}};
 let frames=0,frame=()=>{};
 const c=vm.createContext({document:doc,JSON,Promise,BigInt,Date,setTimeout,clearTimeout,requestAnimationFrame(fn){return setTimeout(()=>{frames++;frame();fn();},0);},cancelAnimationFrame:clearTimeout});vm.runInContext(source,c);
 const lease={};root.__workflowManagement.lease=lease;root.__workflowManagement.permissionsConfirmation={root,state:root.__workflowManagement,key:JSON.stringify(original),title,description,lease,panel,choice};
 return {c,original,expected,root,doc,nodes,box,query,selected,result,title,description,panel,setBump(){bump=node();},onFrame(fn){frame=fn;},get frames(){return frames;}};
}
async function test(name,fn){await fn();console.log('PASS '+name);passed++;}
await test('same original manager nodes bind and authorize an actual result',()=>{const s=scene();assert.equal(s.c.workflowManagers(s.original,s.expected,null),true);assert.equal(s.c.workflowManagers(s.original,s.expected,{kind:'add',value:'UADA001'}),true);});
for(const [name,mutate] of [
 ['ahead metadata',s=>s.title.value+=' newer'],['IME',s=>s.root.__workflowManagement.composing=true],['replacement root',s=>s.doc.root={...s.root}],['replacement panel',s=>s.nodes['#workflow-permissions']={...s.panel}],['replacement selected node',s=>s.box.querySelectorAll=id=>id==='[data-manager-selected]'?[{...s.selected}]:[s.result]],['replacement result',s=>s.box.querySelectorAll=id=>id==='[data-manager-selected]'?[s.selected]:[{...s.result}]],['detached manager',s=>s.selected.isConnected=false],['ahead query',s=>s.query.value='newer'],['newer epoch',s=>s.box.dataset.managerEpoch='22'],['changed visit',s=>s.root.dataset.managementVisit='12'],['new lease',s=>s.root.__workflowManagement.lease={}]
])await test(name+' refuses without draft writes',()=>{const s=scene();assert.equal(s.c.workflowManagers(s.original,s.expected,null),true);mutate(s);const before=[s.title.value,s.description.value,s.query.value];assert.equal(s.c.workflowManagers(s.original,s.expected,{kind:'add',value:'UADA001'}),false);assert.deepEqual([s.title.value,s.description.value,s.query.value],before);});
await test('query event admits only its actual native value',()=>{const s=scene();s.c.workflowManagers(s.original,s.expected,null);s.query.value='next';assert.equal(s.c.workflowManagers(s.original,s.expected,{kind:'query',value:'other'}),false);assert.equal(s.c.workflowManagers(s.original,s.expected,{kind:'query',value:'next'}),true);});
await test('speed bump presence is part of the exact manager witness',()=>{const s=scene();s.c.workflowManagers(s.original,s.expected,null);s.setBump();assert.equal(s.c.workflowManagers(s.original,s.expected,null),false);});
await test('passive render wait binds expected generation once without replaying an action',async()=>{const s=scene();s.box.dataset.managerEpoch='20';s.onFrame(()=>s.box.dataset.managerEpoch='21');assert.equal(await s.c.workflowManagersBind(s.original,s.expected),true);assert.equal(s.frames,1);});
for(const [name,mutate]of [['IME',s=>s.root.__workflowManagement.composing=true],['same ID box replacement',s=>s.nodes['#workflow-permissions-manager-settings']={...s.box}],['ahead native',s=>s.description.value+='ahead']])await test('render wait aborts '+name,async()=>{const s=scene();s.box.dataset.managerEpoch='20';s.onFrame(()=>{mutate(s);s.box.dataset.managerEpoch='21';});assert.equal(await s.c.workflowManagersBind(s.original,s.expected),false);});
await test('fact-phase latest query uses original editable input while selection remains disabled',()=>{
 const s=scene();assert.equal(s.c.workflowManagers(s.original,s.expected,null),true);
 s.selected.disabled=true;s.result.disabled=true;s.query.readOnly=false;s.query.value='successor';
 assert.equal(s.c.workflowManagers(s.original,s.expected,{kind:'query',value:'successor'}),true);
 assert.equal(s.c.workflowManagers(s.original,{...s.expected,query:'successor'},{kind:'add',value:'UADA001'}),false);
 assert.equal(s.c.workflowManagers(s.original,s.expected,null),false);
});
await test('ordinary pending readonly query cannot retire acquisition by a synthetic event',()=>{
 const s=scene();s.c.workflowManagers(s.original,s.expected,null);s.query.readOnly=true;s.query.value='successor';
 assert.equal(s.c.workflowManagers(s.original,s.expected,{kind:'query',value:'successor'}),false);
});
await test('fact-phase query refuses IME and replacement input without changing buffers',()=>{
 const s=scene();s.c.workflowManagers(s.original,s.expected,null);s.query.value='successor';
 s.root.__workflowManagement.composing=true;
 assert.equal(s.c.workflowManagers(s.original,s.expected,{kind:'query',value:'successor'}),false);
 s.root.__workflowManagement.composing=false;s.box.querySelector=()=>({...s.query});
 assert.equal(s.c.workflowManagers(s.original,s.expected,{kind:'query',value:'successor'}),false);
 assert.equal(s.query.value,'successor');assert.equal(s.title.value,s.original.title);
});
console.log(`${passed} manager native bridge tests passed`);
