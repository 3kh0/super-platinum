use super::*;
pub(super) async fn run(
    mut state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    p: &mut Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    query(state, m, d, p, "source-ready", "@alex", data::Mode::Success).await?;
    wait(state, m, |v| {
        search_ready(v, 7, 7, 2, 0)
            && v["completion"]["batchCurrent"] == true
            && v["completion"]["settledBatch"] == true
            && v["completion"]["values"]
                .as_array()
                .is_some_and(|a| a.len() == 9)
    })
    .await?;
    custody::settled_status(state, m, d, p, "source-ready", false).await?;
    p["expectedBlocks"] = current_blocks(d).await?;
    capture(state, m, d, "source-ready", p, rows, false).await?;
    windows(state, m, d, "source-ready", rows, true).await?;
    let before = snapshot(state, m);
    let original = geometry(d, &capture_argument(&before, "source-ready", false)).await?;
    let index = before["completion"]["values"]
        .as_array()
        .ok_or("Missing file choices")?
        .iter()
        .position(|v| v["value"]["file_id"] == data::files::CANVAS)
        .ok_or("Original Canvas missing")?;
    let operation = data::files::invalidate(&mut state.write(), data::files::CANVAS)
        .map_err(|e| e.to_string())?;
    wait(state, m, |v| v["completion"]["current"] != true).await?;
    wait_dom(
        d,
        "({ok:!document.querySelector('.slack-list-channel-option')})",
    )
    .await?;
    let retired = snapshot(state, m);
    let mut old = original["completion"].clone();
    old["index"] = json!(index);
    let stale = command(d, "stale", old).await?;
    let after = snapshot(state, m);
    if stale["accepted"] != false || stale["before"] != stale["after"] || retired != after {
        return Err("File invalidation accepted old choice or changed draft".into());
    }
    p["sourceChange"] = json!({"id":data::files::CANVAS,"before":before,"original":original,"index":index,"operation":operation,"retired":retired,"after":after,"staleChoice":stale});
    capture(state, m, d, "source-refused", p, rows, false).await?;
    Ok(())
}
