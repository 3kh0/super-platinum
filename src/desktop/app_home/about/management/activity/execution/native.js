// One original list mount owns admission and the eventual loaded-detail focus.
async function workflowActivityExecution(expected, channel) {
  const get = id => document.getElementById(id);
  const root=get('workflow-management'),activity=get('workflow-activity'),form=get('workflow-activity-filters');
  const state=root?.__workflowManagement,input=get('workflow-activity-user-search');
  const keys=['status','from','to','limit','user'],fields=keys.map(k=>get('workflow-activity-'+k));
  const nodes=[root,activity,form],inert=nodes.map(n=>n?.inert);
  const dates=workflowActivitySnapshot(expected);
  let row=null,detail=null,detailRevision=null,held=null,ownedRead=null,waiting=true;
  let invalid=false,custodyLost=false,observer,end;
  const ended=new Promise(resolve=>{end=resolve;}),watchers=new Set();
  const enabled=n=>!!n&&!n.disabled&&!n.matches(':disabled');
  const usable=n=>n?.isConnected&&!n.validity?.badInput;
  const identity=()=>state&&root.__workflowManagement===state
    &&nodes.every(n=>n?.isConnected&&get(n.id)===n)&&root.contains(activity)&&activity.contains(form)
    &&root.dataset.managementVisit===String(expected.visit)
    &&root.dataset.managementDocument===String(expected.document)
    &&root.dataset.managementRevision===String(expected.documentRevision)
    &&form.dataset.revision===String(expected.filters?.revision)
    &&get('workflow-activity-user-search')===input&&input?.tagName==='INPUT'
    &&form.contains(input)&&usable(input)&&!input.closest('fieldset[disabled]')
    &&input.dataset.generation===String(expected.memberGeneration)&&input.value===expected.search
    &&fields.every((n,i)=>usable(n)&&enabled(n)&&form.contains(n)&&get('workflow-activity-'+keys[i])===n&&n.value===expected.filters?.[keys[i]])
    &&!state.composing&&!document.querySelector('[role="dialog"][aria-modal="true"]')
    &&(()=>{const now=workflowActivitySnapshot(expected);return dates&&now&&dates.from===now.from&&dates.to===now.to;})()
    &&(!detail||(detail.isConnected&&get(detail.id)===detail&&activity.contains(detail)&&detail.dataset.detailRevision===detailRevision));
  const rowCurrent=()=>row?.isConnected&&get(row.id)===row&&activity.contains(row)&&enabled(row);
  const matches=transition=>!invalid&&identity()&&(!state.lease||state.lease===held)
    &&nodes.every(n=>held?n.inert===true:!n.inert)
    &&activity.dataset.pageRevision===String(expected.pageRevision)
    &&(ownedRead!==null?activity.dataset.activityRead===ownedRead&&(enabled(input)||input.disabled===true)
      :transition||activity.dataset.activityRead==='0'&&enabled(input)&&rowCurrent());
  const inspect=event=>{
    if(event&&(event.type==='compositionstart'||!held)) {invalid=true;custodyLost=true;}
    const retiring=held&&ownedRead!==null&&identity()&&state.lease===held
      &&nodes.every(n=>n.inert===true)&&activity.dataset.activityRead==='0';
    if(waiting){
      if(!identity()||state.lease||nodes.some(n=>n.inert)
        ||!['0',String(expected.retiredRead)].includes(activity.dataset.activityRead)){invalid=true;custodyLost=true;}
    }else if(!matches(held?.serial===0&&ownedRead===null)){
      invalid=true;if(!retiring)custodyLost=true;
    }
    watchers.forEach(check=>check());
  };
  const events=['input','change','compositionstart','pointerdown','keydown','focusin'];
  const unwatch=()=>{observer?.disconnect();events.forEach(e=>root?.removeEventListener(e,inspect,true));};
  const waitFor=async check=>{
    let timer,inspectReady;
    try{return await Promise.race([ended.then(()=>false),new Promise(resolve=>{
      inspectReady=()=>{const value=check();if(value!==null)resolve(value);};
      watchers.add(inspectReady);timer=setTimeout(()=>resolve(false),1000);inspectReady();
    })]);}finally{clearTimeout(timer);watchers.delete(inspectReady);}
  };
  const paint=async(ms,fallback)=>{let frame,timer;try{return await Promise.race([ended.then(()=>false),new Promise(resolve=>{
    frame=requestAnimationFrame(()=>resolve(true));timer=setTimeout(()=>resolve(fallback),ms);
  })]);}finally{cancelAnimationFrame(frame);clearTimeout(timer);}};
  const unlock=async terminal=>{
    if(!held)return;await paint(200,true);if(terminal)unwatch();
    if(state.lease===held){state.lease=null;nodes.forEach((n,i)=>{n.inert=inert[i];});}held=null;
  };
  const lock=async serial=>{
    if(held||!matches(false))return false;
    held={serial,close:()=>{invalid=true;custodyLost=true;end();}};state.lease=held;nodes.forEach(n=>{n.inert=true;});
    if(!await paint(1000,false)||!matches(false)){invalid=true;await unlock(false);return false;}return true;
  };
  try {
    if(!identity()||state.lease||nodes.some(n=>n.inert)){
      channel.send({serial:0,ok:false});await channel.recv();return;
    }
    // MutationObserver supplies a record array, not a user interaction event.
    observer=new MutationObserver(()=>inspect());observer.observe(document.documentElement,{attributes:true,childList:true,subtree:true});
    events.forEach(e=>root.addEventListener(e,inspect,true));
    const ready=await waitFor(()=>{
      if(invalid||!identity()||state.lease||!['0',String(expected.retiredRead)].includes(activity.dataset.activityRead))return false;
      if(activity.dataset.activityRead!=='0'||!enabled(input)||activity.dataset.pageRevision!==String(expected.pageRevision))return null;
      const candidate=get('workflow-activity-run-'+expected.execution);
      if(!candidate?.isConnected||candidate.tagName!=='BUTTON'||!activity.contains(candidate)||!enabled(candidate))return null;
      row=candidate;return true;
    });
    waiting=false;
    if(!ready){channel.send({serial:0,ok:false});await channel.recv();return;}
    channel.send({serial:0,ok:await lock(0)});
    while(true){
      const command=await Promise.race([channel.recv(),ended.then(()=>null)]);if(!command)break;
      if(command.kind==='unlock'){if(held?.serial===command.serial)await unlock(false);continue;}
      if(command.kind==='activate'){
        const allowed=held?.serial===0&&ownedRead===null&&Number.isSafeInteger(command.read)&&command.read>0&&typeof command.detail==='string';
        const ok=allowed&&await waitFor(()=>{
          if(!matches(true))return false;
          if(activity.dataset.activityRead!==String(command.read))return null;
          const current=get('workflow-activity-detail');
          if(!current?.isConnected||!activity.contains(current)||current.dataset.detailRevision!==command.detail)return null;
          detail=current;detailRevision=command.detail;return true;
        });
        if(ok)ownedRead=String(command.read);else invalid=true;
        channel.send({serial:command.serial,ok:!!ok&&matches(false)});continue;
      }
      if(command.kind==='close'){
        if(ownedRead===null||!held||held.serial!==command.lease||state.lease!==held){channel.send({serial:command.serial,ok:false});continue;}
        let cancelled=false;const cancel=()=>{cancelled=true;end();};const cancellation=channel.recv().then(cancel,cancel);
        const ready=await waitFor(()=>{
          if(custodyLost||!identity()||state.lease!==held||!nodes.every(n=>n.inert===true)
            ||![ownedRead,'0'].includes(activity.dataset.activityRead))return false;
          return activity.dataset.activityRead==='0'&&enabled(input)
            &&(!command.focus||detail?.dataset.detailLoaded==='true')?true:null;
        });
        await unlock(true);
        const ok=ready&&!cancelled&&!custodyLost&&identity()&&state.lease===null
          &&nodes.every((n,i)=>n.inert===inert[i])&&activity.dataset.activityRead==='0'&&enabled(input);
        if(ok&&command.focus)detail.focus();
        channel.send({serial:command.serial,ok:!!ok});await cancellation;break;
      }
      channel.send({serial:command.serial,ok:command.kind==='lock'&&ownedRead!==null?await lock(command.serial):false});
    }
  }finally{unwatch();await unlock(false);}
}
