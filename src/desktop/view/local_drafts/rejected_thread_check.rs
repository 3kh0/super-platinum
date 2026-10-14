//! Two-process native proof uses a runner-owned DB, never a real draft account.
use crate::{
    drafts::{rejected_thread_fixture as fixture, settlement_fixture},
    state::ShellState,
};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

pub(super) async fn run(state: Signal<ShellState>) {
    if !matches!(
        std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
        Ok("rejected-thread-return" | "rejected-thread-return-narrow")
    ) || std::env::var("SUPER_PLATINUM_REJECTED_THREAD_CHECK").as_deref() != Ok("1")
    {
        return;
    }
    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::Relaxed) {
        return;
    }
    let phase = std::env::var("SUPER_PLATINUM_REJECTED_THREAD_PHASE").unwrap_or_default();
    let result = tokio::time::timeout(Duration::from_secs(55), check(state, &phase)).await;
    let receipt = match result {
        Ok(Ok(receipt)) => receipt,
        Ok(Err(error)) => json!({"ok":false,"error":error,"phase":phase}),
        Err(_) => json!({"ok":false,"error":"Native thread return check timed out","phase":phase}),
    };
    eprintln!("rejected-thread-check: {receipt}");
}

async fn check(state: Signal<ShellState>, phase: &str) -> Result<Value, String> {
    if !matches!(phase, "restore" | "restart")
        || state.peek().core.session.is_some()
        || state.peek().core.transport.is_some()
    {
        return Err("Explicit disconnected restore/restart fixture required".into());
    }
    // A second process must find the first process's database; it cannot seed a
    // fresh file and label an in-memory return as restart evidence.
    let path = std::env::var_os("SUPER_PLATINUM_DRAFT_SETTLEMENT_DB")
        .ok_or("Missing isolated database")?;
    let exists = tokio::task::spawn_blocking(move || std::path::PathBuf::from(path).is_file())
        .await
        .map_err(|_| "Database existence check failed")?;
    if exists != (phase == "restart") {
        return Err("Database freshness does not match fixture phase".into());
    }
    settlement_fixture::seed(state)
        .await
        .map_err(|fault| format!("Isolated seed/load: {fault:?}"))?;
    if phase == "restart" {
        let readback = fixture::verify()
            .await
            .map_err(|fault| format!("Restart readback: {fault:?}"))?;
        if readback["ok"] != true {
            return Err(
                "Restart did not load restored thread/broadcast and untouched uncertainty".into(),
            );
        }
    }
    let script = format!(
        "const phase={};\n{}",
        serde_json::to_string(phase).unwrap(),
        include_str!("rejected_thread_check.js")
    );
    let mut eval = document::eval(&script);
    let mut edited = false;
    let mut receipt = loop {
        let value = eval
            .recv::<Value>()
            .await
            .map_err(|error| format!("Native receipt: {error:?}"))?;
        if value["ok"] != true {
            return Err(value.to_string());
        }
        let Some(stage) = value["stage"].as_str() else {
            break value;
        };
        if stage == "edited" {
            for _ in 0..50 {
                if state.peek().core.thread_composer.text
                    == format!("{} Edited locally.", fixture::TEXT)
                {
                    edited = true;
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            if !edited {
                return Err("Native input did not reach the thread composer".into());
            }
        } else {
            eprintln!(
                "rejected-thread-stage: {}",
                json!({"ok":true,"phase":phase,"stage":stage})
            );
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        eval.send(())
            .map_err(|error| format!("Native continuation: {error:?}"))?;
    };
    for _ in 0..50 {
        if fixture::exact_composer(&state.peek()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let mut durable = json!({"ok":false});
    for _ in 0..50 {
        durable = fixture::verify()
            .await
            .map_err(|fault| format!("Durable readback: {fault:?}"))?;
        if durable["ok"] == true {
            break;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    let shell = state.peek();
    let exact = fixture::exact_composer(&shell);
    let failed_removed = !shell.pending_delivery.contains_key(fixture::FAILED_TS)
        && shell
            .core
            .threads
            .get(&("T1".into(), "C2".into(), fixture::ROOT.into()))
            .is_some_and(|bag| {
                !bag.messages
                    .iter()
                    .any(|message| message.ts.as_deref() == Some(fixture::FAILED_TS))
            });
    let offline = shell.core.session.is_none() && shell.core.transport.is_none();
    receipt["ok"] = json!(exact && edited && offline && failed_removed && durable["ok"] == true);
    receipt["phase"] = json!(phase);
    receipt["pid"] = json!(std::process::id());
    receipt["threadKey"] = json!(fixture::key(fixture::ROOT));
    receipt["exactThreadComposer"] = json!(exact);
    receipt["channelDraftPreserved"] = json!(shell.core.composer.text == fixture::CHANNEL_TEXT);
    receipt["inputEventApplied"] = json!(edited);
    receipt["failedThreadRowRemoved"] = json!(failed_removed);
    receipt["noTransport"] = json!(shell.core.transport.is_none());
    receipt["noSession"] = json!(shell.core.session.is_none());
    receipt["preparedRequests"] = json!(0);
    receipt["durable"] = durable;
    let _ = eval.send(());
    Ok(receipt)
}
