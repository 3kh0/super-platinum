// Dismiss the exact post-ACK receipt before returning to its selected-view picker.
(() => {
  const root=document.getElementById('slack-list-refinement-save-status');
  const dialog=root?.closest('.slack-list-dialog');
  if(!root||!dialog||root.__refinementStatusReturnBound)return;
  root.__refinementStatusReturnBound=true;
  const serial=root.dataset.refinementSerial;
  if(!serial)return;
  const visible=e=>e?.isConnected&&e.getBoundingClientRect().width>0&&e.getBoundingClientRect().height>0&&!e.closest('[hidden],[inert]');
  const interactive=e=>e?.matches?.('button,input,select,textarea,a[href],[contenteditable="true"]');
  root.addEventListener('click',e=>{
    const button=e.target.closest?.('#slack-list-refinement-status-close');
    if(!button||!root.contains(button)||button.disabled||!visible(button)||document.getElementById(root.id)!==root||root.dataset.refinementSerial!==serial)return;
    const visit=dialog.dataset.dialogGeneration,selection=dialog.dataset.viewSelection;
    const picker=dialog.querySelector('#slack-list-view-select'),value=picker?.value;
    if(!visit||!selection||!picker||!value||!visible(picker)||picker.disabled)return;
    dialog.__refinementStatusReturn?.stop();
    let ended=false,frame=0;
    const stop=()=>{
      if(ended)return;ended=true;observer.disconnect();clearTimeout(timer);cancelAnimationFrame(frame);
      document.removeEventListener('pointerdown',input,true);document.removeEventListener('keydown',input,true);document.removeEventListener('focusin',focused,true);
      if(dialog.__refinementStatusReturn===ticket)delete dialog.__refinementStatusReturn;
    };
    const input=e=>{if(e.isTrusted)stop();};
    const focused=e=>{if(!root.contains(e.target)&&interactive(e.target))stop();};
    const restore=()=>{
      frame=0;if(ended)return;
      if(!dialog.isConnected||document.querySelector('.slack-list-dialog')!==dialog||dialog.dataset.dialogGeneration!==visit||dialog.dataset.viewSelection!==selection||root.dataset.refinementSerial!==serial){stop();return;}
      const status=document.getElementById(root.id);
      if(status){if(status!==root)stop();return;}
      if(root.isConnected||document.getElementById('slack-list-rename')||[...document.querySelectorAll('[role="dialog"]')].filter(visible).at(-1)!==dialog){stop();return;}
      if(dialog.querySelector('#slack-list-view-select')!==picker||picker.value!==value||picker.disabled||!visible(picker)){stop();return;}
      const active=document.activeElement;
      if(active&&active!==document.body&&active.isConnected&&interactive(active)&&!root.contains(active)){stop();return;}
      stop();picker.focus({preventScroll:true});
    };
    const schedule=()=>{if(!ended&&!frame)frame=requestAnimationFrame(restore);};
    const observer=new MutationObserver(records=>{
      // Any ownership mutation retires the ticket, including changes restored
      // within the same delivery batch (ABA).
      if(records.some(r=>r.type==='attributes'&&((r.target===dialog&&['data-dialog-generation','data-view-selection'].includes(r.attributeName))||(r.target===root&&r.attributeName==='data-refinement-serial')))){stop();return;}
      schedule();
    });
    const timer=setTimeout(stop,15000),ticket={stop};
    dialog.__refinementStatusReturn=ticket;
    document.addEventListener('pointerdown',input,true);document.addEventListener('keydown',input,true);document.addEventListener('focusin',focused,true);
    observer.observe(document.body,{subtree:true,childList:true,attributes:true,attributeFilter:['disabled','hidden','inert','data-dialog-generation','data-view-selection','data-refinement-serial']});
    schedule();
  },true);
})();
