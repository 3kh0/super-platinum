use super::*;
use super_platinum_core::ComposerState;

#[test]
fn native_selection_rejects_offsets_inside_surrogate_pairs_or_outside_wire() {
    let wire = "Hi \u{1f98a} <@U1>";
    assert!(valid_selection(wire, 3, 5));
    assert!(!valid_selection(wire, 4, 5));
    assert!(!valid_selection(wire, 3, 4));
    assert!(!valid_selection(wire, 99, 99));
    assert!(!valid_selection(wire, 5, 3));
    assert!(!valid_selection(&"a".repeat(MAX_UNITS + 1), 0, 0));
}

#[test]
fn remote_byte_selection_converts_to_exact_native_wire_offsets() {
    let mut draft = LocalDraft {
        composer: ComposerState::with_text("\u{65e5}\u{1f98a}<@U1>"),
        attachments: vec![],
    };
    draft.composer.set_selection(3, 7);
    let converted = replacement(&draft).unwrap();
    assert_eq!(converted.start, 1);
    assert_eq!(converted.end, 3);
    assert_eq!(converted.direction, Direction::Forward);
    draft.composer.selection.start = 4;
    assert!(replacement(&draft).is_none());
}

#[test]
fn predecessor_comparison_keeps_selection_broadcast_and_presence_exact() {
    let first = LocalDraft {
        composer: ComposerState::with_text("before"),
        attachments: vec![],
    };
    let mut changed = first.clone();
    changed.composer.set_selection(0, 0);
    assert!(!same_plain(&first, &changed));
    changed = first.clone();
    changed.composer.reply_broadcast = true;
    assert!(!same_plain(&first, &changed));
    assert!(!same_book(None, Some(&LocalDraft::default())));
    assert!(same_book(Some(&first), Some(&first)));
}

#[test]
fn typed_native_receipts_reject_extra_fields_unknown_variants_and_bad_direction() {
    assert!(serde_json::from_str::<NativeReceipt>(r#"{"status":"applied","ok":true}"#).is_err());
    assert!(
        serde_json::from_str::<NativeReceipt>(r#"{"status":"unavailable","ok":true}"#).is_err()
    );
    for status in ["unavailable", "composing", "selectionchanged"] {
        assert!(
            serde_json::from_value::<NativeReceipt>(serde_json::json!({"status":status})).is_ok(),
            "valid fieldless receipt: {status}"
        );
        assert!(
            serde_json::from_value::<NativeReceipt>(serde_json::json!({"status":status,"ok":true}))
                .is_err(),
            "extra field on fieldless receipt: {status}"
        );
    }
    assert!(serde_json::from_str::<NativeReceipt>(r#"{"status":"cleared"}"#).is_err());
    let current = serde_json::json!({"id":"channel-composer","scope":"scope","visit":"1", "input":0,
        "composition":0,"composing":false,"wire":"hello","start":0,"end":5,"direction":"sideways",
        "display_start":0,"display_end":5,"display_len":5});
    assert!(
        serde_json::from_value::<NativeReceipt>(
            serde_json::json!({"status":"changed","current":current})
        )
        .is_err()
    );
    assert!(!valid_token("0"));
    assert!(!valid_token("9007199254740992"));
    assert!(valid_token("9007199254740991"));
}
