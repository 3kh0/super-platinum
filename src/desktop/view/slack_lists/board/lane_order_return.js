// Restore only within the same List incarnation. Direct moves have no edit step.
(() => {
  if(window.__listLaneOrderReturn)return;
  const live=e=>e?.isConnected&&e.getBoundingClientRect().width>0&&!e.closest('[hidden],[inert]');
  document.addEventListener('click',event=>{
    const button=event.target.closest?.('button[data-lane-move]');
    const dialog=button?.closest('.slack-list-dialog');
    if(!dialog||button.disabled||!live(button))return;
    dialog.__laneOrderOpener={button,key:button.dataset.laneKey,direction:button.dataset.laneMove,
      visit:dialog.dataset.dialogGeneration,selection:dialog.dataset.viewSelection,
      view:dialog.querySelector('#slack-list-view-select')?.value};
  },true);
  // A later drag cannot inherit a prior button opener.
  document.addEventListener('dragstart',event=>{
    const header=event.target.closest?.('.slack-list-lane-title');
    if(header)delete header.closest('.slack-list-dialog').__laneOrderOpener;
  },true);
  window.__listLaneOrderReturn={arm(root){
    const dialog=root?.closest('.slack-list-dialog');
    if(!dialog||!live(root)||!['left','right'].includes(root.dataset.reorderDirection))return;
    const visit=dialog.dataset.dialogGeneration,selection=dialog.dataset.viewSelection,view=root.dataset.renameView;
    const close=dialog.querySelector('#slack-list-close'),proof=dialog.__laneOrderOpener;
    const owned=()=>document.querySelector('.slack-list-dialog')===dialog&&live(dialog)
      &&dialog.dataset.dialogGeneration===visit&&dialog.dataset.viewSelection===selection
      &&dialog.querySelector('#slack-list-view-select')?.value===view;
    const same=proof&&proof.visit===visit&&proof.selection===selection&&proof.view===view&&proof.direction===root.dataset.reorderDirection;
    const opener=same?proof.button:null;
    if(!owned()||!live(close))return;
    dialog.__laneOrderReturn?.();
    let ended=false,frame=0;
    const stop=()=>{if(ended)return;ended=true;cancelAnimationFrame(frame);observer.disconnect();clearTimeout(timer);
      document.removeEventListener('pointerdown',stop,true);document.removeEventListener('keydown',stop,true);
      if(dialog.__laneOrderReturn===stop)delete dialog.__laneOrderReturn;};
    const restore=()=>{frame=0;if(ended)return;
      if(!owned()){stop();return;}
      const panel=document.getElementById('slack-list-rename');
      if(panel){if(panel!==root)stop();return;}
      if([...document.querySelectorAll('[role="dialog"]')].filter(live).at(-1)!==dialog){stop();return;}
      const active=document.activeElement;
      if(active&&active!==document.body&&active!==dialog&&live(active)&&active.matches('button,input,textarea,select,a,[contenteditable="true"]')){stop();return;}
      // Header drags, retired visits and disabled boundary controls return to the
      // captured List close button. Never substitute a newly rendered opener.
      if(opener&&(!live(opener)||!dialog.contains(opener)||opener.dataset.laneKey!==proof.key||opener.dataset.laneMove!==proof.direction)){stop();return;}
      const target=opener&&!opener.disabled?opener:close;
      if(target===close&&dialog.querySelector('#slack-list-close')!==close){stop();return;}
      if(!live(target)||target.disabled){stop();return;}
      stop();target.focus({preventScroll:true});
    };
    const schedule=()=>{if(!ended&&!frame)frame=requestAnimationFrame(restore);};
    const observer=new MutationObserver(schedule),timer=setTimeout(stop,1000);
    dialog.__laneOrderReturn=stop;
    document.addEventListener('pointerdown',stop,true);document.addEventListener('keydown',stop,true);
    observer.observe(dialog,{subtree:true,childList:true,attributes:true,attributeFilter:['disabled','data-dialog-generation','data-view-selection']});
    schedule();
  }};
})()
