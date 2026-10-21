use super::*;

async fn capture_reply(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    stage: &str,
    p: &Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    let message = snapshot(state, m)["composeLedger"]["messages"][0].clone();
    let expected = json!({"ts":message["ts"],"files":[
        {"id":data::files::CANVAS,"kind":"canvas","label":data::files::CANVAS_LABEL},
        {"id":data::files::LIST,"kind":"file","label":data::files::LIST_LABEL}
    ]});
    let probe = |scroll: bool| {
        format!(
            "({})({},{})",
            include_str!("transcript.js"),
            expected,
            json!(scroll)
        )
    };
    wait_dom(d, &probe(true)).await?;
    let before = checked(d, &probe(false)).await?;
    capture(state, m, d, stage, p, rows, false).await?;
    let after = checked(d, &probe(false)).await?;
    if before != after || snapshot(state, m)["composeLedger"]["messages"][0] != message {
        return Err("Painted file titles crossed original reply/source during capture".into());
    }
    rows.last_mut().ok_or("Missing reply capture")?["proof"]["renderedFiles"] =
        json!({"message":message,"before":before,"after":after});
    Ok(())
}
pub(super) async fn run(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    p: &mut Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    capture(state, m, d, "draft", p, rows, false).await?;
    query(state, m, d, p, "local-held", "@alex", data::Mode::Held).await?;
    wait(state, m, |v| {
        search_ready(v, 1, 0, 0, 1)
            && v["completion"]["settledBatch"] == false
            && v["completion"]["batchCurrent"] == true
            && v["completion"]["labels"].as_array().is_some_and(|a| {
                a.len() == 7
                    && a.contains(&json!("@Alex Harbor"))
                    && a.contains(&json!("@Alex Self"))
            })
    })
    .await?;
    p["expectedBlocks"] = current_blocks(d).await?;
    capture(state, m, d, "local-held", p, rows, false).await?;
    let release = data::handle(&state.peek())
        .map_err(|e| e.to_string())?
        .release()
        .map_err(|e| e.to_string())?;
    if release["delivered"] != true {
        return Err("Original member reply not delivered".into());
    }
    p["releases"]
        .as_array_mut()
        .unwrap()
        .push(json!({"stage":"refreshed","value":release}));
    wait(state, m, |v| {
        search_ready(v, 1, 1, 0, 0)
            && v["completion"]["batchCurrent"] == true
            && v["completion"]["settledBatch"] == true
            && v["completion"]["labels"]
                .as_array()
                .is_some_and(|a| a.len() == 9)
    })
    .await?;
    capture(state, m, d, "refreshed", p, rows, false).await?;
    windows(state, m, d, "refreshed", rows, true).await?;
    choose(
        state,
        m,
        d,
        p,
        "keyboard-inserted",
        data::files::CANVAS_LABEL,
        data::files::CANVAS,
        true,
    )
    .await?;
    capture(state, m, d, "keyboard-inserted", p, rows, false).await?;
    query(
        state,
        m,
        d,
        p,
        "mouse-inserted",
        "@alex",
        data::Mode::Success,
    )
    .await?;
    wait(state, m, |v| {
        search_ready(v, 1, 1, 0, 0)
            && v["completion"]["batchCurrent"] == true
            && v["completion"]["settledBatch"] == true
            && v["completion"]["labels"]
                .as_array()
                .is_some_and(|a| a.len() == 9)
    })
    .await?;
    choose(
        state,
        m,
        d,
        p,
        "mouse-inserted",
        data::files::LIST_LABEL,
        data::files::LIST,
        false,
    )
    .await?;
    capture(state, m, d, "mouse-inserted", p, rows, false).await?;
    reload(state, m, d, p).await?;
    capture(state, m, d, "reopened", p, rows, false).await?;
    compose::mode(m, Mode::HeldSuccess)?;
    click(d, "slack-list-comment-send").await?;
    wait_model(state, m, |v| {
        counts(v, 1, 1) && v["composeLedger"]["heldPost"] == true
    })
    .await?;
    capture(state, m, d, "sending", p, rows, true).await?;
    let release = compose::release_post(m)?;
    if release != json!({"delivered":true,"cancelledBeforeReply":false}) {
        return Err("Original completed comment receipt failed".into());
    }
    p["postRelease"] = release;
    p["sentBlocks"] = p["expectedBlocks"].clone();
    p["expectedBlocks"] = json!([]);
    wait_dom(d,"({ok:!document.getElementById('slack-list-comment-draft')||document.getElementById('slack-list-comment-draft').innerText.trim()===''})").await?;
    stable_drafts(state, m).await?;
    capture_reply(state, m, d, "sent", p, rows).await?;
    compose::publish(m)?;
    click(d, "slack-list-comments-refresh").await?;
    wait_model(state, m, |v| {
        v["comments"]["loading"] == false
            && v["ledger"]["requests"]
                .as_array()
                .is_some_and(|a| a.len() == 16)
    })
    .await?;
    let sent = snapshot(state, m)["composeLedger"]["messages"][0]["ts"].clone();
    wait_dom(
        d,
        &format!(
            "({{ok:window.__listCompose.snapshot().rows.filter(r=>r.ts==={sent}).length===1}})"
        ),
    )
    .await?;
    capture_reply(state, m, d, "readback", p, rows).await?;
    Ok(())
}
