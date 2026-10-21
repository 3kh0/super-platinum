(()=>{
 const expected=EXPECTED,registry=window.__firstPublishRegistry;
 const modal=document.getElementById('workflow-finish-up'),root=document.getElementById('workflow-management');
 const check=document.getElementById('workflow-finish-check'),done=document.getElementById('workflow-finish-done');
 const notice=document.getElementById('workflow-finish-notice');
 const details=document.getElementById('workflow-finish-details'),saved=root?.__workflowManagement?.permissionsConfirmation;
 const displayed=e=>{if(!e)return false;for(let p=e;p;p=p.parentElement){const s=getComputedStyle(p);if(p.hidden||s.display==='none'||s.visibility!=='visible')return false;}return true;};
 const originalDetails=!!details&&modal?.contains(details)&&saved?.finishPanes?.details===details
  &&saved?.finishPane?.node===details&&saved.finishPane.name==='details'&&modal.dataset.finishPane==='details'
  &&saved.finishPane.epoch===modal.dataset.finishEpoch&&displayed(details);
 const original=!!modal&&!!root&&registry?.modal?.root===modal&&registry?.parent?.root===root;
 const outcomes=details?[...details.children].filter(e=>e.tagName==='P'&&e.textContent.startsWith('Publication: ')).map(e=>e.textContent):[];
 const add=document.getElementById('workflow-confirmation-add'),insideAdd=e=>!!add&&add.contains(e);
 const statuses=details?[...details.querySelectorAll('[role=status]')].filter(e=>displayed(e)&&!insideAdd(e)).map(e=>e.textContent).filter(t=>t.trim()):[];
 const actual={original,originalDetails,owner:modal?.dataset.finishOwner,busy:modal?.dataset.finishBusy,
  checkEnabled:!!check&&!check.disabled,doneEnabled:!!done&&!done.disabled,
  outcomes,notice:notice?.textContent??null,statuses};
 return{ok:original&&originalDetails&&actual.owner===String(expected.id)&&actual.busy==='false'
  &&actual.checkEnabled&&actual.doneEnabled&&modal.contains(check)&&modal.contains(done)
  &&details.contains(check)&&details.contains(done)&&displayed(check)&&displayed(done)
  &&(expected.notice===null||details.contains(notice)&&!insideAdd(notice)&&displayed(notice))
  &&outcomes.length===1&&outcomes[0]==='Publication: '+expected.outcome
  &&actual.notice===expected.notice
  &&statuses.length===(expected.notice===null?0:1)
  &&(expected.notice===null||statuses[0]===expected.notice),actual,expected};
})()
