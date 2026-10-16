import {readFileSync} from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import test from 'node:test';
const base=process.argv[2]||process.cwd();
const source=readFileSync(path.join(base,'src/desktop/slack_lists/rename/text_create/select_focus.js'),'utf8');
function harness(){
 const frames=[],body={};let focused=0;
 const input={isConnected:true,disabled:false,dataset:{addEpoch:'5'},getClientRects:()=>[1],focus(){focused++;document.activeElement=this;}};
 const root={isConnected:true,dataset:{renameSerial:'2',renameStage:'Editing'},getClientRects:()=>[1],closest:()=>null,contains:e=>e===input};
 const kind={value:'select',dataset:{fieldChoice:'3'}};let top=root;
 const nodes={'slack-list-rename':root,'slack-list-select-option-label':input,'slack-list-new-field-kind':kind};
 const document={body,activeElement:body,getElementById:id=>nodes[id],querySelectorAll:()=>[top]};
 const fn=vm.runInNewContext(`(${source})`,{document,requestAnimationFrame:f=>frames.push(f)});
 return{root,input,kind,nodes,document,run(){fn(2,3,5)},flush(){for(const f of frames.splice(0))f()},focused:()=>focused,setTop:t=>top=t};
}
test('lost focus after an owned Add/remove returns to the actual pending input',()=>{const h=harness();h.run();assert.equal(h.focused(),0);h.flush();assert.equal(h.focused(),1)});
for(const [name,change] of [
 ['new serial',h=>h.root.dataset.renameSerial='8'],['type ABA',h=>h.kind.dataset.fieldChoice='9'],['changed native kind',h=>h.kind.value='text'],['new Add epoch',h=>h.input.dataset.addEpoch='6'],['admitted operation',h=>h.root.dataset.renameStage='Saving'],['disabled input',h=>h.input.disabled=true],['removed owner',h=>h.root.isConnected=false],['replacement node',h=>h.nodes['slack-list-select-option-label']={}],['intervening focus',h=>h.document.activeElement={}],['new top modal',h=>h.setTop({getClientRects:()=>[1],closest:()=>null})]
])test(name+' refuses scheduled focus',()=>{const h=harness();h.run();change(h);h.flush();assert.equal(h.focused(),0)});
