(() => {
 const root=document.getElementById('canvas-comment-reply'),api=window.superPlatinumRichEditor;
 if(!root||!api?.snapshot(root))return {ok:false};
 root.scrollIntoView({block:'center'});root.focus();
 const selection=getSelection(),range=document.createRange();range.selectNodeContents(root);range.collapse(false);selection.removeAllRanges();selection.addRange(range);
 if(!document.execCommand('insertText',false,' Attribution draft \u4e2d\ud83e\udd8a'))return {ok:false};
 root.dispatchEvent(new InputEvent('input',{bubbles:true,inputType:'insertText',data:' Attribution draft \u4e2d\ud83e\udd8a'}));
 return {ok:true};
})()
