import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import {fileURLToPath} from 'node:url';
const here=path.dirname(fileURLToPath(import.meta.url));
const sourcePath=fs.existsSync(path.join(here,'protected_enrollment_check_dom.js'))
  ? path.join(here,'protected_enrollment_check_dom.js')
  : path.join(here,'../src/desktop/drafts/protected/enrollment_fixture/check_dom.js');
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
  return {run:(operation,args={})=>context.protectedEnrollmentNative(operation,{target:'channel',expectedScope:'scope',...args}),
    field,footer,header,pane,document,context,replace:()=>{current=Object.create(field);},
    counts:()=>({captures,releases})};
}

let count=0;
async function test(name,body){await body();count++;console.log(`ok ${count} - ${name}`);}
await test('held consent read never replaces admission/projection tickets',async()=>{
  const f=fixture();f.field.dataset.protectedAdmission='live';f.field.readOnly=true;
  const r=await f.run('read');assert.equal(r.ok,true);assert.equal(r.admission,'live');
  assert.equal(r.readOnly,true);assert.equal(r.visibilityEvidence,'not-measured');
  assert.deepEqual(f.counts(),{captures:0,releases:0});assert.equal(f.field.events.length,0);
});
await test('exact expected scope and optional visit are required',async()=>{
  const f=fixture();
  for(const args of [{expectedScope:''},{expectedScope:'old'},{expectedVisit:'different'}])
    assert.equal((await f.run('read',args)).ok,false);
});
await test('missing controller fails immediately without altering field',async()=>{
  const f=fixture();delete f.field.dataset.completionReady;
  assert.equal((await f.run('type',{text:'changed'})).ok,false);assert.equal(f.field.value,'Initial');
});
await test('late real setter input preserves Unicode backward selection',async()=>{
  const f=fixture(),text='Later '+String.fromCodePoint(0x65e5,0x672c,0x8a9e)+' '+String.fromCodePoint(0x1f680)+' e'+String.fromCodePoint(0x301);
  const r=await f.run('type',{text});assert.equal(r.ok,true);assert.equal(r.native.text,text);
  assert.equal(r.native.display,text);assert.equal(r.native.start,text.indexOf(String.fromCodePoint(0x1f680)));
  assert.equal(r.native.end,text.length);assert.equal(r.native.direction,'backward');
  assert.equal(f.field.events.length,1);assert.equal(f.field.events[0].type,'input');
  assert.equal(f.field.events[0].bubbles,true);assert.deepEqual(f.counts(),{captures:0,releases:0});
});
await test('locked field refuses simulated input',async()=>{
  const f=fixture();f.field.dataset.protectedAdmission='live';
  assert.equal((await f.run('type',{text:'changed'})).ok,false);assert.equal(f.field.value,'Initial');
  assert.equal(f.field.events.length,0);
});
await test('actual field replacement across frames invalidates observation',async()=>{
  const f=fixture();f.context.onFrame=n=>{if(n===1)f.replace();};
  assert.equal((await f.run('read')).ok,false);
});
await test('controller retirement across frames invalidates observation',async()=>{
  const f=fixture();f.context.onFrame=n=>{if(n===1)delete f.field.dataset.completionReady;};
  assert.equal((await f.run('read')).ok,false);
});
await test('unknown operation cannot clear or capture',async()=>{
  const f=fixture();for(const op of ['clear','capture','proof','release'])assert.equal((await f.run(op)).ok,false);
  assert.equal(f.field.value,'Initial');assert.deepEqual(f.counts(),{captures:0,releases:0});
});
await test('readiness waits for exact new scope and visit only',async()=>{
  const f=fixture();assert.equal((await f.run('ready',{previousVisit:'visit-1'})).ready,false);
  f.field.dataset.projectionVisit='visit-2';
  assert.equal((await f.run('ready',{previousVisit:'visit-1',expectedScope:'new'})).ready,false);
  assert.equal((await f.run('ready',{previousVisit:'visit-1'})).ready,true);
  assert.equal((await f.run('ready')).ok,false);
});

function modalFixture() {
  let frame=0;const all=[];
  class Element {
    constructor(id,r,parent=null,text='') {Object.assign(this,{id,r,parent,textContent:text,isConnected:true,tagName:'DIV',className:'',dataset:{},children:[],disabled:false,scrollTop:0});all.push(this);if(parent)parent.children.push(this);}
    getBoundingClientRect(){return {...this.r,y:this.r.y-(this.content?body.scrollTop:0)};}
    contains(e){for(let n=e;n;n=n.parent)if(n===this)return true;return false;}
    getAttribute(n){return n==='aria-label'&&this.id===''?'Close enrollment':null;}
    querySelector(q){return q==='#protected-enrollment-inventory'?body:q===':scope > header'?header:q===':scope > form > footer'?footer:q==='#protected-enrollment-title'?title:null;}
    querySelectorAll(q){return all.filter(e=>this.contains(e)&&(q==='button'?e.tagName==='BUTTON':q==='.protected-enrollment-draft'?e.className===q.slice(1):false));}
    closest(q){return q==='#protected-enrollment-dialog'&&dialog.contains(this)?dialog:null;}
    focus(){document.activeElement=this;}
    click(){this.clicks=(this.clicks??0)+1;}
  }
  const dialog=new Element('protected-enrollment-dialog',{x:200,y:50,width:440,height:280});dialog.dataset.dialogGeneration='protected-20';
  const header=new Element('header',{x:200,y:50,width:440,height:60},dialog);
  const title=new Element('protected-enrollment-title',{x:210,y:60,width:330,height:30},header,'Enable channel draft recovery?');
  const body=new Element('protected-enrollment-inventory',{x:200,y:110,width:440,height:160},dialog,'all saved content');body.clientHeight=160;body.scrollHeight=360;
  const channel=new Element('channel-row',{x:210,y:130,width:420,height:90},body,'channel saved content');channel.className='protected-enrollment-draft';channel.dataset.draftKind='channel';channel.content=true;
  const thread=new Element('thread-row',{x:210,y:260,width:420,height:90},body,'thread saved content');thread.className='protected-enrollment-draft';thread.dataset.draftKind='thread';thread.content=true;
  const footer=new Element('footer',{x:200,y:270,width:440,height:60},dialog);
  const close=new Element('',{x:600,y:65,width:28,height:28},header);close.tagName='BUTTON';
  const cancel=new Element('protected-enrollment-cancel',{x:210,y:285,width:190,height:30},footer,'Cancel');cancel.tagName='BUTTON';
  const submit=new Element('protected-enrollment-submit',{x:420,y:285,width:210,height:30},footer,'Enable recovery');submit.tagName='BUTTON';
  const outside=new Element('outside',{x:0,y:0,width:1,height:1});
  const document={getElementById:id=>all.find(e=>e.id===id&&e.isConnected),activeElement:null,scrollingElement:{scrollTop:0},documentElement:{scrollWidth:668},elementFromPoint:(x,y)=>{
    if(context.cover?.(x,y))return outside;
    return [...all].reverse().find(e=>{if(e===outside)return false;const r=e.getBoundingClientRect();if(!e.isConnected||x<r.x||x>r.x+r.width||y<r.y||y>r.y+r.height)return false;
      return !e.content||(y>=110&&y<=270);});}};
  const context={document,innerWidth:668,innerHeight:380,getComputedStyle:()=>({borderTopLeftRadius:'6px',borderTopRightRadius:'6px',borderBottomRightRadius:'6px',borderBottomLeftRadius:'6px'}),requestAnimationFrame:cb=>{if(context.onFrame)context.onFrame(++frame);cb();}};
  vm.createContext(context);vm.runInContext(source,context);
  return {context,document,dialog,body,header,footer,channel,thread,submit,cancel,
    surface:page=>context.protectedEnrollmentSurface({page}),click:(control='submit',text='Enable recovery')=>context.protectedEnrollmentClick({control,text})};
}
await test('whole inventory page scrolls only modal body',async()=>{
  const f=modalFixture();const r=await f.surface('thread');assert.equal(r.ok,true);
  assert.ok(f.body.scrollTop>0);assert.equal(f.document.scrollingElement.scrollTop,0);
  assert.equal(r.text,'thread saved content');assert.equal(r.hits.selected.points.length,9);
  assert.equal(r.surfaces.selected.y>=r.surfaces.body.y,true);
});
await test('held modal observes viewport without claiming every row readable',async()=>{
  const f=modalFixture();f.body.scrollTop=75;const r=await f.surface('modal');assert.equal(r.ok,true);
  assert.equal(f.body.scrollTop,75);assert.equal(r.text,'all saved content');
  assert.equal(r.surfaces.selected.height,r.surfaces.body.height);
});
await test('partial whole row and ancestor scroll remain fatal',async()=>{
  let f=modalFixture();f.thread.r.height=190;assert.equal((await f.surface('thread')).ok,false);
  f=modalFixture();f.document.scrollingElement.scrollTop=20;assert.equal((await f.surface('channel')).ok,false);
});
await test('one footer or selected-row occlusion fails strict owned grid',async()=>{
  let f=modalFixture();f.context.cover=(x,y)=>Math.abs(x-222)<1&&Math.abs(y-273)<1;
  assert.equal((await f.surface('channel')).ok,false);
  f=modalFixture();f.context.cover=(x,y)=>x>220&&x<240&&y>130&&y<145;
  assert.equal((await f.surface('channel')).ok,false);
});
await test('dialog generation or node replacement across frames invalidates capture',async()=>{
  let f=modalFixture();f.context.onFrame=()=>{f.dialog.dataset.dialogGeneration='replacement';};assert.equal((await f.surface('channel')).ok,false);
  f=modalFixture();f.context.onFrame=()=>{f.thread.isConnected=false;};assert.equal((await f.surface('thread')).ok,false);
});
await test('actual submit restores own body then focuses and clicks once',async()=>{
  const f=modalFixture();f.body.scrollTop=150;const r=await f.click();assert.equal(r.ok,true);
  assert.equal(r.bodyScroll,0);assert.equal(r.focused,true);assert.equal(f.submit.clicks,1);assert.equal(f.document.scrollingElement.scrollTop,0);
});
await test('unsupported radius or edge-covered button cannot be clicked',async()=>{
  let f=modalFixture();f.context.getComputedStyle=()=>({borderTopLeftRadius:'50%',borderTopRightRadius:'6px',borderBottomRightRadius:'6px',borderBottomLeftRadius:'6px'});
  assert.equal((await f.click()).ok,false);assert.equal(f.submit.clicks,undefined);
  f=modalFixture();f.context.cover=(x,y)=>Math.abs(x-525)<1&&Math.abs(y-286)<1;
  assert.equal((await f.click()).ok,false);assert.equal(f.submit.clicks,undefined);
});
await test('disabled wrong-copy and replaced controls never dispatch',async()=>{
  let f=modalFixture();f.submit.disabled=true;assert.equal((await f.click()).ok,false);
  f=modalFixture();assert.equal((await f.click('submit','Wrong action')).ok,false);
  f=modalFixture();f.context.onFrame=()=>{f.submit.isConnected=false;};assert.equal((await f.click()).ok,false);
  assert.equal(f.submit.clicks,undefined);
});
console.log(`${count} actual-source tests passed (mock DOM, not native layout proof)`);
