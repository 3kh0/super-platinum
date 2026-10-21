use super::*;
use crate::profile_edit::sections::rich::search::channel_host::Host;
use crate::view::slack_lists::comments::bridge;
use serde_json::json;

fn setup() -> (ShellState, draft::Binding, draft::tests::Local) {
    let (mut shell, mut binding, local) = draft::tests::setup();
    // The production cache uses an actual account. Bind the existing test visit
    // to that synthetic account before installing any native/query observation.
    shell.core.active_account = Some("00000000-0000-4000-8000-000000001999".into());
    let scope = Scope::capture(&shell).unwrap();
    shell.slack_lists.scope = Some(scope.clone());
    shell
        .slack_lists
        .comments
        .entry
        .as_mut()
        .unwrap()
        .owner
        .scope = scope.clone();
    binding.owner.scope = scope;
    shell.slack_lists.comments.drafts.active = Some(binding.clone());
    let native = bridge::test_mounted(&binding, "original-native");
    assert!(draft::install_native(&mut shell, &binding, native));
    assert!(binding.current(&shell));
    (shell, binding, local)
}
#[test]
fn original_mount_sequence_and_close_never_adopt_successor() {
    let (mut shell, binding, _local) = setup();
    let old = begin(&mut shell, &binding, "original-native", 1).unwrap();
    for seq in [0, 1, MAX_SAFE, u64::MAX] {
        assert!(begin(&mut shell, &binding, "original-native", seq).is_none());
    }
    assert!(begin(&mut shell, &binding, "replacement", 2).is_none());
    assert!(old.current(&shell));
    let next = begin(&mut shell, &binding, "original-native", 2).unwrap();
    assert!(!old.current(&shell));
    assert!(!close(&mut shell, &old));
    assert!(next.current(&shell));
    assert!(close(&mut shell, &next));
    assert!(!next.current(&shell));
    assert!(begin(&mut shell, &binding, "original-native", 2).is_none());
}
#[test]
fn account_socket_root_busy_composition_and_native_retirement_refuse_original() {
    for mutation in 0..7 {
        let (mut shell, binding, _local) = setup();
        let lease = begin(&mut shell, &binding, "original-native", 1).unwrap();
        let blocks = draft::entry(&shell, binding.id()).unwrap().blocks.clone();
        match mutation {
            0 => shell.core.account_epoch += 1,
            1 => {
                shell
                    .core
                    .workspaces
                    .get_mut(&lease.team)
                    .unwrap()
                    .rt_generation += 1
            }
            2 => {
                let old = shell
                    .slack_lists
                    .comments
                    .entry
                    .as_ref()
                    .unwrap()
                    .target
                    .clone();
                let mut raw = old.record_raw().clone();
                raw["thread_ts"] = json!("1800000000.000001");
                shell.slack_lists.comments.entry.as_mut().unwrap().target = old
                    .refresh(
                        &json!({"ok":true,"file":old.document_raw()}),
                        &json!({"ok":true,"record":raw}),
                        false,
                    )
                    .unwrap();
            }
            3 => draft::entry_mut(&mut shell, binding.id()).unwrap().busy = true,
            4 => {
                draft::entry_mut(&mut shell, binding.id())
                    .unwrap()
                    .composing = true
            }
            5 => draft::native_done(&mut shell, &binding, "original-native", true),
            _ => controller::close_shell(&mut shell),
        }
        assert!(!lease.current(&shell), "mutation {mutation}");
        assert!(choose(&mut shell, &lease, 1, 0).is_none());
        assert_eq!(draft::entry(&shell, binding.id()).unwrap().blocks, blocks);
    }
}
#[tokio::test]
async fn original_held_reader_wakes_on_replacement_and_close() {
    for replace in [false, true] {
        let (mut shell, binding, _local) = setup();
        let old = begin(&mut shell, &binding, "original-native", 1).unwrap();
        if replace {
            begin(&mut shell, &binding, "original-native", 2).unwrap();
        } else {
            close(&mut shell, &old);
        }
        tokio::time::timeout(std::time::Duration::from_millis(100), old.cancelled())
            .await
            .unwrap();
    }
}
#[test]
fn displayed_rows_without_private_ranked_receipt_never_authorize_insertion() {
    let (mut shell, binding, _local) = setup();
    let lease = begin(&mut shell, &binding, "original-native", 1).unwrap();
    let q = shell
        .slack_lists
        .comments
        .completion
        .query
        .as_mut()
        .unwrap();
    q.revision = 1;
    q.values.push(Completion {
        value: json!({"type":"channel","channel_id":"C1"}),
        label: "#forged".into(),
    });
    assert!(choose(&mut shell, &lease, 1, 0).is_none());
    assert!(lease.current(&shell));
}
#[test]
fn cooldown_is_account_team_bound_and_stale_owner_cannot_extend_it() {
    let (mut shell, binding, _local) = setup();
    let lease = begin(&mut shell, &binding, "original-native", 1).unwrap();
    assert!(lease.can_dispatch(&shell));
    lease.retain_failure(
        &mut shell,
        &Error::RateLimited {
            retry_after_secs: Some(60),
        },
    );
    assert!(lease.current(&shell));
    assert!(!lease.can_dispatch(&shell));
    let before = shell.slack_lists.comments.completion.retry.clone();
    shell.core.account_epoch += 1;
    lease.retain_failure(
        &mut shell,
        &Error::RateLimited {
            retry_after_secs: Some(600),
        },
    );
    assert_eq!(shell.slack_lists.comments.completion.retry, before);
}
fn with_policy() -> (ShellState, draft::Binding, Lease, draft::tests::Local) {
    let (mut shell, binding, local) = setup();
    let lease = begin(&mut shell, &binding, "original-native", 1).unwrap();
    let prefs = Preferences::fixture(
        Arc::new(crate::scheduled::Context::capture(&shell).unwrap()),
        lease.account.clone(),
        shell.core.session.as_ref().unwrap().d_cookie.clone(),
        crate::app_home::autocomplete::preferences_revision(&shell),
        json!({"ok":true,"assignments":{"friendly_channel_names":{"group":"on"}}}),
        json!({"ok":true,"prefs":{}}),
    )
    .unwrap();
    assert!(prefs.friendly_channels().unwrap());
    shell.slack_lists.comments.completion.preferences = Some(prefs.clone());
    shell
        .slack_lists
        .comments
        .completion
        .query
        .as_mut()
        .unwrap()
        .policy = Some(prefs);
    assert!(lease.current(&shell));
    (shell, binding, lease, local)
}
#[test]
fn actual_preference_event_retires_list_policy_and_refuses_foreign_owners() {
    use super_platinum_core::slack::events::RtEvent;
    for case in 0..5 {
        let (mut shell, binding, lease, _local) = with_policy();
        let before = crate::app_home::autocomplete::preferences_revision(&shell);
        let blocks = draft::entry(&shell, binding.id()).unwrap().blocks.clone();
        let event = RtEvent::PreferencesChanged {
            name: "display_real_names".into(),
            value: json!(true),
        };
        let team = if case == 1 { "T_FOREIGN" } else { &lease.team };
        let generation = lease.socket + u64::from(case == 2);
        if case == 3 {
            shell.core.account_epoch += 1;
        }
        if case == 4 {
            shell.core.active_account = Some("foreign-account".into());
        }
        crate::app_home::realtime(&mut shell, team, generation, &event);
        assert_eq!(
            crate::app_home::autocomplete::preferences_revision(&shell),
            before + u64::from(case == 0),
            "case {case}"
        );
        if case == 0 {
            assert!(!lease.current(&shell));
            assert!(choose(&mut shell, &lease, 1, 0).is_none());
            assert!(
                shell
                    .slack_lists
                    .comments
                    .completion
                    .preferences
                    .as_ref()
                    .is_some_and(|p| !p.current(&shell))
            );
        } else if case < 3 {
            assert!(lease.current(&shell));
        }
        assert_eq!(draft::entry(&shell, binding.id()).unwrap().blocks, blocks);
    }
}
#[test]
fn accepted_assignment_event_invalidates_reusable_policy_between_caret_queries() {
    use super_platinum_core::slack::events::RtEvent;
    let (mut shell, _binding, lease, _local) = with_policy();
    assert!(close(&mut shell, &lease));
    assert!(!lease.current(&shell));
    let before = crate::app_home::autocomplete::preferences_revision(&shell);
    let event = RtEvent::Unknown(
        serde_json::from_value(
            json!({"type":"team_pref_change","name":"friendly_channel_names","value":"off"}),
        )
        .unwrap(),
    );
    crate::app_home::realtime(&mut shell, &lease.team, lease.socket, &event);
    assert_eq!(
        crate::app_home::autocomplete::preferences_revision(&shell),
        before + 1
    );
    assert!(
        shell
            .slack_lists
            .comments
            .completion
            .preferences
            .as_ref()
            .is_some_and(|p| !p.current(&shell))
    );
}

#[test]
fn accepted_preference_event_during_composition_or_busy_cannot_revive_old_policy() {
    use super_platinum_core::slack::events::RtEvent;
    for composing in [false, true] {
        let (mut shell, binding, lease, _local) = with_policy();
        let before = crate::app_home::autocomplete::preferences_revision(&shell);
        let blocks = draft::entry(&shell, binding.id()).unwrap().blocks.clone();
        let draft = draft::entry_mut(&mut shell, binding.id()).unwrap();
        draft.composing = composing;
        draft.busy = !composing;
        assert!(!lease.current(&shell));
        assert!(observes_preferences(&shell, &lease.team, lease.socket));
        let event = RtEvent::PreferencesChanged {
            name: "frecency".into(),
            value: json!("{}"),
        };
        crate::app_home::realtime(&mut shell, &lease.team, lease.socket, &event);
        assert_eq!(
            crate::app_home::autocomplete::preferences_revision(&shell),
            before + 1
        );
        let draft = draft::entry_mut(&mut shell, binding.id()).unwrap();
        draft.composing = false;
        draft.busy = false;
        assert!(lease.original_mount(&shell));
        assert!(
            !lease.current(&shell),
            "ending the transient state must not revive old policy"
        );
        assert!(lease.preferences(&shell).is_none());
        assert!(choose(&mut shell, &lease, 1, 0).is_none());
        assert_eq!(draft::entry(&shell, binding.id()).unwrap().blocks, blocks);
    }
}

#[test]
fn original_caret_kind_is_once_only_and_cannot_switch_after_start() {
    for first in [Kind::Channel, Kind::Member] {
        let (mut shell, binding, old, _local) = with_policy();
        let q = shell
            .slack_lists
            .comments
            .completion
            .query
            .as_mut()
            .unwrap();
        assert!(start(q, first));
        assert!(!start(q, first));
        assert!(!start(
            q,
            if first == Kind::Channel {
                Kind::Member
            } else {
                Kind::Channel
            }
        ));
        assert_eq!(q.kind == Some(first), true);
        let next = begin(&mut shell, &binding, "original-native", 2).unwrap();
        assert!(!old.current(&shell));
        assert!(next.current(&shell));
        let q = shell
            .slack_lists
            .comments
            .completion
            .query
            .as_mut()
            .unwrap();
        assert!(q.kind.is_none());
        assert!(!start(q, Kind::Member)); // Original policy is required before either matcher.
    }
}
#[test]
fn member_context_is_not_borrowed_from_channel_or_neutral_query() {
    let (mut shell, binding, lease, _local) = with_policy();
    assert!(lease.member_context(&shell).is_err());
    let q = shell
        .slack_lists
        .comments
        .completion
        .query
        .as_mut()
        .unwrap();
    assert!(start(q, Kind::Channel));
    assert!(lease.member_context(&shell).is_err());
    let newer = begin(&mut shell, &binding, "original-native", 2).unwrap();
    assert!(lease.member_context(&shell).is_err());
    assert!(newer.member_context(&shell).is_err());
}
