// Synthetic events through the actual WebKit/Dioxus bridge, never a live session.
return await (async () => {
  const wait = ms => new Promise(resolve => setTimeout(resolve, ms));
  const ids = ['channel-composer', 'thread-composer'];
  for (let attempt = 0; attempt < 100; attempt++) {
    if (window.superPlatinumOverlayKeyboard && ids.every(id => document.getElementById(id))) break;
    await wait(25);
  }
  let bubbled = 0;
  const observe = () => bubbled++;
  window.addEventListener('keydown', observe);
  try {
    for (const id of ids) {
      const field = document.getElementById(id);
      if (!field) throw new Error('Fixture composer not mounted');
      const text = `日本語 IME ${id}`;
      field.focus();
      field.value = text;
      field.dispatchEvent(new Event('input', {bubbles:true}));
      await wait(120);
      for (const key of ['Enter', 'Escape', 'b']) {
        for (const webkitFinal of [false, true]) {
          field.dispatchEvent(new CompositionEvent('compositionstart', {bubbles:true}));
          // Exercise both normal ordering and WebKit's final-key ordering.
          if (webkitFinal) field.dispatchEvent(new CompositionEvent('compositionend', {bubbles:true,data:'日本語'}));
          const before = bubbled;
          const event = new KeyboardEvent('keydown', {
            key, bubbles:true, cancelable:true, metaKey:key==='b',
            isComposing:!webkitFinal, keyCode:webkitFinal ? 229 : key==='Enter' ? 13 : 0,
          });
          const defaultAllowed = field.dispatchEvent(event);
          if (!webkitFinal) field.dispatchEvent(new CompositionEvent('compositionend', {bubbles:true,data:'日本語'}));
          await wait(120);
          dioxus.send({kind:'guarded',field:id,case:`${key}-${webkitFinal?'final':'composing'}`,
            text,defaultAllowed,bubbled:bubbled!==before,domUnchanged:field.value===text});
        }
      }
      // Positive control: normal Enter must still reach Rust and queue a send.
      const before = bubbled;
      field.dispatchEvent(new KeyboardEvent('keydown', {key:'Enter',code:'Enter',keyCode:13,bubbles:true,cancelable:true}));
      await wait(200);
      dioxus.send({kind:'send',field:id,case:'ordinary-enter',bubbled:bubbled!==before,domEmpty:field.value===''});
    }
    dioxus.send({kind:'done'});
  } catch (error) {
    dioxus.send({kind:'error',case:String(error)});
  } finally {
    window.removeEventListener('keydown', observe);
  }
})();
