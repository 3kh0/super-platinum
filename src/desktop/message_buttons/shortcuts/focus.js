(() => {
  const root = document.getElementById('app-shortcut-dialog');
  if (!root || root.dataset.dialogGeneration !== '__OWNER__') return;
  const top = () => [...document.querySelectorAll('[data-keyboard-dialog]')].filter(e => e.isConnected && e.getClientRects().length && !e.closest('[hidden],[inert]')).at(-1);
  let owned;
  const remember = event => { if (root.contains(event.target)) owned = event.target; };
  root.addEventListener('focusin', remember);
  const observer = new MutationObserver(() => {
    if (!root.isConnected) { observer.disconnect(); root.removeEventListener('focusin', remember); return; }
    if (top() !== root) return;
    const active = document.activeElement;
    if (owned && (!owned.isConnected || owned.disabled)
        && (!active || active === document.body || active === owned)) {
      const close = root.querySelector('#app-shortcut-close');
      if (close && !close.disabled) close.focus({preventScroll:true});
    }
  });
  observer.observe(root.parentNode, {subtree:true, childList:true, attributes:true, attributeFilter:['disabled']});
  requestAnimationFrame(() => {
    if (root.isConnected && top() === root && (!document.activeElement || document.activeElement === document.body || !document.activeElement.closest('[data-keyboard-dialog]'))) {
      (root.querySelector('#app-shortcut-query') || root.querySelector('#app-shortcut-close'))?.focus({preventScroll:true});
    }
  });
})();
