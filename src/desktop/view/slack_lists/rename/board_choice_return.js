// Native element custody for Board group controls. No synthetic events or fallback IDs.
(() => {
  if(window.__listBoardChoiceReturn)return;
  const live=e=>e?.isConnected&&e.getBoundingClientRect().width>0&&!e.closest('[hidden],[inert]');
  document.addEventListener('click',event=>{
    const button=event.target.closest?.('button[data-board-choice-kind]');
    const dialog=button?.closest('.slack-list-dialog');
    if(!dialog||button.disabled||!live(button))return;
    dialog.__boardChoiceOpener={button,visit:dialog.dataset.dialogGeneration,selection:dialog.dataset.viewSelection,
      view:button.dataset.boardChoiceView,kind:button.dataset.boardChoiceKind,value:button.dataset.boardChoiceValue};
  },true);
  const matches=(root,dialog,p)=>p&&p.visit===dialog.dataset.dialogGeneration&&p.selection===dialog.dataset.viewSelection
    &&p.view===root.dataset.renameView&&p.kind===root.dataset.boardChoice&&p.value===root.dataset.boardChoiceValue;
  window.__listBoardChoiceReturn={
    mount(root){
      const dialog=root?.closest('.slack-list-dialog'),p=dialog?.__boardChoiceOpener;
      if(!dialog)return;
      delete dialog.__boardChoiceOpener;
      if(matches(root,dialog,p))root.__boardChoiceOpener=p;
      else if(['Preparing','Saving','Checking','Uncertain','Conflict','Observed','NotRequested'].includes(root.dataset.renameStage)
        &&root.dataset.renameSerial&&dialog.dataset.dialogGeneration&&dialog.dataset.viewSelection
        &&document.querySelector('.slack-list-dialog')===dialog
        &&dialog.querySelector('#slack-list-view-select')?.value===root.dataset.renameView){
        const close=dialog.querySelector('#slack-list-close');
        if(live(close))root.__boardChoiceOpener={button:null,close,visit:dialog.dataset.dialogGeneration,
          selection:dialog.dataset.viewSelection,view:root.dataset.renameView,kind:root.dataset.boardChoice,value:root.dataset.boardChoiceValue};
      }
      const input=root.querySelector('input:not([disabled]),select:not([disabled])');
      if(live(input))input.focus({preventScroll:true});
    },
    arm(root){
      const dialog=root.closest('.slack-list-dialog'),p=root.__boardChoiceOpener;
      if(!dialog||!matches(root,dialog,p))return;
      dialog.__boardChoiceReturn?.();
      let ended=false,frame=0;
      const stop=()=>{if(ended)return;ended=true;cancelAnimationFrame(frame);observer.disconnect();clearTimeout(timer);
        document.removeEventListener('pointerdown',stop,true);document.removeEventListener('keydown',stop,true);};
      const restore=()=>{frame=0;if(ended)return;
        if(!live(dialog)||document.querySelector('.slack-list-dialog')!==dialog||!matches(root,dialog,p)){stop();return;}
        const panel=document.getElementById('slack-list-rename');
        if(panel){if(panel!==root)stop();return;}
        const dialogs=[...document.querySelectorAll('[role="dialog"]')].filter(live);
        if(dialogs.at(-1)!==dialog){stop();return;}
        const active=document.activeElement;
        if(active&&active!==document.body&&active!==dialog&&live(active)&&active.matches('button,input,textarea,select,a,[contenteditable="true"]')){stop();return;}
        let target=p.button;
        if(target===null){
          target=dialog.querySelector('#slack-list-close');
          if(target!==p.close||!live(target)||target.disabled){stop();return;}
        }else if(!live(target)||target.disabled||!dialog.contains(target)||target.dataset.boardChoiceKind!==p.kind||target.dataset.boardChoiceValue!==p.value||target.dataset.boardChoiceView!==p.view){stop();return;}
        stop();target.focus({preventScroll:true});
      };
      const schedule=()=>{if(!ended&&!frame)frame=requestAnimationFrame(restore);};
      const observer=new MutationObserver(schedule),timer=setTimeout(stop,1000);
      dialog.__boardChoiceReturn=stop;
      document.addEventListener('pointerdown',stop,true);document.addEventListener('keydown',stop,true);
      observer.observe(dialog,{subtree:true,childList:true,attributes:true,attributeFilter:['disabled','data-dialog-generation','data-view-selection']});
      schedule();
    }
  };
})()
