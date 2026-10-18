(function captureProfileDescription(visit, serial, key) {
  const root = document.getElementById('shell-overlay-dialog');
  const pane = root?.querySelector('#profile-section-rich-pane');
  const field = pane?.querySelector('#profile-section-rich');
  const current = () => root?.isConnected && pane?.isConnected && field?.isConnected &&
    document.getElementById('shell-overlay-dialog') === root &&
    document.getElementById('profile-section-rich') === field &&
    root.dataset.dialogGeneration === String(visit) &&
    root.dataset.sectionSerial === String(serial) &&
    root.dataset.sectionLocked === 'false' && pane.dataset.richVisit === String(key);
  if (!current() || pane.dataset.composing !== 'false' || !field.isContentEditable) return null;
  const status = pane.querySelector('#profile-section-rich-status');
  if (status) status.textContent = '';
  let captured;
  try { captured = window.superPlatinumRichEditor.resolved(field); } catch {}
  if (!captured || !Array.isArray(captured.snapshot?.blocks) || !Array.isArray(captured.tokens) ||
      captured.snapshot.composing || captured.snapshot.generation !== String(serial) ||
      !Number.isSafeInteger(captured.snapshot.revision) || captured.snapshot.revision < 0) {
    if (status) status.textContent = 'Could not read this description. Your text is still here. Try again.';
    return null;
  }
  const buttons = [...pane.querySelectorAll('button')].map(e => [e, e.disabled]);
  field.contentEditable = 'false';
  buttons.forEach(([e]) => e.disabled = true);
  let active = true;
  let expire;
  const expired = new Promise(resolve => expire = resolve);
  const ticket = {
    captured, expired,
    release() {
      if (!active) return;
      active = false;
      clearTimeout(watchdog);
      if (field.__profileCapture !== ticket) return;
      delete field.__profileCapture;
      if (!current()) return;
      field.contentEditable = 'true';
      buttons.forEach(([e, disabled]) => { if (e.isConnected) e.disabled = disabled; });
    },
    fail() {
      if (active && current() && field.__profileCapture === ticket && status?.isConnected)
        status.textContent = 'Could not use this description. Your text is still here. Try again.';
      ticket.release();
    }
  };
  field.__profileCapture = ticket;
  const watchdog = setTimeout(() => { ticket.fail(); expire('expired'); }, 5000);
  return ticket;
})
