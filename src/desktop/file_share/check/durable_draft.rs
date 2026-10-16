//! Read actual private draft rows; never claim, save, delete, or alter outcomes.
use super::*;
use super_platinum_core::file_sharing::store::{Journal, drafts::Draft};

#[derive(Clone, PartialEq, serde::Serialize)]
pub(super) struct Snapshot {
    pub present: bool,
    pub drafts: Vec<Draft>,
}
pub(super) async fn read(cap: &fixture::Capability) -> Result<Snapshot, String> {
    let path = cap.storage_path().to_path_buf();
    tokio::task::spawn_blocking(move || -> Result<Snapshot, String> {
        match std::fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                for suffix in ["-wal", "-shm", "-journal"] {
                    let mut name = path.as_os_str().to_os_string();
                    name.push(suffix);
                    match std::fs::symlink_metadata(std::path::PathBuf::from(name)) {
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                        _ => return Err("Draft sidecar without database or unreadable path".into()),
                    }
                }
                return Ok(Snapshot {
                    present: false,
                    drafts: vec![],
                });
            }
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => (),
            _ => return Err("Draft database missing or redirected".into()),
        }
        let journal =
            Journal::open(path, fixture::ACCOUNT).map_err(|_| "Draft read open failed")?;
        let drafts = journal
            .drafts(fixture::TEAM, fixture::USER)
            .map_err(|_| "Draft read failed")?;
        Ok(Snapshot {
            present: true,
            drafts,
        })
    })
    .await
    .map_err(|_| "Draft read worker failed")?
}
// Claims are generated as canonical lowercase UUIDv4 tokens by the core store.
fn token(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        })
        && value.as_bytes()[14] == b'4'
        && matches!(value.as_bytes()[19], b'8' | b'9' | b'a' | b'b')
}
pub(super) fn one(snapshot: &Snapshot) -> Result<&Draft, String> {
    if !snapshot.present || snapshot.drafts.len() != 1 {
        return Err("Expected exactly one real durable draft".into());
    }
    let row = &snapshot.drafts[0];
    let file: super_platinum_core::slack::models::File =
        serde_json::from_value(fixture::source_file()).map_err(|_| "Fixture source shape")?;
    if row.scope.team != fixture::TEAM
        || row.scope.user != fixture::USER
        || row.scope.file != fixture::FILE
        || row.revision == 0
        || row.content.source != serde_json::to_value(&file).map_err(|_| "Source encoding")?
        || Some(row.content.title.as_str()) != file.title.as_deref()
        || !token(&row.token)
    {
        return Err("Draft source/scope/token differs from canonical fixture".into());
    }
    Ok(row)
}
pub(super) fn saved_from(claim: &Draft, saved: &Draft, note: &Value) -> bool {
    saved.scope == claim.scope
        && saved.token == claim.token
        && claim.revision.checked_add(1) == Some(saved.revision)
        && saved.content.source == claim.content.source
        && saved.content.title == claim.content.title
        && json!(saved.content.note) == *note
        && saved.content.destination.as_ref().is_some_and(|d| {
            d.id == fixture::DESTINATION && d.name == "share-fixture" && !d.private
        })
}
pub(super) fn reclaimed(saved: &Draft, reopened: &Draft) -> bool {
    reopened.scope == saved.scope
        && reopened.content == saved.content
        && reopened.token != saved.token
        && saved.revision.checked_add(1) == Some(reopened.revision)
}
#[derive(Default)]
pub(super) struct Oracle {
    claim: Option<Draft>,
    saved: Option<Draft>,
}
impl Oracle {
    pub(super) fn observe(
        &mut self,
        stage: &str,
        case: fixture::Case,
        snapshot: &Snapshot,
        note: &Value,
    ) -> Result<(), String> {
        if stage == "menu" {
            return if !snapshot.present && snapshot.drafts.is_empty() {
                Ok(())
            } else {
                Err("Database existed before explicit Open".into())
            };
        }
        if stage == "dialog" {
            let claim = one(snapshot)?;
            if claim.revision != 1 || !claim.content.empty() {
                return Err("Open did not claim exact empty row".into());
            }
            self.claim = Some(claim.clone());
            return Ok(());
        }
        let claim = self.claim.as_ref().ok_or("Missing original claim")?;
        if stage == "note-ready" {
            return if one(snapshot)? == claim {
                Ok(())
            } else {
                Err("Typing saved before explicit Share".into())
            };
        }
        if matches!(stage, "sending" | "begun") {
            let saved = one(snapshot)?;
            if !saved_from(claim, saved, note) {
                return Err("Begin did not use exact committed note".into());
            }
            self.saved = Some(saved.clone());
            return Ok(());
        }
        if case == fixture::Case::Accepted && stage == "final" {
            let fresh = one(snapshot)?;
            return if fresh.token != claim.token && fresh.revision == 1 && fresh.content.empty() {
                Ok(())
            } else {
                Err("Accepted reopen reused submitted draft".into())
            };
        }
        if case == fixture::Case::Accepted || matches!(stage, "retry-saved" | "retry-recovery") {
            return if snapshot.present && snapshot.drafts.is_empty() {
                Ok(())
            } else {
                Err("Accepted did not remove exact submitted draft".into())
            };
        }
        if self.saved.as_ref() != Some(one(snapshot)?) {
            return Err("Nonaccepted outcome lost or changed saved draft".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn claimed_token_requires_canonical_uuid_v4_shape() {
        assert!(token("d5e65110-3c95-4aad-989e-f01ea6cb5bc2"));
        for invalid in [
            "",
            "d5e651103c954aad989ef01ea6cb5bc2",
            "D5E65110-3C95-4AAD-989E-F01EA6CB5BC2",
            "d5e65110-3c95-1aad-989e-f01ea6cb5bc2",
            "d5e65110-3c95-4aad-789e-f01ea6cb5bc2",
            "d5e65110-3c95-4aad-989e-f01ea6cb5bcg",
        ] {
            assert!(!token(invalid));
        }
    }
    fn claim() -> Draft {
        let file: super_platinum_core::slack::models::File =
            serde_json::from_value(fixture::source_file()).unwrap();
        serde_json::from_value(json!({"scope":{"team":fixture::TEAM,"user":fixture::USER,"file":fixture::FILE},
            "token":"d5e65110-3c95-4aad-989e-f01ea6cb5bc2","revision":1,
            "content":{"source":serde_json::to_value(&file).unwrap(),"title":file.title.unwrap(),"note":[],"destination":null}})).unwrap()
    }
    #[test]
    fn saved_transition_refuses_changed_source_token_revision_and_note() {
        let claim = claim();
        let mut saved = claim.clone();
        saved.revision = 2;
        saved.content.note = serde_json::from_value(expected_note()).unwrap();
        saved.content.destination = Some(
            super_platinum_core::file_sharing::store::drafts::Destination {
                id: fixture::DESTINATION.into(),
                name: "share-fixture".into(),
                private: false,
            },
        );
        assert!(saved_from(&claim, &saved, &expected_note()));
        for n in 0..5 {
            let mut bad = saved.clone();
            match n {
                0 => bad.token = "44764c2b-f704-4347-a5e6-a98b93e698c0".into(),
                1 => bad.revision = 3,
                2 => bad.content.source["title"] = json!("changed"),
                3 => bad.content.note.clear(),
                _ => bad.content.destination.as_mut().unwrap().private = true,
            }
            assert!(!saved_from(&claim, &bad, &expected_note()));
        }
        let mut reopen = saved.clone();
        reopen.token = "44764c2b-f704-4347-a5e6-a98b93e698c0".into();
        reopen.revision += 1;
        assert!(reclaimed(&saved, &reopen));
        assert!(!reclaimed(&saved, &saved));
        reopen.content.note.clear();
        assert!(!reclaimed(&saved, &reopen));
    }
    #[test]
    fn exact_claim_and_reopen_fences_refuse_early_save_scope_change_and_old_token() {
        let c = claim();
        let snapshot = Snapshot {
            present: true,
            drafts: vec![c.clone()],
        };
        let mut oracle = Oracle::default();
        assert!(
            oracle
                .observe("menu", fixture::Case::Accepted, &snapshot, &expected_note())
                .is_err()
        );
        assert!(
            oracle
                .observe(
                    "dialog",
                    fixture::Case::Accepted,
                    &snapshot,
                    &expected_note()
                )
                .is_ok()
        );
        assert!(
            oracle
                .observe(
                    "note-ready",
                    fixture::Case::Accepted,
                    &snapshot,
                    &expected_note()
                )
                .is_ok()
        );
        let mut changed = snapshot.clone();
        changed.drafts[0].content.note = serde_json::from_value(expected_note()).unwrap();
        assert!(
            oracle
                .observe(
                    "note-ready",
                    fixture::Case::Accepted,
                    &changed,
                    &expected_note()
                )
                .is_err()
        );
        changed = snapshot.clone();
        changed.drafts[0].scope.user = "U00000002".into();
        assert!(one(&changed).is_err());
        assert!(
            oracle
                .observe(
                    "final",
                    fixture::Case::Accepted,
                    &snapshot,
                    &expected_note()
                )
                .is_err()
        );
        changed = snapshot.clone();
        changed.drafts[0].token = "44764c2b-f704-4347-a5e6-a98b93e698c0".into();
        assert!(
            oracle
                .observe("final", fixture::Case::Accepted, &changed, &expected_note())
                .is_ok()
        );
        changed.drafts[0].revision = 2;
        assert!(
            oracle
                .observe("final", fixture::Case::Accepted, &changed, &expected_note())
                .is_err()
        );
    }
    #[test]
    fn accepted_cleanup_refuses_retained_row_and_nonaccepted_requires_exact_saved_row() {
        let c = claim();
        let mut oracle = Oracle {
            claim: Some(c.clone()),
            saved: Some(c.clone()),
        };
        let present = Snapshot {
            present: true,
            drafts: vec![c],
        };
        let empty = Snapshot {
            present: true,
            drafts: vec![],
        };
        assert!(
            oracle
                .observe(
                    "outcome",
                    fixture::Case::Accepted,
                    &present,
                    &expected_note()
                )
                .is_err()
        );
        assert!(
            oracle
                .observe("outcome", fixture::Case::Accepted, &empty, &expected_note())
                .is_ok()
        );
        assert!(
            oracle
                .observe(
                    "outcome",
                    fixture::Case::Unconfirmed,
                    &empty,
                    &expected_note()
                )
                .is_err()
        );
        assert!(
            oracle
                .observe(
                    "outcome",
                    fixture::Case::Unconfirmed,
                    &present,
                    &expected_note()
                )
                .is_ok()
        );
    }
}
