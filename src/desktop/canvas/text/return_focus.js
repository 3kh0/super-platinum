// One observer per owned editor. Mapping may restore controls after unmount.
function mountCanvasText(generation, visit, section, removesOriginal) {
  const editor = document.getElementById('canvas-text-editor');
  if (editor?.dataset.editorGeneration !== String(generation) || editor.dataset.canvasVisit !== String(visit)) return;
  const dialog = editor.closest('.canvas-dialog[data-dialog-generation]');
  const owned = () => dialog?.isConnected && document.getElementById(dialog.id) === dialog
    && dialog.dataset.dialogGeneration === String(visit)
    && ((dialog.id === 'shell-overlay-dialog' && dialog.getAttribute('role') === 'dialog')
      || (dialog.id === 'channel-canvas-panel' && dialog.getAttribute('role') === 'region'));
  if (!owned()) return;
  let stopped = false, timer = null;
  const stop = () => {
    if (stopped) return;
    stopped = true;
    observer.disconnect();
    if (timer !== null) clearTimeout(timer);
    document.removeEventListener('focusin', choice, true);
    document.removeEventListener('pointerdown', choice, true);
    document.removeEventListener('keydown', choice, true);
    window.removeEventListener('pagehide', stop);
  };
  const choice = event => {
    if (!editor.isConnected && event.target !== document.body && !editor.contains(event.target)) stop();
    else if (!editor.isConnected && event.type !== 'focusin') stop();
  };
  const check = () => {
    if (stopped) return;
    if (!owned() || editor.dataset.editorGeneration !== String(generation)
      || editor.dataset.canvasVisit !== String(visit)) { stop(); return; }
    if (editor.isConnected) {
      if (document.getElementById('canvas-text-editor') !== editor) stop();
      return;
    }
    if (dialog.querySelector('#canvas-text-editor')) { stop(); return; }
    if (timer === null) timer = setTimeout(stop, 2000);
    const active = document.activeElement;
    if (active && active !== document.body && !editor.contains(active)) { stop(); return; }
    const rows = [...dialog.querySelectorAll('[data-canvas-text-section]')];
    const originals = rows.filter(row => row.dataset.canvasTextSection === section);
    if (originals.length > 1) return;
    const replacement = editor.dataset.replacementTarget;
    const wanted = originals.length ? section : (replacement || section);
    const matches = rows.filter(row => row.dataset.canvasTextSection === wanted);
    if (matches.length > 1) return;
    let paragraph = matches[0], target = paragraph?.querySelector('.canvas-text-open:not(:disabled)');
    // An existing row with delayed/disabled controls is never permission to
    // focus another row. Only a committed deletion may fall back to a sibling.
    if (!paragraph && !replacement && removesOriginal === true) {
      target = dialog.querySelector('.canvas-paragraph-actions button:not(:disabled)');
      paragraph = target?.closest('[data-canvas-text-section]');
    }
    const scroll = dialog.querySelector('.canvas-scroll');
    if (!target || target.disabled || !paragraph || !scroll?.contains(paragraph)
      || !target.getClientRects().length) return;
    stop();
    target.focus({preventScroll:true});
    // Center the complete row, rather than just its shorter Edit button, so
    // multiline text and the control focus outline share the visible region.
    paragraph.scrollIntoView({block:'center', inline:'nearest'});
  };
  const observer = new MutationObserver(check);
  observer.observe(document.documentElement, {childList:true, subtree:true, attributes:true,
    attributeFilter:['disabled', 'data-editor-generation', 'data-canvas-visit', 'data-dialog-generation']});
  document.addEventListener('focusin', choice, true);
  document.addEventListener('pointerdown', choice, true);
  document.addEventListener('keydown', choice, true);
  window.addEventListener('pagehide', stop);
  check();
}
