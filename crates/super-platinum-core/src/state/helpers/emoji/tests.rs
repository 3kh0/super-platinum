use super::*;

#[test]
fn every_retained_v16_and_v17_tone_variant_has_the_exact_original_sequence() {
    for (source, expected_count) in [
        (
            include_str!("../../../composer_emoji/slack-emoji-v16-ordered.json"),
            2020,
        ),
        (
            include_str!("../../../composer_emoji/slack-emoji-v17-ordered.json"),
            2225,
        ),
    ] {
        let rows: Vec<crate::composer_emoji::standard::Entry> =
            serde_json::from_str(source).unwrap();
        let mut count = 0;
        let mut names = std::collections::HashSet::new();
        for row in rows {
            for (tone, variant) in row.skin_variations {
                assert_eq!(variant.name, format!("{}::skin-tone-{tone}", row.name));
                assert!(names.insert(variant.name.clone()));
                let glyph: String = variant
                    .unicode
                    .split('-')
                    .map(|part| char::from_u32(u32::from_str_radix(part, 16).unwrap()).unwrap())
                    .collect();
                assert_eq!(
                    tones::glyph(&variant.name),
                    Some(glyph.as_str()),
                    "{}",
                    variant.name
                );
                assert_eq!(emoji_glyph(&variant.name), glyph, "{}", variant.name);
                count += 1;
            }
        }
        assert_eq!(count, expected_count);
    }
}

#[test]
fn single_and_paired_variants_use_exact_sequences_not_appended_modifiers() {
    assert_eq!(emoji_glyph("wave::skin-tone-6"), "\u{1f44b}\u{1f3ff}");
    assert_eq!(emoji_glyph("thumbsup::skin-tone-3"), "\u{1f44d}\u{1f3fc}");
    assert_eq!(
        emoji_glyph("handshake::skin-tone-2-6"),
        "\u{1faf1}\u{1f3fb}\u{200d}\u{1faf2}\u{1f3ff}"
    );
    assert_eq!(
        emoji_glyph("woman-heart-man::skin-tone-3-5"),
        "\u{1f469}\u{1f3fc}\u{200d}\u{2764}\u{fe0f}\u{200d}\u{1f468}\u{1f3fe}"
    );
}

#[test]
fn aliases_share_glyphs_without_changing_raw_reaction_identity() {
    for name in ["+1::skin-tone-3", "thumbsup::skin-tone-3"] {
        let reaction = crate::slack::models::Reaction {
            name: name.into(),
            count: 2,
            ..Default::default()
        };
        assert_eq!(reaction_summary(&reaction), "\u{1f44d}\u{1f3fc} 2");
        assert_eq!(reaction.name, name);
    }
}

#[test]
fn malformed_unknown_and_unsupported_tones_preserve_full_literal_name() {
    for name in [
        "thumbsup::skin-tone-1",
        "thumbsup::skin-tone-7",
        "thumbsup::skin-tone-03",
        "thumbsup::skin-tone-2-3",
        "thumbsup::skin-tone-3::skin-tone-4",
        "thumbsup::skin-tone-3-extra",
        "thumbsup::",
        "handshake::skin-tone-2-2",
        "smile::skin-tone-3",
        "not_real::skin-tone-3",
        "THUMBSUP::skin-tone-3",
        ":thumbsup::skin-tone-3:",
        "thumbsup::skin-tone-3 ",
        "thumbsup::skin-tone-\u{ff13}",
    ] {
        assert_eq!(tones::glyph(name), None, "{name}");
        assert_eq!(emoji_glyph(name), format!(":{name}:"), "{name}");
    }
}

#[test]
fn ordinary_shortcodes_and_unknown_fallback_remain_unchanged() {
    for name in ["wave", "thumbsup", "+1", "smile", "handshake"] {
        assert_eq!(
            emoji_glyph(name),
            emojis::get_by_shortcode(name).unwrap().as_str()
        );
    }
    assert_eq!(emoji_glyph("not_real"), ":not_real:");
    assert_eq!(emoji_glyph(""), "::");
}

#[test]
fn custom_exact_image_and_alias_resolution_precede_standard_tone_lookup() {
    let emojis = [
        ("wave", "https://example.test/wave.png"),
        ("wave::skin-tone-3", "https://example.test/exact.png"),
        ("my_image", "alias:wave::skin-tone-3"),
        ("my_tone", "alias:thumbsup::skin-tone-3"),
        ("nested_tone", "alias:my_tone"),
        ("cycle", "alias:cycle"),
        ("bad_tone", "alias:thumbsup::skin-tone-9"),
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
        custom_emoji_url(&emojis, "wave"),
        Some("https://example.test/wave.png")
    );
    assert_eq!(
        custom_emoji_source(&emojis, "my_image"),
        Some(EmojiSource::Url("https://example.test/exact.png"))
    );
    assert_eq!(
        custom_emoji_source(&emojis, "nested_tone"),
        Some(EmojiSource::Unicode("\u{1f44d}\u{1f3fc}"))
    );
    for name in ["cycle", "bad_tone", "unsafe"] {
        assert_eq!(custom_emoji_source(&emojis, name), None);
    }
}
