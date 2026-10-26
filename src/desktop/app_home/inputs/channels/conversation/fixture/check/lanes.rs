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
    h::choose(s, d, REMOTE_USER).await?;
    if s.peek()
        .core
        .workspaces
        .values()
        .any(|w| w.users.contains_key(REMOTE_USER))
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
    let proof = h::action(s, d, Some(REMOTE_USER)).await?;
    shot(s, d, n, "member-action", proof, st, c).await?;
    h::close(d).await?;
    click(&format!("[data-remove-user='{REMOTE_USER}']")).await?;
    h::selected(s, None, None).await?;
    shot(s, d, n, "cleared", Value::Null, st, c).await?;
    let proof = h::action(s, d, None).await?;
    shot(s, d, n, "empty-action", proof, st, c).await?;
    h::close(d).await?;
    let proof = refresh(&mut s.write(), false, None, None)?;
    h::ready(s).await?;
    capture::settle_interaction(s, d).await?;
    h::open(d).await?;
    h::choose(s, d, PRIVATE).await?;
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
    let proof = h::action(s, d, Some(PRIVATE)).await?;
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
    h::search_rows(s, d, "Ada, Blair", 1).await?;
    h::choose(s, d, REMOTE).await?;
    shot(s, d, n, "selected", Value::Null, st, c).await?;
    let proof = refresh(&mut s.write(), true, None, Some(Some(BETA)))?;
    h::selected(s, Some(REMOTE), Some(REMOTE)).await?;
    shot(s, d, n, "saved-only", proof, st, c).await?;
    let proof = refresh(&mut s.write(), true, Some(Some(BETA)), None)?;
    h::selected(s, Some(BETA), Some(REMOTE)).await?;
    shot(s, d, n, "initial-changed", proof, st, c).await?;
    let proof = refresh(&mut s.write(), true, Some(None), None)?;
    h::selected(s, None, Some(REMOTE)).await?;
    shot(s, d, n, "initial-removed", proof, st, c).await?;
    let proof = h::action(s, d, Some(REMOTE)).await?;
    shot(s, d, n, "committed-action", proof, st, c).await?;
    h::close(d).await?;
    let detached = crate::app_home::inputs::external::fixture::depart(&mut s.write());
    crate::app_home::inputs::external::fixture::return_home(&mut s.write(), detached);
    h::ready(s).await?;
    h::selected(s, Some(REMOTE), Some(REMOTE)).await?;
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
    h::choose(s, d, REMOTE_USER).await?;
    let refreshed = filter_refresh(&mut s.write())?;
    h::ready(s).await?;
    capture::settle_interaction(s, d).await?;
    h::open(d).await?;
    shot(s, d, n, "explicit-options", refreshed, st, c).await?;
    h::search_rows(s, d, "Ada, Blair", 2).await?;
    shot(s, d, n, "explicit-results", Value::Null, st, c).await?;
    h::choose(s, d, REMOTE).await?;
    let script = include_str!("../../../../../context/fixture/check/choose.js")
        .replace("__SP_TEAM__", &json!("TWORK0003").to_string());
    dom(&script).await?;
    h::ready(s).await?;
    capture::settle_interaction(s, d).await?;
    let proof = h::action(s, d, Some(REMOTE)).await?;
    shot(s, d, n, "context-action", proof, st, c).await
}
