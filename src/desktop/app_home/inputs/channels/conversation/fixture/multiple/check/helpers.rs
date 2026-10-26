use super::*;
pub(super) type Captures = serde_json::Map<String, Value>;
pub(super) async fn ready(state: Signal<ShellState>) -> Result<(), String> {
    for _ in 0..180 {
        if crate::app_home::inputs::ready(&state.peek()) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err(format!(
        "Home channels/native not ready: {}",
        snapshot(&state.peek())
    ))
}
pub(super) async fn open(d: &DesktopContext) -> Result<(), String> {
    wait(
        d,
        "!!document.querySelector('#app-home [data-modal-users] input:not(:disabled)')",
    )
    .await?;
    click("#app-home [data-modal-users] input").await?;
    wait(d,"document.querySelector('#app-home [data-modal-users] input')?.getAttribute('aria-expanded')==='true'").await?;
    Ok(())
}
pub(super) async fn settle(
    state: Signal<ShellState>,
    count: usize,
    held: bool,
) -> Result<(), String> {
    for _ in 0..200 {
        let v = snapshot(&state.peek());
        if v["requests"].as_array().map(Vec::len) == Some(count)
            && v["pending"] == held
            && v["query"]["loading"] == held
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err(format!(
        "Channel query settlement: {}",
        snapshot(&state.peek())
    ))
}
pub(super) async fn search_rows(
    state: Signal<ShellState>,
    d: &DesktopContext,
    query: &str,
    count: usize,
) -> Result<(), String> {
    open(d).await?;
    type_query(query).await?;
    settle(state, count, false).await
}
pub(super) async fn choose(
    s: Signal<ShellState>,
    d: &DesktopContext,
    id: &str,
    values: &[&str],
) -> Result<(), String> {
    capture::settle_interaction(s, d).await?;
    let selector = format!("#app-home [data-entity-id='{}']", id);
    wait(d, &format!("!!document.querySelector({})", json!(selector))).await?;
    click(&selector).await?;
    selected(s, values, values).await
}
pub(super) async fn remove(
    s: Signal<ShellState>,
    d: &DesktopContext,
    id: &str,
    values: &[&str],
) -> Result<(), String> {
    capture::settle_interaction(s, d).await?;
    click(&format!("#app-home [data-remove-user='{id}']")).await?;
    selected(s, values, values).await
}
pub(super) async fn selected(
    s: Signal<ShellState>,
    visible: &[&str],
    committed: &[&str],
) -> Result<(), String> {
    for _ in 0..150 {
        let v = snapshot(&s.peek());
        if v["widget"]["visible"] == json!(visible)
            && v["formState"]["values"]["home-choice"]["value"]["selected_conversations"]
                == json!(committed)
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err(format!(
        "Wrong ordered visible/committed conversations: {}",
        snapshot(&s.peek())
    ))
}
pub(super) async fn collapse(s: Signal<ShellState>, d: &DesktopContext) -> Result<(), String> {
    click("[data-modal-users] .app-modal-users-actions button:last-child").await?;
    retired(s).await
}
/// Refresh retires the old picker through the real runtime before capture.
pub(super) async fn refreshed(s: Signal<ShellState>, d: &DesktopContext) -> Result<(), String> {
    ready(s).await?;
    retired(s).await?;
    capture::settle_interaction(s, d).await
}
pub(super) async fn cancelled(mut state: Signal<ShellState>) -> Result<Value, String> {
    for _ in 0..150 {
        if pending_closed(&state.peek()) {
            let result = release(&mut state.write());
            if result["delivered"] != false {
                return Err("Retired user response delivered".into());
            }
            return Ok(result);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("User receiver not retired".into())
}
pub(super) async fn retired(state: Signal<ShellState>) -> Result<(), String> {
    let expected = snapshot(&state.peek());
    for _ in 0..150 {
        let v = snapshot(&state.peek());
        for key in [
            "authority",
            "rawView",
            "formState",
            "retainedStates",
            "widget",
            "context",
            "mode",
            "channelPolicy",
            "channels",
            "members",
            "conversationFilter",
        ] {
            if v[key] != expected[key] {
                return Err(format!("Retirement changed {key}"));
            }
        }
        if v["query"].is_null() && v["pending"] == false {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Runtime did not retire users picker".into())
}
pub(super) async fn action(
    state: Signal<ShellState>,
    d: &DesktopContext,
    expected: &[&str],
) -> Result<Value, String> {
    ready(state).await?;
    let count = state.peek().app_home.fixture_forms.len();
    click("#app-home [aria-label='Review release']").await?;
    wait(d, "!!document.querySelector('.app-modal')").await?;
    let s = state.peek();
    let value = crate::app_home::inputs::form(&s)
        .ok_or("No form")?
        .state()
        .map_err(|e| e.to_string())?;
    if s.app_home.fixture_forms.len() != count + 1
        || value["values"]["home-choice"]["value"]["selected_conversations"] != json!(expected)
        || value["values"]["home-choice"]["value"]
            .get("response_url_enabled")
            .is_some()
    {
        return Err("Explicit Home channel action mismatch".into());
    }
    Ok(json!({"action":s.app_home.fixture_forms.last(),"selectedConversations":expected}))
}
pub(super) async fn close(d: &DesktopContext) -> Result<(), String> {
    click("[aria-label='Close app form']").await?;
    wait(d, "!document.querySelector('.app-modal')").await?;
    Ok(())
}
pub(super) async fn shot(
    state: Signal<ShellState>,
    d: &DesktopContext,
    name: &str,
    stage: &str,
    proof: Value,
    stages: &mut Vec<Value>,
    captures: &mut Captures,
) -> Result<(), String> {
    capture::shot(state, d, name, stage, proof, stages, captures).await?;
    if matches!(case().as_str(), "filter" | "recovery") {
        let model = stages.last().unwrap()["model"].clone();
        capture::unchanged(&state.peek(), &model, false)?;
        let mut header = dom(include_str!(
            "../../../../../../context/fixture/check/header.js"
        ))
        .await?;
        if header["present"] == true {
            let key = format!("{stage}-context");
            capture::capture_image(d, name, &key, &mut header, captures).await?;
            stages.last_mut().unwrap()["contextCapture"] =
                json!({"captureKey":key,"geometry":header});
        } else {
            stages.last_mut().unwrap()["contextCapture"] = Value::Null;
        }
        capture::unchanged(&state.peek(), &model, false)?;
    }
    Ok(())
}
