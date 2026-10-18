//! All private audio/journal I/O stays off the renderer. The exact captured
//! owner selects a configured account journal; fixtures never fall back to it.
use super::*;
use protocol::custody::{
    self as durable, Record, Scope,
    store::{Journal, Stored},
};

fn scope(token: &Token) -> Result<Scope, Error> {
    Scope::new(&token.owner.account, &token.owner.team, &token.owner.user)
}
pub(super) fn io_gate(mut state: Signal<ShellState>, token: &Token) -> Arc<tokio::sync::Mutex<()>> {
    state
        .write()
        .profile_edit
        .recording
        .entries
        .entry(token.owner.clone())
        .or_default()
        .io_gate
        .clone()
}
fn allowed(shell: &ShellState, token: &Token) -> bool {
    !cfg!(test)
        && !crate::fixture::is_fixture()
        && shell.profile_edit.fixture.is_none()
        && shell.profile_edit.recording.fixture.is_none()
        && (token.current(shell)
            || shell
                .profile_edit
                .recording
                .entries
                .get(&token.owner)
                .and_then(|e| e.receipt.as_ref())
                .is_some_and(|r| r.token == *token)
            || shell
                .profile_edit
                .recording
                .entries
                .get(&token.owner)
                .is_some_and(|e| queue::owns(e, token)))
}
async fn read(scope: Scope) -> Result<(Option<Stored>, u64), ()> {
    tokio::task::spawn_blocking(move || {
        Journal::open_default(scope.account())
            .and_then(|j| Ok((j.get(&scope)?, j.cooldown(&scope)?)))
            .map_err(|_| ())
    })
    .await
    .map_err(|_| ())?
}
async fn write(scope: Scope, revision: Option<u64>, record: Record) -> Result<Stored, ()> {
    tokio::task::spawn_blocking(move || {
        Journal::open_default(scope.account())
            .and_then(|mut j| j.put(revision, &record))
            .map_err(|_| ())
    })
    .await
    .map_err(|_| ())?
}
/// true means storage handled this open; false asks for the first fresh Header.
pub(super) async fn restore(
    mut state: Signal<ShellState>,
    token: &Token,
    visit: u64,
    serial: u64,
) -> bool {
    let gate = io_gate(state, token);
    let _guard = gate.lock().await;
    if !allowed(&state.peek(), token) {
        fault(&mut state.write(), token);
        return true;
    }
    let Ok(scope) = scope(token) else {
        fault(&mut state.write(), token);
        return true;
    };
    // A failed write retains the new private draft in memory until retried.
    if state
        .peek()
        .profile_edit
        .recording
        .entries
        .get(&token.owner)
        .is_some_and(|e| e.retained.is_some())
    {
        install(&mut state.write(), token, visit, serial);
        return true;
    }
    let result = read(scope).await;
    if !current(&state.peek(), token, visit, serial) {
        return true;
    }
    match result {
        Ok((Some(saved), _)) => {
            {
                let mut shell = state.write();
                let e = shell
                    .profile_edit
                    .recording
                    .entries
                    .entry(token.owner.clone())
                    .or_default();
                e.revision = Some(saved.revision);
                e.retained = Some(saved.record);
                e.storage_error = false;
            }
            install(&mut state.write(), token, visit, serial);
            true
        }
        Ok((None, retry_at)) => {
            let mut shell = state.write();
            let e = shell
                .profile_edit
                .recording
                .entries
                .entry(token.owner.clone())
                .or_default();
            e.retry_at = e.retry_at.max(retry_at.min(i64::MAX as u64) as i64);
            e.storage_error = false;
            if e.retry_at > now() {
                e.message = Some("Slack asked us to wait before checking this recording.".into());
                shell.profile_edit.recording.pane.as_mut().unwrap().loading = false;
                true
            } else {
                false
            }
        }
        Err(()) => {
            fault(&mut state.write(), token);
            true
        }
    }
}
fn fault(shell: &mut ShellState, token: &Token) {
    let e = shell
        .profile_edit
        .recording
        .entries
        .entry(token.owner.clone())
        .or_default();
    e.storage_error = true;
    e.storing = false;
    if let Some(r) = e.receipt.as_mut() {
        r.stage = Stage::Unknown;
    }
    e.message=Some("Your recording is retained, but local recovery storage is unavailable. Retry storage before continuing.".into());
    if let Some(p) = shell
        .profile_edit
        .recording
        .pane
        .as_mut()
        .filter(|p| p.token == *token)
    {
        p.loading = false;
    }
}
pub(super) fn install(shell: &mut ShellState, token: &Token, visit: u64, serial: u64) {
    if !current(shell, token, visit, serial) {
        return;
    }
    let Some(record) = shell
        .profile_edit
        .recording
        .entries
        .get(&token.owner)
        .and_then(|e| e.retained.clone())
    else {
        return;
    };
    let Ok(form) = record.form() else {
        fault(shell, token);
        return;
    };
    let pending = record.pending().map(|p| Receipt {
        id: next_id(),
        token: token.clone(),
        visit,
        serial,
        operation: match p.operation() {
            durable::Operation::Upload => Operation::Upload,
            durable::Operation::Save => Operation::Save,
            durable::Operation::Cleanup => Operation::Cleanup,
        },
        stage: Stage::Unknown,
        form: form.clone(),
        ticket: None,
        file: record.staged_file().map(str::to_owned),
    });
    let audio = record.audio().cloned();
    let e = shell
        .profile_edit
        .recording
        .entries
        .get_mut(&token.owner)
        .unwrap();
    let preserve = e
        .inflight
        .is_some_and(|id| e.receipt.as_ref().is_some_and(|r| r.id == id));
    if !preserve {
        e.receipt = pending;
    }
    e.retry_at = e
        .retry_at
        .max(record.retry_at().min(i64::MAX as u64) as i64);
    if !e.storage_error {
        e.message=Some(if record.pending().is_some(){"A recording operation from your previous session needs checking. It has not been repeated."}else{"Your local recording draft was restored."}.into());
    }
    let p = shell.profile_edit.recording.pane.as_mut().unwrap();
    p.loading = false;
    p.media = None;
    p.form = Some(form);
    p.device = record.device().into();
    p.capture_phase = if record.staged_file().is_some() {
        "uploaded"
    } else if audio.is_some() {
        "preview"
    } else {
        "idle"
    }
    .into();
    if let Some(audio) = audio {
        let id =
            super_platinum_core::MediaAssetId::new(super_platinum_core::MediaAssetKind::Attachment);
        shell
            .media
            .insert(id.clone(), audio.mime(), audio.bytes().to_vec());
        let p = shell.profile_edit.recording.pane.as_mut().unwrap();
        p.media = Some(id);
        p.elapsed = audio.elapsed_ms();
    }
}
pub(super) fn snapshot(shell: &ShellState, token: &Token) -> Result<Record, Error> {
    if let Some(record) = shell
        .profile_edit
        .recording
        .entries
        .get(&token.owner)
        .and_then(|e| e.retained.clone())
    {
        return Ok(record);
    }
    let p = shell
        .profile_edit
        .recording
        .pane
        .as_ref()
        .filter(|p| p.token == *token)
        .ok_or(Error::TransportNotConfigured)?;
    let mut r = Record::capture(
        scope(token)?,
        p.form.as_ref().ok_or(Error::TransportNotConfigured)?,
    )?;
    r.set_device(p.device.clone())?;
    Ok(r)
}
pub(super) async fn persist(mut state: Signal<ShellState>, token: &Token, record: Record) -> bool {
    let gate = io_gate(state, token);
    let _guard = gate.lock().await;
    persist_locked(state, token, record).await
}
pub(super) async fn persist_locked(
    mut state: Signal<ShellState>,
    token: &Token,
    mut record: Record,
) -> bool {
    if !allowed(&state.peek(), token) {
        fault(&mut state.write(), token);
        return false;
    }
    let (revision, job) = {
        let mut shell = state.write();
        let e = shell
            .profile_edit
            .recording
            .entries
            .entry(token.owner.clone())
            .or_default();
        if cancel::merge_intent(e, &mut record).is_err() {
            return false;
        }
        if e.storing {
            return false;
        }
        let job = next_id();
        e.storage_job = job;
        e.storing = true;
        e.retained = Some(record.clone());
        (e.revision, job)
    };
    let result = write(record.scope().clone(), revision, record).await;
    let mut shell = state.write();
    let e = shell
        .profile_edit
        .recording
        .entries
        .entry(token.owner.clone())
        .or_default();
    if e.storage_job != job {
        return false;
    }
    e.storing = false;
    match result {
        Ok(saved) => {
            e.revision = Some(saved.revision);
            e.retained = Some(saved.record);
            e.storage_error = false;
            token.current(&shell)
        }
        Err(()) => {
            fault(&mut shell, token);
            false
        }
    }
}
pub(super) async fn prepare(
    state: Signal<ShellState>,
    r: &Receipt,
    audio: Option<durable::Audio>,
) -> bool {
    let result = (|| {
        let mut record = snapshot(&state.peek(), &r.token)?;
        if r.operation != Operation::Cleanup {
            record.set_form(&r.form)?;
        }
        if let Some(audio) = audio {
            record.set_audio(audio)?;
        }
        if r.operation == Operation::Cleanup {
            record.prepare_cleanup(now().max(0) as u64)?;
        } else {
            record.begin(
                match r.operation {
                    Operation::Upload => durable::Operation::Upload,
                    Operation::Save => durable::Operation::Save,
                    Operation::Cleanup => durable::Operation::Cleanup,
                },
                now().max(0) as u64,
            )?;
        }
        Ok::<_, Error>(record)
    })();
    match result {
        Ok(record) => persist(state, &r.token, record).await,
        Err(_) => {
            let mut state = state;
            fault(&mut state.write(), &r.token);
            false
        }
    }
}
pub(super) async fn stage(
    state: Signal<ShellState>,
    r: &Receipt,
    next: durable::Stage,
    file: Option<&str>,
) -> bool {
    let result = (|| {
        let mut record = snapshot(&state.peek(), &r.token)?;
        if let Some(file) = file {
            record.remember_ticket(file)?;
        }
        record.mark(next)?;
        Ok::<_, Error>(record)
    })();
    match result {
        Ok(record) => persist(state, &r.token, record).await,
        Err(_) => {
            let mut state = state;
            fault(&mut state.write(), &r.token);
            false
        }
    }
}
pub(super) async fn failed(
    state: Signal<ShellState>,
    r: &Receipt,
    error: &Error,
    dispatched: bool,
) -> bool {
    let Ok(mut record) = snapshot(&state.peek(), &r.token) else {
        return false;
    };
    if failure_record(
        &mut record,
        error,
        dispatched,
        cancel::requested(&state.peek(), r),
    )
    .is_err()
    {
        return false;
    }
    persist(state, &r.token, record).await
}
pub(super) fn failure_record(
    record: &mut Record,
    error: &Error,
    dispatched: bool,
    cancelled: bool,
) -> Result<(), Error> {
    record.defer(cooldown(error).max(0) as u64);
    if record.pending().is_some() {
        if dispatched {
            record.mark(durable::Stage::Unknown)?;
        } else if cancelled {
            record.request_cancel()?;
        } else {
            record.abort_preparation()?;
        }
    }
    Ok(())
}
pub(super) async fn uploaded(state: Signal<ShellState>, r: &Receipt) -> bool {
    let Some(file) = r.file() else {
        return false;
    };
    let Ok(mut record) = snapshot(&state.peek(), &r.token) else {
        return false;
    };
    if record.confirm_upload(file).is_err() {
        return false;
    }
    persist(state, &r.token, record).await
}
pub(super) async fn saved(
    state: Signal<ShellState>,
    r: &Receipt,
    fresh: &api::OwnProfileSections,
) -> bool {
    let Ok(mut record) = snapshot(&state.peek(), &r.token) else {
        return false;
    };
    if record.confirm_save(fresh).is_err() {
        return false;
    }
    persist(state, &r.token, record).await
}
pub(crate) fn retry(mut state: Signal<ShellState>, visit: u64, serial: u64) {
    let start = {
        let shell = state.peek();
        let Some(p) = shell
            .profile_edit
            .recording
            .pane
            .as_ref()
            .filter(|p| p.visit == visit && p.serial == serial)
        else {
            return;
        };
        let Some(e) = shell
            .profile_edit
            .recording
            .entries
            .get(&p.token.owner)
            .filter(|e| e.storage_error && !e.storing)
        else {
            return;
        };
        (p.token.clone(), e.retained.clone())
    };
    dioxus::core::spawn_forever(async move {
        if let Some(record) = start.1 {
            if persist(state, &start.0, record).await {
                install(&mut state.write(), &start.0, visit, serial);
            }
        } else {
            network::load(state, start.0, visit, serial).await;
        }
    });
}
pub(super) fn save_draft(mut state: Signal<ShellState>, visit: u64, serial: u64) {
    let result = {
        let shell = state.peek();
        let Some(p) = shell
            .profile_edit
            .recording
            .pane
            .as_ref()
            .filter(|p| p.visit == visit && p.serial == serial)
        else {
            return;
        };
        let token = p.token.clone();
        let result = snapshot(&shell, &token).and_then(|mut r| {
            r.set_form(p.form.as_ref().ok_or(Error::TransportNotConfigured)?)?;
            r.set_device(p.device.clone())?;
            Ok(r)
        });
        (token, result)
    };
    if let Ok(record) = result.1 {
        let Some(queued) = queue::reserve(&mut state.write(), &result.0, record) else {
            return;
        };
        state
            .write()
            .profile_edit
            .recording
            .pane
            .as_mut()
            .unwrap()
            .loading = true;
        dioxus::core::spawn_forever(async move {
            if queue::persist(state, queued).await
                && current(&state.peek(), &result.0, visit, serial)
            {
                state
                    .write()
                    .profile_edit
                    .recording
                    .pane
                    .as_mut()
                    .unwrap()
                    .loading = false;
            }
        });
    } else {
        fault(&mut state.write(), &result.0);
    }
}
pub(super) fn preview(
    mut state: Signal<ShellState>,
    visit: u64,
    serial: u64,
    audio: CapturedAudio,
) {
    if !presentation(&state.peek()).is_some_and(|v| {
        v.visit == visit && v.serial == serial && !v.busy && !v.uncertain && v.phase == "retaining"
    }) {
        return;
    }
    if state
        .peek()
        .profile_edit
        .recording
        .pane
        .as_ref()
        .is_none_or(|p| p.capture_generation != audio.generation)
    {
        return;
    }
    let Some((bytes, mime, duration)) = decode_audio(serial, audio) else {
        return;
    };
    let result = {
        let shell = state.peek();
        let Some(p) = shell
            .profile_edit
            .recording
            .pane
            .as_ref()
            .filter(|p| p.visit == visit && p.serial == serial)
        else {
            return;
        };
        let token = p.token.clone();
        let result = snapshot(&shell, &token).and_then(|mut r| {
            r.set_audio(durable::Audio::new(bytes, &mime, duration)?)?;
            Ok(r)
        });
        (token, result)
    };
    let Ok(record) = result.1 else {
        fault(&mut state.write(), &result.0);
        return;
    };
    let Some(queued) = queue::reserve(&mut state.write(), &result.0, record) else {
        return;
    };
    state
        .write()
        .profile_edit
        .recording
        .pane
        .as_mut()
        .unwrap()
        .capture_phase = "saving-preview".into();
    dioxus::core::spawn_forever(async move {
        if queue::persist(state, queued).await && current(&state.peek(), &result.0, visit, serial) {
            state
                .write()
                .profile_edit
                .recording
                .pane
                .as_mut()
                .unwrap()
                .capture_phase = "preview".into();
            dioxus::document::eval(&format!(
                "const r=window.__spNameRecording;if(r?.owner==='{serial}'&&r.root.isConnected){{r.api.retained();}}"
            ));
            upload_retained(state, visit, serial);
        }
    });
}

pub(super) fn keep_current(mut state: Signal<ShellState>, visit: u64, serial: u64) {
    let result = {
        let shell = state.peek();
        if !presentation(&shell)
            .is_some_and(|v| v.visit == visit && v.serial == serial && v.can_keep)
        {
            return;
        }
        let p = shell.profile_edit.recording.pane.as_ref().unwrap();
        let token = p.token.clone();
        let fresh = shell
            .profile_edit
            .recording
            .entries
            .get(&token.owner)
            .and_then(|e| e.checked.clone())
            .unwrap();
        let result = snapshot(&shell, &token).and_then(|mut record| {
            record.keep_current(&fresh)?;
            Ok(record)
        });
        (token, result)
    };
    if let Ok(record) = result.1 {
        let Some(queued) = queue::reserve(&mut state.write(), &result.0, record) else {
            return;
        };
        dioxus::core::spawn_forever(async move {
            if queue::persist(state, queued).await {
                install(&mut state.write(), &result.0, visit, serial);
                network::load_media(state, result.0.clone(), visit, serial).await;
            }
        });
    }
}
pub(super) fn discard_preview(mut state: Signal<ShellState>, visit: u64, serial: u64) {
    let result = {
        let shell = state.peek();
        if !presentation(&shell)
            .is_some_and(|v| v.visit == visit && v.serial == serial && !v.busy && !v.uncertain)
        {
            return;
        }
        let p = shell.profile_edit.recording.pane.as_ref().unwrap();
        let token = p.token.clone();
        let result = snapshot(&shell, &token).and_then(|mut record| {
            record.discard_local_changes()?;
            Ok(record)
        });
        (token, result)
    };
    if let Ok(record) = result.1 {
        let Some(queued) = queue::reserve(&mut state.write(), &result.0, record) else {
            return;
        };
        state
            .write()
            .profile_edit
            .recording
            .pane
            .as_mut()
            .unwrap()
            .loading = true;
        dioxus::core::spawn_forever(async move {
            if queue::persist(state, queued).await {
                install(&mut state.write(), &result.0, visit, serial);
                network::load_media(state, result.0.clone(), visit, serial).await;
            }
        });
    }
}

pub(super) async fn remember_cooldown(mut state: Signal<ShellState>, token: &Token, error: &Error) {
    let until = cooldown(error).max(0) as u64;
    if until == 0 {
        return;
    }
    if !allowed(&state.peek(), token) {
        return;
    }
    let Ok(scope) = scope(token) else {
        return;
    };
    let result = tokio::task::spawn_blocking(move || {
        Journal::open_default(scope.account())
            .and_then(|mut j| j.defer(&scope, until))
            .map_err(|_| ())
    })
    .await;
    let mut shell = state.write();
    let e = shell
        .profile_edit
        .recording
        .entries
        .entry(token.owner.clone())
        .or_default();
    e.retry_at = e.retry_at.max(until.min(i64::MAX as u64) as i64);
    if let Some(record) = e.retained.as_mut() {
        record.defer(until);
    }
    if !matches!(result, Ok(Ok(()))) {
        fault(&mut shell, token);
    }
}

pub(super) fn finish_task(mut state: Signal<ShellState>, owner: &Owner, id: u64) {
    let pane = { finish_task_shell(&mut state.write(), owner, id) };
    if let Some(p) = pane {
        dioxus::core::spawn_forever(network::load_media(state, p.token, p.visit, p.serial));
    }
}
pub(super) fn finish_task_shell(shell: &mut ShellState, owner: &Owner, id: u64) -> Option<Pane> {
    let e = shell
        .profile_edit
        .recording
        .entries
        .get_mut(owner)
        .filter(|e| e.inflight == Some(id))?;
    e.inflight = None;
    if e.retained.as_ref().is_some_and(|r| r.pending().is_none()) {
        e.receipt = None;
    } else if let Some(r) = e.receipt.as_mut() {
        r.stage = Stage::Unknown;
    }
    let pane = shell
        .profile_edit
        .recording
        .pane
        .as_ref()
        .filter(|p| p.token.owner == *owner)
        .cloned();
    if let Some(p) = &pane {
        install(shell, &p.token, p.visit, p.serial);
    }
    pane
}
