(()=>{
 const timeline=document.getElementById('message-timeline'),header=document.querySelector('.conversation-header .header-name'),status=document.querySelector('.conversation .composer-unavailable [role=status]');
 const reason='This channel is archived. Unarchive it in channel details to send messages.';
 const rect=e=>{const r=e.getBoundingClientRect();return{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}};
 const visible=e=>{const r=rect(e),s=getComputedStyle(e),h=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);if(!(r.width>0&&r.height>0&&r.top>=0&&r.left>=0&&r.bottom<=innerHeight+1&&r.right<=innerWidth+1&&s.display!=='none'&&s.visibility==='visible'&&(h===e||e.contains(h))))return false;for(let p=e.parentElement;p;p=p.parentElement){const q=rect(p),t=getComputedStyle(p);if(/auto|scroll|hidden|clip/.test(t.overflowY)&&(r.top<q.top-1||r.bottom>q.bottom+1))return false;if(/auto|scroll|hidden|clip/.test(t.overflowX)&&(r.left<q.left-1||r.right>q.right+1))return false;}return true;};
 if(!timeline||!header||!status||document.getElementById('workflow-management'))return{ok:false};
 const noComposer=!document.getElementById('channel-composer'),controls=[header,timeline,status].map(e=>({id:e.id,visible:visible(e),rect:rect(e)}));
 const timelineScope=timeline.dataset.timelineScope,headerText=header.textContent,archivedReason=status.textContent;
 return{ok:timelineScope===TIMELINE&&headerText===HEADER&&archivedReason===reason&&noComposer&&controls.every(e=>e.visible),viewport:[innerWidth,innerHeight],noOverflow:document.documentElement.scrollWidth<=innerWidth+1,conversation:true,timelineScope,headerText,composerUnavailable:true,noComposer,archivedReason,controls,notices:[]};
})()
