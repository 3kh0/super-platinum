// Restore only the captured native opener, or the same List close control after deletion.
(() => {
  if(window.__listBoardGroupDeleteReturn)return;
  const live=e=>e?.isConnected&&e.getBoundingClientRect().width>0&&!e.closest('[hidden],[inert]');
  document.addEventListener('click',event=>{
    const button=event.target.closest?.('button[data-board-group-delete]');
    const dialog=button?.closest('.slack-list-dialog');
    if(!dialog||button.disabled||!live(button))return;
    dialog.__groupDeleteOpener={button,close:dialog.querySelector('#slack-list-close'),visit:dialog.dataset.dialogGeneration,
      selection:dialog.dataset.viewSelection,view:button.dataset.boardGroupDeleteView,value:button.dataset.boardGroupDelete};
  },true);
  const matches=(root,dialog,p)=>p&&p.visit===dialog.dataset.dialogGeneration&&p.selection===dialog.dataset.viewSelection
    &&p.view===root.dataset.renameView&&p.value===root.dataset.groupDelete;
  window.__listBoardGroupDeleteReturn={
    mount(root){
      const dialog=root?.closest('.slack-list-dialog'),p=dialog?.__groupDeleteOpener;
      if(!dialog)return;
      delete dialog.__groupDeleteOpener;
      if(matches(root,dialog,p))root.__groupDeleteOpener=p;
      else if(['Preparing','Saving','Checking','Uncertain','Conflict','Observed','NotRequested'].includes(root.dataset.renameStage)
        &&root.dataset.renameSerial&&dialog.dataset.dialogGeneration&&dialog.dataset.viewSelection
        &&document.querySelector('.slack-list-dialog')===dialog
        &&dialog.querySelector('#slack-list-view-select')?.value===root.dataset.renameView){
        const close=dialog.querySelector('#slack-list-close');
        if(live(close))root.__groupDeleteOpener={button:null,close,visit:dialog.dataset.dialogGeneration,
          selection:dialog.dataset.viewSelection,view:root.dataset.renameView,value:root.dataset.groupDelete};
      }
      const cancel=root.querySelector('#slack-list-rename-close');
      if(root.dataset.renameStage==='Editing'&&live(cancel))cancel.focus({preventScroll:true});
    },
    arm(root){
      const dialog=root.closest('.slack-list-dialog'),p=root.__groupDeleteOpener;
      if(!dialog||!matches(root,dialog,p))return;
      dialog.__groupDeleteReturn?.();
      let ended=false,frame=0;
      const stop=()=>{if(ended)return;ended=true;cancelAnimationFrame(frame);observer.disconnect();clearTimeout(timer);
        document.removeEventListener('pointerdown',stop,true);document.removeEventListener('keydown',stop,true);};
      const restore=()=>{frame=0;if(ended)return;
        if(!live(dialog)||document.querySelector('.slack-list-dialog')!==dialog||!matches(root,dialog,p)){stop();return;}
        const panel=document.getElementById('slack-list-rename');
        if(panel){if(panel!==root)stop();return;}
        if([...document.querySelectorAll('[role="dialog"]')].filter(live).at(-1)!==dialog){stop();return;}
        const active=document.activeElement;
        if(active&&active!==document.body&&active!==dialog&&live(active)&&active.matches('button,input,textarea,select,a,[contenteditable="true"]')){stop();return;}
        let target=p.button;
        if(!live(target)||target.disabled||!dialog.contains(target)||target.dataset.boardGroupDelete!==p.value||target.dataset.boardGroupDeleteView!==p.view){
          target=dialog.querySelector('#slack-list-close');
          if(target!==p.close){stop();return;}
        }
        stop();if(live(target)&&!target.disabled)target.focus({preventScroll:true});
      };
      const schedule=()=>{if(!ended&&!frame)frame=requestAnimationFrame(restore);};
      const observer=new MutationObserver(schedule),timer=setTimeout(stop,1000);
      dialog.__groupDeleteReturn=stop;
      document.addEventListener('pointerdown',stop,true);document.addEventListener('keydown',stop,true);
      observer.observe(dialog,{subtree:true,childList:true,attributes:true,attributeFilter:['disabled','data-dialog-generation','data-view-selection']});
      schedule();
    }
  };
})()
