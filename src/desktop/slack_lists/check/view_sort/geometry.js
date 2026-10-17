(()=>{
 const api=window.__listViewSort,q=s=>document.querySelector(s),equal=(a,b)=>JSON.stringify(a)===JSON.stringify(b);
 const fields=['C_NAME','C_CHECK','C_NUMBER','C_SELECT','C_USER','C_CHANNEL','C_MULTI'];
 const orders={none:[0,1,2,3,4,5,6,7,8],ascending:[2,3,1,4,7,8,0,5,6],descending:[0,5,8,7,1,4,3,2,6],multi:[0,8,4,2,6,5,7,1,3],reordered:[0,5,8,7,4,1,3,2,6],single:[0,5,8,7,1,4,3,2,6]};
 const legacy={'asc-edit':['Editing','none'], 'asc-saving':['Saving','none'],uncertain:['Uncertain','none'],stale:['Conflict','none'],ascending:[null,'ascending'],descending:[null,'descending'],'clear-edit':['Editing','descending'],cleared:[null,'none']};
 const ordered={'structure-added':['Editing','none'],'structure-removed':['Editing','none'],'multi-edit':['Editing','none'],'multi-saving':['Saving','none'],uncertain:['Uncertain','none'],stale:['Conflict','none'],retired:['Uncertain','none'],multi:[null,'multi'],'reorder-edit':['Editing','multi'],reordered:[null,'reordered'],'remove-edit':['Editing','reordered'],single:[null,'single'],'clear-edit':['Editing','single'],cleared:[null,'none']};
 const desired=(stage,mode)=>{
  if(stage==='clear-edit')return[];
  if(mode==='legacy')return[['C_NUMBER',true]];
  if(stage==='structure-added')return[['C_NUMBER',true],['C_CHECK',true],['C_NAME',true]];
  if(stage==='structure-removed')return[['C_NUMBER',true],['C_NAME',true]];
  if(stage==='reorder-edit')return[['C_NUMBER',false],['C_CHECK',true]];
  if(stage==='remove-edit')return[['C_NUMBER',false]];
  return[['C_CHECK',true],['C_NUMBER',false]];
 };
 api.geometry=stage=>{
  const mode=api.mode(),spec=(mode==='ordered'?ordered:legacy)[stage];if(!spec)return{ok:false,stage};
  const [expectedPhase,projection]=spec,table=expectedPhase===null,editing=expectedPhase==='Editing';
  const root=q('.slack-list-dialog'),panel=q('#slack-list-rename'),picker=q('#slack-list-view-select'),phase=panel?.dataset.renameStage||null;
  const dom=panel?api.draft():null,expected=desired(stage,mode),used=expected.map(r=>r[0]);
  const inventory=table||dom.ok&&dom.rows.length===expected.length&&new Set(dom.rows.map(r=>r.id)).size===dom.rows.length&&dom.rows.every((row,i)=>{
   const [column,ascending]=expected[i],available=fields.filter(f=>f===column||!used.includes(f));
   return row.column===column&&row.ascending===ascending&&row.direction===(ascending?'ascending':'descending')&&equal(row.fieldOptions,available.map(value=>({value,disabled:false})))&&equal(row.directions,['ascending','descending'].map(value=>({value,disabled:false})))&&equal(row.disabled,{field:!editing,direction:!editing,up:!editing||i===0,down:!editing||i===expected.length-1,remove:!editing});
  })&&equal(dom.add.options,['',...fields.filter(f=>!used.includes(f))].map(value=>({value,disabled:false})))&&dom.add.pickerDisabled===!editing&&dom.add.disabled===true&&dom.clearDisabled===(!editing||expected.length===0);
  const controlIds=table?['slack-list-close','slack-list-sort','slack-list-view-select']:expectedPhase==='Saving'?['slack-list-rename-leave']:editing?['slack-list-rename-leave','slack-list-rename-close','slack-list-rename-confirm']:expectedPhase==='Conflict'?['slack-list-rename-leave','slack-list-rename-close','slack-list-rename-check']:['slack-list-rename-leave','slack-list-rename-check'];
  const controls=controlIds.map(id=>{const e=document.getElementById(id);return{id,disabled:!!e?.disabled,hit:api.hit(e),rect:api.rect(e)};});
  const exactButtons=table||equal([...panel?.querySelectorAll('.slack-list-editor-actions button')||[]].map(e=>e.id),controlIds);
  const focusProof=table?api.returnedSort():editing?api.editingFocus():api.phaseFocus(stage),focused=focusProof.ok;
  const status=q('#slack-list-rename-message'),selectors=inventory&&(table||editing?true:api.visible(status));
  const rows=[...document.querySelectorAll('.slack-list-grid tr[data-row-id]')],ids=rows.map(e=>e.dataset.rowId),expectedIds=orders[projection].map(i=>`R_LIST_${String(i).padStart(3,'0')}`);
  const orderedRows=equal(ids,expectedIds),first=rows[0],primary=first?.querySelector('[data-column-id="C_NAME"]'),number=first?.querySelector('[data-column-id="C_NUMBER"]'),numberText=number?.querySelector('.slack-list-cell-text')?.textContent??null;
  const rowVisible=!table||api.visible(first)&&api.visible(primary)&&api.visible(number)&&numberText===(projection==='ascending'?'-3.00':'12.50');
  const warnings=[...document.querySelectorAll('.slack-list-warning')].filter(e=>e.textContent.includes('Only part')),partial=warnings.length===1&&(!table||api.visible(warnings[0]));
  const find=api.findRetained(),findVisible=!table||api.visible(q('.slack-list-find input'));
  const top=[...document.querySelectorAll('[role="dialog"]')].filter(e=>e.isConnected&&e.getClientRects().length).at(-1);
  const actualLayout=!!q('.slack-list-grid')&&!q('.slack-list-board'),bounded=api.visible(root)&&(!panel||api.visible(panel))&&root.scrollWidth<=root.clientWidth+1;
  return{ok:phase===expectedPhase&&actualLayout&&bounded&&top===(panel||root)&&orderedRows&&selectors&&focused&&partial&&rowVisible&&find.ok&&findVisible&&exactButtons&&controls.every(c=>!c.disabled&&c.hit)&&picker?.value==='V_SORT'&&picker?.selectedOptions?.[0]?.textContent==='Shared estimates',schema:2,mode,stage,phase,projection,actualLayout,bounded,ordered:orderedRows,rowIds:ids,owner:{visit:root?.dataset.dialogGeneration,selection:root?.dataset.viewSelection,serial:panel?.dataset.renameSerial||null,view:panel?.dataset.sortView||null},selectors,controls,exactButtons,focused:document.activeElement?.id||null,focusedCorrectly:!!focused,focusProof,ring:panel?api.focusRing(document.activeElement):null,draft:dom,find,findVisible,partial,partialText:warnings[0]?.textContent||null,rowVisible,first:{id:first?.dataset.rowId,primary:api.rect(primary),number:api.rect(number),numberText},selectedView:picker?.value,selectedLabel:picker?.selectedOptions?.[0]?.textContent||null,returnedSort:table?api.returnedSort():null,panel:api.rect(panel),viewport:[innerWidth,innerHeight],scrollCount:api.history().scrolls.length};
 };
 return{ok:true};
})()
