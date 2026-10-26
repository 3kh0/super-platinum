use super::super::{Kind, QuipTitle, Status, dependencies_with_groups, label_with_group_resolver};
use super::*;
use crate::{
    config::WorkspaceSession,
    state::directory_usergroups::{
        Native, NativeMembers, Operation, Origin, RendererReply, Store,
        unknown::{Lookup, Manager, Scope},
    },
};
use serde_json::json;
const ID: &str = "SGROUP001";
fn owner() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "TGROUP001".into(),
        user_id: "U_SELF".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.test".into(),
        token: "synthetic".into(),
    }
}
fn ready() -> (Store, Native, Scope) {
    let store = Store::fresh();
    let native = Native::new();
    let (store, native) = store
        .observe_boot(
            store.begin_boot(&native).unwrap(),
            "account",
            &owner(),
            1,
            &native,
            &json!({"ok":true,"team":{"id":"TGROUP001"},"self":{"id":"U_SELF"}}),
        )
        .unwrap()
        .into_parts();
    (
        store,
        native,
        Scope::new("account", &owner(), 1, 7).unwrap(),
    )
}
fn apply(store: &mut Store, native: &mut Native, rows: Vec<Value>) {
    let ticket = store
        .enqueue(
            "account",
            &owner(),
            1,
            "fixture",
            native,
            Operation::Metadata {
                origin: Origin::Upsert,
                rows,
                native_members: NativeMembers::Preserve,
            },
        )
        .unwrap();
    let pending = store
        .next_for(&ticket, "account", &owner(), 1, native)
        .unwrap()
        .unwrap();
    let reply = RendererReply {
        generation: pending.generation().into(),
        fields: pending.fields().to_vec(),
        lowered: pending.fields().iter().map(|s| s.to_lowercase()).collect(),
        locale: (!pending.fields().is_empty()).then(|| "en-US".into()),
    };
    (*store, *native) = store
        .commit(pending, reply, "account", &owner(), 1, native)
        .unwrap()
        .into_parts();
}
fn file(user: bool) -> Value {
    let mut leaves = vec![
        json!({"type":"text","text":"Notes for "}),
        json!({"type":"usergroup","usergroup_id":ID,"style":{"bold":true},"opaque":{"keep":1}}),
    ];
    if user {
        leaves.push(json!({"type":"text","text":" / "}));
        leaves.push(json!({"type":"user","user_id":"U00000001"}));
    }
    json!({"title":"Wrong fallback","title_blocks":[{"type":"rich_text","block_id":"kept-block","elements":[{"type":"rich_text_section","elements":leaves}]}]})
}
fn label(source: &Value, lookup: &Lookup, quip: QuipTitle<'_>) -> Result<String, Unavailable> {
    label_with_group_resolver(
        Kind::Canvas,
        Status::Normal,
        source,
        quip,
        Some("Untitled"),
        None,
        |_| Ok("@Dana".into()),
        |_| Ok("#channel".into()),
        |id| group_label_from_lookup(id, lookup, Some(GroupTitle::english())),
    )
}
#[test]
fn missing_lookup_requires_explicit_presentation_and_preserves_quip_raw_and_users() {
    let (s, n, scope) = ready();
    let mut manager = Manager::new(scope.clone());
    let lookup = manager.lookup(&scope, &s, &n, ID).unwrap();
    assert!(group_label_from_lookup(ID, &lookup, None).is_err());
    assert!(group_label_from_lookup("SGROUP002", &lookup, Some(GroupTitle::english())).is_err());
    assert_eq!(GroupTitle::english().unknown_label(), "@unknown-group");
    let source = file(false);
    let before = source.clone();
    let dependencies = dependencies_with_groups(&source, None).unwrap();
    assert!(dependencies.users.is_empty());
    assert_eq!(
        label(&source, &lookup, QuipTitle::Nullish).unwrap(),
        "Notes for @unknown-group"
    );
    assert_eq!(label(&source, &lookup, QuipTitle::Text("")).unwrap(), "");
    assert_eq!(
        label(&source, &lookup, QuipTitle::Text("Independent title")).unwrap(),
        "Independent title"
    );
    let users = file(true);
    assert_eq!(
        label(&users, &lookup, QuipTitle::Text("Ignored")).unwrap(),
        "Notes for @unknown-group / @Dana"
    );
    assert_eq!(source, before);
}
#[test]
fn canonical_effective_terminal_and_partial_known_ignore_raw_handle_omission() {
    let (mut s, mut n, scope) = ready();
    apply(
        &mut s,
        &mut n,
        vec![json!({"id":ID,"name":"Original","handle":"retained"})],
    );
    apply(&mut s, &mut n, vec![json!({"id":ID,"name":"Renamed"})]);
    assert!(known_group_label(ID, &n[ID]).is_err());
    let lookup = Lookup::observed(&scope, &s, &n, ID).unwrap();
    assert_eq!(
        group_label_from_lookup(ID, &lookup, None).unwrap(),
        "@retained"
    );
    apply(&mut s, &mut n, vec![json!({"id":ID,"isNonExistent":true})]);
    let raw = n[ID].label_source.clone();
    let lookup = Lookup::observed(&scope, &s, &n, ID).unwrap();
    assert!(known_group_label(ID, &n[ID]).is_err());
    assert!(group_label_from_lookup(ID, &lookup, None).is_err());
    assert_eq!(
        group_label_from_lookup(ID, &lookup, Some(GroupTitle::english())).unwrap(),
        "@unknown-group"
    );
    assert_eq!(lookup.effective()["handle"], "retained");
    assert_eq!(n[ID].label_source, raw);
    assert_eq!(raw, Some(json!({"id":ID,"isNonExistent":true})));
}
#[test]
fn existing_owned_marker_is_not_forged_native_and_plain_compatibility_survives() {
    let (mut s, mut n, scope) = ready();
    let forged = UserGroup::from_observed(&json!({"id":ID,"isNonExistent":true})).unwrap();
    n.insert(ID.into(), forged);
    assert!(Lookup::observed(&scope, &s, &n, ID).is_err());
    n.clear();
    for handle in [json!(""), Value::Null, json!("@@crew"), json!(" spaced ")] {
        // A known empty-only update is skipped by JJAw; include its name so the
        // actual source row is admitted without changing that settled rule.
        apply(
            &mut s,
            &mut n,
            vec![
                json!({"id":ID,"name":"Known","handle":handle,"is_visible":false,"date_delete":3}),
            ],
        );
        let lookup = Lookup::observed(&scope, &s, &n, ID).unwrap();
        assert_eq!(
            group_label_from_lookup(ID, &lookup, None),
            known_group_label(ID, &n[ID])
        );
    }
}
