// Two trusted Tab observations; the existing owned native KeyboardSession sends
// input only after each exact ready receipt. No input is synthesized here.
async function scheduledForwardKeyboard(config, bridge, env=globalThis) {
  const doc=env.document,root=doc.getElementById('forward-dialog');
  if(!root)throw Error('Exact keyboard surface absent');
  const buttons=()=>[...root.querySelectorAll('.forward-schedule button')];
  const named=label=>buttons().find(e=>e.textContent.trim()===label);
  const source=config.ordinal===0?root?.querySelector('.schedule-toggle input[type=checkbox]'):named('Schedule forward');
  const target=named(config.ordinal===0?'Schedule forward':'Cancel scheduling');
  const current=()=>root?.isConnected&&doc.getElementById('forward-dialog')===root
    &&root.dataset.scheduledForwardOwner===config.owner&&root.dataset.editorGeneration===config.generation
    &&source?.isConnected&&target?.isConnected&&!source.disabled&&!target.disabled;
  if(![0,1].includes(config.ordinal)||!current())throw Error('Keyboard owner/controls changed');
  source.scrollIntoView({block:'center',inline:'nearest'});source.focus({preventScroll:true});
  await new Promise(r=>env.requestAnimationFrame(()=>env.requestAnimationFrame(r)));
  if(!current()||doc.activeElement!==source)throw Error('Keyboard source changed');
  const started=env.performance.now();let timer,listener;
  const completion=new Promise((resolve,reject)=>{
    timer=env.setTimeout(()=>reject(Error('Trusted Tab timeout')),8000);
    listener=event=>{
      if(!current()||doc.activeElement!==source||event.target!==source||event.key!=='Tab'||!event.isTrusted
        ||event.repeat||event.isComposing||event.shiftKey||event.ctrlKey||event.altKey||event.metaKey) {
        reject(Error('Unexpected keyboard event/owner'));return;
      }
      doc.removeEventListener('keydown',listener,true);
      env.requestAnimationFrame(()=>env.requestAnimationFrame(()=>{
        const elapsed=env.performance.now()-started;
        if(!current()||doc.activeElement!==target||event.defaultPrevented||!Number.isFinite(elapsed)||elapsed<0||elapsed>=8000) {
          reject(Error('Trusted Tab did not reach exact control'));return;
        }
        resolve({ok:true,ordinal:config.ordinal,owner:config.owner,generation:config.generation,
          key:'Tab',trusted:true,repeat:false,composing:false,modifiers:false,defaultPrevented:false,
          source:config.ordinal===0?'private-confirmation':'Schedule forward',target:target.textContent.trim(),elapsedMs:elapsed});
      }));
    };
    doc.addEventListener('keydown',listener,true);
  });
  try {
    bridge.send({kind:'ready',ordinal:config.ordinal,owner:config.owner,generation:config.generation,
      source:config.ordinal===0?'private-confirmation':'Schedule forward',target:target.textContent.trim()});
    const result=await completion;
    doc.removeEventListener('keydown',listener,true);env.clearTimeout(timer);
    bridge.send({...result,cleaned:true});
  } finally {
    doc.removeEventListener('keydown',listener,true);env.clearTimeout(timer);
  }
}
if(typeof module!=='undefined')module.exports={scheduledForwardKeyboard};
