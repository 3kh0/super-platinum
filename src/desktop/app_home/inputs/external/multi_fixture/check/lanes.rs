use super::helpers::{
    cancelled, close_modal, done, open, ready, remove, retired, row, selected, settle, shot,
};
use super::*;
type Captures = serde_json::Map<String, Value>;
const I: &str = "installed";
const A: &str = "alpha,desktop";
const B: &str = "beta";
async fn options(
    state: Signal<ShellState>,
    d: &DesktopContext,
    count: usize,
) -> Result<(), String> {
    open(d).await?;
    type_query("release").await?;
    settle(state, count, false).await?;
    wait(
        d,
        "document.querySelectorAll('[data-home-external] input[type=checkbox]').length===2",
    )
    .await?;
    Ok(())
}
pub(super) async fn selection(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    c: &mut Captures,
) -> Result<(), String> {
    options(s, d, 1).await?;
    row(d, 0).await?;
    selected(s, &[I, A], &[I, A]).await?;
    shot(s, d, n, "accepted", Value::Null, st, c).await?;
    let before = snapshot(&s.peek());
    row(d, 1).await?;
    selected(s, &[I, A, B], &[I, A]).await?;
    let after = snapshot(&s.peek());
    if before["inputRevision"] != after["inputRevision"] || after["widget"]["invalid"] != true {
        return Err("Invalid widget changed accepted form revision".into());
    }
    shot(
        s,
        d,
        n,
        "over-limit",
        json!({"before":before,"inputRevisionUnchanged":true}),
        st,
        c,
    )
    .await?;
    done(d).await?;
    shot(s, d, n, "closed-invalid", Value::Null, st, c).await?;
    open(d).await?;
    selected(s, &[I, A, B], &[I, A]).await?;
    shot(s, d, n, "reopened-invalid", Value::Null, st, c).await?;
    done(d).await?;
    let action = super::helpers::action(s, d, &[I, A]).await?;
    shot(s, d, n, "accepted-action", action, st, c).await?;
    close_modal(d).await?;
    let change = refresh(&mut s.write(), 4, None, None)?;
    retired(s).await?;
    ready(s, d).await?;
    if snapshot(&s.peek())["widget"]["invalid"] != true {
        return Err("Max-only refresh cleared retained invalid flag".into());
    }
    shot(s, d, n, "raised-maximum", change, st, c).await?;
    remove(I).await?;
    selected(s, &[A, B], &[A, B]).await?;
    shot(s, d, n, "corrected", Value::Null, st, c).await?;
    let action = super::helpers::action(s, d, &[A, B]).await?;
    shot(s, d, n, "corrected-action", action, st, c).await
}
pub(super) async fn validation(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    c: &mut Captures,
) -> Result<(), String> {
    let action = super::helpers::action(s, d, &[I, A, B]).await?;
    shot(s, d, n, "initial-overmax-action", action, st, c).await?;
    close_modal(d).await?;
    let proof = refresh(&mut s.write(), 1, None, None)?;
    ready(s, d).await?;
    shot(s, d, n, "reduced-maximum", proof, st, c).await?;
    let action = super::helpers::action(s, d, &[I, A, B]).await?;
    shot(s, d, n, "reduced-action", action, st, c).await?;
    close_modal(d).await?;
    remove(I).await?;
    selected(s, &[A, B], &[I, A, B]).await?;
    shot(s, d, n, "removal-still-invalid", Value::Null, st, c).await?;
    remove(A).await?;
    selected(s, &[B], &[B]).await?;
    remove(B).await?;
    selected(s, &[], &[]).await?;
    let empty = snapshot(&s.peek());
    wait(
        d,
        &format!(
            r#"(()=>{{const a={},home=document.getElementById('app-home')||document.querySelector('.app-home'),field=home?.querySelector('[data-home-external="home-choice"]'),wrapper=field?.closest('.app-modal-field'),open=field?.querySelector('[data-external-open]');return a.homeCurrent===true&&!!home&&home.dataset.visit===String(a.homeVisit)&&home.dataset.revision===String(a.homeRevision)&&!!wrapper&&!!open&&!open.disabled&&!field.querySelector('.message-multi-selected-option')&&!wrapper.querySelector(':scope > .app-modal-error')&&field.getAttribute('aria-invalid')==='false'&&!document.getElementById(open.id+'-error')&&![field,...field.querySelectorAll('[aria-describedby]')].some(e=>(e.getAttribute('aria-describedby')||'').split(/\s+/).some(id=>id.endsWith('-error')));}})()"#,
            empty["authority"]
        ),
    )
    .await?;
    shot(s, d, n, "required-empty", Value::Null, st, c).await?;
    let action = super::helpers::action(s, d, &[]).await?;
    shot(s, d, n, "empty-action", action, st, c).await?;
    close_modal(d).await?;
    let initial = json!([fixture::option("Release Beta", B)]);
    let proof = refresh(&mut s.write(), 2, Some(initial), None)?;
    ready(s, d).await?;
    selected(s, &[B], &[B]).await?;
    shot(s, d, n, "initial-changed", proof, st, c).await?;
    let before = snapshot(&s.peek());
    let saved = json!([fixture::option("Release Alpha", A)]);
    let proof = refresh(&mut s.write(), 2, None, Some(saved))?;
    ready(s, d).await?;
    selected(s, &[B], &[B]).await?;
    // The existing dirty HomeForm reconciliation owns this retained accepted value;
    // the mounted widget emits nothing for savedOptions-only props.
    if before["widget"] != snapshot(&s.peek())["widget"] {
        return Err("Saved-only refresh changed mounted widget".into());
    }
    shot(
        s,
        d,
        n,
        "saved-prop-only",
        json!({"refresh":proof,"dirtyFormRetained":true,"widgetCallback":false}),
        st,
        c,
    )
    .await
}
pub(super) async fn failure(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    c: &mut Captures,
) -> Result<(), String> {
    options(s, d, 1).await?;
    row(d, 0).await?;
    selected(s, &[I, A], &[I, A]).await?;
    shot(s, d, n, "prior-options", Value::Null, st, c).await?;
    configure(&mut s.write(), Reply::Offline, true);
    type_query("failure").await?;
    settle(s, 2, true).await?;
    shot(s, d, n, "held", Value::Null, st, c).await?;
    // A local over-limit edit cancels the held request without changing accepted state.
    row(d, 1).await?;
    selected(s, &[I, A, B], &[I, A]).await?;
    let release = cancelled(s).await?;
    shot(s, d, n, "edited-held", release, st, c).await?;
    configure(&mut s.write(), Reply::Offline, false);
    type_query("failed").await?;
    settle(s, 3, false).await?;
    wait(
        d,
        "!!document.querySelector('[data-external-retry]:not(:disabled)')",
    )
    .await?;
    shot(s, d, n, "failed", Value::Null, st, c).await?;
    configure(&mut s.write(), Reply::Replacement, false);
    click("[data-external-retry]").await?;
    settle(s, 4, false).await?;
    wait(
        d,
        "document.querySelectorAll('[data-option-index]').length===1",
    )
    .await?;
    shot(s, d, n, "replacement", Value::Null, st, c).await?;
    remove(B).await?;
    selected(s, &[I, A], &[I, A]).await?;
    row(d, 0).await?;
    selected(s, &[I], &[I]).await?;
    row(d, 0).await?;
    selected(s, &[I, A], &[I, A]).await?;
    if snapshot(&s.peek())["widget"]["accepted"][1]["text"]["text"] != "Release Gamma" {
        return Err("Current same-value raw replacement not selected".into());
    }
    shot(s, d, n, "reselected", Value::Null, st, c).await?;
    done(d).await?;
    let action = super::helpers::action(s, d, &[I, A]).await?;
    shot(s, d, n, "replacement-action", action, st, c).await
}
pub(super) async fn retirement(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    c: &mut Captures,
) -> Result<(), String> {
    options(s, d, 1).await?;
    row(d, 1).await?;
    selected(s, &[I, B], &[I, B]).await?;
    configure(&mut s.write(), Reply::Options, true);
    type_query("held-view").await?;
    settle(s, 2, true).await?;
    shot(s, d, n, "view-held", Value::Null, st, c).await?;
    let refreshed = refresh(&mut s.write(), 2, None, None)?;
    let release = cancelled(s).await?;
    retired(s).await?;
    shot(
        s,
        d,
        n,
        "view-retired",
        json!({"refresh":refreshed,"release":release}),
        st,
        c,
    )
    .await?;
    configure(&mut s.write(), Reply::Options, true);
    open(d).await?;
    type_query("held-socket").await?;
    settle(s, 3, true).await?;
    shot(s, d, n, "socket-held", Value::Null, st, c).await?;
    {
        let mut s = s.write();
        let team = s.core.active_team.clone().unwrap();
        s.core.workspaces.get_mut(&team).unwrap().rt_generation += 1;
    }
    let release = cancelled(s).await?;
    retired(s).await?;
    shot(s, d, n, "socket-retired", release, st, c).await?;
    {
        let mut s = s.write();
        let owner = crate::app_home::Owner::capture(&s).ok_or("No fresh Home owner")?;
        let h = s.app_home.surface.as_mut().unwrap();
        h.id = owner.home_visit;
        h.owner = owner;
        h.revision = next();
        crate::app_home::inputs::hydrate(&mut s);
    }
    configure(&mut s.write(), Reply::Options, true);
    open(d).await?;
    type_query("held-account").await?;
    settle(s, 4, true).await?;
    shot(s, d, n, "account-held", Value::Null, st, c).await?;
    s.write().core.account_epoch += 1;
    let release = cancelled(s).await?;
    retired(s).await?;
    shot(s, d, n, "account-retired", release, st, c).await
}
pub(super) async fn unified_lane(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    c: &mut Captures,
) -> Result<(), String> {
    options(s, d, 1).await?;
    row(d, 0).await?;
    selected(s, &[I, A], &[I, A]).await?;
    shot(s, d, n, "selected-two", Value::Null, st, c).await?;
    configure(&mut s.write(), Reply::Options, true);
    type_query("held-context").await?;
    settle(s, 2, true).await?;
    let before = snapshot(&s.peek());
    let buffer=dom(&format!(r#"(()=>{{{}const e=document.querySelector('#app-home input[type=date]');if(!owned(e))return{{ok:false}};const before=e.value;e.value='2026-11-02';return{{ok:true,before,after:e.value,visibleHitTested:true,eventDispatched:false}};}})()"#,fixture::capture::ACTION_GUARD)).await?;
    let choose = include_str!("../../../../context/fixture/check/choose.js")
        .replace("__SP_TEAM__", &json!("TWORK0003").to_string());
    dom(&choose).await?;
    let release = cancelled(s).await?;
    retired(s).await?;
    for _ in 0..100 {
        if crate::app_home::context::Witness::capture(&s.peek())
            .is_ok_and(|w| w.team_id() == Some("TWORK0003"))
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let after = snapshot(&s.peek());
    if before["widget"] != after["widget"]
        || before["formState"] != after["formState"]
        || before["contextSource"]["opens"] != after["contextSource"]["opens"]
    {
        return Err("Context change rewrote multi/widget form or replayed open".into());
    }
    let visit = {
        let s = s.peek();
        let h = s.app_home.surface.as_ref().unwrap();
        actions::Key {
            visit: h.id,
            revision: h.revision,
        }
    };
    if let Some(lease) = crate::app_home::inputs::native::lock(s, visit).await {
        let _ = lease.send(());
        return Err("Unacknowledged native buffer bypassed final lock".into());
    }
    let restored=dom(&format!(r#"(()=>{{{}const e=document.querySelector('#app-home input[type=date]');if(!owned(e)||e.value!=={})return{{ok:false}};e.value={};return{{ok:true,retainedBuffer:true,restored:e.value}};}})()"#,fixture::capture::ACTION_GUARD,buffer["after"],buffer["before"])).await?;
    shot(s,d,n,"context-three",json!({"release":release,"before":before,"buffer":buffer,"restored":restored,"nativeLockRefused":true,"noOpenReplay":true}),st,c).await?;
    configure(&mut s.write(), Reply::Options, false);
    options(s, d, 3).await?;
    shot(s, d, n, "options-three", Value::Null, st, c).await?;
    done(d).await?;
    let action = super::helpers::action(s, d, &[I, A]).await?;
    shot(s, d, n, "action-three", action, st, c).await
}
