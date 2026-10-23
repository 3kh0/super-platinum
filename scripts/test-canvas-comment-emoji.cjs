const fs=require('fs'),vm=require('vm'),assert=require('assert');
const source=fs.readFileSync('src/desktop/canvas/comments/check/emoji/native.js','utf8');
function setup(mode='unobserved',thread=false){
 class E {constructor(text='',opts={}){Object.assign(this,{textContent:text,isConnected:true,dataset:{},attrs:{},all:{},one:{}},opts);}getBoundingClientRect(){return {x:50,y:50,width:100,height:24};}contains(e){return e===this||e===hit;}querySelector(s){return this.one[s]??null;}querySelectorAll(s){return this.all[s]??[];}getAttribute(s){return this.attrs[s]??null;}scrollIntoView(){}focus(){} }
 let hit=new E(),selection={},root=new E(),panel=new E(),scroll=new E('',{attrs:{'aria-busy':'false'},scrollTop:0,scrollLeft:0}),preview=new E(),editor=new E('',{attrs:{contenteditable:'true'}}),text=new E('Canvas draft café 中🦊');
 const body=label=>new E(label,{all:{'.custom-emoji': mode==='unobserved'?[new E('👍🏽',{title:':conflicting_original:'}),new E('A',{title:':unknown_original:'})]:mode==='pending'?[new E(':+1::skin-tone-4:',{title:':+1::skin-tone-4:'}),new E('A',{title:':unknown_original:'})]:[new E('A',{title:':unknown_original:'})],'img.emoji-image':mode==='ready'?[new E('',{alt:':+1::skin-tone-4:',title:':+1::skin-tone-4:',complete:true,naturalWidth:16,naturalHeight:16,attrs:{src:'http://super-platinum-media.localhost/emoji/test?v=1'}})]:[]}});
 const rootBody=body('Root'),replyBody=body('Last reply');root.dataset.commentTs='1719800000.000100';root.one['.message-body']=rootBody;root.one['.canvas-comment-preview']=preview;root.one['.canvas-comment-preview .message-body']=replyBody;
 const reply=new E('',{dataset:{commentTs:'1719800001.000100'},one:{'.message-body':replyBody}}),hidden=new E('',{dataset:{commentTs:'1719800020.000100'},one:{'.message-body':new E('This content was deleted.')}}),link=new E('Original canonical file',{dataset:{canvasFileId:'FCANEMOJI',canvasFileRoot:'1719800030.000100',canvasFileName:'Original canonical file'},attrs:{href:'https://example.test/canonical/FCANEMOJI'}}),file=new E('',{dataset:{commentTs:'1719800030.000100'},one:{'.message-body':new E('Original canonical file',{all:{'[data-canvas-file-id]':[link]}})}});
 panel.one['[data-comment-ts="1719800001.000100"] .message-body']=replyBody;panel.one['.canvas-comments-scroll']=scroll;panel.one['[data-comment-ts="1719800000.000100"]']=root;panel.all['.canvas-comment']=thread?[root,reply]:[root,hidden,file];
 const blocks=[{type:'rich_text',elements:[{type:'rich_text_section',elements:[{type:'text',text:text.textContent}]}]}],holder={root:editor,running:true,scope:'original-scope',generation:3};
 let edits=0;
 const document={getElementById:id=>({'canvas-comments':panel,'canvas-comments-filter':thread?null:new E(),'canvas-comment-reply':thread?editor:null}[id]??null),elementFromPoint:()=>hit,createTreeWalker:()=>({nextNode:()=>text}),createRange:()=>({setStart(n,o){this.start=[n,o]},setEnd(n,o){this.end=[n,o]}})};
 selection.removeAllRanges=()=>{};selection.addRange=r=>{selection.anchorNode=r.start[0];selection.focusNode=r.end[0];selection.anchorOffset=r.start[1];selection.focusOffset=r.end[1];};
 const window={__superPlatinumCanvasReplyCaptures:[['mount-1',holder]],superPlatinumRichEditor:{snapshot:()=>({blocks,composing:false})}};
 const context={window,document,innerWidth:1200,innerHeight:800,NodeFilter:{SHOW_TEXT:4},getSelection:()=>selection};
 const call=vm.runInNewContext('('+source+')',context);
 if(thread)assert.equal(call('draft-arm','').ok,true);else assert.equal(call('stream-arm','').ok,true);
 return {call,context,scroll,window,panel,root,rootBody,replyBody,preview,editor,selection,holder,blocks,hidden,file,document,setHit(v){hit=v;}};
}
let n=0;function test(name,fn){fn();n++;console.log('PASS '+name);}
for(const mode of ['unobserved','pending','ready'])test('actual helper '+mode,()=>{const s=setup(mode);assert.equal(s.call('observe',mode).ok,true);});
test('wrong semantic glyph refused',()=>{const s=setup();s.rootBody.all['.custom-emoji'][0].textContent='👍';assert.equal(s.call('observe','unobserved').ok,false);});
test('image decoding refused',()=>{const s=setup('ready');s.rootBody.all['img.emoji-image'][0].naturalWidth=0;assert.equal(s.call('observe','ready').ok,false);});
test('discarded body refused',()=>{const s=setup();s.hidden.one['.message-body'].textContent='Hidden';assert.equal(s.call('observe','unobserved').ok,false);});
test('root replacement refused',()=>{const s=setup();s.window.__canvasCommentEmojiStream.root={};assert.equal(s.call('observe','unobserved').ok,false);});
test('preview replacement refused',()=>{const s=setup();s.window.__canvasCommentEmojiStream.preview={};assert.equal(s.call('observe','unobserved').ok,false);});
test('original rich caret retained',()=>{const s=setup('ready',true);assert.equal(s.call('observe','ready').ok,true);s.selection.focusOffset=8;assert.equal(s.call('observe','ready').ok,false);});
test('native holder replacement refused',()=>{const s=setup('ready',true);s.holder.generation++;assert.equal(s.call('observe','ready').ok,false);});
test('native block mutation refused',()=>{const s=setup('ready',true);s.blocks[0].elements[0].elements[0].text+='!';assert.equal(s.call('observe','ready').ok,false);});
test('original exposure required',()=>{const s=setup();s.rootBody.contains=()=>false;assert.equal(s.call('observe','unobserved').ok,false);});
test('sequential exposure refuses normal width',()=>{const s=setup();assert.equal(s.call('expose','unobserved','root').ok,false);});
test('narrow selected body alone is exposed',()=>{const s=setup();s.context.innerWidth=668;s.replyBody.contains=()=>false;assert.equal(s.call('observe','unobserved','root').ok,true);assert.equal(s.call('observe','unobserved','preview').ok,false);});
test('narrow body exposure is actual scroll without caret repair',()=>{const s=setup('ready',true);s.context.innerWidth=668;s.rootBody.contains=()=>false;s.rootBody.scrollIntoView=()=>{s.rootBody.contains=()=>true;s.editor.contains=()=>false;s.scroll.scrollTop=20;};const before={...s.selection};const e=s.call('expose','ready','root');assert.equal(e.ok,true);assert.equal(e.scrollAfter.top,20);assert.equal(JSON.stringify(e.before),JSON.stringify(e.after));assert.deepEqual(s.selection,before);assert.equal(s.call('observe','ready','root').ok,true);assert.equal(s.call('observe','ready','editor').ok,false);});
test('scroll focus mutation refused',()=>{const s=setup('ready',true);s.context.innerWidth=668;s.editor.scrollIntoView=()=>{s.document.activeElement={};};assert.equal(s.call('expose','ready','editor').ok,false);});
test('scroll selection mutation refused without restoring it',()=>{const s=setup('ready',true);s.context.innerWidth=668;s.editor.scrollIntoView=()=>{s.selection.focusOffset=8;};assert.equal(s.call('expose','ready','editor').ok,false);assert.equal(s.selection.focusOffset,8);});
test('offscreen decoded image still requires original semantic name',()=>{const s=setup('ready',true);s.context.innerWidth=668;s.rootBody.contains=()=>false;s.rootBody.all['img.emoji-image'][0].title=':foreign:';assert.equal(s.call('observe','ready','editor').ok,false);});
console.log(`${n} tests passed`);
