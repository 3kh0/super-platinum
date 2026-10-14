//! A real unpackaged COM activation class. Callback threads enqueue foreground
//! work; only the Dioxus UI thread touches the native window.
use super::super::route::{Activation, Pending};
use super::*;
use ::windows::{
    Win32::{
        Foundation::{CLASS_E_NOAGGREGATION, E_INVALIDARG, E_POINTER},
        System::Com::{
            CLSCTX_LOCAL_SERVER, CoRegisterClassObject, CoRevokeClassObject, IClassFactory,
            IClassFactory_Impl, REGCLS_MULTIPLEUSE,
        },
        UI::Notifications::{
            INotificationActivationCallback, INotificationActivationCallback_Impl,
            NOTIFICATION_USER_INPUT_DATA,
        },
    },
    core::{BOOL, GUID, IUnknown, Interface, PCWSTR, Ref, implement},
};
use dioxus::prelude::{ReadableExt, Signal};
use std::ffi::c_void;
static ACTIVATIONS: AtomicU64 = AtomicU64::new(0);
static HANDLED: AtomicU64 = AtomicU64::new(0);
static CALLBACK_MS: AtomicU64 = AtomicU64::new(0);
static LAST_HANDLED: Mutex<Option<Value>> = Mutex::new(None);
static FOREGROUND: tokio::sync::Notify = tokio::sync::Notify::const_new();
static PENDING_ROUTE: Mutex<Pending> = Mutex::new(Pending::new());
static STOP: Mutex<Option<std::sync::mpsc::Sender<()>>> = Mutex::new(None);

#[implement(INotificationActivationCallback)]
struct Activator;
impl INotificationActivationCallback_Impl for Activator_Impl {
    fn Activate(
        &self,
        app_id: &PCWSTR,
        arguments: &PCWSTR,
        _data: *const NOTIFICATION_USER_INPUT_DATA,
        count: u32,
    ) -> ::windows::core::Result<()> {
        if app_id.is_null() || arguments.is_null() || count != 0 {
            return Err(E_INVALIDARG.into());
        }
        // COM supplies null-terminated strings; bound decoding before allocation.
        let id = unsafe { bounded_string(*app_id, 128) }
            .map_err(|_| ::windows::core::Error::from_hresult(E_INVALIDARG))?;
        let arguments = unsafe { bounded_string(*arguments, 2048) }
            .map_err(|_| ::windows::core::Error::from_hresult(E_INVALIDARG))?;
        let activation = if cold::active() {
            (id == identity::app_id() && cold::admits(&arguments)).then_some(Activation::Foreground)
        } else {
            admitted(&id, &arguments, identity::app_id())
        };
        let Some(activation) = activation else {
            return Err(E_INVALIDARG.into());
        };
        PENDING_ROUTE.lock().unwrap().push(activation);
        CALLBACK_MS.store(timestamp_ms(), Ordering::Release);
        ACTIVATIONS.fetch_add(1, Ordering::Relaxed);
        FOREGROUND.notify_one();
        Ok(())
    }
}
fn admitted(id: &str, arguments: &str, expected: &str) -> Option<Activation> {
    (id == expected)
        .then(|| Activation::parse(arguments))
        .flatten()
}
unsafe fn bounded_string(value: PCWSTR, limit: usize) -> Result<String, ()> {
    let mut length = 0;
    while length <= limit {
        if unsafe { *value.0.add(length) } == 0 {
            return String::from_utf16(unsafe { std::slice::from_raw_parts(value.0, length) })
                .map_err(|_| ());
        }
        length += 1;
    }
    Err(())
}

#[implement(IClassFactory)]
struct Factory;
impl IClassFactory_Impl for Factory_Impl {
    fn CreateInstance(
        &self,
        outer: Ref<'_, IUnknown>,
        iid: *const GUID,
        object: *mut *mut c_void,
    ) -> ::windows::core::Result<()> {
        if object.is_null() || iid.is_null() {
            return Err(E_POINTER.into());
        }
        unsafe {
            *object = std::ptr::null_mut();
        }
        if !outer.is_null() {
            return Err(CLASS_E_NOAGGREGATION.into());
        }
        let activator: INotificationActivationCallback = Activator.into();
        unsafe { activator.query(iid, object) }.ok()
    }
    fn LockServer(&self, _lock: BOOL) -> ::windows::core::Result<()> {
        Ok(())
    }
}
pub(super) struct Server {
    cookie: u32,
    stop: std::sync::mpsc::Receiver<()>,
}
pub(super) fn register() -> Result<Server, String> {
    let factory: IClassFactory = Factory.into();
    let cookie = unsafe {
        CoRegisterClassObject(
            &identity::clsid(),
            &factory,
            CLSCTX_LOCAL_SERVER,
            REGCLS_MULTIPLEUSE,
        )
    }
    .map_err(|error| format!("Register notification COM activator: {error}"))?;
    let (tx, rx) = std::sync::mpsc::channel();
    *STOP.lock().unwrap() = Some(tx);
    Ok(Server { cookie, stop: rx })
}
impl Server {
    pub(super) fn wait(self) {
        let _ = self.stop.recv();
        if let Err(error) = unsafe { CoRevokeClassObject(self.cookie) } {
            eprintln!("super-platinum: revoke notification COM activator failed: {error}");
        }
    }
}
pub(super) fn stop() {
    if let Some(tx) = STOP.lock().unwrap().take() {
        let _ = tx.send(());
    }
}
pub(super) fn count() -> u64 {
    ACTIVATIONS.load(Ordering::Relaxed)
}
pub(super) fn handled() -> Value {
    json!({"count":HANDLED.load(Ordering::Acquire),"last":LAST_HANDLED.lock().unwrap().clone()})
}
pub(super) fn timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
pub(super) fn fixture_callback(arguments: &str, reset_cold: bool) -> Result<(), String> {
    if !super::route_check::enabled() {
        return Err("Synthetic COM activation requires its isolated fixture.".into());
    }
    if reset_cold {
        *PENDING_ROUTE.lock().unwrap() = Pending::new();
    }
    let callback: INotificationActivationCallback = Activator.into();
    unsafe {
        callback.Activate(
            &HSTRING::from(identity::app_id()),
            &HSTRING::from(arguments),
            &[],
        )
    }
    .map_err(|error| error.to_string())
}
pub(crate) fn session_ready() {
    PENDING_ROUTE.lock().unwrap().ready();
    FOREGROUND.notify_one();
}
pub(crate) fn use_activation(
    state: Signal<crate::state::ShellState>,
    desktop: dioxus::desktop::DesktopContext,
) {
    dioxus::prelude::use_future(move || {
        let desktop = desktop.clone();
        async move {
            loop {
                // Notify retains one permit when activation precedes hook mount.
                FOREGROUND.notified().await;
                let activation = PENDING_ROUTE.lock().unwrap().take();
                let Some(activation) = activation else {
                    continue;
                };
                desktop.window.set_visible(true);
                desktop.window.set_minimized(false);
                desktop.window.set_focus();
                if let Activation::Destination(destination) = activation {
                    // Never switch accounts or wait for a future login to make
                    // an old notification eligible. Slack-link async guards
                    // own all work after this synchronous stable-owner check.
                    let owned = destination.current(&state.peek().core);
                    if owned {
                        crate::slack_links::open(state, &destination.link());
                    }
                }
                use dioxus::desktop::tao::platform::windows::WindowExtWindows;
                let target = desktop.window.hwnd();
                let foreground =
                    unsafe { ::windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() };
                let handled = HANDLED.fetch_add(1, Ordering::AcqRel) + 1;
                let receipt = json!({"handled":handled,"targetHwnd":target,"foregroundHwnd":foreground.0 as isize});
                *LAST_HANDLED.lock().unwrap() = Some(receipt.clone());
                if super::check::click_enabled() || cold::child() {
                    let handled_ms = timestamp_ms();
                    let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
                    let mut foreground = foreground;
                    while foreground.0 as isize != target && tokio::time::Instant::now() < deadline
                    {
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        foreground = unsafe {
                            ::windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow()
                        };
                    }
                    let minimized = unsafe {
                        ::windows::Win32::UI::WindowsAndMessaging::IsIconic(
                            ::windows::Win32::Foundation::HWND(target as *mut _),
                        )
                    }
                    .as_bool();
                    let shell = state.peek();
                    let receipt = json!({"ok":foreground.0 as isize == target && !minimized,
                        "nonce":super::check::click_nonce(),"pid":std::process::id(),"hwnd":target,
                        "foregroundHwnd":foreground.0 as isize,"minimized":minimized,
                        "noTransport":shell.core.transport.is_none(),"noSession":shell.core.session.is_none(),
                        "activationCount":count(),"handledCount":handled,"callbackTimestampMs":CALLBACK_MS.load(Ordering::Acquire),
                        "handledTimestampMs":handled_ms,"foregroundTimestampMs":timestamp_ms()});
                    if cold::child() {
                        cold::handled(receipt);
                    } else {
                        eprintln!("notification-click-activated: {receipt}");
                    }
                    *LAST_HANDLED.lock().unwrap() = Some(
                        json!({"handled":handled,"targetHwnd":target,"foregroundHwnd":foreground.0 as isize}),
                    );
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn activation_accepts_only_same_identity_foreground_command() {
        assert!(admitted("app", "open", "app").is_some());
        for (id, value) in [
            ("other", "open"),
            ("app", "https://example.test"),
            ("app", "open extra"),
            ("app", ""),
        ] {
            assert!(admitted(id, value, "app").is_none());
        }
    }
    #[test]
    fn com_strings_are_bounded_before_decoding() {
        let short = [b'o' as u16, b'k' as u16, 0];
        let long = [b'x' as u16; 5];
        assert_eq!(
            unsafe { bounded_string(PCWSTR(short.as_ptr()), 2) }.unwrap(),
            "ok"
        );
        assert!(unsafe { bounded_string(PCWSTR(long.as_ptr()), 4) }.is_err());
    }
}
