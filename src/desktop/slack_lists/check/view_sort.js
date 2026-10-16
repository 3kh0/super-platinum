(()=>{
 const q=s=>document.querySelector(s),rect=e=>{const r=e?.getBoundingClientRect();return r?[r.x,r.y,r.width,r.height]:null;};
 const visible=e=>{
  if(!e||e.closest('[hidden],[inert]'))return false;
  const r=e.getBoundingClientRect();if(r.width<=0||r.height<=0||r.left< -1||r.top< -1||r.right>innerWidth+1||r.bottom>innerHeight+1)return false;
  for(let p=e.parentElement;p;p=p.parentElement){const s=getComputedStyle(p),b=p.getBoundingClientRect();if(s.display==='none'||s.visibility==='hidden')return false;if(/auto|scroll|hidden|clip/.test(s.overflowX)&&(r.left<b.left-1||r.right>b.right+1))return false;if(/auto|scroll|hidden|clip/.test(s.overflowY)&&(r.top<b.top-1||r.bottom>b.bottom+1))return false;}
  return true;
 };
 const hit=e=>{if(!visible(e))return false;const r=e.getBoundingClientRect();return [.15,.5,.85].every(x=>[.15,.5,.85].every(y=>{const f=document.elementFromPoint(r.left+r.width*x,r.top+r.height*y);return f===e||e.contains(f);}));};
 const find=()=>q('.slack-list-find input');let opener=null,findNode=null,findRange=null;
 const orders={none:[0,1,2,3,4,5,6,7,8],ascending:[2,3,1,4,7,8,0,5,6],descending:[0,5,8,7,1,4,3,2,6]};
 const rowIds=()=>[...document.querySelectorAll('.slack-list-grid tr[data-row-id]')].map(e=>e.dataset.rowId);
 window.__listViewSort={
  click(selector){const e=q(selector);e?.scrollIntoView({block:'nearest',inline:'nearest'});if(!e||e.disabled||!hit(e))return{ok:false,selector};e.focus();e.click();return{ok:true,selector,hitPoints:9};},
  select(selector,value){const e=q(selector);e?.scrollIntoView({block:'nearest',inline:'nearest'});if(!(e instanceof HTMLSelectElement)||e.disabled||!hit(e)||![...e.options].some(o=>o.value===value))return{ok:false,selector};e.focus();Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(e,value);e.dispatchEvent(new Event('change',{bubbles:true}));return{ok:true,selector,value};},
  query(){const e=find();if(!(e instanceof HTMLInputElement)||e.disabled||!hit(e))return{ok:false};e.focus();Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,'Launch item 0');e.dispatchEvent(new Event('input',{bubbles:true}));return{ok:true};},
  bindFind(){const e=find();if(!e||e.value!=='Launch item 0')return{ok:false};findNode=e;findRange=[e.selectionStart,e.selectionEnd,e.selectionDirection];return{ok:true,value:e.value,range:findRange};},
  findRetained(){const e=find();const range=e?[e.selectionStart,e.selectionEnd,e.selectionDirection]:null;return{ok:!!e&&e===findNode&&e.value==='Launch item 0'&&JSON.stringify(range)===JSON.stringify(findRange),sameNode:e===findNode,value:e?.value,range};},
  rememberSort(){opener=q('#slack-list-sort');return{ok:!!opener&&opener.isConnected};},
  returnedSort(){const current=q('#slack-list-sort');return{ok:!!opener&&opener===current&&opener===document.activeElement&&opener.isConnected&&!q('#slack-list-rename'),sameNode:opener===current,focused:opener===document.activeElement};},
  draft(){const panel=q('#slack-list-rename'),field=q('#slack-list-sort-field'),direction=q('#slack-list-sort-direction');return{ok:!!panel&&!!field&&!!direction,serial:panel?.dataset.renameSerial,choice:field?.dataset.sortChoice,field:field?.value,direction:direction?.value,fieldDisabled:field?.disabled,directionDisabled:direction?.disabled,focused:document.activeElement?.id};},
  revealFirst(){const e=q('.slack-list-grid tr[data-row-id]');if(!e)return{ok:false};e.scrollIntoView({block:'nearest',inline:'nearest'});return{ok:true};},
  geometry(stage){
   const phases={'asc-edit':'Editing','asc-saving':'Saving',uncertain:'Uncertain',stale:'Conflict',ascending:null,descending:null,'clear-edit':'Editing',cleared:null};
   const mode=stage==='ascending'?'ascending':['descending','clear-edit'].includes(stage)?'descending':'none';
   const ids=rowIds(),expected=orders[mode].map(i=>`R_LIST_${String(i).padStart(3,'0')}`),ordered=JSON.stringify(ids)===JSON.stringify(expected);
   const table=phases[stage]===null,editing=['asc-edit','clear-edit'].includes(stage),clear=stage==='clear-edit';
   const root=q('.slack-list-dialog'),panel=q('#slack-list-rename'),field=q('#slack-list-sort-field'),direction=q('#slack-list-sort-direction'),picker=q('#slack-list-view-select');
   const phase=panel?.dataset.renameStage||null;
   const controlIds=table?['slack-list-close','slack-list-sort','slack-list-view-select']:stage==='asc-saving'?['slack-list-rename-leave']:editing?['slack-list-rename-leave','slack-list-rename-close','slack-list-rename-confirm']:stage==='stale'?['slack-list-rename-leave','slack-list-rename-close','slack-list-rename-check']:['slack-list-rename-leave','slack-list-rename-check'];
   const controls=controlIds.map(id=>{const e=document.getElementById(id);return{id,disabled:!!e?.disabled,hit:hit(e),rect:rect(e)};});
   const exactButtons=table||JSON.stringify([...panel?.querySelectorAll('button')||[]].map(e=>e.id))===JSON.stringify(controlIds);
   const selectors=table||field?.value===(clear?'':'C_NUMBER')&&direction?.value==='ascending'&&field.disabled===!editing&&direction.disabled===(!editing||clear)&&visible(field)&&visible(direction)&&(!editing||hit(field)&&(!clear?hit(direction):true));
   const focused=table?this.returnedSort().ok:editing?document.activeElement===(clear?field:direction):panel?.contains(document.activeElement);
   const first=q('.slack-list-grid tr[data-row-id]'),primary=first?.querySelector('[data-column-id="C_NAME"]'),number=first?.querySelector('[data-column-id="C_NUMBER"]');
   const expectedNumber={none:'12.50',ascending:'-3.00',descending:'12.50'}[mode];
   const numberText=number?.querySelector('.slack-list-cell-text')?.textContent||null;
   const rowVisible=!table||visible(primary)&&visible(number)&&numberText===expectedNumber;
   const warnings=[...document.querySelectorAll('.slack-list-warning')].filter(e=>e.textContent.includes('Only part'));
   const partial=warnings.length===1&&(!table||visible(warnings[0]));
   const findState=this.findRetained(),findVisible=!table||visible(find());
   const actualLayout=!!q('.slack-list-grid')&&!q('.slack-list-board'),bounded=visible(root)&&(!panel||visible(panel))&&root.scrollWidth<=root.clientWidth+1;
   return{ok:Object.hasOwn(phases,stage)&&phase===phases[stage]&&actualLayout&&bounded&&ordered&&selectors&&focused&&partial&&rowVisible&&findState.ok&&findVisible&&exactButtons&&controls.every(c=>!c.disabled&&c.hit)&&picker?.value==='V_SORT'&&picker?.selectedOptions?.[0]?.textContent==='Shared estimates',
    stage,phase,mode,actualLayout,bounded,ordered,rowIds:ids,owner:{visit:root?.dataset.dialogGeneration,selection:root?.dataset.viewSelection,serial:panel?.dataset.renameSerial||null,view:panel?.dataset.sortView||null},selectors,controls,exactButtons,focused:document.activeElement?.id||null,focusedCorrectly:!!focused,
    draft:panel?this.draft():null,find:findState,findVisible,partial,partialText:warnings[0]?.textContent||null,rowVisible,first:{id:first?.dataset.rowId,primary:rect(primary),number:rect(number),numberText},selectedView:picker?.value,selectedLabel:picker?.selectedOptions?.[0]?.textContent||null,returnedSort:table?this.returnedSort():null,panel:rect(panel),viewport:[innerWidth,innerHeight]};
  }
 };
 return{ok:true};
})()
