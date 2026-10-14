//! Explicit disconnected temporary journal; never resolves configured storage.
use super::*;
pub(super) const ACCOUNT: &str = "00000000-0000-4000-8000-000000000071";
pub(super) const FIRST: &str = "upload-check-one";
pub(super) const SECOND: &str = "upload-check-two";
pub(crate) fn install(shell: &mut ShellState) {
    super::super::delivery_check::fixture::install(shell);
    shell.core.active_account = Some(ACCOUNT.into());
    shell.core.pending_file_messages.clear();
    shell.pending_delivery.clear();
    let team = shell.core.active_team.clone().unwrap();
    let channel = shell.core.active_channel.clone().unwrap();
    let bag = shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .messages
        .entry(channel)
        .or_default();
    *bag = Default::default();
    bag.loaded = true;
    let explicit =
        std::env::var_os("SUPER_PLATINUM_UPLOAD_RESTART_DB").map(std::path::PathBuf::from);
    shell.upload_journal = match explicit.as_ref() {
        Some(path) => State::isolated_disk(path.clone()).expect("isolated restart journal"),
        None => State::isolated(),
    };
    if explicit.is_none()
        || std::env::var("SUPER_PLATINUM_UPLOAD_RESTART_PHASE").as_deref() == Ok("seed")
    {
        seed(shell).expect("synthetic restart receipts");
    }
    shell.refresh_from_core();
    shell.stick_to_bottom = true;
}
fn seed(shell: &ShellState) -> Result<(), AppError> {
    let team = shell.core.active_team.clone().unwrap();
    let channel = shell.core.active_channel.clone().unwrap();
    let user = shell.core.workspaces[&team].self_user_id.clone();
    for (index, client, caption) in [
        (
            1,
            FIRST,
            "Recovered after restarting. Check delivery without uploading again.",
        ),
        (2, SECOND, "A second receipt remains unconfirmed."),
    ] {
        let reserved = shell.upload_journal.backend.reserve(Draft {
            account_id: ACCOUNT.into(),
            team_id: team.clone(),
            user_id: user.clone(),
            channel_id: channel.clone(),
            thread_ts: None,
            reply_broadcast: false,
            client_msg_id: client.into(),
            optimistic_ts: format!("1999999999.00000{index}"),
            caption: caption.into(),
            attachments: vec![uploads::Attachment {
                name: format!("Restart evidence {index}.txt"),
                bytes: 128,
            }],
        })?;
        shell
            .upload_journal
            .backend
            .finalize(reserved.stored.ticket, vec![format!("F{index}")])?;
    }
    Ok(())
}
pub(super) fn enabled(shell: &ShellState) -> bool {
    super::super::delivery_check::fixture::enabled(shell)
        && shell.core.active_account.as_deref() == Some(ACCOUNT)
        && !matches!(shell.upload_journal.backend, Backend::Disk)
}
pub(super) async fn rows(journal: State) -> Result<Vec<Stored>, String> {
    let backend = journal.backend;
    tokio::task::spawn_blocking(move || backend.load(ACCOUNT))
        .await
        .map_err(|_| "journal task failed".to_owned())?
        .map_err(|_| "journal read failed".to_owned())
}
