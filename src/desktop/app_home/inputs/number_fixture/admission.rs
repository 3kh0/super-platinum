use super::*;
async fn date_buffer(
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
async fn acknowledged(d: &DesktopContext, proof: &Value) -> Result<Value, String> {
    let expected = &proof["after"];
    crate::slack_links_check::wait_dom(d,&format!(r#"(()=>{{const x={},e=window.__spHomeCalendarNative?.entries.get(x.id),n=document.getElementById(x.domId);return{{ok:!!e&&e.editableDate&&e.nativeCurrent()&&e.node===n&&n?.isConnected&&n.type==='text'&&e.picker.type==='date'&&n.parentNode===e.host&&e.picker.parentNode===e.host&&!n.disabled&&!n.readOnly&&!e.picker.disabled&&!e.picker.readOnly&&!e.closed&&!e.pending&&!e.lease&&e.mount===x.mount&&e.seq===x.seq&&e.acked===x.seq&&n.value===x.value&&e.picker.value===x.picker&&e.snapshotValue()===x.snapshotValue&&e.ackedValue===x.snapshotValue,id:x.id,domId:x.domId,mount:e?.mount,seq:e?.seq,acked:e?.acked,value:n?.value,picker:e?.picker.value,snapshotValue:e?.snapshotValue(),ackedValue:e?.ackedValue,selected:e?.selected,pending:e?.pending}};}})()"#,expected)).await
}

async fn unseen_number(d: &DesktopContext, value: &str) -> Result<Value, String> {
    let id = crate::view::app_modal::field_id(&key("quiet"));
    let script = format!(
        r#"(()=>{{{}const n=document.getElementById({}),book=window.__spHomeText,e=[...(book?.entries.values()||[])].find(e=>e.node===n);n?.scrollIntoView({{block:'center'}});if(!owned(n)||n.type!=='number'||!e?.number||e.closed||e.seq!==e.acked||e.value!==n.value||e.lease||book.activeLease||n.readOnly||n.disabled)return{{ok:false}};const before={{mount:e.mount,seq:e.seq,acked:e.acked,value:n.value,observed:e.value,local:e.local}};Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(n,{});return{{ok:true,before,after:{{mount:e.mount,seq:e.seq,acked:e.acked,value:n.value,observed:e.value,local:e.local}},event:null,focusRequested:false,trusted:false,visibleHitTested:true}};}})()"#,
        capture::ACTION_GUARD,
        json!(id),
        json!(value)
    );
    crate::slack_links_check::wait_dom(
        d,
        &format!("({{ok:!!document.getElementById({})}})", json!(id)),
    )
    .await?;
    capture::dom(&script).await
}
async fn button_refused(s: Signal<ShellState>, count: usize) -> Result<Value, String> {
    for _ in 0..200 {
        let x = snapshot(&s.peek());
        if x["buttonSync"]["refusals"].as_array().map(Vec::len) == Some(count) {
            return Ok(x);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err("Numeric sibling did not refuse Button".into())
}
pub(super) async fn run(
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
        .and_then(|r| r.iter().find(|r| r["block"] == "day"))
        .ok_or("Date mount missing")?
        .clone();
    let unseen = date_buffer(d, &day, None).await?;
    let p = input(s, d, "live", "6", json!("6"), wire(json!("6"))).await?;
    let quiet = silence(s).await?;
    counts(s, 0, 0, 0, 0).await?;
    let refused = snapshot(&s.peek());
    if refused["admission"]["attempts"].as_array().map(Vec::len) != Some(1)
        || refused["admission"]["attempts"][0]["accepted"] != false
        || refused["formState"]["values"]["day"] != before["formState"]["values"]["day"]
    {
        return Err("Unseen Date did not refuse numeric action without parent mutation".into());
    }
    let correction = date_buffer(d, &day, Some(&unseen["before"])).await?;
    let ack = acknowledged(d, &correction).await?;
    if correction["after"]["seq"].as_u64() != correction["before"]["seq"].as_u64().map(|v| v + 1) {
        return Err("Date correction was not exactly one observed input".into());
    }
    let no_replay = silence(s).await?;
    shot(s,d,n,"unseen-date-refused",json!({"before":before,"unseen":unseen,"input":p,"silence":quiet,"refused":refused,"correction":correction,"ack":ack,"noReplay":no_replay}),st,cap).await?;
    let unseen = unseen_number(d, "11").await?;
    let p = input(s, d, "live", "7", json!("7"), wire(json!("7"))).await?;
    let quiet = silence(s).await?;
    counts(s, 0, 0, 0, 0).await?;
    let refused = snapshot(&s.peek());
    if refused["formState"]["values"]["quiet"]["value"] != wire(json!("5.00")) {
        return Err("Unseen number rewrote committed parent".into());
    }
    let correction = input(s, d, "quiet", "11", json!("11"), wire(json!("11"))).await?;
    let no_replay = silence(s).await?;
    shot(s,d,n,"unseen-number-refused",json!({"unseen":unseen,"input":p,"silence":quiet,"refused":refused,"correction":correction,"noReplay":no_replay}),st,cap).await?;
    let p = input(s, d, "live", "8", json!("8"), wire(json!("8"))).await?;
    counts(s, 1, 1, 0, 0).await?;
    shot(s, d, n, "fresh-after-correction", p, st, cap).await?;
    let unseen = unseen_number(d, "12").await?;
    let clicked = button().await?;
    let refused = button_refused(s, 1).await?;
    counts(s, 1, 1, 0, 0).await?;
    let correction = input(s, d, "quiet", "12", json!("12"), wire(json!("12"))).await?;
    let quiet = silence(s).await?;
    shot(s,d,n,"button-unseen-number-refused",json!({"unseen":unseen,"button":clicked,"refused":refused,"correction":correction,"silence":quiet}),st,cap).await?;
    let clicked = button().await?;
    counts(s, 1, 1, 1, 0).await?;
    if !buttons::release(&mut s.write(), true) {
        return Err("Numeric admission Button reply missing".into());
    }
    counts(s, 1, 1, 1, 1).await?;
    shot(s, d, n, "button-current-parent", clicked, st, cap).await?;
    let started = Instant::now();
    let p = input(s, d, "live", "9", json!("9"), wire(json!("9"))).await?;
    let before_reset = trailing(&s.peek(), started, 1)?;
    let reset = reset(s, d).await?;
    let quiet = silence(s).await?;
    counts(s, 1, 1, 1, 1).await?;
    let after = snapshot(&s.peek());
    if after["queue"] != json!([])
        || after["formState"]["values"]["live"]["value"] != wire(json!("3"))
        || after["formState"]["values"]["quiet"]["value"] != wire(json!("5.00"))
    {
        return Err("Reset failed to cancel queued number action/reseed parent".into());
    }
    for row in after["numberMounts"]
        .as_array()
        .ok_or("Reset numeric mounts missing")?
    {
        let old = before_reset["model"]["numberMounts"]
            .as_array()
            .and_then(|r| r.iter().find(|r| r["block"] == row["block"]))
            .ok_or("Prior numeric mount missing")?;
        if row["current"] != true || row["mount"] == old["mount"] {
            return Err("Reset reused numeric native mount".into());
        }
    }
    shot(
        s,
        d,
        n,
        "reset-cancels-queued",
        json!({"input":p,"beforeReset":before_reset,"reset":reset,"silence":quiet}),
        st,
        cap,
    )
    .await
}
