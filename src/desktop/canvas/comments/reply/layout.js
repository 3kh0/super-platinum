(() => {
 const dialog=document.querySelector('.canvas-dialog'), panel=document.getElementById('canvas-comments'),editor=document.querySelector('.canvas-comment-reply'),scroll=panel?.querySelector('.canvas-comments-scroll');
 if(!dialog||!panel||!editor||!scroll)return {ok:false};
 const rect=e=>{const r=e.getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height,right:r.right,bottom:r.bottom};};
 const bounded=r=>r.width>0&&r.height>0&&r.x>=-1&&r.y>=-1&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1;
 const controls=[...editor.querySelectorAll('textarea,button,input')].filter(e=>!e.closest('details:not([open])'));
 const samples=[];
 for(const e of controls){e.scrollIntoView({block:'center'});const r=rect(e),hit=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);samples.push({id:e.id,label:e.getAttribute('aria-label')||e.textContent,disabled:e.disabled,rect:r,hit:!!hit&&(hit===e||e.contains(hit))});}
 const target=document.getElementById('canvas-reply-archive')||document.getElementById('canvas-comment-reply');target.scrollIntoView({block:'center'});const t=rect(target),hit=document.elementFromPoint(t.x+t.width/2,t.y+t.height/2);
 return {ok:bounded(rect(dialog))&&bounded(rect(panel))&&samples.length>=3&&samples.every(x=>bounded(x.rect)&&x.hit)&&scroll.scrollWidth<=scroll.clientWidth+1&&bounded(t)&&!!hit&&(hit===target||target.contains(hit)),viewport:[innerWidth,innerHeight],dialog:rect(dialog),panel:rect(panel),samples,noOverflow:scroll.scrollWidth<=scroll.clientWidth+1,finalTarget:target.id,finalRect:t,sampledSequentially:true,text:document.getElementById('canvas-comment-reply')?.value,sendDisabled:document.getElementById('canvas-reply-send')?.disabled};
})()
