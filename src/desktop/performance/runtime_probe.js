(() => {
  const started = performance.now();
  const endpoint = location.protocol === 'http:' || location.protocol === 'https:'
    ? 'http://super-platinum-probe.runtime/sample'
    : 'super-platinum-probe://runtime/sample';
  let stopped = false;
  let reports = 0;
  let rafCallbacks = 0;
  let scriptErrors = 0;
  let resourceErrors = 0;
  let unhandledRejections = 0;
  let observedSocket = null;
  let socketHandlers = null;
  let socketGeneration = 0;
  let socketSince = null;
  let socketCounts = null;
  const timers = [];
  const frames = [];
  const count = value => Number.isInteger(value) && value >= 0
    ? Math.min(value, 1000000) : null;
  function stop() {
    if (stopped) return;
    stopped = true;
    detachSocket();
    timers.forEach(clearTimeout);
    frames.forEach(cancelAnimationFrame);
    document.removeEventListener('DOMContentLoaded', ready);
    document.removeEventListener('visibilitychange', visibility);
    window.removeEventListener('load', loaded);
    window.removeEventListener('error', error, true);
    window.removeEventListener('unhandledrejection', rejection);
  }
  function detachSocket() {
    if (observedSocket && socketHandlers) {
      for (const [type, handler] of Object.entries(socketHandlers)) {
        observedSocket.removeEventListener(type, handler);
      }
    }
    observedSocket = null;
    socketHandlers = null;
    socketSince = null;
    socketCounts = null;
  }
  // Rebind only at the existing report cadence. This is incomplete coverage:
  // earlier messages may already have arrived. Never read event.data, URLs,
  // close reasons, keys or edit bytes, and never wrap any interpreter method.
  function observeSocket(candidate, elapsed) {
    if (candidate === observedSocket) return;
    detachSocket();
    if (!candidate || typeof candidate.addEventListener !== 'function'
        || typeof candidate.removeEventListener !== 'function') return;
    observedSocket = candidate;
    socketGeneration++;
    socketSince = Math.floor(elapsed);
    socketCounts = { message: 0, open: 0, close: 0, error: 0 };
    socketHandlers = {};
    for (const type of Object.keys(socketCounts)) {
      const handler = () => {
        if (stopped || observedSocket !== candidate) return;
        if (performance.now() - started >= 20000) { stop(); return; }
        socketCounts[type] = Math.min(socketCounts[type] + 1, 1000000);
      };
      socketHandlers[type] = handler;
      candidate.addEventListener(type, handler);
    }
  }
  function report(phase) {
    if (stopped) return;
    const elapsed = performance.now() - started;
    if (elapsed > 20000 || reports >= 16) { stop(); return; }
    reports++;
    const interpreter = window.interpreter;
    const socket = interpreter?.edits;
    observeSocket(socket, elapsed);
    const socketState = socket?.readyState;
    const root = document.getElementById('main');
    const sample = {
      phase,
      elapsed_ms: Math.floor(elapsed),
      ready: document.readyState,
      visibility: document.visibilityState,
      focused: document.hasFocus(),
      interpreter: !!interpreter,
      headless: typeof interpreter?.headless === 'boolean' ? interpreter.headless : null,
      socket_state: Number.isInteger(socketState) && socketState >= 0 && socketState <= 3
        ? socketState : null,
      socket_observer_generation: observedSocket ? socketGeneration : null,
      socket_observer_since_ms: socketSince,
      socket_messages: socketCounts?.message ?? null,
      socket_opens: socketCounts?.open ?? null,
      socket_closes: socketCounts?.close ?? null,
      socket_errors: socketCounts?.error ?? null,
      // bufferedAmount is an outgoing queue size, NOT an ACK counter.
      socket_buffered_bytes: count(socket?.bufferedAmount),
      interpreter_nodes: Array.isArray(interpreter?.nodes) ? count(interpreter.nodes.length) : null,
      interpreter_stack: Array.isArray(interpreter?.stack) ? count(interpreter.stack.length) : null,
      queued_batches: Array.isArray(interpreter?.queuedBytes) ? count(interpreter.queuedBytes.length) : null,
      root_children: count(root?.childElementCount ?? 0),
      raf_callbacks: rafCallbacks,
      script_errors: scriptErrors,
      resource_errors: resourceErrors,
      unhandled_rejections: unhandledRejections,
    };
    // No credentials, response data, DOM content, or location is transmitted.
    void fetch(endpoint, { method: 'POST', credentials: 'omit', cache: 'no-store',
      headers: { 'Content-Type': 'text/plain' }, body: JSON.stringify(sample)
    }).catch(() => {});
  }
  function frame() {
    frames.push(requestAnimationFrame(() => { if (!stopped) rafCallbacks++; }));
  }
  function ready() { report('dom_ready'); }
  function loaded() { frame(); report('loaded'); }
  function visibility() { report('visibility'); }
  // Count only; never read message/filename/error/reason or cancel propagation.
  // The pinned interpreter's Blob conversion uses a Promise, so a failed edit
  // application can appear as an unhandled rejection while its socket stays open.
  function error(event) {
    if (stopped) return;
    if (performance.now() - started >= 20000) { stop(); return; }
    if (event instanceof ErrorEvent) scriptErrors = Math.min(scriptErrors + 1, 1000000);
    else resourceErrors = Math.min(resourceErrors + 1, 1000000);
  }
  function rejection() {
    if (stopped) return;
    if (performance.now() - started >= 20000) { stop(); return; }
    unhandledRejections = Math.min(unhandledRejections + 1, 1000000);
  }
  document.addEventListener('DOMContentLoaded', ready, { once: true });
  document.addEventListener('visibilitychange', visibility);
  window.addEventListener('load', loaded, { once: true });
  window.addEventListener('error', error, true);
  window.addEventListener('unhandledrejection', rejection);
  report('initial');
  frame();
  for (const delay of [1000, 2000, 5000, 10000]) {
    timers.push(setTimeout(() => report('timer'), delay));
  }
  timers.push(setTimeout(() => { report('finished'); stop(); }, 20000));
})();
