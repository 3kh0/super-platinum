//! Supplemental disconnected geometry and trusted-Tab protocol; no file action.
use super::*;

pub(super) async fn run(mut state: Signal<ShellState>) -> Result<Value, String> {
    let directory = std::path::PathBuf::from(
        std::env::var_os("SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR")
            .ok_or("Ack directory required")?,
    );
    let database = std::path::PathBuf::from(
        std::env::var_os("SUPER_PLATINUM_SAVED_FILE_RECOVERY_DB")
            .ok_or("Private database required")?,
    );
    if !isolated(&state.peek())
        || !directory.is_absolute()
        || database != directory.join("saved-file-recovery.sqlite")
    {
        return Err("Exact disconnected private fixture required".into());
    }
    let mut eval = document::eval(&format!(
        "{}\n{}",
        include_str!("toast_geometry.js"),
        include_str!("toast_check.js")
    ));
    let mut commands = 0;
    let mut stages = 0;
    let mut tabs = 0;
    let names = ["toast-natural", "toast-delete-focus", "toast-dismissed"];
    loop {
        let value: Value = eval.recv().await.map_err(|_| "Toast callback closed")?;
        if !isolated(&state.peek()) {
            return Err("Fixture acquired credentials".into());
        }
        if let Some(command) = value["command"].as_str() {
            if stages != 0 || ["thread", "toast"].get(commands) != Some(&command) {
                return Err("Out-of-order supplemental command".into());
            }
            if command == "thread" {
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
            } else {
                // Explicit fixture setup, once only. No clipboard write or expiry override.
                state.write().show_toast("File link copied.");
            }
            commands += 1;
        } else if value["input"] == "tab" {
            if commands != 2
                || stages != 1
                || tabs >= 4
                || value["index"] != tabs
                || value["layout"]["ok"] != true
                || value["layout"]["toast"]["visible"] != true
            {
                return Err(format!("Invalid Tab admission: {value}"));
            }
            let path = directory.join(format!("{tabs}.tab.ack"));
            let check = path.clone();
            if tokio::task::spawn_blocking(move || check.try_exists())
                .await
                .map_err(|_| "Tab path task failed")?
                .map_err(|_| "Tab path unreadable")?
            {
                return Err("Stale Tab acknowledgement".into());
            }
            eprintln!(
                "message-file-toast-input: {}",
                json!({"ok":true,"noSession":true,"noTransport":true,"index":tabs,"layout":value["layout"]})
            );
            let mut received = false;
            for _ in 0..200 {
                let path = path.clone();
                match tokio::task::spawn_blocking(move || std::fs::read(path))
                    .await
                    .map_err(|_| "Tab acknowledgement task failed")?
                {
                    Ok(bytes) if bytes == b"tabbed\n" => {
                        received = true;
                        break;
                    }
                    Ok(_) => return Err("Malformed Tab acknowledgement".into()),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                    Err(_) => return Err("Tab acknowledgement unreadable".into()),
                }
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
            if !received {
                return Err("Tab acknowledgement deadline".into());
            }
            tabs += 1;
        } else if let Some(stage) = value["stage"].as_str() {
            if commands != 2 || names.get(stages) != Some(&stage) || value["layout"]["ok"] != true {
                return Err(format!("Invalid supplemental stage: {value}"));
            }
            if (stages == 1 && !(1..=4).contains(&tabs)) || value["tabCount"] != tabs {
                return Err("Trusted Tab count differs".into());
            }
            if value["layout"]["tabCount"] != tabs
                || value["layout"]["noActivation"] != true
                || value["layout"]["toastExpected"] != (stages < 2)
                || (stages < 2 && value["layout"]["toast"]["visible"] != true)
                || (stages == 1 && value["layout"]["focusedLabel"] != "Delete…")
            {
                return Err("Supplemental stage visibility/focus differs".into());
            }
            // Same owned capture/ACK path as the original seven-stage fixture.
            super::capture_stage(stage, stages, &value).await?;
            stages += 1;
        } else {
            if value["ok"] != true
                || stages != 3
                || commands != 2
                || value["tabCount"] != tabs
                || value["trustedTabs"] != true
                || value["noActivation"] != true
            {
                return Err(format!("Incomplete toast supplement: {value}"));
            }
            let _ = eval.send("received");
            return Ok(
                json!({"ok":true,"noSession":true,"noTransport":true,"noRequests":true,
                "noActivation":true,"trustedTabs":true,"toastPublications":1,"stages":3,"tabs":tabs,
                "layout":value["layout"]}),
            );
        }
        eval.send("received")
            .map_err(|_| "Toast acknowledgment failed")?;
    }
}
