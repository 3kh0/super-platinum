// Installed endpoint + finite broker; nodes, importer and IPC are modeled.
// No browser, clipboard IO, semantic selector or network acceptance claim.
const fs=require('node:fs'),vm=require('node:vm'),assert=require('node:assert/strict');
const clone=x=>JSON.parse(JSON.stringify(x));
const build=vm.runInNewContext(fs.readFileSync(__dirname+'/dispatch.js','utf8')+';listCommentDeltaClipboardBridge');
const paste=vm.runInNewContext(fs.readFileSync(__dirname+'/paste.js','utf8')+';listCommentDeltaPaste');
function fixture(){
 const listeners=new Map(),root={addEventListener:(k,f)=>listeners.set(k,f),removeEventListener:(k,f)=>{if(listeners.get(k)===f)listeners.delete(k);}};
 const state={root,owned:true,connected:true,disabled:false,frozen:false,composing:false,repairPending:false,
  scope:'original',generation:'4',revision:7,selectionRevision:2,selection:{index:1,length:0},ops:[{insert:'original\n'}]};
 let intent=null,serial=0,clock=0,notified=0;
 const sent=[],inserted=[],retired=[],timers=new Map();
 const model={clone,document:clone,plainPaste:ops=>({ops:clone(ops)})};
 const broker=paste({model,current:()=>state,insert:x=>{inserted.push(clone({...x,root:undefined}));return true;},
  onRequest:x=>{intent=clone(x);listeners.get('comment-delta-paste-request')?.({detail:x.request});},
  onRetired:x=>{retired.push(clone(x));intent=null;},setTimeout:fn=>{timers.set(++clock,fn);return clock;},clearTimeout:id=>timers.delete(id),makeId:()=> 'paste-'+(++serial)});
 const api={clipboardRequest:r=>{assert.equal(r,root);broker.changed();return intent&&clone(intent);},
  resolvePaste:(r,x)=>{assert.equal(r,root);return broker.resolve(x);},rejectPaste:(r,g,id)=>{assert.equal(r,root);assert.equal(g,'4');return broker.reject(id);}};
 const endpoint=build({root,api,options:{scope:'original',mount:'mount1',generation:4},alive:()=>state.connected&&state.owned,
  send:async x=>sent.push(clone(x)),notify:()=>notified++});
 const ops=[{insert:'@Ada',attributes:{slackmention:{id:'U1',label:'@Ada'}}}];
 const begin=()=>broker.begin(ops);
 const ack=(sequence=8)=>({sequence,value:{generation:4,revision:state.revision,pending:broker.pending()}});
 const response=(wire=sent.findLast(x=>x.kind==='clipboard-request'))=>({kind:'clipboard-receipt',scope:'original',mount:'mount1',sequence:wire.sequence,
  receipt:{...clone(wire.clipboard),validity:[['U1',true]],emoji:[]}});
 return {root,state,api,broker,endpoint,sent,inserted,retired,timers,listeners,begin,ack,response,notified:()=>notified,
  expire(){for(const[id,fn]of [...timers]){timers.delete(id);fn();}}};
}
let groups=0;async function check(name,fn){await fn();groups++;}
(async()=>{
 await check('only original acknowledged pending capture dispatches once',async()=>{
  const h=fixture();h.begin();assert.equal(h.sent.length,0);assert(h.notified()>0);
  await h.endpoint.afterAck({sequence:8,value:{generation:4,revision:6,pending:true}});assert.equal(h.sent.length,0);
  await h.endpoint.afterAck(h.ack());await h.endpoint.afterAck(h.ack());assert.equal(h.sent.length,1);
  assert.equal(h.sent[0].kind,'clipboard-request');assert.equal(h.inserted.length,0);assert(h.broker.pending());
  assert(await h.endpoint.receive(h.response()));assert.equal(h.inserted.length,1);assert(!h.broker.pending());
  assert.deepEqual(h.sent.at(-1),{kind:'clipboard-result',request:'paste-1',applied:true});
 });
 await check('positive refusal preserves draft and reports unapplied',async()=>{
  const h=fixture(),before=clone(h.state.ops);h.begin();await h.endpoint.afterAck(h.ack());
  await h.endpoint.receive({kind:'clipboard-rejected',scope:'original',mount:'mount1',sequence:8,request:'paste-1'});
  assert.deepEqual(h.state.ops,before);assert.equal(h.inserted.length,0);assert(!h.broker.pending());
  assert.equal(h.retired.at(-1).reason,'host-unavailable');assert.equal(h.sent.at(-1).applied,false);
 });
 await check('foreign owner sequence and id cannot consume current paste',async()=>{
  for(const field of ['scope','mount','sequence']){
   const h=fixture();h.begin();await h.endpoint.afterAck(h.ack());const bad=h.response();bad[field]='foreign';
   await h.endpoint.receive(bad);assert(h.broker.pending());assert.equal(h.inserted.length,0);
   await h.endpoint.receive(h.response());assert.equal(h.inserted.length,1);
  }
 });
 await check('selection ABA cancels old dispatch; late result cannot consume successor',async()=>{
  const h=fixture();h.begin();await h.endpoint.afterAck(h.ack());const old=h.response();
  h.state.selectionRevision+=2;h.broker.changed();h.begin();await h.endpoint.afterAck(h.ack(9));
  assert.equal(h.sent.filter(x=>x.kind==='clipboard-cancel').length,1);
  await h.endpoint.receive(old);assert(h.broker.pending());assert.equal(h.inserted.length,0);
  await h.endpoint.receive(h.response());assert.equal(h.inserted.length,1);
 });
 await check('retired exact root cannot adopt receipt',async()=>{
  const h=fixture();h.begin();await h.endpoint.afterAck(h.ack());h.state.connected=false;h.broker.changed();
  await h.endpoint.afterAck(h.ack());await h.endpoint.receive(h.response());assert.equal(h.inserted.length,0);
  assert.equal(h.sent.find(x=>x.kind==='clipboard-cancel').request,'paste-1');
 });
 await check('deadline retirement and explicit release leave no active attempt',async()=>{
  const h=fixture();h.begin();await h.endpoint.afterAck(h.ack());h.expire();await h.endpoint.afterAck(h.ack());
  assert(!h.broker.pending());assert.equal(h.inserted.length,0);assert.equal(h.sent.at(-1).kind,'clipboard-cancel');
  h.begin();await h.endpoint.afterAck(h.ack(9));h.endpoint.release();h.endpoint.release();
  assert(!h.broker.pending());assert.equal(h.listeners.size,0);assert.equal(h.timers.size,0);
 });
 await check('unavailable semantic rows cannot become insertion or false success',async()=>{
  const h=fixture();h.begin();await h.endpoint.afterAck(h.ack());const bad=h.response();bad.receipt.validity=[];
  await h.endpoint.receive(bad);assert.equal(h.inserted.length,0);assert(!h.broker.pending());assert.equal(h.sent.at(-1).applied,false);
 });
 await check('composition and typing retirement preserve original raw content',async()=>{
  for(const mutate of [s=>s.composing=true,s=>{s.revision++;s.ops=[{insert:'new input\n'}]}]){
   const h=fixture();h.begin();await h.endpoint.afterAck(h.ack());const old=h.response();mutate(h.state);const before=clone(h.state.ops);
   h.broker.changed();await h.endpoint.afterAck(h.ack(9));await h.endpoint.receive(old);
   assert.deepEqual(h.state.ops,before);assert.equal(h.inserted.length,0);
  }
 });
 console.log(JSON.stringify({ok:true,groups}));
})().catch(error=>{console.error(error);process.exitCode=1});
