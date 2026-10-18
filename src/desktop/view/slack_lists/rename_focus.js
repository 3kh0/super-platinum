(() => {
  const root=document.getElementById('slack-list-rename');
  const dialog=root?.closest('.slack-list-dialog');
  if(!root||!dialog)return;
  const serial=root.dataset.renameSerial,visit=dialog.dataset.dialogGeneration;
  if(root.dataset.groupingView&&!root.__groupingOpener){const opener=dialog.querySelector('#slack-list-grouping');if(opener?.dataset.groupingView===root.dataset.groupingView)root.__groupingOpener=opener;}
  if(root.dataset.reorderDirection&&!root.__reorderOpener){
    const proof=dialog.__viewOrderOpener;
    delete dialog.__viewOrderOpener;
    const opener=proof?.button;
    if(opener?.isConnected&&dialog.contains(opener)&&proof.visit===visit&&proof.selection===dialog.dataset.viewSelection&&proof.view===root.dataset.renameView&&proof.direction===root.dataset.reorderDirection&&opener.dataset.reorderSelection===proof.selection&&opener.dataset.reorderView===proof.view&&opener.dataset.reorderDirection===proof.direction)root.__reorderOpener=opener;
  }
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
  if(root.dataset.deleteColumn&&!root.__deleteOpener)root.__deleteOpener=dialog.querySelector('#slack-list-delete-field');
  root.__renameFocus?.disconnect();
  let queued=false;
  const focus=()=>{
    queued=false;
    if(!root.isConnected||root.dataset.renameSerial!==serial||dialog.dataset.dialogGeneration!==visit){observer.disconnect();return;}
    const dialogs=[...document.querySelectorAll('[role="dialog"]')].filter(e=>e.getBoundingClientRect().width>0&&!e.closest('[hidden],[inert]'));
    if(dialogs.at(-1)!==root)return;
    if(root.dataset.sortFocus||root.dataset.filterFocus||root.dataset.filterCapture||root.dataset.entityFocus||root.__ratingEmojiReturn)return;
    const active=document.activeElement;
    if(root.contains(active)&&!active.disabled)return;
    (root.querySelector('#slack-list-delete-field-choice:not([disabled])')||root.querySelector('#slack-list-grouping-field:not([disabled])')||root.querySelector('#slack-list-filter-set [data-filter-field]:not([disabled])')||root.querySelector('#slack-list-filter-add-field:not([disabled])')||root.querySelector('#slack-list-sort-set [data-sort-field]:not([disabled])')||root.querySelector('#slack-list-sort-add-field:not([disabled])')||root.querySelector('#slack-list-field-choice:not([disabled])')||root.querySelector('#slack-list-precision:not([disabled])')||root.querySelector('input:not([disabled])')||root.querySelector('#slack-list-rename-check:not([disabled])')||root.querySelector('#slack-list-new-view-open:not([disabled])')||root.querySelector('#slack-list-rename-close:not([disabled])')||root.querySelector('#slack-list-rename-leave'))?.focus({preventScroll:true});
  };
  const schedule=()=>{if(!queued){queued=true;requestAnimationFrame(focus)}};
  const observer=new MutationObserver(schedule);
  observer.observe(dialog,{subtree:true,childList:true,attributes:true,attributeFilter:['disabled','data-rename-stage','data-sort-focus','data-rating-return']});
  root.__renameFocus=observer;schedule();
})()
