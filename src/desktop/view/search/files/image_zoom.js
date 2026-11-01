// Fit/slider math: retained vqzr/HACm/lM1q, historical Gantry B2.
// Native inline policy: measured box, overflow-clamped local pan and scrollbars.
// No IPC, resource assignment, file actions, focus or global input handlers.
function nativeImageFit(w, h, vw, vh) {
  if (![w, h, vw, vh].every(v => Number.isFinite(v) && v > 0)) return null;
  return Math.min(1, vw / w, vh / h);
}
function nativeImageScale(fit, level) {
  if (!(fit > 0 && fit <= 1) || !Number.isFinite(level) || level < 0 || level > 1) return null;
  return fit + level * (2 - fit);
}
// Historical HACm/XCmt: actual scale, strict first match, reversed COPY for out.
// Qualified ordinary arrays/standard intrinsics; no runtime/cohort authority.
function nativeImageStep(fit, scale, direction) {
  if (!(fit > 0 && fit <= 1) || !Number.isFinite(scale) || scale <= 0 || scale > 2
      || !['in', 'out'].includes(direction)) return null;
  if ((direction === 'in' && scale === 2) || (direction === 'out' && scale === fit)) return scale;
  const steps = [0, .5, 1, 1.5, 2];
  const target = direction === 'in'
    ? steps.find(step => step > scale) ?? 2
    : steps.slice().reverse().find(step => step < scale) ?? fit;
  return Math.max(fit, Math.min(2, target));
}
function installNativeImageZoom(id) {
  const roots = document.querySelectorAll(`#${CSS.escape(id)}`);
  if (roots.length !== 1) return;
  const root = roots[0], panel = root.closest('.search-file-image');
  const one = selector => {
    const nodes = root.querySelectorAll(selector);
    return nodes.length === 1 ? nodes[0] : null;
  };
  const viewport = one('[data-image-zoom-viewport]'), canvas = one('[data-image-zoom-canvas]');
  const image = one('[data-image-zoom-image]'), slider = one('[data-image-zoom-slider]');
  const fitButton = one('[data-image-zoom-fit]'), label = one('[data-image-zoom-label]');
  const plus = one('[data-image-zoom-plus]'), minus = one('[data-image-zoom-minus]');
  if (!panel || !viewport || !canvas || !image || !slider || !fitButton || !label || !plus || !minus) return;
  root.__nativeImageZoom?.();
  const source = root.dataset.imageZoomSource;
  const ownerDocument = document, styleReader = globalThis.getComputedStyle;
  let stopped = false, resize, mutation, size = null, fit = null, scale = null, activePan = null;
  let activeClick = null;
  let measuredViewport = null, paintedStyles = null;
  const current = () => !stopped && root.__nativeImageZoom === stop
    && root.isConnected && panel.isConnected && root.closest('.search-file-image') === panel
    && document.querySelectorAll(`#${CSS.escape(id)}`).length === 1
    && document.querySelectorAll(`#${CSS.escape(id)}`)[0] === root
    && root.dataset.imageZoomSource === source && !!source
    && image.getAttribute('src')?.split('?')[0] === source
    && one('[data-image-zoom-viewport]') === viewport && one('[data-image-zoom-canvas]') === canvas
    && one('[data-image-zoom-image]') === image && one('[data-image-zoom-slider]') === slider
    && one('[data-image-zoom-fit]') === fitButton && one('[data-image-zoom-label]') === label
    && one('[data-image-zoom-plus]') === plus && one('[data-image-zoom-minus]') === minus;
  const captureMethods = () => {
    const set = viewport.setPointerCapture, has = viewport.hasPointerCapture;
    const release = viewport.releasePointerCapture;
    return [set, has, release].every(fn => typeof fn === 'function') ? {set, has, release} : null;
  };
  const cancelClick = (record = activeClick) => {
    if (record && activeClick !== record) return false;
    activeClick = null;
    if (record) {
      for (const [type, listener] of record.listeners) viewport.removeEventListener(type, listener);
    }
    if (root.__nativeImageZoom !== stop) return false;
    canvas.dataset.imageZoomClickReady = 'false';
    return true;
  };
  const panGeometry = () => {
    if (!current() || canvas.dataset.imageZoomReady !== 'true' || slider.disabled
        || !image.complete || !size || !(scale > fit) || !(fit > 0)
        || image.naturalWidth !== size[0] || image.naturalHeight !== size[1]) return null;
    const vw = viewport.clientWidth, vh = viewport.clientHeight;
    const sw = viewport.scrollWidth, sh = viewport.scrollHeight;
    const left = viewport.scrollLeft, top = viewport.scrollTop;
    if (![...size, scale, fit, vw, vh, sw, sh].every(v => Number.isFinite(v) && v > 0)
        || ![left, top].every(Number.isFinite) || sw < vw || sh < vh
        || left < 0 || top < 0 || left > sw - vw || top > sh - vh
        || (sw === vw && sh === vh)) return null;
    const styles = [image.style.width, image.style.height, canvas.style.width, canvas.style.height];
    if (!styles.every(value => typeof value === 'string' && value.endsWith('px')
        && Number.isFinite(Number(value.slice(0, -2))) && Number(value.slice(0, -2)) > 0)) return null;
    return {vw, vh, sw, sh, left, top, scale, fit, w:size[0], h:size[1], styles,
      imageWidth:canvas.dataset.imageWidth, imageHeight:canvas.dataset.imageHeight};
  };
  const samePanGeometry = (a, b) => !!b
    && ['vw', 'vh', 'sw', 'sh', 'scale', 'fit', 'w', 'h', 'imageWidth', 'imageHeight']
      .every(key => a[key] === b[key])
    && a.styles.every((value, index) => value === b.styles[index]);
  const cancelPan = (record = activePan) => {
    if (record && activePan !== record) return false;
    // Clear first: releasing capture may queue an old lost-capture callback.
    activePan = null;
    if (record) {
      for (const [type, listener] of record.listeners) viewport.removeEventListener(type, listener);
    }
    // A retired callback must not clear or release a later controller's state.
    if (root.__nativeImageZoom !== stop) return false;
    canvas.dataset.imageZoomPanning = 'false';
    canvas.dataset.imageZoomPanReady = 'false';
    if (!record) return true;
    try {
      if (record.capture.has.call(viewport, record.pointer)) {
        record.capture.release.call(viewport, record.pointer);
      }
      return !record.capture.has.call(viewport, record.pointer);
    } catch { return false; }
  };
  const markPanReady = () => {
    if (current()) canvas.dataset.imageZoomPanReady = String(!!captureMethods() && !!panGeometry());
  };
  const disable = () => {
    cancelClick();
    cancelPan();
    measuredViewport = null; paintedStyles = null;
    slider.disabled = true; fitButton.disabled = true;
    plus.disabled = true; minus.disabled = true;
    canvas.dataset.imageZoomReady = 'false';
    label.textContent = 'Fit'; slider.removeAttribute('aria-valuetext');
  };
  const stop = () => {
    if (stopped) return;
    stopped = true;
    resize?.disconnect(); mutation?.disconnect();
    slider.removeEventListener('input', zoom);
    fitButton.removeEventListener('click', reset);
    plus.removeEventListener('click', zoomIn);
    minus.removeEventListener('click', zoomOut);
    viewport.removeEventListener('pointerdown', pointerDown);
    viewport.removeEventListener('wheel', wheel, wheelOptions);
    image.removeEventListener('load', measure);
    image.removeEventListener('error', unavailable);
    disable();
    if (root.__nativeImageZoom === stop) delete root.__nativeImageZoom;
  };
  const paint = resetScroll => {
    cancelClick();
    cancelPan();
    const vw = viewport.clientWidth, vh = viewport.clientHeight;
    const oldW = Number(canvas.dataset.imageWidth) || size[0] * scale;
    const oldH = Number(canvas.dataset.imageHeight) || size[1] * scale;
    // Preserve the visible image center through a slider change. Scrollbar
    // panning is native presentation, not Source drag/gesture equivalence.
    const x = (viewport.scrollLeft + vw / 2 - Math.max(0, (vw - oldW) / 2)) / oldW;
    const y = (viewport.scrollTop + vh / 2 - Math.max(0, (vh - oldH) / 2)) / oldH;
    const w = size[0] * scale, h = size[1] * scale;
    canvas.dataset.imageZoomReady = 'true';
    image.style.width = `${w}px`; image.style.height = `${h}px`;
    canvas.style.width = `${Math.max(vw, w)}px`; canvas.style.height = `${Math.max(vh, h)}px`;
    canvas.dataset.imageWidth = String(w); canvas.dataset.imageHeight = String(h);
    viewport.scrollLeft = resetScroll ? 0 : Math.max(0, x * w + Math.max(0, (vw - w) / 2) - vw / 2);
    viewport.scrollTop = resetScroll ? 0 : Math.max(0, y * h + Math.max(0, (vh - h) / 2) - vh / 2);
    // Source preserves absolute zoom on resize even below a newly larger fit.
    // The native range thumb is bounded while the actual label stays truthful.
    slider.value = String(Math.max(0, Math.min(1, (scale - fit) / (2 - fit))));
    slider.disabled = false; fitButton.disabled = !(scale > fit);
    // Source has endpoint no-ops, not explicit +/- disabled props. Native
    // readiness/owner failure still disables every local control.
    plus.disabled = false; minus.disabled = false;
    label.textContent = `${Math.round(100 * scale)}%`;
    slider.setAttribute('aria-valuetext', label.textContent);
    paintedStyles = wheelStyles();
    markPanReady();
    markClickReady();
  };
  const measure = () => {
    if (!current()) { stop(); return; }
    cancelClick();
    cancelPan();
    const nextSize = [image.naturalWidth, image.naturalHeight];
    if (!image.complete) { disable(); return; }
    const nextViewport = [viewport.clientWidth, viewport.clientHeight];
    const nextFit = nativeImageFit(...nextSize, ...nextViewport);
    if (nextFit === null) { disable(); return; }
    // The admitted raster is static. Geometry replacement is a new lifetime.
    if (size && (size[0] !== nextSize[0] || size[1] !== nextSize[1])) { stop(); return; }
    const atFit = scale === null || scale === fit;
    size = nextSize; fit = nextFit;
    // Only the original load/resize/control measurement establishes this basis.
    measuredViewport = nextViewport;
    if (atFit) scale = fit;
    paint(atFit);
  };
  const zoom = event => {
    if (!current()) { stop(); return; }
    if (event.target !== slider || slider.disabled || !slider.value.trim()) return;
    const level = Number(slider.value);
    measure();
    if (stopped || slider.disabled || fit === null) return;
    const next = nativeImageScale(fit, level);
    if (next === null) return;
    scale = next; paint(false);
  };
  const reset = event => {
    if (!current()) { stop(); return; }
    if (event.target !== fitButton || fitButton.disabled) return;
    measure();
    if (stopped || fit === null || fitButton.disabled) return;
    scale = fit; paint(true);
  };
  const step = (event, button, direction) => {
    if (!current()) { stop(); return; }
    // Registered currentTarget accepts the button's typed SVG children without
    // allowing a peer node to borrow this listener's original owner.
    if (event.currentTarget !== button || button.disabled) return;
    measure();
    if (stopped || fit === null || button.disabled) return;
    const next = nativeImageStep(fit, scale, direction);
    if (next === null || next === scale) return;
    scale = next; paint(false);
  };
  const zoomIn = event => step(event, plus, 'in');
  const zoomOut = event => step(event, minus, 'out');
  const wheelOptions = {passive:false};
  const wheelStyles = () => [image.style.width, image.style.height, canvas.style.width,
    canvas.style.height, canvas.dataset.imageWidth, canvas.dataset.imageHeight];
  // One CSS pixel is a native geometry-consistency tolerance for layout rounding,
  // not Source geometry equality or an allowance for transformed input space.
  const closePixel = (a, b) => Math.abs(a - b) <= 1;
  const pixel = value => typeof value === 'string' && value.endsWith('px')
    && value.slice(0, -2).trim() ? Number(value.slice(0, -2)) : NaN;
  const translationOnly = value => {
    if (value === 'none') return true;
    const match = typeof value === 'string' && /^matrix\(([^)]*)\)$/.exec(value);
    if (!match) return false;
    const parts = match[1].split(',').map(part => part.trim());
    if (parts.length !== 6 || !parts.every(part => /^[+-]?(?:\d+\.?\d*|\.\d+)(?:e[+-]?\d+)?$/i.test(part))) return false;
    const matrix = parts.map(Number);
    // Search's .modal has translateX(-50%): DOMRects include that origin shift,
    // while the identity linear part preserves client lengths and image axes.
    return matrix.every(Number.isFinite) && matrix[0] === 1 && matrix[1] === 0
      && matrix[2] === 0 && matrix[3] === 1;
  };
  const neutralStyle = node => {
    const style = styleReader.call(globalThis, node);
    if (!style || !translationOnly(style.transform)
        || !['1', 'normal'].includes(style.zoom)) return null;
    for (const key of ['rotate', 'scale', 'translate', 'perspective']) {
      if (style[key] && style[key] !== 'none') return null;
    }
    return style;
  };
  const wheelGeometry = requireMeasured => {
    if (!current() || document !== ownerDocument || typeof styleReader !== 'function'
        || globalThis.getComputedStyle !== styleReader || !measuredViewport || !paintedStyles
        || canvas.dataset.imageZoomReady !== 'true' || slider.disabled || !image.complete || !size
        || !(fit > 0 && fit <= 1) || !(scale > 0 && scale <= 2)
        || image.naturalWidth !== size[0] || image.naturalHeight !== size[1]
        || !wheelStyles().every((value, index) => value === paintedStyles[index])) return null;
    try {
      if (image.parentElement !== canvas || canvas.parentElement !== viewport
          || viewport.parentElement !== root) return null;
      let node = image, count = 0, sawRoot = false, sawPanel = false, reachedDocument = false;
      for (; node && count < 128; node = node.parentElement, count++) {
        if (!neutralStyle(node)) return null;
        sawRoot ||= node === root; sawPanel ||= node === panel;
        if (node === ownerDocument.documentElement) { reachedDocument = true; break; }
      }
      if (!sawRoot || !sawPanel || !reachedDocument) return null;
      for (const element of [viewport, canvas, image]) {
        const style = neutralStyle(element);
        if (style.direction !== 'ltr' || ['paddingLeft', 'paddingRight', 'paddingTop', 'paddingBottom']
          .some(key => pixel(style[key]) !== 0)) return null;
      }
      const vw = viewport.clientWidth, vh = viewport.clientHeight;
      const sw = viewport.scrollWidth, sh = viewport.scrollHeight;
      const left = viewport.scrollLeft, top = viewport.scrollTop;
      const borderLeft = viewport.clientLeft, borderTop = viewport.clientTop;
      const vr = viewport.getBoundingClientRect(), cr = canvas.getBoundingClientRect();
      const ir = image.getBoundingClientRect();
      const rect = value => value && ['left', 'top', 'right', 'bottom', 'width', 'height']
        .every(key => Number.isFinite(value[key])) && value.width > 0 && value.height > 0
        && closePixel(value.right - value.left, value.width)
        && closePixel(value.bottom - value.top, value.height);
      if (![...size, fit, scale, vw, vh, sw, sh, viewport.offsetWidth, viewport.offsetHeight]
          .every(value => Number.isFinite(value) && value > 0)
          || ![left, top, borderLeft, borderTop].every(value => Number.isFinite(value) && value >= 0)
          || sw < vw || sh < vh || left > sw - vw || top > sh - vh
          || (requireMeasured && (vw !== measuredViewport[0] || vh !== measuredViewport[1]))
          || !rect(vr) || !rect(cr) || !rect(ir)
          || !closePixel(vr.width, viewport.offsetWidth) || !closePixel(vr.height, viewport.offsetHeight)
          || borderLeft + vw > vr.width + 1 || borderTop + vh > vr.height + 1
          || !closePixel(ir.width, size[0] * scale) || !closePixel(ir.height, size[1] * scale)
          || !closePixel(ir.width, pixel(image.style.width))
          || !closePixel(ir.height, pixel(image.style.height))
          || cr.width + 1 < Math.max(vw, ir.width) || cr.height + 1 < Math.max(vh, ir.height)
          || !closePixel(cr.left, vr.left + borderLeft - left)
          || !closePixel(cr.top, vr.top + borderTop - top)
          || !closePixel(ir.left, cr.left + (cr.width - ir.width) / 2)
          || !closePixel(ir.top, cr.top + (cr.height - ir.height) / 2)) return null;
      return current() ? {vw, vh, sw, sh, left, top, borderLeft, borderTop, vr, cr, ir, scale, fit} : null;
    } catch { return null; }
  };
  const sameWheelGeometry = (a, b) => !!b
    && ['vw', 'vh', 'sw', 'sh', 'left', 'top', 'borderLeft', 'borderTop', 'scale', 'fit']
      .every(key => a[key] === b[key])
    && ['vr', 'cr', 'ir'].every(key => ['left', 'top', 'width', 'height']
      .every(field => a[key][field] === b[key][field]));
  const insideImage = (geometry, x, y) => !!geometry && [x, y].every(Number.isFinite)
    && x >= geometry.vr.left + geometry.borderLeft
    && x < geometry.vr.left + geometry.borderLeft + geometry.vw
    && y >= geometry.vr.top + geometry.borderTop
    && y < geometry.vr.top + geometry.borderTop + geometry.vh
    && x >= geometry.ir.left && x < geometry.ir.right
    && y >= geometry.ir.top && y < geometry.ir.bottom;
  const clickMode = geometry => !geometry ? null : geometry.scale === geometry.fit ? 'in'
    : geometry.sw === geometry.vw && geometry.sh === geometry.vh ? 'out' : null;
  const markClickReady = () => {
    if (current()) canvas.dataset.imageZoomClickReady = clickMode(wheelGeometry(true)) ?? 'false';
  };
  // Presentation only: each caller authenticates and retires its own input first.
  // This helper accepts no event and cannot manufacture wheel/click readiness.
  const zoomAtPoint = (next, x, y, before) => {
    if (!current() || activePan !== null || activeClick !== null || !Number.isFinite(next)
        || next < fit || next > 2 || !insideImage(before, x, y)
        || !sameWheelGeometry(before, wheelGeometry(true))) return;
    const u = (x - before.ir.left) / before.ir.width, v = (y - before.ir.top) / before.ir.height;
    if (next === scale) { markPanReady(); markClickReady(); return; }
    scale = next; paint(false);
    // Reading this write's resulting layout is not a new measured viewport basis.
    // Any changed client size must still wait for the original ResizeObserver.
    const after = wheelGeometry(false);
    if (!after || activePan !== null || activeClick !== null) {
      if (current() && activePan === null && activeClick === null) stop();
      return;
    }
    const left = after.left + after.ir.left + u * after.ir.width - x;
    const top = after.top + after.ir.top + v * after.ir.height - y;
    if (![left, top].every(Number.isFinite)) { stop(); return; }
    viewport.scrollLeft = Math.max(0, Math.min(after.sw - after.vw, left));
    viewport.scrollTop = Math.max(0, Math.min(after.sh - after.vh, top));
    if (!wheelGeometry(false)) {
      if (current() && activePan === null && activeClick === null) stop();
      return;
    }
    markPanReady(); markClickReady();
  };
  const wheel = event => {
    if (!current()) { stop(); return; }
    if (event.currentTarget !== viewport || event.target !== image || event.cancelable !== true
        || event.ctrlKey !== true || event.metaKey !== false || event.altKey !== false
        || event.deltaMode !== 0 || ![event.clientX, event.clientY, event.deltaY].every(Number.isFinite)) return;
    if (size && (image.naturalWidth !== size[0] || image.naturalHeight !== size[1])) { stop(); return; }
    // This path only reads prior readiness. It never measures or revives an image.
    const before = wheelGeometry(true);
    if (!before) return;
    const x = event.clientX, y = event.clientY;
    if (!insideImage(before, x, y)) return;
    const delta = -0.01 * event.deltaY;
    const next = (scale === fit && delta < 0) || (scale === 2 && delta > 0)
      ? scale : Math.max(fit, Math.min(2, scale + delta));
    if (!Number.isFinite(next) || !cancelClick() || !cancelPan() || !current()
        || activePan !== null || activeClick !== null
        || !sameWheelGeometry(before, wheelGeometry(true))) return;
    try {
      event.preventDefault();
      if (event.defaultPrevented !== true) return;
      event.stopPropagation();
    } catch { return; }
    zoomAtPoint(next, x, y, before);
  };
  const ordinaryPointer = event => event.pointerType === 'mouse' && event.isPrimary === true
    && Number.isInteger(event.pointerId) && event.pointerId >= 0
    && event.ctrlKey === false && event.metaKey === false && event.altKey === false
    && Number.isFinite(event.clientX) && Number.isFinite(event.clientY);
  const handledPanEvent = event => {
    if (event.cancelable !== false) event.preventDefault();
    event.stopPropagation();
  };
  const clickStart = (event, suppliedGeometry) => {
    if (activePan || event.currentTarget !== viewport || event.target !== image
        || !ordinaryPointer(event) || event.button !== 0 || event.buttons !== 1) return;
    if (!current()) { stop(); return; }
    const geometry = suppliedGeometry ?? wheelGeometry(true), mode = clickMode(geometry);
    if (!mode || !sameWheelGeometry(geometry, wheelGeometry(true))
        || !insideImage(geometry, event.clientX, event.clientY)) return;
    // A new qualified press supersedes a captureless press whose release was
    // missed. Its old callbacks retain only the retired record, never this one.
    if (!cancelClick() || !current() || activeClick !== null || activePan !== null
        || !sameWheelGeometry(geometry, wheelGeometry(true))) return;
    const record = {geometry, mode, pointer:event.pointerId, moves:0, listeners:[]};
    const ownsEvent = next => activeClick === record && next.currentTarget === viewport
      && next.pointerId === record.pointer;
    const discard = () => {
      if (cancelClick(record) && current() && activeClick === null) markClickReady();
    };
    const live = next => {
      if (!current()) { stop(); return false; }
      if (image.naturalWidth !== size?.[0] || image.naturalHeight !== size?.[1]) {
        stop(); return false;
      }
      const now = wheelGeometry(true);
      if (!ordinaryPointer(next) || clickMode(now) !== mode || activePan !== null
          || !sameWheelGeometry(geometry, now)
          || !insideImage(geometry, next.clientX, next.clientY)) { discard(); return false; }
      return true;
    };
    const move = next => {
      if (activeClick !== record) return;
      if (!ownsEvent(next)) return;
      if (!live(next)) return;
      if (next.target !== image) { discard(); return; }
      if (next.buttons !== 1) { discard(); return; }
      record.moves = Math.min(6, record.moves + 1); // No pan/capture in this purpose.
    };
    const up = next => {
      if (activeClick !== record) return;
      if (!ownsEvent(next)) return;
      if (!live(next)) return;
      if (next.target !== image) { discard(); return; }
      if (next.button !== 0 || next.buttons !== 0 || next.cancelable !== true) { discard(); return; }
      const short = record.moves <= 5;
      if (!cancelClick(record) || !current() || activeClick !== null || activePan !== null
          || !sameWheelGeometry(geometry, wheelGeometry(true))) return;
      try {
        next.preventDefault();
        if (next.defaultPrevented !== true) return;
        next.stopPropagation();
      } catch { return; }
      if (!current() || activeClick !== null || activePan !== null
          || !sameWheelGeometry(geometry, wheelGeometry(true))) return;
      if (!short) { markClickReady(); return; }
      if (mode === 'in') zoomAtPoint(fit < .9 ? 1 : 2, next.clientX, next.clientY, geometry);
      else { scale = fit; paint(true); }
    };
    const cancel = next => {
      if (activeClick !== record) return;
      if (!ownsEvent(next) || (next.target !== image && next.target !== viewport)) return;
      discard(); // Leave/cancel never acts as a short release.
    };
    record.listeners = [['pointermove', move], ['pointerup', up],
      ['pointerleave', cancel], ['pointercancel', cancel]];
    activeClick = record;
    for (const [type, listener] of record.listeners) viewport.addEventListener(type, listener);
    markClickReady();
    event.stopPropagation();
  };
  const panDown = event => {
    if (activePan || event.currentTarget !== viewport || event.target !== image
        || !ordinaryPointer(event) || event.button !== 0 || event.buttons !== 1) return;
    if (!current()) { stop(); return; }
    // Do not measure here: a press cannot revive an errored/disabled image.
    const geometry = panGeometry(), capture = captureMethods();
    if (!geometry || !capture) return;
    const record = {geometry, capture, pointer:event.pointerId, x:event.clientX, y:event.clientY,
      lastLeft:geometry.left, lastTop:geometry.top, moves:0, listeners:[]};
    const ownsEvent = next => activePan === record && next.currentTarget === viewport
      && next.target === viewport && next.pointerId === record.pointer;
    const live = next => {
      if (!current()) { stop(); return false; }
      if (image.naturalWidth !== size?.[0] || image.naturalHeight !== size?.[1]) {
        stop(); return false;
      }
      const now = panGeometry(), methods = captureMethods();
      if (!ordinaryPointer(next) || !samePanGeometry(geometry, now)
          || now.left !== record.lastLeft || now.top !== record.lastTop
          || !methods || methods.set !== capture.set || methods.has !== capture.has
          || methods.release !== capture.release) { cancelPan(record); return false; }
      try {
        if (!capture.has.call(viewport, record.pointer)) { cancelPan(record); return false; }
      } catch { cancelPan(record); return false; }
      return true;
    };
    const move = next => {
      if (activePan !== record) return;
      if (!ownsEvent(next)) return;
      if (!live(next)) return;
      if (next.buttons !== 1) { cancelPan(record); return; }
      const left = record.lastLeft - (next.clientX - record.x);
      const top = record.lastTop - (next.clientY - record.y);
      if (![left, top].every(Number.isFinite)) { cancelPan(record); return; }
      // Only the <=5/>5 distinction matters; count callbacks, including no-op moves.
      record.moves = Math.min(6, record.moves + 1);
      viewport.scrollLeft = Math.max(0, Math.min(geometry.sw - geometry.vw, left));
      viewport.scrollTop = Math.max(0, Math.min(geometry.sh - geometry.vh, top));
      record.lastLeft = viewport.scrollLeft; record.lastTop = viewport.scrollTop;
      // Source clamps each movement. Advance even at an edge so reversing the
      // hand immediately pans back rather than spending a press-origin overshoot.
      record.x = next.clientX; record.y = next.clientY;
      handledPanEvent(next);
    };
    const up = next => {
      if (activePan !== record) return;
      if (!ownsEvent(next)) return;
      if (!live(next)) return;
      if (next.button !== 0 || next.buttons !== 0) { cancelPan(record); return; }
      const short = record.moves <= 5;
      const released = cancelPan(record);
      if (!released || !current() || activePan !== null
          || !samePanGeometry(geometry, panGeometry())) return;
      handledPanEvent(next);
      // Historical ordinary short-release resets even after its earlier pan.
      // Native overflow clamp/capture remain an explicit inline adaptation.
      if (short) { scale = fit; paint(true); }
      else markPanReady();
    };
    const cancel = next => {
      if (activePan !== record) return;
      if (!ownsEvent(next)) return;
      cancelPan(record); // Cancellation never enters the short-release reset.
    };
    record.listeners = [['pointermove', move], ['pointerup', up],
      ['pointercancel', cancel], ['lostpointercapture', cancel]];
    activePan = record;
    for (const [type, listener] of record.listeners) viewport.addEventListener(type, listener);
    try {
      capture.set.call(viewport, record.pointer);
      if (activePan !== record) return;
      if (!capture.has.call(viewport, record.pointer) || !current()
          || !samePanGeometry(geometry, panGeometry())) { cancelPan(record); return; }
    } catch { cancelPan(record); return; }
    canvas.dataset.imageZoomPanning = 'true';
    canvas.dataset.imageZoomPanReady = 'true';
    event.stopPropagation();
  };
  const unavailable = () => { if (!current()) stop(); else disable(); };
  const pointerDown = event => {
    if (fit !== null && scale === fit) { clickStart(event); return; }
    const geometry = wheelGeometry(true);
    if (clickMode(geometry) === 'out') clickStart(event, geometry);
    else panDown(event); // Never a fallback after an overflow pan refusal.
  };
  root.__nativeImageZoom = stop;
  disable();
  slider.addEventListener('input', zoom);
  fitButton.addEventListener('click', reset);
  plus.addEventListener('click', zoomIn);
  minus.addEventListener('click', zoomOut);
  viewport.addEventListener('pointerdown', pointerDown);
  viewport.addEventListener('wheel', wheel, wheelOptions);
  image.addEventListener('load', measure);
  image.addEventListener('error', unavailable);
  resize = new ResizeObserver(measure); resize.observe(viewport);
  mutation = new MutationObserver(() => { if (!current()) stop(); });
  mutation.observe(document.body, {childList:true, subtree:true, attributes:true,
    attributeFilter:['id','src','data-image-zoom-source']});
  measure();
}
if (typeof module !== 'undefined') module.exports = {nativeImageFit, nativeImageScale, nativeImageStep, installNativeImageZoom};
