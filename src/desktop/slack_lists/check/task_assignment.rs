//! Actual assignee editing and permission continuation. No seeded grant result.
use super::*;
use crate::slack_lists::editor::assignment::fixture::dynamic as io;
mod harness;
mod model;
const STAGES: &[&str] = &[
    "initial",
    "editing",
    "no-op",
    "query",
    "selected",
    "native-tab",
    "cancelled",
    "not-now-review",
    "not-now-saving",
    "not-now-uncertain",
    "not-now-observed",
    "clear-editing",
    "clear-saving",
    "clear-ack",
    "clear-observed",
    "share-review",
    "sharing",
    "share-uncertain",
    "share-assignment-saving",
    "share-assignment-ack",
    "share-observed",
    "detached-review",
    "detached-sharing",
    "detached-pending",
    "detached-unconfirmed",
    "abandoned",
    "retained-review",
    "final",
];
pub(super) fn enabled() -> bool {
    io::requested()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("slack-list-task-assignment" | "slack-list-task-assignment-narrow")
        )
}
fn snapshot(state: Signal<ShellState>) -> Result<Value, String> {
    io::diagnostic(&state.peek())
}
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = match run(state, &desktop).await {
            Ok(v) => v,
            Err(error) => json!({"ok":false,"error":error,"failedModel":snapshot(state).ok()}),
        };
        tokio::task::spawn_blocking(move || {
            use std::io::Write;
            std::io::stderr()
                .lock()
                .write_all(format!("slack-list-task-assignment-check: {result}\n").as_bytes())
                .expect("Assignment receipt");
        })
        .await
        .expect("Assignment writer");
    });
}
async fn wait(
    state: Signal<ShellState>,
    condition: impl Fn(&Value) -> bool,
) -> Result<Value, String> {
    for _ in 0..200 {
        let v = snapshot(state)?;
        if condition(&v) {
            return Ok(v);
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err("Assignment state deadline expired".into())
}
async fn action(
    d: &dioxus::desktop::DesktopContext,
    expression: &str,
    actions: &mut Vec<Value>,
) -> Result<Value, String> {
    let v = checked(d, expression).await?;
    actions.push(json!({"expression":expression,"receipt":v}));
    Ok(v)
}
async fn click(
    d: &dioxus::desktop::DesktopContext,
    id: &str,
    actions: &mut Vec<Value>,
) -> Result<Value, String> {
    action(
        d,
        &format!("window.__listAssignment.click({})", json!(format!("#{id}"))),
        actions,
    )
    .await
}
async fn open(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    actions: &mut Vec<Value>,
) -> Result<(), String> {
    action(d, "window.__listAssignment.open()", actions).await?;
    wait(state, |v| {
        v["editor"].is_object()
            && v["entry"].is_null()
            && !v["editor"]["disabled"].as_bool().unwrap_or(true)
    })
    .await?;
    Ok(())
}
async fn toggle(
    d: &dioxus::desktop::DesktopContext,
    id: &str,
    actions: &mut Vec<Value>,
) -> Result<(), String> {
    action(
        d,
        &format!("window.__listAssignment.toggle({})", json!(id)),
        actions,
    )
    .await?;
    wait_dom(
        d,
        &format!("({{ok:document.querySelector('[data-selected-user=\"{id}\"]')!==null}})"),
    )
    .await?;
    Ok(())
}
async fn stage(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    name: &str,
    focus: &[&str],
    proof: &mut Vec<Value>,
) -> Result<(), String> {
    if STAGES.get(proof.len()) != Some(&name) {
        return Err("Unexpected assignment stage order".into());
    }
    let before = snapshot(state)?;
    let geometry = wait_dom(
        d,
        &format!("window.__listAssignment.snapshot({})", json!(focus)),
    )
    .await?;
    if snapshot(state)? != before {
        return Err("Assignment changed during geometry witness".into());
    }
    model::bind(&before, &geometry)?;
    let capture = harness::capture(d, name).await?;
    if snapshot(state)? != before {
        return Err("Assignment changed during capture".into());
    }
    proof.push(json!({"name":name,"model":before,"geometry":geometry,"capture":capture}));
    Ok(())
}
async fn stable(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let mut prior = None;
    for _ in 0..200 {
        let m = snapshot(state)?;
        if prior.as_ref() == Some(&m) {
            let dom = wait_dom(d, "window.__listAssignment.ready()").await?;
            if snapshot(state)? == m {
                return Ok(json!({"model":m,"dom":dom,"stableSamples":2}));
            }
        }
        prior = Some(m);
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err("Assignment setup did not stabilize".into())
}
async fn review(state: Signal<ShellState>) -> Result<(), String> {
    wait(state, |v| {
        v["entry"]["stage"] == "Review" && v["entry"]["job"].is_null()
    })
    .await?;
    Ok(())
}
async fn pending_share(state: Signal<ShellState>) -> Result<Value, String> {
    wait(state, |v| v["ledger"]["pending"]["method"] == "files.share").await
}
fn release(
    mut state: Signal<ShellState>,
    pending: &Value,
    lost: bool,
    apply: bool,
) -> Result<(), String> {
    let p = &pending["ledger"]["pending"];
    io::release(
        &state.peek(),
        p["serial"].as_u64().ok_or("Held serial")?,
        p["job"].as_u64().ok_or("Held job")?,
        lost,
        apply,
    )?;
    let _ = state.write();
    Ok(())
}
async fn cell(state: Signal<ShellState>, count: usize) -> Result<(), String> {
    wait(state, |v| {
        v["ledger"]["cells"].as_array().map(Vec::len) == Some(count)
            && v["mutation"]["phase"] == "Saving"
    })
    .await?;
    Ok(())
}
async fn observed(state: Signal<ShellState>) -> Result<(), String> {
    wait(state, |v| {
        v["mutation"].is_null() && v["editor"].is_null() && v["population"] == v["backend"]
    })
    .await?;
    Ok(())
}
async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let owner = harness::wait_runner(state).await?;
    io::install(&mut state.write())?;
    checked(d, include_str!("task_assignment/dom.js")).await?;
    let mut actions = vec![];
    let mut proof = vec![];
    let static_setup = stable(state, d).await?;
    // Close the intentionally static review. Every subsequent review is produced
    // by actual Save and fresh access requests through the original worker.
    click(d, "slack-list-editor-cancel", &mut actions).await?;
    observed(state).await?;
    open(state, d, &mut actions).await?;
    click(d, "slack-list-editor-cancel", &mut actions).await?;
    observed(state).await?;
    let setup = stable(state, d).await?;
    let table = [
        "[data-row-id=\"R_TASK\"] [data-column-id=\"C_TASK\"]",
        "#slack-list-refresh",
    ];
    let cancel = ["#slack-list-editor-cancel"];
    stage(state, d, "initial", &table, &mut proof).await?;
    open(state, d, &mut actions).await?;
    stage(state, d, "editing", &["#slack-list-user-query"], &mut proof).await?;
    click(d, "slack-list-editor-save", &mut actions).await?;
    wait(state, |v| {
        v["editor"]["error"].is_string() && v["entry"].is_null()
    })
    .await?;
    stage(
        state,
        d,
        "no-op",
        &["#slack-list-editor-save", "#slack-list-editor-cancel"],
        &mut proof,
    )
    .await?;
    action(d, "window.__listAssignment.query('Bob')", &mut actions).await?;
    wait(state, |v| v["editor"]["query"] == "Bob").await?;
    stage(state, d, "query", &["#slack-list-user-query"], &mut proof).await?;
    toggle(d, "U00000002", &mut actions).await?;
    action(d, "window.__listAssignment.query('')", &mut actions).await?;
    wait(state, |v| v["editor"]["query"] == "").await?;
    stage(
        state,
        d,
        "selected",
        &["#slack-list-user-query"],
        &mut proof,
    )
    .await?;
    let keyboard_before =
        action(d, "window.__listAssignment.keyboardBefore()", &mut actions).await?;
    let keyboard_ack = harness::keyboard().await?;
    let keyboard_after = wait_dom(d, "window.__listAssignment.keyboardAfter()").await?;
    stage(
        state,
        d,
        "native-tab",
        &["#slack-list-editor-save"],
        &mut proof,
    )
    .await?;
    click(d, "slack-list-editor-cancel", &mut actions).await?;
    observed(state).await?;
    stage(state, d, "cancelled", &table, &mut proof).await?;
    open(state, d, &mut actions).await?;
    toggle(d, "U00000002", &mut actions).await?;
    toggle(d, "W00000003", &mut actions).await?;
    click(d, "slack-list-editor-save", &mut actions).await?;
    review(state).await?;
    action(
        d,
        "window.__listAssignment.grant('U00000002','read')",
        &mut actions,
    )
    .await?;
    wait(state, |v| v["entry"]["people"][0]["grant"] == "read").await?;
    stage(
        state,
        d,
        "not-now-review",
        &["[data-assignment-user=\"U00000002\"]"],
        &mut proof,
    )
    .await?;
    click(d, "slack-list-assignment-not-now", &mut actions).await?;
    cell(state, 1).await?;
    stage(state, d, "not-now-saving", &cancel, &mut proof).await?;
    io::complete_cell(&mut state.write(), true)?;
    stage(state, d, "not-now-uncertain", &cancel, &mut proof).await?;
    click(d, "slack-list-editor-refresh", &mut actions).await?;
    observed(state).await?;
    stage(state, d, "not-now-observed", &table, &mut proof).await?;
    open(state, d, &mut actions).await?;
    for id in ["U00000001", "U00000002", "W00000003"] {
        action(
            d,
            &format!("window.__listAssignment.remove({})", json!(id)),
            &mut actions,
        )
        .await?;
        wait_dom(
            d,
            &format!("({{ok:document.querySelector('[data-selected-user=\"{id}\"]')===null}})"),
        )
        .await?;
    }
    // The removed chip cannot retain focus; product's mounted editor fallback is observed.
    stage(
        state,
        d,
        "clear-editing",
        &["#slack-list-user-query", "#slack-list-editor-cancel"],
        &mut proof,
    )
    .await?;
    click(d, "slack-list-editor-save", &mut actions).await?;
    cell(state, 2).await?;
    stage(state, d, "clear-saving", &cancel, &mut proof).await?;
    io::complete_cell(&mut state.write(), false)?;
    stage(state, d, "clear-ack", &cancel, &mut proof).await?;
    click(d, "slack-list-editor-refresh", &mut actions).await?;
    observed(state).await?;
    stage(state, d, "clear-observed", &table, &mut proof).await?;
    open(state, d, &mut actions).await?;
    toggle(d, "U00000002", &mut actions).await?;
    click(d, "slack-list-editor-save", &mut actions).await?;
    review(state).await?;
    action(
        d,
        "window.__listAssignment.grant('U00000002','read')",
        &mut actions,
    )
    .await?;
    wait(state, |v| v["entry"]["people"][0]["grant"] == "read").await?;
    stage(
        state,
        d,
        "share-review",
        &["[data-assignment-user=\"U00000002\"]"],
        &mut proof,
    )
    .await?;
    click(d, "slack-list-assignment-share", &mut actions).await?;
    let held = pending_share(state).await?;
    stage(state, d, "sharing", &cancel, &mut proof).await?;
    release(state, &held, true, true)?;
    wait(state, |v| {
        v["entry"]["stage"] == "Uncertain" && v["entry"]["job"].is_null()
    })
    .await?;
    stage(state, d, "share-uncertain", &cancel, &mut proof).await?;
    click(d, "slack-list-assignment-check", &mut actions).await?;
    cell(state, 3).await?;
    stage(state, d, "share-assignment-saving", &cancel, &mut proof).await?;
    io::complete_cell(&mut state.write(), false)?;
    stage(state, d, "share-assignment-ack", &cancel, &mut proof).await?;
    click(d, "slack-list-editor-refresh", &mut actions).await?;
    observed(state).await?;
    stage(state, d, "share-observed", &table, &mut proof).await?;
    open(state, d, &mut actions).await?;
    toggle(d, "W00000003", &mut actions).await?;
    click(d, "slack-list-editor-save", &mut actions).await?;
    review(state).await?;
    stage(state, d, "detached-review", &cancel, &mut proof).await?;
    io::hold_next(&state.peek(), "files.share")?;
    click(d, "slack-list-assignment-share", &mut actions).await?;
    let held = pending_share(state).await?;
    stage(state, d, "detached-sharing", &cancel, &mut proof).await?;
    click(d, "slack-list-editor-cancel", &mut actions).await?;
    wait(state, |v| {
        v["editor"].is_null() && v["recovery"]["canAbandon"] == false
    })
    .await?;
    stage(state, d, "detached-pending", &table, &mut proof).await?;
    release(state, &held, true, false)?;
    wait(state, |v| {
        v["entry"]["job"].is_null() && v["recovery"]["canAbandon"] == true
    })
    .await?;
    stage(state, d, "detached-unconfirmed", &table, &mut proof).await?;
    click(d, "slack-list-assignment-abandon", &mut actions).await?;
    observed(state).await?;
    stage(state, d, "abandoned", &["#slack-list-refresh"], &mut proof).await?;
    click(d, "slack-list-refresh", &mut actions).await?;
    observed(state).await?;
    open(state, d, &mut actions).await?;
    toggle(d, "W00000003", &mut actions).await?;
    click(d, "slack-list-editor-save", &mut actions).await?;
    wait(state, |v| {
        v["entry"]["stage"] == "Uncertain" && v["entry"]["job"].is_null()
    })
    .await?;
    stage(state, d, "retained-review", &cancel, &mut proof).await?;
    click(d, "slack-list-assignment-abandon", &mut actions).await?;
    observed(state).await?;
    stage(state, d, "final", &table, &mut proof).await?;
    let final_model = snapshot(state)?;
    model::validate(&setup["model"], &proof, &final_model)?;
    Ok(
        json!({"ok":true,"schema":1,"owner":owner,"fixture":std::env::var("SUPER_PLATINUM_FIXTURE").unwrap(),"staticSetup":static_setup,"setup":setup,"actions":actions,"keyboard":{"before":keyboard_before,"ack":keyboard_ack,"after":keyboard_after},"stages":proof,"final":final_model}),
    )
}
