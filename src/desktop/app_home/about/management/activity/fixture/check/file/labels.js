(()=>{
 const expected=EXPECTED,detail=document.getElementById('workflow-activity-detail');
 if(!detail)return{ok:false};
 const rect=e=>{const r=e.getBoundingClientRect();return{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}};
 const visible=e=>{if(!e)return false;const r=rect(e),s=getComputedStyle(e),hit=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);if(!(r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1&&s.visibility==='visible'&&s.display!=='none'&&(hit===e||e.contains(hit))))return false;for(let p=e.parentElement;p;p=p.parentElement){const a=rect(p),c=getComputedStyle(p);if(/auto|scroll|hidden|clip/.test(c.overflowY)&&(r.top<a.top-1||r.bottom>a.bottom+1))return false;if(/auto|scroll|hidden|clip/.test(c.overflowX)&&(r.left<a.left-1||r.right>a.right+1))return false;}return true;};
 const spec=[['workflow-activity-trigger-message',0,expected.list,'list',1],['workflow-activity-trigger-message',1,expected.list,'list',3],['workflow-activity-step-message-0',0,expected.canvas,'canvas',1]];
 const rows=spec.map(([container,ordinal,id,kind,part])=>{
  const e=document.getElementById(container)?.querySelectorAll('.workflow-activity-file')[ordinal];if(!e)return{ok:false,container,ordinal};
  e.scrollIntoView({block:'nearest'});
  const name=e.querySelector('.workflow-activity-file-name'),icons=e.querySelectorAll('svg'),icon=icons[0],path=icon?.querySelector('path');
  const text=kind==='list'?expected.listLabel:expected.canvasLabel,tag=kind==='list'&&!expected.inert?'A':'SPAN',href=tag==='A'?expected.url:null;
  const expectedId=tag==='A'?'workflow-activity-file-trigger-'+part:'';
  const iconProof=icon?{material:icon.classList.contains('material-icon'),ariaHidden:icon.getAttribute('aria-hidden'),viewBox:icon.getAttribute('viewBox'),fill:path?.getAttribute('fill'),pathMatches:path?.getAttribute('d')===(kind==='list'?expected.listPath:expected.canvasPath),rect:rect(icon),visible:visible(icon)}:null;
  const nameRect=name?rect(name):null,inline=!!(iconProof&&nameRect&&iconProof.rect.right<=nameRect.left+1&&Math.abs(iconProof.rect.top+iconProof.rect.bottom-nameRect.top-nameRect.bottom)<=6);
  let literalProof;
  if(expected.encoded){
   const fragments=[...(name?.children??[])].map(n=>({index:Number(n.dataset.fileTitleIndex),kind:n.dataset.fileTitleKind,tag:n.tagName,text:n.textContent,title:n.getAttribute('title'),childElements:n.children.length,childNodes:n.childNodes.length,nodeType:n.firstChild?.nodeType??null}));
   const directChildren=name?.children.length??0,nestedElements=[...(name?.children??[])].reduce((sum,n)=>sum+n.querySelectorAll('*').length,0),nestedActions=e.querySelectorAll('a,button,[role=button],script,iframe').length;
   literalProof={ok:!!name&&name.dataset.fileLabel===text&&name.textContent===text&&directChildren===1&&name.childNodes.length===1&&nestedElements===0&&nestedActions===0&&fragments.length===1&&fragments.every(f=>f.index===0&&f.kind==='text'&&f.tag==='SPAN'&&f.text===text&&f.title===null&&f.childElements===0&&f.childNodes===1&&f.nodeType===3),semanticLabel:name?.dataset.fileLabel,text:name?.textContent,directChildren,nestedElements,nestedActions,fragments};
  }
  const ok=e.tagName===tag&&e.id===expectedId&&e.dataset.file===id&&e.dataset.fileKind===kind&&e.dataset.part===String(part)&&e.getAttribute('href')===href&&e.textContent===text&&name?.textContent===text&&icons.length===1&&iconProof.material&&iconProof.ariaHidden==='true'&&iconProof.viewBox==='0 -960 960 960'&&iconProof.fill==='currentColor'&&icon.querySelectorAll('path').length===1&&iconProof.pathMatches&&iconProof.visible&&visible(e)&&visible(name)&&inline&&(!expected.encoded||literalProof.ok);
  return{ok,container,ordinal,id:e.id,file:e.dataset.file,kind:e.dataset.fileKind,part:e.dataset.part,tag:e.tagName,interactive:e.matches('a,button,[role=button]'),href:e.getAttribute('href'),text:e.textContent,name:name?.textContent,icon:iconProof,nameRect,rect:rect(e),visible:visible(e),inline,...(expected.encoded?{literalProof}:{})};
 });
 return{ok:rows.every(r=>r.ok),rows,inert:expected.inert,measurement:'individual-scroll-samples',detailRevision:detail.dataset.detailRevision};
})()
