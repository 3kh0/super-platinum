use super::*;
use super_platinum_core::{
    config::WorkspaceSession,
    slack::models::{Channel, UserGroup},
};

#[test]
fn emoji_channel_owner_survives_unrelated_thread_generation() {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    let channel = EmojiOwner::capture(&shell, ComposerTarget::Channel);
    let thread = EmojiOwner::capture(&shell, ComposerTarget::Thread);
    shell.thread_generation = shell.thread_generation.wrapping_add(1);
    assert!(channel.current(&shell, ComposerTarget::Channel));
    assert!(!thread.current(&shell, ComposerTarget::Thread));
    shell.channel_generation = shell.channel_generation.wrapping_add(1);
    assert!(!channel.current(&shell, ComposerTarget::Channel));
}

fn workspace() -> Workspace {
    Workspace::from_session(&WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        name: "Fixture".into(),
        url: "https://example.invalid".into(),
        token: String::new(),
        enterprise_id: None,
    })
}
#[test]
fn draft_labels_resolve_only_referenced_identities_including_legacy_tokens() {
    let mut ws = workspace();
    for id in ["U1", "U2", "U3"] {
        ws.users.insert(
            id.into(),
            User {
                id: id.into(),
                name: Some("Alex".into()),
                ..Default::default()
            },
        );
    }
    ws.channels.insert(
        "C1".into(),
        Channel {
            id: "C1".into(),
            name: Some("design".into()),
            ..Default::default()
        },
    );
    ws.usergroups.insert(
        "S1".into(),
        UserGroup {
            id: "S1".into(),
            handle: "helpers".into(),
            ..Default::default()
        },
    );
    let found = labels(
        &ws,
        "<@U1> <@U2|old-name> <#C1> <!subteam^S1> <@MISSING> @Alex <https://example.invalid>",
    );
    assert_eq!(found.len(), 4);
    assert_eq!(found["<@U1>"], "@Alex");
    assert_eq!(found["<@U2>"], "@Alex");
    assert_eq!(found["<#C1>"], "#design");
    assert_eq!(found["<!subteam^S1>"], "@helpers");
    assert!(!found.contains_key("<@U3>"));
}
#[test]
fn remote_candidates_use_current_profiles_and_skip_deleted_or_invalid_identities() {
    let mut ws = workspace();
    let remote = User {
        id: "U9".into(),
        name: Some("old-handle".into()),
        real_name: Some("Old Name".into()),
        ..Default::default()
    };
    assert_eq!(remote_choice(&ws, &remote).unwrap().label, "@Old Name");
    ws.users.insert(
        "U9".into(),
        User {
            id: "U9".into(),
            real_name: Some("New Name".into()),
            ..Default::default()
        },
    );
    assert_eq!(remote_choice(&ws, &remote).unwrap().label, "@New Name");
    assert_eq!(remote_choice(&ws, &remote).unwrap().token, "<@U9>");
    ws.users.get_mut("U9").unwrap().deleted = true;
    assert!(remote_choice(&ws, &remote).is_none());
    assert!(
        remote_choice(
            &ws,
            &User {
                id: "bad>id".into(),
                ..Default::default()
            }
        )
        .is_none()
    );
    assert!(remote_choice(&ws, &User::default()).is_none());
}
#[test]
fn exact_remote_match_is_not_hidden_by_eight_cached_prefix_matches() {
    let ws = workspace();
    let mut choices: Vec<_> = (0..8)
        .map(|i| {
            remote_choice(
                &ws,
                &User {
                    id: format!("U{i}"),
                    name: Some(format!("alex{i}")),
                    ..Default::default()
                },
            )
            .unwrap()
        })
        .collect();
    let remote = User {
        id: "U99".into(),
        name: Some("alex".into()),
        ..Default::default()
    };
    merge_remote(&mut choices, &ws, &[remote.clone(), remote], "alex");
    assert_eq!(choices.len(), LIMIT);
    assert_eq!(choices[0].token, "<@U99>");
    assert_eq!(choices[1].token, "<@U0>");
    assert_eq!(choices.iter().filter(|c| c.token == "<@U99>").count(), 1);
}
