// One retained eval owns one exact RichEditor node. Its opaque snapshot tickets
// never cross JSON; Rust receives only cloned content and monotonic capture IDs.
// A detached final capture belongs to this original owner, not a new same-ID node.
async function canvasRichReplyBridge(options, bridge, environment = globalThis) {
  const {document, MutationObserver, setTimeout, clearTimeout} = environment;
  let api = environment.superPlatinumRichEditor;
  const {id, scope, generation, mount} = options;
  if (typeof scope !== 'string' || !scope || typeof mount !== 'string' || !mount
      || !Number.isSafeInteger(generation) || generation < 0) throw Error('Invalid rich reply owner');
  const registry = environment.__superPlatinumCanvasReplyCaptures ??= new Map();
  const prior = registry.get(mount);
  if (options.recover === true && (!prior || prior.scope!==scope || prior.generation!==generation || prior.running))
    throw Error('Original rich reply capture is unavailable');
  if (options.recover !== true && prior) throw Error('Duplicate rich reply mount');
  const root = prior?.root ?? document.getElementById(id);
  if (!root || (!prior && (root.closest('[data-canvas-reply-scope]')?.dataset.canvasReplyScope !== scope
      || root.closest('[data-editor-generation]')?.dataset.editorGeneration !== String(generation))
    )) throw Error('Rich reply mount changed');
  const holder=prior ?? {root,scope,generation,sequence:0,finishing:false,running:false};
  registry.set(mount,holder);
  holder.running=true;
  try {
  // Parent and RichEditor onmounted callbacks can be delivered in either order.
  // Wait only for this already captured node's model; never adopt a replacement.
  for(let attempt=0;!api?.snapshot(root);attempt++) {
    if(attempt===40 || !root.isConnected || document.getElementById(id)!==root)
      throw Error('Rich reply editor did not mount');
    await new Promise(resolve=>setTimeout(resolve,16));
    api=environment.superPlatinumRichEditor;
  }
  const clone = value => JSON.parse(JSON.stringify(value));
  const same = (a,b) => JSON.stringify(a) === JSON.stringify(b);
  let latest = null, nextSequence = holder.sequence, dirty = true, stopped = false, waiting = null, requested = null;
  let wake = null, lastSent = null, finalSent = false, finishing = holder.finishing;
  const alive = () => root.isConnected && document.getElementById(id) === root
    && root.closest('[data-canvas-reply-scope]')?.dataset.canvasReplyScope === scope
    && root.closest('[data-editor-generation]')?.dataset.editorGeneration === String(generation);
  function notify() { dirty = true; if (wake) { const callback=wake; wake=null; callback(); } }
  function snapshot() {
    const ticket = api.snapshot(root);
    if (!ticket || !Array.isArray(ticket.blocks) || !Number.isSafeInteger(ticket.revision)
        || ticket.revision < 0 || typeof ticket.composing !== 'boolean' || ticket.generation !== String(generation))
      throw Error('Rich reply snapshot unavailable');
    const serialized = {blocks:clone(ticket.blocks), revision:ticket.revision,
      composing:ticket.composing, generation, detached:!alive(), finalCapture:finishing};
    if (!latest || !same(latest.value,serialized)) {
      if (!Number.isSafeInteger(++nextSequence)) throw Error('Rich reply capture exhausted');
      holder.sequence=nextSequence;
      latest={sequence:nextSequence,ticket,value:serialized};
    }
    return latest;
  }
  const changed = () => notify();
  let observer=null;
  const send = async value => { await bridge.send({scope,mount,...value}); };
  function readyToReplace(command) {
    return command && command.kind==='replace' && command.scope===scope && command.mount===mount
      && Number.isSafeInteger(command.sequence) && latest && latest.sequence===command.sequence
      && alive() && !latest.value.composing && Array.isArray(command.blocks);
  }
  async function receive() {
    while (!stopped) {
      const command = await bridge.recv();
      if (command?.scope !== scope || command?.mount !== mount) {
        throw Error('Rich reply response owner changed');
      }
      if (command.kind==='ack') {
        if (!waiting || command.sequence!==waiting.sequence) throw Error('Rich reply acknowledgement changed');
        const done=waiting; waiting=null; done.resolve();
      } else if (command.kind==='capture') {
        if(typeof command.request!=='string' || !command.request || requested!==null || finishing)
          throw Error('Rich reply capture request changed');
        requested=command.request;
        notify();
      } else if (command.kind==='finish') {
        if(typeof command.request!=='string' || !command.request || requested!==null || finishing)
          throw Error('Rich reply departure request changed');
        snapshot();
        if(!alive() || latest.value.composing) {
          await send({kind:'finish-refused',request:command.request});
          continue;
        }
        // Freeze only this original node during the final custody handoff.
        // Successful departure removes it; failed disk persistence keeps its
        // captured bytes and an explicit Retry action in the original root.
        root.contentEditable='false';
        finishing=true;
        holder.finishing=true;
        requested=command.request;
        notify();
      } else if (command.kind==='replace') {
        if(typeof command.request!=='string' || !command.request || finishing)
          throw Error('Rich reply replacement request changed');
        // Refresh exact native bytes before spending the original opaque ticket.
        // A newer edit changes sequence even if Rust has not received it yet.
        snapshot();
        const permitted=readyToReplace(command);
        const replacement=permitted ? api.replace(root,latest.ticket,command.blocks,command.labels || {}) : null;
        await send({kind:'replace-result',request:command.request,sequence:command.sequence,replaced:!!replacement});
        notify();
      } else { throw Error('Unknown rich reply command'); }
    }
  }
  let receiverFailure = null;
  // Receive failures wake a blocked producer; they must not disappear in an
  // unobserved promise while the original node's final capture is outstanding.
  let receiver=null;
  try {
  root.addEventListener('input',changed);
  root.addEventListener('compositionstart',changed);
  root.addEventListener('compositionend',changed);
  // Formatting commands can modify nodes without input; ancestor removal may
  // disconnect the complete panel. This observer only schedules an exact-node
  // comparison; it never scans or projects unrelated document content.
  observer = new MutationObserver(records => {
    if (!alive() || records.some(r=>r.target===root || root.contains(r.target))) notify();
  });
  observer.observe(document.documentElement,{subtree:true,childList:true,characterData:true,attributes:true});
  receiver=receive().catch(error=>{
    receiverFailure=error;
    if(waiting){const pending=waiting;waiting=null;pending.reject(error);}
    notify();
  });
    while (!stopped) {
      if(receiverFailure)throw receiverFailure;
      if(!dirty)await new Promise(resolve=>{wake=resolve;});
      dirty=false;
      if(receiverFailure)throw receiverFailure;
      let current=snapshot();
      const request=requested;requested=null;
      if(request!==null && lastSent===current.sequence) {
        if (!Number.isSafeInteger(++nextSequence)) throw Error('Rich reply capture exhausted');
        holder.sequence=nextSequence;
        latest={...current,sequence:nextSequence};current=latest;
      }
      if(lastSent===current.sequence)continue;
      // Only one full content capture is outstanding. Input continues and is
      // coalesced into the newest original-node snapshot after its acknowledgment.
      const acknowledged=new Promise((resolve,reject)=>{waiting={sequence:current.sequence,resolve,reject};});
      await send({kind:'capture',sequence:current.sequence,request,...current.value});
      lastSent=current.sequence;finalSent=current.value.detached || current.value.finalCapture;
      // A bounded wait never silently releases custody: timeout is a terminal
      // error reported by the retained Rust task with the entry still charged.
      let timer;
      try { await Promise.race([acknowledged,new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error('Rich reply acknowledgment timed out')),5000);})]); }
      finally { clearTimeout(timer); }
      if(finalSent){stopped=true;break;}
      // Catch input/removal that occurred during the send/ack window.
      if(!alive() || snapshot().sequence!==lastSent)notify();
    }
  } finally {
    stopped=true;
    holder.running=false;
    observer?.disconnect();
    root.removeEventListener('input',changed);
    root.removeEventListener('compositionstart',changed);
    root.removeEventListener('compositionend',changed);
    // The eval owner ends this receive channel when the function returns. Do
    // not await an idle recv here; the final capture has already been acked.
    void receiver;
  }
  if(registry.get(mount)!==holder)throw Error('Rich reply retained owner changed');
  registry.delete(mount);
  return {scope,mount,final:true,sequence:lastSent};
  } finally { holder.running=false; }
}
