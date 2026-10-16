(()=>{
 const helper=window.__listTextCreate,t=helper?.checkboxTools();
 if(!t||window.__listCheckboxCreate)return{ok:false};
 const {q,rect,visible,hit,reveal,ids}=t;
 const row=()=>q('.slack-list-grid tr[data-row-id="R_LIST_003"]');
 const cell=()=>row()?.querySelector('[data-column-id="C_CHECKBOX_OBSERVED"]');
 const input=()=>cell()?.querySelector('input[type=checkbox]');
 const heading=()=>q('.slack-list-grid th[data-column-id="C_CHECKBOX_OBSERVED"]');
 const phases=Object.freeze({'check-saving':[false,true],'check-uncertain':[false,true],'check-missing':[false,true],checked:[true,false],'uncheck-saving':[true,true],'uncheck-acknowledged':[true,true],unchecked:[false,false]});
 const actions=[];let started=false,toggleCell,toggleInstance;
 function control(){const e=input();return{checked:e?.checked,disabled:e?.disabled,label:e?.getAttribute('aria-label'),rect:rect(e),visible:visible(e),hit:hit(e)};}
 window.__listCheckboxCreate={
  initial(){const e=input(),scrolling=reveal(cell()),c=control();started=!!(e instanceof HTMLInputElement)&&c.checked===false&&c.disabled===false&&c.label==='Verified, Launch item 04'&&c.visible&&c.hit&&visible(heading())&&helper.findRetained().ok;return{ok:started,control:c,scrolling};},
  toggle(desired){const e=input(),scrolling=reveal(cell()),before=control();if(!started||typeof desired!=='boolean'||!(e instanceof HTMLInputElement)||e.disabled||e.checked!==!desired||!hit(e)||before.label!=='Verified, Launch item 04'||actions.filter(a=>a.kind==='toggle').length!==(desired?0:1))return{ok:false,before,scrolling};e.focus();if(document.activeElement!==e)return{ok:false,before,scrolling};toggleCell=cell();toggleInstance=toggleCell?.parentElement?.dataset.instanceId;if(!toggleCell||e.closest('td')!==toggleCell||!toggleInstance)return{ok:false,before,scrolling};const target={row:toggleCell.parentElement.dataset.rowId,column:toggleCell.dataset.columnId,instance:toggleInstance};e.click();const a={ok:true,kind:'toggle',desired,before,hitPoints:9,focusedBefore:true,scrolling,target};actions.push(a);return a;},
  refresh(){const e=q('#slack-list-refresh'),scrolling=reveal(e);if(!started||!e||e.disabled||!hit(e)||actions.filter(a=>a.kind==='refresh').length>=3)return{ok:false,scrolling};e.focus();if(document.activeElement!==e)return{ok:false,scrolling};e.click();const a={ok:true,kind:'refresh',selector:'#slack-list-refresh',hitPoints:9,scrolling};actions.push(a);return a;},
  blocked(){const c=control();return{ok:started&&c.visible&&c.checked===false&&c.disabled===true&&actions.filter(a=>a.kind==='toggle').length===1&&helper.findRetained().ok,control:c,toggleActions:actions.filter(a=>a.kind==='toggle').length,find:helper.findRetained()};},
  geometry(stage,expectedChecked,expectedDisabled){
   if(!Object.hasOwn(phases,stage)||typeof expectedChecked!=='boolean'||typeof expectedDisabled!=='boolean'||JSON.stringify(phases[stage])!==JSON.stringify([expectedChecked,expectedDisabled]))return{ok:false};
   const root=q('.slack-list-dialog'),primary=row()?.querySelector('[data-column-id="C_NAME"]'),scrolling=reveal(cell()),c=control(),h=heading(),picker=q('#slack-list-view-select'),find=q('.slack-list-find input');
   const warnings=[...document.querySelectorAll('.slack-list-warning')].filter(e=>e.textContent.includes('Only part'));
   const findState=helper.findRetained(),rowIds=ids(),refresh=q('#slack-list-refresh');
   const focus=document.activeElement,roving=[...root.querySelectorAll('.slack-list-grid td[tabindex="0"]')];
   const focused={id:focus?.id||null,checkbox:focus===input(),withinList:!!root?.contains(focus),body:focus===document.body,tag:focus?.tagName||null,role:focus?.getAttribute?.('role')||null,row:focus?.parentElement?.dataset?.rowId||null,column:focus?.dataset?.columnId||null,instance:focus?.parentElement?.dataset?.instanceId||null,tabIndex:focus?.tabIndex??null,sameToggleCell:focus===toggleCell,currentCell:focus===cell(),retainedInstance:toggleInstance||null,rovingCount:roving.length,soleRoving:roving.length===1&&roving[0]===focus};
   const cellFocused=focused.withinList&&!focused.body&&!focused.checkbox&&focus?.isConnected===true&&focused.sameToggleCell&&focused.currentCell&&focused.tag==='TD'&&focused.role==='gridcell'&&focused.row==='R_LIST_003'&&focused.column==='C_CHECKBOX_OBSERVED'&&!!toggleInstance&&focused.instance===toggleInstance&&focused.tabIndex===0&&focused.soleRoving;
   const focusedExpected=['check-missing','checked','unchecked'].includes(stage)?focus===refresh:cellFocused;
   const ready=focusedExpected&&started&&!q('#slack-list-rename')&&!!q('.slack-list-grid')&&!q('.slack-list-board')&&visible(root)&&root.scrollWidth<=root.clientWidth+1&&JSON.stringify(rowIds)===JSON.stringify(['R_LIST_003','R_LIST_001','R_LIST_007','R_LIST_005'])&&visible(primary)&&visible(cell())&&visible(h)&&h.textContent.trim()==='Verified'&&primary?.querySelector('.slack-list-cell-text')?.textContent==='Launch item 04'&&c.visible&&c.label==='Verified, Launch item 04'&&c.checked===expectedChecked&&c.disabled===expectedDisabled&&(expectedDisabled||c.hit)&&visible(find)&&findState.ok&&warnings.length===1&&visible(warnings[0])&&picker?.value==='V_FILTER'&&picker.selectedOptions?.[0]?.textContent==='Shared checklist'&&!!refresh&&!refresh.disabled;
   return{ok:!!ready,stage,owner:{visit:root?.dataset.dialogGeneration,selection:root?.dataset.viewSelection},control:c,first:{id:row()?.dataset.rowId,primary:rect(primary),created:rect(cell()),heading:rect(h)},rowIds,selectedView:picker?.value,selectedLabel:picker?.selectedOptions?.[0]?.textContent,find:findState,findVisible:visible(find),partial: warnings.length===1&&visible(warnings[0]),focused,focusedExpected,scrolling,viewport:[innerWidth,innerHeight],actions:actions.slice()};
  }
 };
 return{ok:true};
})()
