// Fixture-only exact-owner fault injection; never loaded by ordinary UI.
(()=>{
 window.__fileShareCloseReader={arm(root,note){
  const api=window.superPlatinumRichEditor,original=api?.read,owner=root?.dataset.fileShareId;
  if(typeof original!=='function'||!owner||!root.isConnected||!note?.isConnected||!root.contains(note)||note.id!=='file-share-note')throw Error('Invalid reader fault owner');
  let calls=0,unexpected=0,restored=false;
  function wrapped(id){
   if(id==='file-share-note'&&document.getElementById('file-share-dialog')===root&&document.getElementById(id)===note&&root.dataset.fileShareId===owner&&note.isConnected){calls++;throw Error('Owned fixture note read failure');}
   unexpected++;return original.apply(this,arguments);
  }
  api.read=wrapped;
  return {counts:()=>({calls,unexpected}),restore(){
   if(restored)return {calls,unexpected,restored:api.read===original};
   restored=true;
   if(api.read!==wrapped)throw Error('Reader changed during owned fault');
   api.read=original;return {calls,unexpected,restored:api.read===original};
  }};
 }};
})()
