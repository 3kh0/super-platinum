use super::*;
use helpers::{self as h, Captures};
pub(super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    c: &mut Captures,
) -> Result<(), String> {
    configure(&mut s.write(), Reply::Offline, false);
    h::search_rows(s, d, "alpha", 1).await?;
    shot(s, d, n, "offline", Value::Null, st, c).await?;
    // Use exact text to target Retry rather than a cached channel or remove button.
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
    configure(&mut s.write(), Reply::Conversations, false);
    click(retry).await?;
    h::settle(s, 3, false).await?;
    shot(s, d, n, "recovered", Value::Null, st, c).await?;
    h::choose(s, d, BETA, &[ALPHA, BETA]).await?;
    retirement(s, d, n, st, c).await
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
async fn retirement(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    c: &mut Captures,
) -> Result<(), String> {
    configure(&mut s.write(), Reply::Conversations, true);
    h::open(d).await?;
    type_query("held-query").await?;
    h::settle(s, 4, true).await?;
    shot(s, d, n, "query-held", Value::Null, st, c).await?;
    click(&format!("[data-remove-user='{ALPHA}']")).await?;
    h::selected(s, &[BETA], &[BETA]).await?;
    let release = h::cancelled(s).await?;
    shot(s, d, n, "query-replaced", release, st, c).await?;
    configure(&mut s.write(), Reply::Conversations, true);
    type_query("held-context").await?;
    h::settle(s, 5, true).await?;
    let before = snapshot(&s.peek());
    let script = include_str!("../../../../../../context/fixture/check/choose.js")
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
    h::open(d).await?;
    shot(
        s,
        d,
        n,
        "context-options",
        json!({"actionContext":"TWORK0003","searchFilterContext":null}),
        st,
        c,
    )
    .await?;
    click("[data-modal-users] .app-modal-users-actions button:last-child").await?;
    h::retired(s).await?;
    let action = h::action(s, d, &[BETA]).await?;
    shot(s, d, n, "context-action", action, st, c).await?;
    h::close(d).await?;
    configure(&mut s.write(), Reply::Conversations, true);
    h::open(d).await?;
    type_query("held-view").await?;
    h::settle(s, 6, true).await?;
    shot(s, d, n, "view-held", Value::Null, st, c).await?;
    let refreshed = refresh(&mut s.write(), true, 2, None, None)?;
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
    configure(&mut s.write(), Reply::Conversations, true);
    h::open(d).await?;
    type_query("held-socket").await?;
    h::settle(s, 7, true).await?;
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
    configure(&mut s.write(), Reply::Conversations, true);
    h::open(d).await?;
    type_query("held-account").await?;
    h::settle(s, 8, true).await?;
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
