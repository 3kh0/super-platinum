import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const source=fs.readFileSync(new URL('../src/desktop/view/slack_lists/comments/bridge.js',import.meta.url),'utf8');
const clipboardSource=fs.readFileSync(new URL('../src/desktop/slack_lists/comments/delta_editor/dispatch.js',import.meta.url),'utf8');
const run=vm.runInNewContext(`${clipboardSource}\n${source}\nlistCommentsComposeBridge`);
const plain=v=>JSON.parse(JSON.stringify(v));
const drain=async()=>{for(let i=0;i<30;i++)await Promise.resolve();};
function fixture({ack=true,shared=null,recover=false,representation='rich-blocks'}={}) {
  const events=new Map(),timers=new Map(),sent=[],incoming=[];let receive=null,observer=null,clock=0,commitReads=0;
  const owner=shared?.owner??{dataset:{listCommentsComposeScope:'entry:visit:4',editorGeneration:'4'}};
  const wrapper=shared?.wrapper??{inert:false,link:{hidden:true},querySelector(s){return s==='.rich-link-controls'?this.link:null;}};
  const root=shared?.root??{isConnected:true,contentEditable:'true',contains:n=>n===root,
    closest:s=>s==='.rich-editor'?wrapper:s==='[data-list-comments-compose-scope]'?owner:null,
    addEventListener(n,fn){events.set(n,fn);},removeEventListener(n,fn){if(events.get(n)===fn)events.delete(n);}};
  const state=shared?.state??{blocks:[{type:'rich_text',elements:[{type:'rich_text_section',elements:[{type:'text',text:'original ???',style:{bold:true}},{type:'file',file_id:'F1',opaque:{retain:1}}]}]}],revision:0,composing:false,generation:'4'};
  const nodes=shared?.nodes??new Map([['slack-list-comment-draft',root]]);
  const env=shared?.env??{};
  Object.assign(env,{document:{getElementById:id=>nodes.get(id),documentElement:{}},
    MutationObserver:class{constructor(fn){observer=fn;}observe(){}disconnect(){observer=null;}},
    setTimeout(fn,ms){const key=++clock;timers.set(key,{fn,ms});return key;},clearTimeout:id=>timers.delete(id),
    superPlatinumRichEditor:{snapshot(node){assert.equal(node,root);return {...state,blocks:plain(state.blocks)};},
      readForCommit(id){commitReads++;assert.equal(nodes.get(id),root);if(state.composing||!wrapper.link.hidden)throw Error('unfinished');return plain(state.blocks);}}});
  const command=value=>{const v={scope:'entry:visit:4',mount:'mount-original',...value};if(receive){const r=receive;receive=null;r(v);}else incoming.push(v);};
  const bridge={send:async value=>{sent.push(plain(value));if(ack&&value.kind==='capture')command({kind:'ack',sequence:value.sequence});},recv:()=>incoming.length?Promise.resolve(incoming.shift()):new Promise(resolve=>{receive=resolve;})};
  const options={id:'slack-list-comment-draft',scope:'entry:visit:4',generation:4,mount:'mount-original',recover};
  if(representation==='plain-delta') {
    options.representation=representation;
    Object.assign(state,{ops:[{insert:'Original '},{insert:{mention:{id:'U1',text:'@Fox'}}},{insert:'\n'}],mode:'PlainText',pending:false});
    const tickets=new WeakSet(),used=new WeakSet();let sealed=null;
    state.released=false;
    env.superPlatinumCommentDelta={
      snapshot(node){assert.equal(node,root);if(state.released)return null;const ticket={ops:plain(state.ops),mode:state.mode,attachmentEffects:'none-v1',pending:state.pending,revision:state.revision,composing:state.composing,generation:state.generation};tickets.add(ticket);return ticket;},
      seal(node,ticket){assert.equal(node,root);if(sealed||used.has(ticket)||!tickets.has(ticket)||state.composing||state.pending)return false;sealed=ticket;return true;},
      validate(node,ticket){assert.equal(node,root);return sealed===ticket&&state.revision===ticket.revision&&!state.composing&&!state.pending&&JSON.stringify(state.ops)===JSON.stringify(ticket.ops);},
      unseal(node,ticket){assert.equal(node,root);if(sealed!==ticket)return false;used.add(ticket);sealed=null;return true;},
      release(node,generation){assert.equal(node,root);assert.equal(generation,'4');state.released=true;return true;},
      clipboardRequest(node){assert.equal(node,root);return state.paste??null;},
      rejectPaste(node,generation,request){assert.equal(node,root);assert.equal(generation,'4');if(state.paste?.request!==request)return false;state.paste=null;state.pending=false;return true;},
      resolvePaste(node,receipt){assert.equal(node,root);if(state.paste?.request!==receipt.request)return false;state.paste=null;state.pending=false;state.ops.push({insert:'pasted'});state.revision++;return true;},
      sealed:()=>sealed,
    };
  }
  const done=run(options,bridge,env);done.catch(()=>{});
  async function packet(kind,request){await drain();const value=sent.findLast(x=>x.kind===kind&&(request===undefined||x.request===request));assert.ok(value,`Missing ${kind}/${request}: ${JSON.stringify(sent)}`);return value;}
  async function seal(request='claim'){command({kind:'capture',request});return packet('capture',request);}
  async function close(){root.isConnected=false;observer?.([{target:owner}]);await drain();return done;}
  return {root,owner,wrapper,state,nodes,env,sent,options,done,command,packet,seal,close,timers,
    shared:{root,owner,wrapper,state,nodes,env},reads:()=>commitReads,
    event:(n,detail)=>events.get(n)?.({detail}),mutation:(target=root)=>observer?.([{target}]),
    expire:ms=>{for(const [key,t]of [...timers])if(t.ms===ms){timers.delete(key);t.fn();}}};
}

test('plain Delta has explicit representation and exact semantic ops through seal, validation and final ACK',async()=>{
  const f=fixture({representation:'plain-delta'});const initial=await f.packet('capture');
  assert.equal(initial.attachmentEffects,'none-v1');assert.equal(initial.blocks,undefined);assert.deepEqual(initial.delta,{mode:'plain-text',ops:plain(f.state.ops)});
  const sealed=await f.seal();assert.ok(f.env.superPlatinumCommentDelta.sealed());assert.equal(f.reads(),0);
  f.command({kind:'validate',request:'valid',lease:'claim',sequence:sealed.sequence});
  assert.equal((await f.packet('release-result','valid')).ok,true);
  f.command({kind:'finish',request:'finish',lease:'claim',sequence:sealed.sequence});
  await f.done;assert.equal(f.state.released,true);assert.equal(f.wrapper.inert,true);
  assert.deepEqual(f.sent.find(x=>x.finalCapture).delta,sealed.delta);assert.equal(sealed.attachmentEffects,'none-v1');assert.equal(f.sent.find(x=>x.finalCapture).attachmentEffects,'none-v1');
});
for(const field of ['composing','pending'])test(`plain Delta ${field} prevents commit while continuous custody remains`,async()=>{
  const f=fixture({representation:'plain-delta'});await f.packet('capture');f.state[field]=true;
  f.command({kind:'capture',request:'refused'});await f.packet('capture-refused','refused');
  assert.equal(f.env.superPlatinumCommentDelta.sealed(),null);assert.equal(f.wrapper.inert,false);
  await f.close();assert.equal(f.sent.at(-1)[field],true);assert.equal(f.state.released,true);
});
test('plain Delta seal failure leaves no lease or successful commit receipt',async()=>{
  const f=fixture({representation:'plain-delta'});await f.packet('capture');f.env.superPlatinumCommentDelta.seal=()=>false;
  f.command({kind:'capture',request:'refused'});await f.packet('capture-refused','refused');
  assert.equal(f.wrapper.inert,false);await f.close();
});
test('plain Delta changed under seal refuses validation and releases same ticket for retry',async()=>{
  const f=fixture({representation:'plain-delta'});const sealed=await f.seal();f.state.ops[0].insert='Changed';f.state.revision++;
  f.command({kind:'validate',request:'invalid',lease:'claim',sequence:sealed.sequence});assert.equal((await f.packet('release-result','invalid')).ok,false);
  assert.ok(f.env.superPlatinumCommentDelta.sealed());
  f.command({kind:'release',request:'retry',lease:'claim',sequence:sealed.sequence});await f.packet('release-result','retry');
  assert.equal(f.env.superPlatinumCommentDelta.sealed(),null);assert.equal(f.wrapper.inert,false);await f.close();
});
test('plain Delta lost ACK unlocks but retains original model and capture registry',async()=>{
  const f=fixture({representation:'plain-delta',ack:false});const first=await f.packet('capture');f.command({kind:'ack',sequence:first.sequence});
  await f.seal();f.expire(5000);await assert.rejects(f.done,/acknowledgment timed out/);
  assert.equal(f.env.superPlatinumCommentDelta.sealed(),null);assert.equal(f.state.released,false);assert.equal(f.env.__listCommentsComposeCaptures.size,1);
});
test('plain Delta replacement receives neither predecessor bytes nor cleanup',async()=>{
  const f=fixture({representation:'plain-delta'});await f.seal();const replacement={inert:true};f.nodes.set(f.options.id,replacement);
  f.root.isConnected=false;f.state.ops.push({insert:'old tail'});f.mutation(f.owner);await f.done;
  assert.equal(replacement.inert,true);assert.equal(f.sent.at(-1).delta.ops.at(-1).insert,'old tail');assert.equal(f.state.released,true);
});
test('plain Delta unchanged retry uses a fresh opaque ticket after the old one is consumed',async()=>{
  const f=fixture({representation:'plain-delta'});const first=await f.seal();const ticket=f.env.superPlatinumCommentDelta.sealed();
  f.command({kind:'release',request:'release',lease:'claim',sequence:first.sequence});await f.packet('release-result','release');
  const second=await f.seal('retry');assert.notEqual(f.env.superPlatinumCommentDelta.sealed(),ticket);assert.deepEqual(second.delta,first.delta);
  await f.close();
});

for(const representation of ['rich-blocks','plain-delta'])test(`${representation} host abort releases unacknowledged original lock and retains recovery`,async()=>{
  const f=fixture({representation,ack:false});const first=await f.packet('capture');
  f.command({kind:'ack',sequence:first.sequence});await f.seal();
  assert.equal(f.wrapper.inert,true);
  f.command({kind:'capture-abort'});
  await assert.rejects(f.done,/capture rejected by host/);
  assert.equal(f.wrapper.inert,false);assert.equal(f.env.__listCommentsComposeCaptures.size,1);
  if(representation==='plain-delta'){
    assert.equal(f.env.superPlatinumCommentDelta.sealed(),null);
    assert.equal(f.state.released,false);
  }
});

test('initial and native-ahead snapshots preserve rich opaque bytes; capture seals before host mutation',async()=>{
  const f=fixture();await f.packet('capture');f.state.blocks[0].elements[0].elements[0].text='native ahead';
  const capture=await f.seal();assert.equal(capture.blocks[0].elements[0].elements[0].text,'native ahead');
  assert.equal(capture.blocks[0].elements[0].elements[1].opaque.retain,1);assert.equal(capture.locked,true);assert.equal(f.wrapper.inert,true);
  assert.equal(f.reads(),1);assert.equal(f.root.contentEditable,'true');
  f.command({kind:'release',request:'release',lease:'claim',sequence:capture.sequence});assert.equal((await f.packet('release-result','release')).ok,true);
  assert.equal(f.wrapper.inert,false);await f.close();
});
test('finish ACKs exact original final content before completing departure',async()=>{
  const f=fixture();const capture=await f.seal();f.command({kind:'finish',request:'depart',lease:'claim',sequence:capture.sequence});
  const result=await f.packet('release-result','depart');assert.equal(result.finished,true);const done=await f.done;
  const final=f.sent.find(x=>x.finalCapture);assert.ok(final);assert.equal(done.sequence,final.sequence);assert.equal(f.wrapper.inert,true);
  assert.equal(f.env.__listCommentsComposeCaptures.size,0);
});
for(const reason of ['composition','open-link','readonly','foreign-inert'])test(`${reason} refuses capture without changing native bytes`,async()=>{
  const f=fixture();await f.packet('capture');if(reason==='composition')f.state.composing=true;if(reason==='open-link')f.wrapper.link.hidden=false;
  if(reason==='readonly')f.root.contentEditable='false';if(reason==='foreign-inert')f.wrapper.inert=true;
  const before=plain(f.state);f.command({kind:'capture',request:'refuse'});await f.packet('capture-refused','refuse');
  assert.deepEqual(plain(f.state),before);assert.equal(f.wrapper.inert,reason==='foreign-inert');await f.close();
});
test('unacknowledged snapshots coalesce input and retain newest detached tail',async()=>{
  const f=fixture({ack:false});const first=await f.packet('capture');f.state.blocks.push({type:'text',text:'tail'});f.state.revision++;f.event('input');
  await drain();assert.equal(f.sent.length,1);f.root.isConnected=false;f.mutation(f.owner);
  f.command({kind:'ack',sequence:first.sequence});const tail=await f.packet('capture');assert.equal(tail.detached,true);assert.equal(tail.blocks.at(-1).text,'tail');
  f.command({kind:'ack',sequence:tail.sequence});await f.done;
});
test('same-ID replacement never supplies final bytes or inherits old unlock',async()=>{
  const f=fixture();const capture=await f.seal();const replacement={inert:true};f.root.isConnected=false;f.nodes.set(f.options.id,replacement);f.state.blocks.push({text:'old tail'});f.mutation(f.owner);
  await f.done;assert.equal(replacement.inert,true);assert.equal(f.sent.at(-1).detached,true);assert.equal(f.sent.at(-1).blocks.at(-1).text,'old tail');
  assert.equal(f.reads(),1);assert.equal(f.wrapper.inert,false);assert.ok(capture.locked);
});
for(const field of ['listCommentsComposeScope','editorGeneration'])test(`changed ${field} retires original capture without commit authority`,async()=>{
  const f=fixture();await f.packet('capture');f.owner.dataset[field]='foreign';f.mutation(f.owner);await f.done;assert.equal(f.sent.at(-1).detached,true);assert.equal(f.reads(),0);
});
test('formatting mutation without input is captured',async()=>{
  const f=fixture();await f.packet('capture');f.state.blocks[0].elements[0].elements[0].style.italic=true;f.state.revision++;f.mutation();
  const captured=await f.packet('capture');assert.equal(captured.blocks[0].elements[0].elements[0].style.italic,true);assert.equal(captured.revision,1);await f.close();
});
test('programmatic buffer drift under seal refuses departure and retains new bytes',async()=>{
  const f=fixture();const capture=await f.seal();f.state.blocks.push({text:'changed while sealed'});f.command({kind:'finish',request:'depart',lease:'claim',sequence:capture.sequence});
  assert.equal((await f.packet('release-result','depart')).ok,false);assert.equal(f.wrapper.inert,false);await f.close();assert.equal(f.sent.at(-1).blocks.at(-1).text,'changed while sealed');
});
test('wrong lease or foreign owner fails closed and unlocks only original wrapper',async()=>{
  for(const wrong of [{lease:'wrong'},{sequence:999},{scope:'foreign'}]) {
    const f=fixture();const capture=await f.seal();f.command({kind:'release',request:'bad',lease:'claim',sequence:capture.sequence,...wrong});
    await assert.rejects(f.done,/changed/);assert.equal(f.wrapper.inert,false);assert.equal(f.env.__listCommentsComposeCaptures.size,1);
  }
});
test('ACK timeout retains exact holder for explicit recovery, no silent completion',async()=>{
  const f=fixture({ack:false});await f.packet('capture');f.expire(5000);await assert.rejects(f.done,/acknowledgment timed out/);
  assert.equal(f.env.__listCommentsComposeCaptures.size,1);assert.equal(f.wrapper.inert,false);
  const retry=fixture({shared:f.shared,recover:true});const capture=await retry.packet('capture');assert.ok(capture.sequence>f.sent[0].sequence);await retry.close();
});
test('wrong acknowledgment fails instead of discarding pending content',async()=>{
  const f=fixture({ack:false});await f.packet('capture');f.command({kind:'ack',sequence:999});await assert.rejects(f.done,/acknowledgment changed/);assert.equal(f.env.__listCommentsComposeCaptures.size,1);
});
test('new same-ID root cannot be adopted during retained recovery',async()=>{
  const f=fixture({ack:false});await f.packet('capture');f.expire(5000);await assert.rejects(f.done);
  const replacement={inert:false};f.nodes.set(f.options.id,replacement);
  const retry=fixture({shared:f.shared,recover:true});await retry.done;assert.equal(retry.sent.at(-1).detached,true);assert.equal(retry.reads(),0);assert.equal(replacement.inert,false);
});
test('initial stale generation and duplicate running mount are refused',async()=>{
  const f=fixture();await f.packet('capture');await assert.rejects(run(f.options,{send(){},recv(){}},f.env),/unavailable/);await f.close();
  const g=fixture();await g.packet('capture');g.owner.dataset.editorGeneration='7';g.mutation();await g.done;
});

test('reused connected wrapper with successor generation is never unlocked by predecessor',async()=>{
  const f=fixture();await f.seal();f.wrapper.isConnected=true;f.owner.dataset.editorGeneration='5';f.mutation(f.owner);await f.done;
  assert.equal(f.wrapper.inert,true);assert.equal(f.sent.at(-1).detached,true);
});

test('readForCommit failure preserves native blocks and refuses without a lease',async()=>{
  const f=fixture();await f.packet('capture');const before=plain(f.state);f.env.superPlatinumRichEditor.readForCommit=()=>{throw Error('unsupported');};
  f.command({kind:'capture',request:'fail'});await f.packet('capture-refused','fail');assert.deepEqual(plain(f.state),before);assert.equal(f.wrapper.inert,false);await f.close();
});
test('readForCommit synchronous replacement cannot grant a seal',async()=>{
  const f=fixture();await f.packet('capture');const replacement={inert:false};f.env.superPlatinumRichEditor.readForCommit=()=>{f.nodes.set(f.options.id,replacement);return plain(f.state.blocks);};
  f.command({kind:'capture',request:'changed'});await f.packet('capture-refused','changed');assert.equal(f.wrapper.inert,false);assert.equal(replacement.inert,false);f.mutation(f.owner);await f.done;
});
test('hidden ancestor capture refuses before reading while continuous custody remains',async()=>{
  const f=fixture();await f.packet('capture');const closest=f.root.closest;f.root.closest=s=>s==='[hidden],[inert]'?{}:closest(s);
  f.command({kind:'capture',request:'hidden'});await f.packet('capture-refused','hidden');assert.equal(f.reads(),0);assert.equal(f.wrapper.inert,false);await f.close();
});
test('final acknowledgment is required before departure result and seal remains through close render',async()=>{
  const f=fixture({ack:false});const first=await f.packet('capture');f.command({kind:'ack',sequence:first.sequence});
  const capture=await f.seal();f.command({kind:'ack',sequence:capture.sequence});f.command({kind:'finish',request:'depart',lease:'claim',sequence:capture.sequence});
  const final=await f.packet('capture');assert.equal(final.finalCapture,true);assert.equal(f.sent.some(x=>x.kind==='release-result'),false);assert.equal(f.wrapper.inert,true);
  f.command({kind:'ack',sequence:final.sequence});await f.done;assert.equal(f.sent.at(-1).finished,true);assert.equal(f.wrapper.inert,true);
});

test('missing ACK for sealed capture unlocks original and leaves recoverable custody',async()=>{
  const f=fixture({ack:false});const first=await f.packet('capture');f.command({kind:'ack',sequence:first.sequence});
  const sealed=await f.seal();assert.equal(sealed.locked,true);assert.equal(f.wrapper.inert,true);f.expire(5000);
  await assert.rejects(f.done,/acknowledgment timed out/);assert.equal(f.wrapper.inert,false);assert.equal(f.env.__listCommentsComposeCaptures.size,1);
});
test('second capture cannot replace a live lease or overwrite its raw content',async()=>{
  const f=fixture();const sealed=await f.seal();f.command({kind:'capture',request:'competing'});await f.packet('capture-refused','competing');
  assert.equal(f.wrapper.inert,true);f.command({kind:'release',request:'release',lease:'claim',sequence:sealed.sequence});await f.packet('release-result','release');await f.close();
});

test('finish permits legitimate busy read-only editor while preserving original sealed bytes',async()=>{
  const f=fixture();const sealed=await f.seal();f.root.contentEditable='false';
  f.command({kind:'finish',request:'sent',lease:'claim',sequence:sealed.sequence});
  assert.equal((await f.packet('release-result','sent')).finished,true);await f.done;
  assert.deepEqual(f.sent.find(x=>x.finalCapture).blocks,sealed.blocks);assert.equal(f.root.contentEditable,'false');
});


test('post-persistence validation keeps original seal through committed detachment',async()=>{
  const f=fixture();const sealed=await f.seal();
  f.command({kind:'validate',request:'ready',lease:'claim',sequence:sealed.sequence});
  const result=await f.packet('release-result','ready');
  assert.equal(result.ok,true);assert.equal(result.validated,true);assert.equal(result.finished,false);
  assert.equal(f.wrapper.inert,true);assert.equal(f.sent.some(x=>x.finalCapture),false);
  assert.deepEqual(f.state.blocks,sealed.blocks);
  await f.close();assert.equal(f.sent.at(-1).detached,true);assert.deepEqual(f.sent.at(-1).blocks,sealed.blocks);
});
for(const changed of ['revision','blocks','composition','link','readonly','hidden','foreign-inert'])
  test(`post-persistence ${changed} drift refuses validation and releases only original lease`,async()=>{
    const f=fixture();const sealed=await f.seal();
    if(changed==='revision')f.state.revision++;
    if(changed==='blocks')f.state.blocks.push({type:'text',text:'newer native buffer'});
    if(changed==='composition')f.state.composing=true;
    if(changed==='link')f.wrapper.link.hidden=false;
    if(changed==='readonly')f.root.contentEditable='false';
    if(changed==='hidden'){const closest=f.root.closest;f.root.closest=s=>s==='[hidden]'?{}:closest(s);}
    if(changed==='foreign-inert')f.wrapper.parentElement={closest:s=>s==='[inert]'?{}:null};
    const original=plain(f.state);
    f.command({kind:'validate',request:'refused',lease:'claim',sequence:sealed.sequence});
    const result=await f.packet('release-result','refused');assert.equal(result.ok,false);assert.equal(result.validated,true);
    assert.equal(f.wrapper.inert,true);assert.deepEqual(plain(f.state),original);
    f.command({kind:'release',request:'release',lease:'claim',sequence:sealed.sequence});
    assert.equal((await f.packet('release-result','release')).ok,true);assert.equal(f.wrapper.inert,false);
    await f.close();assert.deepEqual(f.sent.at(-1).blocks,original.blocks);
  });
test('post-persistence same-ID replacement cannot validate or unlock successor',async()=>{
  const f=fixture();const sealed=await f.seal();const successor={inert:true};
  f.nodes.set(f.options.id,successor);f.root.isConnected=false;
  f.command({kind:'validate',request:'stale',lease:'claim',sequence:sealed.sequence});
  await f.done;assert.equal(f.sent.some(x=>x.request==='stale'&&x.ok===true),false);
  assert.equal(successor.inert,true);assert.deepEqual(f.sent.at(-1).blocks,sealed.blocks);
});


test('native Delta save/cancel intent waits for original acknowledged capture',async()=>{
 for(const action of ['save','cancel']) {
  const f=fixture({representation:'plain-delta',ack:false});const first=await f.packet('capture');
  f.event('comment-delta-action',action);await drain();assert.equal(f.sent.filter(x=>x.kind==='editor-action').length,0);
  f.command({kind:'ack',sequence:first.sequence});const intent=await f.packet('editor-action');
  assert.equal(intent.action,action);assert.equal(intent.sequence,first.sequence);assert.equal(intent.revision,first.revision);
  assert.equal(intent.generation,4);f.root.isConnected=false;f.mutation();const tail=await f.packet('capture');
  f.command({kind:'ack',sequence:tail.sequence});await f.done;
 }
});
for(const field of ['pending','composing'])test(`native Delta action refuses ${field} without any host intent`,async()=>{
 const f=fixture({representation:'plain-delta'});await f.packet('capture');f.state[field]=true;
 f.event('comment-delta-action','save');await drain();assert.equal(f.sent.filter(x=>x.kind==='editor-action').length,0);await f.close();
});
test('native Delta action from replaced original cannot act on successor',async()=>{
 const f=fixture({representation:'plain-delta'});await f.packet('capture');f.nodes.set(f.options.id,{});
 f.event('comment-delta-action','cancel');f.mutation();await f.done;assert.equal(f.sent.filter(x=>x.kind==='editor-action').length,0);
});
test('pending clipboard request follows ACK and host refusal leaves exact draft unchanged',async()=>{
 const f=fixture({representation:'plain-delta',ack:false});const first=await f.packet('capture');
 f.command({kind:'ack',sequence:first.sequence});await drain();const original=plain(f.state.ops);
 f.state.pending=true;f.state.paste={request:'paste-one',scope:f.options.scope,generation:'4',revision:f.state.revision,
  selectionRevision:1,selection:{index:1,length:0},ops:[{insert:'@Ada',attributes:{slackmention:{id:'U1',label:'@Ada'}}}],mentionIds:['U1'],emojiNames:[]};
 f.event('comment-delta-paste-request','paste-one');const held=await f.packet('capture');
 assert.equal(f.sent.filter(x=>x.kind==='clipboard-request').length,0);assert.equal(held.pending,true);
 f.command({kind:'ack',sequence:held.sequence});const request=await f.packet('clipboard-request');
 assert.equal(request.sequence,held.sequence);assert.deepEqual(request.clipboard,plain(f.state.paste));
 f.command({kind:'clipboard-rejected',sequence:request.sequence,request:'paste-one'});
 assert.equal((await f.packet('clipboard-result','paste-one')).applied,false);assert.deepEqual(f.state.ops,original);
 const settled=await f.packet('capture');f.command({kind:'ack',sequence:settled.sequence});await drain();
 f.root.isConnected=false;f.mutation();const tail=await f.packet('capture');f.command({kind:'ack',sequence:tail.sequence});await f.done;
});

test('rich captures do not acquire plain no-attachment capability',async()=>{const f=fixture();const initial=await f.packet('capture');assert.equal('attachmentEffects' in initial,false);const sealed=await f.seal();assert.equal('attachmentEffects' in sealed,false);await f.close();assert.equal('attachmentEffects' in f.sent.at(-1),false)});

for(const kind of ['validate','finish'])test(`plain no-attachment observation changed under lease refuses ${kind}`,async()=>{
 const f=fixture({representation:'plain-delta'}),sealed=await f.seal();
 const original=f.env.superPlatinumCommentDelta.snapshot;
 f.env.superPlatinumCommentDelta.snapshot=node=>({...original(node),attachmentEffects:'different'});
 f.command({kind,request:'changed-effects',lease:'claim',sequence:sealed.sequence});
 assert.equal((await f.packet('release-result','changed-effects')).ok,false);
 assert.equal(f.state.released,false);
 if(kind==='validate')assert.ok(f.env.superPlatinumCommentDelta.sealed());
 else {assert.equal(f.env.superPlatinumCommentDelta.sealed(),null);assert.equal(f.wrapper.inert,false);}
 f.env.superPlatinumCommentDelta.snapshot=original;
 if(kind==='validate'){
   f.command({kind:'release',request:'retry-effects',lease:'claim',sequence:sealed.sequence});
   await f.packet('release-result','retry-effects');
 }
 await f.close();
});
