use super::*;
use crate::slack::models::{Channel, User, UserGroup};

fn workspace() -> Workspace {
    Workspace::from_session(&crate::config::WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        name: "Fixture".into(),
        url: "https://example.invalid".into(),
        token: String::new(),
        enterprise_id: None,
    })
}
fn trigger(text: &str) -> Trigger {
    Trigger::at(text, text.len(), text.len()).unwrap()
}
fn person(id: &str, name: &str) -> User {
    User {
        id: id.into(),
        name: Some(name.into()),
        real_name: Some(name.into()),
        ..Default::default()
    }
}
fn choice(id: &str, label: &str) -> Suggestion {
    Suggestion {
        label: label.into(),
        detail: String::new(),
        target: Target::User(id.into()),
    }
}

#[test]
fn triggers_follow_the_caret_and_handle_unicode_without_matching_email_or_links() {
    for text in [
        "@",
        "hello @alex",
        "hello (@alex",
        "🦊 @みゆ",
        "line\n#design",
        "@e\u{301}",
    ] {
        let found = trigger(text);
        assert_eq!(found.end, text.len());
        assert!(text.is_char_boundary(found.start));
    }
    for text in [
        "me@example",
        "https://example/@name",
        "<#C123",
        "<@U123",
        "hey @alex!",
        "#channel:",
        "\\@literal",
        "plain text",
    ] {
        assert!(
            Trigger::at(text, text.len(), text.len()).is_none(),
            "{text}"
        );
    }
    let text = "🦊 @al trailing";
    assert_eq!(Trigger::at(text, 8, 8).unwrap().query, "al");
    assert!(Trigger::at(text, 1, 1).is_none());
    assert!(Trigger::at(text, 6, 8).is_none());
    assert!(Trigger::at(text, 999, 999).is_none());
}

#[test]
fn code_and_completed_reference_tokens_are_not_completion_queries() {
    for text in [
        "`@name",
        "```rust\n@name",
        "```\n` #name",
        "<@U123>",
        "<!subteam^S123>",
        "<#C123|name>",
    ] {
        assert!(
            Trigger::at(text, text.len(), text.len()).is_none(),
            "{text}"
        );
    }
    assert!(Trigger::at("`code` @a", 9, 9).is_some());
    assert!(trigger("```code```\n#design").kind == Kind::Channel);
    assert!(trigger("\\`literal @a").kind == Kind::Person);
}

#[test]
fn insertion_changes_only_the_selected_occurrence_and_preserves_unicode_suffix() {
    let mut composer = ComposerState::with_text("@Alex said hi 🦊 @al, then @Alex");
    let end = composer.text.find(", then").unwrap();
    composer.set_selection(end, end);
    let query = Trigger::at(&composer.text, end, end).unwrap();
    assert!(query.insert(&mut composer, &choice("U2", "@Alex")));
    assert_eq!(composer.text, "@Alex said hi 🦊 <@U2>, then @Alex");
    assert_eq!(composer.selection.start, "@Alex said hi 🦊 <@U2>".len());
    assert_eq!(composer.selection.start, composer.selection.end);
    // A second person with the same displayed name carries a different ID.
    composer.text.push_str(" @al");
    let end = composer.text.len();
    composer.set_selection(end, end);
    assert!(trigger(&composer.text).insert(&mut composer, &choice("U3", "@Alex")));
    assert!(composer.text.ends_with("<@U3> "));
    assert!(composer.text.contains("<@U2>"));
}

#[test]
fn insertion_rejects_changed_queries_selections_invalid_ids_and_wrong_kind() {
    let query = trigger("@al");
    let selected = choice("U1", "@Alex");
    let mut composer = ComposerState::with_text("@ali");
    assert!(!query.insert(&mut composer, &selected));
    composer = ComposerState::with_text("@al");
    composer.set_selection(0, 3);
    assert!(!query.insert(&mut composer, &selected));
    composer.set_selection(3, 3);
    assert!(!query.insert(&mut composer, &choice("U1> <!channel", "@Alex")));
    let wrong = Suggestion {
        target: Target::Channel("C1".into()),
        ..selected
    };
    assert!(!query.insert(&mut composer, &wrong));
    assert_eq!(composer.text, "@al");
    composer = ComposerState::with_text("@al next");
    composer.set_selection(3, 3);
    assert!(query.insert(&mut composer, &choice("U1", "@Alex")));
    assert_eq!(composer.text, "<@U1> next");
}

#[test]
fn people_are_ranked_bounded_and_duplicate_names_remain_separate_choices() {
    let mut ws = workspace();
    for i in 0..1000 {
        let user = person(&format!("U{i}"), &format!("Alex {i}"));
        ws.users.insert(user.id.clone(), user);
    }
    ws.users.insert("UZ".into(), person("UZ", "alex"));
    ws.users.insert("UA".into(), person("UA", "alex"));
    let mut deleted = person("UD", "alex");
    deleted.deleted = true;
    ws.users.insert("UD".into(), deleted);
    let results = suggestions(&ws, &trigger("@alex"));
    assert_eq!(results.len(), LIMIT);
    assert_eq!(results[0].target, Target::User("UA".into()));
    assert_eq!(results[1].target, Target::User("UZ".into()));
    assert!(
        !results
            .iter()
            .any(|s| s.target == Target::User("UD".into()))
    );
    assert_eq!(suggestions(&ws, &trigger("@Alex")), results);
}

#[test]
fn groups_broadcasts_and_channels_use_explicit_canonical_tokens() {
    let mut ws = workspace();
    ws.usergroups.insert(
        "S1".into(),
        UserGroup {
            id: "S1".into(),
            name: "Helpers".into(),
            handle: "helpers".into(),
            ..Default::default()
        },
    );
    ws.usergroups.insert(
        "S2".into(),
        UserGroup {
            id: "S2".into(),
            name: "Old".into(),
            handle: "helpers-old".into(),
            date_delete: 1,
            ..Default::default()
        },
    );
    assert_eq!(
        suggestions(&ws, &trigger("@help"))[0]
            .target
            .token()
            .as_deref(),
        Some("<!subteam^S1>")
    );
    let channel_broadcast = suggestions(&ws, &trigger("@channel"));
    assert_eq!(
        channel_broadcast[0].target.token().as_deref(),
        Some("<!channel>")
    );
    assert!(channel_broadcast[0].detail.contains("Notify"));
    for (id, name, archived, im) in [
        ("C1", "design", false, false),
        ("C2", "old-design", true, false),
        ("D1", "design-dm", false, true),
    ] {
        ws.channels.insert(
            id.into(),
            Channel {
                id: id.into(),
                name: Some(name.into()),
                is_archived: archived,
                is_im: im,
                ..Default::default()
            },
        );
    }
    let channels = suggestions(&ws, &trigger("#design"));
    assert_eq!(channels.len(), 1);
    assert_eq!(channels[0].target.token().as_deref(), Some("<#C1>"));
}

#[test]
fn missing_profile_fields_do_not_get_an_empty_query_ranking_bonus() {
    let mut ws = workspace();
    ws.users.insert("U1".into(), person("U1", "Alice"));
    let mut zulu = person("U2", "Zulu");
    zulu.real_name = None;
    ws.users.insert("U2".into(), zulu);
    let results = suggestions(&ws, &trigger("@"));
    assert_eq!(results[0].label, "@Alice");
    assert_eq!(results[1].label, "@Zulu");
}
