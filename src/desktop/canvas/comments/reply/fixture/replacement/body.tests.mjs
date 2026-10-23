import assert from 'node:assert/strict';
import fs from 'node:fs';
import test from 'node:test';
const make=new Function(fs.readFileSync(new URL('./body.js',import.meta.url),'utf8')+';return canvasReturnBodyBridge')();
const run=new Function(fs.readFileSync(new URL('../../native.js',import.meta.url),'utf8')+';return canvasRichReplyBridge')();
const owner={scope:'returned-scope',mount:'returned-mount',generation:4},options={...owner,id:'canvas-comment-reply'};
const flush=async()=>{for(let i=0;i<40;i++)await Promise.resolve();};
test('actual native finish capture and genuine ACK are observed unchanged',async()=>{
 let waiting=null;const queue=[],sent=[],events=new Map();
 const push=v=>{if(waiting){const w=waiting;waiting=null;w(v);}else queue.push(v);};
 const root={isConnected:true,contentEditable:'true',contains:()=>false,
  closest:s=>({dataset:s.includes('canvas-reply')?{canvasReplyScope:owner.scope}:{editorGeneration:'4'}}),
  addEventListener:(k,v)=>events.set(k,v),removeEventListener:k=>events.delete(k)};
 const blocks=[{type:'rich_text',opaque:{keep:[false,0,null]},elements:[]}];
 const env={setTimeout,clearTimeout,document:{getElementById:()=>root,documentElement:{}},MutationObserver:class{observe(){}disconnect(){}},
  superPlatinumRichEditor:{snapshot:()=>({blocks,revision:8,composing:false,generation:'4'})}};
 const bridge={send:async v=>{sent.push(v);push({kind:'ack',scope:owner.scope,mount:owner.mount,sequence:v.sequence});},
  recv:()=>queue.length?Promise.resolve(queue.shift()):new Promise(r=>waiting=r)};
 const observed=make(options,bridge,env),task=run(options,observed,env).finally(()=>observed.close());
 await flush();const finish={kind:'finish',scope:owner.scope,mount:owner.mount,request:'actual-finish'};push(finish);
 const terminal=await task,receipt=env.__canvasReturnBodyCaptures.get(owner.mount).state();
 assert.equal(terminal.final,true);assert.deepEqual(receipt.commands,[finish]);assert.equal(receipt.captures.length,1);
 assert.deepEqual(receipt.captures[0],sent[1]);assert.equal(receipt.captures[0].request,finish.request);
 assert.equal(receipt.captures[0].finalCapture,true);assert.equal(receipt.captures[0].detached,false);
 assert.deepEqual(receipt.captures[0].blocks,blocks);assert.equal(receipt.acks.length,1);
 assert.equal(receipt.acks[0].sequence,receipt.captures[0].sequence);assert.equal(receipt.closed,true);
 assert.equal(root.contentEditable,'false');assert.equal(events.size,0);
});
test('ordinary traffic and receive object identity are unchanged',async()=>{
 const env={},sent=[],command={kind:'capture',scope:'s',mount:'m',request:'r'};
 const observed=make(options,{send:async v=>sent.push(v),recv:async()=>command},env);
 const value={kind:'capture',finalCapture:false,opaque:[false,0,null]};await observed.send(value);
 assert.equal(sent[0],value);assert.equal(await observed.recv(),command);
 assert.deepEqual(env.__canvasReturnBodyCaptures.get(owner.mount).state().captures,[]);observed.close();
});
test('original evidence survives later distinct mount and rejects duplicate owner',()=>{
 const env={},bridge={send:async()=>{},recv:async()=>{}};make(options,bridge,env).close();
 const first=env.__canvasReturnBodyCaptures.get(owner.mount).state();
 make({...options,mount:'fresh-reply'},bridge,env).close();
 assert.deepEqual(env.__canvasReturnBodyCaptures.get(owner.mount).state(),first);
 assert.throws(()=>make(options,bridge,env),/registry/);
});
