import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import test from 'node:test';
const source=readFileSync(path.join(process.argv[2]||process.cwd(),'src/desktop/slack_lists/check/text_field_create/rating/cell.js'),'utf8');
const circle='M480-80q-83 0-156-31.5T197-197q-54-54-85.5-127T80-480q0-83 31.5-156T197-763q54-54 127-85.5T480-880q83 0 156 31.5T763-763q54 54 85.5 127T880-480q0 83-31.5 156T763-197q-54 54-127 85.5T480-80Z';
function fixture(which='standard'){
 const config={default:[1,1,':star:'],standard:[5,4,':heart:'],custom:[3,2,':fixture_quality:']}[which], [max,k,emoji]=config;
 const nodes=new Map(),actions=[];let modal=true,locked=false,value=null,alerts=[],footer=[],displayValue=0,displayReady=false,fieldVisible=true,detail=false,board=false,reveals=0,unexpectedFind=null;
 const document={activeElement:null,querySelectorAll:s=>s.includes('.slack-list-card')?[card,...['R_LIST_001','R_LIST_007','R_LIST_005'].map(rowId=>({dataset:{rowId}}))]:[]};
 function el(id){return{id,isConnected:true,hidden:false,covered:false,disabled:false,dataset:{},box:[20,20,80,32],parentElement:null,style:{outlineStyle:'solid',outlineWidth:'2px',outlineOffset:'-2px'},textContent:'',querySelector(){return null;},querySelectorAll(){return[];},getAttribute(){return null;},getBoundingClientRect(){const[x,y,width,height]=this.box;return{x,y,width,height,left:x,top:y,right:x+width,bottom:y+height};},focus(){document.activeElement=this;},click(){if(!this.disabled)actions.push(id);},contains(e){return e===this;},closest(){return null;},matches(s){return s===':disabled'&&!!(this.disabled||this.locked&&locked);}};}
 const p=el('slack-list-cell-editor');p.dataset.editorGeneration='7';const choices=Array.from({length:max},(_,i)=>{const e=el('choice'+(i+1));e.dataset.ratingChoice=String(i+1);e.locked=true;e.getAttribute=n=>n==='aria-label'?`${i+1} of ${max}`:n==='aria-pressed'?String(value===i+1):null;e.click=()=>{if(!e.matches(':disabled')){value=value===i+1?null:i+1;actions.push(e.id);}};return e;});
 const clear=el('slack-list-rating-clear');clear.locked=true;Object.defineProperty(clear,'disabled',{get:()=>locked||value===null});clear.click=()=>{if(!clear.disabled){value=null;actions.push('clear');}};
 const status=el('status');Object.defineProperty(status,'textContent',{get:()=>value===null?'Unrated':`${value} of ${max}`});
 const cancel=el('slack-list-editor-cancel'),save=el('slack-list-editor-save'),refresh=el('slack-list-editor-refresh');cancel.textContent='Cancel';save.textContent='Save';refresh.textContent='Refresh';save.box=[450,300,80,32];cancel.box=[360,300,80,32];footer=[cancel,save];
 const alert=el('alert');alert.textContent='This value cannot be saved. Check the value and choices.';
 p.querySelectorAll=s=>s==='.slack-list-rating-choice'?choices:s==='button'?[...choices,clear,...footer]:s==='[role=alert]'?alerts:[];
 p.querySelector=s=>s==='.slack-list-rating-fields [role=status]'?status:null;p.contains=e=>[p,...choices,clear,status,...footer,...alerts].includes(e);
 for(const e of [...choices,clear])e.closest=s=>s==='.slack-list-rating-fields'?{}:null;
 const row=el('row');row.dataset={rowId:'R_LIST_003',instanceId:'i3'};
 const cell=el('cell');cell.dataset.columnId='C_RATING_OBSERVED';cell.closest=s=>['[data-row-id]','[data-instance-id]'].includes(s)?row:null;
 const edit=el('edit'),root=el('root');root.dataset={dialogGeneration:'2',viewSelection:'3'};root.scrollWidth=root.clientWidth=600;root.contains=e=>[root,cell,row,card,view,find].includes(e);
 const card=el('card');card.dataset.rowId='R_LIST_003';const detailNode=el('slack-list-record-detail');detailNode.dataset={recordId:'R_LIST_003',detailGeneration:'8',detailOpenGeneration:'9'};detailNode.contains=e=>[cell,detailNode].includes(e);
 const view={value:'V_FILTER',selectedOptions:[{textContent:'Shared checklist'}]};let find=el('find'),boundFind=find,boundRange=[3,7,'forward'];find.value='Launch item 0';[find.selectionStart,find.selectionEnd,find.selectionDirection]=boundRange;detailNode.isConnected=false;const openRecord=el('open-record'),back=el('slack-list-record-back');
 let marks=[],group=null,fallback=null,groups=1;
 function render(n,ready=false,table=true){displayValue=n;displayReady=ready;marks=[];group=null;fallback=null;
  if(which==='custom'&&!ready){fallback=el('fallback');fallback.dataset={ratingValue:String(n),ratingMax:String(max)};fallback.textContent=`${n} of ${max} :fixture_quality:`;fallback.scrollWidth=fallback.clientWidth=180;return;}
  group=el('rating');group.dataset={ratingValue:String(n),ratingMax:String(max)};
  marks=Array.from({length:table?max:n},(_,i)=>{const m=el('mark'+i),glyph=el('glyph'+i),img=el('image'+i),svg=el('svg'+i);glyph.textContent=which==='default'?'★':'♥';glyph.getAttribute=()=>emoji;img.complete=true;img.naturalWidth=16;
   m.glyph=glyph;m.img=img;m.svg=svg;m.path=circle;m.extra=0;
   m.querySelector=s=>i<n?(s==='.custom-emoji'&&which!=='custom'?glyph:s==='img.emoji-image'&&which==='custom'?img:null):(s==='svg'?svg:s==='svg path'?{getAttribute:()=>m.path}:null);
   m.querySelectorAll=s=>i<n?(s==='.custom-emoji'&&which!=='custom'?Array(1+m.extra).fill(glyph):s==='img.emoji-image'&&which==='custom'?Array(1+m.extra).fill(img):[]):s==='svg'?[svg]:[];return m;});
 }
 render(0);
 cell.querySelectorAll=s=>s==='.slack-list-rating-mark'?marks:s==='.slack-list-rating,.slack-list-rating-fallback'?Array(groups).fill(group||fallback):[];
 cell.querySelector=s=>s==='.slack-list-edit-cell'?edit:s==='.slack-list-rating'?group:s==='.slack-list-rating-fallback'?fallback:s==='.slack-list-detail-empty'?empty:s==='.slack-list-rating,.slack-list-rating-fallback'?(group||fallback):null;
 const empty=el('empty');empty.textContent='Empty';row.querySelector=()=>cell;card.querySelector=s=>s.includes('C_RATING_OBSERVED')?(fieldVisible?cell:null):s==='.slack-list-record-open'?openRecord:null;detailNode.querySelector=()=>cell;
 const visible=e=>!!e&&e.isConnected&&!e.hidden&&e.box[2]>0&&e.box[3]>0&&e.box[0]>=0&&e.box[1]>=0&&e.box[0]+e.box[2]<=640&&e.box[1]+e.box[3]<=380;
 const tools={q:s=>s==='.slack-list-find input'?(detail?unexpectedFind:find):s==='#slack-list-record-back'?back:s==='#slack-list-cell-editor'?(modal?p:null):s==='#slack-list-record-detail'?(detail?detailNode:null):s==='.slack-list-board'?(board?root:null):s==='.slack-list-grid'?(!board?root:null):nodes.get(s)||null,rect:e=>e?.box.slice()||null,visible,hit:e=>visible(e)&&!e.covered,reveal:()=>{reveals++;return[];},ids:()=>['R_LIST_003','R_LIST_001','R_LIST_007','R_LIST_005']};
 for(const [s,e]of [['.slack-list-grid tr[data-row-id="R_LIST_003"]',row],['.slack-list-card[data-row-id="R_LIST_003"]',card],['.slack-list-dialog',root],['#slack-list-view-select',view],...[[clear,cancel,save,refresh].map(e=>['#'+e.id,e])].flat()])nodes.set(s,e);
 const window={__listTextCreate:{ratingTools:()=>tools,findRetained:()=>({ok:!detail&&find===boundFind&&find.isConnected&&find.value==='Launch item 0'&&JSON.stringify([find.selectionStart,find.selectionEnd,find.selectionDirection])===JSON.stringify(boundRange),sameNode:find===boundFind,value:find.value,range:[find.selectionStart,find.selectionEnd,find.selectionDirection]}),bindFind:()=>{boundFind=find;boundRange=[find.selectionStart,find.selectionEnd,find.selectionDirection];return{ok:true,value:find.value,range:boundRange.slice()};}}};
 assert.equal(vm.runInNewContext(source,{window,document,innerWidth:640,innerHeight:380,getComputedStyle:e=>({overflowX:'visible',overflowY:'visible',...e.style})}).ok,true);
 const api=window.__listRatingCells;assert.equal(api.init(which).ok,true);choices[0].focus();assert.equal(api.ready(null,7).ok,true);
 return{api,get find(){return find;},replaceFind(e){find=e;},unexpectedFind(e){unexpectedFind=e;},tools,choices,clear,status,cancel,save,refresh,alert,cell,root,card,view,document,p,actions,render,emptyDisplay(){marks=[];group=null;fallback=null;},detailNode,get marks(){return marks;},get fallback(){return fallback;},get reveals(){return reveals;},set modal(v){modal=v;},set locked(v){locked=v;},set alerts(v){alerts=v;},set footer(v){footer=v;},set groups(v){groups=v;},set fieldVisible(v){fieldVisible=v;},set detail(v){if(v){assert.equal(api.openDetail().ok,true);find.isConnected=false;detailNode.isConnected=true;}else{detailNode.isConnected=false;find=el('find');find.value='Launch item 0';find.selectionStart=0;find.selectionEnd=0;find.selectionDirection='none';}detail=v;board=true;view.value='V_RATING_BOARD';view.selectedOptions[0].textContent='Rating board';},set board(v){board=v;view.value=v?'V_RATING_BOARD':'V_FILTER';view.selectedOptions[0].textContent=v?'Rating board':'Shared checklist';},set value(v){value=v;},k,max,choose(){assert.equal(api.choose(k,7).ok,true);assert.equal(api.chosen(k,7).ok,true);}};
}
test('actual Rating buttons support selection, same-value clear and explicit clear for all three maxima',()=>{
 for(const which of ['default','standard','custom']){const h=fixture(which);h.choose();assert.equal(h.api.choose(h.k,7).ok,true);assert.equal(h.api.chosen(null,7).ok,true);assert.equal(h.clear.disabled,true);h.choose();assert.equal(h.api.click('slack-list-rating-clear',7).ok,true);assert.equal(h.api.chosen(null,7).ok,true);assert.equal(h.api.choose(h.max+1,7).ok,false);}
});
test('editable surfaces require exact button identity, full rings and nonoccluded hit regions',()=>{
 for(const mutate of [h=>h.choices[0].hidden=true,h=>h.choices[0].covered=true,h=>h.choices[0].box=[620,20,80,32],h=>h.choices[0].dataset.ratingChoice='9',h=>h.p.dataset.editorGeneration='8',h=>h.clear.hidden=true]){const h=fixture();h.choose();assert.equal(h.api.geometry('rating-edit',7,false).ok,true);mutate(h);assert.equal(h.api.geometry('rating-edit',7,false).ok,false);}
});
test('no-op proves exact visible error and no selected value',()=>{const h=fixture();h.alerts=[h.alert];assert.equal(h.api.noop(7).ok,true);h.alert.hidden=true;assert.equal(h.api.noop(7).ok,false);h.alert.hidden=false;h.choose();assert.equal(h.api.noop(7).ok,false);});
test('saving and acknowledgment require exact labels, locked controls, visible uncertainty and stable horizontal footer',()=>{
 const h=fixture();h.choose();h.api.click('slack-list-editor-save',7);h.locked=true;h.save.disabled=true;h.save.textContent='Saving…';h.cancel.focus();assert.equal(h.api.geometry('rating-saving',7,false).ok,true);
 const count=h.actions.length;assert.equal(h.api.disabledNoReplay(7).ok,true);assert.equal(h.actions.length,count);
 h.footer=[h.refresh,h.cancel,h.save];h.save.textContent='Save';h.alert.textContent='This save is unconfirmed. Refresh to check its result before editing again.';h.alerts=[h.alert];h.cancel.box[1]=330;h.save.box[1]=330;assert.equal(h.api.geometry('rating-acknowledged',7,false).ok,true);
 h.alert.hidden=true;assert.equal(h.api.geometry('rating-acknowledged',7,false).ok,false);h.alert.hidden=false;h.save.box[2]=98;assert.equal(h.api.geometry('rating-acknowledged',7,false).ok,false);
});
test('Table displays max slots and rejects wrong placeholders or duplicate selected glyphs',()=>{
 const h=fixture();h.modal=false;h.cell.focus();h.render(4);assert.equal(h.api.geometry('rating-observed',null,false).ok,true);
 h.marks[4].path='outlined-circle';assert.equal(h.api.geometry('rating-observed',null,false).ok,false);h.marks[4].path=circle;h.marks[0].extra=1;assert.equal(h.api.geometry('rating-observed',null,false).ok,false);h.marks[0].extra=0;h.groups=2;assert.equal(h.api.geometry('rating-observed',null,false).ok,false);
});
test('custom pending fallback is fully readable and only decoded ready images permit glyph evidence',()=>{
 const h=fixture('custom');h.modal=false;h.cell.focus();h.render(2);assert.equal(h.api.geometry('rating-observed',null,false).ok,true);h.fallback.scrollWidth=400;assert.equal(h.api.geometry('rating-observed',null,false).ok,false);
 h.render(2,true);assert.equal(h.api.geometry('custom-ready',null,true).ok,true);h.marks[0].img.complete=false;assert.equal(h.api.geometry('custom-ready',null,true).ok,false);h.marks[0].img.complete=true;h.marks[0].img.naturalWidth=0;assert.equal(h.api.geometry('custom-ready',null,true).ok,false);
});
test('returned focus is sampled without scrolling or repairing a different active element',()=>{const h=fixture();h.modal=false;h.cell.focus();const n=h.reveals;assert.equal(h.api.returned().ok,true);h.root.focus();assert.equal(h.api.returned().ok,false);assert.equal(h.reveals,n);h.cell.focus();h.cell.box=[20,370,80,32];assert.equal(h.api.returned().ok,false);assert.equal(h.reveals,n);});
test('cleared Table keeps max filled placeholders rather than stored zero glyph',()=>{const h=fixture();h.modal=false;h.cell.focus();h.render(0);assert.equal(h.api.geometry('cleared',null,false).ok,true);h.render(1);assert.equal(h.api.geometry('cleared',null,false).ok,false);});

test('Board shows only selected count, omits empty Rating, and refuses Table placeholders or wrong layout',()=>{
 const h=fixture();h.modal=false;h.board=true;h.root.focus();h.render(4,false,false);
 assert.equal(h.api.geometry('rating-board',null,false).ok,true);
 h.view.selectedOptions[0].textContent='Other board';assert.equal(h.api.geometry('rating-board',null,false).ok,false);h.view.selectedOptions[0].textContent='Rating board';
 h.render(4,false,true);assert.equal(h.api.geometry('rating-board',null,false).ok,false);
 h.render(4,false,false);h.board=false;assert.equal(h.api.geometry('rating-board',null,false).ok,false);
 h.board=true;h.fieldVisible=false;assert.equal(h.api.geometry('empty-board',null,false).ok,true);
 h.fieldVisible=true;assert.equal(h.api.geometry('empty-board',null,false).ok,false);
 h.fieldVisible=false;h.card.hidden=true;assert.equal(h.api.geometry('empty-board',null,false).ok,false);
});
test('detail shows selected count or truthful Empty and requires its own focus surface',()=>{
 const h=fixture();h.modal=false;h.detail=true;h.detailNode.focus();h.render(4,false,false);
 assert.equal(h.api.geometry('rating-detail',null,false).ok,true);
 h.render(4,false,true);assert.equal(h.api.geometry('rating-detail',null,false).ok,false);
 h.emptyDisplay();assert.equal(h.api.geometry('empty-detail',null,false).ok,true);
 h.root.focus();assert.equal(h.api.geometry('empty-detail',null,false).ok,false);
 h.detailNode.focus();h.render(1,false,false);assert.equal(h.api.geometry('empty-detail',null,false).ok,false);
});

test('detail unmounts Find while model-facing value remains retained; remount observes a distinct new input and actual range',()=>{
 const h=fixture();h.modal=false;h.board=true;const old=h.find,range=[old.selectionStart,old.selectionEnd,old.selectionDirection];
 h.detail=true;h.detailNode.focus();h.render(4,false,false);
 const ready=h.api.detailReady();assert.equal(ready.ok,true);assert.equal(ready.find.absent,true);assert.equal(ready.find.previousDisconnected,true);
 assert.deepEqual(Array.from(ready.find.range),range);assert.equal(h.api.geometry('rating-detail',null,false).ok,true);
 h.detail=false;h.root.focus();const focus=h.document.activeElement,reb=h.api.rebindFind();
 assert.equal(reb.ok,true);assert.equal(reb.sameNode,false);assert.equal(reb.previousDisconnected,true);assert.equal(reb.detailDisconnected,true);
 assert.deepEqual(Array.from(reb.before.range),range);assert.deepEqual(Array.from(reb.after.range),[0,0,'none']);assert.equal(h.document.activeElement,focus);
 assert.equal(h.api.geometry('rating-board',null,false).ok,true);
});
test('detail absence rejects a retained live old input, unexpected new input and owner replacement',()=>{
 for(const fault of ['connected','unexpected','visit','selection','root']){const h=fixture();h.modal=false;h.board=true;const old=h.find;h.detail=true;h.detailNode.focus();h.render(4,false,false);
  assert.equal(h.api.detailReady().ok,true);
  if(fault==='connected')old.isConnected=true;
  if(fault==='unexpected')h.unexpectedFind({isConnected:true,value:'Launch item 0'});
  if(fault==='visit')h.root.dataset.dialogGeneration='99';if(fault==='selection')h.root.dataset.viewSelection='99';if(fault==='root')h.root.isConnected=false;
  assert.equal(h.api.detailReady().ok,false);assert.equal(h.api.geometry('rating-detail',null,false).ok,false);
 }
});
test('Find rebind refuses same node, changed query, invalid range, live detail and stale owner without focus repair',()=>{
 for(const fault of ['same','query','range','detail','owner']){const h=fixture();h.modal=false;h.board=true;const old=h.find;h.detail=true;h.detailNode.focus();assert.equal(h.api.detailReady().ok,true);h.detail=false;h.root.focus();
  if(fault==='same'){old.isConnected=true;h.replaceFind(old);}if(fault==='query')h.find.value='other';if(fault==='range')h.find.selectionEnd=99;if(fault==='detail')h.detailNode.isConnected=true;if(fault==='owner')h.root.dataset.viewSelection='99';
  const active=h.document.activeElement;assert.equal(h.api.rebindFind().ok,false);assert.equal(h.document.activeElement,active);
 }
});
test('detail reset editor return uses expected Find absence without weakening exact cell focus',()=>{
 const h=fixture();h.modal=false;h.board=true;h.detail=true;h.detailNode.focus();assert.equal(h.api.detailReady().ok,true);h.cell.focus();
 assert.equal(h.api.returned().ok,true);h.root.focus();assert.equal(h.api.returned().ok,false);
});
