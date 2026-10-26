const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const source=fs.readFileSync(__dirname+'/bridge.js','utf8'),lease=fs.readFileSync(__dirname+'/../bridge.js','utf8');
// Listener/property double: no HTML email sanitizer, physical typing, IME or native focus proof.
function fixture(config={}){
 const nodes=new Map(),observers=[],sent=[];
 class Node{
  constructor(tag){this.tagName=tag.toUpperCase();this.listeners=new Map();this.dataset={};this.isConnected=true;this.value='';this.disabled=false;this.readOnly=false;this.validity={typeMismatch:false};}
  setAttribute(k,v){this[k]=v;}
  addEventListener(k,f,c=false){const a=this.listeners.get(k)||[];a.push({f,c:!!c});this.listeners.set(k,a);}
  removeEventListener(k,f){this.listeners.set(k,(this.listeners.get(k)||[]).filter(x=>x.f!==f));}
  appendChild(n){n.parentNode=this;this.node=n;}
  fire(k,x={}){const e={type:k,isTrusted:false,defaultPrevented:false,preventDefault(){this.defaultPrevented=true;},stopImmediatePropagation(){this.stopped=true;},...x};for(const {f}of [...(this.listeners.get(k)||[])].sort((a,b)=>b.c-a.c)){f(e);if(e.stopped)break;}return e;}
 }
 const home=new Node('section');home.dataset={visit:'1',revision:'2'};
 const host=new Node('span');host.dataset={homeTextMount:'9',nativeId:'email',locked:'false',emailLoading:'false',required:'true'};host.parentNode=home;
 home.contains=n=>n===host||n===host.node;nodes.set('app-home',home);nodes.set('host',host);
 const ctx={window:{},document:{body:new Node('body'),createElement:t=>new Node(t),getElementById:id=>nodes.get(id)||(id==='email'?host.node:null)},MutationObserver:class{constructor(f){this.f=f;observers.push(this);}observe(){}disconnect(){this.closed=true;}}};
 vm.createContext(ctx);vm.runInContext(lease+'\n'+source,ctx);
 const control=ctx.mountHomeText({host:'host',mount:9,visit:1,revision:2,email:{local:'Initial@Example.com',enter:true,character:true,...config}},{send:e=>sent.push(JSON.parse(JSON.stringify(e)))});
 control.ack({seq:1});const node=host.node;
 const ack=()=>control.ack({seq:sent.at(-1).seq});
 const expected=()=>({visit:1,revision:2,fields:[{id:'email',value:node.value,mount:9,seq:sent.at(-1).seq,email:true}]});
 return{ctx,home,host,node,sent,control,ack,expected,notify(){for(const o of observers)if(!o.closed)o.f();}};
}
test('actual email attributes and default placeholder; no invented native validation attributes',()=>{
 const f=fixture();assert.equal(f.node.type,'email');assert.equal(f.node.autocomplete,'off');assert.equal(f.node.spellcheck,false);assert.equal(f.node.placeholder,'Enter an email address');assert.equal(f.node['aria-required'],'true');assert.equal(f.node.min,'0');assert.equal(f.node.max,'0');
 for(const p of ['required','pattern','inputMode','step','multiple','minLength','maxLength'])assert.equal(f.node[p],undefined,p);
 assert.equal(f.sent[0].email.callback,false);assert.equal(f.sent[0].email.local,'Initial@Example.com');
});
test('every change forwards literal including equal and invalid method controls',()=>{
 const f=fixture();for(const value of ['MiXeD@Example.com','MiXeD@Example.com','bad','', ' x@example.com ', 'x\n@example.com']){f.node.value=value;f.node.fire('input');assert.equal(f.sent.at(-1).email.callback,true);assert.equal(f.sent.at(-1).email.local,value);assert.equal(f.sent.at(-1).email.callbackValue,value);f.ack();}
 assert.equal(f.sent.length,7);
});
test('unconfigured Enter and blur/focus are silent; configured Enter keeps literal/modifiers/repeat',()=>{
 const f=fixture({enter:false,character:false});assert.equal(f.node.fire('keydown',{key:'Enter'}).defaultPrevented,false);f.node.fire('blur');f.node.fire('focus');assert.equal(f.sent.length,1);
 const g=fixture();const e=g.node.fire('keydown',{key:'Enter',shiftKey:true,ctrlKey:true,repeat:true});assert.equal(e.defaultPrevented,true);assert.equal(g.sent.at(-1).email.callbackValue,'Initial@Example.com');assert.equal(g.sent.at(-1).repeat,true);
});
test('source placeholder/loading updates do not synchronize local or invoke callback',()=>{
 const f=fixture();f.node.value='draft@example.com';f.node.fire('input');f.ack();const count=f.sent.length;
 f.host.dataset.placeholder='Work address';f.host.dataset.emailLoading='true';f.notify();assert.equal(f.node.placeholder,'Work address');assert.equal(f.node.value,'draft@example.com');assert.equal(f.node.disabled,true);f.node.fire('input');assert.equal(f.sent.length,count);
 f.host.dataset.emailLoading='false';f.notify();assert.equal(f.node.disabled,false);
});
test('typeMismatch is diagnostic and does not veto exact acknowledged sibling custody',()=>{
 const f=fixture();f.node.value='bad';f.node.validity.typeMismatch=true;f.node.fire('input');assert.equal(f.sent.at(-1).email.typeMismatch,true);f.ack();const release=f.ctx.lockHomeText(f.expected());assert.equal(typeof release,'function');release();
});
test('loading-disabled email can be attested but genuine host lock refuses',()=>{
 const f=fixture();f.host.dataset.emailLoading='true';f.notify();const release=f.ctx.lockHomeText(f.expected());assert.equal(typeof release,'function');release();assert.equal(f.node.disabled,true);
 f.host.dataset.locked='true';f.notify();assert.equal(f.ctx.lockHomeText(f.expected()),null);
});
test('unseen DOM and missing acknowledgement refuse exact admission',()=>{
 const f=fixture();f.node.value='unseen@example.com';assert.equal(f.ctx.lockHomeText(f.expected()),null);f.node.fire('input');assert.equal(f.ctx.lockHomeText(f.expected()),null);f.ack();const release=f.ctx.lockHomeText(f.expected());assert.equal(typeof release,'function');release();
});
test('readonly lease guards edits/Enter, permits Tab, releases exact token twice safely',()=>{
 const f=fixture();const release=f.ctx.lockHomeText(f.expected()),n=f.sent.length;assert.equal(f.node.readOnly,true);f.node.fire('input');assert.equal(f.node.fire('keydown',{key:'Enter'}).defaultPrevented,true);assert.equal(f.node.fire('keydown',{key:'Tab'}).defaultPrevented,false);assert.equal(f.sent.length,n);release();release();assert.equal(f.node.readOnly,false);
 const next=f.ctx.lockHomeText(f.expected());release();assert.equal(f.node.readOnly,true);next();assert.equal(f.node.readOnly,false);
});
test('composition is acknowledged safety; no Enter/callback while incomplete',()=>{
 const f=fixture();f.node.fire('compositionstart');f.ack();f.node.value='x@example.com';f.node.fire('input');assert.equal(f.sent.at(-1).email.callback,false);f.ack();assert.equal(f.ctx.lockHomeText(f.expected()),null);const n=f.sent.length;f.node.fire('keydown',{key:'Enter',isComposing:true});assert.equal(f.sent.length,n);f.node.fire('compositionend');assert.equal(f.sent.at(-1).email.callback,true);f.ack();const release=f.ctx.lockHomeText(f.expected());assert.equal(typeof release,'function');release();
});
test('stale source and replaced node cannot produce events or restore old lease ownership',()=>{
 const f=fixture();const release=f.ctx.lockHomeText(f.expected());f.home.dataset.revision='3';const count=f.sent.length;f.node.fire('input');assert.equal(f.sent.length,count);release();assert.equal(f.ctx.window.__spHomeText.activeLease,null);assert.equal(f.node.readOnly,true);f.notify();assert.equal(f.sent.at(-1).kind,'closed');assert.equal(f.ctx.lockHomeText(f.expected()),null);
});
