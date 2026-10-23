use super::*;
use crate::slack_lists::{
    Document, Record,
    comments::{
        OwnEditIntent, OwnEditPolicy,
        edit_seed::{
            plain::{PlainObservations, PlainSeed},
            prepare::*,
        },
        ordered::ResponseWitness,
    },
};
use serde_json::json;
const ACCOUNT: &str = "11111111-2222-4333-8444-555555555555";
struct Temp(std::path::PathBuf);
impl Temp {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("list-plain-edit-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        Self(dir)
    }
    fn open(&self) -> Journal {
        Journal::open(self.0.join("journal.sqlite"), ACCOUNT).unwrap()
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn fixture() -> (OwnEditSeed, PlainDelta, OwnEditRecord) {
    let document = Document::parse(
        json!({"id":"F01234567","mode":"list","list_metadata":{"schema":[],"views":[]}}),
    )
    .unwrap();
    let row =
        Record::parse(json!({"id":"Rec1","fields":[],"created_by":"U1","date_created":1})).unwrap();
    let bytes = br#"{"ok":true,"messages":[{"ts":"1700000000.000001","subtype":"list_record_comment","slack_list":{"list_id":"F01234567","list_record_id":"Rec1"}},
        {"ts":"1700000001.000001","thread_ts":"1700000000.000001","user":"U1","type":"message","text":"Original","blocks":[{"type":"context","elements":[],"opaque":7}]}]}"#;
    let raw = serde_json::from_slice(bytes).unwrap();
    let page = Page::parse_ordered(
        &Target::capture(&document, &row).unwrap(),
        ResponseWitness::decode(bytes, &raw).unwrap(),
    )
    .unwrap();
    let policy = OwnEditPolicy::observed(
        "T1",
        "U1",
        json!({"id":"C01234567"}),
        true,
        false,
        vec![-1],
        true,
        false,
    )
    .unwrap();
    let mut observed = PlainObservations::default();
    observed.seed.emoji_as_text = Some(true);
    let initial = PlainSeed::capture(
        &page.ordered_response().unwrap().message(1).unwrap(),
        &observed,
    )
    .unwrap();
    let seed = OwnEditSeed::capture_plain(
        "T1",
        "U1",
        &page,
        &page.messages()[1],
        &initial,
        &policy,
        1700000002000,
    )
    .unwrap();
    let delta = PlainDelta::capture(vec![json!({"insert":"Changed *bold*\n"})]).unwrap();
    let preparation = PlainPreparation::capture(
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
    let PreparationStep::Complete(prepared) = preparation
        .advance(&MentionObservations::default())
        .unwrap()
    else {
        panic!()
    };
    let intent = OwnEditIntent::capture_plain(
        &seed,
        &initial,
        &page,
        &page.messages()[1],
        &policy,
        &delta,
        &prepared,
        false,
        &EncodingObservations::default(),
        1700000002000,
    )
    .unwrap();
    (seed, delta, intent.record().clone())
}

#[test]
fn plain_inventory_restart_keeps_delta_separate_and_pending_uncertain_nonreplayable() {
    let temp = Temp::new();
    let (seed, delta, record) = fixture();
    let scope = scope_for(&seed).unwrap();
    let mut journal = temp.open();
    let empty = journal.edit_inventory(&scope, seed.ts()).unwrap();
    let saved = journal
        .edit_save_plain(&empty, &seed, &id(), delta.clone())
        .unwrap();
    assert!(saved.blocks().is_empty());
    assert_eq!(saved.plain_delta(), Some(&delta));
    assert!(
        journal
            .edit_save(&saved, &seed, &id(), content::plain("Wrong mode"))
            .is_err()
    );
    let attempt_id = id();
    let pending = journal.edit_begin(&saved, &attempt_id, &record).unwrap();
    drop(journal);
    let mut journal = temp.open();
    let restored = journal.edit_inventory(&scope, seed.ts()).unwrap();
    assert_eq!(restored, pending);
    assert!(restored.attempt().unwrap().phase().locked());
    assert!(journal.edit_begin(&restored, &id(), &record).is_err());
    assert!(journal.edit_clear(&restored).is_err());
    let uncertain = journal
        .edit_resolve(
            &scope,
            seed.ts(),
            &attempt_id,
            EditResolution::Uncertain(None),
            0,
        )
        .unwrap();
    drop(journal);
    let mut journal = temp.open();
    assert_eq!(
        journal.edit_inventory(&scope, seed.ts()).unwrap(),
        uncertain
    );
    assert!(journal.edit_begin(&uncertain, &id(), &record).is_err());
    assert!(
        journal
            .edit_resolve(
                &scope,
                seed.ts(),
                &attempt_id,
                EditResolution::NotRequested,
                0
            )
            .is_err()
    );
}

#[test]
fn plain_capture_cas_and_wire_binding_keep_newer_draft_and_original_attempt() {
    let temp = Temp::new();
    let (seed, delta, record) = fixture();
    let scope = scope_for(&seed).unwrap();
    let mut journal = temp.open();
    let empty = journal.edit_inventory(&scope, seed.ts()).unwrap();
    let capture = id();
    let saved = journal
        .edit_save_plain(&empty, &seed, &capture, delta.clone())
        .unwrap();
    let other = PlainDelta::capture(vec![json!({"insert":"Later\n"})]).unwrap();
    assert!(
        journal
            .edit_save_plain(&saved, &seed, &capture, other.clone())
            .is_err()
    );
    assert!(
        journal
            .edit_save_plain(&empty, &seed, &id(), other.clone())
            .is_err()
    );
    let newer = journal
        .edit_save_plain(&saved, &seed, &id(), other)
        .unwrap();
    assert!(journal.edit_begin(&newer, &id(), &record).is_err());
    assert!(journal.edit_clear(&saved).is_err());
    let empty_draft = journal
        .edit_save_plain(&newer, &seed, &id(), PlainDelta::capture(vec![]).unwrap())
        .unwrap();
    assert_eq!(empty_draft.plain_delta().unwrap().ops(), &[] as &[Value]);
    assert!(journal.edit_begin(&empty_draft, &id(), &record).is_err());
    let cleared = journal.edit_clear(&empty_draft).unwrap();
    assert!(cleared.seed().is_none() && cleared.plain_delta().is_none());
}

#[test]
fn plain_inventory_unknown_version_and_cross_mode_payload_refuse_restoration() {
    let temp = Temp::new();
    let (seed, delta, _) = fixture();
    let scope = scope_for(&seed).unwrap();
    let mut journal = temp.open();
    let empty = journal.edit_inventory(&scope, seed.ts()).unwrap();
    let saved = journal
        .edit_save_plain(&empty, &seed, &id(), delta)
        .unwrap();
    let original = serde_json::to_value(&saved).unwrap();
    for change in ["version", "blocks", "missing"] {
        let mut raw = original.clone();
        match change {
            "version" => raw["plain_delta"]["version"] = json!(9),
            "blocks" => raw["blocks"] = json!(content::plain("Not Delta")),
            _ => {
                raw.as_object_mut().unwrap().remove("plain_delta");
            }
        }
        journal
            .conn
            .execute(
                "UPDATE edit_drafts SET body=?1 WHERE owner=?2",
                params![
                    serde_json::to_string(&raw).unwrap(),
                    key(&scope, seed.ts()).unwrap()
                ],
            )
            .unwrap();
        assert!(journal.edit_inventory(&scope, seed.ts()).is_err());
    }
}
