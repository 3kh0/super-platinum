// One mounted timeline, no periodic polling. Native owns account/read policy;
// this service supplies geometry and 350 ms stable attention evidence only.
(() => {
  window.superPlatinumReadVisibility?.dispose();
  const root = document.getElementById('message-timeline');
  if (!root) return;
  let disposed = false, timer = 0, frame = 0, signature = '', stable = false;
  const shown = element => {
    if (!element?.isConnected || !element.getClientRects().length) return false;
    for (let node = element; node; node = node.parentElement) {
      const style = getComputedStyle(node);
      if (style.display === 'none' || style.visibility !== 'visible' || Number(style.opacity) === 0 || node.getAttribute('aria-hidden') === 'true') return false;
    }
    return true;
  };
  const viewport = () => {
    const rect = root.getBoundingClientRect();
    const clip = {top:Math.max(0,rect.top),bottom:Math.min(innerHeight,rect.bottom),left:Math.max(0,rect.left),right:Math.min(innerWidth,rect.right)};
    for (let node = root.parentElement; node; node = node.parentElement) {
      const style = getComputedStyle(node), box = node.getBoundingClientRect();
      if (/(hidden|clip|scroll|auto)/.test(style.overflowY)) {clip.top=Math.max(clip.top,box.top);clip.bottom=Math.min(clip.bottom,box.bottom);}
      if (/(hidden|clip|scroll|auto)/.test(style.overflowX)) {clip.left=Math.max(clip.left,box.left);clip.right=Math.min(clip.right,box.right);}
    }
    return clip;
  };
  function sample() {
    const bounds = viewport();
    const visible = shown(root) && document.visibilityState === 'visible' && document.hasFocus() && bounds.bottom > bounds.top && bounds.right > bounds.left;
    const divider = root.querySelector('.unread-divider');
    const rect = divider?.getBoundingClientRect();
    const ghost = root.dataset.readGhost === 'true' || divider?.dataset.ghost === 'true';
    const dividerVisible = visible && !!rect && shown(divider) && rect.height > 0 && rect.top >= bounds.top && rect.bottom <= bounds.bottom && rect.right > bounds.left && rect.left < bounds.right;
    const first = visible ? [...root.querySelectorAll('[data-row-id]')].find(row => { const r=row.getBoundingClientRect(); return shown(row) && r.bottom > bounds.top && r.top < bounds.bottom; }) : null;
    return {scope:root.dataset.timelineScope,key:root.dataset.readKey,visible,dividerVisible,ghost:!!ghost,
      atBottom:root.scrollHeight-root.scrollTop-root.clientHeight < 2,
      pending:root.dataset.scrollPending === 'true', firstId:first?.dataset.rowId ?? null,
      offset:first ? first.getBoundingClientRect().top-root.getBoundingClientRect().top : null};
  }
  function publish() {
    frame = 0;
    if (disposed) return;
    if (!root.isConnected) { dispose(); return; }
    const value = sample();
    const next = JSON.stringify([value.scope,value.key,value.visible,value.dividerVisible,value.ghost,value.atBottom,value.pending]);
    if (signature !== next) {
      signature = next; stable = false; clearTimeout(timer);
      timer = setTimeout(() => {
        timer = 0;
        const fresh = sample();
        const current = JSON.stringify([fresh.scope,fresh.key,fresh.visible,fresh.dividerVisible,fresh.ghost,fresh.atBottom,fresh.pending]);
        if (current !== signature || !root.isConnected) { publish(); return; }
        stable = true;
        dioxus.send({...fresh,stable});
      },350);
    }
    dioxus.send({...value,stable});
  }
  function schedule() { if (!frame && !disposed) frame=requestAnimationFrame(publish); }
  // Focus/visibility loss invalidates before another frame can reuse attention.
  function attention() { if (frame) cancelAnimationFrame(frame); frame=0; publish(); }
  const rows = new Set();
  function observeRows() {
    const next = new Set(root.querySelectorAll('[data-row-id]'));
    for (const row of rows) if (!next.has(row)) { resize.unobserve(row);rows.delete(row); }
    for (const row of next) if (!rows.has(row)) { resize.observe(row);rows.add(row); }
  }
  const mutation = new MutationObserver(() => {observeRows();schedule();});
  mutation.observe(root,{subtree:true,childList:true,attributes:true});
  const resize = new ResizeObserver(schedule);
  resize.observe(root);
  observeRows();
  root.addEventListener('scroll',schedule,{passive:true});
  window.addEventListener('resize',schedule);
  window.addEventListener('focus',attention);
  window.addEventListener('blur',attention);
  document.addEventListener('visibilitychange',attention);
  function dispose() {
    if (disposed) return;
    disposed=true;clearTimeout(timer);cancelAnimationFrame(frame);mutation.disconnect();resize.disconnect();
    root.removeEventListener('scroll',schedule);window.removeEventListener('resize',schedule);
    window.removeEventListener('focus',attention);window.removeEventListener('blur',attention);
    document.removeEventListener('visibilitychange',attention);
    dioxus.send(null);
  }
  window.superPlatinumReadVisibility={dispose,sample};
  publish();
})()
