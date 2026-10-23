import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const script=fs.readFileSync(new URL('./native.js',import.meta.url),'utf8');
function scene(){
 const original={id:7},local={pane:'share'},expected={generation:11,epoch:13,text:'',selected:'',query:'',queryRevision:17,rows:[{id:'CONE',label:'one'}]};
 const nodes={},saved={};
 const panel={dataset:{shareGeneration:'11',shareEpoch:'13',shareSelected:'',shareQueryRevision:'17'},contains:n=>Object.values(nodes).includes(n),isConnected:true};
 const field=(tag,value='')=>({tagName:tag,value,isConnected:true,disabled:false,readOnly:false,validity:{badInput:false},getClientRects(){return this.hidden?[]:[{}]},focus(){this.focused=true},listeners:{},addEventListener(k,fn){this.listeners[k]=fn},dispatched:[],dispatchEvent(event){this.dispatched.push({event,node:this});this.listeners[event.type]?.(event);return !event.defaultPrevented}});
 nodes['workflow-finish-share-pane']=panel;
 nodes['workflow-share-destination']={...field('SELECT'),options:[{value:'',textContent:'Choose a conversation'},{value:'CONE',textContent:'one'}]};
 nodes['workflow-share-message']=field('TEXTAREA');
 nodes['workflow-share-query']={...field('INPUT'),type:'search'};
 for(const[k,label]of Object.entries({send:'Send',cancel:'Cancel',refresh:'Refresh destinations'}))nodes['workflow-share-'+k]={...field('BUTTON'),textContent:label};
 nodes['workflow-confirmation-share-open']={...field('BUTTON'),textContent:'Share'};
 const s={original,local,expected,nodes,saved,panel,parent:true,frame:()=>{}};
 s.root={__workflowManagement:{permissionsConfirmation:saved},querySelector:id=>nodes[id.slice(1)]??null};
 const context=vm.createContext({Event,document:{getElementById:()=>s.root},workflowFinishUp:()=>s.parent,workflowFinishUpBind:async()=>s.parent,
  requestAnimationFrame:cb=>{s.frame();cb()},getComputedStyle:n=>({display:n.hidden?'none':'block',visibility:n.hidden?'hidden':'visible'})});
 vm.runInContext(script,context);
 s.bind=()=>context.workflowShareBind(original,local,expected);
 s.current=action=>context.workflowShareCurrent(original,local,expected,action);
 s.context=context;return s;
}
let checks=0;
async function test(name,fn){await fn();checks++;console.log('PASS '+name)}
await test('bind preserves controls and focuses original query; exact original controls authorize',async()=>{const s=scene();assert.equal(await s.bind(),true);assert.equal(s.current(),true);assert.equal(s.nodes['workflow-share-query'].focused,true)});
await test('entry requires earlier original bind and rejects replacement',async()=>{const s=scene();s.local.pane='details';assert.equal(s.context.workflowShareEntry(s.original,s.local),false);assert.equal(await s.context.workflowShareEntryBind(s.original,s.local),true);assert.equal(s.context.workflowShareEntry(s.original,s.local),true);s.nodes['workflow-confirmation-share-open']={...s.nodes['workflow-confirmation-share-open']};assert.equal(s.context.workflowShareEntry(s.original,s.local),false)});
for(const kind of ['query','message','destination','send','cancel','refresh'])await test('same-id '+kind+' replacement refuses current and rebind',async()=>{const s=scene();await s.bind();s.nodes['workflow-share-'+kind]={...s.nodes['workflow-share-'+kind]};assert.equal(s.current(),false);assert.equal(await s.bind(),false)});
await test('ahead text refuses send but exact input action advances original buffer',async()=>{const s=scene();await s.bind();s.nodes['workflow-share-message'].value='Draft ä¸­';assert.equal(s.current({kind:'send'}),false);assert.equal(s.current({kind:'text',value:'Draft ä¸­'}),true);s.expected.text='Draft ä¸­';s.expected.epoch=14;s.panel.dataset.shareEpoch='14';assert.equal(await s.bind(),true);assert.equal(s.current(),true)});
await test('unknown destination cannot grant selection',async()=>{const s=scene();await s.bind();s.nodes['workflow-share-destination'].value='COTHER';assert.equal(s.current({kind:'select',value:'COTHER'}),false)});
await test('disabled send and readonly text refuse actions',async()=>{const s=scene();await s.bind();s.nodes['workflow-share-send'].disabled=true;assert.equal(s.current({kind:'send'}),false);s.nodes['workflow-share-message'].readOnly=true;assert.equal(s.current({kind:'text',value:''}),false);assert.equal(s.current({kind:'cancel'}),true)});
await test('IME refuses dispatch/rebind until original composition ends',async()=>{const s=scene();await s.bind();const t=s.nodes['workflow-share-message'];t.listeners.compositionstart();assert.equal(s.current({kind:'send'}),false);assert.equal(await s.bind(),false);t.listeners.compositionend();assert.equal(s.current(),true)});
await test('retired parent draft guard refuses child dispatch',async()=>{const s=scene();await s.bind();s.parent=false;assert.equal(s.current(),false);assert.equal(await s.bind(),false)});
await test('old generation or epoch cannot adopt successor',async()=>{for(const key of ['shareEpoch','shareGeneration']){const s=scene();await s.bind();s.panel.dataset[key]='99';assert.equal(s.current(),false);assert.equal(await s.bind(),false)}});
await test('hidden action and changed option label refuse',async()=>{const s=scene();await s.bind();s.nodes['workflow-share-cancel'].hidden=true;assert.equal(s.current({kind:'cancel'}),false);s.nodes['workflow-share-destination'].options[1].textContent='other';assert.equal(s.current(),false)});
await test('loading to ready epoch retains original input and selection nodes',async()=>{const s=scene();await s.bind();const t=s.nodes['workflow-share-message'];s.expected.epoch=14;s.panel.dataset.shareEpoch='14';s.expected.rows.push({id:'CTWO',label:'two'});s.nodes['workflow-share-destination'].options.push({value:'CTWO',textContent:'two'});assert.equal(await s.bind(),true);assert.equal(s.saved.share.text,t)});
await test('person resolution retains original controls and draft while replacing U with actual D',async()=>{
 const s=scene();s.expected.text='Keep this draft';s.nodes['workflow-share-message'].value=s.expected.text;
 s.expected.rows.push({id:'USHARE001',label:'Dana Reviewer'});
 s.nodes['workflow-share-destination'].options.push({value:'USHARE001',textContent:'Dana Reviewer'});
 await s.bind();
 const select=s.nodes['workflow-share-destination'],text=s.nodes['workflow-share-message'];
 select.value='USHARE001';assert.equal(s.current({kind:'select',value:'USHARE001'}),true);
 s.expected.selected='USHARE001';s.panel.dataset.shareSelected='USHARE001';
 s.expected.epoch=14;s.panel.dataset.shareEpoch='14';select.disabled=true;text.readOnly=true;s.nodes['workflow-share-send'].disabled=true;
 assert.equal(await s.bind(),true);assert.equal(s.current({kind:'cancel'}),true);
 assert.equal(s.current({kind:'text',value:s.expected.text}),false);assert.equal(s.current({kind:'send'}),false);
 s.expected.rows[1]={id:'DSHARE001',label:'Dana Reviewer'};
 select.options[2]={value:'DSHARE001',textContent:'Dana Reviewer'};
 s.expected.selected='DSHARE001';s.panel.dataset.shareSelected='DSHARE001';select.value='DSHARE001';
 s.expected.epoch=15;s.panel.dataset.shareEpoch='15';select.disabled=false;text.readOnly=false;s.nodes['workflow-share-send'].disabled=false;
 assert.equal(await s.bind(),true);assert.equal(s.current({kind:'send'}),true);
 assert.equal(s.saved.share.select,select);assert.equal(s.saved.share.text,text);assert.equal(text.value,'Keep this draft');
});
await test('ahead query blocks reads Send and unrelated actions; exact original query action alone allowed',async()=>{
 const s=scene();await s.bind();const q=s.nodes['workflow-share-query'];q.value='Dana';
 assert.equal(s.current(),false);assert.equal(s.current({kind:'send'}),false);
 assert.equal(s.current({kind:'refresh'}),false);assert.equal(s.current({kind:'text',value:''}),false);
 assert.equal(s.current({kind:'query',value:'Da'}),false);
 assert.equal(s.current({kind:'query',value:'Dana'}),true);
 assert.equal(s.current({kind:'cancel'}),true);
});
await test('later bind preserves ahead original query without authorizing reads or Send',async()=>{
 const s=scene();await s.bind();const q=s.nodes['workflow-share-query'];q.value='Latest Dana';
 s.expected.epoch=14;s.panel.dataset.shareEpoch='14';
 assert.equal(await s.bind(),true);assert.equal(s.saved.share.query,q);assert.equal(q.value,'Latest Dana');
 assert.equal(s.current(),false);assert.equal(s.current({kind:'send'}),false);
 assert.equal(s.current({kind:'query',value:'Latest Dana'}),true);
});
await test('query revision retires old snapshot even when values and original nodes match',async()=>{
 const s=scene();await s.bind();const old={...s.expected};
 s.panel.dataset.shareQueryRevision='18';
 assert.equal(s.current(),false);assert.equal(await s.bind(),false);
 s.expected.queryRevision=18;s.expected.epoch=14;s.panel.dataset.shareEpoch='14';
 assert.equal(await s.bind(),true);assert.equal(s.current(),true);
 assert.equal(s.context.workflowShareCurrent(s.original,s.local,old,{kind:'query',value:''}),false);
});
await test('query composition blocks query reads and Send while original Cancel remains available',async()=>{
 const s=scene();await s.bind();const q=s.nodes['workflow-share-query'];q.listeners.compositionstart();q.value='\u4e2d';
 assert.equal(s.current({kind:'query',value:'\u4e2d'}),false);assert.equal(s.current(),false);
 assert.equal(s.current({kind:'send'}),false);assert.equal(s.current({kind:'cancel'}),true);
 s.expected.epoch=14;s.panel.dataset.shareEpoch='14';
 assert.equal(await s.bind(),true);assert.equal(s.saved.share.queryComposing,true);
 assert.equal(s.current({kind:'query',value:'\u4e2d'}),false);assert.equal(s.current({kind:'cancel'}),true);
});
await test('query compositionend dispatches a bubbling input Event on original query after clearing composition',async()=>{
 const s=scene();await s.bind();const q=s.nodes['workflow-share-query'];q.listeners.compositionstart();q.value='\u4e2d';
 let observed;
 q.addEventListener('input',event=>{observed={event,composing:s.saved.share.queryComposing,admitted:s.current({kind:'query',value:q.value})}});
 q.listeners.compositionend();
 assert.equal(q.dispatched.length,1);assert.equal(q.dispatched[0].node,q);
 assert.equal(observed.event instanceof Event,true);assert.equal(observed.event.type,'input');assert.equal(observed.event.bubbles,true);
 assert.equal(observed.composing,false);assert.equal(observed.admitted,true);
 assert.equal(s.current({kind:'send'}),false);
});
await test('rapid latest input permits only exact current buffer and model adoption restores ordinary admission',async()=>{
 const s=scene();await s.bind();const q=s.nodes['workflow-share-query'];
 for(const value of ['D','Da','#research','@Dana']){
  const old=q.value;q.value=value;
  assert.equal(s.current({kind:'query',value:old}),false);
  assert.equal(s.current({kind:'query',value}),true);assert.equal(s.current(),false);
 }
 s.expected.query=q.value;s.expected.queryRevision=18;s.panel.dataset.shareQueryRevision='18';
 s.expected.epoch=14;s.panel.dataset.shareEpoch='14';
 assert.equal(await s.bind(),true);assert.equal(s.current(),true);assert.equal(s.current({kind:'send'}),true);
});
await test('query node type invalid input and disconnection refuse original admission',async()=>{
 for(const mutate of [q=>q.type='text',q=>q.validity.badInput=true,q=>q.isConnected=false]){
  const s=scene();await s.bind();mutate(s.nodes['workflow-share-query']);
  assert.equal(s.current({kind:'query',value:''}),false);assert.equal(s.current(),false);assert.equal(await s.bind(),false);
 }
});
await test('hidden readonly or disabled query refuses query action without weakening Cancel',async()=>{
 for(const mutate of [q=>q.hidden=true,q=>q.readOnly=true,q=>q.disabled=true]){
  const s=scene();await s.bind();mutate(s.nodes['workflow-share-query']);
  assert.equal(s.current({kind:'query',value:''}),false);assert.equal(s.current({kind:'cancel'}),true);
 }
});
await test('same Share generation preserves uncommitted query and caret across serial renders',async()=>{
 const s=scene();await s.bind();const q=s.nodes['workflow-share-query'];q.value='Dana 中';q.selectionStart=2;q.selectionEnd=4;
 s.expected.queryInput=101;assert.equal(await s.bind(),true);assert.equal(q.value,'Dana 中');
 assert.equal(q.selectionStart,2);assert.equal(q.selectionEnd,4);assert.equal(s.current(),false);
 assert.equal(s.current({kind:'query',value:'Dana 中'}),true);
});
await test('fresh Share generation initializes only original query buffer and clears old query composition',async()=>{
 const s=scene();await s.bind();const q=s.nodes['workflow-share-query'];q.value='old query';q.listeners.compositionstart();
 s.expected.generation=12;s.panel.dataset.shareGeneration='12';s.expected.query='';
 assert.equal(await s.bind(),true);assert.equal(s.saved.share.query,q);assert.equal(q.value,'');
 assert.equal(s.saved.share.queryComposing,false);assert.equal(q.focused,true);assert.equal(s.current(),true);
});

function tokenScene(){
 const s=scene();s.bind=()=>s.context.workflowShareBind(s.original,s.local,s.expected);s.current=action=>s.context.workflowShareCurrent(s.original,s.local,s.expected,action);s.expected.recipients=[{id:'UONE',label:'Dana'},{id:'UTWO',label:'Evan'}];
 s.expected.recipientRevision=31;s.expected.selectedRecipient=null;s.panel.dataset.recipientRevision='31';
 s.tokens=[];s.removes=[];
 for(const r of s.expected.recipients){
  const remove={...s.nodes['workflow-share-cancel'],dataset:{workflowShareRecipientRemove:r.id},listeners:{},getAttribute:name=>name==='aria-label'?`Remove ${r.label}`:null};
  const token={isConnected:true,dataset:{workflowShareRecipient:r.id,selected:'false'},contains:n=>n===remove};
  s.tokens.push(token);s.removes.push(remove);
 }
 const contains=s.panel.contains;s.panel.contains=n=>contains(n)||s.tokens.includes(n)||s.removes.includes(n);
 s.root.querySelectorAll=selector=>selector==='[data-workflow-share-recipient]'?s.tokens:s.removes;
 const q=s.nodes['workflow-share-query'];q.focus=()=>{q.focused=true;s.context.document.activeElement=q};
 s.event=(target,type,values={})=>{const e={button:0,key:'',isComposing:false,repeat:false,defaultPrevented:false,preventDefault(){this.defaultPrevented=true},...values};target.listeners[type]?.(e);return e};
 return s;
}
await test('original token mousedown alone authorizes one matching removal and retains query focus',async()=>{
 const s=tokenScene();assert.equal(await s.bind(),true);
 assert.equal(s.current({kind:'recipientRemove',value:'UTWO'}),false);
 const e=s.event(s.removes[1],'mousedown');assert.equal(e.defaultPrevented,true);
 assert.equal(s.context.document.activeElement,s.nodes['workflow-share-query']);
 assert.equal(s.current({kind:'recipientRemove',value:'UONE'}),false);
 assert.equal(s.current({kind:'recipientRemove',value:'UTWO'}),true);
 assert.equal(s.current({kind:'recipientRemove',value:'UTWO'}),false);
});
await test('held resolution permits actual token removal and Cancel but not readonly query keys or Send',async()=>{
 const s=tokenScene();await s.bind();s.nodes['workflow-share-query'].readOnly=true;s.nodes['workflow-share-send'].disabled=true;
 assert.equal(s.event(s.nodes['workflow-share-query'],'keydown',{key:'Backspace'}).defaultPrevented,false);
 assert.equal(s.current({kind:'recipientKey',value:'Backspace'}),false);
 assert.equal(s.event(s.removes[1],'mousedown').defaultPrevented,true);
 assert.equal(s.current({kind:'recipientRemove',value:'UTWO'}),true);
 assert.equal(s.current({kind:'cancel'}),true);assert.equal(s.current({kind:'send'}),false);
});
await test('Backspace selects then selected Delete supplies two separately owned original events',async()=>{
 const s=tokenScene();await s.bind();const q=s.nodes['workflow-share-query'];
 assert.equal(s.event(q,'keydown',{key:'Delete'}).defaultPrevented,false);
 assert.equal(s.event(q,'keydown',{key:'Backspace'}).defaultPrevented,true);
 assert.equal(s.current({kind:'recipientKey',value:'Backspace'}),true);
 s.expected.selectedRecipient=1;s.tokens[1].dataset.selected='true';s.expected.epoch=14;s.panel.dataset.shareEpoch='14';
 assert.equal(await s.bind(),true);
 assert.equal(s.current({kind:'recipientKey',value:'Delete'}),false);
 assert.equal(s.event(q,'keydown',{key:'Delete'}).defaultPrevented,true);
 assert.equal(s.current({kind:'recipientKey',value:'Delete'}),true);
});
await test('token events refuse composition, secondary mouse button, repeat, ahead query and disabled remove',async()=>{
 for(const kind of ['composition','secondary','repeat','ahead','disabled']){
  const s=tokenScene();await s.bind();const q=s.nodes['workflow-share-query'];
  if(kind==='composition')q.listeners.compositionstart();
  if(kind==='ahead')q.value='D';
  if(kind==='disabled')s.removes[1].disabled=true;
  const event=kind==='repeat'?s.event(q,'keydown',{key:'Backspace',repeat:true}):s.event(s.removes[1],'mousedown',{button:kind==='secondary'?2:0});
  assert.equal(event.defaultPrevented,false,kind);
  assert.equal(s.current({kind:kind==='repeat'?'recipientKey':'recipientRemove',value:kind==='repeat'?'Backspace':'UTWO'}),false,kind);
 }
});
await test('same-ID token or remove replacement cannot authorize or rebind',async()=>{
 for(const field of ['tokens','removes']){
  const s=tokenScene();await s.bind();s[field][1]={...s[field][1]};
  assert.equal(s.current(),false);assert.equal(await s.bind(),false);
 }
});
await test('actual detached token removal retains every surviving original node',async()=>{
 const s=tokenScene();await s.bind();const first=s.tokens[0],firstRemove=s.removes[0],last=s.tokens[1];
 s.expected={...s.expected,recipients:s.expected.recipients.slice(1),recipientRevision:32};s.panel.dataset.recipientRevision='32';
 s.tokens=s.tokens.slice(1);s.removes=s.removes.slice(1);s.expected.epoch=14;s.panel.dataset.shareEpoch='14';
 assert.equal(await s.bind(),false); // old node still attached is not accepted as removal
 first.isConnected=false;firstRemove.isConnected=false;
 assert.equal(await s.bind(),true);assert.equal(s.saved.share.tokens[0],last);assert.equal(s.current(),true);
});
await test('recipient revision and epoch retire an already recorded event',async()=>{
 const s=tokenScene();await s.bind();s.event(s.removes[1],'mousedown');
 s.expected.epoch=14;s.panel.dataset.shareEpoch='14';assert.equal(await s.bind(),true);
 assert.equal(s.current({kind:'recipientRemove',value:'UTWO'}),false);
 s.panel.dataset.recipientRevision='99';assert.equal(s.current(),false);
});
await test('person addition clears only exact original query; duplicate restores selection without clearing query',async()=>{
 const s=scene();s.expected.query='Dana';await s.bind();const select=s.nodes['workflow-share-destination'],query=s.nodes['workflow-share-query'];
 select.value='CONE';assert.equal(s.current({kind:'select',value:'CONE',clearQuery:true}),true);assert.equal(query.value,'');
 query.value='Dana';select.value='CONE';assert.equal(s.current({kind:'select',value:'CONE',restoreSelection:true}),true);
 assert.equal(select.value,'');assert.equal(query.value,'Dana');assert.equal(s.current(),true);
});

console.log(`${checks} Share original-node checks passed`);
