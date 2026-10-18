// A post-ACK Check can remove its focused button without dismissing the receipt.
(() => {
  const root=document.getElementById('slack-list-refinement-save-status');
  const dialog=root?.closest('.slack-list-dialog');
  if(!root||!dialog||root.__refinementStatusCheckBound)return;
  root.__refinementStatusCheckBound=true;
  const serial=root.dataset.refinementSerial;
  if(!serial)return;
  const visible=e=>e?.isConnected&&e.getBoundingClientRect().width>0&&e.getBoundingClientRect().height>0&&!e.closest('[hidden],[inert]');
  const interactive=e=>e?.matches?.('button,input,select,textarea,a[href],[contenteditable="true"]');
  root.addEventListener('click',event=>{
    const button=event.target.closest?.('#slack-list-refinement-status-check');
    if(!button||!root.contains(button)||button.disabled||!visible(button)||!visible(root)||!visible(dialog)||document.activeElement!==button||document.getElementById(root.id)!==root||root.dataset.refinementSerial!==serial)return;
    const visit=dialog.dataset.dialogGeneration,selection=dialog.dataset.viewSelection;
    const picker=dialog.querySelector('#slack-list-view-select'),value=picker?.value;
    if(!visit||!selection||!picker||!value||!visible(picker)||picker.disabled)return;
    dialog.__refinementStatusCheck?.stop();
    let ended=false,frame=0;
    const stop=()=>{
      if(ended)return;ended=true;observer.disconnect();clearTimeout(timer);cancelAnimationFrame(frame);
      document.removeEventListener('pointerdown',input,true);document.removeEventListener('keydown',input,true);document.removeEventListener('focusin',focused,true);
      if(dialog.__refinementStatusCheck===ticket)delete dialog.__refinementStatusCheck;
    };
    const input=e=>{if(e.isTrusted)stop();};
    const focused=e=>{if(e.target!==button&&interactive(e.target))stop();};
    const restore=()=>{
      frame=0;if(ended)return;
      if(!visible(dialog)||!visible(root)||document.querySelector('.slack-list-dialog')!==dialog||document.getElementById(root.id)!==root||dialog.dataset.dialogGeneration!==visit||dialog.dataset.viewSelection!==selection||root.dataset.refinementSerial!==serial){stop();return;}
      // An unchanged button means the action has not rendered. A fast completed
      // Check is also allowed once that exact old button has been removed.
      if(button.isConnected&&!button.disabled)return;
      if(document.getElementById('slack-list-rename')||[...document.querySelectorAll('[role="dialog"]')].filter(visible).at(-1)!==dialog){stop();return;}
      if(dialog.querySelector('#slack-list-view-select')!==picker||picker.value!==value||picker.disabled||!visible(picker)){stop();return;}
      const active=document.activeElement;
      if(active&&active!==document.body&&active!==button&&active.isConnected&&interactive(active)){stop();return;}
      stop();picker.focus({preventScroll:true});
    };
    const schedule=()=>{if(!ended&&!frame)frame=requestAnimationFrame(restore);};
    const observer=new MutationObserver(records=>{
      if(records.some(r=>r.type==='attributes'&&((r.target===dialog&&['data-dialog-generation','data-view-selection'].includes(r.attributeName))||(r.target===root&&r.attributeName==='data-refinement-serial')))){stop();return;}
      schedule();
    });
    const timer=setTimeout(stop,15000),ticket={stop};dialog.__refinementStatusCheck=ticket;
    document.addEventListener('pointerdown',input,true);document.addEventListener('keydown',input,true);document.addEventListener('focusin',focused,true);
    observer.observe(document.body,{subtree:true,childList:true,attributes:true,attributeFilter:['disabled','hidden','inert','data-dialog-generation','data-view-selection','data-refinement-serial']});
    schedule();
  },true);
})();
