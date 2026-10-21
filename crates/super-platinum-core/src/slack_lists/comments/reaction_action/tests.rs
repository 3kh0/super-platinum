use super::*;
use serde_json::json;

const CHANNEL: &str = "C01234567";
const TS: &str = "1700000001.000001";
const ROOT: &str = "1700000000.000001";

fn page(patch: Value) -> Page {
    let (doc, row, root) = super::super::tests::seed();
    let mut message = json!({"type":"message","channel":CHANNEL,"ts":TS,"thread_ts":ROOT,
        "user":"U_AUTHOR","text":"Retained rich reply","opaque":{"keep":[1,2]},
        "reactions":[{"name":"wave","count":1,"users":["U_OTHER"]}]});
    message
        .as_object_mut()
        .unwrap()
        .extend(patch.as_object().unwrap().clone());
    Page::parse(
        &Target::capture(&doc, &row).unwrap(),
        json!({"ok":true,"messages":[root,message],
        "has_more":true,"response_metadata":{"next_cursor":"opaque+/cursor"}}),
    )
    .unwrap()
}
fn intent(page: &Page, user: &str) -> ReactionIntent {
    ReactionIntent::capture(
        page.target(),
        page.root().unwrap(),
        &page.messages()[1],
        json!({"id":CHANNEL}),
        user,
        "wave",
    )
    .unwrap()
}
fn capture(
    page: &Page,
    root: &Message,
    message: &Message,
    channel: Value,
) -> Result<ReactionIntent, ListError> {
    ReactionIntent::capture(page.target(), root, message, channel, "U_SELF", "wave")
}

#[test]
fn capture_uses_current_membership_and_preserves_original_facts() {
    let page = page(json!({}));
    let original = page.raw().clone();
    let add = intent(&page, "U_SELF");
    let remove = intent(&page, "U_OTHER");
    assert!(add.record().adding());
    assert!(!remove.record().adding());
    assert_eq!(add.record().ts(), TS);
    assert_eq!(add.record().name(), "wave");
    assert_eq!(add.record().message_raw()["opaque"], json!({"keep":[1,2]}));
    assert!(add.record().current(
        page.target(),
        page.root().unwrap(),
        &page.messages()[1],
        &json!({"id":CHANNEL})
    ));
    let mut changed = page.messages()[1].clone();
    changed.reactions[0].users.push("U_SELF".into());
    changed.reactions[0].count += 1;
    assert!(!add.record().current(
        page.target(),
        page.root().unwrap(),
        &changed,
        &json!({"id":CHANNEL})
    ));
    assert_eq!(page.raw(), &original);
    assert_eq!(page.cursor(), Some("opaque+/cursor"));
}

#[test]
fn reaction_policy_does_not_inherit_posting_access_or_archive_denial() {
    let page = page(json!({}));
    let mut target = page.target().clone();
    target.record["archived"] = json!(true);
    target.document["access"] = json!("read");
    assert!(ReactionIntent::capture(&target,page.root().unwrap(),&page.messages()[1],
        json!({"id":CHANNEL,"is_archived":true,"is_read_only":true,"is_non_threadable":true,"is_locked":true}),
        "U_SELF","wave").is_ok());
    for channel in [
        json!({"id":CHANNEL,"is_frozen":true}),
        json!({"id":CHANNEL,"is_pending_ext_shared":true,"is_im":true}),
    ] {
        assert!(capture(&page, page.root().unwrap(), &page.messages()[1], channel).is_err());
    }
    assert!(
        capture(
            &page,
            page.root().unwrap(),
            &page.messages()[1],
            json!({"id":CHANNEL,"is_pending_ext_shared":true,"is_im":false})
        )
        .is_ok()
    );
    let mut root = page.root().unwrap().clone();
    root.extra.insert("is_locked".into(), json!(true));
    assert!(capture(&page, &root, &page.messages()[1], json!({"id":CHANNEL})).is_err());
}

#[test]
fn root_reply_channel_and_unconditional_availability_are_fenced() {
    let page = page(json!({}));
    let root = page.root().unwrap();
    assert!(capture(&page, root, root, json!({"id":CHANNEL})).is_err());
    assert!(capture(&page, root, &page.messages()[1], json!({"id":"C_OTHER"})).is_err());
    for patch in [
        json!({"channel":"C_OTHER"}),
        json!({"ts":"bad"}),
        json!({"thread_ts":"1700000002.000001"}),
        json!({"isUnknown":true}),
        json!({"is_ephemeral":true}),
        json!({"isNonExistent":true}),
        json!({"no_display":true}),
        json!({"subtype":"file_comment"}),
        json!({"subtype":"tombstone"}),
        json!({"subtype":"list_edit"}),
    ] {
        let mut raw = serde_json::to_value(&page.messages()[1]).unwrap();
        raw.as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        let message = serde_json::from_value(raw).unwrap();
        assert!(
            capture(&page, root, &message, json!({"id":CHANNEL})).is_err(),
            "{patch}"
        );
    }
    for patch in [
        json!({"no_display":true}),
        json!({"isNonExistent":true}),
        json!({"slack_list":{"list_id":"F_OTHER","list_record_id":"Rec1"}}),
    ] {
        let mut raw = serde_json::to_value(root).unwrap();
        raw.as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        assert!(
            capture(
                &page,
                &serde_json::from_value(raw).unwrap(),
                &page.messages()[1],
                json!({"id":CHANNEL})
            )
            .is_err()
        );
    }
}

#[test]
fn existing_pill_only_and_ambiguous_membership_refuse_atomically() {
    for reactions in [
        json!([]),
        json!([{"name":"other","count":1,"users":["U_OTHER"]}]),
        json!([{"name":"wave","count":2,"users":["U_OTHER","U_OTHER"]}]),
        json!([{"name":"wave","count":0,"users":["U_OTHER"]}]),
        json!([{"name":"wave","count":1,"users":["U_OTHER"]},{"name":"wave","count":1,"users":["U_SELF"]}]),
        json!([{"name":"wave","count":1,"users":["U_OTHER"]},{"name":"wave::skin-tone-2","count":1,"users":["U_SELF"]}]),
    ] {
        let page = page(json!({"reactions":reactions}));
        assert!(
            capture(
                &page,
                page.root().unwrap(),
                &page.messages()[1],
                json!({"id":CHANNEL})
            )
            .is_err()
        );
    }
}

#[test]
fn tone_variant_selection_refuses_without_blocking_an_unrelated_plain_pill() {
    let p = page(json!({"reactions":[
        {"name":"wave","count":1,"users":["U_OTHER"]},
        {"name":"thumbsup::skin-tone-2","count":1,"users":["U_SELF"]}
    ]}));
    assert!(intent(&p, "U_SELF").record().adding());
    assert!(
        ReactionIntent::capture(
            p.target(),
            p.root().unwrap(),
            &p.messages()[1],
            json!({"id":CHANNEL}),
            "U_SELF",
            "thumbsup::skin-tone-2"
        )
        .is_err()
    );
}

#[test]
fn own_limit_blocks_add_but_existing_name_does_not_spend_distinct_slot() {
    let mut reactions: Vec<Value> = (0..50)
        .map(|i| json!({"name":format!("emoji_{i}"),"count":1,"users":["U_OTHER"]}))
        .collect();
    reactions[0]["name"] = json!("wave");
    let p = page(json!({"reactions":reactions}));
    assert!(intent(&p, "U_SELF").record().adding());
    for r in reactions.iter_mut().skip(1).take(23) {
        r["users"] = json!(["U_SELF"]);
    }
    let p = page(json!({"reactions":reactions}));
    assert!(
        capture(
            &p,
            p.root().unwrap(),
            &p.messages()[1],
            json!({"id":CHANNEL})
        )
        .is_err()
    );
    reactions[0]["users"] = json!(["U_SELF"]);
    assert!(
        !intent(&page(json!({"reactions":reactions})), "U_SELF")
            .record()
            .adding()
    );
}

#[test]
fn strict_ack_is_distinct_from_desired_state_and_known_rejection() {
    let page = page(json!({}));
    let add = intent(&page, "U_SELF");
    assert_eq!(
        add.record().classify(&json!({"ok":true})),
        ReactionResponse::Confirmed
    );
    assert_eq!(
        add.record()
            .classify(&json!({"ok":false,"error":"already_reacted"})),
        ReactionResponse::DesiredState
    );
    assert_eq!(
        add.record()
            .classify(&json!({"ok":false,"error":"no_reaction"})),
        ReactionResponse::Uncertain
    );
    assert!(
        intent(&page, "U_SELF")
            .accept(json!({"ok":false,"error":"already_reacted"}))
            .is_err()
    );
    let remove = intent(&page, "U_OTHER");
    assert_eq!(
        remove
            .record()
            .classify(&json!({"ok":false,"error":"no_reaction"})),
        ReactionResponse::DesiredState
    );
    assert_eq!(
        remove
            .record()
            .classify(&json!({"ok":false,"error":"already_reacted"})),
        ReactionResponse::Uncertain
    );
    for error in [
        "invalid_name",
        "too_many_emoji",
        "too_many_reactions",
        "thread_locked",
        "ratelimited",
    ] {
        assert_eq!(
            add.record().classify(&json!({"ok":false,"error":error})),
            ReactionResponse::Rejected
        );
    }
    let accepted = add
        .accept(json!({"ok":true,"channel":CHANNEL,"timestamp":TS,"name":"wave"}))
        .unwrap();
    assert_eq!(accepted.record().ts(), TS);
    assert_eq!(accepted.raw()["ok"], true);
}

#[test]
fn malformed_server_errors_and_contradictory_response_identity_stay_uncertain() {
    let page = page(json!({}));
    let record = intent(&page, "U_SELF").record().clone();
    for raw in [
        json!(null),
        json!({}),
        json!({"ok":true,"error":"fatal_error"}),
        json!({"ok":false,"error":"fatal_error"}),
        json!({"ok":false,"error":"internal_error"}),
        json!({"ok":false,"error":"new_server_error"}),
        json!({"ok":true,"channel":"C_OTHER"}),
        json!({"ok":true,"timestamp":"1700000009.000001"}),
        json!({"ok":true,"name":"other"}),
        json!({"ok":false,"error":"invalid_name","channel":"C_OTHER"}),
    ] {
        assert_eq!(record.classify(&raw), ReactionResponse::Uncertain, "{raw}");
    }
}

#[test]
fn readback_requires_positive_original_membership_not_typed_defaults_or_overlay() {
    let original = page(json!({}));
    let add = intent(&original, "U_SELF").record().clone();
    let remove = intent(&original, "U_OTHER").record().clone();
    assert!(!add.observed(&original).unwrap());
    assert!(!remove.observed(&original).unwrap());
    let mut raw = original.raw().clone();
    raw["messages"][1]
        .as_object_mut()
        .unwrap()
        .remove("reactions");
    let mut missing = Page::parse(original.target(), raw).unwrap();
    assert!(!remove.observed(&missing).unwrap());
    missing.apply_reaction(CHANNEL, TS, "U_SELF", "wave", true);
    assert!(!add.observed(&missing).unwrap());
    assert!(!remove.observed(&page(json!({"reactions":[]}))).unwrap());
    assert!(
        !remove
            .observed(&page(json!({"reactions":[
                {"name":"other","count":1,"users":["U_SELF"]}
            ]})))
            .unwrap()
    );
    assert!(
        add.observed(&page(
            json!({"reactions":[{"name":"wave","count":9,"users":["U_SELF"]}]})
        ))
        .unwrap()
    );
    assert!(
        !remove
            .observed(&page(
                json!({"reactions":[{"name":"wave","count":9,"users":["U_SELF"]}]})
            ))
            .unwrap()
    );
    assert!(
        remove
            .observed(&page(
                json!({"reactions":[{"name":"wave","count":1,"users":["U_SELF"]}]})
            ))
            .unwrap()
    );
}

#[test]
fn readback_missing_deleted_foreign_and_duplicate_membership_cannot_confirm() {
    let p = page(json!({}));
    let record = intent(&p, "U_OTHER").record().clone();
    let mut raw = p.raw().clone();
    raw["messages"].as_array_mut().unwrap().pop();
    assert!(
        !record
            .observed(&Page::parse(p.target(), raw).unwrap())
            .unwrap()
    );
    let mut raw = p.raw().clone();
    raw["messages"][1]["reactions"] = json!([]);
    raw["deleted"] = json!([TS]);
    assert!(
        !record
            .observed(&Page::parse(p.target(), raw).unwrap())
            .unwrap()
    );
    let mut other = page(json!({"reactions":[]}));
    other.target.record["id"] = json!("Rec2");
    assert!(record.observed(&other).is_err());
    for reactions in [
        json!([{"name":"wave","count":2,"users":["U_SELF","U_SELF"]}]),
        json!([{"name":"wave","count":0,"users":["U_SELF"]}]),
        json!([{"name":"wave","count":0,"users":[]},{"name":"wave","count":0,"users":[]}]),
    ] {
        assert!(
            record
                .observed(&page(json!({"reactions":reactions})))
                .is_err()
        );
    }
}

fn full(reactions: Value) -> Value {
    json!({"ok":true,"type":"message","channel":CHANNEL,
        "message":{"ts":TS,"thread_ts":ROOT,"reactions":reactions}})
}

#[test]
fn original_full_read_proves_absent_last_reaction_but_page_cannot() {
    let p = page(json!({}));
    let remove = intent(&p, "U_OTHER").record().clone();
    let add = intent(&p, "U_SELF").record().clone();
    assert!(remove.observed_full(&full(json!([]))).unwrap());
    assert!(!add.observed_full(&full(json!([]))).unwrap());
    assert!(!remove.observed(&page(json!({"reactions":[]}))).unwrap());
    assert!(
        remove
            .observed_full(&full(json!([
                {"name":"other","count":1,"users":["U_SELF"]}
            ])))
            .unwrap()
    );
    let mut no_thread = full(json!([]));
    no_thread["message"]
        .as_object_mut()
        .unwrap()
        .remove("thread_ts");
    assert!(remove.observed_full(&no_thread).unwrap());
}

#[test]
fn full_read_requires_exact_original_message_envelope_and_explicit_inventory() {
    let p = page(json!({}));
    let record = intent(&p, "U_OTHER").record().clone();
    for (pointer, value) in [
        ("/ok", json!(false)),
        ("/type", json!("file")),
        ("/channel", json!("C_OTHER")),
        ("/message/ts", json!(ROOT)),
        ("/message/thread_ts", json!("1700000099.000001")),
        ("/message/thread_ts", Value::Null),
        ("/message/reactions", Value::Null),
    ] {
        let mut raw = full(json!([]));
        *raw.pointer_mut(pointer).unwrap() = value;
        assert!(record.observed_full(&raw).is_err(), "{pointer}");
    }
    let mut raw = full(json!([]));
    raw["message"]["channel"] = json!("C_OTHER");
    assert!(record.observed_full(&raw).is_err());
    let mut raw = full(json!([]));
    raw["error"] = json!("internal_error");
    assert!(record.observed_full(&raw).is_err());
    let mut raw = full(json!([]));
    raw["message"].as_object_mut().unwrap().remove("reactions");
    assert!(record.observed_full(&raw).is_err());
    let mut raw = full(json!([]));
    raw["message"]["no_display"] = json!(true);
    assert!(!record.observed_full(&raw).unwrap());
}

#[test]
fn full_self_membership_survives_truncated_other_users_without_transferring_to_page() {
    let p = page(json!({}));
    let remove = intent(&p, "U_OTHER").record().clone();
    let add = intent(&p, "U_SELF").record().clone();
    let truncated = full(json!([{"name":"wave","count":9,"users":["U_SELF"]}]));
    assert!(remove.observed_full(&truncated).unwrap());
    assert!(
        !remove
            .observed(&page(json!({"reactions":[
                {"name":"wave","count":9,"users":["U_SELF"]}
            ]})))
            .unwrap()
    );
    assert!(add.observed_full(&truncated).unwrap());
    assert!(
        remove
            .observed_full(&full(json!([{"name":"wave","count":1,"users":["U_SELF"]}])))
            .unwrap()
    );
    for reactions in [
        json!([{"name":"wave","count":2,"users":["U_SELF","U_SELF"]}]),
        json!([{"name":"wave","count":0,"users":["U_SELF"]}]),
        json!([{"name":"wave","count":0,"users":[]},{"name":"wave","count":0,"users":[]}]),
        json!([{"name":"other","count":1}]),
        json!([{"name":"other","count":1,"users":[null]}]),
    ] {
        assert!(remove.observed_full(&full(reactions)).is_err());
    }
}

#[test]
fn full_reactions_replace_only_current_reply_reactions_not_body_or_receipt() {
    let mut p = page(json!({}));
    let record = intent(&p, "U_OTHER").record().clone();
    let raw = p.raw().clone();
    let target = p.target().clone();
    let before = serde_json::to_value(p.messages()).unwrap();
    let mut response =
        full(json!([{"name":"other","count":2,"users":["U_SELF","U_EXTRA"],"opaque":7}]));
    response["message"]["text"] = json!("Do not replace the body");
    let reactions = record.full_reactions(&response).unwrap().unwrap();
    assert!(p.replace_reply_reactions(CHANNEL, TS, &reactions));
    assert!(!p.replace_reply_reactions(CHANNEL, TS, &reactions));
    let mut after = serde_json::to_value(p.messages()).unwrap();
    assert_eq!(after[1]["reactions"][0]["opaque"], 7);
    after[1]["reactions"] = before[1]["reactions"].clone();
    assert_eq!(after, before);
    assert_eq!(p.raw(), &raw);
    assert_eq!(p.target(), &target);
    assert_eq!(p.cursor(), Some("opaque+/cursor"));
    assert!(p.has_more());
    assert!(!p.replace_reply_reactions("C_OTHER", TS, &[]));
    assert!(!p.replace_reply_reactions(CHANNEL, ROOT, &[]));
    assert!(!p.replace_reply_reactions(CHANNEL, "1700000099.000001", &[]));
    let hidden = page(json!({"no_display":true}));
    assert!(
        !hidden
            .clone()
            .replace_reply_reactions(CHANNEL, TS, &reactions)
    );
    let mut unavailable = response.clone();
    unavailable["message"]["isNonExistent"] = json!(true);
    assert!(record.full_reactions(&unavailable).unwrap().is_none());
    response["channel"] = json!("C_OTHER");
    assert!(record.full_reactions(&response).is_err());
}
