use super::*;
use automerge::{ReadDoc, Value};

fn section(text: &str) -> Section {
    let leaf = RawMessage::from_fields(&[F::bytes(1, text.as_bytes()).unwrap()]).unwrap();
    let content = RawMessage::from_fields(&[F::message(1, &leaf).unwrap()]).unwrap();
    Section::from_raw(
        RawMessage::from_fields(&[
            F::bytes(1, b"synthetic-item").unwrap(),
            F::uint(2, 12_345).unwrap(),
            F::uint(9, 0).unwrap(),
            F::message(12, &content).unwrap(),
        ])
        .unwrap(),
    )
    .unwrap()
}
const SESSION: &str = "012345abcdef";
const NOW_USEC: u64 = 1_791_897_000_123_456;

#[test]
fn bootstrap_is_real_single_change_text_with_source_heads_and_no_expiry() {
    for text in [
        "",
        "Checklist item",
        "雪 café 👩🏽‍💻 e\u{301}",
        "already\u{feff}",
    ] {
        let source = section(text);
        let local = LocalCrdt::bootstrap(&source, SESSION, NOW_USEC).unwrap();
        let wire = local.for_section(&source, SESSION).unwrap();
        let document = Automerge::load(wire.bytes(5).unwrap().unwrap()).unwrap();
        let (value, object) = document.get(ROOT, "text").unwrap().unwrap();
        assert_eq!(value, Value::Object(ObjType::Text));
        assert_eq!(document.text(object).unwrap(), format!("{text}\u{feff}"));
        assert_eq!(
            document.get(ROOT, "deleted").unwrap().unwrap().0,
            Value::from(false)
        );
        let heads = document.get_heads();
        assert_eq!(heads.len(), 1);
        assert_eq!(document.get_changes(&[]).len(), 1);
        assert_eq!(
            document.get_changes(&[])[0].actor_id().to_hex_string(),
            format!("{SESSION}00000000")
        );
        assert_eq!(wire.string(6).unwrap(), Some(heads[0].to_string().as_str()));
        assert_eq!(wire.string(8).unwrap(), wire.string(6).unwrap());
        assert_eq!(wire.uint(2).unwrap(), Some(12_345));
        assert_eq!(wire.uint(3).unwrap(), Some(12_000));
        assert!(
            !wire.has(4),
            "local bootstrap must not invent server expiry"
        );
        assert!(!wire.has(7));
        assert_eq!(
            source,
            section(text),
            "bootstrap never changes canonical bytes"
        );
    }
}

#[test]
fn local_provenance_cannot_cross_owner_or_any_canonical_revision() {
    let source = section("original");
    let local = LocalCrdt::bootstrap(&source, SESSION, NOW_USEC).unwrap();
    assert!(matches!(
        local.for_section(&source, "111111111111"),
        Err(Error::Changed)
    ));
    for changed in [
        section("changed"),
        Section::from_raw(
            source
                .raw()
                .replace_fields(2, &[F::uint(2, 12_346).unwrap()])
                .unwrap(),
        )
        .unwrap(),
        Section::from_raw(
            source
                .raw()
                .replace_fields(99, &[F::uint(99, 1).unwrap()])
                .unwrap(),
        )
        .unwrap(),
    ] {
        assert!(matches!(
            local.for_section(&changed, SESSION),
            Err(Error::Changed)
        ));
    }
}

#[test]
fn unsupported_rtml_or_existing_session_never_gets_rebuilt_as_plain_text() {
    for text in [
        "<b>bold</b>",
        "A &amp; B",
        "a>b",
        "line\nline",
        "\0",
        "\u{85}",
    ] {
        assert!(!supports(&section(text)));
    }
    assert!(!supports(&section(&"x".repeat(MAX_TEXT_BYTES + 1))));
    let source = section("plain");
    for id in ["", "a", "not-hex-1234", "012345abcdef00000000"] {
        assert!(LocalCrdt::bootstrap(&source, id, NOW_USEC).is_err());
    }
    let empty = RawMessage::from_fields(&[]).unwrap();
    let existing = Section::from_raw(
        source
            .raw()
            .replace_fields(38, &[F::message(38, &empty).unwrap()])
            .unwrap(),
    )
    .unwrap();
    assert!(!supports(&existing));
    let deleted = Section::from_raw(
        source
            .raw()
            .replace_fields(3, &[F::uint(3, 1).unwrap()])
            .unwrap(),
    )
    .unwrap();
    assert!(!supports(&deleted));
}

// Generated from the captured TKBX.next + matching public WASM, isolated in a
// Node VM. These six documents contain synthetic text only, never Slack data.
#[test]
fn captured_js_next_documents_load_and_native_bootstrap_matches_their_heads() {
    let cases: serde_json::Value = serde_json::from_str(include_str!("js_oracle.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let text = case["text"].as_str().unwrap();
        let bytes: Vec<u8> = serde_json::from_value(case["bytes"].clone()).unwrap();
        let document = Automerge::load(&bytes).unwrap();
        let (_, object) = document.get(ROOT, "text").unwrap().unwrap();
        assert_eq!(document.text(object).unwrap(), format!("{text}\u{feff}"));
        assert_eq!(
            document.get(ROOT, "deleted").unwrap().unwrap().0,
            Value::from(false)
        );
        let source = section(text);
        let local =
            LocalCrdt::bootstrap(&source, SESSION, case["time"].as_u64().unwrap() * 1_000_000)
                .unwrap();
        let wire = local.for_section(&source, SESSION).unwrap();
        assert_eq!(
            wire.string(8).unwrap().unwrap(),
            case["heads"][0].as_str().unwrap(),
            "same actor and single transaction must have the same change head"
        );
    }
}

/// Explicit offline interoperability gate; root runs this, then the library-only
/// Node oracle loads these Rust bytes. No production documents or network access.
#[test]
#[ignore = "exports synthetic bytes for the captured JavaScript interoperability gate"]
fn export_synthetic_native_bootstrap_for_js() {
    let destination =
        std::env::var("CANVAS_BOOTSTRAP_INTEROP_OUTPUT").expect("explicit synthetic output path");
    let cases: serde_json::Value = serde_json::from_str(include_str!("js_oracle.json")).unwrap();
    let outputs: Vec<_> = cases.as_array().unwrap().iter().map(|case| {
        let text = case["text"].as_str().unwrap();
        let source = section(text);
        let local = LocalCrdt::bootstrap(&source, SESSION, case["time"].as_u64().unwrap() * 1_000_000).unwrap();
        let wire = local.for_section(&source, SESSION).unwrap();
        serde_json::json!({"text":text,"session":SESSION,"time":case["time"],"bytes":wire.bytes(5).unwrap().unwrap(),"heads":[wire.string(8).unwrap().unwrap()]})
    }).collect();
    std::fs::write(destination, serde_json::to_vec(&outputs).unwrap()).unwrap();
}

#[test]
fn preflight_clock_uses_whole_seconds_without_truncating_large_values() {
    let source = section("clock");
    for now in [0, 999_999, 1_000_001, u64::MAX] {
        let local = LocalCrdt::bootstrap(&source, SESSION, now).unwrap();
        let wire = local.for_section(&source, SESSION).unwrap();
        let doc = Automerge::load(wire.bytes(5).unwrap().unwrap()).unwrap();
        assert_eq!(
            doc.get_changes(&[])[0].timestamp(),
            i64::try_from(now / 1_000_000).unwrap()
        );
    }
}
