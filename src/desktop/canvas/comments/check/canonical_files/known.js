(() => {
 const command=__COMMAND__,stage=__STAGE__,root='1719800000.000100';
 const panel=document.getElementById('canvas-comments'),scroll=panel?.querySelector('.canvas-comments-scroll');
 const row=panel?.querySelector(`[data-comment-ts="${root}"]`),editor=document.getElementById('canvas-comment-reply');
 const api=window.superPlatinumRichEditor,pair=[...(window.__superPlatinumCanvasReplyCaptures||[])].find(([,h])=>h.root===editor);
 const viewer=document.querySelector('.file-viewer-dialog'),status=row?.querySelector('[data-canvas-file-acquisition]'),retry=row?.querySelector('[data-canvas-file-retry]');
 const link=row?.querySelector('[data-canvas-file-id="FCANON001"]');
 const line=()=>{if(!editor)return null;const walker=document.createTreeWalker(editor,NodeFilter.SHOW_TEXT);let n;while(n=walker.nextNode()){if(n.textContent.trim()){const r=document.createRange();r.setStart(n,0);r.setEnd(n,Math.min(n.length,30));return r;}}return null;};
 const controls=[...(panel?.querySelectorAll('[data-canvas-comment-action]')||[])];
 if(command==='remember-source'){window.__knownFileRoot={row,controls,editor};return {ok:!!row&&!!editor};}
 const source=window.__knownFileRoot;
 const sourceIdentity={sameRow:!!source&&source.row===row,sameEditor:!!source&&source.editor===editor,sameControls:!!source&&controls.length>=3&&source.controls.length===controls.length&&controls.every((e,i)=>e===source.controls[i]),root:row?.dataset.commentTs??null};
 const targetElement=viewer?viewer.querySelector('header h2'):['known','renamed','recovered'].includes(stage)?link:stage==='draft'||stage==='reopened'?null:status||row?.querySelector('.message-body');
 const target=targetElement||line();
 const rect=e=>{const r=e?.getBoundingClientRect();return r?{x:r.x,y:r.y,width:r.width,height:r.height}:null};
 const proof=(e,owner=e)=>{const r=rect(e);if(!r)return null;const h=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);return {rect:r,text:e.textContent??null,inView:r.width>0&&r.height>0&&r.x>=0&&r.y>=0&&r.x+r.width<=innerWidth+1&&r.y+r.height<=innerHeight+1,hit:!!h&&(h===owner||owner?.contains(h))};};
 if(command==='scroll'){
  if(viewer){target?.scrollIntoView({block:'nearest'});return {ok:true};}
  if(!scroll||!target)return {ok:false};const r=target.getBoundingClientRect(),a=scroll.getBoundingClientRect();scroll.scrollTop+=(r.top+r.bottom-a.top-a.bottom)/2;return {ok:true};
 }
 const ticket=editor&&api?.snapshot(editor),remembered=window.__canonicalDraft;
 const native=ticket?{blocks:ticket.blocks,composing:ticket.composing,mount:pair?.[0]??null,scope:pair?.[1].scope??null,generation:pair?.[1].generation??null,running:pair?.[1].running??false,sameNode:!!remembered&&editor===remembered.root,sameBlocks:!!remembered&&JSON.stringify(ticket.blocks)===JSON.stringify(remembered.blocks),sameMount:!!remembered&&pair?.[0]===remembered.mount}:null;
 const p=proof(target,targetElement||editor),s=rect(scroll),inScroll=!!viewer||!!p&&!!s&&p.rect.y>=s.y-1&&p.rect.y+p.rect.height<=s.y+s.height+1;
 const image=viewer?.querySelector('.viewer img'),close=viewer?.querySelector('header button');
 return {ok:sourceIdentity.sameRow&&sourceIdentity.sameEditor&&sourceIdentity.sameControls&&!!p?.inView&&p.hit&&inScroll&&(!viewer||(image?.complete&&image.naturalWidth>0&&proof(close)?.inView)),
  viewport:[innerWidth,innerHeight],scroll:s,inScroll,target:p,
  targetKind:viewer?'viewer-title':stage==='draft'||stage==='reopened'?'editor-line':['known','renamed','recovered'].includes(stage)?'link':'metadata-status',
  native,sourceIdentity,body:row?.querySelector('.message-body')?.textContent??null,
  status:status?{text:status.querySelector('[role=status]')?.textContent??null,phase:status.getAttribute('data-phase')}:null,
  retry:retry?{...proof(retry),disabled:retry.disabled,style:{opacity:Number(getComputedStyle(retry).opacity),cursor:getComputedStyle(retry).cursor}}:null,
  link:link?{id:link.dataset.canvasFileId,name:link.dataset.canvasFileName,url:link.dataset.canvasFileUrl,root:link.dataset.canvasFileRoot}:null,
  images:row?[...row.querySelectorAll('img')].map(e=>e.getAttribute('src')):[],
  viewer:viewer?{title:viewer.querySelector('header h2')?.textContent,close:proof(close),ready:image?.complete&&image.naturalWidth>0}:null};
})()
