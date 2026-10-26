// Options and callback identity are supplied by the current typed Home source.
function mountHomeStatic(c,channel){
 const host=document.getElementById(c.host),home=document.getElementById('app-home');
 if(!host||host.dataset.homeStaticMount!==String(c.mount)||home?.dataset.visit!==String(c.visit)||home.dataset.revision!==String(c.revision))return null;
 const book=window.__spHomeStatic||(window.__spHomeStatic={entries:new Map()});
 for(const[id,e]of book.entries)if(!e.host.isConnected){e.stop();book.entries.delete(id);}
 const node=document.createElement('select');
 if(host.dataset.compact==='true')node.className='block-select';
 if(host.dataset.accessibleLabel)node.setAttribute('aria-label',host.dataset.accessibleLabel);
 const empty=document.createElement('option');empty.value='';empty.textContent=host.dataset.placeholder||'';empty.disabled=!c.optional;empty.hidden=!c.optional;node.appendChild(empty);
 c.options.forEach((o,i)=>{const n=document.createElement('option');n.value=`option-${i}`;n.textContent=o.group?`${o.group} — ${o.label}`:o.label;node.appendChild(n);});
 node.value=c.selected===null?'':`option-${c.selected}`;
 const e={host,node,mount:c.mount,seq:0,acked:0,index:c.selected,lease:false,closed:false};book.entries.set(c.mount,e);
 const owns=()=>!e.closed&&host.isConnected&&node.parentNode===host&&document.getElementById('app-home')===home&&home.dataset.visit===String(c.visit)&&home.dataset.revision===String(c.revision);
 const selected=()=>node.value===''?null:c.options.findIndex((_,i)=>node.value===`option-${i}`);
 const sync=()=>{if(!owns())return;node.id=host.dataset.nativeId;node.name=node.id;node.disabled=host.dataset.locked==='true';const placeholder=host.dataset.placeholder||'';if(empty.textContent!==placeholder)empty.textContent=placeholder;for(const[a,k]of[['aria-labelledby','label'],['aria-describedby','description'],['aria-required','required'],['aria-invalid','invalid']])node.setAttribute(a,host.dataset[k]||'');};
 const emit=(kind,event,index=selected())=>{e.index=selected();channel.send({mount:c.mount,seq:++e.seq,kind,index,trusted:!!event?.isTrusted});};
 const change=event=>{if(!owns()||node.disabled||e.lease)return;const i=selected();if(i===-1||(i===null&&!c.optional))return;if(i!==null&&c.options[i].value==='')node.value='';emit('change',event,i);};
 const stop=()=>{if(e.closed)return;e.closed=true;observer.disconnect();node.removeEventListener('change',change);emit('closed');};
 e.stop=stop;host.appendChild(node);sync();node.addEventListener('change',change);
 const observer=new MutationObserver(()=>{if(!owns()){stop();return;}sync();});observer.observe(document.body,{subtree:true,childList:true,attributes:true,attributeFilter:['data-locked','data-invalid','data-required','data-label','data-description','data-placeholder']});
 emit('mounted');return{stop,ack(a){if(a.seq===e.seq)e.acked=a.seq;}};
}
function lockHomeStatic(c){
 const book=window.__spHomeStatic,home=document.getElementById('app-home');
 if(!home||home.dataset.visit!==String(c.visit)||home.dataset.revision!==String(c.revision))return null;
 // Date-only forms have no static registry. Their real controls are attested
 // by the subsequent calendar/text lease, after this exact Home check.
 if(c.fields.length===0)return()=>{};
 if(!book)return null;
 let busy=!!book.activeLease;const held=[];
 for(const f of c.fields){const n=document.getElementById(f.id),value=f.index===null?'':`option-${f.index}`;
  if(f.radio){
   const e=book.entries.get(f.mount);
   if(!n||!home.contains(n)||!e||!e.radio||e.closed||e.node!==n||n.parentNode!==e.host||e.lease&&e.lease!==book.activeLease)return null;
   if(!Number.isSafeInteger(f.seq)||f.seq<1||!Number.isSafeInteger(e.seq)||e.seq<f.seq||!Number.isSafeInteger(e.acked)||e.acked<0||e.acked>e.seq)return null;
   const rows=[...n.querySelectorAll('input[data-option-index]')];
   if(rows.length!==f.options.length||rows.some((r,i)=>r!==e.rows[i]||r.type!=='radio'||r.name!==e.host.id||r.dataset.optionIndex!==String(i)||r.value!==f.options[i].value||r.parentNode.dataset.optionSource!==f.options[i].source||r.parentNode.textContent!==e.labels[i]))return null;
   const valid=v=>v===null||typeof v==='string';
   if(!valid(e.value)||!valid(f.value)||rows.some((r,i)=>r.checked!==(e.value===f.options[i].sourceValue)))return null;
   // Hidden selected values remain owned display state. Newer publications must
   // prove each actual current-row replacement before they qualify as Busy.
   let selected=f.value;
   for(let seq=f.seq+1;seq<=e.seq;seq++){
    const change=e.changes.find(v=>v.seq===seq);
    if(!change||!Number.isSafeInteger(change.index)||change.index<0||change.index>=f.options.length)return null;
    const value=f.options[change.index].sourceValue;if(selected===value)return null;selected=value;
   }
   if(selected!==e.value)return null;
   if(e.seq>f.seq||e.acked<e.seq)busy=true;
   held.push({n,e,host:n.parentNode,id:f.id});continue;
  }
  if(f.checkbox){
   const e=book.entries.get(f.mount);
   if(!n||!home.contains(n)||!e||!e.checkbox||e.closed||e.node!==n||n.parentNode!==e.host||e.lease&&e.lease!==book.activeLease)return null;
   if(!Number.isSafeInteger(f.seq)||f.seq<1||!Number.isSafeInteger(e.seq)||e.seq<f.seq||!Number.isSafeInteger(e.acked)||e.acked<0||e.acked>e.seq)return null;
   const rows=[...n.querySelectorAll('input[data-option-index]')];
   if(rows.length!==f.options.length||rows.some((r,i)=>r!==e.rows[i]||r.type!=='checkbox'||r.dataset.optionIndex!==String(i)||r.value!==f.options[i].value||r.parentNode.dataset.optionSource!==f.options[i].source||r.parentNode.textContent!==e.labels[i]))return null;
   const valid=v=>Array.isArray(v)&&v.length<=1000&&new Set(v).size===v.length&&v.every(x=>typeof x==='string');
   if(!valid(e.values)||!valid(f.values)||rows.some((r,i)=>r.checked!==e.values.includes(f.options[i].sourceValue)))return null;
   // Hidden survivors are owned by the sealed selection. For newer published
   // occurrences, replay exact row toggles rather than canonicalizing survivors.
   let selected=f.values.slice();
   for(let seq=f.seq+1;seq<=e.seq;seq++){
    const change=e.changes.find(v=>v.seq===seq);
    if(!change||!Number.isSafeInteger(change.index)||change.index<0||change.index>=f.options.length||typeof change.checked!=='boolean')return null;
    const v=f.options[change.index].sourceValue;
    if(selected.includes(v)===change.checked)return null;
    selected=change.checked?[...selected,v]:selected.filter(x=>x!==v);
   }
   if(JSON.stringify(selected)!==JSON.stringify(e.values))return null;
   if(e.seq>f.seq||e.acked<e.seq)busy=true;
   held.push({n,e,host:n.parentNode,id:f.id});continue;
  }
  if(f.multi){
   if(!n||!home.contains(n))return null;
   let e=null;
   if(f.mount!==null){
    e=book.entries.get(f.mount);
    if(!e||!e.multi||e.closed||e.node!==n||n.parentNode!==e.host||e.lease&&e.lease!==book.activeLease)return null;
    if(!Number.isSafeInteger(f.seq)||f.seq<1||!Number.isSafeInteger(e.seq)||e.seq<f.seq||!Number.isSafeInteger(e.acked)||e.acked<0||e.acked>e.seq)return null;
    const rows=[...n.querySelectorAll('input[data-option-index]')],chips=[...n.querySelectorAll('[data-selected-value]')];
    if(rows.length!==f.options.length||rows.some((r,i)=>r.dataset.optionIndex!==String(i)||r.type!=='checkbox'||r.value!==f.options[i].value||r.parentNode.querySelector('span')?.textContent!==f.options[i].text))return null;
    if(!Array.isArray(e.values)||new Set(e.values).size!==e.values.length||e.values.length>1000||e.values.some(v=>typeof v!=='string'))return null;
    if(chips.length!==e.values.length||chips.some((r,i)=>r.dataset.selectedValue!==e.values[i]||r.querySelector('button')?.dataset.selectedIndex!==String(i)))return null;
    if(rows.some((r,i)=>r.checked!==e.values.includes(f.options[i].sourceValue)))return null;
    if(e.seq===f.seq&&JSON.stringify(e.values)!==JSON.stringify(f.values))return null;
    // Progress is only waitable if the newer published callback is wholly
    // composed of current unique options. Retained display is not new authority.
    if(e.seq>f.seq&&e.values.some(v=>!f.options.some(o=>o.sourceValue===v)))return null;
    if(e.seq>f.seq||e.acked<e.seq)busy=true;
   }else{
    if(n.tagName!=='SELECT'||!n.multiple)return null;
    const options=[...n.options];
    if(options.length!==f.options.length||options.some((o,i)=>o.value!==f.options[i].value||o.textContent!==f.options[i].text||o.selected!==f.values.includes(f.options[i].sourceValue)))return null;
    if(new Set(f.values).size!==f.values.length||f.values.some(v=>!f.options.some(o=>o.sourceValue===v)))return null;
   }
   held.push({n,e,host:n.parentNode,id:f.id});continue;
  }
  if(!n||n.tagName!=='SELECT'||!home.contains(n))return null;
  const options=[...n.options].filter(o=>o.value!=='').map(o=>({value:o.value,text:o.textContent}));if(options.length!==f.options.length||options.some((o,i)=>o.value!==f.options[i].value||o.text!==f.options[i].text))return null;
  let e=null;if(f.mount!==null){
   e=book.entries.get(f.mount);
   if(!e||e.closed||e.node!==n||e.node.parentNode!==e.host||e.lease&&e.lease!==book.activeLease)return null;
   if(!Number.isSafeInteger(f.seq)||f.seq<1||!Number.isSafeInteger(e.seq)||e.seq<f.seq||!Number.isSafeInteger(e.acked)||e.acked<0||e.acked>e.seq)return null;
   if(e.index!==null&&(!Number.isSafeInteger(e.index)||e.index<0||e.index>=options.length))return null;
   // A published occurrence may be ahead of Rust receipt/ack. Only wait when
   // its own native value is intact; an unsignaled buffer is never authority.
   if(n.value!==(e.index===null?'':`option-${e.index}`))return null;
   if(e.seq===f.seq&&e.index!==f.index)return null;
   if(e.seq>f.seq||e.acked<e.seq)busy=true;
  }else if(n.value!==value)return null;
  held.push({n,e,host:n.parentNode,id:f.id});
 }
 // Validate every sibling before classifying progress or another lease as busy.
 if(busy)return 'busy';
 const token={};book.activeLease=token;let done=false;
 const current=h=>book.activeLease===token&&document.getElementById('app-home')===home&&home.dataset.visit===String(c.visit)&&home.dataset.revision===String(c.revision)&&h.n.isConnected&&document.getElementById(h.id)===h.n&&h.n.parentNode===h.host&&(!h.e||book.entries.get(h.e.mount)===h.e&&h.e.lease===token&&!h.e.closed);
 for(const h of held){if(h.e)h.e.lease=token;h.guard=event=>{if(done||!current(h))return;if(event.type==='keydown'&&event.key==='Tab')return;event.preventDefault();event.stopImmediatePropagation();};for(const type of ['input','change','mousedown','click','keydown'])h.n.addEventListener(type,h.guard,true);}
 // Select has no readonly. Preserve focus/value and suppress selection events
 // for the exact short lease; source loading owns its normal disabled state.
 return()=>{if(done)return;done=true;for(const h of held){for(const type of ['input','change','mousedown','click','keydown'])h.n.removeEventListener(type,h.guard,true);if(h.e?.lease===token)h.e.lease=false;}if(book.activeLease===token)book.activeLease=null;};
}
