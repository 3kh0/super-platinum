// DOM test host for actual composite and shared popup adapters; not a browser/pixel claim.
const fs=require('node:fs'),vm=require('node:vm');
const read=p=>fs.readFileSync(process.env.DATETIME_POPUP_BEFORE&&['bridge.js','date-child.js'].includes(p)?process.env.DATETIME_POPUP_BEFORE+'/'+p:__dirname+'/'+p,'utf8');
function setup(optional=true,selected=1792946262,locale='en-US'){
 let document;const nodes=new Map(),events=[],observers=[];let clock=0;
 class Node{
  constructor(tag='div'){this.tagName=tag.toUpperCase();this.dataset={};this.children=[];this.parentNode=null;this.isConnected=true;this.value='';this.disabled=false;this.readOnly=false;this.validity={valid:true,badInput:false};this.attributes={};this.listeners=new Map();this.selectionStart=0;this.selectionEnd=0;this.selectionDirection='none';this.focusCalls=0;this.style={};this.classList={add:()=>{},toggle:()=>{}};}
  set id(v){this._id=v;nodes.set(v,this);}get id(){return this._id;}
  appendChild(n){n.parentNode?.removeChild(n);this.children.push(n);n.parentNode=this;n.isConnected=this.isConnected;return n;}
  insertBefore(n,b){if(!b)return this.appendChild(n);n.parentNode?.removeChild(n);const index=this.children.indexOf(b);if(index<0)throw Error('Foreign reference node');this.children.splice(index,0,n);n.parentNode=this;n.isConnected=this.isConnected;return n;}
  removeChild(n){this.children=this.children.filter(v=>v!==n);n.parentNode=null;n.isConnected=false;if(document&&n.contains(document.activeElement))document.activeElement=document.body;}
  remove(){this.parentNode?.removeChild(this);}
  replaceChildren(){for(const n of [...this.children])this.removeChild(n);}
  get parentElement(){return this.parentNode;}get clientWidth(){return this.getBoundingClientRect().width;}get clientHeight(){return this.getBoundingClientRect().height;}
  scrollIntoView(){}
  get firstElementChild(){return this.children[0]||null;}
  cloneNode(){const n=new Node(this.tagName);n.className=this.className;return n;}
  setAttribute(k,v){this.attributes[k]=String(v);}removeAttribute(k){delete this.attributes[k];}
  getAttribute(k){return this.attributes[k]??null;}
  matches(selector){const tag=selector.match(/^[a-z]+/);if(tag&&this.tagName!==tag[0].toUpperCase())return false;return [...selector.matchAll(/\[([^=\]]+)(?:="([^"]*)")?\]/g)].every(([,k,v])=>{const value=k.startsWith('data-')?this.dataset[k.slice(5).replace(/-([a-z])/g,(_,c)=>c.toUpperCase())]??this.getAttribute(k):this.getAttribute(k);return v===undefined?value!==undefined&&value!==null:String(value)===v;});}
  querySelectorAll(selector){return this.children.flatMap(c=>[...(c.matches(selector)?[c]:[]),...c.querySelectorAll(selector)]);}
  querySelector(selector){return this.querySelectorAll(selector)[0]||null;}
  getBoundingClientRect(){return this.tagName==='SECTION'?{left:0,right:292,top:0,bottom:300,width:292,height:300}:{left:50,right:450,top:100,bottom:140,width:400,height:40};}
  contains(n){return n===this||this.children.some(c=>c.contains(n));}
  getClientRects(){return [{}];}closest(selector){for(let n=this;n;n=n.parentNode)if(selector.split(',').some(s=>n.matches(s.trim())))return n;return null;}
  addEventListener(k,f,capture=false){this.listeners.set(k,[...(this.listeners.get(k)||[]),{f,capture}]);}
  removeEventListener(k,f){this.listeners.set(k,(this.listeners.get(k)||[]).filter(v=>v.f!==f));}
  fire(type,props={}){const path=[];for(let p=this;p;p=['click','keydown','focusin'].includes(type)?p.parentNode:null)path.push(p);const e={type,key:'',target:this,currentTarget:this,prevented:false,defaultPrevented:false,stopped:false,immediate:false,composedPath:()=>[...path],preventDefault(){this.prevented=true;this.defaultPrevented=true;},stopPropagation(){this.stopped=true;},stopImmediatePropagation(){this.immediate=true;this.stopped=true;},...props};for(const capture of [true,false]){for(const p of capture?[...path].reverse():path){e.currentTarget=p;for(const {f}of [...(p.listeners.get(type)||[])].filter(x=>!!x.capture===capture)){f(e);if(e.immediate)break;}if(e.stopped)break;}if(e.stopped)break;}return e;}
  focus(options){this.focusCalls++;this.focusOptions=options;if(this.disabled)return;if(document.activeElement!==this){const old=document.activeElement;document.activeElement=this;old?.fire('blur',{relatedTarget:this});this.fire('focus');this.fire('focusin');}}
  blur(){if(document.activeElement===this){document.activeElement=document.body;this.fire('blur',{relatedTarget:document.body});}}
  select(){this.selectionStart=0;this.selectionEnd=this.value.length;}setSelectionRange(a,b,d){this.selectionStart=a;this.selectionEnd=b;this.selectionDirection=d;}
 }
 const home=new Node();home.id='app-home';home.dataset={visit:'1',revision:'2'};
 const host=new Node();host.id='host';home.appendChild(host);host.dataset={homeNativeMount:'3',nativeId:'input',locked:'false',required:String(!optional)};
 const scaffold=h=>{const trigger=new Node('button');trigger.dataset.homeDateAction='open';h.appendChild(trigger);const template=new Node('span');template.dataset.homeDateChevron='true';template.appendChild(new Node('svg'));h.appendChild(template);};scaffold(host);
 const body=new Node();body.appendChild(home);document={body,activeElement:body,documentElement:{clientWidth:900},getElementById:id=>nodes.get(id),createElement:t=>new Node(t),querySelectorAll:()=>[],addEventListener:(...a)=>body.addEventListener(...a),removeEventListener:(...a)=>body.removeEventListener(...a)};
 const windowNode=new Node('window');windowNode.appendChild(body);windowNode.innerWidth=900;windowNode.innerHeight=700;
 
 const now=Date.UTC(2026,9,26,12);class FixedDate extends Date{constructor(...args){super(...(args.length?args:[now]));}static now(){return now;}}
 const context=vm.createContext({window:windowNode,document,Date:FixedDate,performance:{now:()=>clock},setTimeout:f=>{clock+=10;f();},getComputedStyle:()=>({visibility:'visible',overflowX:'visible',overflowY:'visible'}),MutationObserver:class{constructor(f){this.f=f;observers.push(this);}observe(){}disconnect(){}}});
 vm.runInContext(`const timeDayjs=${read('../time/vendor/dayjs.js')};timeDayjs.extend(${read('../time/vendor/custom-parse-format.js')});${read('../time/normalize.js')}\n${read('../time/model.js')}\n${read('../time/bridge.js')}\nconst homeDateLocales=${read('../date/locales.json')};${read('../date/model.js')}\nconst homeDatePopupLocales=${read('../date/popup-locales.json')};${read('../date/popup-model.js')}\n${read('../date/popup.js')}\n${read('date-child.js')}\n${read('model.js')}\nfunction homeDatetimeCaption(){return 'Time zone: Test local zone';}\n${read('bridge.js')}\n${read('../../native.js')}`,context);
 const config={id:7,host:'host',epoch:1,mount:3,selected,optional,locale,time24:false,visit:1,revision:2};
 let api=context.homeDatetimeNative(config,{send:e=>events.push(e)});const entry=api.entry,node=entry.node,picker=entry.datePicker,date=entry.date;
 const ack=()=>api.ack({mount:entry.mount,seq:entry.seq});ack();
 const type=value=>{node.value=value;node.fire('input');ack();};
 const commits=()=>events.filter(e=>e.kind==='commit');
 const expected=()=>[{id:7,mount:entry.mount,seq:entry.seq,value:entry.snapshotValue()}];
 const remount=(selected,revision=3)=>{host.dataset.homeNativeMount='4';home.dataset.revision=String(revision);const c={...config,mount:4,revision,selected};api=context.homeDatetimeNative(c,{send:e=>events.push(e)});if(api)ack();return api;};
 return {node,date,picker,entry,config,events,ack,type,commits,expected,remount,context,home,host,document,observers,Node,scaffold,get api(){return api;}};
}

module.exports={setup};
