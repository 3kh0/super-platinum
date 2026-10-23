use super::*;
use serde_json::json;

fn payload(version: Version) -> LiveEmoji {
    let raw = Complete::decode(json!({"ok":true,"emoji":{
        "smile":"https://fixture.invalid/wrong-standard-shadow.png",
        "party":"https://fixture.invalid/party.png","party_alias":"alias:party",
        "thumb_alias":"alias:thumbsup","chain":"alias:thumb_alias",
        "loop_a":"alias:loop_b","loop_b":"alias:loop_a",
        "dangling":"alias:unavailable","invalid":"file:///not-an-image"
    },"opaque":"retained"}))
    .unwrap();
    Prepared::build(version, &raw)
        .unwrap()
        .project("en-US".into(), "ja-JP".into(), |_, _| {
            "http://super-platinum-media.emoji/00000000000000000000000000000001?v=7".into()
        })
}

#[test]
fn original_complete_names_and_standard_first_appearance_are_separate() {
    let p = payload(Version::V16);
    assert_eq!(p.appearance(":smile:").unwrap().text, "\u{1f604}");
    assert!(p.appearance(":smile:").unwrap().media.is_none());
    for name in ["loop_a", "loop_b", "dangling", "invalid"] {
        assert!(p.names.iter().any(|n| n == name));
        assert!(p.appearance(&format!(":{name}:")).is_none());
    }
    let custom = p.appearance(":party_alias::skin-tone-4:").unwrap();
    assert_eq!(custom.name, ":party_alias::skin-tone-4:");
    assert_eq!(custom.text, ":party_alias:");
    assert!(custom.media.is_some());
}

#[test]
fn recursive_appearance_does_not_invent_recursive_tone_capability() {
    let p = payload(Version::V17);
    let row = |name| p.rows.iter().find(|r| r.name == name).unwrap();
    assert!(row("thumb_alias").single);
    assert!(!row("chain").single);
    assert!(!row("chain").multiple);
    assert!(row("handshake").single && row("handshake").multiple);
    for name in ["thumbsup", "thumb_alias", "chain"] {
        let exact = format!(":{name}::skin-tone-4:");
        let appearance = p.appearance(&exact).unwrap();
        assert_eq!(appearance.name, exact);
        assert_eq!(appearance.text, "\u{1f44d}\u{1f3fd}");
    }
    assert_eq!(
        p.appearance(":smile::skin-tone-4:").unwrap().text,
        "\u{1f604}"
    );
    assert_eq!(
        p.appearance(":handshake::skin-tone-2-3:").unwrap().text,
        "\u{1faf1}\u{1f3fb}\u{200d}\u{1faf2}\u{1f3fc}"
    );
}

#[test]
fn selected_versions_reverse_and_unicode_order_follow_retained_entries() {
    for version in [Version::V16, Version::V17] {
        let p = payload(version);
        let catalog = Catalog::build(version).unwrap();
        let standard = StandardEmoji::build(version).unwrap();
        let mut index = 0;
        for entry in &catalog.entries {
            assert_eq!(p.unicode[index], glyph(&entry.unicode).unwrap());
            index += 1;
            for (_, value) in variants(entry) {
                assert_eq!(p.unicode[index], glyph(&value.unicode).unwrap());
                index += 1;
            }
        }
        assert_eq!(index, p.unicode.len());
        for (unicode, name) in &p.unicode_names {
            assert_eq!(standard.unicode_name(unicode), Some(name.as_str()));
        }
        assert_eq!(p.names.iter().any(|n| n == "orca"), version == Version::V17);
        assert_eq!(p.locale, "en-US");
        assert_eq!(p.current_locale, "ja-JP");
    }
}

#[test]
fn exact_restored_shortcode_retains_identity_and_refuses_unavailable_syntax() {
    let p = payload(Version::V16);
    let text = ":THUMBSUP::skin-tone-4:";
    assert_eq!(p.appearance(text).unwrap().name, text);
    for text in [
        ":unknown:",
        ":thumbsup::skin-tone-7:",
        ":thumbsup::skin-tone-2-3-4:",
        "thumbsup",
        ":thumbsup::other-4:",
        ":loop_a:",
    ] {
        assert!(p.appearance(text).is_none(), "{text}");
    }
}
