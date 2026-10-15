(() => {
  const root=document.getElementById('slack-list-rename');
  const dialog=root?.closest('.slack-list-dialog');
  if(!root||!dialog)return;
  const serial=root.dataset.renameSerial,visit=dialog.dataset.dialogGeneration;
  if(!root.__renameReturnBound){
    root.__renameReturnBound=true;
    root.addEventListener('click',event=>{
      const close=event.target.closest?.('#slack-list-rename-close');
      if(close&&root.contains(close)&&!close.disabled){
        if(root.dataset.viewCreate)armListViewCreateReturn(root,false);else armListRenameReturn(root);
      }
      const open=event.target.closest?.('#slack-list-new-view-open');
      if(open&&root.contains(open)&&!open.disabled)armListViewCreateReturn(root,true);
    },true);
  }
  root.__renameFocus?.disconnect();
  let queued=false;
  const focus=()=>{
    queued=false;
    if(!root.isConnected||root.dataset.renameSerial!==serial||dialog.dataset.dialogGeneration!==visit){observer.disconnect();return;}
    const dialogs=[...document.querySelectorAll('[role="dialog"]')].filter(e=>e.getBoundingClientRect().width>0&&!e.closest('[hidden],[inert]'));
    if(dialogs.at(-1)!==root)return;
    const active=document.activeElement;
    if(root.contains(active)&&!active.disabled)return;
    (root.querySelector('input:not([disabled])')||root.querySelector('#slack-list-rename-check:not([disabled])')||root.querySelector('#slack-list-new-view-open:not([disabled])')||root.querySelector('#slack-list-rename-close:not([disabled])')||root.querySelector('#slack-list-rename-leave'))?.focus({preventScroll:true});
  };
  const schedule=()=>{if(!queued){queued=true;requestAnimationFrame(focus)}};
  const observer=new MutationObserver(schedule);
  observer.observe(dialog,{subtree:true,childList:true,attributes:true,attributeFilter:['disabled','data-rename-stage']});
  root.__renameFocus=observer;schedule();
})()
