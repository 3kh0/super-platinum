use super::*;
async fn gesture(
    s: Signal<ShellState>,
    d: &DesktopContext,
    block: &str,
    family: &str,
    kind: &str,
    value: &str,
    local: Value,
    parent: Value,
    observed: bool,
    held_attempt: Option<usize>,
) -> Result<Value, String> {
    let id = crate::view::app_modal::field_id(&key(block));
    let x = snapshot(&s.peek());
    let owned_source =
        json!({"visit":x["authority"]["homeVisit"],"revision":x["authority"]["homeRevision"]});
    let prelude = format!(
        r#"{}const id={},family={},source={},n=document.getElementById(id),home=document.getElementById('app-home'),book=window.__spHomeText,e=[...(book?.entries.values()||[])].find(e=>e.node===n);n?.scrollIntoView({{block:'center'}});if(!owned(n)||home?.dataset.visit!==String(source.visit)||home.dataset.revision!==String(source.revision)||n.type!==family||!e?.[family]||e.closed||e.seq!==e.acked||e.lease||book.activeLease||n.readOnly||n.disabled)return{{ok:false}};"#,
        capture::ACTION_GUARD,
        json!(id),
        json!(family),
        owned_source
    );
    crate::slack_links_check::wait_dom(d, &format!("(()=>{{{prelude}return{{ok:true}};}})()"))
        .await?;
    let script = format!(
        r#"(()=>{{{prelude}const kind={},requested={},before={{mount:e.mount,seq:e.seq,value:n.value,observed:e.value,local:e.local}};let delivered=null,prevented=false;if(kind==='input'){{Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(n,requested);delivered=n.value;if(delivered!==requested)return{{ok:false,error:'Native input sanitized requested fixture value',requested,delivered}};n.dispatchEvent(new InputEvent('input',{{bubbles:true,inputType:'insertText',data:null}}));}}else if(kind==='enter'){{n.focus({{preventScroll:true}});const event=new KeyboardEvent('keydown',{{key:'Enter',code:'Enter',bubbles:true,cancelable:true}});n.dispatchEvent(event);prevented=event.defaultPrevented;}}else if(kind==='blur'){{n.focus({{preventScroll:true}});n.blur();}}else return{{ok:false}};return{{ok:true,id,family,kind,requested,delivered,before,after:{{mount:e.mount,seq:e.seq,value:n.value,observed:e.value,local:e.local}},prevented,trusted:false,visibleHitTested:true,validity:{{typeMismatch:n.validity.typeMismatch,badInput:n.validity.badInput,valid:n.validity.valid}}}};}})()"#,
        json!(kind),
        json!(value)
    );
    let mut proof = capture::dom(&script).await?;
    let increment = u64::from(observed);
    if proof["after"]["seq"].as_u64() != proof["before"]["seq"].as_u64().map(|v| v + increment)
        || (kind == "enter" && proof["prevented"] != json!(observed))
    {
        return Err(format!("Email fixture gesture occurrence differs: {proof}"));
    }
    let expected = proof["after"].clone();
    if let Some(index) = held_attempt {
        // The armed observer holds this one dispatch lease until the lane retires
        // its source. Observe its acknowledgement without waiting for release;
        // the pre-mutation guard above still requires an entirely idle lease.
        let held = gated(s).await?;
        let attempt = &held["admission"]["attempts"][index];
        if held["admission"]["waiting"] != true
            || attempt["held"] != true
            || !attempt["accepted"].is_null()
            || attempt["sourceBefore"]["authority"] != x["authority"]
        {
            return Err("Email held input reached a different admission attempt".into());
        }
        let ack = crate::slack_links_check::wait_dom(d,&format!(r#"(()=>{{const x={},source={},book=window.__spHomeText,e=book?.entries.get(x.mount),home=document.getElementById('app-home'),n=e?.node;const ok=!!e&&!e.closed&&!!e.lease&&book.activeLease===e.lease&&e.seq===x.seq&&e.acked===x.seq&&n.value===x.value&&e.value===x.observed&&e.local===x.local&&n.isConnected&&n.parentNode===e.host&&document.getElementById(n.id)===n&&home?.contains(n)&&home.dataset.visit===String(source.visit)&&home.dataset.revision===String(source.revision);return{{ok,mount:e?.mount,seq:e?.seq,acked:e?.acked,value:n?.value,observed:e?.value,local:e?.local,leaseHeld:!!e?.lease,activeLeaseMatches:!!e?.lease&&book.activeLease===e.lease,source}};}})()"#,expected,owned_source)).await?;
        proof["heldAcknowledgement"] = json!({"attempt":index,"dom":ack,"model":held});
    } else {
        crate::slack_links_check::wait_dom(d,&format!(r#"(()=>{{const x={},e=window.__spHomeText?.entries.get(x.mount);return{{ok:!!e&&!e.closed&&!e.lease&&e.seq===x.seq&&e.acked===x.seq&&e.node.value===x.value}}}})()"#,expected)).await?;
    }
    for _ in 0..150 {
        let x = snapshot(&s.peek());
        let rows = if family == "email" {
            "emailWidgets"
        } else {
            "numberWidgets"
        };
        let widget = x[rows]
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
        "Email fixture local/parent did not settle for {block}: {}",
        snapshot(&s.peek())
    ))
}
pub(super) fn wire(value: &str) -> Value {
    json!({"type":"email_text_input","value":value})
}
pub(super) async fn input(
    s: Signal<ShellState>,
    d: &DesktopContext,
    block: &str,
    value: &str,
) -> Result<Value, String> {
    gesture(
        s,
        d,
        block,
        "email",
        "input",
        value,
        json!(value),
        wire(value),
        true,
        None,
    )
    .await
}
pub(super) async fn held_input(
    s: Signal<ShellState>,
    d: &DesktopContext,
    block: &str,
    value: &str,
    attempt: usize,
) -> Result<Value, String> {
    gesture(
        s,
        d,
        block,
        "email",
        "input",
        value,
        json!(value),
        wire(value),
        true,
        Some(attempt),
    )
    .await
}
pub(super) async fn enter(
    s: Signal<ShellState>,
    d: &DesktopContext,
    block: &str,
    value: &str,
) -> Result<Value, String> {
    gesture(
        s,
        d,
        block,
        "email",
        "enter",
        "",
        json!(value),
        wire(value),
        true,
        None,
    )
    .await
}
pub(super) async fn ignored_enter(
    s: Signal<ShellState>,
    d: &DesktopContext,
    block: &str,
    local: &str,
    parent: Value,
) -> Result<Value, String> {
    gesture(
        s,
        d,
        block,
        "email",
        "enter",
        "",
        json!(local),
        parent,
        false,
        None,
    )
    .await
}
pub(super) async fn blur(
    s: Signal<ShellState>,
    d: &DesktopContext,
    block: &str,
    local: &str,
    parent: Value,
) -> Result<Value, String> {
    gesture(
        s,
        d,
        block,
        "email",
        "blur",
        "",
        json!(local),
        parent,
        false,
        None,
    )
    .await
}
pub(super) async fn amount(
    s: Signal<ShellState>,
    d: &DesktopContext,
    value: &str,
) -> Result<Value, String> {
    gesture(
        s,
        d,
        "amount",
        "number",
        "input",
        value,
        json!(value),
        json!({"type":"number_input","value":value}),
        true,
        None,
    )
    .await
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
        return Err("Email Reset did not replace form revision".into());
    }
    settle(s, d).await?;
    Ok(json!({"before":before,"click":p,"productResetAdapter":true}))
}
