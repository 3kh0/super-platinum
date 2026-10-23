// Protected fixture only: delay the real producer's objects, never synthesize capture.
function canvasReplacementBridge(options, bridge, environment = globalThis) {
  const clone = x => JSON.parse(JSON.stringify(x));
  const same = (a,b) => JSON.stringify(a) === JSON.stringify(b);
  const owner = {scope:options.scope,mount:options.mount,generation:options.generation};
  if (!owner.scope || !owner.mount || !Number.isSafeInteger(owner.generation)) throw Error('Invalid replacement owner');
  const entries = environment.__canvasReplacementCaptures ??= new Map();
  if (!(entries instanceof Map) || entries.has(owner.mount) || entries.size >= 8) throw Error('Replacement registry exhausted');
  let phase='idle',closed=false,holder=null,pending=null,timer=null,failure=null;
  const held=[],delivered=[];
  const current = () => {
    const h=environment.__superPlatinumCanvasReplyCaptures?.get(owner.mount);
    return h && (!holder || h===holder) && h.running && h.scope===owner.scope && h.generation===owner.generation;
  };
  const clear = () => {if(timer!==null)environment.clearTimeout(timer);timer=null;};
  const fail = why => {clear();failure=why;phase='failed';const p=pending;pending=null;p?.reject(Error(why));};
  const state = () => clone({...owner,phase,closed,failure,held,delivered});
  const entry = {
    owner,state,
    arm() {
      if(closed || phase!=='idle' || !current())throw Error('Original replacement capture unavailable');
      holder=environment.__superPlatinumCanvasReplyCaptures.get(owner.mount);
      phase='armed';return state();
    },
    release(value) {
      if(closed || !pending || !current() || value.sequence!==pending.copy.sequence
          || value.mount!==owner.mount || value.scope!==owner.scope || value.generation!==owner.generation
          || !same(pending.original,pending.copy))throw Error('Original replacement envelope changed');
      clear();const p=pending;pending=null;phase=p.copy.detached?'detached-released':'edited-released';p.resolve();
      return state();
    }
  };
  entries.set(owner.mount,entry);
  return {
    async send(value) {
      if(closed || failure)throw Error(failure || 'Replacement bridge closed');
      const shouldHold=value?.kind==='capture' && (phase==='armed' || phase==='edited-delivered');
      if(shouldHold) {
        const detached=held.length===1;
        if(value.scope!==owner.scope || value.mount!==owner.mount || value.generation!==owner.generation
            || !Number.isSafeInteger(value.sequence) || value.sequence<1 || !Array.isArray(value.blocks)
            || value.detached!==detached || value.finalCapture!==false || value.request!==null
            || !current() || detached && holder.root.isConnected)throw Error('Unexpected original replacement capture');
        const copy=clone(value);held.push(copy);phase=detached?'detached-held':'edited-held';
        await new Promise((resolve,reject)=>{
          pending={original:value,copy,resolve,reject};
          timer=environment.setTimeout(()=>fail('Original replacement capture deadline exceeded'),60000);
        });
        if(closed || failure || !same(copy,value))throw Error('Held original envelope changed');
        await bridge.send(value);delivered.push(clone(value));phase=detached?'detached-delivered':'edited-delivered';
        return;
      }
      return await bridge.send(value);
    },
    recv() {return bridge.recv();},
    close() {if(closed)return;if(pending)fail('Original replacement bridge closed while held');clear();closed=true;}
  };
}
