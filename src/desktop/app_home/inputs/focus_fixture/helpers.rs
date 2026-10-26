use super::*;
pub(super) type Captures = serde_json::Map<String, Value>;
fn script(body: &str) -> String {
    let fields = FIELDS
        .iter()
        .chain(["day"].iter())
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
        "return focusFixture({},{},{});",
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
    for _ in 0..200 {
        if inputs::ready(&s.peek()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    if !inputs::ready(&s.peek()) {
        return Err("Focus inputs did not become ready".into());
    }
    let x = snapshot(&s.peek());
    let expected = x["focusWidgets"].clone();
    let code = format!(
        r#"const expected={};const home=document.getElementById('app-home'),book=window.__spHomeText;return{{ok:!!home&&expected.every(w=>{{const h=book?.focus?.get(w.instance);return h&&h.config.kind===w.kind&&h.config.flag===w.flag&&h.config.visit===w.visit&&h.started&&h.current();}})&&[...(book?.entries?.values()||[])].filter(e=>home.contains(e.node)).every(e=>!e.closed&&e.seq===e.acked)}};"#,
        expected
    );
    crate::slack_links_check::wait_dom(d, &script(&code)).await?;
    Ok(())
}
pub(super) async fn active(d: &DesktopContext, block: &str, want: bool) -> Result<Value, String> {
    let code = format!(
        "const p=focusFixture('read',{},{{}});return{{ok:p.effect.target?.active==={},effect:p.effect}};",
        json!(id(block)),
        want
    );
    Ok(crate::slack_links_check::wait_dom(d, &script(&code)).await?["effect"].clone())
}
pub(super) fn no_field_focus(e: &Value) -> Result<(), String> {
    if e["fields"]
        .as_array()
        .is_none_or(|rows| rows.iter().any(|r| r["field"]["active"] == true))
    {
        return Err("An unrequested Home field took focus".into());
    }
    Ok(())
}
pub(super) fn refresh(s: &mut ShellState, change: impl FnOnce(&mut Value)) -> Value {
    let h = s.app_home.surface.as_ref().unwrap();
    let team = h.owner.ctx.session.team_id.clone();
    let rt = h.owner.rt;
    let before = h.revision;
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    change(&mut raw);
    raw["hash"] = json!(format!("focus-refresh-{before}"));
    let event = RtEvent::Unknown(
        serde_json::from_value(
            json!({"type":"view_updated","app_id":"A_HOME","view_id":"V_HOME","view":raw}),
        )
        .unwrap(),
    );
    crate::app_home::realtime(s, &team, rt, &event);
    json!({"before":before,"after":s.app_home.surface.as_ref().unwrap().revision})
}
pub(super) fn flags(s: &mut ShellState, target: Option<&str>) -> Value {
    refresh(s, |raw| {
        for b in raw["blocks"].as_array_mut().unwrap() {
            if b["type"] == "input" && b["block_id"] != "day" {
                b["element"]["focus_on_load"] = json!(target.is_some_and(|id| b["block_id"] == id));
            }
        }
    })
}
pub(super) fn widget(x: &Value, block: &str) -> Value {
    x["focusWidgets"]
        .as_array()
        .and_then(|rows| rows.iter().find(|w| w["block"] == block))
        .cloned()
        .unwrap_or(Value::Null)
}
pub(super) async fn quiet(s: Signal<ShellState>, block: &str) -> Result<Value, String> {
    let before = snapshot(&s.peek());
    let first = effect(block).await?;
    tokio::time::sleep(Duration::from_millis(700)).await;
    let after = snapshot(&s.peek());
    let last = effect(block).await?;
    for k in ["requests", "responses", "events"] {
        if before[k] != after[k] {
            return Err(format!("Focus-only silence changed {k}"));
        }
    }
    Ok(json!({"before":before,"after":after,"first":first,"last":last,"waitMs":700}))
}
pub(super) async fn counts(
    s: Signal<ShellState>,
    requests: usize,
    responses: usize,
) -> Result<(), String> {
    for _ in 0..200 {
        let x = snapshot(&s.peek());
        if x["requests"].as_array().map(Vec::len) == Some(requests)
            && x["responses"].as_array().map(Vec::len) == Some(responses)
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err("Focus fixture request counts did not settle".into())
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
    // A held email reply outlives its leading debounce window. Observe that
    // finite window's expiry before freezing a model for a screenshot.
    for _ in 0..200 {
        let x = snapshot(&s.peek());
        if x["emailWindows"].is_null() || x["emailWindows"] == json!([]) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let windows = snapshot(&s.peek())["emailWindows"].clone();
    if !windows.is_null() && windows != json!([]) {
        return Err("Focus capture still has a finite email window".into());
    }
    let before = snapshot(&s.peek());
    proof["effect"] = effect(block).await?;
    let code = script(&format!(
        "return focusFixtureGeometry({});",
        json!(id(block))
    ));
    let mut g = crate::slack_links_check::wait_dom(d, &code).await?;
    if g["before"]["active"] != g["after"]["active"]
        || g["before"]["activeField"]["selection"] != g["after"]["activeField"]["selection"]
    {
        return Err("Capture scrolling changed focus or selection".into());
    }
    capture::capture_image(d, n, stage, &mut g, cap).await?;
    let after = snapshot(&s.peek());
    if before != after {
        return Err("Focus capture changed Home model".into());
    }
    st.push(json!({"stage":stage,"geometry":g,"model":after,"proof":proof}));
    Ok(())
}
