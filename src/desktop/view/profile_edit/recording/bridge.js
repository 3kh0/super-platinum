// Closed renderer lifetime. The caller owns the exact root node and opaque visit.
// Evaluating this factory never opens a microphone; start is an explicit action.
((root, owner, send, env = window) => {
  const MAX = 2 * 1024 * 1024;
  let finish; const closed = new Promise(resolve => { finish = resolve; });
  let dead = false, generation = 0, stream = null, recorder = null;
  let tick = null, deadline = null, permissionTimer = null, stopTimer = null;
  let chunks = [], size = 0, blob = null, preview = null, elapsed = 0;
  let phase = "idle", error = null, started = 0, keep = false;
  let context = null, source = null, analyser = null, samples = [];
  const live = () => !dead && root.isConnected && root.dataset.recordingOwner === owner;
  const clearTimers = () => {
    for (const t of [tick, deadline, permissionTimer, stopTimer]) if (t !== null) env.clearTimeout(t);
    tick = deadline = permissionTimer = stopTimer = null;
  };
  const stopTracks = s => { for (const t of s?.getTracks() || []) t.stop(); };
  const release = () => {
    clearTimers(); stopTracks(stream); stream = null;
    try { source?.disconnect(); analyser?.disconnect(); context?.close()?.catch(() => {}); } catch (_) {}
    context = source = analyser = null;
    if (recorder?.state === "recording") { try { recorder.stop(); } catch (_) {} }
    recorder = null; chunks = []; size = 0;
  };
  const emit = event => {
    if(!live())return false;
    try{send(event);return true;}catch(_){dispose();return false;}
  };
  const notify = () => emit({ owner, generation, kind:"state", phase, elapsed_ms: Math.round(elapsed), warning: phase === "recording" && elapsed >= 5000, error });
  const devices = async () => {
    try {
      const list=await env.navigator.mediaDevices?.enumerateDevices?.();
      if(live() && list) emit({owner,generation,kind:"devices",phase,elapsed_ms:Math.round(elapsed),error:null,
        devices:list.filter(d=>d.kind==="audioinput"&&d.deviceId&&d.deviceId.length<=1024)
          .slice(0,64).map(d=>({id:d.deviceId,label:(d.label||"Microphone").slice(0,256)}))});
    } catch (_) { /* The recorder still works with the system default. */ }
  };
  const waveform = () => {
    if (!analyser || !live()) return;
    const values=new Uint8Array(analyser.fftSize); analyser.getByteTimeDomainData(values);
    let sum=0;for(const v of values)sum+=((v-128)/128)**2;
    samples.push(Math.min(1,Math.sqrt(sum/values.length)*4));if(samples.length>100)samples.shift();
    const canvas=root.querySelector("[data-recording-waveform]"), ctx=canvas?.getContext("2d");
    if(!ctx)return;
    const width=canvas.width,height=canvas.height;ctx.clearRect(0,0,width,height);
    ctx.fillStyle=env.getComputedStyle(canvas).color;
    for(let i=0;i<samples.length;i++){const h=Math.max(2,samples[i]*(height-4));ctx.fillRect(i*width/100,(height-h)/2,Math.max(1,width/100-1),h);}
  };
  const fail = message => {
    generation++; keep = false; release(); phase = "error"; error = message; notify();
  };
  const forgetPreview = () => {
    if (preview) env.URL.revokeObjectURL(preview);
    preview = null; blob = null;
    const audio = root.querySelector("[data-recording-local]");
    if (audio) { audio.pause(); audio.removeAttribute("src"); audio.load(); }
  };
  const cancel = (publish = true) => {
    generation++; keep = false; release(); forgetPreview(); elapsed = 0; error = null;
    phase = "idle"; if (publish) notify();
  };
  const stop = () => {
    if (!live() || phase !== "recording" || !recorder) return false;
    elapsed = Math.min(10000, env.performance.now() - started);
    phase = "stopping"; keep = true; clearTimers(); if(!notify())return false;
    stopTimer = env.setTimeout(() => fail("The microphone did not finish recording. Try again."), 3000);
    try { recorder.stop(); } catch (_) { fail("The microphone could not finish recording."); }
    stopTracks(stream); stream = null;
    return true;
  };
  const start = async (deviceId = "") => {
    if (!live() || !["idle", "preview", "error"].includes(phase)) return false;
    const mime = ["audio/mp4", "audio/webm;codecs=opus", "audio/webm"].find(t => env.MediaRecorder?.isTypeSupported(t));
    if (!mime || !env.navigator.mediaDevices?.getUserMedia) { fail("Audio recording is unavailable on this device."); return false; }
    if(typeof deviceId!=="string" || deviceId.length>1024)return false;
    cancel(false); samples=[]; const g = ++generation; phase = "permission"; error = null; if(!notify())return false;
    permissionTimer = env.setTimeout(() => fail("Microphone permission is still pending. Check device permissions and try again."), 30000);
    let granted;
    try { granted = await env.navigator.mediaDevices.getUserMedia({audio:{noiseSuppression:true,...(deviceId?{deviceId:{exact:deviceId}}:{})},video:false}); }
    catch (e) {
      if (g === generation && live()) fail(e?.name === "NotAllowedError" ? "Microphone access was denied. Allow it in device settings, then try again." : "The microphone is unavailable. Check your input device.");
      return false;
    }
    if (g !== generation || !live()) { stopTracks(granted); return false; }
    env.clearTimeout(permissionTimer); permissionTimer = null; stream = granted;
    devices();
    try {
      const AudioContext=env.AudioContext||env.webkitAudioContext;
      if(AudioContext){context=new AudioContext();source=context.createMediaStreamSource(stream);analyser=context.createAnalyser();analyser.fftSize=256;source.connect(analyser);context.resume()?.catch(()=>{});}
    } catch (_) { /* Visualization failure must not lose a valid recording. */ }
    try {
      const r = new env.MediaRecorder(stream, {mimeType:mime, audioBitsPerSecond:128000}); recorder = r;
      r.addEventListener("dataavailable", event => {
        if (g !== generation || !live() || !event.data?.size) return;
        size += event.data.size;
        if (size > MAX) { fail("The recording is too large. Record a shorter clip."); return; }
        chunks.push(event.data);
      });
      r.addEventListener("error", () => { if (g === generation && live()) fail("The microphone stopped unexpectedly. Try again."); });
      r.addEventListener("stop", () => {
        if (g !== generation || !live()) return;
        if (!keep || phase !== "stopping") { fail("The microphone stopped unexpectedly. Try again."); return; }
        const captured = new env.Blob(chunks, {type:mime}); release();
        if (!captured.size) { fail("No audio was recorded. Try again."); return; }
        blob = captured; preview = env.URL.createObjectURL(blob); phase = "retaining"; error = null;
        const audio = root.querySelector("[data-recording-local]"); if (audio) audio.src = preview;
        if(notify())capture().then(audio=>{if(audio&&g===generation&&live())emit({owner,generation:g,kind:"audio",phase,elapsed_ms:Math.round(elapsed),error:null,audio});}).catch(()=>fail("The local recording could not be retained. Try again."));
      });
      for (const track of granted.getAudioTracks()) track.addEventListener("ended", () => {
        if (g === generation && live() && phase === "recording") fail("The microphone disconnected. Try again.");
      });
      r.start(100); started = env.performance.now(); phase = "recording"; if(!notify())return false;
      const update = () => {
        if (g !== generation || !live() || phase !== "recording") return;
        elapsed = Math.min(10000, env.performance.now() - started); waveform(); if(!notify())return;
        tick = env.setTimeout(update,100);
      };
      tick = env.setTimeout(update,100); deadline = env.setTimeout(stop,10000);
      return true;
    } catch (_) { fail("The microphone could not start recording."); return false; }
  };
  const capture = async () => {
    if (!live() || !["preview","retaining"].includes(phase) || !blob) return null;
    const g = generation, saved = blob;
    const bytes = new Uint8Array(await saved.arrayBuffer());
    if (g !== generation || !live() || blob !== saved || bytes.length === 0 || bytes.length > MAX) return null;
    let binary = "";
    for (let offset=0; offset<bytes.length; offset+=8192) binary += String.fromCharCode(...bytes.subarray(offset,offset+8192));
    return {owner,generation:g,mime:saved.type,elapsed_ms:Math.round(elapsed),base64:env.btoa(binary)};
  };
  const dispose = () => { if (dead) return; dead = true; generation++; keep = false; release(); forgetPreview(); observer.disconnect(); finish(); };
  const observer = new env.MutationObserver(() => { if (!live()) dispose(); });
  observer.observe(env.document.documentElement,{subtree:true,childList:true,attributes:true,attributeFilter:["data-recording-owner"]});
  const retained=()=>{if(!live()||phase!=="retaining")return false;phase="preview";return true;};
  return {start,stop,cancel,capture,dispose,devices,closed,retained};
})
