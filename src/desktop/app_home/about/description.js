// Local display state only. No IPC, transport, user input, or focus movement.
function installAppAboutDescription(id) {
  const roots = document.querySelectorAll(`#${CSS.escape(id)}`);
  if (roots.length !== 1) return;
  const root = roots[0], panel = root.closest('#app-about');
  const bodies = root.querySelectorAll('[data-about-body]');
  const buttons = root.querySelectorAll('[data-about-toggle]');
  if (!panel || bodies.length !== 1 || buttons.length !== 1) return;
  const body = bodies[0], button = buttons[0];
  const visit = root.dataset.aboutVisit, revision = root.dataset.aboutRevision;
  root.__aboutDescription?.();
  let expanded = false, stopped = false, resize, mutation;
  const current = () => !stopped && root.__aboutDescription === stop
    && root.isConnected && panel.isConnected && root.closest('#app-about') === panel
    && panel.dataset.visit === visit && panel.dataset.revision === revision
    && root.querySelectorAll('[data-about-body]')[0] === body
    && root.querySelectorAll('[data-about-toggle]')[0] === button;
  const stop = () => {
    if (stopped) return;
    stopped = true;
    resize?.disconnect(); mutation?.disconnect();
    button.removeEventListener('click', toggle);
    body.removeEventListener('focusin', reveal);
    if (root.__aboutDescription === stop) delete root.__aboutDescription;
  };
  const measure = () => {
    if (!current()) { stop(); return; }
    const overflow = body.scrollHeight > body.clientHeight || body.scrollWidth > body.clientWidth;
    button.hidden = !(expanded || overflow || document.activeElement === button);
    button.textContent = expanded ? 'Show less' : 'Show more';
    button.setAttribute('aria-expanded', String(expanded));
  };
  const setExpanded = value => {
    expanded = value; body.dataset.expanded = String(value); measure();
  };
  const toggle = () => {
    if (!current()) { stop(); return; }
    setExpanded(!expanded);
  };
  const reveal = event => {
    if (!current()) { stop(); return; }
    if (expanded || !body.contains(event.target)) return;
    const target = event.target.getBoundingClientRect(), clip = body.getBoundingClientRect();
    // A keyboard-reached link must not remain visually hidden by the clamp.
    if (target.bottom > clip.bottom || target.top < clip.top) setExpanded(true);
  };
  root.__aboutDescription = stop;
  button.addEventListener('click', toggle);
  body.addEventListener('focusin', reveal);
  resize = new ResizeObserver(measure); resize.observe(body);
  mutation = new MutationObserver(records => {
    if (!current()) stop();
    else if (records.some(record => body === record.target || body.contains(record.target))) measure();
  });
  mutation.observe(document.body, { childList: true, characterData: true, subtree: true, attributes: true, attributeFilter: ['data-visit', 'data-revision'] });
  body.dataset.expanded = 'false';
  measure();
}
