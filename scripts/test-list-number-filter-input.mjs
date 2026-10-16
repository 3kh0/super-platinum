// Legacy single-input helper regressions remain; actual complete-set runtime is covered
// by test-list-compound-filter-capture.mjs. Current UI wiring is checked below.
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import test from 'node:test';
const base=process.argv[2]||'.';
const source=fs.readFileSync(path.join(base,'src/desktop/view/slack_lists/filter/number.js'),'utf8');
function setup() {
  const proof={serial:'7',choice:'19',column:'C_NUMBER',operator:'equal',field:'3'};
  const input={isConnected:true,disabled:false,type:'number',value:'12.50',validity:{valid:true,badInput:false},dataset:{filterChoice:'19',filterColumn:'C_NUMBER',filterOperator:'equal'}};
  const field={isConnected:true,disabled:false,value:'3',dataset:{filterChoice:'19'}};
  const op={isConnected:true,disabled:false,value:'equal',dataset:{filterChoice:'19'}};
  const nodes={'#slack-list-filter-number':input,'#slack-list-filter-condition':field,'#slack-list-filter-operator':op};
  const root={id:'slack-list-rename',isConnected:true,dataset:{renameSerial:'7',renameStage:'Editing'},querySelector:s=>nodes[s],contains:n=>Object.values(nodes).includes(n)};
  let current=root;
  const ctx=vm.createContext({document:{getElementById:()=>current}});
  vm.runInContext(source,ctx);
  return {proof,input,field,op,root,nodes,replace:r=>{current=r;},capture:()=>ctx.numberFilterSnapshot(proof)};
}
test('exact lexical snapshot and actual validity lock all three controls until owned restore',()=>{
 const x=setup(),r=x.capture();assert.deepEqual(Array.from(r.snapshot),['12.50',true]);
 for(const n of [x.input,x.field,x.op])assert.equal(n.disabled,true);
 r.restore();for(const n of [x.input,x.field,x.op])assert.equal(n.disabled,false);
});
test('badInput overrides valid lexical text without changing or clearing native draft',()=>{
 const x=setup();x.input.validity.badInput=true;const r=x.capture();assert.deepEqual(Array.from(r.snapshot),['12.50',false]);
 assert.equal(x.input.value,'12.50');assert.equal(x.input.disabled,false);
});
test('native validity false cannot be replaced by lexical validity',()=>{
 const x=setup();x.input.validity.valid=false;assert.equal(x.capture().snapshot[1],false);
});
test('native partial exponent with sanitized empty value is retained',()=>{
 const x=setup();x.input.value='';x.input.nativeSegments='1e';x.input.validity={valid:false,badInput:true};
 assert.deepEqual(Array.from(x.capture().snapshot),['',false]);assert.equal(x.input.nativeSegments,'1e');
});
test('all owner/epoch/operator/control refusals fail closed before any lock',()=>{
 const changes=[x=>x.root.dataset.renameSerial='8',x=>x.root.dataset.renameStage='Preparing',x=>x.input.dataset.filterChoice='20',x=>x.field.dataset.filterChoice='20',x=>x.op.dataset.filterChoice='20',x=>x.input.dataset.filterColumn='OTHER',x=>x.input.dataset.filterOperator='not_equal',x=>x.op.value='not_equal',x=>x.input.type='text',x=>x.input.isConnected=false,x=>x.root.isConnected=false,x=>x.field.disabled=true,x=>x.op.disabled=true,x=>x.input.disabled=true];
 for(const mutate of changes){const x=setup();mutate(x);assert.equal(x.capture(),null);}
});
test('replacement modal for another serial is not accepted',()=>{
 const x=setup();x.replace({...x.root,dataset:{...x.root.dataset,renameSerial:'8'}});assert.equal(x.capture(),null);
});
test('phase advance prevents old snapshot from enabling controls',()=>{
 const x=setup(),r=x.capture();x.root.dataset.renameStage='Preparing';r.restore();assert.equal(x.input.disabled,true);
});
test('epoch ABA and same-id input replacement prevent stale restore',()=>{
 for(const change of [x=>{x.input.dataset.filterChoice='21';},x=>{x.nodes['#slack-list-filter-number']={...x.input};},x=>x.replace({...x.root})]){
  const x=setup(),r=x.capture();change(x);r.restore();assert.equal(x.input.disabled,true);
 }
});
test('current shared rows retain native partials and whole-set capture',()=>{
 const ui=fs.readFileSync(path.join(base,'src/desktop/view/slack_lists/filter/rows.rs'),'utf8');
 assert.match(ui,/r#type:\s*"number",\s*step:\s*"any",\s*initial_value:/);
 assert.match(ui,/for version in \[epoch\]/);assert.match(ui,/label\{key:\s*"\{id\}:\{version\}"/);
 const capture=fs.readFileSync(path.join(base,'src/desktop/view/slack_lists/filter/capture.rs'),'utf8');
 assert.match(capture,/filter::reserve/);assert.match(capture,/eval\.recv::<Option<Vec<filter::Snapshot>>>/);
 assert.doesNotMatch(ui,/value:\s*"\{initial\}"/);
});

test('named normal/narrow fixture wires catalog boot and actual narrow sizing',()=>{
 for(const p of ['scripts/agent-ui-check.sh','src/desktop/state/boot.rs']) {
  const s=fs.readFileSync(path.join(base,p),'utf8');for(const n of ['slack-list-number-filter','slack-list-number-filter-narrow'])assert.ok(s.includes(n));
 }
 const media=fs.readFileSync(path.join(base,'src/desktop/media.rs'),'utf8');assert.match(media,/\| "slack-list-number-filter-narrow"/);
});

test('unprocessed native field change with old rendered epoch cannot submit old Number input',()=>{const x=setup();x.field.value='1';assert.equal(x.capture(),null);assert.equal(x.input.disabled,false);});
