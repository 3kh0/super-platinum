use super::*;
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
            && v["completion"]["batchCurrent"] == true
            && v["completion"]["labels"].as_array().is_some_and(|a| {
                a.len() == 5
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
            && v["completion"]["labels"]
                .as_array()
                .is_some_and(|a| a.len() == 7)
    })
    .await?;
    capture(state, m, d, "refreshed", p, rows, false).await?;
    command(d, "scroll", json!("bottom")).await?;
    let before = snapshot(state, m);
    let arg = capture_argument(&before, "refreshed", false);
    let bottom = geometry(d, &arg).await?;
    let image = harness::capture(d, "refreshed-bottom").await?;
    command(d, "snapshot", arg).await?;
    if before != snapshot(state, m) {
        return Err("Member scroll crossed source publication".into());
    }
    rows.last_mut().unwrap()["proof"]["bottomWindow"] =
        json!({"geometry":bottom,"capture":image,"model":before});
    command(d, "scroll", json!("top")).await?;
    choose(
        state,
        m,
        d,
        p,
        "keyboard-inserted",
        "@alex-team",
        data::groups::SECOND,
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
        search_ready(v, 2, 2, 0, 0)
            && v["completion"]["batchCurrent"] == true
            && v["completion"]["labels"]
                .as_array()
                .is_some_and(|a| a.len() == 7)
    })
    .await?;
    choose(
        state,
        m,
        d,
        p,
        "mouse-inserted",
        "@alex-team",
        data::groups::FIRST,
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
    capture(state, m, d, "sent", p, rows, false).await?;
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
    capture(state, m, d, "readback", p, rows, false).await?;
    Ok(())
}
