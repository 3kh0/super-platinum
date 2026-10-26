use super::*;
pub(super) async fn ready(state: Signal<ShellState>, _d: &DesktopContext) -> Result<(), String> {
    for _ in 0..150 {
        if crate::app_home::inputs::ready(&state.peek()) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err(format!(
        "Multi Home/native input not ready: {}",
        snapshot(&state.peek())
    ))
}
pub(super) async fn open(d: &DesktopContext) -> Result<(), String> {
    wait(
        d,
        "!!document.querySelector('[data-home-external] [data-external-open]:not(:disabled)')",
    )
    .await?;
    click("[data-home-external] [data-external-open]").await?;
    wait(
        d,
        "!!document.querySelector('[data-home-external] input[type=search]')",
    )
    .await?;
    Ok(())
}
pub(super) async fn done(d: &DesktopContext) -> Result<(), String> {
    click("[data-external-done]").await?;
    wait(d, "!document.querySelector('[data-external-search]')").await?;
    Ok(())
}
pub(super) async fn settle(
    state: Signal<ShellState>,
    count: usize,
    held: bool,
) -> Result<(), String> {
    for _ in 0..180 {
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
        "Multi query settlement: {}",
        snapshot(&state.peek())
    ))
}
pub(super) async fn selected(
    state: Signal<ShellState>,
    values: &[&str],
    accepted: &[&str],
) -> Result<(), String> {
    for _ in 0..100 {
        let v = snapshot(&state.peek());
        let ids = |a: &Value| {
            a.as_array().map(|a| {
                a.iter()
                    .map(|v| v["value"].as_str().unwrap_or("").to_owned())
                    .collect::<Vec<_>>()
            })
        };
        if ids(&v["widget"]["visible"]) == Some(values.iter().map(|v| v.to_string()).collect())
            && ids(&v["widget"]["accepted"])
                == Some(accepted.iter().map(|v| v.to_string()).collect())
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err(format!(
        "Wrong widget/accepted state: {}",
        snapshot(&state.peek())
    ))
}
pub(super) async fn row(d: &DesktopContext, index: usize) -> Result<(), String> {
    let selector = format!("[data-home-external] [data-option-index='{index}']");
    wait(d, &format!("!!document.querySelector({})", json!(selector))).await?;
    click(&selector).await?;
    Ok(())
}
pub(super) async fn remove(value: &str) -> Result<(), String> {
    click(&format!(
        "[data-home-external] [data-option-value={}]",
        json!(value)
    ))
    .await?;
    Ok(())
}
pub(super) async fn cancelled(mut state: Signal<ShellState>) -> Result<Value, String> {
    for _ in 0..150 {
        if fixture::pending_closed(&state.peek()) {
            let proof = fixture::release(&mut state.write());
            if proof["delivered"] != false {
                return Err("Stale reply delivered".into());
            }
            return Ok(proof);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Old query receiver retained".into())
}
pub(super) async fn retired(state: Signal<ShellState>) -> Result<(), String> {
    let before = snapshot(&state.peek());
    for _ in 0..150 {
        let now = snapshot(&state.peek());
        if now["retainedStates"] != before["retainedStates"]
            || now["authority"] != before["authority"]
        {
            return Err("Retirement changed retained state/owner".into());
        }
        if now["query"].is_null() && now["pending"] == false {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Runtime did not retire picker".into())
}
pub(super) async fn action(
    state: Signal<ShellState>,
    d: &DesktopContext,
    expected: &[&str],
) -> Result<Value, String> {
    ready(state, d).await?;
    let before = state.peek().app_home.fixture_forms.len();
    click("#app-home [aria-label='Review release']").await?;
    wait(d, "!!document.querySelector('.app-modal')").await?;
    let s = state.peek();
    if s.app_home.fixture_forms.len() != before + 1 {
        return Err("Explicit action count".into());
    }
    let proof = s.app_home.fixture_forms.last().unwrap().clone();
    let form = crate::app_home::inputs::form(&s)
        .ok_or("No form after action")?
        .state()
        .map_err(|e| e.to_string())?;
    let values = form["values"]["home-choice"]["value"]["selected_options"]
        .as_array()
        .ok_or("No accepted multi array")?;
    if values
        .iter()
        .map(|v| v["value"].as_str().unwrap_or(""))
        .collect::<Vec<_>>()
        != expected
    {
        return Err("Wrong accepted action array".into());
    }
    Ok(json!({"action":proof,"accepted":values}))
}
pub(super) async fn close_modal(d: &DesktopContext) -> Result<(), String> {
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
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    capture::shot(state, d, name, stage, proof, stages, captures).await?;
    if unified() {
        capture::unchanged(&state.peek(), &stages.last().unwrap()["model"])?;
        let mut header = dom(include_str!("../../../../context/fixture/check/header.js")).await?;
        if header["present"] == true {
            let key = format!("{stage}-context");
            capture::capture_image(d, name, &key, &mut header, captures).await?;
            stages.last_mut().unwrap()["contextCapture"] =
                json!({"captureKey":key,"geometry":header});
        } else {
            stages.last_mut().unwrap()["contextCapture"] = Value::Null;
        }
    }
    capture::unchanged(&state.peek(), &stages.last().unwrap()["model"])?;
    Ok(())
}
