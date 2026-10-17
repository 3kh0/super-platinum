use super::super::*;
pub(crate) async fn configure(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let before = draft(state)?;
    if before["kind"] != "number" || before["numberPrecision"] != 0 {
        return Err("Configured Number did not start at default0".into());
    }
    let event = checked(d, "window.__listNumberPrecision.choose()").await?;
    let mut after = None;
    for _ in 0..150 {
        let current = draft(state)?;
        if current["kind"] == "number"
            && current["numberPrecision"] == 3
            && current["choice"] != before["choice"]
            && current["serial"] == before["serial"]
            && current["visit"] == before["visit"]
            && current["selection"] == before["selection"]
            && current["name"] == before["name"]
        {
            after = Some(current);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    let after = after.ok_or("Actual Number precision callback did not settle")?;
    let dom = verify(state, d).await?;
    Ok(json!({"before":before,"event":event,"after":after,"dom":dom}))
}
pub(crate) async fn verify(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let model = draft(state)?;
    let dom = wait_dom(d, "window.__listNumberPrecision.inspect()").await?;
    if model["numberPrecision"] != 3
        || dom["value"] != "3"
        || dom["choice"].as_str() != model["choice"].as_u64().map(|v| v.to_string()).as_deref()
        || draft(state)? != model
    {
        return Err("Precision DOM/model custody changed".into());
    }
    Ok(dom)
}
