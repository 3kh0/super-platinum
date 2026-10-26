//! External callbacks reuse the accepted Date custody and owned Reset contract.
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
        .and_then(|r| {
            r.iter()
                .find(|r| r["block"] == "day" && r["action"] == "value")
        })
        .ok_or("Date registry missing")?
        .clone();
    let unseen = date_buffer(d, &day, None).await?;
    let choice = choose(s, d, "pick", 0, alpha()).await?;
    refused(s, 1).await?;
    counts(s, 0, 0, 0, 0).await?;
    let refused_model = snapshot(&s.peek());
    if refused_model["formState"]["values"]["day"] != before["formState"]["values"]["day"]
        || refused_model["refusals"][0]["reason"] != "sibling-custody"
    {
        return Err("Unseen Date did not refuse external callback custody".into());
    }
    let correction = date_buffer(d, &day, Some(&unseen["before"])).await?;
    let ack = acknowledged(d, &correction).await?;
    if correction["after"]["seq"].as_u64() != correction["before"]["seq"].as_u64().map(|v| v + 1) {
        return Err("Date correction was not exactly one observed input".into());
    }
    shot(s,d,n,"unseen-date-refused",json!({"before":before,"unseen":unseen,"choice":choice,"refused":refused_model,"correction":correction,"ack":ack}),st,cap).await?;
    let fresh = choose(s, d, "pick", 1, beta()).await?;
    counts(s, 1, 1, 0, 0).await?;
    shot(s, d, n, "fresh-choice", fresh, st, cap).await?;
    let quiet = choose(s, d, "quiet", 0, alpha()).await?;
    let multi_open = open(s, d, "multi").await?;
    let multi =
        capture::click("#app-home [data-home-external='multi'] [data-option-index='0']").await?;
    let mut committed = false;
    for _ in 0..150 {
        if snapshot(&s.peek())["formState"]["values"]["multi"]["value"]["selected_options"]
            == json!([installed(), alpha()])
        {
            committed = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    if !committed {
        return Err("Quiet multi did not commit exact descriptors".into());
    }
    let done = close(d, "multi").await?;
    let notes = notes_received(s, "Admission latest sibling").await?;
    let pressed = click(s, d).await?;
    counts(s, 1, 1, 1, 0).await?;
    if !buttons::release(&mut s.write(), true) {
        return Err("External admission Button reply not held".into());
    }
    counts(s, 1, 1, 1, 1).await?;
    shot(s,d,n,"button-latest",json!({"quiet":quiet,"multiOpen":multi_open,"multi":multi,"done":done,"notes":notes,"click":pressed}),st,cap).await?;
    let opened = open(s, d, "pick").await?;
    suggestions::configure(&mut s.write(), suggestions::Reply::Replacement, true);
    let held = query(s, d, "pick", "held-reset", 6, true).await?;
    shot(
        s,
        d,
        n,
        "suggestions-held",
        json!({"open":opened,"query":held}),
        st,
        cap,
    )
    .await?;
    let before_reset = snapshot(&s.peek());
    let reset = reset(s, d).await?;
    let mut retired = false;
    for _ in 0..250 {
        if suggestions::pending_closed(&s.peek()) {
            retired = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    if !retired {
        return Err("Reset did not cancel owned external query receiver".into());
    }
    let release = suggestions::release(&mut s.write());
    settle(s, d).await?;
    let after = snapshot(&s.peek());
    let values = &after["formState"]["values"];
    if release["delivered"] != false
        || !after["pickerBlock"].is_null()
        || values["pick"]["value"]["selected_option"] != installed()
        || values["quiet"]["value"]["selected_option"] != installed()
        || values["required"]["value"] != json!({"type":"external_select"})
        || values["multi"]["value"]["selected_options"] != json!([installed()])
        || values["notes"]["value"]["value"] != "Sibling"
        || !values["day"]["value"]["selected_date"].is_null()
        || after["requests"] != before_reset["requests"]
        || after["buttonSync"]["requests"] != before_reset["buttonSync"]["requests"]
        || after["suggestions"]["requests"].as_array().map(Vec::len) != Some(6)
    {
        return Err("External Reset did not reseed form or retired response delivered".into());
    }
    for block in ["pick", "quiet", "required"] {
        let widget = |x: &Value| {
            x["externalWidgets"]
                .as_array()
                .and_then(|r| r.iter().find(|w| w["block"] == block))
                .cloned()
        };
        let old = widget(&before_reset).ok_or("Prior single external widget missing")?;
        let new = widget(&after).ok_or("Reset single external widget missing")?;
        if new["current"] != true
            || new["instance"] == old["instance"]
            || new["revision"] == old["revision"]
            || new["visible"]
                != if block == "required" {
                    Value::Null
                } else {
                    installed()
                }
        {
            return Err("Reset reused old external single widget".into());
        }
    }
    let multi = |x: &Value| {
        x["multiExternalWidgets"]
            .as_array()
            .and_then(|r| r.iter().find(|w| w["block"] == "multi"))
            .map(|w| w["widget"].clone())
    };
    let old = multi(&before_reset).ok_or("Prior quiet multi widget missing")?;
    let new = multi(&after).ok_or("Reset quiet multi widget missing")?;
    if new["visible"] != json!([installed()])
        || new["accepted"] != json!([installed()])
        || new["revision"] == old["revision"]
        || new["mount"] != after["authority"]["homeVisit"]
        || old["mount"] != before_reset["authority"]["homeVisit"]
        || after["authority"]["homeVisit"] != before_reset["authority"]["homeVisit"]
        || new["invalid"] != false
        || !new["error"].is_null()
    {
        return Err("Reset retained old quiet multi widget".into());
    }
    counts(s, 1, 1, 1, 1).await?;
    let silence = silence(s).await?;
    shot(
        s,
        d,
        n,
        "reset-retired",
        json!({"before":before_reset,"reset":reset,"release":release,"silence":silence}),
        st,
        cap,
    )
    .await
}
