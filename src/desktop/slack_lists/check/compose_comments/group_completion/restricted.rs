//! Actual restricted self permits members while suppressing group acquisition.
use super::*;

pub(super) async fn run(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    p: &mut Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    capture(state, m, d, "draft", p, rows, false).await?;
    query(
        state,
        m,
        d,
        p,
        "restricted-results",
        "@alex",
        data::Mode::Success,
    )
    .await?;
    wait(state, m, |v| {
        search_ready(v, 0, 0, 0, 0)
            && v["completion"]["batchCurrent"] == true
            && v["completion"]["settledBatch"] == true
            && v["completionSource"]["groups"]["selfRestricted"] == true
            && v["completionSource"]["requests"]
                .as_array()
                .is_some_and(|a| a.iter().filter(|r| r["method"] == "users/search").count() == 1)
            && v["completion"]["values"]
                .as_array()
                .is_some_and(|a| a.len() == 4 && a.iter().all(|r| r["value"]["type"] == "user"))
    })
    .await?;
    custody::settled_status(state, m, d, p, "restricted-results", false).await?;
    p["expectedBlocks"] = current_blocks(d).await?;
    capture(state, m, d, "restricted-results", p, rows, false).await?;
    choose(
        state,
        m,
        d,
        p,
        "member-inserted",
        "@Alex Harbor",
        data::members::SECOND,
        true,
    )
    .await?;
    capture(state, m, d, "member-inserted", p, rows, false).await?;
    Ok(())
}
