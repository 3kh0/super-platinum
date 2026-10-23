use super::*;
async fn reply(
    state: Signal<ShellState>,
    m: &Model,
    d: &Desktop,
    name: &str,
    p: &mut Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    let message = snapshot(state, m)["composeLedger"]["messages"][0].clone();
    let expected = json!({"ts":message["ts"],"files":[{"id":data::files::CANVAS,"kind":"canvas","label":source::label()},{"id":data::files::LIST,"kind":"file","label":data::files::LIST_LABEL}]});
    let probe = |scroll| {
        format!(
            "({})({},{})",
            include_str!("../file_completion/transcript.js"),
            expected,
            json!(scroll)
        )
    };
    wait_dom(d, &probe(true)).await?;
    let direct = if data::link_title::named() {
        Some(
            data::link_title::Transcript::capture(&state.peek(), &message)
                .map_err(|e| e.to_string())?,
        )
    } else {
        None
    };
    let direct_before = direct.as_ref().map(|w| w.snapshot(&state.peek()));
    let direct_probe = |arm| {
        format!(
            "({})({},{})",
            include_str!("link_transcript.js"),
            expected,
            json!(arm)
        )
    };
    let direct_native_before = if direct.is_some() {
        Some(checked(d, &direct_probe(true)).await?)
    } else {
        None
    };
    let before = checked(d, &probe(false)).await?;
    shot(state, m, d, name, p, rows).await?;
    let after = checked(d, &probe(false)).await?;
    if before != after || snapshot(state, m)["composeLedger"]["messages"][0] != message {
        return Err("Transcript title changed across capture".into());
    }
    if let Some(witness) = direct {
        let after = witness.snapshot(&state.peek());
        let native_after = checked(d, &direct_probe(false)).await?;
        if !witness.current(&state.peek())
            || direct_before.as_ref() != Some(&after)
            || direct_native_before.as_ref() != Some(&native_after)
        {
            return Err("Original direct transcript token changed across capture".into());
        }
        rows.last_mut().unwrap()["proof"]["directTranscript"] = json!({
            "before":direct_before,"after":after,"nativeBefore":direct_native_before,"nativeAfter":native_after});
    }
    rows.last_mut().unwrap()["proof"]["renderedFiles"] =
        json!({"message":message,"before":before,"after":after});
    Ok(())
}
pub(super) async fn run(
    state: Signal<ShellState>,
    m: &Model,
    d: &Desktop,
    p: &mut Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    query(state, m, d, p, "query-ready", "@alex", data::Mode::Success).await?;
    source_ready(state, m).await?;
    p["expectedBlocks"] = current_blocks(d).await?;
    shot(state, m, d, "query-ready", p, rows).await?;
    choose(
        state,
        m,
        d,
        p,
        "canvas-inserted",
        source::label(),
        data::files::CANVAS,
        true,
    )
    .await?;
    shot(state, m, d, "canvas-inserted", p, rows).await?;
    query(
        state,
        m,
        d,
        p,
        "list-inserted",
        "@alex",
        data::Mode::Success,
    )
    .await?;
    source_ready(state, m).await?;
    choose(
        state,
        m,
        d,
        p,
        "list-inserted",
        data::files::LIST_LABEL,
        data::files::LIST,
        false,
    )
    .await?;
    shot(state, m, d, "list-inserted", p, rows).await?;
    p["beforeReopen"] = snapshot(state, m);
    reload(state, m, d, p).await?;
    wait(state, m, |v| {
        v["titleUsers"]["current"] == true && v["titleUsers"]["observed"]["current"] == true
    })
    .await?;
    shot(state, m, d, "reopened", p, rows).await?;
    compose::mode(m, Mode::HeldSuccess)?;
    action(d, p, "slack-list-comment-send").await?;
    wait_model(state, m, |v| {
        counts(v, 1, 1) && v["composeLedger"]["heldPost"] == true
    })
    .await?;
    shot(state, m, d, "sending", p, rows).await?;
    let release = compose::release_post(m)?;
    if release != json!({"delivered":true,"cancelledBeforeReply":false}) {
        return Err("Original post receipt retired".into());
    }
    p["postRelease"] = release;
    p["sentBlocks"] = p["expectedBlocks"].clone();
    p["expectedBlocks"] = json!([]);
    wait_dom(d,"({ok:!document.getElementById('slack-list-comment-draft')||document.getElementById('slack-list-comment-draft').innerText.trim()===''})").await?;
    stable_drafts(state, m).await?;
    reply(state, m, d, "sent", p, rows).await?;
    compose::publish(m)?;
    action(d, p, "slack-list-comments-refresh").await?;
    wait_model(state, m, |v| {
        v["comments"]["loading"] == false
            && v["ledger"]["requests"]
                .as_array()
                .is_some_and(|a| a.len() == 16)
    })
    .await?;
    reply(state, m, d, "readback", p, rows).await
}
