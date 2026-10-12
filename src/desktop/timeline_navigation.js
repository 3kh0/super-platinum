// Evaluated with a captured native navigation scope. Only a mounted target can
// consume the pending anchor; old virtual rows are not evidence of success.
(async ({scope, selector, latestId, rootId = 'message-timeline'}) => {
  let cancelled = false;
  const cancel = event => {
    if (event.target.closest?.(`#${rootId}`)) cancelled = true;
  };
  for (const type of ['wheel', 'touchmove', 'keydown']) document.addEventListener(type, cancel, {passive:true});
  try {
    for (let attempt = 0; attempt < 8; attempt++) {
      await new Promise(resolve => requestAnimationFrame(resolve));
      if (cancelled) return false;
      const root = document.getElementById(rootId);
      if (!root || root.dataset.timelineScope !== scope) continue;
      const target = selector ? root.querySelector(selector) :
        [...root.querySelectorAll('[data-message-id]')].find(row => row.dataset.messageId === latestId);
      if (!target) continue;
      if (selector) target.scrollIntoView({block:'center'});
      else root.scrollTop = root.scrollHeight;
      // ResizeObserver can run before the ensuing scroll event. Retire the old
      // reading position now, so it cannot undo this navigation in that gap.
      (rootId === 'message-timeline' ? window.superPlatinumTimelineAnchor : window.superPlatinumThreadAnchor)?.capture();
      return true;
    }
    return false;
  } finally {
    for (const type of ['wheel', 'touchmove', 'keydown']) document.removeEventListener(type, cancel);
  }
})
