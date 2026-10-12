// Sample only while the timeline is moving. An idle client must not keep a
// requestAnimationFrame chain alive merely to measure a future scroll.
let activeUntil = 0;
let previous = 0;
let scheduled = false;
let samples = [];

function report() {
  if (samples.length) dioxus.send(samples.splice(0));
}

function frame(now) {
  if (now >= activeUntil || document.hidden) {
    scheduled = false;
    previous = 0;
    report();
    return;
  }
  if (previous) samples.push(now - previous);
  previous = now;
  if (samples.length >= 10) report();
  requestAnimationFrame(frame);
}

document.addEventListener('scroll', event => {
  if (event.target?.id !== 'message-timeline') return;
  activeUntil = performance.now() + 180;
  if (!scheduled && !document.hidden) {
    scheduled = true;
    requestAnimationFrame(frame);
  }
}, { capture: true, passive: true });

if (autoScroll) setTimeout(() => {
  const timeline = document.getElementById('message-timeline');
  if (!timeline) return;
  timeline.scrollTo({ top: 0, behavior: 'smooth' });
  setTimeout(() => timeline.scrollTo({ top: timeline.scrollHeight, behavior: 'smooth' }), 700);
}, 600);
