#!/usr/bin/env node
// Exercise the actual opt-in document probe without a WebView or Slack session.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

const source = readFileSync(new URL('../src/desktop/performance/runtime_probe.js', import.meta.url), 'utf8');
function harness() {
  let now = 0;
  let next = 0;
  const timers = new Map(), frames = new Map(), samples = [];
  class Target {
    listeners = new Map();
    addEventListener(type, callback, options) {
      const list = this.listeners.get(type) ?? [];
      list.push({ callback, capture: options === true || !!options?.capture, once: !!options?.once });
      this.listeners.set(type, list);
    }
    removeEventListener(type, callback, options) {
      const capture = options === true || !!options?.capture;
      this.listeners.set(type, (this.listeners.get(type) ?? []).filter(x => x.callback !== callback || x.capture !== capture));
    }
    emit(type, event = {}) {
      for (const entry of [...(this.listeners.get(type) ?? [])]) {
        entry.callback(event);
        if (entry.once) this.removeEventListener(type, entry.callback, entry.capture);
      }
    }
    count() { return [...this.listeners.values()].reduce((sum, list) => sum + list.length, 0); }
  }
  class ErrorEvent {}
  const window = new Target();
  const document = Object.assign(new Target(), {
    readyState: 'loading', visibilityState: 'visible', hasFocus: () => false,
    getElementById: id => id === 'main' ? { childElementCount: 3 } : null,
  });
  vm.runInNewContext(source, {
    window, document, ErrorEvent, location: { protocol: 'super-platinum:' },
    performance: { now: () => now },
    setTimeout: (callback, delay) => { const id = ++next; timers.set(id, { callback, at: now + delay }); return id; },
    clearTimeout: id => timers.delete(id),
    requestAnimationFrame: callback => { const id = ++next; frames.set(id, callback); return id; },
    cancelAnimationFrame: id => frames.delete(id),
    fetch: (url, options) => { samples.push({ url, options, sample: JSON.parse(options.body) }); return Promise.resolve(); },
  });
  return {
    window, document, samples, ErrorEvent, Target, timers, frames,
    setTime: value => { now = value; },
    runTimer: at => {
      const found = [...timers].find(([, timer]) => timer.at === at);
      assert.ok(found, `timer ${at}`);
      timers.delete(found[0]); now = at; found[1].callback();
    },
    runFrames: () => { for (const [id, callback] of [...frames]) { frames.delete(id); callback(); } },
    stopped: () => { assert.equal(window.count(), 0); assert.equal(document.count(), 0); assert.equal(timers.size, 0); assert.equal(frames.size, 0); },
  };
}
function secretEvent(event = {}) {
  for (const key of ['message', 'filename', 'error', 'reason', 'target', 'stack', 'data', 'origin', 'lastEventId', 'code']) {
    Object.defineProperty(event, key, { get() { throw new Error(`private ${key} was read`); } });
  }
  event.preventDefault = event.stopPropagation = () => { throw new Error('probe interfered with event'); };
  return event;
}
{
  const h = harness();
  assert.equal(h.samples[0].sample.phase, 'initial');
  h.window.emit('error', secretEvent(new h.ErrorEvent()));
  h.window.emit('error', secretEvent());
  h.window.emit('unhandledrejection', secretEvent());
  h.document.readyState = 'complete';
  h.window.interpreter = { headless: true, edits: { readyState: 1 }, queuedBytes: [] };
  h.document.emit('DOMContentLoaded');
  h.window.emit('load'); h.runFrames(); h.runTimer(1000);
  const sample = h.samples.at(-1).sample;
  assert.equal(sample.script_errors, 1); assert.equal(sample.resource_errors, 1);
  assert.equal(sample.unhandled_rejections, 1); assert.equal(sample.raf_callbacks, 2);
  assert.equal(sample.socket_state, 1); assert.equal(sample.root_children, 3);
  for (const { url, options, sample } of h.samples) {
    assert.equal(url, 'super-platinum-probe://runtime/sample');
    assert.equal(options.credentials, 'omit'); assert.equal(options.cache, 'no-store');
    assert.ok(Object.values(sample).every(value => value === null || typeof value !== 'object'));
    assert.ok(!options.body.includes('private'));
  }
  h.runTimer(20000); h.stopped();
  const count = h.samples.length;
  h.window.emit('error', secretEvent()); h.document.emit('visibilitychange');
  assert.equal(h.samples.length, count);
}
// A suspended document can deliver an event before its overdue cleanup timer.
for (const type of ['error', 'unhandledrejection', 'visibilitychange']) {
  const h = harness(); h.setTime(20001);
  (type === 'visibilitychange' ? h.document : h.window).emit(type, secretEvent());
  h.stopped(); assert.equal(h.samples.length, 1);
}
// Frequent visibility changes cannot keep this diagnostic collecting indefinitely.
{
  const h = harness();
  for (let i = 0; i < 30; i++) h.document.emit('visibilitychange');
  assert.equal(h.samples.length, 16); h.stopped();
}
// Socket observers never replace handlers, touch message data, send, or close.
{
  const h = harness();
  const socket = Object.assign(new h.Target(), {readyState:1, bufferedAmount:0});
  Object.defineProperty(socket, 'url', {get(){throw new Error('private socket URL read');}});
  socket.send = socket.close = () => {throw new Error('probe mutated socket');};
  const nativeHandler = () => {};
  socket.onmessage = nativeHandler;
  h.window.interpreter = {headless:true, edits:socket, nodes:new Array(4), stack:new Array(1), queuedBytes:[]};
  socket.emit('message', secretEvent()); // Before attachment: must not be counted.
  h.runTimer(1000);
  assert.equal(socket.count(), 4);
  let sample = h.samples.at(-1).sample;
  assert.equal(sample.socket_observer_since_ms, 1000);
  assert.equal(sample.socket_observer_generation, 1);
  assert.equal(sample.socket_messages, 0);
  assert.equal(sample.interpreter_nodes, 4);
  assert.equal(sample.interpreter_stack, 1);
  for (const type of ['message','open','close','error']) socket.emit(type, secretEvent());
  socket.bufferedAmount = 4000000;
  h.runTimer(2000);
  sample = h.samples.at(-1).sample;
  for (const key of ['socket_messages','socket_opens','socket_closes','socket_errors']) assert.equal(sample[key], 1);
  assert.equal(sample.socket_buffered_bytes, 1000000);
  assert.equal(socket.count(), 4); // Same socket is not attached twice.
  assert.equal(socket.onmessage, nativeHandler);
  const replacement = Object.assign(new h.Target(), {readyState:1, bufferedAmount:0});
  h.window.interpreter.edits = replacement;
  h.runTimer(5000);
  assert.equal(socket.count(), 0); assert.equal(replacement.count(), 4);
  socket.emit('message', secretEvent());
  replacement.emit('message', secretEvent());
  h.runTimer(10000);
  sample = h.samples.at(-1).sample;
  assert.equal(sample.socket_observer_generation, 2);
  assert.equal(sample.socket_observer_since_ms, 5000);
  assert.equal(sample.socket_messages, 1);
  h.setTime(20001); replacement.emit('message', secretEvent());
  assert.equal(replacement.count(), 0); h.stopped();
}
// Flooded socket events only update capped numbers; no per-event reports/timers.
{
  const h = harness(), socket = Object.assign(new h.Target(), {readyState:1});
  h.window.interpreter = {edits:socket};
  h.runTimer(1000);
  const handler = socket.listeners.get('message')[0].callback;
  const before = h.samples.length;
  for (let i=0;i<1000002;i++) handler(secretEvent);
  assert.equal(h.samples.length, before);
  h.runTimer(2000); assert.equal(h.samples.at(-1).sample.socket_messages, 1000000);
  for (let i=0;i<30;i++) h.document.emit('visibilitychange');
  assert.equal(h.samples.length, 16); assert.equal(socket.count(), 0); h.stopped();
}
console.log('runtime probe: payload isolation, passive socket coverage/rebind/caps, structural samples and deadline cleanup passed');
