(()=>{
 const expected=EXPECTED_SHARE,root=document.getElementById('workflow-management'),state=root?.__workflowManagement;
 const parent=state?.permissionsConfirmation,saved=parent?.share;
 const panel=document.getElementById('workflow-finish-share-pane'),modal=document.getElementById('workflow-finish-up');
 const ids={select:'workflow-share-destination',refresh:'workflow-share-refresh',text:'workflow-share-message',cancel:'workflow-share-cancel',send:'workflow-share-send'};
 const nodes=Object.fromEntries(Object.entries(ids).map(([key,id])=>[key,document.getElementById(id)]));
 const original=!!saved&&!!root?.isConnected&&parent.root===root&&parent.state===state&&!!parent.lease&&parent.lease===state.lease&&!state.composing&&!saved.composing
   &&!!panel?.isConnected&&panel===saved.panel&&modal===parent.modal&&parent.finishPane?.node===panel&&parent.finishPane?.name==='share'
   &&Object.entries(nodes).every(([key,e])=>!!e?.isConnected&&saved[key]===e&&panel.contains(e));
 const epoch=original&&panel.dataset.shareEpoch===String(expected.epoch)&&panel.dataset.shareGeneration===String(expected.generation)
   &&saved.epoch===String(expected.epoch)&&saved.generation===String(expected.generation)&&panel.dataset.shareSelected===expected.selected;
 const options=nodes.select?[...nodes.select.options].map(o=>({id:o.value,label:o.textContent})):[];
 const values=epoch&&nodes.select.value===expected.selected&&nodes.text.value===expected.text&&!nodes.text.validity?.badInput
   &&JSON.stringify(options)===JSON.stringify([{id:'',label:'Choose a conversation'},...expected.options]);
 const controls=Object.fromEntries(Object.entries(nodes).map(([key,e])=>[key,e?{disabled:!!e.disabled,readOnly:!!e.readOnly}:null]));
 const ready=values&&expected.bound===true&&expected.loading===false&&['ready','failed'].includes(expected.opening?.status)
   &&['select','refresh','text','cancel'].every(key=>controls[key].disabled===false&&controls[key].readOnly===false)
   &&controls.send.disabled===(expected.opening.status==='failed');
 return{ok:!!ready,measurement:'passive-original-settled-share-controls',original,epoch,values,controls,
   selected:nodes.select?.value??null,epochActual:panel?.dataset.shareEpoch??null,epochExpected:String(expected.epoch)};
})()
