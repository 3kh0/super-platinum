(()=>{
 const kind=KIND,peer=PEER,root=document.getElementById('workflow-management'),state=root?.__workflowManagement,p=state?.permissionsConfirmation,s=p?.share,r=window.__firstPublishRegistry;
 const panel=document.getElementById('workflow-finish-share-pane'),query=document.getElementById('workflow-share-query');
 if(!root?.isConnected||!s||state.composing||s.composing||s.queryComposing||s.panel!==panel||s.query!==query||!query?.isConnected||query.disabled||(kind!=='remove'&&query.readOnly)||panel.dataset.shareEpoch!==s.epoch||panel.dataset.shareGeneration!==s.generation)return{ok:false,reason:'original current query required'};
 const index=s.expected.recipients.findIndex(v=>v.id===peer),node=kind==='remove'?s.removes[index]:query;
 if(!node?.isConnected||node.disabled||!panel.contains(node)||kind==='remove'&&(node.dataset.workflowShareRecipientRemove!==peer||s.tokens[index]?.dataset.workflowShareRecipient!==peer)||kind!=='remove'&&query.value!=='')return{ok:false,reason:'original token/query required'};
 node.scrollIntoView({block:'center',inline:'nearest',behavior:'instant'});const q=node.getBoundingClientRect(),x=(q.left+q.right)/2,y=(q.top+q.bottom)/2,h=document.elementFromPoint(x,y);
 if(q.width<=0||q.height<=0||q.left<0||q.top<0||q.right>innerWidth+1||q.bottom>innerHeight+1||h!==node&&!node.contains(h))return{ok:false,reason:'not exposed'};
 const id=e=>{if(!r.ids.has(e))r.ids.set(e,r.next++);return r.ids.get(e)},nodeId=id(node),before={epoch:s.epoch,recipients:s.expected.recipients,selectedRecipient:s.expected.selectedRecipient,query:query.value};
 query.focus();query.setSelectionRange(0,0);let event=null;const type=kind==='remove'?'mousedown':'keydown',capture=e=>{event={type:e.type,key:e.key??null,button:e.button??null,isTrusted:e.isTrusted,targetNodeId:id(e.target)}};node.addEventListener(type,capture,{once:true});
 const input=kind==='remove'?new MouseEvent(type,{bubbles:true,cancelable:true,button:0}):new KeyboardEvent(type,{bubbles:true,cancelable:true,key:kind,repeat:false,isComposing:false});node.dispatchEvent(input);node.removeEventListener(type,capture);
 return{ok:!!event&&input.defaultPrevented,kind,peer,nodeId,before,event,defaultPrevented:input.defaultPrevented,queryFocused:document.activeElement===query,hitPoint:{x,y},rect:{left:q.left,top:q.top,right:q.right,bottom:q.bottom,width:q.width,height:q.height}};
})()
