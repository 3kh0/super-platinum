import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import {fileURLToPath} from 'node:url';
const here=path.dirname(fileURLToPath(import.meta.url));
const sourcePath=fs.existsSync(path.join(here,'protected_check_dom.js'))
  ? path.join(here,'protected_check_dom.js')
  : path.join(here,'../src/desktop/drafts/protected/native_fixture/check_dom.js');
const source=fs.readFileSync(sourcePath,'utf8');
function fixture() {
  let current, frame=0, captures=0, releases=0;
  class Element {
    constructor(id,r) {this.id=id;this.r=r;this.isConnected=true;this.tagName='DIV';this.className='';this.scrollTop=0;}
    getBoundingClientRect() {return this.r;}
    contains(e) {return e===this||e?.parent===this||e?.parent?.parent===this;}
  }
  class TextArea extends Element {
    get value(){return this.wire;}
    set value(v){this.wire=v;}
    setSelectionRange(start,end,direction){Object.assign(this,{selectionStart:start,selectionEnd:end,selectionDirection:direction});}
    focus(){document.activeElement=this;}
    dispatchEvent(event){this.events.push(event);}
  }
  const pane=new Element('pane',{x:200,y:0,width:468,height:380});
  pane.classList={contains:n=>n==='conversation'};
  const header=new Element('header',{x:200,y:0,width:468,height:48});
  const footer=new Element('footer',{x:200,y:290,width:468,height:90});
  const field=new TextArea('channel-composer',{x:216,y:305,width:436,height:30});
  Object.assign(field,{wire:'Initial',readOnly:false,disabled:false,events:[],tagName:'TEXTAREA',
    dataset:{draftScope:'scope',projectionVisit:'visit-1',completionReady:'true'},parent:footer});
  field.setSelectionRange(0,0,'none');
  Object.defineProperty(field,'value',{get(){return this.wire;},set(v){this.wire=v;}});
  field.closest=s=>s==='footer'?footer:null;
  footer.closest=()=>pane;
  pane.querySelector=()=>header;
  current=field;
  const hit=(e,x,y)=>x>=e.r.x&&x<=e.r.x+e.r.width&&y>=e.r.y&&y<=e.r.y+e.r.height;
  const document={getElementById:id=>id==='channel-composer'?current:null,activeElement:field,
    documentElement:{scrollWidth:668},scrollingElement:{scrollTop:0},
    elementFromPoint:(x,y)=>[field,header,footer,pane].find(e=>hit(e,x,y))};
  const controller={capture:(id,scope,visit,wire)=>{captures++;return {status:'captured',token:'1',current:{id,scope,visit,wire,input:1}};},
    release:token=>{assert.equal(token,'1');releases++;}};
  const context={document,window:{superPlatinumProjection:controller},HTMLTextAreaElement:TextArea,
    InputEvent:class {constructor(type,data){this.type=type;Object.assign(this,data);}},innerWidth:668,innerHeight:380,
    requestAnimationFrame:cb=>{frame++;if(context.onFrame)context.onFrame(frame);cb();}};
  vm.createContext(context);vm.runInContext(source,context);
  return {run:(operation,args={})=>context.protectedDraftsDOM(operation,{target:'channel',...args}),
    field,footer,header,pane,document,context,replace:()=>{current=Object.create(field);},
    counts:()=>({captures,releases})};
}
let count=0;
async function test(name,body){await body();count++;console.log(`ok ${count} - ${name}`);}
await test('whole field/header/footer and all 27 points retained',async()=>{
  const f=fixture(),r=await f.run('read');assert.equal(r.ok,true);
  for(const v of Object.values(r.hitSamples)){assert.equal(v.points.length,9);assert.equal(v.ok,true);}
  assert.equal(r.proof,null);assert.deepEqual(f.counts(),{captures:0,releases:0});
});
await test('real field setter and bubbling input preserve Unicode backward selection',async()=>{
  const f=fixture(),text='日本語 🚀 é',r=await f.run('type',{text});
  assert.equal(r.ok,true);assert.equal(r.native.text,text);assert.equal(r.native.display,text);
  assert.equal(r.native.start,text.indexOf('🚀'));assert.equal(r.native.end,text.length);
  assert.equal(r.native.direction,'backward');assert.equal(f.field.events.length,1);
  assert.equal(f.field.events[0].type,'input');assert.equal(f.field.events[0].bubbles,true);
});
await test('read-only lease refuses input without mutating value or dispatch',async()=>{
  const f=fixture();f.field.readOnly=true;assert.equal((await f.run('type',{text:'changed'})).ok,false);
  assert.equal(f.field.value,'Initial');assert.equal(f.field.events.length,0);
});
await test('mount ABA across frame wait is rejected',async()=>{
  const f=fixture();f.context.onFrame=n=>{if(n===1)f.replace();};assert.equal((await f.run('read')).ok,false);
});
await test('clipped conversation header fails',async()=>{
  const f=fixture();f.header.r.y=-8;assert.equal((await f.run('read')).ok,false);
});
await test('clipped composer footer fails',async()=>{
  const f=fixture();f.footer.r.height=100;assert.equal((await f.run('read')).ok,false);
});
await test('edge occlusion keeps actual offending element diagnostic',async()=>{
  const f=fixture(),original=f.document.elementFromPoint;
  f.document.elementFromPoint=(x,y)=>Math.abs(x-237.8)<.01&&Math.abs(y-306.5)<.01
    ? {tagName:'ASIDE',id:'occluder',className:'toast'}:original(x,y);
  const r=await f.run('read');assert.equal(r.ok,false);
  assert.equal(r.hitSamples.field.points[0].element.id,'occluder');assert.equal(r.hitSamples.field.points[0].owned,false);
});
await test('proof capture is forbidden during actual admission',async()=>{
  const f=fixture();f.field.dataset.protectedAdmission='live';assert.equal((await f.run('proof')).ok,false);
  assert.deepEqual(f.counts(),{captures:0,releases:0});
});
await test('safe proof returns actual controller snapshot and releases only own token',async()=>{
  const f=fixture(),r=await f.run('proof');assert.equal(r.ok,true);assert.equal(r.proof.current.input,1);
  assert.equal(r.proof.current.wire,'Initial');assert.deepEqual(f.counts(),{captures:1,releases:1});
});
await test('unknown operation fails closed',async()=>{
  const f=fixture();assert.equal((await f.run('clear')).ok,false);assert.equal(f.field.value,'Initial');
});
const remount={expectedScope:'returned-scope',previousVisit:'visit-1'};
function publish(f) {f.field.dataset.draftScope=remount.expectedScope;f.field.dataset.projectionVisit='visit-2';}
await test('ready refuses unspecified expected ownership',async()=>{
  const f=fixture();assert.equal((await f.run('ready')).ok,false);
});
await test('ready never accepts the still-mounted away composer',async()=>{
  const f=fixture(),r=await f.run('ready',remount);
  assert.equal(r.ok,true);assert.equal(r.ready,false);assert.equal(r.observedScope,'scope');
  assert.equal(r.expectedScope,remount.expectedScope);assert.deepEqual(f.counts(),{captures:0,releases:0});
});
await test('scope change without a new actual visit remains pending',async()=>{
  const f=fixture();f.field.dataset.draftScope=remount.expectedScope;
  assert.equal((await f.run('ready',remount)).ready,false);
});
await test('matching remount still waits for actual controllers',async()=>{
  const f=fixture();publish(f);delete f.field.dataset.completionReady;
  const r=await f.run('ready',remount);assert.equal(r.ok,true);assert.equal(r.ready,false);
  assert.equal(r.reason,'Composer controllers not mounted');
});
await test('old mount then controller installation admits only exact new identity',async()=>{
  const f=fixture();f.document.activeElement=null;
  assert.equal((await f.run('ready',remount)).ready,false);
  publish(f);f.field.dataset.completionReady='false';
  assert.equal((await f.run('ready',remount)).ready,false);
  f.field.dataset.completionReady='true';const r=await f.run('ready',remount);
  assert.equal(r.ok,true);assert.equal(r.ready,true);assert.equal(r.native.scope,remount.expectedScope);
  assert.equal(r.native.visit,'visit-2');assert.equal(f.document.activeElement,null);
  assert.deepEqual(f.counts(),{captures:0,releases:0});assert.equal(f.field.events.length,0);
});
await test('ready detects replacement during its two frames',async()=>{
  const f=fixture();publish(f);f.context.onFrame=n=>{if(n===1)f.replace();};
  const r=await f.run('ready',remount);assert.equal(r.ok,true);assert.equal(r.ready,false);
  assert.equal(r.reason,'Composer ownership changed during observation');
});
await test('ready detects controller retirement during frame wait',async()=>{
  const f=fixture();publish(f);f.context.onFrame=n=>{if(n===1)delete f.field.dataset.completionReady;};
  const r=await f.run('ready',remount);assert.equal(r.ok,true);assert.equal(r.ready,false);
});
await test('ready does not turn clipped geometry into retryable pending',async()=>{
  const f=fixture();publish(f);f.header.r.y=-8;
  const r=await f.run('ready',remount);assert.equal(r.ok,false);assert.equal(r.ready,false);
});
await test('ordinary observation still rejects controllers missing immediately',async()=>{
  const f=fixture();delete f.field.dataset.completionReady;
  const r=await f.run('focus');assert.equal(r.ok,false);assert.equal(r.reason,'Composer controllers not mounted');
});
console.log(`${count} actual-source tests passed (mock DOM, not native layout proof)`);
