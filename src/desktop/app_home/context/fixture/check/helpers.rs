use super::*;
pub(super) async fn ready_inputs(
    state: Signal<ShellState>,
    d: &DesktopContext,
) -> Result<(), String> {
    capture::wait(
        d,
        "!!document.querySelector('[data-home-external] [data-external-open]')",
    )
    .await?;
    for _ in 0..150 {
        if inputs::ready(&state.peek()) && Witness::capture(&state.peek()).is_ok() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Current native Home form/context not ready".into())
}
pub(super) async fn settle(
    state: Signal<ShellState>,
    count: usize,
    held: bool,
) -> Result<(), String> {
    for _ in 0..180 {
        let model = external::snapshot(&state.peek());
        if model["requests"].as_array().map(Vec::len) == Some(count)
            && model["pending"] == held
            && model["query"]["loading"] == held
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err(format!(
        "Unified query settlement {}",
        external::snapshot(&state.peek())
    ))
}
pub(super) async fn query(
    state: Signal<ShellState>,
    d: &DesktopContext,
    text: &str,
    count: usize,
    held: bool,
) -> Result<(), String> {
    capture::wait(
        d,
        "!!document.querySelector('[data-external-open]:not(:disabled)')",
    )
    .await?;
    capture::click("[data-home-external] [data-external-open]").await?;
    capture::wait(
        d,
        "!!document.querySelector('[data-home-external] input[type=search]')",
    )
    .await?;
    capture::type_query(text).await?;
    settle(state, count, held).await?;
    Ok(())
}
pub(super) async fn select_beta(d: &DesktopContext) -> Result<(), String> {
    capture::wait(
        d,
        "document.querySelectorAll('[data-option-index]').length===2",
    )
    .await?;
    capture::click("[data-home-external] [data-option-index='1']").await?;
    capture::wait(d, "!document.querySelector('[data-external-search]')").await?;
    Ok(())
}
pub(super) async fn choose_team(d: &DesktopContext, team: &str) -> Result<(), String> {
    capture::wait(
        d,
        "!!document.querySelector('select[aria-label=\"App Home workspace\"]:not(:disabled)')",
    )
    .await?;
    let script = include_str!("choose.js").replace("__SP_TEAM__", &json!(team).to_string());
    capture::dom(&script).await?;
    Ok(())
}
pub(super) async fn route(state: Signal<ShellState>, expected: Option<&str>) -> Result<(), String> {
    for _ in 0..100 {
        if Witness::capture(&state.peek()).is_ok_and(|w| w.team_id() == expected) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Home route did not match selected source".into())
}
pub(super) fn unchanged_form(before: &Value, after: &Value) -> Result<(), String> {
    for field in ["formState", "rawView", "inputRevision", "authority"] {
        if before["external"][field] != after["external"][field] {
            return Err(format!("Context choice changed {field}"));
        }
    }
    Ok(())
}
pub(super) async fn native_identity() -> Result<Value, String> {
    capture::dom("(()=>{const book=window.__spHomeCalendarNative,e=document.querySelector('#app-home input[type=date]');if(!book||!e)return{ok:false};return{ok:true,value:e.value,entries:[...book.entries.entries()].map(([id,v])=>({id,mount:v.mount,seq:v.seq,acked:v.acked,pending:v.pending}))};})()").await
}
pub(super) async fn click_action() -> Result<(), String> {
    capture::click("#app-home [aria-label='Review release']").await?;
    Ok(())
}
pub(super) async fn action(d: &DesktopContext) -> Result<(), String> {
    click_action().await?;
    capture::wait(d, "!!document.querySelector('.app-modal')").await?;
    Ok(())
}
pub(super) async fn retired(state: Signal<ShellState>) -> Result<(), String> {
    let before = external::snapshot(&state.peek());
    for _ in 0..150 {
        let now = external::snapshot(&state.peek());
        for field in ["formState", "retainedStates", "authority"] {
            if before[field] != now[field] {
                return Err(format!("Retirement changed {field}"));
            }
        }
        if now["query"].is_null() && now["pending"] == false {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Actual runtime did not retire Home picker".into())
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
    capture::shot(
        state,
        d,
        name,
        stage,
        json!({"exercise":proof,"source":snapshot(&state.peek())}),
        stages,
        captures,
    )
    .await?;
    let mut header = capture::dom(include_str!("header.js")).await?;
    if header["present"] == true {
        let key = format!("{stage}-context");
        capture::capture_image(d, name, &key, &mut header, captures).await?;
        stages.last_mut().unwrap()["contextCapture"] = json!({"captureKey":key,"geometry":header});
    } else {
        stages.last_mut().unwrap()["contextCapture"] = Value::Null;
    }
    let current = snapshot(&state.peek());
    if stages.last().unwrap()["proof"]["source"] != current {
        return Err("Context/source changed during capture".into());
    }
    Ok(())
}

pub(super) async fn native_buffer(value: Option<&str>) -> Result<Value, String> {
    let value = value.map(|v| json!(v)).unwrap_or(json!("2026-11-02"));
    capture::dom(&format!(r#"(()=>{{{}const e=document.querySelector('#app-home input[type=date]');if(!owned(e))return{{ok:false}};const before=e.value;e.focus({{preventScroll:true}});Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,{});return{{ok:true,before,after:e.value,visibleHitTested:true,eventDispatched:false}};}})()"#,capture::ACTION_GUARD,value)).await
}
