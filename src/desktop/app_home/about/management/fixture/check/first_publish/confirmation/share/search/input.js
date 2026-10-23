(()=>{
 const value=QUERY,r=window.__firstPublishRegistry,root=document.getElementById('workflow-management'),state=root?.__workflowManagement;
 const parent=state?.permissionsConfirmation,saved=parent?.share,e=document.getElementById('workflow-share-query');
 if(!e?.isConnected||e.disabled||e.readOnly||!saved||saved.query!==e||parent.state!==state||parent.root!==root||parent.lease!==state.lease||state.composing||saved.composing)return{ok:false,reason:'original query unavailable'};
 e.scrollIntoView({block:'center',inline:'nearest',behavior:'instant'});const q=e.getBoundingClientRect(),x=q.left+q.width/2,y=q.top+q.height/2,h=document.elementFromPoint(x,y);
 if(q.width<=0||q.height<=0||q.left<0||q.top<0||q.right>innerWidth+1||q.bottom>innerHeight+1||(h!==e&&!e.contains(h)))return{ok:false,reason:'query not exposed'};
 if(!r.ids.has(e))r.ids.set(e,r.next++);const nodeId=r.ids.get(e);let event=null;
 const listener=v=>{event={type:v.type,isTrusted:v.isTrusted,targetNodeId:r.ids.get(v.target)}};e.addEventListener('input',listener,{once:true});
 e.focus();e.value=value;e.setSelectionRange(value.length,value.length);e.dispatchEvent(new Event('input',{bubbles:true}));e.removeEventListener('input',listener);
 return{ok:!!event,kind:'query',nodeId,id:e.id,value:e.value,start:e.selectionStart,end:e.selectionEnd,event,originalNode:saved.query===e,hitPoint:{x,y},rect:{left:q.left,top:q.top,right:q.right,bottom:q.bottom,width:q.width,height:q.height}};
})()
