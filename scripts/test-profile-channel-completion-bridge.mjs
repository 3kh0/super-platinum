import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
const dir='src/desktop/view/profile_edit/sections/rich/';
const source=fs.readFileSync(dir+'completion.js','utf8');
const matcher=Function(`return (${fs.readFileSync(dir+'trigger.js','utf8')})`)()(JSON.parse(fs.readFileSync(dir+'trigger-data.json')));
class Element {
 constructor(){this.isConnected=true;this.isContentEditable=true;this.dataset={};this.attributes={};this.listeners=new Map();}
 addEventListener(name,fn){if(!this.listeners.has(name))this.listeners.set(name,new Set());this.listeners.get(name).add(fn);}
 removeEventListener(name,fn){this.listeners.get(name)?.delete(fn);}
 setAttribute(k,v){this.attributes[k]=v;}
 removeAttribute(k){delete this.attributes[k];}
 fire(name,more={}){const event={preventDefault(){this.prevented=true;},stopPropagation(){this.stopped=true;},...more};for(const f of [...this.listeners.get(name)??[]])f(event);return event;}
}
function fixture(){
 const root=new Element(),pane=new Element(),field=new Element(),document=new Element(),anchor={};
 root.dataset={dialogGeneration:'1',sectionSerial:'2',sectionLocked:'false'};pane.dataset={richVisit:'3',composing:'false'};
 pane.contains=node=>node===field||node?.isOption===true;
 root.querySelector=s=>s==='#profile-section-rich-pane'?pane:null;pane.querySelector=s=>s==='#profile-section-rich'?field:null;
 const map=new Map([['shell-overlay-dialog',root],['profile-section-rich',field]]);document.getElementById=id=>map.get(id);document.documentElement=new Element();document.activeElement=field;
 let text='Hello #Fi',revision=1,selection={anchorNode:anchor,anchorOffset:text.length},frames=new Map(),id=0;
 const tickets=new WeakSet(),events=[],insertions=[],observers=[];
 const api={caret(node){assert.equal(node,field);const ticket=Object.freeze({before:text,after:'',revision,generation:'2',composing:false});tickets.add(ticket);return ticket;},
 complete(node,ticket,length,value,label,suffix){assert.equal(node,field);assert(tickets.has(ticket),'reconstructed ticket');if(ticket.before!==text||ticket.revision!==revision)return null;insertions.push({ticket,length,value,label,suffix});return {blocks:[]};}};
 class Observer{constructor(fn){this.fn=fn;this.closed=false;observers.push(this);}observe(){}disconnect(){this.closed=true;}}
 const context=vm.createContext({document,window:{superPlatinumRichEditor:api},getSelection:()=>selection,MutationObserver:Observer,
 requestAnimationFrame:fn=>{frames.set(++id,fn);return id;},cancelAnimationFrame:i=>frames.delete(i)});
 const mount=vm.runInContext(`(${source})`,context);const control=mount(1,2,3,{locale:'en-US'},matcher,e=>events.push(JSON.parse(JSON.stringify(e))));
 const flush=()=>{const all=[...frames.values()];frames.clear();for(const fn of all)fn();};
 flush();
 const change=value=>{text=value;revision++;selection={anchorNode:anchor,anchorOffset:text.length};field.fire('input');flush();};
 return {root,pane,field,document,map,control,events,insertions,observers,flush,change,mount,context,api,
  raw(value){text=value;revision++;selection={anchorNode:anchor,anchorOffset:text.length};},
  move(){selection={anchorNode:{},anchorOffset:text.length};}};
}
let groups=0;
{
 const f=fixture(),q=f.events.at(-1);assert.equal(q.kind,'query');assert.equal(q.query,'Fi');assert.equal(q.token,'#Fi');assert.equal(q.index,6);
 assert(!('ticket'in q));assert(!('value'in q));assert.equal(q.sequence,1);
 f.document.fire('selectionchange');f.flush();assert.equal(f.events.length,1);groups++;
}
{
 const f=fixture();assert(f.control.message({kind:'choices',sequence:1,revision:1,count:2}));
 assert(f.control.message({kind:'choose',sequence:1,revision:1,index:1}));
 assert.equal(f.events.at(-1).kind,'choose');
 assert(!f.control.message({kind:'insert',sequence:1,revision:2,index:1,value:{type:'channel',channel_id:'C2'},label:'#Fixture',suffix:' '}));
 assert(f.control.message({kind:'insert',sequence:1,revision:1,index:1,value:{type:'channel',channel_id:'C2'},label:'#Fixture',suffix:' '}));
 assert.equal(f.insertions.length,1);assert.equal(f.insertions[0].length,3);assert.equal(f.events.at(-1).accepted,true);
 assert(!f.control.message({kind:'insert',sequence:1,revision:1,index:1,value:{type:'channel',channel_id:'C2'},label:'#Fixture',suffix:' '}));groups++;
}
{
 const f=fixture();f.control.message({kind:'choices',sequence:1,revision:1,count:2});f.change('Hello #Other');
 assert.equal(f.events.at(-1).sequence,2);assert(!f.control.message({kind:'choices',sequence:1,revision:9,count:1}));
 f.control.message({kind:'choices',sequence:2,revision:1,count:1});
 assert(!f.control.message({kind:'choose',sequence:1,revision:1,index:0}));assert.equal(f.insertions.length,0);groups++;
}
{
 const f=fixture();f.control.message({kind:'choices',sequence:1,revision:1,count:3});
 const down=f.field.fire('keydown',{key:'ArrowDown'});assert(down.prevented&&down.stopped);assert.equal(f.events.at(-1).index,1);
 const tab=f.field.fire('keydown',{key:'Tab'});assert(tab.prevented&&tab.stopped);assert.equal(f.events.at(-1).kind,'choose');assert.equal(f.events.at(-1).index,1);groups++;
}
{
 const f=fixture();f.control.message({kind:'choices',sequence:1,revision:1,count:1});
 const event=f.field.fire('keydown',{key:'Enter',isComposing:true});assert(!event.prevented);f.field.fire('compositionstart');
 assert.equal(f.events.at(-1).kind,'closed');assert(!f.control.message({kind:'insert',sequence:1,revision:1,index:0,value:{type:'channel',channel_id:'C2'},label:'#Fixture',suffix:' '}));groups++;
}
{
 const f=fixture();f.control.message({kind:'choices',sequence:1,revision:1,count:1});f.control.message({kind:'choose',sequence:1,revision:1,index:0});
 f.root.dataset.sectionSerial='99';assert(!f.control.message({kind:'insert',sequence:1,revision:1,index:0,value:{type:'channel',channel_id:'C2'},label:'#Fixture',suffix:' '}));
 assert(f.observers.every(o=>o.closed));assert.equal(f.insertions.length,0);groups++;
}
{
 const f=fixture();const old=f.control;
 const newer=f.mount(1,2,3,{},matcher,e=>f.events.push(e));assert(!old.current());assert(newer.current());
 old.stop();assert(newer.current());newer.stop();assert.equal(f.field.__profileRichCompletion,undefined);assert(f.observers.every(o=>o.closed));groups++;
}
{
 const f=fixture();f.control.message({kind:'choices',sequence:1,revision:1,count:1});f.control.message({kind:'choose',sequence:1,revision:1,index:0});
 f.field.isContentEditable=false;assert(!f.control.message({kind:'insert',sequence:1,revision:1,index:0,value:{type:'channel',channel_id:'C2'},label:'#Fixture',suffix:' '}));assert.equal(f.insertions.length,0);groups++;
}
{
 const f=fixture();assert(f.control.message({kind:'choices',sequence:1,revision:2,count:2}));
 assert(!f.control.message({kind:'choices',sequence:1,revision:1,count:1}));
 assert(!f.control.message({kind:'choices',sequence:1,revision:2,count:1}));
 assert(f.control.message({kind:'choose',sequence:1,revision:2,index:1}));
 assert(!f.control.message({kind:'choices',sequence:1,revision:3,count:3}));
 assert(!f.control.message({kind:'choose',sequence:1,revision:3,index:2}));
 assert(f.control.message({kind:'insert',sequence:1,revision:2,index:1,value:{type:'channel',channel_id:'C2'},label:'#Fixture',suffix:' '}));
 assert.equal(f.insertions.length,1);groups++;
}

{
 const f=fixture();f.control.message({kind:'choices',sequence:1,revision:1,count:1});
 assert(f.control.message({kind:'refresh'}));
 assert(!f.control.message({kind:'choices',sequence:1,revision:2,count:1}));
 assert(!f.control.message({kind:'choose',sequence:1,revision:1,index:0}));
 f.flush();const fresh=f.events.at(-1);assert.equal(fresh.kind,'query');assert.equal(fresh.sequence,2);
 assert.equal(fresh.query,'Fi');assert.equal(fresh.revision,1);assert(!('ticket' in fresh));
 assert(f.control.message({kind:'choices',sequence:2,revision:1,count:1}));groups++;
}
{
 const f=fixture();f.control.message({kind:'choices',sequence:1,revision:1,count:1});
 f.field.fire('keydown',{key:'Escape'});const n=f.events.length;
 assert(!f.control.message({kind:'refresh'}));f.flush();assert.equal(f.events.length,n);groups++;
}
{
 const f=fixture();f.control.message({kind:'choices',sequence:1,revision:1,count:1});
 f.control.message({kind:'choose',sequence:1,revision:1,index:0});
 assert(!f.control.message({kind:'refresh'}));
 assert(f.control.message({kind:'insert',sequence:1,revision:1,index:0,value:{type:'channel',channel_id:'C2'},label:'#Fixture',suffix:' '}));groups++;
}
{
 const f=fixture();f.pane.dataset.composing='true';assert(!f.control.message({kind:'refresh'}));
 f.pane.dataset.composing='false';f.document.activeElement=f.root;assert(!f.control.message({kind:'refresh'}));
 f.document.activeElement=f.field;f.map.set('profile-section-rich',new Element());assert(!f.control.message({kind:'refresh'}));
 assert(!f.control.current());groups++;
}
// Actual Choose latch recovery after Rust declines stale metadata/date admission.
{
 const f=fixture();f.control.message({kind:'choices',sequence:1,revision:4,count:2});
 f.control.message({kind:'choose',sequence:1,revision:4,index:1});
 assert(!f.control.message({kind:'refresh'})); // metadata effect may precede rejection
 assert(!f.control.message({kind:'rejected',sequence:1,revision:3,index:1}));
 assert(!f.control.message({kind:'rejected',sequence:1,revision:4,index:0}));
 assert(f.control.message({kind:'rejected',sequence:1,revision:4,index:1}));
 assert(!f.control.message({kind:'insert',sequence:1,revision:4,index:1,value:{type:'channel',channel_id:'C2'},label:'#Old',suffix:' '}));
 f.flush();const fresh=f.events.at(-1);assert.equal(fresh.kind,'query');assert.equal(fresh.sequence,2);assert.equal(fresh.query,'Fi');
 f.control.message({kind:'choices',sequence:2,revision:1,count:1});
 assert(!f.control.message({kind:'rejected',sequence:1,revision:4,index:1}));
 assert(f.control.message({kind:'choose',sequence:2,revision:1,index:0}));
 assert(f.control.message({kind:'insert',sequence:2,revision:1,index:0,value:{type:'channel',channel_id:'C2'},label:'#Fresh',suffix:' '}));
 assert.equal(f.insertions.length,1);assert.equal(f.insertions[0].label,'#Fresh');
 assert(!f.control.message({kind:'rejected',sequence:2,revision:1,index:0}));assert.equal(f.insertions.length,1);groups++;
}
{
 const f=fixture();f.control.message({kind:'choices',sequence:1,revision:1,count:1});f.control.message({kind:'choose',sequence:1,revision:1,index:0});
 f.change('Hello #New');f.control.message({kind:'choices',sequence:2,revision:1,count:1});
 const n=f.events.length;assert(!f.control.message({kind:'rejected',sequence:1,revision:1,index:0}));f.flush();assert.equal(f.events.length,n);
 assert(f.control.message({kind:'choose',sequence:2,revision:1,index:0}));groups++;
}
{
 const f=fixture();f.control.message({kind:'choices',sequence:1,revision:1,count:1});f.control.message({kind:'choose',sequence:1,revision:1,index:0});
 f.field.isContentEditable=false;const n=f.events.length;
 assert(f.control.message({kind:'rejected',sequence:1,revision:1,index:0}));f.flush();assert.equal(f.events.length,n);
 f.field.isContentEditable=true;f.field.fire('focus');f.flush();assert.equal(f.events.at(-1).sequence,2);groups++;
}
{
 const f=fixture();f.control.message({kind:'choices',sequence:1,revision:1,count:1});f.control.message({kind:'choose',sequence:1,revision:1,index:0});
 const old=f.control;const newer=f.mount(1,2,3,{},matcher,e=>f.events.push(e));
 assert(!old.message({kind:'rejected',sequence:1,revision:1,index:0}));assert(newer.current());assert.equal(f.insertions.length,0);groups++;
}
// Emoji remains outside this completion route.
for(const text of ['Hello :smile']) {
 const f=fixture();f.change(text);const n=f.events.length;
 const enter=f.field.fire('keydown',{key:'Enter'});assert(!enter.prevented);
 assert.notEqual(f.events.at(-1).kind,'query');assert.equal(f.events.length,n);groups++;
}
// No host choice may consume a ticket after raw input moves ahead of its event.
for(const mutate of [f=>f.raw('Hello #new'),f=>f.move()]) {
 const f=fixture();f.control.message({kind:'choices',sequence:1,revision:1,count:1});mutate(f);
 assert(!f.control.message({kind:'choose',sequence:1,revision:1,index:0}));
 assert.equal(f.events.at(-1).kind,'closed');assert.equal(f.insertions.length,0);groups++;
}
{
 const f=fixture();f.raw('Hello #new');assert(!f.control.message({kind:'choices',sequence:1,revision:1,count:1}));
 assert.equal(f.field.attributes['aria-expanded'],'false');groups++;
}
// Escape also dismisses pending search; keyup does not reopen an unchanged caret.
for(const rows of [false,true]) {
 const f=fixture();if(rows)f.control.message({kind:'choices',sequence:1,revision:1,count:1});
 const e=f.field.fire('keydown',{key:'Escape'});assert(e.prevented&&e.stopped);const n=f.events.length;
 f.field.fire('keyup',{key:'Escape'});f.flush();assert.equal(f.events.length,n);
 f.change('Hello #other');assert.equal(f.events.at(-1).kind,'query');groups++;
}
{
 const f=fixture(),option={isOption:true};const e=f.pane.fire('mousedown',{target:{closest:()=>option}});
 assert(e.prevented);assert.equal(f.document.activeElement,f.field);
 const other=f.pane.fire('mousedown',{target:{closest:()=>({})}});assert(!other.prevented);groups++;
}
{
 const f=fixture();f.control.message({kind:'choices',sequence:1,revision:1,count:1});
 f.root.querySelector=()=>new Element();assert(!f.control.message({kind:'choose',sequence:1,revision:1,index:0}));
 assert.equal(f.insertions.length,0);assert(f.observers.every(o=>o.closed));groups++;
}
{
 const f=fixture();f.control.message({kind:'choices',sequence:1,revision:1,count:1});
 f.control.message({kind:'choose',sequence:1,revision:1,index:0});
 assert(!f.control.message({kind:'insert',sequence:1,revision:1,index:0,value:{type:'user',user_id:'U2'},label:'@User',suffix:' '}));groups++;
}
{
 const f=fixture();f.change('Hello \uff03ship');assert.equal(f.events.at(-1).trigger,'channel');assert.equal(f.events.at(-1).query,'ship');groups++;
}
// A renderer-only departure retires the corresponding host task exactly once.
{
 const f=fixture();f.map.set('profile-section-rich',new Element());
 f.observers.at(-1).fn();f.control.stop();
 assert.equal(f.events.filter(e=>e.kind==='stopped').length,1);
 assert(f.observers.every(o=>o.closed));
 assert.equal(f.field.listeners.get('keydown').size,0);
 assert(!f.control.current());groups++;
}
// Member queries keep exact native tickets and refuse cross-category tokens.
for(const text of ['Hello @Alex','Hello \uff20Alex']) {
 const f=fixture();f.change(text);const q=f.events.at(-1);
 assert.equal(q.kind,'query');assert.equal(q.trigger,'member');assert.equal(q.query,'Alex');
 assert(f.control.message({kind:'choices',sequence:q.sequence,revision:1,count:2}));
 assert(f.control.message({kind:'choose',sequence:q.sequence,revision:1,index:1}));
 assert(!f.control.message({kind:'insert',sequence:q.sequence,revision:1,index:1,value:{type:'channel',channel_id:'C2'},label:'#Wrong',suffix:' '}));
 assert(f.control.message({kind:'insert',sequence:q.sequence,revision:1,index:1,value:{type:'user',user_id:'U2'},label:'@Same name',suffix:' '}));
 assert.equal(f.insertions[0].value.user_id,'U2');assert.equal(f.insertions[0].ticket.before,text);groups++;
}
{
 const f=fixture();f.change('Hello @Alex');const q=f.events.at(-1);
 assert(f.control.message({kind:'choices',sequence:q.sequence,revision:1,count:1}));
 f.raw('Hello @Alex changed');assert(!f.control.message({kind:'choose',sequence:q.sequence,revision:1,index:0}));
 assert.equal(f.insertions.length,0);groups++;
}
// Groups share the @ trigger but retain their exact private group identity.
for (const text of ['Hello @team', 'Hello \uff20team']) {
 const f=fixture();f.change(text);const q=f.events.at(-1);
 assert(f.control.message({kind:'choices',sequence:q.sequence,revision:1,count:2}));
 assert(f.control.message({kind:'choose',sequence:q.sequence,revision:1,index:1}));
 for (const value of [{type:'usergroup',user_id:'U2'}, {type:'usergroup',usergroup_id:''}, {type:'channel',channel_id:'C2'}])
  assert(!f.control.message({kind:'insert',sequence:q.sequence,revision:1,index:1,value,label:'@team',suffix:' '}));
 assert(f.control.message({kind:'insert',sequence:q.sequence,revision:1,index:1,value:{type:'usergroup',usergroup_id:'SGROUP002'},label:'@@team',suffix:' '}));
 assert.equal(f.insertions[0].value.usergroup_id,'SGROUP002');assert.equal(f.insertions[0].ticket.before,text);
 assert.equal(f.insertions[0].label,'@@team');assert.equal(f.insertions.length,1);groups++;
}
{
 const f=fixture();f.control.message({kind:'choices',sequence:1,revision:1,count:1});
 f.control.message({kind:'choose',sequence:1,revision:1,index:0});
 assert(!f.control.message({kind:'insert',sequence:1,revision:1,index:0,value:{type:'usergroup',usergroup_id:'SGROUP001'},label:'@team',suffix:' '}));
 assert.equal(f.insertions.length,0);groups++;
}
console.log(JSON.stringify({actualBridgeHelperGroups:groups,opaqueTicketPreserved:true,noBrowserExecution:true}));
