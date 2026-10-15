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
function setup() {
  let current,clicks=0,frames=0,covered=false,remove=true,replaced=false;
  const text={textContent:'Message scheduled.'};
  const button={id:'',tagName:'BUTTON',className:'toast-dismiss',textContent:'',disabled:false,
    getBoundingClientRect:()=>({x:612,y:326,width:24,height:24}),
    contains:node=>node===button,
    focus(options){assert.equal(options.preventScroll,true);document.activeElement=button;if(replaced)current={};},
    click(){clicks++;if(remove)current=null;}};
  const toast={id:'status-toast',tagName:'DIV',className:'toast',textContent:text.textContent,
    getBoundingClientRect:()=>({x:460,y:314,width:190,height:48}),
    getAttribute:()=> 'status',contains:node=>node===toast||node===button,
    querySelector:selector=>selector==='.toast-text'?text:selector==='.toast-dismiss[aria-label="Dismiss notification"]'?button:null};
  current=toast;
  const document={activeElement:{id:'',tagName:'BODY',textContent:'PRIVATE STYLE AND RUNTIME SOURCE'},documentElement:{scrollWidth:668},
    getElementById:id=>id==='status-toast'?current:null,querySelector:()=>null,
    elementFromPoint(x,y){if(covered)return {id:'cover',tagName:'DIV',className:'overlay'};return x>=612&&x<=636&&y>=326&&y<=350?button:toast;}};
  const context={document,innerWidth:668,innerHeight:380,Date,window:{},
    requestAnimationFrame:callback=>setTimeout(()=>{frames++;callback();},0)};
  const check=vm.runInNewContext(source+';managedScheduleCheck',context);
  return {run:()=>check({op:'dismiss-notice',target:'channel',expected:'Message scheduled.'}),
    check,document,toast,button,text,clicks:()=>clicks,frames:()=>frames,
    cover:()=>covered=true,retain:()=>remove=false,replaceOnFocus:()=>replaced=true,absent:()=>current=null};
}
test('real dismiss retains exact status/control proof and waits for DOM absence',async()=>{
  const h=setup(),receipt=await h.run();
  assert.equal(receipt.ok,true);assert.equal(receipt.clicked,true);assert.equal(receipt.afterAbsent,true);
  assert.equal(receipt.before.text,'Message scheduled.');assert.equal(receipt.before.role,'status');
  assert.equal(receipt.before.toast.points.length,9);assert.equal(receipt.before.dismiss.points.length,9);
  assert.equal(receipt.before.toast.unobscured,true);assert.equal(receipt.before.dismiss.unobscured,true);
  assert.equal(h.clicks(),1);assert(h.frames()>=3);
});
test('unexpected notice copy and non-status elements are never dismissed',async()=>{
  const wrong=setup();wrong.text.textContent='A different operation failed';assert.equal((await wrong.run()).ok,false);assert.equal(wrong.clicks(),0);
  const role=setup();role.toast.getAttribute=()=> 'dialog';assert.equal((await role.run()).ok,false);assert.equal(role.clicks(),0);
});
test('absent notice does not fabricate a success receipt',async()=>{
  const h=setup();h.absent();assert.equal((await h.run()).ok,false);assert.equal(h.clicks(),0);
});
test('covered or disabled dismiss controls retain failure instead of clearing state',async()=>{
  const covered=setup();covered.cover();assert.equal((await covered.run()).ok,false);assert.equal(covered.clicks(),0);
  const disabled=setup();disabled.button.disabled=true;assert.equal((await disabled.run()).ok,false);assert.equal(disabled.clicks(),0);
});
test('offscreen toast is not treated as dismissable',async()=>{
  const h=setup();h.toast.getBoundingClientRect=()=>({x:460,y:360,width:190,height:48});
  assert.equal((await h.run()).ok,false);assert.equal(h.clicks(),0);
});
test('replacement during focus refuses click',async()=>{
  const h=setup();h.replaceOnFocus();assert.equal((await h.run()).ok,false);assert.equal(h.clicks(),0);
});
test('failure to remove notice is bounded and does not repeat click',async()=>{
  const h=setup();h.retain();const result=await h.run();
  assert.equal(result.ok,false);assert.equal(result.afterAbsent,false);assert.equal(h.clicks(),1);assert.equal(h.frames(),62);
});
test('BODY and HTML focus diagnostics exclude injected source; controls are bounded',async()=>{
  const h=setup();
  const panel={textContent:'Panel',getBoundingClientRect:()=>({x:0,y:0,width:668,height:380}),contains:()=>true,
    querySelector:()=>panel};
  h.document.querySelector=selector=>selector==='.scheduled-panel'?panel:null;
  for(const tag of ['BODY','HTML']) {
    h.document.activeElement={tagName:tag,textContent:'PRIVATE STYLE AND RUNTIME SOURCE'};
    assert.equal((await h.check({op:'measure',target:'channel'})).active.text,'');
  }
  h.document.activeElement={tagName:'BUTTON',textContent:'x'.repeat(600)};
  assert.equal((await h.check({op:'measure',target:'channel'})).active.text.length,512);
});
