const fs=require('node:fs'),vm=require('node:vm'),assert=require('node:assert/strict');
const base='src/desktop/app_home/about/management/fixture/check/first_publish/confirmation/share/search/';
const input=fs.readFileSync(base+'input.js','utf8');const ready=fs.readFileSync(base+'ready.js','utf8');
let checks=0;
function env(){
 const box={left:10,top:10,right:210,bottom:40,width:200,height:30};
 const node=id=>({id,isConnected:true,disabled:false,readOnly:false,value:'',contains(x){return x===this},getBoundingClientRect(){return box},scrollIntoView(){},focus(){},setSelectionRange(a,b){this.selectionStart=a;this.selectionEnd=b},addEventListener(t,f){this.listener=f},removeEventListener(){this.listener=null},dispatchEvent(e){e.target=this;this.listener(e)}});
 const root=node('workflow-management'),panel=node('workflow-finish-share-pane'),modal=node('workflow-finish-up');
 const nodes=Object.fromEntries(['select','text','refresh','cancel','send','query'].map(k=>[k,node('workflow-share-'+({select:'destination',text:'message'}[k]??k))]));
 panel.contains=e=>Object.values(nodes).includes(e);panel.dataset={shareEpoch:'4',shareGeneration:'3',shareSelected:'',shareQueryRevision:'2'};
 const state={lease:{},composing:false};const saved={...nodes,panel,composing:false,epoch:'4',generation:'3'};
 state.permissionsConfirmation={root,state,lease:state.lease,modal,finishPane:{node:panel,name:'share'},share:saved};root.__workflowManagement=state;
 nodes.select.options=[{value:'',textContent:'Choose a conversation'}];nodes.query.value='Dana';nodes.text.value='optional';nodes.send.disabled=true;
 const map=Object.fromEntries([root,panel,modal,...Object.values(nodes)].map(n=>[n.id,n]));
 const context={window:{__firstPublishRegistry:{ids:new Map(),next:1}},document:{getElementById:id=>map[id],elementFromPoint:()=>nodes.query},innerWidth:1000,innerHeight:700,Event:class{constructor(type){this.type=type;this.isTrusted=false;}}};
 return{context,nodes,root,state,saved,panel,map,expected:{epoch:4,generation:3,selected:'',text:'optional',options:[],bound:true,loading:false,search:{query:'Dana',revision:2,status:'ready'}}};
}
function runInput(change,expected){let e=env();change?.(e);const r=vm.runInNewContext(input.replace('QUERY',JSON.stringify('@Dana')),e.context);assert.equal(r.ok,expected);checks++;return r;}
let r=runInput(null,true);assert.equal(r.value,'@Dana');assert.equal(r.start,5);assert.equal(r.end,5);assert.equal(r.event.isTrusted,false);checks++;
for(const f of [e=>e.nodes.query.disabled=true,e=>e.nodes.query.readOnly=true,e=>e.nodes.query.isConnected=false,e=>e.saved.query={},e=>e.state.composing=true,e=>e.saved.composing=true,e=>e.state.lease={},e=>e.context.document.elementFromPoint=()=>({})])runInput(f,false);
function runReady(change,expected,initial=false){let e=env();if(initial){e.expected.search.status='ready';e.expected.search.query='';e.nodes.query.value='';}change?.(e);const r=vm.runInNewContext(ready.replace('EXPECTED_SHARE',JSON.stringify(e.expected)).replace('INITIAL_OPEN',JSON.stringify(initial)),e.context);assert.equal(r.ok,expected);checks++;}
runReady(null,true);
for(const f of [e=>e.nodes.text.readOnly=true,e=>e.nodes.cancel.disabled=true,e=>e.nodes.select.disabled=true,e=>e.nodes.query.readOnly=true,e=>e.nodes.query.value='old',e=>e.saved.query={},e=>e.panel.dataset.shareEpoch='3',e=>e.state.composing=true,e=>e.expected.search.status='searching',e=>e.nodes.send.disabled=false])runReady(f,false);
runReady(null,true,true);
runReady(e=>e.expected.search.status='partial',true,true);
runReady(e=>e.expected.search.status='failed',true,true);
runReady(e=>{e.expected.text='retained Unicode \u4e2d';e.nodes.text.value=e.expected.text;},true,true);
runReady(e=>{e.expected.selected='CADDDEST01';e.expected.options=[{id:'CADDDEST01',label:'review-destination'}];e.panel.dataset.shareSelected='CADDDEST01';e.nodes.select.value='CADDDEST01';e.nodes.select.options.push({value:'CADDDEST01',textContent:'review-destination'});e.nodes.send.disabled=false;},true,true);
for(const f of [e=>e.nodes.text.readOnly=true,e=>e.nodes.cancel.disabled=true,e=>e.nodes.select.disabled=true,e=>e.nodes.refresh.disabled=true,
 e=>e.saved.query={},e=>e.panel.dataset.shareQueryRevision='1',e=>e.panel.dataset.shareEpoch='3',e=>e.saved.queryComposing=true,
 e=>e.expected.search.status='idle',e=>{e.nodes.query.value='Dana';e.expected.search.query='Dana'},e=>e.nodes.send.disabled=false])runReady(f,false,true);
runReady(e=>{e.expected.search.status='idle';e.expected.search.query='';e.nodes.query.value=''},false);
console.log(JSON.stringify({ok:true,checks,source:'actual included search/input.js + search/ready.js',native:false}));
