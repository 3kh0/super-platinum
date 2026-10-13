// One RAF chain serves both moving surfaces; no work is scheduled while idle.
// Record the awaited interval even if a visible stall crosses the 180ms deadline.
const surfaces = new Map([
  ['message-timeline', { name: 'channel', until: 0, previous: null, started: null, frames: [], starts: [] }],
  ['thread-body', { name: 'thread', until: 0, previous: null, started: null, frames: [], starts: [] }],
]);
let scheduled = false;

function report(surface) {
  if (surface.frames.length || surface.starts.length) {
    dioxus.send({ surface: surface.name, frames: surface.frames.splice(0), start_delays: surface.starts.splice(0) });
  }
}

function stop(surface) {
  surface.until = 0;
  surface.previous = null;
  surface.started = null;
  report(surface);
}

function frame(now) {
  scheduled = false;
  if (document.hidden) {
    for (const surface of surfaces.values()) stop(surface);
    return;
  }
  let active = false;
  for (const surface of surfaces.values()) {
    if (surface.started === null && surface.previous === null) continue;
    if (surface.started !== null) {
      surface.starts.push(now - surface.started);
      surface.started = null;
    }
    if (surface.previous !== null) surface.frames.push(now - surface.previous);
    if (now >= surface.until) {
      stop(surface);
    } else {
      surface.previous = now;
      active = true;
      if (surface.frames.length >= 10) report(surface);
    }
  }
  if (active) {
    scheduled = true;
    requestAnimationFrame(frame);
  }
}

document.addEventListener('scroll', event => {
  const surface = surfaces.get(event.target?.id);
  if (!surface || document.hidden) return;
  const now = performance.now();
  if (surface.previous === null && surface.started === null) surface.started = now;
  surface.until = now + 180;
  if (!scheduled) {
    scheduled = true;
    requestAnimationFrame(frame);
  }
}, { capture: true, passive: true });

// A hidden interval must not leak into the next visible callback or scroll.
document.addEventListener('visibilitychange', () => {
  if (document.hidden) for (const surface of surfaces.values()) stop(surface);
});

if (autoScroll) setTimeout(() => {
  const timeline = document.getElementById('message-timeline');
  if (!timeline) return;
  timeline.scrollTo({ top: 0, behavior: 'smooth' });
  setTimeout(() => timeline.scrollTo({ top: timeline.scrollHeight, behavior: 'smooth' }), 700);
}, 600);
