// Actual private bridge with a DOM double. Native focus/geometry is separately
// verified by the desktop fixture; this suite does not synthesize OS input.
const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const read=p=>fs.readFileSync(__dirname+'/'+p,'utf8');
function setup(optional=true,selected='09:30',locale='en-US',time24=false,focusFlag=null,locked=false){
 const nodes=new Map(),events=[],observers=[],timers=new Map();let document,timerId=0;
 class Node{
  constructor(tag='div'){this.tagName=tag.toUpperCase();this.dataset={};this.style={};this.listeners=new Map();this.children=[];this.attributes={};this.parentNode=null;this.clientLeft=0;this.clientTop=0;this.isConnected=true;this.value='';this.disabled=false;this.readOnly=false;this.validity={valid:true,badInput:false};this.selectionStart=0;this.selectionEnd=0;this.selectionDirection='none';this.classes=new Set();this.classList={toggle:(k,v)=>v?this.classes.add(k):this.classes.delete(k)};}
  set id(value){this._id=value;nodes.set(value,this);}get id(){return this._id;}
  contains(n){for(let p=n;p;p=p.parentNode)if(p===this)return true;return false;}getClientRects(){return this.isConnected?[this.getBoundingClientRect()]:[];}
  get parentElement(){return this.parentNode;}get clientWidth(){return this.getBoundingClientRect().width;}get clientHeight(){return this.getBoundingClientRect().height;}
  appendChild(n){n.parentNode?.removeChild(n);this.children.push(n);n.parentNode=this;return n;}
  removeChild(n){this.children=this.children.filter(x=>x!==n);n.parentNode=null;}
  remove(){this.parentNode?.removeChild(this);}
  replaceChildren(){for(const n of this.children)n.parentNode=null;this.children=[];}
  setAttribute(k,v){this.attributes[k]=String(v);}removeAttribute(k){delete this.attributes[k];}
  getBoundingClientRect(){return this.rect||{top:100,bottom:132,left:0,right:280,width:280,height:32};}
  addEventListener(k,f,capture=false){const a=this.listeners.get(k)||[];a.push({f,capture});this.listeners.set(k,a);}
  removeEventListener(k,f){this.listeners.set(k,(this.listeners.get(k)||[]).filter(x=>x.f!==f));}
  fire(k,props={}){if(k==='blur'&&document.activeElement===this)document.activeElement=document.body;const e={key:'',target:this,prevented:false,stopped:false,immediate:false,preventDefault(){this.prevented=true},stopPropagation(){this.stopped=true},stopImmediatePropagation(){this.immediate=true;this.stopped=true},...props};for(const {f}of [...(this.listeners.get(k)||[])].sort((a,b)=>Number(b.capture)-Number(a.capture))){f(e);if(e.immediate)break;}return e;}
  focus(){if(this.disabled)return;const old=document.activeElement;if(old===this)return;old?.fire?.('blur',{relatedTarget:this});document.activeElement=this;this.fire('focus');}select(){this.selectionStart=0;this.selectionEnd=this.value.length;}
  setSelectionRange(a,b,d){this.selectionStart=a;this.selectionEnd=b;this.selectionDirection=d;}
  scrollIntoView(){}closest(q){return q==='[role=option]'&&this.attributes.role==='option'?this:null;}
 }
 const home=new Node();home.dataset={visit:'1',revision:'2'};nodes.set('app-home',home);
 const host=new Node();host.dataset={homeNativeMount:'3',nativeId:'time',locked:String(locked),label:'label',required:String(!optional)};nodes.set('host',host);
 home.appendChild(host);
 document={querySelectorAll:()=>[],body:home,activeElement:home,getElementById:id=>nodes.get(id),createElement:tag=>new Node(tag)};
 const window={innerHeight:700,innerWidth:1024,listeners:new Map(),addEventListener(k,f){this.listeners.set(k,f)},removeEventListener(k){this.listeners.delete(k)},fire(k){this.listeners.get(k)?.()}};
 const context=vm.createContext({window,document,setTimeout:(fn,ms)=>{timers.set(++timerId,{fn,ms});return timerId},clearTimeout:id=>timers.delete(id),getComputedStyle:n=>({visibility:'visible',overflowX:n.style.overflowX||'visible',overflowY:n.style.overflowY||'visible'}),MutationObserver:class{constructor(f){this.f=f;observers.push(this)}observe(){}disconnect(){}}});
 vm.runInContext(`const timeDayjs=${read('vendor/dayjs.js')};timeDayjs.extend(${read('vendor/custom-parse-format.js')});${read('normalize.js')}\n${read('model.js')}\n${read('bridge.js')}\n${read('../../native.js')}\n${read('../../focus.js')}`,context);
 const config={id:7,host:'host',mount:3,epoch:1,selected,optional,locale,time24,visit:1,revision:2};
 if(focusFlag!==null){config.focus={id:'time',instance:8,kind:'time',flag:focusFlag,visit:1,revision:2,live:[8]};host.dataset.homeFocus=JSON.stringify(config.focus);}
 const api=context.homeTimeNative(config,{send:e=>events.push(e)}),entry=api.entry,node=entry.node;
 const ack=()=>api.ack({mount:entry.mount,seq:entry.seq});ack();
 const type=value=>{node.value=value;node.fire('input');ack();};
 const commits=()=>events.filter(e=>e.kind==='commit');
 const expected=()=>[{id:7,mount:entry.mount,seq:entry.seq,value:node.value}];
 return{timers,flush(){for(const [id,t]of [...timers]){timers.delete(id);t.fn();}},api,entry,node,events,ack,type,commits,expected,host,home,document,context,config,observers,Node,window};
}
test('closed Enter only opens; expanded typed Enter commits once and suppresses implicit form submit',()=>{
 const s=setup();s.node.focus();const first=s.node.fire('keydown',{key:'Enter'});s.ack();
 assert.equal(first.prevented,true);assert.equal(s.entry.expanded,true);assert.equal(s.commits().length,0);
 s.type('5:45 PM');s.node.fire('keydown',{key:'Enter'});s.ack();assert.equal(s.commits().length,1);assert.equal(s.entry.selected,'17:45');
 s.node.fire('blur');s.ack();assert.equal(s.commits().length,1);assert.equal(s.node.value,'5:45 PM');
});
test('optional blank differs from invalid absent candidate; required invalid stays local only',()=>{
 const s=setup(true,null);s.node.focus();s.node.fire('blur');s.ack();s.node.focus();s.node.fire('blur');s.ack();assert.deepEqual(s.commits().map(x=>x.selected),[null,null]);
 s.node.focus();s.type('garbage');assert.equal(s.entry.localInvalid,true);s.node.fire('blur');s.ack();assert.equal(s.commits().length,2);assert.equal(s.entry.localInvalid,false);
 const r=setup(false);r.node.focus();r.type('garbage');r.node.fire('blur');r.ack();assert.equal(r.commits().length,0);assert.equal(r.entry.selected,'09:30');assert.equal(r.node.value,'garbage');assert.equal(r.entry.localInvalid,false);
});
test('Escape restores selected localized label, hourly options remain unfiltered, optional clear is first',()=>{
 const s=setup(true,'09:30','ko-KR');s.node.focus();s.type('17:45');
 const list=s.host.children.find(x=>x.attributes.role==='listbox');assert.equal(list.children.length,25);assert.equal(list.children[0].dataset.timeClear,'true');
 s.node.fire('keydown',{key:'Escape'});s.ack();assert.equal(s.node.value,'\uC624\uC804 9:30');assert.equal(s.entry.expanded,false);assert.equal(s.commits().length,0);
 s.node.fire('click');s.ack();list.children[0].fire('click');s.ack();assert.equal(s.commits().length,1);assert.equal(s.entry.selected,null);
});
test('same-visit props change selected label while preserving focused draft and parent receives no callback',()=>{
 const s=setup();s.node.focus();s.type('17:45');s.node.setSelectionRange(2,4,'backward');
 s.host.dataset.homeNativeMount='4';s.home.dataset.revision='3';const api=s.context.homeTimeNative({...s.config,mount:4,revision:3,selected:'11:00'},{send:e=>s.events.push(e)});
 assert.equal(api.entry.node,s.node);assert.equal(s.node.value,'17:45');assert.equal(api.entry.selected,'11:00');assert.equal(s.commits().length,0);assert.equal(s.node.selectionStart,2);
 const overlay=s.host.children.find(x=>x.className==='home-time-selected');assert.equal(overlay.textContent,'11:00 AM');assert.equal(overlay.hidden,true);
});
test('readonly lease preserves focused node/range, guards edits/Enter, allows Tab and release is exact/idempotent',()=>{
 const s=setup();s.node.focus();s.type('930');s.node.setSelectionRange(1,2,'backward');const before=s.entry.seq;
 const release=s.context.lockHomeCalendars(s.expected());assert.equal(typeof release,'function');assert.equal(s.node.disabled,false);assert.equal(s.node.readOnly,true);assert.equal(s.document.activeElement,s.node);
 assert.equal(s.node.fire('beforeinput').prevented,true);assert.equal(s.node.fire('keydown',{key:'Enter'}).prevented,true);assert.equal(s.node.fire('keydown',{key:'Tab'}).prevented,false);assert.equal(s.entry.seq,before);
 release();assert.equal(s.node.readOnly,false);assert.equal(s.node.selectionStart,1);const next=s.context.lockHomeCalendars(s.expected());release();assert.equal(s.node.readOnly,true);next();assert.equal(s.node.readOnly,false);
});
test('unseen/unacknowledged/IME/stale source cannot acquire or commit and fresh received draft can',()=>{
 const s=setup();const before=s.expected();s.node.value='930';assert.equal(s.context.lockHomeCalendars(before),null);
 s.node.fire('input');assert.equal(s.context.lockHomeCalendars(s.expected()),null);s.ack();const lease=s.context.lockHomeCalendars(s.expected());assert.equal(typeof lease,'function');lease();
 s.node.fire('compositionstart');s.ack();assert.equal(s.context.lockHomeCalendars(s.expected()),null);s.node.fire('keydown',{key:'Enter',isComposing:true});assert.equal(s.commits().length,0);
 s.node.fire('compositionend');s.ack();s.home.dataset.revision='9';assert.equal(s.context.lockHomeCalendars(s.expected()),null);s.node.fire('blur');assert.equal(s.commits().length,0);
});
test('source disabled is separate from lease; required has no clear and list selection uses canonical value',()=>{
 const s=setup(false);s.node.focus();s.node.fire('click');s.ack();const list=s.host.children.find(x=>x.attributes.role==='listbox');assert.equal(list.children.length,24);assert.equal(list.children[0].dataset.timeClear,undefined);
 list.children[10].fire('click');s.ack();assert.equal(s.entry.selected,'10:00');assert.equal(s.commits().length,1);
 s.host.dataset.locked='true';s.observers[0].f([{type:'attributes',target:s.host}]);assert.equal(s.node.disabled,true);s.node.fire('keydown',{key:'Enter'});assert.equal(s.commits().length,1);
});
test('stale release never edits a remounted node or steals later explicit focus',()=>{
 const s=setup();s.node.focus();const release=s.context.lockHomeCalendars(s.expected());
 s.home.dataset.revision='3';s.api.close();release();assert.equal(s.node.readOnly,true,'old source cannot restore a node');
 const other={};s.document.activeElement=other;s.host.dataset.homeNativeMount='4';
 const next=s.context.homeTimeNative({...s.config,mount:4,revision:3},{send:e=>s.events.push(e)});
 assert.equal(next.entry.node,s.node);assert.equal(s.node.readOnly,false,'new current mount owns its editable state');assert.equal(s.document.activeElement,other);
 next.ack({mount:4,seq:next.entry.seq});const held=s.context.lockHomeCalendars(s.expected());release();assert.equal(s.node.readOnly,true);held();assert.equal(s.node.readOnly,false);
});
test('popup fits intersection of Home clipping ancestors rather than outer window space',()=>{
 const s=setup(),outer=new s.Node(),inner=new s.Node();
 outer.rect={left:20,right:330,top:70,bottom:650,width:310,height:580};outer.style.overflowX='hidden';outer.style.overflowY='hidden';
 inner.rect={left:40,right:320,top:260,bottom:540,width:280,height:280};inner.style.overflowY='auto';
 outer.appendChild(inner);inner.appendChild(s.host);s.host.rect={left:30,right:330,top:300,bottom:332,width:300,height:32};
 s.node.focus();s.node.fire('click');s.ack();const list=s.host.children.find(x=>x.attributes.role==='listbox');
 assert.equal(list.classes.has('above'),false);assert.equal(list.style.top,'36px');assert.equal(list.style.maxHeight,'204px');
 assert.equal(list.style.left,'0px');assert.equal(list.style.width,'300px');
 // Scrolling the anchor toward the lower edge flips into the available space
 // above, still wholly below the inner clipping ancestor's top260.
 const seq=s.entry.seq;s.host.rect={left:30,right:330,top:500,bottom:532,width:300,height:32};s.window.fire('scroll');
 assert.equal(list.classes.has('above'),true);assert.equal(list.style.maxHeight,'236px');assert.equal(list.style.top,'-240px');assert.equal(s.entry.seq,seq);
 inner.style.overflowX='clip';s.window.fire('resize');assert.equal(list.style.left,'10px');assert.equal(list.style.width,'280px');
 assert.equal(s.commits().length,0);assert.equal(list.children.length,25);
});
test('viewport-limited popup remains bounded and masking CSS suppresses only the selected placeholder',()=>{
 const s=setup(false);s.window.innerHeight=200;s.node.focus();s.node.fire('click');s.ack();
 const list=s.host.children.find(x=>x.attributes.role==='listbox');assert.equal(list.classes.has('above'),true);assert.equal(list.style.top,'-100px');assert.equal(list.style.maxHeight,'96px');
 const css=read('../../../../styles/app-modal.css');
 assert.match(css,/\.home-time-control \.home-time-masked::placeholder\s*\{\s*color:\s*transparent;/);
 s.type('');s.node.fire('blur');s.ack();assert.equal(s.node.classes.has('home-time-masked'),true);assert.equal(s.entry.selected,'09:30');assert.equal(s.node.value,'');
});


test('Time true mount focuses and opens, selected label selects at 50ms without callback',()=>{
 const s=setup(true,'09:30','en-US',false,true);
 assert.equal(s.document.activeElement,s.node);assert.equal(s.entry.expanded,true);assert.equal(s.entry.focusSelection.pending,true);
 assert.deepEqual([...s.timers.values()].map(t=>t.ms),[50]);s.flush();assert.equal(s.entry.focusSelection.completed,1);
 assert.equal(s.node.selectionStart,0);assert.equal(s.node.selectionEnd,s.node.value.length);assert.equal(s.commits().length,0);
 assert.deepEqual(s.events.map(e=>e.kind),['mounted','input']);assert.equal(s.events.at(-1).raw_dirty,false);
});
test('Time flag updates focus without opening and unchanged true never retries after away',()=>{
 const s=setup(true,'09:30','en-US',false,false),history=s.context.window.__spHomeText.focus.get(8);
 function flag(value){s.config.focus.flag=value;s.host.dataset.homeFocus=JSON.stringify(s.config.focus);s.observers.forEach(o=>o.f([{type:'attributes',target:s.host}]));}
 flag(true);assert.equal(s.document.activeElement,s.node);assert.equal(s.entry.expanded,false);s.flush();
 const other=new s.Node();other.focus();s.ack();const count=history.attempts;flag(true);assert.equal(history.attempts,count);assert.equal(s.document.activeElement,other);
 flag(false);flag(true);assert.equal(s.document.activeElement,s.node);assert.equal(s.entry.expanded,false);
});
test('semantic-disabled Time remains focusable while all mutation/list paths are inert',()=>{
 const s=setup(true,'09:30','en-US',false,false);s.host.dataset.locked='true';s.config.focus.flag=true;s.host.dataset.homeFocus=JSON.stringify(s.config.focus);s.observers.forEach(o=>o.f([{type:'attributes',target:s.host}]));
 assert.equal(s.node.disabled,false);assert.equal(s.node.readOnly,false);assert.equal(s.node.attributes['aria-disabled'],'true');assert.equal(s.document.activeElement,s.node);assert.equal(s.entry.expanded,false);
 const seq=s.entry.seq,expected=s.expected();s.node.value='17:45';s.node.fire('input');s.node.fire('click');assert.equal(s.node.fire('keydown',{key:'Enter'}).prevented,true);s.node.fire('blur');assert.equal(s.entry.seq,seq);assert.equal(s.commits().length,0);const lease=s.context.lockHomeCalendars(expected);assert.equal(typeof lease,'function');lease();assert.equal(s.node.value,expected[0].value);
 s.node.value='23:59';assert.equal(s.context.lockHomeCalendars(expected),null);
});
test('ordinary held remount consumes focus flag without mutation or release focus',()=>{
 const s=setup(true,'09:30','en-US',false,false);s.type('17:45');const raw=s.node.value;const release=s.context.lockHomeCalendars(s.expected());
 s.host.dataset.homeNativeMount='4';s.home.dataset.revision='3';const config={...s.config,mount:4,revision:3,focus:{...s.config.focus,flag:true,revision:3}};s.host.dataset.homeFocus=JSON.stringify(config.focus);
 const api=s.context.homeTimeNative(config,{send:e=>s.events.push(e)});assert.ok(api);assert.equal(api.entry.node,s.node);assert.equal(s.node.readOnly,true);assert.equal(s.node.value,raw);
 api.ack({mount:4,seq:s.entry.seq});const other=new s.Node();other.focus();release();assert.equal(s.node.readOnly,false);assert.equal(s.document.activeElement,other);assert.equal(s.node.value,raw);assert.equal(s.context.window.__spHomeText.focus.get(8).attempts,0);assert.equal(s.timers.size,0);
});
test('source selected update keeps dirty raw and schedules selection without replaying mount expansion',()=>{
 const s=setup(true,'09:30','en-US',false,false);s.node.focus();s.flush();s.type('17:45');s.node.setSelectionRange(2,3);
 s.host.dataset.homeNativeMount='4';s.home.dataset.revision='3';const config={...s.config,mount:4,revision:3,selected:'10:15',focus:{...s.config.focus,revision:3}};s.host.dataset.homeFocus=JSON.stringify(config.focus);
 const api=s.context.homeTimeNative(config,{send:e=>s.events.push(e)});assert.ok(api);assert.equal(s.node.value,'17:45');assert.equal(s.entry.expanded,true);assert.equal(s.entry.rawDirty,true);s.flush();assert.equal(s.node.selectionEnd,5);assert.equal(s.commits().length,0);
});
test('selection timer never focuses and is retired with old exact source',()=>{
 const s=setup(true,'09:30','en-US',false,true),other=new s.Node();other.focus();s.ack();s.flush();assert.equal(s.document.activeElement,other);
 s.node.focus();assert.ok(s.timers.size);s.home.dataset.revision='99';s.flush();assert.equal(s.entry.focusSelection.pending,false);assert.equal(s.commits().length,0);
});


test('fresh logical readdition reseeds raw and repeats mount focus/open, unlike revision rebind',()=>{
 const s=setup(true,'09:30','en-US',false,false);s.type('17:45');s.host.dataset.homeNativeMount='4';s.home.dataset.revision='3';
 const config={...s.config,mount:4,revision:3,selected:'10:15',focus:{...s.config.focus,instance:9,live:[9],flag:true,revision:3}};s.host.dataset.homeFocus=JSON.stringify(config.focus);
 const next=s.context.homeTimeNative(config,{send:e=>s.events.push(e)});assert.ok(next);assert.equal(s.node.value,'10:15 AM');assert.equal(s.entry.rawDirty,false);assert.equal(s.entry.expanded,true);assert.equal(s.document.activeElement,s.node);assert.equal(s.commits().length,0);
});
test('held selected-prop update preserves raw until current release and never writes parent or focuses',()=>{
 const s=setup(true,'09:30','en-US',false,false);s.type('17:45');const release=s.context.lockHomeCalendars(s.expected());
 s.host.dataset.homeNativeMount='4';s.home.dataset.revision='3';const config={...s.config,mount:4,revision:3,selected:'10:15',focus:{...s.config.focus,revision:3}};s.host.dataset.homeFocus=JSON.stringify(config.focus);
 const api=s.context.homeTimeNative(config,{send:e=>s.events.push(e)});assert.equal(s.entry.selected,'09:30');api.ack({mount:4,seq:s.entry.seq});const other=new s.Node();other.focus();release();
 assert.equal(s.entry.selected,'10:15');assert.equal(s.node.value,'17:45');assert.equal(s.document.activeElement,other);assert.equal(s.commits().length,0);assert.equal(s.events.at(-1).kind,'input');assert.equal(s.events.at(-1).raw_dirty,true);
 api.ack({mount:4,seq:s.entry.seq});const newRelease=s.context.lockHomeCalendars(s.expected());release();assert.equal(s.node.readOnly,true);newRelease();assert.equal(s.node.readOnly,false);
});
test('ordinary detached source replacement restores exact draft and range but owned away cancels restoration',()=>{
 const s=setup(true,'09:30','en-US',false,false);s.node.focus();s.flush();s.type('17:45');s.node.setSelectionRange(1,3,'backward');
 s.host.isConnected=false;s.node.isConnected=false;s.document.activeElement=s.home;s.observers.forEach(o=>o.f([{type:'childList',target:s.home}]));
 s.host.isConnected=true;s.node.isConnected=true;s.host.dataset.homeNativeMount='4';s.home.dataset.revision='3';const config={...s.config,mount:4,revision:3,focus:{...s.config.focus,revision:3}};s.host.dataset.homeFocus=JSON.stringify(config.focus);
 const api=s.context.homeTimeNative(config,{send:e=>s.events.push(e)});assert.ok(api);assert.equal(s.document.activeElement,s.node);assert.equal(s.node.value,'17:45');assert.equal(s.node.selectionStart,1);assert.equal(s.node.selectionEnd,3);assert.equal(s.timers.size,0);
 const other=new s.Node();other.focus();api.ack({mount:4,seq:s.entry.seq});s.host.dataset.homeNativeMount='5';s.home.dataset.revision='4';const next={...config,mount:5,revision:4,focus:{...config.focus,revision:4}};s.host.dataset.homeFocus=JSON.stringify(next.focus);s.context.homeTimeNative(next,{send:e=>s.events.push(e)});assert.equal(s.document.activeElement,other);
});


test('disabled true mount focuses without expanding; missing selected mount opens without timer',()=>{
 const s=setup(true,'09:30','en-US',false,true,true);
 assert.equal(s.document.activeElement,s.node);assert.equal(s.entry.expanded,false);assert.equal(s.node.disabled,false);assert.equal(s.node.readOnly,false);assert.equal(s.node.attributes['aria-disabled'],'true');assert.equal(s.timers.size,1);assert.equal(s.entry.focusSelection.pending,true);s.flush();assert.equal(s.node.selectionStart,0);assert.equal(s.node.selectionEnd,s.node.value.length);assert.equal(s.entry.focusSelection.completed,1);assert.equal(s.commits().length,0);assert.deepEqual(s.events.map(e=>e.kind),['mounted']);
 const other=new s.Node();other.focus();s.host.dataset.locked='false';s.observers.forEach(o=>o.f([{type:'attributes',target:s.host}]));assert.equal(s.document.activeElement,other);assert.equal(s.entry.expanded,false);
 const empty=setup(true,null,'en-US',false,true);assert.equal(empty.entry.expanded,true);assert.equal(empty.timers.size,0);assert.equal(empty.commits().length,0);
});
test('mouse focus excludes delayed selection; stale or foreign logical held mount refuses',()=>{
 const s=setup(true,'09:30','en-US',false,false);s.node.fire('mousedown');s.node.focus();assert.equal(s.timers.size,0);s.node.fire('click');s.ack();assert.equal(s.node.selectionEnd,s.node.value.length,'exact onOpen label selection still applies');
 const release=s.context.lockHomeCalendars(s.expected());s.host.dataset.homeNativeMount='4';const next={...s.config,mount:4,focus:{...s.config.focus,instance:99,live:[99]}};s.host.dataset.homeFocus=JSON.stringify(next.focus);assert.equal(s.context.homeTimeNative(next,{send:()=>{}}),null);assert.equal(s.node.readOnly,true);release();
 s.home.dataset.revision='99';assert.equal(s.context.homeTimeNative({...s.config,mount:4},{send:()=>{}}),null);
});


test('semantic disabled is not a delayed selection veto, while readonly lease remains a veto',()=>{
 const s=setup(true,'09:30','en-US',false,false);s.node.focus();assert.equal(s.timers.size,1);s.host.dataset.locked='true';s.observers.forEach(o=>o.f([{type:'attributes',target:s.host}]));s.flush();assert.equal(s.entry.focusSelection.completed,1);assert.equal(s.node.selectionEnd,s.node.value.length);assert.equal(s.entry.expanded,false);assert.equal(s.commits().length,0);
 const q=setup(true,'09:30','en-US',false,false);q.node.focus();const release=q.context.lockHomeCalendars(q.expected());q.node.setSelectionRange(1,2);q.flush();assert.equal(q.entry.focusSelection.completed,0);assert.equal(q.node.selectionStart,1);assert.equal(q.node.selectionEnd,2);release();assert.equal(q.timers.size,0);
});


test('ordinary revision-stale blur before stop cannot consume detached draft focus intent',()=>{
 const s=setup(true,'09:30','en-US',false,false);s.node.focus();s.flush();s.type('17:45');s.node.setSelectionRange(1,4,'backward');
 // Native source retirement fires blur while the old node can still report
 // connected; this occurs before the child-list observer invokes stop.
 s.home.dataset.revision='3';s.node.fire('blur');assert.equal(s.entry.focused,true);assert.equal(s.entry.restoreFocus,true);assert.equal(s.commits().length,0);
 s.host.isConnected=false;s.node.isConnected=false;s.observers.forEach(o=>o.f([{type:'childList',target:s.home}]));
 s.host.isConnected=true;s.node.isConnected=true;s.host.dataset.homeNativeMount='4';const config={...s.config,mount:4,revision:3,focus:{...s.config.focus,revision:3}};s.host.dataset.homeFocus=JSON.stringify(config.focus);
 const api=s.context.homeTimeNative(config,{send:e=>s.events.push(e)});assert.ok(api);assert.equal(s.document.activeElement,s.node);assert.equal(s.node.value,'17:45');assert.equal(s.node.selectionStart,1);assert.equal(s.node.selectionEnd,4);assert.equal(s.entry.rawDirty,true);assert.equal(s.commits().length,0);assert.equal(s.timers.size,0);
 const other=new s.Node();other.focus();api.ack({mount:4,seq:s.entry.seq});assert.equal(s.entry.restoreFocus,false,'actual owned focus away still cancels restoration');
});
