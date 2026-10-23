(()=>{
 const kind=KIND,label=LABEL,value=VALUE,expected=EXPECTED,root=ROOT,r=window.__firstPublishRegistry;
 if(!r)return{ok:false,reason:'Draft registry absent'};
 const id=e=>{if(!r.ids.has(e))r.ids.set(e,r.next++);return r.ids.get(e)};
 GEOMETRY
 let e,article=null;
 if(kind==='tab')e=[...document.querySelectorAll('.channel-tabs button')].find(e=>e.textContent.trim()===label);
 else if(kind==='sidebar')e=[...document.querySelectorAll('.sidebar button.channel')].find(e=>e.querySelector('.channel-name')?.textContent==='Review requests');
 else if(kind==='reply'){article=document.querySelector('#message-timeline article[data-message-id="'+root+'"]');if(!article?.isConnected)return{ok:false,reason:'Original parent absent'};article.scrollIntoView({block:'center',behavior:'instant'});article.focus({preventScroll:true});e=[...article.querySelectorAll('.message-tools button')].find(e=>e.textContent.trim()==='Reply');}
 else if(kind==='input')e=document.getElementById(label);
 else if(kind==='metadata'&&['workflow-management-title','workflow-management-description'].includes(label))e=document.getElementById(label);
 else if(kind==='button'&&label==='Finish Up')e=document.getElementById('workflow-management-finish-up');
 if(!e?.isConnected||e.disabled||kind==='input'&&(e.readOnly||id(e)!==expected||!e.dataset.draftScope||!e.dataset.projectionVisit))return{ok:false,reason:'Original enabled node absent'};
 if(kind==='metadata'&&(e.readOnly||!r.parent||r.parent.root!==document.getElementById('workflow-management')||![r.parent.title,r.parent.description].includes(e)))return{ok:false,reason:'Original metadata field absent'};
 e.scrollIntoView({block:'nearest',inline:'start',behavior:'instant'});const exposure=measure(e),q=exposure.rect,hitPoint=exposure.hitPoint;
 if(!exposure.displayed||q.width<=0||q.height<=0)return{ok:false,reason:'Original draft action hidden',kind,label};
 if(!hitPoint){
  if(!exposure.coveredByThread||!((kind==='button'&&label==='Finish Up')||(kind==='tab'&&label==='Messages')))return{ok:false,reason:'Original draft action occluded',kind,label,exposure};
  if(window.__firstPublishShareKeyboard&&!window.__firstPublishShareKeyboard.click)return{ok:false,reason:'Original keyboard request pending'};
  e.focus({preventScroll:true});
  const lookup=()=>kind==='button'?document.getElementById('workflow-management-finish-up'):[...document.querySelectorAll('.channel-tabs button')].find(n=>n.textContent.trim()==='Messages');
  const original=()=>e.isConnected&&!e.disabled&&lookup()===e&&document.activeElement===e&&measure(e).originalThread;
  if(!original())return{ok:false,reason:'Original Share keyboard focus refused'};
  const pending={element:e,nodeId:id(e),rect:q,key:'Enter',label,keydown:null,click:null,failure:null};
  const stop=event=>{event.preventDefault();event.stopImmediatePropagation()};
  const keydown=event=>{
   if(pending.failure||pending.keydown||event.key!=='Enter'||event.code!=='Enter'||event.repeat||!event.isTrusted||event.target!==e||!original()){stop(event);pending.failure='Original trusted Enter refused';return;}
   pending.keydown={trusted:true,key:event.key,code:event.code,repeat:event.repeat,targetNodeId:id(event.target),focused:true,connected:e.isConnected,enabled:!e.disabled};
  };
  const click=event=>{
   if(pending.failure||!pending.keydown||pending.click||!event.isTrusted||event.target!==e||event.detail!==0||!original()){stop(event);pending.failure='Original trusted Share click refused';return;}
   pending.click={trusted:true,targetNodeId:id(event.target),detail:event.detail,connected:e.isConnected,enabled:!e.disabled};
   document.removeEventListener('keydown',keydown,true);document.removeEventListener('click',click,true);
  };
  window.__firstPublishShareKeyboard=pending;document.addEventListener('keydown',keydown,true);document.addEventListener('click',click,true);
  return{ok:true,kind:'keyboard-request',label,nodeId:pending.nodeId,rect:q,key:'Enter',hitPoint:null,exposure};
 }
 e.focus({preventScroll:true});let event=null;const editing=kind==='input'||kind==='metadata',type=editing?'input':'click',listener=v=>{event={type:v.type,isTrusted:v.isTrusted,targetNodeId:id(v.target)}};e.addEventListener(type,listener,{once:true});
 if(editing){Object.getOwnPropertyDescriptor(e.tagName==='TEXTAREA'?HTMLTextAreaElement.prototype:HTMLInputElement.prototype,'value').set.call(e,value);if(kind==='input')e.setSelectionRange(7,22,'forward');e.dispatchEvent(new Event('input',{bubbles:true}));}else e.click();
 e.removeEventListener(type,listener);return{ok:!!event,kind:editing?'dom-input':'dom-click',label,nodeId:id(e),event,hitPoint,rect:q,exposure,value:e.value??null,selectionStart:e.selectionStart??null,selectionEnd:e.selectionEnd??null,selectionDirection:e.selectionDirection??null,messageId:article?.dataset.messageId??null};
})()
