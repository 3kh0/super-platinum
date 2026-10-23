const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const source=fs.readFileSync('src/desktop/slack_lists/check/compose_comments/own_edit/plain/emoticon.js','utf8');
function scene(){
 let ops=[{insert:'Happy :)\n'}],composing=false,hits=true,consumed=false,commands=0,last;
 const node={length:8},rect={left:10,top:10,right:100,bottom:40,width:90,height:30};
 const input={isConnected:true,isContentEditable:true,scrollIntoView(){},focus(){},getBoundingClientRect:()=>rect,contains:n=>n===node||n===input,
  dispatchEvent:e=>{last=e;if(e.type==='compositionstart')composing=true;else if(e.type==='compositionend')composing=false;else if(consumed)e.preventDefault();}};
 const root={querySelector:()=>input,closest:()=>null},region={dataset:{listCommentsEditScope:'scope'}};
 const selection={isCollapsed:true,anchorNode:node,focusNode:node,anchorOffset:8,focusOffset:8,toString:()=>'',removeAllRanges(){},addRange(r){this.anchorOffset=r.a;this.focusOffset=r.b;this.isCollapsed=r.a===r.b;this.toString=()=> 'Happy :)'.slice(r.a,r.b)}};
 const win={__ownEditOriginalEditor:root,__ownEditOriginalInput:input,__ownEditOriginalPlainOwner:{scope:'scope',generation:'7'},superPlatinumCommentDelta:{snapshot:()=>({ops:structuredClone(ops),mode:'PlainText',revision:1,generation:'7',composing,pending:false,attachmentEffects:'none-v1'})}};
 class Event{constructor(type,args){Object.assign(this,args,{type,isTrusted:false,defaultPrevented:false})}preventDefault(){this.defaultPrevented=true}}
 const context={window:win,document:{getElementById:id=>id==='slack-list-comment-edit-draft'?root:region,elementFromPoint:()=>hits?input:null,
  execCommand:(cmd,_,text)=>{assert.equal(cmd,'insertText');assert.equal(text,' ');commands++;ops=[{insert:'Happy :) \n'}];return true},
  createTreeWalker:()=>{let once=false;return {nextNode:()=>once?null:(once=true,node)}},createRange:()=>({setStart(n,i){this.a=i},setEnd(n,i){this.b=i}})},
  getSelection:()=>selection,NodeFilter:{SHOW_TEXT:4},KeyboardEvent:Event,CompositionEvent:Event,innerWidth:800,innerHeight:600};
 const run=vm.runInNewContext(source,context);return {run:(a,b={})=>JSON.parse(JSON.stringify(run(a,b))),win,input,root,region,selection,setHits:v=>hits=v,setConsumed:v=>consumed=v,commands:()=>commands,event:()=>last};
}
test('refused Space records separate original default insertion',()=>{const s=scene(),v=s.run('key',{key:'Space'});assert.equal(v.ok,true);assert.equal(v.defaultPrevented,false);assert.equal(v.defaultApplied,true);assert.equal(s.commands(),1);assert.deepEqual(v.after.ops,[{insert:'Happy :) \n'}])});
test('consumed Space cannot apply browser default twice',()=>{const s=scene();s.setConsumed(true);const v=s.run('key',{key:'Space'});assert.equal(v.defaultPrevented,true);assert.equal(v.defaultApplied,false);assert.equal(s.commands(),0)});
test('composition observation never invents browser IME input',()=>{const s=scene();assert.equal(s.run('composition',{active:true}).after.composing,true);const v=s.run('key',{key:'Space'});assert.equal(v.before.composing,true);assert.equal(v.defaultApplied,false);assert.deepEqual(v.before.ops,v.after.ops);assert.equal(s.run('composition',{active:false}).after.composing,false)});
test('Undo and Redo dispatch original keyboard modifiers',()=>{for(const key of ['Undo','Redo']){const s=scene();s.setConsumed(true);const v=s.run('key',{key});assert.equal(v.keyCode,90);assert.equal(v.ctrlKey,true);assert.equal(v.shiftKey,key==='Redo');assert.equal(v.isTrusted,false);assert.equal(s.commands(),0)}});
test('actual literal DOM range selection retained',()=>{const s=scene(),v=s.run('select',{start:6,end:8});assert.equal(v.ok,true);assert.equal(v.selection.text,':)');assert.equal(v.selection.collapsed,false);assert.deepEqual(v.before.ops,v.after.ops)});
test('foreign node, owner, generation and pending tickets refused',()=>{for(const k of ['root','input','scope','generation','pending']){const s=scene();if(k==='root')s.win.__ownEditOriginalEditor={};if(k==='input')s.win.__ownEditOriginalInput={};if(k==='scope')s.region.dataset.listCommentsEditScope='other';if(k==='generation')s.win.__ownEditOriginalPlainOwner.generation='8';if(k==='pending'){const old=s.win.superPlatinumCommentDelta.snapshot;s.win.superPlatinumCommentDelta.snapshot=()=>({...old(),pending:true})}assert.equal(s.run('key',{key:'Space'}).ok,false);assert.equal(s.commands(),0)}});
test('clipped, readonly and foreign selection refused',()=>{for(const k of ['hit','readonly','selection']){const s=scene();if(k==='hit')s.setHits(false);if(k==='readonly')s.input.isContentEditable=false;if(k==='selection')s.selection.anchorNode={};assert.equal(s.run('key',{key:'Space'}).ok,false);assert.equal(s.commands(),0)}});
test('unknown key and selection past input refused',()=>{const s=scene();assert.equal(s.run('key',{key:'Tab'}).ok,false);assert.equal(s.run('select',{start:9,end:10}).ok,false)});
