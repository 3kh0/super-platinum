const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const source=fs.readFileSync(process.env.SP_HOME_TEXT_BRIDGE_SOURCE||__dirname+'/bridge.js','utf8');
// This is a listener/property double, not a browser focus or editing model.
function fixture(mac=false,multiline=false){
 const nodes=new Map(),observers=[],mutations=[];
 let document;
 class Node{
  constructor(tag){this.tagName=tag.toUpperCase();this.listeners=new Map();this.dataset={};this.isConnected=true;this._value='';this.disabled=false;this.readOnly=false;this.selectionStart=1;this.selectionEnd=3;this.selectionDirection='backward';}
  get value(){return this._value;}
  set value(v){mutations.push(['value',this,v]);this._value=v;}
  setAttribute(k,v){this[k]=v;}
  addEventListener(k,f,options){const capture=options===true||!!options?.capture;const list=this.listeners.get(k)||[];list.push({f,capture});this.listeners.set(k,list);}
  removeEventListener(k,f,options){const capture=options===true||!!options?.capture;const list=(this.listeners.get(k)||[]).filter(x=>x.f!==f||x.capture!==capture);if(list.length)this.listeners.set(k,list);else this.listeners.delete(k);}
  appendChild(n){n.parentNode=this;this.node=n;}
  contains(n){return n===this||n===this.node||n.parentNode===this;}
  focus(){mutations.push(['focus',this]);document.activeElement=this;}
  setSelectionRange(a,b,d){mutations.push(['selection',this]);this.selectionStart=a;this.selectionEnd=b;this.selectionDirection=d;}
  fire(k,extra={}){const e={type:k,target:this,isTrusted:false,defaultPrevented:false,cancelable:true,preventDefault(){if(this.cancelable)this.defaultPrevented=true;},stopPropagation(){this.propagationStopped=true;},stopImmediatePropagation(){this.immediateStopped=true;this.propagationStopped=true;},...extra};for(const x of [...(this.listeners.get(k)||[])].sort((a,b)=>Number(b.capture)-Number(a.capture))){x.f(e);if(e.immediateStopped)break;}return e;}
 }
 const home=new Node('section');home.dataset={visit:'1',revision:'2'};
 const host=new Node('span');host.dataset={homeTextMount:'9',nativeId:'text',locked:'false'};host.parentNode=home;
 home.contains=n=>n===host||n===host.node||n.parentNode===home;
 nodes.set('host',host);nodes.set('app-home',home);
 const sent=[];document={body:new Node('body'),activeElement:null,getElementById:id=>nodes.has(id)?nodes.get(id):id==='text'?host.node:null,createElement:tag=>new Node(tag)};
 const ctx={window:{},document,MutationObserver:class{constructor(callback){this.callback=callback;observers.push(this);}observe(){}disconnect(){this.closed=true;}},console};vm.createContext(ctx);vm.runInContext(source,ctx);
 const control=ctx.mountHomeText({host:'host',mount:9,visit:1,revision:2,initial:' raw ',multiline,mac},{send:e=>sent.push(e)});
 control.ack({seq:1});const node=host.node;document.activeElement=node;mutations.length=0;
 const expected=()=>({visit:1,revision:2,fields:[{id:'text',value:node.value,mount:9,seq:ctx.window.__spHomeText.entries.get(9).seq}]});
 const quiet=()=>{const n=new Node('input');n.id='quiet';n.parentNode=home;n._value='quiet literal';nodes.set('quiet',n);return n;};
 return{ctx,node,host,home,sent,control,expected,nodes,quiet,mutations,notify(){for(const o of observers)if(!o.closed)o.callback([]);}};
}

test('actual listener captures literal Enter with original repeat and composition, prevents default',()=>{
 const f=fixture();const e=f.node.fire('keydown',{key:'Enter',code:'Enter',repeat:true,isComposing:true});
 assert.equal(e.defaultPrevented,true);assert.equal(f.sent.at(-1).value,' raw ');assert.equal(f.sent.at(-1).repeat,true);assert.equal(f.sent.at(-1).composing,true);assert.equal(f.sent.at(-1).trusted,false);
});
test('qUwD modifier behavior remains platform specific',()=>{
 for(const mac of[false,true]){const f=fixture(mac);for(const [mods,dispatch]of[[{shiftKey:true},false],[{altKey:true},false],[{ctrlKey:true},false],[{ctrlKey:true,shiftKey:true},!mac],[{metaKey:true,ctrlKey:true,shiftKey:true},true]]){const count=f.sent.length;const e=f.node.fire('keydown',{key:'Enter',...mods});assert.equal(f.sent.length-count,dispatch?1:0);assert.equal(!!e.defaultPrevented,dispatch);}}
});
test('unacknowledged occurrence cannot obtain final text lease',()=>{
 const f=fixture();f.node.value='new';f.node.fire('input');assert.equal(f.ctx.lockHomeText(f.expected()),null);f.control.ack({seq:2});assert.equal(typeof f.ctx.lockHomeText(f.expected()),'function');
});
test('busy lease is distinct from mismatch and release permits next occurrence',()=>{
 const f=fixture();const release=f.ctx.lockHomeText(f.expected());assert.equal(f.node.disabled,false);assert.equal(f.node.readOnly,true);assert.equal(f.ctx.lockHomeText(f.expected()),'busy');release();assert.equal(f.node.disabled,false);assert.equal(f.node.readOnly,false);f.node.fire('keydown',{key:'Enter'});assert.equal(f.sent.at(-1).kind,'enter');f.control.ack({seq:2});const next=f.ctx.lockHomeText(f.expected());assert.equal(typeof next,'function');next();
});
test('replaced node or Home revision refuses event and lease',()=>{
 const f=fixture();const count=f.sent.length;f.home.dataset.revision='3';f.node.fire('keydown',{key:'Enter'});assert.equal(f.sent.length,count);assert.equal(f.ctx.lockHomeText(f.expected()),null);
 const g=fixture();g.node.parentNode=null;g.node.fire('input');assert.equal(g.sent.length,1);assert.equal(g.ctx.lockHomeText(g.expected()),null);
});
test('cleanup removes original listeners before closed receipt',()=>{
 const f=fixture();f.control.stop();assert.equal(f.sent.at(-1).kind,'closed');const count=f.sent.length;f.node.fire('keydown',{key:'Enter'});assert.equal(f.sent.length,count);assert.equal(f.node.listeners.size,0);
});


test('short custody changes only readonly, never focus/value/selection, for input and textarea',()=>{
 for(const multiline of [false,true]){
  const f=fixture(false,multiline),n=f.node;
  const before=[n.value,n.selectionStart,n.selectionEnd,n.selectionDirection];
  const release=f.ctx.lockHomeText(f.expected());
  assert.equal(typeof release,'function');assert.equal(n.readOnly,true);assert.equal(n.disabled,false);
  assert.equal(f.ctx.document.activeElement,n);assert.deepEqual([n.value,n.selectionStart,n.selectionEnd,n.selectionDirection],before);
  f.notify();assert.equal(n.readOnly,true);assert.equal(n.disabled,false);
  release();assert.equal(n.readOnly,false);assert.deepEqual(f.mutations,[]);
  assert.deepEqual([n.value,n.selectionStart,n.selectionEnd,n.selectionDirection],before);
 }
});

test('held Enter/input/beforeinput do not escape; Tab and selection navigation remain available',()=>{
 const f=fixture(),release=f.ctx.lockHomeText(f.expected()),count=f.sent.length;
 for(const type of ['beforeinput','input'])assert.equal(f.node.fire(type,{data:'x',inputType:'insertText'}).defaultPrevented,true);
 assert.equal(f.node.fire('keydown',{key:'Enter'}).defaultPrevented,true);
 for(const key of ['Tab','ArrowLeft','ArrowRight','Home','End'])assert.equal(f.node.fire('keydown',{key,shiftKey:true}).defaultPrevented,false);
 assert.equal(f.sent.length,count);release();
 f.node.fire('keydown',{key:'Enter'});assert.equal(f.sent.length,count+1);assert.equal(f.sent.at(-1).kind,'enter');
});

test('quiet co-lease protects existing handlers and restores prior readonly without disabling',()=>{
 for(const prior of [false,true]){
  const f=fixture(),q=f.quiet();q.readOnly=prior;
  let calls=0;for(const k of ['input','beforeinput','keydown'])q.addEventListener(k,()=>calls++);
  const expected=f.expected();expected.fields.push({id:'quiet',value:q.value,mount:null,seq:null});
  const release=f.ctx.lockHomeText(expected);assert.equal(typeof release,'function');assert.equal(q.readOnly,true);assert.equal(q.disabled,false);
  q.fire('input');q.fire('beforeinput');q.fire('keydown',{key:'Enter'});assert.equal(calls,0);
  q.fire('keydown',{key:'Tab'});assert.equal(calls,1);
  release();assert.equal(q.readOnly,prior);q.fire('input');assert.equal(calls,2);
  assert.deepEqual(f.mutations,[]);
 }
});

test('release never steals explicit later focus or selection',()=>{
 const f=fixture(),q=f.quiet(),release=f.ctx.lockHomeText(f.expected());
 q.focus();f.node.setSelectionRange(0,2,'forward');f.mutations.length=0;
 release();assert.equal(f.ctx.document.activeElement,q);assert.deepEqual([f.node.selectionStart,f.node.selectionEnd,f.node.selectionDirection],[0,2,'forward']);assert.deepEqual(f.mutations,[]);
 const g=fixture(),r=g.ctx.lockHomeText(g.expected());g.ctx.document.activeElement=g.ctx.document.body;r();assert.equal(g.ctx.document.activeElement,g.ctx.document.body);assert.deepEqual(g.mutations,[]);
});

test('genuine disabled state survives release and refuses subsequent lock',()=>{
 const f=fixture(),release=f.ctx.lockHomeText(f.expected());f.host.dataset.locked='true';f.notify();
 assert.equal(f.node.disabled,true);assert.equal(f.node.readOnly,true);release();assert.equal(f.node.disabled,true);assert.equal(f.ctx.lockHomeText(f.expected()),null);
 const count=f.sent.length;f.node.fire('keydown',{key:'Enter'});assert.equal(f.sent.length,count);
});

test('old release is idempotent and cannot unlock a newer lease',()=>{
 const f=fixture(),first=f.ctx.lockHomeText(f.expected());first();
 const second=f.ctx.lockHomeText(f.expected());assert.equal(typeof second,'function');first();
 assert.equal(f.node.readOnly,true);assert.equal(f.ctx.lockHomeText(f.expected()),'busy');
 const count=f.sent.length;f.node.fire('keydown',{key:'Enter'});assert.equal(f.sent.length,count);
 second();assert.equal(f.node.readOnly,false);second();assert.equal(f.node.readOnly,false);
});

test('retired Home or same-ID replacement never receives stale release writes',()=>{
 for(const retire of [f=>{f.home.dataset.revision='3';},f=>{f.nodes.set('app-home',f.quiet());},f=>{const n=f.quiet();n.id='text';n.readOnly=true;f.nodes.set('text',n);}]){
  const f=fixture(),release=f.ctx.lockHomeText(f.expected());retire(f);f.mutations.length=0;release();
  assert.equal(f.node.readOnly,true);assert.deepEqual(f.mutations,[]);
  const current=f.ctx.document.getElementById('text');if(current!==f.node)assert.equal(current.readOnly,true);
 }
});

test('replaced native entry and disconnected quiet node keep stale release inert',()=>{
 const f=fixture(),q=f.quiet(),expected=f.expected();expected.fields.push({id:'quiet',value:q.value,mount:null,seq:null});
 const release=f.ctx.lockHomeText(expected),book=f.ctx.window.__spHomeText,original=book.entries.get(9);
 const replacement={...original,lease:{newer:true}};book.entries.set(9,replacement);q.isConnected=false;
 release();assert.equal(f.node.readOnly,true);assert.equal(q.readOnly,true);assert.equal(replacement.lease.newer,true);
 assert.deepEqual(f.mutations,[]);
});

// Quiet-only Home forms never call mountHomeText. Their first acquisition must
// establish the same shared lease book before any asynchronous calendar work.
function quietOnly(){
 const f=fixture();f.control.stop();delete f.ctx.window.__spHomeText;
 const q=f.quiet();f.mutations.length=0;
 return {...f,q,expected:()=>({visit:1,revision:2,fields:[{id:'quiet',value:q.value,mount:null,seq:null}]})};
}
test('quiet-only first acquisition serializes overlap and restores original readonly',()=>{
 for(const prior of [false,true]){
  const f=quietOnly();f.q.readOnly=prior;
  const first=f.ctx.lockHomeText(f.expected());assert.equal(typeof first,'function');
  assert.equal(f.q.readOnly,true);assert.equal(f.ctx.lockHomeText(f.expected()),'busy');
  let escaped=0;f.q.addEventListener('input',()=>escaped++);f.q.fire('input');assert.equal(escaped,0);
  first();assert.equal(f.q.readOnly,prior);f.q.fire('input');assert.equal(escaped,1);
  const second=f.ctx.lockHomeText(f.expected());assert.equal(typeof second,'function');
  first();assert.equal(f.q.readOnly,true);assert.equal(f.ctx.lockHomeText(f.expected()),'busy');
  second();second();assert.equal(f.q.readOnly,prior);assert.equal(f.q.disabled,false);
  assert.deepEqual(f.mutations,[]);
 }
});
test('quiet-only release cannot restore a retired owner or replacement node',()=>{
 for(const retire of [f=>{f.home.dataset.revision='3';},f=>{const n=f.quiet();n.readOnly=true;f.nodes.set('quiet',n);}]){
  const f=quietOnly(),first=f.ctx.lockHomeText(f.expected());retire(f);first();first();
  assert.equal(f.q.readOnly,true);assert.equal(f.ctx.window.__spHomeText.activeLease,null);
  assert.equal(f.ctx.document.getElementById('quiet').readOnly,true);assert.deepEqual(f.mutations,[]);
 }
});
test('quiet-only unseen value refuses before lease and cannot block a later exact acquisition',()=>{
 const f=quietOnly(),expected=f.expected();f.q._value='unreported';
 assert.equal(f.ctx.lockHomeText(expected),null);assert.equal(f.q.readOnly,false);
 const release=f.ctx.lockHomeText(f.expected());assert.equal(typeof release,'function');
 assert.equal(f.ctx.lockHomeText(f.expected()),'busy');release();assert.equal(f.q.readOnly,false);
});
