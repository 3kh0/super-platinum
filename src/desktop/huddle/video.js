// Media streams stay in the WebView. Only tile metadata and confirmed state
// cross IPC. Helpers are independent of Dioxus and covered by node tests.
function userFromExternal(external) {
  const parts = String(external || '').split('#')[0].split('-');
  return (parts.length === 3 || parts.length === 4) && parts[2] ? parts[2] : null;
}
function tileHasVideo(tile) {
  // `active` requires an already bound element: using it here deadlocks binding.
  return tile.tileId != null && !!tile.boundVideoStream && (tile.localTile || !!tile.boundAttendeeId);
}
function videoLayer(height) {
  return height <= 180 ? 0 : height <= 360 ? 1 : height <= 720 ? 2 : 3;
}
function videoPreferences(senders, height) {
  return [...new Set(senders)].map(id => ({
    id, priority: id.endsWith('#content') ? 1 : 2,
    size: id.endsWith('#content') ? Math.max(2, videoLayer(height)) : videoLayer(height),
  }));
}
function mediaDeviceMessage(source, error) {
  const name = error && error.name;
  if (source === 'share' && ['NotAllowedError', 'AbortError'].includes(name)) return 'Screen sharing was cancelled or blocked. You can try again.';
  if (name === 'NotAllowedError') return `Allow ${source === 'camera' ? 'camera' : 'microphone'} access in System Settings, then try again.`;
  if (name === 'NotFoundError') return `No ${source === 'camera' ? 'camera' : 'microphone'} was found.`;
  if (name === 'InvalidStateError' && source === 'share') return 'Click Share screen in the huddle controls to choose a screen.';
  return `Could not change ${source}: ${describe(error)}`;
}

function configureVideo(s, sdk, configuration, command) {
  s.sdk = sdk;
  s.localUser = command.user;
  s.attendeeId = command.attendee.AttendeeId;
  s.tiles = new Map();
  s.bindings = new Map();
  s.cameraOn = false;
  s.sharing = false;
  s.videoBusy = new Set();
  s.cameraDevice = '';
  s.background = 'none';
  s.deviceIds = { microphone: '', camera: '', speaker: '' };
  s.senders = [];
  s.policy = new sdk.VideoPriorityBasedPolicy(s.logger, sdk.VideoPriorityBasedPolicyConfig.Default);
  configuration.videoDownlinkBandwidthPolicy = s.policy;
  const browser = new sdk.DefaultBrowserBehavior();
  const chromium = browser.hasChromiumWebRTC();
  configuration.enableSimulcastForUnifiedPlanChromiumBasedBrowsers = chromium;
  s.cameraConstraints = { width: { ideal: chromium ? 1280 : 640 }, height: { ideal: chromium ? 720 : 360 }, frameRate: { ideal: 15 } };
  s.videoCapabilities = command.attendee.Capabilities || {};
  s.chromium = chromium;
}

function watchVideo(s) {
  const av = s.av;
  if (!s.chromium) {
    const safari = /AppleWebKit/.test(navigator.userAgent) && !/Chrome/.test(navigator.userAgent);
    const codecs = safari
      ? [s.sdk.VideoCodecCapability.h264ConstrainedBaselineProfile(), s.sdk.VideoCodecCapability.vp8()]
      : [s.sdk.VideoCodecCapability.vp8(), s.sdk.VideoCodecCapability.h264ConstrainedBaselineProfile()];
    av.setVideoCodecSendPreferences(codecs);
    av.setContentShareVideoCodecPreferences(codecs);
  }
  av.addObserver({
    videoTileDidUpdate: tile => {
      if (!isCurrent(s) || tile.tileId == null) return;
      if (!tileHasVideo(tile)) s.tiles.delete(tile.tileId);
      else s.tiles.set(tile.tileId, {
        id: tile.tileId, user: tile.localTile ? s.localUser : userFromExternal(tile.boundExternalUserId),
        local: !!tile.localTile, content: !!tile.isContent,
      });
      reportTiles(s);
    },
    videoTileWasRemoved: id => {
      if (isCurrent(s) && s.tiles.delete(id)) reportTiles(s);
    },
    remoteVideoSourcesDidChange: sources => {
      if (!isCurrent(s)) return;
      s.senders = sources.map(source => source.attendee.attendeeId);
      updateDownlink(s);
    },
  });
  av.addContentShareObserver({ contentShareDidStop: () => {
    if (!isCurrent(s)) return;
    s.sharing = false;
    emit(s.generation, { type: 'share', enabled: false, pending: false });
  } });
  s.domObserver = new MutationObserver(() => {
    cleanupStageLayouts();
    for (const stage of document.querySelectorAll('[data-huddle-stage]')) initializeHuddleStage(stage);
    syncVideoElements(s);
  });
  s.domObserver.observe(document.getElementById('main') || document.body, { childList: true, subtree: true, attributes: true, attributeFilter: ['data-huddle-tile', 'data-huddle-generation'] });
  s.resizeObserver = new ResizeObserver(() => updateDownlink(s));
  const transcript = event => {
    if (!isCurrent(s) || !event.results) return;
    for (const result of event.results) {
      if (!result.alternatives || !result.alternatives[0]) continue;
      const alternative = result.alternatives[0];
      const attendee = alternative.items?.find(item => item.attendee)?.attendee;
      emit(s.generation, { type: 'caption', id: result.resultId, user: userFromExternal(attendee?.externalUserId), text: alternative.transcript, partial: !!result.isPartial });
    }
  };
  s.transcriptObserver = transcript;
  av.transcriptionController?.subscribeToTranscriptEvent(transcript);
  av.addDeviceChangeObserver({
    audioInputsChanged: () => reportDevices(s),
    videoInputsChanged: () => reportDevices(s),
    audioOutputsChanged: () => reportDevices(s),
    audioInputStreamEnded: () => isCurrent(s) && emit(s.generation, { type: 'control-error', reason: 'The microphone disconnected. Choose another microphone in huddle settings.' }),
    videoInputStreamEnded: () => {
      if (!isCurrent(s)) return;
      s.cameraOn = false;
      av.stopLocalVideoTile();
      emit(s.generation, { type: 'camera', enabled: false, pending: false });
    },
  });
  // Slack's peer reactions use a protobuf envelope on "data-channel".
  av.realtimeSubscribeToReceiveDataMessage('data-channel', message => {
    if (!isCurrent(s) || message.throttled) return;
    try {
      const reaction = decodeReaction(message.data);
      if (reaction) emit(s.generation, { type: 'reaction', user: userFromExternal(message.senderExternalUserId), emoji: reaction });
    } catch (_) {}
  });
}

function reportTiles(s) {
  const tiles = [...s.tiles.values()].sort((a, b) => Number(b.content) - Number(a.content) || a.id - b.id);
  const key = JSON.stringify(tiles);
  if (key !== s.lastTiles) {
    s.lastTiles = key;
    emit(s.generation, { type: 'tiles', tiles });
  }
  syncVideoElements(s);
}
function syncVideoElements(s) {
  if (!isCurrent(s)) return;
  const elements = new Map();
  for (const element of document.querySelectorAll('video[data-huddle-tile]')) {
    if (Number(element.dataset.huddleGeneration) !== s.generation) continue;
    const id = Number(element.dataset.huddleTile);
    if (s.tiles.has(id)) elements.set(id, element);
  }
  for (const [id, previous] of s.bindings) {
    if (elements.get(id) === previous) continue;
    s.av.unbindVideoElement(id, false);
    s.resizeObserver.unobserve(previous);
    s.bindings.delete(id);
  }
  for (const [id, element] of elements) {
    if (s.bindings.get(id) === element) continue;
    element.muted = true;
    s.av.bindVideoElement(id, element);
    s.bindings.set(id, element);
    s.resizeObserver.observe(element);
  }
  updateDownlink(s);
}
function updateDownlink(s) {
  if (!isCurrent(s)) return;
  let height = 0;
  for (const element of s.bindings.values()) height = Math.max(height, element.getBoundingClientRect().height * (window.devicePixelRatio || 1));
  // Hide means unsubscribe, including shares; audio continues unchanged.
  const visible = document.querySelector('[data-huddle-stage]');
  const specs = visible ? videoPreferences(s.senders, height) : [];
  const key = JSON.stringify(specs);
  if (key === s.lastPreferences) return;
  s.lastPreferences = key;
  const builder = s.sdk.VideoPreferences.prepare();
  for (const spec of specs) builder.add(new s.sdk.VideoPreference(spec.id, spec.priority, spec.size));
  s.policy.chooseRemoteVideoSources(builder.build());
}

async function reportDevices(s) {
  try {
    const [microphone, camera, speaker] = await Promise.all([s.av.listAudioInputDevices(), s.av.listVideoInputDevices(), s.av.listAudioOutputDevices()]);
    if (!isCurrent(s)) return;
    const rows = (devices, kind) => devices.map((d, i) => ({ id: d.deviceId, label: d.label || `${kind} ${i + 1}` }));
    emit(s.generation, { type: 'devices', microphone: rows(microphone, 'Microphone'), camera: rows(camera, 'Camera'), speaker: rows(speaker, 'Speaker'),
      output_supported: typeof s.audio.setSinkId === 'function', screen_supported: typeof navigator.mediaDevices?.getDisplayMedia === 'function' });
  } catch (_) {}
}

// Per-source serialization prevents double clicks; leave is never queued
// behind a permissions dialog. Re-check ownership after every device await.
async function videoControl(s, source, enabled, stream) {
  if (!isCurrent(s) || s.videoBusy.has(source)) { stream?.getTracks().forEach(t => t.stop()); return; }
  if (source === 'camera' && s.videoBusy.has('device')) {
    emit(s.generation, { type: 'camera', enabled: s.cameraOn, pending: false });
    return;
  }
  s.videoBusy.add(source);
  emit(s.generation, { type: source, enabled: source === 'camera' ? s.cameraOn : s.sharing, pending: true });
  try {
    if (source === 'camera') {
      if (!enabled) {
        s.av.stopLocalVideoTile();
        await s.av.stopVideoInput();
        await s.transform?.stop();
        s.transform = null;
        s.cameraOn = false;
      } else {
        if (['None', 'Receive'].includes(s.videoCapabilities.Video)) throw new Error('This huddle does not allow sending camera video.');
        if (!await startCameraInput(s)) return;
        if (!isCurrent(s)) { await s.av.stopVideoInput(); return; }
        s.av.startLocalVideoTile();
        s.cameraOn = true;
      }
    } else if (!enabled) {
      s.av.stopContentShare();
      s.sharing = false;
    } else {
      if (['None', 'Receive'].includes(s.videoCapabilities.Content)) throw new Error('This huddle does not allow screen sharing.');
      await s.av.startContentShare(stream);
      if (!isCurrent(s)) { s.av.stopContentShare(); stream?.getTracks().forEach(t => t.stop()); return; }
      s.sharing = true;
    }
  } catch (error) {
    stream?.getTracks().forEach(t => t.stop());
    if (isCurrent(s)) emit(s.generation, { type: 'control-error', reason: mediaDeviceMessage(source, error) });
  } finally {
    s.videoBusy.delete(source);
    if (isCurrent(s)) {
      emit(s.generation, { type: source, enabled: source === 'camera' ? s.cameraOn : s.sharing, pending: false });
      void reportDevices(s);
    }
  }
}

async function selectDevice(s, kind, id) {
  if (!isCurrent(s) || s.videoBusy.has('device') || s.videoBusy.has('camera')) return;
  s.videoBusy.add('device');
  emit(s.generation, { type: 'device-pending', pending: true });
  try {
    if (kind === 'microphone') {
      await s.av.startAudioInput(id || { echoCancellation: true, noiseSuppression: true, autoGainControl: true });
      if (!isCurrent(s)) { await s.av.stopAudioInput(); return; }
    } else if (kind === 'speaker') {
      await s.av.chooseAudioOutput(id || null);
    } else if (kind === 'camera') {
      if (s.cameraOn && !await startCameraInput(s, s.background, id)) return;
      if (!isCurrent(s)) { await s.av.stopVideoInput(); return; }
      s.cameraDevice = id;
    } else return;
    if (isCurrent(s)) {
      s.deviceIds[kind] = id;
      emit(s.generation, { type: 'device-selected', kind, id });
    }
  } catch (error) {
    if (isCurrent(s)) emit(s.generation, { type: 'control-error', reason: mediaDeviceMessage(kind, error) });
  } finally {
    s.videoBusy.delete('device');
    if (isCurrent(s)) emit(s.generation, { type: 'device-pending', pending: false });
  }
}

function stopVideo(s) {
  releaseStageLayouts(s.generation);
  s.domObserver?.disconnect();
  s.resizeObserver?.disconnect();
  if (s.transcriptObserver) s.av.transcriptionController?.unsubscribeFromTranscriptEvent(s.transcriptObserver);
  try { s.av.stopContentShare(); } catch (_) {}
  try { s.av.stopLocalVideoTile(); } catch (_) {}
  for (const id of s.bindings?.keys() || []) { try { s.av.unbindVideoElement(id, false); } catch (_) {} }
  s.bindings?.clear();
  return Promise.resolve(s.av.stopVideoInput()).catch(() => {}).then(() => s.transform?.stop()).catch(() => {});
}

// Native DOM listener preserves the click's user activation across screen
// capture. Rust callbacks/IPC cannot initiate getDisplayMedia reliably.
document.addEventListener('click', event => {
  const button = event.target.closest('[data-huddle-share]');
  const s = session;
  if (!button || button.disabled || !s || Number(button.dataset.huddleGeneration) !== s.generation) return;
  if (s.sharing) { void videoControl(s, 'share', false); return; }
  if (s.videoBusy.has('share')) return;
  if (typeof navigator.mediaDevices?.getDisplayMedia !== 'function') {
    emit(s.generation, { type: 'control-error', reason: 'Screen sharing is unavailable in this system WebView.' });
    return;
  }
  // Reserve before the picker opens, so repeated clicks cannot open more pickers.
  s.videoBusy.add('share');
  emit(s.generation, { type: 'share', enabled: false, pending: true });
  const capture = navigator.mediaDevices.getDisplayMedia({ video: { frameRate: { ideal: 15 } }, audio: false });
  capture.then(stream => {
    s.videoBusy.delete('share');
    if (!isCurrent(s)) { stream.getTracks().forEach(t => t.stop()); return; }
    void videoControl(s, 'share', true, stream);
  }).catch(error => {
    s.videoBusy.delete('share');
    if (!isCurrent(s)) return;
    emit(s.generation, { type: 'share', enabled: false, pending: false });
    emit(s.generation, { type: 'control-error', reason: mediaDeviceMessage('share', error) });
  });
});
