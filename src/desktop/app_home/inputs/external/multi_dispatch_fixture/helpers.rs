use super::*;
pub(super) type Captures = serde_json::Map<String, Value>;
fn bound(s: &ShellState) -> Value {
    let mut x = snapshot(s);
    x["domFields"] = json!(inputs::form(s).map(|f|f.fields.iter().map(|(k,v)| {
        let widget = x["externalWidgets"].as_array().and_then(|r|r.iter().find(|w|w["block"]==k.block_id&&w["action"]==k.action_id));
        json!({"id":crate::view::app_modal::field_id(k),"block":k.block_id,"action":k.action_id,
            "kind":match v.kind {InputKind::ExternalSelect{..}=>"external",InputKind::MultiExternalSelect{..}=>"externals",InputKind::Date=>"date",_=>"text"},
            "optional":v.optional,"error":inputs::presentation_error(s,k),
            "selected":widget.map(|w|w["visible"].clone()),
            "multiSelected":inputs::external::multiple::visible(s,k).map(|v|v.iter().map(|o|o.raw().clone()).collect::<Vec<_>>()),
            "multiStatus":inputs::external::multiple::presentation(s,k),
            "disabled":x["cooling"]==true||inputs::external::dispatch::disabled(s,k),
            "value":match &v.value {InputValue::Text(v)=>json!(v),InputValue::Date(_)=>x["dateWidgets"].as_array().and_then(|r|r.iter().find(|w|w["block"]==k.block_id&&w["action"]==k.action_id)).map(|w|w["raw"].clone()).unwrap_or(Value::Null),_=>Value::Null}})
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
    let mut settled = false;
    for _ in 0..300 {
        let x = snapshot(&s.peek());
        let owned = |key: &str| {
            x[key]
                .as_array()
                .is_some_and(|v| v.iter().all(|a| a["current"] == true))
        };
        // A sibling edit invalidates the query immediately; production poll
        // then retires it. Do not freeze DOM expectations from that stale ticket.
        let picker_current =
            x["suggestions"]["query"].is_null() || x["suggestions"]["query"]["current"] == true;
        let active = inputs::ready(&s.peek())
            && picker_current
            && x["buttonProcessing"]
                .as_array()
                .is_some_and(|v| v.iter().all(|a| a["processing"] == false))
            && owned("activity")
            && owned("windows");
        let retired = x["authority"]["homeCurrent"] == false
            && x["nativeMounts"].is_null()
            && x["activity"] == json!([])
            && x["windows"] == json!([]);
        if active || retired {
            settled = true;
            break;
        }
        // Observe actual production tick cleanup; never invoke or mutate it here.
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    if !settled {
        return Err(format!(
            "External source settlement timed out: {}",
            snapshot(&s.peek())
        ));
    }
    let before = snapshot(&s.peek());
    let script = layout(&s.peek(), Value::Null);
    crate::slack_links_check::wait_dom(d, &script)
        .await
        .map_err(|error| {
            format!(
                "{error}; frozen External layout model: {before}; current model: {}",
                snapshot(&s.peek())
            )
        })?;
    let after = snapshot(&s.peek());
    for k in [
        "authority",
        "rawView",
        "formState",
        "fields",
        "widgets",
        "externalWidgets",
        "multiExternalWidgets",
        "externalCatalogs",
        "externalPickerBlock",
        "pickerBlock",
        "suggestions",
        "nativeMounts",
        "context",
        "requests",
        "responses",
        "buttonSync",
        "buttonProcessing",
        "activity",
        "windows",
        "inputRevision",
    ] {
        if before[k] != after[k] {
            return Err(format!(
                "External native settlement changed {k}: {}",
                json!({"before":before[k],"after":after[k]})
            ));
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
        .ok_or("External capture coverage missing")?
        .clone();
    capture::capture_image(d, n, stage, &mut g, cap).await?;
    let mut views = vec![];
    let mut covered = std::collections::BTreeSet::new();
    let visible = |v: &Value| {
        v["rows"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|r| r["geometry"]["visible"] == true)
            .filter_map(|r| r["key"].as_str().map(str::to_owned))
            .collect::<Vec<_>>()
    };
    covered.extend(visible(&g));
    for focus in required {
        if focus.as_str().is_some_and(|k| covered.contains(k)) {
            continue;
        }
        let script = layout(&s.peek(), focus.clone());
        let mut v = crate::slack_links_check::wait_dom(d, &script).await?;
        if v["inventory"] != inventory || v["focusVisible"] != true {
            return Err("External capture inventory changed".into());
        }
        let name = format!("{stage}-{}", focus.as_str().ok_or("External focus key")?);
        capture::capture_image(d, n, &name, &mut v, cap).await?;
        covered.extend(visible(&v));
        views.push(json!({"key":focus,"captureKey":name,"geometry":v}));
    }
    let missing = inventory["captureTargets"]
        .as_array()
        .ok_or("External target inventory missing")?
        .iter()
        .filter_map(|t| t["key"].as_str())
        .filter(|k| !covered.contains(*k))
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(format!(
            "External scroll coverage incomplete: {}",
            json!(missing)
        ));
    }
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
            "External capture changed owned model: {}",
            json!(changed)
        ));
    }
    st.push(json!({"stage":stage,"proof":proof,"geometry":g,"scrollViews":views,"model":after}));
    Ok(())
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
        "External ledger did not settle {}",
        snapshot(&s.peek())
    ))
}
pub(super) async fn click(s: Signal<ShellState>, d: &DesktopContext) -> Result<Value, String> {
    settle(s, d).await?;
    capture::dom(&format!(r#"(()=>{{{}const b=[...document.querySelectorAll('#app-home button')].find(n=>n.textContent.trim()==='Send form');b?.scrollIntoView({{block:'center'}});if(!owned(b))return{{ok:false}};b.click();return{{ok:true,text:b.textContent,visibleHitTested:true,trusted:false}};}})()"#,capture::ACTION_GUARD)).await
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
    raw["hash"] = json!(format!("external-dispatch-refresh-{before}"));
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
    Err("External native lease gate missing".into())
}
pub(super) async fn refused(s: Signal<ShellState>, count: usize) -> Result<(), String> {
    for _ in 0..300 {
        let x = snapshot(&s.peek());
        if x["refusals"].as_array().map(Vec::len) == Some(count) && x["windows"] == json!([]) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err("External stale refusal missing".into())
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
            "External toast dismissal changed model: {}",
            json!(changed)
        ));
    }
    Ok(json!({"captureKey":capture_key,"geometry":g,"click":click,"modelUnchanged":true}))
}

pub(super) async fn no_callback(s: Signal<ShellState>, before: &Value) -> Result<Value, String> {
    let quiet = silence(s).await?;
    let after = snapshot(&s.peek());
    for k in ["formState", "events", "requests", "responses"] {
        if before[k] != after[k] {
            return Err(format!("External silent gesture changed {k}"));
        }
    }
    Ok(quiet)
}
pub(super) async fn prepared(
    s: Signal<ShellState>,
    count: usize,
    started: Instant,
) -> Result<Value, String> {
    loop {
        let x = snapshot(&s.peek());
        if started.elapsed().as_millis() >= 450 {
            return Err("External leading preparation exceeded 450ms".into());
        }
        if x["requests"].as_array().map(Vec::len) == Some(count)
            && x["responses"].as_array().map(Vec::len) == Some(count)
        {
            return Ok(json!({"model":x,"elapsedMs":started.elapsed().as_millis()}));
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}
