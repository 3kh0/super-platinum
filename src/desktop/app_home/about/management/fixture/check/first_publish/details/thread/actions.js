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
 if(!hitPoint)return{ok:false,reason:'Original action fully occluded',label,rect:box};
 if(action==='input'){
  e.focus({preventScroll:true});Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value').set.call(e,DRAFT);
  e.dispatchEvent(new Event('input',{bubbles:true}));e.setSelectionRange(7,22,'forward');
  return{ok:true,kind:'dom-input',nodeId:id(e),rect:box,hitPoint,value:e.value,selectionStart:e.selectionStart,selectionEnd:e.selectionEnd,selectionDirection:e.selectionDirection};
 }
 e.click();return{ok:true,kind:'dom-click',label,nodeId:id(e),rect:box,hitPoint,messageId:article?.dataset.messageId??null};
})()
