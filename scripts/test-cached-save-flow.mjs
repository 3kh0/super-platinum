// Executes the complete installed checker with a deterministic DOM/bridge model.
// This is not a browser/native acceptance test.
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import assert from 'node:assert/strict';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const source=fs.readFileSync(path.join(root,'src/desktop/download/fixture/cached/check.js'),'utf8');
const AsyncFunction=Object.getPrototypeOf(async()=>{}).constructor;
const OLD='Cached original A\nSafe overwrite fixture.\n',NEW=OLD.replace(' A',' B');
async function flow(mode,script=source){
  let clock=0,attempt=0,refreshClicks=0,toast=false,pending=null,hit=null;
  const messages=[],acks=[];
  const rect={left:400,top:300,right:600,bottom:350,width:200,height:50};
  function node(text){return {textContent:text,disabled:false,isConnected:true,getBoundingClientRect(){hit=this;return rect;},contains:()=>false};}
  let pre=node(OLD),button=node('Download');
  const initialPre=pre,initialButton=button;
  button.click=()=>{attempt++;button.textContent='Cancel';if(attempt<3)pending=()=>{button.textContent='Download';if(attempt===2)toast=true;};};
  const refresh=node('Refresh contents');
  refresh.click=()=>{
    refreshClicks++;
    if(mode==='no-refresh')return;
    pre=node(NEW);button=node(mode==='cancel-stuck'?'Cancel':'Download');
    initialPre.isConnected=mode==='old-node-retained';initialButton.isConnected=false;
    if(mode==='toast-stuck')toast=true;
  };
  const document={
    querySelector(s){return s==='.viewer-footer button.primary'?button:s==='.text-preview-content'?pre:s==='.file-refresh'?{getAttribute:()=>mode==='no-refresh'?'false':'false'}:s==='.file-refresh [role="status"]'?{textContent:mode==='no-refresh'?'':'Contents refreshed.'}:null;},
    getElementById(id){return id==='file-preview-refresh'?refresh:id==='status-toast'&&toast?{}:null;},
    elementFromPoint:()=>hit,
  };
  let initialized=false;
  const dioxus={send(v){messages.push(v);},async recv(){
    if(!initialized){initialized=true;return {old:OLD,fresh:NEW};}
    const stage=messages.at(-1).stage;assert.ok(stage,'Only a reported stage can receive ACK');acks.push(stage);
    if(stage==='saved')toast=false;
    return stage;
  }};
  const setTimeout=(callback,ms)=>{clock+=ms;if(clock>10000)throw Error('test outer deadline');if(pending){const action=pending;pending=null;action();}queueMicrotask(callback);};
  await new AsyncFunction('document','dioxus','innerWidth','innerHeight','setTimeout','performance','downloadToastGeometry',script)(document,dioxus,1280,780,setTimeout,{now:()=>clock},async text=>({ok:true,text}));
  return {messages,acks,attempt,refreshClicks,clock};
}
const success=await flow('success');
assert.deepEqual(success.messages.map(v=>v.stage??'terminal'),['initial','canceled','saved','held','refreshed','retired','terminal']);
assert.deepEqual(success.acks,['initial','canceled','saved','held','refreshed','retired']);
assert.deepEqual(success.messages.at(-1),{ok:true});
assert.equal(success.attempt,3);assert.equal(success.refreshClicks,1);
for(const mode of ['no-refresh','cancel-stuck','old-node-retained','toast-stuck']){
  const result=await flow(mode),last=result.messages.at(-1);
  assert.equal(last.ok,false);assert.equal(result.refreshClicks,1);assert.equal(result.attempt,3);
  assert.ok(result.clock<=8080,'Failure reaches bridge within eight seconds after held');
  assert.deepEqual(result.acks,['initial','canceled','saved','held']);
  assert.equal(last.diagnostic.phase,mode==='old-node-retained'||mode==='toast-stuck'?'report-refreshed':'await-fresh-viewer');
  assert.equal(last.diagnostic.label,mode==='cancel-stuck'||mode==='no-refresh'?'Cancel':'Download');
  assert.equal(last.diagnostic.text,mode==='no-refresh'?OLD:NEW);
  assert.equal(last.diagnostic.oldPreConnected,mode==='no-refresh'||mode==='old-node-retained');
  assert.equal(last.diagnostic.toastPresent,mode==='toast-stuck');
}
// Actual previous complete checker misses its native runner's deadline.
if(process.argv[2]){
  const old=fs.readFileSync(process.argv[2],'utf8');
  const result=await flow('no-refresh',old);
  assert.equal(result.messages.at(-1).error,'test outer deadline');
  assert.equal(result.messages.at(-1).diagnostic,undefined);
}
console.log('PASS complete six-stage flow and four bounded failure diagnostics; old deadline reproduced when supplied');
