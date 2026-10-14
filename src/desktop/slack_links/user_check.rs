//! Real rich-link handlers, isolated prepared requests, and native layout captures.
use super::user_fixture as fixture;
use crate::state::{ChannelOpen, ShellState};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::time::Duration;

pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !fixture::enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let receipt =
            match tokio::time::timeout(Duration::from_secs(120), run(state, &desktop)).await {
                Ok(Ok(receipt)) => receipt,
                Ok(Err(error)) => json!({"ok":false,"error":error}),
                Err(_) => json!({"ok":false,"error":"User-link check timed out"}),
            };
        eprintln!("user-links-check: {receipt}");
    });
}
async fn eval(script: String) -> Result<Value, String> {
    let mut evaluation = dioxus::document::eval(&script);
    let result = tokio::time::timeout(Duration::from_secs(4), evaluation.recv::<Value>()).await;
    let _ = evaluation.send(());
    match result {
        Ok(Ok(value)) if value["ok"] == true => Ok(value),
        Ok(Ok(value)) => Err(format!("User link DOM: {value}")),
        _ => Err("User link DOM timed out".into()),
    }
}
async fn click(label: &str) -> Result<(), String> {
    eval(format!(r#"const e=[...document.querySelectorAll('a')].find(e=>e.textContent==={});if(e){{e.scrollIntoView({{block:'nearest'}});e.click();}}dioxus.send({{ok:!!e}});await dioxus.recv();"#,json!(label))).await.map(|_|())
}
async fn wait(
    state: Signal<ShellState>,
    predicate: impl Fn(&ShellState) -> bool,
) -> Result<(), String> {
    for _ in 0..200 {
        if predicate(&state.peek()) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("User link state did not settle".into())
}
async fn frames() -> Result<(), String> {
    eval("requestAnimationFrame(()=>requestAnimationFrame(()=>dioxus.send({ok:true})));await dioxus.recv();".into()).await.map(|_|())
}
async fn home(mut state: Signal<ShellState>) -> Result<(), String> {
    {
        let mut shell = state.write();
        shell.close_profile();
        shell.toast = None;
        let index = shell
            .channels
            .iter()
            .position(|row| row.id == "C2")
            .ok_or("Origin channel missing")?;
        shell.select_channel(index, ChannelOpen::Global);
        if shell.core.composer.text != fixture::DRAFT {
            return Err("Outgoing draft changed".into());
        }
    }
    crate::bootstrap::refresh_selected_channel(state).await;
    frames().await
}
async fn ready() -> Result<(std::path::PathBuf, String, String), String> {
    let (dir, nonce) = fixture::directory()?;
    let name = std::env::var("SUPER_PLATINUM_FIXTURE").map_err(|_| "Missing alias")?;
    let expected = json!({"version":1,"nonce":nonce,"pid":std::process::id(),"fixture":name});
    let path = dir.join("ready.json");
    if path.exists() {
        return Err("Readiness marker must be new".into());
    }
    eprintln!("user-links-waiting: {expected}");
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
async fn capture(
    desktop: &dioxus::desktop::DesktopContext,
    dir: &std::path::Path,
    stage: &str,
    profile: bool,
    captures: &mut serde_json::Map<String, Value>,
    geometry: &mut Vec<Value>,
) -> Result<(), String> {
    frames().await?;
    let mut measured=eval(format!(r#"const rect=e=>{{if(!e)return null;const r=e.getBoundingClientRect();return {{x:r.x,y:r.y,width:r.width,height:r.height}}}};const p=document.querySelector('.profile-pane'),c=document.getElementById('channel-composer'),b=p?.querySelector('[aria-label="Close profile"]');const r=b?.getBoundingClientRect();const hit=r?b.contains(document.elementFromPoint(r.x+r.width/2,r.y+r.height/2)):null;dioxus.send({{ok:!!c&&!!p==={}&&(!p||hit)&&document.documentElement.scrollWidth<=innerWidth+1,viewport:[innerWidth,innerHeight],documentWidth:document.documentElement.scrollWidth,profileVisible:!!p,profile:rect(p),composer:rect(c),closeProfileHit:hit}});await dioxus.recv();"#,profile)).await?;
    measured["stage"] = json!(stage);
    geometry.push(measured);
    eprintln!(
        "user-links-capture: {}",
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
        "user-links-capture: {}",
        json!({"stage":stage,"edge":"after"})
    );
    Ok(())
}
fn ensure(condition: bool, message: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let (dir, nonce, name) = ready().await?;
    ensure(fixture::owner(&state.peek()), "Fixture owner missing")?;
    let mut captures = serde_json::Map::new();
    let mut geometry = Vec::new();
    capture(
        desktop,
        &dir,
        "initial",
        false,
        &mut captures,
        &mut geometry,
    )
    .await?;
    for label in ["Unknown person", "App person"] {
        click(label).await?;
        wait(state, |s| s.toast.is_some()).await?;
        state.write().toast = None;
    }
    ensure(
        fixture::attempts()?.is_empty(),
        "Unsupported user made a request",
    )?;
    click("Open cached person").await?;
    wait(state, |s| {
        s.core.active_channel.as_deref() == Some("DCACHED")
            && s.profile_user.as_deref() == Some("UCACHED")
    })
    .await?;
    ensure(fixture::attempts()?.is_empty(), "Cached DM made a request")?;
    capture(desktop, &dir, "cached", true, &mut captures, &mut geometry).await?;
    // Exercise the existing Message button: it must still close this profile.
    eval(r#"const b=document.querySelector('.profile-pane .profile-action-message');if(b)b.click();dioxus.send({ok:!!b});await dioxus.recv();"#.into()).await?;
    wait(state, |s| {
        s.profile_user.is_none() && s.core.active_channel.as_deref() == Some("DCACHED")
    })
    .await?;
    home(state).await?;
    click("Open new person").await?;
    wait(state, |s| {
        s.core.active_channel.as_deref() == Some("DCREATED")
            && s.profile_user.as_deref() == Some("UCREATED")
    })
    .await?;
    ensure(fixture::attempts()?.len() == 1, "Created DM request count")?;
    capture(desktop, &dir, "created", true, &mut captures, &mut geometry).await?;
    home(state).await?;
    fixture::hold(Some("UDELAY"))?;
    click("Open delayed person").await?;
    wait(state, |s| {
        s.profile_dm_opening.as_deref() == Some("UDELAY")
            && fixture::attempts().is_ok_and(|a| a.len() == 2)
    })
    .await?;
    click("Open delayed person").await?;
    frames().await?;
    ensure(fixture::attempts()?.len() == 2, "Duplicate link resent")?;
    capture(
        desktop,
        &dir,
        "pending",
        false,
        &mut captures,
        &mut geometry,
    )
    .await?;
    {
        let mut shell = state.write();
        let index = shell
            .channels
            .iter()
            .position(|c| c.id == "C1")
            .ok_or("Stale target missing")?;
        shell.select_channel(index, ChannelOpen::Global);
    }
    fixture::hold(None)?;
    wait(state, |_| {
        fixture::completed().is_ok_and(|count| count == 2)
    })
    .await?;
    ensure(
        state.peek().core.active_channel.as_deref() == Some("C1")
            && !state.peek().core.workspaces["T1"]
                .channels
                .contains_key("DDELAY"),
        "Stale response navigated",
    )?;
    capture(desktop, &dir, "stale", false, &mut captures, &mut geometry).await?;
    home(state).await?;
    click("Open wrong response").await?;
    wait(state, |s| {
        s.toast.is_some() && s.profile_dm_opening.is_none()
    })
    .await?;
    ensure(
        state.peek().core.active_channel.as_deref() == Some("C2")
            && !state.peek().core.workspaces["T1"]
                .channels
                .contains_key("DWRONG"),
        "Wrong recipient committed",
    )?;
    state.write().toast = None;
    click("Open unavailable person").await?;
    wait(state, |s| {
        s.toast.is_some() && s.profile_dm_opening.is_none()
    })
    .await?;
    ensure(
        state.peek().core.active_channel.as_deref() == Some("C2")
            && fixture::attempts()?.len() == 4,
        "Failed request navigated",
    )?;
    capture(desktop, &dir, "error", false, &mut captures, &mut geometry).await?;
    state.write().toast = None;
    fixture::hold(Some("UROTATE"))?;
    click("Open rotating person").await?;
    wait(state, |s| {
        s.profile_dm_opening.as_deref() == Some("UROTATE")
            && fixture::attempts().is_ok_and(|a| a.len() == 5)
    })
    .await?;
    state
        .write()
        .core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut("T1")
        .unwrap()
        .token = "fake-user-links-rotated".into();
    fixture::hold(None)?;
    wait(state, |s| s.profile_dm_opening.is_none()).await?;
    ensure(
        state.peek().core.active_channel.as_deref() == Some("C2")
            && !state.peek().core.workspaces["T1"]
                .channels
                .contains_key("DROTATE"),
        "Rotated credentials admitted stale response",
    )?;
    capture(
        desktop,
        &dir,
        "rotation",
        false,
        &mut captures,
        &mut geometry,
    )
    .await?;
    click("Open rotating person").await?;
    wait(state, |s| {
        s.core.active_channel.as_deref() == Some("DROTATE")
            && s.profile_user.as_deref() == Some("UROTATE")
    })
    .await?;
    home(state).await?;
    capture(
        desktop,
        &dir,
        "restored",
        false,
        &mut captures,
        &mut geometry,
    )
    .await?;
    ensure(fixture::owner(&state.peek()), "Fixture owner lost")?;
    let attempts = fixture::attempts()?;
    ensure(attempts.len() == 6, "Unexpected request count")?;
    ensure(
        captures.len() == fixture::STAGES.len(),
        "Missing screenshot stage",
    )?;
    let viewport = geometry.last().ok_or("No geometry")?["viewport"].clone();
    Ok(
        json!({"ok":true,"nonce":nonce,"pid":std::process::id(),"fixture":name,"noTransport":true,"syntheticSession":true,"captures":captures,"geometry":geometry,"viewport":viewport,"preparedRequests":attempts.len(),"attempts":attempts,
        "checks":{"cachedNoRequest":true,"duplicateNoRequest":true,"staleNoInsert":true,"wrongRecipientRefused":true,"errorStayedPut":true,"rotationNoInsert":true,"retryOpened":true,"outgoingDraftPreserved":true,"unsupportedNoRequest":true,"messageClosesProfile":true}}),
    )
}
