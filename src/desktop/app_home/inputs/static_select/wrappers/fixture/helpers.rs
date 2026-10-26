use super::*;
pub(super) type Captures = serde_json::Map<String, Value>;
fn bound(s: &ShellState) -> Value {
    let mut x = snapshot(s);
    x["domFields"]=json!(form(s).map(|f|f.fields.iter().map(|(k,v)| {
        let visible=owner(s).and_then(|o|s.app_home.static_select.widgets.get(&(o,k.clone()))).and_then(|w|w.visible.as_ref());
        let (kind,value)=match (&v.kind,&v.value){(InputKind::StaticSelect{options,..},InputValue::Selected(values))=>{
            let selected=if dispatching(s,k){visible}else{values.first()};("select",selected.and_then(|v|options.iter().position(|o|&o.value==v)).map(|i|format!("option-{i}")).unwrap_or_default())},(_,InputValue::Text(v))=>("text",v.clone()),(_,InputValue::Date(v))=>("date",v.clone().unwrap_or_default()),_=>("unknown",String::new())};
        let inline=f.raw_view()["blocks"].as_array().is_some_and(|bs|bs.iter().any(|b|b["block_id"]==k.block_id&&matches!(b["type"].as_str(),Some("actions"|"section"))));
        json!({"id":crate::view::app_modal::field_id(k),"block":k.block_id,"action":k.action_id,"kind":kind,"value":value,"inline":inline,"disabled":x["cooling"]==true||(kind=="select"&&loading(s,&k.block_id)),"error":v.error,"allowsClear":f.static_allows_clear(k).ok()})
    }).collect::<Vec<_>>()));
    x
}
fn layout(s: &ShellState, focus: Value) -> String {
    format!(
        "(()=>{{const x={},focus={};{} }})()",
        bound(s),
        focus,
        include_str!("layout.js")
    )
}
pub(super) async fn settle(s: Signal<ShellState>, d: &DesktopContext) -> Result<(), String> {
    for _ in 0..300 {
        if inputs::ready(&s.peek())
            && snapshot(&s.peek())["buttonProcessing"]
                .as_array()
                .is_some_and(|v| v.iter().all(|a| a["processing"] == false))
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
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
        "widgets",
        "context",
        "requests",
        "responses",
        "buttonSync",
        "buttonProcessing",
    ] {
        if before[k] != after[k] {
            return Err(format!("Wrapper native settlement changed {k}"));
        }
    }
    Ok(())
}
pub(super) async fn shot(
    s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    stage: &str,
    proof: Value,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    settle(s, d).await?;
    let before = snapshot(&s.peek());
    let script = layout(&s.peek(), Value::Null);
    let mut g = crate::slack_links_check::wait_dom(d, &script).await?;
    let inventory = g["inventory"].clone();
    let required = g["requiredViews"]
        .as_array()
        .ok_or("Wrapper capture coverage missing")?
        .clone();
    capture::capture_image(d, n, stage, &mut g, cap).await?;
    let mut views = vec![];
    for focus in required {
        let script = layout(&s.peek(), focus.clone());
        let mut v = crate::slack_links_check::wait_dom(d, &script).await?;
        if v["inventory"] != inventory || v["focusVisible"] != true {
            return Err("Wrapper capture inventory changed".into());
        }
        let name = format!("{stage}-{}", focus.as_str().ok_or("Wrapper focus key")?);
        capture::capture_image(d, n, &name, &mut v, cap).await?;
        views.push(json!({"key":focus,"captureKey":name,"geometry":v}));
    }
    let after = snapshot(&s.peek());
    if before != after {
        return Err("Wrapper capture changed owned model".into());
    }
    st.push(json!({"stage":stage,"proof":proof,"geometry":g,"scrollViews":views,"model":after}));
    Ok(())
}
pub(super) async fn choose(
    s: Signal<ShellState>,
    d: &DesktopContext,
    block: &str,
    action: &str,
    index: Option<usize>,
) -> Result<Value, String> {
    settle(s, d).await?;
    let id = crate::view::app_modal::field_id(&key(block, action));
    let count = snapshot(&s.peek())["events"].as_array().map_or(0, Vec::len);
    let proof=capture::dom(&format!(r#"(()=>{{{}const n=document.getElementById({});n?.scrollIntoView({{block:'center'}});if(!owned(n)||n.tagName!=='SELECT')return{{ok:false}};const index={},value=index===null?'':`option-${{index}}`,o=[...n.options].find(o=>o.value===value);if(!o||o.disabled)return{{ok:false}};n.value=value;const e=new Event('change',{{bubbles:true,cancelable:true}});n.dispatchEvent(e);return{{ok:true,id:n.id,block:{},action:{},index,value:n.value,prevented:e.defaultPrevented,trusted:e.isTrusted,visibleHitTested:true}};}})()"#,capture::ACTION_GUARD,json!(id),json!(index),json!(block),json!(action))).await?;
    for _ in 0..200 {
        if snapshot(&s.peek())["events"].as_array().map_or(0, Vec::len) == count + 1 {
            return Ok(proof);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err("Wrapper callback missing".into())
}
pub(super) async fn counts(
    s: Signal<ShellState>,
    sr: usize,
    ss: usize,
    br: usize,
    bs: usize,
) -> Result<(), String> {
    for _ in 0..350 {
        let x = snapshot(&s.peek());
        if x["requests"].as_array().map(Vec::len) == Some(sr)
            && x["responses"].as_array().map(Vec::len) == Some(ss)
            && x["buttonSync"]["requests"].as_array().map(Vec::len) == Some(br)
            && x["buttonSync"]["responses"].as_array().map(Vec::len) == Some(bs)
            && x["windows"] == json!([])
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err(format!(
        "Wrapper ledger did not settle {}",
        snapshot(&s.peek())
    ))
}
pub(super) async fn click(s: Signal<ShellState>, d: &DesktopContext) -> Result<Value, String> {
    settle(s, d).await?;
    capture::dom(&format!(r#"(()=>{{{}const b=[...document.querySelectorAll('#app-home button')].find(n=>n.textContent.trim()==='Send form');b?.scrollIntoView({{block:'center'}});if(!owned(b))return{{ok:false}};b.click();return{{ok:true,text:b.textContent,visibleHitTested:true,trusted:false}};}})()"#,capture::ACTION_GUARD)).await
}
pub(super) async fn blocked_click(
    s: Signal<ShellState>,
    d: &DesktopContext,
) -> Result<Value, String> {
    // Observe the actual processing transition before one deliberately suppressed
    // disabled-button click; do not retry the mutation itself.
    let script = format!(
        r#"(()=>{{{}const b=[...document.querySelectorAll('#app-home button')].find(n=>n.textContent.trim()==='Send form');b?.scrollIntoView({{block:'center'}});return{{ok:!!b&&b.disabled&&owned(b),disabled:b?.disabled,text:b?.textContent}};}})()"#,
        capture::ACTION_GUARD.replace("!e||e.disabled", "!e")
    );
    let geometry = crate::slack_links_check::wait_dom(d, &script).await?;
    let before = snapshot(&s.peek());
    if before["buttonEnabled"] != false {
        return Err("Button processing deadline already elapsed before suppression probe".into());
    }
    let click=capture::dom(&format!(r#"(()=>{{{}const b=[...document.querySelectorAll('#app-home button')].find(n=>n.textContent.trim()==='Send form');if(!b?.disabled||!owned(b))return{{ok:false}};b.click();return{{ok:true,disabled:b.disabled,suppressed:true,visibleHitTested:true}};}})()"#,capture::ACTION_GUARD.replace("!e||e.disabled","!e"))).await?;
    Ok(
        json!({"geometry":geometry,"click":click,"processingBefore":before["buttonProcessing"],"buttonEnabledBefore":before["buttonEnabled"]}),
    )
}
pub(super) async fn silence(s: Signal<ShellState>) -> Result<Value, String> {
    let before = snapshot(&s.peek());
    tokio::time::sleep(Duration::from_millis(1100)).await;
    let after = snapshot(&s.peek());
    for key in ["requests", "responses"] {
        if before[key] != after[key] || before["buttonSync"][key] != after["buttonSync"][key] {
            return Err("Automatic wrapper action replay".into());
        }
    }
    Ok(
        json!({"observedMs":1100,"staticRequests":after["requests"].as_array().map(Vec::len),"buttonRequests":after["buttonSync"]["requests"].as_array().map(Vec::len)}),
    )
}
pub(super) fn refresh(s: &mut ShellState, edit: impl FnOnce(&mut Value)) -> Value {
    let h = s.app_home.surface.as_ref().unwrap();
    let team = h.owner.ctx.session.team_id.clone();
    let rt = h.owner.rt;
    let before = h.revision;
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    edit(&mut raw);
    raw["hash"] = json!(format!("wrapper-refresh-{before}"));
    let e = RtEvent::Unknown(
        serde_json::from_value(
            json!({"type":"view_updated","app_id":"A_HOME","view_id":"V_HOME","view":raw}),
        )
        .unwrap(),
    );
    crate::app_home::realtime(s, &team, rt, &e);
    json!({"before":before,"after":s.app_home.surface.as_ref().unwrap().revision})
}
pub(super) async fn gate(s: Signal<ShellState>) -> Result<(), String> {
    for _ in 0..300 {
        if snapshot(&s.peek())["gateWaiting"] == true {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err("Wrapper native lease gate missing".into())
}
pub(super) async fn refused(s: Signal<ShellState>, count: usize) -> Result<(), String> {
    for _ in 0..300 {
        let x = snapshot(&s.peek());
        if x["refusals"].as_array().map(Vec::len) == Some(count) && x["windows"] == json!([]) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err("Wrapper stale refusal missing".into())
}
pub(super) async fn dismiss(
    s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    cap: &mut Captures,
) -> Result<Value, String> {
    let before = snapshot(&s.peek());
    let script = format!(
        r#"(()=>{{{}const t=document.getElementById('status-toast'),b=t?.querySelector('.toast-dismiss');if(!owned(b))return{{ok:false}};return{{ok:true,text:t.querySelector('.toast-text')?.textContent,control:b.getAttribute('aria-label'),visibleHitTested:true}};}})()"#,
        capture::ACTION_GUARD
    );
    let mut g = crate::slack_links_check::wait_dom(d, &script).await?;
    capture::capture_image(d, n, "uncertain-toast", &mut g, cap).await?;
    let click = capture::click("#status-toast .toast-dismiss").await?;
    crate::slack_links_check::wait_dom(d, "({ok:!document.getElementById('status-toast')})")
        .await?;
    if before != snapshot(&s.peek()) {
        return Err("Wrapper toast dismissal changed model".into());
    }
    Ok(json!({"captureKey":"uncertain-toast","geometry":g,"click":click,"modelUnchanged":true}))
}
