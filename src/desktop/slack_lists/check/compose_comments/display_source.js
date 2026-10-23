(action, ready) => {
  const root = document.getElementById('slack-list-comment-draft');
  const row = [...document.querySelectorAll('[data-list-comment-ts]')].find(e => e.dataset.listCommentTs === '1781524801.000001');
  const body = row?.querySelector('.message-body');
  if (!root || !body) return {ok:false};
  const rect = e => {const r=e.getBoundingClientRect(); return {left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height};};
  const visible = e => {const r=rect(e),s=document.querySelector('.slack-list-comments-scroll')?.getBoundingClientRect();return !!s&&r.width>0&&r.height>0&&r.left>=0&&r.right<=innerWidth+1&&r.top>=Math.max(0,s.top)-1&&r.bottom<=Math.min(innerHeight,s.bottom)+1;};
  if (action === 'arm') {
    body.scrollIntoView({block:'nearest'});
    root.focus();
    const walk=document.createTreeWalker(root,NodeFilter.SHOW_TEXT);let text;
    while((text=walk.nextNode()) && text.data.length<3) {}
    if(!text)return {ok:false};
    const selection=getSelection();selection.setBaseAndExtent(text,1,text,3);
    window.__displaySourceProof={root,anchor:selection.anchorNode,focus:selection.focusNode,anchorOffset:selection.anchorOffset,focusOffset:selection.focusOffset,ids:new WeakMap(),serial:0};
  }
  const saved=window.__displaySourceProof;if(!saved)return {ok:false};
  const id=e=>{if(!e)return null;if(!saved.ids.has(e))saved.ids.set(e,++saved.serial);return saved.ids.get(e);};
  const selection=getSelection(),geometry=window.__listCompose.snapshot();
  const images=[...body.querySelectorAll('img.emoji-image')].map(e=>({node:id(e),src:e.getAttribute('src'),alt:e.getAttribute('alt'),title:e.getAttribute('title'),complete:e.complete,width:e.naturalWidth,height:e.naturalHeight,visible:visible(e),rect:rect(e)}));
  const spans=[...body.querySelectorAll('.custom-emoji')].map(e=>({node:id(e),text:e.textContent,title:e.getAttribute('title'),visible:visible(e),rect:rect(e)}));
  const reply={node:id(root),connected:root.isConnected,sameNode:root===saved.root,anchor:id(selection?.anchorNode),focus:id(selection?.focusNode),anchorOffset:selection?.anchorOffset,focusOffset:selection?.focusOffset,sameSelection:selection?.anchorNode===saved.anchor&&selection?.focusNode===saved.focus&&selection?.anchorOffset===saved.anchorOffset&&selection?.focusOffset===saved.focusOffset,blocks:geometry.editor?.blocks,revision:geometry.editor?.revision,composing:geometry.editor?.composing,editable:geometry.editor?.editable};
  const alias=ready ? images.length===1&&images[0].complete&&images[0].width>0&&images[0].alt===':shared_party:'&&images[0].title===':shared_party:'&&images[0].visible : images.length===0&&spans.some(e=>e.text===':shared_party:'&&e.title===':shared_party:'&&e.visible);
  return {ok:geometry.ok&&reply.connected&&reply.sameNode&&reply.sameSelection&&reply.editable&&!reply.composing&&alias&&spans.some(e=>e.text==='\u{1f44b}'&&e.title===':wave:'&&e.visible),ready,row:id(row),body:id(body),bodyText:body.textContent,images,spans,reply,geometry};
}
