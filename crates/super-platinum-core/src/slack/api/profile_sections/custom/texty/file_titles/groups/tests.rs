use super::super::{
    Kind, QuipTitle, Status, dependencies_with_emoji_context, dependencies_with_groups,
    label_with_group_resolver, label_with_resolvers, user_ids,
};
use super::*;
use serde_json::json;
use std::collections::BTreeSet;

fn file(leaves: Vec<Value>) -> Value {
    json!({"title":"Wrong fallback","title_blocks":[{"type":"rich_text","opaque":1,
        "elements":[{"type":"rich_text_section","elements":leaves}]}]})
}
fn leaf() -> Value {
    json!({"type":"usergroup","usergroup_id":"SGROUP001","style":{"bold":true},
        "tab_id":"opaque-existing-group-tab","opaque":{"keep":1}})
}
fn label(source: &Value, group: &UserGroup, quip: QuipTitle<'_>) -> Result<String, Unavailable> {
    label_with_group_resolver(
        Kind::Canvas,
        Status::Normal,
        source,
        quip,
        Some("Untitled"),
        None,
        |_| Ok("@Dana".into()),
        |_| Ok("#channel".into()),
        |id| known_group_label(id, group),
    )
}

#[test]
fn original_thirty_vectors_preserve_nested_spelling_quip_and_scoped_refusals() {
    let oracle: Value = serde_json::from_str(include_str!("oracle.json")).unwrap();
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 30);
    let (mut admitted, mut refused) = (0, 0);
    for case in cases {
        let source = &case["file"];
        let before = source.clone();
        let raw = &case["state"]["userGroups"]["SGROUP001"];
        let group = UserGroup::from_observed(raw).unwrap();
        let dependencies = dependencies_with_groups(source, None).unwrap();
        assert_eq!(dependencies.groups, ["SGROUP001".to_owned()].into());
        assert!(dependencies.channels.is_empty());
        let users = case["userIds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect::<BTreeSet<_>>();
        assert_eq!(dependencies.users, users);
        let quip = match case["overrideName"].as_str().unwrap() {
            "nullish" => QuipTitle::Nullish,
            "empty" => QuipTitle::Text(""),
            "text" => QuipTitle::Text("Independent title"),
            _ => panic!("unknown original vector"),
        };
        let result = label(source, &group, quip);
        if matches!(
            case["name"].as_str().unwrap(),
            "unknown" | "nonexistent" | "truthy-flags"
        ) {
            assert!(result.is_err(), "{case}");
            refused += 1;
        } else {
            assert_eq!(
                result.unwrap(),
                case["finalLabel"].as_str().unwrap(),
                "{case}"
            );
            assert_eq!(
                known_group_label("SGROUP001", &group).unwrap(),
                case["directHandle"].as_str().unwrap()
            );
            admitted += 1;
        }
        assert_eq!(*source, before);
        assert_eq!(group.label_source.as_ref(), Some(raw));
    }
    assert_eq!((admitted, refused), (27, 3));
}

#[test]
fn empty_null_handles_and_full_empty_fallback_are_separate_from_quip() {
    for raw in [
        json!({"id":"SGROUP001"}),
        json!({"id":"SGROUP001","handle":""}),
        json!({"id":"SGROUP001","handle":null}),
        json!({"id":"SGROUP001","handle":false}),
        json!({"id":"SGROUP001","handle":0}),
    ] {
        let group = UserGroup::from_observed(&raw).unwrap();
        assert_eq!(known_group_label("SGROUP001", &group).unwrap(), "");
        let source = file(vec![leaf()]);
        assert_eq!(
            label(&source, &group, QuipTitle::Nullish).unwrap(),
            "Untitled"
        );
        assert_eq!(label(&source, &group, QuipTitle::Text("")).unwrap(), "");
        assert!(
            label_with_group_resolver(
                Kind::Canvas,
                Status::Normal,
                &source,
                QuipTitle::Nullish,
                None,
                None,
                |_| unreachable!(),
                |_| unreachable!(),
                |id| known_group_label(id, &group)
            )
            .is_err()
        );
    }
}

#[test]
fn dependency_sets_are_separate_and_only_actual_users_bypass_quip() {
    let group = UserGroup::from_observed(&json!({"id":"SGROUP001","handle":"design"})).unwrap();
    let mut leaves = vec![
        leaf(),
        json!({"type":"channel","channel_id":"CCHANNEL001"}),
        leaf(),
    ];
    let source = file(leaves.clone());
    let d = dependencies_with_groups(&source, None).unwrap();
    assert_eq!(d.groups, ["SGROUP001".into()].into());
    assert_eq!(d.channels, ["CCHANNEL001".into()].into());
    assert!(d.users.is_empty());
    assert!(label(&source, &group, QuipTitle::Unavailable).is_err());
    assert_eq!(
        label(&source, &group, QuipTitle::Text("override")).unwrap(),
        "override"
    );
    leaves.push(json!({"type":"user","user_id":"UUSER001"}));
    let source = file(leaves);
    assert_eq!(
        dependencies_with_groups(&source, None).unwrap().users,
        ["UUSER001".into()].into()
    );
    assert_eq!(
        label(&source, &group, QuipTitle::Unavailable).unwrap(),
        "@design#channel@design@Dana"
    );
    assert!(
        label_with_group_resolver(
            Kind::Canvas,
            Status::Normal,
            &source,
            QuipTitle::Text("override"),
            Some("Untitled"),
            None,
            |_| Ok("@Dana".into()),
            |_| Ok("#channel".into()),
            |_| Err(Unavailable::InvalidMetadata)
        )
        .is_err()
    );
}

#[test]
fn legacy_wrappers_still_refuse_groups_and_existing_other_leaves_are_unchanged() {
    let source = file(vec![leaf()]);
    assert!(dependencies_with_emoji_context(&source, None).is_err());
    assert!(user_ids(&source).is_err());
    assert!(
        label_with_resolvers(
            Kind::Canvas,
            Status::Normal,
            &source,
            QuipTitle::Text("override"),
            Some("Untitled"),
            None,
            |_| Ok("@Dana".into()),
            |_| Ok("#channel".into())
        )
        .is_err()
    );
    let plain = file(vec![
        json!({"type":"text","text":"Notes "}),
        json!({"type":"channel","channel_id":"CCHANNEL001"}),
        json!({"type":"user","user_id":"UUSER001"}),
    ]);
    let old = label_with_resolvers(
        Kind::Canvas,
        Status::Normal,
        &plain,
        QuipTitle::Unavailable,
        None,
        None,
        |_| Ok("@Dana".into()),
        |_| Ok("#private-channel".into()),
    )
    .unwrap();
    let new = label_with_group_resolver(
        Kind::Canvas,
        Status::Normal,
        &plain,
        QuipTitle::Unavailable,
        None,
        None,
        |_| Ok("@Dana".into()),
        |_| Ok("#private-channel".into()),
        |_| unreachable!(),
    )
    .unwrap();
    assert_eq!(old, new);
    assert_eq!(
        dependencies_with_emoji_context(&plain, None).unwrap(),
        dependencies_with_groups(&plain, None).unwrap()
    );
}

#[test]
fn malformed_ids_source_drift_and_unknown_rows_cannot_supply_labels() {
    for bad in [
        Value::Null,
        json!(3),
        json!(""),
        json!("S1"),
        json!("UGROUP001"),
        json!("S group01"),
        json!("S".repeat(256)),
    ] {
        let mut l = leaf();
        l["usergroup_id"] = bad;
        assert!(dependencies_with_groups(&file(vec![l]), None).is_err());
    }
    let raw = json!({"id":"SGROUP001","handle":"design"});
    let mut group = UserGroup::from_observed(&raw).unwrap();
    assert!(known_group_label("SOTHER001", &group).is_err());
    group.handle = "drift".into();
    assert!(known_group_label("SGROUP001", &group).is_err());
    group = serde_json::from_value(raw.clone()).unwrap();
    assert!(known_group_label("SGROUP001", &group).is_err());
    for flag in [json!(true), json!("false"), json!([]), json!({}), json!(1)] {
        for key in ["isUnknown", "isNonExistent"] {
            let mut row = raw.clone();
            row[key] = flag.clone();
            assert!(
                known_group_label("SGROUP001", &UserGroup::from_observed(&row).unwrap()).is_err()
            );
        }
    }
    for bad in [json!(42), json!(true), json!([]), json!({})] {
        let mut row = raw.clone();
        row["handle"] = bad;
        assert!(UserGroup::from_observed(&row).is_err());
    }
    let mixedcase = UserGroup::from_observed(&json!({"id":"Sgroup001","handle":"design"})).unwrap();
    assert_eq!(
        known_group_label("Sgroup001", &mixedcase).unwrap(),
        "@design"
    );
}

#[test]
fn group_and_emoji_contexts_preserve_independent_admission_and_raw_input() {
    use super::super::EmojiText;
    let group =
        UserGroup::from_observed(&json!({"id":"SGROUP001","handle":"Cafe\u{301}"})).unwrap();
    let source = file(vec![
        json!({"type":"emoji","name":"wave","style":{"italic":true}}),
        json!({"type":"text","text":" Notes for "}),
        leaf(),
    ]);
    let before = source.clone();
    assert!(dependencies_with_groups(&source, None).is_err());
    let emoji = Some(EmojiText::EnglishCanonicalNames);
    let d = dependencies_with_groups(&source, emoji).unwrap();
    assert_eq!(d.groups, ["SGROUP001".into()].into());
    assert!(d.users.is_empty() && d.channels.is_empty());
    assert_eq!(
        label_with_group_resolver(
            Kind::Canvas,
            Status::Normal,
            &source,
            QuipTitle::Nullish,
            Some("Untitled"),
            emoji,
            |_| unreachable!(),
            |_| unreachable!(),
            |id| known_group_label(id, &group)
        )
        .unwrap(),
        "Notes for @Cafe\u{301}"
    );
    assert_eq!(source, before);
    assert!(
        label_with_group_resolver(
            Kind::Canvas,
            Status::Unavailable,
            &source,
            QuipTitle::Nullish,
            Some("Untitled"),
            emoji,
            |_| unreachable!(),
            |_| unreachable!(),
            |_| unreachable!()
        )
        .is_err()
    );
}
