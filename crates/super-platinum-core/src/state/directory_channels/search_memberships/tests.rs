use super::*;
use serde_json::json;
fn owner() -> WorkspaceSession {
    serde_json::from_value(json!({"team_id":"TONE","user_id":"USELF","name":"Fixture","token":"synthetic","url":"https://fixture.slack.com"})).unwrap()
}
fn facts() -> Facts {
    Facts {
        self_id: "USELF".into(),
        current_team: "TONE".into(),
        omit_team: "TONE".into(),
        retain_public_membership_on_archive: false,
        members: BTreeMap::new(),
    }
}
fn raw(store: &mut Store, native: &mut BTreeMap<String, Channel>, rows: Vec<Value>) {
    store
        .enqueue("A", &owner(), 1, Origin::Directory, rows)
        .unwrap();
    drain_raw(store, native);
}
fn drain_raw(store: &mut Store, native: &mut BTreeMap<String, Channel>) {
    let f = facts();
    let p = store
        .prepare("A", &owner(), 1, "Q", native, f.clone())
        .unwrap();
    let r = RendererReply {
        generation: "Q".into(),
        fields: p.fields().to_vec(),
        values: p
            .fields()
            .iter()
            .map(|s| {
                assert!(s.is_ascii());
                s.to_ascii_lowercase()
            })
            .collect(),
    };
    (*store, *native) = store
        .finish(p, &f, r, "A", &owner(), 1, "Q", native)
        .unwrap()
        .into_parts();
}
fn setup() -> (Store, BTreeMap<String, Channel>) {
    let mut s = Store::default();
    let b = s.begin_full_boot("A", &owner(), 1).unwrap();
    assert!(s.finish_full_boot(b, "A", &owner(), 1));
    s.begin("A", &owner(), &BTreeMap::new()).unwrap();
    let mut n = BTreeMap::new();
    raw(
        &mut s,
        &mut n,
        vec![
            json!({"id":"CKNOWN001","name":"known","is_channel":true,"is_member":false,"latest":{"ts":"123"},"last_read":"122","unread_count":3}),
        ],
    );
    (s, n)
}
fn drain_joined(s: &mut Store, n: &mut BTreeMap<String, Channel>) {
    let o = s.observation("A", &owner(), 1, "Q", n).unwrap();
    (*s, *n) = s
        .drain_source_action(&o, "A", &owner(), 1, "Q", n)
        .unwrap()
        .unwrap()
        .into_parts();
}
#[test]
fn share_search_membership_returned_exact_truth_and_empty_no_override() {
    let rows = vec![
        json!({"id":"CPUBLIC01","is_member":true,"is_archived":1}),
        json!({"id":"GPRIVATE1","is_private":"yes","is_archived":{}}),
        json!({"id":"GMPIM0001","is_private":true,"is_mpim":[],"is_archived":true}),
        json!({"id":"CLEFT0001","is_member":true}),
    ];
    let original = rows.clone();
    assert_eq!(returned(&rows, &[]).unwrap(), original);
    let ids = ["CPUBLIC01", "GPRIVATE1", "GMPIM0001"].map(str::to_owned);
    let out = returned(&rows, &ids).unwrap();
    assert_eq!(
        out.iter()
            .map(|r| r["is_member"].clone())
            .collect::<Vec<_>>(),
        vec![json!(false), json!(true), json!(false), json!(false)]
    );
    assert_eq!(rows, original);
}
#[test]
fn share_search_membership_final_pages_stay_raw_and_overflow_companion_is_joined() {
    let mut pages = crate::workflow_share::search::Pages::default();
    let rows = (0..25)
        .map(|i| json!({"id":format!("CSEARCH{i:02}"),"is_member":false}))
        .collect::<Vec<_>>();
    let wire = json!({"results":rows,"member_channels":["CSEARCH00","CSEARCH24"]});
    pages.accept(wire.clone(), None).unwrap();
    let transformed = returned(
        pages.rows().unwrap(),
        &pages.outcomes().unwrap().membership_ids,
    )
    .unwrap();
    assert_eq!(transformed.len(), 24);
    assert_eq!(transformed[0]["is_member"], true);
    assert_eq!(pages.pages()[0].raw(), &wire);
    assert_eq!(pages.rows().unwrap()[0]["is_member"], false);
    let (s, n) = setup();
    let o = s.observation("A", &owner(), 1, "Q", &n).unwrap();
    let ids = transformed
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    let (s, _) = s
        .queue_search_memberships(
            &o,
            "A",
            &owner(),
            1,
            "Q",
            &n,
            &ids,
            &pages.outcomes().unwrap().membership_ids,
        )
        .unwrap()
        .into_parts();
    assert_eq!(
        s.saved.as_ref().unwrap().pending[0].action,
        Some(actions::Action::Joined(vec!["CSEARCH24".into()]))
    );
}
#[test]
fn share_search_membership_known_notreturned_and_unknown_never_grant_destination() {
    let (mut s, mut n) = setup();
    let before = n["CKNOWN001"].clone();
    let o = s.observation("A", &owner(), 1, "Q", &n).unwrap();
    (s, n) = s
        .queue_search_memberships(
            &o,
            "A",
            &owner(),
            1,
            "Q",
            &n,
            &[],
            &["CKNOWN001".into(), "CUNKNOWN1".into()],
        )
        .unwrap()
        .into_parts();
    assert_eq!(
        serde_json::to_value(&n["CKNOWN001"]).unwrap(),
        serde_json::to_value(&before).unwrap()
    );
    drain_joined(&mut s, &mut n);
    let mut expected = serde_json::to_value(before).unwrap();
    expected["is_member"] = json!(true);
    assert_eq!(serde_json::to_value(&n["CKNOWN001"]).unwrap(), expected);
    let o = s.observation("A", &owner(), 1, "Q", &n).unwrap();
    let unknown = o.rows().iter().find(|r| r["id"] == "CUNKNOWN1").unwrap();
    assert_eq!(
        unknown,
        &json!({"id":"CUNKNOWN1","isUnknown":true,"is_member":true})
    );
    assert!(!crate::workflow_share::joined_channel_eligible(
        unknown,
        crate::block_actions::posting_policy::Flags {
            read_only: false,
            thread_only: false
        },
        "TONE",
        "TONE"
    ));
    assert_eq!(
        s.saved
            .as_ref()
            .unwrap()
            .records
            .iter()
            .find(|r| r.source["id"] == "CUNKNOWN1")
            .unwrap()
            .wire,
        Value::Null
    );
}
#[test]
fn share_search_membership_original_observation_retires_for_each_owner_or_source_change() {
    for mode in [
        "account",
        "epoch",
        "generation",
        "credential",
        "typed",
        "source",
    ] {
        let (mut s, mut n) = setup();
        let o = s.observation("A", &owner(), 1, "Q", &n).unwrap();
        let mut w = owner();
        match mode {
            "credential" => w.token = "other".into(),
            "typed" => n.get_mut("CKNOWN001").unwrap().name = Some("changed".into()),
            "source" => {
                s.enqueue(
                    "A",
                    &w,
                    1,
                    Origin::Directory,
                    vec![json!({"id":"COTHER001","name":"new"})],
                )
                .unwrap();
            }
            _ => {}
        }
        let before = s.cached();
        assert!(
            s.queue_search_memberships(
                &o,
                if mode == "account" { "B" } else { "A" },
                &w,
                if mode == "epoch" { 2 } else { 1 },
                if mode == "generation" { "Q2" } else { "Q" },
                &n,
                &[],
                &["CKNOWN001".into()]
            )
            .is_err(),
            "{mode}"
        );
        assert_eq!(s.cached(), before);
    }
}
#[test]
fn share_search_membership_order_is_predecessor_then_joined_then_returned() {
    let (mut s, mut n) = setup();
    s.enqueue(
        "A",
        &owner(),
        1,
        Origin::Directory,
        vec![json!({"id":"CKNOWN001","name":"earlier","is_member":false})],
    )
    .unwrap();
    let o = s.observation("A", &owner(), 1, "Q", &n).unwrap();
    let incoming = returned(
        &[json!({"id":"CRETURN01","name":"returned","is_channel":true,"is_member":false})],
        &["CKNOWN001".into(), "CRETURN01".into()],
    )
    .unwrap();
    let accepted = s
        .capture_receipt_ingress(
            &o,
            "A",
            &owner(),
            1,
            "Q",
            &n,
            Origin::Directory,
            incoming,
            facts(),
        )
        .unwrap();
    let (mut next, _) = s
        .queue_search_memberships(
            &o,
            "A",
            &owner(),
            1,
            "Q",
            &n,
            &["CRETURN01".into()],
            &["CKNOWN001".into(), "CRETURN01".into()],
        )
        .unwrap()
        .into_parts();
    next.enqueue_ingress(accepted, "A", &owner(), 1).unwrap();
    s = next;
    assert_eq!(s.saved.as_ref().unwrap().pending.len(), 3);
    drain_raw(&mut s, &mut n);
    assert_eq!(n["CKNOWN001"].name.as_deref(), Some("earlier"));
    drain_joined(&mut s, &mut n);
    assert_eq!(n["CKNOWN001"].extra["is_member"], true);
    assert!(!n.contains_key("CRETURN01"));
    drain_raw(&mut s, &mut n);
    assert_eq!(n["CRETURN01"].extra["is_member"], true);
}
