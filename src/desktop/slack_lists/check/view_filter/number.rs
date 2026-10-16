//! One Number input in a legacy or compound complete-row capture. Native validity getter observation preserves its result.
use super::*;
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
        wait_dom(d,"({ok:!window.__listViewFilter.rowControl('C_NUMBER','value')?.disabled&&document.getElementById('slack-list-rename')?.dataset.renameStage==='Editing'})").await?;
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
