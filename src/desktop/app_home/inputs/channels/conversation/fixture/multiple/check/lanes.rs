use super::*;
use helpers::{self as h, Captures};
pub(super) async fn selection(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    c: &mut Captures,
) -> Result<(), String> {
    h::search_rows(s, d, "Ada, Blair", 1).await?;
    shot(s, d, n, "results", Value::Null, st, c).await?;
    h::choose(s, d, REMOTE_USER, &[REMOTE_USER]).await?;
    h::choose(s, d, PRIVATE, &[REMOTE_USER, PRIVATE]).await?;
    h::choose(s, d, REMOTE, &[REMOTE_USER, PRIVATE, REMOTE]).await?;
    if s.peek()
        .core
        .workspaces
        .values()
        .any(|w| w.channels.contains_key(REMOTE))
    {
        return Err("Remote MPIM unexpectedly cached".into());
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
    let action = h::action(s, d, &[REMOTE_USER, PRIVATE, REMOTE]).await?;
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
    h::selected(
        s,
        &[REMOTE_USER, PRIVATE, REMOTE],
        &[REMOTE_USER, PRIVATE, REMOTE],
    )
    .await?;
    h::refreshed(s, d).await?;
    shot(s, d, n, "raised-maximum", proof, st, c).await?;
    capture::settle_interaction(s, d).await?;
    h::remove(s, d, PRIVATE, &[REMOTE_USER, REMOTE]).await?;
    shot(s, d, n, "removed", Value::Null, st, c).await?;
    h::remove(s, d, REMOTE_USER, &[REMOTE]).await?;
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
    h::search_rows(s, d, "Ada, Blair", 1).await?;
    h::choose(s, d, REMOTE_USER, &[REMOTE_USER]).await?;
    h::choose(s, d, REMOTE, &[REMOTE_USER, REMOTE]).await?;
    shot(s, d, n, "selected", Value::Null, st, c).await?;
    let proof = refresh(&mut s.write(), true, 2, None, Some(vec![BETA]))?;
    h::selected(s, &[REMOTE_USER, REMOTE], &[REMOTE_USER, REMOTE]).await?;
    h::refreshed(s, d).await?;
    shot(s, d, n, "saved-only", proof, st, c).await?;
    let proof = refresh(&mut s.write(), true, 2, Some(json!([BETA, ALPHA])), None)?;
    h::selected(s, &[BETA, ALPHA], &[REMOTE_USER, REMOTE]).await?;
    shot(s, d, n, "initial-changed", proof, st, c).await?;
    let proof = refresh(&mut s.write(), true, 2, Some(Value::Null), None)?;
    h::selected(s, &[BETA, ALPHA], &[REMOTE_USER, REMOTE]).await?;
    shot(s, d, n, "initial-removed", proof, st, c).await?;
    let proof = refresh(&mut s.write(), true, 2, Some(json!([])), None)?;
    h::selected(s, &[], &[REMOTE_USER, REMOTE]).await?;
    shot(s, d, n, "initial-empty", proof, st, c).await?;
    let action = h::action(s, d, &[REMOTE_USER, REMOTE]).await?;
    shot(s, d, n, "committed-action", action, st, c).await?;
    h::close(d).await?;
    let detached = crate::app_home::inputs::external::fixture::depart(&mut s.write());
    crate::app_home::inputs::external::fixture::return_home(&mut s.write(), detached);
    h::ready(s).await?;
    h::selected(s, &[REMOTE_USER, REMOTE], &[REMOTE_USER, REMOTE]).await?;
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
pub(super) async fn filter(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    c: &mut Captures,
) -> Result<(), String> {
    h::open(d).await?;
    let before = snapshot(&s.peek());
    let ids = before["query"]["rows"]
        .as_array()
        .ok_or("No filter rows")?
        .iter()
        .filter_map(|v| v["id"].as_str())
        .collect::<Vec<_>>();
    if !ids.contains(&"CCONVARC1")
        || ids.contains(&MPIM)
        || ids.contains(&BETA)
        || before["conversationFilter"]["defaultChannels"] != true
    {
        return Err("Nonempty include-absent filter differed".into());
    }
    shot(
        s,
        d,
        n,
        "filter-options",
        json!({"enterpriseRestrictedLookupAdmitted":true}),
        st,
        c,
    )
    .await?;
    h::search_rows(s, d, "release", 1).await?;
    shot(s, d, n, "filter-results", Value::Null, st, c).await?;
    h::choose(s, d, REMOTE_USER, &[ALPHA, REMOTE_USER]).await?;
    let refreshed = filter_refresh(&mut s.write())?;
    h::refreshed(s, d).await?;
    h::open(d).await?;
    shot(s, d, n, "explicit-options", refreshed, st, c).await?;
    h::search_rows(s, d, "Ada, Blair", 2).await?;
    shot(s, d, n, "explicit-results", Value::Null, st, c).await?;
    h::choose(s, d, REMOTE, &[ALPHA, REMOTE_USER, REMOTE]).await?;
    let script = include_str!("../../../../../../context/fixture/check/choose.js")
        .replace("__SP_TEAM__", &json!("TWORK0003").to_string());
    dom(&script).await?;
    h::refreshed(s, d).await?;
    let proof = h::action(s, d, &[ALPHA, REMOTE_USER, REMOTE]).await?;
    shot(s, d, n, "context-action", proof, st, c).await
}
