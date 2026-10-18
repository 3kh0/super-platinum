// Capture exact native opener custody before Dioxus mounts the review modal.
(() => {
  const dialog=document.querySelector('.slack-list-dialog');
  if(!dialog||dialog.__viewOrderBound)return;
  dialog.__viewOrderBound=true;
  dialog.addEventListener('click',event=>{
    delete dialog.__viewOrderOpener;
    const button=event.target.closest?.('button[data-reorder-view]');
    if(!button||!dialog.contains(button)||button.disabled)return;
    const direction=button.dataset.reorderDirection;
    const view=button.dataset.reorderView,selection=dialog.dataset.viewSelection;
    if(!['up','down'].includes(direction)||button.dataset.reorderSelection!==selection||dialog.querySelector('#slack-list-view-select')?.value!==view)return;
    dialog.__viewOrderOpener={button,view,direction,selection,visit:dialog.dataset.dialogGeneration};
  },true);
})();
