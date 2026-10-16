import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import test from 'node:test';
const base=process.argv[2]||process.cwd();
const source=readFileSync(path.join(base,'src/desktop/slack_lists/check/text_field_create.js'),'utf8');
function harness(kind) {
 const element=()=>({closest:()=>null,parentElement:null,getBoundingClientRect:()=>({x:10,y:10,left:10,top:10,right:210,bottom:42,width:200,height:32})});
 const options=[['text','Text'],['number','Number'],['checkbox','Checkbox'],['date','Date']].map(([value,textContent])=>({value,textContent}));
 const hint={...element(),textContent:(kind==='number'?'Number format: 0 decimal places. Missing values stay empty. ':kind==='checkbox'?'Editable empty cells appear unchecked. Values are saved only when you change a checkbox. ':'')+'Shared with everyone who has access to this List.'};
 const nodes={'#slack-list-rename':{...element(),dataset:{renameSerial:'7'},querySelectorAll:()=>[hint]},'#slack-list-rename-name':{...element(),value:'Draft',disabled:false,dataset:{fieldChoice:'9'}},'#slack-list-new-field-kind':{...element(),value:kind,disabled:false,dataset:{fieldChoice:'9'},options},'#slack-list-rename-title':{textContent:'Add field'}};
 const context=vm.createContext({window:{},document:{querySelector:s=>nodes[s]||null},innerWidth:640,innerHeight:380});
 vm.runInContext(source,context);const api=context.window.__listTextCreate;assert.equal(api.configure(kind).ok,true);
 return{api,options};
}
for(const mode of ['text','number','checkbox']) {
 test(`${mode} legacy checker demands exactly four Type choices`,()=>{
  const {api,options}=harness(mode);assert.equal(api.draft().ok,true);
  const date=options.pop();assert.equal(api.draft().ok,false,'old three-choice screen must fail');
  options.push(date);assert.equal(api.draft().ok,true);
  options.push({...date});assert.equal(api.draft().ok,false,'duplicate Date must fail');
 });
}
test('configured historical Text checker cannot be reconfigured as Date',()=>{
 const {api}=harness('text');assert.equal(api.configure('date').ok,false);
});
test('shared product selector exposes Date and uses the existing revision-owned handler',()=>{
 const source=readFileSync(path.join(base,'src/desktop/view/slack_lists/rename.rs'),'utf8');
 assert.match(source,/option\s*\{\s*value:\s*"date",\s*selected:\s*field_kind\s*==\s*"date",\s*"Date"\s*\}/);
 assert.match(source,/rename::text_create::choose_kind\(state,serial,visit,selected,rendered,event.value\(\)\)/);
});
