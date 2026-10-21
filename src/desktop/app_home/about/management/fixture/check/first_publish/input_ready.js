(()=>{
 const expected=EXPECTED,registry=window.__firstPublishRegistry;
 const root=document.getElementById('workflow-management'),modal=document.getElementById('workflow-finish-up');
 const title=document.getElementById('workflow-finish-title'),description=document.getElementById('workflow-finish-description'),upload=document.getElementById('workflow-finish-upload'),details=document.getElementById('workflow-finish-details');
 const saved=root?.__workflowManagement?.permissionsConfirmation;
 const original=!!root&&registry?.parent?.root===root&&registry?.parent?.state===root.__workflowManagement&&!!modal&&registry?.modal?.root===modal&&registry.modal.title===title&&registry.modal.description===description&&registry.modal.upload===upload&&saved?.modal===modal&&saved?.finish?.title===title&&saved.finish.description===description&&saved.finish.upload===upload;
 const bound=!!details&&details.isConnected&&saved?.finishPanes?.details===details&&saved?.finishPane?.node===details&&saved.finishPane.name==='details'&&saved.finishPane.epoch===expected.epoch&&details.style.display==='flex'&&getComputedStyle(details).display==='flex';
 const owner=!!modal&&modal.dataset.finishOwner===expected.owner&&modal.dataset.finishEpoch===expected.epoch&&modal.dataset.finishPane==='details'&&modal.dataset.finishBound==='true'&&modal.dataset.finishBusy==='false'&&modal.dataset.finishIcon===expected.icon;
 const values=!!title&&!!description&&title.isConnected&&description.isConnected&&!title.disabled&&!description.disabled&&!title.readOnly&&!description.readOnly&&title.value===expected.title&&description.value===expected.description&&modal?.dataset.finishTitle===expected.title&&modal?.dataset.finishDescription===expected.description;
 return{ok:original&&bound&&owner&&values,expected,original,bound,owner,values,actual:{renderedOwner:modal?.dataset.finishOwner??null,renderedEpoch:modal?.dataset.finishEpoch??null,bindingEpoch:saved?.finishPane?.epoch??null,pane:modal?.dataset.finishPane??null,paneBound:modal?.dataset.finishBound??null,title:title?.value??null,description:description?.value??null,titleReadonly:title?.readOnly??null,descriptionReadonly:description?.readOnly??null}};
})()
