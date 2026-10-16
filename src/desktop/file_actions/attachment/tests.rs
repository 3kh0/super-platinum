use super::*;
use crate::message_buttons::source::Bag;
use super_platinum_core::slack::models::Message;

pub(crate) fn sample() -> (ShellState, Source, Target) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    check::fixture(&mut shell);
    let team = shell.core.active_team.as_ref().unwrap();
    let channel = shell.core.active_channel.as_ref().unwrap();
    let raw = &shell.core.workspaces[team].messages[channel].messages[0];
    let row = crate::message_vm::message_vm(&shell.core.workspaces[team], raw, &shell.media);
    let source = Source::capture(&shell, &row, channel, Bag::Timeline).unwrap();
    let target = target(
        &shell,
        &source,
        &policy::identity(&raw.files[0]).unwrap(),
        &file_title(&raw.files[0]),
    )
    .unwrap();
    (shell, source, target)
}
pub(crate) fn message(shell: &mut ShellState) -> &mut Message {
    let team = shell.core.active_team.clone().unwrap();
    let channel = shell.core.active_channel.clone().unwrap();
    &mut shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .messages
        .get_mut(&channel)
        .unwrap()
        .messages[0]
}
#[test]
fn canonical_file_actions_reuse_ownership_and_link_policy() {
    let (shell, _, own) = sample();
    assert!(own.current(&shell) && own.owned && own.renameable);
    assert_eq!(
        own.link.as_deref(),
        Some("https://fixture.slack.com/files/USELF/FACTIONS")
    );
    let team = shell.core.active_team.as_ref().unwrap();
    let channel = shell.core.active_channel.as_ref().unwrap();
    for raw in &shell.core.workspaces[team].messages[channel].messages[1..] {
        let row = crate::message_vm::message_vm(&shell.core.workspaces[team], raw, &shell.media);
        let source = Source::capture(&shell, &row, channel, Bag::Timeline).unwrap();
        let action = target(
            &shell,
            &source,
            &policy::identity(&raw.files[0]).unwrap(),
            &file_title(&raw.files[0]),
        )
        .unwrap();
        assert!(!action.owned);
        if action.id == "FNOLINK" {
            assert!(action.link.is_none());
        }
    }
}
#[test]
fn canonical_file_metadata_changes_retire_old_controls_independently_of_block_revision() {
    for change in 0..6 {
        let (mut shell, source, old) = sample();
        let raw = message(&mut shell);
        match change {
            0 => raw.files[0].title = Some("New title".into()),
            1 => {
                raw.files[0]
                    .extra
                    .insert("user".into(), serde_json::json!("UOTHER"));
            }
            2 => {
                raw.files[0].extra.insert(
                    "permalink".into(),
                    serde_json::json!("https://fixture.slack.com/files/UOTHER/OTHER"),
                );
            }
            3 => raw.files.clear(),
            4 => {
                let duplicate = raw.files[0].clone();
                raw.files.push(duplicate);
            }
            _ => raw.files[0].is_external = Some(true),
        }
        assert!(
            source.current(&shell),
            "file-only changes are outside the app block revision"
        );
        assert!(!old.current(&shell));
    }
}
#[test]
fn account_navigation_and_deletion_refuse_retained_attachment_target() {
    for change in 0..4 {
        let (mut shell, _, old) = sample();
        match change {
            0 => shell.core.account_epoch += 1,
            1 => shell.channel_generation += 1,
            2 => shell.core.active_channel = Some("C_OTHER".into()),
            _ => {
                let team = shell.core.active_team.clone().unwrap();
                shell
                    .core
                    .workspaces
                    .get_mut(&team)
                    .unwrap()
                    .deleted_files
                    .insert(old.id.clone());
            }
        }
        assert!(!old.current(&shell));
    }
}
#[test]
fn forged_or_unidentified_media_cannot_gain_canonical_file_actions() {
    let (shell, source, own) = sample();
    let mut identity = own.attachment.unwrap().file;
    identity.id = "FOREIGN".into();
    assert!(target(&shell, &source, &identity, "Release notes · 日本語").is_none());
    identity.id.clear();
    assert!(target(&shell, &source, &identity, "Release notes · 日本語").is_none());
}
#[test]
fn thread_target_checks_its_own_bag_even_with_same_timeline_file() {
    let (mut shell, _, timeline) = sample();
    let team = shell.core.active_team.clone().unwrap();
    let channel = shell.core.active_channel.clone().unwrap();
    let raw = message(&mut shell).clone();
    let root = raw.ts.clone().unwrap();
    let mut bag = super_platinum_core::state::ChannelMessages::default();
    bag.upsert(raw.clone());
    shell
        .core
        .threads
        .insert((team.clone(), channel.clone(), root.clone()), bag);
    shell.thread_root = Some(root.clone());
    let row = crate::message_vm::message_vm(&shell.core.workspaces[&team], &raw, &shell.media);
    let source =
        Source::capture(&shell, &row, &channel, Bag::Thread { root: root.clone() }).unwrap();
    let thread = target(
        &shell,
        &source,
        &policy::identity(&raw.files[0]).unwrap(),
        &file_title(&raw.files[0]),
    )
    .unwrap();
    shell
        .core
        .threads
        .get_mut(&(team, channel, root))
        .unwrap()
        .messages[0]
        .files
        .clear();
    assert!(!thread.current(&shell));
    assert!(timeline.current(&shell));
}
#[test]
fn mounted_duplicate_file_occurrences_have_distinct_action_ids() {
    use dioxus::core::{AttributeValue, Mutation};
    let mut dom = VirtualDom::new(|| {
        let (shell, _, target) = sample();
        let state = use_signal(|| shell);
        rsx! {MessageFile{state,target:target.clone(),children:rsx!{button{"Preview one"}}}
        MessageFile{state,target,children:rsx!{button{"Preview two"}}}}
    });
    let edits = dom.rebuild_to_vec();
    let ids: Vec<_> = edits
        .edits
        .iter()
        .filter_map(|edit| match edit {
            Mutation::SetAttribute {
                name: "id",
                value: AttributeValue::Text(value),
                ..
            } if value.starts_with("file-actions-") => Some(value),
            _ => None,
        })
        .collect();
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1]);
}

// Exercise the real MessageFile -> Actions lifecycle. A changed DOM id on the
// same element is insufficient: the old component's hooks would still survive.
fn mounted_target() -> (
    VirtualDom,
    Signal<ShellState>,
    dioxus::core::ElementId,
    String,
) {
    use std::cell::Cell;
    let slot = Rc::new(Cell::new(None));
    let mut dom = VirtualDom::new_with_props(
        |slot: Rc<Cell<Option<Signal<ShellState>>>>| {
            let state = use_signal(|| sample().0);
            slot.set(Some(state));
            let shell = state.read();
            let team = shell.core.active_team.as_ref().unwrap();
            let channel = shell.core.active_channel.as_ref().unwrap();
            let raw = &shell.core.workspaces[team].messages[channel].messages[0];
            let row =
                crate::message_vm::message_vm(&shell.core.workspaces[team], raw, &shell.media);
            let source = Source::capture(&shell, &row, channel, Bag::Timeline).unwrap();
            let target = target(
                &shell,
                &source,
                &policy::identity(&raw.files[0]).unwrap(),
                &file_title(&raw.files[0]),
            )
            .unwrap();
            rsx! { MessageFile {state,target,children:rsx!{button{"Preview"}}} }
        },
        slot.clone(),
    );
    let edits = dom.rebuild_to_vec();
    let id = mounted_action_ids(&edits)[0];
    let focus = mounted_share_return_addresses(&edits).remove(0);
    (dom, slot.get().unwrap(), id, focus)
}

fn mounted_action_ids(edits: &dioxus::core::Mutations) -> Vec<dioxus::core::ElementId> {
    use dioxus::core::{AttributeValue, Mutation};
    edits
        .edits
        .iter()
        .filter_map(|edit| match edit {
            Mutation::SetAttribute {
                id,
                name: "id",
                value: AttributeValue::Text(value),
                ..
            } if value.starts_with("file-actions-") => Some(*id),
            _ => None,
        })
        .collect()
}

fn mounted_share_return_addresses(edits: &dioxus::core::Mutations) -> Vec<String> {
    use dioxus::core::{AttributeValue, Mutation};
    edits
        .edits
        .iter()
        .filter_map(|edit| match edit {
            Mutation::SetAttribute {
                name: "data-file-share-return",
                value: AttributeValue::Text(value),
                ..
            } if value.starts_with("file-share-return:") => Some((*value).to_owned()),
            _ => None,
        })
        .collect()
}

#[test]
fn mounted_attachment_replaces_action_scope_when_account_navigation_or_file_changes() {
    for change in 0..7 {
        let (mut dom, mut state, old_element, old_focus) = mounted_target();
        dom.in_runtime(|| {
            let mut shell = state.write();
            match change {
                0 => shell.core.account_epoch += 1,
                1 => shell.channel_generation += 1,
                2 => message(&mut shell).files[0].title = Some("New title".into()),
                3 => {
                    message(&mut shell).files[0]
                        .extra
                        .insert("user".into(), serde_json::json!("UOTHER"));
                }
                4 => {
                    message(&mut shell).files[0].extra.insert(
                        "permalink".into(),
                        serde_json::json!("https://fixture.slack.com/files/USELF/OTHER"),
                    );
                }
                5 => message(&mut shell).files[0].filetype = Some("canvas".into()),
                _ => {
                    message(&mut shell).edited =
                        Some(serde_json::json!({"user":"USELF","ts":"1719800499.000000"}))
                }
            }
        });
        let edits = dom.render_immediate_to_vec();
        let ids = mounted_action_ids(&edits);
        assert_eq!(ids.len(), 1, "replacement mounts exactly one trigger");
        assert_ne!(
            ids[0], old_element,
            "old action component must be unmounted"
        );
        let new_focus = mounted_share_return_addresses(&edits).remove(0);
        if change == 2 {
            assert_eq!(
                new_focus, old_focus,
                "title-only remount keeps its focus address"
            );
        } else {
            assert_ne!(
                new_focus, old_focus,
                "owner/navigation/source/permission changes retire focus"
            );
        }
        assert!(state.peek().core.session.is_none() && state.peek().core.transport.is_none());
    }
}

#[test]
fn mounted_attachment_preserves_action_scope_for_unrelated_shell_updates() {
    let (mut dom, mut state, _, _) = mounted_target();
    dom.in_runtime(|| state.write().show_toast("Unrelated update."));
    let edits = dom.render_immediate_to_vec();
    assert!(mounted_action_ids(&edits).is_empty());
}

#[test]
fn canonical_file_accessor_preserves_full_current_metadata_without_a_projection() {
    let (mut shell, _, target) = sample();
    let guard = target.attachment.as_ref().unwrap();
    message(&mut shell).files[0].extra.insert(
        "share_policy_fixture".into(),
        serde_json::json!({"known":false,"opaque":["retained",7]}),
    );
    let canonical = guard.canonical_file(&shell).unwrap();
    assert!(guard.current(&shell));
    assert_eq!(canonical.extra["share_policy_fixture"]["known"], false);
    assert_eq!(canonical.extra["share_policy_fixture"]["opaque"][1], 7);
    let team = shell.core.active_team.as_ref().unwrap();
    let channel = shell.core.active_channel.as_ref().unwrap();
    assert!(std::ptr::eq(
        canonical,
        &shell.core.workspaces[team].messages[channel].messages[0].files[0]
    ));
    message(&mut shell).files[0].extra.insert(
        "share_policy_fixture".into(),
        serde_json::json!({"known":true}),
    );
    assert_eq!(
        guard.canonical_file(&shell).unwrap().extra["share_policy_fixture"]["known"],
        true
    );
}

#[test]
fn canonical_file_accessor_refuses_stale_identity_duplicates_and_navigation() {
    for change in 0..4 {
        let (mut shell, _, target) = sample();
        let guard = target.attachment.as_ref().unwrap();
        match change {
            0 => message(&mut shell).files[0].title = Some("Changed".into()),
            1 => {
                let duplicate = message(&mut shell).files[0].clone();
                message(&mut shell).files.push(duplicate);
            }
            2 => shell.channel_generation += 1,
            _ => {
                let team = shell.core.active_team.clone().unwrap();
                shell
                    .core
                    .workspaces
                    .get_mut(&team)
                    .unwrap()
                    .deleted_files
                    .insert(target.id.clone());
            }
        }
        assert!(guard.canonical_file(&shell).is_none());
        assert!(!guard.current(&shell));
    }
}

#[test]
fn focus_origin_exception_is_only_title_and_never_action_authority() {
    let (mut shell, source, old) = sample();
    message(&mut shell).files[0].title = Some("Changed title".into());
    let new = target(
        &shell,
        &source,
        &old.attachment.as_ref().unwrap().file,
        "Changed title",
    )
    .unwrap();
    assert_ne!(old.title, new.title);
    assert!(same_focus_origin(&old, &new));
    assert!(!old.current(&shell));
    assert!(new.current(&shell));
    for change in 0..9 {
        let mut other = new.clone();
        match change {
            0 => other.id.push('x'),
            1 => other.link = None,
            2 => other.owned = !other.owned,
            3 => other.renameable = !other.renameable,
            4 => other.epoch += 1,
            5 => other.team.push('x'),
            6 => other.attachment = None,
            7 => other.attachment.as_mut().unwrap().file.id.push('x'),
            _ => {
                other.attachment.as_mut().unwrap().source = {
                    shell.channel_generation += 1;
                    let team = shell.core.active_team.as_ref().unwrap();
                    let channel = shell.core.active_channel.as_ref().unwrap();
                    let raw = &shell.core.workspaces[team].messages[channel].messages[0];
                    let row = crate::message_vm::message_vm(
                        &shell.core.workspaces[team],
                        raw,
                        &shell.media,
                    );
                    Source::capture(&shell, &row, channel, Bag::Timeline).unwrap()
                }
            }
        }
        assert!(!same_focus_origin(&new, &other));
    }
}
