// Actual fixture helper and eval body; deterministic channel lifetime, no browser/GC claim.
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {fileURLToPath} from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const source=fs.readFileSync(path.join(root,'src/desktop/canvas_create/fixture/check.js'),'utf8');
const rust=fs.readFileSync(path.join(root,'src/desktop/canvas_create/fixture/check.rs'),'utf8');
const departure=fs.readFileSync(path.join(root,'src/desktop/canvas_create/fixture/check/departure.rs'),'utf8');
const format=JSON.parse('"'+rust.match(/let script = format!\(\s*"((?:\\.|[^"\\])*)"/)[1]+'"');
function script(operation){
  let i=0;const args=[source,JSON.stringify(operation),'{}'];
  return format.replace(/\{\{|\}\}|\{\}/g,x=>x==='{{'?'{':x==='}}'?'}':args[i++]);
}
function fixture(){
  const f={clicks:0,focuses:0,scrolls:0,mounted:false,missing:false,covered:false,coverOnFocus:false,throwClick:false};
  const button={textContent:'New canvas',disabled:false,
    getBoundingClientRect:()=>({left:10,top:10,right:110,bottom:42,width:100,height:32}),
    contains:e=>e===button,scrollIntoView(){f.scrolls++;},
    focus(){f.focuses++;if(f.coverOnFocus)f.covered=true;},
    click(){f.clicks++;f.mounted=true;if(f.throwClick)throw new Error('after dispatch');}};
  const document={querySelector:s=>s==='[data-keyboard-dialog]'&&f.mounted?{}:null,
    getElementById:()=>null,querySelectorAll:s=>s==='.files-header button'&&!f.missing?[button]:[],
    elementFromPoint:()=>f.covered?{}:button};
  const context=vm.createContext({window:{},document,innerWidth:1280,innerHeight:780});
  vm.runInContext(source,context);
  f.button=button;f.call=op=>context.window.canvasCreationCheck(op);
  f.evaluate=(dropReceipt=false)=>{
    let acknowledge;const receipts=[];let closed=false;
    context.dioxus={send:r=>{if(!dropReceipt)receipts.push(r);},recv:()=>new Promise(resolve=>{acknowledge=resolve;})};
    const completion=vm.runInContext('(async()=>{'+script('reopen')+'})()',context).then(()=>{closed=true;});
    return {receipts,completion,release:()=>acknowledge(null),closed:()=>closed};
  };
  return f;
}
let count=0;
const test=async fn=>{await fn();count++;};
await test(()=>{const f=fixture();for(let i=0;i<150;i++)assert.equal(f.call('reopen-ready').ok,true);assert.deepEqual([f.clicks,f.focuses,f.scrolls],[0,0,0]);});
for(const fault of ['mounted','missing','covered'])await test(()=>{const f=fixture();f[fault]=true;assert.equal(f.call('reopen-ready').ok,false);assert.equal(f.call('reopen').ok,false);assert.equal(f.clicks,0);});
await test(()=>{const f=fixture();f.button.disabled=true;assert.equal(f.call('reopen-ready').ok,false);assert.equal(f.call('reopen').ok,false);assert.equal(f.clicks,0);});
await test(()=>{const f=fixture();assert.equal(f.call('reopen-ready').ok,true);f.mounted=true;assert.equal(f.call('reopen').ok,false);assert.equal(f.clicks,0);});
await test(()=>{const f=fixture();assert.equal(f.call('reopen-ready').ok,true);f.coverOnFocus=true;assert.equal(f.call('reopen').ok,false);assert.equal(f.clicks,0);});
await test(async()=>{const f=fixture(),e=f.evaluate();await Promise.resolve();assert.equal(e.receipts[0].ok,true);assert.equal(e.closed(),false);assert.equal(f.clicks,1);e.release();await e.completion;assert.equal(e.closed(),true);assert.equal(f.call('reopen').ok,false);assert.equal(f.clicks,1);});
await test(async()=>{const f=fixture(),e=f.evaluate(true);await Promise.resolve();assert.equal(e.receipts.length,0);assert.equal(e.closed(),false);assert.equal(f.clicks,1);e.release();await e.completion;assert.equal(f.call('reopen').ok,false);assert.equal(f.clicks,1);});
await test(async()=>{const f=fixture();f.throwClick=true;const e=f.evaluate();assert.equal(e.receipts[0].ok,false);assert.match(e.receipts[0].reason,/after dispatch/);assert.equal(f.clicks,1);assert.equal(e.closed(),false);e.release();await e.completion;assert.equal(f.clicks,1);});
await test(()=>{
  // Bind the actual orchestration, including its timeout/error exit: no action retry.
  assert.match(departure,/wait_dom\("reopen-ready", json!\(\{\}\)\)\.await\?;\s*dom\("reopen", json!\(\{\}\)\)\.await\?;\s*ready\(state\)\.await\?;/);
  assert.doesNotMatch(departure,/wait_dom\("reopen"/);
  const poll=rust.slice(rust.indexOf('async fn wait_dom'),rust.indexOf('async fn wait_model'));
  assert.match(poll,/"read" \| "palette" \| "disclosure" \| "reopen-ready"/);
  assert.match(poll,/return Err\(format!\("Native action must not be polled/);
  const receive=rust.slice(rust.indexOf('let received ='),rust.indexOf('async fn wait_dom'));
  assert.ok(receive.indexOf('eval.send(())')<receive.indexOf('let value = received'));
  assert.match(receive,/Native receipt timed out/);
  assert.match(receive,/Native receipt missing/);
  assert.match(receive,/Native receipt acknowledgment failed/);
});
console.log(`${count} actual helper/eval/orchestration groups PASS; no browser, GC or native execution.`);
