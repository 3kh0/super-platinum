// Arm synchronously in the native Done/Cancel click, before Dioxus removes the
// panel. A single RAF is insufficient: it can precede the asynchronous unmount.
function armListRenameReturn(root) {
  const dialog=root.closest('.slack-list-dialog');
  if(!dialog||!root.isConnected)return;
  const visit=dialog.dataset.dialogGeneration,serial=root.dataset.renameSerial,column=root.dataset.renameColumn,view=root.dataset.renameView,precision=root.dataset.precisionColumn,visibility=root.dataset.visibilityView,sort=root.dataset.sortView,filter=root.dataset.filterView,grouping=root.dataset.groupingView,textCreate=root.dataset.newTextFieldView;
  const selection=dialog.dataset.viewSelection;
  const deletion=root.dataset.deleteColumn,deleteOpener=deletion?root.__deleteOpener:null;
  if(deletion&&(!deleteOpener||deleteOpener.dataset.deleteFieldSelection!==selection))return;
  if(!visit||!serial||(!column&&!view)||(view&&!selection))return;
  const groupingOpener=grouping?root.__groupingOpener:null;
  if(grouping&&(!groupingOpener||groupingOpener.dataset.groupingView!==view))return;
  const textOpener=textCreate?dialog.querySelector('#slack-list-new-text-field'):null;
  if(textCreate&&(!textOpener||textOpener.dataset.newTextFieldView!==view))return;
  dialog.__renameReturn?.();
  let ended=false,queued=false,frame=0;
  const stop=()=>{
    if(ended)return;
    ended=true;observer.disconnect();clearTimeout(timer);cancelAnimationFrame(frame);
    document.removeEventListener('pointerdown',userInput,true);
    document.removeEventListener('keydown',userInput,true);
    document.removeEventListener('focusin',focusChanged,true);
    if(dialog.__renameReturn===stop)delete dialog.__renameReturn;
  };
  const userInput=event=>{if(event.isTrusted)stop();};
  const interactive=e=>e?.matches?.('button,input,select,textarea,a[href],[contenteditable="true"]');
  const focusChanged=event=>{
    if(!root.contains(event.target)&&interactive(event.target))stop();
  };
  const visible=e=>e.isConnected&&e.getBoundingClientRect().width>0&&!e.closest('[hidden],[inert]');
  const restore=()=>{
    queued=false;
    if(ended)return;
    if(!dialog.isConnected||document.querySelector('.slack-list-dialog')!==dialog||dialog.dataset.dialogGeneration!==visit){stop();return;}
    const panel=document.getElementById('slack-list-rename');
    if(panel){
      if(panel!==root||panel.dataset.renameSerial!==serial)stop();
      return; // Same panel still mounted: wait for the actual removal mutation.
    }
    const dialogs=[...document.querySelectorAll('[role="dialog"]')].filter(visible);
    if(dialogs.at(-1)!==dialog){stop();return;}
    const active=document.activeElement;
    // Generic dialog/grid focus may come from the existing List focus manager.
    // A chosen live control or new native user input always wins instead.
    if(active&&active!==document.body&&active.isConnected&&interactive(active)&&!root.contains(active)){stop();return;}
    let header;
    if(deletion){
      if(dialog.dataset.viewSelection!==selection){stop();return;}
      header=dialog.querySelector('#slack-list-delete-field');
      if(header!==deleteOpener){stop();return;}
    }else if(view){
      const picker=dialog.querySelector('#slack-list-view-select');
      if(!picker||picker.value!==view||dialog.dataset.viewSelection!==selection){stop();return;}
      header=grouping?dialog.querySelector('#slack-list-grouping'):textCreate?dialog.querySelector('#slack-list-new-text-field'):filter?dialog.querySelector('#slack-list-filter'):sort?dialog.querySelector('#slack-list-sort'):visibility?dialog.querySelector('#slack-list-fields'):[...dialog.querySelectorAll('[data-rename-view]')].find(e=>e.dataset.renameView===view);
    }else if(precision)header=[...dialog.querySelectorAll('button[data-precision-column]')].find(e=>e.dataset.precisionColumn===precision);
    else header=[...dialog.querySelectorAll('button[data-rename-column]')].find(e=>e.dataset.renameColumn===column);
    if(grouping&&header!==groupingOpener){stop();return;}
    if(textCreate&&header!==textOpener){stop();return;}
    const target=header&&!header.disabled&&visible(header)?header:dialog.querySelector('#slack-list-close');
    stop();
    if(target&&!target.disabled&&visible(target))target.focus({preventScroll:true});
  };
  const schedule=()=>{if(!ended&&!queued){queued=true;frame=requestAnimationFrame(restore);}};
  const observer=new MutationObserver(schedule);
  const timer=setTimeout(stop,1000); // Bounded; no background polling after close.
  dialog.__renameReturn=stop;
  document.addEventListener('pointerdown',userInput,true);
  document.addEventListener('keydown',userInput,true);
  document.addEventListener('focusin',focusChanged,true);
  observer.observe(document.body,{subtree:true,childList:true,attributes:true,attributeFilter:['disabled','data-dialog-generation','data-rename-serial']});
  schedule();
}
