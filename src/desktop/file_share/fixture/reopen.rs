//! Separate existing-DB authority. The ordinary fresh constructor is unchanged.
use super::*;
use super_platinum_core::file_sharing::{
    store::Journal,
    store::drafts::{Draft, DraftScope},
};
mod hash;
mod model;
use model::{ExitAudit, Handoff, Marker, Pin};
pub(in crate::file_share) use model::{Input, Variant};

pub(in crate::file_share) struct Seal {
    pub run: String,
    pub nonce: String,
    pub variant: Variant,
    pub input: Input,
    pub database_sha256: String,
    pub marker_sha256: String,
    pub source_sha256: String,
    pub executable_sha256: String,
    pub build_sha256: String,
    pub draft: Draft,
}
fn plain(path: &Path, directory: bool) -> Result<std::fs::Metadata, Rejected> {
    let m = std::fs::symlink_metadata(path).map_err(|_| Rejected::Path)?;
    if redirected(&m) || (directory && !m.is_dir()) || (!directory && !m.is_file()) {
        return Err(Rejected::Path);
    }
    Ok(m)
}
fn absent(path: &Path) -> Result<(), Rejected> {
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        _ => Err(Rejected::Path),
    }
}
fn local_absolute(path: &Path) -> bool {
    use std::path::{Component, Prefix};
    path.is_absolute()
        && matches!(path.components().next(), Some(Component::Prefix(p)) if matches!(p.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)))
        && !path
            .components()
            .any(|p| matches!(p, Component::ParentDir | Component::CurDir))
}
fn sidecars(path: &Path, main_absent: bool) -> Result<(), Rejected> {
    for suffix in if main_absent {
        vec!["", "-wal", "-shm", "-journal"]
    } else {
        vec!["-wal", "-shm", "-journal"]
    } {
        let mut name = path.as_os_str().to_os_string();
        name.push(suffix);
        absent(&PathBuf::from(name))?;
    }
    Ok(())
}
fn pinned(path: &Path, pin: &Pin, limit: u64) -> Result<(), Rejected> {
    let m = plain(path, false)?;
    if pin.size == 0
        || pin.size > limit
        || m.len() != pin.size
        || !model::sha(&pin.sha256)
        || hash::file(path, limit)? != pin.sha256
    {
        return Err(Rejected::Path);
    }
    Ok(())
}
fn read<T: serde::de::DeserializeOwned>(path: &Path, limit: u64) -> Result<T, Rejected> {
    if plain(path, false)?.len() > limit {
        return Err(Rejected::Path);
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| Rejected::Path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Rejected::Path)?;
    if bytes.len() as u64 > limit {
        return Err(Rejected::Path);
    }
    serde_json::from_slice(&bytes).map_err(|_| Rejected::Path)
}
pub(in crate::file_share) fn load(shell: &ShellState) -> Result<Capability, Rejected> {
    let env = |key| std::env::var(key).map_err(|_| Rejected::Gate);
    if !crate::fixture::is_fixture()
        || env("SUPER_PLATINUM_FIXTURE")? != "file-share-draft-reopen"
        || env("SUPER_PLATINUM_FILE_SHARE_CHECK")? != "1"
        || env("SUPER_PLATINUM_FILE_SHARE_CHECK_MODE")? != "durable-reopen"
        || env("SUPER_PLATINUM_FILE_SHARE_CASE")? != "accepted"
        || !disconnected_owner(shell)
    {
        return Err(Rejected::Gate);
    }
    let ack = PathBuf::from(env("SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR")?);
    if !local_absolute(&ack)
        || ack.file_name().and_then(|s| s.to_str()) != Some("reopen")
        || ack.components().any(|p| {
            matches!(
                p,
                std::path::Component::ParentDir | std::path::Component::CurDir
            )
        })
    {
        return Err(Rejected::Path);
    }
    for path in ack.ancestors() {
        plain(path, true)?;
    }
    let ack = ack.canonicalize().map_err(|_| Rejected::Path)?;
    let pair = ack.parent().ok_or(Rejected::Path)?;
    let capture = pair.join("capture");
    plain(&capture, true)?;
    let journal = capture.join("journal");
    plain(&journal, true)?;
    let database = journal.join("file-share.sqlite");
    let saved = ack.join("saved-file-recovery.sqlite");
    // Compare canonical parents rather than accepting another path to same bytes.
    for (key, expected) in [
        ("SUPER_PLATINUM_FILE_SHARE_JOURNAL", &database),
        ("SUPER_PLATINUM_SAVED_FILE_RECOVERY_DB", &saved),
    ] {
        let path = PathBuf::from(env(key)?);
        if !local_absolute(&path)
            || path.components().any(|p| {
                matches!(
                    p,
                    std::path::Component::ParentDir | std::path::Component::CurDir
                )
            })
            || path.file_name() != expected.file_name()
            || path.parent().and_then(|p| p.canonicalize().ok()).as_deref() != expected.parent()
        {
            return Err(Rejected::Path);
        }
        // SavedControl subsequently consumes this environment path itself.
        // Reject redirected supplied ancestors, even when canonical bytes agree.
        for ancestor in path.parent().ok_or(Rejected::Path)?.ancestors() {
            plain(ancestor, true)?;
        }
    }
    sidecars(&database, false)?;
    sidecars(&saved, true)?;
    let marker_path = ack.join("reopen-marker.json");
    plain(&marker_path, false)?;
    let expected = env("SUPER_PLATINUM_FILE_SHARE_REOPEN_MARKER_SHA256")?;
    if !model::sha(&expected) || hash::file(&marker_path, 16384)? != expected {
        return Err(Rejected::Gate);
    }
    let marker: Marker = read(&marker_path, 16384)?;
    if marker.nonce != env("SUPER_PLATINUM_FILE_SHARE_NONCE")?
        || pair.file_name().and_then(|s| s.to_str()) != Some(marker.run.as_str())
    {
        return Err(Rejected::Gate);
    }
    let handoff_path = pair.join("handoff.json");
    plain(&handoff_path, false)?;
    if hash::file(&handoff_path, 16384)? != marker.handoff_sha256 {
        return Err(Rejected::Gate);
    }
    let handoff: Handoff = read(&handoff_path, 16384)?;
    let audit_path = pair.join("process-1-audit.json");
    pinned(&audit_path, &handoff.audit, 16384)?;
    let audit: ExitAudit = read(&audit_path, 16384)?;
    if !model::valid(&marker, &handoff, &audit) {
        return Err(Rejected::Gate);
    }
    let executable = std::env::current_exe().map_err(|_| Rejected::Gate)?;
    if hash::file(&executable, 1024 * 1024 * 1024)? != marker.executable_sha256 {
        return Err(Rejected::Gate);
    }
    pinned(&database, &handoff.database, 16 * 1024 * 1024)?;
    let draft_path = pair.join("closed-draft.json");
    pinned(&draft_path, &handoff.draft, 1024 * 1024)?;
    let expected_draft: Draft = read(&draft_path, 1024 * 1024)?;
    let canonical: super_platinum_core::slack::models::File =
        serde_json::from_value(source_file()).map_err(|_| Rejected::Owner)?;
    let note = match marker.input {
        Input::Text => super::super::check::expected_note(),
        Input::DestinationOnly => json!([]),
    };
    if expected_draft.scope
        != (DraftScope {
            team: TEAM.into(),
            user: USER.into(),
            file: FILE.into(),
        })
        || expected_draft.revision != 2
        || expected_draft.content.source
            != serde_json::to_value(canonical).map_err(|_| Rejected::Owner)?
        || json!(expected_draft.content.note) != note
        || Some(expected_draft.content.title.as_str()) != source_file()["title"].as_str()
        || expected_draft
            .content
            .destination
            .as_ref()
            .is_none_or(|d| d.id != DESTINATION || d.name != "share-fixture" || d.private)
    {
        return Err(Rejected::Owner);
    }
    // Actual core validation uses the captured private path after hash-bound
    // schema2/identity/inventory preflight. No default path or repair fallback.
    let journal = Journal::open(&database, ACCOUNT).map_err(|_| Rejected::Path)?;
    if journal.drafts(TEAM, USER).map_err(|_| Rejected::Path)? != vec![expected_draft.clone()]
        || !journal
            .unresolved(TEAM, USER)
            .map_err(|_| Rejected::Path)?
            .is_empty()
        || journal.retry_at(TEAM, USER).map_err(|_| Rejected::Path)? != 0
    {
        return Err(Rejected::Owner);
    }
    drop(journal);
    let seal = Seal {
        run: marker.run,
        nonce: marker.nonce,
        variant: marker.variant,
        input: marker.input,
        database_sha256: handoff.database.sha256,
        marker_sha256: expected,
        source_sha256: marker.source_sha256,
        executable_sha256: marker.executable_sha256,
        build_sha256: marker.build_sha256,
        draft: expected_draft,
    };
    Ok(Capability(Arc::new(Inner {
        paths: Paths {
            ack,
            database,
            saved_database: saved,
        },
        conversation: Conversation::Channel,
        restart: Some(seal),
        case: Case::Accepted,
        epoch: shell.core.account_epoch,
        generation: shell
            .core
            .workspaces
            .get(TEAM)
            .ok_or(Rejected::Owner)?
            .rt_generation,
        ledger: Mutex::new(Ledger::default()),
    })))
}

#[cfg(test)]
mod tests;
