(()=>{
 const expectedProduct=EXPECTED_PRODUCT;
 const root=document.getElementById('workflow-management'),state=root?.__workflowManagement;
 const saved=state?.permissionsConfirmation,share=saved?.share;
 const modal=document.getElementById('workflow-finish-up'),panel=document.getElementById('workflow-finish-share-pane');
 const registry=window.__firstPublishRegistry;
 const node=e=>e?{id:e.id,nodeId:registry?.ids?.get(e)??null,connected:e.isConnected,tag:e.tagName,
   value:e.value??null,text:e.textContent,disabled:!!e.disabled,readOnly:!!e.readOnly,badInput:e.validity?.badInput??false}:null;
 const ids={select:'workflow-share-destination',text:'workflow-share-message',refresh:'workflow-share-refresh',cancel:'workflow-share-cancel',send:'workflow-share-send'};
 const controls=Object.fromEntries(Object.entries(ids).map(([key,id])=>{
   const e=document.getElementById(id);return[key,{...node(e),sameOriginal:!!e&&e===share?.[key],insidePanel:!!e&&!!panel&&panel.contains(e)}];
 }));
 const select=document.getElementById(ids.select);
 let original=null,originalKeyValid=true;try{original=JSON.parse(saved?.key??'null')}catch{originalKeyValid=false}
 const title=document.getElementById('workflow-management-title'),description=document.getElementById('workflow-management-description');
 const finishTitle=document.getElementById('workflow-finish-title'),finishDescription=document.getElementById('workflow-finish-description'),upload=document.getElementById('workflow-finish-upload');
 const panes=Object.fromEntries(['details','upload','crop','share'].map(key=>{
   const id=key==='details'?'workflow-finish-details':'workflow-finish-'+key+'-pane',e=document.getElementById(id);
   return[key,{...node(e),sameOriginal:!!e&&e===saved?.finishPanes?.[key],display:e?.style?.display??null,computedDisplay:e?getComputedStyle(e).display:null}];
 }));
 return{ok:true,measurement:'passive-original-binding-diagnostic',expectedProduct,
   share:{panel:node(panel),dataset:panel?{...panel.dataset}:null,sameOriginalPanel:!!panel&&panel===share?.panel,
     savedGeneration:share?.generation??null,savedEpoch:share?.epoch??null,savedComposing:share?.composing??null,
     controls,selectedValue:select?.value??null,selectedIndex:select?.selectedIndex??null,
     options:select?[...select.options].map(o=>({value:o.value,text:o.textContent,selected:o.selected,disabled:o.disabled})):[]},
   parent:{root:node(root),dataset:root?{...root.dataset}:null,original,originalKeyValid,
     savedRootSame:!!root&&saved?.root===root,savedStateSame:!!state&&saved?.state===state,
     leasePresent:!!saved?.lease,leaseSame:!!saved?.lease&&state?.lease===saved.lease,composing:state?.composing??null,
     title:node(title),description:node(description),titleSame:!!title&&title===saved?.title,descriptionSame:!!description&&description===saved?.description,
     modal:node(modal),modalDataset:modal?{...modal.dataset}:null,modalSame:!!modal&&modal===saved?.modal,
     dialogs:[...document.querySelectorAll('[role="dialog"][aria-modal="true"]')].map(node),
     finishBinding:saved?.finishBinding?{epoch:saved.finishBinding.epoch,status:saved.finishBinding.status}:null,
     finishBindPromiseKey:saved?.finishBindPromise?.key??null,
     finishPane:saved?.finishPane?{name:saved.finishPane.name,epoch:saved.finishPane.epoch,node:node(saved.finishPane.node),samePanel:saved.finishPane.node===panel}:null,
     finishTitle:node(finishTitle),finishDescription:node(finishDescription),upload:node(upload),
     finishTitleSame:!!finishTitle&&finishTitle===saved?.finish?.title,finishDescriptionSame:!!finishDescription&&finishDescription===saved?.finish?.description,
     uploadSame:!!upload&&upload===saved?.finish?.upload,uploadCount:upload?.files?.length??null,
     uploadSelectionSame:saved?.uploadSelection?upload?.files?.[0]===saved.uploadSelection:null,panes}};
})()
