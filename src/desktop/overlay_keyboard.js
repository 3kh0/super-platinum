// Shared shell dialogs keep keyboard focus out of the obscured conversation.
(() => {
  if (window.superPlatinumOverlayKeyboard) return;
  // Keep IME confirmation/cancel/navigation out of every application handler,
  // including Dioxus's bubbling event bridge. WebKit may send compositionend
  // before its final keydown: that event has isComposing=false but keyCode=229.
  // Do not preventDefault: the input method must still commit/edit native text.
  document.addEventListener('keydown', event => {
    if (event.isComposing || event.keyCode === 229) event.stopImmediatePropagation();
  }, true);
  const selector = '[data-keyboard-dialog]';
  const visible = element => element?.isConnected && element.getClientRects().length > 0
    && getComputedStyle(element).visibility !== 'hidden'
    && !element.closest('[hidden], [inert]');
  const current = () => [...document.querySelectorAll(selector)].filter(visible).at(-1);
  let pending;
  const closing = new WeakSet();
  const cancelPending = () => {
    if (pending) clearTimeout(pending.timer);
    pending = undefined;
  };
  const selectInput = field => {
    field.focus({preventScroll: true});
    if (field.dataset.focusAtEnd === 'true') {
      field.setSelectionRange(field.value.length, field.value.length);
    } else {
      field.select();
    }
  };
  const insert = (field, text) => field.setRangeText(text, field.selectionStart, field.selectionEnd, 'end');
  const replay = (field, events) => {
    let dirty = false;
    const flush = () => {
      if (dirty) field.dispatchEvent(new Event('input', {bubbles: true}));
      dirty = false;
    };
    for (const event of events) {
      if (['Enter', 'ArrowDown', 'ArrowUp'].includes(event.key)) {
        flush();
        field.dispatchEvent(new KeyboardEvent('keydown', {key: event.key, bubbles: true, cancelable: true}));
        if (event.key === 'Enter') return;
        continue;
      }
      dirty = true;
      if (event.text !== undefined) insert(field, event.text);
      else if (event.key === 'Backspace' || event.key === 'Delete') {
        let start = field.selectionStart, end = field.selectionEnd;
        if (start === end) {
          if (event.key === 'Backspace') start -= [...field.value.slice(0, start)].at(-1)?.length ?? 0;
          else end += [...field.value.slice(end)][0]?.length ?? 0;
        }
        field.setRangeText('', start, end, 'end');
      } else if (event.key === 'selectAll') field.select();
    }
    flush();
  };
  // The native render round trip can outlast the first keystrokes after Cmd+K.
  // Replay only into that command's input, never the obscured composer.
  document.addEventListener('keydown', event => {
    if (event.key === 'Escape') {
      if (event.target?.closest?.('[data-escape-local]')) return;
      const dialog = current();
      if (dialog) closing.add(dialog);
      cancelPending();
      return;
    }
    if (!pending) return;
    if (event.isComposing || event.altKey) return;
    let edit;
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'a') edit = {key: 'selectAll'};
    else if (!event.metaKey && !event.ctrlKey) {
      if ([...event.key].length === 1) edit = {text: event.key};
      else if (['Backspace', 'Delete', 'Enter', 'ArrowDown', 'ArrowUp'].includes(event.key)) edit = {key: event.key};
    }
    if (!edit) return;
    pending.edits.push(edit);
    event.preventDefault();
    event.stopImmediatePropagation();
  }, true);
  document.addEventListener('paste', event => {
    if (!pending || !event.clipboardData) return;
    pending.edits.push({text: event.clipboardData.getData('text/plain')});
    event.preventDefault();
    event.stopImmediatePropagation();
  }, true);
  let previous = document.activeElement;
  document.addEventListener('focusin', event => {
    if (!current()) previous = event.target;
  });
  // WebKit on macOS does not always focus a button when it is clicked.
  document.addEventListener('pointerdown', event => {
    cancelPending();
    if (!current()) {
      const opener = event.target.closest('button, a[href], input, select, textarea, [tabindex], [contenteditable="true"]');
      if (opener) previous = opener;
    }
  }, true);
  document.addEventListener('keydown', event => {
    if (event.key !== 'Tab' || event.altKey || event.metaKey || event.ctrlKey) return;
    const dialog = current();
    if (!dialog) return;
    const fields = [...dialog.querySelectorAll('button, input, select, textarea, a[href], [tabindex], [contenteditable="true"]')]
      .filter(element => element.tabIndex >= 0 && !element.matches(':disabled') && visible(element));
    const first = fields[0];
    const last = fields.at(-1);
    const focused = document.activeElement;
    if (!first || !dialog.contains(focused) || focused === dialog
        || (event.shiftKey ? focused === first : focused === last)) {
      event.preventDefault();
      (event.shiftKey ? last : first)?.focus({preventScroll: true});
      if (!first) dialog.focus({preventScroll: true});
    }
  }, true);
  // Recipient selection clears the native field before the next keystroke.
  // Do not dispatch input here: Rust must select from the current query first.
  const resetRecipientInput = field => {
    field.value = '';
    field.setSelectionRange(0, 0);
    field.focus({preventScroll: true});
  };
  document.addEventListener('keydown', event => {
    const field = event.target;
    if (event.key !== 'Enter' || event.isComposing || field?.dataset?.recipientInput !== 'true' || field.disabled) return;
    const option = document.getElementById(field.getAttribute('aria-activedescendant'));
    if (option && !option.disabled) resetRecipientInput(field);
  }, true);
  document.addEventListener('click', event => {
    const option = event.target.closest?.('[data-recipient-option]');
    if (!option || option.disabled) return;
    const field = current()?.querySelector('[data-recipient-input]');
    if (field && !field.disabled) resetRecipientInput(field);
  }, true);
  const initializedInputs = new WeakSet();
  let observedRoot;
  let rootObserver;
  window.superPlatinumOverlayKeyboard = {
    prepare(kind) {
      cancelPending();
      if (!document.getElementById('shell-overlays')) return;
      const dialog = current();
      const field = dialog?.querySelector('[data-dialog-input]');
      if (kind === 'palette' && dialog?.dataset.dialogKind === kind && field && !closing.has(dialog)) {
        selectInput(field);
        return;
      }
      pending = {kind, dialog, generation: dialog?.dataset.dialogGeneration, edits: [], timer: setTimeout(cancelPending, 1500)};
    },
    cancelPending,
    observe(root) {
      if (!root || root === observedRoot) return;
      rootObserver?.disconnect();
      if (observedRoot) cancelPending();
      observedRoot = root;
      const initialize = () => {
        for (const dialog of root.querySelectorAll(selector)) {
          this.mount(dialog);
          for (const field of dialog.querySelectorAll('[data-dialog-input]')) {
            if (!visible(field)) continue;
            const requested = pending && pending.kind === dialog.dataset.dialogKind
              && (dialog !== pending.dialog || dialog.dataset.dialogGeneration !== pending.generation);
            if (requested) {
              const edits = pending.edits;
              cancelPending();
              closing.delete(dialog);
              initializedInputs.add(field);
              // Escape and reopen can be coalesced into one Rust render. The
              // request generation acknowledges the new visit even if its
              // input DOM node was reused. Restore the requested query/prefix
              // only here, never on ordinary result renders or native typing.
              if (field.dataset.dialogValue !== undefined) field.value = field.dataset.dialogValue;
              selectInput(field);
              replay(field, edits);
            } else if (!initializedInputs.has(field)) {
              initializedInputs.add(field);
              selectInput(field);
            }
          }
        }
      };
      rootObserver = new MutationObserver(initialize);
      // Only the overlay slot, never the message timeline or whole document.
      rootObserver.observe(root, {childList: true, subtree: true, attributes: true, attributeFilter: ['data-dialog-generation', 'data-dialog-kind']});
      initialize();
    },
    mount(dialog) {
      if (!dialog || dialog.dataset.keyboardMounted) return;
      dialog.dataset.keyboardMounted = 'true';
      const returnTo = previous;
      const observer = new MutationObserver(() => {
        if (dialog.isConnected) return;
        observer.disconnect();
        // Respect focus explicitly moved by navigation or a replacement dialog.
        if (!current() && visible(returnTo)
            && (!document.activeElement || document.activeElement === document.body)) {
          returnTo.focus({preventScroll: true});
        }
      });
      observer.observe(dialog.parentNode, {childList: true});
    }
  };
  window.superPlatinumOverlayKeyboard.observe(document.getElementById('shell-overlays'));
})();
