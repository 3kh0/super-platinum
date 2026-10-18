use super::*;

pub(super) async fn load(mut state: Signal<ShellState>, token: Token, visit: u64, serial: u64) {
    if state.peek().profile_edit.recording.fixture.is_some() {
        return;
    }
    if storage::restore(state, &token, visit, serial).await {
        load_media(state, token, visit, serial).await;
        return;
    }
    let Some(c) = context(&state.peek(), &token) else {
        finish_load(
            &mut state.write(),
            &token,
            visit,
            serial,
            Err(Error::TransportNotConfigured),
        );
        return;
    };
    let result = api::fetch_own_profile_sections(&c.transport, &c.client, &c.session).await;
    if let Err(error) = &result {
        storage::remember_cooldown(state, &token, error).await;
    }
    finish_load(&mut state.write(), &token, visit, serial, result);
    load_media(state, token, visit, serial).await;
}
pub(super) async fn load_media(
    mut state: Signal<ShellState>,
    token: Token,
    visit: u64,
    serial: u64,
) {
    let file = {
        let shell = state.peek();
        if shell.profile_edit.recording.fixture.is_some()
            || !presentation(&shell).is_some_and(|v| {
                v.visit == visit
                    && v.serial == serial
                    && v.can_record
                    && v.file
                    && v.url.is_none()
                    && v.phase == "idle"
            })
        {
            return;
        }
        shell
            .profile_edit
            .recording
            .pane
            .as_ref()
            .and_then(|p| p.form.as_ref())
            .and_then(|f| f.current_file())
            .map(str::to_owned)
    };
    let Some(file) = file else {
        return;
    };
    let Some(c) = context(&state.peek(), &token) else {
        return;
    };
    let result = read_file(&c, &file).await;
    if current(&state.peek(), &token, visit, serial) {
        match result {
            Ok(raw) => install_media(&mut state.write(), &raw),
            Err(error) => {
                storage::remember_cooldown(state, &token, &error).await;
                preview_error(&mut state.write(), &token, error);
            }
        }
    }
}

pub(super) fn finish_load(
    shell: &mut ShellState,
    token: &Token,
    visit: u64,
    serial: u64,
    result: Result<api::OwnProfileSections, Error>,
) {
    if !current(shell, token, visit, serial) {
        return;
    }
    let result = result.and_then(|s| {
        if s.team_id() != token.owner.team || s.user_id() != token.owner.user {
            return Err(Error::Transport("Wrong recording owner".into()));
        }
        protocol::RecordingDraft::open(&s)
    });
    let p = shell.profile_edit.recording.pane.as_mut().unwrap();
    p.loading = false;
    match result {
        Ok(form) => {
            p.form = Some(form);
            p.message = None;
        }
        Err(e) => {
            p.message = Some(
                "Could not load name recording for this profile. Go back and try again.".into(),
            );
            shell
                .profile_edit
                .recording
                .entries
                .entry(token.owner.clone())
                .or_default()
                .retry_at = cooldown(&e);
        }
    }
}
fn transition(shell: &mut ShellState, r: &Receipt, from: Stage, to: Stage) -> bool {
    let allowed = current(shell, &r.token, r.visit, r.serial) && !competing(shell, &r.token.owner);
    let Some(e) = entry_mut(shell, r) else {
        return false;
    };
    let a = e.receipt.as_mut().unwrap();
    if a.stage != from {
        return false;
    }
    if !allowed {
        a.stage = Stage::Unknown;
        e.message=Some("This recording operation belongs to an earlier profile visit. Check it before continuing.".into());
        return false;
    }
    a.stage = to;
    true
}
pub(super) fn finish_failed(shell: &mut ShellState, r: &Receipt, error: Error, dispatched: bool) {
    let cancelled = cancel::requested(shell, r);
    let Some(e) = entry_mut(shell, r) else {
        return;
    };
    e.retry_at = e.retry_at.max(cooldown(&error));
    if dispatched || cancelled {
        e.receipt.as_mut().unwrap().stage = Stage::Unknown;
        e.message=Some("Slack has not confirmed this recording operation. Check its status before trying another change.".into());
    } else {
        e.receipt = None;
        e.message =
            Some("The recording operation did not finish. Your local draft is retained.".into());
    }
}
async fn fresh(
    state: Signal<ShellState>,
    r: &Receipt,
) -> Result<(Arc<crate::scheduled::Context>, api::OwnProfileSections), Error> {
    if state.peek().profile_edit.recording.fixture.is_some()
        || !current(&state.peek(), &r.token, r.visit, r.serial)
    {
        return Err(Error::TransportNotConfigured);
    }
    let c = context(&state.peek(), &r.token).ok_or(Error::TransportNotConfigured)?;
    let fresh = api::fetch_own_profile_sections(&c.transport, &c.client, &c.session).await?;
    r.form.revalidate(&fresh)?;
    Ok((c, fresh))
}
async fn upload_inner(
    mut state: Signal<ShellState>,
    mut r: Receipt,
    bytes: Vec<u8>,
    mime: String,
    duration: u32,
) {
    let Ok(audio) = protocol::custody::Audio::new(bytes.clone(), &mime, duration) else {
        return;
    };
    if !storage::prepare(state, &r, Some(audio)).await {
        return;
    }
    if cancel::requested(&state.peek(), &r) {
        return;
    }
    let (c, _) = match fresh(state, &r).await {
        Ok(v) => v,
        Err(e) => {
            fail(state, &r, e, false).await;
            return;
        }
    };
    let request = match protocol::ticket_request(&c.client, &c.session, bytes.len()) {
        Ok(r) => r,
        Err(e) => {
            fail(state, &r, e, false).await;
            return;
        }
    };
    if cancel::requested(&state.peek(), &r) {
        return;
    }
    if !storage::stage(state, &r, protocol::custody::Stage::Ticket, None).await
        || cancel::requested(&state.peek(), &r)
        || !transition(&mut state.write(), &r, Stage::Preparing, Stage::Ticket)
    {
        return;
    }
    if let Some(e) = entry_mut(&mut state.write(), &r) {
        e.upload_started = true;
    } else {
        return;
    }
    let result = c
        .transport
        .execute_name_recording(request)
        .await
        .and_then(|v| protocol::UploadTicket::decode(&v));
    let ticket = match result {
        Ok(ticket) => ticket,
        Err(e) => {
            fail(state, &r, e, true).await;
            return;
        }
    };
    r.ticket = Some(ticket.clone());
    if let Some(e) = entry_mut(&mut state.write(), &r) {
        e.receipt.as_mut().unwrap().ticket = Some(ticket.clone());
    } else {
        return;
    }
    if !storage::stage(
        state,
        &r,
        protocol::custody::Stage::Bytes,
        Some(ticket.file()),
    )
    .await
        || cancel::requested(&state.peek(), &r)
        || !transition(&mut state.write(), &r, Stage::Ticket, Stage::Bytes)
    {
        return;
    }
    if let Err(e) = c
        .transport
        .upload_name_recording(&ticket, bytes, &mime)
        .await
    {
        fail(state, &r, e, true).await;
        return;
    }
    if !storage::stage(state, &r, protocol::custody::Stage::Complete, None).await
        || cancel::requested(&state.peek(), &r)
        || !transition(&mut state.write(), &r, Stage::Bytes, Stage::Complete)
    {
        return;
    }
    let result = c
        .transport
        .execute_name_recording(protocol::complete_request(&c.client, &c.session, &ticket))
        .await;
    match result {
        Ok(_) => {
            if cancel::requested(&state.peek(), &r) {
                return;
            }
            if !storage::uploaded(state, &r).await {
                return;
            }
            finish_upload(&mut state.write(), &r);
            storage::finish_task(state, &r.token.owner, r.id);
        }
        Err(e) => {
            fail(state, &r, e, true).await;
            return;
        }
    }
    if current(&state.peek(), &r.token, r.visit, r.serial) {
        let result = read_file(&c, ticket.file()).await;
        if current(&state.peek(), &r.token, r.visit, r.serial) {
            match result {
                Ok(file) => install_media(&mut state.write(), &file),
                Err(error) => {
                    storage::remember_cooldown(state, &r.token, &error).await;
                    preview_error(&mut state.write(), &r.token, error);
                }
            }
        }
    }
}
pub(super) fn finish_upload(shell: &mut ShellState, r: &Receipt) {
    let visible = current(shell, &r.token, r.visit, r.serial);
    let Some(e) = entry_mut(shell, r) else {
        return;
    };
    let Some(file) = e.receipt.as_ref().and_then(|r| r.file()) else {
        return;
    };
    let file = file.to_owned();
    // If the pane retired while the final request was in flight, retain the
    // actual file ID under its owner. Reopen/check never uploads it again.
    if !visible {
        e.receipt.as_mut().unwrap().stage = Stage::Unknown;
        e.message =
            Some("Recording uploaded. Reopen and check it before saving your profile.".into());
        return;
    }
    e.receipt = None;
    e.message = Some("Recording uploaded. Listen to it, then save your profile.".into());
    let p = shell.profile_edit.recording.pane.as_mut().unwrap();
    if let Some(form) = p.form.as_mut() {
        let _ = form.replace(&file);
    }
    p.capture_phase = "uploaded".into();
    p.media = None;
}
async fn save_inner(mut state: Signal<ShellState>, r: Receipt) {
    if !storage::prepare(state, &r, None).await {
        return;
    }
    let (c, fresh) = match fresh(state, &r).await {
        Ok(v) => v,
        Err(e) => {
            fail(state, &r, e, false).await;
            return;
        }
    };
    let request = match r.form.request(&c.client, &c.session, &fresh) {
        Ok(v) => v,
        Err(e) => {
            fail(state, &r, e, false).await;
            return;
        }
    };
    if !storage::stage(state, &r, protocol::custody::Stage::Saving, None).await
        || !transition(&mut state.write(), &r, Stage::Preparing, Stage::Saving)
    {
        return;
    }
    match c.transport.execute_own_profile_sections(request).await {
        Ok(()) => {
            if let Some(e) = entry_mut(&mut state.write(), &r) {
                e.receipt.as_mut().unwrap().stage = Stage::Unknown;
                e.message = Some("Checking your saved recording…".into());
            }
        }
        Err(e) => {
            fail(state, &r, e, true).await;
            return;
        }
    }
    let started = {
        let mut shell = state.write();
        if current(&shell, &r.token, r.visit, r.serial) {
            entry_mut(&mut shell, &r).map(|e| {
                e.checked = None;
                let owned = e.receipt.as_mut().unwrap();
                owned.stage = Stage::Checking;
                owned.clone()
            })
        } else {
            None
        }
    };
    if let Some(r) = started {
        check(state, r).await;
    }
}
pub(super) async fn check(mut state: Signal<ShellState>, r: Receipt) {
    if state.peek().profile_edit.recording.fixture.is_some() {
        return;
    }
    let Some(c) = context(&state.peek(), &r.token) else {
        read_failed(state, &r, Error::TransportNotConfigured).await;
        return;
    };
    let fresh = match api::fetch_own_profile_sections(&c.transport, &c.client, &c.session).await {
        Ok(v) => v,
        Err(e) => {
            read_failed(state, &r, e).await;
            return;
        }
    };
    if !current(&state.peek(), &r.token, r.visit, r.serial) {
        read_failed(state, &r, Error::TransportNotConfigured).await;
        return;
    }
    if r.operation == Operation::Cleanup {
        if let Some(file) = r.file() {
            let result = c
                .transport
                .execute(api::file_info(&c.client, &c.session, file))
                .await
                .and_then(|raw| {
                    protocol::success(&raw)?;
                    Ok(raw)
                });
            let deleted = match result {
                Ok(raw) => {
                    raw["file"]["id"] == file
                        && (raw["file"]["mode"] == "tombstone"
                            || raw["file"]["is_tombstoned"] == true)
                }
                Err(Error::Api(code)) if code == "file_deleted" => true,
                Err(error) => {
                    read_failed(state, &r, error).await;
                    retain_checked(&mut state.write(), &r, &fresh);
                    return;
                }
            };
            if deleted
                && current(&state.peek(), &r.token, r.visit, r.serial)
                && protocol::RecordingDraft::open(&fresh)
                    .is_ok_and(|f| f.current_file() != Some(file))
            {
                let snapshot = storage::snapshot(&state.peek(), &r.token);
                if let Ok(mut record) = snapshot {
                    if record.confirm_cleanup(file).is_ok()
                        && storage::persist(state, &r.token, record).await
                    {
                        storage::install(&mut state.write(), &r.token, r.visit, r.serial);
                    }
                }
                return;
            }
        }
    }
    if r.operation == Operation::Upload {
        if cancel::requested(&state.peek(), &r) {
            finish_check(&mut state.write(), &r, fresh);
            return;
        }
        let file = if let Some(file) = r.file() {
            match read_file(&c, file).await {
                Ok(file) => Some(file),
                Err(error) => {
                    read_failed(state, &r, error).await;
                    if current(&state.peek(), &r.token, r.visit, r.serial)
                        && protocol::RecordingDraft::open(&fresh).is_ok()
                    {
                        if let Some(e) = entry_mut(&mut state.write(), &r) {
                            e.checked = Some(fresh);
                        }
                    }
                    return;
                }
            }
        } else {
            None
        };
        if !current(&state.peek(), &r.token, r.visit, r.serial) {
            read_failed(state, &r, Error::TransportNotConfigured).await;
            return;
        }
        if let Some(file) = file {
            // Keep the original Header preflight. A concurrent profile edit
            // still needs review; finding the upload is not permission to Save.
            if r.form.revalidate(&fresh).is_ok() {
                if !storage::uploaded(state, &r).await {
                    return;
                }
                finish_upload(&mut state.write(), &r);
                install_media(&mut state.write(), &file);
                return;
            }
        }
    }
    if r.operation == Operation::Save
        && r.form.confirms(&fresh)
        && !storage::saved(state, &r, &fresh).await
    {
        return;
    }
    finish_check(&mut state.write(), &r, fresh);
}
pub(super) fn finish_check(shell: &mut ShellState, r: &Receipt, fresh: api::OwnProfileSections) {
    let visible = current(shell, &r.token, r.visit, r.serial);
    let Some(e) = entry_mut(shell, r) else {
        return;
    };
    if e.receipt
        .as_ref()
        .is_none_or(|a| a.stage != Stage::Checking)
    {
        return;
    }
    e.receipt.as_mut().unwrap().stage = Stage::Unknown;
    if !visible || fresh.team_id() != r.token.owner.team || fresh.user_id() != r.token.owner.user {
        return;
    }
    let confirmed = r.operation == Operation::Save && r.form.confirms(&fresh);
    e.checked = Some(fresh.clone());
    if confirmed {
        e.receipt = None;
        e.message = Some("Name recording saved.".into());
        shell.profile_edit.recording.pane.as_mut().unwrap().form =
            protocol::RecordingDraft::open(&fresh).ok();
    } else {
        e.message=Some("The previous operation is still unconfirmed. Check again, or keep the current profile. A private upload may remain in Slack; this does not send or delete it.".into());
    }
}
fn check_error(shell: &mut ShellState, r: &Receipt, error: Error) {
    if let Some(e) = entry_mut(shell, r) {
        e.receipt.as_mut().unwrap().stage = Stage::Unknown;
        e.retry_at = e.retry_at.max(cooldown(&error));
        e.message = Some(
            "Could not check the recording. The previous operation is retained without replay."
                .into(),
        );
    }
}
pub(super) fn retain_checked(shell: &mut ShellState, r: &Receipt, fresh: &api::OwnProfileSections) {
    if current(shell, &r.token, r.visit, r.serial)
        && fresh.team_id() == r.token.owner.team
        && fresh.user_id() == r.token.owner.user
        && protocol::RecordingDraft::open(fresh).is_ok()
    {
        if let Some(e) = entry_mut(shell, r) {
            e.checked = Some(fresh.clone());
        }
    }
}
async fn read_file(c: &crate::scheduled::Context, id: &str) -> Result<Value, Error> {
    let raw = c
        .transport
        .execute(api::file_info(&c.client, &c.session, id))
        .await?;
    protocol::success(&raw)?;
    let f = &raw["file"];
    if f["id"] != id
        || f["user"] != c.session.user_id
        || f["subtype"] != protocol::SUBTYPE
        || f["mode"] != "hosted"
        || !f["size"].as_u64().is_some_and(|n| n > 0)
        || !f["url_private"].as_str().is_some_and(valid_media)
    {
        return Err(Error::Transport(
            "Slack has not confirmed this private recording file.".into(),
        ));
    }
    Ok(f.clone())
}
fn valid_media(raw: &str) -> bool {
    url::Url::parse(raw).is_ok_and(|u| {
        u.scheme() == "https"
            && u.port_or_known_default() == Some(443)
            && u.host_str()
                .is_some_and(|h| h == "files.slack.com" || h.ends_with(".files.slack.com"))
            && u.username().is_empty()
            && u.password().is_none()
            && u.fragment().is_none()
    })
}
fn install_media(shell: &mut ShellState, file: &Value) {
    if !shell.profile_edit.recording.pane.as_ref().is_some_and(|p| {
        matches!(p.capture_phase.as_str(), "idle" | "uploaded")
            && p.form.as_ref().and_then(|f| f.current_file()) == file["id"].as_str()
    }) {
        return;
    }

    let Some(url) = file["url_private"].as_str().filter(|s| valid_media(s)) else {
        return;
    };
    let mime = file["mimetype"].as_str().unwrap_or("audio/mp4");
    if !matches!(mime, "audio/mp4" | "audio/webm" | "audio/webm;codecs=opus") {
        return;
    }
    let id = shell.media.register(
        super_platinum_core::MediaAssetKind::Attachment,
        url,
        mime,
        true,
    );
    if let Some(p) = shell.profile_edit.recording.pane.as_mut() {
        p.media = Some(id);
    }
}

fn preview_error(shell: &mut ShellState, token: &Token, error: Error) {
    let entry = shell
        .profile_edit
        .recording
        .entries
        .entry(token.owner.clone())
        .or_default();
    entry.retry_at = entry.retry_at.max(cooldown(&error));
    entry.message =
        Some("The recording preview is not available yet. Your recording is retained.".into());
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recording_read_error_retains_ticket_and_honors_cooldown() {
        let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
        fixture::install(&mut s);
        let p = s.profile_edit.recording.pane.as_mut().unwrap();
        let (v, n) = (p.visit, p.serial);
        p.capture_phase = "preview".into();
        let r = begin(&mut s, v, n, Operation::Upload).unwrap();
        let e = entry_mut(&mut s, &r).unwrap();
        e.receipt.as_mut().unwrap().stage = Stage::Checking;
        e.receipt.as_mut().unwrap().ticket=Some(protocol::UploadTicket::decode(&serde_json::json!({"ok":true,"file":"F00000002","upload_url":"https://files.slack.com/upload/synthetic"})).unwrap());
        let before = now();
        check_error(
            &mut s,
            &r,
            Error::RateLimited {
                retry_after_secs: Some(120),
            },
        );
        let e = entry_mut(&mut s, &r).unwrap();
        assert!(e.retry_at >= before + 120);
        assert_eq!(
            e.receipt.as_ref().unwrap().ticket.as_ref().unwrap().file(),
            "F00000002"
        );
        assert!(!presentation(&s).unwrap().can_check);
        assert!(!presentation(&s).unwrap().can_record);
        let token = r.token.clone();
        preview_error(
            &mut s,
            &token,
            Error::RateLimited {
                retry_after_secs: Some(180),
            },
        );
        assert!(entry_mut(&mut s, &r).unwrap().retry_at >= before + 180);
    }
}

async fn fail(mut state: Signal<ShellState>, r: &Receipt, error: Error, dispatched: bool) {
    if storage::failed(state, r, &error, dispatched).await {
        finish_failed(&mut state.write(), r, error, dispatched);
    }
}
async fn read_failed(mut state: Signal<ShellState>, r: &Receipt, error: Error) {
    if storage::failed(state, r, &error, true).await {
        check_error(&mut state.write(), r, error);
    }
}
async fn cleanup_inner(mut state: Signal<ShellState>, r: Receipt) {
    if !storage::prepare(state, &r, None).await {
        return;
    }
    let Some(c) = context(&state.peek(), &r.token) else {
        read_failed(state, &r, Error::TransportNotConfigured).await;
        return;
    };
    let fresh = match api::fetch_own_profile_sections(&c.transport, &c.client, &c.session).await {
        Ok(v) => v,
        Err(e) => {
            read_failed(state, &r, e).await;
            return;
        }
    };
    let Some(file) = r.file() else {
        read_failed(state, &r, Error::TransportNotConfigured).await;
        return;
    };
    let info = match c
        .transport
        .execute(api::file_info(&c.client, &c.session, file))
        .await
        .and_then(|raw| {
            protocol::success(&raw)?;
            Ok(raw["file"].clone())
        }) {
        Ok(v) => v,
        Err(e) => {
            read_failed(state, &r, e).await;
            retain_checked(&mut state.write(), &r, &fresh);
            return;
        }
    };
    let request = storage::snapshot(&state.peek(), &r.token)
        .and_then(|record| record.cleanup_request(&c.client, &c.session, &fresh, &info));
    let request = match request {
        Ok(v) => v,
        Err(e) => {
            read_failed(state, &r, e).await;
            retain_checked(&mut state.write(), &r, &fresh);
            return;
        }
    };
    if !storage::stage(state, &r, protocol::custody::Stage::Deleting, None).await
        || !transition(&mut state.write(), &r, Stage::Preparing, Stage::Deleting)
    {
        return;
    }
    match c
        .transport
        .execute(request)
        .await
        .and_then(|v| protocol::success(&v))
    {
        Ok(()) => {
            let Ok(mut record) = storage::snapshot(&state.peek(), &r.token) else {
                return;
            };
            if record.confirm_cleanup(file).is_err() {
                return;
            }
            if storage::persist(state, &r.token, record).await {
                storage::install(&mut state.write(), &r.token, r.visit, r.serial);
            }
        }
        Err(error) => fail(state, &r, error, true).await,
    }
}

pub(super) async fn upload(
    state: Signal<ShellState>,
    r: Receipt,
    bytes: Vec<u8>,
    mime: String,
    duration: u32,
) {
    let owner = r.token.owner.clone();
    let id = r.id;
    upload_inner(state, r.clone(), bytes, mime, duration).await;
    cancel::settled(state, &r).await;
    storage::finish_task(state, &owner, id);
    cancel::cleanup_if_ready(state, &r);
}
pub(super) async fn save(state: Signal<ShellState>, r: Receipt) {
    let owner = r.token.owner.clone();
    let id = r.id;
    save_inner(state, r).await;
    storage::finish_task(state, &owner, id);
}
pub(super) async fn cleanup(state: Signal<ShellState>, r: Receipt) {
    let owner = r.token.owner.clone();
    let id = r.id;
    cleanup_inner(state, r).await;
    storage::finish_task(state, &owner, id);
}
