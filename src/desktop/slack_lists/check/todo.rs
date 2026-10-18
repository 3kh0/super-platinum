//! Actual disconnected reminder/completion actions with separately held outcomes.
use super::*;
use crate::slack_lists::{completion, reminder};
mod harness;
mod model;
const REMINDER: &[&str] = &[
    "initial",
    "cancelled",
    "set-editing",
    "set-saving",
    "set-uncertain",
    "set-observed",
    "reopened",
    "same-time",
    "update-saving",
    "update-ack",
    "update-observed",
    "clear-saving",
    "clear-uncertain",
    "clear-observed",
];
const COMPLETION: &[&str] = &[
    "initial",
    "primary-saving",
    "primary-uncertain",
    "reference-ready",
    "reference-saving",
    "reference-ack",
    "completed",
    "undo-primary-saving",
    "undo-primary-ack",
    "undo-reference-ready",
    "undo-reference-saving",
    "undo-reference-uncertain",
    "unchecked",
];
pub(crate) fn enabled() -> bool {
    std::env::var("SUPER_PLATINUM_LIST_TODO_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("slack-list-todo-reminder"
                | "slack-list-todo-reminder-narrow"
                | "slack-list-todo-completion"
                | "slack-list-todo-completion-narrow")
        )
}
fn mode() -> &'static str {
    if std::env::var("SUPER_PLATINUM_FIXTURE")
        .is_ok_and(|v| v.starts_with("slack-list-todo-reminder"))
    {
        "reminder"
    } else {
        "completion"
    }
}
fn stages() -> &'static [&'static str] {
    if mode() == "reminder" {
        REMINDER
    } else {
        COMPLETION
    }
}
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = match run(state, &desktop).await {
            Ok(v) => v,
            Err(error) => {
                json!({"ok":false,"error":error,"failedModel":model::snapshot(state).ok()})
            }
        };
        tokio::task::spawn_blocking(move || {
            use std::io::Write;
            std::io::stderr()
                .lock()
                .write_all(format!("slack-list-todo-check: {result}\n").as_bytes())
                .expect("To-do terminal output");
        })
        .await
        .expect("To-do writer");
    });
}
async fn action(
    d: &dioxus::desktop::DesktopContext,
    expression: &str,
    actions: &mut Vec<Value>,
) -> Result<Value, String> {
    let value = checked(d, expression).await?;
    actions.push(json!({"expression":expression,"receipt":value}));
    Ok(value)
}
async fn click(
    d: &dioxus::desktop::DesktopContext,
    id: &str,
    actions: &mut Vec<Value>,
) -> Result<Value, String> {
    action(
        d,
        &format!("window.__listTodo.click({})", json!(format!("#{id}"))),
        actions,
    )
    .await
}
async fn status(
    d: &dioxus::desktop::DesktopContext,
    text: &str,
    actions: &mut Vec<Value>,
) -> Result<Value, String> {
    action(
        d,
        &format!("window.__listTodo.statusAction({})", json!(text)),
        actions,
    )
    .await
}
async fn wait(
    state: Signal<ShellState>,
    forms: usize,
    phase: Option<&str>,
    modal: bool,
) -> Result<(), String> {
    for _ in 0..150 {
        let v = model::snapshot(state)?;
        let l = &v["ledger"];
        let count = l["forms"].as_array().ok_or("Missing forms")?.len();
        if count > forms {
            return Err("Unexpected additional To-do write".into());
        }
        let entries = if mode() == "reminder" {
            &l["receipts"]
        } else {
            &l["entries"]
        };
        let phase_ok = match phase {
            Some(want) => entries
                .as_array()
                .is_some_and(|v| v.len() == 1 && v[0]["phase"] == want),
            None => entries == &json!([]),
        };
        if count == forms
            && phase_ok
            && (mode() != "reminder" || l["presentation"].is_object() == modal)
        {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err(format!(
        "To-do state did not reach forms={forms} phase={phase:?} modal={modal}"
    ))
}
async fn stage(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    name: &str,
    proof: &mut Vec<Value>,
) -> Result<(), String> {
    if stages().get(proof.len()) != Some(&name) {
        return Err("To-do stage order changed".into());
    }
    let before = model::snapshot(state)?;
    // The helper observes actual focus/geometry before any capture reveal.
    let geometry = wait_dom(d, &format!("window.__listTodo.snapshot({})", json!(mode()))).await?;
    if model::snapshot(state)? != before {
        return Err("To-do changed during geometry witness".into());
    }
    model::bind_geometry(&before, &geometry)?;
    let capture = harness::capture(d, name).await?;
    if model::snapshot(state)? != before {
        return Err("To-do changed during native capture".into());
    }
    proof.push(json!({"name":name,"model":before,"geometry":geometry,"capture":capture}));
    Ok(())
}
async fn ready(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let mut previous = None;
    for _ in 0..150 {
        let current = model::snapshot(state)?;
        if previous.as_ref() == Some(&current) {
            let dom = wait_dom(d, &format!("window.__listTodo.ready({})", json!(mode()))).await?;
            if model::snapshot(state)? == current {
                return Ok(json!({"model":current,"dom":dom,"stableSamples":2}));
            }
        }
        previous = Some(current);
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err("To-do initial normalization did not stabilize".into())
}
async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let owner = harness::wait_runner(state).await?;
    checked(d, include_str!("todo/dom.js")).await?;
    let setup = ready(state, d).await?;
    model::initial(&setup["model"])?;
    let mut actions = vec![];
    let mut proof = vec![];
    let mut due = vec![];
    if mode() == "reminder" {
        // Initial static modal has no user opener. Establish the real detail-button
        // lifecycle before judging Cancel return; these zero-write setup actions
        // and the extra actual draft read stay in the terminal receipt.
        click(d, "slack-list-reminder-close", &mut actions).await?;
        wait(state, 0, None, false).await?;
        click(d, "slack-list-record-reminder", &mut actions).await?;
        wait(state, 0, None, true).await?;
        stage(state, d, "initial", &mut proof).await?;
        let fill = action(
            d,
            "window.__listTodo.fill('2030-03-12T11:42')",
            &mut actions,
        )
        .await?;
        due.push(fill["seconds"].as_i64().ok_or("Local reminder seconds")?);
        click(d, "slack-list-reminder-close", &mut actions).await?;
        wait(state, 0, None, false).await?;
        stage(state, d, "cancelled", &mut proof).await?;
        click(d, "slack-list-record-reminder", &mut actions).await?;
        wait(state, 0, None, true).await?;
        action(
            d,
            "window.__listTodo.fill('2030-03-12T11:42')",
            &mut actions,
        )
        .await?;
        stage(state, d, "set-editing", &mut proof).await?;
        click(d, "slack-list-reminder-save", &mut actions).await?;
        wait(state, 1, Some("Sending"), true).await?;
        stage(state, d, "set-saving", &mut proof).await?;
        reminder::fixture::complete(&mut state.write(), true, true);
        wait(state, 1, Some("Uncertain"), true).await?;
        stage(state, d, "set-uncertain", &mut proof).await?;
        click(d, "slack-list-reminder-check", &mut actions).await?;
        wait(state, 1, None, false).await?;
        stage(state, d, "set-observed", &mut proof).await?;
        click(d, "slack-list-record-reminder", &mut actions).await?;
        wait(state, 1, None, true).await?;
        stage(state, d, "reopened", &mut proof).await?;
        click(d, "slack-list-reminder-save", &mut actions).await?;
        wait(state, 1, None, true).await?;
        // Wait for the actual native datetime eval reply/error, not a one-frame
        // no-write observation that could race a late dispatch.
        wait_dom(d,"({ok:!!document.querySelector('#slack-list-reminder-editor [role=status]')?.textContent.includes('already has that time')})").await?;
        stage(state, d, "same-time", &mut proof).await?;
        let fill = action(
            d,
            "window.__listTodo.fill('2030-03-13T11:42')",
            &mut actions,
        )
        .await?;
        due.push(
            fill["seconds"]
                .as_i64()
                .ok_or("Updated local reminder seconds")?,
        );
        click(d, "slack-list-reminder-save", &mut actions).await?;
        wait(state, 2, Some("Sending"), true).await?;
        stage(state, d, "update-saving", &mut proof).await?;
        if !reminder::fixture::complete(&mut state.write(), false, true) {
            return Err("Reminder acknowledgement not accepted".into());
        }
        stage(state, d, "update-ack", &mut proof).await?;
        click(d, "slack-list-reminder-check", &mut actions).await?;
        wait(state, 2, None, false).await?;
        stage(state, d, "update-observed", &mut proof).await?;
        click(d, "slack-list-record-reminder", &mut actions).await?;
        wait(state, 2, None, true).await?;
        click(d, "slack-list-reminder-clear", &mut actions).await?;
        wait(state, 3, Some("Sending"), true).await?;
        stage(state, d, "clear-saving", &mut proof).await?;
        reminder::fixture::complete(&mut state.write(), true, true);
        stage(state, d, "clear-uncertain", &mut proof).await?;
        click(d, "slack-list-reminder-check", &mut actions).await?;
        wait(state, 3, None, false).await?;
        stage(state, d, "clear-observed", &mut proof).await?;
    } else {
        stage(state, d, "initial", &mut proof).await?;
        for index in 0..2 {
            action(d,"window.__listTodo.click('#slack-list-record-detail [data-column-id=\"C_TODO_DONE\"] input[type=\"checkbox\"]')",&mut actions).await?;
            wait(state, index * 2 + 1, Some("PrimarySending"), false).await?;
            stage(
                state,
                d,
                if index == 0 {
                    "primary-saving"
                } else {
                    "undo-primary-saving"
                },
                &mut proof,
            )
            .await?;
            completion::fixture::complete(&mut state.write(), index == 0, true);
            stage(
                state,
                d,
                if index == 0 {
                    "primary-uncertain"
                } else {
                    "undo-primary-ack"
                },
                &mut proof,
            )
            .await?;
            status(d, "Check result", &mut actions).await?;
            wait(state, index * 2 + 1, Some("ReferenceReady"), false).await?;
            stage(
                state,
                d,
                if index == 0 {
                    "reference-ready"
                } else {
                    "undo-reference-ready"
                },
                &mut proof,
            )
            .await?;
            status(d, "Update referenced item", &mut actions).await?;
            wait(state, index * 2 + 2, Some("ReferenceSending"), false).await?;
            stage(
                state,
                d,
                if index == 0 {
                    "reference-saving"
                } else {
                    "undo-reference-saving"
                },
                &mut proof,
            )
            .await?;
            completion::fixture::complete(&mut state.write(), index == 1, true);
            stage(
                state,
                d,
                if index == 0 {
                    "reference-ack"
                } else {
                    "undo-reference-uncertain"
                },
                &mut proof,
            )
            .await?;
            status(d, "Check result", &mut actions).await?;
            wait(state, index * 2 + 2, None, false).await?;
            stage(
                state,
                d,
                if index == 0 { "completed" } else { "unchecked" },
                &mut proof,
            )
            .await?;
        }
    }
    let final_model = model::snapshot(state)?;
    model::validate(&setup["model"], &proof, &final_model, &due)?;
    if proof.len() != stages().len() {
        return Err("Missing To-do captures".into());
    }
    Ok(
        json!({"ok":true,"schema":1,"fixture":std::env::var("SUPER_PLATINUM_FIXTURE").unwrap(),"mode":mode(),"owner":owner,"setup":setup,"actions":actions,"due":due,"stages":proof,"final":final_model,"trustedKeyboard":false,"noSession":true,"noTransport":true}),
    )
}
