// A finite, renderer-owned observation attempt, not a clipboard validator or
// network client. The caller supplies current() only from its original model.
// Kept unmounted until the parent owns matching typed request/response handling.
function listCommentDeltaPaste({model,current,insert,onRequest,onRetired,
    setTimeout,clearTimeout,makeId,timeoutMs=5000}) {
  if(!Number.isInteger(timeoutMs)||timeoutMs<1||timeoutMs>5000)throw Error('Invalid paste deadline');
  let pending=null;
  const equal=(a,b)=>JSON.stringify(a)===JSON.stringify(b);
  function admitted(state) {
    return state && state.root && state.connected===true && state.owned===true
      && state.disabled===false && state.frozen===false && state.composing===false
      && state.repairPending===false && typeof state.scope==='string' && !!state.scope
      && typeof state.generation==='string' && /^(0|[1-9][0-9]*)$/.test(state.generation)
      && Number.isSafeInteger(state.revision) && state.revision>=0
      && Number.isSafeInteger(state.selectionRevision) && state.selectionRevision>=0
      && Number.isSafeInteger(state.selection?.index) && state.selection.index>=0
      && Number.isSafeInteger(state.selection?.length) && state.selection.length>=0;
  }
  function same(state,held) {
    return admitted(state) && state.root===held.root && state.scope===held.scope
      && state.generation===held.generation && state.revision===held.revision
      && state.selectionRevision===held.selectionRevision
      && equal(state.selection,held.selection) && equal(state.ops,held.ops);
  }
  function retire(reason) {
    const held=pending;if(!held)return false;
    pending=null;clearTimeout(held.timer);onRetired({request:held.id,reason});return true;
  }
  return Object.freeze({
    begin(rawOps) {
      if(pending)return null;
      const state=current();if(!admitted(state))return null;
      // Only already implemented native types are admitted by this prerequisite.
      // A later portable/HTML successor must extend the exact typed contract.
      const ops=model.document(rawOps,'PlainText');
      const ids=[],emojiNames=[],seenIds=new Set(),seenEmoji=new Set();
      for(const op of ops) {
        const id=op.attributes?.slackmention?.id;
        if(typeof id==='string'&&id&&!seenIds.has(id)){seenIds.add(id);ids.push(id);}
        const emoji=op.insert?.slackemoji?.text;
        if(typeof emoji==='string'&&!seenEmoji.has(emoji)){seenEmoji.add(emoji);emojiNames.push(emoji);}
      }
      const id=makeId();if(typeof id!=='string'||!id)throw Error('Paste identity unavailable');
      const held={root:state.root,scope:state.scope,generation:state.generation,
        revision:state.revision,selectionRevision:state.selectionRevision,
        selection:model.clone(state.selection),ops:model.clone(state.ops),
        id,rawOps:ops,ids,emojiNames,timer:null};
      pending=held;
      held.timer=setTimeout(()=>{if(pending===held)retire('timeout');},timeoutMs);
      try {
        onRequest({request:id,scope:held.scope,generation:held.generation,
          revision:held.revision,selectionRevision:held.selectionRevision,
          selection:model.clone(held.selection),ops:model.clone(ops),mentionIds:[...ids],emojiNames:[...emojiNames]});
      } catch(error) { retire('request-failed');throw error; }
      return id;
    },
    resolve(receipt) {
      const held=pending;
      // A response belonging to an older/foreign request cannot retire the one
      // currently owned by this editor.
      if(!held||receipt?.request!==held.id)return false;
      if(receipt.scope!==held.scope||receipt.generation!==held.generation
          ||receipt.revision!==held.revision||receipt.selectionRevision!==held.selectionRevision
          ||!equal(receipt.selection,held.selection)||!equal(receipt.ops,held.rawOps)) {
        retire('receipt-changed');return false;
      }
      if(!same(current(),held)){retire('editor-changed');return false;}
      const rows=receipt.validity;
      if(!Array.isArray(rows)||rows.length!==held.ids.length
          ||rows.some((row,index)=>!Array.isArray(row)||row.length!==2
            ||row[0]!==held.ids[index]||typeof row[1]!=='boolean')) {
        retire('validity-unavailable');return false;
      }
      const validity=Object.create(null);
      for(const [id,value]of rows)validity[id]=value;
      const emoji=receipt.emoji;
      if(!Array.isArray(emoji)||emoji.length!==held.emojiNames.length
          ||emoji.some((row,index)=>!row||row.name!==held.emojiNames[index])) {
        retire('emoji-unavailable');return false;
      }
      const prepared=model.plainPaste(held.rawOps,validity);
      // No await between this final original-node check and insertion. The
      // supplied inserter must preserve that same synchronous ownership fence.
      if(!same(current(),held)){retire('editor-changed');return false;}
      pending=null;clearTimeout(held.timer);
      let inserted=false;
      try {
        inserted=insert({root:held.root,selection:model.clone(held.selection),ops:prepared.ops,
          originalOps:model.clone(held.rawOps),emoji:model.clone(emoji),request:held.id})===true;
        return inserted;
      } finally {onRetired({request:held.id,reason:inserted?'inserted':'insertion-refused'});}
    },
    changed() {if(pending&&!same(current(),pending))retire('editor-changed');},
    reject(request) {return pending?.id===request?retire('host-unavailable'):false;},
    cancel() {return retire('cancelled');},
    pending() {return !!pending;},
  });
}
