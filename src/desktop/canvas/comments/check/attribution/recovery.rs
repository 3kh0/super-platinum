//! Explicit recovery retains hydrated native data; no fixture restore path.
use super::*;
use data::recovery as recovery_data;
mod capture;
pub(super) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if std::env::var(recovery_data::FLAG).as_deref() != Ok("1") {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = match run(state, &desktop).await {
            Ok(value) => value,
            Err(error) => json!({"ok":false,"error":error,"proof":proof(&state.peek())}),
        };
        let _ = tokio::task::spawn_blocking(move || {
            eprintln!("canvas-editor-recovery-check: {result}")
        })
        .await;
    });
}
fn proof(shell: &ShellState) -> Value {
    let mut value = data::proof(shell);
    value["recovery"] = recovery_data::snapshot(shell);
    value
}
fn members(shell: &ShellState) -> usize {
    recovery_data::snapshot(shell)["requests"]
        .as_array()
        .map(|r| r.iter().filter(|r| r["kind"] == "member").count())
        .unwrap_or(0)
}
async fn issued(state: Signal<ShellState>, count: usize) -> Result<(), String> {
    for _ in 0..100 {
        if members(&state.peek()) == count {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err(format!(
        "Expected original member request {count}: {}",
        recovery_data::snapshot(&state.peek())
    ))
}
async fn retry(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    mode: recovery_data::Mode,
    count: usize,
) -> Result<(), String> {
    recovery_data::configure(&state.peek(), mode)
        .map_err(|_| "Cannot configure closed recovery")?;
    // Only a deliberately held response guarantees that a second click tests
    // an in-flight duplicate. An immediate failure may already have settled.
    let duplicate = matches!(mode, recovery_data::Mode::Held);
    eval(
        desktop,
        &format!(
            r#"(()=>{{const b=document.querySelector('.canvas-attribution-retry');if(!b||b.disabled)return {{ok:false}};b.scrollIntoView({{block:'center'}});b.click();if({duplicate})b.click();return {{ok:true}};}})()"#
        ),
    )
    .await?;
    issued(state, count).await
}
async fn profile(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<(), String> {
    click(desktop, ".canvas-attribution-name").await?;
    wait(
        desktop,
        "!!document.querySelector('#canvas-comment-profile .profile-close')",
    )
    .await?;
    let change = recovery_data::hydrate(&mut state.clone().write())
        .map_err(|_| "Cannot retain synthetic profile hydration")?;
    wait(
        desktop,
        &format!(
            "document.querySelector('#canvas-comment-profile .profile-title')?.textContent==={}",
            change["after"]["profile"]["title"]
        ),
    )
    .await
}
async fn close(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    click(desktop, "#canvas-comment-profile .profile-close").await?;
    wait(
        desktop,
        "!document.getElementById('canvas-comment-profile')",
    )
    .await?;
    ready(desktop, "unavailable").await
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    ready(desktop, "hidden").await?;
    recovery_data::seed(state).await?;
    let mut stages = Vec::new();
    capture::stage(state, desktop, "stream", &mut stages).await?;
    click(desktop, ".canvas-comment-open").await?;
    ready(desktop, "known").await?;
    super::capture::saved(state, desktop).await?;
    eval(desktop, include_str!("input.js")).await?;
    super::capture::saved(state, desktop).await?;
    eval(desktop, include_str!("remember.js")).await?;
    capture::stage(state, desktop, "draft", &mut stages).await?;
    eval(desktop,r#"(()=>{const e=document.querySelector('.canvas-attribution-name');if(!e)return {ok:false};e.scrollIntoView({block:'center'});const r=e.getBoundingClientRect();e.dispatchEvent(new MouseEvent('mouseenter',{bubbles:false,clientX:r.x+r.width/2,clientY:r.y+r.height/2}));return {ok:true};})()"#).await?;
    wait(
        desktop,
        "!!document.querySelector('.profile-hover .profile-name-button')",
    )
    .await?;
    let original = {
        let shell = state.peek();
        let hover = shell
            .profile_hover
            .as_ref()
            .ok_or("Original hover missing")?;
        crate::canvas::comments::profile_hover_token(&shell, hover)
            .ok_or("Original hover witness missing")?
    };
    click(desktop, ".profile-hover .profile-name-button").await?;
    wait(
        desktop,
        "!!document.querySelector('#canvas-comment-profile .profile-close')",
    )
    .await?;
    recovery_data::hydrate(&mut state.write()).map_err(|_| "Cannot retain original hydration")?;
    wait(desktop,"document.querySelector('#canvas-comment-profile .profile-title')?.textContent==='Synthetic loaded profile'").await?;
    capture::stage(state, desktop, "profile", &mut stages).await?;
    close(desktop).await?;
    capture::stage(state, desktop, "closed", &mut stages).await?;
    if members(&state.peek()) != 0 {
        return Err("Passive hydration/Close issued forced member read".into());
    }
    retry(state, desktop, recovery_data::Mode::Held, 1).await?;
    ready(desktop, "pending").await?;
    capture::stage(state, desktop, "recovering", &mut stages).await?;
    recovery_data::release(&state.peek())
        .map_err(|_| "Release original successful repair failed")?;
    ready(desktop, "known").await?;
    wait(
        desktop,
        "document.querySelector('.canvas-attribution-name')?.textContent==='Editor Recovered'",
    )
    .await?;
    let before = {
        let s = state.peek();
        json!({"profile":s.profile_user,"navigation":s.shortcut_generation})
    };
    let current = original.current(&state.peek());
    original.open(state);
    recovery_data::stale(&state.peek(), before, current)
        .map_err(|_| "Old hover callback acquired repaired source")?;
    capture::stage(state, desktop, "recovered", &mut stages).await?;
    profile(state, desktop).await?;
    capture::stage(state, desktop, "reopened", &mut stages).await?;
    close(desktop).await?;
    capture::stage(state, desktop, "returned", &mut stages).await?;
    retry(state, desktop, recovery_data::Mode::Offline, 2).await?;
    ready(desktop, "unavailable").await?;
    capture::stage(state, desktop, "offline", &mut stages).await?;
    retry(state, desktop, recovery_data::Mode::RateLimited, 3).await?;
    ready(desktop, "unavailable").await?;
    wait(
        desktop,
        "!document.querySelector('.canvas-attribution-retry')",
    )
    .await?;
    capture::stage(state, desktop, "cooldown", &mut stages).await?;
    let mut expired = false;
    for _ in 0..220 {
        if members(&state.peek()) != 3 {
            return Err("Cooldown issued automatic member request".into());
        }
        if eval_raw(
            desktop,
            "({ok:!!document.querySelector('.canvas-attribution-retry')})",
        )
        .await?["ok"]
            == true
        {
            expired = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    if !expired {
        return Err("Original real15s cooldown never expired".into());
    }
    recovery_data::expired(&state.peek())
        .map_err(|_| "Original cooldown elapsed/count proof failed")?;
    capture::stage(state, desktop, "expired", &mut stages).await?;
    retry(state, desktop, recovery_data::Mode::Held, 4).await?;
    ready(desktop, "pending").await?;
    capture::stage(state, desktop, "retirement-held", &mut stages).await?;
    refresh(
        state,
        desktop,
        Some(data::OTHER),
        true,
        false,
        false,
        "known",
    )
    .await?;
    for _ in 0..100 {
        if recovery_data::held_closed(&state.peek()) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    if !recovery_data::held_closed(&state.peek()) {
        return Err("Held repair receiver did not retire".into());
    }
    if recovery_data::release(&state.peek()).map_err(|_| "Retired release failed")?["delivered"]
        != false
    {
        return Err("Retired original repair delivered".into());
    }
    super::capture::saved(state, desktop).await?;
    capture::stage(state, desktop, "retired", &mut stages).await?;
    refresh(
        state,
        desktop,
        Some(data::EDITOR),
        true,
        false,
        false,
        "unavailable",
    )
    .await?;
    super::capture::saved(state, desktop).await?;
    if members(&state.peek()) != 4 {
        return Err("Passive read automatically forced member recovery".into());
    }
    capture::stage(state, desktop, "passive-unavailable", &mut stages).await?;
    retry(state, desktop, recovery_data::Mode::Success, 5).await?;
    ready(desktop, "known").await?;
    capture::stage(state, desktop, "final-recovered", &mut stages).await?;
    let proof = proof(&state.peek());
    if proof["requests"].as_array().map(Vec::len) != Some(4) || members(&state.peek()) != 5 {
        return Err("Final original request counts differ".into());
    }
    Ok(
        json!({"ok":true,"noSession":false,"noTransport":false,"isolation":data::isolation(&state.peek()),"requestCount":4,"memberRequestCount":5,
        "readOnly":true,"draftPreserved":true,"staleRefused":true,"explicitRecovery":true,"proof":proof,"layout":stages.last().unwrap()["layout"],"stages":stages}),
    )
}
