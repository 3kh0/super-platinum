//! Local native preference and audio-element proof; never a Slack mutation.
use super::*;
use serde_json::{Value, json};
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !fixture::requested()
        || std::env::var("SUPER_PLATINUM_NOTIFICATION_PRESENTATION_CHECK").as_deref() != Ok("1")
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let receipt = match run(state, &desktop).await {
            Ok(v) => v,
            Err(e) => json!({"ok":false,"error":e}),
        };
        eprintln!("notification-presentation-check: {receipt}");
    });
}
async fn run(
    mut s: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    wait(d,"({ok:document.getElementById('presentation-desktop-sound')?.value==='been_tree.mp3'&&!document.getElementById('presentation-desktop-sound').disabled&&document.getElementById('presentation-priority-sound')?.value==='none'})").await?;
    if s.peek().core.transport.is_some()
        || s.peek().core.session.is_some()
        || writes(&s.peek()) != 0
    {
        return Err("connected fixture or opening write".into());
    }
    let channels = s.peek().core.workspaces["T1"]
        .notifications
        .channels
        .clone();
    let initial = layout(d).await?;
    click(d, "presentation-desktop-sound-preview").await?;
    let preview=wait(d,"(()=>{const p=window.__notificationSoundPreview;return {ok:p?.state==='ended'&&p.name==='been_tree.mp3'&&p.duration>0&&p.currentTime>0,preview:p};})()").await?;
    if writes(&s.peek()) != 0 {
        return Err("preview wrote preference".into());
    }
    change(s, d, "presentation-desktop-sound", Some("b2.mp3"), 1, false).await?;
    wait(d,"(()=>{const p=window.__notificationSoundPreview;return {ok:p?.name==='b2.mp3'&&['playing','ended'].includes(p.state),preview:p};})()").await?;
    change(s, d, "presentation-include-preview", None, 2, false).await?;
    change(
        s,
        d,
        "presentation-priority-sound",
        Some("animal_stick.mp3"),
        3,
        false,
    )
    .await?;
    // Removing known VIPs disables the paid-only choice; delayed events cannot send.
    let vips = s.peek().core.workspaces["T1"].vip_users.clone();
    s.write()
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .vip_users
        .clear();
    wait(
        d,
        "({ok:document.getElementById('presentation-priority-sound').disabled})",
    )
    .await?;
    act(d,"const e=document.getElementById('presentation-priority-sound');e.value='b2.mp3';e.dispatchEvent(new Event('change',{bubbles:true}))").await?;
    wait(d,"({ok:document.getElementById('presentation-priority-sound').disabled&&document.getElementById('presentation-priority-sound').value==='animal_stick.mp3'&&window.__notificationSoundPreview?.name==='animal_stick.mp3'})").await?;
    s.write().core.workspaces.get_mut("T1").unwrap().vip_users = vips;
    wait(
        d,
        "({ok:!document.getElementById('presentation-priority-sound').disabled})",
    )
    .await?;
    if writes(&s.peek()) != 3 {
        return Err("unavailable VIP choice sent".into());
    }
    change(s, d, "presentation-mute-sounds", None, 4, false).await?;
    wait(d,"({ok:!document.getElementById('presentation-desktop-sound')&&!document.getElementById('presentation-priority-sound')&&document.getElementById('presentation-mute-sounds').checked})").await?;
    change(s, d, "presentation-mute-sounds", None, 5, false).await?;
    wait(d,"({ok:document.getElementById('presentation-desktop-sound')?.value==='b2.mp3'&&document.getElementById('presentation-priority-sound')?.value==='animal_stick.mp3'})").await?;
    change(s, d, "presentation-desktop-sound", Some("none"), 6, true).await?;
    check_status(s, d).await?;
    wait(d,"({ok:document.getElementById('presentation-desktop-sound').disabled&&document.getElementById('presentation-desktop-sound').value==='b2.mp3'})").await?;
    fixture::apply_unconfirmed(&mut s.write());
    check_status(s, d).await?;
    wait(d,"({ok:!document.getElementById('presentation-desktop-sound').disabled&&document.getElementById('presentation-desktop-sound').value==='none'&&document.getElementById('presentation-desktop-sound-preview').disabled})").await?;
    if cfg!(target_os = "macos") {
        wait(d,"({ok:!document.getElementById('presentation-dock-bounce').checked&&!document.getElementById('presentation-dock-duration')})").await?;
        change(s, d, "presentation-dock-bounce", None, 7, false).await?;
        wait(
            d,
            "({ok:document.getElementById('presentation-dock-duration')?.value==='short'})",
        )
        .await?;
        change(s, d, "presentation-dock-duration", Some("long"), 8, false).await?;
        change(s, d, "presentation-dock-bounce", None, 9, false).await?;
        wait(d,"({ok:!document.getElementById('presentation-dock-bounce').checked&&!document.getElementById('presentation-dock-duration')})").await?;
        change(s, d, "presentation-dock-bounce", None, 10, true).await?;
        check_status(s, d).await?;
        wait(d,"({ok:document.getElementById('presentation-dock-bounce').disabled&&!document.getElementById('presentation-dock-bounce').checked})").await?;
        fixture::apply_unconfirmed(&mut s.write());
        check_status(s, d).await?;
        wait(d,"({ok:!document.getElementById('presentation-dock-bounce').disabled&&document.getElementById('presentation-dock-bounce').checked&&document.getElementById('presentation-dock-duration')?.value==='long'})").await?;
    }
    // Legacy Nothing is preserved and hides sounds/disables message previews.
    s.write()
        .presentation_notifications
        .fixture
        .as_mut()
        .unwrap()
        .raw["all_notifications_prefs"]["global"]["global_desktop"] = json!("nothing");
    click(d, "presentation-notifications-reload").await?;
    wait(d,"({ok:!document.getElementById('presentation-desktop-sound')&&document.getElementById('presentation-include-preview').disabled})").await?;
    if cfg!(target_os = "macos") {
        wait(d,"({ok:document.getElementById('presentation-dock-bounce').disabled&&document.getElementById('presentation-dock-duration').disabled})").await?;
        act(d,"const e=document.getElementById('presentation-dock-duration');e.value='short';e.dispatchEvent(new Event('change',{bubbles:true}))").await?;
        wait(d,"({ok:document.getElementById('presentation-dock-duration').disabled&&document.getElementById('presentation-dock-duration').value==='long'})").await?;
    }
    s.write()
        .presentation_notifications
        .fixture
        .as_mut()
        .unwrap()
        .raw["all_notifications_prefs"]["global"]["global_desktop"] = json!("everything");
    click(d, "presentation-notifications-reload").await?;
    wait(d,"({ok:document.getElementById('presentation-desktop-sound')?.value==='none'&&!document.getElementById('presentation-include-preview').disabled})").await?;
    click(d, "notification-page-defaults").await?;
    wait(
        d,
        "({ok:!document.getElementById('presentation-notifications')})",
    )
    .await?;
    click(d, "notification-page-sounds").await?;
    wait(d,"({ok:document.getElementById('presentation-desktop-sound')?.value==='none'&&!document.getElementById('presentation-desktop-sound').disabled})").await?;
    let mut reached = Vec::new();
    for id in [
        "presentation-desktop-sound",
        "presentation-include-preview",
        "presentation-mute-sounds",
        "presentation-priority-sound",
        "presentation-priority-sound-preview",
    ] {
        act(d,&format!("const e=document.getElementById('{id}');e.scrollIntoView({{block:'center'}});e.focus()")).await?;
        reached.push(wait(d,&format!("(()=>{{const e=document.getElementById('{id}'),r=e.getBoundingClientRect(),b=document.getElementById('presentation-notifications-scroll').getBoundingClientRect();return{{ok:document.activeElement===e&&r.top>=b.top&&r.bottom<=b.bottom&&document.elementFromPoint(r.left+r.width/2,r.top+r.height/2)===e,id:'{id}'}}}})()")).await?);
    }
    if cfg!(target_os = "macos") {
        for id in ["presentation-dock-bounce", "presentation-dock-duration"] {
            act(d,&format!("const e=document.getElementById('{id}');e.scrollIntoView({{block:'center'}});e.focus()")).await?;
            reached.push(wait(d,&format!("(()=>{{const e=document.getElementById('{id}'),r=e.getBoundingClientRect(),b=document.getElementById('presentation-notifications-scroll').getBoundingClientRect();return{{ok:document.activeElement===e&&!e.disabled&&r.top>=b.top&&r.bottom<=b.bottom&&document.elementFromPoint(r.left+r.width/2,r.top+r.height/2)===e,id:'{id}'}}}})()")).await?);
        }
    }
    let final_layout = layout(d).await?;
    let shell = s.peek();
    let f = shell.presentation_notifications.fixture.as_ref().unwrap();
    if !f.valid
        || f.writes.len() != if cfg!(target_os = "macos") { 10 } else { 6 }
        || f.reads != if cfg!(target_os = "macos") { 16 } else { 11 }
        || shell.core.workspaces["T1"].notifications.channels != channels
        || shell.core.workspaces["T1"].notifications.global["unknown_global"] != json!({"keep":9})
        || shell.core.workspaces["T1"].notifications.user["unknown_pref"] != json!({"keep":7})
    {
        return Err("request count/preserved prefs mismatch".into());
    }
    let mut expected = vec![
        ("desktop_sound", "b2.mp3"),
        ("no_text_in_notifications", "true"),
        ("priority_desktop_sound", "animal_stick.mp3"),
        ("mute_sounds", "true"),
        ("mute_sounds", "false"),
        ("desktop_sound", "none"),
    ];
    if cfg!(target_os = "macos") {
        expected.extend([
            ("mac_ssb_bounce", "short"),
            ("mac_ssb_bounce", "long"),
            ("mac_ssb_bounce", ""),
            ("mac_ssb_bounce", "long"),
        ]);
    }
    for (i, (w, (name, value))) in f.writes.iter().zip(expected).enumerate() {
        let reason = match name {
            "mute_sounds" | "mac_ssb_bounce" => "prefs-api/setUserPrefByApi",
            "priority_desktop_sound" => "select-new-vip-notification-sound",
            _ => "",
        };
        if w["_x_reason"] != reason
            || w["name"] != name
            || w["value"] != value
            || w.get("channel_id").is_some()
            || w.get("channel_ids").is_some()
        {
            return Err(format!("wrong scalar wire {i}: {w}"));
        }
        if !matches!(name, "mute_sounds" | "mac_ssb_bounce")
            && (w["global"] != "true" || w["sync"] != "false")
        {
            return Err(format!("wrong global flags {i}: {w}"));
        }
        if matches!(name, "mute_sounds" | "mac_ssb_bounce")
            && (w.get("global").is_some() || w.get("sync").is_some())
        {
            return Err("mute sounds used global adapter".into());
        }
    }
    Ok(
        json!({"ok":true,"noTransport":true,"openNoWrite":true,"canonicalInit":true,"conditionalControls":true,"previewNoWrite":true,"selectionPreview":true,"duplicateBlocked":true,"unknownReadOnlyCheck":true,"exactWire":true,"unknownPreserved":true,"completionFocus":true,"tabNavigation":true,"preview":preview,"reached":reached,"dockBounce":cfg!(target_os="macos"),"writes":f.writes.len(),"reads":f.reads,"layout":initial,"finalLayout":final_layout}),
    )
}
fn writes(s: &ShellState) -> usize {
    s.presentation_notifications
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
    wait(d,"({ok:[...document.querySelectorAll('#presentation-notifications input,#presentation-notifications select')].every(e=>e.disabled)})").await?;
    act(
        d,
        &format!(
            "document.getElementById('{id}').dispatchEvent(new Event('change',{{bubbles:true}}))"
        ),
    )
    .await?;
    if unknown {
        fixture::complete_without_apply(&mut s.write());
        wait(d,"({ok:document.getElementById('presentation-notifications-reload').textContent==='Check status'&&!document.getElementById('presentation-notifications-reload').disabled})").await?;
    } else {
        fixture::complete(&mut s.write(), false);
        wait(d,&format!("({{ok:document.getElementById('{id}')?.disabled===false&&document.activeElement===document.getElementById('{id}'),active:document.activeElement?.id}})")).await?;
    }
    if writes(&s.peek()) != count {
        return Err("duplicate write".into());
    }
    Ok(())
}
async fn check_status(
    mut s: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<(), String> {
    click(d, "presentation-notifications-reload").await?;
    crate::slack_links_check::wait_state(s, |s| {
        s.presentation_notifications
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
async fn act(d: &dioxus::desktop::DesktopContext, script: &str) -> Result<(), String> {
    wait(d, &format!("(()=>{{{script};return{{ok:true}}}})()"))
        .await
        .map(|_| ())
}
async fn wait(d: &dioxus::desktop::DesktopContext, script: &str) -> Result<Value, String> {
    let observed = format!("(window.__presentationLastCheck=({script}))");
    match crate::slack_links_check::wait_dom(d, &observed).await {
        Ok(v) => Ok(v),
        Err(e) => {
            let diagnostic=crate::slack_links_check::wait_dom(d,"({ok:true,last:window.__presentationLastCheck,preview:window.__notificationSoundPreview,active:document.activeElement?.id,controls:[...document.querySelectorAll('#presentation-notifications input,#presentation-notifications select')].map(e=>({id:e.id,value:e.value,checked:e.checked,disabled:e.disabled}))})").await;
            Err(format!(
                "{e}; diagnostic={diagnostic:?}; predicate={script}"
            ))
        }
    }
}
async fn layout(d: &dioxus::desktop::DesktopContext) -> Result<Value, String> {
    act(
        d,
        "document.getElementById('presentation-notifications-scroll').scrollTop=0",
    )
    .await?;
    wait(d,r#"(()=>{const m=document.getElementById('shell-overlay-dialog'),p=document.getElementById('presentation-notifications-scroll'),e=document.getElementById('presentation-desktop-sound');if(!m||!p||!e)return{ok:false};const r=m.getBoundingClientRect(),b=p.getBoundingClientRect(),q=e.getBoundingClientRect(),nav=[...m.querySelectorAll('.notification-pages button')].map(e=>e.getBoundingClientRect());return{ok:r.top>=0&&r.bottom<=innerHeight&&r.left>=0&&r.right<=innerWidth&&b.height>=64&&q.top>=b.top&&q.bottom<=b.bottom&&nav.length===3&&nav.every(t=>t.top===nav[0].top&&t.left>=r.left&&t.right<=r.right),viewport:[innerWidth,innerHeight],dialog:[r.left,r.top,r.right,r.bottom],scroll:[b.left,b.top,b.right,b.bottom]};})()"#).await
}
