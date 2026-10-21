use super::*;
pub(super) async fn run(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    p: &mut Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    capture(state, m, d, "draft", p, rows, false).await?;
    query(state, m, d, p, "held", "#project", data::Mode::Held).await?;
    wait(state, m, |v| {
        search_ready(v, 1, 0, 0, 1)
            && v["completion"]["labels"]
                .as_array()
                .is_some_and(|a| !a.is_empty())
    })
    .await?;
    p["expectedBlocks"] = current_blocks(d).await?;
    capture(state, m, d, "held", p, rows, false).await?;
    let original = rows.last().unwrap()["geometry"]["completion"].clone();
    query(
        state,
        m,
        d,
        p,
        "query-retired",
        "#project-new",
        data::Mode::Success,
    )
    .await?;
    // Request completion precedes result publication; wait for the settled model.
    wait(state, m, |v| {
        search_ready(v, 2, 2, 1, 1)
            && v["completion"]["current"] == true
            && v["completion"]["started"] == true
            && v["completion"]["consumed"] == false
            && v["completion"]["revision"] == 1
            && v["completion"]["labels"] == json!([])
    })
    .await?;
    p["expectedBlocks"] = current_blocks(d).await?;
    capture(state, m, d, "query-retired", p, rows, false).await?;
    let stale = command(d, "stale", original).await?;
    if stale["before"] != stale["after"] {
        return Err("Stale suggestion changed editor".into());
    }
    let release = data::handle(&state.peek())
        .map_err(|e| e.to_string())?
        .release()
        .map_err(|e| e.to_string())?;
    if release["delivered"] != false || release["cancelledBeforeReply"] != true {
        return Err("Retired channel query received late rows".into());
    }
    p["staleChoice"] = stale;
    p["releases"]
        .as_array_mut()
        .unwrap()
        .push(json!({"stage":"stale-refused","value":release}));
    capture(state, m, d, "stale-refused", p, rows, false).await?;
    for (stage, term, mode, n) in [
        ("malformed", "#malformed", data::Mode::Malformed, 3),
        ("failure", "#offline", data::Mode::Offline, 4),
        ("empty", "#empty", data::Mode::Empty, 5),
    ] {
        query(state, m, d, p, stage, term, mode).await?;
        let revision = if stage == "empty" { 2 } else { 1 };
        wait(state, m, |v| {
            search_ready(v, n, n, 1, 0)
                && v["completion"]["current"] == true
                && v["completion"]["started"] == true
                && v["completion"]["consumed"] == false
                && v["completion"]["revision"] == revision
                && v["completion"]["labels"] == json!([])
        })
        .await?;
        wait_dom(
            d,
            &format!(
                "({{ok:document.querySelector('.slack-list-channel-status')?.textContent==={}}})",
                json!(if stage == "empty" {
                    "No matching channels"
                } else {
                    "Channel suggestions could not load. Edit your query to try again."
                })
            ),
        )
        .await?;
        p["expectedBlocks"] = current_blocks(d).await?;
        capture(state, m, d, stage, p, rows, false).await?;
    }
    query(
        state,
        m,
        d,
        p,
        "cancelled-reopened",
        "#cancelled",
        data::Mode::Held,
    )
    .await?;
    wait(state, m, |v| search_ready(v, 6, 5, 1, 1)).await?;
    p["expectedBlocks"] = current_blocks(d).await?;
    reload(state, m, d, p).await?;
    wait(state, m, |v| search_ready(v, 6, 6, 2, 1)).await?;
    let release = data::handle(&state.peek())
        .map_err(|e| e.to_string())?
        .release()
        .map_err(|e| e.to_string())?;
    if release["delivered"] != false || release["cancelledBeforeReply"] != true {
        return Err("Departed original query received late rows".into());
    }
    p["releases"]
        .as_array_mut()
        .unwrap()
        .push(json!({"stage":"cancelled-reopened","value":release}));
    capture(state, m, d, "cancelled-reopened", p, rows, false).await?;
    Ok(())
}
