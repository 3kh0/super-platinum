// Installed only by the exact disconnected Share fixture checker.
(()=>{
 const dialog=()=>document.getElementById('file-share-dialog');
 const note=()=>document.getElementById('file-share-note');
 const full=e=>{
  if(!e?.getClientRects().length)return {ok:false};
  const r=e.getBoundingClientRect();let l=Math.max(0,r.left),t=Math.max(0,r.top),right=Math.min(innerWidth,r.right),bottom=Math.min(innerHeight,r.bottom);
  for(let p=e.parentElement;p;p=p.parentElement){const s=getComputedStyle(p),q=p.getBoundingClientRect();if(/auto|scroll|hidden|clip/.test(s.overflowX)){l=Math.max(l,q.left+p.clientLeft);right=Math.min(right,q.left+p.clientLeft+p.clientWidth);}if(/auto|scroll|hidden|clip/.test(s.overflowY)){t=Math.max(t,q.top+p.clientTop);bottom=Math.min(bottom,q.top+p.clientTop+p.clientHeight);}}
  const center=document.elementFromPoint((r.left+r.right)/2,(r.top+r.bottom)/2);
  return {ok:r.width>0&&r.height>0&&right-l>=r.width-1&&bottom-t>=r.height-1&&(center===e||e.contains(center)),rect:[r.left,r.top,r.width,r.height],visible:[Math.max(0,right-l),Math.max(0,bottom-t)]};
 };
 const destinationOutline=()=>{
  const d=dialog(),e=d?.querySelector('#file-share-destination');
  if(!e?.getClientRects().length)return {ok:false};
  const r=e.getBoundingClientRect(),style=getComputedStyle(e),width=parseFloat(style.outlineWidth),offset=parseFloat(style.outlineOffset);
  let left=0,top=0,right=innerWidth,bottom=innerHeight;
  for(let p=e.parentElement;p;p=p.parentElement){const s=getComputedStyle(p),q=p.getBoundingClientRect();if(/auto|scroll|hidden|clip/.test(s.overflowX)){left=Math.max(left,q.left+p.clientLeft);right=Math.min(right,q.left+p.clientLeft+p.clientWidth);}if(/auto|scroll|hidden|clip/.test(s.overflowY)){top=Math.max(top,q.top+p.clientTop);bottom=Math.min(bottom,q.top+p.clientTop+p.clientHeight);}}
  const outset=width+offset,outline=[r.left-outset,r.top-outset,r.right+outset,r.bottom+outset],clip=[left,top,right,bottom];
  const focused=document.activeElement===e&&e.matches(':focus-visible');
  return {ok:focused&&Number.isFinite(width)&&Number.isFinite(offset)&&width>0&&style.outlineStyle==='solid'&&outline[0]>=left&&outline[1]>=top&&outline[2]<=right&&outline[3]<=bottom&&full(e).ok,id:e.id,focused,rect:[r.left,r.top,r.width,r.height],outline,clip,width,offset,style:style.outlineStyle};
 };
 const inspect=(stage,required)=>{
  const d=dialog();if(!d)return {ok:false,stage,error:'Missing Share dialog'};
  const bounds=full(d),close=full(document.getElementById('file-share-close'));
  // Dialog hit-center can be a descendant; bounds still include every clipped ancestor.
  const controls=required.map(selector=>({selector,...full(d.querySelector(selector))}));
  const visibleFeedback=[...d.querySelectorAll('.file-share-feedback [role=alert],.file-share-feedback [role=status]')].filter(e=>e.getClientRects().length).map(e=>e.textContent);
  const proof=stage==='final'&&d.dataset.fileSharePhase==='Editing'?destinationOutline():null;
  return {...(proof?{destinationOutline:proof}:{}),ok:bounds.ok&&close.ok&&controls.every(c=>c.ok)&&d.contains(document.activeElement)&&(!proof||proof.ok),stage,viewport:[innerWidth,innerHeight],bounds,close,controls,phase:d.dataset.fileSharePhase,owner:d.dataset.fileShareId,visibleFeedback,focused:document.activeElement.id||document.activeElement.getAttribute('aria-label')||document.activeElement.tagName,readonly:note()?.getAttribute('aria-readonly'),text:note()?.textContent};
 };
 const reveal=selector=>{const e=dialog()?.querySelector(selector);if(!e)return {ok:false};const active=document.activeElement;e.scrollIntoView({block:'nearest',inline:'nearest'});return {ok:document.activeElement===active};};
 const choose=id=>{const e=[...dialog().querySelectorAll('.file-share-option')].find(e=>e.dataset.destination===id);if(!e||e.disabled)return {ok:false};e.click();return {ok:true};};
 const remember=()=>{
  const root=note(),d=dialog();if(!root||root.contentEditable!=='true')return {ok:false};
  root.focus();const range=document.createRange();range.selectNodeContents(root);range.collapse(false);getSelection().removeAllRanges();getSelection().addRange(range);
  if(!document.execCommand('insertText',false,' Share note Café'))return {ok:false};
  const caret=getSelection().getRangeAt(0);if(caret.startContainer.nodeType!==3||caret.startOffset<4)return {ok:false};getSelection().setBaseAndExtent(caret.startContainer,caret.startOffset-4,caret.startContainer,caret.startOffset);
  window.__fileShareRemember={root,dialog:d,owner:d.dataset.fileShareId,blocks:JSON.stringify(window.superPlatinumRichEditor.read(root.id)),text:root.textContent,anchor:getSelection().anchorNode,anchorOffset:getSelection().anchorOffset,focus:getSelection().focusNode,focusOffset:getSelection().focusOffset};
  return {ok:getSelection().toString()==='Café'};
 };
 const retained=readonly=>{
  const old=window.__fileShareRemember,root=note(),selection=getSelection();
  if(!old||root!==old.root)return {ok:false};
  const controls=[...dialog().querySelectorAll('.rich-editor button')].filter(e=>e.getClientRects().length);
  return {ok:dialog()===old.dialog&&dialog().dataset.fileShareId===old.owner&&root.textContent===old.text&&JSON.stringify(window.superPlatinumRichEditor.read(root.id))===old.blocks&&root.contentEditable===(readonly?'false':'true')&&root.getAttribute('aria-readonly')===String(readonly)&&document.activeElement===root&&selection.anchorNode===old.anchor&&selection.anchorOffset===old.anchorOffset&&selection.focusNode===old.focus&&selection.focusOffset===old.focusOffset&&(!readonly||controls.every(e=>e.disabled)),owner:dialog().dataset.fileShareId,rootRetained:true};
 };
 const menu=element=>{
  const panel=element.querySelector('.file-action-panel');const names=[...panel.querySelectorAll('button')].map(e=>e.textContent.trim());
  return {ok:!panel.querySelector('button button')&&JSON.stringify(names)===JSON.stringify(['Save to Later','Share file','Copy link','Open in Slack']),names};
 };
 const diagnostic=()=>{
  const describe=e=>e?{tag:e.tagName||null,id:e.id||null,role:e.getAttribute('role'),connected:e.isConnected,rects:e.getClientRects().length,disabled:!!e.disabled,generation:e.dataset?.dialogGeneration||null,owner:e.dataset?.fileShareId||null,phase:e.dataset?.fileSharePhase||null,shareOwner:e.dataset?.fileShareOwner??null,recoveryOwner:e.dataset?.shareOwner??null}:null;
  const visible=e=>e.isConnected&&e.getClientRects().length&&getComputedStyle(e).visibility!=='hidden'&&!e.closest('[hidden], [inert]');
  const dialogs=[...document.querySelectorAll('[data-keyboard-dialog]')].filter(visible);
  return {active:describe(document.activeElement),root:describe(dialog()),target:describe(dialog()?.querySelector('#file-share-change-destination')),picker:describe(dialog()?.querySelector('#file-share-destination')),note:describe(note()),topDialog:describe(dialogs.at(-1)),savedRoot:describe(document.getElementById('saved-file-shares-dialog')),savedOpener:describe(document.getElementById('saved-file-shares-open')),attachmentActions:[...document.querySelectorAll('.message-file .file-more')].map(describe),modalCandidates:[...document.querySelectorAll('[data-keyboard-dialog],.modal,[role=dialog]')].map(element=>({...describe(element),visible:!!visible(element)}))};
 };
 window.__fileShareUI={inspect,reveal,choose,remember,retained,menu,diagnostic,destinationOutline};return {ok:true};
})()
