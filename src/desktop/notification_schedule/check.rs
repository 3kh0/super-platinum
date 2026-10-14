use super::*;
use serde_json::{Value, json};
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !fixture::requested()
        || std::env::var("SUPER_PLATINUM_NOTIFICATION_SCHEDULE_CHECK").as_deref() != Ok("1")
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let receipt = match run(state, &desktop).await {
            Ok(v) => v,
            Err(e) => json!({"ok":false,"error":e}),
        };
        eprintln!("notification-schedule-check: {receipt}");
    });
}
async fn run(
    mut s: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    wait(d,"({ok:document.getElementById('notification-schedule-daily-start')?.value==='08:00'&&document.getElementById('notification-schedule-daily-end')?.value==='20:00'&&document.getElementById('notification-schedule-mode')?.disabled===false})").await?;
    if s.peek().core.transport.is_some() || s.peek().core.session.is_some() {
        return Err("connected fixture".into());
    }
    let initial = layout(d).await?;
    if writes(&s.peek()) != 0 {
        return Err("Opening wrote preferences".into());
    }
    choose(s, d, "mode", "weekdays", 1, true).await?;
    wait(d,"({ok:document.getElementById('notification-schedule-mode')?.value==='every_day'&&document.getElementById('notification-schedule-mode').disabled})").await?;
    check_status(s, d).await?;
    wait(d,"({ok:document.getElementById('notification-schedule-mode')?.value==='every_day'&&document.getElementById('notification-schedule-mode').disabled})").await?;
    fixture::apply_unconfirmed(&mut s.write());
    check_status(s, d).await?;
    wait(d,"({ok:document.getElementById('notification-schedule-mode')?.value==='weekdays'&&!document.getElementById('notification-schedule-mode').disabled})").await?;
    choose(s, d, "daily-start", "09:00", 2, false).await?;
    choose(s, d, "daily-end", "18:00", 3, false).await?;
    choose(s, d, "mode", "custom", 4, false).await?;
    let mut days = Vec::new();
    for day in Day::ALL {
        let id = format!("notification-schedule-{}-start", day.as_str());
        act(d,&format!("document.getElementById('{id}').scrollIntoView({{block:'center'}});document.getElementById('{id}').focus()")).await?;
        days.push(wait(d,&format!("(()=>{{const e=document.getElementById('{id}'),p=document.getElementById('notification-schedule-scroll'),r=e.getBoundingClientRect(),b=p.getBoundingClientRect();return {{ok:document.activeElement===e&&r.height===32&&r.top>=b.top&&r.bottom<=b.bottom&&document.elementFromPoint(r.left+r.width/2,r.top+r.height/2)===e,day:'{}'}}}})()",day.as_str())).await?);
    }
    let chrome=wait(d,r#"(()=>{const p=document.getElementById('notification-schedule-scroll'),m=document.getElementById('shell-overlay-dialog'),h=m.querySelector('header'),t=m.querySelector('.settings-tabs');const a=h.getBoundingClientRect().top,b=t.getBoundingClientRect().top;p.scrollTop=0;const top=p.scrollTop;p.scrollTop=p.scrollHeight;return {ok:p.scrollTop>top&&h.getBoundingClientRect().top===a&&t.getBoundingClientRect().top===b};})()"#).await?;
    choose(s, d, "monday-start", "all_day", 5, false).await?;
    wait(
        d,
        "({ok:!document.getElementById('notification-schedule-monday-end')})",
    )
    .await?;
    choose(s, d, "tuesday-start", "10:00", 6, false).await?;
    choose(s, d, "friday-end", "23:59", 7, false).await?;
    choose(s, d, "monday-start", "08:30", 8, true).await?;
    wait(d,"({ok:document.getElementById('notification-schedule-reload')?.textContent.trim()==='Check status'&&!document.getElementById('notification-schedule-reload').disabled})").await?;
    act(d,"document.getElementById('notification-schedule-reload').click();document.getElementById('notification-schedule-reload').click()").await?;
    crate::slack_links_check::wait_state(s, |s| {
        s.notification_schedule
            .fixture
            .as_ref()
            .unwrap()
            .read
            .is_some()
    })
    .await?;
    fixture::complete_read(&mut s.write());
    wait(d,"({ok:document.getElementById('notification-schedule-monday-start')?.value==='08:30'&&!document.getElementById('notification-schedule-mode').disabled})").await?;
    let final_layout = layout(d).await?;
    let shell = s.peek();
    let f = shell.notification_schedule.fixture.as_ref().unwrap();
    if !f.valid || f.writes.len() != 8 || f.reads != 10 {
        return Err(format!(
            "bounds: valid={} writes={} reads={}",
            f.valid,
            f.writes.len(),
            f.reads
        ));
    }
    let expected = [
        json!({"dnd_days":"weekdays","dnd_enabled":true}),
        json!({"dnd_end_hour":"09:00","dnd_enabled":true}),
        json!({"dnd_start_hour":"18:00","dnd_enabled":true}),
        json!({"dnd_days":"custom","dnd_enabled":true}),
        json!({"dnd_enabled_monday":"all_day","dnd_enabled":true}),
        json!({"dnd_before_tuesday":"10:00","dnd_enabled_tuesday":"partial","dnd_enabled":true}),
        json!({"dnd_after_friday":"23:59","dnd_enabled_friday":"partial","dnd_enabled":true}),
        json!({"dnd_before_monday":"08:30","dnd_enabled_monday":"partial","dnd_enabled":true}),
    ];
    for (i, (r, e)) in f.writes.iter().zip(expected).enumerate() {
        let got: Value = serde_json::from_str(r["prefs"].as_str().ok_or("missing prefs")?)
            .map_err(|e| e.to_string())?;
        if got != e
            || r["_x_reason"] != "prefs-api/setMultipleUserPrefsByApi"
            || r.get("channel_ids").is_some()
        {
            return Err(format!("wire mismatch {i}: {r}"));
        }
    }
    if shell.core.workspaces["T1"].notifications.user["unknown_pref"] != json!({"keep":7}) {
        return Err("unknown pref lost".into());
    }
    Ok(
        json!({"ok":true,"noTransport":true,"openNoWrite":true,"unpaddedCanonical":true,"everydayWeekdaysCustom":true,"allSevenDays":days,"customNone":true,"pendingFocus":true,"unconfirmedModeCanonical":true,"duplicateBlocked":true,"unknownReadOnlyCheck":true,"exactWire":true,"unknownPreserved":true,"fixedChrome":chrome,"writes":8,"reads":10,"layout":initial,"finalLayout":final_layout}),
    )
}
fn writes(s: &ShellState) -> usize {
    s.notification_schedule
        .fixture
        .as_ref()
        .unwrap()
        .writes
        .len()
}
async fn choose(
    mut s: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    suffix: &str,
    value: &str,
    count: usize,
    unknown: bool,
) -> Result<(), String> {
    let id = format!("notification-schedule-{suffix}");
    wait(
        d,
        &format!("({{ok:document.getElementById('{id}')?.disabled===false}})"),
    )
    .await?;
    act(d,&format!("const e=document.getElementById('{id}');e.scrollIntoView({{block:'center'}});e.focus();e.value='{value}';e.dispatchEvent(new Event('change',{{bubbles:true}}));e.dispatchEvent(new Event('change',{{bubbles:true}}))")).await?;
    crate::slack_links_check::wait_state(s, |s| writes(s) == count).await?;
    wait(d,"({ok:[...document.querySelectorAll('#notification-schedule select')].length>0&&[...document.querySelectorAll('#notification-schedule select')].every(e=>e.disabled)&&document.getElementById('notification-schedule-reload').disabled})").await?;
    if count == 1 && unknown {
        fixture::complete_without_apply(&mut s.write());
    } else {
        fixture::complete(&mut s.write(), unknown);
    }
    if !unknown {
        wait(d,&format!("(()=>{{const e=document.getElementById('{id}');return {{ok:e?.disabled===false&&e.value==='{value}'&&document.activeElement===e,active:document.activeElement?.id,value:e?.value}}}})()")).await?;
    }
    if writes(&s.peek()) != count {
        return Err("duplicate write".into());
    }
    Ok(())
}
async fn wait(d: &dioxus::desktop::DesktopContext, s: &str) -> Result<Value, String> {
    // Preserve the final failed predicate's geometry instead of discarding it
    // when the shared waiter reaches its deadline. This runs only in fixtures.
    let observed = format!("(window.__notificationScheduleLastCheck=({s}))");
    match crate::slack_links_check::wait_dom(d, &observed).await {
        Ok(value) => Ok(value),
        Err(error) => {
            let diagnostics = crate::slack_links_check::wait_dom(d, r#"(()=>{
              const rect=e=>{if(!e)return null;const r=e.getBoundingClientRect();return [r.left,r.top,r.right,r.bottom];};
              return {ok:true,last:window.__notificationScheduleLastCheck,viewport:[innerWidth,innerHeight],
                active:document.activeElement?.id,dialog:rect(document.getElementById('shell-overlay-dialog')),
                scroll:rect(document.getElementById('notification-schedule-scroll')),
                controls:[...document.querySelectorAll('#notification-schedule select')].map(e=>({id:e.id,value:e.value,canonical:e.dataset.canonical,disabled:e.disabled,rect:rect(e)}))};
            })()"#).await;
            Err(format!(
                "{error}; diagnostics={diagnostics:?}; predicate={s}"
            ))
        }
    }
}
async fn act(d: &dioxus::desktop::DesktopContext, s: &str) -> Result<(), String> {
    wait(d, &format!("(()=>{{{s};return {{ok:true}}}})()"))
        .await
        .map(|_| ())
}
async fn layout(d: &dioxus::desktop::DesktopContext) -> Result<Value, String> {
    act(
        d,
        "document.getElementById('notification-schedule-scroll').scrollTop=0",
    )
    .await?;
    wait(d,r#"(()=>{const m=document.getElementById('shell-overlay-dialog'),p=document.getElementById('notification-schedule'),scroll=document.getElementById('notification-schedule-scroll'),mode=document.getElementById('notification-schedule-mode');if(!m||!p||!scroll||!mode)return {ok:false};const r=m.getBoundingClientRect(),b=scroll.getBoundingClientRect(),q=mode.getBoundingClientRect(),tabs=[...m.querySelectorAll('.settings-tab')].map(e=>e.getBoundingClientRect());return {ok:tabs.length===3&&tabs.every(t=>t.top===tabs[0].top)&&r.left>=0&&r.top>=0&&r.right<=innerWidth&&r.bottom<=innerHeight&&b.height>=40&&q.top>=b.top&&q.bottom<=b.bottom&&[...p.querySelectorAll('select,button')].every(e=>{const t=e.getBoundingClientRect();return t.height===32&&t.left>=r.left&&t.right<=r.right;}),tabTops:tabs.map(t=>t.top),viewport:[innerWidth,innerHeight],dialog:[r.left,r.top,r.right,r.bottom],scroll:[b.left,b.top,b.right,b.bottom]};})()"#).await
}

async fn check_status(
    mut s: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<(), String> {
    wait(
        d,
        "({ok:document.getElementById('notification-schedule-reload')?.disabled===false})",
    )
    .await?;
    act(
        d,
        "document.getElementById('notification-schedule-reload').click()",
    )
    .await?;
    crate::slack_links_check::wait_state(s, |s| {
        s.notification_schedule
            .fixture
            .as_ref()
            .unwrap()
            .read
            .is_some()
    })
    .await?;
    fixture::complete_read(&mut s.write());
    Ok(())
}
