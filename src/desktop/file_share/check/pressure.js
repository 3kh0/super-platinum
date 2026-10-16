await (async()=>{
 const config=await dioxus.recv();
 const assert=(value,message)=>{if(!value)throw Error(message);};
 const pause=()=>new Promise(resolve=>setTimeout(resolve,60));
 const report=async value=>{dioxus.send(value);const ack=await dioxus.recv();assert(ack?.ok===true,'exact parent ACK');return ack;};
 const api=window.__fileShareUI;
 const wait=async(predicate,message)=>{for(let i=0;i<160;i++){if(predicate())return;await pause();}throw Error(message+'; diagnostic='+JSON.stringify(api.diagnostic()));};
 const root=()=>document.getElementById('file-share-dialog'),note=()=>document.getElementById('file-share-note');
 const read=()=>window.superPlatinumRichEditor.read('file-share-note');
 const stable=value=>JSON.stringify(value,(_,v)=>v&&typeof v==='object'&&!Array.isArray(v)?Object.fromEntries(Object.entries(v).sort(([a],[b])=>a.localeCompare(b))):v);
 const same=(a,b)=>stable(a)===stable(b);
 const blocks=text=>[{type:'rich_text',elements:[{type:'rich_text_section',elements:[{type:'text',text}]}]}];
 const earlier=i=>blocks('Earlier capture '+i),CURRENT=blocks('Earlier capture 31 Current unsaved edit');
 const more=()=>document.querySelector('.message-file .file-more');
 const reveal=async selector=>{assert(api.reveal(selector).ok,'reveal retains focus');await pause();};
 const press=id=>{const element=document.getElementById(id);assert(element&&!element.disabled,'enabled actual '+id);element.click();};
 const selectEnd=()=>{note().focus();const range=document.createRange();range.selectNodeContents(note());range.collapse(false);getSelection().removeAllRanges();getSelection().addRange(range);};
 const replace=text=>{note().focus();const range=document.createRange();range.selectNodeContents(note());getSelection().removeAllRanges();getSelection().addRange(range);assert(document.execCommand('insertText',false,text),'native editing command');assert(same(read(),blocks(text)),'exact typed replacement');};
 const open=async(review=false)=>{await wait(more,'attachment Actions');more().scrollIntoView({block:'nearest'});more().focus();more().click();await wait(()=>document.querySelector('.file-share-action')&&api.menu(document.querySelector('.message-file')).ok,'exact Share menu');document.querySelector('.file-share-action').click();await wait(()=>root()&&note()&&root().dataset.fileSharePhase===(review?'DraftReview':'Editing')&&note().contentEditable===(review?'false':'true'),review?'retained earlier copy review':'authoritative claimed editor');await wait(()=>{try{return Array.isArray(read());}catch{return false;}},'reader handshake');};
 const closedLayout=opener=>{const r=opener.getBoundingClientRect(),hit=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);return {ok:r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth&&r.bottom<=innerHeight&&(hit===opener||opener.contains(hit))&&document.activeElement===opener&&!root(),viewport:[innerWidth,innerHeight],focused:opener.id,opener:{rect:[r.left,r.top,r.width,r.height]}};};
 try {
  assert(config.identity.workflow==='pressure'&&config.identity.case==='accepted'&&['file-share-pressure','file-share-pressure-narrow'].includes(config.identity.fixture),'exact pressure fixture');
  await open();
  await wait(()=>document.activeElement===document.getElementById('file-share-destination'),'destination focus');
  assert(api.choose(config.destination).ok,'actual destination choice');
  await wait(()=>document.activeElement===document.getElementById('file-share-change-destination'),'destination settled');
  replace('Earlier capture 0');
  const originalRoot=root(),originalNote=note(),owner=root().dataset.fileShareId,opener=more();
  await reveal('#file-share-note');
  await report({stage:'ready',owner,note:read(),layout:api.inspect('ready',['#file-share-note','#file-share-close'])});
  const failures=[];
  for(let index=0;index<32;index++) {
   if(index)replace('Earlier capture '+index);
   press('file-share-close');
   const ack=await report({failed:index});
   assert(ack.failed.index===index&&ack.failed.outcome==='storage','actual indexed storage refusal');failures.push(ack.failed);
   await wait(()=>root()===originalRoot&&note()===originalNote&&root().dataset.fileSharePhase==='Editing'&&note().contentEditable==='true','failed Close keeps same editable mount');
   assert(same(read(),earlier(index)),'failed Close retains actual typed body');
   assert(root().querySelector('.file-share-feedback .file-share-error')?.textContent==='The message could not be saved. Keep this window open and try again.','actual failed-save error');
  }
  selectEnd();assert(document.execCommand('insertText',false,' Current unsaved edit'),'new current undo transaction');assert(same(read(),CURRENT),'current exact unsaved body');
  const range=getSelection().getRangeAt(0);assert(range.startContainer.nodeType===3&&range.startOffset>=4,'current selection text node');
  getSelection().setBaseAndExtent(range.startContainer,range.startOffset,range.startContainer,range.startOffset-4);
  const savedSelection={anchor:getSelection().anchorNode,ao:getSelection().anchorOffset,focus:getSelection().focusNode,fo:getSelection().focusOffset};
  const retained=()=>{const s=getSelection();const sameEditor=root()===originalRoot&&note()===originalNote&&root().dataset.fileShareId===owner;const selection=s.anchorNode===savedSelection.anchor&&s.anchorOffset===savedSelection.ao&&s.focusNode===savedSelection.focus&&s.focusOffset===savedSelection.fo;return {ok:sameEditor&&selection&&s.toString()==='edit'&&same(read(),CURRENT)&&document.activeElement===note()&&note().contentEditable==='true',sameEditor,selection,backward:s.anchorNode===s.focusNode&&s.anchorOffset>s.focusOffset,selectionText:s.toString()};};
  const capture=async(stage,selectors,extra={})=>{await reveal(selectors.at(-1));assert(retained().ok,'current root/body/range retained at '+stage);await report({stage,owner,note:read(),retained:retained(),layout:api.inspect(stage,selectors),...extra});};
  await capture('full',['#file-share-note']);
  // Wrapper removes only the exact private abort trigger before this ACK.
  press('file-share-close');await pause();await pause();
  await capture('capacity',['#file-share-review-earlier'],{capacityAttempt:true});
  const review=document.getElementById('file-share-review-earlier');
  const mouse=new MouseEvent('mousedown',{bubbles:true,cancelable:true});review.dispatchEvent(mouse);assert(mouse.defaultPrevented,'Review mousedown preserves current focus');review.click();
  await wait(()=>document.getElementById('file-share-earlier-text'),'exact earlier text preview');
  const preview=document.querySelector('.file-share-earlier-preview');
  assert(preview&&!preview.querySelector('[contenteditable],.rich-editor'),'preview creates no second editor');
  assert(root().querySelectorAll('#file-share-note').length===1,'single current editor');
  await capture('preview',['#file-share-earlier-text','#file-share-discard-earlier'],{previewText:document.getElementById('file-share-earlier-text').textContent,previewDestination:[...preview.querySelectorAll('p')].map(e=>e.textContent).find(s=>s==='To share-fixture'),previewId:Number(preview.dataset.captureId)});
  const discard=document.getElementById('file-share-discard-earlier');const down=new MouseEvent('mousedown',{bubbles:true,cancelable:true});discard.dispatchEvent(down);assert(down.defaultPrevented,'Discard mousedown preserves current focus');discard.click();
  await wait(()=>!document.querySelector('.file-share-earlier-preview')&&document.activeElement===originalNote,'discard keeps current editor focused');
  const beforeUndo=retained();assert(beforeUndo.ok,'exact range retained after disposal before undo');
  assert(document.execCommand('undo')&&same(read(),earlier(31)),'one undo restores actual previous editor body');
  assert(document.execCommand('redo')&&same(read(),CURRENT),'redo restores current unsaved body');
  // Undo changes its own caret; restore the same backward range on the current
  // text node for the final save. Equality before undo was asserted above.
  note().focus();
  const walker=document.createTreeWalker(note(),NodeFilter.SHOW_TEXT);let leaf=null;
  for(let node=walker.nextNode();node;node=walker.nextNode()){if(node.data.length)leaf=node;}
  assert(leaf&&note().contains(leaf)&&leaf.data.length>=4&&leaf.data.endsWith('edit'),'redo final text leaf has exact edit suffix');
  getSelection().setBaseAndExtent(leaf,leaf.data.length,leaf,leaf.data.length-4);
  savedSelection.anchor=getSelection().anchorNode;savedSelection.ao=getSelection().anchorOffset;savedSelection.focus=getSelection().focusNode;savedSelection.fo=getSelection().focusOffset;
  await capture('discarded',['#file-share-note'],{undoRedo:{ok:true,beforeUndo,undoNote:earlier(31),redoNote:read()}});
  press('file-share-close');await wait(()=>!root(),'actual saved Close');await wait(()=>document.activeElement===opener&&more()===opener,'exact Actions return focus');
  assert(!originalRoot.isConnected&&!originalNote.isConnected,'original editor detached after saved Close');
  await report({stage:'closed',owner,returnedFocus:true,oldDetached:true,layout:closedLayout(opener)});
  // The 31 preserved failed captures retain review priority over the durable row.
  // Rust separately proves the current body remains in the newly claimed r3 row.
  await open(true);await wait(()=>{try{return same(read(),earlier(30));}catch{return false;}},'reopened exact retained earlier copy');
  assert(root()!==originalRoot&&note()!==originalNote&&root().dataset.fileShareId!==owner,'fresh editor owner');
  assert(note().getAttribute('aria-readonly')==='true'&&!document.getElementById('file-share-submit'),'retained review has no editable Share action');
  assert(root().textContent.includes('This captured message has not been saved. Review it before discarding it.'),'explicit unsaved-copy review');
  document.getElementById('file-share-close').focus();await reveal('#file-share-note');
  await report({stage:'reopened',owner:root().dataset.fileShareId,note:read(),freshOwner:true,readonly:true,retainedReview:true,layout:api.inspect('reopened',['#file-share-note','#file-share-close'])});
  await report({ok:true,actualMenu:true,actualDestination:true,syntheticEditing:true,failedCloseCount:failures.length,sameEditorSelection:true,undoRedo:true,actualClose:true,actualReopen:true});
 }catch(error){await report({ok:false,error:error.message,diagnostic:api.diagnostic()});}
})();
