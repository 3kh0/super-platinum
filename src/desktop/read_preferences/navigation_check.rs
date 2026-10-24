//! Native opening, actual visibility service and shared mark admission, disconnected.
use super::{navigation_fixture as fixture, opening, renderer};
use crate::state::ShellState;
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::time::Duration;
use super_platinum_core::{
    domain::PendingScrollTarget,
    state::{ReadMode, ReadPreferences},
};
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !fixture::enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let receipt =
            match tokio::time::timeout(Duration::from_secs(120), run(state, &desktop)).await {
                Ok(Ok(v)) => v,
                Ok(Err(e)) => json!({"ok":false,"error":e}),
                Err(_) => json!({"ok":false,"error":"Read navigation timed out"}),
            };
        eprintln!("read-navigation-check: {receipt}");
    });
}
fn ensure(ok: bool, message: &str) -> Result<(), String> {
    if ok { Ok(()) } else { Err(message.into()) }
}
async fn eval(script: String) -> Result<Value, String> {
    let mut evaluation = dioxus::document::eval(&script);
    let result = tokio::time::timeout(Duration::from_secs(4), evaluation.recv::<Value>()).await;
    let _ = evaluation.send(());
    match result {
        Ok(Ok(value)) if value["ok"] == true => Ok(value),
        Ok(Ok(value)) => Err(format!("Read navigation DOM: {value}")),
        _ => Err("Read navigation DOM timed out".into()),
    }
}
async fn frames() -> Result<(), String> {
    eval("requestAnimationFrame(()=>requestAnimationFrame(()=>dioxus.send({ok:true})));await dioxus.recv();".into()).await.map(|_|())
}
async fn ready() -> Result<(std::path::PathBuf, String, String), String> {
    let (dir, nonce) = fixture::directory()?;
    let name = std::env::var("SUPER_PLATINUM_FIXTURE").map_err(|_| "Missing alias")?;
    let expected = json!({"version":1,"nonce":nonce,"pid":std::process::id(),"fixture":name});
    let path = dir.join("ready.json");
    if path.exists() {
        return Err("Readiness marker must be new".into());
    }
    eprintln!("read-navigation-waiting: {expected}");
    for _ in 0..300 {
        let check = path.clone();
        let found = tokio::task::spawn_blocking(move || -> Result<Option<Value>, String> {
            let meta = match std::fs::symlink_metadata(&check) {
                Ok(meta) => meta,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(_) => return Err("Cannot inspect readiness".into()),
            };
            if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 1024 {
                return Err("Invalid readiness file".into());
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if meta.file_attributes() & 0x400 != 0 {
                    return Err("Readiness reparse point refused".into());
                }
            }
            serde_json::from_slice(&std::fs::read(check).map_err(|_| "Cannot read readiness")?)
                .map(Some)
                .map_err(|_| "Invalid readiness JSON".into())
        })
        .await
        .map_err(|_| "Readiness task failed")??;
        if let Some(found) = found {
            if found != expected {
                return Err("Readiness owner mismatch".into());
            }
            return Ok((dir, nonce, name));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err("Readiness timed out".into())
}

async fn settle(state: Signal<ShellState>) -> Result<Value, String> {
    frames().await?;
    for _ in 0..150 {
        let v=eval("const s=window.superPlatinumReadVisibility?.sample();dioxus.send({ok:!!s,sample:s,viewport:[innerWidth,innerHeight]});await dioxus.recv();".into()).await?;
        let s = state.peek();
        if s.read_presentation
            .observed(&s.timeline_scope(), &renderer::key(&s))
            .is_some_and(|seen| observed_sample_matches(seen, &v["sample"]))
        {
            return Ok(v);
        }
        drop(s);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Real visibility observer did not settle for current scope/key".into())
}
// Scope/key alone can recur across pending -> settled navigation in the same
// visit. Require the actual stable observer message to describe this DOM view.
fn observed_sample_matches(seen: &super::presentation::Observation, sample: &Value) -> bool {
    if !seen.stable {
        return false;
    }
    let mut sample = sample.clone();
    let Some(fields) = sample.as_object_mut() else {
        return false;
    };
    fields.insert("stable".into(), json!(true));
    serde_json::from_value::<super::presentation::Observation>(sample)
        .is_ok_and(|sample| &sample == seen)
}
async fn navigate(state: Signal<ShellState>) -> Result<(), String> {
    let (account, epoch, scope, pending) = {
        let shell = state.peek();
        (
            shell.core.active_account.clone(),
            shell.core.account_epoch,
            shell.timeline_scope(),
            shell.core.pending_scroll_to.clone(),
        )
    };
    ensure(pending.is_some(), "Opening target missing")?;
    let owned = |shell: &ShellState| {
        fixture::owner(shell)
            && shell.core.active_account == account
            && shell.core.account_epoch == epoch
            && shell.timeline_scope() == scope
            && (shell.core.pending_scroll_to == pending || shell.core.pending_scroll_to.is_none())
    };
    let expected = serde_json::to_string(&scope).map_err(|e| e.to_string())?;
    let mut mounted = false;
    for _ in 0..150 {
        ensure(
            owned(&state.peek()),
            "Opening owner or target replaced before mount",
        )?;
        let sample = eval(format!(
            "const root=document.getElementById('message-timeline');dioxus.send({{ok:true,ready:root?.dataset.timelineScope==={expected}&&typeof window.superPlatinumTimelineAnchor?.inputGeneration==='function',scope:root?.dataset.timelineScope??null}});await dioxus.recv();"
        )).await?;
        ensure(
            owned(&state.peek()),
            "Opening owner or target replaced during mount",
        )?;
        if sample["ready"] == true {
            mounted = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    ensure(mounted, "Current timeline anchor service did not mount")?;
    // Refresh once. The independent production timeline effect may finish the
    // same navigation after refresh returns; only its real consumption counts.
    crate::bootstrap::refresh_selected_channel(state).await;
    for _ in 0..150 {
        ensure(
            owned(&state.peek()),
            "Opening owner or target replaced during settlement",
        )?;
        if state.peek().core.pending_scroll_to.is_none() {
            settle(state).await?;
            ensure(
                owned(&state.peek()),
                "Opening owner retired during observation",
            )?;
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err(format!(
        "Production opening did not spend its anchor: scope={scope}, expected={pending:?}, current={:?}",
        state.peek().core.pending_scroll_to
    ))
}
async fn admission(
    state: Signal<ShellState>,
    stage: &str,
    opening: bool,
    allowed: bool,
) -> Result<(), String> {
    let before = fixture::attempts()?.len();
    fixture::arm(stage)?;
    crate::bootstrap::mark_channel_read(state, opening).await;
    fixture::disarm()?;
    ensure(
        fixture::attempts()?.len() == before + usize::from(allowed),
        "Unexpected shared read admission",
    )
}
async fn capture(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    dir: &std::path::Path,
    stage: &str,
    captures: &mut serde_json::Map<String, Value>,
    geometry: &mut Vec<Value>,
) -> Result<(), String> {
    let mut v = settle(state).await?;
    let bounds=eval("const t=document.getElementById('message-timeline'),c=document.getElementById('channel-composer');const rect=e=>{if(!e)return null;const r=e.getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height}};const r=t?.getBoundingClientRect();dioxus.send({ok:!!t&&!!c&&r.width>0&&r.height>0&&r.top>=0&&r.bottom<=innerHeight+1&&document.documentElement.scrollWidth<=innerWidth+1,timeline:rect(t),composer:rect(c),documentWidth:document.documentElement.scrollWidth});await dioxus.recv();".into()).await?;
    v["bounds"] = bounds;
    if stage == "cold-boundary" {
        v["syntheticPausedControls"] = eval("const p=document.getElementById('unread-opening-status'),ids=['unread-opening-continue','unread-opening-latest'];const controls=ids.map(id=>{const e=document.getElementById(id),r=e?.getBoundingClientRect();return {id,text:e?.textContent,hit:!!r&&r.width>0&&r.height>0&&r.top>=0&&r.bottom<=innerHeight&&e.contains(document.elementFromPoint(r.x+r.width/2,r.y+r.height/2))}});dioxus.send({ok:!!p&&controls.every(c=>c.hit),controls});await dioxus.recv();".into()).await?;
    }
    v["stage"] = json!(stage);
    v["canReadOpening"] = json!(renderer::can_read(&state.peek(), true, true));
    v["canReadOngoing"] = json!(renderer::can_read(&state.peek(), true, false));
    geometry.push(v);
    eprintln!(
        "read-navigation-capture: {}",
        json!({"stage":stage,"edge":"before"})
    );
    let (path, _, _) = crate::agent::capture::window(
        Some(
            dir.join(format!("{stage}.png"))
                .to_string_lossy()
                .into_owned(),
        ),
        desktop,
    )
    .await?;
    captures.insert(stage.into(), json!(path.to_string_lossy()));
    eprintln!(
        "read-navigation-capture: {}",
        json!({"stage":stage,"edge":"after"})
    );
    Ok(())
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let (dir, nonce, name) = ready().await?;
    ensure(fixture::owner(&state.peek()), "Disconnected owner missing")?;
    let mut captures = serde_json::Map::new();
    let mut geometry = Vec::new();
    let mut checks = serde_json::Map::new();
    for (stage, mode, allowed) in [
        ("left-off", ReadMode::LeftOffAndRead, true),
        ("newest-read", ReadMode::NewestAndRead, true),
        ("newest-unseen", ReadMode::NewestKeepUnseen, false),
    ] {
        fixture::reset(&mut state.write(), mode);
        navigate(state).await?;
        let sample = settle(state).await?;
        ensure(
            if stage == "left-off" {
                sample["sample"]["dividerVisible"] == true
            } else {
                sample["sample"]["atBottom"] == true && sample["sample"]["dividerVisible"] == false
            },
            "Opening geometry differs from read preference",
        )?;
        admission(state, stage, true, allowed).await?;
        capture(state, desktop, &dir, stage, &mut captures, &mut geometry).await?;
    }
    checks.insert("threeModes".into(), json!(true));
    // Match the complete production opening intent: an older target must retire
    // newest sticking before the main timeline effect sees the pending target.
    {
        let mut s = state.write();
        s.stick_to_bottom = false;
        s.core.pending_scroll_to = Some((
            "C2".into(),
            PendingScrollTarget::FirstUnreadAfter(fixture::ts(12)),
        ));
        s.reset_timeline_window();
    }
    navigate(state).await?;
    let sample = settle(state).await?;
    ensure(
        sample["sample"]["dividerVisible"] == true && sample["sample"]["ghost"] == false,
        &format!("Real unread divider was not visible: {sample}"),
    )?;
    admission(state, "divider", false, true).await?;
    ensure(
        !renderer::can_read(&state.peek(), false, false),
        "Unfocused read admitted",
    )?;
    {
        let mut s = state.write();
        s.channel_generation += 1;
        ensure(
            !renderer::can_read(&s, true, false),
            "Previous scope attention survived navigation",
        )?;
    }
    settle(state).await?;
    checks.insert("focusAndScopeGuards".into(), json!(true));
    capture(
        state,
        desktop,
        &dir,
        "divider",
        &mut captures,
        &mut geometry,
    )
    .await?;
    {
        let mut s = state.write();
        s.core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .messages
            .get_mut("C2")
            .unwrap()
            .last_read = Some(fixture::ts(13));
    }
    let sample = settle(state).await?;
    ensure(
        sample["sample"]["ghost"] == true,
        "Pinned old cursor was not ghost",
    )?;
    admission(state, "ghost", false, false).await?;
    capture(state, desktop, &dir, "ghost", &mut captures, &mut geometry).await?;
    checks.insert("ghostRefused".into(), json!(true));
    {
        let mut s = state.write();
        s.core.workspaces.get_mut("T1").unwrap().read_preferences =
            ReadPreferences::from_prefs(&ReadMode::NewestAndRead.patch());
        s.core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .messages
            .get_mut("C2")
            .unwrap()
            .history_refreshing = true;
    }
    settle(state).await?;
    admission(state, "loading", true, false).await?;
    capture(
        state,
        desktop,
        &dir,
        "loading",
        &mut captures,
        &mut geometry,
    )
    .await?;
    checks.insert("loadingRefused".into(), json!(true));
    // Save an actual partially clipped top row; no fabricated observer sample.
    fixture::reset(&mut state.write(), ReadMode::LeftOffAndRead);
    navigate(state).await?;
    {
        let mut s = state.write();
        let b = s
            .core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .messages
            .get_mut("C2")
            .unwrap();
        b.last_read = Some(fixture::ts(48));
        b.unread_count = 0;
        s.unread_anchor = None;
        s.refresh_from_core();
    }
    eval("const t=document.getElementById('message-timeline');t.scrollTop+=37.25;t.dispatchEvent(new Event('scroll'));dioxus.send({ok:true});await dioxus.recv();".into()).await?;
    settle(state).await?;
    let saved = {
        let s = state.peek();
        s.read_presentation
            .saved(&renderer::conversation(&s).ok_or("No conversation owner")?)
            .cloned()
            .ok_or("Actual scrollmark missing")?
    };
    {
        let mut s = state.write();
        s.channel_generation += 1;
        opening::apply(&mut s, false);
        ensure(
            opening::offset(&s, &PendingScrollTarget::Message(saved.ts.clone()))
                == Some(saved.offset),
            "Saved offset not selected",
        )?;
    }
    navigate(state).await?;
    let restored = settle(state).await?;
    ensure(
        restored["sample"]["firstId"].as_str() == Some(&saved.ts)
            && restored["sample"]["offset"]
                .as_f64()
                .is_some_and(|offset| (offset - saved.offset).abs() < 1.5),
        "Saved position was not restored",
    )?;
    capture(
        state,
        desktop,
        &dir,
        "saved-offset",
        &mut captures,
        &mut geometry,
    )
    .await?;
    checks.insert("savedOffset".into(), json!(true));
    {
        let mut s = state.write();
        opening::cancel(&mut s);
        s.stick_to_bottom = false;
        s.core.pending_scroll_to =
            Some(("C2".into(), PendingScrollTarget::Message(fixture::ts(30))));
        s.reset_timeline_window();
        ensure(
            opening::offset(&s, &PendingScrollTarget::Message(fixture::ts(30))).is_none(),
            "Saved offset leaked into explicit target",
        )?;
    }
    navigate(state).await?;
    eval(format!("const t=document.getElementById('message-timeline'),e=t.querySelector('[data-message-id=\"{}\"]'),r=e?.getBoundingClientRect(),b=t.getBoundingClientRect();dioxus.send({{ok:!!r&&r.bottom>b.top&&r.top<b.bottom}});await dioxus.recv();",fixture::ts(30))).await?;
    capture(
        state,
        desktop,
        &dir,
        "explicit-target",
        &mut captures,
        &mut geometry,
    )
    .await?;
    checks.insert("explicitWins".into(), json!(true));
    // Cold boundary is structurally absent; no fixture fetch is mistaken for live history.
    {
        let mut s = state.write();
        fixture::reset(&mut s, ReadMode::LeftOffAndRead);
        let b = s
            .core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .messages
            .get_mut("C2")
            .unwrap();
        b.messages.retain(|m| {
            m.ts.as_deref()
                .is_some_and(|ts| ts <= fixture::ts(12).as_str())
        });
        b.loaded = false;
        s.refresh_from_core();
        opening::apply(&mut s, true);
        ensure(
            opening::first_after(&s, &fixture::ts(12)).is_none(),
            "Cold unread boundary invented a row",
        )?;
        ensure(
            !renderer::can_read(&s, true, true),
            "Unloaded boundary acquired read authority",
        )?;
        s.core.pending_scroll_to = Some(("C2".into(), PendingScrollTarget::Latest));
        s.reset_timeline_window();
    }
    navigate(state).await?;
    {
        let mut s = state.write();
        s.read_presentation.unread_loading = Some((s.timeline_scope(), fixture::ts(12)));
        s.read_presentation.unread_paused = true;
    }
    settle(state).await?;
    capture(
        state,
        desktop,
        &dir,
        "cold-boundary",
        &mut captures,
        &mut geometry,
    )
    .await?;
    checks.insert("coldBoundaryRefused".into(), json!(true));
    fixture::reset(&mut state.write(), ReadMode::NewestAndRead);
    navigate(state).await?;
    crate::unread::change(state, "C2".into(), Some(fixture::ts(40)));
    for _ in 0..100 {
        if crate::unread::held(&state.peek()) && !crate::unread::busy(&state.peek(), "C2") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    ensure(
        crate::unread::held(&state.peek()),
        "Actual manual unread did not install a hold",
    )?;
    settle(state).await?;
    admission(state, "manual-hold", true, false).await?;
    capture(
        state,
        desktop,
        &dir,
        "manual-hold",
        &mut captures,
        &mut geometry,
    )
    .await?;
    checks.insert("manualHoldRefused".into(), json!(true));
    ensure(fixture::owner(&state.peek()), "Disconnected owner changed")?;
    let attempts = fixture::attempts()?;
    ensure(attempts.len() == 3, "Unexpected armed admission count")?;
    let viewport = geometry.last().ok_or("Missing capture")?["viewport"].clone();
    Ok(
        json!({"ok":true,"nonce":nonce,"pid":std::process::id(),"fixture":name,"noSession":true,"noTransport":true,"armedAdmissionOnly":true,"noAcknowledgement":true,"captures":captures,"geometry":geometry,"viewport":viewport,"preparedRequests":attempts.len(),"attempts":attempts,"checks":checks,"saved":{"ts":saved.ts,"offset":saved.offset}}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> Value {
        json!({"scope":"same-visit","key":"same-settled-key","visible":true,
            "dividerVisible":true,"ghost":false,"atBottom":false,"pending":false,
            "firstId":"10.000001","offset":-12.25})
    }
    fn seen() -> super::super::presentation::Observation {
        let mut value = sample();
        value["stable"] = json!(true);
        serde_json::from_value(value).unwrap()
    }
    #[test]
    fn current_stable_observer_must_match_actual_geometry_not_just_reused_key() {
        let observed = seen();
        assert!(observed_sample_matches(&observed, &sample()));
        for (field, value) in [
            ("dividerVisible", json!(false)),
            ("atBottom", json!(true)),
            ("firstId", json!("20.000001")),
            ("offset", json!(-14.25)),
            ("pending", json!(true)),
            ("scope", json!("next-visit")),
            ("ghost", json!(true)),
        ] {
            let mut stale = sample();
            stale[field] = value;
            assert!(!observed_sample_matches(&observed, &stale), "{field}");
        }
    }
    #[test]
    fn matching_geometry_without_debounced_attention_is_not_settled() {
        let mut observed = seen();
        observed.stable = false;
        assert!(!observed_sample_matches(&observed, &sample()));
        assert!(!observed_sample_matches(&seen(), &Value::Null));
    }
}
