const assert=require('node:assert/strict');
const {scheduledForwardKeyboard}=require('../src/desktop/forward/scheduled/fixture/keyboard.js');
function setup(ordinal=0){
 const listeners=new Set(),frames=[],timers=new Map(),messages=[];let next=0;
 const node=text=>({textContent:text,isConnected:true,disabled:false,scrollIntoView(){},focus(){doc.activeElement=this;}});
 const checkbox=node(''),save=node('Schedule forward'),cancel=node('Cancel scheduling');
 const root={isConnected:true,dataset:{scheduledForwardOwner:'visit-owner',editorGeneration:'7'},querySelector:()=>checkbox,querySelectorAll:()=>[save,cancel]};
 const doc={activeElement:null,getElementById:()=>root,addEventListener:(_k,f)=>listeners.add(f),removeEventListener:(_k,f)=>listeners.delete(f)};
 const env={document:doc,performance:{now:()=>10},requestAnimationFrame:f=>frames.push(f),setTimeout:f=>{timers.set(++next,f);return next;},clearTimeout:id=>timers.delete(id)};
 const bridge={send:v=>messages.push(v)};
 const config={ordinal,owner:'visit-owner',generation:'7'};
 async function ready(){for(let i=0;i<12&&!messages.length;i++){while(frames.length)frames.shift()();await Promise.resolve();}assert.equal(messages[0]?.kind,'ready');}
 async function fire(overrides={},target=ordinal===0?save:cancel){
   const source=ordinal===0?checkbox:save;
   const event={target:source,key:'Tab',isTrusted:true,repeat:false,isComposing:false,shiftKey:false,ctrlKey:false,altKey:false,metaKey:false,defaultPrevented:false,...overrides};
   for(const f of [...listeners])f(event);doc.activeElement=target;
   while(frames.length)frames.shift()();await Promise.resolve();
 }
 return{listeners,frames,timers,messages,root,doc,env,bridge,config,ready,fire,checkbox,save,cancel};
}
(async()=>{
 let count=0;
 for(const ordinal of [0,1]){
  const f=setup(ordinal),promise=scheduledForwardKeyboard(f.config,f.bridge,f.env);await f.ready();await f.fire();await promise;
  assert.equal(f.messages[1].trusted,true);assert.equal(f.messages[1].cleaned,true);assert.equal(f.messages[1].target,ordinal===0?'Schedule forward':'Cancel scheduling');assert.equal(f.listeners.size,0);assert.equal(f.timers.size,0);count++;
 }
 for(const overrides of [{isTrusted:false},{repeat:true},{isComposing:true},{shiftKey:true},{key:'Enter'},{defaultPrevented:true}]){
  const f=setup(),promise=scheduledForwardKeyboard(f.config,f.bridge,f.env);const refusal=assert.rejects(promise,/Unexpected|did not reach/);
  await f.ready();await f.fire(overrides);await refusal;assert.equal(f.messages.length,1);assert.equal(f.listeners.size,0);assert.equal(f.timers.size,0);count++;
 }
 {
  const f=setup(),promise=scheduledForwardKeyboard(f.config,f.bridge,f.env),refusal=assert.rejects(promise,/Unexpected/);
  await f.ready();f.root.dataset.scheduledForwardOwner='new-owner';await f.fire();await refusal;assert.equal(f.listeners.size,0);count++;
 }
 {
  const f=setup(),promise=scheduledForwardKeyboard(f.config,f.bridge,f.env),refusal=assert.rejects(promise,/did not reach/);
  await f.ready();await f.fire({},f.checkbox);await refusal;assert.equal(f.listeners.size,0);count++;
 }
 {
  const f=setup(),promise=scheduledForwardKeyboard(f.config,f.bridge,f.env),refusal=assert.rejects(promise,/timeout/);
  await f.ready();for(const fire of [...f.timers.values()])fire();await refusal;assert.equal(f.listeners.size,0);assert.equal(f.timers.size,0);count++;
 }
 {
  const f=setup(),error=Error('bridge send failed');f.bridge.send=()=>{throw error;};
  const promise=scheduledForwardKeyboard(f.config,f.bridge,f.env),refusal=assert.rejects(promise,e=>e===error);
  for(let i=0;i<12;i++){while(f.frames.length)f.frames.shift()();await Promise.resolve();}
  await refusal;assert.equal(f.listeners.size,0);assert.equal(f.timers.size,0);count++;
 }
 console.log(`${count} actual keyboard helper mock groups passed; no native input`);
})().catch(e=>{console.error(e);process.exitCode=1;});
