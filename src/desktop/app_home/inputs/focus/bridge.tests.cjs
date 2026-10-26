const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
// Event/property double only: no browser caret, scrolling or OS activation claim.
const source=fs.readFileSync(__dirname+'/../focus.js','utf8');
function fixture(kind='singleline',flag=true,extra={}){
 const nodes=new Map(),observers=[],calls=[];let doc;
 class Node {
  constructor(id){this.id=id;this.dataset={};this.isConnected=true;this.disabled=false;this.readOnly=false;this.value='literal draft';this.selectionStart=1;this.selectionEnd=4;this.listeners=new Map();this.hidden=false;nodes.set(id,this);}
  addEventListener(k,f){this.listeners.set(k,[...(this.listeners.get(k)||[]),f]);}
  removeEventListener(k,f){this.listeners.set(k,(this.listeners.get(k)||[]).filter(x=>x!==f));}
  fire(k){for(const f of [...(this.listeners.get(k)||[])])f({type:k,target:this});}
  contains(n){return n===this||n.parentNode===this||n.parentNode?.parentNode===this;}
  appendChild(n){n.parentNode=this;this.child=n;}
  setAttribute(k,v){this[k]=v;}
  getClientRects(){return this.hidden?[]:[{}];}
  closest(){return null;}
  focus(...args){calls.push({id:this.id,args});if(this.disabled||this.failFocus)return;const old=doc.activeElement;if(old!==this){old?.fire('blur');doc.activeElement=this;this.fire('focus');}}
 }
 const home=new Node('app-home');home.dataset={visit:'1',revision:'2'};
 let host=new Node('host'),node=new Node('input');host.parentNode=home;node.parentNode=host;Object.assign(node,extra);
 let c={id:'input',instance:3,kind,flag,visit:1,revision:2,live:[3]};host.dataset.homeFocus=JSON.stringify(c);
 doc={activeElement:null,getElementById:id=>nodes.get(id),querySelectorAll:()=>doc.dialogs||[]};
 const ctx={window:{},document:doc,getComputedStyle:()=>({visibility:'visible'}),MutationObserver:class{constructor(fn){this.fn=fn;observers.push(this);}observe(){}disconnect(){this.stopped=true;}}};
 vm.createContext(ctx);vm.runInContext(source,ctx);
 let control=ctx.window.spHomeFocus.attach(c,node,host);
 const history=()=>ctx.window.__spHomeText.focus.get(c.instance);
 const update=patch=>{c={...c,...patch};host.dataset.homeFocus=JSON.stringify(c);control.update();};
 const away=()=>{const other=new Node('other');other.focus();return other;};
 const remount=(patch={})=>{node.isConnected=false;host.isConnected=false;host=new Node('host');node=new Node('input');host.parentNode=home;node.parentNode=host;c={...c,...patch};home.dataset.revision=String(c.revision);host.dataset.homeFocus=JSON.stringify(c);control=ctx.window.spHomeFocus.attach(c,node,host);return node;};
 return {ctx,doc,home,nodes,calls,history,update,away,remount,get node(){return node;},get host(){return host;},get control(){return control;},Node};
}
test('single-line mount focuses with default options and never writes draft or range',()=>{
 const f=fixture();assert.equal(f.doc.activeElement,f.node);assert.deepEqual(f.calls,[{id:'input',args:[]}]);assert.equal(f.node.value,'literal draft');assert.equal(f.node.selectionStart,1);assert.equal(f.node.selectionEnd,4);assert.equal(f.history().everFocused,true);
});
test('unchanged true after focus-away never steals focus; false-to-true rearms',()=>{
 const f=fixture(),other=f.away();f.update({});assert.equal(f.doc.activeElement,other);assert.equal(f.history().attempts,1);f.update({flag:false});assert.equal(f.doc.activeElement,other);f.update({flag:true});assert.equal(f.doc.activeElement,f.node);assert.equal(f.history().attempts,2);
});
test('manual focus while flag false records ever-focused history',()=>{
 const f=fixture('singleline',false);assert.equal(f.calls.length,0);f.node.focus();f.away();f.update({flag:true});assert.equal(f.history().attempts,1);f.away();f.update({});assert.equal(f.history().attempts,1);
});
test('single-line unsuccessful and disabled attempts retry only on explicit updates',()=>{
 const f=fixture('number',true,{failFocus:true});assert.equal(f.history().attempts,1);assert.equal(f.history().everFocused,false);f.control.update();assert.equal(f.history().attempts,2);f.node.failFocus=false;f.control.update();assert.equal(f.history().everFocused,true);
 const g=fixture('email',true,{disabled:true});assert.equal(g.history().lastReason,'disabled');assert.equal(g.calls.length,0);g.node.disabled=false;g.control.update();assert.equal(g.doc.activeElement,g.node);
});
test('multiline unchanged true never retries disabled mount or focus-away',()=>{
 const f=fixture('multiline',true,{disabled:true});f.node.disabled=false;f.control.update();assert.equal(f.calls.length,0);f.update({flag:false});f.update({flag:true});assert.equal(f.history().attempts,1);f.away();f.control.update();assert.equal(f.history().attempts,1);
});
test('multiline changed true while already focused does not call focus again',()=>{
 const f=fixture('multiline',false);f.node.focus();f.update({flag:true});assert.equal(f.history().attempts,0);assert.equal(f.calls.length,1);
});
test('same logical instance survives revision remount and Reset-like value replacement',()=>{
 const f=fixture();f.away();const n=f.remount({revision:4});assert.notEqual(f.doc.activeElement,n);assert.equal(f.history().attempts,1);assert.equal(f.history().everFocused,true);f.update({flag:false});f.update({flag:true});assert.equal(f.doc.activeElement,n);
});
test('new logical visit or reintroduced instance rearms and prunes old history',()=>{
 const f=fixture();f.away();const n=f.remount({instance:8,live:[8],revision:5});assert.equal(f.doc.activeElement,n);assert.equal(f.history().attempts,1);assert.equal(f.ctx.window.__spHomeText.focus.has(3),false);
});
test('stale owner or replaced node refuses delayed update',()=>{
 for(const retire of[f=>f.home.dataset.revision='9',f=>f.nodes.set('input',new f.Node('input')),f=>f.node.isConnected=false]){const f=fixture('singleline',false);retire(f);f.update({flag:true});assert.equal(f.calls.length,0);assert.equal(f.history().lastReason,'stale');}
});
test('stale delayed attachment cannot prune current history or replace its owner',()=>{
 const f=fixture();const h=f.history(),before=f.calls.length;
 const stale={id:'input',instance:99,kind:'singleline',flag:true,visit:1,revision:1,live:[99]};
 assert.equal(f.ctx.window.spHomeFocus.attach(stale,f.node,f.host),null);
 assert.equal(f.ctx.window.__spHomeText.focus.get(3),h);assert.equal(h.current(),true);assert.equal(f.calls.length,before);
});
test('lease blocks focus without changing readonly, draft, range or scheduling retry',()=>{
 const f=fixture('singleline',false),b=f.ctx.window.__spHomeText;b.activeLease={};f.node.readOnly=true;f.update({flag:true});assert.equal(f.history().lastReason,'lease');assert.equal(f.calls.length,0);b.activeLease=null;f.node.readOnly=false;assert.equal(f.calls.length,0);f.control.update();assert.equal(f.calls.length,1);assert.equal(f.node.value,'literal draft');assert.equal(f.node.selectionStart,1);
});
test('visible dialog and hidden node prevent focus; no timers are installed',()=>{
 const f=fixture('singleline',false);f.doc.dialogs=[new f.Node('dialog')];f.update({flag:true});assert.equal(f.history().lastReason,'dialog');assert.equal(f.calls.length,0);f.doc.dialogs=[];f.node.hidden=true;f.control.update();assert.equal(f.history().lastReason,'hidden');assert.equal(f.calls.length,0);
});
test('quiet focus attachment creates no dispatch entry and detach ignores old events',()=>{
 const f=fixture();assert.equal(f.ctx.window.__spHomeText.entries.size,0);f.control.detach();f.node.fire('blur');assert.equal(f.history().isFocused,false);assert.equal(f.history().current(),false);
});
test('actual dispatch bridge attaches focus without extra occurrence or value rewrite',()=>{
 const f=fixture('singleline',false);f.control.detach();
 const c={id:'real',instance:9,kind:'singleline',flag:true,visit:1,revision:2,live:[9]};
 f.host.dataset={homeTextMount:'10',nativeId:'real',homeFocus:JSON.stringify(c),locked:'false'};
 f.doc.createElement=tag=>new f.Node('created');
 const lookup=f.doc.getElementById;f.doc.getElementById=id=>id==='real'?f.host.child:lookup(id);
 vm.runInContext(fs.readFileSync(__dirname+'/../text/bridge.js','utf8'),f.ctx);
 const sent=[],control=f.ctx.mountHomeText({host:'host',mount:10,visit:1,revision:2,initial:'unchanged draft',focus:c},{send:e=>sent.push(e)});
 assert.equal(f.doc.activeElement,f.host.child);assert.equal(sent.length,1);assert.equal(sent[0].kind,'mounted');assert.equal(sent[0].value,'unchanged draft');
 control.ack({seq:1});assert.equal(f.ctx.window.__spHomeText.entries.get(10).acked,1);
 const history=f.ctx.window.__spHomeText.focus.get(9);assert.equal(history.mount,10);control.stop();assert.equal(history.current(),false);assert.equal(sent.at(-1).kind,'closed');
});


test('Time differs from singleline: denied focus and lease transitions never retry unchanged true',()=>{
 const f=fixture('time',true,{failFocus:true});assert.equal(f.history().attempts,1);f.node.failFocus=false;f.control.update();assert.equal(f.history().attempts,1);assert.notEqual(f.doc.activeElement,f.node);
 f.update({flag:false});f.node.readOnly=true;f.update({flag:true});assert.equal(f.history().lastReason,'lease');f.node.readOnly=false;f.control.update();assert.equal(f.history().attempts,1);
 f.update({flag:false});f.update({flag:true});assert.equal(f.doc.activeElement,f.node);assert.equal(f.history().attempts,2);
});
test('Time already focused flag update makes no extra imperative focus call',()=>{
 const f=fixture('time',false);f.node.focus();f.update({flag:true});assert.equal(f.calls.length,1);assert.equal(f.history().attempts,0);f.update({flag:false});assert.equal(f.doc.activeElement,f.node);
});
