// Executes the exact shared positioning script with the component's adaptations.
// Mock geometry is an algorithm regression, never native CSS/visual evidence.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import {fileURLToPath} from 'node:url';
const here=path.dirname(fileURLToPath(import.meta.url));
let root=here;while(!fs.existsSync(path.join(root,'Cargo.toml'))){const next=path.dirname(root);if(next===root)throw Error('Repository root missing');root=next;}
const component=fs.existsSync(path.join(here,'ui.rs'))?path.join(here,'ui.rs'):path.join(root,'src/desktop/drafts/protected/enrollment_fixture/ui.rs');
const ui=fs.readFileSync(component,'utf8'),source=fs.readFileSync(path.join(root,'src/desktop/view/toast_position.js'),'utf8');
const block=ui.match(/let script=include_str!\("\.\.\/\.\.\/\.\.\/view\/toast_position\.js"\)(.*?);document::eval\(&script\)/s)?.[1];
assert.ok(block,'Actual component script adaptation missing');
const replacements=[...block.matchAll(/\.replace\(("(?:\\.|[^"\\])*")\s*,\s*("(?:\\.|[^"\\])*")\)/g)].map(m=>[JSON.parse(m[1]),JSON.parse(m[2])]);
assert.deepEqual(replacements,[['\'status-toast\'','\'protected-enrollment-status\''],['.composer-wrap,','#status-toast, .composer-wrap,'],['.toast-dismiss','.protected-enrollment-actions']]);
const script=(fixed=true)=>replacements.filter(([from])=>fixed||from!=='.toast-dismiss').reduce((text,[from,to])=>text.split(from).join(to),source);

function run({width=1280,height=780,composerTop=690,actionsHeight=32,hidden=false,fixed=true}={}) {
  const writes=[],listeners=new Map(),pending=[];let actionHeight=actionsHeight,resizeCallback;
  const bounds={width:318,height:107,left:width-18-318,right:width-18,top:height-18-107,bottom:height-18};
  const actions={getBoundingClientRect:()=>({height:actionHeight})};
  const toast={isConnected:true,dataset:{},scrollHeight:107,style:{getPropertyValue:key=>writes.findLast(v=>v[0]===key)?.[1]??'',setProperty:(key,value)=>writes.push([key,value])},
    contains:()=>false,getBoundingClientRect:()=>bounds,querySelector:selector=>selector==='.protected-enrollment-actions'?actions:null};
  const composer={getBoundingClientRect:()=>({width:width-300,height:height-composerTop,left:300,right:width,top:composerTop,bottom:height})};
  const document={hidden,activeElement:null,body:{},getElementById:id=>id==='protected-enrollment-status'?toast:null,querySelectorAll:()=>[composer],
    addEventListener:(name,callback)=>listeners.set(name,callback),removeEventListener:name=>listeners.delete(name)};
  const context={document,innerWidth:width,innerHeight:height,getComputedStyle:()=>({right:'18px',lineHeight:'19.6px',paddingTop:'9px',paddingBottom:'9px',borderTopWidth:'1px',borderBottomWidth:'1px',getPropertyValue:()=> '8px'}),
    ResizeObserver:class{constructor(callback){resizeCallback=callback;}observe(){}unobserve(){}disconnect(){}},
    MutationObserver:class{observe(){}disconnect(){}},requestAnimationFrame:cb=>{pending.push(cb);return pending.length;},cancelAnimationFrame:()=>{},window:{addEventListener(){},removeEventListener(){}}};
  vm.createContext(context);vm.runInContext(script(fixed),context);
  return {toast,writes,context,resize:h=>{actionHeight=h;resizeCallback();while(pending.length)pending.shift()();}};
}
let count=0;function test(name,body){body();console.log(`ok ${++count} - ${name}`);}
test('original missing-dismiss adaptation never reveals status',()=>{
  const r=run({fixed:false});assert.equal(r.toast.dataset.positioned,undefined);assert.equal(r.writes.length,0);
});
test('normal action row positions status above actual composer',()=>{
  const r=run();assert.equal(r.toast.dataset.positioned,'true');
  assert.equal(r.toast.style.getPropertyValue('--toast-bottom'),'98px');assert.equal(r.toast.style.getPropertyValue('--toast-max-height'),'664px');
});
test('narrow wrapped action row remains measured and reachable',()=>{
  const r=run({width:668,height:380,composerTop:290,actionsHeight:72});assert.equal(r.toast.dataset.positioned,'true');
  assert.equal(r.toast.style.getPropertyValue('--toast-bottom'),'98px');assert.equal(r.toast.style.getPropertyValue('--toast-max-height'),'264px');
});
test('wrapped action height that cannot fit keeps surface hidden',()=>{
  const r=run({width:668,height:380,composerTop:100,actionsHeight:72});assert.equal(r.toast.dataset.positioned,undefined);
  assert.equal(r.writes.length,0);
});
test('resizing controls rechecks minimum chrome without forcing visibility',()=>{
  const r=run({width:668,height:380,composerTop:100,actionsHeight:32});assert.equal(r.toast.dataset.positioned,'true');
  r.resize(72);assert.equal(r.toast.dataset.positioned,undefined);
  r.resize(32);assert.equal(r.toast.dataset.positioned,'true');
});
test('absent zero-height controls cannot bypass no-fit guard',()=>{
  const r=run({actionsHeight:0});assert.equal(r.toast.dataset.positioned,undefined);
});
test('hidden document remains hidden regardless of available space',()=>{
  const r=run({hidden:true});assert.equal(r.toast.dataset.positioned,undefined);
});
console.log(`${count} actual adapted-source tests passed (mock geometry; native rerun required)`);
