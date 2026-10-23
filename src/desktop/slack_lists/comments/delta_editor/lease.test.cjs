// Portable tests of the installed editor's lease state machine. DOM/Quill and
// clipboard/model boundaries are modeled; no browser/native/clipboard IO claim.
const fs=require('node:fs'),vm=require('node:vm'),assert=require('node:assert/strict');
const clone=x=>JSON.parse(JSON.stringify(x));
const editorSource=fs.readFileSync(__dirname+'/editor.js','utf8');
let groups=0;
function check(name,fn){fn();groups++;}
function harness(convert=false){
 const byId=new Map(),timers=new Map(),mediaRefreshes=[];let timer=0;
 class Node {
  constructor(){this.children=[];this.attrs={};this.dataset={};this.listeners=new Map();this.isConnected=true;this.parentElement=null;this.contentEditable='inherit';this.className='';this.events=[];}
  appendChild(n){n.parentElement=this;this.children.push(n);return n;}
  contains(n){return this===n||this.children.some(c=>c.contains(n));}
  querySelector(s){return this.children.find(c=>s==='.ql-editor'&&c.className==='ql-editor')||this.children.map(c=>c.querySelector(s)).find(Boolean)||null;}
  closest(s){if(s==='[data-list-comment-scope]'&&this.dataset.listCommentScope!==undefined)return this;if(s==='[data-editor-generation]'&&this.dataset.editorGeneration!==undefined)return this;return this.parentElement?.closest(s)||null;}
  setAttribute(k,v){this.attrs[k]=String(v);if(k==='contenteditable')this.contentEditable=String(v);}
  getAttribute(k){return this.attrs[k]??null;}
  addEventListener(k,f){if(!this.listeners.has(k))this.listeners.set(k,new Set());this.listeners.get(k).add(f);}
  removeEventListener(k,f){this.listeners.get(k)?.delete(f);}
  dispatchEvent(e){this.events.push(e.type);for(const f of [...this.listeners.get(e.type)||[]])f(e);return true;}
 }
 class Delta{retain(){return this}delete(){return this}insert(){return this}}
 class Quill{
  static version='1.3.7';static import(name){return name==='delta'?Delta:class{}}static register(){}
  constructor(host,options){this.root=host.appendChild(new Node());this.root.className='ql-editor';this.handlers=new Map();this.history={clear(){}};this.keyboard={bindings:options.modules.keyboard.bindings,added:[],addBinding:(binding,handler)=>this.keyboard.added.push({binding,handler})};this.enable(!options.readOnly);host.quill=this;this.selection={index:1,length:0};}
  on(k,f){this.handlers.set(k,f)}off(k,f){if(this.handlers.get(k)===f)this.handlers.delete(k)}
  setContents(ops){this.ops=clone(ops);}
  units(){return this.ops.flatMap(op=>typeof op.insert==='string'?[...op.insert].map(insert=>({insert})): [clone(op)]);}
  fromUnits(units){const ops=[];for(const op of units){if(typeof op.insert==='string'&&typeof ops.at(-1)?.insert==='string')ops.at(-1).insert+=op.insert;else ops.push(clone(op));}this.ops=ops;this.handlers.get('text-change')?.();}
  getText(start,length){return this.units().slice(start,start+length).map(op=>typeof op.insert==='string'?op.insert:'').join('');}
  getContents(start,length){if(start!==undefined)return {ops:this.units().slice(start,start+length)};return {ops:clone(this.ops),diff:other=>({length:()=>JSON.stringify(this.ops)===JSON.stringify(other.ops)?0:1})};}
  deleteText(index,length){const units=this.units();units.splice(index,length);this.fromUnits(units);}
  insertEmbed(index,kind,value){const units=this.units();units.splice(index,0,{insert:{[kind]:clone(value)}});this.fromUnits(units);}
  insertText(index,text){const units=this.units();units.splice(index,0,...[...text].map(insert=>({insert})));this.fromUnits(units);}
  update(){if(this.deferred){const f=this.deferred;this.deferred=null;f();}}
  disable(){this.enable(false)}enable(on){this.enabled=on;this.root.contentEditable=String(on);this.root.attrs.contenteditable=String(on);}
  getSelection(){return clone(this.selection)}setSelection(index,length=0){this.selection={index,length};this.handlers.get('selection-change')?.();}focus(){}
  edit(text){this.ops=[{insert:text}];this.handlers.get('text-change')?.();}
  updateContents(ops){this.ops=clone(ops);this.handlers.get('text-change')?.();}
 }
 const observers=[];
 const env={document:{getElementById:id=>byId.get(id),createElement:()=>new Node()},setTimeout:fn=>{timers.set(++timer,fn);return timer},clearTimeout:id=>timers.delete(id),Event:class{constructor(type,options){this.type=type;Object.assign(this,options)}},CustomEvent:class{constructor(type,options){this.type=type;Object.assign(this,options)}},MutationObserver:class{constructor(fn){this.fn=fn;observers.push(this)}observe(){}disconnect(){this.closed=true}},crypto:{randomUUID:()=> 'request'}};
 const model={clone,document:ops=>clone(ops),blocksSlugKey:()=>false,slugSelection:()=>({partial:false,inside:false}),brokenMentions:raw=>({length:()=>raw[0]?.insert==='repair'?1:0,ops:[{insert:'repaired'}]})};
 const context=vm.createContext({listCommentDeltaModel:()=>model,listCommentDeltaBlots:()=>({codec:()=>({encode:clone,decode:clone,prepare:ops=>({ops:clone(ops),commit(){}}),refreshMedia:(root,epoch)=>{mediaRefreshes.push({root,epoch});return true;}})}),listCommentDeltaClipboardEvents:({input})=>({changed(){},pending:()=>!!input.pastePending,request:()=>null,resolve:()=>true,release(){input.clipboardReleased=true;}})});
 vm.runInContext(`const LIST_COMMENT_EMOTICONS=${fs.readFileSync(__dirname+'/emoticons.json','utf8')};${fs.readFileSync(__dirname+'/emoticons.js','utf8')}`,context);vm.runInContext(editorSource,context);const api=context.listCommentDeltaEditor(Quill,env);
 const options={id:'original',scope:'scopeA',generation:'7',label:'Edit',mode:'PlainText',ops:[{insert:'original\n'}],disabled:false,autofocus:false,slugsEnabled:true,mac:false,enterCreatesNewline:true,usePreNewlines:true,spellcheck:true,underlineLinks:true,convertEmoticons:convert,emojiAsText:true,composerSmartSymbols:false,disableSmartSymbols:true,detectGoLinks:false,flattenTreeListBeforePaste:false,supportsPortableContent:false,markdownMode:false,emoji:[]};
 function host(id='original',generation='7'){const n=new Node();n.dataset.listCommentScope='scopeA';n.dataset.editorGeneration=generation;n.setAttribute('data-delta-disabled','false');byId.set(id,n);return n;}
 const root=host();const ticket=api.mount(root,options);
 return {api,root,ticket,input:root.quill.root,quill:root.quill,options,host,byId,observers,timers,mediaRefreshes,flush(){for(const [id,f] of [...timers]){timers.delete(id);f()}},event:(node,type)=>node.dispatchEvent(new env.Event(type)),Node};
}
check('media refresh preserves sealed content and refuses retired ownership',()=>{
 const h=harness(),before=h.api.snapshot(h.root),selection=clone(h.quill.selection);
 assert(h.api.seal(h.root,before));
 assert(h.api.refreshMedia(h.root,'scopeA','7','12'));
 assert.equal(h.mediaRefreshes.length,1);assert.equal(h.mediaRefreshes[0].root,h.input);
 assert.equal(h.mediaRefreshes[0].epoch,'12');assert(h.api.validate(h.root,before));
 const after=h.api.snapshot(h.root);
 assert.deepEqual(clone(after.ops),clone(before.ops));assert.equal(after.revision,before.revision);
 assert.deepEqual(h.quill.selection,selection);
 assert.equal(h.api.refreshMedia(h.root,'wrong','7','13'),false);
 assert.equal(h.api.refreshMedia(h.root,'scopeA','8','13'),false);
 h.root.isConnected=false;assert.equal(h.api.refreshMedia(h.root,'scopeA','7','13'),false);
 h.root.isConnected=true;h.host();assert.equal(h.api.refreshMedia(h.root,'scopeA','7','13'),false);
 assert.equal(h.mediaRefreshes.length,1);
});
check('seal validates and reversibly releases only original ticket',()=>{
 const h=harness(),before=clone(h.ticket.ops);assert(h.api.seal(h.root,h.ticket));assert.equal(h.input.contentEditable,'false');assert(h.api.validate(h.root,h.ticket));assert.deepEqual(clone(h.api.snapshot(h.root).ops),before);assert.equal(h.api.seal(h.root,h.api.snapshot(h.root)),false);assert(h.api.unseal(h.root,h.ticket));assert.equal(h.input.contentEditable,'true');assert.equal(h.api.validate(h.root,h.ticket),false);assert.equal(h.api.seal(h.root,h.ticket),false);assert(h.api.seal(h.root,h.api.snapshot(h.root)));
});
check('cloned and mutated tickets cannot acquire or release lease',()=>{
 const h=harness();assert.equal(h.api.seal(h.root,clone(h.ticket)),false);h.ticket.ops[0].insert='forged';assert.equal(h.api.seal(h.root,h.ticket),false);const t=h.api.snapshot(h.root);assert(h.api.seal(h.root,t));assert.equal(h.api.unseal(h.root,clone(t)),false);assert(h.api.validate(h.root,t));
});
check('content revision protects A-B-A and changes while sealed',()=>{
 const h=harness();h.quill.edit('changed\n');h.quill.edit('original\n');assert.equal(h.api.seal(h.root,h.ticket),false);const t=h.api.snapshot(h.root);assert(t.revision>h.ticket.revision);assert(h.api.seal(h.root,t));h.quill.edit('injected\n');assert.equal(h.api.validate(h.root,t),false);assert.equal(h.input.contentEditable,'false');assert(h.api.unseal(h.root,t));assert.deepEqual(clone(h.api.snapshot(h.root).ops),[{insert:'injected\n'}]);
});
check('composition repair and pending paste refuse before and after drain',()=>{
 for(const mode of ['composition','repair','paste']){const h=harness();if(mode==='composition')h.event(h.input,'compositionstart');if(mode==='repair')h.quill.edit('repair');if(mode==='paste')h.input.pastePending=true;assert.equal(h.api.seal(h.root,h.api.snapshot(h.root)),false);}
 const h=harness();h.quill.deferred=()=>h.quill.edit('repair');assert.equal(h.api.seal(h.root,h.ticket),false);assert.equal(h.api.snapshot(h.root).pending,true);h.flush();assert.equal(h.api.snapshot(h.root).pending,false);
});
check('disabled ownership changes remain disabled after unseal',()=>{
 const h=harness();assert(h.api.seal(h.root,h.ticket));h.root.setAttribute('data-delta-disabled','true');h.observers[0].fn();assert.equal(h.api.validate(h.root,h.ticket),false);assert(h.api.unseal(h.root,h.ticket));assert.equal(h.input.contentEditable,'false');h.root.setAttribute('data-delta-disabled','false');h.observers[0].fn();assert.equal(h.input.contentEditable,'true');assert.equal(h.api.seal(h.root,h.ticket),false);
});
check('same ID successor and detached original retain separate models',()=>{
 const h=harness();assert(h.api.seal(h.root,h.ticket));h.root.isConnected=false;const successor=h.host('original','8');const fresh=h.api.mount(successor,{...h.options,generation:'8'});assert.equal(h.api.validate(h.root,h.ticket),false);assert.equal(h.api.validate(successor,h.ticket),false);const final=h.api.snapshot(h.root);assert.deepEqual(clone(final.ops),clone(h.ticket.ops));assert(h.api.unseal(h.root,h.ticket));assert.equal(h.input.contentEditable,'false');assert.equal(successor.quill.root.contentEditable,'true');assert(h.api.release(h.root,'7'));assert.equal(h.api.snapshot(h.root),null);assert.equal(h.api.release(successor,'7'),false);assert(h.api.seal(successor,fresh));
});
check('original editable replacement and scope/generation changes refuse',()=>{
 for(const change of ['child','scope','generation']){const h=harness();if(change==='child'){const replacement=new h.Node();replacement.className='ql-editor';replacement.parentElement=h.root;h.root.children.unshift(replacement);}if(change==='scope')h.root.dataset.listCommentScope='B';if(change==='generation')h.root.dataset.editorGeneration='8';assert.equal(h.api.seal(h.root,h.ticket),false);}
 const h=harness();assert(h.api.seal(h.root,h.ticket));h.quill.root=new h.Node();assert.equal(h.api.validate(h.root,h.ticket),false);assert.equal(h.api.unseal(h.root,h.ticket),false);
});
check('release waits for caller and consumes original final model',()=>{
 const h=harness();assert(h.api.seal(h.root,h.ticket));h.root.isConnected=false;assert(h.api.snapshot(h.root));assert.equal(h.api.release(h.root,'wrong'),false);assert(h.api.snapshot(h.root));assert(h.api.release(h.root,'7'));assert(h.input.clipboardReleased);assert(h.observers[0].closed);assert.equal(h.api.snapshot(h.root),null);assert.equal(h.api.unseal(h.root,h.ticket),false);
});
check('unsupported positive or unobserved policies do not mount',()=>{
 for(const [key,value] of [['emojiAsText',false],['detectGoLinks',undefined],['supportsPortableContent',true],['markdownMode',true],['spellcheck',undefined]]){const h=harness(),root=h.host('other');assert.throws(()=>h.api.mount(root,{...h.options,id:'other',[key]:value}));assert.equal(h.api.snapshot(root),null);}
});
check('off-policy Enter receives context without converting or changing intent',()=>{
 const h=harness(false);h.quill.edit('Hi :)\n');h.quill.setSelection(5,0);let action;
 h.root.addEventListener('comment-delta-action',e=>action=e.detail);
 assert.equal(h.quill.keyboard.added.length,0);
 assert.equal(h.quill.keyboard.bindings.enter.handler({index:5,length:0},{prefix:'Hi :)',format:{}}),false);
 assert.equal(action,'save');assert.deepEqual(h.quill.ops,[{insert:'Hi :)\n'}]);
 assert.equal(h.api.snapshot(h.root).attachmentEffects,'none-v1');
});
check('enabled Space converts before capture and keeps original capability',()=>{
 const h=harness(true);h.quill.edit('Hi :)\n');h.quill.setSelection(5,0);
 const binding=h.quill.keyboard.added[0];assert.equal(binding.binding.collapsed,true);assert.equal(binding.binding.shiftKey,null);
 assert.equal(binding.handler({index:5,length:0},{prefix:'Hi :)',format:{}}),false);
 const captured=h.api.snapshot(h.root);assert.deepEqual(clone(captured.ops),[{insert:'Hi '},{insert:{slackemoji:{text:':slightly_smiling_face:'}}},{insert:' \n'}]);
 assert.equal(captured.attachmentEffects,'none-v1');assert.deepEqual(h.quill.selection,{index:5,length:0});assert.equal(h.api.seal(h.root,h.ticket),false);
});
check('Enter conversion precedes actual existing Save intent',()=>{
 const h=harness(true);h.quill.edit('Hi :)\n');h.quill.setSelection(5,0);let action;
 h.root.addEventListener('comment-delta-action',e=>{action={value:e.detail,ops:clone(h.api.snapshot(h.root).ops)};});
 assert.equal(h.quill.keyboard.bindings.enter.handler({index:5,length:0},{prefix:'Hi :)',format:{}}),false);
 assert.deepEqual(action,{value:'save',ops:[{insert:'Hi '},{insert:{slackemoji:{text:':slightly_smiling_face:'}}},{insert:'\n'}]});
 assert.deepEqual(h.quill.selection,{index:4,length:0});
});
check('composition pending sealed disabled and replaced original never convert',()=>{
 for(const mode of ['composition','paste','sealed','disabled','replacement']){
  const h=harness(true);h.quill.edit('Hi :)\n');h.quill.setSelection(5,0);
  if(mode==='composition')h.event(h.input,'compositionstart');
  if(mode==='paste')h.input.pastePending=true;
  if(mode==='sealed')assert(h.api.seal(h.root,h.api.snapshot(h.root)));
  if(mode==='disabled')h.api.setDisabled(h.root,'7',true);
  if(mode==='replacement')h.host('original','8');
  assert.equal(h.quill.keyboard.added[0].handler({index:5,length:0},{prefix:'Hi :)',format:{}}),true,mode);
  assert.deepEqual(h.quill.ops,[{insert:'Hi :)\n'}],mode);
 }
});
console.log(JSON.stringify({status:'PASS',groups,actualInstalledEditor:true,modeledQuillDOM:true,browser:false,native:false,clipboardIO:false}));
