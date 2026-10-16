(()=>{
 const q=s=>document.querySelector(s),rect=e=>{const r=e?.getBoundingClientRect();return r?[r.x,r.y,r.width,r.height]:null;};
 const visible=e=>{
  if(!e||e.closest('[hidden],[inert]'))return false;
  const r=e.getBoundingClientRect();if(r.width<=0||r.height<=0||r.left< -1||r.top< -1||r.right>innerWidth+1||r.bottom>innerHeight+1)return false;
  for(let p=e.parentElement;p;p=p.parentElement){const s=getComputedStyle(p),b=p.getBoundingClientRect();if(s.display==='none'||s.visibility==='hidden')return false;if(/auto|scroll|hidden|clip/.test(s.overflowX)&&(r.left<b.left-1||r.right>b.right+1))return false;if(/auto|scroll|hidden|clip/.test(s.overflowY)&&(r.top<b.top-1||r.bottom>b.bottom+1))return false;}
  return true;
 };
 const hit=e=>{if(!visible(e))return false;const r=e.getBoundingClientRect();return [.15,.5,.85].every(x=>[.15,.5,.85].every(y=>{const f=document.elementFromPoint(r.left+r.width*x,r.top+r.height*y);return f===e||e.contains(f);}));};
 const find=()=>q('.slack-list-find input');let opener=null,findNode=null,findRange=null,numberMode=false;
 const orders={none:[2,3,1,4,7,8,0,5,6],checked:[3,1,7,5],unchecked:[2,4,8,0,6]};
 const rowIds=()=>[...document.querySelectorAll('.slack-list-grid tr[data-row-id]')].map(e=>e.dataset.rowId);
 window.__listViewFilter={
  numberMode(){numberMode=true;return{ok:true};},
  click(selector){const e=q(selector);e?.scrollIntoView({block:'nearest',inline:'nearest'});if(!e||e.disabled||!hit(e))return{ok:false,selector};e.focus();e.click();return{ok:true,selector,hitPoints:9};},
  select(selector,value){const e=q(selector);e?.scrollIntoView({block:'nearest',inline:'nearest'});if(!(e instanceof HTMLSelectElement)||e.disabled||!hit(e)||![...e.options].some(o=>o.value===value))return{ok:false,selector};e.focus();Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(e,value);e.dispatchEvent(new Event('change',{bubbles:true}));return{ok:true,selector,value};},
  query(){const e=find();if(!(e instanceof HTMLInputElement)||e.disabled||!hit(e))return{ok:false};e.focus();Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,'Launch item 0');e.dispatchEvent(new Event('input',{bubbles:true}));return{ok:true};},
  bindFind(){const e=find();if(!e||e.value!=='Launch item 0')return{ok:false};findNode=e;findRange=[e.selectionStart,e.selectionEnd,e.selectionDirection];return{ok:true,value:e.value,range:findRange};},
  findRetained(){const e=find();const range=e?[e.selectionStart,e.selectionEnd,e.selectionDirection]:null;return{ok:!!e&&e===findNode&&e.value==='Launch item 0'&&JSON.stringify(range)===JSON.stringify(findRange),sameNode:e===findNode,value:e?.value,range};},
  rememberFilter(){opener=q('#slack-list-filter');return{ok:!!opener&&opener.isConnected};},
  returnedFilter(){const current=q('#slack-list-filter');return{ok:!!opener&&opener===current&&opener===document.activeElement&&opener.isConnected&&!q('#slack-list-rename'),sameNode:opener===current,focused:opener===document.activeElement};},
  draft(){const panel=q('#slack-list-rename'),condition=q('#slack-list-filter-condition');return{ok:!!panel&&!!condition,serial:panel?.dataset.renameSerial,choice:condition?.dataset.filterChoice,condition:condition?.value,conditionDisabled:condition?.disabled,focused:document.activeElement?.id};},
  revealFirst(){const e=q('.slack-list-grid tr[data-row-id]');if(!e)return{ok:false};e.scrollIntoView({block:'nearest',inline:'nearest'});return{ok:true};},
  geometry(stage){
   const captureStage=stage;if(numberMode)stage=({'compare-edit':'checked-edit','compare-saving':'checked-saving',compared:'checked',empty:'unchecked'})[stage]||stage;
   const phases={'checked-edit':'Editing','checked-saving':'Saving',uncertain:'Uncertain',stale:'Conflict',checked:null,unchecked:null,'clear-edit':'Editing',cleared:null};
   const mode=stage==='checked'?'checked':['unchecked','clear-edit'].includes(stage)?'unchecked':'none';
   const activeOrders=numberMode?{none:orders.none,checked:[2,3,1,4,7,8],unchecked:[6]}:orders;
   const ids=rowIds(),expected=activeOrders[mode].map(i=>`R_LIST_${String(i).padStart(3,'0')}`),ordered=JSON.stringify(ids)===JSON.stringify(expected);
   const table=phases[stage]===null,editing=['checked-edit','clear-edit'].includes(stage),clear=stage==='clear-edit';
   const root=q('.slack-list-dialog'),panel=q('#slack-list-rename'),condition=q('#slack-list-filter-condition'),picker=q('#slack-list-view-select');
   const phase=panel?.dataset.renameStage||null;
   const controlIds=table?['slack-list-close','slack-list-filter','slack-list-view-select']:stage==='checked-saving'?['slack-list-rename-leave']:editing?['slack-list-rename-leave','slack-list-rename-close','slack-list-rename-confirm']:stage==='stale'?['slack-list-rename-leave','slack-list-rename-close','slack-list-rename-check']:['slack-list-rename-leave','slack-list-rename-check'];
   const controls=controlIds.map(id=>{const e=document.getElementById(id);return{id,disabled:!!e?.disabled,hit:hit(e),rect:rect(e)};});
   const exactButtons=table||JSON.stringify([...panel?.querySelectorAll('button')||[]].map(e=>e.id))===JSON.stringify(controlIds);
   const op=q('#slack-list-filter-operator'),input=q('#slack-list-filter-number');
   const numeric=numberMode&&!table&&!clear;
   const numberSelectors=!numeric||op?.value==='less_than'&&op.dataset.filterChoice===condition?.dataset.filterChoice&&op.disabled===!editing&&visible(op)&&(!editing||hit(op))&&input?.type==='number'&&input.step==='any'&&input.value==='12.50'&&input.dataset.filterChoice===condition?.dataset.filterChoice&&input.dataset.filterOperator===op.value&&input.dataset.filterColumn==='C_NUMBER'&&input.disabled===!editing&&visible(input)&&(!editing||hit(input));
   const selectors=(table||condition?.value===(clear?'0':numberMode?'3':'1')&&condition.disabled===!editing&&visible(condition)&&(!editing||hit(condition)))&&numberSelectors;
   const focused=table?this.returnedFilter().ok:editing?(document.activeElement===condition||numeric&&[op,input].includes(document.activeElement)):panel?.contains(document.activeElement);
   const first=q('.slack-list-grid tr[data-row-id]'),primary=first?.querySelector('[data-column-id="C_NAME"]'),number=first?.querySelector('[data-column-id="C_NUMBER"]');
   const expectedNumber=(numberMode?{none:'-3.00',checked:'-3.00',unchecked:null}:{none:'-3.00',checked:'0.00',unchecked:'-3.00'})[mode];
   const numberText=number?.querySelector('.slack-list-cell-text')?.textContent||null;
   const rowVisible=!table||visible(primary)&&visible(number)&&numberText===expectedNumber&&(!numberMode||visible(first)&&!!number?.querySelector('.slack-list-cell-text')&&(mode!=='unchecked'||number.querySelector('.slack-list-cell-text').textContent===''));
   const warnings=[...document.querySelectorAll('.slack-list-warning')].filter(e=>e.textContent.includes('Only part'));
   const partial=warnings.length===1&&(!table||visible(warnings[0]));
   const findState=this.findRetained(),findVisible=!table||visible(find());
   const actualLayout=!!q('.slack-list-grid')&&!q('.slack-list-board'),bounded=visible(root)&&(!panel||visible(panel))&&root.scrollWidth<=root.clientWidth+1;
   return{ok:Object.hasOwn(phases,stage)&&phase===phases[stage]&&actualLayout&&bounded&&ordered&&selectors&&focused&&partial&&rowVisible&&findState.ok&&findVisible&&exactButtons&&controls.every(c=>!c.disabled&&c.hit)&&picker?.value==='V_FILTER'&&picker?.selectedOptions?.[0]?.textContent==='Shared checklist',
    stage:captureStage,phase,mode,actualLayout,bounded,ordered,rowIds:ids,owner:{visit:root?.dataset.dialogGeneration,selection:root?.dataset.viewSelection,serial:panel?.dataset.renameSerial||null,view:panel?.dataset.filterView||null},selectors,controls,exactButtons,focused:document.activeElement?.id||null,focusedCorrectly:!!focused,
    ...(numberMode?{numberControls:numeric?{operator:op.value,value:input.value,choice:input.dataset.filterChoice,disabled:input.disabled,operatorRect:rect(op),valueRect:rect(input)}:null}:{}),draft:panel?this.draft():null,find:findState,findVisible,partial,partialText:warnings[0]?.textContent||null,rowVisible,first:{id:first?.dataset.rowId,primary:rect(primary),number:rect(number),numberText},selectedView:picker?.value,selectedLabel:picker?.selectedOptions?.[0]?.textContent||null,returnedFilter:table?this.returnedFilter():null,panel:rect(panel),viewport:[innerWidth,innerHeight]};
  }
 };
 return{ok:true};
})()
