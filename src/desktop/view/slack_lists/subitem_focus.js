function mountListSubitems(open,visit){
 const detail=document.getElementById('slack-list-record-detail'),root=document.getElementById('slack-list-subitems');
 const dialog=detail?.closest('.slack-list-dialog');
 if(!detail||!root||!dialog||detail.dataset.detailOpenGeneration!==String(open)||dialog.dataset.dialogGeneration!==String(visit))return;
 const selection=root.dataset.subitemSelection;
 let ticket;
 const stop=()=>{if(!ticket)return;clearTimeout(ticket.timer);ticket.observer.disconnect();document.removeEventListener('pointerdown',input,true);document.removeEventListener('keydown',input,true);document.removeEventListener('focusin',focus,true);ticket=undefined;};
 const input=e=>{if(e.isTrusted)stop();};
 const focus=e=>{if(ticket&&e.target!==ticket.button&&e.target!==ticket.status)stop();};
 const current=()=>root.dataset.subitemSelection===selection&&detail.isConnected&&root.isConnected&&document.getElementById('slack-list-record-detail')===detail&&detail.dataset.detailOpenGeneration===String(open)&&dialog.dataset.dialogGeneration===String(visit)&&!dialog.querySelector('#slack-list-cell-editor');
 const settle=()=>{
  const t=ticket;if(!t)return;if(!current()){stop();return;}
  const active=document.activeElement;
  if(active&&active!==document.body&&active!==t.button&&active!==t.status){stop();return;}
  const status=root.querySelector('#slack-list-subitem-status');
  if(!status)return;
  if(root.dataset.subitemBusy==='true'){
   if(t.button.disabled){t.status=status;status.focus({preventScroll:true});status.scrollIntoView({block:'nearest',inline:'nearest'});}return;
  }
  const next=root.querySelector('#slack-list-subitem-open:not(:disabled),#slack-list-subitem-check:not(:disabled),#slack-list-subitem-dismiss:not(:disabled)');
  if(next){stop();next.focus({preventScroll:true});next.scrollIntoView({block:'nearest',inline:'nearest'});}
 };
 const click=e=>{
  const button=e.target.closest?.('#slack-list-subitem-add,#slack-list-subitem-check');
  if(!button||button.disabled||!current())return;
  stop();const observer=new MutationObserver(settle);ticket={button,status:null,observer,timer:setTimeout(stop,60000)};
  document.addEventListener('pointerdown',input,true);document.addEventListener('keydown',input,true);document.addEventListener('focusin',focus,true);
  observer.observe(dialog,{subtree:true,childList:true,attributes:true,attributeFilter:['disabled','data-subitem-busy','data-detail-open-generation','data-dialog-generation','data-subitem-selection']});
 };
 root.addEventListener('click',click,true);
 const lifecycle=new MutationObserver(()=>{if(root.isConnected)return;stop();root.removeEventListener('click',click,true);lifecycle.disconnect();});
 lifecycle.observe(dialog,{subtree:true,childList:true});
}
