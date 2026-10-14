//! Exact-gated synthetic storage/writers. No production account or source fallback.
use super::*;
use crate::state::{ComposerTarget, ShellState};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use super_platinum_core::{ComposerState, drafts::store::DraftStore};

pub(crate) const ACCOUNT: &str = "00000000-0000-4000-8000-000000000071";
pub(crate) const ROOT: &str = "1719800400.000000";
pub(crate) const BEFORE: &str = "Original \u{2014} \u{65e5}\u{672c}\u{8a9e} \u{1f680} e\u{301}";
pub(crate) const AFTER: &str = "Observed \u{2014} \u{65e5}\u{672c}\u{8a9e} \u{1f680} e\u{301}";
pub(crate) const NEWER: &str = "Newer local \u{2014} \u{65e5}\u{672c}\u{8a9e} \u{1f680} e\u{301}";
pub(crate) const SCENARIOS: &[&str] = &[
    "apply",
    "ahead",
    "input-before",
    "input-after",
    "input-between",
    "selection",
    "composition-input-first",
    "composition-end-first",
    "remount",
    "navigation",
];

pub(crate) fn enabled() -> bool {
    std::env::var("SUPER_PLATINUM_DRAFT_PROJECTION_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("draft-native-projection" | "draft-native-projection-narrow")
        )
}
pub(crate) fn key(target: ComposerTarget) -> DraftKey {
    DraftKey {
        team: "T1".into(),
        channel: "C2".into(),
        thread: matches!(target, ComposerTarget::Thread).then(|| ROOT.into()),
    }
}
pub(crate) fn local(text: &str, target: ComposerTarget) -> LocalDraft {
    let mut composer = ComposerState::with_text(text);
    composer.reply_broadcast = matches!(target, ComposerTarget::Thread);
    LocalDraft {
        composer,
        attachments: vec![],
    }
}
pub(crate) fn apply(shell: &mut ShellState) {
    shell.core.active_account = Some(ACCOUNT.into());
    shell.core.session = None;
    shell.core.transport = None;
    shell.core.drafts = Default::default();
    shell.core.composer = local(BEFORE, ComposerTarget::Channel).composer;
    shell.core.thread_composer = local(BEFORE, ComposerTarget::Thread).composer;
    shell.core.composer_attachments.clear();
    shell.core.thread_composer_attachments.clear();
    shell.thread_root = Some(ROOT.into());
    shell.thread_generation += 1;
    shell.core.threads.insert(
        ("T1".into(), "C2".into(), ROOT.into()),
        crate::thread_vm::fixture_bag(4),
    );
    shell.core.drafts.items.insert(
        key(ComposerTarget::Channel),
        local(BEFORE, ComposerTarget::Channel),
    );
    shell.core.drafts.items.insert(
        key(ComposerTarget::Thread),
        local(BEFORE, ComposerTarget::Thread),
    );
    shell.sync_composer_drafts();
    shell.refresh_thread_from_core();
}

pub(crate) struct Binding {
    pub account: Arc<Account>,
    pub path: PathBuf,
    pub nonce: String,
    pub scenario: String,
    pub phase: String,
    pub epoch: u64,
    pub editors: BTreeMap<DraftKey, Arc<checkpoint::Editor>>,
    pub routed: AtomicU64,
    pub refused: AtomicU64,
}
fn ready_tuple(binding: &Binding) -> Value {
    json!({"version":1,"nonce":binding.nonce,"pid":std::process::id(),
        "scenario":binding.scenario,"phase":binding.phase})
}
fn admits_ready(actual: &Value, expected: &Value) -> bool {
    actual == expected
}
pub(crate) async fn wait_runner(binding: &Binding) -> Result<(), String> {
    let path = binding
        .path
        .parent()
        .ok_or("Missing private directory")?
        .join(format!("ready-{}.json", binding.phase));
    let expected = ready_tuple(binding);
    let nonce = binding.nonce.clone();
    let database = binding.path.clone();
    // Check absence before advertising readiness; an old marker cannot release
    // a new process, even if a caller reused an otherwise valid directory.
    let check = path.clone();
    tokio::task::spawn_blocking(move || {
        validate_directory(&database, &nonce)?;
        match std::fs::symlink_metadata(check) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            _ => Err("Runner readiness marker must be new".to_owned()),
        }
    })
    .await
    .map_err(|_| "Readiness admission task failed")??;
    eprintln!("draft-projection-waiting: {expected}");
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let check = path.clone();
        let expected = expected.clone();
        let ready = tokio::task::spawn_blocking(move || -> Result<bool, String> {
            let metadata = match std::fs::symlink_metadata(&check) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
                Err(_) => return Err("Cannot inspect runner readiness marker".into()),
                Ok(metadata) => metadata,
            };
            if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 1024 {
                return Err("Invalid runner readiness marker".into());
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if metadata.file_attributes() & 0x400 != 0 {
                    return Err("Reparse readiness marker refused".into());
                }
            }
            let bytes = std::fs::read(check).map_err(|_| "Cannot read readiness marker")?;
            let actual: Value =
                serde_json::from_slice(&bytes).map_err(|_| "Malformed readiness marker")?;
            if !admits_ready(&actual, &expected) {
                return Err("Wrong readiness owner or phase".into());
            }
            Ok(true)
        })
        .await
        .map_err(|_| "Readiness read task failed")??;
        if ready {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("Runner readiness timed out".into());
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}
impl Binding {
    pub fn current(&self, shell: &ShellState) -> bool {
        enabled()
            && shell.core.session.is_none()
            && shell.core.transport.is_none()
            && shell.core.active_account.as_deref() == Some(ACCOUNT)
            && shell.core.account_epoch == self.epoch
            && std::env::var_os("SUPER_PLATINUM_DRAFT_PROJECTION_DB")
                .is_some_and(|p| PathBuf::from(p) == self.path)
            && std::env::var("SUPER_PLATINUM_DRAFT_PROJECTION_NONCE").as_deref()
                == Ok(self.nonce.as_str())
    }
}
pub(crate) fn binding(shell: &ShellState) -> Option<Arc<Binding>> {
    shell
        .draft_writes
        .projection_fixture
        .as_ref()
        .filter(|b| b.current(shell))
        .cloned()
}
pub(crate) fn route(shell: &ShellState, changes: BTreeMap<DraftKey, LocalDraft>) {
    let Some(b) = binding(shell) else { return };
    for (key, local) in changes {
        if let Some(editor) = b.editors.get(&key).filter(|editor| {
            shell
                .draft_writes
                .writers
                .get(ACCOUNT, b.epoch, &key)
                .is_some_and(|current| Arc::ptr_eq(editor, &current))
        }) {
            editor.submit(local);
            b.routed.fetch_add(1, Ordering::Relaxed);
        } else {
            b.refused.fetch_add(1, Ordering::Relaxed);
        }
    }
}
fn configuration() -> Result<(PathBuf, String, String, String), String> {
    if !enabled() {
        return Err("Projection fixture opt-in missing".into());
    }
    let path = PathBuf::from(
        std::env::var_os("SUPER_PLATINUM_DRAFT_PROJECTION_DB")
            .ok_or("Missing isolated database")?,
    );
    let nonce =
        std::env::var("SUPER_PLATINUM_DRAFT_PROJECTION_NONCE").map_err(|_| "Missing nonce")?;
    let scenario = std::env::var("SUPER_PLATINUM_DRAFT_PROJECTION_SCENARIO")
        .map_err(|_| "Missing scenario")?;
    let phase =
        std::env::var("SUPER_PLATINUM_DRAFT_PROJECTION_PHASE").map_err(|_| "Missing phase")?;
    if !path.is_absolute()
        || path.file_name().and_then(|s| s.to_str()) != Some("draft-projection.sqlite")
        || nonce.len() != 32
        || !nonce
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        || !SCENARIOS.contains(&scenario.as_str())
        || !matches!(phase.as_str(), "exercise" | "restart")
    {
        return Err("Invalid isolated fixture configuration".into());
    }
    Ok((path, nonce, scenario, phase))
}
fn validate_directory(path: &std::path::Path, nonce: &str) -> Result<(), String> {
    let parent = path.parent().ok_or("Missing private directory")?;
    if !parent
        .file_name()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.starts_with("draft-native-projection-"))
    {
        return Err("Private directory name required".into());
    }
    for part in parent.ancestors() {
        let meta = std::fs::symlink_metadata(part).map_err(|_| "Missing directory ancestor")?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if meta.file_attributes() & 0x400 != 0 {
                return Err("Reparse directory refused".into());
            }
        }
        if meta.file_type().is_symlink() {
            return Err("Symlink directory refused".into());
        }
    }
    let marker =
        std::fs::read(parent.join("owner.json")).map_err(|_| "Private ownership marker missing")?;
    if marker.len() > 1024 {
        return Err("Oversized ownership marker".into());
    }
    let value: Value = serde_json::from_slice(&marker).map_err(|_| "Invalid ownership marker")?;
    if value != json!({"version":1,"nonce":nonce}) {
        return Err("Wrong ownership marker".into());
    }
    Ok(())
}

pub(crate) async fn install(mut state: Signal<ShellState>) -> Result<Arc<Binding>, String> {
    let (path, nonce, scenario, phase) = configuration()?;
    {
        let shell = state.peek();
        if shell.core.session.is_some()
            || shell.core.transport.is_some()
            || shell.core.active_account.as_deref() != Some(ACCOUNT)
        {
            return Err("Disconnected fixture owner required before storage admission".into());
        }
    }
    let seed_path = path.clone();
    let seed_nonce = nonce.clone();
    let seed_phase = phase.clone();
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        validate_directory(&seed_path, &seed_nonce)?;
        if seed_phase == "exercise" {
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&seed_path)
                .map_err(|_| "Exercise database must be new")?;
            DraftStore::open(&seed_path)
                .map_err(|_| "Open isolated store")?
                .save(
                    [ComposerTarget::Channel, ComposerTarget::Thread]
                        .map(|t| (key(t), local(BEFORE, t))),
                )
                .map_err(|_| "Seed isolated drafts")?;
        } else {
            let meta =
                std::fs::symlink_metadata(&seed_path).map_err(|_| "Restart database missing")?;
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if meta.file_attributes() & 0x400 != 0 {
                    return Err("Reparse database refused".into());
                }
            }
            if !meta.is_file() || meta.file_type().is_symlink() {
                return Err("Invalid restart database".into());
            }
        }
        Ok(())
    })
    .await
    .map_err(|_| "Storage task failed")??;
    let account = Account::new(Backend::Isolated {
        account: ACCOUNT.into(),
        path: path.clone(),
    });
    let epoch = {
        let shell = state.peek();
        if shell.core.session.is_some()
            || shell.core.transport.is_some()
            || shell.core.active_account.as_deref() != Some(ACCOUNT)
        {
            return Err("Disconnected owner required".into());
        }
        shell.core.account_epoch
    };
    let mut views = Vec::new();
    for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
        let owner = account.owner(key(target));
        if phase == "exercise" {
            owner
                .enroll(
                    local(BEFORE, target),
                    Some(RemoteDraft {
                        scope: owner.scope.clone(),
                        id: remote_id(target).into(),
                        client_msg_id: client_id(target).into(),
                        last_updated_ts: "1000".into(),
                        snapshot: codec::encode_composer(
                            &key(target),
                            &local(BEFORE, target).composer,
                        )
                        .map_err(|_| "Seed snapshot")?,
                    }),
                )
                .await
                .map_err(|e| format!("Enroll fixture: {e:?}"))?;
        }
        views.push((
            key(target),
            owner
                .load()
                .await
                .map_err(|e| format!("Load fixture: {e:?}"))?,
        ));
    }
    let mut shell = state.write();
    if shell.core.account_epoch != epoch
        || shell.core.active_account.as_deref() != Some(ACCOUNT)
        || shell.core.session.is_some()
        || shell.core.transport.is_some()
    {
        return Err("Owner changed during fixture load".into());
    }
    shell.managed_drafts = session::Session {
        account: Some(ACCOUNT.into()),
        epoch,
        ready: true,
        ..Default::default()
    };
    let mut editors = BTreeMap::new();
    for (key, view) in views {
        shell
            .managed_drafts
            .apply_view(ACCOUNT, epoch, key.clone(), view.clone());
        shell
            .core
            .drafts
            .items
            .insert(key.clone(), view.local.clone());
        if phase == "exercise" {
            editors.insert(
                key.clone(),
                shell
                    .draft_writes
                    .editor(&account, epoch, key, view)
                    .map_err(|e| format!("Editor: {e:?}"))?,
            );
        }
    }
    let b = Arc::new(Binding {
        account,
        path,
        nonce,
        scenario,
        phase,
        epoch,
        editors,
        routed: AtomicU64::new(0),
        refused: AtomicU64::new(0),
    });
    shell.draft_writes.projection_fixture = Some(b.clone());
    // Rebind without checkpointing remote state as a local authorship event.
    shell.core.drafts.channel = None;
    shell.core.drafts.thread = None;
    shell.sync_composer_drafts();
    Ok(b)
}
pub(crate) fn remote_id(target: ComposerTarget) -> &'static str {
    if matches!(target, ComposerTarget::Thread) {
        "DrFixtureThread"
    } else {
        "DrFixtureChannel"
    }
}
pub(crate) fn client_id(target: ComposerTarget) -> &'static str {
    if matches!(target, ComposerTarget::Thread) {
        "00000000-0000-4000-8000-000000000073"
    } else {
        "00000000-0000-4000-8000-000000000072"
    }
}
pub(crate) fn observation(
    shell: &ShellState,
    target: ComposerTarget,
) -> Result<observation::Prepared, String> {
    let snapshot = codec::encode_composer(&key(target), &local(AFTER, target).composer)
        .map_err(|_| "Observation snapshot")?;
    let raw=serde_json::from_value(json!({"type":"draft_update","draft_id":remote_id(target),"client_msg_id":client_id(target),"event_ts":"10.000001","draft":{"id":remote_id(target),"client_msg_id":client_id(target),"last_updated_ts":"2000","is_deleted":false,"is_sent":false,"date_scheduled":0,"blocks":snapshot.blocks,"destinations":snapshot.destinations,"file_ids":[]}})).map_err(|_|"Observation event")?;
    observation::prepare_fixture(shell, &key(target), &raw)
        .map_err(|e| format!("Observation admission: {e:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn runner_ready_requires_exact_owner_and_stage_tuple() {
        let expected =
            json!({"version":1,"nonce":"abc","pid":42,"scenario":"apply","phase":"exercise"});
        assert!(admits_ready(&expected, &expected));
        for (key, value) in [
            ("version", json!(2)),
            ("nonce", json!("other")),
            ("pid", json!(43)),
            ("scenario", json!("ahead")),
            ("phase", json!("restart")),
        ] {
            let mut changed = expected.clone();
            changed[key] = value;
            assert!(!admits_ready(&changed, &expected));
            let mut missing = expected.clone();
            missing.as_object_mut().unwrap().remove(key);
            assert!(!admits_ready(&missing, &expected));
        }
        let mut extra = expected.clone();
        extra["extra"] = json!(true);
        assert!(!admits_ready(&extra, &expected));
    }
    #[test]
    fn synthetic_text_contains_real_unicode() {
        for text in [BEFORE, AFTER, NEWER] {
            for scalar in [0x2014, 0x65e5, 0x672c, 0x8a9e, 0x1f680, 0x301] {
                assert!(text.chars().any(|c| u32::from(c) == scalar));
            }
            assert!(text.encode_utf16().count() > text.chars().count());
            assert!(!text.contains('?'));
        }
    }
    #[test]
    fn every_fixture_text_round_trips_supported_channel_and_thread_codec() {
        for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
            for text in [BEFORE, AFTER, NEWER] {
                let key = key(target);
                let local = local(text, target);
                let snapshot = codec::encode_composer(&key, &local.composer)
                    .expect("Fixture text must remain inside the managed codec subset");
                let decoded = codec::decode(&key, &snapshot).expect("Exact fixture decode");
                assert_eq!(decoded.composer().text, text);
                assert_eq!(
                    decoded.composer().reply_broadcast,
                    local.composer.reply_broadcast
                );
                assert_eq!(decoded.raw_snapshot(), &snapshot);
            }
        }
    }
    #[test]
    fn managed_fixture_does_not_claim_unsupported_native_mentions() {
        for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
            assert_eq!(
                codec::encode_composer(&key(target), &local("Hello <@U1>", target).composer),
                Err(codec::Unsupported::Entity)
            );
        }
    }
}
