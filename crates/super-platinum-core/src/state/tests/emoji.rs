use super::super::*;
use crate::slack::models::{Emoji, Reaction};

#[test]
fn emoji_text_tokens_extracts_shortcodes_and_display_text_keeps_custom() {
    assert_eq!(
        emoji_text_tokens("ship it :wave: :party-hack:"),
        vec![
            EmojiTextToken::Text("ship it ".into()),
            EmojiTextToken::Emoji("wave".into()),
            EmojiTextToken::Text(" ".into()),
            EmojiTextToken::Emoji("party-hack".into()),
        ]
    );
    assert_eq!(
        emoji_text_to_display("ship it :wave: :party-hack:"),
        "ship it 👋 :party-hack:"
    );
}

#[test]
fn custom_emoji_url_resolves_aliases() {
    let mut ws = Workspace::from_session(&crate::config::WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U_SELF".into(),
        name: "Test".into(),
        url: "https://test.slack.com".into(),
        token: "xoxc-test".into(),
    });
    ws.apply_emojis(vec![
        Emoji {
            name: "party-hack".into(),
            value: "https://emoji.test/party.png".into(),
            ..Default::default()
        },
        Emoji {
            name: "party-alias".into(),
            value: "alias:party-hack".into(),
            ..Default::default()
        },
    ]);

    assert_eq!(
        ws.custom_emoji_url("party-alias"),
        Some("https://emoji.test/party.png")
    );
}

#[test]
fn reaction_summary_format() {
    let r = Reaction {
        name: "thumbsup".into(),
        count: 3,
        ..Default::default()
    };
    assert_eq!(reaction_summary(&r), "👍 3");

    let custom = Reaction {
        name: "hackclub_bug".into(),
        count: 1,
        ..Default::default()
    };
    assert_eq!(reaction_summary(&custom), ":hackclub_bug: 1");
}

#[test]
fn custom_aliases_resolve_unicode_and_reject_cycles_and_unsafe_urls() {
    let emojis = [
        ("nice", "alias:thumbsup"),
        ("very_nice", "alias:nice"),
        ("cycle_a", "alias:cycle_b"),
        ("cycle_b", "alias:cycle_a"),
        ("missing", "alias:does_not_exist"),
        ("unsafe", "javascript:alert(1)"),
    ]
    .into_iter()
    .map(|(name, value)| {
        (
            name.to_owned(),
            Emoji {
                name: name.into(),
                value: value.into(),
                ..Default::default()
            },
        )
    })
    .collect();
    assert_eq!(
        custom_emoji_source(&emojis, "very_nice"),
        Some(EmojiSource::Unicode("👍"))
    );
    for name in ["cycle_a", "missing", "unsafe"] {
        assert_eq!(custom_emoji_source(&emojis, name), None, "{name}");
    }
}
