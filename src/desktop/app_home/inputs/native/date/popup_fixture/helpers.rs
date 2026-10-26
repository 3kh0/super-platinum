use super::*;
pub(super) type Captures = serde_json::Map<String, Value>;
fn script(body: &str) -> String {
    format!(
        "(()=>{{{}\n{}\n{} }})()",
        capture::ACTION_GUARD,
        include_str!("dom.js"),
        body
    )
}
pub(super) async fn probe(action: &str, block: &str, args: Value) -> Result<Value, String> {
    let id = crate::view::app_modal::field_id(&existing::key(block));
    capture::dom(&script(&format!(
        "return datePopupFixture({},{},{});",
        json!(action),
        json!(id),
        args
    )))
    .await
}
pub(super) async fn observed(d: &DesktopContext) -> Result<(), String> {
    capture::wait(d,"[...(window.__spHomeCalendarNative?.entries.values()||[])].filter(e=>e.editableDate&&e.host?.isConnected).every(e=>e.nativeCurrent()&&!e.pending&&e.seq===e.acked&&e.snapshotValue()===e.ackedValue)").await?;
    Ok(())
}
pub(super) async fn settle(s: Signal<ShellState>, d: &DesktopContext) -> Result<(), String> {
    for _ in 0..300 {
        let x = snapshot(&s.peek());
        let current = ["activity", "windows"].iter().all(|key| {
            x[*key]
                .as_array()
                .is_some_and(|a| a.iter().all(|w| w["current"] == true))
        });
        if inputs::ready(&s.peek())
            && current
            && x["buttonProcessing"]
                .as_array()
                .is_some_and(|a| a.iter().all(|w| w["processing"] == false))
        {
            return observed(d).await;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err("Date popup inputs did not settle".into())
}
pub(super) fn parent(s: &ShellState, block: &str, expected: Option<&str>) -> Result<(), String> {
    let x = snapshot(s);
    if x["formState"]["values"][block]["value"]["selected_date"] != json!(expected) {
        return Err(format!("Date popup parent mismatch: {}", x["formState"]));
    }
    Ok(())
}
pub(super) async fn shot(
    s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    stage: &str,
    block: &str,
    mut proof: Value,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    observed(d).await?;
    let mut current = false;
    for _ in 0..200 {
        let model = snapshot(&s.peek());
        current = ["activity", "windows"].iter().all(|key| {
            model[*key].as_array().is_some_and(|rows| {
                rows.iter().all(|row| {
                    row["current"] == true
                        && (row["trailing"].is_null() || row["trailing"]["current"] == true)
                })
            })
        });
        if current {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    if !current {
        return Err("Date popup scheduler did not retire stale source work".into());
    }
    let id = crate::view::app_modal::field_id(&existing::key(block));
    let before = snapshot(&s.peek());
    proof["effect"] = probe("read", block, json!({})).await?;
    let mut g = crate::slack_links_check::wait_dom(
        d,
        &script(&format!("return datePopupGeometry({});", json!(id))),
    )
    .await?;
    capture::capture_image(d, n, stage, &mut g, cap).await?;
    let after = snapshot(&s.peek());
    if before != after {
        return Err(format!(
            "Date popup capture changed model: {}",
            json!({"before":before,"after":after})
        ));
    }
    st.push(json!({"stage":stage,"geometry":g,"model":after,"proof":proof}));
    Ok(())
}
