import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const source=fs.readFileSync(new URL('./native.js',import.meta.url),'utf8');
let checks=0;
function setup(){
 const nodes=new Map(), row={dataset:{triggerId:'FtOne',status:'available'}};
 const button={tagName:'BUTTON',disabled:false,isConnected:true,textContent:'Original title',getClientRects:()=>[{}],closest:()=>row,focus(){doc.activeElement=this;}};
 const panel={dataset:{owner:'7',serial:'2',current:'true',status:'ready'},contains:n=>n===button};
 nodes.set('channel-workflows-panel',panel);nodes.set('channel-workflow-open-featured-0',button);
 const doc={getElementById:id=>nodes.get(id),querySelectorAll:()=>[],activeElement:null};
 let frame=()=>{};
 const context=vm.createContext({window:{},document:doc,BigInt,getComputedStyle:n=>n.style||{display:'block',visibility:'visible'},requestAnimationFrame:cb=>{frame();cb();}});
 vm.runInContext(source,context);
 const e={id:'10',owner:'7',browseSerial:'2',button:'channel-workflow-open-featured-0',trigger:'FtOne'};
 function pane(){const p={dataset:{selection:e.id,current:'true'},isConnected:true,contains:n=>n===close||n===retry};const close={disabled:false,getClientRects:()=>[{}]},retry={disabled:false,getClientRects:()=>[{}]};nodes.set('channel-workflow-details',p);nodes.set('channel-workflow-details-close',close);nodes.set('channel-workflow-details-retry',retry);return p;}
 return {context,e,nodes,panel,button,row,doc,pane,frame:f=>{frame=f;}};
}
function check(value,expected=true){assert.equal(value,expected);checks++;}
{
 const t=setup();check(t.context.workflowDetailsCapture(t.e));t.pane();check(await t.context.workflowDetailsBind(t.e));
 check(t.context.workflowDetailsCurrent(t.e,'close'));check(t.context.workflowDetailsCurrent(t.e,'retry'));
 t.nodes.get('channel-workflow-details-retry').disabled=true;check(t.context.workflowDetailsCurrent(t.e,'retry'),false);
 t.panel.dataset.serial='3';check(t.context.workflowDetailsCurrent(t.e,''),false);
}
{
 const t=setup();t.row.dataset.status='access-denied';check(t.context.workflowDetailsCapture(t.e),false);
 t.row.dataset.status='available';t.doc.querySelectorAll=()=>[{isConnected:true,getClientRects:()=>[{}]}];check(t.context.workflowDetailsCapture(t.e),false);
 t.doc.querySelectorAll=()=>[{isConnected:true,getClientRects:()=>[],style:{display:'none'}}];check(t.context.workflowDetailsCapture(t.e));
 // Secondary role=dialog panes are outside the actual modal selector.
 check(t.context.workflowDetailsOrigin(t.e));
}
{
 const t=setup();check(t.context.workflowDetailsCapture(t.e));let frames=0;
 t.frame(()=>{if(++frames===3)t.pane();});check(await t.context.workflowDetailsBind(t.e));check(frames,3);
 const old=t.nodes.get('channel-workflow-details');t.pane();check(await t.context.workflowDetailsBind(t.e),false);
 t.nodes.set('channel-workflow-details',old);t.button.textContent='forged';check(t.context.workflowDetailsCurrent(t.e,''),false);
}
{
 const t=setup();check(t.context.workflowDetailsCapture(t.e));t.frame(()=>{t.panel.dataset.current='false';});
 check(await t.context.workflowDetailsBind(t.e),false);
}
{
 const t=setup();check(t.context.workflowDetailsCapture(t.e));check(t.context.workflowDetailsCapture({...t.e,id:'11'}));
 check(t.context.workflowDetailsCapture(t.e),false);check(await t.context.workflowDetailsBind(t.e),false);
}
{
 const t=setup();check(t.context.workflowDetailsCapture(t.e));const p=t.pane();check(await t.context.workflowDetailsBind(t.e));
 let frames=0;t.frame(()=>{if(++frames===2)p.isConnected=false;});check(await t.context.workflowDetailsRestore(t.e));check(t.doc.activeElement,t.button);
}
{
 const t=setup();check(t.context.workflowDetailsCapture(t.e));const p=t.pane();check(await t.context.workflowDetailsBind(t.e));p.isConnected=false;
 t.nodes.set(t.e.button,{...t.button});check(await t.context.workflowDetailsRestore(t.e),false);check(t.doc.activeElement,null);
}
console.log(`${checks} original-row/details native checks passed`);
