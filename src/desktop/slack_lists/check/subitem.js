(() => {
 const visible=e=>{const r=e?.getBoundingClientRect();return !!r&&r.width>0&&r.height>0&&!e.closest('[hidden],[inert]')&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1};
 const hit=e=>{if(!visible(e))return false;const r=e.getBoundingClientRect();return [.15,.5,.85].every(x=>[.15,.5,.85].every(y=>{const f=document.elementFromPoint(r.left+r.width*x,r.top+r.height*y);return f===e||e.contains(f)}))};
 const rect=e=>{const r=e?.getBoundingClientRect();return r?[r.left,r.top,r.right,r.bottom]:null};
 const inside=(a,b)=>!!a&&!!b&&a[0]>=b[0]-1&&a[1]>=b[1]-1&&a[2]<=b[2]+1&&a[3]<=b[3]+1;
 const focusedRow=()=>{const e=document.activeElement;if(!e?.dataset.subitemId)return null;const label=e.querySelector('.slack-list-subitem-label');
  const clips=['.slack-list-detail-scroll','.slack-list-subitems-rows'].map(selector=>{const c=e.closest(selector),r=c?.getBoundingClientRect();return r?[r.left+c.clientLeft,r.top+c.clientTop,r.left+c.clientLeft+c.clientWidth,r.top+c.clientTop+c.clientHeight]:null});
  const bounds=rect(e),labelBounds=rect(label);return {id:e.dataset.subitemId,rect:bounds,labelRect:labelBounds,clipRects:clips,fullyVisible:visible(e)&&clips.every(c=>inside(bounds,c)),labelVisible:visible(label)&&inside(labelBounds,bounds),hit:hit(e)};
 };
 const rowButton=()=>[...document.querySelectorAll('[data-row-id="R_HYDRATE_PARENT"] .slack-list-record-open')].find(e=>!e.closest('[hidden],[inert]'));
 window.__listSubitem={
  click(selector){const e=selector==='root'?rowButton():document.querySelector(selector);e?.scrollIntoView({block:'nearest',inline:'nearest'});if(!e||e.disabled||!hit(e))return {ok:false,selector,reason:'not hit-testable'};e.focus();e.click();return {ok:true,selector,hitPoints:9}},
  closeList(){const d=document.querySelector('.slack-list-dialog');if(!d)return {ok:false};d.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));return {ok:true}},
  geometry(stage){const root=document.querySelector('.slack-list-dialog'),detail=document.getElementById('slack-list-record-detail'),status=document.getElementById('slack-list-subitem-status');
   const target=status||document.getElementById('slack-list-subitem-add');if(target&&stage!=='child'&&stage!=='parent-return')target.scrollIntoView({block:'nearest',inline:'nearest'});
   const phase=status?.dataset.subitemStage||null,picker=document.getElementById('slack-list-view-select');
   const controls=!detail?[document.getElementById('slack-list-close'),rowButton()]:stage==='child'?[document.getElementById('slack-list-record-back'),detail.querySelector('[data-column-id="C_NAME"] .slack-list-edit-cell')]:[document.getElementById('slack-list-record-back'),...[...(status||document.getElementById('slack-list-subitems')).querySelectorAll('button')].filter(e=>status||e.id==='slack-list-subitem-add')];
   const bounded=visible(root)&&root.scrollWidth<=root.clientWidth+1&&(!detail||visible(detail));
   const partial=[...document.querySelectorAll('.slack-list-warning')].some(e=>e.textContent.includes('Only part'));
   const focusedInside=!detail||detail.contains(document.activeElement);
   const returned=stage==='parent-return'?focusedRow():null;
   const returnVisible=stage!=='parent-return'||(returned?.id==='R_SERVER_SUBITEM'&&returned.fullyVisible&&returned.labelVisible&&returned.hit);
   return {focusedRow:returned,ok:returnVisible&&bounded&&partial&&focusedInside&&controls.every(hit)&&picker?.value==='V_CUSTOM'&&picker?.selectedOptions?.[0]?.textContent==='Project delivery',stage,bounded,partial,focusedInside,
    phase,controls:controls.map(e=>({id:e?.id||(e?.classList.contains('slack-list-record-open')?'root':e?.classList.contains('slack-list-edit-cell')?'edit-name':null),disabled:e?.disabled,hit:hit(e)})),hitPointsPerControl:9,addDisabled:document.getElementById('slack-list-subitem-add')?.disabled??null,
    parent:detail?.dataset.recordId||null,open:detail?.dataset.detailOpenGeneration||null,focused:document.activeElement?.id||document.activeElement?.dataset.columnId||null,
    pickerID:picker?.value,pickerLabel:picker?.selectedOptions?.[0]?.textContent,actualLayout:document.querySelector('.slack-list-board')?'board':'table',
    childIDs:[...document.querySelectorAll('[data-subitem-id]')].map(e=>e.dataset.subitemId),checkDisabled:document.getElementById('slack-list-subitem-check')?.disabled??null,viewport:[innerWidth,innerHeight]};
  }
 };return {ok:true};
})()
