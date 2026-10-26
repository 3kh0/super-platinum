// Actual private bridge with a DOM double. Native focus/geometry is separately
// verified by the desktop fixture; this suite does not synthesize OS input.
const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const read=p=>fs.readFileSync(__dirname+'/'+p,'utf8');
function setup(optional=true,selected=1792935057,locale='en-US',time24=false,fixedNow=null){
 const nodes=new Map(),events=[],observers=[];let document;
 class Node{
  constructor(tag='div'){this.tagName=tag.toUpperCase();this.dataset={};this.style={};this.listeners=new Map();this.children=[];this.attributes={};this.parentNode=null;this.clientLeft=0;this.clientTop=0;this.isConnected=true;this.value='';this.disabled=false;this.readOnly=false;this.validity={valid:true,badInput:false};this.selectionStart=0;this.selectionEnd=0;this.selectionDirection='none';this.classes=new Set();this.classList={toggle:(k,v)=>v?this.classes.add(k):this.classes.delete(k)};}
  querySelector(){return null;}
  get parentElement(){return this.parentNode;}get clientWidth(){return this.getBoundingClientRect().width;}get clientHeight(){return this.getBoundingClientRect().height;}
  appendChild(n){n.parentNode?.removeChild(n);this.children.push(n);n.parentNode=this;return n;}
  removeChild(n){this.children=this.children.filter(x=>x!==n);n.parentNode=null;}
  remove(){this.parentNode?.removeChild(this);}
  replaceChildren(){for(const n of this.children)n.parentNode=null;this.children=[];}
  setAttribute(k,v){this.attributes[k]=String(v);}removeAttribute(k){delete this.attributes[k];}
  getBoundingClientRect(){return this.rect||{top:100,bottom:132,left:0,right:280,width:280,height:32};}
  addEventListener(k,f,capture=false){const a=this.listeners.get(k)||[];a.push({f,capture});this.listeners.set(k,a);}
  removeEventListener(k,f){this.listeners.set(k,(this.listeners.get(k)||[]).filter(x=>x.f!==f));}
  fire(k,props={}){const e={key:'',target:this,prevented:false,stopped:false,immediate:false,preventDefault(){this.prevented=true},stopPropagation(){this.stopped=true},stopImmediatePropagation(){this.immediate=true;this.stopped=true},...props};for(const {f}of [...(this.listeners.get(k)||[])].sort((a,b)=>Number(b.capture)-Number(a.capture))){f(e);if(e.immediate)break;}return e;}
  focus(){document.activeElement=this;this.fire('focus');}select(){this.selectionStart=0;this.selectionEnd=this.value.length;}
  setSelectionRange(a,b,d){this.selectionStart=a;this.selectionEnd=b;this.selectionDirection=d;}
  scrollIntoView(){}closest(q){return q==='[role=option]'&&this.attributes.role==='option'?this:null;}
 }
 const home=new Node();home.dataset={visit:'1',revision:'2'};nodes.set('app-home',home);
 const host=new Node();host.dataset={homeNativeMount:'3',nativeId:'time',locked:'false',label:'label',required:String(!optional)};nodes.set('host',host);
 document={body:home,activeElement:home,getElementById:id=>nodes.get(id)||[...nodes.values()].flatMap(function all(n){return [n,...n.children.flatMap(all)]}).find(n=>n.id===id),createElement:tag=>new Node(tag)};
 const window={innerHeight:700,innerWidth:1024,listeners:new Map(),addEventListener(k,f){this.listeners.set(k,f)},removeEventListener(k){this.listeners.delete(k)},fire(k){this.listeners.get(k)?.()}};
 let clock=0;const timers=[];
 const tick=async ms=>{clock+=ms;for(const t of timers.splice(0))if(t.at<=clock)t.fn();else timers.push(t);await Promise.resolve();await Promise.resolve();};
 const RealDate=Date;const ClockDate=fixedNow===null?RealDate:class extends RealDate{constructor(...args){super(...(args.length?args:[fixedNow]));}static now(){return fixedNow;}};
 const context=vm.createContext({Date:ClockDate,window,document,performance:{now:()=>clock},setTimeout:(fn,ms)=>timers.push({fn,at:clock+ms}),getComputedStyle:n=>({overflowX:n.style.overflowX||'visible',overflowY:n.style.overflowY||'visible'}),MutationObserver:class{constructor(f){this.f=f;observers.push(this)}observe(){}disconnect(){}}});
 vm.runInContext(`const timeDayjs=${read('../time/vendor/dayjs.js')};timeDayjs.extend(${read('../time/vendor/custom-parse-format.js')});${read('../time/normalize.js')}\n${read('../time/model.js')}\n${read('../time/bridge.js')}\nconst homeDateLocales=${read('../date/locales.json')};${read('../date/model.js')}\nconst homeDatePopupLocales=${read('../date/popup-locales.json')};${read('../date/popup-model.js')}\n${read('../date/popup.js')}\n${read('date-child.js')}\n${read('model.js')}\nfunction homeDatetimeCaption(){return 'Time zone: Test local zone';}\n${read('bridge.js')}\n${read('../../native.js')}`,context);
 const config={id:7,host:'host',mount:3,epoch:1,selected,optional,locale,time24,visit:1,revision:2,focus_on_load:true};
 const api=context.homeDatetimeNative(config,{send:e=>events.push(e)}),entry=api.entry,node=entry.node;
 const ack=()=>api.ack({mount:entry.mount,seq:entry.seq});ack();
 const type=value=>{node.value=value;node.fire('input');ack();};
 const commits=()=>events.filter(e=>e.kind==='commit');
 const expected=()=>[{id:7,mount:entry.mount,seq:entry.seq,value:entry.sentValue}];
 const mount=config=>(context.homeDatetimeMount||context.homeDatetimeNative)(config,{send:e=>events.push(e)});
 return{api,entry,node,date:entry.date,picker:entry.datePicker,events,ack,type,commits,expected,host,home,document,context,config,observers,Node,window,tick,timers,mount};
}

test('source replacement waits for the old composite lease, then publishes and reacquires both editable children',async()=>{
 const s=setup(),release=s.context.lockHomeCalendars(s.expected());assert.equal(typeof release,'function');
 s.api.close();s.host.dataset.homeNativeMount='4';s.home.dataset.revision='3';
 const before=s.events.length,pending=Promise.resolve(s.mount({...s.config,mount:4,revision:3}));
 await s.tick(10);assert.equal(s.events.length,before);
 release();assert.equal(s.date.readOnly,true); // Retired owner cannot write this node.
 await s.tick(10);const api=await pending;assert.ok(api);
 assert.equal(api.entry.mount,4);assert.equal(s.events.at(-1).kind,'mounted');
 api.ack({mount:4,seq:api.entry.seq});assert.equal(api.entry.pending,false);
 assert.equal(s.date.readOnly,false);assert.equal(s.node.readOnly,false);
 const next=s.context.lockHomeCalendars([{id:7,mount:4,seq:api.entry.seq,value:api.entry.sentValue}]);assert.equal(typeof next,'function');
 const token=api.entry.lease;release();assert.equal(api.entry.lease,token);assert.equal(s.date.readOnly,true);assert.equal(s.node.readOnly,true);
 next();assert.equal(s.date.readOnly,false);assert.equal(s.node.readOnly,false);
});

test('waiting registration abandons a replaced host, foreign source, or changed mount without publication',async()=>{
 for(const change of [s=>{s.host.isConnected=false;},s=>{s.home.dataset.revision='4';},s=>{s.host.dataset.homeNativeMount='5';}]){
  const s=setup(),release=s.context.lockHomeCalendars(s.expected());s.api.close();
  s.host.dataset.homeNativeMount='4';s.home.dataset.revision='3';const before=s.events.length;
  const pending=Promise.resolve(s.mount({...s.config,mount:4,revision:3}));change(s);await s.tick(10);
  assert.equal(await pending,null);release();await s.tick(10);assert.equal(s.events.length,before);assert.equal(s.entry.mount,3);
 }
});

test('registration timeout refuses without stealing an old lease or retrying after its deadline',async()=>{
 const s=setup(),release=s.context.lockHomeCalendars(s.expected()),token=s.entry.lease;s.api.close();
 s.host.dataset.homeNativeMount='4';s.home.dataset.revision='3';const before=s.events.length;
 const pending=Promise.resolve(s.mount({...s.config,mount:4,revision:3}));await s.tick(2000);
 assert.equal(await pending,null);assert.equal(s.entry.lease,token);assert.equal(s.events.length,before);
 release();await s.tick(10);assert.equal(s.events.length,before);assert.equal(s.timers.length,0);
});

test('incompatible entry identity refuses immediately rather than using the transient lease retry',async()=>{
 for(const mutate of [s=>{s.entry.epoch=2;},s=>{s.entry.datetime=false;}]){
  const s=setup(),release=s.context.lockHomeCalendars(s.expected());s.api.close();s.host.dataset.homeNativeMount='4';s.home.dataset.revision='3';mutate(s);
  assert.equal(await s.mount({...s.config,mount:4,revision:3}),null);assert.equal(s.timers.length,0);release();
 }
});
test('one real field owns both children; date callback preserves dirty Time and subsequent blur uses new date',()=>{
 const s=setup();assert.equal(s.context.window.__spHomeCalendarNative.entries.size,1);
 s.node.focus();s.type('17:45');const old=s.entry.selected;
 const before=s.commits().length;s.date.value='2026-10-26';s.date.fire('change');s.ack();
 assert.equal(s.commits().length,before+1);assert.equal(s.node.value,'17:45');assert.notEqual(s.entry.selected,old);
 const local=new Date(s.entry.selected*1000);assert.equal(local.getDate(),26);assert.equal(local.getHours(),new Date(old*1000).getHours());assert.equal(local.getSeconds(),0);
 s.node.fire('blur');s.ack();const later=new Date(s.entry.selected*1000);assert.equal(later.getDate(),26);assert.equal(later.getHours(),17);assert.equal(later.getMinutes(),45);
 assert.equal(s.events.at(-1).value,JSON.stringify([s.date.value,s.picker.value,s.node.value]));
});
test('closed/expanded Enter uses original Time leaf without implicit form submission; same canonical value suppresses',()=>{
 const s=setup();s.node.focus();assert.equal(s.node.fire('keydown',{key:'Enter'}).prevented,true);s.ack();assert.equal(s.commits().length,0);
 s.type('5pm');s.node.fire('keydown',{key:'Enter'});s.ack();assert.equal(s.commits().length,1);assert.equal(new Date(s.entry.selected*1000).getHours(),17);
 s.node.fire('blur');s.ack();assert.equal(s.commits().length,1);
});
test('signed epoch callback retained while both selected child props become blank; required clear not invented',()=>{
 const s=setup();s.date.value='1969-12-30';s.date.fire('change');s.ack();assert.ok(s.entry.selected<0);assert.equal(s.date.value,'');assert.equal(s.entry.leaf.selected,null);
 const r=setup(false);r.date.value='';r.date.fire('change');r.ack();assert.equal(r.commits().length,0);assert.notEqual(r.date.value,'');
 r.node.focus();r.type('');r.node.fire('blur');r.ack();assert.equal(r.commits().length,0);
});
test('optional clear and explicit blank Time blur preserve repeat-null semantics; local invalid text is not unseen browser input',()=>{
 const s=setup();s.date.value='';s.date.fire('change');s.ack();assert.equal(s.entry.selected,null);assert.equal(s.commits().length,1);
 s.node.focus();s.type('');s.node.fire('blur');s.ack();s.node.focus();s.node.fire('blur');s.ack();assert.equal(s.commits().length,3);
 s.node.focus();s.type('garbage');assert.equal(s.entry.localInvalid,true);const held=s.context.lockHomeCalendars(s.expected());assert.equal(typeof held,'function');held();
});
test('aggregate lease requires exact acknowledged buffers and current source, protects both children, releases once',()=>{
 const s=setup(),old=s.expected();s.date.value='2026-10-26';assert.equal(s.context.lockHomeCalendars(old),null);
 s.date.fire('input');assert.equal(s.context.lockHomeCalendars(s.expected()),null);s.ack();
 const release=s.context.lockHomeCalendars(s.expected());assert.equal(typeof release,'function');assert.equal(s.date.readOnly,true);assert.equal(s.node.readOnly,true);
 const seq=s.entry.seq;s.date.fire('change');s.node.fire('keydown',{key:'Enter'});assert.equal(s.entry.seq,seq);
 release();release();assert.equal(s.date.readOnly,false);assert.equal(s.node.readOnly,false);
 s.node.value='unseen';assert.equal(s.context.lockHomeCalendars(s.expected()),null);
 s.node.fire('input');s.ack();s.home.dataset.revision='9';assert.equal(s.context.lockHomeCalendars(s.expected()),null);s.node.fire('blur');assert.equal(s.commits().length,0);
});
test('source remount preserves same-visit drafts but old events and old release cannot affect new lease',()=>{
 const s=setup();s.node.focus();s.type('17:45');const oldNode=s.node,oldDate=s.date;
 s.api.close();s.host.dataset.homeNativeMount='4';s.home.dataset.revision='3';
 const api=s.context.homeDatetimeNative({...s.config,mount:4,revision:3},{send:e=>s.events.push(e)});api.ack({mount:4,seq:api.entry.seq});
 assert.equal(api.entry.node,oldNode);assert.equal(api.entry.date,oldDate);assert.equal(oldNode.value,'17:45');assert.equal(s.context.window.__spHomeCalendarNative.entries.size,1);
 const seq=api.entry.seq;s.api.ack({mount:3,seq:99});assert.equal(api.entry.seq,seq);assert.equal(api.entry.acked,seq);
});
test('native badInput and composing never acquire even with exact observed buffers',()=>{
 const s=setup();s.picker.validity={valid:false,badInput:true};s.date.fire('input');s.ack();assert.equal(s.events.at(-1).valid,false);assert.equal(s.context.lockHomeCalendars(s.expected()),null);
 s.picker.validity={valid:true,badInput:false};s.date.fire('input');s.ack();s.node.fire('compositionstart');s.ack();assert.equal(s.context.lockHomeCalendars(s.expected()),null);
 s.node.fire('keydown',{key:'Enter',isComposing:true});assert.equal(s.commits().length,0);
});

test('focused Date draft survives changed selected date and optional Time clear, then blur commits against latest epoch',()=>{
 for(const selected of [1792935057,0]){
  const s=setup(true,selected);s.date.focus();s.date.value='2026-11-02';s.date.fire('input');s.ack();
  // Direct leaf callbacks under a focused Date model the independently executed
  // retained component methods, not simultaneous physical focus on two nodes.
  s.type('17:45');s.node.fire('keydown',{key:'Enter'});s.ack();s.node.fire('keydown',{key:'Enter'});s.ack();
  assert.equal(s.date.value,'2026-11-02');assert.equal(new Date(s.entry.selected*1000).getHours(),17);
  s.date.fire('blur');s.ack();assert.equal(new Date(s.entry.selected*1000).getDate(),2);assert.equal(new Date(s.entry.selected*1000).getHours(),17);
 }
 const s=setup();s.date.focus();s.date.value='2026-11-02';s.date.fire('input');s.ack();
 s.type('');s.node.fire('keydown',{key:'Enter'});s.ack();s.node.fire('keydown',{key:'Enter'});s.ack();
 assert.equal(s.entry.selected,null);assert.equal(s.date.value,'2026-11-02');s.date.fire('blur');s.ack();assert.equal(new Date(s.entry.selected*1000).getDate(),2);
});

test('manual localized Date focus selects without action or popup and does not honor a Home autofocus flag',()=>{
 const s=setup();assert.equal(s.date.type,'text');assert.equal(s.picker.type,'date');
 assert.notEqual(s.document.activeElement,s.date);assert.notEqual(s.document.activeElement,s.node);
 const count=s.events.length;s.date.focus();s.ack();
 assert.equal(s.events.length,count+1);assert.equal(s.events.at(-1).kind,'input');
 assert.equal(s.date.selectionStart,0);assert.equal(s.date.selectionEnd,s.date.value.length);
 assert.match(s.date.value,/October/);assert.equal(s.entry.pickerStatus,'idle');assert.equal(s.commits().length,0);
 assert.equal(s.entry.dateFocused,true);assert.equal(s.entry.dateRawDirty,false);
});

test('German Date Enter keeps literal, resets seconds, preserves Time query and blur dedupes',()=>{
 const s=setup(true,1792946262,'de-DE',true);s.type('17:45');s.date.focus();s.ack();
 assert.match(s.date.value,/25\. Oktober 2026/);
 s.date.value='5. November 2026';s.date.fire('input');s.ack();assert.equal(s.commits().length,0);
 assert.equal(s.date.fire('keydown',{key:'Enter'}).prevented,true);s.ack();
 assert.equal(s.date.value,'5. November 2026');assert.equal(s.picker.value,'2026-11-05');
 assert.equal(s.node.value,'17:45');assert.equal(new Date(s.entry.selected*1000).getSeconds(),0);
 s.date.fire('blur');s.ack();assert.equal(s.commits().length,1);
});

test('optional invalid Date stays literal on Enter and every empty-epoch reattempt can clear again',()=>{
 const s=setup(true,0);s.date.focus();s.ack();s.date.value='not a date';s.date.fire('input');s.ack();
 assert.equal(s.entry.dateBufferValid(),false);assert.equal(s.context.lockHomeCalendars(s.expected()),null);
 s.date.fire('keydown',{key:'Enter'});s.ack();
 assert.equal(s.entry.selected,null);assert.equal(s.date.value,'not a date');assert.equal(s.picker.value,'');
 assert.equal(s.entry.dateResolvedRaw,'not a date');assert.equal(s.entry.dateResolvedSelected,null);
 assert.equal(s.entry.dateBufferValid(),true);const release=s.context.lockHomeCalendars(s.expected());assert.equal(typeof release,'function');release();
 s.date.fire('keydown',{key:'Enter'});s.ack();s.date.fire('blur');s.ack();
 assert.equal(s.commits().length,3);assert.equal(s.date.value,'');assert.ok(s.commits().every(e=>e.selected===null));
});

test('required invalid Date resolves only an actual prior date; missing prior stays blocked',()=>{
 for(const initial of [1792946262,0]){
  const s=setup(false,initial);s.date.focus();s.ack();s.date.value='not a date';s.date.fire('input');s.ack();
  s.date.fire('keydown',{key:'Enter'});s.ack();assert.equal(s.commits().length,0);assert.equal(s.date.value,'not a date');
  assert.equal(s.entry.dateBufferValid(),initial>0);
  assert.equal(s.entry.dateResolvedRaw,initial>0?'not a date':null);
  s.date.fire('keydown',{key:'Escape'});s.ack();assert.notEqual(s.date.value,'not a date');assert.equal(s.commits().length,0);
 }
});

test('resolved invalid pair cannot authorize new unobserved text, new input or a changed Date prop',()=>{
 const s=setup(false);s.date.focus();s.ack();s.date.value='not a date';s.date.fire('input');s.ack();s.date.fire('keydown',{key:'Enter'});s.ack();
 assert.equal(s.entry.dateBufferValid(),true);s.date.value='different invalid';assert.equal(s.context.lockHomeCalendars(s.expected()),null);
 s.date.fire('input');s.ack();assert.equal(s.entry.dateResolvedRaw,null);assert.equal(s.entry.dateBufferValid(),false);
 s.date.fire('keydown',{key:'Enter'});s.ack();
 s.api.close();s.host.dataset.homeNativeMount='4';s.home.dataset.revision='3';
 const api=s.context.homeDatetimeNative({...s.config,mount:4,revision:3,selected:1793900220},{send:e=>s.events.push(e)});api.ack({mount:4,seq:api.entry.seq});
 assert.equal(api.entry.date.value,'different invalid');assert.equal(api.entry.dateResolvedRaw,null);assert.equal(api.entry.dateBufferValid(),false);
});

test('auxiliary chooser remains independently acknowledged and locked after popup integration',()=>{
 const s=setup();s.date.focus();s.ack();
 // Popup DOM is covered by popup.tests; this small host has no trigger.
 assert.equal(s.commits().length,0);s.date.fire('keydown',{key:'Escape'});s.ack();assert.equal(s.entry.pickerStatus,'idle');
 const old=s.expected();s.picker.value='2026-11-03';assert.equal(s.context.lockHomeCalendars(old),null);
 s.picker.fire('input');s.ack();const release=s.context.lockHomeCalendars(s.expected());assert.equal(typeof release,'function');
 assert.equal(s.picker.readOnly,true);assert.equal(s.date.readOnly,true);assert.equal(s.node.readOnly,true);
 const seq=s.entry.seq;s.picker.fire('change');s.date.fire('keydown',{key:'Escape'});assert.equal(s.entry.seq,seq);release();
 s.picker.fire('change');s.ack();assert.equal(s.commits().length,1);assert.equal(s.document.activeElement,s.date);
 assert.equal(s.events.at(-1).value,JSON.stringify([s.date.value,s.picker.value,s.node.value]));
});

test('Date source reattachment preserves exact draft and selection without replaying focus reset',()=>{
 const s=setup();s.date.focus();s.ack();s.date.value='November 3rd, 2026';s.date.fire('input');s.ack();s.date.setSelectionRange(2,8,'forward');
 s.home.dataset.revision='3';s.date.fire('blur',{relatedTarget:null}); // Stale source cannot consume focus intent.
 s.date.isConnected=false;s.document.activeElement=s.document.body;s.api.close();
 s.host.dataset.homeNativeMount='4';s.date.isConnected=true;
 const api=s.context.homeDatetimeNative({...s.config,mount:4,revision:3},{send:e=>s.events.push(e)});api.ack({mount:4,seq:api.entry.seq});
 assert.equal(s.document.activeElement,s.date);assert.equal(s.date.value,'November 3rd, 2026');
 assert.deepEqual([s.date.selectionStart,s.date.selectionEnd,s.date.selectionDirection],[2,8,'forward']);assert.equal(s.commits().length,0);
 const release=s.context.lockHomeCalendars(s.expected());assert.equal(typeof release,'function');
 s.document.activeElement=s.document.body;s.date.fire('blur',{relatedTarget:s.document.body});release();assert.equal(s.document.activeElement,s.document.body);
});

test('actual composite localized and invalid editing vectors match the production bridge',()=>{
 const zone=process.env.TZ||Intl.DateTimeFormat().resolvedOptions().timeZone;
 assert.ok(['UTC','America/New_York'].includes(zone),'Run this bounded differential in an executed reference zone');
 const reference=JSON.parse(fs.readFileSync(`tmp/app-home-datetime-editing-20261026/reference/vectors-${zone.replaceAll('/','_')}.json`,'utf8'));
 const fixed=Date.parse('2026-10-26T12:00:00Z');let count=0;
 for(const row of reference.results){
  if(row.name.startsWith('localized-focus-and-Enter-')){
   const r=row.evidence,s=setup(true,Number(r.before.epoch),r.locale,false,fixed);
   assert.equal(s.date.value,r.before.display,row.name);s.date.focus();s.ack();assert.equal(s.date.value,r.focused.display,row.name);
   s.date.value=r.label;s.date.fire('input');s.ack();s.date.fire('keydown',{key:'Enter'});s.ack();
   assert.equal(s.entry.selected,Number(r.after.epoch),row.name);assert.equal(s.date.value,r.after.display,row.name);
   s.date.fire('blur');s.ack();assert.equal(s.commits().length,r.blur.actions.length,row.name);count++;
  }else if(/^(enter|blur)-/.test(row.name)){
   const r=row.evidence,optional=!row.name.includes('required'),s=setup(optional,Number(r.before.epoch),'en-US',false,fixed);
   s.date.focus();s.ack();s.date.value=r.before.raw;s.date.fire('input');s.ack();
   if(row.name.startsWith('enter-'))s.date.fire('keydown',{key:'Enter'});else s.date.fire('blur');s.ack();
   assert.equal(s.entry.selected||null,Number(r.after.epoch)||null,row.name);
   assert.equal(s.date.value,r.after.display,row.name);assert.equal(s.commits().length,r.after.actions.length,row.name);count++;
  }
 }
 assert.equal(count,14);
});
