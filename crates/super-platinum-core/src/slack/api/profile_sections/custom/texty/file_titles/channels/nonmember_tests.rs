use super::super::{
    EmojiText, Kind, QuipTitle, Status, dependencies_with_emoji_context, label_with_resolvers,
};
use super::*;
use serde_json::json;

const ENGLISH: Option<PrivateChannelText> = Some(PrivateChannelText::EnglishPlaceholder);

fn row() -> Value {
    json!({"id":"CCHANNEL001","is_channel":true,"is_private":true,
        "is_member":false,"name":"must-not-leak"})
}

fn source() -> Value {
    json!({"title":"Do not substitute","title_blocks":[{"type":"rich_text",
        "opaque":{"keep":1},"elements":[{"type":"rich_text_section","elements":[
        {"type":"text","text":"Notes for "},{"type":"channel","channel_id":"CCHANNEL001",
        "style":{"bold":true},"opaque":{"keep":2}}]}]}]})
}

fn label(
    source: &Value,
    row: &Value,
    quip: QuipTitle<'_>,
    context: Option<PrivateChannelText>,
) -> Result<String, Unavailable> {
    label_with_resolvers(
        Kind::Canvas,
        Status::Normal,
        source,
        quip,
        Some("Untitled"),
        None,
        |_| Ok("@Dana".into()),
        |id| known_ordinary_channel_label_with_context(id, row, context),
    )
}

#[test]
fn retained_nonmember_quip_vectors_and_existing_admissions() {
    let original: Value = serde_json::from_str(include_str!("oracle.json")).unwrap();
    let cases = original["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 27);
    let (mut newly_admitted, mut compatible, mut refused) = (0, 0, 0);
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
        let result = label(&source, row, quip, ENGLISH);
        match case["name"].as_str().unwrap() {
            "private-nonmember" => {
                let actual = result.unwrap();
                assert_eq!(actual, case["finalLabel"].as_str().unwrap());
                assert!(!actual.contains("must-not-leak"));
                assert!(label(&source, row, quip, None).is_err());
                assert!(known_ordinary_channel_label("CCHANNEL001", row).is_err());
                assert!(public_channel_label("CCHANNEL001", row).is_err());
                newly_admitted += 1;
            }
            "public" | "renamed" | "archived" | "mixed-user" | "private-member" => {
                assert_eq!(result.unwrap(), case["finalLabel"].as_str().unwrap());
                assert_eq!(
                    known_ordinary_channel_label_with_context("CCHANNEL001", row, ENGLISH),
                    known_ordinary_channel_label("CCHANNEL001", row)
                );
                compatible += 1;
            }
            _ => {
                assert!(result.is_err(), "unsupported retained source: {case}");
                refused += 1;
            }
        }
        assert_eq!(*row, before);
        assert_eq!(source, case["file"]);
    }
    assert_eq!((newly_admitted, compatible, refused), (3, 15, 9));
}

#[test]
fn explicit_context_never_interpolates_private_name_or_infers_membership() {
    for name in [
        "must-not-leak",
        "private-renamed",
        "secret &amp; \u{65e5}\u{672c}",
    ] {
        let mut r = row();
        r["name"] = json!(name);
        let before = r.clone();
        assert_eq!(
            known_ordinary_channel_label_with_context("CCHANNEL001", &r, ENGLISH).unwrap(),
            "#private-channel"
        );
        assert!(known_ordinary_channel_label_with_context("CCHANNEL001", &r, None).is_err());
        assert_eq!(r, before);
    }
    let mut absent_name = row();
    absent_name.as_object_mut().unwrap().remove("name");
    let mut unusable_names = vec![absent_name];
    for name in [Value::Null, json!(""), json!(5), json!([]), json!({})] {
        let mut r = row();
        r["name"] = name;
        unusable_names.push(r);
    }
    for mut r in unusable_names {
        assert_eq!(
            known_ordinary_channel_label_with_context("CCHANNEL001", &r, ENGLISH).unwrap(),
            "#private-channel"
        );
        assert!(known_ordinary_channel_label_with_context("CCHANNEL001", &r, None).is_err());
        r["is_member"] = json!(true);
        assert!(known_ordinary_channel_label_with_context("CCHANNEL001", &r, ENGLISH).is_err());
        r["is_private"] = json!(false);
        assert!(public_channel_label("CCHANNEL001", &r).is_err());
    }
    // The typed context has no malformed/foreign locale or arbitrary text
    // constructor. None is absence, never an inferred global English fallback.
    let mut absent = row();
    absent.as_object_mut().unwrap().remove("is_member");
    let mut invalid = vec![absent];
    for membership in [Value::Null, json!("false"), json!(0), json!([]), json!({})] {
        let mut r = row();
        r["is_member"] = membership;
        invalid.push(r);
    }
    for r in invalid {
        for quip in [
            QuipTitle::Nullish,
            QuipTitle::Text(""),
            QuipTitle::Text("override"),
        ] {
            assert!(label(&source(), &r, quip, ENGLISH).is_err());
        }
    }
    let mut member = row();
    member["is_member"] = json!(true);
    assert_eq!(
        known_ordinary_channel_label_with_context("CCHANNEL001", &member, ENGLISH).unwrap(),
        "#must-not-leak"
    );
    member["is_private"] = json!(false);
    member.as_object_mut().unwrap().remove("is_member");
    assert_eq!(
        known_ordinary_channel_label_with_context("CCHANNEL001", &member, ENGLISH),
        public_channel_label("CCHANNEL001", &member)
    );
}

#[test]
fn placeholder_does_not_bypass_ordinary_shape_or_source_identity() {
    for patch in [
        json!({"id":"COTHER"}),
        json!({"id":"GCHANNEL001"}),
        json!({"is_channel":false}),
        json!({"is_channel":"true"}),
        json!({"is_private":"true"}),
        json!({"is_private":1}),
        json!({"is_group":true}),
        json!({"is_im":true}),
        json!({"is_mpim":true}),
        json!({"is_file":true}),
        json!({"isUnknown":true}),
        json!({"isNonExistent":true}),
        json!({"is_deleted":true}),
        json!({"is_group":"false"}),
        json!({"properties":[]}),
        json!({"properties":{"huddles":{"is_huddle_channel":true}}}),
        json!({"properties":{"meetings_channel":{"is_meeting_channel":true}}}),
        json!({"properties":{"record_channel":{"record_id":"R1"}}}),
        json!({"properties":{"record_channel":false}}),
        json!({"name":"mc:calendar"}),
    ] {
        let mut r = row();
        r.as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        assert!(
            known_ordinary_channel_label_with_context("CCHANNEL001", &r, ENGLISH).is_err(),
            "{r}"
        );
    }
    for id in ["", "C", "C bad", "cCHANNEL001", "GCHANNEL001"] {
        let mut r = row();
        r["id"] = json!(id);
        assert!(known_ordinary_channel_label_with_context(id, &r, ENGLISH).is_err());
    }
    for r in [Value::Null, json!([]), json!("private")] {
        assert!(known_ordinary_channel_label_with_context("CCHANNEL001", &r, ENGLISH).is_err());
    }
}

#[test]
fn placeholder_context_does_not_grant_quip_user_or_emoji_authority() {
    let mut s = source();
    let deps = dependencies_with_emoji_context(&s, None).unwrap();
    assert!(deps.users.is_empty());
    assert_eq!(deps.channels, ["CCHANNEL001".to_owned()].into());
    assert!(label(&s, &row(), QuipTitle::Unavailable, ENGLISH).is_err());
    s["title_blocks"][0]["elements"][0]["elements"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type":"user","user_id":"UCHANNEL001"}));
    assert_eq!(
        label(&s, &row(), QuipTitle::Unavailable, ENGLISH).unwrap(),
        "Notes for #private-channel@Dana"
    );
    assert!(label(&s, &row(), QuipTitle::Text("override"), None).is_err());
    s["title_blocks"][0]["elements"][0]["elements"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type":"emoji","name":"wave"}));
    assert!(dependencies_with_emoji_context(&s, None).is_err());
    assert!(label(&s, &row(), QuipTitle::Text("override"), ENGLISH).is_err());
    let before = s.clone();
    assert_eq!(
        label_with_resolvers(
            Kind::Canvas,
            Status::Normal,
            &s,
            QuipTitle::Text("ignored"),
            Some("Untitled"),
            Some(EmojiText::EnglishCanonicalNames),
            |_| Ok("@Dana".into()),
            |id| known_ordinary_channel_label_with_context(id, &row(), ENGLISH)
        )
        .unwrap(),
        "Notes for #private-channel@Dana:wave:"
    );
    assert_eq!(s, before);
}
