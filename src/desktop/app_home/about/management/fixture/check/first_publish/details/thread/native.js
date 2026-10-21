(()=>{
 const base=BASE,r=window.__firstPublishRegistry;
 const id=e=>{if(!e)return null;if(!r.ids.has(e))r.ids.set(e,r.next++);return r.ids.get(e)};
 const rect=e=>{const v=e.getBoundingClientRect();return{left:v.left,top:v.top,right:v.right,bottom:v.bottom,width:v.width,height:v.height}};
 const node=e=>{if(!e)return null;const style=getComputedStyle(e);return{id:e.id,nodeId:id(e),connected:e.isConnected,display:style.display,displayed:e.isConnected&&e.getClientRects().length>0&&style.display!=='none'&&style.visibility!=='hidden',rect:rect(e)}};
 const pane=document.querySelector('.thread-pane'),body=document.getElementById('thread-body'),composer=document.getElementById('thread-composer');
 if(BIND){
  if(r.detailsThread||!pane?.isConnected||!body?.isConnected||!composer?.isConnected||!pane.contains(body)||!pane.contains(composer)||composer.value!==DRAFT||composer.selectionStart!==7||composer.selectionEnd!==22)return{...base,ok:false,detailsThread:{error:'Original typed thread binding refused'}};
  r.detailsThread={pane,body,composer};
 }
 const saved=r.detailsThread;
 const originalNodes=!!saved&&saved.pane===pane&&saved.body===body&&saved.composer===composer&&!!pane?.isConnected&&!!body?.isConnected&&!!composer?.isConnected;
 const savedDetached=!!saved&&!saved.pane.isConnected&&!saved.body.isConnected&&!saved.composer.isConnected;
 const value={pane:node(pane),body:body?{...node(body),text:body.textContent,rows:[...body.querySelectorAll('article[data-message-id]')].map(e=>({id:e.dataset.messageId,nodeId:id(e),text:e.textContent}))}:null,
  composer:composer?{...node(composer),value:composer.value,selectionStart:composer.selectionStart,selectionEnd:composer.selectionEnd,selectionDirection:composer.selectionDirection,draftScope:composer.dataset.draftScope,projectionVisit:composer.dataset.projectionVisit,readonly:composer.readOnly,disabled:composer.disabled}:null,
  originalNodes,savedDetached};
 return{...base,ok:base.ok&&(!saved||originalNodes||savedDetached&&!pane&&!body&&!composer),detailsThread:value};
})()
