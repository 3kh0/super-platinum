use super::*;
pub(super) async fn events(
    s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let edit = helpers::edit(s, d, "quiet", "  Quiet 東京  ").await?;
    let press = enter(s, d, "quiet", json!({})).await?;
    let silence = quiet(s, 0).await?;
    shot(
        s,
        d,
        n,
        "quiet",
        json!({"edit":edit,"press":press,"silence":silence}),
        st,
        cap,
    )
    .await?;
    helpers::edit(s, d, "enter", "modifier").await?;
    let shift = enter(s, d, "enter", json!({"shiftKey":true})).await?;
    let alt = enter(s, d, "enter", json!({"altKey":true})).await?;
    let ctrl = enter(s, d, "enter", json!({"ctrlKey":true})).await?;
    let silence = quiet(s, 0).await?;
    shot(
        s,
        d,
        n,
        "modifiers",
        json!({"shift":shift,"alt":alt,"ctrl":ctrl,"silence":silence,"platform":"windows"}),
        st,
        cap,
    )
    .await?;
    helpers::edit(s, d, "enter", "  Équipe 東京  ").await?;
    let press = enter(s, d, "enter", json!({})).await?;
    let queued = snapshot(&s.peek())["queue"].clone();
    counts(s, 1, 1).await?;
    shot(
        s,
        d,
        n,
        "literal-enter",
        json!({"press":press,"queued":queued}),
        st,
        cap,
    )
    .await?;
    let first = helpers::edit(s, d, "characters", "  first  ").await?;
    let first_queue = snapshot(&s.peek())["queue"].clone();
    let second = helpers::edit(s, d, "characters", "  東京  ").await?;
    let second_queue = snapshot(&s.peek())["queue"].clone();
    counts(s, 2, 2).await?;
    shot(
        s,
        d,
        n,
        "characters",
        json!({"first":first,"second":second,"firstQueue":first_queue,"secondQueue":second_queue}),
        st,
        cap,
    )
    .await?;
    let equal = helpers::edit(s, d, "characters", "東京 ").await?;
    let silence = quiet(s, 2).await?;
    shot(
        s,
        d,
        n,
        "trim-equal",
        json!({"edit":equal,"silence":silence}),
        st,
        cap,
    )
    .await?;
    helpers::edit(s, d, "characters", "line one\nline two").await?;
    let events_before = snapshot(&s.peek())["events"].as_array().map_or(0, Vec::len);
    let composing_press = enter(
        s,
        d,
        "characters",
        json!({"repeat":true,"isComposing":true,"metaKey":true,"shiftKey":true}),
    )
    .await?;
    let events_after_composing = snapshot(&s.peek())["events"].as_array().map_or(0, Vec::len);
    if composing_press["prevented"] != false || events_after_composing != events_before {
        return Err("Global composing Enter guard did not suppress the text occurrence".into());
    }
    let press = enter(
        s,
        d,
        "characters",
        json!({"repeat":true,"isComposing":false,"metaKey":true,"shiftKey":true}),
    )
    .await?;
    if press["prevented"] != true {
        return Err("Noncomposing multiline Enter did not reach the text listener".into());
    }
    counts(s, 3, 3).await?;
    shot(
        s,
        d,
        n,
        "multiline-enter",
        json!({"press":press,"composingPress":composing_press,"eventsBeforeComposing":events_before,"eventsAfterComposing":events_after_composing,"characterOnlyConfig":true,"globalCompositionGuardPreserved":true}),
        st,
        cap,
    )
    .await?;
    helpers::edit(s, d, "enter", "field one").await?;
    let one = enter(s, d, "enter", json!({})).await?;
    let two = helpers::edit(s, d, "characters", "field two").await?;
    let queued = snapshot(&s.peek())["queue"].clone();
    counts(s, 5, 5).await?;
    shot(
        s,
        d,
        n,
        "independent-fields",
        json!({"one":one,"two":two,"queued":queued}),
        st,
        cap,
    )
    .await
}
pub(super) async fn state(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    helpers::edit(s, d, "enter", "a").await?;
    let press = enter(s, d, "enter", json!({})).await?;
    let queued = snapshot(&s.peek())["queue"].clone();
    helpers::edit(s, d, "enter", " b ").await?;
    helpers::edit(s, d, "quiet", " Latest sibling ").await?;
    counts(s, 1, 1).await?;
    shot(
        s,
        d,
        n,
        "latest-state",
        json!({"press":press,"queued":queued}),
        st,
        cap,
    )
    .await?;
    helpers::edit(s, d, "enter", "valid").await?;
    enter(s, d, "enter", json!({})).await?;
    let queued = snapshot(&s.peek())["queue"].clone();
    helpers::edit(s, d, "enter", "").await?;
    counts(s, 2, 2).await?;
    shot(
        s,
        d,
        n,
        "invalid-latest-state",
        json!({"queued":queued}),
        st,
        cap,
    )
    .await?;
    let invalid = enter(s, d, "enter", json!({})).await?;
    let silence = quiet(s, 2).await?;
    shot(
        s,
        d,
        n,
        "invalid-enter",
        json!({"press":invalid,"silence":silence}),
        st,
        cap,
    )
    .await?;
    let day=capture::dom(&format!(r#"(()=>{{{}const n=document.getElementById({});if(!owned(n))return{{ok:false}};n.value='2026-10-25';n.dispatchEvent(new Event('input',{{bubbles:true}}));n.dispatchEvent(new Event('change',{{bubbles:true}}));return{{ok:true,value:n.value,valid:n.validity.valid}};}})()"#,capture::ACTION_GUARD,json!(crate::view::app_modal::field_id(&key("day"))))).await?;
    for _ in 0..150 {
        if snapshot(&s.peek())["formState"]["values"]["day"]["value"]["selected_date"]
            == "2026-10-25"
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    helpers::edit(s, d, "enter", "calendar").await?;
    enter(s, d, "enter", json!({})).await?;
    counts(s, 3, 3).await?;
    shot(
        s,
        d,
        n,
        "calendar-state",
        json!({"nativeDate":day}),
        st,
        cap,
    )
    .await?;
    s.write().app_home.text.fixture.as_mut().unwrap().hold_gate = true;
    helpers::edit(s, d, "enter", "retired").await?;
    enter(s, d, "enter", json!({})).await?;
    for _ in 0..180 {
        if snapshot(&s.peek())["gateWaiting"] == true {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    if snapshot(&s.peek())["gateWaiting"] != true {
        return Err("Native dispatch gate did not hold".into());
    }
    let lease=capture::dom("(()=>{const t=[...window.__spHomeText.entries.values()].filter(e=>!e.closed),c=[...window.__spHomeCalendarNative.entries.values()].filter(e=>!e.closed);return{ok:t.length===2&&t.every(e=>!!e.lease&&e.node.readOnly&&!e.node.disabled)&&c.length===1&&c.every(e=>e.lease&&e.node.disabled),text:t.map(e=>({mount:e.mount,seq:e.seq,value:e.node.value,lease:!!e.lease,readOnly:e.node.readOnly,disabled:e.node.disabled})),calendar:c.map(e=>({mount:e.mount,seq:e.seq,value:e.node.value,lease:e.lease}))};})()").await?;
    let changed = helpers::refresh(&mut s.write());
    {
        let mut w = s.write();
        let f = w.app_home.text.fixture.as_mut().unwrap();
        f.hold_gate = false;
        f.gate_waiting = false;
        let _ = f.gate.take().unwrap().send(());
    }
    let silence = quiet(s, 3).await?;
    shot(
        s,
        d,
        n,
        "lease-retired",
        json!({"lease":lease,"refresh":changed,"silence":silence}),
        st,
        cap,
    )
    .await?;
    helpers::edit(s, d, "enter", "not sent").await?;
    enter(s, d, "enter", json!({})).await?;
    let mismatch=capture::dom(&format!(r#"(()=>{{{}const n=document.getElementById({});if(!owned(n))return{{ok:false}};n.value='Unacknowledged';return{{ok:true,value:n.value,inputDispatched:false}};}})()"#,capture::ACTION_GUARD,json!(crate::view::app_modal::field_id(&key("quiet"))))).await?;
    let silence = quiet(s, 3).await?;
    capture::dom(&format!("(()=>{{const n=document.getElementById({});n.dispatchEvent(new Event('input',{{bubbles:true}}));return{{ok:true}};}})()",json!(crate::view::app_modal::field_id(&key("quiet"))))).await?;
    for _ in 0..150 {
        if snapshot(&s.peek())["formState"]["values"]["quiet"]["value"]["value"] == "Unacknowledged"
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    shot(
        s,
        d,
        n,
        "unacknowledged-refused",
        json!({"mismatch":mismatch,"silence":silence}),
        st,
        cap,
    )
    .await
}
async fn send(
    s: Signal<ShellState>,
    d: &DesktopContext,
    value: &str,
    requests: usize,
    responses: usize,
) -> Result<Value, String> {
    helpers::edit(s, d, "enter", value).await?;
    let p = enter(s, d, "enter", json!({})).await?;
    counts(s, requests, responses).await?;
    Ok(p)
}
pub(super) async fn lifecycle(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    configure(&mut s.write(), Reply::Held);
    let first = send(s, d, "first", 1, 0).await?;
    let second = send(s, d, "second", 2, 0).await?;
    let ids = snapshot(&s.peek())["pendingIds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap())
        .collect::<Vec<_>>();
    shot(
        s,
        d,
        n,
        "overlapping",
        json!({"first":first,"second":second,"ids":ids}),
        st,
        cap,
    )
    .await?;
    if !release(&mut s.write(), ids[0], Reply::Accepted) {
        return Err("First held response gone".into());
    }
    counts(s, 2, 1).await?;
    shot(
        s,
        d,
        n,
        "earlier-completed",
        json!({"released":ids[0],"laterStillPending":ids[1]}),
        st,
        cap,
    )
    .await?;
    if !release(&mut s.write(), ids[1], Reply::Uncertain) {
        return Err("Second held response gone".into());
    }
    counts(s, 2, 2).await?;
    shot(s, d, n, "uncertain", json!({"released":ids[1]}), st, cap).await?;
    configure(&mut s.write(), Reply::Accepted);
    let p = send(s, d, "fresh", 3, 3).await?;
    shot(s, d, n, "fresh-explicit", json!({"press":p}), st, cap).await?;
    configure(&mut s.write(), Reply::Rejected);
    let p = send(s, d, "rejected", 4, 4).await?;
    shot(s, d, n, "rejected", json!({"press":p}), st, cap).await?;
    configure(&mut s.write(), Reply::Offline);
    let p = send(s, d, "offline", 5, 5).await?;
    shot(s, d, n, "offline", json!({"press":p}), st, cap).await?;
    configure(&mut s.write(), Reply::RateLimited);
    let p = send(s, d, "rate limit", 6, 6).await?;
    let cooling_before = snapshot(&s.peek())["cooling"].clone();
    let disabled_before=crate::slack_links_check::wait_dom(d,"(()=>{const fields=[...document.querySelectorAll('#app-home input,#app-home textarea')];return{ok:fields.length===4&&fields.every(e=>e.disabled),disabled:fields.map(e=>({id:e.id,disabled:e.disabled}))};})()").await?;
    for _ in 0..150 {
        let done = {
            let w = s.peek();
            w.app_home
                .surface
                .as_ref()
                .is_some_and(|h| !cooling(&w, &h.owner.ctx))
        };
        if done {
            break;
        }
        tokio::time::sleep(Duration::from_millis(15)).await;
    }
    settle(s, d).await?;
    let enabled_after=crate::slack_links_check::wait_dom(d,"(()=>{const fields=[...document.querySelectorAll('#app-home input,#app-home textarea')];return{ok:fields.length===4&&fields.every(e=>!e.disabled),disabled:fields.map(e=>({id:e.id,disabled:e.disabled}))};})()").await?;
    let cooling_after = snapshot(&s.peek())["cooling"].clone();
    shot(s,d,n,"rate-limited",json!({"press":p,"coolingBefore":cooling_before,"coolingAfter":cooling_after,"disabledBefore":disabled_before,"enabledAfter":enabled_after,"captureAfterDeadline":true}),st,cap).await?;
    let silence = quiet(s, 6).await?;
    configure(&mut s.write(), Reply::Accepted);
    let p = send(s, d, "after wait", 7, 7).await?;
    shot(
        s,
        d,
        n,
        "rate-recovered",
        json!({"silence":silence,"press":p}),
        st,
        cap,
    )
    .await?;
    let toast_dismissal = if n.ends_with("-narrow") {
        Some(dismiss_failure_toast(s, d).await?)
    } else {
        None
    };
    helpers::edit(s, d, "enter", "old view").await?;
    enter(s, d, "enter", json!({})).await?;
    let changed = helpers::refresh(&mut s.write());
    let silence = quiet(s, 7).await?;
    let mut view_proof = json!({"refresh":changed,"silence":silence});
    if let Some(dismissal) = toast_dismissal {
        view_proof["toastDismissal"] = dismissal;
    }
    shot(s, d, n, "view-retired", view_proof, st, cap).await?;
    helpers::edit(s, d, "enter", "old context").await?;
    enter(s, d, "enter", json!({})).await?;
    {
        let mut w = s.write();
        w.core
            .session
            .as_mut()
            .unwrap()
            .unified_user_client_enabled
            .clear();
    }
    let silence = quiet(s, 7).await?;
    shot(
        s,
        d,
        n,
        "context-retired",
        json!({"source":"current session configuration removed","silence":silence}),
        st,
        cap,
    )
    .await?;
    {
        let mut w = s.write();
        let team = w.core.active_team.clone().unwrap();
        w.core
            .session
            .as_mut()
            .unwrap()
            .unified_user_client_enabled
            .insert(team, false);
    }
    helpers::edit(s, d, "enter", "old socket").await?;
    enter(s, d, "enter", json!({})).await?;
    {
        let mut w = s.write();
        let team = w.core.active_team.clone().unwrap();
        w.core.workspaces.get_mut(&team).unwrap().rt_generation += 1;
    }
    let silence = quiet(s, 7).await?;
    shot(
        s,
        d,
        n,
        "socket-retired",
        json!({"silence":silence}),
        st,
        cap,
    )
    .await?;
    {
        let mut w = s.write();
        let owner = Owner::capture(&w).ok_or("Fixture Home rebind unavailable")?;
        let h = w.app_home.surface.as_mut().unwrap();
        h.id = owner.home_visit;
        h.owner = owner;
        h.revision = next();
        inputs::hydrate(&mut w);
    }
    settle(s, d).await?;
    helpers::edit(s, d, "enter", "old account").await?;
    enter(s, d, "enter", json!({})).await?;
    s.write().core.account_epoch += 1;
    let silence = quiet(s, 7).await?;
    shot(
        s,
        d,
        n,
        "account-retired",
        json!({"fixtureOwnerRebindAfterSocket":true,"silence":silence}),
        st,
        cap,
    )
    .await
}

// A failure toast remains visible in disconnected fixtures; dismiss through its
// real control before testing the post-refresh narrow field geometry.
async fn dismiss_failure_toast(s: Signal<ShellState>, d: &DesktopContext) -> Result<Value, String> {
    let before = snapshot(&s.peek());
    let click = capture::dom(&format!(r#"(()=>{{{}const t=document.getElementById('status-toast'),e=t?.querySelector('.toast-dismiss[aria-label="Dismiss notification"]');if(!t||!owned(e))return{{ok:false}};const text=t.querySelector('.toast-text')?.textContent,control=e.getAttribute('aria-label');e.focus({{preventScroll:true}});e.click();return{{ok:true,text,control,visibleHitTested:true,clicked:true}};}})()"#,capture::ACTION_GUARD)).await?;
    let absent = crate::slack_links_check::wait_dom(d,"(()=>({ok:!document.getElementById('status-toast'),toastPresent:!!document.getElementById('status-toast')}))()").await?;
    let after = snapshot(&s.peek());
    if before != after {
        return Err("Toast dismissal changed Home text source or input state".into());
    }
    Ok(
        json!({"click":click,"afterDom":absent,"beforeModel":before,"afterModel":after,"modelUnchanged":true}),
    )
}
