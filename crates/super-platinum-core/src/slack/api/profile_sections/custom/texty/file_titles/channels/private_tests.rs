use super::super::{
    EmojiText, Kind, QuipTitle, Status, dependencies_with_emoji_context, label_with_emoji_context,
    label_with_resolvers,
};
use super::*;
use serde_json::json;

fn row() -> Value {
    json!({"id":"CCHANNEL001","is_channel":true,"is_private":true,
        "is_member":true,"name":"private-project"})
}
fn source() -> Value {
    json!({"title":"Do not substitute", "title_blocks":[{"type":"rich_text",
        "opaque":{"keep":1},"elements":[{"type":"rich_text_section","elements":[
        {"type":"text","text":"Notes for "},{"type":"channel","channel_id":"CCHANNEL001",
        "style":{"bold":true},"opaque":{"keep":2}}]}]}]})
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
        |id| known_ordinary_channel_label(id, row),
    )
}

#[test]
fn retained_private_member_vectors_and_public_compatibility() {
    let original: Value = serde_json::from_str(include_str!("oracle.json")).unwrap();
    let cases = original["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 27);
    let (mut private_admitted, mut old_public_admitted, mut refused) = (0, 0, 0);
    for case in cases {
        let source = case["file"].clone();
        let row = &case["state"]["channels"]["CCHANNEL001"];
        let before = row.clone();
        let quip = match case["overrideName"].as_str().unwrap() {
            "nullish" => QuipTitle::Nullish,
            "empty" => QuipTitle::Text(""),
            "text" => QuipTitle::Text("Independent title"),
            _ => panic!("unrecognized retained vector"),
        };
        let name = case["name"].as_str().unwrap();
        let result = label(&source, row, quip);
        if name == "private-member" {
            assert_eq!(result.unwrap(), case["finalLabel"].as_str().unwrap());
            assert_eq!(
                public_channel_label("CCHANNEL001", row),
                Err(Unavailable::UnsupportedTitle)
            );
            private_admitted += 1;
        } else if matches!(name, "public" | "renamed" | "archived" | "mixed-user") {
            assert_eq!(result.unwrap(), case["finalLabel"].as_str().unwrap());
            assert_eq!(
                known_ordinary_channel_label("CCHANNEL001", row),
                public_channel_label("CCHANNEL001", row)
            );
            old_public_admitted += 1;
        } else {
            assert!(
                result.is_err(),
                "no private-nonmember/unknown fallback: {case}"
            );
            refused += 1;
        }
        assert_eq!(*row, before);
        assert_eq!(source, case["file"]);
    }
    assert_eq!(
        (private_admitted, old_public_admitted, refused),
        (3, 12, 12)
    );
}

#[test]
fn missing_false_and_malformed_membership_cannot_disclose_private_name_even_with_override() {
    let mut absent = row();
    absent.as_object_mut().unwrap().remove("is_member");
    let mut rows = vec![absent];
    for member in [
        Value::Null,
        json!(false),
        json!(1),
        json!("true"),
        json!([]),
        json!({}),
    ] {
        let mut r = row();
        r["is_member"] = member;
        rows.push(r);
    }
    for r in rows {
        let before = r.clone();
        assert_eq!(
            known_ordinary_channel_label("CCHANNEL001", &r),
            Err(Unavailable::UnsupportedTitle)
        );
        for quip in [
            QuipTitle::Nullish,
            QuipTitle::Text(""),
            QuipTitle::Text("override"),
        ] {
            assert!(label(&source(), &r, quip).is_err());
        }
        assert_eq!(r, before);
    }
    let old = row();
    let mut renamed = old.clone();
    renamed["name"] = json!("private-renamed");
    assert_eq!(
        known_ordinary_channel_label("CCHANNEL001", &old).unwrap(),
        "#private-project"
    );
    assert_eq!(
        known_ordinary_channel_label("CCHANNEL001", &renamed).unwrap(),
        "#private-renamed"
    );
    renamed["is_member"] = json!(false);
    assert!(known_ordinary_channel_label("CCHANNEL001", &renamed).is_err());
}

#[test]
fn private_admission_preserves_all_special_and_malformed_guards_and_exact_name() {
    for patch in [
        json!({"id":"GCHANNEL001"}),
        json!({"id":"COTHER"}),
        json!({"is_channel":false}),
        json!({"is_channel":null}),
        json!({"is_channel":"true"}),
        json!({"is_private":"true"}),
        json!({"is_private":1}),
        json!({"is_private":{}}),
        json!({"is_group":true}),
        json!({"is_im":true}),
        json!({"is_mpim":true}),
        json!({"is_file":true}),
        json!({"isUnknown":true}),
        json!({"isNonExistent":true}),
        json!({"is_deleted":true}),
        json!({"properties":[]}),
        json!({"properties":{"huddles":{"is_huddle_channel":true}}}),
        json!({"properties":{"meetings_channel":{"is_meeting_channel":true}}}),
        json!({"properties":{"record_channel":{"record_id":"R1"}}}),
        json!({"properties":{"record_channel":true}}),
        json!({"name":""}),
        json!({"name":null}),
        json!({"name":5}),
        json!({"name":"mc:calendar"}),
    ] {
        let mut r = row();
        r.as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        assert!(
            known_ordinary_channel_label("CCHANNEL001", &r).is_err(),
            "{r}"
        );
    }
    let legacy = json!({"id":"GCHANNEL001","is_channel":true,"is_private":true,"is_member":true,"name":"private-name"});
    assert!(known_ordinary_channel_label("GCHANNEL001", &legacy).is_err());
    for id in ["", "C", "C bad", "cCHANNEL001"] {
        assert!(known_ordinary_channel_label(id, &row()).is_err());
    }
    let mut r = row();
    r["name"] = json!("Cafe\u{301} &amp; name");
    r["is_archived"] = json!(true);
    assert_eq!(
        known_ordinary_channel_label("CCHANNEL001", &r).unwrap(),
        "#Cafe\u{301} &amp; name"
    );
    // Existing public admission never required membership; do not regress it.
    for private in [Value::Null, json!(false)] {
        let mut r = row();
        r["is_private"] = private;
        r.as_object_mut().unwrap().remove("is_member");
        assert_eq!(
            known_ordinary_channel_label("CCHANNEL001", &r),
            public_channel_label("CCHANNEL001", &r)
        );
        assert!(known_ordinary_channel_label("CCHANNEL001", &r).is_ok());
    }
}

#[test]
fn private_membership_does_not_grant_user_emoji_or_locale_authority() {
    let mut s = source();
    let deps = dependencies_with_emoji_context(&s, None).unwrap();
    assert!(deps.users.is_empty());
    assert_eq!(deps.channels, ["CCHANNEL001".to_owned()].into());
    assert!(label(&s, &row(), QuipTitle::Unavailable).is_err());
    assert!(
        label_with_emoji_context(
            Kind::Canvas,
            Status::Normal,
            &s,
            QuipTitle::Nullish,
            Some("Untitled"),
            None,
            |_| unreachable!()
        )
        .is_err()
    );
    s["title_blocks"][0]["elements"][0]["elements"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type":"user","user_id":"UCHANNEL001"}));
    assert_eq!(
        label(&s, &row(), QuipTitle::Unavailable).unwrap(),
        "Notes for #private-project@Dana"
    );
    s["title_blocks"][0]["elements"][0]["elements"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type":"emoji","name":"wave"}));
    assert!(dependencies_with_emoji_context(&s, None).is_err());
    assert!(label(&s, &row(), QuipTitle::Nullish).is_err());
    let emoji = Some(EmojiText::EnglishCanonicalNames);
    let before = s.clone();
    assert_eq!(
        label_with_resolvers(
            Kind::Canvas,
            Status::Normal,
            &s,
            QuipTitle::Text("ignored"),
            Some("Untitled"),
            emoji,
            |_| Ok("@Dana".into()),
            |id| known_ordinary_channel_label(id, &row())
        )
        .unwrap(),
        "Notes for #private-project@Dana:wave:"
    );
    assert_eq!(s, before);
    let mut nonmember = row();
    nonmember["is_member"] = json!(false);
    assert!(
        label_with_resolvers(
            Kind::Canvas,
            Status::Normal,
            &s,
            QuipTitle::Text("ignored"),
            Some("Untitled"),
            emoji,
            |_| Ok("@Dana".into()),
            |id| known_ordinary_channel_label(id, &nonmember)
        )
        .is_err()
    );
}
