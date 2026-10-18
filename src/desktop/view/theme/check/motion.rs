//! Opt-in disconnected native motion evidence; never changes OS preferences.
use crate::state::{ConnectionStatus, ShellState};
use dioxus::prelude::*;
use serde_json::{Value, json};

const PREFIX: &str = "profile-loading";

pub(crate) fn preference() -> Option<bool> {
    requested_preference(
        std::env::var("SUPER_PLATINUM_AGENT").ok().as_deref(),
        std::env::var("SUPER_PLATINUM_FIXTURE").ok().as_deref(),
        std::env::var("SUPER_PLATINUM_MOTION_REDUCE_CHECK").as_deref() == Ok("1"),
        std::env::var("SUPER_PLATINUM_MOTION_NORMAL_CHECK").as_deref() == Ok("1"),
    )
}

fn requested_preference(
    agent: Option<&str>,
    fixture: Option<&str>,
    reduce: bool,
    normal: bool,
) -> Option<bool> {
    if agent != Some("1") || !matches!(fixture, Some("profile-loading" | "profile-loading-narrow"))
    {
        return None;
    }
    match (reduce, normal) {
        (true, false) => Some(true),
        (false, true) => Some(false),
        _ => None,
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn browser_flag() -> Option<&'static str> {
    preference().map(|reduce| {
        if reduce {
            "--force-prefers-reduced-motion"
        } else {
            "--force-prefers-no-reduced-motion"
        }
    })
}

fn isolated(state: &ShellState, reduce: bool) -> Result<(), String> {
    super::isolated(state)?;
    if preference() != Some(reduce) {
        return Err("Motion fixture preference or identity changed".into());
    }
    Ok(())
}

pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    let Some(reduce) = preference() else {
        return;
    };
    if isolated(&state.peek(), reduce).is_err() {
        return;
    }
    let nonce = format!("{}-{:?}", std::process::id(), std::time::SystemTime::now());
    dioxus::core::spawn_forever(async move {
        let result = run(state, &desktop, reduce, &nonce).await;
        // Release only this check's marker. A failed run is never promoted by cleanup.
        let removed = super::eval(&desktop, &format!("(()=>{{const p=window.__superPlatinumMotionProbe;if(p?.owner==={}){{delete window.__superPlatinumMotionProbe;}}return {{ok:!('__superPlatinumMotionProbe' in window)}};}})()", json!(nonce))).await;
        let receipt = match (result, removed) {
            (Ok(mut value), Ok(cleanup)) if cleanup["ok"] == true => {
                value["probeRemoved"] = json!(true);
                value
            }
            (result, cleanup) => {
                json!({"ok":false,"error":format!("Motion check or cleanup failed: {result:?}; {cleanup:?}")})
            }
        };
        eprintln!("{PREFIX}-check: {receipt}");
    });
}

fn phase(state: &ShellState, index: usize) -> bool {
    match index {
        0 => {
            !state.loading
                && state.profile_user.as_deref() == Some("U1")
                && state
                    .core
                    .profile_pane
                    .as_ref()
                    .is_some_and(|pane| pane.user == "U1" && pane.loading && pane.error.is_none())
        }
        1 => state.loading,
        2 => !state.loading && state.connection.status == ConnectionStatus::Connecting,
        3 => !state.loading && state.connection.status == ConnectionStatus::NoNetwork,
        _ => false,
    }
}

async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    reduce: bool,
    nonce: &str,
) -> Result<Value, String> {
    if !cfg!(target_os = "windows") {
        return Err("Motion fixture launch override requires Windows WebView2".into());
    }
    isolated(&state.peek(), reduce)?;
    if !phase(&state.peek(), 0) {
        return Err("Expected explicit initial profile-loading fixture".into());
    }
    super::wait(
        desktop,
        "({ok:!!document.querySelector('.profile-avatar-loading')})",
    )
    .await?;
    let install = super::eval(
        desktop,
        &include_str!("motion/sample.js").replace("__MOTION_NONCE__", &json!(nonce).to_string()),
    )
    .await?;
    if install["ok"] != true {
        return Err("Owned motion probe already exists".into());
    }
    let fixture = std::env::var("SUPER_PLATINUM_FIXTURE").map_err(|_| "Fixture missing")?;
    let viewport = if fixture.ends_with("-narrow") {
        [668, 380]
    } else {
        [1280, 780]
    };
    let original_connection = {
        let shell = state.peek();
        (shell.connection.status, shell.connection.routable)
    };
    let mut observations = Vec::new();
    for (index, (name, selector, size, duration, label)) in [
        ("profile", ".profile-avatar-loading", 16, "0.8s", ""),
        (
            "workspace",
            ".spinner",
            26,
            "0.8s",
            "Opening your workspace…",
        ),
        (
            "connecting",
            ".rail-connection-spinner",
            12,
            "0.9s",
            "Connecting…",
        ),
        (
            "no-network",
            ".rail-connection-spinner",
            12,
            "0.9s",
            "Waiting for network",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        isolated(&state.peek(), reduce)?;
        if index > 0 {
            let mut shell = state.write();
            shell.loading = index == 1;
            shell.profile_user = None;
            shell.connection.status = if index == 3 {
                ConnectionStatus::NoNetwork
            } else {
                ConnectionStatus::Connecting
            };
            shell.connection.routable = index != 3;
        }
        super::wait(
            desktop,
            &format!("({{ok:!!document.querySelector({})}})", json!(selector)),
        )
        .await?;
        isolated(&state.peek(), reduce)?;
        let config = json!({"name":name,"selector":selector,"size":size,"duration":duration,"label":label,"reduce":reduce,"viewport":viewport});
        let begin = super::eval(
            desktop,
            &format!("window.__superPlatinumMotionProbe.begin({config})"),
        )
        .await?;
        if begin["ok"] != true {
            return Err(format!("Motion state did not initialize: {begin}"));
        }
        for _ in 0..8 {
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            isolated(&state.peek(), reduce)?;
            if !phase(&state.peek(), index) {
                return Err("Motion fixture state changed while sampling".into());
            }
            let sample = super::eval(desktop, "window.__superPlatinumMotionProbe.sample()").await?;
            if sample["ok"] != true {
                return Err(format!("Native motion sample failed: {sample}"));
            }
        }
        let result = super::eval(desktop, "window.__superPlatinumMotionProbe.finish()").await?;
        if result["ok"] != true {
            return Err(format!("Native motion evidence failed: {result}"));
        }
        observations.push(result);
        super::stage(state, PREFIX, name, index).await?;
        isolated(&state.peek(), reduce)?;
        if !phase(&state.peek(), index) {
            return Err("Motion fixture changed during capture".into());
        }
    }
    {
        let mut shell = state.write();
        shell.loading = false;
        shell.profile_user = Some("U1".into());
        shell.connection.status = original_connection.0;
        shell.connection.routable = original_connection.1;
    }
    super::wait(
        desktop,
        "({ok:!!document.querySelector('.profile-avatar-loading')})",
    )
    .await?;
    isolated(&state.peek(), reduce)?;
    let layout = super::eval(
        desktop,
        "({ok:true,viewport:[innerWidth,innerHeight],userAgent:navigator.userAgent})",
    )
    .await?;
    if layout["viewport"] != json!(viewport) {
        return Err("Final native viewport changed".into());
    }
    Ok(
        json!({"ok":true,"noSession":true,"noTransport":true,"fixture":fixture,"reduce":reduce,
        "motionVerified":true,"temporalVerified":true,"profileLoading":true,"states":4,"samplesPerState":8,
        "observations":observations,"layout":layout}),
    )
}

#[cfg(test)]
mod tests {
    use super::requested_preference;

    #[test]
    fn motion_override_requires_exact_fixture_agent_and_one_preference() {
        for fixture in ["profile-loading", "profile-loading-narrow"] {
            assert_eq!(
                requested_preference(Some("1"), Some(fixture), true, false),
                Some(true)
            );
            assert_eq!(
                requested_preference(Some("1"), Some(fixture), false, true),
                Some(false)
            );
            assert_eq!(
                requested_preference(Some("1"), Some(fixture), true, true),
                None
            );
            assert_eq!(
                requested_preference(Some("1"), Some(fixture), false, false),
                None
            );
            assert_eq!(requested_preference(None, Some(fixture), true, false), None);
            assert_eq!(
                requested_preference(Some("true"), Some(fixture), true, false),
                None
            );
        }
        for fixture in [None, Some("profile-pane"), Some("profile-loading-other")] {
            assert_eq!(requested_preference(Some("1"), fixture, true, false), None);
        }
    }
}
