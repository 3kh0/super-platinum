use super::*;
pub(super) async fn numeric(
    s: Signal<ShellState>,
    d: &DesktopContext,
    block: &str,
    kind: &str,
    value: &str,
    local: Value,
    parent: Value,
) -> Result<Value, String> {
    let id = crate::view::app_modal::field_id(&key(block));
    let script = format!(
        r#"(()=>{{{}const id={},kind={},requested={},n=document.getElementById(id),book=window.__spHomeText,e=[...(book?.entries.values()||[])].find(e=>e.node===n);n?.scrollIntoView({{block:'center'}});if(!owned(n)||n.type!=='number'||!e||!e.number||e.closed||e.seq!==e.acked||e.lease||book.activeLease||n.readOnly||n.disabled)return{{ok:false}};const before={{mount:e.mount,seq:e.seq,value:n.value,observed:e.value,local:e.local}};let delivered=null,prevented=false;if(kind==='input'){{Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(n,requested);delivered=n.value;if(delivered!==requested)return{{ok:false,error:'Native number sanitized requested fixture value',requested,delivered}};n.dispatchEvent(new InputEvent('input',{{bubbles:true,inputType:'insertText',data:null}}));}}else if(kind==='enter'){{n.focus({{preventScroll:true}});const event=new KeyboardEvent('keydown',{{key:'Enter',code:'Enter',bubbles:true,cancelable:true}});n.dispatchEvent(event);prevented=event.defaultPrevented;}}else if(kind==='blur'){{n.focus({{preventScroll:true}});n.blur();}}else return{{ok:false}};return{{ok:true,id,kind,requested,delivered,before,after:{{mount:e.mount,seq:e.seq,value:n.value,observed:e.value,local:e.local}},prevented,trusted:false,visibleHitTested:true,validity:{{badInput:n.validity.badInput,stepMismatch:n.validity.stepMismatch,rangeUnderflow:n.validity.rangeUnderflow,rangeOverflow:n.validity.rangeOverflow,valid:n.validity.valid}}}};}})()"#,
        capture::ACTION_GUARD,
        json!(id),
        json!(kind),
        json!(value)
    );
    let proof = capture::dom(&script).await?;
    if proof["after"]["seq"].as_u64() != proof["before"]["seq"].as_u64().map(|v| v + 1) {
        return Err(format!(
            "Numeric gesture was not one observed occurrence: {proof}"
        ));
    }
    let expected = proof["after"].clone();
    crate::slack_links_check::wait_dom(d,&format!(r#"(()=>{{const x={},e=window.__spHomeText?.entries.get(x.mount);return{{ok:!!e&&!e.closed&&!e.lease&&e.seq===x.seq&&e.acked===x.seq&&e.node.value===x.value}}}})()"#,expected)).await?;
    for _ in 0..150 {
        let x = snapshot(&s.peek());
        let widget = x["numberWidgets"]
            .as_array()
            .and_then(|r| r.iter().find(|r| r["block"] == block));
        if widget.is_some_and(|w| w["local"] == local)
            && x["formState"]["values"][block]["value"] == parent
        {
            return Ok(proof);
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    Err(format!(
        "Number local/parent did not settle for {block}: {}",
        snapshot(&s.peek())
    ))
}
pub(super) async fn input(
    s: Signal<ShellState>,
    d: &DesktopContext,
    block: &str,
    value: &str,
    local: Value,
    parent: Value,
) -> Result<Value, String> {
    numeric(s, d, block, "input", value, local, parent).await
}
pub(super) async fn enter(
    s: Signal<ShellState>,
    d: &DesktopContext,
    block: &str,
    local: Value,
    parent: Value,
) -> Result<Value, String> {
    numeric(s, d, block, "enter", "", local, parent).await
}
pub(super) async fn blur(
    s: Signal<ShellState>,
    d: &DesktopContext,
    block: &str,
    local: Value,
    parent: Value,
) -> Result<Value, String> {
    numeric(s, d, block, "blur", "", local, parent).await
}
pub(super) fn wire(value: Value) -> Value {
    json!({"type":"number_input","value":value})
}
pub(super) async fn notes(s: Signal<ShellState>, value: &str) -> Result<Value, String> {
    let id = crate::view::app_modal::field_id(&key("notes"));
    let p=capture::dom(&format!(r#"(()=>{{{}const n=document.getElementById({});n?.scrollIntoView({{block:'center'}});if(!owned(n)||n.readOnly||window.__spHomeText?.activeLease)return{{ok:false}};Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(n,{});n.dispatchEvent(new InputEvent('input',{{bubbles:true,inputType:'insertText',data:null}}));return{{ok:true,value:n.value,trusted:false,visibleHitTested:true,focusRequested:false}};}})()"#,capture::ACTION_GUARD,json!(id),json!(value))).await?;
    for _ in 0..150 {
        if snapshot(&s.peek())["formState"]["values"]["notes"]["value"]["value"] == value {
            return Ok(p);
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    Err("Notes did not settle".into())
}
pub(super) async fn button() -> Result<Value, String> {
    capture::dom(&format!(r#"(()=>{{{}const b=[...document.querySelectorAll('#app-home button')].find(b=>b.textContent.trim()==='Send form');b?.scrollIntoView({{block:'center'}});if(!owned(b)||b.disabled)return{{ok:false}};b.click();return{{ok:true,trusted:false,visibleHitTested:true}};}})()"#,capture::ACTION_GUARD)).await
}
pub(super) async fn reset(s: Signal<ShellState>, d: &DesktopContext) -> Result<Value, String> {
    let before = snapshot(&s.peek());
    let p=capture::dom(&format!(r#"(()=>{{{}const b=[...document.querySelectorAll('#app-home button')].find(b=>b.textContent.trim()==='Reset inputs');b?.scrollIntoView({{block:'center'}});if(!owned(b)||b.disabled)return{{ok:false}};b.click();return{{ok:true,trusted:false,visibleHitTested:true}};}})()"#,capture::ACTION_GUARD)).await?;
    let mut changed = false;
    for _ in 0..200 {
        if snapshot(&s.peek())["authority"]["homeRevision"] != before["authority"]["homeRevision"] {
            changed = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    if !changed {
        return Err("Number Reset did not replace form revision".into());
    }
    settle(s, d).await?;
    Ok(json!({"before":before,"click":p,"productResetAdapter":true}))
}
