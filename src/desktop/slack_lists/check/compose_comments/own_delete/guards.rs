//! Native permission/source refusal and real persisted Retry-After expiry.
use super::*;
const GUARDS: &[&str] = &[
    "draft",
    "confirming",
    "policy-refused",
    "cancelled",
    "source-confirming",
    "source-changed",
    "source-cancelled",
    "reopened",
];
const RATE: &[&str] = &[
    "draft",
    "confirming",
    "rate-limited",
    "cooldown-cancelled",
    "cooldown-reopened",
    "cooldown-expired",
    "deleted",
    "readback",
    "reopened",
];
const WAIT: &str =
    "Slack asked you to wait. Your drafts are retained; no request is automatically retried.";
fn clock() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
async fn take(
    state: Signal<ShellState>,
    model: &Model,
    d: &dioxus::desktop::DesktopContext,
    name: &str,
    proof: &Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    let mut proof = proof.clone();
    proof["observedAt"] = json!(clock());
    capture(state, model, d, name, &proof, rows).await?;
    rows.last_mut().ok_or("Capture missing")?["proof"]["observedUntil"] = json!(clock());
    Ok(())
}
async fn status(d: &dioxus::desktop::DesktopContext, text: &str) -> Result<(), String> {
    wait_dom(d, &format!("({{ok:document.getElementById('slack-list-comment-delete-status')?.textContent==={}}})",json!(text))).await?;
    Ok(())
}
fn change_source(mut state: Signal<ShellState>, model: &Model) -> Result<Value, String> {
    use super_platinum_core::slack::events::RtEvent;
    let mut changed = data::snapshot(&state.peek())["received"].clone();
    changed["text"] = json!("Socket changed original comment \u{65e5}");
    changed["blocks"][0]["elements"][0]["elements"][0]["text"] = changed["text"].clone();
    changed["edited"] = json!({"user":fixture::completion::SELF,"ts":"1781525900.000001"});
    let frame = json!({"type":"message","subtype":"message_changed","channel":"CCOMMENT01","message":changed});
    let event = super_platinum_core::slack::realtime::parse_event(&frame.to_string())
        .ok_or("Selected delete event did not parse")?;
    let witness = match &event {
        RtEvent::MessageChanged {
            original_update: Some(update),
            ..
        } => {
            json!({"kind":format!("{:?}",update.kind()),"outerChannel":update.outer_channel(),"typedTs":update.typed_ts(),"typedChannel":update.typed_channel(),"raw":update.raw()})
        }
        _ => return Err("Selected event lost original partial witness".into()),
    };
    let before = snapshot(state, model);
    let generation = {
        let mut shell = state.write();
        if !data::enabled(&shell) || !fixture::completion::enabled(&shell) {
            return Err("Closed original deletion fixture required".into());
        }
        let generation = shell
            .core
            .workspaces
            .get("T1")
            .ok_or("Original workspace missing")?
            .rt_generation;
        if crate::slack_lists::events::prepare(&mut shell, "T1", generation, &event) {
            return Err("Body edit unexpectedly requested List refresh".into());
        }
        crate::realtime::apply_in_view(&mut shell.core, "T1", generation, event, false);
        generation
    };
    let after = snapshot(state, model);
    if before["delete"]["active"]["sourceCurrent"] != true
        || after["delete"]["active"]["sourceCurrent"] != false
        || before["delete"]["active"]["record"] != after["delete"]["active"]["record"]
        || before["comments"]["draftStore"] != after["comments"]["draftStore"]
        || before["comments"]["comments"]["page"] != after["comments"]["comments"]["page"]
        || before["fixture"] != after["fixture"]
    {
        return Err("Selected event changed reply/HTTP/delete custody unexpectedly".into());
    }
    Ok(
        json!({"team":"T1","generation":generation,"frame":frame,"parsed":true,"witness":witness,
        "before":{"delete":before["delete"],"comments":before["comments"]},"after":{"delete":after["delete"],"comments":after["comments"]}}),
    )
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    case: &str,
) -> Result<Value, String> {
    let expected = match case {
        "own-delete-guards" => GUARDS,
        "own-delete-rate-limit" => RATE,
        _ => return Err("Unknown delete guard case".into()),
    };
    let ready = harness::wait_runner(state).await?;
    let model = fixture::begin(&mut state.write())?;
    fixture::completion::seed(state).await?;
    let source = data::seed(state)?;
    checked(d, include_str!("../../comments.js")).await?;
    checked(d, include_str!("../../compose_comments.js")).await?;
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
    take(state, &model, d, "draft", &proof, &mut rows).await?;
    open_delete(state, d).await?;
    take(state, &model, d, "confirming", &proof, &mut rows).await?;
    if case == "own-delete-guards" {
        proof["permissionDenied"] = data::permission(&state.peek(), false)?;
        button(d, "confirm").await?;
        wait_state(state, |v| v["active"]["busy"] == false).await?;
        status(d,"The message or deletion permissions changed. Nothing was sent; Cancel returns to your saved reply.").await?;
        take(state, &model, d, "policy-refused", &proof, &mut rows).await?;
        close_delete(state, d).await?;
        take(state, &model, d, "cancelled", &proof, &mut rows).await?;
        proof["permissionRestored"] = data::permission(&state.peek(), true)?;
        open_delete(state, d).await?;
        take(state, &model, d, "source-confirming", &proof, &mut rows).await?;
        proof["sourceChanged"] = change_source(state, &model)?;
        wait_state(state, |v| {
            v["active"]["sourceCurrent"] == false && v["active"]["canConfirm"] == false
        })
        .await?;
        wait_dom(
            d,
            "({ok:document.getElementById('slack-list-comment-delete-confirm')?.disabled===true})",
        )
        .await?;
        proof["blockedConfirm"] = command(d, "blocked-confirm", Value::Null).await?;
        take(state, &model, d, "source-changed", &proof, &mut rows).await?;
        close_delete(state, d).await?;
        take(state, &model, d, "source-cancelled", &proof, &mut rows).await?;
    } else {
        data::mode(&state.peek(), data::Mode::RateLimited)?;
        proof["rateIssuedAt"] = json!(clock());
        button(d, "confirm").await?;
        wait_state(state, |v| {
            v["active"]["phase"] == "RateLimited"
                && v["active"]["busy"] == false
                && v["active"]["canConfirm"] == false
        })
        .await?;
        status(d, WAIT).await?;
        proof["cooldownDeadline"] = inventory(state).await?["retry_at"].clone();
        proof["blockedConfirm"] = command(d, "blocked-confirm", Value::Null).await?;
        take(state, &model, d, "rate-limited", &proof, &mut rows).await?;
        close_delete(state, d).await?;
        take(state, &model, d, "cooldown-cancelled", &proof, &mut rows).await?;
        command(d, "open", json!({"ts":data::RECEIVED_TS})).await?;
        wait_state(state, |v| {
            v["active"]["phase"] == "RateLimited"
                && v["active"]["busy"] == false
                && v["active"]["canConfirm"] == false
        })
        .await?;
        status(d, WAIT).await?;
        proof["reopenedBlockedConfirm"] = command(d, "blocked-confirm", Value::Null).await?;
        take(state, &model, d, "cooldown-reopened", &proof, &mut rows).await?;
        wait_state(state, |v| {
            v["active"]["phase"] == "RateLimited"
                && v["active"]["canConfirm"] == true
                && fixture::snapshot(&state.peek(), &model)["comments"]["retryAt"] == 0
        })
        .await?;
        take(state, &model, d, "cooldown-expired", &proof, &mut rows).await?;
        data::mode(&state.peek(), data::Mode::Success)?;
        button(d, "confirm").await?;
        wait_state(state, |v| {
            v["active"]["phase"] == "Confirmed"
                && v["active"]["busy"] == false
                && v["active"]["localPending"] == false
        })
        .await?;
        close_delete(state, d).await?;
        take(state, &model, d, "deleted", &proof, &mut rows).await?;
        data::publish(&state.peek())?;
        click(d, "slack-list-comments-refresh").await?;
        wait_state(state, |_| {
            let s = fixture::snapshot(&state.peek(), &model);
            s["comments"]["loading"] == false
                && s["comments"]["page"]["deleted"] == json!([data::RECEIVED_TS])
        })
        .await?;
        wait_dom(
            d,
            "({ok:document.getElementById('slack-list-comments-refresh')?.disabled===false})",
        )
        .await?;
        take(state, &model, d, "readback", &proof, &mut rows).await?;
    }
    reopen_reply(state, &model, d).await?;
    proof["runtimeForgotten"] = json!(true);
    take(state, &model, d, "reopened", &proof, &mut rows).await?;
    if rows
        .iter()
        .map(|r| r["stage"].as_str().unwrap_or_default())
        .collect::<Vec<_>>()
        != expected
    {
        return Err("Delete guard stage order changed".into());
    }
    Ok(
        json!({"ok":true,"case":case,"fixture":ready["fixture"],"readiness":ready,"stages":rows,"final":snapshot(state,&model),"initialRecords":fixture::initial_records(&model),"noLiveNetwork":true,
        "limits":["Closed original controls and parsed fixture event; no live Slack.","Cooldown uses the real clock and SQLite deadline, not a clock override.","No OS-crash durability or full desktop unit-test execution claimed."]}),
    )
}
