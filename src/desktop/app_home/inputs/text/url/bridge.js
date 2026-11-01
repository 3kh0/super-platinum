// A named native URL data adapter. Validation never replaces the literal or opens it.
function validHomeUrl(literal){
 if(literal==='')return true;
 try{new URL(literal);return true;}catch{}
 try{new URL('http://'+literal);return true;}catch{return false;}
}
function mountHomeUrl(config,channel){
 const host=document.getElementById(config.host),home=document.getElementById('app-home');
 if(!host||host.dataset.homeTextMount!==String(config.mount)||home?.dataset.visit!==String(config.visit)||home.dataset.revision!==String(config.revision))return null;
 const book=window.__spHomeText||(window.__spHomeText={entries:new Map()});
 for(const [id,entry]of book.entries)if(entry.closed||!entry.host.isConnected){entry.stop();book.entries.delete(id);}
 const c=config.policy,node=document.createElement('input');node.type='url';node.autocomplete='off';node.spellcheck=false;
 node.value=c.local;
 const e={host,node,url:true,mount:config.mount,seq:0,acked:0,value:node.value,local:c.local,lease:false,closed:false,composing:false,badInput:false};
 book.entries.set(e.mount,e);
 const owns=()=>!e.closed&&host.isConnected&&node.parentNode===host&&document.getElementById('app-home')===home&&home.dataset.visit===String(config.visit)&&home.dataset.revision===String(config.revision);
 const sync=()=>{if(!owns())return;node.id=host.dataset.nativeId;node.name=node.id;node.placeholder=host.dataset.placeholder||'Enter a URL';
  node.disabled=host.dataset.locked==='true'||host.dataset.urlLoading==='true';
  for(const [a,k]of[['aria-labelledby','label'],['aria-describedby','description'],['aria-invalid','invalid']])node.setAttribute(a,host.dataset[k]||'');
  node.setAttribute('aria-required','false');
 };
 const emit=(kind,event,callback=false,prevented=false)=>{
  if(e.seq>=Number.MAX_SAFE_INTEGER){e.closed=true;return;}
  e.value=node.value;e.badInput=!!node.validity.badInput;
  channel.send({mount:e.mount,seq:++e.seq,kind,value:e.value,key:event?.key??null,prevented,
   local:e.local,callback,callbackValue:e.local,valid:validHomeUrl(e.local),badInput:e.badInput,composing:e.composing||!!event?.isComposing});
 };
 const editable=()=>owns()&&!node.disabled&&!e.lease&&!book.activeLease;
 const input=event=>{if(!editable())return;
  const composing=e.composing||!!event?.isComposing,bad=!!node.validity.badInput;
  if(!composing&&!bad)e.local=node.value;
  emit('input',event,!composing&&!bad);
 };
 const keydown=event=>{if(!editable()||node.value!==e.value||event.key!=='Enter'||!c.enter||e.composing||node.validity.badInput||event.isComposing||event.keyCode===229)return;
  event.preventDefault();e.local=node.value;emit('enter',event,true,true);
 };
 const start=event=>{if(!editable())return;e.composing=true;emit('composition',event);};
 const end=event=>{if(!editable())return;e.composing=false;input(event);};
 const listeners=[['input',input],['keydown',keydown],['compositionstart',start],['compositionend',end]];
 let observer,focusControl;
 const stop=()=>{if(e.closed)return;e.closed=true;focusControl?.detach();observer?.disconnect();for(const [kind,fn]of listeners)node.removeEventListener(kind,fn);emit('closed');};
 e.stop=stop;host.appendChild(node);sync();for(const [kind,fn]of listeners)node.addEventListener(kind,fn);
 observer=new MutationObserver(()=>{if(!owns()){stop();return;}sync();});
 observer.observe(document.body,{subtree:true,childList:true,attributes:true,attributeFilter:['data-locked','data-url-loading','data-invalid','data-label','data-description','data-placeholder']});
 emit('mounted');focusControl=window.spHomeFocus?.attach(config.focus,node,host,e);
 return{stop,ack(a){if(a.seq===e.seq)e.acked=a.seq;}};
}
