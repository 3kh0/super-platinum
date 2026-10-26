use super::*;
pub(super) async fn open(d: &DesktopContext, block: &str) -> Result<Value, String> {
    let selector = format!("#app-home [data-modal-users='{block}'] input[type=search]");
    capture::wait(
        d,
        &format!(
            "!!document.querySelector({} + ':not(:disabled)')",
            json!(selector)
        ),
    )
    .await?;
    let proof = capture::click(&selector).await?;
    capture::wait(
        d,
        &format!(
            "document.querySelector({})?.getAttribute('aria-expanded')==='true'",
            json!(selector)
        ),
    )
    .await?;
    Ok(proof)
}
pub(super) async fn choose(
    s: Signal<ShellState>,
    d: &DesktopContext,
    block: &str,
    id: &str,
) -> Result<Value, String> {
    let opened = open(d, block).await?;
    let selector = format!("#app-home [data-modal-users='{block}'] [data-entity-id='{id}']");
    capture::wait(d, &format!("!!document.querySelector({})", json!(selector))).await?;
    let chosen = capture::click(&selector).await?;
    for _ in 0..150 {
        let x = snapshot(&s.peek());
        if x["formState"]["values"][block]["value"]["selected_user"] == id
            || (block == "multi"
                && x["multiVisible"]
                    .as_array()
                    .is_some_and(|a| a.contains(&json!(id))))
        {
            return Ok(json!({"open":opened,"choose":chosen,"block":block,"id":id}));
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    Err(format!("User choice did not commit {block}/{id}"))
}
pub(super) async fn clear(
    s: Signal<ShellState>,
    d: &DesktopContext,
    block: &str,
    id: &str,
) -> Result<Value, String> {
    let selector = format!("#app-home [data-modal-users='{block}'] [data-remove-user='{id}']");
    capture::wait(
        d,
        &format!(
            "!!document.querySelector({} + ':not(:disabled)')",
            json!(selector)
        ),
    )
    .await?;
    let proof = capture::click(&selector).await?;
    for _ in 0..150 {
        let x = snapshot(&s.peek());
        if x["formState"]["values"][block]["value"].get("selected_user") == Some(&Value::Null) {
            return Ok(proof);
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    Err("User clear did not commit explicit null".into())
}
pub(super) async fn notes(value: &str) -> Result<Value, String> {
    let id = crate::view::app_modal::field_id(&key("notes"));
    capture::dom(&format!(r#"(()=>{{{}const e=document.getElementById({});e?.scrollIntoView({{block:'center'}});if(!owned(e)||e.readOnly||window.__spHomeText?.activeLease)return{{ok:false}};const before=e.value;Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,{});e.dispatchEvent(new Event('input',{{bubbles:true}}));return{{ok:true,before,value:e.value,trusted:false,visibleHitTested:true}};}})()"#,capture::ACTION_GUARD,json!(id),json!(value))).await
}
pub(super) async fn search(
    s: Signal<ShellState>,
    d: &DesktopContext,
    block: &str,
    value: &str,
    count: usize,
) -> Result<Value, String> {
    let opened = open(d, block).await?;
    let selector = format!("#app-home [data-modal-users='{block}'] input[type=search]");
    let proof=capture::dom(&format!(r#"(()=>{{{}const e=document.querySelector({});e?.scrollIntoView({{block:'center'}});if(!owned(e))return{{ok:false}};Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,{});e.dispatchEvent(new Event('input',{{bubbles:true}}));return{{ok:true,value:e.value,trusted:false,visibleHitTested:true}};}})()"#,capture::ACTION_GUARD,json!(selector),json!(value))).await?;
    for _ in 0..250 {
        let x = snapshot(&s.peek());
        if x["directory"]["requests"].as_array().map(Vec::len) == Some(count)
            && x["directory"]["query"]["loading"] == false
        {
            return Ok(json!({"open":opened,"input":proof}));
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err("User directory request did not settle".into())
}
