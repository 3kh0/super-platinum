import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';

const source = readFileSync(new URL('../src/desktop/scroll_performance.js', import.meta.url), 'utf8');

function harness() {
  let time = 0;
  const frames = [];
  const reports = [];
  const listeners = new Map();
  const document = {
    hidden: false,
    addEventListener(type, handler) { listeners.set(type, handler); },
  };
  vm.runInNewContext(source, {
    autoScroll: false, document,
    performance: { now: () => time },
    requestAnimationFrame: callback => frames.push(callback),
    dioxus: { send: samples => reports.push(samples) },
  });
  return {
    frames, reports, document,
    scroll(id = 'message-timeline') { listeners.get('scroll')({ target: { id } }); },
    step() {
      time += 16;
      const queued = frames.splice(0);
      queued.forEach(callback => callback(time));
    },
  };
}

test('idle and unrelated scrolling schedule no animation work', () => {
  const h = harness();
  assert.equal(h.frames.length, 0);
  h.scroll('thread-body');
  assert.equal(h.frames.length, 0);
});

test('one observer runs per scroll burst, flushes, stops, and restarts', () => {
  const h = harness();
  h.scroll();
  h.scroll();
  assert.equal(h.frames.length, 1);
  for (let n = 0; n < 20; n++) h.step();
  assert.equal(h.frames.length, 0);
  assert.ok(h.reports.flat().length > 0);
  assert.ok(h.reports.flat().every(sample => sample === 16));
  const reports = h.reports.length;
  h.scroll();
  assert.equal(h.frames.length, 1);
  for (let n = 0; n < 20; n++) h.step();
  assert.ok(h.reports.length > reports);
  assert.equal(h.frames.length, 0);
});

test('hidden windows stop sampling and resume on a later visible scroll', () => {
  const h = harness();
  h.scroll();
  h.step();
  h.document.hidden = true;
  h.step();
  assert.equal(h.frames.length, 0);
  h.scroll();
  assert.equal(h.frames.length, 0);
  h.document.hidden = false;
  h.scroll();
  assert.equal(h.frames.length, 1);
});
