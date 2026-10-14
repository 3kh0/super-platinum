//! Disconnected native behavior and geometry; synthetic DOM events, not OS input.
use super::*;
use serde_json::{Value, json};
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !fixture::requested()
        || std::env::var("SUPER_PLATINUM_GLOBAL_NOTIFICATIONS_CHECK").as_deref() != Ok("1")
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let receipt = match run(state, &desktop).await {
            Ok(v) => v,
            Err(e) => json!({"ok":false,"error":e}),
        };
        eprintln!("global-notifications-check: {receipt}");
    });
}
async fn run(
    mut s: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    wait(d,"({ok:document.getElementById('global-desktop-mode')?.value==='mentions_dms'&&!document.getElementById('global-desktop-mode').disabled&&document.getElementById('global-keywords-input')?.value==='alpha,  beta'&&document.getElementById('global-keywords-input').dataset.appliedSeed===document.getElementById('global-keywords-input').dataset.seed})").await?;
    if s.peek().core.transport.is_some()
        || s.peek().core.session.is_some()
        || writes(&s.peek()) != 0
    {
        return Err("fixture opening connected or wrote".into());
    }
    let channels = s.peek().core.workspaces["T1"]
        .notifications
        .channels
        .clone();
    let initial = layout(d).await?;
    // Opening separate mobile controls is mounted state only; no-op relink closes them.
    click(d, "global-mobile-overrides").await?;
    wait(d, "({ok:!!document.getElementById('global-mobile-mode')})").await?;
    click(d, "global-mobile-overrides").await?;
    wait(d, "({ok:!document.getElementById('global-mobile-mode')})").await?;
    if writes(&s.peek()) != 0 {
        return Err("local disclosure wrote".into());
    }
    change(s, d, "global-desktop-mode", Some("everything"), 1, false).await?;
    change(s, d, "global-desktop-push", None, 2, false).await?;
    click(d, "global-mobile-overrides").await?;
    wait(d, "({ok:!!document.getElementById('global-mobile-mode')})").await?;
    change(s, d, "global-mobile-mode", Some("mentions_dms"), 3, false).await?;
    change(s, d, "global-mobile-overrides", None, 4, false).await?;
    change(s, d, "global-mobile-push", None, 5, false).await?;
    wait(d,"({ok:document.getElementById('global-mobile-overrides').disabled&&!document.getElementById('global-mobile-push').checked})").await?;
    change(s, d, "global-mobile-push", None, 6, false).await?;
    change(s, d, "global-thread-replies", None, 7, false).await?;
    // Uncontrolled keyword input keeps native text, node, selection during renders.
    act(d,"const e=document.getElementById('global-keywords-input');e.scrollIntoView({block:'center'});e.focus();e.value='alpha,  beta,🦊';e.setSelectionRange(4,4);window.__keywordNode=e;e.dispatchEvent(new Event('input',{bubbles:true}))").await?;
    wait(d,"({ok:document.getElementById('global-keywords-save').disabled===false&&document.getElementById('global-keywords-input')===window.__keywordNode&&window.__keywordNode.value==='alpha,  beta,🦊'&&window.__keywordNode.selectionStart===4})").await?;
    click(d, "global-keywords-discard").await?;
    wait(d,"({ok:document.getElementById('global-keywords-input').value==='alpha,  beta'&&document.activeElement===document.getElementById('global-keywords-input')&&document.getElementById('global-keywords-save').disabled})").await?;
    act(d,"const e=document.getElementById('global-keywords-input');e.value='alpha,  beta,🦊';e.dispatchEvent(new Event('input',{bubbles:true}))").await?;
    wait(
        d,
        "({ok:!document.getElementById('global-keywords-save').disabled})",
    )
    .await?;
    click(d, "notification-page-hours").await?;
    wait(d,"({ok:!document.getElementById('global-notifications')&&!!document.getElementById('notification-schedule')})").await?;
    click(d, "notification-page-defaults").await?;
    wait(d,"({ok:document.getElementById('global-keywords-input')?.value==='alpha,  beta,🦊'&&!document.getElementById('global-keywords-save').disabled})").await?;
    if writes(&s.peek()) != 7 {
        return Err("draft navigation wrote".into());
    }
    // Invalid CSV never sends; user draft remains editable.
    act(d,"const e=document.getElementById('global-keywords-input');e.value=Array(101).fill('a').join(',');e.dispatchEvent(new Event('input',{bubbles:true}))").await?;
    wait(d,"({ok:document.getElementById('global-keywords-save').disabled&&document.getElementById('global-keywords-error').textContent.includes('100')})").await?;
    act(d, "document.getElementById('global-keywords-save').click()").await?;
    if writes(&s.peek()) != 7 {
        return Err("invalid keywords dispatched".into());
    }
    act(d,"const e=document.getElementById('global-keywords-input');e.value='alpha,  beta,🦊';e.dispatchEvent(new Event('input',{bubbles:true}))").await?;
    wait(
        d,
        "({ok:!document.getElementById('global-keywords-save').disabled})",
    )
    .await?;
    change(s, d, "global-keywords-save", None, 8, false).await?;
    wait(d,"({ok:document.getElementById('global-keywords-input').value==='alpha,  beta, 🦊'&&document.getElementById('global-keywords-save').disabled})").await?;
    act(d,"const e=document.getElementById('global-keywords-input');e.value='';e.dispatchEvent(new Event('input',{bubbles:true}))").await?;
    wait(
        d,
        "({ok:!document.getElementById('global-keywords-save').disabled})",
    )
    .await?;
    change(s, d, "global-keywords-save", None, 9, true).await?;
    wait(d,"({ok:document.getElementById('global-notifications-reload').textContent==='Check status'&&!document.getElementById('global-notifications-reload').disabled})").await?;
    check_status(s, d).await?; // unchanged canonical cannot settle uncertain clear
    wait(d,"({ok:document.getElementById('global-keywords-input').disabled&&document.getElementById('global-keywords-input').value===''} )").await?;
    fixture::apply_unconfirmed(&mut s.write());
    check_status(s, d).await?;
    wait(d,"({ok:!document.getElementById('global-keywords-input').disabled&&document.getElementById('global-keywords-input').value===''&&document.getElementById('global-keywords-save').disabled})").await?;
    let mut reached = Vec::new();
    for id in [
        "global-desktop-mode",
        "global-desktop-push",
        "global-mobile-push",
        "global-mobile-overrides",
        "global-thread-replies",
        "global-keywords-input",
        "global-keywords-save",
    ] {
        act(
            d,
            &format!("document.getElementById('{id}').scrollIntoView({{block:'center'}})"),
        )
        .await?;
        reached.push(wait(d,&format!("(()=>{{const e=document.getElementById('{id}'),r=e.getBoundingClientRect(),b=document.getElementById('global-notifications-scroll').getBoundingClientRect();return{{ok:r.top>=b.top&&r.bottom<=b.bottom&&document.elementFromPoint(r.left+r.width/2,r.top+r.height/2)===e,id:'{id}'}}}})()")).await?);
    }
    let final_layout = layout(d).await?;
    let shell = s.peek();
    let f = shell.global_notifications.fixture.as_ref().unwrap();
    if !f.valid
        || f.writes.len() != 9
        || shell.core.workspaces["T1"].notifications.channels != channels
        || shell.core.workspaces["T1"].notifications.global["unknown_global"] != json!({"keep":9})
    {
        return Err("request count/preserved state mismatch".into());
    }
    let patches: Vec<Value> = f
        .writes
        .iter()
        .map(|w| {
            w.get("prefs")
                .and_then(Value::as_str)
                .map(|s| serde_json::from_str(s).unwrap())
                .unwrap_or_else(|| json!({"name":w["name"],"value":w["value"]}))
        })
        .collect();
    let expected = vec![
        json!([{"name":"global_desktop","value":"everything"},{"name":"global_mobile","value":"everything"}]),
        json!({"name":"global_desktop_push_enabled","value":"false"}),
        json!([{"name":"global_mobile","value":"mentions_dms"}]),
        json!({"name":"global_mobile","value":"everything"}),
        json!({"name":"global_mobile","value":"nothing"}),
        json!({"name":"global_mobile","value":"everything"}),
        json!({"name":"threads_everything","value":"true"}),
        json!({"name":"global_keywords","value":"alpha, beta,🦊"}),
        json!({"name":"global_keywords","value":""}),
    ];
    if patches != expected {
        return Err(format!("wrong exact patches: {patches:?}"));
    }
    for (i, w) in f.writes.iter().enumerate() {
        let multi = matches!(i, 0 | 2);
        let reason = if matches!(i, 0 | 2 | 3) {
            "NotificationsChannelDefaults"
        } else if i >= 7 {
            "NotificationsKeywordsInput"
        } else {
            "NotificationsGlobalPrefCheckbox"
        };
        if w["global"] != "true"
            || w["_x_reason"] != reason
            || w.get("channel_id").is_some()
            || w.get("channel_ids").is_some()
            || if multi {
                w.get("sync").is_some()
            } else {
                w["sync"] != "false"
            }
        {
            return Err(format!("wrong request routing {i}: {w}"));
        }
    }
    Ok(
        json!({"ok":true,"noTransport":true,"openNoWrite":true,"canonicalInit":true,"localOnlyOverrides":true,"linkedIndependentMobile":true,"explicitKeywords":true,"dirtyNavigation":true,"invalidNoWrite":true,"nativeCaret":true,"completionFocus":true,"duplicateBlocked":true,"unknownReadOnly":true,"exactWire":true,"preservedOverrides":true,"reached":reached,"writes":9,"reads":f.reads,"layout":initial,"finalLayout":final_layout}),
    )
}
fn writes(s: &ShellState) -> usize {
    s.global_notifications
        .fixture
        .as_ref()
        .unwrap()
        .writes
        .len()
}
async fn click(d: &dioxus::desktop::DesktopContext, id: &str) -> Result<(), String> {
    act(d,&format!("const e=document.getElementById('{id}');e.scrollIntoView({{block:'center'}});e.focus();e.click()")).await
}
async fn change(
    mut s: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    id: &str,
    value: Option<&str>,
    count: usize,
    unknown: bool,
) -> Result<(), String> {
    if let Some(value) = value {
        act(d,&format!("const e=document.getElementById('{id}');e.scrollIntoView({{block:'center'}});e.focus();e.value='{value}';e.dispatchEvent(new Event('change',{{bubbles:true}}));e.dispatchEvent(new Event('change',{{bubbles:true}}))")).await?;
    } else {
        click(d, id).await?;
    }
    crate::slack_links_check::wait_state(s, |s| writes(s) == count)
        .await
        .map_err(|e| format!("{id}: {e}"))?;
    wait(d,"({ok:[...document.querySelectorAll('#global-notifications input,#global-notifications select,#global-notifications textarea')].every(e=>e.disabled)})").await?;
    // A second queued action during the held response cannot dispatch.
    act(
        d,
        &format!(
            "document.getElementById('{id}').dispatchEvent(new Event('change',{{bubbles:true}}))"
        ),
    )
    .await?;
    if unknown {
        fixture::complete_without_apply(&mut s.write());
    } else {
        fixture::complete(&mut s.write(), false);
        let focus = if id == "global-keywords-save" {
            "global-keywords-input"
        } else {
            id
        };
        wait(d,&format!("({{ok:document.getElementById('global-desktop-mode').disabled===false&&document.activeElement===document.getElementById('{focus}'),active:document.activeElement?.id}})")).await?;
    }
    if writes(&s.peek()) != count {
        return Err("duplicate mutation".into());
    }
    Ok(())
}
async fn check_status(
    mut s: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<(), String> {
    click(d, "global-notifications-reload").await?;
    crate::slack_links_check::wait_state(s, |s| {
        s.global_notifications
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
async fn wait(d: &dioxus::desktop::DesktopContext, script: &str) -> Result<Value, String> {
    let observed = format!("(window.__globalNotificationsLastCheck=({script}))");
    match crate::slack_links_check::wait_dom(d, &observed).await {
        Ok(value) => Ok(value),
        Err(error) => {
            let diagnostics = crate::slack_links_check::wait_dom(d, r#"(()=>{const e=document.getElementById('global-keywords-input'),p=document.getElementById('global-notifications');return {ok:true,last:window.__globalNotificationsLastCheck,visit:p?.dataset.visit,active:document.activeElement?.id,keywords:e?{value:e.value,seed:e.dataset.seed,appliedSeed:e.dataset.appliedSeed,start:e.selectionStart,end:e.selectionEnd,disabled:e.disabled,edits:e.__keywordSeed?.edits,composing:e.__keywordSeed?.composing}:null,controls:[...document.querySelectorAll('#global-notifications select,#global-notifications input')].map(e=>({id:e.id,value:e.value,checked:e.checked,disabled:e.disabled}))};})()"#).await;
            Err(format!(
                "{error}; diagnostics={diagnostics:?}; predicate={script}"
            ))
        }
    }
}
async fn act(d: &dioxus::desktop::DesktopContext, script: &str) -> Result<(), String> {
    wait(d, &format!("(()=>{{{script};return{{ok:true}}}})()"))
        .await
        .map(|_| ())
}
async fn layout(d: &dioxus::desktop::DesktopContext) -> Result<Value, String> {
    act(
        d,
        "document.getElementById('global-notifications-scroll').scrollTop=0",
    )
    .await?;
    wait(d,r#"(()=>{const m=document.getElementById('shell-overlay-dialog'),p=document.getElementById('global-notifications-scroll'),mode=document.getElementById('global-desktop-mode');if(!m||!p||!mode)return{ok:false};const r=m.getBoundingClientRect(),b=p.getBoundingClientRect(),q=mode.getBoundingClientRect(),tabs=[...m.querySelectorAll('.settings-tab')].map(e=>e.getBoundingClientRect());return{ok:r.top>=0&&r.bottom<=innerHeight&&r.left>=0&&r.right<=innerWidth&&b.height>=64&&q.top>=b.top&&q.bottom<=b.bottom&&tabs.length===3&&tabs.every(t=>t.top===tabs[0].top),viewport:[innerWidth,innerHeight],dialog:[r.left,r.top,r.right,r.bottom],scroll:[b.left,b.top,b.right,b.bottom]};})()"#).await
}
