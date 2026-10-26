// Exact native node listeners supply occurrence authority; DOM attributes only
// describe the renderer-owned host. Quiet and Modal inputs do not use this bridge.
function mountHomeText(config, channel) {
 if(config.email)return mountHomeEmail(config,channel);
 if(config.number)return mountHomeNumber(config,channel);
 const host=document.getElementById(config.host), home=document.getElementById('app-home');
 if(!host||host.dataset.homeTextMount!==String(config.mount)||home?.dataset.visit!==String(config.visit)||home.dataset.revision!==String(config.revision))return null;
 const book=window.__spHomeText||(window.__spHomeText={entries:new Map()});
 for(const [id,e] of book.entries)if(!e.host.isConnected){e.stop();book.entries.delete(id);}
 const node=document.createElement(config.multiline?'textarea':'input');if(!config.multiline)node.type='text';
 node.value=config.initial;node.autocapitalize='off';node.setAttribute('autocorrect','off');
 const e={host,node,mount:config.mount,seq:0,acked:0,value:node.value,lease:false,closed:false};
 book.entries.set(config.mount,e);
 const owns=()=>!e.closed&&host.isConnected&&node.parentNode===host&&document.getElementById('app-home')===home&&home.dataset.visit===String(config.visit)&&home.dataset.revision===String(config.revision);
 const sync=()=>{if(!owns())return;node.id=host.dataset.nativeId;node.name=node.id;node.placeholder=host.dataset.placeholder||'';node.disabled=host.dataset.locked==='true';for(const [a,k]of[['aria-labelledby','label'],['aria-describedby','description'],['aria-required','required'],['aria-invalid','invalid']])node.setAttribute(a,host.dataset[k]||'');};
 const emit=(kind,event,prevented=false)=>{const seq=++e.seq;e.value=node.value;channel.send({mount:config.mount,seq,kind,value:node.value,key:event?.key??null,code:event?.code??null,ctrl:!!event?.ctrlKey,alt:!!event?.altKey,shift:!!event?.shiftKey,meta:!!event?.metaKey,repeat:!!event?.repeat,composing:!!event?.isComposing,trusted:!!event?.isTrusted,prevented});};
 const input=event=>{if(owns()&&!node.disabled&&!e.lease)emit('input',event);};
 const keydown=event=>{if(!owns()||node.disabled||e.lease||event.key!=='Enter')return;
  // Exact qUwD with only the modifier props supplied by VaQU.
  const newline=!event.metaKey&&(event.ctrlKey?(config.mac||!event.shiftKey):(event.shiftKey||event.altKey));
  if(newline)return;event.preventDefault();emit('enter',event,true);
 };
 const stop=()=>{if(e.closed)return;e.closed=true;focusControl?.detach();observer.disconnect();node.removeEventListener('input',input);node.removeEventListener('keydown',keydown);emit('closed');};
 e.stop=stop;host.appendChild(node);sync();node.addEventListener('input',input);node.addEventListener('keydown',keydown);
 const observer=new MutationObserver(()=>{if(!owns()){stop();return;}sync();});
 observer.observe(document.body,{subtree:true,childList:true,attributes:true,attributeFilter:['data-locked','data-invalid','data-required','data-label','data-description','data-placeholder']});
 emit('mounted');const focusControl=window.spHomeFocus?.attach(config.focus,node,host,e);return{stop,ack(a){if(a.seq===e.seq)e.acked=a.seq;}};
}
function lockHomeText(config){
 const home=document.getElementById('app-home'),held=[];
 if(!home||home.dataset.visit!==String(config.visit)||home.dataset.revision!==String(config.revision))return null;
 const book=window.__spHomeText||(window.__spHomeText={entries:new Map()});if(book.activeLease)return 'busy';
 for(const f of config.fields){const n=document.getElementById(f.id);if(!n||!home.contains(n)||n.value!==f.value)return null;
  const emailEntry=f.email?book.entries.get(f.mount):null;
  // Source loading disables editing, not exact sibling observation or queued delivery.
  const policyDisabled=f.email&&emailEntry?.node===n&&emailEntry.host.dataset.locked!=='true'&&emailEntry.host.dataset.emailLoading==='true';
  if(n.disabled&&!policyDisabled)return null;
  if(f.email&&(n.type!=='email'||!emailEntry?.email||emailEntry.composing))return null;
  if(f.number&&(n.type!=='number'||n.validity.badInput))return null;
  if(f.mount!==null){const e=book?.entries.get(f.mount);if(!e||e.closed||e.lease||e.node!==n||e.node.parentNode!==e.host||e.seq!==f.seq||e.acked!==f.seq||e.value!==f.value||(f.number&&(!e.number||e.badInput||e.composing)))return null;held.push({n,e,f,host:e.host,readOnly:n.readOnly});}
  else held.push({n,e:null,f,host:n.parentNode,readOnly:n.readOnly});
 }
 const lease={};let released=false;if(book)book.activeLease=lease;
 const current=h=>(!book||book.activeLease===lease)&&document.getElementById('app-home')===home&&home.dataset.visit===String(config.visit)&&home.dataset.revision===String(config.revision)&&h.n.isConnected&&document.getElementById(h.f.id)===h.n&&home.contains(h.n)&&h.n.parentNode===h.host&&(!h.e||(book?.entries.get(h.f.mount)===h.e&&!h.e.closed&&h.e.node===h.n&&h.e.host===h.host&&h.e.lease===lease));
 for(const h of held){
  if(h.e)h.e.lease=lease;
  // Readonly preserves the browser's focus and selection. Real policy-disabled
  // state remains independently controlled by the renderer's host attributes.
  h.n.readOnly=true;
  h.guard=event=>{if(released||!current(h))return;if(event.type==='keydown'&&event.key!=='Enter')return;event.preventDefault();event.stopImmediatePropagation();};
  h.n.addEventListener('beforeinput',h.guard,true);h.n.addEventListener('input',h.guard,true);h.n.addEventListener('keydown',h.guard,true);
 }
 return()=>{
  if(released)return;released=true;
  for(const h of held){
   h.n.removeEventListener('beforeinput',h.guard,true);h.n.removeEventListener('input',h.guard,true);h.n.removeEventListener('keydown',h.guard,true);
   if(current(h))h.n.readOnly=h.readOnly;
   if(h.e?.lease===lease)h.e.lease=false;
  }
  if(book?.activeLease===lease)book.activeLease=null;
 };
}
