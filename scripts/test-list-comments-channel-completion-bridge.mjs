import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
const dir='src/desktop/view/profile_edit/sections/rich/';
const source=fs.readFileSync('src/desktop/view/slack_lists/comments/completion.js','utf8');
const matcher=Function(`return (${fs.readFileSync(dir+'trigger.js','utf8')})`)()(JSON.parse(fs.readFileSync(dir+'trigger-data.json')));
class Element {
 closest(){return null;}
 getBoundingClientRect(){return {left:10,top:300,bottom:600,width:500,height:300};}
 constructor(){this.style={};this.isConnected=true;this.isContentEditable=true;this.dataset={};this.attributes={};this.listeners=new Map();}
 addEventListener(name,fn){if(!this.listeners.has(name))this.listeners.set(name,new Set());this.listeners.get(name).add(fn);}
 removeEventListener(name,fn){this.listeners.get(name)?.delete(fn);}
 setAttribute(k,v){this.attributes[k]=v;}
 removeAttribute(k){delete this.attributes[k];}
 fire(name,more={}){const event={preventDefault(){this.prevented=true;},stopPropagation(){this.stopped=true;},...more};for(const f of [...this.listeners.get(name)??[]])f(event);return event;}
}
function fixture(autoPolicy=true){
 const root=new Element(),pane=new Element(),field=new Element(),document=new Element(),anchor={};
 pane.dataset={listCommentsComposeScope:'scope',editorGeneration:'2',composing:'false'};
 pane.contains=node=>node===field||node?.isOption===true;
 root.querySelector=s=>s==='#slack-list-comment-compose'?pane:null;pane.querySelector=s=>s==='#slack-list-comment-draft'?field:null;
 const map=new Map([['shell-overlay-dialog',root],['slack-list-comment-draft',field]]);document.getElementById=id=>map.get(id);document.documentElement=new Element();document.activeElement=field;
 let text='Hello #Fi',revision=1,selection={anchorNode:anchor,anchorOffset:text.length},frames=new Map(),id=0;
 const tickets=new WeakSet(),events=[],insertions=[],observers=[];
 const api={caret(node){assert.equal(node,field);const ticket=Object.freeze({before:text,after:'',revision,generation:'2',composing:false});tickets.add(ticket);return ticket;},
 complete(node,ticket,length,value,label,suffix){assert.equal(node,field);assert(tickets.has(ticket),'reconstructed ticket');if(ticket.before!==text||ticket.revision!==revision)return null;insertions.push({ticket,length,value,label,suffix});return {blocks:[]};}};
 class Observer{constructor(fn){this.fn=fn;this.closed=false;observers.push(this);}observe(){}disconnect(){this.closed=true;}}
 const context=vm.createContext({document,window:{superPlatinumRichEditor:api,__listCommentsComposeCaptures:new Map([['mount',{running:true,root:field,owner:pane}]])},getSelection:()=>selection,MutationObserver:Observer,
 requestAnimationFrame:fn=>{frames.set(++id,fn);return id;},cancelAnimationFrame:i=>frames.delete(i)});
 const mount=vm.runInContext(`(${source})`,context);const control=mount('scope',2,'mount',matcher,e=>{events.push(JSON.parse(JSON.stringify(e)));if(autoPolicy&&e.kind==='context')control.message({kind:'policy',sequence:e.sequence,friendly:false});});
 const flush=()=>{const all=[...frames.values()];frames.clear();for(const fn of all)fn();};
 flush();
 const change=value=>{text=value;revision++;selection={anchorNode:anchor,anchorOffset:text.length};field.fire('input');flush();};
 return {root,pane,field,document,map,control,events,insertions,observers,flush,change,mount,context,api,
  raw(value){text=value;revision++;selection={anchorNode:anchor,anchorOffset:text.length};},
  move(){selection={anchorNode:{},anchorOffset:text.length};}};
}
let groups=0;
{
 const f=fixture(),q=f.events.at(-1);assert.equal(q.kind,'query');assert.equal(q.query,'#Fi');assert.equal(q.index,6);
 assert(!('ticket'in q));assert(!('value'in q));assert.equal(q.sequence,1);
 f.document.fire('selectionchange');f.flush();assert.equal(f.events.length,2);groups++;
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
 assert.equal(f.events.at(-1).sequence,3);assert(!f.control.message({kind:'choices',sequence:1,revision:9,count:1}));
 f.control.message({kind:'choices',sequence:3,revision:1,count:1});
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
 f.pane.dataset.editorGeneration='99';assert(!f.control.message({kind:'insert',sequence:1,revision:1,index:0,value:{type:'channel',channel_id:'C2'},label:'#Fixture',suffix:' '}));
 assert(f.observers.every(o=>o.closed));assert.equal(f.insertions.length,0);groups++;
}
{
 const f=fixture();const old=f.control;
 const newer=f.mount('scope',2,'mount',matcher,e=>f.events.push(e));assert(!old.current());assert(newer.current());
 old.stop();assert(newer.current());newer.stop();assert.equal(f.field.__listCommentCompletion,undefined);assert(f.observers.every(o=>o.closed));groups++;
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
 assert.equal(fresh.query,'#Fi');assert.equal(fresh.revision,1);assert(!('ticket' in fresh));
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
 f.document.activeElement=f.field;f.map.set('slack-list-comment-draft',new Element());assert(!f.control.message({kind:'refresh'}));
 assert(!f.control.current());groups++;
}

{
 const f=fixture(false);assert.equal(f.events.at(-1).kind,'context');
 assert(!f.control.message({kind:'choices',sequence:1,revision:1,count:1}));
 f.raw('Hello #Other');assert(!f.control.message({kind:'policy',sequence:1,friendly:true}));
 assert(!f.events.some(e=>e.kind==='query'));groups++;
}
{
 const f=fixture(false);f.raw('Hello #Old name');f.field.fire('input');f.flush();
 const seq=f.events.at(-1).sequence;assert(f.control.message({kind:'policy',sequence:seq,friendly:true}));
 assert.equal(f.events.at(-1).query,'#Old name');groups++;
}
{
 const f=fixture(false);assert(!f.control.message({kind:'policy',sequence:1,friendly:null}));
 assert(!f.events.some(e=>e.kind==='query'));groups++;
}
{
 const f=fixture();f.control.message({kind:'choices',sequence:1,revision:1,count:1});f.move();
 assert(!f.control.message({kind:'choose',sequence:1,revision:1,index:0}));assert.equal(f.insertions.length,0);groups++;
}

{
 const f=fixture();f.control.message({kind:'choices',sequence:1,revision:1,count:1});
 assert(f.control.message({kind:'failure',sequence:1}));
 assert(!f.control.message({kind:'choose',sequence:1,revision:1,index:0}));
 assert.equal(f.insertions.length,0);groups++;
}
{
 const f=fixture();f.change('Hello #Next');const seq=f.events.at(-1).sequence;
 assert(!f.control.message({kind:'invalidated',sequence:1}));
 assert(f.control.message({kind:'choices',sequence:seq,revision:1,count:1}));
 assert(f.control.message({kind:'invalidated',sequence:seq}));
 assert(!f.control.message({kind:'choose',sequence:seq,revision:1,index:0}));groups++;
}
{
 const f=fixture(false);f.raw('Hello \uff03');f.field.fire('input');f.flush();
 assert(f.control.message({kind:'policy',sequence:f.events.at(-1).sequence,friendly:false}));
 assert.equal(f.events.at(-1).query,'\uff03');groups++;
}
{
 const f=fixture(false);f.raw('https://example.test/#name');f.field.fire('input');f.flush();
 f.control.message({kind:'policy',sequence:f.events.at(-1).sequence,friendly:true});
 assert(!f.events.some(e=>e.kind==='query'));groups++;
}

// Member completion shares the original ticket and sequence with channels.
{
 const f=fixture();f.change('Hello @Ada Lovelace');const q=f.events.at(-1);
 assert.equal(q.kind,'query');assert.equal(q.trigger,'member');assert.equal(q.query,'@Ada Lovelace');assert.equal(q.index,6);
 assert(f.control.message({kind:'choices',sequence:q.sequence,revision:1,count:2}));
 f.field.fire('keydown',{key:'ArrowDown'});f.field.fire('keydown',{key:'Enter'});
 assert.equal(f.events.at(-1).index,1);
 assert(!f.control.message({kind:'insert',sequence:q.sequence,revision:1,index:1,value:{type:'channel',channel_id:'C2'},label:'#Wrong',suffix:' '}));
 const user={type:'user',user_id:'U_ORIGINAL',opaque:{retained:true}};
 assert(f.control.message({kind:'insert',sequence:q.sequence,revision:1,index:1,value:user,label:'@Ada Lovelace',suffix:' '}));
 assert.equal(f.insertions[0].length,'@Ada Lovelace'.length);assert.deepEqual(f.insertions[0].value,user);
 assert.equal(f.insertions[0].ticket.before,'Hello @Ada Lovelace');groups++;
}
{
 const f=fixture();f.change('Hello \uff20');const q=f.events.at(-1);assert.equal(q.trigger,'member');assert.equal(q.query,'\uff20');
 assert(f.control.message({kind:'choices',sequence:q.sequence,revision:1,count:1}));
 assert(f.control.message({kind:'choose',sequence:q.sequence,revision:1,index:0}));
 for(const value of [{type:'user',user_id:''},{type:'usergroup',usergroup_id:''},{type:'broadcast',range:'here'},{type:'file',file_id:'F1'}])
  assert(!f.control.message({kind:'insert',sequence:q.sequence,revision:1,index:0,value,label:'Wrong',suffix:' '}));
 assert(f.control.message({kind:'insert',sequence:q.sequence,revision:1,index:0,value:{type:'user',user_id:'U1'},label:'@Ada',suffix:' '}));groups++;
}
{
 const f=fixture();f.control.message({kind:'choices',sequence:1,revision:1,count:1});f.change('Hello @Ada');const member=f.events.at(-1);
 assert.equal(member.trigger,'member');assert(!f.control.message({kind:'choose',sequence:1,revision:1,index:0}));
 f.control.message({kind:'choices',sequence:member.sequence,revision:1,count:1});f.control.message({kind:'choose',sequence:member.sequence,revision:1,index:0});
 f.change('Hello #Again');const channel=f.events.at(-1);assert.equal(channel.trigger,'channel');
 assert(!f.control.message({kind:'insert',sequence:member.sequence,revision:1,index:0,value:{type:'user',user_id:'U1'},label:'@Old',suffix:' '}));
 assert(!f.control.message({kind:'failure',sequence:member.sequence}));assert(!f.control.message({kind:'invalidated',sequence:member.sequence}));
 assert(f.control.message({kind:'choices',sequence:channel.sequence,revision:1,count:1}));
 assert(f.control.message({kind:'choose',sequence:channel.sequence,revision:1,index:0}));
 assert(!f.control.message({kind:'insert',sequence:channel.sequence,revision:1,index:0,value:{type:'user',user_id:'U1'},label:'@Wrong',suffix:' '}));
 assert.equal(f.insertions.length,0);groups++;
}
{
 const f=fixture(false);f.change('Hello @Ada');const old=f.events.at(-1).sequence;f.change('Hello #Next');const fresh=f.events.at(-1).sequence;
 assert(!f.control.message({kind:'policy',sequence:old,friendly:false}));
 assert(f.control.message({kind:'policy',sequence:fresh,friendly:false}));assert.equal(f.events.at(-1).trigger,'channel');groups++;
}
{
 const f=fixture();f.change('Hello @Ada #project');assert.equal(f.events.at(-1).trigger,'channel');assert.equal(f.events.at(-1).query,'#project');
 f.change('Hello #project @Ada');assert.equal(f.events.at(-1).trigger,'member');assert.equal(f.events.at(-1).query,'@Ada');groups++;
}
{
 for(const text of ['x@ada','@ space','https://example.com/@ada','@a\nb','@a b c d e f g h i j']){
  const f=fixture(false);f.change(text);const context=f.events.at(-1);f.control.message({kind:'policy',sequence:context.sequence,friendly:false});
  assert(!f.events.some(e=>e.kind==='query'),text);
 }
 groups++;
}
{
 const f=fixture();f.change('Hello @Ada');const q=f.events.at(-1);f.control.message({kind:'choices',sequence:q.sequence,revision:1,count:1});
 f.raw('Hello @Grace');assert(!f.control.message({kind:'choose',sequence:q.sequence,revision:1,index:0}));assert.equal(f.insertions.length,0);groups++;
}
{
 const f=fixture();f.change('Hello @Ada');const q=f.events.at(-1);f.control.message({kind:'choices',sequence:q.sequence,revision:1,count:1});
 f.field.fire('compositionstart');assert(!f.control.message({kind:'choose',sequence:q.sequence,revision:1,index:0}));assert.equal(f.insertions.length,0);groups++;
}

// Groups use the same private mixed batch and native caret, not a new trigger.
for(const before of ['Hello @Cafe','Hello \uff20Cafe']){
 const f=fixture();f.change(before);const q=f.events.at(-1);
 assert.equal(q.trigger,'member');
 assert(f.control.message({kind:'choices',sequence:q.sequence,revision:7,count:2}));
 f.field.fire('keydown',{key:'ArrowDown'});f.field.fire('keydown',{key:'Enter'});
 const value={type:'usergroup',usergroup_id:'S_ORIGINAL',opaque:{keep:'same native token'}};
 const label='@@Caf\u00e9 ';
 assert(f.control.message({kind:'insert',sequence:q.sequence,revision:7,index:1,value,label,suffix:' '}));
 assert.deepEqual(f.insertions[0].value,value);assert.equal(f.insertions[0].label,label);
 assert.equal(f.insertions[0].ticket.before,before);assert.equal(f.insertions[0].length,5);
 assert(!f.control.message({kind:'insert',sequence:q.sequence,revision:7,index:1,value,label,suffix:' '}));
 groups++;
}
{
 const f=fixture();f.change('Hello @Same');const q=f.events.at(-1);
 f.control.message({kind:'choices',sequence:q.sequence,revision:2,count:2});
 assert(f.control.message({kind:'choose',sequence:q.sequence,revision:2,index:1}));
 const message={kind:'insert',sequence:q.sequence,revision:2,index:1,label:'@Same',suffix:' '};
 for(const value of [null,{},[],{type:'usergroup'},{type:'usergroup',usergroup_id:null},
   {type:'usergroup',usergroup_id:3},{type:'usergroup',usergroup_id:{}},
   {type:'usergroup',user_id:'U1'},{type:'user',usergroup_id:'S1'},
   {type:'channel',channel_id:'C1'},{type:'broadcast',range:'here'}]){
  assert(!f.control.message({...message,value}));assert.equal(f.insertions.length,0);
 }
 const value={type:'usergroup',usergroup_id:'S_SECOND'};
 assert(!f.control.message({...message,index:0,value}));
 assert(!f.control.message({...message,revision:1,value}));
 assert(!f.control.message({...message,sequence:q.sequence-1,value}));
 assert(!f.control.message({...message,label:null,value}));
 assert(!f.control.message({...message,suffix:'',value}));
 assert(f.control.message({...message,value}));assert.equal(f.insertions[0].value.usergroup_id,'S_SECOND');groups++;
}
{
 const f=fixture();f.control.message({kind:'choices',sequence:1,revision:1,count:1});
 f.control.message({kind:'choose',sequence:1,revision:1,index:0});
 assert(!f.control.message({kind:'insert',sequence:1,revision:1,index:0,value:{type:'usergroup',usergroup_id:'S1'},label:'@Group',suffix:' '}));
 assert.equal(f.insertions.length,0);groups++;
}
for(const retire of [
 f=>f.change('Hello #Other'),f=>{f.pane.dataset.editorGeneration='99';},
 f=>{f.pane.dataset.listCommentsComposeScope='replacement';},
 f=>{f.context.window.__listCommentsComposeCaptures.set('mount',{running:true,root:f.field,owner:f.pane});},
 f=>f.field.fire('compositionstart'),f=>{f.field.isContentEditable=false;},
 f=>{f.map.set('slack-list-comment-draft',new Element());},
 f=>{f.raw('Hello @Changed');}
]){
 const f=fixture();f.change('Hello @Group');const q=f.events.at(-1);
 f.control.message({kind:'choices',sequence:q.sequence,revision:1,count:1});
 f.control.message({kind:'choose',sequence:q.sequence,revision:1,index:0});retire(f);
 assert(!f.control.message({kind:'insert',sequence:q.sequence,revision:1,index:0,value:{type:'usergroup',usergroup_id:'S1'},label:'@Group',suffix:' '}));
 assert.equal(f.insertions.length,0);groups++;
}
// Whole Canvas/List file results keep the private host's wire payload separate
// from its captured display title. No normalization or URL synthesis here.
for(const type of ['canvas','file'])for(const prefix of ['@','\uff20']){
 const f=fixture();const before=`Hello ${prefix}Report`;f.change(before);const q=f.events.at(-1);
 assert.equal(q.trigger,'member');
 assert(f.control.message({kind:'choices',sequence:q.sequence,revision:3,count:2}));
 f.field.fire('keydown',{key:'ArrowDown'});f.field.fire('keydown',{key:'Tab'});
 const base={kind:'insert',sequence:q.sequence,revision:3,index:1,label:' Cafe\u0301 &amp; *literal* ',suffix:' '};
 for(const value of [{type,file_id:''},{type,file_id:7,text:'F1'},
   {type,file_id:'F1'},{type,file_id:'F1',text:''},{type,file_id:'F1',text:7},
   {type,file_id:'F1',text:'F1',url:''},{type,file_id:'F1',text:'F1',url:3},
   {type:'list_record',file_id:'F1',text:'F1',record_id:'R1'},
   {type:'channel',channel_id:'C1'}]){
  assert(!f.control.message({...base,value}));assert.equal(f.insertions.length,0);
 }
 const value={type,file_id:'F_ORIGINAL',text:'https://example.invalid/original?x=1&y=2',
  url:'https://example.invalid/original?x=1&y=2',opaque:{preserve:['raw',type]},style:{italic:true}};
 assert(!f.control.message({...base,value,index:0}));assert(!f.control.message({...base,value,revision:2}));
 assert(!f.control.message({...base,value,sequence:q.sequence-1}));
 assert(f.control.message({...base,value}));
 assert.deepEqual(f.insertions[0].value,value);assert.equal(f.insertions[0].label,base.label);
 assert.notEqual(base.label,base.label.normalize('NFC'));
 assert.equal(f.insertions[0].ticket.before,before);assert.equal(f.insertions[0].length,7);
 assert.equal(f.insertions[0].suffix,' ');assert(!f.control.message({...base,value}));groups++;
}
for(const type of ['canvas','file'])for(const url of [undefined,null]){
 const f=fixture();f.change('Hello @Report');const q=f.events.at(-1);
 f.control.message({kind:'choices',sequence:q.sequence,revision:1,count:1});
 f.control.message({kind:'choose',sequence:q.sequence,revision:1,index:0});
 const value={type,file_id:'F1',text:'F1',...(url===undefined?{}:{url})};
 assert(f.control.message({kind:'insert',sequence:q.sequence,revision:1,index:0,value,label:'Original title',suffix:' '}));
 assert.deepEqual(f.insertions[0].value,value);groups++;
}
for(const type of ['canvas','file']){
 const f=fixture();f.control.message({kind:'choices',sequence:1,revision:1,count:1});
 f.control.message({kind:'choose',sequence:1,revision:1,index:0});
 assert(!f.control.message({kind:'insert',sequence:1,revision:1,index:0,value:{type,file_id:'F1',text:'F1'},label:'Original title',suffix:' '}));
 assert.equal(f.insertions.length,0);groups++;
}
for(const retire of [f=>f.change('Hello @Later'),f=>{f.pane.dataset.editorGeneration='8';},
 f=>{f.context.window.__listCommentsComposeCaptures.delete('mount');},
 f=>f.field.fire('compositionstart'),f=>{f.field.isContentEditable=false;},
 f=>{f.raw('Hello @Native change');}]){
 const f=fixture();f.change('Hello @Report');const q=f.events.at(-1);
 f.control.message({kind:'choices',sequence:q.sequence,revision:1,count:1});
 f.control.message({kind:'choose',sequence:q.sequence,revision:1,index:0});retire(f);
 assert(!f.control.message({kind:'insert',sequence:q.sequence,revision:1,index:0,value:{type:'canvas',file_id:'F1',text:'F1'},label:'Original title',suffix:' '}));
 assert.equal(f.insertions.length,0);groups++;
}
console.log(JSON.stringify({ok:true,groups}));
