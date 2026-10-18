// Narrow scheduled-forward handoff. Never adopts a same-ID replacement root.
function mountScheduledForwardFocus(config, env = globalThis) {
  const document = env.document, root = document.getElementById(config.root);
  if (!root || root.dataset[config.ownerKey] !== config.owner) return false;
  const owners = env.superPlatinumScheduledForwardFocus ||= new Map();
  owners.get(config.root)?.();
  let last = null, pending = false, disposed = false;
  const current = () => !disposed && root.isConnected && document.getElementById(config.root) === root
    && root.dataset[config.ownerKey] === config.owner;
  const scope = node => root.contains(node) && (!config.scope || node.closest(config.scope));
  const available = node => node?.isConnected && !node.disabled && !node.hidden
    && node.getAttribute?.('contenteditable') !== 'false'
    && node.getClientRects().length > 0 && env.getComputedStyle(node).visibility === 'visible';
  const dispose = () => {
    if (disposed) return; disposed = true;
    observer.disconnect(); document.removeEventListener('focusin', remember, true);
    if (owners.get(config.root) === dispose) owners.delete(config.root);
  };
  const remember = event => {
    if (!current()) { dispose(); return; }
    if (scope(event.target)) last = event.target;
    else if (event.target !== document.body) last = null;
  };
  const recover = () => {
    pending = false;
    if (!current()) { dispose(); return; }
    const active = document.activeElement;
    if (!last || available(last) || (active && active !== document.body && active !== last)) return;
    // An editor that remains mounted owns its local status; removing the editor
    // instead returns to the inventory heading, never a newly opened editor.
    const editor = last.closest?.('#scheduled-forward-edit');
    const target = editor?.isConnected && root.contains(editor)
      ? editor.querySelector('[data-scheduled-focus-target]')
      : root.querySelector(config.target) || (config.fallback && root.querySelector(config.fallback));
    if (!available(target)) return;
    target.scrollIntoView({block:'nearest',inline:'nearest'});
    if (!current() || (document.activeElement !== active && document.activeElement !== last)) return;
    target.focus({preventScroll:true});
  };
  const observer = new env.MutationObserver(() => {
    if (!pending) { pending = true; env.requestAnimationFrame(recover); }
  });
  document.addEventListener('focusin', remember, true);
  observer.observe(root, {childList:true,subtree:true,attributes:true,
    attributeFilter:['disabled','hidden','class','style','contenteditable','data-scheduled-forward-owner','data-scheduled-focus-owner']});
  if (scope(document.activeElement)) last = document.activeElement;
  owners.set(config.root, dispose);
  return true;
}
if (typeof module !== 'undefined') module.exports = {mountScheduledForwardFocus};
