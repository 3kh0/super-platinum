requestAnimationFrame(()=>{
 const dialog=document.querySelector('.slack-list-dialog');if(!dialog||dialog.dataset.listKeyboardMounted)return;
 dialog.dataset.listKeyboardMounted='true';
 let retainedCell,lastFocus,interactionCell,retainedIdentity;
 const cellOf=element=>element?.closest?.('td');
 const controlsOf=cell=>[...cell.querySelectorAll('a[href],button,input,select,textarea')].filter(e=>!e.matches(':disabled,.slack-list-tree-toggle,input[type=checkbox]'));
 const normalize=()=>{
   if(dialog.querySelector('.slack-list-board,#slack-list-record-detail'))return;
   const cells=[...dialog.querySelectorAll('td')],active=document.activeElement;
   const activeCell=cellOf(active);
   const cell=cells.includes(activeCell)?activeCell:cells.includes(retainedCell)?retainedCell:cells.find(c=>retainedIdentity&&c.parentElement.dataset.instanceId===retainedIdentity.row&&c.dataset.columnId===retainedIdentity.column)||cells.find(c=>c.tabIndex===0)||cells[0];
   if(!cells.includes(interactionCell))interactionCell=undefined;
   for(const candidate of cells){
     const value=candidate===cell&&!interactionCell?0:-1;if(candidate.tabIndex!==value)candidate.tabIndex=value;
     const local=interactionCell===candidate;
     if(local&&!candidate.hasAttribute('data-escape-local'))candidate.setAttribute('data-escape-local','true');
     if(!local&&candidate.hasAttribute('data-escape-local'))candidate.removeAttribute('data-escape-local');
     for(const control of candidate.querySelectorAll('a[href],button,input,select,textarea')){
       const tab=local&&controlsOf(candidate).includes(control)?0:-1;
       if(control.tabIndex!==tab)control.tabIndex=tab;
     }
   }
   // Moving a keyed DOM node can blur it even though it is connected again by
   // the observer callback. Recover only if focus was not moved elsewhere.
   const lostGridFocus=lastFocus&&cellOf(lastFocus)&&(!lastFocus.isConnected||lastFocus.matches?.(':disabled')||active===document.body);
   const lostButtonFocus=lastFocus?.matches?.('button:disabled');
   const focusUnclaimed=!active||active===document.body||active===lastFocus;
   retainedCell=cell;
   if(cell)retainedIdentity={row:cell.parentElement.dataset.instanceId,column:cell.dataset.columnId};
   // The cell editor owns its focus while saving and restores canonical cell
   // identity on close. A disabled Save button must not trigger grid recovery.
   const editorOwnedFocus=lastFocus?.closest?.('#slack-list-cell-editor');
   if(!editorOwnedFocus&&focusUnclaimed&&(lostGridFocus||lostButtonFocus)){
     const target=cell||dialog.querySelector('#slack-list-close');
     target?.focus({preventScroll:true});
     if(cell)cell.scrollIntoView({block:'nearest',inline:'nearest'});
   }
 };
 // Dioxus only patches changed attributes. Reconcile its initial tabindex
 // values with the current native focus after keyed rows move or disappear.
 const observer=new MutationObserver(normalize);
 observer.observe(dialog,{childList:true,subtree:true,attributes:true,attributeFilter:['tabindex','disabled']});
 const removal=new MutationObserver(()=>{if(!dialog.isConnected){observer.disconnect();removal.disconnect();}});
 removal.observe(dialog.parentNode,{childList:true});
 const refresh=dialog.querySelector('#slack-list-refresh');
 (refresh?.disabled?dialog.querySelector('#slack-list-close'):refresh)?.focus({preventScroll:true});
 dialog.addEventListener('focusin',event=>{
   lastFocus=event.target;
   const cell=cellOf(event.target);
   interactionCell=cell&&controlsOf(cell).includes(event.target)?cell:undefined;
   if(cell)retainedCell=cell;
   normalize();
 });
 // Capture the opener before hiding the layout can blur it. Scope it to this
 // file visit; a late detail mount must never consume another dialog's target.
 dialog.addEventListener('click',event=>{
   const open=event.target.closest('.slack-list-record-open');if(!open)return;
   const row=open.closest('[data-instance-id]');if(!row)return;
   const active=document.activeElement?.closest?.('td,.slack-list-card-field');
   const cell=active&&row.contains(active)?active:row.querySelector('td,.slack-list-card-field');
   dialog.__listRecordOrigin={visit:dialog.dataset.dialogGeneration,record:row.dataset.rowId,instance:row.dataset.instanceId,column:cell?.dataset.columnId,cell};
 },true);
 dialog.addEventListener('click',event=>{
   const group=event.target.closest('.slack-list-group-toggle');
   if(group){group.focus({preventScroll:true});return;}
   const toggle=event.target.closest('.slack-list-tree-toggle');
   if(toggle)toggle.closest('td').focus({preventScroll:true});
 });
 normalize();
 dialog.addEventListener('keydown',event=>{
   const cell=event.target.closest('td');if(!cell)return;
   if(event.key==='Escape'&&interactionCell===cell){
     event.preventDefault();event.stopPropagation();interactionCell=undefined;cell.focus({preventScroll:true});normalize();return;
   }
   if(interactionCell===cell&&event.target!==cell&&!event.target.matches('.slack-list-group-toggle'))return;
   const toggle=cell.querySelector('.slack-list-tree-toggle,.slack-list-group-toggle');
   if(event.key==='Enter'&&event.altKey){
     const open=cell.closest('[data-row-id]')?.querySelector('.slack-list-record-open');
     if(open){event.preventDefault();event.stopPropagation();open.click();}return;
   }
   if(event.key==='F2'){
     const edit=cell.querySelector('.slack-list-edit-cell:not(:disabled)');
     if(edit){event.preventDefault();event.stopPropagation();edit.click();return;}
   }
   if(event.key==='Enter'&&event.target.matches('.slack-list-group-toggle')){event.preventDefault();event.stopPropagation();event.target.click();return;}
   if(event.key==='Enter'||event.key==='F2'){
     const first=controlsOf(cell)[0];
     if(first){event.preventDefault();event.stopPropagation();interactionCell=cell;normalize();first.focus({preventScroll:true});return;}
     if(toggle&&event.key==='Enter'){event.preventDefault();event.stopPropagation();toggle.click();return;}
   }
   if(event.key===' '||event.key==='Enter'){
     const checkbox=cell.querySelector('input[type=checkbox]');
     if(checkbox&&event.target!==checkbox){event.preventDefault();if(!checkbox.disabled)checkbox.click();}
     return;
   }
   if(!['ArrowLeft','ArrowRight','ArrowUp','ArrowDown','Home','End'].includes(event.key))return;
   if(event.altKey||event.metaKey)return;
   const rows=[...cell.closest('tbody').rows],r=rows.indexOf(cell.parentElement),c=cell.cellIndex;
   const row=rows[r],primary=cell.dataset.treePrimary==='true'||row.hasAttribute('data-group-key');
   const move=target=>{if(!target)return;event.preventDefault();event.stopPropagation();target.focus({preventScroll:true});target.scrollIntoView({block:'nearest',inline:'nearest'});};
   if(primary&&event.key==='ArrowRight'&&toggle){
     if(toggle.getAttribute('aria-expanded')==='false'){event.preventDefault();event.stopPropagation();toggle.click();return;}
     const child=rows[r+1];if(row.hasAttribute('data-group-key')&&child?.dataset.rowId){move(child.cells[0]);return;}if(child?.dataset.parentId===row.dataset.rowId){move(child.querySelector('[data-tree-primary=true]'));return;}
   }
   if(primary&&event.key==='ArrowLeft'){
     if(toggle?.getAttribute('aria-expanded')==='true'){event.preventDefault();event.stopPropagation();toggle.click();return;}
     const parent=Number(row.dataset.depth)>0?rows.find(candidate=>candidate.dataset.rowId===row.dataset.parentId):undefined;
     if(parent){move(parent.querySelector('[data-tree-primary=true]'));return;}
   }
   let nr=r,nc=c;
   if(event.key==='ArrowLeft')nc--;
   if(event.key==='ArrowRight')nc++;
   if(event.key==='ArrowUp')nr--;
   if(event.key==='ArrowDown')nr++;
   if(event.key==='Home'){nc=0;if(event.ctrlKey)nr=0;}
   if(event.key==='End'){nc=rows[r].cells.length-1;if(event.ctrlKey)nr=rows.length-1;}
   const targetRow=rows[Math.max(0,Math.min(rows.length-1,nr))];
   move(targetRow.cells[Math.max(0,Math.min(targetRow.cells.length-1,nc))]);
 });
});