import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const source=fs.readFileSync(new URL('../src/desktop/view/slack_lists/comments/bridge.js',import.meta.url),'utf8');
const run=vm.runInNewContext(`${source}\nlistCommentsComposeBridge`);
const plain=v=>JSON.parse(JSON.stringify(v));
const drain=async()=>{for(let i=0;i<30;i++)await Promise.resolve();};
function fixture({ack=true,shared=null,recover=false}={}) {
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
  const done=run(options,bridge,env);done.catch(()=>{});
  async function packet(kind,request){await drain();const value=sent.findLast(x=>x.kind===kind&&(request===undefined||x.request===request));assert.ok(value,`Missing ${kind}/${request}: ${JSON.stringify(sent)}`);return value;}
  async function seal(request='claim'){command({kind:'capture',request});return packet('capture',request);}
  async function close(){root.isConnected=false;observer?.([{target:owner}]);await drain();return done;}
  return {root,owner,wrapper,state,nodes,env,sent,options,done,command,packet,seal,close,timers,
    shared:{root,owner,wrapper,state,nodes,env},reads:()=>commitReads,
    event:n=>events.get(n)?.(),mutation:(target=root)=>observer?.([{target}]),
    expire:ms=>{for(const [key,t]of [...timers])if(t.ms===ms){timers.delete(key);t.fn();}}};
}

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
