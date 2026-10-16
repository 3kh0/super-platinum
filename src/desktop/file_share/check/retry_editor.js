// Exact readonly editor snapshot; checker-only, never changes content or selection.
(()=>{
 const dialog=()=>document.getElementById('file-share-dialog');
 const note=()=>document.getElementById('file-share-note');
 const read=()=>window.superPlatinumRichEditor.read('file-share-note');
 const stable=value=>JSON.stringify(value,(_,v)=>v&&typeof v==='object'&&!Array.isArray(v)?Object.fromEntries(Object.entries(v).sort(([a],[b])=>a.localeCompare(b))):v);
 window.__fileShareRetry={capture(){
  const root=note(),surface=dialog(),selection=getSelection();
  if(!root||!surface||!surface.contains(root)||!root.contains(selection.anchorNode)||!root.contains(selection.focusNode))throw Error('Retry selection is outside exact editor');
  const owner=surface.dataset.fileShareId,blocks=stable(read()),text=root.textContent;
  const anchor=selection.anchorNode,anchorOffset=selection.anchorOffset,focus=selection.focusNode,focusOffset=selection.focusOffset,selectionText=selection.toString();
  if(!selectionText||anchor!==focus||anchorOffset<=focusOffset)throw Error('Retry requires actual backward text selection');
  return ()=>{
   const current=getSelection(),sameRoot=note()===root,sameDialog=dialog()===surface;
   const selectionRetained=current.anchorNode===anchor&&current.anchorOffset===anchorOffset&&current.focusNode===focus&&current.focusOffset===focusOffset;
   return {ok:sameRoot&&sameDialog&&surface.dataset.fileShareId===owner&&root.textContent===text&&stable(read())===blocks&&root.contentEditable==='false'&&root.getAttribute('aria-readonly')==='true'&&document.activeElement===root&&selectionRetained&&current.toString()===selectionText,owner,sameRoot,sameDialog,selectionRetained,selectionText:current.toString(),anchorOffset:current.anchorOffset,focusOffset:current.focusOffset,backward:current.anchorNode===current.focusNode&&current.anchorOffset>current.focusOffset};
  };
 }};
})();
