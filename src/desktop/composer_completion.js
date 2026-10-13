// Native key cancellation must happen before Dioxus's asynchronous event bridge.
// Rust owns candidates and Slack identity; this controller owns focus and selection.
return await (async () => {
  const wait = ms => new Promise(resolve => setTimeout(resolve, ms));
  let field, popup;
  for (let i = 0; i < 60; i++) {
    field = document.getElementById(fieldId);
    popup = document.getElementById(fieldId + '-completion');
    if (field && popup) break;
    await wait(16);
  }
  if (!field || !popup) return;
  window.superPlatinumReferences.mount(field, labels, workspace);
  field.dataset.completionReady = 'true';
  field.setAttribute('aria-autocomplete', 'list');
  field.setAttribute('aria-controls', popup.id);
  let seq = 0, source = '', choices = [], active = 0, composing = false, response, moved = false;
  const snapshot = () => JSON.stringify([field.value, field.selectionStart, field.selectionEnd]);
  const hide = () => {
    popup.hidden = true; choices = []; response = undefined;
    field.setAttribute('aria-expanded', 'false');
    field.removeAttribute('aria-activedescendant');
  };
  const dismiss = () => { seq++; hide(); dioxus.send({kind:'dismiss',seq}); };
  const query = () => {
    if (composing || document.activeElement !== field) return;
    const current = snapshot();
    if (current === source) return;
    source = current; seq++; hide(); moved = false;
    dioxus.send({kind:'query', seq, text:field.value, anchor:field.selectionStart, cursor:field.selectionEnd});
  };
  const highlight = () => {
    [...popup.querySelectorAll('[role=option]')].forEach((row, i) => {
      row.setAttribute('aria-selected', String(i === active));
      if (i === active) {
        field.setAttribute('aria-activedescendant', row.id);
        row.scrollIntoView({block:'nearest'});
      }
    });
  };
  const choose = index => {
    if (!response || response.seq !== seq || source !== snapshot()) return;
    const choice = choices[index];
    if (!choice) return;
    const {start,end,spacer} = response;
    dioxus.send({kind:'choose',seq,token:choice.token});
    // Hide first: execCommand dispatches input synchronously.
    hide();
    window.superPlatinumReferences.insert(field,choice.token,choice.label,start,end,spacer);
    query();
  };
  const keydown = event => {
    if (event.isComposing || event.keyCode === 229 || composing || popup.hidden) return;
    if (event.ctrlKey || event.metaKey || event.altKey || event.shiftKey) return;
    if (event.key === 'Escape') { event.preventDefault(); event.stopImmediatePropagation(); dismiss(); return; }
    if (!choices.length) return;
    if (!['ArrowDown','ArrowUp','Enter','Tab'].includes(event.key)) return;
    event.preventDefault(); event.stopImmediatePropagation();
    if (event.key === 'Enter' || event.key === 'Tab') choose(active);
    else { moved = true; active = (active + (event.key === 'ArrowDown' ? 1 : choices.length - 1)) % choices.length; highlight(); }
  };
  const focus = () => { source = ''; query(); };
  const blur = () => dismiss();
  const begin = () => { composing = true; dismiss(); };
  const end = () => { composing = false; source = ''; query(); };
  const listeners = [['input',query],['keyup',query],['click',query],['select',query],['focus',focus],['blur',blur],
    ['keydown',keydown],['compositionstart',begin],['compositionend',end]];
  for (const [name, handler] of listeners) field.addEventListener(name,handler);
  let stopped;
  const done = new Promise(resolve => { stopped = resolve; });
  const observer = new MutationObserver(() => { if (!field.isConnected || !popup.isConnected) stopped(null); });
  observer.observe(document.body,{childList:true,subtree:true});
  hide(); query();
  try {
    while (field.isConnected && popup.isConnected) {
      const result = await Promise.race([dioxus.recv(), done]);
      if (!result) break;
      if (result.seq !== seq || source !== snapshot() || document.activeElement !== field || composing) continue;
      const previous = moved ? choices[active] : undefined;
      const selected = previous?.token;
      response = result; choices = result.choices;
      // A directory response must not move an explicitly navigated target out
      // from under Enter. Retain that row until the user changes the query.
      if (previous && !choices.some(choice => choice.token === selected)) {
        if (choices.length >= 8) choices.pop();
        choices.push(previous);
      }
      active = Math.max(0, choices.findIndex(choice => choice.token === selected));
      popup.replaceChildren();
      if (!choices.length && !result.status) { hide(); continue; }
      choices.forEach((choice,index) => {
        const row = document.createElement('div');
        row.id = `${popup.id}-${index}`; row.className = 'composer-completion-option';
        row.setAttribute('role','option');
        const label = document.createElement('span'); label.textContent = choice.label;
        const detail = document.createElement('span'); detail.className = 'composer-completion-detail'; detail.textContent = choice.detail;
        row.append(label,detail);
        row.addEventListener('mousedown', event => event.preventDefault());
        row.addEventListener('click', () => choose(index));
        popup.append(row);
      });
      if (result.status) {
        const status = document.createElement('div'); status.className = 'composer-completion-status';
        status.setAttribute('role','status'); status.textContent = result.status; popup.append(status);
      }
      popup.hidden = false; field.setAttribute('aria-expanded','true'); highlight();
    }
  } finally {
    observer.disconnect();
    for (const [name,handler] of listeners) field.removeEventListener(name,handler);
    hide(); delete field.dataset.completionReady;
  }
})();
