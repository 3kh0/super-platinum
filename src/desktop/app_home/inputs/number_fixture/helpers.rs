use super::*;
pub(super) type Captures = serde_json::Map<String, Value>;
fn bound(s: &ShellState) -> Value {
    let mut x = snapshot(s);
    x["domFields"]=json!(inputs::form(s).map(|f|f.fields.iter().map(|(k,v)|{
 let numeric=x["numberMounts"].as_array().and_then(|r|r.iter().find(|r|r["block"]==k.block_id&&r["action"]==k.action_id));
 let date=x["dateWidgets"].as_array().and_then(|r|r.iter().find(|w|w["block"]==k.block_id&&w["action"]==k.action_id));
 let wire=&x["formState"]["values"][&k.block_id][&k.action_id];
 json!({"block":k.block_id,"id":crate::view::app_modal::field_id(k),"raw":v.raw,"numeric":v.raw["type"]=="number_input","value":numeric.map(|m|m["value"].clone()).or_else(||date.map(|w|w["raw"].clone())).unwrap_or_else(||wire["value"].clone()),"error":inputs::presentation_error(s,k)})
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
    let mut ready = false;
    for _ in 0..300 {
        let x = snapshot(&s.peek());
        if (inputs::ready(&s.peek())
            && x["buttonProcessing"]
                .as_array()
                .is_some_and(|r| r.iter().all(|r| r["processing"] == false))
            && x["activity"]
                .as_array()
                .is_some_and(|r| r.iter().all(|r| r["current"] == true)))
            || (x["authority"]["homeCurrent"] == false
                && x["activity"] == json!([])
                && x["queue"] == json!([])
                && x["textMounts"] == json!([])
                && x["nativeMounts"].is_null())
        {
            ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    if !ready {
        return Err(format!(
            "Number ownership did not settle: {}",
            snapshot(&s.peek())
        ));
    }
    let before = snapshot(&s.peek());
    let script = layout(&s.peek(), Value::Null);
    crate::slack_links_check::wait_dom(d, &script)
        .await
        .map_err(|e| format!("{e}; frozen {} current {}", before, snapshot(&s.peek())))?;
    if before != snapshot(&s.peek()) {
        return Err("Number model changed during settlement".into());
    }
    Ok(())
}
pub(super) async fn counts(
    s: Signal<ShellState>,
    req: usize,
    res: usize,
    breq: usize,
    bres: usize,
) -> Result<(), String> {
    for _ in 0..300 {
        let x = snapshot(&s.peek());
        if x["requests"].as_array().map(Vec::len) == Some(req)
            && x["responses"].as_array().map(Vec::len) == Some(res)
            && x["buttonSync"]["requests"].as_array().map(Vec::len) == Some(breq)
            && x["buttonSync"]["responses"].as_array().map(Vec::len) == Some(bres)
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err(format!(
        "Number exact request counts missing: {}",
        snapshot(&s.peek())
    ))
}
pub(super) async fn silence(s: Signal<ShellState>) -> Result<Value, String> {
    let before = snapshot(&s.peek());
    let started = Instant::now();
    tokio::time::sleep(Duration::from_millis(700)).await;
    let after = snapshot(&s.peek());
    for k in ["events", "requests", "responses"] {
        if before[k] != after[k] {
            return Err(format!("Number silent interval changed {k}"));
        }
    }
    Ok(json!({"elapsedMs":started.elapsed().as_millis(),"before":before,"after":after}))
}
pub(super) fn trailing(s: &ShellState, started: Instant, requests: usize) -> Result<Value, String> {
    let x = snapshot(s);
    let elapsed = started.elapsed().as_millis();
    if elapsed >= 450 || x["requests"].as_array().map(Vec::len) != Some(requests) {
        return Err("Number callback missed trailing-only observation window".into());
    }
    Ok(json!({"elapsedMs":elapsed,"model":x}))
}
pub(super) async fn shot(
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
        .ok_or("Number coverage missing")?
        .clone();
    capture::capture_image(d, n, stage, &mut geometry, captures).await?;
    let mut views = vec![];
    for focus in required {
        let script = layout(&s.peek(), focus.clone());
        let mut g = crate::slack_links_check::wait_dom(d, &script).await?;
        if g["inventory"] != inventory || g["focusVisible"] != true {
            return Err("Number scroll inventory changed".into());
        }
        let name = format!("{stage}-{}", focus.as_str().ok_or("Number capture key")?);
        capture::capture_image(d, n, &name, &mut g, captures).await?;
        views.push(json!({"key":focus,"captureKey":name,"geometry":g}));
    }
    let after = snapshot(&s.peek());
    if before != after {
        return Err("Number model changed during capture".into());
    }
    stages.push(
        json!({"stage":stage,"geometry":geometry,"scrollViews":views,"model":after,"proof":proof}),
    );
    Ok(())
}

pub(super) fn refresh(s: &mut ShellState, change: impl FnOnce(&mut Value)) -> Value {
    let h = s.app_home.surface.as_ref().unwrap();
    let team = h.owner.ctx.session.team_id.clone();
    let rt = h.owner.rt;
    let before = h.revision;
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    change(&mut raw);
    raw["hash"] = json!(format!("number-refresh-{before}"));
    let event = RtEvent::Unknown(
        serde_json::from_value(
            json!({"type":"view_updated","app_id":"A_HOME","view_id":"V_HOME","view":raw}),
        )
        .unwrap(),
    );
    crate::app_home::realtime(s, &team, rt, &event);
    json!({"before":before,"after":s.app_home.surface.as_ref().unwrap().revision})
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
    let capture_key = format!(
        "notice-{}",
        before["responses"].as_array().map_or(0, Vec::len)
    );
    capture::capture_image(d, n, &capture_key, &mut g, cap).await?;
    let click = capture::dom(&format!(r#"(()=>{{{}const b=document.querySelector('#status-toast .toast-dismiss');if(!owned(b))return{{ok:false}};b.click();return{{ok:true,text:b.textContent,visibleHitTested:true,trusted:false,focusRequested:false}};}})()"#,capture::ACTION_GUARD)).await?;
    crate::slack_links_check::wait_dom(d, "({ok:!document.getElementById('status-toast')})")
        .await?;
    let after = snapshot(&s.peek());
    if before != after {
        let changed = before
            .as_object()
            .unwrap()
            .iter()
            .filter(|(k, v)| after.get(*k) != Some(*v))
            .map(|(k, v)| json!({"key":k,"before":v,"after":after.get(k)}))
            .collect::<Vec<_>>();
        return Err(format!(
            "Number toast dismissal changed model: {}",
            json!(changed)
        ));
    }
    Ok(json!({"captureKey":capture_key,"geometry":g,"click":click,"modelUnchanged":true}))
}

pub(super) async fn gated(s: Signal<ShellState>) -> Result<Value, String> {
    for _ in 0..220 {
        let x = snapshot(&s.peek());
        if x["admission"]["waiting"] == true {
            return Ok(x);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err("Numeric calendar admission gate missing".into())
}

pub(super) async fn retired(held: &Value, index: usize) -> Result<Value, String> {
    let expected = &held["admission"]["attempts"][index]["expected"];
    capture::dom(&format!(
        "await(async()=>{{const expected={};{} }})()",
        expected,
        include_str!("../text/fixture/admission/retired.js")
    ))
    .await
}
