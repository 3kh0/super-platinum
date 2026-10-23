((action) => {
  const root=document.getElementById('slack-list-comment-edit-draft'),api=window.superPlatinumRichEditor;
  const owner=document.getElementById('slack-list-comment-edit');
  if(!root?.isContentEditable||root!==window.__ownEditOriginalEditor||root.closest('[hidden],[inert]')||!api?.snapshot(root)) return {ok:false};
  const buttons=[...root.closest('.rich-editor').querySelectorAll('.format-toolbar button[aria-label="Italic"]')];
  if(buttons.length!==1)return {ok:false};
  const button=buttons[0],selection=getSelection();
  const selected=()=>selection?.rangeCount&&root.contains(selection.anchorNode)&&root.contains(selection.focusNode)?selection.toString():null;
  const common=()=>({scope:owner?.dataset.listCommentsEditScope,generation:owner?.dataset.editorGeneration,sameNode:root===window.__ownEditOriginalEditor,native:api.snapshot(root),selected:selected()});
  if(action==='select'){
    const walker=document.createTreeWalker(root,NodeFilter.SHOW_TEXT);let node,offset=-1;
    while((node=walker.nextNode())){offset=node.data.indexOf('own');if(offset>=0&&!node.parentElement.closest('[contenteditable="false"]'))break;}
    if(!node||offset<0)return {ok:false};
    root.scrollIntoView({block:'nearest'});root.focus();
    const range=document.createRange();range.setStart(node,offset);range.setEnd(node,offset+3);
    selection.removeAllRanges();selection.addRange(range);window.__ownEditRunsItalic=button;
    return {ok:selected()==='own',action,anchorOffset:selection.anchorOffset,focusOffset:selection.focusOffset,textNode:node.data,...common()};
  }
  if(action==='italic'){
    if(button!==window.__ownEditRunsItalic||button.disabled||selected()!=='own')return {ok:false};
    button.scrollIntoView({block:'nearest'});
    const r=button.getBoundingClientRect(),hit=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);
    const visible=r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1;
    if(!visible||!(hit===button||button.contains(hit)))return {ok:false};
    const before=common(),control={label:button.getAttribute('aria-label'),disabled:button.disabled,sameNode:button===window.__ownEditRunsItalic,visible,hit:true,rect:{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}};
    button.click();return {ok:true,action,before,control};
  }
  if(action==='snapshot')return {ok:true,action,...common()};
  return {ok:false};
})
