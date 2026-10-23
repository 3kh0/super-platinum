async function workflowShareEntryBind(original,local){
 if(!await workflowFinishUpBind(original,local))return false;
 const root=document.getElementById('workflow-management'),saved=root?.__workflowManagement?.permissionsConfirmation;
 const entry=root?.querySelector('#workflow-confirmation-share-open');
 if(!entry?.isConnected||entry.tagName!=='BUTTON'||saved.shareEntry&&saved.shareEntry!==entry)return false;
 saved.shareEntry=entry;return true;
}
function workflowShareEntry(original,local){
 if(local.pane!=='details'||!workflowFinishUp(original,local,null))return false;
 const root=document.getElementById('workflow-management'),saved=root?.__workflowManagement?.permissionsConfirmation;
 const entry=root?.querySelector('#workflow-confirmation-share-open');
 if(!entry||entry.tagName!=='BUTTON'||!entry.isConnected||entry.disabled||entry.textContent!=='Share'||!entry.getClientRects().length)return false;
 return saved.shareEntry===entry;
}
function workflowShareNodes(original,local,expected){
 if(local.pane!=='share'||!workflowFinishUp(original,local,null))return null;
 const root=document.getElementById('workflow-management'),saved=root?.__workflowManagement?.permissionsConfirmation;
 const panel=root.querySelector('#workflow-finish-share-pane');
 if(!panel||panel.dataset.shareGeneration!==String(expected.generation)||panel.dataset.shareEpoch!==String(expected.epoch)||panel.dataset.shareSelected!==expected.selected||panel.dataset.shareQueryRevision!==String(expected.queryRevision))return null;
 const ids={query:'workflow-share-query',select:'workflow-share-destination',text:'workflow-share-message',refresh:'workflow-share-refresh',cancel:'workflow-share-cancel',send:'workflow-share-send'};
 const nodes={root,saved,panel};
 for(const[k,id]of Object.entries(ids)){const n=root.querySelector('#'+id);if(!n?.isConnected||!panel.contains(n))return null;nodes[k]=n;}
 if(nodes.query.tagName!=='INPUT'||nodes.query.type!=='search'||nodes.query.validity?.badInput||nodes.select.tagName!=='SELECT'||nodes.text.tagName!=='TEXTAREA'||nodes.text.validity?.badInput)return null;
 const rows=[{id:'',label:'Choose a conversation'},...expected.rows];
 if(nodes.select.options.length!==rows.length||rows.some((r,i)=>nodes.select.options[i].value!==r.id||nodes.select.options[i].textContent!==r.label))return null;
 if(nodes.cancel.textContent!=='Cancel'||nodes.send.textContent!=='Send'||nodes.refresh.textContent!=='Refresh destinations')return null;
 const recipients=expected.recipients||[];
 nodes.tokens=Array.from(root.querySelectorAll?.('[data-workflow-share-recipient]')||[]);
 nodes.removes=Array.from(root.querySelectorAll?.('[data-workflow-share-recipient-remove]')||[]);
 if(nodes.tokens.length!==recipients.length||nodes.removes.length!==recipients.length)return null;
 if(recipients.some((r,i)=>!nodes.tokens[i].isConnected||!nodes.removes[i].isConnected||!panel.contains(nodes.tokens[i])||nodes.tokens[i].dataset.workflowShareRecipient!==r.id||nodes.removes[i].dataset.workflowShareRecipientRemove!==r.id||!nodes.tokens[i].contains(nodes.removes[i])||nodes.removes[i].tagName!=='BUTTON'||nodes.removes[i].getAttribute('aria-label')!==`Remove ${r.label}`||nodes.tokens[i].dataset.selected!==String(expected.selectedRecipient===i)))return null;
 if(expected.recipientRevision!==undefined&&panel.dataset.recipientRevision!==String(expected.recipientRevision))return null;
 return nodes;
}
async function workflowShareBind(original,local,expected){
 if(!await workflowFinishUpBind(original,local))return false;
 const root=document.getElementById('workflow-management'),saved=root?.__workflowManagement?.permissionsConfirmation;
 const prior=saved?.share;
 for(let frame=0;frame<100;frame++){
  if(root!==document.getElementById('workflow-management')||saved!==root?.__workflowManagement?.permissionsConfirmation||!workflowFinishUp(original,local,null))return false;
  const nodes=workflowShareNodes(original,local,expected);
  if(nodes){
   if(prior&&['panel','query','text','select','send','cancel','refresh'].some(k=>prior[k]!==nodes[k]))return false;
   for(const [i,r]of(expected.recipients||[]).entries()){
    const old=prior?.expected?.recipients?.findIndex(v=>v.id===r.id)??-1;
    if(old>=0&&(prior.tokens[old]!==nodes.tokens[i]||prior.removes[old]!==nodes.removes[i]))return false;
   }
   for(const [i,r]of(prior?.expected?.recipients||[]).entries()){if(!(expected.recipients||[]).some(v=>v.id===r.id)&&(prior.tokens[i].isConnected||prior.removes[i].isConnected))return false;}
   if(nodes.text.value!==expected.text||nodes.select.value!==expected.selected||prior?.composing)return false;
   const fresh=!prior||prior.generation!==String(expected.generation);
   if(fresh)nodes.query.value=expected.query;
   // A pending query may be ahead of its debounced model. Bind only original
   // nodes here; Current still requires its exact value on every read/action.
   const next={...nodes,expected,original,local,generation:String(expected.generation),epoch:String(expected.epoch),composing:false,queryComposing:fresh?false:prior.queryComposing};
   if(!prior){
    nodes.text.addEventListener('compositionstart',()=>{if(saved.share)saved.share.composing=true});
    nodes.text.addEventListener('compositionend',()=>{if(saved.share)saved.share.composing=false});
    nodes.query.addEventListener('compositionstart',()=>{if(saved.share)saved.share.queryComposing=true});
    nodes.query.addEventListener('compositionend',()=>{if(saved.share)saved.share.queryComposing=false;nodes.query.dispatchEvent(new Event('input',{bubbles:true}));});
   }
   for(const remove of nodes.removes){
    if(remove.__workflowShareRecipientBound)continue;
    remove.__workflowShareRecipientBound=true;
    remove.addEventListener('mousedown',event=>{
     const now=saved.share;
     if(!now||now.composing||now.queryComposing||event.button!==0||remove.disabled||!now.removes.includes(remove)||!workflowShareCurrent(now.original,now.local,now.expected,null))return;
     event.preventDefault();now.query.focus({preventScroll:true});
     now.recipientEvent={kind:'recipientRemove',value:remove.dataset.workflowShareRecipientRemove,node:remove,epoch:now.epoch};
    },true);
   }
   if(!prior)nodes.query.addEventListener('keydown',event=>{
    const now=saved.share;if(!now||now.query.readOnly||now.composing||now.queryComposing||event.isComposing||event.repeat||!['Backspace','Delete'].includes(event.key)||document.activeElement!==now.query||now.query.value!==''||!now.removes.length||now.removes.some(n=>n.disabled)||!workflowShareCurrent(now.original,now.local,now.expected,null))return;
    if(event.key==='Delete'&&now.expected.selectedRecipient==null)return;
    event.preventDefault();now.recipientEvent={kind:'recipientKey',value:event.key,node:now.query,epoch:now.epoch};
   },true);
   saved.share=next;if(fresh)nodes.query.focus({preventScroll:true});return true;
  }
  await new Promise(resolve=>requestAnimationFrame(resolve));
 }
 return false;
}
function workflowShareCurrent(original,local,expected,action=null){
 const nodes=workflowShareNodes(original,local,expected),saved=nodes?.saved.share;
 if(!nodes||!saved||nodes.tokens.some((n,i)=>n!==saved.tokens[i]||nodes.removes[i]!==saved.removes[i])||saved.generation!==String(expected.generation)||saved.epoch!==String(expected.epoch)||saved.composing||(saved.queryComposing&&action?.kind!=='cancel')||['panel','query','text','select','send','cancel','refresh'].some(k=>nodes[k]!==saved[k]))return false;
 const text=action?.kind==='text'?action.value:expected.text,selected=action?.kind==='select'?action.value:expected.selected;
 if(nodes.text.value!==text||nodes.select.value!==selected)return false;
 if(action?.kind!=='cancel'&&nodes.query.value!==(action?.kind==='query'?action.value:expected.query))return false;
 if(action?.kind==='select'&&!['',...expected.rows.map(r=>r.id)].includes(selected))return false;
 if(action?.kind==='recipientRemove'||action?.kind==='recipientKey'){
  const proof=saved.recipientEvent;
  if(!proof||proof.kind!==action.kind||proof.value!==action.value||proof.epoch!==saved.epoch)return false;
  const target=action.kind==='recipientKey'?nodes.query:nodes.removes.find(n=>n.dataset.workflowShareRecipientRemove===action.value);
  if(proof.node!==target||!target?.isConnected||target.disabled||!target.getClientRects().length)return false;
  saved.recipientEvent=null;return true;
 }
 if(action){const target=nodes[action.kind];if(!target||target.disabled||target.readOnly||target.hidden||!target.getClientRects().length||getComputedStyle(target).display==='none'||getComputedStyle(target).visibility==='hidden')return false;}
 if(action?.kind==='select'&&action.clearQuery===true)nodes.query.value='';
 if(action?.kind==='select'&&action.restoreSelection===true)nodes.select.value=expected.selected;
 return true;
}
