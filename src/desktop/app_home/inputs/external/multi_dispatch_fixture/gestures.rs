use super::*;
pub(super) async fn open(
    s: Signal<ShellState>,
    d: &DesktopContext,
    block: &str,
) -> Result<Value, String> {
    let before = snapshot(&s.peek());
    if before["pickerBlock"] == block && before["suggestions"]["query"]["current"] == true {
        return Ok(Value::Null);
    }
    let selector = format!("#app-home [data-home-external='{block}'] [data-external-open]");
    capture::wait(
        d,
        &format!(
            "!!document.querySelector({} + ':not(:disabled)')",
            json!(selector)
        ),
    )
    .await?;
    let opened = capture::click(&selector).await?;
    let count = before["suggestions"]["requests"]
        .as_array()
        .map_or(0, Vec::len)
        + 1;
    suggestions_settled(s, count, false).await?;
    Ok(json!({"open":opened,"requests":count}))
}
pub(super) async fn suggestions_settled(
    s: Signal<ShellState>,
    count: usize,
    held: bool,
) -> Result<(), String> {
    for _ in 0..300 {
        let x = snapshot(&s.peek());
        let q = &x["suggestions"];
        if q["requests"].as_array().map(Vec::len) == Some(count)
            && q["pending"] == held
            && q["query"]["current"] == true
            && q["query"]["loading"] == held
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err("External suggestions did not settle at exact request count".into())
}
pub(super) async fn choose(
    s: Signal<ShellState>,
    d: &DesktopContext,
    block: &str,
    index: usize,
    expected: Value,
) -> Result<Value, String> {
    let opened = open(s, d, block).await?;
    let selector =
        format!("#app-home [data-home-external='{block}'] [data-option-index='{index}']");
    capture::wait(
        d,
        &format!(
            "!!document.querySelector({} + ':not(:disabled)')",
            json!(selector)
        ),
    )
    .await?;
    let chosen = capture::click(&selector).await?;
    for _ in 0..150 {
        let x = snapshot(&s.peek());
        if x["formState"]["values"][block]["value"]["selected_option"] == expected {
            return Ok(
                json!({"open":opened,"choose":chosen,"block":block,"index":index,"expected":expected}),
            );
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    Err(format!(
        "External choice did not commit exact descriptor for {block}"
    ))
}
pub(super) async fn clear(
    s: Signal<ShellState>,
    d: &DesktopContext,
    block: &str,
) -> Result<Value, String> {
    let selector = format!(
        "#app-home [data-home-external='{block}'] button[aria-label='Clear selected option']"
    );
    capture::wait(
        d,
        &format!(
            "!!document.querySelector({} + ':not(:disabled)')",
            json!(selector)
        ),
    )
    .await?;
    let cleared = capture::click(&selector).await?;
    for _ in 0..150 {
        if snapshot(&s.peek())["formState"]["values"][block]["value"].get("selected_option")
            == Some(&Value::Null)
        {
            return Ok(cleared);
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    Err("External clear did not commit null".into())
}
pub(super) async fn query(
    s: Signal<ShellState>,
    d: &DesktopContext,
    block: &str,
    text: &str,
    count: usize,
    held: bool,
) -> Result<Value, String> {
    let selector = format!("#app-home [data-home-external='{block}'] input[type=search]");
    let proof=capture::dom(&format!(r#"(()=>{{{}const e=document.querySelector({});if(!owned(e))return{{ok:false}};Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,{});e.dispatchEvent(new Event('input',{{bubbles:true}}));return{{ok:true,value:e.value,trusted:false,visibleHitTested:true}};}})()"#,capture::ACTION_GUARD,json!(selector),json!(text))).await?;
    suggestions_settled(s, count, held).await?;
    capture::wait(
        d,
        &format!(
            "document.querySelector({})?.value==={}",
            json!(selector),
            json!(text)
        ),
    )
    .await?;
    Ok(proof)
}
pub(super) async fn close(d: &DesktopContext, block: &str) -> Result<Value, String> {
    let selector = format!("#app-home [data-home-external='{block}'] [data-external-done]");
    capture::click(&selector).await
}
pub(super) async fn notes(value: &str) -> Result<Value, String> {
    let id = crate::view::app_modal::field_id(&key("notes"));
    capture::dom(&format!(r#"(()=>{{{}const e=document.getElementById({});e?.scrollIntoView({{block:'center'}});if(!owned(e)||e.readOnly||window.__spHomeText?.activeLease)return{{ok:false}};const before=e.value;Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,{});e.dispatchEvent(new Event('input',{{bubbles:true}}));return{{ok:true,before,value:e.value,trusted:false,visibleHitTested:true}};}})()"#,capture::ACTION_GUARD,json!(id),json!(value))).await
}
pub(super) async fn notes_received(s: Signal<ShellState>, value: &str) -> Result<Value, String> {
    let proof = notes(value).await?;
    for _ in 0..150 {
        if snapshot(&s.peek())["formState"]["values"]["notes"]["value"]["value"] == value {
            return Ok(proof);
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    Err("External sibling notes were not acknowledged".into())
}

pub(super) fn multi_widget(s: &ShellState, block: &str) -> Value {
    snapshot(s)["multiExternalWidgets"]
        .as_array()
        .and_then(|rows| rows.iter().find(|w| w["block"] == block))
        .map(|w| w["widget"].clone())
        .unwrap_or(Value::Null)
}
pub(super) async fn array_received(
    s: Signal<ShellState>,
    block: &str,
    visible: &Value,
    parent: &Value,
) -> Result<(), String> {
    for _ in 0..150 {
        let x = snapshot(&s.peek());
        if multi_widget(&s.peek(), block)["visible"] == *visible
            && x["formState"]["values"][block]["value"]["selected_options"] == *parent
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    Err(format!(
        "Multi external array did not settle for {block}: {}",
        snapshot(&s.peek())
    ))
}
pub(super) async fn toggle(
    s: Signal<ShellState>,
    d: &DesktopContext,
    block: &str,
    index: usize,
    visible: Value,
    parent: Value,
) -> Result<Value, String> {
    let opened = open(s, d, block).await?;
    let selector =
        format!("#app-home [data-home-external='{block}'] [data-option-index='{index}']");
    capture::wait(
        d,
        &format!(
            "!!document.querySelector({} + ':not(:disabled)')",
            json!(selector)
        ),
    )
    .await?;
    let chosen = capture::click(&selector).await?;
    array_received(s, block, &visible, &parent).await?;
    Ok(
        json!({"open":opened,"choose":chosen,"block":block,"index":index,"visible":visible,"parent":parent}),
    )
}
pub(super) async fn remove(
    s: Signal<ShellState>,
    d: &DesktopContext,
    block: &str,
    value: &str,
    visible: Value,
    parent: Value,
) -> Result<Value, String> {
    let selector = format!(
        "#app-home [data-home-external='{block}'] [data-option-value={}]",
        json!(value)
    );
    capture::wait(
        d,
        &format!(
            "!!document.querySelector({} + ':not(:disabled)')",
            json!(selector)
        ),
    )
    .await?;
    let removed = capture::click(&selector).await?;
    array_received(s, block, &visible, &parent).await?;
    Ok(json!({"remove":removed,"block":block,"value":value,"visible":visible,"parent":parent}))
}
pub(super) fn suggestion_count(s: &ShellState) -> usize {
    snapshot(s)["suggestions"]["requests"]
        .as_array()
        .map_or(0, Vec::len)
}
pub(super) fn empty_value() -> Value {
    suggestions::option("Empty value release", "")
}
pub(super) fn projected(v: Value) -> Value {
    json!({"text":v["text"],"value":v["value"]})
}
pub(super) fn trailing_before(
    s: &ShellState,
    started: Instant,
    expected: usize,
) -> Result<Value, String> {
    let x = snapshot(s);
    let elapsed = started.elapsed().as_millis();
    if elapsed >= 450 || x["requests"].as_array().map(Vec::len) != Some(expected) {
        return Err("Multi external callbacks missed trailing-only window".into());
    }
    Ok(json!({"elapsedMs":elapsed,"model":x}))
}
