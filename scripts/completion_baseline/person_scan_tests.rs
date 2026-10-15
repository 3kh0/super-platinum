use super::*;
use super_platinum_core::slack::models::{Channel, User, UserGroup, UserProfile};

#[path = "legacy_oracle.rs"]
mod oracle;

fn workspace() -> Workspace {
    Workspace::from_session(&super_platinum_core::config::WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        name: "fixture".into(),
        url: "https://example.invalid".into(),
        token: String::new(),
        enterprise_id: None,
    })
}
fn user(id: &str, handle: &str) -> User {
    User {
        id: id.into(),
        name: Some(handle.into()),
        ..Default::default()
    }
}
fn trigger(kind: Kind, query: &str) -> Trigger {
    Trigger {
        kind,
        query: query.into(),
        start: 0,
        end: query.len() + 1,
    }
}
fn compare(ws: &Workspace, kind: Kind, queries: &[&str]) {
    for query in queries {
        let trigger = trigger(kind, query);
        let frozen = oracle::suggestions(ws, &trigger);
        assert_eq!(
            super_platinum_core::composer_completion::suggestions(ws, &trigger),
            frozen,
            "active legacy source drift: {query:?}"
        );
        assert_eq!(
            suggestions(ws, &trigger),
            frozen,
            "candidate mismatch: {query:?}"
        );
    }
}

#[test]
fn actual_corpus_all_queries_and_categories_match_frozen_legacy() {
    let ws = crate::corpus::workspace(2000);
    for text in crate::corpus::QUERIES {
        let t = crate::corpus::trigger(text);
        compare(&ws, t.kind, &[&t.query]);
    }
}

#[test]
fn display_lookup_fallbacks_blanks_and_raw_real_detail_are_preserved() {
    let mut ws = workspace();
    for (index, (display, profile_real, real, handle)) in [
        (
            Some("  Display  "),
            Some("Profile"),
            Some("Real"),
            Some("handle"),
        ),
        (
            Some(" \t "),
            Some("  Profile  "),
            Some("Real"),
            Some("handle"),
        ),
        (None, Some("  "), Some("  Real  "), Some("handle")),
        (None, None, Some(" \n "), Some("  handle  ")),
        (None, None, None, None),
        (None, Some("Profile detail"), Some(""), Some("")),
    ]
    .into_iter()
    .enumerate()
    {
        let id = format!("U{index}");
        let value = User {
            id: id.clone(),
            name: handle.map(str::to_owned),
            real_name: real.map(str::to_owned),
            profile: Some(UserProfile {
                display_name: display.map(str::to_owned),
                real_name: profile_real.map(str::to_owned),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(
            display_name(
                &{
                    let mut single = workspace();
                    single.users.insert(id.clone(), value.clone());
                    single
                },
                &id
            ),
            super_platinum_core::state::display_name(Some(&value), &id)
        );
        ws.users.insert(id, value);
    }
    ws.users.insert("alias".into(), user("U0", "alias-handle"));
    ws.users
        .insert("missing-alias".into(), user("UNOTINMAP", "missing-handle"));
    compare(
        &ws,
        Kind::Person,
        &[
            "",
            "display",
            "profile",
            "real",
            "handle",
            "alias",
            "unotinmap",
            " ",
            "never",
        ],
    );
}

#[test]
fn contextual_unicode_and_expanding_lowercase_use_exact_str_behavior() {
    let mut ws = workspace();
    let names = [
        "\u{039f}\u{03a3}",
        "\u{03a3}\u{039f}\u{03a3}",
        "\u{0130}pek",
        "Stra\u{00df}e",
        "Jos\u{00c9}",
        "\u{7530}\u{4e2d}",
        "\u{03a3}\u{0301}",
        "A\u{03a3}\u{0301}",
        "e\u{0301}",
        "\u{00c9}",
        "\u{212a}elvin",
    ];
    for (i, name) in names.iter().enumerate() {
        let id = format!("U{i}");
        ws.users.insert(id.clone(), user(&id, name));
        let mut scratch = String::from("stale");
        fold(name, &mut scratch);
        assert_eq!(scratch, name.to_lowercase());
    }
    assert_ne!(
        names[0].to_lowercase(),
        names[0]
            .chars()
            .flat_map(char::to_lowercase)
            .collect::<String>()
    );
    compare(
        &ws,
        Kind::Person,
        &[
            "",
            "\u{03c2}",
            "\u{03c3}",
            "\u{039f}\u{03a3}",
            "i\u{0307}",
            "\u{0130}",
            "stra\u{00df}",
            "jOS\u{00e9}",
            "\u{7530}",
            "\u{0301}",
            "kelvin",
        ],
    );
}

#[test]
fn token_prefix_order_includes_closing_delimiter() {
    let mut ws = workspace();
    for id in ["U", "U0", "UA"] {
        ws.users.insert(id.into(), user(id, "same"));
    }
    let rows = suggestions(&ws, &trigger(Kind::Person, "same"));
    assert_eq!(
        rows.iter().map(|x| x.target.clone()).collect::<Vec<_>>(),
        vec![
            Target::User("U0".into()),
            Target::User("U".into()),
            Target::User("UA".into())
        ]
    );
    compare(&ws, Kind::Person, &["", "same"]);
}

#[test]
fn malformed_alias_equal_ranks_keep_reverse_encounter_ties_and_limit() {
    let mut ws = workspace();
    ws.users.insert("U".into(), user("U", "shared"));
    for i in 0..20 {
        ws.users
            .insert(format!("alias{i}"), user("U", &format!("detail{i}")));
    }
    let expected_details: Vec<_> = ws
        .users
        .values()
        .map(|u| format!("@{}", u.name.as_deref().unwrap()))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .take(LIMIT)
        .collect();
    let actual = suggestions(&ws, &trigger(Kind::Person, "shared"));
    assert_eq!(
        actual.iter().map(|x| x.detail.clone()).collect::<Vec<_>>(),
        expected_details
    );
    for i in 0..20 {
        ws.usergroups.insert(
            format!("alias{i}"),
            UserGroup {
                id: "S".into(),
                handle: "group".into(),
                name: format!("detail{i}"),
                ..Default::default()
            },
        );
    }
    compare(&ws, Kind::Person, &["shared", "group", "", "detail"]);
}

#[test]
fn direct_scan_observes_mutations_filters_and_broadcast_categories() {
    let mut ws = workspace();
    ws.users.insert("U1".into(), user("U1", "here"));
    ws.users.insert("bad".into(), user("bad>id", "here"));
    ws.users.insert("empty".into(), user("", "here"));
    ws.usergroups.insert(
        "S1".into(),
        UserGroup {
            id: "S1".into(),
            handle: "here".into(),
            name: "Here group".into(),
            ..Default::default()
        },
    );
    compare(
        &ws,
        Kind::Person,
        &["", "here", "channel", "everyone", "zzzz"],
    );
    ws.users.get_mut("U1").unwrap().deleted = true;
    ws.usergroups.get_mut("S1").unwrap().date_delete = 7;
    compare(&ws, Kind::Person, &["", "here"]);
    ws.users.get_mut("U1").unwrap().deleted = false;
    ws.users.get_mut("U1").unwrap().name = Some("renamed".into());
    ws.usergroups.get_mut("S1").unwrap().date_delete = 0;
    ws.usergroups.get_mut("S1").unwrap().handle.clear();
    compare(&ws, Kind::Person, &["", "here", "renamed"]);
}

#[test]
fn channel_branch_remains_exact_legacy_with_all_filter_flags() {
    let mut ws = workspace();
    for (i, (name, archived, im, mpim, private, group)) in [
        (Some("same"), false, false, false, false, false),
        (Some("same"), false, false, false, true, false),
        (Some("same"), false, false, false, false, true),
        (Some("same"), true, false, false, false, false),
        (Some("same"), false, true, false, false, false),
        (Some("same"), false, false, true, false, false),
        (Some(""), false, false, false, false, false),
        (None, false, false, false, false, false),
    ]
    .into_iter()
    .enumerate()
    {
        let id = format!("C{i}");
        ws.channels.insert(
            id.clone(),
            Channel {
                id,
                name: name.map(str::to_owned),
                is_archived: archived,
                is_im: im,
                is_mpim: mpim,
                is_private: private,
                is_group: group,
                ..Default::default()
            },
        );
    }
    compare(&ws, Kind::Channel, &["", "same", "never"]);
}
