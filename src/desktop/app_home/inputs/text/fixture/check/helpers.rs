use super::*;
pub(crate) type Captures = serde_json::Map<String, Value>;
fn bound(s: &ShellState) -> Value {
    let mut v = snapshot(s);
    v["domFields"]=json!(form(s).map(|f|f.fields.iter().map(|(k,f)|json!({"block":k.block_id,"id":crate::view::app_modal::field_id(k),"value":match &f.value{InputValue::Text(v)=>v.clone(),InputValue::Date(v)=>v.clone().unwrap_or_default(),_=>String::new()},"error":crate::app_home::inputs::presentation_error(s,k)})).collect::<Vec<_>>()));
    {
        let native = crate::app_home::inputs::native::date::proof(s);
        if let Some(fields) = v["domFields"].as_array_mut() {
            for field in fields {
                if let Some(row) = native["dateWidgets"]
                    .as_array()
                    .and_then(|rows| rows.iter().find(|row| row["block"] == field["block"]))
                {
                    field["value"] = row["raw"].clone();
                }
            }
        }
    }
    v
}
fn layout(s: &ShellState, focus: Value) -> String {
    format!(
        "(()=>{{const x={},focus={};{} }})()",
        bound(s),
        focus,
        include_str!("layout.js")
    )
}
pub(crate) async fn settle(s: Signal<ShellState>, d: &DesktopContext) -> Result<(), String> {
    for _ in 0..200 {
        if inputs::ready(&s.peek()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let before = snapshot(&s.peek());
    let script = layout(&s.peek(), Value::Null);
    crate::slack_links_check::wait_dom(d, &script).await?;
    let after = snapshot(&s.peek());
    for k in [
        "authority",
        "rawView",
        "formState",
        "fields",
        "context",
        "queue",
        "requests",
        "responses",
    ] {
        if before[k] != after[k] {
            return Err(format!("Text settlement changed {k}"));
        }
    }
    Ok(())
}
pub(crate) async fn edit(
    s: Signal<ShellState>,
    d: &DesktopContext,
    block: &str,
    value: &str,
) -> Result<Value, String> {
    settle(s, d).await?;
    let id = crate::view::app_modal::field_id(&key(block));
    let proof=capture::dom(&format!(r#"(()=>{{{}const e=document.getElementById({});if(!owned(e))return{{ok:false}};e.focus({{preventScroll:true}});const proto=e.tagName==='TEXTAREA'?HTMLTextAreaElement.prototype:HTMLInputElement.prototype;Object.getOwnPropertyDescriptor(proto,'value').set.call(e,{});const event=new InputEvent('input',{{bubbles:true,inputType:'insertText',data:null}});e.dispatchEvent(event);return{{ok:true,id:e.id,value:e.value,trusted:event.isTrusted,visibleHitTested:true}};}})()"#,capture::ACTION_GUARD,json!(id),json!(value))).await?;
    for _ in 0..150 {
        if form(&s.peek())
            .and_then(|f| f.fields.get(&key(block)))
            .is_some_and(|f| matches!(&f.value,InputValue::Text(v)if v==value))
        {
            return Ok(proof);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err("Text input did not reach exact form".into())
}
pub(crate) async fn enter(
    s: Signal<ShellState>,
    d: &DesktopContext,
    block: &str,
    extra: Value,
) -> Result<Value, String> {
    settle(s, d).await?;
    let id = crate::view::app_modal::field_id(&key(block));
    let before = snapshot(&s.peek())["events"].as_array().map_or(0, Vec::len);
    let proof=capture::dom(&format!(r#"(()=>{{{}const e=document.getElementById({});if(!owned(e))return{{ok:false}};e.focus({{preventScroll:true}});const options=Object.assign({{key:'Enter',code:'Enter',bubbles:true,cancelable:true}},{}),event=new KeyboardEvent('keydown',options);e.dispatchEvent(event);return{{ok:true,id:e.id,value:e.value,key:event.key,code:event.code,ctrl:event.ctrlKey,alt:event.altKey,shift:event.shiftKey,meta:event.metaKey,repeat:event.repeat,composing:event.isComposing,trusted:event.isTrusted,prevented:event.defaultPrevented,visibleHitTested:true}};}})()"#,capture::ACTION_GUARD,json!(id),extra)).await?;
    if proof["prevented"] == true {
        for _ in 0..150 {
            if snapshot(&s.peek())["events"].as_array().map_or(0, Vec::len) > before {
                return Ok(proof);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        return Err("Enter occurrence not received".into());
    }
    Ok(proof)
}
pub(crate) async fn counts(
    s: Signal<ShellState>,
    requests: usize,
    responses: usize,
) -> Result<(), String> {
    for _ in 0..220 {
        let v = snapshot(&s.peek());
        if v["requests"].as_array().map(Vec::len) == Some(requests)
            && v["responses"].as_array().map(Vec::len) == Some(responses)
            && v["queue"] == json!([])
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(15)).await;
    }
    Err(format!("Text request settlement: {}", snapshot(&s.peek())))
}
pub(crate) async fn quiet(s: Signal<ShellState>, requests: usize) -> Result<Value, String> {
    let before = snapshot(&s.peek());
    tokio::time::sleep(Duration::from_millis(650)).await;
    let after = snapshot(&s.peek());
    if after["requests"].as_array().map(Vec::len) != Some(requests) {
        return Err("Unexpected text dispatch".into());
    }
    Ok(
        json!({"before":before["queue"],"after":after["queue"],"requests":requests,"observedMs":650}),
    )
}
pub(crate) async fn shot(
    s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    stage: &str,
    proof: Value,
    stages: &mut Vec<Value>,
    captures: &mut Captures,
) -> Result<(), String> {
    settle(s, d).await?;
    let before = snapshot(&s.peek());
    let script = layout(&s.peek(), Value::Null);
    let mut geometry = crate::slack_links_check::wait_dom(d, &script).await?;
    let inventory = geometry["inventory"].clone();
    let required = geometry["requiredViews"]
        .as_array()
        .ok_or("Text coverage missing")?
        .clone();
    capture::capture_image(d, n, stage, &mut geometry, captures).await?;
    let mut views = vec![];
    for focus in required {
        let script = layout(&s.peek(), focus.clone());
        let mut g = crate::slack_links_check::wait_dom(d, &script).await?;
        if g["inventory"] != inventory || g["focusVisible"] != true {
            return Err("Text scroll inventory changed".into());
        }
        let name = format!("{stage}-{}", focus.as_str().ok_or("Text capture key")?);
        capture::capture_image(d, n, &name, &mut g, captures).await?;
        views.push(json!({"key":focus,"captureKey":name,"geometry":g}));
    }
    let after = snapshot(&s.peek());
    if before != after {
        return Err("Text model changed during capture".into());
    }
    stages.push(
        json!({"stage":stage,"geometry":geometry,"scrollViews":views,"model":after,"proof":proof}),
    );
    Ok(())
}
pub(crate) fn refresh(s: &mut ShellState) -> Value {
    let h = s.app_home.surface.as_ref().unwrap();
    let team = h.owner.ctx.session.team_id.clone();
    let rt = h.owner.rt;
    let before = h.revision;
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["hash"] = json!(format!("text-refresh-{before}"));
    let event = RtEvent::Unknown(
        serde_json::from_value(
            json!({"type":"view_updated","app_id":"A_HOME","view_id":"V_HOME","view":raw}),
        )
        .unwrap(),
    );
    crate::app_home::realtime(s, &team, rt, &event);
    json!({"before":before,"after":s.app_home.surface.as_ref().unwrap().revision})
}
