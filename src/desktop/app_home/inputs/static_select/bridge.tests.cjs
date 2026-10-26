// Actual bridge evaluation with a narrow DOM double. This does not simulate
// browser focus, native popup selection or physical input.
const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
function setup(optional=true,selected=0){
 const nodes=new Map(),observers=[],events=[],pending=new Set();
 const mutated=(target,type,attributeName)=>{for(const o of observers){const c=o.options;if(!c||!(o.target===target||c.subtree&&o.target.contains(target)))continue;if(type==='childList'&&c.childList||type==='attributes'&&c.attributes&&(!c.attributeFilter||c.attributeFilter.includes(attributeName)))pending.add(o);}};
 const flush=(limit=20)=>{let turns=0;while(pending.size&&turns<limit){const batch=[...pending];pending.clear();for(const o of batch)if(o.options)o.cb();turns++;}assert.equal(pending.size,0,'MutationObserver queue did not quiesce');return turns;};
 class Node{
  constructor(tag){this.tagName=tag.toUpperCase();this.children=[];this.dataset={};this.listeners=new Map();this.disabled=false;this.hidden=false;this.isConnected=true;this._value='';this.attributes={};this.parentNode=null;this.textContent='';}
  set id(v){if(this._id)nodes.delete(this._id);this._id=v;nodes.set(v,this)}get id(){return this._id}
  get options(){return this.children}
  get value(){return this._value}set value(v){this._value=v}
  // DOM textContent replaces children even when assigned the same text.
  get textContent(){return this._text||''}set textContent(v){this._text=v;mutated(this,'childList')}
  appendChild(n){n.parentNode=this;this.children.push(n);mutated(this,'childList')}
  contains(n){return n===this||this.children.some(c=>c.contains(n))}
  setAttribute(k,v){this.attributes[k]=v;mutated(this,'attributes',k)}
  addEventListener(type,fn,capture=false){const list=this.listeners.get(type)||[];list.push({fn,capture});this.listeners.set(type,list)}
  removeEventListener(type,fn,capture=false){this.listeners.set(type,(this.listeners.get(type)||[]).filter(x=>x.fn!==fn||x.capture!==capture))}
  fire(type,props={}){const e={type,isTrusted:false,prevented:false,stopped:false,preventDefault(){this.prevented=true},stopImmediatePropagation(){this.stopped=true},...props};for(const l of [...(this.listeners.get(type)||[])].sort((a,b)=>Number(b.capture)-Number(a.capture))){l.fn(e);if(e.stopped)break}return e}
 }
 const home=new Node('section');home.id='app-home';home.dataset={visit:'1',revision:'2'};
 const host=new Node('span');host.id='host';host.dataset={homeStaticMount:'3',nativeId:'pick',placeholder:'Choose',locked:'false'};home.appendChild(host);
 const window={};const document={getElementById:id=>nodes.get(id),createElement:tag=>new Node(tag),body:home};
 const context=vm.createContext({window,document,MutationObserver:class{constructor(cb){this.cb=cb;observers.push(this)}observe(target,options){this.target=target;this.options=options}disconnect(){this.options=null;pending.delete(this)}}});
 vm.runInContext(fs.readFileSync(process.env.STATIC_BRIDGE_SOURCE||__dirname+'/bridge.js','utf8'),context);
 const config={host:'host',mount:3,visit:1,revision:2,optional,selected,options:[{value:'a',label:'Alpha',group:'Group'},{value:'',label:'Empty',group:'Group'}]};
 const api=context.mountHomeStatic(config,{send:e=>events.push(e)}),entry=window.__spHomeStatic.entries.get(3),node=entry.node;
 api.ack({seq:entry.seq});
 const expected=()=>({visit:1,revision:2,fields:[{id:'pick',mount:3,seq:entry.seq,index:entry.index,options:config.options.map((o,i)=>({value:`option-${i}`,text:`Group — ${o.label}`}))}]});
 return{context,api,entry,node,events,expected,home,host,window,Node,nodes,observers,flush,mutated,config};
}

test('two mounted selects quiesce after subtree and placeholder mutations',()=>{
 const s=setup(),host=new s.Node('span');host.id='second';host.dataset={...s.host.dataset,homeStaticMount:'4',nativeId:'required'};s.home.appendChild(host);
 const api=s.context.mountHomeStatic({...s.config,host:'second',mount:4,optional:false},{send:e=>s.events.push(e)});
 const second=s.window.__spHomeStatic.entries.get(4);api.ack({seq:second.seq});
 assert.ok(s.flush()<=2);assert.equal(s.events.length,2);
 s.home.appendChild(new s.Node('div'));assert.equal(s.flush(),1);
 s.host.dataset.placeholder='Updated';s.mutated(s.host,'attributes','data-placeholder');assert.ok(s.flush()<=2);
 assert.equal(s.node.options[0].textContent,'Updated');assert.equal(second.node.options[0].textContent,'Choose');
 s.host.dataset.locked='true';s.mutated(s.host,'attributes','data-locked');assert.equal(s.flush(),1);assert.equal(s.node.disabled,true);
 assert.equal(s.events.length,2);assert.equal(s.node.value,'option-0');assert.equal(second.node.value,'option-0');
});
test('empty-valued option preserves occurrence index but clears visible/native selection',()=>{
 const s=setup();s.node.value='option-1';s.node.fire('change');
 assert.equal(s.events.at(-1).index,1);assert.equal(s.entry.index,null);assert.equal(s.node.value,'');
 s.api.ack({seq:s.entry.seq});const release=s.context.lockHomeStatic(s.expected());assert.equal(typeof release,'function');release();
});
test('required absent clear is suppressed while actual empty option reaches parent',()=>{
 const s=setup(false);assert.equal(s.node.options[0].disabled,true);assert.equal(s.node.options[0].hidden,true);
 s.node.value='';s.node.fire('change');assert.equal(s.events.length,1);
 s.node.value='option-1';s.node.fire('change');assert.equal(s.events.length,2);assert.equal(s.events.at(-1).index,1);assert.equal(s.entry.index,null);
});
test('pending policy disabled suppresses events independently of lease',()=>{
 const s=setup();s.host.dataset.locked='true';s.observers[0].cb();assert.equal(s.node.disabled,true);
 s.node.fire('change');assert.equal(s.events.length,1);
 s.host.dataset.locked='false';s.observers[0].cb();s.node.fire('change');assert.equal(s.events.length,2);
});
test('lease refuses mismatched value/options/owner',()=>{
 for(const mutate of [s=>s.node.value='option-1',s=>s.node.options[1].textContent='Forged',s=>s.home.dataset.revision='4']){
  const s=setup(),x=s.expected();mutate(s);assert.equal(s.context.lockHomeStatic(x),null);
 }
});
test('published sequence and acknowledgement progress wait for exact current receipt',()=>{
 const s=setup(),old=s.expected();s.node.value='option-1';s.node.fire('change');
 assert.equal(s.context.lockHomeStatic(old),'busy');
 const current=s.expected();assert.equal(s.context.lockHomeStatic(current),'busy');
 s.api.ack({seq:s.entry.seq});assert.equal(s.context.lockHomeStatic(old),'busy');
 const release=s.context.lockHomeStatic(current);assert.equal(typeof release,'function');release();
});
test('busy progress cannot hide unseen buffers, forged source or foreign custody',()=>{
 for(const mutate of [s=>s.node.value='option-0',s=>s.node.options[1].textContent='Forged',s=>s.home.dataset.revision='4',s=>s.entry.closed=true,s=>s.entry.acked=s.entry.seq+1,s=>s.entry.seq=0,s=>s.entry.index=99]){
  const s=setup(),old=s.expected();s.node.value='option-1';s.node.fire('change');mutate(s);assert.equal(s.context.lockHomeStatic(old),null);
 }
 const s=setup(),x=s.expected();s.entry.index=null;s.node.value='';assert.equal(s.context.lockHomeStatic(x),null);
});
test('all siblings are checked before a pending acknowledgement or existing lease returns busy',()=>{
 const s=setup(),quiet=new s.Node('select');quiet.id='quiet';quiet.value='option-0';s.home.appendChild(quiet);
 for(const [value,text]of [['','Choose'],['option-0','Quiet']]){const o=new s.Node('option');o.value=value;o.textContent=text;quiet.appendChild(o)}
 const x=s.expected();x.fields.push({id:'quiet',mount:null,seq:null,index:0,options:[{value:'option-0',text:'Quiet'}]});
 s.entry.acked=0;quiet.value='';assert.equal(s.context.lockHomeStatic(x),null);quiet.value='option-0';assert.equal(s.context.lockHomeStatic(x),'busy');
 s.api.ack({seq:s.entry.seq});const release=s.context.lockHomeStatic(x);assert.equal(typeof release,'function');
 quiet.value='';assert.equal(s.context.lockHomeStatic(x),null);quiet.value='option-0';assert.equal(s.context.lockHomeStatic(x),'busy');release();
});
test('short lease protects quiet inventory and value without disabled or readonly writes',()=>{
 const s=setup(),quiet=new s.Node('select');quiet.id='quiet';quiet.value='option-0';
 for(const [value,text]of [['','Choose'],['option-0','Quiet']]){const o=new s.Node('option');o.value=value;o.textContent=text;quiet.appendChild(o)}s.home.appendChild(quiet);
 const x=s.expected();x.fields.push({id:'quiet',mount:null,seq:null,index:0,options:[{value:'option-0',text:'Quiet'}]});
 quiet.value='';assert.equal(s.context.lockHomeStatic(x),null);quiet.value='option-0';
 let changes=0;quiet.addEventListener('change',()=>changes++);const release=s.context.lockHomeStatic(x);
 assert.equal(typeof release,'function');assert.equal(s.context.lockHomeStatic(x),'busy');assert.equal(s.node.disabled,false);assert.equal(quiet.disabled,false);
 assert.equal(quiet.fire('change').prevented,true);assert.equal(changes,0);assert.equal(quiet.fire('keydown',{key:'Tab'}).prevented,false);
 release();release();quiet.fire('change');assert.equal(changes,1);assert.equal(s.entry.lease,false);
});
test('old idempotent release cannot clear a newer exact lease or mutate replacement',()=>{
 const s=setup();const first=s.context.lockHomeStatic(s.expected());first();const second=s.context.lockHomeStatic(s.expected());const token=s.entry.lease;
 first();assert.equal(s.entry.lease,token);s.home.dataset.revision='3';second();assert.equal(s.entry.lease,false);assert.equal(s.node.value,'option-0');
});


test('date-only empty static inventory needs exact Home identity but no static registry',()=>{
 const s=setup();delete s.window.__spHomeStatic;
 const expected={visit:1,revision:2,fields:[]};
 const release=s.context.lockHomeStatic(expected);
 assert.equal(typeof release,'function');release();release();
 assert.equal(s.window.__spHomeStatic,undefined,'no fabricated registry');
 assert.equal(s.context.lockHomeStatic({...expected,visit:2}),null);
 assert.equal(s.context.lockHomeStatic({...expected,revision:3}),null);
 assert.equal(s.context.lockHomeStatic(s.expected()),null,'nonempty inventory still requires registry');
 s.nodes.delete('app-home');assert.equal(s.context.lockHomeStatic(expected),null);
});
