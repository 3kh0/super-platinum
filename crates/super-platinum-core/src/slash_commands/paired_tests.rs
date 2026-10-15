use super::*;
use crate::slash_commands::{Owner, catalog::Catalog};

fn key() -> DraftKey {
    DraftKey {
        team: "T1".into(),
        channel: "C1".into(),
        thread: None,
    }
}
fn owner() -> Owner {
    Owner {
        account: "c765fef0-1f85-49f0-ab9c-fdb779292337".into(),
        team: "T1".into(),
        user: "U1".into(),
        channel: "C1".into(),
        thread: None,
    }
}
fn local(text: &str) -> LocalDraft {
    LocalDraft {
        composer: crate::ComposerState::with_text(text),
        ..Default::default()
    }
}
fn make_catalog() -> Catalog {
    Catalog::decode(
        owner(),
        json!({"ok":true,"app_actions":[],"commands":[{
            "type":"app","name":"/demo","app":"A1","teams":["T1"],"is_runnable_from_thread":true
        }]}),
    )
    .unwrap()
}
fn setup(text: &str) -> (Paired, Arc<Value>) {
    let local = local(text);
    let desired = codec::encode_composer(&key(), &local.composer).unwrap();
    let emoji = Arc::new(json!({}));
    (
        derive(&key(), &local, &desired, emoji.clone()).unwrap(),
        emoji,
    )
}

#[test]
fn actual_codec_snapshot_derives_source_argument_blocks_without_flattening() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("native-results.json")).unwrap();
    let catalog = make_catalog();
    let choice = catalog.choice(0).unwrap();
    assert_eq!(cases.len(), 8);
    for case in cases {
        let local = local(case["text"].as_str().unwrap());
        let desired = codec::encode_composer(&key(), &local.composer).unwrap();
        assert_eq!(desired.blocks[0]["elements"][0]["elements"], case["runs"]);
        let emoji = Arc::new(json!({}));
        let paired = derive(&key(), &local, &desired, emoji.clone()).unwrap();
        let frozen = paired
            .freeze(
                owner(),
                1,
                paired.original.clone(),
                &emoji,
                &choice,
                &catalog,
            )
            .unwrap();
        assert_eq!(frozen.display, case["source"]["display"]);
        assert_eq!(
            frozen.argument_blocks,
            case["source"]["request"]["args"].get("blocks").cloned()
        );
        assert_eq!(
            frozen.original,
            json!({"format":1,"local":local,"desired":desired})
        );
    }
}

#[test]
fn text_or_snapshot_substitution_fails_before_conversion() {
    let first = local("/demo *original*");
    let desired = codec::encode_composer(&key(), &first.composer).unwrap();
    let other = local("/demo *replacement*");
    assert!(derive(&key(), &other, &desired, Arc::new(json!({}))).is_err());
    let mut unknown = desired.clone();
    unknown.blocks[0]["future"] = json!(true);
    assert!(derive(&key(), &first, &unknown, Arc::new(json!({}))).is_err());
}

#[test]
fn freeze_rechecks_full_original_selection_destination_and_catalog_allocation() {
    let (paired, emoji) = setup("/demo *original*");
    let catalog = make_catalog();
    let choice = catalog.choice(0).unwrap();
    let mut changed = paired.original.clone();
    changed["local"]["composer"]["selection"]["start"] = json!(0);
    assert!(
        paired
            .freeze(owner(), 1, changed, &emoji, &choice, &catalog)
            .is_err()
    );
    let mut other = owner();
    other.channel = "C2".into();
    assert!(
        paired
            .freeze(other, 1, paired.original.clone(), &emoji, &choice, &catalog)
            .is_err()
    );
    assert!(
        paired
            .freeze(
                owner(),
                1,
                paired.original.clone(),
                &Arc::new(json!({})),
                &choice,
                &catalog
            )
            .is_err()
    );
    let replacement = make_catalog();
    assert!(
        paired
            .freeze(
                owner(),
                1,
                paired.original.clone(),
                &emoji,
                &choice,
                &replacement
            )
            .is_err()
    );
}

#[test]
fn unsupported_native_content_keeps_existing_codec_barrier() {
    for text in [
        "/demo <@U12345678>",
        "/demo :grinning:",
        "/demo\n> quote",
        "/demo ```\ncode\n```",
    ] {
        assert!(
            codec::encode_composer(&key(), &local(text).composer).is_err(),
            "{text}"
        );
    }
    let (paired, _) = setup("/demo ordinary");
    assert_eq!(
        paired.original["local"]["composer"]["text"],
        "/demo ordinary"
    );
}

#[test]
fn noncommands_never_create_argument_authority_and_thread_broadcast_stays_unavailable() {
    assert!(
        derive(
            &key(),
            &local("ordinary"),
            &codec::encode(&key(), "ordinary").unwrap(),
            Arc::new(json!({}))
        )
        .is_err()
    );
    let mut key = key();
    key.thread = Some("1.000001".into());
    let mut local = local("/demo argument");
    local.composer.reply_broadcast = true;
    let desired = codec::encode_composer(&key, &local.composer).unwrap();
    assert!(derive(&key, &local, &desired, Arc::new(json!({}))).is_err());
}
