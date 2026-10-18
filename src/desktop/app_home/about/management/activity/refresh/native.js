// Persistent identity custody, with short inert leases only around Rust writes.
async function workflowActivityRefresh(expected, channel) {
  const get = id => document.getElementById(id);
  const root = get('workflow-management'), activity = get('workflow-activity');
  const form = get('workflow-activity-filters'), detail = get('workflow-activity-detail');
  const input = get('workflow-activity-user-search'), state = root?.__workflowManagement;
  const keys = ['status','from','to','limit','user'];
  const fields = keys.map(key => get('workflow-activity-' + key));
  const nodes = [root,activity,form,detail];
  const inert = nodes.map(node => node?.inert);
  let held = null, ownedRead = null, invalid = false, custodyLost = false, observer, end;
  const ended = new Promise(resolve => { end = resolve; });
  const watchers = new Set();
  const usable = node => node?.isConnected && !node.validity?.badInput;
  const enabled = node => !node.disabled && !node.matches(':disabled');
  const identity = () => state && root.__workflowManagement === state
    && nodes.every(node => node?.isConnected && get(node.id) === node)
    && root.contains(activity) && activity.contains(form) && activity.contains(detail)
    && root.dataset.managementVisit === String(expected.visit)
    && root.dataset.managementDocument === String(expected.document)
    && root.dataset.managementRevision === String(expected.documentRevision)
    && detail.dataset.detailRevision === String(expected.detailRevision)
    && form.dataset.revision === String(expected.filters?.revision)
    && get('workflow-activity-user-search') === input && input?.tagName === 'INPUT'
    && form.contains(input) && usable(input) && !input.closest('fieldset[disabled]')
    && input.dataset.generation === String(expected.memberGeneration)
    && typeof expected.search === 'string' && input.value === expected.search
    && fields.every((node,index) => usable(node) && enabled(node) && form.contains(node)
      && get('workflow-activity-' + keys[index]) === node
      && typeof expected.filters?.[keys[index]] === 'string' && node.value === expected.filters[keys[index]])
    && !state.composing && !document.querySelector('[role="dialog"][aria-modal="true"]');
  const matches = transition => !invalid && identity()
    && (!state.lease || state.lease === held)
    && nodes.every(node => held ? node.inert === true : !node.inert)
    && (ownedRead !== null
      ? activity.dataset.activityRead === ownedRead && (enabled(input) || input.disabled === true)
      : transition || (activity.dataset.activityRead === '0' && enabled(input)));
  const send = value => channel.send(expected.trace ? {...value,debug:{invalid,custodyLost,held:held?.serial??null,ownedRead,identity:!!identity(),read:activity?.dataset?.activityRead,lease:!!state?.lease,ours:state?.lease===held,inert:nodes.map(n=>n?.inert),searchEqual:input?.value===expected.search,filtersEqual:fields.map((n,i)=>n?.value===expected.filters?.[keys[i]]),enabled:!!input&&enabled(input)}} : value);
  const inspect = event => {
    // Capture runs before the management composition listener updates its state.
    if (event?.type === 'compositionstart') { invalid = true; custodyLost = true; }
    // Only initial admission may span Rust's exact read-marker/disabled update.
    if (!matches(held?.serial === 0 && ownedRead === null)) {
      invalid = true;
      // The final protected Rust write retires only this read. It still makes
      // this evaluator unusable for another lock; a terminal close may merely
      // acknowledge cleanup. Other custody loss remains permanently recorded.
      const ownRetirement = held && ownedRead !== null && identity()
        && state.lease === held && nodes.every(node => node.inert === true)
        && activity.dataset.activityRead === '0' && (enabled(input) || input.disabled === true);
      if (!ownRetirement) custodyLost = true;
    }
    watchers.forEach(check => check());
  };
  const events = ['input','change','compositionstart'];
  const paint = async (milliseconds, resultOnTimeout) => {
    let frame, timer;
    try {
      return await Promise.race([ended.then(() => false), new Promise(resolve => {
        frame = requestAnimationFrame(() => resolve(true));
        timer = setTimeout(() => resolve(resultOnTimeout), milliseconds);
      })]);
    } finally { cancelAnimationFrame(frame); clearTimeout(timer); }
  };
  const unwatch = () => {
    observer?.disconnect();
    events.forEach(event => root?.removeEventListener(event,inspect,true));
  };
  const unlock = async (terminal = false) => {
    if (!held) return;
    // Keep the previous controlled commit protected until its render chance.
    await paint(200, true);
    if (terminal) unwatch();
    if (state.lease === held) {
      state.lease = null;
      nodes.forEach((node,index) => { node.inert = inert[index]; });
    }
    held = null;
  };
  const lock = async serial => {
    if (held || !matches(false)) return false;
    held = {serial, close: () => { invalid = true; end(); }};
    state.lease = held;
    nodes.forEach(node => { node.inert = true; });
    if (!await paint(1000, false) || !matches(false)) {
      invalid = true;
      await unlock();
      return false;
    }
    return true;
  };
  const activate = async read => {
    if (!held || held.serial !== 0 || ownedRead !== null || !Number.isSafeInteger(read) || read <= 0) return false;
    let timer, inspectRead;
    try {
      const ready = await Promise.race([ended.then(() => false), new Promise(resolve => {
        inspectRead = () => {
          if (!matches(true)) return resolve(false);
          if (activity.dataset.activityRead === String(read)) resolve(true);
        };
        watchers.add(inspectRead);
        timer = setTimeout(() => resolve(false), 1000);
        inspectRead();
      })]);
      if (!ready) { invalid = true; return false; }
      ownedRead = String(read);
      return matches(false);
    } finally { clearTimeout(timer); watchers.delete(inspectRead); }
  };
  const retire = async () => {
    let timer, inspectRead;
    try {
      return await Promise.race([ended.then(() => false), new Promise(resolve => {
        inspectRead = () => {
          const read = activity.dataset.activityRead;
          if (custodyLost || !identity() || !held || state.lease !== held
            || !nodes.every(node => node.inert === true)
            || (read !== ownedRead && read !== '0')) return resolve(false);
          if (read === '0' && enabled(input)) resolve(true);
        };
        watchers.add(inspectRead);
        timer = setTimeout(() => resolve(false), 1000);
        inspectRead();
      })]);
    } finally { clearTimeout(timer); watchers.delete(inspectRead); }
  };
  try {
    if (!matches(false)) {
      send({serial:0,ok:false});
      await channel.recv(); // Preserve evaluator until Rust receives refusal.
      return;
    }
    observer = new MutationObserver(inspect);
    observer.observe(document.documentElement,{attributes:true,subtree:true,childList:true});
    events.forEach(event => root.addEventListener(event,inspect,true));
    send({serial:0,ok:await lock(0)});
    while (true) {
      const command = await Promise.race([channel.recv(),ended.then(() => null)]);
      if (!command) break; // RAII session cancellation.
      if (command.kind === 'close') {
        const ours = ownedRead !== null && held && held.serial === command.lease
          && state.lease === held;
        if (!ours) { send({serial:command.serial,ok:false}); continue; }
        // Rust has retired this read, but its DOM commit may arrive after
        // the next frame. Keep the original lease until both controlled
        // retirement markers have arrived; a frame alone is not evidence.
        let cancelled = false;
        const cancel = () => { cancelled = true; end(); };
        const cancellation = channel.recv().then(cancel, cancel);
        const retired = await retire();
        await unlock(true);
        const ok = retired && !cancelled && !custodyLost && identity() && state.lease === null
          && nodes.every((node,index) => node.inert === inert[index])
          && activity.dataset.activityRead === '0' && enabled(input);
        send({serial:command.serial,ok});
        // Keep this evaluator alive until Rust receives cleanup completion.
        // It no longer observes or owns nodes before a successor can capture.
        await cancellation;
        break;
      }
      if (command.kind === 'unlock') {
        if (held?.serial === command.serial) await unlock();
        continue;
      }
      const ok = command.kind === 'activate' ? await activate(command.read)
        : command.kind === 'lock' && ownedRead !== null ? await lock(command.serial) : false;
      send({serial:command.serial,ok});
    }
  } finally {
    unwatch();
    await unlock();
  }
}
