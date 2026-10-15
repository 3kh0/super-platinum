import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import test from 'node:test';
import {fileURLToPath} from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const source=fs.readFileSync(path.join(root,'src/desktop/performance/emoji_retirement/probe.js'),'utf8');
const driver=fs.readFileSync(path.join(root,'src/desktop/performance/emoji_retirement/driver.js'),'utf8');

function harness(change=()=>{}) {
  let time=0,seq=0,callbackCount=0,frames=0;
  const choices=[{label:':rocket:',detail:'rocket'}];
  const popup={isConnected:true,hidden:true,querySelectorAll(){return choices.map(x=>({children:[{textContent:x.label},{textContent:x.detail}]}));}};
  const f={id:'channel-composer',isConnected:true,value:'',selectionStart:0,selectionEnd:0,selectionDirection:'forward',
    dataset:{draftScope:'scope',projectionVisit:'mount',completionReady:'true',emojiCatalog:'1/local',emojiPreferences:'1'},
    focus(){doc.activeElement=f;},setSelectionRange(a,b,d){this.selectionStart=a;this.selectionEnd=b;this.selectionDirection=d;},
    dispatchEvent(){
      const wire=JSON.stringify([f.value,f.selectionStart,f.selectionEnd,f.selectionDirection]);
      const result={kind:'emoji_results',seq:++seq,owner:'scope:mount:1',catalog:'1/local',preferences:1,choices};
      probe.query(f,seq,result.owner,wire);probe.receive(f,result,result.owner,seq,wire);
      popup.hidden=false;probe.accepted(f,result,result.owner,()=>{callbackCount++;return true;});
    }};
  const doc={activeElement:f,visibilityState:'visible',hasFocus:()=>true,getElementById(id){return id===f.id?f:id===f.id+'-completion'?popup:null;}};
  const context={window:{},document:doc,performance:{now:()=>++time},InputEvent:class{},KeyboardEvent:class{},Event:class{},setTimeout,clearTimeout,
    requestAnimationFrame(cb){frames++;change({f,popup,doc,frames});queueMicrotask(()=>{time+=16;cb(time);});}};
  vm.createContext(context);vm.runInContext(source,context);vm.runInContext(driver,context);
  const probe=context.createEmojiRetirementProbe({clock:()=>++time,document:doc});context.window.__spEmojiRetirement=probe;
  return {context,probe,f,popup,doc,choices,calls:()=>callbackCount,bridge:{send(){},async recv(){return {ok:true,id:'W1/c1/completion.channel/query',nativeComplete:true,state:{catalog:{ready:{token:'1/local'}}}};}}};
}

test('actual query result + DOM authority + two RAF witnesses',async()=>{
  const h=harness();const result=await h.context.window.emojiRetirementAction({id:'W1/c1/completion.channel/query',kind:'ordinary',subcase:'completion.channel',index:1},h.bridge);
  assert.equal(result.ok,true);assert.equal(result.witnesses.length,3);assert.equal(result.accepted.catalog,'1/local');
  assert.equal(result.witnesses[2].channel.wire[3],'forward');assert.equal(h.calls(),1);
  assert.equal(h.probe.evidence().active,null);
});
test('retained probe consumes stillCurrent once and retains no callback',()=>{
  const h=harness();h.probe.begin('x');h.f.value=':ro';h.f.setSelectionRange(3,3,'forward');h.f.dispatchEvent();
  for(let i=0;i<10;i++)h.probe.emoji(h.f,'x');assert.equal(h.calls(),1);
  assert.equal(JSON.stringify(h.probe.evidence()).includes('function'),false);
});
test('result cannot outlive wire, preferences, catalog or actual DOM',()=>{
  for(const mutate of [h=>h.f.value=':rot',h=>h.f.selectionDirection='backward',h=>h.f.dataset.emojiCatalog='2/local',h=>h.f.dataset.emojiPreferences='2',h=>h.popup.hidden=true,h=>h.choices[0]={label:':wrong:',detail:'x'}]){
    const h=harness();h.probe.begin('x');h.f.value=':ro';h.f.setSelectionRange(3,3,'forward');h.f.dispatchEvent();
    assert.ok(h.probe.emoji(h.f,'x'));mutate(h);assert.equal(h.probe.emoji(h.f,'x'),null);
  }
});
test('same query is not adaptively reseeded',async()=>{
  const h=harness();h.f.value=':ro';
  await assert.rejects(()=>h.context.window.emojiRetirementAction({id:'W1/c1/completion.channel/query',kind:'ordinary',subcase:'completion.channel',index:1},h.bridge),/fixed empty/);
});
test('actual second RAF mount/focus/selection changes reject success',async()=>{
  for(const mutate of [h=>h.f.dataset.projectionVisit='new',h=>h.doc.activeElement=null,h=>h.f.selectionDirection='backward']){
    const h=harness(x=>{if(x.frames===2)mutate(x);});
    await assert.rejects(()=>h.context.window.emojiRetirementAction({id:'W1/c1/completion.channel/query',kind:'ordinary',subcase:'completion.channel',index:1},h.bridge),/RAF endpoint lost authority/);
  }
});
test('bounded scalar ledger reports overflow and refuses overlapping actions',()=>{
  const h=harness();h.probe.begin('x');assert.throws(()=>h.probe.begin('y'),/Concurrent/);
  for(let i=0;i<16385;i++)h.probe.invalidate(h.f,{kind:'invalidate'},'owner');
  assert.equal(h.probe.evidence().events.length,16384);assert.equal(h.probe.overflow(),true);
  assert.throws(()=>h.probe.end('wrong'),/identity/);
});

function pickerHarness({ready=true,disabled=false}={}) {
  const h=harness();let dialog=null;
  const original=h.doc.getElementById.bind(h.doc);
  const rows=Array.from({length:120},(_,i)=>({disabled:disabled&&i===0,getAttribute:()=>i===0?'rocket':'name'+i}));
  const query={id:'emoji-query',dispatchEvent(){dialog=null;h.f.focus();}};
  const button={disabled:false,click(){dialog={dataset:{emojiChoiceReady:String(ready),emojiLifecycleCatalog:h.f.dataset.emojiCatalog},querySelectorAll:()=>rows};h.doc.activeElement=query;}};
  h.doc.getElementById=id=>id==='channel-composer-emoji'?button:id==='shell-overlay-dialog'?dialog:id==='emoji-query'?dialog?query:null:original(id);
  const bridge=id=>({send(){},async recv(){return {ok:true,id,nativeComplete:true,state:{catalog:{ready:{token:'1/local'}}}};}});
  return {...h,bridge};
}
test('picker readiness and all 120 actual rows survive both frames and Escape',async()=>{
  const h=pickerHarness(),id='W1/c1/refresh.picker/open';
  const opened=await h.context.window.emojiRetirementAction({id,index:1,kind:'ordinary',subcase:'refresh.picker'},h.bridge(id));
  assert.equal(opened.accepted.names.length,120);assert.equal(opened.witnesses[2].picker.ready,'true');
  const close='W1/c1/refresh.picker/close';
  const closed=await h.context.window.emojiRetirementAction({id:close,index:2,kind:'ordinary',subcase:'refresh.picker'},h.bridge(close));
  assert.equal(closed.witnesses[2].picker,null);assert.equal(closed.witnesses[2].active,'channel-composer');
});
test('unready or disabled picker rows cannot produce a ready endpoint',async()=>{
  for(const options of [{ready:false},{disabled:true}]){
    const h=pickerHarness(options),id='W1/c1/refresh.picker/open';
    await assert.rejects(()=>h.context.window.emojiRetirementAction({id,index:1,kind:'ordinary',subcase:'refresh.picker'},h.bridge(id)),/endpoint deadline/);
  }
});
