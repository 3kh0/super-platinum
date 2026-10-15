import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';
const local=new URL('./composer_completion.js',import.meta.url);
const staged=fs.existsSync(local);
const controllerPath=staged?local:new URL('../src/desktop/composer_completion.js',import.meta.url);
const gatePath=staged?new URL('../emoji-direction-prep/selection_gate.js',import.meta.url):new URL('../src/desktop/view/completion/emoji_selection_gate.js',import.meta.url);
const turn=()=>new Promise(resolve=>setImmediate(resolve));
class Node {
  constructor(id='') { this.id=id;this.isConnected=true;this.dataset={};this.attributes={};this.events={};this.children=[];this.hidden=true; }
  setAttribute(k,v){this.attributes[k]=v;}
  removeAttribute(k){delete this.attributes[k];}
  addEventListener(k,f){(this.events[k]??=[]).push(f);}
  removeEventListener(k,f){this.events[k]=this.events[k]?.filter(x=>x!==f);}
  fire(k,event={}){for(const fn of this.events[k]??[])fn(event);}
  append(...children){this.children.push(...children);}
  replaceChildren(){this.children=[];}
  querySelectorAll(){return this.children.filter(x=>x.attributes.role==='option');}
  scrollIntoView(){}
}
async function setup(text='aa :sm',enabled=true) {
  const field=new Node('composer'),popup=new Node('composer-completion');
  Object.assign(field,{value:text,selectionStart:text.length,selectionEnd:text.length,selectionDirection:'none'});
  field.dataset.draftScope='account/channel/mount';
  Object.assign(field.dataset,{projectionVisit:'mount',emojiCatalog:'cat-1',emojiPreferences:'2'});
  const sent=[],inserted=[];let deliver,failInsert=false;
  const document={activeElement:field,body:new Node(),getElementById:id=>id===field.id?field:id===popup.id?popup:null,createElement:()=>new Node()};
  const context={document,setTimeout,console,window:{},MutationObserver:class{observe(){}disconnect(){}},
    fieldId:field.id,labels:{},workspace:'T',completionOwner:'owner',emojiNativeProbe:enabled,
    dioxus:{send:event=>sent.push(event),recv:()=>new Promise(resolve=>deliver=resolve)}};
  context.window.superPlatinumReferences={mount(){},insert(node,value,label,start,end,spacer){
    inserted.push({value,label,start,end,spacer});if(failInsert)return false;
    node.value=node.value.slice(0,start)+value+spacer+node.value.slice(end);
    node.selectionStart=node.selectionEnd=start+value.length+spacer.length;node.selectionDirection='none';
    node.fire('input');return true;
  }};
  vm.createContext(context);
  vm.runInContext(fs.readFileSync(gatePath,'utf8'),context);
  const code=fs.readFileSync(controllerPath,'utf8');
  const running=vm.runInContext('(async()=>{'+code+'})()',context);
  await turn();
  const result=async value=>{assert(deliver,'controller is awaiting result');const send=deliver;deliver=null;send(value);await turn();};
  const key=key=>field.fire('keydown',{key,preventDefault(){},stopImmediatePropagation(){}});
  const close=async()=>{await result(null);await running;};
  const showEmoji=()=>result({kind:'emoji_results',seq:sent.at(-1).seq,owner:'owner',catalog:'cat-1',preferences:'2',start:3,end:6,spacer:' ',choices:[{id:'Esmile/smile',label:':smile:',detail:''}],status:''});
  return {field,popup,sent,inserted,result,key,close,showEmoji,observations:()=>context.window.__emojiNativeControllerEvents,overflow:()=>context.window.__emojiNativeControllerOverflow,fail:()=>failInsert=true};
}

test('actual composition listener and denied decision are observed without changing admission',async()=>{
  const s=await setup();await s.showEmoji();s.key('Enter');const ticket=s.sent.at(-1);
  s.field.fire('compositionstart');
  await s.result({...ticket,kind:'emoji_admit',allowed:true,value:'x',start:3,end:6,spacer:' '});
  assert.equal(s.observations()[0].kind,'composition-start');assert.equal(s.observations()[0].composing,true);
  const d=s.observations()[1];assert.equal(d.kind,'admission');assert.equal(d.accepted,false);assert.equal(d.composing,true);
  assert.equal(d.result.serial,ticket.serial);assert.equal(d.snapshot.text,'aa :sm');assert.equal(d.focused,true);
  assert.equal(s.inserted.length,0);assert(!s.sent.some(e=>e.kind==='emoji_inserted'));
  s.field.fire('compositionend');assert.equal(s.observations()[2].kind,'composition-end');assert.equal(s.observations()[2].composing,false);
  await s.close();
});
test('actual newer-input and catalog refusal record completed false results',async()=>{
  for(const reason of ['newer','catalog']) {
    const s=await setup();await s.showEmoji();s.key('Enter');const ticket=s.sent.at(-1);
    if(reason==='newer'){s.field.value='newer';s.field.fire('input');}else s.field.dataset.emojiCatalog='cat-2';
    await s.result({...ticket,kind:'emoji_admit',allowed:reason==='newer',value:'x',start:3,end:6,spacer:' '});
    assert.equal(s.observations().length,1);assert.equal(s.observations()[0].accepted,false);
    assert.equal(s.observations()[0].result.allowed,reason==='newer');assert.equal(s.inserted.length,0);await s.close();
  }
});
test('probe is inert outside exact Rust fixture and after scope or mount retirement',async()=>{
  for(const mutation of ['disabled','scope','mount']) {
    const s=await setup('aa :sm',mutation!=='disabled');
    if(mutation==='scope')s.field.dataset.draftScope='retired';
    if(mutation==='mount')s.field.dataset.projectionVisit='retired';
    s.field.fire('compositionstart');s.field.fire('compositionend');
    assert.equal(s.observations(),undefined);await s.close();
  }
});
test('actual observer ledger is bounded and reports overflow',async()=>{
  const s=await setup();for(let i=0;i<33;i++){s.field.fire('compositionstart');s.field.fire('compositionend');}
  assert.equal(s.observations().length,64);assert.equal(s.overflow(),true);await s.close();
});

test('Escape suppresses an identical seed; an intervening edit starts the next guard query',async()=>{
  const s=await setup('\u{1f98a} :rocket');
  s.field.selectionDirection='forward';s.field.fire('input');
  const offer=()=>s.result({kind:'emoji_results',seq:s.sent.at(-1).seq,owner:'owner',catalog:'cat-1',preferences:'2',start:3,end:10,spacer:' ',choices:[{id:'Erocket/rocket',label:':rocket:',detail:''}],status:''});
  await offer();assert.equal(s.popup.hidden,false);
  s.key('Escape');assert.equal(s.popup.hidden,true);
  const afterEscape=s.sent.length;
  // The field already has focus. Assigning the same native value and selection
  // produces input, but correctly cannot undo the user's Escape suppression.
  s.field.value='\u{1f98a} :rocket';s.field.selectionStart=s.field.selectionEnd=10;
  s.field.selectionDirection='forward';s.field.fire('input');
  assert.equal(s.sent.length,afterEscape);assert.equal(s.popup.hidden,true);
  // The fixture now starts a genuinely new edit before retyping its next query.
  s.field.value='\u{1f98a} ';s.field.selectionStart=s.field.selectionEnd=3;s.field.fire('input');
  s.field.value='\u{1f98a} :rocket';s.field.selectionStart=s.field.selectionEnd=10;s.field.fire('input');
  assert.equal(s.sent.length,afterEscape+2);
  assert.equal(s.sent.at(-1).kind,'query');assert.equal(s.sent.at(-1).text,'\u{1f98a} :rocket');
  await offer();assert.equal(s.popup.hidden,false);
  s.key('Enter');assert.equal(s.sent.at(-1).kind,'emoji_choose');
  assert.equal(s.sent.at(-1).snapshot.text,'\u{1f98a} :rocket');
  assert.equal(s.inserted.length,0);await s.close();
});

