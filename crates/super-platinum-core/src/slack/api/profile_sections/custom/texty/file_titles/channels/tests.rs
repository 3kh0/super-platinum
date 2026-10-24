use super::super::{
    EmojiText, Kind, QuipTitle, Status, dependencies_with_emoji_context, label_with_emoji_context,
    label_with_resolvers, user_ids_with_emoji_context,
};
use super::*;
use serde_json::json;
use std::collections::BTreeSet;

fn file(leaf: Value) -> Value {
    json!({"title":"Never substitute", "title_blocks":[{"type":"rich_text","elements":[
        {"type":"rich_text_section","elements":[leaf]}]}]})
}
fn row() -> Value {
    json!({"id":"CCHANNEL001","is_channel":true,"name":"project-old"})
}
fn label(source: &Value, row: &Value, quip: QuipTitle<'_>) -> Result<String, Unavailable> {
    label_with_resolvers(
        Kind::Canvas,
        Status::Normal,
        source,
        quip,
        Some("Untitled"),
        None,
        |_| Ok("@Dana".into()),
        |id| public_channel_label(id, row),
    )
}

#[test]
fn original_channel_vectors_pin_public_names_quip_and_actual_user_bypass() {
    let oracle: Value = serde_json::from_str(include_str!("oracle.json")).unwrap();
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 27);
    let (mut admitted, mut refused) = (0, 0);
    for case in cases {
        let source = case["file"].clone();
        let dependencies = dependencies_with_emoji_context(&source, None).unwrap();
        assert_eq!(dependencies.channels, ["CCHANNEL001".to_owned()].into());
        let actual_users = case["userIds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect::<BTreeSet<_>>();
        assert_eq!(dependencies.users, actual_users);
        let quip = match case["overrideName"].as_str().unwrap() {
            "nullish" => QuipTitle::Nullish,
            "empty" => QuipTitle::Text(""),
            "text" => QuipTitle::Text("Independent title"),
            _ => panic!("unrecognized original vector"),
        };
        let row = &case["state"]["channels"]["CCHANNEL001"];
        let result = label(&source, row, quip);
        if matches!(
            case["name"].as_str().unwrap(),
            "public" | "renamed" | "archived" | "mixed-user"
        ) {
            assert_eq!(
                result.unwrap(),
                case["finalLabel"].as_str().unwrap(),
                "{case}"
            );
            admitted += 1;
        } else {
            assert!(
                result.is_err(),
                "private/unknown fallback is outside this admission: {case}"
            );
            refused += 1;
        }
        assert_eq!(source, case["file"]); // opaque fields, style, source bytes untouched
        assert!(user_ids_with_emoji_context(&source, None).is_err());
        assert!(
            label_with_emoji_context(
                Kind::Canvas,
                Status::Normal,
                &source,
                quip,
                Some("Untitled"),
                None,
                |_| Ok("@Dana".into())
            )
            .is_err()
        );
    }
    assert_eq!((admitted, refused), (12, 15));
}

#[test]
fn channel_resolution_precedes_override_and_only_actual_user_bypasses_quip() {
    let mut source = file(json!({"type":"channel","channel_id":"CCHANNEL001"}));
    assert!(label(&source, &row(), QuipTitle::Unavailable).is_err());
    for quip in [
        QuipTitle::Nullish,
        QuipTitle::Text(""),
        QuipTitle::Text("override"),
    ] {
        let mut hits = Vec::new();
        let result = label_with_resolvers(
            Kind::Canvas,
            Status::Normal,
            &source,
            quip,
            None,
            None,
            |_| panic!("channel must not request a user"),
            |id| {
                hits.push(id.to_owned());
                Err(Unavailable::InvalidMetadata)
            },
        );
        assert_eq!(result, Err(Unavailable::InvalidMetadata));
        assert_eq!(hits, ["CCHANNEL001"]);
    }
    source["title_blocks"][0]["elements"][0]["elements"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type":"user","user_id":"UCHANNEL001"}));
    assert_eq!(
        label(&source, &row(), QuipTitle::Unavailable).unwrap(),
        "#project-old@Dana"
    );
}

#[test]
fn malformed_channel_leaf_refuses_in_both_scanner_and_extractor() {
    for leaf in [
        json!({"type":"channel"}),
        json!({"type":"channel","channel_id":5}),
        json!({"type":"channel","channel_id":""}),
        json!({"type":"channel","channel_id":"C"}),
        json!({"type":"channel","channel_id":"DCHANNEL001"}),
        json!({"type":"channel","channel_id":"C bad"}),
        json!({"type":"channel","channel_id":"CCHANNEL001","tab_id":"special"}),
        json!({"type":"channel","channel_id":"CCHANNEL001","tab_id":false}),
    ] {
        let source = file(leaf);
        assert!(dependencies_with_emoji_context(&source, None).is_err());
        assert!(
            label(
                &source,
                &row(),
                QuipTitle::Text("cannot hide invalid source")
            )
            .is_err()
        );
    }
    for tab in [Value::Null, json!("")] {
        let source = file(
            json!({"type":"channel","channel_id":"CCHANNEL001","tab_id":tab,"opaque":{"keep":1}}),
        );
        assert_eq!(
            label(&source, &row(), QuipTitle::Nullish).unwrap(),
            "#project-old"
        );
    }
}

#[test]
fn public_row_admission_refuses_special_private_malformed_and_foreign_sources() {
    for patch in [
        json!({"id":"COTHER"}),
        json!({"is_channel":false}),
        json!({"is_channel":null}),
        json!({"is_private":true}),
        json!({"is_private":"false"}),
        json!({"is_group":true}),
        json!({"is_im":true}),
        json!({"is_mpim":true}),
        json!({"is_file":true}),
        json!({"isUnknown":true}),
        json!({"isNonExistent":true}),
        json!({"is_deleted":true}),
        json!({"properties":[]}),
        json!({"properties":{"huddles":true}}),
        json!({"properties":{"huddles":{"is_huddle_channel":true}}}),
        json!({"properties":{"meetings_channel":{"is_meeting_channel":true}}}),
        json!({"properties":{"record_channel":{"record_id":"R1"}}}),
        json!({"name":""}),
        json!({"name":5}),
        json!({"name":"mc:calendar"}),
    ] {
        let mut source = row();
        source
            .as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        assert!(
            public_channel_label("CCHANNEL001", &source).is_err(),
            "{source}"
        );
    }
    assert!(public_channel_label("CCHANNEL001", &Value::Null).is_err());
    let source = json!({"id":"CCHANNEL001","is_channel":true,"is_archived":true,
        "name":"Cafe\u{301} &amp; plan","opaque":{"keep":true}});
    assert_eq!(
        public_channel_label("CCHANNEL001", &source).unwrap(),
        "#Cafe\u{301} &amp; plan"
    );
}

#[test]
fn channel_context_does_not_grant_emoji_locale_or_file_status_authority() {
    let mut source = file(json!({"type":"channel","channel_id":"CCHANNEL001"}));
    source["title_blocks"][0]["elements"][0]["elements"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type":"emoji","name":"wave"}));
    assert!(dependencies_with_emoji_context(&source, None).is_err());
    assert!(label(&source, &row(), QuipTitle::Nullish).is_err());
    let emoji = Some(EmojiText::EnglishCanonicalNames);
    let dependencies = dependencies_with_emoji_context(&source, emoji).unwrap();
    assert!(dependencies.users.is_empty());
    assert_eq!(
        label_with_resolvers(
            Kind::Canvas,
            Status::Normal,
            &source,
            QuipTitle::Nullish,
            None,
            emoji,
            |_| panic!("no users"),
            |id| public_channel_label(id, &row())
        )
        .unwrap(),
        "#project-old:wave:"
    );
    assert_eq!(
        label_with_resolvers(
            Kind::Canvas,
            Status::Unavailable,
            &source,
            QuipTitle::Nullish,
            Some("Untitled"),
            emoji,
            |_| panic!("no users"),
            |_| panic!("status first")
        ),
        Err(Unavailable::Status)
    );
    let empty = file(json!({"type":"text","text":""}));
    assert_eq!(
        label(&empty, &row(), QuipTitle::Text("override")),
        Ok("override".into())
    );
    assert_eq!(
        label_with_resolvers(
            Kind::Canvas,
            Status::Normal,
            &empty,
            QuipTitle::Text("override"),
            None,
            emoji,
            |_| unreachable!(),
            |_| unreachable!()
        ),
        Err(Unavailable::LocalizedFallback)
    );
}
