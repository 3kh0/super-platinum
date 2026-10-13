// Synthetic DOM events only: no system pointer or keyboard input.
window.runSelectionLifecycleTests = async attach => {
  const results=[];
  const assert=(ok,label)=>{if(!ok)throw Error(label);};
  const settle=()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));
  document.body.innerHTML='<style>section{height:200px;width:440px;overflow:auto;overflow-anchor:none}.row{height:40px;box-sizing:border-box}</style><section id="message-timeline" data-timeline-scope="channel-A"></section><section id="thread-body" data-timeline-scope="thread-A"></section><button id="outside">Outside</button>';
  const roots=[document.getElementById('message-timeline'),document.getElementById('thread-body')];
  const outside=document.getElementById('outside'), pinned=new Map(), events=[];
  const windows=new Map();
  function render(root){
    if(pinned.get(root.id))return;
    const first=Math.max(0,Math.floor(root.scrollTop/40)-12),end=Math.min(1000,first+32);
    const before=windows.get(root.id);if(before?.first===first&&before?.end===end)return;
    const retained=new Map([...root.querySelectorAll('[data-row-id]')].map(row=>[Number(row.dataset.index),row]));
    const top=document.createElement('div');top.style.height=`${first*40}px`;
    const bottom=document.createElement('div');bottom.style.height=`${(1000-end)*40}px`;
    const rows=Array.from({length:end-first},(_,offset)=>{const index=first+offset;let row=retained.get(index);if(!row){row=document.createElement('div');row.className='row';row.dataset.rowId=`${root.id}-${index}`;row.dataset.index=index;row.textContent=`Selectable row ${index}`;}return row;});
    // Reconcile retained nodes rather than replacing them so native selection
    // can survive a viewport update that keeps its selected row mounted.
    const wanted=[top,...rows,bottom];
    for(let i=0;i<wanted.length;i++)if(root.children[i]!==wanted[i])root.insertBefore(wanted[i],root.children[i]??null);
    while(root.children.length>wanted.length)root.lastElementChild.remove();
    windows.set(root.id,{first,end});
  }
  for(const root of roots){render(root);root.addEventListener('scroll',()=>render(root));}
  const service=attach(event=>{events.push(event);const root=document.getElementById(event.rootId);if(!root||root.dataset.timelineScope!==event.scope)return;pinned.set(root.id,event.pinned);if(!event.pinned)render(root);});
  const visible=root=>{const rect=root.getBoundingClientRect();return [...root.querySelectorAll('[data-row-id]')].filter(row=>{const r=row.getBoundingClientRect();return r.bottom>rect.top&&r.top<rect.bottom;});};
  const start=root=>{const row=visible(root)[0];row.dispatchEvent(new PointerEvent('pointerdown',{button:0,buttons:1,bubbles:true}));row.dispatchEvent(new Event('selectstart',{bubbles:true}));assert(pinned.get(root.id),'selection was not pinned');return row;};
  const selected=start(roots[0]);const selection=getSelection(),range=document.createRange();range.selectNodeContents(selected);selection.removeAllRanges();selection.addRange(range);const text=selection.toString();
  outside.addEventListener('mouseup',event=>event.stopPropagation(),{once:true});
  outside.dispatchEvent(new MouseEvent('mouseup',{bubbles:true}));await settle();
  assert(!pinned.get(roots[0].id),'release outside channel leaves selection pinned');
  assert(selection.toString()===text,'release discarded still-mounted selected text');
  results.push('outside release resumes channel virtualization and preserves mounted selected text');
  selection.removeAllRanges();start(roots[0]);roots[0].scrollTop=18000;roots[0].dispatchEvent(new Event('scroll'));
  assert(visible(roots[0]).length===0,'fixture did not reach omitted rows while pinned');
  outside.dispatchEvent(new MouseEvent('mouseup',{bubbles:true}));await settle();
  assert(!pinned.get(roots[0].id)&&visible(roots[0]).length>0,'outside release left a blank virtual jump');
  results.push('release outside a channel rematerializes a jump into virtual spacers');
  start(roots[1]);roots[1].scrollTop=12000;roots[1].dispatchEvent(new Event('scroll'));
  outside.dispatchEvent(new PointerEvent('pointercancel',{bubbles:true}));await settle();
  assert(!pinned.get(roots[1].id)&&visible(roots[1]).length>0,'pointer cancellation left thread frozen');
  assert(!pinned.get(roots[0].id),'thread cancellation pinned channel');
  results.push('pointer cancellation releases only the selected thread and materializes its viewport');
  start(roots[1]);window.dispatchEvent(new Event('blur'));await settle();
  assert(!pinned.get(roots[1].id),'window blur left selection pinned');
  results.push('window blur releases an unfinished selection gesture');
  start(roots[0]);outside.dispatchEvent(new PointerEvent('pointerup',{button:0,bubbles:true}));await settle();
  assert(!pinned.get(roots[0].id),'pointer release outside left selection pinned');
  const count=events.length;outside.dispatchEvent(new MouseEvent('mouseup',{bubbles:true}));window.dispatchEvent(new Event('blur'));
  assert(events.length===count,'duplicate terminal events notified more than once');
  results.push('outside pointer release is handled once despite subsequent mouseup or blur');
  start(roots[0]);roots[0].dataset.timelineScope='channel-B';pinned.set(roots[0].id,false);await settle();
  assert(events.at(-1).scope==='channel-A'&&!events.at(-1).pinned,'scope change did not retire old gesture');
  start(roots[1]);roots[1].remove();await settle();
  assert(events.at(-1).rootId==='thread-body'&&!events.at(-1).pinned,'unmounted thread retained its gesture');
  results.push('scope replacement and unmount retire the original selection owner');
  start(roots[0]);service.dispose();assert(!pinned.get(roots[0].id),'dispose left selection pinned');
  const disposed=events.length;roots[0].dispatchEvent(new Event('selectstart',{bubbles:true}));outside.dispatchEvent(new MouseEvent('mouseup',{bubbles:true}));
  assert(events.length===disposed,'disposed bridge retained event listeners');
  results.push('bridge disposal releases active selection and removes listeners');
  selection.removeAllRanges();roots[0].remove();outside.remove();return results;
};
