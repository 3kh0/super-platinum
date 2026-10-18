// An exact rendered Activity detail owns this short-lived navigation lease.
// No controlled value, selection, focus, date interpretation or Rust state is written.
function workflowActivityChannelFeedback(expected, message) {
  const root = document.getElementById('workflow-management');
  const detail = document.getElementById('workflow-activity-detail');
  const activity = detail?.closest('#workflow-activity');
  const notice = document.getElementById('workflow-activity-navigation-notice');
  const form = document.getElementById('workflow-activity-filters');
  const input = document.getElementById('workflow-activity-user-search');
  const button = document.getElementById(expected.button);
  const state = root?.__workflowManagement;
  const keys = ['status', 'from', 'to', 'limit', 'user'];
  const fields = keys.map(key => document.getElementById('workflow-activity-' + key));
  const usable = node => node?.isConnected && !node.disabled && !node.matches(':disabled') && !node.validity?.badInput;
  if (typeof message !== 'string' || !message || !root?.isConnected || !state || state.composing
      || !detail?.isConnected || !root.contains(detail) || !activity?.isConnected
      || !notice?.isConnected || !root.contains(notice) || notice.closest('#workflow-activity') !== activity
      || root.dataset.managementVisit !== String(expected.visit)
      || root.dataset.managementDocument !== String(expected.document)
      || root.dataset.managementRevision !== String(expected.revision)
      || detail.dataset.detailRevision !== String(expected.detail)
      || notice.dataset.detailRevision !== String(expected.detail)
      || document.querySelector('[role="dialog"][aria-modal="true"]')
      || !form?.isConnected || !root.contains(form) || form.dataset.revision !== String(expected.filters?.revision)
      || input?.tagName !== 'INPUT' || !usable(input) || !form.contains(input)
      || input.dataset.generation !== String(expected.generation) || input.value !== expected.search
      || fields.some((node, index) => !usable(node) || !form.contains(node)
        || typeof expected.filters?.[keys[index]] !== 'string' || node.value !== expected.filters[keys[index]])
      || button?.tagName !== 'BUTTON' || !usable(button) || !detail.contains(button)
      || typeof expected.channel !== 'string' || !expected.channel || button.dataset.channel !== expected.channel) return false;
  const key = JSON.stringify([expected.chain, expected.visit, expected.document, expected.revision, expected.detail,
    expected.filters?.revision, ...keys.map(k => expected.filters?.[k]), expected.generation,
    expected.search, expected.button, expected.channel]);
  const captured = [root, form, detail, button, input, ...fields];
  if (state.lease && (state.lease.activityChannelKey !== key
      || state.lease.activityChannelNodes?.length !== captured.length
      || !state.lease.activityChannelNodes.every((node, index) => node === captured[index]))) return false;
  notice.textContent = message;
  return true;
}

async function workflowActivityChannelLock(expected, channel) {
  const root = document.getElementById('workflow-management');
  const form = document.getElementById('workflow-activity-filters');
  const detail = document.getElementById('workflow-activity-detail');
  const activity = detail?.closest('#workflow-activity');
  const button = document.getElementById(expected.button);
  const state = root?.__workflowManagement;
  const input = document.getElementById('workflow-activity-user-search');
  const keys = ['status', 'from', 'to', 'limit', 'user'];
  const fields = keys.map(key => document.getElementById('workflow-activity-' + key));
  const captured = [root, form, detail, button, input, ...fields];
  const validChain = (typeof expected.chain === 'string' && expected.chain.length > 0)
    || (Number.isSafeInteger(expected.chain) && expected.chain > 0);
  const scope = () => root?.isConnected && detail?.isConnected
    && document.getElementById('workflow-management') === root
    && document.getElementById('workflow-activity-detail') === detail
    && root.dataset.managementVisit === String(expected.visit)
    && root.dataset.managementDocument === String(expected.document)
    && root.dataset.managementRevision === String(expected.revision)
    && detail.dataset.detailRevision === String(expected.detail)
    && root.contains(detail) && root.__workflowManagement === state;
  const refuse = () => {
    if (!state || !scope()) return;
    const notice = document.getElementById('workflow-activity-navigation-notice');
    if (!notice?.isConnected || !root.contains(notice)
        || !activity?.isConnected || detail.closest('#workflow-activity') !== activity
        || notice.closest('#workflow-activity') !== activity
        || notice.dataset.detailRevision !== String(expected.detail)) return;
    notice.textContent = 'Finish editing the current text before opening this conversation. Your draft is unchanged.';
    state.channelRefusals = (state.channelRefusals || 0) + 1;
  };
  const usable = node => node?.isConnected && !node.disabled && !node.matches(':disabled') && !node.validity?.badInput;
  const matches = held => validChain && scope() && state && !state.composing
    && (!state.lease || (held && state.lease === held))
    && !document.querySelector('[role="dialog"][aria-modal="true"]')
    && form?.isConnected && root.contains(form)
    && document.getElementById('workflow-activity-filters') === form
    && form.dataset.revision === String(expected.filters?.revision)
    && input?.tagName === 'INPUT' && usable(input) && form.contains(input)
    && document.getElementById('workflow-activity-user-search') === input
    && input.dataset.generation === String(expected.generation)
    && typeof expected.search === 'string' && input.value === expected.search
    && fields.every((node, index) => usable(node) && form.contains(node)
      && document.getElementById('workflow-activity-' + keys[index]) === node
      && typeof expected.filters?.[keys[index]] === 'string'
      && node.value === expected.filters[keys[index]])
    && button?.tagName === 'BUTTON' && usable(button) && detail.contains(button)
    && document.getElementById(expected.button) === button
    && typeof expected.channel === 'string' && expected.channel.length > 0
    && button.dataset.channel === expected.channel
    && [root, form, detail].every(node => held ? node.inert === true : !node.inert);
  const key = JSON.stringify([expected.chain, expected.visit, expected.document, expected.revision, expected.detail,
    expected.filters?.revision, ...keys.map(k => expected.filters?.[k]), expected.generation,
    expected.search, expected.button, expected.channel]);
  const previous = state?.lease;
  // A fast read can return before the renderer receives Rust's release message.
  // Only the SAME per-open chain may await its exact original lease's cleanup;
  // another click, replaced node or changed buffer must refuse, never take over.
  if (previous?.activityChannelKey === key
      && previous.activityChannelNodes?.every((node, index) => node === captured[index])
      && previous.activityChannelNodes.length === captured.length
      && previous.done && matches(previous)) {
    let deadline;
    const completed = await Promise.race([
      previous.done.then(() => true),
      new Promise(resolve => { deadline = setTimeout(() => resolve(false), 1000); }),
    ]);
    clearTimeout(deadline);
    if (!completed) {
      refuse();
      try { channel.send(false); } catch {}
      return false;
    }
  }
  let lease;
  if (!matches(null)) {
    refuse();
    try { channel.send(false); } catch {}
    return false;
  }
  const nodes = [root, form, detail];
  const inert = nodes.map(node => node.inert);
  let close, completed, observer, timer, frame, acknowledged = false;
  const closed = new Promise(resolve => { close = resolve; });
  const done = new Promise(resolve => { completed = resolve; });
  lease = { close: () => close('removed'), activityChannelKey:key, activityChannelNodes:captured, done };
  const inspect = () => { if (!matches(lease)) close('changed'); };
  const events = ['input', 'change', 'compositionstart'];
  try {
    state.lease = lease;
    nodes.forEach(node => { node.inert = true; });
    observer = new MutationObserver(inspect);
    observer.observe(document.documentElement, {childList:true, subtree:true, attributes:true});
    events.forEach(event => root.addEventListener(event, inspect, true));
    // Start receiving before acknowledgment so Rust timeout/cancellation cannot
    // leave an unacknowledged renderer lock waiting for a backgrounded frame.
    const released = Promise.resolve().then(() => channel.recv()).then(() => 'release', () => 'channel-error');
    const painted = new Promise(resolve => {
      frame = requestAnimationFrame(() => resolve('frame'));
      timer = setTimeout(() => resolve('timeout'), 2000);
    });
    const ready = await Promise.race([painted, closed, released]);
    clearTimeout(timer); cancelAnimationFrame(frame);
    if (ready !== 'frame' || !matches(lease)) {
      refuse();
      channel.send(false);
      return false;
    }
    channel.send(true);
    acknowledged = true;
    // There is no post-ack unlock timer: Rust retains the lease until its final
    // synchronous commit/refusal, and Release drop also signals this receiver.
    await Promise.race([released, closed]);
    return true;
  } catch {
    if (!acknowledged) refuse();
    return false;
  } finally {
    clearTimeout(timer); cancelAnimationFrame(frame);
    observer?.disconnect();
    events.forEach(event => root.removeEventListener(event, inspect, true));
    if (acknowledged) {
      // Keep the lock through the next render opportunity after Rust commits.
      // The bounded fallback applies only AFTER release, never while Rust owns it.
      await new Promise(resolve => {
        frame = requestAnimationFrame(resolve);
        timer = setTimeout(resolve, 200);
      });
      clearTimeout(timer); cancelAnimationFrame(frame);
    }
    if (state.lease === lease) {
      state.lease = null;
      // Touch only the original captured nodes, including detached originals;
      // never restore a property on a replacement or controlled input buffer.
      nodes.forEach((node, index) => { node.inert = inert[index]; });
    }
    completed();
  }
}
