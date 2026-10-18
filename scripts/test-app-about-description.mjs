import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import path from 'node:path';
const root=process.argv[2];
const source=fs.readFileSync(path.join(root,'src/desktop/app_home/about/description.js'),'utf8');
let groups=0;
function test(name,fn){fn();++groups;console.log('PASS '+name);}
function fixture(overflow=true){
 const listeners=()=>({listeners:{},addEventListener(k,v){this.listeners[k]=v;},removeEventListener(k,v){if(this.listeners[k]===v)delete this.listeners[k];}});
 const button={...listeners(),hidden:true,textContent:'Show more',attributes:{},setAttribute(k,v){this.attributes[k]=v;}};
 const body={...listeners(),dataset:{},clientHeight:120,scrollHeight:overflow?240:80,clientWidth:400,scrollWidth:400,
   contains(n){return n?.inside===true;},getBoundingClientRect(){return {top:20,bottom:140};}};
 const panel={isConnected:true,dataset:{visit:'1',revision:'2'}};
 const root={isConnected:true,dataset:{aboutVisit:'1',aboutRevision:'2'},closest(){return panel;},querySelectorAll(q){return q==='[data-about-body]'?[body]:[button];}};
 const observers=[],document={body:{},activeElement:{},querySelectorAll(){return [root];}};
 class Observer{constructor(callback){this.callback=callback;this.closed=false;observers.push(this);}observe(){}disconnect(){this.closed=true;}}
 const context=vm.createContext({document,CSS:{escape:v=>v},ResizeObserver:Observer,MutationObserver:Observer});
 vm.runInContext(source,context);
 return {root,panel,body,button,document,observers,install(){context.installAppAboutDescription('id');},resize(){observers[0].callback();},mutate(records=[]){observers[1].callback(records);}};
}
test('actual overflow shows accessible toggle; short content has no unnecessary control',()=>{
 for(const overflow of [true,false]){const h=fixture(overflow);h.install();assert.equal(h.button.hidden,!overflow);assert.equal(h.button.attributes['aria-expanded'],'false');assert.equal(h.body.dataset.expanded,'false');}
});
test('pointer/keyboard click toggles existing body without moving focus',()=>{
 const h=fixture();h.install();h.document.activeElement=h.button;h.button.listeners.click();assert.equal(h.body.dataset.expanded,'true');assert.equal(h.button.textContent,'Show less');assert.equal(h.button.attributes['aria-expanded'],'true');assert.equal(h.document.activeElement,h.button);
 h.button.listeners.click();assert.equal(h.body.dataset.expanded,'false');assert.equal(h.button.textContent,'Show more');
});
test('resize and text/media mutation remeasure without changing expanded user choice',()=>{
 const h=fixture(false);h.install();h.body.scrollHeight=200;h.mutate([{target:h.body}]);assert.equal(h.button.hidden,false);h.button.listeners.click();h.body.scrollHeight=80;h.resize();assert.equal(h.body.dataset.expanded,'true');assert.equal(h.button.hidden,false);
});
test('shortening collapsed content does not hide a focused toggle',()=>{
 const h=fixture();h.install();h.document.activeElement=h.button;h.body.scrollHeight=80;h.resize();assert.equal(h.button.hidden,false);h.document.activeElement={};h.resize();assert.equal(h.button.hidden,true);
});
test('keyboard focus on a clipped descendant reveals it without stealing focus',()=>{
 const h=fixture();h.install();const link={inside:true,getBoundingClientRect:()=>({top:180,bottom:200})};h.document.activeElement=link;h.body.listeners.focusin({target:link});assert.equal(h.body.dataset.expanded,'true');assert.equal(h.document.activeElement,link);
});
test('visible link focus does not expand or move focus',()=>{
 const h=fixture();h.install();const link={inside:true,getBoundingClientRect:()=>({top:30,bottom:50})};h.document.activeElement=link;h.body.listeners.focusin({target:link});assert.equal(h.body.dataset.expanded,'false');assert.equal(h.document.activeElement,link);
});
test('retired owner/revision and disconnected nodes clean up before any action',()=>{
 for(const change of [h=>h.panel.dataset.visit='3',h=>h.panel.dataset.revision='9',h=>h.root.isConnected=false,h=>h.panel.isConnected=false]){
  const h=fixture();h.install();change(h);h.button.listeners.click();assert.equal(h.body.dataset.expanded,'false');assert.ok(h.observers.every(o=>o.closed));assert.equal(h.button.listeners.click,undefined);assert.equal(h.body.listeners.focusin,undefined);assert.equal(h.root.__aboutDescription,undefined);
 }
});
test('remount owns one observer pair and old cleanup cannot remove new owner',()=>{
 const h=fixture();h.install();h.button.listeners.click();const old=h.root.__aboutDescription;h.install();assert.ok(h.observers.slice(0,2).every(o=>o.closed));const current=h.root.__aboutDescription;old();h.observers[0].callback();assert.equal(h.button.textContent,'Show more');assert.equal(h.root.__aboutDescription,current);assert.ok(h.button.listeners.click);h.root.isConnected=false;h.observers[3].callback([]);assert.equal(h.root.__aboutDescription,undefined);
});
test('duplicate/missing roots and wrong About identity refuse installation',()=>{
 for(const change of [h=>h.document.querySelectorAll=()=>[],h=>h.document.querySelectorAll=()=>[h.root,h.root],h=>h.panel.dataset.revision='8']){
  const h=fixture();change(h);h.install();assert.equal(h.button.listeners.click,undefined);assert.equal(h.button.hidden,true);
 }
});
console.log(JSON.stringify({groups,scope:'actual helper under pure DOM model; native unrun'}));
