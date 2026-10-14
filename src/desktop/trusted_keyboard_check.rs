//! Windows input-stream verification. No clipboard, send, or synthetic DOM input.
use crate::state::ShellState;
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::{path::Path, time::Duration};

const ORIGINAL: &str = "Native 日本語 🦊 abc";
const DELETED: &str = "Native 日本語 🦊 ab";
const REPLACED: &str = "Native 日本語 🦊 aZ";

pub async fn run(state: Signal<ShellState>) {
    if !cfg!(target_os = "windows")
        || std::env::var("SUPER_PLATINUM_TRUSTED_KEYBOARD_CHECK").as_deref() != Ok("1")
        || !matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("threads" | "threads-keyboard-narrow")
        )
    {
        return;
    }
    let receipt = match check(state).await {
        Ok(value) => value,
        Err(error) => json!({"ok":false,"error":error}),
    };
    eprintln!("trusted-keyboard-check: {receipt}");
}

fn disconnected(shell: &ShellState) -> Result<(), String> {
    if shell.core.session.is_some() || shell.core.transport.is_some() {
        return Err("Connected keyboard fixture refused".into());
    }
    Ok(())
}

fn unchanged(shell: &ShellState) -> Value {
    json!({"account":shell.core.active_account,"epoch":shell.core.account_epoch,
        "team":shell.core.active_team,"channel":shell.core.active_channel,
        "generation":shell.channel_generation,"sendSequence":shell.core.send_seq,
        // The mounted rich rows change as resize/measurement settles. The full
        // history index must remain stable throughout editing.
        "channelRows":shell.channel_rows.iter().map(|m| (&m.id,&m.ts,m.pending)).collect::<Vec<_>>(),
        "threadRows":shell.thread_messages.iter().map(|m| (&m.ts,m.pending)).collect::<Vec<_>>(),
        "pendingDelivery":shell.pending_delivery.len(),"pendingFiles":shell.core.pending_file_messages.len(),
        "channelAttachments":shell.core.composer_attachments.len(),"threadAttachments":shell.core.thread_composer_attachments.len()})
}

fn stages() -> Vec<Value> {
    let full = ORIGINAL.encode_utf16().count();
    let short = DELETED.encode_utf16().count();
    let mut out = Vec::new();
    for field in ["channel-composer", "thread-composer"] {
        for (action, before, after, selection) in [
            ("type", "", ORIGINAL, [full, full]),
            ("backspace", ORIGINAL, DELETED, [short, short]),
            ("select-left", DELETED, DELETED, [short - 1, short]),
            ("replace", DELETED, REPLACED, [short, short]),
        ] {
            out.push(json!({"ordinal":out.len(),"field":field,"action":action,
                "before":before,"after":after,"selection":selection}));
        }
    }
    out
}

async fn check(mut state: Signal<ShellState>) -> Result<Value, String> {
    let directory = std::env::var_os("SUPER_PLATINUM_TRUSTED_KEYBOARD_ACK_DIR")
        .map(std::path::PathBuf::from)
        .ok_or("Missing private keyboard acknowledgments")?;
    let checked_directory = directory.clone();
    tokio::task::spawn_blocking(move || {
        if !checked_directory.is_absolute()
            || !checked_directory
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("sp-keyboard-ack-"))
        {
            return Err(
                "Keyboard acknowledgments require an absolute private directory".to_owned(),
            );
        }
        let metadata = std::fs::symlink_metadata(&checked_directory)
            .map_err(|_| "Keyboard acknowledgment directory is missing")?;
        if !metadata.is_dir()
            || metadata.file_type().is_symlink()
            || std::fs::read_dir(checked_directory)
                .map_err(|_| "Keyboard acknowledgment directory is unreadable")?
                .next()
                .is_some()
        {
            return Err("Keyboard acknowledgment directory must be fresh and empty".into());
        }
        Ok(())
    })
    .await
    .map_err(|_| "Keyboard directory worker failed")??;
    let (root, invariant) = {
        let mut shell = state.write();
        disconnected(&shell)?;
        if !shell.core.composer.text.is_empty()
            || !shell.core.thread_composer.text.is_empty()
            || shell.overlay.is_some()
            || shell.thread_root.is_none()
        {
            return Err("Expected empty, disconnected two-composer fixture".into());
        }
        // At narrow widths the thread covers the channel field. Each target is
        // tested visibly; this fixture-only staging performs no Slack navigation.
        let root = shell.thread_root.take();
        (root, unchanged(&shell))
    };
    let plan = stages();
    let mut eval = document::eval(include_str!("trusted_keyboard_check.js"));
    eval.send(plan.clone())
        .map_err(|_| "Keyboard evaluator setup failed")?;
    for (ordinal, stage) in plan.iter().enumerate() {
        let ready = receive(&mut eval).await?;
        validate_dom(&ready, stage, "ready")?;
        check_state(state, &invariant, &root, ordinal, false)?;
        eprintln!(
            "trusted-keyboard-ready: {}",
            json!({"ok":true,"ordinal":ordinal,
            "field":stage["field"],"action":stage["action"],"dom":ready["dom"],
            "noSession":true,"noTransport":true,"noSend":true})
        );
        wait_ack(&directory, ordinal, "input").await?;
        disconnected(&state.peek())?;
        eval.send("input-complete")
            .map_err(|_| "Keyboard input acknowledgment failed")?;
        let result = receive(&mut eval).await?;
        validate_dom(&result, stage, "result")?;
        validate_events(&result, stage)?;
        // DOM input reaches Rust through the serial renderer bridge; observing
        // the native event does not make the model update atomic with it.
        for attempt in 0..100 {
            disconnected(&state.peek())?;
            match check_state(state, &invariant, &root, ordinal, true) {
                Ok(()) => break,
                Err(error) if attempt == 99 => return Err(error),
                Err(_) => tokio::time::sleep(Duration::from_millis(20)).await,
            }
        }
        eprintln!(
            "trusted-keyboard-stage: {}",
            json!({"ok":true,"ordinal":ordinal,
            "field":stage["field"],"action":stage["action"],"dom":result["dom"],"events":result["events"],
            "noSession":true,"noTransport":true,"noSend":true,"exactRustState":true})
        );
        wait_ack(&directory, ordinal, "captured").await?;
        check_state(state, &invariant, &root, ordinal, true)?;
        if ordinal == 3 {
            state.write().thread_root = root.clone();
        }
        eval.send("captured")
            .map_err(|_| "Keyboard capture acknowledgment failed")?;
    }
    if receive(&mut eval).await?["kind"] != "done" {
        return Err("Keyboard evaluator did not finish".into());
    }
    Ok(
        json!({"ok":true,"stages":8,"noSession":true,"noTransport":true,"noSend":true,
        "clipboardUntouched":true,"inputKind":"Windows SendInput; Unicode packets and editing keys; not OS IME",
        "channelText":state.peek().core.composer.text,"threadText":state.peek().core.thread_composer.text}),
    )
}

async fn receive(eval: &mut document::Eval) -> Result<Value, String> {
    tokio::time::timeout(Duration::from_secs(40), eval.recv::<Value>())
        .await
        .map_err(|_| "Keyboard evaluator timed out")?
        .map_err(|_| "Keyboard evaluator ended early".into())
}

fn validate_dom(receipt: &Value, stage: &Value, kind: &str) -> Result<(), String> {
    if receipt["kind"] != kind
        || receipt["ordinal"] != stage["ordinal"]
        || receipt["field"] != stage["field"]
        || receipt["action"] != stage["action"]
        || receipt["dom"]["focused"] != true
        || receipt["dom"]["visible"] != true
        || receipt["dom"]["text"] != stage[if kind == "ready" { "before" } else { "after" }]
        || (kind == "result" && receipt["dom"]["selection"] != stage["selection"])
    {
        return Err(format!(
            "Unexpected keyboard {kind} receipt at {}",
            stage["ordinal"]
        ));
    }
    Ok(())
}

fn validate_events(receipt: &Value, stage: &Value) -> Result<(), String> {
    let events = receipt["events"]
        .as_array()
        .ok_or("Missing trusted events")?;
    if events.is_empty()
        || events.len() >= 256
        || events.iter().any(|event| {
            event["trusted"] != true
                || event["field"] != stage["field"]
                || event["composing"] != false
                || event["key"] == "Enter"
                || !matches!(
                    event["type"].as_str(),
                    Some("keydown" | "keyup" | "beforeinput" | "input")
                )
        })
    {
        return Err("Untrusted, composing, forbidden or mistargeted keyboard events".into());
    }
    for kind in ["beforeinput", "input"] {
        let changes: Vec<_> = events
            .iter()
            .filter(|event| event["type"] == kind)
            .collect();
        if stage["action"] == "select-left" {
            if !changes.is_empty() {
                return Err("Selection unexpectedly edited text".into());
            }
        } else {
            let expected = if stage["action"] == "backspace" {
                "deleteContentBackward"
            } else {
                "insertText"
            };
            if changes.is_empty() || changes.iter().any(|event| event["inputType"] != expected) {
                return Err("Missing trusted native edit events".into());
            }
        }
    }
    if stage["action"] == "select-left"
        && !events
            .iter()
            .any(|e| e["type"] == "keydown" && e["key"] == "ArrowLeft")
    {
        return Err("Missing trusted ArrowLeft selection".into());
    }
    Ok(())
}

fn check_state(
    state: Signal<ShellState>,
    invariant: &Value,
    root: &Option<String>,
    ordinal: usize,
    after: bool,
) -> Result<(), String> {
    let shell = state.peek();
    disconnected(&shell)?;
    let text = match (ordinal % 4, after) {
        (0, false) => "",
        (0, true) | (1, false) => ORIGINAL,
        (3, true) => REPLACED,
        _ => DELETED,
    };
    let (channel, thread) = if ordinal < 4 {
        (text, "")
    } else {
        (REPLACED, text)
    };
    let current = unchanged(&shell);
    let expected_root = if ordinal < 4 { None } else { root.clone() };
    if current != *invariant
        || shell.overlay.is_some()
        || shell.thread_root != expected_root
        || shell.core.composer.text != channel
        || shell.core.thread_composer.text != thread
    {
        let fields: Vec<_> = invariant
            .as_object()
            .into_iter()
            .flat_map(|fields| fields.keys())
            .filter(|key| current[key.as_str()] != invariant[key.as_str()])
            .collect();
        return Err(format!(
            "Keyboard stage {ordinal} state mismatch: {}",
            json!({
                "invariantFields":fields,"overlayOpen":shell.overlay.is_some(),
                "expectedRoot":expected_root,"actualRoot":shell.thread_root,
                "expectedChannelText":channel,"actualChannelText":shell.core.composer.text,
                "expectedThreadText":thread,"actualThreadText":shell.core.thread_composer.text
            })
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyboard_invariant_tracks_full_history_not_mounted_window() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        let before = unchanged(&shell);
        assert!(shell.channel_rows.len() > 1);
        shell.timeline_start = shell.channel_rows.len() - 1;
        shell.timeline_end = shell.channel_rows.len();
        shell.reproject_channel_window();
        assert_eq!(shell.messages.len(), 1);
        assert_eq!(unchanged(&shell), before);
        shell.channel_rows[0].pending = !shell.channel_rows[0].pending;
        assert_ne!(unchanged(&shell), before);
    }
}

async fn wait_ack(directory: &Path, ordinal: usize, kind: &str) -> Result<(), String> {
    let path = directory.join(format!("{ordinal}.{kind}.ack"));
    for _ in 0..600 {
        let path = path.clone();
        let value = tokio::task::spawn_blocking(move || std::fs::read(path))
            .await
            .map_err(|_| "Keyboard acknowledgment worker failed")?;
        match value {
            Ok(bytes) if bytes == b"complete\n" => return Ok(()),
            Ok(_) => return Err("Invalid keyboard acknowledgment".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("Keyboard acknowledgment read failed".into()),
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err("Keyboard acknowledgment timed out".into())
}
