import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import vm from 'node:vm';
import test from 'node:test';

const directory=path.dirname(fileURLToPath(import.meta.url));
const source=fs.readFileSync(path.join(directory,'../src/desktop/view/slack_lists/editor.rs'),'utf8');
const mount=source.match(/const MOUNT: &str = r#"([\s\S]*?)"#;/)[1];

// Execute the actual mount callback with a small selection/focus DOM model.
// Empty client rectangles model hidden elements and their hidden ancestors.
function setup(kinds) {
  let frame;
  const body={id:'body',tagName:'BODY',closest:()=>null};
  const document={body,activeElement:body,getElementById:()=>editor};
  const nodes=kinds.map(([id,kind,shown=true,disabled=false])=>({id,kind,shown,disabled,
    getClientRects(){return this.shown?[{}]:[];},
    focus(options){assert.equal(options.preventScroll,true);if(this.shown&&!this.disabled)document.activeElement=this;},
  }));
  const dialog={isConnected:true,dataset:{dialogGeneration:'7'},querySelectorAll:()=>[],querySelector:()=>null};
  const editor={isConnected:true,dataset:{editorGeneration:'9'},closest:()=>dialog,
    contains:element=>element===editor||nodes.includes(element),
    querySelectorAll(selector){return nodes.filter(node=>selector==='[contenteditable=true]'?node.kind==='editable':
      selector==='input:not(:disabled)'?node.kind==='input'&&!node.disabled:
      selector==='button:not(:disabled)'?node.kind==='button'&&!node.disabled:false);},
    querySelector(selector){return this.querySelectorAll(selector)[0]||null;},
    focus(options){assert.equal(options.preventScroll,true);document.activeElement=editor;},
  };
  const context={document,requestAnimationFrame:callback=>{frame=callback;},MutationObserver:class {observe(){} disconnect(){}}};
  vm.runInNewContext(mount+';mountListCellEditor(9,7,"row","column");',context);
  return {nodes,editor,dialog,document,run:()=>frame()};
}

test('disabled rich editor skips hidden link Cancel and focuses visible host Cancel',()=>{
  const h=setup([['link-input','input',false,true],['bold','button',true,true],
    ['link-cancel','button',false],['host-cancel','button',true]]);
  h.run();assert.equal(h.document.activeElement.id,'host-cancel');assert.equal(h.editor.dataset.focusPhase,'focused');
});
test('editable content retains priority over visible inputs and buttons',()=>{
  const h=setup([['cancel','button'],['date','input'],['draft','editable']]);
  h.run();assert.equal(h.document.activeElement.id,'draft');
});
test('input priority is retained while hidden and disabled inputs are skipped',()=>{
  const h=setup([['hidden','input',false],['disabled','input',true,true],['cancel','button'],['number','input']]);
  h.run();assert.equal(h.document.activeElement.id,'number');
});
test('no rendered eligible control falls back to the editor itself',()=>{
  const h=setup([['hidden-draft','editable',false],['hidden-input','input',false],['hidden-cancel','button',false]]);
  h.run();assert.equal(h.document.activeElement,h.editor);
});
test('focus moved outside before the frame is preserved',()=>{
  const h=setup([['draft','editable']]),external={id:'external'};
  h.document.activeElement=external;h.run();assert.equal(h.document.activeElement,external);
  assert.equal(h.editor.dataset.focusPhase,'focus-moved');
});
test('stale mount generation still refuses focus',()=>{
  const h=setup([['draft','editable']]);h.dialog.dataset.dialogGeneration='8';h.run();
  assert.equal(h.document.activeElement,h.document.body);assert.equal(h.editor.dataset.focusPhase,'stale-owner');
});
