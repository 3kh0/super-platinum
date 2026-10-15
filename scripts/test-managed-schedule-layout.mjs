import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import vm from 'node:vm';
import test from 'node:test';

const directory=path.dirname(fileURLToPath(import.meta.url));
const file=path.basename(directory)==='scripts'
  ? path.join(directory,'../src/desktop/view/managed_schedule_check/dom.js')
  : path.join(directory,'dom.js');
const source=fs.readFileSync(file,'utf8');
// Small geometric DOM for the actual checker. This tests its scrolling and
// evidence rules; native captures remain necessary to validate CSS/WebView layout.
function setup() {
  const elements={},all=[];
  let scroll=0,frame=0,onFrame=()=>{},obstruction=null;
  function node(name,x,y,width,height,parent=null,inBody=false) {
    const element={name,id:name,tagName:'DIV',className:'',textContent:name,disabled:false,
      isConnected:true,parentElement:parent,dataset:{},children:[],
      getBoundingClientRect:()=>({x,y:y-(inBody?scroll:0),width,height}),
      contains(hit){for(let p=hit;p;p=p.parentElement)if(p===this)return true;return false;},
      querySelector(){return null;},getAttribute(){return null;}};
    parent?.children.push(element);elements[name]=element;all.push(element);return element;
  }
  const dialog=node('managed-schedule-dialog',100,16,520,330);
  dialog.dataset={editorGeneration:'71',composing:'false'};
  const header=node('header',116,32,488,32,dialog);
  const title=node('managed-schedule-title',116,36,280,24,header);
  const close=node('close',572,32,32,32,header);
  const destination=node('destination',116,76,488,18,dialog);
  const body=node('managed-schedule-body',116,106,488,180,dialog);
  body.scrollHeight=320;body.clientHeight=180;
  Object.defineProperty(body,'scrollTop',{get:()=>scroll,set:value=>scroll=Math.max(0,Math.min(value,body.scrollHeight-body.clientHeight))});
  const label=node('managed-schedule-message-label',116,106,488,18,body,true);
  const toolbar=node('toolbar',116,124,488,32,body,true);
  const editor=node('managed-schedule-message',116,156,488,96,body,true);
  editor.previousElementSibling=toolbar;
  const dateLabel=node('dateLabel',116,306,488,18,body,true);
  const date=node('managed-schedule-date',116,336,488,32,body,true);
  date.value='2030-01-01T12:00';
  const hint=node('managed-schedule-hint',116,380,488,36,body,true);
  const footer=node('footer',116,298,488,32,dialog);
  const primary=node('primary',444,298,160,32,footer);
  const selectors={':scope > header':header,'#managed-schedule-title':title,
    ':scope > p.scheduled-hint':destination,':scope > footer':footer,
    '#managed-schedule-message-label':label,'label[for="managed-schedule-date"]':dateLabel,
    '#managed-schedule-hint':hint,'footer button.primary':primary,'[aria-label="Close scheduling"]':close};
  dialog.querySelector=selector=>selectors[selector]??null;
  const ancestor={scrollTop:91};
  const active={id:'original-focus'};
  const document={activeElement:active,documentElement:{scrollWidth:668},
    getElementById:id=>elements[id]??null,querySelector:()=>null,
    elementFromPoint(x,y) {
      if(x<0||y<0||x>668||y>380)return null;
      if(obstruction)return obstruction;
      const contains=(r)=>x>=r.x&&x<=r.x+r.width&&y>=r.y&&y<=r.y+r.height;
      return all.filter(e=>e.isConnected&&contains(e.getBoundingClientRect())&&
        (!body.contains(e)||e===body||contains(body.getBoundingClientRect()))).at(-1)??null;
    }};
  const context={document,innerWidth:668,innerHeight:380,Date,window:{superPlatinumRichEditor:{read:()=>[{type:'rich_text',elements:[]}]}},
    requestAnimationFrame:callback=>setTimeout(()=>{onFrame(++frame);callback();},0)};
  const check=vm.runInNewContext(source+';managedScheduleCheck',context);
  return {check:(op,extra={})=>check({op,target:'channel',...extra}),elements,body,dialog,ancestor,document,active,
    onFrame:fn=>onFrame=fn,obstruct:()=>obstruction={tagName:'DIV',id:'cover',className:'cover'}};
}

test('actual reveal scrolls only body and preserves focus plus generation',async()=>{
  const h=setup();const first=await h.check('reveal',{view:'editor'});
  const second=await h.check('reveal',{view:'date'});
  assert.equal(first.ok,true);assert.equal(second.ok,true);
  assert.equal(first.after.scrollTop,0);assert.equal(second.after.scrollTop,140);
  assert.deepEqual(second.before,first.after);
  assert.equal(second.before.generation,second.after.generation);
  assert.equal(h.ancestor.scrollTop,91);assert.equal(h.document.activeElement,h.active);
});
test('editor measurement keeps truthful clipped date evidence without claiming it visible',async()=>{
  const h=setup();await h.check('reveal',{view:'editor'});
  const value=await h.check('measure',{view:'editor'});
  assert.equal(value.ok,true);assert.equal(value.controls.hint.visible,false);
  assert.equal(value.controls.hint.unobscured,false);
  assert.equal(value.controls.hint.points.length,9);
  assert(value.controls.hint.points.some(p=>p.hit===null&&p.owned===false));
});
test('date measurement requires actual date viewport and preserves clipped editor evidence',async()=>{
  const h=setup();assert.equal((await h.check('measure',{view:'date'})).ok,false);
  await h.check('reveal',{view:'date'});
  const value=await h.check('measure',{view:'date'});
  assert.equal(value.ok,true);assert.equal(value.controls.editor.unobscured,false);
  assert.equal(value.controls.date.unobscured,true);assert.equal(value.controls.hint.unobscured,true);
});
test('selected controls must fit body even if their sample points still fit viewport',async()=>{
  const h=setup();h.elements['managed-schedule-message'].getBoundingClientRect=()=>({x:116,y:156,width:488,height:140});
  const value=await h.check('measure',{view:'editor'});
  assert.equal(value.controls.editor.visible,true);assert.equal(value.controls.editor.unobscured,true);
  assert.equal(value.ok,false);
});
test('stable header/footer obstruction cannot be hidden by selected body scroll',async()=>{
  const h=setup();await h.check('reveal',{view:'date'});h.obstruct();
  const value=await h.check('measure',{view:'date'});
  assert.equal(value.ok,false);assert.equal(value.controls.primary.unobscured,false);
});
test('offscreen dialog remains rejected even if child rectangles are in viewport',async()=>{
  const h=setup();h.dialog.getBoundingClientRect=()=>({x:100,y:310,width:520,height:330});
  assert.equal((await h.check('measure',{view:'editor'})).ok,false);
});
test('missing mount or unknown requested view refuses reveal',async()=>{
  const h=setup();assert.equal((await h.check('reveal',{view:'other'})).ok,false);
  delete h.elements['managed-schedule-body'];
  assert.equal((await h.check('reveal',{view:'editor'})).ok,false);
});
test('mount generation change or disconnection during actual scroll refuses receipt',async()=>{
  const changed=setup();changed.onFrame(n=>{if(n===3)changed.dialog.dataset.editorGeneration='72';});
  assert.equal((await changed.check('reveal',{view:'date'})).ok,false);
  const removed=setup();removed.onFrame(n=>{if(n===3)removed.body.isConnected=false;});
  assert.equal((await removed.check('reveal',{view:'date'})).ok,false);
});
