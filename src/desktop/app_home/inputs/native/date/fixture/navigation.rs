//! Actual offline tab navigation; the existing cache adapter supplies retained source.
use super::*;
use crate::app_home::inputs::external::fixture as cached;
pub(crate) async fn reset(s: Signal<ShellState>, _d: &DesktopContext) -> Result<Value, String> {
    let before = snapshot(&s.peek());
    let click=capture::dom(&format!(r#"(()=>{{{}const b=[...document.querySelectorAll('#app-home button')].find(e=>e.textContent.trim()==='Reset inputs');b?.scrollIntoView({{block:'center'}});if(!owned(b))return{{ok:false}};b.click();return{{ok:true,text:b.textContent,visibleHitTested:true,trusted:false}};}})()"#,capture::ACTION_GUARD)).await?;
    for _ in 0..200 {
        if snapshot(&s.peek())["authority"]["homeRevision"] != before["authority"]["homeRevision"] {
            return Ok(json!({"click":click,"beforeRevision":before["authority"]["homeRevision"]}));
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err("Date Reset did not replace form revision".into())
}
async fn tab(label: &str) -> Result<Value, String> {
    capture::dom(&format!(r#"(()=>{{{}const nav=document.querySelector('.channel-tabs[aria-label="Conversation tabs"]'),b=[...(nav?.querySelectorAll('button')||[])].find(b=>b.textContent.trim()==={});if(!owned(b))return{{ok:false}};const before=b.getAttribute('aria-pressed');b.click();return{{ok:true,label:b.textContent.trim(),before,visibleHitTested:true,actualClick:true,trusted:false}};}})()"#,capture::ACTION_GUARD,json!(label))).await
}
pub(crate) async fn revisit(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
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
    let messages = tab("Messages").await?;
    capture::wait(d,"!document.querySelector('#app-home,.app-home') && [...document.querySelectorAll('.channel-tabs button')].some(b=>b.textContent.trim()==='Messages'&&b.getAttribute('aria-pressed')==='true')").await?;
    let mut departed = false;
    for _ in 0..200 {
        let done = {
            let state = s.peek();
            let x = snapshot(&state);
            state.channel_tab == ChannelTab::Messages
                && state.app_home.surface.is_none()
                && x["nativeMounts"].is_null()
                && x["windows"] == json!([])
                && x["activity"] == json!([])
        };
        if done {
            departed = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let away = snapshot(&s.peek());
    if !departed || away["retainedStates"] != before["retainedStates"] {
        return Err("Date actual tab departure did not retire mounts and retain parent".into());
    }
    let home = tab("Home").await?;
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
            return Err("Date cached Home adapter lost actual navigation owner".into());
        }
        cached::return_home(&mut state, source);
    }
    settle(s, d).await?;
    let after = snapshot(&s.peek());
    if after["formState"] != before["formState"]
        || after["retainedStates"] != before["retainedStates"]
        || after["authority"]["homeVisit"] == before["authority"]["homeVisit"]
    {
        return Err("Date cached return changed committed state or reused visit".into());
    }
    let quiet = silence(s).await?;
    Ok(
        json!({"messages":messages,"home":home,"before":before,"away":away,"silence":quiet,"cachedViewAdapter":true,"adapterAfterActualHomeRoute":true,"homeOpenRequestClaim":false}),
    )
}
