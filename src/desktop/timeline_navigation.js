// Evaluated with a captured native navigation scope. Only a mounted target can
// consume the pending anchor; old virtual rows are not evidence of success.
(async ({scope, selector, latestId, inputGeneration, rootId = 'message-timeline'}) => {
  let cancelled = false;
  const note = (event, detail) => (rootId === 'message-timeline' ? window.superPlatinumTimelineAnchor : window.superPlatinumThreadAnchor)?.note(event, detail);
  note('navigation-start', {selector, latestId});
  const cancel = event => {
    if (event.target.closest?.(`#${rootId}`)) {cancelled = true;note('navigation-cancel', event.type);}
  };
  for (const type of ['wheel', 'touchmove', 'keydown', 'pointerdown']) document.addEventListener(type, cancel, {passive:true});
  // WebKit can suspend animation frames for an occluded native window. Keep
  // navigation bounded so it also releases the awaiting history refresh.
  const nextFrame = () => new Promise(resolve => {
    let frame;
    const timer = setTimeout(() => { cancelAnimationFrame(frame); resolve(); }, 50);
    frame = requestAnimationFrame(() => { clearTimeout(timer); resolve(); });
  });
  try {
    for (let attempt = 0; attempt < 8; attempt++) {
      await nextFrame();
      if (cancelled) return false;
      const root = document.getElementById(rootId);
      if (!root || root.dataset.timelineScope !== scope) continue;
      if (inputGeneration !== undefined) {
        const anchor = rootId === 'message-timeline' ? window.superPlatinumTimelineAnchor : window.superPlatinumThreadAnchor;
        if (!anchor) continue;
        if (anchor.inputGeneration() !== inputGeneration) {
          note('navigation-stale-input', {expected:inputGeneration, actual:anchor.inputGeneration()});
          return false;
        }
      }
      const target = selector ? root.querySelector(selector) :
        [...root.querySelectorAll('[data-message-id]')].find(row => row.dataset.messageId === latestId);
      if (!target) continue;
      note('navigation-apply', {selector, latestId});
      if (selector) target.scrollIntoView({block:'center'});
      else root.scrollTop = root.scrollHeight;
      // ResizeObserver can run before the ensuing scroll event. Retire the old
      // reading position now, so it cannot undo this navigation in that gap.
      (rootId === 'message-timeline' ? window.superPlatinumTimelineAnchor : window.superPlatinumThreadAnchor)?.capture();
      return true;
    }
    return false;
  } finally {
    for (const type of ['wheel', 'touchmove', 'keydown', 'pointerdown']) document.removeEventListener(type, cancel);
  }
})
