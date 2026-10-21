//! Display must preserve each raw reaction's identity and custom image priority.
use super::*;
use serde_json::json;

#[test]
fn tone_projection_preserves_raw_rows_membership_and_labels() {
    let core = crate::fixture::fixture_core();
    let message = serde_json::from_value(json!({
        "ts":"1.0", "user":"U1", "reactions":[
            {"name":"thumbsup::skin-tone-3","count":2,"users":["U0","U1"]},
            {"name":"+1::skin-tone-3","count":1,"users":["U2"]},
            {"name":"handshake::skin-tone-2-6","count":3,"users":["U1"]},
            {"name":"thumbsup::skin-tone-9","count":1,"users":["U1"]}
        ]
    }))
    .unwrap();
    let vm = message_vm(&core.workspaces["T1"], &message, &MediaRegistry::default());
    assert_eq!(vm.reactions.len(), 4);
    let expected = [
        ("thumbsup::skin-tone-3", "\u{1f44d}\u{1f3fc}", 2, true),
        ("+1::skin-tone-3", "\u{1f44d}\u{1f3fc}", 1, false),
        (
            "handshake::skin-tone-2-6",
            "\u{1faf1}\u{1f3fb}\u{200d}\u{1faf2}\u{1f3ff}",
            3,
            false,
        ),
        ("thumbsup::skin-tone-9", ":thumbsup::skin-tone-9:", 1, false),
    ];
    for (reaction, (name, glyph, count, own)) in vm.reactions.iter().zip(expected) {
        assert_eq!(reaction.name, name);
        assert_eq!(reaction.glyph.as_deref(), Some(glyph));
        assert_eq!(reaction.count, count);
        assert_eq!(reaction.own, own);
        assert!(reaction.media.is_none());
        assert!(reaction.reactors_label().ends_with(&format!(":{name}:")));
    }
    assert!(vm.reactions[3].resolved_glyph().is_none());
}

#[test]
fn custom_image_and_alias_precedence_survive_tone_projection() {
    let mut core = crate::fixture::fixture_core();
    let workspace = core.workspaces.get_mut("T1").unwrap();
    workspace.apply_emojis(
        serde_json::from_value(json!([
            {"name":"wave","value":"https://example.test/custom-wave.png"},
            {"name":"my-wave","value":"alias:wave"},
            {"name":"my-thumb","value":"alias:thumbsup::skin-tone-4"}
        ]))
        .unwrap(),
    );
    let message = serde_json::from_value(json!({
        "ts":"1.0", "reactions":[
            {"name":"wave::skin-tone-3","count":1,"users":["U1"]},
            {"name":"my-wave","count":1,"users":["U1"]},
            {"name":"my-thumb","count":1,"users":["U1"]},
            {"name":"missing::skin-tone-3","count":1,"users":["U1"]}
        ]
    }))
    .unwrap();
    let vm = message_vm(workspace, &message, &MediaRegistry::default());
    for reaction in &vm.reactions[..2] {
        assert!(reaction.media.is_some());
        assert!(reaction.glyph.is_none());
    }
    assert_eq!(vm.reactions[2].glyph.as_deref(), Some("\u{1f44d}\u{1f3fd}"));
    assert!(vm.reactions[2].media.is_none());
    assert_eq!(
        vm.reactions[3].glyph.as_deref(),
        Some(":missing::skin-tone-3:")
    );
}
