// Original email local is literal; HTML sanitization is a separate native observation.
function mountHomeEmail(config,channel){
 const host=document.getElementById(config.host),home=document.getElementById('app-home');
 if(!host||host.dataset.homeTextMount!==String(config.mount)||home?.dataset.visit!==String(config.visit)||home.dataset.revision!==String(config.revision))return null;
 const book=window.__spHomeText||(window.__spHomeText={entries:new Map()});
 for(const [id,e]of book.entries)if(!e.host.isConnected){e.stop();book.entries.delete(id);}
 const c=config.email,node=document.createElement('input');node.type='email';node.autocomplete='off';node.spellcheck=false;
 node.setAttribute('min','0');node.setAttribute('max','0');node.value=c.local;
 const e={host,node,email:true,mount:config.mount,seq:0,acked:0,value:node.value,local:c.local,lease:false,closed:false,composing:false};
 book.entries.set(e.mount,e);
 const owns=()=>!e.closed&&host.isConnected&&node.parentNode===host&&document.getElementById('app-home')===home&&home.dataset.visit===String(config.visit)&&home.dataset.revision===String(config.revision);
 const sync=()=>{if(!owns())return;node.id=host.dataset.nativeId;node.name=node.id;node.placeholder=host.dataset.placeholder||'Enter an email address';node.disabled=host.dataset.locked==='true'||host.dataset.emailLoading==='true';
  for(const [a,k]of[['aria-labelledby','label'],['aria-describedby','description'],['aria-required','required'],['aria-invalid','invalid']])node.setAttribute(a,host.dataset[k]||'');
 };
 const emit=(kind,event,callback=false,prevented=false)=>{e.value=node.value;channel.send({mount:e.mount,seq:++e.seq,kind,value:e.value,key:event?.key??null,code:event?.code??null,ctrl:!!event?.ctrlKey,alt:!!event?.altKey,shift:!!event?.shiftKey,meta:!!event?.metaKey,repeat:!!event?.repeat,composing:!!event?.isComposing,trusted:!!event?.isTrusted,prevented,
  email:{local:e.local,callback,callbackValue:e.local,enter:!!c.enter,character:!!c.character,typeMismatch:!!node.validity.typeMismatch,composing:e.composing}});};
 const editable=()=>owns()&&!node.disabled&&!e.lease&&!book.activeLease;
 const input=event=>{if(!editable())return;e.local=node.value;emit('input',event,!e.composing);};
 const keydown=event=>{if(!editable()||node.value!==e.value||event.key!=='Enter'||!c.enter||e.composing||event.isComposing||event.keyCode===229)return;event.preventDefault();emit('enter',event,true,true);};
 const start=event=>{if(!editable())return;e.composing=true;emit('composition',event);};
 const end=event=>{if(!editable())return;e.composing=false;input(event);};
 const listeners=[['input',input],['keydown',keydown],['compositionstart',start],['compositionend',end]];
 const stop=()=>{if(e.closed)return;e.closed=true;focusControl?.detach();observer.disconnect();for(const [k,f]of listeners)node.removeEventListener(k,f);emit('closed');};
 e.stop=stop;host.appendChild(node);sync();for(const [k,f]of listeners)node.addEventListener(k,f);
 const observer=new MutationObserver(()=>{if(!owns()){stop();return;}sync();});
 observer.observe(document.body,{subtree:true,childList:true,attributes:true,attributeFilter:['data-locked','data-email-loading','data-invalid','data-required','data-label','data-description','data-placeholder']});
 emit('mounted');const focusControl=window.spHomeFocus?.attach(config.focus,node,host,e);return{stop,ack(a){if(a.seq===e.seq)e.acked=a.seq;}};
}
