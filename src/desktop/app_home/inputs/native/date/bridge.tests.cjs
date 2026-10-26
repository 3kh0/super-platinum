// Actual bridge with a small DOM double; no physical date parser/picker claims.
const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
function setup(optional=true,initial='2026-10-25',date=true){
 const nodes=new Map(),events=[],observers=[];
 class Node {
  constructor(){this.dataset={};this.listeners=new Map();this.isConnected=true;this.parentNode=null;this.value='';this.disabled=false;this.validity={valid:true,badInput:false};}
  appendChild(n){n.parentNode=this;} removeChild(n){n.parentNode=null;}
  setAttribute(k,v){this[k]=v;}
  addEventListener(k,f){const a=this.listeners.get(k)||[];a.push(f);this.listeners.set(k,a);}
  removeEventListener(k,f){this.listeners.set(k,(this.listeners.get(k)||[]).filter(x=>x!==f));}
  fire(k,props={}){const e={key:'',prevented:false,preventDefault(){this.prevented=true},...props};for(const f of [...(this.listeners.get(k)||[])])f(e);return e;}
  focus(){document.activeElement=this;this.fire('focus');}
 }
 const home=new Node();home.dataset={visit:'1',revision:'2'};nodes.set('app-home',home);
 const host=new Node();host.dataset={homeNativeMount:'3',nativeId:'date',locked:'false'};nodes.set('host',host);
 const document={body:home,activeElement:home,getElementById:id=>nodes.get(id),createElement:()=>new Node(),addEventListener(){},removeEventListener(){}};
 const window={};const context=vm.createContext({window,document,MutationObserver:class{constructor(f){this.f=f;observers.push(this)}observe(){}disconnect(){}}});
 vm.runInContext(fs.readFileSync(process.env.DATE_NATIVE_SOURCE||__dirname+'/../../native.js','utf8'),context);
 const config={id:7,host:'host',mount:3,epoch:1,type:'date',date,optional,initial,selected:initial||null,pristine:true,visit:1,revision:2};
 const api=context.homeCalendarNative(config,{send:e=>events.push(e)}),entry=api.entry,node=entry.node;
 const ack=()=>api.ack({mount:entry.mount,seq:entry.seq});ack();
 const commits=()=>events.filter(e=>e.kind==='commit');
 const expected=()=>[{id:7,mount:entry.mount,seq:entry.seq,value:node.value}];
 return{api,entry,node,events,ack,commits,context,host,home,config,document,expected,observers};
}
test('input is buffer only; changed commit once across paired change blur Enter',()=>{
 const s=setup();s.node.focus();s.node.value='2026-11-01';s.node.fire('input');s.ack();assert.equal(s.commits().length,0);assert.equal(s.entry.selected,'2026-10-25');const release=s.context.lockHomeCalendars(s.expected());assert.equal(typeof release,'function');release();
 s.node.fire('change');s.ack();s.node.fire('blur');s.ack();s.node.fire('keydown',{key:'Enter'});s.ack();
 assert.equal(s.commits().length,1);assert.equal(s.commits()[0].value,'2026-11-01');
});
test('focused blur commits, unfocused blur does not; Enter prevents submit but IME does not commit',()=>{
 const s=setup();s.node.value='2026-11-01';s.node.fire('blur');assert.equal(s.commits().length,0);
 s.node.focus();s.node.fire('blur');assert.equal(s.commits().length,1);s.ack();
 s.node.value='2026-11-02';assert.equal(s.node.fire('keydown',{key:'Enter',isComposing:true}).prevented,false);
 s.node.fire('keydown',{key:'Enter',keyCode:229});assert.equal(s.commits().length,1);
 assert.equal(s.node.fire('keydown',{key:'Enter'}).prevented,true);assert.equal(s.commits().length,2);
});
test('optional clear commits empty; required empty restores prior date or remains empty without callback',()=>{
 const a=setup();a.node.value='';a.node.fire('change');assert.equal(a.commits().length,1);assert.equal(a.commits()[0].value,'');
 const b=setup(false);b.node.value='';b.node.fire('change');assert.equal(b.node.value,'2026-10-25');assert.equal(b.commits().length,0);
 const c=setup(false,'');c.node.fire('change');assert.equal(c.commits().length,0);
});
test('badInput remains unresolved safety buffer and cannot become empty clear or acquire lease',()=>{
 const s=setup();s.node.value='';s.node.validity={valid:false,badInput:true};s.node.fire('change');s.ack();
 assert.equal(s.commits().length,0);assert.equal(s.entry.selected,'2026-10-25');assert.equal(s.context.lockHomeCalendars(s.expected()),null);
 s.node.validity={valid:true,badInput:false};s.node.value='2026-10-25';s.node.fire('change');s.ack();assert.equal(s.commits().length,0);assert.equal(typeof s.context.lockHomeCalendars(s.expected()),'function');
});
test('lease, actual disabled and stale source prevent commits; exact acknowledged value required',()=>{
 const s=setup();const expected=s.expected();const release=s.context.lockHomeCalendars(expected);assert.equal(typeof release,'function');s.node.value='2026-11-01';s.node.fire('change');assert.equal(s.commits().length,0);release();
 assert.equal(s.context.lockHomeCalendars(expected),null);s.node.disabled=true;s.node.fire('change');assert.equal(s.commits().length,0);
 s.node.disabled=false;s.home.dataset.revision='9';s.node.fire('change');assert.equal(s.commits().length,0);
});
test('source remount changes settled display only; uncommitted buffer is not overwritten',()=>{
 const s=setup();s.api.close();s.ack();s.host.dataset.homeNativeMount='4';
 const events=[];const api=s.context.homeCalendarNative({...s.config,mount:4,initial:'2026-12-01',selected:'2026-12-01'},{send:e=>events.push(e)});
 assert.equal(api.entry.node,s.node);assert.equal(s.node.value,'2026-12-01');assert.equal(events.filter(e=>e.kind==='commit').length,0);
 api.ack({mount:4,seq:api.entry.seq});s.node.value='2027-01-01';s.node.fire('input');api.ack({mount:4,seq:api.entry.seq});api.close();api.ack({mount:4,seq:api.entry.seq});
 s.host.dataset.homeNativeMount='5';const next=s.context.homeCalendarNative({...s.config,mount:5,initial:'2026-12-02',selected:'2026-12-02'},{send:e=>events.push(e)});
 assert.equal(next.entry.node.value,'2027-01-01');assert.equal(next.entry.selected,'2026-12-02');
});
test('legacy time/datetime mode continues reporting input/change observations',()=>{
 const s=setup(true,'12:00',false);s.node.value='13:00';s.node.fire('input');s.node.fire('change');assert.equal(s.commits().length,0);assert.equal(s.events.filter(e=>e.kind==='input').length,2);
});

test('first optional blank from absent local date commits null candidate once',()=>{
 const s=setup(true,'');assert.equal(s.entry.selected,null);s.node.focus();s.node.fire('blur');s.ack();assert.equal(s.commits().length,1);assert.equal(s.entry.selected,'');
 s.node.fire('keydown',{key:'Enter'});s.ack();assert.equal(s.commits().length,1);
});

test('acknowledged draft leases without committing; unacked or unseen value still refuses',()=>{
 const s=setup();s.node.value='2026-11-01';s.node.fire('change');s.ack();
 s.node.value='2026-12-01';s.node.fire('input');assert.equal(s.context.lockHomeCalendars(s.expected()),null);s.ack();
 const expected=s.expected();s.node.value='2027-01-01';assert.equal(s.context.lockHomeCalendars(expected),null);s.node.value='2026-12-01';
 const release=s.context.lockHomeCalendars(expected);assert.equal(typeof release,'function');
 assert.equal(s.entry.selected,'2026-11-01');assert.equal(s.node.value,'2026-12-01');assert.equal(s.commits().length,1);release();
 assert.equal(s.node.value,'2026-12-01');assert.equal(s.entry.selected,'2026-11-01');
});


test('lease captured before received progress refuses old stamp and accepts fresh exact stamp',()=>{
 const s=setup(),before=s.expected();
 s.node.value='2026-10-30';s.node.fire('input');s.ack();
 assert.equal(s.context.lockHomeCalendars(before),null);
 // Re-reading only Busy after acknowledgement cannot explain the old failure.
 assert.equal(!!s.entry.lease||s.entry.acked<s.entry.seq,false);
 const after=s.expected();assert.ok(after[0].seq>before[0].seq);
 const release=s.context.lockHomeCalendars(after);assert.equal(typeof release,'function');release();
 assert.equal(s.commits().length,0);
 s.node.value='2026-10-31';assert.equal(s.context.lockHomeCalendars(after),null,'unreported value still refuses');
 s.node.value='2026-10-30';s.node.validity.badInput=true;assert.equal(s.context.lockHomeCalendars(after),null);
 s.node.validity.badInput=false;s.home.dataset.revision='3';assert.equal(s.context.lockHomeCalendars(after),null);
});
