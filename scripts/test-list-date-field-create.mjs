import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import test from 'node:test';
const base=process.argv[2]||process.cwd();
const source=readFileSync(path.join(base,'src/desktop/slack_lists/check/text_field_create.js'),'utf8');
function harness(kind) {
 const element=()=>({closest:()=>null,parentElement:null,getBoundingClientRect:()=>({x:10,y:10,left:10,top:10,right:210,bottom:42,width:200,height:32})});
 const options=[['text','Text'],['number','Number'],['checkbox','Checkbox'],['date','Date'],['select','Select'],['user','People'],['link','Link'],['channel','Channel'],['rating','Rating'],['vote','Vote']].map(([value,textContent])=>({value,textContent}));
 const hint={...element(),textContent:(kind==='number'?'Number format: 0 decimal places. Missing values stay empty. ':kind==='checkbox'?'Editable empty cells appear unchecked. Values are saved only when you change a checkbox. ':'')+'Shared with everyone who has access to this List.'};
 const nodes={'#slack-list-rename':{...element(),dataset:{renameSerial:'7'},querySelectorAll:()=>[hint]},'#slack-list-rename-name':{...element(),value:'Draft',disabled:false,dataset:{fieldChoice:'9'}},'#slack-list-new-field-kind':{...element(),value:kind,disabled:false,dataset:{fieldChoice:'9'},options},'#slack-list-rename-title':{textContent:'Add field'}};
 const context=vm.createContext({window:{},document:{querySelector:s=>nodes[s]||null},innerWidth:640,innerHeight:380});
 vm.runInContext(source,context);const api=context.window.__listTextCreate;assert.equal(api.configure(kind).ok,true);
 return{api,options};
}
for(const mode of ['text','number','checkbox','date','select','user','link','channel']) {
 test(`${mode} legacy checker demands exactly ten current Type choices`,()=>{
  const {api,options}=harness(mode);assert.equal(api.draft().ok,true);
  const date=options.pop();assert.equal(api.draft().ok,false,'old eight-choice screen must fail');
  options.push(date);assert.equal(api.draft().ok,true);
  options.push({...date});assert.equal(api.draft().ok,false,'duplicate Type must fail');
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

 test('Select mode is explicit and cannot reconfigure an existing historical checker',()=>{
  const {api}=harness('text');assert.equal(api.configure('select').ok,false);
  const view=readFileSync(path.join(base,'src/desktop/view/slack_lists/rename.rs'),'utf8');
  assert.match(view,/option\s*\{\s*value:\s*"select",\s*selected:\s*field_kind\s*==\s*"select",\s*"Select"\s*\}/);
 });

test('Select-only tools are withheld from historical modes',()=>{for(const kind of ['text','number','checkbox','date','select','user']){const {api}=harness(kind);assert.equal(!!api.selectTools(),kind==='select');}});

test('People tools are exact-mode only and preserve historical configuration',()=>{for(const kind of ['text','number','checkbox','date','select','user']){const {api}=harness(kind);assert.equal(!!api.peopleTools(),kind==='user');assert.equal(api.configure('user').ok,false);}});

test('Link inventory preserves refusal to reconfigure an existing mode',()=>{
 const {api,options}=harness('text');
 assert.deepEqual(options[6],{value:'link',textContent:'Link'});
 assert.equal(api.configure('link').ok,false);
 const view=readFileSync(path.join(base,'src/desktop/view/slack_lists/rename.rs'),'utf8');
 assert.match(view,/option\s*\{\s*value:\s*"link",\s*selected:\s*field_kind\s*==\s*"link",\s*"Link"\s*\}/);
});

test('Link is an exact independent mode with tools withheld from all historical modes',()=>{
 for(const kind of ['text','number','checkbox','date','select','user','link']){
  const {api}=harness(kind);assert.equal(!!api.linkTools(),kind==='link');assert.equal(api.configure('link').ok,false);
 }
});


test('Channel eighth choice preserves mode reconfiguration refusal',()=>{
 const {api,options}=harness('text');
 assert.deepEqual(options[7],{value:'channel',textContent:'Channel'});
 assert.equal(api.configure('channel').ok,false);
 const view=readFileSync(path.join(base,'src/desktop/view/slack_lists/rename.rs'),'utf8');
 assert.match(view,/option\s*\{\s*value:\s*"channel",\s*selected:\s*field_kind\s*==\s*"channel",\s*"Channel"\s*\}/);
});

test('Channel tools are exact-mode only while historical tools remain withheld',()=>{for(const kind of ['text','number','checkbox','date','select','user','link','channel']){const {api}=harness(kind);assert.equal(!!api.channelTools(),kind==='channel');assert.equal(!!api.linkTools(),kind==='link');assert.equal(!!api.peopleTools(),kind==='user');assert.equal(api.configure('channel').ok,false);}});

test('Rating ninth choice cannot reconfigure an existing historical mode',()=>{
 const {api,options}=harness('text');
 assert.deepEqual(options[8],{value:'rating',textContent:'Rating'});
 assert.equal(api.configure('rating').ok,false);
 const view=readFileSync(path.join(base,'src/desktop/view/slack_lists/rename.rs'),'utf8');
 assert.match(view,/option\s*\{\s*value:\s*"rating",\s*selected:\s*field_kind\s*==\s*"rating",\s*"Rating"\s*\}/);
});


test('Vote tenth choice is exact and cannot reconfigure a historical mode',()=>{
 const {api,options}=harness('text');
 assert.deepEqual(options[9],{value:'vote',textContent:'Vote'});
 assert.equal(options.length,10);assert.equal(api.draft().ok,true);
 assert.equal(api.configure('vote').ok,false);
 options[9]={value:'future_vote',textContent:'Vote'};assert.equal(api.draft().ok,false);
 const view=readFileSync(path.join(base,'src/desktop/view/slack_lists/rename.rs'),'utf8');
 assert.match(view,/option\s*\{\s*value:\s*"vote",\s*selected:\s*field_kind\s*==\s*"vote",\s*"Vote"\s*\}/);
});

test('Rating tools are exact-mode only and preserve historical control isolation',()=>{for(const kind of ['text','number','checkbox','date','select','user','link','channel','rating']){const {api}=harness(kind);assert.equal(!!api.ratingTools(),kind==='rating');assert.equal(!!api.channelTools(),kind==='channel');assert.equal(api.configure('rating').ok,false);}});

test('Vote dynamic tools are closed to the exact configured mode and cannot reconfigure prior modes',()=>{for(const kind of ['text','number','checkbox','date','select','user','link','channel','rating','vote']){const {api}=harness(kind);assert.equal(!!api.voteTools(),kind==='vote');assert.equal(api.configure('vote').ok,false);assert.equal(!!api.ratingTools(),kind==='rating');}});
