(() => {
 const command=__COMMAND__,editor=document.getElementById('canvas-comment-reply'),api=window.superPlatinumRichEditor;
 if(command==='append'){
  if(!editor||!api?.snapshot(editor))return {ok:false};
  const walker=document.createTreeWalker(editor,NodeFilter.SHOW_TEXT);let leaf=null,n;
  while(n=walker.nextNode())if(n.textContent.endsWith(' Canonical tail \u4e2d\ud83e\udd8a'))leaf=n;
  if(!leaf)return {ok:false};
  editor.focus();const r=document.createRange(),s=getSelection();r.setStart(leaf,leaf.length);r.collapse(true);s.removeAllRanges();s.addRange(r);
  const text=' Locked tail \u4e2d\ud83e\udd8a';
  if(!document.execCommand('insertText',false,text))return {ok:false};
  editor.dispatchEvent(new InputEvent('input',{bubbles:true,inputType:'insertText',data:text}));
  return {ok:true,blocks:api.snapshot(editor).blocks,text};
 }
 const panel=document.getElementById('canvas-comments'),retry=document.getElementById('canvas-reply-retry'),send=document.getElementById('canvas-reply-send');
 const alerts=[...(panel?.querySelectorAll('[role=alert]')||[])].filter(e=>e.getBoundingClientRect().height>0&&!e.closest('[hidden]'));
 const saveError=alerts.find(e=>e.textContent==='The reply is retained here but could not be saved locally. Retry local save before leaving the app.');
 if(command==='scroll-refused'){
  const scroll=panel?.querySelector('.canvas-comments-scroll');if(!scroll||!saveError||!retry)return {ok:false};
  const a=saveError.getBoundingClientRect(),b=retry.getBoundingClientRect(),s=scroll.getBoundingClientRect();
  scroll.scrollTop+=(Math.min(a.top,b.top)+Math.max(a.bottom,b.bottom)-s.top-s.bottom)/2;
  return {ok:true};
 }
 const proof=e=>{if(!e)return null;const r=e.getBoundingClientRect(),h=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);
  return {rect:{x:r.x,y:r.y,width:r.width,height:r.height},text:e.textContent,inView:r.width>0&&r.height>0&&r.x>=0&&r.y>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1,hit:!!h&&(h===e||e.contains(h)),disabled:e.disabled??null};};
 return {ok:true,send:proof(send),retry:proof(retry),alerts:alerts.map(proof)};
})()
