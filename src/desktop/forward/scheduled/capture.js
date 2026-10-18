// An exact native snapshot seal. The host deadline is shorter than the watchdog.
// No clipboard, focus movement, synthetic submission or request occurs here.
async function captureScheduledForward(config, bridge, env = globalThis) {
  const root = env.document.getElementById(config.root);
  const editor = env.document.getElementById(config.editor);
  const date = env.document.getElementById(config.date);
  const current = () => root?.isConnected && root.dataset.scheduledForwardOwner === config.owner &&
      editor?.isConnected && date?.isConnected && root.contains(editor) && root.contains(date) &&
      env.document.getElementById(config.root) === root &&
      env.document.getElementById(config.editor) === editor && env.document.getElementById(config.date) === date;
  const locked = () => root.dataset.scheduledForwardBusy === 'true' && editor.contentEditable === 'false' && date.disabled;
  if (!current() || root.dataset.scheduledForwardCapture) throw Error('The scheduling editor changed.');
  // The synchronous host busy reservation owns editability. Do not restore
  // stale disabled bits over a later Dioxus render or another visit.
  let timer = null, released = false, sent = false;
  const release = () => {
    if (released) return; released = true;
    if (timer !== null) env.clearTimeout(timer);
    if (root.dataset.scheduledForwardCapture !== config.token) return;
    delete root.dataset.scheduledForwardCapture;
  };
  root.dataset.scheduledForwardCapture = config.token;
  try {
    timer = env.setTimeout(release, 5000);
    // The synchronous Rust reservation precedes the DOM commit. Wait only for
    // that exact visit's readonly render, never a replacement with the same ID.
    if (!locked()) {
      const started = env.performance.now();
      for (;;) {
        await new Promise(resolve => env.setTimeout(resolve, 16));
        const elapsed = env.performance.now() - started;
        if (!current() || root.dataset.scheduledForwardCapture !== config.token ||
            !Number.isFinite(elapsed) || elapsed < 0 || elapsed >= 500) throw Error('The scheduling editor did not become ready.');
        if (locked()) break;
      }
    }
    if (!config.as_draft && (!date.validity.valid || !date.value)) throw Error('Choose a valid local date and time.');
    const time = config.as_draft ? 0 : Date.parse(date.value);
    if (!Number.isFinite(time)) throw Error('Choose a valid local date and time.');
    const blocks = env.superPlatinumRichEditor.readForCommit(config.editor);
    bridge.send({token: config.token, owner: config.owner, blocks, post_at: Math.floor(time / 1000)});
    sent = true;
    const ack = await bridge.recv();
    if (ack !== config.token) throw Error('Scheduling acknowledgement changed.');
    release();
  } catch (error) {
    // After send, Rust may hold the value. Keep the seal through its shorter
    // admission deadline, even if the return bridge fails synchronously.
    if (!sent) release();
    throw error;
  }
}
if (typeof module !== 'undefined') module.exports = {captureScheduledForward};
