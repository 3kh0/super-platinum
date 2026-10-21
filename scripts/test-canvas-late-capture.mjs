// Actual fixture wrapper and native producer, with deterministic in-memory IO.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import test from 'node:test';

const wrapperSource=fs.readFileSync(new URL('../src/desktop/canvas/comments/reply/fixture/late_capture.js',import.meta.url),'utf8');
const nativeSource=fs.readFileSync(new URL('../src/desktop/canvas/comments/reply/native.js',import.meta.url),'utf8');
const make=new Function(`${wrapperSource}; return canvasLateCaptureBridge;`)();
const runNative=new Function(`${nativeSource}; return canvasRichReplyBridge;`)();
const owner={scope:'original-scope',mount:'original-mount',generation:7};
const options={...owner,id:'canvas-comment-reply'};
const packet=()=>({...owner,kind:'capture',sequence:2,request:'original-request',
  blocks:[{type:'rich_text',opaque:{keep:[0,false,null,'中🦊']}}],revision:4,
  composing:false,detached:false,finalCapture:true});
const flush=async()=>{for(let i=0;i<30;i++)await Promise.resolve();};

function harness(withNative=false) {
  let now=100, nextTimer=0, receiver=null;
  const timers=new Map(), sent=[], inbox=[], events=new Map();
  const root={isConnected:true,contentEditable:'true',
    closest:selector=>({dataset:selector.includes('canvas-reply')?
      {canvasReplyScope:owner.scope}:{editorGeneration:String(owner.generation)}}),
    addEventListener:(name,fn)=>events.set(name,fn),
    removeEventListener:name=>events.delete(name),contains:()=>false};
  const holder={scope:owner.scope,generation:owner.generation,running:true};
  const env={
    performance:{now:()=>now},
    setTimeout:(fn,ms)=>{const id=++nextTimer;timers.set(id,{fn,ms});return id;},
    clearTimeout:id=>timers.delete(id),
    __superPlatinumCanvasReplyCaptures:new Map(withNative?[]:[[owner.mount,holder]]),
    document:{getElementById:()=>root,documentElement:{}},
    MutationObserver:class {observe(){} disconnect(){}},
    superPlatinumRichEditor:{snapshot:()=>({blocks:packet().blocks,revision:4,
      composing:false,generation:String(owner.generation)})}
  };
  function push(value){if(receiver){const next=receiver;receiver=null;next(value);}else inbox.push(value);}
  const bridge={send:async value=>{
    sent.push(value);
    if(withNative&&value.kind==='capture')push({kind:'ack',scope:owner.scope,mount:owner.mount,sequence:value.sequence});
  },recv:()=>inbox.length?Promise.resolve(inbox.shift()):new Promise(resolve=>{receiver=resolve;})};
  const wrapped=make(options,bridge,env);
  const api=env.__superPlatinumCanvasLateCapture;
  return {env,wrapped,api,sent,timers,root,events,push,setNow:v=>{now=v;},
    release:()=>{const s=api.state(owner);return api.release({...owner,request:s.heldPayload.request,sequence:s.heldPayload.sequence});}};
}

test('ordinary sends/receives preserve exact object without timers',async()=>{
  const h=harness(), input={kind:'capture',finalCapture:false,opaque:{x:1}};
  await h.wrapped.send(input);
  assert.equal(h.sent[0],input);
  const command={kind:'capture',scope:owner.scope,mount:owner.mount,request:'read'};
  h.push(command);assert.equal(await h.wrapped.recv(),command);
  assert.equal(h.timers.size,0);
  h.wrapped.close();assert.equal(h.api.state(owner).closed,true);
});

test('one exact final is held, released once and delivered unchanged',async()=>{
  const h=harness(), original=packet();
  h.api.arm(owner);
  const pending=h.wrapped.send(original);
  await flush();
  assert.equal(h.sent.length,0);
  const held=h.api.state(owner);
  assert.equal(held.phase,'held');assert.deepEqual(held.heldPayload,original);
  h.setNow(7100);h.release();await pending;
  assert.equal(h.sent[0],original);
  const done=h.api.state(owner);
  assert.equal(done.finalSends,1);assert.equal(done.releaseCount,1);
  assert.equal(done.releasedAt-done.heldAt,7000);
  assert.equal(done.phase,'delivered');assert.equal(h.timers.size,0);
  assert.throws(()=>h.release());
  await assert.rejects(h.wrapped.send(original),/already/);
  h.wrapped.close();assert.equal(h.api.state(owner).phase,'delivered');
});

test('foreign identity/request/sequence cannot release original',async()=>{
  const h=harness();h.api.arm(owner);const pending=h.wrapped.send(packet());
  for(const changed of [{scope:'foreign'},{mount:'foreign'},{generation:8},
    {request:'foreign'},{sequence:3},{sequence:true}])
    assert.throws(()=>h.api.release({...owner,request:'original-request',sequence:2,...changed}));
  assert.equal(h.sent.length,0);h.release();await pending;h.wrapped.close();
});

test('successor holder with identical fields has no original release authority',async()=>{
  const h=harness();h.api.arm(owner);const pending=h.wrapped.send(packet());
  const rejected=assert.rejects(pending,/closed/);
  h.env.__superPlatinumCanvasReplyCaptures.set(owner.mount,{...h.env.__superPlatinumCanvasReplyCaptures.get(owner.mount)});
  assert.throws(()=>h.release());
  h.wrapped.close();await rejected;assert.equal(h.sent.length,0);
});

test('original payload mutation is refused rather than forged from evidence',async()=>{
  const h=harness();h.api.arm(owner);const p=packet();const pending=h.wrapped.send(p);
  const rejected=assert.rejects(pending,/closed/);
  p.blocks[0].opaque.keep.push('changed');
  assert.throws(()=>h.release());h.wrapped.close();await rejected;
  assert.equal(h.sent.length,0);
});

test('deadline rejects held sender and never auto-delivers',async()=>{
  const h=harness();h.api.arm(owner);const pending=h.wrapped.send(packet());
  const rejected=assert.rejects(pending,/deadline/);
  const timer=[...h.timers.values()][0];assert.equal(timer.ms,60000);
  timer.fn();await rejected;
  assert.equal(h.api.state(owner).phase,'failed');assert.equal(h.sent.length,0);
  assert.equal(h.timers.size,0);h.wrapped.close();
  assert.match(h.api.state(owner).failure,/deadline/);
});

test('original registry evidence survives a successor mount',async()=>{
  const h=harness();h.api.arm(owner);const pending=h.wrapped.send(packet());
  h.release();await pending;h.wrapped.close();
  const original=h.api.state(owner);
  const successor=make({...options,mount:'new-mount'}, {send:async()=>{},recv:async()=>{}},h.env);
  assert.deepEqual(h.api.state(owner),original);
  assert.throws(()=>make(options,{},h.env),/Duplicate/);
  successor.close();
});

test('underlying send failure remains failure with exact once dispatch',async()=>{
  const h=harness();
  const other={...options,mount:'failure-mount'};
  h.env.__superPlatinumCanvasReplyCaptures.set(other.mount,{scope:other.scope,generation:other.generation,running:true});
  const failure=Error('original send failed');let calls=0;
  const w=make(other,{send:async()=>{calls++;throw failure;},recv:async()=>{}},h.env);
  h.api.arm(other);
  const p={...packet(),mount:other.mount};const pending=w.send(p);
  h.api.release({...other,request:p.request,sequence:p.sequence});
  await assert.rejects(pending,e=>e===failure);
  assert.equal(calls,1);assert.equal(h.api.state(other).finalSends,1);
  w.close();h.wrapped.close();
});

test('actual native producer starts no five-second ACK timer during final hold',async()=>{
  const h=harness(true);
  const native=runNative(options,h.wrapped,h.env).finally(()=>h.wrapped.close());
  await flush();
  assert.equal(h.sent.length,1);assert.equal(h.sent[0].finalCapture,false);
  h.api.arm(owner);
  h.push({kind:'finish',scope:owner.scope,mount:owner.mount,request:'real-finish'});
  await flush();
  const held=h.api.state(owner);
  assert.equal(held.phase,'held');
  assert.equal(h.root.contentEditable,'false');
  assert.equal(h.sent.length,1);
  assert.deepEqual([...h.timers.values()].map(t=>t.ms),[60000]);
  assert.deepEqual(held.finishRequests,[{kind:'finish',scope:owner.scope,mount:owner.mount,request:'real-finish'}]);
  h.setNow(7100);h.release();
  const terminal=await native;
  assert.equal(terminal.final,true);
  assert.equal(h.sent.length,2);
  assert.deepEqual(h.sent[1],held.heldPayload);
  assert.equal(h.api.state(owner).finalSends,1);
  assert.equal(h.api.state(owner).closed,true);
  assert.equal(h.env.__superPlatinumCanvasReplyCaptures.has(owner.mount),false);
  assert.equal(h.events.size,0);assert.equal(h.timers.size,0);
});
