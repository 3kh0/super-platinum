import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const here=new URL('.',import.meta.url);
export {scene,crop};
const source=fs.readFileSync(new URL('../native.js',here),'utf8')+'\n'+fs.readFileSync(new URL('native.js',here),'utf8')+'\n'+fs.readFileSync(new URL('../managers/native.js',here),'utf8');
function scene(){
 const original={id:7,visit:11,document:13,revision:17,title:'Original draft',description:'Retained parent',finishUp:true};
 const expected={epoch:21,title:'Workflow',description:'Required details',icon:'',more:false,pane:'details',avatarPending:false,crop:null};
 const field=(tag,value)=>({tagName:tag,value,isConnected:true,validity:{badInput:false},focus(){},disabled:false});
 const parentTitle=field('INPUT',original.title),parentDescription=field('TEXTAREA',original.description);
 const title=field('INPUT',expected.title),description=field('TEXTAREA',expected.description),upload={...field('INPUT',''),type:'file',files:[]};
 const modalNodes={'#workflow-finish-title':title,'#workflow-finish-description':description,'#workflow-finish-upload':upload};
 const paneNodes={};
 for(const [kind,id] of Object.entries({details:'workflow-finish-details',upload:'workflow-finish-upload-pane',crop:'workflow-finish-crop-pane'})){
  const pane={isConnected:true,style:{display:kind==='details'?'flex':'none'},contains:n=>n?.pane===kind};
  paneNodes[kind]=pane;modalNodes['#'+id]=pane;
 }
 for(const [kind,id] of Object.entries({cancel:'workflow-finish-cancel',submit:'workflow-finish-submit',more:'workflow-finish-show-more','upload-back':'workflow-finish-upload-back','upload-retry':'workflow-finish-upload-retry','upload-cancel':'workflow-finish-upload-cancel','crop-back':'workflow-finish-crop-back','crop-cancel':'workflow-finish-crop-cancel','crop-apply':'workflow-finish-crop-apply'}))
  modalNodes['#'+id]={tagName:'BUTTON',isConnected:true,disabled:false,hidden:false,pane:kind.startsWith('upload-')?'upload':kind.startsWith('crop-')?'crop':'details'};
 const modal={isConnected:true,dataset:{finishOwner:'7',finishEpoch:'21',finishIcon:'',finishTitle:expected.title,finishDescription:expected.description,finishBusy:'false',finishPane:'details',finishBound:'true'},querySelector:id=>modalNodes[id]??null,contains:n=>Object.values(modalNodes).includes(n)};
 const nodes={'#workflow-management-title':parentTitle,'#workflow-management-description':parentDescription,'#workflow-finish-up':modal};
 const state={composing:false,lease:{}};
 const root={isConnected:true,dataset:{managementVisit:'11',managementDocument:'13',managementRevision:'17'},__workflowManagement:state,querySelector:id=>nodes[id]??null,contains:n=>Object.values(nodes).includes(n)};
 const saved={root,state,key:JSON.stringify(original),lease:state.lease,title:parentTitle,description:parentDescription};state.permissionsConfirmation=saved;
 const document={dialogs:[modal],activeElement:null,getElementById:id=>id==='workflow-management'?root:id==='workflow-finish-up'?nodes['#workflow-finish-up']:null,querySelector(){return this.dialogs[0]??null},querySelectorAll(){return this.dialogs}};
 const context=vm.createContext({document,JSON,Promise,getComputedStyle:n=>n.style});vm.runInContext(source,context);
 return {original,expected,paneNodes,parentTitle,parentDescription,title,description,upload,modal,modalNodes,nodes,state,root,saved,document,context,
   proof(action=null,bind=false){return context.workflowFinishUp(original,expected,action,bind)}};
}
let count=0;function test(name,fn){fn();count++;console.log('PASS '+name)}
test('only exact captured modal is admitted and original metadata remains untouched',()=>{
 const s=scene();assert.equal(s.proof(null,true),true);assert.equal(s.proof(),true);assert.equal(s.saved.modal,s.modal);assert.equal(s.parentTitle.value,'Original draft');
});
for(const [name,mutate] of [
 ['second modal',s=>s.document.dialogs.push({})],
 ['same-ID modal replacement',s=>{s.nodes['#workflow-finish-up']={...s.modal};s.document.dialogs=[s.nodes['#workflow-finish-up']]}],
 ['original title replacement',s=>s.nodes['#workflow-management-title']={...s.parentTitle}],
 ['modal title replacement',s=>s.modalNodes['#workflow-finish-title']={...s.title}],
 ['modal description replacement',s=>s.modalNodes['#workflow-finish-description']={...s.description}],
 ['upload control replacement',s=>s.modalNodes['#workflow-finish-upload']={...s.upload}],
 ['parent ahead buffer',s=>s.parentTitle.value+=' newer'],
 ['modal ahead buffer',s=>s.title.value+=' newer'],
 ['IME',s=>s.state.composing=true],
 ['retired lease',s=>s.state.lease={}],
 ['retired revision',s=>s.root.dataset.managementRevision='18'],
 ['retired modal epoch',s=>s.modal.dataset.finishEpoch='22'],
 ['unacknowledged icon',s=>s.modal.dataset.finishIcon='forged'],
])test(name+' refuses without editing buffers',()=>{const s=scene();assert.equal(s.proof(null,true),true);mutate(s);const before=[s.parentTitle.value,s.title.value,s.description.value];assert.equal(s.proof(),false);assert.deepEqual([s.parentTitle.value,s.title.value,s.description.value],before)});
test('actual title input advances only its own buffer',()=>{const s=scene();s.proof(null,true);s.title.value='New name';assert.equal(s.proof({kind:'title',value:'New name'}),true);s.description.value='Ahead description';assert.equal(s.proof({kind:'title',value:'New name'}),false)});
test('unrecognized action does not acquire authority',()=>{const s=scene();s.proof(null,true);assert.equal(s.proof({kind:'arbitrary',value:'x'}),false)});
test('upload requires actual selected file and keeps original File identity across awaits',()=>{
 const s=scene();s.proof(null,true);const file={name:'icon.png',size:128,type:'image/png'};s.upload.files=[file];
 assert.equal(s.proof({kind:'upload',value:'other.png'}),false);assert.equal(s.proof({kind:'upload',value:'icon.png'}),true);assert.equal(s.proof(),true);
 s.upload.files=[{...file}];assert.equal(s.proof(),false);
});
function pane(s,name){s.expected.pane=name;s.modal.dataset.finishPane=name;for(const [k,n] of Object.entries(s.paneNodes))n.style.display=k===name?'flex':'none';}
function crop(s){pane(s,'crop');s.expected.crop={x:0,y:0,size:128};for(const k of ['x','y','size'])s.modalNodes['#workflow-finish-crop-'+k]={tagName:'INPUT',value:String(s.expected.crop[k]),isConnected:true,disabled:false,validity:{badInput:false}};assert.equal(s.proof(null,true),true)}
test('crop binds exact controls and requires native values matching selected coordinates',()=>{const s=scene();s.proof(null,true);crop(s);const x=s.modalNodes['#workflow-finish-crop-x'];x.value='20';assert.equal(s.proof({kind:'crop-x',value:'20'}),true);assert.equal(s.proof({kind:'crop-x',value:'30'}),false);assert.equal(s.proof(),false)});
test('same-ID crop replacement cannot authorize upload-derived crop',()=>{const s=scene();s.proof(null,true);crop(s);s.modalNodes['#workflow-finish-crop-size']={...s.modalNodes['#workflow-finish-crop-size']};assert.equal(s.proof(),false)});
test('disabled crop and invalid native numeric buffer refuse',()=>{const s=scene();s.proof(null,true);crop(s);s.modalNodes['#workflow-finish-crop-x'].disabled=true;assert.equal(s.proof({kind:'crop-x',value:'0'}),false);s.modalNodes['#workflow-finish-crop-x'].disabled=false;s.modalNodes['#workflow-finish-crop-size'].value='';assert.equal(s.proof(),false)});
test('successive acknowledged crop edits retain original controls and reject old epoch',()=>{const s=scene();s.proof(null,true);crop(s);const x=s.modalNodes['#workflow-finish-crop-x'];x.value='20';assert.equal(s.proof({kind:'crop-x',value:'20'}),true);s.expected.crop.x=20;s.expected.epoch=22;assert.equal(s.proof(null,true),false);s.modal.dataset.finishEpoch='22';assert.equal(s.proof(null,true),true);const y=s.modalNodes['#workflow-finish-crop-y'];y.value='30';assert.equal(s.proof({kind:'crop-y',value:'30'}),true);s.expected.crop.y=30;s.expected.epoch=23;s.modal.dataset.finishEpoch='23';assert.equal(s.proof(null,true),true);assert.equal(s.saved.crop[0],x);assert.equal(s.saved.crop[1],y)});
test('pinned desktop full Windows path is required independently of multipart basename',()=>{
 const s=scene();s.proof(null,true);const path=String.raw`C:\Users\Fixture\workflow icon.png`,file={name:path,size:0,type:'image/png'};s.upload.files=[file];
 assert.equal(s.proof({kind:'upload',value:'workflow icon.png'}),false);
 assert.equal(s.proof({kind:'upload',value:String.raw`C:\Other\workflow icon.png`}),false);
 assert.equal(s.proof({kind:'upload',value:path}),true);assert.equal(s.proof(),true);
 s.upload.files=[{...file}];assert.equal(s.proof(),false);
});

async function passive(name,change,expected){
 const s=scene();s.proof(null,true);s.expected.epoch=25;s.expected.pane='crop';s.expected.crop={x:0,y:0,size:256};
 const file={name:String.raw`C:\Fixture\icon.png`};s.upload.files=[file];s.saved.uploadSelection=file;
 let tick=0;s.context.performance={now:()=>tick*20};s.context.requestAnimationFrame=fn=>{tick++;change(s,tick);queueMicrotask(fn)};
 const result=await s.context.workflowFinishUpBind(s.original,s.expected);assert.equal(result,expected,name);count++;console.log('PASS '+name);return s;
}
function renderCrop(s){pane(s,'crop');s.modal.dataset.finishEpoch='25';for(const k of ['x','y','size'])s.modalNodes['#workflow-finish-crop-'+k]={tagName:'INPUT',value:String(s.expected.crop[k]),isConnected:true,disabled:true,validity:{badInput:false}};}
const bound=await passive('delayed crop topology binds disabled original nodes without replay', (s,t)=>{if(t===3)renderCrop(s)},true);
assert.equal(bound.saved.finishBinding.status,'ready');assert.equal(bound.saved.crop.length,3);
assert.equal(bound.proof({kind:'crop-x',value:'64'}),false); // no action or values synthesized
await passive('crop bind aborts original lease retirement',(s,t)=>{if(t===1)s.state.lease={};},false);
await passive('crop bind aborts composing original',(s,t)=>{if(t===1)s.state.composing=true;},false);
await passive('crop bind aborts newer epoch',(s,t)=>{if(t===1)s.modal.dataset.finishEpoch='26';},false);
await passive('crop bind aborts exact upload replacement',(s,t)=>{if(t===1)s.modalNodes['#workflow-finish-upload']={...s.upload};},false);
await passive('crop bind aborts selected File replacement',(s,t)=>{if(t===1)s.upload.files=[{...s.upload.files[0]}];},false);
await passive('crop bind never accepts an ahead crop value',(s,t)=>{if(t===1){renderCrop(s);s.modalNodes['#workflow-finish-crop-x'].value='64';}},false);
await passive('crop bind bounded missing topology timeout',()=>{},false);
await passive('crop bind aborts same-ID modal replacement',(s,t)=>{if(t===1){s.nodes['#workflow-finish-up']={...s.modal};s.document.dialogs=[s.nodes['#workflow-finish-up']];}},false);
await passive('crop bind aborts partial subtree replacement',(s,t)=>{if(t===1)s.modalNodes['#workflow-finish-crop-x']={tagName:'INPUT',value:'0'};if(t===2)renderCrop(s);},false);
await passive('crop bind refuses changed local metadata on ready render',(s,t)=>{if(t===1){renderCrop(s);s.title.value='Ahead local name';}},false);

async function removal(name,change,want){
 const s=scene();s.proof(null,true);crop(s);s.oldCrop=s.saved.crop.slice();
 const file={name:String.raw`C:\Fixture\icon.png`};s.upload.files=[file];s.saved.uploadSelection=file;
 s.expected.epoch=30;s.expected.pane='details';s.expected.crop=null;s.expected.icon='https://example.test/crop.png';
 let tick=0;s.context.performance={now:()=>tick*20};s.context.requestAnimationFrame=fn=>{tick++;change(s,tick);queueMicrotask(fn)};
 const result=await s.context.workflowFinishUpBind(s.original,s.expected);assert.equal(result,want,name);
 if(!want)assert.equal(s.saved.crop[0],s.oldCrop[0]);
 count++;console.log('PASS '+name);return s;
}
function removeCrop(s,index){
 for(const i of index===undefined?[0,1,2]:[index]){delete s.modalNodes['#workflow-finish-crop-'+['x','y','size'][i]];s.oldCrop[i].isConnected=false;}
}
function renderRemoved(s){pane(s,'details');s.modal.dataset.finishEpoch='30';s.modal.dataset.finishIcon=s.expected.icon;removeCrop(s);}
const removed=await removal('delayed crop removal clears only original disconnected crop witness',(s,t)=>{if(t===1){s.modal.dataset.finishEpoch='30';s.modal.dataset.finishIcon=s.expected.icon;}if(t===3){pane(s,'details');removeCrop(s);}},true);
assert.equal(removed.saved.crop,null);assert.equal(removed.saved.finishBinding.status,'ready');assert.equal(removed.proof(),true);
await removal('partial original crop removal waits for remaining original nodes',(s,t)=>{if(t===1)removeCrop(s,0);if(t===2)renderRemoved(s);},true);
await removal('crop removal rejects same-ID replacement',(s,t)=>{if(t===1)s.modalNodes['#workflow-finish-crop-x']={...s.oldCrop[0]};},false);
await removal('crop removal rejects reuse elsewhere in connected DOM',(s,t)=>{if(t===1)delete s.modalNodes['#workflow-finish-crop-x'];},false);
await removal('crop removal rejects removed original node reappearance',(s,t)=>{if(t===1)removeCrop(s,0);if(t===2){s.oldCrop[0].isConnected=true;s.modalNodes['#workflow-finish-crop-x']=s.oldCrop[0];}},false);
await removal('crop removal rejects lease retirement',(s,t)=>{if(t===1)s.state.lease={};},false);
await removal('crop removal rejects IME',(s,t)=>{if(t===1)s.state.composing=true;},false);
await removal('crop removal rejects successor epoch',(s,t)=>{if(t===1)s.modal.dataset.finishEpoch='31';},false);
await removal('crop removal rejects File replacement',(s,t)=>{if(t===1)s.upload.files=[{...s.upload.files[0]}];},false);
await removal('crop removal rejects ahead coordinate edit',(s,t)=>{if(t===1)s.oldCrop[0].value='64';},false);
await removal('crop removal rejects modal replacement',(s,t)=>{if(t===1){s.nodes['#workflow-finish-up']={...s.modal};s.document.dialogs=[s.nodes['#workflow-finish-up']];}},false);
await removal('crop removal bounded timeout does not clear original witness',()=>{},false);
await removal('crop removal verifies final local buffers',(s,t)=>{if(t===1){renderRemoved(s);s.title.value='Ahead title';}},false);
{const s=scene();s.context.requestAnimationFrame=()=>assert.fail('unexpected wait');assert.equal(await s.context.workflowFinishUpBind(s.original,s.expected),true);count++;console.log('PASS ordinary non-crop bind retains immediate behavior without scheduling frames');}

async function paneTransition(name,change,want){
 const s=scene();assert.equal(s.proof(null,true),true);
 s.expected.epoch=22;s.expected.pane='upload';s.expected.avatarPending=true;
 let tick=0;s.context.performance={now:()=>tick*20};s.context.requestAnimationFrame=fn=>{tick++;change(s,tick);queueMicrotask(fn)};
 assert.equal(await s.context.workflowFinishUpBind(s.original,s.expected),want,name);
 count++;console.log('PASS '+name);return s;
}
function renderUpload(s){pane(s,'upload');s.modal.dataset.finishEpoch='22';s.upload.disabled=true;}
const uploading=await paneTransition('Details to Upload waits for same-owner pane render and retains hidden controls',(s,t)=>{if(t===3)renderUpload(s)},true);
assert.equal(uploading.saved.finishPanes.details,uploading.paneNodes.details);
assert.equal(uploading.saved.finish.title,uploading.title);assert.equal(uploading.saved.finish.upload,uploading.upload);
assert.equal(uploading.proof(),true); // owned pending request does not self-retire
assert.equal(uploading.proof({kind:'upload-back'}),true);assert.equal(uploading.proof({kind:'upload-cancel'}),true);
assert.equal(uploading.proof({kind:'upload-retry'}),false);assert.equal(uploading.proof({kind:'submit'}),false);
assert.equal(uploading.proof({kind:'title',value:uploading.title.value}),false);
assert.equal(uploading.context.workflowFinishUpDetails(uploading.saved),false);
await paneTransition('pane bind rejects hidden details replacement',(s,t)=>{if(t===1)s.modalNodes['#workflow-finish-details']={...s.paneNodes.details};},false);
await paneTransition('pane bind rejects new active pane replacement',(s,t)=>{if(t===1)s.modalNodes['#workflow-finish-upload-pane']={...s.paneNodes.upload};},false);
await paneTransition('pane bind rejects hidden original file input replacement',(s,t)=>{if(t===1)s.modalNodes['#workflow-finish-upload']={...s.upload};},false);
await paneTransition('pane bind rejects retired lease',(s,t)=>{if(t===1)s.state.lease={};},false);
await paneTransition('pane bind rejects IME',(s,t)=>{if(t===1)s.state.composing=true;},false);
await paneTransition('pane bind rejects newer epoch',(s,t)=>{if(t===1)s.modal.dataset.finishEpoch='23';},false);
await paneTransition('pane bind rejects ahead hidden metadata',(s,t)=>{if(t===1){renderUpload(s);s.title.value+=' ahead';}},false);
await paneTransition('pane bind times out when required pane never displays',()=>{},false);
test('Crop permits original Back/Cancel during saving but rejects Save and hidden Details actions',()=>{
 const s=scene();s.proof(null,true);crop(s);s.expected.avatarPending=true;
 for(const n of s.saved.crop)n.disabled=true;
 assert.equal(s.proof({kind:'crop-back'}),true);assert.equal(s.proof({kind:'crop-cancel'}),true);
 assert.equal(s.proof({kind:'crop-apply'}),false);assert.equal(s.proof({kind:'cancel'}),false);
 assert.equal(s.proof({kind:'submit'}),false);assert.equal(s.proof({kind:'escape'}),true);
});
test('active pane button replacement cannot inherit original Back authority',()=>{
 const s=scene();pane(s,'upload');s.proof(null,true);s.modalNodes['#workflow-finish-upload-back']={...s.modalNodes['#workflow-finish-upload-back']};assert.equal(s.proof({kind:'upload-back'}),false);
});
test('active pane requires exclusive actual visibility and readiness',()=>{
 const s=scene();s.proof(null,true);s.paneNodes.upload.style.display='flex';assert.equal(s.proof(),false);s.paneNodes.upload.style.display='none';s.modal.dataset.finishBound='false';assert.equal(s.proof({kind:'submit'}),false);assert.equal(s.proof(null,true),true);
});
test('Details action eligibility returns only with original pane node and exact binding epoch',()=>{
 const s=scene();s.proof(null,true);assert.equal(s.context.workflowFinishUpDetails(s.saved),true);
 s.modal.dataset.finishEpoch='22';assert.equal(s.context.workflowFinishUpDetails(s.saved),false);
 s.expected.epoch=22;assert.equal(s.proof(null,true),true);assert.equal(s.context.workflowFinishUpDetails(s.saved),true);
});
for(const name of ['upload','crop'])test(`actual embedded copy/run/manager actions refuse hidden Details in ${name}`,()=>{
 const s=scene();s.proof(null,true);pane(s,name);assert.equal(s.proof(null,true),true);
 // Exercise the real entry points: none may inspect/admit hidden selectors.
 assert.equal(s.context.workflowPermissionsChoice(s.original,'everyone'),false);
 assert.equal(s.context.workflowPermissionsSettings(s.original,{}, {kind:'mode',value:'everyone'}),false);
 assert.equal(s.context.workflowManagers(s.original,{}, {kind:'query',value:'new query'}),false);
 assert.equal(s.title.value,s.expected.title);assert.equal(s.description.value,s.expected.description);
});
console.log(`${count} Finish Up original-node bridge tests passed`);
