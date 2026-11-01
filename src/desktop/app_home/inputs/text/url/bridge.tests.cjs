const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const source=fs.readFileSync(__dirname+'/bridge.js','utf8'),lease=fs.readFileSync(__dirname+'/../bridge.js','utf8');
// Listener/property double: no HTML url sanitizer, physical typing, IME or native focus proof.
function fixture(config={}){
 const nodes=new Map(),observers=[],sent=[];
 class Node{
  constructor(tag){this.tagName=tag.toUpperCase();this.listeners=new Map();this.dataset={};this.isConnected=true;this.value='';this.disabled=false;this.readOnly=false;this.validity={typeMismatch:false,badInput:false};}
  setAttribute(k,v){this[k]=v;}
  addEventListener(k,f,c=false){const a=this.listeners.get(k)||[];a.push({f,c:!!c});this.listeners.set(k,a);}
  removeEventListener(k,f){this.listeners.set(k,(this.listeners.get(k)||[]).filter(x=>x.f!==f));}
  appendChild(n){n.parentNode=this;this.node=n;}
  fire(k,x={}){const e={type:k,isTrusted:false,defaultPrevented:false,preventDefault(){this.defaultPrevented=true;},stopImmediatePropagation(){this.stopped=true;},...x};for(const {f}of [...(this.listeners.get(k)||[])].sort((a,b)=>b.c-a.c)){f(e);if(e.stopped)break;}return e;}
 }
 const home=new Node('section');home.dataset={visit:'1',revision:'2'};
 const host=new Node('span');host.dataset={homeTextMount:'9',nativeId:'url',locked:'false',urlLoading:'false',required:'true'};host.parentNode=home;
 home.contains=n=>n===host||n===host.node;nodes.set('app-home',home);nodes.set('host',host);
 const ctx={URL,window:{},document:{body:new Node('body'),createElement:t=>new Node(t),getElementById:id=>nodes.get(id)||(id==='url'?host.node:null)},MutationObserver:class{constructor(f){this.f=f;observers.push(this);}observe(){}disconnect(){this.closed=true;}}};
 vm.createContext(ctx);vm.runInContext(lease+'\n'+source,ctx);
 const control=ctx.mountHomeUrl({host:'host',mount:9,visit:1,revision:2,policy:{local:'https://Initial.test/Path',enter:true,character:true,...config}},{send:e=>sent.push(JSON.parse(JSON.stringify(e)))});
 control.ack({seq:1});const node=host.node;
 const ack=()=>control.ack({seq:sent.at(-1).seq});
 const expected=()=>({visit:1,revision:2,fields:[{id:'url',value:node.value,mount:9,seq:sent.at(-1).seq,url:true}]});
 return{ctx,home,host,node,sent,control,ack,expected,notify(){for(const o of observers)if(!o.closed)o.f();}};
}
test('URL literal validation, clear and native attributes preserve the exact callback value',()=>{
 const f=fixture();assert.equal(f.node.type,'url');assert.equal(f.node['aria-required'],'false');
 for(const name of ['min','max','required','pattern','minLength','maxLength'])assert.equal(f.node[name],undefined,name);
 assert.equal(f.sent[0].callback,false);assert.equal(f.sent[0].local,'https://Initial.test/Path');
 for(const [value,valid]of [['Example.test/Path',true],['HTTP://Example.test/Case',true],['http://[',true],['bad host',false],['',true]]){
  f.node.value=value;f.node.validity.typeMismatch=true;f.node.fire('input');
  assert.equal(f.sent.at(-1).callback,true);assert.equal(f.sent.at(-1).callbackValue,value);assert.equal(f.sent.at(-1).valid,valid);f.ack();
 }
});
test('badInput and composition are not an empty clear and keep lease admission blocked',()=>{
 const f=fixture();f.node.value='';f.node.validity.badInput=true;f.node.fire('input');f.ack();
 assert.equal(f.sent.at(-1).callback,false);assert.equal(f.sent.at(-1).local,'https://Initial.test/Path');assert.equal(f.ctx.lockHomeText(f.expected()),null);
 f.node.validity.badInput=false;f.node.fire('input');assert.equal(f.sent.at(-1).callback,true);assert.equal(f.sent.at(-1).callbackValue,'');assert.equal(f.sent.at(-1).valid,true);f.ack();
 let release=f.ctx.lockHomeText(f.expected());assert.equal(typeof release,'function');release();
 f.node.fire('compositionstart');f.ack();f.node.value='https://typed.test';f.node.fire('input',{isComposing:true});f.ack();assert.equal(f.sent.at(-1).callback,false);assert.equal(f.ctx.lockHomeText(f.expected()),null);
 f.node.fire('compositionend');f.ack();assert.equal(f.sent.at(-1).callbackValue,'https://typed.test');release=f.ctx.lockHomeText(f.expected());assert.equal(typeof release,'function');release();
});
test('original node, exact sequence/ack and independent URL loading govern the complete lease',()=>{
 const f=fixture();f.node.value='unseen.test';assert.equal(f.ctx.lockHomeText(f.expected()),null);f.node.fire('input');assert.equal(f.ctx.lockHomeText(f.expected()),null);f.ack();
 f.host.dataset.urlLoading='true';f.notify();const release=f.ctx.lockHomeText(f.expected());assert.equal(typeof release,'function');assert.equal(f.node.disabled,true);release();
 f.host.dataset.locked='true';f.notify();assert.equal(f.ctx.lockHomeText(f.expected()),null);
 const g=fixture();const expected=g.expected();g.node.type='text';assert.equal(g.ctx.lockHomeText(expected),null);g.node.type='url';g.host.node={...g.node};assert.equal(g.ctx.lockHomeText(expected),null);
});
test('lease blocks edits and Enter while unconfigured Enter, blur and focus stay silent',()=>{
 const f=fixture({enter:false,character:false});assert.equal(f.node.fire('keydown',{key:'Enter'}).defaultPrevented,false);f.node.fire('blur');assert.equal(f.sent.length,1);
 const g=fixture();const release=g.ctx.lockHomeText(g.expected()),count=g.sent.length;assert.equal(typeof release,'function');g.node.fire('input');assert.equal(g.node.fire('keydown',{key:'Enter'}).defaultPrevented,true);assert.equal(g.node.fire('keydown',{key:'Tab'}).defaultPrevented,false);assert.equal(g.sent.length,count);release();
 g.node.fire('keydown',{key:'Enter'});assert.equal(g.sent.at(-1).callback,true);assert.equal(g.sent.at(-1).prevented,true);
});
test('stale root and close retire old listeners and never publish a successor occurrence',()=>{
 const f=fixture();f.home.dataset.revision='3';f.notify();assert.equal(f.sent.at(-1).kind,'closed');const count=f.sent.length;f.node.fire('input');f.node.fire('keydown',{key:'Enter'});assert.equal(f.sent.length,count);assert.equal(f.ctx.lockHomeText(f.expected()),null);
});
