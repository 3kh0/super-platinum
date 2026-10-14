//! Per-user unpackaged identity: Start shortcut + AUMID + real COM activator.
//! Ordinary offline fixtures never register the production application.
use super::*;
use ::windows::{
    Win32::{
        Foundation::PROPERTYKEY,
        Storage::EnhancedStorage::{PKEY_AppUserModel_ID, PKEY_AppUserModel_ToastActivatorCLSID},
        System::{
            Com::{
                CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree, IPersistFile,
                StructuredStorage::{InitPropVariantFromCLSID, PROPVARIANT, PropVariantToGUID},
            },
            Registry::*,
        },
        UI::Shell::{
            FOLDERID_Programs, IShellLinkW, KF_FLAG_DEFAULT, PropertiesSystem::IPropertyStore,
            SHGetKnownFolderPath, SetCurrentProcessExplicitAppUserModelID, ShellLink,
        },
    },
    core::{GUID, HSTRING, Interface, PCWSTR},
};
use std::{
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};

pub(super) const PRODUCTION: &str = "com.echonet.superplatinum";
pub(super) const FIXTURE: &str = "com.echonet.superplatinum.fixture.notifications";
const BADGE: &str = "com.echonet.superplatinum.fixture.badge";
pub(super) const PRODUCTION_CLSID: GUID = GUID::from_u128(0x4b3a4c96_9f72_4afb_a549_84cc5db5c071);
pub(super) const FIXTURE_CLSID: GUID = GUID::from_u128(0x910ce832_ee9f_4c71_832f_0f4bdb37a2b8);
type Pending = std::sync::mpsc::Receiver<Result<Registration, String>>;
static READY: Mutex<Option<Registration>> = Mutex::new(None);
static PENDING: Mutex<Option<Pending>> = Mutex::new(None);
static LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);
static PROCESS_ID_SET: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static CLEANED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub(super) struct Registration {
    pub(super) shortcut: PathBuf,
    pub(super) shortcut_created: bool,
    pub(super) keys_created: Vec<String>,
}

/// Called before the first thread or session access. The private COM launch must
/// remain disconnected even when Windows starts it without the fixture env.
pub(crate) fn prepare_startup() {
    if std::env::args().any(|arg| {
        arg == "--super-platinum-toast-fixture-activated" || arg.starts_with(cold::CHILD_ARG)
    }) {
        unsafe {
            std::env::set_var("SUPER_PLATINUM_FIXTURE", "channel");
            std::env::set_var("SUPER_PLATINUM_NOTIFICATION_ACTIVATION_FIXTURE", "1");
            std::env::remove_var("SUPER_PLATINUM_NOTIFICATION_CHECK");
            std::env::remove_var("SUPER_PLATINUM_NOTIFICATION_COLD_CHECK");
            std::env::remove_var("SUPER_PLATINUM_NOTIFICATION_CLICK_CHECK");
        }
    }
}
pub(super) fn fixture() -> bool {
    std::env::var("SUPER_PLATINUM_FIXTURE").as_deref() == Ok("channel")
        && notification_fixture_requested()
}
fn notification_fixture_requested() -> bool {
    std::env::var("SUPER_PLATINUM_NOTIFICATION_CHECK").as_deref() == Ok("1")
        || std::env::var("SUPER_PLATINUM_NOTIFICATION_ACTIVATION_FIXTURE").as_deref() == Ok("1")
}
fn selected_id(fixture_name: Option<&str>, notification: bool, badge: bool) -> &'static str {
    match fixture_name {
        Some("channel") if notification => FIXTURE,
        Some("channel") if badge => BADGE,
        Some(_) => "com.echonet.superplatinum.fixture",
        None => PRODUCTION,
    }
}
pub(crate) fn app_id() -> &'static str {
    selected_id(
        std::env::var("SUPER_PLATINUM_FIXTURE").ok().as_deref(),
        notification_fixture_requested(),
        matches!(
            std::env::var("SUPER_PLATINUM_TASKBAR_BADGE_CHECK").as_deref(),
            Ok("1" | "projection")
        ),
    )
}
pub(super) fn clsid() -> GUID {
    if fixture() {
        FIXTURE_CLSID
    } else {
        PRODUCTION_CLSID
    }
}
pub(crate) fn initialize() -> Result<(), String> {
    if !PROCESS_ID_SET.load(Ordering::Acquire) {
        unsafe { SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(app_id())) }
            .map_err(|error| format!("Set process AUMID: {error}"))?;
        PROCESS_ID_SET.store(true, Ordering::Release);
    }
    if app_id() != PRODUCTION && !fixture() {
        return Ok(());
    }
    if CLEANED.load(Ordering::Acquire) {
        return Err("Fixture notification identity was cleaned up.".into());
    }
    if READY.lock().unwrap().is_some() {
        return Ok(());
    }
    let mut pending = PENDING.lock().unwrap();
    if pending.is_none() {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("notification-identity".into())
            .spawn(move || {
                let result = (|| {
                    let apartment = Apartment::new()?;
                    let registration = register()?;
                    let server = match activation::register() {
                        Ok(server) => server,
                        Err(error) => {
                            if fixture() && !cold::active() {
                                let _ = remove_owned(&registration);
                            }
                            return Err(error);
                        }
                    };
                    Ok((apartment, registration, server))
                })();
                match result {
                    Ok((_apartment, registration, server)) => {
                        if let Err(error) = tx.send(Ok(registration)) {
                            if let Ok(registration) = error.0 {
                                if fixture() && !cold::active() {
                                    let _ = remove_owned(&registration);
                                }
                            }
                            activation::stop();
                        }
                        server.wait();
                    }
                    Err(error) => {
                        let _ = tx.send(Err(error));
                    }
                }
            })
            .map_err(|error| error.to_string())?;
        *pending = Some(rx);
    }
    // A timeout keeps the original receiver. A later call joins that attempt,
    // rather than starting a second registration while COM work is still active.
    match pending
        .as_ref()
        .unwrap()
        .recv_timeout(Duration::from_secs(5))
    {
        Ok(Ok(registration)) => {
            *READY.lock().unwrap() = Some(registration);
            *pending = None;
            *LAST_ERROR.lock().unwrap() = None;
            Ok(())
        }
        Ok(Err(error)) => {
            *pending = None;
            *LAST_ERROR.lock().unwrap() = Some(error.clone());
            Err(error)
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Err(
            "Notification identity registration is still pending; no second attempt was started."
                .into(),
        ),
        Err(error) => {
            *pending = None;
            Err(error.to_string())
        }
    }
}
pub(super) fn ready() -> Result<(), String> {
    if !PROCESS_ID_SET.load(Ordering::Acquire) {
        return Err("Process notification identity was not set before startup.".into());
    }
    if app_id() != PRODUCTION && !fixture() {
        return Err("Notifications are disabled for ordinary offline fixtures.".into());
    }
    initialize()
}
pub(super) fn receipt() -> Value {
    match READY.lock().unwrap().as_ref() {
        Some(registration) => {
            json!({"app_id":app_id(),"registered":true,"fixture":fixture(),"shortcut":registration.shortcut,"com_activator":true,"cleaned":CLEANED.load(Ordering::Acquire)})
        }
        None => {
            json!({"app_id":app_id(),"registered":false,"error":LAST_ERROR.lock().unwrap().clone()})
        }
    }
}

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}
fn register() -> Result<Registration, String> {
    if cold::child() {
        return cold::register_child();
    }
    if cold::parent() {
        cold::begin_parent()?;
    }
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let programs = programs_path()?;
    std::fs::create_dir_all(&programs).map_err(|error| error.to_string())?;
    let shortcut = programs.join(if fixture() {
        "Super Platinum Notification Fixture.lnk"
    } else {
        "Super Platinum.lnk"
    });
    let shortcut_created = !shortcut.exists();
    let mut owned = Registration {
        shortcut: shortcut.clone(),
        shortcut_created: false,
        keys_created: Vec::new(),
    };
    let result = (|| {
        let link: IShellLinkW = unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }
            .map_err(|error| format!("Create notification shortcut: {error}"))?;
        let persist: IPersistFile = link.cast().map_err(|error| error.to_string())?;
        let store: IPropertyStore = link.cast().map_err(|error| error.to_string())?;
        if !shortcut_created {
            use ::windows::Win32::System::Com::STGM_READ;
            unsafe { persist.Load(PCWSTR(wide(&shortcut).as_ptr()), STGM_READ) }
                .map_err(|error| error.to_string())?;
            let value = unsafe { store.GetValue(&PKEY_AppUserModel_ID) }
                .map_err(|error| error.to_string())?;
            let prior =
                ::windows::core::BSTR::try_from(&value).map_err(|error| error.to_string())?;
            if prior.to_string() != app_id() {
                return Err(
                    "An unrelated Start shortcut already uses the Super Platinum name.".into(),
                );
            }
            if fixture() {
                let value = unsafe { store.GetValue(&PKEY_AppUserModel_ToastActivatorCLSID) }
                    .map_err(|error| error.to_string())?;
                let prior_clsid =
                    unsafe { PropVariantToGUID(&value) }.map_err(|error| error.to_string())?;
                let mut target = vec![0u16; 32768];
                unsafe { link.GetPath(&mut target, std::ptr::null_mut(), 0) }
                    .map_err(|error| error.to_string())?;
                let len = target
                    .iter()
                    .position(|value| *value == 0)
                    .unwrap_or(target.len());
                if prior_clsid != clsid() || target[..len] != wide(&exe)[..wide(&exe).len() - 1] {
                    return Err("Existing fixture shortcut differs; it was preserved.".into());
                }
            }
        }
        if shortcut_created || !fixture() {
            unsafe {
                link.SetPath(PCWSTR(wide(&exe).as_ptr()))
                    .map_err(|error| error.to_string())?;
                link.SetWorkingDirectory(PCWSTR(
                    wide(exe.parent().ok_or("Executable directory unavailable")?).as_ptr(),
                ))
                .map_err(|error| error.to_string())?;
                link.SetIconLocation(PCWSTR(wide(&exe).as_ptr()), 0)
                    .map_err(|error| error.to_string())?;
                link.SetDescription(&HSTRING::from(if cold::parent() {
                    cold::shortcut_description()?
                } else if fixture() {
                    "Super Platinum notification fixture".into()
                } else {
                    "Super Platinum".into()
                }))
                .map_err(|error| error.to_string())?;
                set_property(&store, &PKEY_AppUserModel_ID, &string_property(app_id())?)?;
                set_property(
                    &store,
                    &PKEY_AppUserModel_ToastActivatorCLSID,
                    &InitPropVariantFromCLSID(&clsid()).map_err(|error| error.to_string())?,
                )?;
                store.Commit().map_err(|error| error.to_string())?;
                owned.shortcut_created = shortcut_created;
                if cold::parent() {
                    cold::write_shortcut(&persist)?;
                } else {
                    persist
                        .Save(PCWSTR(wide(&shortcut).as_ptr()), true)
                        .map_err(|error| error.to_string())?;
                }
            }
        }
        let guid = format!("{{{:?}}}", clsid());
        let server_key = format!(r"Software\Classes\CLSID\{guid}\LocalServer32");
        let id_key = format!(r"Software\Classes\AppUserModelId\{}", app_id());
        let argument = if cold::parent() {
            cold::argument()?
        } else if fixture() {
            "--super-platinum-toast-fixture-activated".into()
        } else {
            "--super-platinum-toast-activated".into()
        };
        let command = format!("\"{}\" {argument}", exe.display());
        set_registry(
            &format!(r"Software\Classes\CLSID\{guid}"),
            &[],
            &mut owned.keys_created,
        )?;
        set_registry(&server_key, &[("", &command)], &mut owned.keys_created)?;
        set_registry(
            &id_key,
            &[
                (
                    "DisplayName",
                    if fixture() {
                        "Super Platinum Fixture"
                    } else {
                        "Super Platinum"
                    },
                ),
                ("CustomActivator", &guid),
            ],
            &mut owned.keys_created,
        )?;
        if cold::parent() {
            cold::registered()?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => Ok(owned),
        Err(error) => {
            if fixture() && !cold::active() {
                if let Err(cleanup) = remove_owned(&owned) {
                    return Err(format!("{error}; fixture rollback failed: {cleanup}"));
                }
            }
            Err(error)
        }
    }
}
fn programs_path() -> Result<PathBuf, String> {
    // Use the current per-user location, including redirected Start menus.
    // SHGetKnownFolderPath transfers this string with the COM allocator.
    let raw = unsafe { SHGetKnownFolderPath(&FOLDERID_Programs, KF_FLAG_DEFAULT, None) }
        .map_err(|error| format!("Resolve Start menu Programs folder: {error}"))?;
    let path = PathBuf::from(std::ffi::OsString::from_wide(unsafe { raw.as_wide() }));
    unsafe { CoTaskMemFree(Some(raw.0.cast())) };
    Ok(path)
}
fn string_property(value: &str) -> Result<PROPVARIANT, String> {
    // Shell identity properties are VT_LPWSTR, as in Microsoft's desktop sample.
    let text = unsafe { ::windows::Win32::UI::Shell::SHStrDupW(&HSTRING::from(value)) }
        .map_err(|error| error.to_string())?;
    let mut variant = PROPVARIANT::default();
    unsafe {
        (*variant.Anonymous.Anonymous).vt = ::windows::Win32::System::Variant::VT_LPWSTR;
        (*variant.Anonymous.Anonymous).Anonymous.pwszVal = text;
    }
    Ok(variant)
}
unsafe fn set_property(
    store: &IPropertyStore,
    key: &PROPERTYKEY,
    value: &PROPVARIANT,
) -> Result<(), String> {
    unsafe { store.SetValue(key, value) }.map_err(|error| error.to_string())
}
fn set_registry(
    path: &str,
    values: &[(&str, &str)],
    created: &mut Vec<String>,
) -> Result<(), String> {
    if cold::parent() {
        return cold::write_registry(path, values);
    }
    let mut key = HKEY::default();
    let mut disposition = REG_CREATE_KEY_DISPOSITION::default();
    unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            &HSTRING::from(path),
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut key,
            Some(&mut disposition),
        )
    }
    .ok()
    .map_err(|error| error.to_string())?;
    if disposition == REG_CREATED_NEW_KEY {
        created.push(path.to_owned());
    }
    let result = values.iter().try_for_each(|(name, value)| {
        if fixture() && disposition != REG_CREATED_NEW_KEY {
            return if registry_string(path, name)? == *value {
                Ok(())
            } else {
                Err("Existing fixture registration differs; it was preserved.".into())
            };
        }
        let bytes = value
            .encode_utf16()
            .chain(Some(0))
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        unsafe { RegSetValueExW(key, &HSTRING::from(*name), None, REG_SZ, Some(&bytes)) }
            .ok()
            .map_err(|error| error.to_string())
    });
    unsafe {
        let _ = RegCloseKey(key);
    }
    result
}
pub(super) fn cleanup_fixture() -> Result<Value, String> {
    if !fixture() {
        return Err("Only the isolated notification fixture may remove registration.".into());
    }
    let held = READY.lock().unwrap();
    let registration = held.as_ref().ok_or("No fixture registration")?;
    activation::stop();
    remove_owned(registration)?;
    CLEANED.store(true, Ordering::Release);
    Ok(
        json!({"fixture":true,"shortcut_removed":registration.shortcut_created,"preexisting_shortcut_preserved":!registration.shortcut_created,"registration_keys_removed":registration.keys_created.len(),"preexisting_registration_preserved":true}),
    )
}
fn remove_owned(registration: &Registration) -> Result<(), String> {
    if registration.shortcut_created && registration.shortcut.exists() {
        std::fs::remove_file(&registration.shortcut).map_err(|error| error.to_string())?;
    }
    for path in registration.keys_created.iter().rev() {
        unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(path)) }
            .ok()
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}
fn registry_string(path: &str, name: &str) -> Result<String, String> {
    let mut data = vec![0u16; 32768];
    let mut size = (data.len() * 2) as u32;
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(path),
            &HSTRING::from(name),
            RRF_RT_REG_SZ,
            None,
            Some(data.as_mut_ptr().cast()),
            Some(&mut size),
        )
    }
    .ok()
    .map_err(|error| error.to_string())?;
    let length = data
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(data.len());
    String::from_utf16(&data[..length]).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_exact_fixture_gate_can_register_fixture_toasts() {
        assert_eq!(selected_id(None, false, false), PRODUCTION);
        assert_eq!(selected_id(Some("channel"), true, false), FIXTURE);
        assert_eq!(selected_id(Some("channel"), false, true), BADGE);
        for name in ["slack-list", "channel-narrow", ""] {
            assert_ne!(selected_id(Some(name), true, true), PRODUCTION);
            assert_ne!(selected_id(Some(name), true, true), FIXTURE);
        }
        assert_ne!(PRODUCTION_CLSID, FIXTURE_CLSID);
    }
}
