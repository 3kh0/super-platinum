(()=>{
 const action=ACTION,label=LABEL,r=window.__firstPublishRegistry,expected=EXPECTED;
 const id=e=>{if(!r.ids.has(e))r.ids.set(e,r.next++);return r.ids.get(e)};
 const rect=e=>{const v=e.getBoundingClientRect();return{left:v.left,top:v.top,right:v.right,bottom:v.bottom,width:v.width,height:v.height}};
 let e,article;
 if(action==='reply'){
  article=document.querySelector('#message-timeline article[data-message-id="1781524900.000001"]');
  if(!article?.isConnected)return{ok:false,reason:'Original history parent absent'};
  article.scrollIntoView({block:'center',inline:'nearest',behavior:'instant'});article.focus({preventScroll:true});
  e=[...article.querySelectorAll('.message-tools button')].find(e=>e.textContent.trim()==='Reply');
 }else if(action==='tab')e=[...document.querySelectorAll('.channel-tabs button')].find(e=>e.textContent.trim()===label);
 else if(action==='input')e=document.getElementById('thread-composer');
 if(!e?.isConnected||e.disabled||(action==='input'&&(e.readOnly||id(e)!==expected||!e.dataset.projectionVisit||!e.dataset.draftScope)))return{ok:false,reason:'Original enabled action absent'};
 e.scrollIntoView({block:'nearest',inline:'start',behavior:'instant'});
 const box=rect(e);let hitPoint=null;
 for(const fy of [.5,.2,.8])for(const fx of [.5,.1,.9,.02,.98]){
  const x=box.left+box.width*fx,y=box.top+box.height*fy;
  if(x<0||x>=innerWidth||y<0||y>=innerHeight)continue;
  const hit=document.elementFromPoint(x,y);if(hit===e||e.contains(hit)){hitPoint={x,y};break}
 }
 if(!hitPoint){
  if(action!=='tab'||label!=='Workflows')return{ok:false,reason:'Original action fully occluded',label,rect:box};
  if(window.__firstPublishThreadKeyboard)return{ok:false,reason:'Original keyboard request already exists'};
  e.focus({preventScroll:true});
  if(document.activeElement!==e||!e.isConnected||e.disabled)return{ok:false,reason:'Original tab focus refused'};
  const pending={element:e,nodeId:id(e),rect:box,key:'Enter',label,keydown:null,click:null,failure:null};
  const original=()=>e.isConnected&&!e.disabled&&[...document.querySelectorAll('.channel-tabs button')].find(n=>n.textContent.trim()==='Workflows')===e&&document.activeElement===e;
  const stop=event=>{event.preventDefault();event.stopImmediatePropagation()};
  const keydown=event=>{
   if(pending.failure||pending.keydown||event.key!=='Enter'||event.code!=='Enter'||event.repeat||!event.isTrusted||event.target!==e||!original()){
    stop(event);pending.failure='Original trusted Enter refused';return;
   }
   pending.keydown={trusted:event.isTrusted,key:event.key,code:event.code,repeat:event.repeat,targetNodeId:id(event.target),focused:document.activeElement===e,connected:e.isConnected,enabled:!e.disabled};
  };
  const click=event=>{
   if(pending.failure||!pending.keydown||pending.click||!event.isTrusted||event.target!==e||event.detail!==0||!original()){
    stop(event);pending.failure='Original trusted tab click refused';return;
   }
   pending.click={trusted:event.isTrusted,targetNodeId:id(event.target),detail:event.detail,connected:e.isConnected,enabled:!e.disabled};
   document.removeEventListener('keydown',keydown,true);document.removeEventListener('click',click,true);
  };
  window.__firstPublishThreadKeyboard=pending;
  document.addEventListener('keydown',keydown,true);document.addEventListener('click',click,true);
  return{ok:true,kind:'keyboard-request',label,nodeId:pending.nodeId,rect:box,key:'Enter',hitPoint:null};
 }
 if(action==='input'){
  e.focus({preventScroll:true});Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value').set.call(e,DRAFT);
  e.dispatchEvent(new Event('input',{bubbles:true}));e.setSelectionRange(7,22,'forward');
  return{ok:true,kind:'dom-input',nodeId:id(e),rect:box,hitPoint,value:e.value,selectionStart:e.selectionStart,selectionEnd:e.selectionEnd,selectionDirection:e.selectionDirection};
 }
 e.click();return{ok:true,kind:'dom-click',label,nodeId:id(e),rect:box,hitPoint,messageId:article?.dataset.messageId??null};
})()
