(()=>{
 const r=window.__firstPublishRegistry??={ids:new WeakMap(),next:1,parent:null,modal:null};
 const id=e=>{if(!e)return null;if(!r.ids.has(e))r.ids.set(e,r.next++);return r.ids.get(e)};
 const rect=e=>{const q=e.getBoundingClientRect();return{left:q.left,top:q.top,right:q.right,bottom:q.bottom,width:q.width,height:q.height}};
 const node=e=>{if(!e)return null;let displayed=e.isConnected;for(let p=e;p;p=p.parentElement){const s=getComputedStyle(p);if(s.display==='none'||s.visibility!=='visible'||p.hidden)displayed=false;}return{id:e.id,nodeId:id(e),connected:e.isConnected,displayed,rect:rect(e)}};
 const field=e=>e?{...node(e),value:e.value,selectionStart:e.selectionStart,selectionEnd:e.selectionEnd,selectionDirection:e.selectionDirection,draftScope:e.dataset.draftScope??null,projectionVisit:e.dataset.projectionVisit??null,readonly:e.readOnly,disabled:e.disabled}:null;
 const channel=document.getElementById('channel-composer'),thread=document.getElementById('thread-composer'),pane=document.querySelector('.thread-pane'),body=document.getElementById('thread-body');
 if(BIND==='channel'){if(r.shareDrafts||!channel?.isConnected)return{ok:false,reason:'Original channel binding refused'};r.shareDrafts={window,document,channel,pane:null,body:null,thread:null};}
 if(BIND==='thread'){if(!r.shareDrafts||r.shareDrafts.thread||!thread?.isConnected||!pane?.isConnected||!body?.isConnected)return{ok:false,reason:'Original thread binding refused'};Object.assign(r.shareDrafts,{thread,pane,body});}
 const old=r.shareDrafts;
 return{ok:!old||old.window===window&&old.document===document,windowOriginal:!!old&&old.window===window&&old.document===document,channel:field(channel),thread:field(thread),threadPane:node(pane),threadBody:body?{...node(body),rows:[...body.querySelectorAll('article[data-message-id]')].map(e=>({id:e.dataset.messageId,nodeId:id(e),text:e.textContent}))}:null,
 original:old?{channelNodeId:id(old.channel),threadPaneNodeId:id(old.pane),threadBodyNodeId:id(old.body),threadComposerNodeId:id(old.thread)}:null,channelOriginalConnected:!!old?.channel?.isConnected,threadOriginalConnected:!!old?.thread?.isConnected,sameThreadNodes:!!old?.thread&&old.thread===thread&&old.pane===pane&&old.body===body};
})()