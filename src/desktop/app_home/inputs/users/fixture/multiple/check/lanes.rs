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
    h::choose(s, d, BLAIR, &[ADA, BLAIR]).await?;
    h::choose(s, d, REMOTE, &[ADA, BLAIR, REMOTE]).await?;
    if s.peek()
        .core
        .workspaces
        .values()
        .any(|w| w.users.contains_key(REMOTE))
    {
        return Err("Remote member unexpectedly cached".into());
    }
    shot(
        s,
        d,
        n,
        "over-maximum",
        json!({"remoteNotInWorkspace":true}),
        st,
        c,
    )
    .await?;
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
        return Err("Unacknowledged date bypassed native lock".into());
    }
    let restored=dom(&format!(r#"(()=>{{{}const e=document.querySelector('#app-home input[type=date]');if(!owned(e)||e.value!=={})return{{ok:false}};e.value={};return{{ok:true,value:e.value}};}})()"#,crate::app_home::inputs::external::fixture::capture::ACTION_GUARD,buffer["after"],buffer["before"])).await?;
    let action = h::action(s, d, &[ADA, BLAIR, REMOTE]).await?;
    shot(
        s,
        d,
        n,
        "over-maximum-action",
        json!({"action":action,"buffer":buffer,"restored":restored,"nativeLockRefused":true}),
        st,
        c,
    )
    .await?;
    h::close(d).await?;
    let proof = refresh(&mut s.write(), false, 4, None, None)?;
    h::selected(s, &[ADA, BLAIR, REMOTE], &[ADA, BLAIR, REMOTE]).await?;
    shot(s, d, n, "raised-maximum", proof, st, c).await?;
    capture::settle_interaction(s, d).await?;
    h::remove(s, d, BLAIR, &[ADA, REMOTE]).await?;
    shot(s, d, n, "removed", Value::Null, st, c).await?;
    h::remove(s, d, ADA, &[REMOTE]).await?;
    h::remove(s, d, REMOTE, &[]).await?;
    shot(s, d, n, "required-empty", Value::Null, st, c).await?;
    let action = h::action(s, d, &[]).await?;
    shot(s, d, n, "empty-action", action, st, c).await
}
pub(super) async fn props(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    c: &mut Captures,
) -> Result<(), String> {
    h::open(d).await?;
    h::choose(s, d, BLAIR, &[BLAIR]).await?;
    shot(s, d, n, "selected", Value::Null, st, c).await?;
    let proof = refresh(&mut s.write(), true, 2, None, Some(vec![CASEY]))?;
    h::selected(s, &[BLAIR], &[BLAIR]).await?;
    settle_refreshed_picker(s, d).await?;
    shot(s, d, n, "saved-only", proof, st, c).await?;
    let proof = refresh(&mut s.write(), true, 2, Some(json!([CASEY, ADA])), None)?;
    h::selected(s, &[CASEY, ADA], &[BLAIR]).await?;
    shot(s, d, n, "initial-changed", proof, st, c).await?;
    let proof = refresh(&mut s.write(), true, 2, Some(Value::Null), None)?;
    h::selected(s, &[CASEY, ADA], &[BLAIR]).await?;
    shot(s, d, n, "initial-removed", proof, st, c).await?;
    let proof = refresh(&mut s.write(), true, 2, Some(json!([])), None)?;
    h::selected(s, &[], &[BLAIR]).await?;
    shot(s, d, n, "initial-empty", proof, st, c).await?;
    let action = h::action(s, d, &[BLAIR]).await?;
    shot(s, d, n, "committed-action", action, st, c).await?;
    h::close(d).await?;
    let detached = crate::app_home::inputs::external::fixture::depart(&mut s.write());
    crate::app_home::inputs::external::fixture::return_home(&mut s.write(), detached);
    h::ready(s).await?;
    h::selected(s, &[BLAIR], &[BLAIR]).await?;
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
    h::choose(s, d, REMOTE, &[ADA, REMOTE]).await?;
    let action = h::action(s, d, &[ADA, REMOTE]).await?;
    shot(s, d, n, "selected-action", action, st, c).await
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

/// The current renderer already rejects the old view Ticket. Wait for the real
/// runtime tick to remove that picker before freezing the capture inventory.
async fn settle_refreshed_picker(s: Signal<ShellState>, d: &DesktopContext) -> Result<(), String> {
    let expected = snapshot(&s.peek());
    if expected["pending"] != false
        || (expected["query"].is_object() && expected["query"]["current"] != false)
    {
        return Err("Refreshed props retained an active member query".into());
    }
    for _ in 0..150 {
        let now = snapshot(&s.peek());
        for key in [
            "authority",
            "rawView",
            "formState",
            "retainedStates",
            "widget",
            "context",
        ] {
            if now[key] != expected[key] {
                return Err(format!("Props retirement changed {key}"));
            }
        }
        if now["query"].is_null() && now["pending"] == false {
            // A remounted date may acknowledge during retirement. The existing
            // settlement proves its exact current host/sequence/value before
            // any capture; subsequent edges retain one immutable input revision.
            return capture::settle_interaction(s, d).await;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Runtime did not retire refreshed props picker".into())
}
