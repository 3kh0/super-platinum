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
  let current,frames=0,covered=false;
  const submit={isConnected:true,disabled:false,textContent:'Schedule message',tagName:'BUTTON',id:'',className:'primary',
    getBoundingClientRect:()=>({x:430,y:320,width:190,height:36}),contains:node=>node===submit,
    click(){assert.fail('Retirement must never manufacture a detached click');},
    focus(){assert.fail('Retirement must not move focus');}};
  const dialog={isConnected:true,dataset:{editorGeneration:'42'},querySelectorAll:()=>[submit]};
  current=dialog;
  const document={activeElement:null,documentElement:{scrollWidth:668},
    getElementById:id=>id==='managed-schedule-dialog'?current:null,querySelector:()=>null,
    elementFromPoint:()=>covered?{tagName:'DIV',id:'cover',className:''}:submit};
  const window={};
  const check=vm.runInNewContext(source+';managedScheduleCheck',{
    document,window,innerWidth:668,innerHeight:380,
    requestAnimationFrame:callback=>setTimeout(()=>{frames++;callback();},0),
  });
  return {dialog,submit,window,frames:()=>frames,
    cover:()=>covered=true,
    retire(){dialog.isConnected=false;submit.isConnected=false;current=null;},
    replace(){dialog.isConnected=false;submit.isConnected=false;current={dataset:{editorGeneration:'43'}};},
    retain:()=>check({op:'retain-review',target:'channel',label:'Schedule message'}),
    finish:()=>check({op:'retire-review',target:'channel'})};
}

test('retains the actual connected generation and all nine submit hit points',async()=>{
  const h=setup(),proof=await h.retain();
  assert.equal(proof.ok,true);assert.equal(proof.generation,'42');
  assert.equal(proof.dialogConnected,true);assert.equal(proof.submitConnected,true);
  assert.equal(proof.primary.points.length,9);assert.equal(proof.primary.unobscured,true);
  assert.equal(h.window.__managedScheduleRetirement.dialog,h.dialog);
  assert.equal(h.window.__managedScheduleRetirement.submit,h.submit);
});
test('actual disconnection and no replacement complete retirement without a click',async()=>{
  const h=setup();await h.retain();h.retire();const proof=await h.finish();
  assert.deepEqual(JSON.parse(JSON.stringify(proof)),{ok:true,generation:'42',dialogDisconnected:true,submitDisconnected:true,currentDialogAbsent:true});
  assert.equal(h.window.__managedScheduleRetirement,undefined);
  assert.equal((await h.finish()).ok,false);
});
test('visible replacement dialog cannot be reported as retired current UI',async()=>{
  const h=setup();await h.retain();h.replace();const proof=await h.finish();
  assert.equal(proof.ok,false);assert.equal(h.frames(),64);
  assert.equal(h.window.__managedScheduleRetirement,undefined);
});
test('still-connected old refs fail boundedly and are discarded',async()=>{
  const h=setup();await h.retain();assert.equal((await h.finish()).ok,false);
  assert.equal(h.frames(),64);assert.equal(h.window.__managedScheduleRetirement,undefined);
});
test('a connected submit left after dialog disconnection is not accepted',async()=>{
  const h=setup();await h.retain();h.retire();h.submit.isConnected=true;
  assert.equal((await h.finish()).ok,false);
});
test('disabled, obscured, disconnected or offscreen controls never create proof',async()=>{
  for(const mutate of [h=>h.submit.disabled=true,h=>h.cover(),h=>h.submit.isConnected=false,h=>h.dialog.isConnected=false,
    h=>h.submit.getBoundingClientRect=()=>({x:430,y:370,width:190,height:36})]) {
    const h=setup();mutate(h);assert.equal((await h.retain()).ok,false);
    assert.equal(h.window.__managedScheduleRetirement,undefined);
  }
});
test('retained reference cannot be overwritten by a second call',async()=>{
  const h=setup();await h.retain();const original=h.window.__managedScheduleRetirement;
  assert.equal((await h.retain()).ok,false);assert.equal(h.window.__managedScheduleRetirement,original);
});
test('missing generation or absent original proof never manufactures retirement',async()=>{
  const h=setup();delete h.dialog.dataset.editorGeneration;
  assert.equal((await h.retain()).ok,false);assert.equal((await h.finish()).ok,false);
});
