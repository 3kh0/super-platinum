// Persistent identity custody, with short inert leases only around Rust writes.
async function workflowActivityExport(expected, channel) {
  const get = id => document.getElementById(id);
  const root = get('workflow-management'), activity = get('workflow-activity');
  const form = get('workflow-activity-filters'), action = get('workflow-activity-export');
  const notice = get('workflow-activity-export-native-notice');
  const input = get('workflow-activity-user-search'), state = root?.__workflowManagement;
  const dateWitness = workflowActivityDateWitness(expected, form);
  const keys = ['status','limit','user'];
  const fields = keys.map(key => get('workflow-activity-' + key));
  const nodes = [root,activity,form];
  const dates = workflowActivitySnapshot(expected);
  const sameDates = () => {
    const current = workflowActivitySnapshot(expected);
    return dates !== null && current !== null && dates.from === current.from && dates.to === current.to;
  };
  const inert = nodes.map(node => node?.inert);
  let held = null, ownedRead = null, invalid = false, custodyLost = false, observer, end;
  const ended = new Promise(resolve => { end = resolve; });
  const watchers = new Set();
  const usable = node => node?.isConnected && !node.validity?.badInput;
  const enabled = node => !node.disabled && !node.matches(':disabled');
  const sameRoot = () => root?.isConnected && get('workflow-management') === root
    && root.dataset.managementVisit === String(expected.visit)
    && root.dataset.managementDocument === String(expected.document)
    && root.dataset.managementRevision === String(expected.documentRevision);
  const sameNotice = () => notice?.isConnected && get('workflow-activity-export-native-notice') === notice
    && root?.contains(notice);
  const feedback = (text, refused = false) => {
    if (!sameRoot() || !sameNotice() || !state || root.__workflowManagement !== state) return;
    notice.textContent = text;
    if (refused) {
      const previous = Number.isSafeInteger(state.exportRefusals) && state.exportRefusals >= 0
        ? state.exportRefusals : 0;
      state.exportRefusals = Math.min(previous + 1, Number.MAX_SAFE_INTEGER);
    }
  };
  const identity = () => dateWitness?.current() && state && root.__workflowManagement === state
    && (!notice || sameNotice())
    && nodes.every(node => node?.isConnected && get(node.id) === node)
    && root.contains(activity) && activity.contains(form)
    && action?.isConnected && action.tagName === 'BUTTON' && get(action.id) === action
    && activity.contains(action) && !action.closest('fieldset[disabled]')
    && root.dataset.managementVisit === String(expected.visit)
    && root.dataset.managementDocument === String(expected.document)
    && root.dataset.managementRevision === String(expected.documentRevision)
    && form.dataset.revision === String(expected.filters?.revision)
    && get('workflow-activity-user-search') === input && input?.tagName === 'INPUT'
    && form.contains(input) && usable(input) && !input.closest('fieldset[disabled]')
    && input.dataset.generation === String(expected.memberGeneration)
    && typeof expected.search === 'string' && input.value === expected.search
    && fields.every((node,index) => usable(node) && enabled(node) && form.contains(node)
      && get('workflow-activity-' + keys[index]) === node
      && typeof expected.filters?.[keys[index]] === 'string' && node.value === expected.filters[keys[index]])
    && sameDates() && !state.composing && !document.querySelector('[role="dialog"][aria-modal="true"]');
  const matches = transition => !invalid && identity()
    && (!state.lease || state.lease === held)
    && nodes.every(node => held ? node.inert === true : !node.inert)
    && (ownedRead !== null
      ? activity.dataset.activityRead === ownedRead && (enabled(input) || input.disabled === true)
      : transition || (activity.dataset.activityRead === '0' && enabled(input) && enabled(action)));
  const send = value => channel.send({...value,dates});
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
      feedback('Finish editing the current filters before exporting. Your draft is unchanged.', true);
      send({serial:0,ok:false});
      await channel.recv(); // Preserve evaluator until Rust receives refusal.
      return;
    }
    observer = new MutationObserver(inspect);
    observer.observe(document.documentElement,{attributes:true,subtree:true,childList:true});
    events.forEach(event => root.addEventListener(event,inspect,true));
    const admitted = await lock(0);
    feedback(admitted ? '' : 'Finish editing the current filters before exporting. Your draft is unchanged.', !admitted);
    send({serial:0,ok:admitted});
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
