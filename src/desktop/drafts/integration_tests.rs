use super::*;
use crate::{
    drafts::{session::Session, storage::Backend},
    media::MediaRegistry,
};
use super_platinum_core::{ComposerState, drafts::store::DraftStore};

const ACCOUNT: &str = "00000000-0000-4000-8000-000000000095";

#[tokio::test]
async fn legacy_guard_blocks_only_managed_keys_and_unknown_account_ownership() {
    let mut f = Fixture::new().await;
    let other = DraftKey {
        channel: "unmanaged-channel".into(),
        ..f.key.clone()
    };
    assert!(legacy_ownership(&f.shell, &f.key).is_err());
    assert!(legacy_ownership(&f.shell, &other).is_ok());
    assert!(f.shell.draft_writes.begin_send(ACCOUNT, 7, other.clone()));
    assert!(!f.shell.draft_writes.begin_send(ACCOUNT, 7, other.clone()));
    assert!(legacy_ownership(&f.shell, &other).is_err());
    f.shell.draft_writes.finish_send(ACCOUNT, 6, &other);
    assert!(legacy_ownership(&f.shell, &other).is_err());
    f.shell.draft_writes.finish_send(ACCOUNT, 7, &other);
    assert!(legacy_ownership(&f.shell, &other).is_ok());
    f.shell.managed_drafts.ready = false;
    assert!(legacy_ownership(&f.shell, &other).is_err());
    f.shell.managed_drafts.ready = true;
    f.shell.core.account_epoch += 1;
    assert!(legacy_ownership(&f.shell, &other).is_err());
    f.shell.core.account_epoch = 7;
    f.shell.core.active_account = Some("other-account".into());
    assert!(legacy_ownership(&f.shell, &other).is_err());
}
struct Fixture {
    path: std::path::PathBuf,
    account: Arc<Account>,
    key: DraftKey,
    shell: ShellState,
}
impl Fixture {
    async fn new() -> Self {
        let id = super_platinum_core::slack::api::ForwardMessage::new(
            "C1".into(),
            "100.000001".into(),
            "C2".into(),
            vec![],
        )
        .client_msg_id;
        let path = std::env::temp_dir().join(format!("draft-integration-{id}.sqlite"));
        let key = DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: None,
        };
        DraftStore::open(&path)
            .unwrap()
            .save([(key.clone(), local("Original"))])
            .unwrap();
        let account = Account::new(Backend::Isolated {
            account: ACCOUNT.into(),
            path: path.clone(),
        });
        let owner = account.owner(key.clone());
        owner.enroll(local("Original"), None).await.unwrap();
        let mut shell = ShellState::fixture(MediaRegistry::default());
        shell.core.active_account = Some(ACCOUNT.into());
        shell.core.account_epoch = 7;
        shell.managed_drafts = Session {
            account: Some(ACCOUNT.into()),
            epoch: 7,
            ready: true,
            ..Default::default()
        };
        shell
            .managed_drafts
            .apply_view(ACCOUNT, 7, key.clone(), owner.load().await.unwrap());
        shell.core.composer = ComposerState::with_text("Visible newer composer");
        shell
            .core
            .drafts
            .items
            .insert(key.clone(), local("Visible newer draft book"));
        Self {
            path,
            account,
            key,
            shell,
        }
    }
    fn submit(&mut self, text: &str) -> Arc<Editor> {
        assert!(
            route_managed(
                &mut self.shell,
                &self.account,
                [(self.key.clone(), local(text))].into()
            )
            .is_empty()
        );
        self.shell
            .draft_writes
            .writers
            .get(ACCOUNT, 7, &self.key)
            .unwrap()
    }
    fn assert_visible_untouched(&self) {
        assert_eq!(self.shell.core.composer.text, "Visible newer composer");
        assert_eq!(
            self.shell.core.drafts.items[&self.key].composer.text,
            "Visible newer draft book"
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
fn local(text: &str) -> LocalDraft {
    LocalDraft {
        composer: ComposerState::with_text(text),
        attachments: vec![],
    }
}

#[tokio::test]
async fn known_managed_input_persists_before_poll_and_poll_never_replaces_visible_input() {
    let mut f = Fixture::new().await;
    let editor = f.submit("Saved independently of UI ticks");
    editor.flush().await.unwrap();
    assert_eq!(
        f.account
            .owner(f.key.clone())
            .load()
            .await
            .unwrap()
            .local
            .composer
            .text,
        "Saved independently of UI ticks"
    );
    f.assert_visible_untouched();
    assert!(poll(&mut f.shell));
    assert!(!poll(&mut f.shell));
    assert_eq!(
        f.shell
            .managed_drafts
            .entry(ACCOUNT, &f.key)
            .unwrap()
            .local
            .composer
            .text,
        "Saved independently of UI ticks"
    );
    f.assert_visible_untouched();
}

#[tokio::test]
async fn blocked_input_stays_visible_as_metadata_without_overwriting_composer() {
    let mut f = Fixture::new().await;
    let editor = f.submit("<@unsupported-mention>");
    assert!(editor.flush().await.is_err());
    assert!(poll(&mut f.shell));
    let view = f.shell.managed_drafts.entry(ACCOUNT, &f.key).unwrap();
    assert_eq!(view.local.composer.text, "Original");
    assert_eq!(
        view.uncommitted.as_ref().unwrap().composer.text,
        "<@unsupported-mention>"
    );
    assert_eq!(
        view.blocked.as_ref().unwrap().draft.composer.text,
        "<@unsupported-mention>"
    );
    assert!(!poll(&mut f.shell));
    f.assert_visible_untouched();
}

#[tokio::test]
async fn unknown_or_stale_ownership_returns_input_to_the_legacy_durability_boundary() {
    let mut f = Fixture::new().await;
    let other = DraftKey {
        channel: "unknown-channel".into(),
        ..f.key.clone()
    };
    let remaining = route_managed(
        &mut f.shell,
        &f.account,
        [(other.clone(), local("Unknown ownership"))].into(),
    );
    assert_eq!(remaining[&other].composer.text, "Unknown ownership");
    f.shell.managed_drafts.epoch = 6;
    let remaining = route_managed(
        &mut f.shell,
        &f.account,
        [(f.key.clone(), local("Old session"))].into(),
    );
    assert_eq!(remaining[&f.key].composer.text, "Old session");
    assert!(
        f.shell
            .draft_writes
            .writers
            .get(ACCOUNT, 7, &f.key)
            .is_none()
    );
}

#[tokio::test]
async fn stale_account_poll_and_removed_writer_cannot_reintroduce_metadata() {
    let mut f = Fixture::new().await;
    let editor = f.submit("Saved old visit");
    editor.flush().await.unwrap();
    f.shell.core.account_epoch = 8;
    f.shell.managed_drafts.epoch = 8;
    assert!(!poll(&mut f.shell));
    assert_eq!(
        f.shell
            .managed_drafts
            .entry(ACCOUNT, &f.key)
            .unwrap()
            .local
            .composer
            .text,
        "Original"
    );
    f.shell.core.account_epoch = 7;
    f.shell.managed_drafts.epoch = 7;
    f.shell.draft_writes.remove(ACCOUNT, 7, &f.key);
    // Reset the Desktop scope through the public helper before removal.
    f.shell
        .draft_writes
        .editor(&f.account, 7, f.key.clone(), editor.snapshot().view)
        .unwrap();
    f.shell.draft_writes.remove(ACCOUNT, 7, &f.key);
    assert!(
        f.shell
            .draft_writes
            .writers
            .get(ACCOUNT, 7, &f.key)
            .is_none()
    );
    assert!(!poll(&mut f.shell));
    f.assert_visible_untouched();
}

#[tokio::test]
async fn released_writer_refused_input_is_visible_recovery_without_changing_current_composer() {
    let mut f = Fixture::new().await;
    let view = f.account.owner(f.key.clone()).load().await.unwrap();
    let editor = f
        .shell
        .draft_writes
        .editor(&f.account, 7, f.key.clone(), view)
        .unwrap();
    let revision = editor.release().await.unwrap();
    assert!(
        f.shell
            .managed_drafts
            .remove_entry(ACCOUNT, 7, &f.key, revision)
    );
    // Another local writer wins first. The released editor's conditional write
    // must refuse to overwrite it, but its rejected input must stay visible.
    DraftStore::open(&f.path)
        .unwrap()
        .save([(f.key.clone(), local("Other saved local input"))])
        .unwrap();
    editor.submit(local("Retained released input"));
    assert!(editor.flush().await.is_err());
    assert!(editor.snapshot().view.stored.is_none());
    assert!(poll(&mut f.shell));
    assert!(
        f.shell
            .draft_writes
            .writers
            .get(ACCOUNT, 7, &f.key)
            .is_some()
    );
    let view = f.shell.managed_drafts.entry(ACCOUNT, &f.key).unwrap();
    assert!(view.stored.is_none());
    assert_eq!(
        view.uncommitted.as_ref().unwrap().composer.text,
        "Retained released input"
    );
    assert!(
        crate::drafts::session::recovery_rows(&f.shell)
            .iter()
            .any(|row| {
                row.key == f.key
                    && row
                        .local
                        .as_ref()
                        .is_some_and(|local| local.composer.text == "Retained released input")
            })
    );
    assert!(!poll(&mut f.shell));
    f.assert_visible_untouched();
    f.shell.core.account_epoch = 8;
    f.shell.managed_drafts.epoch = 8;
    assert!(!poll(&mut f.shell));
}

#[tokio::test]
async fn idle_successful_released_writer_is_retired_without_removing_new_managed_metadata() {
    for reenrolled in [false, true] {
        let mut f = Fixture::new().await;
        let owner = f.account.owner(f.key.clone());
        let editor = f
            .shell
            .draft_writes
            .editor(&f.account, 7, f.key.clone(), owner.load().await.unwrap())
            .unwrap();
        let revision = editor.release().await.unwrap();
        assert!(
            f.shell
                .managed_drafts
                .remove_entry(ACCOUNT, 7, &f.key, revision)
        );
        if reenrolled {
            owner.enroll(local("Original"), None).await.unwrap();
            f.shell.managed_drafts.apply_view(
                ACCOUNT,
                7,
                f.key.clone(),
                owner.load().await.unwrap(),
            );
        } else {
            f.shell.managed_drafts.apply_local_recovery(
                ACCOUNT,
                7,
                f.key.clone(),
                local("Original"),
            );
        }
        f.shell.draft_writes.begin_send(ACCOUNT, 7, f.key.clone());
        // Even an identical cached signature must not hide released cleanup.
        f.shell
            .draft_writes
            .signatures
            .insert(f.key.clone(), Signature::from(&editor.snapshot()));
        poll(&mut f.shell);
        assert!(
            f.shell
                .draft_writes
                .writers
                .get(ACCOUNT, 7, &f.key)
                .is_none()
        );
        assert!(f.shell.draft_writes.sending.contains(&f.key));
        assert_eq!(
            f.shell.managed_drafts.entry(ACCOUNT, &f.key).is_some(),
            reenrolled
        );
        assert!(!poll(&mut f.shell));
        f.assert_visible_untouched();
    }
}

#[tokio::test]
async fn released_writer_success_preserves_independent_recovery_evidence() {
    use super_platinum_core::drafts::store::sync::{BlockedLocal, LocalAlternate};
    for matching in [false, true] {
        let mut f = Fixture::new().await;
        let owner = f.account.owner(f.key.clone());
        let editor = f
            .shell
            .draft_writes
            .editor(&f.account, 7, f.key.clone(), owner.load().await.unwrap())
            .unwrap();
        let revision = editor.release().await.unwrap();
        assert!(
            f.shell
                .managed_drafts
                .remove_entry(ACCOUNT, 7, &f.key, revision)
        );
        let pending = if matching {
            "Original"
        } else {
            "Newer independent input"
        };
        assert!(f.shell.managed_drafts.apply_local_recovery(
            ACCOUNT,
            7,
            f.key.clone(),
            local(pending)
        ));
        let identity = (ACCOUNT.to_owned(), f.key.clone());
        f.shell
            .managed_drafts
            .entries
            .get_mut(&identity)
            .unwrap()
            .blocked = Some(BlockedLocal {
            token: "independent-blocked".into(),
            draft: local("First refused version"),
        });
        f.shell.managed_drafts.local_alternates.insert(
            identity.clone(),
            vec![LocalAlternate {
                writer_id: "00000000-0000-4000-8000-000000000096".into(),
                revision: 9,
                draft: local("Other writer version"),
            }],
        );
        f.shell
            .draft_writes
            .signatures
            .insert(f.key.clone(), Signature::from(&editor.snapshot()));
        assert_eq!(poll(&mut f.shell), matching);
        assert!(
            f.shell
                .draft_writes
                .writers
                .get(ACCOUNT, 7, &f.key)
                .is_none()
        );
        let view = f.shell.managed_drafts.entry(ACCOUNT, &f.key).unwrap();
        assert_eq!(
            view.uncommitted
                .as_ref()
                .map(|local| local.composer.text.as_str()),
            (!matching).then_some(pending)
        );
        assert_eq!(
            view.blocked.as_ref().unwrap().draft.composer.text,
            "First refused version"
        );
        assert_eq!(
            f.shell.managed_drafts.local_alternates[&identity][0]
                .draft
                .composer
                .text,
            "Other writer version"
        );
        f.assert_visible_untouched();
    }
}
