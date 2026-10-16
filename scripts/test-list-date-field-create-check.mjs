import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import test from 'node:test';
const base=process.argv[2]||process.cwd();
const source=readFileSync(path.join(base,'src/desktop/slack_lists/check/text_field_create/date.js'),'utf8');
const shared=readFileSync(path.join(base,'src/desktop/slack_lists/check/text_field_create.js'),'utf8');
function fixture(helperSource=source,viewport=[640,380]){
 const nodes=new Map();let modal=true,reveals=0;const actions=[];
 const document={activeElement:null,querySelector:s=>nodes.get(s)||null,querySelectorAll:s=>s==='.slack-list-warning'?[warning]:[]};
 const box={x:20,y:20,left:20,top:20,right:100,bottom:52,width:80,height:32};
 function element(id){return{id,isConnected:true,disabled:false,readOnly:false,dataset:{},parentElement:null,style:{outlineStyle:'solid',outlineWidth:'2px',outlineOffset:'-2px',overflowX:'visible',overflowY:'visible'},box:{...box},getBoundingClientRect(){return this.box;},focus(){document.activeElement=this;},click(){if(!this.disabled)actions.push(this.id);},dispatchEvent(e){actions.push(e.type);},closest(){return null;},contains(e){return e===this;}};}
 class Input {get value(){return this._value||''}set value(v){this._value=v}}
 const input=Object.assign(new Input(),element('slack-list-date-draft'),{type:'date',validity:{valid:true,badInput:false}});
 const panel=element('slack-list-cell-editor');panel.dataset.editorGeneration='7';
 const clear=element('slack-list-date-clear'),cancel=element('slack-list-editor-cancel'),save=element('slack-list-editor-save'),refresh=element('slack-list-editor-refresh');
 const alert=Object.assign(element('alert'),{textContent:'This value cannot be saved. Check the value and choices.'});
 let buttons=[clear,cancel,save];panel.querySelectorAll=s=>s==='button'?buttons:s==='[role=alert]'?[alert]:[];
 panel.contains=e=>[panel,input,clear,cancel,save,refresh,alert].includes(e);
 const row=element('row');row.dataset={rowId:'R_LIST_003',instanceId:'instance1'};
 const cell=element('cell');cell.dataset.columnId='C_DATE_OBSERVED';cell.closest=s=>s==='[data-instance-id]'||s==='[data-row-id]'?row:null;
 const edit=element('edit'),primary=element('primary');cell.querySelector=()=>edit;
 row.querySelector=s=>s.includes('C_NAME')?primary:cell;
 const root=element('root');root.dataset={dialogGeneration:'2',viewSelection:'3'};root.scrollWidth=root.clientWidth=500;root.contains=e=>e===cell||e===row||e===primary;
 const heading=Object.assign(element('heading'),{textContent:'Target date'}),picker={value:'V_FILTER',selectedOptions:[{textContent:'Shared checklist'}]},warning=Object.assign(element('warning'),{textContent:'Only part of this List is loaded'}),find=element('find');
 const visible=e=>!!e&&e.isConnected&&!e.hidden&&e.box.width>0&&e.box.left>=0&&e.box.right<=viewport[0];
 const hit=e=>visible(e)&&!e.covered;
 const tools={q:s=>s==='#slack-list-cell-editor'?(modal?panel:null):s==='#slack-list-date-draft'?(modal?input:null):nodes.get(s)||null,rect:e=>e?[e.box.x,e.box.y,e.box.width,e.box.height]:null,visible,hit,reveal:()=>{reveals++;return[]},ids:()=>['R_LIST_003','R_LIST_001','R_LIST_007','R_LIST_005']};
 nodes.set('.slack-list-grid tr[data-row-id="R_LIST_003"]',row);nodes.set('.slack-list-dialog',root);nodes.set('.slack-list-grid',root);nodes.set('.slack-list-grid th[data-column-id="C_DATE_OBSERVED"]',heading);nodes.set('#slack-list-view-select',picker);nodes.set('.slack-list-find input',find);
 for(const e of [clear,cancel,save,refresh])nodes.set('#'+e.id,e);
 document.elementFromPoint=()=>clear;
 const window={__listTextCreate:{dateTools:()=>tools,findRetained:()=>({ok:true})}};
 const context={window,document,TextEncoder,devicePixelRatio:1.5,innerWidth:viewport[0],innerHeight:viewport[1],getComputedStyle:e=>e.style,HTMLInputElement:Input,Event:class{constructor(type){this.type=type}},KeyboardEvent:class{constructor(type,args){this.type=type;Object.assign(this,args)}}};
 assert.equal(vm.runInNewContext(helperSource,context).ok,true);
 input.focus();const api=window.__listDateCreate;assert.equal(api.ready('',7).ok,true);
 return{api,input,panel,cell,root,row,clear,cancel,save,refresh,alert,actions,document,nodes,context,get reveals(){return reveals;},set modal(v){modal=v;},set buttons(v){buttons=v;}};
}
test('shared configuration retains historical modes and adds only exact Date mode',()=>{
 for(const mode of ['text','number','checkbox','date']){const window={};vm.runInNewContext(shared,{window,document:{},innerWidth:640,innerHeight:380});assert.equal(window.__listTextCreate.configure(mode).ok,true);assert.equal(window.__listTextCreate.configure(mode).ok,false);assert.equal(!!window.__listTextCreate.dateTools(),mode==='date');}
 const window={};vm.runInNewContext(shared,{window,document:{}});assert.equal(window.__listTextCreate.configure('Date').ok,false);
});
test('native Date setter accepts exact leap date and reports actual invalidity',()=>{
 const h=fixture();assert.equal(h.api.input('2028-02-29',7).ok,true);assert.equal(h.input.value,'2028-02-29');assert.deepEqual(h.actions,['input']);
 assert.equal(h.api.input('2028-02-28',7).ok,false);h.input.validity.valid=false;assert.equal(h.api.input('2028-02-29',7).ok,false);
});
test('retired generation, disconnected input and covered input cannot dispatch',()=>{
 const h=fixture();assert.equal(h.api.input('2028-02-29',8).ok,false);h.input.covered=true;assert.equal(h.api.input('2028-02-29',7).ok,false);h.input.covered=false;h.input.isConnected=false;assert.equal(h.api.click('slack-list-editor-save',7).ok,false);assert.deepEqual(h.actions,[]);
});
test('empty no-op requires actual error, same native input and enabled valid draft',()=>{
 const h=fixture();assert.equal(h.api.noop(7).ok,true);h.alert.textContent='some error';assert.equal(h.api.noop(7).ok,false);h.alert.textContent='This value cannot be saved. Check the value and choices.';h.input.disabled=true;assert.equal(h.api.noop(7).ok,false);
});
test('editing geometry requires every exact button and native focus',()=>{
 const h=fixture();assert.equal(h.api.geometry('date-edit',7).ok,true);h.buttons=[h.clear,h.cancel];assert.equal(h.api.geometry('date-edit',7).ok,false);h.buttons=[h.clear,h.cancel,h.save];h.document.activeElement=h.root;assert.equal(h.api.geometry('date-edit',7).ok,false);
});
test('uncertain requires Refresh and disabled Date/Save/Clear with retained leap value',()=>{
 const h=fixture();h.input.value='2028-02-29';h.input.disabled=h.clear.disabled=h.save.disabled=true;h.buttons=[h.clear,h.refresh,h.cancel,h.save];h.cancel.focus();assert.equal(h.api.geometry('date-uncertain',7).ok,true);h.refresh.covered=true;assert.equal(h.api.geometry('date-uncertain',7).ok,false);h.refresh.covered=false;h.save.disabled=false;assert.equal(h.api.geometry('date-uncertain',7).ok,false);
});
test('disabled replays do not invoke mocked native click handlers',()=>{
 const h=fixture();h.input.disabled=h.clear.disabled=h.save.disabled=true;assert.equal(h.api.disabledNoReplay(7).ok,true);assert.deepEqual(h.actions,[]);h.clear.disabled=false;assert.equal(h.api.disabledNoReplay(7).ok,false);
});
test('return proof never repairs focus or scroll and rejects wrong logical cell',()=>{
 const h=fixture();h.modal=false;h.cell.focus();const n=h.reveals;assert.equal(h.api.returned().ok,true);assert.equal(h.reveals,n);h.document.activeElement=h.root;assert.equal(h.api.returned().ok,false);assert.equal(h.reveals,n);h.cell.focus();h.cell.dataset.columnId='wrong';assert.equal(h.api.returned().ok,false);
});
test('return focus outline must fit the actual clipping ancestor',()=>{
 const h=fixture();h.modal=false;h.cell.focus();h.cell.parentElement={style:{overflowX:'hidden',overflowY:'hidden'},getBoundingClientRect:()=>({...h.cell.box,right:99})};assert.equal(h.api.returned().ok,false);
});
test('cleared surface requires exact selected Table, Find and visible Date heading',()=>{
 const h=fixture();h.modal=false;h.cell.focus();assert.equal(h.api.geometry('cleared',null).ok,true);h.nodes.get('#slack-list-view-select').value='other';assert.equal(h.api.geometry('cleared',null).ok,false);h.nodes.get('#slack-list-view-select').value='V_FILTER';h.nodes.get('.slack-list-grid th[data-column-id="C_DATE_OBSERVED"]').hidden=true;assert.equal(h.api.geometry('cleared',null).ok,false);
});

test('CSS-derived Date Clear inset preserves the full focus ring at scroll-body edges',()=>{
 const baseCss=readFileSync(path.join(base,'src/desktop/styles/base.css'),'utf8');
 const listCss=readFileSync(path.join(base,'src/desktop/styles/slack-lists.css'),'utf8');
 const sharedRule=baseCss.match(/(?:^|\n):focus-visible\s*\{([^}]+)\}/)?.[1];
 assert.ok(sharedRule,'shared focus ring must remain present');
 const scopedRule=listCss.match(/\.slack-list-date-actions button:focus-visible\s*\{([^}]+)\}/)?.[1];
 const offset=rule=>rule?.match(/outline-offset:\s*(-?\d+)px/)?.[1];
 const width=sharedRule.match(/outline:\s*(\d+)px\s+solid/)?.[1];
 assert.equal(width,'2');assert.equal(offset(sharedRule),'2');
 const h=fixture(),before={...h.clear.box};
 // Model the button's right/bottom edges coincident with its scrollable body.
 // The actual helper supplies the outline-expansion check; this is not a browser cascade test.
 h.clear.parentElement={style:{overflowX:'auto',overflowY:'auto'},parentElement:null,
   getBoundingClientRect:()=>({...h.clear.box,left:0,top:0})};
 h.clear.style.outlineWidth=width+'px';
 h.clear.style.outlineOffset=(offset(scopedRule)??offset(sharedRule))+'px';
 assert.equal(h.api.click('slack-list-date-clear',7).ok,true);
 assert.deepEqual(h.actions,['slack-list-date-clear']);assert.deepEqual(h.clear.box,before);
 // Removing the scoped rule restores the native1 clipping mechanism and refuses before click.
 h.clear.style.outlineOffset=offset(sharedRule)+'px';
 assert.equal(h.api.click('slack-list-date-clear',7).ok,false);
 assert.deepEqual(h.actions,['slack-list-date-clear']);assert.deepEqual(h.clear.box,before);
});


const plain=value=>JSON.parse(JSON.stringify(value));
function clipParent(h,delta=0){
 const parent={isConnected:true,id:'body-clip',tagName:'DIV',className:'slack-list-editor-body',
  style:{overflowX:'auto',overflowY:'auto',display:'block',visibility:'visible'},parentElement:null,
  getBoundingClientRect:()=>({...h.clear.box,left:0,top:0,right:h.clear.box.right+delta,bottom:h.clear.box.bottom+delta}),
  clientLeft:0,clientTop:0,clientWidth:100,clientHeight:52,scrollLeft:0,scrollTop:0,scrollWidth:100,scrollHeight:52};
 h.clear.parentElement=parent;return parent;
}
test('Date click diagnostics identify every original rejecting branch without click',()=>{
 const cases=[
  ['owner',h=>{h.panel.dataset.editorGeneration='8';},'slack-list-date-clear'],
  ['id',()=>{},'unsupported'],
  ['missing',h=>h.nodes.delete('#slack-list-date-clear'),'slack-list-date-clear'],
  ['disabled',h=>{h.clear.disabled=true;},'slack-list-date-clear'],
  ['hit',h=>{h.clear.covered=true;},'slack-list-date-clear'],
  ['focus',h=>{h.clear.focus=()=>{};},'slack-list-date-clear'],
  ['ring',h=>{clipParent(h);h.clear.style.outlineOffset='2px';},'slack-list-date-clear']
 ];
 for(const [branch,change,id] of cases){const h=fixture();change(h);const r=h.api.click(id,7);assert.equal(r.ok,false);assert.equal(r.id,id);assert.equal(r.diagnostic.branch,branch);assert.deepEqual(h.actions,[]);assert.equal(h.reveals,0);}
});
test('Date ring failure retains exact native rectangle, outline, clip, comparisons and hit samples',()=>{
 const h=fixture();clipParent(h);h.clear.style.outlineOffset='2px';const d=h.api.click('slack-list-date-clear',7).diagnostic;
 assert.equal(d.branch,'ring');assert.equal(d.activeIsElement,true);assert.equal(d.ring.visible,true);
 assert.deepEqual(plain(d.ring.outline),{style:'solid',width:'2px',offset:'2px',expand:4});
 assert.deepEqual(plain(d.ring.clip),{left:0,top:0,right:100,bottom:52});
 assert.deepEqual(plain(d.ring.comparisons),{left:true,top:true,right:false,bottom:false});
 assert.deepEqual(plain(d.ring.margins),{left:16,top:16,right:-4,bottom:-4});
 assert.equal(d.ring.rect.right,100);assert.equal(d.ring.rect.bottom,52);assert.equal(d.ancestors[0].overflowX,'auto');
 assert.equal(d.observationsAfterFailure.hits.length,9);assert.ok(d.observationsAfterFailure.hits.every(p=>p.inside));assert.equal(d.devicePixelRatio,1.5);
});
test('Date ring rejects meaningful fractional clipping and accepts exact zero edges',()=>{
 const h=fixture(),edge=100-1/1024;const p=clipParent(h);p.getBoundingClientRect=()=>({...h.clear.box,left:0,top:0,right:edge});
 const d=h.api.click('slack-list-date-clear',7).diagnostic;assert.equal(d.branch,'ring');assert.equal(d.ring.clip.right,edge);
 assert.equal(d.ring.margins.right,edge-100);assert.ok(d.ring.margins.right<0);assert.equal(d.ring.comparisons.right,false);assert.equal(d.ring.comparisons.bottom,true);
 p.getBoundingClientRect=()=>({...h.clear.box,left:0,top:0});const r=h.api.click('slack-list-date-clear',7);assert.equal(r.ok,true);assert.equal(Object.hasOwn(r,'diagnostic'),false);
});
test('Date diagnostic bounds retain all-ancestor predicate even beyond 24 retained ancestors',()=>{
 const h=fixture();let p=clipParent(h);for(let index=1;index<31;index++){const next={...p,id:'ancestor-'+index,parentElement:null,getBoundingClientRect:()=>({...h.clear.box,left:0,top:0,right:1000,bottom:1000})};p.parentElement=next;p=next;}
 p.getBoundingClientRect=()=>({...h.clear.box,left:0,top:0,right:99,bottom:52});
 const d=h.api.click('slack-list-date-clear',7).diagnostic;assert.equal(d.branch,'ring');assert.equal(d.ring.ancestorCount,31);assert.equal(d.ring.ancestorsTruncated,true);assert.equal(d.ancestors.length,24);assert.equal(d.ring.limiters.right,30);assert.equal(d.ring.margins.right,-1);assert.ok(new TextEncoder().encode(JSON.stringify(d)).length<=16384);
});
test('Date diagnostic size and getter failures cannot replace the original rejection',()=>{
 const h=fixture();h.clear.disabled=true;h.clear.style.display='?'.repeat(20000);let r=h.api.click('slack-list-date-clear',7);
 assert.equal(r.ok,false);assert.equal(r.diagnostic.branch,'disabled');assert.equal(r.diagnostic.truncated,true);assert.ok(new TextEncoder().encode(JSON.stringify(r.diagnostic)).length<=16384);assert.deepEqual(h.actions,[]);
 const f=fixture();f.clear.disabled=true;f.context.getComputedStyle=()=>{throw new Error('diagnostic getter failed '+'x'.repeat(2000));};r=f.api.click('slack-list-date-clear',7);
 assert.equal(r.ok,false);assert.equal(r.id,'slack-list-date-clear');assert.equal(r.diagnostic.branch,'disabled');assert.ok(r.diagnostic.diagnosticError.startsWith('Error: diagnostic getter failed'));assert.ok(r.diagnostic.diagnosticError.length<=1024);assert.deepEqual(f.actions,[]);
});
test('Date successful click remains byte-equivalent to the original literal result and action history',()=>{
 const h=fixture();const r=h.api.click('slack-list-date-clear',7);
 assert.equal(JSON.stringify(r),'{"ok":true,"kind":"click","id":"slack-list-date-clear","generation":"7","hitPoints":9}');assert.deepEqual(h.actions,['slack-list-date-clear']);
 const geometry=h.api.geometry('date-edit',7);assert.deepEqual(plain(geometry.actions),[plain(r)]);assert.equal(Object.hasOwn(geometry.actions[0],'diagnostic'),false);
});

const ringAllowance=2**-16;
test('Date ring accepts the retained native3 serialized edges without inventing exact margin equality',()=>{
 const h=fixture(source,[1280,780]);
 h.clear.box={x:825.28125,y:416.6666870117187,left:825.28125,top:416.6666870117187,right:903.3333358764648,bottom:439.6666870117187,width:78.05208587646484,height:23};
 const clip={left:376.6666870117187,top:358.3333435058594,right:903.3333740234376,bottom:439.6666793823242};
 const p=clipParent(h);p.getBoundingClientRect=()=>clip;
 const replayedMargin=clip.bottom-h.clear.box.bottom,recordedMargin=-(2**-17);
 assert.equal(replayedMargin,-0.000007629394474406581);
 assert.notEqual(replayedMargin,recordedMargin,'serialized edge subtraction is not the recorded predicate-time witness');
 assert.ok(replayedMargin<0&&replayedMargin>=-ringAllowance);
 assert.ok(recordedMargin<0&&recordedMargin>=-ringAllowance);
 assert.equal(h.clear.style.outlineWidth,'2px');assert.equal(h.clear.style.outlineOffset,'-2px');
 assert.equal(h.api.click('slack-list-date-clear',7).ok,true);assert.deepEqual(h.actions,['slack-list-date-clear']);
});

function nextPositive(value){const data=new DataView(new ArrayBuffer(8));data.setFloat64(0,value);data.setBigUint64(0,data.getBigUint64(0)+1n);return data.getFloat64(0);}
function marginFixture(edge,margin){
 const h=fixture(),clip={left:0,top:0,right:200,bottom:200},amount=-margin;
 // Zero-origin arithmetic makes the ACTUAL final subtraction exact, including
 // the immediately adjacent binary64 value; no large-coordinate cancellation.
 if(edge==='left'){h.clear.box.left=0;clip.left=amount;}
 if(edge==='top'){h.clear.box.top=0;clip.top=amount;}
 if(edge==='right'){h.clear.box.left=0;h.clear.box.right=amount||1;clip.right=amount?0:1;h.clear.box.width=h.clear.box.right;}
 if(edge==='bottom'){h.clear.box.top=0;h.clear.box.bottom=amount||1;clip.bottom=amount?0:1;h.clear.box.height=h.clear.box.bottom;}
 h.clear.box.x=h.clear.box.left;h.clear.box.y=h.clear.box.top;
 const p=clipParent(h);p.getBoundingClientRect=()=>clip;
 const computed=edge==='left'?h.clear.box.left-clip.left:edge==='top'?h.clear.box.top-clip.top:clip[edge]-h.clear.box[edge];
 assert.equal(computed,margin,'verify the actual computed margin, not merely the intended perturbation');
 return h;
}
test('Date ring applies the inclusive fixed boundary once on each final finite margin',()=>{
 const below=-nextPositive(ringAllowance);
 assert.ok(below < -ringAllowance);assert.equal(below,-1.5258789062500003e-5);
 for(const edge of ['left','top','right','bottom'])for(const margin of [0,-ringAllowance,below,-2*ringAllowance,-1/1024,-0.01,-4]){
  const h=marginFixture(edge,margin),r=h.api.click('slack-list-date-clear',7),expected=margin>=-ringAllowance;
  assert.equal(r.ok,expected,`${edge}: ${margin}`);
  assert.deepEqual(h.actions,expected?['slack-list-date-clear']:[]);
  if(!expected){assert.equal(r.diagnostic.branch,'ring');assert.equal(r.diagnostic.ring.margins[edge],margin);assert.equal(r.diagnostic.ring.allowanceCss,ringAllowance);assert.equal(r.diagnostic.ring.adjustedComparisons[edge],false);}
 }
});
test('Date ring preserves raw strict refusal alongside amended diagnostics without scaling by DPR',()=>{
 for(const dpr of [1,1.5,3]){
  const h=fixture();h.context.devicePixelRatio=dpr;const p=clipParent(h);
  p.getBoundingClientRect=()=>({...h.clear.box,left:0,top:0,right:h.clear.box.right-ringAllowance,bottom:h.clear.box.bottom-1/1024});
  const d=h.api.click('slack-list-date-clear',7).diagnostic;
  assert.equal(d.ring.margins.right,-ringAllowance);assert.equal(d.ring.comparisons.right,false);assert.equal(d.ring.adjustedComparisons.right,true);
  assert.equal(d.ring.comparisons.bottom,false);assert.equal(d.ring.adjustedComparisons.bottom,false);assert.equal(d.ring.allowanceCss,ringAllowance);
 }
});
test('Date ring rejects nonfinite margins instead of granting unbounded geometry',()=>{
 for(const value of [NaN,Infinity,-Infinity]){
  const h=fixture(),p=clipParent(h);h.context.innerHeight=value;p.style.overflowY='visible';
  const r=h.api.click('slack-list-date-clear',7);assert.equal(r.ok,false);assert.equal(r.diagnostic.branch,'ring');assert.equal(r.diagnostic.ring.adjustedComparisons.bottom,false);assert.deepEqual(h.actions,[]);
 }
});
test('Date ring allowance neither accumulates over ancestors nor replaces outward outline expansion',()=>{
 const h=fixture();let p=clipParent(h);for(let i=1;i<31;i++){const next={...p,id:'limit-'+i,parentElement:null,getBoundingClientRect:()=>({...h.clear.box,left:0,top:0,right:100,bottom:52})};p.parentElement=next;p=next;}
 p.getBoundingClientRect=()=>({...h.clear.box,left:0,top:0,right:100-2*ringAllowance,bottom:52});
 let r=h.api.click('slack-list-date-clear',7);assert.equal(r.ok,false);assert.equal(r.diagnostic.ring.ancestorCount,31);assert.equal(r.diagnostic.ring.adjustedComparisons.right,false);assert.equal(r.diagnostic.ring.margins.right,-2*ringAllowance);
 const f=fixture();clipParent(f);f.clear.style.outlineOffset='2px';r=f.api.click('slack-list-date-clear',7);
 assert.equal(r.ok,false);assert.equal(r.diagnostic.ring.outline.expand,4);assert.equal(r.diagnostic.ring.margins.right,-4);assert.equal(r.diagnostic.ring.adjustedComparisons.right,false);
});
