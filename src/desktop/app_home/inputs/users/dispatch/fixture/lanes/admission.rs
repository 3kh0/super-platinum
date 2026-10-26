//! Actual picker occurrences against unseen sibling custody and Reset retirement.
use super::*;

pub(in crate::app_home::inputs::users::dispatch::fixture) async fn date_buffer(
    d: &DesktopContext,
    expected: &Value,
    restore: Option<&Value>,
) -> Result<Value, String> {
    let dom_id = crate::view::app_modal::field_id(&key("day"));
    if expected["id"].as_u64().is_none()
        || expected["mount"].as_u64().is_none()
        || expected["seq"].as_u64().is_none()
        || expected["pending"] != false
        || expected["invalid"] != false
    {
        return Err("Date registry proof is not acknowledged".into());
    }
    let script = format!(
        r#"(()=>{{{}const x={},domId={},book=window.__spHomeCalendarNative,e=book?.entries.get(x.id),n=document.getElementById(domId);n?.scrollIntoView({{block:'center'}});if(!owned(n)||!e||!e.editableDate||!e.nativeCurrent()||e.node!==n||n.type!=='text'||e.picker.type!=='date'||n.parentNode!==e.host||e.picker.parentNode!==e.host||n.disabled||e.picker.disabled||e.picker.readOnly||e.mount!==x.mount||e.seq!==x.seq||e.closed||e.pending||e.acked!==x.seq||e.lease||book.activeLease||n.readOnly||e.host?.dataset.locked==='true')return{{ok:false}};return{{ok:true}};}})()"#,
        capture::ACTION_GUARD,
        expected,
        json!(dom_id)
    );
    crate::slack_links_check::wait_dom(d, &script).await?;
    let restore = restore.cloned().unwrap_or(Value::Null);
    capture::dom(&format!(r#"(()=>{{{}const x={},domId={},r={},id=x.id,book=window.__spHomeCalendarNative,e=book?.entries.get(id),n=document.getElementById(domId);n?.scrollIntoView({{block:'center'}});if(!owned(n)||!e||!e.editableDate||!e.nativeCurrent()||e.node!==n||n.type!=='text'||e.picker.type!=='date'||n.parentNode!==e.host||e.picker.parentNode!==e.host||n.disabled||e.picker.disabled||e.picker.readOnly||e.mount!==x.mount||e.seq!==x.seq||e.closed||e.pending||e.acked!==x.seq||e.lease||book.activeLease||n.readOnly||e.host?.dataset.locked==='true')return{{ok:false}};const before={{id,domId,mount:e.mount,seq:e.seq,acked:e.acked,value:n.value,picker:e.picker.value,snapshotValue:e.snapshotValue(),ackedValue:e.ackedValue,selected:e.selected,pending:e.pending}};if(r&&(r.id!==id||r.domId!==domId||r.mount!==e.mount||r.seq!==e.seq||r.acked!==e.acked||r.picker!==e.picker.value||r.snapshotValue!==e.ackedValue||n.value!=='2028-02-29'))return{{ok:false,error:'Date custody changed before correction'}};if(!r&&(e.snapshotValue()!==x.value||e.snapshotValue()!==e.ackedValue))return{{ok:false}};Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(n,r?r.value:'2028-02-29');if(r)n.dispatchEvent(new Event('input',{{bubbles:true}}));return{{ok:true,before,after:{{id,domId,mount:e.mount,seq:e.seq,acked:e.acked,value:n.value,picker:e.picker.value,snapshotValue:e.snapshotValue(),ackedValue:e.ackedValue,selected:e.selected,pending:e.pending}},event:r?'input':null,focusRequested:false,trusted:false,visibleHitTested:true}};}})()"#,capture::ACTION_GUARD,expected,json!(dom_id),restore)).await
}
pub(in crate::app_home::inputs::users::dispatch::fixture) async fn acknowledged(
    d: &DesktopContext,
    proof: &Value,
) -> Result<Value, String> {
    let expected = &proof["after"];
    crate::slack_links_check::wait_dom(d,&format!(r#"(()=>{{const x={},e=window.__spHomeCalendarNative?.entries.get(x.id),n=document.getElementById(x.domId);return{{ok:!!e&&e.editableDate&&e.nativeCurrent()&&e.node===n&&n?.isConnected&&n.type==='text'&&e.picker.type==='date'&&n.parentNode===e.host&&e.picker.parentNode===e.host&&!n.disabled&&!n.readOnly&&!e.picker.disabled&&!e.picker.readOnly&&!e.closed&&!e.pending&&!e.lease&&e.mount===x.mount&&e.seq===x.seq&&e.acked===x.seq&&n.value===x.value&&e.picker.value===x.picker&&e.snapshotValue()===x.snapshotValue&&e.ackedValue===x.snapshotValue,id:x.id,domId:x.domId,mount:e?.mount,seq:e?.seq,acked:e?.acked,value:n?.value,picker:e?.picker.value,snapshotValue:e?.snapshotValue(),ackedValue:e?.ackedValue,selected:e?.selected,pending:e?.pending}};}})()"#,expected)).await
}
pub(in crate::app_home::inputs::users::dispatch::fixture) async fn hold_directory(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
) -> Result<Value, String> {
    people::configure(&mut s.write(), people::Reply::People, true);
    let before = snapshot(&s.peek());
    let count = before["directory"]["requests"]
        .as_array()
        .ok_or("Directory ledger missing")?
        .len();
    let opened = open(d, "pick").await?;
    let selector = "#app-home [data-modal-users='pick'] input[type=search]";
    let input = capture::dom(&format!(r#"(()=>{{{}const n=document.querySelector({});n?.scrollIntoView({{block:'center'}});if(!owned(n))return{{ok:false}};Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(n,'held-reset');n.dispatchEvent(new Event('input',{{bubbles:true}}));return{{ok:true,value:n.value,trusted:false,visibleHitTested:true}};}})()"#,capture::ACTION_GUARD,json!(selector))).await?;
    for _ in 0..250 {
        let x = snapshot(&s.peek());
        if x["directory"]["requests"].as_array().map(Vec::len) == Some(count + 1)
            && x["directory"]["pending"] == true
            && x["directory"]["query"]["loading"] == true
            && x["directory"]["query"]["current"] == true
        {
            return Ok(
                json!({"open":opened,"input":input,"beforeRequests":count,"held":x["directory"]}),
            );
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err("User Reset directory request was not held".into())
}

pub(in super::super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    settle(s, d).await?;
    let before = snapshot(&s.peek());
    let day = before["nativeMounts"]
        .as_array()
        .and_then(|rows| {
            rows.iter()
                .find(|e| e["block"] == "day" && e["action"] == "value")
        })
        .ok_or("Current Date registry entry missing")?
        .clone();
    let unseen = date_buffer(d, &day, None).await?;
    let choice = choose(s, d, "pick", BLAIR).await?;
    refused(s, 1).await?;
    counts(s, 0, 0, 0, 0).await?;
    let refused_model = snapshot(&s.peek());
    if refused_model["formState"]["values"]["day"] != before["formState"]["values"]["day"]
        || refused_model["refusals"][0]["reason"] != "sibling-custody"
    {
        return Err("Unseen Date buffer did not refuse user sibling custody".into());
    }
    let correction = date_buffer(d, &day, Some(&unseen["before"])).await?;
    let ack = acknowledged(d, &correction).await?;
    if correction["after"]["seq"].as_u64() != correction["before"]["seq"].as_u64().map(|v| v + 1) {
        return Err("Date correction did not publish exactly one input observation".into());
    }
    shot(s,d,n,"unseen-date-refused",json!({"before":before,"unseen":unseen,"choice":choice,"refused":refused_model,"correction":correction,"ack":ack}),st,cap).await?;

    let fresh = choose(s, d, "pick", CASEY).await?;
    counts(s, 1, 1, 0, 0).await?;
    shot(s, d, n, "fresh-choice", fresh, st, cap).await?;

    let quiet = choose(s, d, "quiet", BLAIR).await?;
    let multi = choose(s, d, "multi", BLAIR).await?;
    let notes = notes("Admission latest sibling").await?;
    let mut notes_received = false;
    for _ in 0..150 {
        if snapshot(&s.peek())["formState"]["values"]["notes"]["value"]["value"]
            == "Admission latest sibling"
        {
            notes_received = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    if !notes_received {
        return Err("Quiet notes input was not acknowledged".into());
    }
    let clicked = click(s, d).await?;
    counts(s, 1, 1, 1, 0).await?;
    if !buttons::release(&mut s.write(), true) {
        return Err("Admission Button reply was not held".into());
    }
    counts(s, 1, 1, 1, 1).await?;
    shot(
        s,
        d,
        n,
        "button-latest",
        json!({"quiet":quiet,"multi":multi,"notes":notes,"click":clicked}),
        st,
        cap,
    )
    .await?;

    let held = hold_directory(s, d).await?;
    shot(s, d, n, "directory-held", held, st, cap).await?;
    let before_reset = snapshot(&s.peek());
    let reset = reset(s, d).await?;
    let mut retired = false;
    for _ in 0..250 {
        if people::pending_closed(&s.peek()) {
            retired = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    if !retired {
        return Err("Reset did not cancel the owned directory receiver".into());
    }
    let release = people::release(&mut s.write());
    settle(s, d).await?;
    let after = snapshot(&s.peek());
    if release["delivered"] != false
        || !after["pickerBlock"].is_null()
        || after["formState"]["values"]["pick"]["value"]["selected_user"] != ADA
        || after["formState"]["values"]["quiet"]["value"]["selected_user"] != ADA
        || after["formState"]["values"]["multi"]["value"]["selected_users"] != json!([ADA])
        || after["multiVisible"] != json!([ADA])
        || after["formState"]["values"]["notes"]["value"]["value"] != "Sibling"
        || after["requests"] != before_reset["requests"]
        || after["buttonSync"]["requests"] != before_reset["buttonSync"]["requests"]
    {
        return Err(
            "Reset did not reseed single/multi state or admitted the retired response".into(),
        );
    }
    for (block, visible) in [
        ("pick", json!(ADA)),
        ("quiet", json!(ADA)),
        ("required", Value::Null),
    ] {
        let widget = |model: &Value| {
            model["userWidgets"]
                .as_array()
                .and_then(|rows| rows.iter().find(|w| w["block"] == block))
                .cloned()
        };
        let old = widget(&before_reset).ok_or("Reset prior user widget missing")?;
        let new = widget(&after).ok_or("Reset replacement user widget missing")?;
        if new["visible"] != visible || new["current"] != true || new["instance"] == old["instance"]
        {
            return Err(format!("Reset reused stale single-user widget {block}"));
        }
    }
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "reset-retired",
        json!({"before":before_reset,"reset":reset,"release":release,"silence":quiet}),
        st,
        cap,
    )
    .await
}
