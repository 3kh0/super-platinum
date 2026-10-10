#!/usr/bin/env node
// Offline regressions for the actual WebView helpers; no SDK, camera or Slack.
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import assert from 'node:assert/strict';
const source = name => readFileSync(new URL(`../src/desktop/huddle/${name}.js`, import.meta.url), 'utf8');
function helpers(overrides = {}) {
  const events = [], listeners = new Map();
  const document = { addEventListener: (type, callback) => listeners.set(type, callback) };
  const window = { addEventListener() {}, innerWidth: 1000, innerHeight: 700 };
  const current = { value: null };
  const capture = overrides.capture || (() => Promise.reject(new Error('Unexpected capture')));
  const api = new Function('document', 'window', 'navigator', 'emit', 'isCurrent', 'startCameraInput', 'describe', 'session',
    source('video') + source('reactions') + source('stage') + `\nreturn { userFromExternal, tileHasVideo, videoLayer, videoPreferences, encodeReaction, decodeReaction, clampHuddleRect, fitHuddleTiles, videoControl, selectDevice };`)(
      document, window, { mediaDevices: { getDisplayMedia: capture } },
      (generation, event) => events.push({ generation, ...event }),
      s => current.value === s, overrides.startCameraInput || (async () => true),
      error => error?.message || String(error), overrides.session || null,
    );
  return { ...api, events, listeners, current };
}
function call() {
  const operations = [];
  return {
    generation: 2, videoBusy: new Set(), cameraOn: false, sharing: false, cameraConstraints: {}, videoCapabilities: {}, deviceIds: {},
    av: {
      startLocalVideoTile: () => operations.push('camera-start'), stopLocalVideoTile: () => operations.push('camera-stop'),
      stopVideoInput: async () => operations.push('video-stop'), stopAudioInput: async () => operations.push('audio-stop'),
      startContentShare: async () => operations.push('share-start'), stopContentShare: () => operations.push('share-stop'),
      listAudioInputDevices: async () => [], listVideoInputDevices: async () => [], listAudioOutputDevices: async () => [],
    },
    audio: {}, operations,
  };
}
test('tiles are discoverable before binding and keep paused streams', () => {
  const h = helpers();
  assert(h.tileHasVideo({ tileId: 1, boundVideoStream: {}, boundAttendeeId: 'a', localTile: false, active: false }));
  assert(!h.tileHasVideo({ tileId: 1, boundVideoStream: null, boundAttendeeId: 'a' }));
  assert.equal(h.userFromExternal('T1-R1-U1#content'), 'U1');
  assert.equal(h.userFromExternal('recorder'), null);
});
test('downlink gives shares priority and requests only the visible camera size', () => {
  const h = helpers();
  assert.deepEqual(h.videoPreferences(['a', 'a', 'b#content'], 180), [
    { id: 'a', priority: 2, size: 0 }, { id: 'b#content', priority: 1, size: 2 },
  ]);
  assert.equal(h.videoLayer(1440), 3);
});
test('Slack reaction wire fields encode a standard reaction and skip unrelated data', () => {
  const h = helpers();
  const bytes = h.encodeReaction('heart', 'a-1');
  assert.deepEqual([...bytes], [10, 9, 10, 5, 104, 101, 97, 114, 116, 16, 2, 34, 3, 97, 45, 49]);
  assert.equal(h.decodeReaction(bytes), 'heart');
  assert.equal(h.decodeReaction(new Uint8Array([34, 3, 97, 45, 49])), null);
  assert.throws(() => h.decodeReaction(new Uint8Array([10, 99])));
  assert.throws(() => h.decodeReaction(new Uint8Array([0])));
});
test('camera failure reports a control error and leaves audio running', async () => {
  const h = helpers({ startCameraInput: async () => { throw Object.assign(new Error('Denied'), { name: 'NotAllowedError' }); } });
  const s = call(); h.current.value = s;
  await h.videoControl(s, 'camera', true);
  assert.equal(s.cameraOn, false);
  assert(!s.operations.includes('audio-stop'));
  assert(h.events.some(event => event.type === 'control-error'));
  assert(h.events.some(event => event.type === 'camera' && !event.pending && !event.enabled));
});
test('leaving while camera setup is pending never publishes the old camera', async () => {
  let resolve;
  const h = helpers({ startCameraInput: () => new Promise(r => { resolve = r; }) });
  const s = call(); h.current.value = s;
  const pending = h.videoControl(s, 'camera', true);
  h.current.value = call(); resolve(true);
  await pending;
  assert(!s.operations.includes('camera-start'));
  assert(s.operations.includes('video-stop'));
  assert(!h.events.some(event => event.type === 'camera' && event.enabled));
});
test('duplicate camera clicks cannot start overlapping operations', async () => {
  let resolve, attempts = 0;
  const h = helpers({ startCameraInput: () => { attempts++; return new Promise(r => { resolve = r; }); } });
  const s = call(); h.current.value = s;
  const pending = h.videoControl(s, 'camera', true);
  await h.videoControl(s, 'camera', true);
  assert.equal(attempts, 1);
  resolve(true); await pending;
  assert.equal(s.cameraOn, true);
});
test('a screen chosen after leaving is stopped instead of shared', async () => {
  let resolve, stopped = false;
  const s = call();
  const h = helpers({ session: s, capture: () => new Promise(r => { resolve = r; }) });
  h.current.value = s;
  const button = { disabled: false, dataset: { huddleGeneration: '2' } };
  h.listeners.get('click')({ target: { closest: () => button } });
  h.current.value = null;
  resolve({ getTracks: () => [{ stop: () => { stopped = true; } }] });
  await new Promise(r => setImmediate(r));
  assert(stopped);
  assert(!s.operations.includes('share-start'));
});
test('cancelled screen picker clears pending state without ending audio', async () => {
  const s = call();
  const h = helpers({ session: s, capture: () => Promise.reject(Object.assign(new Error('Cancelled'), { name: 'NotAllowedError' })) });
  h.current.value = s;
  h.listeners.get('click')({ target: { closest: () => ({ disabled: false, dataset: { huddleGeneration: '2' } }) } });
  await new Promise(r => setImmediate(r));
  assert(!s.videoBusy.has('share'));
  assert(h.events.some(event => event.type === 'share' && !event.pending));
  assert(!s.operations.includes('audio-stop'));
});
test('window geometry survives bad stored values and fits a smaller viewport', () => {
  const h = helpers();
  const rect = h.clampHuddleRect({ left: 5000, top: -200, width: NaN, height: Infinity }, 640, 400);
  assert(rect.left >= 0 && rect.top >= 0 && rect.left + rect.width <= 640 && rect.top + rect.height <= 400);
  assert.equal(h.fitHuddleTiles([16 / 9], 640, 360), 360);
  assert.equal(h.fitHuddleTiles([], 640, 360), 0);
  assert.equal(h.fitHuddleTiles([1, 1], 408, 200), 200);
  assert.equal(h.fitHuddleTiles([1, 1], 200, 408), 200);
});
test('full bridge script parses with the pinned eval channel contract', () => {
  const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
  assert.doesNotThrow(() => new AsyncFunction(['video', 'reactions', 'background', 'stage', 'bridge'].map(source).join('\n')));
});
test('device changes serialize with camera operations', async () => {
  const h = helpers(); const s = call(); h.current.value = s;
  s.videoBusy.add('device');
  await h.videoControl(s, 'camera', true);
  assert(h.events.some(event => event.type === 'camera' && !event.pending));
  s.videoBusy.delete('device'); s.videoBusy.add('camera');
  await h.selectDevice(s, 'camera', 'camera-2');
  assert.equal(s.cameraDevice, undefined);
});
test('unsupported background effects keep the existing camera and release pending state', async () => {
  const events = [];
  const s = call(); s.cameraOn = true; s.background = 'none';
  s.sdk = { BackgroundBlurVideoFrameProcessor: { isSupported: async () => false } };
  const h = new Function('isCurrent', 'emit', 'mediaDeviceMessage', source('background') + '; return { setBackground };')(
    candidate => candidate === s, (generation, event) => events.push(event), (_source, error) => error.message,
  );
  await h.setBackground(s, 'blur');
  assert.equal(s.background, 'none');
  assert(s.cameraOn);
  assert(!s.operations.includes('camera-stop'));
  assert(events.some(event => event.type === 'control-error'));
  assert(events.some(event => event.type === 'device-pending' && !event.pending));
});
test('background processing that finishes after leaving is destroyed', async () => {
  let resolve, destroyed = false, current = true;
  const s = call();
  s.sdk = { BackgroundBlurVideoFrameProcessor: {
    isSupported: async () => true,
    create: () => new Promise(r => { resolve = r; }),
  } };
  const h = new Function('isCurrent', source('background') + '; return { cameraInput };')(() => current);
  const pending = h.cameraInput(s, 'blur');
  await new Promise(r => setImmediate(r));
  current = false;
  resolve({ destroy: async () => { destroyed = true; } });
  await assert.rejects(pending, /Huddle ended/);
  assert(destroyed);
});
