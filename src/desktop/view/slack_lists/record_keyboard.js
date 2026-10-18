function mountListRecordDetail(generation,visit,recordId,returnChild=null){
 const panel=document.getElementById('slack-list-record-detail');
 if(!panel||panel.dataset.detailOpenGeneration!==String(generation))return;
 const dialog=panel.closest('.slack-list-dialog');
 if(!dialog||dialog.dataset.dialogGeneration!==String(visit))return;
 const saved=dialog.__listRecordOrigin;
 const origin=saved?.visit===String(visit)&&saved.record===recordId?saved:undefined;
 delete dialog.__listRecordOrigin;
 let retained,lastFocus,interaction,pagerIntent;
 const fields=()=>[...panel.querySelectorAll('.slack-list-detail-field')];
 const fieldOf=element=>element?.closest?.('.slack-list-detail-field');
 const controls=field=>[...field.querySelectorAll('button,a[href],input,select,textarea')].filter(control=>!control.matches(':disabled,input[type=checkbox]'));
 const clearPager=()=>{if(pagerIntent)clearTimeout(pagerIntent.timeout);pagerIntent=undefined;};
 const move=field=>{if(!field)return;interaction=undefined;retained=field;field.focus({preventScroll:true});field.scrollIntoView({block:'nearest',inline:'nearest'});normalize();};
 function settlePager(){
   const intent=pagerIntent;if(!intent)return false;
   const active=document.activeElement;
   if(!panel.isConnected||dialog.querySelector('#slack-list-cell-editor,#slack-list-reminder-editor')||(active&&active!==document.body&&active!==intent.button&&active!==intent.previous&&active.isConnected)){clearPager();return false;}
   if(intent.old?.isConnected)return false;
   const target=fields()[0];clearPager();
   if(target)move(target);else panel.querySelector('#slack-list-record-back')?.focus({preventScroll:true});
   return true;
 }
 function normalize(){
   if(!panel.isConnected)return;
   if(settlePager())return;
   const all=fields(),active=document.activeElement,focused=fieldOf(active);
   const chosen=all.includes(focused)?focused:all.includes(retained)?retained:all.find(field=>field.dataset.columnId===retained?.dataset.columnId)||all[0];
   if(!all.includes(interaction))interaction=undefined;
   for(const field of all){
     const tab=field===chosen&&!interaction?0:-1;if(field.tabIndex!==tab)field.tabIndex=tab;
     for(const control of field.querySelectorAll('button,a[href],input,select,textarea')){
       const value=interaction===field&&controls(field).includes(control)?0:-1;if(control.tabIndex!==value)control.tabIndex=value;
     }
   }
   retained=chosen;
   const lost=lastFocus&&fieldOf(lastFocus)&&(!lastFocus.isConnected||lastFocus.matches?.(':disabled')||active===document.body);
   if(!pagerIntent&&lost&&!lastFocus?.closest?.('#slack-list-cell-editor')&&(!active||active===document.body||active===lastFocus)){
     chosen?.focus({preventScroll:true});chosen?.scrollIntoView({block:'nearest',inline:'nearest'});
   }
 }
 const focus=event=>{
   if(pagerIntent&&event.target!==pagerIntent.button&&event.target!==pagerIntent.previous)clearPager();
   lastFocus=event.target;
   const field=fieldOf(event.target);interaction=field&&controls(field).includes(event.target)?field:undefined;
   if(field)retained=field;normalize();
 };
 const keydown=event=>{
   const field=fieldOf(event.target);if(!field||!panel.contains(field))return;
   if(event.key==='Escape'&&interaction===field){event.preventDefault();event.stopPropagation();move(field);return;}
   if(interaction===field&&event.target!==field)return;
   if(event.key==='F2'){
     const edit=field.querySelector('.slack-list-edit-cell:not(:disabled)');if(edit){event.preventDefault();event.stopPropagation();edit.click();return;}
   }
   if(event.key==='Enter'){
     const first=controls(field)[0];if(first){event.preventDefault();event.stopPropagation();interaction=field;normalize();first.focus({preventScroll:true});return;}
   }
   if(event.key===' '||event.key==='Enter'){
     const checkbox=field.querySelector('input[type=checkbox]');if(checkbox&&event.target!==checkbox){event.preventDefault();event.stopPropagation();if(!checkbox.disabled)checkbox.click();}return;
   }
   if(!['ArrowUp','ArrowDown','Home','End'].includes(event.key)||event.altKey||event.metaKey)return;
   const all=fields();let index=all.indexOf(field);
   if(event.key==='ArrowUp')index--;if(event.key==='ArrowDown')index++;
   if(event.key==='Home')index=0;if(event.key==='End')index=all.length-1;
   event.preventDefault();event.stopPropagation();move(all[Math.max(0,Math.min(all.length-1,index))]);
 };
 const click=event=>{
   const button=event.target.closest('#slack-list-detail-previous,#slack-list-detail-next');
   if(!button||button.disabled){clearPager();return;}
   clearPager();pagerIntent={button,old:fields()[0],previous:document.activeElement};
   pagerIntent.timeout=setTimeout(clearPager,2000);
   requestAnimationFrame(()=>{if(panel.isConnected)settlePager();});
 };
 dialog.addEventListener('focusin',focus);panel.addEventListener('keydown',keydown);panel.addEventListener('click',click,true);
 const observer=new MutationObserver(()=>{
   if(panel.isConnected){normalize();return;}
   observer.disconnect();clearPager();dialog.removeEventListener('focusin',focus);panel.removeEventListener('keydown',keydown);panel.removeEventListener('click',click,true);
   if(!dialog.isConnected||dialog.dataset.dialogGeneration!==String(visit)||dialog.querySelector('#slack-list-record-detail,#slack-list-cell-editor'))return;
   const active=document.activeElement;
   if(active&&active!==document.body&&!panel.contains(active))return;
   const all=[...dialog.querySelectorAll('.slack-list-grid td[data-column-id],.slack-list-card-field[data-column-id]')].filter(cell=>!cell.closest('[hidden],[inert]'));
   const matches=cell=>cell.closest('[data-row-id]')?.dataset.rowId===recordId;
   const target=origin?.cell?.isConnected&&!origin.cell.closest('[hidden],[inert]')&&matches(origin.cell)?origin.cell:
     all.find(cell=>matches(cell)&&cell.closest('[data-instance-id]')?.dataset.instanceId===origin?.instance&&cell.dataset.columnId===origin?.column)||
     all.find(cell=>matches(cell)&&cell.dataset.columnId===origin?.column)||all.find(matches)||all.find(cell=>cell.tabIndex===0)||dialog.querySelector('#slack-list-close');
   target?.focus({preventScroll:true});target?.scrollIntoView({block:'nearest',inline:'nearest'});
 });
 observer.observe(dialog,{childList:true,subtree:true,attributes:true,attributeFilter:['tabindex','disabled']});
 const previous=document.activeElement;
 let mountInput=false;
 const cancelMount=e=>{if(e.isTrusted)mountInput=true;};
 if(returnChild!==null){document.addEventListener('pointerdown',cancelMount,true);document.addEventListener('keydown',cancelMount,true);}
 requestAnimationFrame(()=>{
   document.removeEventListener('pointerdown',cancelMount,true);document.removeEventListener('keydown',cancelMount,true);
   if(mountInput||!panel.isConnected||document.getElementById('slack-list-record-detail')!==panel||dialog.dataset.dialogGeneration!==String(visit)||dialog.querySelector('#slack-list-cell-editor,#slack-list-reminder-editor'))return;
   const active=document.activeElement;
   if(active!==panel&&panel.contains(active)){normalize();return;}
   if(active&&active!==document.body&&active!==previous)return;
   const child=typeof returnChild==='string'?[...panel.querySelectorAll('[data-subitem-id]')].find(e=>e.dataset.subitemId===returnChild&&!e.disabled):null;
   const target=child||panel.querySelector('#slack-list-record-back');target?.focus({preventScroll:true});if(child)child.scrollIntoView({block:'nearest',inline:'nearest'});normalize();
 });normalize();
}
