use super::*;
use helpers::{self as h, Captures};
pub(super) async fn selection(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    c: &mut Captures,
) -> Result<(), String> {
    h::search_rows(s, d, "reviewer", 1).await?;
    shot(s, d, n, "results", Value::Null, st, c).await?;
    h::choose(s, d, REMOTE).await?;
    if s.peek()
        .core
        .workspaces
        .values()
        .any(|w| w.users.contains_key(REMOTE))
    {
        return Err("Remote-only member was cached".into());
    }
    shot(
        s,
        d,
        n,
        "remote-selected",
        json!({"remoteNotInWorkspace":true}),
        st,
        c,
    )
    .await?;
    click(&format!("[data-remove-user='{REMOTE}']")).await?;
    h::selected(s, None, None).await?;
    shot(s, d, n, "cleared", Value::Null, st, c).await?;
    let proof = h::action(s, d, None).await?;
    shot(s, d, n, "empty-action", proof, st, c).await?;
    h::close(d).await?;
    let proof = refresh(&mut s.write(), false, None, None)?;
    h::ready(s).await?;
    capture::settle_interaction(s, d).await?;
    h::open(d).await?;
    h::choose(s, d, ADA).await?;
    shot(s, d, n, "required-selected", proof, st, c).await?;
    let buffer=dom(&format!(r#"(()=>{{{}const e=document.querySelector('#app-home input[type=date]');if(!owned(e))return{{ok:false}};const before=e.value;e.value='2026-11-02';return{{ok:true,before,after:e.value,eventDispatched:false}};}})()"#,crate::app_home::inputs::external::fixture::capture::ACTION_GUARD)).await?;
    let visit = {
        let s = s.peek();
        let v = s.app_home.surface.as_ref().unwrap();
        actions::Key {
            visit: v.id,
            revision: v.revision,
        }
    };
    if let Some(lease) = crate::app_home::inputs::native::lock(s, visit).await {
        let _ = lease.send(());
        return Err("Unacknowledged date bypassed action lock".into());
    }
    let restored=dom(&format!(r#"(()=>{{{}const e=document.querySelector('#app-home input[type=date]');if(!owned(e)||e.value!=={})return{{ok:false}};e.value={};return{{ok:true,value:e.value}};}})()"#,crate::app_home::inputs::external::fixture::capture::ACTION_GUARD,buffer["after"],buffer["before"])).await?;
    let proof = h::action(s, d, Some(ADA)).await?;
    shot(
        s,
        d,
        n,
        "required-action",
        json!({"action":proof,"buffer":buffer,"restored":restored,"nativeLockRefused":true}),
        st,
        c,
    )
    .await
}
pub(super) async fn props(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    c: &mut Captures,
) -> Result<(), String> {
    h::open(d).await?;
    h::choose(s, d, BLAIR).await?;
    shot(s, d, n, "selected", Value::Null, st, c).await?;
    let proof = refresh(&mut s.write(), true, None, Some(Some(CASEY)))?;
    h::selected(s, Some(BLAIR), Some(BLAIR)).await?;
    shot(s, d, n, "saved-only", proof, st, c).await?;
    let proof = refresh(&mut s.write(), true, Some(Some(CASEY)), None)?;
    h::selected(s, Some(CASEY), Some(BLAIR)).await?;
    shot(s, d, n, "initial-changed", proof, st, c).await?;
    let proof = refresh(&mut s.write(), true, Some(None), None)?;
    h::selected(s, None, Some(BLAIR)).await?;
    shot(s, d, n, "initial-removed", proof, st, c).await?;
    let proof = h::action(s, d, Some(BLAIR)).await?;
    shot(s, d, n, "committed-action", proof, st, c).await?;
    h::close(d).await?;
    let detached = crate::app_home::inputs::external::fixture::depart(&mut s.write());
    crate::app_home::inputs::external::fixture::return_home(&mut s.write(), detached);
    h::ready(s).await?;
    h::selected(s, Some(BLAIR), Some(BLAIR)).await?;
    shot(
        s,
        d,
        n,
        "returned",
        json!({"cachedReturnFixtureAdapter":true}),
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
    configure(&mut s.write(), Reply::Offline, false);
    h::search_rows(s, d, "ada", 1).await?;
    shot(s, d, n, "offline", Value::Null, st, c).await?;
    // Use exact text to target Retry rather than a cached user or remove button.
    let retry = "[data-modal-users] .app-modal-users-actions button:first-child";
    click(retry).await?;
    h::settle(s, 1, false).await?;
    shot(
        s,
        d,
        n,
        "cooldown",
        json!({"requestCountUnchanged":true}),
        st,
        c,
    )
    .await?;
    wait_cooldown(s).await?;
    configure(&mut s.write(), Reply::RateLimited, false);
    click(retry).await?;
    h::settle(s, 2, false).await?;
    shot(s, d, n, "rate-limited", Value::Null, st, c).await?;
    wait_cooldown(s).await?;
    configure(&mut s.write(), Reply::People, false);
    click(retry).await?;
    h::settle(s, 3, false).await?;
    shot(s, d, n, "recovered", Value::Null, st, c).await?;
    h::choose(s, d, BLAIR).await?;
    let proof = h::action(s, d, Some(BLAIR)).await?;
    shot(s, d, n, "selected-action", proof, st, c).await
}
async fn wait_cooldown(s: Signal<ShellState>) -> Result<(), String> {
    for _ in 0..350 {
        if snapshot(&s.peek())["cooling"] == false {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Directory retry deadline did not expire".into())
}
pub(super) async fn retirement(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    c: &mut Captures,
) -> Result<(), String> {
    h::open(d).await?;
    h::choose(s, d, BLAIR).await?;
    configure(&mut s.write(), Reply::People, true);
    h::open(d).await?;
    type_query("held-query").await?;
    h::settle(s, 1, true).await?;
    shot(s, d, n, "query-held", Value::Null, st, c).await?;
    type_query("").await?;
    let release = h::cancelled(s).await?;
    shot(s, d, n, "query-replaced", release, st, c).await?;
    configure(&mut s.write(), Reply::People, true);
    type_query("held-context").await?;
    h::settle(s, 2, true).await?;
    let before = snapshot(&s.peek());
    let script = include_str!("../../../../context/fixture/check/choose.js")
        .replace("__SP_TEAM__", &json!("TWORK0003").to_string());
    dom(&script).await?;
    let release = h::cancelled(s).await?;
    h::retired(s).await?;
    let after = snapshot(&s.peek());
    if before["widget"] != after["widget"] || before["formState"] != after["formState"] {
        return Err("Context rewrote selected user".into());
    }
    shot(
        s,
        d,
        n,
        "context-retired",
        json!({"before":before,"release":release}),
        st,
        c,
    )
    .await?;
    let action = h::action(s, d, Some(BLAIR)).await?;
    shot(s, d, n, "context-action", action, st, c).await?;
    h::close(d).await?;
    configure(&mut s.write(), Reply::People, true);
    h::open(d).await?;
    type_query("held-view").await?;
    h::settle(s, 3, true).await?;
    shot(s, d, n, "view-held", Value::Null, st, c).await?;
    let refreshed = refresh(&mut s.write(), true, None, None)?;
    let release = h::cancelled(s).await?;
    h::retired(s).await?;
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
    configure(&mut s.write(), Reply::People, true);
    h::open(d).await?;
    type_query("held-socket").await?;
    h::settle(s, 4, true).await?;
    shot(s, d, n, "socket-held", Value::Null, st, c).await?;
    {
        let mut s = s.write();
        let team = s.core.active_team.clone().unwrap();
        s.core.workspaces.get_mut(&team).unwrap().rt_generation += 1;
    }
    let release = h::cancelled(s).await?;
    h::retired(s).await?;
    shot(s, d, n, "socket-retired", release, st, c).await?;
    {
        let mut s = s.write();
        let owner = crate::app_home::Owner::capture(&s).ok_or("Missing new socket owner")?;
        let h = s.app_home.surface.as_mut().unwrap();
        h.id = owner.home_visit;
        h.owner = owner;
        h.revision = next();
        crate::app_home::inputs::hydrate(&mut s);
    }
    configure(&mut s.write(), Reply::People, true);
    h::open(d).await?;
    type_query("held-account").await?;
    h::settle(s, 5, true).await?;
    shot(
        s,
        d,
        n,
        "account-held",
        json!({"cachedSocketRebindFixtureAdapter":true}),
        st,
        c,
    )
    .await?;
    s.write().core.account_epoch += 1;
    let release = h::cancelled(s).await?;
    h::retired(s).await?;
    shot(s, d, n, "account-retired", release, st, c).await
}
