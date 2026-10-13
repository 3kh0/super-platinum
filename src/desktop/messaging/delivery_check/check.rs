//! Native checks drive the real action; fixture completions cannot perform I/O.
use super::*;
use serde_json::{Value, json};
const BUTTON: &str = "[data-upload-delivery='1999999999.000001'] .upload-delivery-check";
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if std::env::var("SUPER_PLATINUM_UPLOAD_DELIVERY_CHECK").as_deref() != Ok("1")
        || !matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("upload-delivery-check" | "upload-delivery-check-narrow")
        )
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let receipt = match run(state, &desktop).await {
            Ok(v) => v,
            Err(e) => json!({"ok":false,"error":e}),
        };
        eprintln!("upload-delivery-check: {receipt}");
    });
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    if !fixture::enabled(&state.peek()) {
        return Err("Connected fixture refused".into());
    }
    let initial = geometry(desktop, BUTTON).await?;
    click(desktop, BUTTON).await?;
    wait_dom(
        desktop,
        &format!(
            "({{ok:document.querySelector({})?.disabled===true}})",
            json!(BUTTON)
        ),
    )
    .await?;
    if fixture::calls(&state.peek()) != 1 {
        return Err("First check was not admitted exactly once".into());
    }
    // Disabled native button plus controller admission both reject duplicates.
    eval(
        desktop,
        &format!("document.querySelector({}).click();true", json!(BUTTON)),
    )
    .await?;
    let old = fixture::action(&state.peek(), fixture::FIRST).unwrap();
    super::check(state, old.clone());
    if fixture::calls(&state.peek()) != 1 {
        return Err("Duplicate check admitted".into());
    }
    fixture::complete(state, Ok(HistoryPage::default()));
    wait_dom(desktop,&format!("({{ok:document.querySelector({})?.disabled===false&&document.querySelector('.upload-delivery-hint')?.textContent.includes('still unconfirmed')}})",json!(BUTTON))).await?;
    if state.peek().core.pending_file_messages.len() != 2 {
        return Err("Missing readback removed upload".into());
    }
    click(desktop, BUTTON).await?;
    wait_calls(state, 2).await?;
    // Navigation retires this read's painting authority, not its cooldown.
    state.write().channel_generation += 1;
    fixture::complete(
        state,
        Err(Error::RateLimited {
            retry_after_secs: Some(1),
        }),
    );
    wait_dom(desktop,&format!("({{ok:document.querySelector({})?.disabled===true&&document.querySelector('.upload-delivery-hint')?.textContent.includes('wait')}})",json!(BUTTON))).await?;
    let reopened = fixture::action(&state.peek(), fixture::FIRST).unwrap();
    super::check(state, reopened);
    let other = fixture::action(&state.peek(), fixture::SECOND).unwrap();
    super::check(state, other);
    if fixture::calls(&state.peek()) != 2 {
        return Err("Reopen or second receipt bypassed Retry-After".into());
    }
    wait_dom(
        desktop,
        &format!(
            "({{ok:document.querySelector({})?.disabled===false}})",
            json!(BUTTON)
        ),
    )
    .await?;
    super::check(state, old);
    if fixture::calls(&state.peek()) != 2 {
        return Err("Retired callback admitted".into());
    }
    click(desktop, BUTTON).await?;
    wait_calls(state, 3).await?;
    let page = fixture::canonical(&state.peek(), fixture::FIRST);
    fixture::complete(state, Ok(page));
    wait_dom(desktop,&format!("({{ok:!document.querySelector({})&&document.body.textContent.includes('Confirmed in the conversation.')}})",json!(BUTTON))).await?;
    {
        let shell = state.peek();
        if shell.core.pending_file_messages.len() != 1
            || shell.core.pending_file_messages[0].client_msg_id != fixture::SECOND
            || shell.core.session.is_some()
            || shell.core.transport.is_some()
        {
            return Err("Canonical readback affected wrong owner or connected fixture".into());
        }
    }
    state.write().toast = None;
    let mut result = geometry(
        desktop,
        "[data-upload-delivery='1999999999.000002'] .upload-delivery-check",
    )
    .await?;
    result["initialVisible"] = initial["eachVisible"].clone();
    for flag in [
        "readOnly",
        "duplicateBlocked",
        "missingRetained",
        "retryAfterRetained",
        "staleVisitRejected",
        "exactReadbackOnly",
        "noTransport",
    ] {
        result[flag] = json!(true);
    }
    result["requests"] = json!(fixture::calls(&state.peek()));
    Ok(result)
}
async fn wait_calls(state: Signal<ShellState>, count: usize) -> Result<(), String> {
    for _ in 0..50 {
        if fixture::calls(&state.peek()) == count {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err("Delivery check was not dispatched".into())
}
async fn click(desktop: &dioxus::desktop::DesktopContext, selector: &str) -> Result<(), String> {
    wait_dom(desktop,&format!("(()=>{{const b=document.querySelector({});if(!b||b.disabled)return {{ok:false}};b.scrollIntoView({{block:'nearest'}});b.focus();b.click();return {{ok:true}};}})()",json!(selector))).await?;
    Ok(())
}
async fn geometry(
    desktop: &dioxus::desktop::DesktopContext,
    selector: &str,
) -> Result<Value, String> {
    wait_dom(desktop,&format!(r#"(()=>{{const b=document.querySelector({});if(!b)return {{ok:false}};b.scrollIntoView({{block:'nearest'}});b.focus();const row=b.closest('.upload-delivery'),hint=row.querySelector('.upload-delivery-hint');const visible=e=>{{const r=e.getBoundingClientRect();return r.width>0&&r.height>0&&r.left>=0&&r.right<=innerWidth+1&&r.top>=0&&r.bottom<=innerHeight+1;}};const eachVisible=[b,hint].every(visible);return {{ok:eachVisible&&document.activeElement===b,eachVisible,keyboardFocused:document.activeElement===b,noOverflow:row.scrollWidth<=row.clientWidth+1,viewport:[innerWidth,innerHeight]}};}})()"#,json!(selector))).await
}
async fn wait_dom(
    desktop: &dioxus::desktop::DesktopContext,
    script: &str,
) -> Result<Value, String> {
    let mut last = Value::Null;
    for _ in 0..60 {
        last = eval(desktop, script).await?;
        if last["ok"] == true {
            return Ok(last);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(format!("Delivery check DOM did not settle: {last}"))
}
async fn eval(desktop: &dioxus::desktop::DesktopContext, script: &str) -> Result<Value, String> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    desktop
        .webview
        .evaluate_script_with_callback(script, move |v| {
            let _ = tx.send(v);
        })
        .map_err(|_| "Evaluation failed")?;
    let raw = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .map_err(|_| "Evaluation timeout")?
        .ok_or("Callback closed")?;
    let v: Value = serde_json::from_str(&raw).map_err(|_| "Invalid evaluation")?;
    match v {
        Value::String(s) => {
            serde_json::from_str(&s).map_err(|_| "Invalid encoded evaluation".into())
        }
        v => Ok(v),
    }
}
