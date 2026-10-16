//! Named disconnected actual link controls and existing prepared forwarding builder.
use super::*;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, Ordering};
static STARTED: AtomicBool = AtomicBool::new(false);
const NAME: &str = "forward-links";
const STAGES: [&str; 9] = [
    "initial",
    "create-form",
    "created",
    "edit-form",
    "edited",
    "removed",
    "cancelled",
    "coded",
    "uncoded",
];
pub(super) async fn run(state: Signal<ShellState>) {
    if !crate::fixture::is_fixture()
        || !matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("forward-links" | "forward-links-narrow")
        )
        || std::env::var("SUPER_PLATINUM_RICH_LINKS_CHECK").as_deref() != Ok("1")
        || STARTED.swap(true, Ordering::Relaxed)
    {
        return;
    }
    let result = check(state)
        .await
        .unwrap_or_else(|error| json!({"ok":false,"error":error}));
    eprintln!("{NAME}-check: {result}");
}
async fn check(state: Signal<ShellState>) -> Result<Value, String> {
    if state.peek().core.session.is_some() || state.peek().core.transport.is_some() {
        return Err("Disconnected fixture required".into());
    }
    let original = state.peek().forward_target.clone();
    let messages = state
        .peek()
        .fixture_message_source()
        .ok_or("Missing fixture message source")?
        .to_vec();
    let mut eval = document::eval(include_str!("link_check.js"));
    let mut ordinal = 0;
    let mut prepared = Vec::new();
    loop {
        let value: Value = eval
            .recv()
            .await
            .map_err(|_| "Link check callback closed")?;
        if state.peek().core.session.is_some() || state.peek().core.transport.is_some() {
            return Err("Disconnected fixture required".into());
        }
        if let Some(name) = value["note"].as_str() {
            if [
                ("created", 2),
                ("edited", 4),
                ("removed", 5),
                ("coded", 7),
                ("uncoded", 8),
            ]
            .get(prepared.len())
                != Some(&(name, ordinal))
            {
                return Err("Prepared note order differs".into());
            }
            let note = read_note("fixture-forward").await;
            let mut receipt = super::check::prepared_list_note(
                &state.peek(),
                original.as_ref(),
                &messages,
                note.clone(),
                &value["blocks"],
            );
            let paragraph = note.as_ref().is_some_and(|blocks| {
                super_platinum_core::canvas::edit::text::TextDraft::from_rich_blocks(blocks)
                    .is_ok_and(|draft| draft.rich_blocks() == *blocks)
            });
            receipt["canvasParagraph"] = json!(paragraph);
            if receipt["ok"] != true || !paragraph {
                return Err("Prepared note or Canvas paragraph differs".into());
            }
            prepared.push(receipt.clone());
            eval.send(receipt).map_err(|_| "Prepared receipt failed")?;
            continue;
        }
        if let Some(stage) = value["stage"].as_str() {
            if STAGES.get(ordinal) != Some(&stage) || value["layout"]["ok"] != true {
                return Err("Stage order/layout differs".into());
            }
            capture_stage(stage, ordinal, &value).await?;
            ordinal += 1;
            eval.send("received")
                .map_err(|_| "Stage acknowledgement failed")?;
            continue;
        }
        if value["ok"] != true || ordinal != STAGES.len() || prepared.len() != 5 {
            return Err(format!("Incomplete link workflow: {value}"));
        }
        for key in [
            "create",
            "edit",
            "remove",
            "undoRedo",
            "cancel",
            "selectionRestored",
            "invalidRefused",
            "inlineCode",
            "inlineCodeRemove",
            "inlineCodeUndoRedo",
        ] {
            if value[key] != true {
                return Err(format!("Missing {key}"));
            }
        }
        let _ = eval.send("received");
        return Ok(
            json!({"ok":true,"noSession":true,"noTransport":true,"noRequests":true,"stages":ordinal,"preparedRequests":prepared.len(),"prepared":prepared,"create":true,"edit":true,"remove":true,"undoRedo":true,"cancel":true,"selectionRestored":true,"invalidRefused":true,"canvasParagraph":true,"inlineCode":true,"inlineCodeRemove":true,"inlineCodeUndoRedo":true,"layout":value["layout"]}),
        );
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
