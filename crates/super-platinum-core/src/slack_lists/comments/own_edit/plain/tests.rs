use super::*;
use crate::slack_lists::comments::edit_seed::{plain::PlainObservations, prepare::*};
use crate::slack_lists::{Document, Record};
use serde_json::json;
const NOW: u64 = 1_700_000_061_000;
fn policy(plain: bool, portable: bool) -> OwnEditPolicy {
    OwnEditPolicy::observed(
        "T1",
        "U1",
        json!({"id":"C01234567"}),
        true,
        false,
        vec![-1],
        plain,
        portable,
    )
    .unwrap()
}
fn page(reversed: bool) -> Page {
    let document = Document::parse(json!({"id":"F01234567","mode":"list",
        "list_metadata":{"schema":[],"views":[]}}))
    .unwrap();
    let row =
        Record::parse(json!({"id":"Rec1","fields":[],"created_by":"U1","date_created":1})).unwrap();
    let bytes = r#"{"ok":true,"messages":[
      {"ts":"1700000000.000001","subtype":"list_record_comment","slack_list":{"list_id":"F01234567","list_record_id":"Rec1"}},
      {"ts":"1700000001.000001","thread_ts":"1700000000.000001","type":"message","user":"U1","text":"Original",
       "blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Original","style":{"italic":true,"bold":true}}]}]},
       {"type":"context","elements":[],"opaque":{"keep":true}}]}]}"#;
    let bytes = if reversed {
        bytes.replace(
            "\"italic\":true,\"bold\":true",
            "\"bold\":true,\"italic\":true",
        )
    } else {
        bytes.into()
    };
    let decoded = serde_json::from_str(&bytes).unwrap();
    let witness = ResponseWitness::decode(bytes.as_bytes(), &decoded).unwrap();
    Page::parse_ordered(&Target::capture(&document, &row).unwrap(), witness).unwrap()
}
fn initialized(page: &Page) -> PlainSeed {
    let original = ordered_message(page, "1700000001.000001").unwrap();
    let mut obs = PlainObservations::default();
    obs.seed.emoji_as_text = Some(true);
    obs.highlight_words = Some(vec![]);
    PlainSeed::capture(&original, &obs).unwrap()
}
fn seed(page: &Page) -> OwnEditSeed {
    let initialized = initialized(page);
    OwnEditSeed::capture_plain(
        "T1",
        "U1",
        page,
        &page.messages()[1],
        &initialized,
        &policy(true, false),
        NOW,
    )
    .unwrap()
}
fn prepared(delta: &PlainDelta) -> PreparedPlain {
    let plan = PlainPreparation::capture(
        delta.ops(),
        PreparationSettings {
            is_emoji_displayed_as_text: true,
            add_text_presentation_selectors: false,
            skip_markdown_code: true,
            use_channel_tagging: false,
        },
        &SyntaxObservations::default(),
        &EmojiObservations {
            current_locale: Some("en-US".into()),
            locale: Some("en-US".into()),
            names: Some(Default::default()),
            translation: EmojiTranslation::KnownAbsent,
            ..Default::default()
        },
    )
    .unwrap();
    let PreparationStep::Complete(value) = plan.advance(&MentionObservations::default()).unwrap()
    else {
        panic!("test input has no mention requests")
    };
    value
}

#[test]
fn ordered_plain_seed_restores_inert_without_manufacturing_rich_blocks() {
    let page = page(false);
    let seed = seed(&page);
    assert_eq!(seed.mode(), EditMode::PlainText);
    assert!(seed.editable().is_empty());
    assert!(!seed.plain_delta().unwrap().ops().is_empty());
    assert_eq!(
        seed.preserved(),
        &page.raw()["messages"][1]["blocks"].as_array().unwrap()[1..]
    );
    let restored: OwnEditSeed =
        serde_json::from_value(serde_json::to_value(&seed).unwrap()).unwrap();
    restored.validate().unwrap();
    assert_eq!(restored, seed);
    assert_eq!(
        restored
            .ordered_message()
            .unwrap()
            .unwrap()
            .style_keys(&[0, 0, 0])
            .unwrap(),
        vec!["italic", "bold"]
    );
    assert!(!restored.unchanged(&[]));
    assert!(
        OwnEditIntent::capture(
            &restored,
            &page.messages()[1],
            &policy(false, false),
            content::plain("Changed"),
            NOW
        )
        .is_err()
    );
    assert!(
        OwnEditSeed::capture(
            "T1",
            "U1",
            &page,
            &page.messages()[1],
            &policy(true, false),
            NOW
        )
        .is_err()
    );
    let rich = OwnEditSeed::capture(
        "T1",
        "U1",
        &page,
        &page.messages()[1],
        &policy(false, false),
        NOW,
    )
    .unwrap();
    let legacy = OwnEditIntent::capture(
        &rich,
        &page.messages()[1],
        &policy(false, false),
        content::plain("Rich edit"),
        NOW,
    )
    .unwrap();
    let raw = serde_json::to_value(legacy.record()).unwrap();
    assert_eq!(raw["version"], 1);
    assert!(raw.get("plain_delta").is_none());
    assert!(raw["seed"].get("plain").is_none());
    let restored: OwnEditRecord = serde_json::from_value(raw).unwrap();
    restored.validate().unwrap();
}

#[test]
fn plain_revalidation_requires_actual_order_and_current_mode_body_owner() {
    let original = page(false);
    let seed = seed(&original);
    let reordered = page(true);
    assert_eq!(original.raw(), reordered.raw());
    assert!(
        seed.revalidate(
            &reordered,
            &reordered.messages()[1],
            &policy(true, false),
            NOW
        )
        .is_err()
    );
    let no_order = Page::parse(original.target(), original.raw().clone()).unwrap();
    assert!(
        seed.revalidate(
            &no_order,
            &no_order.messages()[1],
            &policy(true, false),
            NOW
        )
        .is_err()
    );
    for policy in [policy(false, false), policy(false, true)] {
        assert!(
            seed.revalidate(&original, &original.messages()[1], &policy, NOW)
                .is_err()
        );
    }
    assert!(
        seed.revalidate(
            &original,
            &original.messages()[1],
            &policy(true, false),
            NOW
        )
        .is_ok()
    );
    let mut changed = original.messages()[1].clone();
    changed.text = Some("Changed remotely".into());
    assert!(
        seed.revalidate(&original, &changed, &policy(true, false), NOW)
            .is_err()
    );
}

#[test]
fn plain_save_binds_actual_preparation_and_keeps_wire_blocks_separate() {
    let page = page(false);
    let seed = seed(&page);
    let delta = PlainDelta::capture(vec![json!({"insert":"Changed *bold* \u{4e2d}\n"})]).unwrap();
    let prepared = prepared(&delta);
    let intent = OwnEditIntent::capture_plain(
        &seed,
        &initialized(&page),
        &page,
        &page.messages()[1],
        &policy(true, false),
        &delta,
        &prepared,
        false,
        &EncodingObservations::default(),
        NOW,
    )
    .unwrap();
    let record = intent.record();
    assert_eq!(record.plain_delta(), Some(&delta));
    assert_eq!(record.blocks().last(), seed.preserved().last());
    assert_eq!(record.blocks()[0]["type"], "rich_text");
    let restored: OwnEditRecord =
        serde_json::from_value(serde_json::to_value(record).unwrap()).unwrap();
    restored.validate().unwrap();
    let different = PlainDelta::capture(vec![json!({"insert":"Other\n"})]).unwrap();
    assert!(
        OwnEditIntent::capture_plain(
            &seed,
            &initialized(&page),
            &page,
            &page.messages()[1],
            &policy(true, false),
            &different,
            &prepared,
            false,
            &EncodingObservations::default(),
            NOW
        )
        .is_err()
    );
    let raw = json!({"ok":true,"channel":record.target().channel_id(),"ts":record.ts(),
        "message":{"text":record.text(),"blocks":record.blocks()}});
    assert!(intent.accept(raw).is_ok());
}

#[test]
fn plain_versions_shape_and_preserved_partition_refuse_corruption() {
    for ops in [
        vec![json!({"type":"rich_text","elements":[]})],
        vec![json!({"retain":1})],
        vec![json!({"insert":"x","attributes":false})],
    ] {
        assert!(PlainDelta::capture(ops).is_err());
    }
    assert!(PlainDelta::capture(vec![]).is_ok()); // Empty draft is not Send.
    let s = seed(&page(false));
    for path in ["version", "initial"] {
        let mut value = serde_json::to_value(&s).unwrap();
        if path == "version" {
            value["plain"][path] = json!(2);
        } else {
            value["plain"][path]["version"] = json!(2);
        }
        assert!(
            serde_json::from_value::<OwnEditSeed>(value)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    let mut tampered = serde_json::to_value(&s).unwrap();
    tampered["plain"]["initial"]["ops"] = json!([{"insert":"Forged initial\n"}]);
    let inert: OwnEditSeed = serde_json::from_value(tampered).unwrap();
    inert.validate().unwrap(); // Valid inert content is not initialization authority.
    assert!(
        inert
            .revalidate_plain_initialization(&initialized(&page(false)))
            .is_err()
    );
    let mut value = serde_json::to_value(&s).unwrap();
    value["preserved"] = json!([]);
    assert!(
        serde_json::from_value::<OwnEditSeed>(value)
            .unwrap()
            .validate()
            .is_err()
    );
}
