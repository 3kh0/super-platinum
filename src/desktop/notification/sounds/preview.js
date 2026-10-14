// One bounded local preview at a time. Stale play/error callbacks cannot replace
// a later selection or restart audio after leaving the settings page.
(() => {
  if (window.__notificationSoundPreview) return;
  let audio = null, generation = 0, timer = null;
  const api = {state: 'idle', name: null, currentTime: 0, duration: 0, error: null};
  function publish(state, error = null) {
    Object.assign(api, {state, error, currentTime: audio?.currentTime || 0,
      duration: Number.isFinite(audio?.duration) ? audio.duration : 0});
    const status = document.getElementById('notification-sound-preview-status');
    if (status) status.textContent = state === 'error' ? "Couldn't play this preview."
      : state === 'loading' ? 'Loading preview…' : state === 'playing' ? 'Playing preview…'
      : state === 'ended' ? 'Preview finished.' : '';
  }
  function retire() {
    generation++;
    clearTimeout(timer); timer = null;
    if (audio) {
      audio.onended = audio.onerror = null;
      audio.pause(); audio.removeAttribute('src'); audio.load(); audio = null;
    }
  }
  api.stop = () => {retire(); api.name = null; publish('idle');};
  api.play = async name => {
    retire();
    const turn = generation;
    api.name = name;
    audio = new Audio('/notification-sounds/' + encodeURIComponent(name));
    const current = audio;
    publish('loading');
    function failed() {
      if (turn !== generation) return;
      retire(); publish('error', 'playback_failed');
    }
    audio.onerror = failed;
    audio.onended = () => {
      if (turn !== generation) return;
      clearTimeout(timer); timer = null;
      publish('ended');
    };
    timer = setTimeout(failed, 15000);
    try {
      await audio.play();
      if (turn !== generation) {current.pause(); return;}
      if (api.state !== 'ended') publish('playing');
    } catch (_) {failed();}
  };
  window.__notificationSoundPreview = api;
})();
