// Installed editor + model + smart modules and vendored Delta. DOM, codec,
// clipboard and native keyboard delivery are modeled, not browser acceptance.
const fs=require('node:fs'),vm=require('node:vm'),assert=require('node:assert/strict');
const {Delta,load,clone,table}=require('./smart_test_helpers.cjs');
let groups=0;
function test(name,fn){fn();groups++;console.log('PASS '+name);}
function harness(overrides={}) {
  const byId=new Map(),timers=new Map(),emojiNodes=new WeakMap();let serial=0,commits=0;
  class Node {
    constructor(){this.children=[];this.attrs={};this.dataset={};this.listeners=new Map();this.isConnected=true;this.parentElement=null;this.contentEditable='inherit';this.events=[];}
    appendChild(n){n.parentElement=this;this.children.push(n);return n;}
    contains(n){return this===n||this.children.some(c=>c.contains(n));}
    querySelector(s){return this.children.find(c=>s==='.ql-editor'&&c.className==='ql-editor')||null;}
    closest(s){if(s==='[data-list-comment-scope]'&&this.dataset.listCommentScope!==undefined)return this;if(s==='[data-editor-generation]'&&this.dataset.editorGeneration!==undefined)return this;return this.parentElement?.closest(s)||null;}
    setAttribute(k,v){this.attrs[k]=String(v);}getAttribute(k){return this.attrs[k]??null;}
    addEventListener(k,f){if(!this.listeners.has(k))this.listeners.set(k,new Set());this.listeners.get(k).add(f);}
    removeEventListener(k,f){this.listeners.get(k)?.delete(f);}
    dispatchEvent(e){this.events.push(e);for(const f of [...this.listeners.get(e.type)||[]])f(e);return true;}
  }
  class Quill {
    static version='1.3.7';static import(n){return n==='delta'?Delta:class{}}static register(){}
    static find(node){return emojiNodes.has(node)?node:null}
    constructor(root,options){this.options=options;this.root=root.appendChild(new Node());this.root.className='ql-editor';root.quill=this;this.events=new Map();this.selection={index:0,length:0};this.doc=new Delta();this.sources=[];
      this.keyboard={bindings:options.modules.keyboard.bindings,added:[],addBinding:(binding,handler)=>this.keyboard.added.push({binding,handler})};
      this.history={stack:{undo:[],redo:[]},clear(){this.stack={undo:[],redo:[]}},cutoff(){}};this.enable(!options.readOnly);}
    on(n,f){this.events.set(n,f)}off(n,f){if(this.events.get(n)===f)this.events.delete(n)}
    setContents(ops){this.doc=new Delta(ops)}update(){}focus(){}
    enable(on){this.root.contentEditable=String(on)}disable(){this.enable(false)}
    getContents(start,length){return start===undefined?new Delta(clone(this.doc.ops)):this.doc.slice(start,start+length)}
    getText(start=0,length=this.doc.length()){return this.doc.slice(start,start+length).ops.map(o=>typeof o.insert==='string'?o.insert:'').join('')}
    getSelection(){return {...this.selection}}setSelection(index,length=0){this.selection={index,length};this.events.get('selection-change')?.()}
    getIndex(node){return emojiNodes.get(node)?.index}
    updateContents(change,source='api'){change=new Delta(change.ops||change);const old=this.doc;this.doc=old.compose(change);this.sources.push(source);
      this.history.stack.undo.push({undo:this.doc.diff(old),redo:change});this.events.get('text-change')?.(change,old,source);}
    replace(text){const next=new Delta().insert(text+'\n');this.updateContents(this.doc.diff(next),'user');this.setSelection(text.length,0)}
  }
  const observers=[];
  const env={document:{getElementById:id=>byId.get(id),createElement:()=>new Node()},
    setTimeout:fn=>{timers.set(++serial,fn);return serial},clearTimeout:id=>timers.delete(id),
    Event:class{constructor(type,options){this.type=type;Object.assign(this,options)}},CustomEvent:class{constructor(type,options){this.type=type;Object.assign(this,options)}},
    MutationObserver:class{constructor(fn){observers.push(fn)}observe(){}disconnect(){}},crypto:{randomUUID:()=> 'test-paste'}};
  const {context}=load({listCommentDeltaBlots:()=>({codec:()=>({encode:clone,decode:clone,
    prepare:(ops,extra)=>({ops:clone(ops),commit(){commits++}}),
    emojiAt:node=>emojiNodes.has(node)?clone(emojiNodes.get(node).value):null,
    refreshMedia:()=>true})}),
    listCommentDeltaClipboardEvents:({input})=>({changed(){},pending:()=>!!input.pastePending,release(){},request:()=>null})});
  vm.runInContext('const LIST_COMMENT_EMOTICONS='+JSON.stringify(table)+';',context);
  vm.runInContext(fs.readFileSync(__dirname+'/live_emoji.js','utf8'),context);
  vm.runInContext(fs.readFileSync(__dirname+'/editor.js','utf8'),context);
  const api=context.listCommentDeltaEditor(Quill,env);
  const options={id:'original',scope:'A',generation:'7',mode:'PlainText',ops:[{insert:'original\n'}],label:'Edit',disabled:false,autofocus:false,
    slugsEnabled:true,mac:false,enterCreatesNewline:false,usePreNewlines:false,spellcheck:true,underlineLinks:true,convertEmoticons:false,emojiAsText:true,
    composerSmartSymbols:true,disableSmartSymbols:false,detectGoLinks:false,flattenTreeListBeforePaste:false,supportsPortableContent:false,markdownMode:false,emoji:[],...overrides};
  function host(id='original'){const node=new Node();node.dataset.listCommentScope='A';node.dataset.editorGeneration='7';node.setAttribute('data-delta-disabled','false');byId.set(id,node);return node}
  const root=host(),ticket=api.mount(root,options),quill=root.quill;
  return {api,root,quill,ticket,options,host,observers,timers,
    commits:()=>commits,
    emojiNode(index,foreign=false){const node=quill.root.appendChild(new Node());if(!foreign)emojiNodes.set(node,{index,value:clone(quill.doc.slice(index,index+1).ops[0].insert.slackemoji)});return node},
    flush(){for(const [id,fn]of [...timers]){timers.delete(id);fn()}},
    event(type,detail={}){quill.root.dispatchEvent(new env.Event(type,detail))},
    key(key,prefix){const bound=quill.keyboard.added.find(row=>row.binding.key===key);return bound.handler(quill.getSelection(),{prefix,format:{}})},
  };
}
test('enabled live quote repair is pending and uses one guarded user-source update',()=>{
  const h=harness();h.quill.replace('He said "hello"');
  assert.equal(h.api.snapshot(h.root).pending,true);assert.equal(h.api.seal(h.root,h.api.snapshot(h.root)),false);assert.equal(h.timers.size,1);
  h.flush();const t=h.api.snapshot(h.root);assert.equal(t.pending,false);assert.equal(t.attachmentEffects,'none-v1');
  assert.equal(h.quill.getText(),'He said \u201chello\u201d\n');assert.equal(h.quill.sources.at(-1),'user');
  assert(h.api.seal(h.root,t));assert(h.api.validate(h.root,t));assert(h.api.unseal(h.root,t));
});
test('single queue sanitizes before mention repair and quotes, bypassing sanitizer for formatting-only',()=>{
  const h=harness();h.quill.replace('\u200b"a"\ufeff\u2028\u00a0\u2194\ufe0e');h.flush();
  assert.equal(h.quill.getText(),'\u201ca\u201d\n \u2194\n');
  h.quill.setContents([{insert:'\u00a0plain\n'}]);
  h.quill.updateContents(new Delta().retain(1,{unlink:true}),'user');assert.equal(h.timers.size,0);
  assert.equal(h.quill.getText(),'\u00a0plain\n');
});
test('composition defers complete document quote repair until end, once',()=>{
  const h=harness();h.event('compositionstart');h.quill.replace('"first"');h.quill.replace('"last"');
  assert.equal(h.timers.size,0);assert.equal(h.api.snapshot(h.root).composing,true);
  h.event('compositionend');assert.equal(h.timers.size,1);assert.equal(h.api.snapshot(h.root).pending,true);h.flush();
  assert.equal(h.quill.getText(),'\u201clast\u201d\n');assert.equal(h.timers.size,0);
});
test('late repairs refuse replacement, detached, disabled, paste and changed documents',()=>{
  for(const mode of ['replacement','detached','disabled','paste','document']) {
    const h=harness();h.quill.replace('"held"');
    if(mode==='replacement')h.host();if(mode==='detached')h.root.isConnected=false;
    if(mode==='disabled')h.api.setDisabled(h.root,'7',true);
    if(mode==='paste')h.quill.root.pastePending=true;
    if(mode==='document')h.quill.setContents([{insert:'replacement\n'}]);
    h.flush();assert.equal(h.quill.getText(),mode==='document'?'replacement\n':'"held"\n',mode);
  }
  const h=harness();h.quill.replace('"old"');h.quill.replace('plain');assert.equal(h.timers.size,0);h.flush();assert.equal(h.quill.getText(),'plain\n');
});
test('pending paste transitions and sealed originals never run substitutions',()=>{
  const h=harness();h.quill.replace('X --');h.quill.root.pastePending=true;
  assert.equal(h.key(' ','X --'),true);assert.equal(h.quill.getText(),'X --\n');
  h.quill.root.pastePending=false;assert.equal(h.key(' ','X --'),false);assert.equal(h.quill.getText(),'X \u2014 \n');
  h.quill.replace('X --');const t=h.api.snapshot(h.root);assert(h.api.seal(h.root,t));
  assert.equal(h.key(' ','X --'),true);assert.equal(h.quill.getText(),'X --\n');assert(h.api.validate(h.root,t));
});
test('exact binding descriptors plus arrow and quote/ellipsis final Save intent',()=>{
  const h=harness(),[space,period]=h.quill.keyboard.added;
  assert.equal(space.binding.shiftKey,null);assert.equal(space.binding.collapsed,true);assert.equal(space.binding.format.link,undefined);
  assert.equal(period.binding.key,190);assert.equal(period.binding.shiftKey,undefined);
  h.quill.replace('Both <->');assert.equal(h.key(' ','Both <->'),false);
  h.quill.updateContents(new Delta().retain(7).insert('"done" ..'),'user');h.quill.setSelection(16,0);h.flush();
  assert.equal(h.quill.getText(),'Both \u2194 \u201cdone\u201d ..\n');
  assert.equal(h.key(190,'Both \u2194 \u201cdone\u201d ..'),false);
  assert.equal(h.quill.getText(),'Both \u2194 \u201cdone\u201d \u2026\n');
  h.quill.keyboard.bindings.enter.handler(h.quill.getSelection(),{prefix:h.quill.getText(),format:{}});
  assert.equal(h.root.events.at(-1).detail,'save');assert.equal(h.api.snapshot(h.root).pending,false);
});
test('off or disabled preference preserves literal input and adds no binding/repair',()=>{
  for(const options of [{composerSmartSymbols:false},{disableSmartSymbols:true}]){
    const h=harness(options);h.quill.replace('"literal" -- .. \u2194\ufe0e');
    assert.equal(h.timers.size,0);assert.equal(h.quill.keyboard.added.length,0);
    assert.equal(h.quill.getText(),'"literal" -- .. \u2194\ufe0e\n');
  }
});
// detect_go_links is retained policy, but ordinary plain formats exclude link.
test('go-link on and off preserve literal typing, Space, newline and Save intent',()=>{
  for(const detectGoLinks of [true,false]) {
    const h=harness({detectGoLinks,composerSmartSymbols:false});
    assert.equal(h.options.detectGoLinks,detectGoLinks);
    assert(!h.quill.options.formats.includes('link'));
    assert.equal(h.quill.keyboard.added.length,0);
    h.quill.replace('go/team');assert.equal(h.timers.size,0);
    // No module consumes Space: model the browser's ordinary insertion.
    h.quill.updateContents(new Delta().retain(7).insert(' '),'user');h.quill.setSelection(8,0);
    assert.deepEqual(clone(h.api.snapshot(h.root).ops),[{insert:'go/team \n'}]);
    const entry=h.quill.history.stack.undo.at(-1);
    h.quill.updateContents(entry.undo,'user');assert.equal(h.quill.getText(),'go/team\n');
    h.quill.updateContents(entry.redo,'user');assert.equal(h.quill.getText(),'go/team \n');
    h.quill.keyboard.bindings.shiftEnter.handler(h.quill.getSelection(),{prefix:'go/team ',format:{}});
    assert.deepEqual(clone(h.api.snapshot(h.root).ops),[{insert:'go/team \n\n'}]);
    h.quill.updateContents(new Delta().retain(9).insert('done'),'user');h.quill.setSelection(13,0);
    const original=clone(h.api.snapshot(h.root).ops);
    h.quill.keyboard.bindings.enter.handler(h.quill.getSelection(),{prefix:'done',format:{}});
    assert.equal(h.root.events.at(-1).detail,'save');
    assert.deepEqual(clone(h.api.snapshot(h.root).ops),original);
    assert.equal(h.timers.size,0);assert.equal(h.api.snapshot(h.root).attachmentEffects,'none-v1');
  }
});
test('go-link admission retains smart queue and refuses unavailable policy or other modes',()=>{
  const h=harness({detectGoLinks:true});h.quill.replace('go/team');h.flush();
  assert.equal(h.key(' ','go/team'),true);assert.equal(h.quill.getText(),'go/team\n');
  assert(!h.quill.options.formats.includes('link'));
  for(const options of [{mode:'Portable'},{markdownMode:true},{supportsPortableContent:true},
      {emojiAsText:false},{detectGoLinks:undefined}])
    assert.throws(()=>harness({detectGoLinks:true,...options}));
});
const liveCatalog={names:['smile','thumbsup'],unicode:['\u{1f604}','\u{1f44d}'],
  unicodeNames:[['1f604','smile'],['1f44d','thumbsup']],locale:'en-US',currentLocale:'en-US',
  rows:[{name:'smile',appearance:{text:'\u{1f604}'},variants:[],single:false,multiple:false},
    {name:'thumbsup',appearance:{text:'\u{1f44d}'},variants:[['2',{text:'\u{1f44d}\u{1f3fb}'}]],single:true,multiple:false}]};
const live=options=>harness({emojiAsText:false,liveEmoji:clone(liveCatalog),...options});
test('live catalog mounts, one queue converts named and Unicode before quotes with fresh ticket',()=>{
  const h=live();h.quill.replace('"Hi" :smile: \u{1f604}');const pending=h.api.snapshot(h.root);
  assert.equal(pending.pending,true);assert.equal(h.commits(),0);assert.equal(h.api.seal(h.root,pending),false);
  assert.equal(h.timers.size,1);h.flush();const ready=h.api.snapshot(h.root);
  assert.deepEqual(clone(ready.ops),[{insert:'\u201cHi\u201d '},{insert:{slackemoji:{text:':smile:'}}},{insert:' '},{insert:{slackemoji:{text:':smile:'}}},{insert:'\n'}]);
  assert.equal(ready.pending,false);assert.equal(h.commits(),1);assert.equal(h.quill.sources.at(-1),'user');
  assert.equal(h.api.seal(h.root,pending),false);assert(h.api.seal(h.root,ready));assert(h.api.validate(h.root,ready));
});
test('live queue respects composition, original owner, document, disabled and pending paste',()=>{
  for(const mode of ['replacement','detached','disabled','paste','document']) {
    const h=live({composerSmartSymbols:false});h.quill.replace(':smile:');
    if(mode==='replacement')h.host();if(mode==='detached')h.root.isConnected=false;
    if(mode==='disabled')h.api.setDisabled(h.root,'7',true);if(mode==='paste')h.quill.root.pastePending=true;
    if(mode==='document')h.quill.setContents([{insert:'successor\n'}]);
    h.flush();assert.equal(h.commits(),0);assert.equal(h.quill.getText(),mode==='document'?'successor\n':':smile:\n');
  }
  const h=live({composerSmartSymbols:false});h.event('compositionstart');h.quill.replace(':smile:');
  assert.equal(h.timers.size,0);h.event('compositionend');assert.equal(h.timers.size,1);h.flush();assert.equal(h.commits(),1);
  h.quill.replace(':smile:');h.api.release(h.root,'7');h.flush();assert.equal(h.commits(),1);
});
test('owned tone click uses actual original op, default API history and rejects foreign or sealed nodes',()=>{
  const h=live({composerSmartSymbols:false,ops:[{insert:{slackemoji:{text:':thumbsup:'}}},{insert:'\n'}]});
  const original=h.emojiNode(0),foreign=h.emojiNode(0,true);
  h.event('click',{target:foreign});assert.equal(h.commits(),0);
  h.event('click',{target:original});assert.equal(h.commits(),1);
  assert.equal(h.api.snapshot(h.root).ops[0].insert.slackemoji.text,':thumbsup::skin-tone-2:');
  assert.equal(h.quill.sources.at(-1),'api');assert.equal(h.quill.history.stack.undo.length,1);
  // Old DOM/token no longer corresponds to the actual document op.
  h.event('click',{target:original});assert.equal(h.commits(),1);
  const current=h.emojiNode(0),ticket=h.api.snapshot(h.root);assert(h.api.seal(h.root,ticket));
  h.event('click',{target:current});assert.equal(h.commits(),1);assert(h.api.validate(h.root,ticket));assert(h.api.unseal(h.root,ticket));
  const entry=h.quill.history.stack.undo.at(-1);h.quill.updateContents(entry.undo,'user');
  assert.equal(h.api.snapshot(h.root).ops[0].insert.slackemoji.text,':thumbsup:');
  h.quill.updateContents(entry.redo,'user');assert.equal(h.api.snapshot(h.root).ops[0].insert.slackemoji.text,':thumbsup::skin-tone-2:');
});
test('display policy remains explicit; text-mode bypasses live conversion and portable stays refused',()=>{
  assert.throws(()=>live({liveEmoji:null}));assert.throws(()=>live({supportsPortableContent:true}));assert.throws(()=>live({markdownMode:true}));
  const h=harness({emojiAsText:true,liveEmoji:clone(liveCatalog),composerSmartSymbols:false});h.quill.replace(':smile: \u{1f604}');
  assert.equal(h.timers.size,0);assert.equal(h.quill.getText(),':smile: \u{1f604}\n');assert.equal(h.commits(),0);
});
test('tone event fences preserve owner, pending paste, composition, disable and original node',()=>{
  for(const mode of ['replacement','detached','disabled','paste','composition','pending']) {
    const h=live({composerSmartSymbols:false,ops:[{insert:{slackemoji:{text:':thumbsup:'}}},{insert:'\n'}]});
    const node=h.emojiNode(0);
    if(mode==='replacement')h.host();if(mode==='detached')h.root.isConnected=false;
    if(mode==='disabled')h.api.setDisabled(h.root,'7',true);if(mode==='paste')h.quill.root.pastePending=true;
    if(mode==='composition')h.event('compositionstart');
    if(mode==='pending')h.quill.updateContents(new Delta().retain(1).insert(':smile:'),'user');
    h.event('click',{target:node});assert.equal(h.commits(),0,mode);
    assert.equal(h.quill.doc.ops[0].insert.slackemoji.text,':thumbsup:',mode);
  }
});
console.log(JSON.stringify({status:'PASS',groups,installedEditor:true,actualVendoredDelta:true,modeledDOMQuillCodec:true,native:false}));
