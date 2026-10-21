// Installed only by the protected, disconnected late-capture fixture script.
// The real native bridge still creates the snapshot and owns all acknowledgments.
function canvasLateCaptureBridge(options, bridge, environment = globalThis) {
  const clone = value => JSON.parse(JSON.stringify(value));
  const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
  const owner = {scope:options.scope, mount:options.mount, generation:options.generation};
  if (typeof owner.scope !== 'string' || !owner.scope || typeof owner.mount !== 'string'
      || !owner.mount || !Number.isSafeInteger(owner.generation) || owner.generation < 0)
    throw Error('Invalid late-capture owner');
  const key = '__superPlatinumCanvasLateCapture';
  let registry = environment[key];
  if (!registry) {
    const entries = new Map();
    const find = value => {
      const entry = entries.get(value?.mount);
      if (!entry || !same(entry.owner, {scope:value.scope,mount:value.mount,generation:value.generation}))
        throw Error('Original late-capture owner unavailable');
      return entry;
    };
    registry = {
      entries,
      arm(value) { return find(value).arm(); },
      state(value) { return find(value).state(); },
      release(value) { return find(value).release(value); }
    };
    environment[key] = registry;
  }
  if (!(registry.entries instanceof Map) || registry.entries.has(owner.mount)
      || registry.entries.size >= 16) throw Error('Duplicate or exhausted late-capture registry');
  let phase = 'idle', closed = false, heldPayload = null, original = null;
  let sends = 0, finalSends = 0, releaseCount = 0, failure = null, timer = null;
  let armedHolder = null, heldAt = null, releasedAt = null;
  let resume = null, rejectHeld = null;
  const finishRequests = [];
  const state = () => clone({...owner,phase,closed,heldPayload,sends,finalSends,
    releaseCount,failure,finishRequests,heldAt,releasedAt});
  const clear = () => { if(timer!==null)environment.clearTimeout(timer); timer=null; };
  const fail = message => {
    clear();
    if (phase === 'failed') return;
    failure=message;phase='failed';
    const reject=rejectHeld;resume=null;rejectHeld=null;
    if(reject)reject(Error(message));
  };
  const currentHolder = () => {
    const holder=environment.__superPlatinumCanvasReplyCaptures?.get(owner.mount);
    return holder && (!armedHolder || holder===armedHolder) && holder.running && holder.scope===owner.scope
      && holder.generation===owner.generation;
  };
  const entry = {
    owner,state,
    arm() {
      if(closed || phase!=='idle' || !currentHolder())
        throw Error('Original late-capture mount cannot be armed');
      armedHolder=environment.__superPlatinumCanvasReplyCaptures.get(owner.mount);
      phase='armed';
      timer=environment.setTimeout(()=>fail('Late final capture fixture deadline exceeded'),60000);
      return state();
    },
    release(value) {
      if(closed || phase!=='held' || !resume || !currentHolder()
          || value.request!==heldPayload.request || value.sequence!==heldPayload.sequence
          || !Number.isSafeInteger(value.sequence) || !same(original,heldPayload))
        throw Error('Original late final capture cannot be released');
      clear();phase='released';releaseCount++;releasedAt=environment.performance.now();
      const done=resume;resume=null;rejectHeld=null;done();
      return state();
    }
  };
  registry.entries.set(owner.mount,entry);
  return {
    async send(value) {
      if(closed || phase==='failed')throw Error(failure || 'Late-capture bridge closed');
      if(value?.kind==='capture' && value.finalCapture===true && heldPayload!==null)
        throw Error('Original final capture already held or delivered');
      if(value?.kind==='capture' && value.finalCapture===true && phase==='armed') {
        if(value.scope!==owner.scope || value.mount!==owner.mount
            || value.generation!==owner.generation || !Number.isSafeInteger(value.sequence)
            || value.sequence<1 || typeof value.request!=='string' || !value.request
            || !Array.isArray(value.blocks)) {
          fail('Original final capture identity changed');
          throw Error(failure);
        }
        original=value;heldPayload=clone(value);phase='held';heldAt=environment.performance.now();
        await new Promise((resolve,reject)=>{resume=resolve;rejectHeld=reject;});
        if(closed || phase!=='released' || !same(value,heldPayload)) {
          fail('Original held final capture changed');
          throw Error(failure);
        }
        sends++;finalSends++;
        try { await bridge.send(value);phase='delivered'; }
        catch(error) { fail('Original final capture send failed');throw error; }
        return;
      }
      // Every ordinary send remains the real producer's unchanged object.
      sends++;
      return await bridge.send(value);
    },
    async recv() {
      const value=await bridge.recv();
      if(value?.kind==='finish') {
        if(finishRequests.length>=4)throw Error('Late-capture command evidence exhausted');
        finishRequests.push(clone(value));
      }
      return value;
    },
    close() {
      if(closed)return;
      if(phase==='armed' || phase==='held' || phase==='released')
        fail('Original late-capture bridge closed before delivery');
      clear();closed=true;
    }
  };
}
