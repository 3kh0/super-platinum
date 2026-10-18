use super::*;
use crate::block_actions::workflow_management::member_hydration::{Flags, RendererReply};
use serde_json::json;
fn owner() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "TTEST1234".into(),
        user_id: "USELF1234".into(),
        enterprise_id: None,
        name: "Synthetic".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
    }
}
fn asset() -> PlaceholderAsset {
    PlaceholderAsset::observed("https://a.slack-edge.com/bv1-13/").unwrap()
}
fn row(id: &str, name: &str) -> Value {
    json!({"id":id,"name":name,"profile":{"display_name":name}})
}
fn seeded() -> (Store, HashMap<String, User>) {
    let mut store = Store::default();
    let mut native = HashMap::new();
    store.begin("A", &owner(), &BTreeSet::new()).unwrap();
    store
        .enqueue(
            "A",
            &owner(),
            Origin::Directory,
            vec![row("UKNOWN123", "Known")],
            json!({"UKNOWN123":true}),
        )
        .unwrap();
    let flags = Flags::decode(&json!({"ok":true,"assignments":{}})).unwrap();
    let pending = store
        .prepare("A", &owner(), 7, "Q", flags.clone(), &native)
        .unwrap();
    // Only ASCII synthetic names are present in this renderer adapter.
    assert!(pending.fields().iter().all(|s| s.is_ascii()));
    let reply = RendererReply {
        generation: pending.generation().into(),
        operation: "ordinary-toLowerCase".into(),
        fields: pending.fields().to_vec(),
        lowered: pending
            .fields()
            .iter()
            .map(|s| s.to_ascii_lowercase())
            .collect(),
    };
    let commit = store
        .adopt(pending, reply, &flags, "A", &owner(), 7, "Q", &native)
        .unwrap();
    native = commit.into_parts().1;
    (store, native)
}
fn capture(
    store: &Store,
    native: &HashMap<String, User>,
    ids: &[&str],
) -> Result<ReceiptMembers, Error> {
    store.receipt_members(
        "A",
        &owner(),
        7,
        "Q",
        &ids.iter().map(|s| (*s).to_owned()).collect(),
        native,
        &asset(),
    )
}
#[test]
fn exact_known_and_source_absent_are_synchronous_without_store_mutation() {
    let (store, native) = seeded();
    let before = store.saved.clone();
    let revision = store.revision;
    let receipt = capture(&store, &native, &["UKNOWN123", "UABSENT12"]).unwrap();
    assert_eq!(receipt.rows()["UKNOWN123"]["name"], "Known");
    assert_eq!(receipt.rows()["UABSENT12"]["name"], "UABSENT12");
    assert_eq!(receipt.rows()["UABSENT12"]["isUnknown"], true);
    assert!(receipt.rows()["UABSENT12"].get("isNonExistent").is_none());
    assert_eq!(receipt.absent_ids(), &BTreeSet::from(["UABSENT12".into()]));
    assert!(receipt.current(&store, "A", &owner(), 7, "Q", &native));
    assert!(store.saved == before);
    assert_eq!(store.revision, revision);
}
#[test]
fn missing_initialization_native_legacy_and_invalidated_absence_never_become_unknown() {
    assert!(capture(&Store::default(), &HashMap::new(), &["UABSENT12"]).is_err());
    let (mut store, mut native) = seeded();
    native.insert(
        "ULEGACY12".into(),
        serde_json::from_value(row("ULEGACY12", "Legacy")).unwrap(),
    );
    assert!(capture(&store, &native, &["ULEGACY12"]).is_err());
    store
        .saved
        .as_mut()
        .unwrap()
        .invalidated
        .insert("UABSENT12".into());
    assert!(capture(&store, &native, &["UABSENT12"]).is_err());
}
#[test]
fn known_native_drift_and_pending_unknown_refuse() {
    let (mut store, mut native) = seeded();
    native.get_mut("UKNOWN123").unwrap().name = Some("drift".into());
    assert!(capture(&store, &native, &["UKNOWN123"]).is_err());
    store
        .enqueue(
            "A",
            &owner(),
            Origin::Directory,
            vec![row("UABSENT12", "Pending")],
            json!({"UABSENT12":true}),
        )
        .unwrap();
    assert!(capture(&store, &native, &["UABSENT12"]).is_err());
}
#[test]
fn original_owner_revision_and_new_native_presence_retire_receipt() {
    let (mut store, mut native) = seeded();
    let receipt = capture(&store, &native, &["UKNOWN123", "UABSENT12"]).unwrap();
    assert!(!receipt.current(&store, "B", &owner(), 7, "Q", &native));
    assert!(!receipt.current(&store, "A", &owner(), 8, "Q", &native));
    let mut changed = owner();
    changed.token = "rotated".into();
    assert!(!receipt.current(&store, "A", &changed, 7, "Q", &native));
    native.insert(
        "UABSENT12".into(),
        serde_json::from_value(row("UABSENT12", "Now native")).unwrap(),
    );
    assert!(!receipt.current(&store, "A", &owner(), 7, "Q", &native));
    native.remove("UABSENT12");
    store
        .enqueue(
            "A",
            &owner(),
            Origin::Directory,
            vec![row("UOTHER123", "Other")],
            json!({"UOTHER123":true}),
        )
        .unwrap();
    assert!(!receipt.current(&store, "A", &owner(), 7, "Q", &native));
}
