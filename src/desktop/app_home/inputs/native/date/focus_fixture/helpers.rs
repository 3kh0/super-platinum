use super::*;
pub(super) type Captures = serde_json::Map<String, Value>;
fn script(body: &str) -> String {
    let fields = FIELDS
        .iter()
        .map(|b| json!({"block":b,"id":id(b)}))
        .collect::<Vec<_>>();
    format!(
        "(()=>{{{}const fields={};{}\n{} }})()",
        capture::ACTION_GUARD,
        json!(fields),
        include_str!("dom.js"),
        body
    )
}
pub(super) async fn probe(action: &str, block: &str, args: Value) -> Result<Value, String> {
    capture::dom(&script(&format!(
        "return dateFocusFixture({},{},{});",
        json!(action),
        json!(id(block)),
        args
    )))
    .await
}
pub(super) async fn effect(block: &str) -> Result<Value, String> {
    Ok(probe("read", block, json!({})).await?["effect"].clone())
}
pub(super) async fn settle(s: Signal<ShellState>, d: &DesktopContext) -> Result<(), String> {
    for _ in 0..300 {
        if inputs::ready(&s.peek()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    if !inputs::ready(&s.peek()) {
        return Err("Date focus inputs did not settle".into());
    }
    observed(s, d).await
}
/// Exact acknowledged buffers, including deliberately unresolved editing text.
/// This is observation only; it grants no form readiness or lease authority.
pub(super) async fn observed(s: Signal<ShellState>, d: &DesktopContext) -> Result<(), String> {
    let expected = snapshot(&s.peek())["focusWidgets"].clone();
    let code = format!(
        r#"const home=document.getElementById('app-home'),expected={};return{{ok:!!home&&expected.every(w=>{{const h=window.__spHomeText?.focus?.get(w.instance);return h&&h.started&&h.current()&&h.config.flag===w.flag&&h.config.visit===w.visit;}})&&[...(window.__spHomeCalendarNative?.entries.values()||[])].filter(e=>home.contains(e.node)).every(e=>e.nativeCurrent()&&!e.pending&&e.seq===e.acked&&e.snapshotValue()===e.ackedValue)}};"#,
        expected
    );
    crate::slack_links_check::wait_dom(d, &script(&code)).await?;
    Ok(())
}
pub(super) async fn active(d: &DesktopContext, block: &str, wanted: bool) -> Result<Value, String> {
    let code = format!(
        "const p=dateFocusFixture('read',{},{{}});return{{ok:p.effect.target?.active==={},effect:p.effect}};",
        json!(id(block)),
        wanted
    );
    Ok(crate::slack_links_check::wait_dom(d, &script(&code)).await?["effect"].clone())
}
pub(super) fn no_field_focus(e: &Value) -> Result<(), String> {
    if e["fields"]
        .as_array()
        .is_none_or(|rows| rows.iter().any(|r| r["field"]["active"] == true))
    {
        return Err("An unrequested Date fixture field took focus".into());
    }
    Ok(())
}
pub(super) fn selected_all(e: &Value) -> Result<(), String> {
    let f = &e["target"];
    let length = f["value"]
        .as_str()
        .ok_or("Date raw text missing")?
        .encode_utf16()
        .count() as u64;
    if f["active"] != true
        || f["selection"]["start"] != 0
        || f["selection"]["end"] != length
        || f["pickerStatus"] != "idle"
    {
        return Err(format!(
            "Date focus did not select full editing label without picker: {f}"
        ));
    }
    Ok(())
}
pub(super) fn flags(s: &mut ShellState, target: Option<&str>) -> Value {
    existing::helpers::refresh(s, |raw| {
        for b in raw["blocks"].as_array_mut().unwrap() {
            if b["type"] == "input" {
                b["element"]["focus_on_load"] =
                    json!(Some(b["block_id"].as_str().unwrap_or_default()) == target);
            }
        }
    })
}
pub(super) fn widget(s: &ShellState, block: &str) -> Value {
    snapshot(s)["focusWidgets"]
        .as_array()
        .and_then(|a| a.iter().find(|w| w["block"] == block))
        .cloned()
        .unwrap_or(Value::Null)
}
pub(super) async fn quiet(s: Signal<ShellState>) -> Result<Value, String> {
    let before = snapshot(&s.peek());
    tokio::time::sleep(Duration::from_millis(1100)).await;
    let after = snapshot(&s.peek());
    for k in ["events", "requests", "responses"] {
        if before[k] != after[k] {
            return Err(format!("Date focus-only interval changed {k}"));
        }
    }
    if before["buttonSync"] != after["buttonSync"] {
        return Err("Date focus interval changed Button ledger".into());
    }
    Ok(json!({"before":before,"after":after,"waitMs":1100}))
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
    // Source replacement retires scheduler rows on the normal production tick.
    // Observe that retirement before freezing the full capture model, including
    // deliberately unresolved raw buffers which cannot use form-ready settling.
    let mut scheduler_current = false;
    for _ in 0..200 {
        let model = snapshot(&s.peek());
        scheduler_current = ["activity", "windows"].iter().all(|key| {
            model[*key].as_array().is_some_and(|rows| {
                rows.iter().all(|row| {
                    row["current"] == true
                        && (row["trailing"].is_null() || row["trailing"]["current"] == true)
                })
            })
        });
        if scheduler_current {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    if !scheduler_current {
        return Err(format!(
            "Date capture scheduler did not settle: {}",
            snapshot(&s.peek())
        ));
    }
    let before = snapshot(&s.peek());
    proof["effect"] = effect(block).await?;
    let code = script(&format!("return dateFocusGeometry({});", json!(id(block))));
    let mut g = crate::slack_links_check::wait_dom(d, &code).await?;
    if g["before"]["active"] != g["after"]["active"]
        || g["before"]["activeField"]["selection"] != g["after"]["activeField"]["selection"]
        || g["before"]["fields"] != g["after"]["fields"]
    {
        return Err("Date capture scrolling changed actual focus/range/buffer".into());
    }
    capture::capture_image(d, n, stage, &mut g, cap).await?;
    let after = snapshot(&s.peek());
    if before != after {
        let changed = before
            .as_object()
            .into_iter()
            .flat_map(|o| o.keys())
            .chain(after.as_object().into_iter().flat_map(|o| o.keys()))
            .filter(|key| before[*key] != after[*key])
            .collect::<std::collections::BTreeSet<_>>();
        return Err(format!(
            "Date capture changed model: {}",
            json!({"changedKeys":changed,"before":before,"after":after})
        ));
    }
    st.push(json!({"stage":stage,"geometry":g,"model":after,"proof":proof}));
    Ok(())
}
