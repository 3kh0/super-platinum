const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const source=fs.readFileSync('src/desktop/slack_lists/check/compose_comments/own_edit/plain/emoji_display.js','utf8');
function scene(){
 let ops=[{insert:'Happy :)\n'}],composing=false,hits=true,consumed=false,commands=0,last;
 const node={length:8},rect={left:10,top:10,right:100,bottom:40,width:90,height:30};
 const input={isConnected:true,isContentEditable:true,scrollIntoView(){},focus(){},getBoundingClientRect:()=>rect,contains:n=>n===node||n===input,querySelectorAll:()=>[],
  dispatchEvent:e=>{last=e;if(e.type==='compositionstart')composing=true;else if(e.type==='compositionend')composing=false;else if(consumed)e.preventDefault();}};
 const root={querySelector:()=>input,closest:()=>null},region={dataset:{listCommentsEditScope:'scope'}};
 const selection={isCollapsed:true,anchorNode:node,focusNode:node,anchorOffset:8,focusOffset:8,toString:()=>'',removeAllRanges(){},addRange(r){this.anchorOffset=r.a;this.focusOffset=r.b;this.isCollapsed=r.a===r.b;this.toString=()=> 'Happy :)'.slice(r.a,r.b)}};
 const win={__ownEditOriginalEditor:root,__ownEditOriginalInput:input,__ownEditOriginalPlainOwner:{scope:'scope',generation:'7'},superPlatinumCommentDelta:{snapshot:()=>({ops:structuredClone(ops),mode:'PlainText',revision:1,generation:'7',composing,pending:false,attachmentEffects:'none-v1'})}};
 class Event{constructor(type,args){Object.assign(this,args,{type,isTrusted:false,defaultPrevented:false})}preventDefault(){this.defaultPrevented=true}}
 const context={window:win,document:{getElementById:id=>id==='slack-list-comment-edit-draft'?root:id==='slack-list-comment-edit'?region:{disabled:false},elementFromPoint:()=>hits?input:null,
  execCommand:(cmd,_,text)=>{assert.equal(cmd,'insertText');commands++;ops=[{insert:'Happy :)'+text+'\n'}];return true},
  createTreeWalker:()=>{let once=false;return {nextNode:()=>once?null:(once=true,node)}},createRange:()=>({selectNodeContents(){this.a=8;this.b=8},collapse(){},setStart(n,i){this.a=i},setEnd(n,i){this.b=i}})},
  getSelection:()=>selection,NodeFilter:{SHOW_TEXT:4},KeyboardEvent:Event,MouseEvent:Event,CompositionEvent:Event,innerWidth:800,innerHeight:600};
 const run=vm.runInNewContext(source,context);return {run:(a,b={})=>JSON.parse(JSON.stringify(run(a,b))),win,input,root,region,selection,context,setHits:v=>hits=v,setConsumed:v=>consumed=v,commands:()=>commands,event:()=>last};
}


test('Actual append uses insertText without Delta injection',()=>{const s=scene(),v=s.run('append',{text:' Typed :smile: ??'});assert.equal(v.ok,true);assert.equal(v.inputCommand,'insertText');assert.equal(s.commands(),1)});
test('Foreign owner/input, pending/composition and covered input refuse',()=>{for(const k of ['root','scope','pending','composing','input','hit']){const s=scene();if(k==='root')s.win.__ownEditOriginalEditor={};if(k==='input')s.win.__ownEditOriginalInput={};if(k==='scope')s.region.dataset.listCommentsEditScope='other';if(k==='hit')s.setHits(false);if(['pending','composing'].includes(k)){const old=s.win.superPlatinumCommentDelta.snapshot;s.win.superPlatinumCommentDelta.snapshot=()=>({...old(),[k]:true})}assert.equal(s.run('append',{text:'x'}).ok,false);assert.equal(s.commands(),0)}});
test('Original history modifiers, no fabricated default insertion',()=>{for(const key of ['undo','redo']){const s=scene();s.setConsumed(true);const v=s.run(key);assert.equal(v.ok,true);assert.equal(v.event.ctrlKey,true);assert.equal(v.event.shiftKey,key==='redo');assert.equal(v.event.defaultPrevented,true);assert.equal(s.commands(),0)}});
test('Missing original tone and image references refuse',()=>{const s=scene();assert.equal(s.run('tone',{label:':thumbsup:'}).ok,false);assert.equal(s.run('media-hold').ok,false);assert.equal(s.run('media-ready').ok,false)});
test('Controls must settle and exact expected semantic ops are checked',()=>{const s=scene();assert.equal(s.run('state').ok,true);assert.equal(s.run('state',{ops:[]}).ok,false);const get=s.context.document.getElementById;s.context.document.getElementById=k=>k==='slack-list-comment-edit-save'?{disabled:true}:get(k);assert.equal(s.run('state').ok,false)});
test('Media refresh requires same original nodes, ticket and selection',()=>{const s=scene(),img={isConnected:true,complete:true,naturalWidth:1,naturalHeight:1,src:'http://media/?v=1',getAttribute(k){return k==='src'?this.src:':parrot_alias:'}},node={isConnected:true,textContent:'',getAttribute:()=>':parrot_alias:',querySelector:()=>img};s.input.querySelectorAll=()=>[node];assert.equal(s.run('media-hold').ok,true);assert.equal(s.run('media-ready').ok,false);img.src='http://media/?v=2';img.naturalWidth=16;img.naturalHeight=16;assert.equal(s.run('media-ready').ok,true);s.selection.anchorOffset++;assert.equal(s.run('media-ready').ok,false);s.selection.anchorOffset--;const old=s.win.superPlatinumCommentDelta.snapshot;s.win.superPlatinumCommentDelta.snapshot=()=>({...old(),revision:2});assert.equal(s.run('media-ready').ok,false)});

function transcriptScene(saved=false){
 const s=scene(),rect={left:10,top:10,right:200,bottom:30,width:190,height:20};
 const pane={isConnected:true,parentElement:null},row={isConnected:true,parentElement:pane,dataset:{listCommentTs:'1781524801.000001'}},body={isConnected:true,parentElement:row,getBoundingClientRect:()=>rect};
 const glyphs=saved?['\u{1f44d}\u{1f3fe}',':parrot_alias:','\u{1f604}','\u{1f604}']:['\u{1f44d}\u{1f3fd}',':parrot_alias:'];
 const nodes=glyphs.map((text,i)=>({isConnected:true,parentElement:body,textContent:text,innerText:text,getAttribute:()=>i===0?':thumbsup:':i===1?':parrot_alias:':':smile:',getBoundingClientRect:()=>rect}));
 body.textContent=body.innerText='Initial '+glyphs[0]+' Custom :parrot_alias:';body.querySelectorAll=q=>q==='.custom-emoji'?nodes:[];
 row.querySelectorAll=q=>q==='.message-body'?[body]:[];pane.querySelectorAll=()=>[row];
 const get=s.context.document.getElementById;s.context.document.getElementById=id=>id==='slack-list-comments'?pane:get(id);s.context.getComputedStyle=n=>({display:n.hidden?'none':'inline',visibility:'visible',opacity:'1'});
 return {...s,pane,row,body,nodes,glyphs,probe:()=>s.run('transcript',{ts:row.dataset.listCommentTs,glyphs})};
}
test('Original transcript records exact initial and saved Unicode scalars',()=>{for(const saved of [false,true]){const s=transcriptScene(saved),v=s.probe();assert.equal(v.ok,true);assert.deepEqual(v.emojis[0].codepoints,[0x1f44d,saved?0x1f3fe:0x1f3fd]);assert.equal(v.images,0);assert.equal(s.commands(),0)}});
test('Transcript plain base tone or editor-only toned appearance cannot pass',()=>{const s=transcriptScene();s.nodes[0].textContent=s.nodes[0].innerText='\u{1f44d}';assert.equal(s.probe().ok,false)});
test('Missing or duplicate original row/body refuses',()=>{for(const which of ['missing','duplicate','body']){const s=transcriptScene();if(which==='missing')s.pane.querySelectorAll=()=>[];if(which==='duplicate')s.pane.querySelectorAll=()=>[s.row,s.row];if(which==='body')s.row.querySelectorAll=()=>[s.body,s.body];assert.equal(s.probe().ok,false)}});
test('Hidden transcript or image substitution cannot masquerade as glyph',()=>{for(const which of ['hidden','image','detached']){const s=transcriptScene();if(which==='hidden')s.row.hidden=true;if(which==='detached')s.nodes[0].isConnected=false;if(which==='image'){const query=s.body.querySelectorAll;s.body.querySelectorAll=q=>q==='img'?[{}]:query(q)}assert.equal(s.probe().ok,false)}});
test('Original transcript node ids persist and replacement remains observable',()=>{const s=transcriptScene(),a=s.probe(),b=s.probe();assert.equal(a.emojis[0].node,b.emojis[0].node);s.nodes[0]={...s.nodes[0]};const c=s.probe();assert.equal(c.ok,true);assert.notEqual(a.emojis[0].node,c.emojis[0].node)});
