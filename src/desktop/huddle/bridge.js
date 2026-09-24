// Huddle media bridge: runs one Amazon Chime SDK meeting session inside the
// main WebView. Rust drives it over the eval channel (`dioxus.recv`) and hears
// back through `dioxus.send`. Every message carries the call generation it
// belongs to, so a late event from a call already left is dropped in Rust.
//
// This function must never return: once it does, Dioxus closes the channel
// and Rust can no longer send commands.
const SDK_URL = '/huddle/chime-sdk.js';
// Chime volumes are 0..1 and quantized; anything above a whisper is speech.
const SPEAKING_VOLUME = 0.08;
// Roster snapshots are coalesced so a room full of talkers is a few IPC
// messages a second, not one per volume tick.
const ROSTER_INTERVAL_MS = 250;
const STOP_TIMEOUT_MS = 3000;

let sdkLoad = null;
let session = null;

function emit(generation, event) {
  dioxus.send(Object.assign({ generation }, event));
}

function describe(error) {
  if (!error) return 'unknown error';
  if (error.name && error.name !== 'Error') return error.name;
  return String(error.message || error);
}

// WKWebView's user agent carries no `Version/… Safari` token, so the SDK
// files macOS under "WKWebView iOS" and takes iOS-only paths. It is the same
// engine as desktop Safari, which is the configuration Chime supports.
function presentAsSafari() {
  const ua = navigator.userAgent;
  if (/Safari\//.test(ua) || !/AppleWebKit\/[0-9.]+ \(KHTML, like Gecko\)$/.test(ua)) return;
  const patched = ua + ' Version/16.0 Safari/605.1.15';
  try {
    Object.defineProperty(navigator, 'userAgent', { get: () => patched, configurable: true });
  } catch (_) {}
}

function loadSdk() {
  if (window.ChimeSDK) return Promise.resolve(window.ChimeSDK);
  if (!sdkLoad) {
    presentAsSafari();
    sdkLoad = new Promise((resolve, reject) => {
      const script = document.createElement('script');
      script.src = SDK_URL;
      script.onload = () => (window.ChimeSDK ? resolve(window.ChimeSDK) : reject(new Error('Chime SDK missing')));
      script.onerror = () => {
        sdkLoad = null;
        script.remove();
        reject(new Error('Chime SDK failed to load'));
      };
      document.head.appendChild(script);
    });
  }
  return sdkLoad;
}

function isCurrent(s) {
  return session === s;
}

function scheduleRoster(s) {
  if (s.rosterTimer) return;
  s.rosterTimer = setTimeout(() => {
    s.rosterTimer = null;
    if (!isCurrent(s)) return;
    const attendees = [];
    for (const row of s.roster.values()) attendees.push([row.external, row.speaking, row.muted]);
    const key = JSON.stringify(attendees);
    if (key === s.lastRoster) return;
    s.lastRoster = key;
    emit(s.generation, { type: 'roster', attendees });
  }, ROSTER_INTERVAL_MS);
}

async function pickMicrophone(av) {
  try {
    const inputs = await av.listAudioInputDevices();
    const preferred = inputs.find((d) => d.deviceId === 'default') || inputs[0];
    if (preferred && preferred.deviceId) return preferred.deviceId;
  } catch (_) {}
  return { echoCancellation: true, noiseSuppression: true, autoGainControl: true };
}

async function join(command) {
  await leave();
  const generation = command.generation;
  let s = null;
  try {
    const sdk = await loadSdk();
    const logger = new sdk.ConsoleLogger('huddle', sdk.LogLevel.WARN);
    const devices = new sdk.DefaultDeviceController(logger, { enableWebAudio: false });
    const configuration = new sdk.MeetingSessionConfiguration(command.meeting, command.attendee);
    const meeting = new sdk.DefaultMeetingSession(configuration, logger, devices);
    const av = meeting.audioVideo;
    // Outside the Dioxus root, so no re-render can ever replace it.
    const audio = document.createElement('audio');
    audio.autoplay = true;
    audio.hidden = true;
    document.body.appendChild(audio);

    s = { generation, av, devices, audio, roster: new Map(), rosterTimer: null, lastRoster: '' };
    session = s;

    av.addObserver({
      audioVideoDidStart: () => isCurrent(s) && emit(generation, { type: 'started' }),
      audioVideoDidStartConnecting: (reconnecting) =>
        isCurrent(s) && emit(generation, { type: 'connecting', reconnecting: !!reconnecting }),
      audioVideoDidStop: (status) => {
        if (s.stopped) s.stopped();
        if (!isCurrent(s)) return;
        const code = status.statusCode();
        release(s);
        emit(generation, { type: 'stopped', status: sdk.MeetingSessionStatusCode[code] || String(code) });
      },
    });

    av.realtimeSubscribeToAttendeeIdPresence((attendeeId, present, externalUserId) => {
      if (!isCurrent(s)) return;
      if (present) {
        if (!s.roster.has(attendeeId)) {
          s.roster.set(attendeeId, { external: externalUserId || '', speaking: false, muted: false });
          av.realtimeSubscribeToVolumeIndicator(attendeeId, (id, volume, muted) => {
            const row = s.roster.get(id);
            if (!row) return;
            if (volume !== null) row.speaking = volume > SPEAKING_VOLUME;
            if (muted !== null) row.muted = !!muted;
            if (row.muted) row.speaking = false;
            scheduleRoster(s);
          });
        }
      } else if (s.roster.delete(attendeeId)) {
        av.realtimeUnsubscribeFromVolumeIndicator(attendeeId);
      }
      scheduleRoster(s);
    });

    av.realtimeSubscribeToMuteAndUnmuteLocalAudio((muted) => {
      if (isCurrent(s)) emit(generation, { type: 'muted', muted: !!muted });
    });

    // Slack's own profile: full-band speech, mono, with audio redundancy.
    av.setAudioProfile(sdk.AudioProfile.fullbandSpeechMono(true));
    await av.bindAudioElement(audio);
    await av.startAudioInput(await pickMicrophone(av));
    if (!isCurrent(s)) return;
    if (command.muted) av.realtimeMuteLocalAudio();
    av.start();
  } catch (error) {
    if (s && isCurrent(s)) {
      session = null;
      await teardown(s);
    }
    emit(generation, { type: 'failed', reason: describe(error) });
  }
}

// Frees the microphone and output element. Safe to call twice.
function release(s) {
  if (s.released) return;
  s.released = true;
  if (session === s) session = null;
  clearTimeout(s.rosterTimer);
  Promise.resolve()
    .then(() => s.av.stopAudioInput())
    .catch(() => {})
    .then(() => {
      try { s.av.unbindAudioElement(); } catch (_) {}
      try { s.devices.destroy(); } catch (_) {}
      s.audio.remove();
    });
}

async function teardown(s) {
  const stopped = new Promise((resolve) => {
    s.stopped = resolve;
    setTimeout(resolve, STOP_TIMEOUT_MS);
  });
  try {
    s.av.realtimeMuteLocalAudio();
    s.av.stop();
  } catch (_) {
    s.stopped();
  }
  await stopped;
  release(s);
}

async function leave() {
  const s = session;
  if (!s) return;
  // Detach first: the stop this causes is ours, not news for Rust.
  session = null;
  await teardown(s);
}

function setMuted(command) {
  const s = session;
  if (!s || s.generation !== command.generation) return;
  if (command.muted) {
    s.av.realtimeMuteLocalAudio();
  } else if (!s.av.realtimeUnmuteLocalAudio()) {
    emit(s.generation, { type: 'muted', muted: true });
  }
}

// The window going away is a leave the user asked for; stop the session so
// the media path closes instead of timing out.
window.addEventListener('pagehide', () => {
  const s = session;
  if (!s) return;
  session = null;
  try { s.av.stop(); } catch (_) {}
});

for (;;) {
  const command = await dioxus.recv();
  try {
    switch (command && command.type) {
      case 'join':
        await join(command);
        break;
      case 'leave':
        // Named by generation: a teardown for an old call must not stop the
        // one that replaced it.
        if (!session || session.generation === command.generation) await leave();
        break;
      case 'mute':
        setMuted(command);
        break;
    }
  } catch (error) {
    if (command && command.generation) emit(command.generation, { type: 'failed', reason: describe(error) });
  }
}
