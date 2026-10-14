//! Direct WinRT delivery. OS acceptance, history and visible banners are separate
//! outcomes; a timeout or Failed event never schedules a replay.
use super::{DesktopNotification, windows_toast_xml};
use ::windows::{
    Data::Xml::Dom::XmlDocument,
    Foundation::TypedEventHandler,
    UI::Notifications::{
        NotificationSetting, ToastFailedEventArgs, ToastNotification, ToastNotificationManager,
    },
    core::HSTRING,
};
use serde_json::{Value, json};
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};
mod activation;
pub(crate) mod check;
pub(super) mod cold;
mod identity;
mod route_check;
pub(crate) use activation::{session_ready, use_activation};
pub(crate) use cold::recovery_mode as cold_recovery_mode;
pub(crate) use identity::{app_id, initialize, prepare_startup};

static SERIAL: AtomicU64 = AtomicU64::new(1);
static HELD: Mutex<std::collections::VecDeque<ToastNotification>> =
    Mutex::new(std::collections::VecDeque::new());
static PREVIEWS: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());
static LAST: Mutex<Option<Value>> = Mutex::new(None);
static FAILURES: Mutex<std::collections::VecDeque<Value>> =
    Mutex::new(std::collections::VecDeque::new());
static PROCESS_GROUP: OnceLock<String> = OnceLock::new();
// Cleanup cannot overtake an admitted Show, even after its caller times out.
static DELIVERY: DeliveryGate = DeliveryGate::new();
struct DeliveryGate {
    serial: Mutex<()>,
    closing: std::sync::atomic::AtomicBool,
}
impl DeliveryGate {
    const fn new() -> Self {
        Self {
            serial: Mutex::new(()),
            closing: std::sync::atomic::AtomicBool::new(false),
        }
    }
    fn enter(&self) -> Result<std::sync::MutexGuard<'_, ()>, String> {
        let guard = self.serial.lock().unwrap();
        if self.closing.load(Ordering::Acquire) {
            Err("Notification fixture is closing; submission was not started.".into())
        } else {
            Ok(guard)
        }
    }
    fn close(&self) {
        self.closing.store(true, Ordering::Release);
    }
}

pub(super) struct Apartment;
impl Apartment {
    pub(super) fn new() -> Result<Self, String> {
        use ::windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};
        unsafe { RoInitialize(RO_INIT_MULTITHREADED) }
            .map_err(|error| format!("Initialize WinRT: {error}"))?;
        Ok(Self)
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { ::windows::Win32::System::WinRT::RoUninitialize() }
    }
}

pub(crate) async fn submit(
    notification: DesktopNotification,
    preview: bool,
) -> Result<Value, String> {
    // No retry: a bounded caller wait cannot cancel a Show already accepted by OS.
    let work = tokio::task::spawn_blocking(move || submit_blocking(&notification, preview));
    tokio::time::timeout(Duration::from_secs(10), work)
        .await
        .map_err(|_| {
            "Windows notification submission timed out; outcome unknown and no retry was scheduled."
                .to_owned()
        })?
        .map_err(|error| format!("Windows notification worker stopped: {error}"))?
}

fn submit_blocking(notification: &DesktopNotification, preview: bool) -> Result<Value, String> {
    let _delivery = DELIVERY.enter()?;
    identity::ready()?;
    let _apartment = Apartment::new()?;
    let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(app_id()))
        .map_err(|error| format!("Create toast notifier: {error}"))?;
    let setting = notifier.Setting();
    let setting_result = setting.as_ref().copied().map_err(|error| error.code().0);
    if let Err(error) = &setting {
        if !setting_allows_submission(setting_result) {
            return Err(format!("Read toast setting: {error}"));
        }
    }
    let sequence = SERIAL.fetch_add(1, Ordering::Relaxed);
    let tag = format!("{:08x}{:08x}", std::process::id(), sequence as u32);
    let group = PROCESS_GROUP
        .get_or_init(|| {
            let time = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default();
            format!("{:016x}", time.as_nanos() as u64)
        })
        .clone();
    let (tag, group) = cold::preview_identity()?.unwrap_or((tag, group));
    let mut receipt = json!({"notification_preview":preview,"platform":"windows","app_id":app_id(),"identity":identity::receipt(),
        "setting":setting_result.map(setting_name).unwrap_or("unknown"),
        "setting_error":setting_result.err().map(|code|format!("0x{:08X}",code as u32)),
        "setting_before_show":setting_result.map(setting_name).unwrap_or("unknown"),
        "setting_error_before_show":setting_result.err().map(|code|format!("0x{:08X}",code as u32)),
        "submitted":false,"history_present":false,"history_error":null,
        "tag":tag,"group":group,"silent":!notification.sound.as_deref().is_some_and(super::sounds::is_bundled),"preview":preview,
        "activation":"foreground-app","content_route":notification.destination.is_some(),"visible_banner_verified":false});
    if !setting_allows_submission(setting_result) {
        *LAST.lock().unwrap() = Some(receipt.clone());
        return Ok(receipt);
    }
    let xml = XmlDocument::new().map_err(|error| format!("Create toast document: {error}"))?;
    xml.LoadXml(&HSTRING::from(cold::xml(
        windows_toast_xml(notification),
        preview,
    )?))
    .map_err(|error| format!("Load toast document: {error}"))?;
    let toast = ToastNotification::CreateToastNotification(&xml)
        .map_err(|error| format!("Create toast: {error}"))?;
    toast
        .SetTag(&HSTRING::from(&tag))
        .map_err(|error| format!("Set toast tag: {error}"))?;
    toast
        .SetGroup(&HSTRING::from(&group))
        .map_err(|error| format!("Set toast group: {error}"))?;
    let failure = Arc::new(Mutex::new(None::<String>));
    let observed = failure.clone();
    let failed_tag = tag.clone();
    toast
        .Failed(
            &TypedEventHandler::<ToastNotification, ToastFailedEventArgs>::new(move |_, args| {
                let code = args.as_ref().and_then(|args| args.ErrorCode().ok());
                let message = code.map_or_else(
                    || "unknown HRESULT".to_owned(),
                    |code| format!("0x{:08X}", code.0 as u32),
                );
                *observed.lock().unwrap() = Some(message.clone());
                let mut events = FAILURES.lock().unwrap();
                events.push_back(json!({"tag":failed_tag,"hresult":message}));
                while events.len() > 64 {
                    events.pop_front();
                }
                eprintln!("super-platinum: Windows toast Failed ({message}); no retry scheduled");
                Ok(())
            }),
        )
        .map_err(|error| format!("Subscribe toast failure: {error}"))?;
    // Reserve exact cleanup ownership before the possibly ambiguous OS call.
    if preview {
        PREVIEWS.lock().unwrap().push((tag.clone(), group.clone()));
    }
    cold::reserve(&tag, &group)?;
    notifier
        .Show(&toast)
        .map_err(|error| format!("Submit Windows toast: {error}"))?;
    receipt["submitted"] = json!(true);
    // Identity-less apps may have no setting until their first Show. Observe
    // again, without sending a dummy notification or replaying this submission.
    match notifier.Setting() {
        Ok(setting) => {
            receipt["setting"] = json!(setting_name(setting));
            receipt["setting_error"] = Value::Null;
        }
        Err(error) => {
            receipt["setting"] = json!("unknown");
            receipt["setting_error"] = json!(format!("0x{:08X}", error.code().0 as u32));
        }
    }
    // Keep a bounded callback lifetime; this does not poll or resubmit messages.
    let mut held = HELD.lock().unwrap();
    held.push_back(toast);
    while held.len() > 64 {
        held.pop_front();
    }
    drop(held);
    if preview {
        let deadline = Instant::now() + Duration::from_millis(1500);
        loop {
            match history_contains(&tag, &group) {
                Ok(present) => receipt["history_present"] = json!(present),
                Err(error) => {
                    receipt["history_error"] = json!(error);
                    break;
                }
            }
            if receipt["history_present"] == true
                || failure.lock().unwrap().is_some()
                || Instant::now() >= deadline
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    receipt["failed_hresult"] = json!(*failure.lock().unwrap());
    *LAST.lock().unwrap() = Some(receipt.clone());
    Ok(receipt)
}

fn setting_allows_submission(setting: Result<NotificationSetting, i32>) -> bool {
    // Microsoft's ToastNotifierCompat documents that unpackaged apps must send
    // their first notification before Setting works. Keep ERROR_NOT_FOUND as
    // unknown, never synthesize Enabled or bypass an explicit disabled setting.
    // https://github.com/CommunityToolkit/WindowsCommunityToolkit/blob/main/Microsoft.Toolkit.Uwp.Notifications/Toasts/Compat/ToastNotifierCompat.cs
    matches!(setting, Ok(NotificationSetting::Enabled) | Err(-2147023728))
}
fn setting_name(setting: NotificationSetting) -> &'static str {
    match setting {
        NotificationSetting::Enabled => "enabled",
        NotificationSetting::DisabledForApplication => "disabled_for_application",
        NotificationSetting::DisabledForUser => "disabled_for_user",
        NotificationSetting::DisabledByGroupPolicy => "disabled_by_group_policy",
        NotificationSetting::DisabledByManifest => "disabled_by_manifest",
        _ => "unknown",
    }
}
fn history_contains(tag: &str, group: &str) -> Result<bool, String> {
    let history = ToastNotificationManager::History().map_err(|error| error.to_string())?;
    let entries = history
        .GetHistoryWithId(&HSTRING::from(app_id()))
        .map_err(|error| error.to_string())?;
    for entry in entries {
        if entry.Tag().map_err(|error| error.to_string())? == tag
            && entry.Group().map_err(|error| error.to_string())? == group
        {
            return Ok(true);
        }
    }
    Ok(false)
}
pub(crate) fn diagnostics() -> Value {
    json!({"last_submission":LAST.lock().unwrap().clone(),"failed_events":FAILURES.lock().unwrap().clone(),"activations":activation::count(),"activation_handled":activation::handled(),"identity":identity::receipt()})
}
/// Removes only this process's explicitly submitted previews. Never Clear history.
/// Fixture registration cleanup is separately gated and cannot affect production.
pub(crate) async fn cleanup_preview() -> Result<Value, String> {
    if identity::fixture() {
        DELIVERY.close();
    }
    let work = tokio::task::spawn_blocking(|| {
        let _delivery = DELIVERY.serial.lock().unwrap();
        let _apartment = Apartment::new()?;
        let history = ToastNotificationManager::History().map_err(|error| error.to_string())?;
        let mut previews = PREVIEWS.lock().unwrap();
        let mut removed = 0;
        while let Some((tag, group)) = previews.last() {
            history
                .RemoveGroupedTagWithId(
                    &HSTRING::from(tag),
                    &HSTRING::from(group),
                    &HSTRING::from(app_id()),
                )
                .map_err(|error| format!("Remove own preview: {error}"))?;
            if history_contains(tag, group)? {
                return Err("Windows retained the preview after removal.".into());
            }
            previews.pop();
            removed += 1;
        }
        Ok(
            json!({"removed_previews":removed,"remaining_owned_previews":previews.len(),"app_id":app_id()}),
        )
    });
    tokio::time::timeout(Duration::from_secs(5), work)
        .await
        .map_err(|_| {
            "Preview cleanup is still pending; registration retained and cleanup unconfirmed."
                .to_owned()
        })?
        .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn first_unregistered_setting_allows_one_show_but_disabled_or_other_errors_do_not() {
        assert!(setting_allows_submission(Ok(NotificationSetting::Enabled)));
        assert!(setting_allows_submission(Err(0x80070490u32 as i32)));
        for setting in [
            NotificationSetting::DisabledForApplication,
            NotificationSetting::DisabledForUser,
            NotificationSetting::DisabledByGroupPolicy,
            NotificationSetting::DisabledByManifest,
        ] {
            assert!(!setting_allows_submission(Ok(setting)));
        }
        assert!(!setting_allows_submission(Err(0x80070005u32 as i32)));
    }
    #[test]
    fn fixture_close_rejects_queued_submission_and_waits_for_admitted_worker() {
        let gate = Arc::new(DeliveryGate::new());
        let active = gate.enter().unwrap();
        let queued = gate.clone();
        let worker = std::thread::spawn(move || queued.enter().is_err());
        gate.close();
        assert!(gate.serial.try_lock().is_err());
        drop(active);
        assert!(worker.join().unwrap());
        let cleanup = gate.serial.lock().unwrap();
        drop(cleanup);
        assert!(gate.enter().is_err());
    }
}
