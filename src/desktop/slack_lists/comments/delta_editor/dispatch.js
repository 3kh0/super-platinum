// Endpoint on the retained original bridge, never a second editor/capture owner.
// The parent calls afterAck only after durable capture acknowledgement, keeps
// receiving while Rust observes semantic facts, and releases on bridge cleanup.
function listCommentDeltaClipboardBridge({root,api,options,alive,send,notify}) {
  const {scope,mount,generation}=options;
  const clone=value=>JSON.parse(JSON.stringify(value));
  const same=(a,b)=>JSON.stringify(a)===JSON.stringify(b);
  let sent=null,closed=false;
  function request() {
    if(closed||!alive())return null;
    const value=api.clipboardRequest(root);
    return value&&value.scope===scope&&value.generation===String(generation)?value:null;
  }
  function changed(event) {
    if(closed)return;
    const value=request();
    if(!event||!value||event.detail===value.request)notify();
  }
  root.addEventListener('comment-delta-paste-request',changed);
  // Retirement/timeout signals input. afterAck observes it without a new timer.
  async function afterAck(captured) {
    if(closed)return;
    const value=request();
    if(sent&&(!value||!same(sent.clipboard,value))) {
      const old=sent;sent=null;
      await send({kind:'clipboard-cancel',request:old.clipboard.request,sequence:old.sequence,generation});
    }
    if(!value||sent||closed||!alive()||captured?.value?.pending!==true
        ||captured.value.generation!==generation||captured.value.revision!==value.revision
        ||!Number.isSafeInteger(captured.sequence)||captured.sequence<1)return;
    // Recheck after the optional cancellation send above. No successor paste
    // can borrow the acknowledgement of an earlier document/selection.
    if(!same(request(),value))return;
    sent={sequence:captured.sequence,clipboard:clone(value)};
    try {await send({kind:'clipboard-request',sequence:sent.sequence,generation,clipboard:clone(value)});}
    catch(error) {
      if(sent?.clipboard.request===value.request)sent=null;
      api.rejectPaste(root,String(generation),value.request);throw error;
    }
  }
  async function receive(command) {
    if(!['clipboard-receipt','clipboard-rejected'].includes(command?.kind))return false;
    const id=command.kind==='clipboard-receipt'?command.receipt?.request:command.request;
    const admitted=!closed&&sent&&command.scope===scope&&command.mount===mount
      &&command.sequence===sent.sequence&&id===sent.clipboard.request;
    let applied=false;
    if(admitted) {
      const original=sent;sent=null;
      if(alive()&&same(request(),original.clipboard)) {
        if(command.kind==='clipboard-rejected')api.rejectPaste(root,String(generation),id);
        else applied=api.resolvePaste(root,command.receipt)===true;
      }
    }
    // A foreign/late response never retires the active paste. Results are only
    // observations; the ensuing normal capture owns any inserted draft bytes.
    await send({kind:'clipboard-result',request:typeof id==='string'?id:null,applied});
    notify();return true;
  }
  return Object.freeze({afterAck,receive,release() {
    if(closed)return;closed=true;
    root.removeEventListener('comment-delta-paste-request',changed);
    if(sent)api.rejectPaste(root,String(generation),sent.clipboard.request);
    sent=null;
  }});
}
