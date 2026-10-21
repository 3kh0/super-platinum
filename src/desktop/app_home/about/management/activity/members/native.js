// Read-only admission of the actual member input and complete filter buffers.
function workflowMemberNodes(expected) {
  const root=document.getElementById('workflow-management');
  const form=document.getElementById('workflow-activity-filters');
  const input=document.getElementById('workflow-activity-user-search');
  if (!root?.isConnected || !form || !input || !root.contains(form) || !form.contains(input)
      || root.dataset.managementVisit!==String(expected.visit)
      || root.dataset.managementDocument!==String(expected.document)
      || form.dataset.revision!==String(expected.filters.revision)
      || root.__workflowManagement?.composing || input.disabled || input.validity?.badInput
      || input.value!==expected.search
      || document.querySelector('[role="dialog"][aria-modal="true"]')) return null;
  const dateWitness=workflowActivityDateWitness(expected,form);
  if(!dateWitness) return null;
  for (const name of ['status','limit','user']) {
    const node=form.querySelector('#workflow-activity-'+name);
    if (!node || node.disabled || node.validity?.badInput || node.value!==expected.filters[name]) return null;
  }
  return {root,input,dateWitness};
}
function workflowMemberSnapshot(expected) {
  const nodes=workflowMemberNodes(expected);
  return !!nodes && nodes.input.dataset.generation===String(expected.generation);
}
// Input events reach Rust before Dioxus commits its new generation attribute.
// Wait for that attribute only, never for different raw buffers to catch up.
function workflowMemberCommittedSnapshot(expected) {
  const target=String(expected.generation);
  if(!/^\d+$/.test(target)) return Promise.resolve(false);
  const initial=workflowMemberNodes(expected);
  if (!initial) return Promise.resolve(false);
  const generation=()=>initial.input.dataset.generation;
  const pending=()=>/^\d+$/.test(generation()??'') && BigInt(generation())<BigInt(target);
  if (generation()===String(expected.generation)) return Promise.resolve(true);
  if (!pending()) return Promise.resolve(false);
  return new Promise(resolve=>{
    let finished=false,timer;
    const finish=value=>{
      if(finished)return;
      finished=true;observer.disconnect();clearTimeout(timer);
      for(const event of ['input','change','compositionstart']) initial.root.removeEventListener(event,inspect,true);
      resolve(value);
    };
    const inspect=()=>{
      const current=workflowMemberNodes(expected);
      if(!initial.dateWitness.current() || !current || current.root!==initial.root || current.input!==initial.input) return finish(false);
      if(generation()===String(expected.generation)) return finish(true);
      if(!pending()) finish(false);
    };
    const observer=new MutationObserver(inspect);
    observer.observe(document.documentElement,{subtree:true,childList:true,attributes:true});
    for(const event of ['input','change','compositionstart']) initial.root.addEventListener(event,inspect,true);
    timer=setTimeout(()=>finish(false),1000);
    inspect();
  });
}
