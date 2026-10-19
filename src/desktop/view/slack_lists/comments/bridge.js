// The retained eval owns an exact editor node, not its reusable DOM id.
// Rust ACKs content only after retaining it under the original draft binding.
async function listCommentsComposeBridge(options, bridge, environment = globalThis) {
  const {document, MutationObserver, setTimeout, clearTimeout} = environment;
  const {id,scope,generation,mount} = options;
  if(typeof id!=='string'||!id||typeof scope!=='string'||!scope||typeof mount!=='string'||!mount
      ||!Number.isSafeInteger(generation)||generation<0)throw Error('Invalid List draft binding');
  const registry=environment.__listCommentsComposeCaptures??=new Map(),prior=registry.get(mount);
  if(options.recover===true ? !prior||prior.running||prior.scope!==scope||prior.generation!==generation : !!prior)
    throw Error('Original List draft capture unavailable');
  const root=prior?.root??document.getElementById(id);
  const owner=prior?.owner??root?.closest('[data-list-comments-compose-scope]');
  const wrapper=prior?.wrapper??root?.closest('.rich-editor');
  if(!root||!owner||!wrapper||owner.dataset.listCommentsComposeScope!==scope
      ||owner.dataset.editorGeneration!==String(generation))throw Error('List draft mount changed');
  const holder=prior??{root,owner,wrapper,scope,generation,sequence:0,running:false};
  registry.set(mount,holder);holder.running=true;
  let api=environment.superPlatinumRichEditor, observer=null, stopped=false, dirty=true, wake=null;
  let latest=null, waiting=null, receiverFailure=null, lease=null, finalSequence=null, departed=false;
  const commands=[], clone=v=>JSON.parse(JSON.stringify(v)),same=(a,b)=>JSON.stringify(a)===JSON.stringify(b);
  const alive=()=>root.isConnected&&document.getElementById(id)===root
    &&root.closest('[data-list-comments-compose-scope]')===owner&&root.closest('.rich-editor')===wrapper
    &&owner.dataset.listCommentsComposeScope===scope&&owner.dataset.editorGeneration===String(generation);
  const notify=()=>{dirty=true;if(wake){const w=wake;wake=null;w();}};
  const send=v=>bridge.send({scope,mount,...v});
  function snapshot(finalCapture=false) {
    const ticket=api.snapshot(root);
    if(!ticket||!Array.isArray(ticket.blocks)||!Number.isSafeInteger(ticket.revision)||ticket.revision<0
      ||typeof ticket.composing!=='boolean'||ticket.generation!==String(generation))throw Error('List draft snapshot unavailable');
    const value={blocks:clone(ticket.blocks),revision:ticket.revision,composing:ticket.composing,
      generation,detached:!alive(),finalCapture};
    if(!latest||!same(latest.value,value)) {
      if(!Number.isSafeInteger(++holder.sequence))throw Error('List draft sequence exhausted');
      latest={sequence:holder.sequence,ticket,value};
    }
    return latest;
  }
  function unlock() {
    if(!lease)return;
    // This exact wrapper is the only node we locked. Never look up a replacement.
    if(wrapper.inert===true && (alive() || (!root.isConnected && !wrapper.isConnected)))wrapper.inert=lease.priorInert;
    lease=null;
  }
  function committed() {
    if(!alive()||wrapper.inert||root.contentEditable==='false'||root.closest('[hidden],[inert]'))return null;
    // The legacy API takes an id. Both sides of that synchronous call prove it
    // still denotes our exact node; detached capture uses snapshot(node) only.
    const blocks=api.readForCommit(id);
    if(!alive())return null;
    const value=snapshot();
    return !value.value.composing&&same(blocks,value.value.blocks)?value:null;
  }
  async function capture(value,extra={}) {
    if(waiting)throw Error('Overlapping List draft capture');
    const ack=new Promise((resolve,reject)=>{waiting={sequence:value.sequence,resolve,reject};});
    void ack.catch(()=>{}); // A recv failure can precede the send continuation.
    let timer;
    try {
      await send({kind:'capture',sequence:value.sequence,...value.value,...extra});
      await Promise.race([ack,new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error('List draft capture acknowledgment timed out')),5000);})]);
    } finally {clearTimeout(timer);waiting=null;}
    if(value.value.detached||value.value.finalCapture)finalSequence=value.sequence;
  }
  async function receive() {
    while(!stopped) {
      const c=await bridge.recv();
      if(c?.scope!==scope||c?.mount!==mount)throw Error('List draft command owner changed');
      if(c.kind==='ack') {
        if(!waiting||c.sequence!==waiting.sequence)throw Error('List draft acknowledgment changed');
        waiting.resolve();
      } else {
        if(!['capture','release','finish'].includes(c.kind)||typeof c.request!=='string'||!c.request)
          throw Error('Invalid List draft command');
        commands.push(c);notify();
      }
    }
  }
  try {
    // RichEditor and parent mounted callbacks can run in either order.
    for(let n=0;!api?.snapshot(root);n++) {
      if(n===40||!alive())throw Error('List draft editor did not mount');
      await new Promise(resolve=>setTimeout(resolve,16));api=environment.superPlatinumRichEditor;
    }
    root.addEventListener('input',notify);root.addEventListener('compositionstart',notify);root.addEventListener('compositionend',notify);
    observer=new MutationObserver(records=>{if(!alive()||records.some(r=>r.target===root||root.contains(r.target)))notify();});
    observer.observe(document.documentElement,{subtree:true,childList:true,characterData:true,attributes:true});
    void receive().catch(error=>{receiverFailure=error;waiting?.reject(error);notify();});
    let sent=null;
    while(!stopped) {
      if(receiverFailure)throw receiverFailure;
      if(!dirty&&!commands.length)await new Promise(resolve=>{wake=resolve;});
      dirty=false;if(receiverFailure)throw receiverFailure;
      if(!alive()) {
        unlock();const value=snapshot();await capture(value);break;
      }
      const command=commands.shift();
      if(command?.kind==='capture') {
        let value=null;
        try {if(!lease)value=committed();}catch(_) { /* IME/open link/unsupported native input stays untouched. */ }
        if(!value) {await send({kind:'capture-refused',request:command.request});continue;}
        // Refresh sequence even if the bytes were already ACKed continuously.
        if(!Number.isSafeInteger(++holder.sequence))throw Error('List draft sequence exhausted');
        latest=value={...value,sequence:holder.sequence};
        lease={request:command.request,sequence:value.sequence,priorInert:wrapper.inert,revision:value.value.revision,blocks:clone(value.value.blocks)};
        wrapper.inert=true;
        await capture(value,{request:command.request,locked:true});sent=value.sequence;
      } else if(command) {
        if(!lease||command.lease!==lease.request||command.sequence!==lease.sequence)
          throw Error('List draft lease changed');
        if(command.kind==='finish') {
          const current=snapshot();
          if(!alive()||current.value.composing||!wrapper.inert||current.value.revision!==lease.revision||!same(current.value.blocks,lease.blocks)
              ||wrapper.querySelector('.rich-link-controls')?.hidden===false) {
            unlock();await send({kind:'release-result',request:command.request,ok:false,finished:false});notify();continue;
          }
          await capture(snapshot(true));
          await send({kind:'release-result',request:command.request,ok:true,finished:true});
          // Departure consumes the mount. Keep its seal through Rust's close
          // render; otherwise input in that gap would have no capture owner.
          departed=true;break;
        }
        unlock();await send({kind:'release-result',request:command.request,ok:true,finished:false});notify();
      } else if(!lease) {
        const current=snapshot();
        if(sent!==current.sequence){await capture(current);sent=current.sequence;}
      }
    }
    if(finalSequence===null)throw Error('List draft ended without final acknowledgment');
    registry.delete(mount);
    return {scope,mount,final:true,sequence:finalSequence};
  } finally {
    stopped=true;holder.running=false;if(!departed)unlock();observer?.disconnect();
    root.removeEventListener('input',notify);root.removeEventListener('compositionstart',notify);root.removeEventListener('compositionend',notify);
    // Failed/timeout holders deliberately remain recoverable by exact mount.
  }
}
