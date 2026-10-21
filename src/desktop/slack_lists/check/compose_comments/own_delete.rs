//! Mounted own-reply deletion with independently reopened reply/delete journals.
use super::*;
use crate::slack_lists::comments::{fixture::own_delete as data, own_delete as deleting};
mod guards;
mod restart;
mod storage;
const SUCCESS: &[&str] = &[
    "draft",
    "confirming",
    "cancelled",
    "delete-held",
    "deleted",
    "readback",
    "reopened",
];
const FAILURE: &[&str] = &[
    "draft",
    "rejected",
    "unconfirmed",
    "unconfirmed-checked",
    "observed",
    "reopened",
];
fn script(action: &str, arg: Value) -> String {
    format!(
        "({})({},{},({}))",
        include_str!("own_delete.js"),
        json!(action),
        arg,
        include_str!("reaction_actions/text.js")
    )
}
async fn command(
    d: &dioxus::desktop::DesktopContext,
    action: &str,
    arg: Value,
) -> Result<Value, String> {
    checked(d, &script(action, arg)).await
}
async fn button(d: &dioxus::desktop::DesktopContext, suffix: &str) -> Result<Value, String> {
    // Controller completion precedes the WebView render. Wait for the actual
    // enabled control before testing its hit target and activating it.
    let id = format!("slack-list-comment-delete-{suffix}");
    wait_dom(
        d,
        &format!(
            "(()=>{{const n=document.getElementById({});return {{ok:!!n&&!n.disabled&&!n.closest('[inert]')}};}})()",
            json!(id)
        ),
    )
    .await?;
    command(d, "click", json!({"id":id})).await
}
fn snapshot(state: Signal<ShellState>, model: &Model) -> Value {
    json!({"comments":compose::snapshot(&state.peek(),model),"fixture":data::snapshot(&state.peek()),
        "editFixture":fixture::own_edit::snapshot(&state.peek()),"delete":deleting::snapshot(&state.peek())})
}
async fn wait_state(
    state: Signal<ShellState>,
    condition: impl Fn(&Value) -> bool,
) -> Result<Value, String> {
    for _ in 0..400 {
        let v = deleting::snapshot(&state.peek());
        if condition(&v) {
            return Ok(v);
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    Err(format!(
        "Delete controller did not settle: {}",
        deleting::snapshot(&state.peek())
    ))
}
async fn open_delete(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<(), String> {
    command(d, "open", json!({"ts":data::RECEIVED_TS})).await?;
    wait_state(state, |v| {
        v["active"]["canConfirm"] == true && v["active"]["busy"] == false
    })
    .await?;
    Ok(())
}
async fn close_delete(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<(), String> {
    button(d, "cancel").await?;
    wait_state(state, |v| v["active"].is_null()).await?;
    wait_dom(d,"({ok:!document.getElementById('slack-list-comment-delete')&&document.getElementById('slack-list-comment-draft')?.isContentEditable===true})").await?;
    Ok(())
}
async fn inventory(state: Signal<ShellState>) -> Result<Value, String> {
    if deleting::snapshot(&state.peek())["retainedCount"] == 0 {
        Ok(Value::Null)
    } else {
        deleting::journal_snapshot(state).await
    }
}
async fn capture(
    state: Signal<ShellState>,
    model: &Model,
    d: &dioxus::desktop::DesktopContext,
    name: &str,
    proof: &Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    stable_drafts(state, model).await?;
    let before = snapshot(state, model);
    let view = deleting::presentation(&state.peek());
    let reply_send_disabled = crate::slack_lists::comments::draft::presentation(&state.peek())
        .is_none_or(|v| !v.can_send || v.busy || v.locked || v.loading);
    let args = json!({"ts":data::RECEIVED_TS,"active":before["delete"]["active"],"notice":view.and_then(|v|v.notice),
        "visibleTs":before["comments"]["comments"]["visibleTs"],"replySendDisabled":reply_send_disabled});
    let geometry = wait_dom(d, &script("state", args.clone())).await?;
    // The runner's owning connection supplies lock-time inventories. Opening
    // another Journal while it holds EXCLUSIVE would mask the product failure.
    let locked = proof["storage"].is_object() && proof["storage"]["release"].is_null();
    let (journal, deletion) = if locked {
        (Value::Null, Value::Null)
    } else {
        (
            compose::journal_snapshot(state).await?,
            inventory(state).await?,
        )
    };
    if (!locked && journal["blocks"] != proof["replyBlocks"])
        || geometry["reply"]["blocks"] != proof["replyBlocks"]
        || (name != "reopened" && geometry["reply"]["sameNode"] != true)
    {
        return Err(format!("Original reply changed at {name}"));
    }
    let png = harness::capture(d, name).await?;
    let after_geometry = checked(d, &script("state", args)).await?;
    let mut after = snapshot(state, model);
    // retryAfter is derived from the real clock; the absolute retryAt and every
    // stored/controller field must remain unchanged while taking the PNG.
    if proof["cooldownDeadline"].as_u64().is_some_and(|v| v > 0) {
        after["comments"]["comments"]["retryAfter"] =
            before["comments"]["comments"]["retryAfter"].clone();
    }
    if before != after
        || (!locked
            && (journal != compose::journal_snapshot(state).await?
                || deletion != inventory(state).await?))
        || after_geometry["reply"]["blocks"] != geometry["reply"]["blocks"]
    {
        return Err(format!("Deletion source/journal changed during {name}"));
    }
    rows.push(json!({"stage":name,"model":before,"geometry":geometry,"journal":journal,"deleteInventory":deletion,"proof":proof,"capture":png}));
    Ok(())
}
async fn reopen_reply(
    mut state: Signal<ShellState>,
    model: &Model,
    d: &dioxus::desktop::DesktopContext,
) -> Result<(), String> {
    click(d, "slack-list-comments-back").await?;
    wait_dom(d, "({ok:!document.getElementById('slack-list-comments')})").await?;
    stable_drafts(state, model).await?;
    if !crate::slack_lists::comments::draft::fixture_forget_saved(&mut state.write()) {
        return Err("Saved reply cannot be forgotten".into());
    }
    click(d, "slack-list-record-comments").await?;
    wait_dom(d,"({ok:!!document.getElementById('slack-list-comment-write')&&!document.getElementById('slack-list-comments-refresh')?.disabled})").await?;
    write(d).await
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    case: &str,
) -> Result<Value, String> {
    if matches!(case, "own-delete-guards" | "own-delete-rate-limit") {
        return guards::run(state, d, case).await;
    }
    if case == "own-delete-storage" {
        return storage::run(state, d).await;
    }
    if restart::named(case) {
        return restart::run(state, d, case).await;
    }
    let expected = match case {
        "own-delete" => SUCCESS,
        "own-delete-failure" => FAILURE,
        _ => return Err("Unknown deletion case".into()),
    };
    let ready = harness::wait_runner(state).await?;
    let model = fixture::begin(&mut state.write())?;
    fixture::completion::seed(state).await?;
    let source = data::seed(state)?;
    checked(d, include_str!("../comments.js")).await?;
    checked(d, include_str!("../compose_comments.js")).await?;
    wait_dom(
        d,
        "({ok:document.querySelector('.slack-list-dialog')?.dataset.listKeyboardMounted==='true'})",
    )
    .await?;
    open(d, "R_LIST_000").await?;
    let seed = compose::seed_draft(state).await?;
    write(d).await?;
    type_text(d, "Independent reply A \u{4e2d}\u{1f642}").await?;
    settled(d).await?;
    let remembered = command(d, "remember", Value::Null).await?;
    let mut proof = json!({"source":source,"replySeed":seed,"replyBlocks":remembered["blocks"]});
    let mut rows = Vec::new();
    capture(state, &model, d, "draft", &proof, &mut rows).await?;
    open_delete(state, d).await?;
    if case == "own-delete" {
        capture(state, &model, d, "confirming", &proof, &mut rows).await?;
        close_delete(state, d).await?;
        capture(state, &model, d, "cancelled", &proof, &mut rows).await?;
        open_delete(state, d).await?;
        data::mode(&state.peek(), data::Mode::HeldSuccess)?;
        button(d, "confirm").await?;
        for _ in 0..400 {
            if data::snapshot(&state.peek())["held"] == true {
                break;
            }
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
        if data::snapshot(&state.peek())["held"] != true {
            return Err("Original delete did not reach held dispatch".into());
        }
        proof["blockedConfirm"] = command(d, "blocked-confirm", Value::Null).await?;
        capture(state, &model, d, "delete-held", &proof, &mut rows).await?;
        proof["release"] = data::release(&state.peek())?;
        wait_state(state, |v| {
            v["active"]["phase"] == "Confirmed"
                && v["active"]["busy"] == false
                && v["active"]["localPending"] == false
        })
        .await?;
        close_delete(state, d).await?;
        capture(state, &model, d, "deleted", &proof, &mut rows).await?;
        data::publish(&state.peek())?;
        click(d, "slack-list-comments-refresh").await?;
        wait_state(state, |_| {
            let current = fixture::snapshot(&state.peek(), &model);
            current["comments"]["loading"] == false
                && current["comments"]["page"]["deleted"] == json!([data::RECEIVED_TS])
        })
        .await?;
        wait_dom(
            d,
            "({ok:document.getElementById('slack-list-comments-refresh')?.disabled===false})",
        )
        .await?;
        capture(state, &model, d, "readback", &proof, &mut rows).await?;
    } else {
        data::mode(&state.peek(), data::Mode::Rejected)?;
        button(d, "confirm").await?;
        wait_state(state, |v| {
            v["active"]["phase"] == "Rejected" && v["active"]["canConfirm"] == true
        })
        .await?;
        capture(state, &model, d, "rejected", &proof, &mut rows).await?;
        data::mode(&state.peek(), data::Mode::Unconfirmed)?;
        button(d, "confirm").await?;
        wait_state(state, |v| {
            v["active"]["phase"] == "Uncertain" && v["active"]["canCheck"] == true
        })
        .await?;
        proof["blockedConfirm"] = command(d, "blocked-confirm", Value::Null).await?;
        capture(state, &model, d, "unconfirmed", &proof, &mut rows).await?;
        button(d, "check").await?;
        wait_dom(d,"({ok:document.getElementById('slack-list-comment-delete-status')?.textContent==='Deletion is still unconfirmed. Missing or changed comments are not proof; the original attempt will not be sent again.'})").await?;
        wait_state(state, |v| v["active"]["canCheck"] == true).await?;
        capture(state, &model, d, "unconfirmed-checked", &proof, &mut rows).await?;
        data::publish(&state.peek())?;
        button(d, "check").await?;
        wait_state(state, |v| {
            v["active"]["phase"] == "Observed"
                && v["active"]["busy"] == false
                && v["active"]["localPending"] == false
        })
        .await?;
        close_delete(state, d).await?;
        capture(state, &model, d, "observed", &proof, &mut rows).await?;
    }
    reopen_reply(state, &model, d).await?;
    proof["runtimeForgotten"] = json!(true);
    capture(state, &model, d, "reopened", &proof, &mut rows).await?;
    if rows
        .iter()
        .map(|r| r["stage"].as_str().unwrap_or_default())
        .collect::<Vec<_>>()
        != expected
    {
        return Err("Deletion stage order changed".into());
    }
    Ok(
        json!({"ok":true,"case":case,"fixture":ready["fixture"],"readiness":ready,"stages":rows,"final":snapshot(state,&model),
        "initialRecords":fixture::initial_records(&model),"noLiveNetwork":true,
        "limits":["Closed mounted controls; no live Slack actions.","The hidden original reply is verified by its original native node and independent SQLite journal, not hidden pixels.","No deletion restart or storage-lock acceptance claimed."]}),
    )
}
