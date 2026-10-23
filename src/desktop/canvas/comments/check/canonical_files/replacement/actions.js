(() => {
 const command=__COMMAND__,editor=document.getElementById('canvas-comment-reply'),api=window.superPlatinumRichEditor;
 const pair=[...(window.__superPlatinumCanvasReplyCaptures||[])].find(([,h])=>h.root===editor);
 const clone=x=>JSON.parse(JSON.stringify(x));
 if(command==='arm'){
  if(!editor||!pair?.[1].running||!api?.snapshot(editor))return {ok:false};
  const holder=pair[1],owner={mount:pair[0],scope:holder.scope,generation:holder.generation};
  window.__canvasReplacementOriginal={root:editor,holder,...owner,blocks:clone(api.snapshot(editor).blocks)};
  const entry=window.__canvasReplacementCaptures?.get(owner.mount);if(!entry)return {ok:false};
  return {ok:true,bridge:entry.arm()};
 }
 const original=window.__canvasReplacementOriginal,entry=original&&window.__canvasReplacementCaptures?.get(original.mount);
 if(command==='release'){
  const s=entry?.state(),last=s?.held.at(-1);if(!last)return {ok:false};
  return {ok:true,bridge:entry.release({...entry.owner,sequence:last.sequence})};
 }
 const rect=e=>{const r=e?.getBoundingClientRect();return r?{x:r.x,y:r.y,width:r.width,height:r.height}:null;};
 const snapshot=editor&&api?.snapshot(editor);
 const originalSnapshot=original&&api?.snapshot(original.root);
 return {ok:true,viewport:[innerWidth,innerHeight],canvasPresent:!!document.querySelector('.canvas-dialog'),
  editor:snapshot?{blocks:snapshot.blocks,composing:snapshot.composing,rect:rect(editor),
    mount:pair?.[0]??null,scope:pair?.[1].scope??null,generation:pair?.[1].generation??null,
    running:pair?.[1].running??false,sameOriginalNode:editor===original?.root,
    sameOriginalMount:pair?.[0]===original?.mount}:null,
  original:original?{mount:original.mount,scope:original.scope,generation:original.generation,
    connected:original.root.isConnected,holderSame:window.__superPlatinumCanvasReplyCaptures?.get(original.mount)===original.holder,
    running:original.holder.running,blocks:originalSnapshot?.blocks??null}:null,
  bridge:entry?.state()??null};
})()
