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
        "Home users/native not ready: {}",
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
        "User query settlement: {}",
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
    state: Signal<ShellState>,
    d: &DesktopContext,
    id: &str,
    expected: &[&str],
) -> Result<(), String> {
    capture::settle_interaction(state, d).await?;
    let selector = format!("#app-home [data-entity-id='{id}']");
    wait(d, &format!("!!document.querySelector({})", json!(selector))).await?;
    click(&selector).await?;
    selected(state, expected, expected).await
}
pub(super) async fn remove(
    state: Signal<ShellState>,
    d: &DesktopContext,
    id: &str,
    expected: &[&str],
) -> Result<(), String> {
    capture::settle_interaction(state, d).await?;
    click(&format!("#app-home [data-remove-user='{id}']")).await?;
    selected(state, expected, expected).await
}
pub(super) async fn selected(
    state: Signal<ShellState>,
    visible: &[&str],
    committed: &[&str],
) -> Result<(), String> {
    for _ in 0..150 {
        let v = snapshot(&state.peek());
        if v["widget"]["visible"] == json!(visible)
            && v["formState"]["values"]["home-choice"]["value"]["selected_users"]
                == json!(committed)
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err(format!(
        "Wrong ordered multi-user state: {}",
        snapshot(&state.peek())
    ))
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
        if v["retainedStates"] != expected["retainedStates"]
            || v["authority"] != expected["authority"]
        {
            return Err("Retirement changed owner/form".into());
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
    // Close the local picker through its real Done action before opening the
    // overlay. The widget stays mounted; no stale query may race the capture.
    if snapshot(&state.peek())["query"].is_object() {
        click("#app-home .app-modal-users-actions button:last-child").await?;
        for _ in 0..150 {
            if snapshot(&state.peek())["query"].is_null() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        if !snapshot(&state.peek())["query"].is_null() {
            return Err("Picker did not close before action".into());
        }
    }
    capture::settle_interaction(state, d).await?;
    let count = state.peek().app_home.fixture_forms.len();
    click("#app-home [aria-label='Review release']").await?;
    wait(d, "!!document.querySelector('.app-modal')").await?;
    let s = state.peek();
    let value = crate::app_home::inputs::form(&s)
        .ok_or("No form")?
        .state()
        .map_err(|e| e.to_string())?;
    if s.app_home.fixture_forms.len() != count + 1
        || value["values"]["home-choice"]["value"]["selected_users"] != json!(expected)
    {
        return Err("Explicit Home user action mismatch".into());
    }
    Ok(json!({"action":s.app_home.fixture_forms.last(),"selectedUsers":expected}))
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
    if case() == "retirement" {
        let model = stages.last().unwrap()["model"].clone();
        capture::unchanged(&state.peek(), &model, false)?;
        let mut header = dom(include_str!(
            "../../../../../context/fixture/check/header.js"
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
