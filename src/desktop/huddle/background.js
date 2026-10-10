// Use Chime's foreground segmentation, never a full-frame CSS blur. Runtime
// model/worker assets load from the SDK's versioned AWS host only on opt-in.
async function cameraInput(s, effect = s.background, device = s.cameraDevice) {
  const input = device || s.cameraConstraints;
  if (effect === 'none') return { input, transform: null };
  const Processor = effect === 'blur' ? s.sdk.BackgroundBlurVideoFrameProcessor : s.sdk.BackgroundReplacementVideoFrameProcessor;
  if (!await Processor.isSupported(undefined, { logger: s.logger })) throw new Error('Camera backgrounds are unavailable in this system WebView.');
  if (!isCurrent(s)) throw new Error('Huddle ended');
  const options = { logger: s.logger, filterCPUUtilization: 30 };
  if (effect !== 'blur') {
    const canvas = document.createElement('canvas');
    canvas.width = 640; canvas.height = 360;
    const context = canvas.getContext('2d');
    context.fillStyle = effect === 'blue' ? '#14324f' : '#252b32';
    context.fillRect(0, 0, canvas.width, canvas.height);
    options.imageBlob = await new Promise(resolve => canvas.toBlob(resolve));
  }
  const processor = await Processor.create(undefined, options);
  if (!isCurrent(s)) { await processor?.destroy(); throw new Error('Huddle ended'); }
  const transform = new s.sdk.DefaultVideoTransformDevice(s.logger, input, [processor]);
  return { input: transform, transform };
}
async function startCameraInput(s, effect = s.background, device = s.cameraDevice) {
  const next = await cameraInput(s, effect, device);
  try {
    if (!isCurrent(s)) { await next.transform?.stop(); return false; }
    await s.av.startVideoInput(next.input);
    if (!isCurrent(s)) { await s.av.stopVideoInput(); await next.transform?.stop(); return false; }
    const previous = s.transform;
    s.transform = next.transform;
    await previous?.stop();
    return true;
  } catch (error) {
    await next.transform?.stop();
    throw error;
  }
}
async function setBackground(s, effect) {
  if (!isCurrent(s) || !['none', 'blur', 'blue', 'gray'].includes(effect) || s.videoBusy.has('camera') || s.videoBusy.has('device')) return;
  s.videoBusy.add('device');
  emit(s.generation, { type: 'device-pending', pending: true });
  try {
    if (s.cameraOn) {
      if (!await startCameraInput(s, effect)) return;
    }
    if (isCurrent(s)) {
      s.background = effect;
      emit(s.generation, { type: 'background', effect });
    }
  } catch (error) {
    if (isCurrent(s)) emit(s.generation, { type: 'control-error', reason: mediaDeviceMessage('background', error) });
  } finally {
    s.videoBusy.delete('device');
    if (isCurrent(s)) emit(s.generation, { type: 'device-pending', pending: false });
  }
}
