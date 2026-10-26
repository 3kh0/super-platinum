// Typed Dioxus owns inert label/description composition. This listener owns
// native occurrence order only; it never rewrites source text or creates HTML.
function mountHomeCheckboxes(c,channel){
 const host=document.getElementById(c.host),home=document.getElementById('app-home'),node=document.getElementById(host?.dataset.nativeId);
 if(!host||!node||node.parentNode!==host||host.dataset.homeStaticMount!==String(c.mount)||home?.dataset.visit!==String(c.visit)||home.dataset.revision!==String(c.revision))return null;
 const rows=[...node.querySelectorAll('input[data-option-index]')];
 if(rows.length!==c.options.length||rows.some((r,i)=>r.type!=='checkbox'||r.dataset.optionIndex!==String(i)||r.value!==`option-${i}`||r.parentNode.dataset.optionSource!==c.options[i].source))return null;
 const book=window.__spHomeStatic||(window.__spHomeStatic={entries:new Map()});
 for(const[id,e]of book.entries)if(!e.host.isConnected){e.stop();book.entries.delete(id);}
 const e={host,node,mount:c.mount,checkbox:true,seq:0,acked:0,values:c.selected.slice(),changes:[],rows,labels:rows.map(r=>r.parentNode.textContent),lease:false,closed:false};book.entries.set(c.mount,e);
 const owns=()=>!e.closed&&host.isConnected&&node.parentNode===host&&document.getElementById(node.id)===node&&document.getElementById('app-home')===home&&home.dataset.visit===String(c.visit)&&home.dataset.revision===String(c.revision);
 const locked=()=>host.dataset.locked==='true';
 const sync=()=>{if(!owns())return;for(let i=0;i<rows.length;i++){rows[i].disabled=locked();rows[i].checked=e.values.includes(c.options[i].value);}node.setAttribute('aria-disabled',String(locked()));for(const[a,k]of[['aria-labelledby','label'],['aria-describedby','description'],['aria-required','required'],['aria-invalid','invalid']])node.setAttribute(a,host.dataset[k]||'');};
 const emit=(kind,event,index=null,checked=null)=>{const seq=++e.seq;if(kind==='change'){e.changes.push({seq,index,checked});if(e.changes.length>128)e.changes.shift();}channel.send({mount:c.mount,seq,kind,index,checked,values:e.values.slice(),trusted:!!event?.isTrusted});};
 const listeners=rows.map((r,i)=>{const change=event=>{if(!owns()||locked()||e.lease){sync();return;}const checked=r.checked,value=c.options[i].value;
   // Exact native toggle, not a duplicate checked callback or foreign array.
   if(checked===e.values.includes(value)){sync();return;}
   if(checked)e.values.push(value);else e.values=e.values.filter(v=>v!==value);sync();emit('change',event,i,checked);
 };r.addEventListener('change',change);return change;});
 const stop=()=>{if(e.closed)return;e.closed=true;observer.disconnect();rows.forEach((r,i)=>r.removeEventListener('change',listeners[i]));emit('closed');};e.stop=stop;
 sync();const observer=new MutationObserver(()=>{if(!owns()){stop();return;}sync();});observer.observe(document.body,{subtree:true,childList:true,attributes:true,attributeFilter:['data-locked','data-invalid','data-required','data-label','data-description']});
 emit('mounted');return{stop,ack(a){if(a.seq===e.seq)e.acked=a.seq;}};
}
