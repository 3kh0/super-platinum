(()=>{
 const begin=BEGIN,root=document.getElementById('workflow-management'),state=root?.__workflowManagement,
 detail=document.getElementById('workflow-activity-detail'),notice=document.getElementById('workflow-activity-navigation-notice'),
 owner=state?.channelNavigationNotice,form=document.getElementById('workflow-activity-filters'),input=document.getElementById('workflow-activity-user-search'),date=document.getElementById('workflow-activity-date-range');
 if(!root?.isConnected||!detail?.isConnected||!notice?.isConnected||!form||!input||!date||!owner||owner.phase!=='pending'||owner.notice!==notice||!Array.isArray(owner.nodes)||owner.nodes.length<9)return{ok:false};
 const nodes=[root,form,detail,document.getElementById('workflow-activity-channel-step-0-1'),input,date,...['status','limit','user'].map(k=>document.getElementById('workflow-activity-'+k))];
 if(nodes.some((n,i)=>!n?.isConnected||owner.nodes[i]!==n)||owner.detail!==detail.dataset.detailRevision||notice.dataset.detailRevision!==owner.detail||!/^\d+$/.test(owner.chain)||!Number.isSafeInteger(Number(owner.chain))||Number(owner.chain)<=0||typeof owner.key!=='string'||!owner.key.length)return{ok:false};
 if(begin)window.__workflowHeldChannel={root,state,owner,nodes:[...nodes],notice,detail};
 const original=window.__workflowHeldChannel,sameOriginalNodes=!!original&&original.root===root&&original.state===state&&original.owner===owner&&original.notice===notice&&nodes.every((n,i)=>n===original.nodes[i]);
 const rect=e=>{const r=e.getBoundingClientRect();return{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}};
 const r=rect(notice),style=getComputedStyle(notice),hit=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);
 let visible=r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1&&style.display!=='none'&&style.visibility==='visible'&&(hit===notice||notice.contains(hit));
 for(let p=notice.parentElement;p;p=p.parentElement){const a=rect(p),s=getComputedStyle(p);if(/auto|scroll|hidden|clip/.test(s.overflowY)&&(r.top<a.top-1||r.bottom>a.bottom+1))visible=false;}
 const outsideScroll=!notice.closest('.workflow-activity-content');
 const visiblePending=[...document.querySelectorAll('#workflow-activity-navigation-notice')].filter(n=>n.isConnected&&n.textContent==='Opening conversation\u2026'&&n.getBoundingClientRect().width>0&&n.getBoundingClientRect().height>0).length;
 return{ok:sameOriginalNodes&&visible&&outsideScroll&&visiblePending===1&&notice.getAttribute('role')==='status'&&notice.textContent==='Opening conversation\u2026'&&!state.composing&&!state.lease,
 owner:{chain:Number(owner.chain),detail:owner.detail,phase:owner.phase,key:owner.key,button:owner.nodes[3].id,channel:owner.nodes[3].dataset.channel},
 scope:{visit:root.dataset.managementVisit,document:root.dataset.managementDocument,revision:root.dataset.managementRevision,detail:detail.dataset.detailRevision},
 buffers:{search:{value:input.value,start:input.selectionStart,end:input.selectionEnd,generation:input.dataset.generation},
 status:nodes[6].value,limit:nodes[7].value,user:nodes[8].value,
 date:{from:date.dataset.from,to:date.dataset.to,revision:date.dataset.revision,epoch:date.dataset.calendarEpoch,open:date.dataset.calendarOpen,dirty:date.dataset.calendarDirty,pending:date.dataset.calendarPending,origin:date.dataset.rangeOrigin,defaultFrom:date.dataset.defaultFrom,defaultTo:date.dataset.defaultTo}},
 native:{composing:!!state.composing,refusals:state.channelRefusals||0,leaseAbsent:!state.lease},
 notice:{text:notice.textContent,visible,outsideScroll,visiblePending,role:notice.getAttribute('role'),rect:r},sameOriginalNodes};
})()
