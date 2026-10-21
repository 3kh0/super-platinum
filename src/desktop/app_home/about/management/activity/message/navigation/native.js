// A semantic file label is not its textContent once it contains custom emoji.
// Capture the original typed fragments AND their nodes, never flatten an image.
function workflowActivityFilePaint(expected,node) {
  const f=expected.file;
  if (!f || !Number.isSafeInteger(f.part) || f.part < 0
      || node?.dataset.part !== String(f.part)
      || !Array.isArray(f.fragments) || !f.fragments.length) return null;
  const name=node.querySelector('.workflow-activity-file-name');
  if (name?.tagName !== 'SPAN' || !name.isConnected || name.parentElement !== node
      || name.getAttribute('data-file-label') !== f.label) return null;
  const parts=Array.from(name.childNodes);
  if (parts.length !== f.fragments.length) return null;
  const captured=[name];
  for (let i=0;i<parts.length;i++) {
    const part=parts[i], value=f.fragments[i];
    if (!value || part.nodeType !== 1 || part.tagName !== 'SPAN' || !part.isConnected
        || part.dataset.fileTitleKind !== value.kind || part.dataset.fileTitleIndex !== String(i)) return null;
    const children=Array.from(part.childNodes);
    captured.push(part,...children);
    if (value.kind === 'text' || value.kind === 'emoji') {
      if (typeof value.text !== 'string' || children.length !== 1 || children[0].nodeType !== 3
          || children[0].textContent !== value.text || part.textContent !== value.text
          || part.getAttribute('title') !== (value.kind === 'emoji' ? value.title : null)
          || (value.kind === 'emoji' && typeof value.title !== 'string')) return null;
    } else if (value.kind === 'image') {
      const image=children[0];
      if (children.length !== 1 || image?.nodeType !== 1 || image.tagName !== 'IMG' || !image.isConnected
          || part.getAttribute('title') !== null || image.childNodes.length !== 0
          || typeof value.src !== 'string' || !value.src.length
          || typeof value.alt !== 'string' || typeof value.title !== 'string'
          || image.getAttribute('src') !== value.src || image.getAttribute('alt') !== value.alt
          || image.getAttribute('title') !== value.title) return null;
    } else return null;
  }
  return captured;
}
function workflowActivityPaintNodes(expected,node) {
  return expected.file === undefined ? [] : workflowActivityFilePaint(expected,node);
}
// List links are a distinct original occurrence; IDs/labels never grant source authority.
function workflowActivityOccurrence(expected,node) {
  if (expected.file !== undefined) {
    const f=expected.file;
    return expected.channel === undefined && f && f.kind === 'list'
      && typeof f.id === 'string' && f.id.length > 0
      && typeof f.url === 'string' && f.url.length > 0
      && typeof f.label === 'string' && f.label.length > 0
      && node?.tagName === 'A' && node.dataset.file === f.id && node.dataset.fileKind === 'list'
      && node.getAttribute('href') === f.url
      && workflowActivityFilePaint(expected,node) !== null;
  }
  return node?.tagName === 'BUTTON' && typeof expected.channel === 'string'
    && expected.channel.length > 0 && node.dataset.channel === expected.channel;
}
// An exact rendered Activity detail owns this short-lived navigation lease.
// No controlled value, selection, focus, date interpretation or Rust state is written.
function workflowActivityChannelChain(value) {
  if (Number.isSafeInteger(value) && value > 0) return BigInt(value);
  if (typeof value === 'string' && /^[1-9][0-9]*$/.test(value)) return BigInt(value);
  return null;
}
// Channel and List chains come from the same app_home::next counter. Keep the
// latest owner after failure/settlement; an empty footer never revives old work.
function workflowActivityChannelNoticeAllows(expected, state, notice, key, nodes, held) {
  const chain=workflowActivityChannelChain(expected.chain), owner=state?.channelNavigationNotice;
  if (chain === null) return false;
  if (!owner) return true;
  const old=workflowActivityChannelChain(owner.chain);
  if (old === null || old > chain) return false;
  if (old < chain) return true;
  return owner.state === state && owner.key === key && owner.notice === notice
    && owner.nodes.length === nodes.length && owner.nodes.every((node,index)=>node === nodes[index])
    && (held || ['admitted','pending'].includes(owner.phase));
}
function workflowActivityChannelNoticeClaim(expected, state, notice, key, nodes, phase) {
  const chain=workflowActivityChannelChain(expected.chain);
  if (chain === null) return null;
  const previous=state.channelNavigationNotice;
  if (previous) {
    const old=workflowActivityChannelChain(previous.chain);
    if (old === null || old > chain) return null;
    if (old === chain) {
      if (previous.state !== state || previous.key !== key || previous.notice !== notice
          || previous.nodes.length !== nodes.length
          || !previous.nodes.every((node,index)=>node === nodes[index])) return null;
      if (!['admitted','pending'].includes(previous.phase)) return null;
      if (phase === 'refused') previous.phase='refused';
      return previous;
    }
  }
  const owner={chain:String(expected.chain),detail:String(expected.detail),phase,key,notice,nodes,state};
  state.channelNavigationNotice=owner;
  return owner;
}
function workflowActivityChannelFeedback(expected, message, phase='failure') {
  const root = document.getElementById('workflow-management');
  const detail = document.getElementById('workflow-activity-detail');
  const activity = detail?.closest('#workflow-activity');
  const notice = document.getElementById('workflow-activity-navigation-notice');
  const form = document.getElementById('workflow-activity-filters');
  const input = document.getElementById('workflow-activity-user-search');
  const button = document.getElementById(expected.button);
  const state = root?.__workflowManagement;
  const dateWitness = workflowActivityDateWitness(expected, form);
  const keys = ['status', 'limit', 'user'];
  const fields = keys.map(key => document.getElementById('workflow-activity-' + key));
  const usable = node => node?.isConnected && !node.disabled && !node.matches(':disabled') && !node.validity?.badInput;
  if (!dateWitness || typeof message !== 'string' || (!message && expected.file === undefined)
      || !root?.isConnected || !state || state.composing
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
      || !workflowActivityOccurrence(expected,button) || !usable(button) || !detail.contains(button)) return false;
  const key = JSON.stringify([expected.chain, expected.visit, expected.document, expected.revision, expected.detail,
    expected.filters?.revision, ...keys.map(k => expected.filters?.[k]), expected.filters?.from, expected.filters?.to, expected.filters?.date_origin, expected.filters?.default_from, expected.filters?.default_to, dateWitness?.epoch, expected.generation,
    expected.search, expected.button, expected.channel, expected.file]);
  const paint = workflowActivityPaintNodes(expected,button);
  const captured = [root, form, detail, button, input, dateWitness?.node, ...fields, ...(paint || [])];
  if (state.lease && (state.lease.activityChannelKey !== key
      || state.lease.activityChannelNodes?.length !== captured.length
      || !state.lease.activityChannelNodes.every((node, index) => node === captured[index]))) return false;
  {
    const owner=state.channelNavigationNotice;
    if (!owner || owner.state !== state || owner.chain !== String(expected.chain) || owner.key !== key || owner.notice !== notice
        || owner.nodes.length !== captured.length
        || !owner.nodes.every((node,index)=>node === captured[index])
        || owner.phase === 'refused') return false;
    if (phase === 'pending') {
      if (expected.file !== undefined || owner.phase !== 'admitted' || !state.lease) return false;
    } else if (phase !== 'failure') return false;
    // An admitted List clears earlier feedback without ending its own read.
    // Failure is terminal; its exact owner remains after the lease is released.
    if (!message) {
      if (owner.phase !== 'admitted' || !state.lease) return false;
    } else owner.phase=phase;
  }
  notice.textContent = message;
  return true;
}

function workflowActivityChannelPending(expected) {
  return expected.file === undefined
    && workflowActivityChannelFeedback(expected,'Opening conversation…','pending');
}
function workflowActivityChannelSettled(expected) {
  if (expected.file !== undefined) return false;
  const root=document.getElementById('workflow-management');
  const state=root?.__workflowManagement, owner=state?.channelNavigationNotice;
  if (!owner || owner.state !== state || owner.chain !== String(expected.chain) || owner.detail !== String(expected.detail)
      || !['admitted','pending'].includes(owner.phase)) return false;
  const wasPending=owner.phase === 'pending';
  owner.phase='settled';
  // No fresh source/buffer admission is needed to remove our own old status.
  // Never replace another notice or touch inert/lease ownership during cleanup.
  const detail=owner.nodes[2],notice=owner.notice;
  if (!wasPending || owner.nodes[0] !== root || !root?.isConnected
      || root.__workflowManagement !== state || !detail?.isConnected
      || document.getElementById('workflow-activity-detail') !== detail
      || detail.dataset.detailRevision !== owner.detail
      || document.getElementById('workflow-activity-navigation-notice') !== notice
      || !notice?.isConnected || !root.contains(notice)
      || notice.dataset.detailRevision !== owner.detail
      || notice.closest('#workflow-activity') !== detail.closest('#workflow-activity')
      || notice.textContent !== 'Opening conversation…') return false;
  notice.textContent='';
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
  const dateWitness = workflowActivityDateWitness(expected, form);
  const keys = ['status', 'limit', 'user'];
  const fields = keys.map(key => document.getElementById('workflow-activity-' + key));
  const paint = workflowActivityPaintNodes(expected,button);
  const captured = [root, form, detail, button, input, dateWitness?.node, ...fields, ...(paint || [])];
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
    if (workflowActivityChannelChain(expected.chain) !== null) {
      if (!workflowActivityChannelNoticeClaim(expected,state,notice,key,captured,'refused')) return;
    } else if (state.channelNavigationNotice) {
      state.channelNavigationNotice.phase='refused';
    }
    notice.textContent = expected.file ? 'Finish editing the current text before opening this List. Your draft is unchanged.' : 'Finish editing the current text before opening this conversation. Your draft is unchanged.';
    state.channelRefusals = (state.channelRefusals || 0) + 1;
  };
  const usable = node => node?.isConnected && !node.disabled && !node.matches(':disabled') && !node.validity?.badInput;
  const matches = held => dateWitness?.current() && validChain && scope() && state && !state.composing
    && workflowActivityChannelNoticeAllows(expected,state,
      document.getElementById('workflow-activity-navigation-notice'),key,captured,!!held)
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
    && workflowActivityOccurrence(expected,button) && usable(button) && detail.contains(button)
    && paint !== null && (() => {
      const current=workflowActivityPaintNodes(expected,button);
      return current !== null && current.length === paint.length && current.every((node,index)=>node === paint[index]);
    })()
    && document.getElementById(expected.button) === button
    && [root, form, detail].every(node => held ? node.inert === true : !node.inert);
  const key = JSON.stringify([expected.chain, expected.visit, expected.document, expected.revision, expected.detail,
    expected.filters?.revision, ...keys.map(k => expected.filters?.[k]), expected.filters?.from, expected.filters?.to, expected.filters?.date_origin, expected.filters?.default_from, expected.filters?.default_to, dateWitness?.epoch, expected.generation,
    expected.search, expected.button, expected.channel, expected.file]);
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
    observer.observe(document.documentElement, {childList:true, subtree:true, attributes:true, characterData:expected.file !== undefined});
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
    {
      const notice=document.getElementById('workflow-activity-navigation-notice');
      if (!notice?.isConnected || !root.contains(notice)
          || notice.closest('#workflow-activity') !== activity
          || notice.dataset.detailRevision !== String(expected.detail)
          || !workflowActivityChannelNoticeClaim(expected,state,notice,key,captured,'admitted')) {
        refuse();
        channel.send(false);
        return false;
      }
    }
    // A newly admitted List attempt owns this footer. Clear earlier failures
    // only after the exact original occurrence and native buffers are locked.
    if (expected.file !== undefined) workflowActivityChannelFeedback(expected, '');
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
