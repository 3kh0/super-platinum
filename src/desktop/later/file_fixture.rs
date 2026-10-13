//! Disconnected native fixture data, gated independently from production I/O.
use crate::state::{MainView, Overlay, ShellState, ViewerVm};
use dioxus::prelude::*;
use super_platinum_core::{
    MediaAssetKind,
    later::{SavedItem, SavedPage},
    slack::models::File,
};
pub(crate) fn enabled(shell: &ShellState) -> bool {
    matches!(
        std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
        Ok("later-files" | "later-files-narrow")
    ) && std::env::var_os("SUPER_PLATINUM_LATER_FILES_CHECK").is_some()
        && shell.core.session.is_none()
        && shell.core.transport.is_none()
        && shell.core.active_account.as_deref() == Some("00000000-0000-4000-8000-000000000098")
}
pub(super) fn metadata(id: &str) -> Option<File> {
    if !matches!(
        std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
        Ok("later-files" | "later-files-narrow")
    ) || std::env::var_os("SUPER_PLATINUM_LATER_FILES_CHECK").is_none()
    {
        return None;
    }
    let title = match id {
        "FSTART" => "Saved release notes",
        "FCOLDOPEN" => "Cold opened notes",
        "FPREVIEW" => "Background loaded notes",
        _ => return None,
    };
    Some(serde_json::from_value(serde_json::json!({"id":id,"title":title,"name":format!("{id}.txt"),"mimetype":"text/plain","filetype":"text","pretty_type":"Plain text","size":48,"user":"U0","url_private":format!("https://files.slack.com/fixture/{id}"),"permalink":format!("https://app.slack.com/files/U0/{id}")})).unwrap())
}
pub(crate) fn apply(shell: &mut ShellState) {
    seed_recovery();
    shell.core.active_account = Some("00000000-0000-4000-8000-000000000098".into());
    shell.core.session = None;
    shell.core.transport = None;
    shell.core.active_team = Some("T1".into());
    shell.main_view = MainView::Later;
    shell
        .core
        .later
        .ensure_scope(shell.core.account_epoch, "T1");
    let generation = shell.core.later.begin(Default::default(), false);
    shell.core.later.apply_page(
        generation,
        SavedPage {
            counts: super_platinum_core::later::SavedCounts {
                uncompleted_count: 4,
                ..Default::default()
            },
            saved_items: ["FCOLDOPEN", "FPREVIEW", "FMISSING", "FDELETED"]
                .into_iter()
                .map(|id| SavedItem {
                    item_type: "file".into(),
                    item_id: id.into(),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        },
        false,
        None,
    );
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .deleted_files
        .insert("FDELETED".into());
    // Register exact fixture bytes before the production viewer requests them.
    for id in ["FSTART", "FCOLDOPEN", "FPREVIEW"] {
        let file = raw_metadata(id);
        let asset = shell.media.register_deferred(
            MediaAssetKind::Attachment,
            file.full_url().unwrap(),
            "text/plain",
        );
        shell.media.insert(
            asset.clone(),
            "text/plain",
            format!("Native saved file contents: {id}\n").into_bytes(),
        );
        if id == "FSTART" {
            shell.viewer = Some(ViewerVm {
                file: super_platinum_core::file_actions::identity(&file),
                id: asset,
                name: super_platinum_core::state::file_title(&file),
                download_name: super_platinum_core::state::file_download_name(&file),
                mime: "text/plain".into(),
            });
            shell.overlay = Some(Overlay::Viewer);
        }
    }
}
fn raw_metadata(id: &str) -> File {
    // Fixture seeding may also be used by the visual harness without CHECK.
    let title = match id {
        "FSTART" => "Saved release notes",
        "FCOLDOPEN" => "Cold opened notes",
        _ => "Background loaded notes",
    };
    serde_json::from_value(serde_json::json!({"id":id,"title":title,"name":format!("{id}.txt"),"mimetype":"text/plain","filetype":"text","pretty_type":"Plain text","size":48,"user":"U0","url_private":format!("https://files.slack.com/fixture/{id}"),"permalink":format!("https://app.slack.com/files/U0/{id}")})).unwrap()
}
pub(crate) async fn native_check(state: Signal<ShellState>) {
    if enabled(&state.peek()) {
        super::file_check::run(state).await;
    }
}

fn seed_recovery() {
    use super_platinum_core::saved_file_receipts::{Action, FileScope, Journal, Operation};
    if !matches!(
        std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
        Ok("later-files" | "later-files-narrow")
    ) || std::env::var("SUPER_PLATINUM_LATER_FILES_CHECK").as_deref() != Ok("1")
    {
        return;
    }
    let Some(path) =
        std::env::var_os("SUPER_PLATINUM_SAVED_FILE_RECOVERY_DB").map(std::path::PathBuf::from)
    else {
        return;
    };
    if !path.is_absolute()
        || path.file_name().and_then(|v| v.to_str()) != Some("saved-file-recovery.sqlite")
    {
        return;
    }
    // Fixture construction precedes any mounted UI; use the same actual journal
    // API on a worker, without accessing an account's production config path.
    std::thread::spawn(move || {
        let mut journal = Journal::open(path, "00000000-0000-4000-8000-000000000098")
            .expect("injected fixture journal");
        journal
            .begin(
                FileScope {
                    team_id: "T1".into(),
                    user_id: "U0".into(),
                    file_id: "FRECOVERY".into(),
                },
                Operation {
                    title: "Unconfirmed removed file".into(),
                    action: Action::Remove,
                },
                super_platinum_core::state::now_secs().max(0) as u64,
            )
            .expect("fixture receipt");
    })
    .join()
    .expect("fixture worker");
}

pub(super) async fn verify_receipts() -> Result<serde_json::Value, String> {
    use super_platinum_core::saved_file_receipts::{Action, FileScope, Journal, Status};
    if !matches!(
        std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
        Ok("later-files" | "later-files-narrow")
    ) || std::env::var("SUPER_PLATINUM_LATER_FILES_CHECK").as_deref() != Ok("1")
    {
        return Err("fixture gate".into());
    }
    let path = std::env::var_os("SUPER_PLATINUM_SAVED_FILE_RECOVERY_DB")
        .map(std::path::PathBuf::from)
        .ok_or("fixture storage")?;
    if !path.is_absolute()
        || path.file_name().and_then(|v| v.to_str()) != Some("saved-file-recovery.sqlite")
    {
        return Err("fixture storage scope".into());
    }
    tokio::task::spawn_blocking(move|| {
        let journal=Journal::open(path,"00000000-0000-4000-8000-000000000098").map_err(|e|e.to_string())?;
        let get=|id:&str|journal.get(&FileScope{team_id:"T1".into(),user_id:"U0".into(),file_id:id.into()}).map_err(|e|e.to_string());
        let start=get("FSTART")?.ok_or("missing saved removal receipt")?;
        let cold=get("FCOLDOPEN")?.ok_or("missing new uncertain receipt")?;
        let recovered=get("FRECOVERY")?.ok_or("missing retired receipt")?;
        let ok=start.status==Status::Confirmed&&start.operation.action==Action::Remove
            &&cold.status==Status::Uncertain&&cold.operation.action==Action::Remove
            &&recovered.status==Status::Retired&&recovered.retry_at>0;
        Ok(serde_json::json!({"ok":ok,"savedThenRemoved":start.status,"newUncertainty":cold.status,"retired":recovered.status,"retiredCooldown":recovered.retry_at}))
    }).await.map_err(|e|e.to_string())?
}
