use super::*;
pub(super) async fn admission(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut h::Captures,
) -> Result<(), String> {
    let edit = h::edit(s, d, "enter", "acknowledged").await?;
    h::settle(s, d).await?;
    let press = click().await?;
    counts(s, 1, 0, 0).await?;
    let released = unlocked().await?;
    shot(
        s,
        d,
        n,
        "acknowledged-held",
        json!({"edit":edit,"click":press,"released":released}),
        st,
        cap,
    )
    .await?;
    if !release(&mut s.write(), true) {
        return Err("Missing held Button".into());
    }
    counts(s, 1, 1, 0).await?;
    shot(s, d, n, "acknowledged", json!({}), st, cap).await?;
    let before = snapshot(&s.peek());
    let id = crate::view::app_modal::field_id(&key("enter"));
    let raw=capture::dom(&format!(r#"(()=>{{{}const e=document.getElementById({});if(!owned(e))return{{ok:false}};const before=e.value;Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,'unsignaled');return{{ok:true,before,value:e.value,inputDispatched:false,visibleHitTested:true}};}})()"#,capture::ACTION_GUARD,json!(id))).await?;
    let press = click().await?;
    counts(s, 1, 1, 1).await?;
    let after = snapshot(&s.peek());
    if before["formState"] != after["formState"] || before["textMounts"] != after["textMounts"] {
        return Err("Unsignaled value entered model".into());
    }
    // Restore only the DOM literal to the still-authoritative model, with no event.
    let restore=capture::dom(&format!(r#"(()=>{{{}const e=document.getElementById({});if(!owned(e))return{{ok:false}};Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,{});return{{ok:true,value:e.value,inputDispatched:false,visibleHitTested:true}};}})()"#,capture::ACTION_GUARD,json!(id),raw["before"])).await?;
    shot(s,d,n,"unseen-buffer-refused",json!({"raw":raw,"click":press,"restore":restore,"beforeForm":before["formState"],"afterForm":after["formState"]}),st,cap).await?;
    let before = snapshot(&s.peek());
    let lag=capture::dom(&format!(r#"(()=>{{{}const n=document.getElementById({}),e=[...(window.__spHomeText?.entries.values()||[])].find(e=>e.node===n);if(!owned(n)||!e||e.seq!==e.acked||e.seq<1||e.lease)return{{ok:false}};const before={{seq:e.seq,acked:e.acked,value:n.value}};e.acked=e.seq-1;return{{ok:true,before,after:{{seq:e.seq,acked:e.acked,value:n.value}},injectedAcknowledgementLag:true,visibleHitTested:true}};}})()"#,capture::ACTION_GUARD,json!(id))).await?;
    let press = click().await?;
    counts(s, 1, 1, 2).await?;
    let restore=capture::dom(&format!(r#"(()=>{{{}const n=document.getElementById({}),e=[...(window.__spHomeText?.entries.values()||[])].find(e=>e.node===n),old={};if(!owned(n)||!e||e.seq!==old.seq||e.acked!==old.acked-1||n.value!==old.value)return{{ok:false}};e.acked=old.acked;return{{ok:true,seq:e.seq,acked:e.acked,value:n.value,visibleHitTested:true}};}})()"#,capture::ACTION_GUARD,json!(id),lag["before"])).await?;
    let after = snapshot(&s.peek());
    if before["formState"] != after["formState"] || before["textMounts"] != after["textMounts"] {
        return Err("Acknowledgement injection changed model".into());
    }
    shot(s,d,n,"ack-lag-refused",json!({"lag":lag,"click":press,"restore":restore,"beforeForm":before["formState"],"afterForm":after["formState"]}),st,cap).await?;
    let edit = h::edit(s, d, "enter", "fresh acknowledged").await?;
    h::settle(s, d).await?;
    let press = click().await?;
    counts(s, 2, 1, 2).await?;
    let released = unlocked().await?;
    release(&mut s.write(), true);
    counts(s, 2, 2, 2).await?;
    shot(
        s,
        d,
        n,
        "fresh-acknowledged",
        json!({"edit":edit,"click":press,"released":released}),
        st,
        cap,
    )
    .await?;
    s.write()
        .app_home
        .text
        .button_fixture
        .as_mut()
        .unwrap()
        .hold_gate = true;
    let press = click().await?;
    for _ in 0..200 {
        if snapshot(&s.peek())["buttonSync"]["gateWaiting"] == true {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    if snapshot(&s.peek())["buttonSync"]["gateWaiting"] != true {
        return Err("Button final gate not reached".into());
    }
    let before = snapshot(&s.peek());
    let refresh = h::refresh(&mut s.write());
    release_gate(&mut s.write());
    counts(s, 2, 2, 3).await?;
    let released = unlocked().await?;
    shot(
        s,
        d,
        n,
        "view-retired",
        json!({"click":press,"held":before["buttonSync"],"refresh":refresh,"released":released}),
        st,
        cap,
    )
    .await?;
    Ok(())
}
pub(super) async fn pending(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut h::Captures,
) -> Result<(), String> {
    h::edit(s, d, "enter", "before").await?;
    h::settle(s, d).await?;
    let press = click().await?;
    counts(s, 1, 0, 0).await?;
    let released = unlocked().await?;
    shot(
        s,
        d,
        n,
        "button-pending",
        json!({"click":press,"released":released}),
        st,
        cap,
    )
    .await?;
    let edit = h::edit(s, d, "enter", "  after  ").await?;
    let enter = h::enter(s, d, "enter", json!({})).await?;
    h::counts(s, 1, 1).await?;
    let day=capture::dom(&format!(r#"await (async()=>{{{}const node=document.getElementById({}),e=[...(window.__spHomeCalendarNative?.entries.values()||[])].find(e=>e.node===node);const current=()=>e&&e.editableDate&&e.nativeCurrent()&&document.getElementById(node.id)===node&&!e.lease&&!node.disabled&&!node.readOnly&&!e.picker.disabled&&!e.picker.readOnly;const settled=()=>current()&&!e.pending&&e.seq===e.acked&&e.snapshotValue()===e.ackedValue;if(!owned(node)||!settled())return{{ok:false}};const before={{mount:e.mount,seq:e.seq,acked:e.acked,raw:node.value,picker:e.picker.value,value:e.snapshotValue()}},phases=[];for(const kind of ['input','change']){{if(!settled())return{{ok:false}};const seq=e.seq;if(kind==='input'){{e.picker.value='2026-10-25';if(e.picker.value!=='2026-10-25'||!e.picker.validity.valid)return{{ok:false}};}}e.picker.dispatchEvent(new Event(kind,{{bubbles:true}}));if(!current()||e.seq!==seq+1)return{{ok:false}};for(let i=0;i<200&&e.acked!==seq+1;i++){{await new Promise(r=>setTimeout(r,5));if(!current()||e.seq!==seq+1)return{{ok:false}};}}if(!settled())return{{ok:false}};phases.push({{kind,mount:e.mount,beforeSeq:seq,ackSeq:e.acked,raw:node.value,picker:e.picker.value,value:e.snapshotValue()}});}}return{{ok:true,value:node.value,selected:e.selected,picker:e.picker.value,snapshotValue:e.snapshotValue(),disabled:node.disabled,before,phases,visibleHitTested:true,physicalPicker:false,trusted:false}};}})()"#,capture::ACTION_GUARD,json!(crate::view::app_modal::field_id(&key("day"))))).await?;
    for _ in 0..150 {
        if snapshot(&s.peek())["formState"]["values"]["day"]["value"]["selected_date"]
            == "2026-10-25"
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let released = unlocked().await?;
    shot(
        s,
        d,
        n,
        "edited-while-pending",
        json!({"edit":edit,"enter":enter,"day":day,"released":released}),
        st,
        cap,
    )
    .await?;
    release(&mut s.write(), true);
    counts(s, 1, 1, 0).await?;
    shot(s, d, n, "button-accepted", json!({}), st, cap).await?;
    h::edit(s, d, "enter", "a").await?;
    let enter = h::enter(s, d, "enter", json!({})).await?;
    let queued = snapshot(&s.peek())["queue"].clone();
    let edit = h::edit(s, d, "enter", "b").await?;
    h::settle(s, d).await?;
    let press = click().await?;
    counts(s, 2, 1, 0).await?;
    h::counts(s, 2, 2).await?;
    let released = unlocked().await?;
    shot(
        s,
        d,
        n,
        "independent-actions",
        json!({"enter":enter,"queued":queued,"edit":edit,"click":press,"released":released}),
        st,
        cap,
    )
    .await?;
    release(&mut s.write(), false);
    counts(s, 2, 2, 0).await?;
    let toast = dismiss_uncertain(s, d, n, cap).await?;
    // A fresh explicit text occurrence remains independent of uncertain Button custody.
    let edit = h::edit(s, d, "enter", "fresh after uncertain").await?;
    let enter = h::enter(s, d, "enter", json!({})).await?;
    h::counts(s, 3, 3).await?;
    let silence = h::quiet(s, 3).await?;
    let released = unlocked().await?;
    shot(s,d,n,"uncertain-no-replay",json!({"edit":edit,"enter":enter,"silence":silence,"released":released,"toastDismissal":toast}),st,cap).await?;
    Ok(())
}
