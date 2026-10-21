(()=>{
 const capture=CAPTURE,root=document.getElementById('workflow-management'),state=root?.__workflowManagement,
 detail=document.getElementById('workflow-activity-detail'),notice=document.getElementById('workflow-activity-navigation-notice');
 const ids=['workflow-management','workflow-activity-filters','workflow-activity-detail','workflow-activity-user-search','workflow-activity-date-range','workflow-activity-status','workflow-activity-limit','workflow-activity-user','workflow-activity-file-trigger-1','workflow-activity-file-trigger-3','workflow-activity-channel-step-0-1'];
 const nodes=ids.map(id=>document.getElementById(id));
 if(!state||!notice?.isConnected||nodes.some(n=>!n?.isConnected))return{ok:false};
 if(capture)window.__workflowMixedOriginal={nodes,state,notice};
 const saved=window.__workflowMixedOriginal,originalNodes=!!saved&&saved.state===state&&saved.notice===notice&&nodes.every((n,i)=>saved.nodes[i]===n);
 const rect=n=>{const r=n.getBoundingClientRect();return{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}};
 const visible=n=>{const r=rect(n),s=getComputedStyle(n),h=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);if(!(r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1&&s.visibility==='visible'&&s.display!=='none'&&(h===n||n.contains(h))))return false;for(let p=n.parentElement;p;p=p.parentElement){const a=rect(p),v=getComputedStyle(p);if(/auto|scroll|hidden|clip/.test(v.overflowY)&&(r.top<a.top-1||r.bottom>a.bottom+1))return false;}return true;};
 const text=notice.textContent,count=[...document.querySelectorAll('#workflow-activity-navigation-notice')].filter(n=>n.textContent&&visible(n)).length;
 const n=state.channelNavigationNotice,button=n?.nodes?.[3],owner=n?{chain:Number(n.chain),detail:n.detail,phase:n.phase,key:n.key,button:button?.id,channel:button?.dataset.channel??null,file:button?.dataset.file??null}:null;
 const owned=!n||(n.state===state&&n.notice===notice&&n.nodes?.[0]===root&&n.nodes?.[1]===nodes[1]&&n.nodes?.[2]===detail&&n.nodes?.[4]===nodes[3]&&n.nodes?.[5]===nodes[4]&&n.nodes?.[6]===nodes[5]&&n.nodes?.[7]===nodes[6]&&n.nodes?.[8]===nodes[7]&&n.detail===detail.dataset.detailRevision&&Number.isSafeInteger(owner.chain)&&owner.chain>0&&nodes.includes(button));
 const search=nodes[3],date=nodes[4];
 return{ok:originalNodes&&owned&&!state.lease&&!state.composing&&notice.dataset.detailRevision===detail.dataset.detailRevision&&notice.getAttribute('role')==='status'&&(!text||(count===1&&visible(notice))),originalNodes,
 scope:{visit:root.dataset.managementVisit,document:root.dataset.managementDocument,revision:root.dataset.managementRevision,detail:detail.dataset.detailRevision},
 buffers:{search:{value:search.value,start:search.selectionStart,end:search.selectionEnd,generation:search.dataset.generation},status:nodes[5].value,limit:nodes[6].value,user:nodes[7].value,date:{from:date.dataset.from,to:date.dataset.to,revision:date.dataset.revision,epoch:date.dataset.calendarEpoch,open:date.dataset.calendarOpen,dirty:date.dataset.calendarDirty,pending:date.dataset.calendarPending,origin:date.dataset.rangeOrigin,defaultFrom:date.dataset.defaultFrom,defaultTo:date.dataset.defaultTo}},
 owner,notice:{text,role:notice.getAttribute('role'),visible:visible(notice),rect:rect(notice),count},native:{composing:!!state.composing,leaseAbsent:!state.lease,refusals:state.channelRefusals||0}};
})()
