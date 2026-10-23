(()=>{
 const root=document.getElementById('workflow-management'),detail=document.getElementById('workflow-activity-detail');
 const buttons=[...document.querySelectorAll('#workflow-activity-detail button[data-channel]')];
 const names=buttons.map(e=>e.querySelector('.workflow-activity-channel-name'));
 if(!root||!detail||buttons.length!==7||names.some(e=>!e))return{ok:false};
 const prior=window.__activityGraceIdentity;
 if(!prior)window.__activityGraceIdentity={root,detail,buttons,names};
 const saved=window.__activityGraceIdentity;
 const rows=buttons.map((e,i)=>({id:e.id,channel:e.dataset.channel,sameButton:e===saved.buttons[i],sameName:names[i]===saved.names[i],connected:e.isConnected,text:e.textContent}));
 return{ok:root===saved.root&&detail===saved.detail&&rows.every(r=>r.sameButton&&r.sameName&&r.connected),sameRoot:root===saved.root,sameDetail:detail===saved.detail,rows};
})()
