requestAnimationFrame(()=>{
 const input=document.getElementById('slack-list-find-input'),dialog=input?.closest('.slack-list-dialog');
 if(!input||!dialog)return;
 if(!dialog.dataset.findMounted){
  dialog.dataset.findMounted='true';
  dialog.addEventListener('keydown',event=>{
   if(!(event.ctrlKey||event.metaKey)||event.altKey||event.key.toLowerCase()!=='f')return;
   // A nested List surface owns focus even when it temporarily removes Find.
   // Consume its shortcut before the shell can open global Search asynchronously.
   if(dialog.querySelector('#slack-list-cell-editor,#slack-list-rename,#slack-list-record-detail,[aria-modal="true"]')){
    event.preventDefault();event.stopPropagation();return;
   }
   const field=dialog.querySelector('#slack-list-find-input');
   // Unsupported/fallback layouts without a nested surface retain shell routing.
   if(!field?.getClientRects().length)return;
   event.preventDefault();event.stopPropagation();field.focus({preventScroll:true});field.select();
  },true);
 }
 if(input.dataset.findComposition)return;
 input.dataset.findComposition='true';
 input.addEventListener('compositionend',()=>{
  const value=input.value.normalize('NFD');
  if(input.value!==value){input.value=value;input.dispatchEvent(new Event('input',{bubbles:true}));}
 });
});
