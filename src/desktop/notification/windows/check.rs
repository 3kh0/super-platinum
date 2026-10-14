//! One disconnected OS preview. Runner captures actual desktop pixels during the
//! twelve-second observation window; API acceptance does not assert visibility.
use super::*;
use dioxus::prelude::*;
pub(super) fn click_enabled() -> bool {
    identity::fixture()
        && std::env::var("SUPER_PLATINUM_NOTIFICATION_CLICK_CHECK").as_deref() == Ok("1")
}
pub(super) fn click_nonce() -> Option<String> {
    std::env::var("SUPER_PLATINUM_NOTIFICATION_CLICK_NONCE")
        .ok()
        .filter(|nonce| {
            nonce.len() == 32
                && nonce
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        })
}

pub(crate) async fn run(state: Signal<crate::state::ShellState>) {
    if cold::active() {
        cold::run(state).await;
        return;
    }
    if super::route_check::enabled() {
        super::route_check::run(state).await;
        return;
    }
    if !identity::fixture()
        || std::env::args().any(|arg| arg == "--super-platinum-toast-fixture-activated")
    {
        return;
    }
    tokio::time::sleep(Duration::from_secs(2)).await;
    let disconnected = state.peek().core.transport.is_none() && state.peek().core.session.is_none();
    if !disconnected {
        eprintln!(
            "notification-check: {}",
            json!({"ok":false,"error":"Notification check requires a disconnected fixture"})
        );
        return;
    }
    let handled_before = activation::handled()["count"].as_u64().unwrap_or(0);
    let activated_before = activation::count();
    let submitted = if click_enabled() {
        use dioxus::desktop::tao::platform::windows::WindowExtWindows;
        let desktop = dioxus::desktop::window();
        desktop.window.set_minimized(true);
        tokio::time::sleep(Duration::from_millis(250)).await;
        let nonce = click_nonce();
        let marker = format!(
            "Super Platinum activation check {}",
            nonce.as_deref().unwrap_or("missing-nonce")
        );
        let hwnd = desktop.window.hwnd();
        let minimized = unsafe {
            ::windows::Win32::UI::WindowsAndMessaging::IsIconic(::windows::Win32::Foundation::HWND(
                hwnd as *mut _,
            ))
        }
        .as_bool();
        let foreground =
            unsafe { ::windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() };
        eprintln!(
            "notification-click-armed: {}",
            json!({"ok":minimized && nonce.is_some(),
            "noTransport":disconnected,"noSession":state.peek().core.session.is_none(),"pid":std::process::id(),
            "hwnd":hwnd,"nonce":nonce,"app_id":app_id(),"appName":"Super Platinum Fixture","title":"Super Platinum",
            "body":marker,"activationCountBefore":activated_before,"handledCountBefore":handled_before,
            "minimized":minimized,"foregroundHwndBefore":foreground.0 as isize})
        );
        submit(
            DesktopNotification {
                title: "Super Platinum".into(),
                body: marker,
                sound: None,
                destination: None,
            },
            true,
        )
        .await
    } else {
        super::super::show_preview_receipt().await
    };
    match &submitted {
        Ok(receipt) => eprintln!("notification-check-submitted: {receipt}"),
        Err(error) => eprintln!(
            "notification-check-submitted: {}",
            json!({"submitted":false,"error":error})
        ),
    }
    tokio::time::sleep(Duration::from_secs(12)).await;
    let previews = cleanup_preview().await;
    let registration = if previews.is_ok() {
        tokio::time::timeout(
            Duration::from_secs(5),
            tokio::task::spawn_blocking(identity::cleanup_fixture),
        )
        .await
        .map_err(|_| "Fixture registration cleanup timed out; completion is unknown.".to_owned())
        .and_then(|result| result.map_err(|error| error.to_string()))
        .and_then(|result| result)
    } else {
        Err("Fixture registration retained because preview cleanup is unconfirmed.".into())
    };
    let accepted = submitted.as_ref().is_ok_and(|receipt| {
        receipt["submitted"] == true
            && receipt["failed_hresult"].is_null()
            && !FAILURES
                .lock()
                .unwrap()
                .iter()
                .any(|failure| failure["tag"] == receipt["tag"])
    });
    let history = submitted
        .as_ref()
        .is_ok_and(|receipt| receipt["history_present"] == true);
    let handling = activation::handled();
    let click_verified = handling["count"]
        .as_u64()
        .is_some_and(|count| count == handled_before + 1)
        && activation::count() == activated_before + 1
        && handling["last"]["targetHwnd"] == handling["last"]["foregroundHwnd"];
    eprintln!(
        "notification-check: {}",
        json!({"ok":accepted && history && previews.is_ok() && registration.is_ok() && (!click_enabled() || click_verified),
        "noTransport":disconnected,"submissionAccepted":accepted,"historyConfirmed":history,
        "previewRemoved":previews.is_ok(),"fixtureRegistrationCleanup":registration.is_ok(),
        "submission":submitted.ok(),"cleanup":previews.as_ref().ok(),"cleanupError":previews.err(),
        "registration":registration.as_ref().ok(),"registrationError":registration.err(),
        "diagnostics":diagnostics(),"visibleBannerVerified":false,"clickCheck":click_enabled(),"warmActivationVerified":click_enabled() && click_verified})
    );
}
