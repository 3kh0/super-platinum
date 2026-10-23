(()=>{
 const b=BASE,r=window.__firstPublishRegistry,mode=MODE;
 const roles=['parent','title','description','modal','modalTitle','modalDescription','upload'];
 const refs=()=>[r?.parent?.root,r?.parent?.title,r?.parent?.description,r?.modal?.root,r?.modal?.title,r?.modal?.description,r?.modal?.upload];
 const drafts=b.shareDrafts;
 const draftCurrent=()=>drafts?.ok===true&&drafts.windowOriginal===true&&drafts.sameThreadNodes===true&&drafts.threadOriginalConnected===true&&r?.shareDrafts?.window===window&&r.shareDrafts.document===document;
 if(mode==='arm'){
  const nodes=refs();
  if(r?.startupRetirement||b.ok!==true||b.originalNodes!==true||b.modal?.originalNodes!==true||!draftCurrent()||nodes.some(n=>!n?.isConnected)||new Set(nodes).size!==7)return{ok:false,reason:'Original startup retirement binding refused'};
  const nodeIds=Object.fromEntries(nodes.map((n,i)=>{if(!r.ids.has(n))r.ids.set(n,r.next++);return[roles[i],r.ids.get(n)]}));
  const proof={ok:true,kind:'original-startup-retirement-binding',scope:b.scope,modalOwner:b.modal.owner,modalEpoch:b.modal.epoch,nodeIds,windowOriginal:true};
  r.startupRetirement={window,document,parent:r.parent,modal:r.modal,drafts:r.shareDrafts,nodes,proof};
  return proof;
 }
 const old=r?.startupRetirement;
 if(!old||b.present===true)return b;
 const nodes=old.nodes.map((n,i)=>({role:roles[i],nodeId:r.ids.get(n)??null,connected:n?.isConnected===true}));
 const registryOriginal=r.parent===old.parent&&r.modal===old.modal&&r.shareDrafts===old.drafts&&refs().every((n,i)=>n===old.nodes[i])&&nodes.every(n=>n.nodeId===old.proof.nodeIds[n.role]);
 const windowOriginal=old.window===window&&old.document===document;
 const noReplacement=!document.querySelector('[id^="workflow-management"],[id^="workflow-finish"],[id^="workflow-share"],#workflow-configuration');
 const proof={kind:'original-startup-management-retirement',originalObservationOk:b.ok,binding:old.proof,nodes,registryOriginal,windowOriginal,noReplacement,shareEntryAbsent:!document.getElementById('workflow-confirmation-share-open'),actionDispatched:false};
 const ok=b.ok===false&&b.present===false&&b.originalDetached===true&&b.originalNodes===null&&b.leasePresent===false&&b.scope===null&&b.fields===null&&b.modal===null&&b.share===null&&b.noOverflow===true&&b.composing===false&&['controls','feedback','outcomes'].every(k=>Array.isArray(b[k])&&b[k].length===0)&&nodes.length===7&&nodes.every(n=>!n.connected)&&registryOriginal&&windowOriginal&&noReplacement&&proof.shareEntryAbsent&&draftCurrent();
 return{...b,ok,startupRetirement:proof};
})()
