(()=>{
 const expected=EXPECTED,detail=document.getElementById('workflow-activity-detail');
 if(!detail)return{ok:false};
 const rect=e=>{const r=e.getBoundingClientRect();return{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}};
 const visible=(e,hitRequired=true)=>{
  if(!e)return false;const r=rect(e),s=getComputedStyle(e),hit=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);
  if(!(r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1&&s.visibility==='visible'&&s.display!=='none'&&(!hitRequired||hit===e||e.contains(hit))))return false;
  for(let p=e.parentElement;p;p=p.parentElement){const a=rect(p),c=getComputedStyle(p);
   if(/auto|scroll|hidden|clip/.test(c.overflowY)&&(r.top<a.top-1||r.bottom>a.bottom+1))return false;
   if(/auto|scroll|hidden|clip/.test(c.overflowX)&&(r.left<a.left-1||r.right>a.right+1))return false;
  }return true;
 };
 const opaque=src=>/^(?:http:\/\/super-platinum-media\.emoji\/|super-platinum-media:\/\/emoji\/)[a-zA-Z0-9-]+\?v=\d+$/.test(src||'');
 const spec=[['workflow-activity-trigger-message',0,expected.list,'list',1],['workflow-activity-trigger-message',1,expected.list,'list',3],['workflow-activity-step-message-0',0,expected.canvas,'canvas',1]];
 const rows=spec.map(([container,ordinal,id,kind,part])=>{
  const e=document.getElementById(container)?.querySelectorAll('.workflow-activity-file')[ordinal];if(!e)return{ok:false,container,ordinal};
  e.scrollIntoView({block:'nearest'});
  const name=e.querySelector('.workflow-activity-file-name'),icons=e.querySelectorAll('svg'),icon=icons[0],path=icon?.querySelector('path');
  const semantic=kind==='list'?expected.listLabel:expected.canvasLabel,tag=kind==='list'&&!expected.inert?'A':'SPAN',href=tag==='A'?expected.url:null;
  const expectedId=tag==='A'?'workflow-activity-file-trigger-'+part:'';
  const parts=kind==='list'?
   [{kind:'text',text:'R\u00e9vision & \u4e2d '},{kind:'emoji',text:'\u{1f44b}',title:':wave:'},{kind:'text',text:' '},(expected.cold?{kind:'emoji',text:':activity_cold:',title:':activity_cold:'}:{kind:'image',text:'',alt:':activity_ready_alias:'}),{kind:'text',text:' '},{kind:'text',text:':unknown:'}]:
   [{kind:'text',text:'Canvas & caf\u00e9 '},{kind:'emoji',text:'\u{1f44b}',title:':wave:'},{kind:'text',text:' '},{kind:'emoji',text:':activity_cold:',title:':activity_cold:'},{kind:'text',text:' '},{kind:'text',text:':unknown:'}];
  const fragments=[...(name?.children||[])].map((n,index)=>{
   const images=n.querySelectorAll('img'),img=images[0],image=img?{tag:img.tagName,src:img.getAttribute('src'),alt:img.getAttribute('alt'),title:img.getAttribute('title'),complete:img.complete,naturalWidth:img.naturalWidth,naturalHeight:img.naturalHeight,rect:rect(img),visible:visible(img)}:null;
   const p=parts[index],imageOk=p?.kind==='image'?images.length===1&&n.children.length===1&&n.firstElementChild===img&&image.tag==='IMG'&&opaque(image.src)&&image.alt===p.alt&&image.title===p.alt&&image.complete&&image.naturalWidth===16&&image.naturalHeight===16&&image.visible:images.length===0&&n.children.length===0;
   return{ok:!!p&&n.tagName==='SPAN'&&n.dataset.fileTitleIndex===String(index)&&n.dataset.fileTitleKind===p.kind&&n.textContent===p.text&&n.getAttribute('title')===(p.title??null)&&imageOk&&visible(n,false),
    index:Number(n.dataset.fileTitleIndex),kind:n.dataset.fileTitleKind,tag:n.tagName,text:n.textContent,title:n.getAttribute('title'),rect:rect(n),visible:visible(n,false),image};
  });
  const directChildren=name?.children.length??0,nestedActions=name?.querySelectorAll('a,button,[role=button],script,iframe').length??0;
  const iconProof=icon?{material:icon.classList.contains('material-icon'),ariaHidden:icon.getAttribute('aria-hidden'),viewBox:icon.getAttribute('viewBox'),fill:path?.getAttribute('fill'),pathMatches:path?.getAttribute('d')===(kind==='list'?expected.listPath:expected.canvasPath),rect:rect(icon),visible:visible(icon)}:null;
  const nameRect=name?rect(name):null,inline=!!(iconProof&&nameRect&&iconProof.rect.right<=nameRect.left+1&&Math.abs(iconProof.rect.top+iconProof.rect.bottom-nameRect.top-nameRect.bottom)<=6);
  const text=parts.map(p=>p.text).join('');
  const ok=e.tagName===tag&&e.id===expectedId&&e.dataset.file===id&&e.dataset.fileKind===kind&&e.dataset.part===String(part)&&e.getAttribute('href')===href&&e.textContent===text&&name?.textContent===text&&name?.dataset.fileLabel===semantic&&directChildren===6&&nestedActions===0&&fragments.length===6&&fragments.every(p=>p.ok)&&icons.length===1&&iconProof.material&&iconProof.ariaHidden==='true'&&iconProof.viewBox==='0 -960 960 960'&&iconProof.fill==='currentColor'&&icon.querySelectorAll('path').length===1&&iconProof.pathMatches&&iconProof.visible&&visible(e)&&visible(name)&&inline;
  return{ok,container,ordinal,id:e.id,file:e.dataset.file,kind:e.dataset.fileKind,part:e.dataset.part,tag:e.tagName,interactive:e.matches('a,button,[role=button]'),href:e.getAttribute('href'),text:e.textContent,name:name?.textContent,semanticLabel:name?.dataset.fileLabel,fragments,directChildren,nestedActions,icon:iconProof,nameRect,rect:rect(e),visible:visible(e),inline};
 });
 return{ok:rows.every(r=>r.ok),rows,inert:expected.inert,measurement:'individual-scroll-samples',detailRevision:detail.dataset.detailRevision};
})()
