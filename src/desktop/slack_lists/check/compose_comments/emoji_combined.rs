//! Actual supplied URL replacement with independent original reply custody.
use super::*;
use fixture::emoji_combined as source;
fn script(action: &str, mode: &str) -> String {
    format!(
        "({})({},{})",
        include_str!("emoji_combined.js"),
        json!(action),
        json!(mode)
    )
}
async fn take(
    state: Signal<ShellState>,
    model: &Model,
    d: &dioxus::desktop::DesktopContext,
    name: &str,
    mode: &str,
    proof: &Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    stable_drafts(state, model).await?;
    let before = wait_dom(d, &script("observe", mode)).await?;
    let model_before = compose::snapshot(&state.peek(), model);
    let journal = compose::journal_snapshot(state).await?;
    if !source::context_enabled(&state.peek())
        || model_before["noSession"] != false
        || model_before["noTransport"] != false
        || model_before["writes"] != 0
        || model_before["networkCalls"] != 0
        || model_before["ledger"]["requests"]
            .as_array()
            .is_none_or(|r| r.len() != 4)
        || model_before["ledger"]["completed"] != 4
        || model_before["comments"]["loading"] != false
        || before["reply"]["blocks"] != proof["originalA"]
        || journal["blocks"] != proof["originalA"]
    {
        return Err("Display source changed original read/draft custody".into());
    }
    let capture = harness::capture(d, name).await?;
    let after = checked(d, &script("observe", mode)).await?;
    if before != after
        || compose::snapshot(&state.peek(), model) != model_before
        || compose::journal_snapshot(state).await? != journal
    {
        return Err(format!(
            "Original display/draft changed across {name} capture"
        ));
    }
    rows.push(json!({"stage":name,"model":model_before,"journal":journal,"geometry":before["geometry"],"native":{"before":before,"after":after},"source":source::snapshot(&state.peek()),"proof":proof,"capture":capture}));
    Ok(())
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let readiness = harness::wait_runner(state).await?;
    source::seed(state).await?;
    let model = fixture::begin(&mut state.write())?;
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
    type_text(
        d,
        "Final review \u{4e2d}\u{1f642}\nSecond line <literal> & *punctuation*.",
    )
    .await?;
    checked(d, "window.__listCompose.bold()").await?;
    settled(d).await?;
    let native=wait_dom(d,"(()=>{const v=window.__listCompose.snapshot();v.ok=v.ok&&v.editor?.bold===true;return v;})()").await?;
    edits::witness(d).await?;
    let mut proof = json!({"seed":seed,"originalA":native["editor"]["blocks"],"newerB":null,"heldResponse":null,"release":null,"originalMessage":null,"serverResponse":null});
    proof["isolatedSourceGuard"] =
        crate::app_home::channel_fetches::fixture_emoji_guard_probe().await?;
    stable_drafts(state, &model).await?;
    proof["arm"] = checked(d, &script("arm", "unobserved")).await?;
    let mut rows = Vec::new();
    take(
        state,
        &model,
        d,
        "unobserved",
        "unobserved",
        &proof,
        &mut rows,
    )
    .await?;
    proof["assignments"] = source::assignments(state).await?;
    take(state, &model, d, "pending", "pending", &proof, &mut rows).await?;
    proof["mediaA"] = source::deliver_media(&state.peek(), false)?;
    take(state, &model, d, "ready", "ready", &proof, &mut rows).await?;
    proof["replacement"] = inject(state, &model)?;
    take(
        state,
        &model,
        d,
        "replacement-pending",
        "pending",
        &proof,
        &mut rows,
    )
    .await?;
    proof["mediaB"] = source::deliver_media(&state.peek(), true)?;
    take(
        state,
        &model,
        d,
        "replacement-ready",
        "ready",
        &proof,
        &mut rows,
    )
    .await?;
    Ok(edits::result(
        source::CASE,
        readiness,
        rows,
        vec![proof["replacement"].clone()],
        state,
        &model,
    ))
}

fn inject(mut state: Signal<ShellState>, model: &Model) -> Result<Value, String> {
    let frame = source::replacement();
    let event = super_platinum_core::slack::realtime::parse_event(&frame.to_string())
        .ok_or("Original replacement parse failed")?;
    let witness = match &event {
        super_platinum_core::slack::events::RtEvent::MessageChanged {
            original_update: Some(update),
            ..
        } => {
            json!({"kind":format!("{:?}",update.kind()),"outerChannel":update.outer_channel(),"typedChannel":update.typed_channel(),"typedTs":update.typed_ts(),"raw":update.raw()})
        }
        _ => return Err("Original message_changed witness missing".into()),
    };
    let mut shell = state.write();
    if !source::context_enabled(&shell) || source::snapshot(&shell)["standard"]["current"] != true {
        return Err("Synthetic original assignment owner retired".into());
    }
    let before = compose::snapshot(&shell, model);
    if crate::slack_lists::events::prepare(&mut shell, "T1", 7, &event) {
        return Err("Combined replacement requested List refresh".into());
    }
    let after = compose::snapshot(&shell, model);
    if before == after
        || before["draftStore"] != after["draftStore"]
        || before["comments"]["page"] != after["comments"]["page"]
        || !source::context_enabled(&shell)
    {
        return Err("Combined replacement changed original owner/draft/Page".into());
    }
    Ok(
        json!({"stage":"replacement","team":"T1","generation":7,"frame":frame,"parsed":true,"witness":witness,"changed":true,"before":edits::event_state(&before),"after":edits::event_state(&after)}),
    )
}
