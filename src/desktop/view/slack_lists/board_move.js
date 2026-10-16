(() => {
 const visible=node=>!!node?.isConnected&&!node.closest('[hidden],[inert]')&&node.getClientRects().length>0;
 const top=dialog=>[...document.querySelectorAll('[data-keyboard-dialog=true]')].filter(visible).at(-1)===dialog;
 const owns=(dialog,p)=>visible(dialog)&&dialog.dataset.dialogGeneration===String(p.visit)&&(!('selection'in p)||dialog.dataset.viewSelection===String(p.selection))&&(!('layout'in p)||dialog.dataset.layoutGeneration===String(p.layout));
 const fallback=dialog=>dialog.querySelector('#slack-list-view-select:not(:disabled)')||dialog.querySelector('#slack-list-close:not(:disabled)');
 const freeFocus=old=>!document.activeElement||document.activeElement===document.body||old?.contains(document.activeElement);
 function capture(p){
  const dialog=document.querySelector('.slack-list-dialog');
  if(!owns(dialog,p)||!top(dialog)||dialog.querySelector('#slack-list-cell-editor'))return false;
  const cards=[...dialog.querySelectorAll('.slack-list-card')].filter(c=>c.dataset.rowId===p.row&&c.dataset.instanceId===p.instance);
  const opener=cards.length===1?cards[0].querySelector('[data-board-move=true]:not(:disabled)'):null;
  if(!visible(opener)||opener.disabled)return false;
  dialog.__boardMoveOpening={...p,selection:dialog.dataset.viewSelection,opener,previous:document.activeElement};
  return true;
 }
 function mount(p){
  const editor=document.getElementById('slack-list-cell-editor');
  if(!visible(editor)||editor.dataset.editorGeneration!==String(p.editor))return false;
  const dialog=editor.closest('.slack-list-dialog');
  if(!owns(dialog,p)||!top(editor))return false;
  p={...p,selection:dialog.dataset.viewSelection};
  const opening=dialog.__boardMoveOpening;delete dialog.__boardMoveOpening;
  const admitted=opening&&opening.row===p.row&&owns(dialog,opening);
  // Captured before Rust disabled the opener; disabling may already move focus
  // to BODY. Never adopt a replacement node with the same row/instance IDs.
  const exact=admitted?opening.opener:null,previous=admitted?opening.previous:document.activeElement;
  const custody={editor:p.editor,node:editor,opener:exact,previous};dialog.__boardMoveFocus=custody;
  const observer=new MutationObserver(()=>{
   if(editor.isConnected&&dialog.isConnected)return;observer.disconnect();
   requestAnimationFrame(()=>{
    if(dialog.__boardMoveFocus!==custody||!owns(dialog,p))return;
    const receipt=dialog.querySelector('#slack-list-board-move-return');
    if(receipt?.dataset.originEditor===String(p.editor))return;
    delete dialog.__boardMoveFocus;
    if(!top(dialog)||!freeFocus(editor))return;
    // Cancel cannot adopt a replacement card/opener merely because IDs match.
    const target=visible(exact)&&dialog.contains(exact)&&!exact.disabled?exact:fallback(dialog);
    if(visible(target)){target.focus({preventScroll:true});target.scrollIntoView({block:'nearest',inline:'nearest'});}
   });
  });observer.observe(dialog,{childList:true,subtree:true});
  requestAnimationFrame(()=>{
   if(dialog.__boardMoveFocus!==custody||!owns(dialog,p)||!top(editor)||!visible(editor))return;
   if(document.activeElement!==previous&&!freeFocus(editor))return;
   (editor.querySelector('#slack-list-board-destination:not(:disabled)')||editor.querySelector('button:not(:disabled)')||editor).focus({preventScroll:true});
  });return true;
 }
 function returned(p){
  const receipt=document.getElementById('slack-list-board-move-return'),dialog=receipt?.closest('.slack-list-dialog');
  const custody=dialog?.__boardMoveFocus;
  if(!visible(receipt)||receipt.dataset.returnSerial!==String(p.serial)||receipt.dataset.originEditor!==String(p.editor)||!owns(dialog,p)||!top(dialog)||dialog.querySelector('#slack-list-cell-editor'))return null;
  if(!custody||custody.editor!==p.editor)return null;
  delete dialog.__boardMoveFocus;
  if(!freeFocus(custody.node))return null;
  // Rust's owned canonical read resolved this exact current instance. No
  // inferred lane, same-ID source replacement, page change or Find reset.
  const card=p.instance===null?null:[...dialog.querySelectorAll('.slack-list-card')].find(c=>c.dataset.rowId===p.row&&c.dataset.instanceId===p.instance);
  const action=card?.querySelector('[data-board-move=true]:not(:disabled)');
  const reached=visible(action);const target=reached?action:fallback(dialog);
  if(!visible(target))return null;
  // Reveal the resolved card's content, not just its already-visible Move button.
  if(reached)card.scrollIntoView({block:'nearest',inline:'nearest'});
  target.focus({preventScroll:true});target.scrollIntoView({block:'nearest',inline:'nearest'});
  return document.activeElement===target?!reached:null;
 }
 return {capture,mount,returned};
})()
