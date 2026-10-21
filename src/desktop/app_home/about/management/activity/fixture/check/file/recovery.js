(()=>{
 const expected=EXPECTED,detail=document.getElementById('workflow-activity-detail');
 if(!detail?.isConnected)return{ok:false};
 const rect=e=>{const r=e.getBoundingClientRect();return{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}};
 const visible=e=>{if(!e)return false;const r=rect(e),s=getComputedStyle(e),h=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);if(!(r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1&&s.visibility==='visible'&&s.display!=='none'&&(h===e||e.contains(h))))return false;for(let p=e.parentElement;p;p=p.parentElement){const a=rect(p),c=getComputedStyle(p);if(/auto|scroll|hidden|clip/.test(c.overflowY)&&(r.top<a.top-1||r.bottom>a.bottom+1))return false;if(/auto|scroll|hidden|clip/.test(c.overflowX)&&(r.left<a.left-1||r.right>a.right+1))return false;}return true};
 const spec=[['workflow-activity-trigger-message',0,expected.list,'list',1],['workflow-activity-trigger-message',1,expected.list,'list',3],['workflow-activity-step-message-0',0,expected.canvas,'canvas',1]];
 const rows=spec.map(([container,ordinal,id,kind,part])=>{
  const e=document.getElementById(container)?.querySelectorAll('.workflow-activity-file')[ordinal];if(!e)return{ok:false,container,ordinal};
  e.scrollIntoView({block:'nearest'});
  const ready=kind==='list'?expected.listReady:expected.canvasReady;
  const text=ready?(kind==='list'?expected.listLabel:expected.canvasLabel):(kind==='list'?'List unavailable':'Canvas unavailable');
  const tag=kind==='list'&&ready?'A':'SPAN',href=tag==='A'?expected.url:null,expectedId=tag==='A'?'workflow-activity-file-trigger-'+part:'';
  const name=e.querySelector('.workflow-activity-file-name'),icons=e.querySelectorAll('svg'),icon=icons[0],path=icon?.querySelector('path');
  const iconProof=icon?{material:icon.classList.contains('material-icon'),ariaHidden:icon.getAttribute('aria-hidden'),viewBox:icon.getAttribute('viewBox'),fill:path?.getAttribute('fill'),pathMatches:path?.getAttribute('d')===(kind==='list'?expected.listPath:expected.canvasPath),rect:rect(icon),visible:visible(icon)}:null;
  const nameRect=name?rect(name):null,inline=!!(iconProof&&nameRect&&iconProof.rect.right<=nameRect.left+1&&Math.abs(iconProof.rect.top+iconProof.rect.bottom-nameRect.top-nameRect.bottom)<=6);
  const ok=e.tagName===tag&&e.id===expectedId&&e.dataset.file===id&&e.dataset.fileKind===kind&&e.dataset.part===String(part)&&e.getAttribute('href')===href&&e.textContent===text&&name?.textContent===text&&icons.length===1&&iconProof.material&&iconProof.ariaHidden==='true'&&iconProof.viewBox==='0 -960 960 960'&&iconProof.fill==='currentColor'&&icon.querySelectorAll('path').length===1&&iconProof.pathMatches&&iconProof.visible&&visible(e)&&visible(name)&&inline;
  return{ok,container,ordinal,id:e.id,file:e.dataset.file,kind:e.dataset.fileKind,part:e.dataset.part,tag:e.tagName,interactive:e.matches('a,button,[role=button]'),href:e.getAttribute('href'),text:e.textContent,name:name?.textContent,icon:iconProof,nameRect,rect:rect(e),visible:visible(e),inline};
 });
 const retries=[...detail.querySelectorAll('button[data-file-retry]')].map(e=>{
  e.scrollIntoView({block:'nearest'});
  const occurrence=e.previousElementSibling;
  return{file:e.dataset.fileRetry,text:e.textContent,disabled:e.disabled,connected:e.isConnected,rect:rect(e),visible:visible(e),part:occurrence?.dataset.part,kind:occurrence?.dataset.fileKind,originalOccurrence:!!(occurrence?.classList.contains('workflow-activity-file')&&occurrence.dataset.file===e.dataset.fileRetry),container:e.closest('.workflow-activity-message')?.id};
 });
 const n=document.getElementById('workflow-activity-file-notice');
 if(n)n.scrollIntoView({block:'nearest'});
 const notice=n?{id:n.id,text:n.textContent,role:n.getAttribute('role'),rect:rect(n),visible:visible(n)}:null;
 const retryIds=retries.map(r=>r.file);
 const ok=rows.every(r=>r.ok)&&JSON.stringify(retryIds)===JSON.stringify(expected.retries)&&retries.every(r=>r.text==='Retry file details'&&!r.disabled&&r.connected&&r.visible&&r.originalOccurrence)&&
  (expected.notice===null?notice===null:notice?.text===expected.notice&&notice.role==='status'&&notice.visible);
 return{ok,rows,retries,notice,measurement:'individual-scroll-samples',detailRevision:detail.dataset.detailRevision};
})()
