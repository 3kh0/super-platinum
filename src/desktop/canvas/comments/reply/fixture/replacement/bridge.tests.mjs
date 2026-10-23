import assert from 'node:assert/strict';
import fs from 'node:fs';
import test from 'node:test';
const wrapper=fs.readFileSync(new URL('./bridge.js',import.meta.url),'utf8');
const producer=fs.readFileSync(new URL('../../native.js',import.meta.url),'utf8');
const make=new Function(`${wrapper};return canvasReplacementBridge`)();
const native=new Function(`${producer};return canvasRichReplyBridge`)();
const owner={scope:'original-scope',mount:'original-mount',generation:7};
const options={...owner,id:'canvas-comment-reply'};
const flush=async()=>{for(let i=0;i<40;i++)await Promise.resolve();};
const blocks=text=>[{type:'rich_text',opaque:{keep:[0,false,null]},elements:[{type:'rich_text_section',elements:[{type:'text',text}]}]}];
function harness(withNative=false){
 let receiver=null,revision=0,content=blocks('original'),observer=null;
 const sent=[],events=new Map(),timers=new Map(),inbox=[];
 const root={isConnected:true,contentEditable:'true',contains:()=>false,
  closest:s=>({dataset:s.includes('canvas-reply')?{canvasReplyScope:owner.scope}:{editorGeneration:String(owner.generation)}}),
  addEventListener:(k,f)=>events.set(k,f),removeEventListener:k=>events.delete(k)};
 const holder={root,scope:owner.scope,generation:owner.generation,running:true};
 const env={setTimeout:(fn,ms)=>{const id={};timers.set(id,{fn,ms});return id;},clearTimeout:id=>timers.delete(id),
  __superPlatinumCanvasReplyCaptures:new Map(withNative?[]:[[owner.mount,holder]]),
  document:{getElementById:()=>root.isConnected?root:null,documentElement:{}},
  MutationObserver:class{constructor(fn){observer=fn;}observe(){}disconnect(){}},
  superPlatinumRichEditor:{snapshot:()=>({blocks:content,revision,composing:false,generation:String(owner.generation)})}};
 const push=v=>{if(receiver){const r=receiver;receiver=null;r(v);}else inbox.push(v);};
 const wrapped=make(options,{send:async value=>{sent.push(value);if(withNative)push({kind:'ack',scope:owner.scope,mount:owner.mount,sequence:value.sequence});},
  recv:()=>inbox.length?Promise.resolve(inbox.shift()):new Promise(r=>receiver=r)},env);
 const api=env.__canvasReplacementCaptures.get(owner.mount);
 return {env,wrapped,api,sent,events,timers,root,
  edit(){content=blocks('Edited original \u4e2d\ud83e\udd8a');revision++;events.get('input')?.();},
  detach(){root.isConnected=false;observer?.([]);},
  release(){const s=api.state();return api.release({...owner,sequence:s.held.at(-1).sequence});}};
}
const packet=(detached=false)=>({...owner,kind:'capture',sequence:detached?3:2,request:null,
 blocks:blocks('tail'),revision:1,composing:false,detached,finalCapture:false});
test('real producer emits edited and detached objects, both delayed without alteration',async()=>{
 const h=harness(true),running=native(options,h.wrapped,h.env).finally(()=>h.wrapped.close());
 await flush();assert.equal(h.sent.length,1);h.api.arm();h.edit();await flush();
 assert.equal(h.api.state().phase,'edited-held');assert.equal(h.sent.length,1);
 const edited=h.api.state().held[0];assert.equal(edited.detached,false);assert.equal(edited.request,null);
 h.detach();await flush();assert.equal(h.sent.length,1);h.release();await flush();
 assert.equal(h.api.state().phase,'detached-held');const detached=h.api.state().held[1];
 assert.equal(detached.detached,true);assert.equal(detached.finalCapture,false);
 assert.deepEqual(detached.blocks,edited.blocks);assert.ok(detached.sequence>edited.sequence);
 assert.deepEqual(h.sent[1],edited);h.release();const result=await running;
 assert.equal(result.final,true);assert.deepEqual(h.sent[2],detached);
 assert.deepEqual(h.api.state().delivered,h.api.state().held);assert.equal(h.api.state().closed,true);
 assert.equal(h.events.size,0);assert.equal(h.timers.size,0);
});
test('ordinary unarmed traffic retains object identity',async()=>{
 const h=harness(),p=packet();await h.wrapped.send(p);assert.equal(h.sent[0],p);assert.equal(h.timers.size,0);h.wrapped.close();
});
test('wrong owner and sequence cannot release held capture',async()=>{
 const h=harness();h.api.arm();const p=packet(),pending=h.wrapped.send(p);
 for(const v of [{mount:'foreign'},{scope:'foreign'},{generation:8},{sequence:99}])
  assert.throws(()=>h.api.release({...owner,sequence:p.sequence,...v}));
 assert.equal(h.sent.length,0);h.release();await pending;assert.equal(h.sent[0],p);h.wrapped.close();
});
test('same-field replacement holder cannot release original',async()=>{
 const h=harness();h.api.arm();const pending=h.wrapped.send(packet());const rejected=assert.rejects(pending,/closed/);
 h.env.__superPlatinumCanvasReplyCaptures.set(owner.mount,{...h.env.__superPlatinumCanvasReplyCaptures.get(owner.mount)});
 assert.throws(()=>h.release());h.wrapped.close();await rejected;assert.equal(h.sent.length,0);
});
test('mutated original object is refused, never rebuilt from receipt',async()=>{
 const h=harness();h.api.arm();const p=packet(),pending=h.wrapped.send(p);const rejected=assert.rejects(pending,/closed/);
 p.blocks[0].opaque.keep.push('changed');assert.throws(()=>h.release());h.wrapped.close();await rejected;assert.equal(h.sent.length,0);
});
test('deadline rejects without dispatch or replay',async()=>{
 const h=harness();h.api.arm();const pending=h.wrapped.send(packet());const rejected=assert.rejects(pending,/deadline/);
 const timer=[...h.timers.values()][0];assert.equal(timer.ms,60000);timer.fn();await rejected;
 assert.equal(h.sent.length,0);assert.equal(h.timers.size,0);assert.equal(h.api.state().phase,'failed');h.wrapped.close();
});
test('second held object must come from actual detached original',async()=>{
 const h=harness();h.api.arm();const pending=h.wrapped.send(packet());h.release();await pending;
 await assert.rejects(h.wrapped.send(packet(true)),/Unexpected/);assert.equal(h.sent.length,1);h.wrapped.close();
});
test('returned mount preserves original completed evidence',async()=>{
 const h=harness();h.api.arm();let p=h.wrapped.send(packet());h.release();await p;h.detach();
 p=h.wrapped.send(packet(true));h.release();await p;h.wrapped.close();const before=h.api.state();
 make({...options,mount:'returned-mount'},{send:async()=>{},recv:async()=>{}},h.env).close();
 assert.deepEqual(h.api.state(),before);assert.equal(h.env.__canvasReplacementCaptures.size,2);
});
