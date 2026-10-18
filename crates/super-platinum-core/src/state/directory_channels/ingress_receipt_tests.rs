use super::*;
use serde_json::json;
fn owner() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "TONE".into(),
        user_id: "USELF".into(),
        enterprise_id: None,
        name: "Synthetic".into(),
        token: "synthetic".into(),
        url: "https://fixture.slack.com".into(),
    }
}
fn facts(name: &str) -> Facts {
    Facts {
        self_id: "USELF".into(),
        current_team: "TONE".into(),
        omit_team: "TONE".into(),
        retain_public_membership_on_archive: false,
        members: BTreeMap::from([("UPEER123".into(), json!({"id":"UPEER123","name":name}))]),
    }
}
fn store() -> Store {
    let mut s = Store::default();
    let boot = s.begin_full_boot("A", &owner(), 1).unwrap();
    assert!(s.finish_full_boot(boot, "A", &owner(), 1));
    s.begin("A", &owner(), &BTreeMap::new()).unwrap();
    s
}
fn enqueue(s: &mut Store, f: Facts) {
    let native = BTreeMap::new();
    let observed = s.observation("A", &owner(), 1, "Q", &native).unwrap();
    let accepted = s
        .capture_receipt_ingress(
            &observed,
            "A",
            &owner(),
            1,
            "Q",
            &native,
            Origin::Direct,
            vec![json!({"id":"DPEER123","user":"UPEER123","is_im":true})],
            f,
        )
        .unwrap();
    s.enqueue_ingress(accepted, "A", &owner(), 1).unwrap();
}
#[test]
fn later_member_name_cannot_rewrite_accepted_channel_facts() {
    let mut s = store();
    let native = BTreeMap::new();
    let accepted = facts("First");
    enqueue(&mut s, accepted.clone());
    let observed = s.observation("A", &owner(), 1, "Q", &native).unwrap();
    assert_eq!(observed.receipt_facts(), Some(&accepted));
    assert!(
        s.prepare("A", &owner(), 1, "Q", &native, facts("Later"))
            .is_err()
    );
    let pending = s
        .prepare("A", &owner(), 1, "Q", &native, accepted.clone())
        .unwrap();
    assert!(pending.fields().iter().all(|s| s.is_ascii()));
    let reply = RendererReply {
        generation: pending.generation().into(),
        fields: pending.fields().to_vec(),
        values: pending
            .fields()
            .iter()
            .map(|s| s.to_ascii_lowercase())
            .collect(),
    };
    let (done, native) = s
        .finish(pending, &accepted, reply, "A", &owner(), 1, "Q", &native)
        .unwrap()
        .into_parts();
    assert_eq!(native["DPEER123"].name.as_deref(), Some("First"));
    assert_eq!(
        done.ready("A", &owner(), 1, "Q", &native).unwrap().rows()[0]["name"],
        "First"
    );
}
#[test]
fn receipt_facts_roundtrip_and_legacy_missing_remains_ordinary() {
    let mut s = store();
    let f = facts("First");
    enqueue(&mut s, f.clone());
    let raw = s.cached().unwrap();
    assert!(!raw.to_string().contains("synthetic"));
    let restored = Store::restore(Some(&raw), &owner());
    assert_eq!(
        restored
            .observation("A", &owner(), 1, "Q", &BTreeMap::new())
            .unwrap()
            .receipt_facts(),
        Some(&f)
    );
    let mut legacy = raw.clone();
    legacy["pending"][0]
        .as_object_mut()
        .unwrap()
        .remove("receipt_facts");
    let restored = Store::restore(Some(&legacy), &owner());
    assert!(
        restored
            .observation("A", &owner(), 1, "Q", &BTreeMap::new())
            .unwrap()
            .receipt_facts()
            .is_none()
    );
    assert!(
        restored
            .prepare("A", &owner(), 1, "Q", &BTreeMap::new(), facts("Later"))
            .is_ok()
    );
    let mut foreign = raw;
    foreign["pending"][0]["receipt_facts"]["current_team"] = "TOTHER".into();
    assert!(Store::restore(Some(&foreign), &owner()).cached().is_none());
}
#[test]
fn captured_phase_and_original_response_still_gate_receipt_facts() {
    let mut s = store();
    let native = BTreeMap::new();
    let observed = s.observation("A", &owner(), 1, "Q", &native).unwrap();
    let mut wrong = facts("First");
    wrong.self_id = "UOTHER".into();
    assert!(
        s.capture_receipt_ingress(
            &observed,
            "A",
            &owner(),
            1,
            "Q",
            &native,
            Origin::Direct,
            vec![json!({"id":"CONE"})],
            wrong
        )
        .is_err()
    );
    s.invalidate("CONE");
    assert!(
        s.capture_receipt_ingress(
            &observed,
            "A",
            &owner(),
            1,
            "Q",
            &native,
            Origin::Direct,
            vec![json!({"id":"CONE"})],
            facts("First")
        )
        .is_err()
    );
    assert!(!s.has_pending());
}
