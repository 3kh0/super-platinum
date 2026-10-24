((rootId = 'message-timeline', service = 'superPlatinumTimelineAnchor') => {
  window[service]?.dispose();
  const root = document.getElementById(rootId);
  if (!root) return;
  let scope = root.dataset.timelineScope;
  let pending = root.dataset.scrollPending === 'true';
  let anchor = null;
  let inputFrame = 0;
  let pointerHeld = false;
  let inputScope = scope;
  let inputGeneration = 0;
  function generation() {
    if (root.dataset.timelineScope !== inputScope) {
      inputScope = root.dataset.timelineScope;
      inputGeneration = 0;
    }
    return inputGeneration;
  }
  const trace = [];
  const record = (event, detail) => {
    if (!window.superPlatinumAnchorDebug) return;
    trace.push({event, detail, time:performance.now(), pointerHeld, inputFrame:!!inputFrame, anchor:anchor && {height:anchor.height,bottom:anchor.bottom,rows:anchor.rows.slice(0,3)}});
    if(trace.length>100)trace.shift();
  };
  let disposed = false;
  const observed = new Set();
  const blocks = () => [...root.querySelectorAll('[data-row-id]')];
  // Date/unread annotations belong to the wrapper, but can move to a newly
  // prepended predecessor. Preserve the message the reader sees, not that
  // mutable prefix. Non-message rows retain their wrapper-based position.
  const readingElement = row => {
    const message = row.querySelector('[data-message-id]');
    return message?.dataset.messageId === row.dataset.rowId ? message : row;
  };

  function capture(hold = false) {
    record("capture-start");
    if (disposed || !root.isConnected || root.dataset.scrollPending === 'true') {
      anchor = null;
      return;
    }
    const bounds = root.getBoundingClientRect();
    const rows = blocks().map(row => ({id: row.dataset.rowId, rect: readingElement(row).getBoundingClientRect()}));
    anchor = {
      height: root.scrollHeight,
      bottom: !hold && root.scrollHeight - root.scrollTop - root.clientHeight <= 2,
      rows: rows.filter(row => row.rect.bottom > bounds.top && row.rect.top < bounds.bottom)
        .slice(0, 8).map(row => ({id: row.id, offset: row.rect.top - bounds.top})),
    };
    record("capture-complete");
  }

  function restore() {
    record("restore");
    if (!anchor || inputFrame || pointerHeld) return;
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
      const delta = readingElement(row).getBoundingClientRect().top - root.getBoundingClientRect().top - saved.offset;
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
    record("scroll");
    if (pointerHeld || inputFrame) { if (inputFrame) cancelAnimationFrame(inputFrame); inputFrame = 0; }
    // Layout may emit scroll before ResizeObserver (notably in WebKit). Do
    // not replace the old reading position with that intermediate geometry.
    else if (anchor && anchor.height !== root.scrollHeight) check();
    capture();
  }
  function input(event) {
    generation();
    // Mirror the native thread input boundary synchronously. A queued native
    // eval may begin after this event and therefore miss its cancel listener.
    if (['wheel','pointerdown','touchmove'].includes(event.type)) inputGeneration++;
    record(event.type);
    // A scrollbar drag is user navigation too. Without pointer tracking a
    // delayed virtual-window resize can restore the row the reader just left.
    if (event.type === 'pointerdown') {
      if (event.button !== 0) return;
      pointerHeld = true;
    }
    if (event.type === 'keydown' &&
        (!['ArrowUp','ArrowDown','PageUp','PageDown','Home','End',' '].includes(event.key) ||
         event.target.closest('input,textarea,[contenteditable="true"]'))) return;
    if (event.type === 'keydown' && service === 'superPlatinumTimelineAnchor') inputGeneration++;
    // User scrolling outranks a correction queued by a background update.
    if (!inputFrame) inputFrame = requestAnimationFrame(() => {inputFrame = 0; capture();});
  }
  function released() {
    record("release");
    if (!pointerHeld) return;
    pointerHeld = false;
    capture();
  }
  function dispose() {
    disposed = true;
    mutation.disconnect(); resize.disconnect(); observed.clear();
    root.removeEventListener('scroll', scrolled);
    for (const type of ['wheel','touchmove','keydown','pointerdown']) root.removeEventListener(type, input);
    document.removeEventListener('pointerup', released);
    document.removeEventListener('pointercancel', released);
    window.removeEventListener('blur', released);
    if (inputFrame) cancelAnimationFrame(inputFrame);
  }
  const resize = new ResizeObserver(check);
  const mutation = new MutationObserver(() => { check(); observeRows(); });
  root.addEventListener('scroll', scrolled, {passive:true});
  for (const type of ['wheel','touchmove','keydown','pointerdown']) root.addEventListener(type, input, {passive:true});
  document.addEventListener('pointerup', released, {passive:true});
  document.addEventListener('pointercancel', released, {passive:true});
  window.addEventListener('blur', released);
  mutation.observe(root, {subtree:true, childList:true, characterData:true, attributes:true,
    attributeFilter:['style','data-timeline-scope','data-scroll-pending']});
  resize.observe(root);
  observeRows(); capture();
  window[service] = {check, capture, dispose, note:record, inputGeneration:generation, hold: () => capture(true), debug: () => ({anchor, trace, pointerHeld, overflowAnchor:getComputedStyle(root).overflowAnchor})};
})
