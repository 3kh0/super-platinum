// Retain original nodes only. Dismissing a nested dialog must not focus a
// conversation, a replaced comment, or a successor picker with the same id.
function mountListReactionPicker(options, environment = globalThis) {
  const {document, MutationObserver} = environment;
  const panel = document.getElementById('slack-list-comments');
  const picker = document.getElementById('slack-list-reaction-picker');
  const dialog = panel?.closest('.slack-list-dialog');
  if (!panel || !picker || !dialog || !panel.contains(picker)
      || panel.dataset.commentsGeneration !== String(options.generation)
      || picker.dataset.reactionPicker !== String(options.token)) return false;
  const opener = [...panel.querySelectorAll('[data-list-reaction-add]')]
    .find(node => node.dataset.listReactionAdd === options.ts);
  const visible = node => node?.isConnected && !node.disabled
    && !node.closest('[hidden],[inert]') && node.getClientRects().length > 0;
  const observer = new MutationObserver(() => {
    if (picker.isConnected) return;
    observer.disconnect();
    if (!panel.isConnected || !dialog.isConnected
        || document.getElementById('slack-list-comments') !== panel
        || panel.dataset.commentsGeneration !== String(options.generation)
        || panel.querySelector('#slack-list-reaction-picker')) return;
    const active = document.activeElement;
    if (active && active !== document.body && active.isConnected) return;
    if (visible(opener)) opener.focus({preventScroll: true});
  });
  observer.observe(panel, {childList: true, subtree: true});
  return true;
}
