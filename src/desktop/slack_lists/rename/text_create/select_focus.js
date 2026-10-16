(serial, choice, epoch) => {
 const root=document.getElementById('slack-list-rename');
 const input=document.getElementById('slack-list-select-option-label');
 if(!root||!input)return;
 requestAnimationFrame(()=>{
  const top=[...document.querySelectorAll('[data-keyboard-dialog="true"]')].filter(e=>e.getClientRects().length&&!e.closest('[hidden],[inert]')).at(-1);
  if(root.isConnected&&input.isConnected&&root===document.getElementById('slack-list-rename')&&input===document.getElementById('slack-list-select-option-label')&&root.contains(input)&&top===root&&root.dataset.renameSerial===String(serial)&&root.dataset.renameStage==='Editing'&&document.getElementById('slack-list-new-field-kind')?.dataset.fieldChoice===String(choice)&&document.getElementById('slack-list-new-field-kind')?.value==='select'&&input.dataset.addEpoch===String(epoch)&&!input.disabled&&input.getClientRects().length&&document.activeElement===document.body)input.focus();
 });
}
