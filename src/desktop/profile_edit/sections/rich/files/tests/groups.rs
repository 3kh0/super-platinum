use super::*;
use super_platinum_core::state::directory_usergroups::{
    NativeMembers, Operation, Origin, RendererReply, Store,
};
const ID: &str = "SGROUP001";
fn group(handle: &str, version: u64) -> Value {
    json!({"id":ID,"handle":handle,"name":"Fixture group","date_update":version})
}
fn boot(shell: &mut ShellState, token: &Token) {
    let owner =
        crate::directory_member_fetches::Context::capture(shell, &token.owner.team).unwrap();
    self_source(shell, token, false);
    let w = shell.core.workspaces.get_mut(&token.owner.team).unwrap();
    w.usergroups.clear();
    w.directory_usergroups = Store::fresh();
    let lease = w.directory_usergroups.begin_boot(&w.usergroups).unwrap();
    let commit=w.directory_usergroups.observe_boot(lease,&owner.account,&owner.session,owner.epoch,&w.usergroups,
        &json!({"ok":true,"team":{"id":owner.session.team_id},"self":{"id":owner.session.user_id},"subteams":{"self":[]}})).unwrap();
    (w.directory_usergroups, w.usergroups) = commit.into_parts();
}
fn publish(shell: &mut ShellState, token: &Token, raw: Value) {
    let owner =
        crate::directory_member_fetches::Context::capture(shell, &token.owner.team).unwrap();
    let w = shell.core.workspaces.get_mut(&token.owner.team).unwrap();
    let ticket = w
        .directory_usergroups
        .enqueue(
            &owner.account,
            &owner.session,
            owner.epoch,
            "group-title-test",
            &w.usergroups,
            Operation::Metadata {
                origin: Origin::Fetch,
                rows: vec![raw],
                native_members: NativeMembers::Preserve,
            },
        )
        .unwrap();
    let pending = w
        .directory_usergroups
        .next_for(
            &ticket,
            &owner.account,
            &owner.session,
            owner.epoch,
            &w.usergroups,
        )
        .unwrap()
        .unwrap();
    assert!(pending.fields().iter().all(|s| s.is_ascii()));
    let reply = RendererReply {
        generation: pending.generation().into(),
        fields: pending.fields().to_vec(),
        lowered: pending
            .fields()
            .iter()
            .map(|s| s.to_ascii_lowercase())
            .collect(),
        locale: (!pending.fields().is_empty()).then(|| "en-US".into()),
    };
    let commit = w
        .directory_usergroups
        .commit(
            pending,
            reply,
            &owner.account,
            &owner.session,
            owner.epoch,
            &w.usergroups,
        )
        .unwrap();
    (w.directory_usergroups, w.usergroups) = commit.into_parts();
}
fn file() -> Value {
    let mut raw = response("FFILE0001", "Wrong fallback", "quip");
    raw["file"]["title_blocks"] = json!([{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
        {"type":"text","text":"Notes for "},{"type":"usergroup","usergroup_id":ID,"style":{"bold":true},"opaque":{"keep":2}}]}]}]);
    raw
}
#[test]
fn nested_group_requires_shared_history_and_separate_quip_custody() {
    let (mut shell, token) = setup();
    let raw = file();
    put(&mut shell, &token, &raw);
    let deps = dependencies(&shell, &token, &["FFILE0001".into()]).unwrap();
    assert!(deps.ids.is_empty());
    assert!(deps.channels.is_empty());
    assert_eq!(deps.groups, std::collections::BTreeSet::from([ID.into()]));
    assert!(reuse_metadata(&shell, &token, "FFILE0001", true));
    shell
        .core
        .workspaces
        .get_mut(&token.owner.team)
        .unwrap()
        .usergroups
        .insert(
            ID.into(),
            super_platinum_core::slack::models::UserGroup::from_observed(&group("design", 1))
                .unwrap(),
        );
    assert!(
        Observation::capture(&shell, &token, "FFILE0001").is_err(),
        "typed raw label alone grants no history custody"
    );
    boot(&mut shell, &token);
    publish(&mut shell, &token, group("design", 1));
    let observed = Observation::capture(&shell, &token, "FFILE0001").unwrap();
    let wire = json!({"type":"canvas","file_id":"FFILE0001","text":"https://fixture.invalid/doc","opaque":42});
    assert_eq!(observed.label(&wire).unwrap(), "Notes for @design");
    assert!(observed.current(&shell));
    assert!(
        shell
            .profile_edit
            .sections
            .file_titles
            .as_ref()
            .unwrap()
            .policy
            .is_none()
    );
    shell.canvas_titles = Default::default();
    assert!(!observed.current(&shell));
    assert!(
        Observation::capture(&shell, &token, "FFILE0001").is_err(),
        "group is not user-only Quip bypass"
    );
    assert_eq!(
        observed.row.source()["title_blocks"],
        raw["file"]["title_blocks"]
    );
}
#[test]
fn nested_group_rename_retires_opening_but_never_rewrites_mounted_label() {
    let (mut shell, token) = setup();
    boot(&mut shell, &token);
    publish(&mut shell, &token, group("design", 1));
    put(&mut shell, &token, &file());
    let observed = Observation::capture(&shell, &token, "FFILE0001").unwrap();
    let wire = json!({"type":"canvas","file_id":"FFILE0001","text":"https://fixture.invalid/doc","opaque":42});
    let mut catalog = catalog::Catalog::default();
    catalog
        .insert(&wire, observed.label(&wire).unwrap())
        .unwrap();
    let witness = catalog::Witness {
        label_units: None,
        presentation: None,
        value: wire.clone(),
        label: "Notes for @design".into(),
    };
    publish(&mut shell, &token, group("renamed", 2));
    assert!(!observed.current(&shell));
    assert_eq!(observed.label(&wire).unwrap(), "Notes for @design");
    assert!(catalog.context(&[witness], &token.owner).is_ok());
    let current = Observation::capture(&shell, &token, "FFILE0001").unwrap();
    assert_eq!(current.label(&wire).unwrap(), "Notes for @renamed");
    shell
        .core
        .workspaces
        .get_mut(&token.owner.team)
        .unwrap()
        .usergroups
        .get_mut(ID)
        .unwrap()
        .handle = "forged".into();
    assert!(!current.current(&shell));
    assert!(Observation::capture(&shell, &token, "FFILE0001").is_err());
}

fn self_source(shell: &mut ShellState, token: &Token, restricted: bool) {
    use super_platinum_core::{
        block_actions::workflow_management::member_hydration::{Flags, RendererReply},
        state::directory_members::{Origin, Store},
    };
    let o = crate::directory_member_fetches::Context::capture(shell, &token.owner.team).unwrap();
    let w = shell.core.workspaces.get_mut(&token.owner.team).unwrap();
    if w.directory_members
        .subset(
            &o.account,
            &o.session,
            o.epoch,
            "group-self-test",
            &std::collections::BTreeSet::from([o.session.user_id.clone()]),
            &w.users,
        )
        .is_err()
    {
        w.directory_members = Store::default();
        w.directory_members
            .begin(&o.account, &o.session, &Default::default())
            .unwrap();
    }
    w.directory_members
        .enqueue(
            &o.account,
            &o.session,
            Origin::Directory,
            vec![json!({"id":o.session.user_id,"profile":{},"is_restricted":restricted})],
            json!({}),
        )
        .unwrap();
    let flags = Flags::decode(&json!({"ok":true,"assignments":{}})).unwrap();
    let p = w
        .directory_members
        .prepare(
            &o.account,
            &o.session,
            o.epoch,
            "group-self-test",
            flags.clone(),
            &w.users,
        )
        .unwrap();
    assert!(p.fields().iter().all(|v| v.is_ascii()));
    let r = RendererReply {
        generation: p.generation().into(),
        operation: "ordinary-toLowerCase".into(),
        fields: p.fields().to_vec(),
        lowered: p.fields().iter().map(|v| v.to_ascii_lowercase()).collect(),
    };
    w.users = w
        .directory_members
        .adopt(
            p,
            r,
            &flags,
            &o.account,
            &o.session,
            o.epoch,
            "group-self-test",
            &w.users,
        )
        .unwrap()
        .into_parts()
        .1;
}
#[test]
fn nested_missing_placeholder_needs_registered_source_and_keeps_mounted_label() {
    let (mut shell, token) = setup();
    boot(&mut shell, &token);
    put(&mut shell, &token, &file());
    assert!(Observation::capture(&shell, &token, "FFILE0001").is_err());
    let deps = dependencies(&shell, &token, &["FFILE0001".into()]).unwrap();
    assert_eq!(group_hits(&mut shell, &token, &deps).len(), 1);
    let old = Observation::capture(&shell, &token, "FFILE0001").unwrap();
    let wire = json!({"type":"canvas","file_id":"FFILE0001"});
    assert_eq!(old.label(&wire).unwrap(), "Notes for @unknown-group");
    publish(&mut shell, &token, group("found", 1));
    assert!(!old.current(&shell));
    assert_eq!(old.label(&wire).unwrap(), "Notes for @unknown-group");
    assert_eq!(
        Observation::capture(&shell, &token, "FFILE0001")
            .unwrap()
            .label(&wire)
            .unwrap(),
        "Notes for @found"
    );
}
#[test]
fn restricted_self_overrides_absent_and_existing_and_role_change_retires_opening() {
    let (mut shell, token) = setup();
    boot(&mut shell, &token);
    self_source(&mut shell, &token, true);
    put(&mut shell, &token, &file());
    let deps = dependencies(&shell, &token, &["FFILE0001".into()]).unwrap();
    assert!(group_hits(&mut shell, &token, &deps).is_empty());
    let wire = json!({"type":"canvas","file_id":"FFILE0001"});
    let absent = Observation::capture(&shell, &token, "FFILE0001").unwrap();
    assert_eq!(absent.label(&wire).unwrap(), "Notes for @unknown-group");
    publish(&mut shell, &token, group("secret-name", 1));
    let known = Observation::capture(&shell, &token, "FFILE0001").unwrap();
    assert_eq!(known.label(&wire).unwrap(), "Notes for @unknown-group");
    assert!(group_hits(&mut shell, &token, &deps).is_empty());
    self_source(&mut shell, &token, false);
    assert!(!known.current(&shell));
    assert!(!absent.current(&shell));
    assert_eq!(known.label(&wire).unwrap(), "Notes for @unknown-group");
    assert_eq!(
        Observation::capture(&shell, &token, "FFILE0001")
            .unwrap()
            .label(&wire)
            .unwrap(),
        "Notes for @secret-name"
    );
}
