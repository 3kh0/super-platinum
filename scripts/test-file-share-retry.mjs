import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const source=fs.readFileSync(new URL('../src/desktop/file_share/check/retry_editor.js',import.meta.url),'utf8');
function fixture(){
 const text={};const note={textContent:'Café',contentEditable:'false',getAttribute:()=> 'true',contains:n=>n===text};
 const dialog={dataset:{fileShareId:'owner'},contains:n=>n===note};
 const elements={'file-share-dialog':dialog,'file-share-note':note};
 const document={getElementById:id=>elements[id],activeElement:note};
 let blocks=[{type:'rich_text',future:'kept'}];
 const selection={anchorNode:text,anchorOffset:4,focusNode:text,focusOffset:0,toString(){return this.anchorOffset===this.focusOffset?'':'Café';}};
 const window={superPlatinumRichEditor:{read:()=>blocks}};
 vm.runInNewContext(source,{window,document,getSelection:()=>selection});
 return {note,dialog,document,elements,selection,window,setBlocks:b=>blocks=b,retained:window.__fileShareRetry.capture()};
}
test('same readonly mounted note and backward selection pass without mutation',()=>{const f=fixture();assert.equal(f.retained().ok,true);assert.equal(f.selection.anchorOffset,4);assert.equal(f.selection.focusOffset,0);});
test('identical replacement editor and dialog are rejected',()=>{for(const id of ['file-share-note','file-share-dialog']){const f=fixture();f.elements[id]={...f.elements[id]};assert.equal(f.retained().ok,false);}});
test('new generation on same element is rejected',()=>{const f=fixture();f.dialog.dataset.fileShareId='new';assert.equal(f.retained().ok,false);});
test('changed typed unknown metadata or text is rejected',()=>{const f=fixture();f.setBlocks([{type:'rich_text'}]);assert.equal(f.retained().ok,false);const g=fixture();g.note.textContent='different';assert.equal(g.retained().ok,false);});
test('selection collapse or loss to another root is rejected',()=>{for(const field of ['anchorNode','anchorOffset','focusNode','focusOffset']){const f=fixture();f.selection[field]=field.endsWith('Node')?{}:2;assert.equal(f.retained().ok,false);}});
test('readonly and owned focus required independently',()=>{for(const change of [f=>f.note.contentEditable='true',f=>f.note.getAttribute=()=> 'false',f=>f.document.activeElement={}]){const f=fixture();change(f);assert.equal(f.retained().ok,false);}});
test('snapshot refuses a selection outside editor',()=>{const f=fixture();f.selection.anchorNode={};assert.throws(()=>f.window.__fileShareRetry.capture(),/outside exact editor/);});
test('snapshot refuses collapsed or forward selection',()=>{for(const offset of [0,-1]){const f=fixture();f.selection.anchorOffset=offset;assert.throws(()=>f.window.__fileShareRetry.capture(),/backward text selection/);}});
