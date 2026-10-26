//! Real offline tab navigation with an explicitly supplied cached Home source.
use super::*;
use crate::app_home::inputs::external::fixture as cached;

pub(crate) fn named() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("app-home-static-remount" | "app-home-static-remount-narrow")
        )
        && std::env::var("SUPER_PLATINUM_APP_HOME_STATIC_REMOUNT_CHECK").as_deref() == Ok("1")
}
pub(crate) fn start(s: Signal<ShellState>, d: DesktopContext) {
    if !named() {
        return;
    }
    let name = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap_or_default();
    dioxus::core::spawn_forever(async move {
        let result = match tokio::time::timeout(Duration::from_secs(180), run(s, &d, &name)).await {
            Ok(Ok(v)) => v,
            Ok(Err(error)) => {
                json!({"ok":false,"fixture":name,"case":"remount","error":error,"model":snapshot(&s.peek())})
            }
            Err(_) => {
                json!({"ok":false,"fixture":name,"case":"remount","error":"Static remount fixture timeout","model":snapshot(&s.peek())})
            }
        };
        let _ = tokio::task::spawn_blocking(move || {
            eprintln!("app-home-static-remount-check: {result}")
        })
        .await;
    });
}
async fn tab(label: &str) -> Result<Value, String> {
    capture::dom(&format!(r#"(()=>{{{}const nav=document.querySelector('.channel-tabs[aria-label="Conversation tabs"]'),b=[...(nav?.querySelectorAll('button')||[])].find(b=>b.textContent.trim()==={});if(!owned(b))return{{ok:false}};const before=b.getAttribute('aria-pressed');b.click();return{{ok:true,label:b.textContent.trim(),before,visibleHitTested:true,actualClick:true}};}})()"#,capture::ACTION_GUARD,json!(label))).await
}
async fn old_node(mode: &str) -> Result<Value, String> {
    capture::dom(&format!(
        "(()=>{{const mode={};{} }})()",
        json!(mode),
        include_str!("remount.js")
    ))
    .await
}
async fn run(mut s: Signal<ShellState>, d: &DesktopContext, n: &str) -> Result<Value, String> {
    let readiness = capture::readiness(n).await?;
    let mut stages = vec![];
    let mut captures = Captures::new();
    shot(
        s,
        d,
        n,
        "initial",
        json!({"scope":"Cached offline source; no Home-open request claim"}),
        &mut stages,
        &mut captures,
    )
    .await?;
    let selected = choose(s, d, "pick", vec![None]).await?;
    counts(s, 1, 1).await?;
    let silence = quiet(s, 1).await?;
    shot(
        s,
        d,
        n,
        "cleared",
        json!({"selection":selected,"silence":silence}),
        &mut stages,
        &mut captures,
    )
    .await?;
    let original = snapshot(&s.peek());
    let (source, original_owner) = {
        let state = s.peek();
        (
            cached::cached_source(&state),
            state.app_home.surface.as_ref().unwrap().owner.clone(),
        )
    };
    let saved_node = old_node("save").await?;
    let messages = tab("Messages").await?;
    capture::wait(d,"!document.querySelector('#app-home,.app-home') && [...document.querySelectorAll('.channel-tabs button')].some(b=>b.textContent.trim()==='Messages'&&b.getAttribute('aria-pressed')==='true')").await?;
    for _ in 0..200 {
        let done = {
            let state = s.peek();
            let model = snapshot(&state);
            state.channel_tab == ChannelTab::Messages
                && state.app_home.surface.is_none()
                && model["mounts"] == json!([])
                && model["windows"] == json!([])
                && model["activity"] == json!([])
        };
        if done {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let departed_node = old_node("away").await?;
    let before_away = snapshot(&s.peek());
    if before_away["authority"] != Value::Null
        || before_away["mounts"] != json!([])
        || before_away["windows"] != json!([])
        || before_away["activity"] != json!([])
        || before_away["retainedStates"] != original["retainedStates"]
    {
        return Err(
            "Real tab departure did not retire native authority and retain the null form".into(),
        );
    }
    let script = format!(
        r#"(()=>{{{}const nav=document.querySelector('.channel-tabs'),b=[...(nav?.querySelectorAll('button')||[])].find(b=>b.textContent.trim()==='Messages');if(document.querySelector('#app-home,.app-home')||!owned(b)||b.getAttribute('aria-pressed')!=='true')return{{ok:false}};return{{ok:true,viewport:{{width:innerWidth,height:innerHeight}},homeAbsent:true,selectedTab:'Messages',target:{{text:b.textContent.trim(),visibleHitTested:true}},timelinePresent:!!document.getElementById('message-timeline')}};}})()"#,
        capture::ACTION_GUARD
    );
    let mut geometry = crate::slack_links_check::wait_dom(d, &script).await?;
    capture::capture_image(d, n, "away", &mut geometry, &mut captures).await?;
    if snapshot(&s.peek()) != before_away {
        return Err("Away capture changed retained Home state".into());
    }
    stages.push(json!({"stage":"away","geometry":geometry,"scrollViews":[],"model":before_away,"proof":{"navigation":messages,"savedNode":saved_node,"oldNode":departed_node,"original":original,"actualLeave":true}}));

    let home = tab("Home").await?;
    capture::wait(d,"[...document.querySelectorAll('.channel-tabs button')].some(b=>b.textContent.trim()==='Home'&&b.getAttribute('aria-pressed')==='true') && !!document.querySelector('.app-home')").await?;
    {
        let mut state = s.write();
        if state.channel_tab != ChannelTab::AppHome
            || state.app_home.surface.is_some()
            || state.core.active_channel.as_deref() != Some(original_owner.channel.as_str())
            || !crate::message_buttons::inputs::user::directory::context_current(
                &state,
                &original_owner.ctx,
            )
            || state
                .core
                .workspaces
                .get(&original_owner.ctx.session.team_id)
                .is_none_or(|w| w.rt_generation != original_owner.rt)
        {
            return Err("Cached Home adapter lost actual navigation owner".into());
        }
        // Generic offline channel_resources::refresh intentionally does no HTTP.
        // Only after the actual Home tab transition, supply the captured source.
        cached::return_home(&mut state, source);
    }
    settle(s, d).await?;
    let before_old = snapshot(&s.peek());
    if before_old["formState"] != original["formState"]
        || before_old["retainedStates"] != original["retainedStates"]
        || before_old["authority"]["homeVisit"] == original["authority"]["homeVisit"]
    {
        return Err("Cached return changed committed null or reused old visit".into());
    }
    let detached = old_node("probe").await?;
    let silence = quiet(s, 1).await?;
    if snapshot(&s.peek()) != before_old {
        return Err("Detached old-node event changed current Home state".into());
    }
    shot(s,d,n,"returned",json!({"navigation":home,"cachedViewAdapter":true,"adapterAfterActualHomeRoute":true,"homeOpenRequestClaim":false,"beforeDeparture":original,"detachedNode":detached,"silence":silence}),&mut stages,&mut captures).await?;
    let selection = choose(s, d, "pick", vec![Some(1)]).await?;
    counts(s, 2, 2).await?;
    let silence = quiet(s, 2).await?;
    shot(
        s,
        d,
        n,
        "reselected",
        json!({"selection":selection,"silence":silence}),
        &mut stages,
        &mut captures,
    )
    .await?;
    Ok(
        json!({"ok":true,"fixture":n,"case":"remount","readiness":readiness,"viewport":stages[0]["geometry"]["viewport"],"stages":stages,"captures":captures,"model":snapshot(&s.peek()),"captureChecksPassed":true,"networkCalls":0,"visualReview":"Individual original PNG review required"}),
    )
}
