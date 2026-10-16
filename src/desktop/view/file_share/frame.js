((rootId, generation, selectors, scroll) => {
  const dialog = document.getElementById(rootId);
  if (!dialog || dialog.dataset.dialogGeneration !== generation) return;
  requestAnimationFrame(() => {
    if (!dialog.isConnected || document.getElementById(rootId) !== dialog || dialog.dataset.dialogGeneration !== generation) return;
    const visible = e => e.isConnected && e.getClientRects().length && getComputedStyle(e).visibility !== 'hidden' && !e.closest('[hidden], [inert]');
    const top = [...document.querySelectorAll('[data-keyboard-dialog]')].filter(visible).at(-1);
    if (top !== dialog) return;
    const target = selectors.map(selector => dialog.querySelector(selector)).find(e => e && !e.disabled && visible(e));
    if (!target) return;
    if (scroll) target.scrollIntoView({block:'nearest',inline:'nearest'});
    else target.focus({preventScroll:true});
  });
})
