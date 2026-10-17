import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import test from 'node:test';
const base=process.argv[2]||process.cwd();
const source=readFileSync(path.join(base,'src/desktop/slack_lists/check/text_field_create/number_precision/number.js'),'utf8');
const shared=readFileSync(path.join(base,'src/desktop/slack_lists/check/text_field_create.js'),'utf8');
function fixture(helperSource=source,viewport=[640,380]){
 const nodes=new Map();let modal=true,reveals=0;const actions=[];
 const document={activeElement:null,querySelector:s=>nodes.get(s)||null,querySelectorAll:s=>s==='.slack-list-warning'?[warning]:[]};
 const box={x:20,y:20,left:20,top:20,right:100,bottom:52,width:80,height:32};
 function element(id){return{id,isConnected:true,disabled:false,readOnly:false,dataset:{},parentElement:null,style:{outlineStyle:'solid',outlineWidth:'2px',outlineOffset:'-2px',overflowX:'visible',overflowY:'visible'},box:{...box},getBoundingClientRect(){return this.box;},focus(){document.activeElement=this;},click(){if(!this.disabled)actions.push(this.id);},dispatchEvent(e){actions.push(e.type);},closest(){return null;},contains(e){return e===this;}};}
 class Input {get value(){return this._value||''}set value(v){this._value=v}}
 const input=Object.assign(new Input(),element('slack-list-number-draft'),{type:'text',validity:{valid:true,badInput:false}});
 const panel=element('slack-list-cell-editor');panel.dataset.editorGeneration='7';
 const clear=element('slack-list-date-clear'),cancel=element('slack-list-editor-cancel'),save=element('slack-list-editor-save'),refresh=element('slack-list-editor-refresh');
 const alert=Object.assign(element('alert'),{textContent:'This value cannot be saved. Check the value and choices.'});
 let buttons=[cancel,save],alerts=[alert];cancel.textContent="Cancel";save.textContent="Save";refresh.textContent="Refresh";panel.querySelectorAll=s=>s==='button'?buttons:s==='[role=alert]'?alerts:[];
 panel.contains=e=>[panel,input,clear,cancel,save,refresh,alert].includes(e);
 const row=element('row');row.dataset={rowId:'R_LIST_003',instanceId:'instance1'};
 const cell=element('cell');cell.dataset.columnId='C_NUMBER_OBSERVED';cell.closest=s=>s==='[data-instance-id]'||s==='[data-row-id]'?row:null;
 const edit=element('edit'),primary=element('primary');cell.querySelector=s=>s==='.slack-list-cell-text'?{textContent:cell.text||''}:edit;
 row.querySelector=s=>s.includes('C_NAME')?primary:cell;
 const root=element('root');root.dataset={dialogGeneration:'2',viewSelection:'3'};root.scrollWidth=root.clientWidth=500;root.contains=e=>e===cell||e===row||e===primary;
 const heading=Object.assign(element('heading'),{textContent:'Release count'}),picker={value:'V_FILTER',selectedOptions:[{textContent:'Shared checklist'}]},warning=Object.assign(element('warning'),{textContent:'Only part of this List is loaded'}),find=element('find');
 const visible=e=>!!e&&e.isConnected&&!e.hidden&&e.box.width>0&&e.box.left>=0&&e.box.right<=viewport[0];
 const hit=e=>visible(e)&&!e.covered;
 const tools={q:s=>s==='#slack-list-cell-editor'?(modal?panel:null):s==='#slack-list-number-draft'?(modal?input:null):nodes.get(s)||null,rect:e=>e?[e.box.x,e.box.y,e.box.width,e.box.height]:null,visible,hit,reveal:()=>{reveals++;return[]},ids:()=>['R_LIST_003','R_LIST_001','R_LIST_007','R_LIST_005']};
 nodes.set('.slack-list-grid tr[data-row-id="R_LIST_003"]',row);nodes.set('.slack-list-dialog',root);nodes.set('.slack-list-grid',root);nodes.set('.slack-list-grid th[data-column-id="C_NUMBER_OBSERVED"]',heading);nodes.set('#slack-list-view-select',picker);nodes.set('.slack-list-find input',find);
 for(const e of [clear,cancel,save,refresh])nodes.set('#'+e.id,e);
 document.elementFromPoint=()=>clear;
 const window={__listTextCreate:{numberTools:()=>tools,findRetained:()=>({ok:true})}};
 const context={window,document,TextEncoder,devicePixelRatio:1.5,innerWidth:viewport[0],innerHeight:viewport[1],getComputedStyle:e=>e.style,HTMLInputElement:Input,Event:class{constructor(type){this.type=type}},KeyboardEvent:class{constructor(type,args){this.type=type;Object.assign(this,args)}}};
 assert.equal(vm.runInNewContext(helperSource,context).ok,true);
 input.focus();const api=window.__listNumberCreate;assert.equal(api.ready('',7).ok,true);
 return{api,input,panel,cell,root,row,clear,cancel,save,refresh,alert,actions,document,nodes,context,get reveals(){return reveals;},set modal(v){modal=v;},set buttons(v){buttons=v;},set alerts(v){alerts=v;},heading,warning,primary};
}

test('actual native lexical Number setter and clear retain exact value',()=>{const h=fixture();assert.equal(h.api.input('-12.3456',7).ok,true);assert.equal(h.input.value,'-12.3456');assert.equal(h.api.input('',7).ok,true);assert.equal(h.input.value,'');assert.deepEqual(h.actions,['input','input']);assert.equal(h.api.input('-12.346',7).ok,false);});
test('stale generation and disconnected or covered inputs cannot dispatch',()=>{const h=fixture();assert.equal(h.api.input('-12.3456',8).ok,false);h.input.covered=true;assert.equal(h.api.input('-12.3456',7).ok,false);h.input.covered=false;h.input.isConnected=false;assert.equal(h.api.click('slack-list-editor-save',7).ok,false);assert.deepEqual(h.actions,[]);});
test('empty no-op requires exact current input and visible mapped error',()=>{const h=fixture();assert.equal(h.api.noop(7).ok,true);h.alert.textContent='wrong';assert.equal(h.api.noop(7).ok,false);h.alert.textContent='This value cannot be saved. Check the value and choices.';assert.equal(h.api.noop(7).ok,true);h.alert.box.top=-1;assert.equal(h.api.geometry('number-edit',7).ok,false);});
test('number-edit requires exact raw lexical value, footer labels, message and owned focus',()=>{const h=fixture();h.input.value='-12.3456';assert.equal(h.api.geometry('number-edit',7).ok,true);h.input.value='-12.346';assert.equal(h.api.geometry('number-edit',7).ok,false);h.input.value='-12.3456';assert.equal(h.api.geometry('number-edit',7).ok,true);h.cancel.textContent='Wrong';assert.equal(h.api.geometry('number-edit',7).ok,false);h.cancel.textContent='Cancel';h.document.activeElement=h.root;assert.equal(h.api.geometry('number-edit',7).ok,false);});
test('acknowledged retains raw input, Refresh and exact visible uncertainty',()=>{const h=fixture();h.input.value='-12.3456';h.input.disabled=h.save.disabled=true;h.buttons=[h.refresh,h.cancel,h.save];h.alert.textContent='This save is unconfirmed. Refresh to check its result before editing again.';h.cancel.focus();assert.equal(h.api.geometry('number-acknowledged',7).ok,true);h.alert.textContent='';assert.equal(h.api.geometry('number-acknowledged',7).ok,false);h.alert.textContent='This save is unconfirmed. Refresh to check its result before editing again.';assert.equal(h.api.geometry('number-acknowledged',7).ok,true);h.buttons=[h.cancel,h.save];assert.equal(h.api.geometry('number-acknowledged',7).ok,false);});
test('disabled Save replays invoke no handler',()=>{const h=fixture();h.input.disabled=h.save.disabled=true;assert.equal(h.api.disabledNoReplay(7).ok,true);assert.deepEqual(h.actions,[]);h.save.disabled=false;assert.equal(h.api.disabledNoReplay(7).ok,false);});
test('footer dimensions remain stable within generation while error height can move y',()=>{const h=fixture();h.input.value='-12.3456';assert.equal(h.api.geometry('number-edit',7).ok,true);h.alerts=[];h.save.textContent='Saving\u2026';h.input.disabled=h.save.disabled=true;h.cancel.focus();h.cancel.box.y+=20;h.save.box.y+=20;assert.equal(h.api.geometry('number-saving',7).ok,true);h.cancel.box.x+=1;assert.equal(h.api.geometry('number-saving',7).ok,false);});
test('reopen geometry uses raw input; clear-edit is empty and remains editable',()=>{const h=fixture();h.alerts=[];h.input.value='-12.3456';assert.equal(h.api.geometry('number-reopened',7).ok,true);h.input.value='';assert.equal(h.api.geometry('clear-edit',7).ok,true);h.input.value='0';assert.equal(h.api.geometry('clear-edit',7).ok,false);h.input.value='';assert.equal(h.api.geometry('clear-edit',7).ok,true);h.input.disabled=true;assert.equal(h.api.geometry('clear-edit',7).ok,false);});
test('table value is rounded display only and focus proof is passive',()=>{const h=fixture();h.api.open();h.modal=false;h.cell.focus();h.cell.text='-12.346';const n=h.reveals;assert.equal(h.api.geometry('number-observed',null).ok,true);assert.equal(h.reveals,n);h.cell.text='-12.3456';assert.equal(h.api.geometry('number-observed',null).ok,false);h.cell.text='';assert.equal(h.api.geometry('cleared',null).ok,true);h.document.activeElement=h.root;assert.equal(h.api.geometry('cleared',null).ok,false);});
test('meaningful clipping, malformed margins and wrong heading never pass',()=>{const h=fixture();h.api.open();h.modal=false;h.cell.focus();h.cell.text='';assert.equal(h.api.geometry('cleared',null).ok,true);h.cell.box.bottom=380.001;assert.equal(h.api.geometry('cleared',null).ok,false);h.cell.box.bottom=52;assert.equal(h.api.geometry('cleared',null).ok,true);h.heading.textContent='Wrong';assert.equal(h.api.geometry('cleared',null).ok,false);h.heading.textContent='Release count';h.cell.box.bottom=NaN;assert.equal(h.api.geometry('cleared',null).ok,false);});

test('shared configured tools are closed and default0 mode stays unchanged',()=>{for(const [kind,n,yes] of [['number',0,true],['number',3,true],['number',1,false],['text',3,false]]){const window={};vm.runInNewContext(shared,{window,document:{}});const h=window.__listTextCreate;assert.equal(h.configure(kind,n).ok,yes);assert.equal(!!h.numberTools(),yes&&n===3);assert.equal(!!h.dateTools(),false);}});
