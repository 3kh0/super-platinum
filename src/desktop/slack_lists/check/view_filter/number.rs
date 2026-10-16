//! Number-only DOM exercise. Native validity getter observation preserves its result.
use super::*;
fn desired(operator: &str, value: Value) -> Value {
    json!({"column":"C_NUMBER","operator":operator,"value":value})
}
async fn settle(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    want: Value,
) -> Result<Value, String> {
    for _ in 0..150 {
        let model = draft(state)?;
        if model["desired"] == want {
            let choice = model["choice"]
                .as_u64()
                .ok_or("Missing Number epoch")?
                .to_string();
            let expr = format!(
                "({{ok:document.getElementById('slack-list-filter-condition')?.dataset.filterChoice==={0}&&document.getElementById('slack-list-filter-operator')?.dataset.filterChoice==={0}}})",
                json!(choice)
            );
            wait_dom(d, &expr).await?;
            return Ok(model);
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err("Actual Number input did not settle in owned model".into())
}
pub(super) async fn open(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    mode: usize,
) -> Result<Value, String> {
    checked(d, "window.__listViewFilter.rememberFilter()").await?;
    click(d, "slack-list-filter").await?;
    phase(state, Stage::Editing).await?;
    wait_dom(
        d,
        "({ok:document.getElementById('slack-list-filter-condition')===document.activeElement})",
    )
    .await?;
    let selected = checked(
        d,
        "window.__listViewFilter.select('#slack-list-filter-condition','3')",
    )
    .await?;
    let original = draft(state)?;
    let (current, input) = if mode == 1 {
        let empty = settle(state, d, desired("equal", Value::Null)).await?;
        let disabled=wait_dom(d,"({ok:!!document.getElementById('slack-list-rename-confirm')?.disabled&&document.getElementById('slack-list-filter-number')?.value===''})").await?;
        let typing = checked(d, "window.__listViewFilter.typeNumber()").await?;
        let same = settle(state, d, desired("equal", json!("12.50"))).await?;
        if same["choice"] != empty["choice"] {
            return Err("Number typing rotated field/operator epoch".into());
        }
        let retained = checked(d, "window.__listViewFilter.sameNumber()").await?;
        checked(
            d,
            "window.__listViewFilter.select('#slack-list-filter-operator','less_than')",
        )
        .await?;
        let changed = settle(state, d, desired("less_than", json!("12.50"))).await?;
        if changed["choice"] == same["choice"] {
            return Err("Number operator did not rotate epoch".into());
        }
        let remount = wait_dom(d, "window.__listViewFilter.rotatedNumber()").await?;
        (
            changed,
            json!({"incomplete":empty,"disabled":disabled,"typing":typing,"sameEpoch":same,"sameNode":retained,"operatorRemount":remount}),
        )
    } else {
        // Existing comparison is reopened; the actual operator event clears its value.
        let before = settle(state, d, desired("less_than", json!("12.50"))).await?;
        checked(
            d,
            "window.__listViewFilter.select('#slack-list-filter-operator','is_empty')",
        )
        .await?;
        let changed = settle(state, d, desired("is_empty", Value::Null)).await?;
        wait_dom(
            d,
            "({ok:!document.getElementById('slack-list-filter-number')})",
        )
        .await?;
        if changed["choice"] == before["choice"] {
            return Err("Empty operator did not rotate epoch".into());
        }
        (changed, json!({"before":before,"valueRemoved":true}))
    };
    wait_dom(d,"({ok:!!document.getElementById('slack-list-rename-confirm')&&!document.getElementById('slack-list-rename-confirm').disabled})").await?;
    let dom = checked(d, "window.__listViewFilter.draft()").await?;
    if dom["choice"].as_str() != current["choice"].as_u64().map(|n| n.to_string()).as_deref()
        || dom["condition"] != "3"
        || dom["serial"].as_str() != current["serial"].as_u64().map(|n| n.to_string()).as_deref()
    {
        return Err("Rendered Number filter owner drift".into());
    }
    Ok(json!({"model":current,"dom":dom,"selected":selected,"input":input,"opened":original}))
}
pub(super) async fn invalid(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let before = draft(state)?;
    let ledger = rename_fixture::snapshot(&state.peek())?;
    let armed = checked(d, "window.__listViewFilter.armValidity(true)").await?;
    let result: Result<Value, String>=async {
        click(d,"slack-list-rename-confirm").await?;
        let read=wait_dom(d,"window.__listViewFilter.validityRead()").await?;
        wait_dom(d,"({ok:!document.getElementById('slack-list-filter-number')?.disabled&&document.getElementById('slack-list-rename')?.dataset.renameStage==='Editing'})").await?;
        if draft(state)?!=before || rename_fixture::snapshot(&state.peek())?!=ledger || read["reads"].as_array().is_none_or(|r|r.len()!=1||r[0]!=json!({"value":"12.50","valid":false,"badInput":false})){return Err("Invalid browser snapshot admitted or changed Number draft".into());}
        Ok(read)
    }.await;
    // Always restore the exact native property/custom validity even on a failed assertion.
    let restored = checked(d, "window.__listViewFilter.restoreValidity()").await;
    let read = result?;
    let restored = restored?;
    if restored["valid"] != true || restored["value"] != "12.50" {
        return Err("Validity injection did not restore actual input".into());
    }
    Ok(json!({"before":before,"armed":armed,"read":read,"restored":restored,"ledger":ledger}))
}
pub(super) async fn save(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let armed = checked(d, "window.__listViewFilter.armValidity(false)").await?;
    let result = async {
        click(d, "slack-list-rename-confirm").await?;
        phase(state, Stage::Saving).await?;
        pending(state, "writePending").await?;
        Ok::<(), String>(())
    }
    .await;
    let restored = checked(d, "window.__listViewFilter.restoreValidity()").await;
    result?;
    let restored = restored?;
    if restored["reads"]
        != json!([{"value":"12.50","valid":true,"badInput":false},{"value":"12.50","valid":true,"badInput":false}])
    {
        return Err("Actual successful browser validity snapshot differs".into());
    }
    Ok(json!({"armed":armed,"restored":restored}))
}
