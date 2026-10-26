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

async fn unseen(
    d: &DesktopContext,
    block: &str,
    family: &str,
    value: &str,
) -> Result<Value, String> {
    let id = crate::view::app_modal::field_id(&key(block));
    let script = format!(
        r#"(()=>{{{}const n=document.getElementById({}),family={},book=window.__spHomeText,e=[...(book?.entries.values()||[])].find(e=>e.node===n);n?.scrollIntoView({{block:'center'}});if(!owned(n)||n.type!==family||!e?.[family]||e.closed||e.seq!==e.acked||e.value!==n.value||e.lease||book.activeLease||n.readOnly||n.disabled)return{{ok:false}};const before={{mount:e.mount,seq:e.seq,acked:e.acked,value:n.value,observed:e.value,local:e.local}};Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(n,{});if(n.value!=={})return{{ok:false,error:'Native sibling sanitized requested fixture value'}};return{{ok:true,family,before,after:{{mount:e.mount,seq:e.seq,acked:e.acked,value:n.value,observed:e.value,local:e.local}},event:null,focusRequested:false,trusted:false,visibleHitTested:true}};}})()"#,
        capture::ACTION_GUARD,
        json!(id),
        json!(family),
        json!(value),
        json!(value)
    );
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
    Err("Unseen sibling did not refuse Button".into())
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
    let p = input(s, d, "live", "blocked@example.com").await?;
    let quiet = silence(s).await?;
    counts(s, 0, 0, 0, 0).await?;
    let refused = snapshot(&s.peek());
    if refused["admission"]["attempts"].as_array().map(Vec::len) != Some(1)
        || refused["admission"]["attempts"][0]["accepted"] != false
        || refused["formState"]["values"]["day"] != before["formState"]["values"]["day"]
    {
        return Err("Unseen Date was not refused without changing its parent".into());
    }
    let correction = date_buffer(d, &day, Some(&unseen["before"])).await?;
    let ack = acknowledged(d, &correction).await?;
    if correction["after"]["seq"].as_u64() != correction["before"]["seq"].as_u64().map(|v| v + 1) {
        return Err("Date correction was not exactly one input".into());
    }
    let no_replay = silence(s).await?;
    shot(s,d,n,"unseen-date-refused",json!({"before":before,"unseen":unseen,"input":p,"silence":quiet,"refused":refused,"correction":correction,"ack":ack,"noReplay":no_replay}),st,cap).await?;
    let unseen = self::unseen(d, "quiet", "email", "Unseen@Example.com").await?;
    let p = input(s, d, "live", "blocked2@example.com").await?;
    let quiet = silence(s).await?;
    counts(s, 0, 0, 0, 0).await?;
    let refused = snapshot(&s.peek());
    if refused["formState"]["values"]["quiet"]["value"] != json!({"type":"email_text_input"}) {
        return Err("Unseen email rewrote parent".into());
    }
    let correction = input(s, d, "quiet", "Unseen@Example.com").await?;
    let no_replay = silence(s).await?;
    shot(s,d,n,"unseen-email-refused",json!({"unseen":unseen,"input":p,"silence":quiet,"refused":refused,"correction":correction,"noReplay":no_replay}),st,cap).await?;
    let p = input(s, d, "live", "accepted@example.com").await?;
    counts(s, 1, 1, 0, 0).await?;
    shot(s, d, n, "fresh-after-correction", p, st, cap).await?;
    let unseen = self::unseen(d, "amount", "number", "11").await?;
    let clicked = button().await?;
    let refused = button_refused(s, 1).await?;
    counts(s, 1, 1, 0, 0).await?;
    let correction = amount(s, d, "11").await?;
    let quiet = silence(s).await?;
    shot(s,d,n,"button-unseen-number-refused",json!({"unseen":unseen,"button":clicked,"refused":refused,"correction":correction,"silence":quiet}),st,cap).await?;
    let clicked = button().await?;
    counts(s, 1, 1, 1, 0).await?;
    if !buttons::release(&mut s.write(), true) {
        return Err("Email admission Button reply missing".into());
    }
    counts(s, 1, 1, 1, 1).await?;
    shot(s, d, n, "button-current-parent", clicked, st, cap).await?;
    let started = Instant::now();
    let a = input(s, d, "live", "lead@example.com").await?;
    counts(s, 2, 2, 1, 1).await?;
    let leading = helpers::leading(&s.peek(), started, 2)?;
    let b = input(s, d, "live", "queued@example.com").await?;
    let before_reset = trailing(&s.peek(), started, 2)?;
    let reset = reset(s, d).await?;
    let quiet = silence(s).await?;
    counts(s, 2, 2, 1, 1).await?;
    let after = snapshot(&s.peek());
    if after["emailWindows"] != json!([])
        || after["formState"]["values"]["live"]["value"] != json!({"type":"email_text_input"})
        || after["formState"]["values"]["quiet"]["value"] != json!({"type":"email_text_input"})
        || after["formState"]["values"]["amount"]["value"]
            != json!({"type":"number_input","value":"5.00"})
    {
        return Err("Email Reset failed to cancel trailing work and recreate parent".into());
    }
    for row in after["emailMounts"]
        .as_array()
        .ok_or("Email Reset mounts missing")?
    {
        let old = before_reset["model"]["emailMounts"]
            .as_array()
            .and_then(|r| r.iter().find(|r| r["block"] == row["block"]))
            .ok_or("Prior email mount missing")?;
        if row["current"] != true || row["mount"] == old["mount"] {
            return Err("Email Reset reused native mount".into());
        }
    }
    shot(s,d,n,"reset-cancels-trailing",json!({"first":a,"leading":leading,"queued":b,"beforeReset":before_reset,"reset":reset,"silence":quiet}),st,cap).await
}
