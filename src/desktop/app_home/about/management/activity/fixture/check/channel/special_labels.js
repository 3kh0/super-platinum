(()=>{
 const fetched=FETCHED;
 const vectors=[
  ['workflow-activity-channel-trigger-1','CACTKNOWN01','activity-known','LockFilled',LOCK_PATH],
  ['workflow-activity-channel-trigger-3','CACTKNOWN01','activity-known','LockFilled',LOCK_PATH],
  ['workflow-activity-channel-step-0-1','CACTMISS01',fetched?'activity-fetched':'Channel unavailable',fetched?'Tag':null,fetched?TAG_PATH:null],
  ['workflow-activity-channel-step-1-1','CACTARCH01','activity-archived (archived)','Archive',ARCHIVE_PATH],
  ['workflow-activity-channel-step-1-3','CACTPRARCH01','activity-private-archived (archived)','ArchiveFilled',FILLED_PATH]
 ];
 const rect=e=>{const r=e.getBoundingClientRect();return{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}};
 const visible=e=>{if(!e)return false;const r=rect(e),s=getComputedStyle(e),h=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);if(!(r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1&&s.visibility==='visible'&&s.display!=='none'&&(h===e||e.contains(h))))return false;for(let p=e.parentElement;p;p=p.parentElement){const b=rect(p),t=getComputedStyle(p);if(/auto|scroll|hidden|clip/.test(t.overflowY)&&(r.top<b.top-1||r.bottom>b.bottom+1))return false;if(/auto|scroll|hidden|clip/.test(t.overflowX)&&(r.left<b.left-1||r.right>b.right+1))return false;}return true;};
 const rows=vectors.map(([id,channel,expected,iconKind,path])=>{
  const e=document.getElementById(id);if(!e)return{id,ok:false};e.scrollIntoView({block:'nearest'});
  const label=e.querySelector('.workflow-activity-channel-label'),name=e.querySelector('.workflow-activity-channel-name'),icons=[...e.querySelectorAll('.workflow-activity-channel-icon')],svg=icons[0],known=iconKind!==null;
  const icon=svg?{tag:svg.tagName.toLowerCase(),ariaHidden:svg.getAttribute('aria-hidden'),viewBox:svg.getAttribute('viewBox'),material:svg.classList.contains('material-icon'),pathMatches:svg.querySelectorAll('path').length===1&&svg.querySelector('path')?.getAttribute('d')===path,visible:visible(svg),rect:rect(svg)}:null;
  const nameRect=name?rect(name):null,inline=!!(icon&&nameRect&&icon.rect.right<=nameRect.left+1&&Math.abs((icon.rect.top+icon.rect.bottom-nameRect.top-nameRect.bottom)/2)<=3);
  const shape=e.tagName==='BUTTON'&&e.dataset.channel===channel&&label?.tagName==='SPAN'&&name?.tagName==='SPAN'&&label.textContent===expected&&name.textContent===expected&&e.textContent===expected&&e.querySelectorAll('.workflow-activity-channel-label').length===1&&e.querySelectorAll('.workflow-activity-channel-name').length===1&&e.querySelectorAll('svg').length===(known?1:0)&&icons.length===(known?1:0);
  return{id,channel:e.dataset.channel,text:e.textContent,tag:e.tagName,name:name?.textContent,labelTag:label?.tagName,nameTag:name?.tagName,iconKind,iconCount:icons.length,svgCount:e.querySelectorAll('svg').length,icon,nameRect,buttonRect:rect(e),visible:visible(e)&&visible(name),inline:known?inline:null,scrollSample:true,ok:shape&&visible(e)&&visible(name)&&(!known||(icon.tag==='svg'&&icon.ariaHidden==='true'&&icon.viewBox==='0 -960 960 960'&&icon.material&&icon.pathMatches&&icon.visible&&inline))};
 });
 return{ok:rows.every(r=>r.ok),fetched,measurement:'individual-scroll-samples',rows};
})()
