//! Disconnected synthetic file-paste evidence; this does not exercise the OS clipboard.
use crate::state::ShellState;
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

const CHANNEL_NAME: &str = "日本語 🦊.txt";
const THREAD_NAME: &str = "résumé 🎹.txt";

const CHANNEL: &[u8] = &[83, 80, 0, 10, 255];
const THREAD: &[u8] = &[84, 72, 82, 69, 65, 68, 0, 254];

pub(crate) async fn run(state: Signal<ShellState>) {
    if std::env::var("SUPER_PLATINUM_FILE_PASTE_CHECK").as_deref() != Ok("1")
        || !matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("threads-file-paste" | "threads-file-paste-narrow")
        )
    {
        return;
    }
    let receipt = match check(state).await {
        Ok(value) => value,
        Err(error) => json!({"ok":false,"error":error}),
    };
    eprintln!("threads-file-paste-check: {receipt}");
}

fn isolated(shell: &ShellState) -> Result<(), String> {
    if crate::fixture::is_fixture()
        && shell.core.session.is_none()
        && shell.core.transport.is_none()
    {
        Ok(())
    } else {
        Err("File paste check requires a disconnected fixture".into())
    }
}

fn invariant(shell: &ShellState) -> Value {
    json!({"account":shell.core.active_account,"epoch":shell.core.account_epoch,
        "team":shell.core.active_team,"channel":shell.core.active_channel,
        "generation":shell.channel_generation,"threadGeneration":shell.thread_generation,
        "sendSequence":shell.core.send_seq,"pendingDelivery":shell.pending_delivery.len(),
        "pendingFiles":shell.core.pending_file_messages.len(),
        "channelRows":shell.channel_rows.iter().map(|m| (&m.ts,m.pending)).collect::<Vec<_>>(),
        "threadRows":shell.thread_messages.iter().map(|m| (&m.ts,m.pending)).collect::<Vec<_>>(),
        "channelText":shell.core.composer.text,"threadText":shell.core.thread_composer.text})
}

async fn private_directory(variable: &str) -> Result<PathBuf, String> {
    let path = std::env::var_os(variable)
        .map(PathBuf::from)
        .ok_or("Missing private file-paste directory")?;
    tokio::task::spawn_blocking(move || {
        if !path.is_absolute()
            || path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err("File-paste directory must be absolute".into());
        }
        let metadata =
            std::fs::symlink_metadata(&path).map_err(|_| "Missing file-paste directory")?;
        if !super::ordinary_directory(&metadata)
            || std::fs::read_dir(&path)
                .map_err(|_| "Unreadable file-paste directory")?
                .next()
                .is_some()
        {
            return Err("File-paste directory must be ordinary, fresh and empty".into());
        }
        path.canonicalize()
            .map_err(|_| "Cannot resolve file-paste directory".into())
    })
    .await
    .map_err(|_| "File-paste directory worker failed")?
}

async fn check(mut state: Signal<ShellState>) -> Result<Value, String> {
    isolated(&state.peek())?;
    let directory = private_directory("SUPER_PLATINUM_FIXTURE_PASTE_ROOT").await?;
    isolated(&state.peek())?;
    let acknowledgments = private_directory("SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR").await?;
    if directory == acknowledgments {
        return Err("Paste and acknowledgement directories must differ".into());
    }
    let (root, baseline) = {
        let mut shell = state.write();
        isolated(&shell)?;
        if shell.thread_root.is_none()
            || shell.overlay.is_some()
            || !shell.core.composer_attachments.is_empty()
            || !shell.core.thread_composer_attachments.is_empty()
            || !shell.core.composer.text.is_empty()
            || !shell.core.thread_composer.text.is_empty()
        {
            return Err("Expected empty two-composer fixture".into());
        }
        let root = shell.thread_root.take();
        (root, invariant(&shell))
    };
    let mut eval = document::eval(include_str!("file_check.js"));
    eval.send(json!([
        {"stage":"channel","field":"channel-composer","name":CHANNEL_NAME,"bytes":CHANNEL},
        {"stage":"thread","field":"thread-composer","name":THREAD_NAME,"bytes":THREAD}
    ]))
    .map_err(|_| "File-paste evaluator setup failed")?;
    let mut final_layout = Value::Null;
    let mut paths = Vec::new();
    for ordinal in 0..2 {
        let receipt = receive(&mut eval).await?;
        let stage = if ordinal == 0 { "channel" } else { "thread" };
        if receipt["kind"] != "stage"
            || receipt["ordinal"] != ordinal
            || receipt["stage"] != stage
            || receipt["synthetic"] != true
            || receipt["layout"]["ok"] != true
            || receipt["layout"]["displayName"]
                != if ordinal == 0 {
                    CHANNEL_NAME
                } else {
                    THREAD_NAME
                }
        {
            return Err(format!(
                "Unexpected synthetic file-paste receipt: {receipt}"
            ));
        }
        paths = verify_state(state, &baseline, &root, ordinal)?;
        verify_files(directory.clone(), paths.clone()).await?;
        isolated(&state.peek())?;
        final_layout = receipt["layout"].clone();
        eprintln!(
            "threads-file-paste-stage: {}",
            json!({"ok":true,"ordinal":ordinal,"stage":stage,
            "noSession":true,"noTransport":true,"noSend":true,"synthetic":true,"layout":final_layout})
        );
        wait_ack(state, &acknowledgments, ordinal).await?;
        verify_state(state, &baseline, &root, ordinal)?;
        if ordinal == 0 {
            state.write().thread_root = root.clone();
        }
        eval.send("captured")
            .map_err(|_| "File-paste capture acknowledgment failed")?;
    }
    if receive(&mut eval).await?["kind"] != "done" {
        return Err("File-paste evaluator did not finish".into());
    }
    verify_state(state, &baseline, &root, 1)?;
    verify_files(directory, paths.clone()).await?;
    isolated(&state.peek())?;
    Ok(
        json!({"ok":true,"noSession":true,"noTransport":true,"noSend":true,
        "synthetic":true,"osClipboardUntouched":true,"exactRustAttachments":true,"exactFileBytes":true,
        "outsideAndStaleUnattached":true,"originalNamesPreserved":true,"names":[CHANNEL_NAME,THREAD_NAME],"fileCount":2,"paths":paths,"layout":final_layout}),
    )
}

fn verify_state(
    state: Signal<ShellState>,
    baseline: &Value,
    root: &Option<String>,
    ordinal: usize,
) -> Result<Vec<PathBuf>, String> {
    let shell = state.peek();
    isolated(&shell)?;
    let actual = invariant(&shell);
    if actual != *baseline {
        let changed: Vec<_> = baseline.as_object().unwrap().iter()
            .filter(|(key, expected)| actual.get(*key) != Some(*expected))
            .map(|(key, expected)| json!({"field":key,"before":summary(expected),"after":summary(&actual[key])}))
            .collect();
        return Err(format!(
            "Paste changed invariant fields at stage {ordinal}: {}",
            json!(changed)
        ));
    }
    let expected_root = if ordinal == 0 { None } else { root.clone() };
    if shell.overlay.is_some()
        || shell.thread_root != expected_root
        || shell.core.composer_attachments.len() != 1
        || shell.core.thread_composer_attachments.len() != ordinal
    {
        return Err(format!(
            "Paste destination mismatch at stage {ordinal}: {}",
            json!({
                "overlay":shell.overlay.is_some(),"root":shell.thread_root,"expectedRoot":expected_root,
                "channelAttachments":shell.core.composer_attachments.len(),
                "threadAttachments":shell.core.thread_composer_attachments.len(),"expectedThreadAttachments":ordinal
            })
        ));
    }
    let attachments = shell
        .core
        .composer_attachments
        .iter()
        .chain(shell.core.thread_composer_attachments.iter());
    let mut paths = Vec::new();
    for (index, attachment) in attachments.enumerate() {
        let (name, bytes) = if index == 0 {
            (CHANNEL_NAME, CHANNEL)
        } else {
            (THREAD_NAME, THREAD)
        };
        if attachment.name != name
            || attachment.path.file_name().and_then(|v| v.to_str()) == Some(name)
            || attachment.bytes != bytes.len() as u64
            || attachment.uploading
            || attachment.upload_started.is_some()
            || attachment.upload_cancel.is_some()
            || attachment.upload_progress.is_some()
        {
            return Err("Unexpected pasted attachment metadata or upload".into());
        }
        paths.push(attachment.path.clone());
    }
    Ok(paths)
}

async fn verify_files(root: PathBuf, paths: Vec<PathBuf>) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let entries = std::fs::read_dir(&root)
            .map_err(|_| "Paste root unreadable")?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "Paste entries unreadable")?;
        if entries.len() != paths.len() {
            return Err("Unexpected extra paste files".into());
        }
        for (index, path) in paths.iter().enumerate() {
            let metadata = std::fs::symlink_metadata(path).map_err(|_| "Paste file missing")?;
            if !metadata.is_file()
                || metadata.file_type().is_symlink()
                || path.parent() != Some(root.as_path())
            {
                return Err("Paste file escaped private root".into());
            }
            let bytes = std::fs::read(path).map_err(|_| "Paste file unreadable")?;
            if bytes != if index == 0 { CHANNEL } else { THREAD } {
                return Err("Paste bytes changed".into());
            }
        }
        Ok(())
    })
    .await
    .map_err(|_| "Paste file verification worker failed")?
}

async fn receive(eval: &mut document::Eval) -> Result<Value, String> {
    tokio::time::timeout(Duration::from_secs(20), eval.recv::<Value>())
        .await
        .map_err(|_| "File-paste evaluator timed out")?
        .map_err(|_| "File-paste evaluator ended early".into())
}

async fn wait_ack(
    state: Signal<ShellState>,
    directory: &Path,
    ordinal: usize,
) -> Result<(), String> {
    for _ in 0..600 {
        isolated(&state.peek())?;
        let path = directory.join(format!("{ordinal}.ack"));
        let read = tokio::task::spawn_blocking(move || std::fs::read(path))
            .await
            .map_err(|_| "Paste acknowledgment worker failed")?;
        match read {
            Ok(bytes) if bytes == b"captured\n" => return Ok(()),
            Ok(_) => return Err("Malformed paste capture acknowledgment".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err("Paste acknowledgment unreadable".into()),
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err("Paste capture timed out".into())
}

// Fixture-only diagnostics keep a viewport mismatch readable without dumping
// hundreds of known fixture rows. Ownership and text values remain exact.
fn summary(value: &Value) -> Value {
    value
        .as_array()
        .map(|items| json!({"count":items.len(),"first":items.first(),"last":items.last()}))
        .unwrap_or_else(|| value.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invariant_tracks_complete_rows_not_the_resizable_viewport_projection() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        let baseline = invariant(&shell);
        assert!(shell.channel_rows.len() > 2);
        // Initial mount and an attachment chip resize the bounded viewport.
        // Neither operation changes the complete conversation row index.
        shell.timeline_start = shell.channel_rows.len() - 2;
        shell.timeline_end = shell.channel_rows.len();
        shell.reproject_channel_window();
        assert_eq!(shell.messages.len(), 2);
        assert_eq!(invariant(&shell), baseline);
        shell.channel_rows[0].pending = !shell.channel_rows[0].pending;
        assert_ne!(invariant(&shell), baseline);
    }
}
