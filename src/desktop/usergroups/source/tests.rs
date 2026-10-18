use super::*;
use crate::media::MediaRegistry;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use super_platinum_core::{
    config::{Session, WorkspaceSession},
    slack::{Transport, models::UserGroup},
    state::Workspace,
};
const TEAM: &str = "TGROUP001";
const GROUP: &str = "SGROUP001";
fn shell() -> ShellState {
    let mut shell = ShellState::login(MediaRegistry::default());
    let session = WorkspaceSession {
        team_id: TEAM.into(),
        user_id: "USELF0001".into(),
        enterprise_id: None,
        name: "Closed fixture".into(),
        url: "https://example.invalid".into(),
        token: "synthetic-token".into(),
    };
    shell.core.active_account = Some("source-fixture".into());
    shell.core.active_team = Some(TEAM.into());
    shell.core.session = Some(Session {
        d_cookie: "synthetic-cookie".into(),
        unified_user_client_enabled: BTreeMap::new(),
        workspaces: BTreeMap::from([(TEAM.into(), session.clone())]),
    });
    shell.core.transport = Some(Arc::new(Transport::new("synthetic-cookie").unwrap()));
    let mut w = Workspace::from_session(&session);
    let lease = w.directory_usergroups.begin_boot(&w.usergroups).unwrap();
    let commit = w
        .directory_usergroups
        .observe_boot(
            lease,
            "source-fixture",
            &session,
            shell.core.account_epoch,
            &w.usergroups,
            &json!({"ok":true,"team":{"id":TEAM},"self":{"id":"USELF0001"},"subteams":{"self":[]}}),
        )
        .unwrap();
    (w.directory_usergroups, w.usergroups) = commit.into_parts();
    shell.core.workspaces.insert(TEAM.into(), w);
    shell
}
fn metadata(rows: Vec<Value>, native_members: NativeMembers) -> Operation {
    Operation::Metadata {
        origin: Origin::Upsert,
        rows,
        native_members,
    }
}
fn work(shell: &mut ShellState, operation: Operation) -> Work {
    let owner = Arc::new(Context::capture(shell, TEAM).unwrap());
    match enqueue(shell, owner, operation) {
        Admission::Tracked(work) => work,
        Admission::Unavailable => panic!("actual boot source must admit operation"),
    }
}
fn reply(pending: &source_core::Pending) -> source_core::RendererReply {
    // Pure fixture names are explicitly ASCII. Production uses the real locale
    // renderer, whose receipt identity and no-trim behavior have separate tests.
    assert!(pending.fields().iter().all(|s| s.is_ascii()));
    source_core::RendererReply {
        generation: pending.generation().into(),
        fields: pending.fields().to_vec(),
        lowered: pending
            .fields()
            .iter()
            .map(|s| s.to_ascii_lowercase())
            .collect(),
        locale: (!pending.fields().is_empty()).then(|| "en-US".into()),
    }
}
fn complete(shell: &mut ShellState, work: &mut Work) {
    let pending = work.next(shell, &|_| true).unwrap().unwrap();
    let reply = reply(&pending);
    work.finish(shell, pending, reply, &|_| true).unwrap();
}
fn snapshot(shell: &ShellState) -> source_core::Snapshot {
    let ctx = Context::capture(shell, TEAM).unwrap();
    let w = &shell.core.workspaces[TEAM];
    w.directory_usergroups
        .snapshot(&ctx.account, &ctx.session, ctx.epoch, &w.usergroups)
        .unwrap()
}
#[test]
fn fifo_followers_do_not_retire_head_and_source_native_publish_together() {
    let mut s = shell();
    let mut first = work(
        &mut s,
        metadata(
            vec![json!({"id":GROUP,"name":" Mixed Name ","handle":"first","opaque":{"keep":7}})],
            NativeMembers::Preserve,
        ),
    );
    let pending = first.next(&s, &|_| true).unwrap().unwrap();
    let mut second = work(
        &mut s,
        metadata(
            vec![json!({"id":GROUP,"handle":"second"})],
            NativeMembers::Preserve,
        ),
    );
    assert!(first.pending_current(&s, &pending, &|_| true));
    assert!(second.next(&s, &|_| true).unwrap().is_none());
    assert!(s.core.workspaces[TEAM].usergroups.is_empty());
    let reply = reply(&pending);
    first.finish(&mut s, pending, reply, &|_| true).unwrap();
    assert_eq!(s.core.workspaces[TEAM].usergroups[GROUP].handle, "first");
    complete(&mut s, &mut second);
    let observed = snapshot(&s);
    assert_eq!(observed.rows().len(), 1);
    assert_eq!(observed.rows()[0].effective["name"], " Mixed Name ");
    assert_eq!(observed.rows()[0].effective["handle"], "second");
    assert_eq!(observed.rows()[0].effective["opaque"], json!({"keep":7}));
    assert!(observed.rows()[0].effective.get("_name_lc").is_none());
    assert_eq!(s.core.workspaces[TEAM].usergroups[GROUP].handle, "second");
}
#[test]
fn original_account_auth_transport_and_socket_are_not_recaptured_after_wait() {
    for kind in 0..6 {
        let mut s = shell();
        let mut work = work(
            &mut s,
            metadata(
                vec![json!({"id":GROUP,"name":"Name","handle":"original"})],
                NativeMembers::Preserve,
            ),
        );
        let pending = work.next(&s, &|_| true).unwrap().unwrap();
        match kind {
            0 => s.core.active_account = Some("foreign".into()),
            1 => s.core.account_epoch += 1,
            2 => s.core.session.as_mut().unwrap().d_cookie = "changed-cookie".into(),
            3 => {
                s.core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut(TEAM)
                    .unwrap()
                    .token = "changed-token".into()
            }
            4 => s.core.transport = Some(Arc::new(Transport::new("synthetic-cookie").unwrap())),
            _ => s.core.workspaces.get_mut(TEAM).unwrap().rt_generation += 1,
        }
        let reply = reply(&pending);
        assert!(work.finish(&mut s, pending, reply, &|_| true).is_err());
        assert!(s.core.workspaces[TEAM].usergroups.is_empty());
    }
}
#[test]
fn native_drift_and_original_consumer_retirement_refuse_publication() {
    for native_drift in [true, false] {
        let mut s = shell();
        let mut work = work(
            &mut s,
            metadata(
                vec![json!({"id":GROUP,"name":"Queued","handle":"queued"})],
                NativeMembers::Preserve,
            ),
        );
        let pending = work.next(&s, &|_| true).unwrap().unwrap();
        if native_drift {
            s.core.workspaces.get_mut(TEAM).unwrap().usergroups.insert(
                GROUP.into(),
                UserGroup::from_observed(&json!({"id":GROUP,"handle":"newer"})).unwrap(),
            );
        }
        let reply = reply(&pending);
        assert!(
            work.finish(&mut s, pending, reply, &|_| native_drift)
                .is_err()
        );
        let actual = s.core.workspaces[TEAM]
            .usergroups
            .get(GROUP)
            .map(|r| r.handle.as_str());
        assert_eq!(actual, native_drift.then_some("newer"));
    }
}
#[test]
fn dropping_accepted_predecessor_does_not_authorize_follower_from_incomplete_history() {
    let mut s = shell();
    let first = work(
        &mut s,
        metadata(
            vec![json!({"id":GROUP,"name":"First","handle":"first"})],
            NativeMembers::Preserve,
        ),
    );
    let follower = work(
        &mut s,
        metadata(
            vec![json!({"id":GROUP,"handle":"second"})],
            NativeMembers::Preserve,
        ),
    );
    drop(first);
    assert!(follower.next(&s, &|_| true).is_err());
    assert!(s.core.workspaces[TEAM].usergroups.is_empty());
    assert!(
        s.core.workspaces[TEAM]
            .directory_usergroups
            .save()
            .is_null()
    );
}
#[test]
fn native_membership_policy_is_separate_from_effective_metadata_users() {
    let mut s = shell();
    let mut initial = work(
        &mut s,
        metadata(
            vec![json!({"id":GROUP,"name":"First","handle":"first"})],
            NativeMembers::Replace(vec!["UMEMBER01".into()]),
        ),
    );
    complete(&mut s, &mut initial);
    let mut next = work(
        &mut s,
        metadata(
            vec![json!({"id":GROUP,"name":"Second","users":["UMEMBER02"]})],
            NativeMembers::Preserve,
        ),
    );
    complete(&mut s, &mut next);
    let w = &s.core.workspaces[TEAM];
    assert_eq!(w.usergroups[GROUP].users, vec!["UMEMBER01"]);
    assert!(w.usergroups[GROUP].members_loaded);
    assert_eq!(
        snapshot(&s).rows()[0].effective["users"],
        json!(["UMEMBER02"])
    );
}
#[test]
fn unavailable_history_preserves_raw_label_fallback_without_minting_order() {
    let mut s = shell();
    let w = s.core.workspaces.get_mut(TEAM).unwrap();
    w.directory_usergroups = source_core::Store::default();
    w.usergroups.insert(
        GROUP.into(),
        UserGroup::from_observed(&json!({"id":GROUP,"handle":"accepted-raw"})).unwrap(),
    );
    let owner = Arc::new(Context::capture(&s, TEAM).unwrap());
    assert!(matches!(
        enqueue(
            &mut s,
            owner,
            metadata(
                vec![json!({"id":GROUP,"handle":"later"})],
                NativeMembers::Preserve
            )
        ),
        Admission::Unavailable
    ));
    assert_eq!(
        s.core.workspaces[TEAM].usergroups[GROUP].handle,
        "accepted-raw"
    );
    assert!(s.core.workspaces[TEAM].directory_usergroups.is_legacy());
}
