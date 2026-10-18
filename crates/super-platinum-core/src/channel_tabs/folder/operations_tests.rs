use crate::channel_tabs::{Action, Fault, Intent, Inventory, Observation};
use serde_json::{Value, json};
const C: &str = "C00000001";
const A: &str = "Ct00000001";
const U: &str = "Ct00000002";
fn raw() -> Value {
    json!({"id":C,"properties":{"opaque":[3,1],"tabz":[{"id":A,"type":"folder","label":"Resources","data":{"folder_bookmark_id":"Bk1","opaque":true}},{"id":U,"type":"future","opaque":[7,2]}]}})
}
fn inv(v: &Value) -> Inventory {
    Inventory::from_channel(v).unwrap()
}
fn ack() -> Value {
    json!({"ok":true,"bookmark":{"id":"Bk9","type":"local_folder","title":"Launch","channel_id":C,"opaque":[4,2]}})
}
fn created() -> Value {
    let mut v = raw();
    v["properties"]["tabz"].as_array_mut().unwrap().push(json!({"id":"Ct90000001","type":"folder","label":"Launch","data":{"folder_bookmark_id":"Bk9"},"server_extra":true}));
    v
}
fn add() -> Intent {
    Intent::capture(
        inv(&raw()),
        Action::AddFolder {
            label: "Launch".into(),
        },
    )
    .unwrap()
}
fn request(i: &Intent) -> crate::slack::PreparedRequest {
    i.request(
        &crate::slack::SlackClient::default(),
        &crate::config::WorkspaceSession {
            team_id: "T00000001".into(),
            enterprise_id: None,
            user_id: "U00000001".into(),
            name: "Fixture".into(),
            url: "https://fixture.slack.com".into(),
            token: "synthetic".into(),
        },
    )
}
#[test]
fn creation_uses_actual_folder_builder_no_tab_add_or_fabricated_identity() {
    let mut i = add();
    let r = request(&i);
    assert!(
        r.url
            .split('?')
            .next()
            .unwrap()
            .ends_with("/api/bookmarks.add")
    );
    assert!(!r.retry_safe());
    let crate::slack::client::RequestBody::Form(fields) = r.body else {
        panic!("form")
    };
    assert_eq!(
        fields,
        vec![
            ("token".into(), "synthetic".into()),
            ("channel".into(), C.into()),
            ("title".into(), "Launch".into()),
            ("type".into(), "local_folder".into())
        ]
    );
    assert_eq!(
        i.observe(&inv(&created())),
        Observation::Changed,
        "same label is not returned identity"
    );
    i.acknowledge(&ack()).unwrap();
    assert_eq!(i.created_folder(), Some("Bk9"));
    assert_eq!(i.observe(&inv(&created())), Observation::Satisfied);
    assert_eq!(
        i.observe(&inv(&raw())),
        Observation::Unchanged,
        "ACK alone cannot synthesize membership"
    );
}
#[test]
fn creation_preserves_all_siblings_order_and_refuses_ambiguous_or_rebound_receipts() {
    let mut i = add();
    i.acknowledge(&ack()).unwrap();
    for mode in 0..6 {
        let mut v = created();
        match mode {
            0 => v["properties"]["tabz"][1]["opaque"] = json!([2, 7]),
            1 => v["properties"]["tabz"].as_array_mut().unwrap().swap(0, 1),
            2 => v["properties"]["tabz"][2]["data"]["folder_bookmark_id"] = "Bk8".into(),
            3 => v["properties"]["opaque"] = json!([1, 3]),
            4 => {
                let mut duplicate = v["properties"]["tabz"][2].clone();
                duplicate["id"] = "Ct90000002".into();
                v["properties"]["tabz"]
                    .as_array_mut()
                    .unwrap()
                    .push(duplicate);
            }
            _ => {
                v["properties"]["tabz"]
                    .as_array_mut()
                    .unwrap()
                    .insert(0, json!({"id":"Ct80000001","type":"future"}));
            }
        }
        assert_eq!(i.observe(&inv(&v)), Observation::Changed);
    }
    for mode in 0..5 {
        let mut a = ack();
        match mode {
            0 => a["bookmark"]["type"] = "link".into(),
            1 => a["bookmark"]["channel_id"] = "C00000002".into(),
            2 => a["bookmark"]["id"] = "Bk1".into(),
            3 => a["bookmark"]["title"] = "Different".into(),
            _ => a["error"] = "rejected".into(),
        };
        assert!(add().acknowledge(&a).is_err());
    }
    let mut changed = ack();
    changed["bookmark"]["id"] = "Bk8".into();
    assert_eq!(i.acknowledge(&changed), Err(Fault::Changed));
}
#[test]
fn folder_rename_and_move_use_tab_wire_preserving_unknowns_but_remove_stays_separate() {
    let b = inv(&raw());
    let rename = Intent::capture(
        b.clone(),
        Action::Rename {
            id: A.into(),
            label: "Plans".into(),
        },
    )
    .unwrap();
    assert!(request(&rename).url.contains("conversations.updateTab?"));
    let mut renamed = raw();
    renamed["properties"]["tabz"][0]["label"] = "Plans".into();
    assert_eq!(rename.observe(&inv(&renamed)), Observation::Satisfied);
    assert_eq!(
        renamed["properties"]["tabz"][0]["data"],
        raw()["properties"]["tabz"][0]["data"]
    );
    let moved = Intent::capture(
        b.clone(),
        Action::Move {
            id: A.into(),
            after: Some(U.into()),
        },
    )
    .unwrap();
    assert!(request(&moved).url.contains("conversations.moveTab?"));
    let mut v = raw();
    v["properties"]["tabz"].as_array_mut().unwrap().swap(0, 1);
    assert_eq!(moved.observe(&inv(&v)), Observation::Satisfied);
    assert!(
        Intent::capture(b.clone(), Action::Remove { id: A.into() }).is_err(),
        "folder deletion cannot inherit Canvas keep_file_share"
    );
    for label in ["", " ", " padded "] {
        assert!(
            Intent::capture(
                b.clone(),
                Action::Rename {
                    id: A.into(),
                    label: label.into()
                }
            )
            .is_err()
        );
        assert!(
            Intent::capture(
                b.clone(),
                Action::AddFolder {
                    label: label.into()
                }
            )
            .is_err()
        );
    }
    assert!(
        Intent::capture(
            b.clone(),
            Action::Rename {
                id: U.into(),
                label: "Future".into()
            }
        )
        .is_err()
    );
    let mut changed = raw();
    changed["properties"]["tabz"][0]["data"]["folder_bookmark_id"] = "Bk8".into();
    assert_eq!(rename.revalidate(&inv(&changed)), Err(Fault::Changed));
}
