//! Actual owned group-source mutations retire the displayed choice.
use super::*;

pub(super) async fn run(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    p: &mut Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    p["sourceChanges"] = json!([]);
    for (stage, deleted, count) in [("source-renamed", false, 7), ("source-deleted", true, 8)] {
        query(state, m, d, p, stage, "@alex", data::Mode::Empty).await?;
        wait(state, m, |v| {
            search_ready(v, count, count, 2, 0)
                && v["completion"]["batchCurrent"] == true
                && v["completion"]["settledBatch"] == true
                && v["completion"]["current"] == true
                && v["completion"]["values"].as_array().is_some_and(|a| {
                    a.iter()
                        .any(|v| v["value"]["usergroup_id"] == data::groups::FIRST)
                })
        })
        .await?;
        let before = snapshot(state, m);
        let original = geometry(d, &capture_argument(&before, stage, false)).await?;
        let choices = before["completion"]["values"]
            .as_array()
            .ok_or("Missing group choices")?;
        let index = choices
            .iter()
            .position(|v| v["value"]["usergroup_id"] == data::groups::FIRST)
            .ok_or("Original group missing before source change")?;
        let receipt = data::groups::replace(state, data::groups::FIRST, "alex-renamed", deleted)
            .await
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
            return Err("Group source retirement changed draft or accepted old choice".into());
        }
        p["expectedBlocks"] = current_blocks(d).await?;
        p["sourceChanges"].as_array_mut().unwrap().push(json!({
            "stage":stage,"id":data::groups::FIRST,"deleted":deleted,"before":before,
            "original":original,"index":index,"operation":receipt,"retired":retired,
            "after":after,"staleChoice":stale
        }));
        capture(state, m, d, stage, p, rows, false).await?;
    }
    Ok(())
}
