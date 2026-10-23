(()=>{
 const mode=MODE,expected=EXPECTED,r=window.__firstPublishRegistry;
 if(!r?.ids)return{ok:false,reason:'original registry absent'};
 GEOMETRY
 const id=e=>{if(!r.ids.has(e))r.ids.set(e,r.next++);return r.ids.get(e)};
 const timeline=document.getElementById('message-timeline');
 const buttons=timeline?[...timeline.querySelectorAll('[data-workflow-share-cancel]')]:[];
 if(buttons.length>1)return{ok:false,reason:'ambiguous failed Share rows'};
 const button=buttons[0]??null,row=button?.closest('article[data-message-id]')??null;
 const old=r.shareCancel;
 if(mode==='removed')return{ok:!!old&&!old.row.isConnected&&!old.button.isConnected&&!button&&!timeline?.querySelector(`article[data-message-id="${expected.localTs}"]`)};
 if(button&&(!row?.isConnected||!timeline.contains(row)||button.dataset.workflowShareCancel!==row.dataset.messageId))return{ok:false,reason:'failed row association changed'};
 if(button&&!old)r.shareCancel={window,document,timeline,row,button};
 const saved=r.shareCancel,original=saved?{rowNodeId:id(saved.row),cancelNodeId:id(saved.button),rowConnected:saved.row.isConnected,cancelConnected:saved.button.isConnected}:null;
 if(!button)return mode==='observe'?{ok:true,row:null,original}:{ok:false,reason:'original Cancel absent'};
 const same=saved.window===window&&saved.document===document&&saved.timeline===timeline&&saved.row===row&&saved.button===button;
 button.scrollIntoView({block:'center',inline:'nearest',behavior:'instant'});
 const geometry=measure(button),nodeId=id(button),rowNodeId=id(row),localTs=row.dataset.messageId;
 const proof={row:{nodeId:rowNodeId,messageId:localTs,connected:row.isConnected,text:row.textContent,status:row.querySelector('.pending-label')?.textContent??null,links:[...row.querySelectorAll('a[href]')].map(a=>({text:a.textContent,href:a.getAttribute('href')}))},cancel:{nodeId,id:button.id,connected:button.isConnected,text:button.textContent,label:button.getAttribute('aria-label'),disabled:button.disabled,rect:geometry.rect,hitPoint:geometry.hitPoint,displayed:geometry.displayed},originalNodes:same};
 const valid=same&&!button.disabled&&geometry.displayed&&!!geometry.hitPoint&&button.textContent.trim()==='Cancel'&&button.getAttribute('aria-label')==='Cancel failed workflow message';
 if(mode==='observe')return{ok:valid,row:proof,original};
 if(mode!=='click'||!valid||expected.row.nodeId!==rowNodeId||expected.row.messageId!==localTs||expected.cancel.nodeId!==nodeId)return{ok:false,reason:'original exposed Cancel changed'};
 const p=geometry.hitPoint,h=document.elementFromPoint(p.x,p.y);
 if(h!==button&&!button.contains(h))return{ok:false,reason:'Cancel hitpoint changed'};
 let event=null;const listener=e=>{event={type:e.type,isTrusted:e.isTrusted,targetNodeId:id(e.target)}};
 button.focus();button.addEventListener('click',listener,{once:true});button.click();button.removeEventListener('click',listener);
 return{ok:!!event,kind:'click',provenance:'synthetic-dom-click',localTs,rowNodeId,nodeId,rect:geometry.rect,hitPoint:p,event,originalNodes:same};
})()
