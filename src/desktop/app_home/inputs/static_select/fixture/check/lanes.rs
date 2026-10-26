use super::helpers::{refresh, release_gate};
use super::*;
pub(super) async fn selection(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let selected = choose(s, d, "pick", vec![Some(1)]).await?;
    counts(s, 1, 1).await?;
    shot(
        s,
        d,
        n,
        "selected",
        json!({"selection":selected,"singleLeading":quiet(s,1).await?}),
        st,
        cap,
    )
    .await?;
    let repeated = choose(s, d, "pick", vec![Some(1)]).await?;
    counts(s, 2, 2).await?;
    shot(s, d, n, "repeated", json!({"selection":repeated}), st, cap).await?;
    let empty = choose(s, d, "pick", vec![Some(3)]).await?;
    counts(s, 3, 3).await?;
    shot(
        s,
        d,
        n,
        "empty-option",
        json!({"selection":empty,"resolvedEmptyValueToNull":true}),
        st,
        cap,
    )
    .await?;
    let cleared = choose(s, d, "pick", vec![None]).await?;
    counts(s, 4, 4).await?;
    shot(s, d, n, "cleared", json!({"selection":cleared}), st, cap).await?;
    let quiet_choice = choose(s, d, "quiet", vec![Some(2)]).await?;
    shot(
        s,
        d,
        n,
        "quiet-edited",
        json!({"selection":quiet_choice,"silence":quiet(s,4).await?}),
        st,
        cap,
    )
    .await?;
    let native=capture::dom(&format!(r#"(()=>{{{}const e=document.querySelector('#app-home input[type=date]');e?.scrollIntoView({{block:'center'}});if(!owned(e))return{{ok:false}};e.value='2026-10-25';const event=new Event('input',{{bubbles:true}});e.dispatchEvent(event);return{{ok:true,value:e.value,trusted:event.isTrusted,visibleHitTested:true}};}})()"#,capture::ACTION_GUARD)).await?;
    for _ in 0..150 {
        if form(&s.peek())
            .and_then(|f| f.fields.get(&key("day")))
            .is_some_and(
                |f| matches!(&f.value,InputValue::Date(v)if v.as_deref()==Some("2026-10-25")),
            )
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let selected = choose(s, d, "pick", vec![Some(2)]).await?;
    counts(s, 5, 5).await?;
    shot(
        s,
        d,
        n,
        "native-siblings",
        json!({"native":native,"selection":selected}),
        st,
        cap,
    )
    .await?;
    let changed = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]["initial_option"] = option("beta", "Beta")
    });
    shot(s,d,n,"initial-changed",json!({"refresh":changed,"projectionLimit":"Native raw Home refresh is product source hydration; reference proves mounted props only."}),st,cap).await?;
    let changed = refresh(
        &mut s.write(),
        |r| r["state"] = json!({"values":{"pick":{"value":{"type":"static_select","selected_option":option("alpha","Alpha")}}}}),
    );
    shot(
        s,
        d,
        n,
        "saved-ignored",
        json!({"refresh":changed,"dirtyParentRetainedByExistingReconcile":true}),
        st,
        cap,
    )
    .await?;
    let changed = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_option");
    });
    shot(
        s,
        d,
        n,
        "initial-removed",
        json!({"refresh":changed,"silence":quiet(s,5).await?}),
        st,
        cap,
    )
    .await?;
    let selected = choose(s, d, "pick", vec![Some(1)]).await?;
    counts(s, 6, 6).await?;
    let button=capture::dom(&format!(r#"(()=>{{{}const b=[...document.querySelectorAll('#app-home button')].find(e=>e.textContent.trim()==='Send form');b?.scrollIntoView({{block:'center'}});if(!owned(b))return{{ok:false}};b.click();return{{ok:true,text:b.textContent,visibleHitTested:true}};}})()"#,capture::ACTION_GUARD)).await?;
    crate::slack_links_check::wait_dom(d, "({ok:!!document.querySelector('.app-modal')})").await?;
    let wire = snapshot(&s.peek())["actions"].clone();
    crate::app_modal::close(s);
    crate::slack_links_check::wait_dom(d, "({ok:!document.querySelector('.app-modal')})").await?;
    shot(s,d,n,"explicit-button",json!({"selection":selected,"button":button,"buttonWire":wire,"activation":"Existing actual Button prepare/final-current closed Home modal adapter"}),st,cap).await?;
    let before = snapshot(&s.peek());
    let reset=capture::dom(&format!(r#"(()=>{{{}const b=[...document.querySelectorAll('#app-home button')].find(e=>e.textContent.trim()==='Reset inputs');b?.scrollIntoView({{block:'center'}});if(!owned(b))return{{ok:false}};b.click();return{{ok:true,text:b.textContent,visibleHitTested:true}};}})()"#,capture::ACTION_GUARD)).await?;
    for _ in 0..150 {
        if snapshot(&s.peek())["authority"]["homeRevision"] != before["authority"]["homeRevision"] {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    shot(
        s,
        d,
        n,
        "reset",
        json!({"reset":reset,"before":before,"silence":quiet(s,6).await?}),
        st,
        cap,
    )
    .await
}
pub(super) async fn pending(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    configure(&mut s.write(), Reply::Held);
    let burst = choose(s, d, "pick", vec![Some(0), Some(1), Some(2)]).await?;
    counts(s, 2, 0).await?;
    settle(s, d).await?;
    let blocked=capture::dom(r#"(()=>{const e=[...(window.__spHomeStatic?.entries.values()||[])].find(e=>e.node.closest('.app-modal-field')?.dataset.blockId==='pick'),before=e?.seq;if(!e||!e.node.disabled||e.lease)return{ok:false};e.node.dispatchEvent(new Event('change',{bubbles:true}));return{ok:e.seq===before,disabled:e.node.disabled,beforeSeq:before,afterSeq:e.seq,eventTrusted:false};})()"#).await?;
    shot(
        s,
        d,
        n,
        "trailing-held",
        json!({"burst":burst,"disabledCallback":blocked}),
        st,
        cap,
    )
    .await?;
    let ids = snapshot(&s.peek())["pendingIds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap())
        .collect::<Vec<_>>();
    if !release(&mut s.write(), ids[0], Reply::Accepted) {
        return Err("Missing first held static request".into());
    }
    counts(s, 2, 1).await?;
    shot(
        s,
        d,
        n,
        "first-completed",
        json!({"released":ids[0],"laterStillHeld":ids[1]}),
        st,
        cap,
    )
    .await?;
    let independent = choose(s, d, "required", vec![Some(1)]).await?;
    counts(s, 3, 1).await?;
    shot(
        s,
        d,
        n,
        "independent-block",
        json!({"selection":independent}),
        st,
        cap,
    )
    .await?;
    let ids = snapshot(&s.peek())["pendingIds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap())
        .collect::<Vec<_>>();
    for id in &ids {
        release(&mut s.write(), *id, Reply::Accepted);
    }
    counts(s, 3, 3).await?;
    shot(s, d, n, "all-completed", json!({"released":ids}), st, cap).await?;
    configure(&mut s.write(), Reply::Offline);
    let choice = choose(s, d, "pick", vec![Some(0)]).await?;
    counts(s, 4, 4).await?;
    let toast = dismiss(s, d, n, "offline-toast", cap).await?;
    shot(
        s,
        d,
        n,
        "offline",
        json!({"selection":choice,"toast":toast,"silence":quiet(s,4).await?}),
        st,
        cap,
    )
    .await?;
    configure(&mut s.write(), Reply::RateLimited);
    let choice = choose(s, d, "pick", vec![Some(1)]).await?;
    for _ in 0..160 {
        if snapshot(&s.peek())["responses"].as_array().map(Vec::len) == Some(5) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let cooling_before = snapshot(&s.peek())["cooling"].clone();
    let disabled=crate::slack_links_check::wait_dom(d,r#"(()=>{const n=[...document.querySelectorAll('#app-home select,#app-home input')];return{ok:n.length===5&&n.every(n=>n.disabled),fields:n.map(n=>({id:n.id,disabled:n.disabled})),cooling:true};})()"#).await?;
    for _ in 0..180 {
        if snapshot(&s.peek())["cooling"] == false {
            break;
        }
        tokio::time::sleep(Duration::from_millis(15)).await;
    }
    counts(s, 5, 5).await?;
    let toast = dismiss(s, d, n, "rate-toast", cap).await?;
    shot(s,d,n,"cooldown-expired",json!({"selection":choice,"coolingBefore":cooling_before,"disabledBefore":disabled,"coolingAfter":snapshot(&s.peek())["cooling"],"toast":toast,"silence":quiet(s,5).await?}),st,cap).await?;
    configure(&mut s.write(), Reply::Accepted);
    let choice = choose(s, d, "pick", vec![Some(2)]).await?;
    counts(s, 6, 6).await?;
    shot(
        s,
        d,
        n,
        "explicit-recovery",
        json!({"selection":choice}),
        st,
        cap,
    )
    .await?;
    configure(&mut s.write(), Reply::Uncertain);
    let choice = choose(s, d, "pick", vec![Some(0)]).await?;
    counts(s, 7, 7).await?;
    let toast = dismiss(s, d, n, "uncertain-toast", cap).await?;
    shot(
        s,
        d,
        n,
        "uncertain",
        json!({"selection":choice,"toast":toast,"silence":quiet(s,7).await?}),
        st,
        cap,
    )
    .await?;
    configure(&mut s.write(), Reply::Accepted);
    let choice = choose(s, d, "pick", vec![Some(1)]).await?;
    counts(s, 8, 8).await?;
    shot(
        s,
        d,
        n,
        "fresh-after-uncertain",
        json!({"selection":choice}),
        st,
        cap,
    )
    .await
}
pub(super) async fn retirement(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let buffer=capture::dom(&format!(r#"(()=>{{{}const e=document.getElementById({});e?.scrollIntoView({{block:'center'}});if(!owned(e))return{{ok:false}};const before=e.value;e.value='Unreceived native buffer';return{{ok:true,id:e.id,before,after:e.value,eventDispatched:false,visibleHitTested:true}};}})()"#,capture::ACTION_GUARD,json!(crate::view::app_modal::field_id(&key("notes"))))).await?;
    // Native buffer mismatch is intentional here; choose uses the same actual
    // select event but cannot run whole-form settle until the buffer is restored.
    let choice = direct_pick(d, 1).await?;
    refusal(s, 1).await?;
    capture::dom(&format!(r#"(()=>{{const e=document.getElementById({});if(!e||e.value!=={})return{{ok:false}};e.value={};return{{ok:true,eventDispatched:false}};}})()"#,buffer["id"],buffer["after"],buffer["before"])).await?;
    let select_buffer=capture::dom(&format!(r#"(()=>{{{}const e=document.getElementById({});e?.scrollIntoView({{block:'center'}});if(!owned(e))return{{ok:false}};const before=e.value;e.value='option-2';return{{ok:true,id:e.id,before,after:e.value,eventDispatched:false,visibleHitTested:true}};}})()"#,capture::ACTION_GUARD,json!(crate::view::app_modal::field_id(&key("quiet"))))).await?;
    let select_choice = direct_pick(d, 0).await?;
    refusal(s, 2).await?;
    capture::dom(&format!(r#"(()=>{{const e=document.getElementById({});if(!e||e.value!=={})return{{ok:false}};e.value={};return{{ok:true,eventDispatched:false}};}})()"#,select_buffer["id"],select_buffer["after"],select_buffer["before"])).await?;
    shot(
        s,
        d,
        n,
        "native-buffer-refused",
        json!({"buffer":buffer,"selection":choice,"quietSelectBuffer":select_buffer,"quietSelectRefusalSelection":select_choice,"silence":quiet(s,0).await?}),
        st,
        cap,
    )
    .await?;
    hold(&mut s.write());
    choose(s, d, "pick", vec![Some(2)]).await?;
    let held = gate(s).await?;
    let changed = refresh(&mut s.write(), |_| {});
    release_gate(&mut s.write());
    refusal(s, 3).await?;
    shot(
        s,
        d,
        n,
        "view-retired",
        json!({"held":held,"refresh":changed}),
        st,
        cap,
    )
    .await?;
    hold(&mut s.write());
    choose(s, d, "pick", vec![Some(0)]).await?;
    let held = gate(s).await?;
    let config = {
        let mut w = s.write();
        let a = w.core.session.as_mut().unwrap();
        let previous = a.unified_user_client_enabled.clone();
        a.unified_user_client_enabled.clear();
        previous
    };
    let changed = snapshot(&s.peek())["context"].clone();
    release_gate(&mut s.write());
    refusal(s, 4).await?;
    s.write()
        .core
        .session
        .as_mut()
        .unwrap()
        .unified_user_client_enabled = config;
    shot(
        s,
        d,
        n,
        "context-retired",
        json!({"held":held,"unavailableContext":changed,"configurationRestoredAfterRefusal":true}),
        st,
        cap,
    )
    .await?;
    hold(&mut s.write());
    choose(s, d, "pick", vec![Some(1)]).await?;
    let held = gate(s).await?;
    {
        let mut w = s.write();
        w.core.workspaces.get_mut("T1").unwrap().rt_generation += 1;
    }
    release_gate(&mut s.write());
    refusal(s, 5).await?;
    shot(s, d, n, "socket-retired", json!({"held":held}), st, cap).await?;
    {
        let mut w = s.write();
        let owner = Owner::capture(&w).ok_or("Static fixture rebind")?;
        let h = w.app_home.surface.as_mut().unwrap();
        h.id = owner.home_visit;
        h.owner = owner;
        h.revision = next();
        inputs::hydrate(&mut w);
    }
    settle(s, d).await?;
    hold(&mut s.write());
    choose(s, d, "pick", vec![Some(2)]).await?;
    let held = gate(s).await?;
    s.write().core.account_epoch += 1;
    release_gate(&mut s.write());
    refusal(s, 6).await?;
    shot(
        s,
        d,
        n,
        "account-retired",
        json!({"held":held,"fixtureOwnerRebindAfterSocket":true,"silence":quiet(s,0).await?}),
        st,
        cap,
    )
    .await
}
async fn direct_pick(d: &DesktopContext, index: usize) -> Result<Value, String> {
    let _ = d;
    capture::dom(&format!(r#"(()=>{{{}const e=document.getElementById({});e?.scrollIntoView({{block:'center'}});if(!owned(e))return{{ok:false}};e.value={};const event=new Event('change',{{bubbles:true}});e.dispatchEvent(event);return{{ok:true,id:e.id,value:e.value,trusted:event.isTrusted,visibleHitTested:true}};}})()"#,capture::ACTION_GUARD,json!(crate::view::app_modal::field_id(&key("pick"))),json!(format!("option-{index}")))).await
}
