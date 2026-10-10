// Native pointer handlers run locally: resizing does not round-trip every
// pixel through Rust. Only the final, clamped geometry is reported and saved.
const HUDDLE_GEOMETRY_KEY = 'super-platinum:huddle-stage';
function clampHuddleRect(rect, width, height) {
  const margin = 12;
  const finite = (value, fallback) => Number.isFinite(value) ? value : fallback;
  const w = Math.min(Math.max(240, finite(rect?.width, 560)), Math.max(200, width - margin * 2));
  const h = Math.min(Math.max(180, finite(rect?.height, 360)), Math.max(150, height - margin * 2));
  return {
    width: w, height: h,
    left: Math.max(0, Math.min(finite(rect?.left, width - w - margin), Math.max(0, width - w - margin))),
    top: Math.max(0, Math.min(finite(rect?.top, height - h - 64), Math.max(0, height - h - margin))),
  };
}
function applyHuddleRect(stage, rect) {
  for (const key of ['left', 'top', 'width', 'height']) stage.style[key] = rect[key] + 'px';
}
function saveHuddleRect(stage, rect) {
  emit(Number(stage.dataset.huddleGeneration), { type: 'geometry', rect });
  if (!stage.dataset.huddleFixture) {
    try { localStorage.setItem(HUDDLE_GEOMETRY_KEY, JSON.stringify(rect)); } catch (_) {}
  }
}
// Same packing contract as multiSlack: preserve each stream's shape and
// choose the largest common height that fits wrapped rows.
function fitHuddleTiles(ratios, width, height, gap = 8) {
  if (!ratios.length || width <= 0 || height <= 0) return 0;
  const fits = h => {
    let rows = 1, used = 0;
    for (const ratio of ratios) {
      const w = Math.floor(h * ratio);
      if (w > width) return false;
      const next = used ? used + gap + w : w;
      if (next > width) { rows++; used = w; } else used = next;
    }
    return rows * h + (rows - 1) * gap <= height;
  };
  let low = 0, high = Math.floor(height) + 1;
  while (high - low > 1) { const middle = Math.floor((low + high) / 2); if (fits(middle)) low = middle; else high = middle; }
  return low;
}
const huddleStageLayouts = new Map();
function layoutHuddleStage(stage) {
  const grid = stage.querySelector('.huddle-stage-grid');
  if (!grid) return;
  const tiles = [...grid.querySelectorAll('.huddle-stage-tile')];
  const ratios = tiles.map(tile => {
    const video = tile.querySelector('video');
    return video ? (video.videoWidth && video.videoHeight ? video.videoWidth / video.videoHeight : 16 / 9) : 1;
  });
  const height = fitHuddleTiles(ratios, grid.clientWidth - 17, grid.clientHeight - 17);
  tiles.forEach((tile, i) => { tile.style.width = Math.floor(height * ratios[i]) + 'px'; tile.style.height = height + 'px'; });
}
function cleanupStageLayouts() {
  for (const [stage, observers] of huddleStageLayouts) {
    if (!stage.isConnected) { observers.resize.disconnect(); observers.dom.disconnect(); huddleStageLayouts.delete(stage); }
  }
}
function initializeHuddleStage(stage) {
  cleanupStageLayouts();
  if (stage.dataset.huddleInitialized) { layoutHuddleStage(stage); return; }
  stage.dataset.huddleInitialized = 'true';
  emit(Number(stage.dataset.huddleGeneration), { type: 'capabilities',
    screen_supported: typeof navigator.mediaDevices?.getDisplayMedia === 'function',
    output_supported: typeof HTMLMediaElement.prototype.setSinkId === 'function',
  });
  const observer = new ResizeObserver(() => layoutHuddleStage(stage));
  observer.observe(stage);
  const domObserver = new MutationObserver(() => { initializeDemoVideos(stage); layoutHuddleStage(stage); });
  domObserver.observe(stage, { childList: true, subtree: true });
  huddleStageLayouts.set(stage, { resize: observer, dom: domObserver });
  stage.addEventListener('loadedmetadata', () => layoutHuddleStage(stage), true);
  stage.addEventListener('resize', () => layoutHuddleStage(stage), true);
  let saved = null;
  if (!stage.dataset.huddleFixture) {
    try { saved = JSON.parse(localStorage.getItem(HUDDLE_GEOMETRY_KEY)); } catch (_) {}
  }
  const rect = clampHuddleRect(saved || { left: parseFloat(stage.style.left), top: parseFloat(stage.style.top), width: parseFloat(stage.style.width), height: parseFloat(stage.style.height) }, window.innerWidth, window.innerHeight);
  applyHuddleRect(stage, rect);
  saveHuddleRect(stage, rect);
  initializeDemoVideos(stage);

}
function initializeDemoVideos(stage) {
  // Deterministic previews exercise native video layout without camera access,
  // Slack credentials, or a network session.
  for (const video of stage.querySelectorAll('video[data-huddle-demo]')) {
    if (video.dataset.demoReady) continue;
    video.dataset.demoReady = 'true';
    const canvas = document.createElement('canvas');
    canvas.width = 640; canvas.height = 360;
    const ctx = canvas.getContext('2d');
    ctx.fillStyle = video.dataset.huddleDemo === 'share' ? '#202934' : '#354151';
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    ctx.fillStyle = '#d9e2ec'; ctx.font = '24px system-ui';
    ctx.fillText(video.dataset.huddleDemo === 'share' ? 'Shared screen · release checklist' : 'Camera preview', 32, 56);
    ctx.fillStyle = '#8ea3ba'; ctx.font = '18px system-ui';
    ctx.fillText('Offline huddle fixture', 32, 92);
    if (video.dataset.huddleDemo === 'share') {
      for (let i = 0; i < 4; i++) { ctx.fillStyle = i === 0 ? '#477db0' : '#485969'; ctx.fillRect(32, 130 + i * 42, 480 - i * 48, 18); }
    }
    video.poster = canvas.toDataURL();
    // Fixtures use a deterministic poster. Native captureStream playback
    // depends on the system WebView and does not validate Chime transmission.

  }
}
document.addEventListener('pointerdown', event => {
  const handle = event.target.closest('[data-huddle-drag], [data-huddle-resize]');
  const stage = handle?.closest('[data-huddle-stage]');
  if (!stage || stage.dataset.huddleExpanded === 'true' || event.button !== 0 || event.target.closest('button')) return;
  event.preventDefault();
  handle.setPointerCapture(event.pointerId);
  const bounds = stage.getBoundingClientRect();
  const start = { left: bounds.left, top: bounds.top, width: bounds.width, height: bounds.height };
  const x = event.clientX, y = event.clientY;
  let rect = start;
  const move = e => {
    const dx = e.clientX - x, dy = e.clientY - y;
    const corner = handle.dataset.huddleResize;
    let next = { ...start };
    if (!corner) { next.left += dx; next.top += dy; }
    else {
      const left = corner.includes('left'), top = corner.includes('top');
      next.width = Math.max(240, start.width + (left ? -dx : dx));
      next.height = Math.max(180, start.height + (top ? -dy : dy));
      if (left) next.left = start.left + start.width - next.width;
      if (top) next.top = start.top + start.height - next.height;
    }
    rect = clampHuddleRect(next, window.innerWidth, window.innerHeight);
    applyHuddleRect(stage, rect);
  };
  const finish = () => {
    handle.removeEventListener('pointermove', move);
    handle.removeEventListener('pointerup', finish);
    handle.removeEventListener('pointercancel', finish);
    saveHuddleRect(stage, rect);
  };
  handle.addEventListener('pointermove', move);
  handle.addEventListener('pointerup', finish);
  handle.addEventListener('pointercancel', finish);
});
window.addEventListener('resize', () => {
  cleanupStageLayouts();
  for (const stage of document.querySelectorAll('[data-huddle-stage]')) {
    if (stage.dataset.huddleExpanded === 'true') continue;
    const bounds = stage.getBoundingClientRect();
    const rect = clampHuddleRect({ left: bounds.left, top: bounds.top, width: bounds.width, height: bounds.height }, window.innerWidth, window.innerHeight);
    applyHuddleRect(stage, rect); saveHuddleRect(stage, rect);
  }
});
window.superPlatinumInitializeHuddleStage = () => {
  cleanupStageLayouts();
  for (const stage of document.querySelectorAll('[data-huddle-stage]')) initializeHuddleStage(stage);
};

function releaseStageLayouts(generation) {
  for (const [stage, observers] of huddleStageLayouts) {
    if (Number(stage.dataset.huddleGeneration) !== generation) continue;
    observers.resize.disconnect(); observers.dom.disconnect(); huddleStageLayouts.delete(stage);
  }
}
