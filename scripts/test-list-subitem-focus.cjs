// Executes the actual production helper, with deterministic observer/timer scheduling.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const source=fs.readFileSync(path.join(__dirname,'../src/desktop/view/slack_lists/subitem_focus.js'),'utf8');
function setup(){
 const listeners=new Map(),observers=[],timers=new Map();let id=0,click;
 const document={body:{},activeElement:null,addEventListener(k,f){if(!listeners.has(k))listeners.set(k,new Set());listeners.get(k).add(f)},removeEventListener(k,f){listeners.get(k)?.delete(f)}};
 const emit=(k,e)=>{for(const f of [...(listeners.get(k)||[])])f(e)};
 const element=(data={})=>({isConnected:true,disabled:false,dataset:data,focus(){document.activeElement=this;emit('focusin',{target:this})},scrollIntoView(){this.scrolled=true},closest(){return dialog}});
 const dialog=element({dialogGeneration:'4'}),detail=element({detailOpenGeneration:'5'}),root=element({subitemSelection:'6',subitemBusy:'false'}),button=element(),status=element(),next=element(),other=element();
 let editor=false,present=false;
 detail.closest=()=>dialog;button.closest=()=>button;
 root.addEventListener=(_k,f,c)=>{assert.equal(c,true);click=f};root.removeEventListener=()=>{click=null};
 root.querySelector=s=>s==='#slack-list-subitem-status'?(present?status:null):(!next.disabled&&present?next:null);
 dialog.querySelector=()=>editor?other:null;
 document.getElementById=k=>k==='slack-list-record-detail'?detail:k==='slack-list-subitems'?root:null;
 class Observer{constructor(fn){this.fn=fn;this.live=false;observers.push(this)}observe(){this.live=true}disconnect(){this.live=false}}
 const ctx=vm.createContext({document,MutationObserver:Observer,setTimeout(fn){timers.set(++id,fn);return id},clearTimeout(n){timers.delete(n)}});vm.runInContext(source,ctx);ctx.mountListSubitems(5,4);document.activeElement=button;
 const mutate=()=>{for(const o of observers)if(o.live)o.fn()};
 return {document,dialog,detail,root,button,status,next,other,timers,observers,emit,mutate,arm(){click({target:button})},busy(){present=true;root.dataset.subitemBusy='true';button.disabled=true;mutate()},done(){root.dataset.subitemBusy='false';mutate()},editor(){editor=true;mutate()},unmount(){root.isConnected=false;detail.isConnected=false;document.activeElement=document.body;mutate()},expire(){for(const fn of [...timers.values()])fn()},stopped(){assert.equal(timers.size,0);assert([...listeners.values()].every(s=>s.size===0));assert.equal(observers.filter(o=>o.live).length,root.isConnected?1:0)}};
}
test('capture arms before disabling; held work retains status focus and completion focuses action',()=>{const h=setup();h.arm();h.busy();assert.equal(h.document.activeElement,h.status);assert(h.status.scrolled);h.done();assert.equal(h.document.activeElement,h.next);assert(h.next.scrolled);h.stopped()});
test('enabled action never gets focus stolen before pending transition',()=>{const h=setup();h.arm();h.mutate();assert.equal(h.document.activeElement,h.button);h.expire();h.stopped()});
test('trusted keyboard or pointer movement cancels pending focus authority',()=>{for(const type of ['keydown','pointerdown']){const h=setup();h.arm();h.busy();h.emit(type,{isTrusted:true});h.done();assert.equal(h.document.activeElement,h.status);h.stopped()}});
test('explicit control focus is retained',()=>{const h=setup();h.arm();h.busy();h.other.focus();h.done();assert.equal(h.document.activeElement,h.other);h.stopped()});
test('changed visit, mount, selection and selection ABA refuse focus',()=>{for(const change of ['visit','mount','selection','aba']){const h=setup();h.arm();h.busy();if(change==='visit')h.dialog.dataset.dialogGeneration='9';if(change==='mount')h.detail.dataset.detailOpenGeneration='9';if(change==='selection'||change==='aba')h.root.dataset.subitemSelection='9';h.mutate();if(change==='aba')h.root.dataset.subitemSelection='6';h.done();assert.equal(h.document.activeElement,h.status);h.stopped()}});
test('new cell editor cancels pending restoration',()=>{const h=setup();h.arm();h.busy();h.editor();h.done();assert.equal(h.document.activeElement,h.status);h.stopped()});
test('disabled completion target never enabled by focus helper and expires',()=>{const h=setup();h.arm();h.busy();h.next.disabled=true;h.done();assert.equal(h.document.activeElement,h.status);h.expire();assert(h.next.disabled);h.stopped()});
test('actual unmount removes both transaction and mount observers',()=>{const h=setup();h.arm();h.busy();h.unmount();h.stopped()});
test('deadline removes pending listeners without moving user focus',()=>{const h=setup();h.arm();h.busy();h.expire();h.done();assert.equal(h.document.activeElement,h.status);h.stopped()});

function recordMount(returnChild='CHILD'){
 const frames=[],listeners=new Map();let observer;
 const document={body:{},activeElement:null,addEventListener(k,f){if(!listeners.has(k))listeners.set(k,new Set());listeners.get(k).add(f)},removeEventListener(k,f){listeners.get(k)?.delete(f)}};
 const element=props=>Object.assign({isConnected:true,dataset:{},disabled:false,addEventListener(){},removeEventListener(){},closest(){return null},contains(e){return e===this||e?.parent===this},focus(){document.activeElement=this},scrollIntoView(){this.scrolled=true}},props);
 const dialog=element({dataset:{dialogGeneration:'4'}}),panel=element({dataset:{detailOpenGeneration:'5'}}),back=element({parent:panel}),other=element({dataset:{subitemId:'OTHER'},parent:panel}),child=element({dataset:{subitemId:'CHILD'},parent:panel}),user=element({});
 document.panel=panel;document.getElementById=()=>document.panel;document.activeElement=document.body;
 panel.closest=()=>dialog;panel.querySelectorAll=s=>s==='[data-subitem-id]'?[other,child]:[];panel.querySelector=()=>back;dialog.querySelector=()=>null;
 const ctx=vm.createContext({document,MutationObserver:class{constructor(fn){observer=fn}observe(){}disconnect(){}},requestAnimationFrame:f=>frames.push(f),setTimeout:()=>1,clearTimeout(){}});
 vm.runInContext(fs.readFileSync(path.join(__dirname,'../src/desktop/view/slack_lists/record_keyboard.js'),'utf8'),ctx);ctx.mountListRecordDetail(5,4,'PARENT',returnChild);
 return {document,dialog,panel,back,other,child,user,frame(){for(const f of frames.splice(0))f()},input(){for(const f of listeners.get('keydown')||[])f({isTrusted:true})},stopped(){assert([...listeners.values()].every(s=>s.size===0))}};
}
test('actual parent mount focuses exact returned child among siblings',()=>{const h=recordMount();h.frame();assert.equal(h.document.activeElement,h.child);assert(h.child.scrolled);h.stopped()});
test('missing or disabled returned child uses Back without enabling it',()=>{for(const absent of [false,true]){const h=recordMount(absent?'MISSING':'CHILD');h.child.disabled=true;h.frame();assert.equal(h.document.activeElement,h.back);assert(h.child.disabled);h.stopped()}});
test('parent mount refuses changed visit or replaced node',()=>{for(const changed of ['visit','node']){const h=recordMount();if(changed==='visit')h.dialog.dataset.dialogGeneration='9';else h.document.panel=h.user;h.frame();assert.equal(h.document.activeElement,h.document.body);h.stopped()}});
test('parent return preserves trusted input and explicit newer focus',()=>{for(const kind of ['input','focus']){const h=recordMount();if(kind==='input')h.input();else h.user.focus();h.frame();assert.notEqual(h.document.activeElement,h.child);h.stopped()}});
test('ordinary detail mount remains Back-focused',()=>{const h=recordMount(null);h.frame();assert.equal(h.document.activeElement,h.back);h.stopped()});
