(() => {
 const command=__COMMAND__,stage=__STAGE__,editor=document.getElementById('canvas-comment-reply'),api=window.superPlatinumRichEditor;
 const pair=[...(window.__superPlatinumCanvasReplyCaptures||[])].find(([,h])=>h.root===editor);
 const clone=v=>JSON.parse(JSON.stringify(v));
 if(command==='append'){
  if(!editor||!api?.snapshot(editor))return {ok:false};
  const walker=document.createTreeWalker(editor,NodeFilter.SHOW_TEXT);let leaf=null,node;
  while(node=walker.nextNode())if(node.textContent==='Opaque attributes retained.')leaf=node;
  if(!leaf)return {ok:false};
  editor.focus();const s=getSelection(),r=document.createRange();r.setStart(leaf,leaf.length);r.collapse(true);s.removeAllRanges();s.addRange(r);
  const text=' Canonical tail \u4e2d\ud83e\udd8a';
  if(!document.execCommand('insertText',false,text))return {ok:false};
  editor.dispatchEvent(new InputEvent('input',{bubbles:true,inputType:'insertText',data:text}));return {ok:true};
 }
 if(command==='remember'){
  const ticket=editor&&api?.snapshot(editor);if(!ticket||!pair?.[1].running)return {ok:false};
  window.__canonicalDraft={root:editor,mount:pair[0],scope:pair[1].scope,generation:pair[1].generation,blocks:clone(ticket.blocks)};
  return {ok:true,mount:pair[0],blocks:clone(ticket.blocks)};
 }
 if(command==='composition-start'||command==='composition-end'){
  if(!editor)return {ok:false};editor.dispatchEvent(new CompositionEvent(command==='composition-start'?'compositionstart':'compositionend',{bubbles:true,data:''}));
  return {ok:true};
 }
 if(command==='fail-snapshot'){
  if(!editor||!pair?.[1].running||!api?.snapshot(editor))return {ok:false};
  const original=api.snapshot,root=editor;window.__canonicalFault={count:0,restored:false};
  api.snapshot=function(node){if(node===root){api.snapshot=original;window.__canonicalFault.count++;window.__canonicalFault.restored=true;throw Error('Exact original fixture capture failure');}return original.call(this,node);};
  editor.dispatchEvent(new InputEvent('input',{bubbles:true,inputType:'insertText',data:''}));return {ok:true};
 }
 if(command==='click'||command==='double-click'){
  const a=document.querySelector('[data-canvas-file-id="FCANON001"]');if(!a)return {ok:false};
  a.scrollIntoView({block:'center'});const events=[];
  for(let i=0;i<(command==='double-click'?2:1);i++){const e=new MouseEvent('click',{bubbles:true,cancelable:true});a.dispatchEvent(e);events.push(e.defaultPrevented);}
  return {ok:events.every(Boolean),prevented:events};
 }
 const panel=document.getElementById('canvas-comments'),scroll=panel?.querySelector('.canvas-comments-scroll');
 const viewer=document.querySelector('.file-viewer-dialog'),retry=document.getElementById('canvas-reply-retry');
 const rect=e=>{const r=e?.getBoundingClientRect();return r?{x:r.x,y:r.y,width:r.width,height:r.height}:null};
 const proof=(e,owner=e)=>{const r=rect(e);if(!r)return null;const h=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);return {rect:r,text:e.textContent??null,inView:r.width>0&&r.height>0&&r.x>=0&&r.y>=0&&r.x+r.width<=innerWidth+1&&r.y+r.height<=innerHeight+1,hit:!!h&&(h===owner||owner?.contains(h))};};
 const line=()=>{if(!editor)return null;const walker=document.createTreeWalker(editor,NodeFilter.SHOW_TEXT);let n;while(n=walker.nextNode()){if(n.textContent.trim()){const r=document.createRange();r.setStart(n,0);r.setEnd(n,Math.min(n.length,30));return r;}}return null;};
 const targetElement=viewer?viewer.querySelector('header h2'):stage==='capture-failed'?retry:stage==='held'?panel?.querySelector('[data-canvas-file-id]'):null;
 const range=targetElement?null:line(),target=targetElement||range;
 if(command==='scroll'){
  if(viewer){viewer.querySelector('header h2')?.scrollIntoView({block:'nearest'});return {ok:true};}
  if(!scroll||!target)return {ok:false};const r=target.getBoundingClientRect(),a=scroll.getBoundingClientRect();scroll.scrollTop+=(r.top+r.bottom-a.top-a.bottom)/2;return {ok:true};
 }
 const ticket=editor&&api?.snapshot(editor),remembered=window.__canonicalDraft;
 const native=ticket?{blocks:ticket.blocks,composing:ticket.composing,mount:pair?.[0]??null,scope:pair?.[1].scope??null,generation:pair?.[1].generation??null,running:pair?.[1].running??false,sameNode:!!remembered&&editor===remembered.root,sameBlocks:!!remembered&&JSON.stringify(ticket.blocks)===JSON.stringify(remembered.blocks),sameMount:!!remembered&&pair?.[0]===remembered.mount}:null;
 const targetProof=proof(target,targetElement||editor);
 const scrollRect=rect(scroll);
 const inScroll=!!viewer||!!targetProof&&!!scrollRect&&targetProof.rect.y>=scrollRect.y-1&&targetProof.rect.y+targetProof.rect.height<=scrollRect.y+scrollRect.height+1;
 const image=viewer?.querySelector('.viewer img');
 const imageProof=image?{...proof(image),ready:image.complete&&image.naturalWidth>0,src:image.getAttribute('src')}:null;
 const panelAlerts=[...(panel?.querySelectorAll('[role=alert]')||[])].map(e=>{
  const r=e.getBoundingClientRect(),s=getComputedStyle(e);
  return {text:e.textContent,owner:e.closest('.canvas-comment-reply')?'composer':'panel',
   visible:!!e.getClientRects().length&&r.width>0&&r.height>0&&s.display!=='none'&&s.visibility!=='hidden'&&!e.closest('[hidden]')};
 });
 return {ok:!!targetProof?.inView&&targetProof.hit&&inScroll&&(!viewer||(imageProof?.ready&&proof(viewer.querySelector('header button'))?.inView)),
  viewport:[innerWidth,innerHeight],scroll:scrollRect,inScroll,target:targetProof,targetKind:viewer?'viewer-title':stage==='capture-failed'?'retry':stage==='held'?'link':'editor-line',
  native,fault:window.__canonicalFault??null,alerts:[...(panel?.querySelectorAll('[role=alert]')||[])].map(e=>e.textContent),panelAlerts,
  retry:retry?{...proof(retry),disabled:retry.disabled}:null,
  viewer:viewer?{title:viewer.querySelector('header h2')?.textContent,close:proof(viewer.querySelector('header button')),image:imageProof}:null};
})()
