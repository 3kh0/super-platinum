use super::super::tests::{prepared_with, publish_origin, receipt};
use super::*;
use serde_json::json;
use super_platinum_core::state::posting_metadata::authority::BootRead;
const ID: &str = "DOPEN001";

fn prepared() -> (ShellState, Owner) {
    let (mut s, owner) = prepared_with(|s| {
        let team = s.core.active_team.clone().unwrap();
        s.core
            .session
            .as_mut()
            .unwrap()
            .unified_user_client_enabled
            .insert(team.clone(), false);
        let session = s.core.session.as_ref().unwrap().workspaces[&team].clone();
        let transport = s.core.transport.clone().unwrap();
        let boot = BootRead::begin(&mut s.core, &transport, &session).unwrap();
        boot.apply(&mut s.core,json!({"ok":true,"team":{"id":team,"plan":"plus"},"self":{"id":session.user_id,"team_id":team,"profile":{}},"read_only_channels":[],"thread_only_channels":[]}),false).unwrap();
    });
    let c = &owner.context;
    let flags = s.app_home.channel_fetches.slots[&c.key()]
        .transformations
        .flags
        .clone();
    let w = s.core.workspaces.get_mut(&c.session.team_id).unwrap();
    while w
        .directory_members
        .has_pending(&c.account, &c.session)
        .unwrap()
    {
        let pending = w
            .directory_members
            .prepare(
                &c.account,
                &c.session,
                c.epoch,
                GENERATION,
                flags.clone(),
                &w.users,
            )
            .unwrap();
        assert!(pending.fields().iter().all(|f| f.is_ascii()));
        let reply=super_platinum_core::block_actions::workflow_management::member_hydration::RendererReply {generation:GENERATION.into(),operation:"ordinary-toLowerCase".into(),fields:pending.fields().to_vec(),lowered:pending.fields().iter().map(|v|v.to_ascii_lowercase()).collect()};
        let (_, native) = w
            .directory_members
            .adopt(
                pending, reply, &flags, &c.account, &c.session, c.epoch, GENERATION, &w.users,
            )
            .unwrap()
            .into_parts();
        w.users = native;
    }
    (s, owner)
}
fn queued(s: &mut ShellState, owner: &Owner) -> (Completion, Value) {
    let raw = json!({"id":ID,"is_im":true,"user":owner.context.session.user_id,"is_open":true,"latest":{"ts":"42.5"},"last_read":"41.1","is_read_only":false,"is_thread_only":false,"opaque":{"retained":true}});
    let companions = Companions::capture(&raw).unwrap();
    let members = Members::capture(s, owner).unwrap();
    let authority = Authority::capture(&s.core, &owner.context.session.team_id).unwrap();
    let mut r = receipt(s, owner);
    r.origin = Origin::OpenedConversation;
    r.accept(s, std::slice::from_ref(&raw), &|_| true).unwrap();
    let completion = Arc::new(Mutex::new(Status::Pending));
    let observed = observe(s, &owner.context).unwrap();
    s.app_home.channel_fetches.opened.push(Pending {
        owner: owner.clone(),
        observed,
        lifecycle: r.lifecycle,
        transformations: r.transformations,
        members,
        authority,
        raw: raw.clone(),
        companions,
        completion: completion.clone(),
    });
    (completion, raw)
}
#[test]
fn original_open_queue_commits_canonical_cursors_and_flags_without_http_witness() {
    let (mut s, owner) = prepared();
    let team = owner.context.session.team_id.clone();
    let (done, raw) = queued(&mut s, &owner);
    assert!(!s.core.workspaces[&team].channels.contains_key(ID));
    assert!(!s.core.workspaces[&team].messages.contains_key(ID));
    publish_origin(&mut s, &owner, Origin::OpenedConversation);
    assert!(matches!(*done.lock().unwrap(), Status::Applied));
    let catalog = canonical::capture(&mut s, &team).unwrap();
    let row = catalog.row(ID).unwrap();
    assert_eq!(row["opaque"], raw["opaque"]);
    assert_eq!(row["fromAnotherTeam"], false);
    assert!(row.get("latest").is_none() && row.get("last_read").is_none());
    let bag = &s.core.workspaces[&team].messages[ID];
    assert_eq!(bag.latest.as_deref(), Some("42.5"));
    assert_eq!(bag.last_read.as_deref(), Some("41.1"));
    let a = Authority::capture(&s.core, &team).unwrap();
    assert!(a.raw_channel(&s.core, ID).is_err());
    assert!(
        a.metadata(&s.core)
            .unwrap()
            .effective(a.configuration(), &raw)
            .is_ok()
    );
}
#[test]
fn newer_source_queue_retires_older_open_companions() {
    let (mut s, owner) = prepared();
    let (done, _) = queued(&mut s, &owner);
    let mut newer = receipt(&mut s, &owner);
    newer.origin = Origin::OpenedConversation;
    newer
        .accept(&mut s, &[json!({"id":ID,"is_open":false})], &|_| true)
        .unwrap();
    publish_origin(&mut s, &owner, Origin::OpenedConversation);
    assert!(matches!(*done.lock().unwrap(), Status::Retired));
    assert!(
        !s.core.workspaces[&owner.context.session.team_id]
            .messages
            .contains_key(ID)
    );
}
#[test]
fn account_socket_and_member_changes_refuse_companion_publication() {
    for mode in 0..3 {
        let (mut s, owner) = prepared();
        let (done, _) = queued(&mut s, &owner);
        match mode {
            0 => s.core.account_epoch += 1,
            1 => {
                s.core
                    .workspaces
                    .get_mut(&owner.context.session.team_id)
                    .unwrap()
                    .rt_generation += 1
            }
            _ => {
                s.core
                    .workspaces
                    .get_mut(&owner.context.session.team_id)
                    .unwrap()
                    .users
                    .clear();
            }
        }
        assert!(prepare(&mut s, &owner).is_empty());
        assert!(matches!(*done.lock().unwrap(), Status::Retired));
    }
}
#[test]
fn original_member_observation_rejects_changed_native_projection() {
    let (mut s, owner) = prepared();
    let m = Members::capture(&s, &owner).unwrap();
    assert!(m.current(&s, &owner));
    s.core
        .workspaces
        .get_mut(&owner.context.session.team_id)
        .unwrap()
        .users
        .clear();
    assert!(!m.current(&s, &owner));
}
