(()=>{
 const old=window.__knownGroupEditor,e=document.getElementById('profile-section-rich'),
  v=e&&window.superPlatinumRichEditor?.resolved(e),s=getSelection();
 const sameNode=!!old&&e===old.e&&e?.parentElement===old.pane;
 const sameSelection=!!old&&!!s&&s.anchorNode===old.anchor&&s.focusNode===old.focus
  &&s.anchorOffset===old.start&&s.focusOffset===old.end;
 const unchanged=!!old&&JSON.stringify(v?.snapshot.blocks)===JSON.stringify(old.blocks)
  &&JSON.stringify(v?.tokens)===JSON.stringify(old.tokens);
 const noToast=!document.getElementById('status-toast');
 return{ok:sameNode&&sameSelection&&unchanged&&e.isContentEditable&&noToast,
  sameNode,sameSelection,noToast,blocks:v?.snapshot.blocks,tokens:v?.tokens,
  start:s?.anchorOffset,end:s?.focusOffset};
})()
