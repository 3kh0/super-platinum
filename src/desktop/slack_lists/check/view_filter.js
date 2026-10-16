(()=>{
 const q=s=>document.querySelector(s),rect=e=>{const r=e?.getBoundingClientRect();return r?[r.x,r.y,r.width,r.height]:null;};
 const visible=e=>{
  if(!e||e.closest('[hidden],[inert]'))return false;
  const r=e.getBoundingClientRect();if(r.width<=0||r.height<=0||r.left< -1||r.top< -1||r.right>innerWidth+1||r.bottom>innerHeight+1)return false;
  for(let p=e.parentElement;p;p=p.parentElement){const s=getComputedStyle(p),b=p.getBoundingClientRect();if(s.display==='none'||s.visibility==='hidden')return false;if(/auto|scroll|hidden|clip/.test(s.overflowX)&&(r.left<b.left-1||r.right>b.right+1))return false;if(/auto|scroll|hidden|clip/.test(s.overflowY)&&(r.top<b.top-1||r.bottom>b.bottom+1))return false;}
  return true;
 };
 const hit=e=>{if(!visible(e))return false;const r=e.getBoundingClientRect();return [.15,.5,.85].every(x=>[.15,.5,.85].every(y=>{const f=document.elementFromPoint(r.left+r.width*x,r.top+r.height*y);return f===e||e.contains(f);}));};
 const find=()=>q('.slack-list-find input');let opener=null,findNode=null,findRange=null,mode='checkbox';const history=[];
 const orders={none:[2,3,1,4,7,8,0,5,6],checked:[3,1,7,5],unchecked:[2,4,8,0,6],comparison:[2,3,1,4,7,8],empty:[6],mixed:[3,1,7],single:[2,3,1,4,7,8]};
 const rowIds=()=>[...document.querySelectorAll('.slack-list-grid tr[data-row-id]')].map(e=>e.dataset.rowId);
 const clips=e=>{const out=[];for(let p=e?.parentElement;p;p=p.parentElement){const s=getComputedStyle(p);if(/auto|scroll|hidden|clip/.test(s.overflowX+s.overflowY))out.push({id:p.id||null,class:p.className,rect:rect(p),scroll:[p.scrollLeft,p.scrollTop]});}return out;};
 const api=window.__listViewFilter={
  rect,visible,hit,
  focusRing(e){if(!e)return{ok:false};const s=getComputedStyle(e),r=e.getBoundingClientRect(),width=parseFloat(s.outlineWidth),offset=parseFloat(s.outlineOffset),out=Math.max(0,width+offset);const bounds=[r.left-out,r.top-out,r.right+out,r.bottom+out];let clipped=bounds.some(v=>!Number.isFinite(v))||bounds[0]<0||bounds[1]<0||bounds[2]>innerWidth||bounds[3]>innerHeight;for(let p=e.parentElement;p;p=p.parentElement){const c=getComputedStyle(p),b=p.getBoundingClientRect();if(/auto|scroll|hidden|clip/.test(c.overflowX)&&(bounds[0]<b.left||bounds[2]>b.right))clipped=true;if(/auto|scroll|hidden|clip/.test(c.overflowY)&&(bounds[1]<b.top||bounds[3]>b.bottom))clipped=true;}return{ok:visible(e)&&Number.isFinite(width)&&width>0&&Number.isFinite(offset)&&s.outlineStyle!=='none'&&!clipped,bounds,width,offset,style:s.outlineStyle,clipped};},
  setMode(value){if(!['checkbox','number','compound'].includes(value)||history.length)return{ok:false};mode=value;return{ok:true,mode};},
  numberMode(){return{ok:mode==='number'||mode==='compound'};},
  history(){return{ok:true,schema:2,mode,scrolls:history.slice()};},
  reveal(e,reason){if(!e?.isConnected)return{ok:false,reason};const before={rect:rect(e),clips:clips(e)},active=document.activeElement,focused=active?.id||null;e.scrollIntoView({block:'nearest',inline:'nearest'});const after={rect:rect(e),clips:clips(e)};const record={index:history.length,reason,target:e.id||null,focusedBefore:focused,focusedAfter:document.activeElement?.id||null,sameActiveElement:document.activeElement===active,before,after};history.push(record);return{ok:visible(e)&&record.sameActiveElement,...record};},
  click(selector){const e=q(selector),reveal=this.reveal(e,'activate');if(!reveal.ok||e.disabled||!hit(e))return{ok:false,selector,reveal};e.focus();e.click();return{ok:true,selector,hitPoints:9,reveal};},
  select(selector,value){const e=q(selector),reveal=this.reveal(e,'select');if(!(e instanceof HTMLSelectElement)||!reveal.ok||e.disabled||!hit(e)||![...e.options].some(o=>o.value===value))return{ok:false,selector,reveal};e.focus();Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(e,value);e.dispatchEvent(new Event('change',{bubbles:true}));return{ok:true,selector,value,reveal};},
  query(){const e=find();if(!(e instanceof HTMLInputElement)||e.disabled||!hit(e))return{ok:false};e.focus();Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,'Launch item 0');e.dispatchEvent(new Event('input',{bubbles:true}));return{ok:true};},
  bindFind(){const e=find();if(!e||e.value!=='Launch item 0')return{ok:false};findNode=e;findRange=[e.selectionStart,e.selectionEnd,e.selectionDirection];return{ok:true,value:e.value,range:findRange};},
  findRetained(){const e=find();const range=e?[e.selectionStart,e.selectionEnd,e.selectionDirection]:null;return{ok:!!e&&e===findNode&&e.value==='Launch item 0'&&JSON.stringify(range)===JSON.stringify(findRange),sameNode:e===findNode,value:e?.value,range};},
  rememberFilter(){opener=q('#slack-list-filter');return{ok:!!opener&&opener.isConnected};},
  returnedFilter(){const current=q('#slack-list-filter');return{ok:!!opener&&opener===current&&opener===document.activeElement&&opener.isConnected&&!q('#slack-list-rename'),sameNode:opener===current,focused:opener===document.activeElement};},
  revealFirst(){return this.reveal(q('.slack-list-grid tr[data-row-id]'),'table-first-row');},
  geometry(stage){
   const phases=mode==='compound'?{'mixed-edit':'Editing','mixed-saving':'Saving',uncertain:'Uncertain',conflict:'Conflict',retired:'Uncertain',mixed:null,'single-edit':'Editing',single:null,'clear-edit':'Editing',cleared:null}:mode==='number'?{'compare-edit':'Editing','compare-saving':'Saving',uncertain:'Uncertain',stale:'Conflict',compared:null,empty:null,'clear-edit':'Editing',cleared:null}:{'checked-edit':'Editing','checked-saving':'Saving',uncertain:'Uncertain',stale:'Conflict',checked:null,unchecked:null,'clear-edit':'Editing',cleared:null};
   const table=phases[stage]===null,editing=phases[stage]==='Editing',clear=stage==='clear-edit';
   const projected=({checked:'checked',unchecked:'unchecked',compared:'comparison',empty:'empty',mixed:'mixed',single:'single','single-edit':'mixed'})[stage]||(clear?(mode==='compound'?'single':mode==='number'?'empty':'unchecked'):'none');
   const ids=rowIds(),expected=orders[projected].map(i=>`R_LIST_${String(i).padStart(3,'0')}`),ordered=JSON.stringify(ids)===JSON.stringify(expected);
   const root=q('.slack-list-dialog'),panel=q('#slack-list-rename'),picker=q('#slack-list-view-select'),phase=panel?.dataset.renameStage||null;
   const controlIds=table?['slack-list-close','slack-list-filter','slack-list-view-select']:phases[stage]==='Saving'?['slack-list-rename-leave']:editing?['slack-list-rename-leave','slack-list-rename-close','slack-list-rename-confirm']:phases[stage]==='Conflict'?['slack-list-rename-leave','slack-list-rename-close','slack-list-rename-check']:['slack-list-rename-leave','slack-list-rename-check'];
   const controls=controlIds.map(id=>{const e=document.getElementById(id);return{id,disabled:!!e?.disabled,hit:hit(e),rect:rect(e)};});
   const exactButtons=table||JSON.stringify([...panel?.querySelectorAll('.slack-list-editor-actions button')||[]].map(e=>e.id))===JSON.stringify(controlIds);
   const dom=panel?this.draft():null;
   const rowExpected=clear?[]:mode==='checkbox'?[['C_CHECK','is_not_empty',null]]:mode==='compound'&&stage!=='single-edit'?[['C_CHECK','is_not_empty',null],['C_NUMBER','less_than','12.50']]:[['C_NUMBER','less_than','12.50']];
   const inventory=table||dom.ok&&dom.editable===String(editing)&&dom.rows.length===rowExpected.length&&dom.rows.every((r,i)=>{
    const [column,op,value]=rowExpected[i],number=column==='C_NUMBER';return r.column===column&&r.operator===op&&r.value===value&&r.disabled.every(v=>v===!editing)&&JSON.stringify(r.operators)===JSON.stringify(number?['equal','not_equal','less_than','greater_than','is_empty','is_not_empty']:['is_empty','is_not_empty'])&&(!number||r.input?.type==='number'&&r.input.step==='any'&&r.input.valid&&!r.input.badInput);
   });
   const selected=panel?(!editing?q('#slack-list-rename-message'):clear?q('#slack-list-filter-add-field'):this.rowControl(rowExpected.at(-1)[0],rowExpected.at(-1)[2]===null?'operator':'value')):null;
   const selectors=inventory&&(table||visible(selected)&&(!editing||hit(selected)));
   const focused=table?this.returnedFilter().ok:panel?.contains(document.activeElement)&&this.focusRing(document.activeElement).ok;
   const first=q('.slack-list-grid tr[data-row-id]'),primary=first?.querySelector('[data-column-id="C_NAME"]'),number=first?.querySelector('[data-column-id="C_NUMBER"]');
   const expectedNumber=['checked','mixed'].includes(projected)?'0.00':projected==='empty'?null:'-3.00';
   const numberNode=number?.querySelector('.slack-list-cell-text'),numberText=numberNode?.textContent||null;
   const rowVisible=!table||visible(first)&&visible(primary)&&visible(number)&&numberText===expectedNumber&&!!numberNode&&(projected!=='empty'||numberNode.textContent==='');
   const warnings=[...document.querySelectorAll('.slack-list-warning')].filter(e=>e.textContent.includes('Only part'));
   const partial=warnings.length===1&&(!table||visible(warnings[0])),findState=this.findRetained(),findVisible=!table||visible(find());
   const top=[...document.querySelectorAll('[role="dialog"]')].filter(e=>e.isConnected&&e.getClientRects().length).at(-1);
   const actualLayout=!!q('.slack-list-grid')&&!q('.slack-list-board'),bounded=visible(root)&&(!panel||visible(panel))&&root.scrollWidth<=root.clientWidth+1;
   return{ok:Object.hasOwn(phases,stage)&&phase===phases[stage]&&actualLayout&&bounded&&top===(panel||root)&&ordered&&selectors&&focused&&partial&&rowVisible&&findState.ok&&findVisible&&exactButtons&&controls.every(c=>!c.disabled&&c.hit)&&picker?.value==='V_FILTER'&&picker?.selectedOptions?.[0]?.textContent==='Shared checklist',schema:2,mode,stage,phase,projected,actualLayout,bounded,ordered,rowIds:ids,owner:{visit:root?.dataset.dialogGeneration,selection:root?.dataset.viewSelection,serial:panel?.dataset.renameSerial||null,view:panel?.dataset.filterView||null},selectors,controls,exactButtons,focused:document.activeElement?.id||null,focusedCorrectly:!!focused,draft:dom,find:findState,findVisible,partial,partialText:warnings[0]?.textContent||null,rowVisible,first:{id:first?.dataset.rowId,primary:rect(primary),number:rect(number),numberText},selectedView:picker?.value,selectedLabel:picker?.selectedOptions?.[0]?.textContent||null,returnedFilter:table?this.returnedFilter():null,panel:rect(panel),viewport:[innerWidth,innerHeight],scrollCount:history.length};
  }
 };
 return{ok:true};
})()
