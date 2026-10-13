import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';
const source=readFileSync(new URL('../src/desktop/composer_emoji/dom.js',import.meta.url),'utf8');
function setup(text='🦊 <@U1> tail',start=9,end=start) {
  let field={isConnected:true,dataset:{draftScope:'scope',completionReady:'true'},value:text,
    selectionStart:start,selectionEnd:end,selectionDirection:'backward',
    focus(){document.activeElement=this;},setSelectionRange(a,b,d){this.selectionStart=a;this.selectionEnd=b;this.selectionDirection=d;}};
  const button={},body={},frames=[],insertions=[];
  const listeners={};
  const makeDialog=()=>({contains:n=>n===dialog,querySelector:selector=>selector==='#emoji-picker-title'?{textContent:'Insert emoji'}:{}});
  let dialog=makeDialog();
  const document={body,activeElement:field,addEventListener:(type,fn)=>listeners[type]=fn,getElementById:id=>id==='field'?field:id==='field-emoji'?button:dialog};
  const window={superPlatinumReferences:{insert(f,value,label,a,b,spacer){
    insertions.push({value,label,a,b,spacer});f.value=f.value.slice(0,a)+value+f.value.slice(b);
    f.focus();f.setSelectionRange(a+value.length,a+value.length);
  }}};
  vm.runInNewContext(source,{window,document,requestAnimationFrame:f=>frames.push(f)});
  const api=window.superPlatinumComposerEmoji;
  const capture=(serial=1)=>{assert.equal(api.capture(serial,'field','scope'),true);document.activeElement=dialog;};
  const flush=()=>{for(let i=0;frames.length&&i<65;i++)frames.shift()();};
  const close=()=>{dialog=undefined;document.activeElement=body;flush();};
  return {api,document,field,insertions,capture,close,flush,
    choice(type='click',oldDialog=dialog) {
      let stopped=false;
      const target={id:'emoji-query',closest:selector=>selector==='#shell-overlay-dialog'?oldDialog:{}};
      listeners[type]({type,target,key:'Enter',preventDefault(){},stopImmediatePropagation(){stopped=true;}});
      return {stopped,dialog:oldDialog};
    },
    newDialog(){dialog=makeDialog();},replace(){field={...field};},frames};
}
test('Unicode caret and selection insert exactly once then restore focus',()=>{
  const h=setup('🦊 <@U1> word tail',9,13);h.capture();
  assert.equal(h.api.choose(1,'🚀'),true);assert.equal(h.api.choose(1,'🚀'),false);
  h.close();assert.equal(h.field.value,'🦊 <@U1> 🚀 tail');
  assert.equal(h.field.selectionStart,11);assert.equal(h.document.activeElement,h.field);
  assert.equal(h.insertions.length,1);
});
test('Cancel retains exact selection and wire text',()=>{
  const h=setup('🦊 <@U1> tail',3,8);h.capture();h.api.cancel(1);h.close();
  assert.equal(h.field.value,'🦊 <@U1> tail');assert.equal(h.field.selectionStart,3);
  assert.equal(h.field.selectionEnd,8);assert.equal(h.field.selectionDirection,'backward');
  assert.equal(h.insertions.length,0);assert.equal(h.document.activeElement,h.field);
});
test('changed text, caret, mount, scope, composition and disabled fields reject delayed insertion',()=>{
  for(const change of [h=>h.field.value+='new',h=>h.field.selectionStart=0,h=>h.replace(),
    h=>h.field.dataset.draftScope='new',h=>h.field.isConnected=false,
    h=>h.field.dataset.composing='true',h=>h.field.disabled=true,h=>h.field.readOnly=true]) {
    const h=setup();h.capture();h.api.choose(1,':ship_it:');change(h);h.close();
    assert.equal(h.insertions.length,0);
  }
});
test('manual focus and replacement picker cannot be stolen by a closing picker',()=>{
  const h=setup();h.capture();h.api.choose(1,'🚀');
  h.document.activeElement={};h.flush();assert.equal(h.insertions.length,0);
  const other=setup();other.capture();other.api.choose(1,'🚀');other.capture(2);other.close();
  assert.equal(other.insertions.length,0);
});
test('capture requires ready reference mapping and exact owner, and discard is serial scoped',()=>{
  const h=setup();h.field.dataset.completionReady='false';assert.equal(h.api.capture(1,'field','scope'),false);
  h.field.dataset.completionReady='true';assert.equal(h.api.capture(1,'field','old'),false);
  h.capture(2);h.api.discard(1);assert.equal(h.api.choose(2,':ship_it:'),true);h.close();
  assert.equal(h.insertions.length,1);
});

test('native choice is consumed once before bridge dispatch, including after ticket removal',()=>{
  for(const type of ['click','keydown']) {
    const h=setup();h.capture();
    const first=h.choice(type);assert.equal(first.stopped,false);
    h.api.choose(1,'🚀');
    assert.equal(h.choice(type,first.dialog).stopped,true);
    h.close();h.newDialog();h.capture(2);
    assert.equal(h.choice(type,first.dialog).stopped,true);
    assert.equal(h.choice(type).stopped,false);
  }
});
