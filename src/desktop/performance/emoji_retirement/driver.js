async function emojiRetirementAction(action,bridge) {
  const probe=window.__spEmojiRetirement;
  if(!probe)throw Error('Missing retirement probe');
  const start=performance.now(),deadline=start+5000;
  const verb=action.id.split('/').at(-1),subcase=action.subcase||'';
  const fieldId=subcase==='completion.thread'?'thread-composer':'channel-composer';
  const field=()=>document.getElementById(fieldId);
  const wire=f=>[f.value,f.selectionStart,f.selectionEnd,f.selectionDirection];
  const info=f=>f?{scope:f.dataset.draftScope,mount:f.dataset.projectionVisit,wire:wire(f),catalog:f.dataset.emojiCatalog,preferences:f.dataset.emojiPreferences,ready:f.dataset.completionReady,connected:f.isConnected}:null;
  const before={channel:info(document.getElementById('channel-composer')),thread:info(document.getElementById('thread-composer'))};
  const key=node=>node.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));
  const f=field();
  if(!document.hasFocus()||document.visibilityState!=='visible'||!f?.isConnected)throw Error('Action window/field unavailable');
  if(action.kind!=='setup'&&f.dataset.completionReady!=='true')throw Error('Controller is not ready');
  const isOpen=verb==='open'||verb==='seed-picker-open';
  const isPickerClose=verb==='seed-picker-close'||verb==='close'&&subcase==='refresh.picker';
  const isQuery=verb==='query';
  const isEmpty=verb==='account-clear'||verb==='boot-ready';
  const isDismiss=verb==='dismiss';
  const sameBefore=()=>{
    const previous=fieldId==='thread-composer'?before.thread:before.channel;
    return field()===f&&f.isConnected&&previous?.scope===f.dataset.draftScope
      &&previous.mount===f.dataset.projectionVisit;
  };
  probe.begin(action.id);
  bridge.send({kind:'start',id:action.id,index:action.index});
  if(isQuery||isEmpty){
    f.focus({preventScroll:true});
    if(isQuery&&f.value!=='')throw Error('Query lacked fixed empty/new-mount setup');
    f.value=isQuery?':ro':'';const end=f.value.length;f.setSelectionRange(end,end,'forward');
    if(f.selectionDirection!=='forward')throw Error('Forward selection refused');
    f.dispatchEvent(new InputEvent('input',{bubbles:true,inputType:isQuery?'insertText':'deleteContentBackward',data:isQuery?':ro':null}));
    if(verb==='boot-ready')f.dispatchEvent(new Event('focus'));
  }else if(isOpen){
    f.focus({preventScroll:true});const button=document.getElementById(fieldId+'-emoji');
    if(!button||button.disabled)throw Error('Picker button unavailable');button.click();
  }else if(isPickerClose){
    const q=document.getElementById('emoji-query');if(!q)throw Error('Picker not open');key(q);
  }else if(isDismiss){f.focus({preventScroll:true});key(f);}
  let timer;
  try {
    const ack=await Promise.race([bridge.recv(),new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error('Native action acknowledgement timeout')),Math.max(0,deadline-performance.now()));})]);
    const ackAt=performance.now();
    clearTimeout(timer);
    if(!ack?.ok||ack.id!==action.id)throw Error('Native action denied: '+JSON.stringify(ack));
    let accepted=null,acceptedAt=null;
    const witnesses=[];
    const witness=()=>{
      const dialog=document.getElementById('shell-overlay-dialog');
      const popup=document.getElementById(fieldId+'-completion');
      return {focused:document.hasFocus(),visibility:document.visibilityState,active:document.activeElement?.id||'',
        channel:info(document.getElementById('channel-composer')),thread:info(document.getElementById('thread-composer')),
        popup:popup?{hidden:popup.hidden,connected:popup.isConnected,rows:[...popup.querySelectorAll('[role=option]')].map(r=>({label:r.children[0]?.textContent||'',detail:r.children[1]?.textContent||''}))}:null,
        picker:dialog?{ready:dialog.dataset.emojiChoiceReady,catalog:dialog.dataset.emojiLifecycleCatalog,
          rows:[...dialog.querySelectorAll('#emoji-options button')].map(n=>({name:n.getAttribute('aria-label'),disabled:n.disabled}))}:null};
    };
    function predicate(){
      if(!document.hasFocus()||document.visibilityState!=='visible'||probe.overflow())return null;
      if(verb==='close'&&subcase==='completion.thread')return !document.getElementById('thread-composer')
        &&document.getElementById('channel-composer')?.dataset.projectionVisit===before.channel.mount?{closedThread:true}:null;
      const current=field();
      if(!current?.isConnected)return null;
      if(isQuery){if(!sameBefore()||document.activeElement!==f)return null;return probe.emoji(f,action.id);}
      if(isEmpty){return sameBefore()&&document.activeElement===f&&probe.empty(f)?{empty:true}:null;}
      if(isOpen||subcase==='refresh.picker'&&verb==='refresh'){
        const dialog=document.getElementById('shell-overlay-dialog'),q=document.getElementById('emoji-query');
        const names=dialog?[...dialog.querySelectorAll('#emoji-options button')]:[];
        const token=dialog?.dataset.emojiLifecycleCatalog;
        if(!sameBefore()||dialog?.dataset.emojiChoiceReady!=='true'||!token
          ||token!==f.dataset.emojiCatalog||document.activeElement!==q||names.length!==120
          ||!names.some(n=>n.getAttribute('aria-label')==='rocket')||names.some(n=>n.disabled))return null;
        if(verb==='refresh'&&token===before.channel.catalog)return null;
        return {picker:true,catalog:token,names:names.map(n=>n.getAttribute('aria-label'))};
      }
      if(isPickerClose){return sameBefore()&&!document.getElementById('shell-overlay-dialog')
        &&document.activeElement===f&&JSON.stringify(wire(f))===JSON.stringify(before.channel.wire)?{closed:true}:null;}
      if(isDismiss){const p=document.getElementById(fieldId+'-completion');return sameBefore()&&p?.hidden
        &&JSON.stringify(wire(f))===JSON.stringify((fieldId==='thread-composer'?before.thread:before.channel).wire)?{dismissed:true}:null;}
      if(subcase==='refresh.closed'&&verb==='refresh'){
        return sameBefore()&&!document.getElementById('shell-overlay-dialog')
          &&document.getElementById(fieldId+'-completion')?.hidden
          &&f.dataset.emojiCatalog===ack.state.catalog.ready?.token
          &&f.dataset.emojiCatalog!==before.channel.catalog?{catalog:f.dataset.emojiCatalog,preferences:f.dataset.emojiPreferences,ready:f.dataset.completionReady,connected:f.isConnected}:null;
      }
      if(verb==='thread-open'){
        const t=document.getElementById('thread-composer');
        if(!t?.isConnected||t.dataset.draftScope!==ack.state.threadScope||t.dataset.completionReady!=='true')return null;
        if(document.activeElement!==t)t.focus({preventScroll:true});
        return t.value===''&&probe.empty(t)?{scope:t.dataset.draftScope,mount:t.dataset.projectionVisit}:null;
      }
      if(verb==='navigate'||verb==='thread-channel'||verb==='replace'){
        const c=document.getElementById('channel-composer');
        if(!c?.isConnected||c.dataset.draftScope!==ack.state.channelScope||c.dataset.projectionVisit===before.channel.mount
          ||c.dataset.completionReady!=='true'||document.getElementById('shell-overlay-dialog'))return null;
        if(document.activeElement!==c)c.focus({preventScroll:true});
        if(verb!=='thread-channel'&&!probe.empty(c))return null;
        return {scope:c.dataset.draftScope,mount:c.dataset.projectionVisit};
      }
      if(verb==='recover-local'||verb==='recover-remote')return current.dataset.emojiCatalog===ack.state.catalog.ready?.token?{catalog:current.dataset.emojiCatalog}:null;
      // Fault/setup native endpoints require an actual completed native operation.
      return ack.nativeComplete?{nativeComplete:true,generation:ack.generation||null}:null;
    }
    await new Promise((resolve,reject)=>{
      function check(){
        if(performance.now()>deadline)return reject(Error('Action endpoint deadline'));
        const result=predicate();if(result){accepted=result;acceptedAt=performance.now();witnesses.push(witness());return resolve();}
        requestAnimationFrame(check);
      }check();
    });
    const frame=()=>new Promise((resolve,reject)=>requestAnimationFrame(()=>{
      const at=performance.now();if(at>deadline||!predicate())reject(Error('RAF endpoint lost authority'));else {witnesses.push(witness());resolve(at);}
    }));
    const raf1=await frame(),raf2=await frame();
    probe.end(action.id);
    return {ok:true,id:action.id,index:action.index,start,ackAt,acceptedAt,raf1,raf2,accepted,before,ack,witnesses,
      after:{channel:info(document.getElementById('channel-composer')),thread:info(document.getElementById('thread-composer'))}};
  }finally{clearTimeout(timer);}
}
window.emojiRetirementAction=emojiRetirementAction;
