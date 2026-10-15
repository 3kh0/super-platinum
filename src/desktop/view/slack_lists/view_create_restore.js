// Native click owns this one-use ticket before Dioxus changes state. Open needs
// the actual synchronous reducer's post-transition clock, not a guessed increment.
function armListViewCreateReturn(root,opening) {
  const dialog=root.closest('.slack-list-dialog');
  if(!dialog||!root.isConnected||!root.dataset.viewCreate)return;
  const visit=dialog.dataset.dialogGeneration,serial=root.dataset.renameSerial;
  const selection=dialog.dataset.viewSelection,picker=dialog.querySelector('#slack-list-view-select');
  if(!visit||!serial||!selection||!picker)return;
  const originalID=picker.value;
  window.__listViewCreateReturn?.stop();
  let ended=false,queued=false,frame=0,expected=null,selectionChanges=[];
  const stop=()=>{if(ended)return;ended=true;observer.disconnect();clearTimeout(timer);cancelAnimationFrame(frame);
    document.removeEventListener('pointerdown',input,true);document.removeEventListener('keydown',input,true);document.removeEventListener('focusin',focus,true);
    if(window.__listViewCreateReturn===ticket)delete window.__listViewCreateReturn;
  };
  const interactive=e=>e?.matches?.('button,input,select,textarea,a[href],[contenteditable="true"]');
  const input=e=>{if(e.isTrusted)stop();};
  const focus=e=>{if(!root.contains(e.target)&&interactive(e.target))stop();};
  const visible=e=>e?.isConnected&&e.getBoundingClientRect().width>0&&!e.closest('[hidden],[inert]');
  const restore=()=>{
    queued=false;if(ended)return;
    if(!dialog.isConnected||document.querySelector('.slack-list-dialog')!==dialog||dialog.dataset.dialogGeneration!==visit){stop();return;}
    const panel=document.getElementById('slack-list-rename');
    if(panel){if(panel!==root||panel.dataset.renameSerial!==serial)stop();return;}
    if(opening&&!expected)return;
    const dialogs=[...document.querySelectorAll('[role="dialog"]')].filter(visible);
    if(dialogs.at(-1)!==dialog){stop();return;}
    const active=document.activeElement;
    if(active&&active!==document.body&&active.isConnected&&interactive(active)&&!root.contains(active)){stop();return;}
    if(dialog.querySelector('#slack-list-view-select')!==picker){stop();return;}
    if(opening){
      // Attribute old-values catch A-B-A even if one observer delivery sees only A.
      if(selectionChanges.length!==1||selectionChanges[0]!==selection||dialog.dataset.viewSelection!==String(expected.selection)||picker.value!==expected.id){stop();return;}
    }else if(selectionChanges.length||dialog.dataset.viewSelection!==selection||picker.value!==originalID){stop();return;}
    const origin=opening?picker:dialog.querySelector('#slack-list-new-view');
    const target=origin&&!origin.disabled&&visible(origin)?origin:dialog.querySelector('#slack-list-close');
    stop();if(target&&!target.disabled&&visible(target))target.focus({preventScroll:true});
  };
  const schedule=()=>{if(!ended&&!queued){queued=true;frame=requestAnimationFrame(restore);}};
  const observer=new MutationObserver(records=>{
    for(const r of records)if(r.type==='attributes'&&r.target===dialog&&r.attributeName==='data-view-selection')selectionChanges.push(r.oldValue);
    if(selectionChanges.length>(opening?1:0)){stop();return;}schedule();
  });
  const timer=setTimeout(stop,1000);
  const ticket={stop,commit(proof){
    if(ended||!opening||expected||String(proof.serial)!==serial||String(proof.visit)!==visit||!proof.id||String(proof.selection)===selection){stop();return;}
    expected=proof;schedule();
  }};
  window.__listViewCreateReturn=ticket;
  document.addEventListener('pointerdown',input,true);document.addEventListener('keydown',input,true);document.addEventListener('focusin',focus,true);
  observer.observe(document.body,{subtree:true,childList:true,attributes:true,attributeOldValue:true,attributeFilter:['disabled','data-dialog-generation','data-rename-serial','data-view-selection']});schedule();
}
