(() => {
  for (const id of ['file-share-dialog', 'saved-file-shares-dialog']) {
    const dialog = document.getElementById(id);
    if (!dialog || dialog.dataset.shareShortcutsInstalled) continue;
    dialog.dataset.shareShortcutsInstalled = 'true';
    // Bubble after editor-local handlers, before the shell's window handler.
    // The shell does not honor defaultPrevented for K/F/G, so stop propagation
    // even when an editor already handled the same shortcut.
    dialog.addEventListener('keydown', event => {
      const key = event.key?.toLowerCase();
      if (!(event.ctrlKey || event.metaKey)) return;
      if (key !== 'k' && !(!event.altKey && !event.shiftKey && (key === 'f' || key === 'g'))) return;
      event.preventDefault();
      event.stopPropagation();
    });
  }
})();
