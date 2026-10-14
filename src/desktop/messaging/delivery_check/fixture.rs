//! Disconnected read completion seam. It has no upload or HTTP implementation.
use super::*;
use super_platinum_core::{
    domain::{ComposerAttachment, PendingUploadReceipt},
    slack::models::Message,
};
pub(super) const FIRST: &str = "upload-check-one";
pub(super) const SECOND: &str = "upload-check-two";
#[derive(Default)]
pub(crate) struct Fixture {
    pub(super) calls: usize,
    pending: Option<Attempt>,
}
pub(crate) fn enabled(shell: &ShellState) -> bool {
    shell.upload_checks.fixture.is_some()
        && shell.core.session.is_none()
        && shell.core.transport.is_none()
}
pub(crate) fn install(shell: &mut ShellState) {
    shell.core.session = None;
    shell.core.transport = None;
    shell.core.active_account = Some("offline-upload-owner".into());
    shell.overlay = None;
    shell.toast = None;
    shell.upload_checks = State::default();
    shell.upload_checks.fixture = Some(Fixture::default());
    shell.core.pending_file_messages.clear();
    shell.pending_delivery.clear();
    let team = shell.core.active_team.clone().unwrap();
    let channel = shell.core.active_channel.clone().unwrap();
    let user = shell.core.workspaces[&team].self_user_id.clone();
    let bag = shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .messages
        .entry(channel.clone())
        .or_default();
    *bag = Default::default();
    bag.loaded = true;
    for (index, client, name) in [
        (1, FIRST, "Delivery check.txt"),
        (2, SECOND, "Still unconfirmed.txt"),
    ] {
        let ts = format!("1999999999.00000{index}");
        let message = Message {
            ts: Some(ts.clone()),
            channel: Some(channel.clone()),
            user: Some(user.clone()),
            client_msg_id: Some(client.into()),
            text: Some(
                if index == 1 {
                    "Check an earlier upload without sending it again."
                } else {
                    "Another upload stays unconfirmed."
                }
                .into(),
            ),
            ..Default::default()
        };
        bag.upsert(message);
        bag.pending.push(ts.clone());
        shell.core.pending_file_messages.push(PendingFileMessage {
            finalized: Some(PendingUploadReceipt {
                account: shell.core.active_account.clone(),
                sender: user.clone(),
                file_ids: vec![format!("F_CHECK_{index}")],
            }),
            team: team.clone(),
            channel: channel.clone(),
            thread_ts: None,
            reply_broadcast: false,
            message_ts: ts.clone(),
            client_msg_id: client.into(),
            text: "Retained upload caption".into(),
            attachments: vec![ComposerAttachment {
                id: index,
                path: name.into(),
                name: name.into(),
                bytes: 128,
                uploading: false,
                upload_started: None,
                upload_cancel: None,
                upload_progress: None,
                preview_path: None,
            }],
        });
        shell
            .pending_delivery
            .insert(ts, crate::state::PendingDeliveryStatus::Unconfirmed);
    }
    shell.refresh_from_core();
    shell.stick_to_bottom = true;
}
fn ts(shell: &ShellState, client: &str) -> Option<String> {
    shell
        .core
        .pending_file_messages
        .iter()
        .find(|p| p.client_msg_id == client)
        .map(|p| p.message_ts.clone())
}
pub(super) fn action(shell: &ShellState, client: &str) -> Option<Action> {
    presentation(shell, &ts(shell, client)?).map(|p| p.action)
}
pub(super) fn dispatch(shell: &mut ShellState, attempt: Attempt) {
    if !enabled(shell) {
        return;
    }
    let f = shell.upload_checks.fixture.as_mut().unwrap();
    f.calls += 1;
    assert!(f.pending.is_none());
    f.pending = Some(attempt);
}
pub(in crate::messaging) fn complete(
    mut state: Signal<ShellState>,
    result: Result<HistoryPage, Error>,
) {
    let attempt = state
        .write()
        .upload_checks
        .fixture
        .as_mut()
        .unwrap()
        .pending
        .take()
        .unwrap();
    complete_attempt(state, attempt, result);
}
pub(in crate::messaging) fn canonical(shell: &ShellState, client: &str) -> HistoryPage {
    let action = action(shell, client).unwrap();
    HistoryPage {
        messages: vec![Message {
            ts: Some("2000000000.000001".into()),
            channel: Some(action.key.channel),
            user: Some(action.sender),
            files: vec![super_platinum_core::slack::models::File {
                id: Some(action.files[0].clone()),
                title: Some("Delivery check.txt".into()),
                ..Default::default()
            }],
            text: Some("Confirmed in the conversation.".into()),
            ..Default::default()
        }],
        ..Default::default()
    }
}
pub(in crate::messaging) fn calls(shell: &ShellState) -> usize {
    shell.upload_checks.fixture.as_ref().unwrap().calls
}
