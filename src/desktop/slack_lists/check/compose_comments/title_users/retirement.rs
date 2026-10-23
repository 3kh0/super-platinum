use super::*;
pub(super) async fn run(
    state: Signal<ShellState>,
    m: &Model,
    d: &Desktop,
    p: &mut Value,
    rows: &mut Vec<Value>,
    disk: &Disk,
) -> Result<(), String> {
    query(state, m, d, p, "discovery", "@alex", data::Mode::Success).await?;
    let discovery = next(state, m, "discovery").await?;
    p["expectedBlocks"] = current_blocks(d).await?;
    shot(state, m, d, "discovery", p, rows).await?;
    release(state, p, discovery, "discovery", true)?;
    let held = next(state, m, "member").await?;
    shot(state, m, d, "member-held", p, rows).await?;
    p["oldQuery"] = snapshot(state, m)["completion"].clone();
    query(
        state,
        m,
        d,
        p,
        "query-replaced",
        "@nomatch",
        data::Mode::Success,
    )
    .await?;
    wait(state, m, |v| {
        v["completion"]["batchCurrent"] == true
            && v["completion"]["settledBatch"] == true
            && v["completion"]["labels"] == json!([])
    })
    .await?;
    p["expectedBlocks"] = current_blocks(d).await?;
    // Empty original query status is source text; use strict base empty geometry then retain exact stage name.
    shot(state, m, d, "query-replaced", p, rows).await?;
    release(state, p, held, "member", true)?;
    wait(state, m, |v| {
        v["completionSource"]["titleUsers"]["driver"]["active"] == 0
    })
    .await?;
    shot(state, m, d, "late-query", p, rows).await?;
    query(
        state,
        m,
        d,
        p,
        "second-discovery",
        "@al",
        data::Mode::Success,
    )
    .await?;
    let discovery = next(state, m, "discovery").await?;
    p["expectedBlocks"] = current_blocks(d).await?;
    shot(state, m, d, "second-discovery", p, rows).await?;
    release(state, p, discovery, "discovery", true)?;
    let held = next(state, m, "member").await?;
    shot(state, m, d, "second-member", p, rows).await?;
    p["originalBeforeLeave"] = snapshot(state, m);
    p["originalJournal"] = disk.read().await?;
    action(d, p, "slack-list-comments-back").await?;
    wait_dom(d,"({ok:!document.getElementById('slack-list-comments')&&document.activeElement===document.getElementById('slack-list-record-comments')})").await?;
    action(d, p, "slack-list-record-back").await?;
    wait_dom(d,"({ok:!document.getElementById('slack-list-record-detail')&&!!document.querySelector('.slack-list-grid')})").await?;
    open(d, "R_LIST_001").await?;
    pending_shot(state, m, d, "record-replaced", p, rows, disk).await?;
    action(d, p, "slack-list-close").await?;
    wait_dom(d, "({ok:!document.querySelector('.slack-list-dialog')})").await?;
    pending_shot(state, m, d, "closed", p, rows, disk).await?;
    release(state, p, held, "member", true)?;
    wait(state, m, |v| {
        v["completionSource"]["titleUsers"]["driver"]["active"] == 0
    })
    .await?;
    pending_shot(state, m, d, "late-closed", p, rows, disk).await
}
