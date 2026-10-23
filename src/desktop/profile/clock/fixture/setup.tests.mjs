import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const script=fs.readFileSync(new URL('./setup.js',import.meta.url),'utf8');
function scene(){
  const rect={x:20,y:30,width:200,height:40};
  const make=()=>({isConnected:true,getBoundingClientRect:()=>rect,contains(n){return n===this;},dispatchEvent(e){this.event=e;},focus(){},scrollIntoView(){}});
  const composer=make();composer.value='';composer.setSelectionRange=(s,e)=>{composer.selectionStart=s;composer.selectionEnd=e;};
  class Textarea{} Object.defineProperty(Textarea.prototype,'value',{set(v){this.value=v;}});
  const close=make(),author=make();author.textContent='Maya Chen';author.closest=()=>({getAttribute:()=> '1781524900.000001'});
  const root=make();root.querySelector=()=>close;
  const w={ids:new WeakMap(),next:1,root,composer};
  const c={window:{__profileTimeEvidence:w},innerWidth:800,innerHeight:600,HTMLTextAreaElement:Textarea,
    Event:class{constructor(type,options){Object.assign(this,options,{type,isTrusted:false});}},MouseEvent:class{constructor(type,options){Object.assign(this,options,{type,isTrusted:false});}}};
  c.document={getElementById:()=>composer,querySelector:()=>root.isConnected?root:null,querySelectorAll:()=>[author],elementFromPoint:()=>c.hit};
  c.run=mode=>{c.mode=mode;return vm.runInNewContext(script,c);};
  return Object.assign(c,{composer,root,close,author,w});
}
test('covered original composer is refused before focus or input dispatch',()=>{
  const c=scene();c.hit=c.root;assert.equal(c.run('input').ok,false);assert.equal(c.composer.event,undefined);assert.equal(c.composer.value,'');
});
test('exposed input retains its actual rect, point and original synthetic event',()=>{
  const c=scene();c.hit=c.composer;const a=c.run('input');assert.equal(a.ok,true);assert.equal(a.isTrusted,false);assert.equal(a.rect.x,20);assert.equal(a.selectionStart,2);assert.equal(a.selectionEnd,9);assert.equal(c.composer.event.type,'input');
});
test('Close and reopened author require actual exposed originals',()=>{
  const c=scene();c.hit=c.root;assert.equal(c.run('close').ok,false);c.hit=c.close;assert.equal(c.run('close').ok,true);
  assert.equal(c.run('reopen').ok,false);c.root.isConnected=false;c.hit=c.author;
  const a=c.run('reopen');assert.equal(a.ok,true);assert.equal(a.messageId,'1781524900.000001');assert.equal(a.label,'Maya Chen');assert.equal(c.author.event.type,'click');
});
test('replaced composer and disconnected author refuse setup continuation',()=>{
  const c=scene();c.document.getElementById=()=>({});assert.equal(c.run('input').ok,false);
  const d=scene();d.w.setupAuthor=d.author;d.author.isConnected=false;assert.equal(d.run('selection').ok,false);
});
