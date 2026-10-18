// Exact mounted refinement editor and explicit native post-ACK selection proof.
(() => {
  const root=document.getElementById('slack-list-rename');
  const dialog=root?.closest('.slack-list-dialog');
  if(!root||!dialog||!root.dataset.refinementSave||root.__refinementReturnBound)return;
  root.__refinementReturnBound=true;
  const serial=root.dataset.renameSerial,visit=dialog.dataset.dialogGeneration;
  const picker=dialog.querySelector('#slack-list-view-select');
  const opener=dialog.querySelector(root.dataset.refinementSave==='new'?'#slack-list-refinement-save-new':'#slack-list-refinement-save');
  const close=dialog.querySelector('#slack-list-close');
  if(!serial||!visit||!picker||!opener||!close)return;
  const selection=dialog.dataset.viewSelection,original=picker.value;
  const visible=e=>e?.isConnected&&e.getBoundingClientRect().width>0&&!e.closest('[hidden],[inert]');
  const interactive=e=>e?.matches?.('button,input,select,textarea,a[href],[contenteditable="true"]');
  const arm=ack=>{
    dialog.__refinementReturn?.stop();
    let ended=false,frame=0,expected=null,changes=[];
    const stop=()=>{if(ended)return;ended=true;observer.disconnect();clearTimeout(timer);cancelAnimationFrame(frame);
      document.removeEventListener('pointerdown',input,true);document.removeEventListener('keydown',input,true);document.removeEventListener('focusin',focused,true);
      if(dialog.__refinementReturn===ticket)delete dialog.__refinementReturn;
      if(window.__listRefinementReturn===ticket)delete window.__listRefinementReturn;
    };
    const input=e=>{if(e.isTrusted)stop();};
    const focused=e=>{if(!root.contains(e.target)&&interactive(e.target))stop();};
    const restore=()=>{
      frame=0;if(ended)return;
      if(!dialog.isConnected||document.querySelector('.slack-list-dialog')!==dialog||dialog.dataset.dialogGeneration!==visit){stop();return;}
      const panel=document.getElementById('slack-list-rename');
      if(panel){if(panel!==root||panel.dataset.renameSerial!==serial)stop();return;}
      if(ack&&!expected)return;
      if([...document.querySelectorAll('[role="dialog"]')].filter(visible).at(-1)!==dialog){stop();return;}
      const active=document.activeElement;
      if(active&&active!==document.body&&active.isConnected&&interactive(active)&&!root.contains(active)){stop();return;}
      if(dialog.querySelector('#slack-list-view-select')!==picker||dialog.querySelector('#slack-list-close')!==close){stop();return;}
      if(ack){if(changes.length!==1||changes[0]!==selection||dialog.dataset.viewSelection!==expected.selection||picker.value!==expected.id){stop();return;}}
      else if(changes.length||dialog.dataset.viewSelection!==selection||picker.value!==original){stop();return;}
      const target=ack?picker:(opener.isConnected&&dialog.contains(opener)&&!opener.disabled&&visible(opener)?opener:picker);
      stop();if(!target.disabled&&visible(target))target.focus({preventScroll:true});
    };
    const schedule=()=>{if(!ended&&!frame)frame=requestAnimationFrame(restore);};
    const observer=new MutationObserver(records=>{
      for(const r of records)if(r.type==='attributes'&&r.target===dialog&&r.attributeName==='data-view-selection')changes.push(r.oldValue);
      if(changes.length>(ack?1:0)){stop();return;}schedule();
    });
    const timer=setTimeout(stop,60000);
    const ticket={stop,commit(p){
      if(ended||!ack||expected||String(p.serial)!==serial||String(p.visit)!==visit||!p.id||String(p.selection)===selection){stop();return;}
      expected={id:p.id,selection:String(p.selection)};schedule();
    }};
    dialog.__refinementReturn=ticket;window.__listRefinementReturn=ticket;
    document.addEventListener('pointerdown',input,true);document.addEventListener('keydown',input,true);document.addEventListener('focusin',focused,true);
    observer.observe(document.body,{subtree:true,childList:true,attributes:true,attributeOldValue:true,attributeFilter:['disabled','data-dialog-generation','data-rename-serial','data-view-selection']});schedule();
  };
  root.__armRefinementReturn=arm;
  root.addEventListener('click',e=>{
    const open=e.target.closest?.('#slack-list-new-view-open');
    if(open&&root.contains(open)&&!open.disabled)arm(true);
    const confirm=e.target.closest?.('#slack-list-rename-confirm');
    if(root.dataset.refinementSave==='new'&&confirm&&root.contains(confirm)&&!confirm.disabled)arm(true);
  },true);
})();
