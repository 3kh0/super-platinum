import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';
const controller=readFileSync(new URL('../src/desktop/composer_completion.js',import.meta.url),'utf8');
const tick=()=>new Promise(resolve=>setImmediate(resolve));

async function harness() {
  class Element extends EventTarget {
    constructor(){super();this.children=[];this.attributes={};this.dataset={};this.isConnected=true;}
    setAttribute(key,value){this.attributes[key]=value;}
    getAttribute(key){return this.attributes[key];}
    removeAttribute(key){delete this.attributes[key];}
    append(...children){this.children.push(...children);}
    replaceChildren(){this.children=[];}
    querySelectorAll(){return this.children.filter(child=>child.attributes.role==='option');}
    scrollIntoView(){}
  }
  const field=new Element(),popup=new Element();
  field.id='field';popup.id='field-completion';field.value='@a';field.selectionStart=field.selectionEnd=2;
  const sent=[],inserted=[];let stopped;
  let resolve;const queued=[];
  const recv=()=>queued.length?Promise.resolve(queued.shift()):new Promise(r=>resolve=r);
  const deliver=async reply=>{if(resolve){const r=resolve;resolve=undefined;r(reply);}else queued.push(reply);await tick();};
  const document={activeElement:field,body:{},getElementById:id=>id==='field'?field:popup,createElement:()=>new Element()};
  const context={fieldId:'field',labels:{},workspace:'T1',document,setTimeout,Promise,
    MutationObserver:class {constructor(callback){stopped=callback;}observe(){}disconnect(){}},
    dioxus:{send:event=>sent.push(event),recv},
    window:{superPlatinumReferences:{mount(){},insert(...args){inserted.push(args);}}}};
  const running=vm.runInNewContext(`(async()=>{${controller}})()`,context);
  await tick();
  const key=name=>{const event=new Event('keydown',{cancelable:true});Object.defineProperty(event,'key',{value:name});field.dispatchEvent(event);return event;};
  const edit=text=>{field.value=text;field.selectionStart=field.selectionEnd=text.length;field.dispatchEvent(new Event('input'));};
  const reply=(seq,ids)=>({seq,start:0,end:2,spacer:' ',choices:ids.map(id=>({token:`<@${id}>`,label:`@${id}`,detail:''})),status:''});
  const close=async()=>{field.isConnected=false;stopped();await running;};
  return {field,popup,sent,inserted,deliver,key,edit,reply,close};
}

test('out-of-order and dismissed responses cannot reopen a stale query',async()=>{
  const h=await harness();
  try {
    const first=h.sent.at(-1).seq;
    h.edit('@b');const second=h.sent.at(-1).seq;
    await h.deliver(h.reply(first,['OLD']));assert.equal(h.popup.hidden,true);
    await h.deliver(h.reply(second,['NEW']));assert.equal(h.popup.hidden,false);
    assert.equal(h.key('Escape').defaultPrevented,true);assert.equal(h.popup.hidden,true);
    await h.deliver(h.reply(second,['LATE']));assert.equal(h.popup.hidden,true);
    assert.equal(h.inserted.length,0);
  } finally {await h.close();}
});
test('remote ranking preserves the person explicitly reached with arrow keys',async()=>{
  const h=await harness();
  try {
    const seq=h.sent.at(-1).seq;
    await h.deliver(h.reply(seq,['U1','U2']));
    assert.equal(h.key('ArrowDown').defaultPrevented,true);
    await h.deliver(h.reply(seq,Array.from({length:8},(_,i)=>`R${i}`)));
    assert.equal(h.popup.querySelectorAll().length,8);
    assert.equal(h.key('Enter').defaultPrevented,true);
    assert.equal(h.inserted[0][1],'<@U2>');
  } finally {await h.close();}
});
test('a moved caret and IME key cannot select a stale suggestion',async()=>{
  const h=await harness();
  try {
    await h.deliver(h.reply(h.sent.at(-1).seq,['U1']));
    const ime=new Event('keydown',{cancelable:true});
    Object.defineProperties(ime,{key:{value:'Enter'},isComposing:{value:true}});
    h.field.dispatchEvent(ime);assert.equal(ime.defaultPrevented,false);assert.equal(h.inserted.length,0);
    h.field.selectionStart=h.field.selectionEnd=0;
    h.key('Enter');assert.equal(h.inserted.length,0);
  } finally {await h.close();}
});
