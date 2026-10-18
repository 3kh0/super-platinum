const {strict:assert}=require('node:assert');
const {mountScheduledForwardFocus}=require('../src/desktop/forward/scheduled/focus.js');
function fixture() {
  const listeners=new Set(),frames=[],observers=[];
  const doc={activeElement:null,addEventListener:(_n,f)=>listeners.add(f),removeEventListener:(_n,f)=>listeners.delete(f)};
  class Node {
    constructor(id,parent=null){this.id=id;this.parentElement=parent;this.isConnected=true;this.disabled=false;this.hidden=false;this.dataset={};this.attrs={};this.nodes=[];parent?.nodes.push(this);}
    contains(n){return n===this||this.nodes.some(x=>x.contains(n));}
    closest(s){if(s==='#scheduled-forward-edit'&&this.id==='scheduled-forward-edit')return this;return this.parentElement?.closest(s)||null;}
    querySelector(s){return this.nodes.flatMap(x=>[x,...x.nodes]).find(n=>s==='status'?n.id==='status':s==='fallback'?n.id==='fallback':s==='[data-scheduled-focus-target]'?n.id==='local-status':false)||null;}
    getClientRects(){return this.hidden?[]:[{}];}
    getAttribute(k){return this.attrs[k]??null;}
    scrollIntoView(){this.scrolled=true;}
    focus(){doc.activeElement=this;for(const fn of [...listeners])fn({target:this});}
  }
  const body=new Node('body'),root=new Node('root',body),status=new Node('status',root),button=new Node('button',root),outside=new Node('outside',body);
  root.dataset.owner='one';doc.body=body;doc.activeElement=button;doc.getElementById=id=>id==='root'?root:null;
  const env={document:doc,getComputedStyle:()=>({visibility:'visible'}),requestAnimationFrame:f=>frames.push(f),MutationObserver:class{constructor(fn){this.fn=fn;observers.push(this);}observe(_r,options){this.options=options;}disconnect(){this.closed=true;}}};
  const config={root:'root',ownerKey:'owner',owner:'one',target:'status',fallback:'fallback'};
  assert.equal(mountScheduledForwardFocus(config,env),true);
  const flush=()=>{for(const observer of observers)if(!observer.closed)observer.fn();while(frames.length)frames.shift()();};
  const disable=()=>{button.disabled=true;doc.activeElement=body;};
  return {env,doc,body,root,status,button,outside,flush,disable,Node,config,observers,frames,listeners};
}
const cases=[
 ['disabled control gives focus to owned visible status',()=>{const f=fixture();f.disable();f.flush();assert.equal(f.doc.activeElement,f.status);assert(f.status.scrolled);}],
 ['retained enabled focus is never moved',()=>{const f=fixture();f.flush();assert.equal(f.doc.activeElement,f.button);}],
 ['newer outside focus wins',()=>{const f=fixture();f.disable();f.outside.focus();f.flush();assert.equal(f.doc.activeElement,f.outside);}],
 ['same-ID root replacement is refused',()=>{const f=fixture();f.disable();f.doc.getElementById=()=>new f.Node('root');f.flush();assert.equal(f.doc.activeElement,f.body);assert.equal(f.listeners.size,0);}],
 ['changed owner stops old callbacks',()=>{const f=fixture();f.disable();f.root.dataset.owner='two';f.flush();assert.equal(f.doc.activeElement,f.body);}],
 ['current editor status owns disabled editor action',()=>{const f=fixture();const editor=new f.Node('scheduled-forward-edit',f.root),status=new f.Node('local-status',editor),save=new f.Node('save',editor);save.focus();save.disabled=true;f.doc.activeElement=f.body;f.flush();assert.equal(f.doc.activeElement,status);}],
 ['removed editor returns inventory heading rather than new editor',()=>{const f=fixture();const editor=new f.Node('scheduled-forward-edit',f.root),save=new f.Node('save',editor);save.focus();editor.isConnected=false;save.isConnected=false;f.doc.activeElement=f.body;const replacement=new f.Node('scheduled-forward-edit',f.root);new f.Node('local-status',replacement);f.flush();assert.equal(f.doc.activeElement,f.status);}],
 ['capture readonly editability loss has owned fallback',()=>{const f=fixture();f.button.attrs.contenteditable='false';f.doc.activeElement=f.body;f.flush();assert.equal(f.doc.activeElement,f.status);assert(f.observers[0].options.attributeFilter.includes('contenteditable'));}],
 ['hidden fallback never receives focus',()=>{const f=fixture();f.disable();f.status.hidden=true;f.flush();assert.equal(f.doc.activeElement,f.body);}],
 ['collapse can use explicit remaining close control',()=>{const f=fixture();f.disable();f.root.nodes=f.root.nodes.filter(n=>n!==f.status);const close=new f.Node('fallback',f.root);f.flush();assert.equal(f.doc.activeElement,close);}],
 ['later focus between mutation and frame is preserved',()=>{const f=fixture();f.disable();f.observers[0].fn();f.outside.focus();while(f.frames.length)f.frames.shift()();assert.equal(f.doc.activeElement,f.outside);}],
 ['mount replacement retires previous observer',()=>{const f=fixture();assert(mountScheduledForwardFocus(f.config,f.env));assert.equal(f.observers[0].closed,true);assert.equal(f.listeners.size,1);}],
];
for(const [name,test]of cases){test();console.log('PASS '+name);}
console.log(`${cases.length} actual focus helper groups passed`);
