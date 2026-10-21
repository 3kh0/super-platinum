(()=>{
 const base=BASE,r=window.__firstPublishRegistry;
 const id=e=>{if(!e)return null;if(!r.ids.has(e))r.ids.set(e,r.next++);return r.ids.get(e)};
 const rect=e=>{const v=e.getBoundingClientRect();return{left:v.left,top:v.top,right:v.right,bottom:v.bottom,width:v.width,height:v.height}};
 const paint=e=>{e.scrollIntoView({block:'center',inline:'nearest',behavior:'instant'});const v=rect(e),h=document.elementFromPoint(v.left+v.width/2,v.top+v.height/2);return{id:e.id,nodeId:id(e),tag:e.tagName,text:e.textContent,connected:e.isConnected,rect:v,visible:v.width>0&&v.height>0&&v.left>=0&&v.top>=0&&v.right<=innerWidth+1&&v.bottom<=innerHeight+1&&(h===e||e.contains(h))};};
 const plain=e=>e?{id:e.id,nodeId:id(e),connected:e.isConnected,value:e.value??null,selectionStart:e.selectionStart??null,selectionEnd:e.selectionEnd??null}:null;
 const custody={browse:plain(document.getElementById('channel-workflows-panel')),thread:plain(document.querySelector('.thread-pane')),composer:plain(document.getElementById('channel-composer')),threadComposer:plain(document.getElementById('thread-composer'))};
 const browseNode=document.getElementById('channel-workflows-panel');
 const detailsLayout={browse:browseNode?rect(browseNode):null,pane:null};
 const pane=document.getElementById('channel-workflow-details'),origin=window.__workflowDetailsOrigin;
 if(!pane){
  const header=document.getElementById('channel-details-trigger'),timeline=document.getElementById('message-timeline'),composer=document.getElementById('channel-composer'),active=document.querySelector('.sidebar button.channel.active .channel-name');
  const successor=!browseNode&&!document.getElementById('workflow-management')&&!document.getElementById('workflow-finish-up')&&header?.textContent==='#private-review'&&active?.textContent==='private-review'&&header.isConnected&&active.isConnected&&timeline?.isConnected&&composer?.isConnected;
  const channelSuccessor=successor?{header:paint(header),timeline:plain(timeline),composer:plain(composer),active:plain(active),name:active.textContent,modalAbsent:true}:null;
  return{...base,ok:base.ok||!!channelSuccessor&&channelSuccessor.header.visible&&base.noOverflow,details:null,detailsLayout,detailsCustody:custody,detailsFocus:document.activeElement?.id??null,channelSuccessor};
 }
 detailsLayout.pane={rect:rect(pane),position:getComputedStyle(pane).position};
 let product;try{product=JSON.parse(pane.dataset.fixtureReadback)}catch{return{...base,ok:false,details:{error:'Missing original details readback'},detailsCustody:custody}}
 const controls=[...pane.querySelectorAll('button')].map(e=>({...paint(e),disabled:e.disabled}));
 const content=['channel-workflow-details-title','channel-workflow-details-description','channel-workflow-details-error-title','channel-workflow-details-error-message','channel-workflow-details-loading'].map(k=>document.getElementById(k)).filter(Boolean).map(paint);
 const feedback=[...pane.querySelectorAll('[role=status],[role=alert]')].map(paint);
 const steps=[...pane.querySelectorAll('.channel-workflow-details-steps li')].map(paint);
 const headings=[...pane.querySelectorAll('h2,h3')].map(e=>e.textContent);
 const original=!!origin&&origin.pane===pane&&origin.panel===document.getElementById('channel-workflows-panel')&&origin.button===document.getElementById(origin.button?.id)&&origin.id===pane.dataset.selection;
 const value={pane:{id:pane.id,nodeId:id(pane),connected:pane.isConnected,role:pane.getAttribute('role'),ariaLabel:pane.getAttribute('aria-label')},product,controls,content,feedback,steps,headings,originalNodes:original,opener:plain(origin?.button),brokenImages:[...pane.querySelectorAll('img')].filter(e=>!e.complete||e.naturalWidth===0).length};
 return{...base,ok:base.ok&&original&&product.current===true&&controls.every(e=>e.visible)&&content.every(e=>e.visible)&&feedback.every(e=>e.visible)&&steps.every(e=>e.visible)&&value.brokenImages===0,details:value,detailsLayout,detailsCustody:custody,detailsFocus:document.activeElement?.id??null};
})()
