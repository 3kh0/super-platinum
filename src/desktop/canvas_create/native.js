(() => {
  if (window.superPlatinumCanvasCreation) return;
  const mounts = new Map();
  const current = scope => {
    const m = mounts.get(scope);
    return m && m.title.isConnected && m.body.isConnected &&
      document.getElementById('canvas-create-title') === m.title &&
      document.getElementById('canvas-create-body') === m.body ? m : null;
  };
  window.superPlatinumCanvasCreation = {
    install(scope, editable) {
      const title = document.getElementById('canvas-create-title');
      const body = document.getElementById('canvas-create-body');
      if (!title || !body || title.dataset.canvasVisit !== scope || body.dataset.canvasVisit !== scope) return false;
      if (current(scope)) return true;
      const m = { title, body, composing: new Set(), seal: null, editable, cleanup: [] };
      for (const node of [title, body]) {
        const start = () => m.composing.add(node);
        const end = () => m.composing.delete(node);
        node.addEventListener('compositionstart', start);
        node.addEventListener('compositionend', end);
        m.cleanup.push(() => { node.removeEventListener('compositionstart', start); node.removeEventListener('compositionend', end); });
        // Initial readOnly is deliberate: no provisional input can precede
        // installation of the composition observer.
        node.readOnly = !editable;
      }
      mounts.set(scope, m);
      return true;
    },
    capture(scope, token) {
      const m = current(scope);
      if (!m || m.seal) return { status: 'unavailable' };
      if (m.composing.size) return { status: 'composing' };
      const title = m.title.value, markdown = m.body.value;
      if (new TextEncoder().encode(title).length > 16384 || new TextEncoder().encode(markdown).length > 2097152)
        return { status: 'capacity' };
      // This synchronous seal prevents edits between reading the pair and its
      // custody acknowledgement. Values/selection are never rewritten.
      m.title.readOnly = m.body.readOnly = true;
      m.seal = token;
      return { status: 'sealed', title, markdown };
    },
    release(scope, token) {
      const m = current(scope);
      if (!m || m.seal !== token) return false;
      m.seal = null;
      m.title.readOnly = m.body.readOnly = !m.editable;
      return true;
    },
    finish(scope, token, editable) {
      const m = current(scope);
      if (!m || m.seal !== token) return false;
      m.editable = editable;
      m.seal = null;
      m.title.readOnly = m.body.readOnly = !editable;
      return true;
    },
    editable(scope, enabled) {
      const m=current(scope);
      if (!m || m.seal || m.composing.size) return false;
      m.editable=enabled;
      m.title.readOnly=m.body.readOnly=!enabled;
      return true;
    },
    retire(scope) {
      const m = mounts.get(scope);
      if (!m) return;
      m.cleanup.forEach(f => f());
      mounts.delete(scope);
    }
  };
})();
