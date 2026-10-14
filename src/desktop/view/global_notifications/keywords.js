// Renderer-only canonical seeding. Native input wins over delayed application work.
(() => {
  if (window.__globalKeywordSeed) return;
  function field(visit, seed) {
    const panel = document.getElementById('global-notifications');
    const input = document.getElementById('global-keywords-input');
    if (!panel || panel.dataset.visit !== String(visit) || !input ||
        !panel.contains(input) || (seed !== undefined && input.dataset.seed !== String(seed))) return null;
    return {panel, input};
  }
  function install(visit) {
    const found = field(visit);
    if (!found) return null;
    const {panel, input} = found;
    if (input.__keywordSeed?.visit === String(visit)) return found;
    const state = input.__keywordSeed = {
      visit: String(visit), seed: input.dataset.seed, value: input.value,
      edits: 0, seededEdits: 0, composing: false, action: null,
    };
    input.addEventListener('input', () => { state.edits++; }, true);
    input.addEventListener('compositionstart', () => { state.composing = true; state.edits++; }, true);
    input.addEventListener('compositionend', () => { state.composing = false; state.edits++; }, true);
    // Capture in WebKit before the click crosses the asynchronous Rust bridge.
    if (!panel.__keywordClickInstalled) {
      panel.__keywordClickInstalled = true;
      panel.addEventListener('click', event => {
        const button = event.target.closest?.('button');
        const current = document.getElementById('global-keywords-input');
        const active = current?.__keywordSeed;
        if (!button || button.disabled || !panel.contains(button) || !current ||
            !panel.contains(current) || active?.visit !== panel.dataset.visit ||
            !['global-keywords-save', 'global-keywords-discard'].includes(button.id)) return;
        active.action = {seed: active.seed, value: current.value, edits: active.edits};
      }, true);
    }
    input.dataset.appliedSeed = state.seed;
    return found;
  }
  function sync(visit, seed, text) {
    const found = field(visit, seed);
    if (!found) return false;
    const installed = install(visit);
    if (!installed || installed.input !== found.input) return false;
    const {input} = found, state = input.__keywordSeed;
    if (state.seed === String(seed)) return true;
    const expected = state.action?.seed === state.seed ? state.action :
      {value: state.value, edits: state.seededEdits};
    const untouched = !state.composing && state.edits === expected.edits && input.value === expected.value;
    if (untouched && input.value !== text) input.value = text;
    state.seed = String(seed);
    state.value = text;
    state.seededEdits = state.edits;
    state.action = null;
    input.dataset.appliedSeed = state.seed;
    if (!untouched) {
      // The native edit may have crossed the bridge before the reset settled.
      // Republish the surviving value so Rust cannot remain falsely clean.
      input.dispatchEvent(new Event('input', {bubbles:true}));
    }
    return untouched;
  }
  window.__globalKeywordSeed = {install, sync};
})();
