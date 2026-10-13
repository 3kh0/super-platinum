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
    dioxus: { send: samples => reports.push(JSON.parse(JSON.stringify(samples))) },
  });
  return {
    frames, reports, document,
    scroll(id = 'message-timeline') { listeners.get('scroll')({ target: { id } }); },
    advance(ms) { time += ms; },
    hidden(value) { document.hidden = value; listeners.get('visibilitychange')(); },
    step(ms = 16) {
      time += ms;
      const queued = frames.splice(0);
      queued.forEach(callback => callback(time));
    },
    samples(surface, field = 'frames') {
      return reports.filter(report => report.surface === surface).flatMap(report => report[field]);
    },
  };
}

test('idle and unrelated scrolling schedule no animation work', () => {
  const h = harness();
  assert.equal(h.frames.length, 0);
  h.scroll('sidebar');
  assert.equal(h.frames.length, 0);
});

test('one observer runs per scroll burst, flushes, stops, and restarts without idle time', () => {
  const h = harness();
  h.scroll();
  h.scroll();
  assert.equal(h.frames.length, 1);
  for (let n = 0; n < 20; n++) h.step();
  assert.equal(h.frames.length, 0);
  assert.ok(h.samples('channel').length > 0);
  assert.ok(h.samples('channel').every(sample => sample === 16));
  assert.deepEqual(h.samples('channel', 'start_delays'), [16]);
  const reports = h.reports.length;
  h.advance(10000);
  h.scroll();
  assert.equal(h.frames.length, 1);
  for (let n = 0; n < 20; n++) h.step();
  assert.ok(h.reports.length > reports);
  assert.equal(h.frames.length, 0);
  assert.ok(h.samples('channel').every(sample => sample === 16));
  assert.deepEqual(h.samples('channel', 'start_delays'), [16, 16]);
});

test('visible frame stalled beyond the burst deadline is recorded before stopping', () => {
  const h = harness();
  h.scroll();
  h.step();
  h.step(500);
  assert.equal(h.frames.length, 0);
  assert.deepEqual(h.samples('channel'), [500]);
  assert.deepEqual(h.samples('channel', 'start_delays'), [16]);
});

test('a stall before the first frame is separately reported as scroll start delay', () => {
  const h = harness();
  h.scroll();
  h.step(1800);
  assert.equal(h.frames.length, 0);
  assert.deepEqual(h.samples('channel'), []);
  assert.deepEqual(h.samples('channel', 'start_delays'), [1800]);
});

test('channel and thread share one RAF chain but retain independent interval samples', () => {
  const h = harness();
  h.scroll();
  h.step();
  h.advance(4);
  h.scroll('thread-body');
  assert.equal(h.frames.length, 1);
  h.step();
  h.step(500);
  assert.equal(h.frames.length, 0);
  assert.deepEqual(h.samples('channel'), [20, 500]);
  assert.deepEqual(h.samples('thread'), [500]);
  assert.deepEqual(h.samples('channel', 'start_delays'), [16]);
  assert.deepEqual(h.samples('thread', 'start_delays'), [16]);
});

test('hidden windows stop sampling and resume on a later visible scroll', () => {
  const h = harness();
  h.scroll();
  h.step();
  h.hidden(true);
  h.step(1000);
  assert.equal(h.frames.length, 0);
  h.scroll();
  assert.equal(h.frames.length, 0);
  h.hidden(false);
  h.scroll();
  assert.equal(h.frames.length, 1);
  h.step();
  h.step(500);
  assert.deepEqual(h.samples('channel'), [500]);
  assert.deepEqual(h.samples('channel', 'start_delays'), [16, 16]);
});

test('hide and restore before pending RAF excludes hidden time and requires new scroll', () => {
  const h = harness();
  h.scroll('thread-body');
  h.step();
  h.hidden(true);
  h.advance(1000);
  h.hidden(false);
  h.step();
  assert.equal(h.frames.length, 0);
  assert.deepEqual(h.samples('thread'), []);
  h.scroll('thread-body');
  h.step();
  h.step();
  h.hidden(true);
  assert.deepEqual(h.samples('thread'), [16]);
  assert.deepEqual(h.samples('thread', 'start_delays'), [16, 16]);
});

test('zero-valued RAF timestamp remains a valid previous frame', () => {
  const h = harness();
  h.scroll();
  h.step(0);
  h.step(500);
  assert.deepEqual(h.samples('channel'), [500]);
  assert.deepEqual(h.samples('channel', 'start_delays'), [0]);
});
