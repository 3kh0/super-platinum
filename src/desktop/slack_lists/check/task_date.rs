//! Actual date editor actions, two retained cell writes and one trusted Tab.
use super::*;
use crate::slack_lists::fixture::task_date as fixture;
mod harness;
mod model;
const STAGES: &[&str] = &[
    "table",
    "editing",
    "native-tab-save",
    "cancelled",
    "update-editing",
    "update-saving",
    "update-detached",
    "update-uncertain",
    "update-observed",
    "clear-editing",
    "clear-saving",
    "clear-observed",
    "policy-refused",
];
pub(crate) fn enabled() -> bool {
    std::env::var("SUPER_PLATINUM_LIST_TASK_DATE_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("slack-list-task-date" | "slack-list-task-date-narrow")
        )
}
fn stages() -> &'static [&'static str] {
    STAGES
}
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = match run(state, &desktop).await {
            Ok(v) => v,
            Err(e) => {
                json!({"ok":false,"error":e,"failedModel":fixture::snapshot(&state.peek()).ok()})
            }
        };
        tokio::task::spawn_blocking(move || {
            use std::io::Write;
            std::io::stderr()
                .lock()
                .write_all(format!("slack-list-task-date-check: {result}\n").as_bytes())
                .expect("date receipt");
        })
        .await
        .expect("date writer");
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
) -> Result<(), String> {
    action(
        d,
        &format!("window.__taskDate.click({})", json!(format!("#{id}"))),
        actions,
    )
    .await?;
    Ok(())
}
async fn wait(
    state: Signal<ShellState>,
    count: usize,
    phase: Option<&str>,
    editor: bool,
) -> Result<Value, String> {
    for _ in 0..150 {
        let m = fixture::snapshot(&state.peek())?;
        let n = m["forms"].as_array().ok_or("Date forms shape")?.len();
        if n > count {
            return Err("Unexpected additional date write".into());
        }
        if n == count
            && m["editor"].is_object() == editor
            && m["mutation"]["phase"].as_str() == phase
        {
            return Ok(m);
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err(format!(
        "Date state did not reach {count}/{phase:?}/{editor}"
    ))
}
async fn stage(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    name: &str,
    initial: &Value,
    proof: &mut Vec<Value>,
) -> Result<(), String> {
    if STAGES.get(proof.len()) != Some(&name) {
        return Err("Date stage order".into());
    }
    let before = fixture::snapshot(&state.peek())?;
    model::stage(initial, &before, name)?;
    let prior = proof.last().map(|stage| &stage["model"]).unwrap_or(initial);
    model::continued(prior, &before)?;
    let geometry = wait_dom(d, "window.__taskDate.snapshot()").await?;
    model::bind(&before, &geometry)?;
    if fixture::snapshot(&state.peek())? != before {
        return Err("Date changed during geometry".into());
    }
    let capture = harness::capture(d, name).await?;
    if fixture::snapshot(&state.peek())? != before {
        return Err("Date changed during capture".into());
    }
    if checked(d, "window.__taskDate.snapshot()").await? != geometry {
        return Err("Date DOM/focus changed during capture".into());
    }
    proof.push(json!({"name":name,"model":before,"geometry":geometry,"capture":capture}));
    Ok(())
}
async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let owner = harness::wait_runner(state).await?;
    checked(d, include_str!("task_date/dom.js")).await?;
    wait_dom(d, "window.__taskDate.ready()").await?;
    let mut actions = vec![];
    let mut proof = vec![];
    action(d, "window.__taskDate.choose('V_TASK_DATE')", &mut actions).await?;
    wait_dom(d, "window.__taskDate.ready('V_TASK_DATE')").await?;
    action(d, "window.__taskDate.find('Review')", &mut actions).await?;
    let mut stable = None;
    for _ in 0..150 {
        let before = fixture::snapshot(&state.peek())?;
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
        if before["find"]["applied"] == "Review" && fixture::snapshot(&state.peek())? == before {
            stable = Some(before);
            break;
        }
    }
    let initial = stable.ok_or("Date setup normalization timed out")?;
    model::initial(&initial)?;
    action(d, "window.__taskDate.rememberCell()", &mut actions).await?;
    stage(state, d, "table", &initial, &mut proof).await?;
    action(d, "window.__taskDate.open()", &mut actions).await?;
    wait(state, 0, None, true).await?;
    stage(state, d, "editing", &initial, &mut proof).await?;
    let admitted = action(d, "window.__taskDate.keyboardBefore()", &mut actions).await?;
    let keyboard_owner = harness::keyboard().await?;
    let keyboard = wait_dom(d, "window.__taskDate.keyboardAfter()").await?;
    stage(state, d, "native-tab-save", &initial, &mut proof).await?;
    click(d, "slack-list-editor-cancel", &mut actions).await?;
    wait(state, 0, None, false).await?;
    wait_dom(d, "window.__taskDate.returnedCell()").await?;
    stage(state, d, "cancelled", &initial, &mut proof).await?;
    action(d, "window.__taskDate.open()", &mut actions).await?;
    wait(state, 0, None, true).await?;
    action(d, "window.__taskDate.fill('2026-10-18')", &mut actions).await?;
    stage(state, d, "update-editing", &initial, &mut proof).await?;
    click(d, "slack-list-editor-save", &mut actions).await?;
    wait(state, 1, Some("Saving"), true).await?;
    stage(state, d, "update-saving", &initial, &mut proof).await?;
    click(d, "slack-list-editor-cancel", &mut actions).await?;
    wait(state, 1, Some("Saving"), false).await?;
    stage(state, d, "update-detached", &initial, &mut proof).await?;
    fixture::complete(&mut state.write(), true)?;
    wait(state, 1, Some("Uncertain"), false).await?;
    stage(state, d, "update-uncertain", &initial, &mut proof).await?;
    click(d, "slack-list-refresh", &mut actions).await?;
    wait(state, 1, None, false).await?;
    stage(state, d, "update-observed", &initial, &mut proof).await?;
    action(d, "window.__taskDate.open()", &mut actions).await?;
    wait(state, 1, None, true).await?;
    click(d, "slack-list-date-clear", &mut actions).await?;
    wait_dom(d, "window.__taskDate.value('')").await?;
    stage(state, d, "clear-editing", &initial, &mut proof).await?;
    click(d, "slack-list-editor-save", &mut actions).await?;
    wait(state, 2, Some("Saving"), true).await?;
    stage(state, d, "clear-saving", &initial, &mut proof).await?;
    fixture::complete(&mut state.write(), false)?;
    wait(state, 2, Some("Uncertain"), true).await?;
    click(d, "slack-list-editor-cancel", &mut actions).await?;
    click(d, "slack-list-refresh", &mut actions).await?;
    wait(state, 2, None, false).await?;
    stage(state, d, "clear-observed", &initial, &mut proof).await?;
    action(d, "window.__taskDate.open()", &mut actions).await?;
    wait(state, 2, None, true).await?;
    action(d, "window.__taskDate.fill('2026-10-19')", &mut actions).await?;
    fixture::policy_change(&mut state.write());
    wait_dom(d, "window.__taskDate.refused()").await?;
    stage(state, d, "policy-refused", &initial, &mut proof).await?;
    Ok(
        json!({"ok":true,"schema":1,"fixture":owner["fixture"],"owner":owner,"initial":initial,"actions":actions,"keyboard":{"admitted":admitted,"owner":keyboard_owner,"observed":keyboard},"stages":proof,"final":fixture::snapshot(&state.peek())?,"trustedKeyboard":true,"programmaticDateInput":true,"noSession":state.peek().core.session.is_none(),"noTransport":state.peek().core.transport.is_none()}),
    )
}
