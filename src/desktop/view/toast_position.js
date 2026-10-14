// A toast owns these observers only for its mounted lifetime.
function toastPlacement(viewport, obstacles, margin, gap, minimumHeight) {
  if (!(viewport.width > 0 && viewport.height > 0)) return null;
  let ceiling = viewport.height;
  for (const rect of obstacles) {
    if (!(rect.width > 0 && rect.height > 0) || rect.bottom <= 0 || rect.top >= viewport.height
        || rect.right <= 0 || rect.left >= viewport.width) continue;
    ceiling = Math.min(ceiling, Math.max(0, rect.top - gap));
  }
  const bottom = Math.max(margin, Math.ceil(viewport.height - ceiling));
  const maxHeight = Math.max(0, Math.floor(viewport.height - bottom - margin));
  return minimumHeight > 0 && maxHeight >= minimumHeight ? {bottom, maxHeight} : null;
}

function toastFocusedPlacement(base, viewport, toastSize, focus, margin, gap, minimumHeight) {
  if (!base || !focus || !(focus.width > 0 && focus.height > 0)) return base;
  const right = viewport.width - margin, left = right - toastSize.width;
  const bottom = viewport.height - base.bottom;
  const top = bottom - Math.min(toastSize.height, base.maxHeight);
  if (focus.right <= left || focus.left >= right || focus.bottom <= top || focus.top >= bottom) return base;
  const above = Math.floor(focus.top - gap - margin);
  if (above >= minimumHeight) return {bottom:Math.ceil(viewport.height - focus.top + gap), maxHeight:above};
  const below = Math.floor(bottom - focus.bottom - gap);
  if (below >= minimumHeight) return {bottom:base.bottom, maxHeight:below};
  return null;
}

(() => {
  const toast = document.getElementById('status-toast');
  if (!toast) return;
  toast.__disposePosition?.();
  const protectedSelector = '.composer-wrap, .thread-composer, .composer-completion, [role="dialog"][aria-modal="true"]';
  let targets = new Set(), focused = null, frame = null, disposed = false, rescan = true;
  const styles = getComputedStyle(toast);
  const margin = parseFloat(styles.right) || 18;
  const gap = parseFloat(styles.getPropertyValue('--space-2')) || 8;
  const set = (name, value) => {
    if (toast.style.getPropertyValue(name) !== value) toast.style.setProperty(name, value);
  };
  const focusedControl = () => {
    const active = document.activeElement;
    if (!active || toast.contains(active) || active.disabled) return null;
    return active.isContentEditable || active.matches('button,input,textarea,select,a[href],summary,[role="button"],[role="link"],[tabindex]') ? active : null;
  };
  const update = () => {
    frame = null;
    if (!toast.isConnected) { dispose(); return; }
    if (document.hidden || innerWidth <= 0 || innerHeight <= 0) {
      delete toast.dataset.positioned;
      return;
    }
    if (rescan) {
      rescan = false;
      const next = new Set(document.querySelectorAll(protectedSelector));
      for (const target of targets) if (!next.has(target) && target !== focused) resize.unobserve(target);
      for (const target of next) if (!targets.has(target)) resize.observe(target);
      targets = next;
    }
    const active = focusedControl();
    if (active !== focused) {
      if (focused && !targets.has(focused)) resize.unobserve(focused);
      if (active && !targets.has(active)) resize.observe(active);
      focused = active;
    }
    const dismissHeight = toast.querySelector('.toast-dismiss')?.getBoundingClientRect().height || 0;
    const chrome = ['paddingTop','paddingBottom','borderTopWidth','borderBottomWidth']
      .reduce((sum, key) => sum + (parseFloat(styles[key]) || 0), 0);
    const minimumHeight = dismissHeight > 0
      ? Math.ceil(Math.max(dismissHeight, parseFloat(styles.lineHeight) || 0) + chrome) : 0;
    const viewport = {width:innerWidth, height:innerHeight};
    const base = toastPlacement(viewport,
      [...targets].map(target => target.getBoundingClientRect()), margin, gap, minimumHeight);
    const bounds = toast.getBoundingClientRect();
    const border = (parseFloat(styles.borderTopWidth) || 0) + (parseFloat(styles.borderBottomWidth) || 0);
    const placement = toastFocusedPlacement(base, viewport,
      {width:bounds.width,height:Math.max(bounds.height,toast.scrollHeight + border)},
      focused?.getBoundingClientRect(), margin, gap, minimumHeight);
    // Never reveal clipped chrome or an unreachable dismiss control. Keep the
    // observer alive so a resize/remount can reveal the still-current notice.
    if (!placement) { delete toast.dataset.positioned; return; }
    set('--toast-bottom', placement.bottom + 'px');
    set('--toast-max-height', placement.maxHeight + 'px');
    toast.dataset.positioned = 'true';
  };
  const schedule = () => {
    if (!disposed && frame === null) frame = requestAnimationFrame(update);
  };
  const relevant = node => node.nodeType === 1 &&
    (node.matches(protectedSelector) || node.querySelector(protectedSelector));
  const resize = new ResizeObserver(schedule);
  const scroll = event => {
    const parent = event.target;
    const active = focusedControl();
    if (parent === document || (parent?.contains &&
        ((active && parent.contains(active)) || [...targets].some(target => parent.contains(target))))) schedule();
  };
  const mutation = new MutationObserver(records => {
    if (!toast.isConnected) { dispose(); return; }
    // Transcript row/text changes do not require a geometry read. Rescan only
    // when a protected surface mounts, unmounts or changes its visibility/role.
    if ((focused && !focused.isConnected) || records.some(record => record.type === 'attributes'
      ? targets.has(record.target) || record.target.matches(protectedSelector)
      : [...record.addedNodes, ...record.removedNodes].some(relevant))) {
      rescan = true;
      schedule();
    }
  });
  function dispose() {
    if (disposed) return;
    disposed = true;
    if (frame !== null) cancelAnimationFrame(frame);
    resize.disconnect(); mutation.disconnect(); targets.clear();
    window.removeEventListener('resize', schedule);
    document.removeEventListener('visibilitychange', schedule);
    document.removeEventListener('focusin', schedule);
    document.removeEventListener('focusout', schedule);
    document.removeEventListener('scroll', scroll, true);
    delete toast.__disposePosition;
  }
  toast.__disposePosition = dispose;
  resize.observe(toast);
  mutation.observe(document.body, {childList:true, subtree:true, attributes:true, attributeFilter:['hidden','class','role','aria-modal']});
  window.addEventListener('resize', schedule);
  document.addEventListener('visibilitychange', schedule);
  document.addEventListener('focusin', schedule);
  document.addEventListener('focusout', schedule);
  document.addEventListener('scroll', scroll, true);
  update();
})();
