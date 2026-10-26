// Native ordered selection. The mounted source and full original option rows
// are immutable; only an explicit checkbox/token callback changes local order.
function mountHomeMultiStatic(c,channel){
 const host=document.getElementById(c.host),home=document.getElementById('app-home');
 if(!host||host.dataset.homeStaticMount!==String(c.mount)||home?.dataset.visit!==String(c.visit)||home.dataset.revision!==String(c.revision))return null;
 const book=window.__spHomeStatic||(window.__spHomeStatic={entries:new Map()});
 for(const[id,e]of book.entries)if(!e.host.isConnected){e.stop();book.entries.delete(id);}
 const node=document.createElement('div');node.className='home-static-multi';node.setAttribute('role','group');
 const chips=document.createElement('div');chips.className='message-multi-selected';node.appendChild(chips);
 const rows=document.createElement('div');rows.className='app-modal-options';node.appendChild(rows);
 const status=document.createElement('p');status.className='app-home-notice';status.setAttribute('role','status');status.dataset.selectionStatus='true';node.appendChild(status);
 let selected=c.selected.slice();
 const e={host,node,mount:c.mount,multi:true,seq:0,acked:0,values:selected.map(o=>o.value),lease:false,closed:false};book.entries.set(c.mount,e);
 const owns=()=>!e.closed&&host.isConnected&&node.parentNode===host&&document.getElementById('app-home')===home&&home.dataset.visit===String(c.visit)&&home.dataset.revision===String(c.revision);
 const locked=()=>host.dataset.locked==='true';
 const emit=(kind,event)=>{e.values=selected.map(o=>o.value);channel.send({mount:c.mount,seq:++e.seq,kind,values:e.values.slice(),trusted:!!event?.isTrusted});};
 const canEdit=()=>owns()&&!locked()&&!e.lease;
 const controls=[];
 c.options.forEach((o,i)=>{const label=document.createElement('label');label.className='app-modal-option';const input=document.createElement('input');input.type='checkbox';input.dataset.optionIndex=String(i);input.value=`option-${i}`;
 const text=document.createElement('span');text.textContent=o.group?`${o.group}: ${o.label}`:o.label;
 label.appendChild(input);label.appendChild(text);rows.appendChild(label);controls.push(input);
 input.addEventListener('change',event=>{if(!canEdit()){sync();return;}if(input.checked){if(!selected.some(v=>v.value===o.value))selected.push(o.raw);}else selected=selected.filter(v=>v.value!==o.value);paint();emit('change',event);});
 });
 const paint=()=>{chips.replaceChildren();selected.forEach((o,i)=>{const chip=document.createElement('span');chip.className='message-multi-selected-option';chip.dataset.selectedValue=o.value;const text=document.createElement('span');text.textContent=o.text?.text||o.value;chip.appendChild(text);
 const remove=document.createElement('button');remove.type='button';remove.textContent='Remove';remove.dataset.selectedIndex=String(i);remove.setAttribute('aria-label',`Remove ${text.textContent}`);remove.addEventListener('click',event=>{if(!canEdit())return;selected.splice(i,1);paint();emit('change',event);});chip.appendChild(remove);chips.appendChild(chip);});sync();};
 const sync=()=>{if(!owns())return;node.id=host.dataset.nativeId;const disabled=locked();
 for(const[a,k]of[['aria-labelledby','label'],['aria-describedby','description'],['aria-required','required']])node.setAttribute(a,host.dataset[k]||'');
 const maximum=host.dataset.maximum===''?null:Number(host.dataset.maximum),invalid=host.dataset.invalid==='true'||(maximum!==null&&selected.length>maximum);node.setAttribute('aria-invalid',String(invalid));
 for(let i=0;i<controls.length;i++){controls[i].disabled=disabled;controls[i].checked=selected.some(o=>o.value===c.options[i].value);}
 for(const button of chips.querySelectorAll('button'))button.disabled=disabled;
 const copy=maximum===null?'':`You can select up to ${maximum} items.`;if(status.textContent!==copy)status.textContent=copy;status.hidden=maximum===null;
 };
 const stop=()=>{if(e.closed)return;e.closed=true;observer.disconnect();emit('closed');};e.stop=stop;host.appendChild(node);paint();
 const observer=new MutationObserver(()=>{if(!owns()){stop();return;}sync();});observer.observe(document.body,{subtree:true,childList:true,attributes:true,attributeFilter:['data-locked','data-invalid','data-required','data-label','data-description','data-maximum']});
 emit('mounted');return{stop,ack(a){if(a.seq===e.seq)e.acked=a.seq;}};
}
