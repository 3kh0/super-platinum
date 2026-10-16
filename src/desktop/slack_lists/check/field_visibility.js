(()=>{
 const q=s=>document.querySelector(s),cellSelector='.slack-list-grid tr[data-row-id="R_LIST_005"] [data-column-id="C_NUMBER"]';
 const rect=e=>{const r=e?.getBoundingClientRect();return r?[r.x,r.y,r.width,r.height]:null;};
 const visible=e=>{
  if(!e||e.closest('[hidden],[inert]'))return false;
  const r=e.getBoundingClientRect();if(r.width<=0||r.height<=0||r.left< -1||r.top< -1||r.right>innerWidth+1||r.bottom>innerHeight+1)return false;
  for(let p=e.parentElement;p;p=p.parentElement){const s=getComputedStyle(p),b=p.getBoundingClientRect();if(s.display==='none'||s.visibility==='hidden')return false;
   if(/auto|scroll|hidden|clip/.test(s.overflowX)&&(r.left<b.left-1||r.right>b.right+1))return false;
   if(/auto|scroll|hidden|clip/.test(s.overflowY)&&(r.top<b.top-1||r.bottom>b.bottom+1))return false;}
  return true;
 };
 const hit=e=>{if(!visible(e))return false;const r=e.getBoundingClientRect();return [.15,.5,.85].every(x=>[.15,.5,.85].every(y=>{const f=document.elementFromPoint(r.left+r.width*x,r.top+r.height*y);return f===e||e.contains(f);}));};
 let fields=null;
 window.__listFieldVisibility={
  click(selector){const e=q(selector);e?.scrollIntoView({block:'nearest',inline:'nearest'});if(!e||e.disabled||!hit(e))return{ok:false,selector};e.focus();e.click();return{ok:true,selector,hitPoints:9};},
  select(selector,value){const e=q(selector);e?.scrollIntoView({block:'nearest',inline:'nearest'});if(!(e instanceof HTMLSelectElement)||e.disabled||!hit(e)||![...e.options].some(o=>o.value===value))return{ok:false,selector};e.focus();Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(e,value);e.dispatchEvent(new Event('change',{bubbles:true}));return{ok:true,selector,value};},
  rememberFields(){fields=q('#slack-list-fields');return{ok:!!fields&&fields.isConnected};},
  returnedFields(){const current=q('#slack-list-fields');return{ok:!!fields&&fields===current&&fields===document.activeElement&&fields.isConnected&&!q('#slack-list-rename'),sameNode:fields===current,focused:fields===document.activeElement};},
  revealNumber(){const e=q(cellSelector);if(!e)return{ok:false};e.scrollIntoView({block:'nearest',inline:'nearest'});return{ok:true};},
  geometry(stage){
   const phases={'hide-edit':'Editing','hide-saving':'Saving',uncertain:'Uncertain',stale:'Uncertain',hidden:null,'show-edit':'Editing',shown:null};
   const actualLayout=!!q('.slack-list-grid')&&!q('.slack-list-board');
   const root=q('.slack-list-dialog'),panel=q('#slack-list-rename'),phase=panel?.dataset.renameStage||null;
   const field=q('#slack-list-field-choice'),choice=q('#slack-list-field-visible'),picker=q('#slack-list-view-select');
   const editing=['hide-edit','show-edit'].includes(stage),shown=stage==='shown',hidden=stage==='hidden',table=hidden||shown;
   const desired=stage==='show-edit'?'shown':'hidden';
   const controlIds=table?['slack-list-close','slack-list-fields','slack-list-view-select']:
     stage==='hide-saving'?['slack-list-rename-leave']:
     editing?['slack-list-rename-leave','slack-list-rename-close','slack-list-rename-confirm']:['slack-list-rename-leave','slack-list-rename-check'];
   const controls=controlIds.map(id=>{const e=document.getElementById(id);return{id,disabled:!!e?.disabled,hit:hit(e),rect:rect(e)};});
   const actualButtons=panel?[...panel.querySelectorAll('button')].map(e=>e.id):[];
   const exactButtons=table||JSON.stringify(actualButtons)===JSON.stringify(controlIds);
   const selectors=!panel||field?.value==='C_NUMBER'&&choice?.value===desired&&field.disabled===!editing&&choice.disabled===!editing&&visible(field)&&visible(choice)&&(!editing||hit(field)&&hit(choice));
   const focused=table?this.returnedFields().ok:editing?document.activeElement===choice:panel?.contains(document.activeElement);
   const number=q(cellSelector),numberText=number?.querySelector('.slack-list-cell-text')?.textContent||null;
   const numberNodes=[...document.querySelectorAll('.slack-list-grid [data-column-id="C_NUMBER"]')].length;
   const numericHeader=q('button[data-precision-column="C_NUMBER"]');
   const visibility=hidden?numberNodes===0&&!numericHeader:shown?numberText==='12.50'&&visible(number)&&!!numericHeader:true;
   const bounded=visible(root)&&(!panel||visible(panel))&&root.scrollWidth<=root.clientWidth+1;
   const partial=[...document.querySelectorAll('.slack-list-warning')].some(e=>e.textContent.includes('Only part'));
   const selectedView=picker?.value||null,selectedLabel=picker?.selectedOptions?.[0]?.textContent||null;
   return{ok:Object.hasOwn(phases,stage)&&phase===phases[stage]&&actualLayout&&bounded&&selectors&&focused&&partial&&visibility&&exactButtons&&controls.every(c=>!c.disabled&&c.hit)&&selectedView==='V_PRECISION'&&selectedLabel==='Estimates',
    stage,phase,actualLayout,owner:{visit:root?.dataset.dialogGeneration,selection:root?.dataset.viewSelection,serial:panel?.dataset.renameSerial||null,view:panel?.dataset.visibilityView||null},bounded,partial,controls,exactButtons,selectors,focused:document.activeElement?.id||null,focusedCorrectly:!!focused,
    field:field?{value:field.value,disabled:field.disabled,choice:field.dataset.visibilityChoice,rect:rect(field)}:null,
    visibility:choice?{value:choice.value,disabled:choice.disabled,rect:rect(choice)}:null,
    current:q('#slack-list-field-visibility-current')?.textContent||null,
    number:{nodes:numberNodes,text:numberText,visible:visible(number),rect:rect(number)},
    selectedView,selectedLabel,returnedFields:table?this.returnedFields():null,panel:rect(panel),viewport:[innerWidth,innerHeight]};
  }
 };
 return{ok:true};
})()
