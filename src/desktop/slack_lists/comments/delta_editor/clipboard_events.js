// Original-node clipboard event controller. It never reads the system clipboard
// outside the user's event, imports HTML, or dispatches Slack/network requests.
function listCommentDeltaClipboardEvents({root,input,quill,model,codec,context,
    request,retired,notice,signal,setTimeout,clearTimeout,makeId}) {
  const Delta=quill.constructor.import('delta');
  const clipboard=listCommentDeltaClipboard(model),cleanup=[];
  const equal=(a,b)=>JSON.stringify(a)===JSON.stringify(b);
  let currentRequest=null,closed=false;
  function state() {return closed?null:context();}
  function owned(value) {
    return value?.root===root&&value.owned&&value.connected&&root.contains(input);
  }
  function writable(value) {
    return owned(value)&&!value.disabled&&!value.frozen&&!value.composing&&!value.repairPending;
  }
  function unchanged(before) {
    const after=state();
    return writable(after)&&after.revision===before.revision
      &&after.selectionRevision===before.selectionRevision
      &&equal(after.selection,before.selection)&&equal(after.ops,before.ops);
  }
  function insert(value) {
    const before=state();if(!writable(before)||value.root!==root||!equal(value.selection,before.selection))return false;
    // All opaque appearances/types must be admitted before Quill mutates.
    const plan=codec.prepare(value.ops,value.emoji||[]);
    if(!unchanged(before))return false;
    const contents=new Delta(plan.ops),range=value.selection;
    plan.commit();
    quill.updateContents(new Delta().retain(range.index).delete(range.length).concat(contents),'user');
    quill.setSelection(range.index+contents.length(),0,'user');return true;
  }
  const broker=listCommentDeltaPaste({model,current:state,insert,
    onRequest:value=>{currentRequest=value;request(model.clone(value));signal();},
    onRetired:value=>{currentRequest=null;retired(value);signal();},
    setTimeout,clearTimeout,makeId});
  function listen(type,callback){input.addEventListener(type,callback,true);cleanup.push(()=>input.removeEventListener(type,callback,true));}
  function stop(event){event.preventDefault();event.stopPropagation();}
  function unavailable(message){notice(message);}
  function copy(event) {
    if(event.type==='cut')stop(event);
    let before;try{before=state();}catch{stop(event);return;}
    if(!owned(before))return;
    if(event.type==='cut'&&!writable(before)){stop(event);return;}
    const range=before.selection;if(!range||range.length===0)return;
    stop(event);
    try {
      // The currently admitted plain subset has no block-format newline, so
      // O+Q+ correctCopyRange is identity and its line-format repair is empty.
      const ops=codec.decode(quill.getContents(range.index,range.length).ops);
      const payload=clipboard.copy(ops),written=[];
      for(const[type,text]of Object.entries(payload)) {
        try {event.clipboardData.setData(type,text);if(event.clipboardData.getData(type)===text)written.push(type);}catch{}
      }
      if(!written.length){unavailable('The clipboard did not accept this content. The draft is retained.');return;}
      if(event.type==='cut'&&unchanged(before)) {
        // Same original selection as the copy. Local failure above retains the
        // draft rather than spending a cut whose clipboard write is unknown.
        quill.deleteText(range.index,range.length);
        quill.setSelection(range.index);
      }
    } catch {unavailable('This selection could not be copied without changing its content.');}
  }
  function paste(event) {
    if(event.defaultPrevented)return;
    // This private editor disabled Quill's HTML importer. Never fall through to
    // contenteditable's browser paste on a pending/retired/refused operation.
    stop(event);
    let before;try{before=state();}catch{return;}
    if(!writable(before)||!before.selection)return;
    const data=event.clipboardData;if(!data)return;
    // O+Q+ shouldSkipOnPaste requires the companion plain flavor when custom
    // matchers are disabled, even if a typed flavor is also present.
    if(!(data.getData('text/plain')||data.getData('Text'))){stop(event);return;}
    stop(event);
    if(broker.pending()){unavailable('A clipboard observation is still pending.');return;}
    const selection=model.slugSelection(quill);
    if(before.slugsEnabled&&(selection.partial||selection.inside)){
      unavailable('Select the whole linked item to replace or remove it.');return;
    }
    try {
      const value=clipboard.read(type=>data.getData(type),{markdownMode:false,mirror:{}});
      if(!value)return;
      // Retained getPastedDelta returns typed Delta before consulting HTML.
      // Semantic Slack HTML still requires its separately typed importer.
      if(value.type!=='slack/texty') {
        const html=data.getData('slack/html')||data.getData('text/html');
        if(html&&(html.includes('data-stringify-')||html.includes('data-block-source')))
          throw Error('Semantic HTML importer unavailable');
      }
      const ops=model.document(value.ops,'PlainText');
      const needsObservation=ops.some(op=>op.attributes?.slackmention?.id||typeof op.insert==='object'&&op.insert.slackemoji);
      if(needsObservation){broker.begin(ops);return;}
      if(unchanged(before))insert({root,selection:before.selection,ops:model.plainPaste(ops,{}).ops});
    } catch {unavailable('This clipboard content is not supported by this editor yet.');}
  }
  listen('copy',copy);listen('cut',copy);listen('paste',paste);
  return Object.freeze({
    pending:()=>broker.pending(),
    request:()=>currentRequest?model.clone(currentRequest):null,
    changed:()=>broker.changed(),
    resolve(receipt) {
      if(!currentRequest||receipt?.request!==currentRequest.request)return false;
      // Exact positive appearances accompany the same original paste receipt;
      // broker validates all owner/selection/content and ordered row fields.
      try{return broker.resolve(receipt);}catch{
        unavailable('This clipboard content could not be inserted without changing its content.');return false;
      }
    },
    reject(request) {return currentRequest?.request===request&&broker.reject(request);},
    release(){closed=true;broker.cancel();cleanup.forEach(fn=>fn());},
  });
}
