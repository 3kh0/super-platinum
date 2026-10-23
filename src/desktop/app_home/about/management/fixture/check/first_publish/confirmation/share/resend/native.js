(()=>{
 const mode=MODE,expected=EXPECTED,r=window.__firstPublishRegistry;
 if(!r?.ids)return{ok:false,reason:'original registry absent'};
 GEOMETRY
 const id=e=>{if(!e)return null;if(!r.ids.has(e))r.ids.set(e,r.next++);return r.ids.get(e)};
 const timeline=document.getElementById('message-timeline'),rows=timeline?[...timeline.querySelectorAll('article[data-message-id]')]:[];
 // Only bind on the actual failed destination, never original draft history.
 const destination=document.getElementById('channel-details-trigger')?.textContent==='#review-destination';
 if(!destination)return mode==='observe'?{ok:true,row:null,original:null}:{ok:false,reason:'not destination'};
 if(rows.length!==1)return{ok:false,reason:'exact original destination row absent'};
 const row=rows[0],retry=row.querySelector('[data-workflow-share-resend]'),cancel=row.querySelector('[data-workflow-share-cancel]');
 if(!r.shareResend&&(retry||cancel))r.shareResend={window,document,timeline,row,retry,cancel};
 const saved=r.shareResend;
 const paint=e=>{if(!e)return null;e.scrollIntoView({block:'center',inline:'nearest',behavior:'instant'});const m=measure(e);return{nodeId:id(e),connected:e.isConnected,text:e.textContent,label:e.getAttribute('aria-label'),disabled:!!e.disabled,rect:m.rect,hitPoint:m.hitPoint,displayed:m.displayed}};
 const proof={nodeId:id(row),messageId:row.dataset.messageId,connected:row.isConnected,text:row.textContent,status:row.querySelector('.pending-label')?.textContent??null,links:[...row.querySelectorAll('a[href]')].map(a=>({text:a.textContent,href:a.getAttribute('href')})),resend:paint(retry),cancel:paint(cancel)};
 const original=saved?{rowNodeId:id(saved.row),resendNodeId:id(saved.retry),cancelNodeId:id(saved.cancel),rowConnected:saved.row.isConnected,resendConnected:!!saved.retry?.isConnected,cancelConnected:!!saved.cancel?.isConnected}:null;
 if(mode==='observe')return{ok:true,row:proof,original};
 const same=saved&&saved.window===window&&saved.document===document&&saved.timeline===timeline&&saved.row===row&&saved.retry===retry;
 const p=proof.resend;
 if(mode!=='click'||!same||!row.isConnected||!p||!p.connected||p.disabled||!p.displayed||!p.hitPoint||retry.dataset.workflowShareResend!==row.dataset.messageId||p.text.trim()!=='Try again'||p.label!=='Retry failed workflow message'||expected.nodeId!==proof.nodeId||expected.messageId!==proof.messageId||expected.resend.nodeId!==p.nodeId)return{ok:false,reason:'original exposed Try again changed'};
 const h=document.elementFromPoint(p.hitPoint.x,p.hitPoint.y);
 if(h!==retry&&!retry.contains(h))return{ok:false,reason:'Try again hitpoint changed'};
 let event=null;const listener=e=>{event={type:e.type,isTrusted:e.isTrusted,targetNodeId:id(e.target)}};
 retry.focus();retry.addEventListener('click',listener,{once:true});retry.click();retry.removeEventListener('click',listener);
 return{ok:!!event,kind:'click',provenance:'synthetic-dom-click',localTs:proof.messageId,rowNodeId:proof.nodeId,nodeId:p.nodeId,rect:p.rect,hitPoint:p.hitPoint,event,originalNodes:same};
})()
