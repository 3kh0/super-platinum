function mountListDelete(visit,generation){
 const panel=document.getElementById('slack-list-delete-confirm'),dialog=panel?.closest('.slack-list-dialog');
 if(!panel||panel.dataset.deleteGeneration!==String(generation)||dialog?.dataset.dialogGeneration!==String(visit))return;
 const previous=document.activeElement;
 // WebKit can blur the opener as soon as the confirmation disables it.
 // Keep the actual initiating control, not BODY or a previously focused field.
 const origin=dialog.querySelector('#slack-list-record-delete');
 const observer=new MutationObserver(()=>{
  if(panel.isConnected)return;
  observer.disconnect();
  if(!dialog.isConnected||dialog.dataset.dialogGeneration!==String(visit)||dialog.querySelector('#slack-list-delete-confirm,#slack-list-cell-editor'))return;
  const active=document.activeElement;
  if(active&&active!==document.body&&!panel.contains(active))return;
  const target=origin?.isConnected&&!origin.matches(':disabled')?origin:
   dialog.querySelector('#slack-list-record-delete:not(:disabled)')||dialog.querySelector('#slack-list-record-back')||
   dialog.querySelector('td[tabindex="0"],.slack-list-card-field[tabindex="0"]')||dialog.querySelector('#slack-list-close');
  target?.focus({preventScroll:true});target?.scrollIntoView({block:'nearest',inline:'nearest'});
 });
 observer.observe(dialog,{childList:true,subtree:true});
 requestAnimationFrame(()=>{
  if(!panel.isConnected||dialog.dataset.dialogGeneration!==String(visit))return;
  const active=document.activeElement;
  if(active&&active!==document.body&&active!==previous&&!panel.contains(active))return;
  panel.querySelector('#slack-list-delete-cancel')?.focus({preventScroll:true});
 });
}

function mountListDeleteStatus(visit,serial){
 const status=document.getElementById('slack-list-delete-status'),dialog=status?.closest('.slack-list-dialog');
 if(!status||status.dataset.deleteSerial!==String(serial)||dialog?.dataset.dialogGeneration!==String(visit))return;
 let ownedFocus;
 const focus=event=>{ownedFocus=status.contains(event.target)?event.target:undefined;};
 dialog.addEventListener('focusin',focus);
 const observer=new MutationObserver(()=>{
  const removed=!status.isConnected;
  if(removed){observer.disconnect();dialog.removeEventListener('focusin',focus);}
  if(!ownedFocus||ownedFocus.isConnected||!dialog.isConnected||dialog.dataset.dialogGeneration!==String(visit))return;
  const active=document.activeElement;
  if(active&&active!==document.body&&active!==ownedFocus)return;
  if(dialog.querySelector('#slack-list-delete-confirm,#slack-list-cell-editor'))return;
  const target=dialog.querySelector('#slack-list-record-back')||
   dialog.querySelector('td[tabindex="0"],.slack-list-card-field[tabindex="0"]')||dialog.querySelector('#slack-list-close');
  ownedFocus=undefined;
  target?.focus({preventScroll:true});target?.scrollIntoView({block:'nearest',inline:'nearest'});
 });
 observer.observe(dialog,{childList:true,subtree:true});
}
