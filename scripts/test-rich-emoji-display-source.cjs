const fs=require('fs'),vm=require('vm'),assert=require('assert');
const source=fs.readFileSync('src/desktop/slack_lists/check/compose_comments/display_source.js','utf8');
function run(ready,change=()=>{}){
 const box={left:10,top:10,right:100,bottom:30,width:90,height:20};
 const el=(extra={})=>({isConnected:true,getBoundingClientRect:()=>box,...extra});
 const text={},root=el(),selection={anchorNode:text,focusNode:text,anchorOffset:1,focusOffset:3};
 const image=el({complete:true,naturalWidth:16,naturalHeight:16,getAttribute:k=>({src:'http://super-platinum-media.localhost/media/a?v=2',alt:':shared_party:',title:':shared_party:'})[k]});
 const wave=el({textContent:'\u{1f44b}',getAttribute:()=>':wave:'});
 const alias=el({textContent:':shared_party:',getAttribute:()=>':shared_party:'});
 const body=el({textContent:ready?'Shared  Standard \u{1f44b}':'Shared :shared_party: Standard \u{1f44b}',querySelectorAll:s=>s==='img.emoji-image'?(ready?[image]:[]):ready?[wave]:[alias,wave]});
 const row=el({dataset:{listCommentTs:'1781524801.000001'},querySelector:()=>body});
 const geometry={ok:true,editor:{blocks:[{original:true}],revision:4,composing:false,editable:true}};
 const saved={root,anchor:text,focus:text,anchorOffset:1,focusOffset:3,ids:new WeakMap(),serial:0};
 const window={__displaySourceProof:saved,__listCompose:{snapshot:()=>geometry}};
 const document={getElementById:()=>root,querySelectorAll:()=>[row],querySelector:()=>el()};
 change({root,selection,image,wave,alias,body,geometry,saved});
 return vm.runInNewContext('('+source+')',{window,document,getSelection:()=>selection,innerWidth:800,innerHeight:600})('observe',ready);
}
let count=0;const test=(name,fn)=>{fn();count++;console.log('PASS '+name);};
test('pending retains alias and sibling',()=>assert.equal(run(false).ok,true));
test('ready requires original image',()=>assert.equal(run(true).ok,true));
test('caret loss refused',()=>assert.equal(run(true,x=>x.selection.anchorOffset=2).ok,false));
test('reply replacement refused',()=>assert.equal(run(true,x=>x.saved.root={}).ok,false));
test('image not decoded refused',()=>assert.equal(run(true,x=>x.image.naturalWidth=0).ok,false));
test('wrong alias refused',()=>assert.equal(run(false,x=>x.alias.textContent=':wrong:').ok,false));
test('sibling drift refused',()=>assert.equal(run(true,x=>x.wave.textContent='wrong').ok,false));
test('composition refused',()=>assert.equal(run(true,x=>x.geometry.editor.composing=true).ok,false));
console.log(`${count} passed, 0 skipped`);
