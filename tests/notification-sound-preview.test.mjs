import assert from 'node:assert/strict';
import test from 'node:test';
import vm from 'node:vm';
import fs from 'node:fs';
const source = fs.readFileSync(new URL('../src/desktop/notification/sounds/preview.js', import.meta.url), 'utf8');
function setup() {
  const sounds = [], timers = new Map(), status = {textContent: ''};
  let counter = 0;
  class Audio {
    constructor(src) { this.src = src; this.currentTime = 0; this.duration = 1; sounds.push(this); }
    play() { return new Promise((resolve, reject) => {this.resolve = resolve; this.reject = reject;}); }
    pause() { this.pauses = (this.pauses || 0) + 1; }
    removeAttribute(name) { delete this[name]; }
    load() { this.loads = (this.loads || 0) + 1; }
  }
  const context = {window: {}, document: {getElementById: () => status}, Audio,
    setTimeout(fn) {timers.set(++counter, fn); return counter;}, clearTimeout(id) {timers.delete(id);}};
  vm.runInNewContext(source, context);
  return {api: context.window.__notificationSoundPreview, sounds, timers, status, context};
}
test('explicit preview loads local asset and reports playback and completion', async () => {
  const t = setup(); const pending = t.api.play('b2.mp3'); const a = t.sounds[0];
  assert.equal(a.src, '/notification-sounds/b2.mp3'); assert.equal(t.api.state, 'loading');
  a.resolve(); await pending; assert.equal(t.api.state, 'playing');
  a.currentTime = 1; a.onended(); assert.equal(t.api.state, 'ended');
  assert.equal(t.api.currentTime, 1); assert.equal(t.timers.size, 0);
  assert.equal(t.status.textContent, 'Preview finished.');
});
test('replacing a pending preview cancels its audio and ignores late callbacks', async () => {
  const t = setup(); const first = t.api.play('b2.mp3'), old = t.sounds[0], oldError = old.onerror;
  const second = t.api.play('boop.mp3'), current = t.sounds[1];
  assert.equal(old.pauses, 1); assert.equal(old.src, undefined); assert.equal(old.loads, 1);
  oldError(); old.resolve(); await first; assert.equal(t.api.name, 'boop.mp3'); assert.equal(t.api.state, 'loading');
  current.resolve(); await second; assert.equal(t.api.state, 'playing'); assert.equal(t.timers.size, 1);
});
test('leaving settings stops playback and a late rejection cannot change idle state', async () => {
  const t = setup(); const pending = t.api.play('b2.mp3'), a = t.sounds[0];
  t.api.stop(); a.reject(new Error('aborted')); await pending;
  assert.equal(t.api.state, 'idle'); assert.equal(t.api.name, null); assert.equal(t.timers.size, 0);
  assert.equal(a.src, undefined); assert.equal(t.status.textContent, '');
});
test('playback errors and stalled audio fail visibly with bounded cleanup', async () => {
  const t = setup(); const pending = t.api.play('b2.mp3');
  t.sounds[0].reject(new Error('denied')); await pending;
  assert.equal(t.api.state, 'error'); assert.equal(t.timers.size, 0); assert.match(t.status.textContent, /Couldn't/);
  const later = t.api.play('boop.mp3'); const a = t.sounds[1];
  [...t.timers.values()][0](); assert.equal(t.api.state, 'error'); assert.equal(a.src, undefined);
  a.resolve(); await later; assert.equal(t.api.state, 'error');
});
test('reinstall does not recreate or interrupt active preview', async () => {
  const t = setup(); const pending = t.api.play('b2.mp3');
  vm.runInNewContext(source, t.context); assert.equal(t.context.window.__notificationSoundPreview, t.api);
  assert.equal(t.sounds.length, 1); t.sounds[0].resolve(); await pending;
});
