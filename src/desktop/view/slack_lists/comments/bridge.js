// The retained eval owns an exact editor node, not its reusable DOM id.
// Rust ACKs content only after retaining it under the original draft binding.
async function listCommentsComposeBridge(options, bridge, environment = globalThis) {
  const {document, MutationObserver, setTimeout, clearTimeout} = environment;
  const {id,scope,generation,mount} = options;
  const representation=options.representation??'rich-blocks',delta=representation==='plain-delta';
  if(!['rich-blocks','plain-delta'].includes(representation))throw Error('Unknown List draft representation');
  if(typeof id!=='string'||!id||typeof scope!=='string'||!scope||typeof mount!=='string'||!mount
      ||!Number.isSafeInteger(generation)||generation<0)throw Error('Invalid List draft binding');
  const registry=environment.__listCommentsComposeCaptures??=new Map(),prior=registry.get(mount);
  if(options.recover===true ? !prior||prior.running||prior.scope!==scope||prior.generation!==generation||prior.representation!==representation : !!prior)
    throw Error('Original List draft capture unavailable');
  const root=prior?.root??document.getElementById(id);
  const owner=prior?.owner??root?.closest('[data-list-comments-compose-scope]');
  const wrapper=prior?.wrapper??root?.closest('.rich-editor');
  if(!root||!owner||!wrapper||owner.dataset.listCommentsComposeScope!==scope
      ||owner.dataset.editorGeneration!==String(generation))throw Error('List draft mount changed');
  const holder=prior??{root,owner,wrapper,scope,generation,representation,sequence:0,running:false};
  registry.set(mount,holder);holder.running=true;
  const editorApi=()=>delta?environment.superPlatinumCommentDelta:environment.superPlatinumRichEditor;
  let api=editorApi(), observer=null, stopped=false, dirty=true, wake=null;
  let latest=null, waiting=null, receiverFailure=null, lease=null, finalSequence=null, departed=false;
  let clipboard=null,action=null;
  const commands=[], clone=v=>JSON.parse(JSON.stringify(v)),same=(a,b)=>JSON.stringify(a)===JSON.stringify(b);
  const alive=()=>root.isConnected&&document.getElementById(id)===root
    &&root.closest('[data-list-comments-compose-scope]')===owner&&root.closest('.rich-editor')===wrapper
    &&owner.dataset.listCommentsComposeScope===scope&&owner.dataset.editorGeneration===String(generation);
  const notify=()=>{dirty=true;if(wake){const w=wake;wake=null;w();}};
  const send=v=>bridge.send({scope,mount,...v});
  const content=value=>delta?value.delta:value.blocks;
  function editorAction(event) {
    if(!delta||!alive()||lease||action||wrapper.inert||!['save','cancel'].includes(event.detail))return;
    try {
      const current=snapshot();
      if(current.value.composing||current.value.pending)return;
      action={action:event.detail,sequence:current.sequence,revision:current.value.revision};notify();
    } catch(error) {receiverFailure=error;notify();}
  }
  async function afterAck(current) {
    if(clipboard)await clipboard.afterAck(current);
    if(!action)return;
    const original=action;action=null;
    const now=snapshot();
    if(alive()&&!lease&&!wrapper.inert&&current.sequence===original.sequence
        &&now.sequence===current.sequence&&!now.value.composing&&!now.value.pending)
      await send({kind:'editor-action',action:original.action,sequence:current.sequence,generation,revision:original.revision});
  }
  function snapshot(finalCapture=false) {
    const ticket=api.snapshot(root);
    if(!ticket||!(delta?Array.isArray(ticket.ops)&&ticket.mode==='PlainText'&&typeof ticket.pending==='boolean':Array.isArray(ticket.blocks))||!Number.isSafeInteger(ticket.revision)||ticket.revision<0
      ||typeof ticket.composing!=='boolean'||ticket.generation!==String(generation))throw Error('List draft snapshot unavailable');
    const payload=delta?{delta:{mode:'plain-text',ops:clone(ticket.ops)},pending:ticket.pending,attachmentEffects:ticket.attachmentEffects}:{blocks:clone(ticket.blocks)};
    const value={...payload,revision:ticket.revision,composing:ticket.composing,
      generation,detached:!alive(),finalCapture};
    if(!latest||!same(latest.value,value)) {
      if(!Number.isSafeInteger(++holder.sequence))throw Error('List draft sequence exhausted');
      latest={sequence:holder.sequence,ticket,value};
    } else if(delta)latest.ticket=ticket; // Fresh opaque ticket; bytes may be unchanged after unseal.
    return latest;
  }
  function unlock() {
    if(!lease)return;
    if(delta)api.unseal(root,lease.ticket);
    // This exact wrapper is the only node we locked. Never look up a replacement.
    if(wrapper.inert===true && (alive() || (!root.isConnected && !wrapper.isConnected)))wrapper.inert=lease.priorInert;
    lease=null;
  }
  function committed() {
    if(!alive()||wrapper.inert||root.contentEditable==='false'||root.closest('[hidden],[inert]'))return null;
    if(delta) {
      const value=snapshot();
      return alive()&&!value.value.composing&&!value.value.pending?value:null;
    }
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
      if(c.kind==='capture-abort')throw Error('List draft capture rejected by host');
      if(c.kind==='ack') {
        if(!waiting||c.sequence!==waiting.sequence)throw Error('List draft acknowledgment changed');
        waiting.resolve();
      } else if(clipboard&&await clipboard.receive(c)) {
        // Clipboard results are not capture acknowledgements or native leases.
      } else {
        if(!['capture','release','finish','validate'].includes(c.kind)||typeof c.request!=='string'||!c.request)
          throw Error('Invalid List draft command');
        commands.push(c);notify();
      }
    }
  }
  try {
    // RichEditor and parent mounted callbacks can run in either order.
    for(let n=0;!api?.snapshot(root);n++) {
      if(n===40||!alive())throw Error('List draft editor did not mount');
      await new Promise(resolve=>setTimeout(resolve,16));api=editorApi();
    }
    if(delta) {
      clipboard=listCommentDeltaClipboardBridge({root,api,options,alive,send,notify});
      root.addEventListener('comment-delta-action',editorAction);
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
        if(delta&&!api.seal(root,value.ticket)) {await send({kind:'capture-refused',request:command.request});continue;}
        latest=value={...value,sequence:holder.sequence};
        lease={request:command.request,sequence:value.sequence,priorInert:wrapper.inert,revision:value.value.revision,content:clone(content(value.value)),attachmentEffects:value.value.attachmentEffects,ticket:value.ticket};
        wrapper.inert=true;
        await capture(value,{request:command.request,locked:true});sent=value.sequence;
      } else if(command) {
        if(!lease||command.lease!==lease.request||command.sequence!==lease.sequence)
          throw Error('List draft lease changed');
        if(command.kind==='validate') {
          const current=snapshot();
          const ok=alive()&&!current.value.composing&&!current.value.pending&&wrapper.inert===true
            &&(delta||root.contentEditable!=='false')&&!root.closest('[hidden]')&&!wrapper.parentElement?.closest('[inert]')
            &&current.value.revision===lease.revision&&current.value.attachmentEffects===lease.attachmentEffects&&same(content(current.value),lease.content)
            &&(delta?api.validate(root,lease.ticket):wrapper.querySelector('.rich-link-controls')?.hidden!==false);
          // Keep the seal on either outcome. Rust's original lease owns release
          // on refusal; a successful route leaves it held until actual detach.
          await send({kind:'release-result',request:command.request,ok,finished:false,validated:true});
          continue;
        }
        if(command.kind==='finish') {
          const current=snapshot();
          if(!alive()||current.value.composing||current.value.pending||!wrapper.inert||current.value.revision!==lease.revision||current.value.attachmentEffects!==lease.attachmentEffects||!same(content(current.value),lease.content)
              ||(delta?!api.validate(root,lease.ticket):wrapper.querySelector('.rich-link-controls')?.hidden===false)) {
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
        await afterAck(current);
      }
    }
    if(finalSequence===null)throw Error('List draft ended without final acknowledgment');
    if(delta&&!api.release(root,String(generation)))throw Error('List Delta terminal cleanup unavailable');
    registry.delete(mount);
    return {scope,mount,final:true,sequence:finalSequence};
  } finally {
    stopped=true;holder.running=false;if(!departed)unlock();observer?.disconnect();
    clipboard?.release();root.removeEventListener('comment-delta-action',editorAction);
    root.removeEventListener('input',notify);root.removeEventListener('compositionstart',notify);root.removeEventListener('compositionend',notify);
    // Failed/timeout holders deliberately remain recoverable by exact mount.
  }
}
