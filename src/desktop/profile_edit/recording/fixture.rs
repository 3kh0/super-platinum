//! Entirely synthetic audio/profile state. This marker never permits a device
//! or network fallback, including ordinary screenshot runs without check flags.
use super::*;
use serde_json::json;
#[derive(Clone)]
pub(super) struct Fixture {
    pub raw: Value,
    pub attempts: Vec<Value>,
    pub reads: usize,
}
pub(crate) fn install(shell: &mut ShellState) {
    super::super::fixture::install(shell);
    let visit = shell.profile_edit.draft.as_ref().unwrap().visit;
    let (token, serial) = open_shell(shell, visit).unwrap();
    let raw = json!({"ok":true,"result":{"data":{"user":{"id":token.owner.user,"isSelf":true,"profileSections":[{"sectionId":"H1","type":"HEADER","profileElements":[
        {"elementId":"name","elementKey":"Name Pronunciation","__typename":"ProfileTextElement","text":"Zo-ee","permissions":{"ui":true}},
        {"elementId":"title","elementKey":"title","__typename":"ProfileTextElement","text":"Product engineer","permissions":{"ui":false}},
        {"elementId":"recording","elementKey":"Name Recording","__typename":"ProfileFileElement","fileId":"F00000001","permissions":{"ui":false}}
    ]}]}}}});
    let fresh =
        api::OwnProfileSections::from_response(&super::super::fixture::session(&token.owner), &raw)
            .unwrap();
    network::finish_load(shell, &token, visit, serial, Ok(fresh));
    shell.profile_edit.recording.fixture = Some(Fixture {
        raw,
        attempts: Vec::new(),
        reads: 0,
    });
    synthetic_media(shell);
    shell.profile_edit.recording.pane.as_mut().unwrap().message =
        Some("Synthetic audio preview. This fixture does not use a microphone or Slack.".into());
}
fn allowed(shell: &ShellState, visit: u64, serial: u64) -> bool {
    shell.profile_edit.recording.fixture.is_some()
        && shell.core.session.is_none()
        && shell.core.transport.is_none()
        && presentation(shell).is_some_and(|v| v.visit == visit && v.serial == serial)
}
fn synthetic_media(shell: &mut ShellState) {
    let id =
        super_platinum_core::MediaAssetId::new(super_platinum_core::MediaAssetKind::Attachment);
    shell.media.insert(
        id.clone(),
        "audio/wav",
        include_bytes!("fixture.wav").to_vec(),
    );
    shell.profile_edit.recording.pane.as_mut().unwrap().media = Some(id);
}
pub(crate) fn preview(mut state: Signal<ShellState>, visit: u64, serial: u64) {
    finish_capture_shell(&mut state.write(), visit, serial);
}
pub(super) fn finish_capture_shell(shell: &mut ShellState, visit: u64, serial: u64) {
    if !allowed(shell, visit, serial) || !presentation(shell).is_some_and(|v| v.can_record) {
        return;
    }
    let p = shell.profile_edit.recording.pane.as_mut().unwrap();
    p.capture_phase = "preview".into();
    p.elapsed = 1000;
    synthetic_media(shell);
    upload_shell(shell, visit, serial);
}
pub(crate) fn upload(mut state: Signal<ShellState>, visit: u64, serial: u64) {
    upload_shell(&mut state.write(), visit, serial);
}
pub(super) fn upload_shell(shell: &mut ShellState, visit: u64, serial: u64) {
    if !allowed(shell, visit, serial) {
        return;
    }
    let Some(mut r) = begin(shell, visit, serial, Operation::Upload) else {
        return;
    };
    r.ticket=Some(protocol::UploadTicket::decode(&json!({"ok":true,"file":"F00000002","upload_url":"https://files.slack.com/upload/synthetic"})).unwrap());
    let e = entry_mut(shell, &r).unwrap();
    e.receipt.as_mut().unwrap().ticket = r.ticket.clone();
    e.receipt.as_mut().unwrap().stage = Stage::Complete;
    network::finish_upload(shell, &r);
    let mut retained = retained(shell, false);
    retained
        .set_form(
            shell
                .profile_edit
                .recording
                .pane
                .as_ref()
                .unwrap()
                .form
                .as_ref()
                .unwrap(),
        )
        .unwrap();
    shell
        .profile_edit
        .recording
        .entries
        .get_mut(&r.token.owner)
        .unwrap()
        .retained = Some(retained);
    synthetic_media(shell);
}
pub(crate) fn save(mut state: Signal<ShellState>, visit: u64, serial: u64) {
    save_shell(&mut state.write(), visit, serial);
}
pub(super) fn save_shell(shell: &mut ShellState, visit: u64, serial: u64) {
    if !allowed(shell, visit, serial) {
        return;
    }
    let Some(r) = begin(shell, visit, serial, Operation::Save) else {
        return;
    };
    if let Some(record) = shell
        .profile_edit
        .recording
        .entries
        .get_mut(&r.token.owner)
        .and_then(|e| e.retained.as_mut())
    {
        record.set_form(&r.form).unwrap();
        record
            .begin(protocol::custody::Operation::Save, now().max(0) as u64)
            .unwrap();
        record.mark(protocol::custody::Stage::Saving).unwrap();
    }
    let w = super::super::fixture::session(&r.token.owner);
    let f = shell.profile_edit.recording.fixture.as_mut().unwrap();
    let fresh = api::OwnProfileSections::from_response(&w, &f.raw).unwrap();
    let request = r
        .form
        .request(
            &super_platinum_core::slack::SlackClient::default(),
            &w,
            &fresh,
        )
        .unwrap();
    let super_platinum_core::slack::client::RequestBody::Form(fields) = request.body else {
        panic!("recording fixture form")
    };
    f.attempts.push(json!({"method":"users.profile.setSections","elements":fields.iter().find(|(k,_)|k=="elements").map(|(_,v)|serde_json::from_str::<Value>(v).unwrap()).unwrap()}));
    f.raw["result"]["data"]["user"]["profileSections"][0]["profileElements"][2]["fileId"] =
        r.form.current_file().map_or(Value::Null, |s| json!(s));
    entry_mut(shell, &r)
        .unwrap()
        .receipt
        .as_mut()
        .unwrap()
        .stage = Stage::Unknown;
    check_shell(shell, visit, serial);
}
pub(crate) fn check(mut state: Signal<ShellState>, visit: u64, serial: u64) {
    check_shell(&mut state.write(), visit, serial);
}
pub(super) fn check_shell(shell: &mut ShellState, visit: u64, serial: u64) {
    if !allowed(shell, visit, serial) {
        return;
    }
    let Some(r) = begin_check(shell, visit, serial) else {
        return;
    };
    let w = super::super::fixture::session(&r.token.owner);
    let f = shell.profile_edit.recording.fixture.as_mut().unwrap();
    f.reads += 1;
    let fresh = api::OwnProfileSections::from_response(&w, &f.raw).unwrap();
    if r.operation == Operation::Upload && cancel::requested(shell, &r) {
        network::finish_check(shell, &r, fresh);
        return;
    }
    if r.operation == Operation::Upload
        && r.file().is_some()
        && shell
            .profile_edit
            .recording
            .entries
            .get(&r.token.owner)
            .is_some_and(|e| e.retained.is_some())
    {
        if r.form.revalidate(&fresh).is_ok() {
            let e = entry_mut(shell, &r).unwrap();
            e.retained
                .as_mut()
                .unwrap()
                .confirm_upload(r.file().unwrap())
                .unwrap();
            network::finish_upload(shell, &r);
            synthetic_media(shell);
            return;
        }
    }
    if r.operation == Operation::Save && r.form.confirms(&fresh) {
        if let Some(record) = shell
            .profile_edit
            .recording
            .entries
            .get_mut(&r.token.owner)
            .and_then(|e| e.retained.as_mut())
        {
            record.confirm_save(&fresh).unwrap();
        }
    }
    network::finish_check(shell, &r, fresh);
}

fn retained(shell: &ShellState, uncertain: bool) -> protocol::custody::Record {
    use protocol::custody as d;
    let p = shell.profile_edit.recording.pane.as_ref().unwrap();
    let scope = d::Scope::new(
        &p.token.owner.account,
        &p.token.owner.team,
        &p.token.owner.user,
    )
    .unwrap();
    let mut form = p.form.clone().unwrap();
    // Preserve the original Header, regardless of the pane's staged selection.
    if let Some(original) = form.original_file().map(str::to_owned) {
        form.replace(&original).unwrap();
    } else {
        form.remove();
    }
    let mut record = d::Record::capture(scope, &form).unwrap();
    record.set_device("synthetic-microphone".into()).unwrap();
    record
        .set_audio(
            d::Audio::new(
                b"synthetic compressed fixture audio".to_vec(),
                "audio/webm",
                1000,
            )
            .unwrap(),
        )
        .unwrap();
    record.begin(d::Operation::Upload, 1).unwrap();
    record.mark(d::Stage::Ticket).unwrap();
    record.remember_ticket("F00000002").unwrap();
    record.mark(d::Stage::Bytes).unwrap();
    if uncertain {
        record.recovered()
    } else {
        record.mark(d::Stage::Complete).unwrap();
        record.confirm_upload("F00000002").unwrap();
        record
    }
}
pub(crate) fn install_recovery(shell: &mut ShellState, uncertain: bool) {
    install(shell);
    let p = shell.profile_edit.recording.pane.clone().unwrap();
    let record = retained(shell, uncertain);
    let e = shell
        .profile_edit
        .recording
        .entries
        .entry(p.token.owner.clone())
        .or_default();
    e.retained = Some(record);
    e.revision = Some(7);
    storage::install(shell, &p.token, p.visit, p.serial);
    synthetic_media(shell);
}
pub(crate) fn cleanup(mut state: Signal<ShellState>, visit: u64, serial: u64) {
    cleanup_shell(&mut state.write(), visit, serial);
}
pub(super) fn cleanup_shell(shell: &mut ShellState, visit: u64, serial: u64) {
    use protocol::custody as d;
    if !allowed(shell, visit, serial) {
        return;
    }
    let Some(r) = begin(shell, visit, serial, Operation::Cleanup) else {
        return;
    };
    let w = super::super::fixture::session(&r.token.owner);
    let fresh = api::OwnProfileSections::from_response(
        &w,
        &shell.profile_edit.recording.fixture.as_ref().unwrap().raw,
    )
    .unwrap();
    let mut record = shell
        .profile_edit
        .recording
        .entries
        .get(&r.token.owner)
        .and_then(|e| e.retained.clone())
        .unwrap();
    record.prepare_cleanup(now().max(0) as u64).unwrap();
    let file = record.staged_file().unwrap().to_owned();
    let metadata = json!({"id":file,"user":w.user_id,"subtype":protocol::SUBTYPE,"mode":"hosted","is_public":false,"public_url_shared":false,"shares":{},"channels":[],"groups":[],"ims":[]});
    let request = match record.cleanup_request(
        &super_platinum_core::slack::SlackClient::default(),
        &w,
        &fresh,
        &metadata,
    ) {
        Ok(request) => request,
        Err(_) => {
            record.mark(d::Stage::Unknown).unwrap();
            let e = entry_mut(shell, &r).unwrap();
            e.retained = Some(record);
            e.receipt.as_mut().unwrap().stage = Stage::Unknown;
            network::retain_checked(shell, &r, &fresh);
            return;
        }
    };
    let super_platinum_core::slack::client::RequestBody::Form(fields) = request.body else {
        panic!("cleanup form")
    };
    shell
        .profile_edit
        .recording
        .fixture
        .as_mut()
        .unwrap()
        .attempts
        .push(
            json!({"method":"files.delete","file":fields.iter().find(|(k,_)|k=="file").unwrap().1}),
        );
    record.mark(d::Stage::Deleting).unwrap();
    record.confirm_cleanup(&file).unwrap();
    shell
        .profile_edit
        .recording
        .entries
        .get_mut(&r.token.owner)
        .unwrap()
        .retained = Some(record);
    storage::install(shell, &r.token, visit, serial);
    synthetic_media(shell);
}

pub(crate) fn install_cancellation(shell: &mut ShellState, active: bool) {
    install_recovery(shell, true);
    let p = shell.profile_edit.recording.pane.clone().unwrap();
    let e = shell
        .profile_edit
        .recording
        .entries
        .get_mut(&p.token.owner)
        .unwrap();
    if active {
        let r = e.receipt.as_mut().unwrap();
        r.stage = Stage::Bytes;
        e.inflight = Some(r.id);
        e.upload_started = true;
        e.message =
            Some("Synthetic upload in progress. Cancel uses only local fixture state.".into());
    } else {
        e.retained.as_mut().unwrap().request_cancel().unwrap();
        e.message = Some(
            "Canceled upload recovered. The exact unused private file is retained for cleanup."
                .into(),
        );
    }
}
pub(super) fn cancel_upload_shell(shell: &mut ShellState, visit: u64, serial: u64) {
    if !allowed(shell, visit, serial) {
        return;
    }
    let Some(r) = cancel::request_shell(shell, visit, serial) else {
        return;
    };
    let e = entry_mut(shell, &r).unwrap();
    e.retained.as_mut().unwrap().request_cancel().unwrap();
    storage::finish_task_shell(shell, &r.token.owner, r.id);
    cleanup_shell(shell, visit, serial);
}
