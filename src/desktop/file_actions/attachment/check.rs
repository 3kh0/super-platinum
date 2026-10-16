//! Exact named, disconnected fixture. Actual controls; no clipboard/network writes.
use super::*;
use serde_json::{Value, json};
const NAME: &str = "message-file-actions";
mod toast;
const STAGES: [&str; 7] = [
    "initial",
    "menu",
    "confirmation",
    "offline",
    "account-reset",
    "thread-menu",
    "deleted",
];
fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("message-file-actions" | "message-file-actions-narrow")
        )
}
fn isolated(shell: &ShellState) -> bool {
    shell.core.session.is_none() && shell.core.transport.is_none()
}
pub(crate) fn fixture(shell: &mut ShellState) {
    crate::file_actions_check::fixture(shell);
    shell.core.active_account = Some("00000000-0000-4000-8000-000000000161".into());
    shell.main_view = crate::state::MainView::Home;
    shell.channel_tab = crate::state::ChannelTab::Messages;
    shell.overlay = None;
    shell.viewer = None;
    let team = shell.core.active_team.clone().unwrap();
    let channel = shell.core.active_channel.clone().unwrap();
    let user = shell.core.workspaces[&team].self_user_id.clone();
    let files:Vec<super_platinum_core::slack::models::File>=serde_json::from_value(json!([
        {"id":"FACTIONS","title":"Release notes · 日本語","name":"release-notes.txt","user":user,"filetype":"text","mimetype":"text/plain","url_private":"https://files.slack.com/fixture/actions","permalink":"https://fixture.slack.com/files/USELF/FACTIONS","size":42},
        {"id":"FIMAGE","title":"Design image","name":"design.png","user":"UOTHER","filetype":"png","mimetype":"image/png","url_private":"https://files.slack.com/fixture/image","thumb_360":"https://files.slack.com/fixture/thumb","original_w":120,"original_h":48,"permalink":"https://fixture.slack.com/files/UOTHER/FIMAGE"},
        {"id":"FNOLINK","title":"Private document","name":"private.txt","user":"UOTHER","mimetype":"text/plain","url_private":"https://files.slack.com/fixture/private","permalink":"javascript:alert(1)"}
    ])).unwrap();
    let mut bag = super_platinum_core::state::ChannelMessages::default();
    for (index, file) in files.iter().enumerate() {
        bag.upsert(serde_json::from_value(json!({"ts":format!("171980040{}.000000",index),"channel":channel,"user":user,"text":"","files":[file]})).unwrap());
    }
    shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .messages
        .insert(channel.clone(), bag.clone());
    shell.messages = bag
        .messages
        .iter()
        .map(|m| crate::message_vm::message_vm(&shell.core.workspaces[&team], m, &shell.media))
        .collect();
    // Thumbnail registration is the same projection path; only its bytes are synthetic.
    if let Some(preview) = files[1].preview() {
        let id = shell.media.register_image(
            super_platinum_core::MediaAssetKind::Attachment,
            &preview.url,
            preview.mime,
        );
        shell.media.insert(id,"image/svg+xml",br##"<svg xmlns="http://www.w3.org/2000/svg" width="120" height="48"><rect width="120" height="48" fill="#777"/><text x="8" y="28" fill="white">Fixture image</text></svg>"##.as_slice());
    }
    shell.sync_fixture_messages();
    shell.thread_root = None;
    shell.thread_messages.clear();
}
pub(crate) fn start(state: Signal<ShellState>, _desktop: dioxus::desktop::DesktopContext) {
    if !requested()
        || std::env::var("SUPER_PLATINUM_MESSAGE_FILE_ACTIONS_CHECK").as_deref() != Ok("1")
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        if std::env::var("SUPER_PLATINUM_MESSAGE_FILE_TOAST_CHECK").as_deref() == Ok("1") {
            let result = toast::run(state)
                .await
                .unwrap_or_else(|error| json!({"ok":false,"error":error}));
            eprintln!("message-file-toast-check: {result}");
            return;
        }
        let result = run(state)
            .await
            .unwrap_or_else(|error| json!({"ok":false,"error":error}));
        eprintln!("{NAME}-check: {result}");
    });
}
async fn run(mut state: Signal<ShellState>) -> Result<Value, String> {
    if !isolated(&state.peek()) {
        return Err("Disconnected fixture required".into());
    }
    let acknowledgments = std::path::PathBuf::from(
        std::env::var_os("SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR")
            .ok_or("Capture directory required")?,
    );
    let database = std::path::PathBuf::from(
        std::env::var_os("SUPER_PLATINUM_SAVED_FILE_RECOVERY_DB")
            .ok_or("Private saved-file database required")?,
    );
    if !acknowledgments.is_absolute()
        || database != acknowledgments.join("saved-file-recovery.sqlite")
    {
        return Err("Saved-file database must use the owned capture directory".into());
    }
    let mut eval = document::eval(include_str!("check.js"));
    let mut ordinal = 0;
    let mut commands = Vec::new();
    loop {
        let value: Value = eval
            .recv()
            .await
            .map_err(|_| "Native message-file callback closed")?;
        if !isolated(&state.peek()) {
            return Err("Fixture acquired credentials".into());
        }
        if let Some(command) = value["command"].as_str() {
            let expected = [("account", 4), ("thread", 5), ("delete", 6)];
            if expected.get(commands.len()) != Some(&(command, ordinal)) {
                return Err("Duplicate or out-of-order fixture command".into());
            }
            commands.push(command.to_owned());
        }
        match value["command"].as_str() {
            Some("account") => {
                state.write().core.account_epoch += 1;
            }
            Some("thread") => {
                let mut shell = state.write();
                let team = shell.core.active_team.clone().unwrap();
                let channel = shell.core.active_channel.clone().unwrap();
                let root = "1719800400.000000";
                let mut bag = super_platinum_core::state::ChannelMessages::default();
                let mut message =
                    shell.core.workspaces[&team].messages[&channel].messages[0].clone();
                message.thread_ts = Some(root.into());
                bag.upsert(message);
                shell.core.threads.insert((team, channel, root.into()), bag);
                shell.begin_thread(root.into(), None);
                shell.refresh_thread_from_core();
            }
            Some("delete") => {
                let mut shell = state.write();
                let team = shell.core.active_team.clone().unwrap();
                shell.core.remove_file(&team, "FACTIONS");
                shell.refresh_from_core();
            }
            Some(_) => return Err("Unknown fixture command".into()),
            None => {
                if let Some(stage) = value["stage"].as_str() {
                    if STAGES.get(ordinal) != Some(&stage) || value["layout"]["ok"] != true {
                        return Err(format!("Invalid stage: {value}"));
                    }
                    capture_stage(stage, ordinal, &value).await?;
                    ordinal += 1;
                } else {
                    if value["ok"] != true || ordinal != STAGES.len() || commands.len() != 3 {
                        return Err(format!("Incomplete native check: {value}"));
                    }
                    let saved = crate::later::file_actions::Target::capture(
                        &state.peek(),
                        "FACTIONS",
                        "Release notes",
                    )
                    .ok_or("Saved-file target required")?;
                    if crate::later::file_actions::load(&saved)
                        .await?
                        .receipt
                        .is_some()
                    {
                        return Err("Disconnected Save unexpectedly reserved a write".into());
                    }
                    let _ = eval.send("received");
                    return Ok(
                        json!({"ok":true,"noSession":true,"noTransport":true,"stages":ordinal,"commands":commands.len(),"commandOrder":commands,"noRequests":true,
                        "permissions":true,"exactCopyStub":true,"cancelPreserved":true,"offlinePreserved":true,"saveOfflinePreserved":true,"saveJournalEmpty":true,"accountReset":true,"threadIdentity":true,"deletedRefused":true,"layout":value["layout"]}),
                    );
                }
            }
        }
        eval.send("received")
            .map_err(|_| "Native stage acknowledgement failed")?;
    }
}
async fn capture_stage(stage: &str, ordinal: usize, value: &Value) -> Result<(), String> {
    let directory = std::env::var_os("SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR")
        .ok_or("Capture acknowledgements required")?;
    let directory = std::path::PathBuf::from(directory);
    if !directory.is_absolute() {
        return Err("Absolute capture directory required".into());
    }
    let path = directory.join(format!("{ordinal}.ack"));
    let before = path.clone();
    if tokio::task::spawn_blocking(move || before.exists())
        .await
        .map_err(|_| "Acknowledgement check failed")?
    {
        return Err("Reused capture acknowledgement".into());
    }
    eprintln!(
        "{NAME}-stage: {}",
        json!({"ok":true,"noSession":true,"noTransport":true,"ordinal":ordinal,"stage":stage,"layout":value["layout"]})
    );
    for _ in 0..600 {
        let path = path.clone();
        match tokio::task::spawn_blocking(move || std::fs::read(path))
            .await
            .map_err(|_| "Acknowledgement task failed")?
        {
            Ok(bytes) if bytes == b"captured\n" => return Ok(()),
            Ok(_) => return Err("Malformed acknowledgement".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err("Acknowledgement unreadable".into()),
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err("Capture acknowledgement deadline".into())
}
