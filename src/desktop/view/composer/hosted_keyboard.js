(() => {
  const root = document.getElementById('hosted-file-picker');
  if (!root || root.dataset.keyboardReady) return;
  root.dataset.keyboardReady = 'true';
  const visible = el => el.isConnected && el.getClientRects().length && !el.disabled;
  root.addEventListener('keydown', event => {
    if (event.key !== 'Tab') return;
    const nodes = [...root.querySelectorAll('button,input,select,[tabindex="0"]')].filter(visible);
    if (!nodes.length) return;
    const index = nodes.indexOf(document.activeElement);
    if (event.shiftKey && index <= 0) { event.preventDefault(); nodes.at(-1).focus(); }
    else if (!event.shiftKey && (index < 0 || index === nodes.length - 1)) { event.preventDefault(); nodes[0].focus(); }
  });
  requestAnimationFrame(() => {
    if (root.isConnected && document.getElementById('hosted-file-picker') === root)
      root.querySelector('#hosted-picker-query')?.focus({preventScroll:true});
  });
})();
