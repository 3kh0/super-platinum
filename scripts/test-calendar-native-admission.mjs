import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import path from 'node:path';
const root=process.argv[2]||path.resolve(path.dirname(new URL(import.meta.url).pathname.replace(/^\/(?:([A-Za-z]:))/, '$1')),'..');
const source=fs.readFileSync(path.join(root,'src/desktop/message_buttons/shortcuts/calendar/native.js'),'utf8');
let count=0;
function test(name,fn){fn();++count;console.log('PASS '+name);}
function fixture(){
 let observer;
 const nodes=[];
 const fieldset={disabled:false,dataset:{calendarEnabled:'true'}};
 const root={isConnected:true,dataset:{dialogGeneration:'7',calendarEditing:'true',calendarReady:'true'},listeners:{},
  addEventListener(k,f){this.listeners[k]=f;},getClientRects(){return [1];},
  contains(n){return n===fieldset||nodes.includes(n);},
  querySelectorAll(q){if(q==='#calendar-fields')return [fieldset];if(q==='#calendar-create')return nodes.filter(n=>n.id==='calendar-create');return q.includes('button')?nodes:nodes.filter(n=>n.type!=='button');}};
 const input=(id,type,value,disabled=false)=>{const n={id,type,value,disabled,dataset:{},validity:{valid:true,badInput:false},checked:false};nodes.push(n);return n;};
 const title=input('calendar-title','text','Original');
 const date=input('calendar-date','date','2026-10-18');
 const start=input('calendar-start','time','09:15');
 const end=input('calendar-end','time','10:00');
 const create=input('calendar-create','button','');
 const close=input('app-shortcut-close','button','');close.dataset.calendarDismiss='true';
 const cancel=input('calendar-cancel','button','');cancel.dataset.calendarDismiss='true';
 let roots=[root],dialogs=[root];
 const document={body:{},querySelectorAll:q=>q==='#app-shortcut-dialog'?roots:dialogs};
 const context=vm.createContext({document,MutationObserver:class{constructor(f){observer=f;}observe(){}disconnect(){}}});
 vm.runInContext(source,context);context.installCalendarAdmission(7);
 const expected={fields:Object.fromEntries(nodes.filter(n=>n.type!=='button').map(n=>[n.id,{kind:n.type,value:n.value,checked:null}]))};
 return {root,fieldset,nodes,title,date,start,end,create,close,cancel,expected,context,flush:()=>observer?.(),setRoots:x=>roots=x,setDialogs:x=>dialogs=x,lock:()=>context.lockCalendarCreate(7,expected)};
}
test('valid actual controls are held; Close and Cancel stay usable',()=>{const f=fixture(),release=f.lock();assert.equal(typeof release,'function');assert(f.fieldset.disabled);assert(f.create.disabled);assert(!f.close.disabled);assert(!f.cancel.disabled);assert.equal(f.lock(),null);release();assert(!f.fieldset.disabled);release();});
for(const field of ['title','date','start','end'])test('unprocessed '+field+' edit refuses stale snapshot',()=>{const f=fixture();f[field].value+='new';assert.equal(f.lock(),null);assert(!f.create.disabled);});
for(const valid of [true,false])test('badInput refuses even with matching empty serialized value '+valid,()=>{const f=fixture();f.start.value='';f.expected.fields['calendar-start'].value='';f.start.validity={valid,badInput:true};assert.equal(f.lock(),null);f.start.validity={valid:true,badInput:false};assert.equal(typeof f.lock(),'function');});
test('composition prevents capture until actual compositionend',()=>{const f=fixture();f.root.listeners.compositionstart({target:f.title});assert.equal(f.lock(),null);f.root.listeners.compositionend({target:f.title});assert.equal(typeof f.lock(),'function');});
test('missing, duplicate, extra and wrong-type controls refuse',()=>{for(const change of [f=>f.nodes.pop(),f=>f.nodes.push({...f.start}),f=>f.nodes.push({...f.start,id:'unexpected'}),f=>f.start.type='text']){const f=fixture();if(change.toString().includes('pop'))f.nodes.splice(f.nodes.indexOf(f.start),1);else change(f);assert.equal(f.lock(),null);}});
test('new top dialog, new generation, missing installation and duplicate root refuse',()=>{for(const change of [f=>f.setDialogs([f.root,{isConnected:true,getClientRects:()=>[1]}]),f=>f.root.dataset.dialogGeneration='8',f=>delete f.root.__calendarAdmission,f=>f.setRoots([f.root,f.root])]){const f=fixture();change(f);assert.equal(f.lock(),null);}});
test('rerender cannot reopen held controls and release respects Creating',()=>{const f=fixture(),release=f.lock();f.fieldset.disabled=false;f.flush();assert(f.fieldset.disabled);f.root.dataset.calendarEditing='false';f.fieldset.dataset.calendarEnabled='false';f.root.dataset.calendarReady='false';release();assert(f.fieldset.disabled);assert(f.create.disabled);assert(!f.cancel.disabled);});
test('dialog removal releases exact old lease without enabling newer controls',()=>{const f=fixture(),release=f.lock();f.root.isConnected=false;f.setRoots([]);f.flush();assert.equal(f.root.__calendarAdmission.lease,null);assert(f.fieldset.disabled);release();});
test('preexisting disabled fields remain disabled on refusal release',()=>{const f=fixture();f.end.disabled=true;const release=f.lock();release();assert(f.end.disabled);assert(!f.start.disabled);});
test('newer per-control disabled state and changed fieldset eligibility survive release',()=>{const f=fixture(),release=f.lock();f.end.disabled=true;f.fieldset.dataset.calendarEnabled='false';f.root.dataset.calendarReady='false';release();assert(f.end.disabled);assert(f.fieldset.disabled);assert(f.create.disabled);});
test('removed composing control does not wedge an explicit replacement',()=>{const f=fixture();f.root.listeners.compositionstart({target:{}});assert.equal(typeof f.lock(),'function');});

// Execute the actual pinned attribute setter cases against an explicit DOM model.
// This is a pure regression, not a WebView hidden-picker-buffer acceptance claim.
const pinned='C:/Users/mortar/.cargo/git/checkouts/dioxus-d8abd2ecf6e8b5b4/57d6794/packages/interpreter/src/ts/set_attribute.ts';
const setter=fs.readFileSync(pinned,'utf8');
const cases=setter.slice(setter.indexOf('    case "value":'),setter.indexOf('    case "checked":'));
const apply=new Function('node','field','value','setAttributeDefault','switch(field){'+cases+'}');
test('pinned initial_value leaves a dirty native buffer while value replaces it',()=>{
 const native=()=>{let v='',buffer='partial 1:';return {tagName:'INPUT',get value(){return v;},set value(x){v=x;buffer='';},set defaultValue(x){this.default=x;},get buffer(){return buffer;}};};
 const retained=native();apply(retained,'initial_value','09:15',()=>{});assert.equal(retained.buffer,'partial 1:');
 const controlled=native();apply(controlled,'value','09:15',()=>{});assert.equal(controlled.buffer,'');
});
const rust=fs.readFileSync(path.join(root,'src/desktop/message_buttons/shortcuts/calendar/native.rs'),'utf8');
const literal=rust.match(/"(\{\}\\nconst release=lockCalendarCreate[^\n]+)",/);
assert(literal,'actual Rust eval wrapper');
const template=JSON.parse('"'+literal[1]+'"').replace('{}\n','').replace('{id}','7').replace('{expected}','expected').replaceAll('{{','{').replaceAll('}}','}');
for(const failure of ['send','receive','none']){
 const f=fixture();f.context.expected=f.expected;
 let acknowledge;
 f.context.dioxus={send(ok){assert(ok);if(failure==='send')throw new Error('closed IPC send');},recv(){if(failure==='receive')return Promise.reject(new Error('closed IPC receive'));return new Promise(resolve=>acknowledge=resolve);}};
 const running=vm.runInContext('(async()=>{'+template+'})()',f.context);
 if(failure==='none'){assert(f.fieldset.disabled);acknowledge();await running;}else await assert.rejects(running,/closed IPC/);
 assert.equal(f.root.__calendarAdmission.lease,null);assert(!f.fieldset.disabled);assert(!f.create.disabled);
 ++count;console.log('PASS actual generated eval release on '+failure);
}
console.log(JSON.stringify({groups:count,scope:'actual helper plus pinned setter under pure DOM model; native unrun'}));
