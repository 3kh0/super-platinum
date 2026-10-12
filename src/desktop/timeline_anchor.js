((rootId = 'message-timeline', service = 'superPlatinumTimelineAnchor') => {
  window[service]?.dispose();
  const root = document.getElementById(rootId);
  if (!root) return;
  let scope = root.dataset.timelineScope;
  let pending = root.dataset.scrollPending === 'true';
  let anchor = null;
  let inputFrame = 0;
  let disposed = false;
  const observed = new Set();
  const blocks = () => [...root.querySelectorAll('[data-row-id]')];

  function capture(hold = false) {
    if (disposed || !root.isConnected || root.dataset.scrollPending === 'true') {
      anchor = null;
      return;
    }
    const bounds = root.getBoundingClientRect();
    const rows = blocks().map(row => ({id: row.dataset.rowId, rect: row.getBoundingClientRect()}));
    anchor = {
      height: root.scrollHeight,
      bottom: !hold && root.scrollHeight - root.scrollTop - root.clientHeight <= 2,
      rows: rows.filter(row => row.rect.bottom > bounds.top && row.rect.top < bounds.bottom)
        .slice(0, 8).map(row => ({id: row.id, offset: row.rect.top - bounds.top})),
    };
  }

  function restore() {
    if (!anchor || inputFrame) return;
    if (anchor.bottom) {
      const target = root.scrollHeight - root.clientHeight;
      if (Math.abs(root.scrollTop - target) > 0.5) root.scrollTop = target;
      return;
    }
    // Keep several visible identities so deleting the first row can preserve
    // the next one. Neither message text nor source URLs enter this state.
    const current = new Map(blocks().map(row => [row.dataset.rowId, row]));
    for (const saved of anchor.rows) {
      const row = current.get(saved.id);
      if (!row) continue;
      const delta = row.getBoundingClientRect().top - root.getBoundingClientRect().top - saved.offset;
      if (Math.abs(delta) > 0.5) root.scrollTop += delta;
      return;
    }
    capture();
  }

  function observeRows() {
    const current = new Set(blocks());
    for (const row of observed) if (!current.has(row)) { resize.unobserve(row); observed.delete(row); }
    for (const row of current) if (!observed.has(row)) { resize.observe(row); observed.add(row); }
  }

  function check() {
    if (disposed) return;
    if (!root.isConnected) { dispose(); return; }
    const nextScope = root.dataset.timelineScope;
    const nextPending = root.dataset.scrollPending === 'true';
    if (nextScope !== scope || nextPending !== pending) {
      scope = nextScope;
      pending = nextPending;
      anchor = null;
    }
    if (pending) anchor = null;
    else if (!anchor) capture();
    else restore();
  }

  function scrolled() {
    if (inputFrame) { cancelAnimationFrame(inputFrame); inputFrame = 0; }
    // Layout may emit scroll before ResizeObserver (notably in WebKit). Do
    // not replace the old reading position with that intermediate geometry.
    else if (anchor && anchor.height !== root.scrollHeight) check();
    capture();
  }
  function input(event) {
    if (event.type === 'keydown' &&
        (!['ArrowUp','ArrowDown','PageUp','PageDown','Home','End',' '].includes(event.key) ||
         event.target.closest('input,textarea,[contenteditable="true"]'))) return;
    // User scrolling outranks a correction queued by a background update.
    if (!inputFrame) inputFrame = requestAnimationFrame(() => {inputFrame = 0; capture();});
  }
  function dispose() {
    disposed = true;
    mutation.disconnect(); resize.disconnect(); observed.clear();
    root.removeEventListener('scroll', scrolled);
    for (const type of ['wheel','touchmove','keydown']) root.removeEventListener(type, input);
    if (inputFrame) cancelAnimationFrame(inputFrame);
  }
  const resize = new ResizeObserver(check);
  const mutation = new MutationObserver(() => { check(); observeRows(); });
  root.addEventListener('scroll', scrolled, {passive:true});
  for (const type of ['wheel','touchmove','keydown']) root.addEventListener(type, input, {passive:true});
  mutation.observe(root, {subtree:true, childList:true, characterData:true, attributes:true,
    attributeFilter:['style','data-timeline-scope','data-scroll-pending']});
  resize.observe(root);
  observeRows(); capture();
  window[service] = {check, capture, dispose, hold: () => capture(true)};
})
