(() => {
  const editor=document.getElementById('slack-list-comment-edit-draft');
  const api=window.superPlatinumRichEditor;
  if(!editor?.isContentEditable||editor!==window.__ownEditOriginalEditor||editor.closest('[hidden],[inert]')) return {ok:false};
  const before=api.snapshot(editor);
  editor.focus();
  const undone=document.execCommand('undo',false);
  const after=api.snapshot(editor);
  return {ok:undone&&!!after&&editor===document.getElementById('slack-list-comment-edit-draft'),
    action:'undo',undone,sameNode:true,before,after};
})
