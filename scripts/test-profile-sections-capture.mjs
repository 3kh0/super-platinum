import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const source=fs.readFileSync(new URL('../src/desktop/view/profile_edit/sections/capture.js',import.meta.url),'utf8');
const bridge=vm.runInNewContext(fs.readFileSync(new URL('../src/desktop/view/profile_edit/sections/bridge.js',import.meta.url),'utf8'));
function fixture(){
  const timers=new Map();let next=0;
  const status={isConnected:true,textContent:''};
  const button={disabled:false,isConnected:true};
  const disabled={disabled:true,isConnected:true};
  const field={id:'profile-section-rich',isConnected:true,contentEditable:'true',get isContentEditable(){return this.contentEditable==='true';}};
  const pane={isConnected:true,dataset:{richVisit:'3',composing:'false'},querySelector:s=>s==='#profile-section-rich-status'?status:field,querySelectorAll:()=>[button,disabled]};
  const root={isConnected:true,dataset:{dialogGeneration:'1',sectionSerial:'2',sectionLocked:'false'},querySelector:()=>pane};
  const nodes=new Map([['shell-overlay-dialog',root],['profile-section-rich',field]]);
  let reads=0;
  const blocks=[{type:'rich_text',elements:[]}];
  const captured={snapshot:{blocks,revision:0,composing:false,generation:'2'},tokens:[]};
  const editor={resolved(node){assert.equal(node,field);reads++;return captured;}};
  const capture=vm.runInNewContext(source,{document:{getElementById:id=>nodes.get(id)},window:{superPlatinumRichEditor:editor},setTimeout(fn,ms){assert.equal(ms,5000);timers.set(++next,fn);return next;},clearTimeout:id=>timers.delete(id)});
  const expire=()=>{for(const [id,fn]of [...timers]){timers.delete(id);fn();}};
  return {button,disabled,field,pane,root,nodes,blocks,captured,capture,status,timers,expire,editor,reads:()=>reads};
}
test('exact current native read seals input and restores original button state',()=>{
  const f=fixture(),ticket=f.capture(1,2,3);
  assert.equal(ticket.captured,f.captured);assert.equal(f.reads(),1);
  assert.equal(f.field.isContentEditable,false);assert.equal(f.button.disabled,true);
  ticket.release();assert.equal(f.field.isContentEditable,true);assert.equal(f.button.disabled,false);assert.equal(f.disabled.disabled,true);
});

test('resolved duplicate witnesses and canonical labels reach the bridge unchanged',async()=>{
  const f=fixture();
  f.captured.tokens=[1,2].map(index=>({value:{type:'channel',channel_id:'CMENTION01',fixture_occurrence:index},label:'#project-harbor'}));
  f.captured.snapshot.blocks=[{type:'rich_text',elements:[{type:'rich_text_section',elements:f.captured.tokens.map(t=>t.value)}]}];
  let sent;
  await bridge(f.capture,1,2,3,{send:v=>sent=v,recv:async()=>true});
  assert.equal(sent,f.captured);
  assert.deepEqual(sent.tokens.map(t=>t.value.fixture_occurrence),[1,2]);
  assert.deepEqual(sent.tokens.map(t=>t.label),['#project-harbor','#project-harbor']);
});

test('malformed or stale resolved snapshot refuses without sealing or timers',()=>{
  for(const alter of [f=>f.captured.snapshot.blocks=null,f=>f.captured.tokens=null,
      f=>f.captured.snapshot.composing=true,f=>f.captured.snapshot.generation='9',
      f=>f.captured.snapshot.revision=-1,f=>f.captured.snapshot.revision=NaN,
      f=>f.captured.snapshot.revision=Number.MAX_SAFE_INTEGER+1]){
    const f=fixture();alter(f);assert.equal(f.capture(1,2,3),null);
    assert.equal(f.field.isContentEditable,true);assert.equal(f.button.disabled,false);
    assert.equal(f.timers.size,0);assert.match(f.status.textContent,/Your text is still here/);
  }
});
test('IME, locked, readonly and wrong generation refuse before reading',()=>{
  for(const alter of [f=>f.pane.dataset.composing='true',f=>f.root.dataset.sectionLocked='true',f=>f.field.contentEditable='false',f=>f.root.dataset.dialogGeneration='8',f=>f.root.dataset.sectionSerial='8',f=>f.pane.dataset.richVisit='8']){
    const f=fixture();alter(f);assert.equal(f.capture(1,2,3),null);assert.equal(f.reads(),0);
  }
});
test('replaced dialog, editor or rich visit cannot be unlocked by an old ticket',()=>{
  for(const alter of [f=>f.nodes.set('shell-overlay-dialog',{}),f=>f.nodes.set('profile-section-rich',{}),f=>f.pane.dataset.richVisit='4',f=>f.root.dataset.sectionLocked='true',f=>f.root.isConnected=false]){
    const f=fixture(),ticket=f.capture(1,2,3);alter(f);ticket.release();assert.equal(f.field.isContentEditable,false);assert.equal(f.button.disabled,true);
  }
});
test('detached predecessor is never read',()=>{const f=fixture();f.field.isConnected=false;assert.equal(f.capture(1,2,3),null);assert.equal(f.reads(),0);});

test('native read failure preserves text and reports recovery without sealing',async()=>{
  const f=fixture();f.editor.resolved=()=>{throw Error('read');};
  let sent;
  await bridge(f.capture,1,2,3,{send:v=>sent=v,recv:()=>assert.fail('no ticket')});
  assert.equal(sent,null);assert.equal(f.field.contentEditable,'true');
  assert.match(f.status.textContent,/Could not read/);assert.equal(f.timers.size,0);
});
test('send failure releases exact ticket immediately and reports recovery',async()=>{
  const f=fixture();await bridge(f.capture,1,2,3,{send(){throw Error('send');},recv:()=>assert.fail('send failed')});
  assert.equal(f.field.contentEditable,'true');assert.equal(f.button.disabled,false);
  assert.equal(f.disabled.disabled,true);assert.match(f.status.textContent,/text is still here/);assert.equal(f.timers.size,0);
});
test('recv rejection after send retains seal until watchdog, never authorizing late host data',async()=>{
  const f=fixture();const done=bridge(f.capture,1,2,3,{send:v=>assert.equal(v,f.captured),recv:()=>Promise.reject(Error('recv'))});
  await Promise.resolve();await Promise.resolve();
  assert.equal(f.field.contentEditable,'false');assert.equal(f.timers.size,1);
  f.expire();await done;assert.equal(f.field.contentEditable,'true');assert.match(f.status.textContent,/Try again/);
});
test('dropped host or missing ack expires with bounded visible recovery',async()=>{
  const f=fixture();const done=bridge(f.capture,1,2,3,{send(){},recv:()=>new Promise(()=>{})});
  assert.equal(f.field.contentEditable,'false');f.expire();await done;
  assert.equal(f.field.contentEditable,'true');assert.equal(f.button.disabled,false);assert.equal(f.timers.size,0);
  assert.match(f.status.textContent,/Try again/);
});
test('normal host ack releases before watchdog without a false failure',async()=>{
  const f=fixture();await bridge(f.capture,1,2,3,{send(){},recv:async()=>true});
  assert.equal(f.field.contentEditable,'true');assert.equal(f.status.textContent,'');assert.equal(f.timers.size,0);
});
test('explicit host refusal preserves native text until watchdog and reports retry',async()=>{
  const f=fixture();const done=bridge(f.capture,1,2,3,{send(){},recv:async()=>false});
  await Promise.resolve();assert.equal(f.field.contentEditable,'false');f.expire();await done;
  assert.equal(f.field.contentEditable,'true');assert.match(f.status.textContent,/Try again/);
});
test('cancel or replacement cannot mutate successor nodes on watchdog',async()=>{
  const f=fixture();const done=bridge(f.capture,1,2,3,{send(){},recv:()=>new Promise(()=>{})});
  const successor={isConnected:true,contentEditable:'false'};
  f.nodes.set('profile-section-rich',successor);f.field.isConnected=false;
  f.expire();await done;
  assert.equal(successor.contentEditable,'false');assert.equal(f.field.contentEditable,'false');assert.equal(f.status.textContent,'');
});
test('idempotent old cleanup never releases a successor ticket on the same editor',()=>{
  const f=fixture(),old=f.capture(1,2,3);old.release();
  const current=f.capture(1,2,3);old.release();old.fail();
  assert.equal(f.field.contentEditable,'false');assert.equal(f.timers.size,1);
  current.release();assert.equal(f.field.contentEditable,'true');assert.equal(f.timers.size,0);
});
test('late ack after watchdog cannot release a new seal or erase the recovery status',async()=>{
  const f=fixture();let ack;
  const done=bridge(f.capture,1,2,3,{send(){},recv:()=>new Promise(resolve=>ack=resolve)});
  f.expire();await done;assert.match(f.status.textContent,/Try again/);
  const next=f.capture(1,2,3);assert.equal(f.field.contentEditable,'false');
  ack(true);await Promise.resolve();await Promise.resolve();
  assert.equal(f.field.contentEditable,'false');assert.equal(f.timers.size,1);next.release();
});
