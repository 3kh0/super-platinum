//! Real tab actions, with the established offline cached-source return adapter.
use super::*;
use crate::app_home::inputs::external::fixture as cached;
pub(super) async fn revisit(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    target: Option<&str>,
) -> Result<Value, String> {
    settle(s, d).await?;
    let before = snapshot(&s.peek());
    let (source, owner) = {
        let state = s.peek();
        (
            cached::cached_source(&state),
            state.app_home.surface.as_ref().unwrap().owner.clone(),
        )
    };
    let messages = probe("tab", "qsingle", json!({"label":"Messages"})).await?;
    capture::wait(d,"!document.querySelector('#app-home,.app-home') && [...document.querySelectorAll('.channel-tabs button')].some(b=>b.textContent.trim()==='Messages'&&b.getAttribute('aria-pressed')==='true')").await?;
    for _ in 0..200 {
        let x = snapshot(&s.peek());
        if s.peek().app_home.surface.is_none()
            && x["queue"] == json!([])
            && x["textMounts"] == json!([])
            && x["activity"] == json!([])
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let away = snapshot(&s.peek());
    if s.peek().app_home.surface.is_some() || away["retainedStates"] != before["retainedStates"] {
        return Err("Tab departure did not retain exact parents".into());
    }
    let observer = probe("begin", target.unwrap_or("qsingle"), json!({})).await?;
    let home = probe("tab", "qsingle", json!({"label":"Home"})).await?;
    capture::wait(d,"[...document.querySelectorAll('.channel-tabs button')].some(b=>b.textContent.trim()==='Home'&&b.getAttribute('aria-pressed')==='true') && !!document.querySelector('.app-home')").await?;
    {
        let mut state = s.write();
        if state.channel_tab != ChannelTab::AppHome
            || state.app_home.surface.is_some()
            || state.core.active_channel.as_deref() != Some(owner.channel.as_str())
            || !crate::message_buttons::inputs::user::directory::context_current(&state, &owner.ctx)
            || state
                .core
                .workspaces
                .get(&owner.ctx.session.team_id)
                .is_none_or(|w| w.rt_generation != owner.rt)
        {
            return Err("Cached focus return lost actual navigation owner".into());
        }
        cached::return_home(&mut state, source);
    }
    settle(s, d).await?;
    let effect = if let Some(block) = target {
        active(d, block, true).await?
    } else {
        effect("qsingle").await?
    };
    if target.is_none() {
        no_field_focus(&effect)?;
    }
    let after = snapshot(&s.peek());
    if after["authority"]["homeVisit"] == before["authority"]["homeVisit"] {
        return Err("Actual return reused focus visit".into());
    }
    Ok(
        json!({"before":before,"messages":messages,"away":away,"observer":observer,"home":home,"after":after,"effect":effect,"cachedViewAdapter":true,"adapterAfterActualHomeRoute":true,"homeOpenRequestClaim":false}),
    )
}
