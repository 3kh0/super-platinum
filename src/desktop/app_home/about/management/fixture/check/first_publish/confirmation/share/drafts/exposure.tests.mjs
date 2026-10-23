import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const read=name=>fs.readFileSync(new URL(name,import.meta.url),'utf8');
const geometry=read('geometry.js'),actionSource=read('actions.js');
let count=0;
function fixture({covered=true,foreign=false,replaced=false}={}){
 const listeners=new Map(),elements=new Map(),ids=new WeakMap();let next=1,clicked=0,inputs=0;
 const node=(id,left=20,right=200)=>({id,isConnected:true,disabled:false,readOnly:false,hidden:false,tagName:'BUTTON',parentElement:null,dataset:{},style:{},textContent:id,
  getBoundingClientRect:()=>({left,top:100,right,bottom:140,width:right-left,height:40}),scrollIntoView(){},focus(){document.activeElement=this},contains(n){return n===this},addEventListener(t,f){this.listener={t,f}},removeEventListener(){this.listener=null},dispatchEvent(e){inputs++;e.target=this;this.listener?.f(e)},click(){clicked++;this.listener?.f({type:'click',isTrusted:false,target:this})},setSelectionRange(){}});
 const pane=node('pane',100,600),thread=node('thread-composer',120,500),body=node('thread-body',100,600),root=node('workflow-management');
 pane.contains=e=>e===pane||e===thread||e===body;
 const title=node('workflow-management-title',20,400);title.tagName='INPUT';
 const description=node('workflow-management-description',20,400);description.tagName='TEXTAREA';
 const button=node('workflow-management-finish-up',200,400);button.textContent='Finish Up';
 const messages=node('messages',200,400);messages.textContent='Messages';
 const other=node('foreign',100,600);
 [thread,body,root,title,description,button].forEach(e=>{elements.set(e.id,e);ids.set(e,next++)});
 const document={activeElement:null,getElementById:id=>elements.get(id),querySelector:s=>s==='.thread-pane'?pane:null,
 querySelectorAll:s=>s==='.channel-tabs button'?[messages]:[],
 elementFromPoint:(x,y)=>covered&&x>=100?(foreign?other:pane):(y>=100&&y<=140?title:null),
 addEventListener:(t,f)=>listeners.set(t,f),removeEventListener:t=>listeners.delete(t)};
 const registry={ids,next,parent:{root,title,description},shareDrafts:{pane,thread:replaced?node('old'):thread,body}};
 const window={__firstPublishRegistry:registry};
 const context=vm.createContext({window,document,innerWidth:650,innerHeight:500,getComputedStyle:e=>({display:e.hidden?'none':'block',visibility:'visible'}),HTMLTextAreaElement:{prototype:{}},HTMLInputElement:{prototype:{}},Event:class{constructor(type){this.type=type;this.isTrusted=false}}});
 for(const k of ['HTMLInputElement','HTMLTextAreaElement'])Object.defineProperty(context[k].prototype,'value',{set(v){this.value=v}});
 function action(kind,label,value='value'){
  const code=actionSource.replace('GEOMETRY',geometry).replace('KIND',JSON.stringify(kind)).replace('LABEL',JSON.stringify(label)).replace('VALUE',JSON.stringify(value)).replace('EXPECTED','null').replace('ROOT','"root"');
  return vm.runInContext(code,context);
 }
 const event=(target,extra={})=>({target,isTrusted:true,key:'Enter',code:'Enter',repeat:false,detail:0,preventDefault(){this.prevented=true},stopImmediatePropagation(){this.stopped=true},...extra});
 root.querySelectorAll=s=>s==='.workflow-management-actions button'?[button]:[];
 function observe(extra={}){const base={ok:false,present:true,modal:null,confirmationAdd:null,destination:null,share:null,outcomes:[],originalNodes:true,noOverflow:true,composing:false,controls:[{id:button.id,nodeId:ids.get(button),visible:false,hit:false}],feedback:[],...extra};return vm.runInContext(read('exposure.js').replace('BASE',JSON.stringify(base)).replace('GEOMETRY',geometry),context)}
 return{action,observe,context,document,pane,thread,button,title,messages,listeners,event,stats:()=>({clicked,inputs})};
}
function test(name,run){run();count++;console.log('ok',name)}
test('partly exposed original metadata uses actual left point',()=>{const f=fixture();const a=f.action('metadata','workflow-management-title','exact');assert.equal(a.ok,true);assert.equal(a.kind,'dom-input');assert.ok(a.hitPoint.x<100);assert.equal(a.exposure.centerHit,false);assert.equal(f.title.value,'exact');assert.equal(f.stats().inputs,1)});
test('replacement original thread cannot grant covered-button keyboard',()=>{const f=fixture({replaced:true});assert.equal(f.action('button','Finish Up').ok,false);assert.equal(f.stats().clicked,0)});
test('foreign overlay cannot grant keyboard',()=>{const f=fixture({foreign:true});assert.equal(f.action('button','Finish Up').ok,false)});
test('fully occluded original button asks once without clicking',()=>{const f=fixture();const a=f.action('button','Finish Up');assert.equal(a.kind,'keyboard-request');assert.equal(f.stats().clicked,0);assert.equal(f.action('button','Finish Up').ok,false)});
test('actual trusted Enter and native click recorded',()=>{const f=fixture();f.action('button','Finish Up');f.listeners.get('keydown')(f.event(f.button));f.listeners.get('click')(f.event(f.button));const p=f.context.window.__firstPublishShareKeyboard;assert.equal(p.keydown.trusted,true);assert.equal(p.click.trusted,true);assert.equal(f.listeners.size,0)});
test('synthetic Enter refused before native activation',()=>{const f=fixture();f.action('button','Finish Up');const e=f.event(f.button,{isTrusted:false});f.listeners.get('keydown')(e);assert.equal(e.prevented,true);assert.ok(f.context.window.__firstPublishShareKeyboard.failure)});
test('replaced node after focus refuses trusted Enter',()=>{const f=fixture();f.action('button','Finish Up');f.button.isConnected=false;const e=f.event(f.button);f.listeners.get('keydown')(e);assert.equal(e.prevented,true)});
test('wrong focused target refuses native click',()=>{const f=fixture();f.action('button','Finish Up');f.listeners.get('keydown')(f.event(f.button));f.document.activeElement=f.title;const e=f.event(f.button);f.listeners.get('click')(e);assert.equal(e.prevented,true)});
test('nonallowlisted fully occluded target refuses',()=>{const f=fixture();assert.equal(f.action('tab','Other').ok,false)});
test('Messages uses same guarded original-thread keyboard route',()=>{const f=fixture();assert.equal(f.action('tab','Messages').kind,'keyboard-request')});
test('passive covered control remains explicitly not visible',()=>{const f=fixture(),v=f.observe();assert.equal(v.ok,true);assert.equal(v.controls[0].visible,false);assert.equal(v.controls[0].hit,false);assert.equal(v.controls[0].exposure.coveredByThread,true)});
test('passive foreign cover cannot replace original thread evidence',()=>{const f=fixture({foreign:true});assert.equal(f.observe().ok,false)});
test('passive retired thread binding cannot authorize background',()=>{const f=fixture({replaced:true});assert.equal(f.observe().ok,false)});
test('stale child pane failure is not repaired',()=>{for(const key of ['modal','confirmationAdd','destination','share'])assert.equal(fixture().observe({[key]:{}}).ok,false)});
test('missing original controls cannot become ready',()=>{const f=fixture();assert.equal(f.observe({controls:[]}).ok,false)});
console.log(`${count} Share-draft exposure/activation tests passed`);
