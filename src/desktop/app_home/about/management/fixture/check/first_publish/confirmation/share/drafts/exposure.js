(()=>{
 const base=BASE,r=window.__firstPublishRegistry;
 GEOMETRY
 const root=document.getElementById('workflow-management'),modal=document.getElementById('workflow-finish-up');
 if(modal||!root||!r?.shareDrafts?.thread)return base;
 // The outer Add/Share wrappers have no active pane on this exact editor path.
 // Never recover a false wrapper result for a modal, destination or stale child.
 if(base.present!==true||base.modal!==null||base.confirmationAdd!==null||base.destination!==null||base.share!==null||!Array.isArray(base.outcomes)||base.outcomes.length!==0)return{...base,ok:false};
 const observe=e=>{e.scrollIntoView({block:'center',inline:'nearest',behavior:'instant'});return measure(e)};
 const controls=[...root.querySelectorAll('.workflow-management-actions button')];
 if(!controls.length||controls.length!==base.controls.length)return{...base,ok:false};
 base.controls=base.controls.map((v,i)=>{
  const e=controls[i];if(e.id!==v.id||r.ids.get(e)!==v.nodeId)return{...v,exposure:null};
  return{...v,exposure:observe(e)};
 });
 const feedback=[...root.querySelectorAll('.workflow-management-feedback p')].filter(e=>e.textContent.trim());
 if(feedback.length!==base.feedback.length)return{...base,ok:false};
 base.feedback=base.feedback.map((v,i)=>r.ids.get(feedback[i])===v.nodeId?({...v,exposure:observe(feedback[i])}):({...v,exposure:null}));
 const admitted=v=>!!v.exposure?.originalThread&&v.exposure.displayed&&!!(v.exposure.hitPoint||v.exposure.coveredByThread);
 base.backgroundThread={original:true,controlsRetained:true,measurement:'individual-original-exposed-region-or-owned-thread-occlusion'};
 base.ok=base.originalNodes===true&&base.noOverflow===true&&!base.composing&&base.controls.every(admitted)&&base.feedback.every(admitted);
 return base;
})()
