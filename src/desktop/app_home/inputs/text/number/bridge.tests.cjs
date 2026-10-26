const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const source=fs.readFileSync(__dirname+'/bridge.js','utf8'),leaseSource=fs.readFileSync(__dirname+'/../bridge.js','utf8');
const pure={};vm.createContext(pure);vm.runInContext(source,pure);
const transition=(c,l,k,v='')=>JSON.parse(JSON.stringify(pure.homeNumberTransition(c,l,k,v)));
test('actual retained direct-change vectors match local and callback projection (method-only)',()=>{
 const path=require('node:path');
 const vectors=JSON.parse(fs.readFileSync(path.resolve(__dirname,'../../../../../../tmp/app-home-number-input-20261025/reference/vectors.json'),'utf8')).vectors;
 const rows=vectors.filter(v=>v.name.startsWith('method-change-'));assert.ok(rows.length>=20);
 for(const row of rows){const m=/^method-change-(true|false)-(.*)$/.exec(row.name),raw=JSON.parse(m[2]);
  const result=transition({decimal:m[1]==='true',dispatch:true},'3','input',raw);
  assert.equal(result.local,row.local[0],row.name);assert.equal(result.value,row.parent.value,row.name);
  assert.equal(result.callback,row.events.length>0,row.name);
 }
});
// Listener/property double only: intentionally does not pretend to implement
// browser number sanitization, native stepper, focus, locale or physical IME.
function fixture(overrides={}){
 const nodes=new Map(),observers=[],sent=[];
 class Node{
  constructor(tag){this.tagName=tag.toUpperCase();this.listeners=new Map();this.dataset={};this.isConnected=true;this.value='';this.disabled=false;this.readOnly=false;this.validity={valid:true,badInput:false,stepMismatch:false,rangeUnderflow:false,rangeOverflow:false};}
  setAttribute(k,v){this[k]=v;}removeAttribute(k){delete this[k];}
  addEventListener(k,f,c=false){const a=this.listeners.get(k)||[];a.push({f,c:!!c});this.listeners.set(k,a);}
  removeEventListener(k,f){this.listeners.set(k,(this.listeners.get(k)||[]).filter(x=>x.f!==f));}
  appendChild(n){n.parentNode=this;this.node=n;}
  fire(k,extra={}){const e={type:k,key:null,isTrusted:false,defaultPrevented:false,preventDefault(){this.defaultPrevented=true;},stopImmediatePropagation(){this.stopped=true;},...extra};for(const {f}of [...(this.listeners.get(k)||[])].sort((a,b)=>b.c-a.c)){f(e);if(e.stopped)break;}return e;}
 }
 const home=new Node('section');home.dataset={visit:'1',revision:'2'};
 const host=new Node('span');host.dataset={homeTextMount:'9',nativeId:'number',locked:'false'};host.parentNode=home;
 home.contains=n=>n===host||n===host.node;nodes.set('app-home',home);nodes.set('host',host);
 const ctx={window:{},document:{body:new Node('body'),getElementById:id=>nodes.get(id)||(id==='number'?host.node:null),createElement:tag=>new Node(tag)},MutationObserver:class{constructor(cb){this.cb=cb;observers.push(this);}observe(){}disconnect(){this.stopped=true;}}};
 vm.createContext(ctx);vm.runInContext(leaseSource+'\n'+source,ctx);
 const config={decimal:true,dispatch:true,enter:true,character:true,local:'1.20',placeholder:'Enter a number',min:null,max:null,...overrides};
 const control=ctx.mountHomeText({host:'host',mount:9,visit:1,revision:2,number:config},{send:e=>sent.push(JSON.parse(JSON.stringify(e)))});
 const node=host.node,entry=ctx.window.__spHomeText.entries.get(9);
 const ack=()=>control.ack({seq:entry.seq});ack();
 const expected=()=>({visit:1,revision:2,fields:[{id:'number',mount:9,seq:entry.seq,value:entry.value,number:true}]});
 const input=v=>{node.value=v;node.fire('input');};
 return{ctx,node,home,host,entry,sent,control,ack,input,expected,notify(){for(const o of observers)if(!o.stopped)o.cb();}};
}
test('original canonical change vectors retain spelling separately from JS Number wire',()=>{
 for(const [raw,wire]of [['002.50','2.5'],['-0','0'],['1e3','1000'],['1e-2','0.01'],['+2','2']]){
  assert.deepEqual(transition({decimal:true,dispatch:true},'3','input',raw),{local:raw,callback:true,value:wire});
 }
});
test('integer-local rejection occurs after callback, and dispatching equality differs from quiet',()=>{
 assert.deepEqual(transition({decimal:false,dispatch:true},'3','input','1.20'),{local:'3',callback:true,value:'1.2'});
 assert.equal(transition({decimal:true,dispatch:true},'2','input','2').callback,false);
 assert.equal(transition({decimal:true,dispatch:false},'2','input','2').callback,true);
 assert.equal(transition({decimal:true,dispatch:true},null,'input','').callback,true);
});
test('blur exact decimal suffix normalization preserves exponent and pristine absence',()=>{
 for(const [local,value]of [['2.500','2.5'],['2.000','2'],['2.','2']])assert.deepEqual(transition({},local,'blur'),{local:value,callback:true,value});
 assert.equal(transition({},'1.20e10','blur').callback,false);
 assert.deepEqual(transition({},null,'blur'),{local:null,callback:false,value:null});
});
test('mount creates number without invented step or pattern and forwards bounds',()=>{
 const f=fixture({min:'-2',max:'10'});assert.equal(f.node.type,'number');assert.equal(f.node.min,'-2');assert.equal(f.node.max,'10');
 assert.equal(f.node.autocomplete,'off');for(const k of ['step','pattern','inputMode'])assert.equal(f.node[k],undefined);
 assert.equal(f.sent[0].number.local,'1.20');assert.equal(f.sent[0].number.callback,false);
});
test('input updates raw/local while callback normalizes, blur updates local again',()=>{
 const f=fixture();f.input('002.50');assert.equal(f.sent.at(-1).value,'002.50');assert.equal(f.sent.at(-1).number.callbackValue,'2.5');f.ack();
 f.node.fire('blur');assert.equal(f.node.value,'002.5');assert.equal(f.sent.at(-1).number.callbackValue,'002.5');
});
test('integer rejected display restoration does not lose normalized attempted callback',()=>{
 const f=fixture({decimal:false,local:'4'});f.input('1.20');assert.equal(f.node.value,'4');assert.equal(f.sent.at(-1).number.local,'4');assert.equal(f.sent.at(-1).number.callbackValue,'1.2');
});
test('Enter config and literal differ from plain-text modifier rules; IME remains guarded',()=>{
 const f=fixture();assert.equal(f.node.fire('keydown',{key:'Enter',shiftKey:true,ctrlKey:true}).defaultPrevented,true);assert.equal(f.sent.at(-1).number.callbackValue,'1.20');
 const n=f.sent.length;f.node.fire('keydown',{key:'Enter',isComposing:true});f.node.fire('keydown',{key:'Enter',keyCode:229});assert.equal(f.sent.length,n);
 const g=fixture({enter:false});assert.equal(g.node.fire('keydown',{key:'Enter'}).defaultPrevented,false);assert.equal(g.sent.length,1);
});
test('blank pristine blur observes without throwing or inventing a clear callback',()=>{
 const f=fixture({local:null});f.node.fire('blur');assert.equal(f.sent.at(-1).number.local,null);assert.equal(f.sent.at(-1).number.callback,false);assert.equal(f.node.value,'');
});
test('range and step mismatch do not veto exact acknowledged custody, bad input does',()=>{
 const f=fixture();f.node.validity={valid:false,badInput:false,stepMismatch:true,rangeUnderflow:true,rangeOverflow:false};f.input('2.50');f.ack();
 let release=f.ctx.lockHomeText(f.expected());assert.equal(typeof release,'function');release();
 f.node.validity.badInput=true;f.node.value='';f.node.fire('input');f.ack();assert.equal(f.sent.at(-1).number.callback,false);assert.equal(f.ctx.lockHomeText(f.expected()),null);
});
test('unseen and unacknowledged buffers refuse; readonly lease gates input/Enter but allows Tab',()=>{
 const f=fixture();f.node.value='3';assert.equal(f.ctx.lockHomeText(f.expected()),null);const count=f.sent.length;f.node.fire('keydown',{key:'Enter'});assert.equal(f.sent.length,count);
 f.node.fire('input');assert.equal(f.ctx.lockHomeText(f.expected()),null);f.ack();const release=f.ctx.lockHomeText(f.expected());assert.equal(typeof release,'function');assert.equal(f.node.readOnly,true);assert.equal(f.node.disabled,false);
 const n=f.sent.length;f.node.fire('input');assert.equal(f.node.fire('keydown',{key:'Enter'}).defaultPrevented,true);assert.equal(f.node.fire('keydown',{key:'Tab'}).defaultPrevented,false);assert.equal(f.sent.length,n);release();release();assert.equal(f.node.readOnly,false);
});
test('old source and old release token cannot regain authority',()=>{
 const f=fixture(),release=f.ctx.lockHomeText(f.expected());f.home.dataset.revision='3';f.notify();const n=f.sent.length;f.node.fire('input');assert.equal(f.sent.length,n);assert.equal(f.ctx.lockHomeText(f.expected()),null);release();assert.equal(f.node.readOnly,true);
});
test('composition has acknowledged safety state and cannot invoke callback until complete',()=>{
 const f=fixture();f.node.fire('compositionstart');f.ack();assert.equal(f.ctx.lockHomeText(f.expected()),null);f.node.value='2';f.node.fire('input');assert.equal(f.sent.at(-1).number.callback,false);
 f.node.fire('compositionend');f.ack();assert.equal(f.sent.at(-1).number.callbackValue,'2');const release=f.ctx.lockHomeText(f.expected());assert.equal(typeof release,'function');release();
});
