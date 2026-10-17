async function restoreRatingEmoji(serial, visit, choice, token) {
 const root=document.getElementById('slack-list-rename'),picker=document.getElementById('slack-list-rating-picker');
 const dialog=root?.closest('.slack-list-dialog'),button=root?.querySelector('#slack-list-new-rating-emoji'),prior=document.activeElement;
 const captured=!!root&&root.dataset.renameSerial===String(serial)&&dialog?.dataset.dialogGeneration===String(visit)
   &&!!picker&&picker.dataset.ratingPicker===String(token)&&picker.isConnected&&picker.contains(prior)
   &&!!button&&button.dataset.fieldChoice===String(choice)&&!root.__ratingEmojiReturn;
 const custody=captured?{}:null;
 const release=()=>{
  document.removeEventListener('focusin',focus,true);
  if(custody&&root.__ratingEmojiReturn===custody){delete root.__ratingEmojiReturn;root.removeAttribute('data-rating-return');}
 };
 let lost=false;
 const focus=event=>{if(event.target!==document.body&&!picker?.contains(event.target))lost=true;};
 let captureTimer,stopObservation;
 try {
 // Generic editor recovery must not win the removal frame while this exact
 // captured picker owns return. Clearing the attribute wakes that recovery.
 if(captured){root.__ratingEmojiReturn=custody;root.setAttribute('data-rating-return',String(token));document.addEventListener('focusin',focus,true);}
 dioxus.send(captured);
 const revision=await Promise.race([Promise.resolve(dioxus.recv()).catch(()=>null),new Promise(resolve=>{captureTimer=setTimeout(()=>resolve(null),600);})]);
 clearTimeout(captureTimer);
 if(!captured||lost||!Number.isSafeInteger(revision)||revision<0){release();return;}
 // A single bounded observation starts only after exact controller disposition.
 // It covers removal before continuation or after any earlier animation frame.
 await new Promise(resolve=>{
  let frame=null,finished=false;const deadline=performance.now()+1500;
  const end=()=>{if(finished)return;finished=true;observer.disconnect();clearTimeout(timer);if(frame!==null)cancelAnimationFrame(frame);release();resolve();};
  const shown=n=>n?.isConnected&&n.getClientRects().length>0&&!n.closest('[hidden],[inert]')&&!['hidden','collapse'].includes(getComputedStyle(n).visibility);
  const check=()=>{
   frame=null;if(finished)return;if(lost||performance.now()>=deadline)return end();
   if(document.getElementById('slack-list-rename')!==root||!root.isConnected||!dialog.isConnected||root.dataset.renameSerial!==String(serial)||dialog.dataset.dialogGeneration!==String(visit))return end();
   const active=document.activeElement;
   if(active&&active!==document.body&&!picker.contains(active))return end();
   const current=document.getElementById('slack-list-rating-picker');
   if(current&&current!==picker)return end();
   if(picker.isConnected)return;
   if(root.inert||button.disabled||!button.isConnected||root.querySelector('#slack-list-new-rating-emoji')!==button||button.dataset.fieldChoice!==String(revision))return;
   if(document.querySelector('.emoji-picker'))return end();
   const top=[...document.querySelectorAll('[role="dialog"]')].filter(shown).at(-1);
   if(top!==root||!shown(button))return end();
   end();button.focus({preventScroll:true});
  };
  const schedule=()=>{if(!finished&&frame===null)frame=requestAnimationFrame(check);};
  const observer=new MutationObserver(schedule),timer=setTimeout(end,1500);
  stopObservation=end;
  observer.observe(document.body,{childList:true,subtree:true,attributes:true,attributeFilter:['inert','disabled','data-field-choice','data-rename-serial','data-dialog-generation']});
  schedule();
 });
 }finally{clearTimeout(captureTimer);stopObservation?.();release();}
}
