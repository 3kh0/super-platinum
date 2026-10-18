//! Explicit recorder/upload/Header save with owner-bound unresolved receipts.
use super::*;
use crate::media::MediaUri;
use super_platinum_core::slack::api::recording as protocol;
pub(crate) mod cancel;
pub(crate) mod fixture;
mod network;
mod queue;
mod replacement;
pub(crate) use replacement::carry_pending;
pub(crate) mod storage;
#[cfg(test)]
mod tests;

#[derive(Clone, Default)]
pub(crate) struct State {
    pane: Option<Pane>,
    entries: BTreeMap<Owner, Entry>,
    fixture: Option<fixture::Fixture>,
    preferred: BTreeMap<Owner, String>,
}
#[derive(Clone)]
struct Pane {
    token: Token,
    visit: u64,
    serial: u64,
    form: Option<protocol::RecordingDraft>,
    loading: bool,
    capture_phase: String,
    capture_generation: u64,
    device: String,
    devices: Vec<Microphone>,
    elapsed: u32,
    message: Option<String>,
    media: Option<super_platinum_core::MediaAssetId>,
}
#[derive(Clone, Default)]
struct Entry {
    receipt: Option<Receipt>,
    retry_at: i64,
    checked: Option<api::OwnProfileSections>,
    message: Option<String>,
    retained: Option<protocol::custody::Record>,
    revision: Option<u64>,
    storing: bool,
    storage_error: bool,
    storage_job: u64,
    inflight: Option<u64>,
    io_gate: Arc<tokio::sync::Mutex<()>>,
    cancel_requested: Option<String>,
    upload_started: bool,
    queued: Option<(u64, Token)>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Operation {
    Upload,
    Save,
    Cleanup,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Preparing,
    Ticket,
    Bytes,
    Complete,
    Saving,
    Deleting,
    Checking,
    Unknown,
}
#[derive(Clone)]
struct Receipt {
    id: u64,
    token: Token,
    visit: u64,
    serial: u64,
    operation: Operation,
    stage: Stage,
    form: protocol::RecordingDraft,
    ticket: Option<protocol::UploadTicket>,
    file: Option<String>,
}
impl Receipt {
    fn file(&self) -> Option<&str> {
        self.ticket
            .as_ref()
            .map(|t| t.file())
            .or(self.file.as_deref())
    }
}
pub(crate) struct View {
    pub visit: u64,
    pub serial: u64,
    pub phase: String,
    pub device: String,
    pub devices: Vec<Microphone>,
    pub elapsed: u32,
    pub busy: bool,
    pub uncertain: bool,
    pub can_record: bool,
    pub can_save: bool,
    pub can_check: bool,
    pub can_keep: bool,
    pub can_cleanup: bool,
    pub can_cancel_upload: bool,
    pub can_remove: bool,
    pub storage_error: bool,
    pub file: bool,
    pub dirty: bool,
    pub url: Option<String>,
    pub message: Option<String>,
    pub fixture: bool,
}
fn current(shell: &ShellState, token: &Token, visit: u64, serial: u64) -> bool {
    draft_current(shell, token, visit)
        && shell
            .profile_edit
            .recording
            .pane
            .as_ref()
            .is_some_and(|p| p.token == *token && p.visit == visit && p.serial == serial)
}
pub(super) fn active(shell: &ShellState) -> bool {
    shell
        .profile_edit
        .recording
        .pane
        .as_ref()
        .is_some_and(|p| current(shell, &p.token, p.visit, p.serial))
}
pub(crate) fn pending(shell: &ShellState) -> bool {
    Owner::capture(shell)
        .and_then(|o| shell.profile_edit.recording.entries.get(&o))
        .is_some_and(|e| e.receipt.is_some() || e.inflight.is_some() || e.queued.is_some())
}
fn competing(shell: &ShellState, owner: &Owner) -> bool {
    sections::pending(shell)
        || photo::pending(shell)
        || crate::self_status::blocks_profile_edit(shell)
        || crate::profile_title::blocks_profile_writes(shell)
        || shell
            .profile_edit
            .entries
            .get(owner)
            .is_some_and(|e| e.pending.is_some())
}
pub(crate) fn presentation(shell: &ShellState) -> Option<View> {
    let p = shell
        .profile_edit
        .recording
        .pane
        .as_ref()
        .filter(|p| current(shell, &p.token, p.visit, p.serial))?;
    let e = shell.profile_edit.recording.entries.get(&p.token.owner);
    let r = e.and_then(|e| e.receipt.as_ref());
    let busy = p.loading
        || e.is_some_and(|e| e.storing || e.inflight.is_some() || e.queued.is_some())
        || r.is_some_and(|r| r.stage != Stage::Unknown);
    let allowed = !busy
        && e.is_none_or(|e| !e.storage_error)
        && r.is_none()
        && e.is_none_or(|e| e.retry_at <= now())
        && !competing(shell, &p.token.owner)
        && p.form.is_some();
    let capturing = matches!(
        p.capture_phase.as_str(),
        "permission" | "recording" | "stopping" | "retaining"
    );
    Some(View {
        visit: p.visit,
        serial: p.serial,
        phase: p.capture_phase.clone(),
        device: p.device.clone(),
        devices: p.devices.clone(),
        elapsed: p.elapsed,
        busy,
        uncertain: r.is_some(),
        can_record: allowed
            && !capturing
            && p.capture_phase != "saving-preview"
            && e.is_none_or(|e| {
                e.retained
                    .as_ref()
                    .is_none_or(|r| r.staged_file().is_none())
            }),
        can_save: allowed
            && !capturing
            && !matches!(p.capture_phase.as_str(), "preview" | "saving-preview")
            && p.form.as_ref().is_some_and(|f| f.dirty()),
        can_check: !busy
            && e.is_some_and(|e| !e.storage_error && e.retry_at <= now())
            && r.is_some(),
        can_keep: !busy
            && r.is_some()
            && e.is_some_and(|e| !e.storage_error && e.checked.is_some()),
        can_remove: allowed
            && !capturing
            && p.form.as_ref().is_some_and(|f| f.current_file().is_some()),
        can_cancel_upload: e.is_some_and(|e| {
            !e.storage_error
                && !e.storing
                && e.cancel_requested.is_none()
                && e.inflight.is_some()
                && e.retained.as_ref().is_some_and(|r| {
                    !r.cancel_requested()
                        && r.pending()
                            .is_some_and(|p| p.operation() == protocol::custody::Operation::Upload)
                })
        }),
        can_cleanup: (allowed
            || (!busy
                && !competing(shell, &p.token.owner)
                && p.form.is_some()
                && e.is_some_and(|e| {
                    !e.storage_error
                        && e.retry_at <= now()
                        && e.retained.as_ref().is_some_and(|r| {
                            r.cancel_requested()
                                && r.pending().is_some_and(|p| {
                                    p.operation() == protocol::custody::Operation::Upload
                                })
                        })
                })))
            && !capturing
            && p.capture_phase != "saving-preview"
            && e.is_some_and(|e| {
                e.retained
                    .as_ref()
                    .is_some_and(|r| r.staged_file().is_some())
            }),
        storage_error: e.is_some_and(|e| e.storage_error),
        file: p.form.as_ref().is_some_and(|f| f.current_file().is_some()),
        dirty: p.form.as_ref().is_some_and(|f| f.dirty()) || p.capture_phase == "preview",
        url: p
            .media
            .as_ref()
            .map(|id| id.webview_uri_at(shell.media_epoch)),
        message: e
            .and_then(|e| e.message.clone())
            .or_else(|| p.message.clone()),
        fixture: shell.profile_edit.recording.fixture.is_some(),
    })
}
fn open_shell(shell: &mut ShellState, visit: u64) -> Option<(Token, u64)> {
    let token = shell
        .profile_edit
        .draft
        .as_ref()
        .filter(|d| d.visit == visit && draft_current(shell, &d.token, visit))?
        .token
        .clone();
    if sections::active(shell)
        || photo::presentation(shell).is_some()
        || competing(shell, &token.owner)
        || (shell.profile_edit.recording.entries.len() >= 16
            && !shell
                .profile_edit
                .recording
                .entries
                .contains_key(&token.owner))
    {
        return None;
    }
    let entry = shell.profile_edit.recording.entries.get(&token.owner);
    let receipt = entry.and_then(|e| e.receipt.as_ref());
    let cooling = entry.is_some_and(|e| e.retry_at > now());
    let serial = next_id();
    shell.profile_edit.recording.pane = Some(Pane {
        token: token.clone(),
        visit,
        serial,
        form: receipt.map(|r| r.form.clone()),
        loading: receipt.is_none() && !cooling,
        capture_phase: "idle".into(),
        capture_generation: 0,
        device: shell
            .profile_edit
            .recording
            .preferred
            .get(&token.owner)
            .cloned()
            .unwrap_or_default(),
        devices: Vec::new(),
        elapsed: 0,
        message: cooling.then(|| "Slack asked us to wait before trying again.".into()),
        media: None,
    });
    (receipt.is_none() && !cooling).then_some((token, serial))
}
pub(crate) fn open(mut state: Signal<ShellState>, visit: u64) {
    let start = { open_shell(&mut state.write(), visit) };
    if let Some((token, serial)) = start {
        dioxus::core::spawn_forever(network::load(state, token, visit, serial));
    } else {
        let pane = state.peek().profile_edit.recording.pane.clone();
        if let Some(p) = pane {
            storage::install(&mut state.write(), &p.token, p.visit, p.serial);
        }
    }
}
#[derive(Clone, serde::Deserialize)]
pub(crate) struct Microphone {
    pub id: String,
    pub label: String,
}
#[derive(serde::Deserialize)]
pub(crate) struct CaptureEvent {
    owner: String,
    #[serde(default)]
    generation: u64,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    devices: Vec<Microphone>,
    #[serde(default)]
    audio: Option<CapturedAudio>,
    phase: String,
    elapsed_ms: u32,
    error: Option<String>,
}
pub(crate) fn capture_event(
    mut state: Signal<ShellState>,
    visit: u64,
    serial: u64,
    event: CaptureEvent,
) {
    if event.kind == "audio" {
        if let Some(audio) = event.audio {
            storage::preview(state, visit, serial, audio);
        }
        return;
    }
    let mut shell = state.write();
    if !presentation(&shell)
        .is_some_and(|v| v.visit == visit && v.serial == serial && !v.busy && !v.uncertain)
        || event.owner != serial.to_string()
        || !matches!(
            event.phase.as_str(),
            "idle" | "permission" | "recording" | "stopping" | "retaining" | "preview" | "error"
        )
    {
        return;
    }
    let p = shell.profile_edit.recording.pane.as_mut().unwrap();
    if event.generation < p.capture_generation {
        return;
    }
    if event.kind == "state" {
        p.capture_generation = event.generation;
    }
    if event.kind == "devices" {
        p.devices = event
            .devices
            .into_iter()
            .filter(|d| !d.id.is_empty() && d.id.len() <= 1024 && d.label.len() <= 1024)
            .take(64)
            .collect();
        return;
    }
    if !event.kind.is_empty() && event.kind != "state" {
        return;
    }
    p.capture_phase = event.phase;
    p.elapsed = event.elapsed_ms.min(protocol::MAX_DURATION_MS);
    p.message = event.error;
}
pub(crate) fn remove(mut state: Signal<ShellState>, visit: u64, serial: u64) {
    let mut shell = state.write();
    if !presentation(&shell).is_some_and(|v| v.visit == visit && v.serial == serial && v.can_remove)
    {
        return;
    }
    let p = shell.profile_edit.recording.pane.as_mut().unwrap();
    p.form.as_mut().unwrap().remove();
    p.media = None;
    p.message = Some("The recording will be removed from your profile when you save.".into());
    let fixture = shell.profile_edit.recording.fixture.is_some();
    drop(shell);
    if !fixture {
        storage::save_draft(state, visit, serial);
    }
}
fn begin(shell: &mut ShellState, visit: u64, serial: u64, operation: Operation) -> Option<Receipt> {
    let view = presentation(shell)?;
    if view.visit != visit
        || view.serial != serial
        || !match operation {
            Operation::Save => view.can_save,
            Operation::Upload => view.can_record && view.phase == "preview",
            Operation::Cleanup => view.can_cleanup,
        }
    {
        return None;
    }
    let p = shell.profile_edit.recording.pane.as_ref()?;
    let r = Receipt {
        id: next_id(),
        token: p.token.clone(),
        visit,
        serial,
        operation,
        stage: Stage::Preparing,
        form: p.form.clone()?,
        ticket: None,
        file: shell
            .profile_edit
            .recording
            .entries
            .get(&p.token.owner)
            .and_then(|e| e.retained.as_ref())
            .and_then(|r| r.staged_file())
            .map(str::to_owned),
    };
    let live = shell.profile_edit.recording.fixture.is_none();
    let entry = shell
        .profile_edit
        .recording
        .entries
        .entry(r.token.owner.clone())
        .or_default();
    entry.checked = None;
    entry.message = None;
    entry.cancel_requested = None;
    entry.upload_started = false;
    entry.inflight = live.then_some(r.id);
    entry.receipt = Some(r.clone());
    Some(r)
}
fn entry_mut<'a>(shell: &'a mut ShellState, r: &Receipt) -> Option<&'a mut Entry> {
    shell
        .profile_edit
        .recording
        .entries
        .get_mut(&r.token.owner)
        .filter(|e| {
            e.receipt
                .as_ref()
                .is_some_and(|a| a.id == r.id && a.token == r.token)
        })
}
pub(crate) fn save(mut state: Signal<ShellState>, visit: u64, serial: u64) {
    let started = { begin(&mut state.write(), visit, serial, Operation::Save) };
    if let Some(r) = started {
        dioxus::core::spawn_forever(network::save(state, r));
    }
}
#[derive(serde::Deserialize)]
pub(crate) struct CapturedAudio {
    owner: String,
    #[serde(default)]
    generation: u64,
    mime: String,
    elapsed_ms: u32,
    base64: String,
}
fn decode_audio(serial: u64, audio: CapturedAudio) -> Option<(Vec<u8>, String, u32)> {
    use base64::Engine;
    if audio.owner != serial.to_string()
        || audio.elapsed_ms > protocol::MAX_DURATION_MS
        || audio.base64.len() > protocol::MAX_BYTES.div_ceil(3) * 4
        || !matches!(
            audio.mime.as_str(),
            "audio/mp4" | "audio/webm;codecs=opus" | "audio/webm"
        )
    {
        return None;
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(audio.base64)
        .ok()?;
    if bytes.is_empty() || bytes.len() > protocol::MAX_BYTES {
        return None;
    }
    Some((bytes, audio.mime, audio.elapsed_ms))
}
pub(crate) fn upload(mut state: Signal<ShellState>, visit: u64, serial: u64, audio: CapturedAudio) {
    if state
        .peek()
        .profile_edit
        .recording
        .pane
        .as_ref()
        .is_none_or(|p| p.capture_generation != audio.generation || p.capture_phase != "preview")
    {
        return;
    }
    let Some((bytes, mime, duration)) = decode_audio(serial, audio) else {
        return;
    };
    let started = { begin(&mut state.write(), visit, serial, Operation::Upload) };
    if let Some(r) = started {
        dioxus::core::spawn_forever(network::upload(state, r, bytes, mime, duration));
    }
}
pub(crate) fn upload_retained(mut state: Signal<ShellState>, visit: u64, serial: u64) {
    let audio = {
        let shell = state.peek();
        shell
            .profile_edit
            .recording
            .pane
            .as_ref()
            .and_then(|p| shell.profile_edit.recording.entries.get(&p.token.owner))
            .and_then(|e| e.retained.as_ref())
            .and_then(|r| r.audio())
            .cloned()
    };
    let Some(audio) = audio else {
        return;
    };
    let started = { begin(&mut state.write(), visit, serial, Operation::Upload) };
    if let Some(r) = started {
        dioxus::core::spawn_forever(network::upload(
            state,
            r,
            audio.bytes().to_vec(),
            audio.mime().into(),
            audio.elapsed_ms(),
        ));
    }
}
pub(crate) fn cleanup(mut state: Signal<ShellState>, visit: u64, serial: u64) {
    let started = { begin(&mut state.write(), visit, serial, Operation::Cleanup) };
    if let Some(r) = started {
        dioxus::core::spawn_forever(network::cleanup(state, r));
    }
}
pub(crate) fn check(mut state: Signal<ShellState>, visit: u64, serial: u64) {
    let started = { begin_check(&mut state.write(), visit, serial) };
    if let Some(r) = started {
        dioxus::core::spawn_forever(network::check(state, r));
    }
}
fn begin_check(shell: &mut ShellState, visit: u64, serial: u64) -> Option<Receipt> {
    if !presentation(shell).is_some_and(|v| v.visit == visit && v.serial == serial && v.can_check) {
        return None;
    }
    let p = shell.profile_edit.recording.pane.as_ref()?;
    let token = p.token.clone();
    let e = shell.profile_edit.recording.entries.get_mut(&token.owner)?;
    let r = e.receipt.as_mut()?;
    r.token = token;
    r.visit = visit;
    r.serial = serial;
    r.stage = Stage::Checking;
    e.checked = None;
    Some(r.clone())
}
pub(crate) fn keep(mut state: Signal<ShellState>, visit: u64, serial: u64) {
    if state.peek().profile_edit.recording.fixture.is_none() {
        storage::keep_current(state, visit, serial);
        return;
    }
    let mut shell = state.write();
    if !presentation(&shell).is_some_and(|v| v.visit == visit && v.serial == serial && v.can_keep) {
        return;
    }
    let owner = shell
        .profile_edit
        .recording
        .pane
        .as_ref()
        .unwrap()
        .token
        .owner
        .clone();
    let e = shell
        .profile_edit
        .recording
        .entries
        .get_mut(&owner)
        .unwrap();
    let Some(fresh) = e.checked.take() else {
        return;
    };
    if let Some(record) = e.retained.as_mut() {
        let _ = record.keep_current(&fresh);
    }
    e.receipt = None;
    e.message = None;
    let p = shell.profile_edit.recording.pane.as_mut().unwrap();
    p.form = protocol::RecordingDraft::open(&fresh).ok();
    p.message = Some("Kept the current profile recording.".into());
}
pub(crate) fn back(mut state: Signal<ShellState>, visit: u64, serial: u64) {
    let mut shell = state.write();
    if presentation(&shell).is_some_and(|v| v.visit == visit && v.serial == serial && !v.busy) {
        shell.profile_edit.recording.pane = None;
    }
}
pub(super) fn intercept_close(shell: &mut ShellState, visit: Option<u64>) -> bool {
    let Some(p) = shell.profile_edit.recording.pane.as_ref() else {
        return false;
    };
    if visit.is_some_and(|v| v != p.visit) || presentation(shell).is_some_and(|v| v.busy) {
        return true;
    }
    shell.profile_edit.recording.pane = None;
    false
}
pub(super) fn poll(shell: &mut ShellState) -> bool {
    let stale = shell
        .profile_edit
        .recording
        .pane
        .as_ref()
        .is_some_and(|p| !current(shell, &p.token, p.visit, p.serial));
    if stale {
        shell.profile_edit.recording.pane = None;
    }
    let mut changed = stale;
    // Retire read authority, never replay a stage that may have reached Slack.
    let retired: Vec<_> = shell
        .profile_edit
        .recording
        .entries
        .iter()
        .filter(|(_, e)| e.receipt.as_ref().is_some_and(|r| !r.token.current(shell)))
        .map(|(o, _)| o.clone())
        .collect();
    for owner in retired {
        let e = shell
            .profile_edit
            .recording
            .entries
            .get_mut(&owner)
            .unwrap();
        e.checked = None;
        if e.receipt
            .as_ref()
            .is_some_and(|r| r.stage == Stage::Preparing && e.inflight != Some(r.id))
        {
            e.receipt = None;
            changed = true;
        } else if let Some(r) = e.receipt.as_mut().filter(|r| r.stage != Stage::Unknown) {
            r.stage = Stage::Unknown;
            changed = true;
        }
    }
    for e in shell.profile_edit.recording.entries.values_mut() {
        if e.retry_at != 0 && e.retry_at <= now() {
            e.retry_at = 0;
            changed = true;
        }
    }
    let visible = shell
        .profile_edit
        .recording
        .pane
        .as_ref()
        .map(|p| p.token.owner.clone());
    let before = shell.profile_edit.recording.entries.len();
    shell.profile_edit.recording.entries.retain(|owner, e| {
        visible.as_ref() == Some(owner)
            || e.receipt.is_some()
            || e.retry_at > now()
            || e.retained.is_some()
            || e.storage_error
            || e.storing
            || e.inflight.is_some()
    });
    changed || before != shell.profile_edit.recording.entries.len()
}

pub(crate) fn media_failed(mut state: Signal<ShellState>, visit: u64, serial: u64) {
    let mut shell = state.write();
    if presentation(&shell).is_some_and(|v| v.visit == visit && v.serial == serial) {
        shell.profile_edit.recording.pane.as_mut().unwrap().message =
            Some("Audio could not be played. Your recording change is still retained.".into());
    }
}
pub(crate) fn cancel_capture(mut state: Signal<ShellState>, visit: u64, serial: u64) {
    if state.peek().profile_edit.recording.fixture.is_none() {
        storage::discard_preview(state, visit, serial);
        return;
    }
    let mut shell = state.write();
    if presentation(&shell)
        .is_some_and(|v| v.visit == visit && v.serial == serial && !v.busy && !v.uncertain)
    {
        let p = shell.profile_edit.recording.pane.as_mut().unwrap();
        p.capture_phase = "idle".into();
        p.elapsed = 0;
    }
}

pub(crate) fn start_capture(mut state: Signal<ShellState>, visit: u64, serial: u64) -> bool {
    reserve_capture(&mut state.write(), visit, serial)
}
fn reserve_capture(shell: &mut ShellState, visit: u64, serial: u64) -> bool {
    if !presentation(shell).is_some_and(|v| v.visit == visit && v.serial == serial && v.can_record)
    {
        return false;
    }
    let p = shell.profile_edit.recording.pane.as_mut().unwrap();
    p.media = None;
    p.capture_phase = "permission".into();
    p.elapsed = 0;
    p.message = None;
    true
}

pub(crate) fn choose_microphone(
    mut state: Signal<ShellState>,
    visit: u64,
    serial: u64,
    id: String,
) {
    let mut shell = state.write();
    if !presentation(&shell).is_some_and(|v| v.visit == visit && v.serial == serial && v.can_record)
    {
        return;
    }
    let p = shell.profile_edit.recording.pane.as_mut().unwrap();
    if !id.is_empty() && !p.devices.iter().any(|d| d.id == id) {
        return;
    }
    p.device = id.clone();
    let owner = p.token.owner.clone();
    if shell.profile_edit.recording.preferred.len() < 16
        || shell.profile_edit.recording.preferred.contains_key(&owner)
    {
        shell.profile_edit.recording.preferred.insert(owner, id);
    }
    let fixture = shell.profile_edit.recording.fixture.is_some();
    drop(shell);
    if !fixture {
        storage::save_draft(state, visit, serial);
    }
}
