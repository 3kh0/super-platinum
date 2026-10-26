use super::*;
pub(super) type Captures = serde_json::Map<String, Value>;
fn bound(s: &ShellState) -> Value {
    let mut x = snapshot(s);
    x["domFields"]=json!(form(s).map(|f|f.fields.iter().map(|(k,v)| {
        let (kind,value)=match (&v.kind,&v.value){
            (InputKind::Radio{..},InputValue::Selected(values))=>("radio",json!(values.first())),
            (_,InputValue::Text(v))=>("text",json!(v)),(_,InputValue::Date(v))=>("date",json!(v.clone().unwrap_or_default())),_=>("unknown",Value::Null)};
        let options=match &v.kind{InputKind::Radio{options}=>json!(options.iter().map(|o|o.raw.clone()).collect::<Vec<_>>()),_=>Value::Null};
        let error=if kind=="radio" {super::super::presentation_error(s,k)} else {v.error.clone()};
        json!({"id":crate::view::app_modal::field_id(k),"block":k.block_id,"action":k.action_id,"kind":kind,"value":value,"options":options,"disabled":x["cooling"]==true||(kind=="radio"&&loading(s,&k.block_id)),"error":error})
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
            && x["radioMounts"] == json!([])
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
            "Radio source settlement timed out: {}",
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
        "radioWidgets",
        "radioMounts",
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
                "Radio native settlement changed {k}: {}",
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
        .ok_or("Radio capture coverage missing")?
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
            return Err("Radio capture inventory changed".into());
        }
        let name = format!("{stage}-{}", focus.as_str().ok_or("Radio focus key")?);
        capture::capture_image(d, n, &name, &mut v, cap).await?;
        covered.extend(visible(&v));
        views.push(json!({"key":focus,"captureKey":name,"geometry":v}));
    }
    let missing = inventory["captureTargets"]
        .as_array()
        .ok_or("Radio target inventory missing")?
        .iter()
        .filter_map(|t| t["key"].as_str())
        .filter(|k| !covered.contains(*k))
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(format!(
            "Radio scroll coverage incomplete: {}",
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
            "Radio capture changed owned model: {}",
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
    edit_with_notes(s, d, block, ops, None).await
}
pub(super) async fn edit_sibling(
    s: Signal<ShellState>,
    d: &DesktopContext,
    block: &str,
    ops: Vec<Value>,
    notes: &str,
) -> Result<Value, String> {
    edit_with_notes(s, d, block, ops, Some(notes)).await
}
async fn edit_with_notes(
    s: Signal<ShellState>,
    d: &DesktopContext,
    block: &str,
    ops: Vec<Value>,
    notes: Option<&str>,
) -> Result<Value, String> {
    settle(s, d).await?;
    let id = crate::view::app_modal::field_id(&key(block));
    let before = snapshot(&s.peek())["events"].as_array().map_or(0, Vec::len);
    let proof=capture::dom(&format!(r#"await (async()=>{{{}const root=document.getElementById({});if(!root)return{{ok:false}};const started=performance.now(),actions=[];for(const op of {}){{let e,entry,book;const current=()=>book&&entry&&book.entries.get(entry.mount)===entry&&!entry.closed&&entry.radio&&entry.node===root&&entry.host.isConnected&&root.parentNode===entry.host&&document.getElementById(root.id)===root&&entry.rows.includes(e)&&e.isConnected&&root.contains(e);const available=()=>current()&&!entry.lease&&!book.activeLease&&entry.host.dataset.locked!=="true"&&!e.disabled&&entry.acked===entry.seq;for(let t=0;t<200;t++){{book=window.__spHomeStatic;entry=[...(book?.entries.values()||[])].find(v=>v.node===root);e=root.querySelector(`input[data-option-index="${{op.index}}"]`);e?.scrollIntoView({{block:'center'}});if(owned(e)&&available())break;await new Promise(r=>setTimeout(r,5));}}if(!owned(e)||!available())return{{ok:false,error:'Radio row custody not available',op,mount:entry?.mount,seq:entry?.seq,acked:entry?.acked,leased:!!entry?.lease,activeLease:!!book?.activeLease}};if(e.checked)return{{ok:false,error:'Radio change requested already selected row',op}};const before=e.checked,beforeSeq=entry.seq,expectedSeq=beforeSeq+1,mount=entry.mount;e.click();if(!current()||entry.seq!==expectedSeq)return{{ok:false,error:'Radio click did not create exact occurrence',op,mount,beforeSeq,expectedSeq,actualSeq:entry.seq}};let acknowledged=false,ackSeq=null;for(let t=0;t<200;t++){{if(!current()||entry.seq!==expectedSeq)return{{ok:false,error:'Radio occurrence ownership changed',op,mount,expectedSeq,actualSeq:entry.seq}};if(entry.acked===expectedSeq){{acknowledged=true;ackSeq=entry.acked;break;}}await new Promise(r=>setTimeout(r,5));}}if(!acknowledged)return{{ok:false,error:'Radio occurrence not acknowledged',op,mount,expectedSeq,ackSeq:entry.acked}};actions.push({{op,mount,beforeSeq,expectedSeq,leaseFree:true,checkedBefore:before,checkedAfter:e.checked,trusted:false,visibleHitTested:true,acknowledged,ackSeq,elapsedMs:performance.now()-started}});}}let sibling=null;const note={};if(note){{const input=document.getElementById(note.id);input?.scrollIntoView({{block:"center"}});if(!owned(input))return{{ok:false,error:"Sibling not available"}};input.value=note.value;input.dispatchEvent(new Event("input",{{bubbles:true}}));sibling={{value:input.value,trusted:false,visibleHitTested:true}};}}return{{ok:true,id:root.id,actions,sibling,elapsedMs:performance.now()-started}};}})()"#,capture::ACTION_GUARD,json!(id),json!(ops),json!(notes.map(|value|json!({"id":crate::view::app_modal::field_id(&key("notes")),"value":value}))))).await?;
    for _ in 0..200 {
        let x = snapshot(&s.peek());
        if x["events"].as_array().map_or(0, Vec::len) == before + ops.len() {
            return Ok(proof);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err("Radio native callback missing".into())
}
pub(super) fn select(index: usize) -> Value {
    json!({"kind":"select","index":index})
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
        "Radio ledger did not settle {}",
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
    raw["hash"] = json!(format!("radio-refresh-{before}"));
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
    Err("Radio native lease gate missing".into())
}
pub(super) async fn refused(s: Signal<ShellState>, count: usize) -> Result<(), String> {
    for _ in 0..300 {
        let x = snapshot(&s.peek());
        if x["refusals"].as_array().map(Vec::len) == Some(count) && x["windows"] == json!([]) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err("Radio stale refusal missing".into())
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
        return Err("Radio toast dismissal changed model".into());
    }
    Ok(json!({"captureKey":capture_key,"geometry":g,"click":click,"modelUnchanged":true}))
}

// Clicking the checked native row is a real gesture but creates no change event.
pub(super) async fn same_checked(
    s: Signal<ShellState>,
    d: &DesktopContext,
    block: &str,
    index: usize,
) -> Result<Value, String> {
    settle(s, d).await?;
    let before = snapshot(&s.peek());
    let id = crate::view::app_modal::field_id(&key(block));
    let proof=capture::dom(&format!(r#"(()=>{{{}const root=document.getElementById({}),book=window.__spHomeStatic,entry=[...(book?.entries.values()||[])].find(v=>v.node===root),row=root?.querySelector('input[data-option-index="{}"]');row?.scrollIntoView({{block:'center'}});if(!owned(row)||!entry?.radio||entry.closed||!entry.host.isConnected||root.parentNode!==entry.host||document.getElementById(root.id)!==root||!entry.rows.includes(row)||entry.lease||book.activeLease||entry.host.dataset.locked==='true'||row.disabled||!row.checked||entry.acked!==entry.seq)return{{ok:false}};const seq=entry.seq,value=entry.value;row.click();return{{ok:book.entries.get(entry.mount)===entry&&entry.seq===seq&&entry.acked===seq&&entry.value===value&&row.checked,mount:entry.mount,index:{},beforeSeq:seq,afterSeq:entry.seq,value,checked:true,leaseFree:true,visibleHitTested:true,trusted:false}};}})()"#,capture::ACTION_GUARD,json!(id),index,index)).await?;
    let silence = silence(s).await?;
    if snapshot(&s.peek()) != before {
        return Err("Checked radio gesture changed owned model".into());
    }
    Ok(json!({"click":proof,"silence":silence,"modelUnchanged":true}))
}
