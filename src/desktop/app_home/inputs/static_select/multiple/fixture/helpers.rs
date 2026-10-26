use super::*;
pub(super) type Captures = serde_json::Map<String, Value>;
fn bound(s: &ShellState) -> Value {
    let mut x = snapshot(s);
    x["domFields"]=json!(form(s).map(|f|f.fields.iter().map(|(k,v)| {
        let (kind,value)=match (&v.kind,&v.value){
            (InputKind::StaticSelect{multiple:true,options,..},InputValue::Selected(values))=>if dispatching(s,k){("multi",json!(values))}else{("quiet-multi",json!(options.iter().enumerate().filter(|(_,o)|values.contains(&o.value)).map(|(i,_)|format!("option-{i}")).collect::<Vec<_>>()))},
            (_,InputValue::Text(v))=>("text",json!(v)),(_,InputValue::Date(v))=>("date",json!(v.clone().unwrap_or_default())),_=>("unknown",Value::Null)};
        json!({"id":crate::view::app_modal::field_id(k),"block":k.block_id,"action":k.action_id,"kind":kind,"value":value,"disabled":x["cooling"]==true||(kind=="multi"&&loading(s,&k.block_id)),"error":v.error})
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
        let active = inputs::ready(&s.peek())
            && x["buttonProcessing"]
                .as_array()
                .is_some_and(|v| v.iter().all(|a| a["processing"] == false))
            && owned("activity")
            && owned("windows");
        let retired = x["authority"]["homeCurrent"] == false
            && x["multiMounts"] == json!([])
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
            "Multi-static source settlement timed out: {}",
            snapshot(&s.peek())
        ));
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
        "multiWidgets",
        "multiMounts",
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
                "Multi-static native settlement changed {k}: {}",
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
        .ok_or("Multi-static capture coverage missing")?
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
            return Err("Multi-static capture inventory changed".into());
        }
        let name = format!(
            "{stage}-{}",
            focus.as_str().ok_or("Multi-static focus key")?
        );
        capture::capture_image(d, n, &name, &mut v, cap).await?;
        covered.extend(visible(&v));
        views.push(json!({"key":focus,"captureKey":name,"geometry":v}));
    }
    let missing = inventory["captureTargets"]
        .as_array()
        .ok_or("Multi-static target inventory missing")?
        .iter()
        .filter_map(|t| t["key"].as_str())
        .filter(|k| !covered.contains(*k))
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(format!(
            "Multi-static scroll coverage incomplete: {}",
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
            "Multi-static capture changed owned model: {}",
            json!(changed)
        ));
    }
    st.push(json!({"stage":stage,"proof":proof,"geometry":g,"scrollViews":views,"model":after}));
    Ok(())
}
// A transaction can deliberately deliver two callbacks before trailing debounce.
// Each gesture hits an actual owned control; no synthetic raw occurrence is admitted.
pub(super) async fn edit(
    s: Signal<ShellState>,
    d: &DesktopContext,
    block: &str,
    ops: Vec<Value>,
) -> Result<Value, String> {
    settle(s, d).await?;
    let id = crate::view::app_modal::field_id(&key(block));
    let before = snapshot(&s.peek())["events"].as_array().map_or(0, Vec::len);
    let proof=capture::dom(&format!(r#"await (async()=>{{{}const root=document.getElementById({});if(!root)return{{ok:false}};const actions=[];for(const op of {}){{const selector=op.kind==='remove'?`button[data-selected-index="${{op.index}}"]`:`[data-option-index="${{op.index}}"]`;let e=root.querySelector(selector);if(e&&e.tagName!=='INPUT'&&op.kind!=='remove')e=e.querySelector('input');e?.scrollIntoView({{block:'center'}});if(!owned(e))return{{ok:false,selector}};e.click();let acknowledged=false,ackSeq=null;for(let t=0;t<200;t++){{const entry=[...(window.__spHomeStatic?.entries.values()||[])].find(e=>e.node===root);if(entry&&!entry.closed&&entry.acked===entry.seq){{acknowledged=true;ackSeq=entry.seq;break;}}await new Promise(r=>setTimeout(r,5));}}if(!acknowledged)return{{ok:false,error:'Multi-static occurrence not acknowledged',op}};actions.push({{op,trusted:false,visibleHitTested:true,acknowledged,ackSeq}});}}return{{ok:true,id:root.id,actions}};}})()"#,capture::ACTION_GUARD,json!(id),json!(ops))).await?;
    for _ in 0..200 {
        let x = snapshot(&s.peek());
        if x["events"].as_array().map_or(0, Vec::len) == before + ops.len() {
            return Ok(proof);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err("Multi-static native callback missing".into())
}
pub(super) fn toggle(index: usize) -> Value {
    json!({"kind":"toggle","index":index})
}
pub(super) fn remove(index: usize) -> Value {
    json!({"kind":"remove","index":index})
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
        "Multi-static ledger did not settle {}",
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
    raw["hash"] = json!(format!("multi-static-refresh-{before}"));
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
    Err("Multi-static native lease gate missing".into())
}
pub(super) async fn refused(s: Signal<ShellState>, count: usize) -> Result<(), String> {
    for _ in 0..300 {
        let x = snapshot(&s.peek());
        if x["refusals"].as_array().map(Vec::len) == Some(count) && x["windows"] == json!([]) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err("Multi-static stale refusal missing".into())
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
    let click = capture::click("#status-toast .toast-dismiss").await?;
    crate::slack_links_check::wait_dom(d, "({ok:!document.getElementById('status-toast')})")
        .await?;
    if before != snapshot(&s.peek()) {
        return Err("Multi-static toast dismissal changed model".into());
    }
    Ok(json!({"captureKey":capture_key,"geometry":g,"click":click,"modelUnchanged":true}))
}

pub(super) async fn sibling(s: Signal<ShellState>, text: &str) -> Result<Value, String> {
    let id = crate::view::app_modal::field_id(&key("notes"));
    let proof=capture::dom(&format!(r#"(()=>{{{}const e=document.getElementById({});e?.scrollIntoView({{block:'center'}});if(!owned(e))return{{ok:false}};e.value={};e.dispatchEvent(new Event('input',{{bubbles:true}}));return{{ok:true,value:e.value,trusted:false,visibleHitTested:true}};}})()"#,capture::ACTION_GUARD,json!(id),json!(text))).await?;
    for _ in 0..200 {
        if form(&s.peek())
            .and_then(|f| f.fields.get(&key("notes")))
            .is_some_and(|f| matches!(&f.value,InputValue::Text(v) if v==text))
        {
            return Ok(proof);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err("Sibling callback missing".into())
}

pub(super) async fn quiet_edit(s: Signal<ShellState>) -> Result<Value, String> {
    let id = crate::view::app_modal::field_id(&key("quiet"));
    let proof=capture::dom(&format!(r#"(()=>{{{}const n=document.getElementById({});n?.scrollIntoView({{block:'center'}});if(!owned(n)||n.tagName!=='SELECT'||!n.multiple)return{{ok:false}};for(const o of n.options)o.selected=['option-1','option-2'].includes(o.value);n.dispatchEvent(new Event('change',{{bubbles:true}}));return{{ok:true,values:[...n.selectedOptions].map(o=>o.value),trusted:false,visibleHitTested:true}};}})()"#,capture::ACTION_GUARD,json!(id))).await?;
    for _ in 0..200 {
        if form(&s.peek()).and_then(|f|f.fields.get(&key("quiet"))).is_some_and(|f|matches!(&f.value,InputValue::Selected(v) if v==&vec!["beta".to_string(),"gamma".to_string()])){return Ok(proof);}
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err("Quiet multi callback missing".into())
}
