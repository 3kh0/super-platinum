// Number callbacks preserve native spelling independently of committed parent wire.
// No step/pattern/inputMode is supplied: this is the original number control.
function homeNumberTransition(config, local, kind, raw) {
 if(kind==='input'){
  const v=raw.trim();
  const callback=!(config.dispatch&&v===local?.trim());
  const value=v===''?null:Number(v).toString();
  const accepted=(!Number.isNaN(Number(v))&& (config.decimal||!v.includes('.'))) || v==='-'||v===''||(config.decimal&&v==='.');
  return {local:accepted?v:local,callback,value:callback?value:null};
 }
 if(kind==='enter')return {local,callback:!!config.enter,value:local||null};
 if(kind==='blur'&&typeof local==='string'&&local.includes('.')&&(local.endsWith('0')||local.endsWith('.'))){
  const v=local.replace(/(\.[0-9]*?[1-9])0+$|\.0*$/,'$1');
  if(v!==local)return {local:v,callback:true,value:v||null};
 }
 // Original omitted-seed blur throws. Product keeps the pristine value and
 // emits no invented clear; it never deliberately throws into the renderer.
 return {local,callback:false,value:null};
}
function mountHomeNumber(config, channel){
 const host=document.getElementById(config.host),home=document.getElementById('app-home');
 if(!host||host.dataset.homeTextMount!==String(config.mount)||home?.dataset.visit!==String(config.visit)||home.dataset.revision!==String(config.revision))return null;
 const book=window.__spHomeText||(window.__spHomeText={entries:new Map()});
 for(const [id,e]of book.entries)if(!e.host.isConnected){e.stop();book.entries.delete(id);}
 const n=config.number,node=document.createElement('input');node.type='number';node.autocomplete='off';
 node.value=n.local??'';
 const e={host,node,number:true,mount:config.mount,seq:0,acked:0,value:node.value,local:n.local,lease:false,closed:false,badInput:false,composing:false};
 book.entries.set(config.mount,e);
 const owns=()=>!e.closed&&host.isConnected&&node.parentNode===host&&document.getElementById('app-home')===home&&home.dataset.visit===String(config.visit)&&home.dataset.revision===String(config.revision);
 const sync=()=>{if(!owns())return;node.id=host.dataset.nativeId;node.name=node.id;node.placeholder=n.placeholder;node.disabled=host.dataset.locked==='true';
  for(const [a,k]of[['aria-labelledby','label'],['aria-describedby','description'],['aria-required','required'],['aria-invalid','invalid']])node.setAttribute(a,host.dataset[k]||'');
  for(const key of ['min','max']){if(n[key]!==null&&n[key]!==undefined)node.setAttribute(key,String(Number(n[key])));else node.removeAttribute(key);}
 };
 const emit=(kind,event,transition={local:e.local,callback:false,value:null},prevented=false)=>{
  e.local=transition.local;e.value=node.value;e.badInput=!!node.validity.badInput;
  channel.send({mount:e.mount,seq:++e.seq,kind,value:e.value,key:event?.key??null,code:event?.code??null,
   ctrl:!!event?.ctrlKey,alt:!!event?.altKey,shift:!!event?.shiftKey,meta:!!event?.metaKey,repeat:!!event?.repeat,composing:!!event?.isComposing,trusted:!!event?.isTrusted,prevented,
   number:{local:e.local,callback:transition.callback,callbackValue:transition.value,badInput:e.badInput,composing:e.composing,
    valid:node.validity.valid,stepMismatch:node.validity.stepMismatch,rangeUnderflow:node.validity.rangeUnderflow,rangeOverflow:node.validity.rangeOverflow}});
 };
 const editable=()=>owns()&&!node.disabled&&!e.lease&&!book.activeLease;
 const input=event=>{if(!editable())return;
  if(node.validity.badInput||e.composing){emit('input',event);return;}
  const t=homeNumberTransition(n,e.local,'input',node.value);
  // React's controlled value restores a rejected local edit. A callback can
  // already have normalized that attempted value, so carry both independently.
  if(t.local!==node.value)node.value=t.local??'';
  emit('input',event,t);
 };
 const keydown=event=>{if(!editable()||node.value!==e.value||node.validity.badInput||event.key!=='Enter'||!n.enter||e.composing||event.isComposing||event.keyCode===229)return;
  event.preventDefault();emit('enter',event,homeNumberTransition(n,e.local,'enter',node.value),true);
 };
 const blur=event=>{if(!editable()||node.value!==e.value)return;
  if(node.validity.badInput||e.composing){emit('blur',event);return;}
  const t=homeNumberTransition(n,e.local,'blur',node.value);if(t.callback)node.value=t.local??'';emit('blur',event,t);
 };
 const start=event=>{if(!editable())return;e.composing=true;emit('composition',event);};
 const end=event=>{if(!editable())return;e.composing=false;input(event);};
 const listeners=[['input',input],['keydown',keydown],['blur',blur],['compositionstart',start],['compositionend',end]];
 const stop=()=>{if(e.closed)return;e.closed=true;focusControl?.detach();observer.disconnect();for(const [kind,fn]of listeners)node.removeEventListener(kind,fn);emit('closed');};
 e.stop=stop;host.appendChild(node);sync();for(const [kind,fn]of listeners)node.addEventListener(kind,fn);
 const observer=new MutationObserver(()=>{if(!owns()){stop();return;}sync();});
 observer.observe(document.body,{subtree:true,childList:true,attributes:true,attributeFilter:['data-locked','data-invalid','data-required','data-label','data-description']});
 emit('mounted');const focusControl=window.spHomeFocus?.attach(config.focus,node,host,e);return{stop,ack(a){if(a.seq===e.seq)e.acked=a.seq;}};
}
