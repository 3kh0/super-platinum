use super::*;
pub(super) type Captures = serde_json::Map<String, Value>;
fn bound(s: &ShellState) -> Value {
    let mut x = snapshot(s);
    x["domFields"]=json!(form(s).map(|f|f.fields.iter().map(|(k,v)|{
        let visible=owner(s).and_then(|o|s.app_home.static_select.widgets.get(&(o,k.clone()))).and_then(|w|w.visible.as_ref());
        let (kind,value)=match (&v.kind,&v.value) {
            (InputKind::StaticSelect{options,..},InputValue::Selected(values))=>{
                let selected=if dispatching(s,k){visible}else{values.first()};
                ("select",selected.and_then(|v|options.iter().position(|o|&o.value==v)).map(|i|format!("option-{i}")).unwrap_or_default())
            },
            (_,InputValue::Text(v))=>("text",v.clone()),(_,InputValue::Date(v))=>("date",v.clone().unwrap_or_default()),_=>("unknown",String::new())
        };
        json!({"id":crate::view::app_modal::field_id(k),"block":k.block_id,"kind":kind,"value":value,"disabled":x["cooling"]==true||loading(s,&k.block_id),"error":v.error})
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
    for _ in 0..200 {
        if inputs::ready(&s.peek())
            || (snapshot(&s.peek())["authority"]["homeCurrent"] == false
                && snapshot(&s.peek())["mounts"] == json!([])
                && snapshot(&s.peek())["activity"] == json!([]))
        {
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
        "widgets",
        "context",
        "requests",
        "responses",
    ] {
        if before[k] != after[k] {
            return Err(format!("Static native settlement changed {k}"));
        }
    }
    Ok(())
}
pub(super) async fn choose(
    s: Signal<ShellState>,
    d: &DesktopContext,
    block: &str,
    indices: Vec<Option<usize>>,
) -> Result<Value, String> {
    settle(s, d).await?;
    let id = crate::view::app_modal::field_id(&key(block));
    let before = snapshot(&s.peek())["events"].as_array().map_or(0, Vec::len);
    let proof=capture::dom(&format!(r#"(()=>{{{}const n=document.getElementById({});n?.scrollIntoView({{block:'center'}});if(!owned(n)||n.tagName!=='SELECT')return{{ok:false}};const events=[];for(const i of {}){{const value=i===null?'':`option-${{i}}`,o=[...n.options].find(o=>o.value===value);if(!o||o.disabled)return{{ok:false}};n.value=value;const e=new Event('change',{{bubbles:true,cancelable:true}});n.dispatchEvent(e);events.push({{index:i,value:n.value,trusted:e.isTrusted,prevented:e.defaultPrevented}});}}return{{ok:true,id:n.id,visibleHitTested:true,events}};}})()"#,capture::ACTION_GUARD,json!(id),json!(indices))).await?;
    if block != "quiet" {
        for _ in 0..150 {
            if snapshot(&s.peek())["events"].as_array().map_or(0, Vec::len)
                >= before + indices.len()
            {
                return Ok(proof);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        return Err("Static native callback missing".into());
    }
    for _ in 0..150 {
        let value = form(&s.peek())
            .and_then(|f| f.fields.get(&key(block)))
            .map(|f| f.value.clone());
        if matches!(value,Some(InputValue::Selected(ref v))if indices.last().copied().flatten().map(|i|["alpha","beta","gamma"][i]).map(|v|vec![v.to_owned()]).unwrap_or_default()==*v)
        {
            return Ok(proof);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err("Quiet select update missing".into())
}
pub(super) async fn counts(
    s: Signal<ShellState>,
    requests: usize,
    responses: usize,
) -> Result<(), String> {
    for _ in 0..240 {
        let v = snapshot(&s.peek());
        if v["requests"].as_array().map(Vec::len) == Some(requests)
            && v["responses"].as_array().map(Vec::len) == Some(responses)
            && v["windows"] == json!([])
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(15)).await;
    }
    Err(format!(
        "Static request settlement failed: {}",
        snapshot(&s.peek())
    ))
}
pub(super) async fn refusal(s: Signal<ShellState>, count: usize) -> Result<(), String> {
    for _ in 0..220 {
        if snapshot(&s.peek())["refusals"].as_array().map(Vec::len) == Some(count)
            && snapshot(&s.peek())["windows"] == json!([])
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(15)).await;
    }
    Err("Static refusal missing".into())
}
pub(super) async fn quiet(s: Signal<ShellState>, requests: usize) -> Result<Value, String> {
    let before = snapshot(&s.peek());
    tokio::time::sleep(Duration::from_millis(650)).await;
    let after = snapshot(&s.peek());
    if after["requests"].as_array().map(Vec::len) != Some(requests) {
        return Err("Unexpected static replay".into());
    }
    Ok(
        json!({"observedMs":650,"requests":requests,"beforeWindows":before["windows"],"afterWindows":after["windows"]}),
    )
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
    let mut g = crate::slack_links_check::wait_dom(d, &script).await?;
    let inventory = g["inventory"].clone();
    let required = g["requiredViews"]
        .as_array()
        .ok_or("Static coverage absent")?
        .clone();
    capture::capture_image(d, n, stage, &mut g, captures).await?;
    let mut views = vec![];
    for focus in required {
        let script = layout(&s.peek(), focus.clone());
        let mut v = crate::slack_links_check::wait_dom(d, &script).await?;
        if v["inventory"] != inventory || v["focusVisible"] != true {
            return Err("Static scroll inventory changed".into());
        }
        let name = format!("{stage}-{}", focus.as_str().ok_or("Static coverage key")?);
        capture::capture_image(d, n, &name, &mut v, captures).await?;
        views.push(json!({"key":focus,"captureKey":name,"geometry":v}));
    }
    let after = snapshot(&s.peek());
    if before != after {
        return Err("Static capture changed owned model".into());
    }
    stages
        .push(json!({"stage":stage,"geometry":g,"scrollViews":views,"model":after,"proof":proof}));
    Ok(())
}
pub(super) fn refresh(s: &mut ShellState, mutate: impl FnOnce(&mut Value)) -> Value {
    let h = s.app_home.surface.as_ref().unwrap();
    let team = h.owner.ctx.session.team_id.clone();
    let rt = h.owner.rt;
    let before = h.revision;
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    mutate(&mut raw);
    raw["hash"] = json!(format!("static-refresh-{before}"));
    let event = RtEvent::Unknown(
        serde_json::from_value(
            json!({"type":"view_updated","app_id":"A_HOME","view_id":"V_HOME","view":raw}),
        )
        .unwrap(),
    );
    crate::app_home::realtime(s, &team, rt, &event);
    json!({"before":before,"after":s.app_home.surface.as_ref().unwrap().revision})
}
pub(super) async fn gate(s: Signal<ShellState>) -> Result<Value, String> {
    for _ in 0..200 {
        if snapshot(&s.peek())["gateWaiting"] == true {
            return capture::dom(r#"(()=>{const entries=[...(window.__spHomeStatic?.entries.values()||[])].filter(e=>e.lease);return{ok:entries.length===2,entries:entries.map(e=>({mount:e.mount,seq:e.seq,acked:e.acked,index:e.index,value:e.node.value,lease:!!e.lease}))};})()"#).await;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err("Static delivery gate missing".into())
}
pub(super) fn hold(s: &mut ShellState) {
    let f = s.app_home.static_select.fixture.as_mut().unwrap();
    f.hold_gate = true;
    f.gate_waiting = false;
}
pub(super) fn release_gate(s: &mut ShellState) {
    let f = s.app_home.static_select.fixture.as_mut().unwrap();
    f.hold_gate = false;
    f.gate_waiting = false;
    if let Some(g) = f.gate.take() {
        let _ = g.send(());
    }
}
pub(super) async fn dismiss(
    s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    label: &str,
    captures: &mut Captures,
) -> Result<Value, String> {
    let script = format!(
        r#"(()=>{{{}const t=document.getElementById('status-toast'),b=t?.querySelector('.toast-dismiss[aria-label="Dismiss notification"]');if(!t)return{{ok:true,present:false}};if(!owned(b))return{{ok:false}};return{{ok:true,present:true,text:t.querySelector('.toast-text')?.textContent,control:b.getAttribute('aria-label'),visibleHitTested:true}};}})()"#,
        capture::ACTION_GUARD
    );
    let mut proof = capture::dom(&script).await?;
    if proof["present"] == true {
        let before = snapshot(&s.peek());
        capture::capture_image(d, n, label, &mut proof, captures).await?;
        capture::dom(&format!(r#"(()=>{{{}const b=document.querySelector('#status-toast .toast-dismiss');if(!owned(b))return{{ok:false}};b.click();return{{ok:true}};}})()"#,capture::ACTION_GUARD)).await?;
        crate::slack_links_check::wait_dom(d, "({ok:!document.getElementById('status-toast')})")
            .await?;
        if before != snapshot(&s.peek()) {
            return Err("Toast dismissal changed Home state".into());
        }
    }
    Ok(proof)
}
