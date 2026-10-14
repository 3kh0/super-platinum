use super::*;
use crate::state::{MainView, Overlay};
use super_platinum_core::{ComposerState, scheduled::DraftFilter};

fn fixture() -> (ShellState, Operation) {
    let mut shell =
        ShellState::fixture_variant(crate::media::MediaRegistry::default(), "drafts-empty");
    let scope = Scope {
        account_id: "00000000-0000-4000-8000-000000000091".into(),
        key: DraftKey {
            team: "T1".into(),
            channel: "C2".into(),
            thread: None,
        },
    };
    let base = RemoteDraft {
        scope: scope.clone(),
        id: "DrUpdate".into(),
        client_msg_id: "00000000-0000-4000-8000-000000000092".into(),
        last_updated_ts: "10.000001".into(),
        snapshot: codec::encode_composer(&scope.key, &ComposerState::with_text("before")).unwrap(),
    };
    let mut sync = DraftSync::adopt_remote(&scope, base).unwrap();
    let local = LocalDraft {
        composer: ComposerState::with_text("after"),
        attachments: vec![],
    };
    sync.edit(
        &scope,
        Some(codec::encode_composer(&scope.key, &local.composer).unwrap()),
    );
    let op = sync.begin_operation(&scope).unwrap();
    assert_eq!(op.kind, OperationKind::Update);
    assert!(sync.mark_uncertain(&scope, &op.id));
    shell.core.active_account = Some(scope.account_id.clone());
    shell.core.active_team = Some(scope.key.team.clone());
    shell.core.account_epoch = 7;
    shell.managed_drafts = session::Session {
        account: Some(scope.account_id.clone()),
        epoch: 7,
        ready: true,
        ..Default::default()
    };
    assert!(shell.managed_drafts.apply_view(
        &scope.account_id,
        7,
        scope.key,
        View {
            local,
            stored: Some(Stored {
                revision: 1,
                state: sync
            }),
            blocked: None,
            uncommitted: None,
        }
    ));
    shell.main_view = MainView::Scheduled;
    shell.core.scheduled.filter = DraftFilter::Drafts;
    shell.thread_root = None;
    shell.overlay = None;
    (shell, op)
}

#[test]
fn unresolved_update_suppresses_toast_only_after_live_inline_publication() {
    let (shell, op) = fixture();
    let mut shown = None;
    assert!(inline_update_feedback(
        &shell,
        &op,
        7,
        false,
        "cooldown",
        &mut |text| {
            shown = Some(text);
            true
        }
    ));
    assert_eq!(shown, Some("cooldown"));
    // An identical row can exist after navigation remounts it, while the
    // originating component's signal is gone. Its failed publication must
    // retain the toast, rather than assuming the new row owns the feedback.
    let mut attempts = 0;
    assert!(!inline_update_feedback(
        &shell,
        &op,
        7,
        false,
        "cooldown",
        &mut |_| {
            attempts += 1;
            false
        }
    ));
    assert_eq!(attempts, 1);
}

#[test]
fn confirmation_and_changed_operation_keep_notification() {
    let (mut shell, op) = fixture();
    let mut must_not_publish = |_| panic!("Feedback row is not applicable");
    assert!(!inline_update_feedback(
        &shell,
        &op,
        7,
        true,
        "confirmed",
        &mut must_not_publish
    ));
    let mut different = op.clone();
    different.client_msg_id.push('x'); // same id is insufficient
    assert!(!inline_update_feedback(
        &shell,
        &different,
        7,
        false,
        "result",
        &mut must_not_publish
    ));
    different = op.clone();
    different.kind = OperationKind::Send;
    assert!(!inline_update_feedback(
        &shell,
        &different,
        7,
        false,
        "result",
        &mut must_not_publish
    ));
    shell.managed_drafts.entries.clear();
    assert!(!inline_update_feedback(
        &shell,
        &op,
        7,
        false,
        "result",
        &mut must_not_publish
    ));
}

#[test]
fn navigation_filter_and_covering_surfaces_keep_notification() {
    for change in [
        (|s: &mut ShellState| s.main_view = MainView::Home) as fn(&mut ShellState),
        |s| s.core.scheduled.filter = DraftFilter::Scheduled,
        |s| s.core.active_team = Some("T2".into()),
        |s| s.thread_root = Some("100.000001".into()),
        |s| s.overlay = Some(Overlay::Search),
        |s| s.show_profile_hover("U1".into(), 100.0, 100.0),
    ] {
        let (mut shell, op) = fixture();
        change(&mut shell);
        assert!(!inline_update_feedback(
            &shell,
            &op,
            7,
            false,
            "result",
            &mut |_| panic!("Hidden row received feedback")
        ));
    }
}

#[test]
fn owner_change_or_stale_projection_cannot_suppress_notification() {
    for change in [
        (|s: &mut ShellState| s.core.active_account = Some("other".into())) as fn(&mut ShellState),
        |s| s.core.account_epoch += 1,
        |s| s.managed_drafts.account = Some("other".into()),
        |s| s.managed_drafts.epoch += 1,
    ] {
        let (mut shell, op) = fixture();
        change(&mut shell);
        assert!(!inline_update_feedback(
            &shell,
            &op,
            7,
            false,
            "result",
            &mut |_| panic!("Stale owner received feedback")
        ));
    }
}
