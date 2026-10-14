//! Explicit disconnected cold-launch fixture. A durable nonce/executable-bound
//! ledger owns each narrow mutation before it occurs. Never used by real toasts.
use super::*;
use std::{fs, path::PathBuf};
mod model;
mod platform;
mod registration;
use model::{Executable, Kind, Ledger, Phase};
pub(super) fn write_registry(path: &str, values: &[(&str, &str)]) -> Result<(), String> {
    registration::registry(path, values)
}
pub(super) fn write_shortcut(
    persist: &::windows::Win32::System::Com::IPersistFile,
) -> Result<(), String> {
    registration::shortcut(persist)
}
static LEDGER: Mutex<Option<Ledger>> = Mutex::new(None);
static EXECUTABLE: OnceLock<Executable> = OnceLock::new();
static CHILD_HANDLED: Mutex<Option<Value>> = Mutex::new(None);
static CHILD_WAKE: tokio::sync::Notify = tokio::sync::Notify::const_new();
pub(super) const CHILD_ARG: &str = "--super-platinum-toast-cold-activated=";
const RECOVER_ARG: &str = "--super-platinum-toast-cold-recover=";

pub(super) fn parent() -> bool {
    identity::fixture()
        && std::env::var("SUPER_PLATINUM_NOTIFICATION_COLD_CHECK").as_deref() == Ok("1")
        && std::env::var_os("SUPER_PLATINUM_NOTIFICATION_CLICK_CHECK").is_none()
}
pub(super) fn child() -> bool {
    std::env::args().any(|argument| argument.starts_with(CHILD_ARG))
}
pub(super) fn active() -> bool {
    parent() || child()
}
fn nonce() -> Result<String, String> {
    let nonce = if child() {
        std::env::args().find_map(|arg| arg.strip_prefix(CHILD_ARG).map(str::to_owned))
    } else {
        std::env::var("SUPER_PLATINUM_NOTIFICATION_COLD_NONCE").ok()
    };
    nonce
        .filter(|value| model::valid_nonce(value))
        .ok_or("Missing/invalid cold fixture nonce".into())
}
fn executable() -> Result<Executable, String> {
    if let Some(exe) = EXECUTABLE.get() {
        return Ok(exe.clone());
    }
    let exe = platform::executable()?;
    let _ = EXECUTABLE.set(exe.clone());
    Ok(exe)
}
fn directory() -> Result<PathBuf, String> {
    platform::directory(&nonce()?)
}
fn persist(ledger: &Ledger) -> Result<(), String> {
    platform::atomic_json(
        &platform::directory(&ledger.nonce)?.join("ledger.json"),
        ledger,
    )
}
fn change(update: impl FnOnce(&mut Ledger) -> Result<(), String>) -> Result<(), String> {
    let mut held = LEDGER.lock().unwrap();
    let mut next = held
        .as_ref()
        .ok_or("Cold fixture ledger is unavailable")?
        .clone();
    update(&mut next)?;
    persist(&next)?;
    *held = Some(next);
    Ok(())
}
fn read(nonce: &str) -> Result<Ledger, String> {
    let root = platform::root()?;
    let exe = executable()?;
    let path = platform::directory(nonce)?.join("ledger.json");
    let parse = |path: &std::path::Path| -> Result<Option<Ledger>, String> {
        if path.try_exists().map_err(|e| e.to_string())? {
            Ledger::parse(
                &platform::read_bounded(path, model::MAX_LEDGER)?,
                nonce,
                &exe,
            )
            .map(Some)
        } else {
            Ok(None)
        }
    };
    let ledger = select_handoff(parse(&root.join("owner"))?, parse(&path)?)?;
    registration::validate_expected(&ledger)?;
    Ok(ledger)
}
fn select_handoff(lease: Option<Ledger>, saved: Option<Ledger>) -> Result<Ledger, String> {
    let Some(lease) = lease else {
        return saved
            .filter(|ledger| ledger.phase == Phase::Cleaned)
            .ok_or("Cold fixture lease is missing; incomplete ledger preserved".into());
    };
    if lease.phase != Phase::Acquiring || !lease.artifacts.is_empty() || lease.child.is_some() {
        return Err("Invalid immutable cold fixture lease".into());
    }
    let ledger = saved.unwrap_or_else(|| lease.clone());
    if ledger.owner != lease.owner {
        return Err("Cold ledger owner differs from immutable lease".into());
    }
    Ok(ledger)
}
fn registration_absent() -> Result<bool, String> {
    Ok(!platform::shortcut()?
        .try_exists()
        .map_err(|e| e.to_string())?
        && ![Kind::Clsid, Kind::LocalServer, Kind::AppId]
            .into_iter()
            .map(platform::key_exists)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .any(|present| present))
}

pub(super) fn begin_parent() -> Result<(), String> {
    let _operation = platform::operation_lock()?;
    if LEDGER.lock().unwrap().is_some() {
        return Err("Cold setup already began; recover before retrying".into());
    }
    let nonce = nonce()?;
    let exe = executable()?;
    let root = platform::root()?;
    if !registration_absent()? {
        return Err("Cold fixture requires all registration artifacts absent".into());
    }
    let dir = platform::directory(&nonce)?;
    if dir.exists() {
        return Err("Cold nonce already exists; never reuse a handoff".into());
    }
    let ledger = Ledger {
        version: model::VERSION,
        nonce,
        executable: exe,
        owner: platform::current_process()?,
        child: None,
        created_ms: activation::timestamp_ms(),
        phase: Phase::Acquiring,
        artifacts: Vec::new(),
        preview: None,
        preview_removed: false,
    };
    platform::exclusive_json(&root.join("owner"), &ledger)?;
    fs::create_dir(&dir).map_err(|e| e.to_string())?;
    persist(&ledger)?;
    *LEDGER.lock().unwrap() = Some(ledger);
    if !registration_absent()? {
        return Err("Cold fixture requires every registration artifact absent; existing artifacts preserved".into());
    }
    Ok(())
}

pub(super) fn argument() -> Result<String, String> {
    Ok(format!("{CHILD_ARG}{}", nonce()?))
}
pub(super) fn shortcut_description() -> Result<String, String> {
    Ok(format!(
        "Super Platinum cold notification fixture {}",
        nonce()?
    ))
}
pub(super) fn register_child() -> Result<identity::Registration, String> {
    let _operation = platform::operation_lock()?;
    let nonce = nonce()?;
    let mut ledger = read(&nonce)?;
    if activation::timestamp_ms().saturating_sub(ledger.created_ms) > 60 * 60 * 1000 {
        return Err("Cold handoff expired; exact recovery is required".into());
    }
    registration::verify_complete(&ledger)?;
    let child = platform::current_process()?;
    ledger.claim_child(child.clone(), platform::alive(&ledger.owner)?)?;
    // An immutable create-new claim prevents two simultaneously launched COM
    // children from both adopting the same ready ledger.
    platform::exclusive_json(&directory()?.join("child-claim.json"), &child)?;
    persist(&ledger)?;
    let registration = identity::Registration {
        shortcut: platform::shortcut()?,
        shortcut_created: false,
        keys_created: Vec::new(),
    };
    *LEDGER.lock().unwrap() = Some(ledger);
    Ok(registration)
}
pub(super) fn registered() -> Result<(), String> {
    change(|ledger| {
        ledger.phase = Phase::Registered;
        Ok(())
    })
}
pub(super) fn reserve(tag: &str, group: &str) -> Result<(), String> {
    if !parent() {
        return Ok(());
    }
    change(|ledger| ledger.reserve_preview(tag.into(), group.into()))
}
pub(super) fn preview_identity() -> Result<Option<(String, String)>, String> {
    if !parent() {
        return Ok(None);
    }
    let nonce = nonce()?;
    Ok(Some((nonce[..16].into(), nonce[16..].into())))
}
pub(super) fn xml(xml: String, preview: bool) -> Result<String, String> {
    if !parent() {
        return Ok(xml);
    }
    if !preview {
        return Err("Cold fixture may submit only its isolated preview".into());
    }
    Ok(xml.replacen(
        "<toast launch='open'>",
        &format!("<toast launch='cold:{}' duration='long'>", nonce()?),
        1,
    ))
}
pub(super) fn admits(arguments: &str) -> bool {
    child()
        && nonce().is_ok_and(|nonce| arguments == format!("cold:{nonce}"))
        && LEDGER.lock().unwrap().as_ref().is_some_and(|ledger| {
            ledger
                .child
                .as_ref()
                .is_some_and(|child| child.pid == std::process::id())
        })
}
pub(super) fn handled(value: Value) {
    if child() {
        *CHILD_HANDLED.lock().unwrap() = Some(value);
        CHILD_WAKE.notify_one();
    }
}

pub(super) async fn run(state: dioxus::prelude::Signal<crate::state::ShellState>) {
    use dioxus::prelude::ReadableExt;
    if !active() {
        return;
    }
    let disconnected = state.peek().core.session.is_none() && state.peek().core.transport.is_none();
    if !disconnected {
        eprintln!("notification-cold-error: disconnected fixture required");
        return;
    }
    if parent() {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let result = async {
            let nonce = nonce()?;
            let submitted = submit(DesktopNotification {
                title: "Super Platinum".into(),
                body: format!("Super Platinum cold activation check {nonce}"),
                sound: None,
                destination: None,
            }, true).await?;
            let accepted = submitted["submitted"] == true
                && submitted["history_present"] == true
                && submitted["failed_hresult"].is_null();
            if !accepted {
                return Err("Cold preview was not accepted into history; recover exact ledger".into());
            }
            let handoff_directory = tokio::task::spawn_blocking(|| {
                change(|ledger| { ledger.phase = Phase::Ready; Ok(()) })?;
                directory()
            }).await.map_err(|e| e.to_string())??;
            let ledger = LEDGER.lock().unwrap().clone().ok_or("Missing cold ledger")?;
            let no_transport = state.peek().core.transport.is_none();
            let no_session = state.peek().core.session.is_none();
            let receipt = json!({"ok":true,"nonce":nonce,"parent":ledger.owner,"executable":ledger.executable,
                "app_id":identity::FIXTURE,"appName":"Super Platinum Fixture","title":"Super Platinum",
                "body":format!("Super Platinum cold activation check {nonce}"),"directory":handoff_directory,
                "submitted":submitted,"noTransport":no_transport,"noSession":no_session,"exitAuthorized":false});
            let copy = receipt.clone();
            tokio::task::spawn_blocking(move || platform::atomic_json(&directory()?.join("parent.json"), &copy))
                .await.map_err(|e| e.to_string())??;
            eprintln!("notification-cold-ready: {receipt}");
            Ok::<(), String>(())
        }.await;
        if let Err(error) = result {
            eprintln!("notification-cold-error: {error}");
        }
        // Runner must review the durable ready receipt before terminating this
        // exact process. No automatic exit, replay, or time-based deletion.
    } else {
        let waited = tokio::time::timeout(Duration::from_secs(30), CHILD_WAKE.notified()).await;
        let receipt = CHILD_HANDLED.lock().unwrap().clone();
        let result = tokio::time::timeout(
            Duration::from_secs(10),
            tokio::task::spawn_blocking(move || {
                let mut receipt = receipt.ok_or("Cold COM activation did not reach the UI hook")?;
                receipt["nonce"] = json!(nonce()?);
                receipt["process"] = json!(platform::current_process()?);
                receipt["executable"] = json!(executable()?);
                platform::atomic_json(&directory()?.join("child.json"), &receipt)?;
                cleanup(false)
            }),
        )
        .await;
        if waited.is_err() || !matches!(&result, Ok(Ok(Ok(_)))) {
            eprintln!(
                "notification-cold-error: child receipt/cleanup unconfirmed; use exact recovery"
            );
        }
    }
}

fn cleanup(recovery: bool) -> Result<Value, String> {
    let nonce = nonce()?;
    cleanup_nonce(&nonce, recovery)
}
fn cleanup_nonce(nonce: &str, recovery: bool) -> Result<Value, String> {
    let _operation = platform::operation_lock()?;
    let mut ledger = read(nonce)?;
    let caller = platform::current_process()?;
    let directory = platform::directory(nonce)?;
    let claim = directory.join("child-claim.json");
    if claim.try_exists().map_err(|e| e.to_string())? {
        let claimed: model::Process = serde_json::from_slice(&platform::read_bounded(&claim, 256)?)
            .map_err(|_| "Invalid cold child claim")?;
        if claimed.pid == 0
            || claimed.created == 0
            || claimed == ledger.owner
            || ledger.child.as_ref().is_some_and(|child| child != &claimed)
        {
            return Err("Conflicting cold child ownership; preserved".into());
        }
        ledger.child = Some(claimed);
    }
    let owner_alive = platform::alive(&ledger.owner)?;
    let child_alive = ledger
        .child
        .as_ref()
        .map(platform::alive)
        .transpose()?
        .unwrap_or(false);
    if (recovery && (owner_alive || child_alive))
        || !ledger.cleanup_allowed(&caller, owner_alive, child_alive)
    {
        return Err("Cold fixture process still owns these artifacts; cleanup refused".into());
    }
    if !directory.exists() {
        fs::create_dir(&directory).map_err(|e| e.to_string())?;
    }
    ledger.begin_cleanup()?;
    persist(&ledger)?;
    let _apartment = Apartment::new()?;
    if !ledger.preview_removed {
        if let Some(preview) = &ledger.preview {
            let history = ToastNotificationManager::History().map_err(|e| e.to_string())?;
            history
                .RemoveGroupedTagWithId(
                    &HSTRING::from(&preview.tag),
                    &HSTRING::from(&preview.group),
                    &HSTRING::from(identity::FIXTURE),
                )
                .map_err(|e| e.to_string())?;
        }
        verify_preview_absent(&ledger)?;
        ledger.preview_removed = true;
        persist(&ledger)?;
    }
    // Never trust a persisted removed flag as proof of present OS state, including
    // an idempotent recovery after the final lease-release receipt was interrupted.
    verify_preview_absent(&ledger)?;
    activation::stop();
    for index in (0..ledger.artifacts.len()).rev() {
        if ledger.artifacts[index].removed {
            continue;
        }
        registration::remove(&ledger.artifacts[index])?;
        ledger.artifacts[index].removed = true;
        persist(&ledger)?;
    }
    if !registration_absent()? {
        return Err("Cold fixture has remaining or replaced artifacts; cleanup incomplete".into());
    }
    ledger.phase = Phase::Cleaned;
    persist(&ledger)?;
    let mut receipt = json!({"ok":true,"nonce":nonce,"recovery":recovery,"previewRemoved":true,
        "registrationRestored":true,"ownedArtifacts":ledger.artifacts.len(),"process":caller,"leaseReleased":false});
    // A durable cleanup receipt precedes releasing the lease. A crash at either
    // following step can be retried from a validated Cleaned ledger.
    platform::atomic_json(&directory.join("cleanup.json"), &receipt)?;
    let owner = platform::root()?.join("owner");
    if owner.try_exists().map_err(|e| e.to_string())? {
        let lease = Ledger::parse(
            &platform::read_bounded(&owner, model::MAX_LEDGER)?,
            nonce,
            &executable()?,
        )?;
        if lease.owner != ledger.owner {
            return Err("Cold lease was replaced; preserved".into());
        }
        fs::remove_file(owner).map_err(|e| e.to_string())?;
    }
    receipt["leaseReleased"] = json!(true);
    platform::atomic_json(&directory.join("cleanup.json"), &receipt)?;
    Ok(receipt)
}

fn verify_preview_absent(ledger: &Ledger) -> Result<(), String> {
    let Some(preview) = &ledger.preview else {
        return Ok(());
    };
    let items = ToastNotificationManager::History()
        .map_err(|e| e.to_string())?
        .GetHistoryWithId(&HSTRING::from(identity::FIXTURE))
        .map_err(|e| e.to_string())?;
    for item in items {
        let tag = item.Tag().map_err(|e| e.to_string())?;
        let group = item.Group().map_err(|e| e.to_string())?;
        if tag == HSTRING::from(&preview.tag) && group == HSTRING::from(&preview.group) {
            return Err("Cold preview is still in history; registration preserved".into());
        }
    }
    Ok(())
}

/// Early-main recovery: no application UI, fixture registration, or session load.
pub(crate) fn recovery_mode() -> Option<i32> {
    let nonce = std::env::args()
        .find_map(|argument| argument.strip_prefix(RECOVER_ARG).map(str::to_owned))?;
    let result = cleanup_nonce(&nonce, true);
    match result {
        Ok(receipt) => {
            eprintln!("notification-cold-recovery: {receipt}");
            Some(0)
        }
        Err(error) => {
            eprintln!(
                "notification-cold-recovery: {}",
                json!({"ok":false,"error":error})
            );
            Some(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn lease() -> Ledger {
        Ledger {
            version: model::VERSION,
            nonce: "0123456789abcdefabcdef0123456789".into(),
            executable: Executable {
                path: r"C:\fixture\app.exe".into(),
                sha256: "a".repeat(64),
            },
            owner: model::Process {
                pid: 12,
                created: 99,
            },
            child: None,
            created_ms: 1,
            phase: Phase::Acquiring,
            artifacts: Vec::new(),
            preview: None,
            preview_removed: false,
        }
    }
    #[test]
    fn lease_survives_failure_before_nonce_directory_or_ledger_publication() {
        let lease = lease();
        let recovered = select_handoff(Some(lease.clone()), None).unwrap();
        assert_eq!(recovered.owner, lease.owner);
        assert_eq!(recovered.phase, Phase::Acquiring);
        assert!(recovered.artifacts.is_empty());
        let mut foreign = lease.clone();
        foreign.owner.created += 1;
        assert!(select_handoff(Some(lease), Some(foreign)).is_err());
    }
    #[test]
    fn recovery_without_lease_requires_completed_ledger() {
        assert!(select_handoff(None, None).is_err());
        assert!(select_handoff(None, Some(lease())).is_err());
        let mut cleaned = lease();
        cleaned.phase = Phase::Cleaned;
        cleaned.preview_removed = true;
        // A repeated attempt can fail after this durable write but before its
        // receipt. It must remain recoverable without recreating a missing lease.
        cleaned.begin_cleanup().unwrap();
        let saved = serde_json::to_vec(&cleaned).unwrap();
        let cleaned = Ledger::parse(&saved, &cleaned.nonce, &cleaned.executable).unwrap();
        assert_eq!(
            select_handoff(None, Some(cleaned)).unwrap().phase,
            Phase::Cleaned
        );
        // OS history/registration absence is still checked by cleanup_nonce.
    }
}
