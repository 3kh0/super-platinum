use super::*;
use crate::composer_completion::suggestions;
use crate::slack::models::{Channel, User, UserGroup, UserProfile};

fn workspace() -> Workspace {
    Workspace::from_session(&crate::config::WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        name: "Index test".into(),
        url: "https://example.invalid".into(),
        token: String::new(),
        enterprise_id: None,
    })
}
fn user(id: &str, name: &str) -> User {
    User {
        id: id.into(),
        name: Some(name.into()),
        ..Default::default()
    }
}
fn trigger(query: &str) -> Trigger {
    Trigger::at(query, query.len(), query.len()).unwrap()
}
fn compare(ws: &Workspace, queries: &[&str]) {
    let snapshot = Snapshot::capture(ws);
    assert!(snapshot.matches(ws));
    let index = Index::build(&snapshot);
    for query in queries {
        let trigger = trigger(query);
        assert_eq!(
            index.suggestions(&trigger),
            suggestions(ws, &trigger),
            "{query}"
        );
    }
}

#[test]
fn prepared_queries_preserve_exact_prefix_substring_ties_unicode_and_limits() {
    let mut ws = workspace();
    for i in 0..3000 {
        let id = format!("U{i:06}");
        let mut u = user(&id, &format!("alex-{i:06}"));
        u.profile = Some(UserProfile {
            display_name: Some(format!(
                "{} {i:06}",
                ["Alex", "JOSÉ", "İpek", "田中", "Chloé", "Алекс"][i % 6]
            )),
            ..Default::default()
        });
        u.deleted = i % 17 == 0;
        ws.users.insert(id, u);
    }
    for id in ["UA", "UZ"] {
        ws.users.insert(id.into(), user(id, "alex"));
    }
    ws.usergroups.insert(
        "S1".into(),
        UserGroup {
            id: "S1".into(),
            handle: "alex".into(),
            name: "Helpers".into(),
            ..Default::default()
        },
    );
    compare(
        &ws,
        &[
            "@",
            "@a",
            "@al",
            "@Alex",
            "@alex-000001",
            "@lex",
            "@josé",
            "@İ",
            "@田",
            "@here",
            "@everyone",
            "@channel",
            "@missing",
        ],
    );
    let index = Index::build(&Snapshot::capture(&ws));
    let exact = index.suggestions(&trigger("@alex"));
    assert_eq!(exact[0].target, Target::User("UA".into()));
    assert_eq!(exact[1].target, Target::User("UZ".into()));
    assert!(index.string_capacity_bytes() > 0);
}

#[test]
fn preserves_display_fallback_trimming_empty_real_and_inconsistent_imported_id_lookup() {
    let mut ws = workspace();
    for (id, display, profile_real, real, name) in [
        (
            "U1",
            Some("  Display  "),
            Some("Profile"),
            Some("Real"),
            Some("handle"),
        ),
        (
            "U2",
            Some("  "),
            Some(" Profile "),
            Some("Real"),
            Some("handle"),
        ),
        ("U3", None, None, Some(" Real "), Some("handle")),
        ("U4", None, None, None, Some(" handle ")),
        ("U5", None, None, None, None),
        ("U6", None, Some("Profile"), Some(""), Some("")),
    ] {
        ws.users.insert(
            id.into(),
            User {
                id: id.into(),
                name: name.map(Into::into),
                real_name: real.map(Into::into),
                profile: Some(UserProfile {
                    display_name: display.map(Into::into),
                    real_name: profile_real.map(Into::into),
                    ..Default::default()
                }),
                ..Default::default()
            },
        );
    }
    ws.users
        .insert("different-key".into(), user("U1", "shadow-handle"));
    ws.users
        .insert("missing-key".into(), user("UMISSING", "lookup-missing"));
    ws.users.insert("invalid".into(), user("bad>id", "handle"));
    compare(
        &ws,
        &[
            "@",
            "@display",
            "@profile",
            "@real",
            "@handle",
            "@shadow",
            "@lookup",
            "@umissing",
        ],
    );
}

#[test]
fn filters_deleted_archived_dm_group_dm_and_invalid_identities_without_changing_private_detail() {
    let mut ws = workspace();
    for (id, archived, im, mpim, private, group) in [
        ("C1", false, false, false, false, false),
        ("C2", true, false, false, false, false),
        ("D1", false, true, false, false, false),
        ("G1", false, false, true, false, false),
        ("C3", false, false, false, true, false),
        ("C4", false, false, false, false, true),
        ("invalid>id", false, false, false, false, false),
    ] {
        ws.channels.insert(
            id.into(),
            Channel {
                id: id.into(),
                name: Some("design".into()),
                is_archived: archived,
                is_im: im,
                is_mpim: mpim,
                is_private: private,
                is_group: group,
                ..Default::default()
            },
        );
    }
    for (id, handle, deleted) in [
        ("S1", "design", 0),
        ("S2", "design", 1),
        ("S3", "", 0),
        ("bad>id", "design", 0),
    ] {
        ws.usergroups.insert(
            id.into(),
            UserGroup {
                id: id.into(),
                handle: handle.into(),
                date_delete: deleted,
                ..Default::default()
            },
        );
    }
    compare(&ws, &["#", "#design", "#esign", "@design", "@"]);
    assert_eq!(
        Index::build(&Snapshot::capture(&ws))
            .suggestions(&trigger("#design"))
            .len(),
        3
    );
}

#[test]
fn publication_snapshot_detects_search_metadata_changes_but_ignores_presence_images_and_read_counts()
 {
    let mut ws = workspace();
    ws.users.insert("U1".into(), user("U1", "alex"));
    ws.channels.insert(
        "C1".into(),
        Channel {
            id: "C1".into(),
            name: Some("design".into()),
            ..Default::default()
        },
    );
    let snapshot = Snapshot::capture(&ws);
    ws.users.get_mut("U1").unwrap().profile = Some(UserProfile {
        image_72: Some("https://example.invalid/image".into()),
        ..Default::default()
    });
    ws.channels.get_mut("C1").unwrap().unread_count = Some(99);
    assert!(snapshot.matches(&ws));
    ws.users
        .get_mut("U1")
        .unwrap()
        .profile
        .as_mut()
        .unwrap()
        .display_name = Some("renamed".into());
    assert!(!snapshot.matches(&ws));
    let snapshot = Snapshot::capture(&ws);
    ws.users.get_mut("U1").unwrap().deleted = true;
    assert!(!snapshot.matches(&ws));
    let snapshot = Snapshot::capture(&ws);
    ws.channels.get_mut("C1").unwrap().is_archived = true;
    assert!(!snapshot.matches(&ws));
    let snapshot = Snapshot::capture(&ws);
    ws.users.insert("U2".into(), user("U2", "new"));
    assert!(!snapshot.matches(&ws));
}

#[test]
fn consumed_snapshot_is_independent_of_later_source_changes() {
    let mut ws = workspace();
    ws.users.insert("U1".into(), user("U1", "alex"));
    let snapshot = Snapshot::capture(&ws);
    let before = Index::build(&snapshot).suggestions(&trigger("@alex"));
    ws.users.get_mut("U1").unwrap().name = Some("renamed".into());
    assert!(!snapshot.matches(&ws));
    assert_eq!(
        Index::build(&snapshot).suggestions(&trigger("@alex")),
        before
    );
    assert!(suggestions(&ws, &trigger("@alex")).is_empty());
}

#[test]
fn iteration_order_is_part_of_snapshot_validity() {
    let mut ws = workspace();
    ws.users.insert("U1".into(), user("U1", "alex"));
    ws.users.insert("U2".into(), user("U2", "alex"));
    let mut snapshot = Snapshot::capture(&ws);
    assert!(snapshot.matches(&ws));
    snapshot.users.reverse();
    assert!(!snapshot.matches(&ws));
}
