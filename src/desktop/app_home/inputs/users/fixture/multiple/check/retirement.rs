use super::*;
use helpers::{self as h, Captures};
pub(super) async fn retirement(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    c: &mut Captures,
) -> Result<(), String> {
    capture::settle_interaction(s, d).await?;
    h::open(d).await?;
    h::choose(s, d, BLAIR, &[ADA, BLAIR]).await?;
    configure(&mut s.write(), Reply::People, true);
    capture::settle_interaction(s, d).await?;
    h::open(d).await?;
    type_query("ada").await?;
    h::settle(s, 1, true).await?;
    shot(s, d, n, "query-held", Value::Null, st, c).await?;
    h::choose(s, d, ADA, &[BLAIR]).await?;
    let release = h::cancelled(s).await?;
    shot(s, d, n, "selection-reticketed", release, st, c).await?;
    configure(&mut s.write(), Reply::People, true);
    type_query("held-context").await?;
    h::settle(s, 2, true).await?;
    let before = snapshot(&s.peek());
    let script = include_str!("../../../../../context/fixture/check/choose.js")
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
    let action = h::action(s, d, &[BLAIR]).await?;
    shot(s, d, n, "context-action", action, st, c).await?;
    h::close(d).await?;
    configure(&mut s.write(), Reply::People, true);
    capture::settle_interaction(s, d).await?;
    h::open(d).await?;
    type_query("held-view").await?;
    h::settle(s, 3, true).await?;
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
    configure(&mut s.write(), Reply::People, true);
    capture::settle_interaction(s, d).await?;
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
    capture::settle_interaction(s, d).await?;
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
